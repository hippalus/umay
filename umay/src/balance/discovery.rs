use crate::balance::Backend;
use arc_swap::ArcSwap;
use async_trait::async_trait;
use hickory_resolver::config::{ResolverConfig, ResolverOpts};
use hickory_resolver::TokioAsyncResolver;
use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::Arc;

#[async_trait]
pub trait ServiceDiscovery {
    async fn discover(&self) -> anyhow::Result<Arc<BTreeSet<Backend>>>;
}

pub struct DnsDiscovery {
    resolver: TokioAsyncResolver,
    hostname: String,
    port: u16,
}

impl DnsDiscovery {
    pub fn new(hostname: String, port: u16) -> anyhow::Result<Self> {
        let resolver =
            TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());
        Ok(Self {
            resolver,
            hostname,
            port,
        })
    }
}

#[async_trait]
impl ServiceDiscovery for DnsDiscovery {
    async fn discover(&self) -> anyhow::Result<Arc<BTreeSet<Backend>>> {
        let ips = self.resolver.lookup_ip(&self.hostname).await?;
        let backends = ips
            .iter()
            .map(|ip| Backend::new(SocketAddr::new(ip, self.port), 1))
            .collect();
        Ok(Arc::new(backends))
    }
}

pub struct LocalDiscovery {
    backends: ArcSwap<BTreeSet<Backend>>,
}

impl Default for LocalDiscovery {
    fn default() -> Self {
        Self {
            backends: ArcSwap::from_pointee(BTreeSet::new()),
        }
    }
}

impl LocalDiscovery {
    pub fn with_backends(backends: Vec<SocketAddr>) -> Self {
        let backends = backends
            .into_iter()
            .map(|addr| Backend::new(addr, 1))
            .collect();
        Self {
            backends: ArcSwap::from_pointee(backends),
        }
    }

    pub fn add_backend(&self, addr: SocketAddr) {
        self.backends.rcu(|backends| {
            let mut new_backends = (**backends).clone();
            new_backends.insert(Backend::new(addr, 1));
            new_backends
        });
    }

    pub fn remove_backend(&self, addr: &SocketAddr) {
        self.backends.rcu(|backends| {
            let mut new_backends = (**backends).clone();
            new_backends.retain(|backend| &backend.addr != addr);
            new_backends
        });
    }

    pub fn set_backends(&self, backends: Vec<SocketAddr>) {
        let new_backends = backends
            .into_iter()
            .map(|addr| Backend::new(addr, 1))
            .collect();
        self.backends.store(Arc::new(new_backends));
    }

    pub fn clear_backends(&self) {
        self.backends.store(Arc::new(BTreeSet::new()));
    }
}

#[async_trait]
impl ServiceDiscovery for LocalDiscovery {
    async fn discover(&self) -> anyhow::Result<Arc<BTreeSet<Backend>>> {
        Ok(self.backends.load_full())
    }
}
