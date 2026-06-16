# 05-proc-init-boot-proc: 进程表初始化与 VM ELF 加载

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/proc.c:119-160`, `minix3/minix/kernel/main.c:157-282`, `minix3/minix/kernel/arch/i386/protect.c:379-456`, `minix3/minix/kernel/arch/i386/arch_system.c:146-192`, `minix3/minix/kernel/arch/earm/protect.c:106-183`, `minix3/minix/kernel/arch/earm/arch_system.c:42-60`
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

### 1.1 阶段 C 的位置

03 文档的六阶段总览中，阶段 C 是 `proc_init + arch_boot_proc`：

| 阶段 | 标记 | 函数 | 做什么 | 文档 |
|------|------|------|--------|------|
| A: 入口 | T2 | kmain 入口 | memcpy(&kinfo)、BSS 检查 | 03 |
| B: cstart | T2+T3 | cstart() | prot_init → init_clock → intr_init → arch_init | 03+04 |
| **C: 进程表** | **T4+T5** | **proc_init + arch_boot_proc** | **清空进程表、加载 VM ELF** | **本文** |
| D: post-init | T6+T7 | arch_post_init + memory_init | ptproc=VM、freepdes 分配 | 06 |
| E: system | T8+T9 | system_init + add_memmap | 系统调用初始化、bootstrap 回收 | 07 |
| F: finish | T9.5+T10 | bsp_finish_booting | SMP 初始化、启动完成、切换用户态 | 07 |

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
| arch_proc_reset | 设置 PSW=INIT_PSW, CS/DS/SS/ES/FS/GS=USER selectors | 设置 PSR=EL0t/EL1h | 设置 sstatus=SPP=0/SPIE=1 |
| arch_proc_init | arch_proc_reset + 设置 PC/SP/bx(ps_strings) | arch_proc_reset + 设置 PC/SP/retreg(ps_strings) | arch_proc_reset + 设置 PC/SP/a0(ps_strings) |
| arch_boot_proc | 仅 VM: libexec_load_elf + arch_proc_init | 仅 VM: libexec_load_elf + arch_proc_init | 仅 VM: libexec_load_elf + arch_proc_init |
| FPU/ExtReg | 用户进程清零 FPU state | 无 FPU 初始化（lazy） | 无 FPU 初始化（lazy） |
| VM ELF 加载 | pg_map(PG_ALLOCATEME) + pg_load() | pg_map(PG_ALLOCATEME) + pg_load() | pg_map(PG_ALLOCATEME) + pg_load() |

### 1.5 Rust 版与 C 版的差异

| 方面 | Minix3 C | minix-rs |
|------|---------|----------|
| 进程表 | 全局数组 `struct proc proc[NR_TASKS+NR_PROCS]` | `ProcessTable` 结构体，内含 `Box<[KProcess]>` |
| 特权表 | 全局数组 `struct priv priv[NR_SYS_PROCS]` | `PrivTable` 结构体，内含 `Box<[KPriv]>` |
| proc_init | `memset` + 循环设置 p_nr/p_endpoint/p_rts_flags | `ProcessTable::new()` 构造时完成初始化 |
| arch_proc_reset | `memset(&reg, 0)` + 设置段寄存器/PSW | `ArchProcReset::initial_reg_state()` 返回 `InitialRegState` |
| arch_proc_init | `arch_proc_reset` + 设置 PC/SP/ps_strings | `ArchProcInit::init_regs()` 返回 `(pc, sp, ps_strings_reg)` |
| arch_boot_proc | `libexec_load_elf` + 回调函数 | `BootProcArch::load_vm_elf()` trait 方法 + `VmLoadResult` |
| VM ELF 加载 | `pg_map(PG_ALLOCATEME, ...)` + `pg_load()` | `BootProcArch::load_vm_elf()` 返回 `VmLoadResult` |
| boot image | 全局 `image[]` 数组 | `KernelInfo::boot_modules` 切片 |
| 进程特权分配 | `get_priv(rp, static_priv_id(proc_nr))` | `PrivTable::assign_static()` |
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

`proc.c:119-160`：

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
2. **PSR**：`INIT_TASK_PSR` 用于内核任务（EL1h），`INIT_PSR` 用于用户进程（EL0t）。32 位 ARM 值：`INIT_TASK_PSR = 0x53`，`INIT_PSR = 0x50`；64 位 AArch64 值：`0x3C5`（EL1h, F/I/A/D masked）和 `0x0`（EL0t）。
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
   - **VM**：`VM_F`
   - **内核任务**：`TSK_F`（非 IDLE）或 `IDL_F`（IDLE）
   - **RS**：`RSYS_F`

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

#### libexec_load_elf() C 源码分析（`libexec/exec_elf.c:122-318`）

`libexec_load_elf()` 是 Minix3 的通用 ELF 加载框架，用于内核启动和用户态 `exec()`。核心流程：

1. **ELF 头校验**（`elf_unpack` → `check_header` → `elf_sane`）：
   - 验证 ELF magic（`\x7fELF`）、数据编码、版本号
   - 拒绝动态链接 ELF（`elf_has_interpreter` 检测 PT_INTERP 段）
   - 程序头偏移必须在第一个扇区内（`e_phoff <= 512`）

2. **PT_LOAD 段遍历**（`for i in 0..e_phnum`）：
   - 跳过非 PT_LOAD 或 `p_memsz == 0` 的段
   - 页对齐计算：`vaddr -= page_offset; foffset -= page_offset`
   - 对齐检查：`p_vaddr % PAGE_SIZE == p_offset % PAGE_SIZE`（否则禁用 mmap）
   - PF_R/PF_W/PF_X 标志映射为 `mmap_prot`

3. **内存分配策略**（两种路径）：
   - **mmap 路径**（`execi->memmap`）：优先尝试 mmap 映射文件段，剩余部分用 `allocmem_ondemand`
   - **allocmem 路径**（`allocmem_prealloc_junk`）：分配物理页 → `copymem` 复制段数据 → `clearmem` 清零页内未使用部分

4. **栈分配**：`allocmem_ondemand(stacklow, stack_size)` 在最后分配

5. **返回值**：`execi->pc = hdr->e_entry + load_offset; execi->load_base = startv`

**Rust 对应**：`minix-elf::segment_iter()` 替代 `elf_unpack` + PT_LOAD 遍历；`Paging::map()` 替代 `pg_map(PG_ALLOCATEME)`；直接 `copy_nonoverlapping` 替代 `copymem` 回调。

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

### 2.6 get_priv() 和 fill_sendto_mask()：特权与IPC掩码

#### get_priv()（`system.c:272-345`）

```c
int get_priv(register struct proc *rc, int proc_type)
{
    register struct priv *sp;
    int i;

    for (sp = FIRST_PRIV_ADDR; sp < END_PRIV_ADDR; sp++) {
        if (sp->s_proc_nr == NONE) {
            sp->s_proc_nr = proc_nr(rc);
            sp->s_flags = (proc_type == TASK_Q || proc_type == TASK_SYS)
                ? IDL_F : SYS_PROC;
            rc->p_priv = sp;
            return OK;
        }
    }
    return ENOSPC;
}
```

**关键逻辑**：
1. 线性扫描 `priv[]` 数组（`NR_SYS_PROCS=64`个slot），找到空闲slot（`s_proc_nr == NONE`）
2. **静态分配模式**：`proc_type` 是 `static_priv_id(proc_nr)` 返回的静态ID，映射到 `priv[]` 的固定位置
3. **标志设置**：内核任务 `IDL_F`，用户态 `SYS_PROC`
4. 返回值：成功返回 `OK(0)`，无空闲返回 `ENOSPC`

#### fill_sendto_mask()（`system.c:347-380`）

```c
void fill_sendto_mask(const struct proc *rp, sys_map_t *map)
{
    int i;
    memset(map, 0, sizeof(sys_map_t));
    for (i = 0; i < NR_SYS_PROCS; i++) {
        struct priv *priv = PRIV_ADDR(i);
        if (priv->s_flags & SYS_PROC && priv->s_proc_nr != NONE) {
            int priv_id = priv_id(priv);
            if (priv_id != rp->p_priv->s_id) {
                set_sys_bit(*map, priv_id);
            }
        }
    }
}
```

**关键逻辑**：
1. 构建进程允许 `sendto` 的系统进程位图（`sys_map_t` = u32位图，最多32个系统进程）
2. 允许 `sendto` 除自身外的所有 `SYS_PROC` 特权进程
3. 后续由调用方根据进程类型进一步裁剪位图（如内核任务限制为 `TASK_Q`）

### 2.7 错误路径分析

| 错误场景 | C代码位置 | C处理方式 | Rust需处理 |
|---------|----------|----------|-----------|
| `proc_addr` 返回 NULL | proc.c:179 | `panic("couldn't find a free process slot")` | `ProcessTable::get_mut()` 返回 `Option`，由上层 `expect()` 或 `panic` |
| `get_priv` 返回 `ENOSPC` | system.c:272 | 调用方 `assert(r != ENOSPC)`（main.c 不检查返回值） | `PrivTable::assign_static()` 返回 `Result` 或 `expect` |
| VM ELF 加载失败 | protect.c:421-423 | `panic("VM loading failed")` | `load_vm_elf()` 返回 `Result`，由上层决定 panic（与C行为一致） |
| `NR_BOOT_MODULES` 校验失败 | main.c:161-162 | `panic("expecting %d...")` | `assert_eq!` 宏（已实现） |
| `p_priv` 为 NULL 时访问 | — | C代码不检查（假定已初始化） | Rust `Option<&KPriv>` 提供编译期安全 |

---

## 3. Rust 设计决策

### 3.1 核心理念：纯函数式 trait 设计

C 版的 `arch_proc_reset()` 和 `arch_proc_init()` 直接修改进程结构体的字段。这要求 arch 层知道 kernel 层的内部结构——Rust 中不能这样（arch crate 不能依赖 kernel crate）。

**解决方案**：trait 方法返回**纯值**（`InitialRegState`、`(pc, sp, ps_strings_reg)`），由 kernel 层的 `init_proc_and_boot()` 负责将这些值写入 `KProcess`。这实现了关注点分离：

- **arch 层**：知道"x86-64 的初始 PSW 是什么"、"aarch64 的 ps_strings 放在哪个寄存器"
- **kernel 层**：知道"如何将寄存器状态写入 trap frame"、"进程结构体的字段布局"

```rust
// arch 层只返回值，不修改进程结构体
pub trait ArchProcReset {
    fn initial_reg_state(is_kernel: bool, proc_nr: i32) -> InitialRegState;
}

