# 25-misc-unported: 杂项与未移植系统调用

> **分类**: 杂项
> **源码**: `minix3/minix/kernel/system/do_unused.c`, `do_getinfo.c`, `do_trace.c`, `do_update.c`, `do_profile.c`
> **前置**: 全部前序文档
> **C 总行数**: ~500 行

---

## Ch1: 概念

**核心问题**: 哪些系统调用尚未被前序文档覆盖？如何处理它们？

### 1.1 已覆盖的系统调用

| 文档 | 覆盖的系统调用 |
|------|--------------|
| 16 | SYS_FORK, SYS_EXEC, SYS_EXIT, SYS_CLEAR, SYS_RUNCTL, SYS_SCHEDCTL, SYS_STATECTL |
| 17 | SYS_VIRCOPY, SYS_PHYSCOPY, SYS_SAFECOPYFROM, SYS_SAFECOPYTO, SYS_VSAFECOPY, SYS_UMAP, SYS_UMAP_REMOTE, SYS_VUMAP, SYS_MEMSET, SYS_SAFEMEMSET |
| 18 | SYS_KILL, SYS_GETKSIG, SYS_ENDKSIG, SYS_SIGSEND, SYS_SIGRETURN |
| 19 | SYS_IRQCTL, SYS_DEVIO, SYS_VDEVIO |
| 20 | SYS_TIMES, SYS_SETALARM, SYS_STIME, SYS_SETTIME, SYS_VTIMER |
| 23 | SYS_DATACOPY |

### 1.2 未覆盖的系统调用

| 系统调用 | C 处理函数 | 优先级 | 说明 |
|---------|-----------|--------|------|
| SYS_GETINFO | `do_getinfo()` | 高 | 内核信息查询（版本、内存、进程表等） |
| SYS_TRACE | `do_trace()` | 中 | 进程追踪（ptrace） |
| SYS_PRIVCTL | `do_privctl()` | 高 | 权限控制（已在 21 中描述） |
| SYS_SDEVIO | `do_sdevio.c` | 低 | 安全设备 I/O（x86-only，类似 SDEVIO） |
| SYS_IOPENABLE | 内联 | 低 | I/O 权限启用（x86-only） |
| SYS_READBIOS | `do_readbios.c` | 低 | BIOS 数据读取（x86-only） |
| SYS_UPDATE | `do_update.c` | 低 | 进程更新（RS 使用） |
| SYS_PROFILE | `do_profile.c` | 低 | 性能分析 |
| SYS_UNUSED | `do_unused()` | N/A | 未使用的系统调用号 |

### 1.3 SYS_GETINFO 详解

GETINFO 是最复杂的未覆盖调用，它有多个子请求：

| 子请求 | 语义 | 返回数据 |
|--------|------|---------|
| GET_KINFO | 内核信息 | `struct kinfo` |
| GET_PROC | 进程表项 | `struct proc` |
| GET_PROCTAB | 整个进程表 | `struct proc[]` |
| GET_PRIVTAB | 权限表 | `struct priv[]` |
| GET_SCHEDINFO | 调度信息 | 优先级/队列 |
| GET_PROC2 | 扩展进程表项 | `struct proc` (64-bit) |
| GET_MACHINE | 机器信息 | `struct machine` |
| GET_KENV | 内核环境变量 | 字符串 |
| GET_LOCKTIMING | 锁计时 | 统计数据 |
| GET_BIOSCTRS | BIOS 计数器 | x86-only |
| GET_IRQHOOKS | IRQ 钩子表 | `struct irq_hook[]` |
| GET_RANDOMNESS | 随机数种子 | `struct randomness` |
| GET_CPUINFO | CPU 信息 | per-CPU 数据 |
| GET_LOADINFO | 负载信息 | `struct loadinfo` |

---

## Ch2: C 源码分析

### do_getinfo.c (197 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 20-197 | `do_getinfo()` | switch(request) → data_copy_vmcheck 到用户空间 |

### do_trace.c (87 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 20-87 | `do_trace()` | switch(request): TR_GETINS/TR_SETINS/TR_GETDATA/TR_SETDATA 等 |

### do_update.c (59 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 20-59 | `do_update()` | RS 进程更新：交换进程槽位 |

### do_profile.c (95 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 20-95 | `do_profile()` | 性能分析控制：开始/停止/重置 |

### do_unused.c (9 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 1-9 | `do_unused()` | 返回 ENOSYS |

---

## Ch3: Rust 设计决策

| # | 决策 | 选项 | 结论 | 理由 |
|---|------|------|------|------|
| D1 | GETINFO 子请求 | 整数 vs enum | **`GetInfoRequest` enum** | 类型安全 |
| D2 | 未实现调用 | panic vs ENOSYS | **返回 ENOSYS** | 与 C 一致 |
| D3 | GETINFO 数据拷贝 | 直接拷贝 vs 序列化 | **直接拷贝** | 与 C 一致，性能优先 |
| D4 | x86-only 调用 | 条件编译 vs BadCall | **BadCall** | 与 19 D6 一致 |
| D5 | TRACE | 保留 vs 延迟 | **延迟实现** | 调试功能，非核心 |

---

## Ch4: 实现要点

