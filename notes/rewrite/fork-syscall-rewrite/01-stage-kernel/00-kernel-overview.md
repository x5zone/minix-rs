# 00-kernel-overview: Kernel 整体架构概览

> **分类**: Kernel 整体层级
> **源码**: `minix3/minix/kernel/`（34 个 .c 文件，~13,610 行 C）+ `minix3/minix/kernel/arch/i386/`
> **说明**: 内核是什么、怎么启动、怎么运行——一个面向新读者的入口文档

---

## 1. Kernel 是什么

### 1.1 先从一个类比开始

Kernel 和 VM/PM/VFS 不同。VM/PM/VFS 是用户态服务——它们像 Web Server 一样，在事件循环里等待 IPC 消息、分派 handler、返回响应。Kernel 不能这么类比。

Kernel 是**操作系统中最先运行、最后停止的程序**。它没有"接收请求→处理→响应"的主循环——它本身就是调度器，负责决定谁该运行、谁该等待。更合适的类比是：

> Kernel 像一个**舞台的总导演**——它加载布景（页表、GDT/IDT）、安排演员上场顺序（调度）、传递道具（IPC 消息），但自己不在舞台上演出。演出的是用户态服务（VM/PM/VFS）和用户进程。

Kernel 直接操作硬件——从 GRUB 拿 multiboot、建立页表、设置中断控制器、操纵 CR3 寄存器。这些都是 VM（用户态服务器）无权做的事。

### 1.2 Kernel 的核心职责

| 职责 | 说明 |
|------|------|
| **硬件发现** | 从 GRUB/multiboot 解析物理内存 map、内核模块列表 |
| **页表管理** | 建立初始页表、切换地址空间、响应 VM 的 sys_vmctl 请求 |
| **保护与中断** | GDT/IDT/TSS 初始化、异常帧解析、IRQ 转发 |
| **进程调度** | proc 结构体、就绪队列、pick_proc、上下文切换 |
| **IPC 机制** | SEND/RECEIVE/NOTIFY 原语、异步消息 |
| **系统调用路由** | kernel_call_dispatch → do_xxx() → 转发到 PM/VM/VFS |
| **时钟与定时器** | 时钟中断、虚拟定时器、配置文件定时器 |

### 1.3 内核和 VM 的关系

```
Kernel（内核态）                          VM（用户态）
─────                                    ─────
发现物理内存（GRUB → multiboot）    →   拿到 memmap 描述（sys_getkinfo IPC）
建立初始页表（恒等映射）            →   VM 看不到 CR3
捕获页错误 → 转发 VM               →   处理缺页逻辑（CoW / 按需分配）
执行 VM 的 sys_vmctl 请求          ←   请求内核改页表（SET_PDBR / MAP_PHYS）
```

这本质上是两个独立进程之间的协议——Kernel 不替 VM 做决策，VM 不直接操硬件。和单体内核（Linux）不同，页表的管理**跨两个执行主体**。

### 1.4 执行模型

Kernel 是裸机程序。不存在"主循环"——内核代码总是以以下三种方式之一被调用：

1. **异常/中断** → 硬件触发 exception_handler，内核决定响应
2. **系统调用** → 用户态 `sys_call` → kernel_call_dispatch → do_xxx()
3. **进程切换** → 调度器挑选下一个进程 → switch_to_user() 回用户态

### 1.5 内核执行模型约束（Rust 开发必读）

> **本节是 03-stage-kernel 所有文档的共享约束**。开发内核 Rust 代码时，必须遵守以下规则，它们覆盖全局 CLAUDE.md 中的"单线程事件循环"假设。

#### 1.5.1 内核不是单线程用户态 server

VM/PM/VFS 是用户态服务——单线程事件循环，`Rc`/`RefCell`/`!Send`/`!Sync` 安全。**Kernel 不是**。

Minix3 内核支持 SMP（对称多处理），使用 BKL（Big Kernel Lock）自旋锁确保同一时刻只有一个 CPU 执行内核代码（`smp.h:48`）。但 BKL 在以下路径中会被释放，形成并发窗口：

