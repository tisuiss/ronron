#!/bin/sh
# Packages the two extensions into release/ (CI: the Windows release attaches them).
set -e
cd "$(dirname "$0")"
OUT="${1:-release}"
mkdir -p "$OUT"
OUT_ABS=$(cd "$OUT" && pwd)
for side in local remote; do
  cp ../LICENSE "$side/LICENSE"
  cp README.md "$side/README.md"
  v=$(node -p "require('./$side/package.json').version")
  (cd "$side" && npx --yes @vscode/vsce@3 package --no-dependencies -o "$OUT_ABS/ronron-vscode-$side-$v.vsix")
done
