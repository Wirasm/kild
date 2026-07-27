import { expect, test } from 'bun:test';

import { DEFAULT_WAKE_CAP, Inbox, MAX_QUEUED_POSTS } from './inbox.ts';

function post(from: string, text = 'hi', ts = 1) {
  return { from, text, ts };
}

test('an empty inbox drains empty and reports idle', () => {
  const inbox = new Inbox();
  expect(inbox.drain()).toEqual({ posts: [], idle: true, capped: false });
});

test('enqueued posts drain in order and mark the agent working', () => {
  const inbox = new Inbox();
  inbox.enqueue(post('reviewer', 'one', 1));
  inbox.enqueue(post('implementer', 'two', 2));
  expect(inbox.drain()).toEqual({
    posts: [post('reviewer', 'one', 1), post('implementer', 'two', 2)],
    idle: false,
    capped: false,
  });
});

test('drain is destructive — the same post never wakes twice', () => {
  const inbox = new Inbox();
  inbox.enqueue(post('reviewer'));
  expect(inbox.drain().posts).toHaveLength(1);
  expect(inbox.pending).toBe(0);
  expect(inbox.drain()).toEqual({ posts: [], idle: true, capped: false });
});

test('the wake cap suppresses the drain after N consecutive wakes, keeping the mail', () => {
  const inbox = new Inbox();
  for (let wake = 1; wake <= DEFAULT_WAKE_CAP; wake += 1) {
    inbox.enqueue(post('loop'));
    expect(inbox.drain()).toMatchObject({ idle: false, capped: false });
    expect(inbox.consecutiveWakes).toBe(wake);
  }
  inbox.enqueue(post('loop'));
  // Reports empty (so the harness is allowed to stop) but does NOT eat the message.
  expect(inbox.drain()).toEqual({ posts: [], idle: true, capped: true });
  expect(inbox.pending).toBe(1);
});

test('a capped drain resets the counter, so the next drain delivers the held mail', () => {
  const inbox = new Inbox(1);
  inbox.enqueue(post('a', 'first'));
  expect(inbox.drain()).toMatchObject({ posts: [post('a', 'first')], capped: false });
  inbox.enqueue(post('b', 'second'));
  expect(inbox.drain()).toMatchObject({ posts: [], capped: true });
  expect(inbox.drain()).toMatchObject({ posts: [post('b', 'second')], capped: false });
});

test('an empty drain resets the wake counter', () => {
  const inbox = new Inbox(2);
  inbox.enqueue(post('a'));
  inbox.drain();
  expect(inbox.consecutiveWakes).toBe(1);
  expect(inbox.drain().idle).toBe(true);
  expect(inbox.consecutiveWakes).toBe(0);
});

test('two inboxes waking each other terminate — every cycle ends in a silent drain', () => {
  const a = new Inbox();
  const b = new Inbox();
  let wakes = 0;
  // Each drained post makes the other side post back. Without the cap this never ends.
  a.enqueue(post('b'));
  for (let turn = 0; turn < 50; turn += 1) {
    const drainedA = a.drain();
    if (drainedA.posts.length > 0) {
      wakes += 1;
      b.enqueue(post('a'));
    }
    const drainedB = b.drain();
    if (drainedB.posts.length > 0) {
      wakes += 1;
      a.enqueue(post('b'));
    }
    if (drainedA.idle && drainedB.idle) break;
  }
  expect(wakes).toBeLessThanOrEqual(DEFAULT_WAKE_CAP * 2);
});

test('the queue is bounded — an agent that walked away cannot grow it forever', () => {
  const inbox = new Inbox();
  for (let index = 0; index < MAX_QUEUED_POSTS + 10; index += 1) {
    inbox.enqueue(post('spammer', `msg-${index}`, index));
  }
  expect(inbox.pending).toBe(MAX_QUEUED_POSTS);
  // Oldest dropped first: the newest post survives, the very first does not.
  const { posts } = inbox.drain();
  expect(posts[0]?.text).toBe('msg-10');
  expect(posts.at(-1)?.text).toBe(`msg-${MAX_QUEUED_POSTS + 9}`);
});
