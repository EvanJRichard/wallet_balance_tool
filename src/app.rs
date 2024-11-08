use iced::{
    alignment, executor, Alignment, Application, Color, Command, Element, Length, Theme,
};
use iced::widget::{
    button, column, container, progress_bar, row, scrollable, text, text_input, Button, Column,
    Container, ProgressBar, Row, Scrollable, Text, TextInput,
};
use crate::executor::CustomExecutor;
use crate::messages::Message;
use crate::wallet::{check_balances, AddressBalance};

pub struct WalletBalanceApp {
    xpub_input: String,
    balances: Vec<AddressBalance>,
    error: Option<String>,
    loading: bool,
    current_page: usize,
    addresses_per_page: usize,
    total_addresses_checked: usize,
}

impl WalletBalanceApp {
    pub fn new() -> Self {
        Self {
            xpub_input: String::new(),
            balances: Vec::new(),
            error: None,
            loading: false,
            current_page: 0,
            addresses_per_page: 10,
            total_addresses_checked: 0,
        }
    }

    fn calculate_address_range(&self) -> (usize, usize) {
        let start = self.current_page * self.addresses_per_page;
        let end = start + self.addresses_per_page;
        (start, end)
    }
}

impl Application for WalletBalanceApp {
    type Message = Message;
    type Executor = CustomExecutor;
    type Flags = ();
    type Theme = Theme;

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (WalletBalanceApp::new(), Command::none())
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
                self.total_addresses_checked = 0;
                Command::none()
            }
            Message::CheckBalance => {
                self.loading = true;
                self.error = None;
                self.current_page = 0;
                self.balances.clear();
                self.total_addresses_checked = 0;
                let xpub = self.xpub_input.clone();
                let range = self.calculate_address_range();
                Command::perform(
                    async move { check_balances(&xpub, range.0, range.1).await },
                    Message::BalanceResult,
                )
            }
            Message::LoadMore => {
                if !self.loading {
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
                    Ok(new_balances) => {
                        self.total_addresses_checked += new_balances.len();
                        self.balances.extend(new_balances);
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
        let title = Text::new("Bitcoin Wallet Balance Discovery Tool")
            .size(24)
            .width(Length::Fill)
            .horizontal_alignment(alignment::Horizontal::Center);

        let input = TextInput::new(
            "Enter extended public key (xpub/vpub)",
            &self.xpub_input,
        )
        .on_input(Message::XpubInputChanged)
        .padding(10)
        .size(16);


        let check_button = Button::new(Text::new("Check Balance"))
            .on_press(Message::CheckBalance)
            .padding(10);

        let mut content = Column::new()
            .push(title)
            .push(input)
            .push(check_button)
            .spacing(15)
            .padding(20)
            .width(Length::Fill)
            .align_items(Alignment::Center);

        if self.loading {
            content = content.push(
                Column::new()
                    .push(Text::new(format!(
                        "Loading addresses {}-{}...",
                        self.total_addresses_checked,
                        self.total_addresses_checked + self.addresses_per_page
                    )))
                    .push(ProgressBar::new(0.0..=100.0, 50.0).width(Length::Fixed(300.0)))
                    .spacing(10)
                    .padding(10),
            );
        }

        if let Some(error) = &self.error {
            content = content.push(
                Text::new(error)
                    .size(14)
                    .style(Color::from_rgb(0.8, 0.0, 0.0))
                    .width(Length::Fill)
                    .horizontal_alignment(alignment::Horizontal::Center),
            );
        }

        if !self.balances.is_empty() {
            let total: f64 = self.balances.iter().map(|b| b.balance).sum();

            let header_row = Row::new()
                .push(Text::new("Path").size(14).width(Length::FillPortion(2)))
                .push(Text::new("Address").size(14).width(Length::FillPortion(5)))
                .push(Text::new("Balance (BTC)").size(14).width(Length::FillPortion(2)))
                .spacing(10)
                .padding(5);

            let balances_list = self.balances.iter().fold(
                Column::new().push(header_row).spacing(2),
                |col, balance| {
                    col.push(
                        Row::new()
                            .push(
                                Text::new(&balance.derivation_path)
                                    .size(12)
                                    .width(Length::FillPortion(2)),
                            )
                            .push(
                                Text::new(&balance.address)
                                    .size(12)
                                    .width(Length::FillPortion(5)),
                            )
                            .push(
                                Text::new(format!("{:.8} BTC", balance.balance))
                                    .size(12)
                                    .width(Length::FillPortion(2)),
                            )
                            .spacing(10)
                            .padding(5),
                    )
                },
            );

            let scrollable_content = Scrollable::new(balances_list)
                .height(Length::Fixed(250.0))
                .width(Length::Fill);

            let summary = Column::new()
                .push(Text::new(format!(
                    "Addresses checked: {}",
                    self.total_addresses_checked
                )))
                .push(
                    Text::new(format!("Total Balance: {:.8} BTC", total))
                        .size(16)
                        .style(Color::from_rgb(0.0, 0.5, 0.0)),
                )
                .spacing(5)
                .padding(5);

            content = content.push(scrollable_content).push(summary).push(
                Button::new(Text::new("Load More Addresses"))
                    .on_press(Message::LoadMore)
                    .padding(5),
            );
        }

        Container::new(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .padding(10)
            .into()
    }
}
