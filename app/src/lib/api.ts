import type {
  Agent,
  ArchivedRoom,
  Message,
  Project,
  RoomSpec,
  RoomSummary,
  UiEvent,
  Worktree,
} from "./types";

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

/** Fetch past rooms (read-only history) recovered from the engine's on-disk store.
 *  Their participant subprocesses are gone — these are conversation records only. */
export async function listArchivedRooms(): Promise<ArchivedRoom[]> {
  const r = await fetch(`${BASE}/api/rooms/archive`);
  if (!r.ok) throw new Error(`archived rooms request failed (${r.status})`);
  return r.json();
}

/** Fetch LIVE rooms with their logs — so the cockpit can load the conversation so far
 *  for a room it didn't open itself (e.g. one set up via the CLI), or after a reload.
 *  Same shape as {@link ArchivedRoom} but these rooms are still running. */
export async function listLiveRooms(): Promise<ArchivedRoom[]> {
  const r = await fetch(`${BASE}/api/rooms/live`);
  if (!r.ok) throw new Error(`live rooms request failed (${r.status})`);
  return r.json();
}

/** A room message as broadcast by the engine (carries the room id for routing). */
type WireRoomMessage = Message & { roomId: string };

/**
 * WebSocket client to the kild engine — open / post / close rooms, and receive the
 * room list ({@link RoomSummary}[]), the shared message log ({@link Message}), and
 * each participant's streamed transcript ({@link UiEvent}s tagged by room +
 * participant).
 *
 * It auto-reconnects: in dev the engine restarts on every code change (and loses its
 * in-memory rooms), so the socket reconnects and `onStatus(false)` lets the UI mark
 * live participants dead. Frames sent while disconnected are queued and flushed.
 */
export class EngineSocket {
  private ws!: WebSocket;
  private closed = false;
  private queue: string[] = [];

  constructor(
    private onEvent: (room: string, participant: string, event: UiEvent) => void,
    private onStatus?: (connected: boolean) => void,
    private onRooms?: (rooms: RoomSummary[]) => void,
    private onRoomMessage?: (room: string, message: Message) => void,
    private onArchivedRoom?: (room: ArchivedRoom) => void,
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
      let msg: {
        rooms?: RoomSummary[];
        roomMessage?: WireRoomMessage;
        archivedRoom?: ArchivedRoom;
        room?: string;
        participant?: string;
        event?: UiEvent;
      };
      try {
        msg = JSON.parse(ev.data);
      } catch {
        return; // ignore malformed frames
      }
      if (Array.isArray(msg.rooms)) this.onRooms?.(msg.rooms);
      else if (msg.archivedRoom) this.onArchivedRoom?.(msg.archivedRoom);
      else if (msg.roomMessage) this.onRoomMessage?.(msg.roomMessage.roomId, msg.roomMessage);
      else if (typeof msg.room === "string" && typeof msg.participant === "string" && msg.event) {
        this.onEvent(msg.room, msg.participant, msg.event);
      }
    });
    this.ws.addEventListener("close", () => {
      // Drop queued frames: the restarted engine has lost those rooms.
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

  openRoom(id: string, spec: RoomSpec): void {
    this.send({ type: "room_open", id, ...spec });
  }
  postToRoom(id: string, text: string): void {
    this.send({ type: "room_post", id, text });
  }
  addParticipant(id: string, participant: { name: string; agent?: string; model?: string }): void {
    this.send({ type: "room_add", id, participant });
  }
  /** Trip the manual circuit breaker: stop the room's agents but keep it read-only. */
  haltRoom(id: string): void {
    this.send({ type: "room_halt", id });
  }
  closeRoom(id: string): void {
    this.send({ type: "room_close", id });
  }
  close(): void {
    this.closed = true;
    this.ws.close();
  }
}
