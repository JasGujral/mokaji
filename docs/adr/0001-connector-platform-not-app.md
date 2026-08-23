# ADR 0001 — MOKaji is a connector platform, not an app

- **Status:** accepted
- **Date:** 2026-08-13 (recorded here 2026-08-23)
- **Source:** `../REQUIREMENTS — MOKaji v1.md` DEC-4/5/6 · `../research/Inspiration — Reference Architectures.md`

## Context

DEC-1 chose the *smallest* useful surface — five panels, voice-first. DEC-4 chose the *largest*
architecture — standard models, a connector SDK, a declarative panel manifest. These pull opposite
ways, and the tension is deliberate: **thin surface, deep platform.**

The reason is what the moat actually is. The model is a commodity and gets better without us. The
assembled personal context does not. So the standardization layer *is* the product, and it gets
built before the panels that consume it.

OpenBB was analysed as the reference architecture: an empty core with entry-point-discovered
extensions, standard typed models as the contract, and a `transform_query → extract →
transform_data` shape per provider.

## Decision

1. **Empty core + discovered connectors.** The vault is connector #1, not a special case.
2. **Standard models are the contract** (§5). Connector-specific data lives in `raw`, never in a
   typed field. One concept, one name, everywhere.
3. **Connectors implement TET**, with each stage separately testable and every error naming its stage.
4. **The Deck renders from a declarative `panels.json`** from M-1 — retrofitting a manifest later is
   expensive. Life/Work modes become layout manifests (`decks.json`), not code paths.
5. **Connectors are Rust-native** so credentials never leave the native side — with a process/HTTP
   shim (A-7) as the escape hatch, which can also wrap an existing MCP server.

## Consequences

- **Good:** every Tier 2–5 connector is purely additive; contracts written as public APIs mean
  open-sourcing is a packaging job; credentials never reach the renderer; work mode is a manifest.
- **Bad:** v1 is substantial for one user, and M-0 ships with nothing on screen. Rust-only
  connectors make each new source expensive (RISK-2) — the A-7 shim was promoted to **M** precisely
  because it is the sole mitigation.
- **Enforced by:** A-12 schema versioning, without which "purely additive" is an unenforceable claim.

## Explicitly not copied from OpenBB

AGPLv3 (adoption tax — see `LICENSING.md`) · a cloud-hosted UI as the flagship (MOKaji inverts
this: the local HUD is the flagship) · a user-facing build/rebuild step · breadth-first surface
expansion · provider count as a success metric.
