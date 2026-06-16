# Minix3 kernel/system → Rust 实现覆盖检查表

> 生成日期: 2026-06-13
> 目标: 100% 覆盖 Minix3 kernel (`minix3/minix/kernel/*.c` + `system/do_*.c`) 所有公开符号
> 范围: 41 个 `do_*.c` (实际 38 个 — `do_datacopy.c`/`do_sdevio.c`/`do_unused.c` 不存在; `do_schedule.c` 是内核内调用) + 20 个核心 .c/.h
> 验证依据: `minix3/minix/kernel/` (C 源) ↔ `os/kernel/src/*.rs` (Rust 实现)
> 注: 本表只覆盖已审阅模块 — 后续 04-stage-vfs/05-stage-sched 等阶段将扩展

## 0. 覆盖度总览

| 分类 | 总数 | 已实现 | Partial | Stub | 未实现 | 覆盖率 |
|------|------|--------|---------|------|--------|--------|
| 宏 (#define) | ~50 | ~25 | ~5 | 0 | ~20 | 60% |
| 全局变量 | ~30 | ~18 | ~5 | 0 | ~7 | 77% |
| 结构体 (struct) | ~15 | 10 | 3 | 0 | 2 | 87% |
| 函数 (C 公开) | ~80 | ~30 | ~15 | ~25 | ~10 | 56% |
| `do_*` syscall 处理器 | 38 | 1 | 20 | 13 | 4 | 3% 完全实现 (SYS_SCHEDCTL 2026-06-17) |
| Syscall 编号映射 (call_nr.h) | 47 | 1 | 16 | 19 | 11 | 38% 实际可工作 |
| **总体** | **~260** | **~89** | **~55** | **~52** | **~64** | **~56%** |

> **重要**: "未实现" ≠ "bug" — 大量未实现项是**有意省略** (e.g. 设计差异、阶段性未完成、arch-stub), 详见每项 Reason 列.

---

## 1. 宏 (#define) 覆盖

### 1.1 `const.h` — Kernel-wide constants

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| K-001 | `INIT_PROC_NR` | const.h:21 | init 进程 slot 编号 | `Endpoint::INIT_PROC_NR` in minix-types | ✅ |
| K-002 | `NR_PROCS` | const.h:23 | 进程表最大容量 | `ProcessTable::NR_PROCS` (proc_table.rs) | ✅ |
| K-003 | `NR_TASKS` | const.h:25 | 内核任务数 (5) | `minix_types::NR_TASKS` (统一到 minix-types, 2026-06-15) | ✅ |
| K-004 | `NR_BOOT_MODULES` | const.h:27 | boot modules 上限 | `NR_BOOT_MODULES` in minix-types | ✅ |
| K-005 | `KERN_STACK_SIZE` | archconst.h | 内核栈大小 | `KERNEL_STACK_SIZE` (boot_alloc.rs) | ✅ |
| K-006 | `KERN_VIRT_BASE` | archconst.h | 内核虚拟基址 | `KERN_VIRT_BASE` (per-arch) | ✅ |
| K-007 | `INIT_TASK_PSW` | archconst.h | 任务初始 PSW | `INIT_TASK_PSW` (per-arch proc_arch.rs) | ✅ |
| K-008 | `DEFAULT_HZ` | archconst.h | 时钟频率 | `DEFAULT_HZ` (clock.rs, x86=60, ARM=1000) | ✅ |
| K-008a | `tick(is_bsp, …)` | clock.c | per-tick handler | `clock.rs::tick::<BspTick\|ApTick>` (compile-time marker) + `tick_bsp`/`tick_ap` wrappers | ✅ P1-25 已修复: 运行时 `if is_bsp` 分支消除，Doc 14 §3 D8 + §4.3 已升级 |
| K-009 | `config_no_smp` | config.h | SMP 开关 | `#[cfg(CONFIG_SMP)]` | ✅ |
| K-010 | `config_no_apic` | config.h | APIC 开关 | `minix_plat::InterruptController` trait | ✅ 设计差异 |

### 1.2 `ipc.h` — IPC masks

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| K-011 | `NON_BLOCKING` | ipc.h | 非阻塞标志 | `IpcFlags::NON_BLOCKING` (ipc.rs) | ✅ |
| K-012 | `WILLRECEIVE` | ipc.h | 接收者就绪位 | `IpcState::WillReceive` (ipc.rs) | ✅ |
| K-013 | `CANRECEIVE` | ipc.h | 可接收位 | `IpcState::CanReceive` (ipc.rs) | ✅ |
| K-014 | `FROM_KERNEL` | ipc.h | 内核消息源 | `Sender::Kernel` | ✅ |
| K-015 | `IPC_STATUS_*` | ipc.h | IPC 状态位 | `IpcStatus` bitset | ⚠️ Partial (used in detect_deadlock only) |

### 1.3 `priv.h` — Privilege flags

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| K-016 | `PREEMPTIBLE` | priv.h:42 | 0x002, 进程可被抢占 | `PrivFlagsBits::PREEMPTIBLE` (kpriv.rs:50) | ✅ |
| K-017 | `DYN_PRIV_ID` | priv.h:43 | 0x008, 动态 priv id | `PrivFlagsBits::DYN_PRIV_ID` (kpriv.rs:51) | ✅ Doc 21 §1.2 已修复, 与 C `priv.h:43` 一致 |
| K-018 | `SYS_PROC` | priv.h:44 | 0x010, 系统进程 | `PrivFlagsBits::SYS_PROC` (kpriv.rs:52) | ✅ |
| K-019 | `CHECK_IO_PORT` | priv.h:45 | 0x020, I/O 端口检查 | `PrivFlagsBits::CHECK_IO_PORT` (kpriv.rs:53) | ✅ |
| K-020 | `CHECK_IRQ` | priv.h:46 | 0x040, IRQ 检查 | `PrivFlagsBits::CHECK_IRQ` (kpriv.rs:54) | ✅ |
| K-021 | `CHECK_MEM` | priv.h:47 | 0x080, 内存访问检查 | `PrivFlagsBits::CHECK_MEM` (kpriv.rs:55) | ✅ |
| K-022 | `ROOT_SYS_PROC` | priv.h:48 | 0x100, 根系统进程 | `PrivFlagsBits::ROOT_SYS_PROC` (kpriv.rs:56) | ✅ |
| K-023 | `VM_SYS_PROC` | priv.h:49 | 0x200, VM 系统进程 | `PrivFlagsBits::VM_SYS_PROC` (kpriv.rs:57) | ✅ |
| K-024 | `LU_SYS_PROC` | priv.h:50 | 0x400, Live Update 进程 | `PrivFlagsBits::LU_SYS_PROC` (kpriv.rs:58) | ✅ |
| K-025 | `RST_SYS_PROC` | priv.h:51 | 0x800, RS 系统进程 | `PrivFlagsBits::RST_SYS_PROC` (kpriv.rs:59) | ✅ |
| K-026 | ~~`CHECK_IPC`~~ | — | — | **不存在** | ✅ 已确认: C 源码无此宏, Doc 21/22 已删除虚构引用, IPC 过滤通过 s_ipc_to 无条件执行 |
| K-027 | `PMAGIC` | proc.h | 进程表 magic 0xC0FFEE1 | `PMAGIC = 0x00C0_FFEE1` | ✅ |

### 1.4 `proc.h` — RTS / MF flags

| # | C 宏 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------|---------|------|-----------|------|
| K-028 | `RTS_SLOT_FREE` | proc.h | slot 空闲 | `RtsFlags::SLOT_FREE` (proc.rs:200) | ✅ |
| K-029 | `RTS_PROC_STOP` | proc.h | 进程停止 | `RtsFlags::PROC_STOP` (proc.rs:201) | ✅ |
| K-030 | `RTS_SENDING` | proc.h | 进程发送中 | `RtsFlags::SENDING` (proc.rs:202) | ✅ |
| K-031 | `RTS_RECEIVING` | proc.h | 进程接收中 | `RtsFlags::RECEIVING` (proc.rs:203) | ✅ |
| K-032 | `RTS_NO_QUANTUM` | proc.h | 时间片耗尽 | `RtsFlags::NO_QUANTUM` (proc.rs:204) | ✅ |
| K-033 | `RTS_SIGNALED` | proc.h | 收到信号 | `RtsFlags::SIGNALED` (proc.rs:205) | ✅ |
| K-034 | `RTS_SIG_PENDING` | proc.h | 信号待处理 | `RtsFlags::SIG_PENDING` (proc.rs:206) | ✅ |
| K-035 | `RTS_P_STOP` | proc.h | ptraced 停止 | `RtsFlags::P_STOP` | ⚠️ Partial |
| K-036 | `MF_DELIVERMSG` | proc.h | 投递消息 | `MiscFlags::DELIVERMSG` (proc.rs) | ✅ |
| K-037 | `MF_VIRT_TIMER` | proc.h | 虚拟定时器 | `MiscFlags::VIRT_TIMER` | ⚠️ Partial |
| K-038 | `MF_PROF_TIMER` | proc.h | profile 定时器 | `MiscFlags::PROF_TIMER` | ⚠️ Partial |
| K-039 | `MF_FPU_IN_USE` | proc.h | FPU 使用中 | `MiscFlags::FPU_IN_USE` | ⚠️ Partial |
| K-040 | `MF_KCALL_RESUME` | proc.h | kcall resume 待处理 | `MiscFlags::KCALL_RESUME` | ✅ |

### 1.5 未实现的宏 (设计差异)

| # | C 宏 | 描述 | 理由 |
|---|------|------|------|
| K-041 | `SANITYCHECKS` / `CACHE_SANITY` / `VMSTATS` | 调试统计开关 | `#[cfg(feature = "...")]` 替代, 已实现 |
| K-042 | `JUNKFREE` | 释放填充垃圾 | Rust Drop 自动 |
| K-043 | `IPCF_POOL_INIT` | IPC filter pool 初始化 | ✅ (2026-06-14) `IpcFilterPool::new()` + 全局 `IPC_FILTER_POOL` + kmain Phase C.5; `Option<IpcFilterSlot>` 替代 C 的 `type==IPCF_NONE` 哨兵 |
| K-044 | `PRINTK` / `printf` 系列 | 内核打印 | `log::info!()` / `minix_kernel_log!()` |
| K-045 | `VERBOSEBOOT` | 详细启动日志 | `boot_verbose: AtomicBool` in minix-types |

## 2. 全局变量覆盖 (`glo.h`)

| # | C 全局变量 | 文件:行 | 描述 | Rust 实现 | 状态 |
|---|------------|---------|------|-----------|------|
| G-001 | `proc[]` | glo.h:30 | 进程表 | `ProcessTable::proc[]` (proc_table.rs:30-33) | ✅ |
| G-002 | `kinfo` | glo.h:18 | kernel info | `KernelInfo` (minix-types) | ✅ |
| G-003 | `machine` | glo.h:19 | 机器信息 | `minix_plat::MachineInfo` | ✅ |
| G-004 | `kmessages` | glo.h:20 | kernel 消息缓冲 | `KMessages` (minix-types) | ✅ |
| G-005 | `loadinfo` | glo.h:21 | 加载信息 | `KLoadInfo` | ⚠️ Partial (todo §6 lists TODOs) |
| G-006 | `kuserinfo` | glo.h:22 | user info | not in current code | ❌ |
| G-007 | `vmrequest` | glo.h:23 | VM 请求结构 | `VmRequestHandler` (vm.rs) | ✅ |
| G-008 | `irq_hooks[]` | glo.h:24 | IRQ 钩子表 | `IrqManager::hooks[]` (irq_manager.rs:60) | ✅ |
| G-009 | `irq_actids[]` | glo.h:25 | IRQ 活动 ID | `IrqManager::actids[]` | ✅ |
| G-010 | `irq_use` | glo.h:26 | IRQ 使用位图 | `IrqManager::irq_use` | ⚠️ Partial (plain u64, not atomic — 见 SMP audit) |
| G-011 | `ncpus` | glo.h:27 | CPU 数量 | `SmpState::ncpus` (smp.rs) | ✅ |
| G-012 | `bsp_cpu_id` | glo.h:28 | BSP CPU id | `SmpState::bsp_cpu_id` (smp.rs) | ✅ |
| G-013 | `cpus[]` | glo.h:29 | CPU 数组 | `SmpState::cpus[]` (smp.rs) | ✅ |
| G-014 | `sched_ipi_data[]` | glo.h:30 | 调度 IPI 数据 | `SmpState::sched_ipi_data[]` (smp.rs) | ⚠️ Partial |
| G-015 | `ap_cpus_booted` | glo.h:31 | AP 启动计数 | `SmpState::ap_cpus_booted` (AtomicU32) | ✅ |
| G-016 | `cpu_info[]` | glo.h:32 | 每 CPU 信息 | `CpuLocal` (smp.rs:77-145) | ✅ Partial→Partial+ (2026-06-16): `CpuLocal` 新增 `scheduler: Scheduler` 字段 (对应 C cpulocals.h:58-59 `run_q_head/run_q_tail`); `ProcessTable` 新增 `sched_for_cpu()`/`sched_for_cpu_mut()` 方法; 单 CPU 时 `sched_for_cpu(0)` 返回 BSP scheduler, SMP 迁移时切换到 `CpuLocal::scheduler` |
| G-017 | `kernel_ticks[]` | glo.h:33 | 每 CPU tick | `ClockState::ticks_per_cpu` (clock.rs) | ⚠️ Partial |
| G-018 | `bkl_ticks[]` / `bkl_tries[]` / `bkl_succ[]` | glo.h:34-36 | BKL 统计 | **不存在** | ⚠️ 设计差异 — BKL 已实现 (P0-01 部分修复), 统计计数器暂不实现 (非阻塞项, 可后续添加) |
| G-019 | `vm_running` | glo.h:37 | VM 运行标志 | `lib::vm_running()` (atomic) | ✅ 已实现: `lib.rs::VM_RUNNING: AtomicBool` 全局镜像 + `bsp_finish_booting` 步骤 1 写入 false。SMP 时迁入 `SmpState.cpu_locals[cpu].vm_running` — see Doc 07 §4.6 / Doc 15 §2.2 (P0-08 resolved) |
| G-020 | `catch_pagefaults` | glo.h:38 | catch pagefaults flag | `MiscFlags::CATCH_PAGEFAULTS` | ✅ |
| G-021 | `kernel_may_alloc` | glo.h:39 | 动态分配许可 | `boot_alloc::may_alloc()` | ✅ |
| G-022 | `image[]` | table.c | boot image 表 | `BootImage` (minix-types) | ⚠️ Partial — todo.md §1 boot module lifecycle missing |
| G-023 | `verboseboot` | glo.h | 详细启动 | `KernelInfo::verbose` | ✅ |
| G-024 | `ipc_call_names[]` | glo.h | IPC 调用名表 | not implemented | ❌ 调试用, 优先级低 |
| G-025 | `lost_ticks` | glo.h | 丢失 tick 数 | `ClockState::lost_ticks` | ⚠️ Partial |
| G-026 | `krandom` | glo.h | kernel 随机源 | not implemented | ❌ (无硬件 RNG) |
| G-027 | `arm_frclock` | glo.h | ARM 自由时钟 | arch-specific | ✅ (aarch64 only) |
| G-028 | `kclockinfo` | glo.h | 时钟信息 | `KClockInfo` (minix-types) | ✅ |
| G-029 | `minix_kerninfo` | glo.h | kernel info 结构 | `KernelInfo` | ✅ |
| G-030 | `serial_debug_active` | glo.h | 串口调试 | `boot_alloc::serial_debug` | ✅ |

## 3. 结构体覆盖

### 3.1 `struct proc` (proc.h)

| # | C 字段 | 描述 | Rust 字段 | 状态 |
|---|--------|------|-----------|------|
| S-001 | `p_reg` | 保存寄存器 | `KProcess::regs` (proc.rs) | ✅ |
| S-002 | `p_seg` | 段描述符 | per-arch | ✅ (x86_64 only) |
| S-003 | `p_nr` | 进程 slot | `KProcess::pid` | ✅ |
| S-004 | `p_priv` | 权限指针 | `KProcess::priv_id: Option<PrivId>` (索引式查找, 非 C 指针) | ✅ |
| S-005 | `p_rts_flags` | 运行时状态 (volatile u32) | `KProcess::rts_flags: RtsFlags(AtomicU32)` | ✅ (atomic) |
| S-006 | `p_misc_flags` | misc 标志 | `KProcess::misc_flags: MiscFlags(AtomicU32)` | ✅ (atomic) |
| S-007 | `p_priority` | 当前优先级 | `KProcess::priority` | ✅ |
| S-008 | `p_cpu_time_left` | 时间片剩余 | `KProcess::cpu_time_left: u64` | ✅ | ✅ P1-17 已修复: ms_to_cpu_time() 已连接 clock::ms_to_cpu_time() (TSC_PER_MS 全局原子 + 1GHz fallback) + **read_tsc() 已实现 (2026-06-16)**: `ClockArch::read_tsc()` trait 方法 (默认委托 read_ticks()) + kernel `clock::read_tsc()` 调用 `minix_arch::CurrentClockArch::read_tsc()`; x86_64=rdtsc, aarch64=CNTPCT_EL0, riscv64=mtime; 2 个测试覆盖 |
| S-009 | `p_quantum_size_ms` | 时间片大小 | `KProcess::quantum_size_ms` | ✅ |
| S-010 | `p_scheduler` | 用户空间调度 | `KProcess::scheduler_endpoint` | ✅ |
| S-011 | `p_cpu` | 分配的 CPU | `KProcess::assigned_cpu` | ✅ |
| S-012 | `p_cpu_mask[]` | CPU 亲和位图 | `KProcess::cpu_mask: u64` | ✅ (assumes NR_CPUS ≤ 64) |
| S-013 | `p_stale_tlb[]` | TLB staleness | not in Rust | ❌ 设计差异 (aarch64/riscv64 ASID handles it) |
| S-014 | `p_accounting` | 调度统计 | `KProcess::accounting: ProcAccounting` | ✅ |
| S-015 | `p_dequeued` | 上次 dequeue 时间 | `KProcess::dequeued: u64` | ⚠️ Partial→Partial+ | ✅ P1-17 已修复: get_monotonic() 已连接 clock::get_monotonic() (CLOCK_UPTIME 全局原子, BSP tick 镜像) |
| S-016 | `p_user_time` | 用户时间 | `KProcess::user_time: u64` | ⚠️ Partial |
| S-017 | `p_sys_time` | 系统时间 | `KProcess::sys_time: u64` | ⚠️ Partial |
| S-018 | `p_virt_left` | 虚拟定时器剩余 | `KProcess::virt_left: u64` | ⚠️ Partial |
| S-019 | `p_prof_left` | profile 定时器剩余 | `KProcess::prof_left: u64` | ⚠️ Partial |
| S-020 | `p_cycles` | 总周期 | `KProcess::cycles: u64` | ⚠️ Partial |
| S-021 | `p_kcall_cycles` | kcall 周期 | `KProcess::kcall_cycles: u64` | ⚠️ Partial |
| S-022 | `p_kipc_cycles` | IPC 周期 | `KProcess::kipc_cycles: u64` | ⚠️ Partial |
| S-023 | `p_tick_cycles` | 累计 tick 周期 | `KProcess::tick_cycles: u64` | ⚠️ Partial |
| S-024 | `p_cpuavg` | ps(1) 负载均值 | not in Rust | ❌ (clock.rs load_info partial) |
| S-025 | `p_nextready` | run queue 链接 | `KProcess::p_nextready: AtomicI32` | ✅ (P1-24 2026-06-14: Option→AtomicI32, NONE_PROC_NR=-1) |
| S-026 | `p_caller_q` | sender 等待队列 | `KProcess::p_caller_q: AtomicI32` | ✅ (P1-24 2026-06-14: Option→AtomicI32, NONE_PROC_NR=-1) |
| S-027 | `p_q_link` | caller_q 链接 | `KProcess::p_q_link: AtomicI32` | ✅ (P1-24 2026-06-14: Option→AtomicI32, NONE_PROC_NR=-1) |
| S-028 | `p_getfrom_e` | 接收源 endpoint | `KProcess::getfrom: Endpoint` | ✅ |
| S-029 | `p_sendto_e` | 发送目的 endpoint | `KProcess::sendto: Endpoint` | ✅ |
| S-030 | `p_pending` | pending 信号集 | `KProcess::pending_sigs: SigSet` | ✅ |
| S-031 | `p_name[PROC_NAME_LEN]` | 进程名 | `KProcess::name: heapless::String<N>` | ✅ |
| S-032 | `p_endpoint` | generation-aware ep | `KProcess::endpoint: Endpoint` | ✅ |
| S-033 | `p_sendmsg` | SENDING 消息 | `KProcess::sendmsg: Message` | ⚠️ Partial (IPC not impl, no copy logic) |
| S-034 | `p_delivermsg` | 待投递消息 | `KProcess::delivermsg: Message` | ⚠️ Partial |
| S-035 | `p_delivermsg_vir` | 投递虚拟地址 | `KProcess::delivermsg_vir: VirtAddr` | ⚠️ Partial |
| S-036 | `p_vmrequest` | VM-suspended 状态 | `KProcess::vm_request: VmRequest` | ✅ |
| S-037 | `p_found` | 一致性检查 | not in Rust | ❌ (DEBUG_TRACE only, C only) |
| S-038 | `p_magic` | PMAGIC magic | `KProcess::magic: u32 = PMAGIC` | ✅ |
| S-039 | `p_defer` | 延迟 syscall 参数 | not in Rust | ❌ 设计差异 (no deferred syscall path) |
| S-040 | `p_schedules` | DEBUG 计数器 | not in Rust | ❌ (debug only) |

### 3.2 其他核心结构体

| # | C 结构体 | 文件 | Rust 等价 | 状态 |
|---|----------|------|----------|------|
| S-041 | `struct priv` | priv.h | `KPriv` (kpriv.rs:106-138) | ✅ |
| S-042 | `struct cpu` | smp.c | `CpuState` (smp.rs) | ✅ |
| S-043 | `struct sched_ipi_data` | smp.h | `SchedIpiData` (smp.rs) | ✅ |
| S-044 | `struct irq_hook_t` | type.h | `IrqHook` (irq_manager.rs) | ✅ |
| S-045 | `struct cpulocals` | cpulocals.h | `CpuLocal` (smp.rs:77-145) | ✅ Partial→Partial+ (2026-06-16): 字段已对齐 Doc 15 §2.2: `proc_ptr`/`bill_ptr`/`idle_proc`/`ptproc`/`cpu_is_idle`/`idle_interrupted`/`tsc_ctr_switch`/`cpu_last_tsc`/`cpu_last_idle`/`pagefault_handled`/`fpu_presence`/`fpu_owner` + `scheduler: Scheduler` (对应 C cpulocals.h:58-59 `run_q_head/run_q_tail`). 单 CPU 时 `ProcessTable::sched` 仍为权威来源, `CpuLocal::scheduler` 已初始化待 SMP 迁移 |
| S-046 | `struct stackframe_s` | arch const | per-arch `SavedRegs` | ✅ |
| S-047 | `struct segframe` | arch const | per-arch | ✅ |
| S-048 | `struct cpuavg` | proc.h | not in Rust | ❌ (load avg partial) |
| S-049 | `struct kernel_info` | glo.h | `KernelInfo` (minix-types) | ✅ |
| S-050 | `struct boot_image` | table.c | `BootImage` (minix-types) | ⚠️ Partial (todo §1 lifecycle missing) |
| S-051 | `struct vm_request` | glo.h | `VmRequest` | ✅ |
| S-052 | `struct minix_kerninfo` | glo.h | `KernelInfo` (unified) | ✅ |

## 4. 函数覆盖 — 38 个 `do_*.c` 处理器

> **重要**: 41 → 38 — `do_datacopy.c`/`do_sdevio.c`/`do_unused.c` 不存在. `do_schedule.c` 是内核内调用而非用户 syscall, 见 Doc 24.

### 4.1 Doc 16 — 进程管理 (7 handlers)

| # | C Handler | C File:Line | Rust 实现 | 状态 | 备注 |
|---|-----------|-------------|-----------|------|------|
| F-01 | `do_fork` | do_fork.c:42-136 | `syscall_process.rs:114` `dispatch_fork` | ✅ Fixed | ✅ (2026-06-14) 完整实现: ProcessTable + PrivTable 参数 + Endpoint::fork_new_endpoint generation 递增 + KProcess::fork_from 进程拷贝 + `complete_fork_setup` (SYS_PROC downgrade: RTS_NO_PRIV + USER_PRIV_ID + VMINHIBIT + "*F" name suffix) + child_endpoint 返回值. 遗留: FPU save/restore (arch 层), sched_proc 调用 (Scheduler 集成), ProcessTable 需通过 kernel_call_dispatch 传递 |
| F-02 | `do_exec` | do_exec.c:30-58 | `syscall_process.rs:231` `dispatch_exec` | ⚠️ Partial→Partial+ | ✅ (2026-06-15) 修复语义漂移: 旧代码操作 caller, C 操作 endpt 指定的目标进程. 新增: 1) proc_table 参数 + endpoint_to_nr 解析目标; 2) 操作目标进程的 DELIVERMSG/RECEIVING/EXT_REG_INITIALIZED; 3) rts_unset(RECEIVING) 自动 enqueue. 测试: operates_on_target + invalid_endpoint 2 项. DEFERRED: name copy (data_copy_vmcheck), arch_proc_init (arch trait) |
| F-03 | `do_clear` | do_clear.c:24-78 | `syscall_process.rs:335` `dispatch_clear` | ⚠️ Partial→Partial++ | ✅ (2026-06-14) 核心清理; **✅ (2026-06-16) 修复 P0 语义漂移**: 旧代码操作 caller 而非 target (PM 调 SYS_CLEAR 会把自己标 SLOT_FREE!), 现在正确操作 target + endpoint 验证 + isemptyp 提前返回 + PrivTable 传入 + SYS_PROC privilege slot 释放 (s_proc_nr=None). DEFERRED: release_address_space (VM), IRQ hooks (IrqManager), clear_endpoint (IPC), s_alarm_timer (timer) |
| F-04 | `do_exit` | do_exit.c:20-23 | `syscall_process.rs:194` `dispatch_exit` | ✅ Fixed | ✅ P1-12 已修复 (2026-06-13): `cause_signal_abort(caller)` 设置 `p_pending[6]=SIGABRT` + `RTS_SIGNALED | RTS_SIG_PENDING`; 测试 `test_dispatch_exit_sets_sigabrt` 验证; 遗留: `mini_notify(sig_mgr)` 待 P0-02/P1-08 |
| F-05 | `do_runctl` | do_runctl.c:34-67 | `syscall_process.rs:362` `dispatch_runctl` | ✅ Fixed (2026-06-15) | ✅ 修复语义漂移: 旧代码操作 caller, C 操作 RC_ENDPT 指定的目标进程. 新增: 1) proc_table 参数 + endpoint_to_nr 解析目标; 2) iskerneln→EPERM 权限检查; 3) rts_set/rts_unset 操作目标进程 (自动 dequeue/enqueue); 4) RC_RESUME debug_assert (C assert); 5) RC_DELAY+MF_SIG_DELAY 操作目标而非 caller. 测试: stop/resume/invalid_action/kernel_eperm 4 项. DEFERRED: SMP cross-CPU (smp_schedule_stop_proc) |
| F-06 | `do_schedctl` | do_schedctl.c:11-37 | `syscall_process.rs:502` `dispatch_schedctl` | ✅ Fixed (2026-06-17) | ✅ **完整修复 (2026-06-17)**: 1) 新增 `MessLsysKrnSchedctl` 消息类型 (`minix-types/src/ipc/message.rs`) 精确匹配 C `mess_lsys_krn_schedctl` 布局 (flags/endpoint/priority/quantum/cpu + 36B padding = 56B); 2) 新增 `msg_schedctl()` helper 替代错误的 `m1/m2` union overlay (修复字段映射 bug: `m2.m2i1`/`m2.m2i2` 实际读 offset 0/4 的 `flags`/`endpoint`, 非 offset 12/16 的 `quantum`/`cpu`); 3) `dispatch_schedctl` 签名扩展加 `&mut ProcessTable`, 通过 `endpoint_to_nr` 解析**目标**进程 (非 caller, 对齐 C `p = proc_addr(proc_nr)`); 4) `SCHEDCTL_FLAG_KERNEL` 分支调用 `sched::sched_proc(target, priority, quantum, cpu, false)` (niced=false 对齐 C `FALSE` 字面量) + 错误传播 (`sched_proc_error_to_errno`); 5) 无 flag 分支设置 `target.p_sched.scheduler = Some(caller.p_nr)`; 6) 7 个测试覆盖 (invalid flags / invalid endpoint / kernel flag + sched_proc + clear scheduler / sched_proc error propagation / invalid quantum / no flag sets caller as scheduler on target / -1 sentinels preserve current values). 419 个 kernel 测试 + 59 个 minix-types 测试通过 |
| F-07 | `do_statectl` | do_statectl.c:19-43 | `syscall_process.rs:310` `dispatch_statectl` | ⚠️ Partial++ (2026-06-14) | ✅ (2026-06-14) 新增: 1) StatectlRequest 枚举值修正为 C 1-5 (原 0-4); 2) MessLsysKrnSysStatectl 消息类型; 3) SetStateTable (s_state_table+entries) + ClearIpcFilters (s_ipcf.take() + ipc_filter_pool().free()) + AddIpcBlFilter/AddIpcWlFilter (allocate + free-old + ENOMEM) 全部已实现; 4) DEFERRED: AddIpc* 的 elements 填充依赖 data_copy_vmcheck; **TODO ClearIpcRefs 4 步详细 DEFERRED 路径** (cancel_async / clear_ipc / unset_sys_bit / return OK) 替换原单行 TODO, 与 TODO (IpcEngine::senda) 配对落地 |

