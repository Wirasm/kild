# Resume Task List

Task lists now transfer automatically across kild sessions when using `--resume`.

## How It Works

KILD automatically manages task list persistence:

1. When you `kild create` a session with Claude, a task list ID is generated
2. When you `kild open --resume`, the task list is preserved from the previous session
3. When you `kild destroy`, the task list is cleaned up automatically

## Usage

```bash
# Create a new kild (task list is created automatically)
kild create my-feature --agent claude

# Stop the agent
kild stop my-feature

# Resume with the same task list
kild open my-feature --resume
```

No manual task list management needed - it just works.
