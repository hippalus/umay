use crate::app::server::UmayServer;
use crate::app::signal;
use crate::config::UmayConfig;
use eyre::WrapErr;
use std::sync::Arc;
use tokio::runtime::Builder;
use tracing::level_filters::LevelFilter;
use tracing::{error, info};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter, Registry};

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

mod app;
mod balance;
mod config;
mod proxy;
mod tls;

fn main() -> eyre::Result<()> {
    init_logger(LevelFilter::DEBUG);

    let config = Arc::new(UmayConfig::load()?);

    let runtime = build_runtime(config.worker_threads())?;
    runtime.block_on(run_server(config))
}

fn init_logger(default_level: LevelFilter) {
    let filter_layer = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level.to_string()));

    let fmt_layer = fmt::layer()
        .with_thread_names(true)
        .with_thread_ids(true)
        .with_line_number(true)
        .compact();

    Registry::default()
        .with(filter_layer)
        .with(fmt_layer)
        .init();
}

fn build_runtime(worker_threads: usize) -> eyre::Result<tokio::runtime::Runtime> {
    Builder::new_multi_thread()
        .enable_all()
        .thread_name("umay-runtime-worker")
        .worker_threads(worker_threads)
        .max_blocking_threads(worker_threads)
        .build()
        .wrap_err("Failed to build runtime")
}

async fn run_server(config: Arc<UmayConfig>) -> eyre::Result<()> {
    match UmayServer::try_from(config.clone()) {
        Ok(umay) => {
            let shutdown_signal = signal::shutdown().await;
            match umay.run(shutdown_signal).await {
                Ok(_) => info!("Server shutdown gracefully"),
                Err(err) => error!("Server shutdown with error: {}", err),
            }
        }
        Err(err) => {
            error!("Failed to initialize Umay server: {}", err);
        }
    }
    Ok(())
}
