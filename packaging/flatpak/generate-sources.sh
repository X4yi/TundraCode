#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TOOLS_DIR="$SCRIPT_DIR/.flatpak-builder-tools"

CARGO_LOCK="$PROJECT_ROOT/Cargo.lock"
PACKAGE_JSON="$PROJECT_ROOT/package.json"
PACKAGE_LOCK="$PROJECT_ROOT/package-lock.json"

CARGO_SOURCES="$SCRIPT_DIR/cargo-sources.json"
NODE_SOURCES="$SCRIPT_DIR/node-sources.json"

if [ ! -f "$CARGO_LOCK" ]; then
    echo "Error: Cargo.lock not found at $CARGO_LOCK"
    exit 1
fi

if [ ! -f "$PACKAGE_JSON" ]; then
    echo "Error: package.json not found at $PACKAGE_JSON"
    exit 1
fi

if [ ! -d "$TOOLS_DIR" ]; then
    echo "Cloning flatpak-builder-tools..."
    git clone --depth 1 https://github.com/flatpak/flatpak-builder-tools.git "$TOOLS_DIR"
fi

echo "Generating cargo-sources.json..."
python3 "$TOOLS_DIR/cargo/flatpak-cargo-generator.py" \
    "$CARGO_LOCK" \
    -o "$CARGO_SOURCES"

echo "Generating node-sources.json..."
python3 "$TOOLS_DIR/node/flatpak-node-generator.py" \
    npm "$PACKAGE_LOCK" \
    -o "$NODE_SOURCES"

echo "Done! Generated:"
echo "  - $CARGO_SOURCES"
echo "  - $NODE_SOURCES"
