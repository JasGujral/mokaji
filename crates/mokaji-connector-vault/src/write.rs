//! Writing to the vault — **B-2, B-3, B-4, B-5, CON-4**.
//!
//! This is the one requirement in the table rated **Severe** impact, and it is the only part of
//! MOKaji that can destroy something the user cannot get back. Four controls, all mandatory:
//!
//! - **B-3 — surgical, hash-guarded.** Never rewrite a file. Replace one line, or append one line,
//!   and abort if the file changed since it was read. A vault is edited by a human in Obsidian at
//!   the same time we are writing to it; assuming otherwise is how notes get clobbered.
//! - **B-4 — dry-run is the DEFAULT.** [`VaultWriter::new`] returns a writer that prints diffs and
//!   applies nothing. Turning it off is an explicit call, not a config file someone forgets about.
//! - **B-5 — snapshot before the session's first write**, exactly once, and never after a failure.
//! - **CON-4 — undoable for 30 seconds**, which is the safety net for voice mis-transcription.
//!
//! The rule the tests enforce and the code exists to honour: **every byte outside the edited line
//! is identical afterwards.**

use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use mokaji_core::intent::UNDO_WINDOW;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// What went wrong. Distinct variants because the right response differs: drift means re-read and
/// retry, a missing file means the vault moved, and a guard failure means *stop*.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// B-3: the file changed since the record was read.
    #[error("{path} changed since it was read — refusing to write (B-3). Re-read and try again.")]
    Drift {
        /// Which file.
        path: String,
    },
    /// The target line is not what we were told it would be.
    #[error("{path}:{line} is not the line we were given — refusing to write (B-3)")]
    LineDrift {
        /// Which file.
        path: String,
        /// Which line.
        line: usize,
    },
    /// Filesystem trouble.
    #[error("{path}: {message}")]
    Io {
        /// Which file.
        path: String,
        /// What happened.
        message: String,
    },
    /// The undo window closed.
    #[error("nothing to undo — the {0}s window has passed")]
    UndoExpired(i64),
    /// No such undo id.
    #[error("no pending undo with id {0}")]
    UndoUnknown(String),
}

type Result<T> = std::result::Result<T, WriteError>;

/// One surgical change.
#[derive(Debug, Clone)]
pub enum Edit {
    /// Append `- [ ] text 📅 due` to today's daily note, under `## Tasks` when that heading
    /// exists. Creates the note from the template shape if it is not there yet (B-2).
    AddTask {
        /// Task text.
        text: String,
        /// Optional due date, written in the Tasks plugin's syntax.
        due: Option<NaiveDate>,
    },
    /// Create a fleeting note in `00 Inbox` (B-2).
    Capture {
        /// Note body.
        text: String,
    },
    /// Tick one checkbox, identified by file and line, guarded by that line's exact content.
    CompleteTask {
        /// Vault-relative path.
        path: String,
        /// 1-based line number.
        line: usize,
        /// The line as we last read it. Anything else means the file moved under us.
        expect_line: String,
        /// Local date to stamp as the completion date (`✅`), so momentum counts it today (X-11).
        completed_on: NaiveDate,
    },
}

/// Proof of what happened — or, in dry-run, of what would have.
#[derive(Debug, Clone)]
pub struct Receipt {
    /// Idempotency key (A-8).
    pub mutation_id: String,
    /// False in dry-run. **B-4 makes false the default.**
    pub applied: bool,
    /// Which file.
    pub path: String,
    /// A unified-ish diff of the change, suitable for showing or speaking.
    pub diff: String,
    /// Present only when something was actually applied (CON-4).
    pub undo_id: Option<String>,
}

/// Restores one file to its previous state.
#[derive(Debug, Clone)]
struct UndoEntry {
    id: String,
    path: PathBuf,
    /// `None` means the file did not exist and undo should remove it again.
    before: Option<String>,
    at: DateTime<Utc>,
}

