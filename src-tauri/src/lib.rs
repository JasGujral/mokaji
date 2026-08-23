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

use mokaji_connector_ics::IcsConnector;
use mokaji_connector_vault::write::{CopySnapshot, Edit, VaultWriter};
use mokaji_connector_vault::VaultConnector;
use mokaji_core::connector::{Connector, Health, StandardQuery};
use mokaji_core::intent::{parse as parse_intent, Intent, GRAMMAR};
use mokaji_core::metrics::Readiness;
use mokaji_core::model::{AnyRecord, Kind, MetricValue};
use mokaji_core::router::Router;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;

/// Where the vault is, resolved once at startup.
///
/// `None` is a real state, not an error to paper over. A GUI app launched from Finder gets
/// neither your shell environment nor a useful working directory, so on macOS *both* discovery
/// routes fail by default — and a HUD that responds to "I have no data" by reporting 100% OPTIMAL
/// is worse than one that fails loudly.
pub struct AppState {
    vault: std::sync::Mutex<Option<PathBuf>>,
    /// A folder of `.ics` files. B-7's secondary path, promoted to first: it needs no OAuth
    /// client, no app verification and no browser round-trip, so `calLoad` stops being
    /// permanently zero without waiting on anyone's API console.
    calendar: std::sync::Mutex<Option<PathBuf>>,
    /// **B-4: dry-run, and it stays that way in M-1/M-2.** The Console can show you exactly what a
    /// command would do; arming it is a separate decision that belongs with the voice loop's
    /// spoken confirmation and undo, not with a text box.
    writer: std::sync::Mutex<VaultWriter>,
}

impl AppState {
    fn vault(&self) -> Option<PathBuf> {
        self.vault.lock().ok().and_then(|v| v.clone())
    }

    fn calendar(&self) -> Option<PathBuf> {
        self.calendar.lock().ok().and_then(|v| v.clone())
    }

    fn connectors(&self) -> Vec<Arc<dyn Connector>> {
        let mut out: Vec<Arc<dyn Connector>> = Vec::new();
        if let Some(v) = self.vault() {
            out.push(Arc::new(VaultConnector::new(v)));
        }
        if let Some(c) = self.calendar() {
            out.push(Arc::new(IcsConnector::new(c)));
        }
        out
    }
}

// Vault discovery lives in the connector crate, shared with the CLI and covered by workspace
// tests. It was two implementations for one day, which was long enough for them to disagree.
use mokaji_connector_vault::discover;

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
    /// **False when no connector answered at all.**
    ///
    /// `Readiness::compute` on an empty set returns 100% OPTIMAL — correct arithmetic for "nothing
    /// to do", and a lie for "nothing was read". Those two cases must never look alike on screen,
    /// so the distinction is carried here rather than inferred from a zero.
    has_data: bool,
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

/// What a Console command would do, without doing it (B-4, CON-4).
#[derive(Serialize)]
pub struct Preview {
    /// The parsed intent's name, for the UI.
    kind: String,
    /// CON-4: what it will do, in a sentence, before it does it.
    describes: String,
    /// Whether acting on it would change the vault.
    mutating: bool,
    /// The exact diff that would be applied. Empty for read-only intents.
    diff: String,
    /// True when nothing local matched and CON-2 would escalate to the model router (M-4).
    unmatched: bool,
}

#[derive(Serialize)]
pub struct BootInfo {
    vault: Option<String>,
    calendar: Option<String>,
    version: String,
    /// Milestone the running build corresponds to, so a screenshot is never ambiguous.
    milestone: String,
}

/// Today's events, in local-calendar-day terms (§5).
fn today_events() -> StandardQuery {
    StandardQuery {
        kind: Kind::Event,
        window: Some("today".into()),
        params: serde_json::Map::new(),
    }
}

/// An event, flattened for the renderer.
#[derive(Serialize)]
pub struct EventView {
    id: String,
    title: String,
    start: String,
    end: String,
    all_day: bool,
    location: Option<String>,
    /// Starting within the next 90 minutes — the handoff's "soon" flag.
    soon: bool,
    source: String,
    source_ref: String,
}

