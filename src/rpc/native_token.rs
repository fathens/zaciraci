// ネイティブトークンサービスのgRPC実装

use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::generated::zaciraci::common::Empty;
use crate::generated::zaciraci::native_token::{
    BalanceResponse, TransferRequest, TransferResponse,
    native_token_service_server::NativeTokenService,
};
use crate::services::native_token::{
    NativeTokenService as NativeTokenServiceTrait, 
    NativeTokenServiceImpl,
};

// gRPCサービス実装
#[derive(Default)]
pub struct NativeTokenServiceGrpc {
    service: Arc<NativeTokenServiceImpl>,
}

impl NativeTokenServiceGrpc {
    pub fn new(service: Arc<NativeTokenServiceImpl>) -> Self {
        Self { service }
    }
}

#[tonic::async_trait]
impl NativeTokenService for NativeTokenServiceGrpc {
    async fn get_balance(&self, _request: Request<Empty>) -> Result<Response<BalanceResponse>, Status> {
        // ビジネスロジック層を呼び出し
        match self.service.get_balance() {
            Ok(balance) => {
                let reply = BalanceResponse {
                    balance,
                };
                Ok(Response::new(reply))
            }
            Err(err) => {
                Err(Status::internal(format!("残高取得エラー: {}", err)))
            }
        }
    }
    
    async fn transfer(&self, request: Request<TransferRequest>) -> Result<Response<TransferResponse>, Status> {
        let req = request.into_inner();
        
        // ビジネスロジック層を呼び出し
        match self.service.transfer(&req.receiver, &req.amount) {
            Ok((success, transaction_hash)) => {
                let reply = TransferResponse {
                    success,
                    transaction_hash,
                };
                Ok(Response::new(reply))
            }
            Err(err) => {
                Err(Status::internal(format!("送金エラー: {}", err)))
            }
        }
    }
}

// 利便性のためのタイプエイリアス
pub type NativeTokenServiceServer = 
    crate::generated::zaciraci::native_token::native_token_service_server::NativeTokenServiceServer<NativeTokenServiceGrpc>;
