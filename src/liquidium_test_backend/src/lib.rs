use std::{
    cell::{Ref, RefCell},
    collections::{HashMap, HashSet},
    fmt::format,
    ops::Mul,
    str::FromStr,
    vec,
};

use bitcoin::{
    consensus::{encode::deserialize_hex, serialize},
    Address, Network, Transaction,
};
use bitcoin_api::{
    build_transaction_with_fee, get_btc_address, get_fee_per_byte, send_transaction,
    transform_network,
};
use candid::{candid_method, Nat, Principal};
use ecdsa_api::get_ecdsa_public_key;
use ic_cdk::{
    api::{
        call,
        management_canister::{
            self,
            bitcoin::{BitcoinNetwork, GetBalanceRequest, GetUtxosRequest, Outpoint, Utxo},
            ecdsa::{self, EcdsaKeyId, EcdsaPublicKeyArgument},
            http_request::{
                CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse, TransformArgs,
                TransformContext,
            },
            main::raw_rand,
        },
        time,
    }, caller, println, query, update
};
use ordinals::{Artifact, Edict, Rune, Runestone};
use p2pkh::{build_p2pkh_spend_tx, ecdsa_sign_transaction};
use redblack::RedBlackTree;
use rune_trx::{RuneTransactionBuilder, RuneTransferParams};
use serde_json::{json, Value};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use types::{
    Bundle, Collateral, ExpiryResource, LoanOffer, LoanRequest, LoanStatus, LoanTransaction,
    OutpointData, RuneIdData,
};
use utils::{json_to_transaction, validate_rune_transaction};

mod bitcoin_api;
mod ecdsa_api;
mod p2pkh;
mod redblack;
mod rune_trx;
mod types;
mod utils;

#[cfg(test)]
mod test;

const RPC_USER: &str = "ic-btc-integration";
const RPC_PASS: &str = "QPQiNaph19FqUsCrBRN0FII7lyM26B51fAMeBQzCb-E=";
const BTC_RPC_URL: &str = "http://127.0.0.1:18443";

#[ic_cdk::query]
fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}

thread_local! {
    static LENDER_OFFER : RefCell<HashMap<Principal, HashSet<String>>> = RefCell::new(HashMap::new());
    static LOAN_OFFER : RefCell<HashMap<String, LoanOffer>> = RefCell::new(HashMap::new());
    static BUNDLES : RefCell<HashMap<String, Bundle>> = RefCell::new(HashMap::new());
    static COLLATERALS : RefCell<HashMap<String, Collateral>> = RefCell::new(HashMap::new());
    static LENDER_BUNDLE : RefCell<HashMap<Principal, HashSet<String>>> = RefCell::new(HashMap::new());
    static BORROWER_COLLATERALS : RefCell<HashMap<Principal, HashSet<String>>> = RefCell::new(HashMap::new());
    static LOAN_TRANSACTIONS : RefCell<HashMap<String, LoanTransaction>> = RefCell::new(HashMap::new());
    static BORROWER_LOAN_TRANSACTIONS : RefCell<HashMap<Principal, HashSet<String>>> = RefCell::new(HashMap::new());
    static SORTED_DATA: std::cell::RefCell<RedBlackTree<ExpiryResource<String>>> = std::cell::RefCell::new(RedBlackTree::new());
   
}

#[update(guard = "is_not_anonymous")]
pub async fn deposit_address() -> String {
    let derivation_path = vec![caller().to_text().as_bytes().to_vec()];
    get_p2p_address_from_path(derivation_path).await
}

pub async fn get_p2p_address_from_path(derivation_path: Vec<Vec<u8>>) -> std::string::String {
    let ecdsa_response = get_ecdsa_public_key(derivation_path).await;

    public_key_to_p2pkh_address(&ecdsa_response.public_key)
}

