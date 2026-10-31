use super::color::{ColorTheme, format_duration_secs, truncate_str};
use crate::state::FullQueueStatus;
use chrono::Utc;

pub fn render_full_dashboard(
    status: &FullQueueStatus,
    highlight_pid: Option<u32>,
    theme: &ColorTheme,
) -> Vec<String> {
    let mut lines = Vec::new();

    // Top border
    let bar_len = 50;
    let bar = "─".repeat(bar_len);
    lines.push(format!(
        "{}{}{}{}{}",
        theme.dim("╭─ "),
        theme.bold_cyan("bque"),
        theme.dim(" • Global Build Queue & Governor "),
        theme.dim(&bar),
        ""
    ));

    // Active Benchmark
    if let Some(bench) = &status.active_bench {
        lines.push(theme.dim("│"));
        lines.push(format!(
            "{}  {}",
            theme.dim("│"),
            theme.bold_yellow("🧪 ACTIVE BENCHMARK IN PROGRESS")
        ));
        lines.push(format!(
            "{}     PID: {} • {}",
            theme.dim("│"),
            theme.bold_cyan(&bench.pid.to_string()),
            theme.white(&truncate_str(&bench.description, 60))
        ));
        let elapsed = (Utc::now().timestamp() - bench.start_time.timestamp()).max(0) as u64;
        lines.push(format!(
            "{}     Started: {} ago • Frozen processes: {}",
            theme.dim("│"),
            theme.dim(&format_duration_secs(elapsed)),
            theme.bold_yellow(&bench.frozen_pids.len().to_string())
        ));
    }

    // Manual Pause
    if let Some(pause) = &status.pause_state {
        lines.push(theme.dim("│"));
        lines.push(format!(
            "{}  {}",
            theme.dim("│"),
            theme.bold_magenta("⏸️  BUILDS MANUALLY PAUSED")
        ));
        lines.push(format!(
            "{}     Reason: {}",
            theme.dim("│"),
            theme.bold_white(&pause.reason)
        ));
        let elapsed = (Utc::now().timestamp() - pause.paused_at.timestamp()).max(0) as u64;
        lines.push(format!(
            "{}     Paused by PID: {} ({} ago)",
            theme.dim("│"),
            theme.bold_cyan(&pause.paused_by_pid.to_string()),
            theme.dim(&format_duration_secs(elapsed))
        ));
    }

    // Active Lock Holder
    lines.push(theme.dim("│"));
    if let Some(lock) = &status.active_lock {
        let elapsed = (Utc::now().timestamp() - lock.start_time.timestamp()).max(0) as u64;
        lines.push(format!(
            "{}  {}",
            theme.dim("│"),
            theme.bold_green("🏃 ACTIVE BUILD LOCK")
        ));
        lines.push(format!(
            "{}     PID: {} • {}",
            theme.dim("│"),
            theme.bold_cyan(&lock.pid.to_string()),
            theme.bold_white(&truncate_str(&lock.description, 60))
        ));
        lines.push(format!(
            "{}     Running: {} • Priority: {} • CPU: {}% (soft-cap)",
            theme.dim("│"),
            theme.green(&format_duration_secs(elapsed)),
            theme.cyan(&lock.priority),
            theme.bold(&lock.cpu_rate.to_string())
        ));
        lines.push(format!(
            "{}     CWD: {}",
            theme.dim("│"),
            theme.dim(&truncate_str(&lock.cwd, 65))
        ));
    } else {
        lines.push(format!(
            "{}  {}",
            theme.dim("│"),
            theme.bold_green("🟢 BUILD QUEUE IDLE")
        ));
        lines.push(format!(
            "{}     (No active builds. Lock is immediately available)",
            theme.dim("│")
        ));
    }

    // Waiters
    lines.push(theme.dim("│"));
    lines.push(format!(
        "{}  {}",
        theme.dim("│"),
        theme.bold_cyan(&format!("⏳ QUEUED WAITERS ({})", status.waiters.len()))
    ));
    if status.waiters.is_empty() {
        lines.push(format!(
            "{}     (No waiting tasks in queue)",
            theme.dim("│")
        ));
    } else {
        for (i, w) in status.waiters.iter().enumerate() {
            let elapsed = (Utc::now().timestamp() - w.queued_at.timestamp()).max(0) as u64;
            let is_me = highlight_pid == Some(w.pid);
            let num = format!("#{}", i + 1);
            let pid_str = format!("PID {}", w.pid);
            let time_str = format!("[{:>5}]", format_duration_secs(elapsed));

            if is_me {
                lines.push(format!(
                    "{}   {} {:<3} {:<10} {} {} (You)",
                    theme.dim("│"),
                    theme.bold_cyan("▸"),
                    theme.bold_cyan(&num),
                    theme.bold_cyan(&pid_str),
                    theme.dim(&time_str),
                    theme.bold_white(&truncate_str(&w.description, 45))
                ));
            } else {
                lines.push(format!(
                    "{}     {:<3} {:<10} {} {}",
                    theme.dim("│"),
                    theme.cyan(&num),
                    theme.cyan(&pid_str),
                    theme.dim(&time_str),
                    theme.white(&truncate_str(&w.description, 45))
                ));
            }
        }
    }

    // Observers
    if !status.observers.is_empty() {
        lines.push(theme.dim("│"));
        lines.push(format!(
            "{}  {}",
            theme.dim("│"),
            theme.bold_white(&format!("👀 OBSERVERS ({})", status.observers.len()))
        ));
        for obs in &status.observers {
            let elapsed = (Utc::now().timestamp() - obs.start_time.timestamp()).max(0) as u64;
            lines.push(format!(
                "{}     PID {:<6} [{:>5}] {}",
                theme.dim("│"),
                theme.cyan(&obs.pid.to_string()),
                theme.dim(&format_duration_secs(elapsed)),
                theme.white(&truncate_str(&obs.description, 50))
            ));
        }
    }

    // Bottom border
    lines.push(theme.dim("│"));
    lines.push(format!(
        "{}{}",
        theme.dim("╰─"),
        theme.dim(&"─".repeat(bar_len + 30))
    ));

    lines
}

