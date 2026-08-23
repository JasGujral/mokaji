//! Standard models — **A-1, §5**. *The contract.*
//!
//! Connector-specific data lives in [`Record::raw`], **never** in typed fields. A concept has
//! exactly one name across all connectors: a Google Calendar `summary` and an `.ics` `SUMMARY`
//! both become `title`.
//!
//! **Time semantics (§5).** All instants are stored UTC and rendered in the machine's local
//! timezone. "Today", "due today", the daily-note filename and the morning-briefing boundary all
//! use the **local calendar day**, rolling over at local midnight. `since`/`last` on chasers are
//! local dates.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Stable record identity, e.g. `"vault:task:9f3a…"` — stable across syncs.
pub type RecordId = String;

/// Connector identity, e.g. `"vault"`, `"gcal"`, `"gmail"`.
pub type ConnectorId = String;

/// Which part of life a record belongs to. Work mode is deferred (X-2) but reserved here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Area {
    /// Work.
    Work,
    /// Personal.
    Personal,
    /// Neither, or not yet classified.
    Other,
}

/// Eisenhower quadrant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quadrant {
    /// Urgent + important.
    Do,
    /// Important, not urgent.
    Schedule,
    /// Urgent, not important.
    Delegate,
    /// Neither.
    Eliminate,
}

/// The envelope every record shares (§5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record<T> {
    /// A-12.
    pub schema_version: u16,
    /// Stable across syncs.
    pub id: RecordId,
    /// Which connector produced this.
    pub source: ConnectorId,
    /// Pointer home, e.g. `"08 Journal/Daily/2026-08-13.md#L42"`.
    pub source_ref: String,
    /// Work / Personal / Other.
    pub area: Area,
    /// When this was pulled from the source.
    pub fetched_at: DateTime<Utc>,
    /// The typed payload.
    pub data: T,
    /// Connector-specific extras. Never promote these into typed fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<serde_json::Value>,
    /// **A-12.** Envelope keys this build does not know about. Captured on deserialization and
    /// written back out unchanged, so a record produced by a newer connector survives a
    /// round-trip through an older core instead of being silently truncated.
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// A reference to a person — resolved to a full [`Person`] only when a connector can.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonRef {
    /// Display name if known.
    pub name: Option<String>,
    /// Email if known.
    pub email: Option<String>,
}

/// A task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// The task text.
    pub text: String,
    /// Whether it is done.
    pub done: bool,
    /// Local date it was completed. **X-11: `momentum` counts done *today*, not all-time.**
    pub done_at: Option<NaiveDate>,
    /// Due instant. **X-10: `urgent` is a typed predicate on this, never a regex on free text.**
    /// Free-text dates are parsed to a `DateTime` at the connector boundary (C-8).
    pub due: Option<DateTime<Utc>>,
    /// Eisenhower quadrant, if bucketed.
    pub quad: Option<Quadrant>,
    /// Owning project.
    pub project: Option<String>,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// An RSVP response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rsvp {
    /// Accepted.
    Accepted,
    /// Declined.
    Declined,
    /// Tentative.
    Tentative,
    /// No response yet.
    NeedsAction,
}

/// A calendar event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Title. (`summary` / `SUMMARY` both normalize to this.)
    pub title: String,
    /// Start instant (UTC).
    pub start: DateTime<Utc>,
    /// End instant (UTC).
    pub end: DateTime<Utc>,
    /// All-day flag.
    pub all_day: bool,
    /// Location.
    pub location: Option<String>,
    /// Attendees.
    #[serde(default)]
    pub attendees: Vec<PersonRef>,
    /// Own RSVP.
    pub response: Option<Rsvp>,
}

/// What kind of chaser this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChaserKind {
    /// Jas asked for something and is waiting on it.
    Waiting,
    /// Someone is waiting on Jas.
    Nudge,
}

/// Something owed, in either direction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chaser {
    /// Waiting-on vs nudge.
    pub kind: ChaserKind,
    /// The counterparty.
    pub who: PersonRef,
    /// What is owed.
    pub what: String,
    /// Local date it started.
    pub since: NaiveDate,
    /// Local date of the last poke.
    pub last: Option<NaiveDate>,
    /// Whether it is past due.
    pub overdue: bool,
}

