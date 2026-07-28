#!/bin/bash
# End-to-end migration check: drives the whole reshaped surface against a real engine.
#
# Unit tests prove each part in isolation; this proves the parts agree with each other over
# HTTP, which is where the interesting failures live. It has already earned its keep — it
# found `POST /stop` 500ing on an empty body, and `loadProjects` returning undefined for a
# hand-edited registry and 500ing routes across the engine.
#
# Binds $KILD_E2E_PORT (default 4611) and NEVER 4517: the operator's own engine must survive this running.
#
# Two checks depend on a working model provider (an agent has to reach `ensureWorktree`
# before there is a branch to land). Without auth they report INFO, not FAIL — land itself
# is covered by src/kild/kild-land.test.ts against real temp repos.
#
#   Usage:  ./scripts/e2e.sh          (exit 0 = every check passed)
set -u
# Absolute, resolved BEFORE any cd: $0 is relative to the invocation dir, which we leave.
ENGINE_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SH="$(mktemp -d)"   # a fresh rig each run: stale worktrees would masquerade as findings
PORT="${KILD_E2E_PORT:-4611}"   # never 4517; override if something else holds it
E=http://localhost:$PORT
RIG=$SH/e2e; rm -rf "$RIG"; mkdir -p "$RIG"
PASS=0; FAIL=0
ok(){ PASS=$((PASS+1)); echo "  PASS  $1"; }
no(){ FAIL=$((FAIL+1)); echo "  FAIL  $1  |  got: $2"; }
chk(){ if echo "$2" | grep -q "$3"; then ok "$1"; else no "$1" "$(echo "$2"|head -c 200)"; fi }
chkno(){ if echo "$2" | grep -q "$3"; then no "$1" "$(echo "$2"|head -c 160)"; else ok "$1"; fi }

# --- a real repo with worktrees, including an orphan and a non-kild tree
REPO=$RIG/repo; mkdir -p "$REPO"; cd "$REPO"
git init -q -b main; git config user.email a@b.c; git config user.name t
echo one > a.txt; git add .; git commit -qm init
git worktree add -q -b kild/orphan-work "$RIG/wt-work" >/dev/null 2>&1
( cd "$RIG/wt-work" && echo work > w.txt && git add . && git commit -qm "real work" )
git worktree add -q -b kild/orphan-litter "$RIG/wt-litter" >/dev/null 2>&1
echo "litter: true" > "$RIG/wt-litter/.archon.yaml"
git worktree add -q -b feature/theirs "$RIG/wt-theirs" >/dev/null 2>&1

export KILD_HOME=$RIG/home
mkdir -p "$KILD_HOME"
# resolveKild scans REGISTERED projects, so register the rig repo
printf '{"projects":[{"name":"e2e","path":"%s"}]}' "$REPO" > "$KILD_HOME/projects.json"
# hooks.onClose replaces memory.synthesis — prove the seam fires
mkdir -p "$REPO/.kild"
cat > "$REPO/.kild/config.json" <<JSON
{ "hooks": { "onClose": { "command": ["sh","-c","echo fired kild={{name}} ledger=\$KILD_CLOSE_LEDGER_PATH > $RIG/hook.log"] } } }
JSON

cd "$ENGINE_DIR"
KILD_PORT=$PORT bun run src/server.ts > "$RIG/engine.log" 2>&1 &
EPID=$!
for i in $(seq 1 40); do curl -sf $E/api/health >/dev/null 2>&1 && break; sleep 0.4; done

echo "══ 1. enumerate-from-git: orphans addressable, foreign trees not"
L=$(curl -s "$E/api/kilds?path=$REPO")
chk "orphan with real commits is listed"      "$L" "orphan-work"
chk "orphan with only litter is listed"       "$L" "orphan-litter"
chkno "a non-kild worktree is NOT listed"     "$L" "theirs"
chk "orphans flagged"                          "$L" '"orphan":true'

