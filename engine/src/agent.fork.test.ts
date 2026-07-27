import { afterAll, beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { SessionManager as PiSessionManager } from '@earendil-works/pi-coding-agent';

// The worker's fork-spawn (KILD_FORK_SESSION) rests on one SDK guarantee:
// SessionManager.forkFrom copies the source session's history into a brand-new
// session file and NEVER writes the source — so a fork can be prompted freely
// without polluting or corrupting the original. These tests pin that contract
// (an explicit sessionDir keeps them out of the user's ~/.pi).

let tmp: string;

beforeAll(() => {
  tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-fork-'));
});

afterAll(() => {
  fs.rmSync(tmp, { recursive: true, force: true });
});

function writeSourceSession(): string {
  const dir = fs.mkdtempSync(path.join(tmp, 'source-'));
  const file = path.join(dir, '2026-07-24T10-00-00-000Z_source.jsonl');
  const entries = [
    {
      type: 'session',
      version: 3,
      id: 'source-session',
      timestamp: '2026-07-24T10:00:00.000Z',
      cwd: '/original/project',
    },
    {
      type: 'message',
      id: 'e1',
      parentId: null,
      timestamp: '2026-07-24T10:00:01.000Z',
      message: { role: 'user', content: [{ type: 'text', text: 'the authoring history' }] },
    },
  ];
  fs.writeFileSync(file, `${entries.map((e) => JSON.stringify(e)).join('\n')}\n`);
  return file;
}

test('forkFrom creates a NEW session file carrying the source history', () => {
  const source = writeSourceSession();
  const targetCwd = fs.mkdtempSync(path.join(tmp, 'target-'));
  const sessionDir = fs.mkdtempSync(path.join(tmp, 'sessions-'));

  const forked = PiSessionManager.forkFrom(source, targetCwd, sessionDir);
  const forkedFile = forked.getSessionFile() as string;

  expect(forkedFile).toBeDefined();
  expect(forkedFile).not.toBe(source);
  expect(forked.getSessionId()).not.toBe('source-session');
  // The copied history is in the fork, and its header points back to the source.
  expect(fs.readFileSync(forkedFile, 'utf8')).toContain('the authoring history');
  expect(forked.getHeader()?.parentSession).toBe(source);
});

test('the source session file is never written — even when the fork is appended to', () => {
  const source = writeSourceSession();
  const before = fs.readFileSync(source, 'utf8');
  const sessionDir = fs.mkdtempSync(path.join(tmp, 'sessions-'));

  const forked = PiSessionManager.forkFrom(
    source,
    fs.mkdtempSync(path.join(tmp, 'target-')),
    sessionDir,
  );
  forked.appendMessage({
    role: 'user',
    content: [{ type: 'text', text: 'a question against the frozen snapshot' }],
  });

  expect(fs.readFileSync(source, 'utf8')).toBe(before); // byte-identical
  expect(fs.readFileSync(forked.getSessionFile() as string, 'utf8')).toContain(
    'a question against the frozen snapshot',
  );
});
