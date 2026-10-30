# bque 命令判定矩阵与安全指南 / Usage Matrix & Safety Guide


## 1. Command Mapping Matrix

| Original Native Command | Recommended Execution (with bque) | Underlying bque Mode | Priority Class |
| :--- | :--- | :--- | :--- |
| `cargo build` | `just build` | `bque run` | `Idle` (Inherited by compiler tree) |
| `cargo check --workspace` | `just check` | `bque run` | `Idle` |
| `cargo clippy -- -D warnings` | `just clippy` (or `bque-runner.ps1 -Cmd "..."`) | `bque run` | `Idle` |
| `cargo test --workspace` | `just test` | `bque run` | `Idle` |
| `pnpm build` | `just build` (or `bque-runner.ps1 -Cmd "pnpm build"`) | `bque run` | `Idle` |
| `cmake --build target/debug` | `just build` | `bque run` | `Idle` |
| `cargo bench` | `just bench` | `bque bench` | `High` (Freezes background builds) |
| `pnpm bench` | `just bench` | `bque bench` | `High` (Freezes background builds) |
| `cargo fmt` | `cargo fmt` (Direct) | **Bypass** | Foreground |
| `git status` / `git commit` | Direct commands | **Bypass** | Foreground |

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

If a process crashed abruptly or a build lock was left behind:
```powershell
# 1. Inspect lock status
just queue-status   # or: bque status

# 2. Force clean orphaned state files
just queue-clean    # or: bque clean

# 3. Manually pause/resume builds when gaming or running profiling
bque pause
bque resume
```

