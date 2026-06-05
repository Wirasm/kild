import type { Channel, ChannelMember, ChannelMessage, ChannelSummary } from './channel-types.ts';

/**
 * In-memory store of live channels and their message logs — runtime state only,
 * no behaviour (delivery/broadcast live in the router/manager). We prove the model
 * in memory first; a JSON-file backing comes later, behind this same interface.
 */
export class ChannelRegistry {
  private readonly channels = new Map<string, Channel>();

  create(channel: Channel): void {
    this.channels.set(channel.id, channel);
  }

  get(channelId: string): Channel | undefined {
    return this.channels.get(channelId);
  }

  remove(channelId: string): void {
    this.channels.delete(channelId);
  }

  /** Find which channel + member a session belongs to — the reverse lookup that
   *  routes a member's `message_out` back to its channel. */
  locateSession(sessionId: string): { channel: Channel; member: ChannelMember } | undefined {
    for (const channel of this.channels.values()) {
      const member = channel.members.find((m) => m.sessionId === sessionId);
      if (member) return { channel, member };
    }
    return undefined;
  }

  appendMessage(channelId: string, message: ChannelMessage): void {
    this.channels.get(channelId)?.log.push(message);
  }

  summaries(): ChannelSummary[] {
    return [...this.channels.values()].map((c) => ({
      id: c.id,
      name: c.name,
      members: c.members.map((m) => ({ name: m.name, agent: m.agent })),
    }));
  }
}
