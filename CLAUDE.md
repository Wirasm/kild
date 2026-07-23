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

**Two halves, one boundary:**

- **Engine** (`engine/`, TypeScript on bun) — the agent runtime, daemon, and CLI.
  Runs `pi` coding-agent sessions in-process via the **coding-agent SDK**, exposes
  them over an HTTP + WebSocket server (Hono), and gives any command-line agent a
  kild runtime through the `kild` CLI.
- **Cockpit** (`app/`, Tauri shell + SvelteKit) — a native window hosting the web
  UI. The frontend talks to the engine over HTTP + WebSocket. The Rust in the shell
  is the irreducible Tauri bootstrap only — no logic lives there.

**pi owns cognition** — LLM providers, sessions, context compaction, tool calling,
agent status, and **auth** (it reads `~/.pi/agent/auth.json`, so the user's Claude
Max / ChatGPT OAuth subscriptions work natively). **kild owns orchestration** — who
runs where, how agents hand off, and how work lands: projects, agents, sessions,
worktrees, the cockpit. We do not reimplement what pi already does.

**Prompts are data, not code.** Agent personalities — including the orchestrator's
own — live in `.pi/agents` / `.claude/agents`, authored and edited by humans *and*
agents. kild ships orchestration mechanism and, at most, a default system prompt;
it never bakes an agent personality into the codebase.

## Stack

