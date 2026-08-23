# Contributing

MOKaji is built for one person on one Mac right now (DEC-2), but every contract is written as a
public API so that stops being true without a rewrite. Contributions are welcome; the rules below
exist because of what this software touches.

## Before you write code

Run `./scripts/install-hooks.sh`. It is not optional — the hooks are the only thing standing
between a distracted commit and a permanent public record of someone's private life.

```sh
./scripts/install-hooks.sh
./scripts/gen-private-terms.sh /path/to/your/vault   # arms the vault-content check
cargo test
cargo test -- --include-ignored    # the current milestone's exit checklist; red is expected
```

## The two rules that are not negotiable

**1. Nothing personal, ever — and nothing from the operator's vault, at all.** No real names, email addresses, task text, note bodies, calendar
titles, message subjects or absolute home paths — not in source, not in tests, not in docs, not in
a commit message, not in a screenshot. Fixtures are synthetic, use `example.com`, and carry the
word "synthetic" in their first five lines. `scripts/check-personal-data.sh` enforces this and runs
on every commit and every CI run.

**2. No credentials, ever.** They live in the macOS Keychain — see `SECURITY.md`. There is no
`.env` with real values; if you are creating one, that is the bug. `gitleaks` scans full history
on every push.

## Design rules

The authoritative spec is a private design document, so the operative summary lives in `CLAUDE.md`
— read it before proposing architecture. The short version:

- Standard models are the contract. Connector-specific data goes in `raw`, never in a typed field.
- One concept, one name, across every connector. `summary` and `SUMMARY` both become `title`.
- Connectors implement TET, each stage separately testable, every error naming its stage.
- Adding a networking dependency outside `mokaji-net` is a **security review**, not a chore. The
  hook and CI will both stop you. If it is genuinely needed, say why in the commit body.
- All instants UTC; "today" means the local calendar day.

## Commits

Reference the requirement id where one applies: `feat(router): dedupe on content key (A-4)`.
If a change has no id, explain the why in the body. Milestones ship as coherent units — see the
milestone table in `README.md`.

## Running it yourself

You will need your own Google OAuth client. Gmail read scopes are Google *restricted* scopes:
a personal/testing client works for you as a single user, but distributing a verified app is a
prerequisite for anything wider, and that verification is not done. Nothing in this repo ships
credentials, and nothing ever will.
