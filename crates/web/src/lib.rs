#![deny(warnings)]

mod services;

pub mod proto {
    tonic::include_proto!("zaciraci.v1");
}

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
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

/// Start the gRPC / grpc-web server.
///
/// # Transport security
///
/// This server is intentionally started **without TLS**. The process is
/// expected to run behind a TLS-terminating reverse proxy (e.g. the
/// ingress layer defined in `run_local/docker-compose.yml` or the
/// production deployment). Every authenticated request carries a Google
/// ID token as a `Bearer` credential; if this process is ever exposed
/// directly to an untrusted network, those tokens will travel in
/// plaintext. Operators must ensure:
///
/// 1. The listening socket is only reachable via a TLS-terminating
///    proxy, or is bound to a loopback / Unix-domain socket.
/// 2. `GOOGLE_CLIENT_ID` is configured so the authenticator does not
///    accept every token.
pub async fn serve(port: u16) -> anyhow::Result<()> {
    let log = DEFAULT.new(o!("module" => "web"));

    let addr = SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 0], port));

    // Bootstrap the Google authenticator. JWKS is fetched eagerly with
    // fail-open semantics (empty cache on failure, logged) and a background
    // refresh task is spawned. UserCache is loaded from the DB with bounded
    // retries inside bootstrap to tolerate a briefly-unavailable database.
    let startup = common::config::startup::get();
    let authenticator = GoogleAuthenticator::bootstrap(startup.google_client_id.clone())
        .await
        .context("failed to bootstrap authenticator")?;
    let auth_interceptor = AuthInterceptor::new(Arc::new(authenticator));

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
        .context("gRPC server failed")?;

    Ok(())
}
