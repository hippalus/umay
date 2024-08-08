use crate::balance::discovery::ServiceDiscovery;
use anyhow::Result;
use arc_swap::ArcSwap;
use rand::prelude::IteratorRandom;
use std::collections::BTreeSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::error;

pub mod discovery;

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

#[derive(Clone)]
pub enum Selector {
    Random,
    RoundRobin(Arc<Mutex<usize>>),
    LeastConnection(Arc<Mutex<Vec<(Backend, usize)>>>),
    ConsistentHashing,
}

pub struct LoadBalancer {
    selector: Selector,
    backends: Arc<Backends>,
}

impl LoadBalancer {
    pub fn new(backends: Backends, selector: Selector) -> Self {
        Self {
            selector,
            backends: Arc::new(backends),
        }
    }

    pub async fn select(&self, key: Option<&str>) -> Option<Backend> {
        let backends = self.backends.get_backends();
        if backends.is_empty() {
            return None;
        }

        match &self.selector {
            Selector::Random => backends.iter().choose(&mut rand::thread_rng()).cloned(),
            Selector::RoundRobin(counter) => {
                let mut index = counter.lock().await;
                *index = (*index + 1) % backends.len();
                backends.iter().nth(*index).cloned()
            }
            Selector::LeastConnection(connections) => {
                let mut conns = connections.lock().await;
                conns.sort_by_key(|(_, count)| *count);
                conns.first().map(|(backend, _)| backend.clone())
            }
            Selector::ConsistentHashing => {
                if let Some(key) = key {
                    let hash = {
                        let mut hasher = DefaultHasher::new();
                        key.hash(&mut hasher);
                        hasher.finish()
                    };
                    backends
                        .iter()
                        .min_by_key(|backend| backend.hash_key() ^ hash)
                        .cloned()
                } else {
                    None
                }
            }
        }
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
