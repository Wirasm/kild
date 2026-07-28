import type { InboxMessage } from './inbox.ts';
import type { AgentSpec, KildIdentity, KildStatus, Message } from './kild-types.ts';

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

/**
 * The agent id of the process this CLI is running INSIDE, when it is running inside one.
 *
 * The engine sets `KILD_AGENT_ID` on every agent it spawns, and an agent that shells
 * `kild send` from bash is one of those. Forwarding it is what makes such a message
 * attributable: without it the agent presented no credential and its own words landed on the
 * log as `human` — the engine could not tell one of its own agents from the operator typing.
 * A human's shell has no such variable and is still `human`, which is correct.
 */
const selfAgentId = (): string | undefined => process.env.KILD_AGENT_ID || undefined;

/** `to` names the agents being addressed, exactly as the in-kild `send` tool does, and
 *  like it, it is required — the engine has no default recipient to fall back to.
 *
 *  `token` is the attached-harness credential from `attach`. An agent kild spawned proves
 *  itself with `agentId`; an attached one has no such process and this is the only way it
 *  can name itself as the thing it is already addressed as. Presenting neither is correct
 *  for a human's shell and lands on the log as the unattributed label. */
export async function sendMessage(
  kildId: string,
  to: string[],
  text: string,
  agentId: string | undefined = selfAgentId(),
  token?: string,
): Promise<KildActionResponse> {
  return engineFetch(`/api/kilds/${encodeURIComponent(kildId)}/messages`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    body: JSON.stringify({ to, text, ...(agentId ? { agentId } : {}) }),
  });
}

export async function stopKild(
  kildId: string,
  agentId: string | undefined = selfAgentId(),
): Promise<KildActionResponse> {
  return engineFetch(`/api/kilds/${encodeURIComponent(kildId)}/stop`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ ...(agentId ? { agentId } : {}) }),
  });
}

function collectionQuery(opts: { state?: string; repo?: string }): string {
  const query = new URLSearchParams();
  if (opts.state) query.set('state', opts.state);
  if (opts.repo) query.set('path', opts.repo);
  return query.size > 0 ? `?${query}` : '';
}

/**
 * The kild collection, CHEAP: identity and structure for live kilds unioned with the
 * `kild/*` worktrees git reports (an orphaned tree has no agents and is addressed by its
 * worktree name). No git, no logs, no cost — poll this one.
 *
 * `state` filters (`live` | `orphan`); `repo` scopes to one checkout.
 */
export async function listKilds(
  opts: { state?: string; repo?: string } = {},
): Promise<KildIdentity[]> {
  return engineFetch(`/api/kilds${collectionQuery(opts)}`);
}

/** The same collection, COSTLY: each kild's git state and cost rollups, on its own cadence.
 *  `state` also accepts `reclaimable` here, since that filter reads `ahead`. */
export async function kildsStatus(
  opts: { state?: string; repo?: string } = {},
): Promise<KildStatus[]> {
  return engineFetch(`/api/kilds/status${collectionQuery(opts)}`);
}

/** One kild with its git state and cost — never its log (see {@link kildMessages}). */
export async function getKild(kildId: string): Promise<KildStatus> {
  return engineFetch(`/api/kilds/${encodeURIComponent(kildId)}`);
}

/** A kild's message log, cursored by `seq`. `since` is exclusive — pass the last seq you
 *  saw. Works for an archived kild too: its log is the read-only record. */
export async function kildMessages(kildId: string, since?: number): Promise<Message[]> {
  const suffix = since === undefined ? '' : `?since=${since}`;
  return engineFetch(`/api/kilds/${encodeURIComponent(kildId)}/messages${suffix}`);
}

export interface AttachResponse extends KildActionResponse {
  /** Bearer token for this (kild, handle): send it as `Authorization: Bearer <token>` and
   *  the engine attributes the message to this handle instead of the unattributed label.
   *  Stable across re-attaches, and gone when the kild stops. */
  token: string;
}

/** Register an attached agent — a harness kild does not own claiming a `@handle`.
 *  Idempotent by handle: re-attaching is a no-op returning the same token, not an error. */
export async function attachAgent(kildId: string, handle: string): Promise<AttachResponse> {
  return engineFetch(`/api/kilds/${encodeURIComponent(kildId)}/agents/attach`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ handle }),
  });
}

export interface DrainInboxResponse {
  ok: true;
  messages: InboxMessage[];
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

/** Spawn an agent into a live kild. Unlike the WS frame this ANSWERS: a rejection comes
 *  back as a thrown error carrying the engine's reason, never a silent no-op.
 *
 *  `task` rides the same request but is NOT part of the spec — it is the new agent's first
 *  message, delivered from whoever the engine resolves this caller to be. */
export async function spawnKildAgent(
  kildId: string,
  agent: AgentSpec & { task?: string },
): Promise<{ ok: true; handle: string; message: string }> {
  return engineFetch(`/api/kilds/${encodeURIComponent(kildId)}/agents`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(agent),
  });
}

/** Stop ONE agent in a kild; the kild and its other agents keep going. */
export async function stopKildAgent(kildId: string, handle: string): Promise<KildActionResponse> {
  return engineFetch(
    `/api/kilds/${encodeURIComponent(kildId)}/agents/${encodeURIComponent(handle)}`,
    { method: 'DELETE' },
  );
}

/** What disposal did, and what it cost. The `kild/<name>` branch always survives. */
export interface DisposeKildResponse {
  ok: true;
  id: string;
  worktree: string;
  branch: string;
  branchKept: true;
  removed: string;
  /** Uncommitted/untracked files the removal discarded — named, never hidden. Empty means
   *  empty only when `discardedError` is absent. */
  discarded: string[];
  /** Why the list could not be determined. Present ⇒ `discarded` is unknown, not empty. */
  discardedError?: string;
  forced: boolean;
  message: string;
}

/** Dispose of a kild's worktree. Refused when the branch carries commits base does not
 *  have (`force` overrides — the branch, and every commit on it, survives regardless). */
export async function disposeKild(kildId: string, force = false): Promise<DisposeKildResponse> {
  const query = force ? '?force=true' : '';
  return engineFetch(`/api/kilds/${encodeURIComponent(kildId)}${query}`, { method: 'DELETE' });
}

/** A land, real or hypothetical — the same shape either way. */
export interface LandResponse {
  base: string;
  branch: string | null;
  commits: Array<{ sha: string; subject: string }>;
  files: string[];
  collides: string[];
  wouldMerge: boolean;
  merged: boolean;
  sha?: string;
  error?: string;
  dryRun: boolean;
}

/** The dry run: what landing would do. Touches nothing. */
export async function landPreview(kildId: string): Promise<LandResponse> {
  return engineFetch(`/api/kilds/${encodeURIComponent(kildId)}/land`);
}

/** Perform the land and report the merge sha. */
export async function landKild(kildId: string): Promise<LandResponse> {
  return engineFetch(`/api/kilds/${encodeURIComponent(kildId)}/land`, { method: 'POST' });
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
