# 17-main-init: kmain 与内核启动流程

> **分类**: Kernel 时间与初始化
> **源码**: `minix3/minix/kernel/main.c`(522行), `arch/i386/arch_system.c: arch_init`()(246), `bsp_finish_booting`()
> **说明**: kmain → bsp_finish_booting → announce → 启动所有 boot_proc → 进入调度循环——开机全链路

---

## 1. 概述

### 1.1 概念定义/作用

**内核启动流程**是从引导加载器将控制权交给内核到第一个用户进程开始运行的全过程。Minix3 的启动流程分为两个阶段：

1. **架构相关初始化**（`cstart()` / `arch_init()`）：由汇编入口调用，完成硬件检测、内存布局发现、页表设置等底层工作
2. **架构无关初始化**（`kmain()` → `bsp_finish_booting()`）：初始化进程表、特权结构、系统调用向量、时钟等通用子系统，最终进入调度循环

启动流程的核心挑战是**初始化顺序依赖**：许多子系统相互依赖（如进程表需要特权结构、调度需要时钟），必须按正确顺序初始化。

### 1.2 与 Minix3 的对应关系

| 功能 | 函数 | 源文件 |
|------|------|--------|
| 内核主入口 | `kmain()` | main.c:115 |
| BSP 完成启动 | `bsp_finish_booting()` | main.c:38 |
| 架构相关初始化 | `cstart()` | arch/i386/arch_system.c |
| 进程表初始化 | `proc_init()` | proc.c |
| 系统调用初始化 | `system_init()` | system.c |
| 时钟初始化 | `init_clock()` | clock.c |
| 内存初始化 | `memory_init()` | main.c |
| 启动公告 | `announce()` | main.c:333 |
| SMP 初始化 | `smp_init()` | smp.c |
| 启动映像定义 | `image[]` | table.c |

### 1.3 关键状态/机制说明

**启动映像（boot image）**：Minix3 的启动映像是一个静态定义的进程数组 `image[NR_BOOT_PROCS]`，包含每个启动进程的属性：进程名、进程号、特权模板、优先级、时间片等。`kmain()` 遍历此数组初始化进程表。

**可调度性判断**：启动映像中的进程分为两类：
- **立即可调度**：内核任务（IDLE、CLOCK、SYSTEM）、RS（根系统进程）、VM——它们在 `kmain()` 中获得特权并可直接运行
- **延迟调度**：其他系统进程（PM、VFS、INIT 等）——设置 `RTS_NO_PRIV | RTS_NO_QUANTUM`，等待 RS 设置特权后才能运行

**VMINHIBIT / BOOTINHIBIT**：除 VM 外的所有用户态启动进程设置 `RTS_VMINHIBIT | RTS_BOOTINHIBIT`，确保它们在 VM 设置好页表之前不会运行。

**kernel_may_alloc**：全局标志，启动阶段为 1（允许内核分配内存），`bsp_finish_booting()` 中设为 0（VM 运行后内核不再允许分配）。

### 1.4 行为规则

1. **BSS 检查**：`kmain()` 首先检查 BSS 段是否被正确清零（`bss_test == 0`）
2. **启动模块数验证**：`NR_BOOT_MODULES` 必须等于 multiboot 报告的模块数
3. **内核任务立即运行**：内核任务和 RS/VM 在 `kmain()` 中获得特权
4. **其他进程等待特权**：非内核任务设置 `RTS_NO_PRIV`，等待 RS 设置特权
5. **VM 之前不可运行**：除 VM 外的用户进程设置 `RTS_VMINHIBIT | RTS_BOOTINHIBIT`
6. **所有进程初始停止**：所有进程设置 `RTS_PROC_STOP`，`bsp_finish_booting()` 中统一解除
7. **BSP 最后进入调度**：`bsp_finish_booting()` 调用 `switch_to_user()` 进入调度循环，不再返回

## 2. C 源码分析

### 2.1 相关定义（常量、配置等）

