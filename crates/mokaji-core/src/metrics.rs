//! Derived metrics — **C-8**. *The Reactor Core's numbers.*
//!
//! These are **computed, never stored** (X-14: the SQLite index is a cache, connector sources are
//! the truth). The vault's `[[Jarvis Dashboard]]` reproduces the same formulas in Dataview so the
//! numbers exist before the app does — and M-1's exit criterion is that the two agree *exactly*.
//! If they ever disagree, one of them is a bug worth chasing rather than a discrepancy worth
//! documenting.
//!
//! Two of these formulas were corrected during requirements review, and both corrections showed
//! up in real data before the code existed:
//!
//! - **X-10 — `urgent` is a typed predicate on `due`, not a regex over task text.** The prototype
//!   matched `/today|now|overdue|tonight|HH:MM/i` against the task's words, which makes "read the
//!   article about burnout tonight" urgent and "file taxes 📅 today" not.
//! - **X-11 — `done` means done *today*, not all-time.** Four tasks completed in June were still
//!   reporting 25% momentum in August. Momentum that never decays is not momentum.

use crate::model::AnyRecord;
use chrono::{DateTime, Datelike, Local, Utc};

/// The Reactor Core readout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Readiness {
    /// Open (not done) tasks.
    pub open: usize,
    /// Tasks completed **today**, local calendar day (X-11).
    pub done_today: usize,
    /// Open tasks with a `due` at or before end of today, local (X-10).
    pub urgent: usize,
    /// Chasers flagged overdue.
    pub overdue: usize,
    /// Calendar events in the window. Zero until the calendar connector lands at M-5 — documented,
    /// not a bug.
    pub events: usize,
    /// `round(done_today / (open + done_today) * 100)`, or 100 when there is nothing to do.
    pub momentum: u8,
    /// `clamp(100 - urgent*16 - overdue*10, 8, 100)`.
    pub focus: u8,
    /// `clamp(round(events/8*100), 0, 100)`.
    pub cal_load: u8,
    /// `clamp(100 - open*7 - events*5, 6, 100)`.
    pub bandwidth: u8,
    /// `round(focus*0.4 + momentum*0.3 + (100-cal_load)*0.3)`.
    pub readiness: u8,
}

/// The word the footer shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// `readiness >= 70`
    Optimal,
    /// `readiness >= 45`
    Steady,
    /// below 45
    Strained,
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Optimal => "OPTIMAL",
            Self::Steady => "STEADY",
            Self::Strained => "STRAINED",
        })
    }
}

impl Readiness {
    /// The state word for this readout.
    #[must_use]
    pub fn state(&self) -> State {
        match self.readiness {
            70..=100 => State::Optimal,
            45..=69 => State::Steady,
            _ => State::Strained,
        }
    }

    /// Compute the readout from routed records, as of `now`.
    ///
    /// `now` is passed in rather than read from the clock so the formulas are testable and so
    /// "today" is unambiguous at a local midnight boundary (§5: the local calendar day is what
    /// rolls over, not UTC).
    #[must_use]
    pub fn compute(records: &[AnyRecord], now: DateTime<Local>) -> Self {
        let end_of_today = end_of_local_day(now);
        let today = now.date_naive();

        let mut open = 0usize;
        let mut done_today = 0usize;
        let mut urgent = 0usize;

        for r in records {
            if let AnyRecord::Task(t) = r {
                if t.data.done {
                    // X-11: only today's completions. A task completed with no recorded date
                    // cannot be proven to be today's, so it does not count — silently crediting
                    // it would be the all-time bug wearing a disguise.
                    if t.data.done_at == Some(today) {
                        done_today += 1;
                    }
                } else {
                    open += 1;
                    // X-10: typed predicate. No due date means not urgent, however the words read.
                    if t.data.due.is_some_and(|d| d <= end_of_today) {
                        urgent += 1;
                    }
                }
            }
        }

        // Both kinds count: a nudge you owe someone is as overdue as something owed to you.
        let overdue = records
            .iter()
            .filter(|r| matches!(r, AnyRecord::Chaser(c) if c.data.overdue))
            .count();

        let events = records
            .iter()
            .filter(|r| matches!(r, AnyRecord::Event(_)))
            .count();

        let total = open + done_today;
        let momentum = if total == 0 {
            100
        } else {
            round_pct(done_today as f64 / total as f64 * 100.0)
        };
        let focus = clamp_u8(
            100i64 - (urgent as i64) * 16 - (overdue as i64) * 10,
            8,
            100,
        );
        let cal_load = clamp_u8(round_i(events as f64 / 8.0 * 100.0), 0, 100);
        let bandwidth = clamp_u8(100i64 - (open as i64) * 7 - (events as i64) * 5, 6, 100);
        let readiness = round_pct(
            f64::from(focus) * 0.4 + f64::from(momentum) * 0.3 + f64::from(100 - cal_load) * 0.3,
        );

        Self {
            open,
            done_today,
            urgent,
            overdue,
            events,
            momentum,
            focus,
            cal_load,
            bandwidth,
            readiness,
        }
    }
}

