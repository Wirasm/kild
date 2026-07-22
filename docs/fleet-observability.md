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
- [x] De-hardcode CLI verbs: `kild fleet` → `--agent ?? default` (no baked `brain`);
      `kild room` → one general-purpose `default` participant when no `--participants`
- [x] kild's system prompt: a generic mechanism prompt every session gets, on top of
      everything (`engine/src/kild/mechanism-prompt.ts`, wired in `worker.ts`) — outcome-first,
      verify-before-believe, scope discipline, blocked→escalate, use real tools. Teaches how
      to work, not who to be; room-comms part is conditional so a bare `kild run` is fine too.
- [x] Relocate `.pi/agents/*.md` role pack out of core → parked in `tmp/agents/`; kild ships
      no roles, personas come from the project (`.claude/agents` / `.pi/agents`). Bare kild =
      `default` (mechanism prompt only).
- [ ] Disambiguate engine "worker" (subprocess runtime) vs role "worker" (persona) naming
- [ ] Lifecycle collapse (4 states + 6 guards → running|stopped + visibility flag) — own branch

### Phase 2.5 — BUILD missing CLI primitives (found by test-driving kild from an external agent)

Test-drive (2026-07-22): parked kild's roles, bridged prp-core into the project, drove kild
from an outside Claude Code session over bash. What works vs what's missing:

- ✅ `kild run` (one-shot: spawn a pi agent to completion, `--json` result) — fully drivable.
- ✅ `kild project` / `kild agent ls|show` / `kild worktree` — drivable.
- ✅ prp-core AGENTS load once symlinked into the project's `.claude/agents` (kild discovers
      project `.claude/agents`/`.pi/agents`). **Gap: the plugin layout (`prp-core/agents`,
      `prp-core/skills`) is NOT auto-discovered — kild has no plugin-awareness; you bridge
      via the standard `.claude/agents` path.**
- ❌ **The FLEET is not drivable from the CLI.** Per kild's own `kild-cli` skill: "live,
      steerable sessions are driven from the cockpit UI over the WebSocket, not the CLI." The
      `room`/`fleet` CLI verbs are INTERACTIVE (open a WebSocket, stream, read stdin) — not
      scriptable primitives. An external agent (or a human over bash) cannot launch-detached,
      observe, or steer a room. This blocks the whole "agent drives the fleet" goal.

**CLI primitives — built + verified by driving kild from an external session:**
- [x] `kild rooms [--json]` — live rooms + git/collision status (surfaces `/api/rooms/live`)
- [x] `kild room open … --detach` → prints a room id, returns immediately
- [x] `kild room post <id> <text>` — steer an existing room from a separate call
- [x] `kild room close <id>` — close a specific room by id
- [ ] fleet-level equivalents (open/observe/steer many rooms non-interactively)
- [ ] Bridge prp SKILLS discovery: agents bridge via `.claude/agents` (works); skills need
      `.claude/skills` (kild ships `kild-cli` there) — confirm prp-* skills load for the DRIVER
      (driver gets no skills profile; falls back to pi default discovery)

**Config/plugin system — BUILT + verified.** `.kild/config.json` (project) and
`$KILD_HOME/config.json` (global): a `plugins: ["./prp-core"]` entry contributes the
plugin's `agents/` + `skills/`; `agentPaths`/`skillPaths` add explicit dirs. Absolute or
`~/…` paths load from ANYWHERE on the system. Config skills are wired into EVERY session's
resource loader (`worker.ts`), so an invited agent — not just the lead — can load
`prp-implement` when the orchestrator asks. Verified live: a minimax session in the kild
repo (config → prp-core) reported all prp-* skills available. `resolvePluginPaths` in
`config.ts`; `agentDirs` is now config-aware. This is how the orchestrator-as-skill model
works: the driver is a `default` agent that loads the `prp-orchestrate` skill and sequences
fresh skill-loading agents by message. kild-cli skill updated with the room primitives.

Note: the observability data layer (Phase 1) exists but is reachable only by an in-fleet
agent (rooms_status tool) or HTTP — an EXTERNAL driver has no CLI window into it. These
primitives are what make "me or an agent drives the fleet" real over bash.

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
- [x] Coordination discipline mined; `never delegate understanding` added to orchestrator
