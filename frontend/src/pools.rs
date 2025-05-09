use bigdecimal::BigDecimal;
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;
use zaciraci_common::{
    ApiResponse,
    pools::{PoolId, PoolRecordsRequest, TradeRequest},
    types::NearUnit,
};

/// メインビューコンポーネント
#[component]
pub fn view() -> Element {
    rsx! {
        div { class: "pools-view",
            trade_estimates_view {}
            pool_records_view {}
        }
    }
}

/// トレード見積もりセクションのコンポーネント
#[component]
fn trade_estimates_view() -> Element {
    rsx! {
        h2 { "Trade Estimates" }
        div { class: "trade-estimates-container",
            style: "display: grid; grid-template-columns: 1fr 1fr; gap: 2rem;",
            // A
            estimate_trade_view {
                id: "a",
                default_token_in: Some("wrap.near".to_string()),
                default_token_out: None,
                default_amount: Some("1".to_string()),
            }

            // B
            estimate_trade_view {
                id: "b",
                default_token_in: Some("wrap.near".to_string()),
                default_token_out: None,
                default_amount: Some("1".to_string()),
            }

            // C
            estimate_trade_view {
                id: "c",
                default_token_in: Some("wrap.near".to_string()),
                default_token_out: None,
                default_amount: Some("1".to_string()),
            }

            // D
            estimate_trade_view {
                id: "d",
                default_token_in: Some("wrap.near".to_string()),
                default_token_out: None,
                default_amount: Some("1".to_string()),
            }
        }
    }
}

/// トレード見積もりコンポーネント
#[component]
fn estimate_trade_view(
    id: &'static str, // コンポーネントの一意識別子
    default_token_in: Option<String>,
    default_token_out: Option<String>,
    default_amount: Option<String>,
) -> Element {
    let client = use_signal(crate::server_api::get_client);

    // 現在時刻をデフォルト値として使用
    let now = chrono::Local::now()
        .naive_utc()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();

    // コンポーネント内で状態を定義
    let mut timestamp = use_signal(|| now);
    let mut token_in = use_signal(|| default_token_in.unwrap_or_else(|| "wrap.near".to_string()));
    let mut token_out = use_signal(|| default_token_out.unwrap_or_else(|| "".to_string()));
    let mut amount_in = use_signal(|| default_amount.unwrap_or_else(|| "1".to_string()));
    let mut amount_unit = use_signal(|| NearUnit::Near.to_string());
    let mut amount_out = use_signal(|| "0".to_string());
    let mut loading = use_signal(|| "".to_string());

    rsx! {
        div { class: "estimate_trade-container",
            div { class: "timestamp",
                input { type: "datetime-local", name: "timestamp_{id}", value: "{timestamp}",
                    oninput: move |e| timestamp.set(e.value())
                }
            }
            div { class: "token_in",
                input { type: "text", name: "token_in_{id}", value: "{token_in}", size: "30",
                    oninput: move |e| token_in.set(e.value())
                }
            }
            div { class: "token_out",
                input { type: "text", name: "token_out_{id}", value: "{token_out}", size: "30",
                    oninput: move |e| token_out.set(e.value())
                }
            }
            div { class: "amount",
                div { class: "amount_in",
                    input { type: "text", name: "amount_in_{id}", value: "{amount_in}", size: "30",
                        oninput: move |e| amount_in.set(e.value())
                    }
                    select {
                        name: "amount_unit_{id}",
                        value: "{amount_unit.to_string()}",
                        onchange: move |e| amount_unit.set(e.value()),
                        option { value: "NEAR", "NEAR" }
                        option { value: "mNEAR", "mNEAR" }
                        option { value: "yNEAR", "yNEAR" }
                    }
                }
                div { class: "amount_out",
                    input { type: "text", name: "amount_out_{id}", value: "{amount_out}", size: "30",
                        oninput: move |e| amount_out.set(e.value())
                    }
                }
            }
            div { class: "button-with-loading",
                button { class: "btn btn-primary",
                    onclick: move |_| {
                        spawn_local({
                            let timestamp = timestamp.read().clone();
                            let token_in = token_in.read().clone();
                            let token_out = token_out.read().clone();
                            let amount_in = amount_in.read().clone();
                            let amount_unit = amount_unit.read().clone();
                            let mut amount_out = amount_out;
                            let mut loading = loading;
                            let client = client.read().clone();

                            async move {
                                let unit: NearUnit = amount_unit.parse().unwrap();
                                let amount_in_value = unit.to_yocto(amount_in.parse().unwrap());
                                amount_out.set("".to_string());
                                loading.set("Loading...".to_string());
                                let res = client.pools.estimate_trade(TradeRequest {
                                    timestamp: timestamp.parse().unwrap(),
                                    token_in: token_in.parse().unwrap(),
                                    token_out: token_out.parse().unwrap(),
                                    amount_in: amount_in_value,
                                }).await.unwrap();
                                match res {
                                    ApiResponse::Success(res) => {
                                        loading.set("".to_string());
                                        let amount_out_value = unit.from_yocto(res.amount_out);
                                        amount_out.set(format_amount(amount_out_value));
                                    }
                                    ApiResponse::Error(e) => {
                                        loading.set(e.to_string());
                                    }
                                }
                            }
                        });
                    },
                    "Estimate"
                }
                span { class: "loading", "{loading}" }
            }
        }
    }
}

