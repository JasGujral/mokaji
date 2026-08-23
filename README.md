# MOKaji

**An always-on, locally-running command center for your desk that you talk to.**

It reads your vault, calendar and email through one standardized connector layer, and files,
answers and nudges — without ever sending your voice off the machine.

> Local data, local senses, opt-in cloud cognition.

---

## Where the thinking lives

This repo is **code only**. The design work behind it — landscape research, philosophy,
requirements, design handoff — lives in a **private** folder alongside it and is deliberately not
committed, because it is interleaved with one person's actual life. The operative parts of it are
restated in `CLAUDE.md` and `CONTRIBUTING.md`, so nothing you need to contribute is missing.

If you are reading this from a clone, the `../` paths below will not resolve for you. That is
expected.

| Document | What it is |
|---|---|
| `../REQUIREMENTS — MOKaji v1.md` | **Authoritative.** Locked decisions, functional requirements, the standard models, the interface contracts, milestones. When anything conflicts, this wins |
| `../STATUS.md` | Current milestone and the live task list |
| `../MOKaji — Philosophy (draft).md` | What we believe and why |
| `../research/01 — Competitive Landscape & Positioning.md` | Why this category keeps dying |
| `../research/Inspiration — Reference Architectures.md` | OpenBB as the reference architecture |
| `../design_handoff_command_center/` | What it looks like, plus a runnable prototype |

Requirement IDs (`A-4`, `PRIV-5`, `X-10`, …) are cited throughout the source. They point at
`REQUIREMENTS`. Keep them accurate — they are how the code stays traceable to the spec.

## The shape

MOKaji is **a connector platform, not an app**. The moat is the standardization layer: the
assembled personal context, not the model. So the contracts get built before the panels.

```
                 ┌──────────── Deck (React) ────────────┐
                 │  panels.json → panels → decks.json   │   thin surface (5 panels in v1)
                 └───────────────────┬──────────────────┘
                                     │  Tauri commands (default-deny, SEC-1)
                 ┌───────────────────▼──────────────────┐
                 │  mokaji-core — THE CONTRACT          │   deep platform
                 │  standard models · Connector (TET)   │
                 │  registry · router (dedupe + sort)   │
                 │  ModelProvider (local ⇄ cloud)       │
                 └───┬───────────┬───────────┬──────────┘
                     │           │           │
                  vault        gcal        gmail          … + the A-7 process/HTTP shim,
                (connector #1, not a special case)          which can wrap any MCP server
```

Every connector implements **TET** — `transform_query` → `extract` → `transform_data`. A Google
Calendar `summary` and an `.ics` `SUMMARY` both become `title`. One concept, one name, everywhere.

## Layout

```
crates/
  mokaji-core/             standard models, Connector trait, registry, router, metrics
  mokaji-net/              the SINGLE outbound HTTP chokepoint (PRIV-5) — nothing else opens a socket
  mokaji-secrets/          credentials, macOS Keychain only, Rust-side only (PRIV-4)
  mokaji-connector-vault/  connector #1 — reads an Obsidian vault
  mokaji-connectors-fake/  fake connectors that prove the contract (the M-0 exit criterion)
  mokaji-cli/              `mokaji` — a terminal readout of the vault, read-only
docs/adr/                  architecture decision records
```

## Install

```sh
./scripts/install.sh            # builds and puts `mokaji` on your PATH (~/.local/bin)
./scripts/install.sh --app      # also builds the desktop app into /Applications (macOS, from M-1)
```

Set `MOKAJI_VAULT_PATH` in your shell profile and you can drop the `--vault` flag.

## Try it

There is no app window yet — that arrives with the Deck at M-1. What exists is a terminal readout,
which is the fastest honest test of the data layer:

```sh
mokaji                      # Reactor Core readout
mokaji tasks                # open tasks, in the Deck's order
mokaji chasers              # waiting-on and need-to-nudge
mokaji vitals               # today's tracker metrics
mokaji health               # connector health
```

