# 05-proc-init-boot-proc: 进程表初始化与 VM ELF 加载

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/proc.c:119-173`, `minix3/minix/kernel/main.c:157-282`, `minix3/minix/kernel/arch/i386/protect.c:379-456`, `minix3/minix/kernel/arch/i386/arch_system.c:146-192`, `minix3/minix/kernel/arch/earm/protect.c:106-183`, `minix3/minix/kernel/arch/earm/arch_system.c:42-60`
> **说明**: cstart() 完成后，内核清空进程表、遍历 boot image 设置进程特权、加载 VM ELF 到 bootstrap 页表
> **前置**: [04-clock-interrupt-init.md](04-clock-interrupt-init.md) — 时钟和中断控制器已初始化

---

## 1. 概述

### 1.0 进程表：内核如何看待"进程"

"进程是运行中的程序"——这是教科书上的定义。但在内核眼中，进程更具体也更无聊：

> **进程 = 进程表中的一个 slot + 一组寄存器状态 + 一组特权属性**

`proc_init()` 做的就是初始化这个"白板"：把所有 slot 标记为"空闲"（`SLOT_FREE`），给每个 slot 分配编号和 endpoint ID，清零寄存器状态。这时还没有任何代码在运行——进程表只是一个空的容器。

**那进程是怎么"活"起来的？**

进程不"自己活起来"——它是被内核"造"出来的。内核遍历 boot image（编译时硬编码的进程清单），为每个进程：
1. 在进程表中分配一个 slot
2. 设置特权级别（`priv` 结构）
3. 初始化寄存器状态（PC、SP、PSW——第一次进入用户态时从哪里开始执行）
4. 只有 VM 特殊处理：解析其 ELF 二进制，分配物理页，映射到页表

**为什么 VM 是特殊的？——"开天辟地"问题**

VM（Memory Manager）是 Minix3 微内核中负责管理地址空间的进程。每个用户进程的页表都由 VM 创建和维护。但 VM 自己也需要地址空间才能运行——而 VM 启动前，没有进程有自己的页表。

这是一个**鸡生蛋问题**：

```
VM 必须运行才能为其他进程创建页表
→ 但 VM 自己也需要地址空间
→ 没有"别人"能为 VM 创建页表
→ 内核必须亲自用 bootstrap 页表"抱"VM 起来
```

解决方案：内核在 `arch_boot_proc()` 中，用自己建立的 bootstrap 页表解析 VM 的 ELF 二进制、分配物理页、建立映射。VM 的第一行代码就在 bootstrap 页表上运行。VM 运行后做的第一件事是——**扔掉 bootstrap 页表，建立属于自己的页表结构**。

这个"开天辟地"的模式在 OS 设计中反复出现：第一个进程必须由内核手工创建，然后它才能创建更多进程。UNIX 中 PID 1（init）就是由内核手工 fork 出来的。

**boot image 的哲学含义**：Minix3 内核硬编码了哪些进程是系统进程（`NR_BOOT_PROCS`），包括 VM、PM、VFS、RS 等。这意味着内核在编译时就知道了进程拓扑——微内核的"内核内置进程拓扑"和宏内核的"内核内置驱动列表"本质上是一样的设计包袱。

### 1.1 阶段 C 的位置

03 文档的六阶段总览中，阶段 C 是 `proc_init + arch_boot_proc`：

| 阶段 | 标记 | 函数 | 做什么 | 文档 |
|------|------|------|--------|------|
| A: 入口 | T3 | kmain 入口 | memcpy(&kinfo)、BSS 检查 | 03 |
| B: cstart | T3→T4 | cstart() | prot_init → init_clock → intr_init → arch_init | 03+04 |
| **C: 进程表** | **T7→T8** | **proc_init + arch_boot_proc** | **清空进程表、加载 VM ELF** | **本文** |
| D: post-init | T9→T10 | arch_post_init + memory_init | ptproc=VM、freepdes 分配 | 06 |
| E: system | T11 | system_init | 特权表初始化 | 07 |
| F: finish | T12 | bsp_finish_booting | 回收 bootstrap、切换用户态 | 07 |

阶段 C 包含两个核心操作：

1. **`proc_init()`**：清空进程表，将每个 slot 标记为 `SLOT_FREE`，设置 `p_nr` 和 `p_endpoint`，调用 `arch_proc_reset()` 初始化架构特定寄存器状态。同时清空特权表（`priv` table）。

2. **`arch_boot_proc()` 循环**：遍历 boot image 中的每个进程，为其分配特权结构、设置调度参数、调用 `arch_boot_proc()` 进行架构特定初始化。其中最重要的是 **加载 VM 的 ELF 二进制到 bootstrap 页表**——这是内核启动 VM 的唯一途径。

### 1.2 为什么 proc_init 必须在 arch_boot_proc 之前

`proc_init()` 将所有进程 slot 标记为 `SLOT_FREE`，这是后续所有进程分配操作的前提。`arch_boot_proc()` 循环中调用 `proc_addr(ip->proc_nr)` 获取进程指针，该操作依赖 `p_nr` 已正确设置。

### 1.3 arch_boot_proc 的核心：加载 VM ELF

`arch_boot_proc()` 对大多数进程只做 `arch_proc_init()`（设置 PC/SP），但对 VM 进程做了一件特殊的事：**解析 VM 的 ELF 二进制，分配物理页，映射到 bootstrap 页表**。

这是必要的，因为：

- VM 是第一个用户态进程，它负责为所有其他进程创建页表
- 在 VM 运行之前，没有进程有页表——内核必须用 bootstrap 页表让 VM 先跑起来
- VM 的 ELF 加载使用 `libexec_load_elf()`，它调用 `pg_map(PG_ALLOCATEME, ...)` 在 bootstrap 页表中分配和映射页面

### 1.4 三架构对照

| 方面 | x86-64 | aarch64 | riscv64 |
|------|--------|---------|---------|
| arch_proc_reset | 清零寄存器 + 设置 CS/DS/SS/ES/FS/GS = USER_DS_SELECTOR + 设置 PSW | 清零 p_reg + 设置 PSR (EL0t) | 清零 p_reg + 设置 sstatus (SPP=0, SPIE=1) |
| arch_proc_init | arch_proc_reset + 设置 PC/SP/bx(ps_strings) | arch_proc_reset + 设置 PC/SP/retreg(ps_strings) | arch_proc_reset + 设置 PC/SP/a0(ps_strings) |
| arch_boot_proc | 仅 VM: libexec_load_elf + arch_proc_init | 仅 VM: libexec_load_elf + arch_proc_init | 仅 VM: libexec_load_elf + arch_proc_init |
| FPU/ExtReg | memset(fpu_state[nr], 0, FPU_XFP_SIZE) | 无 FPU 初始化 | 无 FPU 初始化 |
| VM ELF 加载 | pg_map(PG_ALLOCATEME) + pg_load() | pg_map(PG_ALLOCATEME) + pg_load() | pg_map(PG_ALLOCATEME) + pg_load() |

### 1.5 Rust 版与 C 版的差异

| 方面 | Minix3 C | minix-rs |
|------|---------|----------|
| 进程表 | 全局数组 `struct proc proc[NR_TASKS+NR_PROCS]` | `ProcessTable` 结构体，内含 `Box<[KProcess]>` |
| 特权表 | 全局数组 `struct priv priv[NR_SYS_PROCS]` | `PrivTable` 结构体，内含 `Box<[KPriv]>` |
| proc_init | `memset` + 循环设置 p_nr/p_endpoint/p_rts_flags | `ProcessTable::new()` 构造时完成初始化 |
| arch_proc_reset | `memset(&reg, 0)` + 设置段寄存器/PSW | `ArchProcReset` trait 方法（参数：`is_kernel`, `proc_nr`） |
| arch_boot_proc | `libexec_load_elf` + 回调函数 | `BootProcArch::load_vm_elf()` trait 方法 + `VmLoadResult` |
| VM ELF 加载 | `pg_map(PG_ALLOCATEME, ...)` + `pg_load()` | `BootProcArch::load_vm_elf()` 返回 `VmLoadResult` |
| boot image | 全局 `image[]` 数组 | `KernelInfo::boot_modules` 切片 |
| 进程特权分配 | `get_priv(rp, static_priv_id(proc_nr))` | `PrivTable::assign_static()` (TODO) |
| RTS 标志 | 位操作宏 `RTS_SET/RTS_UNSET` | `RtsFlags::set/clear` 方法 |
| boot 循环 | `main.c` 内联循环 | `init_proc_and_boot()` 函数 |

---

## 2. C 源码分析

### 2.1 proc_init()：清空进程表和特权表

`proc_init()` 操作两个全局数组——进程表 `proc[]` 和特权表 `priv[]`。理解它做了什么，必须先理解这两个表的结构和关系。

#### 2.1.0 进程表 `struct proc`：内核眼中的"进程"

进程表是内核最核心的数据结构。每个进程（包括内核 task 和用户进程）在进程表中占一个 slot。`struct proc` 的字段可以分为 5 组：

| 组 | 字段 | 作用 |
|----|------|------|
| **寄存器** | `p_reg` (stackframe_s), `p_seg` (segframe) | 上下文保存：进程被切换出时，寄存器快照存在 `p_reg`；段描述符存在 `p_seg` |
| **调度** | `p_rts_flags`, `p_priority`, `p_quantum_size_ms`, `p_scheduler`, `p_nextready` | 调度状态：`p_rts_flags` 非零 = 不可运行；`p_nextready` 链成调度队列 |
| **IPC** | `p_getfrom_e`, `p_sendto_e`, `p_sendmsg`, `p_delivermsg`, `p_caller_q`, `p_q_link` | 消息传递：Minix3 的 IPC 是同步阻塞的，这些字段记录"谁等谁"、"消息在哪" |
| **统计** | `p_user_time`, `p_sys_time`, `p_cycles`, `p_accounting` | 性能计数：用户态/内核态时间、调度延迟等 |
| **VM** | `p_vmrequest`, `p_endpoint` | 虚拟内存协作：进程访问缺页内存时，内核通过 `p_vmrequest` 挂起进程，等 VM 处理 |

**关键字段 `p_nr` 和 `p_endpoint`**：

- `p_nr`（进程号）：进程在 `proc[]` 数组中的索引。范围 `-NR_TASKS` 到 `NR_PROCS-1`。负数是内核 task（IDLE、CLOCK、SYSTEM 等），0 及以上是用户进程（VM、PM、VFS 等）。`p_nr` 在进程生命周期内不变。
- `p_endpoint`（端点号）：IPC 使用的进程标识。`endpoint = (generation, p_nr)`——当进程退出、新进程复用同一 slot 时，generation 递增，确保旧消息不会发给新进程。这是 Minix3 的"进程版本号"机制。

**关键字段 `p_rts_flags`**：进程可运行当且仅当 `p_rts_flags == 0`。每一位代表一个不可运行原因：

| 位 | 含义 | 谁设置 | 谁清除 |
|----|------|--------|--------|
| `RTS_SLOT_FREE` | slot 空闲 | `proc_init()` | `alloc_proc()` |
| `RTS_PROC_STOP` | 进程已停止 | `proc_stop()` | 信号/调度器 |
| `RTS_SENDING` | 阻塞在 send | `mini_send()` | `mini_receive()` |
| `RTS_RECEIVING` | 阻塞在 receive | `mini_receive()` | `mini_send()` |
| `RTS_VMINHIBIT` | 等 VM 创建页表 | boot 循环 | `vmctl_vminhibit_clear()` |
| `RTS_BOOTINHIBIT` | 等 boot 完成 | boot 循环 | `bsp_finish_booting()` |

#### 2.1.1 特权表 `struct priv`：系统进程的权限控制

不是所有进程都有特权表。只有**系统进程**（VM、PM、VFS、RS、驱动等）才需要 `priv`——普通用户进程的 `p_priv` 指向一个共享的默认特权结构。

`struct priv` 的核心字段：

| 字段 | 作用 |
|------|------|
| `s_proc_nr` | 关联的进程号（`NONE` = 未分配） |
| `s_id` | 特权结构索引 |
| `s_flags` | 权限标志（`PREEMPTIBLE`/`BILLABLE`/`IDL_F` 等） |
| `s_trap_mask` | 允许的系统调用 trap 编号 |
| `s_ipc_to` | 允许 IPC 的目标进程位图 |
| `s_k_call_mask[]` | 允许的内核调用位图 |
| `s_io_tab[]` / `s_irq_tab[]` | 允许的 I/O 端口和 IRQ |
| `s_sig_mgr` / `s_bak_sig_mgr` | 信号管理器（谁处理这个进程的信号） |

**特权表的分配方式**：Minix3 区分"静态分配"和"动态分配"。系统进程（`NR_STATIC_PRIV_IDS` 个）在 `proc_init()` 时通过 `static_priv_id(proc_nr)` 计算索引，直接映射到 `priv[]` 数组的固定位置。普通进程运行时通过 `get_priv()` 动态分配。

**进程表和特权表的关系**：`proc.p_priv` 指向 `priv[s_id]`，`priv.s_proc_nr` 指回 `proc.p_nr`。两个表通过指针双向关联。

#### 2.1.2 proc_init() 逐行分析

`proc.c:119-173`：

```c
void proc_init(void)
{
    struct proc * rp;
    struct priv *sp;
    int i;

    for (rp = BEG_PROC_ADDR, i = -NR_TASKS; rp < END_PROC_ADDR; ++rp, ++i) {
        rp->p_rts_flags = RTS_SLOT_FREE;
        rp->p_magic = PMAGIC;
        rp->p_nr = i;
        rp->p_endpoint = _ENDPOINT(0, rp->p_nr);
        rp->p_scheduler = NULL;
        rp->p_priority = 0;
        rp->p_quantum_size_ms = 0;
        arch_proc_reset(rp);
    }
    for (sp = BEG_PRIV_ADDR, i = 0; sp < END_PRIV_ADDR; ++sp, ++i) {
        sp->s_proc_nr = NONE;
        sp->s_id = (sys_id_t) i;
        ppriv_addr[i] = sp;
        sp->s_sig_mgr = NONE;
        sp->s_bak_sig_mgr = NONE;
    }
    idle_priv.s_flags = IDL_F;
    for (i = 0; i < CONFIG_MAX_CPUS; i++) {
        struct proc * ip = get_cpu_var_ptr(i, idle_proc);
        ip->p_endpoint = IDLE;
        ip->p_priv = &idle_priv;
        ip->p_rts_flags |= RTS_PROC_STOP;
        set_idle_name(ip->p_name, i);
    }
}
```

**逐行分析**：

1. **进程表循环**（`BEG_PROC_ADDR` → `END_PROC_ADDR`）：
   - `p_rts_flags = RTS_SLOT_FREE`：标记 slot 为空闲
   - `p_magic = PMAGIC`：魔数，用于调试检测野指针
   - `p_nr = i`：进程号从 `-NR_TASKS` 到 `NR_PROCS-1`
   - `p_endpoint = _ENDPOINT(0, p_nr)`：初始 endpoint，generation=0
   - `p_scheduler = NULL`：无用户态调度器
   - `p_priority = 0` / `p_quantum_size_ms = 0`：默认调度参数
   - `arch_proc_reset(rp)`：架构特定寄存器初始化

2. **特权表循环**（`BEG_PRIV_ADDR` → `END_PRIV_ADDR`）：
   - `s_proc_nr = NONE`：标记为空闲
   - `s_id = i`：特权结构索引
   - `ppriv_addr[i] = sp`：建立索引→指针映射
   - `s_sig_mgr = NONE` / `s_bak_sig_mgr = NONE`：无信号管理器

3. **IDLE 进程初始化**：
   - 每个 CPU 一个 IDLE 进程
   - `idle_priv.s_flags = IDL_F`：IDLE 特权标志
   - `p_rts_flags |= RTS_PROC_STOP`：IDLE 永远不被调度

### 2.2 arch_proc_reset()：架构特定寄存器初始化

#### x86-64（`arch_system.c:146-192`）

```c
void arch_proc_reset(struct proc *pr)
{
    char *v = NULL;
    struct stackframe_s reg;

    assert(pr->p_nr < NR_PROCS);

    if(pr->p_nr >= 0) {
        v = fpu_state[pr->p_nr];
        assert(!((vir_bytes)v % FPUALIGN));
        memset(v, 0, FPU_XFP_SIZE);
    }

    memset(&reg, 0, sizeof(pr->p_reg));
    if(iskerneln(pr->p_nr))
        reg.psw = INIT_TASK_PSW;
    else
        reg.psw = INIT_PSW;

    pr->p_seg.fpu_state = v;

    pr->p_reg.cs = USER_CS_SELECTOR;
    pr->p_reg.gs = pr->p_reg.fs =
    pr->p_reg.ss = pr->p_reg.es =
    pr->p_reg.ds = USER_DS_SELECTOR;

    arch_proc_setcontext(pr, &reg, 0, KTS_FULLCONTEXT);
}
```

**关键点**：

1. **FPU 状态**：仅用户进程（`p_nr >= 0`）有 FPU 保存区。内核任务不使用 FPU。
2. **PSW**：内核任务用 `INIT_TASK_PSW`（IOPL=1），用户进程用 `INIT_PSW`（IOPL=0）。64 位模式下 PSW 是 RFLAGS，bit 1 必须为 1。C 源码 32 位值：`INIT_TASK_PSW = 0x1200`，`INIT_PSW = 0x0200`；64 位值：`0x1202` 和 `0x0202`（增加 bit 1）。
3. **段选择子**：所有进程的 CS/DS/SS/ES/FS/GS 统一设为 `USER_CS_SELECTOR` / `USER_DS_SELECTOR`。这是因为 Minix3 在 64 位模式下使用平坦内存模型，所有进程共享相同的段描述符。
4. **arch_proc_setcontext**：将 `reg` 复制到 `pr->p_reg`，设置 `MF_CONTEXT_SET` 标志。

#### aarch64（`arch_system.c:42-60`）

```c
void arch_proc_reset(struct proc *pr)
{
    assert(pr->p_nr < NR_PROCS);
    memset(&pr->p_reg, 0, sizeof(pr->p_reg));
    if(iskerneln(pr->p_nr)) {
        pr->p_reg.psr = INIT_TASK_PSR;
    } else {
        pr->p_reg.psr = INIT_PSR;
    }
}
```

**关键点**：

1. **无 FPU 初始化**：ARM 的 `arch_proc_reset` 不初始化 FPU 状态。FPU/VFP 状态在首次使用时通过 lazy switch 初始化。
2. **PSR**：`INIT_TASK_PSR` 用于内核任务（EL1h），`INIT_PSR` 用于用户进程（EL0t）。C 源码 32 位 ARM 值：`INIT_TASK_PSR = PSR_SVC32_MODE | PSR_F = 0x53`，`INIT_PSR = PSR_USR32_MODE | PSR_F = 0x50`；64 位 AArch64 值：`0x3C5`（EL1h, F/I/A/D masked）和 `0x0`（EL0t）。32 位 ARM 的 SVC32/USR32 模式在 AArch64 中对应 EL1/EL0。
3. **无段选择子**：ARM 不使用段选择子，寄存器初始化比 x86 简单得多。

### 2.3 main.c 的 boot image 循环

`main.c:157-282`：

```c
proc_init();
IPCF_POOL_INIT();

