import { afterAll, beforeAll, expect, test } from 'bun:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import {
  clampTranscriptTail,
  DEFAULT_TRANSCRIPT_TAIL,
  MAX_TRANSCRIPT_TAIL,
  parseSessionTranscript,
  readSessionTranscript,
} from './session-transcript.ts';

// Synthetic pi session JSONL, mirroring the real v3 shapes observed under
// ~/.pi/agent/sessions/: session header, model/thinking bookkeeping, then message
// entries (user/assistant/toolResult) whose content blocks are text/thinking/toolCall.
const line = (value: unknown) => JSON.stringify(value);
const message = (role: string, content: unknown) =>
  line({ type: 'message', message: { role, content } });

const FIXTURE_LINES = [
  line({ type: 'session', version: 3, id: 'sess-1', timestamp: 't', cwd: '/tmp/ws' }),
  line({ type: 'model_change', id: 'a', parentId: null, provider: 'minimax', modelId: 'M3' }),
  line({ type: 'thinking_level_change', id: 'b', thinkingLevel: 'medium' }),
  message('user', [{ type: 'text', text: 'fix the bug' }]),
  message('assistant', [
    { type: 'thinking', thinking: 'internal reasoning — not transcript text' },
    { type: 'text', text: 'Looking at the file.' },
    { type: 'toolCall', id: 'c1', name: 'read', arguments: { path: 'a.ts' } },
  ]),
  message('toolResult', [{ type: 'text', text: 'contents of a.ts' }]),
  message('assistant', [{ type: 'thinking', thinking: 'thinking-only entry' }]),
  message('assistant', [{ type: 'text', text: 'Done — fixed.' }]),
  line({ type: 'future_entry_kind', payload: true }),
  message('supervisor', [{ type: 'text', text: 'unknown role' }]),
  'this line is not JSON at all',
  '{"type":"message","message":{"role":"assistant","content":[{"type":"te', // partial trailing write
];
const FIXTURE = FIXTURE_LINES.join('\n');

test('parses the known message shapes into compact entries and skips everything else', () => {
  const { entries, total } = parseSessionTranscript(FIXTURE);
  expect(total).toBe(4);
  expect(entries).toEqual([
    { role: 'user', text: 'fix the bug' },
    { role: 'assistant', text: 'Looking at the file.', toolCalls: ['read'] },
    { role: 'tool', text: 'contents of a.ts' },
    { role: 'assistant', text: 'Done — fixed.' },
  ]);
});

test('tail keeps only the most recent entries but reports the full total', () => {
  const { entries, total } = parseSessionTranscript(FIXTURE, 2);
  expect(total).toBe(4);
  expect(entries.map((e) => e.text)).toEqual(['contents of a.ts', 'Done — fixed.']);
});

test('a string-content message (legacy/loose shape) still yields its text', () => {
  const { entries } = parseSessionTranscript(message('user', 'plain string content'));
  expect(entries).toEqual([{ role: 'user', text: 'plain string content' }]);
});

test('a toolCall-only assistant entry survives with empty text', () => {
  const { entries } = parseSessionTranscript(
    message('assistant', [{ type: 'toolCall', id: 'c9', name: 'bash', arguments: {} }]),
  );
  expect(entries).toEqual([{ role: 'assistant', text: '', toolCalls: ['bash'] }]);
});

test('oversized entry text is truncated to bound the response size', () => {
  const { entries } = parseSessionTranscript(
    message('toolResult', [{ type: 'text', text: 'x'.repeat(10_000) }]),
  );
  expect(entries[0]?.text.length).toBeLessThan(4_100);
  expect(entries[0]?.text.endsWith('… (truncated)')).toBe(true);
});

test('garbage input never throws — empty and hostile lines produce an empty transcript', () => {
  expect(parseSessionTranscript('')).toEqual({ entries: [], total: 0 });
  expect(parseSessionTranscript('null\n42\n"str"\n[1,2]\n{"type":"message"}\n{}')).toEqual({
    entries: [],
    total: 0,
  });
});

test('clampTranscriptTail: default for absent/invalid, clamped to [1, MAX]', () => {
  expect(clampTranscriptTail()).toBe(DEFAULT_TRANSCRIPT_TAIL);
  expect(clampTranscriptTail(Number.NaN)).toBe(DEFAULT_TRANSCRIPT_TAIL);
  expect(clampTranscriptTail(0)).toBe(1);
  expect(clampTranscriptTail(7)).toBe(7);
  expect(clampTranscriptTail(1_000_000)).toBe(MAX_TRANSCRIPT_TAIL);
});

// ── file-backed read (the endpoint path) ────────────────────────────────────────────

let tmp: string;
beforeAll(() => {
  tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'kild-transcript-'));
});
afterAll(() => {
  fs.rmSync(tmp, { recursive: true, force: true });
});

test('readSessionTranscript reads and parses a fixture file with a tail', async () => {
  const file = path.join(tmp, 'session.jsonl');
  fs.writeFileSync(file, FIXTURE);
  const { entries, total } = await readSessionTranscript(file, 1);
  expect(total).toBe(4);
  expect(entries).toEqual([{ role: 'assistant', text: 'Done — fixed.' }]);
});

test('readSessionTranscript throws on a missing file (the caller maps it to 404)', async () => {
  expect(readSessionTranscript(path.join(tmp, 'gone.jsonl'))).rejects.toThrow();
});
