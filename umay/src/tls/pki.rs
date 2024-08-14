use rcgen::{
    Certificate, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose, SerialNumber,
};
use rustls::pki_types::PrivatePkcs8KeyDer;
use rustls::{RootCertStore, ServerConfig};
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

pub struct TestPki {
    pub roots: Arc<RootCertStore>,
    pub ca_cert: (Certificate, KeyPair),
    pub client_cert: (Certificate, KeyPair),
    pub server_cert: (Certificate, KeyPair),
}

impl Default for TestPki {
    fn default() -> Self {
        let ca_cert = Self::create_ca_cert();
        let server_cert = Self::create_server_cert(&ca_cert.0, &ca_cert.1);
        let client_cert = Self::create_client_cert(&ca_cert.0, &ca_cert.1);
        let roots = Self::create_root_cert_store(&ca_cert.0);

        Self {
            roots: Arc::new(roots),
            ca_cert,
            client_cert,
            server_cert,
        }
    }
}

impl TestPki {
    fn create_ca_cert() -> (Certificate, KeyPair) {
        let alg = &rcgen::PKCS_ECDSA_P256_SHA256;
        let mut params = CertificateParams::new(Vec::new()).unwrap();
        params
            .distinguished_name
            .push(DnType::OrganizationName, "Rustls Server Acceptor");
        params
            .distinguished_name
            .push(DnType::CommonName, "Test CA");
        params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::CrlSign,
        ];

        let key_pair = KeyPair::generate_for(alg).unwrap();
        let cert = params.self_signed(&key_pair).unwrap();
        (cert, key_pair)
    }

    fn create_server_cert(ca_cert: &Certificate, ca_key: &KeyPair) -> (Certificate, KeyPair) {
        let mut params = CertificateParams::new(vec!["localhost".to_string()]).unwrap();
        params.is_ca = IsCa::NoCa;
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

        let key_pair = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let cert = params.signed_by(&key_pair, ca_cert, ca_key).unwrap();
        (cert, key_pair)
    }

    fn create_client_cert(ca_cert: &Certificate, ca_key: &KeyPair) -> (Certificate, KeyPair) {
        let mut params = CertificateParams::new(Vec::new()).unwrap();
        params
            .distinguished_name
            .push(DnType::CommonName, "Test Client");
        params.is_ca = IsCa::NoCa;
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        params.serial_number = Some(SerialNumber::from(vec![0xC0, 0xFF, 0xEE]));

        let key_pair = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let cert = params.signed_by(&key_pair, ca_cert, ca_key).unwrap();
        (cert, key_pair)
    }

    fn create_root_cert_store(ca_cert: &Certificate) -> RootCertStore {
        let mut roots = RootCertStore::empty();
        roots.add(ca_cert.der().clone()).unwrap();
        roots
    }

    pub fn write_certs_and_keys(&self, directory: &str) -> anyhow::Result<()> {
        fs::create_dir_all(directory)?;

        self.write_cert_and_key(directory, "ca", &self.ca_cert.0, &self.ca_cert.1)?;
        self.write_cert_and_key(
            directory,
            "server",
            &self.server_cert.0,
            &self.server_cert.1,
        )?;
        self.write_cert_and_key(
            directory,
            "client",
            &self.client_cert.0,
            &self.client_cert.1,
        )?;

        self.write_full_chain(directory)?;

        Ok(())
    }

    fn write_cert_and_key(
        &self,
        directory: &str,
        prefix: &str,
        cert: &Certificate,
        key: &KeyPair,
    ) -> std::io::Result<()> {
        let cert_path = Path::new(directory).join(format!("{}.crt", prefix));
        let key_path = Path::new(directory).join(format!("{}.key", prefix));

        fs::write(cert_path, cert.der())?;
        fs::write(key_path, key.serialize_pem())?;

        Ok(())
    }

    fn write_full_chain(&self, directory: &str) -> anyhow::Result<()> {
        let chain_path = Path::new(directory).join("ca-chain.pem");
        let mut file = File::create(chain_path)?;

        file.write_all(self.ca_cert.0.pem().as_bytes())?;
        file.write_all(self.server_cert.0.pem().as_bytes())?;
        file.write_all(self.client_cert.0.pem().as_bytes())?;

        Ok(())
    }
    pub fn server_config(&self) -> Arc<ServerConfig> {
        let mut server_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![self.server_cert.0.der().clone()],
                PrivatePkcs8KeyDer::from(self.server_cert.1.serialize_der()).into(),
            )
            .unwrap();

        server_config.key_log = Arc::new(rustls::KeyLogFile::new());

        Arc::new(server_config)
    }
}
