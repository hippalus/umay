use crate::app::config::{Protocol, StreamServer};
use crate::balance::LoadBalancer;
use crate::tls::server::{Server, TlsTerminator};
use crate::tls::ServerTls;
use eyre::Result;
use futures::future::BoxFuture;
use std::sync::Arc;
use std::task::Poll;
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tower::Service;
use tracing::{debug, error, info};

pub struct StreamProxy {
    stream_config: Arc<StreamServer>,
    tls_server: Arc<Server>,
    load_balancer: Arc<LoadBalancer>,
}

impl StreamProxy {
    pub fn new(
        stream_config: Arc<StreamServer>,
        tls_server: Arc<Server>,
        load_balancer: Arc<LoadBalancer>,
    ) -> Self {
        Self {
            stream_config,
            tls_server,
            load_balancer,
        }
    }

    async fn handle_connection<IO>(&self, client_io: IO) -> Result<()>
    where
        IO: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Sync + Unpin + 'static,
    {
        let (server_tls, tls_stream) = self.tls_server.terminate(client_io).await?;

        match server_tls {
            ServerTls::Established {
                client_id,
                negotiated_protocol,
            } => {
                info!(
                    "Established TLS connection: {:?} {:?}",
                    client_id, negotiated_protocol
                );
            }
            ServerTls::Passthru { sni } => {
                info!("Passthrough connection with SNI: {:?}", sni);
            }
        }

        match self.load_balancer.select(None).await {
            Some(backend) => {
                debug!("Selected backend: {:?}", backend);
                match self.stream_config.listen().protocol().clone() {
                    Protocol::Tcp => {
                        let upstream = TcpStream::connect(backend.addr).await?;
                        self.proxy(tls_stream, upstream).await?;
                    }
                    Protocol::Udp => {}
                    Protocol::Wss => {}
                    Protocol::Https => {}
                }
            }
            None => return Err(eyre::eyre!("No backends available")),
        }

        Ok(())
    }

    async fn proxy<IO>(&self, client: TlsStream<IO>, server: TcpStream) -> Result<()>
    where
        IO: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Sync + Unpin + 'static,
    {
        let (mut client_reader, mut client_writer) = tokio::io::split(client);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        let client_to_server = tokio::io::copy(&mut client_reader, &mut server_writer);
        let server_to_client = tokio::io::copy(&mut server_reader, &mut client_writer);

        tokio::select! {
            result = client_to_server => {
                if let Err(e) = result {
                    error!("Error in client to server communication: {:?}", e);
                }
            }
            result = server_to_client => {
                if let Err(e) = result {
                    error!("Error in server to client communication: {:?}", e);
                }
            }
        }

        Ok(())
    }

    pub fn load_balancer(&self) -> Arc<LoadBalancer> {
        Arc::clone(&self.load_balancer)
    }

    pub fn port(&self) -> u16 {
        self.stream_config.listen().port()
    }
}

impl<IO> Service<IO> for StreamProxy
where
    IO: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Sync + Unpin + 'static,
{
    type Response = ();
    type Error = eyre::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: IO) -> Self::Future {
        let this = self.clone();
        Box::pin(async move { this.handle_connection(req).await })
    }
}

impl Clone for StreamProxy {
    fn clone(&self) -> Self {
        Self {
            stream_config: Arc::clone(&self.stream_config),
            tls_server: Arc::clone(&self.tls_server),
            load_balancer: Arc::clone(&self.load_balancer),
        }
    }
}
