import type { LiveRoomStatus, RoomMessage } from '../room/room-types.ts';
import type { WorkstreamGitStatus } from '../worktree-status.ts';

/** The director's compact view of a workstream's git state: a summary, not the full
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

/** One overlap: `room` also changed `files`. The specific overlapping files ARE the
 *  actionable signal (which is why they're surfaced compactly, unlike the full list). */
export interface WorkstreamCollision {
  room: string;
  files: string[];
}

export interface CompactRoomStatus {
  id: string;
  name: string;
  participants: Array<{ name: string; agent?: string }>;
  posts: RoomMessage[];
  /** The workstream's git/worktree summary — code state, not just conversation. */
  git?: CompactGitStatus;
  /** Other live workstreams that touch the same files — a merge collision waiting to
   *  happen. Empty/absent when this workstream collides with none. */
  collidesWith?: WorkstreamCollision[];
}

/** Full changed-file list → a count for the compact view. `path` and the rest ride
 *  through; only the potentially-large file list is dropped (pull it if you need it). */
function toCompactGit(git: WorkstreamGitStatus): CompactGitStatus {
  const { changedFiles, ...rest } = git;
  return { ...rest, changedFileCount: changedFiles.length };
}

/** Cross-workstream collisions: for each room, the other live rooms that changed any of
 *  the same files (committed vs base). Pure — computed once over the enriched set. */
export function computeCollisions(rooms: LiveRoomStatus[]): Map<string, WorkstreamCollision[]> {
  const result = new Map<string, WorkstreamCollision[]>();
  for (const a of rooms) {
    const aFiles = new Set(a.git?.changedFiles ?? []);
    if (aFiles.size === 0) continue;
    const collisions: WorkstreamCollision[] = [];
    for (const b of rooms) {
      if (b.id === a.id) continue;
      const shared = (b.git?.changedFiles ?? []).filter((file) => aFiles.has(file));
      if (shared.length > 0) collisions.push({ room: b.name, files: shared });
    }
    if (collisions.length > 0) result.set(a.id, collisions);
  }
  return result;
}

export function compactLiveRooms(liveRooms: LiveRoomStatus[]): CompactRoomStatus[] {
  const collisions = computeCollisions(liveRooms);
  return liveRooms.map((room) => ({
    id: room.id,
    name: room.name,
    participants: room.participants.map((participant) => ({
      name: participant.name,
      agent: participant.agent,
    })),
    posts: room.log.slice(-2),
    ...(room.git ? { git: toCompactGit(room.git) } : {}),
    ...(collisions.has(room.id) ? { collidesWith: collisions.get(room.id) } : {}),
  }));
}
