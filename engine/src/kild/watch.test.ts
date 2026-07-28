import { describe, expect, test } from 'bun:test';

import type { Message } from './kild-types.ts';
import { pollResult, WATCH_EXIT, watchSummary } from './watch.ts';

const message = (seq: number, from: string, to: string[] = ['kild']): Message => ({
  id: `m-${seq}`,
  kildId: 'k',
  from,
  to,
  text: 'x',
  ts: seq,
  seq,
});

describe('pollResult', () => {
  test('somebody else speaking is a reason to wake', () => {
    const { incoming } = pollResult([message(1, 'claude')], 'kild', 0);
    expect(incoming).toHaveLength(1);
  });

  test("the watcher's OWN messages never wake it", () => {
    // Otherwise every send would immediately re-trigger the watcher that sent it.
    const { incoming } = pollResult([message(1, 'kild'), message(2, 'kild')], 'kild', 0);
    expect(incoming).toEqual([]);
  });

  test('own messages still advance the cursor', () => {
    // Or they would be re-examined on every poll, forever.
    const { cursor } = pollResult([message(7, 'kild')], 'kild', 3);
    expect(cursor).toBe(7);
  });

  test('a message addressed to someone else still wakes', () => {
    // The watcher wants to know the conversation moved. Filtering on `to` would make it deaf
    // to everything except its own mail, which is what `inbox` is for.
    const { incoming } = pollResult([message(1, 'claude', ['agent'])], 'kild', 0);
    expect(incoming).toHaveLength(1);
  });

  test('a mixed batch reports only the others, and advances past all of it', () => {
    const batch = [message(4, 'kild'), message(5, 'claude'), message(6, 'kild')];
    const { incoming, cursor } = pollResult(batch, 'kild', 3);
    expect(incoming.map((m) => m.seq)).toEqual([5]);
    expect(cursor).toBe(6);
  });

  test('an empty batch leaves the cursor exactly where it was', () => {
    expect(pollResult([], 'kild', 9)).toEqual({ incoming: [], cursor: 9 });
  });

  test('a message at or behind the cursor is neither fresh mail nor a cursor rewind', () => {
    // `since` is documented exclusive, so this should never arrive — but if it ever does,
    // reporting it as new wakes a watcher on already-seen content, repeatedly. Asserting only
    // the cursor here left that half completely unconstrained.
    const { incoming, cursor } = pollResult([message(2, 'claude')], 'kild', 10);
    expect(cursor).toBe(10);
    expect(incoming).toEqual([]);
  });

  test('a batch straddling the cursor reports only what is genuinely new', () => {
    const batch = [message(9, 'claude'), message(10, 'claude'), message(11, 'claude')];
    const { incoming } = pollResult(batch, 'kild', 10);
    expect(incoming.map((m) => m.seq)).toEqual([11]);
  });
});

describe('watchSummary', () => {
  test('names the sender and counts', () => {
    expect(watchSummary([message(1, 'claude')])).toBe('1 new message from @claude');
  });

  test('pluralises, dedupes senders, and keeps first-appearance order', () => {
    const batch = [message(1, 'claude'), message(2, 'agent'), message(3, 'claude')];
    expect(watchSummary(batch)).toBe('3 new messages from @claude, @agent');
  });
});

describe('exit codes', () => {
  test('a quiet engine and a dead engine are different codes', () => {
    // The whole interface for whatever runs this in the background. If these collided, a
    // harness could not tell "nothing happened" from "I no longer know", which is the exact
    // silent-failure shape this command exists to remove — and it is not retrofittable.
    expect(WATCH_EXIT.quiet).not.toBe(WATCH_EXIT.unreachable);
  });

  test('only mail is success', () => {
    expect(WATCH_EXIT.mail).toBe(0);
    for (const code of [WATCH_EXIT.usage, WATCH_EXIT.quiet, WATCH_EXIT.unreachable]) {
      expect(code).toBeGreaterThan(0);
    }
  });

  test('every outcome has a distinct code', () => {
    const codes = Object.values(WATCH_EXIT);
    expect(new Set(codes).size).toBe(codes.length);
  });
});
