---
name: bque
description: >-

  Cross-platform (Windows & Linux/WSL2) build queue with 100-unit budget scheduling, automatic low-CPU yielding, mutual-exclusion scopes, stall/duration watchdogs (exit 124), CPU throttling, clean-room benchmark protection, and Windows 4KB pipe hygiene. Use when running builds, cargo, cmake, ninja, pnpm build, tests, benchmarks, profiling, or when encountering CPU contention, hanging commands, or build lock collisions.
  Triggers: bque, build-queue, queue, build queue, run build, cargo build, pnpm build, cmake build, ninja, ninja -C, cmake, ctest, cargo test, cargo clippy, bench, benchmark, profiling, queue-status, queue-clean, compile freeze, cpu throttle, pipeline deadlock, stall-timeout, yielding, budget.

---

<!-- PENGJ_TEMPLATE_START -->

# bque — Cross-Platform Budget Scheduler, Activity Watchdog & Benchmark Protector

A daemonless build queue and resource governor for Windows and Linux/WSL2. Instead of a blind single-slot lock, `bque` coordinates tasks through a **100-unit shared Budget**, automatically lends capacity (**Yielding**) when a running task stays at low CPU, protects shared build directories with optional **Scope** locks, catches hung tasks with opt-in **Watchdogs**, and freezes background builds for 0% CPU interference during benchmarks.

---

## 1. Decision Table: Weight Tiers, Modes & Bypass

| Scenario | Use bque? | Recommended Invocation | Reason |
| :--- | :---: | :--- | :--- |
| **Heavy compilation** (`cargo build`, `cmake --build`, `ninja`, `pnpm build`, `tsc`) | **YES (`heavy`)** | `just build` or `bque run -- <cmd>` | Claims default `weight=100` (serializes active heavy builds); yields automatically if CPU stays `<5%` for 10s |
| **Workspace check / full test** (`cargo clippy --workspace`, `cargo test --workspace`) | **YES (`heavy` or `medium`)** | `just check` / `just test` or `bque run --weight medium -- <cmd>` | Throttled to `Idle`; `medium` (`50`) allows two moderate tasks to overlap when safe |
| **Targeted single-crate check / unit test / linter** | **YES (`light`)** | `bque run --weight light -- cargo check -p <crate>` | Claims `25/100` budget; runs concurrently with other light tasks and backfills idle capacity |
| **Shared build directory collision risk** (e.g. same CMake/Ninja build dir) | **YES (`--scope`)** | `bque run --scope <dir-key> -- <cmd>` | Strict mutual exclusion per scope — never overlaps even if the holder enters `Yielding` |
| **Automated / Agent long commands** (risk of hangs or infinite loops) | **YES (Watchdog)** | `bque run --stall-timeout 300 --max-duration 1800 -- <cmd>` | Kills child tree and exits **`124`** on `<0.5%` CPU stall or hard duration limit while keeping native TTY |
| **Benchmarks & Profiling** (`cargo bench`, `perf`, timing probes) | **YES (`bench`)** | `just bench` or `bque bench -- <cmd>` | **Clean-room mode**: freezes all active tasks & observers with RAII auto-resume; elevates to `High` |
| **Long-running service** (dev server, watch loop, `xtask dev`, `pnpm dev`) | **YES (`--observer`)** | `bque run --observer -- <cmd>` | Registered in status and throttled, but consumes `0` Budget and never blocks waiters |
| **Nested / recursive call** (`BQUE_ACTIVE_PID` set) | **NO** | Direct command or `bque run` (native auto-bypass) | Already supervised by outer `bque`; avoids nested queuing |
| **Fast formatting & Git** (`cargo fmt`, `git status`, `commit`, `push`) | **NO** | Direct command | Sub-second or I/O/network ops must never queue behind builds |
| **Queue control & inspection** (`status`, `clean`, `pause`, `resume`, `probe`) | **NO** | `bque status` / `bque clean` / `bque pause` | Control-plane commands execute immediately |

Full command mapping and recovery details: [references/usage-matrix.md](references/usage-matrix.md).

---

## 2. Prohibited Raw Commands (Mandatory Wrappers)

AI assistants must **never** run raw heavy build, test, or benchmark commands directly in the terminal on managed workstations. Always convert them to wrapped forms:

