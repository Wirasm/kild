import { expect, test } from 'bun:test';

import { readSkillsProfile, skillsProfileForAgent } from './skills-profile.ts';

test('accepts an absolute skills profile path', () => {
  expect(readSkillsProfile('/profiles/prp')).toBe('/profiles/prp');
});

test('rejects a relative skills profile path at engine startup', () => {
  expect(() => readSkillsProfile('profiles/prp')).toThrow(
    'KILD_SKILLS_PROFILE must be an absolute path',
  );
});

test('assigns the profile only to kild agents', () => {
  expect(skillsProfileForAgent('kild-1', '/profiles/prp')).toBe('/profiles/prp');
  expect(skillsProfileForAgent(undefined, '/profiles/prp')).toBeUndefined();
  expect(skillsProfileForAgent('kild-1', undefined)).toBeUndefined();
});
