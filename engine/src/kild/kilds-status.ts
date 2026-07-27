import { type AgentView, type CostTotals, costTotals, type KildStatus } from './kild-types.ts';
import type { KildGitStatus } from './worktree-status.ts';

/** The director's compact view of a kild's git state: a summary, not the full
 *  changed-file list. Per the pull-not-push discipline, the director sees a COUNT plus
 *  the actionable collisions; the full list stays in the pull/human layer. `path` is
 *  kept — a driving agent needs it to `cd` in and land the work. */
export interface CompactGitStatus {
  path: string;
  branch: string | null;
  base: string;
  ahead: number;
  behind: number;
  dirty: boolean;
  uncommittedFiles: number;
  changedFileCount: number;
  conflictsWithBase: boolean | null;
  error?: string;
}

/** One-line git summary shared by kild list and detail displays. */
export function formatCompactGitSummary(git?: CompactGitStatus): string {
  if (!git) return '';
  return ` · ${git.branch ?? '?'} +${git.ahead}/-${git.behind}${git.dirty ? ' dirty' : ''}${git.conflictsWithBase ? ' CONFLICTS' : ''}`;
}

/** One overlap: `kild` also changed `files`. The specific overlapping files ARE the
 *  actionable signal (which is why they're surfaced compactly, unlike the full list). */
export interface KildCollision {
  kild: string;
  files: string[];
}

export interface CompactKildStatus {
  id: string;
  name: string;
  agents: AgentView[];
  /** The kild's git/worktree summary — code state, not just conversation. */
  git?: CompactGitStatus;
  /** Other live kilds that touch the same files — a merge collision waiting to
   *  happen. Empty/absent when this kild collides with none. */
  collidesWith?: KildCollision[];
  /** Kild cost rollup (tokens + USD) summed over agents — absent until at least
   *  one agent has reported `stats`. Per-agent figures ride `agents`. */
  totals?: CostTotals;
}

/** Full changed-file list → a count for the compact view. `path` and the rest ride
 *  through; only the potentially-large file list is dropped (pull it if you need it). */
function toCompactGit(git: KildGitStatus): CompactGitStatus {
  const { changedFiles, ...rest } = git;
  return { ...rest, changedFileCount: changedFiles.length };
}

/** Cross-kild collisions: for each kild, the other live kilds that changed any of
 *  the same files (committed vs base). Pure — computed once over the enriched set. */
export function computeCollisions(kilds: KildStatus[]): Map<string, KildCollision[]> {
  const result = new Map<string, KildCollision[]>();
  for (const a of kilds) {
    const aFiles = new Set(a.git?.changedFiles ?? []);
    if (aFiles.size === 0) continue;
    const collisions: KildCollision[] = [];
    for (const b of kilds) {
      if (b.id === a.id) continue;
      const shared = (b.git?.changedFiles ?? []).filter((file) => aFiles.has(file));
      if (shared.length > 0) collisions.push({ kild: b.name, files: shared });
    }
    if (collisions.length > 0) result.set(a.id, collisions);
  }
  return result;
}

export function compactLiveKilds(kildStatuses: KildStatus[]): CompactKildStatus[] {
  const collisions = computeCollisions(kildStatuses);
  return kildStatuses.map((kild) => {
    const totals = kild.totals ?? costTotals(kild.agents);
    return {
      id: kild.id,
      name: kild.name,
      agents: kild.agents.map((agent) => ({ ...agent })),
      ...(totals ? { totals } : {}),
      ...(kild.git ? { git: toCompactGit(kild.git) } : {}),
      ...(collisions.has(kild.id) ? { collidesWith: collisions.get(kild.id) } : {}),
    };
  });
}
