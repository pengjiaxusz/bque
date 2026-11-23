---
name: template-sync
description: >-

  Template synchronization, adoption, and audit workflow for pengj-templates. Automatically diagnoses tech stack, paradigms, and commit habits to recommend tailored profiles when adopting projects (adopt); audits template drifts, script tampering, and managed block violations (audit); updates managed projects from upstream templates (update); resolves review notices (needs_review / conflicted / transition zones).
  Triggers: template-sync, sync-template, update-template, apply-template, adopt-project, template-audit, template-diff.

---

<!-- PENGJ_TEMPLATE_START -->

# Template Sync (Update, Adopt & Audit)

Workflow for auditing downstream project alignment, updating downstream projects from upstream `pengj-templates`, or adopting templates into existing repositories.

## Quick Start

### Fast-Track Pipeline (Recommended)

Prefer running the bundled fast-track pipeline script `sync-template.ps1`. It detects precompiled binaries for sub-second startup, supports batch project directories, strictly enforces the selective staging red line (never `git add .`), and can close the loop (audit -> update -> selective add -> commit -> push) in a single tool call:

```powershell
# Fast-track dry-run audit for one or multiple projects
pwsh .agents/skills/template-sync/scripts/sync-template.ps1 -Projects @("<project-dir-1>", "<project-dir-2>")

# Fast-track one-click batch update with selective staging, commit and push (default)
pwsh .agents/skills/template-sync/scripts/sync-template.ps1 -Projects @("<project-dir-1>", "<project-dir-2>") -Apply
```

### Direct CLI Commands

Run from `pengj-templates` repo or ensure `pengj-templates-cli` is in PATH / `PENGJ_TEMPLATES` is set:

```powershell
# 0. Audit template alignment & drifts (supports single --dir or batch --dirs)
cargo run -p pengj-templates-cli -- audit --dirs <dir1,dir2> --diff

# 1. Update managed projects (with --sync-skills to align pure skill assets without blocks)
cargo run -p pengj-templates-cli -- update --dirs <dir1,dir2> --sync-skills

# 2. Adopt / Apply templates to an existing project (no manifest yet)
cargo run -p pengj-templates-cli -- adopt --dir <project-dir> --layers <layer1,layer2> [options]
```

## Workflow

### 1. Diagnose & Select Action

Check whether the target project has `.pengj-templates/manifest.json`:

- **Inspection / Verification**: Run **Audit Mode** (Step 2C) to detect whether downstream has drifted or violated template rules.
- **Manifest exists**: The project is already managed. Proceed to **Update Mode** (Step 2A).
- **Manifest does NOT exist**: The project is not yet managed. Proceed to **Adopt Mode** (Step 2B).

### 2A. Update Mode (Sync Upstream Changes)

1. Run the fast-track script or CLI update:
   ```powershell
   # Recommended: fast-track script (commits and pushes the applied project by default)
   pwsh .agents/skills/template-sync/scripts/sync-template.ps1 -Projects @("<project-dir>") -Apply

   # Or via CLI directly (then follow Step 4 to commit the applied project)
   cargo run -p pengj-templates-cli -- update --dir <project-dir> --sync-skills
   ```
2. The engine reads `.pengj-templates/manifest.json` to reuse original options (`edition`, `skills`, `skill_lang`, etc.) and performs baseline-driven sync:
   - **Pure skill assets (`.agents/skills/`)**: When `--sync-skills` is enabled (default in fast-track script), pure skills and helper scripts without managed blocks are cleanly updated to the latest template without raising false conflicts.
   - **Unmodified files**: Overwritten cleanly with the latest template.
   - **Files with managed blocks (`PENGJ_TEMPLATE_START/END`)**: Managed block is replaced in place; user-owned area outside is preserved.
   - **TOML / JSON (`.cargo/config.toml`, `package.json`)**: Structured union merge.
   - **Unmanaged modified files**: Flagged as `conflicted` and skipped (never silently overwritten).

### 2B. Adopt Mode (Adoption & Recommendation for New/Existing Projects)

When adopting an unmanaged new or existing repository, **never run adopt blindly without user alignment**. The AI must first perform an automated project diagnosis, generate a recommended profile, and actively consult the user:

