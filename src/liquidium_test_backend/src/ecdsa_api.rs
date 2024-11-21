use ic_cdk::api::management_canister::ecdsa::{self, sign_with_ecdsa, EcdsaCurve, EcdsaKeyId, EcdsaPublicKeyArgument, EcdsaPublicKeyResponse, SignWithEcdsaArgument};

pub async fn get_ecdsa_public_key(derivation_path : Vec<Vec<u8>>) -> EcdsaPublicKeyResponse {
    let (ecdsa_response,) = ecdsa::ecdsa_public_key(EcdsaPublicKeyArgument {
        canister_id: None,
        derivation_path,
        key_id: EcdsaKeyId {
            curve: ecdsa::EcdsaCurve::Secp256k1,
            name: "dfx_test_key".to_string(),
        },
    })
    .await
    .unwrap();

    return ecdsa_response;
}

pub async fn get_ecdsa_signature(
    key_name: String,
    derivation_path: Vec<Vec<u8>>,
    message_hash: Vec<u8>,
) -> Vec<u8> {
    let key_id = EcdsaKeyId {
        curve: EcdsaCurve::Secp256k1,
        name: key_name,
    };

    let res = sign_with_ecdsa(SignWithEcdsaArgument {
        message_hash,
        derivation_path,
        key_id,
    })
    .await;

    res.unwrap().0.signature
}