# Fleet observability — spec + task tracker

Shared understanding (2026-07-22): **kild = communication tools + observability for
agents. Agent-driven. PRP prescribes process.** Landing/merging/`gh` are agent bash
actions, not engine primitives. Roles (orchestrator/worker/…) are the current prompt
layer (`.pi/agents/*.md`), not core — see [room-mailbox-notes.md](./room-mailbox-notes.md).

## The seven verbs — what kild owns

| Verb | Owner | State |
|---|---|---|
| Launch | kild (mechanism) | ✅ any model/provider, persona\|skill\|generic, optional worktree |
| Communicate | kild (mechanism) | ✅ structured `to`, invite, to-human |
| Observe | kild (mechanism) | 🟡 conversation only — **missing code/git state** |
| Steer/interrupt | kild (mechanism) | 🟡 halt/close; inject = post; no pause-inject-resume |
| Land / merge / gh | **agent (bash)** | ✅ agents do this; kild just enables bash + surfaces paths |
| Gate | **PRP (process)** + kild comms | ✅ agent posts, waits, director answers |
| Resume (fresh) | **agent (bash)** | ✅ new agent reads archived room log, continues |

**Scope boundaries — deliberately NOT kild:**
- Hard-enforced gates (agent *cannot* act without approval) → permission/isolation layer.
- Live-context resume (restore an in-flight session's warm context) → pi/runtime layer.

## Observability is PULL, not push (token discipline)

Two consumers, two modes — this governs every status/log surface:

- **Director agent → compact, pull-based status.** `rooms_status` = participants + last
  posts + a git *summary* + collisions. NEVER stream a sub-agent's transcript or
  token-level detail into the director's context. Trust the agent is working in the
  right place; burning the director's context to *watch* it is the anti-pattern. The
  director pulls a summary when it wants to check, and pulls full logs only to debug.
- **Human (cockpit) → full live stream.** The WS broadcast carries every session's
  events to the human in real time — no token cost, rich view. The human can always
  check live; the agent should not have to.
- **Logs/transcripts** (persisted room log + session transcript) are for after-the-fact
  debugging and the occasional live check — pulled on demand, never auto-injected.

Concretely: the compact status shows a changed-file **count** (not the list) plus the
**actionable collision** (the specific overlapping files) — the full changed-file list
stays in the pull/human layer.

## The one in-scope gap: observability of code state

Agent-driven landing and collision-avoidance are only as good as what the driving
agent can *see*. Today observability is conversation-shaped (`rooms_status` = posts +
participants). To let an agent land work in order and avoid two workstreams clobbering
the same files, surface **git/worktree state** next to the message log.

### Interface (contract)

```ts
// engine/src/kild/worktree-status.ts
export interface WorkstreamGitStatus {
  path: string;                 // effective dir (worktree if set, else repo cwd)
  branch: string | null;
  base: string;                 // base branch compared against (default: main)
  ahead: number;                // commits on branch not in base
  behind: number;               // commits on base not in branch
  dirty: boolean;               // uncommitted changes present
  uncommittedFiles: number;
  changedFiles: string[];       // files changed vs base (committed) — drives collision detection
  conflictsWithBase: boolean | null; // null = undetermined (slice 2)
  error?: string;               // git failure surfaced, never thrown
}

export async function workstreamGitStatus(
  dir: string,
  base?: string,
): Promise<WorkstreamGitStatus>;
```

Git commands (all `execFile`, no shell; failures captured into `error`, never thrown —
observability must never crash the status call):
- branch: `rev-parse --abbrev-ref HEAD`
- base: `symbolic-ref --short refs/remotes/origin/HEAD` → strip `origin/`; fallback `main`
- ahead/behind: `rev-list --left-right --count <base>...HEAD`
- dirty + count: `status --porcelain` (any lines → dirty; count = lines)
- changedFiles: `diff --name-only <base>...HEAD`

### Wiring

- `CompactRoomStatus` (rooms-status.ts) gains optional `git?: WorkstreamGitStatus`.
- The engine computes it per live room when serving `GET /api/rooms/live` (it has the
  paths; the fleet tool stays a pure display). Effective dir = `worktreePath(room.worktree)`
  if set, else `room.cwd`.
- `rooms_status` tool renders the git block so a driving agent sees code state.

### Slices

1. Per-workstream git status (branch/ahead/behind/dirty/changedFiles) in `rooms_status`.
2. Cross-workstream **collision detection**: pairwise `changedFiles` overlap → "A and B
   both touch src/x.ts" warning; plus `conflictsWithBase` via `merge-tree --write-tree`.
3. Cockpit rendering: per-room git badge + collision warnings (app/).

## Roadmap — build / remove / extension

Ordered: build the missing mechanism first (immediate value), then strip the policy
out of the core (make kild framework-agnostic), then package as a pi extension (last).

### Phase 1 — BUILD the missing primitive: code-state observability
- [x] S1: `worktree-status.ts` helper + test (the contract above)
- [x] S1: extend `CompactRoomStatus`; compute git in `/api/rooms/live`; update rooms-status test
- [x] S1: `rooms_status` tool surfaces the git block (rides the JSON; no tool change needed)
- [x] S2: cross-workstream collision (changedFiles overlap) — `computeCollisions`, surfaced
      as compact `collidesWith` (the overlapping files only)
- [x] S2: `conflictsWithBase` via `git merge-tree --write-tree` (exit 0/1/other → false/true/null)
- [x] S2: compact git trims changed-file list to a COUNT (pull-not-push discipline)
- [ ] S3: cockpit — per-room git badge + collision warnings (app/)

### Phase 2 — REMOVE the policy baked into the mechanism (framework-agnostic core)
- [ ] De-hardcode CLI verbs: `kild fleet`→`brain`, `kild room`→`orchestrator,worker`
      become flags/config, not baked-in names
- [ ] Relocate `.pi/agents/*.md` role pack out of core → a PRP pack; ship a minimal
      generic mechanism prompt + one `default` so bare kild still works
  - The generic/`default` prompt should be **Claude Code inspired**: mechanism-level
    "how to operate" guidance (outcome-first comms, verify-before-believe, scope
    discipline, one relayable sentence) drawn from the mined CC discipline — MINUS
    CC's task-list/tmux/role specifics. It teaches how to work, not who to be.
- [ ] Disambiguate engine "worker" (subprocess runtime) vs role "worker" (persona) naming
- [ ] Lifecycle collapse (4 states + 6 guards → running|stopped + visibility flag) — own branch

### Phase 3 — BUILD the pi extension (final)
- [ ] Extract the coordination core (router + types + events + lifecycle + addressing rule)
      behind the existing `RoomDelivery` seam — one core, not a fork
- [ ] kild engine keeps its `RoomDelivery` adapter (subprocess + WS)
- [ ] pi extension provides a second `RoomDelivery` adapter (pi in-session spawn + tools),
      so **pi can drive kild's rooms with no engine running** (slots in as a pi skill/extension)
- [ ] Answer first: does "pi drives kild" even need the extension, or is kild's existing
      CLI/HTTP enough over bash? Only extract on a concrete "pure pi, no engine" need.

### Scope boundaries — never build in kild
- Hard-enforced gates → permission/isolation layer
- Live-context resume → pi/runtime session persistence
- Landing / merge / `gh` → agent bash actions (PRP process)

### Done
- [x] Structured `to` addressing (killed regex bug) — merged to main (`4929e8a`)
- [x] Busy-race verdict: pi queues, no watcher needed
- [x] Live cross-vendor validation (gpt-5.6-sol + MiniMax-M3)
- [x] CC coordination discipline mined; `never delegate understanding` added
