---
name: bque
description: >-

  Universal cross-worktree build queue serialization, CPU throttling, and clean-room benchmark protection skill. Enforces Idle priority, clean-room benchmark execution (auto-freezing background builds), queue introspection, pipeline hygiene (preventing 4KB anonymous pipe deadlocks on Windows), and deadlock recovery. Use when running builds, cargo, cmake, ninja, pnpm build, tests, benchmarks, profiling, or when encountering CPU contention, hanging commands, or build lock errors.
  Triggers: bque, build-queue, queue, build queue, run build, cargo build, pnpm build, cmake build, bench, benchmark, profiling, queue-status, queue-clean, compile freeze, cpu throttle, pipeline deadlock.

---

<!-- PENGJ_TEMPLATE_START -->

# bque — Universal Build Queue, CPU Governor & Clean-Room Benchmark Protector

A cross-worktree build queue and governor skill. Prevents multi-repository build collisions, keeps foreground interactivity silky-smooth via `Idle` priority lowering, guarantees 0% CPU background disturbance for benchmarks via process freezing, and enforces pipeline hygiene.

---

## ⚡ 1. When to Use vs When NOT to Use bque

| Scenario | Use bque? | Mode / Command | Reason |
| :--- | :---: | :--- | :--- |
| **Heavy Compilation** (`cargo build`, `cmake`, `ninja`, `pnpm build`, `tsc`, `xtask pack`) | **YES** | `just build` or `tools/bque-runner.ps1 -Cmd "<cmd>"` | Serializes builds, lowers process priority to `Idle` to avoid freezing the OS |
| **Workspace Lint & Test** (`cargo check`, `cargo clippy`, `cargo test`, `vitest run`) | **YES** | `just check` / `just test` or `bque-runner.ps1 -Cmd "<cmd>"` | High CPU utilization tasks require queue mutual exclusion and background throttling |
| **Benchmarks & Profiling** (`bench`, `perf`, timing probes) | **YES** | `just bench` or `tools/bque-runner.ps1 -Bench -Cmd "<cmd>"` | **Clean-room mode**: automatically freezes background builds to ensure 0% CPU disturbance |
| **Nested / Recursive invocation** (`BQUE_ACTIVE_PID` set) | **NO** | Direct command (`& <cmd>`) | Already under bque governor supervision; avoids recursive queue deadlock |
| **Fast Formatting & Lint** (`cargo fmt`, single-file `prettier`/`eslint`) | **NO** | Direct command | Fast completion (<1s), negligible CPU usage, no need to queue |
| **Git Operations** (`git status`, `commit`, `push`, `rebase`) | **NO** | Direct command | I/O-bound operational commands, must never queue behind builds |
| **State Inspection** (`queue-status`, `queue-clean`) | **NO** | `just queue-status` / `just queue-clean` | Control and diagnostic tools must run immediately |

See [references/usage-matrix.md](references/usage-matrix.md) for full scenario matrix and command mappings.

---

## 🛑 2. Strictly Prohibited Raw Commands (Mandatory Wrappers)

AI assistants are **STRICTLY PROHIBITED** from executing raw build or compiler commands directly in the terminal! Bypassing `bque` / `just` starves all CPU cores, freezes desktop interactivity, contaminates benchmark accuracy, and triggers Windows 4KB pipe deadlocks. Always convert raw commands into their wrapped forms:

