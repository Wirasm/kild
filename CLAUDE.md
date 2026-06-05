# CLAUDE.md

This file guides Claude Code (claude.ai/code) when working in this repository.

## Project Overview

**kild** runs parallel AI coding agents (**pi**) in isolated git worktrees, with a
native UI to watch and steer them. It is a single-developer tool — no multi-tenant
complexity. Optimize for one power user orchestrating many agents.

**Two halves, one boundary:**

- **Daemon** (Rust, long-lived; local or VPS) — supervises many `pi --mode rpc`
  subprocesses (each is an agent session running in a worktree; a worktree may
  host several), aggregates their structured event streams, and exposes them to
  clients over a socket / TCP+TLS.
- **Client** (Tauri: Rust backend + web frontend; plus a CLI) — renders the
  structured pi events (streamed text, tool cards, context/cost) and a
  worktree/artifact browser.

**pi owns the agent runtime** — LLM providers, sessions, context compaction, tool
calling, agent status. **kild owns orchestration** — worktrees, projects,
supervision, comms, UI. We do not reimplement what pi already does.

Rust (daemon/core/CLI), Tauri + web (UI), `pi` v0.78 as the agent backbone.

## Core Principles

**Think Before Coding** — Identify the primitive first: (1) what core abstraction
does this touch? (2) is the primitive sound, or itself incomplete? (3) root cause
vs symptom — fix where the error originates. (4) what is the minimal change that
fixes the root cause? (5) what does it unlock? Surface tradeoffs. If multiple
interpretations exist, present them — don't pick silently. If something is unclear,
stop, name what's confusing, and ask.

**Lego — Vertical Slices + Traits** — The architecture is small, composable,
swappable parts. Each slice owns its types, logic, and (only where there is a real
swap axis) one trait. Extend by adding a slice or a trait impl, never by editing a
god-module. Easily replaceable, easily extensible. The single most important
boundary: **only the `rpc` slice knows `pi` exists** — keep it narrow so the agent
backbone stays swappable.

**KISS + YAGNI** — Minimum code that solves the problem, nothing speculative. No
config key, trait, feature flag, or "flexibility" without a current caller. Three
similar lines beat a premature helper (rule of three before extracting). If you
wrote 200 lines and it could be 50, rewrite it. Would a senior engineer call this
overcomplicated? If yes, simplify.

**Type Safety (CRITICAL)** — Rust's type system is a feature, not an obstacle. No
`.unwrap()` / `.expect()` in production paths — propagate with `?`. Don't `.clone()`
to satisfy the borrow checker — fix the ownership (`Rc`/`Arc` clones are fine).
Prefer borrowed args (`&str`, `&[T]`, `&Path`). Newtypes for domain identifiers.

**Fail Fast + Explicit Errors** — `thiserror` per slice; explicit errors for
unsupported or unsafe states. Never silently swallow an error or broaden a
capability. Document fallback only when it is intentional and safe.

**Agent-First, Not Parse-After** — Don't parse pi's prose to infer intent. Give pi
tools / structured output to express intent directly (RPC commands, custom tools,
extensions). Add capabilities, not regex filters.

**Backbone-Agnostic** — pi is driven through the `rpc` slice only, over its
documented `--mode rpc` JSON protocol. No pi internals leak into other slices'
types. Swap the backbone by swapping that one slice.

**CLI-First** — Every capability is reachable and testable via the CLI before any
UI work. The CLI is the primary development and testing interface.

**Green Checks = Done** — `cargo fmt --check`, `cargo clippy --all -- -D warnings`,
`cargo test --all`, `cargo build --all` all pass. Iterate until green; no human
babysitting for lint/type fixes.

**No Shims, No Backwards Compat** — Greenfield, single dev, no external consumers.
Rename all usages everywhere; never add type aliases, re-exports, or wrapper types
for compatibility. One name, one type, one location. If something is unused, delete
it completely.

## Essential Commands

```bash
cargo fmt                          # format
cargo fmt --check                  # format check (CI)
cargo clippy --all -- -D warnings  # lint (warnings = errors)
cargo test --all                   # tests
cargo build --all                  # build

cargo run -p kild -- "<prompt>"    # SPIKE: drive `pi --mode rpc`, render events
```

pi must be on `PATH` (`pi --version` → 0.78.x) and authenticated
(`~/.pi/agent/auth.json`).

## Architecture

### Directory layout (current)

