import { type ChildProcess, spawn } from 'node:child_process';

import type { UiEvent } from './events.ts';
import type {
  CommandAck,
  CommandResult,
  KildActionSuccess,
  SendOut,
  SpawnOut,
  StopOut,
} from './kild-types.ts';
import { readSkillsProfile, skillsProfileForAgent } from './skills-profile.ts';
import { worktreePath, worktreeRef } from './worktree.ts';

export interface SpawnRequest {
  model?: string;
  cwd?: string;
  persona?: string;
  /** Session label shown in listings (e.g. the kild name). Never a registry project
   *  name — purely display. */
  label?: string;
  /** Worktree *name* (not path). Absent → run in the project's main checkout.
   *  Present → the agent ensures `kild/<name>` and runs there. Two sessions naming
   *  the same worktree share its tree (attach); different names split. The agent
   *  creates-or-attaches from `cwd` (the repo). */
  worktree?: string;
  /** Base branch a brand-new worktree forks from (e.g. `dev`). Ignored when attaching to
   *  an existing tree. Absent → the checkout's current HEAD. */
  base?: string;
  /** Absolute path of an existing pi session file to fork from. The agent copies its
   *  full history into a brand-new session file (frozen snapshot) — the source file is
   *  never written, so the original session cannot be polluted or corrupted. */
  forkFrom?: string;
  /** Extra environment for the agent process — opaque to the manager. Agents in a kild
   *  use it to carry `KILD_KILD_ID` / `KILD_HANDLE`; it never overrides the `KILD_*`
   *  vars the manager itself sets. */
  env?: Record<string, string>;
}

/** Metadata UI clients show for an agent — including ones the CLI started. */
export interface AgentInfo {
  id: string;
  model?: string;
  cwd?: string;
  persona?: string;
  /** Session label (display only — see {@link SpawnRequest.label}). */
  label?: string;
  origin: 'ui' | 'cli';
  /** The selected worktree's name (echoed for the worktrees-in-use cross-check). */
  worktree?: string;
  /** `kild/<name>` ref, when the agent runs in a worktree (else undefined). */
  branch?: string;
  /** Deterministic on-disk worktree path, when the agent runs in a worktree. */
  worktreePath?: string;
  /** The underlying pi session id — reopen this agent in a terminal with
   *  `pi --session <piSessionFile ?? piSessionId>`. */
  piSessionId?: string;
  /** Absolute pi session file path (the robust resume handle; works from any cwd). */
  piSessionFile?: string;
}

/** A message broadcast to every connected client. */
export type Outbound = { agent: string; event: UiEvent } | { agents: AgentInfo[] };

type MaybePromise<T> = T | Promise<T>;

// Deliberately captured at engine startup rather than per spawn. An agent process
// inherits the engine environment, so PiSession removes it below and selectively
// restores it only for agents in a kild.
const SKILLS_PROFILE = readSkillsProfile(process.env.KILD_SKILLS_PROFILE);

/** The manager-owned `KILD_*` agent environment for a spawn request. Layered on top
 *  of `req.env`, so a request can never override the manager's own vars. Pure —
 *  exported for tests (the request→env plumbing without spawning an agent). */
export function agentEnv(
  id: string,
  req: SpawnRequest,
  skillsProfile: string | undefined,
): Record<string, string> {
  return {
    KILD_ROLE: 'agent',
    KILD_MODEL: req.model ?? '',
    KILD_CWD: req.cwd ?? process.cwd(),
    KILD_PERSONA: req.persona ?? '',
    // Session identity is manager-owned: REST callers use it to identify kild creators.
    KILD_SESSION_ID: id,
    // The worktree *name*; the agent ensures it from KILD_CWD (the repo).
    KILD_WORKTREE: req.worktree ?? '',
    // Base branch a brand-new worktree forks from (empty → current HEAD).
    KILD_BASE: req.base ?? '',
    // pi session file to fork this session from (empty → fresh session).
    KILD_FORK_SESSION: req.forkFrom ?? '',
    // A profile is a kild capability assignment, not inherited agent state.
    KILD_SKILLS_PROFILE: skillsProfile ?? '',
  };
}

/** Control-line callbacks for an agent process — used by the KildManager to route an
 *  agent's `send` / `spawn` back into its kild. A bare (non-kild) session passes none,
 *  so the control lines are simply never emitted. */
export interface AgentCallbacks {
  onSend?: (m: SendOut) => MaybePromise<CommandResult<KildActionSuccess>>;
  onSpawn?: (s: SpawnOut) => MaybePromise<CommandResult<KildActionSuccess>>;
  onStop?: (s: StopOut) => MaybePromise<CommandResult<KildActionSuccess>>;
}

/**
 * One agent session = one subprocess (see `agent.ts`). The subprocess is the same
 * binary re-invoked with `KILD_ROLE=agent`; we talk to it over its stdio:
 * `UiEvent` JSONL out, prompt/stop JSONL in.
 */
class PiSession {
  private readonly child: ChildProcess;
  private buf = '';

