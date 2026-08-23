//! Schema versioning — **A-12**.
//!
//! `Record`, `panels.json` and `decks.json` each carry a `schema_version`. Unknown fields are
//! preserved on round-trip; a **major** version mismatch fails loudly with a migration message.
//! Without this, §6's "every Tier 2–5 connector is purely additive" claim is unenforceable.

/// The record envelope schema version this build reads and writes.
pub const RECORD_SCHEMA_VERSION: u16 = 1;

/// The `panels.json` schema version this build reads (§7b).
pub const PANELS_SCHEMA_VERSION: u16 = 1;

/// The `decks.json` schema version this build reads (§7c).
pub const DECKS_SCHEMA_VERSION: u16 = 1;

/// Check a version found on disk against what this build supports.
///
/// # Errors
/// [`crate::Error::SchemaVersion`] when the major versions differ (A-12: fail loudly).
pub fn check(found: u16, supported: u16, what: &str) -> crate::Result<()> {
    if found == supported {
        return Ok(());
    }
    Err(crate::Error::SchemaVersion {
        found,
        supported,
        hint: format!("{what}: run the migration for v{found} → v{supported} before loading"),
    })
}
