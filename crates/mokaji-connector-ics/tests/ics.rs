//! The iCalendar connector.
//!
//! Fixture is invented — a lighthouse station that does not exist. Nothing from anyone's real
//! calendar enters this repository (see `SECURITY.md`).

use chrono::{DateTime, TimeZone, Utc};
use mokaji_connector_ics::parse::{self, RawEvent};
use mokaji_connector_ics::{window, IcsConnector};
use mokaji_core::connector::{Connector, StandardQuery};
use mokaji_core::model::{AnyRecord, Kind};
use mokaji_core::router::Router;
use std::path::PathBuf;
use std::sync::Arc;

const FIXTURE: &str = include_str!("../fixtures/station.ics");

fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn find(uid: &str) -> RawEvent {
    parse::events(FIXTURE)
        .into_iter()
        .find(|e| e.uid == uid)
        .expect("uid present")
}

// ---------------------------------------------------------------- the file format's sharp edges

#[test]
fn folded_lines_are_rejoined() {
    // Miss this and a long summary arrives truncated at exactly 75 octets, which looks like a data
    // problem and gets debugged in the wrong place.
    let e = find("folded-002");
    assert_eq!(
        e.summary,
        "Quarterly inspection of the lens assembly and the rotation bearing, including the spare"
    );
}

#[test]
fn escaped_characters_are_unescaped() {
    let e = find("folded-002");
    assert_eq!(e.location.as_deref(), Some("Lamp room, upper gallery"));
}

#[test]
fn duration_stands_in_for_a_missing_end() {
    let e = find("folded-002");
    assert_eq!(e.end - e.start, chrono::Duration::minutes(90));
}

#[test]
fn an_all_day_event_spans_a_day_rather_than_an_instant() {
    let e = find("allday-003");
    assert!(e.all_day);
    assert_eq!(e.end - e.start, chrono::Duration::days(1));
}

#[test]
fn an_event_with_no_end_and_no_duration_is_instantaneous() {
    let e = find("untitled-006");
    assert_eq!(e.start, e.end);
}

#[test]
fn a_missing_summary_gets_a_readable_placeholder() {
    // Legal, and it appears in real exports. "(no title)" is more useful on a Deck than a blank row.
    assert_eq!(find("untitled-006").summary, "(no title)");
}

#[test]
fn attendees_lose_their_mailto_prefix() {
    assert_eq!(find("handover-001").attendees, vec!["keeper@example.com"]);
}

#[test]
fn durations_parse_in_both_halves() {
    assert_eq!(
        parse::parse_duration("PT1H30M"),
        Some(chrono::Duration::minutes(90))
    );
    assert_eq!(
        parse::parse_duration("P1D"),
        Some(chrono::Duration::days(1))
    );
    assert_eq!(
        parse::parse_duration("P1DT2H"),
        Some(chrono::Duration::hours(26))
    );
    assert_eq!(parse::parse_duration("nonsense"), None);
}

// ------------------------------------------------------------------------------- recurrence

#[test]
fn a_daily_rule_expands_within_the_window_and_honours_count() {
    let e = find("daily-004");
    let all = parse::expand(&e, utc("2026-08-24T00:00:00Z"), utc("2026-09-01T00:00:00Z"));
    assert_eq!(all.len(), 5, "COUNT=5 means five, not five per query");

    let just_one_day = parse::expand(&e, utc("2026-08-26T00:00:00Z"), utc("2026-08-27T00:00:00Z"));
    assert_eq!(just_one_day.len(), 1, "and the window still narrows it");
    assert_eq!(just_one_day[0].start, utc("2026-08-26T06:00:00Z"));
}

#[test]
fn a_weekly_rule_with_byday_lands_on_the_named_days() {
    // 2026-08-24 is a Monday. MO,TH with COUNT=4 → Mon 24, Thu 27, Mon 31, Thu 3 Sep.
    let e = find("weekly-005");
    let all = parse::expand(&e, utc("2026-08-24T00:00:00Z"), utc("2026-09-30T00:00:00Z"));
    let days: Vec<String> = all
        .iter()
        .map(|o| o.start.format("%Y-%m-%d").to_string())
        .collect();
    assert_eq!(
        days,
        vec!["2026-08-24", "2026-08-27", "2026-08-31", "2026-09-03"]
    );
}

