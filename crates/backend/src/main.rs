//! Backend binary orchestrator.
//!
//! **CRITICAL: single-process invariant** — この backend は `ROOT_ACCOUNT_ID` ごとに
//! **単一プロセス/コンテナ**で起動する前提。REF Finance の storage 管理
//! (`ensure_ref_storage_setup`) は同一プロセス内の `static REF_STORAGE_LOCKS` に依存して
//! 並行実行を直列化しており、同一ウォレットを握る別プロセスが同時起動すると二重 initial
//! deposit や二重 top-up の race で `max_top_up` cap を超えた NEAR 流出が起こりうる。
//! deployment は rolling restart 等で必ず「旧プロセス停止 → 新プロセス起動」の順を守ること。
//!
//! 詳細は `crates/blockchain/src/ref_finance/storage.rs` の `REF_STORAGE_LOCKS` doc を参照。

#![deny(warnings)]

use common::config::ConfigResolver;
use logging::*;

#[tokio::main]
async fn main() {
    let log = DEFAULT.new(o!("function" => "main"));
    info!(log, "Starting up");

    let cfg = ConfigResolver;
    let startup = common::config::startup::get();

    // DB から設定をロード（失敗時はスキップ）
    if let Err(e) = persistence::config_store::reload_to_config(&startup.instance_id).await {
        warn!(log, "failed to load config from DB, continuing with env/TOML"; "error" => %e);
    }

    let base = blockchain::wallet::new_wallet().derive(0).unwrap();
    let account_zero = base.derive(0).unwrap();
    info!(log, "Account 0 created"; "pubkey" => %account_zero.pub_base58());

    tokio::spawn(trade::run(cfg));
    tokio::spawn(arbitrage::run(cfg));
    tokio::spawn(persistence::maintenance::run(cfg));
    {
        let log = log.clone();
        tokio::spawn(async move {
            if let Err(e) = web::serve(50051).await {
                error!(log, "web server exited"; "error" => %e);
            }
        });
    }
    tokio::signal::ctrl_c().await.ok();
}
