import { expect, test } from 'bun:test';

import { Mailbox } from './attached.ts';
import {
  formatDelivery,
  type RoomDelivery,
  routeRoomMessage,
  unknownRecipients,
} from './room-router.ts';
import type { Room, RoomMessage, RoomParticipant } from './room-types.ts';

/** Participant names prefixed `@` are ATTACHED (kild does not own them). */
function participant(name: string): RoomParticipant {
  return name.startsWith('@')
    ? { name: name.slice(1), kind: 'attached', mailbox: new Mailbox() }
    : { name, sessionId: `s-${name}`, persona: name };
}

function fixture(participantNames: string[] = ['orchestrator', 'worker']) {
  const room: Room = {
    id: 'r1',
    name: 'demo',
    cwd: '/tmp',
    participants: participantNames.map(participant),
    log: [],
    state: 'running',
  };
  const delivered: Array<{ sessionId: string; from: string; text: string }> = [];
  const queued: Array<{ name: string; from: string; text: string }> = [];
  const broadcast: RoomMessage[] = [];
  const delivery: RoomDelivery = {
    deliverAsTurn: (sessionId, from, text) => delivered.push({ sessionId, from, text }),
    queueForAttached: (target, from, text) => queued.push({ name: target.name, from, text }),
    broadcast: (m) => broadcast.push(m),
  };
  return { room, delivered, queued, broadcast, delivery };
}

function message(from: string, to: string[], text: string): RoomMessage {
  return { id: 'm1', roomId: 'r1', from, to, text, ts: 0 };
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

test('does not warn for system notices', () => {
  const { room } = fixture();
  expect(unknownRecipients(room, { ...notice('@revewer joined'), to: ['revewer'] })).toEqual([]);
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

test('@human is not delivered a turn, but a non-lead @human post ALSO wakes the lead', () => {
  const { room, delivered, broadcast, delivery } = fixture(); // lead = orchestrator
  routeRoomMessage(room, message('worker', ['human'], '@human done'), delivery);
  expect(broadcast).toHaveLength(1);
  // @human gets no turn (not a session), but the lead is woken so it isn't left blind.
  expect(delivered).toEqual([
    { sessionId: 's-orchestrator', from: 'worker', text: '[#demo] @worker: @human done' },
  ]);
});

test("the lead's own @human post is terminal — wakes no one", () => {
  const { room, delivered, broadcast, delivery } = fixture(); // lead = orchestrator
  routeRoomMessage(room, message('orchestrator', ['human'], 'final report'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([]); // the lead is the sender, so it isn't re-woken
});

test('a non-lead @human post does not double-deliver when the lead is already addressed', () => {
  const { room, delivered, delivery } = fixture();
  routeRoomMessage(room, message('worker', ['orchestrator', 'human'], '...'), delivery);
  expect(delivered.map((x) => x.sessionId)).toEqual(['s-orchestrator']); // once, not twice
});

test('delivers to other addressed participants but never the sender or @human', () => {
  const { room, delivered, delivery } = fixture();
  routeRoomMessage(room, message('worker', ['orchestrator', 'human', 'worker'], '...'), delivery);
  expect(delivered.map((d) => d.sessionId)).toEqual(['s-orchestrator']);
});

test('no addressee in a SINGLE-participant room → delivered to the sole agent (chats like 1:1)', () => {
  const { room, delivered, delivery } = fixture(['solo']);
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

test('a post addressed to an ATTACHED participant queues instead of prompting', () => {
  const { room, delivered, queued, broadcast, delivery } = fixture(['orchestrator', '@claude']);
  routeRoomMessage(room, message('orchestrator', ['claude'], 'review is done'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([]); // nothing to push to — kild does not own that process
  // Structured and RAW: how it reads is decided at the drain, not in the router.
  expect(queued).toEqual([{ name: 'claude', from: 'orchestrator', text: 'review is done' }]);
});

test('a mixed room routes both ways from one post', () => {
  const { room, delivered, queued, delivery } = fixture(['orchestrator', 'worker', '@claude']);
  routeRoomMessage(room, message('orchestrator', ['worker', 'claude'], 'status?'), delivery);
  expect(delivered).toEqual([
    { sessionId: 's-worker', from: 'orchestrator', text: '[#demo] @orchestrator: status?' },
  ]);
  expect(queued).toEqual([{ name: 'claude', from: 'orchestrator', text: 'status?' }]);
});

test('a post the attached participant is not addressed in never reaches its mailbox', () => {
  const { room, queued, delivery } = fixture(['orchestrator', 'worker', '@claude']);
  // No addressee in a multi-participant room → broadcast only; the mailbox is not a
  // firehose, or the wake cap would trip on traffic @claude was never asked to read.
  routeRoomMessage(room, message('worker', [], 'thinking out loud'), delivery);
  routeRoomMessage(room, message('worker', ['orchestrator'], 'for you only'), delivery);
  expect(queued).toEqual([]);
});