pub trait ArchProcInit: ArchProcReset {
    fn init_regs(is_kernel: bool, proc_nr: i32, pc: VirBytes, sp: VirBytes,
                 ps_strings: VirBytes) -> InitialRegs;
}

// kernel 层负责将返回值写入进程结构体
fn init_proc_and_boot(kernel_info: &KernelInfo) {
    let reg_state = CurrentBootProcArch::initial_reg_state(is_kernel, nr);
    proc.set_reg_state(reg_state);  // kernel 层操作
}
```

### 3.2 proc_init → ProcessTable::new()

C 的 `proc_init()` 在运行时清空全局数组。Rust 版将初始化逻辑移入 `ProcessTable::new()` 构造函数：

- 每个 `KProcess` 在创建时已设置 `p_nr`、`p_endpoint`、`p_rts_flags = SLOT_FREE`
- IDLE 进程的特殊初始化在 `ProcessTable::new()` 中完成
- 特权表初始化在 `PrivTable::new()` 中完成
- TODO(P2): C 版在 `proc_init` 中设置 `p_magic = PMAGIC`（proc.c:132）用于检测野指针/内存踩踏。Rust 版可考虑在 `cfg(debug_assertions)` 下添加 `p_magic: u32` 字段

**理由**：Rust 的类型系统保证 `ProcessTable` 在构造后即处于一致状态，无需运行时 `memset`。

### 3.3 ArchProcReset trait：返回寄存器状态

```rust
pub trait ArchProcReset {
    /// 返回新进程的初始寄存器状态。
    fn initial_reg_state(is_kernel: bool, proc_nr: i32) -> InitialRegState;
}
```

**理由**：三种架构的初始寄存器状态不同（x86-64 有段选择子，aarch64/riscv64 没有），但语义相同——"新进程的寄存器应该是什么初始值"。trait 抽象的是"返回初始状态"这个语义，而非"修改进程结构体"这个副作用。

### 3.4 ArchProcInit trait：返回 PC/SP/ps_strings

```rust
pub trait ArchProcInit: ArchProcReset {
    /// 返回进程的初始 PC、SP 和 ps_strings 寄存器值。
    fn init_regs(is_kernel: bool, proc_nr: i32, pc: VirBytes, sp: VirBytes,
                 ps_strings: VirBytes) -> InitialRegs;
}
```

**理由**：`arch_proc_init` 在三种架构上设置不同的寄存器（x86-64: PC/SP/BX, aarch64: PC/SP/R0, riscv64: PC/SP/A0）。`ArchProcInit` 继承 `ArchProcReset`，因为 `init` 内部调用 `reset`。方法返回 `InitialRegs` 而非修改进程结构体，遵循纯函数式设计。

### 3.5 BootProcArch trait：ELFF 加载

```rust
pub trait BootProcArch: ArchProcInit {
    /// 加载 VM ELF 到 bootstrap 页表，返回入口点、栈指针、ps_strings。
    fn load_vm_elf<P: Paging>(
        module: &BootModule,
        kernel_info: &KernelInfo,
        paging: &mut P,
    ) -> VmLoadResult;
}
```

**理由**：`arch_boot_proc` 的核心逻辑（VM ELF 加载）在所有架构上相同，但 `arch_proc_init` 设置的寄存器不同。trait 将架构差异封装在实现中。`BootProcArch` 继承 `ArchProcInit`，确保 ELF 加载后可以调用 `init()` 设置寄存器。

### 3.6 VM ELF 加载策略

C 版使用 `libexec_load_elf()` + 回调函数（`pg_map(PG_ALLOCATEME, ...)`）。Rust 版采用以下策略：

1. **ELF 解析**：使用 `object` crate（`no_std` 兼容）解析 ELF 头和程序头
2. **页面分配**：使用 `boot_alloc` 的 bump allocator 分配物理页
3. **映射**：通过 `Paging` trait 的 `map` 方法映射到 bootstrap 页表
4. **复制**：直接 `memcpy` 从 boot module 物理地址到映射后的虚拟地址

**理由**：C 版的 `libexec` 框架是为用户态 `exec()` 设计的通用框架，内核启动时只需要其 ELF 加载子集。Rust 版用更简单直接的实现替代。

### 3.7 ps_strings 处理

C 版在栈顶构造 `struct ps_strings`，包含 argv/envp 指针。Rust 版保留此行为：

```
PsStrings layout on the stack (BSD convention):

