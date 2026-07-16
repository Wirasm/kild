import { type ChildProcess, spawn } from 'node:child_process';

import type { UiEvent } from './events.ts';
import { worktreePath, worktreeRef } from './worktree.ts';

export interface SpawnRequest {
  model?: string;
  cwd?: string;
  agent?: string;
  projectName?: string;
  /** Worktree *name* (not path). Absent → run in the project's main checkout.
   *  Present → the worker ensures `kild/<name>` and runs the agent there. Two
   *  sessions naming the same worktree share its tree (attach); different names
   *  split. The worker creates-or-attaches from `cwd` (the repo). */
  worktree?: string;
  /** Extra environment for the worker — opaque to the manager. Room participants use
   *  it to carry `KILD_ROOM` / `KILD_PARTICIPANT`; it never overrides the `KILD_*`
   *  vars the manager itself sets. */
  env?: Record<string, string>;
}

/** Metadata the cockpit shows for a session — including ones the CLI started. */
export interface SessionInfo {
  id: string;
  model?: string;
  cwd?: string;
  agent?: string;
  projectName?: string;
  origin: 'ui' | 'cli';
  /** The selected worktree's name (echoed for the worktrees-in-use cross-check). */
  worktree?: string;
  /** `kild/<name>` ref, when the session runs in a worktree (else undefined). */
  branch?: string;
  /** Deterministic on-disk worktree path, when the session runs in a worktree. */
  worktreePath?: string;
}

/** A message broadcast to every connected client. */
export type Outbound = { session: string; event: UiEvent } | { sessions: SessionInfo[] };

/** Control-line callbacks for a session's worker — used by the RoomManager to route
 *  a participant's `post_message` / `invite_agent` back into its room. A bare
 *  (non-room) session passes none, so the control lines are simply never emitted. */
export interface SessionCallbacks {
  onMessage?: (m: { text: string; to?: string[]; implicit?: boolean }) => void;
  onInvite?: (i: { name: string; agent?: string; model?: string }) => void;
  onCloseRoom?: (c: { reason?: string }) => void;
}

/**
 * One agent session = one worker subprocess (see `worker.ts`). The worker is the
 * same binary re-invoked with `KILD_ROLE=worker`; we talk to it over its stdio:
 * `UiEvent` JSONL out, prompt/stop JSONL in.
 */
class PiSession {
  private readonly child: ChildProcess;
  private buf = '';

  constructor(req: SpawnRequest, onEvent: (event: UiEvent) => void, callbacks?: SessionCallbacks) {
    this.child = spawn(process.argv[0] as string, process.argv.slice(1), {
      env: {
        ...process.env,
        ...req.env, // extra worker env (e.g. room membership); our KILD_* win below
        KILD_ROLE: 'worker',
        KILD_MODEL: req.model ?? '',
        KILD_CWD: req.cwd ?? process.cwd(),
        KILD_AGENT: req.agent ?? '',
        // The worktree *name*; the worker ensures it from KILD_CWD (the repo).
        KILD_WORKTREE: req.worktree ?? '',
      },
      stdio: ['pipe', 'pipe', 'inherit'],
    });

    this.child.stdout?.on('data', (chunk: Buffer) => {
      this.buf += chunk.toString();
      const lines = this.buf.split('\n');
      this.buf = lines.pop() ?? ''; // keep the incomplete trailing line
      for (const raw of lines) {
        const line = raw.trim();
        if (!line) continue;
        let parsed: {
          kind?: string;
          text?: string;
          to?: string[];
          implicit?: boolean;
          name?: string;
          agent?: string;
          model?: string;
          reason?: string;
        };
        try {
          parsed = JSON.parse(line);
        } catch {
          continue; // non-JSON line from the worker (a stray log); ignore.
        }
        // A room participant's `post_message` / `invite_agent` arrive as control lines
        // routed back to its room, not the transcript. Everything else is a UiEvent.
        if (parsed.kind === 'message_out') {
          callbacks?.onMessage?.({
            text: parsed.text ?? '',
            to: parsed.to,
            implicit: parsed.implicit,
          });
        } else if (parsed.kind === 'invite' && parsed.name) {
          callbacks?.onInvite?.({ name: parsed.name, agent: parsed.agent, model: parsed.model });
        } else if (parsed.kind === 'close_room') {
          callbacks?.onCloseRoom?.({ reason: parsed.reason });
        } else if (parsed.kind) {
          onEvent(parsed as UiEvent);
        }
      }
    });
    this.child.on('error', (err) =>
      onEvent({ kind: 'error', message: `worker failed: ${err.message}` }),
    );
    this.child.on('exit', () => onEvent({ kind: 'session_end' }));
  }

