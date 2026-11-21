# Importing the Package's Issues, Then Closing Them Out

A handoff package carries tickets as often as it carries patches, and a ticket inside a zip is a ticket nobody can open. Two moments: import before you replay, close out after you push.

## 1. What a package carries

| Shape | Looks like | How to import |
| :--- | :--- | :--- |
| Issue bundle | `issues/NNNN-*.md` with `key` / `title` / `labels` / `parent` / `blocked_by` front matter | `issue-push.ps1` — the bundle format this skill's own scripts read and write |
| Table in the doc | a markdown table mapping patch → ticket → title | turn each row into an issue file with a `key`, then push the directory |
| Bare numbers | the doc just cites `#102–#113` | nothing to import; verify each with `gh issue view <n>` and reuse the numbers |

The bundle form is the one to prefer, because it carries the dependency wiring (`parent`, `blocked_by`) that a plain `gh issue create` loop loses.

## 2. Three checks before importing

1. **Target repo.** `gh repo view --json nameWithOwner` — in a fork `gh` defaults to the upstream repo. Pass `-Repo <owner/name>` explicitly and compare.
2. **Labels exist.** A front-matter label that is not on the repo fails the whole batch. `gh label list` first; create the missing ones deliberately, or strip them from the files and say so in the report.
3. **Duplicates.** If the doc's numbers already exist, do not recreate them. If you are re-importing a set that was imported before, the script is idempotent by `key` — but a **regenerated** key set (`T-01` vs `S1-01`) creates duplicates; `gh issue list --search '"<title>"'` before pushing.

## 3. Import

```powershell
# preview: every create, parent link and blocked_by edge, no remote writes
pwsh .agents/skills/patch-handoff/scripts/issue-push.ps1 -Repo <owner/name> `
    -InputDir <pkg>/issues -Mapping ./issue-mapping.json -DryRun

# create; add -Resume if it stops on its time budget
pwsh .agents/skills/patch-handoff/scripts/issue-push.ps1 -Repo <owner/name> `
    -InputDir <pkg>/issues -Mapping ./issue-mapping.json
```

It writes `issue-mapping.json` (`key` → real number), backfills `number` / `url` into the issue files, links parents as sub-issues and dependencies as native `blocked_by`.

The script is part of this skill (see [issue-sync.md](./issue-sync.md)); there is no extra skill to install.

## 4. The mapping table

Write this into the report before you close anything:

```
patch   issue   #     landed
0001    T-01    #101  yes (3f2a1c9)
0004    T-04    #104  no  (dropped: duplicate of #101)
```

Sources, in order of reliability: the doc's own patch↔ticket table → the numbers already cited in issue subjects → `issue-mapping.json` keys. Match only on explicit identifiers (a key embedded in the patch subject, or a number the doc states). **If a patch and a ticket do not obviously correspond, stop and ask the user.** A guessed number closes somebody else's ticket.

## 5. Close-out by mapping

| State | Action |
| :--- | :--- |
| Landed, folded into commit `abc1234` | `gh issue comment <n>` (SHA, gates, counts) → `gh issue close <n>` → labels: add new, then remove old |
| Patch dropped / folded elsewhere | keep **open**, comment with the patch number and the reason |
| Was already closed before this handoff | leave it closed; add the landing comment only, do not reopen |

`gh issue close <n> --comment "..."` on an already-closed issue discards the comment and still exits 0 — comment first, then close, then read the output.

## 6. Failure modes

- **Missing label** — the batch aborts mid-way; some issues exist, some do not. Re-run after fixing the labels: the script resumes by `key`.
- **`gh` older than 2.94** — `gh issue` has no sub-issue or dependency subcommands; those edges need `gh api` REST calls, which the script already does. Do not hand-roll GraphQL to work around it.
- **Time budget hit** — the state file is written; pass `-Resume`, do not start over.
- **Unauthenticated `gh`** — the report must say `issues not imported: gh not authenticated` **plus the exact command**. Never paraphrase the ticket list into prose and call it tracked.
- **Numbers from another repo** — a doc written against an upstream project cites upstream numbers; they mean nothing in your fork. Re-import under new numbers and say so.

## 7. Temp artifacts to delete at S8

`issue-mapping.json`, the unpacked `issues/` tree, the unpack dir itself, `fix-spec/`, logs. List them, confirm, delete — then verify the unpack dir is gone and `git status --porcelain -uall` is empty.
