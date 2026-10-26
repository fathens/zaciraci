use super::*;

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/enrich/",
            $name,
            ".json"
        ))
    };
}

/// Real mainnet transaction receipt trees captured from NearBlocks. Each
/// file is one top-level receipt object as written by the Python
/// collector (`_tx_hash` carries the transaction hash).
pub fn enrich_fixture(name: &str) -> (TxHash, Vec<Value>) {
    let text = match name {
        "swap_single" => fixture!("swap_single"),
        "swap_chain" => fixture!("swap_chain"),
        "split" => fixture!("split"),
        "swap_withdraw" => fixture!("swap_withdraw"),
        "ft_basic" => fixture!("ft_basic"),
        "ft_near_withdraw" => fixture!("ft_near_withdraw"),
        "ft_split" => fixture!("ft_split"),
        "failed" => fixture!("failed"),
        "ft_unparseable_msg" => fixture!("ft_unparseable_msg"),
        other => panic!("unknown fixture {other}"),
    };
    let value: Value = serde_json::from_str(text).unwrap();
    let tx_hash = TxHash::new(value["_tx_hash"].as_str().unwrap()).unwrap();
    (tx_hash, vec![value])
}

pub fn parse_fixture(name: &str) -> ParsedTx {
    let (tx_hash, roots) = enrich_fixture(name);
    parse_tx_receipts(&tx_hash, &roots).unwrap()
}

pub fn list_fixture() -> Vec<Value> {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/list_items.json"
    )))
    .unwrap()
}

pub fn units(v: u128) -> TokenSmallestUnits {
    TokenSmallestUnits::from_u128(v)
}

pub fn tok(name: &str) -> TokenAccount {
    name.parse().unwrap()
}

/// Mutable access to the first Ref receipt found in the tree (depth-first).
pub fn ref_receipt_mut(root: &mut Value) -> &mut Value {
    fn find(v: &mut Value) -> Option<&mut Value> {
        if v["receiver_account_id"].as_str() == Some(REF_CONTRACT)
            && v["outcome"]["status"] == Value::Bool(true)
        {
            return Some(v);
        }
        let children = v.get_mut("receipts")?.as_array_mut()?;
        for child in children {
            if let Some(found) = find(child) {
                return Some(found);
            }
        }
        None
    }
    find(root).expect("fixture has a successful Ref receipt")
}

pub fn b64_json(v: &str) -> String {
    BASE64.encode(v.as_bytes())
}