#[test]
fn an_unexpanded_frequency_returns_the_base_occurrence_rather_than_nothing() {
    // MONTHLY and YEARLY are not expanded yet. A connector that silently drops your birthday is
    // worse than one that admits a limit — so the base event still appears.
    let base = RawEvent {
        uid: "m".into(),
        summary: "Monthly drill".into(),
        start: utc("2026-08-24T09:00:00Z"),
        end: utc("2026-08-24T10:00:00Z"),
        all_day: false,
        location: None,
        attendees: vec![],
        rrule: Some("FREQ=MONTHLY;COUNT=6".into()),
    };
    let got = parse::expand(
        &base,
        utc("2026-08-24T00:00:00Z"),
        utc("2026-08-25T00:00:00Z"),
    );
    assert_eq!(got.len(), 1);
}

#[test]
fn a_rule_with_no_count_and_no_until_cannot_spin_forever() {
    let base = RawEvent {
        uid: "e".into(),
        summary: "Endless".into(),
        start: utc("2026-01-01T09:00:00Z"),
        end: utc("2026-01-01T10:00:00Z"),
        all_day: false,
        location: None,
        attendees: vec![],
        rrule: Some("FREQ=DAILY".into()),
    };
    let got = parse::expand(
        &base,
        utc("2026-01-01T00:00:00Z"),
        utc("2027-01-01T00:00:00Z"),
    );
    assert_eq!(got.len(), 365, "bounded by the window, and capped besides");
}

// ------------------------------------------------------------------------------- the connector

fn scratch() -> PathBuf {
    let d = std::env::temp_dir().join(format!("mokaji-ics-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("station.ics"), FIXTURE).unwrap();
    d
}

#[tokio::test]
async fn the_connector_round_trips_events_through_tet() {
    let dir = scratch();
    let connectors: Vec<Arc<dyn Connector>> = vec![Arc::new(IcsConnector::new(&dir))];
    let q = StandardQuery {
        kind: Kind::Event,
        window: Some("month".into()),
        params: serde_json::Map::new(),
    };
    let out = Router::new().resolve(&connectors, &q).await;
    assert!(out.is_complete(), "{:?}", out.failures);
    assert!(!out.records.is_empty());

    // A-5: events come back start-ascending.
    let starts: Vec<i64> = out
        .records
        .iter()
        .filter_map(|r| match r {
            AnyRecord::Event(e) => Some(e.data.start.timestamp()),
            _ => None,
        })
        .collect();
    let mut sorted = starts.clone();
    sorted.sort_unstable();
    assert_eq!(starts, sorted, "deterministic order");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn every_occurrence_is_its_own_record_and_points_home() {
    let dir = scratch();
    let connectors: Vec<Arc<dyn Connector>> = vec![Arc::new(IcsConnector::new(&dir))];
    let q = StandardQuery {
        kind: Kind::Event,
        window: Some("month".into()),
        params: serde_json::Map::new(),
    };
    let out = Router::new().resolve(&connectors, &q).await;

    let ids: Vec<&str> = out
        .records
        .iter()
        .filter_map(|r| match r {
            AnyRecord::Event(e) => Some(e.id.as_str()),
            _ => None,
        })
        .collect();
    let unique: std::collections::BTreeSet<&&str> = ids.iter().collect();
    assert_eq!(
        ids.len(),
        unique.len(),
        "two occurrences of a recurring event must be two records, not one overwriting the other"
    );
    for r in &out.records {
        let AnyRecord::Event(e) = r else { continue };
        assert!(
            e.source_ref.contains(".ics#"),
            "an event you cannot trace: {}",
            e.source_ref
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_missing_calendar_folder_is_reported_not_fatal() {
    let missing = std::env::temp_dir().join("mokaji-ics-nope");
    let c = IcsConnector::new(&missing);
    assert!(matches!(
        c.health().await,
        mokaji_core::connector::Health::Down(_)
    ));

    let connectors: Vec<Arc<dyn Connector>> = vec![Arc::new(IcsConnector::new(&missing))];
    let q = StandardQuery {
        kind: Kind::Event,
        window: None,
        params: serde_json::Map::new(),
    };
    let out = Router::new().resolve(&connectors, &q).await;
    assert!(
        out.records.is_empty(),
        "A-6: degrade, do not blank the Deck"
    );
}

#[test]
fn the_window_is_the_local_calendar_day() {
    let now = chrono::Local
        .with_ymd_and_hms(2026, 8, 24, 15, 30, 0)
        .single()
        .unwrap();
    let (from, to) = window(Some("today"), now);
    assert_eq!(to - from, chrono::Duration::days(1));
    let (from_w, to_w) = window(Some("week"), now);
    assert_eq!(to_w - from_w, chrono::Duration::days(7));
}
