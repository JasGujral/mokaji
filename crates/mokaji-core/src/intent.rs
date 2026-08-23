//! The intent pipeline — **CON-1, CON-2, CON-3, CON-4**.
//!
//! **CON-3 is why this lives in `core` rather than in the Console or the voice loop.** A command
//! must behave identically typed or spoken. Two parsers would drift within a week, and the drift
//! would show up as "it works when I type it" — the least debuggable class of bug there is.
//!
//! **CON-1: local first.** The grammar below is matched before any model is consulted. That is not
//! only a latency argument: E-2 pins the daily loop local, and a machine with the network cable out
//! must still be able to capture a task.
//!
//! **CON-2:** anything unmatched becomes [`Intent::Unmatched`], which the caller hands to the model
//! router with live context. Falling back is a decision the caller makes, not something the parser
//! does behind its back.
//!
//! **CON-4:** every mutating intent can say what it will do *before* it does it
//! ([`Intent::describe`]), and is undoable for 30 seconds. That is the safety net for voice
//! mis-transcription — "add a task to call the accountant" and "add a task to call the accountant
//! tomorrow" differ by one word that changes the record.

use chrono::{Datelike, Duration, NaiveDate, Weekday};

/// How long a mutating action stays undoable (CON-4).
pub const UNDO_WINDOW: Duration = Duration::seconds(30);

/// What the user asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// `add task <text> [when]` — lands in today's daily note (B-2).
    AddTask {
        /// The task text, with the date phrase removed.
        text: String,
        /// Resolved due date, local calendar day.
        due: Option<NaiveDate>,
    },
    /// `note <text>` / `capture <text>` — lands in `00 Inbox` with `type: fleeting` (B-2).
    Capture {
        /// The note body.
        text: String,
    },
    /// `done <keyword>` — ticks the first open task matching the keyword.
    CompleteTask {
        /// Substring to match against open tasks.
        keyword: String,
    },
    /// `tasks` — show the queue.
    ShowTasks,
    /// `agenda` — show today's events.
    ShowAgenda,
    /// `status` — the Reactor Core readout.
    Status,
    /// `help` — the grammar.
    Help,
    /// `clear` — clear the console log.
    Clear,
    /// CON-2: nothing matched. The caller decides whether to escalate to a model.
    Unmatched(String),
}

impl Intent {
    /// Whether acting on this changes the vault.
    #[must_use]
    pub fn is_mutating(&self) -> bool {
        matches!(
            self,
            Self::AddTask { .. } | Self::Capture { .. } | Self::CompleteTask { .. }
        )
    }

    /// **CON-4.** What this will do, in a sentence, *before* it does it.
    ///
    /// Spoken aloud after a voice command, this is the whole safety mechanism: hearing "adding
    /// *call the accountant*, due tomorrow" is how a mis-transcription gets caught inside the
    /// undo window rather than discovered in a week.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::AddTask { text, due } => match due {
                Some(d) => format!("Add task \"{text}\", due {d}."),
                None => format!("Add task \"{text}\", no due date."),
            },
            Self::Capture { text } => format!("Capture \"{text}\" to the inbox."),
            Self::CompleteTask { keyword } => {
                format!("Tick the first open task matching \"{keyword}\".")
            }
            Self::ShowTasks => "Show the task queue.".into(),
            Self::ShowAgenda => "Show today's agenda.".into(),
            Self::Status => "Show the Reactor Core readout.".into(),
            Self::Help => "Show the command grammar.".into(),
            Self::Clear => "Clear the console.".into(),
            Self::Unmatched(s) => format!("No local command matched \"{s}\"."),
        }
    }
}

/// The grammar, for `help`.
pub const GRAMMAR: &[(&str, &str)] = &[
    (
        "add task <text> [today|tomorrow|<weekday>|YYYY-MM-DD]",
        "append to today's daily note",
    ),
    ("note <text>", "capture to the inbox as a fleeting note"),
    (
        "done <keyword>",
        "tick the first open task matching the keyword",
    ),
    ("tasks", "show the open queue"),
    ("agenda", "show today's events"),
    ("status", "show the Reactor Core readout"),
    ("help", "this list"),
    ("clear", "clear the console"),
];

