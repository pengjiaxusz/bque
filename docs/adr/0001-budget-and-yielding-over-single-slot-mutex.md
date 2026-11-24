---
status: accepted
---

# Budget-Based Work-Conserving Scheduling with Yielding over Single-Slot Mutex

A strict single-slot global mutex causes severe head-of-line blocking whenever the lock holder enters a low-CPU phase (dependency download, I/O wait, single-threaded test, or hang), leaving the rest of the host idle. We replace the single-slot build lock with a 100-unit shared budget pool combined with automatic yielding: heavy builds still default to full budget (serializing against each other when actively compiling), lightweight tasks can claim smaller shares to run concurrently, and any running task with sustained low CPU temporarily lends its budget to queued waiters without being killed.

## Considered Options

- **Single-slot mutex + forced stall kill**: Simple, but cannot utilize spare cores during legitimate low-CPU tasks and risks killing slow downloads or waits.
- **Budget + yielding + opt-in stall kill (chosen)**: Preserves isolation for active heavy builds while eliminating head-of-line blocking for low-CPU and hung tasks.
