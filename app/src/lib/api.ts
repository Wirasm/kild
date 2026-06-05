import type { Agent, Project, SessionInfo, UiEvent, Worktree } from "./types";

/** The kild engine's base URL. Override with VITE_KILD_ENGINE. */
const BASE = import.meta.env.VITE_KILD_ENGINE ?? "http://localhost:4517";

export async function listProjects(): Promise<Project[]> {
  const r = await fetch(`${BASE}/api/projects`);
  if (!r.ok) throw new Error(`projects request failed (${r.status})`);
  return r.json();
}

export async function addProject(name: string, path: string): Promise<Project> {
  const r = await fetch(`${BASE}/api/projects`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ name, path }),
  });
  if (!r.ok) {
    const body = (await r.json().catch(() => ({}))) as { error?: string };
    throw new Error(body.error ?? `add project failed (${r.status})`);
  }
  return r.json();
}

export async function listAgents(project?: string): Promise<Agent[]> {
  const q = project ? `?project=${encodeURIComponent(project)}` : "";
  const r = await fetch(`${BASE}/api/agents${q}`);
  if (!r.ok) throw new Error(`agents request failed (${r.status})`);
  return r.json();
}

/** List the project's `kild/*` worktrees (the engine merge-prunes first). `project`
 *  may be a registered name or a path. */
export async function listWorktrees(project: string): Promise<Worktree[]> {
  const r = await fetch(`${BASE}/api/worktrees?project=${encodeURIComponent(project)}`);
  if (!r.ok) throw new Error(`worktrees request failed (${r.status})`);
  return r.json();
}

/** Remove a worktree by name (frees disk; the `kild/<name>` branch persists). */
export async function removeWorktree(project: string, name: string): Promise<void> {
  const r = await fetch(`${BASE}/api/worktrees`, {
    method: "DELETE",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ project, name }),
  });
  if (!r.ok) {
    const body = (await r.json().catch(() => ({}))) as { error?: string };
    throw new Error(body.error ?? `remove worktree failed (${r.status})`);
  }
}

/** Run the merge-prune now; returns the names pruned. */
export async function pruneWorktrees(project: string): Promise<string[]> {
  const r = await fetch(`${BASE}/api/worktrees/prune`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ project }),
  });
  if (!r.ok) throw new Error(`prune worktrees failed (${r.status})`);
  return ((await r.json()) as { pruned: string[] }).pruned;
}

/** Reveal a worktree path in the OS file browser (engine validates it's under the
 *  worktree root). Keeps the frontend pure-web — no Tauri API. */
export async function openWorktree(path: string): Promise<void> {
  const r = await fetch(`${BASE}/api/open`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ path }),
  });
  if (!r.ok) {
    const body = (await r.json().catch(() => ({}))) as { error?: string };
    throw new Error(body.error ?? `open failed (${r.status})`);
  }
}

export type SpawnOptions = {
  model?: string;
  cwd?: string;
  agent?: string;
  projectName?: string;
  /** Selected worktree name; undefined = run in the project's main checkout. */
  worktree?: string;
};

/**
 * WebSocket client to the kild engine — spawn/prompt/stop sessions and receive
 * the streamed {@link UiEvent}s, tagged by session id.
 *
 * It auto-reconnects: in dev the engine restarts on every code change (and
 * loses its in-memory sessions), so the socket reconnects and `onStatus(false)`
 * lets the UI mark live sessions dead. Messages sent while disconnected are
 * queued and flushed on reconnect.
 */
export class EngineSocket {
  private ws!: WebSocket;
  private closed = false;
  private queue: string[] = [];

  constructor(
    private onEvent: (session: string, event: UiEvent) => void,
    private onStatus?: (connected: boolean) => void,
    private onSessions?: (sessions: SessionInfo[]) => void,
  ) {
    this.connect();
  }

  private connect(): void {
    this.ws = new WebSocket(`${BASE.replace(/^http/, "ws")}/ws`);

    this.ws.addEventListener("open", () => {
      this.onStatus?.(true);
      for (const m of this.queue) this.ws.send(m);
      this.queue = [];
    });
    this.ws.addEventListener("message", (ev) => {
      let msg: { session?: string; event?: UiEvent; sessions?: SessionInfo[] };
      try {
        msg = JSON.parse(ev.data);
      } catch {
        return; // ignore malformed frames
      }
      if (Array.isArray(msg.sessions)) this.onSessions?.(msg.sessions);
      else if (typeof msg.session === "string" && msg.event) this.onEvent(msg.session, msg.event);
    });
    this.ws.addEventListener("close", () => {
      // Drop queued commands: the restarted engine has lost those sessions, so
      // replaying prompt/stop for them would silently no-op.
      this.queue = [];
      this.onStatus?.(false);
      if (!this.closed) setTimeout(() => this.connect(), 1000);
    });
    this.ws.addEventListener("error", () => this.ws.close());
  }

  private send(msg: unknown): void {
    const data = JSON.stringify(msg);
    if (this.ws.readyState === WebSocket.OPEN) this.ws.send(data);
    else this.queue.push(data);
  }

  spawn(id: string, opts: SpawnOptions): void {
    this.send({ type: "spawn", id, ...opts });
  }
  prompt(id: string, text: string): void {
    this.send({ type: "prompt", id, text });
  }
  stop(id: string): void {
    this.send({ type: "stop", id });
  }
  close(): void {
    this.closed = true;
    this.ws.close();
  }
}