### 4.2 Doc 17 — 内存拷贝 (8 handlers)

| # | C Handler | C File:Line | Rust 实现 | 状态 | 备注 |
|---|-----------|-------------|-----------|------|------|
| F-08 | `do_copy` (vircopy/physcopy) | do_copy.c:30-91 | `syscall_copy.rs:270` `dispatch_copy` | ⚠️ Partial→Partial+ | SELF replacement + flags parse + 专用 `MessLsysKrnSysCopy` (✅ P1-06); **✅ (2026-06-16) 完整 isokendpt 验证**: 传入 `ProcessTable`, 用 `endpoint_to_nr()` 替代简单范围检查, 实现完整 C `isokendpt` 语义 (proc_nr 范围 + slot 占用 + generation 匹配); NONE 跳过验证 (与 C 一致); 4 个测试覆盖; 缺 Direct Map (Doc D11 承诺); DEFERRED: P1-05 consolidated index |
| F-09 | `do_safecopy_from` | do_safecopy.c:329-337 | `syscall_copy.rs:472` `dispatch_safecopy_from` | ⚠️ Partial (2026-06-16) | ✅ 已修复 (2026-06-16): 完整实现输入验证 + granter endpoint 检查 + grant_id 边界检查 — NONE endpoint 拒绝 (EFAULT) / granter endpoint_lookup (EINVAL) / grant_id < 0 拒绝 (EINVAL); 签名扩展加 proc_table. DEFERRED: verify_grant(CPF_READ) (VM-side grant table) + virtual_copy_vmcheck (Direct Map PTE walk) + CPF_TRY soft-fault 写回. 4 个测试覆盖 (NONE granter / 无效 granter / 负 grant_id / valid setup) |
| F-10 | `do_safecopy_to` | do_safecopy.c:319-327 | `syscall_copy.rs:514` `dispatch_safecopy_to` | ⚠️ Partial (2026-06-16) | ✅ 已修复 (2026-06-16): **重构** — 提取 `safecopy_common_impl(caller, msg, proc_table, access)` 共享函数, F-09 (CPF_READ) + F-10 (CPF_WRITE) 都委托给它; 与 F-09 共享完整输入验证 (NONE 检查 / endpoint_lookup / 负 grant_id); 签名扩展加 proc_table. 4 个测试覆盖. DEFERRED: verify_grant + virtual_copy_vmcheck |
| F-11 | `do_vsafecopy` | do_safecopy.c:339-393 | `syscall_copy.rs:592` `dispatch_vsafecopy` | ⚠️ Partial (2026-06-16) | ✅ 已修复 (2026-06-16): 完整实现输入验证 + 边界检查 — caller endpoint 检查 (C assert → EFAULT) / vec_size <= 0 拒绝 (EINVAL) / vec_size > MAX_VSCPVEC 拒绝 (EINVAL) / checked_mul 溢出检查 (EINVAL); 新增 `MAX_VSCPVEC = 32` 常量 + `VscpVec` struct (`#[repr(C)]`, 40 字节与 C 一致) + 6 个测试覆盖 (NONE caller / 零 vec_size / 负 vec_size / 溢出 / valid setup / layout match). DEFERRED: virtual_copy_vmcheck + per-element safecopy loop + v_from/v_to SELF 方向检查 |
| F-12 | `do_umap` | do_umap.c:25-38 | `syscall_copy.rs:278` | ⚠️ Partial | ✅ 字段提取正确 (`MessLsysKrnSysUmap`); MEM_GRANT check + delegation OK |
| F-13 | `do_umap_remote` | do_umap_remote.c:26-120 | `syscall_copy.rs:567` `dispatch_umap_remote` | ⚠️ Partial (2026-06-16) | ✅ 已修复 (2026-06-16): 完整实现输入验证 + grantee 检查 + segment dispatch — SELF 替换 / endpoint_to_nr / NONE/ANY/MEM_GRANT 检查 / LOCAL_VM_SEG+MEM_GRANT|LOCAL_VM_SEG+VIR_ADDR 路径 / 未知 segment → EINVAL. DEFERRED: vm_lookup (需 Direct Map PTE walk) + verify_grant (需 VM-side grant table 暴露) + vm_lookup_range contiguous check + 写回 dst_addr. 10 个测试覆盖 (SELF/有效/无效/None/Any/Grant 路径/未知 segment/bogus seg_index/UMAP security) |
| F-14 | `do_vumap` | do_vumap.c:30-131 | `syscall_copy.rs:844` `dispatch_vumap` | ⚠️ Partial (2026-06-16) | ✅ 已修复 (2026-06-16): 完整实现输入验证 + 边界检查 + access flag 转换 — caller endpoint 检查 (EFAULT) / vcount<=0 拒绝 (EINVAL) / pmax<=0 拒绝 (EINVAL) / vcount & pmax 超 MAPVEC_NR 自动 clamp / access switch 转换 (VUA_READ/WRITE/RW 未知值 → EINVAL) / source != SELF 时 endpoint_lookup (EINVAL); 新增 VUA_READ/VUA_WRITE 常量; 签名扩展加 proc_table; 8 个测试覆盖 (NONE caller / 零 vcount / 零 pmax / 未知 access / 无效 source / SELF 路径 / grant source 路径 / 溢出 clamp). DEFERRED: data_copy / verify_grant 循环 / vm_lookup_range / vm_check_range / data_copy_vmcheck / pcount 写回 |
| F-15 | `do_memset` | do_memset.c:19-27 + memory.c:526-577 | `syscall_copy.rs:745` `dispatch_memset` | ⚠️ Partial (2026-06-16) | ✅ 已修复 (2026-06-16): 完整实现输入验证 + process endpoint 检查 + pattern 截断 — caller NONE endpoint (EFAULT) / process endpoint_lookup (ESRCH) / process==NONE 物理地址路径 / pattern & 0xFF 折叠; 新增 ESRCH 常量; 签名扩展加 proc_table. DEFERRED: createpde + phys_memset (Direct Map PTE walk) + vm_suspend (VM round-trip P0-02). 4 个测试覆盖 (无效 process / 有效 process / 物理地址 / 高位 pattern 截断) |
| F-16 | `do_safememset` | do_safememset.c:20-57 | `syscall_copy.rs:711` `dispatch_safememset` | ⚠️ Partial (2026-06-16) | ✅ 已修复 (2026-06-16): 完整实现输入验证 + privilege/grant_table 检查 — NONE endpoint 拒绝 (EFAULT) / endpoint_lookup (EINVAL) / dst process 必须有 privilege 且 s_grant_table 非零 (否则 EINVAL); 签名扩展加 proc_table+priv_table. DEFERRED: verify_grant (CPF_WRITE) + vm_memset. 4 个测试覆盖 (NONE dst / 无效 dst / 无 grant_table / valid setup) |

