use super::color::{ColorTheme, format_duration_secs};
use super::status::render_full_dashboard;
use crate::state::FullQueueStatus;
use std::io::Write;

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub struct LiveProgressReporter {
    pub theme: ColorTheme,
    pub my_pid: u32,
    pub my_desc: String,
    pub last_line_count: usize,
    pub spinner_idx: usize,
    pub last_heartbeat_secs: u64,
    pub last_state_summary: Option<String>,
}

impl LiveProgressReporter {
    pub fn new(my_pid: u32, my_desc: impl Into<String>) -> Self {
        Self {
            theme: ColorTheme::new(),
            my_pid,
            my_desc: my_desc.into(),
            last_line_count: 0,
            spinner_idx: 0,
            last_heartbeat_secs: 0,
            last_state_summary: None,
        }
    }

    pub fn tick_queue(&mut self, status: &FullQueueStatus, elapsed_secs: u64) {
        if self.theme.enabled {
            let spinner = SPINNER_FRAMES[self.spinner_idx % SPINNER_FRAMES.len()];
            self.spinner_idx += 1;

            let lines = render_full_dashboard(
                status,
                Some(self.my_pid),
                Some((spinner, elapsed_secs)),
                &self.theme,
            );

            let mut stdout = std::io::stdout().lock();
            if self.last_line_count > 0 {
                let _ = write!(stdout, "\x1b[{}A\r", self.last_line_count);
            }

            for line in &lines {
                let _ = writeln!(stdout, "\x1b[2K{}", line);
            }
            let _ = stdout.flush();
            self.last_line_count = lines.len();
        } else {
            // Non-TTY / CI mode: event-driven + sparse heartbeat
            let pos = status
                .waiters
                .iter()
                .position(|w| w.pid == self.my_pid)
                .map(|i| i + 1)
                .unwrap_or(1);

            let summary = if let Some(bench) = &status.active_bench {
                format!("Blocked by Benchmark PID {}", bench.pid)
            } else if let Some(pause) = &status.pause_state {
                format!("Blocked by Pause '{}'", pause.reason)
            } else if !status.active_tasks.is_empty() {
                let first = &status.active_tasks[0];
                let held = (chrono::Utc::now().timestamp() - first.start_time.timestamp()).max(0);
                if held as u64 >= super::status::LONG_HOLD_HINT_SECS {
                    format!(
                        "Waiting on budget {}/{} (PID {} holding {}: dev servers should run with --observer)",
                        status.budget_used,
                        status.budget_total,
                        first.pid,
                        format_duration_secs(held as u64)
                    )
                } else {
                    format!(
                        "Waiting on budget {}/{} ({} active task(s), lead PID {})",
                        status.budget_used,
                        status.budget_total,
                        status.active_tasks.len(),
                        first.pid
                    )
                }
            } else {
                "Waiting for budget".to_string()
            };

            let state_key = format!("pos:{},{}", pos, summary);
            let state_changed = self.last_state_summary.as_deref() != Some(&state_key);

            if state_changed {
                println!(
                    "{} Queued at position #{}. {} [elapsed: {}]...",
                    self.theme.badge_bque(),
                    pos,
                    summary,
                    format_duration_secs(elapsed_secs)
                );
                self.last_state_summary = Some(state_key);
                self.last_heartbeat_secs = elapsed_secs;
            } else if elapsed_secs >= self.last_heartbeat_secs + 30 {
                println!(
                    "{} (heartbeat) Still queued at position #{}. {} [elapsed: {}]...",
                    self.theme.badge_bque(),
                    pos,
                    summary,
                    format_duration_secs(elapsed_secs)
                );
                self.last_heartbeat_secs = elapsed_secs;
            }
        }
    }

    pub fn finish_acquired(&mut self, elapsed_secs: u64) {
        if self.theme.enabled && self.last_line_count > 0 {
            let mut stdout = std::io::stdout().lock();
            let _ = write!(stdout, "\x1b[{}A\r\x1b[J", self.last_line_count);
            let _ = stdout.flush();
            self.last_line_count = 0;
        }

        if elapsed_secs >= 1 {
            println!(
                "{} {} Budget acquired after {}. Running: {}",
                self.theme.badge_bque(),
                self.theme.bold_green("🚀"),
                self.theme.green(&format_duration_secs(elapsed_secs)),
                self.theme.bold_white(&self.my_desc)
            );
        }
    }

    pub fn tick_bench(&mut self, status: &FullQueueStatus, elapsed_secs: u64) {
        if self.theme.enabled {
            let spinner = SPINNER_FRAMES[self.spinner_idx % SPINNER_FRAMES.len()];
            self.spinner_idx += 1;

            let lines = render_full_dashboard(
                status,
                Some(self.my_pid),
                Some((spinner, elapsed_secs)),
                &self.theme,
            );

            let mut stdout = std::io::stdout().lock();
            if self.last_line_count > 0 {
                let _ = write!(stdout, "\x1b[{}A\r", self.last_line_count);
            }

            for line in &lines {
                let _ = writeln!(stdout, "\x1b[2K{}", line);
            }
            let _ = stdout.flush();
            self.last_line_count = lines.len();
        } else if elapsed_secs >= self.last_heartbeat_secs + 15 || self.last_heartbeat_secs == 0 {
            println!(
                "{} Waiting for previous benchmark to complete [elapsed: {}]...",
                self.theme.badge_bench(),
                format_duration_secs(elapsed_secs)
            );
            self.last_heartbeat_secs = elapsed_secs;
        }
    }

    pub fn finish_bench_acquired(&mut self, elapsed_secs: u64) {
        if self.theme.enabled && self.last_line_count > 0 {
            let mut stdout = std::io::stdout().lock();
            let _ = write!(stdout, "\x1b[{}A\r\x1b[J", self.last_line_count);
            let _ = stdout.flush();
            self.last_line_count = 0;
        }

        if elapsed_secs >= 1 {
            println!(
                "{} {} Exclusive benchmark lock acquired after {}.",
                self.theme.badge_bench(),
                self.theme.bold_green("🚀"),
                self.theme.green(&format_duration_secs(elapsed_secs))
            );
        }
    }
}