if(NR_BOOT_MODULES != kinfo.mbi.mi_mods_count)
    panic("expecting %d boot processes/modules, found %d",
          NR_BOOT_MODULES, kinfo.mbi.mi_mods_count);

for (i=0; i < NR_BOOT_PROCS; ++i) {
    int schedulable_proc;
    proc_nr_t proc_nr;
    int ipc_to_m, kcalls;
    sys_map_t map;

    ip = &image[i];
    rp = proc_addr(ip->proc_nr);
    ip->endpoint = rp->p_endpoint;
    rp->p_cpu_time_left = 0;
    if(i < NR_TASKS)
        strlcpy(rp->p_name, ip->proc_name, sizeof(rp->p_name));

    if(i >= NR_TASKS) {
        multiboot_module_t *mb_mod = &kinfo.module_list[i - NR_TASKS];
        ip->start_addr = mb_mod->mod_start;
        ip->len = mb_mod->mod_end - mb_mod->mod_start;
    }

    reset_proc_accounting(rp);

    proc_nr = proc_nr(rp);
    schedulable_proc = (iskerneln(proc_nr) || isrootsysn(proc_nr) ||
        proc_nr == VM_PROC_NR);

    if(schedulable_proc) {
        (void) get_priv(rp, static_priv_id(proc_nr));
        // ... 设置 s_flags, s_trap_mask, ipc_to_m, kcalls ...
        fill_sendto_mask(rp, &map);
        // ... 填充 s_k_call_mask ...
    } else {
        RTS_SET(rp, RTS_NO_PRIV | RTS_NO_QUANTUM);
    }

    arch_boot_proc(ip, rp);

    if(!get_cpulocal_var(proc_ptr))
        get_cpulocal_var(proc_ptr) = rp;

    if(rp->p_nr != VM_PROC_NR && rp->p_nr >= 0) {
        rp->p_rts_flags |= RTS_VMINHIBIT;
        rp->p_rts_flags |= RTS_BOOTINHIBIT;
    }

    rp->p_rts_flags |= RTS_PROC_STOP;
    rp->p_rts_flags &= ~RTS_SLOT_FREE;
}
```

**关键逻辑**：

1. **boot module 地址填充**：`i >= NR_TASKS` 的进程（用户态服务）从 multiboot module 获取 ELF 二进制的物理地址和长度。

2. **schedulable 判定**：只有内核任务、root system process（RS）和 VM 可以立即调度。其他进程需要等待 RS 设置特权。

3. **特权分配**：`get_priv(rp, static_priv_id(proc_nr))` 为 schedulable 进程分配静态特权 ID。不同类型的进程有不同的特权标志：
   - **VM**：`VM_F`（VM 系统进程标志）
   - **内核任务**：`TSK_F`（任务标志）或 `IDL_F`（IDLE）
   - **RS**：`RSYS_F`（Root 系统进程标志）

4. **非 schedulable 进程**：设置 `RTS_NO_PRIV | RTS_NO_QUANTUM`，阻止其运行。

5. **arch_boot_proc**：架构特定初始化。对 VM 进程，加载 ELF 到 bootstrap 页表。

6. **VM inhibit**：除 VM 外的用户态进程设置 `RTS_VMINHIBIT | RTS_BOOTINHIBIT`，等待 VM 为其创建页表。

7. **最终状态**：所有进程设置 `RTS_PROC_STOP`，清除 `RTS_SLOT_FREE`。

### 2.4 arch_boot_proc()：VM ELF 加载

#### x86-64（`protect.c:388-456`）

```c
void arch_boot_proc(struct boot_image *ip, struct proc *rp)
{
    multiboot_module_t *mod;
    struct ps_strings *psp;
    char *sp;

    if(rp->p_nr < 0) return;  // 内核任务跳过

    mod = bootmod(rp->p_nr);

    if(rp->p_nr == VM_PROC_NR) {
        struct exec_info execi;
        memset(&execi, 0, sizeof(execi));

        execi.stack_high = kinfo.user_sp;
        execi.stack_size = 64 * 1024;
        execi.proc_e = ip->endpoint;
        execi.hdr = (char *) mod->mod_start;
        execi.filesize = execi.hdr_len = mod->mod_end - mod->mod_start;
        strlcpy(execi.progname, ip->proc_name, sizeof(execi.progname));
        execi.frame_len = 0;

        execi.copymem = libexec_copy_memcpy;
        execi.clearmem = libexec_clear_memset;
        execi.allocmem_prealloc_junk = libexec_pg_alloc;
        execi.allocmem_prealloc_cleared = libexec_pg_alloc;
        execi.allocmem_ondemand = libexec_pg_alloc;
        execi.clearproc = NULL;

        if(libexec_load_elf(&execi) != OK)
            panic("VM loading failed");

        sp = (char *)execi.stack_high;
        sp -= sizeof(struct ps_strings);
        psp = (struct ps_strings *) sp;
        sp -= (sizeof(void *) + sizeof(void *) + sizeof(int));
        psp->ps_argvstr = (char **)(sp + sizeof(int));
        psp->ps_nargvstr = 0;
        psp->ps_envstr = psp->ps_argvstr + sizeof(void *);
        psp->ps_nenvstr = 0;

        arch_proc_init(rp, execi.pc, (vir_bytes)sp,
            execi.stack_high - sizeof(struct ps_strings),
            ip->proc_name);

        add_memmap(&kinfo, mod->mod_start, mod->mod_end-mod->mod_start);
        mod->mod_end = mod->mod_start = 0;
        kinfo.vm_allocated_bytes = alloc_for_vm;
    }
}
```

**关键流程**：

1. **内核任务跳过**：`p_nr < 0` 的进程（CLOCK、SYSTEM 等）不需要 ELF 加载。

2. **仅 VM 加载**：只有 VM 进程在 `arch_boot_proc` 中被加载。其他用户态进程（PM、VFS 等）的 ELF 加载由 RS 在运行时完成。

3. **exec_info 设置**：
   - `stack_high = kinfo.user_sp`：用户栈顶（USR_STACKTOP）
   - `stack_size = 64 * 1024`：64KB 栈
   - `hdr`：ELF 头物理地址（identity mapping 可直接访问）
   - `copymem/clearmem/allocmem`：回调函数，`libexec_pg_alloc` 调用 `pg_map(PG_ALLOCATEME, ...)` 在 bootstrap 页表中分配页面

4. **ps_strings**：在栈顶放置 `struct ps_strings`，包含 argv/envp 指针。这是 BSD 风格的进程启动约定。

5. **arch_proc_init**：设置进程的 PC = `execi.pc`（ELF 入口点）、SP = 栈指针、bx = ps_strings 地址。

6. **内存回收**：加载完成后，VM 的 boot module 物理内存被标记为空闲（`add_memmap`）。

#### aarch64（`protect.c:115-183`）

与 x86-64 版本几乎完全相同，唯一的差异在 `arch_proc_init`：
- x86-64: `pr->p_reg.bx = ps_str`（ebx 寄存器）
- aarch64: `pr->p_reg.retreg = ps_str`（r0 寄存器）

### 2.5 arch_proc_init()：设置进程初始 PC/SP

#### x86-64（`memory.c:722-733`）

```c
void arch_proc_init(struct proc *pr, const u32_t ip, const u32_t sp,
    const u32_t ps_str, char *name)
{
    arch_proc_reset(pr);
    strlcpy(pr->p_name, name, sizeof(pr->p_name));
    pr->p_reg.pc = ip;
    pr->p_reg.sp = sp;
    pr->p_reg.bx = ps_str;
}
```

#### aarch64（`memory.c:627-638`）

```c
void arch_proc_init(struct proc *pr, const u32_t ip, const u32_t sp,
    const u32_t ps_str, char *name)
{
    arch_proc_reset(pr);
    strcpy(pr->p_name, name);
    pr->p_reg.pc = ip;
    pr->p_reg.sp = sp;
    pr->p_reg.retreg = ps_str;
}
```

**差异**：x86-64 用 `bx`（ebx）传递 ps_strings，aarch64 用 `retreg`（r0）传递。

---

## 3. Rust 设计决策

### 3.1 proc_init → ProcessTable::new()

C 的 `proc_init()` 在运行时清空全局数组。Rust 版将初始化逻辑移入 `ProcessTable::new()` 构造函数：

- 每个 `KProcess` 在创建时已设置 `p_nr`、`p_endpoint`、`p_rts_flags = SLOT_FREE`
- `arch_proc_reset()` 的功能由 `KProcess::new()` 中的默认值实现
- IDLE 进程的特殊初始化在 `ProcessTable::new()` 中完成

**理由**：Rust 的类型系统保证 `ProcessTable` 在构造后即处于一致状态，无需运行时 `memset`。

### 3.2 arch_proc_reset → ArchProcReset trait

```rust
pub trait ArchProcReset {
    /// Reset architecture-specific process state to default values.
    ///
    /// Called during process table initialization and when a process slot
    /// is being recycled. Sets register state to safe defaults:
    /// - x86-64: CS/DS/SS/ES/FS/GS = USER selectors, PSW = INIT_PSW
    /// - aarch64: PSR = EL0t (user) or EL1h (kernel task)
    /// - riscv64: sstatus = SPP=0, SPIE=1 (user) or SPP=1, SPIE=0 (kernel)
    fn reset(is_kernel: bool, proc_nr: i32);
}
```

**理由**：三种架构的 `arch_proc_reset` 行为不同（x86-64 设置段选择子，aarch64/riscv64 设置 PSR/sstatus），需要 trait 抽象。方法签名不直接操作 `KProcess`，而是由调用方在 `KProcess` 上应用结果——这避免了 arch 层对 kernel 层的依赖。

### 3.3 arch_boot_proc → BootProcArch trait

```rust
pub trait BootProcArch: ArchProcInit {
    /// Load a VM ELF binary into the bootstrap page table.
    ///
    /// For kernel tasks (p_nr < 0): no-op.
    /// For VM: load ELF into bootstrap page table, set PC/SP.
    /// For other user processes: set initial register state only
    /// (ELF loading is done by RS at runtime).
    ///
    /// C: arch_boot_proc() — protect.c:388 (x86) / protect.c:115 (ARM)
    fn load_vm_elf<P: Paging>(
        module: &BootModule,
        kernel_info: &KernelInfo,
        paging: &mut P,
    ) -> VmLoadResult;
}
```

**理由**：`arch_boot_proc` 的核心逻辑（VM ELF 加载）在所有架构上相同，但 `arch_proc_init` 设置的寄存器不同。trait 将架构差异封装在实现中。`BootProcArch` 继承 `ArchProcInit`，确保 ELF 加载后可以调用 `init()` 设置寄存器。

### 3.4 arch_proc_init → ArchProcInit trait

```rust
pub trait ArchProcInit: ArchProcReset {
    /// Initialize a process with a specific entry point and stack pointer.
    ///
    /// Called after arch_proc_reset to set the process's initial PC, SP,
    /// and architecture-specific argument register (ps_strings).
    ///
    /// C: arch_proc_init() — memory.c:722 (x86) / memory.c:627 (ARM)
    fn init(
        is_kernel: bool,
        proc_nr: i32,
        pc: VirBytes,
        sp: VirBytes,
        ps_strings: VirBytes,
        name: &str,
    );
}
```

**理由**：`arch_proc_init` 在三种架构上设置不同的寄存器（x86-64: PC/SP/BX, aarch64: PC/SP/R0, riscv64: PC/SP/A0），需要 trait 抽象。`ArchProcInit` 继承 `ArchProcReset`，因为 `init` 内部调用 `reset`。

### 3.5 VM ELF 加载策略

C 版使用 `libexec_load_elf()` + 回调函数（`pg_map(PG_ALLOCATEME, ...)`）。Rust 版采用以下策略：

1. **ELF 解析**：使用 `object` crate（`no_std` 兼容）解析 ELF 头和程序头
2. **页面分配**：使用 `boot_alloc` 的 bump allocator 分配物理页
3. **映射**：通过 `Paging` trait 的 `map` 方法映射到 bootstrap 页表
4. **复制**：直接 `memcpy` 从 boot module 物理地址到映射后的虚拟地址

**理由**：C 版的 `libexec` 框架是为用户态 `exec()` 设计的通用框架，内核启动时只需要其 ELF 加载子集。Rust 版用更简单直接的实现替代。

### 3.6 ps_strings 处理

C 版在栈顶构造 `struct ps_strings`，包含 argv/envp 指针。Rust 版保留此行为：

```rust
/// PsStrings layout on the stack (BSD convention):
///
/// High address (stack_high):
///   +--------------------+
///   | struct ps_strings  |  ps_nargvstr=0, ps_argvstr, ps_envstr, ps_nenvstr=0
///   +--------------------+
///   | padding (int)      |  argc = 0
///   | argv pointer       |  points to padding above
///   | envp pointer       |  points after argv
///   +--------------------+  <-- SP
```

**理由**：VM 的 C 启动代码期望栈上有 ps_strings 结构。Rust 版必须保持相同的栈布局。

### 3.7 boot_proc 循环中的特权分配

C 版在 `main.c` 循环中为 schedulable 进程调用 `get_priv()` 并设置特权标志。Rust 版将此逻辑封装在 `init_proc_and_boot()` 函数中：

```rust
fn init_proc_and_boot(kernel_info: &KernelInfo) {
    // ... proc_init + priv_table init ...
    // ... boot module iteration with privilege assignment ...
}
```

**理由**：C 版的特权分配逻辑散落在 `main.c` 的 120 行循环中。Rust 版将其提取为 `init_proc_and_boot()` 独立函数，与 `init_protection()`（prot_init）和 `init_clock_and_interrupts()`（clock+intr）保持一致的阶段划分。

---

## 4. 实现详解

### 4.1 ProcessTable::new() — proc_init 等价

```rust
// os/kernel/src/proc_table.rs
// ProcessTable::new() 构造函数：
// - 创建 NR_TASKS + NR_PROCS 个 KProcess
// - 每个 KProcess::new(nr, endpoint) 设置 p_nr, p_endpoint, p_rts_flags = SLOT_FREE
// - IDLE 进程设置 RTS_PROC_STOP
```

Rust 版的 `ProcessTable::new()` 覆盖了 C 版 `proc_init()` 的进程表部分。特权表的初始化在 `PrivTable::new()` 中完成。

### 4.2 ArchProcReset 实现

#### x86-64

```rust
pub struct X86_64ProcArch;