```
  ⚛  REACTOR CORE — 64%  STEADY

  Focus clarity          ████████████████░░░░  84%
  Momentum               ██████░░░░░░░░░░░░░░  33%  (2/6 cleared today)
  Bandwidth              ███████████████░░░░░  72%

  Open tasks             4
  Urgent (due ≤ today)   1
  Chasers overdue        0
  Calendar load          0%  (no calendar until M-5)
```

*(Numbers above are from the invented test fixture, not from anyone's vault — see the hard rule
below.)*

Read-only, opens no socket, writes nothing. Omit `--vault` and it looks upward from the current
directory for a folder containing `08 Journal/Daily` (H-3: an empty config still boots).

### The hard rule

**Nothing from the operator's vault ever enters this repository** — not task text, note titles,
project names, chaser wording, screenshots, or commit messages. Every fixture here is invented; the
recurring lighthouse station in the test data exists precisely because nobody could mistake it for
anyone's real notes.

It is enforced rather than promised. `./scripts/gen-private-terms.sh <vault>` reads the vault into a
**gitignored** denylist, and the pre-commit hook refuses any staged change containing one of its
phrases. Patterns alone cannot do this — a real task and an invented one are indistinguishable by
shape — so the check has to know the content it is guarding, and that knowledge never leaves the
machine.

**Vault conventions it reads.** Due dates the Tasks-plugin way — `- [ ] Call the accountant 📅
2026-08-24`. Completions carry `✅ <date>`, and only *today's* count toward momentum. Blank `- [ ]`
placeholders are skipped. Chasers are tasks tagged `#waiting` / `#nudge`, with `#overdue` when
slipped. Fenced code blocks are never tasks — documentation that counts as data is a trap, and one
vault's `Chasers.md` examples were quietly costing it 10 points of Focus Clarity.

`reqwest` appears in exactly one `Cargo.toml` in this workspace, and CI fails if that stops being
true. That single fact is what makes PRIV-1 — *audio never leaves the device* — a structural
property rather than a promise: the audio crate cannot acquire the ability to transmit without a
visible dependency change that fails the build.

Not yet present, added as their milestones start: `src-tauri/` and `src/` (M-0), the vault
connector (M-1), `mokaji-audio` (M-2).

## Milestones

| # | | Exit criterion (short) |
|---|---|---|
| **M-0** | Contracts & skeleton | ✅ **exit criterion met** — two fake connectors round-trip through TET; router dedupes and sorts deterministically; PRIV-5 passes. **Nothing on screen — that is correct** |
| **M-1** | Vault + Deck v0 | 🟡 connector + metrics done and matching; Deck still to come |
| **M-2** | Voice v0 — push-to-talk | A spoken task lands in the daily note in < 5 s **with the network cable out** |
| **M-3** | Voice v1 — always-on | Wake word → overlay ≤ 300 ms, idle CPU ≤ 2%, no non-commercial model shipped |
| **M-4** | The brain | "Plan my day" returns a **cited** plan; the audit log shows byte-for-byte what left |
| **M-5** | Senses | A three-connector morning briefing, spoken and not dismissed, ≥4 of 5 weekdays |

**Cut milestones from the end, never depth from M-0.**

## The lines we don't cross

- **Audio never leaves the device.** No flag, config or provider can transmit microphone bytes.
  Enforced structurally: the audio crate has no network dependency.
- **All outbound traffic goes through `mokaji-net`.** One client, one kill switch, and a test that
  asserts no other socket is opened process-wide.
- **Credentials live in the macOS Keychain**, accessed Rust-side. The renderer never sees a token.
- **Vault writes are surgical and hash-guarded**, dry-run by default, snapshot before first write.
- **No ambient capture** — no screenshots, no keylogging, no clipboard scraping.
- **SQLite is a cache, never a source of truth.** Rebuildable from sources at any time.

## If you are cloning this

Built for one person on one Apple Silicon Mac (DEC-2), and honest about it. It will not do anything
useful for you without your own vault and your own credentials.

