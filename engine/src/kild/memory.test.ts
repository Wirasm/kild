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

test('appendRoomLog creates .kild with a memory .gitignore and appends in order', () => {
  const project = fs.mkdtempSync(path.join(tmp, 'proj-'));
  appendRoomLog(room(project), new Date('2026-07-24T12:00:00Z'));
  appendRoomLog(room(project, { id: 'room-2', name: 'second' }), new Date('2026-07-25T12:00:00Z'));

  const log = fs.readFileSync(path.join(project, '.kild', 'LOG.md'), 'utf8');
  expect(log.indexOf('fix-auth')).toBeLessThan(log.indexOf('second'));

  const gitignore = fs.readFileSync(path.join(project, '.kild', '.gitignore'), 'utf8');
  for (const name of ['MEMORY.md', 'LOG.md', 'direction.md', '.memory-state.json']) {
    expect(gitignore).toContain(name);
  }
});

test('an existing .kild/.gitignore is never clobbered', () => {
  const project = fs.mkdtempSync(path.join(tmp, 'proj-'));
  fs.mkdirSync(path.join(project, '.kild'), { recursive: true });
  fs.writeFileSync(path.join(project, '.kild', '.gitignore'), '# user-managed\n');
  appendRoomLog(room(project));
  expect(fs.readFileSync(path.join(project, '.kild', '.gitignore'), 'utf8')).toBe(
    '# user-managed\n',
  );
});

test('projectMemorySection composes MEMORY.md + direction.md and is empty when absent', () => {
  const project = fs.mkdtempSync(path.join(tmp, 'proj-'));
  expect(projectMemorySection(project)).toBe('');
  fs.mkdirSync(path.join(project, '.kild'), { recursive: true });
  fs.writeFileSync(path.join(project, '.kild', 'MEMORY.md'), 'Auth uses tokens.');
  fs.writeFileSync(path.join(project, '.kild', 'direction.md'), 'Ship v2 by fall.');
  const section = projectMemorySection(project);
  expect(section).toContain('<project-memory>');
  expect(section).toContain('Auth uses tokens.');
  expect(section).toContain('Ship v2 by fall.');
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
  const prompt = synthesisPrompt(room('/p'), '/home/rooms/room-1.json');
  expect(prompt).toContain('[kild memory synthesis]');
  expect(prompt).toContain('/home/rooms/room-1.json');
  expect(prompt).toContain('.kild/MEMORY.md');
  expect(prompt).toContain('READ-ONLY');
});
