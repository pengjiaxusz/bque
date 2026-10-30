# justfile — 常用任务
# 跨平台 shell：unix 用 sh，Windows 用 PowerShell，不强制 bash
[unix]
set shell := ["sh", "-uc"]
[windows]
set shell := ["powershell", "-NoProfile", "-Command"]

default:
    @just --list

# 全局跨工作树编译排队状态
queue-status:
    @pwsh -NoProfile -File tools/bque-runner.ps1 -Status

# 应急清理全局编译锁状态
queue-clean:
    @pwsh -NoProfile -File tools/bque-runner.ps1 -Clean

# 手动暂停后台编译（让路游戏或交互压测）
queue-pause:
    @pwsh -NoProfile -File tools/bque-runner.ps1 -Pause

# 手动恢复后台编译
queue-resume:
    @pwsh -NoProfile -File tools/bque-runner.ps1 -Resume

# 构建
[windows]
build *ARGS:
    @pwsh -NoProfile -File tools/bque-runner.ps1 -Desc "Cargo Build" -Cmd "cargo build {{ ARGS }}"

[unix]
build *ARGS:
    cargo build {{ ARGS }}

# 检查（clippy 全开）
[windows]
check *ARGS:
    @pwsh -NoProfile -File tools/bque-runner.ps1 -Desc "Cargo Clippy" -Cmd "cargo clippy --all-targets --all-features -- -D warnings {{ ARGS }}"

[unix]
check *ARGS:
    cargo clippy --all-targets --all-features -- -D warnings {{ ARGS }}

# 测试
[windows]
test *ARGS:
    @pwsh -NoProfile -File tools/bque-runner.ps1 -Desc "Cargo Test" -Cmd "cargo test --all-features {{ ARGS }}"

[unix]
test *ARGS:
    cargo test --all-features {{ ARGS }}

# 专项基准测试（bque bench 自动挂起后台编译以提供纯净 CPU 环境）
[windows]
bench *ARGS:
    @pwsh -NoProfile -File tools/bque-runner.ps1 -Bench -Desc "Cargo Bench" -Cmd "cargo bench {{ ARGS }}"

[unix]
bench *ARGS:
    cargo bench {{ ARGS }}

# 格式化检查
fmt:
    cargo fmt --check

# 一键全检查（CI 用）
ci: fmt check test

# 一键门禁
gate: ci
alias check-gate := gate
