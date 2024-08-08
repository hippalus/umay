use std::net::SocketAddr;

use once_cell::sync::Lazy;
use rustls::pki_types::ServerName;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_rustls::TlsConnector;
use umay::app::server::Server;
use umay::tls::pki::TestPki;

static TEST_PKI: Lazy<Arc<TestPki>> = Lazy::new(|| Arc::new(TestPki::default()));

async fn start_backend(
    addr: SocketAddr,
    mut shutdown_rx: oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
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
                                            tracing::error!("Error writing to socket: {:?}", e);
                                            break;
                                        }
                                    },
                                    Err(e) => {
                                        tracing::error!("Error reading from socket: {:?}", e);
                                        break;
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => tracing::error!("Error accepting connection: {:?}", e),
                }
            }
            _ = &mut shutdown_rx => {
                tracing::info!("Backend server is shutting down...");
                break;
            }
        }
    }
    Ok(())
}

async fn start_client(
    proxy_addr: SocketAddr,
) -> anyhow::Result<tokio_rustls::client::TlsStream<TcpStream>> {
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(TEST_PKI.roots.clone())
        .with_no_client_auth();

    let connector = TlsConnector::from(Arc::new(config));
    let domain = ServerName::try_from("localhost")?;

    let stream = TcpStream::connect(proxy_addr).await?;
    let stream = connector.connect(domain, stream).await?;

    Ok(stream)
}

#[tokio::test]
async fn test_proxy_integration() -> anyhow::Result<()> {
    let upstream_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 1883);
    let (backend_shutdown_tx, backend_shutdown_rx) = oneshot::channel();
    let upstream_handle = tokio::spawn(start_backend(upstream_addr, backend_shutdown_rx));

    let proxy_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 8883);
    let server = Server::new(
        proxy_addr,
        upstream_addr.ip().to_string(),
        upstream_addr.port(),
        TEST_PKI.server_config(),
    )?;

    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.run().await {
            tracing::error!("Server error: {:?}", e);
        }
    });

    tokio::time::sleep(Duration::from_secs(1)).await;

    let mut client_stream = start_client(proxy_addr).await?;
    let msg = b"Hello, Proxy!";
    client_stream.write_all(msg).await?;

    let mut buf = [0; 1024];
    let n = client_stream.read(&mut buf).await?;

    assert_eq!(
        &buf[..n],
        msg,
        "Received message does not match sent message"
    );
    tracing::info!("Client received: {:?}", String::from_utf8_lossy(&buf[..n]));

    backend_shutdown_tx
        .send(())
        .expect("Failed to send backend shutdown signal");
    server_handle.abort();

    tokio::time::timeout(Duration::from_secs(10), upstream_handle)
        .await??
        .expect("Failed to complete upstream_handle");

    Ok(())
}