#### 2.1.1 启动映像进程列表

| 进程 | 进程号 | 特权模板 | 可调度? | 说明 |
|------|--------|---------|--------|------|
| ASYNCM | -5 | TSK_F | 是 | 异步消息通知 |
| IDLE | -4 | IDL_F | 是 | 空闲任务 |
| CLOCK | -3 | TSK_F | 是 | 时钟任务 |
| SYSTEM | -2 | TSK_F | 是 | 系统服务任务 |
| KERNEL | -1 | — | — | 内核伪进程（不在映像中） |
| VM | 0 | VM_F | 是 | 虚拟内存管理器 |
| RS | 1 | RSYS_F | 是 | 重生服务器 |
| PM | 2 | — | 否 | 进程管理器 |
| SCHED | 3 | — | 否 | 调度器 |
| VFS | 4 | — | 否 | 虚拟文件系统 |
| INIT | 8 | — | 否 | 初始化进程 |

#### 2.1.2 启动相关 RTS 标志

| 标志 | 启动含义 |
|------|---------|
| `RTS_SLOT_FREE` | 进程槽位空闲（初始状态） |
| `RTS_PROC_STOP` | 进程被停止（所有启动进程初始设置） |
| `RTS_NO_PRIV` | 无特权（等待 RS 设置） |
| `RTS_NO_QUANTUM` | 无时间片（等待调度器分配） |
| `RTS_VMINHIBIT` | 等待 VM 设置页表 |
| `RTS_BOOTINHIBIT` | 等待 VM 就绪 |

### 2.2 核心数据结构

#### 2.2.1 kinfo_t 内核信息结构

| 字段 | 类型 | 含义 |
|------|------|------|
| `boot_procs[]` | `struct boot_image[]` | 启动映像进程数组 |
| `mbi` | `multiboot_info_t` | Multiboot 信息 |
| `module_list[]` | `multiboot_module_t[]` | 启动模块列表 |
| `kmess` | `struct kmessages` | 内核消息缓冲区 |
| `bootstrap_start` | `phys_bytes` | 引导代码起始地址 |
| `bootstrap_len` | `phys_bytes` | 引导代码长度 |

#### 2.2.2 struct boot_image 启动映像条目

| 字段 | 类型 | 含义 |
|------|------|------|
| `proc_nr` | `proc_nr_t` | 进程号 |
| `proc_name` | `char[]` | 进程名 |
| `endpoint` | `endpoint_t` | IPC endpoint |
| `start_addr` | `vir_bytes` | 启动模块加载地址 |
| `len` | `vir_bytes` | 启动模块长度 |

### 2.3 关键函数分析

#### 2.3.1 kmain()——内核主入口

`minix3/minix/kernel/main.c:115-328`

```c
void kmain(kinfo_t *local_cbi)
```

**功能**：内核的主入口函数，由汇编启动代码调用。

**行为**（按执行顺序）：

1. **BSS 检查**：`assert(bss_test == 0)`，验证 BSS 段被正确清零
2. **保存启动参数**：`memcpy(&kinfo, local_cbi, sizeof(kinfo))`
3. **板级识别**：`get_board_id_by_name(env_get(BOARDVARNAME))`
4. **串口初始化**（ARM）：`arch_ser_init()`
5. **允许内核分配**：`kernel_may_alloc = 1`
6. **架构相关初始化**：`cstart()`——设置页表、中断控制器、检测硬件
7. **获取大内核锁**：`BKL_LOCK()`
8. **进程表初始化**：`proc_init()`——清空所有槽位，设置 `RTS_SLOT_FREE`
9. **IPC 过滤器池初始化**：`IPCF_POOL_INIT()`
10. **启动模块数验证**：`NR_BOOT_MODULES == kinfo.mbi.mi_mods_count`
11. **遍历启动映像初始化进程**（`for i = 0; i < NR_BOOT_PROCS`）：
    - 获取进程指针 `rp = proc_addr(ip->proc_nr)`
    - 设置 endpoint：`ip->endpoint = rp->p_endpoint`
    - 判断可调度性：`iskerneln || isrootsysn || VM_PROC_NR`
    - **可调度进程**：`get_priv(rp, static_priv_id)`，设置特权模板
    - **不可调度进程**：`RTS_SET(rp, RTS_NO_PRIV | RTS_NO_QUANTUM)`
    - 架构相关初始化：`arch_boot_proc(ip, rp)`
    - 设置 VMINHIBIT / BOOTINHIBIT（非 VM 的用户进程）
    - 设置 `RTS_PROC_STOP`，清除 `RTS_SLOT_FREE`
