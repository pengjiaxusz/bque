use std::io::IsTerminal;

#[cfg(windows)]
pub fn enable_ansi_support() {
    use windows_sys::Win32::System::Console::{
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE,
        SetConsoleMode,
    };
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if !handle.is_null() && handle != (-1isize) as _ {
            let mut mode = 0;
            if GetConsoleMode(handle, &mut mode) != 0 {
                let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
            }
        }
    }
}

#[cfg(not(windows))]
pub fn enable_ansi_support() {}

#[derive(Clone, Copy, Debug)]
pub struct ColorTheme {
    pub enabled: bool,
}

impl Default for ColorTheme {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorTheme {
    pub fn new() -> Self {
        let is_term = std::io::stdout().is_terminal();
        let no_color = std::env::var_os("NO_COLOR").is_some();
        let enabled = is_term && !no_color;
        if enabled {
            enable_ansi_support();
        }
        Self { enabled }
    }

    pub fn plain() -> Self {
        Self { enabled: false }
    }

    pub fn bold(&self, text: &str) -> String {
        if self.enabled {
            format!("\x1b[1m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    pub fn dim(&self, text: &str) -> String {
        if self.enabled {
            format!("\x1b[90m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    pub fn cyan(&self, text: &str) -> String {
        if self.enabled {
            format!("\x1b[36m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    pub fn bold_cyan(&self, text: &str) -> String {
        if self.enabled {
            format!("\x1b[1;36m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    pub fn green(&self, text: &str) -> String {
        if self.enabled {
            format!("\x1b[32m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    pub fn bold_green(&self, text: &str) -> String {
        if self.enabled {
            format!("\x1b[1;32m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    pub fn yellow(&self, text: &str) -> String {
        if self.enabled {
            format!("\x1b[33m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    pub fn bold_yellow(&self, text: &str) -> String {
        if self.enabled {
            format!("\x1b[1;33m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    pub fn magenta(&self, text: &str) -> String {
        if self.enabled {
            format!("\x1b[35m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    pub fn bold_magenta(&self, text: &str) -> String {
        if self.enabled {
            format!("\x1b[1;35m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    pub fn red(&self, text: &str) -> String {
        if self.enabled {
            format!("\x1b[31m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    pub fn bold_red(&self, text: &str) -> String {
        if self.enabled {
            format!("\x1b[1;31m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    pub fn white(&self, text: &str) -> String {
        if self.enabled {
            format!("\x1b[37m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    pub fn bold_white(&self, text: &str) -> String {
        if self.enabled {
            format!("\x1b[1;37m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    pub fn badge_bque(&self) -> String {
        if self.enabled {
            "\x1b[1;36m[bque]\x1b[0m".to_string()
        } else {
            "[bque]".to_string()
        }
    }

    pub fn badge_bench(&self) -> String {
        if self.enabled {
            "\x1b[1;35m[bque bench]\x1b[0m".to_string()
        } else {
            "[bque bench]".to_string()
        }
    }
}

pub fn truncate_str(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let prefix: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", prefix)
    }
}

/// Shorten a path without losing its tail: `E:\D\...\dunting-qt\worktrees\a`.
///
/// The trailing segments name the project and worktree, which is exactly what
/// identifies a task, so they are kept while the middle is elided.
pub fn compact_path(path: &str, max_chars: usize) -> String {
    if path.chars().count() <= max_chars {
        return path.to_string();
    }

    let separator = if path.contains('\\') { '\\' } else { '/' };
    let segments: Vec<&str> = path.split(separator).filter(|s| !s.is_empty()).collect();
    if segments.len() < 3 || max_chars < 16 {
        return truncate_str(path, max_chars);
    }

    let head = segments[0];
    let head_len = head.chars().count();
    let mut tail: Vec<&str> = Vec::new();
    let mut tail_len = 0usize;
    for segment in segments.iter().skip(1).rev() {
        let cost = segment.chars().count() + 1; // separator + segment
        // 5 chars reserved for the elision marker: head + "\...\".
        if head_len + 5 + tail_len + cost > max_chars {
            break;
        }
        tail_len += cost;
        tail.push(segment);
    }
    if tail.is_empty() {
        return truncate_str(path, max_chars);
    }
    tail.reverse();
    format!(
        "{}{}...{}{}",
        head,
        separator,
        separator,
        tail.join(&separator.to_string())
    )
}

pub fn format_duration_secs(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60)
    }
}

pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c == 'm' {
                in_escape = false;
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 8), "hello...");
    }

    #[test]
    fn test_compact_path_keeps_the_worktree_tail() {
        let path = r"E:\D\pengj\dunting-qt\worktrees\a";
        assert_eq!(compact_path(path, 60), path);
        let short = compact_path(path, 28);
        assert!(short.starts_with("E:\\...\\"), "got: {short}");
        assert!(short.ends_with("worktrees\\a"), "got: {short}");
        assert!(short.chars().count() <= 28, "got: {short}");
    }

    #[test]
    fn test_format_duration_secs() {
        assert_eq!(format_duration_secs(5), "5s");
        assert_eq!(format_duration_secs(65), "1m 05s");
        assert_eq!(format_duration_secs(3665), "1h 01m");
    }

    #[test]
    fn test_strip_ansi() {
        let styled = "\x1b[1;36m[bque]\x1b[0m hello";
        assert_eq!(strip_ansi(styled), "[bque] hello");
    }
}
