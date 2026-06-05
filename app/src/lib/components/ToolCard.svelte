<script lang="ts">
  interface Props {
    name: string;
    args: string;
    status: "running" | "ok" | "error";
  }

  let { name, args, status }: Props = $props();
  let isOpen = $state(false);

  // Parse arguments and extract primary target for display
  function getInlineArg(name: string, argsStr: string): string {
    if (!argsStr) return "";
    try {
      const parsed = JSON.parse(argsStr);
      if (typeof parsed !== 'object' || parsed === null) return "";

      switch (name) {
        case "read_file":
        case "write_to_file":
        case "replace_file_content":
        case "multi_replace_file_content":
        case "view_file": {
          const file = parsed.TargetFile || parsed.AbsolutePath || parsed.path || parsed.Target;
          if (file) {
            // Extract relative-looking suffix for readability
            const parts = file.split(/[/\\]/);
            return parts.slice(-2).join("/");
          }
          break;
        }
        case "run_command":
          return parsed.CommandLine || "";
        case "grep_search":
          return parsed.Query || "";
        case "list_dir": {
          const dir = parsed.DirectoryPath || "";
          const parts = dir.split(/[/\\]/);
          return parts.slice(-2).join("/") || dir;
        }
        default: {
          const common = parsed.TargetFile || parsed.AbsolutePath || parsed.path || parsed.CommandLine || parsed.Query || parsed.DirectoryPath;
          if (common) {
            if (typeof common === 'string' && (common.includes('/') || common.includes('\\'))) {
              return common.split(/[/\\]/).slice(-2).join("/");
            }
            return String(common);
          }
        }
      }
    } catch {
      // Not JSON or parsing failed
    }
    return "";
  }

  let inlineArg = $derived(getInlineArg(name, args));
</script>

<div class="tool {status}" class:open={isOpen}>
  <button class="tool-header" onclick={() => isOpen = !isOpen}>
    <span class="tool-name">
      🔧 {name}{#if inlineArg}<span class="tool-arg">: {inlineArg}</span>{/if}
    </span>
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
  .tool-arg {
    color: var(--text-bright);
    font-weight: normal;
    font-family: var(--mono);
    opacity: 0.85;
  }
</style>
