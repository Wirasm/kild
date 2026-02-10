#!/bin/bash
# diagnose-shim.sh — Diagnose and smoke-test the KILD tmux shim setup.
#
# Usage (from the kild project root):
#   ./crates/kild-tmux-shim/test-scripts/diagnose-shim.sh
#
# This script:
#   1. Checks prerequisites (binaries, daemon, shim symlink)
#   2. Destroys any existing shim-test kild
#   3. Creates a fresh daemon-mode kild with --no-agent (bare shell)
#   4. Inspects the daemon PTY environment via IPC
#   5. Runs the tmux shim smoke test commands
#   6. Reports results
#   7. Cleans up

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

PASS=0
FAIL=0
WARN=0

pass() { echo -e "  ${GREEN}PASS${NC} $1"; PASS=$((PASS+1)); }
fail() { echo -e "  ${RED}FAIL${NC} $1"; FAIL=$((FAIL+1)); }
warn() { echo -e "  ${YELLOW}WARN${NC} $1"; WARN=$((WARN+1)); }
info() { echo -e "  ${BLUE}INFO${NC} $1"; }
header() { echo -e "\n${BOLD}=== $1 ===${NC}"; }

KILD_BIN="${KILD_BIN:-target/debug/kild}"
SHIM_BIN="${SHIM_BIN:-target/debug/kild-tmux-shim}"
KILD_DIR="${HOME}/.kild"
BRANCH="shim-diag"

# ---------------------------------------------------------------------------
header "1. Prerequisites"
# ---------------------------------------------------------------------------

# Check kild binary
if [[ -x "$KILD_BIN" ]]; then
    pass "kild binary exists at $KILD_BIN"
else
    fail "kild binary not found at $KILD_BIN (run: cargo build --all)"
    exit 1
fi

# Check shim binary
if [[ -x "$SHIM_BIN" ]]; then
    pass "kild-tmux-shim binary exists at $SHIM_BIN"
else
    fail "kild-tmux-shim binary not found at $SHIM_BIN (run: cargo build --all)"
    exit 1
fi

# Check shim symlink
SHIM_LINK="${KILD_DIR}/bin/tmux"
if [[ -L "$SHIM_LINK" ]]; then
    LINK_TARGET=$(readlink "$SHIM_LINK")
    pass "shim symlink exists: $SHIM_LINK -> $LINK_TARGET"
    if [[ -x "$SHIM_LINK" ]]; then
        pass "shim symlink is executable"
    else
        fail "shim symlink is NOT executable"
    fi
else
    warn "shim symlink missing at $SHIM_LINK (kild create --daemon will create it)"
fi

# Check daemon socket
DAEMON_SOCK="${KILD_DIR}/daemon.sock"
if [[ -S "$DAEMON_SOCK" ]]; then
    pass "daemon socket exists at $DAEMON_SOCK"
else
    fail "daemon socket not found at $DAEMON_SOCK"
    echo ""
    echo "  Start the daemon first:"
    echo "    $KILD_BIN daemon start --foreground"
    echo ""
    exit 1
fi

# Check daemon status
DAEMON_STATUS=$("$KILD_BIN" daemon status --json 2>/dev/null || echo '{"error":"failed"}')
if echo "$DAEMON_STATUS" | grep -q '"running"'; then
    pass "daemon is running"
    info "daemon status: $DAEMON_STATUS"
else
    fail "daemon not responding"
    info "daemon status: $DAEMON_STATUS"
    exit 1
fi

# ---------------------------------------------------------------------------
header "2. Clean up previous test kild"
# ---------------------------------------------------------------------------

if "$KILD_BIN" status "$BRANCH" --json 2>/dev/null | grep -q '"id"'; then
    info "destroying existing kild '$BRANCH'..."
    "$KILD_BIN" destroy "$BRANCH" --force 2>/dev/null || true
    pass "previous test kild destroyed"