/// The last instant of `now`'s **local** calendar day, as UTC (§5).
#[must_use]
pub fn end_of_local_day(now: DateTime<Local>) -> DateTime<Utc> {
    use chrono::TimeZone;
    let d = now.date_naive();
    let naive = d
        .succ_opt()
        .unwrap_or(d)
        .and_hms_opt(0, 0, 0)
        .expect("midnight is a valid time");
    // A local midnight can be skipped by a DST jump; the earliest valid instant is the honest
    // answer, and `latest` covers the ambiguous-repeat direction.
    Local
        .from_local_datetime(&naive)
        .earliest()
        .or_else(|| Local.from_local_datetime(&naive).latest())
        .map_or_else(|| now.with_timezone(&Utc), |dt| dt.with_timezone(&Utc))
        - chrono::Duration::nanoseconds(1)
}

/// Whether `d` falls on `now`'s local calendar day.
#[must_use]
pub fn is_local_today(d: DateTime<Utc>, now: DateTime<Local>) -> bool {
    let local = d.with_timezone(&Local);
    local.year() == now.year() && local.ordinal() == now.ordinal()
}

fn round_pct(v: f64) -> u8 {
    clamp_u8(round_i(v), 0, 100)
}

fn round_i(v: f64) -> i64 {
    v.round() as i64
}