12. **更新启动进程信息**：`memcpy(kinfo.boot_procs, image, ...)`
13. **IPC 调用名注册**：`IPCNAME(SEND)` 等
14. **架构后初始化**：`arch_post_init()`
15. **内存初始化**：`memory_init()`
16. **系统调用初始化**：`system_init()`
17. **释放引导内存**：`add_memmap(&kinfo, ...)`
18. **SMP 初始化**（条件编译）：`smp_init()` 或单 CPU 回退
19. **完成启动**：`bsp_finish_booting()`

#### 2.3.2 bsp_finish_booting()——BSP 完成启动

`minix3/minix/kernel/main.c:38-109`

```c
void bsp_finish_booting(void)
```

**功能**：BSP 完成最后的启动步骤，进入调度循环。

**行为**（按执行顺序）：

1. **CPU 识别**：`cpu_identify()`
2. **VM 标记未运行**：`vm_running = 0`
3. **随机数初始化**：`krandom.random_sources/elements = ...`
4. **设置计费/当前进程指针**：`bill_ptr = proc_ptr = idle_proc`
5. **打印启动公告**：`announce()`
6. **解除启动进程的 PROC_STOP**：`RTS_UNSET(proc_addr(i), RTS_PROC_STOP)` for `i = 0..NR_BOOT_PROCS-NR_TASKS-1`
7. **周期计费初始化**：`cycles_accounting_init()`
8. **BSP 定时器初始化**：`boot_cpu_init_timer(system_hz)`
9. **FPU 初始化**：`fpu_init()`
10. **SMP 就绪标志**：`cpu_set_flag(bsp_cpu_id, CPU_IS_READY)`
11. **禁止内核分配**：`kernel_may_alloc = 0`
12. **进入调度循环**：`switch_to_user()`（不返回）

**关键设计**：`RTS_UNSET(proc_addr(i), RTS_PROC_STOP)` 仅解除用户进程（`i >= NR_TASKS`）的 PROC_STOP，内核任务（IDLE、CLOCK、SYSTEM）保持 PROC_STOP——它们通过其他机制运行（IDLE 由 `idle()` 函数管理，CLOCK/SYSTEM 通过 IPC 回复运行）。

#### 2.3.3 cstart()——架构相关初始化

`minix3/minix/kernel/arch/i386/arch_system.c`

```c
void cstart(void)
```

**功能**：x86 架构相关的内核初始化。

**行为**：
1. 低级硬件初始化（GDT/IDT 设置）
2. 内存布局检测（物理内存范围）
3. 保护模式配置
4. 中断控制器初始化（PIC/APIC）
5. 环境变量解析

#### 2.3.4 announce()——启动公告

`minix3/minix/kernel/main.c:333-346`

```c
static void announce(void)
```

**功能**：打印 MINIX 启动横幅，包含版本号和版权信息。

### 2.4 调用关系/调用点分析

#### 2.4.1 完整启动路径

