
# bque Installation & Environment Setup Guide

## One-Click Fast Install & Update (Recommended)

Run the following command directly in PowerShell. **No administrator privileges required**; it automatically detects existing `bque` instances in `PATH` and performs hot replacement in-place, or installs to user tools directory and updates `PATH`. Takes effect immediately in the current session:

```powershell
# Standard one-click install/update (PowerShell 7+ / Default)
irm https://github.com/pengjiaxusz/bque/releases/latest/download/install.ps1 | iex

# Fallback compatible command (Windows PowerShell 5.1 / Proxy environments)
iex (curl.exe -sL https://github.com/pengjiaxusz/bque/releases/latest/download/install.ps1 | Out-String)
```

- **Default install path**: `$env:LOCALAPPDATA\bque\bin\bque.exe`
- **Key benefits**:
  - **Install equals update**: If an older `bque` exists (including older instances installed via `cargo install` in `.cargo/bin`), the script automatically detects and hot-updates them, eliminating PATH precedence shadowing;
  - Automatically writes to current user persistent `PATH` environment variable (no manual GUI editing, no UAC elevation);
  - Automatically injects into current PowerShell session without restarting the terminal;
  - Out of the box for frontend, C++, or environments without Rust/Cargo installed.

## CLI Self-Update (When bque is already installed)

If `bque` is already installed, update directly via the built-in self-update command:

```powershell
# Check for new version
bque update --check

# One-click upgrade to latest release
bque update
```

The background runner silently and asynchronously checks for updates every 24 hours (cached in `$env:LOCALAPPDATA\bque\update_cache.json`). When a newer version is discovered, a friendly notification will be printed upon command completion (without interrupting any build).

## Manual Binary Installation (Offline / Fallback)

1. Navigate to GitHub Releases:
   👉 **https://github.com/pengjiaxusz/bque/releases/latest**
2. Download the latest `bque-windows-x86_64.zip` (or `bque.exe`).
3. Extract `bque.exe` into a user directory (recommended `$env:LOCALAPPDATA\bque\bin`):
   ```powershell
   $targetDir = "$env:LOCALAPPDATA\bque\bin"
   New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
   # Place extracted bque.exe into $targetDir
   ```
4. Append directory to user `PATH`:
   ```powershell
   [Environment]::SetEnvironmentVariable(
       "Path",
       [Environment]::GetEnvironmentVariable("Path", "User") + ";$env:LOCALAPPDATA\bque\bin",
       "User"
   )
   ```
5. Verify installation:
   ```powershell
   bque --version
   ```

## Build & Install from Source

If a standard Rust toolchain is configured on the machine:
```powershell
git clone https://github.com/pengjiaxusz/bque.git
cd bque
cargo install --path .
```

## Health & Connectivity Verification

Prefer the project recipe — it probes `PATH`, falls back to the default install directory and
prints the one-click install command when bque is missing:
```powershell
just queue-doctor
```

To inspect kernel objects and lock status directly:
```powershell
bque status
```
Expected normal output:
```
============================================================
              bque Global Build Queue & Governor            
============================================================

[1] Active Lock Holder:
  (No active lock holder. Build queue is idle)

[2] Queued Waiters (0):
  (Queue is empty)
============================================================
```
