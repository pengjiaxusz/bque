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

### Pre-built Binaries
Download the latest `bque.exe` or release archive from [GitHub Releases](https://github.com/pengjiaxusz/bque/releases/latest) and add it to your system `PATH`.

### From Source
```bash
git clone https://github.com/pengjiaxusz/bque.git
cd bque
cargo install --path .
```

Ensure `%USERPROFILE%\.cargo\bin` is in your system `PATH`.

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
============================================================
              bque Global Build Queue & Governor            
============================================================

[1] Active Lock Holder:
  PID:         12450
  Description: Compile Engine
  Command:     cmake --build build --config Release
  Working Dir: C:\Projects\game-engine
  Priority:    Idle
  CPU Rate:    65% (soft-cap)
  Running For: 18s

[2] Queued Waiters (1):
  #1  PID 28312  [   6s] Unit tests ('C:\Projects\web-service')

[3] Observers (0):
  (No observers active)
============================================================
```

Output machine-readable JSON using `bque status --json`.

### 4. Manual Pause & Resume (`bque pause` / `bque resume`)
Temporarily pause all background builds (e.g. before launching an intensive application):

```powershell
bque pause --reason "Gaming session"
# Later:
bque resume
```

### 5. Cleanup Stale Records (`bque clean`)
Safely evicts orphaned waiter records or stale state files if a terminal was forcefully terminated:

```powershell
bque clean
```

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