High address (stack_high):
  +--------------------+
  | struct ps_strings  |  ps_nargvstr=0, ps_argvstr, ps_envstr, ps_nenvstr=0
  +--------------------+
  | padding (int)      |  argc = 0
  | argv pointer       |  points to padding above
  | envp pointer       |  points after argv
  +--------------------+  <-- SP
```

**理由**：VM 的 C 启动代码期望栈上有 ps_strings 结构。Rust 版必须保持相同的栈布局。

### 3.8 特权分配：静态ID映射

C 版在 `main.c` 循环中为 schedulable 进程调用 `get_priv()` 并设置特权标志。Rust 版将此逻辑封装在 `PrivTable::assign_static()` 方法中：

```rust
impl PrivTable {
    pub fn assign_static(&mut self, proc_nr: ProcNr, priv_id: PrivId) -> Result<&mut KPriv, PrivError> {
        // C: get_priv(rp, static_priv_id(proc_nr))
        // 1. 检查 priv_id 是否在静态范围内
        // 2. 设置 s_proc_nr = proc_nr
        // 3. 返回 &mut KPriv 供调用方设置 flags
    }
}
```

**理由**：C 版的特权分配逻辑散落在 `main.c` 的 120 行循环中。Rust 版将其提取为 `PrivTable` 的方法，与 `ProcessTable` 保持一致的封装风格。

### 3.9 设计决策的替代方案

| 决策 | 替代方案 | 为何不选 |
|------|---------|---------|
| trait 返回纯值 | trait 接受 `&mut KProcess` | arch 层不能依赖 kernel 层，违反分层架构 |
| `ArchProcReset` 和 `ArchProcInit` 合并 | 单 trait 两个方法 | `reset` 在进程回收时独立调用，不需要 PC/SP |
| `load_vm_elf` 放在 `BootProcArch` | 放在 `ArchProcInit` | `load_vm_elf` 需要 `Paging` trait bound，`ArchProcInit` 不需要 |
| ELF 加载用 `object` crate | 手写 ELF 解析器 | `object` crate 已经过充分测试，支持 `no_std` |

---

## 4. 实现详解

### 4.1 核心类型定义

> 设计决策：§3.1 — 纯函数式 trait 设计，arch 层返回纯值

```rust
// os/arch/src/arch/proc_arch.rs

