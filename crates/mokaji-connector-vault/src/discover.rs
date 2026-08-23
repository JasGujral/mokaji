//! Finding the vault without being told — **H-3**.
//!
//! "An empty config must boot to a working app against a discovered vault" means something
//! different in a terminal and in a double-clicked app. A shell has a working directory and an
//! environment; a macOS app launched from Finder has neither, which is how MOKaji once reported a
//! confident `100% OPTIMAL` while reading nothing at all.
//!
//! This module is the single implementation, used by both the CLI and the desktop shell. It was
//! two implementations for exactly one day, which was long enough for them to disagree.

use std::path::{Path, PathBuf};

/// A vault, for MOKaji's purposes, is a folder containing `08 Journal/Daily`.
///
/// Deliberately narrower than "an Obsidian vault": a vault without that layout would produce an
/// empty Deck, and silently adopting one is worse than asking.
#[must_use]
pub fn is_vault(p: &Path) -> bool {
    !p.as_os_str().is_empty() && p.join("08 Journal/Daily").is_dir()
}

/// Where the remembered vault choice lives.
#[must_use]
pub fn config_path() -> Option<PathBuf> {
    home().map(|h| h.join(".config/mokaji/vault"))
}

/// Where the remembered calendar folder lives.
#[must_use]
pub fn calendar_config_path() -> Option<PathBuf> {
    home().map(|h| h.join(".config/mokaji/calendar"))
}

/// The folder of `.ics` files the user chose, if it still exists.
#[must_use]
pub fn remembered_calendar() -> Option<PathBuf> {
    let text = std::fs::read_to_string(calendar_config_path()?).ok()?;
    let p = PathBuf::from(text.trim());
    p.is_dir().then_some(p)
}

/// Remember a calendar folder.
///
/// # Errors
/// If the path is not a directory, or the file cannot be written.
pub fn remember_calendar(p: &Path) -> Result<(), String> {
    if !p.is_dir() {
        return Err(format!("{} is not a folder", p.display()));
    }
    let cfg = calendar_config_path().ok_or("cannot locate $HOME")?;
    if let Some(dir) = cfg.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&cfg, p.display().to_string()).map_err(|e| e.to_string())
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// The vault the user last chose, if it is still a vault.
#[must_use]
pub fn remembered() -> Option<PathBuf> {
    let text = std::fs::read_to_string(config_path()?).ok()?;
    let p = PathBuf::from(text.trim());
    is_vault(&p).then_some(p)
}

/// Remember a choice.
///
/// # Errors
/// If the path is not a vault, or the file cannot be written.
pub fn remember(p: &Path) -> Result<(), String> {
    if !is_vault(p) {
        return Err(format!(
            "{} does not look like a vault — expected a folder containing `08 Journal/Daily`",
            p.display()
        ));
    }
    let cfg = config_path().ok_or("cannot locate $HOME")?;
    if let Some(dir) = cfg.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&cfg, p.display().to_string()).map_err(|e| e.to_string())
}

/// Every vault path Obsidian has recorded, most recently opened first.
///
/// Asking the application that owns the vaults where they are beats guessing, and it is what makes
/// a first launch work for someone who has never configured MOKaji.
#[must_use]
pub fn obsidian_vaults() -> Vec<PathBuf> {
    let Some(home) = home() else {
        return Vec::new();
    };
    for c in [
        home.join("Library/Application Support/obsidian/obsidian.json"),
        home.join(".config/obsidian/obsidian.json"),
    ] {
        let Ok(text) = std::fs::read_to_string(&c) else {
            continue;
        };
        let vaults = parse_obsidian_json(&text);
        if !vaults.is_empty() {
            return vaults;
        }
    }
    Vec::new()
}

/// Parse Obsidian's vault registry. Split out so it is testable without Obsidian installed.
#[must_use]
pub fn parse_obsidian_json(text: &str) -> Vec<PathBuf> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let Some(vaults) = json.get("vaults").and_then(|v| v.as_object()) else {
        return Vec::new();
    };
    let mut rows: Vec<(i64, PathBuf)> = vaults
        .values()
        .filter_map(|v| {
            let path = v.get("path")?.as_str()?;
            let ts = v.get("ts").and_then(serde_json::Value::as_i64).unwrap_or(0);
            Some((ts, PathBuf::from(path)))
        })
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.0));
    rows.into_iter().map(|(_, p)| p).collect()
}

