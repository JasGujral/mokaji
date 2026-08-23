//! # Fake connectors — the M-0 proof
//!
//! Two connectors backed by recorded fixtures. They exist so the **contract** can be proven before
//! a single real source is wired up:
//!
//! > **M-0 exit criterion.** Two *fake* connectors round-trip `Task`s and `Event`s through TET;
//! > the router merges, dedupes by content key and sorts deterministically; PRIV-5's "no other
//! > socket" test passes. All in `cargo test`. **Nothing visible on screen — that is correct.**
//!
//! ## Why the fixtures are spelled inconsistently on purpose
//!
//! `fixtures/calendar_gcal.json` speaks Google Calendar (`summary`, RFC3339 `start`) and
//! `fixtures/calendar_ics.json` speaks iCalendar (`SUMMARY`, `DTSTART`). Both contain the same
//! 09:00 handover, one of them written `"  Watch-Handover!  "`. That is not a typo — it is **A-4** in a
//! test tube. Deduping on `(source, source_ref)` keeps both, because `evt-1` and `ics-9911` are
//! different strings from different systems. Deduping on the content identity key
//! `(normalized_title, start, duration)` keeps one. The second behaviour is the correct one, and
//! at M-5 it stops being hypothetical.
//!
//! `fixtures/tasks_vault.json` does the same for tasks, and includes one task with no `due` so
//! **A-5**'s nulls-last ordering has something to be wrong about.
//!
//! **REL-4:** TET stages are tested against recorded fixtures, never a live API. These fakes are
//! the pattern every real connector's tests follow.

#![forbid(unsafe_code)]

use async_trait::async_trait;
use mokaji_core::connector::{
    Capability, Connector, Health, ProviderQuery, RawPayload, StandardQuery,
};
use mokaji_core::model::{
    Area, Event, Kind, PersonRef, Record, Rsvp, Task, {AnyRecord, ConnectorId},
};
use mokaji_core::version::RECORD_SCHEMA_VERSION;
use mokaji_core::{Error, Result};

pub use mokaji_core as core;

const GCAL: &str = include_str!("../fixtures/calendar_gcal.json");
const ICS: &str = include_str!("../fixtures/calendar_ics.json");
const TASKS: &str = include_str!("../fixtures/tasks_vault.json");

/// Which fixture dialect a [`FakeConnector`] speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// Google-Calendar-shaped events.
    GcalEvents,
    /// iCalendar-shaped events.
    IcsEvents,
    /// Vault-shaped tasks.
    VaultTasks,
}

/// Where a [`FakeConnector`] should fail, so A-2's stage naming and A-6's partial failure are
/// testable without waiting for a real source to break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailAt {
    /// Fail in `transform_query`.
    TransformQuery,
    /// Fail in `extract`.
    Extract,
    /// Fail in `transform_data`.
    TransformData,
}

/// A connector over recorded fixtures.
pub struct FakeConnector {
    id: ConnectorId,
    dialect: Dialect,
    caps: Vec<Capability>,
    fail_at: Option<FailAt>,
    /// Extra envelope keys, to prove A-12 preserves what this build does not understand.
    unknown_envelope_keys: bool,
}

impl FakeConnector {
    /// A Google-Calendar-shaped event source.
    #[must_use]
    pub fn gcal(id: &str) -> Self {
        Self::new(id, Dialect::GcalEvents, &[Capability::Read(Kind::Event)])
    }

    /// An iCalendar-shaped event source carrying an overlapping standup (A-4).
    #[must_use]
    pub fn ics(id: &str) -> Self {
        Self::new(id, Dialect::IcsEvents, &[Capability::Read(Kind::Event)])
    }

    /// A vault-shaped task source.
    #[must_use]
    pub fn vault(id: &str) -> Self {
        Self::new(id, Dialect::VaultTasks, &[Capability::Read(Kind::Task)])
    }

