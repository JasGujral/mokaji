#!/usr/bin/env bash
# MOKaji reads one person's vault, calendar and inbox. This repo is public. Those two facts are
# only compatible if nothing personal ever lands in a commit.
#
# Runs in CI over the whole tree, and per-commit from .githooks/pre-commit.
# Usage: scripts/check-personal-data.sh [files...]   (no args = scan the tracked tree)
set -uo pipefail
cd "$(dirname "$0")/.."

if [ "$#" -gt 0 ]; then
  files=("$@")
elif git rev-parse --git-dir >/dev/null 2>&1; then
  mapfile -t files < <(git ls-files)
else
  # works outside a checkout too, so the check is runnable anywhere
  mapfile -t files < <(find . -type d \( -name .git -o -name target -o -name node_modules \) -prune -o -type f -print | sed 's|^\./||')
fi
[ "${#files[@]}" -eq 0 ] && exit 0

fail=0
say() { printf '\033[31mpersonal-data: %s\033[0m\n' "$1" >&2; fail=1; }

# Absolute paths that identify a machine or a person. Vault location belongs in config, never
# in source, docs or fixtures.
HOMEPATHS='/Users/[a-z][a-z0-9._-]*|/home/[a-z][a-z0-9._-]*|C:\\\\Users\\\\'
# Contact details.
EMAIL='[A-Za-z0-9._%+-]+@(gmail|googlemail|outlook|hotmail|yahoo|icloud|proton(mail)?)\.[A-Za-z]{2,}'
PHONE='\+[0-9]{1,3}[- ]?[0-9]{6,}'

for f in "${files[@]}"; do
  [ -f "$f" ] || continue
  case "$f" in
    LICENSE|NOTICE|scripts/check-personal-data.sh|.githooks/pre-commit|CONTRIBUTING.md) continue ;;
    *.png|*.jpg|*.jpeg|*.gif|*.pdf|*.gguf|*.onnx|Cargo.lock) continue ;;
  esac

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
done

# Fixtures are the likeliest leak: real task text, real note bodies, real names.
# Everything under a fixtures/ directory must be synthetic, and must say so.
while IFS= read -r fx; do
  [ -f "$fx" ] || continue
  case "$fx" in *.md|*.json|*.yaml|*.yml|*.txt) ;; *) continue ;; esac
  head -5 "$fx" | grep -qi 'synthetic' || \
    say "$fx is a fixture without a 'synthetic' marker in its first 5 lines — real personal data must never be committed"
done < <(printf '%s\n' "${files[@]}" | grep -E '(^|/)fixtures/' || true)

[ "$fail" -eq 0 ] && echo "personal-data: clean (${#files[@]} files)"
exit $fail
