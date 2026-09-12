#!/usr/bin/env bash
set -euo pipefail

FFI_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$FFI_DIR/.." && pwd)"
DIST_DIR="$REPO_ROOT/dist/hayagriva-ffi"
LIB_SOURCE_DIR="$FFI_DIR/target/release"
INCLUDE_DIR="$DIST_DIR/inc/hayagriva"
LIB_DIR="$DIST_DIR/libs"

if ! command -v cargo >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    . "$HOME/.cargo/env"
fi

mkdir -p "$INCLUDE_DIR" "$LIB_DIR"

cargo build --manifest-path "$FFI_DIR/Cargo.toml" --release

cp "$FFI_DIR/include/hayagriva.h" "$INCLUDE_DIR/hayagriva.h"
if [ "$(uname -s)" = "Darwin" ]; then
    cp "$LIB_SOURCE_DIR/libhayagriva_ffi.dylib" "$LIB_DIR/"
    install_name_tool -id "@rpath/libhayagriva_ffi.dylib" "$LIB_DIR/libhayagriva_ffi.dylib"
else
    cp "$LIB_SOURCE_DIR/libhayagriva_ffi.so" "$LIB_DIR/"
fi

find "$DIST_DIR" -type f | sort