#[update(guard = "is_not_anonymous")]
pub async fn create_liquidity_bundle(mut bundle: Bundle) -> Result<(String, String), String> {
    let lender_path = vec![caller().to_text().as_bytes().to_vec()];
    let lender_pk = get_ecdsa_public_key(lender_path.clone()).await.public_key;
    let lender_addr = get_btc_address(lender_path.clone(), BitcoinNetwork::Regtest).await;

    let (balance,) =
        ic_cdk::api::management_canister::bitcoin::bitcoin_get_balance(GetBalanceRequest {
            address: lender_addr.to_string(),
            network: BitcoinNetwork::Regtest,
            min_confirmations: None,
        })
        .await
        .unwrap();
    if bundle.amount > balance {
        return Err(format!("Insufficient Fund"));
    }
    bundle.lender = caller();
    bundle.loan_trx = None;
    let (raw_bytes,) = raw_rand().await.unwrap();
    let bundle_id = hex::encode(&raw_bytes);
    let bundle_path = vec![raw_bytes];

    let bundle_addr = get_btc_address(bundle_path, BitcoinNetwork::Regtest).await;

    let (lender_utxo,) =
        ic_cdk::api::management_canister::bitcoin::bitcoin_get_utxos(GetUtxosRequest {
            address: lender_addr.to_string(),
            network: BitcoinNetwork::Regtest,
            filter: None,
        })
        .await
        .unwrap();
    let fee_byte = get_fee_per_byte(BitcoinNetwork::Regtest).await;
    let bundle_transaction = build_p2pkh_spend_tx(
        &lender_pk,
        &lender_addr,
        &lender_utxo.utxos,
        &bundle_addr,
        bundle.amount,
        fee_byte,
    )
    .await?;
    let signed_bundle_transaction = ecdsa_sign_transaction(
        &lender_pk,
        &lender_addr,
        bundle_transaction,
        "dfx_test_key".to_string(),
        lender_path,
        ecdsa_api::get_ecdsa_signature,
    )
    .await;
    let tx_bytes = serialize(&signed_bundle_transaction);
    send_transaction(BitcoinNetwork::Regtest, tx_bytes).await;
    let txn_id = signed_bundle_transaction.compute_txid().to_string();

    bundle.id = bundle_id.clone();
    bundle.address = bundle_addr.to_string();
    bundle.txn_id = txn_id.clone();

    BUNDLES.with_borrow_mut(|store| {
        store.insert(bundle_id.clone(), bundle);
    });

    LENDER_BUNDLE.with_borrow_mut(|store| {
        let set = store.entry(caller()).or_default();
        set.insert(bundle_id.clone())
    });

    Ok((bundle_id, txn_id))
}