else
    info "no existing kild '$BRANCH' found"
fi

# ---------------------------------------------------------------------------
header "3. Create daemon-mode kild (bare shell)"
# ---------------------------------------------------------------------------

# Use --no-agent so we get a bare shell to inspect, not Claude Code
CREATE_OUTPUT=$("$KILD_BIN" create "$BRANCH" --daemon --no-agent 2>&1) || {
    fail "kild create failed"
    echo "$CREATE_OUTPUT"
    exit 1
}
pass "kild create $BRANCH --daemon --no-agent succeeded"
echo "$CREATE_OUTPUT" | sed 's/^/  /'

# Give the daemon a moment to start the PTY
sleep 1

# Check session status
SESSION_JSON=$("$KILD_BIN" status "$BRANCH" --json 2>/dev/null || echo '{}')
info "session JSON: $SESSION_JSON"

# Extract daemon_session_id from the session JSON
DAEMON_SID=$(echo "$SESSION_JSON" | python3 -c "
import json, sys
data = json.load(sys.stdin)
agents = data.get('agents', [])
if agents:
    print(agents[0].get('daemon_session_id', ''))
" 2>/dev/null || echo "")

if [[ -n "$DAEMON_SID" ]]; then
    pass "daemon_session_id: $DAEMON_SID"
else
    fail "no daemon_session_id in session JSON"
fi

# ---------------------------------------------------------------------------
header "4. Check shim symlink (should be created by kild create)"
# ---------------------------------------------------------------------------

if [[ -L "$SHIM_LINK" ]]; then
    LINK_TARGET=$(readlink "$SHIM_LINK")
    pass "shim symlink: $SHIM_LINK -> $LINK_TARGET"
    if [[ -x "$LINK_TARGET" ]] || [[ -x "$SHIM_LINK" ]]; then
        pass "shim target is executable"
    else
        fail "shim target is NOT executable: $LINK_TARGET"
    fi
else
    fail "shim symlink still missing after kild create"
fi

# ---------------------------------------------------------------------------
header "5. Check shim state directory"
# ---------------------------------------------------------------------------

SESSION_ID=$(echo "$SESSION_JSON" | python3 -c "
import json, sys
print(json.load(sys.stdin).get('id', ''))
" 2>/dev/null || echo "")

SHIM_STATE_DIR="${KILD_DIR}/shim/${SESSION_ID}"
if [[ -d "$SHIM_STATE_DIR" ]]; then
    pass "shim state dir exists: $SHIM_STATE_DIR"

    if [[ -f "$SHIM_STATE_DIR/panes.json" ]]; then
        pass "panes.json exists"
        info "contents:"
        cat "$SHIM_STATE_DIR/panes.json" | python3 -m json.tool 2>/dev/null | sed 's/^/    /'
    else
        fail "panes.json missing"
    fi

    if [[ -f "$SHIM_STATE_DIR/panes.lock" ]]; then
        pass "panes.lock exists"
    else
        warn "panes.lock missing"
    fi
else
    fail "shim state dir missing: $SHIM_STATE_DIR"
fi

# ---------------------------------------------------------------------------
header "6. Inspect daemon PTY environment"
# ---------------------------------------------------------------------------

# Send 'env | grep -E "TMUX|KILD|PATH"' to the daemon PTY and read output
# We do this by writing to the PTY's stdin and reading from its output
# via the daemon IPC protocol.

info "sending environment inspection command to daemon PTY..."

# We need to send commands to the PTY and capture output.
# The simplest approach: use the daemon IPC directly to write stdin,
# wait, then read output.

if [[ -n "$DAEMON_SID" ]]; then
    # Write a command to the PTY stdin via IPC
    # Using socat to send JSONL to the daemon socket
    REQUEST_ID="diag-$(date +%s)"

    # Send env dump command
    ENV_CMD='echo "===DIAG_START==="; echo "TMUX=$TMUX"; echo "TMUX_PANE=$TMUX_PANE"; echo "KILD_SHIM_SESSION=$KILD_SHIM_SESSION"; echo "PATH=$PATH"; which tmux 2>/dev/null || echo "tmux: not found"; echo "===DIAG_END==="'

    # Base64 encode the command + newline
    ENCODED=$(printf '%s\n' "$ENV_CMD" | base64)

    WRITE_MSG=$(cat <<EOF
{"id":"${REQUEST_ID}","type":"write_stdin","session_id":"${DAEMON_SID}","data":"${ENCODED}"}
EOF
)

    # Send via socat
    if command -v socat &>/dev/null; then
        RESPONSE=$(echo "$WRITE_MSG" | socat - UNIX-CONNECT:"$DAEMON_SOCK" 2>/dev/null || echo "socat failed")
        if echo "$RESPONSE" | grep -q '"type":"ok"'; then
            pass "wrote env inspection command to PTY"
        else
            warn "write_stdin response: $RESPONSE"
        fi

        # Wait for output
        sleep 1

        # Read PTY output via IPC
        READ_MSG=$(cat <<EOF
{"id":"${REQUEST_ID}-read","type":"read_output","session_id":"${DAEMON_SID}"}
EOF
)
        PTY_OUTPUT=$(echo "$READ_MSG" | socat - UNIX-CONNECT:"$DAEMON_SOCK" 2>/dev/null || echo "")

        if [[ -n "$PTY_OUTPUT" ]]; then
            info "raw daemon response:"
            echo "$PTY_OUTPUT" | head -20 | sed 's/^/    /'

            # Try to extract and decode the output
            OUTPUT_DATA=$(echo "$PTY_OUTPUT" | python3 -c "
import json, sys, base64
try:
    data = json.loads(sys.stdin.read())
    if 'output' in data:
        print(base64.b64decode(data['output']).decode('utf-8', errors='replace'))
    elif 'data' in data:
        print(base64.b64decode(data['data']).decode('utf-8', errors='replace'))
    else:
        print(json.dumps(data, indent=2))
except:
    pass
" 2>/dev/null || echo "")

            if [[ -n "$OUTPUT_DATA" ]]; then
                info "decoded PTY output:"
                echo "$OUTPUT_DATA" | sed 's/^/    /'
            fi
        fi
    else
        warn "socat not installed — skipping PTY environment inspection"
        info "install with: brew install socat"
        info "alternatively, run 'kild attach $BRANCH' and type:"
        info "  echo \$TMUX"
        info "  echo \$TMUX_PANE"
        info "  which tmux"
    fi
else
    warn "no daemon_session_id — skipping PTY environment inspection"
fi

# ---------------------------------------------------------------------------
header "7. Tmux shim smoke test (from outside)"
# ---------------------------------------------------------------------------

# These tests run the shim binary directly with the correct env vars,
# simulating what would happen inside the daemon PTY.

info "running shim commands with simulated environment..."

# Set up the environment as the daemon PTY would have it
export TMUX="${DAEMON_SOCK},$$,0"
export TMUX_PANE="%0"
export KILD_SHIM_SESSION="$SESSION_ID"
export PATH="${KILD_DIR}/bin:$PATH"

# Test 1: tmux -V
echo ""
info "test: tmux -V"
VERSION_OUTPUT=$("$SHIM_BIN" -V 2>&1) || true
if echo "$VERSION_OUTPUT" | grep -q "tmux"; then
    pass "tmux -V -> $VERSION_OUTPUT"
else
    fail "tmux -V returned: $VERSION_OUTPUT"
fi

# Test 2: display-message pane_id
info "test: display-message -p '#{pane_id}'"
PANE_ID_OUTPUT=$("$SHIM_BIN" display-message -p '#{pane_id}' 2>&1) || true
if [[ "$PANE_ID_OUTPUT" == "%0" ]]; then
    pass "display-message pane_id -> $PANE_ID_OUTPUT"
else
    fail "display-message pane_id -> '$PANE_ID_OUTPUT' (expected '%0')"
fi

# Test 3: display-message session:window
info "test: display-message -p '#{session_name}:#{window_index}'"
WINDOW_OUTPUT=$("$SHIM_BIN" display-message -p '#{session_name}:#{window_index}' 2>&1) || true
if echo "$WINDOW_OUTPUT" | grep -qE "^kild_0:0$"; then
    pass "display-message window -> $WINDOW_OUTPUT"
else
    fail "display-message window -> '$WINDOW_OUTPUT' (expected 'kild_0:0')"
fi

# Test 4: list-panes
info "test: list-panes -F '#{pane_id}'"
PANES_OUTPUT=$("$SHIM_BIN" list-panes -t "kild_0:0" -F '#{pane_id}' 2>&1) || true
if echo "$PANES_OUTPUT" | grep -q "^%0$"; then
    pass "list-panes shows %0"
    info "all panes: $(echo "$PANES_OUTPUT" | tr '\n' ' ')"
else
    fail "list-panes output: '$PANES_OUTPUT' (expected '%0')"
fi

# Test 5: split-window (creates new daemon PTY)
info "test: split-window -t %0 -h -P -F '#{pane_id}'"
SPLIT_OUTPUT=$("$SHIM_BIN" split-window -t "%0" -h -P -F '#{pane_id}' 2>&1)
SPLIT_EXIT=$?
if [[ $SPLIT_EXIT -eq 0 ]] && echo "$SPLIT_OUTPUT" | grep -qE "^%[0-9]+$"; then
    NEW_PANE=$(echo "$SPLIT_OUTPUT" | head -1)
    pass "split-window created pane: $NEW_PANE"

    # Test 6: list-panes should now show 2 panes
    info "test: list-panes after split"
    PANES_AFTER=$("$SHIM_BIN" list-panes -t "kild_0:0" -F '#{pane_id}' 2>&1) || true
    PANE_COUNT=$(echo "$PANES_AFTER" | wc -l | tr -d ' ')
    if [[ "$PANE_COUNT" -ge 2 ]]; then
        pass "list-panes shows $PANE_COUNT panes after split"
    else
        fail "list-panes shows $PANE_COUNT panes (expected >= 2)"
    fi

    # Test 7: send-keys
    info "test: send-keys -t $NEW_PANE 'echo hello' Enter"
    SEND_OUTPUT=$("$SHIM_BIN" send-keys -t "$NEW_PANE" "echo hello from shim test" Enter 2>&1)
    SEND_EXIT=$?
    if [[ $SEND_EXIT -eq 0 ]]; then
        pass "send-keys succeeded"
    else
        fail "send-keys failed (exit $SEND_EXIT): $SEND_OUTPUT"
    fi

    # Test 8: select-pane (styling — should be no-op success)
    info "test: select-pane -t $NEW_PANE -P 'bg=default,fg=blue'"
    STYLE_OUTPUT=$("$SHIM_BIN" select-pane -t "$NEW_PANE" -P "bg=default,fg=blue" 2>&1)
    if [[ $? -eq 0 ]]; then
        pass "select-pane styling succeeded"
    else
        fail "select-pane styling failed: $STYLE_OUTPUT"
    fi

    # Test 9: select-pane title
    info "test: select-pane -t $NEW_PANE -T 'researcher'"
    TITLE_OUTPUT=$("$SHIM_BIN" select-pane -t "$NEW_PANE" -T "researcher" 2>&1)
    if [[ $? -eq 0 ]]; then
        pass "select-pane title succeeded"
    else
        fail "select-pane title failed: $TITLE_OUTPUT"
    fi

    # Test 10: set-option pane border
    info "test: set-option -p -t $NEW_PANE pane-border-style 'fg=blue'"
    SETOPT_OUTPUT=$("$SHIM_BIN" set-option -p -t "$NEW_PANE" pane-border-style "fg=blue" 2>&1)
    if [[ $? -eq 0 ]]; then
        pass "set-option succeeded"
    else
        fail "set-option failed: $SETOPT_OUTPUT"
    fi

    # Test 11: select-layout (no-op)
    info "test: select-layout main-vertical"
    LAYOUT_OUTPUT=$("$SHIM_BIN" select-layout -t "kild_0:0" main-vertical 2>&1)
    if [[ $? -eq 0 ]]; then
        pass "select-layout succeeded"
    else
        fail "select-layout failed: $LAYOUT_OUTPUT"
    fi

    # Test 12: resize-pane (no-op)
    info "test: resize-pane -t %0 -x 30%"
    RESIZE_OUTPUT=$("$SHIM_BIN" resize-pane -t "%0" -x "30%" 2>&1)
    if [[ $? -eq 0 ]]; then
        pass "resize-pane succeeded"
    else
        fail "resize-pane failed: $RESIZE_OUTPUT"
    fi

    # Test 13: has-session
    info "test: has-session -t kild_0"
    "$SHIM_BIN" has-session -t "kild_0" 2>&1
    if [[ $? -eq 0 ]]; then
        pass "has-session kild_0 -> exists"
    else
        fail "has-session kild_0 -> not found"
    fi

    # Test 14: kill-pane
    info "test: kill-pane -t $NEW_PANE"
    KILL_OUTPUT=$("$SHIM_BIN" kill-pane -t "$NEW_PANE" 2>&1)
    if [[ $? -eq 0 ]]; then
        pass "kill-pane succeeded"
    else
        fail "kill-pane failed: $KILL_OUTPUT"
    fi

    # Verify pane is gone
    PANES_FINAL=$("$SHIM_BIN" list-panes -t "kild_0:0" -F '#{pane_id}' 2>&1) || true
    if ! echo "$PANES_FINAL" | grep -q "$NEW_PANE"; then
        pass "pane $NEW_PANE removed from list"
    else
        fail "pane $NEW_PANE still in list after kill"
    fi
else
    fail "split-window failed (exit $SPLIT_EXIT): $SPLIT_OUTPUT"
    warn "skipping dependent tests (send-keys, kill-pane, etc.)"
fi

# ---------------------------------------------------------------------------
header "8. Final shim state"
# ---------------------------------------------------------------------------

if [[ -f "$SHIM_STATE_DIR/panes.json" ]]; then
    info "final panes.json:"
    cat "$SHIM_STATE_DIR/panes.json" | python3 -m json.tool 2>/dev/null | sed 's/^/    /'
fi

# ---------------------------------------------------------------------------
header "9. Cleanup"
# ---------------------------------------------------------------------------

info "destroying test kild '$BRANCH'..."
"$KILD_BIN" destroy "$BRANCH" --force 2>/dev/null && pass "test kild destroyed" || warn "destroy failed (may need manual cleanup)"

# ---------------------------------------------------------------------------
header "Summary"
# ---------------------------------------------------------------------------

echo ""
echo -e "  ${GREEN}$PASS passed${NC}, ${RED}$FAIL failed${NC}, ${YELLOW}$WARN warnings${NC}"
echo ""

if [[ $FAIL -eq 0 ]]; then
    echo -e "  ${GREEN}${BOLD}All checks passed!${NC} The shim is ready for real agent team testing."
    echo ""
    echo "  Next steps:"
    echo "    1. kild create agent-team-test --daemon"
    echo "    2. kild attach agent-team-test"
    echo "    3. Inside the PTY, verify: echo \$TMUX && which tmux"
    echo "    4. Run: claude"
    echo "    5. Tell Claude: 'Create a team with 2 teammates for a simple task'"
    echo ""
else
    echo -e "  ${RED}${BOLD}Some checks failed.${NC} Fix the issues above before testing with real agent teams."
fi

exit $FAIL
