
# bque Command Decision Matrix & Safety Guide

## 1. Command Mapping Matrix

| Original Native Command | Recommended Execution (with bque) | Underlying bque Mode | Priority Class |
| :--- | :--- | :--- | :--- |
| `cargo build` | `just build` | `bque run` | `Idle` (Inherited by compiler tree) |
| `cargo check --workspace` | `just check` | `bque run` | `Idle` |
| `cargo clippy -- -D warnings` | `just clippy` (or `bque run -- cargo clippy ...`) | `bque run` | `Idle` |
| `cargo test --workspace` | `just test` | `bque run` | `Idle` |
| `pnpm build` | `just build` (or `bque run -- pnpm build`) | `bque run` | `Idle` |
| `cmake --build target/debug` | `just build` | `bque run` | `Idle` |
| `ninja -C target/debug <target>` | `just test-build <target>` (or `bque run -- ninja ...`) | `bque run` | `Idle` |
| `ctest --output-on-failure` | `just test` (or `bque run -- ctest ...`) | `bque run` | `Idle` |
| `cargo bench` | `just bench` | `bque bench` | `High` (Freezes background builds) |
| `pnpm bench` | `just bench` | `bque bench` | `High` (Freezes background builds) |
| `cargo fmt` | `cargo fmt` (Direct) | **Bypass** | Foreground |
| `git status` / `git commit` | Direct commands | **Bypass** | Foreground |
| Any ad-hoc heavy command | `just q-run <cmd>` | `bque run` | `Idle` |
| Any ad-hoc benchmark | `just q-bench <cmd>` | `bque bench` | `High` (Freezes background builds) |
| Long-running service: dev server, watch loop, `xtask dev`, `pnpm dev`, `cargo watch` | `bque run --observer -- <cmd>` (**never** plain `bque run`) | `bque run --observer` | `Idle` |

`bque run --observer` applies the same priority / CPU budget but does **not** take the global
build lock. A server that holds the lock blocks every other build for as long as it runs — the
classic "my build hangs for hours" symptom. `bque status` flags a holder older than 30 minutes
for exactly this reason.

`q-run` / `q-bench` are the generic wrappers for commands that have no dedicated `just` recipe.
Do not add a `--` separator after the recipe name (`just q-run cargo build --release`), since it
would become part of the wrapped command. On Unix / CI these wrappers execute the command directly;
bque is a Windows-only global lock.

---

## 2. Windows 4KB Anonymous Pipe Deadlock & Safe Handling

### The Problem
When running native console processes (e.g. `cmake`, `cargo`, `ninja`, `ctest`) in PowerShell:
```powershell
# DANGEROUS ANTI-PATTERN: DO NOT RUN THIS!
cargo test 2>&1 | Select-Object -First 100  # pipeline-hygiene: ok anti-pattern doc
```
- PowerShell creates an anonymous pipe with a fixed 4KB buffer.
- `Select-Object -First 100` stops reading once it has collected 100 objects and closes its end of the pipeline.
- The upstream native process (`cargo.exe` or `cl.exe`) continues trying to output logs. As soon as the 4KB buffer is filled, the Win32 `WriteFile` system call blocks indefinitely.
- The pipeline freezes for 120 seconds until a timeout kills it.

### The Safe Pattern (Redirect to disk first)
```powershell
# Safe: redirect all streams (*>) to a temporary log file, then read
cargo test *> test_out.log
Get-Content test_out.log -TotalCount 100
Remove-Item test_out.log -ErrorAction SilentlyContinue
```

---

## 3. Abnormal State Recovery & Deadlock Clearance

Every queue record is bound to the process instance that wrote it (PID **plus** process creation
time, so a recycled PID is not mistaken for a live owner) and refreshed by a heartbeat. Dead and
phantom records are dropped automatically the next time any `status`, `run` or `bench` consults
the queue — a stuck entry normally disappears on its own.

If a process crashed abruptly, or the queue is held by a record you want gone right now:

```powershell
# 0. Verify the installation and PATH fallback
just queue-doctor

# 1. Inspect lock status (this also performs the healing pass)
just queue-status   # or: bque status

# 2. Clean orphaned state files, and report exactly what was removed
just queue-clean    # or: bque clean

# 3. Emergency unblock: also drop records whose owner still looks alive
bque clean --force
# The kernel mutex stays authoritative, so a genuinely running build keeps running

# 4. Manually pause/resume builds when gaming or running profiling
just queue-pause    # or: bque pause
just queue-resume   # or: bque resume
```

`bque clean --force` clears state records, not processes: a long-running dev server that still
holds the kernel mutex keeps the queue busy. Stop that process (or restart it with
`bque run --observer`) to actually release the lock.

