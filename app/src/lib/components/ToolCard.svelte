<script lang="ts">
  interface Props {
    name: string;
    args: string;
    status: "running" | "ok" | "error";
  }

  let { name, args, status }: Props = $props();
  let isOpen = $state(false);
</script>

<div class="tool {status}" class:open={isOpen}>
  <button class="tool-header" onclick={() => isOpen = !isOpen}>
    <span class="tool-name">🔧 {name}</span>
    <span class="tool-status-mark" class:running={status === "running"} class:ok={status === "ok"} class:error={status === "error"}>
      {#if status === "running"}
        running…
      {:else}
        {status === "ok" ? "completed" : "failed"}
      {/if}
    </span>
    <span class="chevron" class:open={isOpen}>▾</span>
    <span class="tool-mark">{status === "running" ? "…" : status === "ok" ? "✓" : "✗"}</span>
  </button>
  
  <div class="tool-details-wrapper">
    <div class="tool-details">
      <div class="detail-row">
        <span class="label">arguments:</span>
        <pre class="args-code"><code>{args}</code></pre>
      </div>
    </div>
  </div>
</div>

<style>
  .tool {
    align-self: flex-start;
    display: flex;
    flex-direction: column;
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text-subtle);
    background: var(--surface);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    padding: 8px 12px;
    min-width: 300px;
    max-width: 90%;
    transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  }
  .tool:hover {
    border-color: var(--border);
    box-shadow: var(--shadow-subtle);
  }
  .tool.running {
    border-left: 3px solid var(--ice);
  }
  .tool.running.open {
    box-shadow: var(--glow-ice);
  }
  .tool.ok {
    border-left: 3px solid var(--aurora);
  }
  .tool.ok.open {
    box-shadow: var(--glow-aurora);
  }
  .tool.error {
    border-left: 3px solid var(--ember);
    background: #1a0f10;
  }
  .tool.error.open {
    box-shadow: var(--glow-ember);
  }

  .tool-header {
    display: flex;
    align-items: center;
    gap: 12px;
    cursor: pointer;
    background: transparent;
    border: none;
    padding: 0;
    margin: 0;
    width: 100%;
    text-align: left;
    font: inherit;
    color: inherit;
  }

  .tool-name {
    color: var(--ice);
    font-weight: 600;
  }
  .tool-status-mark {
    font-size: 11px;
    color: var(--text-muted);
  }
  .tool-status-mark.running {
    color: var(--ice-dim);
  }
  .tool-status-mark.ok {
    color: var(--aurora);
  }
  .tool-status-mark.error {
    color: var(--ember);
  }

  .chevron {
    font-size: 12px;
    color: var(--text-muted);
    transition: transform 0.25s cubic-bezier(0.4, 0, 0.2, 1);
    transform-origin: center;
  }
  .chevron.open {
    transform: rotate(-180deg);
  }

  .tool-mark {
    margin-left: auto;
    font-weight: bold;
  }
  .tool.ok .tool-mark {
    color: var(--aurora);
  }
  .tool.error .tool-mark {
    color: var(--ember);
  }

  /* Grid Height Animation Technique */
  .tool-details-wrapper {
    display: grid;
    grid-template-rows: 0fr;
    transition: grid-template-rows 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  }
  .tool.open .tool-details-wrapper {
    grid-template-rows: 1fr;
  }
  .tool-details {
    overflow: hidden;
    min-height: 0;
  }

  .detail-row {
    margin-top: 10px;
    padding-top: 10px;
    border-top: 1px solid var(--border-subtle);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .label {
    font-size: 10px;
    text-transform: uppercase;
    color: var(--text-muted);
    letter-spacing: 0.5px;
  }
  .args-code {
    margin: 0;
    padding: 8px;
    background: var(--void);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    overflow-x: auto;
    color: var(--text);
  }
  .args-code code {
    white-space: pre-wrap;
    word-break: break-all;
  }
</style>
