# 06-structure.md — 06-proc-init-boot-proc.md 知识点结构与重组诊断（v2，源码驱动）

> **目的**：从 **minix3 源码** 与 **Rust rewrite 实现** 出发，全量罗列 06 文档应当覆盖的知识点；按 OS 概念维度组织（而非按章节）；诊断 Ch1→Ch2→Ch3→Ch4→Ch5 纵向链路断裂点与"开发文档味"问题；提出重组方案。
>
> **方法**：① 先用 `coverage-extract.py` 机器穷举 C 源码符号；② 逐文件精读 minix3 关键源码（proc.c / main.c / system.c / protect.c / arch_system.c / exec_elf.c / priv.h / proc.h / const.h / com.h / param.h）；③ 逐文件精读 Rust 实现（os/kernel/proc.rs / proc_table.rs / kpriv.rs / capability.rs / lib.rs + os/arch/{arch,x86_64,arm64,riscv64}/boot.rs）；④ 按 OS 概念维度归并知识点；⑤ 对照当前 06 文档找出缺口。
>
> **范围声明**：本文档覆盖 Stage C（boot 期进程初始化 + VM ELF 加载）。Stage F（SMP 调度）与 fork 运行时路径仅作为"预留/对照"提及，不展开。

---

## 一、知识点全集（按 OS 概念维度组织，源码驱动）

> 每个知识点标注 **C 源码锚点**（`file:line`）与 **Rust 锚点**（`file:line`），缺一不可。

### 概念组 A：进程抽象与进程表

| 维度 | 应覆盖的知识点 | C 源码锚点 | Rust 锚点 |
|------|--------------|-----------|----------|
| **A.0 进程概念本身** | 进程 = CPU 单执行流的抽象 + 可保存/可恢复的状态；单 CPU 单执行流 vs 多进程抽象的矛盾；进程表是"保存进程状态的数据结构"，是矛盾的桥梁 | （概念，无直接锚点；隐含于 `proc[]` 设计） | （概念） |
| **A.1 进程表布局** | `EXTERN struct proc proc[NR_TASKS + NR_PROCS]`；前 NR_TASKS 项是内核 task（p_nr<0），后 NR_PROCS 项是用户进程（p_nr≥0）；`BEG_PROC_ADDR`/`END_PROC_ADDR`/`BEG_USER_ADDR` 宏；`proc_addr(n) = &proc[NR_TASKS + n]` | proc.h:283, proc.h:266-269 | proc_table.rs: `PROC_TABLE_SIZE = NR_TASKS + NR_PROCS` |
| **A.2 进程号判定宏** | `isokprocn(n)`、`iskerneln(n)`、`isrootsysn(n)`、`isemptyp(p)`、`proc_ptr_ok(p)` | proc.h:272-279, proc.h:174 | proc_table.rs: `is_valid_nr` / `is_kernel` / `is_empty` |
| **A.3 进程标识符** | `p_nr`（proc_nr_t = int，槽索引，内核 task 为负）；`p_endpoint`（endpoint_t = generation+slot，防陈旧引用）；`_ENDPOINT(g,n)` 构造；`PMAGIC=0xC0FFEE1` 校验字 | proc.h:25,82; const.h:164; com.h | proc.rs: `p_nr: ProcNr`、`p_endpoint: Endpoint`、`#[cfg(debug_assertions)] p_magic` |
| **A.4 内核 task 端点常量** | ASYNCM=-5, IDLE=-4, CLOCK=-3, SYSTEM=-2, KERNEL=-1 | com.h:47-51 | proc.rs: `proc_nr::{IDLE,CLOCK,SYSTEM,KERNEL}` + `KERNEL_TASKS` 表 |
| **A.5 用户进程端点常量** | DS=0, RS=1, PM=2, SCHED=3, VFS=4, MEM=5, TTY=6, MIB=7, VM=8, PFS=9, MFS=10, INIT=11；`ROOT_SYS_PROC_NR=RS=2`、`ROOT_USR_PROC_NR=INIT=11` | com.h:60-78 | proc.rs: `BOOT_MODULE_PROC_NRS`、`proc_nr::{RS_PROC_NR,VM_PROC_NR}` |
| **A.6 slot 生命周期** | SLOT_FREE → 分配（清 SLOT_FREE）→ 运行 → 退出 → 回收（设 SLOT_FREE）；`isemptyp(p) = (p->p_rts_flags == RTS_SLOT_FREE)` | proc.h:141, proc.h:274 | proc_table.rs: `is_empty` |
| **A.7 proc_init() 实现** | 清空进程表：每 slot 设 `p_rts_flags=RTS_SLOT_FREE`、`p_magic=PMAGIC`、`p_nr=i`、`p_endpoint=_ENDPOINT(0,p_nr)`、`p_scheduler=NULL`、`p_priority=0`、`p_quantum_size_ms=0`；调 `arch_proc_reset(rp)`；IDLE 每 CPU 一个，共享 `idle_priv`，`p_endpoint=IDLE`，`RTS_PROC_STOP`；`set_idle_name()` 生成 "idle0"/"idle1"/... | proc.c:119-161 | proc_table.rs: `ProcessTable::new()` const fn + lib.rs: `init_proc_and_boot` Step 3a IDLE 分支 |
| **A.8 Rust ProcessTable 类型设计** | `pub struct ProcessTable { procs: [KProcess; PROC_TABLE_SIZE], sched: Scheduler, vm_request_queue: VmRequestQueue }`；`const fn new()` 编译期初始化；`static mut PROC_TABLE` BSS 全局；`get(nr)`/`get_mut(nr)` 返回 `Option` 强制边界检查（C 的 `proc_addr` 无检查）；`iter()`/`iter_mut()` 替代 `BEG_PROC_ADDR..END_PROC_ADDR` 循环；`endpoint_to_nr(e)` 替代 `isokendpt()`；SMP 安全：所有方法要求持有 BKL | （C 无对应类型，散落全局数组） | proc_table.rs: `ProcessTable` |
| **A.9 Rust KProcess 结构体** | `p_nr`/`p_endpoint`/`p_seg`/`priv_id: Option<PrivId>`（替代 C `struct priv *p_priv` 指针）/`p_rts_flags`/`p_misc_flags`/`p_sched`/`p_accounting`/`p_time`/`p_cycles`/`p_cpuavg`/`p_nextready: AtomicI32`/`p_caller_q: AtomicI32`/`p_q_link: AtomicI32`/`p_getfrom_e`/`p_sendto_e`/`p_pending`/`p_name`/`p_sendmsg`/`p_delivermsg`/`p_delivermsg_vir`/`cpu_context: CurrentCpuContext`/`p_next_restart`/`p_next_requestor`/`p_vm_suspend: Option<VmSuspendContext>` | proc.h:283-330 | proc.rs:754-909 |
| **A.10 指针→索引的 rewrite 决策** | C 用 `struct proc *p_priv`、`struct proc *p_nextready`、`struct proc *p_scheduler` 等裸指针；Rust 用 `Option<PrivId>`/`AtomicI32`/`Option<ProcNr>` 索引替代，避免裸指针跨 CPU 共享、便于边界检查、便于 const fn 构造 | proc.h:25, 283-330 | proc.rs: `priv_id`/`p_nextready`/`p_sched.scheduler` |

### 概念组 B：特权表与能力模型

