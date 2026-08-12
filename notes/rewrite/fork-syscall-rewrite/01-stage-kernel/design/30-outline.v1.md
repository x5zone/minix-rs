# 30-kernel-profile: 大纲（Outline v1）

> **文档**: `30-kernel-profile.md`
> **状态**: v1 快照（2026-08-12）
> **教学目标**: 从"内核如何采样自身执行"这一问题出发，建立统计 profiling 的架构角色心智模型

---

## Ch1: 概念（统计 profiling）

### 教学目标
- **核心问题**: 内核运行时，CPU 时间花在哪些进程上？哪些代码路径是热点？用户态 profiling 工具无法观察内核态执行。内核如何采样自身执行，为性能优化提供数据？
- **CPU/OS perspective question**: "内核如何采样自身执行？"——这是内核自省的元问题。

### 1.1 统计 profiling 的机制
- **采样时钟**: 独立于调度时钟的专用时钟，周期性触发中断
- **采样 handler**: 中断时记录当前进程 + PC，存入 buffer
- **分类统计**: 区分 idle / system / user 样本
- **NMI profiling**: 用不可屏蔽中断采样，即使内核关中断也能触发

### 1.2 SPROFILE 条件编译
- 整个 `profile.c` 用 `#if SPROFILE` 包裹
- `SPROFILE` 未启用时，文件为空

### 1.3 与 25-misc-unported.md 的关系
- 25 文档化 `do_sprofile` 系统调用（用户态接口）
- 30 文档化 `profile.c` 内核侧（采样收集）
- 两者是"接口/实现"配对

### 1.4 redox 对照
- **redox**: 无内核 profiling——外部工具（perf）
- **Minix3**: 内核内 profiling + SPROFILE 条件编译
- **minix-rs**: trait 接口保留 + 样本收集 DEFERRED

### 1.5 本章不讲什么
- `do_sprofile` 系统调用（见 25-misc-unported.md）
- NMI watchdog（见 26-watchdog.md）
- 调度时钟机制（见 15-clock-timer.md）

---

## Ch2: C 源码分析

### 2.1 文件清单
| 文件 | 行数 | 核心内容 |
|------|------|---------|
| `profile.c` | 157 | 7 函数 + 1 全局数组，全部 `#if SPROFILE` |

### 2.2 7 个函数详解
- `init_profile_clock`: 初始化采样时钟
- `stop_profile_clock`: 停止采样时钟
- `sprof_save_sample`: 保存采样到 buffer
- `sprof_save_proc`: 保存进程信息到 buffer
- `profile_sample`: 主采样逻辑
- `profile_clock_handler`: 时钟中断 handler
- `nmi_sprofile_handler`: NMI 采样 handler

---

## Ch3: 设计决策

- D1: profile 时钟接口保留（trait）
- D2: 样本收集 DEFERRED
- D3: NMI profiling WONTFIX
- D4: sprof_sample_buffer DEFERRED

---

## Ch4: Rust 实现

### 4.1 已实现
- `ClockArch::init_profile_clock` trait 方法
- `X86_64ClockArch::stop_profile_clock` impl

### 4.2 DEFERRED
- `sprof_save_sample` / `sprof_save_proc` / `profile_sample` / `profile_clock_handler`

### 4.3 WONTFIX
- `nmi_sprofile_handler`（见 26-watchdog.md）

---

## Ch5: 测试

不需要测试（DEFERRED + WONTFIX）。

---

## Ch6: 跨文档引用
- [15-clock-timer.md](15-clock-timer.md): ClockArch trait
- [25-misc-unported.md](25-misc-unported.md): do_sprofile 系统调用
- [26-watchdog.md](26-watchdog.md): NMI profiling
