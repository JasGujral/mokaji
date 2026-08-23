//! `Router` — **A-4, A-5, A-6**.
//!
//! Resolves a [`StandardQuery`] across **all** connectors declaring the capability, then merges,
//! dedupes and sorts. Three rules, each of which exists because the obvious alternative is wrong.
//!
//! ## A-4 — dedupe on a content identity key
//!
//! | Model | Content identity key |
//! |---|---|
//! | `Event` | `(normalized_title, start, duration)` |
//! | `Task`  | `(normalized_text, due)` |
//!
//! Deduping by `(source, source_ref)` **cannot** work: sources differ by construction, and the
//! same meeting arriving via `gcal` *and* a local `.ics` is a v1 reality. Ties are broken by a
//! configured [`SourcePrecedence`].
//!
//! ## A-5 — deterministic sort
//!
//! | Model | Sort key |
//! |---|---|
//! | `Event` | `start` asc, then `title` |
//! | `Task`  | `due` asc **nulls last**, then `text` |
//!
//! Identical queries return identical order. `id` is appended as a final tiebreaker so the order
//! is a *total* one — without it, two records agreeing on the declared key could still swap places
//! between runs, which is exactly the flakiness A-5 exists to prevent.
//!
//! ## A-6 — partial, not fatal
//!
//! One dead connector degrades only its panels and raises a health badge. It never blanks the
//! Deck, so [`RoutedResult`] carries failures *alongside* records rather than replacing them.

use crate::connector::{Capability, Connector, StandardQuery};
use crate::model::{AnyRecord, Kind};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;

/// Source precedence, most-trusted first. Breaks content-key ties (A-4).
///
/// A source not named here ranks below every source that is, so adding a connector can never
/// silently outrank a configured one.
#[derive(Debug, Clone, Default)]
pub struct SourcePrecedence(pub Vec<String>);

impl SourcePrecedence {
    /// Build from an iterator of connector ids, most-trusted first.
    pub fn new<I, S>(order: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self(order.into_iter().map(Into::into).collect())
    }

    /// Lower is more trusted. Unknown sources rank last.
    #[must_use]
    pub fn rank(&self, source: &str) -> usize {
        self.0
            .iter()
            .position(|s| s == source)
            .unwrap_or(usize::MAX)
    }
}

/// Normalize free text for content-identity comparison.
///
/// Case-folded, with every non-alphanumeric character removed outright — spaces and punctuation
/// alike. `"Watch handover"`, `"stand-up"` and `"  STAND UP! "` all become `"watchhandover"`.
///
/// Deliberately lossy, and the aggression is the point. The same meeting reaches us from Google
/// Calendar and a local `.ics` spelled differently by two systems that never agreed on anything;
/// treating those as two meetings is the bug this function exists to prevent. Keeping separators
/// would leave `"Watch-Handover!"` and `"Watch handover"` distinct, which is exactly the failure mode.
///
/// The obvious objection — that this could merge two genuinely different titles — is handled by
/// the rest of the key: [`ContentKey::Event`] also carries `start` and duration, and
/// [`ContentKey::Task`] carries `due`. Two different events would have to share a normalized
/// title *and* a start instant *and* a duration to collide.
#[must_use]
pub fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The identity of a record's *content*, independent of where it came from (A-4).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ContentKey {
    /// `(normalized_text, due)`
    Task(String, Option<DateTime<Utc>>),
    /// `(normalized_title, start, duration_seconds)`
    Event(String, DateTime<Utc>, i64),
    /// Kinds without a defined identity key fall back to the record id, which dedupes nothing
    /// across sources — correct, because inventing a key would merge unrelated records.
    ById(Kind, String),
}

impl ContentKey {
    /// Derive the content identity key for a record.
    #[must_use]
    pub fn of(record: &AnyRecord) -> Self {
        match record {
            AnyRecord::Task(r) => Self::Task(normalize(&r.data.text), r.data.due),
            AnyRecord::Event(r) => Self::Event(
                normalize(&r.data.title),
                r.data.start,
                (r.data.end - r.data.start).num_seconds(),
            ),
            other => Self::ById(other.kind(), record_id(other).to_owned()),
        }
    }
}

fn record_id(r: &AnyRecord) -> &str {
    match r {
        AnyRecord::Task(x) => &x.id,
        AnyRecord::Event(x) => &x.id,
        AnyRecord::Chaser(x) => &x.id,
        AnyRecord::Note(x) => &x.id,
        AnyRecord::Person(x) => &x.id,
        AnyRecord::Metric(x) => &x.id,
        AnyRecord::Message(x) => &x.id,
        AnyRecord::Goal(x) => &x.id,
    }
}

/// The declared sort key for a record (A-5).
///
/// Encoded so `Ord` does the right thing: the leading `u8` puts records with no `due` after those
/// that have one (**nulls last**), which `Option<T>`'s own ordering would get backwards.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SortKey(u8, Option<DateTime<Utc>>, String, String);