| 维度 | 应覆盖的知识点 | C 源码锚点 | Rust 锚点 |
|------|--------------|-----------|----------|
| **B.1 特权表为什么独立** | `EXTERN struct priv priv[NR_SYS_PROCS]`；`EXTERN struct priv *ppriv_addr[NR_SYS_PROCS]` 直接槽指针加速；稀缺资源：NR_SYS_PROCS(64) << NR_PROCS(256)；系统进程各有一个 priv，用户进程共享 USER_PRIV_ID；空间效率：common 字段在 proc，privileged 字段在 priv | priv.h:94-95, priv.h:1-10 | kpriv.rs: `PrivTable { privs: [KPriv; NR_SYS_PROCS] }` |
| **B.2 priv 结构体字段全集** | `s_proc_nr`/`s_id`/`s_flags`/`s_init_flags`/`s_asyntab`/`s_asynsize`/`s_asynendpoint`/`s_trap_mask`/`s_ipc_to`(sys_map_t)/`s_k_call_mask[SYS_CALL_MASK_SIZE]`/`s_sig_mgr`/`s_bak_sig_mgr`/`s_notify_pending`/`s_asyn_pending`/`s_int_pending`/`s_sig_pending`/`s_ipcf`/`s_alarm_timer`/`s_stack_guard`/`s_diag_sig`/`s_nr_io_range`/`s_io_tab[NR_IO_RANGE]`/`s_nr_mem_range`/`s_mem_tab[NR_MEM_RANGE]`/`s_nr_irq`/`s_irq_tab[NR_IRQ]`/`s_grant_table`/`s_grant_entries`/`s_grant_endpoint`/`s_state_table`/`s_state_entries` | priv.h:21-58 | kpriv.rs: `KPriv` 6 子结构 |
| **B.3 特权表布局宏** | `BEG_PRIV_ADDR`/`END_PRIV_ADDR`/`BEG_STATIC_PRIV_ADDR`/`END_STATIC_PRIV_ADDR`/`BEG_DYN_PRIV_ADDR`/`END_DYN_PRIV_ADDR`；静态区前 NR_STATIC_PRIV_IDS(=NR_BOOT_PROCS) 个 boot 期分配；动态区后续运行时分配 | priv.h:65-72 | kpriv.rs: `NR_STATIC_PRIV_IDS` 常量 + `assign_static`/`assign_dynamic` 分区 |
| **B.4 特权 ID 与映射宏** | `priv_addr(i)=ppriv_addr[i]`；`priv_id(rp)=rp->p_priv->s_id`；`priv(rp)=rp->p_priv`；`id_to_nr(id)`/`nr_to_id(nr)`；`may_send_to(rp,nr)=get_sys_bit(priv(rp)->s_ipc_to, nr_to_id(nr))`；`NR_STATIC_PRIV_IDS=NR_BOOT_PROCS`；`is_static_priv_id(id)`；`static_priv_id(n)=NR_TASKS+n`；`USER_PRIV_ID=static_priv_id(ROOT_USR_PROC_NR)`；`NULL_PRIV_ID=-1` | priv.h:73-86; minix/priv.h:10-21 | kpriv.rs: `PrivId` newtype + `assign_static`/`grant_capability` |
| **B.5 默认特权设置表** | 标志：`IDL_F=SYS_PROC\|BILLABLE`、`TSK_F=SYS_PROC`、`SRV_F=SYS_PROC\|PREEMPTIBLE`、`DSRV_F=SRV_F\|DYN_PRIV_ID`、`RSYS_F=SRV_F\|ROOT_SYS_PROC`、`VM_F=SYS_PROC\|VM_SYS_PROC`、`USR_F=BILLABLE\|PREEMPTIBLE`、`IMM_F=ROOT_SYS_PROC\|VM_SYS_PROC\|PREEMPTIBLE`；陷阱：`CSK_T=1<<RECEIVE`、`TSK_T=0`、`SRV_T=~0`、`USR_T=1<<SENDREC`；目标：`TSK_M=NO_M`、`SRV_M=ALL_M`、`USR_M=ALL_M`；内核调用：`TSK_KC=NO_C`、`SRV_KC=ALL_C`、`USR_KC=NO_C`；信号管理器：`SRV_SM=ROOT_SYS_PROC_NR`、`USR_SM=PM_PROC_NR`；调度器：`SRV_SCH=KERNEL`、`DSRV_SCH=SCHED_PROC_NR`、`USR_SCH=NONE` | minix/priv.h:36-100 | capability.rs: `CapabilityTemplate::{Idle,KernelTask,Vm,RootService,Deferred}` 各变体的 `capabilities()`/`trap_mask()`/`ipc_mask()`/`kcall_mask()` |
| **B.6 get_priv() 实现** | 静态分配：`priv_id != NULL_PRIV_ID` → `sp=&priv[priv_id]`，校验 `is_static_priv_id` + 槽未占用；动态分配：`priv_id == NULL_PRIV_ID` → 遍历 `BEG_DYN_PRIV_ADDR..END_DYN_PRIV_ADDR` 找空闲槽；错误码：`ENOSPC`/`EINVAL`/`EBUSY`；关联：`rc->p_priv=sp; sp->s_proc_nr=proc_nr(rc)` | system.c:274-301 | kpriv.rs: `assign_static`/`assign_dynamic` 返回 `Result<PrivId, CapabilityError>` |
| **B.7 set_sendto_bit / fill_sendto_mask** | `set_sendto_bit(rp,id)` 设 IPC 目标位，保持对称性（双向都设）；`unset_sendto_bit(rp,id)` 清除，保持对称性；`fill_sendto_mask(rp,map)` 遍历 NR_SYS_PROCS 按 map 设/清每位；对称性：A 能发到 B ⇔ B 能发到 A（除非 B 只支持 RECEIVE） | system.c:307-360 | capability.rs: `IpcMask::may_send_to` + kpriv.rs: `configure_boot_priv` 内部调用 |
| **B.8 Rust PrivTable 类型设计** | `pub struct PrivTable { privs: [KPriv; NR_SYS_PROCS] }`；`NR_SYS_PROCS=64`；`const fn new()` 编译期初始化；`static mut PRIV_TABLE` BSS 全局；`get(id)`/`get_mut(id)` 返回 Option；`assign_static(proc_nr)` 替代 `get_priv(rp, static_priv_id(proc_nr))`；`configure_boot_priv(priv_id, flags, init_flags, trap_mask, ipc_to, k_call_mask, sig_mgr)` 替代 main.c:178-248 逐字段设置；`grant_capability(proc_nr, template)` 新 API 单一调用封装 assign + configure | （C 无对应类型） | kpriv.rs: `PrivTable` |
| **B.9 Rust KPriv 6 子结构设计** | `PrivCapability`（s_proc_nr/s_id/s_flags/s_init_flags）；`PrivSignals`（s_asyntab/s_asynsize/s_asynendpoint/s_sig_mgr/s_bak_sig_mgr/s_notify_pending/s_asyn_pending/s_int_pending/s_sig_pending）；`PrivIpc`（s_trap_mask/s_ipc_to/s_k_call_mask）；`PrivIo`（s_nr_io_range/s_io_tab/s_nr_irq/s_irq_tab）；`PrivMem`（s_nr_mem_range/s_mem_tab/s_ipcf/s_stack_guard/s_diag_sig）；`PrivRuntime`（s_alarm_timer/s_grant_table/s_grant_entries/s_grant_endpoint/s_state_table/s_state_entries）；每子结构 `const fn new()` 可编译期构造 | priv.h:21-58（C 单一大结构体） | kpriv.rs:228-289 |
| **B.10 Rust CapabilityTemplate 设计** | `enum CapabilityTemplate { Idle, KernelTask, Vm, RootService, Deferred }`；5 变体对应 5 类进程角色；`capabilities()`→ProcessCapability（bitflags）；`trap_mask()`→TrapMask；`ipc_mask()`→IpcMask；`kcall_mask()`→KCallMask；替代旧 API：assign_static + configure_boot_priv 两步合一；错误：`CapabilityError { InvalidProcNr, SlotOccupied, NoFreeSlots }` | （C 无对应抽象，散落 main.c:178-248 if-else） | capability.rs:104-180 |
| **B.11 Rust Newtype 类型安全** | `TrapMask(u32)` 陷阱掩码；`IpcMask(u64)` IPC 目标掩码，`may_send_to(sys_id)` 方法；`KCallMask(u64)` 内核调用掩码；类型系统强制：不能把 IpcMask 传给 KCallMask 参数；`PrivId(u32)` newtype 替代 C 的裸 int | （C 全是裸 int/bitchunk_t） | capability.rs:230-330, kpriv.rs: `PrivId` |
| **B.12 boot 期特权授予流程** | main.c:178-248 按 proc_nr 分支：VM→VM_F+SRV_T+SRV_M+SRV_KC+PM_SM；内核 task→(IDLE?IDL_F:TSK_F)+TSK_T+NO_M+NO_C；RS→RSYS_F+SRV_T+SRV_M+SRV_KC+ROOT_SYS_SM；其他用户→RTS_NO_PRIV\|RTS_NO_QUANTUM（等 RS 运行时设） | main.c:178-248 | lib.rs: `init_proc_and_boot` Step 3a/3b 的 `grant_capability(nr, template)` 调用 |

### 概念组 C：RTS 标志与进程状态

| 维度 | 应覆盖的知识点 | C 源码锚点 | Rust 锚点 |
|------|--------------|-----------|----------|
| **C.1 RTS 标志位全集** | `RTS_SLOT_FREE=0x01`/`RTS_PROC_STOP=0x02`/`RTS_SENDING=0x04`/`RTS_RECEIVING=0x08`/`RTS_SIGNALED=0x10`/`RTS_SIG_PENDING=0x20`/`RTS_P_STOP=0x40`/`RTS_NO_PRIV=0x80`/`RTS_NO_ENDPOINT=0x100`/`RTS_VMINHIBIT=0x200`/`RTS_PAGEFAULT=0x400`/`RTS_VMREQUEST=0x800`/`RTS_VMREQTARGET=0x1000`/`RTS_PREEMPTED=0x4000`/`RTS_NO_QUANTUM=0x8000`/`RTS_BOOTINHIBIT=0x10000` | proc.h:141-167 | proc.rs: `RtsFlagsBits` bitflags 17 位 |
| **C.2 为什么用位图而非枚举** | "A process is runnable iff p_rts_flags == 0"（proc.h:168）；多原因可叠加（如 SENDING+SIGNALED）；枚举只能表达单一状态，位图可表达多原因组合；`rts_f_is_runnable(flg)=((flg)==0)`、`proc_is_runnable(p)=rts_f_is_runnable(p->p_rts_flags)` | proc.h:168-170 | proc.rs: `RtsFlags::is_runnable` |
| **C.3 RTS 宏与调度队列一致性** | `RTS_ISSET(rp,f)`；`RTS_SET(rp,f)` 设标志，若从可运行变不可运行则 `dequeue(rp)`；`RTS_UNSET(rp,f)` 清标志，若从不可运行变可运行则 `enqueue(rp)`；`RTS_SETFLAGS(rp,f)` 直接设值；关键：RTS_SET/UNSET 自动维护调度队列一致性 | proc.h:202-230 | proc_table.rs: `rts_set`/`rts_unset` + Scheduler 集成 |
| **C.4 阶段 C 涉及的 RTS 位** | `RTS_NO_PRIV\|RTS_NO_QUANTUM`（非调度进程，等 RS 设特权）；`RTS_VMINHIBIT\|RTS_BOOTINHIBIT`（用户进程等 VM 设页表）；`RTS_PROC_STOP`（所有 boot 进程初始停止）；清 `RTS_SLOT_FREE`（槽已占用） | main.c:226, 264-273 | lib.rs: `init_proc_and_boot` Step 3a/3b 的 `rts_flags.set/clear` |
| **C.5 Rust RtsFlags 设计** | `bitflags! { pub struct RtsFlagsBits: u32 { ... } }` 17 位全保留；`RtsFlags` 包装类型（含 AtomicU32）；`rts_set(nr,flags)`/`rts_unset(nr,flags)` 替代 C 宏，自动维护调度队列；"非零=不可运行"不变量通过 `is_runnable()` 方法表达 | （C 是裸 int + 宏） | proc.rs:130-150, proc_table.rs: `rts_set`/`rts_unset` |
| **C.6 misc_flags（次要标志全集）** | `MF_REPLY_PEND=0x001`（IPC_REQUEST 回复待处理）、`MF_VIRT_TIMER=0x002`（虚拟定时器运行）、`MF_PROF_TIMER=0x004`（profile 定时器运行）、`MF_KCALL_RESUME=0x008`（内核调用被中断待恢复）、`MF_DELIVERMSG=0x040`（运行前复制消息）、`MF_SIG_DELAY=0x080`（发送结束后发信号）、`MF_SC_ACTIVE/DEFER/TRACE=0x100/200/400`（syscall 追踪三态）、`MF_FPU_INITIALIZED=0x1000`（FPU 已初始化，`proc_used_fpu(p)` 宏检测此位）、`MF_SENDING_FROM_KERNEL=0x2000`（消息来自内核）、`MF_CONTEXT_SET=0x4000`（禁止触碰上下文）、`MF_SPROF_SEEN=0x8000`、`MF_FLUSH_TLB=0x10000`（SMP TLB 刷新）、`MF_SENDA_VM_MISS=0x20000`、`MF_STEP=0x40000`（单步）、`MF_MSGFAILED=0x80000`、`MF_NICED=0x100000`（用户降低优先级）；与 RTS 区别：RTS 决定可运行性（ dequeue/enqueue），misc_flags 记录次要运行时状态（不影响调度队列） | proc.h:234-262 | proc.rs: `MiscFlags` bitflags |
| **C.7 P_BLOCKEDON 宏** | `P_BLOCKEDON(p)`：返回进程阻塞在哪个端点上——`RTS_SENDING` → `p_sendto_e`；`RTS_RECEIVING` → `p_getfrom_e`；否则 `NONE`；先查 SENDING 再查 RECEIVING（因 sendrec 可能同时置位，p_getfrom_e 此时无意义） | proc.h:187-198 | proc.rs: `blocked_on()` 方法 |
| **C.8 proc_kernel_scheduler 宏** | `proc_kernel_scheduler(p) = (p->p_scheduler == NULL \|\| p->p_scheduler == p)`；判断进程是否由内核默认调度（无用户态调度器或自调度）；与 M.4 的 `p_scheduler` 字段配合 | proc.h:178-179 | proc.rs: `SchedFields.scheduler: Option<ProcNr>` + `is_kernel_scheduled()` 方法 |

