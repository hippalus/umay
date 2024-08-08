use crate::app::signal_handler;
use crate::balance::discovery::DnsDiscovery;
use crate::balance::{Backends, LoadBalancer, Selector};
use crate::proxy::ProxyService;
use crate::tls;
use rustls::pki_types::ServerName;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use rustls::ServerConfig;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, watch};
use tower::Service;

const EXIT_TIMEOUT: u64 = 60 * 5;
const CLOSE_TIMEOUT: u64 = 5;

pub struct Server {
    proxy_service: ProxyService,
    shutdown_watch: watch::Sender<bool>,
    shutdown_recv: watch::Receiver<bool>,
    listen_addr: SocketAddr,
}

impl Server {
    pub fn new(
        listen_addr: SocketAddr,
        upstream_host: String,
        upstream_port: u16,
        config: Arc<ServerConfig>,
    ) -> anyhow::Result<Self> {
        let (_, tls_config_rx) = watch::channel(config);
        let tls_server = tls::Server::new(ServerName::try_from("localhost")?, tls_config_rx);

        let discovery = Box::new(DnsDiscovery::new(upstream_host, upstream_port)?);
        let backends = Backends::new(discovery);
        let load_balancer = Arc::new(LoadBalancer::new(
            backends,
            Selector::RoundRobin(Arc::new(tokio::sync::Mutex::new(0))),
        ));

        let proxy_service = ProxyService::new(tls_server, load_balancer);

        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        Ok(Server {
            proxy_service,
            shutdown_watch: shutdown_tx,
            shutdown_recv: shutdown_rx,
            listen_addr,
        })
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(self.listen_addr).await?;
        tracing::info!("Listening on {}", self.listen_addr);

        let lb = self.proxy_service.load_balancer().clone();
        let lb_refresh_handle = tokio::spawn(async move {
            lb.start_refresh_task(Duration::from_secs(60)).await;
        });

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        tokio::spawn(signal_handler(shutdown_tx));

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((socket, _)) => {
                            let mut service = self.proxy_service.clone();
                            tokio::spawn(async move {
                                if let Err(e) = service.call(socket).await {
                                    tracing::error!("Error handling connection: {:?}", e);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("Error accepting connection: {:?}", e);
                        }
                    }
                }
                _ = &mut shutdown_rx => {
                    tracing::info!("Shutdown signal received, starting graceful shutdown");
                    break;
                }
            }
        }

        self.shutdown_watch
            .send(true)
            .expect("Failed to send shutdown signal");
        lb_refresh_handle.abort();

        tokio::time::sleep(Duration::from_secs(CLOSE_TIMEOUT)).await;
        tracing::info!("Graceful shutdown: grace period {}s starts", EXIT_TIMEOUT);
        tokio::time::sleep(Duration::from_secs(EXIT_TIMEOUT)).await;
        tracing::info!("Graceful shutdown: grace period ends");

        Ok(())
    }
}
