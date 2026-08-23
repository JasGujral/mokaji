//! `ModelProvider` and the routing policy — **DEC-3, FR-E, §7d**.
//!
//! ```text
//! policy[TaskClass] -> Tier ; Cloud requires Consent(TaskClass) ; Cloud failure -> Local
//! ```
//!
//! **X-1 / PRIV-1 / PRIV-2.** Local data, local senses, opt-in cloud cognition. Audio never
//! reaches a provider — the audio crate has no network dependency at all, and that is enforced
//! structurally, not by convention. Vault contents, the index and the audit log leave only as
//! explicit, consented, audit-logged model context.
//!
//! **E-2.** Briefing assembly is *pinned local*, so the core daily loop has no cloud dependency.
//!
//! # M-0 work remaining
//! - [ ] the policy table as data, with briefing pinned to [`Tier::Local`]
//! - [ ] consent gate + append-only audit log of exactly what bytes left the machine
//! - [ ] cloud-failure → local degradation with a spoken note
//! - [ ] OQ-1 stays open: Ollama-as-a-service vs llama.cpp sidecar — decided at M-4 start,
//!   reversible behind this trait

use async_trait::async_trait;

/// Where a completion runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// On this machine.
    Local,
    /// Off this machine, with consent and an audit entry.
    Cloud,
}

/// What kind of thinking is being asked for. The policy table is keyed by this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskClass {
    /// Parsing an utterance into an intent. Small and constrained — pinned local (RISK-4).
    IntentParse,
    /// Assembling the morning briefing. **Pinned local by E-2.**
    Briefing,
    /// Open-ended reasoning, planning, drafting. May escalate to cloud with consent.
    Reasoning,
}

/// A completion request.
#[derive(Debug, Clone)]
pub struct Completion {
    /// What is being asked.
    pub class: TaskClass,
    /// The prompt.
    pub prompt: String,
}

/// A streamed completion.
pub struct CompletionStream(pub tokio::sync::mpsc::Receiver<String>);

/// A source of completions (§7d).
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Stable id, e.g. `"anthropic"`, `"local"`.
    fn id(&self) -> &str;

    /// Local or cloud.
    fn tier(&self) -> Tier;

    /// Run a completion.
    async fn complete(&self, req: Completion) -> crate::Result<CompletionStream>;
}
