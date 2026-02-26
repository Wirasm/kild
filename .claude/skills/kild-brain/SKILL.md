---
name: kild-brain
description: |
  Honryū — KILD fleet supervisor. Invoke to start, assess, or direct the fleet brain.

  TRIGGERS - Use this skill when user says:
  - "start my honryu", "launch the brain", "start the kild orchestrator", "initialize honryu"
  - "start my fleet manager", "boot honryu", "wake up the brain", "start honryu for me"
  - "assess the fleet", "what's the fleet doing", "fleet status"
  - "plan the next wave", "what should we work on next", "spin up more kilds"
  - "start the next wave", "execute the wave", "launch the wave", "run the wave plan"
  - "merge queue", "what's ready to merge", "land the PRs"
  - "direct the workers", "what should <branch> do next"
  - "honryu", "brain", "ask the brain"

context: fork
agent: kild-brain
allowed-tools: Bash, Read, Write, Glob, Grep, Task
---

User request: $ARGUMENTS

---

## Current Fleet State

!`kild list --json 2>/dev/null || echo '{"sessions":[],"fleet_summary":{"total":0}}'`

---

## Protocol

**Honryū ALWAYS runs as a persistent daemon kild session, never as an ad-hoc fork.**

This skill is a thin router. It ensures the honryu daemon session exists, delivers
the user's request to it, and reports back. It does NOT execute brain logic itself.

### Step 1: Ensure Honryū daemon is running

```bash
STATUS=$(kild list --json 2>/dev/null | jq -r '.sessions[] | select(.branch == "honryu") | .status // empty' 2>/dev/null)

case "$STATUS" in
  active)
    echo "Honryū is already running."
    ;;
  stopped)
    kild open honryu --no-attach --resume --initial-prompt "You've been restarted by the Tōryō. Orient yourself: check kild list --json, ~/.kild/brain/state.json, today's session log, and .kild/wave-plan.json (if it exists, mention it). Then greet the Tōryō and summarize the fleet state."
    echo "Honryū restarted."
    ;;
  *)
    kild create honryu --daemon --main --agent claude --yolo --note "Honryū fleet supervisor" --initial-prompt "You are Honryū, the KILD fleet supervisor. You have just been initialized by the Tōryō. Orient yourself: run kild list --json, read ~/.kild/brain/state.json, today's session log, and .kild/wave-plan.json if they exist. If a wave plan exists, mention it. Then greet the Tōryō and report fleet state. You are running on the main branch — do not create worktrees for yourself."
    echo "Honryū initialized."
    ;;
esac
```

### Step 2: Deliver the user's request

If the user just wanted to start/launch Honryū, skip this step — report that it's running
and tell them to use `kild attach honryu` to see it.

If the user has a directive or question (e.g., "assess the fleet", "execute the wave",
"tell workers to commit"), inject it into the running Honryū session:

```bash
kild inject honryu "<user's request here>"
```

**IMPORTANT:** Do NOT execute brain logic (stopping workers, creating kilds, running
overlaps, etc.) from this forked skill. All fleet operations must go through the
persistent honryu daemon session so it maintains context and history.

### Step 3: Report back

Tell the user:
- What action was taken (created / restarted / already running)
- That the request was injected (if applicable)
- How to monitor: `kild attach honryu`
