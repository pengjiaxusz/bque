---
status: accepted
---

# CPU-Only Stall Detection Preserving Native TTY Inheritance

Detecting stalled tasks could combine process-tree CPU sampling with stdout/stderr silence tracking. We intentionally detect stalls using process-tree CPU activity alone (combined with an optional `--max-duration` hard ceiling) so child commands keep direct `Stdio::inherit()` TTY attachment. Capturing pipes would break ANSI colors, interactive progress bars, and terminal width detection across compilers and test runners.

## Considered Options

- **Pipe stdout/stderr for output-silence detection**: Distinguishes silent hangs from printing loops, but breaks native TTY behavior for wrapped build tools.
- **Process-tree CPU activity + optional `--max-duration` (chosen)**: Preserves native TTY output while catching both 0%-CPU hangs and 100%-CPU infinite loops with exit code `124`.
