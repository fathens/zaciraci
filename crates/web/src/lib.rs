#![deny(warnings)]

mod services;

pub mod proto {
    tonic::include_proto!("zaciraci.v1");
}

use logging::*;
use proto::health_service_server::HealthServiceServer;
use services::health::HealthServiceImpl;

pub async fn serve(addr: &str) {
    let log = DEFAULT.new(o!("module" => "web"));

    let addr = addr.parse().expect("valid socket address");

    let health_svc = HealthServiceServer::new(HealthServiceImpl);

    info!(log, "gRPC server starting"; "addr" => %addr);

    tonic::transport::Server::builder()
        .accept_http1(true)
        .layer(tonic_web::GrpcWebLayer::new())
        .add_service(health_svc)
        .serve(addr)
        .await
        .expect("gRPC server failed");
}
