use std::net::SocketAddr;

use rustls::pki_types::ServerName;
use rustls::RootCertStore;
use rustls_pemfile::certs;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_rustls::TlsConnector;
use umay::app::config::{AppConfig, ServiceConfig};
use umay::app::server::Server;
use webpki::types::IpAddr;

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
    let ca_cert = include_bytes!("../tests/resources/ca.pem").to_vec();

    let ca_cert = certs(&mut std::io::Cursor::new(ca_cert)).next().unwrap()?;

    let mut roots = RootCertStore::empty();
    roots.add(ca_cert)?;

    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    let connector = TlsConnector::from(Arc::new(config));
    let domain =
        ServerName::try_from("default.default.serviceaccount.identity.umay.cluster.local")?; // Ensure this matches the certificate's CN or SAN

    let stream = TcpStream::connect(proxy_addr).await?;
    let stream = connector.connect(domain, stream).await?;

    Ok(stream)
}

#[tokio::test]
async fn test_proxy_integration() -> anyhow::Result<()> {
    let upstream_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 1994);
    let proxy_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 9994);

    let (backend_shutdown_tx, backend_shutdown_rx) = oneshot::channel();
    let upstream_handle = tokio::spawn(start_backend(upstream_addr, backend_shutdown_rx));

    let config = test_config();

    let server = Server::build(config.clone())?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.spawn(shutdown_rx).await {
            tracing::error!("Server error: {:?}", e);
        }
    });

    // Allow some time for the servers to start
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

    // Shutdown backend and proxy servers
    backend_shutdown_tx
        .send(())
        .expect("Failed to send backend shutdown signal");
    shutdown_tx
        .send(())
        .expect("Failed to send shutdown signal");
    server_handle.await?;

    // Wait for the backend server to complete
    tokio::time::timeout(Duration::from_secs(10), upstream_handle)
        .await??
        .expect("Failed to complete upstream_handle");

    Ok(())
}

fn test_config() -> Arc<AppConfig> {
    let service_config = ServiceConfig::new(
        "default.default.serviceaccount.identity.umay.cluster.local".to_string(),
        9994,
        "tests/resources/default-default-ca/crt.der".to_string(),
        "tests/resources/default-default-ca/key.pem".to_string(),
        "tests/resources/ca.pem".to_string(),
        "localhost".to_string(),
        1994,
        "dns".to_string(),
        100,
        "round_robin".to_string(),
    );

    let app_config = AppConfig::new(vec![service_config], 3, 1, 1, 1);

    Arc::new(app_config)
}