/// Takes the session's safety snapshot (B-5).
pub trait Snapshotter: Send + Sync {
    /// Snapshot `root`, returning a human-readable description of what was taken.
    ///
    /// # Errors
    /// If the snapshot could not be taken. A failed snapshot must block the write — the whole
    /// point is that it happens *first*.
    fn snapshot(&self, root: &Path) -> Result<String>;
}

/// Copies the vault to a timestamped sibling directory.
///
/// The fallback for a vault that is not a git repository. Slower and cruder than a commit, but it
/// works everywhere, and B-5 is not optional just because someone has not run `git init`.
pub struct CopySnapshot {
    /// Where snapshots go. Outside the vault, so a snapshot never becomes a note.
    pub dest_root: PathBuf,
}

impl Snapshotter for CopySnapshot {
    fn snapshot(&self, root: &Path) -> Result<String> {
        let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let dest = self.dest_root.join(format!("vault-snapshot-{stamp}"));
        copy_tree(root, &dest).map_err(|e| WriteError::Io {
            path: dest.display().to_string(),
            message: e.to_string(),
        })?;
        Ok(format!("copied to {}", dest.display()))
    }
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        // `.git`, `.obsidian` and friends are not content, and copying them makes the snapshot
        // slower than the thing it is protecting.
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let src = entry.path();
        let dst = to.join(&name);
        if src.is_dir() {
            copy_tree(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// Writes to a vault, safely.
pub struct VaultWriter {
    root: PathBuf,
    dry_run: bool,
    snapshotter: Box<dyn Snapshotter>,
    snapshot_taken: Option<String>,
    undo: Vec<UndoEntry>,
    seq: u64,
}

impl VaultWriter {
    /// A writer in **dry-run mode**, which is the default and stays the default (B-4).
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, snapshotter: Box<dyn Snapshotter>) -> Self {
        Self {
            root: root.into(),
            dry_run: true,
            snapshotter,
            snapshot_taken: None,
            undo: Vec::new(),
            seq: 0,
        }
    }

    /// Actually write. Deliberately a method call rather than a constructor argument: the type you
    /// get by default is the safe one, and turning the safety off is a line of code someone has to
    /// write and a reviewer can see.
    #[must_use]
    pub fn armed(mut self) -> Self {
        self.dry_run = false;
        self
    }

    /// Whether this writer will actually change files.
    #[must_use]
    pub fn is_armed(&self) -> bool {
        !self.dry_run
    }

    /// What the session's snapshot was, once taken (B-5).
    #[must_use]
    pub fn snapshot_note(&self) -> Option<&str> {
        self.snapshot_taken.as_deref()
    }

    /// Apply an edit, or describe it in dry-run.
    ///
    /// # Errors
    /// [`WriteError`] on drift, IO failure, or a failed snapshot.
    pub fn apply(&mut self, edit: &Edit, now: DateTime<Local>) -> Result<Receipt> {
        self.seq += 1;
        let mutation_id = format!("m{}", self.seq);

        let (rel, before, after) = match edit {
            Edit::AddTask { text, due } => self.plan_add_task(text, *due, now)?,
            Edit::Capture { text } => self.plan_capture(text, now)?,
            Edit::CompleteTask {
                path,
                line,
                expect_line,
                completed_on,
            } => self.plan_complete(path, *line, expect_line, *completed_on)?,
        };

        let diff = diff_of(&rel, before.as_deref(), &after);

        if self.dry_run {
            return Ok(Receipt {
                mutation_id,
                applied: false,
                path: rel,
                diff,
                undo_id: None,
            });
        }

        // B-5: exactly once per session, and BEFORE the first byte is written. A snapshot taken
        // after the write protects nothing.
        if self.snapshot_taken.is_none() {
            let note = self.snapshotter.snapshot(&self.root)?;
            self.snapshot_taken = Some(note);
        }

        let full = self.root.join(&rel);

        // B-3: re-check immediately before writing. The gap between planning and writing is small
        // but it is not zero, and Obsidian saves on a timer.
        let current = std::fs::read_to_string(&full).ok();
        if current.as_deref() != before.as_deref() {
            return Err(WriteError::Drift { path: rel });
        }

        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|e| WriteError::Io {
                path: parent.display().to_string(),
                message: e.to_string(),
            })?;
        }
        std::fs::write(&full, &after).map_err(|e| WriteError::Io {
            path: rel.clone(),
            message: e.to_string(),
        })?;

        let undo_id = format!("u{}", self.seq);
        self.undo.push(UndoEntry {
            id: undo_id.clone(),
            path: full,
            before,
            at: now.with_timezone(&Utc),
        });

        Ok(Receipt {
            mutation_id,
            applied: true,
            path: rel,
            diff,
            undo_id: Some(undo_id),
        })
    }

