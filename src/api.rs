use bitcoin::Network;
use serde::Deserialize;

pub async fn get_address_balance(address: &str, network: Network) -> Result<f64, String> {
    let base_url = match network {
        Network::Bitcoin => "https://blockstream.info/api",
        Network::Testnet => "https://blockstream.info/testnet/api",
        _ => return Err("Unsupported network".to_string()),
    };

    let url = format!("{}/address/{}", base_url, address);
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("API request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("API error: {}", response.status()));
    }

    let data: AddressData = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse API response: {}", e))?;

    let balance_satoshis = data.chain_stats.funded_txo_sum - data.chain_stats.spent_txo_sum;
    Ok(balance_satoshis as f64 / 100_000_000.0)
}

#[derive(Deserialize)]
struct AddressData {
    chain_stats: ChainStats,
}

#[derive(Deserialize)]
struct ChainStats {
    funded_txo_sum: u64,
    spent_txo_sum: u64,
}
