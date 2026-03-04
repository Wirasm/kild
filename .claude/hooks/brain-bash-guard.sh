#!/usr/bin/env bash
#
# brain-bash-guard.sh — PreToolUse hook for the kild-brain agent.
# Blocks bash commands that access project source code.
# The brain operates the fleet via the kild CLI, not by reading source.
#
# Exit 0 = allow, Exit 2 = block with reason on stderr.

set -euo pipefail

# Extract the command from CLAUDE_CODE_TOOL_INPUT (JSON with "command" field).
COMMAND="${CLAUDE_CODE_TOOL_INPUT:-}"
if [ -z "$COMMAND" ]; then
  exit 0
fi

# Extract the "command" field value from JSON.
# Use grep+sed to avoid jq dependency.
CMD=$(echo "$COMMAND" | grep -o '"command"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | sed 's/.*"command"[[:space:]]*:[[:space:]]*"//;s/"$//')

if [ -z "$CMD" ]; then
  exit 0
fi

# --- Blocklist: patterns that indicate source code access ---

# Source code paths
for pattern in "crates/" "src/" "target/" "tests/"; do
  if echo "$CMD" | grep -q "$pattern"; then
    echo "BLOCKED: Brain must not access source code (matched: $pattern)." >&2
    echo "Use kild CLI commands (kild diff, kild stats) instead of reading source directly." >&2
    exit 2
  fi
done

# Build/compile commands
for pattern in "^cargo " "^rustc" "^rustup"; do
  if echo "$CMD" | grep -qE "$pattern"; then
    echo "BLOCKED: Brain must not run build tools (matched: $pattern)." >&2
    echo "Workers handle builds. Use kild CLI to manage the fleet." >&2
    exit 2
  fi
done

# Raw git commands that have kild CLI equivalents
if echo "$CMD" | grep -qE "^git diff"; then
  echo "BLOCKED: Use 'kild diff <branch>' instead of raw git diff." >&2
  exit 2
fi

if echo "$CMD" | grep -qE "^git log"; then
  echo "BLOCKED: Use 'kild stats <branch>' instead of raw git log." >&2
  exit 2
fi

# If no blocklist pattern matched, allow the command.
exit 0