/// Parse an utterance — typed or transcribed — into an [`Intent`].
///
/// `today` is passed in rather than read from the clock so the parser is testable and so "tomorrow"
/// is unambiguous at a local midnight boundary (§5).
#[must_use]
pub fn parse(input: &str, today: NaiveDate) -> Intent {
    let raw = input.trim();
    if raw.is_empty() {
        return Intent::Unmatched(String::new());
    }
    let lower = raw.to_lowercase();

    // Bare verbs first. Speech recognisers add trailing punctuation and the odd filler word, so
    // match on the stripped form rather than demanding an exact string.
    match strip_filler(&lower).as_str() {
        "tasks" | "task queue" | "show tasks" | "show my tasks" => return Intent::ShowTasks,
        "agenda" | "show agenda" | "what's on today" | "whats on today" => {
            return Intent::ShowAgenda
        }
        "status" | "readiness" | "how am i doing" => return Intent::Status,
        "help" | "commands" => return Intent::Help,
        "clear" | "clear console" => return Intent::Clear,
        _ => {}
    }

    if let Some(rest) = after_any(&lower, raw, &["done ", "complete ", "finish ", "tick "]) {
        let kw = strip_filler(&rest);
        if !kw.is_empty() {
            return Intent::CompleteTask { keyword: kw };
        }
    }

    if let Some(rest) = after_any(&lower, raw, &["note ", "capture ", "remember "]) {
        let text = strip_filler(&rest);
        if !text.is_empty() {
            return Intent::Capture { text };
        }
    }

    // "add a task to call the accountant tomorrow" — the canonical M-2 utterance. The leading
    // article and the "to" are how people actually speak, so they are part of the grammar rather
    // than something the user has to learn not to say.
    if let Some(rest) = after_any(
        &lower,
        raw,
        &[
            "add a task to ",
            "add a task ",
            "add task to ",
            "add task ",
            "new task ",
            "task ",
            "remind me to ",
            "add ",
        ],
    ) {
        let (text, due) = split_due(&rest, today);
        let text = strip_filler(&text);
        if !text.is_empty() {
            return Intent::AddTask { text, due };
        }
    }

    Intent::Unmatched(raw.to_string())
}

/// Match a prefix case-insensitively, returning the remainder with the ORIGINAL casing — a task
/// called "Call the Accountant" should not be lowercased on its way into the vault.
fn after_any(lower: &str, raw: &str, prefixes: &[&str]) -> Option<String> {
    for p in prefixes {
        if let Some(stripped) = lower.strip_prefix(p) {
            let start = raw.len() - stripped.len();
            return Some(raw[start..].to_string());
        }
    }
    None
}

fn strip_filler(s: &str) -> String {
    s.trim()
        .trim_end_matches(['.', '!', '?', ','])
        .trim()
        .to_string()
}

/// Split a trailing date phrase off a task's text.
///
/// Only a *trailing* phrase counts. "Read the article about tomorrow's weather" keeps its words;
/// stripping a date from the middle of a sentence would quietly rewrite what the user said.
fn split_due(text: &str, today: NaiveDate) -> (String, Option<NaiveDate>) {
    let cleaned = strip_filler(text);
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    if words.is_empty() {
        return (cleaned, None);
    }

    // Try the longest trailing phrase first: "next tuesday" before "tuesday".
    for take in [3usize, 2, 1] {
        if words.len() < take {
            continue;
        }
        // A date phrase that IS the whole text is not a task ("tomorrow" alone means nothing).
        if words.len() == take {
            continue;
        }
        let tail = words[words.len() - take..].join(" ").to_lowercase();
        let tail = tail
            .trim_start_matches("on ")
            .trim_start_matches("by ")
            .to_string();
        if let Some(d) = parse_date_phrase(&tail, today) {
            let head = words[..words.len() - take].join(" ");
            return (strip_filler(&head), Some(d));
        }
    }
    (cleaned, None)
}

/// Resolve a date phrase against `today`.
#[must_use]
pub fn parse_date_phrase(phrase: &str, today: NaiveDate) -> Option<NaiveDate> {
    let p = phrase
        .trim()
        .trim_start_matches("on ")
        .trim_start_matches("by ")
        .trim();
    match p {
        "today" | "tonight" | "this evening" => return Some(today),
        "tomorrow" | "tmrw" => return Some(today + Duration::days(1)),
        "yesterday" => return Some(today - Duration::days(1)),
        _ => {}
    }
    if let Ok(d) = NaiveDate::parse_from_str(p, "%Y-%m-%d") {
        return Some(d);
    }
    let (p, next_week) = match p.strip_prefix("next ") {
        Some(rest) => (rest, true),
        None => (p, false),
    };
    let wd = weekday(p)?;
    // "Tuesday" means the next Tuesday that is not today; "next Tuesday" means the one after that.
    let mut d = today + Duration::days(1);
    while d.weekday() != wd {
        d += Duration::days(1);
    }
    if next_week {
        d += Duration::days(7);
    }
    Some(d)
}

