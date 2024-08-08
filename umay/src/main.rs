extern crate alloc;

use std::net::SocketAddr;

use crate::app::server::Server;
use tokio::runtime::Builder;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};
use crate::tls::pki::TEST_PKI;

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

mod app;
mod balance;
mod proxy;
mod tls;

fn main() -> anyhow::Result<()> {
    init_logger(LevelFilter::DEBUG);

    let runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_name("umay")
        .worker_threads(4)
        .max_blocking_threads(4)
        .build()
        .expect("failed to build runtime!");

    let listen_addr: SocketAddr = "0.0.0.0:8883".parse()?;
    let upstream_host = "localhost".to_string();
    let upstream_port = 1883;

    let server = Server::new(
        listen_addr,
        upstream_host,
        upstream_port,
        TEST_PKI.server_config(),
    )?;

    runtime.block_on(server.run())
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