- **Bring your own Google OAuth client.** Gmail read scopes are Google *restricted* scopes. A
  personal/testing client is fine for a single user; app verification is a prerequisite for
  anything wider and has not been done. **No credentials ship in this repo, and none ever will.**
- **No personal data ships either.** Every fixture is synthetic. `scripts/check-personal-data.sh`
  runs on every commit and every CI run to keep it that way — absolute home paths, personal email
  addresses and unmarked fixtures all fail the build.
- **Licence: Apache-2.0** — permissive, with a patent grant, because this is a platform other
  people are meant to write connectors against. See `LICENSE` and `LICENSING.md`.

## Build

```sh
./scripts/install-hooks.sh          # once per clone — see below, it is not optional
cargo test                          # M-0 contract tests
cargo test -- --include-ignored     # + the M-0 exit checklist (currently red by design)
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

Targets Apple Silicon macOS. Cross-platform is post-v1.

## Secrets

**No credential ever enters this repo or lands on disk in plaintext.** They live in the macOS
Keychain, Rust-side only — `SECURITY.md` has the service/account names and the OAuth flow.
`.env.example` documents configuration; it holds no secrets and neither should your `.env`.

Three layers keep that honest, rather than hoping:

- `.githooks/pre-commit` — refuses credential-shaped filenames and contents, and refuses a
  networking dependency added outside `mokaji-net`. Install it with `./scripts/install-hooks.sh`
- `scripts/scan-secrets.sh` in CI, over **full history**, not just the tip — a secret committed
  and later deleted is still published. In-repo rather than a downloaded binary: the previous
  version pinned a gitleaks release tag that was a guess, and a wrong pin fails the build for a
  reason unrelated to the code, which only teaches people to ignore red CI
- the `network-boundary` CI job, which is what makes "audio never leaves the device" structural
  rather than aspirational

If the hook blocks something you meant to do, `--no-verify` exists — but a networking dependency
outside `mokaji-net` is a security review, and the commit body should say why.

## Releases

Every milestone is a **patch** release; **0.1.0 is the first real release**, cut when all six
milestones are done.

| Version | |
|---|---|
| `0.0.1` | M-0 — contracts, the outbound chokepoint, Keychain, the vault connector, the CLI |
| `0.0.2` | M-1 — the app window |
| `0.0.3` | M-2 — push-to-talk voice, fully offline |
| `0.0.4` / `0.0.5` / `0.0.6` | M-3 / M-4 / M-5 |
| **`0.1.0`** | **first release** |

Everything before `0.1.0` is development. Expect breaking changes to the connector and panel
contracts until the interfaces stop moving — which is the same reason the outbound MCP server
(Tier 5) is deliberately unscheduled.

## Status

**M-0 — exit criterion met.** `cargo test` is green with zero `#[ignore]`s remaining in
`crates/mokaji-connectors-fake/tests/m0_exit.rs`:

- **A-2** TET round-trips `Task` and `Event`, and every error names the stage it came from
- **A-4** the same standup arriving as `summary` from one source and `SUMMARY` from another
  collapses to one record; source precedence decides which spelling survives
- **A-5** events sort by `start` then `title`, tasks by `due` **nulls last** then `text`, and the
  order does not depend on connector registration order
- **A-6** a connector failing mid-`extract` produces a badge and a failure entry while the healthy
  connector's records still arrive
- **A-12** an envelope key this build has never heard of survives serialize → deserialize; a major
  version mismatch fails loudly with a migration hint
- **PRIV-5** a full router pass opens zero sockets, and no crate outside `mokaji-net` may even name
  a networking dependency

Since then, also in M-0: the real HTTP client behind the kill switch — every request carries an
unforgeable `Consent` token and is written to the audit log **before** it is sent, body verbatim,
so a crash mid-flight still leaves evidence; and `mokaji-secrets`, where a `Secret` newtype
redacts itself in `Debug` and `Display`, because the realistic way a credential escapes is a
`dbg!` in a bug report, not a burglar.

Still open inside M-0's contents: the Tauri + Vite + React scaffold and design tokens as CSS vars.

See `../STATUS.md` for the milestone board.
