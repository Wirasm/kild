import type { ArchivedRoom, RoomMessage } from '../room/room-types.ts';

export interface CompactRoomStatus {
  id: string;
  name: string;
  participants: Array<{ name: string; agent?: string }>;
  posts: RoomMessage[];
}

export function compactLiveRooms(liveRooms: ArchivedRoom[]): CompactRoomStatus[] {
  return liveRooms.map((room) => ({
    id: room.id,
    name: room.name,
    participants: room.participants.map((participant) => ({
      name: participant.name,
      agent: participant.agent,
    })),
    posts: room.log.slice(-2),
  }));
}
