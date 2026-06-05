# kild

A developer cockpit for orchestrating coding-agent teams across projects. The
human plans and reviews; agents (**pi**) automate the coding. kild gives
observability and steering — a native UI to watch and steer many agents at once,
plus a CLI any command-line agent can drive.

- **Engine** (`engine/`, TypeScript on bun) — the agent runtime, daemon, and CLI.
  Runs each agent session in its own subprocess on the pi coding-agent SDK (native
  pi auth, real concurrency), and exposes them over HTTP + WebSocket.
- **Cockpit** (`app/`, Tauri + SvelteKit) — a native window that talks to the
  engine over HTTP + WebSocket. The only Rust is the thin Tauri shell.

pi owns the agent runtime (providers, sessions, compaction, auth); kild owns
orchestration (sessions, worktrees, projects, the cockpit). We don't reimplement
what pi already does.

## Run

```bash
cd app && bun install && bun run tauri dev   # builds the engine, opens the window
```

The engine on its own:

```bash
cd engine && bun install && bun run dev      # HTTP + WS on 127.0.0.1:4517
bun run cli -- run --model anthropic/claude-haiku-4-5 "what files are here?"
```

`pi` must be on PATH and authenticated (`~/.pi/agent/auth.json`).

See [`CLAUDE.md`](./CLAUDE.md) for principles and architecture.
