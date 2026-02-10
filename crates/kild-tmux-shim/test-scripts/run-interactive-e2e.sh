#!/bin/bash
# Level 4: Interactive E2E test with real Claude Code agent teams
#
# This script sets up a daemon kild, then YOU run claude interactively inside it.
# The script monitors shim logs in the background and reports what happened.
#
# Usage: ./run-interactive-e2e.sh
#
# IMPORTANT: claude -p (non-interactive) forces in-process teammates.
# The tmux shim only works with interactive Claude sessions inside a daemon PTY.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
RESULTS_DIR="/tmp/kild-shim-test/e2e-$(date +%Y%m%d-%H%M%S)"
KILD="$REPO_ROOT/target/debug/kild"
BRANCH="shim-e2e"

mkdir -p "$RESULTS_DIR"

cleanup() {
    echo ""
    echo "=== Cleanup ==="
    # Save final artifacts
    if [ -n "${SESSION_ID:-}" ]; then
        SHIM_STATE="$HOME/.kild/shim/$SESSION_ID"
        cp "$SHIM_STATE/panes.json" "$RESULTS_DIR/panes-final.json" 2>/dev/null || true
        cp "$SHIM_STATE/shim.log" "$RESULTS_DIR/shim.log" 2>/dev/null || true
    fi

    "$KILD" destroy "$BRANCH" --force 2>/dev/null || true
    "$KILD" daemon stop 2>/dev/null || true

    echo ""
    echo "=== Results ==="
    if [ -f "$RESULTS_DIR/shim.log" ]; then
        echo "Shim was called! Analyzing..."
        echo ""
        SPLIT_COUNT=$(grep -c "split_window_completed" "$RESULTS_DIR/shim.log" 2>/dev/null || echo 0)
        SEND_COUNT=$(grep -c "send_keys_completed" "$RESULTS_DIR/shim.log" 2>/dev/null || echo 0)
        KILL_COUNT=$(grep -c "kill_pane_completed" "$RESULTS_DIR/shim.log" 2>/dev/null || echo 0)
        echo "  split-window: $SPLIT_COUNT"
        echo "  send-keys:    $SEND_COUNT"
        echo "  kill-pane:    $KILL_COUNT"
        echo ""
        echo "Full shim log: $RESULTS_DIR/shim.log"
        echo "Pane state:    $RESULTS_DIR/panes-final.json"
    else
        echo "Shim was NEVER called. Claude likely used in-process mode."
        echo "This means $TMUX was not detected or non-interactive mode was forced."
    fi
    echo ""
    echo "All results: $RESULTS_DIR"
}
trap cleanup EXIT

echo "=== Level 4: Interactive E2E Test ==="
echo "Results: $RESULTS_DIR"
echo ""

# Build
echo "--- Building ---"
cargo build -p kild -p kild-tmux-shim --quiet 2>&1
echo "OK"
echo ""

# Start daemon
echo "--- Starting daemon ---"
"$KILD" daemon stop 2>/dev/null || true
"$KILD" daemon start 2>&1
echo ""

# Create kild with daemon mode (no agent - we'll run claude manually)
echo "--- Creating test kild ---"
"$KILD" create "$BRANCH" --daemon --no-agent 2>&1
SESSION_ID=$("$KILD" status "$BRANCH" --json 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
echo ""

# Enable shim logging
SHIM_STATE="$HOME/.kild/shim/$SESSION_ID"
echo "--- Shim state ---"
echo "  Session ID: $SESSION_ID"
echo "  State dir:  $SHIM_STATE"
echo "  Panes:      $(cat "$SHIM_STATE/panes.json" 2>/dev/null | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'{len(d[\"panes\"])} pane(s)')" 2>/dev/null || echo "unknown")"
echo ""

# Save initial state
cp "$SHIM_STATE/panes.json" "$RESULTS_DIR/panes-before.json" 2>/dev/null || true

cat <<'INSTRUCTIONS'
================================================================
  INTERACTIVE E2E TEST — WHAT TO DO
================================================================

1. In ANOTHER terminal, attach to the kild:

     kild attach shim-e2e

2. Inside the attached PTY, verify the environment:

     echo $TMUX          # Should show daemon socket path
     echo $TMUX_PANE     # Should show %0
     which tmux           # Should show ~/.kild/bin/tmux
     tmux -V              # Should show "tmux 3.4" (our shim!)

3. Start claude interactively:

     CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1 claude --teammate-mode tmux --model haiku

4. Ask Claude to create a team:

     Create an agent team with 2 teammates.
     Teammate 1: list all Cargo.toml files in this repo.
     Teammate 2: count the total lines of Rust code.
     Use haiku model for both teammates.

5. Watch for:
   - Does Claude say "View teammates:" in its output?
   - Do you see tmux split-window activity in the shim logs?
   - Check shim logs in real-time from another terminal:
       tail -f ~/.kild/shim/SESSION_ID/shim.log

6. When done, exit claude (Ctrl+C or /exit), then Ctrl+C to detach.

7. Press Ctrl+C here to stop this script and see results.

================================================================
INSTRUCTIONS

echo ""
echo "Shim session for log monitoring: $SESSION_ID"
echo "  tail -f $SHIM_STATE/shim.log"
echo ""
echo "Waiting... (Ctrl+C to finish and see results)"

# Wait for user to finish
while true; do
    sleep 5
    # Check if shim log appeared/grew
    if [ -f "$SHIM_STATE/shim.log" ]; then
        LINE_COUNT=$(wc -l < "$SHIM_STATE/shim.log" 2>/dev/null || echo 0)
        if [ "$LINE_COUNT" -gt 0 ]; then
            LATEST=$(tail -1 "$SHIM_STATE/shim.log" 2>/dev/null | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('fields',{}).get('event','?'))" 2>/dev/null || echo "?")
            echo "  [shim] $LINE_COUNT log entries, latest: $LATEST"
        fi
    fi
done
