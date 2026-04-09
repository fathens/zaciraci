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
///
/// # Threat model: token replay
///
/// This server validates Google ID tokens (signature, `iss`, `aud`, `exp`,
/// `nbf`, `iat` max-age) but does **not** maintain a `jti` blacklist or a
/// nonce cache. A leaked token can therefore be replayed by anyone who
/// intercepts it until the earlier of:
///
/// - the token's `exp` (Google issues 1h `exp` for ID tokens), or
/// - the application's `iat`-based max-age (also 1h, applied as a
///   defense-in-depth ceiling in `google_auth::validator`).
///
/// This trade-off is accepted for the current Slint-client deployment
/// because:
///
/// - The transport is HTTPS-terminated upstream, so passive interception
///   requires a compromised intermediary.
/// - Token lifetimes are short (1 hour) and tokens are not stored
///   long-term on the client.
/// - The user population is small and explicitly enrolled in
///   `authorized_users`, so the blast radius of a replay is bounded by
///   the role of the leaked principal.
///
/// A future enhancement would add a `jti` TTL cache (e.g. via the `moka`
/// crate) so that any presented `jti` is recorded and a second use within
/// the cache window is rejected. This is left as a follow-up; it should
/// be revisited if the threat model changes (mobile clients, public
/// network deployments, or higher-privilege roles).
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