| 释放 BKL 的路径 | 源码位置 | 说明 |
|----------------|---------|------|
| 时钟中断 | `arch_clock.c:92,107,118` | 处理定时器时释放 BKL |
| APIC 中断 | `smp.c:44,86-94` | IPI 处理时释放 BKL |
| IPI 同步等待 | `smp.c:86-94` | 等待其他 CPU 响应时释放 BKL |

**推理**：系统调用处理全程持有 BKL，`cross_space_copy`/`cross_space_memset` 在此范围内执行，因此这些函数内部不需要额外同步。但 BKL 释放窗口内可能有其他 CPU 的中断处理代码访问共享数据。

#### 1.5.2 Rust 类型约束

| 约束 | 说明 | 与 VM/PM/VFS 的区别 |
|------|------|---------------------|
| `Rc`/`RefCell` | 可用，但必须确保不在 BKL 释放窗口内访问 | VM 中无限制 |
| `!Send`/`!Sync` | 可用，但跨 CPU 共享数据必须 `Send`+`Sync` | VM 中无限制 |
| `core::sync::atomic` | 允许使用（`no_std` 兼容） | VM 中通常不需要 |
| `unsafe` | 必然存在（页表操作、寄存器读写），必须加 SAFETY 注释 | VM 中极少 |
| `AssumeSyncCell` | **禁止**用于跨 CPU 共享数据 | VM 中可用 |

#### 1.5.3 硬件操作约束

| 约束 | 说明 |
|------|------|
| 所有硬件操作必须通过 trait | CR3/PTE 位操作、`#[cfg(target_arch)]` 行为选择 → **禁止** |
| `unsafe` 块必须有 SAFETY 注释 | 说明为什么操作安全、什么不变量被维护 |
| 缺页不是信号 | 内核中缺页 = VMSUSPEND，不是 `SIGSEGV` |
| 错误码对齐 Minix3 | `VmCopyError` 必须映射到 Minix3 errno，禁止自创错误码 |

#### 1.5.4 与 VM/PM/VFS 开发的关键差异速查

| 维度 | VM/PM/VFS | Kernel |
|------|-----------|--------|
| 执行模型 | 单线程事件循环 | BKL + SMP + 中断 |
| 并发 | 无 | BKL 释放窗口内有并发 |
| 内存安全 | 进程隔离 | 直接操作物理内存 |
| `unsafe` | 极少 | 必然存在 |
| 硬件 | 通过 IPC 请求内核 | 直接操作（必须 trait 抽象） |
| 错误处理 | `Result` + IPC 回复 | `Result` + VMSUSPEND + kernel panic |
| `no_std` | 是 | 是，允许 `core::sync::atomic` |
| 测试 | 用户态单元测试 | `#[cfg(test)]` + mock trait |

---

## 2. 内核的启动线：从 GRUB 到第一个用户进程

Kernel 的所有内容可以沿时间线串起——**从 GRUB 交出控制权到系统可以调度用户进程**。

```
GRUB 跳转到 cstart(magic, ebx)
  │
  ▼  pre_init.c (243行)
阶段 1: 硬件发现
  ├── get_parameters(ebx) → kinfo.memmap[]     ← 解析物理内存布局
  ├── cut_memmap() → 扣除内核自身 + boot模块
  └── add_memmap() → 标记可用区域
  │
  ▼  pg_utils.c (317行)
阶段 2: 页表
  ├── pg_identity(cbi) → 4MB 大页恒等映射
  ├── pg_mapkernel() → 映射内核到高地址
  ├── pg_load() → CR3 加载页目录
  └── vm_enable_paging() → CR0.PG=1，开分页
  │
  ▼  protect.c (456行)
阶段 3: 保护模式
  ├── prot_init() → GDT/IDT/TSS
  ├── idt_init() → 异常向量
  └── arch_boot_proc() → 为 boot_proc 加载二进制
  │
  ▼  main.c (522行)
阶段 4: 内核主流程
  ├── kmain() → 初始化中断控制器、时钟
  ├── bsp_finish_booting() → 取消 RTS_PROC_STOP，启动所有进程
  ├── announce() → 打印系统横幅
  └── switch_to_user() ← 进入调度循环
  │
  ▼  proc.c 调度部分
阶段 5: 运行中
  ├── pick_proc() → 选进程
  ├── context_switch() → 切上下文
  └── 回到阶段 5（永远循环）
```