/// 新进程的初始寄存器状态（arch_proc_reset 的返回值）。
///
/// C: arch_proc_reset() 设置 PSW + 段选择子 + FPU 状态
#[derive(Debug, Clone, Copy)]
pub struct InitialRegState {
    /// 状态寄存器（RFLAGS/x86-64, SPSR/aarch64, sstatus/riscv64）
    pub status: u64,
    /// 段选择子（x86-64: CS/DS/SS/ES/FS/GS; aarch64/riscv64: 全零）
    pub segment_selectors: SegmentSelectors,
    /// 是否需要清零 FPU 状态（x86-64: 用户进程=true; aarch64/riscv64: false）
    pub fpu_needs_zero: bool,
}

/// x86-64 段选择子。aarch64/riscv64 使用 Default（全零）。
#[derive(Debug, Clone, Copy, Default)]
pub struct SegmentSelectors {
    pub cs: u64,
    pub ds: u64,
    pub ss: u64,
    pub es: u64,
    pub fs: u64,
    pub gs: u64,
}

/// arch_proc_init 的返回值：进程的初始 PC、SP、ps_strings 寄存器值。
///
/// C: arch_proc_init() 设置 pr->p_reg.pc, pr->p_reg.sp, pr->p_reg.bx/retreg/a0
#[derive(Debug, Clone, Copy)]
pub struct InitialRegs {
    /// 程序计数器（入口点）
    pub pc: VirBytes,
    /// 栈指针
    pub sp: VirBytes,
    /// ps_strings 寄存器的值（x86-64: rbx, aarch64: r0, riscv64: a0）
    pub ps_strings_reg: u64,
}

/// VM ELF 加载结果。
pub struct VmLoadResult {
    pub pc: VirBytes,
    pub sp: VirBytes,
    pub ps_strings: VirBytes,
    pub allocated_bytes: usize,
}
```

### 4.2 ArchProcReset 实现

#### x86-64

```rust
// os/arch/src/x86_64/proc_arch.rs

/// 64-bit x86-64 初始 RFLAGS
/// C: INIT_TASK_PSW = 0x1200 (32-bit) → 0x1202 (64-bit, bit1=1)
const INIT_TASK_PSW: u64 = 0x1202;
/// C: INIT_PSW = 0x0200 (32-bit) → 0x0202 (64-bit, bit1=1)
const INIT_PSW: u64 = 0x0202;
/// C: USER_CS_SELECTOR = 0x1B (GDT index 3, RPL=3)
const USER_CS_SELECTOR: u64 = 0x1B;
/// C: USER_DS_SELECTOR = 0x23 (GDT index 4, RPL=3)
const USER_DS_SELECTOR: u64 = 0x23;

impl ArchProcReset for X86_64ProcArch {
    fn initial_reg_state(is_kernel: bool, proc_nr: i32) -> InitialRegState {
        // C: arch_system.c:146-192
        //
        // 1. 设置 PSW/RFLAGS
        //    - 内核任务：INIT_TASK_PSW (IOPL=1, IF=1, bit1=1)
        //    - 用户进程：INIT_PSW (IOPL=0, IF=1, bit1=1)
        // 2. 设置段选择子：CS=USER_CS_SELECTOR, DS/SS/ES/FS/GS=USER_DS_SELECTOR
        // 3. 用户进程需要清零 FPU 状态

        let status = if is_kernel { INIT_TASK_PSW } else { INIT_PSW };

        let segments = SegmentSelectors {
            cs: USER_CS_SELECTOR,
            ds: USER_DS_SELECTOR,
            ss: USER_DS_SELECTOR,
            es: USER_DS_SELECTOR,
            fs: USER_DS_SELECTOR,
            gs: USER_DS_SELECTOR,
        };

        // C: if(pr->p_nr >= 0) memset(fpu_state[pr->p_nr], 0, FPU_XFP_SIZE)
        let fpu_needs_zero = !is_kernel;

        InitialRegState { status, segment_selectors: segments, fpu_needs_zero }
    }
}
```

#### aarch64

```rust
// os/arch/src/arm64/proc_arch.rs

/// AArch64 INIT_TASK_PSR: EL1h, F/I/A/D masked
/// C: 32-bit ARM INIT_TASK_PSR = 0x53 (SVC32) → 64-bit = 0x3C5 (EL1h)
const INIT_TASK_PSR: u64 = 0x000003C5;
/// AArch64 INIT_PSR: EL0t
/// C: 32-bit ARM INIT_PSR = 0x50 (USR32) → 64-bit = 0x0 (EL0t)
const INIT_PSR: u64 = 0x00000000;

impl ArchProcReset for AArch64ProcArch {
    fn initial_reg_state(is_kernel: bool, proc_nr: i32) -> InitialRegState {
        // C: earm/arch_system.c:42-60
        // 设置 PSR: EL0t (用户) 或 EL1h (内核任务)
        // 无段选择子，无 FPU 初始化（lazy switch）
        let _ = proc_nr;
        let status = if is_kernel { INIT_TASK_PSR } else { INIT_PSR };
        InitialRegState {
            status,
            segment_selectors: SegmentSelectors::default(),
            fpu_needs_zero: false, // ARM: FPU lazy init
        }
    }
}
```

#### riscv64

```rust
// os/arch/src/riscv64/proc_arch.rs

/// RISC-V sstatus: SPP=1 (S-mode), SPIE=0
const INIT_TASK_SSTATUS: u64 = 0x00000100;
/// RISC-V sstatus: SPP=0 (U-mode), SPIE=1
const INIT_SSTATUS: u64 = 0x00000020;