fn clamp_u8(v: i64, lo: i64, hi: i64) -> u8 {
    v.clamp(lo, hi) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Area, Chaser, ChaserKind, PersonRef, Record, Task};
    use chrono::{NaiveDate, TimeZone};

    fn now() -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 8, 23, 10, 0, 0)
            .single()
            .expect("a real instant")
    }

    fn envelope<T>(data: T) -> Record<T> {
        Record {
            schema_version: crate::version::RECORD_SCHEMA_VERSION,
            id: "vault:x".into(),
            source: "vault".into(),
            source_ref: "x.md#L1".into(),
            area: Area::Personal,
            fetched_at: DateTime::UNIX_EPOCH,
            data,
            raw: None,
            extra: serde_json::Map::new(),
        }
    }

    fn task(text: &str, done: bool, due: Option<&str>, done_at: Option<&str>) -> AnyRecord {
        AnyRecord::Task(envelope(Task {
            text: text.into(),
            done,
            done_at: done_at.map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap()),
            due: due.map(|d| DateTime::parse_from_rfc3339(d).unwrap().with_timezone(&Utc)),
            quad: None,
            project: None,
            tags: vec![],
        }))
    }

    fn chaser(overdue: bool) -> AnyRecord {
        AnyRecord::Chaser(envelope(Chaser {
            kind: ChaserKind::Waiting,
            who: PersonRef {
                name: None,
                email: None,
            },
            what: "something".into(),
            since: NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
            last: None,
            overdue,
        }))
    }

    #[test]
    fn nothing_to_do_is_full_momentum_not_zero() {
        let r = Readiness::compute(&[], now());
        assert_eq!(
            r.momentum, 100,
            "an empty queue is not a failure to clear it"
        );
        assert_eq!(r.focus, 100);
        assert_eq!(r.readiness, 100);
        assert_eq!(r.state(), State::Optimal);
    }

    #[test]
    fn x11_only_todays_completions_count_toward_momentum() {
        let records = vec![
            task("serviced the winch", true, None, Some("2026-06-20")),
            task("trimmed the wick", true, None, Some("2026-08-23")),
            task("order lamp oil", false, None, None),
        ];
        let r = Readiness::compute(&records, now());
        assert_eq!(r.done_today, 1, "the June completion does not count");
        assert_eq!(r.open, 1);
        assert_eq!(
            r.momentum, 50,
            "1 of (1 open + 1 done today). Counting all-time would give 67% forever"
        );
    }

    #[test]
    fn x11_a_completion_with_no_date_is_not_credited_to_today() {
        let records = vec![
            task("done, no date", true, None, None),
            task("open", false, None, None),
        ];
        let r = Readiness::compute(&records, now());
        assert_eq!(
            r.done_today, 0,
            "we cannot prove it was today, and guessing would be the all-time bug in disguise"
        );
    }

    #[test]
    fn x10_urgent_is_a_typed_predicate_not_a_word_match() {
        let records = vec![
            // Reads urgent, carries no due date. The old regex would have counted this.
            task(
                "read the manual about storm procedure tonight",
                false,
                None,
                None,
            ),
            // Reads calm, due today. The old regex would have missed this.
            task(
                "file the tide report",
                false,
                Some("2026-08-23T17:00:00Z"),
                None,
            ),
            task(
                "collect the supply drop",
                false,
                Some("2026-08-24T09:00:00Z"),
                None,
            ),
        ];
        let r = Readiness::compute(&records, now());
        assert_eq!(
            r.urgent, 1,
            "exactly the one with a due date at or before end of today"
        );
        assert_eq!(r.focus, 100 - 16);
    }

    #[test]
    fn overdue_chasers_cost_ten_points_each() {
        let records = vec![chaser(true), chaser(true), chaser(false)];
        let r = Readiness::compute(&records, now());
        assert_eq!(r.overdue, 2);
        assert_eq!(r.focus, 80);
    }

    #[test]
    fn focus_has_a_floor_so_a_bad_day_is_never_zero() {
        let records: Vec<_> = (0..20)
            .map(|i| task(&format!("t{i}"), false, Some("2026-08-23T09:00:00Z"), None))
            .collect();
        let r = Readiness::compute(&records, now());
        assert_eq!(r.focus, 8, "clamped, not negative");
        assert_eq!(r.bandwidth, 6, "also clamped");
        assert_eq!(r.state(), State::Strained);
    }

    /// A regression lock on the arithmetic, using invented tasks. Only the *shape* is realistic —
    /// a handful of open items and some completions dated in the past. No one's actual notes
    /// appear in this repo, which is a hard rule (see `SECURITY.md`).
    #[test]
    fn matches_the_real_vault_readout() {
        let mut records: Vec<AnyRecord> = (0..11)
            .map(|i| task(&format!("upkeep item {i}"), false, None, None))
            .collect();
        // Four completions dated in the past: the case X-11 exists for.
        for i in 0..4 {
            records.push(task(
                &format!("done in june {i}"),
                true,
                None,
                Some("2026-06-20"),
            ));
        }
        let r = Readiness::compute(&records, now());
        assert_eq!(r.open, 11);
        assert_eq!(r.done_today, 0);
        assert_eq!(r.momentum, 0);
        assert_eq!(r.urgent, 0, "nothing here carries a due date");
        assert_eq!(r.overdue, 0, "no overdue chasers in this fixture");
        assert_eq!(r.focus, 100);
        assert_eq!(r.bandwidth, 23);
        assert_eq!(r.cal_load, 0, "no calendar connector until M-5");
        assert_eq!(r.readiness, 70);
        assert_eq!(r.state(), State::Optimal);
    }
}
