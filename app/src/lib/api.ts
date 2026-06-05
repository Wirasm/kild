import type { Agent, Project, UiEvent } from "./types";

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

export type SpawnOptions = { model?: string; cwd?: string; agent?: string };

/**
 * WebSocket client to the kild engine — spawn/prompt/stop sessions and receive
 * the streamed {@link UiEvent}s, tagged by session id. Replaces the old Tauri
 * `invoke`/`listen` surface; the event shape is unchanged.
 */
export class EngineSocket {
  private ws: WebSocket;
  private ready: Promise<void>;

  constructor(onEvent: (session: string, event: UiEvent) => void) {
    this.ws = new WebSocket(`${BASE.replace(/^http/, "ws")}/ws`);
    this.ready = new Promise((resolve, reject) => {
      this.ws.addEventListener("open", () => resolve(), { once: true });
      this.ws.addEventListener("error", () => reject(new Error("engine socket error")), {
        once: true,
      });
    });
    this.ws.addEventListener("message", (ev) => {
      const { session, event } = JSON.parse(ev.data) as { session: string; event: UiEvent };
      onEvent(session, event);
    });
  }

  private async send(msg: unknown): Promise<void> {
    await this.ready;
    this.ws.send(JSON.stringify(msg));
  }

  spawn(id: string, opts: SpawnOptions): Promise<void> {
    return this.send({ type: "spawn", id, ...opts });
  }
  prompt(id: string, text: string): Promise<void> {
    return this.send({ type: "prompt", id, text });
  }
  stop(id: string): Promise<void> {
    return this.send({ type: "stop", id });
  }
  close(): void {
    this.ws.close();
  }
}
