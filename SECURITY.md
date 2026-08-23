# Security

MOKaji reads a personal vault, a calendar and an inbox, and listens to a microphone in a home
office. The threat model is not "a determined nation state" — it is **accidental disclosure**: a
credential committed to git, a log line containing a note, an audio buffer reaching a provider
because someone added a dependency without thinking.

So the controls here are **structural** wherever possible. Anything enforced only by good
intentions is treated as unenforced.

## The hard rule

**Nothing from the operator's vault ever enters this repository.** Not task text, note titles,
project names, chaser wording, screenshots, commit messages or code comments. Fixtures are
invented; the vault supplies shape, never content.

This is enforced rather than promised. `scripts/gen-private-terms.sh <vault>` reads the vault into
a gitignored denylist, and the pre-commit hook refuses any staged change containing one of its
phrases. Patterns cannot do this job on their own — a real task and an invented one are
indistinguishable by shape, which is exactly why the check has to know the actual content it is
guarding. The denylist never leaves the machine, and the hook refuses to commit it.

Run it once per clone, and again whenever the vault changes materially:

```sh
./scripts/gen-private-terms.sh /path/to/your/vault
```

## The invariants

| # | Invariant | How it is enforced, not merely intended |
|---|---|---|
| **PRIV-1** | Audio never leaves the device | The audio crate has **no network dependency**. It cannot acquire one without a `Cargo.toml` change, which the `network-boundary` CI job fails. Verified additionally by packet capture at M-2 and M-3 |
| **PRIV-5** | All outbound traffic goes through one client | Only `mokaji-net` may depend on a networking crate — same CI job. A kill switch disables it. A test asserts no other socket opens process-wide |
| **PRIV-4** | Credentials live in the macOS Keychain | Rust-side only. The renderer never receives a token or key. Nothing credential-shaped is readable from disk |
| **PRIV-2** | Vault, index, caches and audit log stay local | They leave only as explicit, consented, audit-logged model context |
| **PRIV-3** | No ambient capture | No screenshotting, no keylogging, no clipboard scraping. Only declared connectors |
| **SEC-1** | Default-deny Tauri capabilities | The renderer gets only the commands it needs |
| **SEC-2** | Model output is never trusted | Never `eval`'d, never shelled, never used to build a file path without allow-list validation |
| **SEC-3** | The A-7 connector shim is not an open door | Loopback bind only, plus a per-session shared secret |

## Secrets: where they live

**Nowhere in this repo. Nowhere on disk in plaintext. Ever.**

| Secret | Home | Keychain service | Account |
|---|---|---|---|
| Anthropic API key | macOS Keychain | `com.mokaji.provider.anthropic` | `api-key` |
| Google OAuth client id | Keychain (not secret, kept together) | `com.mokaji.oauth.google` | `client-id` |
| Google OAuth client secret | macOS Keychain | `com.mokaji.oauth.google` | `client-secret` |
| Google refresh token | macOS Keychain | `com.mokaji.oauth.google` | `refresh-token` |
| Work mailbox app password | macOS Keychain | `com.mokaji.mail.work` | `app-password` |
| Personal mailbox app password | macOS Keychain | `com.mokaji.mail.personal` | `app-password` |

The two mailboxes get **two services rather than two accounts under one service**, so revoking
work access is a single `delete` that cannot take personal with it. Mailbox addresses, hosts and
folder names are not secrets and live in `~/.config/mokaji/mail.json`; that file has no field a
password could be written into, and a test asserts it.
| A-7 shim session secret | Generated per session, memory only | — | — |

`.env.example` documents what configuration exists. There is no `.env` with real values — if you
find yourself creating one for a *credential*, that is the bug.

`mokaji-secrets` owns this. Its `Secret` newtype redacts itself in both `Debug` and `Display`, so
a credential cannot ride into a log line, a panic message or a bug report by accident — reading the
value requires calling `.expose()`, which is greppable and reviewable. **Platform caveat:** the
Keychain implementation compiles only on macOS, so ubuntu CI covers the trait, the constants and
the in-memory double but not `KeychainStore` itself. `cargo test` on the Mac is the only honest
coverage of that path.

Outbound requests carry a `Consent` token that cannot be defaulted into existence, and every one
is written to the audit log **before** it is sent — body verbatim, not a byte count, because
"something left" is not the promise PRIV-2 makes.

### OAuth (B-10)

Loopback redirect flow: a transient `localhost` listener on an ephemeral port, PKCE, state
parameter checked, listener closed the moment the code arrives. No client secret in the renderer,
no secret in a URL, no out-of-band paste flow.

Gmail read scopes are Google **restricted** scopes. v1 runs under a personal/testing OAuth client,
which DEC-2 makes acceptable for exactly one user. App verification is a hard prerequisite for a
second user (RISK-8) — it is a line item in the open-sourcing backlog, not an afterthought.

Request the narrowest scope that works, and re-check on every scope change:
`gmail.readonly` and `calendar.readonly` in v1. **B-9: no sending, no auto-replies, no archiving,
no label mutation.**

## Writing to the vault (REL-1)

The one **Severe**-impact risk in the table. Three controls, all mandatory:

1. **B-3** — surgical line edits, never file rewrites, preceded by a content-hash check that
   aborts on drift.
2. **B-4** — dry-run is the **default**. It prints the diff and applies nothing until explicitly
   disabled in config.
3. **B-5** — a snapshot before the first write of each session (git commit if the vault is a repo,
   else a timestamped copy).

## Logging

Log **shapes, not contents**: record counts, connector ids, stages, durations, error kinds. Never
task text, note bodies, email subjects, attendee names or transcripts. A stack trace that embeds a
record is a disclosure bug — treat it as one.

The audit log is the deliberate exception: it exists to show byte-for-byte what left the machine,
it is local-only, and it is purgeable on demand (FR-E).

## Dependencies

- `cargo deny` / `cargo audit` before each milestone closes.
- **A new networking dependency is a security review, not a chore.** The `network-boundary` CI job
  exists to make adding one loud.
- Model assets are licence-audited before bundling — see `LICENSING.md`. RISK-3 is a licence risk,
  not a security one, but the gate is the same gate.

## Reporting

Personal project, single user. If you are reading this after it was open-sourced, mail the address
in the repo metadata rather than opening a public issue.
