# bque

A daemonless, cross-platform build queue and resource governor that coordinates concurrent builds and clean-room benchmarks across repositories on a single host.

## Language

**Budget**:
The host's shared capacity pool (100 units) that governs how many managed tasks may run concurrently.
_Avoid_: Limit, quota, cap

**Weight**:
The portion of the host's Budget (1–100) claimed by a managed task (`light` = 25, `medium` = 50, `heavy` = 100).
_Avoid_: Slots, cost, priority

**Scope**:
An optional mutual-exclusion key that serializes tasks sharing a resource (such as a build directory) regardless of Budget or Yielding state.
_Avoid_: Lock group, namespace, lane

**Backfill**:
Admitting a queued waiter with a smaller Weight ahead of a blocked head-of-line waiter when sufficient Budget remains and no Scope conflicts, bounded by a starvation cap so head-of-line tasks always run.
_Avoid_: Queue jumping, priority bypass

**Yielding**:
A running task state in which sustained low CPU activity causes the task to temporarily lend its Budget to queued waiters without being terminated.
_Avoid_: Idle, sleeping, paused

**Oversubscription**:
A temporary condition in which a Yielding task resumes heavy CPU work while a borrower is still running, causing combined active Weight to exceed the 100-unit Budget until one finishes.
_Avoid_: Overcommit, burst, collision

**Stalled**:
A running task state in which zero CPU activity persists past an explicit stall timeout, making the task eligible for forced termination.
_Avoid_: Hung, dead, frozen

**Observer**:
A managed task that appears in queue status and receives resource throttling without consuming Budget or waiting in line.
_Avoid_: Bypass, unlocked task

**Clean-Room Benchmark**:
An exclusive benchmark run that suspends active managed tasks so performance measurements execute without background CPU contention.
_Avoid_: Bench lock, priority run
