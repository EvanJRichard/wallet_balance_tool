use bitcoin::{
    base58, bip32::{ChildNumber, DerivationPath, ExtendedPubKey}, hashes::{sha256, Hash as BitcoinHash}, Address, Network,
    PublicKey, secp256k1::Secp256k1,
};
use std::str::FromStr;

use crate::api::get_address_balance;
use crate::utils::enforce_rate_limit;

#[derive(Debug, Clone)]
pub struct AddressBalance {
    pub address: String,
    pub balance: f64,
    pub derivation_path: String,
}

pub async fn check_balances(
    xpub: &str,
    start_idx: usize,
    end_idx: usize,
) -> Result<Vec<AddressBalance>, String> {
    let (network, xpub_to_use) = parse_xpub(xpub)?;
    let extended_pubkey = ExtendedPubKey::from_str(&xpub_to_use)
        .map_err(|e| format!("Invalid extended public key: {}", e))?;
    let secp = Secp256k1::new();
    let mut balances = Vec::new();

    // Check external addresses
    for i in start_idx..end_idx {
        let path = DerivationPath::from_str(&format!("m/0/{}", i))
            .map_err(|e| format!("Invalid derivation path: {}", e))?;
        let derived_pubkey = extended_pubkey
            .derive_pub(&secp, &path)
            .map_err(|e| format!("Derivation error: {}", e))?;
        let public_key = PublicKey::new(derived_pubkey.public_key);
        let address = Address::p2wpkh(&public_key, network)
            .map_err(|e| format!("Address generation error: {}", e))?
            .to_string();

        enforce_rate_limit().await;

        let balance = match get_address_balance(&address, network).await {
            Ok(bal) => bal,
            Err(e) if e.contains("rate limit") || e.contains("exceeded") => {
                return Ok(balances);
            }
            Err(e) => return Err(e),
        };

        balances.push(AddressBalance {
            address,
            balance,
            derivation_path: path.to_string(),
        });
    }

    // Check change address
    let change_idx = start_idx / 10;
    let path = DerivationPath::from_str(&format!("m/1/{}", change_idx))
        .map_err(|e| format!("Invalid derivation path: {}", e))?;
    let derived_pubkey = extended_pubkey
        .derive_pub(&secp, &path)
        .map_err(|e| format!("Derivation error: {}", e))?;
    let public_key = PublicKey::new(derived_pubkey.public_key);
    let address = Address::p2wpkh(&public_key, network)
        .map_err(|e| format!("Address generation error: {}", e))?
        .to_string();

    enforce_rate_limit().await;

    let balance = match get_address_balance(&address, network).await {
        Ok(bal) => bal,
        Err(e) if e.contains("rate limit") || e.contains("exceeded") => {
            return Ok(balances);
        }
        Err(e) => return Err(e),
    };

    balances.push(AddressBalance {
        address,
        balance,
        derivation_path: path.to_string(),
    });

    Ok(balances)
}

fn parse_xpub(xpub: &str) -> Result<(Network, String), String> {
    if xpub.starts_with("vpub") {
        let decoded = base58::from_check(xpub)
            .map_err(|e| format!("Failed to decode vpub: {}", e))?;

        if decoded.len() < 78 {
            return Err("Invalid extended public key length".to_string());
        }

        // Extract the key material (everything except version and checksum)
        let key_material = &decoded[4..decoded.len() - 4];

        // Create new vector with tpub version bytes
        let mut modified = Vec::with_capacity(78);
        modified.extend_from_slice(&[0x04, 0x35, 0x87, 0xCF]); // tpub version
        modified.extend_from_slice(key_material);
        
        // Calculate double SHA256 checksum
        let hash1 = sha256::Hash::hash(&modified);
        let hash2 = sha256::Hash::hash(hash1.as_ref());
        let checksum = hash2[..4].to_vec(); // checksum now owns the data
        modified.extend_from_slice(&checksum);

        let tpub = base58::encode_check(&modified);

        Ok((Network::Testnet, tpub))
    } else if xpub.starts_with("xpub") {
        Ok((Network::Bitcoin, xpub.to_string()))
    } else {
        Err("Unsupported extended public key format. Must start with 'xpub' or 'vpub'".to_string())
    }
}
