//! Errors. **A-2: every error names the TET stage it came from.**

use thiserror::Error;

/// The TET stage an error originated in (A-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// `transform_query` — turning a `StandardQuery` into a provider-shaped query.
    TransformQuery,
    /// `extract` — talking to the source.
    Extract,
    /// `transform_data` — turning the raw payload into standard records.
    TransformData,
    /// `apply` — write-back (A-8).
    Apply,
}

/// The crate error type.
#[derive(Debug, Error)]
pub enum Error {
    /// A TET stage failed. The stage is always named (A-2).
    #[error("connector `{connector}` failed in {stage:?}: {message}")]
    Stage {
        /// Which connector.
        connector: String,
        /// Which stage.
        stage: Stage,
        /// What went wrong.
        message: String,
    },

    /// The connector does not declare this capability (A-3).
    #[error("connector `{connector}` does not support {what}")]
    Unsupported {
        /// Which connector.
        connector: String,
        /// What was asked of it.
        what: String,
    },

    /// A major `schema_version` mismatch. Fails loudly with a migration message (A-12).
    #[error("schema version mismatch: found {found}, this build supports {supported} — {hint}")]
    SchemaVersion {
        /// The version on the data.
        found: u16,
        /// The version this build understands.
        supported: u16,
        /// How to migrate.
        hint: String,
    },
}

/// Crate result alias.
pub type Result<T> = std::result::Result<T, Error>;
