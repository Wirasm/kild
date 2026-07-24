import { afterAll, beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import {
  appendRoomLog,
  fleetMemorySection,
  formatRoomLogEntry,
  projectMemorySection,
  synthesisPrompt,
} from './memory.ts';
import type { Room } from './room/room-types.ts';

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

function room(cwd: string, overrides: Partial<Room> = {}): Room {
  return {
    id: 'room-1',
    name: 'fix-auth',
    cwd,
    participants: [
      {
        name: 'worker',
        sessionId: 's-1',
        agent: 'worker',
        model: 'openai-codex/gpt-5.6-sol',
        piSessionId: 'aaaa-bbbb',
        piSessionFile: '/sessions/aaaa-bbbb.jsonl',
      },
    ],
    log: [
      {
        id: 'm0',
        roomId: 'room-1',
        from: 'human',
        to: ['worker'],
        text: 'Fix the auth bug',
        ts: 1,
      },
      { id: 'm1', roomId: 'room-1', from: 'human', to: [], text: 'joined', ts: 2, system: true },
      {
        id: 'm2',
        roomId: 'room-1',
        from: 'worker',
        to: ['human'],
        text: 'Done: commit abc123, tests green',
        ts: 3,
      },
    ],
    state: 'closed',
    worktree: 'fix-auth',
    base: 'main',
    ...overrides,
  };
}

test('the log entry carries goal, outcome, decisions, resume handles, worktree — pure facts', () => {
  const entry = formatRoomLogEntry(
    room('/p', {
      decisions: [
        {
          key: 'api-shape',
          summary: 'REST or RPC?',
          openedBy: 'worker',
          openedAt: 1,
          resolvedBy: 'human',
          resolvedAt: 2,
          note: 'REST',
        },
        { key: 'auth', summary: 'token TTL?', openedBy: 'worker', openedAt: 3 },
      ],
    }),
    new Date('2026-07-24T12:00:00Z'),
  );
  expect(entry).toContain('## 2026-07-24 — fix-auth (room-1)');
  expect(entry).toContain('- goal: Fix the auth bug');
  expect(entry).toContain('- outcome: Done: commit abc123, tests green');
  expect(entry).toContain('→ resolved by @human: REST');
  expect(entry).toContain('decision UNRESOLVED at close: auth');
  expect(entry).toContain(
    '- agent @worker (worker, openai-codex/gpt-5.6-sol) — pi --session /sessions/aaaa-bbbb.jsonl',
  );
  expect(entry).toContain('- worktree: kild/fix-auth (base main)');
});

test('appendRoomLog creates the memory dir with a .gitignore and appends in order', () => {
  const project = fs.mkdtempSync(path.join(tmp, 'proj-'));
  const dir = path.join(project, '.kild');
  appendRoomLog(room(project), dir, new Date('2026-07-24T12:00:00Z'));
  appendRoomLog(
    room(project, { id: 'room-2', name: 'second' }),
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
  appendRoomLog(room(project), dir);
  expect(fs.readFileSync(path.join(dir, '.gitignore'), 'utf8')).toBe('# user-managed\n');
});

test('a memory dir outside the project gets the log but NO .gitignore', () => {
  const project = fs.mkdtempSync(path.join(tmp, 'proj-'));
  const external = fs.mkdtempSync(path.join(tmp, 'store-')); // e.g. a user-home store
  appendRoomLog(room(project), external, new Date('2026-07-24T12:00:00Z'));

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

test('fleetMemorySection reads $KILD_HOME/MAIN_MEMORY.md and is empty when absent', () => {
  expect(fleetMemorySection()).toBe('');
  fs.mkdirSync(process.env.KILD_HOME as string, { recursive: true });
  fs.writeFileSync(
    path.join(process.env.KILD_HOME as string, 'MAIN_MEMORY.md'),
    '- projA — the API',
  );
  expect(fleetMemorySection()).toContain('projA — the API');
});

test('the synthesis charter names the transcript, the memory file, and the constraints', () => {
  const prompt = synthesisPrompt(room('/p'), '/home/rooms/room-1.json', '/p/.kild');
  expect(prompt).toContain('[kild memory synthesis]');
  expect(prompt).toContain('/home/rooms/room-1.json');
  expect(prompt).toContain('.kild/MEMORY.md');
  expect(prompt).toContain('READ-ONLY');
});

test('the synthesis charter names the ACTUAL configured paths for an external dir', () => {
  const prompt = synthesisPrompt(room('/p'), '/home/rooms/room-1.json', '/stores/proj');
  expect(prompt).toContain('/stores/proj/MEMORY.md');
  expect(prompt).toContain('/stores/proj/LOG.md');
  expect(prompt).toContain('/stores/proj/direction.md');
  expect(prompt).not.toContain('.kild/MEMORY.md');
});
