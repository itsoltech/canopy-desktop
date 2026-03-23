#!/bin/bash
# canopy: Forward Claude Code hook JSON to canopy main process via HTTP
[ -z "$CANOPY_HOOK_PORT" ] && exit 0
INPUT=$(cat)
RESPONSE=$(curl -s -X POST "http://127.0.0.1:${CANOPY_HOOK_PORT}/hook" \
  -H "Content-Type: application/json" \
  -d "$INPUT" 2>/dev/null) || exit 0
# Only output if response has hookSpecificOutput (SessionStart, PermissionRequest)
[ -n "$RESPONSE" ] && [ "$RESPONSE" != "{}" ] && echo "$RESPONSE"
exit 0
