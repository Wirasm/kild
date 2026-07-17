import type { UiEvent } from '../events.ts';

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
  /** Session that opened this room. It is notified only when it is not a participant. */
  openedBy?: string;
  participants: RoomParticipant[];
  log: RoomMessage[];
  /** Canonical lifecycle state for this room. */
  state: RoomLifecycleState;
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
  /** Opener session identity from a session-aware REST caller; absent for ordinary REST callers. */
  openedBy?: string;
}

/** Lightweight room descriptor for client lists. */
export interface RoomSummary {
  id: string;
  name: string;
  worktree?: string;
  participants: Array<{ name: string; agent?: string }>;
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
  participants: Array<{ name: string; agent?: string }>;
  /** Canonical lifecycle state when persisted by room-owned producers. Optional so
   *  older history files and out-of-scope fixtures continue to type-check. */
  state?: RoomLifecycleState;
  log: RoomMessage[];
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
  /** Explicit addressees. The implicit-reply path sets this; the tool path omits it
   *  and the router falls back to parsing `@mentions` from the text. */
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
