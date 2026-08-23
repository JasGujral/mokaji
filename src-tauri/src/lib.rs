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
    /// **B-4: dry-run.** Everything the Console previews goes through this one, so a preview
    /// cannot write by accident — it is not that we remember to be careful, it is that this writer
    /// has no ability to change a file.
    preview: std::sync::Mutex<VaultWriter>,
    /// The armed writer. Reachable only from `apply`, which a person triggers *after* seeing the
    /// diff — CON-4's "states what it will do" and "undoable for 30 s" are both satisfied by the
    /// two-step, so a global arm switch would add risk without adding capability.
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
            // The DRY-RUN writer. Using the armed one here would make every keystroke that parses
            // into a mutating intent write to the vault — which is exactly the bug the two-writer
            // split exists to make impossible rather than merely unlikely.
            let mut w = state
                .preview
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

/// The result of actually applying a command.
#[derive(Serialize)]
pub struct Applied {
    /// Which file changed.
    path: String,
    /// The diff that was written.
    diff: String,
    /// CON-4: hand this back to `undo_write` within 30 seconds.
    undo_id: String,
    /// Seconds remaining on the undo window, for the countdown.
    undo_seconds: i64,
}

/// Apply a Console command **for real**.
///
/// Deliberately a separate command from `preview`, taking the same input string rather than a
/// token from the preview: the person types, sees the diff, and then asks for it. Nothing here can
/// run without that second, explicit action.
///
/// # Errors
/// If the intent is not a mutating one, or the write is refused (B-3 drift, B-5 snapshot failure).
#[tauri::command]
fn apply(state: tauri::State<'_, AppState>, input: String) -> Result<Applied, String> {
    let now = chrono::Local::now();
    let intent = parse_intent(&input, now.date_naive());
    let edit = match &intent {
        Intent::AddTask { text, due } => Edit::AddTask {
            text: text.clone(),
            due: *due,
        },
        Intent::Capture { text } => Edit::Capture { text: text.clone() },
        other => {
            return Err(format!(
                "`{}` changes nothing — nothing to apply",
                other.describe()
            ))
        }
    };

    let mut w = state
        .writer
        .lock()
        .map_err(|_| "writer lock poisoned".to_string())?;
    let receipt = w.apply(&edit, now).map_err(|e| e.to_string())?;
    let undo_id = receipt
        .undo_id
        .ok_or_else(|| "write reported no undo token".to_string())?;
    Ok(Applied {
        path: receipt.path,
        diff: receipt.diff,
        undo_seconds: w.undo_remaining(now).unwrap_or(0),
        undo_id,
    })
}

/// Undo a write inside the 30-second window (CON-4).
///
/// # Errors
/// If the window has closed or the id is unknown — and it says which, because "too late" and
/// "never happened" call for different reactions.
#[tauri::command]
fn undo_write(state: tauri::State<'_, AppState>, undo_id: String) -> Result<String, String> {
    let mut w = state
        .writer
        .lock()
        .map_err(|_| "writer lock poisoned".to_string())?;
    w.undo(&undo_id, chrono::Local::now())
        .map_err(|e| e.to_string())
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
        milestone: "M-2".to_string(),
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

/// **B-6 — watch the vault and tell the Deck when it changes.**
///
/// A HUD that is up to sixty seconds stale is a HUD you check twice, and checking twice is how a
/// glanceable thing becomes an app you open. Edits made in Obsidian should appear here in about a
/// second.
///
/// Debounced, because a single save in an editor produces a burst of filesystem events and
/// re-reading the vault once per event would spend more time reading than the poll it replaces.
fn watch_vault(app: tauri::AppHandle, root: PathBuf) {
    use notify::{RecursiveMode, Watcher};
    use tauri::Emitter;

    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let Ok(mut watcher) = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        }) else {
            return;
        };
        if watcher.watch(&root, RecursiveMode::Recursive).is_err() {
            return;
        }

        loop {
            // Block for the first event, then swallow everything that arrives in the next 400 ms.
            let Ok(first) = rx.recv() else { return };
            if first.is_err() {
                continue;
            }
            while rx
                .recv_timeout(std::time::Duration::from_millis(400))
                .is_ok()
            {}
            let _ = app.emit("vault-changed", ());
        }
    });
}