impl ArchProcReset for Riscv64ProcArch {
    fn initial_reg_state(is_kernel: bool, proc_nr: i32) -> InitialRegState {
        // 无 C 源码 — 类比 aarch64 设计
        // SPP=1/SPIE=0 (内核) 或 SPP=0/SPIE=1 (用户)
        let _ = proc_nr;
        let status = if is_kernel { INIT_TASK_SSTATUS } else { INIT_SSTATUS };
        InitialRegState {
            status,
            segment_selectors: SegmentSelectors::default(),
            fpu_needs_zero: false, // RISC-V: FPU lazy init
        }
    }
}
```

### 4.3 ArchProcInit 实现

> 设计决策：§3.4 — 返回 InitialRegs 而非修改进程结构体

#### x86-64

```rust
impl ArchProcInit for X86_64ProcArch {
    fn init_regs(is_kernel: bool, proc_nr: i32, pc: VirBytes, sp: VirBytes,
                 ps_strings: VirBytes) -> InitialRegs {
        // C: memory.c:722-733
        //   arch_proc_reset(pr);
        //   pr->p_reg.pc = ip;    // rip
        //   pr->p_reg.sp = sp;    // rsp
        //   pr->p_reg.bx = ps_str; // rbx = ps_strings
        //
        // 注意：reset 的返回值由调用方在调用 init_regs 之前应用，
        // 因为 ArchProcInit: ArchProcReset，调用方可以：
        //   let reg_state = <Arch as ArchProcReset>::initial_reg_state(...);
        //   proc.apply_reg_state(reg_state);
        //   let regs = <Arch as ArchProcInit>::init_regs(...);
        //   proc.set_pc_sp(regs);
        let _ = (is_kernel, proc_nr);
        InitialRegs {
            pc,
            sp,
            ps_strings_reg: ps_strings.0, // rbx value
        }
    }
}
```

#### aarch64

```rust
impl ArchProcInit for AArch64ProcArch {
    fn init_regs(is_kernel: bool, proc_nr: i32, pc: VirBytes, sp: VirBytes,
                 ps_strings: VirBytes) -> InitialRegs {
        // C: earm/memory.c:627-638
        //   pr->p_reg.pc = ip;        // pc
        //   pr->p_reg.sp = sp;        // sp
        //   pr->p_reg.retreg = ps_str; // r0 = ps_strings
        let _ = (is_kernel, proc_nr);
        InitialRegs {
            pc,
            sp,
            ps_strings_reg: ps_strings.0, // r0 value
        }
    }
}
```

#### riscv64

```rust
impl ArchProcInit for Riscv64ProcArch {
    fn init_regs(is_kernel: bool, proc_nr: i32, pc: VirBytes, sp: VirBytes,
                 ps_strings: VirBytes) -> InitialRegs {
        // 类比 aarch64 设计
        //   sepc = pc
        //   sp = sp
        //   a0 = ps_strings
        let _ = (is_kernel, proc_nr);
        InitialRegs {
            pc,
            sp,
            ps_strings_reg: ps_strings.0, // a0 value
        }
    }
}
```

### 4.4 BootProcArch 实现：load_vm_elf

所有架构共享相同的 `load_vm_elf` 逻辑，差异仅在 `ArchProcInit`：

```rust
// os/arch/src/x86_64/proc_arch.rs

impl BootProcArch for X86_64ProcArch {
    fn load_vm_elf<P: Paging>(
        module: &BootModule,
        kernel_info: &KernelInfo,
        paging: &mut P,
    ) -> VmLoadResult {
        // C: protect.c:388-456
        //
        // 1. 解析 ELF 头获取入口点和程序头
        // 2. 遍历 PT_LOAD 段：分配物理页、映射到 bootstrap 页表、复制段数据
        // 3. 在栈顶设置 ps_strings 结构
        // 4. 返回入口点、栈指针、ps_strings 地址

        // Step 1: 解析 ELF
        let elf_data = unsafe {
            core::slice::from_raw_parts(
                module.start_addr.as_ptr(),
                module.len,
            )
        };
        let elf = elf::ElfBytes::<elf::endian::AnyEndian>::minimal_parse(elf_data)
            .expect("VM ELF parse failed");

        let entry = elf.ehdr.e_entry;

        // Step 2: 加载 PT_LOAD 段
        let mut allocated = 0usize;
        if let Some(segments) = elf.segments() {
            for phdr in segments
                .iter()
                .filter(|ph| ph.p_type == elf::abi::PT_LOAD)
            {
                let vaddr = VirBytes(phdr.p_vaddr);
                let memsz = phdr.p_memsz as usize;
                let filesz = phdr.p_filesz as usize;
                let offset = phdr.p_offset as usize;

                // 分配物理页并映射到 bootstrap 页表
                for page_offset in (0..memsz).step_by(P::PAGE_SIZE) {
                    let page_vaddr = VirBytes(vaddr.0 + page_offset as u64);
                    // C: pg_map(PG_ALLOCATEME, vaddr, vaddr+len, &kinfo)
                    // 使用 Paging trait 分配和映射
                    paging.map_alloc(page_vaddr, P::PAGE_SIZE)
                        .expect("VM ELF page allocation failed");
                    allocated += P::PAGE_SIZE;
                }

                // 复制段数据（仅 filesz 部分，其余已清零）
                if filesz > 0 {
                    let src = &elf_data[offset..offset + filesz];
                    // SAFETY: vaddr was just mapped above
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            src.as_ptr(),
                            vaddr.0 as *mut u8,
                            filesz,
                        );
                    }
                }
            }
        }

        // Step 3: 设置 ps_strings 栈布局
        let stack_high = kernel_info.user_sp;
        let stack_size = 64 * 1024; // C: execi.stack_size = 64 * 1024
        let sp = VirBytes(stack_high.0 - stack_size as u64);

        // C: sp -= sizeof(struct ps_strings) + 3 words
        let ps_strings_size = core::mem::size_of::<PsStrings>();
        let ps_strings_addr = VirBytes(sp.0 - ps_strings_size as u64);
        let final_sp = VirBytes(ps_strings_addr.0
            - 3 * core::mem::size_of::<u64>() as u64);

        VmLoadResult {
            pc: VirBytes(entry),
            sp: final_sp,
            ps_strings: ps_strings_addr,
            allocated_bytes: allocated,
        }
    }
}

