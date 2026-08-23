#!/usr/bin/env bash
# MOKaji reads one person's vault, calendar and inbox. This repo is public. Those two facts are
# only compatible if nothing personal ever lands in a commit.
#
# Runs in CI over the whole tree, and per-commit from .githooks/pre-commit.
#   scripts/check-personal-data.sh [files...]     (no args = the tracked tree)
#
# PORTABILITY: macOS ships bash 3.2 (2007), which has no `mapfile` and errors on an empty array
# expansion under `set -u`. This script therefore uses no arrays at all — it streams filenames
# through a pipeline instead. CI runs bash 5 and the developer's machine runs 3.2, so "works on
# the runner" is not the bar.
set -uo pipefail
cd "$(dirname "$0")/.."

fail=0
count=0
say() { printf '\033[31mpersonal-data: %s\033[0m\n' "$1" >&2; fail=1; }

# Absolute paths that identify a machine or a person. A vault location belongs in config, never in
# source, docs or fixtures.
HOMEPATHS='/Users/[a-z][a-z0-9._-]*|/home/[a-z][a-z0-9._-]*|C:\\\\Users\\\\'
EMAIL='[A-Za-z0-9._%+-]+@(gmail|googlemail|outlook|hotmail|yahoo|icloud|proton(mail)?)\.[A-Za-z]{2,}'
PHONE='\+[0-9]{1,3}[- ]?[0-9]{6,}'

list_files() {
  if [ "$#" -gt 0 ]; then
    printf '%s\n' "$@"
  elif git rev-parse --git-dir >/dev/null 2>&1; then
    git ls-files
  else
    find . -type d \( -name .git -o -name target -o -name node_modules \) -prune -o -type f -print \
      | sed 's|^\./||'
  fi
}

while IFS= read -r f; do
  [ -n "$f" ] || continue
  [ -f "$f" ] || continue
  case "$f" in
    LICENSE|NOTICE|scripts/check-personal-data.sh|scripts/scan-secrets.sh|.githooks/pre-commit|CONTRIBUTING.md) continue ;;
    *.png|*.jpg|*.jpeg|*.gif|*.pdf|*.gguf|*.onnx|Cargo.lock) continue ;;
  esac
  count=$((count + 1))

  if grep -EnI "$HOMEPATHS" "$f" >/dev/null 2>&1; then
    say "$f contains an absolute home path — use a config value or a relative fixture path"
    grep -EnI "$HOMEPATHS" "$f" | head -3 >&2
  fi
  if grep -EnI "$EMAIL" "$f" >/dev/null 2>&1; then
    say "$f contains a personal email address — fixtures must use example.com"
    grep -EnI "$EMAIL" "$f" | head -3 >&2
  fi
  if grep -EnI "$PHONE" "$f" >/dev/null 2>&1; then
    say "$f contains something shaped like a phone number"
  fi

  # Fixtures are the likeliest leak: real task text, real note bodies, real names. Everything under
  # a fixtures/ directory must be invented, and must say so where a reader will see it.
  case "$f" in
    *fixtures/*.md|*fixtures/*.json|*fixtures/*.yaml|*fixtures/*.yml|*fixtures/*.txt)
      if ! head -5 "$f" | grep -qi 'synthetic'; then
        say "$f is a fixture without a 'synthetic' marker in its first 5 lines — nothing real may be committed"
      fi
      ;;
  esac
done <<EOF
$(list_files "$@")
EOF

[ "$fail" -eq 0 ] && echo "personal-data: clean ($count files)"
exit $fail
