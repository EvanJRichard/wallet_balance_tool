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
        
        if decoded.len() < 78 {  // Extended keys should be 78 bytes
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
        println!("Converted vpub to tpub: {}", tpub); // Debug output
        
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
    
    // For testnet, we use m/0/0/i derivation path (account/change/index)
    let base_path = if network == Network::Testnet {
        "m/0/0" // account 0, external chain
    } else {
        "m/0" // simpler path for mainnet
    };
    
    // Derive addresses
    for i in 0..20 { // this could be configurable in a future version
        let path = DerivationPath::from_str(&format!("{}/{}", base_path, i))
            .map_err(|e| format!("Invalid derivation path: {}", e))?;
        
        let derived_pubkey = extended_pubkey
            .derive_pub(&secp, &path)
            .map_err(|e| format!("Derivation error: {}", e))?;
        
        let public_key = PublicKey::new(derived_pubkey.public_key);
        
        let address = Address::p2pkh(
            &public_key,
            network,
        );

        // Use a different API endpoint for testnet
        let balance = if network == Network::Testnet {
            get_testnet_address_balance(&address.to_string()).await?
        } else {
            get_address_balance(&address.to_string()).await?
        };
        
        balances.push(AddressBalance {
            address: address.to_string(),
            balance: balance,
            derivation_path: format!("{}/{}", base_path, i), // Using the full path in display
        });
    }

    Ok(balances)
}

async fn get_testnet_address_balance(address: &str) -> Result<f64, String> {
    let url = format!("https://blockstream.info/testnet/api/address/{}", address);
    println!("Checking balance for address: {} at URL: {}", address, url);
    
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("API request failed: {}", e))?;
    
    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to get response text: {}", e))?;
    
    println!("Response from API: {}", text);
    
    let data: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Failed to parse API response: {}", e))?;
    
    // Print the full response structure
    println!("Parsed JSON data: {:#?}", data);
    
    let funded = data["chain_stats"]["funded_txo_sum"]
        .as_u64()
        .unwrap_or(0);
    let spent = data["chain_stats"]["spent_txo_sum"]
        .as_u64()
        .unwrap_or(0);
    
    println!("Funded amount: {}, Spent amount: {}", funded, spent);
    
    let balance_satoshis = funded.saturating_sub(spent);
    let balance_btc = balance_satoshis as f64 / 100_000_000.0;
    
    println!("Calculated balance: {} BTC", balance_btc);
    
    Ok(balance_btc)
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