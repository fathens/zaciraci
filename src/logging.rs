use once_cell::sync::Lazy;
pub use slog::*;

fn wrap<D: Drain<Err = Never, Ok = ()> + Send + 'static>(drain: D) -> Fuse<slog_async::Async> {
    slog_async::Async::default(slog_envlogger::new(drain)).fuse()
}

pub static DEFAULT: Lazy<Logger> = Lazy::new(|| {
    let mk_term = || {
        slog_term::FullFormat::new(slog_term::TermDecorator::new().build())
            .build()
            .fuse()
    };

    let mk_json = || slog_json::Json::default(std::io::stdout()).fuse();

    let format = std::env::var("LOG_FORMAT").unwrap_or_default();
    let drain = match format.as_str() {
        "json" => wrap(mk_json()),
        _ => wrap(mk_term()),
    };

    Logger::root(
        drain,
        o!(
            "version" => env!("CARGO_PKG_VERSION"),
        ),
    )
});