echo "══ 2. the cheap route is cheap"
chkno "no git block on /api/kilds"             "$L" '"git"'
chkno "no logs on /api/kilds"                  "$L" '"log"'
S=$(curl -s "$E/api/kilds/status?path=$REPO")
chk "git block IS on /status"                  "$S" '"branch"'
chk "changedFiles on /status (collisions derive from this)" "$S" "changedFiles"
R=$(curl -s "$E/api/kilds?path=$REPO&state=reclaimable")
chk "reclaimable on cheap route refuses, names /status" "$R" "status"
RS=$(curl -s "$E/api/kilds/status?path=$REPO&state=reclaimable")
chk "litter tree is reclaimable"               "$RS" "orphan-litter"
chkno "tree with commits is NOT reclaimable"   "$RS" "orphan-work"

echo "══ 3. disposal guards on authored commits, not dirt"
D1=$(curl -s -o "$RIG/d1" -w '%{http_code}' -X DELETE "$E/api/kilds/orphan-work?path=$REPO")
chk "refuses a branch with commits (409)"      "$D1" "409"
chk "refusal explains and offers force"        "$(cat $RIG/d1)" "force"
chk "tree survives the refusal"                "$([ -d "$RIG/wt-work" ] && echo yes)" "yes"
D2=$(curl -s -X DELETE "$E/api/kilds/orphan-litter?path=$REPO")
chk "removes a tree whose only dirt is litter" "$D2" '"ok":true'
chk "litter is listed, not hidden"             "$D2" "archon"
chk "branch is kept"                           "$D2" "branchKept"
chk "tree is gone"                             "$([ -d "$RIG/wt-litter" ] || echo gone)" "gone"
chk "branch really survives"                   "$(cd $REPO && git branch --list 'kild/orphan-litter')" "orphan-litter"

echo "══ 4. create a kild, directed send, attach, inbox"
C=$(curl -s -X POST $E/api/kilds -H 'content-type: application/json' \
  -d "{\"name\":\"e2e\",\"cwd\":\"$REPO\",\"worktree\":\"e2e\",\"agents\":[{\"handle\":\"coder\",\"persona\":\"general\"}],\"kickoff\":{\"to\":[\"coder\"],\"text\":\"start\"}}")
ID=$(echo "$C" | python3 -c 'import json,sys;print(json.load(sys.stdin).get("id",""))' 2>/dev/null)
chk "kild created" "$C" '"ok":true'
BAD=$(curl -s -X POST $E/api/kilds -H 'content-type: application/json' \
  -d "{\"name\":\"x\",\"cwd\":\"$REPO\",\"agents\":[{\"handle\":\"a\"}],\"kickoff\":{\"text\":\"no recipient\"}}")
chk "kickoff naming nobody is rejected"        "$BAD" "error"
A=$(curl -s -X POST $E/api/kilds/$ID/agents/attach -H 'content-type: application/json' -d '{"handle":"honryo"}')
chk "human-driven harness attaches as an ordinary handle" "$A" '"ok":true'
N1=$(curl -s -X POST $E/api/kilds/$ID/messages -H 'content-type: application/json' -d '{"text":"unaddressed"}')
chk "send with no recipient is rejected"       "$N1" "error"
N2=$(curl -s -X POST $E/api/kilds/$ID/messages -H 'content-type: application/json' -d '{"text":"empty to","to":[]}')
chk "send with empty to is rejected"           "$N2" "error"
N3=$(curl -s -X POST $E/api/kilds/$ID/messages -H 'content-type: application/json' -d '{"text":"x","to":["ghost"]}')
chk "unknown recipient names the roster"       "$N3" "coder"
curl -s -X POST $E/api/kilds/$ID/messages -H 'content-type: application/json' -d '{"text":"first","to":["honryo"]}' >/dev/null
curl -s -X POST $E/api/kilds/$ID/messages -H 'content-type: application/json' -d '{"text":"second","to":["honryo"]}' >/dev/null
DR=$(curl -s -X POST $E/api/kilds/$ID/inbox/drain -H 'content-type: application/json' -d '{"handle":"honryo"}')
chk "attached inbox drains both messages"      "$DR" "second"
DR2=$(curl -s -X POST $E/api/kilds/$ID/inbox/drain -H 'content-type: application/json' -d '{"handle":"honryo"}')
chk "second drain reports idle"                "$DR2" '"idle":true'
LIVE=$(curl -s "$E/api/kilds")
chk "ownership axis on a live kild's agents"    "$LIVE" "ownership"
chk "the attached harness is owned by nobody"   "$LIVE" '"ownership":"attached"'
chk "the spawned agent is owned"                "$LIVE" '"ownership":"owned"'  

