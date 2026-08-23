//! **M-0 exit criteria, as an executable checklist.**
//!
//! mokata Article 3 — *spec before code; acceptance criteria each map to a test; RED before
//! GREEN* — is why this file was written before the implementation existed. M-0 is done when it
//! runs green and no `#[ignore]` remains.
//!
//! Requirement ids refer to the project's authoritative requirements document; `CLAUDE.md`
//! carries the operative summary.

use mokaji_connectors_fake::{FailAt, FakeConnector};
use mokaji_core::connector::{Capability, Connector, StandardQuery};
use mokaji_core::model::{AnyRecord, Kind};
use mokaji_core::registry::ConnectorRegistry;
use mokaji_core::router::{normalize, ContentKey, Router, SourcePrecedence};
use std::sync::Arc;

fn query(kind: Kind) -> StandardQuery {
    StandardQuery {
        kind,
        window: Some("today".into()),
        params: serde_json::Map::new(),
    }
}

fn titles(records: &[AnyRecord]) -> Vec<String> {
    records
        .iter()
        .map(|r| match r {
            AnyRecord::Event(e) => e.data.title.clone(),
            AnyRecord::Task(t) => t.data.text.clone(),
            other => format!("{:?}", other.kind()),
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// A-2 — TET
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn fake_connector_round_trips_tasks_through_tet() {
    let c = FakeConnector::vault("vault");
    let q = query(Kind::Task);

    let pq = c.transform_query(&q).expect("transform_query");
    let raw = c.extract(pq).await.expect("extract");
    let records = c.transform_data(raw).expect("transform_data");

    assert_eq!(records.len(), 4, "every fixture task survives TET");
    let AnyRecord::Task(first) = &records[0] else {
        panic!("expected a Task")
    };
    assert_eq!(first.data.text, "Calibrate the lamp rotation");
    assert_eq!(first.source, "vault");
    assert_eq!(
        first.source_ref, "notes/daily/2026-08-24.md#L12",
        "the envelope points home (§5)"
    );
    assert!(first.data.due.is_some(), "X-10: due is a parsed DateTime");
    assert!(
        records
            .iter()
            .any(|r| matches!(r, AnyRecord::Task(t) if t.data.due.is_none())),
        "one fixture task has no due date, so A-5 has something to order"
    );
}

#[tokio::test]
async fn fake_connector_round_trips_events_through_tet() {
    // Two dialects, one standard model. `summary` and `SUMMARY` both become `title` (§5).
    for c in [FakeConnector::gcal("gcal"), FakeConnector::ics("ics")] {
        let q = query(Kind::Event);
        let pq = c.transform_query(&q).expect("transform_query");
        let raw = c.extract(pq).await.expect("extract");
        let records = c.transform_data(raw).expect("transform_data");

        assert_eq!(records.len(), 2, "{} emitted both fixture events", c.id());
        let AnyRecord::Event(e) = &records[0] else {
            panic!("expected an Event")
        };
        assert!(
            !e.data.title.is_empty(),
            "title is populated from either dialect"
        );
        assert!(e.data.end > e.data.start);
    }
}

#[tokio::test]
async fn tet_errors_name_the_stage_they_came_from() {
    use mokaji_core::error::Stage;

    let cases = [
        (FailAt::TransformQuery, Stage::TransformQuery),
        (FailAt::Extract, Stage::Extract),
        (FailAt::TransformData, Stage::TransformData),
    ];

    for (inject, expected) in cases {
        let c = FakeConnector::gcal("gcal").failing_at(inject);
        let q = query(Kind::Event);

        let err = match c.transform_query(&q) {
            Err(e) => e,
            Ok(pq) => match c.extract(pq).await {
                Err(e) => e,
                Ok(raw) => c.transform_data(raw).expect_err("should have failed"),
            },
        };

        match err {
            mokaji_core::Error::Stage {
                stage, connector, ..
            } => {
                assert_eq!(stage, expected, "A-2: the error names its stage");
                assert_eq!(connector, "gcal", "and names the connector");
            }
            other => panic!("expected a staged error, got {other:?}"),
        }
        // The stage must also survive Display, since that is what reaches a health badge.
        let c = FakeConnector::gcal("gcal").failing_at(inject);
        let rendered = match c.transform_query(&q) {
            Err(e) => e.to_string(),
            Ok(pq) => match c.extract(pq).await {
                Err(e) => e.to_string(),
                Ok(raw) => c.transform_data(raw).unwrap_err().to_string(),
            },
        };
        assert!(
            rendered.contains(&format!("{expected:?}")),
            "stage should be visible in `{rendered}`"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// A-4 — dedupe on content identity, not (source, source_ref)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn router_dedupes_the_same_meeting_from_two_sources() {
    let connectors: Vec<Arc<dyn Connector>> = vec![
        Arc::new(FakeConnector::gcal("gcal")),
        Arc::new(FakeConnector::ics("ics")),
    ];

    let out = Router::new()
        .with_precedence(SourcePrecedence::new(["gcal", "ics"]))
        .resolve(&connectors, &query(Kind::Event))
        .await;

    assert!(out.is_complete(), "no failures expected");
    assert_eq!(
        out.records.len(),
        3,
        "4 raw events collapse to 3: the handover arrives twice. Got {:?}",
        titles(&out.records)
    );

    let standups: Vec<_> = out
        .records
        .iter()
        .filter(|r| matches!(r, AnyRecord::Event(e) if normalize(&e.data.title) == "watchhandover"))
        .collect();
    assert_eq!(standups.len(), 1, "exactly one handover survives");

    // The point of A-4, stated as an assertion: the two records had different source_refs, so
    // deduping by (source, source_ref) would have kept both.
    let AnyRecord::Event(kept) = standups[0] else {
        panic!()
    };
    assert_ne!(kept.source_ref, "", "the survivor still points home");
}

#[tokio::test]
async fn source_precedence_decides_which_duplicate_survives() {
    let connectors: Vec<Arc<dyn Connector>> = vec![
        Arc::new(FakeConnector::gcal("gcal")),
        Arc::new(FakeConnector::ics("ics")),
    ];

    for (order, expected_source, expected_title) in [
        (["gcal", "ics"], "gcal", "Watch handover"),
        (["ics", "gcal"], "ics", "  Watch-Handover!  "),
    ] {
        let out = Router::new()
            .with_precedence(SourcePrecedence::new(order))
            .resolve(&connectors, &query(Kind::Event))
            .await;

        let standup = out
            .records
            .iter()
            .find(
                |r| matches!(r, AnyRecord::Event(e) if normalize(&e.data.title) == "watchhandover"),
            )
            .expect("a handover survives");

        assert_eq!(standup.source(), expected_source, "order {order:?}");
        let AnyRecord::Event(e) = standup else {
            panic!()
        };
        assert_eq!(
            e.data.title, expected_title,
            "the winning source's spelling is the one kept"
        );
    }
}

#[test]
fn content_key_ignores_source_and_normalizes_text() {
    // Same task, two spellings → one key. This is the invariant the router leans on.
    let a = ContentKey::Task(normalize("Calibrate the lamp rotation"), None);
    let b = ContentKey::Task(normalize("  calibrate   the LAMP rotation!! "), None);
    assert_eq!(a, b, "spelling differences must not create a second task");

    // …and the key genuinely discriminates: a different due date is a different task.
    let with_due = ContentKey::Task(
        normalize("Calibrate the lamp rotation"),
        Some(chrono::Utc::now()),
    );
    assert_ne!(a, with_due);
}

// ---------------------------------------------------------------------------------------------
// A-5 — deterministic sort
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn events_sort_deterministically() {
    let connectors: Vec<Arc<dyn Connector>> = vec![
        Arc::new(FakeConnector::ics("ics")),
        Arc::new(FakeConnector::gcal("gcal")),
    ];
    let router = Router::new().with_precedence(SourcePrecedence::new(["gcal", "ics"]));

    let first = router.resolve(&connectors, &query(Kind::Event)).await;
    assert_eq!(
        titles(&first.records),
        vec!["Watch handover", "Lens inspection", "Supply boat"],
        "start ascending, then title"
    );

    // Registration order must not change the answer.
    let reversed: Vec<Arc<dyn Connector>> = vec![
        Arc::new(FakeConnector::gcal("gcal")),
        Arc::new(FakeConnector::ics("ics")),
    ];
    let second = router.resolve(&reversed, &query(Kind::Event)).await;
    assert_eq!(
        titles(&first.records),
        titles(&second.records),
        "identical queries return identical order regardless of registration order"
    );
}

#[tokio::test]
async fn tasks_sort_deterministically_with_nulls_last() {
    let connectors: Vec<Arc<dyn Connector>> = vec![Arc::new(FakeConnector::vault("vault"))];
    let out = Router::new().resolve(&connectors, &query(Kind::Task)).await;

    let order = titles(&out.records);
    assert_eq!(
        order,
        vec![
            "Order lamp oil",              // due 08-23
            "Calibrate the lamp rotation", // due 08-24 (the duplicate collapsed into it)
            "Log the tide readings",       // no due date — LAST, not first
        ],
        "due ascending, nulls last, then text"
    );

    let AnyRecord::Task(last) = out.records.last().expect("non-empty") else {
        panic!()
    };
    assert!(
        last.data.due.is_none(),
        "a task with no due date sorts last — Option's own ordering would put it first, which is \
         exactly the bug A-5 exists to prevent"
    );
}

// ---------------------------------------------------------------------------------------------
// A-6 — partial failure
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn one_dead_connector_degrades_only_itself() {
    let connectors: Vec<Arc<dyn Connector>> = vec![
        Arc::new(FakeConnector::gcal("gcal").failing_at(FailAt::Extract)),
        Arc::new(FakeConnector::ics("ics")),
    ];

    let out = Router::new()
        .with_precedence(SourcePrecedence::new(["gcal", "ics"]))
        .resolve(&connectors, &query(Kind::Event))
        .await;

    assert!(!out.is_complete(), "the failure is reported, not swallowed");
    assert_eq!(out.failures.len(), 1);
    assert_eq!(out.failures[0].connector, "gcal");
    assert!(
        out.failures[0].reason.contains("Extract"),
        "the badge can say which stage broke: {}",
        out.failures[0].reason
    );

    // The Deck does not blank: the healthy connector's records still arrive.
    assert_eq!(
        titles(&out.records),
        vec!["  Watch-Handover!  ", "Supply boat"]
    );
}

#[tokio::test]
async fn registry_reports_health_and_capabilities() {
    let mut reg = ConnectorRegistry::new();
    reg.register(Arc::new(FakeConnector::vault("vault")))
        .register(Arc::new(
            FakeConnector::gcal("gcal").failing_at(FailAt::Extract),
        ));

    assert_eq!(reg.readers_of(Kind::Task).len(), 1, "A-3: capability index");
    assert_eq!(reg.readers_of(Kind::Event).len(), 1);
    assert_eq!(reg.readers_of(Kind::Message).len(), 0);
    assert!(reg.capabilities().contains(&Capability::Read(Kind::Task)));

    let health = reg.health().await;
    assert!(matches!(
        health["vault"],
        mokaji_core::connector::Health::Ok
    ));
    assert!(
        matches!(health["gcal"], mokaji_core::connector::Health::Down(_)),
        "an unhealthy connector is reported, not removed (A-6)"
    );
}

// ---------------------------------------------------------------------------------------------
// A-12 — schema versioning
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn unknown_fields_are_preserved_on_round_trip() {
    let c = FakeConnector::gcal("gcal").with_unknown_envelope_keys();
    let q = query(Kind::Event);
    let pq = c.transform_query(&q).unwrap();
    let raw = c.extract(pq).await.unwrap();
    let records = c.transform_data(raw).unwrap();

    let AnyRecord::Event(rec) = &records[0] else {
        panic!()
    };
    assert!(
        rec.extra.contains_key("confidence"),
        "an envelope key this build has never heard of is captured"
    );

    let json = serde_json::to_string(rec).expect("serialize");
    assert!(
        json.contains("\"confidence\":0.93"),
        "…and written back out unchanged rather than silently dropped: {json}"
    );

    let back: mokaji_core::model::Record<mokaji_core::model::Event> =
        serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.extra, rec.extra, "stable across a full round-trip");
    assert_eq!(back.id, rec.id);
}

#[test]
fn schema_version_mismatch_fails_loudly() {
    use mokaji_core::version;

    version::check(1, 1, "panels.json").expect("matching versions are fine");

    let err = version::check(2, 1, "panels.json").expect_err("a mismatch must not pass silently");
    match err {
        mokaji_core::Error::SchemaVersion {
            found,
            supported,
            hint,
        } => {
            assert_eq!((found, supported), (2, 1));
            assert!(
                hint.contains("migration"),
                "the error tells you what to do, not just that you are wrong: {hint}"
            );
        }
        other => panic!("expected SchemaVersion, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// PRIV-5 — the network boundary
// ---------------------------------------------------------------------------------------------

/// Count open socket file descriptors for this process. `None` when the platform gives us no
/// cheap way to look — the caller then falls back to the structural assertion, so this can never
/// pass by accident.
fn open_sockets() -> Option<usize> {
    #[cfg(target_os = "linux")]
    {
        let mut n = 0;
        for entry in std::fs::read_dir("/proc/self/fd").ok()? {
            let Ok(entry) = entry else { continue };
            if let Ok(target) = std::fs::read_link(entry.path()) {
                if target.to_string_lossy().starts_with("socket:") {
                    n += 1;
                }
            }
        }
        Some(n)
    }
    // macOS is the target platform and has no /proc — ask lsof for this process's internet
    // sockets instead. One header line, hence the subtraction.
    #[cfg(not(target_os = "linux"))]
    {
        let out = std::process::Command::new("lsof")
            .args(["-p", &std::process::id().to_string(), "-a", "-i"])
            .output()
            .ok()?;
        Some(
            out.stdout
                .iter()
                .filter(|b| **b == b'\n')
                .count()
                .saturating_sub(1),
        )
    }
}

#[tokio::test]
async fn no_other_socket_is_opened_process_wide() {
    // 1. Structural: only mokaji-net may name a networking crate. This is the assertion that
    //    makes PRIV-1 ("audio never leaves the device") true by construction — the audio crate
    //    cannot acquire the *ability* to transmit without failing this.
    const NET_CRATES: [&str; 9] = [
        "reqwest",
        "hyper",
        "ureq",
        "curl",
        "tokio-tungstenite",
        "socket2",
        "tiny_http",
        "axum",
        "warp",
    ];
    let crates_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/");
    let mut checked = 0;
    for entry in std::fs::read_dir(crates_dir).expect("read crates/") {
        let dir = entry.expect("entry").path();
        if dir.file_name().and_then(|n| n.to_str()) == Some("mokaji-net") {
            continue;
        }
        let manifest = dir.join("Cargo.toml");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        checked += 1;
        for line in text.lines() {
            let name = line.split(['=', ' ']).next().unwrap_or("").trim();
            assert!(
                !NET_CRATES.contains(&name),
                "{}: networking dependency `{name}` outside mokaji-net (PRIV-5)",
                manifest.display()
            );
        }
    }
    assert!(checked >= 2, "the boundary check actually inspected crates");

    // 2. Behavioural: a full router pass over the fakes opens no socket at all.
    let before = open_sockets();
    let connectors: Vec<Arc<dyn Connector>> = vec![
        Arc::new(FakeConnector::gcal("gcal")),
        Arc::new(FakeConnector::ics("ics")),
        Arc::new(FakeConnector::vault("vault")),
    ];
    let router = Router::new().with_precedence(SourcePrecedence::new(["gcal", "ics", "vault"]));
    let events = router.resolve(&connectors, &query(Kind::Event)).await;
    let tasks = router.resolve(&connectors, &query(Kind::Task)).await;
    assert!(!events.records.is_empty() && !tasks.records.is_empty());
    let after = open_sockets();

    if let (Some(b), Some(a)) = (before, after) {
        assert!(
            a <= b,
            "a full router pass opened {} socket(s); the fakes are in-memory and must open none",
            a - b
        );
    }
}
