import { afterAll, beforeAll, expect, test } from 'bun:test';
import { execFile as execFileCb } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import type { Kild } from './kild-types.ts';
import {
  appendKildLog,
  collectLedgerFacts,
  formatKildLogEntry,
  type KildLedgerFacts,
  projectMemorySection,
} from './memory.ts';

const execFile = promisify(execFileCb);

let tmp: string;
let prevHome: string | undefined;

beforeAll(() => {
  prevHome = process.env.KILD_HOME;
  tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-memory-'));
  process.env.KILD_HOME = path.join(tmp, 'home');
});

afterAll(() => {
  if (prevHome === undefined) delete process.env.KILD_HOME;
  else process.env.KILD_HOME = prevHome;
  fs.rmSync(tmp, { recursive: true, force: true });
});

function kild(cwd: string, overrides: Partial<Kild> = {}): Kild {
  return {
    id: 'kild-1',
    name: 'fix-auth',
    cwd,
    agents: [
      {
        handle: 'agent',
        id: 's-1',
        persona: 'agent',
        model: 'openai-codex/gpt-5.6-sol',
        tokens: 41200,
        cost: 0.31,
        piSessionId: 'aaaa-bbbb',
        piSessionFile: '/sessions/aaaa-bbbb.jsonl',
      },
    ],
    log: [
      {
        id: 'm0',
        kildId: 'kild-1',
        from: 'human',
        to: ['agent'],
        text: 'Fix the auth bug',
        ts: 1,
        seq: 1,
      },
      {
        id: 'm1',
        kildId: 'kild-1',
        from: 'agent',
        to: ['human'],
        text: 'second message',
        ts: 2,
        seq: 2,
      },
    ],
    worktree: 'fix-auth',
    base: 'main',
    ...overrides,
  };
}

/** Git facts as the collector would return them for an unlanded kild branch. */
function facts(overrides: Partial<KildLedgerFacts> = {}): KildLedgerFacts {
  return {
    base: 'main',
    branch: 'kild/fix-auth',
    commits: 3,
    tip: '9f3a1c2',
    changedFiles: 7,
    uncommittedFiles: 2,
    landed: false,
    ...overrides,
  };
}

test('the ledger entry carries land result, code facts, agent spend, resume handle', () => {
  const entry = formatKildLogEntry(kild('/p'), facts(), new Date('2026-07-24T12:00:00Z'));
  expect(entry).toContain('## 2026-07-24 — fix-auth (kild-1)');
  expect(entry).toContain('- goal: Fix the auth bug');
  expect(entry).toContain('- landed: no — 3 commits on kild/fix-auth not in main (tip 9f3a1c2)');
  expect(entry).toContain('- code: 3 commits vs main, 7 files changed, 2 uncommitted');
  expect(entry).toContain(
    '- agent @agent (agent, openai-codex/gpt-5.6-sol) — 41200 tokens, $0.3100 — ' +
      'pi --session /sessions/aaaa-bbbb.jsonl',
  );
  expect(entry).toContain('- worktree: kild/fix-auth (base main)');
});

test('the entry NEVER carries a prose outcome — not even the last message', () => {
  // The bug this ledger replaced: `outcome:` was the tail of the message log, so a
  // throwaway post became the recorded result of the run.
  const entry = formatKildLogEntry(kild('/p'), facts(), new Date('2026-07-24T12:00:00Z'));
  expect(entry).not.toContain('outcome');
  expect(entry).not.toContain('second message');
});

test('a kild stopped without landing says so plainly, with or without commits', () => {
  const nothing = formatKildLogEntry(
    kild('/p'),
    facts({ commits: 0, tip: undefined, changedFiles: 0, uncommittedFiles: 0 }),
    new Date('2026-07-24T12:00:00Z'),
  );
  expect(nothing).toContain('- landed: no — nothing committed on kild/fix-auth vs main');
  expect(nothing).toContain('- code: 0 commits vs main, 0 files changed');

  const landed = formatKildLogEntry(
    kild('/p'),
    facts({ commits: 0, tip: undefined, landed: true }),
    new Date('2026-07-24T12:00:00Z'),
  );
  expect(landed).toContain('- landed: yes — kild/fix-auth is contained in main');
});

test('a landed kild records what it landed, not what is left over afterwards', () => {
  // The contradiction this pins: `collectLedgerFacts` runs at close, AFTER the merge, and
  // `base..HEAD` is empty once the branch is contained in base — so a landed kild wrote
  // `code: 0 commits, 0 files changed` directly beneath `landed: yes`. The kild that
  // finished its work recorded none of it. The counts now come from the land itself.
  const entry = formatKildLogEntry(
    kild('/p'),
    facts({ commits: 4, changedFiles: 7, landed: true, landedSha: 'abcdef1234567890' }),
    new Date('2026-07-24T12:00:00Z'),
  );
  expect(entry).toContain('- landed: yes — kild/fix-auth merged into main as abcdef1');
  expect(entry).toContain('- code: 4 commits landed into main, 7 files changed');
  // …and "vs main" is not claimed for a branch that IS main's history now.
  expect(entry).not.toContain('commits vs main');
});