**这就是内核的主线叙事。** Kkernel 没有 VM 那种"先学 fork 再回头补自举"的转折——开机过程本身是严格线性的。

---

## 3. 文档导航：01~30 的叙事逻辑

> **修订说明 (2026-06-13)**: 本节早期版本引用了一组旧文件名（`03-vm-request.md`、`04-protection.md`、`05-exception-interrupt.md` 等），这些文件已重命名为现行的 25 篇文档。下表已与实际文档对齐；旧版误报（旧文件名引用）已标注并保留链接以便回溯。
>
> **修订说明 (2026-06-20)**: 新增 `04-platform-discovery.md`（平台硬件发现抽象），原 `04~24` 顺延为 `05~25`。本次新增把"硬件参数从何而来"这一前置问题独立成章，置于 cstart 时序（03）与硬件接管（05）之间。

> **组织原则（为什么不按子系统分篇）**：本套文档按**读者学习顺序**而非**代码子系统**线性排列。按子系统切割（调度一篇、IPC 一篇、系统调用一篇）对已理解全貌的作者是自然的，但对首次读者会造成四类跳跃：① 前向引用——机制 A 的说明依赖尚未读到的文档 N+X；② 后向依赖——术语（如 RTS 标志）散落多篇，读完整套才能拼出全貌；③ 概念碎片化——`struct proc` 字段分散各处；④ 机制与服务混杂——同一代码在两个上下文解释。因此本套文档遵循三条组织原则：**概念首次出现即完整解释**（后续只引用不复述）、**禁止前向引用**（依赖机制前置）、**每篇覆盖一个读者可独立阅读的语义单元**。各文档开头的前置/边界声明（如 12-ipc-core §前言）即此原则的落地。**正面组织原则**：每篇运行时文档都应能回答"它在 `switch_to_user()` 循环中的哪个位置"——调度、IPC、异常、系统调用都是这个汇聚点（见 §4.1）的组成部分；不在循环中出现机制的，是它被循环中的某一步调用。

### 3.1 阶段 1：引导（01~02）

内核是第一个运行的程序。没人给它 IPC、没人给它内存 map。它自己从 GRUB 的 multiboot 数据结构中解析一切。

| 编号 | 文档 | 覆盖的 C 源文件 | 角色 |
|------|------|---------------|------|
| 01 | [boot-shim-bootstrap](01-boot-shim-bootstrap.md) | pre_init.c + pg_utils.c | GRUB→内存 map→大页恒等映射→开分页；boot-shim 移交控制权到 `arch_boot` |
| 02 | [higher-half-kernel](02-higher-half-kernel.md) | head.S + kernel.lds | 内核 ELF 加载、链接脚本布局、`HigherHalf::jump_to_kmain` 切栈跳转 |

### 3.2 阶段 2：cstart、平台发现与硬件接管（03~05）

进入 `kmain` 后，内核先从 boot-shim 交付的 `KernelInfo` 中发现硬件参数（平台发现），再建立保护模式基础设施并接管硬件中断。

| 编号 | 文档 | 覆盖的 C 源文件 | 角色 |
|------|------|---------------|------|
| 03 | [kmain-cstart](03-kmain-cstart.md) | main.c:115-147, 403-481 + protect.c:321-367 | 从 `kmain()` 入口到保护结构就绪（GDT/TSS/段寄存器） |
| 04 | [platform-discovery](04-platform-discovery.md) | arch_system.c:246-288 (acpi_init) + earm/arch_system.c:101-132 (bsp_init) | 平台硬件发现抽象：`PlatformDesc` trait、DTB/ACPI 数据源、QEMU 兜底、`init_from_kinfo()` 在 T2.5 注入硬件参数 |
| 05 | [clock-interrupt-init](05-clock-interrupt-init.md) | clock.c:48-74 + i8259.c + arch_system.c | `init_clock` + `intr_init` + `arch_init`，让内核响应硬件事件（基址来自 04 的 `PlatformDesc`） |

