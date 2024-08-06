use std::sync::Arc;

use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

pub struct RustlsServer {
    name: ServerName<'static>,
    config: Arc<ServerConfig>,
}

impl RustlsServer {
    pub fn new(name: ServerName<'static>, config: Arc<ServerConfig>) -> Self {
        Self { name, config }
    }

    pub fn tls_acceptor(&self) -> anyhow::Result<Arc<TlsAcceptor>> {
        Ok(Arc::new(TlsAcceptor::from(Arc::clone(&self.config))))
    }
}
