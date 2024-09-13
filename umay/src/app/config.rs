use config::{Environment, File};
use eyre::{Context, Result};
use hickory_resolver::config::{NameServerConfig, ResolverConfig, ResolverOpts};
use hickory_resolver::Name;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::net::SocketAddr;
use std::time::Duration;
use std::{env, fs};
use tracing::warn;

const CONFIG_BASE_PATH: &str = "config/";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UmayConfig {
    worker_threads: usize,
    close_timeout: u64,
    exit_timeout: u64,
    shutdown_grace_period: u64,
    stream: Option<StreamConfig>, // Optional stream config
    http: Option<HttpConfig>,     // Optional http config
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StreamConfig {
    upstreams: HashMap<String, Upstream>, // Dynamic upstreams
    servers: Vec<StreamServer>,
}

impl StreamConfig {
    pub fn upstream(&self, key: &str) -> Option<&Upstream> {
        self.upstreams.get(key)
    }

    pub fn servers(&self) -> &Vec<StreamServer> {
        self.servers.as_ref()
    }

    pub fn new(upstreams: HashMap<String, Upstream>, servers: Vec<StreamServer>) -> Self {
        Self { upstreams, servers }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Upstream {
    load_balancer: LoadBalancer,
    service_discovery: ServiceDiscovery,
    servers: Vec<UpstreamServer>,
}

impl Upstream {
    pub fn service_discovery(&self) -> &ServiceDiscovery {
        &self.service_discovery
    }

    pub fn load_balancer(&self) -> &LoadBalancer {
        &self.load_balancer
    }

    pub fn servers(&self) -> &Vec<UpstreamServer> {
        self.servers.as_ref()
    }

    pub fn new(
        load_balancer: LoadBalancer,
        service_discovery: ServiceDiscovery,
        servers: Vec<UpstreamServer>,
    ) -> Self {
        Self {
            load_balancer,
            service_discovery,
            servers,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpstreamServer {
    address: String,
    port: u16,
}

impl UpstreamServer {
    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn to_socket_addrs(&self) -> Result<SocketAddr> {
        Ok(SocketAddr::new(self.address.parse()?, self.port))
    }

    pub fn new(address: String, port: u16) -> Self {
        Self { address, port }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StreamServer {
    name: String,
    listen: ListenConfig,
    proxy_pass: String, // The proxy_pass is now a string that maps to a dynamic upstream
    tls: Option<TlsConfig>, // TLS configuration encapsulated here
}

impl StreamServer {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn listen(&self) -> &ListenConfig {
        &self.listen
    }

    pub fn proxy_pass(&self) -> &str {
        &self.proxy_pass
    }

    pub fn tls(&self) -> Option<&TlsConfig> {
        self.tls.as_ref()
    }

    pub fn new(
        name: String,
        listen: ListenConfig,
        proxy_pass: String,
        tls: Option<TlsConfig>,
    ) -> Self {
        Self {
            name,
            listen,
            proxy_pass,
            tls,
        }
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn set_listen(&mut self, listen: ListenConfig) {
        self.listen = listen;
    }

    pub fn set_proxy_pass(&mut self, proxy_pass: String) {
        self.proxy_pass = proxy_pass;
    }

    pub fn set_tls(&mut self, tls: Option<TlsConfig>) {
        self.tls = tls;
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ListenConfig {
    port: u16,
    protocol: Protocol,
}

impl ListenConfig {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn protocol(&self) -> &Protocol {
        &self.protocol
    }

    pub fn new(port: u16, protocol: Protocol) -> Self {
        Self { port, protocol }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TlsConfig {
    enabled: bool,
    proxy_tls_certificate: String,
    proxy_tls_certificate_key: String,
    proxy_tls_trusted_certificate: String,
    proxy_tls_verify: bool,
    proxy_tls_verify_depth: usize,
    proxy_tls_session_reuse: bool,
    proxy_tls_protocols: Vec<String>,
    proxy_tls_ciphers: String,
}

impl TlsConfig {
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn proxy_tls_certificate(&self) -> eyre::Result<Vec<u8>> {
        let mut file = fs::File::open(&self.proxy_tls_certificate)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        Ok(buffer)
    }

    pub fn proxy_tls_certificate_key(&self) -> eyre::Result<Vec<u8>> {
        let mut file = fs::File::open(&self.proxy_tls_certificate_key)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        Ok(buffer)
    }

    pub fn proxy_tls_trusted_certificate(&self) -> eyre::Result<Vec<u8>> {
        let mut file = fs::File::open(&self.proxy_tls_trusted_certificate)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        Ok(buffer)
    }

    pub fn proxy_tls_verify(&self) -> bool {
        self.proxy_tls_verify
    }

    pub fn proxy_tls_verify_depth(&self) -> usize {
        self.proxy_tls_verify_depth
    }

    pub fn proxy_tls_session_reuse(&self) -> bool {
        self.proxy_tls_session_reuse
    }

    pub fn proxy_tls_protocols(&self) -> &Vec<String> {
        &self.proxy_tls_protocols
    }

    pub fn proxy_tls_ciphers(&self) -> &str {
        &self.proxy_tls_ciphers
    }

    pub fn new(
        enabled: bool,
        proxy_tls_certificate: String,
        proxy_tls_certificate_key: String,
        proxy_tls_trusted_certificate: String,
        proxy_tls_verify: bool,
        proxy_tls_verify_depth: usize,
        proxy_tls_session_reuse: bool,
        proxy_tls_protocols: Vec<String>,
        proxy_tls_ciphers: String,
    ) -> Self {
        Self {
            enabled,
            proxy_tls_certificate,
            proxy_tls_certificate_key,
            proxy_tls_trusted_certificate,
            proxy_tls_verify,
            proxy_tls_verify_depth,
            proxy_tls_session_reuse,
            proxy_tls_protocols,
            proxy_tls_ciphers,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HttpConfig {
    upstreams: HashMap<String, Upstream>, // Dynamic upstreams
    servers: Vec<HttpServer>,
}

impl HttpConfig {
    pub fn upstreams(&self) -> &HashMap<String, Upstream> {
        &self.upstreams
    }

    pub fn servers(&self) -> &Vec<HttpServer> {
        &self.servers
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HttpServer {
    name: String,
    listen: ListenConfig,
    tls: Option<TlsConfig>, // TLS configuration encapsulated here
    proxy_pass: String,     // Maps to the dynamic upstream in the HashMap
    location: LocationConfig,
    proxy_http_version: String,
    proxy_set_header: String,
    keepalive_timeout: usize,
}

impl HttpServer {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn listen(&self) -> &ListenConfig {
        &self.listen
    }

    pub fn tls(&self) -> Option<&TlsConfig> {
        self.tls.as_ref()
    }

    pub fn proxy_pass(&self) -> &str {
        &self.proxy_pass
    }

    pub fn location(&self) -> &LocationConfig {
        &self.location
    }

    pub fn proxy_http_version(&self) -> &str {
        &self.proxy_http_version
    }

    pub fn proxy_set_header(&self) -> &str {
        &self.proxy_set_header
    }

    pub fn keepalive_timeout(&self) -> usize {
        self.keepalive_timeout
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn set_listen(&mut self, listen: ListenConfig) {
        self.listen = listen;
    }

    pub fn set_tls(&mut self, tls: Option<TlsConfig>) {
        self.tls = tls;
    }

    pub fn set_proxy_pass(&mut self, proxy_pass: String) {
        self.proxy_pass = proxy_pass;
    }

    pub fn set_location(&mut self, location: LocationConfig) {
        self.location = location;
    }

    pub fn set_proxy_http_version(&mut self, proxy_http_version: String) {
        self.proxy_http_version = proxy_http_version;
    }

    pub fn set_proxy_set_header(&mut self, proxy_set_header: String) {
        self.proxy_set_header = proxy_set_header;
    }

    pub fn set_keepalive_timeout(&mut self, keepalive_timeout: usize) {
        self.keepalive_timeout = keepalive_timeout;
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocationConfig {
    path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Tcp,
    Udp,
    Ws,
    Http,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum LoadBalancer {
    RoundRobin,
    LeastConn,
    Random,
    IpHash,
    WeightedRoundRobin,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ServiceDiscovery {
    Dns,
    Local,
}

impl UmayConfig {
    pub fn load() -> Result<Self> {
        let config_path = Self::get_env_var(
            "CONFIG_BASE_PATH",
            CONFIG_BASE_PATH,
            "CONFIG_BASE_PATH not set. Using the default environment: {}",
        );

        let config = Self::get_config(&config_path)?;

        let config: UmayConfig = config
            .try_deserialize::<Self>()
            .wrap_err("Failed to load configuration")?;

        config.validate()?;

        Ok(config)
    }

    fn get_config(config_path: &str) -> eyre::Result<config::Config> {
        config::Config::builder()
            .add_source(File::with_name(&format!("{}umay.yaml", config_path)))
            .add_source(Environment::with_prefix("UMAY").separator("_"))
            .build()
            .wrap_err("Failed to build configuration")
    }

    fn validate(&self) -> Result<()> {
        if self.stream.is_none() && self.http.is_none() {
            eyre::bail!("At least one of 'stream' or 'http' configurations must be present.");
        }
        Ok(())
    }

    pub fn get_upstream(&self, name: &str) -> Option<&Upstream> {
        if let Some(stream) = &self.stream {
            if let Some(upstream) = stream.upstreams.get(name) {
                return Some(upstream);
            }
        }

        if let Some(http) = &self.http {
            if let Some(upstream) = http.upstreams.get(name) {
                return Some(upstream);
            }
        }
        None
    }

    fn get_env_var(var: &str, default: &str, warning: &str) -> String {
        env::var(var).unwrap_or_else(|_| {
            warn!("{}", warning);
            default.to_string()
        })
    }

    pub fn worker_threads(&self) -> usize {
        self.worker_threads
    }

    pub fn close_timeout(&self) -> Duration {
        Duration::from_secs(self.close_timeout)
    }

    pub fn exit_timeout(&self) -> Duration {
        Duration::from_secs(self.exit_timeout)
    }

    pub fn shutdown_grace_period(&self) -> Duration {
        Duration::from_secs(self.shutdown_grace_period)
    }

    pub fn stream(&self) -> Option<&StreamConfig> {
        self.stream.as_ref()
    }

    pub fn http(&self) -> Option<&HttpConfig> {
        self.http.as_ref()
    }

    pub fn new(
        worker_threads: usize,
        close_timeout: u64,
        exit_timeout: u64,
        shutdown_grace_period: u64,
        stream: Option<StreamConfig>,
        http: Option<HttpConfig>,
    ) -> Self {
        Self {
            worker_threads,
            close_timeout,
            exit_timeout,
            shutdown_grace_period,
            stream,
            http,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct DnsConfig {
    nameservers: Option<Vec<String>>,
    search: Vec<String>,
    ndots: Option<usize>,
}

impl DnsConfig {
    pub fn into_resolver_config(self) -> eyre::Result<(ResolverConfig, ResolverOpts)> {
        let mut config = ResolverConfig::new();
        let mut opts = ResolverOpts::default();

        if let Some(nameservers) = self.nameservers {
            for ns in nameservers {
                let socket_addr: SocketAddr = ns.parse().wrap_err("Invalid nameserver address")?;
                config.add_name_server(NameServerConfig::new(
                    socket_addr,
                    hickory_resolver::config::Protocol::Udp,
                ));
            }
        }

        for domain in self.search {
            let name: Name = domain.parse().wrap_err("Invalid search domain")?;
            config.add_search(name);
        }

        if let Some(ndts) = self.ndots {
            opts.ndots = ndts;
        }

        Ok((config, opts))
    }
}
