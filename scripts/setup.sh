#!/bin/bash
set -e

PLUGIN_ROOT="${CLAUDE_PLUGIN_ROOT:-$(dirname "$(dirname "$0")")}"
BIN_DIR="${PLUGIN_ROOT}/bin"
BINARY_NAME="lint"
REPO="chenhunghan/ralph-hook-lint"

# Skip if binary already exists
if [[ -x "$BIN_DIR/$BINARY_NAME" ]]; then
  exit 0
fi

mkdir -p "$BIN_DIR"

# Detect platform
OS=$(uname -s)
ARCH=$(uname -m)

case "$OS-$ARCH" in
  Darwin-arm64)
    PLATFORM="aarch64-apple-darwin"
    ;;
  Darwin-x86_64)
    PLATFORM="x86_64-apple-darwin"
    ;;
  Linux-x86_64)
    PLATFORM="x86_64-unknown-linux-gnu"
    ;;
  Linux-aarch64)
    PLATFORM="aarch64-unknown-linux-gnu"
    ;;
  *)
    echo "{\"continue\": true, \"systemMessage\": \"lint-hook: unsupported platform $OS-$ARCH\"}"
    exit 0
    ;;
esac

# Get latest release URL
RELEASE_URL="https://github.com/${REPO}/releases/latest/download/${BINARY_NAME}-${PLATFORM}.tar.gz"

# Download and extract
cd "$BIN_DIR"
if curl -fsSL "$RELEASE_URL" | tar xz 2>/dev/null; then
  chmod +x "$BIN_DIR/$BINARY_NAME"
  echo '{"continue": true, "systemMessage": "lint-hook: binary installed successfully"}'
else
  echo "{\"continue\": true, \"systemMessage\": \"lint-hook: failed to download binary from $RELEASE_URL\"}"
fi

exit 0
