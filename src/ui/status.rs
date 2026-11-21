use super::color::{ColorTheme, compact_path, format_duration_secs, truncate_str};
use crate::state::FullQueueStatus;
use chrono::{DateTime, Local, Utc};

/// A holder older than this is almost always a dev server or watch loop that
/// should have run as an observer instead of occupying the global lock.
pub const LONG_HOLD_HINT_SECS: u64 = 30 * 60;

/// Width limit for the status card. Wider than the old one-line layout because
/// every task now carries its working directory and command line, which is what
/// tells a developer *which* worktree a job belongs to.
pub const MAX_CARD_WIDTH: usize = 100;

/// Room left for a detail value after the `│` gutter, indent and 4-char label.
const DETAIL_BUDGET: usize = MAX_CARD_WIDTH - 13;

/// Plain width of `╭─ bque • Global Build Queue & Governor `.
const STATIC_PREFIX_WIDTH: usize = 40;
/// Plain width of `╭─ bque • Global Build Queue & Governor ── `.
const LIVE_PREFIX_WIDTH: usize = 43;

/// Description budget on the task headline, leaving room for the PID, timer and
/// the `(You)` marker.
const HEADLINE_BUDGET: usize = MAX_CARD_WIDTH - 40;

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
        body_lines.push(section_title(
            theme,
            theme.bold_yellow("🧪 ACTIVE BENCHMARK IN PROGRESS"),
        ));
        body_lines.push(headline(theme, &bench.pid, &bench.description));
        push_details(
            &mut body_lines,
            theme,
            Some(&bench.cwd),
            &bench.command,
            &bench.description,
        );
        let elapsed = (Utc::now().timestamp() - bench.start_time.timestamp()).max(0) as u64;
        let mut meta = vec![
            format!("running {}", format_duration_secs(elapsed)),
            format!("started {}", clock(bench.start_time)),
        ];
        if !bench.frozen_pids.is_empty() {
            meta.push(format!("frozen {} process(es)", bench.frozen_pids.len()));
        }
        body_lines.push(meta_line(theme, &meta));
        need_spacer = true;
    }

    // 2. Manual Pause
    if let Some(pause) = &status.pause_state {
        if need_spacer {
            body_lines.push(theme.dim("│"));
        }
        body_lines.push(section_title(
            theme,
            theme.bold_magenta("⏸️  BUILDS MANUALLY PAUSED"),
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
        body_lines.push(section_title(
            theme,
            theme.bold_green("🏃 ACTIVE BUILD LOCK"),
        ));
        body_lines.push(headline(theme, &lock.pid, &lock.description));
        push_details(
            &mut body_lines,
            theme,
            Some(&lock.cwd),
            &lock.command,
            &lock.description,
        );
        body_lines.push(meta_line(
            theme,
            &[
                lock.priority.clone(),
                format!("CPU {}% (soft-cap)", lock.cpu_rate),
                format!("held {}", format_duration_secs(elapsed)),
                format!("started {}", clock(lock.start_time)),
            ],
        ));
        if elapsed >= LONG_HOLD_HINT_SECS {
            body_lines.push(format!(
                "{}     {}",
                theme.dim("│"),
                theme.yellow(
                    "💡 Holder is long-running: dev servers and watch loops should use --observer"
                )
            ));
        }
    } else {
        body_lines.push(section_title(
            theme,
            theme.bold_green("🟢 BUILD QUEUE IDLE"),
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
                    theme.bold_white(&truncate_str(&w.description, HEADLINE_BUDGET))
                ));
            } else {
                body_lines.push(format!(
                    "{}     {:<3} {:<10} {} {}",
                    theme.dim("│"),
                    theme.cyan(&num),
                    theme.cyan(&pid_str),
                    theme.dim(&time_str),
                    theme.white(&truncate_str(&w.description, HEADLINE_BUDGET))
                ));
            }
            push_details(
                &mut body_lines,
                theme,
                Some(&w.cwd),
                &w.command,
                &w.description,
            );
            body_lines.push(meta_line(
                theme,
                &[format!("queued {}", clock(w.queued_at))],
            ));
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
            // Pad before colouring: escape sequences would defeat the alignment.
            let time_str = format!("[{:>5}]", format_duration_secs(elapsed));
            body_lines.push(format!(
                "{}     PID {:<6} {} {}",
                theme.dim("│"),
                theme.cyan(&obs.pid.to_string()),
                theme.dim(&time_str),
                theme.white(&truncate_str(&obs.description, HEADLINE_BUDGET))
            ));
            push_details(
                &mut body_lines,
                theme,
                Some(&obs.cwd),
                &obs.command,
                &obs.description,
            );
            body_lines.push(meta_line(
                theme,
                &[
                    obs.priority.clone(),
                    format!("CPU {}% (soft-cap)", obs.cpu_rate),
                    format!("since {}", clock(obs.start_time)),
                ],
            ));
        }
    }

    // 6. Stale records reaped while building this snapshot
    if !status.reaped.is_empty() {
        body_lines.push(theme.dim("│"));
        body_lines.push(format!(
            "{}  {}",
            theme.dim("│"),
            theme.bold_yellow(&format!(
                "🧹 AUTO-REAPED STALE RECORDS ({})",
                status.reaped.total()
            ))
        ));
        body_lines.push(format!(
            "{}     Removed: {} (owner gone or PID recycled)",
            theme.dim("│"),
            theme.dim(&status.reaped.describe())
        ));
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
            // `╭─ bque • Global Build Queue & Governor ── ` is 43 plain chars; then
            // the spinner, " [", the timer and "] ".
            let prefix_len = LIVE_PREFIX_WIDTH + 1 + 2 + time_str.chars().count() + 2;
            let target = (content_max + 3).max(prefix_len + 4).min(MAX_CARD_WIDTH);
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
            let prefix_len = STATIC_PREFIX_WIDTH;
            let target = (content_max + 3).max(prefix_len + 4).min(MAX_CARD_WIDTH);
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

/// Section heading: `│  <title>`, with the title already coloured by the caller.
fn section_title(theme: &ColorTheme, title: String) -> String {
    format!("{}  {}", theme.dim("│"), title)
}

/// `│     PID 1234 • task summary`
fn headline(theme: &ColorTheme, pid: &u32, description: &str) -> String {
    format!(
        "{}     PID {} • {}",
        theme.dim("│"),
        theme.bold_cyan(&pid.to_string()),
        theme.bold_white(&truncate_str(description, HEADLINE_BUDGET))
    )
}

/// Working directory and command line for one task.
///
/// The directory is the part that answers "whose job is this?" once several
/// worktrees of the same repository share one machine-wide queue, so it is
/// always rendered, never tucked behind a flag.
fn push_details(
    lines: &mut Vec<String>,
    theme: &ColorTheme,
    cwd: Option<&str>,
    command: &str,
    description: &str,
) {
    if let Some(cwd) = cwd.filter(|cwd| !cwd.trim().is_empty()) {
        lines.push(format!(
            "{}       {} {}",
            theme.dim("│"),
            theme.dim("dir "),
            theme.cyan(&compact_path(cwd, DETAIL_BUDGET))
        ));
    }
    // Only repeat the command when a custom description hides it.
    if !command.trim().is_empty() && command != description {
        lines.push(format!(
            "{}       {} {}",
            theme.dim("│"),
            theme.dim("cmd "),
            theme.white(&truncate_str(command, DETAIL_BUDGET))
        ));
    }
}

/// `│       meta priority · CPU 60% (soft-cap) · held 21m 03s · started 06:40:12`
fn meta_line(theme: &ColorTheme, parts: &[String]) -> String {
    format!(
        "{}       {} {}",
        theme.dim("│"),
        theme.dim("meta"),
        theme.dim(&truncate_str(&parts.join(" · "), DETAIL_BUDGET))
    )
}

fn clock(at: DateTime<Utc>) -> String {
    at.with_timezone(&Local).format("%H:%M:%S").to_string()
}
