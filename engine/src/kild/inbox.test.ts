import { expect, test } from 'bun:test';

import { DEFAULT_WAKE_CAP, Inbox, MAX_QUEUED_MESSAGES } from './inbox.ts';

function message(from: string, text = 'hi', ts = 1) {
  return { from, text, ts };
}

test('an empty inbox drains empty and reports idle', () => {
  const inbox = new Inbox();
  expect(inbox.drain()).toEqual({ messages: [], idle: true, capped: false });
});

test('enqueued messages drain in order and mark the agent working', () => {
  const inbox = new Inbox();
  inbox.enqueue(message('reviewer', 'one', 1));
  inbox.enqueue(message('implementer', 'two', 2));
  expect(inbox.drain()).toEqual({
    messages: [message('reviewer', 'one', 1), message('implementer', 'two', 2)],
    idle: false,
    capped: false,
  });
});

test('drain is destructive — the same message never wakes twice', () => {
  const inbox = new Inbox();
  inbox.enqueue(message('reviewer'));
  expect(inbox.drain().messages).toHaveLength(1);
  expect(inbox.pending).toBe(0);
  expect(inbox.drain()).toEqual({ messages: [], idle: true, capped: false });
});

test('the wake cap suppresses the drain after N consecutive wakes, keeping the mail', () => {
  const inbox = new Inbox();
  for (let wake = 1; wake <= DEFAULT_WAKE_CAP; wake += 1) {
    inbox.enqueue(message('loop'));
    expect(inbox.drain()).toMatchObject({ idle: false, capped: false });
    expect(inbox.consecutiveWakes).toBe(wake);
  }
  inbox.enqueue(message('loop'));
  // Reports empty (so the harness is allowed to stop) but does NOT eat the message.
  expect(inbox.drain()).toEqual({ messages: [], idle: true, capped: true });
  expect(inbox.pending).toBe(1);
});

test('a capped drain resets the counter, so the next drain delivers the held mail', () => {
  const inbox = new Inbox(1);
  inbox.enqueue(message('a', 'first'));
  expect(inbox.drain()).toMatchObject({ messages: [message('a', 'first')], capped: false });
  inbox.enqueue(message('b', 'second'));
  expect(inbox.drain()).toMatchObject({ messages: [], capped: true });
  expect(inbox.drain()).toMatchObject({ messages: [message('b', 'second')], capped: false });
});

test('an empty drain resets the wake counter', () => {
  const inbox = new Inbox(2);
  inbox.enqueue(message('a'));
  inbox.drain();
  expect(inbox.consecutiveWakes).toBe(1);
  expect(inbox.drain().idle).toBe(true);
  expect(inbox.consecutiveWakes).toBe(0);
});

test('two inboxes waking each other terminate — every cycle ends in a silent drain', () => {
  const a = new Inbox();
  const b = new Inbox();
  let wakes = 0;
  // Each drained message makes the other side message back. Without the cap this never ends.
  a.enqueue(message('b'));
  for (let turn = 0; turn < 50; turn += 1) {
    const drainedA = a.drain();
    if (drainedA.messages.length > 0) {
      wakes += 1;
      b.enqueue(message('a'));
    }
    const drainedB = b.drain();
    if (drainedB.messages.length > 0) {
      wakes += 1;
      a.enqueue(message('b'));
    }
    if (drainedA.idle && drainedB.idle) break;
  }
  expect(wakes).toBeLessThanOrEqual(DEFAULT_WAKE_CAP * 2);
});

test('the queue is bounded — an agent that walked away cannot grow it forever', () => {
  const inbox = new Inbox();
  for (let index = 0; index < MAX_QUEUED_MESSAGES + 10; index += 1) {
    inbox.enqueue(message('spammer', `msg-${index}`, index));
  }
  expect(inbox.pending).toBe(MAX_QUEUED_MESSAGES);
  // Oldest dropped first: the newest message survives, the very first does not.
  const { messages } = inbox.drain();
  expect(messages[0]?.text).toBe('msg-10');
  expect(messages.at(-1)?.text).toBe(`msg-${MAX_QUEUED_MESSAGES + 9}`);
});
