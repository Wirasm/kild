import { expect, test } from 'bun:test';

import { parseMentions } from './parse-mentions.ts';

test('extracts a single @handle', () => {
  expect(parseMentions('@worker do X')).toEqual(['worker']);
});

test('dedupes and preserves first-seen order', () => {
  expect(parseMentions('@orchestrator @human hi @orchestrator again')).toEqual([
    'orchestrator',
    'human',
  ]);
});

test('accepts hyphens and underscores in handles', () => {
  expect(parseMentions('@code-reviewer and @qa_bot')).toEqual(['code-reviewer', 'qa_bot']);
});

test('returns empty when there are no mentions', () => {
  expect(parseMentions('just some prose, no handles')).toEqual([]);
});

test('an npm scope is not a mention', () => {
  expect(parseMentions('read the docs in @earendil-works/pi-coding-agent')).toEqual([]);
});

test('an email-like token is not a mention', () => {
  expect(parseMentions('mail rasmus@example.com about it')).toEqual([]);
});

test('a real mention still parses next to code-ish tokens', () => {
  expect(parseMentions('@worker read @scope/pkg and report')).toEqual(['worker']);
});
