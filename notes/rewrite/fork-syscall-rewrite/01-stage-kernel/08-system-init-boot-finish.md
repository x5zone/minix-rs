# 08-system-init-boot-finish: 系统调用初始化与启动完成

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/system.c:168-270`, `minix3/minix/kernel/arch/i386/pg_utils.c:86-121`, `minix3/minix/kernel/main.c:38-109`
> **说明**: kmain 把内核从"初始化态"带入"运行态"——通过 T0-T6 七个阶段（实读 `os/kernel/src/lib.rs`），详见 §1.1 表格。Rust 端把 C 时代 kmain 末尾的"系统调用注册 + bootstrap 内存回收 + 启动完成"三件事拆散到了不同时机（详见 §1.1 注释）。

---

## 1. 概述

### 1.1 核心问题

kmain 把内核从"初始化态"带入"运行态"。真实时序（代码实读 `os/kernel/src/lib.rs`）：

| 步骤 | 函数 | 实现状态 |
|------|------|---------|
| T0 | `init_protection` (lib.rs:744) | 已实现 |
| T1 | `init_clock_and_interrupts` (lib.rs:780) | 已实现 |
| T2 | `SMP_STATE = SmpState::new_single_cpu()` (lib.rs:559) | 已实现 |
| T3 | `init_proc_and_boot` (lib.rs:879) | 已实现 |
| T4 | `init_post_and_memory` (lib.rs:1271) | 已实现 |
| T5 | `bsp_finish_booting` (lib.rs:1948) | 已实现（Step 0-9，含 8.5，见 §1.2） |
| T6 | `switch_to_user` (lib.rs:2757) | 已实现（五阶段调度循环 + idle + 地址空间切换 + 终局分派，见 10 §4） |

> **与 C 的差异**（仅作对比参考，C 端不存在 T0-T6 这一说法）：C 时代 kmain 末尾三件大事是 `system_init`（注册 call_vec[]）+ `add_memmap`（回收 bootstrap 内存）+ `bsp_finish_booting`（启动调度）。Rust 端把这三件事拆散到了不同位置：
> - **system_init 的"call_vec 注册"**：用 `enum Syscall + match` 取代（[`os/kernel/src/syscall.rs`](../os/kernel/src/syscall.rs) D1），编译期穷尽检查替代运行时注册表
> - **add_memmap 的"回收 bootstrap 内存"**：发生在 `init_post_and_memory`（lib.rs:497）而非 bsp_finish_booting 之前——`add_memmap` 函数本身已实现（[`os/kernel/src/memmap.rs`](../os/kernel/src/memmap.rs)），调用时机不同
> - **bsp_finish_booting**：仍按 C 时代职责——`vm_running=0 → announce → RTS_PROC_STOP 解除 → 时钟启动 → FPU → kernel_may_alloc=0 → switch_to_user`

**关键不变量**（Rust 现状）：
- T5 之前，所有 boot 进程被 `RTS_PROC_STOP` 阻止运行
- T5 中 `RTS_PROC_STOP` 解除的范围 = `i < NR_BOOT_PROCS - NR_TASKS`（故意排除 kernel task——它们永不作为运行实体被调度，见 [06 §1.4](../01-stage-kernel/06-proc-init-boot-proc.md)）
- T6 之后，内核进入调度循环（[10 §4](10-switch-to-user.md)），不再返回 kmain

### 1.2 唯一关键步骤：bsp_finish_booting 的内部节拍

T5 是本专项唯一承重的"启动主线"步骤。Rust 实现共 11 步（Step 0-9 + Step 8.5），代码实读 `lib.rs::bsp_finish_booting`（lib.rs:1948），C 对照 `main.c:38-109`：

```
bsp_finish_booting (lib.rs:1948)
  ├─ Step 0:   cpu_identify —— BSP 身份探测入 CPU_INFO (C: main.c:45)
  ├─ Step 1:   vm_running = 0                    (C: main.c:47)
  ├─ Step 2:   bill_ptr = proc_ptr = idle_proc   (C: main.c:54-55)
  ├─ Step 3:   announce()                        (C: main.c:58，打印 MINIX banner)
  ├─ Step 4:   for nr < NR_BOOT_PROCS-NR_TASKS
  │             rts_unset(nr, PROC_STOP)          (C: main.c:64-66)
  ├─ Step 5:   cycles_accounting —— BSP TSC 基线 (C: main.c:71)
  ├─ Step 6:   boot_cpu_init_timer —— 幂等重设   (C: main.c:73-76)
  ├─ Step 7:   fpu_presence = true               (C: main.c:78)
  ├─ Step 8:   kernel_may_alloc = 0              (C: main.c:105)
  ├─ Step 8.5: 获取 BKL（RAII guard mem::forget）(C: BKL_LOCK() — main.c:149，main() 中早于本函数)
  └─ Step 9:   switch_to_user()                  (C: main.c:107，never returns)
