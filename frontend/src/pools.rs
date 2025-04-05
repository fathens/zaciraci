use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;
use zaciraci_common::{pools::TradeRequest, types::NearUnit};

#[component]
pub fn view() -> Element {
    let client = use_signal(|| crate::server_api::get_client());
    
    let now = chrono::Local::now().naive_utc().format("%Y-%m-%dT%H:%M:%S").to_string();
    let mut timestamp_a = use_signal(|| now.clone());
    let mut timestamp_b = use_signal(|| now.clone());
    let mut timestamp_c = use_signal(|| now.clone());

    let mut amount_unit_a = use_signal(|| NearUnit::Near.to_string());
    let mut amount_unit_b = use_signal(|| NearUnit::Near.to_string());
    let mut amount_unit_c = use_signal(|| NearUnit::Near.to_string());

    let mut amount_in_a = use_signal(|| "1".to_string());
    let mut amount_in_b = use_signal(|| "1".to_string());
    let mut amount_in_c = use_signal(|| "1".to_string());

    let mut amount_out_a = use_signal(|| "0".to_string());
    let mut amount_out_b = use_signal(|| "0".to_string());
    let mut amount_out_c = use_signal(|| "0".to_string());

    let mut token_in_a = use_signal(|| "wrap.near".to_string());
    let mut token_in_b = use_signal(|| "wrap.near".to_string());
    let mut token_in_c = use_signal(|| "wrap.near".to_string());

    let mut token_out_a = use_signal(|| "".to_string());
    let mut token_out_b = use_signal(|| "".to_string());
    let mut token_out_c = use_signal(|| "".to_string());

    rsx! {
        // A
        div { class: "estimate_trade-container",
            div { class: "timestamp",
                input { type: "datetime-local", name: "timestamp_a", value: "{timestamp_a}",
                    oninput: move |e| timestamp_a.set(e.value())
                }
            }
            div { class: "token_in",
                input { type: "text", name: "token_in_a", value: "{token_in_a}",
                    oninput: move |e| token_in_a.set(e.value())
                }
            }
            div { class: "token_out",
                input { type: "text", name: "token_out_a", value: "{token_out_a}",
                    oninput: move |e| token_out_a.set(e.value())
                }
            }
            div { class: "amount",
                div { class: "amount_in",
                    input { type: "text", name: "amount_in_a", value: "{amount_in_a}",
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
                    input { type: "text", name: "amount_out_a", value: "{amount_out_a}",
                        oninput: move |e| amount_out_a.set(e.value())
                    }
                }
            }
            button { class: "btn btn-primary",
                onclick: move |_| {
                    spawn_local(async move {
                        let unit: NearUnit = amount_unit_a().parse().unwrap();
                        let amount_in = unit.to_yocto(amount_in_a().parse().unwrap());
                        let res = client().pools.estimate_trade(TradeRequest {
                            timestamp: timestamp_a().parse().unwrap(),
                            token_in: token_in_a().parse().unwrap(),
                            token_out: token_out_a().parse().unwrap(),
                            amount_in,
                        }).await.unwrap();

                        let amount_out = unit.from_yocto(res.amount_out);
                        amount_out_a.set(amount_out.to_string());
                    });
                },
                "Estimate"
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
                input { type: "text", name: "token_in_b", value: "{token_in_b}",
                    oninput: move |e| token_in_b.set(e.value())
                }
            }
            div { class: "token_out",
                input { type: "text", name: "token_out_b", value: "{token_out_b}",
                    oninput: move |e| token_out_b.set(e.value())
                }
            }
            div { class: "amount",
                div { class: "amount_in",
                    input { type: "text", name: "amount_in_b", value: "{amount_in_b}",
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
                    input { type: "text", name: "amount_out_b", value: "{amount_out_b}",
                        oninput: move |e| amount_out_b.set(e.value())
                    }
                }
            }
            button { class: "btn btn-primary",
                onclick: move |_| {
                    spawn_local(async move {
                        let unit: NearUnit = amount_unit_b().parse().unwrap();
                        let amount_in = unit.to_yocto(amount_in_b().parse().unwrap());
                        let res = client().pools.estimate_trade(TradeRequest {
                            timestamp: timestamp_b().parse().unwrap(),
                            token_in: token_in_b().parse().unwrap(),
                            token_out: token_out_b().parse().unwrap(),
                            amount_in,
                        }).await.unwrap();

                        let amount_out = unit.from_yocto(res.amount_out);
                        amount_out_b.set(amount_out.to_string());
                    });
                },
                "Estimate"
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
                input { type: "text", name: "token_in_c", value: "{token_in_c}",
                    oninput: move |e| token_in_c.set(e.value())
                }
            }
            div { class: "token_out",
                input { type: "text", name: "token_out_c", value: "{token_out_c}",
                    oninput: move |e| token_out_c.set(e.value())
                }
            }
            div { class: "amount",
                div { class: "amount_in",
                    input { type: "text", name: "amount_in_c", value: "{amount_in_c}",
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
                    input { type: "text", name: "amount_out_c", value: "{amount_out_c}",
                        oninput: move |e| amount_out_c.set(e.value())
                    }
                }
            }
            button { class: "btn btn-primary",
                onclick: move |_| {
                    spawn_local(async move {
                        let unit: NearUnit = amount_unit_c().parse().unwrap();
                        let amount_in = unit.to_yocto(amount_in_c().parse().unwrap());
                        let res = client().pools.estimate_trade(TradeRequest {
                            timestamp: timestamp_c().parse().unwrap(),
                            token_in: token_in_c().parse().unwrap(),
                            token_out: token_out_c().parse().unwrap(),
                            amount_in,
                        }).await.unwrap();

                        let amount_out = unit.from_yocto(res.amount_out);
                        amount_out_c.set(amount_out.to_string());
                    });
                },
                "Estimate"
            }
        }
    }
}