### 3.3 阶段 3：进程与跨空间（06~07）

硬件就绪后，内核建立进程表并配置跨地址空间运行所需的状态。

| 编号 | 文档 | 覆盖的 C 源文件 | 角色 |
|------|------|---------------|------|
| 06 | [proc-init-boot-proc](06-proc-init-boot-proc.md) | proc.c:119-160 + main.c:157-282 + libexec_load_elf | 清空进程表、遍历 boot image 设置特权、用 minix-elf 加载 VM ELF 到 bootstrap 页表 |
| 07 | [cross-space-init](07-cross-space-init.md) | protect.c:370-377 + memory.c:707-717 | 设置 `ptproc` 与 `freepdes`，为运行时跨地址空间访问铺路 |

### 3.4 阶段 4：系统初始化与 VM 启动（08~09）

| 编号 | 文档 | 覆盖的 C 源文件 | 角色 |
|------|------|---------------|------|
| 08 | [system-init-boot-finish](08-system-init-boot-finish.md) | system.c:168-278 + main.c:38-117 | `system_init` + `bsp_finish_booting` + 内存回收——把内核从"初始化态"带入"运行态" |
| 09 | [vm-boot-protocol](09-vm-boot-protocol.md) | system/do_vmctl.c | VM 与内核的启动协议：`SYS_VMCTL` 请求分发表 |

### 3.5 阶段 5：调度与 IPC（10~12）

调度器和 IPC 是 Minix3 微内核的两条脊柱——所有"运行"和"通信"都依赖它们。

| 编号 | 文档 | 覆盖的 C 源文件 | 角色 |
|------|------|---------------|------|
| 10 | [switch-to-user](10-switch-to-user.md) | proc.c:299-477 + proc.c:176-213 | `switch_to_user` + `idle`——从内核态返回用户态的最后一英里 |
| 11 | [scheduling-primitives](11-scheduling-primitives.md) | proc.c:1595-1832 + proc.c:1893-1910 | `enqueue` / `dequeue` / `pick_proc` / `proc_no_time` 调度原语 |
| 12 | [ipc-core](12-ipc-core.md) | proc.c:599-1590 | `mini_send` / `mini_receive` / `mini_senda` / `do_ipc`——IPC 是内核最复杂的部分（~1000 行） |

### 3.6 阶段 6：系统调用分发与异常（13~14）

| 编号 | 文档 | 覆盖的 C 源文件 | 角色 |
|------|------|---------------|------|
| 13 | [syscall-dispatch](13-syscall-dispatch.md) | system.c:52-167 | `kernel_call_dispatch` + `kernel_call_finish`——`do_xxx()` 分发循环 |
| 14 | [exception-interrupt](14-exception-interrupt.md) | exception.c + interrupt.c + clock.c:70-199 | 异常帧、页错误转发 VM、IRQ 处理 |

### 3.7 阶段 7：时钟、SMP 与系统调用实现（15~21）

`kernel_call_dispatch` 分发的 58 个 syscall 在 15~21 中按主题分组。

