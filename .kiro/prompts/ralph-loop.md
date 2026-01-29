---
description: Start autonomous Ralph loop to execute PRD plan until all validations pass
argument-hint: 
---

# Ralph Loop Executor

---

## Your Mission

First, ask the user which PRD they want to run Ralph on. Then start an autonomous Ralph loop that executes the PRD iteratively until all user stories pass.

**Core Philosophy**: Self-referential feedback loop. Each iteration, you see your previous work in files and git history. You implement, validate, fix, repeat - until complete.

---

## Phase 1: DISCOVER - Which PRD to Execute

Ask the user:
- Which PRD do you want to run Ralph on?
- Show available PRDs in `.kiro/artifacts/prds/`
- Let them select or specify the PRD name

Wait for their response before proceeding.

---

## Phase 2: VALIDATE - Check PRD Exists

### 2.1 Verify PRD Structure

Check that `.kiro/artifacts/prds/{prd-name}/` contains:
- `prd.json` - Task definitions
- `prompt.md` - Ralph instructions
- `progress.txt` - Learning log
- `ralph.sh` - Execution script

### 2.2 Parse Max Iterations

Default to 20 iterations, or ask user if they want different limit.

**PHASE_2_CHECKPOINT:**
- [ ] PRD directory exists and is valid
- [ ] All required files present
- [ ] Max iterations set

---

## Phase 3: SETUP - Initialize Ralph State

### 3.1 Create State File

Create `.kiro/artifacts/prds/{prd-name}/ralph.state.md`:

```markdown
---
active: true
iteration: 1
max_iterations: {N}
prd_name: "{prd-name}"
started_at: "{ISO timestamp}"
---

# Ralph Loop State

## Current Task
Execute PRD user stories and iterate until all pass.

## PRD Reference
.kiro/artifacts/prds/{prd-name}/prd.json

## Instructions
1. Read prd.json for current user stories
2. Pick highest priority story where passes: false
3. Implement that ONE story
4. Run typecheck and tests
5. Update prd.json: passes: true
6. Append learnings to progress.txt
7. When ALL stories pass: output <promise>COMPLETE</promise>

## Progress Log
(Append learnings after each iteration)
```

### 3.2 Display Startup Message

```markdown
## Ralph Loop Activated

**PRD**: {prd-name}
**Location**: .kiro/artifacts/prds/{prd-name}/
**Iteration**: 1
**Max iterations**: {N}

The Ralph loop is now active. Each iteration will:
- Read current PRD state
- Implement next incomplete story
- Validate and commit changes
- Update PRD tracking

To monitor: `cat .kiro/artifacts/prds/{prd-name}/ralph.state.md`
To check progress: `cat .kiro/artifacts/prds/{prd-name}/prd.json`

---

CRITICAL REQUIREMENTS:
- Work through user stories one at a time
- Run typecheck and tests after each story
- Update prd.json to mark stories complete
- Only output <promise>COMPLETE</promise> when ALL stories pass
- Commit each completed story

---

Starting iteration 1...
```

**PHASE_3_CHECKPOINT:**
- [ ] State file created
- [ ] Startup message displayed

---

## Phase 4: EXECUTE - Start Ralph Script

### 4.1 Navigate to PRD Directory

```bash
cd .kiro/artifacts/prds/{prd-name}
```

### 4.2 Start Ralph Loop

Execute the Ralph script with max iterations:

```bash
./ralph.sh {max-iterations}
```

This will:
- Run the autonomous loop
- Each iteration pipes `prompt.md` into Kiro CLI
- Continue until all stories pass or max iterations reached
- Output `<promise>COMPLETE</promise>` when done

### 4.3 Monitor Progress

While Ralph runs, you can monitor:
```bash
# Check story completion status
cat prd.json | jq '.userStories[] | {id, passes}'

# View accumulated learnings
cat progress.txt

# Check git commits
git log --oneline -10
```

**PHASE_4_CHECKPOINT:**
- [ ] Ralph script executed
- [ ] Loop is running autonomously

---

## Success Criteria

- **USER_INPUT**: Asked which PRD to run and got response
- **SCRIPT_EXECUTED**: Ralph script started successfully
- **LOOP_RUNNING**: Autonomous loop is executing
- **MONITORING_AVAILABLE**: User can track progress via files
