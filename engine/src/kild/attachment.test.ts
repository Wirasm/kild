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

  test('only the newest fork resolves, however many came before it', async () => {
    const sessions = ['s1', 's2', 's3', 's4'];
    for (const session of sessions) await recordAttachment(session, 'kild-a', 'kild');
    // Their records are all still on disk — deleting them cannot be made race-free and is not
    // what makes this correct. The claim is: exactly one session resolves.
    for (const stale of sessions.slice(0, -1)) {
      expect(await findAttachment(stale)).toBeNull();
    }
    expect(await findAttachment('s4')).toMatchObject({ kildId: 'kild-a', handle: 'kild' });
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
    await fs.mkdir(path.join(home, 'attached', 'claims', 'kild-a'), { recursive: true });
    await fs.writeFile(path.join(home, 'attached', 'claims', 'kild-a', 'kild'), 's\n');
    await fs.writeFile(recordFile('s'), JSON.stringify({ kildId: 'kild-a', handle: 'kild' }));
    expect(await findAttachment('s')).toMatchObject({ attachedAt: 0 });
  });

  test('a record nobody claims is not an attachment', async () => {
    // Only the claim decides who holds a handle. A record with no claim behind it is a
    // leftover, not a licence to act as that handle.
    await fs.mkdir(path.join(home, 'attached'), { recursive: true });
    await fs.writeFile(recordFile('orphan'), JSON.stringify({ kildId: 'k', handle: 'kild' }));
    expect(await findAttachment('orphan')).toBeNull();
  });
});

describe('concurrent attaches', () => {
  /** How many of these sessions consider themselves attached. Exactly one may. */
  async function resolving(sessions: string[]): Promise<string[]> {
    const held = await Promise.all(
      sessions.map(async (s) => ((await findAttachment(s)) ? s : null)),
    );
    return held.filter((s): s is string => s !== null);
  }

  test('two sessions claiming one handle at once leave exactly one attached', async () => {
    // The scan-and-delete this replaced lost BOTH records here: each saw the other's freshly
    // written file as somebody else's and deleted it. An atomic claim cannot do that — there
    // is no intermediate state for a second writer to observe.
    await Promise.all([
      recordAttachment('race-a', 'kild-x', 'claude'),
      recordAttachment('race-b', 'kild-x', 'claude'),
    ]);
    expect(await resolving(['race-a', 'race-b'])).toHaveLength(1);
  });

  test('many sessions racing on one handle still leave exactly one', async () => {
    const sessions = ['a', 'b', 'c', 'd', 'e', 'f'].map((s) => `many-${s}`);
    await Promise.all(sessions.map((s) => recordAttachment(s, 'kild-y', 'kild')));
    expect(await resolving(sessions)).toHaveLength(1);
  });

  test('the survivor is the one the claim names, and it resolves correctly', async () => {
    const sessions = ['w-1', 'w-2', 'w-3'];
    await Promise.all(sessions.map((s) => recordAttachment(s, 'kild-w', 'kild')));
    const [winner] = await resolving(sessions);
    expect(winner).toBeDefined();
    const claim = await fs.readFile(
      path.join(home, 'attached', 'claims', 'kild-w', 'kild'),
      'utf8',
    );
    expect(claim.trim()).toBe(winner as string);
    expect(await findAttachment(winner as string)).toMatchObject({ kildId: 'kild-w' });
  });

  test('concurrent attaches to DIFFERENT handles all survive', async () => {
    const pairs: Array<[string, string]> = [
      ['h-1', 'alpha'],
      ['h-2', 'beta'],
      ['h-3', 'gamma'],
    ];
    await Promise.all(pairs.map(([s, h]) => recordAttachment(s, 'kild-z', h)));
    expect(await resolving(pairs.map(([s]) => s))).toHaveLength(3);
  });

  test('a superseded session reads as not attached, never as somebody else', async () => {
    await recordAttachment('first', 'kild-q', 'kild');
    await recordAttachment('second', 'kild-q', 'kild');
    expect(await findAttachment('first')).toBeNull();
    expect(await findAttachment('second')).toMatchObject({ kildId: 'kild-q', handle: 'kild' });
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

describe('path segments cannot escape the directory', () => {
  test.each([
    ['..', 'traversal token'],
    ['.', 'current directory'],
  ])('a handle of %s is refused (%s)', async (handle) => {
    // `path.join` collapses `..` against the segment before it, so this would target the
    // shared claims DIRECTORY rather than a file in it — turning it into a plain file and
    // breaking every later attach with ENOTDIR, permanently.
    expect(recordAttachment('sess', 'kild-a', handle)).rejects.toThrow('invalid handle');
  });

  test.each([
    ['..', 'traversal token'],
    ['.', 'current directory'],
  ])('a kild id of %s is refused (%s)', async (kildId) => {
    expect(recordAttachment('sess', kildId, 'kild')).rejects.toThrow('invalid kild id');
  });

  test('a rejected segment leaves the claims directory intact', async () => {
    await recordAttachment('good', 'kild-real', 'kild');
    await expect(recordAttachment('sess', 'kild-a', '..')).rejects.toThrow();
    // The decisive check: a normal attach still works afterwards.
    await recordAttachment('after', 'kild-other', 'kild');
    expect(await findAttachment('after')).toMatchObject({ kildId: 'kild-other' });
    expect(await findAttachment('good')).toMatchObject({ kildId: 'kild-real' });
  });

  test('dots are still allowed INSIDE a value', async () => {
    await recordAttachment('sess.1', 'kild.v1.2', 'agent.one');
    expect(await findAttachment('sess.1')).toMatchObject({ kildId: 'kild.v1.2' });
  });
});