/// Breadth-first sweep for a vault, bounded and pruned.
///
/// Bounded because an unbounded walk of a home directory is how a launcher becomes a disk
/// thrasher. `Library` alone can hold hundreds of thousands of files and never a vault worth
/// finding.
#[must_use]
pub fn sweep(root: &Path, max_depth: usize) -> Option<PathBuf> {
    const SKIP: [&str; 8] = [
        "Library",
        "Applications",
        "node_modules",
        "target",
        ".git",
        "Pictures",
        "Music",
        "Movies",
    ];
    // Breadth-first, so a vault two levels down is found before one six levels down.
    let mut queue = std::collections::VecDeque::from([(root.to_path_buf(), 0usize)]);
    while let Some((dir, depth)) = queue.pop_front() {
        if is_vault(&dir) {
            return Some(dir);
        }
        if depth >= max_depth {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut children: Vec<PathBuf> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .filter(|p| {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                !name.starts_with('.') && !SKIP.contains(&name.as_str())
            })
            .collect();
        children.sort();
        for c in children {
            queue.push_back((c, depth + 1));
        }
    }
    None
}

/// Find the vault, in descending order of how much the answer can be trusted.
#[must_use]
pub fn discover() -> Option<PathBuf> {
    if let Some(p) = remembered() {
        return Some(p);
    }
    if let Some(p) = obsidian_vaults().into_iter().find(|p| is_vault(p)) {
        return Some(p);
    }
    if let Some(p) = std::env::var_os("MOKAJI_VAULT_PATH").map(PathBuf::from) {
        if is_vault(&p) {
            return Some(p);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for dir in cwd.ancestors() {
            if is_vault(dir) {
                return Some(dir.to_path_buf());
            }
            if let Ok(rd) = std::fs::read_dir(dir) {
                let mut children: Vec<PathBuf> = rd
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.is_dir())
                    .collect();
                children.sort();
                if let Some(found) = children.into_iter().find(|c| is_vault(c)) {
                    return Some(found);
                }
            }
        }
    }
    home().and_then(|h| sweep(&h, 6))
}

/// `~` is what a person types; it is not a path.
#[must_use]
pub fn expand_home(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), home()) {
        (Some(rest), Some(h)) => h.join(rest),
        _ => PathBuf::from(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mokaji-disc-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn make_vault(at: &Path) {
        std::fs::create_dir_all(at.join("08 Journal/Daily")).unwrap();
    }

    #[test]
    fn a_vault_is_the_layout_we_actually_read_not_just_any_folder() {
        let d = scratch("shape");
        assert!(!is_vault(&d), "an empty folder is not a vault");
        make_vault(&d);
        assert!(is_vault(&d));
        assert!(!is_vault(Path::new("")), "an unset path is not a vault");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn obsidians_registry_is_read_most_recent_first() {
        // The shape Obsidian actually writes.
        let json = r#"{"vaults":{
            "a1":{"path":"/tmp/older-vault","ts":1700000000000},
            "b2":{"path":"/tmp/newest-vault","ts":1800000000000},
            "c3":{"path":"/tmp/middle-vault","ts":1750000000000}
        }}"#;
        let got = parse_obsidian_json(json);
        assert_eq!(
            got,
            vec![
                PathBuf::from("/tmp/newest-vault"),
                PathBuf::from("/tmp/middle-vault"),
                PathBuf::from("/tmp/older-vault"),
            ],
            "the one you were last in is the one you probably mean"
        );
    }

    #[test]
    fn a_malformed_registry_yields_nothing_rather_than_panicking() {
        assert!(parse_obsidian_json("not json").is_empty());
        assert!(parse_obsidian_json("{}").is_empty());
        assert!(parse_obsidian_json(r#"{"vaults":{"a":{"ts":1}}}"#).is_empty());
    }

    #[test]
    fn the_sweep_finds_a_nested_vault_and_stops_at_its_depth_limit() {
        let root = scratch("sweep");
        make_vault(&root.join("Documents/dev/notes"));
        assert_eq!(sweep(&root, 6), Some(root.join("Documents/dev/notes")));
        assert_eq!(sweep(&root, 2), None, "bounded, so it cannot thrash a disk");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_sweep_prunes_the_folders_that_make_it_slow() {
        let root = scratch("prune");
        make_vault(&root.join("Library/Containers/notes"));
        make_vault(&root.join(".hidden/notes"));
        assert_eq!(
            sweep(&root, 6),
            None,
            "Library and dotfolders hold hundreds of thousands of files and never a vault worth finding"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_sweep_is_breadth_first_so_the_shallower_vault_wins() {
        let root = scratch("bfs");
        make_vault(&root.join("a/notes"));
        make_vault(&root.join("b/c/d/notes"));
        assert_eq!(sweep(&root, 6), Some(root.join("a/notes")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn remember_refuses_a_folder_that_is_not_a_vault() {
        let d = scratch("remember");
        let err = remember(&d).expect_err("must refuse");
        assert!(
            err.contains("08 Journal/Daily"),
            "the error says what was expected: {err}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
