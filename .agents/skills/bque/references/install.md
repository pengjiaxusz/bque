# bque 安装与环境配置指南 / Installation & Setup

## One-Liner Quick Install (Recommended)

Run the following command in PowerShell. It requires **no administrator privileges**, automatically downloads the latest release, installs it to your user directory, updates your `PATH`, and takes effect in the current session immediately:

```powershell
irm https://github.com/pengjiaxusz/bque/releases/latest/download/install.ps1 | iex
```

- **Default installation path**: `$env:LOCALAPPDATA\bque\bin\bque.exe`
- **Features**:
  - Automatically configures current user persistent `PATH` (no UAC elevation needed);
  - Injects into current terminal session directly (no restart required);
  - Non-Rust users can use it out of the box.

## Manual Binary Installation (Alternative)

1. Navigate to the GitHub Releases page:
   👉 **https://github.com/pengjiaxusz/bque/releases/latest**
2. Download `bque-windows-x86_64.zip` (or `bque.exe`).
3. Extract `bque.exe` to a dedicated user tools directory, preferably `$env:LOCALAPPDATA\bque\bin`:
   ```powershell
   $targetDir = "$env:LOCALAPPDATA\bque\bin"
   New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
   # Move extracted bque.exe into $targetDir
   ```
4. Add the directory to your persistent user environment variable `PATH`:
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

## Building & Installing from Source

If you have the standard Rust toolchain installed:
```powershell
git clone https://github.com/pengjiaxusz/bque.git
cd bque
cargo install --path .
```

## Health Check & Verification

Run the status introspection command to verify that bque kernel objects and global locks are working properly:
```powershell
bque status
```
Expected output:
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
