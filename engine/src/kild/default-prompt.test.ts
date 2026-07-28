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

test('`default` is not listed — it describes nothing, and the prompt says so', () => {
  const section = formatPersonasSection([
    { name: 'default', description: '' },
    { name: 'reviewer', description: 'r' },
  ]);
  expect(section).not.toContain('- default');
  // But an agent still has to know it can ask for it.
  expect(section).toContain('`default` (not listed)');
});

test('a persona with no description is still listed — a name beats invisibility', () => {
  const section = formatPersonasSection([{ name: 'coder', description: '' }]);
  expect(section).toContain('- coder');
  expect(section).not.toContain('coder —');
});

test('no personas beyond default yields no section at all, not an empty heading', () => {
  expect(formatPersonasSection([{ name: 'default', description: '' }])).toBe('');
  expect(formatPersonasSection([])).toBe('');
});

test('the catalog explains that handle and persona are separate', () => {
  // The distinction that makes two instances of one persona addressable: same persona,
  // different handles. Without it an agent assumes one persona means one agent.
  const section = formatPersonasSection([{ name: 'coder', description: 'writes code' }]);
  expect(section).toContain('spawn the same persona twice under two handles');
});
