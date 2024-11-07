use iced::{
    widget::{button, column, container, row, text, text_input},
    Application, Command, Element, Length, Settings, Theme,
    executor,
};
use bitcoin::{
    bip32::{ExtendedPubKey, DerivationPath},
    secp256k1::Secp256k1,
    Address, Network, PublicKey,
    base58,
    hashes::{sha256, Hash},
};
use std::{str::FromStr, future::Future};

// Custom executor for Tokio
#[derive(Debug)]
struct TokenExecutor {
    runtime: tokio::runtime::Runtime,
}

impl executor::Executor for TokenExecutor {
    fn new() -> Result<Self, std::io::Error> {
        Ok(Self {
            runtime: tokio::runtime::Runtime::new()?
        })
    }

    fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        self.runtime.spawn(future);
    }
}

#[derive(Debug, Clone)]
enum Message {
    XpubInputChanged(String),
    CheckBalance,
    BalanceResult(Result<Vec<AddressBalance>, String>),
}

#[derive(Debug, Clone)]
struct AddressBalance {
    address: String,
    balance: f64,
    derivation_path: String,
}

struct WalletBalanceApp {
    xpub_input: String,
    balances: Vec<AddressBalance>,
    error: Option<String>,
    loading: bool,
}

impl Application for WalletBalanceApp {
    type Message = Message;
    type Theme = Theme;
    type Executor = TokenExecutor;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (
            Self {
                xpub_input: String::new(),
                balances: Vec::new(),
                error: None,
                loading: false,
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Bitcoin Wallet Balance Discovery Tool")
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::XpubInputChanged(value) => {
                self.xpub_input = value;
                Command::none()
            }
            Message::CheckBalance => {
                self.loading = true;
                self.error = None;
                let xpub = self.xpub_input.clone();
                Command::perform(
                    async move { check_balances(&xpub).await },
                    Message::BalanceResult,
                )
            }
            Message::BalanceResult(result) => {
                self.loading = false;
                match result {
                    Ok(balances) => {
                        self.balances = balances;
                        self.error = None;
                    }
                    Err(e) => {
                        self.error = Some(e);
                    }
                }
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let input = text_input("Enter extended public key (xpub)", &self.xpub_input)
            .on_input(Message::XpubInputChanged)
            .padding(10)
            .size(20);

        let check_button = button("Check Balance")
            .on_press(Message::CheckBalance)
            .padding(10);

        let mut content = column![
            text("Bitcoin Wallet Balance Discovery Tool").size(28),
            input,
            check_button,
        ]
        .spacing(20)
        .padding(20);

        if self.loading {
            content = content.push(text("Loading...").size(20));
        }

        if let Some(error) = &self.error {
            content = content.push(text(error).size(20).style(iced::Color::from_rgb(1.0, 0.0, 0.0)));
        }

        if !self.balances.is_empty() {
            let total: f64 = self.balances.iter().map(|b| b.balance).sum();
            
            let balances_list = self.balances.iter().fold(
                column![].spacing(10),
                |col, balance| {
                    col.push(
                        row![
                            text(&balance.derivation_path).width(Length::FillPortion(2)),
                            text(&balance.address).width(Length::FillPortion(5)),
                            text(format!("{} BTC", balance.balance)).width(Length::FillPortion(2)),
                        ]
                        .spacing(20)
                    )
                },
            );

            content = content
                .push(text("Balances:").size(24))
                .push(balances_list)
                .push(text(format!("Total Balance: {} BTC", total)).size(24));
        }

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into()
    }
}

async fn check_balances(xpub: &str) -> Result<Vec<AddressBalance>, String> {
    // Determine network and handle version bytes
    let (network, xpub_to_use) = if xpub.starts_with("vpub") {
        let decoded = base58::decode(xpub)
            .map_err(|e| format!("Failed to decode vpub: {}", e))?;
        
        if decoded.len() < 78 {
            return Err("Invalid extended public key length".to_string());
        }

        // Extract the key material (everything except version and checksum)
        let key_material = &decoded[4..decoded.len()-4];
        
        // Create new vector with tpub version bytes
        let mut modified = Vec::with_capacity(78);
        modified.extend_from_slice(&[0x04, 0x35, 0x87, 0xCF]); // tpub version
        modified.extend_from_slice(key_material);
        
        // Calculate double SHA256 checksum
        let hash1 = sha256::Hash::hash(&modified[..modified.len()]);
        let hash2 = sha256::Hash::hash(&hash1[..]);
        
        // Add checksum
        modified.extend_from_slice(&hash2[0..4]);
        
        let tpub = base58::encode(&modified);
        println!("Converted vpub to tpub: {}", tpub);
        
        (Network::Testnet, tpub)
    } else if xpub.starts_with("xpub") {
        (Network::Bitcoin, xpub.to_string())
    } else {
        return Err("Unsupported extended public key format. Must start with 'xpub' or 'vpub'".to_string());
    };

    println!("Using key: {}", xpub_to_use);

    let extended_pubkey = ExtendedPubKey::from_str(&xpub_to_use)
        .map_err(|e| format!("Invalid extended public key: {}", e))?;

    let secp = Secp256k1::new();
    let mut balances = Vec::new();
    
    // Check both external and change addresses
    for account in 0..2 {
        // Determine how many addresses to check based on account
        let max_index = if account == 0 { 46 } else { 12 };
        
        for index in 0..max_index {
            // Create child number sequence directly
            let child_numbers = [
                bitcoin::bip32::ChildNumber::from_normal_idx(account)
                    .map_err(|e| format!("Invalid account number: {}", e))?,
                bitcoin::bip32::ChildNumber::from_normal_idx(index)
                    .map_err(|e| format!("Invalid index: {}", e))?,
            ];
            
            let path = DerivationPath::from(child_numbers.as_ref());
            println!("Deriving path: m/{}/{}", account, index);
            
            let derived_pubkey = extended_pubkey
                .derive_pub(&secp, &path)
                .map_err(|e| format!("Derivation error: {}", e))?;
            
            let public_key = PublicKey::new(derived_pubkey.public_key);
            
            // Generate native SegWit address (P2WPKH)
            let address = Address::p2wpkh(&public_key, network)
                .map_err(|e| format!("Address generation error: {}", e))?;

            println!("Generated address for m/{}/{}: {}", account, index, address);
            
            let balance = if network == Network::Testnet {
                get_testnet_address_balance(&address.to_string()).await?
            } else {
                get_address_balance(&address.to_string()).await?
            };

            if balance > 0.0 {
                println!("Found balance of {} BTC at {}", balance, address);
            }
            
            balances.push(AddressBalance {
                address: address.to_string(),
                balance,
                derivation_path: format!("m/{}/{}", account, index),
            });
        }
    }

    Ok(balances)
}

// ... [Previous code remains same until balance checking functions] ...

async fn get_testnet_address_balance(address: &str) -> Result<f64, String> {
    // Try multiple API endpoints with retry logic
    for attempt in 0..3 {
        if attempt > 0 {
            // Exponential backoff: 2s, 4s, 8s
            let delay = 2u64.pow(attempt as u32);
            println!("Rate limit hit, waiting {} seconds before retry...", delay);
            tokio::time::sleep(tokio::time::Duration::from_secs(delay)).await;
        }

        // Alternate between different API endpoints
        let result = match attempt % 2 {
            0 => get_balance_from_blockstream(address).await,
            1 => get_balance_from_mempool(address).await,
            _ => unreachable!(),
        };

        match result {
            Ok(balance) => return Ok(balance),
            Err(e) if e.contains("rate limit") || e.contains("exceeded") => {
                println!("Rate limit error, will retry: {}", e);
                continue;
            },
            Err(e) => return Err(e),
        }
    }

    Err("All API attempts failed".to_string())
}

async fn get_balance_from_blockstream(address: &str) -> Result<f64, String> {
    let url = format!("https://blockstream.info/testnet/api/address/{}", address);
    println!("Checking balance via Blockstream for address: {}", address);
    
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("API request failed: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("API error: {}", response.status()));
    }
    
    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to get response text: {}", e))?;
    
    if text.contains("exceeded") {
        return Err("Rate limit exceeded".to_string());
    }
    
    let data: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Failed to parse API response: {}", e))?;
    
    let funded = data["chain_stats"]["funded_txo_sum"]
        .as_u64()
        .unwrap_or(0);
    let spent = data["chain_stats"]["spent_txo_sum"]
        .as_u64()
        .unwrap_or(0);
    
    let balance_satoshis = funded.saturating_sub(spent);
    Ok(balance_satoshis as f64 / 100_000_000.0)
}

async fn get_balance_from_mempool(address: &str) -> Result<f64, String> {
    let url = format!("https://mempool.space/testnet/api/address/{}", address);
    println!("Checking balance via Mempool.space for address: {}", address);
    
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("API request failed: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("API error: {}", response.status()));
    }

    let data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse API response: {}", e))?;
    
    // Mempool.space returns the balance directly
    let balance_satoshis = data["chain_stats"]["funded_txo_sum"]
        .as_u64()
        .unwrap_or(0)
        .saturating_sub(
            data["chain_stats"]["spent_txo_sum"]
                .as_u64()
                .unwrap_or(0)
        );
    
    Ok(balance_satoshis as f64 / 100_000_000.0)
}

async fn get_address_balance(address: &str) -> Result<f64, String> {
    let url = format!("https://blockchain.info/rawaddr/{}", address);
    
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("API request failed: {}", e))?;
    
    let data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse API response: {}", e))?;
    
    let balance_satoshis = data["final_balance"]
        .as_u64()
        .ok_or_else(|| "Invalid balance format".to_string())?;
    
    Ok(balance_satoshis as f64 / 100_000_000.0)
}

fn main() -> iced::Result {
    WalletBalanceApp::run(Settings::default())
}