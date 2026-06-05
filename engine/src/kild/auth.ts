import fs from 'node:fs';
import path from 'node:path';
// pi-ai owns the OAuth mechanism; this is the bridge the coding-agent uses too.
import { getOAuthApiKey } from '@earendil-works/pi-ai/oauth';
import { configureProvider } from '@flue/runtime';

type AuthEntry = { type?: string; key?: string; access?: string; expires?: number };
type AuthFile = Record<string, AuthEntry>;

/**
 * Bridge pi's stored credentials (`~/.pi/agent/auth.json`) into Flue's provider
 * config. This closes the one real auth gap from the first spike pass: Flue's
 * runtime does not read auth.json, so the user's Claude Max / ChatGPT OAuth
 * subscriptions didn't work. pi-ai's `getOAuthApiKey` refreshes the token and
 * returns it; the anthropic/openai-codex providers detect an OAuth token (an
 * `sk-ant-oat…` shaped key) and switch to Bearer + the right beta headers
 * automatically — so passing it as `apiKey` is all that's needed.
 *
 * Returns the providers it wired up.
 */
export async function bridgePiAuth(): Promise<string[]> {
  const authPath = path.join(process.env.HOME ?? '', '.pi/agent/auth.json');
  let auth: AuthFile;
  try {
    auth = JSON.parse(fs.readFileSync(authPath, 'utf8')) as AuthFile;
  } catch {
    return [];
  }

  const bridged: string[] = [];

  // OAuth subscriptions (Claude Pro/Max, ChatGPT Codex): refresh + pass token.
  for (const id of ['anthropic', 'openai-codex'] as const) {
    if (auth[id]?.type !== 'oauth') continue;
    const result = await getOAuthApiKey(id, auth as never);
    if (result) {
      configureProvider(id, { apiKey: result.apiKey });
      bridged.push(id);
    }
  }

  // Raw API-key providers (minimax, …): expose as the env var pi-ai reads.
  const minimax = auth.minimax;
  if (minimax?.key) {
    process.env.MINIMAX_API_KEY ??= minimax.key;
    bridged.push('minimax');
  }

  return bridged;
}
