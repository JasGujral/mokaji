//! iCalendar parsing — enough of RFC 5545 to read a real calendar export.
//!
//! Written by hand rather than pulled from a crate for one reason: the file format is where a
//! calendar's lies live. Folded lines, escaped commas, three different date forms and a recurrence
//! grammar that most implementations get subtly wrong. A dependency would hide all of that; here
//! every decision is visible and every one of them has a test.

use chrono::{DateTime, Datelike, Weekday};
use chrono::{Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};

/// One event as the file describes it, before it becomes a standard `Event`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEvent {
    /// `UID`.
    pub uid: String,
    /// `SUMMARY` — becomes `title`, because one concept has one name (§5).
    pub summary: String,
    /// Start instant, UTC.
    pub start: DateTime<Utc>,
    /// End instant, UTC.
    pub end: DateTime<Utc>,
    /// `VALUE=DATE` rather than a timestamp.
    pub all_day: bool,
    /// `LOCATION`.
    pub location: Option<String>,
    /// `ATTENDEE` mailtos.
    pub attendees: Vec<String>,
    /// The raw `RRULE`, if any.
    pub rrule: Option<String>,
}

/// Unfold RFC 5545 continuation lines.
///
/// A line beginning with a space or tab continues the previous one. Miss this and a long summary
/// arrives truncated at exactly 75 octets, which looks like a data problem rather than a parsing
/// one and gets debugged in the wrong place.
#[must_use]
pub fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if (line.starts_with(' ') || line.starts_with('\t')) && !out.is_empty() {
            let last = out.len() - 1;
            out[last].push_str(&line[1..]);
        } else {
            out.push(line.to_string());
        }
    }
    out
}

/// A content line: its name, its parameters, and its value.
pub type ContentLine = (String, Vec<(String, String)>, String);

/// Split `NAME;PARAM=x:value` into name, parameters and value.
#[must_use]
pub fn split_line(line: &str) -> Option<ContentLine> {
    let colon = find_unquoted(line, ':')?;
    let (head, value) = line.split_at(colon);
    let value = &value[1..];
    let mut parts = head.split(';');
    let name = parts.next()?.trim().to_uppercase();
    let params = parts
        .filter_map(|p| {
            let (k, v) = p.split_once('=')?;
            Some((
                k.trim().to_uppercase(),
                v.trim().trim_matches('"').to_string(),
            ))
        })
        .collect();
    Some((name, params, unescape(value)))
}

fn find_unquoted(s: &str, needle: char) -> Option<usize> {
    let mut quoted = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => quoted = !quoted,
            c if c == needle && !quoted => return Some(i),
            _ => {}
        }
    }
    None
}

/// Undo RFC 5545 escaping. A comma in a location is `\,` on the wire.
#[must_use]
pub fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Parse a `DTSTART`/`DTEND` value into UTC, plus whether it was a date rather than a timestamp.
///
/// Three forms exist and all three appear in the wild:
/// `20260824T090000Z` (UTC), `20260824T090000` with a `TZID` (local), `20260824` (all-day).
///
/// A floating or `TZID` time is interpreted in the machine's local zone. That is a deliberate
/// simplification with a real consequence — an event created in another timezone will land at the
/// wrong hour — and it is recorded here rather than discovered later. Full `VTIMEZONE` handling is
/// its own project.
#[must_use]
pub fn parse_dt(value: &str, params: &[(String, String)]) -> Option<(DateTime<Utc>, bool)> {
    let v = value.trim();
    let is_date = params
        .iter()
        .any(|(k, val)| k == "VALUE" && val.eq_ignore_ascii_case("DATE"))
        || (v.len() == 8 && !v.contains('T'));

    if is_date {
        let d = NaiveDate::parse_from_str(v, "%Y%m%d").ok()?;
        let naive = d.and_hms_opt(0, 0, 0)?;
        return Some((local_to_utc(naive), true));
    }
    if let Some(stripped) = v.strip_suffix('Z') {
        let naive = NaiveDateTime::parse_from_str(stripped, "%Y%m%dT%H%M%S").ok()?;
        return Some((Utc.from_utc_datetime(&naive), false));
    }
    let naive = NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok()?;
    Some((local_to_utc(naive), false))
}

fn local_to_utc(naive: NaiveDateTime) -> DateTime<Utc> {
    chrono::Local
        .from_local_datetime(&naive)
        .earliest()
        .map_or_else(|| Utc.from_utc_datetime(&naive), |d| d.with_timezone(&Utc))
}

/// Every `VEVENT` in a calendar file.
#[must_use]
pub fn events(text: &str) -> Vec<RawEvent> {
    let mut out = Vec::new();
    let mut cur: Option<Partial> = None;

    for line in unfold(text) {
        let Some((name, params, value)) = split_line(&line) else {
            continue;
        };
        match name.as_str() {
            "BEGIN" if value == "VEVENT" => cur = Some(Partial::default()),
            "END" if value == "VEVENT" => {
                if let Some(p) = cur.take() {
                    if let Some(e) = p.finish() {
                        out.push(e);
                    }
                }
            }
            _ => {
                if let Some(p) = cur.as_mut() {
                    p.field(&name, &params, &value);
                }
            }
        }
    }
    out
}

#[derive(Default)]
struct Partial {
    uid: Option<String>,
    summary: Option<String>,
    start: Option<(DateTime<Utc>, bool)>,
    end: Option<(DateTime<Utc>, bool)>,
    duration: Option<Duration>,
    location: Option<String>,
    attendees: Vec<String>,
    rrule: Option<String>,
}