#[update(guard = "is_not_anonymous")]
pub async fn create_collateral(mut collateral: Collateral) -> Result<(String, String), String> {
    let user_addr = deposit_address().await;
    let minimum_dust = 1000;
    let mut total_value = 0u64;
    let mut sat_amount = 0u64;
   
    let mut checked_trx = HashSet::new();

    let (utxo_resp,) =
        ic_cdk::api::management_canister::bitcoin::bitcoin_get_utxos(GetUtxosRequest {
            address: user_addr,
            network: BitcoinNetwork::Regtest,
            filter: None,
        })
        .await
        .unwrap();

    // get utxos that contains runes
    for utxo in &utxo_resp.utxos {
        let utxo_value = utxo.value;
        let output_data = OutpointData::from_utxo(utxo);
        if total_value >= collateral.amount {
            if sat_amount < minimum_dust {
                sat_amount += utxo_value;
                // output_vec.push(utxo);
                continue;
            }
            break;
        }

        let txn_id = output_data.txn_id.clone();

        ic_cdk::println!("Utxo Trx: {}", txn_id);

        // total_value += output_data.amount;
        if checked_trx.contains(&txn_id) {
            continue;
        }

        let trx_hex = get_raw_transaction(&txn_id).await.unwrap();
        let trx: Transaction = deserialize_hex(&trx_hex).unwrap();
        checked_trx.insert(txn_id.clone());
        let artifact_opt = ordinals::Runestone::decipher(&trx);
        if artifact_opt.is_none() {
            continue;
        }
        let artifact = artifact_opt.unwrap();
        let runestone = match artifact {
            Artifact::Cenotaph(cenotaph) => {continue;},
            Artifact::Runestone(runestone) => runestone,
        };

        for edict in runestone.edicts {
            let rune_id = RuneIdData::from(edict.id).to_string();
            if collateral.rune_id == rune_id {
                total_value += edict.amount as u64
            }
        }
        // output_vec.push(utxo);
    }

    if collateral.amount > total_value {
        return Err(format!("Insufficient Funds"));
    }

    let (raw_bytes,) = raw_rand().await.unwrap();

    let collateral_id = hex::encode(&raw_bytes);
    let colateral_path = vec![raw_bytes];

    let collateral_addr = get_btc_address(colateral_path, BitcoinNetwork::Regtest).await;
    let collateral_addr_str = collateral_addr.to_string();
    let fee_rate = get_fee_per_byte(BitcoinNetwork::Regtest).await;
    let rune_params = RuneTransferParams {
        amount: collateral.amount,
        recipient_address: collateral_addr,
        fee_rate,

        rune_id: collateral.rune_id.clone()
    };
    let rune_trx_builder = RuneTransactionBuilder::new();
    let borrower_path = vec![caller().to_text().as_bytes().to_vec()];
    let user_address = get_btc_address(borrower_path.clone(), BitcoinNetwork::Regtest).await;
    let transaction = rune_trx_builder
        .build_unsigned_transfer(&rune_params, &utxo_resp.utxos, &user_address, total_value)
        .await
        .unwrap();

    let borrower_pk = get_ecdsa_public_key(borrower_path.clone()).await.public_key;
    
    let signed = ecdsa_sign_transaction(
        &borrower_pk,
        &user_address,
        transaction,
        format!("dfx_test_key"),
        borrower_path,
        ecdsa_api::get_ecdsa_signature,
    )
    .await;

    let txn_id = signed.compute_txid().to_string();
    let signed_bytes = serialize(&signed);

    collateral.owner = caller();
    collateral.id = collateral_id.clone();
    collateral.loan_trx = None;
    collateral.txn_id = txn_id.clone();
    collateral.address = collateral_addr_str;

    send_transaction(BitcoinNetwork::Regtest, signed_bytes).await;

    COLLATERALS.with_borrow_mut(|store| store.insert(collateral.id.clone(), collateral));

    BORROWER_COLLATERALS.with_borrow_mut(|store| {
        let user_set = store.entry(caller()).or_default();
        user_set.insert(collateral_id.clone())
    });

    Ok((collateral_id, txn_id))
}

#[update(guard = "is_not_anonymous")]
pub async fn create_loan_offer(mut offer: LoanOffer) -> Result<(), String> {
    BUNDLES.with_borrow(|store| {
        if !store.contains_key(&offer.liquidity_bundle_id) {
            return Err(format!("Collateral not found"));
        }

        let bundle = store.get(&offer.liquidity_bundle_id).unwrap();
        if caller() != bundle.lender {
            return Err(format!("You are not owner of the collateral bundle"));
        }

        if bundle.loan_trx.is_some() {
            return Err(format!("This Liquidity Bundle is in use."));
        }

        Ok(())
    })?;

    let (rnd_bytes,) = raw_rand().await.unwrap();
    let loan_id = hex::encode(rnd_bytes);
    offer.lender = caller();
    offer.id = loan_id.clone();
    LOAN_OFFER.with_borrow_mut(|store| {
        store.insert(loan_id.clone(), offer);
    });

    LENDER_OFFER.with_borrow_mut(|store| {
        let set = store.entry(caller()).or_default();
        set.insert(loan_id)
    });

    Ok(())
}

#[query]
pub async fn get_loan_offers(lender: Option<Principal>) -> Vec<LoanOffer> {
    if lender.is_none() {
        LOAN_OFFER.with_borrow(|store| {
            let mut loan_vec = vec![];
            for (_, v) in store {
                loan_vec.push(v.clone());
            }
            loan_vec
        })
    } else {
        let lender_id = lender.unwrap();
        LENDER_OFFER.with_borrow_mut(|store| {
            let set = store.entry(lender_id).or_default();
            set.iter()
                .map(|id| LOAN_OFFER.with_borrow(|store| store.get(id).cloned().unwrap()))
                .collect()
        })
    }
}


