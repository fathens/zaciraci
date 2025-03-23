// ストレージサービスのgRPC実装

use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::generated::zaciraci::common::Empty;
use crate::generated::zaciraci::storage::{
    DepositMinResponse, DepositRequest, DepositResponse,
    UnregisterRequest, UnregisterResponse,
    storage_service_server::StorageService,
};
use crate::services::storage::{
    StorageService as StorageServiceTrait, 
    StorageServiceImpl,
};

// gRPCサービス実装
#[derive(Default)]
pub struct StorageServiceGrpc {
    service: Arc<StorageServiceImpl>,
}

impl StorageServiceGrpc {
    pub fn new(service: Arc<StorageServiceImpl>) -> Self {
        Self { service }
    }
}

#[tonic::async_trait]
impl StorageService for StorageServiceGrpc {
    async fn get_deposit_min(&self, _request: Request<Empty>) -> Result<Response<DepositMinResponse>, Status> {
        // ビジネスロジック層を呼び出し
        match self.service.get_deposit_min() {
            Ok(min_amount) => {
                let reply = DepositMinResponse {
                    min_amount,
                };
                Ok(Response::new(reply))
            }
            Err(err) => {
                Err(Status::internal(format!("最小デポジット額取得エラー: {}", err)))
            }
        }
    }
    
    async fn deposit(&self, request: Request<DepositRequest>) -> Result<Response<DepositResponse>, Status> {
        let req = request.into_inner();
        
        // ビジネスロジック層を呼び出し
        match self.service.deposit(&req.amount) {
            Ok((success, transaction_hash)) => {
                let reply = DepositResponse {
                    success,
                    transaction_hash,
                };
                Ok(Response::new(reply))
            }
            Err(err) => {
                Err(Status::internal(format!("デポジットエラー: {}", err)))
            }
        }
    }
    
    async fn unregister(&self, request: Request<UnregisterRequest>) -> Result<Response<UnregisterResponse>, Status> {
        let req = request.into_inner();
        
        // ビジネスロジック層を呼び出し
        match self.service.unregister(&req.token_account) {
            Ok((success, transaction_hash)) => {
                let reply = UnregisterResponse {
                    success,
                    transaction_hash,
                };
                Ok(Response::new(reply))
            }
            Err(err) => {
                Err(Status::internal(format!("登録解除エラー: {}", err)))
            }
        }
    }
}

// 利便性のためのタイプエイリアス
pub type StorageServiceServer = 
    crate::generated::zaciraci::storage::storage_service_server::StorageServiceServer<StorageServiceGrpc>;
