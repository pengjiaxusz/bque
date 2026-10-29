# justfile — 常用任务
# 跨平台 shell：unix 用 sh，Windows 用 PowerShell，不强制 bash
[unix]
set shell := ["sh", "-uc"]
[windows]
set shell := ["powershell", "-NoProfile", "-Command"]

default:
    @just --list

# 构建
build:
    cargo build

# 检查（clippy 全开）
check:
    cargo clippy --all-targets --all-features -- -D warnings

# 测试
test:
    cargo test --all-features

# 格式化检查
fmt:
    cargo fmt --check

# 一键全检查（CI 用）
ci: fmt check test