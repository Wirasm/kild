import type { MailboxPost } from '../room/attached.ts';
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

export async function closeRoom(
  roomId: string,
  sessionId?: string,
  force?: boolean,
): Promise<RoomActionResponse> {
  return engineFetch(`/api/rooms/${encodeURIComponent(roomId)}/close`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ ...(sessionId ? { sessionId } : {}), ...(force ? { force } : {}) }),
  });
}

export async function getLiveRooms(): Promise<LiveRoomStatus[]> {
  return engineFetch('/api/rooms/live');
}

/** Register an attached participant — a harness kild does not own claiming a `@handle`.
 *  Idempotent by name: re-joining is a no-op, not an error. */
export async function joinRoom(roomId: string, name: string): Promise<RoomActionResponse> {
  return engineFetch(`/api/rooms/${encodeURIComponent(roomId)}/join`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name }),
  });
}

export interface DrainRoomResponse {
  ok: true;
  posts: MailboxPost[];
  /** True when this drain reported nothing — the attached participant's idle signal. */
  idle: boolean;
  /** True when the engine's wake cap withheld mail. The caller is told nothing either way. */
  capped: boolean;
}

/** Hard deadline for a drain. This call sits inside a turn-end hook on EVERY turn, so it
 *  gets one short attempt and no retries: a slow or wedged engine must cost the operator
 *  a blink, not a stall. */
const DRAIN_TIMEOUT_MS = 1500;

/** Destructively read an attached participant's mailbox. Throws on any failure — the
 *  hook-shaped caller is the one that decides failure means silence. */
export async function drainRoom(roomId: string, name: string): Promise<DrainRoomResponse> {
  return engineFetch(`/api/rooms/${encodeURIComponent(roomId)}/drain`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name }),
    signal: AbortSignal.timeout(DRAIN_TIMEOUT_MS),
  });
}

export interface SpawnSessionRequest {
  persona?: string;
  model?: string;
  cwd?: string;
  worktree?: string;
  base?: string;
  /** Session label shown in listings (e.g. 'operator', a room name). Display only. */
  label?: string;
  /** Grant the operator room-control tools (open/post/status/close rooms). */
  operator?: boolean;
  /** Absolute pi session file to fork from — the spawned session starts from a frozen
   *  copy of its history (a new session file; the source is never written). */
  forkFrom?: string;
  /** Initial prompt delivered right after spawn (e.g. the operator session's goal). */
  prompt?: string;
}

/** Spawn a detached session (e.g. a detached operator) through the engine; returns its id. */
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
  persona?: string;
  model?: string;
  worktree?: string;
  cwd?: string;
}

export async function listSessions(): Promise<SessionSummary[]> {
  return engineFetch('/api/sessions');
}