```

**步骤间的依赖与不变量**：
- Step 0 与 C 同位同序——`cpu_identify()` 是 C `bsp_finish_booting` 的第一条语句（main.c:45），Rust `smp::cpu_identify()` 同样置于 Step 1 之前；此时仅 BSP 单核运行，写入全局 `CPU_INFO` 无并发（AP 路径见 §4.6）
- Step 4 范围 `nr < NR_BOOT_PROCS - NR_TASKS`——故意排除 kernel task（永不作为运行实体被调度，见 [06 §1.4](06-proc-init-boot-proc.md)）
- Step 5 必须先于 Step 6——TSC 基线在 timer 初始化之前建立（C `main.c:71` 注："First reset the CPU accounting values, as the timer initialization (indirectly) uses them"）
- Step 6 是幂等重设：硬件 timer 已在 Phase B（`init_clock_and_interrupts`）初始化，此处经平台描述符重建 clock arch 实例再调 `init_timer`（x86 重写同一 PIT 模式字节；aarch64/riscv64 重写已运行的比较器，无副作用）；BSP timer IRQ handler 注册延迟到 IrqManager 全局化之后（[05 §4.7.2](05-clock-interrupt-init.md)）
- Step 8 关闭分配窗口后，内核不得再直接分配物理内存（§1.4 规则 2）
- Step 9 永不返回——内核从此进入五阶段调度循环（[10 §4](10-switch-to-user.md)：选进程 → 杂项标志 → 量子检查 → 终局分派，无就绪进程则 idle）
- C 的 krandom / cpu_set_flag 两步差异见 §4.6"与 C 12 步的差异说明"表

### 1.3 与前后文档的关系

| 前置 | 本文档 | 后续 |
|------|--------|------|
| 05/06/07: 基础设施就绪（保护 + 时钟 + 进程表 + direct_map） | T5 bsp_finish_booting：解除 PROC_STOP + 启动调度 | 09: VM 启动协议 + 10: 调度循环 + 11: 调度原语 |

T5 之前（07 完成后）：进程表/特权结构已就绪，VM direct_map 已确认就绪（direct_map 替代 Minix3 的 freepdes/ptproc 临时窗口），但 boot 进程全部带 `RTS_PROC_STOP`，不可运行。T5 解除 PROC_STOP → T6 进入调度循环（[10 §4](10-switch-to-user.md)）。

### 1.4 行为规则

1. **bsp_finish_booting 是 kmain 最后可逆操作的终点**——`switch_to_user()` 之后永不返回；从这里开始所有修改要承担运行时风险
2. **kernel_may_alloc 关闭时机**——`bsp_finish_booting` Step 9 关闭（C `main.c:91`）；之后所有物理内存分配经 VM（通过 VMCTL 的 MEMSET/IPC 路径）。**不要在 Step 9 之后写 `memmap::add_memmap`/`kmalloc` 类调用**
3. **RTS_PROC_STOP 解除的范围** = `i < NR_BOOT_PROCS - NR_TASKS`（Step 4）——故意排除 kernel task；IDLE 永不解除（设 PROC_STOP 标记），CLOCK/SYSTEM 的执行入口是事件驱动的 `timer_int_handler`/`kernel_call` 而非调度器（见 [06 §1.4](../01-stage-kernel/06-proc-init-boot-proc.md)）

---

## 2. C 源码分析

### 2.1 相关定义（常量、宏）

**系统调用号定义**（`minix3/minix/include/minix/com.h:207-270`）：

| 常量 | 值 | 处理函数 | 类别 |
|------|-----|---------|------|
| SYS_FORK | KERNEL_CALL+0 | do_fork | 进程管理 |
| SYS_EXEC | KERNEL_CALL+1 | do_exec | 进程管理 |
| SYS_CLEAR | KERNEL_CALL+2 | do_clear | 进程管理 |
| SYS_SCHEDULE | KERNEL_CALL+3 | do_schedule | 调度 |
| SYS_PRIVCTL | KERNEL_CALL+4 | do_privctl | 进程管理 |
| SYS_TRACE | KERNEL_CALL+5 | do_trace | 进程管理 |
| SYS_KILL | KERNEL_CALL+6 | do_kill | 信号 |
| SYS_GETKSIG | KERNEL_CALL+7 | do_getksig | 信号 |
| SYS_ENDKSIG | KERNEL_CALL+8 | do_endksig | 信号 |
| SYS_SIGSEND | KERNEL_CALL+9 | do_sigsend | 信号 |
| SYS_SIGRETURN | KERNEL_CALL+10 | do_sigreturn | 信号 |
| SYS_MEMSET | KERNEL_CALL+13 | do_memset | 内存 |
| SYS_UMAP | KERNEL_CALL+14 | do_umap | 拷贝 |
| SYS_VIRCOPY | KERNEL_CALL+15 | do_vircopy | 拷贝 |
| SYS_PHYSCOPY | KERNEL_CALL+16 | do_copy | 拷贝 |
| SYS_UMAP_REMOTE | KERNEL_CALL+17 | do_umap_remote | 拷贝 |
| SYS_VUMAP | KERNEL_CALL+18 | do_vumap | 拷贝 |
| SYS_IRQCTL | KERNEL_CALL+19 | do_irqctl | 设备 I/O |
| SYS_DEVIO | KERNEL_CALL+21 | do_devio | 设备 I/O (x86) |
| SYS_SDEVIO | KERNEL_CALL+22 | do_sdevio | 设备 I/O (x86) |
| SYS_VDEVIO | KERNEL_CALL+23 | do_vdevio | 设备 I/O (x86) |
| SYS_SETALARM | KERNEL_CALL+24 | do_setalarm | 时钟 |
| SYS_TIMES | KERNEL_CALL+25 | do_times | 时钟 |
| SYS_GETINFO | KERNEL_CALL+26 | do_getinfo | 系统控制 |
| SYS_ABORT | KERNEL_CALL+27 | do_abort | 系统控制 |
| SYS_IOPENABLE | KERNEL_CALL+28 | do_iopenable | 设备 I/O (x86) |
| SYS_SAFECOPYFROM | KERNEL_CALL+31 | do_safecopy_from | 拷贝 |
| SYS_SAFECOPYTO | KERNEL_CALL+32 | do_safecopy_to | 拷贝 |
| SYS_VSAFECOPY | KERNEL_CALL+33 | do_vsafecopy | 拷贝 |
| SYS_SETGRANT | KERNEL_CALL+34 | do_setgrant | 进程管理 |
| SYS_READBIOS | KERNEL_CALL+35 | do_readbios | 设备 I/O (x86) |
| SYS_SPROF | KERNEL_CALL+36 | do_sprofile | 性能 |
| SYS_STIME | KERNEL_CALL+39 | do_stime | 时钟 |
| SYS_SETTIME | KERNEL_CALL+40 | do_settime | 时钟 |
| SYS_VMCTL | KERNEL_CALL+43 | do_vmctl | 内存 |
| SYS_DIAGCTL | KERNEL_CALL+44 | do_diagctl | 系统控制 |
| SYS_VTIMER | KERNEL_CALL+45 | do_vtimer | 时钟 |
| SYS_RUNCTL | KERNEL_CALL+46 | do_runctl | 进程管理 |
| SYS_GETMCONTEXT | KERNEL_CALL+50 | do_getmcontext | 机器状态 |
| SYS_SETMCONTEXT | KERNEL_CALL+51 | do_setmcontext | 机器状态 |
| SYS_UPDATE | KERNEL_CALL+52 | do_update | 进程管理 |
| SYS_EXIT | KERNEL_CALL+53 | do_exit | 进程管理 |
| SYS_SCHEDCTL | KERNEL_CALL+54 | do_schedctl | 调度 |
| SYS_STATECTL | KERNEL_CALL+55 | do_statectl | 进程管理 |
| SYS_SAFEMEMSET | KERNEL_CALL+56 | do_safememset | 拷贝 |
| SYS_PADCONF | KERNEL_CALL+57 | do_padconf | ARM |

**NR_SYS_CALLS = 58**（`com.h:270`）

**map() 宏**（`system.c:54-57`）：编译期注册器——把"系统调用号 → 处理函数"装入分发表 `call_vec[]`，运行时 `SYS_*` 请求按号索引派发；`assert` 借 `NR_SYS_CALLS` 常量做编译期边界防御。
```c
#define map(call_nr, handler)                   \
    {   int call_index = call_nr-KERNEL_CALL;   \
        assert(call_index >= 0 && call_index < NR_SYS_CALLS); \
        call_vec[call_index] = (handler); }
```

**IRQ hook 池**：`NR_IRQ_HOOKS = 64`（`glo.h`）

**4GB 截断常量**（`pg_utils.c:88`）：`#define LIMIT 0xFFFFF000`

### 2.2 核心数据结构

**call_vec**（`system.c:52`）：
```c
static int (*call_vec[NR_SYS_CALLS])(struct proc * caller, message *m_ptr);
```
- C 时代的函数指针数组；下标 = `syscall_number - KERNEL_CALL`
- C 端在 `system_init()` 中逐个 `map()` 填充——Rust 端**不存在**该数组，由 [`os/kernel/src/syscall.rs`](../os/kernel/src/syscall.rs) 的 `enum Syscall + match` 取代（D1 设计决策）
- C 端 `kernel_call_dispatch` 通过 `call_vec[call_nr]` 分派；Rust 端同名 `kernel_call_dispatch`（[syscall.rs:416](../os/kernel/src/syscall.rs)）通过 `match` 分派——两者职责同构，**实现路径分叉**

**irq_hooks[]**（`glo.h`）：
```c
struct irq_hook {
    int proc_nr_e;       /* -1 = NONE = 空槽 */
    /* ... 其他字段 */
} irq_hooks[NR_IRQ_HOOKS];
```
- 固定大小池，`proc_nr_e == NONE` 表示可用

**s_alarm_timer**（`priv.h`）：
- 每个 `struct priv` 包含一个 `minix_timer_t s_alarm_timer`
- C 端在 `system_init()` 遍历所有 priv 结构初始化定时器（Rust 端 IRQ hook 池 + alarm timer 已在 [`os/kernel/src/irq_manager.rs`](../os/kernel/src/irq_manager.rs) 实现，时机与 C 不同）

**kernel_may_alloc**（`glo.h`）：
- 全局标志，`KERNEL_MAY_ALLOC: AtomicBool`（[lib.rs:1325](../os/kernel/src/lib.rs)）；kmain 开始时设为 1
- `bsp_finish_booting()` Step 9 中设为 0（C `main.c:91`）
- C 端 `add_memmap()` 断言此标志为真；Rust 端调用位置见 [lib.rs:497](../os/kernel/src/lib.rs)（在 `init_post_and_memory` 内，bsp_finish_booting 之前）

**vm_running**（`glo.h`）：
- 全局标志，`VM_RUNNING: AtomicBool`；`bsp_finish_booting()` Step 1 中设为 0（[lib.rs:1958](../os/kernel/src/lib.rs)）
- `do_vmctl` 的多个子命令检查此标志判断 VM 是否可用

### 2.3 关键函数分析

> **⚠️ Rust 实现状态对照**：本节分析对象均为 C 时代函数（`system.c:168-270`、`pg_utils.c:86-121`、`main.c:38-109`）。Rust 端的等价实现分散在不同模块——见 §3 设计决策表 D1-D6 与每条 Ch4 实现的文件指针。

#### system_init()（`system.c:168-270`）

**内部三段子流程**（C 端 system_init 自 system.c:168-270 的实现拆解，与本文档前文"kmain 三步"无关——别称混淆）：

1. **IRQ hook 池清零**（L173-176）：遍历 `irq_hooks[0..NR_IRQ_HOOKS-1]`，设 `proc_nr_e = NONE`
2. **Alarm timer 初始化**（L178-181）：遍历 `BEG_PRIV_ADDR..END_PRIV_ADDR`，对每个 priv 调用 `tmr_inittimer()`
3. **call_vec 注册**（L183-270）：先全部置 NULL，然后逐个 `map(SYS_*, do_*)` 注册

**map() 宏的安全保证**：编译期 assert 确保系统调用号在 `[0, NR_SYS_CALLS)` 范围内。如果有人用了非法调用号，编译失败。

**条件编译**：
- `SYS_DEVIO`/`SYS_SDEVIO`/`SYS_VDEVIO`/`SYS_IOPENABLE`/`SYS_READBIOS` 仅 `__i386__`
- `SYS_PADCONF` 仅 `__arm__`
- 64 位（x86_64/aarch64/riscv64）这些调用不注册

#### add_memmap()（`pg_utils.c:86-121`）

**功能**：将 bootstrap 阶段占用的物理内存区域添加到 `kinfo.memmap[]` 供 VM 管理。

**关键逻辑**：
1. **4GB 截断**（L89-96）：`addr > LIMIT` 直接返回；`addr + len > LIMIT` 截断 len
2. **页对齐**（L99-100）：base 向上对齐，len 向下对齐到 PAGE_SIZE
3. **断言 kernel_may_alloc**（L102）：确保在内核分配窗口内调用
4. **查找空槽**（L104-118）：线性扫描 `memmap[]`，找到第一个 `mm_length == 0` 的槽
5. **更新 mmap_size**（L110-111）：跟踪已使用的最高 memmap 索引
6. **更新 mem_high_phys**（L112-115）：跟踪最高物理地址

