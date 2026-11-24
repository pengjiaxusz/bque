# Specification: Budget-Based Work-Conserving Scheduler & Activity Watchdog

- **Status**: Approved
- **Related Glossary**: [`GLOSSARY.md`](../../GLOSSARY.md)
- **Related ADRs**:
  - [ADR-0001: Budget-Based Work-Conserving Scheduling with Yielding](../adr/0001-budget-and-yielding-over-single-slot-mutex.md)
  - [ADR-0002: Daemonless State-File Coordination](../adr/0002-daemonless-state-file-coordination.md)
  - [ADR-0003: Soft Oversubscription on Yield Recovery](../adr/0003-soft-oversubscription-on-yield-recovery.md)
  - [ADR-0004: CPU-Only Stall Detection Preserving Native TTY](../adr/0004-cpu-only-stall-detection-preserving-tty.md)

## 1. Problem Statement

`bque` currently enforces a strict 1-slot global build mutex (`Bque_Global_Build_Lock`) with no execution-phase watchdog and a self-throttling CPU governor. This causes four recurring failures:

1. **Head-of-line blocking on hung tasks**: `--timeout` only bounds queue wait time; once a task holds the lock, a hung child keeps the `bque` heartbeat alive indefinitely and blocks all queued tasks.
2. **Wasted capacity during low-CPU tasks**: Dependency downloads, I/O-bound steps, and single-threaded tests hold the exclusive lock while leaving most CPU cores idle.
3. **Permanent 0%-CPU freeze on abnormal benchmark exit**: `bque bench` lacks an RAII unfreeze guard and orphan-freeze recovery, leaving suspended build trees frozen at 0% CPU while still holding the lock.
4. **Governor self-throttling**: `sample_system_cpu_load` counts `bque`'s own build CPU as external contention, throttling builds down to 35% on an otherwise idle host.

## 2. Architecture & Data Model

### 2.1 State Directory Layout (`src/state/storage.rs`)

- `tasks/task_<pid>.json`: One file per active managed task (`TaskState`), eliminating concurrent write contention across active tasks.
- `waiters/waiter_<pid>.json`: Queued waiters (`WaiterState`), extended with `weight: u32` and `scope: Option<String>`.
- `observers/observer_<pid>.json`: Observers (`LockState`), unchanged in role.
- `bench_state.json`: Extended with `frozen_identities: Vec<ProcessIdentity>` alongside legacy `frozen_pids`.
- `pause_state.json`: Extended with `frozen_identities: Vec<ProcessIdentity>` and `keep_observers: bool`.

`TaskState` fields:
- `pid`, `start_time`, `command`, `description`, `cwd`, `priority`, `cpu_rate`
- `weight: u32` (1..=100, default 100)
- `scope: Option<String>`
- `activity_state: ActivityState` (`Active` | `Yielding`)
- `cpu_pct: f64` (latest sampled process-tree CPU share of total host capacity, 0.0..=100.0)
- `low_cpu_secs: u64` (consecutive seconds below yielding threshold)
- `identity: Option<ProcessIdentity>`, `heartbeat: Option<DateTime<Utc>>`

`FullQueueStatus` JSON additions (backward-compatible):
- `active_tasks: Vec<TaskState>`
- `budget_used: u32` (sum of `weight` for `Active` tasks; `Yielding` tasks contribute `0`)
- `budget_active_raw: u32` (sum of `weight` for `Active` tasks; `> 100` indicates oversubscription)
- `budget_total: u32` (`100`)
- `oversubscribed: bool` (`budget_active_raw > budget_total`)
- `active_lock: Option<LockState>` (retained for backward compatibility, populated from the first active task when present)

### 2.2 Daemonless Coordination & Admission (`src/lock/mutex.rs`, `src/commands.rs`)

- Replace long-held `Bque_Global_Build_Lock` with a short-lived coordination mutex `Bque_Coord_Lock` acquired only for the brief critical section (~milliseconds) of `reap() -> check admission -> register task / unregister waiter`.
- Admission predicates for waiter `W` with `(weight, scope)`:
  1. `bench_state` is `None` and `pause_state` is `None`.
  2. No live `TaskState` (whether `Active` or `Yielding`) holds the same `Some(scope)`.
  3. `effective_budget_used + W.weight <= 100`, where `effective_budget_used = sum(t.weight for t in active_tasks if t.activity_state == Active)`.
  4. **FIFO with Bounded Backfill**: Waiters are sorted by `queued_at`. If `W` is not at index 0, `W` may backfill only if every earlier waiter `E` with `E.queued_at < W.queued_at` that cannot currently fit has been waiting `< 60s` (`BACKFILL_STARVATION_SECS = 60`). Once any earlier waiter has waited `>= 60s`, no later waiter may jump ahead of it.

### 2.3 Cross-Platform Process-Tree CPU Sampling (`src/job/activity.rs`)

