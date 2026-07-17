import { expect, test } from 'bun:test';

import {
  formatDelivery,
  hasNoDeliverableRecipients,
  type RoomDelivery,
  routeRoomMessage,
  unknownRecipients,
} from './room-router.ts';
import type { Room, RoomMessage } from './room-types.ts';

function fixture(participantNames: string[] = ['orchestrator', 'worker']) {
  const room: Room = {
    id: 'r1',
    name: 'demo',
    cwd: '/tmp',
    participants: participantNames.map((name) => ({ name, sessionId: `s-${name}`, agent: name })),
    log: [],
    state: 'running',
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

function implicitReply(from: string, to: string[], text: string): RoomMessage {
  return { id: 'm1', roomId: 'r1', from, to, text, ts: 0, implicit: true };
}

/** An engine notice: shown to the operator, addressed to no one (`to: []`). */
function notice(text: string): RoomMessage {
  return { id: 'm1', roomId: 'r1', from: 'human', to: [], text, ts: 0, system: true };
}

test('finds only resolved recipients that are not participants or @human', () => {
  const { room } = fixture();
  expect(
    unknownRecipients(room, message('orchestrator', ['worker', 'human', 'revewer'], '...')),
  ).toEqual(['revewer']);
});

test('does not warn for system notices or implicit replies', () => {
  const { room } = fixture();
  expect(unknownRecipients(room, { ...notice('@revewer joined'), to: ['revewer'] })).toEqual([]);
  expect(unknownRecipients(room, implicitReply('worker', ['revewer'], '...'))).toEqual([]);
});

test('finds a non-participant post with no deliverable recipient in a multi-participant room', () => {
  const { room } = fixture();
  expect(hasNoDeliverableRecipients(room, message('human', [], 'approve the gate'))).toBe(true);
  expect(hasNoDeliverableRecipients(room, message('brain', ['human'], 'approve the gate'))).toBe(
    true,
  );
});

test('does not flag bare posts from participants, single-participant rooms, notices, or narration', () => {
  const { room } = fixture();
  expect(hasNoDeliverableRecipients(room, message('worker', [], 'thinking out loud'))).toBe(false);
  expect(hasNoDeliverableRecipients(room, notice('room update'))).toBe(false);
  expect(hasNoDeliverableRecipients(room, implicitReply('worker', [], 'standing by'))).toBe(false);

  const { room: singleParticipantRoom } = fixture(['worker']);
  expect(
    hasNoDeliverableRecipients(singleParticipantRoom, message('human', [], 'fix the bug')),
  ).toBe(false);
});

test('delivers a mention to that participant as a turn AND broadcasts it', () => {
  const { room, delivered, broadcast, delivery } = fixture();
  routeRoomMessage(room, message('orchestrator', ['worker'], '@worker do X'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([
    { sessionId: 's-worker', from: 'orchestrator', text: '[#demo] @orchestrator: @worker do X' },
  ]);
});

test('`to` is authoritative — text @mentions never re-address a post', () => {
  const { room, delivered, delivery } = fixture();
  // The manager already answered "addressed to whom?" when it recorded the post.
  // Re-parsing the text here would be a second, divergent answer.
  routeRoomMessage(room, message('orchestrator', [], '@worker do X'), delivery);
  expect(delivered).toEqual([]);
});

test('a notice broadcasts but NEVER delivers a turn, even when it names a participant', () => {
  const { room, delivered, broadcast, delivery } = fixture();
  // Regression: `to: []` (addressed to no one) was overridden by re-parsing the text,
  // so joining a room prompted the joiner with "@worker joined the room.".
  routeRoomMessage(room, notice('@worker joined the room.'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([]);
});

test('a notice in a SINGLE-participant room is not delivered by the 1:1 rule', () => {
  const { room, delivered, broadcast, delivery } = fixture(['solo']);
  // Regression: halting a 1:1 room prompted the agent with "Room halted by the operator."
  routeRoomMessage(room, notice('Room halted by the operator.'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([]);
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

test('an implicit reply broadcasts but NEVER delivers a turn (no agent ping-pong)', () => {
  const { room, delivered, broadcast, delivery } = fixture();
  // The reviewer's narration auto-posted back to the orchestrator: human sees it, but
  // it must not prompt the orchestrator — else the two loop forever.
  routeRoomMessage(room, implicitReply('reviewer', ['orchestrator'], 'standing by'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([]);
});

test('an implicit reply that @mentions another agent still delivers no turn', () => {
  const { room, delivered, broadcast, delivery } = fixture();
  routeRoomMessage(room, implicitReply('reviewer', [], '@orchestrator standing by'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([]); // explicit post_message is required to prompt an agent
});

test('formatDelivery frames the post with room, sender, and text', () => {
  expect(formatDelivery('demo', 'orchestrator', 'do X')).toBe('[#demo] @orchestrator: do X');
});
