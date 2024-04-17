use once_cell::sync::Lazy;
pub use slog::*;

pub static DEFAULT: Lazy<Logger> = Lazy::new(|| {
    let drain = slog_term::FullFormat::new(slog_term::TermDecorator::new().build())
        .build()
        .fuse();
    // let drain = Mutex::new(slog_json::Json::default(io::stdout())).fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    Logger::root(
        drain,
        o!(
            "version" => env!("CARGO_PKG_VERSION")
        ),
    )
});
