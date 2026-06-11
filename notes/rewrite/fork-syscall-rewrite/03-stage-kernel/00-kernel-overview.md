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

## 3. 文档导航：01~22 的叙事逻辑

### 3.1 阶段 1：硬件发现（01~03）

内核是第一个运行的程序。没人给它 IPC、没人给它内存 map。它自己从 GRUB 的 multiboot 数据结构中解析一切。

| 编号 | 文档 | 覆盖的 C 源文件（行数） | 角色 |
|------|------|----------------------|------|
| 01 | [multiboot-bootstrap](01-boot-shim-bootstrap.md) | pre_init.c(243) + pg_utils.c(317) | GRUB→内存map→大页恒等映射→开分页 |
| 02 | [page-table-kernel](02-page-table-kernel.md) | memory.c(1020) 内核页表核心 | pagedir_mappings、createpde、内核如何替 VM 操作页表 |
| 03 | [vm-request](03-vm-request.md) | proc.h vmrequest 字段 + proc.c vmrequest 调用 | VMREQUEST 挂起/恢复机制（和 02 紧密关联） |

### 3.2 阶段 2：保护与中断（04~05）

分页打开了，但还没有中断。用户进程不能碰硬件，内核需要 GDT/IDT/TSS 来保护自己。

| 编号 | 文档 | 覆盖的 C 源文件 | 角色 |
|------|------|---------------|------|
| 04 | [protection](04-protection.md) | protect.c(456) | GDT/IDT/TSS、段初始化、boot_proc 二进制加载 |
| 05 | [exception-interrupt](05-exception-interrupt.md) | exception.c(386) + interrupt.c(177) | 异常帧、页错误转发 VM、IRQ 处理 |

### 3.3 阶段 3：进程抽象（06~08）

有了保护模式，内核需要管理谁在执行——进程表、调度队列、endpoint 标识。

| 编号 | 文档 | 覆盖的 C 源文件 | 角色 |
|------|------|---------------|------|
| 06 | [proc-struct](06-proc-struct.md) | proc.h 完整结构体(~160行) + RTS/misc 标志 | struct proc 每个字段 + RTS 逐位解释 |
| 07 | [scheduling](07-scheduling.md) | proc.c 调度部分(~700行) | pick_proc / enqueue / dequeue / switch_to_user / idle / accounting |
| 08 | [endpoint](08-endpoint.md) | proc.c endpoint 部分(~100行) + endpoint.h | endpoint_lookup / isokendpt_f / generation 机制 |

### 3.4 阶段 4：IPC（09~10）

进程之间存在，还需要通信。IPC 是 Minix3 微内核的脊柱——所有服务通过消息传递协作。

| 编号 | 文档 | 覆盖的 C 源文件 | 角色 |
|------|------|---------------|------|
| 09 | [sync-ipc](09-sync-ipc.md) | proc.c: do_ipc(599~773) | SEND/RECEIVE/BOTH/NOTIFY 四个原语 |
| 10 | [async-ipc](10-async-ipc.md) | proc.c: mini_senda / try_async / cancel_async | 异步消息传递 |

### 3.5 阶段 5：特权与系统调用（11~15）

进程+调度+IPC 都有了，现在来处理系统调用——这是用户态进程请求内核服务的唯一通道。

| 编号 | 文档 | 覆盖的 C 源文件 | 角色 |
|------|------|---------------|------|
| 11 | [privilege](11-privilege.md) | priv.h(80行) + system.c 特权操作 | priv 结构体 + priv_add_irq/io/mem + 权限 mask |
| 12 | [syscall-dispatch](12-syscall-dispatch.md) | system.c: kernel_call_dispatch + finish | do_xxx() 分发循环 + 结果回写 |
| 13 | [syscall-memory](13-syscall-memory.md) | system.c: sys_vmctl / sys_vm_map + arch_system 相关 | VM 完成页表操作后通过 sys_vmctl 通知内核 |
| 14 | [syscall-fork-exec](14-syscall-fork-exec.md) | system.c: sys_fork(~200行) + arch_system fork 相关 | 内核的 fork——创建 proc 结构、复制寄存器、生成 endpoint |
| 15 | [syscall-exit-signal](15-syscall-exit-signal.md) | system.c: sys_exit/sys_kill + sig_delay_done | 退出和信号 |

### 3.6 阶段 6：时间与初始化（16~17）

| 编号 | 文档 | 覆盖的 C 源文件 | 角色 |
|------|------|---------------|------|
| 16 | [timer](16-timer.md) | clock.c(312) + arch_clock.c(440) | 时钟滴答、虚拟/剖面定时器、看门狗 |
| 17 | [main-init](17-main-init.md) | main.c(522) + arch_system.c arch_init | kmain → bsp_finish_booting → announce → 开任务 |

### 3.7 阶段 7：多核与补全（18~21）

| 编号 | 文档 | 覆盖的 C 源文件 | 角色 |
|------|------|---------------|------|
| 18 | [smp](18-smp.md) | smp.c(205) + apic.c(1304) + arch_smp.c(360) | SMP 架构、AP Boot、APIC 中断 |
| 19 | [debug-serial](19-debug-serial.md) | debug.c(563) + ser_dump_* | 内核调试基础设施 |
| 20 | [acpi-watchdog](20-acpi-watchdog.md) | acpi.c(410) + watchdog.c + arch_watchdog | ACPI 电源管理、看门狗 |
| 21 | [unported-symbols](21-unported-symbols.md) | 其余小文件 | 未移植函数的 ARCH 标注 |

### 3.8 补充：全局概念

| 编号 | 文档 | 说明 |
|------|------|------|
| 99 | [global-concepts](99-global-concepts.md) | 跨文档共享的全局概念（RTS 标志位完整表、priv 结构体、常量定义等） |

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

本目录应与 [../02-stage-vm](../02-stage-vm/) 交叉参照。

| 本目录概念 | VM 侧文档 |
|-----------|---------|
| `sys_vmctl` 内核实现 | [26-vm-init-main.md](../02-stage-vm/26-vm-init-main.md) §4.3（VM dispatch 调用 sys_vmctl） |
| 页错误转发 | [15-pagefault.md](../02-stage-vm/15-pagefault.md) |
| VMREQUEST 协议 | 03-vm-request.md（本目录） + [26-vm-init-main.md](../02-stage-vm/26-vm-init-main.md) |

---

## 7. 参见

- [../02-stage-vm/00-vm-overview.md](../02-stage-vm/00-vm-overview.md) — VM 整体架构
- [../02-stage-vm/26-vm-init-main.md](../02-stage-vm/26-vm-init-main.md) — VM 初始化与主循环（kernel→VM 交互）
- `minix3/minix/kernel/kernel.h` — 内核顶层头文件

---

*分类: Kernel 整体层级*
