import type { ArchivedRoom, ParticipantSpec } from '../room/room-types.ts';

const ENGINE = process.env.KILD_ENGINE ?? 'http://localhost:4517';

export interface OpenRoomRequest {
  name: string;
  cwd?: string;
  project?: string;
  worktree?: string;
  participants: ParticipantSpec[];
  kickoff: string;
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

export async function getLiveRooms(): Promise<ArchivedRoom[]> {
  return engineFetch('/api/rooms/live');
}