### 概念组 D：boot image 与启动清单

| 维度 | 应覆盖的知识点 | C 源码锚点 | Rust 锚点 |
|------|--------------|-----------|----------|
| **D.1 boot image 为什么编译时硬编码** | bootstrapping：boot 期无文件系统；`struct boot_image image[NR_BOOT_PROCS]`；顺序必须与 boot image 中程序顺序一致；内核 task 必须在前；DS 必须是第一个系统进程（异步发布系统事件）；RS 紧随其后（ping 消息优先级） | table.c:44-66 | proc.rs: `KERNEL_TASKS` + `BOOT_MODULE_PROC_NRS` 常量数组 |
| **D.2 boot image 三类进程** | 内核 task（5 个）：ASYNCM/IDLE/CLOCK/SYSTEM/KERNEL；系统服务（用户态）：DS/RS/PM/SCHED/VFS/MEM/TTY/MIB/VM/PFS/MFS；普通用户进程：INIT | table.c:47-65; com.h:47-71 | proc.rs: `KERNEL_TASKS` + `BOOT_MODULE_PROC_NRS` |
| **D.3 boot_image 结构体** | `struct boot_image { proc_nr_t proc_nr; char *proc_name; }`；简单：只有进程号和名字；其他属性（start_addr/len/endpoint）在 main.c 循环中填充 | table.c:44 | （Rust 用 `BootModule` from minix_boot crate + `KERNEL_TASKS` 元组数组） |
| **D.4 NR_BOOT_PROCS vs NR_BOOT_MODULES** | `NR_BOOT_PROCS = NR_TASKS + LAST_SPECIAL_PROC_NR + 1 = 5+11+1 = 17`；`NR_BOOT_MODULES = INIT_PROC_NR+1 = 12`；NR_BOOT_PROCS 包含内核 task，NR_BOOT_MODULES 只含用户态模块；main.c:160 校验 `NR_BOOT_MODULES == kinfo.mbi.mi_mods_count` | param.h:9; com.h:74; main.c:160 | proc.rs: `NR_BOOT_MODULES=12`、`NR_BOOT_PROCS=NR_TASKS+NR_BOOT_MODULES=17` |
| **D.5 boot 循环逻辑（main.c 核心循环）** | 遍历 `image[0..NR_BOOT_PROCS]`；`ip=&image[i]; rp=proc_addr(ip->proc_nr)`；`ip->endpoint=rp->p_endpoint`（同步端点）；内核 task（i<NR_TASKS）：复制名字；用户进程（i≥NR_TASKS）：从 `kinfo.module_list[i-NR_TASKS]` 取 start_addr/len；`reset_proc_accounting(rp)`；判断 schedulable_proc：`iskerneln\|\|isrootsysn\|\|VM_PROC_NR`；schedulable：`get_priv(rp, static_priv_id(proc_nr))` + 按类型设特权；非 schedulable：`RTS_SET(rp, RTS_NO_PRIV\|RTS_NO_QUANTUM)`；`arch_boot_proc(ip,rp)` 架构特定初始化（VM ELF 加载）；非 VM 用户进程：`RTS_VMINHIBIT\|RTS_BOOTINHIBIT`；所有进程：`RTS_PROC_STOP`，清 `RTS_SLOT_FREE` | main.c:165-282 | lib.rs: `init_proc_and_boot` Step 3a/3b |
| **D.6 Rust boot image 类型设计** | `KERNEL_TASKS: &[(&str, ProcNr); 5]` 编译期固定列表；`BOOT_MODULE_PROC_NRS: &[ProcNr; 12]` 用户态模块进程号；`NR_BOOT_MODULES=12`、`NR_BOOT_PROCS=17`；替代 C 的 `image[]` 数组；`BootModule` from minix_boot crate 含 name/start/len | table.c:44 | proc.rs:71-103 |
| **D.7 Rust init_proc_and_boot() 主流程** | Step 1+2: 获取全局 proc_table/priv_table；校验 `kernel_info.boot_modules.len()==NR_BOOT_MODULES`；Step 3a: 遍历 KERNEL_TASKS，设名字 + grant_capability + build_cpu_context + RTS_PROC_STOP；Step 3b: 遍历 boot_modules，设名字 + 判断 schedulable + grant_capability 或 RTS_NO_PRIV + build_cpu_context + VMINHIBIT/BOOTINHIBIT + RTS_PROC_STOP；Step 4: 更新 boot_procs 信息（Rust 中 kernel_info 已含） | main.c:165-282 | lib.rs:699-870 |
| **D.8 schedulable 判定** | `schedulable_proc = iskerneln(proc_nr) \|\| isrootsysn(proc_nr) \|\| proc_nr == VM_PROC_NR`；含义：内核 task + RS + VM 立即可调度；其他用户进程需等 RS 运行时设特权 | main.c:173-174 | lib.rs: `is_root_sys \|\| is_vm` |

### 概念组 E：CPU 上下文与寄存器初始化

| 维度 | 应覆盖的知识点 | C 源码锚点 | Rust 锚点 |
|------|--------------|-----------|----------|
| **E.1 CPU 四问** | ① 寄存器初值（PSW/PSR/sstatus + 段选择子）；② 用户态入口（PC/ELR/sepc）；③ 栈和参数（SP + ps_strings）；④ VM 地址空间（页表根） | （隐含于 arch_proc_reset + arch_proc_init + arch_boot_proc 三函数） | arch/boot.rs: `CpuContextArch` trait + `EntrySpec` |
| **E.2 arch_proc_reset()** | x86: 分配 FPU 状态区（FPUALIGN 对齐）+ memset 清零 + 设 PSW（INIT_TASK_PSW/INIT_PSW）+ 设段选择子（USER_CS/DS_SELECTOR）+ arch_proc_setcontext(KTS_FULLCONTEXT)；ARM: memset 清零 p_reg + 设 PSR；区别：x86 有 FPU 状态区 + 段选择子，ARM 无 | arch_system.c:146-182(x86), arch_system.c:42-53(ARM) | arch/{x86_64,arm64,riscv64}/boot.rs: `build_cpu_context` |
| **E.3 arch_proc_init()** | 调用 arch_proc_reset(pr) 重新清零；strlcpy 名字；设 `p_reg.pc=ip`、`p_reg.sp=sp`、`p_reg.bx=ps_str`；bx 寄存器存 ps_strings 地址（x86 约定） | memory.c:722-735(x86) | （Rust 合并进 build_cpu_context，无独立函数） |
| **E.4 arch_boot_proc()** | `if(rp->p_nr < 0) return` 内核 task 不处理；`mod=bootmod(rp->p_nr)` 取 boot module；仅 VM_PROC_NR 特殊处理：构造 `exec_info execi`（stack_high/stack_size/proc_e/hdr/filesize/progname/frame_len + 回调）；设回调 copymem/clearmem/allocmem_prealloc_junk/allocmem_prealloc_cleared/allocmem_ondemand/clearproc；`libexec_load_elf(&execi)` 解析 ELF + 映射到 bootstrap 页表；设置 ps_strings 结构（argv/envp）；调整 SP（下移 3 字：argc/argv/envp）；`arch_proc_init(rp, execi.pc, sp, ps_str, ip->proc_name)`；`add_memmap` 记录 VM blob；清 mod_start/mod_end；记录 `kinfo.vm_allocated_bytes` | protect.c:388-455(x86), protect.c:115-182(ARM) | arch/boot.rs: `load_vm_elf` + `build_cpu_context(ProcKind::Vm, ...)` |
| **E.5 三架构状态寄存器初值** | x86: `INIT_PSW=0x0202`（用户态，IF=1）、`INIT_TASK_PSW=0x1202`（内核态，IOPL=1）；ARM: `INIT_PSR=0x0000_0000`（用户态）、`INIT_TASK_PSR=0x0000_03C5`（内核态）；RISC-V: `INIT_USER_SSTATUS`（SPP=0,SPIE=1）、`INIT_TASK_SSTATUS`（SPP=1） | x86: const.h; ARM: earm/include/archconst.h:11-12; RISC-V: riscv/include/archconst.h | arch/{x86_64,arm64,riscv64}/boot.rs: `INIT_PSW`/`INIT_TASK_PSW`/`INIT_PSR`/`INIT_TASK_PSR`/`INIT_USER_SSTATUS`/`INIT_TASK_SSTATUS` 常量 |
| **E.6 ps_strings 机制** | `struct ps_strings { char **ps_argvstr; int ps_nargvstr; char **ps_envstr; int ps_nenvstr; }`；放在栈顶下方；SP 下移 3 字（argc/argv/envp）给启动代码；x86: bx 存 ps_strings 地址；ARM: r0；RISC-V: a0 | protect.c:425-441 | arch/boot.rs: `EntrySpec::loaded(pc, sp, ps_strings)` + `VmLoadResult.ps_strings` |
| **E.7 Rust CpuContextArch trait 设计** | `trait CpuContextArch { type CpuContext; type TrapFrame; fn build_cpu_context(kind, proc_nr, entry) -> CpuContext; fn apply_to_trap_frame(ctx, frame); fn enable_user_io(ctx); fn inherit_fpu_state(child, parent); }`；单 trait 替代 C 的 3 函数（arch_proc_reset + arch_proc_init + arch_boot_proc）；CpuContext 是关联类型，arch 私有，kernel 层不检视字段；ProcKind 枚举：KernelTask/Vm/RootService/UserService/UserProcess；EntrySpec 结构：pc/sp/ps_strings 均为 Option，明确"未知"语义 | （C 是 3 个独立 arch 函数） | arch/boot.rs:138-200 |
| **E.8 Rust 三架构 CpuContext 实现** | x86_64: `X86_64CpuContext { psw, cs, ds, ss, es, fs, gs, pc, sp, bx, fpu_policy }` + `X86FpuInitPolicy { KernelTask, LazyUserInit }`；aarch64: `AArch64CpuContext { psr, pc, sp, r0, fpu_enable_el0 }`；riscv64: `Riscv64CpuContext { sstatus, sepc, sp, a0 }` | （C 是 `struct proc` 内嵌 `struct proc_reg` + FPU 区） | arch/{x86_64,arm64,riscv64}/boot.rs |
| **E.9 Rust FPU 现代模型** | 不翻译 Minix3 的 fnsave/fxrstor（32 位遗留）；x86_64: XSAVE lazy init（LazyUserInit 策略）；aarch64: CPACR_EL1.FPEN per-process（fpu_enable_el0 字段）；riscv64: sstatus.FS 状态机（Off/Initial/Clean/Dirty），设 Initial 触发首次 FP 陷阱；FPU 策略是 CpuContext 字段，kernel 层从不读取 | （C 是 `p_seg.fpu_state[FPU_XFP_SIZE]` + fnsave/fxrstor） | arch/{x86_64,arm64,riscv64}/boot.rs: FPU 相关字段 + `inherit_fpu_state` |
| **E.10 ProcKind 枚举设计** | `enum ProcKind { KernelTask, Vm, RootService, UserService, UserProcess }`；驱动 build_cpu_context 选择 PSW/PSR/sstatus + FPU 策略；替代 C 的 `iskerneln`/`isrootsysn`/`proc_nr==VM_PROC_NR` 散落判定 | （C 散落 main.c:173-174 if-else） | arch/boot.rs: `ProcKind` |
| **E.11 EntrySpec 设计** | `struct EntrySpec { pc: Option<VirBytes>, sp: Option<VirBytes>, ps_strings: Option<VirBytes> }`；`KERNEL_TASK` 常量（全 None）；`DEFERRED` 常量（全 None，表示 RS 将运行时加载）；`loaded(pc,sp,ps_str)` 构造已加载入口；明确"未知/延迟/已加载"三态，替代 C 的"零值表示未知"隐式约定 | （C 用 ip->start_addr=0 表示未加载） | arch/boot.rs: `EntrySpec` |
| **E.12 enable_user_io（IOPL 提升）** | x86: 某些驱动需要 IOPL=3 直接 IN/OUT；`enable_user_io(ctx)` 设 PSW.IOPL=3；ARM/RISC-V 无对应概念（用 MMIO 映射代替） | （C 在 arch_boot_proc 内嵌） | arch/{x86_64}/boot.rs: `enable_user_io` |