#[update(guard = "is_not_anonymous")]
async fn get_loan(request: LoanRequest) -> Result<String, String> {
    let (offer, bundle, collateral) = COLLATERALS
        .with_borrow_mut(|store| {
            let collateral_opt = store.get_mut(&request.collateral_id);
            if collateral_opt.is_none() {
                return Err(format!("Collateral not found"));
            };

            let collateral = collateral_opt.unwrap();
            if caller() != collateral.owner {
                return Err(format!("You are not authorized to used this collateral"));
            };

            if collateral.loan_trx.is_some() {
                return Err(format!("This collateral is in use"));
            }

            let offer_opt =
                LOAN_OFFER.with_borrow_mut(|store| store.get(&request.offer_id).cloned());

            if offer_opt.is_none() {
                return Err(format!("Offer doesn't exixst"));
            }

            let offer = offer_opt.unwrap();

            if offer.rune_id != collateral.rune_id {
                return Err(format!(
                    "Your collateral rune is not same with rune required for offer"
                ));
            }

            if offer.collateral_amt > collateral.amount {
                return Err(format!(
                    "Your collateral amount is less than required for this offer"
                ));
            }

            let bundle = BUNDLES.with_borrow(|store| {
                let bundle_opt = store.get(&offer.liquidity_bundle_id);
                if bundle_opt.is_none() {
                    return Err(format!("Bundle not found"));
                }

                let bundle = bundle_opt.unwrap();
                if bundle.loan_trx.is_some() {
                    return Err(format!("Liquidity Bundle is in use"));
                }

                Ok(bundle.clone())
            })?;

            Ok((
                offer,
                bundle,
                collateral.clone(),
            ))
        })?;
    

    let current_time = nanoseconds_to_milliseconds(time());
    let expiry = current_time + days_to_milliseconds(offer.term as u64);
    let json_trx = get_transaction_info(&collateral.txn_id).await?;

    check_confirmation(&json_trx)?;

    let trx = json_to_transaction(&json_trx)?;

    validate_rune_transaction(&trx, &collateral.rune_id, collateral.amount as u128)?;

    let sat_worth = get_rune_unit_price(collateral.rune_id).await.mul(collateral.amount);
    let borrowed_amt = (sat_worth as f64).mul(offer.max_ltv as f64).floor() as u64;
    let (raw_byte,) = raw_rand().await.unwrap();
    let loan_trx_id = hex::encode(raw_byte);

    let mut loan_trx = LoanTransaction {
        bundle_id: bundle.id.clone(),
        collateral_id: request.collateral_id.clone(),
        owner: caller(),
        expires_at: expiry,
        sat_worth,
        id: loan_trx_id.clone(),
        ltv: offer.max_ltv as f64,
        borrowed_amount: borrowed_amt,
        txn_id: format!(""),
        loan_status: types::LoanStatus::PENDING,
        created_at: current_time,
        interest: offer.interest,
    };

    let loan_expiry = loan_trx.expires_at;

    let bundle_path = vec![hex::decode(bundle.id).unwrap()];
    let bundle_pk = get_ecdsa_public_key(bundle_path.clone()).await.public_key;
    let lender_path = vec![bundle.lender.to_text().as_bytes().to_vec()];
    let lender_addr = get_btc_address(lender_path.clone(), BitcoinNetwork::Regtest).await;
    let bundle_addr = get_btc_address(bundle_path.clone(), BitcoinNetwork::Regtest).await;
    let (bundle_utxo_resp,) =
        ic_cdk::api::management_canister::bitcoin::bitcoin_get_utxos(GetUtxosRequest {
            address: bundle.address.clone(),
            network: BitcoinNetwork::Regtest,
            filter: None,
        })
        .await
        .unwrap();
    let dst_addr = Address::from_str(&request.receive_addr)
        .unwrap()
        .require_network(transform_network(BitcoinNetwork::Regtest))
        .unwrap();
    let fee_byte = get_fee_per_byte(BitcoinNetwork::Regtest).await;
    let transaction = build_p2pkh_spend_tx(
        &bundle_pk,
        &lender_addr,
        &bundle_utxo_resp.utxos,
        &dst_addr,
        borrowed_amt,
        fee_byte,
    )
    .await?;
    let signed_trx = ecdsa_sign_transaction(
        &bundle_pk,
        &bundle_addr,
        transaction,
        "dfx_test_key".to_string(),
        bundle_path,
        ecdsa_api::get_ecdsa_signature,
    )
    .await;

    let tx_bytes = serialize(&signed_trx);
    send_transaction(BitcoinNetwork::Regtest, tx_bytes).await;
    loan_trx.txn_id = signed_trx.compute_txid().to_string();
    COLLATERALS.with_borrow_mut(|store| {
        let collateral = store.get_mut(&loan_trx.collateral_id).unwrap();
        collateral.loan_trx = Some(loan_trx_id.clone())
    });

    BUNDLES.with_borrow_mut(|store| {
        let collateral = store.get_mut(&loan_trx.bundle_id).unwrap();
        collateral.loan_trx = Some(loan_trx_id.clone())
    });

    LOAN_TRANSACTIONS.with_borrow_mut(|store| store.insert(loan_trx.id.clone(), loan_trx));

    BORROWER_LOAN_TRANSACTIONS.with_borrow_mut(|store| {
        let set = store.entry(caller()).or_default();
        set.insert(loan_trx_id.clone())
    });

    SORTED_DATA.with_borrow_mut(|store| {
        let resource = ExpiryResource {
            expiration: loan_expiry,
            data: Some(loan_trx_id.clone()),
        };
        store.insert(resource);
    });
    Ok(loan_trx_id)
}

