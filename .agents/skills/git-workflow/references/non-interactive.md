
# Non-Interactive Git Specification & Silent Execution

> **Core Invariant**: Under an automated AI Agent environment, Git MUST NEVER pop up external editors (nano, vim, notepad) or block on interactive keyboard input. All Git operations must run silently, non-interactively, and deterministically.

---

## 1. Environment & Config Hardening

To prevent git from launching `$EDITOR` / `$VISUAL` across different operating systems, ensure the following environment variables or command-level configurations are applied:

### Cross-Platform Environment Variables

| Variable | Windows PowerShell | POSIX Shell (bash / zsh / sh) | Purpose |
| :--- | :--- | :--- | :--- |
| `GIT_EDITOR` | `$env:GIT_EDITOR = "cmd.exe /c exit 0"` | `export GIT_EDITOR=true` | Blocks editor popup on commit/rebase/merge |
| `GIT_SEQUENCE_EDITOR` | `$env:GIT_SEQUENCE_EDITOR = "cmd.exe /c exit 0"` | `export GIT_SEQUENCE_EDITOR=true` | Blocks editor popup during interactive rebase TODOs |
| `GIT_PAGER` | `$env:GIT_PAGER = "cat"` | `export GIT_PAGER=cat` | Prevents hanging on pagers (less/more) |
| `GIT_TERMINAL_PROMPT` | `$env:GIT_TERMINAL_PROMPT = "0"` | `export GIT_TERMINAL_PROMPT=0` | Prevents hanging on auth/credential prompts |

### Single-Command Inline Overrides (Highest Reliability)

If executing ad-hoc git commands directly via shell tools without persistent environment variables, always prepend `-c core.editor=true` and `-c sequence.editor=true`:

```bash
# General safe commit / rebase continue / cherry-pick continue
git -c core.editor=true commit --amend --no-edit
git -c core.editor=true rebase --continue
git -c core.editor=true cherry-pick --continue
git --no-pager log -n 5 --oneline
```

---

## 2. Command-Level Defense Matrix

| Git Action | Hazardous Interactive Command | Safe Non-Interactive Replacement | Why it hangs |
| :--- | :--- | :--- | :--- |
| **Commit** | `git commit` (no `-m`) | `git commit -m "<type>: <msg>"` | Git opens editor for commit message |
| **Amend** | `git commit --amend` | `git commit --amend --no-edit` | Git opens editor to edit previous message |
| **Rebase Continue** | `git rebase --continue` | `git -c core.editor=true rebase --continue` | Opens editor to verify rebased commit message |
| **Interactive Rebase** | `git rebase -i <upstream>` | **Strictly Forbidden** in Agent flows | Demands manual pick/squash editing |
| **Merge** | `git merge <branch>` | `git merge --no-edit <branch>` | Opens editor for merge commit message |
| **Pull** | `git pull` (implicit merge) | `git pull --rebase --autostash` | May trigger unexpected merge message editor |
| **Diff / Log** | `git log`, `git diff` | `git --no-pager log`, `git --no-pager diff` | Pager waits for 'q' keystroke |
| **Merge Tool** | `git mergetool` | **Strictly Forbidden** | Attempts to spawn external GUI diff tools |

---

## 3. Conflict Resolution & Zero-User-Editing Rule

1. **Agent Owns Conflict Editing**:
   When a rebase or merge conflict occurs, the Agent has full access to workspace file reading and writing tools (`read_file`, `edit_file`, `write_file`).
   **The Agent MUST locate conflict markers (`<<<<<<<`, `=======`, `>>>>>>>`), analyze both sides, perform the code merge directly in the files, and verify syntax.**
   **NEVER ask the user to manually open an editor to resolve conflicts.**

2. **Non-Interactive Resume**:
   After cleaning up conflict markers and staging changes via `git add <files>`:
   - For rebase: `git -c core.editor=true rebase --continue`
   - For cherry-pick / branch-sync: `pwsh .agents/skills/git-workflow/scripts/continue-sync.ps1 -Continue`

3. **Safe Abort on Unresolvable Conflicts**:
   If conflicting changes represent conflicting high-level architectures or data loss risks that cannot be deterministically inferred:
   - Execute `git rebase --abort` (or `continue-sync.ps1 -Abort`) immediately to restore a clean working tree.
   - Present clear decision options to the user with full context, rather than leaving the repository in a suspended state.
