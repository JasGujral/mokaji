//! # mokaji-connector-ics — the calendar that needs no credentials
//!
//! **B-7's secondary path, promoted to first.** Google Calendar needs an OAuth client, app
//! verification and a browser round-trip; a folder of `.ics` files needs none of that, and every
//! calendar application on the machine can export or subscribe to one. So this lands first, and
//! `calLoad` stops being permanently zero without waiting on anyone's API console.
//!
//! It also earns its place architecturally. A-4 dedupes on a content identity key precisely
//! because the same meeting arrives from two calendars — and until now that was a claim proven
//! only by fixtures. With `.ics` and Google both feeding `Event`, it becomes the ordinary case.

#![forbid(unsafe_code)]

pub mod parse;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Local, TimeZone, Utc};
use mokaji_core::connector::{
    Capability, Connector, Health, ProviderQuery, RawPayload, StandardQuery,
};
use mokaji_core::model::{AnyRecord, Area, ConnectorId, Event, Kind, PersonRef, Record};
use mokaji_core::version::RECORD_SCHEMA_VERSION;
use mokaji_core::{Error, Result};
use std::path::{Path, PathBuf};

/// Reads `.ics` files from a folder.
pub struct IcsConnector {
    id: ConnectorId,
    root: PathBuf,
}

impl IcsConnector {
    /// Point at a folder of `.ics` files.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            id: "ics".into(),
            root: root.into(),
        }
    }

    /// Override the connector id, for a second calendar folder.
    #[must_use]
    pub fn with_id(mut self, id: &str) -> Self {
        self.id = id.into();
        self
    }

    fn err(&self, stage: mokaji_core::error::Stage, message: impl Into<String>) -> Error {
        Error::Stage {
            connector: self.id.clone(),
            stage,
            message: message.into(),
        }
    }
}

/// Resolve a `StandardQuery`'s window to a UTC range, using the **local** calendar day (§5).
#[must_use]
pub fn window(spec: Option<&str>, now: DateTime<Local>) -> (DateTime<Utc>, DateTime<Utc>) {
    let start_of_today = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|n| Local.from_local_datetime(&n).earliest())
        .map_or(now, |d| d);
    let days = match spec.unwrap_or("today") {
        "today" => 1,
        "tomorrow" => 2,
        "week" => 7,
        "month" => 31,
        _ => 1,
    };
    let from = if spec == Some("tomorrow") {
        start_of_today + Duration::days(1)
    } else {
        start_of_today
    };
    (
        from.with_timezone(&Utc),
        (start_of_today + Duration::days(days)).with_timezone(&Utc),
    )
}

fn ics_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let entry = entry?;
            let p = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ics")) {
                out.push(p);
            }
        }
    }
    // A-5 promises identical results for identical queries; directory order is a property of the
    // filesystem, not of the data.
    out.sort();
    Ok(out)
}

#[async_trait]
impl Connector for IcsConnector {
    fn id(&self) -> ConnectorId {
        self.id.clone()
    }

    fn schema_version(&self) -> u16 {
        RECORD_SCHEMA_VERSION
    }

    fn capabilities(&self) -> &[Capability] {
        &[Capability::Read(Kind::Event)]
    }

    async fn health(&self) -> Health {
        if !self.root.exists() {
            return Health::Down(format!("no calendar folder at {}", self.root.display()));
        }
        match ics_files(&self.root) {
            Ok(f) if f.is_empty() => {
                Health::Degraded(format!("no .ics files in {}", self.root.display()))
            }
            Ok(_) => Health::Ok,
            Err(e) => Health::Down(e.to_string()),
        }
    }

    fn transform_query(&self, q: &StandardQuery) -> Result<ProviderQuery> {
        if q.kind != Kind::Event {
            return Err(self.err(
                mokaji_core::error::Stage::TransformQuery,
                format!("this source provides Events, not {:?}", q.kind),
            ));
        }
        let (from, to) = window(q.window.as_deref(), Local::now());
        Ok(ProviderQuery(serde_json::json!({
            "from": from.to_rfc3339(),
            "to": to.to_rfc3339(),
        })))
    }

    async fn extract(&self, pq: ProviderQuery) -> Result<RawPayload> {
        let stage = mokaji_core::error::Stage::Extract;
        let files = ics_files(&self.root)
            .map_err(|e| self.err(stage, format!("reading {}: {e}", self.root.display())))?;
        let mut docs = Vec::new();
        for f in files {
            let text = std::fs::read_to_string(&f)
                .map_err(|e| self.err(stage, format!("{}: {e}", f.display())))?;
            let name = f
                .strip_prefix(&self.root)
                .unwrap_or(&f)
                .to_string_lossy()
                .to_string();
            docs.push(serde_json::json!({ "path": name, "text": text }));
        }
        Ok(RawPayload(serde_json::json!({
            "from": pq.0["from"].clone(),
            "to": pq.0["to"].clone(),
            "docs": docs,
        })))
    }

    fn transform_data(&self, raw: RawPayload) -> Result<Vec<AnyRecord>> {
        let stage = mokaji_core::error::Stage::TransformData;
        let parse_bound = |k: &str| -> Result<DateTime<Utc>> {
            raw.0[k]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.with_timezone(&Utc))
                .ok_or_else(|| self.err(stage, format!("payload has no usable `{k}`")))
        };
        let from = parse_bound("from")?;
        let to = parse_bound("to")?;

        let docs = raw.0["docs"]
            .as_array()
            .ok_or_else(|| self.err(stage, "payload has no `docs`"))?;

        let mut out = Vec::new();
        for doc in docs {
            let path = doc["path"].as_str().unwrap_or_default();
            let text = doc["text"].as_str().unwrap_or_default();
            for base in parse::events(text) {
                for occ in parse::expand(&base, from, to) {
                    let source_ref = format!("{path}#{}", occ.uid);
                    out.push(AnyRecord::Event(Record {
                        schema_version: RECORD_SCHEMA_VERSION,
                        // The id carries the start, so two occurrences of a recurring event are
                        // two records rather than one that overwrites the other.
                        id: format!("{}:event:{}:{}", self.id, occ.uid, occ.start.timestamp()),
                        source: self.id.clone(),
                        source_ref,
                        area: Area::Personal,
                        fetched_at: Utc::now(),
                        data: Event {
                            // `SUMMARY` becomes `title` — one concept, one name (§5).
                            title: occ.summary.clone(),
                            start: occ.start,
                            end: occ.end,
                            all_day: occ.all_day,
                            location: occ.location.clone(),
                            attendees: occ
                                .attendees
                                .iter()
                                .map(|a| PersonRef {
                                    name: None,
                                    email: Some(a.clone()),
                                })
                                .collect(),
                            response: None,
                        },
                        raw: None,
                        extra: serde_json::Map::new(),
                    }));
                }
            }
        }
        Ok(out)
    }
}
