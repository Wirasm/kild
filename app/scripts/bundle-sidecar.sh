#!/usr/bin/env bash
# Compile the kild engine to a standalone binary and place it where Tauri's
# externalBin (sidecar) expects it: `binaries/kild-engine-<host-target-triple>`.
# Run by `beforeBuildCommand` before `tauri build`.
set -euo pipefail

APP="$(cd "$(dirname "$0")/.." && pwd)"
ENGINE="$APP/../engine"
TRIPLE="$(rustc -Vv | grep '^host:' | cut -d' ' -f2)"

echo "bundling engine sidecar for $TRIPLE"
(cd "$ENGINE" && bun run compile)

mkdir -p "$APP/src-tauri/binaries"
cp "$ENGINE/dist/kild-engine" "$APP/src-tauri/binaries/kild-engine-$TRIPLE"
echo "→ src-tauri/binaries/kild-engine-$TRIPLE"
