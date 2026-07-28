import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

import { findAttachment, recordAttachment } from './attachment.ts';

let home: string;
let previous: string | undefined;

beforeEach(async () => {
  previous = process.env.KILD_HOME;
  // Resolved with realpath: on macOS the temp dir is a symlink (/var → /private/var), and
  // half the engine's paths come from git already resolved. One spelling, everywhere.
  home = await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(), 'kild-attach-')));
  process.env.KILD_HOME = home;
});

afterEach(async () => {
  if (previous === undefined) delete process.env.KILD_HOME;
  else process.env.KILD_HOME = previous;
  await fs.rm(home, { recursive: true, force: true });
});

const recordFile = (session: string) => path.join(home, 'attached', `${session}.json`);

describe('recordAttachment / findAttachment', () => {
  test('a session finds the kild and handle it attached as', async () => {
    await recordAttachment('sess-1', 'kild-a', 'kild');
    expect(await findAttachment('sess-1')).toMatchObject({ kildId: 'kild-a', handle: 'kild' });
  });

  test('a session that never attached has no attachment', async () => {
    expect(await findAttachment('never-attached')).toBeNull();
  });

  test('re-attaching the same session re-points it', async () => {
    await recordAttachment('sess-1', 'kild-a', 'kild');
    await recordAttachment('sess-1', 'kild-b', 'helm');
    expect(await findAttachment('sess-1')).toMatchObject({ kildId: 'kild-b', handle: 'helm' });
  });

  test('attachedAt is recorded', async () => {
    const before = Date.now();
    await recordAttachment('sess-1', 'kild-a', 'kild');
    const record = await findAttachment('sess-1');
    expect(record?.attachedAt).toBeGreaterThanOrEqual(before);
  });
});

describe('supersession — the fork case', () => {
  test('a new session claiming the same handle supersedes the old record', async () => {
    await recordAttachment('old-session', 'kild-a', 'kild');
    await recordAttachment('new-session', 'kild-a', 'kild');

    // The fork inherits the handle; the session it forked from must not keep a claim on it,
    // or two sessions would race on one destructive inbox.
    expect(await findAttachment('new-session')).toMatchObject({ kildId: 'kild-a' });
    expect(await findAttachment('old-session')).toBeNull();
  });

  test('a different handle in the same kild is left alone', async () => {
    await recordAttachment('sess-other', 'kild-a', 'helm');
    await recordAttachment('sess-mine', 'kild-a', 'kild');
    expect(await findAttachment('sess-other')).toMatchObject({ handle: 'helm' });
  });

  test('the same handle in a different kild is left alone', async () => {
    await recordAttachment('sess-other', 'kild-b', 'kild');
    await recordAttachment('sess-mine', 'kild-a', 'kild');
    expect(await findAttachment('sess-other')).toMatchObject({ kildId: 'kild-b' });
  });

  test('forks do not accumulate records', async () => {
    for (const session of ['s1', 's2', 's3', 's4']) {
      await recordAttachment(session, 'kild-a', 'kild');
    }
    expect(await fs.readdir(path.join(home, 'attached'))).toEqual(['s4.json']);
  });
});

describe('unreadable is not the same fact as absent', () => {
  // A send that swallowed "cannot read" would post with no credential — silently
  // unattributed, which is the bug this module exists to prevent. Only a missing file means
  // "not attached"; everything else throws and each caller decides. The turn-end hook
  // catches at its own call site (see cli.attached.test.ts).
  test('malformed JSON throws rather than reading as not attached', async () => {
    await fs.mkdir(path.join(home, 'attached'), { recursive: true });
    await fs.writeFile(recordFile('sess-1'), '{not json');
    expect(findAttachment('sess-1')).rejects.toThrow('unreadable attachment record');
  });

  test('a record missing its fields throws', async () => {
    await fs.mkdir(path.join(home, 'attached'), { recursive: true });
    await fs.writeFile(recordFile('sess-1'), JSON.stringify({ kildId: 'kild-a' }));
    expect(findAttachment('sess-1')).rejects.toThrow('malformed attachment record');
  });

  test('a record with non-string fields throws', async () => {
    await fs.mkdir(path.join(home, 'attached'), { recursive: true });
    await fs.writeFile(recordFile('sess-1'), JSON.stringify({ kildId: 1, handle: [] }));
    expect(findAttachment('sess-1')).rejects.toThrow('malformed attachment record');
  });

  test('an absent record is still simply null', async () => {
    expect(await findAttachment('never-attached')).toBeNull();
  });

  test('a record without attachedAt still resolves', async () => {
    await fs.mkdir(path.join(home, 'attached'), { recursive: true });
    await fs.writeFile(recordFile('s'), JSON.stringify({ kildId: 'kild-a', handle: 'kild' }));
    expect(await findAttachment('s')).toMatchObject({ attachedAt: 0 });
  });
});

describe('concurrent attaches', () => {
  test('two sessions claiming one handle at once leave exactly one attached', async () => {
    // Without mutual exclusion each supersede scan sees the other's freshly written record as
    // somebody else's and deletes it, leaving NEITHER attached — data loss in precisely the
    // fork case this record exists to serve.
    await Promise.all([
      recordAttachment('race-a', 'kild-x', 'claude'),
      recordAttachment('race-b', 'kild-x', 'claude'),
    ]);
    const survivors = (await fs.readdir(path.join(home, 'attached'))).filter((f) =>
      f.endsWith('.json'),
    );
    expect(survivors).toHaveLength(1);
  });

  test('many sessions racing on one handle still leave exactly one', async () => {
    await Promise.all(
      ['a', 'b', 'c', 'd', 'e', 'f'].map((s) => recordAttachment(`many-${s}`, 'kild-y', 'kild')),
    );
    const survivors = (await fs.readdir(path.join(home, 'attached'))).filter((f) =>
      f.endsWith('.json'),
    );
    expect(survivors).toHaveLength(1);
  });

  test('concurrent attaches to DIFFERENT handles all survive', async () => {
    await Promise.all([
      recordAttachment('h-1', 'kild-z', 'alpha'),
      recordAttachment('h-2', 'kild-z', 'beta'),
      recordAttachment('h-3', 'kild-z', 'gamma'),
    ]);
    const survivors = (await fs.readdir(path.join(home, 'attached'))).filter((f) =>
      f.endsWith('.json'),
    );
    expect(survivors).toHaveLength(3);
  });
});

describe('session ids reach the filesystem', () => {
  test.each([
    ['../escape', 'traversal'],
    ['a/b', 'separator'],
    ['', 'empty'],
    ['sess$(whoami)', 'shell metacharacters'],
  ])('%s is refused (%s)', async (session) => {
    expect(recordAttachment(session, 'kild-a', 'kild')).rejects.toThrow('invalid session id');
    expect(findAttachment(session)).rejects.toThrow('invalid session id');
  });

  test('a traversing id cannot write outside the directory', async () => {
    await expect(recordAttachment('../../escaped', 'kild-a', 'kild')).rejects.toThrow();
    await expect(fs.readFile(path.join(home, '..', '..', 'escaped.json'))).rejects.toThrow();
  });
});
