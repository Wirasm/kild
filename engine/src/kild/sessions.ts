import { type ChildProcess, spawn } from 'node:child_process';

import type { UiEvent } from './events.ts';

export interface SpawnRequest {
  model?: string;
  cwd?: string;
  agent?: string;
  projectName?: string;
}

/** Metadata the cockpit shows for a session — including ones the CLI started. */
export interface SessionInfo {
  id: string;
  model?: string;
  cwd?: string;
  agent?: string;
  projectName?: string;
  origin: 'ui' | 'cli';
}

/** A message broadcast to every connected client. */
export type Outbound = { session: string; event: UiEvent } | { sessions: SessionInfo[] };

/**
 * One agent session = one worker subprocess (see `worker.ts`). The worker is the
 * same binary re-invoked with `KILD_ROLE=worker`; we talk to it over its stdio:
 * `UiEvent` JSONL out, prompt/stop JSONL in.
 */
class PiSession {
  private readonly child: ChildProcess;
  private buf = '';

  constructor(req: SpawnRequest, onEvent: (event: UiEvent) => void) {
    this.child = spawn(process.argv[0] as string, process.argv.slice(1), {
      env: {
        ...process.env,
        KILD_ROLE: 'worker',
        KILD_MODEL: req.model ?? '',
        KILD_CWD: req.cwd ?? process.cwd(),
        KILD_AGENT: req.agent ?? '',
      },
      stdio: ['pipe', 'pipe', 'inherit'],
    });

    this.child.stdout?.on('data', (chunk: Buffer) => {
      this.buf += chunk.toString();
      let nl: number;
      while ((nl = this.buf.indexOf('\n')) >= 0) {
        const line = this.buf.slice(0, nl).trim();
        this.buf = this.buf.slice(nl + 1);
        if (!line) continue;
        try {
          const event = JSON.parse(line) as UiEvent;
          if ('kind' in event) onEvent(event);
        } catch {
          // non-JSON line from the worker (a stray log); ignore.
        }
      }
    });
    this.child.on('exit', () => onEvent({ kind: 'session_end' }));
  }

  prompt(text: string): void {
    this.child.stdin?.write(`${JSON.stringify({ type: 'prompt', text })}\n`);
  }

  stop(): void {
    this.child.stdin?.write(`${JSON.stringify({ type: 'stop' })}\n`);
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

  spawn(id: string, req: SpawnRequest, origin: 'ui' | 'cli' = 'ui'): void {
    if (this.sessions.has(id)) return;
    const info: SessionInfo = {
      id,
      model: req.model,
      cwd: req.cwd,
      agent: req.agent,
      projectName: req.projectName,
      origin,
    };
    const session = new PiSession(req, (event) => {
      this.broadcast({ session: id, event });
      if (event.kind === 'session_end') {
        this.sessions.delete(id);
        this.broadcast({ sessions: this.list() });
      }
    });
    this.sessions.set(id, { session, info });
    this.broadcast({ sessions: this.list() });
  }

  prompt(id: string, text: string): void {
    this.sessions.get(id)?.session.prompt(text);
  }

  stop(id: string): void {
    const entry = this.sessions.get(id);
    if (!entry) return;
    entry.session.stop();
    this.sessions.delete(id);
    this.broadcast({ sessions: this.list() });
  }

  private broadcast(msg: Outbound): void {
    for (const fn of this.subscribers) fn(msg);
  }
}

/** Engine-wide singleton. */
export const sessionManager = new SessionManager();
