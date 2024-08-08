use crate::balance::{Backend, LoadBalancer};
use crate::tls::{Server as TlsServer, ServerTls};
use anyhow::Result;
use futures::future::BoxFuture;
use std::sync::Arc;
use std::task::Poll;
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tower::Service;
use tracing::info;

pub struct ProxyService {
    tls_terminator: TlsServer,
    load_balancer: Arc<LoadBalancer>,
}

impl ProxyService {
    pub fn new(tls_terminator: TlsServer, load_balancer: Arc<LoadBalancer>) -> Self {
        Self {
            tls_terminator,
            load_balancer,
        }
    }

    async fn handle_connection(&self, client: TcpStream) -> Result<()> {
        let (server_tls, tls_stream) = self.tls_terminator.clone().call(client).await?;

        if let ServerTls::Established {
            client_id,
            negotiated_protocol,
        } = server_tls
        {
            info!(
                ?client_id,
                ?negotiated_protocol,
                "TLS connection established"
            );
        }

        if let Some(backend) = self.load_balancer.select(None).await {
            self.proxy(tls_stream, backend).await?;
        } else {
            return Err(anyhow::anyhow!("No backend available"));
        }

        Ok(())
    }

    async fn proxy(&self, client: TlsStream<TcpStream>, backend: Backend) -> Result<()> {
        let mut server = TcpStream::connect(backend.addr).await?;

        let (mut client_reader, mut client_writer) = tokio::io::split(client);
        let (mut server_reader, mut server_writer) = server.split();

        let client_to_server = tokio::io::copy(&mut client_reader, &mut server_writer);
        let server_to_client = tokio::io::copy(&mut server_reader, &mut client_writer);

        tokio::select! {
            result = client_to_server => {
                if let Err(e) = result {
                    tracing::error!("Error in client to server communication: {:?}", e);
                }
            }
            result = server_to_client => {
                if let Err(e) = result {
                    tracing::error!("Error in server to client communication: {:?}", e);
                }
            }
        }

        Ok(())
    }

    pub fn load_balancer(&self) -> Arc<LoadBalancer> {
        Arc::clone(&self.load_balancer)
    }
}

impl Service<TcpStream> for ProxyService {
    type Response = ();
    type Error = anyhow::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: TcpStream) -> Self::Future {
        let this = self.clone();
        Box::pin(async move { this.handle_connection(req).await })
    }
}

impl Clone for ProxyService {
    fn clone(&self) -> Self {
        Self {
            tls_terminator: self.tls_terminator.clone(),
            load_balancer: Arc::clone(&self.load_balancer),
        }
    }
}