```
kild/
├── Cargo.toml                     # workspace (central dep versions)
├── CLAUDE.md
├── .claude/
│   ├── MANIFEST.md                # what to mirror from ../kild-old (audited reuse plan)
│   └── PRPs/branding/             # brand + vision + design system (Tallinn Night)
├── app/                           # Tauri 2 + SvelteKit conversation UI (its own workspace)
│   ├── src/routes/+page.svelte    #   sidebar (projects/sessions/new) + per-session transcript
│   └── src-tauri/src/lib.rs       #   commands (sessions/projects/agents) + per-session event pump
└── crates/
    ├── kild-core/                 # orchestration library — vertical slices
    │   └── src/
    │       ├── lib.rs
    │       ├── paths.rs           # kild's state paths (~/.config/kild; $KILD_HOME override)
    │       ├── rpc/               # the ONLY pi boundary — drives `pi --mode rpc`
    │       │   ├── rpc_types.rs   #   RpcCommand (in) + PiOutput/DeltaKind events (out)
    │       │   ├── rpc_client.rs  #   PiRpcSession + PiRpcWriter (split for concurrent drive)
    │       │   ├── rpc_run.rs     #   run_to_completion → RunOutcome (one-shot, for CLI/daemon)
    │       │   └── rpc_errors.rs
    │       ├── project/           # a project is a directory an agent works in (session cwd)
    │       │   ├── project_types.rs   #   Project { name, path }
    │       │   ├── project_store.rs    #   persisted to ~/.config/kild/projects.json
    │       │   └── project_errors.rs
    │       └── agent/             # a reusable role: name + system prompt (read from convention dirs)
    │           ├── agent_types.rs     #   Agent { name, system_prompt }
    │           ├── agent_store.rs      #   scans .kild/.claude/.pi agents; --append-system-prompt
    │           └── agent_errors.rs
    └── kild/                      # CLI — the primary, skill-friendly interface
        └── src/
            ├── main.rs           #   parse → dispatch → exit code; no logic here
            └── commands/         #   thin presentation layer (delegates to kild-core)
                ├── mod.rs        #     clap Cli + dispatch router
                ├── project.rs    #     kild project {ls,add,rm}
                ├── agent.rs      #     kild agent {ls,show}
                └── run.rs        #     kild run — one-shot agent task, --json for skills
```

The CLI is **thin by contract**: it parses args, delegates to a `kild-core` slice,
and formats output. No business logic lives in `crates/kild`. Reads go to stdout
(plain or `--json`); progress/errors go to stderr; non-zero exit means failure — so
an agent can drive kild over the Bash tool and parse stdout cleanly.

Planned slices (see `.claude/MANIFEST.md`): `worktree`, `git`, `comms`, `config`,
`forge`, plus the `kild-daemon` binary (extracting the in-app supervisor for
persistence / VPS — and the owner of live, shareable sessions for `kild session …`).
(`rpc`, `project`, `agent`, the in-app multi-session registry, the Tauri `app/`, and
the `project`/`agent`/`run` CLI now exist.)

### Naming conventions

Every file is uniquely grep-able — `rpc_client` returns exactly one file.

- **`{slice}_{role}.rs`** — `rpc_client.rs`, `rpc_types.rs`, `rpc_errors.rs`. Never
  bare `client.rs` / `types.rs` / `errors.rs` (the old repo's ambiguity we left
  behind).
- Rust casing: modules/files `snake_case`, types/traits `PascalCase`, fns/vars
  `snake_case`, consts `SCREAMING_SNAKE_CASE`.
- Collocated tests: `#[cfg(test)] mod tests` in-file, or `{slice}_{role}_tests.rs`
  for large suites.
- Trait implementers named by role: `<Name>Backend`, `<Name>Forge`. Factory keys
  lowercase + stable (`"github"`).

### Architectural boundaries

- **Slice seal.** A slice owns its domain types and logic. Cross-slice access goes
  through the owning slice's public API, never its internals.
- **The pi boundary.** pi is touched only in `kild-core::rpc`. No `PiOutput` / pi
  JSON shapes appear in `worktree` / `project` / UI types — translate at the
  boundary into kild domain types. This is what keeps the backbone swappable.
- **Dependency direction is inward to contracts.** Concrete impls depend on
  trait/config/util layers, not on each other. UI never imports supervision
  internals; the supervisor never imports forge policy.
- **Add capability by trait impl + registration first**, not cross-module rewrites.
  Introduce a shared abstraction only after the rule of three, with a real caller.

## pi Integration

pi is driven via **`pi --mode rpc`** — a JSON protocol over stdin/stdout, strict
JSONL (LF-framed). Commands in (`RpcCommand`), structured events out (`PiOutput`:
`message_update` text deltas, `tool_execution_*`, `agent_end`, and `response` for
`get_session_stats` → context/cost). Reference: pi's own `docs/rpc.md` in the
installed package (`~/.npm-global/lib/node_modules/@earendil-works/pi-coding-agent/docs`).

pi owns providers, sessions/persistence, compaction, and status — **do not
reimplement these**. The unit is the **agent session** (one `pi --mode rpc`
subprocess), not the worktree: a worktree is just the session's `cwd`, and one
worktree may host several sessions. Subprocess-per-session gives crash isolation
for free.

## Structured Logging

`tracing` with JSON output. Event format `{layer}.{domain}.{action}_{state}` — e.g.
`daemon.supervisor.spawn_started`, `core.rpc.command_failed`. Layers map to crates
(`core`, `daemon`, `cli`, `ui`). States: `_started` / `_completed` / `_failed` /
`_skipped`. Always name fields (`%e` Display for errors, `?val` Debug for structs);
never log bare `{:?}`. Every user-visible op gets a `_started` / `_completed` pair;
failures emit `_failed` with the error attached.

## Brand / Design

Brand, vision, personas, and the **Tallinn Night** design system live in
`.claude/PRPs/branding/` (carried from the old repo). The palette + HTML mockups are
the source of truth for the Tauri UI's look. Port the old `theme.rs` palette → CSS
variables (see `.claude/MANIFEST.md`).

## Git

`main` is the trunk. Branch for non-trivial work; never force-push `main`. Commit
messages: imperative, human-voiced, no AI attribution.
