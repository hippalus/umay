use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::server::WebPkiClientVerifier;
use rustls::RootCertStore;
use rustls_pemfile::private_key;
use tokio_rustls::rustls::client::danger::ServerCertVerifier;
use tokio_rustls::rustls::client::WebPkiServerVerifier;
use tracing::debug;

//TODO refactor for mTLS support
#[derive(Debug)]
pub struct Store {
    certificate: Certificate,
    server_cert_verifier: Arc<dyn ServerCertVerifier>,
    server_name: ServerName<'static>,
    client_cfg: Arc<rustls::ClientConfig>,
    server_cfg: Arc<rustls::ServerConfig>,
}

#[derive(Debug)]
pub struct Certificate {
    cert: CertificateDer<'static>,
    chain: Vec<CertificateDer<'static>>,
    private_key: PrivateKeyDer<'static>,
    roots: Arc<RootCertStore>,
}

impl Certificate {
    pub fn new<P: AsRef<Path>>(key_path: P, cert_path: P, root_cert: P) -> Result<Self> {
        debug!("Creating new Certificate instance");

        let server_cert = Self::parse_cert_file(&cert_path)?;
        let private_key = Self::parse_key_file(&key_path)?;
        let root_certs = Self::parse_chain_file(&root_cert)?;

        let mut roots = RootCertStore::empty();
        roots.add_parsable_certificates(root_certs.clone());

        Ok(Self {
            cert: server_cert,
            chain: root_certs,
            private_key,
            roots: Arc::new(roots),
        })
    }
    fn parse_cert_file<P: AsRef<Path>>(path: P) -> Result<CertificateDer<'static>> {
        let file = File::open(path.as_ref())
            .with_context(|| format!("Failed to open .crt file: {:?}", path.as_ref()))?;
        let mut reader = BufReader::new(file);

        let mut content = Vec::new();
        reader
            .read_to_end(&mut content)
            .context("Failed to read file content")?;

        debug!("File content size: {} bytes", content.len());

        if content.is_empty() {
            return Err(anyhow::anyhow!("The .crt file is empty"));
        }

        let cert = CertificateDer::from(content.as_slice());
        Ok(cert.into_owned())
    }

    fn parse_chain_file<P: AsRef<Path>>(path: P) -> Result<Vec<CertificateDer<'static>>> {
        let file = std::fs::File::open(path.as_ref())
            .with_context(|| format!("Failed to open certificate file: {:?}", path.as_ref()))?;
        let mut reader = std::io::BufReader::new(file);
        let certs = rustls_pemfile::certs(&mut reader)
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("Failed to parse certificates from file")?;

        if certs.is_empty() {
            return Err(anyhow::anyhow!("No certificates found in the file"));
        }

        debug!(
            "Successfully parsed {} certificates from {:?}",
            certs.len(),
            path.as_ref()
        );
        Ok(certs)
    }

    fn parse_key_file<P: AsRef<Path>>(path: P) -> Result<PrivateKeyDer<'static>> {
        let file = File::open(path.as_ref())
            .with_context(|| format!("Failed to open .key file: {:?}", path.as_ref()))?;
        let mut reader = BufReader::new(file);
        let keys =
            private_key(&mut reader).context("Failed to parse private key from .key file")?;
        let key = keys
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No private key found in .key file"))?;
        debug!("Successfully parsed private key from {:?}", path.as_ref());
        Ok(key)
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

impl Store {
    pub fn new<P: AsRef<Path>>(key_path: P, cert_path: P, chain_paths: P) -> Result<Self> {
        debug!("Creating new Store instance");
        let certificate = Certificate::new(key_path, cert_path, chain_paths)?;

        let cert = certificate.cert();
        let roots = certificate.roots();
        let key = certificate.private_key();

        let server_cert_verifier = WebPkiServerVerifier::builder(Arc::clone(&roots))
            .build()
            .context("Failed to build server cert verifier")?;

        let client_cfg = rustls::ClientConfig::builder()
            .with_webpki_verifier(Arc::clone(&server_cert_verifier))
            .with_no_client_auth();

        let server_cfg = Self::server_config(roots, vec![cert.clone()], key.clone_key())?;

        Ok(Self {
            certificate,
            server_cert_verifier,
            server_name: ServerName::try_from("localhost")
                .expect("'localhost' is always a valid ServerName"),
            client_cfg: Arc::new(client_cfg),
            server_cfg,
        })
    }

    fn server_config(
        roots: Arc<RootCertStore>,
        cert_chain: Vec<CertificateDer<'static>>,
        key_der: PrivateKeyDer<'static>,
        // cert_resolver: Arc<dyn ResolvesServerCert>,
    ) -> Result<Arc<rustls::ServerConfig>> {
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
