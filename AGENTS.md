# AGENTS.md — kild

kild is an orchestration engine for agent teams: rooms (parallel workstreams in git
worktrees), kild sessions (pi-SDK agent processes), and the observability around them.
It is engine + CLI + pi extension only — there is no UI in this repo; UI clients (helm)
consume the REST/WS API on `localhost:4517`.

## Boundaries (violations are bugs)

- **Only the engine knows pi exists.** pi shapes are translated into kild domain types
  in one place; the CLI and API clients never see them.
- **kild ships mechanism, never intelligence.** No personas, no process prompts in code —
  personas are data (`.pi/agents/`, config `plugins:`); the only shipped prompt is the
  generic mechanism prompt.
- **kild knows nothing about prp** (or any intelligence layer). Integration happens via
  generic config paths (`plugins`, `memory.dir`) only.
- Vocabulary: `../GLOSSARY.md` is law — operator, room, participant, persona, kild
  session vs pi session, archived.

## How to work here

- Layout: `engine/src/` (server.ts, cli.ts, worker.ts, `kild/` domain slices, `operator/`
  tools), `pi-extension/`, `docs/`, `HANDOVER.md` (canonical state doc).
- The engine is usually RUNNING from this checkout — never start/stop servers; do code
  work in a git worktree on a branch, PR to `main`.
- Gates: `bun test`, `bun run typecheck`, `bun run lint` from `engine/` — all green
  before any PR.
- Conventional commits, written as a human — no AI attribution, no Co-Authored-By.
- Vertical slices: a change owns its types and logic; extend by adding a slice, never
  by growing a god-module. No shims, no backwards-compat aliases.