#[update]
async fn btc_balance(address: String) -> u64 {
    let (bal,) =
        ic_cdk::api::management_canister::bitcoin::bitcoin_get_balance(GetBalanceRequest {
            address,
            network: BitcoinNetwork::Regtest,
            min_confirmations: None,
        })
        .await
        .unwrap();
    return bal;
}

fn check_confirmation(tx_data: &Value) -> Result<(), String> {
    // Check if blockhash exists (transaction is in a block)
    if tx_data["blockhash"].is_null() {
        return Err("Transaction not yet included in a block".to_string());
    }

    // Verify confirmations value exists and is greater than 0
    match tx_data["confirmations"].as_u64() {
        Some(confirms) if confirms > 0 => Ok(()),
        Some(_) => Err("Transaction is in mempool".to_string()),
        None => Err("Unable to get confirmation data".to_string()),
    }
}

async fn check_expired_loan() {
    let list = SORTED_DATA.with_borrow_mut(|rb| {
        let expiry = ExpiryResource {
            expiration: (time() / 1_000_000),
            data: None,
        };
        let list = rb.remove_less_than_or_equal(&expiry);

        list
    });

    for resource in list {
        if resource.data.is_none() {
            continue;
        }

        let trx_id = resource.data.unwrap();
        let trx_opt = LOAN_TRANSACTIONS.with_borrow_mut(|store| {
            return store.get(&trx_id).cloned();
        });

        if trx_opt.is_none() {
            continue;
        }

        let trx = trx_opt.unwrap();
        if trx.loan_status != LoanStatus::PENDING {
            continue;
        }

        let bundle_opt = BUNDLES.with_borrow_mut(|store| {
            return store.get(&trx.bundle_id).cloned();
        });

        if bundle_opt.is_none() {
            continue;
        }

        let bundle = bundle_opt.unwrap();

        let lender_addr =
            get_p2p_address_from_path(vec![bundle.lender.to_text().as_bytes().to_vec()]).await;

        // get collater
        let collateral_opt =
            COLLATERALS.with_borrow(|store| store.get(&trx.collateral_id).cloned());
        if collateral_opt.is_none() {
            continue;
        }

        let collateral = collateral_opt.unwrap();

        let rune_trx_builder =
            RuneTransactionBuilder::new();

        // let rune = rune_opt.unwrap();
        let fee_rate = get_fee_per_byte(BitcoinNetwork::Regtest).await;
        let rune_params = RuneTransferParams {
            amount: collateral.amount,
            recipient_address: Address::from_str(&lender_addr)
                .unwrap()
                .require_network(transform_network(BitcoinNetwork::Regtest))
                .unwrap(),
            fee_rate: fee_rate,
            rune_id: collateral.rune_id.clone()
        };

        let borrower_path = vec![collateral.owner.to_text().as_bytes().to_vec()];
        let borrower_addr = get_p2p_address_from_path(borrower_path.clone()).await;
        let collateral_path = vec![hex::decode(&collateral.id).unwrap()];
        let borrower_addr = Address::from_str(&borrower_addr)
            .unwrap()
            .require_network(transform_network(BitcoinNetwork::Regtest))
            .unwrap();
        let (utxo_resp,) =
            ic_cdk::api::management_canister::bitcoin::bitcoin_get_utxos(GetUtxosRequest {
                address: collateral.address.clone(),
                network: BitcoinNetwork::Regtest,
                filter: None,
            })
            .await
            .unwrap();
        let unsigned_trx_rslt = rune_trx_builder
            .build_unsigned_transfer(&rune_params, &utxo_resp.utxos, &borrower_addr, collateral.amount)
            .await;
        if unsigned_trx_rslt.is_err() {
            continue;
        }

        let collateral_pk = get_ecdsa_public_key(collateral_path.clone())
            .await
            .public_key;
        let collateral_btc_address = Address::from_str(&collateral.address)
            .unwrap()
            .require_network(transform_network(BitcoinNetwork::Regtest))
            .unwrap();
        let signed_trx = ecdsa_sign_transaction(
            &collateral_pk,
            &collateral_btc_address,
            unsigned_trx_rslt.unwrap(),
            "dfx_test_key".to_string(),
            collateral_path,
            ecdsa_api::get_ecdsa_signature,
        )
        .await;
        let signed_bytes = serialize(&signed_trx);
        let rune_trx_id = signed_trx.compute_txid().to_string();

        send_transaction(BitcoinNetwork::Regtest, signed_bytes).await;

        COLLATERALS.with_borrow_mut(|store| store.remove(&collateral.id));

        BUNDLES.with_borrow_mut(|store| store.remove(&bundle.id));

        LOAN_TRANSACTIONS.with_borrow_mut(|store| {
            let trx = store.get_mut(&trx_id).unwrap();
            trx.loan_status = LoanStatus::DEFAULTED(rune_trx_id)
        })
    }
}

