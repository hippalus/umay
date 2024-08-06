use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use once_cell::sync::Lazy;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;
use tracing::{error, info};

use crate::test::TestPki;
use crate::tls::RustlsServer;

pub static TEST_PKI: Lazy<Arc<TestPki>> = Lazy::new(|| Arc::new(TestPki::default()));

pub struct ProxyServer {
    shutdown_signal: Arc<AtomicBool>,
    shutdown_complete_tx: mpsc::UnboundedSender<()>,
}

impl ProxyServer {
    pub fn new(
        shutdown_signal: Arc<AtomicBool>,
        shutdown_complete_tx: mpsc::UnboundedSender<()>,
    ) -> Self {
        Self {
            shutdown_signal,
            shutdown_complete_tx,
        }
    }

    pub async fn run(&self, proxy_addr: &str, backend_addr: &str) -> Result<()> {
        let server = RustlsServer::new(
            ServerName::try_from("0.0.0.0").unwrap(),
            TEST_PKI.server_config(),
        );

        let listener = TcpListener::bind(proxy_addr).await?;

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    if self.shutdown_signal.load(Ordering::SeqCst) {
                        break;
                    }

                    match accept_result {
                        Ok((socket, _)) => {
                            let tls_acceptor = match server.tls_acceptor() {
                                Ok(acceptor) => acceptor,
                                Err(e) => {
                                    error!("Failed to create TLS acceptor: {:?}", e);
                                    continue;
                                }
                            };
                            let backend_addr = backend_addr.to_string();

                            tokio::spawn(Self::handle_connection(socket, tls_acceptor, backend_addr));
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {:?}", e);
                        }
                    }
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                    if self.shutdown_signal.load(Ordering::SeqCst) {
                        break;
                    }
                }
            }
        }

        info!("Server is shutting down...");
        let _ = self.shutdown_complete_tx.send(());
        Ok(())
    }

    async fn handle_connection(
        socket: TcpStream,
        tls_acceptor: Arc<TlsAcceptor>,
        backend_addr: String,
    ) {
        match tls_acceptor.accept(socket).await {
            Ok(inbound) => match TcpStream::connect(backend_addr).await {
                Ok(outbound) => {
                    if let Err(e) = Self::proxy(inbound, outbound).await {
                        error!("Proxy error: {:?}", e);
                    }
                }
                Err(e) => error!("Failed to connect to backend: {:?}", e),
            },
            Err(e) => error!("Failed to accept TLS connection: {:?}", e),
        }
    }

    async fn proxy(inbound: TlsStream<TcpStream>, outbound: TcpStream) -> Result<()> {
        let (mut inbound, mut outbound) = (inbound, outbound);

        io::copy_bidirectional(&mut inbound, &mut outbound).await?;
        Ok(())
    }

    pub fn shutdown(&self) {
        self.shutdown_signal.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::oneshot;
    use tokio::time::timeout;
    use tokio_rustls::{rustls, TlsConnector};
    use tracing::{error, info};

    use super::*;

    #[tokio::test]
    async fn test_proxy() -> Result<()> {
        let proxy_addr = "127.0.0.1:8883";
        let backend_addr = "127.0.0.1:1883";
        let (shutdown_complete_tx, mut shutdown_complete_rx) = mpsc::unbounded_channel();
        let shutdown_signal = Arc::new(AtomicBool::new(false));
        let server = Arc::new(ProxyServer::new(
            shutdown_signal.clone(),
            shutdown_complete_tx,
        ));

        // Start the backend server
        let (backend_shutdown_tx, backend_shutdown_rx) = oneshot::channel();
        let backend_handle =
            tokio::spawn(start_backend(backend_addr.to_string(), backend_shutdown_rx));

        // Start the proxy server
        let proxy = Arc::clone(&server);
        let proxy_handle = tokio::spawn(async move {
            if let Err(e) = proxy.run(proxy_addr, backend_addr).await {
                error!("Proxy server failed: {:?}", e);
            }
        });

        // Give the servers time to start
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Test client communication
        let mut client_stream = start_client(proxy_addr).await?;
        let msg = b"Hello, backend!";
        client_stream.write_all(msg).await?;
        let mut buf = vec![0; 1024];
        let n = client_stream.read(&mut buf).await?;

        assert_eq!(
            &buf[..n],
            msg,
            "Received message does not match sent message"
        );
        info!("Client received: {:?}", String::from_utf8_lossy(&buf[..n]));

        // Shutdown the proxy server
        server.shutdown();

        // Wait for the proxy server to complete shutdown
        timeout(Duration::from_secs(5), shutdown_complete_rx.recv())
            .await
            .expect("Proxy server didn't shut down in time")
            .expect("Proxy server shutdown channel closed unexpectedly");

        // Signal the backend server to shutdown
        backend_shutdown_tx
            .send(())
            .expect("Failed to send backend shutdown signal");

        // Wait for both servers to complete their shutdown
        timeout(Duration::from_secs(5), proxy_handle)
            .await
            .expect("Proxy handle didn't complete in time")?;

        timeout(Duration::from_secs(5), backend_handle)
            .await
            .expect("Backend handle didn't complete in time")
            .expect("Backend handle panicked")?;

        Ok(())
    }

    async fn start_backend(addr: String, mut shutdown_rx: oneshot::Receiver<()>) -> Result<()> {
        let listener = TcpListener::bind(&addr).await?;
        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((mut socket, _)) => {
                            tokio::spawn(async move {
                                let mut buffer = [0; 1024];
                                loop {
                                    match socket.read(&mut buffer).await {
                                        Ok(0) => break,
                                        Ok(n) => {
                                            if let Err(e) = socket.write_all(&buffer[..n]).await {
                                                error!("Error writing to socket: {:?}", e);
                                                break;
                                            }
                                        },
                                        Err(e) => {
                                            error!("Error reading from socket: {:?}", e);
                                            break;
                                        }
                                    }
                                }
                            });
                        }
                        Err(e) => error!("Error accepting connection: {:?}", e),
                    }
                }
                _ = &mut shutdown_rx => {
                    info!("Backend server is shutting down...");
                    break;
                }
            }
        }
        Ok(())
    }

    async fn start_client(proxy_addr: &str) -> Result<tokio_rustls::client::TlsStream<TcpStream>> {
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(TEST_PKI.roots.clone())
            .with_no_client_auth();

        let connector = TlsConnector::from(Arc::new(config));
        let domain = ServerName::try_from("localhost").unwrap();

        let stream = TcpStream::connect(proxy_addr).await?;
        let stream = connector.connect(domain, stream).await?;

        Ok(stream)
    }
}
