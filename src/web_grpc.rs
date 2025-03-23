// gRPCサーバーの実装
use std::sync::Arc;
use tonic::transport::Server;

// 各サービスの実装をインポート
use crate::services::{
    health::HealthServiceImpl,
    native_token::NativeTokenServiceImpl,
    pools::PoolsServiceImpl,
    storage::StorageServiceImpl,
};

// gRPC実装をインポート
use crate::rpc::{
    health::{HealthServiceGrpc, HealthServiceServer},
    native_token::{NativeTokenServiceGrpc, NativeTokenServiceServer},
    pools::{PoolsServiceGrpc, PoolsServiceServer},
    storage::{StorageServiceGrpc, StorageServiceServer},
};

pub async fn run() {
    // サーバーのアドレス設定
    let addr = "[::1]:50051".parse().unwrap();
    
    // 各サービスの初期化
    let health_service = Arc::new(HealthServiceImpl::default());
    let native_token_service = Arc::new(NativeTokenServiceImpl::default());
    let pools_service = Arc::new(PoolsServiceImpl::default());
    let storage_service = Arc::new(StorageServiceImpl::default());
    
    // gRPCサービスの初期化
    let health_grpc = HealthServiceGrpc::new(health_service);
    let native_token_grpc = NativeTokenServiceGrpc::new(native_token_service);
    let pools_grpc = PoolsServiceGrpc::new(pools_service);
    let storage_grpc = StorageServiceGrpc::new(storage_service);

    println!("gRPCサーバーを開始します: {}", addr);

    // サーバーを構築して実行
    Server::builder()
        .add_service(HealthServiceServer::new(health_grpc))
        .add_service(NativeTokenServiceServer::new(native_token_grpc))
        .add_service(PoolsServiceServer::new(pools_grpc))
        .add_service(StorageServiceServer::new(storage_grpc))
        .serve(addr)
        .await
        .unwrap();
}