/// Build and run the app.
///
/// # Panics
/// If Tauri cannot construct the window.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let vault = discover::discover();
    let calendar = discover::remembered_calendar();
    let watched = vault.clone();
    // B-5's snapshot destination sits OUTSIDE the vault, so a snapshot never becomes a note.
    let snapshots = vault
        .as_ref()
        .and_then(|v| v.parent().map(|p| p.join(".mokaji-snapshots")))
        .unwrap_or_else(std::env::temp_dir);
    let snapshotter = || {
        Box::new(CopySnapshot {
            dest_root: snapshots.clone(),
        })
    };
    // Named `preview_writer` rather than `preview`: a local binding of that name shadows the
    // command function of the same name, and the resulting error is a long way from the cause.
    let preview_writer = std::sync::Mutex::new(VaultWriter::new(
        vault.clone().unwrap_or_default(),
        snapshotter(),
    ));
    // `.armed()` is the only place in the codebase that turns off B-4's default, and it is one
    // grep away from being found.
    let writer = std::sync::Mutex::new(
        VaultWriter::new(vault.clone().unwrap_or_default(), snapshotter()).armed(),
    );
    tauri::Builder::default()
        .manage(AppState {
            vault: std::sync::Mutex::new(vault),
            calendar: std::sync::Mutex::new(calendar),
            preview: preview_writer,
            writer,
        })
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(move |app| {
            if let Some(v) = watched.clone() {
                watch_vault(app.handle().clone(), v);
            }
            register_summon(app.handle());
            Ok(())
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
            apply,
            undo_write,
            secret_status,
            set_secret,
            clear_secret,
            act,
            window_hide,
            window_show,
            open_note,
            suggest_calendars
        ])
        .run(tauri::generate_context!())
        .expect("error while running MOKaji");
}

// ---------------------------------------------------------------------------------------------
// Voice / window control — CON-3's "typed or spoken behaves identically", made literal.
// ---------------------------------------------------------------------------------------------

/// What the caller should *do* about an utterance.
///
/// The parser lives in `core` (CON-3) and returns an [`Intent`]; this is the same thing rendered
/// for a renderer that must not import Rust types. The tag is exhaustive on purpose — a new intent
/// that the UI forgets to handle shows up as an unknown tag in one `switch`, not as silence.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    /// A vault write. The renderer must show the diff and wait for a person — never auto-apply.
    Write {
        /// CON-4's sentence, said *before* anything happens.
        describe: String,
    },
    /// Bring a Deck panel forward, or put it away.
    Panel {
        /// Panel name, articles and the word "panel" already stripped.
        name: String,
        /// True to show.
        on: bool,
    },
    /// Open a note in Obsidian.
    Open {
        /// The search phrase, as spoken.
        query: String,
    },
    /// Hide or restore the HUD itself.
    Window {
        /// True to show.
        on: bool,
    },
    /// A read-only view command (`status`, `help`, `clear`).
    Ui {
        /// Which one.
        name: String,
    },
    /// CON-2: nothing local matched. Escalation is the caller's decision.
    Unmatched {
        /// What was said.
        text: String,
    },
}

/// Turn one utterance — typed or transcribed — into an [`Action`].
///
/// This is the single entry point for the voice loop. It deliberately does **not** act: a
/// mis-transcription must be recoverable, and the only reliable way to guarantee that is for the
/// thing that hears you and the thing that changes your vault to be two separate steps.
#[tauri::command]
fn act(input: String) -> Action {
    match parse_intent(&input, chrono::Local::now().date_naive()) {
        i @ (Intent::AddTask { .. } | Intent::Capture { .. } | Intent::CompleteTask { .. }) => {
            Action::Write {
                describe: i.describe(),
            }
        }
        Intent::TogglePanel { name, on } => Action::Panel { name, on },
        Intent::Open { query } => Action::Open { query },
        Intent::HideWindow => Action::Window { on: false },
        Intent::ShowWindow => Action::Window { on: true },
        Intent::Status => Action::Ui {
            name: "status".into(),
        },
        Intent::Help => Action::Ui {
            name: "help".into(),
        },
        Intent::Clear => Action::Ui {
            name: "clear".into(),
        },
        Intent::Unmatched(text) => Action::Unmatched { text },
    }
}

