#!/usr/bin/env bash
# Secret scan over the working tree AND the full history.
#
# This file is the single source of truth for the patterns. There was a .gitleaks.toml carrying a
# second copy; two copies of the same rules drift, and its documented placeholder values
# ("GOCSPX-xxxx…") were themselves flagged by the history scan. Deleted — one place, no drift.
#
# In-repo on purpose. The previous version downloaded a pinned gitleaks release, and the pin was a
# guess: a tag that does not exist fails the build for a reason unrelated to the code, which is the
# worst kind of red CI. This has no network dependency and no version to drift.
#
# PORTABILITY: macOS ships bash 3.2 — no `mapfile`, no associative arrays.
set -uo pipefail
cd "$(dirname "$0")/.."

# Files that legitimately describe secret shapes without containing any.
ALLOW_LIST='SECURITY.md CONTRIBUTING.md CLAUDE.md README.md .env.example scripts/scan-secrets.sh .githooks/pre-commit .gitleaks.toml'

PATTERNS='GOCSPX-[A-Za-z0-9_-]{28,}'
PATTERNS="$PATTERNS|sk-ant-[A-Za-z0-9_-]{20,}"
PATTERNS="$PATTERNS|ghp_[A-Za-z0-9]{36}"
PATTERNS="$PATTERNS|github_pat_[A-Za-z0-9_]{60,}"
PATTERNS="$PATTERNS|AKIA[0-9A-Z]{16}"
PATTERNS="$PATTERNS|-----BEGIN [A-Z ]*PRIVATE KEY-----"
PATTERNS="$PATTERNS|\"(client_secret|refresh_token|private_key_id)\"[[:space:]]*:[[:space:]]*\"[^\"]{8,}\""

# A documented placeholder is not a secret. Without this, every honest example of what a
# credential looks like becomes a build failure, and the fix people reach for is to stop
# documenting — exactly backwards.
PLACEHOLDER='[xX]{8,}|EXAMPLE|example\.com|placeholder|REDACTED|<redacted>|your-|YOUR_'

is_allowed() {
  for a in $ALLOW_LIST; do [ "$1" = "$a" ] && return 0; done
  return 1
}

fail=0

echo "==> working tree"
tree_clean=1
while IFS= read -r f; do
  [ -n "$f" ] || continue
  [ -f "$f" ] || continue
  is_allowed "$f" && continue
  hits=$(grep -EnI "$PATTERNS" "$f" 2>/dev/null | grep -Ev "$PLACEHOLDER" || true)
  if [ -n "$hits" ]; then
    echo "  LEAK in $f" >&2
    echo "$hits" | cut -c1-100 | sed 's/^/    /' >&2
    fail=1; tree_clean=0
  fi
done <<EOF
$(git ls-files 2>/dev/null || find . -type d \( -name .git -o -name target -o -name node_modules \) -prune -o -type f -print | sed 's|^\./||')
EOF
[ "$tree_clean" -eq 1 ] && echo "    clean"

# History matters as much as the tip: a secret committed and then deleted is still published.
echo "==> full history"
if git rev-parse --git-dir >/dev/null 2>&1; then
  hits=$(git log --all -p --no-color -U0 -- . \
           ':(exclude)SECURITY.md' ':(exclude)CONTRIBUTING.md' ':(exclude)CLAUDE.md' \
           ':(exclude)README.md' ':(exclude).env.example' ':(exclude)scripts/scan-secrets.sh' \
           ':(exclude).githooks/pre-commit' ':(exclude).gitleaks.toml' 2>/dev/null \
         | grep -E "^\+" | grep -E "$PATTERNS" | grep -Ev "$PLACEHOLDER" | head -5 || true)
  if [ -n "$hits" ]; then
    echo "  LEAK in history:" >&2
    echo "$hits" | cut -c1-100 | sed 's/^/    /' >&2
    echo "  a secret committed and later deleted is still published — rotate it first, then" >&2
    echo "  rewrite history or recreate the repository." >&2
    fail=1
  else
    echo "    clean"
  fi
else
  echo "    (not a git checkout — skipped)"
fi

exit $fail
