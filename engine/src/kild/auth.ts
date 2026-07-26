import fs from 'node:fs';
import path from 'node:path';
// ModelRuntime owns credential storage + OAuth refresh (pi-ai's mechanism underneath);
// resolving through it is exactly what the coding agent itself does.
import { ModelRuntime } from '@earendil-works/pi-coding-agent';
import { configureProvider } from '@flue/runtime';

type AuthEntry = { type?: string; key?: string; access?: string; expires?: number };
type AuthFile = Record<string, AuthEntry>;

/**
 * Bridge pi's stored credentials (`~/.pi/agent/auth.json`) into Flue's provider
 * config. This closes the one real auth gap from the first spike pass: Flue's
 * runtime does not read auth.json, so the user's Claude Max / ChatGPT OAuth
 * subscriptions didn't work. `ModelRuntime.getAuth` refreshes the token and
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
  // The runtime reads the same auth.json; created lazily only when OAuth entries exist.
  let runtime: ModelRuntime | undefined;
  for (const id of ['anthropic', 'openai-codex'] as const) {
    if (auth[id]?.type !== 'oauth') continue;
    runtime ??= await ModelRuntime.create();
    const apiKey = (await runtime.getAuth(id))?.auth.apiKey;
    if (apiKey) {
      configureProvider(id, { apiKey });
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
