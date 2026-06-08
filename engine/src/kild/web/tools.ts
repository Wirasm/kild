import type { ToolDefinition } from '@earendil-works/pi-coding-agent';

import { webEnabled } from '../config.ts';
import { createWebFetchTool } from './fetch-tool.ts';
import { webSearchProvider } from './search.ts';
import { createWebSearchTool } from './search-tool.ts';

/** The web tools an agent session should get, in any context (engine worker or the
 *  CLI's in-process fallback). `webfetch` is always present when web is enabled (it's
 *  in-process, no backend); `web_search` is added only when a provider is configured. */
export function webTools(): ToolDefinition[] {
  if (!webEnabled()) return [];
  const provider = webSearchProvider();
  return [createWebFetchTool(), ...(provider ? [createWebSearchTool(provider)] : [])];
}
