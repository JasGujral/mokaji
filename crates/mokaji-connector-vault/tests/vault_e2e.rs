//! End-to-end: a synthetic vault shaped like the real one, routed and reduced to a readout.
//!
//! The fixture below is **entirely invented** — a lighthouse station that does not exist, chosen
//! precisely because nobody could mistake it for anyone's real notes. It reproduces only the
//! *shape* a real vault has (a count of open tasks, completions dated in the past, chaser examples
//! fenced, a daily note with tracker frontmatter), never anyone's content. That is a hard rule for
//! this repo — see `SECURITY.md`.
//!
//! Locking the expected readout here means a formula change has to be deliberate.

use mokaji_connector_vault::VaultConnector;
use mokaji_core::connector::{Connector, StandardQuery};
use mokaji_core::metrics::{Readiness, State};
use mokaji_core::model::{AnyRecord, Kind, MetricValue};
use mokaji_core::router::Router;
use std::path::PathBuf;
use std::sync::Arc;

/// A throwaway vault on disk. Named from the process id and a counter so parallel tests never
/// collide — deliberately not random, since a reproducible path is easier to inspect when a test
/// fails.
fn scratch_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mokaji-vault-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let daily = dir.join("08 Journal/Daily");
    let projects = dir.join("01 Projects");
    let cc = dir.join("09 Command Center");
    for d in [&daily, &projects, &cc] {
        std::fs::create_dir_all(d).expect("mkdir");
    }

    // A daily note: 3 filled priorities, one blank template placeholder, tracker frontmatter.
    std::fs::write(
        daily.join("2026-06-20.md"),
        "---\ntype: daily\ncreated: 2026-06-20\ntags: [daily]\nmood: 4\nenergy: 4\nfocus: 4\ndeep_work_hours: 3\nsleep_hours: 7\nexercised: true\n---\n# Saturday, 20 June 2026\n\n## Top 3 priorities\n- [ ] Check the fog signal\n- [ ] Log the tide readings\n- [ ] Sweep the lamp room\n\n## Tasks\n- [ ] \n",
    )
    .expect("write daily");

    // A project with four June completions and four open tasks.
    std::fs::write(
        projects.join("Station upkeep.md"),
        "---\ntype: project\nstatus: active\n---\n# Station upkeep\n\n## Tasks\n- [x] Paint the south railing ✅ 2026-06-20\n- [x] Replace the gasket ✅ 2026-06-20\n- [x] Service the winch ✅ 2026-06-20\n- [x] Restock the paraffin ✅ 2026-06-20\n- [ ] Order lamp oil\n- [ ] Trim the wick\n- [ ] Grease the rotation bearing\n- [ ] Test the backup generator\n",
    )
    .expect("write project");

    // A second project with four open tasks.
    std::fs::write(
        projects.join("Tide survey.md"),
        "---\ntype: project\nstatus: active\n---\n# Tide survey\n\n## Tasks\n- [ ] Calibrate the tide gauge\n- [ ] Record the spring tides\n- [ ] Chart the neap range\n- [ ] File the quarterly return\n",
    )
    .expect("write project 2");

    // Chasers: the documentation examples are fenced, so they are examples and not data.
    std::fs::write(
        cc.join("Chasers.md"),
        "---\ntype: moc\n---\n# Chasers\n\n## Waiting on\n```tasks\nnot done\ntags include #waiting\n```\n\n## Add a chaser\n\n```markdown\n- [ ] #waiting Harbour office to confirm the tide tables — since 2026-06-18\n- [ ] #waiting #overdue Chandlery invoice — since 2026-06-10\n```\n",
    )
    .expect("write chasers");

    dir
}

fn query(kind: Kind) -> StandardQuery {
    StandardQuery {
        kind,
        window: None,
        params: serde_json::Map::new(),
    }
}

fn now() -> chrono::DateTime<chrono::Local> {
    use chrono::TimeZone;
    chrono::Local
        .with_ymd_and_hms(2026, 8, 23, 10, 0, 0)
        .single()
        .expect("a real instant")
}

