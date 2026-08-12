# 30-kernel-profile: 设计结构（Design Structure）

> **文档**: `30-kernel-profile.md`
> **状态**: v1 快照（2026-08-12）

---

## 知识点全集

### 1. 统计 profiling 机制
- 采样时钟（独立于调度时钟）
- 采样 handler（记录进程 + PC）
- 分类统计（idle / system / user）
- NMI profiling（不可屏蔽中断采样）

### 2. profile.c 7 个函数
1. `init_profile_clock(freq)`: 初始化采样时钟 + 注册 IRQ handler
2. `stop_profile_clock()`: 停止采样时钟 + 注销 IRQ handler
3. `sprof_save_sample(p, pc)`: 保存 endpoint + PC 到 buffer
4. `sprof_save_proc(p)`: 保存 endpoint + name 到 buffer
5. `profile_sample(p, pc)`: 主采样逻辑，分类 idle/system/user
6. `profile_clock_handler(hook)`: 时钟中断 handler
7. `nmi_sprofile_handler(frame)`: NMI 采样 handler

### 3. 全局状态
- `sprof_sample_buffer[SAMPLE_BUFFER_SIZE]`: 采样缓冲区
- `sprof_info`: 采样统计（mem_used / idle_samples / system_samples / user_samples / total_samples）
- `sprofiling`: 是否正在 profiling
- `sprof_mem_size`: buffer 总大小

### 4. 条件编译
- `#if SPROFILE`: 整个 profile.c
- SPROFILE 未启用时文件为空

### 5. 与 25/26 的关系
- 25: `do_sprofile` 系统调用（用户态接口）
- 26: `nmi_sprofile_handler`（NMI 路径）
- 30: `profile.c` 内核侧（采样收集）

### 6. 64-bit 重写决策
- D1: trait 接口保留（ClockArch::init_profile_clock）
- D2: 样本收集 DEFERRED
- D3: NMI WONTFIX
- D4: buffer DEFERRED