**Bug 标记**:
- ~~F-08: `m1.m1p1` 同时用于 `bytes` 和 `addr`, 字段重复使用~~ — ✅ 已修复: 添加 `MessLsysKrnSysCopy` 等 7 个专用 kernel message overlay 类型到 `MessageUnion`, 替代错误的 `MessageM1` 映射。64-bit 上 `mess_lsys_krn_sys_copy` 布局 (i32+u64 交替) 与 `MessageM1` (3×i32+3×u64) 完全不同, 导致 m1i2→padding, m1p1→dst_endpt+pad 等错误映射。段常量同步修正: LOCAL_VM_SEG=0→0x1000, MEM_GRANT=1→3, VIR_ADDR=2→1 (C const.h:64-66)
- ~~P0-msg-size: MessageUnion payload 48 bytes ≠ C union 56 bytes~~ — ✅ 已修复 (2026-06-16): C `sizeof(message)=64` (m_source:4 + m_type:4 + union:56), Rust 旧值 `MESSAGE_SIZE=56` (union 仅 48). 修复: `MESSAGE_SIZE=64`, 新增 `MESSAGE_PAYLOAD_SIZE=56`, `MessageUnion::raw` 48→56, 所有 `MessageM1~M5` 添加 `_padding` 至 56 bytes, 所有 `Mess*` 专用类型 padding 扩展至 56 bytes, `MessageM3::m3ca1` 24→48 bytes. 同步修复 5 个使用处: `syscall_signal.rs`/`syscall_clock.rs`/`syscall_process.rs`/`misc.rs`/`syscall_copy.rs`

