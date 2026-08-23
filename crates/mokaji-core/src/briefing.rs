//! The morning briefing — **M-5's exit criterion, and the strongest reading of E-2**.
//!
//! E-2 pins `BriefingAssembly` local, and §12 forbids a cloud dependency in the daily loop. The
//! most robust way to satisfy that is not to route the briefing to a local model — it is for the
//! briefing to **need no model at all**. Everything below is computed from records by ordinary
//! code: the same inputs always produce the same words, there is nothing to be down, nothing to
//! warm up, nothing to hallucinate, and it works with the network cable out and no weights on
//! disk.
//!
//! A model can still improve the *phrasing* later. It can never be responsible for the *facts*,
//! because a briefing whose claims cannot be traced to a record id is indistinguishable from a
//! plausible invention (E-8) — and that is the one thing this system is arguing against.
//!
//! **Citations are computed from what was supplied, never requested from a model.** Every line
//! carries the record ids behind it, so "why did it say that?" is answerable by pointing at the
//! note, the event or the message rather than by re-running anything.
//!
//! ## Why the spoken form is separate from the written one
//!
//! Because they fail differently. Read aloud, "3 urgent" is fine and "08 Journal/Daily/2026-08-23.md#L42"
//! is unbearable; on screen the pointer is the useful part. Generating one and stripping it for
//! the other produces text that is bad in both places, so both are built from the same facts and
//! neither is derived from the other.

use crate::metrics::{Readiness, State};
use crate::model::{AnyRecord, Kind};
use crate::provider::Citation;
use chrono::{DateTime, Duration, Local, Timelike};

/// Which part of the day a line belongs to. Ordering is the reading order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    /// How the day looks overall.
    Readiness,
    /// What is on the calendar.
    Agenda,
    /// What is due.
    Tasks,
    /// Who is waiting on you, or you on them.
    Chasers,
    /// What arrived.
    Mail,
}

impl Section {
    /// The heading shown on screen.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Readiness => "Readiness",
            Self::Agenda => "Agenda",
            Self::Tasks => "Due",
            Self::Chasers => "Chasing",
            Self::Mail => "Mail",
        }
    }
}

/// One statement, and the records that make it true.
#[derive(Debug, Clone)]
pub struct Line {
    /// Which section.
    pub section: Section,
    /// The written form, for the panel.
    pub text: String,
    /// E-8: what backs this claim.
    pub citations: Vec<Citation>,
}

/// The whole briefing.
#[derive(Debug, Clone)]
pub struct Briefing {
    /// "Good morning" and the date.
    pub greeting: String,
    /// The written lines, in reading order.
    pub lines: Vec<Line>,
    /// One continuous paragraph, safe to hand to a speech synthesiser.
    pub spoken: String,
    /// Every citation across every line, deduplicated.
    pub citations: Vec<Citation>,
    /// Which connectors contributed. M-5's exit criterion is a *three-connector* briefing, so this
    /// is the thing that says whether the criterion was actually met rather than merely attempted.
    pub sources: Vec<String>,
}

impl Briefing {
    /// Whether this briefing drew on at least three distinct connectors (M-5).
    #[must_use]
    pub fn is_three_connector(&self) -> bool {
        self.sources.len() >= 3
    }
}

