use crate::app::config::{Protocol, StreamServer};
use crate::balance::LoadBalancer;
use crate::tls::server::{Server, TlsTerminator};
use crate::tls::ServerTls;
use eyre::Result;
use futures::future::BoxFuture;
use futures::SinkExt;
use std::sync::Arc;
use std::task::Poll;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tokio_stream::StreamExt;
use tokio_tungstenite::{accept_async, connect_async, MaybeTlsStream, WebSocketStream};
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

    //TODO : make this function as tower Service and implement the call method
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

        //TODO: make this section tower layer and implement the call method
        match self.load_balancer.select(None).await {
            Some(backend) => {
                debug!("Selected backend: {:?}", backend);
                match self.stream_config.listen().protocol().clone() {
                    Protocol::Tcp => {
                        let upstream = TcpStream::connect(backend.addr).await?;
                        // TODO:: make this function as tower Service and implement the call method
                        self.proxy_tcp(tls_stream, upstream).await?;
                    }
                    Protocol::Ws => {
                        let client_ws = accept_async(tls_stream).await?;
                        let upstream_url =
                            format!("ws://{}:{}", backend.addr.ip(), backend.addr.port());
                        let (upstream_ws, response) = connect_async(&upstream_url).await?;
                        debug!("Connected to upstream: {:?}", response);
                        // TODO:: make this function as tower Service and implement the call method
                        self.proxy_ws(client_ws, upstream_ws).await?;
                    }
                    _ => {
                        return Err(eyre::eyre!("Unsupported protocol"));
                    }
                }
            }
            None => return Err(eyre::eyre!("No backends available")),
        }

        Ok(())
    }

    // TODO:: make this function as tower Service and implement the call method
    async fn proxy_tcp<IO>(&self, client: TlsStream<IO>, server: TcpStream) -> Result<()>
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

    // TODO:: make this function as tower Service and implement the call method
    async fn proxy_ws<IO>(
        &self,
        mut client_ws: WebSocketStream<IO>,
        mut upstream_ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    ) -> Result<()>
    where
        IO: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
    {
        loop {
            tokio::select! {
                Some(client_message) = client_ws.next() => {
                    let client_message = client_message?;
                    upstream_ws.send(client_message).await?;
                }
                Some(upstream_message) = upstream_ws.next() => {
                    let upstream_message = upstream_message?;
                    client_ws.send(upstream_message).await?;
                }
            }
        }
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