/// Put the HUD away.
///
/// # Errors
/// If the window has gone.
#[tauri::command]
fn window_hide(window: tauri::Window) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}

/// Bring the HUD back and give it focus — "come back" should not also require a click.
///
/// # Errors
/// If the window has gone.
#[tauri::command]
fn window_show(window: tauri::Window) -> Result<(), String> {
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}

/// Open a note in Obsidian by title.
///
/// X-6 superseded `obsidian://` as the *read* path and kept it as UX: MOKaji reads the vault
/// directly, but opening the real editor is one of the few things it should not try to do itself.
///
/// The vault name is taken from the folder name, which is what Obsidian uses.
///
/// # Errors
/// If no vault is configured, nothing matches, or the URL handler refuses.
#[tauri::command]
fn open_note(state: tauri::State<'_, AppState>, query: String) -> Result<String, String> {
    let root = state.vault().ok_or("no vault configured")?;
    let vault_name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or("vault path has no name")?;
    let hit = find_note(&root, &query).ok_or_else(|| format!("no note matching \"{query}\""))?;
    let url = format!(
        "obsidian://open?vault={}&file={}",
        urlencode(&vault_name),
        urlencode(&hit)
    );
    open_url(&url)?;
    Ok(hit)
}

/// Find the best note title match under `root`.
///
/// Prefix beats substring beats nothing, and shorter beats longer — "tide" should open *Tide
/// Survey* rather than *Tide Survey Archive 2019*, because the shorter title is the one a person
/// means when they say the short thing.
fn find_note(root: &std::path::Path, query: &str) -> Option<String> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }
    let mut best: Option<(u8, usize, String)> = None;
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")) {
                continue;
            }
            let stem = name.trim_end_matches(".md").to_string();
            let lower = stem.to_lowercase();
            let rank = if lower == needle {
                0
            } else if lower.starts_with(&needle) {
                1
            } else if lower.contains(&needle) {
                2
            } else {
                continue;
            };
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .to_string();
            let cand = (rank, stem.len(), rel);
            if best.as_ref().is_none_or(|b| cand < *b) {
                best = Some(cand);
            }
        }
    }
    best.map(|(_, _, rel)| rel)
}

/// Percent-encode everything that is not unreserved. Small and local rather than a dependency:
/// PRIV-5 makes every added crate a thing to justify, and this is twelve lines.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Hand a URL to the OS. Not a network call — the handler is another local application.
fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("/usr/bin/open");
    #[cfg(not(target_os = "macos"))]
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(url)
        .status()
        .map_err(|e| e.to_string())
        .and_then(|s| {
            if s.success() {
                Ok(())
            } else {
                Err("the URL handler refused".into())
            }
        })
}

/// The summon hotkey — **⌥Space**.
///
/// V-1 says the wake word is the primary path, but a wake word that has not been trained yet is
/// not a path at all, and an always-on HUD you have to go and click is just an app. The hotkey is
/// the floor: it works with the microphone off, in a meeting, and on the first run.
///
/// ⌥Space rather than ⌘Space (Spotlight) or ⌃Space (input sources). Failure to register is
/// logged, not fatal — another application holding the combination is a reason to fall back to the
/// window, not a reason to refuse to start.
fn register_summon(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    use tauri_plugin_global_shortcut::{
        Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
    };

    let summon = Shortcut::new(Some(Modifiers::ALT), Code::Space);
    let handle = app.clone();
    if let Err(e) = app
        .global_shortcut()
        .on_shortcut(summon, move |_, _, event| {
            // Fire on press only; a shortcut handler that also fires on release toggles twice.
            if event.state != ShortcutState::Pressed {
                return;
            }
            if let Some(w) = handle.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
            let _ = handle.emit("summon", ());
        })
    {
        eprintln!("mokaji: could not register the summon hotkey (Alt+Space): {e}");
    }
}

