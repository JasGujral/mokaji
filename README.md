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
./scripts/install-hooks.sh                      # once per clone — see the hard rule below
./scripts/gen-private-terms.sh /path/to/vault   # arms the vault-content check
./scripts/install.sh                            # the `mokaji` CLI, into ~/.local/bin
./scripts/install.sh --app                      # …plus the desktop app, into /Applications (macOS)
```

Put `export MOKAJI_VAULT_PATH="<path-to-your-vault>"` in your shell profile and both the CLI and
the app find it without being told.

## The app

`open -a MOKaji` — a bento Deck of glass panels over a dark field: Reactor Core, Daily Briefing,
Task Queue, Agenda, Command Console. Panels are declared in `src/panels.json` and placed by a
skyline bin-packer; **positions are computed, never stored**, which is what makes a deck a list of
panel ids rather than a saved pixel layout. Only *UI* state persists client-side (C-11) — the
numbers always come from the vault.

Panels tell you what they cannot do rather than showing nothing. An empty panel that explains
itself is honest; one that silently shows nothing is indistinguishable from a broken connector —
which is the same reason the Core shows **NO DATA** rather than the 100% OPTIMAL that correct
arithmetic over zero records would otherwise produce.

**⌥Space** opens the command bar from anywhere on the machine, whether or not MOKaji has focus;
⌘K does the same inside the window. It parses as you type and states what it will do before it
does it, which is the safety net a mis-transcription needs. Everything it can do, the Console can
do, because both call the same parser in `mokaji-core` — CON-3 exists so a command cannot behave
differently typed and spoken.

```
add a task to call the harbour office tomorrow
done accountant                 open the tide survey
brief me                        quiet
show agenda                     hide the task queue panel
hide                            come back
```

The **Daily Briefing** is assembled from every configured sense and reads itself out. Each line
carries a count you can click to see the record ids behind it: a briefing whose claims cannot be
traced is indistinguishable from a plausible invention, and tracing is the whole argument.

## The CLI

Still the fastest honest test of the data layer, and the debugging path when a panel disagrees
with Obsidian:

```sh
mokaji                      # Reactor Core readout
mokaji tasks                # open tasks, in the Deck's order
mokaji chasers              # waiting-on and need-to-nudge
mokaji vitals               # today's tracker metrics
mokaji health               # connector health
```

### Senses

| | What it needs | What it gives |
|---|---|---|
| **Vault** | a folder containing `08 Journal/Daily` | tasks, chasers, notes, metrics |
| **Calendar** | a folder of `.ics` files — `~/Library/Calendars` is the one macOS already maintains for every account in Internet Accounts | events, with A-4 collapsing the same meeting arriving from two calendars |
| **Mail** | an IMAP app password per mailbox, in the Keychain | messages, **read and classify only** |

Mail opens the mailbox with `EXAMINE` rather than `SELECT` and fetches headers, never bodies. It
cannot send, reply, archive, delete or mark anything read — B-9 written as an absence of code
rather than a setting. Whether a message *needs action* is decided by structural signals (the
server's `\Seen` flag, whether the sender is an unattended address, whether it is you) and never
by the words in a subject line: those words are chosen by the sender, and the senders most fluent
in urgency have the least claim on your attention. That is X-10's lesson, applied where it is
easiest to forget.

```
  ⚛  REACTOR CORE — 64%  STEADY

  Focus clarity          ████████████████░░░░  84%
  Momentum               ██████░░░░░░░░░░░░░░  33%  (2/6 cleared today)
  Bandwidth              ███████████████░░░░░  72%

  Open tasks             4
  Urgent (due ≤ today)   1
  Chasers overdue        0
  Calendar load          0%  (until a calendar folder is configured)
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

`mokaji-audio` is the one crate still to come (M-3). It will have **no network dependency**, which
is what makes "audio never leaves the device" a fact about the build rather than a promise.

## Milestones

| # | | Exit criterion (short) |
|---|---|---|
| **M-0** | Contracts & skeleton | ✅ **exit criterion met** — two fake connectors round-trip through TET; router dedupes and sorts deterministically; PRIV-5 passes. **Nothing on screen — that is correct** |
| **M-1** | Vault + Deck v0 | ✅ the app window — real vault numbers, panels from a manifest |
| **M-2** | Voice v0 — push-to-talk | ✅ the command surface — ⌥Space anywhere, one parser for typed and spoken, a task lands in the daily note **with the network cable out**. Dictation itself waits on M-3's local engine; the field is the interim, and says so |
| **M-3** | Voice v1 — always-on | ⏳ **blocked on hardware, not design.** Wake word ("Hey Kaji") → overlay ≤ 300 ms, idle CPU ≤ 2%, no non-commercial model shipped. The ring buffer, the wake-word model and local STT are CoreAudio/Metal builds that compile only on the target Mac |
| **M-4** | The brain | Contracts done (`provider.rs`: tiers, policy, consent, citations, E-2 pinning). Needs a local runtime chosen and wired — "plan my day" returns a **cited** plan; the audit log shows byte-for-byte what left |
| **M-5** | Senses | ✅ vault + calendar + mail, assembled into a briefing that cites every claim and reads itself out. Whether it is **not dismissed ≥4 of 5 weekdays** is a fact about use, not about code — the app reports which senses answered so the criterion can actually be judged |

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
| `0.0.3` | the Console writes — confirm, apply, undo; the vault watcher |
| `0.0.4` | M-2 — the command bar; ⌥Space, one parser, the Core as a summary |
| `0.0.5` | M-5 — the third sense: mail over IMAP, and the briefing that cites itself |
| `0.0.6` | M-3 / M-4 — the wake word and the local model, both of which need the target Mac |
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