```
引导加载器 (Multiboot)
  └─ 汇编入口 (start.S)
       ├─ 设置栈、GDT、分页
       └─ cstart() → kmain(local_cbi)
            ├─ BSS 检查
            ├─ 保存启动参数
            ├─ cstart()（架构初始化）
            ├─ BKL_LOCK()
            ├─ proc_init()
            ├─ IPCF_POOL_INIT()
            ├─ 遍历 image[] 初始化进程
            │    ├─ [可调度?] get_priv() + 特权模板
            │    └─ [不可调度?] RTS_NO_PRIV | RTS_NO_QUANTUM
            ├─ arch_post_init()
            ├─ memory_init()
            ├─ system_init()
            ├─ [SMP?] smp_init() / smp_single_cpu_fallback()
            └─ bsp_finish_booting()
                 ├─ announce()
                 ├─ RTS_UNSET(PROC_STOP) 解除用户进程
                 ├─ boot_cpu_init_timer()
                 ├─ fpu_init()
                 ├─ kernel_may_alloc = 0
                 └─ switch_to_user() → 进入调度
```

#### 2.4.2 启动进程的状态变迁

```
kmain() 初始化:
  所有进程: RTS_SLOT_FREE → RTS_PROC_STOP (清除 SLOT_FREE)

bsp_finish_booting():
  用户进程: RTS_PROC_STOP → 清除 (RTS_UNSET)

VM 运行后:
  用户进程: RTS_VMINHIBIT | RTS_BOOTINHIBIT → 清除 (VMCTL)

RS 设置特权后:
  系统进程: RTS_NO_PRIV → 清除 (sys_privctl)

调度器分配时间片后:
  进程: RTS_NO_QUANTUM → 清除 (sys_schedule)

最终: p_rts_flags == 0 → 进程可运行
```

### 2.5 设计要点/特殊处理

#### 2.5.1 启动映像的静态定义

Minix3 的启动映像在编译时静态定义（`image[]` 数组），而非运行时动态发现。这简化了启动流程——内核确切知道有哪些进程需要初始化，以及每个进程的属性。代价是修改启动映像需要重新编译内核。

#### 2.5.2 分阶段可运行性

启动进程不是一次性全部可运行，而是分阶段解除阻塞：

1. **kmain**：内核任务 + RS + VM 获得特权
2. **bsp_finish_booting**：用户进程解除 PROC_STOP
3. **VM 运行**：解除 VMINHIBIT / BOOTINHIBIT
4. **RS 运行**：通过 `sys_privctl` 为其他系统进程设置特权（解除 NO_PRIV）
5. **调度器运行**：通过 `sys_schedule` 分配时间片（解除 NO_QUANTUM）

这确保了进程按正确的依赖顺序启动——VM 必须先运行才能管理内存，RS 必须先运行才能设置特权。

#### 2.5.3 kernel_may_alloc 标志

`kernel_may_alloc` 在启动阶段为 1，允许内核使用 `alloc_mem()` 分配物理内存。`bsp_finish_booting()` 中设为 0，因为 VM 运行后由 VM 负责内存分配。内核在 VM 运行后分配内存可能导致冲突——内核和 VM 可能分配同一块物理内存。

#### 2.5.4 VM 的特殊启动地位

VM 是唯一一个在 `kmain()` 中获得特权但不设置 VMINHIBIT 的用户进程。VM 必须最先运行，因为它负责为其他进程设置页表。VM 的特权模板 `VM_F` 包含 `VM_SYS_PROC` 标志，使其可以执行 VM 控制操作。

#### 2.5.5 BKL（大内核锁）

`kmain()` 中获取大内核锁 `BKL_LOCK()`。在 SMP 配置下，BKL 确保启动阶段只有一个 CPU 执行内核代码。AP 在启动时等待 BKL，直到 BSP 完成初始化后才参与调度。

#### 2.5.6 shutdown 流程

`prepare_shutdown()` 设置一个 1 秒的定时器，到期后调用 `minix_shutdown()`。延迟 1 秒是为了让进程有机会执行清理工作。`minix_shutdown()` 禁用所有中断、停止本地定时器、SMP 下停止 AP，然后调用 `arch_shutdown()` 执行架构相关的关机操作（关机/重启/断电）。
