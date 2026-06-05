import { EventEmitter } from 'node:events';

import { defineTool, Type } from '@flue/runtime';
import type { ToolDefinition } from '@flue/runtime';

/** A message in a kild room. */
export interface RoomMessage {
  room: string;
  from: string;
  text: string;
  ts: number;
}

/**
 * In-process comms bus — agent-to-agent **rooms**, the battery Flue does not
 * provide. Flue's `session.task()` is a strict parent→child tree (subagents may
 * not spawn subagents); a room is a peer graph where independent agents and the
 * human all read/write the same channel. The UI observes via `subscribe`.
 */
class RoomBus extends EventEmitter {
  private readonly log: RoomMessage[] = [];
  private readonly members = new Map<string, Set<string>>();

  join(room: string, agent: string): void {
    if (!this.members.has(room)) this.members.set(room, new Set());
    this.members.get(room)?.add(agent);
  }

  post(room: string, from: string, text: string): RoomMessage {
    const msg: RoomMessage = { room, from, text, ts: Date.now() };
    this.log.push(msg);
    this.emit('message', msg);
    return msg;
  }

  history(room: string): RoomMessage[] {
    return this.log.filter((m) => m.room === room);
  }

  roster(room: string): string[] {
    return [...(this.members.get(room) ?? [])];
  }

  subscribe(listener: (msg: RoomMessage) => void): () => void {
    this.on('message', listener);
    return () => this.off('message', listener);
  }
}

/** Process-wide singleton so agents in different harnesses share rooms. */
export const rooms = new RoomBus();

/** Tools that let an agent speak and listen in a room — kild-owned comms. */
export function roomTools(room: string, agent: string): ToolDefinition[] {
  rooms.join(room, agent);
  return [
    defineTool({
      name: 'room_send',
      description: `Post a message to room "${room}" so other agents (and the human) can read it.`,
      parameters: Type.Object({ text: Type.String({ description: 'The message body.' }) }),
      execute: async (args) => {
        rooms.post(room, agent, String((args as { text: string }).text));
        return 'delivered';
      },
    }),
    defineTool({
      name: 'room_read',
      description: `Read the full message history of room "${room}" as a JSON array of {from,text}.`,
      parameters: Type.Object({}),
      execute: async () =>
        JSON.stringify(rooms.history(room).map((m) => ({ from: m.from, text: m.text }))),
    }),
    defineTool({
      name: 'room_members',
      description: `List the agents present in room "${room}".`,
      parameters: Type.Object({}),
      execute: async () => JSON.stringify(rooms.roster(room)),
    }),
  ];
}
