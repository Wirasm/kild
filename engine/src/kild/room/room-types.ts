import type { UiEvent } from '../events.ts';
import type { WorkstreamGitStatus } from '../worktree-status.ts';
import type { RoomDecision } from './room-decisions.ts';

/**
 * Room domain — the operator-facing primitive: a set of participants (agent
 * instances + the human) exchanging Messages on one shared log. A single-agent
 * "session" is just a 1-participant Room. Pure types; no behaviour (state lives in
 * the registry, routing in the router, lifecycle in the manager).
 */

/** Reserved participant handle for the human operator. Messages to `@human` surface
 *  in the cockpit/CLI only — there is no session to deliver a turn to. */
export const HUMAN = 'human';

/** A participant in a room: an agent instance addressable by its `@name` handle.
 *  (The human is a virtual participant — never in this list.) */
export interface RoomParticipant {
  /** The `@mention` handle — equals the agent's name (e.g. `orchestrator`). */
  name: string;
  /** The kild session id running this participant. */
  sessionId: string;
  /** The agent definition (`.pi/agents/<agent>.md`) it runs as. */
  agent?: string;
  /** The model this participant runs on — the requested ref at spawn, upgraded to the
   *  provider-resolved `provider/id` once the session reports it. Lets an observer see
   *  which model each agent used in a run. */
  model?: string;
  /** Who invited this participant — the inviting participant's name, or {@link HUMAN} for
   *  the opener's initial roster. Ground-truth spawn edge (vs inferring it from the log)
   *  and the routing target for its idle/done notice. */
  invitedBy?: string;
  /** The underlying pi session id — the durable terminal-resume handle
   *  (`pi --session <piSessionFile ?? piSessionId>`). Captured from the worker's
   *  `pi_session` event and persisted with the room, so any agent in any room —
   *  live or archived — can be reopened in the pi CLI. */
  piSessionId?: string;
  /** Absolute pi session file path (resume works from any cwd). */
  piSessionFile?: string;
  /** True when the session has finished a turn and is waiting. Set on `agent_end`,
   *  cleared when a new prompt is delivered. Dedups the idle failsafe to one check per
   *  active→idle transition, and rides {@link ParticipantView} so observers can rank
   *  finished-and-waiting rooms without parsing logs. */
  idle?: boolean;
  /** True once this participant made an EXPLICIT post_message since its last activation.
   *  If it goes idle with this still false, it finished without reporting — the failsafe
   *  nudges it to post. Reset when a new turn is delivered. Rides {@link ParticipantView}
   *  with `idle` (idle+posted = finished AND reported). */
  posted?: boolean;
  /** Latest cumulative token count for this participant's session, captured from its
   *  `stats` UiEvents (emitted at each turn end). */
  tokens?: number;
  /** Latest cumulative cost (USD) for this participant's session, from `stats` UiEvents. */
  cost?: number;
}

/** A participant as surfaced to observers (room lists, status, archive) — identity plus
 *  the model it ran on. No kild sessionId (that's an internal handle); the PI session
 *  identity IS exposed — it's the durable handle for reopening the agent in a terminal. */
export interface ParticipantView {
  name: string;
  agent?: string;
  model?: string;
  piSessionId?: string;
  piSessionFile?: string;
  /** Attention state: finished a turn and waiting for input (see {@link RoomParticipant.idle}). */
  idle?: boolean;
  /** Attention state: reported via an explicit delivered post this activation. */
  posted?: boolean;
  /** Latest cumulative session token count (from `stats` UiEvents). */
  tokens?: number;
  /** Latest cumulative session cost in USD (from `stats` UiEvents). */
  cost?: number;
}

/** The one mapping from a live participant to its observer view — every list/status/
 *  archive producer uses this so new view fields appear everywhere at once. */
export function participantView(participant: RoomParticipant): ParticipantView {
  return {
    name: participant.name,
    agent: participant.agent,
    model: participant.model,
    piSessionId: participant.piSessionId,
    piSessionFile: participant.piSessionFile,
    idle: participant.idle,
    posted: participant.posted,
    tokens: participant.tokens,
    cost: participant.cost,
  };
}

/** A room's cost rollup, summed over the participants that have reported `stats`. */
export interface RoomCostTotals {
  tokens: number;
  cost: number;
}

/** Sum participant costs into a room total — undefined until at least one participant
 *  has reported stats, so payloads without cost data stay clean of zero-noise. */
export function roomCostTotals(
  participants: Array<Pick<ParticipantView, 'tokens' | 'cost'>>,
): RoomCostTotals | undefined {
  const reported = participants.filter((p) => p.tokens !== undefined || p.cost !== undefined);
  if (reported.length === 0) return undefined;
  return {
    tokens: reported.reduce((sum, p) => sum + (p.tokens ?? 0), 0),
    cost: reported.reduce((sum, p) => sum + (p.cost ?? 0), 0),
  };
}

/** A single post on a room's shared log — the conversation unit. */
export interface RoomMessage {
  id: string;
  roomId: string;
  /** Participant name of the sender, or {@link HUMAN}. */
  from: string;
  /** Resolved addressee handles (empty = broadcast to all, no turn delivered). */
  to: string[];
  text: string;
  /** Epoch millis, stamped by the engine on receipt. */
  ts: number;
  /** True when this is an agent's turn-final text auto-posted as its reply (it did
   *  not call `post_message` itself). */
  implicit?: boolean;
  /** True for engine-generated notices (e.g. a participant joining). */
  system?: boolean;
}

/** The canonical room lifecycle — transitions are enforced centrally by the room
 *  manager/lifecycle helper rather than inferred from booleans or registry presence. */
