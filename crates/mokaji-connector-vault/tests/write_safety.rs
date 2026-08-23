//! The vault write path — **B-2, B-3, B-4, B-5, CON-4**.
//!
//! This is the only part of MOKaji that can destroy something the user cannot get back, and it is
//! the one requirement in the table rated **Severe**. The tests below are the argument that it is
//! safe, so each one names the failure it exists to prevent.
//!
//! Fixtures are invented — a lighthouse station that does not exist. Nothing from anyone's real
//! notes enters this repository (see `SECURITY.md`).

use chrono::{Local, NaiveDate, TimeZone};
use mokaji_connector_vault::write::{Edit, Snapshotter, VaultWriter, WriteError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Counts snapshots without touching the disk, so "exactly once per session" is observable.
#[derive(Default)]
struct CountingSnapshot {
    calls: Arc<AtomicUsize>,
    fail: bool,
}

impl Snapshotter for CountingSnapshot {
    fn snapshot(&self, _root: &Path) -> Result<String, WriteError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(WriteError::Io {
                path: "snapshot".into(),
                message: "no space".into(),
            });
        }
        Ok("counted".into())
    }
}

fn now() -> chrono::DateTime<Local> {
    Local
        .with_ymd_and_hms(2026, 8, 23, 10, 0, 0)
        .single()
        .unwrap()
}

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

/// A throwaway vault holding one daily note and one project.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mokaji-write-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("08 Journal/Daily")).unwrap();
    std::fs::create_dir_all(dir.join("01 Projects")).unwrap();
    std::fs::create_dir_all(dir.join("00 Inbox")).unwrap();

    std::fs::write(
        dir.join("08 Journal/Daily/2026-08-23.md"),
        "---\ntype: daily\ncreated: 2026-08-23\n---\n# Sunday, 23 August 2026\n\n## ✅ Tasks\n- [ ] Check the fog signal\n\n## 📝 Log\n- Calm sea.\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("01 Projects/Station upkeep.md"),
        "---\ntype: project\n---\n# Station upkeep\n\n## Tasks\n- [ ] Order lamp oil\n- [ ] Trim the wick\n",
    )
    .unwrap();
    dir
}

fn writer(dir: &Path, calls: Arc<AtomicUsize>) -> VaultWriter {
    VaultWriter::new(dir, Box::new(CountingSnapshot { calls, fail: false }))
}

// ---------------------------------------------------------------------------------------------
// B-4 — dry-run is the DEFAULT
// ---------------------------------------------------------------------------------------------