echo "══ 5. messages are their own cursored resource"
M=$(curl -s "$E/api/kilds/$ID/messages")
chk "seq is present"                           "$M" '"seq"'
MS=$(curl -s "$E/api/kilds/$ID/messages?since=1")
chkno "since= excludes what you already saw"   "$MS" '"seq":1'
chkno "no system notices in the log"           "$M" '"system"'

echo "══ 6. spawn answers instead of warning to a log"
SP=$(curl -s -o "$RIG/sp" -w '%{http_code}' -X POST $E/api/kilds/nope/agents \
  -H 'content-type: application/json' -d '{"handle":"x","persona":"default"}')
chk "spawn into a missing kild returns 404"    "$SP" "404"
chk "and says why"                             "$(cat $RIG/sp)" "no such kild"

echo "══ 7. deleted routes are really gone"
for r in "/api/worktrees" "/api/agents/x/transcript"; do
  chk "404 $r" "$(curl -s -o /dev/null -w '%{http_code}' "$E$r")" "404"
done
for r in "/api/agents/x/prompt" "/api/agents/x/stop" "/api/worktrees/prune"; do
  chk "404 $r" "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$E$r")" "404"
done
chk "GET /api/agents survives (process inventory)" "$(curl -s -o /dev/null -w '%{http_code}' $E/api/agents)" "200"

echo "══ 8. land: dry run touches nothing, execute records a sha"
( cd "$KILD_HOME/worktrees/e2e" 2>/dev/null && echo landed > l.txt && git add . && git commit -qm "kild work" ) 2>/dev/null
BEFORE=$(cd $REPO && git rev-parse HEAD)
LD=$(curl -s "$E/api/kilds/$ID/land")
chk "dry run reports"                          "$LD" "dryRun"
chk "dry run changed nothing"                  "$([ "$BEFORE" = "$(cd $REPO && git rev-parse HEAD)" ] && echo same)" "same"
LX=$(curl -s -X POST "$E/api/kilds/$ID/land")
if echo "$LX" | grep -q '"merged":true'; then
  ok "execute merged"
  chk "and returned a sha"                     "$LX" '"sha"'
else
  echo "  INFO  land: $(echo "$LX"|head -c 160)"
fi

echo "══ 9. stop → archive, ledger facts, hooks.onClose fires"
ST=$(curl -s -X POST $E/api/kilds/$ID/stop)
chk "stop archives"                            "$ST" '"ok":true'
sleep 3
LOG=$(cat "$REPO/.kild/LOG.md" 2>/dev/null)
chkno "ledger has NO prose outcome line"       "$LOG" "outcome:"
chk "ledger records land state"                "$LOG" "landed:"
chk "ledger records code facts"                "$LOG" "code:"
chk "hooks.onClose fired"                      "$(cat $RIG/hook.log 2>/dev/null)" "fired"
chk "hook received the ledger path as a fact"  "$(cat $RIG/hook.log 2>/dev/null)" "LOG.md"
AM=$(curl -s "$E/api/kilds/$ID/messages")
chk "archived kild's log is still readable"    "$AM" "first"
chkno "no state field on the archive"          "$(curl -s $E/api/kilds/archive)" '"state"'

echo "══ 10. no HUMAN, no lead, no lifecycle in the wire"
ALL="$L$S$M$(curl -s $E/api/kilds/archive)"
chkno "no 'stopped' kild-level lifecycle flag" "$ALL" '"stopped":true,"state"'
# (ownership is asserted in section 4, while a live kild still has agents)

kill $EPID 2>/dev/null; wait $EPID 2>/dev/null
echo ""
echo "═══════════════════════════════════════"
echo "  PASS $PASS   FAIL $FAIL"
echo "═══════════════════════════════════════"
[ "$FAIL" -eq 0 ]
