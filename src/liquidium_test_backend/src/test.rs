use candid::{encode_one, Decode, Principal};
use ic_cdk::api::management_canister::main::create_canister;
use pocket_ic::PocketIc;

const TEST_WASM : &[u8] = include_bytes!("../../bin/liquidium_test_backend.wasm");

const test_account: &str = "t2lrx-mmt4s-tqhf7-nizyn-22rtl-ze2e4-dsypi-2yiig-l3wfk-afar6-4ae";
const user_2: &str = "gjsb5-aob2b-zf7yv-7olwn-zkz6u-bpjjl-tjix4-h5l5r-fgc4u-tizdh-nae";

fn create_pocket() -> (PocketIc, Principal) {
    let pic = PocketIc::new();
    let user = Principal::from_text(test_account).unwrap();
    let backend_id = pic.create_canister();
    
    pic.add_cycles(backend_id, 300_000_000_000_000);
    pic.install_canister(backend_id, TEST_WASM.to_vec(), vec![], None);
    (pic, backend_id)
}
#[test]
fn get_deposit_address() {
    let (pic, backend_id) = create_pocket();
    let test_user = Principal::from_text(test_account).unwrap();
    let wasmresult = pic.update_call(backend_id, test_user, "deposit_address", encode_one(()).unwrap()).unwrap();
    match wasmresult {
        pocket_ic::WasmResult::Reply(vec) => {
            let address = Decode!(&vec, String).unwrap();
            println!("BTC address: {}", address)
        },
        pocket_ic::WasmResult::Reject(_) => {
            panic!()
        },
    }
}