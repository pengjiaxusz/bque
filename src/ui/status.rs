use super::color::{ColorTheme, format_duration_secs, truncate_str};
use crate::state::FullQueueStatus;
use chrono::Utc;

pub fn render_full_dashboard(
    status: &FullQueueStatus,
    highlight_pid: Option<u32>,
    live_header: Option<(&str, u64)>, // (spinner_frame, elapsed_secs)
    theme: &ColorTheme,
) -> Vec<String> {
    let mut body_lines = Vec::new();
    let mut need_spacer = false;

    // 1. Active Benchmark
    if let Some(bench) = &status.active_bench {
        if need_spacer {
            body_lines.push(theme.dim("│"));
        }
        body_lines.push(format!(
            "{}  {}",
            theme.dim("│"),
            theme.bold_yellow("🧪 ACTIVE BENCHMARK IN PROGRESS")
        ));
        body_lines.push(format!(
            "{}     PID: {} • {}",
            theme.dim("│"),
            theme.bold_cyan(&bench.pid.to_string()),
            theme.white(&truncate_str(&bench.description, 50))
        ));
        let elapsed = (Utc::now().timestamp() - bench.start_time.timestamp()).max(0) as u64;
        body_lines.push(format!(
            "{}     Started: {} ago • Frozen processes: {}",
            theme.dim("│"),
            theme.dim(&format_duration_secs(elapsed)),
            theme.bold_yellow(&bench.frozen_pids.len().to_string())
        ));
        need_spacer = true;
    }

    // 2. Manual Pause
    if let Some(pause) = &status.pause_state {
        if need_spacer {
            body_lines.push(theme.dim("│"));
        }
        body_lines.push(format!(
            "{}  {}",
            theme.dim("│"),
            theme.bold_magenta("⏸️  BUILDS MANUALLY PAUSED")
        ));
        body_lines.push(format!(
            "{}     Reason: {}",
            theme.dim("│"),
            theme.bold_white(&pause.reason)
        ));
        let elapsed = (Utc::now().timestamp() - pause.paused_at.timestamp()).max(0) as u64;
        body_lines.push(format!(
            "{}     Paused by PID: {} ({} ago)",
            theme.dim("│"),
            theme.bold_cyan(&pause.paused_by_pid.to_string()),
            theme.dim(&format_duration_secs(elapsed))
        ));
        need_spacer = true;
    }

    // 3. Active Lock Holder
    if need_spacer {
        body_lines.push(theme.dim("│"));
    }
    if let Some(lock) = &status.active_lock {
        let elapsed = (Utc::now().timestamp() - lock.start_time.timestamp()).max(0) as u64;
        body_lines.push(format!(
            "{}  {}",
            theme.dim("│"),
            theme.bold_green("🏃 ACTIVE BUILD LOCK")
        ));
        body_lines.push(format!(
            "{}     PID: {} • {}",
            theme.dim("│"),
            theme.bold_cyan(&lock.pid.to_string()),
            theme.bold_white(&truncate_str(&lock.description, 50))
        ));
        lines_priority_cpu(
            &mut body_lines,
            theme,
            elapsed,
            &lock.priority,
            lock.cpu_rate,
        );
        body_lines.push(format!(
            "{}     CWD: {}",
            theme.dim("│"),
            theme.dim(&truncate_str(&lock.cwd, 50))
        ));
    } else {
        body_lines.push(format!(
            "{}  {}",
            theme.dim("│"),
            theme.bold_green("🟢 BUILD QUEUE IDLE")
        ));
        body_lines.push(format!(
            "{}     (No active builds. Lock is immediately available)",
            theme.dim("│")
        ));
    }
    need_spacer = true;

    // 4. Waiters
    if need_spacer {
        body_lines.push(theme.dim("│"));
    }
    body_lines.push(format!(
        "{}  {}",
        theme.dim("│"),
        theme.bold_cyan(&format!("⏳ QUEUED WAITERS ({})", status.waiters.len()))
    ));
    if status.waiters.is_empty() {
        body_lines.push(format!(
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
                body_lines.push(format!(
                    "{}   {} {:<3} {:<10} {} {} (You)",
                    theme.dim("│"),
                    theme.bold_cyan("▸"),
                    theme.bold_cyan(&num),
                    theme.bold_cyan(&pid_str),
                    theme.dim(&time_str),
                    theme.bold_white(&truncate_str(&w.description, 36))
                ));
            } else {
                body_lines.push(format!(
                    "{}     {:<3} {:<10} {} {}",
                    theme.dim("│"),
                    theme.cyan(&num),
                    theme.cyan(&pid_str),
                    theme.dim(&time_str),
                    theme.white(&truncate_str(&w.description, 36))
                ));
            }
        }
    }

    // 5. Observers
    if !status.observers.is_empty() {
        body_lines.push(theme.dim("│"));
        body_lines.push(format!(
            "{}  {}",
            theme.dim("│"),
            theme.bold_white(&format!("👀 OBSERVERS ({})", status.observers.len()))
        ));
        for obs in &status.observers {
            let elapsed = (Utc::now().timestamp() - obs.start_time.timestamp()).max(0) as u64;
            body_lines.push(format!(
                "{}     PID {:<6} [{:>5}] {}",
                theme.dim("│"),
                theme.cyan(&obs.pid.to_string()),
                theme.dim(&format_duration_secs(elapsed)),
                theme.white(&truncate_str(&obs.description, 40))
            ));
        }
    }

    // Determine target card width dynamically based on content
    let content_max = body_lines
        .iter()
        .map(|l| super::color::strip_ansi(l).chars().count())
        .max()
        .unwrap_or(50);

    // Build top and bottom lines with exact matched length
    let (top_line, total_width) = match live_header {
        Some((spinner, elapsed_secs)) => {
            let time_str = format_duration_secs(elapsed_secs);
            // Header prefix: "╭─ bque • Global Build Queue & Governor ── " (45) + spinner (1) + " [" + time + "] "
            let prefix_len = 45 + 1 + 2 + time_str.chars().count() + 2;
            let target = (content_max + 3).max(prefix_len + 4).min(72);
            let trailing = target.saturating_sub(prefix_len).max(3);
            let total = prefix_len + trailing;
            let line = format!(
                "{}{}{}{}{}{}{}{}{}",
                theme.dim("╭─ "),
                theme.bold_cyan("bque"),
                theme.dim(" • Global Build Queue & Governor ── "),
                theme.bold_yellow(spinner),
                theme.dim(" ["),
                theme.bold_cyan(&time_str),
                theme.dim("] "),
                theme.dim(&"─".repeat(trailing)),
                ""
            );
            (line, total)
        }
        None => {
            // Standard static header: "╭─ bque • Global Build Queue & Governor " (42)
            let prefix_len = 42;
            let target = (content_max + 3).max(prefix_len + 4).min(72);
            let trailing = target.saturating_sub(prefix_len).max(4);
            let total = prefix_len + trailing;
            let line = format!(
                "{}{}{}{}",
                theme.dim("╭─ "),
                theme.bold_cyan("bque"),
                theme.dim(" • Global Build Queue & Governor "),
                theme.dim(&"─".repeat(trailing))
            );
            (line, total)
        }
    };

    let bottom_prefix_len = 2; // "╰─"
    let bottom_trailing = total_width.saturating_sub(bottom_prefix_len);
    let bottom_line = format!(
        "{}{}",
        theme.dim("╰─"),
        theme.dim(&"─".repeat(bottom_trailing))
    );

    let mut result = Vec::with_capacity(body_lines.len() + 2);
    result.push(top_line);
    result.extend(body_lines);
    result.push(bottom_line);

    result
}

fn lines_priority_cpu(
    lines: &mut Vec<String>,
    theme: &ColorTheme,
    elapsed: u64,
    priority: &str,
    cpu_rate: u32,
) {
    lines.push(format!(
        "{}     Running: {} • Priority: {} • CPU: {}% (soft-cap)",
        theme.dim("│"),
        theme.green(&format_duration_secs(elapsed)),
        theme.cyan(priority),
        theme.bold(&cpu_rate.to_string())
    ));
}
