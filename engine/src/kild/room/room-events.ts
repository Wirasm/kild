import type { Room, RoomMessage } from './room-types.ts';

/** Stable fallback when a lifecycle event has no participant/human-authored post to report. */
export const NO_FINAL_POST = '(no non-system posts recorded)';

export type RoomOperatorEvent =
  | { kind: 'human_post'; from: string; text: string }
  | { kind: 'halted'; finalPost: string }
  | { kind: 'closed'; finalPost: string };

/** The final meaningful room post — engine notices are lifecycle metadata, not work output. */
export function finalNonSystemPost(room: Pick<Room, 'log'>): string {
  for (let index = room.log.length - 1; index >= 0; index -= 1) {
    const message = room.log[index];
    if (message && !message.system) return message.text;
  }
  return NO_FINAL_POST;
}

/** Return the opener only when it is outside the room, avoiding self-directed room turns. */
export function openerNotificationTarget(
  room: Pick<Room, 'openedBy' | 'participants'>,
): string | undefined {
  if (!room.openedBy) return undefined;
  return room.participants.some((participant) => participant.sessionId === room.openedBy)
    ? undefined
    : room.openedBy;
}

/** A direct SessionManager prompt, deliberately distinct from a RoomMessage/room delivery. */
export function formatOperatorNotification(roomName: string, event: RoomOperatorEvent): string {
  const label = `[kild operator notification] Room '${roomName}'`;
  if (event.kind === 'human_post') {
    return `${label}: @${event.from} posted to @human: ${event.text}`;
  }
  const state = event.kind === 'halted' ? 'was halted' : 'was closed and archived';
  return `${label} ${state}. Final non-system post: ${event.finalPost}`;
}

export function humanPostEvent(message: RoomMessage): RoomOperatorEvent {
  return { kind: 'human_post', from: message.from, text: message.text };
}