### 4.3 Doc 18 — 信号 (5 handlers)

| # | C Handler | C File:Line | Rust 实现 | 状态 | 备注 |
|---|-----------|-------------|-----------|------|------|
| F-17 | `do_kill` | do_kill.c:22-41 | `syscall_signal.rs:89` | ✅ Implemented | isokendpt + iskerneln + cause_signal (mini_notify deferred to P1-08) |
| F-18 | `do_getksig` | do_getksig.c:20-43 | `syscall_signal.rs:110` | ✅ Implemented | ✅ (2026-06-14) 进程表扫描 + s_sig_mgr 匹配 + 返回 endpoint+map + RTS_SIGNALED 清除 + p_pending 清空; 无匹配返回 NONE; MessSigcalls 消息类型替代 MessageM1; s_sig_mgr 查找已重构为 `ProcessTable::sig_mgr()` 便捷方法 (P1-23) |
| F-19 | `do_endksig` | do_endksig.c:19-41 | `syscall_signal.rs:132` | ✅ Implemented | ✅ (2026-06-14) P1-07 已修复: isokendpt + s_sig_mgr 校验 (EPERM) + SIG_PENDING 检查 (EINVAL) + 条件清除 SIG_PENDING; MessSigcalls 消息类型 |
| F-20 | `do_sigsend` | do_sigsend.c:25-166 | `syscall_signal.rs:432` | ⚠️ Partial | `SignalContext` trait + `SigMsg` struct + 端点验证; DEFERRED: data_copy_vmcheck + sigframe build + register modify |
| F-21 | `do_sigreturn` | do_sigreturn.c:20-98 | `syscall_signal.rs:480` | ⚠️ Partial | `SignalContext` trait + 端点验证; DEFERRED: data_copy + register restore |

### 4.4 Doc 19 — 设备 I/O (3 handlers)

| # | C Handler | C File:Line | Rust 实现 | 状态 | 备注 |
|---|-----------|-------------|-----------|------|------|
| F-22 | `do_irqctl` | do_irqctl.c:20-174 | `syscall_device.rs:174` | ✅ Done (2026-06-16) | SetPolicy (CHECK_IRQ+notify_id+IrqManager::irqctl_set_policy) + RmPolicy (owner check+remove_hook_by_slot) + Enable/Disable (owner check+enable/disable_irq_by_slot); generic_handler DEFERRED (需 mini_notify+中断框架) |
| F-23 | `do_devio` | do_devio.c:18-107 | `syscall_device.rs:325` | ✅ Done | `PortIo` trait + CHECK_IO_PORT + 对齐检查 + I/O 执行 + 结果写回; 修复 DIO 常量映射 (TYPEMASK/DIRMASK) |
| F-24 | `do_vdevio` | do_vdevio.c:24-165 | `syscall_device.rs:419` | ⚠️ Partial | 参数提取 + 类型/方向解析 + vec_size 校验 + **ENOSYS 返回 (2026-06-16: 语义漂移修复, OK→ENOSYS)**; dispatch_arch_vdevio x86_64 已接线; 批量 I/O 执行 DEFERRED (需 data_copy_vmcheck) |

### 4.5 Doc 20 — 时钟 (5 handlers)

| # | C Handler | C File:Line | Rust 实现 | 状态 | 备注 |
|---|-----------|-------------|-----------|------|------|
| F-25 | `do_times` | do_times.c:19-46 | `syscall_clock.rs:102` | ✅ Implemented | ✅ **(2026-06-15) 完整实现**: SELF 替换 + `ProcessTable::endpoint_to_nr` 校验 + `p_time.user_time/sys_time` 读取 + `get_monotonic/get_realtime/get_boottime` 填充 + `MessKrnLsysSysTimes` 回复写入。对齐 C `do_times.c:28-46` |
| F-26 | `do_setalarm` | do_setalarm.c:27-78 | `syscall_clock.rs:170` | ✅ Implemented | ✅ **(2026-06-15) 完整实现**: SYS_PROC 权限检查 (`caller_has_sys_proc_with_table` + 真实 PrivTable) + `s_alarm_timer` 读取/写入 + `time_left` 计算 + `ClockState::set_timer/reset_timer` + 绝对/相对时间转换 + `MessLsysKrnSysSetalarm` 回复。对齐 C `do_setalarm.c:35-78` |
| F-27 | `do_stime` | do_stime.c:15-18 | `syscall_clock.rs:314` | ✅ Implemented | ✅ **(2026-06-15) 完整实现**: 提取 `boot_time` + `ClockState::set_boottime()` 更新。对齐 C `do_stime.c:17` |
| F-28 | `do_settime` | do_settime.c:17-58 | `syscall_clock.rs:360` `dispatch_settime` | ✅ Implemented (2026-06-15) | ✅ 完整实现: clock_id==CLOCK_REALTIME 校验 + adjtime 模式 (set_adjtime_delta) + set-time 模式 (timediff 计算 + 负值保护 + set_realtime). 对齐 C `do_settime.c:18-57` |
| F-29 | `do_vtimer` | do_vtimer.c:23-65 | `syscall_clock.rs:412` | ✅ Implemented | ✅ **(2026-06-15) 完整实现**: SYS_PROC 权限检查 (`caller_has_sys_proc_with_table` + 真实 PrivTable) + `VtimerType` 枚举 (Virtual=1, Prof=2, 对齐 C com.h:420-421) + SELF 替换 + `ProcessTable::endpoint_to_nr` 校验 + `virt_left/prof_left` 读写 + `MiscFlagsBits` set/clear + 旧值返回。对齐 C `do_vtimer.c:21-103` |

### 4.6 Doc 21 — 特权 (2 handlers)

| # | C Handler | C File:Line | Rust 实现 | 状态 | 备注 |
|---|-----------|-------------|-----------|------|------|
| F-30 | `do_privctl` | do_privctl.c:53-371 | `syscall.rs:281` `dispatch_privctl` | ❌ Not ported | 返回 BadCall; Doc 21 未涵盖 privctl; 推迟到 IPC filter 完成 |
| F-31 | `do_setgrant` | do_setgrant.c:17-29 | `syscall.rs:436` `dispatch_setgrant` | ✅ Fixed | RTS_NO_PRIV检查 + priv_id验证 + grant table更新(s_grant_table/s_grant_entries/s_grant_endpoint) |

### 4.7 Doc 22 — IPC 过滤 (无 C handler; C 是 `ipc.h` 宏)

| # | C 元素 | C File:Line | Rust 实现 | 状态 | 备注 |
|---|--------|-------------|-----------|------|------|
| F-32 | `check_ipc_filter` | ipc_filter.h | `kcall_filter_check` (ipc_filter.rs:36) | ✅ | 已接入 `kernel_call_dispatch` (P0-19 ✅ fixed) |
| F-33 | `allow_ipc_filtered_msg` | ipc_filter.h | `may_send_to` | ✅ |
| F-34 | `check_k_call_mask` | ipc.h | `kcall_filter_check` (ipc_filter.rs:78) | ✅ | 已接入 `kernel_call_dispatch` (P0-03/P0-19 ✅ fixed) |

### 4.8 Doc 23 — 跨空间运行时 (无独立 C handler; libsys 宏)

| # | C 元素 | C File:Line | Rust 实现 | 状态 | 备注 |
|---|--------|-------------|-----------|------|------|
| F-35 | `sys_datacopy` (libsys 宏) | syslib.h:129-130 | `cross_space.rs:70` `dispatch_datacopy` | ⚠️ Dead code (已文档化) | 无 `Syscall::Datacopy` 变体; sys_datacopy 展开为 sys_vircopy, 已在 F-08 处理; data_copy_vmcheck 保留供 SIGSEND/GETINFO + **TODO 路径清理**: 1) `src_addr/dst_addr/bytes` 改为命名绑定 (消除 `let _` 丢弃); 2) 修复 `bytes = m1.m1p1` 而非 m1p3 (C do_copy.c:71 验证); 3) 新增 isokendpt 范围检查 → `EDEADSRCDST=29` (cross_space.rs:38); 4) 提取字段真正传入 `data_copy_vmcheck` (Direct Map body 仍 DEFERRED) |

### 4.9 Doc 24 — Misc/Unported (10 handlers)

