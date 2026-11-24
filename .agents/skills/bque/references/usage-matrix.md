
# bque Command Decision Matrix & Safety Guide

## 1. Command Mapping Matrix

| Original Native Command | Recommended Execution (with bque) | Weight / Mode | Priority Class |
| :--- | :--- | :--- | :--- |
| `cargo build` | `just build` | `heavy` (`100`, auto-yielding) | `Idle` (inherited by tree) |
| `cargo check --workspace` | `just check` | `heavy` (`100`) or `--weight medium` (`50`) | `Idle` |
| `cargo clippy -p <crate>` | `bque run --weight light -- cargo clippy -p <crate>` | `light` (`25`, concurrent) | `Idle` |
| `cargo test --workspace` | `just test` | `heavy` (`100`, auto-yielding) | `Idle` |
| `cargo test -p <crate> --lib` | `bque run --weight light -- cargo test -p <crate> --lib` | `light` (`25`, concurrent) | `Idle` |
| `pnpm build` / `tsc` | `just build` or `bque run -- pnpm build` | `heavy` (`100`) | `Idle` |
| `cmake --build target/debug` | `just build` or `bque run --scope cmake-debug -- cmake --build target/debug` | `heavy` + `--scope` | `Idle` |
| `ninja -C target/debug <target>` | `just test-build <target>` or `bque run --scope cmake-debug -- ninja ...` | `heavy` + `--scope` | `Idle` |
| `ctest --output-on-failure` | `just test` or `bque run -- ctest ...` | `heavy` (`100`) | `Idle` |
| `cargo bench` / `pnpm bench` | `just bench` or `bque bench -- <cmd>` | `bque bench` | `High` (freezes all active tasks & observers) |
| `cargo fmt` / `git ...` | Direct command | **Bypass** | Foreground |
| Automated / Agent build or test | `bque run --stall-timeout 300 --max-duration 1800 -- <cmd>` | Watchdog (`exit 124`) | `Idle` |
| Long-running service (`xtask dev`, `pnpm dev`, `cargo watch`) | `bque run --observer -- <cmd>` (**never** plain `bque run`) | `--observer` (`0` Budget) | `Idle` |

### Key Scheduling Mechanics (v0.3.0+)

- **100-unit Budget & Backfill**: Total host budget is `100`. Multiple `light` (`25`) or `medium` (`50`) tasks run concurrently. Smaller waiters may backfill spare capacity ahead of a blocked heavy waiter until the head waiter has waited `60s` (starvation cap).
- **Automatic Yielding & Soft Oversubscription**: Any running task staying below `5%` host CPU for `--yield-after` seconds (default `10s`) enters `Yielding` and temporarily lends its budget to waiters without being killed (disable with `--no-yield`). If a yielding task later resumes heavy CPU work while a borrower is still running, both stay `Active` under soft oversubscription (`OVERSUBSCRIBED`), splitting the CPU soft-cap proportionally while blocking further admissions.
- **Scope Exclusion (`--scope <key>`)**: Tasks sharing a `--scope` key never overlap — even if the holder enters `Yielding` — protecting shared build directories from corruption.
- **Opt-in Watchdogs (Exit `124`)**: `--stall-timeout <secs>` terminates the child tree if process-tree CPU stays `<0.5%` continuously; `--max-duration <secs>` enforces a hard wall-clock ceiling. Both preserve `Stdio::inherit()` native TTY output and return exit code `124`.
- **Cross-Platform Boundary**: `bque` runs natively on both Windows and Linux/WSL2. Generated `justfile` `[unix]` recipes pass through without `bque` so CI runners stay zero-dependency; invoke `bque` directly on Linux/WSL2 dev hosts when cross-repo queuing is desired.

---

## 2. Windows 4KB Anonymous Pipe Deadlock & Safe Handling

### The Problem
```powershell
# DANGEROUS ANTI-PATTERN: DO NOT RUN THIS!
cargo test 2>&1 | Select-Object -First 100  # pipeline-hygiene: ok anti-pattern doc
```
PowerShell creates a 4KB anonymous pipe. Once `Select-Object -First 100` closes its read end, the upstream native compiler blocks indefinitely on Win32 `WriteFile`.

### The Safe Pattern (Redirect to disk first)
```powershell
cargo test *> test_out.log
Get-Content test_out.log -TotalCount 100
Remove-Item test_out.log -ErrorAction SilentlyContinue
```

---

## 3. Abnormal State Recovery & Orphan Freeze Healing

Every queue record (`tasks/task_<pid>.json`, `waiters/`, `observers/`, `bench_state.json`, `pause_state.json`) is pinned to `(pid, created_at_ms)` and refreshed by a heartbeat. Coordination uses a short-lived `Bque_Coord_Lock` rather than a long-held global mutex:

- Dead or recycled-PID records are reaped automatically on any `status`, `run`, or `bench` call.
- If a `bque bench` process crashes while background builds were suspended, the next `reap()` / `bque status` / `bque clean` verifies recorded process identities and **automatically unfreezes** the orphaned build trees.

```powershell
# 0. Verify installation and host tier (`bque probe --json`)
just doctor

# 1. Inspect active tasks, Budget (used/100), live CPU%, Yielding state, and waiters
bque status

# 2. Clean dead records and auto-unfreeze any orphaned frozen processes
bque clean

# 3. Emergency unblock: drop all state records and resume any frozen processes
bque clean --force

# 4. Manually freeze/resume active builds (optionally keep dev-server observers alive)
bque pause --keep-observers --reason "Gaming or profiling"
bque resume
```