fn sort_key(r: &AnyRecord) -> SortKey {
    match r {
        AnyRecord::Task(x) => SortKey(
            u8::from(x.data.due.is_none()), // 0 = has a due date and sorts first
            x.data.due,
            x.data.text.clone(),
            x.id.clone(),
        ),
        AnyRecord::Event(x) => SortKey(0, Some(x.data.start), x.data.title.clone(), x.id.clone()),
        other => SortKey(0, None, String::new(), record_id(other).to_owned()),
    }
}

/// Why a connector did not contribute to a result (A-6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// Which connector.
    pub connector: String,
    /// What went wrong, already stage-named by [`crate::Error`].
    pub reason: String,
}

/// The result of a routed query. Carries failures alongside records so the Deck degrades
/// per-panel rather than blanking (A-6).
#[derive(Debug, Default, Clone)]
pub struct RoutedResult {
    /// Merged, deduped, deterministically sorted records.
    pub records: Vec<AnyRecord>,
    /// Connectors that failed, and why. Non-fatal by construction.
    pub failures: Vec<Failure>,
}

impl RoutedResult {
    /// Whether every connector that was asked actually answered.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Fans a [`StandardQuery`] out across connectors and merges what comes back.
#[derive(Default, Clone)]
pub struct Router {
    precedence: SourcePrecedence,
}

impl Router {
    /// A router with no configured precedence — ties fall back to connector id order.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the source precedence used to break content-key ties (A-4).
    #[must_use]
    pub fn with_precedence(mut self, precedence: SourcePrecedence) -> Self {
        self.precedence = precedence;
        self
    }

    /// Run `q` against every connector declaring `Read(q.kind)`, then merge, dedupe and sort.
    ///
    /// Never returns `Err`: a connector failure is data (A-6), not a control-flow event. A caller
    /// that wants strictness checks [`RoutedResult::is_complete`].
    pub async fn resolve(
        &self,
        connectors: &[Arc<dyn Connector>],
        q: &StandardQuery,
    ) -> RoutedResult {
        let capable: Vec<_> = connectors
            .iter()
            .filter(|c| c.capabilities().contains(&Capability::Read(q.kind)))
            .cloned()
            .collect();

        // Concurrent fan-out: the slowest connector sets the latency, not the sum of all of them.
        let outcomes = futures::future::join_all(capable.iter().map(|c| {
            let c = c.clone();
            async move { (c.id(), run_tet(c.as_ref(), q).await) }
        }))
        .await;

        let mut records = Vec::new();
        let mut failures = Vec::new();
        for (id, outcome) in outcomes {
            match outcome {
                Ok(mut rs) => records.append(&mut rs),
                Err(e) => failures.push(Failure {
                    connector: id,
                    reason: e.to_string(),
                }),
            }
        }

        records = self.dedupe(records);
        records.sort_by_key(sort_key);

        // Failure order must be deterministic too — join_all preserves input order, but sorting
        // makes that independent of how the registry happens to be ordered.
        failures.sort_by(|a, b| a.connector.cmp(&b.connector));

        RoutedResult { records, failures }
    }

    /// Collapse records that describe the same thing, keeping the most-trusted source (A-4).
    fn dedupe(&self, records: Vec<AnyRecord>) -> Vec<AnyRecord> {
        let mut best: HashMap<ContentKey, AnyRecord> = HashMap::with_capacity(records.len());
        for r in records {
            let key = ContentKey::of(&r);
            match best.get(&key) {
                Some(existing) => {
                    let incoming_rank = self.precedence.rank(r.source());
                    let existing_rank = self.precedence.rank(existing.source());
                    // Strictly-better wins. On an exact tie the incumbent stays, so the outcome
                    // does not depend on which connector happened to answer first.
                    if incoming_rank < existing_rank {
                        best.insert(key, r);
                    }
                }
                None => {
                    best.insert(key, r);
                }
            }
        }
        best.into_values().collect()
    }
}

/// Drive one connector through all three TET stages (A-2).
async fn run_tet(c: &dyn Connector, q: &StandardQuery) -> crate::Result<Vec<AnyRecord>> {
    let pq = c.transform_query(q)?;
    let raw = c.extract(pq).await?;
    c.transform_data(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_erases_case_punctuation_and_whitespace() {
        // The case that matters: two systems, one meeting.
        assert_eq!(
            normalize("  Watch-Handover!  "),
            normalize("Watch handover")
        );
        assert_eq!(normalize("Watch handover"), "watchhandover");
        assert_eq!(normalize("Lens   Inspection"), normalize("lens inspection"));
        assert_eq!(
            normalize("calibrate   the LAMP rotation!!"),
            normalize("Calibrate the lamp rotation")
        );
        assert_eq!(normalize(""), "");
        assert_eq!(normalize("!!!"), "");
    }

    #[test]
    fn normalize_still_separates_genuinely_different_titles() {
        assert_ne!(normalize("Lens inspection"), normalize("Supply boat"));
        assert_ne!(normalize("Watch A"), normalize("Watch B"));
    }

    #[test]
    fn precedence_ranks_unknown_sources_last() {
        let p = SourcePrecedence::new(["gcal", "ics"]);
        assert!(p.rank("gcal") < p.rank("ics"));
        assert!(p.rank("ics") < p.rank("somebody-else"));
    }
}
