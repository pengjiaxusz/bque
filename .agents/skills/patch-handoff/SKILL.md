---
name: patch-handoff
description: >-

  Verify and land an offline patch handoff package (`HANDOFF.md` + `patches/NNNN-*.patch`): replay the series with `git am`, repair the build and tests, fold each repair back into the commit that introduced the defect, normalize authorship to the repository owner's identity (auto-resolved, never hand-filled), force-push with lease, sync the worktree, file every problem found during acceptance as a GitHub issue, close out the referenced tickets plus the issues you filed, and delete the unpacked temp content. Every acceptance claim in the handoff document is an unverified hypothesis until you reproduce it yourself. Use when receiving a patch series or handoff zip from another agent, or when a delivered series does not compile as claimed. Triggers: patch-handoff, handoff package, handoff zip, patch series, git am, apply patches, verify handoff, fold fixes, offline handoff, file handoff issues, close handoff issues.

---

<!-- PENGJ_TEMPLATE_START -->
# Patch Handoff — Verify & Land an Offline Patch Series

Turn another agent's offline handoff package into commits that actually live in the repo, and prove it with the project's real gates instead of the document's claims.

**Assume the document is wrong until reproduced.** Handoff packages routinely claim `git am` / lint / tests all passed while the series does not compile and the recorded baseline SHA is off. Treat every claim as a hypothesis, reproduce it, and state the outcome in your report.

## Work in Place — Do Not Clone the Project to Test

**Accept and land the patches directly in the user's existing worktree. Do NOT `git clone` the repo again, copy it to a scratch directory, or spin up a parallel build just to "test safely".** Re-cloning plus reinstalling dependencies and rebuilding a complex project costs tens of minutes to hours and buys nothing: the gates you must reproduce live in this worktree, and the commits must land here. A throwaway copy is a second, divergent state you would have to reconcile later — strictly more work and more risk.

Only clone when the user explicitly asks for isolation (e.g. the worktree must stay pristine for another in-flight task), and even then install hooks in the clone before working. Default: operate in place, keep the worktree clean enough to `git am`, and clean up only the unpack directory and logs you created (see S8).

## Before Touching Anything: Pin Seven Parameters

Read them out of the repo, never out of `HANDOFF.md`.

| Parameter | Where to get it | Why it matters |
| :--- | :--- | :--- |
| Base ref | `git rev-parse --abbrev-ref origin/HEAD`; compare with the SHA the doc claims | Most `git am` failures are a wrong base, not a bad patch |
| Gate chain | `justfile` / `Makefile` / `package.json` / `.github/workflows/*.yml` | Your locally chosen commands are not the CI gate |
| Format check | Whether CI runs `fmt --check` / `lint` as a separate step | A linter passing says nothing about formatting |
| Hook installation | `git config core.hooksPath`; run the repo's installer in the current worktree if unset | Hooks are per-worktree config and do not travel with a clone |
| Author identity | `normalize-authors.sh --print-identity` — owner from `origin` URL + `git config user.email`; NEVER hand-fill or read from the package | Package authors are synthetic placeholders; a hand-typed name is the leak |
| Issue tracker | The issue numbers the doc cites, plus the repo name | Closing them is part of landing, not an afterthought |
| Issue write target | `gh repo view --json nameWithOwner`; does each cited number exist (`gh issue view <n>`) | On a fork `gh` defaults to the upstream repo — filing there is filing in the wrong place |
| Issue status flow | `gh label list`; the project's in-progress / needs-decision labels | Miss the label and you close tickets without moving the board — the tracker silently rots |

## Quick Start

```bash
PKG=<handoff zip or unpacked dir>
BASE="origin/$(git rev-parse --abbrev-ref origin/HEAD | sed 's|^origin/||')"
git status -sb; git rev-list --count "$BASE"..HEAD     # must be clean and 0

.agents/skills/patch-handoff/scripts/inspect-handoff-package.sh "$PKG"   # pre-flight audit
.agents/skills/patch-handoff/scripts/normalize-authors.sh "$BASE" --print-identity  # confirm target identity
.agents/skills/patch-handoff/scripts/replay-patch-series.sh <dir>/patches <fix-spec-dir> "$BASE"

# then run the whole gate chain, no skipped steps
# after folding repairs, normalize authorship and then push:
.agents/skills/patch-handoff/scripts/normalize-authors.sh "$BASE"   # no name/email: auto from repo
```

