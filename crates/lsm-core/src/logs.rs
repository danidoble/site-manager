//! Logging setup (`tracing` + file appender).

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize tracing: stderr + a file under `log_dir/app.log`.
///
/// Returns a [`WorkerGuard`] that must be held alive for the file writer to keep
/// flushing. Safe to call once per process; additional calls are no-ops.
pub fn init(log_dir: &Path) -> Option<WorkerGuard> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let stderr_layer = fmt::layer().with_target(false).with_writer(std::io::stderr);

    // Both branches fully inline so each `try_init()` sees a concrete subscriber
    // type (avoids `impl Layer<Registry>` not being `Layer<Layered<..>>`).
    if std::fs::create_dir_all(log_dir).is_ok() {
        let appender = tracing_appender::rolling::never(log_dir, "app.log");
        let (writer, guard) = tracing_appender::non_blocking(appender);
        let file_layer = fmt::layer().with_ansi(false).with_writer(writer);
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(stderr_layer)
            .with(file_layer)
            .try_init();
        return Some(guard);
    }

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(stderr_layer)
        .try_init();
    None
}
