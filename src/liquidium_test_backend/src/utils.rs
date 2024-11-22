use bitcoin::{consensus::deserialize, Transaction};
use ordinals::{Artifact, RuneId, Runestone};
use serde_json::Value;


pub fn json_to_transaction(tx_data: &Value) -> Result<Transaction, String> {
    // Get hex from JSON
    let hex = tx_data["hex"]
        .as_str()
        .ok_or("No hex field found in transaction data")?;
    
    // Convert hex to bytes
    let tx_bytes = hex::decode(hex)
        .map_err(|e| format!("Failed to decode hex: {}", e))?;
    
    // Deserialize bytes to Transaction
    deserialize(&tx_bytes)
        .map_err(|e| format!("Failed to deserialize transaction: {}", e))
}

pub fn validate_rune_transaction(
    transaction: &Transaction,
    rune_id: &str,
    expected_amount: u128,
) -> Result<u128, String> {
    // Parse rune ID
    let parts: Vec<&str> = rune_id.split(':').collect();
    if parts.len() != 2 {
        return Err("Invalid rune ID format. Expected 'block:tx'".to_string());
    }

    let expected_id = RuneId {
        block: parts[0].parse()
            .map_err(|_| "Invalid block number")?,
        tx: parts[1].parse()
            .map_err(|_| "Invalid transaction index")?
    };

    // Extract Runestone
    let artifact = Runestone::decipher(transaction)
        .ok_or("No Runestone found in transaction")?;

    // Check for Cenotaph
    let runestone = match artifact {
        Artifact::Cenotaph(_) => Err("Transaction contains a cenotaph"),
        Artifact::Runestone(runestone) => Ok(runestone),
    }?;

    // Calculate total amount for the specific rune
    let total_amount: u128 = runestone.edicts
        .iter()
        .filter(|edict| edict.id == expected_id)
        .map(|edict| edict.amount)
        .sum();

    if total_amount == 0 {
        return Err("Expected rune ID not found in transaction".to_string());
    }

    if total_amount != expected_amount {
        return Err(format!(
            "Invalid amount. Expected: {}, Found: {}",
            expected_amount,
            total_amount
        ));
    }

    Ok(total_amount)
}