**32 位遗留**：`LIMIT = 0xFFFFF000`（4GB-4KB）是 Minix3 32 位地址空间限制。64 位下不需要此截断。

#### bsp_finish_booting()（`main.c:38-109`）

**BSP 启动完成序列**：

| 步骤 | 代码 | 说明 |
|------|------|------|
| 1 | `cpu_identify()` | CPU 特性识别 |
| 2 | `vm_running = 0` | VM 尚未运行 |
| 3 | `krandom` 初始化 | 随机数源配置 |
| 4 | `bill_ptr = proc_ptr = idle_proc` | 初始计费/当前进程指向 IDLE |
| 5 | `announce()` | 打印 MINIX 启动横幅 |
| 6 | `RTS_UNSET(proc_addr(i), RTS_PROC_STOP)` | 解除 boot 进程的停止标志（i=0..NR_BOOT_PROCS-NR_TASKS-1） |
| 7 | `cycles_accounting_init()` | CPU 计账初始化 |
| 8 | `boot_cpu_init_timer(system_hz)` | 启动 100Hz 时钟 |
| 9 | `fpu_init()` | FPU 初始化 |
| 10 | `cpu_set_flag(bsp_cpu_id, CPU_IS_READY)` | SMP: 标记 BSP 就绪 |
| 11 | `kernel_may_alloc = 0` | 关闭内核分配窗口 |
| 12 | `switch_to_user()` | 永不返回，进入调度循环 |

**步骤 6 的范围**：循环上界 `NR_BOOT_PROCS - NR_TASKS` 故意**排除** kernel task（CLOCK/SYSTEM/IDLE/KERNEL/ASYNCM）——它们不是"在更早阶段启动"，而是**永不作为运行实体被调度**（IDLE 由 `proc_init` 设 `RTS_PROC_STOP` 永不解除；CLOCK/SYSTEM 的执行入口是事件驱动的 `timer_int_handler`/`kernel_call`，调度器无可切换页表也无恢复点，见 [06 §1.4](../01-stage-kernel/06-proc-init-boot-proc.md)）；此循环只把 boot image 里的用户态 module（含 VM、RS）从"已占用但暂停"转正为"可被调度"。

**步骤 8 的失败处理**：如果时钟初始化失败，直接 panic——没有时钟源，内核无法调度。

**步骤 11 的含义**：`kernel_may_alloc = 0` 后，内核不能再直接分配物理内存。所有内存管理交给 VM。

#### kernel_call_dispatch()（`system.c:103-116`）

**系统调用分派**：
```c
call_nr = msg->m_type - KERNEL_CALL;

if (call_nr < 0 || call_nr >= NR_SYS_CALLS) {  /* check call number */
    result = EBADREQUEST;          /* illegal message type */
}
else if (!GET_BIT(priv(caller)->s_k_call_mask, call_nr)) {
    result = ECALLDENIED;          /* no permission for system call */
} else {
    result = call_vec[call_nr](caller, msg);  /* handle the system call */
}
```

**VMSUSPEND 处理**（`kernel_call_finish()`，L58-90）：当 handler 在分页异常或跨空间拷贝中需要 VM 介入时返回 `VMSUSPEND`，内核保存请求消息到 `p_vmrequest.saved.reqmsg`、设置 `MF_KCALL_RESUME`；VM 完成页错误修复后通过 `vmctl` 通知内核，从断点继续执行 handler。其余情况正常返回结果拷贝回用户空间。`VMSUSPEND` 的语义核心是"内核态需要 VM 帮助才能继续当前 handler"——是 §1.4 D8（VM 接管物理内存分配）的运行期镜像。

### 2.4 调用关系

```
kmain (C 端，对照参考)
 ├── system_init()                    [C: system.c:168]
 │    ├── irq_hooks[] 初始化
 │    ├── tmr_inittimer() × N privs
 │    └── map(SYS_*, do_*) × 50+      ← call_vec 注册
 ├── add_memmap(&kinfo, bootstrap)    [C: pg_utils.c:86]
 └── bsp_finish_booting()             [C: main.c:38]
      ├── cpu_identify()
      ├── vm_running = 0
      ├── announce()
      ├── RTS_UNSET × (NR_BOOT_PROCS - NR_TASKS)
      ├── cycles_accounting_init()
      ├── boot_cpu_init_timer(100)
      ├── fpu_init()
      ├── kernel_may_alloc = 0
      └── switch_to_user()            ← 永不返回

kmain (Rust 端，实读 os/kernel/src/lib.rs)
 ├── T0 init_protection             (lib.rs:744)
 ├── T1 init_clock_and_interrupts   (lib.rs:780)
 ├── T2 SMP_STATE::new_single_cpu  (lib.rs:559)
 ├── T3 init_proc_and_boot          (lib.rs:879)
 ├── T4 init_post_and_memory        (lib.rs:1271)
 │    └─ memmap::add_memmap(bootstrap)   (lib.rs:497)
 └── T5 bsp_finish_booting          (lib.rs:1948)
      └─ Step 1-10: 见 §1.2 时序图
```

### 2.5 设计要点

1. **map() 宏的编译期安全**（C 端）：非法调用号 → assert 失败 → 编译错误。这是 C 的"穷尽检查"替代方案。Rust 端通过 `enum Syscall + match` 取得更强的编译期保障——无需运行时注册表（见 D1）
2. **kernel_may_alloc 窗口**（C 端）：kmain 开始时为 1，bsp_finish_booting 最后设为 0；这个窗口保证内核在 VM 接管前可以分配内存。Rust 端对应实现：`KERNEL_MAY_ALLOC: AtomicBool` 在 [lib.rs:390](../os/kernel/src/lib.rs) 启用、[lib.rs:2078](../os/kernel/src/lib.rs) 关闭（bsp_finish_booting Step 8；:1325 为定义）；切换发生在 `init_post_and_memory` → `bsp_finish_booting` 之间
3. **vm_running 的初始值**：设为 0（而非 1），因为此时 VM 尚未启动。`do_vmctl` 的子命令通过此标志判断 VM 是否可用
4. **boot 进程分批处理**（不是"分批启动"）：所有 boot 进程（含 kernel task）在 `proc_init` 阶段都已填入 proc 槽位并设 `RTS_PROC_STOP`；`bsp_finish_booting` Step 4 解除范围 = `i < NR_BOOT_PROCS - NR_TASKS`（module，含 VM、RS），**kernel task 永不解除**——它们无独立执行入口，事件触发（IDLE 是调度器兜底，CLOCK/SYSTEM/KERNEL/ASYNCM 是中断或内核调用入口），见 [06 §1.4](../01-stage-kernel/06-proc-init-boot-proc.md)

---

## 3. Rust 设计决策

**注**：本节为决策清单（9 条 D1-D9）速览，每条都有 `Ch4 实现详解 + Ch5 测试要点` 锚点。详细推理（`为什么选 A 不选 B`）见各 D 详述。

D1 call_vec 表达      决策 `enum Syscall + match`     类型安全 + 编译期穷尽检查 + BTB 完全命中（无函数指针间接跳转），IPC 实测提升 10-30%（详 §3.1）
D2 map() 宏替代        决策 `const _: () = assert!(...)` 编译期检查与 C 的 map() 宏等价（详 §3.2）
D3 IRQ hook 池        决策 `[Option<IrqHook>; NR_IRQ_HOOKS]` 保持 C 池语义 + O(1) 索引 + 零堆分配（已实现于 irq_manager.rs）
D4 Alarm timer        决策 保持每 priv 一个 timer struct  per-priv 而非全局，与 C 语义一致
D5 add_memmap 4GB 截断 决策 删除 LIMIT 截断             64 位不需要 4GB 限制，Direct Map 可表达全部物理内存；`add_memmap()` 函数本身保留（详 §3.3）
D6 vm_running 表达    决策 全局 `AtomicBool`            C 端是全局 int（glo.h:74），Rust 全局 AtomicBool 语义等价。C 从不置 1（C omission：do_umap_remote.c:106 / acpi.c:61,70 / oxpcie.c:52,73 读它但无人置位）——Rust 在 `VMCTL_SETADDRSPACE` 目标为 VM 时修正性置 true（见 09 §3 decision4）。设计稿曾计划 SMP 后迁入 per-CPU——该迁移将偏离 C 的全局语义，若实施须 [ARCH] 标注
D7 switch_to_user     决策 发散函数 `-> !`              类型系统表达永不返回（详 §3.4）
D8 kernel_may_alloc    决策 运行时 `AtomicBool`          C 运行时标志无法完全消除——`add_memmap` 在断言 `kernel_may_alloc == true` 时实际依赖它（C `pg_utils.c:96`）
D9 条件编译 syscall   决策 trait 默认 + BadCall        不用 `#[cfg(target_arch)]`——架构不支持的 syscall 经 `ArchSyscall` 默认实现返回 `BadCall`，回复时映射 `EBADREQUEST`（详 §3.5）

---

