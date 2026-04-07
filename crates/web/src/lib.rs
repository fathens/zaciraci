#![deny(warnings)]

mod services;

pub mod proto {
    tonic::include_proto!("zaciraci.v1");
}

use std::net::SocketAddr;
use std::sync::Arc;

use google_auth::GoogleAuthenticator;
use grpc_auth::AuthInterceptor;
use logging::*;
use proto::config_service_server::ConfigServiceServer;
use proto::health_service_server::HealthServiceServer;
use proto::portfolio_service_server::PortfolioServiceServer;
use services::config::ConfigServiceImpl;
use services::health::HealthServiceImpl;
use services::portfolio::PortfolioServiceImpl;
use tonic::service::interceptor::InterceptedService;

pub async fn serve(port: u16) {
    let log = DEFAULT.new(o!("module" => "web"));

    let addr = SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 0], port));

    // Bootstrap the Google authenticator. JWKS is fetched eagerly with
    // fail-open semantics (empty cache on failure, logged) and a background
    // refresh task is spawned. UserCache is loaded from the DB.
    let startup = common::config::startup::get();
    let authenticator = match GoogleAuthenticator::bootstrap(startup.google_client_id.clone()).await
    {
        Ok(auth) => Arc::new(auth),
        Err(e) => {
            error!(log, "auth_bootstrap_failed"; "error" => %e);
            panic!("failed to bootstrap authenticator: {e}");
        }
    };
    let auth_interceptor = AuthInterceptor::new(authenticator);

    // Health is intentionally exempt from authentication so liveness probes
    // can work without credentials.
    let health_svc = HealthServiceServer::new(HealthServiceImpl);

    // Authenticated services wrap the raw server in an InterceptedService.
    let config_svc = InterceptedService::new(
        ConfigServiceServer::new(ConfigServiceImpl),
        auth_interceptor.clone(),
    );
    let portfolio_svc = InterceptedService::new(
        PortfolioServiceServer::new(PortfolioServiceImpl),
        auth_interceptor,
    );

    info!(log, "gRPC server starting"; "addr" => %addr);

    tonic::transport::Server::builder()
        .accept_http1(true)
        .layer(tonic_web::GrpcWebLayer::new())
        .add_service(health_svc)
        .add_service(config_svc)
        .add_service(portfolio_svc)
        .serve(addr)
        .await
        .expect("gRPC server failed");
}
