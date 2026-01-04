#!/bin/bash
# Bundle ONNX Runtime into the macOS app bundle
# This script copies the libonnxruntime dylib into Contents/Frameworks/
# so the app can find it at runtime without needing ORT_DYLIB_PATH set

set -e

# Find the app bundle
APP_BUNDLE="${1:-src-tauri/target/debug/bundle/macos/Transcription App.app}"

if [ ! -d "$APP_BUNDLE" ]; then
    echo "Error: App bundle not found at: $APP_BUNDLE"
    exit 1
fi

# Find the ONNX Runtime dylib
ORT_VENV="${ORT_VENV:-$HOME/.transcriptionapp/ort-venv}"
DYLIB_PATH=$(find "$ORT_VENV" -name "libonnxruntime.*.dylib" 2>/dev/null | head -1)

if [ -z "$DYLIB_PATH" ]; then
    echo "Error: Could not find libonnxruntime.*.dylib in $ORT_VENV"
    echo "Run ./scripts/setup-ort.sh first to install ONNX Runtime"
    exit 1
fi

# Create Frameworks directory
FRAMEWORKS_DIR="$APP_BUNDLE/Contents/Frameworks"
mkdir -p "$FRAMEWORKS_DIR"

# Copy the dylib
DYLIB_NAME=$(basename "$DYLIB_PATH")
cp "$DYLIB_PATH" "$FRAMEWORKS_DIR/"

echo "Bundled ONNX Runtime: $DYLIB_NAME"
echo "  From: $DYLIB_PATH"
echo "  To: $FRAMEWORKS_DIR/$DYLIB_NAME"

# Verify
if [ -f "$FRAMEWORKS_DIR/$DYLIB_NAME" ]; then
    echo "Success! App bundle now includes ONNX Runtime"
    ls -lh "$FRAMEWORKS_DIR/$DYLIB_NAME"
else
    echo "Error: Failed to copy dylib"
    exit 1
fi