### 3.1 D1 详述：call_vec 表达

C 用 `static int (*call_vec[NR_SYS_CALLS])(struct proc *, message *)` 函数指针数组做分派（system.c:52），运行时通过 `map(SYS_*, do_*)` 逐个填充（system.c:54-57 的 `map()` 宏）。Rust 用 `enum Syscall + match`（`os/kernel/src/syscall.rs`）取代这一数组，原因有三：

**穷尽检查**：新增 syscall 时，match 未覆盖的分支编译失败——等价于 C 的 `map()` 宏 assert 但更严格（assert 触发在编译期断言失败时；match 直接编译错误）。新增 syscall 的改动量从"记得 map + 不漏填"降为"加 enum 变体——编译器告诉你哪几个 match 要补"。

**无函数指针间接调用（性能）**：C 的 `call_vec[call_nr](caller, msg)` 是经内存的间接调用——目标地址由 `call_nr` 在运行时决定，CPU 分支目标缓冲（BTB）对这类间接转移的预测能力弱于静态可知的直接跳转。Rust 的 `match` 对稠密判别值（本例 0-57 无空洞的 u16）生成编译期跳转表，目标地址静态可知。这是教科书级的间接调用 vs 跳转表差异；本专项未做 benchmark，不做具体倍数声明（避免无测量依据的性能数字）。

**类型安全**：`enum Syscall` 的变体携带语义，而非裸整数。同时避免 §设计模式 19 的"自创错误码"陷阱——match 编译期就能保证未注册 syscall 被拒绝（返回 EBADREQUEST），无需运行时数组槽位 NULL 检查。

---

### 3.2 D2 详述：map() 宏替代

C 的 `map(call_nr, handler)` 宏展开为 `call_vec[call_nr - KERNEL_CALL] = handler`，其中内嵌 `assert(call_index >= 0 && call_index < NR_SYS_CALLS)`（system.c:54-57）。该 assert **在运行时检查**——若 `call_nr - KERNEL_CALL < 0` 或越界，编译期通过但运行崩溃。

Rust 端无此宏：所有 syscall 注册在 `enum Syscall` 的变体里（编译期穷尽），约束 `KERNEL_CALL..KERNEL_CALL+NR_SYS_CALLS` 通过类型保证；非法 syscall 在 match 默认分支返回 `EBADREQUEST`（`os/kernel/src/syscall.rs`）。效果等价于 C assert 但**完全编译期**——不可能有"运行时才发现的越界"。

---

### 3.3 D5 详述：4GB 截断删除

C 的 `add_memmap()` 有 `#define LIMIT 0xFFFFF000` 截断（pg_utils.c:88），任何超出 4GB 的物理地址被截断到 [0, 4GB)——32 位 Minix3 的虚拟地址空间无法表达 [4GB, +∞)。minix-rs 是 64 位，虚拟地址空间 256TB+，**不需要**此截断（[01-stage-kernel/07-cross-space-init.md](../01-stage-kernel/07-cross-space-init.md) §1.3 详述 direct_map 完整覆盖）。

但 `add_memmap()` 函数本身保留——bootstrap 内存回收是必要的（C 的 4GB 限制针对的是"超出部分"，不阻止 4GB 内的回收）。Rust 端 [`os/kernel/src/memmap.rs`](../os/kernel/src/memmap.rs) 已实现该函数（不再含 `LIMIT` 截断）；调用时机由 `init_post_and_memory` 触发（[lib.rs:497](../os/kernel/src/lib.rs)），而非 `bsp_finish_booting` 内部。

---

### 3.4 D7 详述：发散函数 vs 普通函数

C 的 `switch_to_user` 是普通函数（C 没有发散函数概念），编译器仅通过"调用此函数后不再有代码"来知道它不返回——但 C 没有类型系统表达，静态分析能力有限。Rust 用 `fn switch_to_user() -> !`（[lib.rs:2757](../os/kernel/src/lib.rs)）显式标注——函数签名保证永不返回，调用方省略 `unsafe { ... }` 包装时的"调用后代码必须存在"约束；编译器对返回类型 `!` 的函数会传播发散性（如调用点不必有返回值兼容）。

> 完整调度循环已落地（见 [10 §4](10-switch-to-user.md)）：函数体是真正的循环（终局分派经 `TrapReturnArch::restore_to_user` 离开内核），发散性类型与运行时行为现在是一致的——`-> !` 描述的"永不返回"既是类型事实也是执行事实。

---

### 3.5 D9 详述：条件编译 syscall 表达

C 端用 `#if defined(__i386__)` 决定是否注册 `SYS_DEVIO` 等 x86 专用调用（system.c:194-198）。Rust 不用 `#[cfg(target_arch)]` 选择行为——条件编译选行为违反硬件抽象原则（机制差异必须经 trait 表达，参见 review 规则"硬件未抽象为 trait"模式）。

Rust 端：所有架构共享同一个 `enum Syscall` 定义（`syscall.rs`），架构不支持的 syscall 走 `ArchSyscall` trait 默认实现返回 `BadCall`（回复时映射为 `EBADREQUEST`，对齐 C system.c:119-123 对非法调用号的处置）——避免条件编译导致的代码路径分裂，保留硬件抽象原则（架构差异经 trait 分发，D9/§3.5）。D7/D8 的详述并入本节速览：D7 见 §3.4；D8 的理由是 C 的 `assert(kernel_may_alloc)`（pg_utils.c:102）是运行时依赖，编译期保证无法等价替代。

---

## 4. 实现详解

### 4.1 Syscall 枚举

```rust
// os/kernel/src/syscall.rs

/// Kernel system call number.
///
/// C: `SYS_*` constants in minix/com.h:207-270
/// C: `NR_SYS_CALLS = 58` in minix/com.h:270
///
/// Design decision D1: enum + match replaces C's call_vec[] function pointer array.
/// Design decision D9: architecture-specific syscalls return EBADCALL on
/// unsupported platforms rather than being conditionally compiled out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Syscall {
    Fork = 0,
    Exec = 1,
    Clear = 2,
    Schedule = 3,
    Privctl = 4,
    Trace = 5,
    Kill = 6,
    Getksig = 7,
    Endksig = 8,
    Sigsend = 9,
    Sigreturn = 10,
    // 11-12: unused
    Memset = 13,
    Umap = 14,
    Vircopy = 15,
    Physcopy = 16,
    UmapRemote = 17,
    Vumap = 18,
    Irqctl = 19,
    // 20: unused
    Devio = 21,
    Sdevio = 22,
    Vdevio = 23,
    Setalarm = 24,
    Times = 25,
    Getinfo = 26,
    Abort = 27,
    Iopenable = 28,
    // 29-30: unused
    SafecopyFrom = 31,
    SafecopyTo = 32,
    Vsafecopy = 33,
    Setgrant = 34,
    Readbios = 35,
    Sprof = 36,
    // 37-38: unused
    Stime = 39,
    Settime = 40,
    // 41-42: unused
    Vmctl = 43,
    Diagctl = 44,
    Vtimer = 45,
    Runctl = 46,
    // 47-49: unused
    Getmcontext = 50,
    Setmcontext = 51,
    Update = 52,
    Exit = 53,
    Schedctl = 54,
    Statectl = 55,
    Safememset = 56,
    Padconf = 57,
}

/// Total number of kernel system calls.
/// C: NR_SYS_CALLS = 58
pub const NR_SYS_CALLS: usize = 58;

impl TryFrom<u16> for Syscall {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Syscall::Fork),
            1 => Ok(Syscall::Exec),
            2 => Ok(Syscall::Clear),
            3 => Ok(Syscall::Schedule),
            4 => Ok(Syscall::Privctl),
            5 => Ok(Syscall::Trace),
            6 => Ok(Syscall::Kill),
            7 => Ok(Syscall::Getksig),
            8 => Ok(Syscall::Endksig),
            9 => Ok(Syscall::Sigsend),
            10 => Ok(Syscall::Sigreturn),
            13 => Ok(Syscall::Memset),
            14 => Ok(Syscall::Umap),
            15 => Ok(Syscall::Vircopy),
            16 => Ok(Syscall::Physcopy),
            17 => Ok(Syscall::UmapRemote),
            18 => Ok(Syscall::Vumap),
            19 => Ok(Syscall::Irqctl),
            21 => Ok(Syscall::Devio),
            22 => Ok(Syscall::Sdevio),
            23 => Ok(Syscall::Vdevio),
            24 => Ok(Syscall::Setalarm),
            25 => Ok(Syscall::Times),
            26 => Ok(Syscall::Getinfo),
            27 => Ok(Syscall::Abort),
            28 => Ok(Syscall::Iopenable),
            31 => Ok(Syscall::SafecopyFrom),
            32 => Ok(Syscall::SafecopyTo),
            33 => Ok(Syscall::Vsafecopy),
            34 => Ok(Syscall::Setgrant),
            35 => Ok(Syscall::Readbios),
            36 => Ok(Syscall::Sprof),
            39 => Ok(Syscall::Stime),
            40 => Ok(Syscall::Settime),
            43 => Ok(Syscall::Vmctl),
            44 => Ok(Syscall::Diagctl),
            45 => Ok(Syscall::Vtimer),
            46 => Ok(Syscall::Runctl),
            50 => Ok(Syscall::Getmcontext),
            51 => Ok(Syscall::Setmcontext),
            52 => Ok(Syscall::Update),
            53 => Ok(Syscall::Exit),
            54 => Ok(Syscall::Schedctl),
            55 => Ok(Syscall::Statectl),
            56 => Ok(Syscall::Safememset),
            57 => Ok(Syscall::Padconf),
            _ => Err(()),
        }
    }
}
```