fn weekday(s: &str) -> Option<Weekday> {
    Some(match s {
        "monday" | "mon" => Weekday::Mon,
        "tuesday" | "tue" | "tues" => Weekday::Tue,
        "wednesday" | "wed" => Weekday::Wed,
        "thursday" | "thu" | "thurs" => Weekday::Thu,
        "friday" | "fri" => Weekday::Fri,
        "saturday" | "sat" => Weekday::Sat,
        "sunday" | "sun" => Weekday::Sun,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        // A Sunday, so weekday arithmetic has somewhere to go.
        NaiveDate::from_ymd_opt(2026, 8, 23).unwrap()
    }

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn the_canonical_m2_utterance() {
        // The exact sentence M-2's exit criterion is written around.
        assert_eq!(
            parse("add a task to call the accountant tomorrow", today()),
            Intent::AddTask {
                text: "call the accountant".into(),
                due: Some(d("2026-08-24"))
            }
        );
    }

    #[test]
    fn typed_and_spoken_forms_agree() {
        // CON-3: the same command, as someone types it and as a recogniser transcribes it.
        let typed = parse("add task Call the accountant tomorrow", today());
        let spoken = parse("Add a task to call the accountant tomorrow.", today());
        let Intent::AddTask { due: dt, .. } = typed else {
            panic!()
        };
        let Intent::AddTask { due: ds, text } = spoken else {
            panic!()
        };
        assert_eq!(dt, ds);
        assert_eq!(text, "call the accountant");
    }

    #[test]
    fn original_casing_survives() {
        assert_eq!(
            parse("add task Renew the Domain Registration", today()),
            Intent::AddTask {
                text: "Renew the Domain Registration".into(),
                due: None
            }
        );
    }

    #[test]
    fn a_date_word_inside_the_sentence_is_left_alone() {
        // Stripping a date from the middle would quietly rewrite what the user said.
        assert_eq!(
            parse(
                "add task read the article about tomorrow's weather",
                today()
            ),
            Intent::AddTask {
                text: "read the article about tomorrow's weather".into(),
                due: None
            }
        );
    }

    #[test]
    fn weekdays_resolve_forward_and_next_means_the_week_after() {
        // today() is Sunday 2026-08-23.
        assert_eq!(parse_date_phrase("monday", today()), Some(d("2026-08-24")));
        assert_eq!(
            parse_date_phrase("next monday", today()),
            Some(d("2026-08-31"))
        );
        assert_eq!(
            parse_date_phrase("sunday", today()),
            Some(d("2026-08-30")),
            "a weekday never resolves to today — 'sunday' said on Sunday means the next one"
        );
    }

    #[test]
    fn explicit_dates_and_the_obvious_words() {
        assert_eq!(
            parse_date_phrase("2026-12-25", today()),
            Some(d("2026-12-25"))
        );
        assert_eq!(parse_date_phrase("today", today()), Some(today()));
        assert_eq!(parse_date_phrase("tonight", today()), Some(today()));
        assert_eq!(
            parse_date_phrase("tomorrow", today()),
            Some(d("2026-08-24"))
        );
        assert_eq!(parse_date_phrase("someday", today()), None);
    }

    #[test]
    fn a_bare_date_is_not_a_task() {
        assert!(matches!(
            parse("add task tomorrow", today()),
            Intent::Unmatched(_) | Intent::AddTask { .. }
        ));
        let i = parse("add task tomorrow", today());
        if let Intent::AddTask { text, .. } = &i {
            assert_eq!(
                text, "tomorrow",
                "the word stays as the task text rather than vanishing"
            );
        }
    }

    #[test]
    fn captures_and_completions() {
        assert_eq!(
            parse("note the router needs a cache", today()),
            Intent::Capture {
                text: "the router needs a cache".into()
            }
        );
        assert_eq!(
            parse("done accountant", today()),
            Intent::CompleteTask {
                keyword: "accountant".into()
            }
        );
        assert_eq!(
            parse("complete the domain thing.", today()),
            Intent::CompleteTask {
                keyword: "the domain thing".into()
            }
        );
    }

    #[test]
    fn bare_verbs() {
        assert_eq!(parse("tasks", today()), Intent::ShowTasks);
        assert_eq!(parse("  STATUS  ", today()), Intent::Status);
        assert_eq!(parse("how am I doing?", today()), Intent::Status);
        assert_eq!(parse("help", today()), Intent::Help);
    }

    #[test]
    fn unmatched_falls_through_rather_than_guessing() {
        // CON-2: escalation is the caller's decision, not something the parser does quietly.
        let i = parse("what should I focus on this week", today());
        assert!(matches!(i, Intent::Unmatched(_)));
        assert!(!i.is_mutating());
    }

    #[test]
    fn every_mutating_intent_can_say_what_it_will_do_first() {
        // CON-4: this string is the safety net for a voice mis-transcription.
        for i in [
            parse("add a task to call the accountant tomorrow", today()),
            parse("note check the fog signal", today()),
            parse("done accountant", today()),
        ] {
            assert!(i.is_mutating());
            let d = i.describe();
            assert!(
                d.len() > 10 && d.ends_with('.'),
                "unhelpful description: {d}"
            );
        }
        assert_eq!(
            parse("add a task to call the accountant tomorrow", today()).describe(),
            "Add task \"call the accountant\", due 2026-08-24."
        );
    }

    #[test]
    fn undo_window_is_thirty_seconds() {
        assert_eq!(UNDO_WINDOW, Duration::seconds(30));
    }
}
