#!/usr/bin/env bash
# Pack the Copilot prompt.
#
# Usage: ./package.sh <output-dir>

set -euo pipefail
OUT_DIR="${1:-./dist}"
mkdir -p "$OUT_DIR"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tar -czf "${OUT_DIR}/xnip-copilot-prompt.tar.gz" -C "${HERE}" xnip.md
echo "Packaged: ${OUT_DIR}/xnip-copilot-prompt.tar.gz"
