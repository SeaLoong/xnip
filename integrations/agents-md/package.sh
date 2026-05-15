#!/usr/bin/env bash
# Pack the AGENTS.md snippet.
#
# Usage: ./package.sh <output-dir>

set -euo pipefail
OUT_DIR="${1:-./dist}"
mkdir -p "$OUT_DIR"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tar -czf "${OUT_DIR}/xnip-agents-md.tar.gz" -C "${HERE}" AGENTS.md
echo "Packaged: ${OUT_DIR}/xnip-agents-md.tar.gz"
