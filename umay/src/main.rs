use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tokio::runtime::Builder;
use tokio::sync::mpsc;

use crate::proxy::ProxyServer;

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

mod credentials;
mod proxy;
mod test;
mod tls;

fn main() -> anyhow::Result<()> {
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_name("umay")
        .worker_threads(4)
        .max_blocking_threads(4)
        .build()
        .expect("failed to build runtime!");

    let _guard = runtime.enter();
    let _ = runtime.block_on(async {
        let (shutdown_tx, shutdown_rx) = mpsc::unbounded_channel();
        let shutdown_signal = Arc::new(AtomicBool::new(false));
        let server = ProxyServer::new(shutdown_signal.clone(), shutdown_tx);

        server.run("0.0.0.0:8883", "0.0.0.0:1883").await
    });
    Ok(())
}