### 概念组 F：VM ELF 加载与 bootstrap 页表

| 维度 | 应覆盖的知识点 | C 源码锚点 | Rust 锚点 |
|------|--------------|-----------|----------|
| **F.1 VM 的"鸡生蛋"问题** | VM 需要地址空间才能运行；但没有进程能创建地址空间（VM 是第一个）；解决：内核手工创建 VM 的 bootstrap 页表 | （隐含于 arch_boot_proc 的 VM 分支） | arch/boot.rs: `load_vm_elf` doc |
| **F.2 libexec_load_elf 框架** | 输入：`struct exec_info execi`（hdr/hdr_len/stack_high/stack_size/proc_e/filesize/progname/frame_len + 回调）；回调：copymem/clearmem/allocmem_prealloc_junk/allocmem_prealloc_cleared/allocmem_ondemand/clearproc/memmap；流程：elf_unpack → elf_has_interpreter → 遍历 PT_LOAD → 映射+复制+清零 → 分配栈 → 返回 pc/load_base；错误码：ENOEXEC（ELF 无效）、ENOMEM（映射失败） | lib/libexec/exec_elf.c:127-318 | arch/boot.rs: `load_vm_elf`（用 minix_elf crate 替代 libexec） |
| **F.3 libexec_pg_alloc 回调** | `pg_map(PG_ALLOCATEME, vaddr, vaddr+len, &kinfo)` 让分配器选物理帧；`pg_load()` 加载页表；`memset(vaddr, 0, len)` 清零；`alloc_for_vm += len` 累计 VM 占用 | protect.c:379-386(x86) | arch/boot.rs: `paging.map(VirBytes, PhysBytes, flags)` + identity mapping |
| **F.4 bootstrap 页表三部分映射** | 恒等映射：物理地址==虚拟地址（boot 期需要）；内核高半区：内核代码在 0xffff_8000_0000_0000 以上；VM 用户态：VM ELF 段 + 栈 | （隐含于 protect.c 的 pg_map 调用） | arch/boot.rs: `load_vm_elf` 注释 + kernel_info.kern_virt_base |
| **F.5 Rust load_vm_elf free function 设计** | `fn load_vm_elf<P: Paging>(module, kernel_info, paging) -> Result<VmLoadResult, VmLoadError>`；free function 而非 trait 方法（三架构实现字节相同，false polymorphism）；用 `minix_elf` crate 解析 ELF；逐页映射 PT_LOAD 段（identity mapping，paddr=vaddr）；复制段字节到映射页；分配用户栈（VM_STACK_SIZE=64KB）；返回 `VmLoadResult { pc, sp, ps_strings, allocated_bytes }`；错误处理：`VmLoadError { InvalidElf, MappingFailed }`（Result，无静默失败） | lib/libexec/exec_elf.c:127 | arch/boot.rs:215-300 |
| **F.6 Rust VmLoadResult 与 EntrySpec 衔接** | `VmLoadResult { pc, sp, ps_strings, allocated_bytes }`；`EntrySpec::loaded(pc, sp, ps_strings)` 构造已加载入口；`EntrySpec::KERNEL_TASK`/`EntrySpec::DEFERRED` 常量；build_cpu_context 接受 EntrySpec 决定是否填 pc/sp/ps_strings | （C 直接写 p_reg.pc/sp/bx） | arch/boot.rs: `VmLoadResult` + `EntrySpec` |
| **F.7 ELF 段标志到页标志映射** | `PF_R\|PF_W\|PF_X` → `PageFlags::PRESENT\|USER_ACCESSIBLE\|WRITABLE\|EXECUTABLE`；`elf_flags_to_page_flags(elf_flags)` 函数 | （C 在 libexec 内嵌） | arch/boot.rs: `elf_flags_to_page_flags` |
| **F.8 VM 占用字节统计** | `kinfo.vm_allocated_bytes` 记录 VM ELF + 栈占用；用于后续内存管理决策 | protect.c:453 | arch/boot.rs: `VmLoadResult.allocated_bytes` |
| **F.9 DEFERRED 路径（当前 Rust 实现）** | 真实 VM ELF 加载需 bootstrap 页表（current_page_table 经 kmain 传入）；当前 Rust 实现标 DEFERRED，VM 启动时 PC=0，RS 在用户态启动期加载真实 VM ELF；这是诚实的"未实现"标记，非静默失败 | （C 无 DEFERRED 概念，boot 期必加载） | lib.rs: `init_proc_and_boot` Step 3b `#[cfg(not(feature="mock"))]` 分支 |
| **F.10 exec_info 结构体** | `struct exec_info { vir_bytes stack_high; size_t stack_size; endpoint_t proc_e; char *hdr; size_t hdr_len; size_t filesize; char progname[PROCNAME_LEN]; size_t frame_len; vir_bytes pc; vir_bytes load_offset; exec_loadfunc_t copymem/clearmem/allocmem_prealloc_junk/allocmem_prealloc_cleared/allocmem_ondemand; clearproc_func_t clearproc; }`；libexec 框架的通用加载描述符；boot 期填充：stack_high=kinfo.user_sp、stack_size=64KB、hdr=mod->mod_start（物理地址直接用）、filesize=mod_end-mod_start、frame_len=0（无 argv/envp） | lib/libexec/exec_elf.c:libexec_load_elf 内；minix/libexec.h:struct exec_info | （Rust 用 `BootModule` + `KernelInfo` 直接传参，无 exec_info 中间结构） |
| **F.11 libexec 回调机制（6 回调）** | `copymem=libexec_copy_memcpy`（复制段字节到映射页）；`clearmem=libexec_clear_memset`（清零 BSS 区）；`allocmem_prealloc_junk=libexec_pg_alloc`（预分配页，不清零）；`allocmem_prealloc_cleared=libexec_pg_alloc`（预分配页，清零）；`allocmem_ondemand=libexec_pg_alloc`（按需分配）；`clearproc=NULL`（进程级清理，boot 期不需要）；libexec_load_elf 内部按 ELF 段类型调用不同回调 | protect.c:411-417(x86); lib/libexec/exec_elf.c | （Rust load_vm_elf 内联所有逻辑，无回调抽象） |
| **F.12 alloc_for_vm 累加器** | `static int alloc_for_vm = 0;` 文件级静态变量；libexec_pg_alloc 每次分配 `alloc_for_vm += len`；arch_boot_proc 结束时 `kinfo.vm_allocated_bytes = alloc_for_vm`；记录 VM ELF + 栈总占用，供后续内存管理 | protect.c:368, 384, 454(x86) | arch/boot.rs: `VmLoadResult.allocated_bytes`（函数返回值，非全局变量） |
| **F.13 bootmod() 函数** | `multiboot_module_t *bootmod(int pnr)`：按进程号查 boot module；内部遍历 `kinfo.module_list`，匹配 `proc_nr`；返回 module 指针（含 mod_start/mod_end/mod_name）；arch_boot_proc 用 `mod = bootmod(rp->p_nr)` 取 VM 的 ELF 镜像 | protect.c:273-294(x86), protect.c:52-72(ARM) | （Rust 用 `kernel_info.boot_modules[i]` 索引，无 bootmod 函数） |
| **F.14 mod 清零（释放 boot module）** | VM ELF 加载完成后：`mod->mod_end = mod->mod_start = 0;`——清零 module 的 start/end，标记此 module 已被消费；防止重复加载；`add_memmap(&kinfo, mod->mod_start, ...)` 在清零前记录内存区域 | protect.c:448-450(x86) | （Rust 无显式清零，boot_modules 生命周期由 KernelInfo 管理） |

### 概念组 G：SMP 预留（阶段 F，本阶段仅预留字段）

