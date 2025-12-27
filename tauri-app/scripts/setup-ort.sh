#!/bin/bash
# Setup ONNX Runtime for speaker diarization
# This script creates a Python venv with onnxruntime and exports ORT_DYLIB_PATH

set -e

ORT_VENV="${ORT_VENV:-$HOME/.transcriptionapp/ort-venv}"
ORT_VERSION="1.23.2"

# Create venv if it doesn't exist
if [ ! -d "$ORT_VENV" ]; then
    echo "Creating ONNX Runtime virtual environment at $ORT_VENV..."
    python3 -m venv "$ORT_VENV"
    "$ORT_VENV/bin/pip" install --quiet onnxruntime
    echo "ONNX Runtime installed successfully"
fi

# Find the dylib
DYLIB_PATH=$(find "$ORT_VENV" -name "libonnxruntime.*.dylib" 2>/dev/null | head -1)

if [ -z "$DYLIB_PATH" ]; then
    echo "Error: Could not find libonnxruntime.*.dylib in $ORT_VENV"
    exit 1
fi

echo "$DYLIB_PATH"
