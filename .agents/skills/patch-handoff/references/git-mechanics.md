# Git Mechanics for Landing a Patch Series

## 1. Hook trigger matrix

This is measured behaviour, not documentation folklore. It decides which gate can still catch a bad commit.

| How the commit is made | `pre-commit` | `commit-msg` | `pre-applypatch` / `post-applypatch` | `pre-push` |
| :--- | :--- | :--- | :--- | :--- |
| `git commit` | yes | yes | no | later |
| `git commit --amend` | yes | yes | no | later |
| `git am` | **no** | **no** | yes | later |
| `git cherry-pick` | **no** | **no** | no | later |
| `git rebase` | **no** | **no** | no | later |

Consequences:

- A series landed with `git am` never passed the project's commit-time checks (formatting, language guards, message rules). The only remaining backstop is `pre-push`.
- Do not try to repair in `pre-applypatch`: writing files and `git add`-ing them inside that hook does not reach the commit — `git am` runs `read-tree` afterwards, which wipes the index and leaves only a dirty worktree. That hook can reject, never amend.
- Hooks live in `.git/config` (`core.hooksPath`). They are per-worktree config and do not travel. Confirm they are installed in the current worktree, or `pre-push` simply does not exist. (If the user explicitly told you to isolate in a clone, install hooks there first.)
- Proof the backstop is live: the push output shows the hook's own log lines. No log lines means no hook.

## 2. `git am` failure triage

Work down this list; do not start editing patches.

1. **Wrong base.** `git log --oneline -3` on the target ref versus the doc's claimed SHA. Retry on the real base.
2. **Patch already applied.** `git am` reports "patch does not apply" while `git apply --check --reverse <patch>` succeeds. Use `git am --skip` only after confirming the content is present.
3. **Context drift, content fine.** `git am -3` (three-way) usually lands it; review the diff afterwards.
4. **Real conflict.** `git am --show-current-patch` to see what it wanted; resolve, `git add`, `git am --continue`; `git am --abort` to back out entirely.
5. **Binary or mode-only patch.** The base blob is missing; ask for the base commit instead of hand-crafting it.
6. **Mailbox damage.** A missing `From ` separator or CRLF mangling in the `.patch` file. Re-export rather than repair.

Leave the worktree in place when a replay stops mid-series — the half-applied state is the evidence — and say so in the report.

## 3. Fold a repair back into the commit that introduced it

Precondition: the file was touched by exactly one commit in the series.

```bash
git log --oneline <base>..HEAD -- <path> | wc -l    # must be 1
```

Then: overwrite the file with the repaired version, `git add`, `git commit --amend --no-edit` while that commit is `HEAD` — which is what `scripts/replay-patch-series.sh` automates. If the count is greater than 1, do not fold: fix in a final commit and state in the report why the 1:1 mapping broke.

`git rebase -i` with `--exec` is the alternative when repairs are already committed; it is fine, but never let it open an editor (`GIT_SEQUENCE_EDITOR=true`).

## 4. Author normalization

```bash
# Target identity is auto-resolved from the repo: owner from `origin` URL,
# email from `git config user.email`. NEVER type a name/email by hand.
scripts/normalize-authors.sh <base> --print-identity   # preview the target
scripts/normalize-authors.sh <base>                    # rewrite author, keep dates
git log --format="%an <%ae>" <base>..HEAD | sort -u    # must be exactly the target line
```

- `--env-filter` changes authorship and keeps author dates. `--reset-author` also resets the date, which destroys the timeline the handoff described.
- The script verifies the range ends with **exactly** the resolved target identity — one author left is not enough if it is the wrong one.
- Only the author line is attribution; leave committer identity alone unless the project says otherwise.
- `filter-branch` rewrites every commit in the range, so the range must stay small and pre-push: run it before pushing, never after.
- Pass `<name> <email>` positionally only as a deliberate override (e.g. no `origin` remote), and justify it in the report.

## 5. After the force push

```bash
git fetch --prune --force <remote>        # without --force the remote ref stays stale
git rev-list --count <remote>/<branch>..HEAD   # must be 0, else stop and ask
git status --porcelain -uall                   # must be empty
git reset --hard <remote>/<branch>
```

`git pull` after a force push merges the old history back in; `reset --hard` is the correct verb. If the branch is a submodule pointer in a parent repo, expect the parent to show the submodule as modified — that is a pointer difference, not damage. Do not commit or revert the parent on the user's behalf.

Also note: shared build caches (`CARGO_TARGET_DIR`, `node_modules` in a shared location) let a fresh clone materialize gigabytes of build output all over again — one more reason to work in place instead of re-cloning. If you were explicitly told to isolate in a clone, clean up what you created in S8.
