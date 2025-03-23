// プールサービスのgRPC実装

use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::generated::zaciraci::common::Empty;
use crate::generated::zaciraci::pools::{
    GetAllPoolsResponse, EstimateReturnRequest, EstimateReturnResponse,
    GetReturnRequest, GetReturnResponse, ListAllTokensResponse,
    ListReturnsRequest, ListReturnsResponse, PickGoalsRequest, PickGoalsResponse,
    RunSwapRequest, RunSwapResponse, Pool as ProtoPool, Token as ProtoToken,
    ReturnInfo as ProtoReturnInfo, GoalInfo as ProtoGoalInfo,
    pools_service_server::PoolsService,
};
use crate::services::pools::{
    PoolsService as PoolsServiceTrait, PoolsServiceImpl,
    Pool, Token, ReturnInfo, GoalInfo,
};

// gRPCサービス実装
#[derive(Default)]
pub struct PoolsServiceGrpc {
    service: Arc<PoolsServiceImpl>,
}

impl PoolsServiceGrpc {
    pub fn new(service: Arc<PoolsServiceImpl>) -> Self {
        Self { service }
    }
    
    // サービスのPool型をProto用に変換
    fn convert_pool(pool: &Pool) -> ProtoPool {
        ProtoPool {
            id: pool.id.clone(),
            tokens: pool.tokens.iter().map(|t| ProtoToken {
                account_id: t.account_id.clone(),
                symbol: t.symbol.clone(),
                balance: t.balance.clone(),
            }).collect(),
        }
    }
    
    // サービスのToken型をProto用に変換
    fn convert_token(token: &Token) -> ProtoToken {
        ProtoToken {
            account_id: token.account_id.clone(),
            symbol: token.symbol.clone(),
            balance: token.balance.clone(),
        }
    }
    
    // サービスのReturnInfo型をProto用に変換
    fn convert_return_info(info: &ReturnInfo) -> ProtoReturnInfo {
        ProtoReturnInfo {
            token_account: info.token_account.clone(),
            return_amount: info.return_amount.clone(),
        }
    }
    
    // サービスのGoalInfo型をProto用に変換
    fn convert_goal_info(info: &GoalInfo) -> ProtoGoalInfo {
        ProtoGoalInfo {
            token_account: info.token_account.clone(),
            expected_return: info.expected_return.clone(),
        }
    }
}

#[tonic::async_trait]
impl PoolsService for PoolsServiceGrpc {
    async fn get_all_pools(&self, _request: Request<Empty>) -> Result<Response<GetAllPoolsResponse>, Status> {
        match self.service.get_all_pools() {
            Ok(pools) => {
                let proto_pools = pools.iter()
                    .map(|p| Self::convert_pool(p))
                    .collect();
                
                let reply = GetAllPoolsResponse {
                    pools: proto_pools,
                };
                Ok(Response::new(reply))
            }
            Err(err) => {
                Err(Status::internal(format!("プール取得エラー: {}", err)))
            }
        }
    }
    
    async fn estimate_return(
        &self, 
        request: Request<EstimateReturnRequest>
    ) -> Result<Response<EstimateReturnResponse>, Status> {
        let req = request.into_inner();
        
        match self.service.estimate_return(&req.pool_id, &req.amount) {
            Ok(estimated_return) => {
                let reply = EstimateReturnResponse {
                    estimated_return,
                };
                Ok(Response::new(reply))
            }
            Err(err) => {
                Err(Status::internal(format!("見積もりエラー: {}", err)))
            }
        }
    }
    
    async fn get_return(
        &self, 
        request: Request<GetReturnRequest>
    ) -> Result<Response<GetReturnResponse>, Status> {
        let req = request.into_inner();
        
        match self.service.get_return(&req.pool_id, &req.amount) {
            Ok(return_amount) => {
                let reply = GetReturnResponse {
                    return_amount,
                };
                Ok(Response::new(reply))
            }
            Err(err) => {
                Err(Status::internal(format!("リターン取得エラー: {}", err)))
            }
        }
    }
    
    async fn list_all_tokens(&self, _request: Request<Empty>) -> Result<Response<ListAllTokensResponse>, Status> {
        match self.service.list_all_tokens() {
            Ok(tokens) => {
                let proto_tokens = tokens.iter()
                    .map(|t| Self::convert_token(t))
                    .collect();
                
                let reply = ListAllTokensResponse {
                    tokens: proto_tokens,
                };
                Ok(Response::new(reply))
            }
            Err(err) => {
                Err(Status::internal(format!("トークンリスト取得エラー: {}", err)))
            }
        }
    }
    
    async fn list_returns(
        &self, 
        request: Request<ListReturnsRequest>
    ) -> Result<Response<ListReturnsResponse>, Status> {
        let req = request.into_inner();
        
        match self.service.list_returns(&req.token_account, &req.amount) {
            Ok(returns) => {
                let proto_returns = returns.iter()
                    .map(|r| Self::convert_return_info(r))
                    .collect();
                
                let reply = ListReturnsResponse {
                    returns: proto_returns,
                };
                Ok(Response::new(reply))
            }
            Err(err) => {
                Err(Status::internal(format!("リターンリスト取得エラー: {}", err)))
            }
        }
    }
    
    async fn pick_goals(
        &self, 
        request: Request<PickGoalsRequest>
    ) -> Result<Response<PickGoalsResponse>, Status> {
        let req = request.into_inner();
        
        match self.service.pick_goals(&req.token_account, &req.amount) {
            Ok(goals) => {
                let proto_goals = goals.iter()
                    .map(|g| Self::convert_goal_info(g))
                    .collect();
                
                let reply = PickGoalsResponse {
                    goals: proto_goals,
                };
                Ok(Response::new(reply))
            }
            Err(err) => {
                Err(Status::internal(format!("目標選択エラー: {}", err)))
            }
        }
    }
    
    async fn run_swap(
        &self, 
        request: Request<RunSwapRequest>
    ) -> Result<Response<RunSwapResponse>, Status> {
        let req = request.into_inner();
        
        match self.service.run_swap(&req.token_in_account, &req.initial_value, &req.token_out_account) {
            Ok((success, transaction_hash, result_amount)) => {
                let reply = RunSwapResponse {
                    success,
                    transaction_hash,
                    result_amount,
                };
                Ok(Response::new(reply))
            }
            Err(err) => {
                Err(Status::internal(format!("スワップ実行エラー: {}", err)))
            }
        }
    }
}

// 利便性のためのタイプエイリアス
pub type PoolsServiceServer = 
    crate::generated::zaciraci::pools::pools_service_server::PoolsServiceServer<PoolsServiceGrpc>;
