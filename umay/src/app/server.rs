use crate::app::config::{
    LoadBalancer as LoadBalancerConfig, Protocol, ServiceDiscovery as ServiceDiscoveryConfig,
    UmayConfig, Upstream,
};
use crate::app::metric::Metrics;
use crate::balance::discovery::{DnsDiscovery, LocalDiscovery, ServiceDiscovery};
use crate::balance::selection::SelectionAlgorithm;
use crate::balance::{selection, Backends, LoadBalancer};
use crate::proxy::http::HttpProxy;
use crate::proxy::stream::StreamProxy;
use crate::tls;
use crate::tls::credentials::Store;
use eyre::{eyre, Context, ContextCompat, OptionExt, Result};
use futures::StreamExt;
use selection::{LeastConnections, Random, RoundRobin, WeightedRoundRobin};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio_stream::wrappers::TcpListenerStream;
use tokio_stream::Stream;
use tower::Service;
use tracing::log::warn;
use tracing::{debug, error, info};

pub struct UmayServer {
    stream_proxies: Vec<StreamProxy>,
    http_proxies: Vec<HttpProxy>,
    config: Arc<UmayConfig>,
    metrics: Arc<Metrics>,
}

impl TryFrom<Arc<UmayConfig>> for UmayServer {
    type Error = eyre::Error;

    fn try_from(config: Arc<UmayConfig>) -> Result<Self> {
        let mut stream_proxies = vec![];

        if let Some(stream_config) = config.stream() {
            for stream_server in stream_config.servers() {
                let tls_config = stream_server
                    .tls()
                    .ok_or_eyre("No TLS configuration found")?;
                let store = Store::try_from(tls_config)?;
                let tls_server = initialize_tls_server(&store)?;

                let upstream = stream_config
                    .upstream(stream_server.proxy_pass())
                    .wrap_err("Failed to find upstream for stream server")?;
                let load_balancer = initialize_load_balancer(upstream)?;

                // Handle different protocols
                match stream_server.listen().protocol() {
                    Protocol::Tcp | Protocol::Ws => {
                        stream_proxies.push(StreamProxy::new(
                            Arc::new(stream_server.clone()),
                            tls_server,
                            load_balancer,
                        ));
                    }
                    Protocol::Udp => {
                        todo!() // UDP implementation
                    }
                    Protocol::Http => {
                        todo!() // HTTPS implementation
                    }
                }
            }
        }

        let http_proxies = vec![]; // For now, since HttpProxy isn't yet initialized

        Ok(Self {
            stream_proxies,
            http_proxies,
            config,
            metrics: Arc::new(Metrics::new("umay".to_string(), 1.0)),
        })
    }
}

impl UmayServer {
    pub async fn run(&self, mut shutdown_rx: watch::Receiver<()>) -> Result<()> {
        for stream_proxy in self.stream_proxies.iter().cloned() {
            let port = stream_proxy.port();
            stream_proxy
                .load_balancer()
                .start_refresh_task(Duration::from_secs(30));

            let receiver = shutdown_rx.clone();
            tokio::spawn(async move {
                if let Err(e) = Self::run_service(stream_proxy, port, receiver).await {
                    error!("Error running service on port {}: {:?}", port, e);
                }
            });
        }

        tokio::select! {
            _ = shutdown_rx.changed() => {
                info!("Shutdown signal received, starting graceful shutdown.");
                self.shutdown().await;
            }
        }

        Ok(())
    }

