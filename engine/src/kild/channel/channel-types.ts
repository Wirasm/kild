/**
 * Channel domain — the primitives for a multi-agent "room": members talk to each
 * other and to the human through one shared message log. Pure types; no behaviour
 * (state lives in the registry, routing in the router, lifecycle in the manager).
 */

/** Reserved member handle for the human operator. Messages to `@human` surface in
 *  the cockpit/CLI only — there is no session to deliver a turn to. */
export const HUMAN = 'human';

/** A participant in a channel: an agent session addressable by its `@name` handle. */
export interface ChannelMember {
  /** The `@mention` handle — equals the agent's name (e.g. `orchestrator`). */
  name: string;
  /** The kild session id running this member. */
  sessionId: string;
  /** The agent definition (`.pi/agents/<agent>.md`) the member runs as. */
  agent: string;
}

/** A single post on a channel's shared log. */
export interface ChannelMessage {
  id: string;
  channelId: string;
  /** Member name of the sender, or {@link HUMAN}. */
  from: string;
  /** Resolved `@mention` handles this post addresses (empty = broadcast to all). */
  mentions: string[];
  text: string;
  /** Epoch millis, stamped by the engine on receipt. */
  ts: number;
}

/** A live channel: a named room with members and a shared message log. */
export interface Channel {
  id: string;
  name: string;
  /** Project directory the members run in (their cwd). */
  cwd: string;
  members: ChannelMember[];
  log: ChannelMessage[];
}

/** Spec to open a channel: who the members are and where they run. */
export interface OpenChannelSpec {
  name: string;
  cwd: string;
  members: Array<{ name: string; agent: string; model?: string }>;
  /** Optional shared worktree name — every member attaches to `kild/<name>`, so the
   *  room collaborates on one branch/tree. Absent → members run in the main checkout. */
  worktree?: string;
}

/** Lightweight channel descriptor for client lists. */
export interface ChannelSummary {
  id: string;
  name: string;
  members: Array<{ name: string; agent: string }>;
}

/** What the engine broadcasts to clients about channels. */
export type ChannelOutbound = { channelMessage: ChannelMessage } | { channels: ChannelSummary[] };

/** The worker→engine control line emitted when a member calls `post_message`.
 *  Distinct from `UiEvent`: it is routed to the channel, not shown as transcript. */
export interface MessageOut {
  kind: 'message_out';
  text: string;
}
