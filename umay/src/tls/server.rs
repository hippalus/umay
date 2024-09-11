use crate::tls;
use crate::tls::{NegotiatedProtocol, ServerTls};
use async_trait::async_trait;
use pin_project::pin_project;
use rustls::pki_types::ServerName;
use rustls::ServerConfig;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;
use tower::Service;

#[async_trait]
pub trait TlsTerminator<I>: Send + Sync {
    async fn terminate(&self, stream: I) -> eyre::Result<(ServerTls, TlsStream<I>)>;
}

#[derive(Clone)]
pub struct Server {
    name: ServerName<'static>,
    acceptor: Arc<TlsAcceptor>,
}

#[async_trait]
impl<I> TlsTerminator<I> for Server
where
    I: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Sync + Unpin + 'static,
{
    async fn terminate(&self, stream: I) -> eyre::Result<(ServerTls, TlsStream<I>)> {
        let mut server = self.clone();
        server.call(stream).await
    }
}

impl Server {
    pub fn new(name: ServerName<'static>, config: Arc<ServerConfig>) -> Self {
        let acceptor = Arc::new(TlsAcceptor::from(config));
        Self { name, acceptor }
    }
}

impl<I> Service<I> for Server
where
    I: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Sync + Unpin + 'static,
{
    type Response = (ServerTls, TlsStream<I>);
    type Error = eyre::Error;
    type Future = TerminateFuture<I>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<eyre::Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, io: I) -> Self::Future {
        TerminateFuture {
            future: self.acceptor.accept(io),
        }
    }
}

#[pin_project]
pub struct TerminateFuture<I> {
    #[pin]
    future: tokio_rustls::Accept<I>,
}

impl<I> Future for TerminateFuture<I>
where
    I: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Sync + Unpin + 'static,
{
    type Output = eyre::Result<(ServerTls, TlsStream<I>)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let tls_stream = futures::ready!(this.future.poll(cx))?;

        let sni = tls_stream.get_ref().1.server_name().map(|s| s.to_owned());
        if let Some(sni) = sni {
            let server_tls = ServerTls::Passthru {
                sni: ServerName::try_from(sni).expect("Invalid SNI"),
            };
            return Poll::Ready(Ok((server_tls, tls_stream)));
        }

        let client_id = tls::client_identity(&tls_stream);
        let negotiated_protocol = tls_stream
            .get_ref()
            .1
            .alpn_protocol()
            .map(|b| NegotiatedProtocol(b.to_vec()));

        let server_tls = ServerTls::Established {
            client_id,
            negotiated_protocol,
        };

        Poll::Ready(Ok((server_tls, tls_stream)))
    }
}