pub fn render_compact_queue_widget(
    status: &FullQueueStatus,
    my_pid: u32,
    my_desc: &str,
    elapsed_secs: u64,
    spinner_frame: &str,
    theme: &ColorTheme,
) -> Vec<String> {
    let mut lines = Vec::new();
    let bar_len = 45;

    // Header with animated spinner and timer
    lines.push(format!(
        "{}{}{}{}{}{}{}",
        theme.dim("╭─ "),
        theme.bold_cyan("bque queue"),
        theme.dim(" ── "),
        theme.bold_yellow(spinner_frame),
        theme.dim(" Waiting for lock ["),
        theme.bold_cyan(&format_duration_secs(elapsed_secs)),
        theme.dim(&format!("] ─{}", "─".repeat(bar_len)))
    ));

    // Blocker status
    if let Some(bench) = &status.active_bench {
        let b_elapsed = (Utc::now().timestamp() - bench.start_time.timestamp()).max(0) as u64;
        lines.push(format!(
            "{} {} Blocked by Benchmark: PID {} ('{}')",
            theme.dim("│"),
            theme.bold_yellow("🧪"),
            theme.bold_cyan(&bench.pid.to_string()),
            theme.bold_yellow(&truncate_str(&bench.description, 40))
        ));
        lines.push(format!(
            "{}    Running for: {} • Background builds frozen",
            theme.dim("│"),
            theme.dim(&format_duration_secs(b_elapsed))
        ));
    } else if let Some(pause) = &status.pause_state {
        lines.push(format!(
            "{} {} Blocked by Manual Pause: '{}'",
            theme.dim("│"),
            theme.bold_magenta("⏸️ "),
            theme.bold_white(&pause.reason)
        ));
    } else if let Some(lock) = &status.active_lock {
        let l_elapsed = (Utc::now().timestamp() - lock.start_time.timestamp()).max(0) as u64;
        lines.push(format!(
            "{} {} Lock Holder: PID {} ('{}')",
            theme.dim("│"),
            theme.bold_green("🏃"),
            theme.bold_cyan(&lock.pid.to_string()),
            theme.white(&truncate_str(&lock.description, 40))
        ));
        lines.push(format!(
            "{}    Running for: {} • Priority: {} • CPU: {}%",
            theme.dim("│"),
            theme.green(&format_duration_secs(l_elapsed)),
            theme.cyan(&lock.priority),
            theme.dim(&lock.cpu_rate.to_string())
        ));
    } else {
        lines.push(format!(
            "{} {} Lock is clearing, acquiring ownership...",
            theme.dim("│"),
            theme.bold_green("🟢")
        ));
    }

    // Queue position
    let pos = status
        .waiters
        .iter()
        .position(|w| w.pid == my_pid)
        .map(|i| i + 1)
        .unwrap_or(1);
    let total = status.waiters.len().max(1);

    if total > 1 {
        lines.push(format!(
            "{} {} Queue Position: {} of {}",
            theme.dim("│"),
            theme.bold_cyan("⏳"),
            theme.bold_cyan(&format!("#{}", pos)),
            theme.dim(&total.to_string())
        ));
    } else {
        lines.push(format!(
            "{} {} Queue Position: {} (Next in line)",
            theme.dim("│"),
            theme.bold_cyan("⏳"),
            theme.bold_cyan(&format!("#{}", pos))
        ));
    }

    // My Task line
    lines.push(format!(
        "{} {} Your Task: PID {} ('{}')",
        theme.dim("│"),
        theme.bold_cyan("▸"),
        theme.cyan(&my_pid.to_string()),
        theme.bold_white(&truncate_str(my_desc, 45))
    ));

    // Bottom border
    lines.push(format!(
        "{}{}",
        theme.dim("╰─"),
        theme.dim(&"─".repeat(bar_len + 25))
    ));

    lines
}

pub fn render_compact_bench_widget(
    prev_bench: Option<(&u32, &str)>,
    elapsed_secs: u64,
    spinner_frame: &str,
    theme: &ColorTheme,
) -> Vec<String> {
    let mut lines = Vec::new();
    let bar_len = 45;

    lines.push(format!(
        "{}{}{}{}{}{}{}",
        theme.dim("╭─ "),
        theme.bold_magenta("bque bench"),
        theme.dim(" ── "),
        theme.bold_yellow(spinner_frame),
        theme.dim(" Waiting for exclusive lock ["),
        theme.bold_cyan(&format_duration_secs(elapsed_secs)),
        theme.dim(&format!("] ─{}", "─".repeat(bar_len)))
    ));

    if let Some((pid, desc)) = prev_bench {
        lines.push(format!(
            "{} {} Blocked by previous benchmark: PID {} ('{}')",
            theme.dim("│"),
            theme.bold_yellow("🧪"),
            theme.bold_cyan(&pid.to_string()),
            theme.white(&truncate_str(desc, 40))
        ));
    } else {
        lines.push(format!(
            "{} {} Waiting for previous benchmark to release lock...",
            theme.dim("│"),
            theme.bold_yellow("🧪")
        ));
    }

    lines.push(format!(
        "{}{}",
        theme.dim("╰─"),
        theme.dim(&"─".repeat(bar_len + 25))
    ));

    lines
}