/// BSD ps_strings 结构体（栈顶布局）。
#[repr(C)]
struct PsStrings {
    ps_argvstr: u64,   // char **
    ps_nargvstr: i32,  // int
    ps_envstr: u64,    // char **
    ps_nenvstr: i32,   // int
}
```

### 4.5 init_proc_and_boot() — 主流程

```rust
// os/kernel/src/lib.rs

/// Phase C of kmain: proc_init + arch_boot_proc.
///
/// C: proc.c:119-160 (proc_init) + main.c:157-282 (boot image loop)
#[cfg(not(feature = "mock"))]
fn init_proc_and_boot(kernel_info: &KernelInfo) {
    use minix_arch::{ArchProcReset, BootProcArch, CurrentBootProcArch};
    use crate::proc::{ProcNr, rts, proc_nr};
    use crate::proc_table::{ProcessTable, NR_TASKS};
    use crate::kpriv::{PrivTable, priv_flag_set};

    // ── Step 1: proc_init 等价 ──
    // C: proc_init() — proc.c:119-167
    let mut proc_table = ProcessTable::new();
    let mut priv_table = PrivTable::new();

    // C: NR_BOOT_MODULES check — main.c:160-162
    assert_eq!(
        kernel_info.boot_modules.len(),
        NR_BOOT_MODULES,
        "expected {} boot modules, found {}",
        NR_BOOT_MODULES, kernel_info.boot_modules.len()
    );

    // ── Step 2: boot image 循环 ──
    // C: for (i=0; i < NR_BOOT_PROCS; ++i) — main.c:164
    for (i, module) in kernel_info.boot_modules.iter().enumerate() {
        let nr: ProcNr = if i < NR_TASKS {
            (i as ProcNr) - (NR_TASKS as ProcNr)
        } else {
            (i - NR_TASKS) as ProcNr
        };

        let proc = proc_table.get_mut(nr)
            .expect("boot proc: invalid process number");

        // C: strlcpy(rp->p_name, ip->proc_name, ...)
        proc.set_boot_name(module.name);

        let is_kernel = nr < 0;
        let is_vm = nr == proc_nr::VM_PROC_NR;
        let is_root_sys = nr == proc_nr::RS_PROC_NR;
        let schedulable = is_kernel || is_root_sys || is_vm;

        // ── 特权分配 ──
        // C: main.c:176-254
        if schedulable {
            // C: get_priv(rp, static_priv_id(proc_nr)) — main.c:200
            let priv_id = priv_table.assign_static(nr)
                .expect("assign_static: static priv slot occupied");

            // C: priv(rp)->s_flags = VM_F/TSK_F/RSYS_F — main.c:179-224
            if is_vm {
                priv_table.configure_boot_priv(
                    priv_id,
                    priv_flag_set::VM_F,  // s_flags
                    0,  // s_init_flags
                    0,  // s_trap_mask: SRV_T (TODO: fill_sendto_mask)
                    0,  // s_ipc_to
                    [0; 2],  // s_k_call_mask: SRV_KC (TODO)
                    Endpoint::from_generation_slot(0, nr),  // SELF
                );
            } else if is_kernel {
                let flags = if nr == proc_nr::IDLE {
                    priv_flag_set::IDL_F
                } else {
                    priv_flag_set::TSK_F
                };
                priv_table.configure_boot_priv(
                    priv_id, flags, 0, 0, 0, [0; 2], Endpoint::NONE,
                );
            } else {
                priv_table.configure_boot_priv(
                    priv_id,
                    priv_flag_set::RSYS_F,  // s_flags
                    0,  // s_init_flags: SRV_I
                    0,  // s_trap_mask: SRV_T
                    0,  // s_ipc_to: SRV_M
                    [0; 2],  // s_k_call_mask: SRV_KC
                    Endpoint::from_generation_slot(0, nr),  // SRV_SM: SELF
                );
            }
        } else {
            // C: RTS_SET(rp, RTS_NO_PRIV | RTS_NO_QUANTUM) — main.c:226
            proc.p_rts_flags.set(rts::NO_PRIV | rts::NO_QUANTUM);
        }

        // ── 架构特定初始化 ──
        // C: arch_boot_proc(ip, rp) — main.c:257
        //
        // Step A: 设置初始寄存器状态（PSW/PSR/sstatus，段选择子，FPU）
        // C: arch_proc_reset(pr) — arch_system.c:146-192/42-60
        let reg_state = CurrentBootProcArch::initial_reg_state(is_kernel, nr);
        proc.set_boot_initial_reg_state(reg_state.status, reg_state.fpu_needs_zero);

        // Step B: 用户态引导进程设置 PC/SP/ps_strings
        // 内核任务跳过（C: if(rp->p_nr < 0) return; — protect.c:393）
        if !is_kernel {
            let (pc, sp, ps_strings) = if is_vm {
                // C: arch_boot_proc for VM — protect.c:395-452
                // TODO(P0): 集成 load_vm_elf() 后使用真实值
                (VirBytes(0), VirBytes(0), VirBytes(0))
            } else {
                // 其他用户进程：ELF 由 RS 在运行时加载
                (VirBytes(0), VirBytes(0), VirBytes(0))
            };

            let init_regs = CurrentBootProcArch::init_regs(
                is_kernel, nr, pc, sp, ps_strings,
            );
            proc.set_boot_pc_sp(init_regs.pc, init_regs.sp, init_regs.ps_strings_reg);
        }

        // ── VM inhibit ──
        // C: main.c:267-270
        if nr != proc_nr::VM_PROC_NR && nr >= 0 {
            proc.p_rts_flags.set(rts::VMINHIBIT | rts::BOOTINHIBIT);
        }

        // C: rp->p_rts_flags |= RTS_PROC_STOP — main.c:272
        proc.p_rts_flags.set(rts::PROC_STOP);
        // C: rp->p_rts_flags &= ~RTS_SLOT_FREE — main.c:273
        proc.p_rts_flags.clear(rts::SLOT_FREE);
    }
}
```

**关键差异与 C 版**：

| 方面 | C 版 | Rust 版 |
|------|------|---------|
| 进程名设置 | `strlcpy(rp->p_name, ...)` 直接赋值 | `proc.set_boot_name()` 封装方法 |
| 特权分配 | `get_priv(rp, static_priv_id(proc_nr))` 返回 `int` | `assign_static(nr)` 返回 `Option<PrivId>` |
| 特权配置 | 内联 `priv(rp)->s_flags = ...` | `configure_boot_priv()` 批量设置 |
| 寄存器初始化 | `arch_proc_init()` 直接修改 `p_reg` | `initial_reg_state()` + `init_regs()` 返回纯值，通过 setter 写入 |
| ELF 加载 | `arch_boot_proc()` 内联完成 | `load_vm_elf()` trait 方法（TODO: 集成 Paging） |
| IPC 掩码 | `fill_sendto_mask()` 构建位图 | TODO(P1): 后续实现 |
| 内核调用掩码 | `priv(rp)->s_k_call_mask[j] = ~0` | TODO(P1): 后续实现 |

### 4.6 KProcess 新增方法

```rust
// os/kernel/src/proc.rs

