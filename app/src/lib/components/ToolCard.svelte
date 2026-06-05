<script lang="ts">
  interface Props {
    name: string;
    args: string;
    status: "running" | "ok" | "error";
  }

  let { name, args, status }: Props = $props();
</script>

<div class="tool {status}">
  <details>
    <summary>
      <span class="tool-name">🔧 {name}</span>
      <span class="tool-status-mark" class:running={status === "running"} class:ok={status === "ok"} class:error={status === "error"}>
        {#if status === "running"}
          running…
        {:else}
          {status === "ok" ? "completed" : "failed"}
        {/if}
      </span>
      <span class="tool-mark">{status === "running" ? "…" : status === "ok" ? "✓" : "✗"}</span>
    </summary>
    <div class="tool-details">
      <div class="detail-row">
        <span class="label">arguments:</span>
        <pre class="args-code"><code>{args}</code></pre>
      </div>
    </div>
  </details>
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
    transition: border-color 0.2s ease, box-shadow 0.2s ease;
  }
  .tool:hover {
    border-color: var(--border);
    box-shadow: var(--shadow-subtle);
  }
  .tool.running {
    border-left: 3px solid var(--ice);
  }
  .tool.ok {
    border-left: 3px solid var(--aurora);
  }
  .tool.error {
    border-left: 3px solid var(--ember);
    background: #1a0f10;
  }

  details {
    width: 100%;
  }
  summary {
    display: flex;
    align-items: center;
    gap: 12px;
    cursor: pointer;
    list-style: none;
    user-select: none;
  }
  summary::-webkit-details-marker {
    display: none;
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

  .tool-details {
    margin-top: 10px;
    padding-top: 8px;
    border-top: 1px solid var(--border-subtle);
  }
  .detail-row {
    display: flex;
    flex-direction: column;
    gap: 4px;
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