  prompt(text: string, from?: string): void {
    this.child.stdin?.write(`${JSON.stringify({ type: 'prompt', text, from })}\n`);
  }

  stop(): void {
    this.child.stdin?.write(`${JSON.stringify({ type: 'stop' })}\n`);
    this.child.kill();
  }

  /** Hard-kill the worker (no graceful stop handshake) — for engine shutdown,
   *  where we just need children gone before the process exits. */
  kill(): void {
    this.child.kill();
  }
}

/**
 * The engine's single owner of all live sessions. Each session is an isolated
 * subprocess, so sessions run concurrently and a crash takes down only its own
 * process. Every client (cockpit WS connections, and the CLI) subscribes to the
 * same broadcast, so a session started anywhere is visible everywhere.
 */
class SessionManager {
  private readonly sessions = new Map<string, { session: PiSession; info: SessionInfo }>();
  private readonly subscribers = new Set<(msg: Outbound) => void>();

  subscribe(fn: (msg: Outbound) => void): () => void {
    this.subscribers.add(fn);
    fn({ sessions: this.list() }); // catch the new client up
    return () => {
      this.subscribers.delete(fn);
    };
  }

  list(): SessionInfo[] {
    return [...this.sessions.values()].map((s) => s.info);
  }

  spawn(
    id: string,
    req: SpawnRequest,
    origin: 'ui' | 'cli' = 'ui',
    callbacks?: SessionCallbacks,
  ): void {
    if (this.sessions.has(id)) return;
    const info: SessionInfo = {
      id,
      model: req.model,
      cwd: req.cwd,
      agent: req.agent,
      projectName: req.projectName,
      origin,
    };
    if (req.worktree) {
      // Deterministic derivation (no await → spawn stays synchronous, no race).
      // A bad name throws here; surface it as an error for this id rather than
      // throwing out of spawn() and aborting the whole connection's frame.
      try {
        info.worktree = req.worktree;
        info.branch = worktreeRef(req.worktree);
        info.worktreePath = worktreePath(req.worktree);
      } catch (err) {
        this.broadcast({
          session: id,
          event: { kind: 'error', message: err instanceof Error ? err.message : String(err) },
        });
        this.broadcast({ session: id, event: { kind: 'session_end' } });
        return;
      }
    }
    const session = new PiSession(
      req,
      (event) => {
        this.broadcast({ session: id, event });
        if (event.kind === 'session_end') {
          this.sessions.delete(id);
          this.broadcast({ sessions: this.list() });
        }
      },
      callbacks,
    );
    this.sessions.set(id, { session, info });
    this.broadcast({ sessions: this.list() });
  }

  prompt(id: string, text: string, from?: string): void {
    this.sessions.get(id)?.session.prompt(text, from);
  }

  stop(id: string): void {
    const entry = this.sessions.get(id);
    if (!entry) return;
    // Deliberately does NOT remove the session's worktree. Worktrees persist; a
    // shared worktree (a reviewer attached to a coder's tree) must survive any one
    // session closing. Removal is explicit (`kild worktree rm` / UI) or automatic
    // only via merge-prune. See worktree.ts:pruneMergedWorktrees.
    entry.session.stop();
    this.sessions.delete(id);
    this.broadcast({ sessions: this.list() });
  }

  /** Kill every worker subprocess. Called on engine shutdown so a `--watch`
   *  reload (or Ctrl-C) never orphans workers (they'd otherwise reparent to init). */
  shutdown(): void {
    for (const { session } of this.sessions.values()) session.kill();
  }

  private broadcast(msg: Outbound): void {
    for (const fn of this.subscribers) fn(msg);
  }
}

/** Engine-wide singleton. */
export const sessionManager = new SessionManager();
