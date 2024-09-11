use core::fmt::Debug;
use std::io::Cursor;
use std::sync::Arc;

use crate::app::config::TlsConfig;
use eyre::{Context, Result};
use rustls::client::{ResolvesClientCert, Resumption};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::server::{ClientHello, ResolvesServerCert, WebPkiClientVerifier};
use rustls::sign::CertifiedKey;
use rustls::{crypto, RootCertStore, SignatureScheme};
use rustls_pemfile::{certs, private_key};
use tokio_rustls::rustls::client::danger::ServerCertVerifier;
use tokio_rustls::rustls::client::WebPkiServerVerifier;
use tracing::debug;

#[derive(Debug)]
pub struct Store {
    server_name: ServerName<'static>,
    server_cert_verifier: Arc<dyn ServerCertVerifier + Send + Sync>,
    client_cfg: Arc<rustls::ClientConfig>,
    server_cfg: Arc<rustls::ServerConfig>,
}

#[derive(Clone, Debug)]
struct CertResolver(Arc<CertifiedKey>);

impl ResolvesClientCert for CertResolver {
    fn resolve(
        &self,
        _root_hint_subjects: &[&[u8]],
        _sigschemes: &[SignatureScheme],
    ) -> Option<Arc<CertifiedKey>> {
        Some(Arc::clone(&self.0))
    }

    fn has_certs(&self) -> bool {
        true
    }
}

impl ResolvesServerCert for CertResolver {
    fn resolve(&self, _client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(Arc::clone(&self.0))
    }
}

impl TryFrom<&TlsConfig> for Store {
    type Error = eyre::Error;

    fn try_from(value: &TlsConfig) -> std::result::Result<Self, Self::Error> {
        Self::new(
            ServerName::try_from("default.default.serviceaccount.identity.umay.cluster.local")?,
            value.proxy_tls_trusted_certificate()?,
            value.proxy_tls_certificate()?,
            value.proxy_tls_certificate_key()?,
            vec![],
        )
    }
}
impl Store {
    pub fn new(
        server_name: ServerName<'static>,
        roots_pem: Vec<u8>,
        server_cert: Vec<u8>,
        key: Vec<u8>,
        intermediates: Vec<Vec<u8>>,
    ) -> Result<Self> {
        debug!("Creating new Store instance");

        let roots = Self::create_root_store(&roots_pem)?;
        let certified_key = Self::create_certified_key(server_cert, key, intermediates)?;
        let resolver = Arc::new(CertResolver(certified_key));

        let cert_verifier = WebPkiServerVerifier::builder(roots.clone()).build()?;
        let client_cfg = Self::create_client_config(cert_verifier.clone(), resolver.clone())?;
        let server_cfg = Self::create_server_config(&roots, resolver.clone())?;

        Ok(Self {
            server_cert_verifier: cert_verifier,
            server_name,
            client_cfg,
            server_cfg,
        })
    }

    fn create_root_store(roots_pem: &[u8]) -> Result<Arc<RootCertStore>> {
        let mut roots = RootCertStore::empty();
        let certs = certs(&mut Cursor::new(std::str::from_utf8(roots_pem)?))
            .collect::<std::result::Result<Vec<CertificateDer<'static>>, _>>()?;

        if certs.is_empty() {
            return Err(eyre::eyre!("No certificates found in the chain file"));
        }
        roots.add_parsable_certificates(certs);
        Ok(Arc::new(roots))
    }

    fn create_certified_key(
        server_cert: Vec<u8>,
        key: Vec<u8>,
        intermediates: Vec<Vec<u8>>,
    ) -> Result<Arc<CertifiedKey>> {
        let mut chain = vec![CertificateDer::from(server_cert.as_slice()).into_owned()];
        chain.extend(
            intermediates
                .into_iter()
                .map(|der| CertificateDer::from(der.as_slice()).into_owned()),
        );

        let private_key = Self::extract_private_key(key)?;
        let signing_key = crypto::aws_lc_rs::sign::any_ecdsa_type(&private_key)?;
        Ok(Arc::new(CertifiedKey::new(chain, signing_key)))
    }

    fn extract_private_key(key: Vec<u8>) -> Result<PrivateKeyDer<'static>> {
        let mut reader = Cursor::new(key);
        private_key(&mut reader)
            .wrap_err("Failed to read private key")?
            .into_iter()
            .next()
            .ok_or_else(|| eyre::eyre!("No private key found in key file"))
    }

    fn create_client_config(
        server_cert_verifier: Arc<WebPkiServerVerifier>,
        resolver: Arc<CertResolver>,
    ) -> Result<Arc<rustls::ClientConfig>> {
        let mut client_cfg = rustls::ClientConfig::builder()
            .with_webpki_verifier(server_cert_verifier)
            .with_client_cert_resolver(resolver);

        client_cfg.resumption = Resumption::disabled();

        Ok(Arc::new(client_cfg))
    }

    fn create_server_config(
        roots: &Arc<RootCertStore>,
        resolver: Arc<CertResolver>,
    ) -> Result<Arc<rustls::ServerConfig>> {
        let client_cert_verifier = WebPkiClientVerifier::builder(roots.clone())
            .allow_unauthenticated()
            .build()?;

        let server_cfg = rustls::ServerConfig::builder()
            .with_client_cert_verifier(client_cert_verifier)
            .with_cert_resolver(resolver);

        Ok(Arc::new(server_cfg))
    }

    pub fn server_name(&self) -> &ServerName<'static> {
        &self.server_name
    }

    pub fn client_cfg(&self) -> Arc<rustls::ClientConfig> {
        Arc::clone(&self.client_cfg)
    }

    pub fn server_cfg(&self) -> Arc<rustls::ServerConfig> {
        Arc::clone(&self.server_cfg)
    }
}
