use super::*;

fn parts(methods: Vec<&str>) -> RefReceiptParts {
    RefReceiptParts {
        receipt_id: receipt_id(1),
        tx_hash: tx_hash(1),
        block_height: 1,
        block_time: ts(0),
        predecessor: "alice.near".parse().unwrap(),
        methods: methods.into_iter().map(str::to_owned).collect(),
        ok: true,
    }
}

#[test]
fn accepts_one_to_sixteen_methods() {
    let r = RefReceipt::new(parts(vec!["swap", "withdraw"])).unwrap();
    assert_eq!(r.methods(), ["swap", "withdraw"]);
    let sixteen: Vec<&str> = vec!["swap"; MAX_METHODS];
    assert!(RefReceipt::new(parts(sixteen)).is_ok());
}

#[test]
fn rejects_empty_and_too_many_methods() {
    assert_eq!(
        RefReceipt::new(parts(vec![])).unwrap_err(),
        RefReceiptError::NoMethods
    );
    let too_many: Vec<&str> = vec!["swap"; MAX_METHODS + 1];
    assert_eq!(
        RefReceipt::new(parts(too_many)).unwrap_err(),
        RefReceiptError::TooManyMethods(MAX_METHODS + 1)
    );
}

#[test]
fn rejects_empty_method_name() {
    assert_eq!(
        RefReceipt::new(parts(vec!["swap", ""])).unwrap_err(),
        RefReceiptError::EmptyMethod
    );
}

#[test]
fn db_row_with_null_method_element_is_an_error() {
    let row = DbRefReceipt {
        receipt_id: receipt_id(1).as_str().to_owned(),
        tx_hash: tx_hash(1).as_str().to_owned(),
        block_height: 1,
        block_time: ts(0),
        predecessor: "alice.near".to_owned(),
        methods: vec![Some("swap".to_owned()), None],
        ok: true,
    };
    let err = RefReceipt::from_db(row).unwrap_err();
    assert!(
        format!("{err:#}").contains("NULL element in methods"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn db_row_with_invalid_ids_is_an_error() {
    let mut row = DbRefReceipt {
        receipt_id: "not-base58-0OIl".to_owned(),
        tx_hash: tx_hash(1).as_str().to_owned(),
        block_height: 1,
        block_time: ts(0),
        predecessor: "alice.near".to_owned(),
        methods: vec![Some("swap".to_owned())],
        ok: true,
    };
    assert!(RefReceipt::from_db(row.clone()).is_err());
    row.receipt_id = receipt_id(1).as_str().to_owned();
    row.predecessor = "Not A Valid Account".to_owned();
    assert!(RefReceipt::from_db(row.clone()).is_err());
    row.predecessor = "alice.near".to_owned();
    row.block_height = -1;
    assert!(RefReceipt::from_db(row).is_err());
}

#[test]
fn prepare_rows_dedupes_receipts_and_keeps_earliest_tx_time() {
    let receipts = vec![
        make_receipt(1, 10, 300, true),
        make_receipt(1, 10, 300, true),  // duplicate receipt
        make_receipt(2, 10, 100, true),  // same tx, earlier
        make_receipt(3, 11, 200, false), // failed: no frame row
        make_receipt(4, 12, 400, true),
    ];
    let (rows, txs) = prepare_rows(&receipts).unwrap();
    assert_eq!(rows.len(), 4);
    let txs: Vec<(String, NaiveDateTime)> =
        txs.into_iter().map(|t| (t.tx_hash, t.block_time)).collect();
    let mut expected = vec![
        (tx_hash(10).as_str().to_owned(), ts(100)),
        (tx_hash(12).as_str().to_owned(), ts(400)),
    ];
    expected.sort();
    assert_eq!(txs, expected);
}
