import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';

import { postRoom } from './engine-client.ts';

export function createPostRoomTool(): ToolDefinition {
  return {
    name: 'post_room',
    label: 'Post Room',
    description:
      'Post a message to a live kild room through the engine REST API. The engine ' +
      'derives actor identity from the live session when present.',
    promptSnippet: 'post_room — send a message to a live room by id',
    parameters: Type.Object({
      roomId: Type.String({ description: 'The live room id.' }),
      text: Type.String({ description: 'The message body to post.' }),
    }),
    async execute(_toolCallId, params) {
      const { roomId, text } = params as { roomId: string; text: string };
      const { message } = await postRoom(roomId, text, process.env.KILD_SESSION_ID);
      return {
        content: [{ type: 'text' as const, text: message }],
        details: null,
      };
    },
  };
}
