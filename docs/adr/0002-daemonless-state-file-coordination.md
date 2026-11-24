---
status: accepted
---

# Daemonless State-File Coordination over Auto-Spawn Coordinator

Multi-slot budget allocation and yielding could be managed by an auto-spawned idle-exit daemon, which would centralize FIFO wakeups and process-tree sampling. We keep bque strictly daemonless: each running task samples its own process tree's CPU usage and publishes liveness/activity to a shared state file, while admission and budget accounting happen atomically under a short-lived coordination mutex. This preserves bque's zero-resident-process guarantee and crash-safe self-healing without introducing daemon lifecycle edge cases on Windows and Linux.

## Considered Options

- **Auto-spawn idle-exit daemon**: Cleaner centralized scheduling, but adds daemon startup races, IPC protocol maintenance, and background process lifecycle complexity.
- **Daemonless atomic state-file coordination (chosen)**: Keeps deployment trivial and crash recovery automatic via process identity and kernel mutexes.