export type RoomLifecycleState = 'opening' | 'running' | 'halted' | 'closed';

/** A live room: participants + a shared message log + a workspace. */
export interface Room {
  id: string;
  name: string;
  /** Project directory the participants run in (their cwd). */
  cwd: string;
  /** Optional shared worktree name — every participant attaches to `kild/<name>`. */
  worktree?: string;
  /** Base branch the worktree was created from and that git status/collisions are
   *  measured against (so ahead/behind and changed files reflect this workstream's own
   *  work, not everything the base is ahead of `main`). */
  base?: string;
  /** Session that opened this room. It is notified only when it is not a participant. */
  openedBy?: string;
  participants: RoomParticipant[];
  log: RoomMessage[];
  /** Canonical lifecycle state for this room. */
  state: RoomLifecycleState;
  /** Keyed decision ledger folded from `needs-decision[key]:` / `resolved[key]` post
   *  markers (see room-decisions). Optional so out-of-scope fixtures stay untouched. */
  decisions?: RoomDecision[];
}

/** A participant to spawn into a room. */
export interface ParticipantSpec {
  name: string;
  agent?: string;
  model?: string;
}

/** Spec to open a room: who the participants are and where they run. */
export interface OpenRoomSpec {
  name: string;
  cwd: string;
  participants: ParticipantSpec[];
  /** Optional shared worktree — every participant attaches to one `kild/<name>` tree. */
  worktree?: string;
  /** Base branch for the worktree + git-status baseline (default: the checkout's current
   *  branch). Editable via `.kild/config.json` `baseBranch` or the `--base` CLI flag. */
  base?: string;
  /** Opener session identity from a session-aware REST caller; absent for ordinary REST callers. */
  openedBy?: string;
}

/** Lightweight room descriptor for client lists. */
export interface RoomSummary {
  id: string;
  name: string;
  worktree?: string;
  participants: ParticipantView[];
  /** Canonical lifecycle state when surfaced by room-owned producers. Optional so
   *  out-of-scope fixtures/consumers do not need coordinated edits in this slice. */
  state?: RoomLifecycleState;
  /** True when the operator has halted the room (sessions stopped, kept read-only).
   *  Derived compatibility field for existing non-room consumers. */
  stopped?: boolean;
}

/** A room recovered from disk after an engine restart — its conversation log with no
 *  live participants (their sessions are gone). The cockpit renders it read-only. */
export interface ArchivedRoom {
  id: string;
  name: string;
  worktree?: string;
  participants: ParticipantView[];
  /** Canonical lifecycle state when persisted by room-owned producers. Optional so
   *  older history files and out-of-scope fixtures continue to type-check. */
  state?: RoomLifecycleState;
  log: RoomMessage[];
  /** Keyed decision ledger (see room-decisions). Optional: older history files predate it. */
  decisions?: RoomDecision[];
}

/** A live room enriched with its workstream's git/worktree state — the code-state
 *  half of observability, so a driving agent can land work and avoid collisions. Git is
 *  live-only (never persisted); computed on demand when serving live-room status. */
export interface LiveRoomStatus extends ArchivedRoom {
  git?: WorkstreamGitStatus;
  /** Room cost rollup summed over participants — absent until stats have arrived. */
  totals?: RoomCostTotals;
}

/** Typed room-domain result: every command either succeeds with a value or fails with
 *  an explicit room error code + message. */
export type CommandResult<T> =
  | { ok: true; value: T }
  | { ok: false; code: RoomErrorCode; message: string };

/** Room-domain failure categories — transport-agnostic, mapped by REST/worker layers. */
export type RoomErrorCode = 'not_found' | 'invalid_state' | 'rejected';

export interface RoomActionSuccess {
  message: string;
  /** For posts: the resolved recipients other than the sender (the human counts — it is
   *  the operator channel). Empty = the post reached no one, e.g. a self-addressed post;
   *  the idle failsafe treats such a post as not-a-report. */
  deliveredTo?: string[];
}

export interface OpenRoomSuccess extends RoomActionSuccess {
  roomId: string;
}

/** What the engine broadcasts to clients about rooms. */
export type RoomOutbound =
  | { roomMessage: RoomMessage }
  | { rooms: RoomSummary[] }
  /** A room that just closed with history — pushed so clients show it as read-only
   *  history immediately, without refetching the archive or restarting. */
  | { archivedRoom: ArchivedRoom }
  /** A participant's transcript event (its UiEvent stream), tagged by room+participant. */
  | { room: string; participant: string; event: UiEvent };

/** Worker→engine control line: an agent called `post_message`. Distinct from a
 *  `UiEvent` — routed to the room, not shown as the participant's raw transcript. */
export interface MessageOut {
  kind: 'message_out';
  requestId?: string;
  text: string;
  /** Explicit addressees (structured, never parsed from the text). Omitted by the tool
   *  path when the agent didn't address anyone — the manager then defaults to the room
   *  lead. The implicit-reply path sets it to the turn's sender. */
  to?: string[];
  implicit?: boolean;
}

/** Worker→engine control line: an agent called `invite_agent` to pull in another. */
export interface InviteOut {
  kind: 'invite';
  requestId?: string;
  name: string;
  agent?: string;
  model?: string;
}

/** Worker→engine control line: the room lead called `close_room`. */
export interface CloseRoomOut {
  kind: 'close_room';
  requestId?: string;
  reason?: string;
}

/** Engine→worker acknowledgement for an explicit room command. */
export interface RoomCommandAck {
  type: 'room_command_result';
  requestId: string;
  result: CommandResult<RoomActionSuccess>;
}
