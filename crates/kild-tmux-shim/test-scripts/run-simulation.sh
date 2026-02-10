#!/bin/bash
# Level 3: Automated simulation test
# Replays Claude Code's exact 14-step agent team spawn sequence against the real daemon.
# No Claude Code binary needed — just the shim + daemon.
#
# Usage: ./run-simulation.sh
# Exit: 0 = passed, 1 = failed

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
RESULTS_DIR="/tmp/kild-shim-test/simulation-$(date +%Y%m%d-%H%M%S)"
KILD="$REPO_ROOT/target/debug/kild"
BRANCH="shim-sim-test"

mkdir -p "$RESULTS_DIR"

cleanup() {
    echo ""
    echo "--- Cleanup ---"
    "$KILD" destroy "$BRANCH" --force 2>/dev/null || true
    "$KILD" daemon stop 2>/dev/null || true
    echo "Results saved to: $RESULTS_DIR"
}
trap cleanup EXIT

echo "=== Level 3: Simulation Test ==="
echo "Results: $RESULTS_DIR"
echo ""

# Step 1: Build
echo "--- Building ---"
cargo build -p kild -p kild-tmux-shim --quiet 2>&1
echo "OK: Build succeeded"

# Step 2: Start daemon
echo ""
echo "--- Starting daemon ---"
"$KILD" daemon stop 2>/dev/null || true
"$KILD" daemon start 2>&1
DAEMON_PID=$("$KILD" daemon status --json 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('pid','unknown'))" 2>/dev/null || echo "unknown")
echo "OK: Daemon running (PID: $DAEMON_PID)"

# Step 3: Create test kild
echo ""
echo "--- Creating test kild ---"
"$KILD" create "$BRANCH" --daemon --no-agent 2>&1
SESSION_ID=$("$KILD" status "$BRANCH" --json 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
echo "OK: Session ID: $SESSION_ID"

# Step 4: Verify shim state
echo ""
echo "--- Verifying shim state ---"
SHIM_STATE="$HOME/.kild/shim/$SESSION_ID"
if [ ! -f "$SHIM_STATE/panes.json" ]; then
    echo "FAIL: panes.json not found at $SHIM_STATE"
    exit 1
fi
echo "OK: Shim state initialized at $SHIM_STATE"

# Step 5: Run simulation
echo ""
echo "--- Running Claude Code spawn simulation ---"
TMUX="/Users/rasmus/.kild/daemon.sock,$DAEMON_PID,0" \
TMUX_PANE="%0" \
KILD_SHIM_SESSION="$SESSION_ID" \
KILD_SHIM_LOG=1 \
PATH="$HOME/.kild/bin:$PATH" \
bash "$REPO_ROOT/crates/kild-tmux-shim/tests/fixtures/claude-code-spawn-simulation.sh" 2>&1 | tee "$RESULTS_DIR/simulation.log"

SIM_EXIT=${PIPESTATUS[0]}

# Step 6: Save artifacts
echo ""
echo "--- Saving artifacts ---"
cp "$SHIM_STATE/panes.json" "$RESULTS_DIR/panes-after.json" 2>/dev/null || true
cp "$SHIM_STATE/shim.log" "$RESULTS_DIR/shim.log" 2>/dev/null || true

# Step 7: Analyze shim log
echo ""
echo "--- Shim Log Analysis ---"
if [ -f "$RESULTS_DIR/shim.log" ]; then
    SPLIT_COUNT=$(grep -c "split_window_completed" "$RESULTS_DIR/shim.log" || echo 0)
    SEND_COUNT=$(grep -c "send_keys_completed" "$RESULTS_DIR/shim.log" || echo 0)
    KILL_COUNT=$(grep -c "kill_pane_completed" "$RESULTS_DIR/shim.log" || echo 0)
    IPC_CREATE=$(grep -c "ipc.create_session_completed" "$RESULTS_DIR/shim.log" || echo 0)
    IPC_DESTROY=$(grep -c "ipc.destroy_session_completed" "$RESULTS_DIR/shim.log" || echo 0)
    IPC_WRITE=$(grep -c "ipc.write_stdin_completed" "$RESULTS_DIR/shim.log" || echo 0)

    echo "  split-window completed: $SPLIT_COUNT (expect 3)"
    echo "  send-keys completed:    $SEND_COUNT (expect 3)"
    echo "  kill-pane completed:    $KILL_COUNT (expect 3)"
    echo "  IPC create_session:     $IPC_CREATE (expect 3)"
    echo "  IPC destroy_session:    $IPC_DESTROY (expect 3)"
    echo "  IPC write_stdin:        $IPC_WRITE (expect 3)"
else
    echo "  WARNING: No shim log found"
fi

# Final result
echo ""
if [ "$SIM_EXIT" -eq 0 ]; then
    echo "=== PASSED ==="
    exit 0
else
    echo "=== FAILED (exit code: $SIM_EXIT) ==="
    exit 1
fi
