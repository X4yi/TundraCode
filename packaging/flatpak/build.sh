#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="${1:-$PROJECT_ROOT/.flatpak-build}"
REPO_DIR="${2:-$PROJECT_ROOT/.flatpak-repo}"
BUNDLE_FILE="$PROJECT_ROOT/tundracode.flatpak"

echo "=== TundraCode Flatpak Builder ==="
echo ""

cd "$SCRIPT_DIR"

echo "[1/3] Generating dependency sources..."
bash "$SCRIPT_DIR/generate-sources.sh"

echo ""
echo "[2/3] Building Flatpak..."
flatpak-builder \
    --force-clean \
    --repo="$REPO_DIR" \
    "$BUILD_DIR" \
    "$SCRIPT_DIR/com.tundracode.dev.yml"

echo ""
echo "[3/3] Creating bundle..."
flatpak build-bundle \
    "$REPO_DIR" \
    "$BUNDLE_FILE" \
    com.tundracode.dev

echo ""
echo "=== Build Complete ==="
echo ""
echo "Bundle created at: $BUNDLE_FILE"
echo ""
echo "To install:"
echo "  flatpak install --user $BUNDLE_FILE"
echo ""
echo "To run:"
echo "  flatpak run com.tundracode.dev"