  constructor(
    id: string,
    req: SpawnRequest,
    onEvent: (event: UiEvent) => void,
    callbacks?: AgentCallbacks,
  ) {
    const { KILD_SKILLS_PROFILE: _inheritedSkillsProfile, ...parentEnv } = process.env;
    const skillsProfile = skillsProfileForAgent(req.env?.KILD_KILD_ID, SKILLS_PROFILE);
    this.child = spawn(process.argv[0] as string, process.argv.slice(1), {
      env: {
        ...parentEnv,
        ...req.env, // extra agent env (e.g. kild membership); our KILD_* win below
        ...agentEnv(id, req, skillsProfile),
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
        let parsed:
          | (Partial<SendOut> & { kind: 'send' })
          | (Partial<SpawnOut> & { kind: 'spawn' })
          | (Partial<StopOut> & { kind: 'stop' })
          | (UiEvent & { kind: UiEvent['kind'] });
        try {
          parsed = JSON.parse(line) as typeof parsed;
        } catch {
          continue; // non-JSON line from the agent (a stray log); ignore.
        }
        // An agent's `send` / `spawn` arrive as control lines routed back to its kild,
        // not the transcript. Everything else is a UiEvent.
        if (parsed.kind === 'send') {
          void this.acknowledge(parsed.requestId, callbacks?.onSend?.(parsed as SendOut));
        } else if (parsed.kind === 'spawn' && parsed.handle) {
          void this.acknowledge(parsed.requestId, callbacks?.onSpawn?.(parsed as SpawnOut));
        } else if (parsed.kind === 'stop') {
          void this.acknowledge(parsed.requestId, callbacks?.onStop?.(parsed as StopOut));
        } else if (parsed.kind) {
          onEvent(parsed as UiEvent);
        }
      }
    });
    this.child.on('error', (err) =>
      onEvent({ kind: 'error', message: `agent failed: ${err.message}` }),
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

  /** Hard-kill the agent process (no graceful stop handshake) — for engine shutdown,
   *  where we just need children gone before the process exits. */
  kill(): void {
    this.child.kill();
  }

  private async acknowledge(
    requestId: string | undefined,
    pending: MaybePromise<CommandResult<KildActionSuccess>> | undefined,
  ): Promise<void> {
    if (!requestId) return;
    const result =
      (await pending) ??
      ({ ok: false, code: 'rejected', message: 'kild command unavailable' } as const);
    const ack: CommandAck = { type: 'command_result', requestId, result };
    this.child.stdin?.write(`${JSON.stringify(ack)}\n`);
  }
}

/**
 * The engine's single owner of all live agents. Each agent is an isolated
 * subprocess, so agents run concurrently and a crash takes down only its own
 * process. Every client (UI-client WS connections, and the CLI) subscribes to the
 * same broadcast, so an agent started anywhere is visible everywhere.
 */
export class AgentManager {
  private readonly sessions = new Map<string, { session: PiSession; info: AgentInfo }>();
  private readonly subscribers = new Set<(msg: Outbound) => void>();

  subscribe(fn: (msg: Outbound) => void): () => void {
    this.subscribers.add(fn);
    fn({ agents: this.list() }); // catch the new client up
    return () => {
      this.subscribers.delete(fn);
    };
  }

  list(): AgentInfo[] {
    return [...this.sessions.values()].map((s) => s.info);
  }

  spawn(
    id: string,
    req: SpawnRequest,
    origin: 'ui' | 'cli' = 'ui',
    callbacks?: AgentCallbacks,
  ): void {
    if (this.sessions.has(id)) return;
    const info: AgentInfo = {
      id,
      model: req.model,
      cwd: req.cwd,
      persona: req.persona,
      label: req.label,
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
          agent: id,
          event: { kind: 'error', message: err instanceof Error ? err.message : String(err) },
        });
        this.broadcast({ agent: id, event: { kind: 'session_end' } });
        return;
      }
    }
    const session = new PiSession(
      id,
      req,
      (event) => {
        // Capture the pi session's durable identity so any client can offer a
        // terminal resume (`pi --session …`) for this agent.
        if (event.kind === 'pi_session') {
          info.piSessionId = event.id;
          info.piSessionFile = event.file;
          this.broadcast({ agents: this.list() });
        }
        this.broadcast({ agent: id, event });
        if (event.kind === 'session_end') {
          this.sessions.delete(id);
          this.broadcast({ agents: this.list() });
        }
      },
      callbacks,
    );
    this.sessions.set(id, { session, info });
    this.broadcast({ agents: this.list() });
  }

  /** Returns false for a dead/missing agent so callers can silently drop best-effort signals. */
  prompt(id: string, text: string, from?: string): boolean {
    const entry = this.sessions.get(id);
    if (!entry) return false;
    entry.session.prompt(text, from);
    return true;
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
    this.broadcast({ agents: this.list() });
  }

  /** Kill every agent subprocess. Called on engine shutdown so a `--watch`
   *  reload (or Ctrl-C) never orphans them (they'd otherwise reparent to init). */
  shutdown(): void {
    for (const { session } of this.sessions.values()) session.kill();
  }

  private broadcast(msg: Outbound): void {
    for (const fn of this.subscribers) fn(msg);
  }
}

/** Engine-wide singleton. */
export const agentManager = new AgentManager();
