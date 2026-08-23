#!/usr/bin/env bash
# Point git at the committed hooks. Run once per clone.
set -euo pipefail
cd "$(dirname "$0")/.."
git config core.hooksPath .githooks
chmod +x .githooks/*
echo "hooks installed: $(ls .githooks | tr '\n' ' ')"
echo "bypass a single commit with --no-verify (and say why in the commit body)"