| 编号 | 文档 | 覆盖的 C 源文件 | 角色 |
|------|------|---------------|------|
| 15 | [clock-timer](15-clock-timer.md) | clock.c + clock.h | `ClockState`、`TimerAction`、`vtimer_check`——100Hz 时钟驱动调度与闹钟 |
| 16 | [smp](16-smp.md) | smp.c + smp.h + cpulocals.h | BKL、per-CPU 数据、IPI 跨 CPU 调度（当前为单核占位，多核见文档） |
| 17 | [syscall-process](17-syscall-process.md) | do_fork.c + do_exec.c + do_exit.c + do_clear.c + do_runctl.c + do_schedctl.c + do_statectl.c | 进程管理调用（fork/exec/exit/clear/runctl/schedctl/statectl） |
| 18 | [syscall-copy](18-syscall-copy.md) | do_copy.c + do_safecopy.c + do_umap.c + do_umap_remote.c + do_vumap.c + do_memset.c + do_safememset.c | 跨空间拷贝（vircopy/physcopy/umap/memset） |
| 19 | [syscall-signal](19-syscall-signal.md) | do_kill.c + do_getksig.c + do_endksig.c + do_sigsend.c + do_sigreturn.c | 信号调用 |
| 20 | [syscall-device](20-syscall-device.md) | do_irqctl.c + do_devio.c + do_vdevio.c | 设备调用（IRQ 注册、I/O 端口、VDEVIO） |
| 21 | [syscall-clock](21-syscall-clock.md) | do_times.c + do_setalarm.c + do_stime.c + do_settime.c + do_vtimer.c | 时钟调用（times/setalarm/stime/settime/vtimer） |

### 3.8 阶段 8：权限与跨地址空间运行时（22~25）

| 编号 | 文档 | 覆盖的 C 源文件 | 角色 |
|------|------|---------------|------|
| 22 | [privilege](22-privilege.md) | priv.h + system.c:274-540 | `priv` 结构体 + `priv_add_irq/io/mem` + `s_k_call_mask`/`s_ipc_to` 权限位 |
| 23 | [ipc-filter](23-ipc-filter.md) | ipc.h + system.c:540-660 | IPC 过滤：`may_send_to` / `may_receive_from` 调用检查 |
| 24 | [cross-space-runtime](24-cross-space-runtime.md) | syslib.h + do_copy.c + memory.c | 运行时跨空间拷贝——`SYS_DATACOPY` 宏展开为 `sys_vircopy` |
| 25 | [misc-unported](25-misc-unported.md) | do_unused.c + do_getinfo.c + do_trace.c + do_update.c + do_profile.c | 杂项未移植 syscall |

### 3.9 阶段 9：内核基础设施与调试工具（26~30）

> **2026-08-12 新增**: 26-30 覆盖非主线叙事的内核基础设施——watchdog、utility、usermapped-data、debug、profile。这些是内核"正常运行"之外的工具：lockup 检测、错误报告、用户可见数据、调试验证、性能采样。
> **2026-08-13 新增**: 31 覆盖 FPU 子系统——lazy 上下文切换 + #NM 陷阱路径（2026-08-13 Task 1 复扫发现的唯一真实 OS 知识点缺口，详见 [31-fpu-context-switching.md](31-fpu-context-switching.md)）。

| 编号 | 文档 | 覆盖的 C 源文件 | 角色 |
|------|------|---------------|------|
| 26 | [watchdog](26-watchdog.md) | watchdog.c + watchdog.h | NMI Watchdog——内核 lockup 检测（WONTFIX 文档化） |
| 27 | [kernel-utility](27-kernel-utility.md) | utility.c + const.h + do_diagctl.c | 内核工具函数——panic/kputc/_exit |
| 28 | [usermapped-data](28-usermapped-data.md) | usermapped_data.c + arch/i386/usermapped_data_arch.c | `.usermapped` 段机制——用户可见内核数据（WONTFIX：64-bit 不保留） |
| 29 | [kernel-debug](29-kernel-debug.md) | debug.c | 调试基础设施——runqueues_ok/rtsflagstr/BKL timing（Partial+：runqueues_ok/print_proc 已实现，BKL timing 用 debug_assert! 替代） |
| 30 | [kernel-profile](30-kernel-profile.md) | profile.c | 统计 profile——采样时钟 + NMI profiling（Partial+：clock interface + sample collection 已实现，NMI WONTFIX） |
| 31 | [fpu-context-switching](31-fpu-context-switching.md) | arch/i386/arch_system.c（fpu 函数族）+ proc.c（copr_not_available_handler）+ exception.c + mpx.S + do_sigsend.c | FPU 上下文切换——lazy 模型 + CR0.TS/#NM 陷阱路径 + fpu_owner 协议（完整覆盖；FpuTrap 分发已实现，lazy-restore 主体与信号路径保存为显式缺口） |