/// プールレコードセクションのコンポーネント
#[component]
fn pool_records_view() -> Element {
    let client = use_signal(crate::server_api::get_client);

    let mut pools_timestamp = use_signal(|| {
        chrono::Local::now()
            .naive_utc()
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string()
    });
    let mut pool_ids = use_signal(|| "".to_string());
    let mut pools_loading = use_signal(|| "".to_string());
    let mut pools = use_signal(|| "".to_string());

    rsx! {
        h2 { "Pool Records" }
        div { class: "pool_records-container",
            div { class: "pool_records",
                div { class: "pool_records_input",
                    textarea { name: "pool_ids", value: "{pool_ids}", rows: "10", cols: "10",
                        oninput: move |e| pool_ids.set(e.value())
                    }
                }
                div { class: "timestamp",
                    input { type: "datetime-local", name: "pools_timestamp", value: "{pools_timestamp}",
                        oninput: move |e| pools_timestamp.set(e.value())
                    }
                }
                div { class: "button-with-loading",
                    button { class: "btn btn-primary",
                        onclick: move |_| {
                            spawn_local({
                                let pools_timestamp = pools_timestamp.read().clone();
                                let pool_ids = pool_ids.read().clone();
                                let mut pools_loading = pools_loading;
                                let mut pools = pools;
                                let client = client.read().clone();

                                async move {
                                    pools_loading.set("Loading...".to_string());
                                    pools.set("".to_string());
                                    let mut ids = vec![];
                                    for s in pool_ids.split_whitespace().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                                        match s.parse::<u32>() {
                                            Ok(id) => ids.push(PoolId(id)),
                                            Err(e) => {
                                                pools_loading.set(format!("Failed to parse pool ID '{}': {}", s, e));
                                                return;
                                            }
                                        }
                                    }
                                    if ids.is_empty() {
                                        pools_loading.set("No valid pool IDs provided".to_string());
                                        return;
                                    }
                                    ids.sort();
                                    ids.dedup();
                                    let res = client.pools.get_pool_records(PoolRecordsRequest {
                                        timestamp: pools_timestamp.parse().unwrap(),
                                        pool_ids: ids,
                                    }).await.unwrap();
                                    match res {
                                        ApiResponse::Success(res) => {
                                            pools_loading.set("".to_string());
                                            pools.set(serde_json::to_string_pretty(&res.pools).unwrap());
                                        }
                                        ApiResponse::Error(e) => {
                                            pools_loading.set(e.to_string());
                                        }
                                    }
                                }
                            });
                        },
                        "Get"
                    }
                    span { class: "loading", "{pools_loading}" }
                }
                div { class: "pools",
                    textarea { readonly: true, rows: "20", cols: "80", "{pools}" }
                }
            }
        }
    }
}

/// BigDecimal 値をフォーマットする関数
fn format_amount(amount: BigDecimal) -> String {
    format!("{:.24}", amount)
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDateTime;

    #[test]
    fn test_native_time() {
        let now = chrono::Local::now();
        let s = now.naive_utc().format("%Y-%m-%dT%H:%M:%S").to_string();
        let js = format!("\"{s}\"");
        let nt: NaiveDateTime = serde_json::from_slice(js.as_bytes()).unwrap();
        assert_eq!(s, nt.format("%Y-%m-%dT%H:%M:%S").to_string());
    }
}
