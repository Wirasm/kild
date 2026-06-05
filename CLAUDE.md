# CLAUDE.md

This file guides Claude Code (claude.ai/code) when working in this repository.

## Project Overview

**kild** is a developer cockpit for orchestrating coding-agent teams across
projects. The human **plans and reviews**; agents **automate the coding**. kild
gives observability and steering where it matters — and, over time, a merge agent
team, GitHub-integrated reviews, and a learning operator-mirror "brain." It is a
single-developer tool — no multi-tenant complexity. Optimize for one power user
orchestrating many agents.

**Two halves, one boundary:**

- **Engine** (`engine/`, TypeScript on bun) — the agent runtime, daemon, and CLI.
  Runs `pi` coding-agent sessions in-process via the **coding-agent SDK**, exposes
  them over an HTTP + WebSocket server (Hono), and gives any command-line agent a
  kild runtime through the `kild` CLI.
- **Cockpit** (`app/`, Tauri shell + SvelteKit) — a native window hosting the web
  UI. The frontend talks to the engine over HTTP + WebSocket. The Rust in the shell
  is the irreducible Tauri bootstrap only — no logic lives there.

**pi owns the agent runtime** — LLM providers, sessions, context compaction, tool
calling, agent status, and **auth** (it reads `~/.pi/agent/auth.json`, so the
user's Claude Max / ChatGPT OAuth subscriptions work natively). **kild owns
orchestration** — projects, agents, sessions, worktrees, rooms, the cockpit. We do
not reimplement what pi already does.

## Stack

- **Runtime / package manager / bundler:** bun.
- **Lint + format:** biome (matches the Flue project's own toolchain).
- **Web framework:** hono (the engine's HTTP + WS server; also Flue's framework).
- **Agent kernel:** `@earendil-works/pi-coding-agent` (the in-process SDK) +
  `@earendil-works/pi-ai`. The cockpit/CLI use the **coding-agent SDK** directly.
- **Flue** (`@flue/runtime`) is a **complementary layer** — its sandbox abstraction,
  deploy targets, and workflow model — and the upstream we contribute back to. It is
  not the runtime the hot path flows through.
- **UI:** SvelteKit (Svelte 5 runes) + adapter-static, in a Tauri 2 shell.

## Core Principles

**Think Before Coding** — Identify the primitive first: what core abstraction does
this touch? Is it sound or itself incomplete? Root cause vs symptom. What is the
minimal change that fixes the root cause? Surface tradeoffs; if multiple
interpretations exist, present them — don't pick silently.

**Lego — Vertical Slices** — Small, composable, swappable parts. Each slice owns its
types and logic. Extend by adding a slice, never by editing a god-module. The
single most important boundary: **only the engine knows pi exists** — keep that
narrow so the agent backbone stays swappable.

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
│   │   ├── server.ts           #   Hono HTTP (projects/agents) + WS (sessions) — cockpit backend + daemon
│   │   ├── cli.ts              #   the `kild` CLI (project/agent/run); thin, delegates to the lib
│   │   └── kild/               #   shared library
│   │       ├── config.ts       #     default model + state dir (~/.config/kild via $KILD_HOME)
│   │       ├── projects.ts     #     project registry (a project = a directory an agent works in)
│   │       ├── agents.ts       #     agents read from .kild/.claude/.pi convention dirs
│   │       ├── sessions.ts     #     SessionManager: coding-agent SDK sessions → UiEvent stream
│   │       ├── worktree.ts     #     git worktrees (+ a Flue local() sandbox over a worktree)
│   │       ├── run.ts          #     [Flue layer] one-shot run via Flue
│   │       ├── rooms.ts        #     [Flue layer] agent-to-agent rooms (peer comms)
│   │       ├── brain.ts        #     [Flue layer] operator-mirror agent (kild capabilities as tools)
│   │       ├── observability.ts#     [Flue layer] observe() → cockpit event log
│   │       └── auth.ts         #     [Flue layer] bridge ~/.pi auth into the Flue runtime
│   └── src/workflows/          #   [Flue layer] runnable Flue workflows (rooms/brain/merge/run demos)
└── app/                        # the cockpit — Tauri 2 + SvelteKit
    ├── src/
    │   ├── lib/api.ts          #   engine client (REST + EngineSocket over WS)
    │   ├── lib/types.ts        #   shared FE types (Project, Agent, Session, UiEvent, Item)
    │   ├── lib/components/      #   Sidebar, Topbar, Ledger, ToolCard, Composer
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
  workflows (rooms, brain, merge team) — and is the upstream we contribute to.
- **The cockpit is a web client.** The frontend reaches the engine over HTTP + WS
  only; the Tauri shell hosts the webview and nothing else.

### Naming conventions

- Files: `kebab-case.ts` or `snake_case` per surrounding code; one clear name each.
- TypeScript casing: types/interfaces `PascalCase`, functions/vars `camelCase`,
  consts `SCREAMING_SNAKE_CASE`.
- Domain identifiers stay camelCase across the wire (engine ↔ cockpit), e.g.
  `systemPrompt`, `context_pct` (match the existing `UiEvent` shape exactly).

## The cockpit ↔ engine protocol

- **REST:** `GET /api/projects`, `POST /api/projects`, `GET /api/agents?project=…`.
- **WebSocket** `/ws`: client → `{type:'spawn'|'prompt'|'stop', id, …}`; server →
  `{session, event}` where `event` is a `UiEvent`
  (`model | text | tool_start | tool_end | retry | agent_end | stats | session_end`).
  Session ids are client-generated UUIDs.

## Brand / Design

Brand, vision, personas, and the **Tallinn Night** design system live in
`.claude/PRPs/branding/`. `app/src/lib/theme/tokens.css` is the palette source of
truth for the UI.

## Git

`main` is the trunk. Branch for non-trivial work; never force-push `main`. Commit
messages: imperative, human-voiced, no AI attribution.
```