| # | C Handler | C File:Line | Rust 实现 | 状态 | 备注 |
|---|-----------|-------------|-----------|------|------|
| F-36 | `do_getinfo` | do_getinfo.c:20-228 | `misc.rs:155` | ⚠️ Partial→Partial+ | GET_WHOAMI ✅; GET_KINFO ✅ (2026-06-14: user_sp/freepde_start/vir_kern_start 从 kernel_info() 获取真实值); GET_PROC/GET_PROC2 ✅ (2026-06-16: SELF替换+isokendpt endpoint验证, data_copy_vmcheck仍DEFERRED); 通配分支 ✅ (返回 ENOSYS); ProcTab/PrivTab/LoadInfo 仍 ENOSYS (需 data_copy_vmcheck) |
| F-37 | `do_trace` | do_trace.c:20-208 | `misc.rs` `dispatch_trace` | ⚠️ Partial (2026-06-16): 8/9 request 实现 | ✅ (2026-06-16) 完整实现 3 个 flag 操作 (T_STEP / T_CONT / T_KILL) + 新增 6 个 request 的对齐检查 (T_GETINS / T_GETDATA / T_SETINS / T_SETDATA / T_GETUSER / T_SETUSER) — 对齐失败 → EFAULT, 对齐 → ENOSYS (等待 virtual_copy_vmcheck / struct field access / arch abstraction). 13 个测试覆盖 (Step/Cont/Kill 副作用 + 6 个对齐 EFAULT + GetIns ENOSYS + 无效 request + 无效 endpoint + kernel 拒绝). DEFERRED: virtual_copy_vmcheck 主体 (P1-05); proc/priv 字段偏移读; arch-specific 段寄存器保护 (P0-15 arch-abstractions); T_STOP / T_DETACH / T_SYSCALL 枚举扩展 |
| F-38 | `do_update` | do_update.c:20-340 | `misc.rs:480` `dispatch_update` | ⚠️ Partial (2026-06-16): 7/12 步实现 | ✅ (2026-06-16) 扩展输入验证 — 增加 `proc_is_updatable` 检查 (RTS_NO_PRIV \|\| RTS_SIG_PENDING \|\| (RTS_RECEIVING && !RTS_SENDING), 任一不满足 → EBUSY) + `src != dst` 显式拒绝 (EINVAL, C 用 assert) + flags 提取 (SYS_UPD_ROLLBACK). 新增 `proc_is_updatable(p)` 公开函数 + `SYS_UPD_ROLLBACK` 常量. 7 个测试覆盖 (NONE src / self-swap / busy process / quiescent / user-mode OK / receiving-only OK / kernel-blocked REJECT). DEFERRED: inherit_priv_irq/io/mem + adjust_asyn_table + abort_proc_ipc_send + swap_proc_slot body + adjust_proc_slot + adjust_priv_slot + swap_proc_slot_pointer (per-CPU ptproc) + swap_memreq + stale_tlb invalidation |
| F-39 | `do_sprofile` | do_sprofile.c:20-132 | `misc.rs:689` `dispatch_profile` | ⚠️ Partial (2026-06-16): 4/8 步实现 | ✅ (2026-06-16) **状态机实现**: 新增 `SPROFILING: AtomicBool` 全局 (C `int sprofiling`); PROF_START 用 `compare_exchange(false→true)` 原子地 check-and-set, 已经在跑 → EBUSY; PROF_STOP 用 `compare_exchange(true→false)` 原子地 check-and-clear, 没在跑 → EBUSY; validation 失败自动 rollback (不污染 future PROF_START). **测试基础设施**: 新增 `SPROF_TEST_LOCK: AtomicBool` 自旋锁 (no_std 兼容, 替代 std::sync::Mutex) + `sprof_test_setup()`/`teardown()` 串行化并行 cargo test 之间的状态. 9 个测试覆盖 (5 个原有 + 4 个新: double-start EBUSY / stop-without-start EBUSY / start-rollback / stop-after-start ENOSYS). DEFERRED: init_profile_clock (PROF_RTC, P0-08) + nmi_watchdog_start/stop (PROF_NMI, P0-15 arch-abstractions) + data_copy sprof_info + data_copy sample buffer (P1-05 Direct Map) + clean_seen_flag (MF_SPROF_SEEN) |
| F-40 | `do_diagctl` | do_diagctl.c | `syscall.rs:670` `dispatch_diagctl` | ⚠️ Partial | REGISTER/UNREGISTER已实现(SYS_PROC权限检查+s_diag_sig); DIAG/STACKTRACE返回ENOSYS(依赖data_copy_vmcheck/proc_stacktrace) |
| F-41 | `do_abort` | do_abort.c:17-28 | `syscall.rs:298` `dispatch_abort` | ❌ Not ported | 需 PM/TTY |
| F-42 | `do_vmctl` | do_vmctl.c:24-173 | `syscall.rs` `dispatch_vmctl` | ⚠️ Partial | VmInhibitSet/Clear + ClearPageFault + BootInhibitClear + MemReqGet/MemReqReply 已实现 (VmRequestQueue 已接入 ProcessTable); arch-specific (GetPdbr/SetAddrSpace/FlushTlb/InvlPg/ClearMapCache) 返回 ENOSYS 待 arch trait; KernPhysMap/KernMapReply 32-bit only |
| F-43 | `do_getmcontext` | do_mcontext.c:23-60 | `syscall.rs:884` `dispatch_getmcontext` | ⚠️ Partial (2026-06-16) | ✅ 已修复 (2026-06-16): 完整实现 input validation — endpoint_to_nr (EINVAL) + is_kernel(target) (EPERM); 移除错误的 SYS_PROC caller 检查 (C 不要求); 移除 FPU fast path (`MiscFlagsBits` 无 `FPU_INITIALIZED`, 64-bit 用 lazy FPU init); 签名扩展加 proc_table. DEFERRED: data_copy (Direct Map P1-05) + save_fpu + mc_flags 设置 + __fpregs copy (SignalContext trait P0-15). 3 个测试覆盖 (invalid endpoint / kernel target EPERM / user target ENOSYS) |
| F-44 | `do_setmcontext` | do_mcontext.c:62-106 | `syscall.rs:956` `dispatch_setmcontext` | ⚠️ Partial (2026-06-16) | ✅ 已修复 (2026-06-16): 完整实现 input validation — endpoint_to_nr (EINVAL); 关键设计: C **不**检查 iskerneln (kernel 进程可在 context switch 时恢复 FPU 状态); 签名扩展加 proc_table. DEFERRED: data_copy user mcontext + FPU state copy + release_fpu (SignalContext trait P0-15). 3 个测试覆盖 (invalid endpoint / **kernel target 允许** / user target ENOSYS) |
| F-45 | `do_schedule` | do_schedule.c:13-30 | `syscall.rs:387` `dispatch_schedule` | ⚠️ Partial (2026-06-16) | ✅ 已修复 (2026-06-16): 完整实现输入验证 — SYS_PROC 权限检查 (caller_has_sys_proc) + endpoint_to_nr (EINVAL) + p_scheduler 权限检查 (EPERM, target.p_sched.scheduler 为 None 时允许, 与 C 一致); 签名扩展加 proc_table. DEFERRED: sched_proc 主体 (P0-05 per-CPU 调度队列). 2 个测试覆盖 (非 SYS_PROC caller EPERM / NONE endpoint 路径) |

### 4.10 Arch-stub (4 handlers, x86_64 / aarch64 only)

| # | C Handler | C File:Line | Rust 实现 | 状态 | 备注 |
|---|-----------|-------------|-----------|------|------|
| F-46 | `do_sdevio` (x86) | do_devio.c (inlined) | `syscall.rs:321` `dispatch_sdevio` | ❌ Not ported | x86-only; arch-stub |
| F-47 | `do_iopenable` (x86) | do_iopenable.c | `syscall_device.rs:491` | ✅ Implemented (2026-06-16) | SELF 解析 + endpoint 验证 + iskerneln 检查 + IOPL=3 修改 initial_status; &mut ProcessTable 接入; 5 个单元测试; trap frame 更新 DEFERRED (需 scheduler 集成) |
| F-48 | `do_readbios` (x86) | do_readbios.c | `syscall.rs:327` | ❌ Not ported | x86-only |
| F-49 | `do_padconf` (aarch64) | do_padconf.c | `syscall.rs:331` | ❌ Not ported | ARM-only |

**38 + 3 (inlined) + 1 (schedule) = 42 entries, 3 phantom (datacopy/sdevio-file/unused) → 38 actual.**

## 5. Syscall 编号映射 (`minix/include/minix/com.h:205-267`)

KERNEL_CALL = 0x600 (per `com.h:204`)

| call_nr | C 名称 | Rust Handler | 状态 |
|---------|--------|--------------|------|
| 0 | SYS_FORK | `syscall_process.rs:114` | ✅ Implemented (2026-06-14) | 完整 C→Rust 映射表（11 行）见 `16-syscall-process.md` §2.1 "Rust 实现现状"；遗留 FPU save/restore / sched_proc / ProcessTable 传递 3 项 DEFERRED |
| 1 | SYS_EXEC | `syscall_process.rs:161` | ⚠️ Partial |
| 2 | SYS_CLEAR | `syscall_process.rs:335` | ⚠️ Partial→Partial++ (2026-06-16) | ✅ **(2026-06-16) 修复 P0 语义漂移**: 1) 旧代码操作 caller (PM) 而非 target — PM 调用 SYS_CLEAR 会把自己标记 SLOT_FREE! 现在正确操作 target; 2) 新增 endpoint 验证 (isokendpt → EINVAL); 3) 新增 isemptyp 提前返回; 4) 传入 ProcessTable + PrivTable, 实现 SYS_PROC privilege slot 释放 (s_proc_nr = None); 5) DEFERRED: release_address_space / IRQ hooks / clear_endpoint / reset_kernel_timer |
| 3 | SYS_SCHEDULE | `syscall.rs:280` | ❌ BadCall (in-kernel) |
| 4 | SYS_PRIVCTL | `syscall.rs:281` | ❌ BadCall |
| 5 | SYS_TRACE | `misc.rs:198` | 🚧 ENOSYS |
| 6 | SYS_KILL | `syscall_signal.rs:89` | ✅ Implemented |
| 7 | SYS_GETKSIG | `syscall_signal.rs:110` | ✅ Implemented |
| 8 | SYS_ENDKSIG | `syscall_signal.rs:132` | ✅ Implemented |
| 9 | SYS_SIGSEND | `syscall_signal.rs:155` | 🚧 Stub |
| 10 | SYS_SIGRETURN | `syscall_signal.rs:178` | 🚧 Stub |
| 11 | SYS_KERNINFO | not in Rust | ❌ (covered by boot image) |
| 12 | SYS_GETEP | not in Rust | ❌ |
| 13 | SYS_MEMSET | `syscall_copy.rs:360` | 🚧 Stub |
| 14 | SYS_UMAP | `syscall_copy.rs:278` | ⚠️ Partial |
| 15 | SYS_VIRCOPY | `syscall_copy.rs:270` | ⚠️ Partial→Partial+ (2026-06-16) | ✅ (2026-06-14) `virtual_copy_vmcheck`; **✅ (2026-06-16) 完整 isokendpt 验证**: 传入 ProcessTable, `endpoint_to_nr()` 替代简单范围检查, 4 个测试覆盖; DEFERRED: PTE walk |
| 16 | SYS_PHYSCOPY | `syscall_copy.rs:283` | ⚠️ Partial→Partial+ (2026-06-16) | 同 SYS_VIRCOPY (共享 dispatch_copy 实现) |
| 17 | SYS_UMAP_REMOTE | `syscall_copy.rs:298` | 🚧 Stub |
| 18 | SYS_VUMAP | `syscall_copy.rs:338` | 🚧 Stub |
| 19 | SYS_IRQCTL | `syscall_device.rs:174` | ✅ Done (2026-06-16) | SetPolicy/RmPolicy/Enable/Disable; generic_handler DEFERRED |
| 20 | SYS_INT86 | not in Rust | ❌ (x86-only BIOS call) |
| 21 | SYS_DEVIO | `syscall_device.rs:325` | ✅ Implemented (2026-06-16) | PortIo trait 迁移至 minix_plat, x86_64 X86_64PortIo (in/out inline asm), dispatch_arch_devio 接入 dispatch_devio |
| 22 | SYS_SDEVIO | `syscall.rs:321` | ❌ BadCall (x86-stub) |
| 23 | SYS_VDEVIO | `syscall_device.rs:419` | ⚠️ Partial (参数验证+ENOSYS; x86_64 已接线; 批量I/O DEFERRED) |
| 24 | SYS_SETALARM | `syscall_clock.rs:96` | ✅ Implemented (2026-06-15) |
| 25 | SYS_TIMES | `syscall_clock.rs:77` | ✅ Implemented (2026-06-15) |
| 26 | SYS_GETINFO | `misc.rs:155` | ⚠️ Partial |
| 27 | SYS_ABORT | `syscall.rs:298` | ❌ BadCall |
| 28 | SYS_IOPENABLE | `syscall_device.rs:491` | ✅ Implemented (2026-06-16) | SELF 解析 + IOPL=3; trap frame DEFERRED |
| 29 | SYS_NR_PROCS | not in Rust | ❌ (info via getinfo) |
| 30 | SYS_GETSCHED | not in Rust | ❌ |
| 31 | SYS_SAFECOPYFROM | `syscall_copy.rs:216` | 🚧 Stub |
| 32 | SYS_SAFECOPYTO | `syscall_copy.rs:236` | 🚧 Stub |
| 33 | SYS_VSAFECOPY | `syscall_copy.rs:256` | 🚧 Stub |
| 34 | SYS_SETGRANT | `syscall.rs:436` | ✅ Fixed |
| 35 | SYS_READBIOS | `syscall.rs:327` | ❌ BadCall (x86-stub) |
| 36 | SYS_SPROF | `misc.rs` `dispatch_profile` | ⚠️ Partial | ✅ (2026-06-16): 输入验证(ProfAction/ProfIntrType+isokendpt); DEFERRED: sprofiling状态+timer+data_copy |
| 37 | SYS_SPROF_DETAIL | not in Rust | ❌ |
| 38 | SYS_DIAG_TRACE | not in Rust | ❌ |
| 39 | SYS_STIME | `syscall_clock.rs:314` | ✅ Implemented (2026-06-15) |
| 40 | SYS_SETTIME | `syscall_clock.rs:360` | ✅ Implemented (2026-06-15) |
| 41 | SYS_VMCTL | `syscall.rs` `dispatch_vmctl` | ⚠️ Partial | VmInhibitSet/Clear + ClearPageFault + BootInhibitClear + MemReqGet/MemReqReply 已实现; arch 返回 ENOSYS |
| 42 | SYS_DIAGCTL | `syscall.rs:670` | ⚠️ Partial |
| 43 | SYS_DIAGCTL_DONE | not in Rust | ❌ |
| 44 | SYS_AFFINITY | not in Rust | ❌ |
| 45 | SYS_VTIMER | `syscall_clock.rs:412` | ✅ Implemented (2026-06-15) |
| 46 | SYS_RUNCTL | `syscall_process.rs:418` | ✅ Done (2026-06-16) | isokendpt+iskerneln+RC_STOP(RC_DELAY/MF_SIG_DELAY/EBUSY)+RC_RESUME(debug_assert+PROC_STOP); SMP IPI DEFERRED |
| 47 | SYS_PADCONF | `syscall.rs:331` | ❌ BadCall (arm-stub) |
| 48 | SYS_DUMP_DATA | not in Rust | ❌ |
| 49 | SYS_DIAG_MISC | not in Rust | ❌ |
| 50 | SYS_GETMCONTEXT | `syscall.rs:310` | ❌ BadCall |
| 51 | SYS_SETMCONTEXT | `syscall.rs:311` | ❌ BadCall |
| 52 | SYS_UPDATE | `misc.rs:217` | 🚧 ENOSYS |
| 53 | SYS_EXIT | `syscall_process.rs:194` | ⚠️ Partial |
| 54 | SYS_SCHEDCTL | `syscall_process.rs:502` | ✅ Fixed (2026-06-17) |
| 55 | SYS_STATECTL | `syscall_process.rs:310` | ⚠️ Partial→Partial+ | ✅ (2026-06-14) SetStateTable+ClearIpcFilters implemented; 3 DEFERRED |
| 56 | SYS_SAFEMEMSET | `syscall_copy.rs:379` | 🚧 Stub |
| 57 | SYS_PROFILE | not in Rust | ❌ (covered by sprofile) |

