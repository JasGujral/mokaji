# Licensing

**Repo licence: Apache-2.0.** Decided 2026-08-23, closing OQ-5 early — the repo went public
sooner than planned, and a public repo without a licence grants nobody anything.

Apache-2.0 over MIT for the explicit **patent grant**: MOKaji is a platform other people write
connectors against, and the grant is what makes that comfortable for anyone with a legal
department. Explicitly *not* AGPLv3 (adoption tax — the one thing deliberately not copied from
OpenBB, per the inspiration research).

Files: `LICENSE` (full text), `NOTICE` (attribution), `license = "Apache-2.0"` in the workspace
`Cargo.toml`.

## The trap this file exists to prevent — RISK-3

Shipping one non-commercially-licensed model asset would poison a permissive open-source licence
for the whole project. Two known landmines:

| Asset | Problem | Plan |
|---|---|---|
| **openWakeWord** | Code is Apache-2.0, but the **pretrained models are CC BY-NC-SA** | **Train our own** wake-word model (V-3). Blocked on OQ-2 (the phrase) — decide by M-2 exit, because training lead time sits inside M-3 |
| **Piper TTS** | Maintained release is **GPL-3.0** via espeak-ng | **Kokoro-82M** with a permissively-licensed phonemizer (V-4) |

## The gate

**A licence audit of every shipped asset is an exit gate on M-3.** No model, voice, font or
dataset enters the bundle without a row in the table below.

| Asset | Version | Licence | Commercial OK | Verified |
|---|---|---|---|---|
| _(none yet)_ | | | | |

Fonts are vendored (C-9, REL-3 — no network fetch to render the UI), so they need rows here too.