    async fn run_service<S>(
        service: S,
        port: u16,
        mut shutdown_rx: watch::Receiver<()>,
    ) -> Result<()>
    where
        S: Service<TcpStream, Response=(), Error=eyre::Error> + Clone + Send + 'static,
        S::Future: Send + 'static,
    {
        let mut tcp_listener_stream = bind_listener(port).await?;
        info!("Listening on 0.0.0.0:{}", port);

        loop {
            tokio::select! {
                connection = tcp_listener_stream.next() => {
                    match connection {
                       Some(Ok(socket)) => {
                            let mut service_clone = service.clone();
                            tokio::spawn(async move {
                                if let Err(e) = service_clone.call(socket).await {
                                    error!("Error handling connection: {:?}", e);
                                }
                            });
                        }
                        Some(Err(e)) => {
                            error!("Error accepting connection: {:?}", e);
                        }
                        None => {
                            warn!("No accepting connection");
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("Shutting down service on port {}", port);
                    break;
                }
            }
        }

        Ok(())
    }
    async fn shutdown(&self) {
        info!(
            "Graceful shutdown: grace period {:?} starts",
            self.config.shutdown_grace_period()
        );

        tokio::time::sleep(self.config.shutdown_grace_period()).await;

        tokio::time::sleep(self.config.exit_timeout()).await;
        info!("Graceful shutdown: grace period ends");
    }
}

async fn bind_listener(port: u16) -> Result<Pin<Box<dyn Stream<Item=Result<TcpStream>> + Send>>> {
    let listen_addr = format!("0.0.0.0:{}", port);
    let tcp_listener = {
        let std_tcp_listener = std::net::TcpListener::bind(&listen_addr)?;
        // Ensure non-blocking mode for Tokio
        std_tcp_listener.set_nonblocking(true)?;
        TcpListener::from_std(std_tcp_listener)
            .wrap_err(format!("Failed to bind to address: {}", listen_addr))?
    };

    let stream = TcpListenerStream::new(tcp_listener).map(|res| {
        let tcp = res
            .map_err(|e| eyre!(e))
            .wrap_err("Failed to accept connection")?;

        super::set_nodelay_or_warn(&tcp);
        let tcp = super::set_keepalive_or_warn(tcp, None).wrap_err("Failed to set keepalive")?;

        fn ipv4_mapped(orig: SocketAddr) -> SocketAddr {
            if let SocketAddr::V6(v6) = orig {
                if let Some(ip) = v6.ip().to_ipv4_mapped() {
                    return (ip, orig.port()).into();
                }
            }
            orig
        }

        let client_addr = tcp.peer_addr().wrap_err("Failed to get peer address")?;
        let client = ipv4_mapped(client_addr);
        debug!("Accepted connection from {}", client);

        Ok(tcp)
    });

    Ok(Box::pin(stream))
}

fn initialize_tls_server(store: &Store) -> Result<Arc<tls::server::Server>> {
    Ok(Arc::new(tls::server::Server::new(
        store.server_name().to_owned(),
        store.server_cfg(),
    )))
}

fn initialize_load_balancer(upstream: &Upstream) -> Result<Arc<LoadBalancer>> {
    let discovery = create_discovery(upstream)?;
    let backends = Backends::new(discovery);

    let balancer: LoadBalancerConfig = upstream.load_balancer().to_owned();
    let selector = create_selector(balancer)?;

    Ok(Arc::new(LoadBalancer::new(backends, selector)))
}

fn create_discovery(
    config: &Upstream,
) -> Result<Box<dyn ServiceDiscovery + Send + Sync + 'static>> {
    match config.service_discovery().clone() {
        ServiceDiscoveryConfig::Dns => {
            let us = config
                .servers()
                .iter()
                .next()
                .ok_or_else(|| eyre!("No servers found"))?;

            let discovery = DnsDiscovery::new(us.address().to_owned(), us.port(), None)?;

            Ok(Box::new(discovery))
        }
        ServiceDiscoveryConfig::Local => {
            let mut backends = vec![];
            for us in config.servers() {
                backends.push(us.to_socket_addrs()?);
            }
            Ok(Box::new(LocalDiscovery::with_backends(backends)))
        }
    }
}

fn create_selector(
    load_balancer: LoadBalancerConfig,
) -> Result<Arc<dyn SelectionAlgorithm + Send + Sync>> {
    match load_balancer {
        LoadBalancerConfig::Random => Ok(Arc::new(Random)),
        LoadBalancerConfig::RoundRobin => Ok(Arc::new(RoundRobin::default())),
        LoadBalancerConfig::WeightedRoundRobin => Ok(Arc::new(WeightedRoundRobin::default())),
        LoadBalancerConfig::LeastConn => Ok(Arc::new(LeastConnections::default())),
        LoadBalancerConfig::IpHash => todo!(),
    }
}
