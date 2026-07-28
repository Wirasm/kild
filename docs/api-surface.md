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

## 6. `task` on spawn, and `invitedBy` on the wire

> Superseded in part by §7: the REST route below is unchanged, but the agent-facing `spawn`
> tool it describes no longer exists — an agent creates by addressing. `invitedBy` and the
> derived-spawner rule are unaffected and are what §7 is built on.

**Decision: `spawn` takes an optional `task`, delivered as the new agent's first message.**
Not a new concept and not a stored field — it fuses `spawn` + `send` exactly as `kickoff`
already does for `POST /api/kilds`, because every real delegation was spawn-then-send and a
spawn on its own produces an agent that sits idle. One tool call instead of two, and the
mechanism an agent has to learn is unchanged: a task is a message.

**It is not stored on the agent.** The record of what an agent was asked to do is message
`seq n` on the kild log, which a later revision naturally follows. A copy on the roster would
be a second answer that goes stale the moment the spawner says "actually, do X instead" — and
the log already had the true one.

**Consequence, and the thing that had to be right:** the spawner is now a *message sender*.
`POST /api/kilds/:id/agents` used to take `invitedBy` from the request body, which was
harmless while it was only roster metadata. Once it becomes the `from` of a message it is a
forgery path, so the route stopped trusting it: the spawner is resolved from the caller's
credential like every other actor (§4), and a present `invitedBy` is a `409` naming that
field. An agent's own `spawn` control line carries no sender and cannot — the engine takes it
from the session the line arrived on. The agent says what it wants done; the engine says who
asked.

**`invitedBy` is exposed on the cheap roster view.** It was already recorded and already free
to read; omitting it just meant nobody could see the spawn edge. It is what tells five agents
running one persona apart by origin, and it is ground truth where inferring the same thing
from the log would be guesswork.

**Identification stays deliberately plain:** the spawner chooses the handle (mandatory,
unique), `invitedBy` says who spawned it, and the first message says what it was for. No
generated ids for the agent to juggle, no `task` index, no per-instance naming scheme in the
engine.

## 7. An agent has one verb: `send` creates what it addresses

**Decision: the agent-facing `spawn` tool is deleted.** Inside a kild, addressing a handle
nobody holds creates that agent and delivers the message to it. `persona` and `model` on
`send` say who a created recipient should be; the handle names the instance.

This is §6 taken to its conclusion. `task`-on-spawn fused the two calls but left two tools,
and the seam showed: `spawn` could do a send, `send` could not do a spawn, and an agent had
to know which to reach for. Spawning was never the goal — it is what you do *in order to*
say something — so the create step disappears into the verb that has the message. The
idle-just-created agent is now unreachable rather than warned about, and an agent learns two
tools (`send`, `stop`) instead of three.

**It is not an inference.** The handles come from the caller as data, never from the text.
What changed is what an unknown handle *means*, and only for a caller that may grow the
kild: the manager's `send` takes a `createMissing` spec, agents pass it, REST does not. At
the boundary an unknown recipient stays a rejection naming the roster — a typo from a client
must not quietly cost a process, and `POST /api/kilds/:id/agents` is right there for the
explicit act. Same reasoning for `kild send` and the pi extension's `kild_send`.

**Batch-validated, so it is atomic in the way that matters.** One call naming three new
handles validates all three personas before starting any; one bad name creates none of them.
A create that throws mid-batch rolls back its siblings.

**The rejection is the discovery path.** A persona that cannot be resolved comes back naming
the personas that exist *and* the current roster — which is the whole answer to "how does an
agent learn who is in its kild": progressive disclosure at the moment of use, not a roster
tool returning a snapshot that is stale before the turn ends. `kild show` / `kild ls` /
`GET /api/kilds` remain the roster view for humans and clients.

**Cost of the fusion, stated plainly:** an agent can now create processes by naming handles,
capped by `MAX_AGENTS` (8) and gated on the persona resolving. A hallucinated handle whose
name happens to match a persona file creates an agent. That is the accepted downside; the
alternative — a confirmation step — is ceremony the engine cannot enforce anyway.

## 8. Two things the reshape did NOT fold, and why

Asked while helm was porting, so the answers are recorded rather than left to look like
omissions. Neither was ever in the proposed surface above.

**`git/commits`, `git/files` and `git/diff` stay three routes.** They are three different
resources, not three names for one: the commit list, the per-file diff stats, and one file's
patch (which needs `?path=` and can 404 on a path git did not report). §1's rule is *one
address per object* — it is about two identifier schemes for a single object, not about
merging distinct resources behind a mode parameter, which would give one route three response
shapes.

**`/api/kilds/archive` stays its own route rather than `?state=archived`.** A stopped kild is
not a filter over live kilds: it has no git state, no agents to address and no worktree that
is guaranteed to exist, so a `?state=` union would return a shape where half the fields are
structurally absent. `?state=` filters what a listing already contains; the archive is a
different collection.

**But the archive is a LISTING, so it now obeys the listing rule: no log.** It used to carry
every archived kild's full message history — and the archive only grows, so "list my stopped
kilds" meant "send me every conversation I have ever had", which is exactly the cost the
cheap/costly split removed for live kilds. `GET /api/kilds/:id/messages` already serves an
archived kild's log. Same for the WS `{archivedKild}` frame: a subscribed client already
received every `{message}`, and one that was not reads `/messages`.

## 9. The project registry has no REST surface

`GET|POST /api/projects` are **deleted**. Nothing called them: the CLI reads and writes
`$KILD_HOME/projects.json` directly (`kild project ls|add|rm`), the pi extension passes a
registered NAME in a request body and lets the engine resolve it, and helm confirmed zero
references in its source while porting.

The registry itself is load-bearing and stays — it scopes the unscoped `GET /api/kilds` and
resolves `--project` / `project=` — but a loopback write route for a local file the operator
owns was a second way to do what `kild project add` already does. Cheap to reinstate if a UI
ever wants a project picker; there is no reason to carry it until one does.