#[update(guard = "is_not_anonymous")]
async fn pay_loan(loan_trx: String) -> Result<(String, String), String> {
    let loan_transaction = LOAN_TRANSACTIONS.with_borrow(|store| store.get(&loan_trx).cloned());
    if loan_transaction.is_none() {
        return Err(format!("Transaction not found"));
    }
    // for demonstration
    let loan_payment = 10000u64;

    let loan_trx = loan_transaction.unwrap();
    if caller() != loan_trx.owner {
        return Err(format!("You are not authorized"));
    }
    let bundle = BUNDLES.with_borrow(|store| store.get(&loan_trx.bundle_id).cloned());
    let collateral = COLLATERALS.with_borrow(|store| store.get(&loan_trx.collateral_id).cloned());

    if bundle.is_none() || collateral.is_none() {
        return Err(format!("Bundle/Collateral not Found"));
    }
    let bundle = bundle.unwrap();
    let collateral = collateral.unwrap();
    let lender_path = vec![bundle.lender.to_text().as_bytes().to_vec()];
    let borrower_path = vec![loan_trx.owner.to_text().as_bytes().to_vec()];
    let borrower_pk = get_ecdsa_public_key(borrower_path.clone()).await.public_key;
    let lender_addr_str = get_p2p_address_from_path(lender_path.clone()).await;
    let borrower_sddr_str = get_p2p_address_from_path(borrower_path.clone()).await;
    let borrowe_btc_addr = Address::from_str(&borrower_sddr_str)
        .unwrap()
        .require_network(transform_network(BitcoinNetwork::Regtest))
        .unwrap();
    let lender_btc_addr = Address::from_str(&lender_addr_str)
        .unwrap()
        .require_network(transform_network(BitcoinNetwork::Regtest))
        .unwrap();
    let user_addr = deposit_address().await;
    let (utxo_resp,) =
        ic_cdk::api::management_canister::bitcoin::bitcoin_get_utxos(GetUtxosRequest {
            address: user_addr,
            network: BitcoinNetwork::Regtest,
            filter: None,
        })
        .await
        .unwrap();
    let fee_byte = get_fee_per_byte(BitcoinNetwork::Regtest).await;

    let transaction = build_p2pkh_spend_tx(
        &borrower_pk,
        &borrowe_btc_addr,
        &utxo_resp.utxos,
        &lender_btc_addr,
        loan_payment,
        fee_byte,
    )
    .await?;

    let signed_trx = ecdsa_sign_transaction(
        &borrower_pk,
        &borrowe_btc_addr,
        transaction,
        "dfx_test_key".to_string(),
        borrower_path,
        ecdsa_api::get_ecdsa_signature,
    )
    .await;
    let txid = signed_trx.compute_txid().to_string();
    let signed_bytes = serialize(&signed_trx);
    send_transaction(BitcoinNetwork::Regtest, signed_bytes).await;

    let rune_trx_builder = RuneTransactionBuilder::new();

    let rune_param = RuneTransferParams {
        amount: collateral.amount,
        recipient_address: borrowe_btc_addr.clone(),
        fee_rate: fee_byte,
        rune_id: collateral.rune_id.clone()
    };
    let (collateral_utxo_resp,) =
        ic_cdk::api::management_canister::bitcoin::bitcoin_get_utxos(GetUtxosRequest {
            address: collateral.address.clone(),
            network: BitcoinNetwork::Regtest,
            filter: None,
        })
        .await
        .unwrap();

    let rune_trx = rune_trx_builder
        .build_unsigned_transfer(&rune_param, &collateral_utxo_resp.utxos, &borrowe_btc_addr, collateral.amount)
        .await
        .unwrap();
    let collateral_path = vec![hex::decode(&collateral.id).unwrap()];
    let collateral_pk = get_ecdsa_public_key(collateral_path.clone())
        .await
        .public_key;
    let collateral_btc_addr = Address::from_str(&collateral.address)
        .unwrap()
        .require_network(transform_network(BitcoinNetwork::Regtest))
        .unwrap();

    let signed_rune_trx = ecdsa_sign_transaction(
        &collateral_pk,
        &collateral_btc_addr,
        rune_trx,
        format!("dfx_test_key"),
        collateral_path,
        ecdsa_api::get_ecdsa_signature,
    )
    .await;
    let tx_bytes = serialize(&signed_rune_trx);

    send_transaction(BitcoinNetwork::Regtest, tx_bytes).await;

    let rune_txn_id = signed_rune_trx.compute_txid().to_string();

    LOAN_TRANSACTIONS.with_borrow_mut(|store| {
        let loan = store.get_mut(&loan_trx.id).unwrap();
        loan.loan_status = LoanStatus::PAID_BACK(txid.clone())
    });

    Ok((txid, rune_txn_id))
}

