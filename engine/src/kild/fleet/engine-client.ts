import type { ArchivedRoom, ParticipantSpec } from '../room/room-types.ts';

const ENGINE = process.env.KILD_ENGINE ?? 'http://localhost:4517';

export interface OpenRoomRequest {
  name: string;
  cwd?: string;
  project?: string;
  worktree?: string;
  participants: ParticipantSpec[];
  kickoff: string;
}

async function engineFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${ENGINE}${path}`, init);
  if (!response.ok) {
    const body = (await response.json().catch(() => ({}))) as { error?: string };
    throw new Error(body.error ?? `${path} failed (${response.status})`);
  }
  return response.json() as Promise<T>;
}

export async function openRoom(req: OpenRoomRequest): Promise<{ id: string }> {
  return engineFetch('/api/rooms', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(req),
  });
}

export async function postRoom(roomId: string, text: string, from?: string): Promise<{ ok: true }> {
  return engineFetch(`/api/rooms/${encodeURIComponent(roomId)}/post`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ text, from }),
  });
}

export async function closeRoom(roomId: string): Promise<{ ok: true }> {
  return engineFetch(`/api/rooms/${encodeURIComponent(roomId)}/close`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({}),
  });
}

export async function getLiveRooms(): Promise<ArchivedRoom[]> {
  return engineFetch('/api/rooms/live');
}
