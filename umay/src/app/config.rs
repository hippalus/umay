#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppConfig {
    pub services: Vec<ServiceConfig>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceConfig {
    pub ip: String,
    pub port: u16,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
    pub backend: Vec<String>,
}
