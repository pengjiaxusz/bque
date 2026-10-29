
# Branch Sync & Linear Merging Specification

Fast worktree-aware linear branch synchronization with safety backup pointers, chronological ordering, and Tree-Diff Guard.

---

## 1. 1-Shot Linear Synchronization Pipeline

When syncing branches or consolidating worktrees, run the automated pipeline:

```powershell
# 1-Shot sync ALL branches into the current active integration branch
pwsh .agents/skills/git-workflow/scripts/sync-branch.ps1 -Apply

# Sync a specific single branch
pwsh .agents/skills/git-workflow/scripts/sync-branch.ps1 -SourceBranch 'feat/x' -Apply

# Read-only topology inspection
pwsh .agents/skills/git-workflow/scripts/show-branch-topology.ps1 -Detailed
```

### Pipeline Execution Order

1. **Dynamic Target Resolution**: Discovers the integration branch (defaults to the currently checked out branch).
2. **Remote-Truth Discovery**: Queries `git ls-remote` directly to avoid stale `refs/remotes/*` tracking.
3. **Net-Commit Extraction**: Computes unreachable commits (`rev-list --not`), patch-id deduplication, and sorts in ascending committer time order (`%ct`).
4. **Safety Snapshot**: Records every branch tip into `refs/sync-backup/<branch>/<timestamp>-<sha>`.
5. **Linear Rebase / Cherry-Pick**: Merges chronologically without creating merge commits.
6. **Tree-Diff Guard**: Mathematically verifies that 100% of the diff tree from source commits is preserved.
7. **Batch Worktree & Branch Alignment**: Aligns worktrees and executes safe push with `--force-with-lease`.
8. **Automated Verification**: Runs the project's verification test (e.g., `cargo test` / `pnpm test`).

---

## 2. Tree-Diff Guard Safeguard

Before branches are reset or pushed, the system performs a dual-layer invariant check:
- **Zero Cherry Residue**: `git cherry -v` must report zero `+` lines.
- **Tree-Diff File Equivalence**: Every file touched between the merge base and the source branch must be preserved in the integration branch.

**If either check fails, branches are NEVER reset or pushed**, and integration rolls back immediately to the pre-merge snapshot.

---

## 3. Disaster Recovery & Snapshot Management

If any unexpected conflict or misalignment occurs:

```powershell
# List existing backup snapshots
pwsh .agents/skills/git-workflow/scripts/manage-sync-backups.ps1 -List

# Restore a branch to a specific snapshot
pwsh .agents/skills/git-workflow/scripts/manage-sync-backups.ps1 -RestoreBranch 'feat/x' -BackupRef 'refs/sync-backup/...'
```
