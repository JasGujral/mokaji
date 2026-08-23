#!/usr/bin/env bash
# Build the local denylist that enforces this repo's hardest rule:
#
#   NOTHING FROM THE OPERATOR'S VAULT EVER ENTERS THIS REPOSITORY.
#
# Not task text, not note titles, not project names, not chaser wording — nothing. Fixtures are
# invented from scratch; the vault supplies *shape*, never content.
#
# Pattern-matching alone cannot enforce that, because a real task looks exactly like an invented
# one. So this script reads the vault and writes every phrase it finds to `.private-terms`, which
# the pre-commit hook then greps staged changes against. A phrase that exists in the vault cannot
# be committed, full stop.
#
#   ./scripts/gen-private-terms.sh /path/to/vault
#
# `.private-terms` is gitignored and MUST stay that way — the denylist is itself the disclosure it
# exists to prevent. The hook refuses to commit it even if someone forces it into the index.
set -euo pipefail
cd "$(dirname "$0")/.."

VAULT="${1:-${MOKAJI_VAULT_PATH:-}}"
if [ -z "$VAULT" ] || [ ! -d "$VAULT" ]; then
  echo "usage: $0 <vault-path>   (or set MOKAJI_VAULT_PATH)" >&2
  exit 2
fi

OUT=.private-terms
python3 - "$VAULT" "$OUT" <<'PY'
import os, re, sys, unicodedata

vault, out = sys.argv[1], sys.argv[2]
terms = set()

TASK = re.compile(r'^\s*[-*+]\s*\[.\]\s*(.+?)\s*$')
HEAD = re.compile(r'^#{1,6}\s+(.+?)\s*$')
# Tasks-plugin signifiers and tags are metadata, not content.
STRIP = re.compile(r'[📅✅➕⏳🛫🔁]\s*\d{4}-\d{2}-\d{2}|[📅✅➕⏳🛫🔁]|#[\w/-]+|\[\[|\]\]|`')

def clean(s: str) -> str:
    s = STRIP.sub(' ', s)
    s = unicodedata.normalize('NFKC', s)
    s = re.sub(r'[^0-9A-Za-z ]+', ' ', s)
    return ' '.join(s.split()).lower()

def keep(s: str) -> bool:
    # Short or generic phrases would flag innocent code. Require real specificity:
    # at least three words AND at least eighteen characters.
    return len(s) >= 18 and len(s.split()) >= 3

for root, dirs, files in os.walk(vault):
    dirs[:] = [d for d in dirs if not d.startswith('.') and d != '_backups']
    for fn in files:
        if not fn.endswith('.md'):
            continue
        stem = clean(os.path.splitext(fn)[0])
        if keep(stem):
            terms.add(stem)
        path = os.path.join(root, fn)
        try:
            text = open(path, encoding='utf-8', errors='replace').read()
        except OSError:
            continue
        for line in text.splitlines():
            for rx in (TASK, HEAD):
                m = rx.match(line)
                if m:
                    c = clean(m.group(1))
                    if keep(c):
                        terms.add(c)

with open(out, 'w', encoding='utf-8') as f:
    f.write('\n'.join(sorted(terms)) + '\n')
print(f"{len(terms)} phrases")
PY

echo "wrote $OUT — gitignored, never commit it"
echo "the pre-commit hook now blocks any staged change containing one of these phrases"
