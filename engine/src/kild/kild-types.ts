import type { UiEvent } from './events.ts';
import type { Inbox } from './inbox.ts';
import type { KildGitStatus } from './worktree-status.ts';

/**
 * Kild domain — the operator-facing primitive: a set of agents (agent instances +
 * the human) exchanging Messages on one shared log. A single-agent "session" is just
 * a 1-agent Kild. Pure types; no behaviour (state lives in the registry, routing in
 * the router, lifecycle in the manager).
 */

/** Reserved agent handle for the human operator. Messages to `@human` surface
 *  in UI clients/CLI only — there is no session to deliver a turn to. */
export const HUMAN = 'human';

/** How kild reaches an agent.
 *
 *  - `owned` (the default, and everything that existed before): kild owns the process,
 *    so delivery is a PUSH — a prompt written to its stdin the moment a message is routed.
 *  - `attached`: a harness the human drives (a Claude Code session), which kild registers
 *    and addresses but never spawns. There is nothing to push to, so delivery inverts —
 *    kild queues into an {@link Inbox} and the harness PULLS at its own turn boundary.
 *
 *  Absent means `owned`: kilds persisted before attached agents existed decode
 *  unchanged (see {@link ArchivedKild}). */
export type Ownership = 'owned' | 'attached';

/** Everything both ownerships carry — identity plus the idle state observers read. */
interface AgentBase {
  /** The `@mention` handle (e.g. `orchestrator`). */
  handle: string;
  /** The persona (`.pi/agents/<name>.md` — the dir name is upstream pi convention) it
   *  runs as. */
  persona?: string;
  /** The model this agent runs on — the requested ref at spawn, upgraded to the
   *  provider-resolved `provider/id` once the session reports it. Lets an observer see
   *  which model each agent used in a run. */
  model?: string;
  /** Who spawned this agent — the spawning agent's handle, or {@link HUMAN} for the
   *  creator's initial roster. Ground-truth spawn edge (vs inferring it from the log)
   *  and the routing target for its idle/done notice. */
  invitedBy?: string;
  /** True when the agent has finished a turn and is waiting. Set on `agent_end` for an
   *  owned agent and by an EMPTY drain for an attached one, cleared when work arrives.
   *  Rides {@link AgentView} so observers can rank finished-and-waiting kilds without
   *  parsing logs. */
  idle?: boolean;
  /** Latest cumulative token count for this agent's session, captured from its
   *  `stats` UiEvents (emitted at each turn end). */
  tokens?: number;
  /** Latest cumulative cost (USD) for this agent's session, from `stats` UiEvents. */
  cost?: number;
}

/** An agent kild spawned and owns: an agent session addressable by its `@handle`,
 *  delivered to by writing a prompt to its stdin. */
export interface OwnedAgent extends AgentBase {
  /** Absent (the persisted default) or explicit — both mean kild owns the process. */
  ownership?: 'owned';
  /** The kild session id running this agent. */
  id: string;
  /** The underlying pi session id — the durable terminal-resume handle
   *  (`pi --session <piSessionFile ?? piSessionId>`). Captured from the agent's
   *  `pi_session` event and persisted with the kild, so any agent in any kild —
   *  live or archived — can be reopened in the pi CLI. */
  piSessionId?: string;
  /** Absolute pi session file path (resume works from any cwd). */
  piSessionFile?: string;
}

/** An agent kild registered but did not spawn — a harness the human drives. It has
 *  no kild session and no pi session (nothing to resume): its whole transport is the
 *  inbox it drains at its own turn boundary. */
export interface AttachedAgent extends AgentBase {
  ownership: 'attached';
  /** Unread messages addressed to this agent, waiting for the next drain. Live state
   *  only — never persisted (an inbox outlives nothing; the kild log is the record). */
  inbox: Inbox;
}

/** An agent in a kild, of either ownership. The human is a virtual agent — never
 *  in this list. */
export type Agent = OwnedAgent | AttachedAgent;

/** The kild session behind an agent, or `undefined` when kild does not own its
 *  process. The ONE place callers that genuinely do not care about the ownership (stop
 *  the sessions, find a kild by session id) ask the question. */
