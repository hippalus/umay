use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::server::TlsStream;

pub mod client;
pub mod credentials;
pub mod pki;
pub mod server;

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

fn client_identity<I>(tls_stream: &TlsStream<I>) -> Option<ClientId> {
    let (_io, session) = tls_stream.get_ref();
    session
        .peer_certificates()
        .and_then(|certs| certs.first().map(|cert| ClientId(cert.as_ref().to_vec())))
}
