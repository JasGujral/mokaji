# Development is governed by mokata

This repo is developed under [mokata](https://github.com/JasGujral) — the same author's agent
framework — and that is deliberate on both sides. MOKaji gets a governance layer it would
otherwise have to invent; mokata gets a real, non-toy project to be tested against. Bugs found
here go back upstream.

## Setup

```sh
mokata init          # in this directory, once — profile `full`
mokata doctor        # confirm the capability wiring resolved
```

**What is committed:** `.mokata/manifest.json` and `.mokata/constitution.md`. They are the
reviewable contract, which is the whole point of mokata writing them to disk rather than holding
them in a config service.

**What is not:**

- `.mokata/temp_local/` — pipeline state, resume checkpoints, the freshness index, caches, the
  SQLite memory store and the audit ledger. The memory store in particular can hold notes about
  this repo's contents, and this repo is public.
- `.claude/` and `.mcp.json` — the harness wiring. `mokata init` writes absolute paths into these
  (`/opt/homebrew/bin/mokata-hook`, plus a version-pinned `site-packages` directory). Committing
  them would hand every other contributor a config pointing at binaries that do not exist on their
  machine, and would break CI. Run `mokata init` yourself and get wiring that matches your install.

## Why the two fit

mokata's articles and MOKaji's requirements were written independently and land in the same place.
That is the reason to use it here rather than merely a convenience:

| mokata article | MOKaji requirement it lines up with |
|---|---|
| **1 — Human-gate every durable write.** Nothing written silently or autonomously | **B-4**: vault dry-run is the *default*; every intended mutation prints as a diff and applies nothing until explicitly enabled. Also how this repo is developed — an agent proposes, a human commits |
| **2 — Local-first, private by default. No telemetry** | **PRIV-1/2/5**: audio never leaves the device, the vault and index never leave, all outbound traffic through one auditable chokepoint with a kill switch |
| **3 — Spec before code; acceptance criteria map to tests. RED before GREEN** | `crates/mokaji-connectors-fake/tests/m0_exit.rs` *is* this article: the milestone's exit criteria written as failing tests before the implementation exists. The milestone is done when they go green |
| **4 — Degrade, never break. A missing optional dependency is never a hard failure** | **A-6**: one dead connector degrades only its panels and raises a health badge, never blanks the Deck. **REL-2**: offline is a first-class mode. **REL-5**: a connector panic never takes the process down |
| **5 — Review every decision; a human can walk back any choice** | **FR-E**: the consent gate plus an append-only audit log showing byte-for-byte what left the machine, purgeable on demand |

## Working notes

- **Article 3 is the one that shapes the day-to-day here.** Write the exit criteria as `#[ignore]`d
  tests first, then delete the `#[ignore]` as each becomes true. A milestone is finished when its
  exit file runs green with `--include-ignored` and no `#[ignore]` remains.
- **Article 4 cuts both ways.** mokata itself degrades — `code_graph` falls back to ripgrep, then
  grep. If a fallback fires unexpectedly, that is a finding worth reporting upstream, not a
  workaround to paper over.
- **Log mokata bugs as you hit them.** That is half the point of developing here. A note in the
  commit body is enough; move it upstream when it is reproducible.

## Findings so far

Dogfooding is only worth anything if the findings get written down. Observed on `mokata 0.0.18`,
profile `full`, during first-run setup of this repo on 2026-08-23.

| # | Observation | Why it matters |
|---|---|---|
| **1** | Setup reported `✓ semantic memory model installed (mokata[embeddings])`, then `mokata doctor` in the same session reported `retrieval stack • semantic: off (no embedder configured — ranking is lexical only)` | The install succeeded but nothing wired it up, so the feature the user just opted into is silently inactive. Two subsystems disagreeing about the same fact is the kind of thing that erodes trust in every other ✓ on the screen. Looks like a wiring step missing after install, not a failed download |
| **2** | `doctor` raised `role-conflict: capability 'code_graph' claimed by ['code-review-graph', 'serena', 'ast', 'ripgrep', 'grep']` — but setup had just reported code-review-graph and serena as *detected but NOT installed* | A capability claim from a tool that is not installed is not a conflict. Warning about it turns a clean setup into one with a finding, which trains people to ignore findings |
| **3** | `mokata init` wrote machine-absolute paths into `.claude/settings.json` and `.mcp.json` inside the repo | These are repo-scoped files that people commit by reflex. On a shared or public repo they break every other machine. Emitting a bare `mokata-hook` (resolved via `PATH`), or writing a `.gitignore` entry alongside them, would remove the footgun. Worked around here by gitignoring both |
| **4** | The embedded AST floor answers Python repos; this is Rust, so `code_graph` degrades to `ripgrep` | Not a bug — Article 4 working as designed, and `doctor` says so clearly. Worth recording that structural queries here are lexical until a graph tool is wired |
| **5** | `doctor` flags that the pip package and the Claude Code plugin ship separate servers, so skills may appear twice | Self-reported and clearly explained. Noted so it is not re-diagnosed later |

Finding 1 is the one worth fixing first: it is a correctness bug in what the tool *tells* you,
which is worse than a bug in what it does.