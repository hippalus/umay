use std::collections::HashMap;
use std::net::SocketAddr;

use rustls::pki_types::ServerName;
use rustls::RootCertStore;
use rustls_pemfile::certs;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, watch};
use tokio_rustls::TlsConnector;
use umay::app::server::UmayServer;
use umay::config::{
    ListenConfig, LoadBalancer, Protocol, ServiceDiscovery, StreamConfig, StreamServer, TlsConfig,
    UmayConfig, Upstream, UpstreamServer,
};

async fn start_backend(
    addr: SocketAddr,
    mut shutdown_rx: oneshot::Receiver<()>,
) -> eyre::Result<()> {
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
) -> eyre::Result<tokio_rustls::client::TlsStream<TcpStream>> {
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
async fn test_proxy_integration() -> eyre::Result<()> {
    let upstream_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 1994);
    let proxy_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 9994);

    let (backend_shutdown_tx, backend_shutdown_rx) = oneshot::channel();
    let upstream_handle = tokio::spawn(start_backend(upstream_addr, backend_shutdown_rx));

    let config = test_config();

    let server: UmayServer = UmayServer::try_from(config.clone())?;
    let (shutdown_tx, shutdown_rx) = watch::channel(());
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.run(shutdown_rx).await {
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
fn test_config() -> Arc<UmayConfig> {
    let upstream_server = UpstreamServer::new("localhost".to_string(), 1994);

    let upstream = Upstream::new(
        LoadBalancer::RoundRobin,
        ServiceDiscovery::Dns,
        vec![upstream_server],
    );

    let tls_config = TlsConfig::new(
        true,                                                     // TLS enabled
        "tests/resources/default-default-ca/crt.der".to_string(), // TLS certificate path
        "tests/resources/default-default-ca/key.pem".to_string(), // TLS certificate key path
        "tests/resources/ca.pem".to_string(),                     // Trusted CA certificate
        true,                                                     // Verify peer certificate
        2,                                                        // Verify depth
        true,                                                     // Session reuse enabled
        vec!["TLSv1.2".to_string(), "TLSv1.3".to_string()],       // Supported TLS protocols
        "HIGH:!aNULL:!MD5".to_string(),                           // Cipher suites
    );

    let stream_server = StreamServer::new(
        "default.default.serviceaccount.identity.umay.cluster.local".to_string(),
        ListenConfig::new(9994, Protocol::Tcp),
        "localhost".to_string(),
        Some(tls_config),
    );

    let stream_config = StreamConfig::new(
        HashMap::from([("localhost".to_string(), upstream)]),
        vec![stream_server],
    );

    let umay_config = UmayConfig::new(
        4,                   // Worker threads
        1,                   // Close timeout in seconds
        1,                   // Exit timeout in seconds
        1,                   // Shutdown grace period in seconds
        Some(stream_config), // Stream configuration
        None,                // HTTP configuration (None for now)
    );

    Arc::new(umay_config)
}