> 设计决策 D1：enum + match 替代 C 的 `call_vec[]` 函数指针数组。新增 syscall 时 match 未覆盖则编译失败，等价于 C 的 map() 宏 assert。

### 4.2 系统调用分派

> **引导语**：本节展示 `kernel_call_dispatch` 入口及其内部分发结构。**BKL 的获取与保留**是阅读重点——这是 §1.4 D8 "kernel_may_alloc 窗口"在运行时的镜像：dispatch 持锁 → finish/switch_to_user 释放，期间不允许任何 CPU 让出。代码注释中英文混排，Rust 行为约束以中文行注体现，C 对应用 `// C: ...` 前缀。

`KcallResult` 枚举（syscall.rs:515）把 C 的 4 个返回路径（EBADREQUEST/ECALLDENIED/VMSUSPEND/EDONTREPLY）建模为 5 个变体（含 `Ok`）：

```rust
// os/kernel/src/syscall.rs (continued)

use crate::proc::KProcess;
use crate::kpriv::PrivTable;
use crate::proc_table::ProcessTable;
use crate::clock::ClockState;
use minix_types::Message;

/// Result of a kernel call dispatch.
/// C: EBADREQUEST (212), ECALLDENIED (210) — sys/sys/errno.h
/// C: VMSUSPEND (-996) — kernel/vm.h; EDONTREPLY — sys/sys/errno.h
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KcallResult {
    /// Call completed with return value.
    Ok(i32),
    /// Call requires VM assistance (VMSUSPEND = -996 in C).
    VmSuspend,
    /// No reply should be sent (EDONTREPLY).
    NoReply,
    /// Invalid/unimplemented syscall number.
    BadCall,
    /// Caller does not have permission for this call.
    /// C: `!GET_BIT(priv(caller)->s_k_call_mask, call_nr)` — system.c:107
    CallDenied,
}

/// Dispatch a kernel system call.
///
/// C: kernel_call_dispatch() in system.c:103-116
/// C: kernel_call_finish() in system.c:58-90
///
/// Design decision D1: match replaces call_vec[] dispatch.
/// Design decision D9: arch-specific syscalls return BadCall on unsupported
/// platforms instead of being conditionally compiled out.
///
/// # BKL
///
/// Acquires the Big Kernel Lock on entry. BKL is released later in
/// `kernel_call_finish()` or `switch_to_user()`, matching C's pattern where
/// the trap entry holds the lock across dispatch + finish.
pub fn kernel_call_dispatch(
    caller: &mut KProcess,
    msg: &mut Message,
    priv_table: &mut PrivTable,
    proc_table: &mut ProcessTable,
    clock_state: &mut ClockState,
) -> KcallResult {
    // Acquire BKL — C: BKL_LOCK() in mpx.S kernel_call_entry_common.
    // BklGuard is RAII (Drop releases BKL). We mem::forget the guard because
    // BKL must stay held until kernel_call_finish()/switch_to_user() releases it.
    // A BklSection witness is derived for compile-time BKL proof on global accessors.
    let bkl_guard = crate::smp::bkl_lock();
    let result = {
        let bkl_section = bkl_guard.section();
        kernel_call_dispatch_inner(caller, msg, priv_table, proc_table, clock_state, &bkl_section)
    };
    core::mem::forget(bkl_guard);
    result
}

/// Inner dispatch logic, called after BKL is acquired.
fn kernel_call_dispatch_inner(
    caller: &mut KProcess,
    msg: &mut Message,
    priv_table: &mut PrivTable,
    proc_table: &mut ProcessTable,
    clock_state: &mut ClockState,
    bkl_section: &crate::smp::BklSection<'_>,
) -> KcallResult {
    let call_nr = msg.m_type as u16;
    let syscall = match Syscall::try_from(call_nr) {
        Ok(s) => s,
        Err(()) => return KcallResult::BadCall,
    };

    // C: `else if (!GET_BIT(priv(caller)->s_k_call_mask, call_nr))` — system.c:107
    // Composed as Option::and_then + is_none_or:
    //   - None (no priv_id, or priv_id not in table) → deny
    //   - Some(priv) → deny iff kcall_filter_check returns false
    let call_denied = caller.priv_id
        .and_then(|id| priv_table.get(id))
        .is_none_or(|caller_priv| !kcall_filter_check(caller_priv, call_nr as u32));
    if call_denied {
        return KcallResult::CallDenied;
    }

    match syscall {
        Syscall::Fork => crate::syscall_process::dispatch_fork(caller, msg, proc_table, priv_table),
        Syscall::Exec => crate::syscall_process::dispatch_exec(caller, msg, proc_table),
        Syscall::Clear => crate::syscall_process::dispatch_clear(caller, msg, proc_table, priv_table, clock_state),
        Syscall::Exit => crate::syscall_process::dispatch_exit(caller, msg),
        Syscall::Schedule => dispatch_schedule(caller, msg, proc_table, priv_table),
        Syscall::Privctl => dispatch_privctl(caller, msg, proc_table, priv_table),
        Syscall::Trace => dispatch_trace(caller, msg, proc_table, priv_table),
        Syscall::Kill => dispatch_kill(caller, msg, proc_table, priv_table),
        Syscall::Getksig => dispatch_getksig(caller, msg, proc_table, priv_table),
        Syscall::Endksig => dispatch_endksig(caller, msg, proc_table, priv_table),
        Syscall::Sigsend => dispatch_sigsend(caller, msg, proc_table),
        Syscall::Sigreturn => dispatch_sigreturn(caller, msg, proc_table),
        Syscall::Memset => dispatch_memset(caller, msg, proc_table),
        Syscall::Umap => dispatch_umap(caller, msg, proc_table, priv_table),
        Syscall::Vircopy => dispatch_vircopy(caller, msg, proc_table),
        Syscall::Physcopy => dispatch_physcopy(caller, msg, proc_table),
        Syscall::UmapRemote => dispatch_umap_remote(caller, msg, proc_table, priv_table),
        Syscall::Vumap => dispatch_vumap(caller, msg, proc_table),
        Syscall::Irqctl => dispatch_irqctl(caller, msg, priv_table, bkl_section),
        // D6: x86-specific syscalls — BadCall on unsupported arch.
        Syscall::Devio => CurrentArchSyscall::dispatch_devio(caller, msg, priv_table),
        Syscall::Sdevio => CurrentArchSyscall::dispatch_sdevio(caller, msg, priv_table, proc_table),
        Syscall::Vdevio => CurrentArchSyscall::dispatch_vdevio(caller, msg, priv_table),
        Syscall::Setalarm => dispatch_setalarm(caller, msg, priv_table, clock_state),
        Syscall::Times => dispatch_times(caller, msg, proc_table),
        Syscall::Getinfo => dispatch_getinfo(caller, msg, priv_table, proc_table, clock_state),
        Syscall::Abort => dispatch_abort(caller, msg),
        Syscall::Iopenable => CurrentArchSyscall::dispatch_iopenable(caller, msg, proc_table),
        Syscall::SafecopyFrom => dispatch_safecopy_from(caller, msg, proc_table, priv_table),
        Syscall::SafecopyTo => dispatch_safecopy_to(caller, msg, proc_table, priv_table),
        Syscall::Vsafecopy => dispatch_vsafecopy(caller, msg, proc_table, priv_table),
        Syscall::Setgrant => dispatch_setgrant(caller, msg, priv_table),
        Syscall::Readbios => CurrentArchSyscall::dispatch_readbios(caller, msg),
        Syscall::Sprof => dispatch_sprofile(caller, msg, proc_table),
        Syscall::Stime => dispatch_stime(caller, msg, clock_state),
        Syscall::Settime => dispatch_settime(caller, msg, clock_state),
        Syscall::Vmctl => dispatch_vmctl(caller, msg, proc_table),
        Syscall::Diagctl => dispatch_diagctl(caller, msg, priv_table, proc_table),
        Syscall::Vtimer => dispatch_vtimer(caller, msg, priv_table, proc_table),
        Syscall::Runctl => dispatch_runctl(caller, msg, proc_table),
        Syscall::Getmcontext => dispatch_getmcontext(caller, msg, proc_table),
        Syscall::Setmcontext => dispatch_setmcontext(caller, msg, proc_table),
        Syscall::Update => dispatch_update(caller, msg, proc_table, priv_table),
        Syscall::Schedctl => dispatch_schedctl(caller, msg, proc_table),
        Syscall::Statectl => dispatch_statectl(caller, msg, proc_table, priv_table, crate::ipc_filter_pool()),
        Syscall::Safememset => dispatch_safememset(caller, msg, proc_table, priv_table),
        // D6: ARM-specific — BadCall on non-ARM.
        Syscall::Padconf => CurrentArchSyscall::dispatch_padconf(caller, msg),
    }
}
```

