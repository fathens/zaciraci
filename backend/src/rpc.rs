use futures_util::StreamExt;
use tarpc::{
    server::{self, Channel},
    serde_transport::tcp,
};
use tokio_serde::formats::Json;
use service::ZaciraciServiceImpl;
use zaciraci_common::rpc::ZaciraciService;

mod service;

// サーバーの起動関数
pub async fn run() {
    // TCP リスナーの作成
    let addr = "0.0.0.0:8080".parse::<std::net::SocketAddr>().unwrap();
    
    // サーバーインスタンスの作成
    let server = ZaciraciServiceImpl;
    
    // トランスポートリスナーの設定
    let mut listener = tcp::listen(addr, Json::default).await.unwrap();
    
    // 接続の受け入れとサービス実行
    listener.config_mut().max_frame_length(usize::MAX);
    
    listener
        .filter_map(|r| async move { r.ok() })
        .map(server::BaseChannel::with_defaults)
        .for_each(|channel| {
            let server_clone = server.clone();
            async move {
                let server = channel.execute(server_clone.serve());
                tokio::spawn(server.for_each(|response| async {
                    tokio::spawn(response);
                }));
            }
        })
        .await;
}
