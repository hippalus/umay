use socket2::TcpKeepalive;
use std::time::Duration;
use tokio::net::TcpStream;

pub mod config;
pub mod metric;
pub mod server;
pub mod signal;

fn set_nodelay_or_warn(socket: &TcpStream) {
    if let Err(e) = socket.set_nodelay(true) {
        tracing::warn!("failed to set nodelay: {}", e);
    }
}

fn set_keepalive_or_warn(
    tcp: tokio::net::TcpStream,
    keepalive_duration: Option<Duration>,
) -> eyre::Result<tokio::net::TcpStream> {
    let sock = {
        let stream: std::net::TcpStream = tokio::net::TcpStream::into_std(tcp)?;
        socket2::Socket::from(stream)
    };

    let ka = keepalive_duration
        .into_iter()
        .fold(TcpKeepalive::new(), |k, t| k.with_time(t));

    if let Err(e) = sock.set_tcp_keepalive(&ka) {
        tracing::warn!("failed to set keepalive: {}", e);
    }

    let stream: std::net::TcpStream = socket2::Socket::into(sock);
    Ok(tokio::net::TcpStream::from_std(stream)?)
}
