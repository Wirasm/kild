# MANIFEST — what to mirror from `../kild-old`

The previous Rust implementation (`../kild-old`) was audited slice by slice. ~6% of
its Rust is genuine reusable value; the rest is delete-list (multi-agent / fleet /
tmux / hooks), GPUI scaffolding, ceremony, or tests. This is the lift plan.

**Mirror = copy + light edit. Never depend on `../kild-old` at build time.** The
hard-won correctness (libgit2 races, TLS pinning, rebase auto-abort, PID-reuse
guards) is exactly what to copy rather than re-derive.

## Verbatim — the gems (copy, don't rewrite)

| From `../kild-old` | Into | Why |
|---|---|---|
| `crates/kild-core/src/daemon/tofu.rs` | daemon/remote slice | SHA-256 TLS cert pinning — the VPS story. Zero domain coupling. |
| `crates/kild-protocol/src/types.rs` `newtype_string!` macro + SessionId/BranchName/ProjectId | `ids` slice | Pristine compile-time id safety. |
| `crates/kild-daemon/src/protocol/codec.rs` | `kild-daemon` | Generic async JSONL framing (drop the kild-core test import). |
| `crates/kild-daemon/src/tls.rs`, `server/shutdown.rs` | `kild-daemon` | Self-signed cert gen + SIGTERM→CancellationToken. |
| `crates/kild-git/src/{query,cli}.rs`, `status/{commits,worktree}.rs`, `errors.rs`, `validation.rs` | `git` slice | git2/CLI wrappers; rebase auto-abort + main-repo guard are hard-won. ~85% survives; `kild/` naming touched in only 4 spots. |
| `crates/kild-core/src/projects/*`, `files/*` | `project` slice | Textbook encapsulation, no bloat deps. |
| `crates/kild-ui/src/theme.rs` palette | `app/` CSS variables | Tallinn Night → `:root` vars (also in `.claude/PRPs/branding/`). |

## Adapt — light surgery

| From | Change |
|---|---|
| `kild-daemon/src/server/mod.rs` + `session/manager.rs` | keep listener bring-up + session-map/attach/lifecycle skeleton; **replace `pty/` entirely** with the `rpc`-slice child driver (already built) |
| `kild-core/src/daemon/autostart.rs` + `mod.rs` (`find_sibling_binary`, staleness) | keep spawn/poll/backoff/crash-detect flow; swap the 2 PTY IPC calls |
| `kild-core/src/git/handler.rs` (worktree create + libgit2 retry) + `overlaps.rs` | mirror; decouple `overlaps.rs` from `&[Session]` → `&[WorktreeRef]` (one line) |
| `kild-core/src/process/operations.rs` + `types.rs` | keep kill-with-PID-reuse-guard + find-in-directory; **drop `pid_file.rs` + `cleanup.rs`** (daemon owns the PID) |
| `kild-protocol` `IpcConnection` / `AsyncIpcClient` / TLS verifier | mirror transport; **generify over message type** — current PTY message types are throwaway |
| `kild-core/src/cleanup/operations.rs` | keep git-worktree orphan reconciliation; drop the 5-strategy enum |
| `kild-paths`, `kild-config` | mirror, then delete dead methods/knobs (~40% / ~55%: agent/fleet/terminal/hooks/keybindings) |

## Drop — do not carry

- `sessions/` (16.4k — multi-agent / fleet / inbox / hooks), `kild-tmux-shim`,
  `kild-teams`, daemon `hooks/` + `client/` (dead code), `health/` (~90% redundant
  once pi reports status over RPC).
- `state/` Command→Store→Event — CQRS-lite ceremony serving only the deleted GPUI
  UI; the CLI already bypasses it. Call functions directly; in the UI, Tauri
  `emit()` is the event bus.
- All of `kild-ui/{views,terminal,watcher,refresh,teams}` and the PTY message types.

## Already done

- **`rpc` slice** (`kild-core::rpc`) — drives `pi --mode rpc`. New code, not
  mirrored: it is the spine the old PTY supervisor becomes.
- **`project` slice** (`kild-core::project`) — `Project { name, path }`, the
  session's cwd; persisted to `~/.config/kild/projects.json`. New code.
- **Tauri `app/`** — conversation UI (project picker + streamed transcript).
- **`.claude/PRPs/branding/`** — carried over verbatim.

## Build order

1. `rpc` ✅ — drive a pi session, render events.
2. `project` ✅ — a project is a directory; sessions bind to it as cwd.
3. `app/` (Tauri) ✅ — conversation UI (slice 1) + project picker (slice 2).
4. `supervisor` — many pi sessions at once, keyed by session id; aggregate fleet
   state (multi-session sidebar). *next*
5. `git` + `worktree` — mirror the gems; per-repo worktrees within a project.
6. `kild-daemon` — wrap the supervisor behind the JSONL/TLS server skeleton.
