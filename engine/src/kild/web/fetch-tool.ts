import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';

import { fetchUrl } from './fetch.ts';

/**
 * The agent-facing `webfetch` tool. Plain HTTP + HTML→markdown (no headless browser,
 * no service, no key). Always available when web is enabled — it needs no backend.
 */
export function createWebFetchTool(): ToolDefinition {
  return {
    name: 'webfetch',
    label: 'Web Fetch',
    description:
      'Fetch a public http(s) URL and return its content as markdown (default) or raw ' +
      'html. Use this to read a page found via web_search.',
    promptSnippet: 'webfetch — read a url as markdown',
    parameters: Type.Object({
      url: Type.String({ description: 'The http(s) URL to fetch.' }),
      format: Type.Optional(
        Type.Union([Type.Literal('markdown'), Type.Literal('html')], {
          description: 'Output format (default markdown).',
        }),
      ),
    }),
    async execute(_toolCallId, params) {
      const { url, format } = params as { url: string; format?: 'markdown' | 'html' };
      const text = await fetchUrl(url, format ?? 'markdown');
      return { content: [{ type: 'text' as const, text }], details: { url } };
    },
  };
}
