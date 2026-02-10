#!/bin/bash
# Verify the tmux shim environment is correctly set up.
# Run this INSIDE a daemon kild (via `kild attach <branch>`) before starting claude.
#
# Usage: bash verify-shim-env.sh

set -uo pipefail

ERRORS=0

check() {
    local label="$1"
    local ok="$2"
    if [ "$ok" = "true" ]; then
        echo "OK:   $label"
    else
        echo "FAIL: $label"
        ERRORS=$((ERRORS + 1))
    fi
}

echo "=== Shim Environment Verification ==="
echo ""

# $TMUX must be set
if [ -n "${TMUX:-}" ]; then
    check "\$TMUX is set: $TMUX" "true"
else
    check "\$TMUX is set" "false"
fi

# $TMUX_PANE must be set
if [ -n "${TMUX_PANE:-}" ]; then
    check "\$TMUX_PANE is set: $TMUX_PANE" "true"
else
    check "\$TMUX_PANE is set" "false"
fi

# $KILD_SHIM_SESSION must be set
if [ -n "${KILD_SHIM_SESSION:-}" ]; then
    check "\$KILD_SHIM_SESSION is set: $KILD_SHIM_SESSION" "true"
else
    check "\$KILD_SHIM_SESSION is set" "false"
fi

# tmux on PATH should be our shim
TMUX_PATH=$(which tmux 2>/dev/null || echo "not found")
if [[ "$TMUX_PATH" == *".kild/bin/tmux"* ]]; then
    check "tmux on PATH is KILD shim: $TMUX_PATH" "true"
else
    check "tmux on PATH is KILD shim (got: $TMUX_PATH)" "false"
fi

# Shim responds to -V
TMUX_VERSION=$(tmux -V 2>&1 || true)
if [ "$TMUX_VERSION" = "tmux 3.4" ]; then
    check "tmux -V returns 'tmux 3.4': $TMUX_VERSION" "true"
else
    check "tmux -V returns 'tmux 3.4' (got: $TMUX_VERSION)" "false"
fi

# Shim state directory exists
if [ -n "${KILD_SHIM_SESSION:-}" ]; then
    SHIM_DIR="$HOME/.kild/shim/$KILD_SHIM_SESSION"
    if [ -f "$SHIM_DIR/panes.json" ]; then
        PANE_COUNT=$(python3 -c "import json; d=json.load(open('$SHIM_DIR/panes.json')); print(len(d['panes']))" 2>/dev/null || echo "?")
        check "Shim state exists ($PANE_COUNT pane(s))" "true"
    else
        check "Shim state exists at $SHIM_DIR" "false"
    fi
fi

# display-message works
PANE_ID=$(tmux display-message -p "#{pane_id}" 2>&1 || true)
if [[ "$PANE_ID" == %* ]]; then
    check "display-message returns pane ID: $PANE_ID" "true"
else
    check "display-message returns pane ID (got: $PANE_ID)" "false"
fi

# list-panes works
PANES=$(tmux list-panes -t "kild_0:0" -F "#{pane_id}" 2>&1 || true)
if [[ "$PANES" == %* ]]; then
    check "list-panes works: $(echo "$PANES" | tr '\n' ' ')" "true"
else
    check "list-panes works (got: $PANES)" "false"
fi

echo ""
if [ $ERRORS -gt 0 ]; then
    echo "=== $ERRORS CHECK(S) FAILED ==="
    echo "Fix these before running claude with agent teams."
    exit 1
else
    echo "=== ALL CHECKS PASSED ==="
    echo ""
    echo "Ready! Start claude with:"
    echo "  CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1 claude --teammate-mode tmux --model haiku"
    echo ""
    echo "Then ask:"
    echo "  Create an agent team with 2 teammates. One lists all .rs files, the other counts test functions."
    echo ""
    echo "Monitor shim logs:"
    echo "  tail -f $HOME/.kild/shim/$KILD_SHIM_SESSION/shim.log"
    exit 0
fi
