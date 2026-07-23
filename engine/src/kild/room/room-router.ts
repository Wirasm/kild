import { HUMAN, type Room, type RoomMessage } from './room-types.ts';

/** The side-effects routing needs, injected so the router stays decoupled from the
 *  SessionManager and the WS layer (and unit-testable on its own). */
export interface RoomDelivery {
  /** Deliver a post to a participant as a new turn (prompt its session). `from` is
   *  the sender, passed structured so the participant can reply to it. */
  deliverAsTurn: (sessionId: string, from: string, text: string) => void;
  /** Broadcast a post to all clients so the human (CLI/UI) sees it. */
  broadcast: (message: RoomMessage) => void;
}

/** How a delivered post reads to the participant receiving it: who, where, what. */
export function formatDelivery(roomName: string, from: string, text: string): string {
  return `[#${roomName}] @${from}: ${text}`;
}

/** Return resolved addressees that cannot receive a post in this room. Engine notices
 * and implicit replies are never user-addressed, so they cannot yield a warning. */
export function unknownRecipients(room: Room, message: RoomMessage): string[] {
  if (message.system || message.implicit) return [];
  const participants = new Set(room.participants.map((participant) => participant.name));
  return message.to.filter((recipient) => recipient !== HUMAN && !participants.has(recipient));
}

/**
 * Route one already-recorded post: show it to the human (broadcast), then deliver
 * it as a turn to each addressed participant.
 *
 * **`message.to` is authoritative.** The room manager resolved it when it recorded the
 * post — that is the one place that answers "who is this addressed to?" (a system notice
 * → nobody, else an explicit `to`, else the room lead). Addressing is a structured list,
 * never parsed from the message text — the router must never re-derive addressees from
 * prose. Re-deriving would be a second, divergent answer: it once silently overrode a
 * deliberate empty `to`, so a system notice like "@worker joined the room." prompted
 * @worker with a turn.
 *
 * This function owns only *delivery* policy — who actually gets prompted:
 * - `@human` is not a session, so it's never delivered a turn — the broadcast is how the
 *   operator receives. BUT a `@human` post from a non-lead ALSO wakes the room lead (the
 *   human's in-room proxy/coordinator), so a sub-agent "reporting to the human" never
 *   leaves the lead blind. The lead's own posts to `@human` stay terminal (wake no one).
 * - a participant is never delivered its own post;
 * - **no addressee + exactly one participant → that participant** — so a bare post in
 *   a single-agent room reaches the agent, and it chats like a 1:1 session;
 * - no addressee + multiple participants → broadcast only (no turn).
 *
 * **Notices and implicit replies broadcast but never deliver a turn.** A notice is
 * engine-generated (a participant joining, the room halting): the operator should see it,
 * but it addresses no one — waking an agent with it is noise at best and, for a halt,
 * the opposite of the intent.
 *
 * **Implicit replies broadcast but never deliver a turn.** An implicit reply is an
 * agent's turn-final narration auto-posted because it did not call `post_message`. If
 * we delivered those as turns, two agents would ping-pong forever — each delivered turn
 * produces narration that is delivered back, so "I'll stay quiet" becomes a message
 * that prompts the other agent. Prompting another agent therefore requires an *explicit*
 * `post_message` (@mention) — exactly what the room agent prompts instruct ("no one sees
 * your normal output — only what you post"). The human still sees the narration (it's
 * broadcast); it just doesn't drive another turn.
 */
export function routeRoomMessage(room: Room, message: RoomMessage, delivery: RoomDelivery): void {
  delivery.broadcast(message);
  // Narration and engine notices are shown to the human, never delivered as a turn.
  if (message.implicit || message.system) return;

  const targets = message.to.filter((t) => t !== HUMAN && t !== message.from);
  if (targets.length === 0 && room.participants.length === 1) {
    const sole = room.participants[0];
    if (sole && sole.name !== message.from) targets.push(sole.name);
  }

  // A `@human` report from a non-lead also wakes the room lead — so an agent that reports
  // "to the human" keeps the coordinator in the loop instead of leaving it blind. The lead
  // itself reporting to the human stays terminal (it's the sender, so it's not re-woken).
  const lead = room.participants[0];
  if (
    message.to.includes(HUMAN) &&
    lead &&
    lead.name !== message.from &&
    !targets.includes(lead.name)
  ) {
    targets.push(lead.name);
  }

  for (const name of targets) {
    const participant = room.participants.find((p) => p.name === name);
    if (participant) {
      delivery.deliverAsTurn(
        participant.sessionId,
        message.from,
        formatDelivery(room.name, message.from, message.text),
      );
    }
  }
}
