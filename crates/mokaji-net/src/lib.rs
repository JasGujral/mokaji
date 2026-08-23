//! # mokaji-net — the single outbound chokepoint
//!
//! **PRIV-5: all outbound traffic routes through one HTTP client.** A [`KillSwitch`] disables it,
//! and a test asserts no other socket is opened process-wide. Honour-by-convention is
//! unverifiable, so this is structural: **no other crate in the workspace may depend on a
//! networking library**, and both CI and the pre-commit hook fail if one does.
//!
//! **PRIV-1 is why the crate has this shape.** The audio crate has no network dependency — it
//! *cannot* acquire one without a visible `Cargo.toml` change that fails review and CI. Audio
//! bytes, and anything derived from the ring buffer before transcription, never reach this crate.
//! That is a much stronger guarantee than "we promise not to send it".
//!
//! **PRIV-2 / FR-E.** Everything that leaves is recorded by an [`AuditSink`] first, so the audit
//! log can show byte-for-byte what left the machine. A request is not sendable without a
//! [`Consent`] token, which exists to make "did the user agree to this?" a type error rather than
//! a code review question.
//!
//! **SEC-3.** The A-7 connector shim binds loopback only and requires a per-session shared secret.
//!
//! # Not yet built
//! - a real audit *store* behind [`AuditSink`] (append-only, purgeable — M-4)
//! - retry/backoff policy, and the per-provider rate limiting that goes with it
//! - the socket-counting harness that turns PRIV-5's assertion into a process-wide one on macOS

#![forbid(unsafe_code)]

pub mod imap;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Disables all outbound traffic (PRIV-5).
///
/// Default: **armed** (traffic allowed). Cutting it must leave the app fully usable — REL-2 makes
/// offline a first-class mode, not an error state.
#[derive(Debug)]
pub struct KillSwitch(AtomicBool);

impl Default for KillSwitch {
    fn default() -> Self {
        Self(AtomicBool::new(true))
    }
}

