mod app;
mod executor;
mod messages;
mod wallet;
mod api;
mod utils;

use iced::{Application, Settings};
use app::WalletBalanceApp;

fn main() -> iced::Result {
    let mut settings = Settings::default();
    settings.window.resizable = false;
    settings.window.size = (800, 600);
    WalletBalanceApp::run(settings)
}
