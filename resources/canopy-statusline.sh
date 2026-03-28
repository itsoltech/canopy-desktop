#!/bin/bash
# canopy: Forward Claude Code status line JSON to canopy main process
[ -z "$CANOPY_HOOK_PORT" ] && exit 0
INPUT=$(cat)
curl -s -X POST "http://127.0.0.1:${CANOPY_HOOK_PORT}/status" \
  -H "Content-Type: application/json" \
  -H "X-Canopy-Auth: ${CANOPY_HOOK_TOKEN}" \
  -d "$INPUT" 2>/dev/null &
# Output empty line so Claude's own status bar stays clean
echo ""