/// Note kind, per the vault's Zettelkasten layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    /// Inbox capture.
    Fleeting,
    /// Processed, evergreen.
    Permanent,
    /// Map of content.
    Moc,
}

/// A note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    /// Title.
    pub title: String,
    /// Body markdown.
    pub body: String,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Fleeting / permanent / MOC.
    pub kind: NoteKind,
}

/// A person.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    /// Display name.
    pub name: String,
    /// Known emails.
    #[serde(default)]
    pub emails: Vec<String>,
    /// Other handles, keyed by service.
    #[serde(default)]
    pub handles: BTreeMap<String, String>,
}

/// A tracked metric value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MetricValue {
    /// Numeric (`mood`, `energy`, `focus`, `deep_work_hours`).
    Number(f64),
    /// Boolean (habit done / not done).
    Bool(bool),
    /// Free text.
    Text(String),
}

/// A daily-note tracker metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    /// `mood` / `energy` / `focus` / `deep_work_hours` / habit name.
    pub key: String,
    /// The value.
    pub value: MetricValue,
    /// Local date it applies to.
    pub at: NaiveDate,
}

/// An inbound message (Gmail in v1 — **read + classify only**, B-9).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Sender.
    pub from: PersonRef,
    /// Subject.
    pub subject: String,
    /// Snippet.
    pub snippet: String,
    /// Received instant (UTC).
    pub received: DateTime<Utc>,
    /// Whether it asks something of the user.
    pub needs_action: bool,
    /// Pointer back to the thread.
    pub thread_ref: String,
}

/// A goal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    /// The goal text.
    pub text: String,
    /// Horizon, e.g. `"quarter"`, `"year"`.
    pub horizon: String,
    /// Status.
    pub status: String,
    /// Target local date.
    pub target_date: Option<NaiveDate>,
    /// Percent complete.
    pub pct: Option<u8>,
}

/// A financial transaction. **Defined now, unimplemented in v1** (§5) — it exists so the Tier 3
/// finance connector is purely additive (§6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// Signed amount in minor units.
    pub amount: i64,
    /// ISO 4217 currency.
    pub currency: String,
    /// Merchant.
    pub merchant: String,
    /// Category.
    pub category: String,
    /// Instant (UTC).
    pub at: DateTime<Utc>,
}

/// The model kinds the router can be asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// [`Task`].
    Task,
    /// [`Event`].
    Event,
    /// [`Chaser`].
    Chaser,
    /// [`Note`].
    Note,
    /// [`Person`].
    Person,
    /// [`Metric`].
    Metric,
    /// [`Message`].
    Message,
    /// [`Goal`].
    Goal,
    /// [`Transaction`] — reserved, not implemented in v1.
    Transaction,
}

/// A record of any kind, as it travels through the router.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnyRecord {
    /// A task.
    Task(Record<Task>),
    /// An event.
    Event(Record<Event>),
    /// A chaser.
    Chaser(Record<Chaser>),
    /// A note.
    Note(Record<Note>),
    /// A person.
    Person(Record<Person>),
    /// A metric.
    Metric(Record<Metric>),
    /// A message.
    Message(Record<Message>),
    /// A goal.
    Goal(Record<Goal>),
}

impl AnyRecord {
    /// The [`Kind`] discriminant.
    #[must_use]
    pub fn kind(&self) -> Kind {
        match self {
            Self::Task(_) => Kind::Task,
            Self::Event(_) => Kind::Event,
            Self::Chaser(_) => Kind::Chaser,
            Self::Note(_) => Kind::Note,
            Self::Person(_) => Kind::Person,
            Self::Metric(_) => Kind::Metric,
            Self::Message(_) => Kind::Message,
            Self::Goal(_) => Kind::Goal,
        }
    }

    /// Which connector produced this record.
    #[must_use]
    pub fn source(&self) -> &str {
        match self {
            Self::Task(r) => &r.source,
            Self::Event(r) => &r.source,
            Self::Chaser(r) => &r.source,
            Self::Note(r) => &r.source,
            Self::Person(r) => &r.source,
            Self::Metric(r) => &r.source,
            Self::Message(r) => &r.source,
            Self::Goal(r) => &r.source,
        }
    }
}
