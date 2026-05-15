#!/usr/bin/env bash
# Pack the Cursor rule for `<workspace>/.cursor/rules/xnip.mdc`.
#
# Usage: ./package.sh <output-dir>
#
# Output: <output-dir>/xnip-cursor-rule.tar.gz containing xnip.mdc

set -euo pipefail

OUT_DIR="${1:-./dist}"
mkdir -p "$OUT_DIR"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

tar -czf "${OUT_DIR}/xnip-cursor-rule.tar.gz" -C "${HERE}" xnip.mdc

echo "Packaged: ${OUT_DIR}/xnip-cursor-rule.tar.gz"
