---
name: subrepo-sync
description: >-

  Universal sub-repository / submodule synchronization, commit impact analysis, and adaptation workflow. Extract unapplied commits between baseline and target, cluster changes by semantic Conventional Commits and domain subsystems (motion, tokens, styles, components, breaking changes), safe checkout/update, guide host refactoring and replacement of handwritten wheels, and enforce host verification gates. Use when updating Git submodules, CMake FetchContent dependencies, embedded sub-repos, or syncing downstream repos with subrepo changes. Triggers: subrepo-sync, submodule-sync, subrepo, submodule, update-subrepo, upgrade-subrepo, sync-submodule, 升级子仓库, 同步子仓库, 子模块升级, 子仓库升级.

---

<!-- PENGJ_TEMPLATE_START -->

# Sub-Repository Sync & Adaptation (Subrepo Sync)

Safely upgrade sub-repositories (Git submodules, CMake FetchContent targets, vendored repos), extract unapplied commit history, cluster impact across subsystems, refactor host code, and enforce project gates.

## Quick Start

Run the built-in detection script from repo root to inspect differences or update:

```powershell
# 1. Inspect unapplied commits and analyze impact clustering
pwsh .agents/skills/subrepo-sync/scripts/show-unapplied-commits.ps1 -SubrepoPath "<path/to/subrepo>"

# 2. Confirm and check out target commits directly
pwsh .agents/skills/subrepo-sync/scripts/show-unapplied-commits.ps1 -SubrepoPath "<path/to/subrepo>" -Update
```

## Standard Upgrade Workflow

```
[1. Baseline & Target] ──► [2. Unapplied Commits & Clustering] ──► [3. Safe Checkout / Update] ──► [4. Host Refactor & Adapt] ──► [5. Gate Verification & Commit]
```

### Step 1: Baseline & Target Discovery
1. Read current baseline commit: `git -C <subrepo-path> rev-parse HEAD`.
2. Discover target commit source:
   - Sibling local dev worktree (`..\<name>`);
   - Environment variable `$env:<NAME>_DIR`;
   - Remote upstream branch (`origin/main` or `origin/master`).
3. **When several pending commits each bumped the pin, settle the ordering before picking a target**:
   the new pin must be a **descendant** of the old one, otherwise the features you merge in
   **silently fail** (surfacing as missing properties or symbols at the API/QML layer).
   Read each commit's pin (`git show <sha>:<lockfile> | grep <PIN variable>`), run
   `git fetch origin <sha>` inside the subrepo first (the object is usually absent locally —
   **the fetch is mandatory**), then check `git merge-base --is-ancestor <old> <new>`;
   finally confirm the target capability really exists in the source tree actually used for builds.

### Step 2: Unapplied Commits & Semantic Impact Clustering
Extract commits via `git log <old>..<new> --oneline --no-merges` and cluster:
- **💥 Breaking Changes**: `feat!:`, `fix!:`, `BREAKING CHANGE:` ➔ Check signature & breaking API changes.
- **🎬 Motion & Transitions**: `motion`, `easing`, `animate` ➔ Verify exit transitions, easing curves, and duration collapse.
- **🎨 Tokens & Themes**: `token`, `theme`, `palette`, `color` ➔ Ensure foreground/background color coherence.
- **📐 Styles & Layers**: `styles`, `cascade-layer`, `layer`, `css` ➔ Ensure cascade layer order is preserved.
- **🧩 Canonical Components**: `component`, `feat(...)` ➔ Replace host handwritten wheels with upstream components.
- **⚡ Performance & 🐛 Fixes**: `perf:`, `fix:` ➔ Clean up temporary workarounds in the host.

### Step 3: Safe Checkout & Pointer Update
- Verify host and subrepo working trees are clean (`git status --short`).
- Checkout target commit in the subrepo (`git checkout <target-hash>` or `git submodule update`).

### Step 4: Host Refactoring & Adaptation
1. Replace duplicate host implementations with canonical components.
2. Comply with project architecture red lines (declared in the project-specific area below).

### ⚠️ Rebuild the Test Target First (Stale Artifact Trap)

A normal build usually **produces only the main executable, not the test executables**; the dependency's
modules (static libraries, resources, QML) are compiled into each test executable. If you run test gates
right after bumping the dependency pin, **the old executables load the new source tree's resources with
the old dependency modules**, producing a string of errors that look self-inflicted:

- `Cannot assign to non-existent property "<newly added property>"`;
- `Type <host component> unavailable`;
- If host code was changed in parallel, unrelated errors of the same shape appear too
  (same cause: the executable predates that property).

**These are stale-artifact false positives, not regressions** — rebuild/relink the test target and they
disappear. Before investigating, confirm the test executables were built *after* the dependency update.

### Step 5: Verification Gates & Conventional Commit
1. Run host compilation, static checks, and unit tests.
2. Run project hygiene and UI verification gates.
3. Commit host changes using Conventional Commits: `chore(deps): update <subrepo> to <short-hash> and adapt <changes>`.

## Progressive Disclosure & Reference

- [Detailed Architecture & Integration Modes](REFERENCE.md)
- [Real-World Upgrade & Refactoring Examples](EXAMPLES.md)

<!-- PENGJ_TEMPLATE_END -->

<!-- 以下为项目专属区域：模板更新只替换上方托管块，本区域归项目所有、完整保留。 -->
## 项目专属子仓库配置与门禁契约

> 在此处声明本项目的子仓库路径、领域专属聚类关键词、宿主验证门禁与架构红线。

### 1. 托管子仓库登记
| 子仓库名称 | 相对路径 | 依赖模式 (Submodule / FetchContent / 本地) | 上游跟踪分支 |
| :--- | :--- | :--- | :--- |
| `example-subrepo` | `submodules/example-subrepo` | Git Submodule | `origin/main` |

### 2. 宿主架构红线
- **单真相源驱动**：严禁在依赖缓存目录中直接修改或分叉；所有通用组件与契约必须源自 upstream。
- **组件自举律**：上游一旦推出规范控件，必须及时重构并移除宿主自研的重复手写实现。

### 3. 项目专属验证门禁
```powershell
# 静态与编译检查
# cargo check --workspace
# pnpm build

# 自动化测试与卫生门禁
# cargo test --workspace
# pwsh tools/hygiene.ps1
```