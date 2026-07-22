import type { LiveRoomStatus, RoomMessage } from '../room/room-types.ts';
import type { WorkstreamGitStatus } from '../worktree-status.ts';

export interface CompactRoomStatus {
  id: string;
  name: string;
  participants: Array<{ name: string; agent?: string }>;
  posts: RoomMessage[];
  /** The workstream's git/worktree state, when available — lets a driving agent see
   *  code state (branch, ahead/behind, dirty, changed files), not just conversation. */
  git?: WorkstreamGitStatus;
}

export function compactLiveRooms(liveRooms: LiveRoomStatus[]): CompactRoomStatus[] {
  return liveRooms.map((room) => ({
    id: room.id,
    name: room.name,
    participants: room.participants.map((participant) => ({
      name: participant.name,
      agent: participant.agent,
    })),
    posts: room.log.slice(-2),
    ...(room.git ? { git: room.git } : {}),
  }));
}
