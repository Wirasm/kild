import { randomUUID } from 'node:crypto';

import { sessionManager } from '../sessions.ts';
import { ChannelRegistry } from './channel-registry.ts';
import { type ChannelDelivery, routeChannelMessage } from './channel-router.ts';
import {
  type Channel,
  type ChannelMessage,
  type ChannelOutbound,
  HUMAN,
  type OpenChannelSpec,
} from './channel-types.ts';
import { parseMentions } from './parse-mentions.ts';

/**
 * Owns live channels: opens them (one session per member), routes every post
 * (member→member as turns, everything to the human as a broadcast), and closes
 * them (the human kill switch — the only loop control in v1). Sits beside the
 * SessionManager: members ARE sessions, so they appear in the cockpit like any
 * other, and the SessionManager stays channel-agnostic (it only forwards a
 * member's `message_out` to the callback we hand it).
 */
class ChannelManager {
  private readonly registry = new ChannelRegistry();
  private readonly subscribers = new Set<(msg: ChannelOutbound) => void>();

  subscribe(fn: (msg: ChannelOutbound) => void): () => void {
    this.subscribers.add(fn);
    fn({ channels: this.registry.summaries() }); // catch the new client up
    return () => {
      this.subscribers.delete(fn);
    };
  }

  /** Open a channel under a caller-supplied id: spawn one member session per spec
   *  entry, each wired so its `post_message` calls route back here. */
  open(channelId: string, spec: OpenChannelSpec): void {
    const channel: Channel = {
      id: channelId,
      name: spec.name,
      cwd: spec.cwd,
      members: [],
      log: [],
    };
    this.registry.create(channel);

    for (const m of spec.members) {
      const sessionId = randomUUID();
      channel.members.push({ name: m.name, sessionId, agent: m.agent });
      sessionManager.spawn(
        sessionId,
        {
          model: m.model,
          cwd: spec.cwd,
          agent: m.agent,
          projectName: spec.name,
          // Opaque to the SessionManager; the worker reads these to register
          // `post_message` and tag its outbound messages with this member.
          env: { KILD_CHANNEL: channelId, KILD_MEMBER: m.name },
        },
        'cli',
        (text) => this.handleMemberMessage(sessionId, text),
      );
    }
    this.broadcast({ channels: this.registry.summaries() });
  }

  /** The human posts into the channel (kick-off and steering). */
  postFromHuman(channelId: string, text: string): void {
    this.post(channelId, HUMAN, text);
  }

  /** Stop every member session — the human kill switch / channel teardown. */
  close(channelId: string): void {
    const channel = this.registry.get(channelId);
    if (!channel) return;
    for (const member of channel.members) sessionManager.stop(member.sessionId);
    this.registry.remove(channelId);
    this.broadcast({ channels: this.registry.summaries() });
  }

  /** A member called `post_message`: resolve which channel/member it was and route. */
  private handleMemberMessage(sessionId: string, text: string): void {
    const located = this.registry.locateSession(sessionId);
    if (!located) return; // session is not (or no longer) a channel member
    this.post(located.channel.id, located.member.name, text);
  }

  /** Record + route one post from `from` (a member name or {@link HUMAN}). */
  private post(channelId: string, from: string, text: string): void {
    const channel = this.registry.get(channelId);
    if (!channel) return;
    const message: ChannelMessage = {
      id: randomUUID(),
      channelId,
      from,
      mentions: parseMentions(text),
      text,
      ts: Date.now(),
    };
    this.registry.appendMessage(channelId, message);
    routeChannelMessage(channel, message, this.delivery());
  }

  private delivery(): ChannelDelivery {
    return {
      deliverAsTurn: (sessionId, text) => sessionManager.prompt(sessionId, text),
      broadcast: (message) => this.broadcast({ channelMessage: message }),
    };
  }

  private broadcast(msg: ChannelOutbound): void {
    for (const fn of this.subscribers) fn(msg);
  }
}

/** Engine-wide singleton, mirroring {@link sessionManager}. */
export const channelManager = new ChannelManager();
