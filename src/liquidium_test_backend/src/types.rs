use std::cmp::Ordering;
use candid::{CandidType, Deserialize, Principal};
use ic_cdk::api::management_canister::bitcoin::Utxo;
use ordinals::RuneId;
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, CandidType, Deserialize, Clone)]
pub struct LoanOffer {
    pub id : String,
    // this is the collateral amount
    pub collateral_amt: u64, // runes
    pub rune_id: String,
    pub max_ltv : f32,
    pub lender: Principal,
    pub liquidity_bundle_id : String,
    pub term: u32,
    pub interest: f32
}

#[derive(Debug, CandidType, Serialize, Clone, Deserialize)]
pub struct OutpointData {
    pub txn_id : String,
    pub index : u32,
    pub amount: u64,
    pub height : u32
}

impl OutpointData {
    pub fn from_utxo(utxo : &Utxo) -> Self {
        Self {
            txn_id : hex::encode(utxo.outpoint.txid.clone()),
            index: utxo.outpoint.vout,
            amount: utxo.value,
            height: utxo.height
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buffer = vec![];
        ciborium::into_writer(self, &mut buffer).unwrap();
        buffer
    }

    pub fn compute_hash(&self) -> String {
        let type_bytes = self.to_bytes();
        let mut hasher = Sha256::new();
        hasher.update(type_bytes);
        let result = hasher.finalize();
        hex::encode(result)
    }
}

#[derive(Debug, CandidType, Deserialize, Clone)]
pub struct RuneIdData {
    pub block: u64,
    pub tx: u32
}

impl From<RuneId> for RuneIdData {
    fn from(value: RuneId) -> Self {
        RuneIdData { block: value.block, tx: value.tx }
    }
}

impl ToString for RuneIdData {
    fn to_string(&self) -> String {
        format!("{}:{}", self.block, self.tx)
    }
}
#[derive(Debug, CandidType, Deserialize, Clone)]
pub struct Collateral {
    pub id : String,
    pub rune_id: String,
    pub trx_hex : Option<String>,
    pub amount: u64,
    pub owner : Principal,
    pub txn_id : String,
    pub address : String,
    // checks if this collateral is in use.
    pub loan_trx : Option<String>
}


#[derive(Debug, CandidType, Deserialize, Clone)]
pub struct Bundle {
    pub id : String,
    pub lender : Principal,
    pub amount: u64,
    pub txn_id: String, // containin the transaction
    // checks if this collateral is in use.
    pub loan_trx : Option<String>,
    pub address : String
}
#[derive(Debug, CandidType, Deserialize, Clone)]
pub struct LoanRequest {
    pub offer_id : String,
    pub collateral_id: String,
    pub receive_addr : String
}

#[derive(Debug, PartialEq, Eq, Clone,  CandidType, Deserialize)]
pub enum LoanStatus {
    PENDING,
    PAID_BACK(String),
    DEFAULTED(String)
}

#[derive(Debug, Clone,  CandidType, Deserialize)]
pub struct LoanTransaction {
    pub bundle_id : String,
    pub collateral_id: String,
    pub owner: Principal,
    pub created_at : u64,
    pub expires_at : u64,
    pub sat_worth: u64,
    pub id : String,
    pub ltv : f64,
    pub borrowed_amount: u64,
    pub txn_id: String,
    pub loan_status: LoanStatus,
    pub interest : f32
}

#[derive(Debug, Clone)]
pub struct ExpiryResource<T> {
    pub expiration : u64, // millisecs
    pub data : Option<T>
}
impl <T>Ord for ExpiryResource<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.expiration.cmp(&other.expiration)
    }
}

impl<T> PartialEq for ExpiryResource<T> {
    fn eq(&self, other: &Self) -> bool {
        self.expiration == other.expiration
    }
}

impl<T> Eq for ExpiryResource<T> {}

impl<T> PartialOrd for ExpiryResource<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

