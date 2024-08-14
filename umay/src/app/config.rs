use anyhow::{anyhow, Context};
use config::{Environment, File};
use serde::{Deserialize, Serialize};
use std::env;
use std::net::SocketAddr;
use std::time::Duration;
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
    chain_path: String,
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

    pub fn cert_path(&self) -> &str {
        &self.cert_path
    }

    pub fn key_path(&self) -> &str {
        &self.key_path
    }

    pub fn chain_path(&self) -> &str {
        &self.chain_path
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
}

impl AppConfig {
    pub fn new() -> anyhow::Result<Self> {
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
        //TODO:

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
}
