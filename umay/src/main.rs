extern crate alloc;

use crate::app::config::AppConfig;
use crate::app::server::Server;
use crate::app::signal;
use std::sync::Arc;
use tokio::runtime::Builder;
use tracing::level_filters::LevelFilter;
use tracing::{error, info};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

mod app;
mod balance;
mod proxy;
mod tls;

fn main() -> anyhow::Result<()> {
    init_logger(LevelFilter::DEBUG);
    let config = Arc::new(AppConfig::new()?);

    let runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_name("umay")
        .worker_threads(config.worker_threads())
        .max_blocking_threads(config.worker_threads())
        .build()
        .expect("failed to build runtime!");
    let _ = runtime.enter();

    runtime.block_on(async move {
        let server_result = Server::build(config);
        let server = match server_result {
            Ok(server) => server,
            Err(e) => {
                error!("Initialization failure: {}", e);
                std::process::exit(1);
            }
        };

        let shutdown_rx = signal::shutdown().await;

        let drain = server.spawn(shutdown_rx).await;
        match drain {
            Ok(_) => {
                info!("Server shutdown successfully");
            }
            Err(e) => {
                error!("Server shutdown failed: {}", e);
            }
        }
    });

    Ok(())
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