/// Calendar folders worth offering, newest-useful first.
///
/// **The zero-credential path to Google Calendar.** macOS Calendar.app writes every event of every
/// subscribed account as its own `.ics` under `~/Library/Calendars`, so adding a Google account in
/// System Settings → Internet Accounts makes both work and personal calendars readable here with
/// no OAuth client, no app verification, and nothing leaving the machine.
#[tauri::command]
fn suggest_calendars() -> Vec<String> {
    let mut out = Vec::new();
    let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_default();
    for rel in ["Library/Calendars", "Calendars", "Documents/Calendars"] {
        let p = home.join(rel);
        if p.is_dir() {
            out.push(p.display().to_string());
        }
    }
    out
}

#[cfg(test)]
mod voice_tests {
    use super::{find_note, urlencode};

    fn fixture() -> tempdir_lite::Dir {
        let d = tempdir_lite::Dir::new("mokaji-open");
        // Invented station notes. PRIV: fixtures never echo a real vault — they supply shape.
        d.write("08 Journal/Daily/2026-08-23.md", "# log");
        d.write("02 Projects/Tide Survey.md", "# tide");
        d.write("02 Projects/Tide Survey Archive 1998.md", "# old");
        d.write("03 Areas/Fog Signal Upkeep.md", "# fog");
        d.write(".obsidian/workspace.json", "{}");
        d
    }

    #[test]
    fn the_shorter_title_wins_because_that_is_what_a_person_means() {
        let d = fixture();
        // "tide" must open *Tide Survey*, not *Tide Survey Archive 1998* — when someone says the
        // short thing they mean the short thing.
        assert_eq!(
            find_note(d.path(), "tide").as_deref(),
            Some("02 Projects/Tide Survey.md")
        );
    }

    #[test]
    fn substring_matches_but_a_miss_is_a_miss_rather_than_a_guess() {
        let d = fixture();
        assert_eq!(
            find_note(d.path(), "fog signal").as_deref(),
            Some("03 Areas/Fog Signal Upkeep.md")
        );
        // Opening the wrong note is worse than opening none: the failure is silent and the user
        // has already looked away.
        assert_eq!(find_note(d.path(), "harbour dues"), None);
        assert_eq!(find_note(d.path(), "   "), None);
    }

    #[test]
    fn dotfolders_are_not_notes() {
        let d = fixture();
        assert_eq!(find_note(d.path(), "workspace"), None);
    }

    #[test]
    fn urlencode_escapes_what_obsidian_would_otherwise_read_as_syntax() {
        assert_eq!(urlencode("Tide Survey"), "Tide%20Survey");
        assert_eq!(urlencode("02 Projects/Tide.md"), "02%20Projects/Tide.md");
        assert_eq!(urlencode("Q&A"), "Q%26A");
        assert_eq!(urlencode("a-b_c.d~e"), "a-b_c.d~e");
    }

    /// A ten-line temporary directory rather than a dev-dependency. PRIV-5 makes every added crate
    /// something to justify, and this is not worth a supply chain.
    mod tempdir_lite {
        use std::path::{Path, PathBuf};

        pub struct Dir(PathBuf);

        impl Dir {
            pub fn new(tag: &str) -> Self {
                let n = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos());
                let p = std::env::temp_dir().join(format!("{tag}-{n}"));
                std::fs::create_dir_all(&p).expect("temp dir");
                Self(p)
            }

            pub fn path(&self) -> &Path {
                &self.0
            }

            pub fn write(&self, rel: &str, body: &str) {
                let p = self.0.join(rel);
                std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
                std::fs::write(p, body).expect("write");
            }
        }

        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}
