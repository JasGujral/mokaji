#!/usr/bin/env bash
# Build MOKaji and put it on this machine.
#
#   ./scripts/install.sh              the CLI only
#   ./scripts/install.sh --app        the CLI plus the desktop app (macOS)
#
# Installs the CLI to ~/.local/bin — no sudo, nothing written outside your home directory.
#
# PORTABILITY: macOS ships bash 3.2 — no `mapfile`, no associative arrays, and empty-array
# expansion errors under `set -u`. Keep this script to POSIX-ish constructs.
set -euo pipefail
cd "$(dirname "$0")/.."

BIN_DIR="${MOKAJI_BIN_DIR:-$HOME/.local/bin}"
WITH_APP=0
[ "${1:-}" = "--app" ] && WITH_APP=1

echo "==> building the CLI (release)"
cargo build --release -p mokaji-cli

mkdir -p "$BIN_DIR"
install -m 0755 target/release/mokaji "$BIN_DIR/mokaji"
echo "==> installed $BIN_DIR/mokaji"

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *)
    echo
    echo "    $BIN_DIR is not on your PATH. Add this to ~/.zshrc:"
    echo "        export PATH=\"\$HOME/.local/bin:\$PATH\""
    ;;
esac

if [ "$WITH_APP" = "1" ]; then
  if [ "$(uname -s)" != "Darwin" ]; then
    echo "==> --app is macOS-only; skipping the bundle" >&2
  else
    echo "==> installing frontend dependencies"
    npm install --no-audit --no-fund

    # Tauri bundles a .icns on macOS and only ships a .png in the repo, because generated icon
    # binaries are build output rather than source. Regenerate them from the one source icon.
    if [ ! -f src-tauri/icons/icon.icns ]; then
      echo "==> generating platform icons from src-tauri/icons/icon.png"
      npx --yes @tauri-apps/cli icon src-tauri/icons/icon.png
    fi

    echo "==> building the desktop app (first run compiles the whole Tauri tree — expect minutes)"
    npm run tauri build

    APP=""
    for candidate in \
      src-tauri/target/release/bundle/macos/*.app \
      src-tauri/target/*/release/bundle/macos/*.app; do
      [ -d "$candidate" ] && APP="$candidate" && break
    done

    if [ -n "$APP" ]; then
      rm -rf "/Applications/$(basename "$APP")"
      cp -R "$APP" /Applications/
      echo "==> installed /Applications/$(basename "$APP")"
      echo "    open it with:  open -a MOKaji"
    else
      echo "==> no .app was produced — check the tauri build output above" >&2
      exit 1
    fi
  fi
fi

echo
echo "The CLI:"
echo "    mokaji            Reactor Core readout"
echo "    mokaji tasks      open tasks, in the Deck's order"
echo "    mokaji chasers    waiting-on and need-to-nudge"
echo "    mokaji vitals     today's tracker metrics"
echo "    mokaji health     connector health"
echo
echo "Point both at your vault by putting this in ~/.zshrc:"
echo "    export MOKAJI_VAULT_PATH=\"<path-to-your-vault>\""
