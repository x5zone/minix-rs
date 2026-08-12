# 28-usermapped-data: 大纲（Outline v1）

> **文档**: `28-usermapped-data.md`
> **状态**: v1 快照（2026-08-12 首次生成）
> **教学目标**: 从"内核如何安全暴露信息给用户态"这一问题出发，建立"共享内存 vs 系统调用"的权衡心智模型

---

## Ch1: 概念（内核信息向用户态暴露的架构机制）

### 教学目标
- **核心问题**: 内核拥有大量信息（时钟、CPU、负载、诊断消息等），用户态进程需要频繁读取。每次系统调用都有开销（上下文切换 + 寄存器保存/恢复）。内核如何在不牺牲安全性的前提下，让用户态高效读取这些只读信息？
- **CPU/OS perspective question**: "内核如何安全暴露信息给用户态？"——这是地址空间隔离与信息共享的权衡问题。
- **目标读者**: 已读 02/06/07/09 的读者，理解内核地址空间布局与跨空间映射机制
- **前置知识**: 链接脚本段定义（02）、`arch_phys_map()` 机制（07）、VM 启动协议（09）

### 1.1 两种信息暴露策略
- **共享内存映射**（Minix3 32-bit）: 内核将数据结构放在专用段，VM 映射到每个进程地址空间，用户态直接指针读取
- **系统调用获取**（minix-rs 64-bit）: 用户态通过 `sys_getinfo` 等系统调用显式请求数据，内核拷贝到用户缓冲区

### 1.2 `.usermapped` section 的架构角色
- **链接器层面**: kernel.lds 定义 `.usermapped_glo`（可执行 IPC trampolines）+ `.usermapped`（只读数据）
- **映射层面**: `arch_phys_map()` 返回段物理地址，VM 用 `VMMF_USER` 标志映射到用户空间
- **访问层面**: 用户态通过 `get_minix_kerninfo()` 获取顶层指针，再按字段访问

### 1.3 IPC 入口向量表的三套机制
- **softint**: `int $VEC` 软中断——兼容所有 x86，最慢
- **sysenter**: Intel 快速系统调用——避免中断开销
- **syscall**: AMD 快速系统调用——现代 x86 标准
- **64-bit 演进**: x86-64 统一使用 `syscall` 指令，不需要用户态 trampoline

### 1.4 redox 对照
- **redox**: 无 usermapped 段——scheme 模型，用户态通过 scheme 请求获取内核信息
- **Minix3**: usermapped 段直接映射——性能优化，避免系统调用开销
- **minix-rs**: 64-bit 采用 sys_getinfo 模型（类似 redox scheme 请求，但保留 Minix3 集中式调用号）

### 1.5 本章不讲什么
- 链接脚本完整分析（见 02-higher-half-kernel.md §2.1）
- `arch_phys_map()` 完整实现（见 07-cross-space-init.md）
- VM 映射机制细节（见 09-vm-boot-protocol.md）
- `sys_getinfo` 子请求全集（见 25-misc-unported.md §2.2）

---

## Ch2: C 源码分析

### 2.1 文件清单
| 文件 | 行数 | 核心内容 |
|------|------|---------|
| `usermapped_data.c` | 15 | 8 个数据结构声明（`__section(".usermapped")`） |
| `arch/i386/usermapped_data_arch.c` | 32 | 3 个 IPC 向量表定义 |
| `arch/i386/usermapped_glo_ipc.S` | 108 | 3×7=21 个 IPC trampoline 函数 |
| `arch/i386/kernel.lds` | 36 | 链接脚本段定义（L24-28） |
| `arch/i386/memory.c:744-806` | 63 | `arch_phys_map()` usermapped 段返回 |
| `include/minix/type.h:104-244` | 141 | 8 个结构体定义 |

### 2.2 8 个数据结构字段语义
- `minix_kerninfo`: 顶层结构（magic + flags + 7 个结构指针）
- `kinfo`: 内核信息（~50 字段，含 memmap/vir_base/proc_count/user_sp 等）
- `machine`: 机器信息（5 字段: processors_count/bsp_id/apic_enabled/acpi_rsdp/board_id）
- `kmessages`: 诊断消息（环形缓冲区 + 80×25 文本缓冲区）
- `loadinfo`: 负载平均（proc_load_history 数组）
- `kuserinfo`: userland ABI（kui_size + kui_user_sp）
- `arm_frclock`: ARM 自由时钟（hz + tcrr 地址）
- `kclockinfo`: 时钟（boottime/uptime/realtime/hz，含 64-bit 保留字段）