| Prohibited Raw Command | Hazards | Mandatory Wrapped Alternative |
| :--- | :--- | :--- |
| `ninja -C <dir> <target>` | Saturates all cores, freezes desktop, collides across worktrees | `just test-build <target>` or `bque run --scope <dir> -- ninja -C <dir> <target>` |
| `cmake --build <dir> ...` | Build directory lock contention, starves foreground UI | `just build` or `bque run --scope <dir> -- cmake --build <dir> ...` |
| `cargo build` / `cargo test` / `cargo clippy` | CPU saturation and cross-worktree contention | `just build` / `just test` / `just check`, or `bque run [--weight light] -- cargo ...` |
| Raw benchmark binary / `cargo bench` | Background builds pollute L3 cache and skew timings | `just bench` or `bque bench -- <cmd>` |
| `ctest ...` / `<TestBinary>.exe` | Unthrottled multi-threaded test bursts | `just test` or `bque run -- ctest ...` |
| `pnpm build` / `tsc` (large workspaces) | V8 CPU and memory spikes | `just build` or `bque run -- pnpm build` |

---

## 3. Two Execution Tiers & Platform Boundary

### Tier 1: Unified `just` Entry (Preferred)
Check the repository's `justfile` first; standard targets already wire `bque`:
```powershell
just build           # Queued build at Idle priority (default weight=100, auto-yielding)
just check           # Queued clippy/check
just test            # Queued test suite
just bench           # Clean-room benchmark (freezes active tasks & observers)
bque status          # Inspect active tasks, Budget (used/100), live CPU%, waiters
bque clean           # Reap dead records and auto-unfreeze orphaned frozen processes
just doctor          # Verify toolchain, bque version, and host resource tier (`bque probe`)
```

**Platform note**: `bque` works natively on **both Windows and Linux/WSL2** (cross-platform budget scheduling, yielding, watchdogs, and freezing). In generated `justfile` templates, `[unix]` recipes intentionally pass through without `bque` so CI runners need zero extra setup; on Linux/WSL2 workstations with `bque` installed, invoke `bque run` / `bque bench` directly whenever you want cross-repo coordination.

### Tier 2: Direct Native `bque` CLI (Ad-hoc & Tuned Calls)
When you need custom flags (`--weight`, `--scope`, `--stall-timeout`, `--max-duration`, `--observer`, `--cwd`), call `bque` directly:
```powershell
# Default heavy build (weight=100, auto-yields after 10s at <5% CPU)
bque run -- cargo build --release

# Lightweight concurrent check (weight=25)
bque run --weight light -- cargo clippy -p my-crate

# Strict directory isolation + watchdog for automated runs (exit 124 on stall/timeout)
bque run --scope target-debug --stall-timeout 300 --max-duration 1800 -- cargo test

# Pause active builds manually (optionally keep dev-server observers running)
bque pause --keep-observers --reason "Interactive profiling"
bque resume
```

---

## 4. Two Hard Hygiene Rules

1. **Windows 4KB Anonymous Pipe Hygiene (Strictly Forbidden)**:
   - **FORBIDDEN**: Never pipe native console build commands into PowerShell truncating filters (e.g., `cargo test 2>&1 | Select-Object -First 100` # pipeline-hygiene: ok anti-pattern doc).
   - **Why**: Native Windows processes ignore PowerShell cooperative breaks; once the 4KB pipe buffer fills, `WriteFile` blocks forever.
   - **Fix**: Redirect to a file first, then inspect: `cargo test *> out.log; Get-Content out.log -TotalCount 100`.
2. **Scoped Process Termination**:
   - Never kill processes blindly by image name (`Stop-Process -Name ...`). Match strictly by worktree path or use `bque`'s own `--stall-timeout` / `--max-duration`.

---

## 5. Environment & Installation

Run `just doctor` to check `bque` availability and host tier (`bque probe --json`). Full setup guide: [references/install.md](references/install.md).


<!-- PENGJ_TEMPLATE_END -->

<!-- Project-specific section below: template updates only replace the managed block above; this section belongs to the project and is preserved in full. -->
## Project-Specific Queue Policy & Custom Configuration

> Register project-specific heavy task tags, custom timeout thresholds, or prerequisite environment scripts here.