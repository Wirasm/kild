import { expect, test } from 'bun:test';

import { type ChannelDelivery, formatDelivery, routeChannelMessage } from './channel-router.ts';
import type { Channel, ChannelMessage } from './channel-types.ts';

function fixture() {
  const channel: Channel = {
    id: 'c1',
    name: 'demo',
    cwd: '/tmp',
    members: [
      { name: 'orchestrator', sessionId: 's-orch', agent: 'orchestrator' },
      { name: 'worker', sessionId: 's-work', agent: 'worker' },
    ],
    log: [],
  };
  const delivered: Array<{ sessionId: string; text: string }> = [];
  const broadcast: ChannelMessage[] = [];
  const delivery: ChannelDelivery = {
    deliverAsTurn: (sessionId, text) => delivered.push({ sessionId, text }),
    broadcast: (m) => broadcast.push(m),
  };
  return { channel, delivered, broadcast, delivery };
}

function message(from: string, mentions: string[], text: string): ChannelMessage {
  return { id: 'm1', channelId: 'c1', from, mentions, text, ts: 0 };
}

test('delivers a mention to that member as a turn AND broadcasts it', () => {
  const { channel, delivered, broadcast, delivery } = fixture();
  routeChannelMessage(channel, message('orchestrator', ['worker'], '@worker do X'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([{ sessionId: 's-work', text: '[#demo] @orchestrator: @worker do X' }]);
});

test('@human is broadcast only — never delivered as a turn', () => {
  const { channel, delivered, broadcast, delivery } = fixture();
  routeChannelMessage(channel, message('worker', ['human'], '@human done'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([]);
});

test('delivers to other mentioned members but never to the sender or @human', () => {
  const { channel, delivered, delivery } = fixture();
  routeChannelMessage(
    channel,
    message('worker', ['orchestrator', 'human', 'worker'], '@orchestrator @human @worker'),
    delivery,
  );
  expect(delivered.map((d) => d.sessionId)).toEqual(['s-orch']);
});

test('a broadcast (no mentions) still reaches the human, delivers no turns', () => {
  const { channel, delivered, broadcast, delivery } = fixture();
  routeChannelMessage(channel, message('orchestrator', [], 'thinking out loud'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([]);
});

test('formatDelivery frames the post with channel, sender, and text', () => {
  expect(formatDelivery('demo', 'orchestrator', 'do X')).toBe('[#demo] @orchestrator: do X');
});
