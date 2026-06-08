<script lang="ts">
  import Topbar from "./Topbar.svelte";
  import Ledger from "./Ledger.svelte";
  import Composer from "./Composer.svelte";
  import RoomLog from "./RoomLog.svelte";
  import Dropdown from "./Dropdown.svelte";
  import type { Agent, Participant, Room } from "../types";

  interface Props {
    room: Room;
    activeParticipant: Participant | null;
    agents: Agent[];
    input: string;
    onSelectParticipant: (name: string) => void;
    onInvite: (agent: string) => void;
    onClose: () => void;
    onHalt: () => void;
    onOpenWorktree: (path: string) => void;
    onSend: () => void;
  }
  let {
    room,
    activeParticipant,
    agents,
    input = $bindable(),
    onSelectParticipant,
    onInvite,
    onClose,
    onHalt,
    onOpenWorktree,
    onSend,
  }: Props = $props();

  let solo = $derived(room.participants.length === 1);
  let inviting = $state(false);
  let inviteAgent = $state("default");
  let invitable = $derived(
    agents.map((a) => a.name).filter((n) => !room.participants.some((p) => p.name === n)),
  );
  $effect(() => {
    if (inviting && invitable.length && !invitable.includes(inviteAgent)) inviteAgent = invitable[0];
  });

  function doInvite() {
    if (inviteAgent && invitable.includes(inviteAgent)) onInvite(inviteAgent);
    inviting = false;
  }
</script>

<Topbar {room} participant={activeParticipant} {onClose} {onHalt} {onOpenWorktree} />

{#if room.participants.length > 1}
  <div class="roster">
    {#each room.participants as p (p.name)}
      <button
        class="chip"
        class:active={p.name === activeParticipant?.name}
        onclick={() => onSelectParticipant(p.name)}
      >
        <span class="dot" class:running={p.running}></span>@{p.name}
      </button>
    {/each}
    {#if !room.archived}
      <div class="invite">
        {#if inviting}
          <Dropdown bind:value={inviteAgent} options={invitable} label="Agent" />
          <button class="mini" onclick={doInvite}>add</button>
          <button class="mini" onclick={() => (inviting = false)}>✕</button>
        {:else}
          <button class="mini" onclick={() => (inviting = true)} disabled={invitable.length === 0}>
            + invite
          </button>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<div class="body">
  {#if room.archived}
    <!-- Read-only history: per-participant transcripts aren't persisted, so show the
         shared room log (the conversation record) regardless of participant count. -->
    <div class="archived-log"><RoomLog log={room.log} /></div>
  {:else if solo}
    {#if activeParticipant}
      <Ledger
        items={activeParticipant.items}
        running={activeParticipant.running}
        agentName={activeParticipant.name}
      />
    {/if}
  {:else}
    <div class="split">
      <div class="pane">
        <div class="pane-label">room</div>
        <div class="pane-body"><RoomLog log={room.log} /></div>
      </div>
      {#if activeParticipant}
        <div class="pane">
          <div class="pane-label">@{activeParticipant.name} · working detail</div>
          <div class="pane-body">
            <Ledger
              items={activeParticipant.items}
              running={activeParticipant.running}
              agentName={activeParticipant.name}
            />
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<Composer
  bind:input={input}
  status={room.status}
  running={solo ? (activeParticipant?.running ?? false) : false}
  {onSend}
  placeholder={activeParticipant
    ? `Message @${activeParticipant.name}…  (@name to address others)`
    : "Message the room…"}
/>

<style>
  .roster {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--border-subtle);
    flex-wrap: wrap;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-subtle);
    border-radius: 14px;
    padding: 3px 10px;
    font-size: 12px;
    cursor: pointer;
  }
  .chip.active {
    border-color: var(--ice);
    color: var(--ice);
  }
  .chip .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--border);
  }
  .chip .dot.running {
    background: var(--aurora);
  }
  .invite {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: auto;
  }
  .mini {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-muted);
    border-radius: 6px;
    padding: 3px 8px;
    font-size: 11px;
    cursor: pointer;
  }
  .mini:hover:not(:disabled) {
    color: var(--ice);
    border-color: var(--ice);
  }
  .mini:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .archived-log {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .split {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
  }
  .pane {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--border-subtle);
  }
  .pane:last-child {
    border-right: none;
  }
  .pane-label {
    padding: 6px 14px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle);
  }
  .pane-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
</style>