test('an UNLANDED kild still measures against base — the wording tracks the state', () => {
  const entry = formatKildLogEntry(
    kild('/p'),
    facts({ commits: 2, changedFiles: 3, landed: false }),
    new Date('2026-07-24T12:00:00Z'),
  );
  expect(entry).toContain('- code: 2 commits vs main, 3 files changed');
});

test('a RECORDED merge sha wins over inference — the ledger names the commit', () => {
  // Inference can only ever assert containment ("is contained in main"). When the engine
  // performed the land it holds the sha, so the entry says which commit it became — and
  // keeps saying it even if git has since gone unreadable.
  const entry = formatKildLogEntry(
    kild('/p'),
    facts({ commits: 0, tip: undefined, landed: true, landedSha: 'abcdef1234567890' }),
    new Date('2026-07-24T12:00:00Z'),
  );
  expect(entry).toContain('- landed: yes — kild/fix-auth merged into main as abcdef1');

  const despiteGitError = formatKildLogEntry(
    kild('/p'),
    facts({ landedSha: 'abcdef1234567890', gitError: 'base ref not found: main' }),
    new Date('2026-07-24T12:00:00Z'),
  );
  expect(despiteGitError).toContain('merged into main as abcdef1');
  expect(despiteGitError).not.toContain('landed: unknown');
});

test('collectLedgerFacts treats a recorded sha as landed without asking git', async () => {
  const dir = await repoWithBranch(); // 2 commits ahead of main, nothing merged
  const collected = await collectLedgerFacts(
    dir,
    { base: 'main', source: 'explicit' },
    'abcdef1234567890',
  );
  expect(collected.landed).toBe(true);
  expect(collected.landedSha).toBe('abcdef1234567890');
  // The code facts are still measured, not overwritten by the land record.
  expect(collected.commits).toBe(2);
});

test('a kild that ran in the checkout says there was no branch to land', () => {
  // No worktree: the agents worked on the base branch itself, so "0 commits vs main
  // on main" would imply a branch that never existed.
  const entry = formatKildLogEntry(
    kild('/p', { worktree: undefined }),
    facts({ branch: 'main', commits: 0, tip: undefined, changedFiles: 0 }),
    new Date('2026-07-24T12:00:00Z'),
  );
  expect(entry).toContain('- landed: no — ran on main itself, no branch to land');
  expect(entry).not.toContain('- worktree:');
});

test('a git failure is recorded as unknown, never as an invented land result', () => {
  const entry = formatKildLogEntry(
    kild('/p'),
    facts({ commits: 0, tip: undefined, changedFiles: 0, gitError: 'base ref not found: main' }),
    new Date('2026-07-24T12:00:00Z'),
  );
  expect(entry).toContain('- landed: unknown — git: base ref not found: main');
});

test('an agent whose persona was never recorded is not given an invented one', () => {
  const entry = formatKildLogEntry(
    kild('/p', {
      agents: [
        { handle: 'coder', id: 's-1' },
        { handle: 'claude', ownership: 'attached', inbox: { drain: () => ({}) } as never },
      ],
    }),
    facts(),
    new Date('2026-07-24T12:00:00Z'),
  );
  // Not `(default)`. The roster records the persona that actually ran, so a genuinely
  // absent one (a kild persisted before it was recorded) is reported as absent rather than
  // named — the ledger holds facts, and `default` here was a guess that read as one.
  expect(entry).toContain('- agent @coder\n');
  expect(entry).not.toContain('default');
  expect(entry).toContain('- attached @claude');
});

test('a recorded persona and model both ride the agent line', () => {
  const entry = formatKildLogEntry(
    kild('/p', {
      agents: [{ handle: 'reviewer', id: 's-1', persona: 'coder', model: 'anthropic/opus' }],
    }),
    facts(),
    new Date('2026-07-24T12:00:00Z'),
  );
  // The handle and the persona differ on purpose: attribution used to report the persona as
  // if it were the name, which made two agents sharing a persona indistinguishable.
  expect(entry).toContain('- agent @reviewer (coder, anthropic/opus)');
});

// ── the facts themselves, against a real repo ────────────────────────────────

async function git(dir: string, args: string[]): Promise<void> {
  await execFile('git', ['-C', dir, '-c', 'user.email=t@t', '-c', 'user.name=t', ...args]);
}

/** A repo on `main` with one commit, plus a `kild/work` branch with two more. */
async function repoWithBranch(): Promise<string> {
  const dir = fs.mkdtempSync(path.join(tmp, 'repo-'));
  await git(dir, ['init', '-b', 'main']);
  fs.writeFileSync(path.join(dir, 'README.md'), 'hello\n');
  await git(dir, ['add', '.']);
  await git(dir, ['commit', '-m', 'initial']);
  await git(dir, ['checkout', '-b', 'kild/work']);
  for (const name of ['a.ts', 'b.ts']) {
    fs.writeFileSync(path.join(dir, name), 'x\n');
    await git(dir, ['add', '.']);
    await git(dir, ['commit', '-m', `add ${name}`]);
  }
  return dir;
}

