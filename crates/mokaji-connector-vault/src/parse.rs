//! Markdown parsing. Pure functions over `&str`, so **A-2**'s "each stage separately testable"
//! is real rather than aspirational — every rule below can be checked without a filesystem.

use chrono::NaiveDate;

/// A task line as written in the vault, before it becomes a standard `Task`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawTask {
    /// Display text, with the metadata markers stripped out.
    pub text: String,
    /// Whether the checkbox is ticked.
    pub done: bool,
    /// From the Tasks plugin's `📅 YYYY-MM-DD`.
    pub due: Option<NaiveDate>,
    /// From the Tasks plugin's `✅ YYYY-MM-DD`, written when you tick the box.
    pub completion: Option<NaiveDate>,
    /// `#tags` found in the line, without the `#`.
    pub tags: Vec<String>,
    /// 1-based line number, for the `source_ref` that points home.
    pub line: usize,
}

/// The Tasks plugin's date signifiers. Only these two matter to v1; the others
/// (`➕` created, `⏳` scheduled, `🛫` start, `🔁` recurring) are recognised so they are stripped
/// from the display text rather than left as visual noise.
const DUE: char = '📅';
const DONE: char = '✅';
const OTHER_DATE_SIGNIFIERS: [char; 4] = ['➕', '⏳', '🛫', '🔁'];

/// Split YAML frontmatter from the body.
///
/// Returns `(frontmatter, body, body_start_line)`. A file with no frontmatter is all body, which
/// is the common case for project notes.
#[must_use]
pub fn split_frontmatter(content: &str) -> (Option<&str>, &str, usize) {
    let rest = match content.strip_prefix("---\n") {
        Some(r) => r,
        None => return (None, content, 1),
    };
    match rest.find("\n---\n") {
        Some(end) => {
            let fm = &rest[..end];
            let body = &rest[end + 5..];
            // 1 for the opening ---, the frontmatter's own lines, 1 for the closing ---
            let start = 2 + fm.lines().count();
            (Some(fm), body, start + 1)
        }
        None => (None, content, 1),
    }
}

/// Read one scalar key out of frontmatter.
///
/// Deliberately not a full YAML parser: v1 needs a handful of scalars (`type`, `mood`, `energy`,
/// `focus`, `deep_work_hours`, `sleep_hours`, `exercised`, `horizon`, `status`, `target_date`),
/// and pulling in a YAML dependency to read `mood: 4` would be a poor trade. Arrays and nested
/// maps return `None` rather than something half-parsed — an honest absence beats a wrong value.
#[must_use]
pub fn frontmatter_value<'a>(fm: &'a str, key: &str) -> Option<&'a str> {
    for line in fm.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        if k.trim() != key {
            continue;
        }
        let v = v.trim();
        if v.is_empty() || v.starts_with('[') || v.starts_with('{') {
            return None;
        }
        return Some(v.trim_matches('"').trim_matches('\''));
    }
    None
}

/// Every task line in `body`, **skipping fenced code blocks**.
///
/// The fence rule is not pedantry. `09 Command Center/Chasers.md` documented its own format with
/// three `- [ ]` example lines, one tagged `#overdue`; Dataview counted the invented example as a
/// real overdue chaser and quietly took 10 points off Focus Clarity. Documentation that counts as
/// data is a trap, and this is where it gets defused.
///
/// `body_start_line` is the 1-based line number `body` begins at, so `RawTask::line` points at the
/// real line in the real file.
#[must_use]
pub fn tasks(body: &str, body_start_line: usize) -> Vec<RawTask> {
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut fence_marker = "";

    for (i, line) in body.lines().enumerate() {
        let trimmed = line.trim_start();

        // Track fences. The closing marker must be at least as long as the opening one, which is
        // how nested code blocks in markdown work.
        if let Some(marker) = fence_of(trimmed) {
            if in_fence {
                if marker.len() >= fence_marker.len() && marker.starts_with(&fence_marker[..1]) {
                    in_fence = false;
                    fence_marker = "";
                }
            } else {
                in_fence = true;
                fence_marker = marker;
            }
            continue;
        }
        if in_fence {
            continue;
        }

        let Some(raw) = checkbox(trimmed) else {
            continue;
        };
        let (done, rest) = raw;
        let parsed = parse_task_text(rest);

        // Blank `- [ ]` placeholders come from the daily-note template. An empty checkbox is an
        // affordance, not a commitment — counting it would drain bandwidth a little more every day.
        if parsed.0.is_empty() {
            continue;
        }

        out.push(RawTask {
            text: parsed.0,
            done,
            due: parsed.1,
            completion: parsed.2,
            tags: parsed.3,
            line: body_start_line + i,
        });
    }
    out
}

