#[derive(Debug, Clone)]
pub enum Message {
    XpubInputChanged(String),
    CheckBalance,
    LoadMore,
    BalanceResult(Result<Vec<crate::wallet::AddressBalance>, String>),
}
