import { afterAll, beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { Kild } from './kild-types.ts';
import {
  appendKildLog,
  formatKildLogEntry,
  NO_FINAL_MESSAGE,
  projectMemorySection,
  synthesisPrompt,
} from './memory.ts';

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
      },
      {
        id: 'm1',
        kildId: 'kild-1',
        from: 'agent',
        to: ['human'],
        text: 'Done: commit abc123, tests green',
        ts: 2,
      },
    ],
    worktree: 'fix-auth',
    base: 'main',
    ...overrides,
  };
}

test('the log entry carries goal, outcome, resume handles, worktree — pure facts', () => {
  const entry = formatKildLogEntry(kild('/p'), new Date('2026-07-24T12:00:00Z'));
  expect(entry).toContain('## 2026-07-24 — fix-auth (kild-1)');
  expect(entry).toContain('- goal: Fix the auth bug');
  expect(entry).toContain('- outcome: Done: commit abc123, tests green');
  expect(entry).toContain(
    '- agent @agent (agent, openai-codex/gpt-5.6-sol) — pi --session /sessions/aaaa-bbbb.jsonl',
  );
  expect(entry).toContain('- worktree: kild/fix-auth (base main)');
});

test('goal is the first message and outcome the last; an empty kild says so', () => {
  // Both were "the first/last non-system post" while engine notices sat on the log.
  // With notices gone the log is nothing but real messages, so the ends are the ends.
  const empty = formatKildLogEntry(kild('/p', { log: [] }), new Date('2026-07-24T12:00:00Z'));
  expect(empty).not.toContain('- goal:');
  expect(empty).toContain(`- outcome: ${NO_FINAL_MESSAGE}`);
  expect(NO_FINAL_MESSAGE).toBe('(no messages recorded)');
});

test('appendKildLog creates the memory dir with a .gitignore and appends in order', () => {
  const project = fs.mkdtempSync(path.join(tmp, 'proj-'));
  const dir = path.join(project, '.kild');
  appendKildLog(kild(project), dir, new Date('2026-07-24T12:00:00Z'));
  appendKildLog(
    kild(project, { id: 'kild-2', name: 'second' }),
    dir,
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
  appendKildLog(kild(project), dir);
  expect(fs.readFileSync(path.join(dir, '.gitignore'), 'utf8')).toBe('# user-managed\n');
});

test('a memory dir outside the project gets the log but NO .gitignore', () => {
  const project = fs.mkdtempSync(path.join(tmp, 'proj-'));
  const external = fs.mkdtempSync(path.join(tmp, 'store-')); // e.g. a user-home store
  appendKildLog(kild(project), external, new Date('2026-07-24T12:00:00Z'));

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

test('the synthesis charter names the transcript, the memory file, and the constraints', () => {
  const prompt = synthesisPrompt(kild('/p'), '/home/kilds/kild-1.json', '/p/.kild');
  expect(prompt).toContain('[kild memory synthesis]');
  expect(prompt).toContain('/home/kilds/kild-1.json');
  expect(prompt).toContain('.kild/MEMORY.md');
  expect(prompt).toContain('READ-ONLY');
});

test('the synthesis charter names the ACTUAL configured paths for an external dir', () => {
  const prompt = synthesisPrompt(kild('/p'), '/home/kilds/kild-1.json', '/stores/proj');
  expect(prompt).toContain('/stores/proj/MEMORY.md');
  expect(prompt).toContain('/stores/proj/LOG.md');
  expect(prompt).toContain('/stores/proj/direction.md');
  expect(prompt).not.toContain('.kild/MEMORY.md');
});
