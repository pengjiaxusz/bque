# Filing and Closing Out Issues for a Handoff

Two moments matter: while verifying (problems get filed), and after landing (every issue in scope gets a terminal state). A handoff that ends with an unnumbered "open items" paragraph has produced homework, not tracking.

## 1. What deserves an issue

| What you found | File it? | Why |
| :--- | :--- | :--- |
| Does not compile as delivered | Yes, in the same pass | It is a fact, not an opinion, and the upstream agent can act on an issue |
| Doc claims a gate that does not exist, or a pass that does not reproduce | Yes | Keeps the next handoff honest |
| Ticket cited but no patch behind it | Yes | A mapping gap belongs on the tracker |
| Behavioural gap tests cannot see | Yes, unless the user accepts it in this session | Otherwise it evaporates with the report |
| Repair you already folded | No — the commit is the record | Close the source issue if there was one |
| Style nit fixed in passing | No | The commit message is enough |

Rule of thumb: if someone owes a follow-up, it is an issue. If the commit already says it, it is not.

## 2. Dedupe before opening anything

```bash
gh issue list --state all --search '"<package name>" in:title'
gh issue list --state open --search '<symptom words>'
```

Same cause → comment on the existing issue ("also hit by handoff `<pkg>`, patch 0007") and cite that number in your report instead of opening a twin. Two issues for one defect double the closing work and halve the trust in the tracker.

## 3. Body template

```bash
gh issue create \
  --title '[handoff] <pkg>: <one-line symptom>' \
  --label bug \
  --body "$(cat <<'EOF'
Package: <file name>, patches 0001-0012
Where: patch 0007 touches src/foo.rs

Claimed: `cargo build` clean.
Actual: fails with E0609 `no field 'size'` at src/foo.rs:88 — the repo has `font_size`.

Reproduce:
    git am patches/0007-*.patch && cargo build

Fixed means: the series applies and the gate chain passes on the real base <sha>.
EOF
)"
```

`<<'EOF'` keeps backticks and `$` literal. The command prints the issue URL, which is where the number for your report comes from — copy it, do not guess it.

## 4. State machine

| Moment | Action | Command |
| :--- | :--- | :--- |
| Landed, folded into a commit | Comment with the SHA, then close | `gh issue comment <n> --body "..."` then `gh issue close <n>` |
| Decided not to fix | Keep open, label it, name the owner | `gh issue edit <n> --add-label needs-decision --add-assignee @me` |
| Duplicate of an existing issue | Comment on the survivor, close the twin | `gh issue close <n> --reason "not planned"` |
| Belongs to the upstream agent | Keep open, say who acts | comment, then `--add-assignee` or @-mention |

Labels come from `gh label list`. If the project has none, use one neutral label and say so in the report rather than inventing a taxonomy for a single handoff.

## 5. `gh` failure modes that cost you the record

- `gh issue close <n> --comment "..."` on an already-closed issue discards the comment and still exits 0. Always comment first, then close, and read the output.
- Flip labels in the right order: `--add-label` first, `--remove-label` second, or the issue is briefly label-less and drops off the board.
- In a fork, `gh` targets the upstream repo unless `-R <owner>/<repo>` is passed. Check `gh repo view --json nameWithOwner` first — filing into someone else's repo is worse than not filing.
- `gh` missing or unauthenticated: the report must say `issue not filed: gh not authenticated` **plus the exact command**. Do not paraphrase the issue into prose and call it tracked.
- On Windows Git Bash, a heredoc can mangle quotes; write the body to a temp file and use `--body-file`, then delete that file in S8 like every other temp artifact.

## 6. Report footer

```
closed: #101, #102, #103 (folded 3f2a1c9)
filed:  #141 (E0609 in 0007), #142 (missing test for menu labels)
open:   #142 — needs the user to decide whether blank rows are acceptable
```

That footer is the difference between "landed" and "landed, and nobody has to re-derive what happened".