- **Windows**: Query `JobObjectBasicAccountingInformation` on the task's `JobObject` handle (`TotalUserTime + TotalKernelTime`, 100ns ticks -> ms).
- **Linux**: Enumerate `/proc` descendants rooted at the child PID (plus matching PGID) and sum `utime + stime` ticks converted to ms via `CLK_TCK`.
- Normalized host CPU percentage over sampling interval `dt`:
  `cpu_pct = (delta_cpu_ms / (dt_ms * logical_cores)) * 100.0`, clamped to `[0.0, 100.0]`.

### 2.4 Activity Monitor, Yielding & Watchdog (`src/runner/execute.rs`)

- Runs on a 2-second cadence while the child executes (also refreshes task heartbeat and writes updated `cpu_pct`, `activity_state`, `cpu_rate` to `tasks/task_<pid>.json`).
- **Yielding** (enabled by default unless `--no-yield`):
  - If `cpu_pct < 5.0` continuously for `yield_after` seconds (default `10s`), transition `Active -> Yielding`.
  - If `cpu_pct >= 5.0` while `Yielding`, transition `Yielding -> Active` immediately (soft oversubscription if total active weight exceeds 100).
- **Stall Watchdog** (opt-in via `--stall-timeout <secs>`):
  - If `cpu_pct < 0.5` continuously for `stall_timeout` seconds, print diagnostic to stderr, terminate the entire child process tree (Windows `TerminateJobObject` / Unix `kill(-pgid, SIGTERM)` then `SIGKILL`), and return exit code `124`.
- **Max Duration Watchdog** (opt-in via `--max-duration <secs>`):
  - If wall-clock execution exceeds `max_duration` seconds, print diagnostic to stderr, terminate the entire child process tree, and return exit code `124`.
- Native `Stdio::inherit()` is preserved throughout.

### 2.5 Governor External-Only Contention & Oversubscription Sharing (`src/governor/`)

- Compute `external_cpu = (total_system_cpu_pct - bque_tasks_cpu_pct).max(0.0)` where `bque_tasks_cpu_pct` sums `cpu_pct` across live `active_tasks` + `observers`.
- Determine base `host_budget_rate` from `external_cpu` (100% idle / 65% moderate >= 25% / 35% heavy >= 60%).
- When oversubscribed (`total_active_weight > 100`), apply proportional share:
  `effective_rate = (host_budget_rate * my_weight / total_active_weight).clamp(10, 100)`.
- Persist updated `cpu_rate` to the task state file when it changes.

### 2.6 Safe Freeze / Unfreeze for `bench` & `pause` (`src/job/suspend.rs`, `src/bench/runner.rs`, `src/commands.rs`)

- `ProcessFreezer::suspend_process_tree_identities(root_pid) -> Result<Vec<ProcessIdentity>>` records `(pid, created_at_ms)` for every frozen process.
- `ProcessFreezer::resume_identities(&[ProcessIdentity])` verifies each process's `ProcessIdentity::classify() == Liveness::Alive` before calling `NtResumeProcess` / `SIGCONT` (preventing PID-reuse accidents).
- `FreezeGuard` RAII struct ensures `bque bench` always resumes frozen processes and clears `bench_state.json` on normal exit, error, or panic.
- `reap()` automatically resumes `frozen_identities` whenever it cleans a dead/reused `bench_state.json`.
- `bque pause [--keep-observers]` freezes all live `active_tasks` (and `observers` unless `--keep-observers`) and stores `frozen_identities` in `pause_state.json`; `bque resume` unfreezes them and removes `pause_state.json`.

### 2.7 CLI Surface (`src/cli/mod.rs`)

New `bque run` options:
- `--weight <1-100|light|medium|heavy>` (default: `heavy` / `100`; `light=25`, `medium=50`, `heavy=100`)
- `--scope <KEY>` (optional mutual-exclusion key)
- `--yield-after <SECS>` (default: `10`)
- `--no-yield` (disable automatic yielding)
- `--stall-timeout <SECS>` (opt-in stall watchdog, exit `124`)
- `--max-duration <SECS>` (opt-in total execution timeout, exit `124`)

New `bque pause` option:
- `--keep-observers` (do not freeze observers when pausing)

## 3. Verification Plan

- **Unit tests**: weight parsing, admission & backfill starvation cap, oversubscription proportional rate calculation, governor external-CPU subtraction, process-tree CPU sampling, identity-checked freeze/resume.
- **Integration tests (`tests/integration_tests.rs`)**:
  - Concurrent `--weight light` tasks execute in parallel.
  - Low-CPU `sleep` task transitions to `Yielding` after `--yield-after` and admits a queued `heavy` waiter.
  - `--scope` prevents concurrent execution even when the holder is `Yielding`.
  - `--stall-timeout` terminates a hung sleep process and exits with code `124`.
  - `--max-duration` terminates a busy-loop process and exits with code `124`.
  - `bque status --json` exposes `active_tasks`, `budget_used`, `budget_total`, and legacy `active_lock`.
  - Orphaned `bench_state.json` with frozen identities is healed by `reap()`.
- **Cross-platform verification**:
  - Linux (`cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`) in cloud sandbox.
  - Windows 11 (`cargo test` + `cargo clippy --all-targets`) on PC1 via Dagu in an isolated worktree.
