import { expect, test } from 'bun:test';

import { composeSessionTurn, DEFAULT_PROMPT } from './default-prompt.ts';

test('the mechanism prompt sits on top of the role-wrapped turn', () => {
  const turn = '<role>\nPERSONA\n</role>\n\nhi';
  expect(composeSessionTurn(turn, DEFAULT_PROMPT)).toBe(`${DEFAULT_PROMPT}\n\n${turn}`);
});

test('a null prefix leaves the turn unchanged (mechanism disabled)', () => {
  expect(composeSessionTurn('just the turn', null)).toBe('just the turn');
});
