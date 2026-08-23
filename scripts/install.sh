#!/usr/bin/env bash
# Build MOKaji and put it on this machine's PATH.
#
#   ./scripts/install.sh              build + install the CLI
#   ./scripts/install.sh --app        also build and install the desktop app (macOS)
#
# Installs to ~/.local/bin, which needs no sudo and no write access outside your home directory.
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
    echo "==> --app is macOS-only; skipping" >&2
  else
    echo "==> building the desktop app (this takes a while the first time)"
    npm install
    npm run tauri build
    APP=$(find src-tauri/target/release/bundle/macos -maxdepth 1 -name '*.app' | head -1)
    if [ -n "$APP" ]; then
      rm -rf "/Applications/$(basename "$APP")"
      cp -R "$APP" /Applications/
      echo "==> installed /Applications/$(basename "$APP")"
    else
      echo "==> no .app produced — check the tauri build output" >&2
    fi
  fi
fi

echo
echo "Try it:"
echo "    mokaji --vault <path-to-your-vault>"
echo "    mokaji tasks    --vault <path>"
echo "    mokaji chasers  --vault <path>"
echo "    mokaji health   --vault <path>"
echo
echo "Set MOKAJI_VAULT_PATH in ~/.zshrc to drop the --vault flag:"
echo "    export MOKAJI_VAULT_PATH=\"<path-to-your-vault>\""
