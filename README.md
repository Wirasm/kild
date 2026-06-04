# kild

Run parallel AI coding agents (**pi**) in isolated git worktrees, with a native UI
to watch and steer them. A single-developer tool — built like Lego: small,
composable, swappable parts.

- **Daemon** supervises one `pi --mode rpc` subprocess per worktree (local or VPS).
- **Client** (Tauri + web, and a CLI) renders the structured agent event stream
  (streamed text, tool cards, context/cost) plus a worktree/artifact browser.

pi owns the agent runtime (providers, sessions, compaction, status); kild owns
orchestration (worktrees, projects, supervision, comms, UI). We don't reimplement
what pi already does.

See [`CLAUDE.md`](./CLAUDE.md) for principles and architecture, and
[`.claude/MANIFEST.md`](./.claude/MANIFEST.md) for the audited reuse plan from the
previous Rust implementation (`../kild-old`).

## Status — slice 1 (spike)

Drive `pi --mode rpc` and render its structured event stream:

```bash
cargo run -p kild -- "what files are in this directory?"
```

Requires `pi` 0.78+ on `PATH` and authenticated (`~/.pi/agent/auth.json`).
