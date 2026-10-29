
# Commit & Push Workflow

Conventional commit workflow with pre-response commit gates, semantic splitting, and immediate push protection.

---

## 1. Trigger Conditions (Mandatory Proactive Triggers)

- **Code modification & milestone completion (Mandatory barrier — no accumulation)**:
  Whenever files are modified and verified in the current turn, you MUST run this commit workflow (pre-commit checks, split commits, and `git push`) before reporting back to the user or proceeding to the next step. Never leave verified changes sitting in the working directory before yielding control; writing `walkthrough.md` in planning modes (`/plan` / `/boost`) does NOT conclude the task — you must commit and push immediately after.
- **Task conclusion (Mandatory)**:
  Before concluding any development or maintenance task or delivering final responses, ensure the working tree is clean and all commits are pushed to the remote repository.
- **Explicit user instruction**:
  When user invokes `/commit`, "commit", "commit & push", etc.

---

## 2. Gather Context (Standard Cross-Platform Git)

Run from repository root:

```bash
git status
git --no-pager diff --stat
git --no-pager diff --cached --stat
```

---

## 3. Pre-Commit Completeness Check

This step belongs to the **PROJECT**, not the template:
Follow whatever the project-specific area in `SKILL.md` defines (commands, test suites, checklists). If the project-specific area defines no checks, use the generic fallback:
- Scan the diff: if it touches build/deps, docs/AGENTS.md/config, or public naming, verify build / update docs / run formatter as appropriate before committing.
- **Architecture doc consistency check**: If this change touches architecture (new/adjusted module boundaries, data flow, lifecycle, system topology, core invariants, etc.), verify `docs/architecture/README.md` and the corresponding domain docs are in sync; see `.agents/skills/arch-align` for details.

Do NOT invent arbitrary checks on the fly; reference the project-specific area.

---

## 4. Semantic Splitting

Separate unrelated areas into distinct commits (e.g., do not mix documentation updates with core feature implementations or build script refactorings).

---

## 5. Commit Message Format

Format: `type(scope): subject`


- **Subject in English** (imperative, short, no trailing period; strictly forbidden to contain Chinese characters).
  - ❌ Forbidden: `feat(cli): 新增参数支持`
  - ✅ Correct: `feat(cli): add new flags`
- **Type/Scope in English**:
  - Types: `feat` `fix` `docs` `style` `refactor` `perf` `test` `build` `ci` `chore` `revert`
  - Scope: from the project's `commitlint.config.js` scope-enum whitelist when present.


---

## 6. Commit & Push Execution

```bash
# Non-interactive commit
git commit -m "<type>(<scope>): <subject>"

# Push immediately
git push

# If remote diverged, rebase silently and push
git pull --rebase --autostash
git push
```

---

## 7. Amend (Amend Guardrail)

Amend is ONLY permitted when explicitly requested by the user, and ONLY for the most recent, unpushed commit that nobody else depends on:

```bash
# MUST supply --no-edit or explicit -m to avoid popping up editors
git commit --amend --no-edit
git push
```
