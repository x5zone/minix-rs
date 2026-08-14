# 28-usermapped-data: 用户可见内核数据（.usermapped 段机制）

> **分类**: 架构机制（WONTFIX 文档化）
> **源码**: `minix3/minix/kernel/usermapped_data.c`, `minix3/minix/kernel/arch/i386/usermapped_data_arch.c`, `minix3/minix/kernel/arch/i386/usermapped_glo_ipc.S`, `minix3/minix/kernel/arch/i386/kernel.lds`, `minix3/minix/kernel/arch/i386/memory.c:744-806`
> **关联 Rust**: `os/libs/minix-boot/src/kernel_info.rs`（KernelInfo，boot→kernel）, `os/kernel/src/clock.rs`（ClockState，kclockinfo 内部化）
> **前置**: [02-higher-half-kernel.md](02-higher-half-kernel.md), [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md), [07-cross-space-init.md](07-cross-space-init.md), [09-vm-boot-protocol.md](09-vm-boot-protocol.md)
> **C 总行数**: ~295 行（含链接脚本 + 汇编 + C 声明）

---

## Ch1: 概念

**核心问题**: 内核拥有大量信息（时钟、CPU、负载、诊断消息等），用户态进程需要频繁读取。每次系统调用都有开销（上下文切换 + 寄存器保存/恢复）。内核如何在不牺牲安全性的前提下，让用户态高效读取这些只读信息？

这是地址空间隔离与信息共享的根本权衡。CPU 提供了页表权限位（user/supervisor）让内核选择哪些页面在用户态可见。Minix3 利用这一机制，将一组只读内核数据结构放在专用链接段（`.usermapped`），由 VM 映射到每个进程的地址空间，用户态直接指针读取——无需系统调用。

### 1.1 两种信息暴露策略

| 策略 | 机制 | 优点 | 缺点 |
|------|------|------|------|
| **共享内存映射** | 内核数据放专用段，VM 映射到用户空间 | 零系统调用开销；随机访问 | 内核数据布局泄漏到用户态 ABI；映射管理复杂 |
| **系统调用获取** | 用户态 `sys_getinfo` 请求，内核拷贝到用户缓冲区 | 布局封装；权限可控 | 每次访问系统调用开销；不适合高频读取 |

Minix3 32-bit 采用**共享内存映射**策略；minix-rs 64-bit 采用**系统调用获取**策略。这不是简单的优劣选择，而是 32-bit→64-bit 架构演进中的重新权衡：

- **32-bit 时代**: 系统调用开销相对较高（`int $0x80` 软中断），共享内存避免频繁中断
- **64-bit 时代**: `syscall` 指令开销极低（~10ns），且 64-bit 地址空间管理更复杂，共享内存映射的复杂度收益比下降

### 1.2 `.usermapped` section 的架构角色

**链接器层面**（[kernel.lds:24-28](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/kernel.lds)）：

```ld
. = ALIGN(4096); usermapped_start = .;
.usermapped_glo : AT(ADDR(.usermapped_glo) - _kern_offset) { *(.usermapped_glo) }
. = ALIGN(4096); usermapped_nonglo_start = .;
.usermapped : AT(ADDR(.usermapped) - _kern_offset) { *(.usermapped) }
. = ALIGN(4096); usermapped_end = .;
```

两个段：
- `.usermapped_glo`（global, executable）: IPC trampoline 汇编代码，映射为用户可执行
- `.usermapped`（data, read-only）: 8 个内核数据结构，映射为用户可读

段位于内核镜像起始（unpaged 段之后，`.text` 之前），4KB 对齐确保页表粒度。

**映射层面**（[arch/i386/memory.c:746-806](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/memory.c)）：`arch_phys_map()` 函数返回段的物理地址和长度，VM 调用此函数后用 `VMMF_USER` 标志映射到每个进程的用户地址空间。

