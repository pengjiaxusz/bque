---
status: accepted
---

# Soft Oversubscription on Yield Recovery over Suspending Borrowers

When a Yielding task lends its Budget to a queued waiter and later resumes heavy CPU work, the combined active Weight temporarily exceeds 100. Rather than freezing the borrower with `NtSuspendProcess`/`SIGSTOP` until the original task finishes, both tasks remain Active under soft oversubscription: the governor splits available CPU soft-cap proportionally by Weight, and no new waiters are admitted until active Weight drops back within Budget.

## Considered Options

- **Suspend the borrower**: Enforces a strict 100-unit ceiling at all times, but freezing an arbitrary in-flight task risks breaking network connections, heartbeats, or internal tool locks.
- **Soft oversubscription with proportional CPU cap (chosen)**: Keeps both tasks making steady progress while protecting interactive responsiveness and blocking further admissions.
