use iced::{
    widget::{button, column, container, row, text, text_input, progress_bar, scrollable, Container},
    Application, Command, Element, Length, Settings, Theme,
    executor, Executor,
    alignment, theme,
};
use bitcoin::{
    bip32::{ExtendedPubKey, DerivationPath, ChildNumber},
    secp256k1::Secp256k1,
    Address, Network, PublicKey,
    base58,
    hashes::{sha256, Hash},
};
use std::{str::FromStr, future::Future};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tokio::runtime::Runtime;
use tokio::runtime::Handle;
use parking_lot::Mutex;

static LAST_REQUEST: AtomicU64 = AtomicU64::new(0);
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug)]
struct TokenExecutor {
    runtime: Arc<Runtime>,
}

impl Clone for TokenExecutor {
    fn clone(&self) -> Self {
        Self {
            runtime: Arc::clone(&self.runtime)
        }
    }
}

impl executor::Executor for TokenExecutor {
    fn new() -> Result<Self, std::io::Error> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("iced-runtime")
            .build()?;

        Ok(Self {
            runtime: Arc::new(runtime)
        })
    }

    fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        let runtime_handle = self.runtime.handle().clone();
        runtime_handle.spawn(future);
    }
}

#[derive(Debug, Clone)]
enum Message {
    XpubInputChanged(String),
    CheckBalance,
    LoadMore,
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
    current_page: usize,
    addresses_per_page: usize,
    has_more: bool,
    total_addresses_checked: usize,
    executor: TokenExecutor,
}

impl WalletBalanceApp {
    fn new() -> Result<Self, std::io::Error> {
        let executor = TokenExecutor::new()?;
        
        Ok(Self {
            xpub_input: String::new(),
            balances: Vec::new(),
            error: None,
            loading: false,
            current_page: 0,
            addresses_per_page: 10,
            has_more: true,
            total_addresses_checked: 0,
            executor,
        })
    }

    fn calculate_address_range(&self) -> (usize, usize) {
        let start = self.current_page * self.addresses_per_page;
        let end = start + self.addresses_per_page;
        (start, end)
    }

}

impl Application for WalletBalanceApp {
    type Message = Message;
    type Theme = Theme;
    type Executor = TokenExecutor;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (WalletBalanceApp::new().expect("Failed to create application"), Command::none())
    }

    fn title(&self) -> String {
        String::from("Bitcoin Wallet Balance Discovery Tool")
    }

 fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::XpubInputChanged(value) => {
                self.xpub_input = value;
                self.current_page = 0;
                self.balances.clear();
                self.has_more = true;
                self.total_addresses_checked = 0;
                Command::none()
            }
            Message::CheckBalance => {
                self.loading = true;
                self.error = None;
                self.current_page = 0;
                self.balances.clear();
                self.has_more = true;
                self.total_addresses_checked = 0;
                let xpub = self.xpub_input.clone();
                let range = self.calculate_address_range();
                Command::perform(
                    async move { check_balances(&xpub, range.0, range.1).await },
                    Message::BalanceResult,
                )
            }
            Message::LoadMore => {
                if !self.loading && self.has_more {
                    self.loading = true;
                    self.current_page += 1;
                    let xpub = self.xpub_input.clone();
                    let range = self.calculate_address_range();
                    Command::perform(
                        async move { check_balances(&xpub, range.0, range.1).await },
                        Message::BalanceResult,
                    )
                } else {
                    Command::none()
                }
            }
            Message::BalanceResult(result) => {
                self.loading = false;
                match result {
                    Ok(mut new_balances) => {
                        self.total_addresses_checked += new_balances.len();
                        self.balances.append(&mut new_balances);
                        self.has_more = true;  // Always allow loading more addresses
                    }
                    Err(e) => {
                        self.error = Some(format!("Error (showing partial results): {}", e));
                    }
                }
                Command::none()
            }
        }
    }

