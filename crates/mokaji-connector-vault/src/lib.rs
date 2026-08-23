//! # mokaji-connector-vault — connector #1
//!
//! Reads an Obsidian vault. Deliberately not a special case: it implements the same `Connector`
//! trait every other source will, so the core stays empty and the vault has no privileged path
//! into it.
//!
//! **B-4: writes are dry-run by default.** [`write::VaultWriter`] prints diffs and applies
//! nothing until it is explicitly armed. The `Connector` still declares Read-only capabilities:
//! the write path is exercised directly by the Console and the voice loop, and it will be declared
//! through the trait once `apply` is wired to `Mutation` (A-8).

#![forbid(unsafe_code)]

pub mod parse;
pub mod write;

use mokaji_core::connector::{
    Capability, Connector, Health, ProviderQuery, RawPayload, StandardQuery,
};
use mokaji_core::model::{
    AnyRecord, Area, Chaser, ChaserKind, ConnectorId, Kind, Metric, MetricValue, PersonRef, Record,
    Task,
};
use mokaji_core::version::RECORD_SCHEMA_VERSION;
use mokaji_core::{Error, Result};

use async_trait::async_trait;
use chrono::{NaiveDate, TimeZone, Utc};
use std::path::{Path, PathBuf};

/// Folders the connector reads, matching the vault's PARA layout.
mod folder {
    /// Task sources. The dashboard's Dataview reads exactly these two.
    pub const TASK_SOURCES: [&str; 2] = ["01 Projects", "08 Journal/Daily"];
    /// Daily notes, for tracker metrics.
    pub const DAILY: &str = "08 Journal/Daily";
    /// The chasers file.
    pub const CHASERS: &str = "09 Command Center/Chasers.md";
}

/// Tracker keys read from daily-note frontmatter.
///
/// `sleep_hours` and `exercised` are in the vault template but missing from the requirements
/// doc's Metric list — the vault is the ground truth here, so they are read.
const TRACKER_KEYS: [&str; 5] = ["mood", "energy", "focus", "deep_work_hours", "sleep_hours"];
const TRACKER_BOOL_KEYS: [&str; 1] = ["exercised"];

/// Reads an Obsidian vault from the filesystem (X-6: filesystem reads, not `obsidian://`).
pub struct VaultConnector {
    id: ConnectorId,
    root: PathBuf,
}

impl VaultConnector {
    /// Point at a vault root.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            id: "vault".into(),
            root: root.into(),
        }
    }

    /// Override the connector id (useful when two vaults are mounted).
    #[must_use]
    pub fn with_id(mut self, id: &str) -> Self {
        self.id = id.into();
        self
    }

    /// The vault root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn err(&self, stage: mokaji_core::error::Stage, message: impl Into<String>) -> Error {
        Error::Stage {
            connector: self.id.clone(),
            stage,
            message: message.into(),
        }
    }
}

