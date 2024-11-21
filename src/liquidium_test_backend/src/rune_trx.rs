use bitcoin::{
    absolute::LockTime,
    blockdata::{opcodes::all::OP_RETURN, script::Builder},
    consensus::encode::serialize,
    hashes::Hash,
    secp256k1::{Secp256k1, SecretKey},
    sighash::{SighashCache, TapSighashType},
    transaction::Version,
    Address, Amount, Network, Script, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid, Witness,
};
use ic_cdk::api::management_canister::bitcoin::Utxo;
use ordinals::{Edict, RuneId, Runestone};
use std::collections::HashMap;
use std::{io::Read, str::FromStr};

#[derive(Debug)]
pub struct RuneTransferParams {
    // pub symbol: String,
    pub amount: u64,
    pub recipient_address: Address,
    pub fee_rate: u64, // sats/vbyte,
    pub rune_id: String,
}

fn to_runeid(id: &str) -> RuneId {
    let d: Vec<&str> = id.split(":").collect();
    let block: u64 = d[0].parse().unwrap();
    RuneId {
        block,
        tx: d[1].parse().unwrap(),
    }
}

pub struct RuneTransactionBuilder {
    network: Network,
    dust_limit: Amount,
    secp: Secp256k1<bitcoin::secp256k1::All>,
}

impl RuneTransactionBuilder {
    pub fn new(network: Network) -> Self {
        Self {
            network,
            dust_limit: Amount::from_sat(546),
            secp: Secp256k1::new(),
        }
    }

    /// Build unsigned transfer transaction
    pub async fn build_unsigned_transfer(
        &self,
        params: &RuneTransferParams,
        utxos: &[Utxo],
        change_address: &Address,
    ) -> Result<Transaction, Box<dyn std::error::Error>> {
        // Calculate total available input amount
        let total_input: Amount = utxos
            .iter()
            .map(|utxo| Amount::from_sat(utxo.value))
            .sum::<Amount>();

        // Create inputs
        let inputs: Vec<TxIn> = utxos
            .iter()
            .map(|utxo| {
                let txid = Txid::from_slice(utxo.outpoint.txid.as_slice()).unwrap();
                return TxIn {
                    previous_output: bitcoin::OutPoint::new(txid, utxo.outpoint.vout),
                    script_sig: ScriptBuf::new(),
                    sequence: Sequence::MAX,
                    witness: Witness::new(),
                };
            })
            .collect();

        // Create outputs
        // let mut outputs = Vec::new();

        let edict = Edict {
            id: to_runeid(&params.rune_id),
            amount: params.amount as u128,
            output: 0,
        };

        let stone = Runestone {
            edicts: vec![edict],
            etching: None,
            mint: None,
            pointer: None,
        };

        let mut outputs_vec = vec![
            TxOut {
                value: Amount::from_sat(546),
                script_pubkey: stone.encipher(),
            },
            TxOut {
                value: Amount::from_sat(0),
                script_pubkey: params.recipient_address.script_pubkey(),
            },
        ];

        // // 1. Add Rune OP_RETURN output
        // // let rune_script = self.create_rune_transfer_script(&params.symbol, params.amount);
        // outputs.push(TxOut {
        //     value: Amount::from_sat(546),
        //     script_pubkey: ScriptBuf::from_bytes(script_buf.as_bytes().to_vec()),
        // });

        // // 2. Add recipient output (with dust limit for Rune bearing output)
        // outputs.push(TxOut {
        //     value: 0,
        //     script_pubkey: params.recipient_address.script_pubkey(),
        // });

        // Calculate approximate transaction size for fee
        let approx_tx_size = self.estimate_tx_size(inputs.len(), outputs_vec.len() + 1); // +1 for potential change
        let fee_amount = Amount::from_sat((approx_tx_size as u64 * params.fee_rate) as u64);

        // Calculate total output amount so far
        let output_amount = outputs_vec
            .iter()
            .map(|output| output.value)
            .sum::<Amount>();

        // Calculate change amount
        if let Some(change_amount) = total_input
            .checked_sub(output_amount)
            .unwrap()
            .checked_sub(fee_amount)
        {
            // Only add change output if it's above dust limit
            if change_amount > self.dust_limit {
                outputs_vec.push(TxOut {
                    value: change_amount,
                    script_pubkey: change_address.script_pubkey(),
                });
            }
        } else {
            return Err("Insufficient funds for transfer".into());
        }

        Ok(Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: inputs,
            output: outputs_vec,
        })
    }

    fn estimate_tx_size(&self, num_inputs: usize, num_outputs: usize) -> usize {
        // Basic transaction overhead
        let mut size = 10; // version + locktime

        // Size for inputs (approximate)
        size += num_inputs * 148; // typical signed input size

        // Size for outputs
        size += num_outputs * 34; // typical output size

        size
    }
}