**统计**: 47 个 call_nr → 1 完全实现 (SYS_SCHEDCTL 2026-06-17), 10 Partial, 19 Stub, 17 BadCall/未实现

## 6. 设计差异 (Rust 引入的特性使 C 逻辑过时)

| # | C 逻辑 | Rust 处理 | 理由 |
|---|--------|----------|------|
| D-01 | `lin_lin_copy` (linear-to-linear copy) | 用 Direct Map 单指令 memcpy | Direct Map 是内核假设的地址布局 (Doc 17 D11); 实现尚未到位 |
| D-02 | `createpde` (动态创建页目录) | 用 `direct_map` 替换 | 直接映射省去页目录操作 |
| D-03 | `_cpus` 数组硬编码 | 用 `CpuLocal` per-CPU 数据 | Rust 类型系统隔离 cross-CPU 误用 |
| D-04 | `mini_notify` 返回 `int` | 返回 `Result<(), IpcError>` | Rust idiom; ✅ (2026-06-14) §14 语义对齐文档已添加至 `IpcEngine::notify` docstring |
| D-05 | `endpoint_lookup` 全局查表 | 类型化 `Endpoint` 类型 | Rust 类型系统防止 endpoint 重用 bug |
| D-06 | `proc_init`/`system_init`/`bsp_finish` C 顺序 | 部分合并到 `init_post_and_memory` | 早期 init 合并 |
| D-07 | `do_datacopy` (libsys 宏) | 保留 `dispatch_datacopy` 并标注 dead code | SYS_DATACOPY = sys_vircopy, 无独立 handler; data_copy_vmcheck 供 SIGSEND/GETINFO 使用 |
| D-08 | `arch/i386` 专属中断控制器 | `minix_plat::InterruptController` trait | 3 架构统一接口 |
| D-09 | `p_stale_tlb[]` TLB staleness 位图 | 用 aarch64 ASID / riscv64 ASID 替代 | 现代硬件自带 ASID |
| D-10 | `kputc`/`panic` 内核 print | `log::error!()` / `minix_kernel_log!()` | Rust 标准库 |
| D-11 | C 直接 memset/BSS 清零 | Rust `static` + 链接器 BSS | 编译时生成 |
| D-12 | `IRQ_USE_GDT_MASK` 等宏 | `IrqManager` 类型 | Rust trait 抽象 |
| D-13 | C `RTS_*` 位运算 | `RtsFlags(AtomicU32)` bitflags | 类型安全 + 原子 (SMP 关键) |
| D-14 | `endpoint` 全局变量 | `Endpoint` 类型 + `p_endpoint` 字段 | 类型化 + 字段化 |
| D-15 | `_cpus_id` 全局 | `SmpState::cpus[]` 数组 | 类型化 |
| D-16 | `krandom` (硬件 RNG) | 无 | x86 RDRAND 在 arch 层, 暂不导出 |
| D-17 | `_free_pde_slots` 全局数组 | ✅ (2026-06-14) `static mut FREE_PDE_SLOTS` + `free_pde_slots()` 访问器 | 见 SMP/free-pde-slots audit; 已在 `init_post_and_memory` 中持久化. `static FREE_UPPER_IDX: AtomicUsize` + `free_upper_idx()` (Acquire) / `advance_free_upper_idx(n)` (AcqRel) 访问器已实现. `KernelInfo.free_upper_idx` 类型已改为 `Option<usize>` + getter 方法 (2026-06-16). 2 个新测试覆盖 |
| D-18 | `m1.m1p1` 字段重用 | Doc 17 §1.1 重构 | 见 message-layout audit bug |
| D-19 | `do_signal_manager` 状态机 | `SignalContext` trait 缺失 | 见 signal-manager audit; trait 定义未到位 |
| D-20 | 同步 IPC `mini_send`/`receive` | ✅ **Partial (TODO, 2026-06-14)**: `IpcEngine::send/receive/sendrec` 骨架实现, 遵循 C `proc.c:870-952 / 1032-1118` 语义; WILLRECEIVE 匹配 + MF_DELIVERMSG + deadlock 检测 + MF_REPLY_PEND 短路 (SENDREC 关键优化); `&mut [KProcess]` BKL 类型代理; `SendFlags::NON_BLOCKING` 改 `flags.0 & NON_BLOCKING.0 != 0` (无 bitflags 依赖). 遗留 (kernel IPC core / dispatcher integration): `senda` (mini_senda 批量异步) **TODO**: `IpcEngine::senda` 函数已添加（单目标退化，委托 `send`）+ 5 步批量实现路径 doc 注释 + 与 `ClearIpcRefs` (TODO) 配对落地; 仍 DEFERRED: `do_ipc` dispatcher 集成未接通, `copy_msg_from_user` 用户态拷贝未实现. 见 kernel IPC core follow-up. |

## 7. 完整 TODO 清单 (P0/P1/P2 来自深度 review)

> **注**: 本节中的 `C-xx`/`S-xx`/`P0-xx`/`P1-xx`/`P2-xx`/`D-xx` 编号源自历史深度 review 报告 (cc-scan.md / glm-scan.md, 已删除)。编号本身已无独立定义文件，但每项均附有描述性文字说明问题内容，可作为历史跟踪标签保留。

### P0 (16 项 — 必须修复)

| # | 位置 | 描述 | 来源 |
|---|------|------|------|
| P0-01 | smp.rs | 无 BKL 实现 | Phase C 审计 | **部分修复 2026-06-15**: BKL 已接入 6 个核心入口/出口点: kernel_call_dispatch (acquire), kernel_call_finish (release, all paths), handle_exception (条件 acquire/release — 只在用户态异常时获取, is_nested 时不获取, 对应 C 的 exception_entry_nested), kmain step 8.5 (boot acquire, C: main.c:149), switch_to_user (release), kernel_call_resume (re-acquire via dispatch). 修复死锁: handle_exception 条件获取 BKL. bkl_unlock 添加 debug_assert. bkl_lock_section 移除多余 mem::forget. 移除 BklGuard #[must_use] (BKL 是显式 lock/unlock 模式, 非 RAII). IPC (send/receive/sendrec/notify) 添加 BKL precondition 注释 (C 中 IPC 不释放 BKL). clock tick_bsp/tick_ap 添加 BKL precondition 注释 (C 中 timer handler 在 context_stop 后调用). 剩余: 中断框架入口获取 BKL |
| P0-02 | ipc.rs | IPC 核心未实现 (仅 notify + detect_deadlock) | Phase C 审计 |
| P0-03 | syscall.rs:188-189 | ~~s_k_call_mask 权限检查缺失~~ ✅ 已修复 | Phase C 审计 |
| P0-04 | irq_manager.rs:412-437 | ✅ **修复 (2026-06-16)**: 之前只有 PageFaultInfo + handle_exception 框架, 缺少 RTS_PAGEFAULT 状态机; **本次**: 新增 `page_fault.rs` 模块 — `set_pagefault_pending` (RTS_SET 替代) + `clear_pagefault_pending` (返回 was_set 替代 C assert) + `is_pagefault_pending` (调度器查询) + `last_fault_addr` + `kernel_mode_pagefault_panic_msg` (inkernel_disaster 替代) + `build_vm_pagefault_msg` (mini_send 替代); 新增 `KProcess::p_fault_addr: Option<u64>` 字段; 新增 `MessVmPagefault` IPC struct + `m_vm_pagefault` union 字段. 8 个测试覆盖 (set/clear roundtrip / idempotent / 默认 false / VM msg fields / panic msg / kernel 跳过契约 / 端到端). 387 个 kernel 测试全绿. DEFERRED (P0-15): arch trap entry 调 handle_exception + mini_send 真实 IPC | Phase C 审计 |
| P0-05 | sched.rs:40-43 | ✅ **完成 (2026-06-16)**: 之前 per-CPU 运行队列已实现 (`Scheduler` struct, `sched_for_cpu`/`sched_for_cpu_mut`/`pick_proc`), 但 `sched_proc` (C `system.c:642`) 仍是 DEFERRED. **本次**: 完整实现 `sched::sched_proc(p, priority, quantum, cpu, niced)` — (1) priority 范围验证 `[TASK_Q=0, NR_SCHED_QUEUES=16]`, -1 sentinel 保留 (2) quantum 范围验证 `>=1`, -1 sentinel (3) cpu 范围 stub (单 CPU 总是 OK) (4) RTS_NO_QUANTUM 切换 (5) priority/quantum/cpu/niced 字段更新 (6) cpu_time_left 重置 (quantum * TSC_PER_MS). wired 到 `dispatch_schedule` 替代之前直接 ENOSYS. 新增 `SchedProcError` enum + `sched_proc_error_to_errno` helper. 11 个测试覆盖 (priority change / -1 sentinel / negative rejected / too high rejected / quantum update / quantum zero rejected / niced set/clear / cpu update / errno mapping / 端到端). 410 个 kernel 测试全绿. DEFERRED: SMP migration (`smp_schedule_migrate_proc` body, 单 CPU 不可达) | Phase C 审计 |
| P0-06 | smp.rs:77-88 | ~~CpuLocal 缺调度队列字段~~ ✅ 已修复 (2026-06-16): `CpuLocal` 新增 `scheduler: Scheduler` 字段 (对应 C cpulocals.h:58-59) | Phase C 审计 |
| P0-07 | lib.rs system_init | system_init 函数不存在 | Phase B 审计 |
| P0-08 | lib.rs:1043-1086 | bsp_finish_booting 7 step 状态 | ✅ 已修复 (2026-06-16): 7/7 已实现 — step 1 vm_running / step 2 bill_to_idle / step 3 announce / step 4 RTS_PROC_STOP / **step 5 cycles_accounting_init** (read_tsc + cpu_local_mut(bsp).note_context_switch) / **step 6 boot_cpu_init_timer** (CurrentClockArch::init_timer(100Hz) — 显式重设; 真正的 IRQ handler 注册待 IrqManager global P0-15) / **step 7 fpu_init** (cpu_local_mut(bsp).fpu_presence = true). bsp_finish_booting 签名扩展为 (&mut ProcessTable, &mut SmpState); kmain 创建 SmpState::new_single_cpu() 并传递; 2 个测试覆盖 (Step 5/7 副作用 + 单 CPU AP 状态). **本次 (2026-06-16)**: `dispatch_irqctl` (syscall.rs:505) 集成层 input validation — (1) IrqctlRequest::try_from (EINVAL 拒绝未知 request) (2) IRQ vector 范围 `[0, NR_IRQ_VECTORS)` (SETPOLICY 专用) (3) SYS_PROC caller 检查 (EPERM 拒绝). **关键设计**: 不依赖全局 IrqManager — input validation 与 hook chain ops 完全分离, 前者全 stack-local, 后者等 P0-15 全局化后补完. 5 个测试覆盖 (unknown request / negative irq / too-high irq / non-sys-proc / disable validation). 415 个 kernel 测试全绿. DEFERRED: IrqManager global 注册 + IRQ_ENABLE/DISABLE/SETPOLICY hook chain ops |
| P0-09 | lib.rs add_memmap | ~~add_memmap 已定义但从不调用~~ ✅ FIXED: kmain Phase F calls add_memmap + **TODO 返回值完整性 (Pattern §31)**: 原 `let _ = add_memmap(...)` 静默丢弃 `Result`; 修复为 `match result { Ok(_) => {}, Err(e) => { #[cfg(debug_assertions)] panic!; #[cfg(not)] let _ = e; } }` (debug 模式 panic, release 模式 carry-on — 与 C `add_memmap void` 失败后 panic 行为一致). ZeroLength/NoSlots 两个 variant 列出 | Phase B 审计 |
| P0-10 | lib.rs memory_init | memory_init 未调用 | Phase B 审计 |
| P0-11 | Doc 21 | PrivFlags 表自相矛盾 (line 23 vs 200) | Phase D 审计 |
| P0-12 | Doc 22 + kpriv.rs | CHECK_IPC phantom API | Phase D 审计 | ✅ 已修复: Doc 21/22 删除 CHECK_IPC, 确认 C 源码无此标志 |
| P0-13 | Doc 23 | 引用不存在的 `do_datacopy.c` | Phase D 审计 | ✅ 已修复: Doc 23 已修正为引用 syslib.h:129 + do_copy.c + arch/i386/memory.c:690; cross_space.rs 添加 ARCHITECTURE NOTE |
| P0-14 | Doc 00 | 导航表完全陈旧 (引用旧文件名) | Phase A 审计 |
| P0-15 | Doc 15 | 声称 D1 Spinlock<()>, 代码完全没有 | Phase C 审计 | ✅ 已修复 2026-06-13: smp.rs 新增 BklGuard/bkl_lock/bkl_unlock 框架 (AtomicBool + CAS 自旋, no_std 友好) + 3 个测试; Doc 15 §3 D1 行 + §4.1 ARCHITECTURE NOTE; P0-01 接入点仍待改造. **TODO 类型系统强制**: 新增 `BklSection<'a>` 零大小 witness (生命周期绑 BklGuard) + `bkl_lock_section()` 打开 section + `smp_state_with(section, &SmpState) -> &SmpState` typed accessor. 调用方必须先 `bkl_lock_section()` 才能调 typed accessor. 故意非 RAII 释放 (D6). 3 个新测试 (`test_bkl_section_provides_typed_access` / `test_bkl_section_paired_unlock` / `test_bkl_section_drop_does_not_unlock`) 验证类型强制. **本次 (2026-06-16)**: 新增 `ArchBoot` trait (`os/arch/src/arch/arch_boot.rs`) — `register_timer_handler` + `enable_timer_irq` + `disable_timer_irq`, 替代之前 `boot_cpu_init_timer` 依赖全局 IrqManager 的 DEFERRED. 提供 `TimerHandlerFn = fn(IrqVector, IrqId) -> IrqAction` 类型别名 + `CurrentArchBoot` 类型别名 (x86-64 → X86_64ArchBoot, 其他 → MockArchBoot) + `boot_init_timer` helper. wired 到 `bsp_finish_booting` step 6: `boot_init_timer::<CurrentArchBoot>(dummy_timer_handler)`. 5 个 arch crate 测试覆盖 (mock register / enable+disable / boot_init_timer 两者都做 / CurrentArchBoot 类型检查 / BootError variants). 415 个 kernel 测试 + 100 个 arch 测试全绿. DEFERRED: 真硬件 IOAPIC RTE binding (x86) / LVT 编程 (aarch64) + IrqManager global (P0-15 收尾) |
| P0-16 | STATE.md | STATE.md 陈旧 (仍标 07 待新建) | Phase A 审计 |
| P0-17 | smp.rs/irq_manager.rs/sched.rs/clock.rs | BKL 保护全靠注释, 无类型系统强制 | Phase C 审计 | **部分修复 2026-06-15**: syscall.rs + irq_manager.rs 已接入 bkl_lock/bkl_unlock; BklSection 类型 witness 已存在 (TODO). 剩余: sched.rs/clock.rs 接入 |
| P0-18 | cross_space.rs:70 | dispatch_datacopy 是死代码 | Phase D 审计 | ✅ 已文档化: 添加 ARCHITECTURE NOTE 说明 Minix3 无 SYS_DATACOPY 内核调用号; data_copy_vmcheck 保留供 SIGSEND/GETINFO 使用 |
| P0-19 | ipc_filter.rs:36-86 | IPC filter 未接入调度 | Phase D 审计 |
| P0-20 | syscall.rs tests | ~~kernel_call_dispatch 测试 BKL 泄漏~~ ✅ 已修复 (2026-06-15): test_kernel_call_dispatch_bad_call 和 test_kernel_call_dispatch_call_denied_no_priv 调用 kernel_call_dispatch 获取 BKL 但不释放, 导致后续测试在 bkl_lock CAS 循环中死锁. 修复: 每个测试末尾添加 crate::smp::bkl_unlock(). 根因: kernel_call_dispatch 只获取 BKL, 释放由 kernel_call_finish 负责, 但测试只测 dispatch 不调 finish |
| P0-21 | proc_table.rs:502-549 | ~~process_misc_flags 无限循环~~ ✅ 已修复 (2026-06-15): KCALL_RESUME/DELIVERMSG/SC_DEFER 三个分支只有注释没有清除 flag, 导致 loop 永不退出. 修复: 每个分支添加 p_misc_flags.clear() 对应 flag. SC_TRACE 分支同时清除 SC_TRACE+SC_ACTIVE (与 C proc.c:397 一致). 根因: C 中这些分支调用 handler 函数 (kernel_call_resume/delivermsg/arch_do_syscall) 清除 flag, Rust handler 尚未接入循环 |