/// Compose the briefing from routed records.
///
/// `now` is passed in rather than read from the clock so the output is testable and so "today"
/// means the local calendar day (§5) rather than whatever UTC happened to be.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn compose(records: &[AnyRecord], now: DateTime<Local>) -> Briefing {
    let readiness = Readiness::compute(records, now);
    let end_of_today = crate::metrics::end_of_local_day(now);
    let mut lines: Vec<Line> = Vec::new();
    let mut said: Vec<String> = Vec::new();

    // ---- Readiness -----------------------------------------------------------------------
    // First because it is the sentence that decides whether the rest is worth hearing.
    let mood = match readiness.state() {
        State::Optimal => "You have room today",
        State::Steady => "Today is workable",
        State::Strained => "Today is tight",
    };
    let readiness_text = format!(
        "{mood} — readiness {}%, {} open, {} cleared today.",
        readiness.readiness, readiness.open, readiness.done_today
    );
    lines.push(Line {
        section: Section::Readiness,
        text: readiness_text.clone(),
        citations: Vec::new(),
    });
    said.push(readiness_text);

    // ---- Agenda --------------------------------------------------------------------------
    let mut events: Vec<_> = records
        .iter()
        .filter_map(|r| match r {
            AnyRecord::Event(e) if e.data.end >= now.to_utc() => Some(e),
            _ => None,
        })
        .collect();
    events.sort_by(|a, b| {
        a.data
            .start
            .cmp(&b.data.start)
            .then(a.data.title.cmp(&b.data.title))
    });

    if events.is_empty() {
        lines.push(Line {
            section: Section::Agenda,
            text: "Nothing left on the calendar today.".into(),
            citations: Vec::new(),
        });
        said.push("Nothing left on your calendar.".into());
    } else {
        let next = events[0];
        let when = next.data.start.with_timezone(&Local);
        let gap = when.signed_duration_since(now);
        let lead = if gap < Duration::zero() {
            format!("{} is running now", next.data.title)
        } else if gap < Duration::minutes(90) {
            format!("{} in {}", next.data.title, humanise(gap))
        } else {
            format!("{} at {}", next.data.title, when.format("%H:%M"))
        };
        let rest = events.len().saturating_sub(1);
        let text = if rest == 0 {
            format!("{lead}, and nothing after it.")
        } else {
            format!("{lead}, then {} more.", rest)
        };
        lines.push(Line {
            section: Section::Agenda,
            text: text.clone(),
            citations: cite(
                events
                    .iter()
                    .take(4)
                    .map(|e| (e.id.clone(), e.source.clone(), e.source_ref.clone())),
            ),
        });
        said.push(text);
    }

    // ---- Tasks ---------------------------------------------------------------------------
    // X-10: urgent is a typed predicate on `due`, never a word in the text.
    let mut urgent: Vec<_> = records
        .iter()
        .filter_map(|r| match r {
            AnyRecord::Task(t) if !t.data.done && t.data.due.is_some_and(|d| d <= end_of_today) => {
                Some(t)
            }
            _ => None,
        })
        .collect();
    urgent.sort_by(|a, b| {
        a.data
            .due
            .cmp(&b.data.due)
            .then(a.data.text.cmp(&b.data.text))
    });

    if urgent.is_empty() {
        let text = if readiness.open == 0 {
            "Nothing due, and the queue is empty.".to_string()
        } else {
            format!("Nothing due today; {} open behind it.", readiness.open)
        };
        lines.push(Line {
            section: Section::Tasks,
            text: text.clone(),
            citations: Vec::new(),
        });
        said.push(text);
    } else {
        let named: Vec<&str> = urgent
            .iter()
            .take(3)
            .map(|t| t.data.text.as_str())
            .collect();
        let text = format!(
            "{} due today: {}{}",
            urgent.len(),
            list(&named),
            if urgent.len() > named.len() {
                format!(", and {} more.", urgent.len() - named.len())
            } else {
                ".".into()
            }
        );
        lines.push(Line {
            section: Section::Tasks,
            text: text.clone(),
            citations: cite(
                urgent
                    .iter()
                    .take(6)
                    .map(|t| (t.id.clone(), t.source.clone(), t.source_ref.clone())),
            ),
        });
        said.push(text);
    }

    // ---- Chasers -------------------------------------------------------------------------
    let overdue: Vec<_> = records
        .iter()
        .filter_map(|r| match r {
            AnyRecord::Chaser(c) if c.data.overdue => Some(c),
            _ => None,
        })
        .collect();
    if !overdue.is_empty() {
        let who: Vec<String> = overdue
            .iter()
            .take(3)
            .map(|c| {
                c.data
                    .who
                    .name
                    .clone()
                    .or_else(|| c.data.who.email.clone())
                    .unwrap_or_else(|| "someone".into())
            })
            .collect();
        let refs: Vec<&str> = who.iter().map(String::as_str).collect();
        let text = format!("{} overdue: {}.", overdue.len(), list(&refs));
        lines.push(Line {
            section: Section::Chasers,
            text: text.clone(),
            citations: cite(
                overdue
                    .iter()
                    .take(6)
                    .map(|c| (c.id.clone(), c.source.clone(), c.source_ref.clone())),
            ),
        });
        said.push(text);
    }

    // ---- Mail ----------------------------------------------------------------------------
    // needs_action is computed by the mail connector from structural signals only — never from
    // the words in a subject line. See `mokaji-connector-mail::classify`.
    let mut wants: Vec<_> = records
        .iter()
        .filter_map(|r| match r {
            AnyRecord::Message(m) if m.data.needs_action => Some(m),
            _ => None,
        })
        .collect();
    // Newest first — and by id after that, so two messages that arrived in the same second do
    // not swap places between two glances (A-5).
    wants.sort_by(|a, b| b.data.received.cmp(&a.data.received).then(a.id.cmp(&b.id)));

    if !wants.is_empty() {
        let senders: Vec<String> = wants
            .iter()
            .take(3)
            .map(|m| {
                m.data
                    .from
                    .name
                    .clone()
                    .or_else(|| m.data.from.email.clone())
                    .unwrap_or_else(|| "an unknown sender".into())
            })
            .collect();
        let refs: Vec<&str> = senders.iter().map(String::as_str).collect();
        let text = format!(
            "{} unread {} you: {}.",
            wants.len(),
            if wants.len() == 1 {
                "message wants"
            } else {
                "messages want"
            },
            list(&refs)
        );
        lines.push(Line {
            section: Section::Mail,
            text: text.clone(),
            citations: cite(
                wants
                    .iter()
                    .take(6)
                    .map(|m| (m.id.clone(), m.source.clone(), m.source_ref.clone())),
            ),
        });
        said.push(text);
    }

    let mut citations: Vec<Citation> = Vec::new();
    for l in &lines {
        for c in &l.citations {
            if !citations.iter().any(|x| x.record_id == c.record_id) {
                citations.push(c.clone());
            }
        }
    }

    let mut sources: Vec<String> = Vec::new();
    for r in records {
        let s = source_of(r);
        if !sources.contains(&s) {
            sources.push(s);
        }
    }
    sources.sort();

    Briefing {
        greeting: greeting(now),
        lines,
        spoken: said.join(" "),
        citations,
        sources,
    }
}