impl KProcess {
    /// 设置进程名。对应 C 的 `strlcpy(rp->p_name, name, ...)`。
    /// C: main.c:170, protect.c:441
    pub fn set_boot_name(&mut self, name: &str) {
        self.p_name = ProcName::from_str(name);
    }

    /// 应用 ArchProcReset 返回的初始寄存器状态。
    ///
    /// 将 arch 层返回的纯值写入进程结构体。
    /// 存储架构特定的初始状态寄存器（PSW/PSR/sstatus）和段选择子。
    ///
    /// C: arch_proc_reset(pr) — arch_system.c:146-192 (x86), arch_system.c:42-60 (ARM)
    pub fn set_boot_initial_reg_state(&mut self, status: u64, _fpu_needs_zero: bool) {
        self.initial_status = status;
        // FPU 清零由 arch 层在设置 trap frame 时处理。
        // x86-64: fpu_needs_zero=true 表示 arch 层在首次执行前清零异常帧中的 FPU 保存区。
        // aarch64/riscv64: FPU 延迟初始化 (fpu_needs_zero 始终为 false)。
        let _ = _fpu_needs_zero;
    }

    /// 设置初始 PC、SP 和 ps_strings 寄存器值。
    ///
    /// 对应 ArchProcInit::init_regs() 的返回值。
    ///
    /// C: pr->p_reg.pc = ip; pr->p_reg.sp = sp; pr->p_reg.bx = ps_str;
    ///    — protect.c:445-447 (x86), protect.c:169-171 (ARM)
    pub fn set_boot_pc_sp(&mut self, pc: VirBytes, sp: VirBytes, ps_strings_reg: u64) {
        self.initial_pc = pc;
        self.initial_sp = sp;
        self.initial_ps_strings_reg = ps_strings_reg;
    }
}
```

**KProcess 新增字段**（见 `os/kernel/src/proc.rs`）：

```rust
pub struct KProcess {
    // ... existing fields ...
    // ── Boot-time initial register state (05-proc-init-boot-proc.md §3.2, §3.3) ──
    /// 初始 PC（程序计数器/指令指针）。
    pub initial_pc: VirBytes,
    /// 初始 SP（栈指针）。
    pub initial_sp: VirBytes,
    /// 初始 ps_strings 寄存器值。
    pub initial_ps_strings_reg: u64,
    /// 初始状态寄存器值（PSW/PSR/sstatus）。
    pub initial_status: u64,
}
```

**设计理由**：这些字段存储架构特定的初始寄存器值，使得调度器在首次调度进程时，可以据此设置 trap frame。C 版将初始值直接写入 `p_reg` 字段，但 Rust 版将"初始值"与"运行时寄存器"分离，因为调度器在不同阶段有不同的寄存器设置需求。

### 4.7 PrivTable::assign_static() 和 configure_boot_priv()

```rust
// os/kernel/src/kpriv.rs

impl PrivTable {
    /// 为引导进程分配静态特权结构。
    ///
    /// 对应 C 的 `get_priv(rp, static_priv_id(proc_nr))`。
    /// static_priv_id 映射：`priv_id = NR_TASKS + proc_nr`（proc_nr >= 0）。
    /// 内核任务（proc_nr < 0）使用相同公式，因为其 priv_id 由静态 slot 布局决定。
    ///
    /// C: get_priv() — system.c:272-311, static_priv_id() — priv.h:12
    ///
    /// # 返回值
    /// `Some(priv_id)` 成功，`None` 如果 priv_id 超出范围或 slot 已被占用。
    pub fn assign_static(&mut self, proc_nr: ProcNr) -> Option<PrivId> {
        let priv_id = if proc_nr < 0 {
            (NR_TASKS as ProcNr + proc_nr) as PrivId
        } else {
            (NR_TASKS + proc_nr as PrivId) as PrivId
        };
        let priv_ = self.get_mut(priv_id)?;
        if priv_.s_proc_nr.is_some() {
            return None;
        }
        priv_.s_proc_nr = Some(proc_nr);
        Some(priv_id)
    }