fn view(&self) -> Element<Message> {
    let title = text("Bitcoin Wallet Balance Discovery Tool")
        .size(24)
        .width(Length::Fill)
        .horizontal_alignment(alignment::Horizontal::Center);

    let input = text_input("Enter extended public key (xpub/vpub)", &self.xpub_input)
        .on_input(Message::XpubInputChanged)
        .padding(10)
        .size(16);

    let check_button = button("Check Balance")
        .on_press(Message::CheckBalance)
        .padding(10)
        .style(theme::Button::Primary);

    let mut content = column![
        title,
        input,
        check_button,
    ]
    .spacing(15)
    .padding(20)
    .width(Length::Fill)
    .align_items(alignment::Alignment::Center);

    if self.loading {
        content = content.push(
            column![
                text(format!(
                    "Loading addresses {}-{}...", 
                    self.total_addresses_checked,
                    self.total_addresses_checked + self.addresses_per_page
                ))
                .size(14),
                progress_bar(0.0..=100.0, 50.0)
                    .width(Length::Fixed(300.0))
            ]
            .spacing(10)
            .padding(10)
        );
    }

    if let Some(error) = &self.error {
        content = content.push(
            text(error)
                .size(14)
                .style(iced::Color::from_rgb(0.8, 0.0, 0.0))
                .width(Length::Fill)
                .horizontal_alignment(alignment::Horizontal::Center)
        );
    }

    if !self.balances.is_empty() {
        let total: f64 = self.balances.iter().map(|b| b.balance).sum();
        
        // Header row
        let header_row = row![
            text("Path").size(14).width(Length::FillPortion(2)),
            text("Address").size(14).width(Length::FillPortion(5)),
            text("Balance (BTC)").size(14).width(Length::FillPortion(2)),
        ]
        .spacing(10)
        .padding(5);

        // Scrollable balance list with reduced height
        let balances_list = self.balances.iter().fold(
            column![header_row].spacing(2),
            |col, balance| {
                col.push(
                    row![
                        text(&balance.derivation_path)
                            .size(12)
                            .width(Length::FillPortion(2)),
                        text(&balance.address)
                            .size(12)
                            .width(Length::FillPortion(5)),
                        text(format!("{:.8} BTC", balance.balance))
                            .size(12)
                            .width(Length::FillPortion(2)),
                    ]
                    .spacing(10)
                    .padding(5)
                )
            },
        );

        let scrollable_content = scrollable(balances_list)
            .height(Length::Fixed(250.0))  // Reduced from 300.0 to make room for Load More button
            .width(Length::Fill);

        let summary = column![
            text(format!("Addresses checked: {}", self.total_addresses_checked))
                .size(14),
            text(format!("Total Balance: {:.8} BTC", total))
                .size(16)
                .style(theme::Text::Color(iced::Color::from_rgb(0.0, 0.5, 0.0)))
        ]
        .spacing(5)  // Reduced spacing
        .padding(5); // Reduced padding

        content = content
            .push(scrollable_content)
            .push(summary);

        // Add Load More button with less spacing
        content = content.push(
            button("Load More Addresses")
                .on_press(Message::LoadMore)
                .padding(5)
                .style(theme::Button::Secondary)
        );
    }

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x()
        .padding(10)  // Reduced padding
        .into()
    }
}

async fn enforce_rate_limit() {
    let last = LAST_REQUEST.load(std::sync::atomic::Ordering::Relaxed);
    let now = Instant::now().elapsed().as_millis() as u64;
    let elapsed = now.saturating_sub(last);
    
    if elapsed < MIN_REQUEST_INTERVAL.as_millis() as u64 {
        sleep(Duration::from_millis(MIN_REQUEST_INTERVAL.as_millis() as u64 - elapsed)).await;
    }
    
    LAST_REQUEST.store(now, std::sync::atomic::Ordering::Relaxed);
}

