---
name: bque
description: >-

  Universal cross-worktree build queue serialization, CPU throttling, and clean-room benchmark protection skill. Enforces Idle priority, clean-room benchmark execution (auto-freezing background builds), queue introspection, pipeline hygiene (preventing 4KB anonymous pipe deadlocks on Windows), and deadlock recovery. Use when running builds, cargo, cmake, ninja, pnpm build, tests, benchmarks, profiling, or when encountering CPU contention, hanging commands, or build lock errors.
  Triggers: bque, build-queue, queue, build queue, run build, cargo build, pnpm build, cmake build, ninja, ninja -C, cmake, ctest, cargo test, cargo clippy, bench, benchmark, profiling, queue-status, queue-clean, compile freeze, cpu throttle, pipeline deadlock.

---

<!-- PENGJ_TEMPLATE_START -->

# bque — Universal Build Queue, CPU Governor & Clean-Room Benchmark Protector

A cross-worktree build queue and governor skill. Prevents multi-repository build collisions, keeps foreground interactivity silky-smooth via `Idle` priority lowering, guarantees 0% CPU background disturbance for benchmarks via process freezing, and enforces pipeline hygiene.

---

## ⚡ 1. When to Use vs When NOT to Use bque

| Scenario | Use bque? | Mode / Command | Reason |
| :--- | :---: | :--- | :--- |
| **Heavy Compilation** (`cargo build`, `cmake`, `ninja`, `pnpm build`, `tsc`, `xtask pack`) | **YES** | `just build` or `bque run -- <cmd>` | Serializes builds, lowers process priority to `Idle` to avoid freezing the OS |
| **Workspace Lint & Test** (`cargo check`, `cargo clippy`, `cargo test`, `vitest run`) | **YES** | `just check` / `just test` or `bque run -- <cmd>` | High CPU utilization tasks require queue mutual exclusion and background throttling |
| **Benchmarks & Profiling** (`bench`, `perf`, timing probes) | **YES** | `just bench` or `bque bench -- <cmd>` | **Clean-room mode**: automatically freezes background builds to ensure 0% CPU disturbance |
| **Long-running service** (dev server, watch loop, `xtask dev`, `pnpm dev`) | **YES, observer mode** | `bque run --observer -- <cmd>` | Same priority / CPU budget, but never takes the global build lock — a server holding it blocks every other build |
| **Nested / Recursive invocation** (`BQUE_ACTIVE_PID` set) | **NO** | Direct command (`& <cmd>`) or `bque run` (native auto-bypass) | Already under bque governor supervision; avoids recursive queue deadlock |
| **Fast Formatting & Lint** (`cargo fmt`, single-file `prettier`/`eslint`) | **NO** | Direct command | Fast completion (<1s), negligible CPU usage, no need to queue |
| **Git Operations** (`git status`, `commit`, `push`, `rebase`) | **NO** | Direct command | I/O-bound operational commands, must never queue behind builds |
| **State Inspection** (`status`, `clean`) | **NO** | `bque status` / `bque clean` | Control and diagnostic tools must run immediately |

See [references/usage-matrix.md](references/usage-matrix.md) for full scenario matrix and command mappings.

---

## 🛑 2. Strictly Prohibited Raw Commands (Mandatory Wrappers)

AI assistants are **STRICTLY PROHIBITED** from executing raw build or compiler commands directly in the terminal! Bypassing `bque` / `just` starves all CPU cores, freezes desktop interactivity, contaminates benchmark accuracy, and triggers Windows 4KB pipe deadlocks. Always convert raw commands into their wrapped forms:

| Prohibited Raw Command (FORBIDDEN) | Hazards & Failure Modes | Mandatory Wrapped Alternative |
| :--- | :--- | :--- |
| ❌ `ninja -C <dir> <target>` / `ninja ...` | Consumes 100% CPU cores, freezes the system, causes worktree conflicts | `just test-build <target>` or `bque run -- ninja -C <dir> <target>` |
| ❌ `cmake --build <dir> ...` | Causes target lock collisions, starves foreground responsiveness | `just build` or `bque run -- cmake --build <dir> ...` |
| ❌ `cargo build` / `cargo test` / `cargo clippy` | Cargo target directory lock collisions, heavy CPU contention | `just build` / `just test` / `just check` or `bque run -- cargo ...` |
| ❌ Raw Benchmark invocation (e.g. `ninja -C target/debug DuntingThumbnailPipelineBenchmark` or `./bench.exe`) | **Fatal Benchmark Corruption**: background builds pollute CPU cache, invalidating benchmark data | `just test-bench` or `bque bench -- <cmd>` (auto-freezes background builds for 0% CPU disturbance) |
| ❌ `ctest ...` / `<TestBinary>.exe` | Unthrottled multi-threaded test runner saturates CPU | `just test` or `bque run -- ctest ...` |
| ❌ `pnpm build` / `tsc` (in large frontend workspaces) | V8 engine multi-thread memory and CPU spikes | `just build` or `bque run -- pnpm build` |

---

## 🚀 3. Two Execution Tiers (Priority Order)

### Tier 1: Unified `just` Entry (Recommended)
Always check the repository's `justfile` first. Standard targets automatically invoke bque:
```powershell
just build           # Compile under bque queue & Idle priority
just check           # Check code under bque supervision
just test            # Run test suite safely
just bench           # Clean-room benchmark (freezes background builds)
bque status          # Inspect current lock holder and waiters
bque clean           # Clean abnormal or abandoned locks
just doctor          # Verify toolchain, bque installation, PATH fallback and lock state
```

When no matching recipe exists, go straight to the native CLI (`bque run -- <cmd>`) — there is no
ad-hoc `just` wrapper. A wrapper can only forward the command, never bque's own flags (`--cwd`,
`--observer`, `--priority`), which is exactly why it was retired: wrap the *specific* command in a
project recipe instead. The `justfile` framework lives in a managed block
(`# PENGJ_TEMPLATE_START/END`); project-owned recipes stay outside it and survive template updates.
On Unix and in CI the `[unix]` variants of these recipes pass through without bque — that is
expected, since bque is a Windows-only global lock; never force a raw `bque` call there.

### Tier 2: Direct Native `bque` CLI (Custom / Ad-hoc Commands)
When no matching `just` recipe exists, directly invoke `bque`:
```powershell
# Standard queue execution (defaults to Idle priority, native reentrancy bypass)
bque run -- cargo build --release

# Clean-room benchmark execution (High priority, auto-freezes background builds)
bque bench -- pnpm bench

# Introspection and queue cleanup
bque status
bque clean

# Long-running service: same priority, but no global build lock
bque run --observer -- cargo run -- dev
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

Run `just doctor` first: it probes `PATH`, falls back to `$env:LOCALAPPDATA\bque\bin`
and prints the one-click install command when bque is missing. For the full setup guide:
👉 See [references/install.md](references/install.md).


<!-- PENGJ_TEMPLATE_END -->

<!-- Project-specific section below: template updates only replace the managed block above; this section belongs to the project and is preserved in full. -->
## Project-Specific Queue Policy & Custom Configuration

> Register project-specific heavy task tags, custom timeout thresholds, or prerequisite environment scripts here.