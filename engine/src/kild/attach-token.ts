import { randomBytes } from 'node:crypto';

import type { KildActionSuccess } from './kild-types.ts';

/**
 * The credential `attach` mints, and the whole of what it means.
 *
 * An attached agent had handles for *receiving* and none for *sending*: it has no kild
 * session by definition, so every attached sender was attributed to the unattributed
 * label and two harnesses in one kild were indistinguishable on the log. `attach` is
 * already the moment a harness becomes addressable; this makes it the moment it gets an
 * identity.
 *
 * **This is attribution, not authorization.** The token says who a caller *is*, and
 * nothing checks whether it may do anything: the engine is a loopback, single-operator
 * port, so any local process — including every agent the engine spawned — can read the
 * token out of the environment or the response it was handed. Gating destructive verbs
 * (`stop`, `DELETE`) on a seat bit here would break the CLI, curl and helm, which present
 * no credential, and would buy ceremony rather than a boundary. Enforcement becomes worth
 * building the moment the engine stops being loopback-only or the token stops being
 * readable by the processes it is meant to constrain (a real secret store, or a credential
 * the operator holds and agents never see) — at which point the seat is a bit ON this
 * identity, refused for any agent the engine spawned, and the refusal is a 403 rather than
 * an attribution fallback.
 */

/** The (kild, handle) pair a token stands for. A token means exactly this and nothing
 *  more — no rank, no capability, no scope beyond the one kild. */
export interface AttachIdentity {
  kildId: string;
  handle: string;
}

/** `attach`'s answer: the message, plus the credential the harness sends back to be
 *  attributed. Lives beside the token because it is the only thing that carries one. */
export interface AttachSuccess extends KildActionSuccess {
  /** Bearer token for this (kild, handle) — send it as `Authorization: Bearer <token>`. */
  token: string;
}

/** 32 bytes of CSPRNG as base64url (43 chars). Deliberately opaque and *empty*: the kild
 *  and handle live only in the map, so a token cannot be parsed, forged from a kild id, or
 *  guessed from another kild's token. */
const TOKEN_BYTES = 32;

/**
 * Live attach credentials, held in memory beside the live kilds.
 *
 * Nothing here is persisted and nothing expires. A token is minted when a handle attaches
 * and forgotten when its kild stops ({@link forget}) — the kild is the lifetime, so there
 * is no reaper, no TTL, and no file that can outlive the thing it names. An engine restart
 * takes every token with it, which is correct: the kilds are gone too.
 */
export class AttachTokens {
  private readonly identities = new Map<string, AttachIdentity>();
  /** kild id → handle → token, so a kild's tokens can be dropped in one go. */
  private readonly minted = new Map<string, Map<string, string>>();

  /**
   * The token for one (kild, handle): minted on first attach and **stable** thereafter.
   *
   * Re-attaching is idempotent, and rotating here would quietly break that. A hook that
   * calls `attach` on every session start would invalidate the token the running session
   * already holds, and a second process claiming the same handle would silently revoke the
   * first one's identity — an identity takeover dressed as a no-op. Returning the same
   * token instead means a harness that lost its token can simply re-attach and be handed it
   * back. The cost is that a leaked token cannot be rotated; on a loopback port where every
   * local process can read it anyway, that is not a cost worth paying for.
   */
  mint(kildId: string, handle: string): string {
    const forKild = this.minted.get(kildId) ?? new Map<string, string>();
    this.minted.set(kildId, forKild);
    const existing = forKild.get(handle);
    if (existing) return existing;
    const token = randomBytes(TOKEN_BYTES).toString('base64url');
    forKild.set(handle, token);
    this.identities.set(token, { kildId, handle });
    return token;
  }

  /** Who the bearer of this token is, or undefined when nothing minted it. Undefined is
   *  never "unattributed" — the caller rejects, because a credential the engine cannot
   *  place is a mistake to report, not an identity to downgrade. */
  identify(token: string): AttachIdentity | undefined {
    return this.identities.get(token);
  }

  /** Drop every token for a kild. Called when the kild stops: tokens die with it. */
  forget(kildId: string): void {
    for (const token of this.minted.get(kildId)?.values() ?? []) this.identities.delete(token);
    this.minted.delete(kildId);
  }
}
