import { afterAll, beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { KildRegistry } from './kild-registry.ts';
import type { Kild, Message } from './kild-types.ts';

let tmp: string;
let prevHome: string | undefined;

beforeAll(() => {
  prevHome = process.env.KILD_HOME;
  tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-kilds-'));
  process.env.KILD_HOME = tmp; // KildRegistry reads kildHome() in its constructor
});

afterAll(() => {
  if (prevHome === undefined) delete process.env.KILD_HOME;
  else process.env.KILD_HOME = prevHome;
  fs.rmSync(tmp, { recursive: true, force: true });
});

function kild(id: string): Kild {
  return {
    id,
    name: 'demo',
    cwd: '/tmp',
    agents: [{ handle: 'agent', id: 's1', persona: 'agent' }],
    log: [],
  };
}

function msg(kildId: string, text: string): Message {
  return { id: `${kildId}-1`, kildId, from: 'human', to: ['agent'], text, ts: 1 };
}

test('appendMessage write-throughs the kild log; an empty kild leaves no file', () => {
  const reg = new KildRegistry();
  reg.create(kild('kild-a'));
  const file = path.join(tmp, 'kilds', 'kild-a.json');
  expect(fs.existsSync(file)).toBe(false); // no messages yet → no history clutter
  reg.appendMessage('kild-a', msg('kild-a', 'hello'));
  expect(fs.existsSync(file)).toBe(true);
  expect(JSON.parse(fs.readFileSync(file, 'utf8'))).toMatchObject({ id: 'kild-a' });
});

test('a fresh registry loads past kilds into the archive (read-only) with their log', () => {
  const reg = new KildRegistry(); // re-reads $KILD_HOME/kilds at construction
  const found = reg.archived().find((a) => a.id === 'kild-a');
  expect(found).toBeDefined();
  expect(found?.log.map((m) => m.text)).toEqual(['hello']);
  // `ownership` is resolved, not passed through: the archive on disk predates the field, and
  // the view states it outright rather than leaving a client to infer it from absence.
  expect(found?.agents).toEqual([{ handle: 'agent', ownership: 'owned', persona: 'agent' }]);
  // Archived kilds are history only — they are NOT live in-memory kilds.
  expect(reg.get('kild-a')).toBeUndefined();
});

// ── A history file is untrusted input ────────────────────────────────────────────────
// It is written by an older engine, hand-edited, or truncated by a crash mid-write. The
// loader used to assert its shape with a bare cast and swallow every failure, so a dropped
// archive and "this kild never existed" were the same answer to every reader.

test('a malformed history file is skipped and REPORTED, and its siblings still load', () => {
  const dir = path.join(tmp, 'kilds');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'truncated.json'), '{"id":"truncated","log":[{"text":');
  fs.writeFileSync(path.join(dir, 'not-an-object.json'), '"just a string"');
  fs.writeFileSync(path.join(dir, 'no-id.json'), '{"name":"nameless","log":[]}');
  fs.writeFileSync(path.join(dir, 'log-not-array.json'), '{"id":"bad-log","log":"nope"}');

  const errors: string[] = [];
  const original = console.error;
  console.error = (...args: unknown[]) => errors.push(args.join(' '));
  let ids: string[];
  try {
    ids = new KildRegistry().archived().map((a) => a.id);
  } finally {
    console.error = original;
  }

  expect(ids).not.toContain('truncated');
  expect(ids).not.toContain('bad-log');
  expect(ids).toContain('kild-a'); // one bad file does not cost you the rest of your history
  // Four bad files, four complaints — silence is what made this indistinguishable from
  // having no history at all.
  expect(errors).toHaveLength(4);
  expect(errors.every((line) => line.includes('unreadable history file'))).toBe(true);
});

test('a message that is not a message is dropped, and the rest of the log survives', () => {
  const dir = path.join(tmp, 'kilds');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(
    path.join(dir, 'partial.json'),
    JSON.stringify({
      id: 'partial',
      name: 'demo',
      agents: [],
      log: [{ ...msg('partial', 'real'), seq: 1 }, null, 42, { from: 'human' }],
    }),
  );
  const found = new KildRegistry().archived().find((a) => a.id === 'partial');
  expect(found?.log.map((m) => m.text)).toEqual(['real']);
});

// ── The archive has to be sortable ────────────────────────────────────────────────────
// Dropping `log` from the listing dropped time with it — the last message's `ts` had been
// carrying that job by accident. Without a timestamp, ordering history newest-first costs one
// `/messages` request per archived kild: N requests to sort a list, which is more expensive
// than the payload the projection removed.

test('a kild archived now carries the moment it ended', () => {
  const reg = new KildRegistry();
  reg.create(kild('kild-ended'));
  reg.appendMessage('kild-ended', msg('kild-ended', 'work'));
  const before = Date.now();
  const snap = reg.remove('kild-ended');
  expect(snap?.endedAt).toBeGreaterThanOrEqual(before);
  // …and it survives the round trip to disk, which is where a client reads it from.
  const reloaded = new KildRegistry().archived().find((a) => a.id === 'kild-ended');
  expect(reloaded?.endedAt).toBe(snap?.endedAt as number);
});