async fn get_rune_unit_price(rune_id: String) -> u64 {
    return 100u64;
}

// Get raw transaction hex from Bitcoin RPC
async fn get_raw_transaction(txid: &str) -> Result<String, String> {
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "getrawtransaction",
        "params": [txid, false]  // false for raw hex
    });

    let auth = format!("{}:{}", RPC_USER, RPC_PASS);
    let auth_header = format!("Basic {}", BASE64.encode(auth.as_bytes()));

    let headers = vec![
        HttpHeader {
            name: "Content-Type".to_string(),
            value: "application/json".to_string(),
        },
        HttpHeader {
            name: "Authorization".to_string(),
            value: auth_header,
        },
    ];

    let request = CanisterHttpRequestArgument {
        url: BTC_RPC_URL.to_string(),
        method: HttpMethod::POST,
        body: Some(request_body.to_string().into_bytes()),
        max_response_bytes: Some(2048),
        transform: Some(TransformContext::from_name(
            "http_transform".to_string(),
            vec![],
        )),
        headers,
    };

    match ic_cdk::api::management_canister::http_request::http_request(request, 10_000_000_000)
        .await
    {
        Ok((response,)) => parse_raw_tx_response(&response),
        Err((_, m)) => Err(format!("HTTP request failed: {:?}", m)),
    }
}