### 2.3 IPC trampoline 三套机制
- 栈布局（softint/sysenter/syscall 各不同）
- 寄存器约定（%eax=dest-src, %ebx=msg, %ecx=opcode）
- `proc_stacktrace()` 依赖（需找到 %ebp）

### 2.4 `arch_phys_map()` 映射逻辑
- `usermapped_glo_index`: 返回 `.usermapped_glo` 物理地址 + `VMMF_USER | VMMF_GLO`
- `usermapped_index`: 返回 `.usermapped` 物理地址 + `VMMF_USER`
- VM 调用此函数获取段信息，然后映射到用户进程地址空间

---

## Ch3: 设计决策（64-bit 重写）

### 3.1 D1: 不保留 `.usermapped` 段
- **C**: 链接脚本定义专用段 + VM 映射到用户空间
- **Rust 64-bit**: 不定义此段；数据通过 `sys_getinfo` 获取
- **理由**: 64-bit 使用 `syscall` 指令直接入内核，不需要用户态 IPC trampoline；数据结构布局不泄漏到用户态 ABI

### 3.2 D2: KernelInfo (boot→kernel) 保留
- **C**: `kinfo` 结构既用于 boot→kernel 传递，又通过 usermapped 暴露给用户态
- **Rust 64-bit**: `KernelInfo`（`os/libs/minix-boot/src/kernel_info.rs`）仅用于 boot-shim → kernel 传递
- **理由**: boot 阶段尚无系统调用机制，必须用共享内存

### 3.3 D3: `kclockinfo` 改为 `ClockState` 内部字段
- **C**: `kclockinfo` 全局变量，usermapped 暴露
- **Rust 64-bit**: `ClockState` 结构体字段（`clock.rs:673`），通过 `get_monotonic()` 等函数访问
- **理由**: 封装性；避免全局可变状态；函数访问允许添加验证逻辑

### 3.4 D4: `minix_kerninfo` 顶层结构不保留
- **C**: 用户态通过 `get_minix_kerninfo()` 获取顶层指针，再按字段访问
- **Rust 64-bit**: 无统一"内核信息页"；各信息独立获取
- **理由**: 64-bit 无 usermapped 段，不需要顶层指针结构

### 3.5 D5: IPC 入口向量表不保留
- **C**: 3 套向量表（softint/sysenter/syscall）+ 21 个 trampoline 函数
- **Rust 64-bit**: 统一 `syscall` 指令入内核
- **理由**: 64-bit 指令集统一；不需要兼容 32-bit 多入口机制

---

## Ch4: Rust 实现

### 4.1 已有实现
- `KernelInfo`（`os/libs/minix-boot/src/kernel_info.rs`）: boot→kernel 信息传递
- `ClockState`（`os/kernel/src/clock.rs:673`）: kclockinfo 内部化
- `LoadInfoStruct`（`os/kernel/src/misc.rs`）: GET_LOADINFO 子请求

### 4.2 不实现（WONTFIX）
- `.usermapped` section 链接脚本定义
- 8 个数据结构的 usermapped 声明
- 3 个 IPC 向量表
- 21 个 IPC trampoline 汇编函数
- `arch_phys_map()` usermapped 段返回逻辑

### 4.3 替代方案
- `sys_getinfo` 系统调用（见 25-misc-unported.md）
- `KernelInfo` boot→kernel 传递（见 01-boot-shim-bootstrap.md）

---

## Ch5: 测试

### 5.1 已有测试
- `ClockState` 字段访问测试（clock.rs）
- `KernelInfo` boot 传递测试（boot_integration.rs）

### 5.2 不需要测试（WONTFIX 项）
- usermapped 段映射测试
- IPC trampoline 功能测试

---

## Ch6: 跨文档引用

- [02-higher-half-kernel.md](02-higher-half-kernel.md) §2.1: 链接脚本段定义
- [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md): kinfo 结构初始化
- [07-cross-space-init.md](07-cross-space-init.md): arch_phys_map() 机制
- [09-vm-boot-protocol.md](09-vm-boot-protocol.md): VM 映射建立
- [13-syscall-dispatch.md](13-syscall-dispatch.md): sys_getinfo 系统调用
- [15-clock-timer.md](15-clock-timer.md): kclockinfo → ClockState
- [25-misc-unported.md](25-misc-unported.md): GET_KINFO / GET_MACHINE 等子请求
