use super::checker::{
    clean_old_binaries, fetch_remote_latest_version, is_newer_version, sidecar_binary_path,
};
use crate::cli::UpdateArgs;
use crate::ui::ColorTheme;
use anyhow::{Result, anyhow};

fn release_binary_asset() -> Option<&'static str> {
    if cfg!(windows) {
        Some("bque.exe")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("bque-linux-x86_64")
    } else {
        None
    }
}

pub fn handle_update(args: UpdateArgs) -> Result<()> {
    let theme = ColorTheme::new();
    let current_version = env!("CARGO_PKG_VERSION");

    clean_old_binaries();

    println!("{} Checking for updates...", theme.badge_bque());
    let target_tag = if let Some(tag) = &args.tag {
        tag.clone()
    } else {
        match fetch_remote_latest_version() {
            Some(v) => v,
            None => {
                return Err(anyhow!(
                    "Failed to check latest release from GitHub. Check your network connection or visit https://github.com/pengjiaxusz/bque/releases"
                ));
            }
        }
    };

    let has_update = is_newer_version(current_version, &target_tag);

    if args.check {
        if has_update {
            println!(
                "{} 🔔 A new version is available: {} ➜ {}",
                theme.badge_bque(),
                theme.dim(&format!("v{}", current_version)),
                theme.bold_green(&target_tag)
            );
            println!("  Run '{}' to upgrade.", theme.bold_cyan("bque update"));
        } else {
            println!(
                "{} ✔ bque is up to date (v{}).",
                theme.badge_bque(),
                current_version
            );
        }
        return Ok(());
    }

    if !has_update && !args.force {
        println!(
            "{} ✔ bque is already on the latest version (v{}). Use '--force' to reinstall.",
            theme.badge_bque(),
            current_version
        );
        return Ok(());
    }

    println!(
        "{} 🚀 Updating bque from v{} ➜ {}...",
        theme.badge_bque(),
        current_version,
        theme.bold_green(&target_tag)
    );

    let current_exe = std::env::current_exe()?;
    let temp_download = sidecar_binary_path(&current_exe, ".download");
    let old_exe = sidecar_binary_path(&current_exe, ".old");

    let download_ok = if let Some(asset) = release_binary_asset() {
        let download_url = if let Some(tag) = &args.tag {
            format!(
                "https://github.com/pengjiaxusz/bque/releases/download/{}/{}",
                tag, asset
            )
        } else {
            format!(
                "https://github.com/pengjiaxusz/bque/releases/latest/download/{}",
                asset
            )
        };

        println!("--> Downloading {}...", theme.dim(&download_url));

        let curl_bin = if cfg!(windows) { "curl.exe" } else { "curl" };
        match std::process::Command::new(curl_bin)
            .args([
                "-fLs",
                "--connect-timeout",
                "10",
                "-o",
                temp_download.to_string_lossy().as_ref(),
                &download_url,
            ])
            .status()
        {
            Ok(s) => s.success() && temp_download.exists(),
            Err(_) => false,
        }
    } else {
        false
    };

    if download_ok {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o755);
            std::fs::set_permissions(&temp_download, perms)?;
        }

        // Safe hot-swap: rename running binary to .old, move downloaded binary into place
        if old_exe.exists() {
            let _ = std::fs::remove_file(&old_exe);
        }
        std::fs::rename(&current_exe, &old_exe)?;
        std::fs::rename(&temp_download, &current_exe)?;
        let _ = std::fs::remove_file(&old_exe);

        println!(
            "{} ✨ Successfully updated bque to {}!",
            theme.badge_bque(),
            theme.bold_green(&target_tag)
        );
        println!(
            "  Installed at: {}",
            theme.dim(&current_exe.display().to_string())
        );
        return Ok(());
    }

    // Fallback: If direct binary download failed, try cargo install
    println!(
        "{} Direct download unavailable, falling back to cargo install...",
        theme.badge_bque()
    );

    let cargo_status = std::process::Command::new("cargo")
        .args([
            "install",
            "--git",
            "https://github.com/pengjiaxusz/bque.git",
            "--force",
        ])
        .status();

    if let Ok(s) = cargo_status
        && s.success()
    {
        println!(
            "{} ✨ Successfully installed latest bque via cargo!",
            theme.badge_bque()
        );
        return Ok(());
    }

    let manual_cmd = if cfg!(windows) {
        "irm https://github.com/pengjiaxusz/bque/releases/latest/download/install.ps1 | iex"
    } else {
        "curl -fsSL https://github.com/pengjiaxusz/bque/releases/latest/download/install.sh | sh"
    };

    Err(anyhow!(
        "Failed to update bque. You can update manually using:\n  {}",
        manual_cmd
    ))
}
