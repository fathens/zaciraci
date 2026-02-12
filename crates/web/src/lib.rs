#![deny(warnings)]

mod services;

pub mod proto {
    tonic::include_proto!("zaciraci.v1");
}

use logging::*;
use proto::config_service_server::ConfigServiceServer;
use proto::health_service_server::HealthServiceServer;
use services::config::ConfigServiceImpl;
use services::health::HealthServiceImpl;

pub async fn serve(addr: &str) {
    let log = DEFAULT.new(o!("module" => "web"));

    let addr = addr.parse().expect("valid socket address");

    let health_svc = HealthServiceServer::new(HealthServiceImpl);
    let config_svc = ConfigServiceServer::new(ConfigServiceImpl);

    info!(log, "gRPC server starting"; "addr" => %addr);

    tonic::transport::Server::builder()
        .accept_http1(true)
        .layer(tonic_web::GrpcWebLayer::new())
        .add_service(health_svc)
        .add_service(config_svc)
        .serve(addr)
        .await
        .expect("gRPC server failed");
}
