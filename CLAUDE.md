# CLAUDE.md — mokaji

The **code** repo. Docs, research and design live one level up in the `Jarvis` folder and are not
committed here. See `README.md` for the map.

## THE HARD RULE — read this before anything else

**Nothing from the operator's vault ever enters this repository.** Not a task, not a note title,
not a project name, not a chaser's wording, not a screenshot, not a commit message, not a log
line, not a code comment "for context". The vault supplies **shape** — that a daily note has
frontmatter, that a task line looks like `- [ ] x 📅 date` — and never **content**.

This repo is public and MOKaji reads one person's entire life. Those two facts are only compatible
because of this rule, and the rule has no exceptions for "it's just an example" or "it's only a
test fixture". Every fixture in this repo is invented from scratch; the recurring lighthouse
station in the test data exists precisely because nobody could mistake it for anyone's real notes.

Enforced, not merely stated:

- `scripts/gen-private-terms.sh <vault>` writes a **gitignored** denylist of every phrase in the
  vault. `.githooks/pre-commit` greps every staged change against it and refuses the commit on a
  match — pattern-matching alone cannot do this job, because a real task looks exactly like an
  invented one.
- The hook also refuses to commit `.private-terms` itself. The denylist is the disclosure it
  exists to prevent.
- `scripts/check-personal-data.sh` runs per-commit and in CI: absolute home paths, personal email
  addresses, phone-shaped strings, unmarked fixtures.
- If the hook fires, **it is right**. Invent different fixture content. Do not weaken the check,
  and do not `--no-verify` past it.

## The other rule

`../REQUIREMENTS — MOKaji v1.md` is **authoritative**. It outranks `../design_handoff_command_center/`,
`../ARCHITECTURE — Obsidian powering Jarvis.md` and `../MOKaji — Philosophy (draft).md`. Its §3
records all 14 known conflicts and how each was resolved — **read §3 before "fixing" something that
looks wrong in an older doc.** Several of those older statements are deliberately superseded.

Before designing anything new, check it against §1 (locked decisions) and §12 (anti-requirements).

## Locked — reopening one is a change request, not a discussion

- **DEC-1** v1 is a voice-first slice: 5 panels (Console, Reactor Core, Briefing, Task Queue, Agenda)
- **DEC-2** Audience is one person on one Mac, life mode. Work mode deferred, architecturally reserved
- **DEC-3** Hybrid model router from day one, local + cloud behind `ModelProvider`
- **DEC-4/5/6** Full connector platform · Rust-native connectors · plus a process/HTTP shim escape hatch
- **DEC-8** Fully local voice from M-2: whisper.cpp Metal — **not** macOS `Speech.framework`

## Already rejected — do not re-propose

- **Web Speech API / `SFSpeechRecognizer`** for voice (X-4, X-9). Both stream audio off-device.
  `SFSpeechRecognizer` degrades to the server path *silently* when the on-device asset is missing
- **MCP as the inbound connector transport** (X-3). Native Rust in, MCP survives only via the A-7
  shim and, post-v1, as an outbound server (Tier 5)
- **`obsidian://` deep links as the read path** (X-6). Filesystem reads from M-1; deep links are UX
- **Regexing free text for `urgent`** (X-10). Typed predicate on `due`; parsing happens at the
  connector boundary
- **All-time `done` for momentum** (X-11). `done` means done **today**
- **`localStorage`/Zustand as canonical state** (X-14). Only *UI* state persists client-side
- **Dedupe by `(source, source_ref)`** (A-4). It cannot work — sources differ by construction

## House rules

- **Cite requirement IDs in code and commits.** `// A-4:` , `feat(router): dedupe on content key (A-4)`.
  If a change has no ID, say why in the commit body
- **`lower_snake_case`** for all Rust fields and record keys, manifest files included (`grid_data`,
  `min_w` — not camelCase). One concept, one name, across every connector
- **All instants UTC.** "Today" means the machine's **local calendar day**, rolling at local midnight
- **Connector-specific data goes in `raw`.** Never leak a provider field into a typed model
- **No **M** requirement may depend on an **S**, **C** or **W** one.** Keep the rule when amending
- **Contracts are written as public APIs** even though there is one user (RISK-9) — open-sourcing
  should be a packaging job, not a rewrite

## Structural guarantees — these are enforced, not aspirational

- Only `mokaji-net` may depend on a networking crate. A CI lint asserts it. This is what makes
  PRIV-1 ("audio never leaves") true by construction rather than by good intentions
- Every TET error names its stage (A-2)
- Every record and manifest carries a `schema_version` (A-12)
- Vault writes: hash-check → abort on drift, dry-run by default, snapshot before the session's
  first write (B-3/B-4/B-5). This is the one **Severe**-impact risk in the table

## Secrets — non-negotiable

Credentials live in the **macOS Keychain**, Rust-side only (PRIV-4). Never in the repo, never in a
`.env`, never in the renderer, never in a log line, never in a commit — not even temporarily, not
even in a branch you plan to rebase. `SECURITY.md` holds the service/account names and the OAuth
loopback flow.

`.githooks/pre-commit` blocks credential-shaped files and content, and blocks networking
dependencies added outside `mokaji-net`. Run `./scripts/install-hooks.sh` after cloning. Do not
weaken the hook to get a commit through — if it fires, it is usually right.

Log **shapes, not contents**: counts, connector ids, stages, durations, error kinds. Never task
text, note bodies, email subjects, attendee names or transcripts.

## Working rule

Build milestone by milestone. Each ends with a demo you can use. **Cut from the end, never depth
from M-0.** Never ship the prototype as-is — re-implement properly. Update `../STATUS.md` after
meaningful progress.
