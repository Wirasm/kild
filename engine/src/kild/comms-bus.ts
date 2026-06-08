import { EventEmitter } from 'node:events';
import type { ToolDefinition } from '@flue/runtime';
import { defineTool, Type } from '@flue/runtime';

/** A message on a kild comms channel (the Flue-layer peer-comms bus). */
export interface CommsMessage {
  room: string;
  from: string;
  text: string;
  ts: number;
}

/**
 * In-process comms bus — agent-to-agent peer comms, the battery Flue does not
 * provide. Flue's `session.task()` is a strict parent→child tree (subagents may
 * not spawn subagents); this bus is a peer graph where independent agents and the
 * human all read/write the same channel. The UI observes via `subscribe`.
 *
 * NOTE: distinct from the operator-facing **Room** primitive (`kild/room/*`) — this
 * is the Flue-layer peer-comms demo used by the brain / rooms workflows.
 */
class CommsBus extends EventEmitter {
  private readonly log: CommsMessage[] = [];
  private readonly members = new Map<string, Set<string>>();

  join(room: string, agent: string): void {
    if (!this.members.has(room)) this.members.set(room, new Set());
    this.members.get(room)?.add(agent);
  }

  post(room: string, from: string, text: string): CommsMessage {
    const msg: CommsMessage = { room, from, text, ts: Date.now() };
    this.log.push(msg);
    this.emit('message', msg);
    return msg;
  }

  history(room: string): CommsMessage[] {
    return this.log.filter((m) => m.room === room);
  }

  roster(room: string): string[] {
    return [...(this.members.get(room) ?? [])];
  }

  subscribe(listener: (msg: CommsMessage) => void): () => void {
    this.on('message', listener);
    return () => this.off('message', listener);
  }
}

/** Process-wide singleton so agents in different harnesses share the bus. */
export const commsBus = new CommsBus();

/** Tools that let an agent speak and listen on a comms channel — kild-owned comms. */
export function commsTools(room: string, agent: string): ToolDefinition[] {
  commsBus.join(room, agent);
  return [
    defineTool({
      name: 'room_send',
      description: `Post a message to room "${room}" so other agents (and the human) can read it.`,
      parameters: Type.Object({ text: Type.String({ description: 'The message body.' }) }),
      execute: async (args) => {
        commsBus.post(room, agent, String((args as { text: string }).text));
        return 'delivered';
      },
    }),
    defineTool({
      name: 'room_read',
      description: `Read the full message history of room "${room}" as a JSON array of {from,text}.`,
      parameters: Type.Object({}),
      execute: async () =>
        JSON.stringify(commsBus.history(room).map((m) => ({ from: m.from, text: m.text }))),
    }),
    defineTool({
      name: 'room_members',
      description: `List the agents present in room "${room}".`,
      parameters: Type.Object({}),
      execute: async () => JSON.stringify(commsBus.roster(room)),
    }),
  ];
}
