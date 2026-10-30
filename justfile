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
    @bque status

# 应急清理全局编译锁状态
queue-clean:
    @bque clean

# 手动暂停后台编译（让路游戏或交互压测）
queue-pause:
    @bque pause

# 手动恢复后台编译
queue-resume:
    @bque resume

# 构建
[windows]
build *ARGS:
    @bque run -- cargo build {{ ARGS }}

[unix]
build *ARGS:
    cargo build {{ ARGS }}

# 检查（clippy 全开）
[windows]
check *ARGS:
    @bque run -- cargo clippy --all-targets --all-features -- -D warnings {{ ARGS }}

[unix]
check *ARGS:
    cargo clippy --all-targets --all-features -- -D warnings {{ ARGS }}

# 测试
[windows]
test *ARGS:
    @bque run -- cargo test --all-features {{ ARGS }}

[unix]
test *ARGS:
    cargo test --all-features {{ ARGS }}

# 专项基准测试（bque bench 自动挂起后台编译以提供纯净 CPU 环境）
[windows]
bench *ARGS:
    @bque bench -- cargo bench {{ ARGS }}

[unix]
bench *ARGS:
    cargo bench {{ ARGS }}

# 格式化检查
fmt:
    cargo fmt --check

# 一键全检查（CI 用）
ci: fmt check test