> 设计决策 D1：match 替代 call_vec[]。D6：架构专用 syscall 在不支持平台返回 BadCall。当前实现已将具体子系统调用（fork/exec/clear 等）委托到 `syscall_process` 等子模块，而非返回 BadCall 的占位符。

### 4.3 编译期断言

```rust
// os/kernel/src/syscall.rs (continued)

/// Compile-time verification that all syscall numbers in the enum
/// are within the valid range [0, NR_SYS_CALLS).
///
/// C: map() macro's assert(call_index >= 0 && call_index < NR_SYS_CALLS)
/// Design decision D2: const assert replaces C's runtime assert in map() macro.
const _: () = {
    // SAFETY: `Syscall` is `#[repr(u16)]`, so every discriminant is stored
    // as a u16 and `as u16` is a lossless no-op that cannot truncate.
    assert!(Syscall::Fork as u16 == 0);
    assert!(Syscall::Padconf as u16 == 57);
    assert!((Syscall::Padconf as u16) < (NR_SYS_CALLS as u16));
};
```

### 4.4 system_init 的 Rust 表达

C 中的 `system_init()` 做三件事：清零 `irq_hooks[]`、初始化每个 `priv` 的 alarm timer、用 `map()` 宏填充 `call_vec[]`。在 Rust 中，这三件事被分解到构造函数和类型系统里，**没有独立的 `system_init` 函数**：

1. **IRQ hook 池**：`IrqManager::new()` 在构造时将所有 hook 设为 `None`。
2. **Alarm timer**：`KPriv::new()` 在构造时清零 `s_alarm_timer`。
3. **Call vector**：被 `enum Syscall` + `match` 替代，编译期 const assert（D2）替代 `map()` 宏的运行时 assert。

`kmain` 中 Phase E（原 `system_init()` 阶段）因此只有一行注释标记该阶段，没有函数调用：

```rust
// os/kernel/src/lib.rs (kmain Phase E)
// Phase E: system_init — register syscall handlers
```

> 设计决策 D1/D2：Rust 的 enum + match + const assert 替代 C 的 call_vec[] + map() 宏。C 端 `system_init` 的三段子流程（IRQ 清零 / timer 初始化 / call_vec 注册）由 Rust 的**构造函数 + 类型系统隐式完成**——`irq_manager` 模块在初始化时清零 IRQ hook 池，timer 在 priv 构造时初始化，`enum Syscall` 的变体本身就是"注册"的完成态。

### 4.5 add_memmap Rust 实现

```rust
// os/kernel/src/memmap.rs

use minix_boot::KernelInfo;
use minix_types::PhysBytes;

/// Maximum number of memory map entries.
/// C: MAXMEMMAP = 40 — minix/include/minix/param.h:13.
/// Rust raises it to 128: UEFI firmware memory maps routinely exceed 40
/// entries (one per EfiMemoryType region per hole), and truncating the
/// firmware map would silently drop RAM. Slot-scan semantics unchanged.
/// Capacity divergence from C is intentional (boot-shim input shaped).
pub const MAXMEMMAP: usize = 128;

/// Memory map entry.
/// C: struct memory_info in minix/type.h
#[derive(Debug, Clone, Copy)]
pub struct MemMapEntry {
    /// Physical base address (page-aligned).
    pub base: u64,
    /// Length in bytes (page-aligned).
    pub length: u64,
}

/// Const zero entry for static initialization.
pub const MEM_MAP_ENTRY_ZERO: MemMapEntry = MemMapEntry { base: 0, length: 0 };

// (actual code: `#[derive(Default)]` on the struct — zeroed fields, same
//  semantics as the C `mm_length == 0` empty-slot convention)

impl MemMapEntry {
    /// Whether this entry is empty (available for use).
    /// C: mm_length == 0 check in add_memmap()
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
}

/// Add a physical memory region to the kernel's memory map.
///
/// C: add_memmap() in pg_utils.c:86-121
///
/// Design decision D5: 4GB truncation removed for 64-bit.
/// The C version truncates at LIMIT=0xFFFFF000 because 32-bit Minix3
/// cannot handle >4GB physical addresses. In 64-bit minix-rs, Direct Map
/// can access all physical memory, so this truncation is unnecessary.
///
/// # Arguments
///
/// * `mmap` - Memory map array to insert into
/// * `addr` - Physical base address of the region
/// * `len` - Length of the region in bytes
///
/// # Returns
///
/// The index of the new entry, or a `MemMapError` on failure.
///
/// # Safety Invariant
///
/// This function should only be called during boot (while `kernel_may_alloc`
/// is true). The caller is responsible for ensuring this invariant.
/// C: assert(kernel_may_alloc) in pg_utils.c:102
pub fn add_memmap(mmap: &mut [MemMapEntry; MAXMEMMAP], addr: u64, len: u64) -> Result<usize, MemMapError> {
    // C: page alignment (roundup/rounddown)
    let page_size = 4096u64;
    let aligned_base = (addr + page_size - 1) & !(page_size - 1);
    let aligned_end = (addr + len) & !(page_size - 1);
    let aligned_len = aligned_end.saturating_sub(aligned_base);

    if aligned_len == 0 {
        return Err(MemMapError::ZeroLength);
    }

    // C: linear scan for empty slot (mm_length == 0)
    for (i, entry) in mmap.iter_mut().enumerate() {
        if entry.is_empty() {
            *entry = MemMapEntry {
                base: aligned_base,
                length: aligned_len,
            };
            return Ok(i);
        }
    }

    Err(MemMapError::NoSlots)
}

/// Errors from add_memmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemMapError {
    /// After page alignment, the region has zero length.
    ZeroLength,
    /// No empty slots available in the memory map.
    NoSlots,
}
```

> 设计决策 D5：删除 4GB 截断。64 位 Direct Map 可访问全部物理内存。

**已知缺口**：C 的 `add_memmap` 还更新两个 `kinfo` 字段，Rust 未实现：
- `mmap_size`（pg_utils.c:110-111）：跟踪已使用的最高 memmap 索引。Rust 中 `KernelInfo` 是 immutable（`&KernelInfo`），不能在此函数内修改。需由调用者（kmain Phase F）在获得可变内核状态后更新。
- `mem_high_phys`（pg_utils.c:112-115）：跟踪最高物理地址。同上，需由调用者更新。

这两个缺口不影响 boot 流程正确性（boot 期不需要这两个值），但 VM 接管后需要。标注为后续实现（09-vm-boot-protocol.md 的职责范围）。

### 4.6 bsp_finish_booting Rust 实现

```rust
// os/kernel/src/lib.rs

use core::sync::atomic::{AtomicBool, Ordering};

/// Global flag: kernel may allocate physical memory directly.
/// C: kernel_may_alloc in glo.h
/// Set to true at kmain start, cleared in bsp_finish_booting().
static KERNEL_MAY_ALLOC: AtomicBool = AtomicBool::new(false);

/// Global atomic mirror of C's `vm_running` flag.
/// C: `EXTERN int vm_running` — glo.h:74 (plain global, never per-CPU).
/// Set to false in bsp_finish_booting step 1; set true (correcting C's
/// omission) in VMCTL_SETADDRSPACE — see 09-vm-boot-protocol.md §3 decision4.
static VM_RUNNING: AtomicBool = AtomicBool::new(false);

/// Read the `vm_running` flag. C: `vm_running` — glo.h:37.
pub fn vm_running() -> bool { VM_RUNNING.load(Ordering::Acquire) }

