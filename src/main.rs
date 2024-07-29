use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Builder;

fn main() -> anyhow::Result<()> {
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_name("proxy")
        .worker_threads(4)
        .max_blocking_threads(2)
        .build()
        .expect("failed to build threaded runtime!");

    runtime.block_on(run_proxy("127.0.0.1:8080", "127.0.0.1:8081"))
}

async fn run_proxy(proxy_addr: &str, backend_addr: &str) -> anyhow::Result<()> {
    let listener = TcpListener::bind(proxy_addr).await?;

    loop {
        let (socket, _) = listener.accept().await?;
        let backend_addr = backend_addr.to_string();
        tokio::spawn(async move {
            if let Err(e) = forward(socket, &backend_addr).await {
                eprintln!("Failed to handle client: {}", e);
            }
        });
    }
}

async fn forward(mut inbound: TcpStream, backend_addr: &str) -> anyhow::Result<()> {
    let mut outbound = TcpStream::connect(backend_addr).await?;

    let (mut ri, mut wi) = inbound.split();
    let (mut ro, mut wo) = outbound.split();

    let client_to_server = async {
        tokio::io::copy(&mut ri, &mut wo).await?;
        wo.shutdown().await
    };

    let server_to_client = async {
        tokio::io::copy(&mut ro, &mut wi).await?;
        wi.shutdown().await
    };

    tokio::try_join!(client_to_server, server_to_client)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    use super::*;

    #[tokio::test]
    async fn test_tcp_forwarding() {
        // Setup backend server
        let backend_addr = "127.0.0.1:8081";
        let listener = TcpListener::bind(backend_addr).await.unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0; 1024];
            let n = socket.read(&mut buf).await.unwrap();
            socket.write_all(&buf[0..n]).await.unwrap();
        });


        tokio::spawn(async move {
            run_proxy("127.0.0.1:8080", "127.0.0.1:8081").await.unwrap();
        });

        // FIXME: wait for proxy to start
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Setup client
        let client_addr = "127.0.0.1:8080";
        let mut client = TcpStream::connect(client_addr).await.unwrap();
        let msg = b"Hello, world!";
        client.write_all(msg).await.unwrap();

        let mut buf = [0; 1024];
        let n = client.read(&mut buf).await.unwrap();


        assert_eq!(&buf[0..n], msg);
        assert_eq!(String::from_utf8(buf[0..n].to_vec().clone()).unwrap(), "Hello, world!".to_string());
    }
}