import { expect, test } from 'bun:test';

import { stripFrontmatter } from './agents.ts';

test('strips a leading YAML frontmatter block', () => {
  expect(
    stripFrontmatter('---\nname: planner\ndescription: plans\n---\nYou are a planner.\n'),
  ).toBe('You are a planner.');
});

test('passes through content with no frontmatter', () => {
  expect(stripFrontmatter('You are a planner.')).toBe('You are a planner.');
});

test('normalizes CRLF', () => {
  expect(stripFrontmatter('---\r\nname: x\r\n---\r\nbody')).toBe('body');
});
