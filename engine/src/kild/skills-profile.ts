import path from 'node:path';

/** Read once when the engine starts: a room-only directory of skills. */
export function readSkillsProfile(value: string | undefined): string | undefined {
  if (!value) return undefined;
  if (!path.isAbsolute(value)) {
    throw new Error('KILD_SKILLS_PROFILE must be an absolute path');
  }
  return value;
}

/** A capability profile belongs only to room participants, never ordinary sessions. */
export function skillsProfileForWorker(
  roomId: string | undefined,
  skillsProfile: string | undefined,
): string | undefined {
  return roomId ? skillsProfile : undefined;
}
