use crate::ui::ColorTheme;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const DEFAULT_CHECK_INTERVAL_HOURS: i64 = 24;
const DEFAULT_NOTIFY_INTERVAL_HOURS: i64 = 24;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UpdateCache {
    pub last_checked: DateTime<Utc>,
    pub latest_version: String,
    pub last_notified: Option<DateTime<Utc>>,
}

pub fn get_cache_path() -> PathBuf {
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        let dir = PathBuf::from(local_app_data).join("bque");
        let _ = std::fs::create_dir_all(&dir);
        dir.join("update_cache.json")
    } else {
        let dir = std::env::temp_dir().join("bque");
        let _ = std::fs::create_dir_all(&dir);
        dir.join("update_cache.json")
    }
}

pub fn load_cache() -> Option<UpdateCache> {
    let path = get_cache_path();
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn save_cache(cache: &UpdateCache) {
    let path = get_cache_path();
    if let Ok(data) = serde_json::to_string_pretty(cache) {
        let _ = std::fs::write(path, data);
    }
}

pub fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let clean = v.trim().trim_start_matches('v');
    let mut parts = clean.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    let patch: u32 = parts.next()?.split('-').next()?.parse().ok()?;
    Some((major, minor, patch))
}

pub fn is_newer_version(current: &str, remote: &str) -> bool {
    match (parse_version(current), parse_version(remote)) {
        (Some(curr), Some(rem)) => rem > curr,
        _ => false,
    }
}

pub fn fetch_remote_latest_version() -> Option<String> {
    // 1. Try curl.exe with HEAD redirect (instant, zero rate-limit)
    if let Ok(output) = std::process::Command::new("curl.exe")
        .args([
            "-ILs",
            "-o",
            "NUL",
            "-w",
            "%{url_effective}",
            "--max-time",
            "3",
            "https://github.com/pengjiaxusz/bque/releases/latest",
        ])
        .output()
        && output.status.success()
    {
        let url = String::from_utf8_lossy(&output.stdout);
        if let Some(pos) = url.rfind("/tag/") {
            let tag = url[pos + 5..].trim();
            if !tag.is_empty() {
                return Some(tag.to_string());
            }
        }
    }

    // 2. Fallback: try git ls-remote tags
    if let Ok(git_out) = std::process::Command::new("git")
        .args([
            "ls-remote",
            "--tags",
            "--refs",
            "https://github.com/pengjiaxusz/bque.git",
        ])
        .output()
        && git_out.status.success()
    {
        let text = String::from_utf8_lossy(&git_out.stdout);
        let mut latest: Option<((u32, u32, u32), String)> = None;
        for line in text.lines() {
            if let Some(pos) = line.rfind("refs/tags/") {
                let tag = line[pos + 10..].trim();
                if let Some(parsed) = parse_version(tag) {
                    let is_greater = latest.as_ref().is_none_or(|(p, _)| parsed > *p);
                    if is_greater {
                        latest = Some((parsed, tag.to_string()));
                    }
                }
            }
        }
        if let Some((_, tag)) = latest {
            return Some(tag);
        }
    }

    None
}

/// Spawns a background thread to check for updates if the interval has passed.
/// Never blocks the main command!
pub fn maybe_spawn_background_check() {
    let now = Utc::now();
    let cache = load_cache();

    let check_interval_hours = std::env::var("BQUE_UPDATE_CHECK_INTERVAL_HOURS")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(DEFAULT_CHECK_INTERVAL_HOURS);

    let needs_check = match &cache {
        Some(c) => (now - c.last_checked).num_hours() >= check_interval_hours,
        None => true,
    };

    if needs_check {
        std::thread::spawn(move || {
            if let Some(remote_ver) = fetch_remote_latest_version() {
                let mut current_cache = load_cache().unwrap_or(UpdateCache {
                    last_checked: now,
                    latest_version: remote_ver.clone(),
                    last_notified: None,
                });
                current_cache.last_checked = Utc::now();
                current_cache.latest_version = remote_ver;
                save_cache(&current_cache);
            }
        });
    }
}

/// Checks if an update notification should be displayed, and prints a styled notice if so.
pub fn check_and_print_notice(theme: &ColorTheme) {
    let current_version = env!("CARGO_PKG_VERSION");
    let mut cache = match load_cache() {
        Some(c) => c,
        None => return,
    };

    if !is_newer_version(current_version, &cache.latest_version) {
        return;
    }

    let now = Utc::now();
    let notify_interval_hours = std::env::var("BQUE_UPDATE_NOTIFY_INTERVAL_HOURS")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(DEFAULT_NOTIFY_INTERVAL_HOURS);

    let should_notify = match cache.last_notified {
        Some(last) => (now - last).num_hours() >= notify_interval_hours,
        None => true,
    };

    if should_notify {
        let remote_tag = &cache.latest_version;
        println!();
        println!(
            "{}{}",
            theme.dim("╭─ "),
            theme.bold_yellow("bque update available ──────────────────────────────")
        );
        println!(
            "{} 🔔 A new version of bque is available: {} ➜ {}",
            theme.dim("│"),
            theme.dim(&format!("v{}", current_version)),
            theme.bold_green(remote_tag)
        );
        println!(
            "{}    Run '{}' to upgrade.",
            theme.dim("│"),
            theme.bold_cyan("bque update")
        );
        println!(
            "{}",
            theme.dim("╰──────────────────────────────────────────────────────────")
        );

        cache.last_notified = Some(now);
        save_cache(&cache);
    }
}

/// Automatically removes leftover `.old` executables from previous updates.
pub fn clean_old_binaries() {
    if let Ok(current_exe) = std::env::current_exe() {
        let old_exe = current_exe.with_extension("exe.old");
        if old_exe.exists() {
            let _ = std::fs::remove_file(old_exe);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse_version("v0.1.1"), Some((0, 1, 1)));
        assert_eq!(parse_version("v1.2.3-beta"), Some((1, 2, 3)));
    }

    #[test]
    fn test_is_newer_version() {
        assert!(is_newer_version("0.1.0", "v0.1.1"));
        assert!(is_newer_version("0.1.0", "0.2.0"));
        assert!(is_newer_version("0.1.0", "1.0.0"));
        assert!(!is_newer_version("0.1.1", "v0.1.0"));
        assert!(!is_newer_version("0.1.0", "v0.1.0"));
    }
}