export function agentProcessId(agent: Agent): string | undefined {
  return agent.ownership === 'attached' ? undefined : agent.id;
}

/** An agent as surfaced to observers (kild lists, status, archive) — identity plus the
 *  model it ran on. No kild process id (that's an internal handle); the PI session
 *  identity IS exposed — it's the durable handle for reopening the agent in a terminal. */
export interface AgentView {
  handle: string;
  /** Omitted for an owned agent (the default), `'attached'` for one kild does not own —
   *  so a roster stops implying every agent is a process kild can steer. */
  ownership?: Ownership;
  persona?: string;
  model?: string;
  piSessionId?: string;
  piSessionFile?: string;
  /** Attention state: finished a turn and waiting for input (see {@link AgentBase.idle}). */
  idle?: boolean;
  /** Latest cumulative session token count (from `stats` UiEvents). */
  tokens?: number;
  /** Latest cumulative session cost in USD (from `stats` UiEvents). */
  cost?: number;
}

/** The one mapping from a live agent to its observer view — every list/status/
 *  archive producer uses this so new view fields appear everywhere at once. */
export function agentView(agent: Agent): AgentView {
  const attached = agent.ownership === 'attached';
  return {
    handle: agent.handle,
    ownership: agent.ownership,
    persona: agent.persona,
    model: agent.model,
    piSessionId: attached ? undefined : agent.piSessionId,
    piSessionFile: attached ? undefined : agent.piSessionFile,
    idle: agent.idle,
    tokens: agent.tokens,
    cost: agent.cost,
  };
}

/** A kild's cost rollup, summed over the agents that have reported `stats`. */
export interface CostTotals {
  tokens: number;
  cost: number;
}

/** Sum agent costs into a kild total — undefined until at least one agent has reported
 *  stats, so payloads without cost data stay clean of zero-noise. */
export function costTotals(
  agents: Array<Pick<AgentView, 'tokens' | 'cost'>>,
): CostTotals | undefined {
  const reported = agents.filter((a) => a.tokens !== undefined || a.cost !== undefined);
  if (reported.length === 0) return undefined;
  return {
    tokens: reported.reduce((sum, a) => sum + (a.tokens ?? 0), 0),
    cost: reported.reduce((sum, a) => sum + (a.cost ?? 0), 0),
  };
}

/** A single message on a kild's shared log — the conversation unit. */
export interface Message {
  id: string;
  kildId: string;
  /** Handle of the sender, or {@link HUMAN}. */
  from: string;
  /** Resolved addressee handles (empty = broadcast to all, no turn delivered). */
  to: string[];
  text: string;
  /** Epoch millis, stamped by the engine on receipt. */
  ts: number;
  /** True for engine-generated notices (e.g. an agent joining). */
  system?: boolean;
}

/** The canonical kild lifecycle — transitions are enforced centrally by the kild
 *  manager/lifecycle helper rather than inferred from booleans or registry presence. */
export type KildLifecycleState = 'opening' | 'running' | 'halted' | 'closed';

/** A live kild: agents + a shared message log + a workspace. */
export interface Kild {
  id: string;
  name: string;
  /** Project directory the agents run in (their cwd). */
  cwd: string;
  /** Optional shared worktree name — every agent attaches to `kild/<name>`. */
  worktree?: string;
  /** Base branch the worktree was created from and that git status/collisions are
   *  measured against (so ahead/behind and changed files reflect this kild's own
   *  work, not everything the base is ahead of `main`). */
  base?: string;
  /** Session that created this kild. It is notified only when it is not an agent in it. */
  openedBy?: string;
  agents: Agent[];
  log: Message[];
  /** Canonical lifecycle state for this kild. */
  state: KildLifecycleState;
}

/** An agent to spawn into a kild. */
export interface AgentSpec {
  handle: string;
  persona?: string;
  model?: string;
}