| Prohibited Raw Command (FORBIDDEN) | Hazards & Failure Modes | Mandatory Wrapped Alternative |
| :--- | :--- | :--- |
| ❌ `ninja -C <dir> <target>` / `ninja ...` | Consumes 100% CPU cores, freezes the system, causes worktree conflicts | `just test-build <target>` or `pwsh tools/bque-runner.ps1 -Cmd "ninja -C <dir> <target>"` or `bque run -- ninja -C <dir> <target>` |
| ❌ `cmake --build <dir> ...` | Causes target lock collisions, starves foreground responsiveness | `pwsh tools/locked-cmake-build.ps1 ...` or `just build` or `pwsh tools/bque-runner.ps1 -Cmd "cmake --build ..."` |
| ❌ `cargo build` / `cargo test` / `cargo clippy` | Cargo target directory lock collisions, heavy CPU contention | `just build` / `just test` / `just check` or `pwsh tools/bque-runner.ps1 -Cmd "cargo ..."` |
| ❌ Raw Benchmark invocation (e.g. `ninja -C target/debug DuntingThumbnailPipelineBenchmark` or `./bench.exe`) | **Fatal Benchmark Corruption**: background builds pollute CPU cache, invalidating benchmark data | `just test-bench` or `pwsh tools/bque-runner.ps1 -Bench -Cmd "..."` or `bque bench -- ...` (auto-freezes background builds for 0% CPU disturbance) |
| ❌ `ctest ...` / `<TestBinary>.exe` | Unthrottled multi-threaded test runner saturates CPU | `just test` or `pwsh tools/run-tests.ps1` or `pwsh tools/bque-runner.ps1 -Cmd "ctest ..."` |
| ❌ `pnpm build` / `tsc` (in large frontend workspaces) | V8 engine multi-thread memory and CPU spikes | `just build` or `pwsh tools/bque-runner.ps1 -Cmd "pnpm build"` |

---

## 🚀 3. Three Execution Tiers (Priority Order)

### Tier 1: Unified `just` Entry (Recommended)
Always check the repository's `justfile` first. Standard targets automatically invoke the bque runner:
```powershell
just build           # Compile under bque queue & Idle priority
just check           # Check code under bque supervision
just test            # Run test suite safely
just bench           # Clean-room benchmark (freezes background builds)
just queue-status    # Inspect current lock holder and waiters
just queue-clean     # Clean abnormal or abandoned locks
```

### Tier 2: Universal Runner Delegation (When no matching just recipe exists)
If the project has `tools/bque-runner.ps1` and you need a custom build command:
```powershell
# Standard queue execution (-Cmd preserves complex flags and switches)
pwsh tools/bque-runner.ps1 -Desc "Custom Build" -Cmd "cargo build --release"

# Clean-room benchmark execution
pwsh tools/bque-runner.ps1 -Bench -Desc "Custom Benchmark" -Cmd "pnpm bench"
```

### Tier 3: Direct Native CLI (Raw bque commands)
If running outside a repository or calling bque directly:
```powershell
bque run -d "Heavy Task" -p idle -- cargo build --workspace
bque bench -d "Performance Probe" -- cargo bench
bque status
bque clean
```

---

## 🛡️ 4. Two Hard Ironclad Rules

1. **Pipeline Hygiene (Strictly Forbidden)**:
   - **FORBIDDEN**: Never pipe native console build commands into PowerShell truncation filters (e.g., `cargo test 2>&1 | Select-Object -First 100` # pipeline-hygiene: ok anti-pattern doc).
   - **Root Cause**: Windows native processes do not handle PowerShell cooperative breaks; when the 4KB anonymous pipe buffer fills up, the process blocks on write, resulting in a permanent 120s deadlock.
   - **Solution**: Always redirect to disk first, then inspect: `cargo test *> out.log; Get-Content out.log -TotalCount 100`.
2. **Process Termination Hygiene**:
   - When cleaning or rebuilding, **never kill processes by name blindly** (`Stop-Process -Name ...`).
   - Filter strictly by the current worktree's path before terminating to avoid killing processes in other working trees.

---

## 🔧 5. Environment & Installation

If `bque` is missing from system `PATH`, inspect and follow the installation guide:
👉 See [references/install.md](references/install.md).


<!-- PENGJ_TEMPLATE_END -->

<!-- 以下为项目专属区域：模板更新只替换上方托管块，本区域归项目所有、完整保留。 -->
## 项目专属排队策略与定制配置

> 可在此登记本项目特殊的耗时任务标签（如 Heavy 测试、特殊离线渲染等）、专属超时阈值或前置环境准备脚本。