    /// 为引导进程设置特权标志、trap mask、IPC mask、内核调用 mask 和调度参数。
    /// 对应 C 中 main.c:178-248 的按类型特权设置。
    pub fn configure_boot_priv(
        &mut self, priv_id: PrivId, flags: u16, init_flags: i32,
        trap_mask: u16, ipc_to: u64, k_call_mask: [u32; 2], sig_mgr: Endpoint,
    ) {
        if let Some(priv_) = self.get_mut(priv_id) {
            priv_.s_flags = flags;
            priv_.s_init_flags = init_flags;
            priv_.s_trap_mask = trap_mask;
            priv_.s_ipc_to = ipc_to;
            priv_.s_k_call_mask = k_call_mask;
            priv_.s_sig_mgr = sig_mgr;
        }
    }
}
```

**设计理由**：C 版的特权分配和配置逻辑散落在 `main.c` 的 120 行循环中，不同类型进程（VM、内核任务、RS）有不同的特权标志组合。Rust 版将其拆分为两个步骤：
1. `assign_static`：分配 slot（建立 `proc_nr ↔ priv_id` 关联）
2. `configure_boot_priv`：设置特权属性（flags、trap_mask、IPC 权限等）

这使得调用方可以明确控制两个独立的操作。`assign_static` 返回 `PrivId` 而非 `&mut KPriv`，避免借用检查器限制调用方后续对 `PrivTable` 的访问。

---

## 5. 测试要点

### 5.1 ProcessTable 测试

| 测试 | 验证 | C 对应 |
|------|------|--------|
| `new()` 所有 slot 为 `SLOT_FREE` | 初始状态正确 | `rp->p_rts_flags = RTS_SLOT_FREE` |
| `new()` p_nr 正确 | `-NR_TASKS` 到 `NR_PROCS-1` | `rp->p_nr = i` |
| `new()` p_endpoint 正确 | `_ENDPOINT(0, p_nr)` | `rp->p_endpoint = _ENDPOINT(0, rp->p_nr)` |
| IDLE 进程 `RTS_PROC_STOP` | IDLE 不可调度 | `ip->p_rts_flags |= RTS_PROC_STOP` |

### 5.2 ArchProcReset 测试

| 测试 | 验证 | 架构 |
|------|------|------|
| 用户进程 PSW/PSR/sstatus 正确 | `INIT_PSW`/`INIT_PSR`/`INIT_SSTATUS` | all |
| 内核任务 PSW/PSR/sstatus 正确 | `INIT_TASK_PSW`/`INIT_TASK_PSR`/`INIT_TASK_SSTATUS` | all |
| x86-64 段选择子正确 | CS=USER_CS_SELECTOR, DS/SS=USER_DS_SELECTOR | x86-64 |
| x86-64 FPU 状态清零 | `fpu_needs_zero = !is_kernel` | x86-64 |
| aarch64 无段选择子 | `SegmentSelectors::default()` | aarch64 |
| riscv64 无段选择子 | `SegmentSelectors::default()` | riscv64 |

### 5.3 ArchProcInit 测试

| 测试 | 验证 | 架构 |
|------|------|------|
| PC/SP 设置正确 | `InitialRegs.pc/sp` 等于传入值 | all |
| x86-64 ps_strings 寄存器 | `ps_strings_reg` = ps_strings.0 | x86-64 |
| aarch64 ps_strings 寄存器 | `ps_strings_reg` = ps_strings.0 | aarch64 |
| riscv64 ps_strings 寄存器 | `ps_strings_reg` = ps_strings.0 | riscv64 |

### 5.4 BootProcArch 测试

| 测试 | 验证 |
|------|------|
| VM ELF 加载成功 | `VmLoadResult.pc` = ELF entry, `sp` = user stack |
| ps_strings 栈布局正确 | `VmLoadResult.ps_strings` 指向正确栈地址 |
| ELF PT_LOAD 段分配 | `allocated_bytes` >= 总段大小 |
| 无效 ELF panic | `expect("VM ELF parse failed")` |

### 5.5 init_proc_and_boot 集成测试

| 测试 | 验证 | C 对应 |
|------|------|--------|
| schedulable 进程获得特权 | `PrivTable::assign_static()` 成功 | `get_priv(rp, static_priv_id(proc_nr))` |
| 非 schedulable 进程 RTS_NO_PRIV | `p_rts_flags` 包含 `NO_PRIV \| NO_QUANTUM` | `RTS_SET(rp, ...)` |
| VM 特权标志 VM_F | `kpriv.s_flags` 包含 `VM_F` | `priv(rp)->s_flags = VM_F` |
| 非 VM 用户进程 VMINHIBIT | `p_rts_flags` 包含 `VMINHIBIT \| BOOTINHIBIT` | `rp->p_rts_flags \|= ...` |
| 所有进程 PROC_STOP | `p_rts_flags` 包含 `PROC_STOP` | `rp->p_rts_flags \|= RTS_PROC_STOP` |
| NR_BOOT_MODULES 不匹配 | `assert_eq!` panic | `panic("expecting %d...")` |
| 内核任务跳过 ELF 加载 | `load_vm_elf` 不被调用 | `if(rp->p_nr < 0) return` |

---

## 6. 参见

- [03-kmain-cstart.md](03-kmain-cstart.md) — kmain 入口与保护模式初始化
- [04-clock-interrupt-init.md](04-clock-interrupt-init.md) — 时钟与中断初始化
- [06-cross-space-init.md](06-cross-space-init.md) — 阶段 D：跨地址空间初始化
- [tmp-06-proc-struct.md](tmp-06-proc-struct.md) — 进程结构体详细设计
- [tmp-11-privilege.md](tmp-11-privilege.md) — 特权系统详细设计
- [99-global-concepts.md](99-global-concepts.md) — 全局概念定义
- `minix3/minix/kernel/proc.c:119-160` — proc_init 源码
- `minix3/minix/kernel/main.c:157-282` — boot image 循环源码
- `minix3/minix/kernel/arch/i386/arch_system.c:146-192` — x86 arch_proc_reset
- `minix3/minix/kernel/arch/earm/arch_system.c:42-60` — ARM arch_proc_reset
- `minix3/minix/kernel/arch/i386/memory.c:722-733` — x86 arch_proc_init
- `minix3/minix/kernel/arch/earm/memory.c:627-638` — ARM arch_proc_init
- `minix3/minix/kernel/arch/i386/protect.c:379-456` — x86 arch_boot_proc
- `minix3/minix/kernel/arch/earm/protect.c:106-183` — ARM arch_boot_proc
- `minix3/minix/kernel/system.c:272-380` — get_priv + fill_sendto_mask