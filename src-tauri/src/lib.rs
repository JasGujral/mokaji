//! The Tauri shell's Rust side.
//!
//! **SEC-1: default-deny.** The renderer gets exactly the commands below and nothing else. It
//! never receives a credential, never opens a socket, and never touches the filesystem directly —
//! every path into the vault goes through the connector, so the renderer cannot bypass the
//! contract even by accident.
//!
//! **X-14:** canonical state lives in connector sources. Nothing here is a store; every command
//! resolves fresh through the router. Only *UI* state (deck layout, panel sizes, prefs) persists
//! client-side.

use mokaji_connector_vault::VaultConnector;
use mokaji_core::connector::{Connector, Health, StandardQuery};
use mokaji_core::metrics::Readiness;
use mokaji_core::model::{AnyRecord, Kind, MetricValue};
use mokaji_core::router::Router;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;

/// Where the vault is, resolved once at startup.
pub struct AppState {
    vault: PathBuf,
}

impl AppState {
    fn connectors(&self) -> Vec<Arc<dyn Connector>> {
        vec![Arc::new(VaultConnector::new(&self.vault))]
    }
}

/// H-3: an empty config must still boot to a working app against a discovered vault.
fn discover_vault() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MOKAJI_VAULT_PATH") {
        let p = PathBuf::from(p);
        if p.join("08 Journal/Daily").is_dir() {
            return Some(p);
        }
    }
    let cwd = std::env::current_dir().ok()?;
    for dir in cwd.ancestors() {
        if dir.join("08 Journal/Daily").is_dir() {
            return Some(dir.to_path_buf());
        }
        let mut children: Vec<PathBuf> = std::fs::read_dir(dir)
            .ok()?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        children.sort();
        if let Some(found) = children
            .into_iter()
            .find(|c| c.join("08 Journal/Daily").is_dir())
        {
            return Some(found);
        }
    }
    None
}

/// The Reactor Core readout, plus the health badges that go with it (A-6).
#[derive(Serialize)]
pub struct CoreView {
    readiness: u8,
    state: String,
    focus: u8,
    momentum: u8,
    bandwidth: u8,
    cal_load: u8,
    open: usize,
    done_today: usize,
    urgent: usize,
    overdue: usize,
    events: usize,
    /// Non-fatal connector failures. The Deck shows a badge; it does not blank (A-6).
    failures: Vec<FailureView>,
}

#[derive(Serialize)]
pub struct FailureView {
    connector: String,
    reason: String,
}

/// A task, flattened for the renderer. `source_ref` is what makes a record openable — a number you
/// cannot trace back to a line in a file is a number you cannot trust.
#[derive(Serialize)]
pub struct TaskView {
    id: String,
    text: String,
    done: bool,
    /// RFC3339, or null. The renderer formats; it does not parse dates out of prose.
    due: Option<String>,
    urgent: bool,
    project: Option<String>,
    tags: Vec<String>,
    source: String,
    source_ref: String,
}

#[derive(Serialize)]
pub struct ChaserView {
    id: String,
    kind: String,
    what: String,
    since: String,
    overdue: bool,
    source_ref: String,
}

#[derive(Serialize)]
pub struct MetricView {
    key: String,
    value: String,
    at: String,
}

#[derive(Serialize)]
pub struct HealthView {
    connector: String,
    state: String,
    detail: Option<String>,
}

#[derive(Serialize)]
pub struct BootInfo {
    vault: Option<String>,
    version: String,
    /// Milestone the running build corresponds to, so a screenshot is never ambiguous.
    milestone: String,
}

fn q(kind: Kind) -> StandardQuery {
    StandardQuery {
        kind,
        window: None,
        params: serde_json::Map::new(),
    }
}

#[tauri::command]
fn boot_info(state: tauri::State<'_, AppState>) -> BootInfo {
    BootInfo {
        vault: Some(state.vault.display().to_string()).filter(|s| !s.is_empty()),
        version: env!("CARGO_PKG_VERSION").to_string(),
        milestone: "M-1".to_string(),
    }
}

