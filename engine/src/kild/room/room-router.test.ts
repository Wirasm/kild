import { expect, test } from 'bun:test';

import { formatDelivery, type RoomDelivery, routeRoomMessage } from './room-router.ts';
import type { Room, RoomMessage } from './room-types.ts';

function fixture(participantNames: string[] = ['orchestrator', 'worker']) {
  const room: Room = {
    id: 'r1',
    name: 'demo',
    cwd: '/tmp',
    participants: participantNames.map((name) => ({ name, sessionId: `s-${name}`, agent: name })),
    log: [],
  };
  const delivered: Array<{ sessionId: string; from: string; text: string }> = [];
  const broadcast: RoomMessage[] = [];
  const delivery: RoomDelivery = {
    deliverAsTurn: (sessionId, from, text) => delivered.push({ sessionId, from, text }),
    broadcast: (m) => broadcast.push(m),
  };
  return { room, delivered, broadcast, delivery };
}

function message(from: string, to: string[], text: string): RoomMessage {
  return { id: 'm1', roomId: 'r1', from, to, text, ts: 0 };
}

test('delivers a mention to that participant as a turn AND broadcasts it', () => {
  const { room, delivered, broadcast, delivery } = fixture();
  routeRoomMessage(room, message('orchestrator', ['worker'], '@worker do X'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([
    { sessionId: 's-worker', from: 'orchestrator', text: '[#demo] @orchestrator: @worker do X' },
  ]);
});

test('falls back to parsing @mentions from the text when `to` is empty', () => {
  const { room, delivered, delivery } = fixture();
  routeRoomMessage(room, message('orchestrator', [], '@worker do X'), delivery);
  expect(delivered.map((d) => d.sessionId)).toEqual(['s-worker']);
});

test('@human is broadcast only — never delivered as a turn', () => {
  const { room, delivered, broadcast, delivery } = fixture();
  routeRoomMessage(room, message('worker', ['human'], '@human done'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([]);
});

test('delivers to other addressed participants but never the sender or @human', () => {
  const { room, delivered, delivery } = fixture();
  routeRoomMessage(room, message('worker', ['orchestrator', 'human', 'worker'], '...'), delivery);
  expect(delivered.map((d) => d.sessionId)).toEqual(['s-orchestrator']);
});

test('no addressee in a SINGLE-participant room → delivered to the sole agent (chats like 1:1)', () => {
  const { room, delivered } = fixture(['solo']);
  const delivery: RoomDelivery = {
    deliverAsTurn: (sessionId, from, text) => delivered.push({ sessionId, from, text }),
    broadcast: () => {},
  };
  routeRoomMessage(room, message('human', [], 'fix the bug'), delivery);
  expect(delivered).toEqual([
    { sessionId: 's-solo', from: 'human', text: '[#demo] @human: fix the bug' },
  ]);
});

test('no addressee in a MULTI-participant room → broadcast only, no turn', () => {
  const { room, delivered, broadcast, delivery } = fixture();
  routeRoomMessage(room, message('orchestrator', [], 'thinking out loud'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([]);
});

test('formatDelivery frames the post with room, sender, and text', () => {
  expect(formatDelivery('demo', 'orchestrator', 'do X')).toBe('[#demo] @orchestrator: do X');
});
