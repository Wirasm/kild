import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';

import { postRoom } from './engine-client.ts';

export function createPostRoomTool(): ToolDefinition {
  return {
    name: 'post_room',
    label: 'Post Room',
    description:
      'Post a message to a live kild room through the engine REST API. `from` defaults ' +
      'to `human` when omitted.',
    promptSnippet: 'post_room — send a message to a live room by id',
    parameters: Type.Object({
      roomId: Type.String({ description: 'The live room id.' }),
      text: Type.String({ description: 'The message body to post.' }),
      from: Type.Optional(Type.String({ description: 'Optional sender label override.' })),
    }),
    async execute(_toolCallId, params) {
      const { roomId, text, from } = params as { roomId: string; text: string; from?: string };
      // Default the sender to 'brain' — this tool is only held by the fleet brain,
      // and rooms must see who is actually steering them.
      await postRoom(roomId, text, from ?? 'brain');
      return {
        content: [{ type: 'text' as const, text: 'Posted to the room.' }],
        details: null,
      };
    },
  };
}
