import { execFile as execFileCb } from 'node:child_process';
import path from 'node:path';
import { promisify } from 'node:util';

import type { SandboxFactory } from '@flue/runtime';
import { local } from '@flue/runtime/node';

/**
 * Flue-promotable: a git-worktree-backed {@link SandboxFactory}.
 *
 * This module is the upstream contribution — it is INTENTIONALLY self-contained
 * (its own ~15 lines of `execFile git worktree`, no `@kild/*` imports, no `kild/`
 * naming convention) so it can be lifted into Flue's sandbox abstraction verbatim.
 * Do not import kild policy here.
 *
 * Flue exposes no end-of-life hook on `SessionEnv` (no `dispose`/`close`) and never
 * tears a sandbox down, so a resource-owning sandbox like this cannot auto-remove
 * its worktree. We therefore return BOTH the factory and a caller-managed
 * `cleanup()` — matching Flue's existing model, where the Daytona example has the
 * caller create AND destroy the container.
 */
const execFile = promisify(execFileCb);

export interface WorktreeSandboxOptions {
  /** Path to the git repository the worktree is added from. */
  repo: string;
  /** The branch the worktree checks out (force-created with `-B`). */
  branch: string;
  /** Directory under which the worktree is placed (`<root>/<branch>`). */
  root: string;
}

export interface WorktreeSandbox {
  sandbox: SandboxFactory;
  /** Remove the worktree from disk. Caller-managed — Flue has no teardown hook. */
  cleanup(): Promise<void>;
}

/** A {@link SandboxFactory} whose `createSessionEnv` first materializes a git
 *  worktree, then runs the session's shell/fs inside it via Flue's `local()`. */
export function worktree(opts: WorktreeSandboxOptions): WorktreeSandbox {
  const wtPath = path.join(opts.root, opts.branch.replace(/\//g, '-'));

  const sandbox: SandboxFactory = {
    async createSessionEnv({ id }) {
      // `git worktree add -B` force-creates the branch + checkout. Idempotent enough
      // for a demo; the kild session path uses kild's own create-or-attach policy.
      await execFile('git', ['-C', opts.repo, 'worktree', 'remove', '--force', wtPath]).catch(
        () => {},
      );
      await execFile('git', ['-C', opts.repo, 'worktree', 'add', '-B', opts.branch, wtPath]);
      // Delegate the actual filesystem/exec env to Flue's local sandbox, rooted in
      // the worktree. (Flue's `node` entry exports `local`, not `createLocalSessionEnv`.)
      return local({ cwd: wtPath }).createSessionEnv({ id });
    },
  };

  return {
    sandbox,
    // Caller-managed teardown. Errors propagate — a failed cleanup leaves a worktree
    // on disk and the caller must know (Flue offers no teardown hook to fall back on).
    cleanup: async () => {
      await execFile('git', ['-C', opts.repo, 'worktree', 'remove', '--force', wtPath]);
    },
  };
}