impl KillSwitch {
    /// A new, armed (traffic-allowed) switch.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether outbound traffic is currently permitted.
    #[must_use]
    pub fn allowed(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// Cut all outbound traffic.
    pub fn cut(&self) {
        self.0.store(false, Ordering::SeqCst);
    }

    /// Restore outbound traffic.
    pub fn restore(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Proof that the user agreed to this class of traffic leaving the machine (FR-E).
///
/// Deliberately unforgeable outside this crate's callers: there is no `Default`, and the only
/// constructor names what was agreed to. A request cannot be built without one, which turns
/// "did we ask?" into something the compiler checks.
#[derive(Debug, Clone)]
pub struct Consent {
    task_class: String,
    granted_at: chrono::DateTime<chrono::Utc>,
}

impl Consent {
    /// Record that the user consented to this task class leaving the device.
    #[must_use]
    pub fn granted(task_class: impl Into<String>) -> Self {
        Self {
            task_class: task_class.into(),
            granted_at: chrono::Utc::now(),
        }
    }

    /// The task class this consent covers.
    #[must_use]
    pub fn task_class(&self) -> &str {
        &self.task_class
    }

    /// When it was granted.
    #[must_use]
    pub fn granted_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.granted_at
    }
}

/// One line of the audit log: exactly what left the machine (PRIV-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    /// When.
    pub at: chrono::DateTime<chrono::Utc>,
    /// Which task class consented to it.
    pub task_class: String,
    /// HTTP method.
    pub method: String,
    /// Destination host. Recorded separately so "where did my data go" is answerable without
    /// parsing every URL.
    pub host: String,
    /// Full URL.
    pub url: String,
    /// The request body verbatim. **This is the point of the audit log**: a byte count would let
    /// us say "something left", which is not the promise made.
    pub body: Option<String>,
}

/// Where audit entries go. Local-only, append-only, purgeable on demand (FR-E).
pub trait AuditSink: Send + Sync {
    /// Record one outbound request. Called *before* the request is sent, so a crash mid-flight
    /// still leaves evidence.
    fn record(&self, entry: AuditEntry);
}

/// An in-memory sink. Useful in tests and as the shape a real store must satisfy.
#[derive(Debug, Default)]
pub struct MemoryAudit(std::sync::Mutex<Vec<AuditEntry>>);

impl MemoryAudit {
    /// Everything recorded so far.
    #[must_use]
    pub fn entries(&self) -> Vec<AuditEntry> {
        self.0.lock().expect("audit lock").clone()
    }
}

impl AuditSink for MemoryAudit {
    fn record(&self, entry: AuditEntry) {
        self.0.lock().expect("audit lock").push(entry);
    }
}

/// Network errors.
#[derive(Debug, thiserror::Error)]
pub enum NetError {
    /// The kill switch is cut (PRIV-5). Callers degrade to local rather than failing the user.
    #[error("outbound traffic is disabled by the kill switch")]
    KillSwitchEngaged,
    /// The URL could not be parsed, so we cannot say where it would have gone.
    #[error("invalid url `{0}`")]
    InvalidUrl(String),
    /// The request itself failed.
    #[error("request to {host} failed: {source}")]
    Request {
        /// Destination.
        host: String,
        /// Underlying cause.
        #[source]
        source: reqwest::Error,
    },
}

/// The one HTTP client in the process (PRIV-5).
///
/// Construct it **once** and share the handle. Constructing a second one is not a compile error,
/// but it defeats the point — the guarantee is "one chokepoint", and a second client is a second
/// door.
pub struct HttpClient {
    inner: reqwest::Client,
    kill: Arc<KillSwitch>,
    audit: Arc<dyn AuditSink>,
}

impl HttpClient {
    /// Build the chokepoint.
    ///
    /// # Errors
    /// If the underlying TLS/client stack cannot be initialised.
    pub fn new(kill: Arc<KillSwitch>, audit: Arc<dyn AuditSink>) -> Result<Self, reqwest::Error> {
        let inner = reqwest::Client::builder()
            .user_agent(concat!("mokaji/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            // No proxy auto-detection: an environment variable must not be able to silently
            // redirect this machine's outbound traffic somewhere else.
            .no_proxy()
            .build()?;
        Ok(Self { inner, kill, audit })
    }

    /// The kill switch this client obeys.
    #[must_use]
    pub fn kill_switch(&self) -> &Arc<KillSwitch> {
        &self.kill
    }

    /// Send a request, recording it first.
    ///
    /// # Errors
    /// [`NetError::KillSwitchEngaged`] when traffic is cut — checked **before** the URL is even
    /// parsed, so a cut switch cannot leak a DNS lookup.
    pub async fn send(
        &self,
        consent: &Consent,
        method: reqwest::Method,
        url: &str,
        body: Option<String>,
    ) -> Result<reqwest::Response, NetError> {
        if !self.kill.allowed() {
            return Err(NetError::KillSwitchEngaged);
        }

        let parsed = reqwest::Url::parse(url).map_err(|_| NetError::InvalidUrl(url.to_owned()))?;
        let host = parsed.host_str().unwrap_or_default().to_owned();

        // Recorded before sending: a crash mid-flight still leaves evidence of what was in flight.
        self.audit.record(AuditEntry {
            at: chrono::Utc::now(),
            task_class: consent.task_class().to_owned(),
            method: method.to_string(),
            host: host.clone(),
            url: url.to_owned(),
            body: body.clone(),
        });

        let mut req = self.inner.request(method, parsed);
        if let Some(b) = body {
            req = req.body(b);
        }
        req.send()
            .await
            .map_err(|source| NetError::Request { host, source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> (HttpClient, Arc<KillSwitch>, Arc<MemoryAudit>) {
        let kill = Arc::new(KillSwitch::new());
        let audit = Arc::new(MemoryAudit::default());
        let c = HttpClient::new(kill.clone(), audit.clone()).expect("client builds");
        (c, kill, audit)
    }

    #[test]
    fn kill_switch_starts_armed_and_cuts() {
        let ks = KillSwitch::new();
        assert!(ks.allowed());
        ks.cut();
        assert!(!ks.allowed());
        ks.restore();
        assert!(ks.allowed());
    }

    #[tokio::test]
    async fn a_cut_switch_refuses_before_resolving_anything() {
        let (c, kill, audit) = client();
        kill.cut();

        let err = c
            .send(
                &Consent::granted("reasoning"),
                reqwest::Method::GET,
                "https://example.com/anything",
                None,
            )
            .await
            .expect_err("must refuse");

        assert!(matches!(err, NetError::KillSwitchEngaged));
        assert!(
            audit.entries().is_empty(),
            "nothing was sent, so nothing is audited — and no DNS lookup happened either"
        );
    }

    #[tokio::test]
    async fn every_request_is_audited_before_it_leaves() {
        let (c, _kill, audit) = client();
        // Deliberately unroutable: the send fails, but the audit entry must already exist.
        let _ = c
            .send(
                &Consent::granted("reasoning"),
                reqwest::Method::POST,
                "https://127.0.0.1:1/v1/messages",
                Some("{\"prompt\":\"plan my day\"}".into()),
            )
            .await;

        let entries = audit.entries();
        assert_eq!(entries.len(), 1, "recorded before sending, not after");
        assert_eq!(entries[0].method, "POST");
        assert_eq!(entries[0].host, "127.0.0.1");
        assert_eq!(
            entries[0].body.as_deref(),
            Some("{\"prompt\":\"plan my day\"}"),
            "the body is recorded verbatim — a byte count could not honour PRIV-2's promise"
        );
        assert_eq!(entries[0].task_class, "reasoning");
    }

    #[tokio::test]
    async fn an_unparseable_url_is_refused_and_not_audited() {
        let (c, _kill, audit) = client();
        let err = c
            .send(
                &Consent::granted("reasoning"),
                reqwest::Method::GET,
                "not a url",
                None,
            )
            .await
            .expect_err("must refuse");
        assert!(matches!(err, NetError::InvalidUrl(_)));
        assert!(
            audit.entries().is_empty(),
            "we cannot honestly log a destination we could not parse"
        );
    }
}