| 维度 | 应覆盖的知识点 | C 源码锚点 | Rust 锚点 |
|------|--------------|-----------|----------|
| **G.1 per-CPU 数据** | C: `get_cpu_var(var)`/`get_cpulocal_var(var)` 宏；`CONFIG_MAX_CPUS` 最大 CPU 数；per-CPU：`proc_ptr`/`bill_ptr`/`run_q_head[]`/`run_q_tail[]`/`idle_proc` | glo.h, proc.h | （阶段 F 实现 SmpState） |
| **G.2 CPU 亲和性** | `p_cpu_mask[BITMAP_CHUNKS(CONFIG_MAX_CPUS)]` 位图；位 i 设 ⇔ 可在 CPU i 运行；默认全 1（任意 CPU） | proc.h:37 | proc.rs: `CpuMask { bits: u64 }` + `SchedFields.cpu_mask` |
| **G.3 BKL 串行化** | Big Kernel Lock 自旋锁；所有内核代码持 BKL 执行；per-CPU 数据在 BKL 下无需原子操作 | （隐含于 kernel 全局锁） | （阶段 F 实现 BKL） |
| **G.4 Rust SMP 设计** | `CpuMask { bits: u64 }` 32 位够用单 u64 存储；`SchedFields { priority, quantum, cpu, cpu_mask, scheduler }`；`cpu_mask: CpuMask` CPU 亲和性；`scheduler: Option<ProcNr>` 用户态调度器；不用 Rc/RefCell（SMP 跨 CPU 不安全）；用 AtomicI32/AtomicU32 + BKL 保护 | proc.h:37 | proc.rs:398-489 |
| **G.5 Rust CpuLocal（阶段 F）** | 当前阶段 C 不涉及 SMP 调度；字段已预留（cpu_mask 等）；实际 per-CPU 数据在阶段 F 的 SmpState | （C 用 get_cpu_var 宏） | （阶段 F） |

### 概念组 H：fork 路径的进程初始化（运行时路径，非 boot 路径）

| 维度 | 应覆盖的知识点 | C 源码锚点 | Rust 锚点 |
|------|--------------|-----------|----------|
| **H.1 fork 是运行时路径** | boot 路径：init_proc_and_boot() 初始化所有 boot 进程；fork 路径：运行时创建子进程；06 文档主要讲 boot 路径，fork 仅涉及 FPU 继承 | （C fork 在 PM/内核分散） | proc.rs: `fork_from` |
| **H.2 FPU 状态继承** | `KProcess::fork_from(parent, child_nr, child_endpoint) -> Self`；继承：priority/quantum/cpu/cpu_mask/ipc 端点；不继承：accounting（重置）/cpuavg（重置）/队列指针（None）；FPU 继承：`<CurrentCpuContextArch as CpuContextArch>::inherit_fpu_state(child, parent)`；x86_64: 复制 fpu_policy；aarch64: 复制 fpu_enable_el0；riscv64: 复制 sstatus（含 FS 字段） | （C: `memcpy(rpc->p_seg.fpu_state, rpp->p_seg.fpu_state, FPU_XFP_SIZE)` under `proc_used_fpu(rpp)`） | proc.rs:1376-1460 |
| **H.3 CPU 亲和性继承** | `cpu_mask: parent.p_sched.cpu_mask` 直接复制；子进程继承父进程的 CPU 亲和性 | （C: p_cpu_mask memcpy） | proc.rs: `fork_from` |
| **H.4 RTS 标志继承策略** | 子进程不继承父的 SENDING/RECEIVING/PREEMPTED 等运行时状态；子进程初始带 PROC_STOP（等 PM 唤醒）；子进程清 SLOT_FREE | （C: fork 内逐位清） | proc.rs: `fork_from` 的 RTS 修正逻辑 |

### 概念组 I：零堆启动约束

| 维度 | 应覆盖的知识点 | C 源码锚点 | Rust 锚点 |
|------|--------------|-----------|----------|
| **I.1 为什么零堆** | `#![no_std]` 不链接 std；boot 期无堆分配器（allocator 在 VM 启动后才可用）；所有数据结构必须编译期或 BSS 静态分配 | （C 全局数组天然零堆） | （Rust 约束） |
| **I.2 Rust 零堆实现** | `ProcessTable { procs: [KProcess; PROC_TABLE_SIZE] }` 固定数组；`PrivTable { privs: [KPriv; NR_SYS_PROCS] }` 固定数组；`const fn new()` 编译期构造；`static mut PROC_TABLE`/`static mut PRIV_TABLE` BSS 全局；不用 `Box<[KProcess]>`（运行时堆分配） | （C 全局数组） | proc_table.rs + kpriv.rs |
| **I.3 const fn 初始化约束** | `const fn` 不能用 for 循环（用 while）；不能调用非 const fn；不能用 String/Vec；KProcess/KPriv 所有字段必须 const-constructible；这是为什么用 AtomicI32 而非 Mutex | （C 无此约束） | proc_table.rs: `ProcessTable::new` while 循环 |
| **I.4 kernel_may_alloc 标志（boot 期内存分配窗口）** | `EXTERN int kernel_may_alloc;`（glo.h:76）；boot 期 `kernel_may_alloc = 1`（pre_init.c 初始化）——内核可临时分配物理内存（VM 还没启动，无用户态内存管理器）；`pg_utils.c` 的 `alloc_mem`/`alloc_pte` 等函数 `assert(kernel_may_alloc)` 校验；bsp_finish_booting 设 `kernel_may_alloc = 0`（VM 即将接管，内核不再直接分配）；**boot 期内存分配窗口**：proc_init → boot 循环 → arch_post_init → memory_init 期间允许，bsp_finish_booting 后关闭 | glo.h:76; pre_init.c:33; main.c:105,142; pg_utils.c:42,74,102,143,274 | lib.rs: `static KERNEL_MAY_ALLOC: AtomicBool` + `kernel_may_alloc()` 函数 + Step 8 设 false |

### 概念组 J：测试覆盖

| 维度 | 应覆盖的知识点 | C 源码锚点 | Rust 锚点 |
|------|--------------|-----------|----------|
| **J.1 arch 层测试** | x86_64: PSW 初值/段选择子/apply_to_trap_frame/inherit_fpu_state；aarch64: PSR 初值/fpu_enable_el0/inherit_fpu_state；riscv64: sstatus 初值/SPP/SPIE 位/inherit_fpu_state | （C 无单元测试） | arch/{x86_64,arm64,riscv64}/boot.rs `#[cfg(test)]` |
| **J.2 kernel 层测试** | ProcessTable: const fn 可编译/per-slot p_nr/p_endpoint/IDLE slot 特殊处理；PrivTable: const fn 可编译/grant_capability 各模板/重复分配失败；KProcess: fork_from 继承/accounting 重置/队列指针独立；RTS 标志: rts_set/rts_unset 调度队列维护 | （C 无单元测试） | proc.rs/proc_table.rs/kpriv.rs `#[cfg(test)]` |
| **J.3 集成测试** | init_proc_and_boot() 主流程；VM ELF 加载（mock paging）；boot_modules 数量校验 | （C 无集成测试） | lib.rs 测试 + arch/boot.rs `load_vm_elf_invalid_elf_returns_err` |
| **J.4 L3 grep 证据** | 旧 API（assign_static + configure_boot_priv）0 残留；initial_pc/initial_sp/initial_ps_strings_reg/initial_status 旧字段 0 残留 | （C 无对应） | （grep 验证） |

### 概念组 K：跨架构统一抽象

| 维度 | 应覆盖的知识点 | C 源码锚点 | Rust 锚点 |
|------|--------------|-----------|----------|
| **K.1 三架构差异点** | x86: 段选择子 + FPU 状态区 + IOPL；ARM: 无段 + CPACR_EL1.FPEN per-process；RISC-V: 无段 + sstatus.FS 状态机 | arch_system.c/protect.c 各架构版本 | arch/{x86_64,arm64,riscv64}/boot.rs |
| **K.2 统一抽象原则** | 上层（kernel）只通过 trait 方法操作，不读 arch 私有字段；arch 层实现 trait，封装差异；不用 `#[cfg(target_arch)]` 在上层做行为选择 | （C 用 #ifdef ARCH_X86 等散落） | arch/boot.rs: `CpuContextArch` trait + `CurrentCpuContextArch` type alias |
| **K.3 CurrentCpuContextArch 类型别名** | `type CurrentCpuContextArch = X86_64CpuContextArch`（或 ARM/RISC-V）；编译期单架构选择；上层用 `<CurrentCpuContextArch as CpuContextArch>::build_cpu_context(...)` 调用 | （C 用 #ifdef 编译期选择） | arch/boot.rs: `CurrentCpuContextArch` |
| **K.4 CurrentCpuContext 关联类型** | `type CpuContext = <CurrentCpuContextArch as CpuContextArch>::CpuContext`；KProcess.cpu_context 字段类型；arch 私有，kernel 层只移动不读 | （C struct proc 内嵌 arch 私有字段） | proc.rs: `cpu_context: CurrentCpuContext` |

### 概念组 L：错误处理与不变量

| 维度 | 应覆盖的知识点 | C 源码锚点 | Rust 锚点 |
|------|--------------|-----------|----------|
| **L.1 errno → Result 转换** | C: get_priv 返回 EINVAL/ENOSPC/EBUSY；Rust: `Result<PrivId, CapabilityError>`；C: libexec_load_elf 返回 ENOEXEC/ENOMEM；Rust: `Result<VmLoadResult, VmLoadError>` | system.c:274-301; exec_elf.c | kpriv.rs: `CapabilityError` + arch/boot.rs: `VmLoadError` |
| **L.2 不变量表达** | C: `p_rts_flags == RTS_SLOT_FREE` 表示空槽（注释约定）；Rust: `is_empty()` 方法 + debug_assert；C: `p_vmrequest` 始终嵌入 + RTS_VMREQUEST 控制有效性；Rust: `p_vm_suspend: Option<VmSuspendContext>` + 不变量注释 `RTS_VMREQUEST <==> p_vm_suspend.is_some()` | proc.h:141, proc.h:274 | proc.rs:892-893 不变量注释 |
| **L.3 panic vs Result 策略** | boot 期错误（如 grant_capability 槽占用）panic：boot 不可恢复；运行时错误（如 fork 失败）返回 Result；VM ELF 加载 DEFERRED 不 panic，诚实标记未实现 | （C boot 期 panic/minix_panic） | lib.rs: `expect("grant_capability: ...")` + arch/boot.rs: `VmLoadError` |

### 概念组 M：调度字段与统计初始化