#[tauri::command]
async fn core(state: tauri::State<'_, AppState>) -> Result<CoreView, String> {
    let connectors = state.connectors();
    let router = Router::new();
    let tasks = router.resolve(&connectors, &q(Kind::Task)).await;
    let chasers = router.resolve(&connectors, &q(Kind::Chaser)).await;

    let mut all = tasks.records.clone();
    all.extend(chasers.records.clone());
    let r = Readiness::compute(&all, chrono::Local::now());

    let failures = tasks
        .failures
        .iter()
        .chain(chasers.failures.iter())
        .map(|f| FailureView {
            connector: f.connector.clone(),
            reason: f.reason.clone(),
        })
        .collect();

    Ok(CoreView {
        readiness: r.readiness,
        state: r.state().to_string(),
        focus: r.focus,
        momentum: r.momentum,
        bandwidth: r.bandwidth,
        cal_load: r.cal_load,
        open: r.open,
        done_today: r.done_today,
        urgent: r.urgent,
        overdue: r.overdue,
        events: r.events,
        failures,
    })
}

#[tauri::command]
async fn tasks(state: tauri::State<'_, AppState>) -> Result<Vec<TaskView>, String> {
    let out = Router::new()
        .resolve(&state.connectors(), &q(Kind::Task))
        .await;
    let end_of_today = mokaji_core::metrics::end_of_local_day(chrono::Local::now());
    Ok(out
        .records
        .iter()
        .filter_map(|r| match r {
            AnyRecord::Task(t) if !t.data.done => Some(TaskView {
                id: t.id.clone(),
                text: t.data.text.clone(),
                done: t.data.done,
                due: t.data.due.map(|d| d.to_rfc3339()),
                urgent: t.data.due.is_some_and(|d| d <= end_of_today),
                project: t.data.project.clone(),
                tags: t.data.tags.clone(),
                source: t.source.clone(),
                source_ref: t.source_ref.clone(),
            }),
            _ => None,
        })
        .collect())
}

#[tauri::command]
async fn chasers(state: tauri::State<'_, AppState>) -> Result<Vec<ChaserView>, String> {
    let out = Router::new()
        .resolve(&state.connectors(), &q(Kind::Chaser))
        .await;
    Ok(out
        .records
        .iter()
        .filter_map(|r| match r {
            AnyRecord::Chaser(c) => Some(ChaserView {
                id: c.id.clone(),
                kind: format!("{:?}", c.data.kind).to_lowercase(),
                what: c.data.what.clone(),
                since: c.data.since.to_string(),
                overdue: c.data.overdue,
                source_ref: c.source_ref.clone(),
            }),
            _ => None,
        })
        .collect())
}

#[tauri::command]
async fn vitals(state: tauri::State<'_, AppState>) -> Result<Vec<MetricView>, String> {
    let out = Router::new()
        .resolve(&state.connectors(), &q(Kind::Metric))
        .await;
    let today = chrono::Local::now().date_naive();
    Ok(out
        .records
        .iter()
        .filter_map(|r| match r {
            AnyRecord::Metric(m) if m.data.at == today => Some(MetricView {
                key: m.data.key.clone(),
                value: match &m.data.value {
                    MetricValue::Number(n) => n.to_string(),
                    MetricValue::Bool(b) => b.to_string(),
                    MetricValue::Text(t) => t.clone(),
                },
                at: m.data.at.to_string(),
            }),
            _ => None,
        })
        .collect())
}

#[tauri::command]
async fn health(state: tauri::State<'_, AppState>) -> Result<Vec<HealthView>, String> {
    let mut out = Vec::new();
    for c in state.connectors() {
        let (s, detail) = match c.health().await {
            Health::Ok => ("ok", None),
            Health::Degraded(d) => ("degraded", Some(d)),
            Health::Down(d) => ("down", Some(d)),
        };
        out.push(HealthView {
            connector: c.id(),
            state: s.to_string(),
            detail,
        });
    }
    Ok(out)
}

/// Build and run the app.
///
/// # Panics
/// If Tauri cannot construct the window.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let vault = discover_vault().unwrap_or_default();
    tauri::Builder::default()
        .manage(AppState { vault })
        .invoke_handler(tauri::generate_handler![
            boot_info, core, tasks, chasers, vitals, health
        ])
        .run(tauri::generate_context!())
        .expect("error while running MOKaji");
}
