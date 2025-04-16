use bigdecimal::BigDecimal;
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;
use zaciraci_common::{
    ApiResponse,
    pools::{PoolId, PoolRecordsRequest, TradeRequest},
    types::NearUnit,
};

#[component]
pub fn view() -> Element {
    let client = use_signal(crate::server_api::get_client);

    let now = chrono::Local::now()
        .naive_utc()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let mut timestamp_a = use_signal(|| now.clone());
    let mut timestamp_b = use_signal(|| now.clone());
    let mut timestamp_c = use_signal(|| now.clone());
    let mut timestamp_d = use_signal(|| now.clone());

    let mut amount_unit_a = use_signal(|| NearUnit::Near.to_string());
    let mut amount_unit_b = use_signal(|| NearUnit::Near.to_string());
    let mut amount_unit_c = use_signal(|| NearUnit::Near.to_string());
    let mut amount_unit_d = use_signal(|| NearUnit::Near.to_string());

    let mut amount_in_a = use_signal(|| "1".to_string());
    let mut amount_in_b = use_signal(|| "1".to_string());
    let mut amount_in_c = use_signal(|| "1".to_string());
    let mut amount_in_d = use_signal(|| "1".to_string());

    let mut amount_out_a = use_signal(|| "0".to_string());
    let mut amount_out_b = use_signal(|| "0".to_string());
    let mut amount_out_c = use_signal(|| "0".to_string());
    let mut amount_out_d = use_signal(|| "0".to_string());

    let mut token_in_a = use_signal(|| "wrap.near".to_string());
    let mut token_in_b = use_signal(|| "wrap.near".to_string());
    let mut token_in_c = use_signal(|| "wrap.near".to_string());
    let mut token_in_d = use_signal(|| "wrap.near".to_string());

    let mut token_out_a = use_signal(|| "".to_string());
    let mut token_out_b = use_signal(|| "".to_string());
    let mut token_out_c = use_signal(|| "".to_string());
    let mut token_out_d = use_signal(|| "".to_string());

    let mut loading_a = use_signal(|| "".to_string());
    let mut loading_b = use_signal(|| "".to_string());
    let mut loading_c = use_signal(|| "".to_string());
    let mut loading_d = use_signal(|| "".to_string());

    fn format_amount(amount: BigDecimal) -> String {
        format!("{:.24}", amount)
    }

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
        div { class: "pools-view",
            h2 { "Trade Estimates" }
            div { class: "trade-estimates-container",
                style: "display: grid; grid-template-columns: 1fr 1fr; gap: 2rem;",
                // A
                div { class: "estimate_trade-container",
                    div { class: "timestamp",
                        input { type: "datetime-local", name: "timestamp_a", value: "{timestamp_a}",
                            oninput: move |e| timestamp_a.set(e.value())
                        }
                    }
                    div { class: "token_in",
                        input { type: "text", name: "token_in_a", value: "{token_in_a}", size: "30",
                            oninput: move |e| token_in_a.set(e.value())
                        }
                    }
                    div { class: "token_out",
                        input { type: "text", name: "token_out_a", value: "{token_out_a}", size: "30",
                            oninput: move |e| token_out_a.set(e.value())
                        }
                    }
                    div { class: "amount",
                        div { class: "amount_in",
                            input { type: "text", name: "amount_in_a", value: "{amount_in_a}", size: "30",
                                oninput: move |e| amount_in_a.set(e.value())
                            }
                            select {
                                name: "amount_unit_a",
                                value: "{amount_unit_a.to_string()}",
                                onchange: move |e| amount_unit_a.set(e.value()),
                                option { value: "NEAR", "NEAR" }
                                option { value: "mNEAR", "mNEAR" }
                                option { value: "yNEAR", "yNEAR" }
                            }
                        }
                        div { class: "amount_out",
                            input { type: "text", name: "amount_out_a", value: "{amount_out_a}", size: "30",
                                oninput: move |e| amount_out_a.set(e.value())
                            }
                        }
                    }
                    div { class: "button-with-loading",
                        button { class: "btn btn-primary",
                            onclick: move |_| {
                                spawn_local(async move {
                                    let unit: NearUnit = amount_unit_a().parse().unwrap();
                                    let amount_in = unit.to_yocto(amount_in_a().parse().unwrap());
                                    amount_out_a.set("".to_string());
                                    loading_a.set("Loading...".to_string());
                                    let res = client().pools.estimate_trade(TradeRequest {
                                        timestamp: timestamp_a().parse().unwrap(),
                                        token_in: token_in_a().parse().unwrap(),
                                        token_out: token_out_a().parse().unwrap(),
                                        amount_in,
                                    }).await.unwrap();
                                    match res {
                                        ApiResponse::Success(res) => {
                                            loading_a.set("".to_string());
                                            let amount_out = unit.from_yocto(res.amount_out);
                                            amount_out_a.set(format_amount(amount_out));
                                        }
                                        ApiResponse::Error(e) => {
                                            loading_a.set(e.to_string());
                                        }
                                    }
                                });
                            },
                            "Estimate"
                        }
                        span { class: "loading", "{loading_a}" }
                    }
                }

                // B
                div { class: "estimate_trade-container",
                    div { class: "timestamp",
                        input { type: "datetime-local", name: "timestamp_b", value: "{timestamp_b}",
                            oninput: move |e| timestamp_b.set(e.value())
                        }
                    }
                    div { class: "token_in",
                        input { type: "text", name: "token_in_b", value: "{token_in_b}", size: "30",
                            oninput: move |e| token_in_b.set(e.value())
                        }
                    }
                    div { class: "token_out",
                        input { type: "text", name: "token_out_b", value: "{token_out_b}", size: "30",
                            oninput: move |e| token_out_b.set(e.value())
                        }
                    }
                    div { class: "amount",
                        div { class: "amount_in",
                            input { type: "text", name: "amount_in_b", value: "{amount_in_b}", size: "30",
                                oninput: move |e| amount_in_b.set(e.value())
                            }
                            select {
                                name: "amount_unit_b",
                                value: "{amount_unit_b.to_string()}",
                                onchange: move |e| amount_unit_b.set(e.value()),
                                option { value: "NEAR", "NEAR" }
                                option { value: "mNEAR", "mNEAR" }
                                option { value: "yNEAR", "yNEAR" }
                            }
                        }
                        div { class: "amount_out",
                            input { type: "text", name: "amount_out_b", value: "{amount_out_b}", size: "30",
                                oninput: move |e| amount_out_b.set(e.value())
                            }
                        }
                    }
                    div { class: "button-with-loading",
                        button { class: "btn btn-primary",
                            onclick: move |_| {
                                spawn_local(async move {
                                    let unit: NearUnit = amount_unit_b().parse().unwrap();
                                    let amount_in = unit.to_yocto(amount_in_b().parse().unwrap());
                                    amount_out_b.set("".to_string());
                                    loading_b.set("Loading...".to_string());
                                    let res = client().pools.estimate_trade(TradeRequest {
                                        timestamp: timestamp_b().parse().unwrap(),
                                        token_in: token_in_b().parse().unwrap(),
                                        token_out: token_out_b().parse().unwrap(),
                                        amount_in,
                                    }).await.unwrap();
                                    loading_b.set("".to_string());
                                    match res {
                                        ApiResponse::Success(res) => {
                                            loading_b.set("".to_string());
                                            let amount_out = unit.from_yocto(res.amount_out);
                                            amount_out_b.set(format_amount(amount_out));
                                        }
                                        ApiResponse::Error(e) => {
                                            loading_b.set(e.to_string());
                                        }
                                    }
                                });
                            },
                            "Estimate"
                        }
                        span { class: "loading", "{loading_b}" }
                    }
                }

                // C
                div { class: "estimate_trade-container",
                    div { class: "timestamp",
                        input { type: "datetime-local", name: "timestamp_c", value: "{timestamp_c}",
                            oninput: move |e| timestamp_c.set(e.value())
                        }
                    }
                    div { class: "token_in",
                        input { type: "text", name: "token_in_c", value: "{token_in_c}", size: "30",
                            oninput: move |e| token_in_c.set(e.value())
                        }
                    }
                    div { class: "token_out",
                        input { type: "text", name: "token_out_c", value: "{token_out_c}", size: "30",
                            oninput: move |e| token_out_c.set(e.value())
                        }
                    }
                    div { class: "amount",
                        div { class: "amount_in",
                            input { type: "text", name: "amount_in_c", value: "{amount_in_c}", size: "30",
                                oninput: move |e| amount_in_c.set(e.value())
                            }
                            select {
                                name: "amount_unit_c",
                                value: "{amount_unit_c.to_string()}",
                                onchange: move |e| amount_unit_c.set(e.value()),
                                option { value: "NEAR", "NEAR" }
                                option { value: "mNEAR", "mNEAR" }
                                option { value: "yNEAR", "yNEAR" }
                            }
                        }
                        div { class: "amount_out",
                            input { type: "text", name: "amount_out_c", value: "{amount_out_c}", size: "30",
                                oninput: move |e| amount_out_c.set(e.value())
                            }
                        }
                    }
                    div { class: "button-with-loading",
                        button { class: "btn btn-primary",
                            onclick: move |_| {
                                spawn_local(async move {
                                    let unit: NearUnit = amount_unit_c().parse().unwrap();
                                    let amount_in = unit.to_yocto(amount_in_c().parse().unwrap());
                                    amount_out_c.set("".to_string());
                                    loading_c.set("Loading...".to_string());
                                    let res = client().pools.estimate_trade(TradeRequest {
                                        timestamp: timestamp_c().parse().unwrap(),
                                        token_in: token_in_c().parse().unwrap(),
                                        token_out: token_out_c().parse().unwrap(),
                                        amount_in,
                                    }).await.unwrap();
                                    match res {
                                        ApiResponse::Success(res) => {
                                            loading_c.set("".to_string());
                                            let amount_out = unit.from_yocto(res.amount_out);
                                            amount_out_c.set(format_amount(amount_out));
                                        }
                                        ApiResponse::Error(e) => {
                                            loading_c.set(e.to_string());
                                        }
                                    }
                                });
                            },
                            "Estimate"
                        }
                        span { class: "loading", "{loading_c}" }
                    }
                }

                // D
                div { class: "estimate_trade-container",
                    div { class: "timestamp",
                        input { type: "datetime-local", name: "timestamp_d", value: "{timestamp_d}",
                            oninput: move |e| timestamp_d.set(e.value())
                        }
                    }
                    div { class: "token_in",
                        input { type: "text", name: "token_in_d", value: "{token_in_d}", size: "30",
                            oninput: move |e| token_in_d.set(e.value())
                        }
                    }
                    div { class: "token_out",
                        input { type: "text", name: "token_out_d", value: "{token_out_d}", size: "30",
                            oninput: move |e| token_out_d.set(e.value())
                        }
                    }
                    div { class: "amount",
                        div { class: "amount_in",
                            input { type: "text", name: "amount_in_d", value: "{amount_in_d}", size: "30",
                                oninput: move |e| amount_in_d.set(e.value())
                            }
                            select {
                                name: "amount_unit_d",
                                value: "{amount_unit_d.to_string()}",
                                onchange: move |e| amount_unit_d.set(e.value()),
                                option { value: "NEAR", "NEAR" }
                                option { value: "mNEAR", "mNEAR" }
                                option { value: "yNEAR", "yNEAR" }
                            }
                        }
                        div { class: "amount_out",
                            input { type: "text", name: "amount_out_d", value: "{amount_out_d}", size: "30",
                                oninput: move |e| amount_out_d.set(e.value())
                            }
                        }
                    }
                    div { class: "button-with-loading",
                        button { class: "btn btn-primary",
                            onclick: move |_| {
                                spawn_local(async move {
                                    let unit: NearUnit = amount_unit_d().parse().unwrap();
                                    let amount_in = unit.to_yocto(amount_in_d().parse().unwrap());
                                    amount_out_d.set("".to_string());
                                    loading_d.set("Loading...".to_string());
                                    let res = client().pools.estimate_trade(TradeRequest {
                                        timestamp: timestamp_d().parse().unwrap(),
                                        token_in: token_in_d().parse().unwrap(),
                                        token_out: token_out_d().parse().unwrap(),
                                        amount_in,
                                    }).await.unwrap();
                                    match res {
                                        ApiResponse::Success(res) => {
                                            loading_d.set("".to_string());
                                            let amount_out = unit.from_yocto(res.amount_out);
                                            amount_out_d.set(format_amount(amount_out));
                                        }
                                        ApiResponse::Error(e) => {
                                            loading_d.set(e.to_string());
                                        }
                                    }
                                });
                            },
                            "Estimate"
                        }
                        span { class: "loading", "{loading_d}" }
                    }
                }
            }
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
                                spawn_local(async move {
                                    pools_loading.set("Loading...".to_string());
                                    pools.set("".to_string());
                                    let mut ids = vec![];
                                    for s in pool_ids().split_whitespace().map(|s| s.trim()).filter(|s| !s.is_empty()) {
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
                                    let res = client().pools.get_pool_records(PoolRecordsRequest {
                                        timestamp: pools_timestamp().parse().unwrap(),
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