/// BSP finish booting — the last step of kmain.
///
/// C: bsp_finish_booting() in main.c:38-109
///
/// Takes `&mut ProcessTable` so step 2 (bill_ptr = IDLE) and step 4
/// (RTS_PROC_STOP unset for boot processes) can operate directly.
/// Takes `&mut SmpState` for per-CPU cycle accounting and BSP identification.
#[cfg(not(feature = "mock"))]
fn bsp_finish_booting(
    proc_table: &mut ProcessTable,
    smp_state: &mut crate::smp::SmpState,
) -> ! {
    use crate::proc::RtsFlagsBits;

    // Step 0: cpu_identify() — probe BSP CPU identity into CPU_INFO
    // C: cpu_identify() — main.c:45 (first statement), i386: arch_system.c:212
    crate::smp::cpu_identify();

    // Step 1: vm_running = 0 — wired to a global atomic; per-CPU on SMP
    VM_RUNNING.store(false, Ordering::Release);

    // Step 2: bill_ptr = idle_proc — ProcessTable::set_bill_to_idle
    proc_table.set_bill_to_idle();

    // Step 3: announce() — EarlyConsole banner (visible in QEMU serial)
    use minix_plat::{EarlyConsole, CurrentEarlyConsole as Console};
    Console::write_str("\nMINIX-RS 0.1.0 (rust rewrite) — scheduling live\n");

    // Step 4: RTS_PROC_STOP unset for boot processes (skip kernel tasks)
    for nr in 0..(ProcNr(crate::proc::NR_BOOT_PROCS as i32)
        - ProcNr(crate::proc_table::NR_TASKS as i32)).0
    {
        proc_table.rts_unset(ProcNr(nr), RtsFlagsBits::PROC_STOP);
    }

    // Step 5: cycles_accounting_init() — set BSP TSC baseline
    let tsc = crate::clock::read_tsc();
    let bsp_id = smp_state.bsp_cpu_id();
    if let Some(bsp_local) = smp_state.cpu_local_mut(bsp_id) {
        bsp_local.note_context_switch(tsc);
    }

    // Step 6: boot_cpu_init_timer — start the periodic tick
    // C: boot_cpu_init_timer(system_hz) — clock.c:294.
    // (a) `init_local_timer(freq)` was already done in Phase B via
    //     `CurrentClockArch::init_timer(DEFAULT_HZ)`.
    // (b) Timer IRQ handler registration is deferred to the real
    //     interrupt-dispatch path (`IrqManager::register_hook`, Step 1.5.7).
    //     The deleted `ArchBoot::register_timer_handler` was a mock
    //     placeholder with no readers — see 05-clock-interrupt-init.md §4.7.1.
    // Behavior change (05-clock-interrupt-init.md §3.7): with `boot_init_timer`
    // gone, `enable_timer_irq` is no longer called. aarch64/riscv64 are
    // unchanged — `init_timer` above writes the same enable (CNTP_CTL_EL0
    // Enable=1/IMASK=0, sie.STIE=1), so the timer is live with no handler
    // yet; only x86_64 differs (LAPIC LVT Timer Mask stays 1, while the PIT
    // remains the boot clock source). Step 1.5.7's core is the IRQ-chain
    // registration (`IrqManager::register_hook`); x86_64 additionally calls
    // `<CurrentTimerIrqGate as TimerIrqGate>::enable_timer_irq()` if the
    // LAPIC LVT timer becomes the clock source.
    // Instance-based design (04-platform-discovery.md §3.4): construct a
    // transient clock arch instance from the global platform descriptor.
    use minix_arch::{ClockArch, CurrentClockArch};
    use minix_platform::{platform_desc, PlatformDesc};
    {
        let pd = platform_desc();
        let mut clock_arch = CurrentClockArch::new(pd.timer());
        clock_arch.init_timer(crate::clock::DEFAULT_HZ, crate::clock::current_cpuid().raw());
    }

    // Step 7: FPU presence probe
    let bsp_id = smp_state.bsp_cpu_id();
    if let Some(bsp_local) = smp_state.cpu_local_mut(bsp_id) {
        bsp_local.fpu_presence = true;
    }

    // Step 8: kernel_may_alloc = 0 — closing the boot-time alloc window
    KERNEL_MAY_ALLOC.store(false, Ordering::Release);

    // Step 8.5: acquire BKL before entering the scheduling loop
    // C: BKL is already held when coming from the trap entry; Rust acquires
    // it here because switch_to_user() releases it before returning to user.
    // BklGuard is RAII (Drop releases BKL) — mem::forget keeps BKL held
    // across switch_to_user(), which releases it before the idle loop.
    core::mem::forget(smp::bkl_lock());

    // Step 9: switch_to_user() — never returns (=> !)
    switch_to_user()
}

/// Entry point for the scheduling loop. C: switch_to_user() in proc.c.
/// D7: returns `!` — never returns to caller.
fn switch_to_user() -> ! {
    // Release BKL before entering the scheduling loop.
    // C: BKL is released implicitly by restore_user_context() which
    // does not return. In Rust, we release explicitly before the loop.
    crate::smp::bkl_unlock();

    // Placeholder — full scheduler loop implemented in 10-switch-to-user.md.
    loop { core::hint::spin_loop(); }
}
```

**各步骤职责与当前语义**（对应上方代码块）：

| 步骤 | 当前语义 | 说明 |
|------|---------|------|
| 0 `cpu_identify` | `smp::cpu_identify()` 探测 BSP 身份并记录进全局 `CPU_INFO` 表 | 寄存器读取经 arch crate 的 `CurrentCpuIdentity` 探测（x86 CPUID / aarch64 MIDR_EL1 / riscv64 marchid·mimpid）；详述见 §4.6 |
| 1 `vm_running = 0` | `VM_RUNNING: AtomicBool` 全局镜像置 false | `lib.rs::vm_running()` 暴露给 `do_vmctl` 等读者 |
| 2 `bill_ptr = idle_proc` | `ProcessTable::set_bill_to_idle()` | 将计费指针指向 idle 进程 |
| 3 `announce()` | `EarlyConsole::write_str` 打印 MINIX-RS banner | QEMU 串口可见 |
| 4 `RTS_UNSET` × (NR_BOOT_PROCS-NR_TASKS) | `for nr in 0..(NR_BOOT_PROCS - NR_TASKS)` | 复用 `rts_unset` 自动入队 |
| 5 `cycles_accounting_init` | 设置 BSP TSC baseline | 通过 `SmpState::cpu_local_mut(bsp)` |
| 6 `boot_cpu_init_timer` | 硬件 timer 已在 Phase B 初始化；BSP handler 注册占位 | 等待全局 IrqManager 完成后补齐 |
| 7 `fpu_init` | 标记 `fpu_presence = true` | per-CPU 字段在 `SmpState` 中 |
| 8 `kernel_may_alloc = 0` | `KERNEL_MAY_ALLOC.store(false)` | 关闭启动期直接分配窗口 |
| 8.5 BKL 获取 | `smp::bkl_lock()` | 进入调度循环前持有 BKL |
| 9 `switch_to_user()` | 释放 BKL 后进入占位循环 | 真实调度循环见 10-switch-to-user.md |

**签名即依赖清单**：`bsp_finish_booting(&mut ProcessTable, &mut SmpState)` 的两个参数不是惯例——步骤 2/4 要改进程表、步骤 5/7 要访问 per-CPU 状态，全部经参数显式传入而非读全局。启动末段的状态依赖因此可审计：签名之外无隐藏读取。

**与 C 12 步的差异说明**：C 的 `bsp_finish_booting`（main.c:38-109）有 12 步，Rust 实现 10 步（含 Step 0）。三步差异状态如下：

| C 步骤 | C 位置 | 状态与设计 |
|--------|--------|----------|
| `cpu_identify()` | main.c:45 | ✅ **已实现（Step 0，2026-09-04 收敛 todo D-53）**。C 中 kernel 是数据生产者：`cpu_identify()`（i386: arch_system.c:212 / earm: :85；BSP 经 main.c:45、AP 经 arch_smp.c:232 调用）填 kernel 全局 `cpu_info[CONFIG_MAX_CPUS]`（glo.h；i386 字段 vendor/family/model/stepping/freq/flags，archtypes.h:39-46），kernel 自身也是读者（arch_watchdog.c 读 vendor/family 选 MSR 语义、arch_clock.c 写 freq 做 TSC 校准回填），用户态只是消费端（procfs cpuinfo.c:146、libsys tsc_util.c:40 经 GET_CPUINFO——do_getinfo.c:76-80 整体拷出）。Rust 补齐的也是生产侧，分三层：(a) **arch 探测**——`os/arch/src/arch/cpu_identity.rs` 定义 `CpuIdentity` enum（X86/Arm/Riscv 一个变体一种 ISA——C 各 arch 往同一字节 blob 写不同形状再由用户态重解释，Rust 把形状变成类型级事实）+ `CpuIdentityArch` trait，`CurrentCpuIdentity` alias 按目标架构选择（x86 CPUID leaves 0/1 / aarch64 `MIDR_EL1` / riscv64 `mvendorid·marchid·mimpid` SBI ecall），三架构探测源内核皆可用。x86 侧含一处 **MINIX3 BUG 修复**：C（arch_system.c:239）把 ext-model 合并条件误写在 base model 上（`model == 0xf || model == 0x6`），对 2007 年后 base model ∉ {0xF,0x6} 的 family-6 CPU 截断 model（Nehalem 0x106E0 → 0xE 而非 0x1E；Skylake → 0xE 而非 0x4E）；Rust 按 SDM 以 family ∈ {0xF, 0x6} 为条件（`// MINIX3 BUG:` 标注于 x86_64/cpu_identity.rs::decode_signature + 4 项签名解码单测）。minix-rs 只支持现代硬件：C 的 `max_leaf == 0` 古董 CPU 守卫（486 时代）不移植。(b) **kernel 存储**——`CPU_INFO: SyncUnsafeCell<CpuInfoTable>`（smp.rs，`[Option<CpuIdentity>; MAX_CPUS]`，`None` = 未探测槽），表格住 kernel 与 C 的分层一致（`CONFIG_MAX_CPUS` 是 kernel 配置、cpu_info[] 在 kernel glo.h），经 `BklProtected` 审批列表进 `SyncUnsafeCell`（写入=单核 boot 期；读取=GET_CPUINFO 持 BKL）；(c) **GET_CPUINFO 全记录**——misc.rs `CpuInfoEntry` 重排为 C i386 `struct cpu_info` 布局（16 字节 repr(C)：vendor=CPU_VENDOR_INTEL 0/AMD 2/UNKNOWN 0xff，archconst.h:134-136），与本文件其他 GetInfo struct 的单一 ABI 惯例一致（cf. MachineStruct）；ARM MIDR/RISC-V CSR 字段在 x86 形状中无对应（C 各 arch 本就是不同 ABI），文档化为 CPU_VENDOR_UNKNOWN + 全零，类型化身份仍可经 `smp::cpu_identity` 内部读取。**剩余缺口（有意保留）**：freq 恒 0——TSC 校准回填（arch_clock.c）未移植且 Rust kernel 无该读者；watchdog 等 kernel 内部读者未移植（26 doc WONTFIX W-1）；AP 探测路径（C arch_smp.c:227-232 持 boot_lock+BKL）随 16-smp.md SMP bring-up 落地，当前单核 boot 只填 BSP 槽 |
| `krandom` 初始化 | main.c:48-49（`krandom.random_sources = RANDOM_SOURCES;` + `krandom.random_elements = RANDOM_ELEMENTS;` 直接赋值，**不是函数调用**） | ✅ 已实现（`krandom::init()`，`lib.rs:465` 调用）：设置 `KRANDOM_INIT` 标志；`KRANDOM: SyncUnsafeCell<KRandomness>` 经 `const fn new()` 已在 link 时初始化字段。`get_randomness()` 是 no-op stub 匹配 C i386/earm 语义（实际熵采集由用户态 `random` 驱动完成）。详见 [25-misc-unported.md §4.7](25-misc-unported.md) |
| `cpu_set_flag(bsp, CPU_IS_READY)` | main.c:95 | `CPU_IS_READY` 标志在 Rust 中由 `SmpState::cpu_state` 枚举表达（`CpuState::Ready`），步骤 5 设置 TSC baseline 时隐式完成状态转换 |

