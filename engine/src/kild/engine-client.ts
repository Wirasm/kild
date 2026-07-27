import type { InboxMessage } from './inbox.ts';
import type { AgentSpec, LiveKildStatus } from './kild-types.ts';

const ENGINE = process.env.KILD_ENGINE ?? 'http://localhost:4517';

export interface NewKildRequest {
  name: string;
  cwd?: string;
  project?: string;
  worktree?: string;
  agents: AgentSpec[];
  /** The first message into the kild — addressed like any other: it names its
   *  recipients. There is no agent it falls through to. */
  kickoff: { to: string[]; text: string };
  /** Base branch for the worktree + git-status baseline (default: checkout's branch). */
  base?: string;
  /** Live session that created the kild; ordinary REST callers omit this. */
  openedBy?: string;
}

export interface NewKildResponse {
  ok: true;
  id: string;
  message: string;
}

export interface KildActionResponse {
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

export async function newKild(req: NewKildRequest): Promise<NewKildResponse> {
  return engineFetch('/api/kilds', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(req),
  });
}

/** `to` names the agents being addressed, exactly as the in-kild `send` tool does, and
 *  like it, it is required — the engine has no default recipient to fall back to. */
export async function sendMessage(
  kildId: string,
  to: string[],
  text: string,
  sessionId?: string,
): Promise<KildActionResponse> {
  return engineFetch(`/api/kilds/${encodeURIComponent(kildId)}/messages`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ to, text, ...(sessionId ? { sessionId } : {}) }),
  });
}

export async function stopKild(kildId: string, sessionId?: string): Promise<KildActionResponse> {
  return engineFetch(`/api/kilds/${encodeURIComponent(kildId)}/stop`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ ...(sessionId ? { sessionId } : {}) }),
  });
}

export async function getLiveKilds(): Promise<LiveKildStatus[]> {
  return engineFetch('/api/kilds');
}

/** Register an attached agent — a harness kild does not own claiming a `@handle`.
 *  Idempotent by handle: re-attaching is a no-op, not an error. */
export async function attachAgent(kildId: string, handle: string): Promise<KildActionResponse> {
  return engineFetch(`/api/kilds/${encodeURIComponent(kildId)}/agents/attach`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ handle }),
  });
}

export interface DrainInboxResponse {
  ok: true;
  posts: InboxMessage[];
  /** True when this drain reported nothing — the attached agent's idle signal. */
  idle: boolean;
  /** True when the engine's wake cap withheld mail. The caller is told nothing either way. */
  capped: boolean;
}

/** Hard deadline for a drain. This call sits inside a turn-end hook on EVERY turn, so it
 *  gets one short attempt and no retries: a slow or wedged engine must cost the operator
 *  a blink, not a stall. */
const DRAIN_TIMEOUT_MS = 1500;

/** Destructively read an attached agent's inbox. Throws on any failure — the
 *  hook-shaped caller is the one that decides failure means silence. */
export async function drainInbox(kildId: string, handle: string): Promise<DrainInboxResponse> {
  return engineFetch(`/api/kilds/${encodeURIComponent(kildId)}/inbox/drain`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ handle }),
    signal: AbortSignal.timeout(DRAIN_TIMEOUT_MS),
  });
}

export interface SpawnAgentRequest {
  persona?: string;
  model?: string;
  cwd?: string;
  worktree?: string;
  base?: string;
  /** Session label shown in listings (e.g. a kild name). Display only. */
  label?: string;
  /** Absolute pi session file to fork from — the spawned agent starts from a frozen
   *  copy of its history (a new session file; the source is never written). */
  forkFrom?: string;
  /** Initial prompt delivered right after spawn. */
  prompt?: string;
}

/** Spawn a detached agent through the engine; returns its id. */
export async function spawnAgent(req: SpawnAgentRequest): Promise<{ ok: true; id: string }> {
  return engineFetch('/api/agents', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(req),
  });
}

export async function promptAgent(id: string, text: string): Promise<{ ok: boolean }> {
  return engineFetch(`/api/agents/${encodeURIComponent(id)}/prompt`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ text }),
  });
}

export async function stopAgent(id: string): Promise<{ ok: boolean }> {
  return engineFetch(`/api/agents/${encodeURIComponent(id)}/stop`, { method: 'POST' });
}

export interface AgentSummary {
  id: string;
  persona?: string;
  model?: string;
  worktree?: string;
  cwd?: string;
}

export async function listAgents(): Promise<AgentSummary[]> {
  return engineFetch('/api/agents');
}
