//! `mokaji` — a terminal readout of the vault.
//!
//! Not a milestone deliverable. It exists because M-1's exit criterion is *exact* numeric
//! agreement with the vault's `[[Jarvis Dashboard]]`, and finding out whether that holds should
//! not require a working Deck first. A wrong number is easier to see in a terminal than behind a
//! half-built UI, and this stays useful afterwards as the debugging path.
//!
//! Read-only. It opens no socket, and writes nothing.

use mokaji_connector_vault::VaultConnector;
use mokaji_core::connector::{Connector, Health, StandardQuery};
use mokaji_core::metrics::Readiness;
use mokaji_core::model::{AnyRecord, Kind};
use mokaji_core::router::Router;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const USAGE: &str = "\
mokaji — a terminal readout of your vault (read-only)

USAGE:
    mokaji [COMMAND] [--vault <PATH>]

COMMANDS:
    core       Reactor Core readout (default)
    tasks      open tasks, in the Deck's order
    chasers    waiting-on and need-to-nudge
    vitals     today's tracker metrics
    health     connector health

The vault is found from --vault, then $MOKAJI_VAULT_PATH, then by looking upward from the
current directory for a folder containing `08 Journal/Daily`.
";

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return std::process::ExitCode::SUCCESS;
    }

    let mut command = "core".to_string();
    let mut vault_arg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--vault" => match it.next() {
                Some(v) => vault_arg = Some(PathBuf::from(v)),
                None => {
                    eprintln!("--vault needs a path");
                    return std::process::ExitCode::FAILURE;
                }
            },
            other if !other.starts_with('-') => command = other.to_string(),
            other => {
                eprintln!("unknown option `{other}`\n\n{USAGE}");
                return std::process::ExitCode::FAILURE;
            }
        }
    }

    let Some(vault) = vault_arg.or_else(discover_vault) else {
        eprintln!(
            "Could not find a vault.\n\n\
             Pass one with `mokaji --vault <PATH>`, or set MOKAJI_VAULT_PATH.\n\
             A vault is a folder containing `08 Journal/Daily`."
        );
        return std::process::ExitCode::FAILURE;
    };

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(run(&command, &vault))
}

/// H-3: an empty config must still boot against a discovered vault. Walk up from the current
/// directory, checking each ancestor and its children for the marker folder.
fn discover_vault() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MOKAJI_VAULT_PATH") {
        let p = PathBuf::from(p);
        if is_vault(&p) {
            return Some(p);
        }
    }
    let cwd = std::env::current_dir().ok()?;
    for dir in cwd.ancestors() {
        if is_vault(dir) {
            return Some(dir.to_path_buf());
        }
        let mut children: Vec<PathBuf> = std::fs::read_dir(dir)
            .ok()?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        children.sort();
        if let Some(found) = children.into_iter().find(|c| is_vault(c)) {
            return Some(found);
        }
    }
    None
}

fn is_vault(p: &Path) -> bool {
    p.join("08 Journal/Daily").is_dir()
}

async fn run(command: &str, vault: &Path) -> std::process::ExitCode {
    let connector = Arc::new(VaultConnector::new(vault));
    let connectors: Vec<Arc<dyn Connector>> = vec![connector.clone()];
    let router = Router::new();

    match command {
        "health" => {
            println!("vault  {}", vault.display());
            match connector.health().await {
                Health::Ok => println!("state  OK"),
                Health::Degraded(why) => println!("state  DEGRADED — {why}"),
                Health::Down(why) => {
                    println!("state  DOWN — {why}");
                    return std::process::ExitCode::FAILURE;
                }
            }
            for kind in [Kind::Task, Kind::Chaser, Kind::Metric] {
                let out = router.resolve(&connectors, &q(kind)).await;
                println!("{:<8} {} record(s)", format!("{kind:?}"), out.records.len());
            }
        }
        "core" => {
            let tasks = router.resolve(&connectors, &q(Kind::Task)).await;
            let chasers = router.resolve(&connectors, &q(Kind::Chaser)).await;
            let mut all = tasks.records.clone();
            all.extend(chasers.records.clone());
            let r = Readiness::compute(&all, chrono::Local::now());

            println!("\n  ⚛  REACTOR CORE — {}%  {}\n", r.readiness, r.state());
            row("Focus clarity", &format!("{}%", r.focus), r.focus);
            row(
                "Momentum",
                &format!(
                    "{}%  ({}/{} cleared today)",
                    r.momentum,
                    r.done_today,
                    r.open + r.done_today
                ),
                r.momentum,
            );
            row("Bandwidth", &format!("{}%", r.bandwidth), r.bandwidth);
            println!();
            plain("Open tasks", &r.open.to_string());
            plain("Urgent (due ≤ today)", &r.urgent.to_string());
            plain("Chasers overdue", &r.overdue.to_string());
            plain(
                "Calendar load",
                &format!("{}%  (no calendar until M-5)", r.cal_load),
            );

            for f in tasks.failures.iter().chain(chasers.failures.iter()) {
                eprintln!("\n  ⚠  {} degraded: {}", f.connector, f.reason);
            }
            println!("\n  vault  {}\n", vault.display());
        }
        "tasks" => {
            let out = router.resolve(&connectors, &q(Kind::Task)).await;
            let mut n = 0;
            for r in &out.records {
                let AnyRecord::Task(t) = r else { continue };
                if t.data.done {
                    continue;
                }
                n += 1;
                let due = t.data.due.map_or_else(
                    || "         ".to_string(),
                    |d| {
                        d.with_timezone(&chrono::Local)
                            .format("%Y-%m-%d")
                            .to_string()
                    },
                );
                println!("  {due}  {}", t.data.text);
                println!("             ↳ {}", t.source_ref);
            }
            println!("\n  {n} open\n");
        }
        "chasers" => {
            let out = router.resolve(&connectors, &q(Kind::Chaser)).await;
            for r in &out.records {
                let AnyRecord::Chaser(c) = r else { continue };
                let flag = if c.data.overdue { "OVERDUE" } else { "       " };
                println!(
                    "  {flag}  {:<8} {}  (since {})",
                    format!("{:?}", c.data.kind).to_lowercase(),
                    c.data.what,
                    c.data.since
                );
            }
            println!("\n  {} chaser(s)\n", out.records.len());
        }
        "vitals" => {
            let out = router.resolve(&connectors, &q(Kind::Metric)).await;
            let today = chrono::Local::now().date_naive();
            let mut any = false;
            for r in &out.records {
                let AnyRecord::Metric(m) = r else { continue };
                if m.data.at != today {
                    continue;
                }
                any = true;
                plain(&m.data.key, &format!("{:?}", m.data.value));
            }
            if !any {
                println!(
                    "\n  no tracker entry for {today} — today's daily note has no metrics yet\n"
                );
            }
        }
        other => {
            eprintln!("unknown command `{other}`\n\n{USAGE}");
            return std::process::ExitCode::FAILURE;
        }
    }
    std::process::ExitCode::SUCCESS
}

fn q(kind: Kind) -> StandardQuery {
    StandardQuery {
        kind,
        window: None,
        params: serde_json::Map::new(),
    }
}

fn row(label: &str, value: &str, pct: u8) {
    let filled = (usize::from(pct) * 20) / 100;
    let bar: String = "█".repeat(filled) + &"░".repeat(20 - filled);
    println!("  {label:<22} {bar}  {value}");
}

fn plain(label: &str, value: &str) {
    println!("  {label:<22} {value}");
}