/** Spec to create a kild: who the agents are and where they run. */
export interface NewKildSpec {
  name: string;
  cwd: string;
  agents: AgentSpec[];
  /** Optional shared worktree — every agent attaches to one `kild/<name>` tree. */
  worktree?: string;
  /** Base branch for the worktree + git-status baseline (default: the checkout's current
   *  branch). Editable via `.kild/config.json` `baseBranch` or the `--base` CLI flag. */
  base?: string;
  /** Creator session identity from a session-aware REST caller; absent for ordinary
   *  REST callers. */
  openedBy?: string;
}

/** Lightweight kild descriptor for client lists. */
export interface KildSummary {
  id: string;
  name: string;
  worktree?: string;
  agents: AgentView[];
  /** Canonical lifecycle state when surfaced by kild-owned producers. Optional so
   *  out-of-scope fixtures/consumers do not need coordinated edits in this slice. */
  state?: KildLifecycleState;
  /** True when the operator has halted the kild (sessions stopped, kept read-only).
   *  Derived compatibility field for existing non-kild consumers. */
  stopped?: boolean;
}

/** A kild recovered from disk after an engine restart — its conversation log with no
 *  live agents (their sessions are gone). UI clients render it read-only. */
export interface ArchivedKild {
  id: string;
  name: string;
  worktree?: string;
  agents: AgentView[];
  /** Canonical lifecycle state when persisted by kild-owned producers. Optional so
   *  older history files and out-of-scope fixtures continue to type-check. */
  state?: KildLifecycleState;
  log: Message[];
  /** Project directory the kild ran in — persisted so archived kilds stay attributable
   *  to their project after the worktree is pruned. Optional: older files predate it. */
  cwd?: string;
  /** Base branch the kild measured against. Optional: older files predate it. */
  base?: string;
}

/** A live kild enriched with its git/worktree state — the code-state half of
 *  observability, so a driving agent can land work and avoid collisions. Git is
 *  live-only (never persisted); computed on demand when serving live-kild status. */
export interface LiveKildStatus extends ArchivedKild {
  git?: KildGitStatus;
  /** Kild cost rollup summed over agents — absent until stats have arrived. */
  totals?: CostTotals;
}

/** Typed kild-domain result: every command either succeeds with a value or fails with
 *  an explicit kild error code + message. */
export type CommandResult<T> =
  | { ok: true; value: T }
  | { ok: false; code: KildErrorCode; message: string };

/** Kild-domain failure categories — transport-agnostic, mapped by REST/agent layers. */
export type KildErrorCode = 'not_found' | 'invalid_state' | 'rejected';

export interface KildActionSuccess {
  message: string;
  /** For sends: the resolved recipients other than the sender (the human counts — it is
   *  the operator channel). Empty = the message reached no one, e.g. a self-addressed
   *  send. */
  deliveredTo?: string[];
}

export interface NewKildSuccess extends KildActionSuccess {
  kildId: string;
}

/** What the engine broadcasts to clients about kilds. */
export type KildOutbound =
  | { message: Message }
  | { kilds: KildSummary[] }
  /** A kild that just stopped with history — pushed so clients show it as read-only
   *  history immediately, without refetching the archive or restarting. */
  | { archivedKild: ArchivedKild }
  /** An agent's transcript event (its UiEvent stream), tagged by kild+agent. */
  | { kild: string; agent: string; event: UiEvent };

/** Agent→engine control line: an agent called `send`. Distinct from a `UiEvent` —
 *  routed to the kild, not shown as the agent's raw transcript. */
export interface SendOut {
  kind: 'send';
  requestId?: string;
  text: string;
  /** Explicit addressees (structured, never parsed from the text). Omitted by the tool
   *  path when the agent didn't address anyone — the manager then defaults to the kild
   *  lead. */
  to?: string[];
}

/** Agent→engine control line: an agent called `spawn` to pull in another. */
export interface SpawnOut {
  kind: 'spawn';
  requestId?: string;
  handle: string;
  persona?: string;
  model?: string;
}

/** Agent→engine control line: the kild lead called `stop`. */
export interface StopOut {
  kind: 'stop';
  requestId?: string;
  reason?: string;
}

/** Engine→agent acknowledgement for an explicit kild command. */
export interface CommandAck {
  type: 'command_result';
  requestId: string;
  result: CommandResult<KildActionSuccess>;
}
