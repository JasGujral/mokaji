# mokata constitution

The governing articles for this project. Committed, reviewable, and editable. mokata
reads this as the standing contract for how work is done here.

## Article 1 — Human-gate every durable write
Every write to code, memory, or config is staged and approved by a human. Nothing is
written silently or autonomously. (Inviolable — cannot be configured away.)

## Article 2 — Local-first, private by default
Nothing leaves this machine unless a human explicitly wires an external service. No
telemetry. (Inviolable — cannot be configured away.)

## Article 3 — Spec before code; prove completeness
No implementation before an approved spec whose acceptance criteria each map to a
test. RED before GREEN. Correctness is demonstrated with evidence, not asserted.

## Article 4 — Degrade, never break
When a wired tool is absent, fall back to a declared alternative. A missing optional
dependency is never a hard failure.

## Article 5 — Review every decision
Every gate decision, tool call, and durable write is auditable. A human can
reconstruct and walk back any choice the system made.
