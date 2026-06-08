import { parseMentions } from './parse-mentions.ts';
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

/** A message's addressees: explicit `to` if set, else the `@mentions` in the text. */
function resolveAddressees(message: RoomMessage): string[] {
  return message.to.length > 0 ? message.to : parseMentions(message.text);
}

/**
 * Route one already-recorded post: show it to the human (broadcast), then deliver
 * it as a turn to each addressed participant. Addressing rules:
 * - explicit `to` wins, else `@mentions` in the text;
 * - **no addressee + exactly one participant → that participant** — so a bare post in
 *   a single-agent room reaches the agent, and it chats like a 1:1 session;
 * - no addressee + multiple participants → broadcast only (no turn).
 * `@human` is never delivered as a turn (the broadcast is how the operator receives),
 * and a participant is never delivered its own post.
 */
export function routeRoomMessage(room: Room, message: RoomMessage, delivery: RoomDelivery): void {
  delivery.broadcast(message);

  let targets = resolveAddressees(message).filter((t) => t !== HUMAN && t !== message.from);
  if (targets.length === 0 && room.participants.length === 1) {
    const sole = room.participants[0];
    if (sole && sole.name !== message.from) targets = [sole.name];
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