#[test]
fn a_fresh_writer_changes_nothing() {
    let dir = scratch("dryrun");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut w = writer(&dir, calls.clone());
    assert!(
        !w.is_armed(),
        "B-4: the type you get by default is the safe one"
    );

    let path = dir.join("08 Journal/Daily/2026-08-23.md");
    let before = std::fs::read_to_string(&path).unwrap();

    let r = w
        .apply(
            &Edit::AddTask {
                text: "Order lamp oil".into(),
                due: Some(d("2026-08-24")),
            },
            now(),
        )
        .unwrap();

    assert!(!r.applied);
    assert!(
        r.undo_id.is_none(),
        "nothing was applied, so there is nothing to undo"
    );
    assert!(
        r.diff.contains("+- [ ] Order lamp oil 📅 2026-08-24"),
        "diff: {}",
        r.diff
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        before,
        "not one byte moved"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "a dry run needs no snapshot"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------------------------
// B-2 / B-3 — surgical edits
// ---------------------------------------------------------------------------------------------

#[test]
fn adding_a_task_touches_only_the_line_it_adds() {
    let dir = scratch("add");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut w = writer(&dir, calls).armed();
    let path = dir.join("08 Journal/Daily/2026-08-23.md");
    let before = std::fs::read_to_string(&path).unwrap();

    w.apply(
        &Edit::AddTask {
            text: "Order lamp oil".into(),
            due: Some(d("2026-08-24")),
        },
        now(),
    )
    .unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    let b: Vec<&str> = before.lines().collect();
    let a: Vec<&str> = after.lines().collect();
    assert_eq!(a.len(), b.len() + 1, "exactly one line added");
    assert_eq!(
        a.iter().filter(|l| !b.contains(l)).count(),
        1,
        "and every other line is byte-for-byte what it was"
    );
    assert!(after.contains("- [ ] Order lamp oil 📅 2026-08-24"));
    // It lands under the Tasks heading, after what is already there — today's additions in order,
    // not newest-first.
    let idx_head = a.iter().position(|l| l.trim() == "## ✅ Tasks").unwrap();
    let idx_new = a.iter().position(|l| l.contains("Order lamp oil")).unwrap();
    let idx_log = a.iter().position(|l| l.trim() == "## 📝 Log").unwrap();
    assert!(
        idx_head < idx_new && idx_new < idx_log,
        "placed under the right heading"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn completing_a_task_ticks_one_line_and_stamps_the_date() {
    let dir = scratch("complete");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut w = writer(&dir, calls).armed();

    w.apply(
        &Edit::CompleteTask {
            path: "01 Projects/Station upkeep.md".into(),
            line: 7,
            expect_line: "- [ ] Order lamp oil".into(),
            completed_on: d("2026-08-23"),
        },
        now(),
    )
    .unwrap();

    let after = std::fs::read_to_string(dir.join("01 Projects/Station upkeep.md")).unwrap();
    assert!(after.contains("- [x] Order lamp oil ✅ 2026-08-23"));
    assert!(
        after.contains("- [ ] Trim the wick"),
        "the neighbouring task is untouched"
    );
    // X-11: without the completion date the task cannot count toward today's momentum, so the
    // tick alone would be a silent half-write.
    assert!(after.contains("✅ 2026-08-23"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_line_that_changed_under_us_aborts_the_write() {
    // The failure this exists to prevent: Obsidian saves on a timer, so the file we read a second
    // ago is not necessarily the file we are about to write.
    let dir = scratch("drift");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut w = writer(&dir, calls).armed();
    let path = dir.join("01 Projects/Station upkeep.md");

    // The human edits the task text in Obsidian between our read and our write.
    let edited = std::fs::read_to_string(&path)
        .unwrap()
        .replace("- [ ] Order lamp oil", "- [ ] Order paraffin instead");
    std::fs::write(&path, &edited).unwrap();

    let err = w
        .apply(
            &Edit::CompleteTask {
                path: "01 Projects/Station upkeep.md".into(),
                line: 7,
                expect_line: "- [ ] Order lamp oil".into(),
                completed_on: d("2026-08-23"),
            },
            now(),
        )
        .expect_err("B-3: must refuse");

    assert!(matches!(err, WriteError::LineDrift { .. }), "got {err:?}");
    // …and prove the guard is about CONTENT, not a wrong line number: the same edit against the
    // unedited file succeeds. Without this, a typo in the line number would make the test above
    // pass for entirely the wrong reason — which is exactly what it did on first run.
    std::fs::write(
        &path,
        std::fs::read_to_string(&path)
            .unwrap()
            .replace("- [ ] Order paraffin instead", "- [ ] Order lamp oil"),
    )
    .unwrap();
    w.apply(
        &Edit::CompleteTask {
            path: "01 Projects/Station upkeep.md".into(),
            line: 7,
            expect_line: "- [ ] Order lamp oil".into(),
            completed_on: d("2026-08-23"),
        },
        now(),
    )
    .expect("the same edit against the file it was planned from must succeed");
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("✅ 2026-08-23"),
        "the successful follow-up write landed"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_capture_never_overwrites_an_existing_one() {
    // Two thoughts on one topic in one day is normal. Silently replacing the first would be the
    // worst available answer.
    let dir = scratch("capture");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut w = writer(&dir, calls).armed();

    w.apply(
        &Edit::Capture {
            text: "The lamp bearing needs grease".into(),
        },
        now(),
    )
    .unwrap();
    w.apply(
        &Edit::Capture {
            text: "The lamp bearing needs grease".into(),
        },
        now(),
    )
    .unwrap();

    let file = dir.join("00 Inbox/The-lamp-bearing-needs-grease.md");
    let content = std::fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("type: fleeting"),
        "B-2: captures are fleeting notes"
    );
    assert_eq!(
        content.matches("- The lamp bearing needs grease").count(),
        2,
        "appended, not replaced"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn adding_a_task_creates_the_daily_note_when_it_is_missing() {
    let dir = scratch("newday");
    std::fs::remove_file(dir.join("08 Journal/Daily/2026-08-23.md")).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut w = writer(&dir, calls).armed();

    let r = w
        .apply(
            &Edit::AddTask {
                text: "Log the tide readings".into(),
                due: None,
            },
            now(),
        )
        .unwrap();
    assert!(r.diff.contains("(new file)"));
    let content = std::fs::read_to_string(dir.join("08 Journal/Daily/2026-08-23.md")).unwrap();
    assert!(
        content.starts_with("---\ntype: daily"),
        "frontmatter the vault's Dataview can read"
    );
    assert!(content.contains("- [ ] Log the tide readings"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------------------------
// B-5 — snapshot before the first write, exactly once
// ---------------------------------------------------------------------------------------------

#[test]
fn the_session_snapshot_is_taken_once_before_the_first_write() {
    let dir = scratch("snap");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut w = writer(&dir, calls.clone()).armed();
    assert!(w.snapshot_note().is_none());

    w.apply(
        &Edit::AddTask {
            text: "Trim the wick".into(),
            due: None,
        },
        now(),
    )
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(w.snapshot_note().is_some());

    w.apply(
        &Edit::AddTask {
            text: "Grease the bearing".into(),
            due: None,
        },
        now(),
    )
    .unwrap();
    w.apply(
        &Edit::Capture {
            text: "Swell building from the south".into(),
        },
        now(),
    )
    .unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "once per session, not once per write"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_snapshot_blocks_the_write_entirely() {
    // The whole point of B-5 is that it happens FIRST. A write that proceeds after the safety net
    // failed to deploy is worse than no safety net, because the user believes there is one.
    let dir = scratch("snapfail");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut w = VaultWriter::new(
        &dir,
        Box::new(CountingSnapshot {
            calls: calls.clone(),
            fail: true,
        }),
    )
    .armed();
    let path = dir.join("08 Journal/Daily/2026-08-23.md");
    let before = std::fs::read_to_string(&path).unwrap();

    let err = w
        .apply(
            &Edit::AddTask {
                text: "Order lamp oil".into(),
                due: None,
            },
            now(),
        )
        .expect_err("must not write");
    assert!(matches!(err, WriteError::Io { .. }));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        before,
        "nothing was written"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------------------------
// CON-4 — undoable for 30 seconds
// ---------------------------------------------------------------------------------------------

#[test]
fn undo_restores_the_file_within_the_window() {
    let dir = scratch("undo");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut w = writer(&dir, calls).armed();
    let path = dir.join("08 Journal/Daily/2026-08-23.md");
    let before = std::fs::read_to_string(&path).unwrap();

    let r = w
        .apply(
            &Edit::AddTask {
                text: "Mis-heard task".into(),
                due: None,
            },
            now(),
        )
        .unwrap();
    assert_ne!(std::fs::read_to_string(&path).unwrap(), before);

    let id = r.undo_id.unwrap();
    assert_eq!(w.undo_remaining(now()), Some(30));
    w.undo(&id, now() + chrono::Duration::seconds(12)).unwrap();

    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        before,
        "byte-for-byte back to where it started — this is the net under a mis-transcription"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn undo_removes_a_file_that_did_not_exist_before() {
    let dir = scratch("undonew");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut w = writer(&dir, calls).armed();

    let r = w
        .apply(
            &Edit::Capture {
                text: "Something I did not mean to say".into(),
            },
            now(),
        )
        .unwrap();
    let file = dir.join("00 Inbox/Something-I-did-not-mean-to-say.md");
    assert!(file.exists());

    w.undo(&r.undo_id.unwrap(), now()).unwrap();
    assert!(
        !file.exists(),
        "undoing a creation means the file is gone, not emptied"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn undo_refuses_after_thirty_seconds() {
    let dir = scratch("undolate");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut w = writer(&dir, calls).armed();
    let r = w
        .apply(
            &Edit::AddTask {
                text: "Order lamp oil".into(),
                due: None,
            },
            now(),
        )
        .unwrap();

    let err = w
        .undo(&r.undo_id.unwrap(), now() + chrono::Duration::seconds(31))
        .expect_err("the window closed");
    assert!(matches!(err, WriteError::UndoExpired(30)));
    assert_eq!(
        w.undo_remaining(now() + chrono::Duration::seconds(31)),
        None
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------------------------
// The end-to-end shape of M-2's exit criterion, minus the audio
// ---------------------------------------------------------------------------------------------

#[test]
fn the_canonical_utterance_reaches_the_vault() {
    // "Add a task to call the accountant tomorrow" — spoken, parsed, described, written, undoable.
    // Everything M-2 needs except the microphone.
    use mokaji_core::intent::{parse, Intent};

    let dir = scratch("e2e");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut w = writer(&dir, calls).armed();

    let intent = parse(
        "add a task to order the lamp oil tomorrow",
        now().date_naive(),
    );
    let Intent::AddTask { text, due } = intent.clone() else {
        panic!("expected AddTask")
    };

    // CON-4: what it will do, said before it does it.
    assert_eq!(
        intent.describe(),
        "Add task \"order the lamp oil\", due 2026-08-24."
    );

    let r = w.apply(&Edit::AddTask { text, due }, now()).unwrap();
    assert!(r.applied);

    let content = std::fs::read_to_string(dir.join("08 Journal/Daily/2026-08-23.md")).unwrap();
    assert!(content.contains("- [ ] order the lamp oil 📅 2026-08-24"));

    // And it is reversible for thirty seconds.
    w.undo(&r.undo_id.unwrap(), now() + chrono::Duration::seconds(5))
        .unwrap();
    let content = std::fs::read_to_string(dir.join("08 Journal/Daily/2026-08-23.md")).unwrap();
    assert!(!content.contains("order the lamp oil"));
    let _ = std::fs::remove_dir_all(&dir);
}
