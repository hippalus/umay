use crate::app::config::{AppConfig, ServiceConfig};
use crate::app::metric::Metrics;
use crate::balance::discovery::{DnsDiscovery, LocalDiscovery, ServiceDiscovery};
use crate::balance::selection::SelectionAlgorithm;
use crate::balance::{selection, Backends, LoadBalancer};
use crate::proxy::ProxyService;
use crate::tls;
use crate::tls::credentials::Store;
use anyhow::{anyhow, Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::oneshot::Receiver;
use tower::Service;
use tracing::{error, info};

pub struct Server {
    proxy_service: ProxyService,
    config: Arc<AppConfig>,
    metrics: Arc<Metrics>,
}

impl Server {
    pub fn build(config: Arc<AppConfig>) -> Result<Self> {
        let service_config = config.get_first_service_config()?;

        let server_name = service_config.server_name()?.to_owned();
        let store = Store::new(
            server_name.clone(),
            service_config.roots_ca()?,
            service_config.cert()?,
            service_config.key()?,
            vec![],
        )?;

        let tls_server = initialize_tls_server(&store)?;
        let load_balancer = initialize_load_balancer(&service_config)?;

        let proxy_service = ProxyService::new(tls_server, load_balancer);

        Ok(Self {
            proxy_service,
            config,
            metrics: Arc::new(Metrics::new("umay".to_string(), 1.0)),
        })
    }

    pub async fn spawn(self, mut shutdown_rx: Receiver<()>) -> Result<()> {
        let service_config = self.config.get_first_service_config()?;

        let listener = bind_listener(service_config.port()).await?;

        info!("Listening on 0.0.0.0:{}", service_config.port());
        self.start_load_balancer_refresh(service_config.discovery_refresh_interval());

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((socket, _)) => {
                            let mut service = self.proxy_service.clone();
                            tokio::spawn(async move {
                                if let Err(e) = service.call(socket).await {
                                    error!("Error handling connection: {:?}", e);
                                }
                            });
                        }
                        Err(e) => error!("Error accepting connection: {:?}", e),
                    }
                }
                _ = &mut shutdown_rx => {
                    info!("Shutdown signal received, starting graceful shutdown");
                    self.shutdown().await;
                    break;
                }
            }
        }

        Ok(())
    }

    fn start_load_balancer_refresh(&self, refresh_interval: Duration) {
        let lb = self.proxy_service.load_balancer().clone();
        tokio::spawn(async move {
            lb.start_refresh_task(refresh_interval).await;
        });
    }

    pub async fn shutdown(&self) {
        info!(
            "Graceful shutdown: grace period {:?} starts",
            self.config.shutdown_grace_period()
        );

        tokio::time::sleep(self.config.shutdown_grace_period()).await;

        tokio::time::sleep(self.config.exit_timeout()).await;
        info!("Graceful shutdown: grace period ends");
    }
}

async fn bind_listener(port: u16) -> Result<TcpListener> {
    let listen_addr = format!("0.0.0.0:{}", port);
    TcpListener::bind(&listen_addr)
        .await
        .context(format!("Failed to bind to address: {}", listen_addr))
}

fn initialize_tls_server(store: &Store) -> Result<Arc<tls::server::Server>> {
    Ok(Arc::new(tls::server::Server::new(
        store.server_name().to_owned(),
        store.server_cfg(),
    )))
}

fn initialize_load_balancer(service_config: &ServiceConfig) -> Result<Arc<LoadBalancer>> {
    let discovery = create_discovery(service_config)?;
    let backends = Backends::new(discovery);
    let selector = create_selector(service_config)?;
    Ok(Arc::new(LoadBalancer::new(backends, selector)))
}

fn create_discovery(
    config: &ServiceConfig,
) -> Result<Box<dyn ServiceDiscovery + Send + Sync + 'static>> {
    match config.discovery_type() {
        "dns" => Ok(Box::new(DnsDiscovery::new(
            config.upstream_host().to_owned(),
            config.upstream_port(),
            config.dns_config().cloned(),
        )?)),
        "local" => Ok(Box::new(LocalDiscovery::with_backends(vec![
            config.upstream_addr()?
        ]))),
        _ => Err(anyhow!(
            "Invalid discovery type: {}",
            config.discovery_type()
        )),
    }
}

fn create_selector(config: &ServiceConfig) -> Result<Arc<dyn SelectionAlgorithm + Send + Sync>> {
    match config.load_balancer_selection() {
        "random" => Ok(Arc::new(selection::Random)),
        "round_robin" => Ok(Arc::new(selection::RoundRobin::default())),
        "weighted_round_robin" => Ok(Arc::new(selection::WeightedRoundRobin::default())),
        "least_connection" => Ok(Arc::new(selection::LeastConnections::default())),
        _ => Err(anyhow!(
            "Invalid load balancer selection: {}",
            config.load_balancer_selection()
        )),
    }
}
