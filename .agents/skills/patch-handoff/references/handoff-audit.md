# Auditing a Handoff Document

A handoff package ships claims, not facts. This file is how to sort them and what your report must contain.

## 1. Classify every claim

| Class | Example | How to verify |
| :--- | :--- | :--- |
| Executable | "`just test` passes", "`cargo build` clean" | Run it yourself, capture the summary line |
| Baseline | "based on `a1b2c3d`" | `git rev-parse <base>`; `git merge-base --is-ancestor`; does the series apply on it |
| Structural | "12 patches, one per ticket #102–#113" | Count patches, read subjects, map to issue numbers |
| Scope | "implements the state machine and tests" | Diffstat, then grep the diff for the named symbols |
| Behavioural | "renders the menu labels" | Cannot be verified by tests alone — inspect output |

Only the last class is dangerous: it takes a claim that no green command contradicts and quietly fails it. Give behavioural claims their own section in the report.

## 2. The usual lies, in order of frequency

1. **Does not compile as delivered.** Type errors, wrong field or variant names, arity mismatches against the real API. Expect this by default.
2. **Wrong baseline SHA.** The patch context often reveals the real base — the commit where it applies cleanly.
3. **Gates named incorrectly.** The doc names commands the project does not have, or skips the formatting step that CI enforces.
4. **Counts inflated.** "N tests passing" refers to an older run, or to a subset, or counts ignored tests.
5. **Synthetic authorship.** Placeholder identity like `dev@project.local`, or a name the upstream agent invented. Fix with `scripts/normalize-authors.sh <base>` **without** name/email, so only the repo-owner identity resolved from `origin` + `git config user.email` reaches the shared branch.

## 3. Acceptance criteria you write

For each gate, record command and passing signature. Both columns come from the repo, not from the document:

| Command | Passing signature |
| :--- | :--- |
| `<project lint>` | exit 0, no `warning:` / `error` |
| `<project format check>` | no diff output |
| `<project test>` | `failed=0`, count recorded for the baseline |
| `<hook>` | the hook's own log lines appear before the push succeeds |

Add one line per run: the total count, so the next handoff has something real to compare against.

## 4. Report format

Four blocks, always:

1. **Where and what** — which worktree, which base, which commit range, in what order.
2. **Per-file findings** — `file : problem : fix`, one row per repair, including repairs that were purely mechanical.
3. **Gate results** — each command with its conclusion and the test count.
4. **Open items** — documentation errors, behavioural gaps tests cannot see, anything the user must decide — plus the patch↔issue mapping: which ticket closed with which fold-back SHA, which stayed open and why.

Never summarise a landed series as "implemented and verified" without disclosing what was broken on arrival. The user reads the report to decide whether to trust the upstream agent again — and the mapping table is what turns "we landed 12 patches" into something they can audit on the tracker.

## 5. When to stop and ask

Stop before touching the remote when: the series applies but the mapping is broken beyond repair, a test failure looks like a genuine regression rather than a compile artifact, the base ref is ambiguous, or folding a fix requires modifying a commit outside the series.