fn fence_of(trimmed: &str) -> Option<&str> {
    for m in ["```", "~~~"] {
        if trimmed.starts_with(m) {
            let len = trimmed
                .chars()
                .take_while(|c| *c == m.as_bytes()[0] as char)
                .count();
            return Some(&trimmed[..len]);
        }
    }
    None
}

/// `- [ ] text` / `- [x] text` / `* [X] text`, returning `(done, remainder)`.
fn checkbox(trimmed: &str) -> Option<(bool, &str)> {
    let rest = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))?;
    let rest = rest.strip_prefix('[')?;
    let (mark, rest) = rest.split_at(rest.char_indices().nth(1)?.0);
    let rest = rest.strip_prefix(']')?;
    let done = match mark {
        " " => false,
        "x" | "X" => true,
        // Any other marker is one of Obsidian's custom checkbox states (`/`, `-`, `?`). They are
        // not "done", and treating them as open is the conservative reading.
        _ => false,
    };
    Some((done, rest.trim()))
}

/// Pull the metadata out of a task's text, returning `(clean_text, due, completion, tags)`.
fn parse_task_text(s: &str) -> (String, Option<NaiveDate>, Option<NaiveDate>, Vec<String>) {
    let mut due = None;
    let mut completion = None;
    let mut tags = Vec::new();
    let mut text = String::with_capacity(s.len());

    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == DUE || c == DONE || OTHER_DATE_SIGNIFIERS.contains(&c) {
            let rest: String = chars.clone().collect();
            let date = leading_date(&rest);
            if date.is_some() {
                // consume the whitespace + 10 date chars we just read
                let skip = rest.len() - rest.trim_start().len() + 10;
                for _ in 0..skip {
                    chars.next();
                }
            }
            match c {
                DUE => due = date,
                DONE => completion = date,
                _ => {}
            }
            continue;
        }
        if c == '#' {
            let tag: String = chars
                .clone()
                .take_while(|ch| ch.is_alphanumeric() || *ch == '-' || *ch == '_' || *ch == '/')
                .collect();
            if !tag.is_empty() {
                for _ in 0..tag.chars().count() {
                    chars.next();
                }
                tags.push(tag);
                continue;
            }
        }
        text.push(c);
    }

    (collapse_ws(&text), due, completion, tags)
}