/// Every `.md` file under `dir`, recursively, in a stable order.
///
/// Sorted because A-5 promises identical results for identical queries, and directory iteration
/// order is a property of the filesystem, not of the data.
fn markdown_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // Obsidian's own state, version control, and our backups are not content.
            if name.starts_with('.') || name == "_backups" {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

fn read_files(root: &Path, rel_paths: &[&str]) -> std::io::Result<Vec<serde_json::Value>> {
    let mut files = Vec::new();
    for rel in rel_paths {
        let p = root.join(rel);
        let candidates = if p.is_dir() {
            markdown_files(&p)?
        } else if p.exists() {
            vec![p]
        } else {
            vec![]
        };
        for path in candidates {
            let content = std::fs::read_to_string(&path)?;
            let rel_display = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            files.push(serde_json::json!({ "path": rel_display, "content": content }));
        }
    }
    Ok(files)
}

fn local_midnight_utc(d: NaiveDate) -> chrono::DateTime<Utc> {
    // §5: a due date written as a bare day means "by the end of that local day". Stored UTC.
    let naive = d.and_hms_opt(23, 59, 59).expect("23:59:59 is a valid time");
    chrono::Local
        .from_local_datetime(&naive)
        .earliest()
        .map_or_else(
            || Utc.from_utc_datetime(&naive),
            |dt| dt.with_timezone(&Utc),
        )
}

#[async_trait]
impl Connector for VaultConnector {
    fn id(&self) -> ConnectorId {
        self.id.clone()
    }

    fn schema_version(&self) -> u16 {
        RECORD_SCHEMA_VERSION
    }

    fn capabilities(&self) -> &[Capability] {
        // Read-only until the write path has its hash guard and snapshot (B-3/B-5).
        &[
            Capability::Read(Kind::Task),
            Capability::Read(Kind::Metric),
            Capability::Read(Kind::Chaser),
        ]
    }

    async fn health(&self) -> Health {
        if !self.root.exists() {
            return Health::Down(format!("vault not found at {}", self.root.display()));
        }
        if !self.root.join(folder::DAILY).exists() {
            return Health::Degraded(format!(
                "no `{}` folder — metrics will be empty",
                folder::DAILY
            ));
        }
        Health::Ok
    }

    fn transform_query(&self, q: &StandardQuery) -> Result<ProviderQuery> {
        let paths: Vec<&str> = match q.kind {
            Kind::Task => folder::TASK_SOURCES.to_vec(),
            Kind::Metric => vec![folder::DAILY],
            Kind::Chaser => vec![folder::CHASERS],
            other => {
                return Err(self.err(
                    mokaji_core::error::Stage::TransformQuery,
                    format!("the vault connector does not serve {other:?} yet"),
                ))
            }
        };
        Ok(ProviderQuery(serde_json::json!({
            "kind": q.kind,
            "paths": paths,
        })))
    }

    async fn extract(&self, pq: ProviderQuery) -> Result<RawPayload> {
        let stage = mokaji_core::error::Stage::Extract;
        let paths: Vec<String> = pq.0["paths"]
            .as_array()
            .ok_or_else(|| self.err(stage, "provider query has no `paths`"))?
            .iter()
            .filter_map(|v| v.as_str().map(ToOwned::to_owned))
            .collect();
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        let files = read_files(&self.root, &refs)
            .map_err(|e| self.err(stage, format!("reading {}: {e}", self.root.display())))?;
        Ok(RawPayload(serde_json::json!({
            "kind": pq.0["kind"].clone(),
            "files": files,
        })))
    }

    fn transform_data(&self, raw: RawPayload) -> Result<Vec<AnyRecord>> {
        let stage = mokaji_core::error::Stage::TransformData;
        let kind: Kind = serde_json::from_value(raw.0["kind"].clone())
            .map_err(|e| self.err(stage, format!("unreadable kind: {e}")))?;
        let files = raw.0["files"]
            .as_array()
            .ok_or_else(|| self.err(stage, "payload has no `files`"))?;

        let mut out = Vec::new();
        for f in files {
            let path = f["path"].as_str().unwrap_or_default();
            let content = f["content"].as_str().unwrap_or_default();
            let (fm, body, body_start) = parse::split_frontmatter(content);

            match kind {
                Kind::Task => {
                    for t in parse::tasks(body, body_start) {
                        let source_ref = format!("{path}#L{}", t.line);
                        out.push(AnyRecord::Task(Record {
                            schema_version: RECORD_SCHEMA_VERSION,
                            id: format!("{}:task:{source_ref}", self.id),
                            source: self.id.clone(),
                            source_ref,
                            area: Area::Personal,
                            fetched_at: Utc::now(),
                            data: Task {
                                text: t.text,
                                done: t.done,
                                done_at: t.completion,
                                due: t.due.map(local_midnight_utc),
                                quad: None,
                                project: project_of(path),
                                tags: t.tags,
                            },
                            raw: None,
                            extra: serde_json::Map::new(),
                        }));
                    }
                }
                Kind::Chaser => {
                    for t in parse::tasks(body, body_start) {
                        let kind = if t.tags.iter().any(|x| x == "waiting") {
                            ChaserKind::Waiting
                        } else if t.tags.iter().any(|x| x == "nudge") {
                            ChaserKind::Nudge
                        } else {
                            continue; // an untagged task in Chasers.md is a note to self, not a chaser
                        };
                        let source_ref = format!("{path}#L{}", t.line);
                        out.push(AnyRecord::Chaser(Record {
                            schema_version: RECORD_SCHEMA_VERSION,
                            id: format!("{}:chaser:{source_ref}", self.id),
                            source: self.id.clone(),
                            source_ref,
                            area: Area::Personal,
                            fetched_at: Utc::now(),
                            data: Chaser {
                                kind,
                                who: PersonRef {
                                    name: None,
                                    email: None,
                                },
                                what: t.text.clone(),
                                since: parse::since_date(&t.text)
                                    .unwrap_or_else(|| Utc::now().date_naive()),
                                last: None,
                                overdue: t.tags.iter().any(|x| x == "overdue"),
                            },
                            raw: None,
                            extra: serde_json::Map::new(),
                        }));
                    }
                }
                Kind::Metric => {
                    let Some(fm) = fm else { continue };
                    if parse::frontmatter_value(fm, "type") != Some("daily") {
                        continue;
                    }
                    let Some(at) = daily_note_date(path, fm) else {
                        continue;
                    };
                    for key in TRACKER_KEYS {
                        if let Some(v) = parse::frontmatter_value(fm, key) {
                            if let Ok(n) = v.parse::<f64>() {
                                out.push(metric_record(
                                    self,
                                    path,
                                    key,
                                    MetricValue::Number(n),
                                    at,
                                ));
                            }
                        }
                    }
                    for key in TRACKER_BOOL_KEYS {
                        if let Some(v) = parse::frontmatter_value(fm, key) {
                            if let Ok(b) = v.parse::<bool>() {
                                out.push(metric_record(self, path, key, MetricValue::Bool(b), at));
                            }
                        }
                    }
                }
                other => {
                    return Err(self.err(stage, format!("cannot transform {other:?}")));
                }
            }
        }
        Ok(out)
    }
}

fn metric_record(
    c: &VaultConnector,
    path: &str,
    key: &str,
    value: MetricValue,
    at: NaiveDate,
) -> AnyRecord {
    AnyRecord::Metric(Record {
        schema_version: RECORD_SCHEMA_VERSION,
        id: format!("{}:metric:{path}:{key}", c.id),
        source: c.id.clone(),
        source_ref: path.to_owned(),
        area: Area::Personal,
        fetched_at: Utc::now(),
        data: Metric {
            key: key.to_owned(),
            value,
            at,
        },
        raw: None,
        extra: serde_json::Map::new(),
    })
}

/// A daily note's date, from its filename first and `created:` as a fallback. The filename is
/// authoritative because Periodic Notes derives it, while `created` can be copied by a template.
fn daily_note_date(path: &str, fm: &str) -> Option<NaiveDate> {
    let stem = Path::new(path).file_stem()?.to_string_lossy().to_string();
    NaiveDate::parse_from_str(&stem, "%Y-%m-%d")
        .ok()
        .or_else(|| {
            parse::frontmatter_value(fm, "created")
                .and_then(|v| NaiveDate::parse_from_str(v, "%Y-%m-%d").ok())
        })
}

/// A task's owning project, when it came from `01 Projects`.
fn project_of(path: &str) -> Option<String> {
    path.strip_prefix("01 Projects/")
        .and_then(|p| Path::new(p).file_stem())
        .map(|s| s.to_string_lossy().to_string())
}