    fn new(id: &str, dialect: Dialect, caps: &[Capability]) -> Self {
        Self {
            id: id.to_owned(),
            dialect,
            caps: caps.to_vec(),
            fail_at: None,
            unknown_envelope_keys: false,
        }
    }

    /// Make this connector fail at a chosen stage.
    #[must_use]
    pub fn failing_at(mut self, stage: FailAt) -> Self {
        self.fail_at = Some(stage);
        self
    }

    /// Emit envelope keys this build does not know about (A-12).
    #[must_use]
    pub fn with_unknown_envelope_keys(mut self) -> Self {
        self.unknown_envelope_keys = true;
        self
    }

    fn err(&self, stage: mokaji_core::error::Stage, message: &str) -> Error {
        Error::Stage {
            connector: self.id.clone(),
            stage,
            message: message.to_owned(),
        }
    }

    fn fixture(&self) -> &'static str {
        match self.dialect {
            Dialect::GcalEvents => GCAL,
            Dialect::IcsEvents => ICS,
            Dialect::VaultTasks => TASKS,
        }
    }

    fn envelope<T>(&self, id: String, source_ref: String, data: T) -> Record<T> {
        let mut extra = serde_json::Map::new();
        if self.unknown_envelope_keys {
            // A field a future connector might add and this build has never heard of.
            extra.insert("confidence".into(), serde_json::json!(0.93));
        }
        Record {
            schema_version: RECORD_SCHEMA_VERSION,
            id,
            source: self.id.clone(),
            source_ref,
            area: Area::Personal,
            // Fixed so tests are deterministic; a real connector stamps the wall clock here.
            fetched_at: chrono::DateTime::UNIX_EPOCH,
            data,
            raw: None,
            extra,
        }
    }
}

fn parse_ts(s: &str, ctx: &str) -> std::result::Result<chrono::DateTime<chrono::Utc>, String> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&chrono::Utc))
        .map_err(|e| format!("{ctx}: {e}"))
}

#[async_trait]
impl Connector for FakeConnector {
    fn id(&self) -> ConnectorId {
        self.id.clone()
    }

    fn schema_version(&self) -> u16 {
        RECORD_SCHEMA_VERSION
    }

    fn capabilities(&self) -> &[Capability] {
        &self.caps
    }

    async fn health(&self) -> Health {
        match self.fail_at {
            None => Health::Ok,
            Some(stage) => Health::Down(format!("fixture connector is set to fail at {stage:?}")),
        }
    }

    fn transform_query(&self, q: &StandardQuery) -> Result<ProviderQuery> {
        if self.fail_at == Some(FailAt::TransformQuery) {
            return Err(self.err(
                mokaji_core::error::Stage::TransformQuery,
                "injected failure",
            ));
        }
        let expects = match self.dialect {
            Dialect::VaultTasks => Kind::Task,
            _ => Kind::Event,
        };
        if q.kind != expects {
            return Err(self.err(
                mokaji_core::error::Stage::TransformQuery,
                &format!(
                    "cannot serve {:?}, this source provides {expects:?}",
                    q.kind
                ),
            ));
        }
        Ok(ProviderQuery(serde_json::json!({
            "dialect": format!("{:?}", self.dialect),
            "window": q.window,
        })))
    }

    async fn extract(&self, _pq: ProviderQuery) -> Result<RawPayload> {
        if self.fail_at == Some(FailAt::Extract) {
            return Err(self.err(mokaji_core::error::Stage::Extract, "injected failure"));
        }
        let v: serde_json::Value = serde_json::from_str(self.fixture())
            .map_err(|e| self.err(mokaji_core::error::Stage::Extract, &e.to_string()))?;
        Ok(RawPayload(v))
    }

