# just/bque.just — global cross-worktree build queue (managed by the bque layer; template updates overwrite this file)
# bque is a Windows global kernel lock scheduler: the unix branch does not queue and only passes the command through.
# Every heavy command should go through q-run / q-bench to avoid saturating the CPU and polluting benchmarks.

# Inspect the global cross-worktree queue (lock holder / waiters / CPU budget)
[windows]
queue-status:
    @bque status
[unix]
queue-status:
    @echo "bque is Windows-only: no global build queue on this platform"

# Emergency cleanup of stale build locks and orphaned waiter records
[windows]
queue-clean:
    @bque clean
[unix]
queue-clean:
    @echo "bque is Windows-only: no global build lock to clean"

# Manually pause background builds (for gaming / interactive profiling / system-wide tracing)
[windows]
queue-pause:
    @bque pause
[unix]
queue-pause:
    @echo "bque is Windows-only: nothing to pause"

# Manually resume background builds
[windows]
queue-resume:
    @bque resume
[unix]
queue-resume:
    @echo "bque is Windows-only: nothing to resume"

# Environment check: probe bque on PATH, fall back to the default install directory, and print a one-line installer when missing
[windows]
queue-doctor:
    @$cmd = Get-Command bque -ErrorAction SilentlyContinue; if (-not $cmd -and (Test-Path "$env:LOCALAPPDATA\bque\bin\bque.exe")) { $env:Path = "$env:LOCALAPPDATA\bque\bin;$env:Path"; $cmd = Get-Command bque -ErrorAction SilentlyContinue }; if ($cmd) { Write-Host "bque: $($cmd.Source)"; bque --version; bque status } else { Write-Host 'bque missing. Install: irm https://github.com/pengjiaxusz/bque/releases/latest/download/install.ps1 | iex' }
[unix]
queue-doctor:
    @echo "bque is Windows-only: no installation check needed"

# Usage: just q-run cargo build --release (do not add `--`; it becomes part of the command)
# Generic queued runner: put any heavy command under the global lock + Idle priority
[windows]
q-run *ARGS:
    @bque run -- {{ARGS}}
[unix]
q-run *ARGS:
    {{ARGS}}

# Usage: just q-bench cargo bench --workspace (do not add `--`)
# Clean-room benchmark: exclusive lock + second-level suspension of background builds for 0% CPU interference
[windows]
q-bench *ARGS:
    @bque bench -- {{ARGS}}
[unix]
q-bench *ARGS:
    {{ARGS}}