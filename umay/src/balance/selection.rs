use crate::balance::Backend;
use arc_swap::ArcSwap;
use async_trait::async_trait;
use rand::Rng;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[async_trait]
pub trait SelectionAlgorithm: Send + Sync {
    async fn select(&self, backends: &Arc<BTreeSet<Backend>>) -> Option<Backend>;
}

pub struct RoundRobin {
    index: AtomicUsize,
}

impl Default for RoundRobin {
    fn default() -> Self {
        Self {
            index: AtomicUsize::new(0),
        }
    }
}
#[async_trait]
impl SelectionAlgorithm for RoundRobin {
    async fn select(&self, backends: &Arc<BTreeSet<Backend>>) -> Option<Backend> {
        let len = backends.len();
        if len == 0 {
            return None;
        }
        let index = self.index.fetch_add(1, Ordering::Relaxed) % len;
        backends.iter().nth(index).cloned()
    }
}
pub struct WeightedRoundRobin {
    index: AtomicUsize,
}

impl Default for WeightedRoundRobin {
    fn default() -> Self {
        Self {
            index: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl SelectionAlgorithm for WeightedRoundRobin {
    async fn select(&self, backends: &Arc<BTreeSet<Backend>>) -> Option<Backend> {
        let total_weight: usize = backends.iter().map(|b| b.weight).sum();
        if total_weight == 0 {
            return None;
        }
        let mut index = self.index.fetch_add(1, Ordering::Relaxed) % total_weight;
        for backend in backends.iter() {
            if index < backend.weight {
                return Some(backend.clone());
            }
            index -= backend.weight;
        }
        None
    }
}

pub struct LeastConnections {
    connections: ArcSwap<BTreeMap<SocketAddr, usize>>,
}

impl Default for LeastConnections {
    fn default() -> Self {
        Self {
            connections: ArcSwap::from_pointee(BTreeMap::new()),
        }
    }
}

impl LeastConnections {
    pub fn increment(&self, addr: &SocketAddr) {
        self.connections.rcu(|connections| {
            let mut new_connections = connections.as_ref().clone();
            *new_connections.entry(*addr).or_insert(0) += 1;
            new_connections
        });
    }

    pub fn decrement(&self, addr: &SocketAddr) {
        self.connections.rcu(|connections| {
            let mut new_connections = connections.as_ref().clone();
            if let Some(count) = new_connections.get_mut(addr) {
                if *count > 0 {
                    *count -= 1;
                }
            }
            new_connections
        });
    }
}

#[async_trait]
impl SelectionAlgorithm for LeastConnections {
    async fn select(&self, backends: &Arc<BTreeSet<Backend>>) -> Option<Backend> {
        let connections = self.connections.load();
        backends
            .iter()
            .min_by_key(|b| connections.get(&b.addr).unwrap_or(&0))
            .cloned()
    }
}

#[derive(Default)]
pub struct Random;

#[async_trait]
impl SelectionAlgorithm for Random {
    async fn select(&self, backends: &Arc<BTreeSet<Backend>>) -> Option<Backend> {
        if backends.is_empty() {
            return None;
        }
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..backends.len());
        backends.iter().nth(index).cloned()
    }
}

pub struct ConsistentHashing {
    virtual_nodes: usize,
}

impl ConsistentHashing {
    pub fn new(virtual_nodes: usize) -> Self {
        ConsistentHashing { virtual_nodes }
    }

    fn hash<T: Hash>(t: &T) -> u64 {
        let mut s = DefaultHasher::new();
        t.hash(&mut s);
        s.finish()
    }
}

#[async_trait]
impl SelectionAlgorithm for ConsistentHashing {
    async fn select(&self, backends: &Arc<BTreeSet<Backend>>) -> Option<Backend> {
        todo!()
    }
}