/// Today's agenda.
#[tauri::command]
async fn agenda(state: tauri::State<'_, AppState>) -> Result<Vec<EventView>, String> {
    let out = Router::new()
        .resolve(&state.connectors(), &today_events())
        .await;
    let now = chrono::Utc::now();
    Ok(out
        .records
        .iter()
        .filter_map(|r| match r {
            AnyRecord::Event(e) => Some(EventView {
                id: e.id.clone(),
                title: e.data.title.clone(),
                start: e.data.start.to_rfc3339(),
                end: e.data.end.to_rfc3339(),
                all_day: e.data.all_day,
                location: e.data.location.clone(),
                soon: e.data.start > now && e.data.start - now < chrono::Duration::minutes(90),
                source: e.source.clone(),
                source_ref: e.source_ref.clone(),
            }),
            _ => None,
        })
        .collect())
}

/// Point at a folder of `.ics` files.
///
/// # Errors
/// If the path is not a folder, or the choice cannot be persisted.
#[tauri::command]
fn set_calendar(state: tauri::State<'_, AppState>, path: String) -> Result<String, String> {
    let p = discover::expand_home(path.trim());
    discover::remember_calendar(&p)?;
    *state
        .calendar
        .lock()
        .map_err(|_| "calendar lock poisoned")? = Some(p.clone());
    Ok(p.display().to_string())
}

fn q(kind: Kind) -> StandardQuery {
    StandardQuery {
        kind,
        window: None,
        params: serde_json::Map::new(),
    }
}

/// Parse a Console line and show what it would do — **without doing it**.
///
/// CON-3: this is the same parser the voice loop will use, so a command cannot behave differently
/// typed and spoken. CON-1: it runs before any model is consulted. B-4: the writer behind it is in
/// dry-run, so this is a preview by construction rather than by carefulness.
#[tauri::command]
fn preview(state: tauri::State<'_, AppState>, input: String) -> Result<Preview, String> {
    let now = chrono::Local::now();
    let intent = parse_intent(&input, now.date_naive());

    let edit = match &intent {
        Intent::AddTask { text, due } => Some(Edit::AddTask {
            text: text.clone(),
            due: *due,
        }),
        Intent::Capture { text } => Some(Edit::Capture { text: text.clone() }),
        // CompleteTask needs a resolved target line, which means matching against the queue first.
        // That lands with the armed write path; previewing a guess would be worse than saying so.
        _ => None,
    };

    let diff = match edit {
        Some(e) => {
            let mut w = state
                .writer
                .lock()
                .map_err(|_| "writer lock poisoned".to_string())?;
            match w.apply(&e, now) {
                Ok(r) => r.diff,
                Err(err) => format!("cannot preview: {err}"),
            }
        }
        None => String::new(),
    };

    Ok(Preview {
        kind: format!("{intent:?}")
            .split_whitespace()
            .next()
            .unwrap_or("Unknown")
            .to_string(),
        describes: intent.describe(),
        mutating: intent.is_mutating(),
        diff,
        unmatched: matches!(intent, Intent::Unmatched(_)),
    })
}

/// The local grammar, for `help` (CON-1).
#[tauri::command]
fn grammar() -> Vec<(String, String)> {
    GRAMMAR
        .iter()
        .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
        .collect()
}

#[tauri::command]
fn boot_info(state: tauri::State<'_, AppState>) -> BootInfo {
    BootInfo {
        vault: state.vault().map(|v| v.display().to_string()),
        calendar: state.calendar().map(|v| v.display().to_string()),
        version: env!("CARGO_PKG_VERSION").to_string(),
        milestone: "M-1".to_string(),
    }
}

#[tauri::command]
async fn core(state: tauri::State<'_, AppState>) -> Result<CoreView, String> {
    let connectors = state.connectors();
    let configured = !connectors.is_empty();
    let router = Router::new();
    let tasks = router.resolve(&connectors, &q(Kind::Task)).await;
    let chasers = router.resolve(&connectors, &q(Kind::Chaser)).await;
    let events = router.resolve(&connectors, &today_events()).await;

    let mut all = tasks.records.clone();
    all.extend(chasers.records.clone());
    all.extend(events.records.clone());
    let r = Readiness::compute(&all, chrono::Local::now());

    let mut failures: Vec<FailureView> = tasks
        .failures
        .iter()
        .chain(chasers.failures.iter())
        .chain(events.failures.iter())
        .map(|f| FailureView {
            connector: f.connector.clone(),
            reason: f.reason.clone(),
        })
        .collect();
    if !configured {
        failures.push(FailureView {
            connector: "vault".into(),
            reason: "no vault configured".into(),
        });
    }

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
        has_data: configured && failures.is_empty(),
        failures,
    })
}