fn source_of(r: &AnyRecord) -> String {
    match r {
        AnyRecord::Task(x) => x.source.clone(),
        AnyRecord::Event(x) => x.source.clone(),
        AnyRecord::Chaser(x) => x.source.clone(),
        AnyRecord::Note(x) => x.source.clone(),
        AnyRecord::Person(x) => x.source.clone(),
        AnyRecord::Metric(x) => x.source.clone(),
        AnyRecord::Message(x) => x.source.clone(),
        AnyRecord::Goal(x) => x.source.clone(),
    }
}

fn cite(it: impl Iterator<Item = (String, String, String)>) -> Vec<Citation> {
    it.map(|(record_id, source, source_ref)| Citation {
        record_id,
        source,
        source_ref,
    })
    .collect()
}

/// "Good morning" / "Good afternoon" / "Good evening", plus the date.
fn greeting(now: DateTime<Local>) -> String {
    let part = match now.hour() {
        0..=4 => "Still up",
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        _ => "Good evening",
    };
    format!("{part}. It is {}.", now.format("%A, %-d %B"))
}

/// "in 20 minutes" rather than "in 0h20m". Spoken text is the constraint here.
fn humanise(d: Duration) -> String {
    let mins = d.num_minutes().max(0);
    match mins {
        0 => "under a minute".into(),
        1 => "a minute".into(),
        2..=59 => format!("{mins} minutes"),
        60..=119 => "an hour".into(),
        _ => format!("{} hours", mins / 60),
    }
}