async fn check_balances(xpub: &str, start_idx: usize, end_idx: usize) -> Result<Vec<AddressBalance>, String> {
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

    let extended_pubkey = ExtendedPubKey::from_str(&xpub_to_use)
        .map_err(|e| format!("Invalid extended public key: {}", e))?;

    let secp = Secp256k1::new();
    let mut balances = Vec::new();
    
    // Simpler range calculation - we'll do 10 external and 1 change address per page
    let start_external = start_idx;
    let end_external = end_idx;
    let change_idx = start_idx / 10; // One change address per 10 external addresses

    println!("Checking external addresses {}-{} and change address {}", 
             start_external, end_external, change_idx);

    // First check external addresses
    for i in start_external..end_external {
        let child_numbers = [
            ChildNumber::from_normal_idx(0)
                .map_err(|e| format!("Invalid account number: {}", e))?,
            ChildNumber::from_normal_idx(i as u32)
                .map_err(|e| format!("Invalid index: {}", e))?,
        ];
        
        let path = DerivationPath::from(child_numbers.as_ref());
        println!("Deriving path: m/0/{}", i);
        
        let derived_pubkey = extended_pubkey
            .derive_pub(&secp, &path)
            .map_err(|e| format!("Derivation error: {}", e))?;
        
        let public_key = PublicKey::new(derived_pubkey.public_key);
        let address = Address::p2wpkh(&public_key, network)
            .map_err(|e| format!("Address generation error: {}", e))?;

        enforce_rate_limit().await;

        let balance = match get_testnet_address_balance(&address.to_string()).await {
            Ok(bal) => bal,
            Err(e) if e.contains("rate limit") || e.contains("exceeded") => {
                return Ok(balances);
            }
            Err(e) => return Err(e),
        };

        if balance > 0.0 {
            println!("Found balance of {} BTC at m/0/{}: {}", balance, i, address);
        }
        
        balances.push(AddressBalance {
            address: address.to_string(),
            balance,
            derivation_path: format!("m/0/{}", i),
        });
    }

    // Then check one change address
    let child_numbers = [
        ChildNumber::from_normal_idx(1)
            .map_err(|e| format!("Invalid account number: {}", e))?,
        ChildNumber::from_normal_idx(change_idx as u32)
            .map_err(|e| format!("Invalid index: {}", e))?,
    ];
    
    let path = DerivationPath::from(child_numbers.as_ref());
    println!("Deriving change path: m/1/{}", change_idx);
    
    let derived_pubkey = extended_pubkey
        .derive_pub(&secp, &path)
        .map_err(|e| format!("Derivation error: {}", e))?;
    
    let public_key = PublicKey::new(derived_pubkey.public_key);
    let address = Address::p2wpkh(&public_key, network)
        .map_err(|e| format!("Address generation error: {}", e))?;

    enforce_rate_limit().await;

    let balance = match get_testnet_address_balance(&address.to_string()).await {
        Ok(bal) => bal,
        Err(e) if e.contains("rate limit") || e.contains("exceeded") => {
            return Ok(balances);
        }
        Err(e) => return Err(e),
    };

    if balance > 0.0 {
        println!("Found balance of {} BTC at m/1/{}: {}", balance, change_idx, address);
    }
    
    balances.push(AddressBalance {
        address: address.to_string(),
        balance,
        derivation_path: format!("m/1/{}", change_idx),
    });

    Ok(balances)
}

async fn get_testnet_address_balance(address: &str) -> Result<f64, String> {
    let url = format!("https://blockstream.info/testnet/api/address/{}", address);
    println!("Checking balance for address: {} at URL: {}", address, url);
    
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

fn main() -> iced::Result {
    let mut settings = Settings::default();
    settings.window.resizable = false;  // Disable resizing, I think I'm using tokio wrong and causing a crash on resize
    settings.window.size = (800, 600);  // Set a reasonable fixed window size
    WalletBalanceApp::run(settings)
}