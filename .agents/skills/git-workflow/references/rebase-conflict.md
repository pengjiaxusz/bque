
# Rebase & Conflict Resolution Guide

Deterministic, non-interactive rebase operations, autonomous in-code conflict resolution, and safe rollback patterns.

---

## 1. Silent Rebase Best Practices

Rebasing must always run in non-interactive mode. Never invoke `git rebase -i` or let Git trigger external editors.

### Safe Rebase Commands

```bash
# Pull upstream changes linearly without merge commits
git pull --rebase --autostash

# Rebase current branch onto target branch
git rebase <target-branch>

# If rebase pauses on conflict, resolve in-code, stage files, and continue silently:
git add <resolved-files>
git -c core.editor=true rebase --continue
```

---

## 2. In-Code Conflict Resolution (Zero User Manual Editing)

When a rebase or cherry-pick conflict pauses execution:

### Step 1: Identify Conflicted Files
```bash
git status --porcelain | grep '^UU'
# Or in PowerShell:
git status -s
```

### Step 2: Locate Conflict Markers
Open each conflicted file using `read_file`. Conflict markers follow this structure:

```text
<<<<<<< HEAD (Current change on target branch)
existing_code_block();
=======
incoming_feature_code_block();
>>>>>>> commit-hash (Incoming commit subject)
```

### Step 3: Autonomous Resolution
- Understand the technical intent of both sides from code context and commit messages.
- Synthesize the changes directly within the workspace using `edit_file` / `write_file`.
- **Completely remove all conflict markers (`<<<<<<<`, `=======`, `>>>>>>>`)**.
- **STRICT PROHIBITION**: NEVER ask the user to manually resolve conflicts in their local terminal editor.

### Step 4: Verification & Silent Continuation
- Verify the build and syntax (e.g., `cargo check`, `pnpm build`, or tests).
- Stage resolved files:
  ```bash
  git add <resolved-files>
  ```
- Continue silently:
  - For standard rebase:
    ```bash
    git -c core.editor=true rebase --continue
    ```
  - For `sync-branch.ps1` flow:
    ```powershell
    pwsh .agents/skills/git-workflow/scripts/continue-sync.ps1 -Continue
    ```

---

## 3. Safe Abort & Decision Escalation

If conflicts represent incompatible architectural decisions or ambiguous requirements:
1. **Never leave Git in a suspended rebase state**.
2. Abort immediately:
   ```bash
   git rebase --abort
   # Or for sync-branch flow:
   pwsh .agents/skills/git-workflow/scripts/continue-sync.ps1 -Abort
   ```
3. Report the conflict breakdown and specific decision choices clearly to the user.
