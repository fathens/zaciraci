#![deny(warnings)]

mod services;

pub mod proto {
    tonic::include_proto!("zaciraci.v1");
}

use std::net::SocketAddr;

use logging::*;
use proto::config_service_server::ConfigServiceServer;
use proto::health_service_server::HealthServiceServer;
use proto::portfolio_service_server::PortfolioServiceServer;
use services::config::ConfigServiceImpl;
use services::health::HealthServiceImpl;
use services::portfolio::PortfolioServiceImpl;

pub async fn serve(port: u16) {
    let log = DEFAULT.new(o!("module" => "web"));

    let addr = SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 0], port));

    let health_svc = HealthServiceServer::new(HealthServiceImpl);
    let config_svc = ConfigServiceServer::new(ConfigServiceImpl);
    let portfolio_svc = PortfolioServiceServer::new(PortfolioServiceImpl);

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
