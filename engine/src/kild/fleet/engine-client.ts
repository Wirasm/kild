import type { LiveRoomStatus, ParticipantSpec } from '../room/room-types.ts';

const ENGINE = process.env.KILD_ENGINE ?? 'http://localhost:4517';

export interface OpenRoomRequest {
  name: string;
  cwd?: string;
  project?: string;
  worktree?: string;
  participants: ParticipantSpec[];
  kickoff: string;
  /** Base branch for the worktree + git-status baseline (default: checkout's branch). */
  base?: string;
  /** Live session that opened the room; ordinary REST callers omit this. */
  openedBy?: string;
}

export interface OpenRoomResponse {
  ok: true;
  id: string;
  message: string;
}

export interface RoomActionResponse {
  ok: true;
  message: string;
}

async function engineFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${ENGINE}${path}`, init);
  if (!response.ok) {
    const body = (await response.json().catch(() => ({}))) as { error?: string };
    throw new Error(body.error ?? `${path} failed (${response.status})`);
  }
  return response.json() as Promise<T>;
}

export async function openRoom(req: OpenRoomRequest): Promise<OpenRoomResponse> {
  return engineFetch('/api/rooms', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(req),
  });
}

export async function postRoom(
  roomId: string,
  text: string,
  sessionId?: string,
): Promise<RoomActionResponse> {
  return engineFetch(`/api/rooms/${encodeURIComponent(roomId)}/post`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ text, ...(sessionId ? { sessionId } : {}) }),
  });
}

export async function closeRoom(roomId: string, sessionId?: string): Promise<RoomActionResponse> {
  return engineFetch(`/api/rooms/${encodeURIComponent(roomId)}/close`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(sessionId ? { sessionId } : {}),
  });
}

export async function getLiveRooms(): Promise<LiveRoomStatus[]> {
  return engineFetch('/api/rooms/live');
}

export interface SpawnSessionRequest {
  agent?: string;
  model?: string;
  cwd?: string;
  worktree?: string;
  base?: string;
  projectName?: string;
  /** Grant the fleet room-control tools (open/post/status/close rooms). */
  fleet?: boolean;
  /** Initial prompt delivered right after spawn (e.g. the fleet driver's goal). */
  prompt?: string;
}

/** Spawn a detached session (e.g. a fleet driver) through the engine; returns its id. */
export async function spawnSession(req: SpawnSessionRequest): Promise<{ ok: true; id: string }> {
  return engineFetch('/api/sessions', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(req),
  });
}

export async function promptSession(id: string, text: string): Promise<{ ok: boolean }> {
  return engineFetch(`/api/sessions/${encodeURIComponent(id)}/prompt`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ text }),
  });
}

export async function stopSession(id: string): Promise<{ ok: boolean }> {
  return engineFetch(`/api/sessions/${encodeURIComponent(id)}/stop`, { method: 'POST' });
}

export interface SessionSummary {
  id: string;
  agent?: string;
  model?: string;
  worktree?: string;
  cwd?: string;
}

export async function listSessions(): Promise<SessionSummary[]> {
  return engineFetch('/api/sessions');
}
