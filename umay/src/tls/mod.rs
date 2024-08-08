use std::sync::Arc;
use std::task::{Context, Poll};

use anyhow::Result;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::server::TlsStream;
use tower::Service;

use pin_project::pin_project;
use std::future::Future;
use std::{io, pin::Pin};
use tokio::sync::watch;

pub mod credentials;
pub mod pki;

#[derive(Clone, Debug)]
pub struct ClientId(pub Vec<u8>);

pub enum ServerTls {
    Established {
        client_id: Option<ClientId>,
        negotiated_protocol: Option<NegotiatedProtocol>,
    },
    Passthru {
        sni: ServerName<'static>,
    },
}

#[derive(Clone, Debug)]
pub struct NegotiatedProtocol(pub Vec<u8>);

#[derive(Clone)]
pub struct Server {
    name: ServerName<'static>,
    rx: watch::Receiver<Arc<ServerConfig>>,
}

#[pin_project]
pub struct TerminateFuture<I> {
    #[pin]
    future: tokio_rustls::Accept<I>,
}

impl Server {
    pub fn new(name: ServerName<'static>, rx: watch::Receiver<Arc<ServerConfig>>) -> Self {
        Self { name, rx }
    }

    pub async fn spawn_with_alpn(self, alpn_protocols: Vec<Vec<u8>>) -> Result<Self, io::Error> {
        if alpn_protocols.is_empty() {
            return Ok(self);
        }

        let mut orig_rx = self.rx;

        let mut config = (**orig_rx.borrow_and_update()).clone();
        config.alpn_protocols = alpn_protocols.clone();
        let (tx, rx) = watch::channel(Arc::new(config));

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tx.closed() => return,
                    res = orig_rx.changed() => {
                        if res.is_err() {
                            return;
                        }
                    }
                }

                let mut config = (*orig_rx.borrow().clone()).clone();
                config.alpn_protocols = alpn_protocols.clone();
                let _ = tx.send(Arc::new(config));
            }
        });

        Ok(Self::new(self.name, rx))
    }
}

impl<I> Service<I> for Server
where
    I: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Sync + Unpin + 'static,
{
    type Response = (ServerTls, TlsStream<I>);
    type Error = anyhow::Error;
    type Future = TerminateFuture<I>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, io: I) -> Self::Future {
        let acceptor = tokio_rustls::TlsAcceptor::from((*self.rx.borrow()).clone());
        TerminateFuture {
            future: acceptor.accept(io),
        }
    }
}

impl<I> Future for TerminateFuture<I>
where
    I: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Sync + Unpin + 'static,
{
    type Output = anyhow::Result<(ServerTls, TlsStream<I>)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> std::task::Poll<Self::Output> {
        let this = self.project();
        let tls_stream = futures::ready!(this.future.poll(cx))?;

        let client_id = client_identity(&tls_stream);
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

fn client_identity<I>(tls: &TlsStream<I>) -> Option<ClientId> {
    let (_io, session) = tls.get_ref();
    session
        .peer_certificates()
        .and_then(|certs| certs.first().map(|cert| ClientId(cert.as_ref().to_vec())))
}
