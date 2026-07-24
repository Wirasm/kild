# CLAUDE.md

This file guides Claude Code (claude.ai/code) when working in this repository.

## Project Overview

**kild** is an **opinionated orchestrator for pi/Flue agent teams** — one operator
directs many coding agents, each isolated, all visible, the work landed. The
operator is **either a human or an orchestrator agent that stands in for them**;
both drive the same surface (invoke, author, prompt, hand off, land). The "brain"
is not special infrastructure — just an agent holding kild's orchestration tools.
It is a single-developer tool — no multi-tenant complexity. Optimize for one
operator directing many agents. See [`VISION.md`](./VISION.md).

**What kild is — engine, CLI, pi extension. No UI:**

- **Engine** (`engine/`, TypeScript on bun) — the agent runtime, daemon, and CLI.
  Runs `pi` coding-agent sessions in-process via the **coding-agent SDK**, exposes
  them over an HTTP + WebSocket server (Hono), and gives any command-line agent a
  kild runtime through the `kild` CLI.
- **pi extension** (`pi-extension/`) — drives kild rooms/fleets from inside a pi
  session; a thin client over the engine's REST API.
- **The API is the UI contract.** kild ships no UI. The engine's REST + WebSocket
  surface is the contract; external clients — e.g.
  [helm](https://github.com/Wirasm/helm), a separate native UI project — consume
  it. Never add UI code to this repo; evolve the API and let clients follow.

**pi owns cognition** — LLM providers, sessions, context compaction, tool calling,
agent status, and **auth** (it reads `~/.pi/agent/auth.json`, so the user's Claude
Max / ChatGPT OAuth subscriptions work natively). **kild owns orchestration** — who
runs where, how agents hand off, and how work lands: projects, agents, sessions,
worktrees, rooms. We do not reimplement what pi already does.

**Prompts are data, not code.** Agent personalities — including the orchestrator's
own — live in `.pi/agents` / `.claude/agents`, authored and edited by humans *and*
agents. kild ships orchestration mechanism and, at most, a default system prompt;
it never bakes an agent personality into the codebase.

## Stack

- **Runtime / package manager / bundler:** bun.
- **Lint + format:** biome (matches the Flue project's own toolchain).
- **Web framework:** hono (the engine's HTTP + WS server; also Flue's framework).
- **Agent kernel:** `@earendil-works/pi-coding-agent` (the in-process SDK) +
  `@earendil-works/pi-ai`. Engine sessions run on the **coding-agent SDK** directly.
- **Flue** (`@flue/runtime`) is a **committed dependency** for sandbox experiments,
  deploy targets, and the upstream we contribute back to. Its workflows are frozen,
  explicitly invoked experiments—not on the session hot path—and must not grow until
  the fleet layer names a real server or CLI endpoint.

## Core Principles

**Think Before Coding** — Identify the primitive first: what core abstraction does
this touch? Is it sound or itself incomplete? Root cause vs symptom. What is the
minimal change that fixes the root cause? Surface tradeoffs; if multiple
interpretations exist, present them — don't pick silently.

**Lego — Vertical Slices** — Small, composable parts. Each slice owns its types and
logic. Extend by adding a slice, never by editing a god-module. The single most
important boundary: **only the engine knows pi exists** — keep that narrow so pi's
shapes are translated into kild domain types in one place and API/CLI consumers
never couple to them.

**KISS + YAGNI** — Minimum code that solves the problem, nothing speculative. No
config key, feature flag, or "flexibility" without a current caller. Rule of three
before extracting. Would a senior engineer call this overcomplicated? If yes,
simplify.

**Type Safety** — Strict TypeScript. No `any` in production paths; translate at
boundaries into kild domain types. Narrow loose external shapes (pi events) once, at
the boundary, then pass typed data.

**Fail Fast + Explicit Errors** — Never silently swallow an error or broaden a
capability. Surface failures (server → error response, CLI → stderr + non-zero
exit). Document a fallback only when it is intentional and safe.

**Agent-First** — Don't parse pi's prose to infer intent. Give pi tools / structured
output. When kild itself is consumed by an agent (the CLI as a skill), emit
structured output (`--json`), not prose to regex.

**Deliver Signals, Not Sights** — Assume the operator is an agent (a pi driver,
another orchestrator); a human watching a UI client is the special case. Anything that matters
must arrive as an explicit deliverable — a post with recipients, an event to the
opener, a nudge, a ledger entry. UI visibility is a courtesy for whoever happens to
be watching, never the contract. Narration is diagnostic exhaust, never a signal.

**CLI-First** — Every capability is reachable and testable via the `kild` CLI. The
CLI is a first-class interface: it gives a kild runtime to any command-line agent
(via the kild-cli skill), independent of any UI client.

**Green Checks = Done** — `bun run typecheck`, `bun run lint` (biome), and
`bun test` all pass. Iterate until green.

**No Shims, No Backwards Compat** — Greenfield, single dev, no external consumers.
Rename everywhere; never add aliases or wrapper types for compatibility. One name,
one type, one location. If something is unused, delete it.

## Essential Commands

```bash
# Engine (cd engine)
bun install
bun run serve          # start the HTTP+WS engine on :4517 (KILD_PORT to override)
bun run dev            # serve with --watch
bun run cli -- <args>  # the kild CLI, e.g. `bun run cli -- project ls --json`
bun run typecheck      # tsc --noEmit
bun test               # bun's test runner
bun run lint           # biome check src
bun run format         # biome format --write src
```

`pi` must be on `PATH` and authenticated (`~/.pi/agent/auth.json`).

## Architecture

```
kild/
├── CLAUDE.md
├── .claude/
│   └── skills/kild-cli/        # the kild CLI as an Agent Skill (for command-line agents)
├── engine/                     # the kild engine — TypeScript on bun
│   ├── package.json            #   bin: kild → src/cli.ts
│   ├── biome.jsonc
│   ├── src/
│   │   ├── server.ts           #   Hono HTTP (projects/agents/worktrees/open) + WS (sessions) — API server + daemon
│   │   ├── cli.ts              #   the `kild` CLI (project/agent/worktree/run); thin, delegates to the lib
│   │   ├── worker.ts           #   per-session subprocess; ensures the worktree, then createAgentSession({cwd})
│   │   ├── kild/               #   shared library
│   │   │   ├── config.ts       #     default model + state dir (~/.config/kild via $KILD_HOME)
│   │   │   ├── projects.ts     #     project registry (a project = a directory an agent works in)
│   │   │   ├── agents.ts       #     agents read from .kild/.claude/.pi convention dirs
│   │   │   ├── sessions.ts     #     SessionManager: coding-agent SDK sessions → UiEvent stream
│   │   │   ├── room/           #     the Room primitive: registry/router/manager + post_message/invite tools
│   │   │   ├── worktree.ts     #     [kild-owned] git worktree CRUD + ensureWorktree + merge-prune (NO @flue)
│   │   │   ├── run.ts          #     [Flue layer] one-shot run via Flue
│   │   │   ├── brain.ts        #     [Flue layer] operator-mirror agent (kild tools; posts into real Rooms)
│   │   │   └── auth.ts         #     [Flue layer] bridge ~/.pi auth into the Flue runtime
│   │   └── flue/               #   [Flue layer] Flue-promotable mechanisms
│   │       └── worktree-sandbox.ts #  worktree() SandboxFactory (self-contained; upstream contribution)
│   └── src/workflows/          #   [Flue layer] runnable Flue workflows (brain/merge/run demos)
└── pi-extension/               # drive kild rooms/fleets from a pi session (thin REST client)
```

### Architectural boundaries

- **The pi boundary.** pi is touched only in the engine, through the coding-agent
  SDK. pi event shapes are translated to kild domain types (`UiEvent`, `RunOutcome`)
  at that boundary — they never cross the API. This keeps the backbone swappable.
- **SDK substrate, Flue layer.** Interactive sessions run on the
  coding-agent SDK (`createAgentSession`, native auth, `AgentSession.subscribe`
  events). Flue is used for the sandbox abstraction, deploy, and the orchestration
  workflows (brain, merge team) — and is the upstream we contribute to.
- **UI clients are web clients.** Any UI (e.g. helm) reaches the engine over the
  HTTP + WS API only — no privileged path, no shared code with the engine.
- **The worktree boundary.** kild owns worktree *policy* — the `kild/<name>` naming,
  the `$KILD_HOME/worktrees/<name>` path, validation, create-or-attach, merge-prune.
  That logic lives in `engine/src/kild/worktree.ts` and is on the
  session hot path, so it has **no `@flue` import**. The general *mechanism* — a
  `worktree()` SandboxFactory — lives in `engine/src/flue/worktree-sandbox.ts`,
  self-contained (no `kild/*` imports) so it can be lifted into Flue verbatim.
  Worktrees **persist**: a session closing never removes one; only an explicit
  `kild worktree rm` / API delete or the automatic merge-prune does.

### Naming conventions

- Files: `kebab-case.ts` or `snake_case` per surrounding code; one clear name each.
- TypeScript casing: types/interfaces `PascalCase`, functions/vars `camelCase`,
  consts `SCREAMING_SNAKE_CASE`.
- Domain identifiers stay camelCase across the wire (engine ↔ clients), e.g.
  `systemPrompt`, `context_pct` (match the existing `UiEvent` shape exactly).

## The engine API — the contract UI clients consume

- **REST:** `GET /api/projects`, `POST /api/projects`, `GET /api/agents?project=…`,
  `GET /api/worktrees?project=…`, `DELETE /api/worktrees` (`{project,name}`),
  `POST /api/worktrees/prune` (`{project}`), `POST /api/open` (`{path}`, scoped to the
  worktree root).
- **WebSocket** `/ws`: client → `{type:'spawn'|'prompt'|'stop', id, …}`; server →
  `{session, event}` where `event` is a `UiEvent`
  (`model | text | tool_start | tool_end | retry | agent_end | stats | session_end`).
  Session ids are client-generated UUIDs. A `spawn` may carry `worktree` (a name) to
  run the agent in an isolated `kild/<name>` worktree; `SessionInfo` then carries
  `branch` + `worktreePath`.

This REST + WS surface is kild's public contract: external UI clients (e.g.
[helm](https://github.com/Wirasm/helm)) build against it. Treat changes to it as
contract changes.

## Git

`main` is the trunk. Branch for non-trivial work; never force-push `main`. Commit
messages: imperative, human-voiced, no AI attribution.
```