#### 1. Automated Tech Stack & Feature Diagnosis (Infer Recommended Profile)

Inspect the target project workspace across the following dimensions:
- **Tech Stack & Layer Compatibility**:
  - Check project files: If `Cargo.toml` exists, it matches the Rust stack — recommend `rust` or `rust-workspace`. **Strictly avoid Rust layers for non-Rust repositories**.
  - Check toolchains: does the repo need local Git hook commit enforcement? If yes, recommend the `gate` layer — it is a pure Rust implementation with no Node dependency, and its hooks call `pengj-templates-cli` (one binary covers the template CLI, the MCP installer and the gate). For projects that only want files and no local hook enforcement, default to excluding `gate`, but present it as an optional enhancement.
  - Editor settings: Recommend `vscode` to inject file nesting patterns (`explorer.fileNesting`); existing `.vscode` configs merge automatically.
  - `common` and `agent` are foundational and core layers for all projects (always required).
- **Programming Paradigm Detection (`--chinese` Chinese Programming)**:
  - Check for non-ASCII or Chinese directory names or prevalent non-English/Chinese identifiers (classes, methods, variables).
  - If detected, **strongly recommend `--chinese`** to inject Chinese programming guidelines and bilingual naming anchors.
- **Commit Conventions & Language Detection (`--commit-zh` / `--commit-and-push`)**:
  - Run `git log -n 5 --oneline` to inspect commit styles.
  - If commits use Chinese subjects/bodies, keep `--commit-zh` enabled (default true).
  - **Strongly recommend `--commit-and-push`** to enforce the "Pre-Response Commit Gate", ensuring the AI commits and pushes verified changes before concluding turns.
- **Existing Rules & Asset Assessment**:
  - If `AGENTS.md` exists, inform the user that managed blocks will be prepended at the top, cleanly preserving existing project-specific rules below.
  - If legacy custom skills exist, note that they will be taken over into transition zones (`needs_review`).
- **Skill Selection (`--skills`)**:
  - Core baseline: `commit` (Conventional Commits), `template-sync` (upstream updates).
  - Architecture & Collaboration: `write-docs` (documentation writing & updates, rules owned by the project), `git-workflow` (universal git, conventional commits & linear branch sync).
  - Utilities: `caveman` (token compression), `grill-me` / `grilling` (design grilling), `subrepo-sync` (submodule sync), `write-a-skill` (authoring skills).
  - Engineering & Productivity Suites (from `mattpocock/skills`): `ask-matt`, `code-review`, `codebase-design`, `diagnosing-bugs`, `domain-modeling`, `grill-with-docs`, `implement`, `implement-spec`, `improve-codebase-architecture`, `pr`, `prototype`, `research`, `retro`, `setup-matt-pocock-skills`, `tdd`, `to-spec`, `to-tickets`, `triage`, `wayfinder`, `wizard`, `handoff`, `teach`, `to-questionnaire`, `wait-what`, `writing-for-agents`. Default to all skills or tailored subsets.

#### 2. Present Diagnosis & Consult the User (Provide a Recommended Profile)

Present a structured diagnostic report to the user:
- **Project Diagnosis Summary**: Tech stack, Chinese programming style, commit conventions, existing asset safety;
- **Primary Recommendation (Marked `(Recommended)`)**: Specific layers (e.g. `--layers common,agent,vscode`), options (e.g. `--chinese`, `--commit-and-push`), and the technical rationale;
- **Alternative / Add-on Profiles**: Feasible variations (e.g. adding the `gate` layer for local commit enforcement);
- **Command Preview**: Exact CLI command preview;
- **Consultation**: Ask the user via structured choice (e.g. `ask_question` or interactive prompt) for confirmation or adjustments.

#### 3. Execute Adopt Command

Upon user confirmation, execute:
```powershell
cargo run -p pengj-templates-cli -- adopt --dir <project-dir> --layers <layer1,layer2> [options]
```
*(Note: `--dir` specifies the target project directory; do not pass bare positional arguments).*

### 2C. Audit Mode (Template Drift Inspection & Governance)

Run audit to detect whether downstream has drifted or tampered with template-managed assets:
```powershell
cargo run -p pengj-templates-cli -- audit --dir <project-dir> --diff
```

