//! In-memory NearBlocks API and ledger for cycle tests.

use super::*;
use crate::client::ListPage;
use crate::credit::CreditSource;
use chrono::NaiveDate;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

/// What the fake answers for one transaction.
#[derive(Debug, Clone)]
pub enum TxAnswer {
    Receipts(Vec<Value>),
    Status(u16),
    RateLimited,
    Network,
}

#[derive(Debug, Default)]
pub struct FakeApi {
    /// Pages served in order for `Newest`, then by cursor.
    pub pages: Mutex<VecDeque<ListPage>>,
    pub by_cursor: Mutex<HashMap<String, ListPage>>,
    pub by_before_ns: Mutex<HashMap<u64, ListPage>>,
    pub txns: Mutex<HashMap<String, TxAnswer>>,
    /// Errors served for a cursor instead of a page (`Receipts` is ignored).
    pub list_errors: Mutex<HashMap<String, TxAnswer>>,
    pub list_calls: Mutex<Vec<ListFrom>>,
    pub txn_calls: Mutex<Vec<String>>,
}

impl FakeApi {
    pub fn with_pages(pages: Vec<ListPage>) -> Self {
        Self {
            pages: Mutex::new(pages.into()),
            ..Default::default()
        }
    }

    pub fn answer(&self, tx: &str, answer: TxAnswer) {
        self.txns.lock().unwrap().insert(tx.to_owned(), answer);
    }

    pub fn fail_cursor(&self, cursor: &str, answer: TxAnswer) {
        self.list_errors
            .lock()
            .unwrap()
            .insert(cursor.to_owned(), answer);
    }
}

fn answer_error(answer: &TxAnswer) -> Option<FetchError> {
    match answer {
        TxAnswer::Receipts(_) => None,
        TxAnswer::Status(code) => Some(FetchError::Http(
            reqwest::StatusCode::from_u16(*code).unwrap(),
        )),
        TxAnswer::RateLimited => Some(FetchError::RateLimited),
        TxAnswer::Network => Some(FetchError::Network("connection refused".to_owned())),
    }
}

fn take_credit(credits: &dyn CreditSource) -> Result<(), FetchError> {
    credits
        .take(Utc::now().date_naive())
        .map(drop)
        .ok_or(FetchError::NoCredit)
}

impl NearBlocksApi for FakeApi {
    async fn list_receipts(
        &self,
        from: &ListFrom,
        credits: &dyn CreditSource,
    ) -> Result<ListPage, FetchError> {
        take_credit(credits)?;
        self.list_calls.lock().unwrap().push(from.clone());
        if let ListFrom::Cursor(c) = from
            && let Some(err) = self
                .list_errors
                .lock()
                .unwrap()
                .get(c)
                .and_then(answer_error)
        {
            return Err(err);
        }
        let page = match from {
            ListFrom::Newest => self.pages.lock().unwrap().pop_front(),
            ListFrom::Cursor(c) => self
                .by_cursor
                .lock()
                .unwrap()
                .get(c)
                .cloned()
                .or_else(|| self.pages.lock().unwrap().pop_front()),
            ListFrom::BeforeNs(ns) => self.by_before_ns.lock().unwrap().get(ns).cloned(),
        };
        Ok(page.unwrap_or(ListPage {
            items: vec![],
            next_cursor: None,
        }))
    }

    async fn txn_receipts(
        &self,
        tx_hash: &TxHash,
        credits: &dyn CreditSource,
    ) -> Result<Vec<Value>, FetchError> {
        take_credit(credits)?;
        self.txn_calls
            .lock()
            .unwrap()
            .push(tx_hash.as_str().to_owned());
        match self.txns.lock().unwrap().get(tx_hash.as_str()) {
            Some(TxAnswer::Receipts(v)) => Ok(v.clone()),
            Some(other) => Err(answer_error(other).expect("non-receipt answer is an error")),
            None => Ok(vec![]),
        }
    }
}

/// Ledger that records charges in memory (the DB ledger is exercised by
/// its own tests).
#[derive(Debug, Default, Clone)]
pub struct MemLedger {
    pub charges: Arc<Mutex<Vec<(NaiveDate, u32)>>>,
}

impl MemLedger {
    pub fn total(&self) -> u64 {
        self.charges
            .lock()
            .unwrap()
            .iter()
            .map(|(_, n)| u64::from(*n))
            .sum()
    }
}

impl LedgerStore for MemLedger {
    async fn add_requests(&self, day: NaiveDate, requests: u32) -> Result<u64> {
        let mut c = self.charges.lock().unwrap();
        c.push((day, requests));
        Ok(c.iter()
            .filter(|(d, _)| *d == day)
            .map(|(_, n)| u64::from(*n))
            .sum())
    }
}

/// Deterministic base58 ids for synthetic list items.
pub fn hash(n: u32) -> String {
    let mut bytes = [0u8; 32];
    bytes[..4].copy_from_slice(&n.to_be_bytes());
    bytes[31] = 1;
    bs58::encode(bytes).into_string()
}

/// A list item for a successful `swap` receipt `n` of tx `tx` at `ts_ns`.
pub fn list_item(n: u32, tx: u32, ts_ns: u64, ok: bool) -> Value {
    json!({
        "receipt_id": hash(1_000_000 + n),
        "transaction_hash": hash(tx),
        "predecessor_account_id": format!("sender{n}.near"),
        "receiver_account_id": parse::REF_CONTRACT,
        "actions": [{"action": "FUNCTION_CALL", "method": "swap"}],
        "outcome": {"status": ok},
        "block": {"block_height": 221_000_000u64 + u64::from(n), "block_timestamp": ts_ns.to_string()}
    })
}

/// The real `swap_single` receipt tree, re-labelled with `tx` and `ts_ns`
/// so it matches a synthetic list item.
pub fn swap_tree(tx: u32, receipt_n: u32, ts_ns: u64) -> Vec<Value> {
    let mut v: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/enrich/swap_single.json"
    )))
    .unwrap();
    v["_tx_hash"] = Value::String(hash(tx));
    v["receipt_id"] = Value::String(hash(1_000_000 + receipt_n));
    v["block"]["block_timestamp"] = Value::String(ts_ns.to_string());
    vec![v]
}

pub fn ns(secs: i64) -> u64 {
    u64::try_from(secs).unwrap() * 1_000_000_000
}

pub fn config() -> CycleConfig {
    CycleConfig {
        retention_days: 90,
        operator_cap: None,
        workers: 2,
        run_deadline: Duration::from_secs(600),
        gap_time: Duration::from_secs(600),
        fwd_max_pages: FWD_MAX_PAGES,
        enrich_window: ENRICH_WINDOW,
        max_enrich_attempts: 2,
    }
}

pub fn tracker(ledger: MemLedger, cap: Option<u64>) -> BudgetTracker<MemLedger> {
    BudgetTracker::new(
        ledger,
        &budget::STANDARD,
        budget::Ledger::new(),
        crate::accounting::StreamStats::default(),
        cap,
        Utc::now().date_naive(),
    )
}
