#!/usr/bin/env bash
# Pack the Claude Code skill as a tar.gz suitable for `~/.claude/skills/xnip/`.
#
# Usage: ./package.sh <output-dir>
#
# Output: <output-dir>/xnip-claude-code-skill.tar.gz

set -euo pipefail

OUT_DIR="${1:-./dist}"
mkdir -p "$OUT_DIR"

# Use this script's directory as the working root.
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

tar -czf "${OUT_DIR}/xnip-claude-code-skill.tar.gz" -C "${HERE}" SKILL.md

echo "Packaged: ${OUT_DIR}/xnip-claude-code-skill.tar.gz"