    /// Undo a previously applied edit, if the 30-second window is still open (CON-4).
    ///
    /// # Errors
    /// [`WriteError::UndoUnknown`] or [`WriteError::UndoExpired`].
    pub fn undo(&mut self, undo_id: &str, now: DateTime<Local>) -> Result<String> {
        let idx = self
            .undo
            .iter()
            .position(|e| e.id == undo_id)
            .ok_or_else(|| WriteError::UndoUnknown(undo_id.to_string()))?;

        if now.with_timezone(&Utc) - self.undo[idx].at > UNDO_WINDOW {
            return Err(WriteError::UndoExpired(UNDO_WINDOW.num_seconds()));
        }

        let entry = self.undo.remove(idx);
        match &entry.before {
            Some(content) => std::fs::write(&entry.path, content),
            // The file did not exist before, so undoing means it should not exist now.
            None => std::fs::remove_file(&entry.path),
        }
        .map_err(|e| WriteError::Io {
            path: entry.path.display().to_string(),
            message: e.to_string(),
        })?;

        Ok(format!("restored {}", entry.path.display()))
    }

    /// How long is left on the newest undo, in seconds.
    #[must_use]
    pub fn undo_remaining(&self, now: DateTime<Local>) -> Option<i64> {
        let last = self.undo.last()?;
        let left = UNDO_WINDOW - (now.with_timezone(&Utc) - last.at);
        (left > Duration::zero()).then(|| left.num_seconds())
    }

    // ---- planning: pure, so the diff a user sees is computed the same way the write is ----

    fn plan_add_task(
        &self,
        text: &str,
        due: Option<NaiveDate>,
        now: DateTime<Local>,
    ) -> Result<(String, Option<String>, String)> {
        let date = now.date_naive();
        let rel = format!("08 Journal/Daily/{date}.md");
        let full = self.root.join(&rel);
        let before = std::fs::read_to_string(&full).ok();

        let line = match due {
            Some(d) => format!("- [ ] {text} 📅 {d}"),
            None => format!("- [ ] {text}"),
        };

        let after = match &before {
            Some(content) => insert_under_heading(content, "## ✅ Tasks", "## Tasks", &line),
            None => format!(
                "---\ntype: daily\ncreated: {date}\ntags: [daily]\n---\n# {}\n\n## ✅ Tasks\n{line}\n",
                now.format("%A, %d %B %Y")
            ),
        };
        Ok((rel, before, after))
    }

    fn plan_capture(
        &self,
        text: &str,
        now: DateTime<Local>,
    ) -> Result<(String, Option<String>, String)> {
        let date = now.date_naive();
        let slug = slug(text);
        let rel = format!("00 Inbox/{slug}.md");
        let full = self.root.join(&rel);
        let before = std::fs::read_to_string(&full).ok();
        // Never overwrite an existing capture; append instead. Two thoughts on one topic in one
        // day is normal, and silently replacing the first would be the worst possible answer.
        let after = match &before {
            Some(c) => format!("{}\n- {text}\n", c.trim_end()),
            None => format!(
                "---\ntype: fleeting\ncreated: {date}\ntags: [fleeting]\n---\n# {}\n\n- {text}\n",
                first_line(text)
            ),
        };
        Ok((rel, before, after))
    }

