fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir("src/generated")
        .compile(
            &[
                "rpcdef/common.proto",
                "rpcdef/health.proto",
                "rpcdef/native_token.proto",
                "rpcdef/pools.proto",
                "rpcdef/storage.proto",
            ],
            &["rpcdef"],
        )?;
    Ok(())
}
