import { expect, test } from 'bun:test';

import { readSkillsProfile, skillsProfileForWorker } from './skills-profile.ts';

test('accepts an absolute skills profile path', () => {
  expect(readSkillsProfile('/profiles/prp')).toBe('/profiles/prp');
});

test('rejects a relative skills profile path at engine startup', () => {
  expect(() => readSkillsProfile('profiles/prp')).toThrow(
    'KILD_SKILLS_PROFILE must be an absolute path',
  );
});

test('assigns the profile only to room participants', () => {
  expect(skillsProfileForWorker('room-1', '/profiles/prp')).toBe('/profiles/prp');
  expect(skillsProfileForWorker(undefined, '/profiles/prp')).toBeUndefined();
  expect(skillsProfileForWorker('room-1', undefined)).toBeUndefined();
});
