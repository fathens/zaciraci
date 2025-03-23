use futures_util::StreamExt;
use tarpc::{
    client, context,
    server::{self, Channel},
    serde_transport::tcp,
};
use tokio_serde::formats::Json;
use service::ZaciraciServiceImpl;

mod service;

// Tarpc サービスの定義
#[tarpc::service]
pub trait ZaciraciService {
    /// サーバーの健全性チェック
    async fn healthcheck() -> String;
    
    /// ネイティブトークンの残高を取得
    async fn native_token_balance() -> String;
    
    /// ネイティブトークンを転送
    async fn native_token_transfer(receiver: String, amount: String) -> String;
    
    /// すべてのプールを取得
    async fn get_all_pools() -> String;
    
    /// リターンを推定
    async fn estimate_return(pool_id: u32, amount: u128) -> String;
    
    /// リターンを取得
    async fn get_return(pool_id: u32, amount: u128) -> String;
    
    /// すべてのトークンをリスト
    async fn list_all_tokens() -> String;
    
    /// リターンをリスト
    async fn list_returns(token_account: String, initial_value: String) -> String;
    
    /// ゴールを選択
    async fn pick_goals(token_account: String, initial_value: String) -> String;
    
    /// スワップを実行
    async fn run_swap(token_in_account: String, initial_value: String, token_out_account: String) -> String;
    
    /// 最小ストレージ預金を取得
    async fn storage_deposit_min() -> String;
    
    /// ストレージに預金
    async fn storage_deposit(amount: String) -> String;
    
    /// トークンの登録を解除
    async fn storage_unregister_token(token_account: String) -> String;
    
    /// 預金リストを取得
    async fn deposit_list() -> String;
    
    /// ネイティブトークンをラップ
    async fn wrap_native_token(amount: String) -> String;
    
    /// ネイティブトークンをアンラップ
    async fn unwrap_native_token(amount: String) -> String;
    
    /// トークンを預金
    async fn deposit_token(token_account: String, amount: String) -> String;
    
    /// トークンを引き出し
    async fn withdraw_token(token_account: String, amount: String) -> String;
}


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

// クライアント接続用のヘルパー関数
#[allow(dead_code)]
pub async fn connect() -> ZaciraciServiceClient {
    let addr = "127.0.0.1:8080".parse::<std::net::SocketAddr>().unwrap();
    let transport = tcp::connect(addr, Json::default).await.unwrap();
    ZaciraciServiceClient::new(client::Config::default(), transport).spawn()
}

// クライアント使用例
#[allow(dead_code)]
pub async fn client_example() -> String {
    let client = connect().await;
    let ctx = context::current();
    
    // 健全性チェック
    let health = client.healthcheck(ctx).await.unwrap();
    println!("ヘルスチェック結果: {}", health);
    
    // 残高チェック
    let balance = client.native_token_balance(ctx).await.unwrap();
    println!("残高: {}", balance);
    
    balance
}
