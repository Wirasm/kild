#!/bin/bash
set -e

# Ralph Loop - Autonomous agent implementation loop
# Usage: ./scripts/ralph-loop.sh <prd-directory> [max-iterations]

PRD_DIR="${1:-.kiro/artifacts/prds}"
MAX_ITERATIONS="${2:-20}"

if [ ! -d "$PRD_DIR" ]; then
  echo "❌ Error: PRD directory not found: $PRD_DIR"
  echo "Usage: $0 <prd-directory> [max-iterations]"
  exit 1
fi

# Resolve to absolute path
PRD_DIR="$(cd "$PRD_DIR" && pwd)"
PRD_NAME="$(basename "$PRD_DIR")"

# Files that must exist
PRD_JSON="$PRD_DIR/prd.json"
PROMPT_MD="$PRD_DIR/prompt.md"
PROGRESS_TXT="$PRD_DIR/progress.txt"

if [ ! -f "$PRD_JSON" ]; then
  echo "❌ Error: prd.json not found in $PRD_DIR"
  exit 1
fi

if [ ! -f "$PROMPT_MD" ]; then
  echo "❌ Error: prompt.md not found in $PRD_DIR"
  exit 1
fi

# Create progress.txt if it doesn't exist
if [ ! -f "$PROGRESS_TXT" ]; then
  echo "# Progress Log - $PRD_NAME" > "$PROGRESS_TXT"
  echo "" >> "$PROGRESS_TXT"
  echo "Started: $(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$PROGRESS_TXT"
  echo "" >> "$PROGRESS_TXT"
fi

echo "🤖 Ralph Loop - $PRD_NAME"
echo "📁 PRD Directory: $PRD_DIR"
echo "🔄 Max Iterations: $MAX_ITERATIONS"
echo ""

# Count incomplete stories
INCOMPLETE_COUNT=$(jq '[.userStories[] | select(.passes == false)] | length' "$PRD_JSON")
echo "📋 Found $INCOMPLETE_COUNT incomplete stories"
echo ""

for i in $(seq 1 $MAX_ITERATIONS); do
  echo "=== Iteration $i/$MAX_ITERATIONS ==="
  
  # Build the complete prompt with meta context + current task
  FULL_PROMPT=$(cat <<EOF
---
description: Ralph Agent - Autonomous Implementation Loop
---

# Ralph Instructions for $PRD_NAME

## Meta Context (Read Once, Apply Always)

$(cat "$PROMPT_MD")

---

## Current Iteration Context

**Iteration**: $i/$MAX_ITERATIONS
**PRD Location**: $PRD_DIR
**Working Directory**: $(pwd)

### Your Task for This Iteration

1. Read prd.json to see current user story status
2. Read progress.txt to see what previous iterations accomplished
3. Pick the highest priority story where passes: false
4. Implement that ONE story completely
5. Update prd.json to mark passes: true
6. Append learnings to progress.txt

### Files You Need

- **PRD**: $PRD_JSON
- **Progress**: $PROGRESS_TXT
- **Working Dir**: Current directory ($(pwd))

### Stop Condition

When ALL user stories have passes: true, output:
<promise>COMPLETE</promise>

Otherwise, complete your current story and end normally.

---

## Previous Progress

$(cat "$PROGRESS_TXT")

EOF
)
  
  # Run Kiro with the complete prompt
  echo "🤖 Running Ralph iteration..."
  OUTPUT=$(echo "$FULL_PROMPT" | kiro-cli chat --no-interactive --trust-all-tools 2>&1 | tee /dev/stderr) || true
  
  # Check for completion
  if echo "$OUTPUT" | grep -q "<promise>COMPLETE</promise>"; then
    echo ""
    echo "✅ Iteration $i completed successfully"
    echo ""
    echo "🎉 All user stories completed!"
    exit 0
  fi
  
  # Update incomplete count
  INCOMPLETE_COUNT=$(jq '[.userStories[] | select(.passes == false)] | length' "$PRD_JSON")
  echo ""
  echo "✅ Iteration $i completed successfully"
  echo "📋 Remaining incomplete stories: $INCOMPLETE_COUNT"
  echo ""
  
  # Check if we're making progress
  if [ "$INCOMPLETE_COUNT" -eq 0 ]; then
    echo "🎉 All user stories completed!"
    exit 0
  fi
  
  # Brief pause between iterations
  sleep 2
done

echo ""
echo "⚠️ Max iterations ($MAX_ITERATIONS) reached"
echo "📋 Remaining incomplete stories: $INCOMPLETE_COUNT"
exit 1
