//! The `Connector` trait — **A-2, §7a**. *TET is the whole contract.*
//!
//! `transform_query` → `extract` → `transform_data`. The three stages are **separately testable**
//! and every error names its stage ([`crate::error::Stage`]).
//!
//! Connectors are Rust-native (DEC-5) so credentials never leave the native side (PRIV-4). The
//! cost of that is mitigated by the process/HTTP shim (A-7, DEC-6), which lets any local
//! executable or `localhost` endpoint speak the contract in JSON — including a wrapped MCP server
//! (X-3).

use crate::model::{AnyRecord, ConnectorId, Kind};
use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// What a connector declares it can do (A-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Can read this kind.
    Read(Kind),
    /// Can write this kind (A-8).
    Write(Kind),
    /// Can push changes for this kind (A-11).
    Push(Kind),
}

/// Per-connector health, surfaced as a badge. A-6: unhealthy degrades its panels only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    /// Working.
    Ok,
    /// Working, but something is off (stale cache, near a rate limit).
    Degraded(String),
    /// Not working.
    Down(String),
}

/// A query in standard terms, before any connector sees it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardQuery {
    /// Which model kind is being asked for.
    pub kind: Kind,
    /// Optional time window, in local-calendar-day terms (§5).
    pub window: Option<String>,
    /// Free-form parameters declared by the panel manifest (§7b `params`).
    #[serde(default)]
    pub params: serde_json::Map<String, serde_json::Value>,
}

/// A query shaped for one specific provider. Opaque to the router.
#[derive(Debug, Clone)]
pub struct ProviderQuery(pub serde_json::Value);

/// Whatever the source returned, untouched.
#[derive(Debug, Clone)]
pub struct RawPayload(pub serde_json::Value);

/// A write-back request. Idempotent by [`Mutation::id`] (A-8).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mutation {
    /// Idempotency key.
    pub id: String,
    /// What is being mutated.
    pub kind: Kind,
    /// The operation, connector-interpreted.
    pub op: serde_json::Value,
}

/// Proof a mutation was applied — or, in dry-run (B-4), what *would* have been applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    /// The mutation this answers.
    pub mutation_id: String,
    /// Whether it was actually applied, or only previewed (B-4 dry-run is the default).
    pub applied: bool,
    /// Human-readable diff of the change.
    pub diff: String,
}

/// A pushed change (A-11).
#[derive(Debug, Clone)]
pub struct ChangeEvent {
    /// Which connector.
    pub source: ConnectorId,
    /// Which kind changed.
    pub kind: Kind,
}

/// The connector contract (§7a).
#[async_trait]
pub trait Connector: Send + Sync {
    /// Stable id, e.g. `"vault"`.
    fn id(&self) -> ConnectorId;

    /// Which record schema version this connector emits (A-12).
    fn schema_version(&self) -> u16;

    /// What it declares it can do (A-3).
    fn capabilities(&self) -> &[Capability];

    /// Current health (A-3, A-6).
    async fn health(&self) -> Health;

    /// **T** — standard query in, provider query out.
    fn transform_query(&self, q: &StandardQuery) -> Result<ProviderQuery>;

    /// **E** — talk to the source.
    async fn extract(&self, pq: ProviderQuery) -> Result<RawPayload>;

    /// **T** — raw payload in, standard records out.
    fn transform_data(&self, raw: RawPayload) -> Result<Vec<AnyRecord>>;

    /// Write-back (A-8). Defaults to unsupported.
    ///
    /// # Errors
    /// [`crate::Error::Unsupported`] unless the connector overrides this.
    async fn apply(&self, m: Mutation) -> Result<Receipt> {
        Err(crate::Error::Unsupported {
            connector: self.id(),
            what: format!("apply({})", m.id),
        })
    }

    /// Push subscription (A-11). `None` means poll-only.
    fn subscribe(&self) -> Option<tokio::sync::broadcast::Receiver<ChangeEvent>> {
        None
    }
}
