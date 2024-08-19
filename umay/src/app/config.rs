use anyhow::{anyhow, Context};
use config::{Environment, File};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::net::SocketAddr;
use std::time::Duration;
use std::{env, fs};
use tracing::{info, warn};
use webpki::types::ServerName;

const CONFIG_BASE_PATH: &str = "config/";
const DEFAULT_ENV: &str = "development";

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct AppConfig {
    services: Vec<ServiceConfig>,
    worker_threads: usize,
    close_timeout: u64,
    exit_timeout: u64,
    shutdown_grace_period: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ServiceConfig {
    name: String,
    port: u16,
    cert_path: String,
    key_path: String,
    ca_path: String,
    upstream_host: String,
    upstream_port: u16,
    discovery_type: String,
    discovery_refresh_interval: u64,
    load_balancer_selection: String,
}

impl ServiceConfig {
    pub fn server_name(&self) -> anyhow::Result<ServerName> {
        ServerName::try_from(self.name.as_str()).context("Invalid server name")
    }

    pub fn upstream_addr(&self) -> anyhow::Result<SocketAddr> {
        format!("{}:{}", self.upstream_host, self.upstream_port)
            .parse()
            .context("Invalid upstream address")
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn cert(&self) -> anyhow::Result<Vec<u8>> {
        let mut file = fs::File::open(&self.cert_path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        Ok(buffer)
    }

    pub fn key(&self) -> anyhow::Result<Vec<u8>> {
        let mut file = fs::File::open(&self.key_path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        Ok(buffer)
    }

    pub fn roots_ca(&self) -> anyhow::Result<Vec<u8>> {
        let mut file = fs::File::open(&self.ca_path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        Ok(buffer)
    }

    pub fn upstream_host(&self) -> &str {
        &self.upstream_host
    }

    pub fn upstream_port(&self) -> u16 {
        self.upstream_port
    }

    pub fn discovery_type(&self) -> &str {
        &self.discovery_type
    }

    pub fn discovery_refresh_interval(&self) -> u64 {
        self.discovery_refresh_interval
    }

    pub fn load_balancer_selection(&self) -> &str {
        &self.load_balancer_selection
    }

    pub fn ca_path(&self) -> &str {
        &self.ca_path
    }

    pub fn new(
        name: String,
        port: u16,
        cert_path: String,
        key_path: String,
        ca_path: String,
        upstream_host: String,
        upstream_port: u16,
        discovery_type: String,
        discovery_refresh_interval: u64,
        load_balancer_selection: String,
    ) -> Self {
        Self {
            name,
            port,
            cert_path,
            key_path,
            ca_path,
            upstream_host,
            upstream_port,
            discovery_type,
            discovery_refresh_interval,
            load_balancer_selection,
        }
    }
}

impl AppConfig {
    pub fn try_default() -> anyhow::Result<Self> {
        let run_env = AppConfig::get_env_var(
            "RUN_ENV",
            DEFAULT_ENV,
            "RUN_ENV not set. Using the default environment: {}",
        );
        let config_path = AppConfig::get_env_var(
            "CONFIG_BASE_PATH",
            CONFIG_BASE_PATH,
            "CONFIG_BASE_PATH not set. Using the default environment: {}",
        );

        let config = AppConfig::get_config(&run_env, &config_path)?;

        let mut app_config = config
            .try_deserialize::<Self>()
            .context("Failed to load configuration")?;

        AppConfig::set_env_vars(&mut app_config)?;

        info!("Configuration loaded successfully {:?}", app_config);
        Ok(app_config)
    }

    pub fn get_first_service_config(&self) -> anyhow::Result<ServiceConfig> {
        self.services
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("No services configured"))
    }

    fn set_env_vars(app_config: &mut Self) -> anyhow::Result<()> {
        if let Ok(worker_threads) = env::var("UMAY_WORKER_THREADS") {
            app_config.worker_threads = worker_threads.parse()?;
        }
        if let Ok(close_timeout) = env::var("UMAY_CLOSE_TIMEOUT") {
            app_config.close_timeout = close_timeout.parse()?;
        }
        if let Ok(exit_timeout) = env::var("UMAY_EXIT_TIMEOUT") {
            app_config.exit_timeout = exit_timeout.parse()?;
        }
        if let Ok(shutdown_grace_period) = env::var("UMAY_SHUTDOWN_GRACE_PERIOD") {
            app_config.shutdown_grace_period = shutdown_grace_period.parse()?;
        }

        for (index, service) in app_config.services.iter_mut().enumerate() {
            let prefix = format!("UMAY_SERVICE_{}_", index);
            if let Ok(name) = env::var(format!("{}NAME", prefix)) {
                service.name = name;
            }
            if let Ok(port) = env::var(format!("{}PORT", prefix)) {
                service.port = port.parse()?;
            }
            if let Ok(cert_path) = env::var(format!("{}CERT_PATH", prefix)) {
                service.cert_path = cert_path;
            }
            if let Ok(key_path) = env::var(format!("{}KEY_PATH", prefix)) {
                service.key_path = key_path;
            }
            if let Ok(ca_path) = env::var(format!("{}CA_PATH", prefix)) {
                service.ca_path = ca_path;
            }
            if let Ok(upstream_host) = env::var(format!("{}UPSTREAM_HOST", prefix)) {
                service.upstream_host = upstream_host;
            }
            if let Ok(upstream_port) = env::var(format!("{}UPSTREAM_PORT", prefix)) {
                service.upstream_port = upstream_port.parse()?;
            }
            if let Ok(discovery_type) = env::var(format!("{}DISCOVERY_TYPE", prefix)) {
                service.discovery_type = discovery_type;
            }
            if let Ok(discovery_refresh_interval) =
                env::var(format!("{}DISCOVERY_REFRESH_INTERVAL", prefix))
            {
                service.discovery_refresh_interval = discovery_refresh_interval.parse()?;
            }
            if let Ok(load_balancer_selection) =
                env::var(format!("{}LOAD_BALANCER_SELECTION", prefix))
            {
                service.load_balancer_selection = load_balancer_selection;
            }
        }
        Ok(())
    }

    fn get_env_var(var: &str, default: &str, warning: &str) -> String {
        env::var(var).unwrap_or_else(|_| {
            warn!("{}", warning);
            default.to_string()
        })
    }

    fn get_config(run_env: &str, config_path: &str) -> anyhow::Result<config::Config> {
        config::Config::builder()
            .add_source(File::with_name(&format!("{}default.toml", config_path)))
            .add_source(
                File::with_name(&format!("{}{}.toml", config_path, run_env)).required(false),
            )
            .add_source(Environment::with_prefix("UMAY").separator("_"))
            .build()
            .context("Failed to build configuration")
    }

    pub fn services(&self) -> &Vec<ServiceConfig> {
        &self.services
    }

    pub fn worker_threads(&self) -> usize {
        self.worker_threads
    }

    pub fn shutdown_grace_period(&self) -> Duration {
        Duration::from_secs(self.shutdown_grace_period)
    }

    pub fn close_timeout(&self) -> Duration {
        Duration::from_secs(self.close_timeout)
    }

    pub fn exit_timeout(&self) -> Duration {
        Duration::from_secs(self.exit_timeout)
    }

    pub fn new(
        services: Vec<ServiceConfig>,
        worker_threads: usize,
        close_timeout: u64,
        exit_timeout: u64,
        shutdown_grace_period: u64,
    ) -> Self {
        Self {
            services,
            worker_threads,
            close_timeout,
            exit_timeout,
            shutdown_grace_period,
        }
    }
}