// Get raw transaction hex from Bitcoin RPC
async fn get_transaction_info(txid: &str) -> Result<Value, String> {
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "getrawtransaction",
        "params": [txid, true]  // true for verbose output
    });

    let auth = format!("{}:{}", RPC_USER, RPC_PASS);
    let auth_header = format!("Basic {}", BASE64.encode(auth.as_bytes()));

    let headers = vec![
        HttpHeader {
            name: "Content-Type".to_string(),
            value: "application/json".to_string(),
        },
        HttpHeader {
            name: "Authorization".to_string(),
            value: auth_header,
        },
    ];

    let request = CanisterHttpRequestArgument {
        url: BTC_RPC_URL.to_string(),
        method: HttpMethod::POST,
        body: Some(request_body.to_string().into_bytes()),
        max_response_bytes: Some(2048),
        transform: Some(TransformContext::from_name(
            "http_transform".to_string(),
            vec![],
        )),
        headers,
    };

    match ic_cdk::api::management_canister::http_request::http_request(request, 10_000_000_000)
        .await
    {
        Ok((response,)) => {
            if response.status != Nat::from(200u32) {
                return Err(format!("HTTP error: {}", response.status));
            }

            let response_body = String::from_utf8(response.body)
                .map_err(|e| format!("Failed to parse response body: {}", e))?;

            let json: Value = serde_json::from_str(&response_body)
                .map_err(|e| format!("Failed to parse JSON: {}", e))?;

            // Check for RPC error
            if let Some(error) = json.get("error") {
                if !error.is_null() {
                    return Err(format!("RPC error: {:?}", error));
                }
            }

            // Get result
            json.get("result")
                .cloned()
                .ok_or_else(|| "No result field in response".to_string())
        }
        Err((_, m)) => Err(format!("HTTP request failed: {:?}", m)),
    }
}

fn parse_raw_tx_response(response: &HttpResponse) -> Result<String, String> {
    if response.status != Nat::from(200u32) {
        return Err(format!("HTTP error: {}", response.status));
    }

    let response_str =
        String::from_utf8(response.body.clone()).map_err(|e| format!("Invalid UTF-8: {}", e))?;

    let json_response: Value =
        serde_json::from_str(&response_str).map_err(|e| format!("JSON parse error: {}", e))?;

    // Check for RPC error
    if let Some(error) = json_response.get("error") {
        if !error.is_null() {
            return Err(format!("RPC error: {:?}", error));
        }
    }

    // Get result (raw transaction hex)
    json_response
        .get("result")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "No result in response".to_string())
}


#[update]
async fn test(txid : String) {
    let trx_hex = get_raw_transaction(&txid).await.unwrap();
    let trx : Transaction = deserialize_hex(&trx_hex).unwrap();
    let artifact_opt = Runestone::decipher(&trx);
    let a = artifact_opt.unwrap();
    match a {
    Artifact::Cenotaph(cenotaph) => {
        println!("A centaph")
    },
    Artifact::Runestone(runestone) => {
      for edict in runestone.edicts {
          ic_cdk::println!("Rune Id: {}: amount: {}", edict.id.block, edict.amount)
      }  
    },
        
    }
}

#[query]
fn get_colateral(id: String) -> std::option::Option<Collateral> {
    COLLATERALS.with_borrow(|store| store.get(&id).cloned())
}

#[query]
fn get_bundle(id: String) -> Option<Bundle> {
    BUNDLES.with_borrow(|store| store.get(&id).cloned())
}
#[query]
fn get_loan_transaction(id: String) -> Option<LoanTransaction> {
    LOAN_TRANSACTIONS.with_borrow(|store| store.get(&id).cloned())
}
#[query]
fn http_transform(args: TransformArgs) -> HttpResponse {
    HttpResponse {
        status: args.response.status,
        headers: vec![],
        body: args.response.body,
    }
}

// async fn fetc_btc_txn(txn_id : String) -> Transaction {

// }

fn is_not_anonymous() -> Result<(), String> {
    if caller() == Principal::anonymous() {
        return Err(format!("You are not authenticated!"));
    } else {
        return Ok(());
    }
}

// Converts a public key to a P2PKH address.
pub fn public_key_to_p2pkh_address(public_key: &[u8]) -> String {
    let public_key_type = bitcoin::PublicKey::from_slice(public_key).unwrap();

    let addr = bitcoin::Address::p2pkh(&public_key_type, Network::Regtest);

    addr.to_string()
}

fn nanoseconds_to_milliseconds(nanoseconds: u64) -> u64 {
    nanoseconds / 1_000_000
}

fn days_to_milliseconds(days: u64) -> u64 {
    const MILLIS_PER_DAY: u64 = 24 * 60 * 60 * 1000; // 24h * 60m * 60s * 1000ms
    days * MILLIS_PER_DAY
}

#[query]
#[candid_method(query)]
fn export_candid() -> String {
    ic_cdk::export_candid!();
    __export_service()
}
