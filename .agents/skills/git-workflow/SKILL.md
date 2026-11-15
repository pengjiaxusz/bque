---
name: git-workflow
description: >-

  Universal Git workflow automation: conventional commits, fast worktree-aware linear branch sync, and non-interactive rebase & conflict resolution. Enforces strict zero-user-editing guardrails (GIT_EDITOR=true, --no-edit, no editor popups). Progressive disclosure routes into commit, sync, and rebase sub-workflows.
  Triggers: git, git-workflow, commit, branch-sync, rebase, cherry-pick, merge, push, pull, worktree sync, sync branch, align branch, milestone commit, auto commit.

---

<!-- PENGJ_TEMPLATE_START -->

# Git Workflow — Universal Git Operations & Progressive Automation

Universal Git workflow governing conventional commits, linear branch synchronization, and non-interactive rebase & conflict resolution.

---

## ⚡ 0. Core Non-Interactive Invariant (Zero User Manual Editing)

Under an automated AI Agent environment, Git **MUST NEVER pop up external editors** (nano, vim, notepad) or block on interactive keyboard input:
1. **Silent Environment**: Inject `$env:GIT_EDITOR = "cmd.exe /c exit 0"` (Windows pwsh) or `GIT_EDITOR=true` (POSIX shell), `GIT_SEQUENCE_EDITOR=true`, and `GIT_PAGER=cat`.
2. **Command Flag Guard**: Always specify `-m "<message>"` for commits, append `--no-edit` for amends/merges, and use `-c core.editor=true` for `rebase --continue`.
3. **Agent Conflict Ownership**: When rebase or merge conflicts occur, the Agent must resolve conflict markers (`<<<<<<<`) directly in the code files. **NEVER ask the user to manually edit conflicts**.
4. **Safe Abort on Ambiguity**: If conflicting changes are architecturally incompatible, execute `git rebase --abort` immediately to restore a clean state before reporting to the user.

See [references/non-interactive.md](references/non-interactive.md) for full configuration templates.

---

## 🚀 1. Sub-Workflow: Conventional Commit & Push

Always push immediately after committing to prevent work loss.

### 1.1 Trigger Conditions (Mandatory Proactive Triggers)
- **Pre-Response Commit Gate (Mandatory Barrier — No Accumulation)**: Whenever files (code, tests, or docs) are created, modified, or deleted in the current turn and have passed verification (builds/tests pass), **you MUST run the commit workflow (completeness checks, split commits, and `git push`) as the very last tool-calling step BEFORE stopping to output your final response to the user**. Never leave verified changes sitting in the working directory before yielding control; never defer committing until the end of a multi-turn conversation or wait for the user to prompt `/commit`.
  - **Planning Modes (`/plan` / `/boost`) Enforcement**: In planning modes, creating or updating `walkthrough.md` does NOT conclude the task; **you are strictly forbidden from stopping after writing a walkthrough — you MUST proceed immediately to commit and push before finishing your turn**.
  - **Exemptions**: Committing is only skipped if the turn is purely conversational (zero file changes in `git status`) or if tests/builds failed and you are stopping specifically to ask the user for diagnostic input.
- **Task Conclusion**: Before concluding any task or delivering final responses, the working tree must be clean and all commits pushed to the remote repository.
- **Explicit user instruction**: When user invokes `/commit`, "commit", "commit & push", etc.

### 1.2 Pre-Commit Completeness Check — Project-Defined
This step belongs to the **PROJECT**, not the template: follow whatever the project-specific area below the managed block defines (which commands to run, what to verify). If undefined, use the generic fallback (scan diff: verify build / update docs / run format as appropriate).
If this change touches module boundaries, public APIs, data flow, lifecycle, or adds a module/subsystem, update the matching architecture/design docs before committing.
- **Rust static check**: run `just check` (or `cargo clippy --all-targets --all-features -- -D warnings`, adding `--workspace` for workspaces) and keep warnings at zero before committing.

### 1.3 Commit Format & Immediate Push
`git commit -m "<type>(<scope>): <subject>"` followed immediately by `git push`.

- Subject in English (imperative, short, no trailing period; strictly forbidden to contain Chinese characters).
- Type/scope in English (`feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`).
- Applies to EVERY commit entry point, including host-provided `/commit` commands and hand-written `git commit`: rewrite any auto-generated Chinese subject before committing.


Detailed commit guidelines and semantic splitting: [references/commit.md](references/commit.md).

---

## 🔄 2. Sub-Workflow: Fast Worktree-Aware Branch Sync

Synchronize all branches or a feature branch into the integration branch in **1-shot** with safety backup pointers (`refs/sync-backup/`), chronological ordering, and Tree-Diff Guard:

```powershell
# Recommended: 1-Shot sync ALL branches into the current active branch
pwsh .agents/skills/git-workflow/scripts/sync-branch.ps1 -Apply

# Sync a specific single branch
pwsh .agents/skills/git-workflow/scripts/sync-branch.ps1 -SourceBranch 'feat/x' -Apply

# Read-only topology inspection (zero side-effects)
pwsh .agents/skills/git-workflow/scripts/show-branch-topology.ps1 -Detailed
```

Detailed branch sync safeguards and recovery: [references/branch-sync.md](references/branch-sync.md).

---

## 🔀 3. Sub-Workflow: Rebase & Conflict Resolution

Pull upstream changes and perform rebasing without merge pollution:

```bash
# Linearly pull remote updates
git pull --rebase --autostash

# If conflict occurs: resolve markers in code, stage, then continue silently:
git add <resolved-files>
git -c core.editor=true rebase --continue
# Or for sync-branch pipeline:
pwsh .agents/skills/git-workflow/scripts/continue-sync.ps1 -Continue

# Safe abort if unresolvable:
git rebase --abort
# Or for sync-branch pipeline:
pwsh .agents/skills/git-workflow/scripts/continue-sync.ps1 -Abort
```

Detailed rebase and conflict resolution steps: [references/rebase-conflict.md](references/rebase-conflict.md).

---

## 📚 4. Progressive Disclosure Index

- [Non-Interactive Git Specification](references/non-interactive.md)
- [Commit & Push Guidelines](references/commit.md)
- [Branch Sync & Tree-Diff Guard](references/branch-sync.md)
- [Rebase & Conflict Resolution](references/rebase-conflict.md)

<!-- PENGJ_TEMPLATE_END -->


<!-- Project-specific area left empty by default (add your pre-commit checks and repository guardrails here) -->
