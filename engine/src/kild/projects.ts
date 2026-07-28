import fs from 'node:fs/promises';
import path from 'node:path';

import { kildHome } from './config.ts';

/** A project is a directory an agent works in — mirror of kild-core::project. */
export interface Project {
  name: string;
  path: string;
}

function projectsFile(): string {
  return path.join(kildHome(), 'projects.json');
}

/**
 * Registered projects, or `[]`.
 *
 * This must never return a non-array. It used to hand back `parsed.projects` unchecked, so a
 * `projects.json` that was hand-edited into a bare array — or anything else without that key
 * — yielded `undefined`, and every caller does `.map`/`.find` on the result. One malformed
 * file 500'd routes across the whole engine, blaming the server for the operator's typo.
 */
export async function loadProjects(): Promise<Project[]> {
  let raw: string;
  try {
    raw = await fs.readFile(projectsFile(), 'utf8');
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') return [];
    throw err;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return []; // unreadable registry → no projects, not a crashed request
  }
  // Accept the canonical `{projects: [...]}`, and tolerate a bare array, which is what a
  // hand-edited file usually becomes.
  const list = Array.isArray(parsed)
    ? parsed
    : Array.isArray((parsed as { projects?: unknown })?.projects)
      ? (parsed as { projects: unknown[] }).projects
      : [];
  return list.filter(
    (entry): entry is Project =>
      typeof entry === 'object' &&
      entry !== null &&
      typeof (entry as Project).name === 'string' &&
      typeof (entry as Project).path === 'string',
  );
}

export async function findProject(name: string): Promise<Project | null> {
  return (await loadProjects()).find((p) => p.name === name) ?? null;
}

/** Remove a project by name (no-op if absent). */
export async function removeProject(name: string): Promise<void> {
  const projects = (await loadProjects()).filter((p) => p.name !== name);
  await fs.mkdir(kildHome(), { recursive: true });
  await fs.writeFile(projectsFile(), JSON.stringify({ projects }, null, 2));
}

/** Register a project. Path must be an existing dir; names unique; `~/` expands. */
export async function addProject(name: string, dir: string): Promise<Project> {
  const resolved = dir.startsWith('~/')
    ? path.join(process.env.HOME ?? '', dir.slice(2))
    : path.resolve(dir);
  const stat = await fs.stat(resolved).catch(() => null);
  if (!stat?.isDirectory()) throw new Error(`not a directory: ${resolved}`);

  const projects = await loadProjects();
  if (projects.some((p) => p.name === name)) throw new Error(`duplicate project name: ${name}`);

  const project: Project = { name, path: resolved };
  projects.push(project);
  await fs.mkdir(kildHome(), { recursive: true });
  await fs.writeFile(projectsFile(), JSON.stringify({ projects }, null, 2));
  return project;
}