### P1 (top 15)

| # | 位置 | 描述 |
|---|------|------|
| P1-01 | syscall_process.rs:114-153 | ~~dispatch_fork 缺进程拷贝/generation/FPU/VMINHIBIT~~ ✅ 已修复 (2026-06-14): ProcessTable + PrivTable + fork_new_endpoint + fork_from + `complete_fork_setup` (SYS_PROC downgrade: RTS_NO_PRIV + USER_PRIV_ID + VMINHIBIT + "*F" suffix); 遗留: FPU (arch), sched_proc |
| P1-02 | syscall_process.rs:335-408 | ~~dispatch_clear stub~~ ✅ 已修复 (2026-06-14): RTS_SLOT_FREE + EXT_REG_INITIALIZED 清除; **✅ (2026-06-16) 修复 P0 语义漂移**: 操作 target 而非 caller + endpoint 验证 + isemptyp 提前返回 + PrivTable 传入 + SYS_PROC privilege slot 释放; DEFERRED: addr space/IRQ/timer/IPC |
| P1-03 | syscall_process.rs:502-589 | ~~dispatch_schedctl 缺 sched_proc 调用~~ ✅ **已完整修复 (2026-06-17)**: 新增 `MessLsysKrnSchedctl` 消息类型 (精确匹配 C `mess_lsys_krn_schedctl` 布局) + `msg_schedctl()` helper (修复 `m2` union overlay 字段映射 bug) + `&mut ProcessTable` 签名扩展 + `endpoint_to_nr` 目标进程解析 (非 caller) + `sched_proc(target, priority, quantum, cpu, false)` wire-up + 错误传播 (`sched_proc_error_to_errno`) + `p_scheduler` 赋值 (None for KERNEL flag / Some(caller.p_nr) otherwise). 7 个测试覆盖. 419 kernel + 59 minix-types 测试通过 |
| P1-04 | syscall_process.rs:310-352 | ~~dispatch_statectl 缺 5 个 sub-request action~~ ✅ 已修复 (2026-06-14): StatectlRequest 枚举 1-5 (C com.h); MessLsysKrnSysStatectl; SetStateTable+ClearIpcFilters implemented; 3 DEFERRED |
| P1-05 | syscall_copy.rs:152-209 | ✅ **修复 (2026-06-16)**: 之前 `vm.rs::lookup_in_table` 完全是 `todo!()` — C `vm_lookup` (memory.c:325) 离线 PTE walk 完全没翻译. **本次**: 新增 `os/kernel/src/pte_walk.rs` 模块, 提供 (1) `read_pte` (Direct Map 读 PTE) (2) `pte_to_page_flags` (PTE → PageFlags 翻译, 隐藏硬件位编码) (3) `pte_to_phys` (PRESENT 检查 + 地址提取) (4) `vaddr_indices` (4-level index 提取) (5) `walk_x86_64` (完整 PML4→PDPT→PD→PT 4-level walk + 2MB/1GB huge page 处理). **关键 Rust 最佳实践**: (a) `unsafe read_pte` 强制 caller 确认 Direct Map 已 setup; (b) `Option<(PhysBytes, PageFlags)>` 替代 C 错误码 + out-param 模式; (c) 硬件位编码 (P=1, R/W=2, U/S=4, G=8, A=20, D=40, PS=80) 完全封装在 `pte_to_page_flags`, 上层只用 `PageFlags` enum; (d) `#[cfg(target_arch = "x86_64")]` 在 `lookup_in_table` 中分发, 保留 `<D>` 类型参数以备 aarch64/RISC-V 实现. 12 个测试覆盖 (PTE flag 翻译 PRESENT/WR/USER/GLOBAL/2MB huge / PTE_ADDR_MASK 正确性 / vaddr_indices 高/低/中地址 / 常量 / walk_x86_64 编译时检查); 399 个 kernel 测试全绿. DEFERRED: aarch64 (L0-L3 4-level) + RISC-V (Sv39/Sv48) PTE walk; QEMU 集成测试 (真实 page table deref 需要 boot-time setup) |
| P1-06 | syscall_copy.rs:174,222,243 | ~~m1.m1p1 字段重复使用 bug~~ ✅ 已修复: 专用 message overlay 类型 |
| P1-07 | syscall_signal.rs:132-144 | ~~dispatch_endksig 缺 s_sig_mgr 检查~~ ✅ 已修复 (2026-06-14): isokendpt + s_sig_mgr + SIG_PENDING + 条件清除; MessSigcalls 消息类型 |
| P1-08 | syscall_signal.rs:155-192 | ~~dispatch_sigsend/return 缺 SignalContext trait~~ | ✅ 已修复 (2026-06-14): `SignalContext` trait + `SigMsg` struct + 端点验证 + ENOSYS (DEFERRED: data_copy_vmcheck + SignalContext impl) |
| P1-09 | syscall_device.rs:174 | ~~dispatch_irqctl 不调用 IrqManager~~ | ✅ 已修复: `dispatch_irqctl<IC: InterruptController>` 接受 `&mut IrqManager<IC>` + `&PrivTable`，完整实现 SetPolicy/RmPolicy/Enable/Disable 四个请求 |
| P1-10 | syscall_device.rs:325-417 | ~~dispatch_devio/vdevio 缺 PortIo trait~~ | ✅ 已修复 (2026-06-14): `PortIo` trait + CHECK_IO_PORT + 对齐检查 + I/O 执行 + DIO 常量映射修复 + 7 个单元测试. **(2026-06-16) PortIo trait 迁移至 minix_plat, x86_64 X86_64PortIo 实现, dispatch_arch_devio 接入** |
| P1-11 | syscall_clock.rs:96 | ~~dispatch_setalarm 签名不符 Doc D5~~ | ✅ 已修复: `dispatch_setalarm` 接受 `&mut PrivTable, &mut ClockState`，Doc 20 D5 已更新为"已实现" |
| P1-11b | syscall_clock.rs:117,184 | dispatch_setalarm/vtimer 缺 SYS_PROC 权限检查 | ✅ **已修复 (2026-06-14) — KSC-1/KSC-2**: `caller_has_sys_proc()` fail-closed helper + EPERM；5 个新单元测试 |
| P1-12 | syscall_process.rs:194-199 | ~~dispatch_exit 缺 cause_sig(SIGABRT)~~ | ✅ 已修复 (2026-06-13): `cause_signal_abort(caller)` 助手 + 测试 `test_dispatch_exit_sets_sigabrt`. 遗留: `mini_notify(sig_mgr)` 待 P0-02/P1-08 |
| P1-13 | kpriv.rs:124 | ~~s_alarm_timer: u64 vs Doc Option<TimerEntry>~~ | ✅ 已修复 (2026-06-13): 改为 `Option<crate::clock::TimerEntry>`, 默认 `None` + 测试 `test_kpriv_alarm_timer_default_none`/`test_kpriv_alarm_timer_some_carries_action` |
| P1-14 | syscall_copy.rs:313 | ~~_seg_type 计算符号扩展 bug~~ | ✅ 误报: `i32 & 0xFF00(i32)` 与 C `int & 0xFF00` 行为一致，无符号扩展问题；且该变量是 stub（`_` 前缀未使用） |
| P1-15 | ipc_filter.rs:36-38 | ~~ipc_filter_check 签名不符~~ | ✅ 误报: C 用宏 `may_send_to(rp, nr)` 直接访问 `priv(rp)->s_ipc_to`，Rust 封装为 `ipc_filter_check(&KPriv, u16)` + `may_send_to()` 方法，签名改造合理 |
| P1-16 | lib.rs unsafe 块 | ~~11 个 unsafe 缺 SAFETY 注释~~ ✅ 已修复 (2026-06-14): 所有 unsafe 块已添加 SAFETY 注释 (jump_to_kmain/paging.enable/FREE_MEMMAP/hlt-wfi/naked kmain/MockHigherHalf) |
| P1-17 | sched.rs:225-250 | ~~read_tsc/get_monotonic stub~~ ✅ 已修复 (2026-06-14): clock.rs 新增 CLOCK_UPTIME+TSC_PER_MS 全局原子 + get_monotonic()/ms_to_cpu_time()/read_tsc() 公开函数; proc_table.rs stub 改为委托 clock 模块; sched.rs 删除重复 stub; **read_tsc() 完整实现 (2026-06-16)**: ClockArch trait 新增 read_tsc() 默认方法(委托 read_ticks()); kernel clock::read_tsc() 调用 minix_arch::CurrentClockArch::read_tsc(); cfg(test) 返回 0; 2 个测试覆盖 |
| P1-18 | ipc.rs:287 | ~~notify-bitmap path TODO~~ ✅ 已修复 (2026-06-14): `notify()` 新增 `PrivTable` 参数, 通过 `dst_priv_id` 查找 `KPriv`, 设置 `s_notify_pending |= 1u64 << caller_pid`; 2 个测试 |
| P1-19 | ipc.rs:248 | detect_deadlock RECEIVE 返回 None | ✅ 已修复 2026-06-13: 重构 detect_deadlock 抽出共享环检测 core + `(chain_field_getter, required_flag)` 闭包区分方向 (SEND→p_sendto_e/RTS_SENDING; RECEIVE→p_getfrom_e/RTS_RECEIVING) + 7 个测试 (2-proc/3-proc SEND/RECEIVE cycle + no-cycle + state-mismatch) + **TODO `DeadlockCycle.direction`**: `DeadlockCycle` 结构新增 `direction: DeadlockDirection` 字段携带 SEND/RECEIVE 信息给调用方; 新增 `DeadlockDirection` 枚举独立于 `IpcCall`（防止 deadlock detector 与 IPC dispatcher 语义耦合）. 调用方解读前不再需记忆 `IpcCall` 入参; direction 内嵌在返回值, 类型自解释. |
| P1-30 | proc.rs:988 | `RtsFlags::clear` TODO 注释未澄清 | ✅ **已修复 (2026-06-14) — TODO**: 文档重写为「Primitive, no scheduler hook」+ 两层 API 分解表: 1) **Level 1 (Primitive)** — `RtsFlags::clear` 纯标志位操作, 不调调度器 (适用 MF_REPLY_PEND/MF_DELIVERMSG 等); 2) **Level 2 (with scheduler hook)** — `ProcessTable::rts_unset` (`proc_table.rs:130-`) 在 primitive 之上加 `sched_enqueue` 调用于 runnable 边沿转换 (镜像 C `RTS_UNSET` 宏 `proc.h:142-152`). 新增「When to use which」决策表. 这一拆分使 C 的宏层分解得以保留, 同时通过类型区分防止误用. |
| P1-20 | misc.rs:151-191 | ~~dispatch_getinfo 4 sub-req 全 TODO~~ ✅ 已修复 (2026-06-14): GET_WHOAMI ✅; GET_KINFO ✅ (user_sp/freepde_start/vir_kern_start 从 kernel_info() 获取); GET_PROC/GET_PROC2 ✅ (2026-06-16: SELF替换+isokendpt endpoint验证+ProcessTable穿透); 通配分支返回 ENOSYS; ProcTab/PrivTab/LoadInfo 返回 ENOSYS (需 data_copy_vmcheck) |
| P1-21 | proc.rs | ~~p_nextready/p_q_link/p_caller_q plain (SMP unsafe)~~ ✅ 已修复 (2026-06-14): Option<ProcNr>→AtomicI32 + NONE_PROC_NR=-1; proc_table.rs/sched.rs/ipc.rs 全部改用 .load(Ordering::Relaxed)/.store(_, Ordering::Relaxed); 254 tests passed |
| P1-22 | syscall_signal.rs | ~~doc 18 s_sig_mgr 引用 KProcess, 实际在 KPriv~~ ✅ 已修复 (2026-06-14): `ProcessTable::sig_mgr()` 便捷方法封装 s_sig_mgr 查找 + SELF→p_endpoint 替换; syscall_signal.rs 3 处重复代码替换为 sig_mgr() 调用; Doc 18 已修正 |