- **Runtime / package manager / bundler:** bun.
- **Lint + format:** biome (matches the Flue project's own toolchain).
- **Web framework:** hono (the engine's HTTP + WS server; also Flue's framework).
- **Agent kernel:** `@earendil-works/pi-coding-agent` (the in-process SDK) +
  `@earendil-works/pi-ai`. The cockpit/CLI use the **coding-agent SDK** directly.
- **Flue** (`@flue/runtime`) is a **committed dependency** for sandbox experiments,
  deploy targets, and the upstream we contribute back to. Its workflows are frozen,
  explicitly invoked experiments—not on the session hot path—and must not grow until
  the fleet layer names a real server or CLI endpoint.
- **UI:** SvelteKit (Svelte 5 runes) + adapter-static, in a Tauri 2 shell.

## Core Principles

**Think Before Coding** — Identify the primitive first: what core abstraction does
this touch? Is it sound or itself incomplete? Root cause vs symptom. What is the
minimal change that fixes the root cause? Surface tradeoffs; if multiple
interpretations exist, present them — don't pick silently.

**Lego — Vertical Slices** — Small, composable parts. Each slice owns its types and
logic. Extend by adding a slice, never by editing a god-module. The single most
important boundary: **only the engine knows pi exists** — keep that narrow so pi's
shapes are translated into kild domain types in one place and the cockpit/CLI never
couple to them.

**KISS + YAGNI** — Minimum code that solves the problem, nothing speculative. No
config key, feature flag, or "flexibility" without a current caller. Rule of three
before extracting. Would a senior engineer call this overcomplicated? If yes,
simplify.

**Type Safety** — Strict TypeScript. No `any` in production paths; translate at
boundaries into kild domain types. Narrow loose external shapes (pi events) once, at
the boundary, then pass typed data.

**Fail Fast + Explicit Errors** — Never silently swallow an error or broaden a
capability. Surface failures (server → error response, CLI → stderr + non-zero exit,
UI → banner). Document a fallback only when it is intentional and safe.

**Agent-First** — Don't parse pi's prose to infer intent. Give pi tools / structured
output. When kild itself is consumed by an agent (the CLI as a skill), emit
structured output (`--json`), not prose to regex.

**Deliver Signals, Not Sights** — Assume the operator is an agent (a pi driver,
another orchestrator); the cockpit human is the special case. Anything that matters
must arrive as an explicit deliverable — a post with recipients, an event to the
opener, a nudge, a ledger entry. UI visibility is a courtesy for whoever happens to
be watching, never the contract. Narration is diagnostic exhaust, never a signal.

**CLI-First** — Every capability is reachable and testable via the `kild` CLI. The
CLI is a first-class secondary interface: it gives a kild runtime to any
command-line agent (via the kild-cli skill), independent of the UI.

**Green Checks = Done** — `bun run typecheck`, `bun run lint` (biome), and the FE's
`bun run check` (svelte-check) all pass. Iterate until green.

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
bun run lint           # biome check src
bun run format         # biome format --write src

# Cockpit (cd app)
bun run tauri dev      # ONE command: builds the engine sidecar, starts engine + frontend, opens the window
bun run dev            # frontend-only vite dev server on :1420
bun run check          # svelte-check
bun run tauri build    # release .app — bundles the engine as a sidecar the shell spawns
```

Dev loop: `cd app && bun run tauri dev`. Its `beforeDevCommand` compiles the engine
to a binary (Tauri's `externalBin` requires it present) and runs the live engine +
frontend concurrently; the shell opens the window. In a **release** build the shell
spawns the bundled engine sidecar (`#[cfg(not(debug_assertions))]` in `lib.rs`); in
dev the engine is the live `bun run dev` process instead.

`pi` must be on `PATH` and authenticated (`~/.pi/agent/auth.json`).

## Architecture

```
kild/
├── CLAUDE.md
├── .claude/
│   ├── skills/kild-cli/        # the kild CLI as an Agent Skill (for command-line agents)
│   └── PRPs/branding/          # brand + vision + Tallinn Night design system
├── engine/                     # the kild engine — TypeScript on bun
│   ├── package.json            #   bin: kild → src/cli.ts
│   ├── biome.jsonc
│   ├── src/
│   │   ├── server.ts           #   Hono HTTP (projects/agents/worktrees/open) + WS (sessions) — cockpit backend + daemon
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
└── app/                        # the cockpit — Tauri 2 + SvelteKit
    ├── src/
    │   ├── lib/api.ts          #   engine client (REST + EngineSocket over WS)
    │   ├── lib/types.ts        #   shared FE types (Project, Agent, Session, UiEvent, Item)
    │   ├── lib/components/      #   Sidebar, Topbar, Ledger, ToolCard, Composer, ProjectModal, Dropdown
    │   ├── lib/theme/tokens.css #   Tallinn Night design tokens
    │   └── routes/+page.svelte #   sidebar (projects/sessions/new) + per-session transcript
    └── src-tauri/              #   thin Rust shell (webview host only; no logic)
```

### Architectural boundaries

- **The pi boundary.** pi is touched only in the engine, through the coding-agent
  SDK. pi event shapes are translated to kild domain types (`UiEvent`, `RunOutcome`)
  at that boundary — they never reach the cockpit. This keeps the backbone swappable.
- **SDK substrate, Flue layer.** Interactive cockpit/CLI sessions run on the
  coding-agent SDK (`createAgentSession`, native auth, `AgentSession.subscribe`
  events). Flue is used for the sandbox abstraction, deploy, and the orchestration
  workflows (brain, merge team) — and is the upstream we contribute to.
- **The cockpit is a web client.** The frontend reaches the engine over HTTP + WS
  only; the Tauri shell hosts the webview and nothing else.
- **The worktree boundary.** kild owns worktree *policy* — the `kild/<name>` naming,
  the `$KILD_HOME/worktrees/<name>` path, validation, create-or-attach, merge-prune,
  the cockpit UI. That logic lives in `engine/src/kild/worktree.ts` and is on the
  session hot path, so it has **no `@flue` import**. The general *mechanism* — a
  `worktree()` SandboxFactory — lives in `engine/src/flue/worktree-sandbox.ts`,
  self-contained (no `kild/*` imports) so it can be lifted into Flue verbatim.
  Worktrees **persist**: a session closing never removes one; only an explicit
  `kild worktree rm` / UI action or the automatic merge-prune does.

### Naming conventions

- Files: `kebab-case.ts` or `snake_case` per surrounding code; one clear name each.
- TypeScript casing: types/interfaces `PascalCase`, functions/vars `camelCase`,
  consts `SCREAMING_SNAKE_CASE`.
- Domain identifiers stay camelCase across the wire (engine ↔ cockpit), e.g.
  `systemPrompt`, `context_pct` (match the existing `UiEvent` shape exactly).

## The cockpit ↔ engine protocol

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

## Brand / Design

Brand, vision, personas, and the **Tallinn Night** design system live in
`.claude/PRPs/branding/`. `app/src/lib/theme/tokens.css` is the palette source of
truth for the UI.

## Git

`main` is the trunk. Branch for non-trivial work; never force-push `main`. Commit
messages: imperative, human-voiced, no AI attribution.
```
