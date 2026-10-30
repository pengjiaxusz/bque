# bque

[English](README.md) | [简体中文](README_zh.md)

[![CI](https://github.com/pengjiaxusz/bque/actions/workflows/ci.yml/badge.svg)](https://github.com/pengjiaxusz/bque/actions)
[![Release](https://img.shields.io/github/v/release/pengjiaxusz/bque)](https://github.com/pengjiaxusz/bque/releases/latest)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

**bque**（Build Queue & Resource Governor）是一个专为 Windows 平台打造的高性能、无守护进程（Daemonless）系统级工具。用于协调多项目/多工作树间的繁重编译与后台任务排队，通过 Windows Job Object 实现动态 CPU 配额管控，并为基准测试（跑分/Profiling）提供完全纯净且不受干扰的 CPU 运行环境。

---

## 为什么需要 bque？

### 1. 多项目并行争抢问题
在同时维护多个代码仓库、Git 工作树（Worktrees）或跨语言项目（如 C++ CMake/Ninja、Rust Cargo、Go、Node.js 等）时，后台编译与测试容易相互踩踏：
- 多个编译同时触发导致 CPU 瞬间打满 100%，磁盘与内存带宽被挤爆；
- 前台工具（IDE、浏览器、终端）发生丢帧、卡顿与输入延迟；
- 共享编译缓存或构建目录并发访问引发锁争用与依赖损坏。

### 2. 跑分与基准测试被干扰问题
进行精准的性能跑分（Benchmark）或 Profiling 时，必须拥有**绝对干净的 CPU 运行环境**：
- 仅将编译设为 `Idle` 优先级**远远不够**：Windows 调度器在空闲核心或超线程空隙中仍会调度 Idle 线程；
- 共享 L3 缓存被污染击穿、内存总线竞争会导致跑分结果波动 5%–25%；
- 多核高负荷编译会触发 CPU 功耗墙（PL1/PL2）降频与温度墙，压低单核最高睿频加速。

`bque` 通过引入**干净跑分锁（Clean-Room Benchmark Lock）**解决此痛点：
- `bque bench` 独占执行跑分时，自动调用内核级进程挂起接口（`NtSuspendProcess`），秒级**冻结（Suspend）当前正在运行的所有后台编译进程树**（CPU 占用瞬间归 0.0%）；
- 自动将跑分提升至 `High` 优先级，并支持绑定大核（Performance Cores）；
- 跑分结束时，自动通过 `NtResumeProcess` 无缝唤醒挂起的编译任务，**绝不丢失前序编译成果**。

---

## 核心特性

- **无守护进程架构（Daemonless）**：完全基于 Windows 内核原生对象（`Named Mutex`、`Job Object`、`Virtual Memory`）。无需任何常驻后台服务或守护进程；若进程崩溃或被强杀，内核通过 `AbandonedMutex` 自动安全移交锁。
- **微秒级极致轻量（<2ms 启动）**：纯 Rust 编写，零重型运行时，执行命令开销几乎无法感知。
- **进程优先级自动降级**：默认以 `Idle` 优先级启动任务，编译子孙进程树（`cl.exe`、`clang`、`rustc`、`ninja` 等）全量自动继承，前台操作永远丝滑。
- **Job Object 动态 CPU 软上限（Soft-Cap）**：支持灵活设定 CPU 配额（如 35%、50%、75%、100%）。软上限语义保证：系统空闲时允许满载利用全部 CPU，只有在前台应用需要资源或发生争抢时才精准限流。
- **纯净跑分模式（`bque bench`）**：
  - 自动获取跑分独占排他锁；
  - 秒级挂起冻结后台所有运行中的编译进程树；
  - 跑分提速至 `High` 优先级并可指定核心亲和性（`p-cores` / `e-cores`）；
  - 跑分退出后瞬间原位恢复所有编译任务。
- **动态争抢调速器（Dynamic Contention Governor）**：后台独立线程每 5s 周期采样系统整体 CPU 争抢情况。当检测到外部系统负载过高时自适应调节编译配额，空闲时自动放开。
- **全局状态自省**：通过 `bque status` 实时查看当前持锁者、排队任务队列、观察者及挂起状态。

---

## 安装方式

### 一键快速安装（推荐）
在 PowerShell 中运行以下命令。**无需管理员权限**，自动下载最新发布版本、安装至 `$env:LOCALAPPDATA\bque\bin` 并自动配置 PATH，当前终端即刻生效：
```powershell
irm https://github.com/pengjiaxusz/bque/releases/latest/download/install.ps1 | iex
```

### 预编译二进制下载
直接前往 [GitHub Releases](https://github.com/pengjiaxusz/bque/releases/latest) 下载最新的 `bque-windows-x86_64.zip` 或 `bque.exe`，并将其所在目录（推荐 `$env:LOCALAPPDATA\bque\bin`）加入系统环境变量 `PATH` 即可。

### 源码编译安装
```bash
git clone https://github.com/pengjiaxusz/bque.git
cd bque
cargo install --path .
```

确保 `%USERPROFILE%\.cargo\bin` 已加入系统环境变量 `PATH`。

---

## 使用指南

### 1. 编译与任务排队执行（`bque run`）
在任意命令前添加 `bque run --` 即可接入全局调度网络：

```powershell
# 基础编译（全局互斥排队 + 默认 Idle 优先级）
bque run -- cargo build --release

# 附带自定义任务描述（在队列状态中直观展示）
bque run --desc "编译 Web 前端" -- cmake --build build

# 限制 CPU 软上限为 50%
bque run --cpu-rate 50 -- cargo test

# 观察者模式（仅在状态中登记并限流，不阻塞编译队列排队）
bque run --observer --desc "代码检查" -- cargo clippy
```

### 2. 纯净跑分保护（`bque bench`）
在完全隔离的干净环境中运行基准性能测试：

```powershell
# 自动挂起后台所有编译任务，提权至 High 优先级运行跑分，退出后自动恢复
bque bench -- python run_benchmarks.py

# 严格绑定至高性能 P 核执行跑分
bque bench --cores p-cores -- cargo bench
```

### 3. 全局队列状态查看（`bque status`）
查看跨项目的所有活动构建与排队状态：

```powershell
bque status
```

输出示例：
```text
============================================================
              bque Global Build Queue & Governor            
============================================================

[1] Active Lock Holder:
  PID:         12450
  Description: Compile Engine
  Command:     cmake --build build --config Release
  Working Dir: C:\Projects\game-engine
  Priority:    Idle
  CPU Rate:    65% (soft-cap)
  Running For: 18s

[2] Queued Waiters (1):
  #1  PID 28312  [   6s] Unit tests ('C:\Projects\web-service')

[3] Observers (0):
  (No observers active)
============================================================
```

如需供自动化脚本解析，可使用 `bque status --json` 输出结构化 JSON。

### 4. 手动暂停与恢复（`bque pause` / `bque resume`）
在进行大型游戏或高负荷实时任务时，一键挂起所有后台构建：

```powershell
bque pause --reason "打游戏免打扰"
# 完成后恢复：
bque resume
```

### 5. 清理异常残留（`bque clean`）
若终端被强制关闭或异常终止，安全清理孤立的排队记录与临时状态：

```powershell
bque clean
```

---

## 项目接入示例

### 在 `justfile` 中使用
```just
# 将日常繁重命令接入 bque
build:
    bque run -- cargo build --all-targets

test:
    bque run -- cargo test

bench:
    bque bench -- cargo bench
```

### 在 PowerShell 脚本中接入与引导提示
```powershell
# 优先使用 bque，未安装时给出指引
if (Get-Command bque -ErrorAction SilentlyContinue) {
    & bque run --desc "服务编译" -- cmake --build build
} else {
    Write-Warning "未检测到 bque，建议安装以实现跨项目编译排队与 CPU 调度保护。"
    Write-Warning "下载地址: https://github.com/pengjiaxusz/bque/releases/latest"
    cmake --build build
}
```

---

## 架构简图

```text
┌────────────────────────────────────────────────────────┐
│                        CLI 命令行                      │
│       bque run  │  bque bench  │  bque status          │
└──────────────────────────┬─────────────────────────────┘
                           │
             ┌─────────────┴─────────────┐
             ▼                           ▼
┌─────────────────────────┐ ┌─────────────────────────┐
│       排队与互斥        │ │        纯净跑分         │
│ - Local\Bque_Build_Lock │ │ - Local\Bque_Bench_Lock │
│ - 崩溃自动恢复(Abandoned)│ │ - NtSuspendProcess 挂起 │
│ - 排队等待者注册与唤醒  │ │ - NtResumeProcess 恢复  │
└────────────┬────────────┘ └────────────┬────────────┘
             │                           │
             └─────────────┬─────────────┘
                           ▼
┌────────────────────────────────────────────────────────┐
│                      资源调度层                        │
│ - SetPriorityClass（Idle / BelowNormal / Normal / High）│
│ - Job Object（JOBOBJECT_CPU_RATE_CONTROL_INFORMATION） │
│ - 动态系统争抢采样调速器（5s 周期自适应调整）           │
│ - CPU 拓扑识别与绑核亲和性（P-Core / E-Core）           │
└────────────────────────────────────────────────────────┘
```

---

## 开源协议

本项目采用双重开源协议授权：
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) 或 http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) 或 http://opensource.org/licenses/MIT)

可按需选择任一协议使用。