**Governance Decision Tree**:
- **`ProjectCustomized` (Safe)**: Content added outside `PENGJ_TEMPLATE_START/END` in the project-specific area. Benign and expected.
- **`UpstreamNewer` (Safe to update)**: Upstream template evolved; downstream untouched. Run `update` to cleanly sync.
- **`ViolatedManagedBlock` / `LocallyModifiedFile` (Violation / Critical Drift)**:
  - **Rule 1 (Universal Value)**: If the downstream modification addresses a common limitation (e.g. branch auto-detection, toolchain adaptation), contribute it to upstream `pengj-templates`, make it generic, and run `update` to eliminate the downstream hack!
  - **Rule 2 (Project Environment)**: If it is project-specific (e.g. untracked generated directories, build flags), move the configuration to the project-specific area outside the managed block (e.g. `SKILL.md` ignore regex), and restore the managed file to pure template!
  - **Rule 3 (Strict Anti-tampering)**: Never keep private hacks in template scripts or inside managed blocks in downstream projects.

### 3. Post-Processing & Conflict Resolution

Inspect the CLI execution report:

- **`needs_review` — Transition Zone Merge**:
  - If a legacy custom skill was taken over, the template body is applied and the original custom content moves into `<!-- === PENGJ_ADOPT_TRANSITION_ZONE === -->`.
  - Assist the user in moving domain-specific checks from the transition zone into the project-specific area outside the managed block.
  - **Delete the transition zone comment and content** after merging.
- **`conflicted` — Unmanaged Modified Files**:
  - Review `path` and `reason`. Compare diffs and manually apply upstream template updates if needed.
- **Toolchain / Workspace Wiring**:
  - Verify the gate layer landed as expected (`.pengj-templates/gate.toml` plus `.githooks/*`), and that `.vscode/settings.json` or `.cargo/config.toml` merged as expected.

### 4. Verify & Auto-Commit the Applied Project

1. Switch to the target project directory and run verification checks matching its stack:
   ```powershell
   # e.g., for Rust
   cargo check && cargo test
   # e.g., for Frontend
   pnpm build
   ```
2. **Auto-commit policy & selective staging (Red line)**:
   - **Commit and push directly — never ask**: after a successful `update` / `adopt`, the Agent MUST commit the template sync in the **applied project** and push it immediately. Do NOT stop to ask the user "shall I commit / shall I push?". This is pre-authorized and unconditional — never leave applied template updates sitting in the worktree waiting for confirmation; "asking every time" and "never committing" are exactly the behaviours being eliminated here.
   - **Commit the right repository**: the commit above belongs to the downstream project that was just applied. Changes to the upstream `pengj-templates` repository (template sources, engine, docs) are committed separately in that repository, never mixed in.
   - **Batch sync commits per repository**: when one `update` touches multiple projects, **every applied project must be committed and pushed separately** (each following its own `commit_zh` and gates) — none may be left dirty. For a project with submodules, commit and push the child repository first, then the parent's pointer bump. If a project's worktree still carries **someone else's uncommitted edits** (a parallel session or the user mid-edit), stage only this sync's paths and commit, then **skip that repository's push** and never format or repair those files — the session owning them pushes.
   - **Strict selective staging**: stage ONLY the paths listed in this run's update report (`[Updated]` / `[Added]` plus `.pengj-templates/manifest.json`). NEVER `git add .` or `git commit -a`, so pre-existing unrelated or dirty working-tree changes are not swept in.
   - **Message language follows the target project**: read `options.commit_zh` from the target's `.pengj-templates/manifest.json`. When true the subject MUST be written in Chinese (see the target's `commit.md` for examples), when false it MUST be English (e.g. `chore: sync layered template updates`). The commit-msg hook enforces this.
   - **Opt out** only when the user explicitly asked not to commit beforehand: pass `-NoCommit` / `-NoPush` to the fast-track script, or skip Step 4 manually. Absent such an explicit instruction, commit and push without asking.

<!-- PENGJ_TEMPLATE_END -->

<!-- Project-specific section below: template updates only replace the managed block above; this section belongs to the project and is preserved in full. -->
## Project-Specific Sync Configuration & Shortcuts

> Register this project's upstream template URL, default layer set, and pre/post-sync scripts here.