use crate::balance::discovery::ServiceDiscovery;
use crate::balance::selection::SelectionAlgorithm;
use anyhow::Result;
use arc_swap::ArcSwap;
use std::collections::BTreeSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::error;

pub mod discovery;
pub mod selection;

#[derive(Clone, Hash, PartialEq, PartialOrd, Eq, Ord, Debug)]
pub struct Backend {
    pub addr: SocketAddr,
    pub weight: usize,
}

impl Backend {
    pub fn new(addr: SocketAddr, weight: usize) -> Self {
        Backend { addr, weight }
    }

    pub fn hash_key(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

pub struct Backends {
    discovery: Box<dyn ServiceDiscovery + Send + Sync + 'static>,
    backends: ArcSwap<BTreeSet<Backend>>,
}

impl Backends {
    pub fn new(discovery: Box<dyn ServiceDiscovery + Send + Sync + 'static>) -> Self {
        Self {
            discovery,
            backends: ArcSwap::from_pointee(BTreeSet::new()),
        }
    }

    pub async fn refresh(&self) -> Result<()> {
        let new_backends = self.discovery.discover().await?;
        self.backends.store(new_backends);
        Ok(())
    }

    pub fn get_backends(&self) -> Arc<BTreeSet<Backend>> {
        self.backends.load_full()
    }
}

pub struct LoadBalancer {
    selection: Arc<dyn SelectionAlgorithm + Send + Sync>,
    backends: Arc<Backends>,
}

impl LoadBalancer {
    pub fn new(backends: Backends, selection: Arc<dyn SelectionAlgorithm>) -> Self {
        Self {
            selection,
            backends: Arc::new(backends),
        }
    }

    pub async fn select(&self, key: Option<&str>) -> Option<Backend> {
        let backends = self.backends.get_backends();
        if backends.is_empty() {
            return None;
        }

        self.selection.select(&backends).await
    }

    pub async fn start_refresh_task(self: Arc<Self>, duration: Duration) {
        let mut ticker = tokio::time::interval(duration);
        loop {
            ticker.tick().await;
            if let Err(e) = self.backends.refresh().await {
                error!("Failed to refresh backends: {:?}", e);
            }
        }
    }
}
