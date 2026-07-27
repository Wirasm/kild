# API surface — decisions for the reshape

Settled calls from the rev-2 review, to be implemented in the reshape step (after the router
commit, before helm ports). Recorded here so the reasoning is not re-litigated.

The rename was deliberately 1:1 — twenty-five endpoints in, twenty-five out. That is correct for
crossing a pin boundary, but it means the demolition has not yet reached the surface clients
touch. This is that work.

## 1. An agent is addressed by handle

**Decision: `/api/kilds/:id/agents/:handle` is the one address.** The parallel
`/api/agents/:id/*` family is deleted; the agent manager stays as internal mechanism and stops
being a public resource.

This reverses an earlier call in `helm-migration.md`, which said `id` should win because handle
is "an addressing concept, not a routing key." That distinction is exactly what produces two
identifier schemes for one object — the same shape the reframe exists to delete. One address.

Verified as safe: handles are unique within a kild for its lifetime. An agent is not removed
from the roster when stopped, so a handle never rebinds to a second agent.

*Correction to the review's reasoning:* it argues a session id is "a pi implementation detail
that leaks the runtime." It is not — the engine's id is `randomUUID()` assigned by the manager;
`piSessionId` is the separate pi-level handle. The conclusion stands, the premise does not.

**Consequence:** prompting an owned agent *is* sending it a message.
`POST /api/kilds/:id/messages` absorbs `POST /api/agents/:id/prompt` — one delivery path, not
two.

## 2. Disposal: `DELETE /api/kilds/:id`, guarded on authored commits

`land` handles success. `stop` halts agents and keeps the tree. **Nothing handles a kild nobody
ever lands** — which in a triage workflow is most of them.

Measured on the engine's own machine: 116 kild worktrees, **zero** reclaimable by
`kild worktree prune` across six projects. `pruneMergedWorktrees` removes only trees whose
branch merged into the default branch *and* which are clean, so abandoned work is structurally
unreclaimable forever. Separately, 27 of 27 sibling agent worktrees refused removal as dirty —
15 carrying an identical six-file provisioning diff written into every tree at creation, before
any agent ran.

**The guard is authored-vs-provisioned, not clean-vs-dirty.** Refuse deletion when the branch
carries commits not reachable from base — that is real work, and the surviving branch is the
safety net. Ignore uncommitted working-tree state: litter is not work, and treating it as work
is precisely what produced 116 permanent trees.

This supersedes the softer "merged + untracked-only is reclaimable" line in
`worktree-disposal.md`. Guard on commits; the working tree is not evidence of anything.

## 3. Fold `/api/worktrees` into `/api/kilds`

A kild *is* a worktree; two resource families owning one object contradicts the sentence the
model is built on. `DELETE /api/worktrees` already implements refusal logic — the disposal verb
existed all along, attached to the wrong noun.

- `GET /api/kilds` lists them, `?state=` filters
- `DELETE /api/kilds/:id` inherits the refusal logic (re-guarded per §2)
- prune becomes a filter over the collection, not its own verb

**Required for this to work:** `GET /api/kilds` must enumerate `kild/*` worktrees **from git**,
not only from the in-memory registry. A tree whose kild record is gone (older engine, lost
state) has no id to address, and folding the worktree family would otherwise strand it
permanently — the exact failure being fixed. Orphans surface as kilds with no agents and no log.

## 4. Attribution comes from a credential minted at `attach`

`attach` is already the moment a harness becomes addressable. Make it the moment it gets an
identity: it returns a bearer token, and `from` is resolved from that token rather than sent in
the body.

*Correction:* the review states `from` is still caller-supplied. It is not — `rest-attribution.ts`
rejects it outright (*"from is not allowed; actor identity is engine-derived"*) and resolves the
actor from the caller's session. The `from` field in the migration guide's message shape refers
to the *stored message*, not a request field.

**The underlying gap is real, though, and the credential closes it.** The seat is currently
unenforceable: `POST /api/kilds/:id/stop` sits on an unauthenticated loopback port that every
supervised agent can reach. "Hard authority cannot be delegated to something running inside what
it may need to kill" is aspirational until something checks.

The seat bit is mintable **only for an agent the engine did not spawn**. An owned agent cannot
obtain one, because the engine mints its token — which makes the model's own rule checkable
rather than declared.

This is attribution, not security: it is loopback, and any local process can read the token. It
makes a handle mean something and turns the seat into a capability. That is the right amount of
machinery here, and no more.

**Sequencing:** land this with the router commit. `send` is where `from` is resolved, so
retrofitting attribution afterwards means touching every route twice.

## 5. Two cheap wins

**Split cheap identity from costly status.** `GET /api/kilds` today computes git status for
every kild on every call, forcing a slow cadence on a query that is mostly identity. Split:
`/api/kilds` returns identity, tree edges, agents and `idle` for free; `/api/kilds/status`
returns ahead/behind, dirty and cost on its own cadence.

**Correction:** an earlier draft of this section said `/api/kilds/status` would return
`collidesWith`. It does not, and no server route ever did. Collisions are a *cross-kild
derivation* over `changedFiles` — `computeCollisions()` in `kilds-status.ts`, client-side.
What `/status` serves is the `changedFiles[]` that makes the derivation possible. (Same
error the migration guide made; see the `collidesWith` note there.)

**Give messages a monotonic `seq`.** `ts` is `Date.now()` and can go backwards, so it cannot be
a cursor. Add `seq` and let clients page with `?since=<seq>`.

## Not adopted yet

Deleting `GET|POST /api/projects` is proposed on the grounds that no client uses them. Confirmed
for the CLI — it calls `loadProjects()` directly rather than over REST. Whether helm needs them
is helm's call; the registry itself stays either way, since `--project` name resolution depends
on it.
