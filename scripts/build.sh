#!/usr/bin/env bash
# Build herdr-graph in release mode and place the binary at <plugin>/bin/herdr-graph (spec §12).
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$here"
cargo build --release --bin herdr-graph
mkdir -p bin
cp -f target/release/herdr-graph bin/herdr-graph
chmod 755 bin/herdr-graph
echo "built $here/bin/herdr-graph"