fn leading_date(s: &str) -> Option<NaiveDate> {
    let t = s.trim_start();
    if t.len() < 10 {
        return None;
    }
    NaiveDate::parse_from_str(&t[..10], "%Y-%m-%d").ok()
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `— since 2026-06-18` anywhere in a chaser line.
#[must_use]
pub fn since_date(text: &str) -> Option<NaiveDate> {
    let lower = text.to_lowercase();
    let idx = lower.find("since ")?;
    leading_date(&text[idx + 6..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn frontmatter_splits_and_reads_scalars() {
        let content = "---\ntype: daily\nmood: 4\nexercised: true\ntags: [daily]\narea: \"[[Personal]]\"\n---\n# Title\nbody\n";
        let (fm, body, start) = split_frontmatter(content);
        let fm = fm.expect("frontmatter found");
        assert_eq!(frontmatter_value(fm, "type"), Some("daily"));
        assert_eq!(frontmatter_value(fm, "mood"), Some("4"));
        assert_eq!(frontmatter_value(fm, "exercised"), Some("true"));
        assert_eq!(
            frontmatter_value(fm, "tags"),
            None,
            "an array returns None rather than something half-parsed"
        );
        assert_eq!(frontmatter_value(fm, "missing"), None);
        assert!(body.starts_with("# Title"));
        assert_eq!(start, 8, "body line numbers stay true to the file");
    }

    #[test]
    fn a_file_without_frontmatter_is_all_body() {
        let (fm, body, start) = split_frontmatter("# Title\n- [ ] a task\n");
        assert!(fm.is_none());
        assert_eq!(start, 1);
        assert_eq!(tasks(body, start)[0].line, 2);
    }

    #[test]
    fn reads_tasks_plugin_dates() {
        let t = &tasks("- [ ] Order lamp oil 📅 2026-08-24\n", 1)[0];
        assert_eq!(t.text, "Order lamp oil");
        assert_eq!(t.due, Some(d("2026-08-24")));
        assert!(!t.done);

        let t = &tasks("- [x] Paint the south railing ✅ 2026-06-20\n", 1)[0];
        assert_eq!(t.text, "Paint the south railing");
        assert_eq!(t.completion, Some(d("2026-06-20")));
        assert!(t.done);
    }

    #[test]
    fn strips_other_signifiers_from_display_text() {
        let t = &tasks("- [ ] Trim the wick 🔁 ➕ 2026-08-01 📅 2026-08-24\n", 1)[0];
        assert_eq!(
            t.text, "Trim the wick",
            "no emoji litter in the display text"
        );
        assert_eq!(t.due, Some(d("2026-08-24")));
    }

    #[test]
    fn extracts_tags_without_leaving_them_in_the_text() {
        let t = &tasks("- [ ] #waiting #overdue Chandlery invoice\n", 1)[0];
        assert_eq!(t.tags, vec!["waiting", "overdue"]);
        assert_eq!(t.text, "Chandlery invoice");
    }

    #[test]
    fn blank_checkboxes_are_skipped() {
        let found = tasks("- [ ] \n- [ ]\n- [ ] real one\n", 1);
        assert_eq!(
            found.len(),
            1,
            "template placeholders are an affordance, not a task"
        );
        assert_eq!(found[0].text, "real one");
    }

    #[test]
    fn fenced_code_blocks_are_not_tasks() {
        // The exact shape of Chasers.md after the fix.
        let body = "## Add a chaser\n\n```markdown\n- [ ] #waiting #overdue Chandlery invoice\n```\n\n- [ ] #nudge a real one\n";
        let found = tasks(body, 1);
        assert_eq!(
            found.len(),
            1,
            "the documented example must not count as data"
        );
        assert_eq!(found[0].tags, vec!["nudge"]);
    }

    #[test]
    fn tasks_blocks_from_the_tasks_plugin_are_also_fenced_and_skipped() {
        let body = "```tasks\nnot done\ntags include #waiting\n```\n- [ ] #waiting real\n";
        assert_eq!(tasks(body, 1).len(), 1);
    }

    #[test]
    fn custom_checkbox_states_are_treated_as_open() {
        let found = tasks("- [/] in progress\n- [-] cancelled\n", 1);
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|t| !t.done), "only [x] means done");
    }

    #[test]
    fn since_dates_are_read_from_chaser_lines() {
        assert_eq!(
            since_date("Harbour office to confirm the tide tables — since 2026-06-18"),
            Some(d("2026-06-18"))
        );
        assert_eq!(since_date("no date here"), None);
    }

    #[test]
    fn line_numbers_point_at_the_real_line() {
        let content = "---\ntype: daily\n---\n# Title\n\n- [ ] first\n- [ ] second\n";
        let (_, body, start) = split_frontmatter(content);
        let found = tasks(body, start);
        assert_eq!(found[0].line, 6);
        assert_eq!(found[1].line, 7);
    }
}