impl ArchProcReset for X86_64ProcArch {
    fn reset(is_kernel: bool, proc_nr: i32) {
        // C: arch_system.c:146-192
        // Register state defaults:
        // - psw: INIT_PSW (user) or INIT_TASK_PSW (kernel)
        // - cs: USER_CS_SELECTOR
        // - ds/ss/es/fs/gs: USER_DS_SELECTOR
        // - FPU state: zeroed for user processes (proc_nr >= 0)
        let _ = (is_kernel, proc_nr);
    }
}
```

#### aarch64

```rust
pub struct AArch64ProcArch;

impl ArchProcReset for AArch64ProcArch {
    fn reset(is_kernel: bool, proc_nr: i32) {
        // C: earm/arch_system.c:42-60
        // Clear p_reg
        // Set PSR: EL0t for user, EL1h for kernel tasks
        let _ = (is_kernel, proc_nr);
    }
}
```

#### riscv64

```rust
pub struct Riscv64ProcArch;

impl ArchProcReset for Riscv64ProcArch {
    fn reset(is_kernel: bool, proc_nr: i32) {
        // No C source — designed by analogy with aarch64
        // Clear p_reg
        // Set sstatus: SPP=0/SPIE=1 for user, SPP=1/SPIE=0 for kernel
        let _ = (is_kernel, proc_nr);
    }
}
```

### 4.3 ArchProcInit 实现

#### x86-64

```rust
impl ArchProcInit for X86_64ProcArch {
    fn init(is_kernel: bool, proc_nr: i32, pc: VirBytes, sp: VirBytes,
            ps_strings: VirBytes, name: &str) {
        // C: memory.c:722-733
        Self::reset(is_kernel, proc_nr);
        // Set register state:
        // - rip = pc (entry point)
        // - rsp = sp (stack pointer)
        // - rbx = ps_strings (argument to C runtime)
    }
}
```

#### aarch64

```rust
impl ArchProcInit for AArch64ProcArch {
    fn init(is_kernel: bool, proc_nr: i32, pc: VirBytes, sp: VirBytes,
            ps_strings: VirBytes, name: &str) {
        // C: earm/memory.c:627-638
        Self::reset(is_kernel, proc_nr);
        // Set register state:
        // - pc = pc (entry point)
        // - sp = sp (stack pointer)
        // - r0 = ps_strings (aarch64: retreg = ps_strings)
    }
}
```

#### riscv64

```rust
impl ArchProcInit for Riscv64ProcArch {
    fn init(is_kernel: bool, proc_nr: i32, pc: VirBytes, sp: VirBytes,
            ps_strings: VirBytes, name: &str) {
        Self::reset(is_kernel, proc_nr);
        // Set register state:
        // - sepc = pc (entry point)
        // - sp = sp (stack pointer)
        // - a0 = ps_strings (riscv64: first argument register)
    }
}
```

### 4.4 BootProcArch 实现

所有架构共享相同的 `load_vm_elf` 逻辑，差异仅在 `ArchProcInit`：

```rust
pub struct VmLoadResult {
    pub pc: VirBytes,
    pub sp: VirBytes,
    pub ps_strings: VirBytes,
    pub allocated_bytes: usize,
}

