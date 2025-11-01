use crate::config;
use once_cell::sync::Lazy;
pub use slog::*;

fn wrap<D>(drain: D) -> Fuse<slog_async::Async>
where
    D: Drain<Err = Never, Ok = ()> + Send + 'static,
{
    slog_async::Async::new(slog_envlogger::new(drain))
        .chan_size(2 << 16)
        .thread_name("slog-async".into())
        .build()
        .fuse()
}

pub static DEFAULT: Lazy<Logger> = Lazy::new(|| {
    let mk_term = || {
        slog_term::FullFormat::new(slog_term::TermDecorator::new().build())
            .build()
            .fuse()
    };

    let mk_json = || slog_json::Json::default(std::io::stdout()).fuse();

    let format = config::get("RUST_LOG_FORMAT").unwrap_or_default();
    let drain = match format.as_str() {
        "json" => wrap(mk_json()),
        _ => wrap(mk_term()),
    };

    Logger::root(
        drain,
        o!(
            "version" => env!("CARGO_PKG_VERSION"),
            "commit" => option_env!("GIT_COMMIT_HASH").unwrap_or("unknown"),
        ),
    )
});
