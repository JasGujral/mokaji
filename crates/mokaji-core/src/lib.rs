//! # mokaji-core — the standardization layer
//!
//! This crate **is** the product's moat (see `Jarvis/research/Inspiration — Reference
//! Architectures.md`). Everything else — panels, voice, the brain — is a consumer of the
//! contracts defined here.
//!
//! Authoritative spec: `Jarvis/REQUIREMENTS — MOKaji v1.md`. Requirement IDs are cited in
//! module docs so the code and the spec stay traceable to each other.
//!
//! ## Milestone M-0 scope
//!
//! | Module | Requirement | What it owns |
//! |---|---|---|
//! | [`model`]    | A-1, §5  | Standard models as serde structs. No connector-specific fields. |
//! | [`connector`]| A-2, §7a | The `Connector` trait: `transform_query` → `extract` → `transform_data`. |
//! | [`registry`] | A-3      | Startup discovery, declared capabilities, per-connector health. |
//! | [`router`]   | A-4/5/6  | Fan-out, content-key dedupe, deterministic sort, partial failure. |
//! | [`version`]  | A-12     | `schema_version` on every record and manifest; unknown fields preserved. |
//! | [`provider`] | §7d      | `ModelProvider` + the policy table (local vs cloud, with consent). |
//!
//! **M-0 exit criterion:** two *fake* connectors round-trip `Task` and `Event` through TET, the
//! router merges and dedupes on the content identity key and sorts deterministically, and the
//! PRIV-5 "no other socket" test passes — all in `cargo test`, with nothing on screen.

#![forbid(unsafe_code)]

pub mod connector;
pub mod error;
pub mod metrics;
pub mod model;
pub mod provider;
pub mod registry;
pub mod router;
pub mod version;

pub use error::{Error, Result};
