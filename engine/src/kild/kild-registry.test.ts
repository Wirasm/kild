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

function kild(id: string, state: Kild['state'] = 'running'): Kild {
  return {
    id,
    name: 'demo',
    cwd: '/tmp',
    agents: [{ handle: 'agent', id: 's1', persona: 'agent' }],
    log: [],
    state,
  };
}

function msg(kildId: string, text: string): Message {
  return { id: `${kildId}-1`, kildId, from: 'human', to: ['agent'], text, ts: 1 };
}

test('appendMessage write-throughs the kild log with lifecycle state; an empty kild leaves no file', () => {
  const reg = new KildRegistry();
  reg.create(kild('kild-a'));
  const file = path.join(tmp, 'kilds', 'kild-a.json');
  expect(fs.existsSync(file)).toBe(false); // no messages yet → no history clutter
  reg.appendMessage('kild-a', msg('kild-a', 'hello'));
  expect(fs.existsSync(file)).toBe(true);
  expect(JSON.parse(fs.readFileSync(file, 'utf8'))).toMatchObject({
    id: 'kild-a',
    state: 'running',
  });
});

test('a fresh registry loads past kilds into the archive (read-only) with their log', () => {
  const reg = new KildRegistry(); // re-reads $KILD_HOME/kilds at construction
  const found = reg.archived().find((a) => a.id === 'kild-a');
  expect(found).toBeDefined();
  expect(found?.log.map((m) => m.text)).toEqual(['hello']);
  expect(found?.agents).toEqual([{ handle: 'agent', persona: 'agent' }]);
  expect(found?.state).toBe('running');
  // Archived kilds are history only — they are NOT live in-memory kilds.
  expect(reg.get('kild-a')).toBeUndefined();
});

test('remove() archives a kild with history immediately as closed and returns the snapshot', () => {
  const reg = new KildRegistry();
  reg.create(kild('kild-b', 'closed'));
  reg.appendMessage('kild-b', msg('kild-b', 'hi'));
  const snap = reg.remove('kild-b');
  expect(snap?.id).toBe('kild-b');
  expect(snap?.state).toBe('closed');
  expect(reg.get('kild-b')).toBeUndefined(); // no longer live
  expect(reg.archived().find((a) => a.id === 'kild-b')?.state).toBe('closed'); // archived now, no restart
});

test('halted to closed archive state persists across reload', () => {
  const reg = new KildRegistry();
  reg.create(kild('kild-d', 'halted'));
  reg.appendMessage('kild-d', msg('kild-d', 'halted first'));
  const live = reg.get('kild-d');
  expect(live).toBeDefined();
  if (!live) throw new Error('expected kild-d to be live before close');
  live.state = 'closed';
  expect(reg.remove('kild-d')?.state).toBe('closed');

  const reloaded = new KildRegistry();
  const found = reloaded.archived().find((kild) => kild.id === 'kild-d');
  expect(found).toBeDefined();
  expect(found?.state).toBe('closed');
  expect(found?.log.map((message) => message.text)).toEqual(['halted first']);
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
  reg.create(kild('kild-f', 'closed'));
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