| 维度 | 应覆盖的知识点 | C 源码锚点 | Rust 锚点 |
|------|--------------|-----------|----------|
| **M.1 调度优先级常量** | `SRV_Q`（server 优先级队列）、`SRV_QT`（server quantum ms）；`TSK_Q`/`TSK_QT`（task，实际 boot 循环未设）；`USER_Q`（用户默认）；`MIN_USER_Q`/`MAX_USER_Q` 范围；`NONE_Q` | kernel/sched.h（常量定义） | proc.rs: `priority::USER_Q` 模块常量 |
| **M.2 p_priority / p_quantum_size_ms 初始化** | proc_init 设 `p_priority=0`、`p_quantum_size_ms=0`（所有 slot 默认）；boot 循环中 schedulable 进程覆写：VM→`SRV_Q`/`SRV_QT`、RS→`SRV_Q`/`SRV_QT`；内核 task 不覆写（保持 0）；非 schedulable 用户进程保持 0（等 RS 运行时设） | proc.c:132-133; main.c:209-210, 232-233 | lib.rs: `init_proc_and_boot` Step 3a/3b（当前 Rust 未显式设 priority/quantum，由 SchedFields::new() 默认） |
| **M.3 p_cpu_time_left** | boot 循环中每个进程设 `p_cpu_time_left = 0`（剩余时间片清零）；调度器用此字段跟踪当前 quantum 剩余 | main.c:171 | （Rust 用 Quantum 类型封装，SchedFields.quantum） |
| **M.4 p_scheduler 指针** | proc_init 设 `p_scheduler = NULL`（内核默认调度）；`proc_kernel_scheduler(p) = (p->p_scheduler == NULL \|\| p->p_scheduler == p)`；用户态调度器运行时由 RS 通过 sys_sched_setscheduler 设置 | proc.c:131; proc.h:183-184 | proc.rs: `SchedFields.scheduler: Option<ProcNr>`（None=内核默认） |
| **M.5 Rust SchedFields 结构体** | `pub struct SchedFields { priority: AtomicI8, quantum: Quantum, cpu: AtomicU32, cpu_mask: CpuMask, scheduler: Option<ProcNr> }`；`const fn new()` 默认 `priority=USER_Q`、`quantum=200ms`、`cpu=0`、`cpu_mask=all`、`scheduler=None`；AtomicXxx 用于 SMP 跨 CPU 访问（BKL 下安全） | proc.h:37-42（C 散落 proc 结构体内） | proc.rs:445-489 |
| **M.6 reset_proc_accounting()** | boot 循环中每个进程调 `reset_proc_accounting(rp)` 清零统计字段（cycles/time/queue stats）；确保 boot 进程从干净状态开始 | proc.c:1912; main.c:176 | proc.rs: `Accounting::reset()` / `Accounting::new()` const fn |
| **M.7 Rust 统计结构体** | `Accounting { enter_queue, time_in_queue, dequeues, ipc_sync, ipc_async, preempted }`（调度统计）；`TimeStats`（时间统计）；`CyclesStats`（周期统计）；`CpuAvg`（CPU 平均）；`CpuCycles` newtype；每结构 `const fn new()` 可编译期构造；`Accounting::bill_to_idle()` 用于 bsp_finish_booting 设 IDLE 计费 | proc.h（C 散落 proc 结构体内） | proc.rs:490-680 |

### 概念组 N：boot→running 转换（bsp_finish_booting）

> **关键**：这是 boot 流程的最后一步——将所有 boot 进程从 PROC_STOP 唤醒为可运行。06 文档当前完全缺失这一环节。

| 维度 | 应覆盖的知识点 | C 源码锚点 | Rust 锚点 |
|------|--------------|-----------|----------|
| **N.1 bsp_finish_booting() 角色** | boot 流程最后一步：proc_init → boot 循环（设特权/上下文/RTS_PROC_STOP）→ arch_post_init → memory_init → system_init → **bsp_finish_booting**（唤醒 boot 进程 → switch_to_user）；之前所有进程带 RTS_PROC_STOP，此处清除让它们可运行 | main.c:38-87, main.c:305-318 | lib.rs: `finish_booting` / `bsp_finish_booting` 等价 |
| **N.2 RTS_PROC_STOP 清除（唤醒 boot 进程）** | `for(i=0; i < NR_BOOT_PROCS - NR_TASKS; i++) RTS_UNSET(proc_addr(i), RTS_PROC_STOP);`——遍历用户态 boot 进程（不含内核 task），清除 PROC_STOP；RTS_UNSET 自动 enqueue 让它们进入就绪队列；**这是 boot 进程真正开始运行的瞬间** | main.c:56-58 | lib.rs: finish_booting 步骤（清除 PROC_STOP + enqueue） |
| **N.3 proc_ptr / bill_ptr 初始化** | `get_cpulocal_var(proc_ptr) = get_cpulocal_var_ptr(idle_proc);`（当前运行进程=IDLE）；`get_cpulocal_var(bill_ptr) = get_cpulocal_var_ptr(idle_proc);`（计费目标=IDLE）；per-CPU 指针，调度器用 proc_ptr 找当前进程 | main.c:50-51 | （阶段 F per-CPU，Rust 用 SmpState/CurrentProc 抽象） |
| **N.4 fpu_init()** | 全局 FPU 初始化（设 CR0/CR4 FPU 相关位、设 TS 位）；在 bsp_finish_booting 中调用；与 per-process FPU 策略（E.9）不同：这是 CPU 级 FPU 使能 | main.c:73 | （arch 层 fpu_init，阶段 C 调用） |
| **N.5 kernel_may_alloc = 0** | boot 期 `kernel_may_alloc = 1`（内核可临时分配物理内存，因为 VM 还没启动）；bsp_finish_booting 设 `kernel_may_alloc = 0`（VM 即将接管内存管理，内核不再直接分配）；**boot 期内存分配窗口关闭** | main.c:84; main.c:99; glo.h:39 | lib.rs: `kernel_may_alloc()` AtomicBool + Step 8 设 false |
| **N.6 cycles_accounting_init()** | CPU 周期计数器初始化（读 TSC/相关计数器基线）；调度统计依赖此基线 | main.c:67 | （Rust Accounting 相关初始化） |
| **N.7 boot_cpu_init_timer()** | 启动 boot CPU 的定时器中断（system_hz 频率）；失败则 panic（无时钟源无法调度）；时钟中断驱动调度器抢占 | main.c:69-72 | （05-clock-interrupt-init 文档范围） |
| **N.8 switch_to_user()** | 从内核态切换到用户态运行第一个进程；汇编实现（iret/eret/sret）；详见 10-switch-to-user.md；**boot 流程终点**——此后内核只在中断/syscall 时运行 | main.c:86 | （10-switch-to-user.md 范围） |
| **N.9 Rust boot→running 转换设计** | Rust 将 bsp_finish_booting 拆为：(1) `finish_booting()` 清除 PROC_STOP + enqueue；(2) per-CPU 指针初始化；(3) `kernel_may_alloc = false`；(4) switch_to_user（arch 层）；每步对应 C 的 bsp_finish_booting 行 | main.c:38-87 | lib.rs: `bsp_finish_booting` 等价函数 |

### 概念组 O：boot 期架构后初始化与内存映射

| 维度 | 应覆盖的知识点 | C 源码锚点 | Rust 锚点 |
|------|--------------|-----------|----------|
| **O.1 arch_post_init()** | x86: `get_cpulocal_var(ptproc) = vm;`（当前页表进程=VM）；`pg_info(&vm->p_seg.p_cr3, &vm->p_seg.p_cr3_v);`（获取 VM 页表物理地址）；在 boot 循环后调用，VM slot 已初始化 | protect.c:370-376 | lib.rs: `init_post_and_memory()` |
| **O.2 memory_init()** | x86: 初始化内存映射代码；分配 createpde() 临时映射用的空闲页目录项；在 arch_post_init 后调用 | memory.c:707 | lib.rs: `init_post_and_memory()` 合并 |
| **O.3 system_init()** | 初始化 SYSTEM task 的 IPC 通道；为后续 kernel_call 做准备 | system.c:168 | （Rust system_init 等价） |
| **O.4 add_memmap()** | 记录内存区域到 kinfo 内存映射表；boot 期记录 bootstrap 区和 VM blob；VM 启动后用此表管理物理内存 | pg_utils.c:86; main.c:302; protect.c:449 | （Rust memmap.rs） |
| **O.5 IPCF_POOL_INIT()** | IPC filter 池初始化（清零 `ipc_filter_pool[]`）；在 proc_init 后调用；为系统进程 IPC 过滤做准备 | main.c:158; ipc_filter.h:71 | ipc_filter.rs: `IpcFilterPool::new()` |

---

## 二、纵向链路映射表（Ch1→Ch2→Ch3→Ch4→Ch5）

> 状态：✅=已覆盖且对应；⚠️=部分覆盖或对应弱；❌=缺失；🆕=需新增