### 3.10 补充：全局概念

| 编号 | 文档 | 说明 |
|------|------|------|
| 99 | [global-concepts](99-global-concepts.md) | 跨文档共享的全局概念（`RTS` 标志位完整表、`priv` 结构体、常量定义等） |

---

## 4. 和 VM/PM/VFS 的关键差异

### 4.1 没有事件循环

```
VM:    loop { ipc_receive()? → dispatch() → ipc_send() }
PM:    loop { ipc_receive()? → dispatch() → ipc_send() }
VFS:   loop { ipc_receive()? → dispatch() → ipc_send() }

Kernel: 无主循环。代码总是以三种路径之一被调用：
         (1) 硬件中断/异常
         (2) 系统调用 (sys_call → kernel_call_dispatch)
         (3) 进程切换
```

### 4.2 fork 不是主线

```
VM fork:  贯穿 15+ 个文件，上千行，涉及物理内存/页表/CoW/缺页/mmap...
PM fork:  贯穿 ~10 个文件，涉及 slot/signal/endpoint/VM/FVS 协调
Kernel fork:  ~200 行，创建 proc 结构 + 复制寄存器 + 生成 endpoint
```

90% 的 fork 工作量在 VM（页表和 CoW），10% 在 PM（生命周期协调），不到 5% 在 Kernel（proc 槽分配）。这就是为什么内核不需要以 fork 为主线。

### 4.3 没有"鸡生蛋"的自举困境

```
VM:   需要内存 → 但 VM 管理内存 → 先借 kernel 静态内存 → 再建自己的分配器
Kernel: 第一个运行的程序 → GRUB 告诉它一切 → 不需要向任何人借
```

内核的自举是线性的，不需要"先学 X 再回头补 Y"。

---

## 5. 设计原则

### 5.1 内核态 vs 用户态

Kernel 代码在 ring0 运行，可以直接操作所有硬件。用户态服务（VM/PM/VFS）在 ring3。

Rust 实现中，Kernel 代码不需要 `no_std` 约束（它自己是内核）。但需要 trait 抽象来隔离架构特定代码（目前是 x86-64，未来可能 arm64）。

### 5.2 通过 VM 协议的间接操作

Kernel 不独立管理进程的页表。页表的建立、修改由 VM 决策，Kernel 只执行 VM 的 sys_vmctl 请求。这是一个设计上的意图——分离策略（VM）和机制（Kernel）。

### 5.3 中断上下文 vs 进程上下文

```
中断上下文:   异常/IRQ handler → 不能睡眠、不能访问用户内存
进程上下文:   系统调用 → 可以睡眠、可以访问当前进程内存
```

内核大部分 IPC 代码在进程上下文中运行，页错误转发在中断上下文中运行。

---

## 6. 与 02-stage-vm 的关系

本目录应与 [../02-stage-vm](../02-stage-vm/draft/) 交叉参照。

| 本目录概念 | VM 侧文档 |
|-----------|---------|
| `sys_vmctl` 内核实现 | [26-vm-init-main.md](../02-stage-vm/draft/26-vm-init-main.md) §4.3（VM dispatch 调用 sys_vmctl） |
| 页错误转发 | [15-pagefault.md](../02-stage-vm/draft/15-pagefault.md) |
| VMREQUEST 协议 | 03-vm-request.md（本目录） + [26-vm-init-main.md](../02-stage-vm/draft/26-vm-init-main.md) |

---

## 7. 参见

- [../02-stage-vm/draft/00-vm-overview.md](../02-stage-vm/draft/00-vm-overview.md) — VM 整体架构
- [../02-stage-vm/draft/26-vm-init-main.md](../02-stage-vm/draft/26-vm-init-main.md) — VM 初始化与主循环（kernel→VM 交互）
- `minix3/minix/kernel/kernel.h` — 内核顶层头文件

---

*分类: Kernel 整体层级*