#[tokio::test]
async fn reproduces_the_dashboard_readout() {
    let dir = scratch_vault("readout");
    let connectors: Vec<Arc<dyn Connector>> = vec![Arc::new(VaultConnector::new(&dir))];
    let router = Router::new();

    let mut records = router.resolve(&connectors, &query(Kind::Task)).await;
    assert!(
        records.is_complete(),
        "vault read cleanly: {:?}",
        records.failures
    );
    let chasers = router.resolve(&connectors, &query(Kind::Chaser)).await;
    records.records.extend(chasers.records);

    let r = Readiness::compute(&records.records, now());

    assert_eq!(
        r.open, 11,
        "12 checkboxes minus the blank template placeholder"
    );
    assert_eq!(r.done_today, 0, "the four completions are dated in June");
    assert_eq!(r.momentum, 0);
    assert_eq!(r.urgent, 0, "no task carries a 📅 due date yet");
    assert_eq!(
        r.overdue, 0,
        "the #overdue example is inside a code fence — documentation, not data"
    );
    assert_eq!(r.focus, 100);
    assert_eq!(r.bandwidth, 23);
    assert_eq!(r.cal_load, 0, "no calendar connector until M-5");
    assert_eq!(r.readiness, 70);
    assert_eq!(r.state(), State::Optimal);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_due_date_moves_the_needle_and_a_word_does_not() {
    let dir = scratch_vault("due");
    std::fs::write(
        dir.join("01 Projects/Urgent.md"),
        "# Urgent\n- [ ] read the manual about storm procedure tonight\n- [ ] file the tide report 📅 2026-08-23\n",
    )
    .expect("write");

    let connectors: Vec<Arc<dyn Connector>> = vec![Arc::new(VaultConnector::new(&dir))];
    let out = Router::new().resolve(&connectors, &query(Kind::Task)).await;
    let r = Readiness::compute(&out.records, now());

    assert_eq!(
        r.urgent, 1,
        "X-10: the dated one counts, the one that merely sounds urgent does not"
    );
    assert_eq!(r.focus, 84);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn tracker_metrics_come_out_of_daily_note_frontmatter() {
    let dir = scratch_vault("metrics");
    let connectors: Vec<Arc<dyn Connector>> = vec![Arc::new(VaultConnector::new(&dir))];
    let out = Router::new()
        .resolve(&connectors, &query(Kind::Metric))
        .await;
    assert!(out.is_complete());

    let mut found: Vec<(String, String)> = out
        .records
        .iter()
        .filter_map(|r| match r {
            AnyRecord::Metric(m) => Some((
                m.data.key.clone(),
                match &m.data.value {
                    MetricValue::Number(n) => n.to_string(),
                    MetricValue::Bool(b) => b.to_string(),
                    MetricValue::Text(t) => t.clone(),
                },
            )),
            _ => None,
        })
        .collect();
    found.sort();

    assert_eq!(
        found,
        vec![
            ("deep_work_hours".to_string(), "3".to_string()),
            ("energy".to_string(), "4".to_string()),
            ("exercised".to_string(), "true".to_string()),
            ("focus".to_string(), "4".to_string()),
            ("mood".to_string(), "4".to_string()),
            ("sleep_hours".to_string(), "7".to_string()),
        ],
        "sleep_hours and exercised are in the vault template but absent from the requirements' \
         Metric list — the vault is ground truth"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn every_record_points_home() {
    let dir = scratch_vault("refs");
    let connectors: Vec<Arc<dyn Connector>> = vec![Arc::new(VaultConnector::new(&dir))];
    let out = Router::new().resolve(&connectors, &query(Kind::Task)).await;

    for r in &out.records {
        let AnyRecord::Task(t) = r else { continue };
        assert!(
            t.source_ref.contains(".md#L"),
            "a record you cannot open is a record you cannot trust: {}",
            t.source_ref
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_missing_vault_is_reported_not_fatal() {
    let missing = std::env::temp_dir().join("mokaji-vault-does-not-exist");
    let connectors: Vec<Arc<dyn Connector>> = vec![Arc::new(VaultConnector::new(&missing))];
    let out = Router::new().resolve(&connectors, &query(Kind::Task)).await;

    // A-6: the Deck degrades, it does not blank. The router returns a result either way.
    assert!(out.records.is_empty());
    assert!(
        matches!(
            connectors[0].health().await,
            mokaji_core::connector::Health::Down(_)
        ),
        "health says why, so the badge can too"
    );
}
