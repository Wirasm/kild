import { expect, test } from 'bun:test';

import { composeSessionTurn, MECHANISM_PROMPT } from './mechanism-prompt.ts';

test('the mechanism prompt sits on top of the role-wrapped turn', () => {
  const turn = '<role>\nPERSONA\n</role>\n\nhi';
  expect(composeSessionTurn(turn, MECHANISM_PROMPT)).toBe(`${MECHANISM_PROMPT}\n\n${turn}`);
});

test('a null prefix leaves the turn unchanged (mechanism disabled)', () => {
  expect(composeSessionTurn('just the turn', null)).toBe('just the turn');
});