## Workflow

| Step | Action | Key point |
| :-- | :--- | :--- |
| S1 | Pre-flight | Inventory patches, authors, touched files; compare claimed baseline with reality; file an issue for every discrepancy you confirm (dedupe first) |
| S2 | Replay | `git am patches/*.patch` in order; on failure diagnose the base first, then `--show-current-patch` |
| S3 | Repair | Run the gate chain top-down; syntax errors mask type errors, so re-run after each fix; log anything you cannot fix as an issue |
| S4 | Fold | Fold each repair into the commit that introduced it, keeping one commit per ticket |
| S5 | Author + push | Run `normalize-authors.sh "$BASE"` **without name/email** so the identity is auto-resolved from the repo (keeping author dates); then `push --force-with-lease` |
| S6 | Resync | After a force push: `fetch --prune --force` then `reset --hard`, never `git pull` |
| S7 | Issues | Close out **both** lines: each implemented ticket (comment, then close, then labels) and every issue you filed during S1–S4 (closed with its fold-back SHA, or left open with a label and an owner) |
| S8 | Cleanup | Delete the unpacked temp content you created: unpack dir, fix-spec dir, logs. Verify it is gone. No throwaway clone should exist — do not clone to test |

## File Every Problem as an Issue — Not Just in the Report

Report prose is read once and then rots; an issue keeps a number somebody can point at. Anything you confirm while verifying gets a GitHub issue **in the same pass**, before you move on:

- The series does not compile as delivered, or the doc's claims do not reproduce.
- Scope gaps: a claimed feature is missing, or a cited ticket has no patch.
- Behavioural gaps the tests cannot see, while the user has not decided yet.
- Anything you deliberately leave unfixed.

Rules for filing:

1. **Deduplicate first.** `gh issue list --state all --search '"<symptom>"'`; if a twin exists, extend it with a comment instead of opening another one.
2. **One issue per distinct defect.** Title `[handoff] <package>: <symptom in one line>`; body carries package + patch number, claim vs reality, the command that reproduces it, and what "fixed" looks like.
3. **Reuse the project's labels** (`gh label list`); do not invent a taxonomy for one handoff.
4. **Back-reference from the report.** Every open-item row names its `#number` — a row without a number is an issue you forgot to file.
5. **No `gh`, no write access, or `gh` pointing at a fork's upstream:** say so in the report and print the command you would have run. Never quietly degrade back to prose.

Filed issues are not decoration: S7 has to give each one a terminal state.

## Issue Close-out — After Landing, Nothing Stays Dangling

Run two lines, both of which must end with a state you can name:

1. **Ticket line** (the tickets the doc maps to): for each one, `gh issue comment <n>` with the landing summary — folded commit SHA, gate commands, test counts — **then** `gh issue close <n>`, **then** flip labels (add the new label before removing the old one, so no issue is ever label-less). Do not use `gh issue close --comment`: on an already-closed issue it drops the comment and still exits 0.
2. **Filed-issue line** (what you opened in S1–S4): fixed in this pass → comment with the fold-back SHA and close it; not fixed, or waiting on the user → keep it open, add the project's decision/blocked label, assign or @-mention whoever moves next, and say so in the report.
3. **Self-check.** `gh issue list --state open --search '[handoff]'` must leave only issues you can justify. End the report with three lines: `closed #…` / `filed #…` / `still open #…, why`.

## Cleanup — the Unpacked Temp Content Must Go

Only `patches/*.patch` has lasting value in a handoff package; `HANDOFF.md`, the doc's side files and your `fix-spec/` tree are scaffolding. Once the handoff is landed, delete them:

- Unpack dir (`handoff-*/`, a `mktemp -d` path), fix-spec dir, logs, exported intermediates.
- List before deleting (iron rule 5), delete only what you created, no wildcards, nothing in shared locations such as `~/.cache`.
- Exception: a worktree left half-applied by a stopped `git am` is evidence — keep it until the user has seen it, then delete it.
- Verify: `git status --porcelain -uall` is empty and the unpack dir no longer exists. A leftover unpack dir is how the next handoff gets confused with the last one.

## Iron Rules

1. `HANDOFF.md` and the handoff directory never enter the repo; only `patches/*.patch` may arrive via `git am`.
2. The claimed baseline SHA is not evidence: the real base is the commit where the series applies cleanly and every API reference lines up. Report the discrepancy.
3. One commit per ticket. Fold repairs back into the offending commit; do not append `fix(...)` commits that break the 1:1 mapping.
4. Authorship must be the repository owner's identity. Run `normalize-authors.sh` **without** name/email so it resolves the owner from `origin` and the email from `git config user.email`; never hand-fill them or copy them from the package.
5. Before deleting anything, print the list and get confirmation. Delete only what you created. No wildcards.
6. A failing test is reported to the user, not silently accommodated by editing the assertion.
7. `git am`, `cherry-pick` and `rebase` trigger no `pre-commit` hook — only `pre-push` can still catch the damage. Confirm hooks are installed in the current worktree before you work; if you were explicitly told to isolate in a clone, install hooks there first.
8. **Work in the user's worktree.** Never clone or copy the project just to verify or land a patch. Complex projects lose tens of minutes to hours on a fresh clone plus dependency setup, for zero gain.
9. **A problem you found but did not file does not exist.** Every confirmed defect goes to GitHub as an issue in the pass that found it — deduped, with the reproducing command — and the report cites its number.
10. **Nothing in scope stays dangling after landing.** Tickets and filed issues both end closed (with the fold-back SHA) or open with a label and a named owner. `gh issue close --comment` is not a substitute for comment-then-close.
11. **The unpacked temp content is not an artifact.** Unpack dir, fix-spec dir and logs are deleted at S8, and you verify they are gone rather than assuming.

## How to Write Acceptance Criteria

Build a two-column table — command, passing signature — from the project's real gate chain (fill it in the project-specific area below or in a reference file). Then respect the boundary:

- A gate passing locally means the commands you ran passed; it does not mean CI passes. CI usually runs a superset (formatting, language guards, title checks).
- For tests, read the summary line (`failed=0`, total count) rather than the absence of red text.
- For newly added behaviour, grep the log for the specific test name to confirm it actually executed instead of being skipped or unnamed.

## What Tests Cannot Catch

Green build plus green tests is not feature completeness. Shape or snapshot assertions that count elements cannot tell "the element is there" from "the element draws nothing" — a real case had a popup reserve capacity for its menu labels and never push them, so every row rendered blank while all tests passed. Sample the new output paths by hand (text, icons, separators, side effects) and put what you find in the report for the user to decide on.

## Scripts

| Script | Purpose |
| :--- | :--- |
| `scripts/inspect-handoff-package.sh` | Pre-flight: patch inventory, authors, touched files, claimed-vs-actual baseline |
| `scripts/replay-patch-series.sh` | Ordered `git am` plus fold-back of repair files into their commits |
| `scripts/normalize-authors.sh` | Rewrite authorship over a commit range to the auto-resolved owner identity (dates kept); `--print-identity` previews the target |
| `scripts/sync-after-force-push.sh` | Safe post-force-push worktree resync, refuses on dirty or unpushed state |

## References

- [references/handoff-audit.md](references/handoff-audit.md) — classifying claims, the usual lies, report format
- [references/git-mechanics.md](references/git-mechanics.md) — hook trigger matrix, `git am` failure triage, fold preconditions, author rewrite
- [references/field-notes.md](references/field-notes.md) — real error-to-API table from one landed series
- [references/issue-closeout.md](references/issue-closeout.md) — when to file, the issue body template, dedupe, the state machine, `gh` failure modes
<!-- PENGJ_TEMPLATE_END -->


<!-- Project-specific area left empty by default (fill in the gate chain, issue repo and repository conventions here; author identity is auto-resolved, do not hand-fill it) -->
