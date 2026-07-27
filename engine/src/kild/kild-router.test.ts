import { expect, test } from 'bun:test';

import { Inbox } from './inbox.ts';
import { type Delivery, formatDelivery, routeMessage, unknownRecipients } from './kild-router.ts';
import type { Agent, Kild, Message } from './kild-types.ts';

/** Agent handles prefixed `@` are ATTACHED (kild does not own them). */
function agent(handle: string): Agent {
  return handle.startsWith('@')
    ? { handle: handle.slice(1), ownership: 'attached', inbox: new Inbox() }
    : { handle, id: `s-${handle}`, persona: handle };
}

function fixture(handles: string[] = ['orchestrator', 'coder']) {
  const kild: Kild = {
    id: 'r1',
    name: 'demo',
    cwd: '/tmp',
    agents: handles.map(agent),
    log: [],
    state: 'running',
  };
  const delivered: Array<{ agentId: string; from: string; text: string }> = [];
  const queued: Array<{ handle: string; from: string; text: string }> = [];
  const broadcast: Message[] = [];
  const delivery: Delivery = {
    deliverAsTurn: (agentId, from, text) => delivered.push({ agentId, from, text }),
    queueForAttached: (target, from, text) => queued.push({ handle: target.handle, from, text }),
    broadcast: (m) => broadcast.push(m),
  };
  return { kild, delivered, queued, broadcast, delivery };
}

function message(from: string, to: string[], text: string): Message {
  return { id: 'm1', kildId: 'r1', from, to, text, ts: 0 };
}

/** An engine notice: shown to the operator, addressed to no one (`to: []`). */
function notice(text: string): Message {
  return { id: 'm1', kildId: 'r1', from: 'human', to: [], text, ts: 0, system: true };
}

test('finds only resolved recipients that are not agents or @human', () => {
  const { kild } = fixture();
  expect(
    unknownRecipients(kild, message('orchestrator', ['coder', 'human', 'revewer'], '...')),
  ).toEqual(['revewer']);
});

test('does not warn for system notices', () => {
  const { kild } = fixture();
  expect(unknownRecipients(kild, { ...notice('@revewer joined'), to: ['revewer'] })).toEqual([]);
});

test('delivers a mention to that agent as a turn AND broadcasts it', () => {
  const { kild, delivered, broadcast, delivery } = fixture();
  routeMessage(kild, message('orchestrator', ['coder'], '@coder do X'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([
    { agentId: 's-coder', from: 'orchestrator', text: '[#demo] @orchestrator: @coder do X' },
  ]);
});

test('`to` is authoritative — text @mentions never re-address a message', () => {
  const { kild, delivered, delivery } = fixture();
  // The manager already answered "addressed to whom?" when it recorded the message.
  // Re-parsing the text here would be a second, divergent answer.
  routeMessage(kild, message('orchestrator', [], '@coder do X'), delivery);
  expect(delivered).toEqual([]);
});

test('a notice broadcasts but NEVER delivers a turn, even when it names an agent', () => {
  const { kild, delivered, broadcast, delivery } = fixture();
  // Regression: `to: []` (addressed to no one) was overridden by re-parsing the text,
  // so joining a kild prompted the joiner with "@coder joined the kild.".
  routeMessage(kild, notice('@coder joined the kild.'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([]);
});

test('a notice in a SINGLE-agent kild is not delivered by the 1:1 rule', () => {
  const { kild, delivered, broadcast, delivery } = fixture(['solo']);
  // Regression: halting a 1:1 kild prompted the agent with "Kild halted by the operator."
  routeMessage(kild, notice('Kild halted by the operator.'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([]);
});

test('@human is not delivered a turn, but a non-lead @human message ALSO wakes the lead', () => {
  const { kild, delivered, broadcast, delivery } = fixture(); // lead = orchestrator
  routeMessage(kild, message('coder', ['human'], '@human done'), delivery);
  expect(broadcast).toHaveLength(1);
  // @human gets no turn (not a session), but the lead is woken so it isn't left blind.
  expect(delivered).toEqual([
    { agentId: 's-orchestrator', from: 'coder', text: '[#demo] @coder: @human done' },
  ]);
});

test("the lead's own @human message is terminal — wakes no one", () => {
  const { kild, delivered, broadcast, delivery } = fixture(); // lead = orchestrator
  routeMessage(kild, message('orchestrator', ['human'], 'final report'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([]); // the lead is the sender, so it isn't re-woken
});

test('a non-lead @human message does not double-deliver when the lead is already addressed', () => {
  const { kild, delivered, delivery } = fixture();
  routeMessage(kild, message('coder', ['orchestrator', 'human'], '...'), delivery);
  expect(delivered.map((x) => x.agentId)).toEqual(['s-orchestrator']); // once, not twice
});

test('delivers to other addressed agents but never the sender or @human', () => {
  const { kild, delivered, delivery } = fixture();
  routeMessage(kild, message('coder', ['orchestrator', 'human', 'coder'], '...'), delivery);
  expect(delivered.map((d) => d.agentId)).toEqual(['s-orchestrator']);
});

test('no addressee in a SINGLE-agent kild → delivered to the sole agent (chats like 1:1)', () => {
  const { kild, delivered, delivery } = fixture(['solo']);
  routeMessage(kild, message('human', [], 'fix the bug'), delivery);
  expect(delivered).toEqual([
    { agentId: 's-solo', from: 'human', text: '[#demo] @human: fix the bug' },
  ]);
});

test('no addressee in a MULTI-agent kild → broadcast only, no turn', () => {
  const { kild, delivered, broadcast, delivery } = fixture();
  routeMessage(kild, message('orchestrator', [], 'thinking out loud'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([]);
});

test('formatDelivery frames the message with kild, sender, and text', () => {
  expect(formatDelivery('demo', 'orchestrator', 'do X')).toBe('[#demo] @orchestrator: do X');
});

test('a message addressed to an ATTACHED agent queues instead of prompting', () => {
  const { kild, delivered, queued, broadcast, delivery } = fixture(['orchestrator', '@claude']);
  routeMessage(kild, message('orchestrator', ['claude'], 'review is done'), delivery);
  expect(broadcast).toHaveLength(1);
  expect(delivered).toEqual([]); // nothing to push to — kild does not own that process
  // Structured and RAW: how it reads is decided at the drain, not in the router.
  expect(queued).toEqual([{ handle: 'claude', from: 'orchestrator', text: 'review is done' }]);
});

test('a mixed kild routes both ways from one message', () => {
  const { kild, delivered, queued, delivery } = fixture(['orchestrator', 'coder', '@claude']);
  routeMessage(kild, message('orchestrator', ['coder', 'claude'], 'status?'), delivery);
  expect(delivered).toEqual([
    { agentId: 's-coder', from: 'orchestrator', text: '[#demo] @orchestrator: status?' },
  ]);
  expect(queued).toEqual([{ handle: 'claude', from: 'orchestrator', text: 'status?' }]);
});

test('a message the attached agent is not addressed in never reaches its inbox', () => {
  const { kild, queued, delivery } = fixture(['orchestrator', 'coder', '@claude']);
  // No addressee in a multi-agent kild → broadcast only; the inbox is not a
  // firehose, or the wake cap would trip on traffic @claude was never asked to read.
  routeMessage(kild, message('coder', [], 'thinking out loud'), delivery);
  routeMessage(kild, message('coder', ['orchestrator'], 'for you only'), delivery);
  expect(queued).toEqual([]);
});
