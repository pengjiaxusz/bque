
# Sub-Repository Architecture & Integration Modes Guide (REFERENCE.md)

This document details common sub-repository integration patterns, the mechanics of the two-tier semantic clustering engine, and safe update, exception handling, and rollback procedures.

---

## 1. Comparison of Common Sub-Repository Dependency Modes

In multi-repository architectures, host codebases typically incorporate sub-repositories via three primary modes:

| Dependency Mode | Applicable Scenario | Version Pinning Mechanism | Local Development Linking |
| :--- | :--- | :--- | :--- |
| **1. Git Submodule (Standard)** | Production standards, cross-platform UI libraries, shared core algorithms | Precise commit SHA (pointer) recorded by host repo | Redirect via `git checkout` or environment variables to sibling workspace |
| **2. CMake FetchContent / Cache Fetch** | C/C++ dependency packages, build-time downloads | Controlled by CMake scripts or pinned commit tags | Redirect to local folder via `-D<NAME>_DEV_LOCAL=ON` |
| **3. Local Sibling Worktree Co-Dev** | Daily bi-directional co-development, rapid feature prototyping | Symlinks, build bundler aliases (Vite alias / Cargo path) | Real-time hot module replacement (HMR), takes effect without packaging |

### Mode Switching & Alias Redirection Best Practices
In frontend setups (Vite / Webpack) or Rust/CMake builds, consider supporting environment-variable-driven path overrides:
- Defaults to `submodules/<name>` source;
- When `$env:<NAME>_DIR` is set or a sibling directory `..\<name>` exists, dynamically remap bundler aliases, avoiding manual source edits;
- Never hardcode absolute file paths into committed code.

---

## 2. Two-Tier Semantic Clustering Engine Mechanics

When updating a subrepo across dozens or hundreds of commits, manually reading git logs line by line is inefficient. `show-unapplied-commits.ps1` runs a two-tier semantic filter:

### Tier 1: Conventional Commits Classification
- **💥 Breaking Changes**: Matches `^[a-f0-9]+\s+[a-z0-9\-]+!:` or `BREAKING CHANGE:`.
  - Signifies breaking API changes, renamed properties, or behavioral shifts. Must be prioritized by Agent/developers to adapt host callers.
- **✨ Features (`feat:`)**: New functionality, components, or exports.
- **🐛 Bug Fixes (`fix:`)**: Bug fixes and boundary guards.
- **⚡ Performance (`perf:`)**: Performance optimizations.
- **♻️ Refactor (`refactor:`)**: Internal restructuring, usually non-breaking to public APIs.

### Tier 2: Domain Subsystem Keyword Matching
Regular expressions match subjects and bodies against key subsystem domains:
- **Motion & Transitions (Motion)**: `motion|easing|animate|transition`
- **Design Tokens (Tokens)**: `token|theme|palette|color`
- **Styles & Cascade Layers (Styles)**: `styles|cascade-layer|layer|subpath|css`
- **Standard Components (Components)**: `component|widget|control`

### Custom Categorization Rules (-ConfigPath)
To customize clustering for project-specific domains, supply a custom JSON config via `-ConfigPath`:
```json
{
  "categories": [
    {
      "name": "Shaders & Pipeline",
      "color": "Magenta",
      "regex": "(shader|hlsl|glsl|pipeline|render-pass)"
    },
    {
      "name": "Network & RPC Protocol",
      "color": "Cyan",
      "regex": "(proto|grpc|rpc|packet|socket)"
    }
  ]
}
```

---

## 3. Safe Updates, Error Handling & Rollback Procedures

### 1. Dirty Working Tree Guard
When invoking `-Update`, the script verifies subrepo `git status --short`:
- If the subrepo has uncommitted changes, checkout is aborted to protect local edits;
- Developers must `git stash` or commit inside the subrepo before running updates.

### 2. Detached HEAD Clarification
Git submodules detach HEAD to a specific commit upon checkout by design, as the host tracks exact commit SHAs.
- To develop inside the subrepo: create a new branch (`git checkout -b feat/my-change`), push to upstream, then advance host submodule pointer.

### 3. Pointer Rollback Emergency Workflow
If compilation fails or verification gates fail after upgrading:
```powershell
# 1. Unstage the submodule pointer change in host repo
git restore --staged submodules/<name>

# 2. Reset subrepo working tree back to currently recorded host commit
git submodule update --init --recursive submodules/<name>
```
