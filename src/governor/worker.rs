use super::activity::{
    ActivityGovernorConfig, apply_oversubscription_share, evaluate_target_cpu_rate_excluding,
};
use crate::job::JobObject;
use crate::state::StateStorage;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

pub struct GovernorWorker {
    stop_signal: Arc<AtomicBool>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl GovernorWorker {
    pub fn start(
        job_object: Arc<JobObject>,
        storage: StateStorage,
        pid: u32,
        my_weight: u32,
        observer: bool,
        initial_rate: u32,
        hard_cap: bool,
    ) -> Self {
        let stop_signal = Arc::new(AtomicBool::new(false));
        let stop_clone = stop_signal.clone();
        let config = ActivityGovernorConfig::default();

        let thread_handle = thread::spawn(move || {
            let mut last_rate = initial_rate;
            while !stop_clone.load(Ordering::Relaxed) {
                // Sleep in short slices so shutdown on Drop is responsive.
                for _ in 0..10 {
                    thread::sleep(Duration::from_millis(500));
                    if stop_clone.load(Ordering::Relaxed) {
                        return;
                    }
                }

                let bque_cpu = storage.total_active_cpu_pct();
                let host_rate = evaluate_target_cpu_rate_excluding(&config, bque_cpu);
                let target_rate = if observer {
                    host_rate
                } else {
                    let tasks = storage.list_active_tasks().unwrap_or_default();
                    let active_weight = StateStorage::effective_budget_used(&tasks);
                    apply_oversubscription_share(host_rate, my_weight, active_weight)
                };

                if target_rate != last_rate
                    && job_object.set_cpu_rate(target_rate, hard_cap).is_ok()
                {
                    last_rate = target_rate;
                    if !observer {
                        let _ = storage.update_task_cpu_rate(pid, target_rate);
                    }
                }
            }
        });

        Self {
            stop_signal,
            thread_handle: Some(thread_handle),
        }
    }

    #[allow(dead_code)]
    pub fn stop(mut self) {
        self.stop_signal.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for GovernorWorker {
    fn drop(&mut self) {
        self.stop_signal.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}
