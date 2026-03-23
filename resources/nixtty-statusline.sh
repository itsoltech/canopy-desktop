#!/bin/bash
# nixtty: Forward Claude Code status line JSON to nixtty main process
[ -z "$NIXTTY_HOOK_PORT" ] && exit 0
INPUT=$(cat)
curl -s -X POST "http://127.0.0.1:${NIXTTY_HOOK_PORT}/status" \
  -H "Content-Type: application/json" \
  -d "$INPUT" 2>/dev/null &
# Output empty line so Claude's own status bar stays clean
echo ""