test('a kild the engine died under still has a time — it is stamped on every save', () => {
  // The shape that would otherwise need a decode-time fallback: write-through persisted it,
  // `remove()` never ran. Stamping at save instead of at stop means there is no hole to
  // infer a value for, so nothing has to guess one from the last message.
  const reg = new KildRegistry();
  reg.create(kild('kild-crashed'));
  const before = Date.now();
  reg.appendMessage('kild-crashed', msg('kild-crashed', 'mid-flight')); // write-through only
  const onDisk = JSON.parse(
    fs.readFileSync(path.join(tmp, 'kilds', 'kild-crashed.json'), 'utf8'),
  ) as { endedAt?: number };
  expect(onDisk.endedAt).toBeGreaterThanOrEqual(before);
});

test('an archive written before `endedAt` has none — no fallback invents one', () => {
  // We own both sides of this wire, so an old file is a fact to state, not a shape to shim.
  // Inferring an end time from the last message would be a guess dressed as a fact; a client
  // sorts the unknowns last.
  const dir = path.join(tmp, 'kilds');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(
    path.join(dir, 'no-end.json'),
    JSON.stringify({
      id: 'no-end',
      name: 'demo',
      agents: [],
      log: [{ ...msg('no-end', 'last'), ts: 2000, seq: 1 }],
    }),
  );
  const found = new KildRegistry().archived().find((a) => a.id === 'no-end');
  expect(found).toBeDefined();
  expect(found?.endedAt).toBeUndefined();
});

test('remove() moves a kild with history straight into the archive and returns the snapshot', () => {
  const reg = new KildRegistry();
  reg.create(kild('kild-b'));
  reg.appendMessage('kild-b', msg('kild-b', 'hi'));
  const snap = reg.remove('kild-b');
  expect(snap?.id).toBe('kild-b');
  // Leaving the live map IS being stopped — there is no state to read back.
  expect(reg.get('kild-b')).toBeUndefined();
  expect(reg.archived().some((a) => a.id === 'kild-b')).toBe(true); // archived now, no restart
});

test('an archived kild survives reload with its log', () => {
  const reg = new KildRegistry();
  reg.create(kild('kild-d'));
  reg.appendMessage('kild-d', msg('kild-d', 'said first'));
  expect(reg.remove('kild-d')?.id).toBe('kild-d');

  const reloaded = new KildRegistry();
  const found = reloaded.archived().find((kild) => kild.id === 'kild-d');
  expect(found).toBeDefined();
  expect(found?.log.map((message) => message.text)).toEqual(['said first']);
  expect(reloaded.get('kild-d')).toBeUndefined(); // reloaded history is never live
});

test('remove() of an empty kild archives nothing', () => {
  const reg = new KildRegistry();
  reg.create(kild('kild-c'));
  expect(reg.remove('kild-c')).toBeUndefined();
  expect(reg.archived().some((a) => a.id === 'kild-c')).toBe(false);
});

test('appendMessage persists atomically: rename failure keeps the previous file and surfaces the error', () => {
  const reg = new KildRegistry();
  reg.create(kild('kild-e'));
  reg.appendMessage('kild-e', msg('kild-e', 'first'));

  const file = path.join(tmp, 'kilds', 'kild-e.json');
  const original = fs.readFileSync(file, 'utf8');
  const renameSync = fs.renameSync;
  fs.renameSync = ((from: fs.PathLike, to: fs.PathLike) => {
    if (String(to) === file) throw new Error('rename blocked');
    return renameSync(from, to);
  }) as typeof fs.renameSync;

  try {
    expect(() => reg.appendMessage('kild-e', msg('kild-e', 'second'))).toThrow(
      'kild: failed to persist kild kild-e: rename blocked',
    );
  } finally {
    fs.renameSync = renameSync;
  }

  expect(fs.readFileSync(file, 'utf8')).toBe(original);
  expect(
    fs
      .readdirSync(path.join(tmp, 'kilds'))
      .filter((name) => name.startsWith('kild-e.json.') && name.endsWith('.tmp')),
  ).toEqual([]);
});

test('remove() surfaces archive persistence failures', () => {
  const reg = new KildRegistry();
  reg.create(kild('kild-f'));
  reg.appendMessage('kild-f', msg('kild-f', 'hi'));

  const renameSync = fs.renameSync;
  fs.renameSync = (() => {
    throw new Error('rename blocked');
  }) as typeof fs.renameSync;

  try {
    expect(() => reg.remove('kild-f')).toThrow(
      'kild: failed to persist kild kild-f: rename blocked',
    );
  } finally {
    fs.renameSync = renameSync;
  }
});
