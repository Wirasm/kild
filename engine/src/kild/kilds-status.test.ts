import { expect, test } from 'bun:test';

import { compactLiveKilds, formatCompactGitSummary } from './kilds-status.ts';

test('formatCompactGitSummary returns empty for absent status', () => {
  expect(formatCompactGitSummary()).toEqual('');
});

test('formatCompactGitSummary preserves clean known-branch divergence', () => {
  expect(
    formatCompactGitSummary({
      path: '/tmp/ws',
      branch: 'feature-x',
      base: 'main',
      ahead: 2,
      behind: 1,
      dirty: false,
      uncommittedFiles: 0,
      changedFileCount: 0,
      conflictsWithBase: null,
    }),
  ).toEqual(' · feature-x +2/-1');
});

test('formatCompactGitSummary renders a null branch as unknown', () => {
  expect(
    formatCompactGitSummary({
      path: '/tmp/ws',
      branch: null,
      base: 'main',
      ahead: 0,
      behind: 0,
      dirty: false,
      uncommittedFiles: 0,
      changedFileCount: 0,
      conflictsWithBase: null,
    }),
  ).toEqual(' · ? +0/-0');
});

test('formatCompactGitSummary appends dirty and conflict markers', () => {
  expect(
    formatCompactGitSummary({
      path: '/tmp/ws',
      branch: 'feature-x',
      base: 'main',
      ahead: 2,
      behind: 0,
      dirty: true,
      uncommittedFiles: 1,
      changedFileCount: 1,
      conflictsWithBase: true,
    }),
  ).toEqual(' · feature-x +2/-0 dirty CONFLICTS');
});

test('a live kild with no log compacts to an empty post list', () => {
  expect(
    compactLiveKilds([
      {
        id: 'kild-1',
        name: 'ops',
        agents: [{ handle: 'brain', persona: 'brain' }],
        log: [],
      },
    ]),
  ).toEqual([
    {
      id: 'kild-1',
      name: 'ops',
      agents: [{ handle: 'brain', persona: 'brain' }],
      posts: [],
    },
  ]);
});

test('a live kild keeps only the last two posts and preserves order', () => {
  const kilds = [
    {
      id: 'kild-1',
      name: 'ops',
      agents: [{ handle: 'brain', persona: 'brain' }],
      log: [
        { id: 'm1', kildId: 'kild-1', from: 'human', to: ['brain'], text: 'one', ts: 1 },
        { id: 'm2', kildId: 'kild-1', from: 'brain', to: ['human'], text: 'two', ts: 2 },
        { id: 'm3', kildId: 'kild-1', from: 'human', to: ['brain'], text: 'three', ts: 3 },
      ],
    },
  ];

  const compact = compactLiveKilds(kilds);
  expect(compact[0]?.posts.map((message) => message.id)).toEqual(['m2', 'm3']);
});

test('git compacts to a summary: changed-file COUNT, not the list (pull discipline)', () => {
  const git = {
    path: '/tmp/ws',
    branch: 'feature-x',
    base: 'main',
    ahead: 2,
    behind: 0,
    dirty: true,
    uncommittedFiles: 1,
    changedFiles: ['src/a.ts', 'src/b.ts'],
    conflictsWithBase: null,
  };
  const compact = compactLiveKilds([
    {
      id: 'kild-1',
      name: 'ops',
      agents: [{ handle: 'brain', persona: 'brain' }],
      log: [],
      git,
    },
  ]);
  expect(compact[0]?.git).toEqual({
    path: '/tmp/ws',
    branch: 'feature-x',
    base: 'main',
    ahead: 2,
    behind: 0,
    dirty: true,
    uncommittedFiles: 1,
    changedFileCount: 2,
    conflictsWithBase: null,
  });
  // The full list is NOT in the director's compact view.
  expect(compact[0]?.git).not.toHaveProperty('changedFiles');
});

test('a live kild without git status has no git key', () => {
  const compact = compactLiveKilds([
    { id: 'kild-1', name: 'ops', agents: [{ handle: 'brain', persona: 'brain' }], log: [] },
  ]);
  expect(compact[0]).not.toHaveProperty('git');
});

test('collisions: two kilds that touch the same file each name the other', () => {
  const mk = (id: string, name: string, changedFiles: string[]) => ({
    id,
    name,
    agents: [{ handle: 'coder', persona: 'coder' }],
    log: [],
    git: {
      path: `/tmp/${name}`,
      branch: name,
      base: 'main',
      ahead: 1,
      behind: 0,
      dirty: false,
      uncommittedFiles: 0,
      changedFiles,
      conflictsWithBase: null,
    },
  });
  const compact = compactLiveKilds([
    mk('r1', 'auth', ['src/auth.ts', 'src/shared.ts']),
    mk('r2', 'billing', ['src/billing.ts', 'src/shared.ts']),
    mk('r3', 'docs', ['README.md']),
  ]);
  const byId = Object.fromEntries(compact.map((r) => [r.id, r]));
  expect(byId.r1?.collidesWith).toEqual([{ kild: 'billing', files: ['src/shared.ts'] }]);
  expect(byId.r2?.collidesWith).toEqual([{ kild: 'auth', files: ['src/shared.ts'] }]);
  expect(byId.r3).not.toHaveProperty('collidesWith');
});

test('per-agent attention + cost ride the compact view, with a kild totals rollup', () => {
  const compact = compactLiveKilds([
    {
      id: 'kild-1',
      name: 'ops',
      agents: [
        { handle: 'coder', idle: true, tokens: 3400, cost: 1.25 },
        { handle: 'reviewer', tokens: 600, cost: 0.25 },
      ],
      log: [],
    },
  ]);
  expect(compact[0]?.agents).toEqual([
    { handle: 'coder', idle: true, tokens: 3400, cost: 1.25 },
    { handle: 'reviewer', tokens: 600, cost: 0.25 },
  ]);
  expect(compact[0]?.totals).toEqual({ tokens: 4000, cost: 1.5 });
});

test('a kild whose agents have no stats gets no totals key', () => {
  const compact = compactLiveKilds([
    { id: 'kild-1', name: 'ops', agents: [{ handle: 'coder' }], log: [] },
  ]);
  expect(compact[0]).not.toHaveProperty('totals');
});

test('server-computed totals on the live status are preferred over recomputing', () => {
  const compact = compactLiveKilds([
    {
      id: 'kild-1',
      name: 'ops',
      agents: [{ handle: 'coder', tokens: 100, cost: 0.1 }],
      log: [],
      totals: { tokens: 4000, cost: 1.5 },
    },
  ]);
  expect(compact[0]?.totals).toEqual({ tokens: 4000, cost: 1.5 });
});

test('compaction copies agent and post arrays without mutating the source kild', () => {
  const kilds = [
    {
      id: 'kild-1',
      name: 'ops',
      agents: [{ handle: 'brain', persona: 'brain' }],
      log: [{ id: 'm1', kildId: 'kild-1', from: 'human', to: ['brain'], text: 'one', ts: 1 }],
    },
  ];

  const compact = compactLiveKilds(kilds);
  expect(compact[0]?.agents).not.toBe(kilds[0]?.agents);
  expect(compact[0]?.posts).not.toBe(kilds[0]?.log);
  expect(kilds[0]?.log.map((message) => message.id)).toEqual(['m1']);
});
