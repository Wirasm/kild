# kild

> **kild v2 — a ground-up rewrite, under active development.** kild was rebuilt
> from Rust to **TypeScript on bun** (engine + CLI). Expect churn while v2
> stabilises. The original Rust implementation (v1) is preserved at the
> [`v1-rust`](https://github.com/Wirasm/kild/releases/tag/v1-rust)
> tag and the [`archive/rust`](https://github.com/Wirasm/kild/tree/archive/rust)
> branch.

An engine for orchestrating coding-agent teams across projects. The human plans
and reviews; agents (**pi**) automate the coding. kild gives observability and
steering — an agent runtime with an HTTP + WebSocket API, plus a CLI any
command-line agent can drive.

- **Engine** (`engine/`, TypeScript on bun) — the agent runtime, daemon, and CLI.
  Runs each agent session in its own subprocess on the pi coding-agent SDK (native
  pi auth, real concurrency), and exposes them over HTTP + WebSocket.
- **The API is the UI contract.** kild ships no UI of its own — the engine's
  REST + WebSocket surface is the contract any client consumes. External clients
  such as [helm](https://github.com/Wirasm/helm) (a native UI) talk to the engine
  over that API.
- **pi extension** (`pi-extension/`) — drive kild rooms and fleets from a pi
  session; a thin client over the engine's REST API.

pi owns the agent runtime (providers, sessions, compaction, auth); kild owns
orchestration (sessions, worktrees, projects, rooms). We don't reimplement
what pi already does.

## Run

```bash
cd engine && bun install && bun run dev      # HTTP + WS on 127.0.0.1:4517
bun run cli -- run --model anthropic/claude-haiku-4-5 "what files are here?"
```

`pi` must be on PATH and authenticated (`~/.pi/agent/auth.json`).

See [`CLAUDE.md`](./CLAUDE.md) for principles and architecture.