impl Partial {
    fn field(&mut self, name: &str, params: &[(String, String)], value: &str) {
        match name {
            "UID" => self.uid = Some(value.to_string()),
            "SUMMARY" => self.summary = Some(value.to_string()),
            "DTSTART" => self.start = parse_dt(value, params),
            "DTEND" => self.end = parse_dt(value, params),
            "DURATION" => self.duration = parse_duration(value),
            "LOCATION" => self.location = Some(value.to_string()),
            "RRULE" => self.rrule = Some(value.to_string()),
            "ATTENDEE" => {
                let a = value
                    .trim_start_matches("mailto:")
                    .trim_start_matches("MAILTO:");
                if !a.is_empty() {
                    self.attendees.push(a.to_string());
                }
            }
            _ => {}
        }
    }

    fn finish(self) -> Option<RawEvent> {
        let (start, all_day) = self.start?;
        // An event with no DTEND takes its DURATION, or is a whole day, or is instantaneous —
        // in that order, which is what RFC 5545 says and what calendars actually emit.
        let end = match (self.end, self.duration) {
            (Some((e, _)), _) => e,
            (None, Some(d)) => start + d,
            (None, None) if all_day => start + Duration::days(1),
            (None, None) => start,
        };
        Some(RawEvent {
            uid: self
                .uid
                .unwrap_or_else(|| format!("ics-{}", start.timestamp())),
            // An event with no SUMMARY is legal and appears in real exports; "(no title)" is more
            // useful on a Deck than an empty row.
            summary: self.summary.unwrap_or_else(|| "(no title)".into()),
            start,
            end,
            all_day,
            location: self.location,
            attendees: self.attendees,
            rrule: self.rrule,
        })
    }
}

/// `PT1H30M`, `P1D` and friends.
#[must_use]
pub fn parse_duration(v: &str) -> Option<Duration> {
    let s = v.trim().strip_prefix('P')?;
    let (date_part, time_part) = match s.split_once('T') {
        Some((d, t)) => (d, t),
        None => (s, ""),
    };
    let mut total = Duration::zero();
    let mut num = String::new();
    for c in date_part.chars() {
        if c.is_ascii_digit() {
            num.push(c);
        } else {
            let n: i64 = num.parse().ok()?;
            num.clear();
            total += match c {
                'W' => Duration::weeks(n),
                'D' => Duration::days(n),
                _ => return None,
            };
        }
    }
    for c in time_part.chars() {
        if c.is_ascii_digit() {
            num.push(c);
        } else {
            let n: i64 = num.parse().ok()?;
            num.clear();
            total += match c {
                'H' => Duration::hours(n),
                'M' => Duration::minutes(n),
                'S' => Duration::seconds(n),
                _ => return None,
            };
        }
    }
    Some(total)
}

/// Expand a recurring event into the occurrences that fall inside `[from, to)`.
///
/// **`DAILY` and `WEEKLY` only.** `MONTHLY` and `YEARLY` are not expanded, and the base occurrence
/// is returned alone. That is a real gap and it is stated here rather than silently producing a
/// calendar that is missing your birthday — a connector that quietly drops events is worse than one
/// that admits a limit.
#[must_use]
pub fn expand(event: &RawEvent, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<RawEvent> {
    let Some(rule) = &event.rrule else {
        return in_window(event, from, to);
    };
    let parts: Vec<(String, String)> = rule
        .split(';')
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.trim().to_uppercase(), v.trim().to_string()))
        .collect();
    let get = |k: &str| parts.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone());

    let freq = get("FREQ").unwrap_or_default();
    if freq != "DAILY" && freq != "WEEKLY" {
        return in_window(event, from, to);
    }

    let interval: i64 = get("INTERVAL")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .max(1);
    let count: Option<usize> = get("COUNT").and_then(|v| v.parse().ok());
    let until: Option<DateTime<Utc>> = get("UNTIL").and_then(|v| parse_dt(&v, &[]).map(|(d, _)| d));
    let bydays: Vec<Weekday> = get("BYDAY")
        .map(|v| v.split(',').filter_map(|d| weekday(d.trim())).collect())
        .unwrap_or_default();

    let span = event.end - event.start;
    let mut out = Vec::new();
    let mut cursor = event.start;
    let mut emitted = 0usize;
    // A hard iteration cap: a malformed rule with no COUNT and no UNTIL must not spin forever.
    for _ in 0..3000 {
        if cursor >= to {
            break;
        }
        if let Some(u) = until {
            if cursor > u {
                break;
            }
        }
        if let Some(c) = count {
            if emitted >= c {
                break;
            }
        }

        let matches_day = bydays.is_empty() || bydays.contains(&cursor.weekday());
        if matches_day {
            emitted += 1;
            if cursor + span > from && cursor < to {
                out.push(RawEvent {
                    start: cursor,
                    end: cursor + span,
                    ..event.clone()
                });
            }
        }

        cursor += if freq == "DAILY" {
            Duration::days(interval)
        } else if bydays.is_empty() {
            Duration::weeks(interval)
        } else {
            // With BYDAY, step a day at a time and let the day filter do the selecting; stepping a
            // week would only ever emit the start day.
            Duration::days(1)
        };
    }
    out
}

fn in_window(e: &RawEvent, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<RawEvent> {
    if e.end > from && e.start < to {
        vec![e.clone()]
    } else {
        Vec::new()
    }
}

fn weekday(s: &str) -> Option<Weekday> {
    Some(match s.to_uppercase().as_str() {
        "MO" => Weekday::Mon,
        "TU" => Weekday::Tue,
        "WE" => Weekday::Wed,
        "TH" => Weekday::Thu,
        "FR" => Weekday::Fri,
        "SA" => Weekday::Sat,
        "SU" => Weekday::Sun,
        _ => return None,
    })
}
