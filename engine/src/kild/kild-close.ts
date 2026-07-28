import path from 'node:path';

import { configuredCloseHook, configuredMemoryDir } from './config.ts';
import { type HookAgentSpawn, type KildCloseEvent, runCloseHook } from './hooks.ts';
import type { Kild } from './kild-types.ts';
import { appendKildLog, collectLedgerFacts, kildTranscriptPath } from './memory.ts';
import { worktreePath } from './worktree.ts';

/**
 * The close lifecycle — what the engine does at the one moment a kild ENDS.
 *
 * Three things, and all three are mechanism: write the ledger entry from facts the engine
 * holds, hand those facts to whoever is listening, and run whatever config declared. The
 * engine has no opinion about what should be made of any of it — that is the whole point of
 * the seam (see `docs/onclose-hook.md`).
 *
 * Its own slice because the ordering is the interesting part and it is easy to get wrong: the
 * ledger is written from the worktree BEFORE anything can land or prune it, and the event
 * carries the ledger path, so the hook can read what was just written. A caller that had to
 * remember that ordering would eventually forget it.
 *
 * **Every failure here is logged loud and swallowed.** A stop must never fail because a
 * ledger file was read-only or a hook binary was missing — the kild is already over, and
 * refusing to finish tearing it down would leave a worse state than a missing log line. This
 * is the one boundary where swallowing is the right call, which is why it is stated once,
 * here, rather than at each call site.
 */

/** What the close needs from the engine: somewhere to announce the event, and a way to start
 *  the agent a hook may declare. Injected so this slice owns no process management and no
 *  subscriber list of its own. */
export interface CloseDeps {
  announce(event: KildCloseEvent): void;
  spawnHookAgent(spec: HookAgentSpawn, prompt: string): void;
}

const errText = (err: unknown): string => (err instanceof Error ? err.message : String(err));

export async function closeKild(kild: Kild, deps: CloseDeps): Promise<void> {
  // The kild's own tree if it had one: the ledger's facts are about the code that was
  // written, and after this the tree can be landed or pruned and the answers change.
  const dir = kild.worktree ? worktreePath(kild.worktree) : kild.cwd;
  const memoryDir = await configuredMemoryDir(kild.cwd);
  const ledgerPath = path.join(memoryDir, 'LOG.md');

  try {
    appendKildLog(
      kild,
      memoryDir,
      await collectLedgerFacts(dir, kild.base, kild.landedSha, kild.landed),
    );
  } catch (err) {
    console.error(`kild: ledger append failed for '${kild.name}': ${errText(err)}`);
  }

  const event: KildCloseEvent = {
    kildId: kild.id,
    name: kild.name,
    cwd: kild.cwd,
    worktree: kild.worktree,
    base: kild.base,
    transcriptPath: kildTranscriptPath(kild.id),
    ledgerPath,
  };
  deps.announce(event);

  try {
    const hook = await configuredCloseHook(kild.cwd);
    if (!hook) return;
    // Not awaited: a hook is someone else's work, and a slow one must not hold a stop open.
    // Its own failures are already captured inside.
    void runCloseHook(hook, event, deps.spawnHookAgent).catch((err) => {
      console.error(`kild: onClose hook failed for '${kild.name}': ${errText(err)}`);
    });
  } catch (err) {
    console.error(`kild: onClose hook failed for '${kild.name}': ${errText(err)}`);
  }
}
