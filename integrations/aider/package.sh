#!/usr/bin/env bash
# Pack the Aider conventions for `--read CONVENTIONS.md`.
#
# Usage: ./package.sh <output-dir>

set -euo pipefail
OUT_DIR="${1:-./dist}"
mkdir -p "$OUT_DIR"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tar -czf "${OUT_DIR}/xnip-aider-conventions.tar.gz" -C "${HERE}" CONVENTIONS.md
echo "Packaged: ${OUT_DIR}/xnip-aider-conventions.tar.gz"
