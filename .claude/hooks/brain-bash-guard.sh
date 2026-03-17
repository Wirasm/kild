#!/usr/bin/env bash
#
# brain-bash-guard.sh — PreToolUse hook for the kild-brain agent.
# Blocks bash commands that access project source code.
# The brain operates the fleet via the kild CLI, not by reading source.
#
# Exit 0 = allow, Exit 2 = block with reason on stderr.
#
# ADVISORY: This guard catches common direct access patterns but cannot
# prevent all indirect execution (e.g., bash -c, sh -c, subshells).
# It is a best-effort safety net, not a sandbox.

set -uo pipefail

# Fail closed on unexpected errors.
trap 'echo "brain-bash-guard: unexpected error, blocking as safety measure" >&2; exit 2' ERR

# Extract the command from CLAUDE_CODE_TOOL_INPUT (JSON with "command" field).
COMMAND="${CLAUDE_CODE_TOOL_INPUT:-}"
if [ -z "$COMMAND" ]; then
  exit 0
fi

# Extract the "command" field value from JSON.
CMD=$(printf '%s' "$COMMAND" | jq -r '.command // empty' 2>/dev/null)

if [ -z "$CMD" ]; then
  # Parse failure or missing field — block rather than allow.
  echo "BLOCKED: could not parse tool input command field." >&2
  exit 2
fi

# --- Blocklist: patterns that indicate source code access ---
# NOTE: This is advisory — subshell invocations (bash -c, sh -c) can bypass
# these checks. The brain agent instructions also prohibit source access.

# Source code paths (anchored to word boundary to reduce false positives)
for pattern in "crates/" "target/" "tests/"; do
  if [[ "$CMD" == *"$pattern"* ]]; then
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

# Subshell invocations that could bypass the guard
for pattern in "^bash -c" "^sh -c" "^env bash" "^env sh"; do
  if echo "$CMD" | grep -qE "$pattern"; then
    echo "BLOCKED: Brain must not use subshell invocations (matched: $pattern)." >&2
    echo "Run commands directly so the guard can inspect them." >&2
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
