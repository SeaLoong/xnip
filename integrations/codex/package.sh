#!/usr/bin/env bash
# Package Codex instructions without exposing a user-invoked skill command.
set -euo pipefail
OUT_DIR="${1:-./dist}"
mkdir -p "$OUT_DIR"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tar -czf "${OUT_DIR}/xnip-codex-instructions.tar.gz" -C "${HERE}" AGENTS.md guide.md references
echo "Packaged: ${OUT_DIR}/xnip-codex-instructions.tar.gz"
