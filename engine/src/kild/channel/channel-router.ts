import { type Channel, type ChannelMessage, HUMAN } from './channel-types.ts';

/** The side-effects routing needs, injected so the router stays decoupled from the
 *  SessionManager and the WS layer (and unit-testable on its own). */
export interface ChannelDelivery {
  /** Deliver a post to a member as a new turn (prompt its session). */
  deliverAsTurn: (sessionId: string, text: string) => void;
  /** Broadcast a post to all clients so the human (CLI/UI) sees it. */
  broadcast: (message: ChannelMessage) => void;
}

/** How a delivered post reads to the member receiving it: who, where, what. */
export function formatDelivery(channelName: string, from: string, text: string): string {
  return `[#${channelName}] @${from}: ${text}`;
}

/**
 * Route one already-recorded post: show it to the human (broadcast), then deliver
 * it as a turn to every mentioned member. The broadcast happens regardless of
 * mentions so the channel reads as one shared log. `@human` is never delivered as
 * a turn — the broadcast is how the operator "receives" it — and a member is never
 * delivered its own post.
 */
export function routeChannelMessage(
  channel: Channel,
  message: ChannelMessage,
  delivery: ChannelDelivery,
): void {
  delivery.broadcast(message);
  for (const mention of message.mentions) {
    if (mention === HUMAN || mention === message.from) continue;
    const member = channel.members.find((m) => m.name === mention);
    if (member) {
      delivery.deliverAsTurn(
        member.sessionId,
        formatDelivery(channel.name, message.from, message.text),
      );
    }
  }
}
