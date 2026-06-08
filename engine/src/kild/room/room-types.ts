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

/** A live room: participants + a shared message log + a workspace. */
export interface Room {
  id: string;
  name: string;
  /** Project directory the participants run in (their cwd). */
  cwd: string;
  /** Optional shared worktree name — every participant attaches to `kild/<name>`. */
  worktree?: string;
  participants: RoomParticipant[];
  log: RoomMessage[];
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
}

/** Lightweight room descriptor for client lists. */
export interface RoomSummary {
  id: string;
  name: string;
  worktree?: string;
  participants: Array<{ name: string; agent?: string }>;
}

/** What the engine broadcasts to clients about rooms. */
export type RoomOutbound =
  | { roomMessage: RoomMessage }
  | { rooms: RoomSummary[] }
  /** A participant's transcript event (its UiEvent stream), tagged by room+participant. */
  | { room: string; participant: string; event: UiEvent };

/** Worker→engine control line: an agent called `post_message`. Distinct from a
 *  `UiEvent` — routed to the room, not shown as the participant's raw transcript. */
export interface MessageOut {
  kind: 'message_out';
  text: string;
  /** Explicit addressees. The implicit-reply path sets this; the tool path omits it
   *  and the router falls back to parsing `@mentions` from the text. */
  to?: string[];
  implicit?: boolean;
}

/** Worker→engine control line: an agent called `invite_agent` to pull in another. */
export interface InviteOut {
  kind: 'invite';
  name: string;
  agent?: string;
  model?: string;
}