| 概念组 | Ch1 概念 | Ch2 C 源码 | Ch3 Rust 设计 | Ch4 实现 | Ch5 测试 | 链路状态 |
|--------|---------|-----------|--------------|---------|---------|---------|
| **A.0 进程概念** | ❌ 缺失（直接讲进程表） | — | — | — | — | **断裂** 🆕 |
| **A.1-A.6 进程表布局/标识符/slot 生命周期** | §1.1 ✅ | §2.1 ✅ | §3.5 零堆（部分）⚠️ | §4.3 ✅ | §5.2 ✅ | **断裂**（Ch3 无 ProcessTable 设计专节） |
| **A.7 proc_init()** | §1.6 ✅ | §2.1 ✅ | §3.5（部分）⚠️ | §4.3 ✅ | §5.2 ✅ | **断裂** |
| **A.8-A.10 ProcessTable/KProcess 类型设计** | — | — | ❌ 缺失 | §4.3/§4.4 ✅ | §5.2 ✅ | **断裂** 🆕 |
| **B.1-B.5 特权表/priv 结构/默认设置** | §1.2 ✅ | §2.4 ✅ | §3.7 KPriv 6 子结构（部分）⚠️ | §4.6 ✅ | §5.2 ✅ | **断裂**（Ch3 无 PrivTable 整体设计） |
| **B.6-B.7 get_priv/fill_sendto_mask** | — | §2.4 ✅ | §3.4（部分）⚠️ | §4.5 ✅ | §5.2 ✅ | **断裂** |
| **B.8-B.11 PrivTable/KPriv/CapabilityTemplate/Newtype** | — | — | §3.4+§3.7 ✅ | §4.5+§4.6 ✅ | §5.2 ✅ | ✅ |
| **B.12 boot 期特权授予流程** | §1.6 ✅ | §2.3 ✅ | §3.4 ✅ | §4.5+§4.7 ✅ | §5.2 ✅ | ✅ |
| **C.1-C.3 RTS 标志/位图理由/宏** | §1.3 ✅ | §2.1（部分）⚠️ | ❌ 缺失 | §4.4（部分）⚠️ | §5.2（部分）⚠️ | **断裂** 🆕 |
| **C.4 阶段 C 涉及的 RTS 位** | §1.3 ✅ | §2.3 ✅ | ❌ 缺失 | §4.7 ✅ | — | **断裂** 🆕 |
| **C.5-C.6 Rust RtsFlags/misc_flags** | — | — | ❌ 缺失 | §4.4（部分）⚠️ | §5.2（部分）⚠️ | **断裂** 🆕 |
| **D.1-D.5 boot image/三类进程/循环逻辑** | §1.4 ✅ | §2.3 ✅ | §3.5（部分）⚠️ | §4.7 ✅ | §5.3 ✅ | **断裂**（Ch3 无 boot image 类型设计专节） |
| **D.6-D.8 Rust boot image 类型/init_proc_and_boot/schedulable** | — | — | ❌ 缺失 | §4.7 ✅ | §5.3 ✅ | **断裂** 🆕 |
| **E.1 CPU 四问** | §1.7 ✅ | §2.2+§2.5+§2.6 ✅ | §3.1+§3.2 ✅ | §4.1 ✅ | §5.1 ✅ | ✅ |
| **E.2-E.4 arch_proc_reset/arch_proc_init/arch_boot_proc** | §1.7 ✅ | §2.2+§2.5+§2.6 ✅ | §3.1 ✅ | §4.1+§4.2 ✅ | §5.1 ✅ | ✅ |
| **E.5 三架构状态寄存器** | §1.7 ✅ | §2.2 ✅ | §3.1 ✅ | §4.1 ✅ | §5.1 ✅ | ✅ |
| **E.6 ps_strings** | §1.7 ✅ | §2.5 ✅ | §3.2 ✅ | §4.2 ✅ | §5.1 ✅ | ✅ |
| **E.7-E.8 CpuContextArch trait + 三架构实现** | — | — | §3.1 ✅ | §4.1 ✅ | §5.1 ✅ | ✅ |
| **E.9 FPU 现代模型** | — | — | §3.3 ✅ | §4.1+§4.8 ✅ | §5.1 ✅ | ✅ |
| **E.10-E.11 ProcKind/EntrySpec** | — | — | §3.2 ✅ | §4.1 ✅ | §5.1 ✅ | ✅ |
| **E.12 enable_user_io** | — | §2.5（部分）⚠️ | ❌ 缺失 | §4.1（部分）⚠️ | §5.1（部分）⚠️ | **断裂** 🆕 |
| **F.1 VM 鸡生蛋** | §1.5 ✅ | §2.5 ✅ | §3.1（部分）⚠️ | §4.2 ✅ | §5.3 ✅ | ✅ |
| **F.2-F.3 libexec_load_elf/pg_alloc 回调** | §1.5 ✅ | §2.5+§2.7+附录A ✅ | §3.1（部分）⚠️ | §4.2 ✅ | §5.3 ✅ | ✅ |
| **F.4 bootstrap 页表三部分** | §1.5 ✅ | §2.5 ✅ | §3.1（部分）⚠️ | §4.2 ✅ | §5.3 ✅ | ✅ |
| **F.5-F.9 load_vm_elf/VmLoadResult/DEFERRED** | — | — | §3.1 ✅ | §4.2 ✅ | §5.3 ✅ | ✅ |
| **G.1-G.5 SMP 预留** | — | — | §3.6 ✅ | — | — | ✅（标注阶段 F） |
| **H.1-H.4 fork 路径** | — | — | §3.3（部分）⚠️ | §4.8 ✅ | §5.2 ✅ | ✅（标注运行时路径） |
| **I.1-I.3 零堆启动** | — | — | §3.5 ✅ | §4.3 ✅ | §5.2 ✅ | ✅ |
| **J.1-J.4 测试覆盖** | — | — | — | — | §5.1-§5.5 ✅ | ✅ |
| **K.1-K.4 跨架构统一抽象** | §1.7 ✅ | §2.2 ✅ | §3.1 ✅ | §4.1 ✅ | §5.1 ✅ | ✅ |
| **L.1-L.3 错误处理/不变量/panic 策略** | — | — | ❌ 缺失 | §4.5（部分）⚠️ | — | **断裂** 🆕 |
| **M.1-M.4 调度优先级/p_priority/p_quantum/p_scheduler** | — | §2.3（部分）⚠️ | ❌ 缺失 | — | — | **断裂** 🆕 |
| **M.5-M.7 Rust SchedFields/Accounting/reset_proc_accounting** | — | — | ❌ 缺失 | §4.4（部分）⚠️ | §5.2（部分）⚠️ | **断裂** 🆕 |
| **N.1-N.9 boot→running 转换（bsp_finish_booting）** | — | ❌ 缺失 | ❌ 缺失 | ❌ 缺失 | — | **断裂** 🆕 |
| **O.1-O.5 arch_post_init/memory_init/add_memmap/IPCF_POOL_INIT** | — | ❌ 缺失 | ❌ 缺失 | ❌ 缺失 | — | **断裂** 🆕 |

---

## 三、对应关系断裂诊断

### 断裂点 1：Ch1.1 讲进程表，Ch3 无 ProcessTable 设计专节
**现状**：Ch1.1 用整节讲"为什么内核需要进程表"，但 Ch3 只在 §3.5"零堆启动"里带过 ProcessTable，没有专节回答：固定数组 vs 动态分配？const fn 怎么写？指针→索引怎么换？边界检查怎么强制？
**应该**：Ch3 新增"§3.X 进程表设计"专节，对应 Ch1.1，覆盖概念组 A.8-A.10。

### 断裂点 2：Ch1.2 讲特权表，Ch3 无 PrivTable 整体设计专节
**现状**：Ch1.2 讲特权表独立性，Ch3 只有 §3.7"KPriv 字段重组"讲子结构，没有 PrivTable 整体设计（const fn / 静态区动态区分 / grant_capability API）。
**应该**：Ch3 新增"§3.X 特权表设计"专节，对应 Ch1.2，覆盖概念组 B.8-B.11。

### 断裂点 3：Ch1.3 讲 RTS 标志，Ch3/Ch4 无 Rust RtsFlags 设计
**现状**：Ch1.3 讲 RTS 位图理由，但 Ch3/Ch4 完全没有 RtsFlags bitflags 设计、rts_set/rts_unset 调度队列维护、misc_flags。
**应该**：Ch3 新增"§3.X 进程状态表达"专节，对应 Ch1.3，覆盖概念组 C.1-C.6。

### 断裂点 4：Ch1.4 讲 boot image，Ch3 无 boot image 类型设计专节
**现状**：Ch1.4 讲 boot image 三类进程，Ch3 无 KERNEL_TASKS/BOOT_MODULE_PROC_NRS 常量数组设计、BootModule 类型衔接。
**应该**：Ch3 新增"§3.X boot image 类型设计"专节，对应 Ch1.4，覆盖概念组 D.6-D.8。

### 断裂点 5：Ch1 缺进程概念抽象（A.0）
**现状**：Ch1.1 直接讲"为什么内核需要进程表"，没有先建立"进程是什么"的概念。
**应该**：Ch1.1 开篇补"进程 = CPU 执行流抽象 + 可保存/恢复状态" preamble，再讲进程表。

### 断裂点 6：enable_user_io（E.12）缺失
**现状**：Ch2.5 提到 arch_boot_proc 但未讲 IOPL 提升；Ch3/Ch4 完全没有 enable_user_io。
**应该**：Ch3 §3.1 CpuContextArch trait 方法列表补 enable_user_io；Ch4.1 补 x86_64 实现。

### 断裂点 7：错误处理与不变量（L.1-L.3）缺失
**现状**：Ch3 无 errno→Result 转换策略、不变量表达、panic vs Result 策略专节。
**应该**：Ch3 新增"§3.X 错误处理与不变量"专节，覆盖概念组 L。

### 断裂点 8：调度字段与统计初始化（M.1-M.7）缺失
**现状**：Ch2.3 提到 boot 循环但未讲 `p_priority`/`p_quantum_size_ms` 覆写（VM/RS→SRV_Q/SRV_QT）；Ch3 完全没有 `SchedFields` 结构体设计、`Accounting` 统计结构体、`reset_proc_accounting()` 调用；Ch4.4 仅部分提及调度字段。
**应该**：Ch2.3 补 `p_priority`/`p_quantum_size_ms` 覆写细节；Ch3 新增"§3.X 调度字段与统计设计"专节，覆盖 M.5-M.7（SchedFields/Accounting/reset_proc_accounting）；Ch4.4 补实现细节。

### 断裂点 9：boot→running 转换（N.1-N.9）完全缺失
**现状**：06 文档当前完全缺失 `bsp_finish_booting()` 这一 boot 流程最后一步——清除 RTS_PROC_STOP 唤醒 boot 进程、初始化 proc_ptr/bill_ptr、fpu_init、设 kernel_may_alloc=0、switch_to_user。Ch1-Ch5 全链路缺失。
**应该**：Ch1 补"boot 流程终点"概念（所有 boot 进程带 PROC_STOP，bsp_finish_booting 唤醒）；Ch2 补 bsp_finish_booting 源码分析；Ch3 补 Rust finish_booting 设计；Ch4 补实现；Ch5 补测试。**这是 06 文档最大的覆盖缺口**。

### 断裂点 10：boot 期架构后初始化与内存映射（O.1-O.5）缺失
**现状**：06 文档未覆盖 boot 循环之后、bsp_finish_booting 之前的 `arch_post_init()`/`memory_init()`/`system_init()`/`add_memmap()`/`IPCF_POOL_INIT()` 等步骤。这些是 boot 流程的中间环节。
**应该**：Ch2 补 arch_post_init/memory_init/system_init 源码分析；Ch3 补 Rust init_post_and_memory 设计；Ch4 补实现。或明确标注"这些步骤属于 06 文档范围边界，详见其他文档"。

---

## 四、开发文档味诊断（模式 56 决策日志体 + 模式 15 开发记录体）

