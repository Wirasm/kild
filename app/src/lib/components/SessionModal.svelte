<script lang="ts">
  import Dropdown from "./Dropdown.svelte";
  import type { Agent } from "../types";

  type RunMode = "main" | "new" | "existing";

  interface Props {
    isOpen: boolean;
    agents: Agent[];
    agentName: string;
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
    agentName = $bindable(),
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
        if (!dialogEl.open) {
          dialogEl.showModal();
        }
      } else {
        if (dialogEl.open) {
          dialogEl.close();
        }
      }
    }
  });

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      onStart();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<dialog
  bind:this={dialogEl}
  onclose={onClose}
  onclick={(e) => {
    if (e.target === dialogEl) {
      onClose();
    }
  }}
>
  <div class="modal-card">
    <header class="modal-header">
      <h3>New Session</h3>
      <button class="close-btn" type="button" onclick={onClose}>✕</button>
    </header>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="modal-body" onkeydown={handleKeydown}>
      <p class="description">Start an AI workspace session inside the project <strong>{projectName}</strong>.</p>
      
      <div class="form-group">
        <span class="field-label">Select Agent</span>
        <Dropdown bind:value={agentName} options={agents.map((a) => a.name)} label="Agent" />
      </div>

      <div class="form-group">
        <span class="field-label">Select Model</span>
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
          <input
            class="wt-input"
            type="text"
            placeholder="branch name, e.g. fix-auth"
            bind:value={worktreeName}
          />
          <span class="hint">Isolated on <code>kild/{worktreeName || "<name>"}</code>.</span>
        {:else if runMode === "existing"}
          <Dropdown bind:value={existingWorktree} options={worktrees} label="Worktree" />
          <span class="hint">Attach to the existing <code>kild/{existingWorktree}</code> tree (shared).</span>
        {:else}
          <span class="hint">Runs in the project's main checkout — no isolation.</span>
        {/if}
      </div>
    </div>

    <footer class="modal-footer">
      <button class="secondary" type="button" onclick={onClose}>Cancel</button>
      <button class="primary" type="button" onclick={onStart}>Start Session</button>
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
    opacity: 0;
    transform: scale(0.95);
    transition: 
      opacity 0.2s ease-out,
      transform 0.2s cubic-bezier(0.34, 1.56, 0.64, 1),
      display 0.2s allow-discrete,
      overlay 0.2s allow-discrete;
  }

  dialog[open] {
    opacity: 1;
    transform: scale(1);
  }

  @starting-style {
    dialog[open] {
      opacity: 0;
      transform: scale(0.95);
    }
  }

  dialog::backdrop {
    background: rgba(5, 5, 7, 0.6);
    backdrop-filter: blur(4px);
    opacity: 0;
    transition: 
      opacity 0.2s ease-out,
      display 0.2s allow-discrete,
      overlay 0.2s allow-discrete;
  }

  dialog[open]::backdrop {
    opacity: 1;
  }

  @starting-style {
    dialog[open]::backdrop {
      opacity: 0;
    }
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
    line-height: 1;
    transition: color 0.15s ease;
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
    overflow: visible;
  }

  .form-group .field-label {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
    font-weight: 600;
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
    font-weight: 500;
    padding: 5px 8px;
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .run-toggle button:hover:not(.selected):not(:disabled) {
    color: var(--text-bright);
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
    box-shadow: var(--glow-ice);
  }

  .hint {
    font-size: 11px;
    color: var(--text-muted);
  }
  .hint code {
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
  .modal-footer button.primary:hover {
    background: var(--ice-dim);
    box-shadow: var(--glow-ice);
  }

  .modal-footer button.secondary {
    background: transparent;
    border-color: var(--border);
    color: var(--text-subtle);
  }
  .modal-footer button.secondary:hover {
    background: rgba(255, 255, 255, 0.03);
    color: var(--text-bright);
    border-color: var(--border-focus);
  }
</style>
