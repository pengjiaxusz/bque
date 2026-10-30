# bque 安装与环境配置指南 / Installation & Setup


## Precompiled Binary Installation (Recommended)

1. Navigate to the GitHub Releases page:
   👉 **https://github.com/pengjiaxusz/bque/releases/latest**
2. Download `bque-windows-x86_64.zip` (or `bque.exe`).
3. Place `bque.exe` in a persistent tools directory (e.g., `C:\Tools` or via Scoop/winget).
4. Add the directory to your system environment variable `PATH`:
   ```powershell
   # Add to current user PATH permanently
   [Environment]::SetEnvironmentVariable(
       "Path",
       [Environment]::GetEnvironmentVariable("Path", "User") + ";C:\Tools",
       "User"
   )
   ```
5. Reopen your terminal and verify:
   ```powershell
   bque --version
   ```

## Building & Installing from Source

If you have the Rust toolchain installed:
```powershell
git clone https://github.com/pengjiaxusz/bque.git
cd bque
cargo install --path .
```

## Health Check & Verification

Run the following command to verify whether bque is operational:
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

