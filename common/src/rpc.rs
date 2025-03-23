// Tarpc サービスの定義
#[tarpc::service]
pub trait ZaciraciService {
    //// basic

    /// サーバーの健全性チェック
    async fn healthcheck() -> String;
    
    /// ネイティブトークンの残高を取得
    async fn native_token_balance() -> String;
    
    /// ネイティブトークンを転送
    async fn native_token_transfer(receiver: String, amount: String) -> String;

    //// pools
    
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
    
    //// storage
    
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