    fn plan_complete(
        &self,
        rel: &str,
        line_no: usize,
        expect: &str,
        completed_on: NaiveDate,
    ) -> Result<(String, Option<String>, String)> {
        let full = self.root.join(rel);
        let before = std::fs::read_to_string(&full).map_err(|e| WriteError::Io {
            path: rel.to_string(),
            message: e.to_string(),
        })?;

        let mut lines: Vec<String> = before.lines().map(ToString::to_string).collect();
        let idx = line_no.checked_sub(1).ok_or(WriteError::LineDrift {
            path: rel.to_string(),
            line: line_no,
        })?;
        let current = lines.get(idx).ok_or(WriteError::LineDrift {
            path: rel.to_string(),
            line: line_no,
        })?;

        // B-3, the sharp end: the line must be exactly what we were told. Not "starts with", not
        // "contains" — a task whose text was edited in Obsidian is a different task now.
        if current.trim() != expect.trim() {
            return Err(WriteError::LineDrift {
                path: rel.to_string(),
                line: line_no,
            });
        }

        let ticked = current
            .replacen("- [ ]", "- [x]", 1)
            .replacen("* [ ]", "* [x]", 1);
        // X-11 needs a completion date or the task cannot count toward today's momentum.
        let ticked = if ticked.contains('✅') {
            ticked
        } else {
            format!("{} ✅ {completed_on}", ticked.trim_end())
        };
        lines[idx] = ticked;

        let mut after = lines.join("\n");
        if before.ends_with('\n') {
            after.push('\n');
        }
        Ok((rel.to_string(), Some(before), after))
    }
}

/// Insert `line` after the first matching heading, or append at the end if none is present.
fn insert_under_heading(content: &str, primary: &str, fallback: &str, line: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let target = lines.iter().position(|l| {
        let t = l.trim();
        t == primary || t == fallback
    });

    let mut out: Vec<String> = lines.iter().map(ToString::to_string).collect();
    match target {
        Some(h) => {
            // Place it after the heading's existing items, so today's additions land in order
            // rather than newest-first.
            let mut at = h + 1;
            while at < out.len() {
                let t = out[at].trim();
                if t.starts_with("- ") || t.starts_with("* ") || t.is_empty() {
                    at += 1;
                } else {
                    break;
                }
            }
            // Step back over trailing blanks so we do not leave a gap in the list.
            while at > h + 1 && out[at - 1].trim().is_empty() {
                at -= 1;
            }
            out.insert(at, line.to_string());
        }
        None => {
            if !out.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
                out.push(String::new());
            }
            out.push("## ✅ Tasks".to_string());
            out.push(line.to_string());
        }
    }
    let mut s = out.join("\n");
    if content.ends_with('\n') {
        s.push('\n');
    }
    s
}

fn diff_of(path: &str, before: Option<&str>, after: &str) -> String {
    let b: Vec<&str> = before.unwrap_or("").lines().collect();
    let a: Vec<&str> = after.lines().collect();
    let mut out = format!("--- {path}\n");
    if before.is_none() {
        out.push_str("(new file)\n");
    }
    // Only the changed lines: a diff nobody reads is a confirmation nobody gives.
    for (i, line) in a.iter().enumerate() {
        match b.get(i) {
            Some(old) if old == line => {}
            Some(old) => {
                out.push_str(&format!("-{old}\n+{line}\n"));
            }
            None => out.push_str(&format!("+{line}\n")),
        }
    }
    for old in b.iter().skip(a.len()) {
        out.push_str(&format!("-{old}\n"));
    }
    out
}

fn slug(text: &str) -> String {
    let s: String = text
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .take(8)
        .collect::<Vec<_>>()
        .join("-");
    if s.is_empty() {
        let mut h = DefaultHasher::new();
        text.hash(&mut h);
        format!("capture-{:x}", h.finish())
    } else {
        s
    }
}

fn first_line(text: &str) -> String {
    text.lines()
        .next()
        .unwrap_or(text)
        .chars()
        .take(60)
        .collect()
}