    fn transform_data(&self, raw: RawPayload) -> Result<Vec<AnyRecord>> {
        if self.fail_at == Some(FailAt::TransformData) {
            return Err(self.err(mokaji_core::error::Stage::TransformData, "injected failure"));
        }
        let stage = mokaji_core::error::Stage::TransformData;
        let bad = |m: String| self.err(stage, &m);

        match self.dialect {
            Dialect::GcalEvents => {
                let items = raw.0["items"]
                    .as_array()
                    .ok_or_else(|| bad("`items` missing or not an array".into()))?;
                items
                    .iter()
                    .map(|it| {
                        let id = it["id"].as_str().unwrap_or_default();
                        let event = Event {
                            // `summary` becomes `title` — one concept, one name.
                            title: it["summary"].as_str().unwrap_or_default().to_owned(),
                            start: parse_ts(it["start"].as_str().unwrap_or_default(), "start")
                                .map_err(&bad)?,
                            end: parse_ts(it["end"].as_str().unwrap_or_default(), "end")
                                .map_err(&bad)?,
                            all_day: false,
                            location: it["location"].as_str().map(ToOwned::to_owned),
                            attendees: it["attendees"]
                                .as_array()
                                .map(|a| {
                                    a.iter()
                                        .map(|p| PersonRef {
                                            name: p["displayName"].as_str().map(ToOwned::to_owned),
                                            email: p["email"].as_str().map(ToOwned::to_owned),
                                        })
                                        .collect()
                                })
                                .unwrap_or_default(),
                            response: match it["responseStatus"].as_str() {
                                Some("accepted") => Some(Rsvp::Accepted),
                                Some("declined") => Some(Rsvp::Declined),
                                Some("tentative") => Some(Rsvp::Tentative),
                                Some("needsAction") => Some(Rsvp::NeedsAction),
                                _ => None,
                            },
                        };
                        Ok(AnyRecord::Event(self.envelope(
                            format!("{}:event:{id}", self.id),
                            id.to_owned(),
                            event,
                        )))
                    })
                    .collect()
            }
            Dialect::IcsEvents => {
                let items = raw.0["VEVENT"]
                    .as_array()
                    .ok_or_else(|| bad("`VEVENT` missing or not an array".into()))?;
                items
                    .iter()
                    .map(|it| {
                        let uid = it["UID"].as_str().unwrap_or_default();
                        let event = Event {
                            // `SUMMARY` becomes `title` too.
                            title: it["SUMMARY"].as_str().unwrap_or_default().to_owned(),
                            start: parse_ts(it["DTSTART"].as_str().unwrap_or_default(), "DTSTART")
                                .map_err(&bad)?,
                            end: parse_ts(it["DTEND"].as_str().unwrap_or_default(), "DTEND")
                                .map_err(&bad)?,
                            all_day: false,
                            location: it["LOCATION"].as_str().map(ToOwned::to_owned),
                            attendees: Vec::new(),
                            response: None,
                        };
                        Ok(AnyRecord::Event(self.envelope(
                            format!("{}:event:{uid}", self.id),
                            uid.to_owned(),
                            event,
                        )))
                    })
                    .collect()
            }
            Dialect::VaultTasks => {
                let items = raw.0["tasks"]
                    .as_array()
                    .ok_or_else(|| bad("`tasks` missing or not an array".into()))?;
                items
                    .iter()
                    .map(|it| {
                        let r = it["ref"].as_str().unwrap_or_default();
                        // X-10: free-text dates are parsed to a DateTime *here*, at the connector
                        // boundary — never regexed downstream.
                        let due = match it["due"].as_str() {
                            Some(s) => Some(parse_ts(s, "due").map_err(&bad)?),
                            None => None,
                        };
                        let task = Task {
                            text: it["text"].as_str().unwrap_or_default().to_owned(),
                            done: it["done"].as_bool().unwrap_or(false),
                            done_at: None,
                            due,
                            quad: None,
                            project: None,
                            tags: Vec::new(),
                        };
                        Ok(AnyRecord::Task(self.envelope(
                            format!("{}:task:{r}", self.id),
                            r.to_owned(),
                            task,
                        )))
                    })
                    .collect()
            }
        }
    }
}
