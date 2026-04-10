#!/bin/bash
# canopy skills — manage agent skills via Canopy
# Requires CANOPY_SKILLS_PORT and CANOPY_SKILLS_TOKEN_FILE env vars

set -e

if [ -z "$CANOPY_SKILLS_PORT" ] || [ -z "$CANOPY_SKILLS_TOKEN_FILE" ]; then
  echo "Error: Not running inside Canopy terminal" >&2
  exit 1
fi

# Read token from file (written with mode 0o600 at startup) rather than from env,
# so the plaintext token is never stored in the environment of child processes.
CANOPY_SKILLS_TOKEN=$(cat "$CANOPY_SKILLS_TOKEN_FILE" 2>/dev/null)
if [ -z "$CANOPY_SKILLS_TOKEN" ]; then
  echo "Error: Could not read skills auth token" >&2
  exit 1
fi

BASE_URL="http://127.0.0.1:${CANOPY_SKILLS_PORT}"

action="${1:-help}"
shift 2>/dev/null || true

case "$action" in
  list)
    BODY='{"action":"list","args":{}}'
    if [ "$1" = "--global" ]; then
      BODY='{"action":"list","args":{"scope":"global"}}'
    elif [ "$1" = "--agent" ] && [ -n "$2" ]; then
      BODY=$(jq -n --arg agent "$2" '{"action":"list","args":{"agent":$agent}}')
    fi
    RESULT=$(curl -s -X POST "$BASE_URL" \
      -H "Content-Type: application/json" \
      -H "X-Canopy-Auth: $CANOPY_SKILLS_TOKEN" \
      -d "$BODY" 2>/dev/null)

    # Pretty-print the list
    echo "$RESULT" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    skills = data.get('skills', [])
    if not skills:
        print('No skills installed.')
    else:
        print(f'Installed Skills ({len(skills)}):')
        print()
        for s in skills:
            agents = ', '.join(s.get('enabledAgents', []))
            print(f'  \033[1m{s[\"name\"]}\033[0m  \033[2m({s[\"id\"]})\033[0m')
            if s.get('description'):
                print(f'    {s[\"description\"]}')
            print(f'    \033[1mAgents:\033[0m  {agents}')
            print(f'    \033[1mScope:\033[0m   {s[\"scope\"]}')
            print(f'    \033[1mSource:\033[0m  {s[\"sourceType\"]}')
            print()
except Exception as e:
    print(f'Error: {e}', file=sys.stderr)
    sys.exit(1)
" 2>/dev/null || echo "$RESULT"
    ;;

  install)
    SOURCE="$1"
    if [ -z "$SOURCE" ]; then
      echo "Usage: canopy skills install <source> [--agent <agent>] [--global] [--copy|--symlink]"
      exit 1
    fi
    shift
    # Build args object safely using jq
    ARGS=$(jq -n --arg source "$SOURCE" --arg workspacePath "$PWD" \
      '{"source":$source,"workspacePath":$workspacePath}')
    while [ $# -gt 0 ]; do
      case "$1" in
        --agent)
          ARGS=$(echo "$ARGS" | jq --arg agent "$2" '. + {"agents":[$agent]}')
          shift 2 ;;
        --global)
          ARGS=$(echo "$ARGS" | jq '. + {"scope":"global"}')
          shift ;;
        --copy)
          ARGS=$(echo "$ARGS" | jq '. + {"method":"copy"}')
          shift ;;
        --symlink)
          ARGS=$(echo "$ARGS" | jq '. + {"method":"symlink"}')
          shift ;;
        *) shift ;;
      esac
    done
    BODY=$(jq -n --argjson args "$ARGS" '{"action":"install","args":$args}')
    RESULT=$(curl -s -X POST "$BASE_URL" \
      -H "Content-Type: application/json" \
      -H "X-Canopy-Auth: $CANOPY_SKILLS_TOKEN" \
      -d "$BODY" 2>/dev/null)
    if echo "$RESULT" | grep -q '"error"'; then
      echo "Error: $(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('error',''))" 2>/dev/null || echo "$RESULT")"
      exit 1
    fi
    NAME=$(echo "$RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('name',''))" 2>/dev/null || echo "skill")
    echo "Installed: $NAME"
    ;;

  remove)
    ID="$1"
    if [ -z "$ID" ]; then
      echo "Usage: canopy skills remove <skill-id>"
      exit 1
    fi
    BODY=$(jq -n --arg id "$ID" --arg workspacePath "$PWD" \
      '{"action":"remove","args":{"id":$id,"workspacePath":$workspacePath}}')
    RESULT=$(curl -s -X POST "$BASE_URL" \
      -H "Content-Type: application/json" \
      -H "X-Canopy-Auth: $CANOPY_SKILLS_TOKEN" \
      -d "$BODY" 2>/dev/null)
    if echo "$RESULT" | grep -q '"error"'; then
      echo "Error: $(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('error',''))" 2>/dev/null || echo "$RESULT")"
      exit 1
    fi
    echo "Removed: $ID"
    ;;

  update)
    ID="$1"
    if [ -z "$ID" ]; then
      echo "Usage: canopy skills update <skill-id>"
      exit 1
    fi
    BODY=$(jq -n --arg id "$ID" --arg workspacePath "$PWD" \
      '{"action":"update","args":{"id":$id,"workspacePath":$workspacePath}}')
    RESULT=$(curl -s -X POST "$BASE_URL" \
      -H "Content-Type: application/json" \
      -H "X-Canopy-Auth: $CANOPY_SKILLS_TOKEN" \
      -d "$BODY" 2>/dev/null)
    if echo "$RESULT" | grep -q '"error"'; then
      echo "Error: $(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('error',''))" 2>/dev/null || echo "$RESULT")"
      exit 1
    fi
    echo "Updated: $ID"
    ;;

  info)
    ID="$1"
    if [ -z "$ID" ]; then
      echo "Usage: canopy skills info <skill-id>"
      exit 1
    fi
    BODY=$(jq -n --arg id "$ID" '{"action":"get","args":{"id":$id}}')
    RESULT=$(curl -s -X POST "$BASE_URL" \
      -H "Content-Type: application/json" \
      -H "X-Canopy-Auth: $CANOPY_SKILLS_TOKEN" \
      -d "$BODY" 2>/dev/null)
    echo "$RESULT" | python3 -c "
import sys, json
d = json.load(sys.stdin)
if 'error' in d:
    print(f'Error: {d[\"error\"]}')
    sys.exit(1)
print(f'{d[\"name\"]} v{d.get(\"version\",\"?\")}')
print(f'  ID:          {d[\"id\"]}')
print(f'  Description: {d.get(\"description\",\"(none)\")}')
print(f'  Agents:      {\", \".join(d.get(\"agents\",[]))}')
print(f'  Enabled:     {\", \".join(d.get(\"enabledAgents\",[]))}')
print(f'  Scope:       {d.get(\"scope\",\"?\")}')
print(f'  Source:      {d.get(\"sourceType\",\"?\")} — {d.get(\"sourceUri\",\"?\")}')
print(f'  Method:      {d.get(\"installMethod\",\"?\")}')
" 2>/dev/null || echo "$RESULT"
    ;;

  help|*)
    echo "Usage: canopy skills <command> [options]"
    echo ""
    echo "Commands:"
    echo "  list    [--global] [--agent <name>]    List installed skills"
    echo "  install <source> [--agent <name>] [--global] [--copy|--symlink]"
    echo "  remove  <skill-id>                     Remove a skill"
    echo "  update  <skill-id>                     Update from source"
    echo "  info    <skill-id>                     Show skill details"
    ;;
esac
