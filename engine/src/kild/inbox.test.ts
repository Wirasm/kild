import { expect, test } from 'bun:test';

import { DEFAULT_WAKE_CAP, MAX_QUEUED_POSTS, Mailbox } from './attached.ts';

function post(from: string, text = 'hi', ts = 1) {
  return { from, text, ts };
}

test('an empty mailbox drains empty and reports idle', () => {
  const mailbox = new Mailbox();
  expect(mailbox.drain()).toEqual({ posts: [], idle: true, capped: false });
});

test('enqueued posts drain in order and mark the participant working', () => {
  const mailbox = new Mailbox();
  mailbox.enqueue(post('reviewer', 'one', 1));
  mailbox.enqueue(post('implementer', 'two', 2));
  expect(mailbox.drain()).toEqual({
    posts: [post('reviewer', 'one', 1), post('implementer', 'two', 2)],
    idle: false,
    capped: false,
  });
});

test('drain is destructive — the same post never wakes twice', () => {
  const mailbox = new Mailbox();
  mailbox.enqueue(post('reviewer'));
  expect(mailbox.drain().posts).toHaveLength(1);
  expect(mailbox.pending).toBe(0);
  expect(mailbox.drain()).toEqual({ posts: [], idle: true, capped: false });
});

test('the wake cap suppresses the drain after N consecutive wakes, keeping the mail', () => {
  const mailbox = new Mailbox();
  for (let wake = 1; wake <= DEFAULT_WAKE_CAP; wake += 1) {
    mailbox.enqueue(post('loop'));
    expect(mailbox.drain()).toMatchObject({ idle: false, capped: false });
    expect(mailbox.consecutiveWakes).toBe(wake);
  }
  mailbox.enqueue(post('loop'));
  // Reports empty (so the harness is allowed to stop) but does NOT eat the message.
  expect(mailbox.drain()).toEqual({ posts: [], idle: true, capped: true });
  expect(mailbox.pending).toBe(1);
});

test('a capped drain resets the counter, so the next drain delivers the held mail', () => {
  const mailbox = new Mailbox(1);
  mailbox.enqueue(post('a', 'first'));
  expect(mailbox.drain()).toMatchObject({ posts: [post('a', 'first')], capped: false });
  mailbox.enqueue(post('b', 'second'));
  expect(mailbox.drain()).toMatchObject({ posts: [], capped: true });
  expect(mailbox.drain()).toMatchObject({ posts: [post('b', 'second')], capped: false });
});

test('an empty drain resets the wake counter', () => {
  const mailbox = new Mailbox(2);
  mailbox.enqueue(post('a'));
  mailbox.drain();
  expect(mailbox.consecutiveWakes).toBe(1);
  expect(mailbox.drain().idle).toBe(true);
  expect(mailbox.consecutiveWakes).toBe(0);
});

test('two mailboxes waking each other terminate — every cycle ends in a silent drain', () => {
  const a = new Mailbox();
  const b = new Mailbox();
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

test('the queue is bounded — a participant that walked away cannot grow it forever', () => {
  const mailbox = new Mailbox();
  for (let index = 0; index < MAX_QUEUED_POSTS + 10; index += 1) {
    mailbox.enqueue(post('spammer', `msg-${index}`, index));
  }
  expect(mailbox.pending).toBe(MAX_QUEUED_POSTS);
  // Oldest dropped first: the newest post survives, the very first does not.
  const { posts } = mailbox.drain();
  expect(posts[0]?.text).toBe('msg-10');
  expect(posts.at(-1)?.text).toBe(`msg-${MAX_QUEUED_POSTS + 9}`);
});