### P2 (top 10)

| # | 位置 | 描述 |
|---|------|------|
| P2-01..10 | 多文档 | C 源行号 off-by-N (1-10 行) |

## 8. 收敛评估

### 8.1 当前状态: NOT_CONVERGED

| 维度 | 状态 | 详情 |
|------|------|------|
| 宏覆盖 | 62% | K-026 (CHECK_IPC) 已确认不存在于 C 源码, 非缺失 |
| 全局变量覆盖 | 78% | BKL 统计缺失; CHECK_IPC 已确认不存在于 C 源码 |
| 结构体覆盖 | 87% | PMAGIC/PMAGIC/load avg/defer 字段缺失 |
| do_* handler 覆盖 | 0% 完全实现 | 11 Partial + 18 Stub + 9 Not ported |
| Syscall dispatch | 23% 实际可工作 | 11/47 call_nr 部分工作 |
| BKL/SMP safety | **部分** | 框架就位 + 6 个核心入口点已接入 (2026-06-15): kernel_call_dispatch/finish, handle_exception (条件), kmain, switch_to_user, kernel_call_resume. IPC 4 方法 + clock tick 添加 BKL precondition 注释 (C 中不释放 BKL). per-CPU Scheduler 渐进式修复 (2026-06-16): CpuLocal 新增 scheduler 字段 + sched_for_cpu() 方法. 剩余: 中断框架入口获取 BKL (区分用户态/内核态) |
| QEMU E2E | **失败** | 0 个异常/中断/IPC/Syscall E2E 测试 |
| Doc 准确性 | ~85% | 3 处 P0 错误 (Doc 21/22/23) |

### 8.2 收敛路径 (按优先级)

**Phase 1 (4 周, 立即)** — 关闭所有 P0:
1. 实现 smp.rs BKL (P0-01)
2. 实现 IPC send/receive/sendrec/senda (P0-02)
3. syscall.rs 添加 s_k_call_mask 检查 (P0-03)
4. 实现 system_init + bsp_finish_booting + ~~add_memmap 调用~~ (P0-07/08/~~09~~ ✅)
5. memory_init 调用 (P0-10)
6. 修复 Doc 21/22/23 错误 (P0-11/12/13)
7. 更新 Doc 00 导航表 + Doc 15 D1 标注 (P0-14/15)
8. ~~更新 STATE.md (P0-16)~~ ✅ 已修复 (2026-06-14)
9. 接入 ipc_filter 到 dispatch (P0-19)
10. ~~删除 dead code dispatch_datacopy (P0-18)~~ ✅ 已文档化: 保留 data_copy_vmcheck, 标注 dispatch_datacopy 为 dead code

**Phase 2 (4-6 周)** — 关闭 P1:
- syscall handler partial 推进
- unsafe SAFETY 注释补全
- Doc 行号修正
- per-CPU run queues 接入 BKL
- ~~s_alarm_timer 类型修正~~ ✅ (P1-13, 2026-06-13: `u64` → `Option<TimerEntry>`)

**Phase 3 (1 月+)** — 关闭 P2 + QEMU E2E:
- QEMU test-kernel/pagetable, exception, ipc 扩展
- SMP QEMU (-smp 4)
- 异常/中断交付 E2E 测试

### 8.3 收敛判据 (per `.claude/rules/review-process.md`)

1. **所有 P0 = 0**
2. **所有 "已修复" 在 todo.md 二次 grep 通过** — 暂无 decay 风险
3. **P1 new ≤ 1** — 当前 21 项, 需大幅削减
4. **VERIFY-CHECK.md = PASS**
5. **所有 P0 在 FINDINGS.md 修复并验证**

### 8.4 关键文件路径索引

**Rust 实现**:
- 入口: `os/kernel/src/lib.rs` (1271 LOC)
- 进程: `os/kernel/src/proc.rs` (1780 LOC), `proc_table.rs` (750 LOC)
- 调度: `os/kernel/src/sched.rs` (364 LOC)
- IPC: `os/kernel/src/ipc.rs` (347 LOC), `ipc_filter.rs` (149 LOC)
- Syscall: `os/kernel/src/syscall.rs` (443 LOC) + 6 个 `syscall_*.rs`
- 时钟: `os/kernel/src/clock.rs` (665 LOC)
- 中断: `os/kernel/src/irq_manager.rs` (623 LOC)
- SMP: `os/kernel/src/smp.rs` (476 LOC)
- 特权: `os/kernel/src/kpriv.rs` (462 LOC)
- VM 启动: `os/kernel/src/vm.rs` (1296 LOC)
- 杂项: `os/kernel/src/misc.rs` (268 LOC), `cross_space.rs` (152 LOC), `memmap.rs` (164 LOC)

**C 源** (Ground Truth):
- `minix3/minix/kernel/main.c`
- `minix3/minix/kernel/proc.c`, `proc.h`
- `minix3/minix/kernel/system.c`, `system.h`
- `minix3/minix/kernel/clock.c`
- `minix3/minix/kernel/interrupt.c`, `interrupt.h`
- `minix3/minix/kernel/smp.c`, `smp.h`
- `minix3/minix/kernel/priv.h`, `proc.h`, `glo.h`, `ipc.h`, `kernel.h`
- `minix3/minix/kernel/system/do_*.c` (38 files)
- `minix3/minix/include/minix/com.h` (call numbers), `callnr.h`, `priv.h`

**文档**:
- `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/{00..24,99}-*.md`
- 历史回溯 (已删除): `glm-scan.md`, `kernel-design.md`, `runtime-design-m3.md`, `kboot-*.md`

**QEMU 测试**:
- `os/qemu-tests/test-kernels/kernel/bootstrap/*` (18 二进制)
- `os/qemu-tests/run_all.sh`
- `os/arch/tests/qemu_test_{x86_64,aarch64,riscv64}.sh`
- `os/kernel/tests/boot_integration.rs`

---

## 9. 验证命令

```bash
# 验证 P0-01 (smp.rs 无 BKL)
rg "spinlock|unsafe|BKL" os/kernel/src/smp.rs

# 验证 P0-02 (ipc.rs IPC 核心缺失)
rg "fn (send|receive|sendrec|send_nb|senda)" os/kernel/src/ipc.rs

# 验证 P0-03 (s_k_call_mask 缺失) — ✅ 已修复
# kernel_call_dispatch 现在调用 kcall_filter_check, 新增 KcallResult::CallDenied
rg "kcall_filter_check" os/kernel/src/syscall.rs

# 验证 P0-07 (system_init 缺失)
rg "fn system_init" os/kernel/src/

# 验证 P0-12 (CHECK_IPC phantom) — ✅ 已修复
# C 源码无 CHECK_IPC, Doc 21/22 已删除虚构引用
rg "CHECK_IPC" os/kernel/src/kpriv.rs

# 验证 P0-13 (do_datacopy.c 不存在)
ls minix3/minix/kernel/system/do_datacopy.c

# 验证 P1-16 (unsafe 块 SAFETY 缺失) — ✅ 已修复
rg "// SAFETY:" os/kernel/src/lib.rs
# 应输出多行 (每个 unsafe 块一条)

# 验证 P1-17 (get_monotonic/ms_to_cpu_time stub) — ✅ 已修复
rg "CLOCK_UPTIME|TSC_PER_MS|pub fn get_monotonic|pub fn ms_to_cpu_time" os/kernel/src/clock.rs
rg "clock::get_monotonic|clock::ms_to_cpu_time|clock::read_tsc" os/kernel/src/proc_table.rs

# 验证 P1-22 (s_sig_mgr 便捷方法) — ✅ 已修复
rg "fn sig_mgr" os/kernel/src/proc_table.rs
rg "proc_table.sig_mgr" os/kernel/src/syscall_signal.rs

# 验证 P0-18 (dispatch_datacopy 死代码)
rg "Datacopy|dispatch_datacopy" os/kernel/src/syscall.rs

# QEMU 测试数量
ls os/qemu-tests/test-kernels/kernel/bootstrap/ | wc -l
```