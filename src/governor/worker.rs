use super::activity::{ActivityGovernorConfig, evaluate_target_cpu_rate};
use crate::job::JobObject;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

pub struct GovernorWorker {
    stop_signal: Arc<AtomicBool>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl GovernorWorker {
    pub fn start(job_object: Arc<JobObject>, initial_rate: u32, hard_cap: bool) -> Self {
        let stop_signal = Arc::new(AtomicBool::new(false));
        let stop_clone = stop_signal.clone();
        let config = ActivityGovernorConfig::default();

        let thread_handle = thread::spawn(move || {
            let mut last_rate = initial_rate;
            while !stop_clone.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_secs(5));
                if stop_clone.load(Ordering::Relaxed) {
                    break;
                }

                let target_rate = evaluate_target_cpu_rate(&config);
                if target_rate != last_rate
                    && job_object.set_cpu_rate(target_rate, hard_cap).is_ok()
                {
                    last_rate = target_rate;
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