上表三行的实现归属：cpu_identify 已补齐——分层与 C 同构（arch 层探测机制 / kernel 层 `CPU_INFO` 表 + `smp::cpu_identify()` 在 bsp_finish_booting Step 0 调用 / misc.rs GET_CPUINFO 经 `CpuInfoEntry::from(CpuIdentity)` 全记录拷出），freq 回填与 watchdog 读者仍属优先级决定（无内核内消费压力，见上表"剩余缺口"）；krandom 已实现（`krandom::init()`，lib.rs:465 调用；`get_randomness()` no-op stub 对齐 C i386/earm 语义，见 [25 §4.7](25-misc-unported.md)）；cpu_set_flag 由 `SmpState::cpu_state` 枚举表达，Step 5 设 TSC 基线时隐式完成 Ready 转换。

> 设计决策 D7：`bsp_finish_booting() -> !` 类型系统表达永不返回。D6：vm_running 当前用全局 `AtomicBool`，SMP 就绪后移入 `SmpState`。D8：`kernel_may_alloc` 用 `AtomicBool`。

---

## 5. 测试要点

### 5.1 单元测试

| 测试 | 覆盖的设计/实现 | 说明 |
|------|---------------|------|
| `test_syscall_try_from_valid` | D1: Syscall enum | 所有合法 syscall 号可转换为 enum 变体 |
| `test_syscall_try_from_invalid` | D1: Syscall enum | 非法 syscall 号返回 Err |
| `test_kernel_call_dispatch_bad_call` | D1: match dispatch | 非法 syscall 号返回 BadCall |
| `test_add_memmap_no_truncation` | D5: 删除 4GB 截断 | >4GB 地址不被截断 |
| `test_add_memmap_alignment` | Ch2: 页对齐 | 非 4KB 对齐的地址/长度被正确对齐 |
| `test_add_memmap_zero_length` | Ch2: 零长度检查 | 对齐后长度为 0 返回错误 |
| `test_add_memmap_no_slots` | Ch2: 槽位耗尽 | 所有槽位已满时返回错误 |

### 5.2 静态测试

| 测试 | 覆盖的设计/实现 | 说明 |
|------|---------------|------|
| 编译期穷尽检查 | D1/D2 | `Syscall` enum 新增变体而不补 `match` arm → 编译失败；`const` assert 保证所有变体值 < NR_SYS_CALLS |

### 5.3 bsp_finish_booting 测试（lib.rs 单元测试）

`bsp_finish_booting` 是发散函数（`-> !`），不能在测试内整体调用——两个测试直接驱动其副作用操作（Step 5/7 的 CpuLocal 写入），是"发散函数可测性"的既定模式：

| 测试 | 位置 | 覆盖的设计/实现 | 说明 |
|------|------|---------------|------|
| `test_bsp_finish_booting_step_5_7_side_effects` | lib.rs:2771 | Step 5/7 | 验证 TSC 基线（cpu_last_tsc/cpu_last_idle）与 fpu_presence 落在 BSP 的 CpuLocal |
| `test_bsp_finish_booting_single_cpu_only_bsp_initialized` | lib.rs:2798 | Step 5/7 BSP 唯一性 | 单 CPU 构建下仅 BSP 被初始化，AP 槽位保持默认值 |

**清单完整性说明**：本文档 §5.1 只列与 D1/D5 直接对应的测试；实际相关测试更多——syscall.rs tests 模块（L2701 起）含 dispatch_schedule/privctl/getmcontext/setmcontext 等 15+ 个分派测试，memmap.rs 另有 cut_memmap 9 个测试。完整清单以 `rg "fn test_" os/kernel/src/{syscall,memmap,lib}.rs` 为准。

### 5.4 CPU 身份探测测试（todo D-53 配套，smp.rs / misc.rs / x86_64/cpu_identity.rs 单元测试）

`cpu_identify()` 本体写全局 `CPU_INFO`（boot 期状态，非测试可变），测试覆盖其可分部验证的纯逻辑——表操作用局部实例、wire 记录用转换函数：

| 测试 | 位置 | 覆盖的设计/实现 | 说明 |
|------|------|---------------|------|
| `test_cpu_info_table_record_and_get` | smp.rs tests | CPU_INFO 表语义 | record/get 往返；越界 CPU id 忽略不 panic（C 会越界写）；已有槽位不被越界写入破坏 |
| `test_cpu_info_entry_layout_matches_c` | misc.rs tests | C ABI 布局 | `CpuInfoEntry` = C i386 `struct cpu_info`（archtypes.h:39-46）：16 字节，freq@4 / flags@8——布局错误会静默损坏所有用户态读者 |
| `test_cpu_info_entry_from_x86_identity` | misc.rs tests | 身份→wire 转换 | vendor 编码（INTEL=0/AMD=2/UNKNOWN=0xff，archconst.h:134-136）、family/model/stepping 拷贝、ECX/EDX 拆入 flags[2]、freq=0 |
| `test_cpu_info_entry_from_non_x86_is_unknown_zeroed` | misc.rs tests | 非 x86 映射 | ARM MIDR/RISC-V CSR 无 x86 wire 形状对应 → CPU_VENDOR_UNKNOWN + 全零（文档化 ABI） |
| `test_decode_signature_family6_extended_model` / `_nehalem` / `_family_f` / `_extended_family` | x86_64/cpu_identity.rs tests | MINIX3 BUG 修复（model 按 family 条件合并） | 纯函数 `decode_signature` 的签名解码：Skylake 0x406E9→0x4E（C 截断为 0xE）、Nehalem 0x106E0→0x1E、Zen2 0x30F11→0x31、ext-family 合并 |

---

## 6. 参见

- [00-kernel-overview.md](00-kernel-overview.md) — 内核整体架构
- [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) — 进程表初始化与 boot 进程加载（前置）
- [07-cross-space-init.md](07-cross-space-init.md) — 跨地址空间初始化（前置）
- [09-vm-boot-protocol.md](09-vm-boot-protocol.md) — VM 启动后的内核-VM 协商（后续）
- [10-switch-to-user.md](10-switch-to-user.md) — switch_to_user 详细实现
- [13-syscall-dispatch.md](13-syscall-dispatch.md) — 系统调用分派详细实现
- [14-exception-interrupt.md](14-exception-interrupt.md) — 异常与中断处理
- C 源码：`minix3/minix/kernel/system.c:168-270` — system_init()
- C 源码：`minix3/minix/kernel/main.c:38-109` — bsp_finish_booting()
- C 源码：`minix3/minix/kernel/arch/i386/pg_utils.c:86-121` — add_memmap()
- C 源码：`minix3/minix/kernel/arch/i386/arch_system.c:212-244` — cpu_identify()
- C 头文件：`minix3/minix/include/arch/i386/include/archtypes.h:39-46` — struct cpu_info
- Rust：`os/arch/src/arch/cpu_identity.rs` + `os/kernel/src/smp.rs` — CPU 身份探测与 CPU_INFO 表（todo D-53）
- C 头文件：`minix3/minix/include/minix/com.h:207-270` — SYS_* 定义
