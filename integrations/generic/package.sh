#!/usr/bin/env bash
# Pack the generic SKILL.md (auto-synced from docs/SKILL.md by `cargo xtask sync-integrations`).
#
# Usage: ./package.sh <output-dir>

set -euo pipefail
OUT_DIR="${1:-./dist}"
mkdir -p "$OUT_DIR"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tar -czf "${OUT_DIR}/xnip-generic-skill.tar.gz" -C "${HERE}" SKILL.md
echo "Packaged: ${OUT_DIR}/xnip-generic-skill.tar.gz"
