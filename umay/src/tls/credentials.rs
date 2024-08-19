use core::fmt::Debug;
use std::io::Cursor;
use std::sync::Arc;

use anyhow::{Context, Result};
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
    server_cert_verifier: Arc<dyn ServerCertVerifier>,
    client_cfg: Arc<rustls::ClientConfig>,
    server_cfg: Arc<rustls::ServerConfig>,
}

#[derive(Clone, Debug)]
struct CertResolver(Arc<CertifiedKey>);

#[derive(Clone)]
struct Key(Arc<PrivateKeyDer<'static>>);

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

impl Store {
    pub fn new(
        server_name: ServerName<'static>,
        roots_pem: Vec<u8>,
        server_cert: Vec<u8>,
        key: Vec<u8>,
        intermediates: Vec<Vec<u8>>,
    ) -> Result<Self> {
        debug!("Creating new Store instance");

        let mut roots = RootCertStore::empty();
        let certs = certs(&mut Cursor::new(std::str::from_utf8(roots_pem.as_ref())?))
            .collect::<std::result::Result<Vec<CertificateDer<'static>>, _>>()?;

        if certs.is_empty() {
            return Err(anyhow::anyhow!("No certificates found in the chain file"));
        }
        roots.add_parsable_certificates(certs.clone());
        let roots = Arc::new(roots);

        let mut chain = Vec::with_capacity(intermediates.len() + 1);
        chain.push(CertificateDer::from(server_cert.as_slice()).into_owned());
        chain.extend(
            intermediates
                .into_iter()
                .map(|der| CertificateDer::from(der.as_slice()).into_owned()),
        );

        let mut reader = Cursor::new(key);
        let private_key = private_key(&mut reader)
            .context("Failed to read private key")?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No private key found in key file"))?;

        let signing_key = crypto::aws_lc_rs::sign::any_ecdsa_type(&private_key)?;
        let certified_key = Arc::new(CertifiedKey::new(chain, signing_key));
        let resolver = Arc::new(CertResolver(certified_key));

        let server_cert_verifier = WebPkiServerVerifier::builder(roots.clone()).build()?;

        let mut client_cfg = rustls::ClientConfig::builder()
            .with_webpki_verifier(Arc::clone(&server_cert_verifier))
            .with_client_cert_resolver(resolver.clone());
        //.with_no_client_auth();

        client_cfg.resumption = Resumption::disabled();
        let client_cfg = client_cfg.into();

        let client_cert_verifier = WebPkiClientVerifier::builder(roots.clone())
            .allow_unauthenticated()
            .build()?;

        let server_cfg = rustls::ServerConfig::builder()
            .with_client_cert_verifier(client_cert_verifier)
            .with_cert_resolver(resolver.clone())
            .into();

        Ok(Self {
            server_cert_verifier,
            server_name,
            client_cfg,
            server_cfg,
        })
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