/// Join with commas and a final "and" — the difference between a briefing and a CSV read aloud.
fn list(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [a] => (*a).to_string(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// The kinds a briefing draws on, so a caller can ask for exactly these and no more.
#[must_use]
pub fn kinds() -> &'static [Kind] {
    &[Kind::Task, Kind::Event, Kind::Chaser, Kind::Message]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Chaser, ChaserKind, Event, Message, PersonRef, Record, Task};
    use crate::version::RECORD_SCHEMA_VERSION;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<chrono::Utc> {
        chrono::Utc
            .with_ymd_and_hms(2026, 8, 24, h, m, 0)
            .single()
            .expect("ts")
    }

    fn now() -> DateTime<Local> {
        at(7, 0).with_timezone(&Local)
    }

    fn rec<T>(source: &str, id: &str, data: T) -> Record<T> {
        Record {
            schema_version: RECORD_SCHEMA_VERSION,
            id: id.into(),
            source: source.into(),
            source_ref: format!("{source}#{id}"),
            area: crate::model::Area::Work,
            fetched_at: at(7, 0),
            data,
            raw: None,
            extra: serde_json::Map::new(),
        }
    }

    fn task(id: &str, text: &str, due: Option<DateTime<chrono::Utc>>) -> AnyRecord {
        AnyRecord::Task(rec(
            "vault",
            id,
            Task {
                text: text.into(),
                done: false,
                due,
                done_at: None,
                quad: None,
                project: None,
                tags: Vec::new(),
            },
        ))
    }

    fn event(id: &str, title: &str, start: DateTime<chrono::Utc>) -> AnyRecord {
        AnyRecord::Event(rec(
            "ics",
            id,
            Event {
                title: title.into(),
                start,
                end: start + Duration::minutes(30),
                all_day: false,
                location: None,
                attendees: Vec::new(),
                response: None,
            },
        ))
    }

    fn message(id: &str, from: &str, needs_action: bool) -> AnyRecord {
        AnyRecord::Message(rec(
            "mail-work",
            id,
            Message {
                from: PersonRef {
                    name: Some(from.into()),
                    email: Some("ops@example.org".into()),
                },
                subject: "Fog signal inspection window".into(),
                snippet: String::new(),
                received: at(6, 40),
                needs_action,
                thread_ref: id.into(),
            },
        ))
    }

    #[test]
    fn a_three_connector_briefing_is_what_m5_actually_asks_for() {
        let records = vec![
            task("t1", "file the lamp-room report", Some(at(17, 0))),
            event("e1", "Harbour standup", at(9, 0)),
            message("m1", "Harbour Office", true),
        ];
        let b = compose(&records, now());
        // The exit criterion is a *three-connector* briefing. Two connectors and a hopeful
        // sentence is not the same thing, so the type answers the question directly.
        assert!(b.is_three_connector(), "sources: {:?}", b.sources);
        assert_eq!(b.sources, vec!["ics", "mail-work", "vault"]);
    }

    #[test]
    fn every_claim_traces_back_to_a_record() {
        // E-8. A briefing whose lines cannot be traced is indistinguishable from a plausible
        // invention, and that is the one thing this system exists to avoid.
        let records = vec![
            task("t1", "file the lamp-room report", Some(at(17, 0))),
            event("e1", "Harbour standup", at(9, 0)),
            message("m1", "Harbour Office", true),
        ];
        let b = compose(&records, now());
        for line in &b.lines {
            if line.section == Section::Readiness {
                continue; // computed from all records; cites none in particular
            }
            assert!(
                !line.citations.is_empty(),
                "{:?} cites nothing: {}",
                line.section,
                line.text
            );
        }
        assert_eq!(b.citations.len(), 3);
        assert!(b.citations.iter().any(|c| c.source == "mail-work"));
    }

    #[test]
    fn the_spoken_form_never_contains_a_file_pointer() {
        // Read aloud, "08 Journal/Daily/2026-08-24.md#L42" is unbearable. Both forms are built
        // from the same facts precisely so neither has to be stripped out of the other.
        let records = vec![
            task("t1", "file the lamp-room report", Some(at(17, 0))),
            event("e1", "Harbour standup", at(9, 0)),
        ];
        let b = compose(&records, now());
        assert!(!b.spoken.contains('#'));
        assert!(!b.spoken.contains(".md"));
        assert!(!b.spoken.contains("vault#"));
        assert!(b.spoken.contains("Harbour standup"));
    }

    #[test]
    fn an_empty_day_is_told_plainly_rather_than_padded() {
        let b = compose(&[], now());
        assert!(!b.is_three_connector());
        let text: String = b.lines.iter().map(|l| l.text.clone()).collect();
        assert!(text.contains("Nothing left on the calendar"));
        assert!(text.contains("queue is empty"));
        // No mail line at all rather than "0 messages" — a briefing that reports absences is one
        // you stop listening to.
        assert!(!b.lines.iter().any(|l| l.section == Section::Mail));
    }

    #[test]
    fn urgency_is_a_typed_predicate_here_too() {
        // X-10, restated where it would be easiest to forget: a task that merely says "urgent"
        // is not due, and a task due at five o'clock is.
        let records = vec![
            task("t1", "URGENT!! re-paint the gallery rail", None),
            task("t2", "file the lamp-room report", Some(at(17, 0))),
        ];
        let b = compose(&records, now());
        let due = b
            .lines
            .iter()
            .find(|l| l.section == Section::Tasks)
            .expect("a due line");
        assert!(due.text.starts_with("1 due today"), "{}", due.text);
        assert!(due.text.contains("lamp-room"));
        assert!(!due.text.contains("gallery rail"));
    }

    #[test]
    fn mail_that_wants_nothing_produces_no_mail_line() {
        let records = vec![message("m1", "Newsletter", false)];
        let b = compose(&records, now());
        assert!(!b.lines.iter().any(|l| l.section == Section::Mail));
    }

    #[test]
    fn the_same_records_always_produce_the_same_words() {
        // A-5's determinism, applied to prose. A briefing that reshuffles itself between two
        // glances is one you cannot trust to have told you everything.
        let records = vec![
            task("t2", "b task", Some(at(17, 0))),
            task("t1", "a task", Some(at(17, 0))),
            event("e2", "Second", at(11, 0)),
            event("e1", "First", at(9, 0)),
        ];
        let a = compose(&records, now());
        let mut shuffled = records;
        shuffled.reverse();
        let b = compose(&shuffled, now());
        assert_eq!(a.spoken, b.spoken);
        assert!(a.spoken.contains("First"));
    }

    #[test]
    fn lists_read_like_sentences() {
        assert_eq!(list(&[]), "");
        assert_eq!(list(&["one"]), "one");
        assert_eq!(list(&["one", "two"]), "one and two");
        assert_eq!(list(&["one", "two", "three"]), "one, two, and three");
    }

    #[test]
    fn chasers_appear_only_when_overdue() {
        let overdue = AnyRecord::Chaser(rec(
            "vault",
            "c1",
            Chaser {
                kind: ChaserKind::Waiting,
                who: PersonRef {
                    name: Some("Harbour Office".into()),
                    email: None,
                },
                what: "the inspection date".into(),
                since: now().date_naive() - Duration::days(9),
                last: None,
                overdue: true,
            },
        ));
        let b = compose(&[overdue], now());
        let line = b
            .lines
            .iter()
            .find(|l| l.section == Section::Chasers)
            .expect("a chasing line");
        assert!(line.text.contains("Harbour Office"));
        assert!(!line.citations.is_empty());
    }
}