**访问层面**：用户态通过 `get_minix_kerninfo()`（[lib/libc/sys/kernel_utils.c:26](file:///home/xzhao/github/minix-rs/minix3/minix/lib/libc/sys/kernel_utils.c)）获取顶层 `struct minix_kerninfo *` 指针，再按字段访问其他结构。

### 1.3 IPC 入口向量表的三套机制

Minix3 32-bit 提供 3 种 IPC 入口机制，对应不同 x86 指令：

| 向量表 | 入口指令 | 优点 | 缺点 |
|--------|---------|------|------|
| `minix_ipcvecs_softint` | `int $VEC` 软中断 | 兼容所有 x86 | 最慢（中断门开销） |
| `minix_ipcvecs_sysenter` | `sysenter` | Intel 快速系统调用 | 仅 Intel；栈管理复杂 |
| `minix_ipcvecs_syscall` | `syscall` | AMD 快速系统调用 | 仅 AMD K7+ |

每表 7 个函数指针（send / receive / sendrec / sendnb / notify / do_kernel_call / senda），指向 `.usermapped_glo` 段中的 trampoline 汇编函数。用户态调用 IPC 时，函数指针跳转到 trampoline，trampoline 用对应指令陷入内核。

**64-bit 演进**: x86-64 统一使用 `syscall` 指令直接入内核，不需要用户态 trampoline 跳板。aarch64 使用 `hvc`/`smc`，riscv64 使用 `ecall`——都是单指令入内核，无需多入口机制。

### 1.4 redox 对照

不同内核对"信息暴露"的策略差异反映其架构哲学：

- **redox**: 无 usermapped 段——scheme 模型，用户态通过 scheme 请求获取内核信息。内核最简，信息暴露责任下推到用户态 scheme。
- **Minix3 32-bit**: usermapped 段直接映射——性能优化，避免系统调用开销。内核主动暴露数据，用户态直接读取。
- **minix-rs 64-bit**: 采用 `sys_getinfo` 系统调用模型（类似 redox scheme 请求，但保留 Minix3 集中式调用号）。在安全性与性能之间重新权衡。

### 1.5 本章不讲什么

- 链接脚本完整分析（见 [02-higher-half-kernel.md](02-higher-half-kernel.md) §2.1）
- `arch_phys_map()` 完整实现（见 [07-cross-space-init.md](07-cross-space-init.md)）
- VM 映射机制细节（见 [09-vm-boot-protocol.md](09-vm-boot-protocol.md)）
- `sys_getinfo` 子请求全集（见 [25-misc-unported.md](25-misc-unported.md) §2.2）
- 时钟机制（见 [15-clock-timer.md](15-clock-timer.md)）

---

## Ch2: C 源码分析

### 2.1 文件清单

| 文件 | 行数 | 核心内容 |
|------|------|---------|
| [usermapped_data.c](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/usermapped_data.c) | 15 | 8 个数据结构声明（`__section(".usermapped")`） |
| [arch/i386/usermapped_data_arch.c](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/usermapped_data_arch.c) | 33 | 3 个 IPC 向量表定义 |
| [arch/i386/usermapped_glo_ipc.S](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/usermapped_glo_ipc.S) | 108 | 3×7=21 个 IPC trampoline 函数 |
| [arch/i386/kernel.lds](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/kernel.lds) | 37 | 链接脚本段定义（L24-28） |
| [arch/i386/memory.c:744-806](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/memory.c) | 63 | `arch_phys_map()` usermapped 段返回 |
| [include/minix/type.h:98-244](file:///home/xzhao/github/minix-rs/minix3/minix/include/minix/type.h) | 147 | §2.2 的 7 个结构体定义（`kinfo` 在 param.h；区内另夹 io_range/minix_mem_range/boot_image/memory/k_randomness 5 个辅助结构体） |
| [include/minix/param.h:14-47](file:///home/xzhao/github/minix-rs/minix3/minix/include/minix/param.h) | 34 | `struct kinfo` 定义 |

### 2.2 8 个用户可见数据结构

[usermapped_data.c](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/usermapped_data.c) 声明 8 个全局变量，全部用 `__section(".usermapped")` 属性放入用户映射段：

```c
struct minix_kerninfo minix_kerninfo __section(".usermapped");
struct kinfo kinfo __section(".usermapped");
struct machine machine __section(".usermapped");
struct kmessages kmessages __section(".usermapped");
struct loadinfo loadinfo __section(".usermapped");
struct kuserinfo kuserinfo __section(".usermapped");
struct arm_frclock arm_frclock __section(".usermapped");
struct kclockinfo kclockinfo __section(".usermapped");
```

**各结构字段语义**：

| 结构 | C 定义位置 | 关键字段 | userland ABI | 用途 |
|------|-----------|---------|-------------|------|
| `minix_kerninfo` | type.h:214 | kerninfo_magic(0xfc3b84bf) + ki_flags + 7 个结构指针 | 部分（kuserinfo/ipcvecs） | 顶层入口，用户态通过它找到其他结构 |
| `kinfo` | param.h:14 | mbi + memmap[] + freepde_start + user_sp + nr_procs/nr_tasks + vir_kern_start | ❌ NOT userland ABI（legacy user_sp 例外） | 内核启动信息，boot→kernel 传递 |
| `machine` | type.h:122 | processors_count + bsp_id + apic_enabled + acpi_rsdp + board_id | ❌ NOT userland ABI | 机器硬件信息 |
| `kmessages` | type.h:170 | km_next + km_size + km_buf[] + kmess_buf[80*25] | ❌ NOT userland ABI | 内核诊断消息环形缓冲区 |
| `loadinfo` | type.h:98 | proc_load_history[] + proc_last_slot + last_clock | ❌ NOT userland ABI | 系统负载平均值 |
| `kuserinfo` | type.h:205 | kui_size + kui_user_sp | ✅ userland ABI | 用户态 ABI（栈顶地址） |
| `arm_frclock` | type.h:197 | hz + tcrr | ❌ NOT userland ABI | ARM 自由运行时钟（32-bit ARM 专用） |
| `kclockinfo` | type.h:104 | boottime + uptime + realtime + hz（含 64-bit 保留字段） | ❌ NOT userland ABI（volatile） | 时钟信息 |

**`minix_kerninfo` 顶层结构详解**（[type.h:214-244](file:///home/xzhao/github/minix-rs/minix3/minix/include/minix/type.h)）：

```c
struct minix_kerninfo {
    u32_t kerninfo_magic;           // 0xfc3b84bf 魔数验证
    u32_t minix_feature_flags;      // 内核特性标志
    u32_t ki_flags;                 // 哪些指针有效
    u32_t flags_unused2/3/4;
    struct kinfo        *kinfo;     // 内核信息（NOT userland ABI）
    struct machine      *machine;   // 机器信息（NOT userland ABI）
    struct kmessages    *kmessages; // 诊断消息（NOT userland ABI）
    struct loadinfo     *loadinfo;  // 负载信息（NOT userland ABI）
    struct minix_ipcvecs *minix_ipcvecs;  // IPC 入口（userland ABI）
    struct kuserinfo    *kuserinfo; // 用户 ABI（userland ABI）
    struct arm_frclock  *arm_frclock; // ARM 时钟（NOT userland ABI）
    volatile struct kclockinfo *kclockinfo; // 时钟（NOT userland ABI）
};
```

`ki_flags` 标志位：
- `MINIX_KIF_IPCVECS` (1L<<0): `minix_ipcvecs` 指针有效
- `MINIX_KIF_USERINFO` (1L<<1): `kuserinfo` 指针有效

用户态用 `KUSERINFO_HAS_FIELD(kui, f)` 宏检查 `kuserinfo` 是否有某字段（基于 `kui_size` 偏移比较）——这是 ABI 向后兼容机制。

### 2.3 IPC trampoline 三套机制

[usermapped_glo_ipc.S](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/usermapped_glo_ipc.S) 用宏 `IPCFUNC(name,SETARGS,VEC,POSTTRAP)` 生成 3×7=21 个 trampoline 函数。以 `send` 为例：

**softint 版本**（最简单）：
```asm
usermapped_send_softint:
    push %ebp
    movl %esp, %ebp
    push %ebx
    movl 8(%ebp), %eax    ; eax = dest-src
    movl 12(%ebp), %ebx   ; ebx = message pointer
    movl $SEND, %ecx      ; ecx = opcode
    int  $IPCVEC_UM       ; 软中断陷入内核
    mov  %ebx, %ecx       ; 保存 %ebx
    pop  %ebx
    pop  %ebp
    ret
```

**sysenter 版本**（Intel 快速调用，栈管理复杂）：
- 额外 push 第二个 %ebp（调用者 ebp，供 proc_stacktrace 栈回溯）+ %edx/%esi/%edi
- `%esi` 保存恢复后的 %esp，`%edx` 保存返回 %eip
- `movl $0f, %edx` 设置返回标签
- `sysenter` 陷入内核，返回到 `0:` 标签

**syscall 版本**（AMD 快速调用，类似 sysenter）：
- 寄存器约定相同，但用 `syscall` 指令
- `%ecx` 被 SYSCALL 指令破坏，需 `movl %ecx, %edx` 备份

> **栈布局依赖**: 注释指出 `proc_stacktrace()` 依赖此栈布局找到 `%ebp`——修改栈布局会破坏栈回溯功能。

### 2.4 `arch_phys_map()` 映射逻辑

[arch/i386/memory.c:746-806](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/memory.c) 的 `arch_phys_map()` 函数返回 usermapped 段的物理地址和标志：

```c
if(index == usermapped_glo_index) {
    *addr = vir2phys(&usermapped_start);
    *len = glo_len;  // usermapped_nonglo_start - usermapped_start
    *flags = VMMF_USER | VMMF_GLO;  // 用户可读 + 全局可执行
    return OK;
}
else if(index == usermapped_index) {
    *addr = vir2phys(&usermapped_nonglo_start);
    *len = &usermapped_end - &usermapped_nonglo_start;
    *flags = VMMF_USER;  // 用户可读
    return OK;
}
```

VM 调用此函数枚举所有需要映射的段，然后在进程地址空间建立映射。`VMMF_USER` 使 PTE 置用户位（pagetable.c:1206）；`VMMF_GLO` 进一步置 PTE **Global 位**（`I386_VM_GLOBAL`，pagetable.c:1219）——TLB 全局页，CR3 切换/进程切换时不被冲刷。usermapped_glo 段映射到所有进程（同一物理页 + 同一虚拟地址），配合 Global 位避免频繁 TLB 失效。

---

## Ch3: 设计决策（64-bit 重写）

### 3.1 D1: 64-bit 重写不保留 `.usermapped` 段

**ARCH: Architectural Evolution** — 从"直接内存映射"到"系统调用获取"

**C 行为**: kernel.lds 定义 `.usermapped_glo` + `.usermapped` 段；`arch_phys_map()` 返回物理地址；VM 映射到用户空间。

**Rust 64-bit 决策**: 不定义此段；不实现 `arch_phys_map()` usermapped 分支；数据通过 `sys_getinfo` 获取。

**理由**:
1. 64-bit 使用 `syscall` 指令直接入内核，不需要用户态 IPC trampoline（`.usermapped_glo` 段废弃）
2. 数据结构布局不泄漏到用户态 ABI（安全性提升）
3. 简化地址空间管理（VM 不需要为每个进程映射 usermapped 段）
4. 64-bit `syscall` 指令开销极低（~10ns），共享内存的性能优势不再显著

**影响**: 用户态从直接指针读取改为系统调用获取，性能略降但安全性提升。这与 [02-higher-half-kernel.md:116](02-higher-half-kernel.md) 的声明一致："后者是 Minix3 用户态段共享机制（USMAPPED 宏），在 64-bit 重写中已废弃"。

### 3.2 D2: KernelInfo (boot→kernel) 保留并增强

**C 行为**: `kinfo` 结构既用于 boot→kernel 传递（pre_init.c 填充），又通过 usermapped 暴露给用户态。

**Rust 64-bit 决策**: `KernelInfo`（[os/libs/minix-boot/src/kernel_info.rs:13](file:///home/xzhao/github/minix-rs/os/libs/minix-boot/src/kernel_info.rs)）仅用于 boot-shim → kernel 传递，不暴露给用户态。

**理由**: boot 阶段尚无系统调用机制，必须用共享内存；boot→kernel 是受控环境（同一二进制或已知兼容的二进制），安全性可保证。

**字段差异**: Rust `KernelInfo` 精简为 12 字段（C `kinfo` 有 ~33 字段），去除 multiboot 特定字段（`mbi`/`module_list`/`memmap[]`）和 usermapped 相关字段，保留 `memmap`/`kern_virt_base`/`kern_phys_base`/`kern_size`/`free_upper_idx`/`user_sp`/`kern_stack_top`/`syscall_entry`/`boot_modules`/`bootstrap_start`/`bootstrap_len`/`platform_sources`。

### 3.3 D3: `kclockinfo` 改为 `ClockState` 内部字段

**C 行为**: `struct kclockinfo kclockinfo __section(".usermapped")` 全局变量，用户态直接读取 `kclockinfo.uptime`。

**Rust 64-bit 决策**: `ClockState` 结构体字段（[os/kernel/src/clock.rs:708](file:///home/xzhao/github/minix-rs/os/kernel/src/clock.rs)），通过 `get_monotonic()` / `get_realtime()` / `get_boottime()` 函数访问。

**理由**:
1. 封装性——内部字段可添加验证逻辑
2. 避免全局可变状态——`ClockState` 实例化后受所有权约束
3. 不泄漏布局——用户态不依赖 `kclockinfo` 字段偏移

**访问路径**: 用户态通过 `sys_getinfo` GET_HZ / GET_TIME 等子请求间接获取（见 [25-misc-unported.md](25-misc-unported.md)）。

### 3.4 D4: `minix_kerninfo` 顶层结构不保留

**C 行为**: `struct minix_kerninfo minix_kerninfo __section(".usermapped")` 含 magic + flags + 7 个结构指针，作为用户态访问内核信息的顶层入口。

**Rust 64-bit 决策**: 无统一"内核信息页"概念；各信息独立获取。

**理由**: 64-bit 无 usermapped 段，不需要顶层指针结构作为用户态入口。

**替代**: 用户态通过 `sys_getinfo` 各子请求独立获取（GET_KINFO / GET_MACHINE / GET_LOADINFO 等，见 [25-misc-unported.md](25-misc-unported.md) §2.2）。

### 3.5 D5: IPC 入口向量表不保留

**ARCH: Architectural Evolution** — 从"多入口 + 用户态 trampoline"到"单入口 + 内核直接处理"

**C 行为**: 3 套向量表（softint/sysenter/syscall）+ 21 个 trampoline 函数（[usermapped_glo_ipc.S](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/usermapped_glo_ipc.S)）。

**Rust 64-bit 决策**: 统一 `syscall` 指令入内核。

**理由**:
1. 64-bit 指令集统一使用 `syscall`（x86-64）/ `ecall`（riscv64）/ `hvc`（aarch64）
2. 不需要兼容 32-bit 的 softint/sysenter 多入口机制
3. 内核直接处理系统调用，不需要用户态 trampoline 跳板

**影响**: 用户态 IPC 调用从"函数指针 → trampoline → int/sysenter/syscall"简化为"syscall 指令直接入内核"。

### 3.6 D6: `kuserinfo` userland ABI 不保留

**C 行为**: `struct kuserinfo kuserinfo __section(".usermapped")` 含 `kui_size` + `kui_user_sp`，是 userland ABI，用户态通过 `minix_get_user_sp()` 读取。

**Rust 64-bit 决策**: 用户栈顶通过 `KernelInfo.user_sp`（boot→kernel）+ `sys_getinfo` GET_KINFO（用户态查询）获取。

**理由**: minix-rs 是完整重写，无 legacy binary 兼容需求；不需要 `kui_size` ABI 兼容性检查机制。

---

## Ch4: Rust 实现

### 4.1 已有实现

#### KernelInfo（boot→kernel 信息传递）

[os/libs/minix-boot/src/kernel_info.rs:13](file:///home/xzhao/github/minix-rs/os/libs/minix-boot/src/kernel_info.rs) 定义 `KernelInfo` 结构体，对应 C `kinfo` 的 boot→kernel 用途：

```rust
pub struct KernelInfo {
    pub memmap: &'static [MemoryRegion],
    pub kern_virt_base: VirBytes,
    pub kern_phys_base: PhysBytes,
    pub kern_size: u64,
    pub free_upper_idx: Option<usize>,
    pub user_sp: VirBytes,
    pub kern_stack_top: VirBytes,
    pub syscall_entry: VirBytes,
    pub boot_modules: &'static [BootModule],
    pub bootstrap_start: PhysBytes,
    pub bootstrap_len: u64,
    pub platform_sources: &'static [PlatformDescSource],
}
```

这与 C `kinfo` 的关系：保留 boot→kernel 传递所需的 12 字段，去除 usermapped 暴露所需的字段（`kmessages`/`do_serial_debug`/`minix_panicing` 等）。

#### ClockState（kclockinfo 内部化）

[os/kernel/src/clock.rs:708](file:///home/xzhao/github/minix-rs/os/kernel/src/clock.rs) 定义 `ClockState` 结构体，对应 C `kclockinfo` + `kloadinfo` + `clock_timers`：

```rust
struct ClockState {
    hz: u32,                    // C: kclockinfo.hz
    uptime: u64,                // C: kclockinfo.uptime (BSP only)
    realtime: u64,              // C: kclockinfo.realtime (BSP only)
    boottime: u64,              // C: kclockinfo.boottime (BSP only)
    loadinfo: LoadInfo,         // C: kloadinfo (all CPUs)
    // ...（另有 cpu_id/is_bsp/adjtime_delta/timers）
}
```

非 `Atomic`——`ClockState` 是 per-CPU 实例（`cpu_id` 字段），全局时钟字段仅 BSP 实例写、无跨 CPU 竞争。用户态可读副本在静态 `AtomicU64`：`CLOCK_UPTIME`/`CLOCK_REALTIME`/`CLOCK_BOOTTIME`（clock.rs:46/53/60），任何上下文（BKL 内外）只读原子。

用户态访问通过函数封装：`get_monotonic()` / `get_realtime()` / `get_boottime()`（clock.rs:77/85/93），而非直接读取全局变量。

#### LoadInfoStruct（GET_LOADINFO 子请求）

[os/kernel/src/misc.rs](file:///home/xzhao/github/minix-rs/os/kernel/src/misc.rs) 定义 `LoadInfoStruct`，用于 `sys_getinfo` GET_LOADINFO 子请求返回给用户态。

### 4.2 不实现（WONTFIX）

以下 C 符号在 64-bit 重写中**不实现**，由 `sys_getinfo` 系统调用替代：

| C 符号 | C 位置 | 替代方案 |
|--------|--------|---------|
| `.usermapped` section | kernel.lds:27 | 不保留 |
| `.usermapped_glo` section | kernel.lds:25 | 不保留 |
| `minix_kerninfo` | usermapped_data.c:4 | 无统一入口；sys_getinfo 各子请求 |
| `kinfo`（usermapped 用途） | usermapped_data.c:7 | KernelInfo（boot→kernel）+ sys_getinfo GET_KINFO |
| `machine` | usermapped_data.c:8 | sys_getinfo GET_MACHINE / GET_CPUINFO |
| `kmessages` | usermapped_data.c:9 | 无替代（future work） |
| `loadinfo`（usermapped 用途） | usermapped_data.c:10 | sys_getinfo GET_LOADINFO |
| `kuserinfo` | usermapped_data.c:11 | KernelInfo.user_sp + sys_getinfo GET_KINFO |
| `arm_frclock` | usermapped_data.c:13 | ARM 32-bit 专用，不保留 |
| `kclockinfo`（usermapped 用途） | usermapped_data.c:15 | ClockState 内部字段 + sys_getinfo GET_HZ |
| `minix_ipcvecs_softint` | usermapped_data_arch.c:4 | 64-bit syscall 指令 |
| `minix_ipcvecs_sysenter` | usermapped_data_arch.c:14 | 64-bit syscall 指令 |
| `minix_ipcvecs_syscall` | usermapped_data_arch.c:24 | 64-bit syscall 指令（直接，无 trampoline） |
| 21 个 trampoline 函数 | usermapped_glo_ipc.S | 64-bit syscall/ecall/hvc 直接入内核 |
| `arch_phys_map()` usermapped 分支 | memory.c:794-806 | 不保留 |
| `get_minix_kerninfo()` | lib/libc/sys/kernel_utils.c:26 | 不保留 |

### 4.3 替代方案对照

| 用户态需求 | C 32-bit 路径 | Rust 64-bit 路径 |
|-----------|--------------|-----------------|
| 获取内核信息 | `get_minix_kerninfo()->kinfo` 直接读 | `sys_getinfo` GET_KINFO |
| 获取机器信息 | `get_minix_kerninfo()->machine` 直接读 | `sys_getinfo` GET_MACHINE / GET_CPUINFO |
| 获取负载信息 | `get_minix_kerninfo()->loadinfo` 直接读 | `sys_getinfo` GET_LOADINFO |
| 获取时钟频率 | `get_minix_kerninfo()->kclockinfo->hz` 直接读 | `sys_getinfo` GET_HZ |
| 获取用户栈顶 | `minix_get_user_sp()` 读 kuserinfo | `sys_getinfo` GET_KINFO 返回 user_sp |
| 发送 IPC 消息 | `minix_ipcvecs->send(...)` → trampoline → int | `syscall` 指令直接入内核 |

---

## Ch5: 测试

### 5.1 已有测试

| 测试 | 位置 | 覆盖内容 |
|------|------|---------|
| `ClockState` 字段访问 | `os/kernel/src/clock.rs` tests | kclockinfo 内部化后的读写 |
| `KernelInfo` boot 传递 | `os/kernel/tests/boot_integration.rs` | boot→kernel 信息传递 |
| `LoadInfoStruct` 序列化 | `os/kernel/src/misc.rs` tests | GET_LOADINFO 子请求 |

### 5.2 不需要测试（WONTFIX 项）

- `.usermapped` 段映射测试（段不保留）
- IPC trampoline 功能测试（trampoline 不保留）
- `get_minix_kerninfo()` 测试（函数不保留）

---

## Ch6: 跨文档引用

### 6.1 前序引用

- [02-higher-half-kernel.md](02-higher-half-kernel.md) §2.1: 链接脚本 `.usermapped` 段定义（L116 声明"已废弃"）
- [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md): `kinfo` 结构在 boot 阶段初始化
- [07-cross-space-init.md](07-cross-space-init.md): `arch_phys_map()` 机制（usermapped 段映射入口）
- [09-vm-boot-protocol.md](09-vm-boot-protocol.md): VM 启动时建立 usermapped 映射

### 6.2 后续引用

- [13-syscall-dispatch.md](13-syscall-dispatch.md): `sys_getinfo` 系统调用（64-bit 替代方案）
- [15-clock-timer.md](15-clock-timer.md): `kclockinfo` → `ClockState` 内部化
- [25-misc-unported.md](25-misc-unported.md) §2.2: GET_KINFO / GET_MACHINE / GET_LOADINFO 等子请求

---

## 附录 A: 结构体字段全集

### A.1 `struct kinfo`（param.h:14-47）

| 字段 | 类型 | 用途 | Rust 对应 |
|------|------|------|----------|
| `mbi` | `multiboot_info_t` | Multiboot 信息 | ❌ 不保留（UEFI 直接提供） |
| `module_list[]` | `multiboot_module_t[]` | Boot 模块列表 | `KernelInfo.boot_modules` |
| `memmap[]` | `multiboot_memory_map_t[]` | 自由内存映射 | `KernelInfo.memmap` |
| `freepde_start` | `int` | 空闲 PDE 起始 | `KernelInfo.free_upper_idx` |
| `user_sp` | `vir_bytes` | 用户栈顶 | `KernelInfo.user_sp` |
| `vir_kern_start` | `vir_bytes` | 内核虚拟地址起始 | `KernelInfo.kern_virt_base` |
| `bootstrap_start/len` | `vir_bytes` | Bootstrap 段 | `KernelInfo.bootstrap_start/len` |
| `nr_procs` | `int` | 用户进程数 | ❌ 通过 sys_getinfo GET_KINFO |
| `nr_tasks` | `int` | 内核任务数 | ❌ 通过 sys_getinfo GET_KINFO |
| `kmessages` | `struct kmessages *` | 诊断消息指针 | ❌ 不保留 |
| 其余字段 | — | serial debug / panic / release 等 | ❌ 不保留 |

### A.2 `struct machine`（type.h:122-131）

| 字段 | 类型 | 用途 | Rust 对应 |
|------|------|------|----------|
| `processors_count` | `unsigned` | CPU 数 | ❌ 通过 sys_getinfo GET_CPUINFO |
| `bsp_id` | `unsigned` | BSP CPU ID | ❌ 通过 sys_getinfo GET_CPUINFO |
| `apic_enabled` | `int` | APIC 是否启用 | ❌ 不保留（64-bit 默认 APIC） |
| `acpi_rsdp` | `phys_bytes` | ACPI RSDP 地址 | `KernelInfo.platform_sources` |
| `board_id` | `unsigned int` | 板 ID | ❌ 通过 sys_getinfo GET_MACHINE |
