import { beforeEach, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

/**
 * `loadProjects` must never hand back a non-array.
 *
 * It used to return `JSON.parse(raw).projects` unchecked, so any `projects.json` without
 * that exact key yielded `undefined` — and every caller does `.map`/`.find` on the result.
 * One hand-edited registry therefore 500'd routes across the whole engine, with the server
 * blaming itself for the operator's typo. Found by the end-to-end migration test, whose rig
 * wrote a bare array.
 *
 * A registry we cannot read means "no projects registered", not "the request failed".
 */
let home: string;

beforeEach(() => {
  home = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-projects-'));
  process.env.KILD_HOME = home;
});

const write = (contents: string) => fs.writeFileSync(path.join(home, 'projects.json'), contents);
const load = async () => (await import('./projects.ts')).loadProjects();

test('the canonical {projects: [...]} shape loads', async () => {
  write(JSON.stringify({ projects: [{ name: 'a', path: '/tmp/a' }] }));
  expect(await load()).toEqual([{ name: 'a', path: '/tmp/a' }]);
});

test('a bare array is tolerated — it is what a hand-edited file usually becomes', async () => {
  write(JSON.stringify([{ name: 'a', path: '/tmp/a' }]));
  expect(await load()).toEqual([{ name: 'a', path: '/tmp/a' }]);
});

test('a missing registry is no projects, not an error', async () => {
  expect(await load()).toEqual([]);
});

test('malformed JSON yields no projects rather than throwing', async () => {
  write('{ this is not json');
  expect(await load()).toEqual([]);
});

test('an object without a projects key yields an ARRAY, never undefined', async () => {
  // The original bug in one assertion: this returned `undefined`, and the caller's `.map`
  // threw somewhere far away, surfacing as a 500 on an unrelated route.
  write(JSON.stringify({ somethingElse: true }));
  const loaded = await load();
  expect(Array.isArray(loaded)).toBe(true);
  expect(loaded).toEqual([]);
});

test('a projects key holding a non-array yields an array', async () => {
  write(JSON.stringify({ projects: 'oops' }));
  expect(await load()).toEqual([]);
});

test('entries missing name or path are dropped, the rest survive', async () => {
  write(
    JSON.stringify({
      projects: [
        { name: 'good', path: '/tmp/good' },
        { name: 'no-path' },
        { path: '/tmp/no-name' },
        'not-an-object',
        null,
      ],
    }),
  );
  expect(await load()).toEqual([{ name: 'good', path: '/tmp/good' }]);
});