/// Which credentials are present — **booleans only**.
///
/// PRIV-4: the renderer never receives a token or key. It can learn that one is set, so the
/// settings panel can offer to replace it, and that is the entire surface.
#[tauri::command]
fn secret_status() -> std::collections::BTreeMap<String, bool> {
    use mokaji_secrets::{account, service};
    let mut out = std::collections::BTreeMap::new();
    let store = platform_store();
    out.insert(
        "anthropic".to_string(),
        store.get(service::ANTHROPIC, account::API_KEY).is_ok(),
    );
    out
}

/// Store a credential in the Keychain.
///
/// # Errors
/// If the name is unknown or the platform store refuses.
#[tauri::command]
fn set_secret(name: String, value: String) -> Result<(), String> {
    use mokaji_secrets::{account, service, Secret};
    if value.trim().is_empty() {
        return Err("empty value".into());
    }
    let (svc, acct) = match name.as_str() {
        "anthropic" => (service::ANTHROPIC, account::API_KEY),
        other => return Err(format!("unknown credential `{other}`")),
    };
    platform_store()
        .set(svc, acct, &Secret::new(value))
        .map_err(|e| e.to_string())
}

/// Remove a credential. Idempotent — revocation is usually done in a hurry.
///
/// # Errors
/// If the name is unknown.
#[tauri::command]
fn clear_secret(name: String) -> Result<(), String> {
    use mokaji_secrets::{account, service};
    let (svc, acct) = match name.as_str() {
        "anthropic" => (service::ANTHROPIC, account::API_KEY),
        other => return Err(format!("unknown credential `{other}`")),
    };
    platform_store()
        .delete(svc, acct)
        .map_err(|e| e.to_string())
}

/// The Keychain on macOS; an in-memory store elsewhere, so a Linux dev build runs without
/// pretending it has somewhere safe to put a key.
fn platform_store() -> Box<dyn mokaji_secrets::SecretStore> {
    #[cfg(target_os = "macos")]
    {
        Box::new(mokaji_secrets::KeychainStore)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(mokaji_secrets::MemoryStore::default())
    }
}

/// Remember a vault path, so a double-clicked app finds it next time.
///
/// # Errors
/// If the path is not a vault, or the choice cannot be persisted.
#[tauri::command]
fn set_vault(state: tauri::State<'_, AppState>, path: String) -> Result<String, String> {
    let p = discover::expand_home(path.trim());
    discover::remember(&p)?;
    *state.vault.lock().map_err(|_| "vault lock poisoned")? = Some(p.clone());
    Ok(p.display().to_string())
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
    if state.vault().is_none() {
        out.push(HealthView {
            connector: "vault".into(),
            state: "down".into(),
            detail: Some("no vault configured".into()),
        });
    }
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
    let vault = discover::discover();
    let calendar = discover::remembered_calendar();
    // B-5's snapshot destination sits OUTSIDE the vault, so a snapshot never becomes a note.
    let snapshots = vault
        .as_ref()
        .and_then(|v| v.parent().map(|p| p.join(".mokaji-snapshots")))
        .unwrap_or_else(std::env::temp_dir);
    let writer = std::sync::Mutex::new(VaultWriter::new(
        vault.clone().unwrap_or_default(),
        Box::new(CopySnapshot {
            dest_root: snapshots,
        }),
    ));
    tauri::Builder::default()
        .manage(AppState {
            vault: std::sync::Mutex::new(vault),
            calendar: std::sync::Mutex::new(calendar),
            writer,
        })
        .invoke_handler(tauri::generate_handler![
            boot_info,
            core,
            tasks,
            chasers,
            vitals,
            health,
            preview,
            grammar,
            set_vault,
            set_calendar,
            agenda,
            secret_status,
            set_secret,
            clear_secret
        ])
        .run(tauri::generate_context!())
        .expect("error while running MOKaji");
}
