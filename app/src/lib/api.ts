import type { Agent, Project, SessionInfo, UiEvent } from "./types";

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

export type SpawnOptions = { model?: string; cwd?: string; agent?: string; projectName?: string };

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
      const msg = JSON.parse(ev.data) as
        | { session: string; event: UiEvent }
        | { sessions: SessionInfo[] };
      if ("sessions" in msg) this.onSessions?.(msg.sessions);
      else this.onEvent(msg.session, msg.event);
    });
    this.ws.addEventListener("close", () => {
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
