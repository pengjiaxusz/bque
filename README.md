# bque

[English](README.md) | [简体中文](README_zh.md)

[![CI](https://github.com/pengjiaxusz/bque/actions/workflows/ci.yml/badge.svg)](https://github.com/pengjiaxusz/bque/actions)
[![Release](https://img.shields.io/github/v/release/pengjiaxusz/bque)](https://github.com/pengjiaxusz/bque/releases/latest)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

**bque** (Build Queue & Resource Governor) is a high-performance, daemonless system utility on Windows designed to coordinate heavy builds and background tasks across multiple projects, enforce dynamic CPU rate budgets via Windows Job Objects, and guarantee clean, undisturbed CPU conditions for benchmarks.

---

## Why bque?

### 1. The Multi-Project Contention Problem
When developing across multiple repositories, worktrees, or parallel language stacks (e.g. C++ with CMake/Ninja, Rust with Cargo, Go, Node.js), background builds inevitably collide:
- Simultaneous compilations spike CPU usage to 100% and saturate disk IO.
- Foreground tools (IDEs, browsers, terminals) stutter and drop frames.
- Concurrent access to build caches or files causes lock contention.

### 2. The Benchmark Degradation Problem
Benchmarking and profiling require a **completely clean CPU environment**:
- Running builds with `Idle` priority is **not enough**: the Windows scheduler still runs Idle threads on spare core cycles or hyperthread gaps.
- Shared L3 cache eviction and memory bus contention skew benchmark timings by 5%–25%.
- All-core CPU load triggers power limits (PL1/PL2) and thermal throttling, dropping single-core boost clock frequencies.

`bque` solves this by introducing a **Clean-Room Benchmark Lock** (`bque bench`) that automatically **freezes (suspends) running background builds** via kernel-level process suspension (`NtSuspendProcess`), elevates benchmark priority to `High`, and pins execution to performance cores. Once the benchmark completes, paused builds resume automatically without losing progress.

---

## Features

- **Daemonless Architecture**: Built entirely on Windows kernel primitives (`Named Mutex`, `Job Objects`, `Virtual Memory`). Zero background services or daemon processes required; abnormal process exits automatically release locks via `AbandonedMutex`.
- **Microsecond Overhead (<2ms startup)**: Written in pure Rust with zero heavy runtime, wrapping quick commands with negligible latency.
- **Process Priority Demotion**: Automatically runs tasks at `Idle` priority (inherited by all child compiler processes such as `cl.exe`, `clang`, `rustc`, `ninja`).
- **Job Object CPU Rate Soft-Cap**: Sets dynamic CPU quotas (e.g. 35%, 50%, 75%, 100%). Soft-cap semantics allow tasks to use full CPU capacity when the system is quiet, and throttle back only when competing with interactive foreground applications.
- **Clean-Room Benchmark Mode (`bque bench`)**:
  - Automatically acquires exclusive benchmark access.
  - Instantly freezes active background builds in kernel space (`NtSuspendProcess`).
  - Elevates benchmark to `High` priority and applies CPU core affinity (`p-cores` / `e-cores`).
  - Seamlessly resumes paused builds when the benchmark concludes.
- **Dynamic Contention Governor**: Actively samples real-time system CPU load in 5s intervals. Dynamically adjusts Job Object CPU quotas when system contention is detected to ensure foreground responsiveness.
- **Full Introspection**: Inspect real-time queue states, active lock owners, waiters, and pause flags via `bque status`.

---

## Installation

### One-Liner Quick Install (Recommended)
Run the following command in PowerShell. It requires **no administrator privileges**, automatically downloads the latest release, installs it to `$env:LOCALAPPDATA\bque\bin`, updates your `PATH`, and takes effect immediately in the current session:
```powershell
irm https://github.com/pengjiaxusz/bque/releases/latest/download/install.ps1 | iex
```

### Pre-built Binaries
Download the latest `bque-windows-x86_64.zip` or `bque.exe` from [GitHub Releases](https://github.com/pengjiaxusz/bque/releases/latest) and place it in your tools directory (recommended: `$env:LOCALAPPDATA\bque\bin`), then add it to your system `PATH`.

### From Source
```bash
git clone https://github.com/pengjiaxusz/bque.git
cd bque
cargo install --path .
```

Ensure `%USERPROFILE%\.cargo\bin` is in your system `PATH`.

### Updating an Existing Install
`bque` can replace itself from GitHub Releases: it downloads the latest release
next to the running executable, renames the live binary to `bque.exe.old` and
moves the new one in place, so no admin rights and no reinstall are needed.
```powershell
bque update            # download the latest release over the current binary
bque update --check    # only report whether a newer release exists
bque update --tag v0.1.2 --force   # pin a specific release and reinstall anyway
```

When the change you want is in a local checkout but not yet released, install
from the working tree so the binary matches the source:
```powershell
cd D:\path\to\bque
cargo install --path . --force
```

Both paths are safe to mix, but they write to different files: `bque update`
replaces whichever `bque.exe` launched it, while `cargo install` always writes
`%USERPROFILE%\.cargo\bin\bque.exe`. Run `Get-Command bque` to see which one
your shell resolves, and `bque --version` to confirm the result.

> A release and a local checkout can share the same version number. `bque
> update` compares versions only, so it reports "up to date" even when your
> checkout carries newer commits — install from the working tree to pick those
> up.

---

## Usage

### 1. Build and Task Execution (`bque run`)
Wrap any command behind `bque run --`:

```powershell
# Basic compilation (runs at Idle priority with mutual exclusion)
bque run -- cargo build --release

# With a custom description for queue visibility
bque run --desc "Compile Web App" -- cmake --build build

# Enforce a 50% CPU soft-cap
bque run --cpu-rate 50 -- cargo test

# Observer mode (monitored in status without blocking the build queue)
bque run --observer --desc "Linter" -- cargo clippy
```

### 2. Clean-Room Benchmark Protection (`bque bench`)
Run performance benchmarks with complete CPU isolation:

```powershell
# Freezes any running bque builds, sets High priority, and runs the benchmark
bque bench -- python run_benchmarks.py

# Pin benchmark strictly to performance cores
bque bench --cores p-cores -- cargo bench
```

### 3. Global Status Inspection (`bque status`)
Check active jobs and queued waiters across all projects:

```powershell
bque status
```

Example output:
```text
╭─ bque • Global Build Queue & Governor ───────────────────────────────────────────────────────────
│  🏃 ACTIVE BUILD LOCK
│     PID 12450 • Compile Engine
│       dir  C:\Projects\game-engine\worktrees\render
│       cmd  cmake --build build --config Release
│       meta Idle · CPU 65% (soft-cap) · held 18s · started 06:40:12
│
│  ⏳ QUEUED WAITERS (1)
│     #1  PID 28312  [   6s] Unit tests
│       dir  C:\Projects\game-engine\worktrees\audio
│       cmd  cargo test --workspace
│       meta queued 06:59:30
│
│  👀 OBSERVERS (1)
│     PID 30121  [10m 33s] dev server
│       dir  C:\Projects\game-engine
│       cmd  cargo run -q -p xtask -- dev
│       meta Idle · CPU 30% (soft-cap) · since 06:50:12
╰──────────────────────────────────────────────────────────────────────────────────────────────────
```

Every task — lock holder, waiter or observer — reports its own `dir` (working
directory), `cmd` (full command line when a custom description hides it) and
`meta` (priority, CPU budget, elapsed time and start clock). When several
worktrees of the same repository share one machine-wide queue, this is what
tells you whose job is which.

Output machine-readable JSON using `bque status --json`.

### 4. Manual Pause & Resume (`bque pause` / `bque resume`)
Temporarily pause all background builds (e.g. before launching an intensive application):

```powershell
bque pause --reason "Gaming session"
# Later:
bque resume
```

### 5. Cleanup Stale Records (`bque clean`)
Every queue record is pinned to the process *instance* that wrote it — PID plus process creation time — and refreshed by a heartbeat, so an entry can never outlive its owner:

- a dead owner is dropped the moment any `status`, `run` or `bench` consults the queue;
- a **recycled PID** (the number now belongs to an unrelated process) is detected by the creation-time mismatch and dropped as well;
- only when the process cannot be inspected at all (access denied) does the heartbeat lease decide.

`bque clean` runs that same healing pass on demand and reports exactly what it removed:

```powershell
bque clean
```

If the queue is jammed by an entry you want gone right now, force a full reset:

```powershell
bque clean --force
```

`--force` also drops lock, benchmark and pause records whose owner still looks alive. The kernel mutex stays authoritative: a genuinely running build keeps running, and an abandoned lock is recovered by the next waiter.

---

## Project Integration Examples

### In `justfile`
```just
# Wrap heavy builds with bque
build:
    bque run -- cargo build --all-targets

test:
    bque run -- cargo test

bench:
    bque bench -- cargo bench
```

### In PowerShell Scripts
```powershell
# Check for bque and run under global coordination
if (Get-Command bque -ErrorAction SilentlyContinue) {
    & bque run --desc "Service Build" -- cmake --build build
} else {
    Write-Warning "bque is recommended for multi-task build coordination."
    Write-Warning "Download: https://github.com/pengjiaxusz/bque/releases/latest"
    cmake --build build
}
```

---

## Architecture Overview

```text
┌────────────────────────────────────────────────────────┐
│                        CLI                             │
│       bque run  │  bque bench  │  bque status          │
└──────────────────────────┬─────────────────────────────┘
                           │
             ┌─────────────┴─────────────┐
             ▼                           ▼
┌─────────────────────────┐ ┌─────────────────────────┐
│     Lock & Queue        │ │  Clean-Room Benchmark   │
│ - Local\Bque_Build_Lock │ │ - Local\Bque_Bench_Lock │
│ - Abandoned recovery    │ │ - NtSuspendProcess      │
│ - Waiter registration   │ │ - NtResumeProcess       │
└────────────┬────────────┘ └────────────┬────────────┘
             │                           │
             └─────────────┬─────────────┘
                           ▼
┌────────────────────────────────────────────────────────┐
│               Resource Governance Layer                │
│ - SetPriorityClass (Idle / BelowNormal / Normal / High)│
│ - Job Object (JOBOBJECT_CPU_RATE_CONTROL_INFORMATION)  │
│ - Dynamic System Contention Governor (5s sample loop)  │
│ - CPU Topology & Core Affinity (P-Core / E-Core)       │
└────────────────────────────────────────────────────────┘
```

---

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
