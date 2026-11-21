use crate::state::storage::{
    BENCH_HEARTBEAT_INTERVAL, LOCK_HEARTBEAT_INTERVAL, OBSERVER_HEARTBEAT_INTERVAL, StateStorage,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// How often the worker wakes up. Kept short so shutdown is prompt; the actual
/// heartbeat cadence is the interval passed to `start`.
const TICK: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug)]
pub enum HeartbeatKind {
    Lock,
    Bench,
    Observer,
}

/// Refreshes the heartbeat of a long-running state record.
///
/// The heartbeat is the fallback proof of life used when a record's process
/// cannot be inspected (see `state::process`). Without it, a suspended or
/// access-denied owner would only be recoverable by hand.
pub struct StateHeartbeat {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl StateHeartbeat {
    pub fn start_lock(storage: StateStorage, pid: u32) -> Self {
        Self::start(storage, pid, HeartbeatKind::Lock, LOCK_HEARTBEAT_INTERVAL)
    }

    pub fn start_bench(storage: StateStorage, pid: u32) -> Self {
        Self::start(storage, pid, HeartbeatKind::Bench, BENCH_HEARTBEAT_INTERVAL)
    }

    /// Observers hold no lock, but they still own a record: without a heartbeat
    /// a suspended or access-denied dev server would age out of the dashboard.
    pub fn start_observer(storage: StateStorage, pid: u32) -> Self {
        Self::start(
            storage,
            pid,
            HeartbeatKind::Observer,
            OBSERVER_HEARTBEAT_INTERVAL,
        )
    }

    fn start(storage: StateStorage, pid: u32, kind: HeartbeatKind, interval: Duration) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_signal = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            let mut waited = Duration::ZERO;
            while !stop_signal.load(Ordering::Relaxed) {
                thread::sleep(TICK);
                if stop_signal.load(Ordering::Relaxed) {
                    break;
                }
                waited += TICK;
                if waited < interval {
                    continue;
                }
                waited = Duration::ZERO;
                let _ = match kind {
                    HeartbeatKind::Lock => storage.touch_lock_state(pid),
                    HeartbeatKind::Bench => storage.touch_bench_state(pid),
                    HeartbeatKind::Observer => storage.touch_observer(pid),
                };
            }
        });

        Self {
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for StateHeartbeat {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
