// Protoファイルから自動生成されるコードのモジュール
// build.rsによって生成されるコードを公開する

// common
pub mod zaciraci {
    pub mod common {
        include!("generated/zaciraci.common.rs");
    }

    // health
    pub mod health {
        include!("generated/zaciraci.health.rs");
        pub use health_service_server::HealthService;
    }

    // native_token
    pub mod native_token {
        include!("generated/zaciraci.native_token.rs");
        pub use native_token_service_server::NativeTokenService;
    }

    // pools
    pub mod pools {
        include!("generated/zaciraci.pools.rs");
        pub use pools_service_server::PoolsService;
    }

    // storage
    pub mod storage {
        include!("generated/zaciraci.storage.rs");
        pub use storage_service_server::StorageService;
    }
}
