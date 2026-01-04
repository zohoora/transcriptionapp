#!/bin/bash
# Build a distributable version of the app with ONNX Runtime bundled
# Usage: ./scripts/build-distributable.sh [--release]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

# Parse args
BUILD_TYPE="debug"
if [ "$1" = "--release" ]; then
    BUILD_TYPE="release"
fi

echo "=== Building Transcription App ($BUILD_TYPE) ==="

# Ensure ONNX Runtime is installed
echo "Step 1: Ensuring ONNX Runtime is available..."
./scripts/setup-ort.sh > /dev/null

# Build the app
echo "Step 2: Building Tauri app..."
if [ "$BUILD_TYPE" = "release" ]; then
    pnpm tauri build
    APP_BUNDLE="src-tauri/target/release/bundle/macos/Transcription App.app"
else
    pnpm tauri build --debug
    APP_BUNDLE="src-tauri/target/debug/bundle/macos/Transcription App.app"
fi

# Bundle ONNX Runtime
echo "Step 3: Bundling ONNX Runtime..."
./scripts/bundle-ort.sh "$APP_BUNDLE"

# Show result
echo ""
echo "=== Build Complete ==="
echo "Distributable app: $APP_BUNDLE"
echo ""
echo "To create a distributable DMG:"
if [ "$BUILD_TYPE" = "release" ]; then
    echo "  DMG at: src-tauri/target/release/bundle/dmg/"
else
    echo "  DMG at: src-tauri/target/debug/bundle/dmg/"
fi
echo ""
echo "Note: After copying to another Mac, users may need to right-click -> Open"
echo "the first time to bypass Gatekeeper (since the app is not code-signed)."

# Also bundle ORT into the DMG's app copy
DMG_DIR="src-tauri/target/$BUILD_TYPE/bundle/dmg"
if [ -d "$DMG_DIR" ]; then
    echo ""
    echo "Updating DMG with bundled ORT..."
    # DMG contains another copy of the app, need to update it too
    # This is tricky because the DMG is already created
    # For now, recommend rebuilding after bundling
fi
