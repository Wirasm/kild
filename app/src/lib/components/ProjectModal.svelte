<script lang="ts">
  interface Props {
    isOpen: boolean;
    newName: string;
    newPath: string;
    addError: string | null;
    onAdd: () => void;
    onClose: () => void;
  }

  let {
    isOpen = $bindable(),
    newName = $bindable(),
    newPath = $bindable(),
    addError,
    onAdd,
    onClose,
  }: Props = $props();

  let dialogEl: HTMLDialogElement | undefined = $state();

  // Handle open/close state reactively on dialogEl
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
      onAdd();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<dialog
  bind:this={dialogEl}
  onclose={onClose}
  onclick={(e) => {
    // Close modal if user clicks the backdrop overlay
    if (e.target === dialogEl) {
      onClose();
    }
  }}
>
  <div class="modal-card">
    <header class="modal-header">
      <h3>Add Project</h3>
      <button class="close-btn" type="button" onclick={onClose}>✕</button>
    </header>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="modal-body" onkeydown={handleKeydown}>
      <p class="description">Register a local directory as a project to run AI sessions in it.</p>
      
      <div class="form-group">
        <label for="proj-name">project name</label>
        <input
          id="proj-name"
          bind:value={newName}
          placeholder="e.g. auth-service"
          autocomplete="off"
        />
      </div>

      <div class="form-group">
        <label for="proj-path">project path</label>
        <input
          id="proj-path"
          bind:value={newPath}
          placeholder="e.g. ~/projects/auth-service"
          autocomplete="off"
        />
      </div>

      {#if addError}
        <div class="error-banner">{addError}</div>
      {/if}
    </div>

    <footer class="modal-footer">
      <button class="secondary" type="button" onclick={onClose}>Cancel</button>
      <button class="primary" type="button" onclick={onAdd}>Add Project</button>
    </footer>
  </div>
</dialog>

<style>
  dialog {
    background: transparent;
    border: none;
    padding: 0;
    margin: auto;
    max-width: 500px;
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
    overflow: hidden;
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
    gap: 16px;
  }

  .description {
    margin: 0;
    font-size: 12px;
    color: var(--text-subtle);
    line-height: 1.4;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .form-group label {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
    font-weight: 600;
  }

  .form-group input {
    background: var(--void);
    color: var(--text-bright);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px 12px;
    font: inherit;
    font-size: 13px;
    outline: none;
    transition: border-color 0.2s ease;
  }
  .form-group input:focus {
    border-color: var(--ice);
    box-shadow: var(--glow-ice);
  }

  .error-banner {
    background: rgba(224, 108, 117, 0.1);
    border: 1px solid var(--ember);
    color: var(--ember);
    border-radius: 6px;
    padding: 8px 12px;
    font-size: 12px;
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