impl BootProcArch for X86_64ProcArch {
    fn load_vm_elf<P: Paging>(
        module: &BootModule,
        kernel_info: &KernelInfo,
        paging: &mut P,
    ) -> VmLoadResult {
        // C: protect.c:388-456
        // 1. Parse ELF header and program headers
        // 2. For each PT_LOAD segment: allocate pages, map in bootstrap
        //    page table, copy segment data
        // 3. Set up user stack
        // 4. Return entry point and stack pointer
        // ... (placeholder: actual ELF loading requires `object` crate)
    }
}
```

### 4.5 init_proc_and_boot() — main.c 循环的 Rust 版

```rust
fn init_proc_and_boot(kernel_info: &KernelInfo) {
    use minix_arch::{ArchProcInit, BootProcArch, CurrentBootProcArch};
    use crate::proc::{ProcNr, ProcName, rts};
    use crate::proc_table::{ProcessTable, NR_TASKS};
    use crate::kpriv::PrivTable;

    // Step 1: Initialize process table (proc_init equivalent)
    let mut proc_table = ProcessTable::new();

    // Step 2: Initialize privilege table
    let mut priv_table = PrivTable::new();

    // Step 3: Iterate over boot modules
    for (i, module) in kernel_info.boot_modules.iter().enumerate() {
        let nr: ProcNr = if i < NR_TASKS {
            (i as ProcNr) - (NR_TASKS as ProcNr)  // kernel tasks: -NR_TASKS..-1
        } else {
            (i - NR_TASKS) as ProcNr  // user processes: 0..
        };

        let proc = proc_table.get_mut(nr);
        if proc.is_none() { continue; }
        let proc = proc.unwrap();

        proc.p_name = ProcName::from_str(module.name);

        let is_kernel = nr < 0;
        let is_vm = nr == VM_PROC_NR;
        let schedulable = is_kernel || is_root_sys || is_vm;

        if schedulable {
            // Assign static privilege
            // TODO: implement PrivTable::assign_static()
        } else {
            proc.p_rts_flags.set(rts::NO_PRIV | rts::NO_QUANTUM);
        }

        // Architecture-specific boot initialization
        if is_kernel {
            // kernel tasks: arch_boot_proc skips
        } else if is_vm {
            CurrentBootProcArch::init(false, nr, VirBytes(0), VirBytes(0),
                VirBytes(0), module.name);
        } else {
            CurrentBootProcArch::init(false, nr, VirBytes(0), VirBytes(0),
                VirBytes(0), module.name);
        }

        // VM inhibit: all user processes except VM
        if nr != VM_PROC_NR && nr >= 0 {
            proc.p_rts_flags.set(rts::VMINHIBIT | rts::BOOTINHIBIT);
        }

        proc.p_rts_flags.set(rts::PROC_STOP);
        proc.p_rts_flags.clear(rts::SLOT_FREE);
    }
}
```

---

## 5. 测试要点

### 5.1 proc_init 测试

| 测试 | 验证 | C 对应 |
|------|------|--------|
| `ProcessTable::new()` 所有 slot 为 `SLOT_FREE` | 初始状态正确 | `rp->p_rts_flags = RTS_SLOT_FREE` |
| `ProcessTable::new()` p_nr 正确 | `-NR_TASKS` 到 `NR_PROCS-1` | `rp->p_nr = i` |
| `ProcessTable::new()` p_endpoint 正确 | `_ENDPOINT(0, p_nr)` | `rp->p_endpoint = _ENDPOINT(0, rp->p_nr)` |
| IDLE 进程 `RTS_PROC_STOP` | IDLE 不可调度 | `ip->p_rts_flags |= RTS_PROC_STOP` |

### 5.2 ArchProcReset 测试

| 测试 | 验证 | 架构 |
|------|------|------|
| 用户进程 PSW/PSR/sstatus 正确 | 用户态标志位 | all |
| 内核任务 PSW/PSR/sstatus 正确 | 内核态标志位 | all |
| x86-64 段选择子正确 | CS/DS/SS = USER selectors | x86-64 |
| x86-64 FPU 状态清零 | 用户进程 ext_reg_state = 0 | x86-64 |

### 5.3 ArchProcInit 测试

| 测试 | 验证 | 架构 |
|------|------|------|
| PC/SP 设置正确 | 入口点和栈指针 | all |
| ps_strings 寄存器正确 | rbx(x86)/r0(arm)/a0(riscv) | all |
| 进程名设置正确 | `p_name` | all |
| reset 在 init 内部被调用 | 状态一致 | all |

### 5.4 BootProcArch 测试

| 测试 | 验证 |
|------|------|
| VM ELF 加载成功 | VmLoadResult.pc = ELF entry, sp = user stack |
| ps_strings 栈布局正确 | VmLoadResult.ps_strings 正确 |
| 非 VM 用户进程无 ELF 加载 | 仅设置 RTS 标志 |

### 5.5 init_proc_and_boot 测试

| 测试 | 验证 | C 对应 |
|------|------|--------|
| schedulable 进程获得特权 | `get_priv` 成功 | `get_priv(rp, static_priv_id(proc_nr))` |
| 非 schedulable 进程 RTS_NO_PRIV | 标志位正确 | `RTS_SET(rp, RTS_NO_PRIV \| RTS_NO_QUANTUM)` |
| VM 特权标志 VM_F | `s_flags` 包含 VM_SYS_PROC | `priv(rp)->s_flags = VM_F` |
| 非 VM 用户进程 VMINHIBIT | 标志位正确 | `rp->p_rts_flags |= RTS_VMINHIBIT` |
| 所有进程 PROC_STOP | 标志位正确 | `rp->p_rts_flags |= RTS_PROC_STOP` |

---

## 6. 参见

- [03-kmain-entry-protection.md](03-kmain-entry-protection.md) — kmain 入口与保护模式初始化
- [04-clock-interrupt-init.md](04-clock-interrupt-init.md) — 时钟与中断初始化
- [tmp-06-proc-struct.md](tmp-06-proc-struct.md) — 进程结构体详细设计
- [tmp-11-privilege.md](tmp-11-privilege.md) — 特权系统详细设计
- [99-global-concepts.md](99-global-concepts.md) — 全局概念定义
