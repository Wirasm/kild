---
name: kild-brain
description: |
  Honryū — KILD fleet supervisor. Invoke to start, assess, or direct the fleet brain.

  TRIGGERS - Use this skill when user says:
  - "start my honryu", "launch the brain", "start the kild orchestrator", "initialize honryu"
  - "start my fleet manager", "boot honryu", "wake up the brain", "start honryu for me"
  - "assess the fleet", "what's the fleet doing", "fleet status"
  - "plan the next wave", "what should we work on next", "spin up more kilds"
  - "merge queue", "what's ready to merge", "land the PRs"
  - "direct the workers", "what should <branch> do next"
  - "honryu", "brain", "ask the brain"

context: fork
agent: kild-brain
allowed-tools: Bash, Read, Write, Glob, Grep, Task
---

Your task: $ARGUMENTS

---

## Current Fleet State

!`kild list --json 2>/dev/null || echo '{"sessions":[],"fleet_summary":{"total":0}}'`

## Brain Memory

!`cat ~/.kild/brain/state.json 2>/dev/null || echo 'No prior state.'`

!`tail -30 ~/.kild/brain/sessions/$(date +%Y-%m-%d).md 2>/dev/null || echo 'No session log today.'`

---

## If the task is to START or LAUNCH Honryū

When the user's intent is to start, launch, boot, or initialize Honryū as a persistent daemon session (not to ask it a question), follow this protocol:

**Check current status and act:**

```bash
STATUS=$(kild list --json 2>/dev/null | jq -r '.sessions[] | select(.branch == "honryu") | .status // empty' 2>/dev/null)

case "$STATUS" in
  active)
    # Already running — just report
    echo "Honryū is already running."
    ;;
  stopped)
    # Exists but stopped — reopen headlessly and orient.
    # --initial-prompt delivers the orientation via PTY stdin on startup.
    # No sleep needed — PTY stdin is kernel-buffered until the agent reads it.
    kild open honryu --no-attach --resume --initial-prompt "You've been restarted by the Tōryō. Orient yourself: check kild list --json, ~/.kild/brain/state.json, and today's session log. Then greet the Tōryō and summarize the fleet state."
    echo "Honryū restarted."
    ;;
  *)
    # No session — create fresh on main branch.
    # --initial-prompt delivers the first instruction via PTY stdin on startup.
    kild create honryu --daemon --main --agent claude --yolo --note "Honryū fleet supervisor" --initial-prompt "You are Honryū, the KILD fleet supervisor. You have just been initialized by the Tōryō. Orient yourself: run kild list --json, read ~/.kild/brain/state.json and today's session log if they exist. Then greet the Tōryō and report fleet state. You are running on the main branch — do not create worktrees for yourself."
    echo "Honryū initialized."
    ;;
esac
```

Report back what action was taken. Tell the user that Honryū is now running as a daemon session — they can monitor it by opening an attach window with `kild attach honryu` or by watching events arrive in this session.

---

## Otherwise — Ad-hoc fleet query or directive

Use the kild CLI and your available tools to complete the task. Log significant decisions to `~/.kild/brain/sessions/YYYY-MM-DD.md`. Update `~/.kild/brain/state.json` after fleet changes.

```bash
# Fleet overlap map (check before wave planning)
kild overlaps 2>/dev/null || echo 'No overlaps detected.'
```