### 4.1 GetInfoRequest enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum GetInfoRequest {
    KInfo = 0,
    Proc = 1,
    ProcTab = 2,
    PrivTab = 3,
    SchedInfo = 4,
    Proc2 = 5,
    Machine = 6,
    KEvn = 7,
    LockTiming = 8,
    BiosCtrs = 9,
    IrqHooks = 10,
    Randomness = 11,
    CpuInfo = 12,
    LoadInfo = 13,
}
```

### 4.2 未实现调用处理

```rust
/// Handle unimplemented system calls.
/// C: `do_unused()` — do_unused.c
pub fn dispatch_unused() -> KcallResult {
    KcallResult::Ok(ENOSYS)
}
```

### 4.3 GETINFO 数据拷贝

```rust
pub fn dispatch_getinfo(caller: &mut KProcess, msg: &Message) -> KcallResult {
    match request {
        GetInfoRequest::KInfo => {
            // 构建 kinfo 结构
            // data_copy_vmcheck 到用户空间
        }
        GetInfoRequest::ProcTab => {
            // 拷贝整个进程表
        }
        // ...
    }
}
```

---

## 测试

- 单元：GetInfoRequest TryFrom<i32>
- 单元：dispatch_unused 返回 ENOSYS
- 单元：GETINFO 各子请求的数据大小

---

## 补充：调试、ACPI/看门狗与未迁移符号

> 来源：tmp-19-debug-serial.md, tmp-20-acpi-watchdog.md, tmp-21-unported-symbols.md

### 内核调试基础设施

内核在无标准输出环境下的调试工具集：

1. **串口输出（serial debug）**：通过 COM1 的 I/O 端口（0x3F8）输出调试信息，不依赖任何内核子系统
2. **进程转储（proc dump）**：打印进程表、调度队列、IPC 状态等内核数据结构
3. **调度队列一致性检查**：`runqueues_ok()` 验证就绪队列完整性
4. **消息追踪（IPC dump）**：条件编译下记录和打印所有 IPC 消息传递
5. **内核消息缓冲区（kmessages）**：环形缓冲区记录内核输出，供 `dmesg` 读取

条件编译宏：`DEBUG_SCHED_CHECK`、`DEBUG_DUMPIPC`、`DEBUG_SERIAL`、`DEBUG_TIME_LOCK`

| 功能 | 函数 | 源文件 |
|------|------|--------|
| 调度队列检查 | `runqueues_ok()` | debug.c:16 |
| RTS 标志字符串 | `rtsflagstr()` | debug.c:136 |
| 打印进程信息 | `print_proc()` | debug.c:249 |
| 串口输出 | `ser_debug()` | arch_system.c |
| 进程转储 | `ser_dump_proc()` | arch_system.c |
| 栈回溯 | `proc_stacktrace()` | arch_system.c |

### ACPI 电源管理与看门狗

**ACPI** 提供硬件拓扑发现和电源管理：

```
RSDP (Root System Description Pointer)
  └─ RSDT (Root System Description Table)
       ├─ MADT → CPU 拓扑、APIC 信息
       ├─ FADT → 电源管理寄存器
       │    └─ DSDT → AML 字节码
       └─ 其他 SSDT 表
```

**ACPI 关机流程**：`acpi_poweroff()` 通过向 PM1a_CNT / PM1b_CNT 寄存器写入 SLP_TYP + SLP_EN 实现关机。

**NMI 看门狗**：使用 CPU 性能计数器溢出中断作为 NMI 触发源。正常情况下时钟中断定期重置计数器，若内核死锁导致时钟中断停止，计数器溢出触发 NMI。

| 功能 | 函数 | 源文件 |
|------|------|--------|
| ACPI 初始化 | `acpi_init()` | arch/i386/acpi.c |
| 系统关机 | `acpi_poweroff()` | arch/i386/acpi.c |
| 看门狗初始化 | `arch_watchdog_init()` | arch/i386/arch_watchdog.c |
| 看门狗 NMI 处理 | `watchdog_nmi_handler()` | watchdog.c |

ACPI 行为规则：
1. ACPI 仅在 VM 运行前使用：`acpi_phys_copy()` 在 VM 运行后 panic
2. RSDP 搜索范围：BIOS 区域 0xE0000-0xFFFFF
3. 看门狗需要 APIC，仅支持 Intel/AMD

### 未迁移 C 函数清点

尚未被 06~20 号文档覆盖的 C 函数，按 ARCH 理由分类：

| ARCH 理由 | 缩写 | 含义 |
|-----------|------|------|
| x86-32 硬件指令 | HW | 直接使用 x86 I/O 端口指令或特定寄存器操作 |
| x86 分段/分页 | SEG | 操作 GDT/IDT/TSS/LDT 等 x86 保护模式数据结构 |
| x86 异常帧 | EXC | 依赖 x86 特定的栈帧布局 |
| 启动阶段专用 | BOOT | 仅在分页启用前的极早期启动中使用 |
| 条件编译守卫 | COND | 被 `#if USE_xxx` 等条件编译包裹 |
| 调试/诊断 | DBG | 仅用于内核调试输出 |
| 语义替代 | ALT | 功能需存在，但 64 位架构下实现方式完全不同 |

规则：
1. 每个未迁移函数必须标注 ARCH 理由
2. 语义保留原则：即使 C 函数不迁移，其功能必须在 minix-rs 中有对应实现
3. arch/earm/ 不迁移，minix-rs 仅面向 x86-64

---

## 参见

- [22-privilege.md](22-privilege.md) — SYS_PRIVCTL
- [20-syscall-device.md](20-syscall-device.md) — x86-only 调用处理