test('collectLedgerFacts reports real commits, files and an unlanded branch', async () => {
  const dir = await repoWithBranch();
  fs.writeFileSync(path.join(dir, 'c.ts'), 'x\n'); // uncommitted

  const collected = await collectLedgerFacts(dir, { base: 'main', source: 'explicit' });
  expect(collected.gitError).toBeUndefined();
  expect(collected.base).toBe('main');
  expect(collected.branch).toBe('kild/work');
  expect(collected.commits).toBe(2);
  expect(collected.tip).toMatch(/^[0-9a-f]{7}$/);
  expect(collected.changedFiles).toBe(2);
  expect(collected.uncommittedFiles).toBe(1);
  expect(collected.landed).toBe(false);
});

test('collectLedgerFacts reports landed once the branch is contained in base', async () => {
  const dir = await repoWithBranch();
  await git(dir, ['checkout', 'main']);
  await git(dir, ['merge', '--no-ff', '-m', 'land', 'kild/work']);
  await git(dir, ['checkout', 'kild/work']);

  const collected = await collectLedgerFacts(dir, { base: 'main', source: 'explicit' });
  expect(collected.commits).toBe(0);
  expect(collected.landed).toBe(true);
});

test('collectLedgerFacts on a non-repo yields an error, not a throw', async () => {
  const dir = fs.mkdtempSync(path.join(tmp, 'norepo-'));
  const collected = await collectLedgerFacts(dir, { base: 'main', source: 'explicit' });
  expect(collected.gitError).toBeDefined();
  expect(collected.landed).toBe(false);
  expect(collected.commits).toBe(0);
});

// ── the file half ────────────────────────────────────────────────────────────

test('appendKildLog creates the memory dir with a .gitignore and appends in order', () => {
  const project = fs.mkdtempSync(path.join(tmp, 'proj-'));
  const dir = path.join(project, '.kild');
  appendKildLog(kild(project), dir, facts(), new Date('2026-07-24T12:00:00Z'));
  appendKildLog(
    kild(project, { id: 'kild-2', name: 'second' }),
    dir,
    facts(),
    new Date('2026-07-25T12:00:00Z'),
  );

  const log = fs.readFileSync(path.join(dir, 'LOG.md'), 'utf8');
  expect(log.indexOf('fix-auth')).toBeLessThan(log.indexOf('second'));

  const gitignore = fs.readFileSync(path.join(dir, '.gitignore'), 'utf8');
  for (const name of ['MEMORY.md', 'LOG.md', 'direction.md', '.memory-state.json']) {
    expect(gitignore).toContain(name);
  }
});

test('an existing memory-dir .gitignore is never clobbered', () => {
  const project = fs.mkdtempSync(path.join(tmp, 'proj-'));
  const dir = path.join(project, '.kild');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, '.gitignore'), '# user-managed\n');
  appendKildLog(kild(project), dir, facts());
  expect(fs.readFileSync(path.join(dir, '.gitignore'), 'utf8')).toBe('# user-managed\n');
});

test('a memory dir outside the project gets the log but NO .gitignore', () => {
  const project = fs.mkdtempSync(path.join(tmp, 'proj-'));
  const external = fs.mkdtempSync(path.join(tmp, 'store-')); // e.g. a user-home store
  appendKildLog(kild(project), external, facts(), new Date('2026-07-24T12:00:00Z'));

  expect(fs.readFileSync(path.join(external, 'LOG.md'), 'utf8')).toContain('fix-auth');
  expect(fs.existsSync(path.join(external, '.gitignore'))).toBe(false);
  expect(fs.existsSync(path.join(project, '.kild'))).toBe(false); // nothing written in-project
});

test('projectMemorySection composes MEMORY.md + direction.md and is empty when absent', () => {
  const project = fs.mkdtempSync(path.join(tmp, 'proj-'));
  const dir = path.join(project, '.kild');
  expect(projectMemorySection(project, dir)).toBe('');
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'MEMORY.md'), 'Auth uses tokens.');
  fs.writeFileSync(path.join(dir, 'direction.md'), 'Ship v2 by fall.');
  const section = projectMemorySection(project, dir);
  expect(section).toContain('<project-memory>');
  expect(section).toContain('(.kild/MEMORY.md)'); // default dir reads project-relative
  expect(section).toContain('Auth uses tokens.');
  expect(section).toContain('Ship v2 by fall.');
});

test('projectMemorySection reads an external memory dir and names its actual paths', () => {
  const project = fs.mkdtempSync(path.join(tmp, 'proj-'));
  const external = fs.mkdtempSync(path.join(tmp, 'store-'));
  fs.writeFileSync(path.join(external, 'MEMORY.md'), 'Lives outside the repo.');
  const section = projectMemorySection(project, external);
  expect(section).toContain('Lives outside the repo.');
  expect(section).toContain(`(${path.join(external, 'MEMORY.md')})`);
});
