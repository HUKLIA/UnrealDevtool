#!/bin/bash
# Run this from WSL2 to cross-compile a Windows release .exe.
# Output goes into "WSL2 Build/" (gitignored) instead of the default target/.
#
# One-time setup (if not done yet):
#   sudo apt update && sudo apt install -y build-essential gcc-mingw-w64-x86-64
#   rustup target add x86_64-pc-windows-gnu

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

CARGO_TARGET_DIR="WSL2 Build" \
  cargo build --release --target x86_64-pc-windows-gnu

echo ""
echo "Done: WSL2 Build/x86_64-pc-windows-gnu/release/unreal_devtool.exe"
