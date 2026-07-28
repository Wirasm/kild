import { expect, test } from 'bun:test';

import { composeSessionTurn, DEFAULT_PROMPT, formatPersonasSection } from './default-prompt.ts';

test('the mechanism prompt sits on top of the role-wrapped turn', () => {
  const turn = '<role>\nPERSONA\n</role>\n\nhi';
  expect(composeSessionTurn(turn, DEFAULT_PROMPT)).toBe(`${DEFAULT_PROMPT}\n\n${turn}`);
});

test('a null prefix leaves the turn unchanged (mechanism disabled)', () => {
  expect(composeSessionTurn('just the turn', null)).toBe('just the turn');
});

// ── the persona catalog ──────────────────────────────────────────────────────────────────
// A delegating agent needs to know which personas EXIST, or it guesses a name and takes a
// rejection. `description` is the reason a catalog beats a name list: it is the signal a
// persona author writes to say what the persona is for.

test('the catalog lists each persona with its description', () => {
  const section = formatPersonasSection([
    { name: 'default', description: '' },
    { name: 'reviewer', description: 'Checks a diff against the tests it claims to pass' },
    { name: 'planner', description: 'Drafts an implementation plan' },
  ]);
  expect(section).toContain('- reviewer — Checks a diff against the tests it claims to pass');
  expect(section).toContain('- planner — Drafts an implementation plan');
});

test('the built-in general agent is listed like any other, with its description', () => {
  // It used to be filtered out for having no description. Describing it was the fix; hiding
  // the one persona that always exists left an agent unable to delegate anything general.
  const section = formatPersonasSection([
    { name: 'general', description: 'General-purpose, no specialisation. Use when no other fits' },
    { name: 'reviewer', description: 'r' },
  ]);
  expect(section).toContain('- general — General-purpose, no specialisation.');
  expect(section).toContain('- reviewer — r');
});

test('a persona with no description is still listed — a name beats invisibility', () => {
  const section = formatPersonasSection([{ name: 'coder', description: '' }]);
  expect(section).toContain('- coder');
  expect(section).not.toContain('coder —');
});

test('an empty catalog yields no section at all, not an empty heading', () => {
  expect(formatPersonasSection([])).toBe('');
});

test('the catalog explains that handle and persona are separate', () => {
  // The distinction that makes two instances of one persona addressable: same persona,
  // different handles. Without it an agent assumes one persona means one agent.
  const section = formatPersonasSection([{ name: 'coder', description: 'writes code' }]);
  expect(section).toContain('name the same persona under two handles');
});
