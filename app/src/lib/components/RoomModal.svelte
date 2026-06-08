<script lang="ts">
  import Dropdown from "./Dropdown.svelte";
  import type { Agent } from "../types";

  type RunMode = "main" | "new" | "existing";

  interface Props {
    isOpen: boolean;
    agents: Agent[];
    selectedAgents: string[];
    model: string;
    models: string[];
    projectName: string;
    worktrees: string[];
    runMode: RunMode;
    worktreeName: string;
    existingWorktree: string;
    onStart: () => void;
    onClose: () => void;
  }

  let {
    isOpen = $bindable(),
    agents,
    selectedAgents = $bindable(),
    model = $bindable(),
    models,
    projectName,
    worktrees,
    runMode = $bindable(),
    worktreeName = $bindable(),
    existingWorktree = $bindable(),
    onStart,
    onClose,
  }: Props = $props();

  let dialogEl: HTMLDialogElement | undefined = $state();

  $effect(() => {
    if (dialogEl) {
      if (isOpen) {
        if (!dialogEl.open) dialogEl.showModal();
      } else if (dialogEl.open) {
        dialogEl.close();
      }
    }
  });

  function toggle(name: string) {
    selectedAgents = selectedAgents.includes(name)
      ? selectedAgents.filter((x) => x !== name)
      : [...selectedAgents, name];
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<dialog
  bind:this={dialogEl}
  onclose={onClose}
  onclick={(e) => {
    if (e.target === dialogEl) onClose();
  }}
>
  <div class="modal-card">
    <header class="modal-header">
      <h3>New Room</h3>
      <button class="close-btn" type="button" onclick={onClose}>✕</button>
    </header>

    <div class="modal-body">
      <p class="description">
        A room of one or more agents inside <strong>{projectName}</strong>. Pick one for a single
        agent, or several to start a room.
      </p>

      <div class="form-group">
        <span class="field-label">Participants ({selectedAgents.length})</span>
        <div class="agent-list">
          {#each agents as a (a.name)}
            <button
              type="button"
              class="agent"
              class:on={selectedAgents.includes(a.name)}
              onclick={() => toggle(a.name)}
            >
              <span class="check">{selectedAgents.includes(a.name) ? "✓" : ""}</span>
              <span class="a-name">{a.name}</span>
              {#if a.description}<span class="a-desc">{a.description}</span>{/if}
            </button>
          {/each}
        </div>
        <span class="hint">
          {#if selectedAgents.length <= 1}Single agent.{:else}Room of {selectedAgents.length} — lead
            is <code>@{selectedAgents[0]}</code>.{/if}
        </span>
      </div>

      <div class="form-group">
        <span class="field-label">Model</span>
        <Dropdown bind:value={model} options={models} label="Model" />
      </div>

      <div class="form-group">
        <span class="field-label">Run in</span>
        <div class="run-toggle">
          <button type="button" class:selected={runMode === "main"} onclick={() => (runMode = "main")}>
            main checkout
          </button>
          <button type="button" class:selected={runMode === "new"} onclick={() => (runMode = "new")}>
            new worktree
          </button>
          <button
            type="button"
            class:selected={runMode === "existing"}
            disabled={worktrees.length === 0}
            title={worktrees.length === 0 ? "No existing worktrees" : ""}
            onclick={() => (runMode = "existing")}
          >
            existing
          </button>
        </div>

        {#if runMode === "new"}
          <input class="wt-input" type="text" placeholder="branch name, e.g. fix-auth" bind:value={worktreeName} />
          <span class="hint">Shared on <code>kild/{worktreeName || "<name>"}</code> (all agents attach).</span>
        {:else if runMode === "existing"}
          <Dropdown bind:value={existingWorktree} options={worktrees} label="Worktree" />
          <span class="hint">Attach the room to <code>kild/{existingWorktree}</code> (shared).</span>
        {:else}
          <span class="hint">Runs in the project's main checkout — no isolation.</span>
        {/if}
      </div>
    </div>

    <footer class="modal-footer">
      <button class="secondary" type="button" onclick={onClose}>Cancel</button>
      <button class="primary" type="button" disabled={selectedAgents.length === 0} onclick={onStart}>
        {selectedAgents.length <= 1 ? "Start Agent" : "Start Room"}
      </button>
    </footer>
  </div>
</dialog>

<style>
  dialog {
    background: transparent;
    border: none;
    padding: 0;
    margin: auto;
    max-width: 460px;
    width: 90%;
    outline: none;
  }
  dialog::backdrop {
    background: rgba(5, 5, 7, 0.6);
    backdrop-filter: blur(4px);
  }
  .modal-card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: var(--shadow-modal);
    display: flex;
    flex-direction: column;
    overflow: visible;
  }
  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 20px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--obsidian);
  }
  .modal-header h3 {
    margin: 0;
    color: var(--text-bright);
    font-size: 15px;
    font-weight: 600;
  }
  .close-btn {
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 14px;
    padding: 4px;
  }
  .close-btn:hover {
    color: var(--ember);
  }
  .modal-body {
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 20px;
    overflow: visible;
  }
  .description {
    margin: 0;
    font-size: 12px;
    color: var(--text-subtle);
    line-height: 1.4;
  }
  .description strong {
    color: var(--ice);
  }
  .form-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .field-label {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
    font-weight: 600;
  }
  .agent-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 180px;
    overflow-y: auto;
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    padding: 4px;
  }
  .agent {
    display: flex;
    align-items: baseline;
    gap: 8px;
    background: transparent;
    border: none;
    color: var(--text-subtle);
    padding: 6px 8px;
    border-radius: 6px;
    cursor: pointer;
    text-align: left;
    font-size: 13px;
  }
  .agent:hover {
    background: rgba(255, 255, 255, 0.03);
  }
  .agent.on {
    color: var(--ice);
  }
  .agent .check {
    width: 12px;
    color: var(--ice);
  }
  .agent .a-desc {
    color: var(--text-muted);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .run-toggle {
    display: flex;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    padding: 3px;
    gap: 2px;
  }
  .run-toggle button {
    flex: 1;
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 11px;
    padding: 5px 8px;
    border-radius: 6px;
    cursor: pointer;
  }
  .run-toggle button.selected {
    background: var(--surface);
    color: var(--ice);
    box-shadow: var(--shadow-subtle);
  }
  .run-toggle button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .wt-input {
    background: var(--surface);
    color: var(--text-bright);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 7px 10px;
    font: inherit;
    font-size: 13px;
  }
  .wt-input:focus {
    outline: none;
    border-color: var(--border-focus);
  }
  .hint {
    font-size: 11px;
    color: var(--text-muted);
  }
  .hint code,
  code {
    font-family: var(--mono);
    color: var(--ice);
  }
  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding: 14px 20px;
    border-top: 1px solid var(--border-subtle);
    background: var(--obsidian);
  }
  .modal-footer button {
    padding: 8px 16px;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    border: 1px solid transparent;
  }
  .modal-footer button.primary {
    background: var(--ice);
    color: var(--void);
  }
  .modal-footer button.primary:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .modal-footer button.secondary {
    background: transparent;
    border-color: var(--border);
    color: var(--text-subtle);
  }
</style>
