//! `ConnectorRegistry` — **A-3**.
//!
//! Discovers connectors at startup, exposes their declared capabilities, and reports per-connector
//! health.
//!
//! The core is **empty by design**: the vault is connector #1, not a special case. Nothing in this
//! module knows what a vault, a calendar or an inbox is, and if it ever needs to, the abstraction
//! has failed.

use crate::connector::{Capability, Connector, Health};
use crate::model::Kind;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Holds every discovered connector and answers "who can serve this kind?".
#[derive(Default, Clone)]
pub struct ConnectorRegistry {
    connectors: Vec<Arc<dyn Connector>>,
}

impl ConnectorRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a connector. Later registration order never affects query results — the router
    /// sorts deterministically (A-5) and breaks ties by configured precedence (A-4).
    pub fn register(&mut self, c: Arc<dyn Connector>) -> &mut Self {
        self.connectors.push(c);
        self
    }

    /// Every registered connector.
    #[must_use]
    pub fn all(&self) -> &[Arc<dyn Connector>] {
        &self.connectors
    }

    /// Connectors declaring `Read(kind)`.
    #[must_use]
    pub fn readers_of(&self, kind: Kind) -> Vec<Arc<dyn Connector>> {
        self.with_capability(Capability::Read(kind))
    }

    /// Connectors declaring `Write(kind)` (A-8).
    #[must_use]
    pub fn writers_of(&self, kind: Kind) -> Vec<Arc<dyn Connector>> {
        self.with_capability(Capability::Write(kind))
    }

    /// Connectors declaring a specific capability.
    #[must_use]
    pub fn with_capability(&self, cap: Capability) -> Vec<Arc<dyn Connector>> {
        self.connectors
            .iter()
            .filter(|c| c.capabilities().contains(&cap))
            .cloned()
            .collect()
    }

    /// Health of every connector, keyed by id and ordered so the badge row never reshuffles.
    ///
    /// **A-6:** an unhealthy connector is reported, not removed. Its panels degrade; the Deck does
    /// not blank.
    pub async fn health(&self) -> BTreeMap<String, Health> {
        let mut out = BTreeMap::new();
        for c in &self.connectors {
            out.insert(c.id(), c.health().await);
        }
        out
    }

    /// Every capability declared by anything registered, deduplicated and ordered.
    ///
    /// This is what the panel layer consults to decide whether a manifest entry can be satisfied
    /// at all, rather than rendering an empty panel and calling it data.
    #[must_use]
    pub fn capabilities(&self) -> Vec<Capability> {
        let mut caps: Vec<Capability> = self
            .connectors
            .iter()
            .flat_map(|c| c.capabilities().iter().copied())
            .collect();
        caps.sort_by_key(|c| format!("{c:?}"));
        caps.dedup();
        caps
    }
}
