use std::io::Cursor;
use std::sync::Arc;

use anyhow::{Context, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::server::WebPkiClientVerifier;
use rustls::RootCertStore;
use rustls_pemfile::private_key;
use tokio_rustls::rustls::client::danger::ServerCertVerifier;
use tokio_rustls::rustls::client::WebPkiServerVerifier;
use tracing::warn;

//TODO refactor for mTLS support
#[derive(Debug)]
pub struct Store {
    certificate: Certificate,
    server_cert_verifier: Arc<dyn ServerCertVerifier>,
    server_name: ServerName<'static>,
    client_cfg: Arc<rustls::ClientConfig>,
    server_cfg: Arc<rustls::ServerConfig>,
}
impl Store {
    pub fn new(key: &[u8], cert: &[u8], chain: Vec<&[u8]>) -> anyhow::Result<Self> {
        let certificate = Certificate::new(key, cert, chain)?;

        let cert_chain = certificate.chain();
        let roots = certificate.roots();
        let key = certificate.private_key();

        let server_cert_verifier = WebPkiServerVerifier::builder(Arc::clone(&roots)).build()?;

        let client_cfg = rustls::ClientConfig::builder()
            .with_webpki_verifier(Arc::clone(&server_cert_verifier))
            .with_no_client_auth();

        // let resolver = Arc::new(ResolvesServerCertUsingSni::new());
        //  resolver.add("localhost", rustls::sign::CertifiedKey::new(certs, Arc::new(key)))?;

        let server_cfg = Self::server_config(roots, cert_chain.clone(), key.clone_key())?;

        Ok(Self {
            certificate,
            server_cert_verifier,
            server_name: ServerName::try_from("localhost").unwrap(),
            client_cfg: Arc::new(client_cfg),
            server_cfg,
        })
    }
    fn server_config(
        roots: Arc<RootCertStore>,
        cert_chain: Vec<CertificateDer<'static>>,
        key_der: PrivateKeyDer<'static>,
        // cert_resolver: Arc<dyn ResolvesServerCert>,
    ) -> anyhow::Result<Arc<rustls::ServerConfig>> {
        let client_cert_verifier = WebPkiClientVerifier::builder(roots)
            .allow_unauthenticated()
            .build()?;

        let config = rustls::ServerConfig::builder()
            .with_client_cert_verifier(client_cert_verifier)
            .with_single_cert(cert_chain, key_der)?;
        // .with_cert_resolver(cert_resolver);

        Ok(Arc::new(config))
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

#[derive(Debug)]
pub struct Certificate {
    cert: CertificateDer<'static>,
    chain: Vec<CertificateDer<'static>>,
    private_key: PrivateKeyDer<'static>,
    roots: Arc<RootCertStore>,
}

impl Certificate {
    pub fn new(key: &[u8], cert: &[u8], chain: Vec<&[u8]>) -> Result<Self> {
        let cert_chain = Self::parse_cert(cert)?;
        let private_key = Self::parse_key(key)?;

        let mut roots = RootCertStore::empty();
        for anchor in chain {
            let certs = Self::parse_cert(anchor)?;
            roots.add_parsable_certificates(certs);
        }

        Ok(Self {
            cert: cert_chain[0].clone(),
            chain: cert_chain,
            private_key,
            roots: Arc::new(roots),
        })
    }

    pub fn parse_cert(mut cert: &[u8]) -> Result<Vec<CertificateDer<'static>>> {
        let mut reader = std::io::BufReader::new(Cursor::new(&mut cert));
        let certs = rustls_pemfile::certs(&mut reader)
            .collect::<Result<Vec<CertificateDer<'static>>, _>>()
            .context("failed to parse certificates")?;

        Ok(certs)
    }

    pub fn parse_key(mut key: &[u8]) -> Result<PrivateKeyDer<'static>> {
        let mut reader = std::io::BufReader::new(Cursor::new(&mut key));
        let keys = private_key(&mut reader).context("failed to parse private key")?;

        keys.into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("no private key found"))
    }

    fn root_store(certs: Vec<CertificateDer>) -> anyhow::Result<RootCertStore> {
        let mut roots = RootCertStore::empty();
        let (_, skipped) = roots.add_parsable_certificates(certs);

        if skipped != 0 {
            warn!("Skipped {} invalid trust anchors", skipped);
        }

        Ok(roots)
    }

    pub fn chain(&self) -> &Vec<CertificateDer<'static>> {
        &self.chain
    }

    pub fn cert(&self) -> &CertificateDer<'static> {
        &self.cert
    }

    pub fn roots(&self) -> Arc<RootCertStore> {
        Arc::clone(&self.roots)
    }

    pub fn private_key(&self) -> &PrivateKeyDer<'static> {
        &self.private_key
    }
}