| 位置 | 当前叙事（决策日志体/开发记录体） | 应改为（教学话术） |
|------|---------------------|-------------------|
| §3.1 "朴素 Rust 翻译的问题" | "最初的 Rust 设计用 3 个 trait 镜像这 3 个 C 函数..." | "如果用 3 个 trait 镜像 C 的 reset/init/boot_proc，会暴露三个问题..."（假设性推理，不提"最初设计"） |
| §3.1 "旧设计的问题" | "旧 Rust 设计用 `Box<[KProcess]>`..." | "如果在 boot 期用 `Box<[KProcess]>` 堆分配，会破坏零堆约束..."（讲约束，不提"旧设计"） |
| §3.4 "从 6 个裸参数到 CapabilityTemplate" | "最初的实现需要调用方手动传 6 个参数..." | "如果让调用方手动传 flags/init_flags/trap_mask/ipc_to/k_call_mask/sig_mgr 6 个参数，会有三个问题..."（假设性推理） |
| §3.5 "零堆启动" | "我们最初用 Box，后来改成固定数组..." | "boot 期没有堆分配器，所以 ProcessTable 必须用固定数组 + const fn..."（讲约束驱动） |
| §3.7 "KPriv 字段重组" | "旧版 KPriv 是单一大结构体，访问散落..." | "如果把 priv 的 30+ 字段平铺在一个结构体里，访问语义会混乱..."（讲问题驱动） |
| §4.8 "fork 路径" | "我们后来加了 fork_from..." | "fork 运行时路径需要继承父进程的 FPU 策略和 CPU 亲和性..."（讲需求驱动） |

**通用改写原则**：
1. 删除"最初/旧版/后来/我们改成"等迭代过程词
2. 改为"如果 X 设计，会有 Y 问题，所以用 Z"假设性推理
3. 讲约束（零堆/SMP/BKL/no_std）驱动设计，不讲历史驱动
4. 读者关心"为什么这样设计"，不关心"我们踩过什么坑"

---

## 五、文档规模与拆分评估

### 当前规模
- 06-proc-init-boot-proc.md：1737 行
- 概念组 A-L 共 12 组，约 80 个知识点

### 拆分方案对比

| 方案 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| **A. 不拆分** | 保持单文档，补 Ch3 缺失章节 | 纵向链路完整；读者一处看全；交叉引用少 | 文档更长（预计 ~2200 行） |
| **B. 拆为 06+07** | 06=进程表+特权表+RTS（概念组 A/B/C）；07=boot image+CPU 上下文+VM ELF（概念组 D/E/F） | 每文档 ~1100 行可读 | 纵向链路被拆；boot 流程跨文档；07 编号顺延影响 |
| **C. 拆为 06+07+08** | 06=进程表+特权表；07=boot image+CPU 上下文；08=VM ELF+SMP+fork | 每文档 ~750 行 | 纵向链路碎裂严重；交叉引用爆炸 |

### 推荐：方案 A（不拆分）
**理由**：
1. 06 的核心是"boot 期进程初始化"一条纵向链路，拆分会打断"proc_init → boot 循环 → arch_boot_proc → VM ELF"的因果叙事
2. 概念组 A-F 强耦合（进程表 ↔ 特权表 ↔ RTS ↔ boot image ↔ CPU 上下文 ↔ VM ELF），拆分后交叉引用成本高于阅读成本
3. 2200 行虽长但结构清晰（Ch1 概念 → Ch2 源码 → Ch3 设计 → Ch4 实现 → Ch5 测试），读者可按需跳读
4. 拆分会导致 07-12 全部顺延，影响外部引用稳定性

---

## 六、重组方案（基于方案 A）

### Ch1 修复：补进程概念 preamble
- §1.1 开篇补 A.0："进程 = CPU 执行流抽象 + 可保存/恢复状态；单 CPU 单执行流 vs 多进程抽象的矛盾；进程表是矛盾的桥梁"
- 其余 §1.2-§1.8 保持

### Ch3 修复：补对应章节 + 去开发文档味
**新增章节**（修复对应关系断裂）：
- **§3.X 进程表设计**（对应 Ch1.1，覆盖 A.8-A.10）：固定数组 vs 动态分配；const fn 初始化；指针→索引替代；边界检查强制；SMP 安全（BKL）
- **§3.X 特权表设计**（对应 Ch1.2，覆盖 B.8-B.11）：PrivTable 固定数组；静态区/动态区分区；grant_capability API；CapabilityTemplate 5 变体；Newtype 类型安全
- **§3.X 进程状态表达**（对应 Ch1.3，覆盖 C.1-C.8）：RtsFlags bitflags 设计；rts_set/rts_unset 调度队列维护；misc_flags 全集（MF_* 18 位）与 RTS 区别；P_BLOCKEDON/proc_kernel_scheduler 宏；"非零=不可运行"不变量
- **§3.X boot image 类型设计**（对应 Ch1.4，覆盖 D.6-D.8）：KERNEL_TASKS/BOOT_MODULE_PROC_NRS 常量数组；BootModule 类型衔接；NR_BOOT_PROCS vs NR_BOOT_MODULES；schedulable 判定
- **§3.X 调度字段与统计设计**（覆盖 M.1-M.7）：SchedFields 结构体（AtomicI8 priority/Quantum/AtomicU32 cpu/CpuMask/Option<ProcNr> scheduler）；Accounting/TimeStats/CyclesStats 统计结构体；reset_proc_accounting 语义；p_priority/p_quantum 覆写策略（VM/RS→SRV_Q/SRV_QT，其他保持默认）
- **§3.X boot→running 转换设计**（覆盖 N.1-N.9）：finish_booting 清除 RTS_PROC_STOP + enqueue；proc_ptr/bill_ptr 初始化；fpu_init CPU 级 FPU 使能；kernel_may_alloc=0 关闭 boot 分配窗口；switch_to_user（引用 10-switch-to-user.md）
- **§3.X 错误处理与不变量**（覆盖 L.1-L.3）：errno→Result 转换；不变量表达（Option 替代 always-embedded + flag）；panic vs Result 策略（boot 期 panic，运行时 Result）

**改写现有章节**（去开发文档味）：
- §3.1：删"最初的 Rust 设计"，改"如果用 3 个 trait..."
- §3.4：删"最初的实现需要调用方手动传 6 个参数"，改"如果让调用方手动传 6 个参数..."
- §3.5：删"我们最初用 Box"，改"boot 期没有堆分配器，所以..."
- §3.7：删"旧版 KPriv 是单一大结构体"，改"如果把 priv 的 30+ 字段平铺..."
- §3.1 补 enable_user_io（E.12）

### Ch2 修复：补源码分析缺口
- §2.3 补 `p_priority`/`p_quantum_size_ms` 覆写（VM/RS→SRV_Q/SRV_QT）+ `reset_proc_accounting` 调用
- §2.X 补 bsp_finish_booting 源码分析（RTS_PROC_STOP 清除循环、proc_ptr/bill_ptr、fpu_init、kernel_may_alloc=0、switch_to_user）
- §2.X 补 arch_post_init/memory_init/system_init/IPCF_POOL_INIT 源码分析（或标注范围边界）

### Ch4 修复：明确 HOW 边界
- §4.1 补 enable_user_io x86_64 实现
- §4.4 补 RtsFlags/misc_flags 实现细节 + SchedFields/Accounting 实现细节
- §4.7 补 schedulable 判定实现
- §4.X 补 finish_booting 实现细节（RTS_UNSET 循环、kernel_may_alloc 设 false）
- 删除"我们后来加了"等开发记录词

### Ch5 修复：补测试
- §5.1 补 enable_user_io 测试
- §5.2 补 RtsFlags rts_set/rts_unset 调度队列维护测试
- §5.2 补 grant_capability 重复分配失败测试
- §5.X 补 SchedFields/Accounting 初始化测试
- §5.X 补 finish_booting 唤醒 boot 进程测试（RTS_PROC_STOP 清除 + enqueue）

### 纵向链路验证
重组后重新填表二，确认所有"断裂"行变为 ✅。

---

## 七、知识点覆盖完整性自检

### 对照 minix3 源码文件
- [x] proc.c: proc_init / IDLE 初始化 / RTS 宏 / reset_proc_accounting → A.7, C.3, M.6
- [x] main.c: boot 循环 / schedulable 判定 / 特权授予 / VMINHIBIT / bsp_finish_booting / kernel_may_alloc → D.5, D.8, B.12, C.4, N.1-N.8, I.4
- [x] system.c: get_priv / set_sendto_bit / fill_sendto_mask / system_init → B.6, B.7, O.3
- [x] protect.c (x86/ARM): arch_boot_proc / VM ELF / ps_strings / bootstrap 页表 / arch_post_init / bootmod / alloc_for_vm → E.4, F.1-F.14, O.1
- [x] arch_system.c: arch_proc_reset / arch_proc_init → E.2, E.3
- [x] exec_elf.c: libexec_load_elf 框架 / exec_info 结构 → F.2, F.10, F.11
- [x] priv.h: priv 结构体 / 布局宏 / 映射宏 → B.2, B.3, B.4
- [x] proc.h: proc 结构体 / RTS 位 / 判定宏 / misc_flags MF_* / P_BLOCKEDON / proc_kernel_scheduler → A.1-A.6, C.1-C.8
- [x] const.h: PMAGIC / INIT_PSW → A.3, E.5
- [x] com.h: 端点常量 / NR_BOOT_MODULES → A.4, A.5, D.4
- [x] param.h: NR_BOOT_PROCS → D.4
- [x] minix/priv.h: 默认特权设置 / static_priv_id → B.5, B.4
- [x] sched.h: SRV_Q / SRV_QT / TSK_Q / USER_Q / MIN_USER_Q / MAX_USER_Q / NONE_Q → M.1
- [x] glo.h: kernel_may_alloc 声明 → I.4, N.5
- [x] pre_init.c: kernel_may_alloc = 1 初始化 → I.4
- [x] pg_utils.c: alloc_mem / assert(kernel_may_alloc) / add_memmap → I.4, O.4
- [x] proto.h: arch_post_init 声明 → O.1
- [x] memory.c: memory_init / arch_proc_init → O.2, E.3
- [x] ipc_filter.h: IPCF_POOL_INIT → O.5

### 对照 Rust 实现文件
- [x] os/kernel/src/proc.rs: KProcess / RtsFlags / CpuMask / SchedFields / Accounting / fork_from / MiscFlags → A.9, A.10, C.5, C.6, G.4, H.2-H.4, M.5, M.7
- [x] os/kernel/src/proc_table.rs: ProcessTable / const fn / get/get_mut / rts_set/rts_unset → A.8, I.2, I.3, C.5
- [x] os/kernel/src/kpriv.rs: PrivTable / KPriv 6 子结构 / grant_capability → B.8, B.9, B.10
- [x] os/kernel/src/capability.rs: CapabilityTemplate / Newtype / masks → B.10, B.11
- [x] os/kernel/src/lib.rs: init_proc_and_boot 主流程 / kernel_may_alloc / finish_booting → D.7, B.12, C.4, I.4, N.9
- [x] os/arch/src/arch/boot.rs: CpuContextArch trait / load_vm_elf / EntrySpec / ProcKind → E.7, E.10, F.5-F.9