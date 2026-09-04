# 19-syscall-signal: 信号系统调用

> **分类**: 系统调用服务
> **C 源码**: `minix3/minix/kernel/system/do_kill.c`, `do_getksig.c`, `do_endksig.c`, `do_sigsend.c`, `do_sigreturn.c`; `minix3/minix/kernel/system.c` (`cause_sig` / `sig_delay_done`)
> **Rust 实现**: `os/kernel/src/syscall_signal.rs` (1035 行), `os/kernel/src/proc.rs` (`SigSet` / `p_pending`), `os/kernel/src/proc_table.rs` (`sig_mgr`), `os/kernel/src/kpriv.rs` (`PrivSignals`)
> **覆盖**: SYS_KILL / SYS_GETKSIG / SYS_ENDKSIG / SYS_SIGSEND / SYS_SIGRETURN 五个系统调用，以及内核内部函数 `cause_sig`
> **前置**: [11-scheduling-primitives.md](11-scheduling-primitives.md) (RTS_SIGNALED / RTS_SIG_PENDING), [14-exception-interrupt.md](14-exception-interrupt.md) (CPU 异常 → cause_sig), [16-smp.md](16-smp.md) (BKL), [22-privilege.md](22-privilege.md) (s_sig_mgr)

> **范围说明**: 本快照的 minix3 源码不含 `do_sigctl.c` / `do_ksig.c` / `do_sighold.c`（SYS_SIGCTL / SYS_SIGHOLD 不在此版本），Rust 实现亦未覆盖。本文聚焦五个已实现的信号系统调用。信号常量均已定位：`SIGKSIG=74`（signal.h:274）、`SIGKSIGSM=73`（signal.h:273）、`SIGSNDELAY=70`（signal.h:264）、`SIGS_IS_LETHAL`（signal.h:280-282）、`SC_MAGIC=0xc0ffee1`（i386 signal.h:115，arch 相关）。`SIGS_IS_LETHAL` 的 backup 切换 / panic 子路径已于 2026-09-05 在 Rust 侧实现（§4.3/§4.7）；`sig_delay_done`（SIGSNDELAY）与 DIAGCTL SIGKMESS 通知仍 DEFERRED。

---

## 1. 概念建构

**核心问题**: 微内核中，信号处理器运行在用户态（PM 或自定义信号管理器），而内核才是唯一能修改进程寄存器、设置挂起位图、向用户栈写入信号帧的执行体。内核如何作为信号中介，协调"信号产生方"与"用户态信号管理器"，而不破坏特权级隔离？

**Minix3 的回答**: **双路径分层 + 推拉结合**。内核只承担"挂起 + 通知 + 改寄存器"三项特权操作，把"何时处理、用什么处理器"的决策权交给用户态信号管理器。两条路径共享 `p_pending` 位图与 `RTS_SIGNALED` / `RTS_SIG_PENDING` 状态：

- **内核信号路径**（KILL → GETKSIG → ENDKSIG）：内核产生信号 → 设置 `p_pending` + `RTS_SIGNALED` → 通知信号管理器（"推"）→ 管理器主动轮询拉取信号位图（"拉"）→ 处理完毕确认。三步闭环，内核从不主动调用用户态处理器。
- **POSIX 信号路径**（SIGSEND → SIGRETURN）：管理器决定安装用户态处理器 → 通过 VM proxy 在用户栈构建 sigframe → 内核修改 PC/SP 让进程下次进入用户态时从处理器入口开始执行 → 处理器返回时内核从用户栈恢复原寄存器。

> **架构范围**: 三架构共性概念（信号位图、推拉模型、双路径）在下文主体展开；sigcontext / sigframe 的字段布局是架构相关细节（x86_64 段寄存器 / aarch64 通用寄存器组），标注为 `(架构相关)` 并在 §2.2 / §4.6 集中说明。

### 1.1 内核信号路径：cause_sig → GETKSIG 轮询 → ENDKSIG 完成

**灵魂本质**: 内核信号路径是三步闭环——产生信号 → 信号管理器拉取 → 信号管理器确认完成；内核不主动推送信号给处理器。

**WHY → WHAT → HOW 弧线**:

- **WHY**: 微内核中信号处理器在用户态（PM 或自定义 SM），内核既不知道 SM 的 IPC 阻塞状态，也不能直接调用用户态函数。需要一种解耦机制让内核"挂起信号"而 SM"按需拉取"——否则内核必须同步等待 SM 响应，破坏内核非阻塞原则。
- **WHAT**: 三步闭环：(1) 内核通过 `cause_sig()` 设置 `p_pending` 位图 + `RTS_SIGNALED` 标志，并通知 SM；(2) SM 通过 SYS_GETKSIG 主动轮询，线性扫描所有 `RTS_SIGNALED` 进程，拉取信号位图，将进程从"待处理"转为"SM 处理中"（`RTS_SIG_PENDING` 阻塞调度）；(3) SM 处理完毕通过 SYS_ENDKSIG 确认，如无新信号则清除 `RTS_SIG_PENDING`，进程可继续调度。
- **HOW**: C 实现 `cause_sig()` (system.c:389-449) + `do_getksig()` (do_getksig.c:18-42) + `do_endksig()` (do_endksig.c:15-38)。

**关键语义**:

| 状态 | 含义 | 调度影响 |
|------|------|---------|
| `RTS_SIGNALED` | 进程有未拉取的内核信号（待 SM 取走） | SM 尚未介入 |
| `RTS_SIG_PENDING` | SM 正在处理信号 | 进程不可调度（阻塞在调度器） |
| `p_pending` | 挂起信号位图（`SigSet` = 64 位，对应 `_NSIG = 64`） | GETKSIG 拉取后清空 |

**推拉结合**: 内核"推"通知（向 SM 的 `s_sig_pending` 加入 `SIGKSIG`），SM "拉"信号（GETKSIG 扫描 `RTS_SIGNALED` 进程）。通知只触发一次（去重），拉取可重复直到无 `RTS_SIGNALED` 进程。

**SELF 路径**: 当进程是自己的信号管理器（`s_sig_mgr == SELF`），不走外部 IPC 通知，直接写入自身的 `s_sig_pending` 并发自通知信号 `SIGKSIGSM`，避免循环通知。

### 1.2 POSIX 信号路径：SIGSEND 安装处理器 → SIGRETURN 恢复

**灵魂本质**: POSIX 路径是信号管理器通过 VM proxy 直接操作进程栈帧——在用户栈上构建 sigframe 并修改 PC/SP，让进程从处理器入口开始执行。

**WHY → WHAT → HOW 弧线**:

- **WHY**: POSIX 信号处理器是用户态函数，必须在用户栈上运行；内核不能直接调用用户态函数（特权级隔离）。需要一种机制让进程"下次进入用户态时从信号处理器开始执行"，且处理器返回后能恢复原执行点。
- **WHAT**: SM 通过 SYS_SIGSEND 在用户栈上构建 sigframe（含保存的寄存器 sigcontext + 处理器参数 + 返回跳板地址），并修改进程 PC/SP 让其从处理器入口开始执行；处理器返回时通过 sigreturn 库函数跳到 SYS_SIGRETURN，内核从用户栈恢复 sigcontext，进程从原 PC 继续执行。
- **HOW**: C 实现 `do_sigsend()` (do_sigsend.c:19-162) + `do_sigreturn()` (do_sigreturn.c:19-95)。

**SIGSEND 关键步骤**:

1. 从用户空间拷贝 `sigmsg` 结构（含 sighandler / mask / signo / sigreturn / stkptr）
2. 计算用户栈指针（`arch_get_sp`），向下留出 sigframe 空间
3. 构建 sigcontext（保存当前寄存器）`(架构相关)`
4. 构建 sigframe（sigcontext + 处理器参数 + 返回地址）
5. 拷贝 sigframe 到用户栈（可能 VMSUSPEND）
6. 修改进程寄存器：SP→sigframe, PC→sighandler **（必须最后，见 §1.3）**

**SIGRETURN 关键步骤**:

1. 从用户栈拷贝 sigcontext（用 `data_copy`，不会 VMSUSPEND）
2. 恢复寄存器 `(架构相关)`；x86 需保留 psw 系统位（IF 等），只合并用户位
3. 调用 `arch_proc_setcontext` 加载上下文
4. 校验 `sc_magic`（仅警告，不返回错误）
5. 恢复 FPU 状态（x86 only）`(架构相关)`

**与内核信号路径的关系**: GETKSIG 是分叉点——SM 拉取信号后决定走 SIGSEND（安装 POSIX 处理器）还是直接处理（如 SIGKILL 默认终止，无需 SIGSEND）。

### 1.3 SIGSEND 时序约束：拷贝后修改寄存器

**灵魂本质**: `data_copy_vmcheck` 可能 VMSUSPEND，恢复后系统调用从入口重新执行——寄存器修改必须发生在最后一次拷贝之后，否则会被重复执行导致状态损坏。

**WHY → WHAT → HOW 弧线**:

- **WHY**: 用户空间页可能未映射，`data_copy_vmcheck` 触发 VMSUSPEND——当前系统调用挂起，等 VM 处理页缺失后从入口重新执行。这是微内核的天然结果：内核不直接访问用户空间，需通过 VM proxy。
- **WHAT**: SIGSEND 前 5 步（sigmsg 拷贝、sigcontext 构建、sigframe 拷贝）是幂等的——重复执行结果相同。但第 6 步（寄存器修改）若在拷贝之前执行，VMSUSPEND 恢复后会再次修改寄存器，导致 SP/PC 被多次重写。
- **HOW**: C 源码 do_sigsend.c:126-131 显式 WARNING 注释："changes to process registers *MUST* be deferred until after this last copy"。

**幂等性分析**:

| 步骤 | C 位置 | 幂等? | 说明 |
|------|--------|------|------|
| sigmsg 拷贝 | do_sigsend.c:36-39 | ✅ | 重读覆盖 |
| sigcontext 构建 | do_sigsend.c:50-115 | ✅ | memset + 字段填充 |
| sigframe 拷贝 | do_sigsend.c:121-124 | ✅ | 重写覆盖 |
| 寄存器修改 | do_sigsend.c:134-151 | ❌ | SP/PC 重写非幂等；MF_FPU_INITIALIZED 清除语义不应重复 |

**Rust 设计含义**: `SignalContext::setup_handler_entry` 方法必须标注 SAFETY 约束——"只能在最后一次 `data_copy_vmcheck` 成功后调用"（见 §4.5 / §4.6）。

### 1.4 信号管理器：s_sig_mgr + s_bak_sig_mgr

**灵魂本质**: 每个进程关联一个信号管理器 endpoint；自管理进程收到致命信号时尝试转发给 backup，无 backup 则 panic。

**WHY → WHAT → HOW 弧线**:

- **WHY**: 微内核中不同进程可有不同的信号管理器（用户进程归 PM，系统进程可自定义）。需要 per-process 字段记录 SM 身份，并在 SM 自身故障时提供 fallback——否则自管理进程收到致命信号将无人处理，系统不可恢复。
- **WHAT**: `KPriv.s_sig_mgr` 存储进程的信号管理器 endpoint（`SELF` 表示自管理）；`s_bak_sig_mgr` 存储备份 SM。GETKSIG/ENDKSIG 用 `caller.p_endpoint == sig_mgr` 校验调用者身份。
- **HOW**: C 实现 `priv(rp)->s_sig_mgr` (system.c:412) + `priv(rp)->s_bak_sig_mgr` (system.c:419)。

**致命信号 panic 路径** (system.c:416-432):

- 自管理进程收到致命信号（`SIGS_IS_LETHAL(sig_nr)`）→ 检查 backup
- 有 backup：切换 `s_sig_mgr = s_bak_sig_mgr`，清除 `s_bak_sig_mgr`，递归调用 `cause_sig` 重试
- 无 backup：`panic("cause_sig: sig manager %d gets lethal signal %d for itself")`——系统不可恢复

**backup 切换的副作用**: `RTS_UNSET(sig_mgr_rp, RTS_NO_PRIV)` (system.c:424)——backup SM 可能因 NO_PRIV 阻塞，切换时需清除让其可接收信号。

> **实现状态（2026-09-05）**: 本小节语义已全部落地——`cause_signal` 三路径（外部 / SELF 非致命 / SELF 致命 backup 提升或 panic）见 §4.3；`dispatch_exit`（SYS_EXIT）经完整 `cause_sig` 自杀（todo D-6/D-11，[17-syscall-process.md](17-syscall-process.md) §4.3）。
>
> **为什么"自管理进程死亡"是内核级事件**：普通进程的信号由 PM 之类的外部管理器处理——进程死了，管理器负责收割（通知父进程、回收资源）。自管理进程（典型如 VM，C main.c:208 `s_sig_mgr = SELF`）**自己就是自己的管理器**：它若死于致命信号，就没有任何存活者会替它"处理后事"，它名下的孤儿信号将永远无人认领。C 给出的答案是：RS 在部署时预设一个 backup 管理器（live update 的接替者）；有 backup 就提升并让它接管；没有 backup，内核直接 panic，而不是让系统带着无人收割的进程半死不活地运行。这一"系统级看护进程不允许静默消失"的设计与主流 OS 一致——Linux 对 init(PID 1) 死亡同样以 `panic("Attempted to kill init!")` 兜底，因为它是所有孤儿进程的最后收割者；Redox 内核无此概念（信号直接由内核分发给目标），其"系统服务崩溃即重启"由 userspace scheme 管理，等效职责不在内核——这正是 Minix3 把故障收敛（failover）决策放在内核的原因：**看护者不能由被看护者自己担任**。

### 1.5 本章小结

内核作为信号中介的核心抽象是"双路径 + 推拉"：内核信号路径用三步闭环（cause_sig → GETKSIG → ENDKSIG）解耦产生方与管理器；POSIX 路径用栈帧改写（SIGSEND → SIGRETURN）让用户态处理器得以运行。两者共享 `p_pending` / `RTS_SIGNALED` / `RTS_SIG_PENDING` 状态，由 BKL 保证原子性。架构相关细节（sigcontext 字段、寄存器修改）由 `SignalContext` trait 统一抽象（§3 D2/D6、§4.6）。后续章节按"概念 → C 源码 → 设计决策 → 实现 → 测试"展开。

---

## 2. C 源码分析

### 2.1 信号相关常量与消息字段

| 符号 | 位置 | 说明 |
|------|------|------|
| `_NSIG` | signal.h | 信号数上限 = 64 |
| `sigset_t` | sigtypes.h:60-62 | 信号位图 = `__uint32_t __bits[4]`（**128 位**）；Rust 对应 `SigSet(u64)`（64 位，容纳 1-64 信号，见 §4.3 注） |
| `sig_mask(sig)` | signal.h | 信号编号到位掩码转换（1-based → 0-based） |
| `SIGKSIG` | signal.h:274 | 内核→SM 通知信号（值 = 74，超出 `_NSIG` 范围） |
| `SIGKSIGSM` | signal.h:273 | 自管理进程的自通知信号（值 = 73）；Rust SELF 路径已实现（§4.3） |
| `SIGSNDELAY` | signal.h:264 | 停止延迟结束信号（值 = 70）；`sig_delay_done` 对应 Rust DEFERRED（§4.7） |
| `SC_MAGIC` | i386 signal.h:115 | sigcontext 完整性魔数（值 = 0xc0ffee1，`(架构相关)`）；Rust `check_magic` 已实现（§4.6） |
| `SIGS_IS_LETHAL(sig)` | signal.h:280-282 | 致命信号判断宏（SIGILL=4/SIGABRT=6/SIGEMT=7/SIGFPE=8/SIGBUS=10/SIGSEGV=11）；Rust `is_lethal` 已实现（syscall_signal.rs:101-103，§4.3） |
| `m_sigcalls.endpt` | minix/ipc.h:2622 | 目标进程 endpoint（5 个信号 syscall 共用） |
| `m_sigcalls.sig` | minix/ipc.h:2622 | 信号编号（KILL 用） |
| `m_sigcalls.map` | minix/ipc.h:2622 | 待处理信号位图（GETKSIG 返回） |
| `m_sigcalls.sigctx` | minix/ipc.h:2622 | sigcontext 指针（SIGSEND/SIGRETURN 用） |

> Rust 对应 `MessSigcalls` (`os/libs/minix-types/src/ipc/message.rs:1025`)：`map: u64` / `endpt: i32` / `sig: i32` / `sigctx: u64`。

### 2.2 核心数据结构

**`struct sigmsg`** (minix/type.h:71-77)——SM 填充、内核通过 `data_copy_vmcheck` 读入的信号消息：

| 字段 | 类型 | 说明 |
|------|------|------|
| `sm_signo` | `int` | 信号编号 |
| `sm_mask` | `sigset_t` | 处理器执行期间阻塞的信号掩码 |
| `sm_sighandler` | `vir_bytes` | 信号处理器地址 |
| `sm_sigreturn` | `vir_bytes` | sigreturn 库函数地址（处理器返回跳板） |
| `sm_stkptr` | `vir_bytes` | 用户栈指针（内核填写，来自 `arch_get_sp`） |

**`struct sigcontext`** (sys/arch/{i386,arm}/include/signal.h:85/92，`架构相关`)——保存的寄存器上下文：

| 字段（x86） | 字段（arm） | 说明 |
|------------|------------|------|
| `sc_gs/sc_fs/sc_es/sc_ds` | — | 段寄存器 (x86 特有) |
| `sc_edi/sc_esi/sc_ebp/sc_ebx/sc_edx/sc_ecx/sc_eax` | `sc_r0..sc_r12` | 通用寄存器 |
| `sc_eip` | `sc_pc` | 程序计数器 |
| `sc_cs` | — | 代码段 (x86 特有) |
| `sc_eflags` | `sc_spsr` | 状态字（x86: eflags；arm: psr） |
| `sc_esp` | `sc_usr_sp` | 用户栈指针 |
| `sc_ss` | — | 栈段 (x86 特有) |
| — | `sc_usr_lr` / `sc_svc_lr` | 链接寄存器 (arm 特有) |
| `sc_fpu_state` | — | FPU 状态 (x86 特有) |
| `sc_mask` | `sc_mask` | 处理器执行期间阻塞的信号 |
| `sc_flags` | `sc_flags` | `MF_FPU_INITIALIZED` 等标志 |
| `sc_magic` | `sc_magic` | `SC_MAGIC` 完整性校验 |
| `trap_style` | `trap_style` | 进入内核的 trap 类型 |

> **架构范围**: x86_64 保留段寄存器字段与 FPU 状态；aarch64 用 `sc_r0..sc_r12` + `sc_usr_lr/sc_svc_lr`，无 FPU 状态字段。riscv64 在本 minix3 快照中未实现（DEFERRED）。

**`struct sigframe_sigcontext`** (sys/arch/{i386,arm}/include/frame.h:151/92)——写到用户栈的帧：

| 字段 | 说明 |
|------|------|
| `sf_sc` | `struct sigcontext`——保存的寄存器 |
| `sf_scp` | 指向 `sf_sc` 的指针 |
| `sf_fp` | 帧指针 |
| `sf_signum` | 信号编号 |
| `sf_ra` | 返回地址（原始 PC） |
| `sf_ra_sigreturn` | sigreturn 库函数地址 |
| `sf_scpcopy` | `sf_scp` 的副本（架构相关） |

### 2.3 关键函数 file:line 索引（行为契约）

#### `do_kill()` — do_kill.c:17-38

| 字段 | 内容 |
|------|------|
| 输入 | `caller`, `m_ptr`（含 `m_sigcalls.endpt` + `m_sigcalls.sig`） |
| 输出 | 返回 OK / EINVAL / EPERM |
| 副作用 | 调用 `cause_sig(proc_nr, sig_nr)` 修改 `p_pending` + `RTS_SIGNALED` |
| 错误码 | EINVAL（无效 endpoint do_kill.c:30 或 `sig_nr >= _NSIG` do_kill.c:31）/ EPERM（目标为内核任务 do_kill.c:32） |
| 时序 | 校验 → cause_sig (do_kill.c:35) → 返回 OK (do_kill.c:37) |
| 状态前置 | 调用者持 BKL；目标进程可能任意状态 |
| 状态后置 | 目标 `p_pending` 含 sig_nr 位；若之前未 SIGNALED 则置 RTS_SIGNALED + SIG_PENDING |
| 竞争条件 | `cause_sig` 内部去重检查（system.c:439）避免重复通知；BKL 保证原子性 |

#### `cause_sig()` — system.c:389-449

| 字段 | 内容 |
|------|------|
| 输入 | `proc_nr_t proc_nr`, `int sig_nr` |
| 输出 | 无返回值（void） |
| 副作用 | 修改 `p_pending`、`p_rts_flags`、`s_sig_pending`；可能 panic |
| 错误码 | 无（panic 是终止路径 system.c:430） |
| 时序 | 查 sig_mgr (system.c:412) → SELF 路径检查 (system.c:416) → 致命信号检查 (system.c:417) → 去重检查 (system.c:439) → 信号位图+RTS 设置 (system.c:442-444) → SIGKSIG 通知 (system.c:445) |
| 状态前置 | 调用者持 BKL；信号相关函数仅在 CPU 异常或内核进程级调用 |
| 状态后置 | 目标进程 SIGNALED（若之前未）；SM 的 `s_sig_pending` 含 SIGKSIG |
| 竞争条件 | BKL 保证原子性；C 注释（system.c:400-403）声明无竞争（信号函数仅在异常/内核进程级调用，run-to-completion） |

#### `do_getksig()` — do_getksig.c:18-42

| 字段 | 内容 |
|------|------|
| 输入 | `caller`, `m_ptr` |
| 输出 | `m_sigcalls.endpt`（找到的进程或 NONE do_getksig.c:40）+ `m_sigcalls.map`（信号位图 do_getksig.c:32） |
| 副作用 | 清除目标的 `p_pending` (do_getksig.c:33) + `RTS_SIGNALED` (do_getksig.c:34) |
| 错误码 | 总是返回 OK（无信号时 endpt = NONE） |
| 时序 | 线性扫描 BEG_USER_ADDR..END_PROC_ADDR (do_getksig.c:27) → RTS_SIGNALED 检查 (do_getksig.c:28) → sig_mgr 校验 (do_getksig.c:29) → 填消息 (do_getksig.c:31-32) → 清状态 (do_getksig.c:33-34) |
| 状态前置 | 调用者持 BKL；caller 是某个进程的 sig_mgr |
| 状态后置 | 找到进程：`p_pending` 清空，RTS_SIGNALED 清除（`RTS_SIG_PENDING` 仍保留，注释 do_getksig.c:34 "blocked by SIG_PENDING"） |
| 竞争条件 | BKL 保证扫描原子性 |

#### `do_endksig()` — do_endksig.c:15-38

| 字段 | 内容 |
|------|------|
| 输入 | `caller`, `m_ptr`（含 `m_sigcalls.endpt`） |
| 输出 | 返回 OK / EINVAL / EPERM |
| 副作用 | 若无新信号：`RTS_UNSET(SIG_PENDING)` (do_endksig.c:36)；若有新信号：保留 SIG_PENDING 等下次 GETKSIG |
| 错误码 | EINVAL（无效 endpoint do_endksig.c:27-28 或 SIG_PENDING 未设置 do_endksig.c:32）/ EPERM（caller 非 sig_mgr do_endksig.c:31） |
| 时序 | 校验 endpoint → sig_mgr 校验 → SIG_PENDING 校验 → 检查新 SIGNALED (do_endksig.c:35) → 决定是否清除 SIG_PENDING |
| 状态前置 | 调用者持 BKL；目标有 SIG_PENDING |
| 状态后置 | 若无新信号：SIG_PENDING 清除，进程可继续调度 |
| 竞争条件 | BKL 保证 |

#### `do_sigsend()` — do_sigsend.c:19-162

| 字段 | 内容 |
|------|------|
| 输入 | `caller`, `m_ptr`（含 `m_sigcalls.endpt` + `m_sigcalls.sigctx`） |
| 输出 | 返回 OK / EINVAL / EPERM / 数据拷贝错误 |
| 副作用 | 修改目标 `p_reg`（sp/pc/fp/lr 等）+ `p_misc_flags`（清 `MF_FPU_INITIALIZED` do_sigsend.c:154，arm 设 `MF_CONTEXT_SET` do_sigsend.c:150） |
| 错误码 | EINVAL（无效 endpoint do_sigsend.c:31 或 `trap_style==KTS_NONE` do_sigsend.c:79-81）/ EPERM（内核任务 do_sigsend.c:32）/ `data_copy_vmcheck` 错误 |
| 时序 | 校验 → sigmsg 拷贝 (do_sigsend.c:36-39，可能 VMSUSPEND) → 计算 SP (do_sigsend.c:46-47) → 构建 sigcontext (do_sigsend.c:50-115，架构相关) → sigframe 拷贝 (do_sigsend.c:121-124，可能 VMSUSPEND) → **修改寄存器** (do_sigsend.c:134-151，必须最后) |
| 状态前置 | 调用者持 BKL；目标非内核任务 |
| 状态后置 | 目标 `p_reg.sp = sigframe 地址`，`p_reg.pc = sighandler` |
| 竞争条件 | VMSUSPEND 可重入；前 5 步幂等；第 6 步非幂等（见 §1.3） |

#### `do_sigreturn()` — do_sigreturn.c:19-95

| 字段 | 内容 |
|------|------|
| 输入 | `caller`, `m_ptr`（含 `m_sigcalls.endpt` + `m_sigcalls.sigctx`） |
| 输出 | 返回 OK / EINVAL / EPERM / 数据拷贝错误 |
| 副作用 | 修改目标 `p_reg`（恢复寄存器）+ `p_misc_flags`（恢复 `MF_FPU_INITIALIZED` do_sigreturn.c:89） |
| 错误码 | EINVAL（无效 endpoint do_sigreturn.c:28）/ EPERM（内核任务 do_sigreturn.c:29）/ `data_copy` 错误 |
| 时序 | 校验 → sigcontext 拷贝 (do_sigreturn.c:33-36) → psw 用户位合并 (do_sigreturn.c:40-41，x86) → 恢复寄存器 (do_sigreturn.c:44-78，架构相关) → `arch_proc_setcontext` (do_sigreturn.c:81) → `sc_magic` 校验 (do_sigreturn.c:83) → FPU 恢复 (do_sigreturn.c:85-93，x86) |
| 状态前置 | 调用者持 BKL；目标从信号处理器返回 |
| 状态后置 | 目标 `p_reg` 恢复到 SIGSEND 之前；FPU 状态恢复 |
| 竞争条件 | 用 `data_copy`（非 `data_copy_vmcheck`）不会 VMSUSPEND；sigreturn 不可重入 |

#### `sig_delay_done()` — system.c:454-464

| 字段 | 内容 |
|------|------|
| 输入 | `struct proc *rp` |
| 输出 | 无返回值（void） |
| 副作用 | 清除 `MF_SIG_DELAY` (system.c:461)；调用 `cause_sig(proc_nr(rp), SIGSNDELAY)` (system.c:463) |
| 错误码 | 无 |
| 时序 | 清 flag → cause_sig(SIGSNDELAY) |
| 状态前置 | 进程不再发送直接消息 |
| 状态后置 | 通知 PM 停止延迟结束 |
| 竞争条件 | BKL 保证 |
| Rust 状态 | DEFERRED（需 PM 通知接口） |

### 2.4 调用关系图（双路径时序）

**内核信号路径时序**:

```
[内核内部] cause_sig(proc_nr, sig_nr)        system.c:389
  ├─ sigaddset(&p_pending, sig_nr)           system.c:442
  ├─ RTS_SET(SIGNALED | SIG_PENDING)         system.c:444
  └─ send_sig(sig_mgr, SIGKSIG) → mini_notify system.c:445

[SM 主动轮询] SYS_GETKSIG                     do_getksig.c:18
  ├─ 扫描 BEG_USER_ADDR..END_PROC_ADDR        do_getksig.c:27
  ├─ 找 RTS_SIGNALED 且 caller == s_sig_mgr   do_getksig.c:28-29
  ├─ 填消息: endpt + map                      do_getksig.c:31-32
  ├─ sigemptyset(&p_pending)                  do_getksig.c:33
  └─ RTS_UNSET(SIGNALED)                      do_getksig.c:34

[SM 处理完毕] SYS_ENDKSIG                     do_endksig.c:15
  ├─ 校验 caller == s_sig_mgr                do_endksig.c:31
  ├─ 校验 RTS_SIG_PENDING 已设置             do_endksig.c:32
  └─ if (!RTS_SIGNALED): RTS_UNSET(SIG_PENDING) do_endksig.c:35-36
```

**POSIX 信号路径时序**:

```
[SM 安装处理器] SYS_SIGSEND                   do_sigsend.c:19
  ├─ data_copy_vmcheck(sigmsg)               do_sigsend.c:36-39  ← 可能 VMSUSPEND
  ├─ 计算 SP (arch_get_sp)                   do_sigsend.c:46-47
  ├─ 构建 sigcontext                         do_sigsend.c:50-115 ← 架构相关
  ├─ data_copy_vmcheck(sigframe)             do_sigsend.c:121-124 ← 可能 VMSUSPEND
  └─ 修改寄存器                              do_sigsend.c:134-151 ← 必须最后

[处理器返回] SYS_SIGRETURN                    do_sigreturn.c:19
  ├─ data_copy(sigcontext)                   do_sigreturn.c:33-36
  ├─ psw 用户位合并（x86）                   do_sigreturn.c:40-41
  ├─ 恢复寄存器                              do_sigreturn.c:44-78 ← 架构相关
  ├─ arch_proc_setcontext                    do_sigreturn.c:81
  └─ FPU 恢复（x86）                         do_sigreturn.c:85-93
```

---

## 3. Rust 设计决策

> 本章采用 hypothesis-driven：每个决策以"如果选 X 会有 Y 问题所以用 Z"展开，含被否决选项与理由。

### D1. 信号位图类型：`SigSet(u64)` newtype

- 如果用 `sigset_t` struct 包装 `u64`：与 C 的 `sigset_t` 行为一致，但 Rust 中 struct 包装单字段类型增加无意义的间接层；调用 `sigaddset` / `sigismember` 需函数调用而非方法。
- 如果用 `bitflags!` 宏：类型安全支持位组合（`|` / `&` / `contains`），但 64 个信号需列 64 个常量，冗长；且 `bitflags!` 语义是"标志位集合"而非"信号集合"——信号编号是协议常量非正交标志。
- 所以用 **`SigSet(u64)` newtype**：类型安全（防止与普通 u64 混淆），单字段直接位运算，与 C `sigset_t` 语义对齐；提供 `add` / `remove` / `contains` / `clear` / `is_empty` / `get` 方法。

> 设计决策：`design.md §2.1`。权威定义 `os/kernel/src/proc.rs:1100`。

### D2. sigcontext 保存/恢复：trait 抽象替代 `#[cfg(target_arch)]`

- 如果直接在 `dispatch_sigsend` 中用 `#[cfg(target_arch = "x86_64")]` 选择寄存器字段：内核代码出现架构分支，每加一个架构需修改 `syscall_signal.rs` 多处；违反"硬件抽象为 trait"原则。
- 如果用函数指针表（C 方式）：类型不安全，且无法利用 Rust trait 静态分发优化。
- 所以定义 **`trait SignalContext`**：把 sigcontext / sigframe 定义为关联类型（架构不同字段不同），各架构在 arch 层提供实现，内核 dispatch 调用 trait 方法。配合 D6 用泛型静态分发，零虚拟开销。

> 设计决策：`design.md §2.4`。trait 定义 `os/arch/src/arch/signal_context.rs:154`。

### D3. SIGSEND 寄存器修改时序：保留 C 的"拷贝后修改"约束

- 如果不保留时序约束（在拷贝前修改寄存器）：VMSUSPEND 恢复后系统调用从入口重新执行，寄存器被多次修改——SP/PC 重写看似幂等，但 `p_reg.pc = sighandler` 若 sighandler 已变则错；`MF_FPU_INITIALIZED` 清除语义上不应重复。
- 如果完全避免 VMSUSPEND（用同步拷贝）：微内核不能直接访问用户空间，必须通过 VM proxy，VM proxy 可能页缺失——VMSUSPEND 不可避免。
- 所以**保留 C 的时序约束**：`SignalContext::setup_handler_entry` 必须在最后一次 `data_copy_vmcheck` 成功后调用；trait 方法 SAFETY 注释明确约束（见 §4.6）。

> 设计决策：`design.md §3.4`。

### D4. cause_sig 位置：free function + 表参数 vs KProcess 方法

- 如果用全局函数 `fn cause_signal(target_nr, sig_nr, proc_table, priv_table)`：参数列表长（4 个），且需访问 `ProcessTable` + `PrivTable` 两个表；调用方需先查表再传参。
- 如果完全用 KProcess 方法 `fn cause_signal(&mut self, sig_nr, priv_table)`：封装 `p_pending` + `p_rts_flags` 操作；但 `cause_sig` 还需修改 SM 的 `s_sig_pending`，需访问 `PrivTable`，且 SELF 路径需查 `s_sig_mgr`，不能完全封装在 KProcess 内。
- 所以当前用 **free function** `cause_signal(target_nr, sig_nr, proc_table, priv_table)` (`syscall_signal.rs:194`)：显式传入两个表，借用清晰；design §3.2 给出未来重构为 KProcess 方法的目标签名（需拆分借用）。

> 设计决策：`design.md §3.2`（目标方法签名）。当前实现 `os/kernel/src/syscall_signal.rs:194`。

### D5. GETKSIG 进程扫描：线性扫描 vs 信号队列

- 如果用信号队列（链表 / BTreeSet 维护待处理进程）：O(1) 或 O(log N) 查找；但每次 `cause_sig` 需入队、ENDKSIG 需出队——增加状态管理复杂度；且与 C 不一致，调试困难。
- 如果用线性扫描（C 方式）：O(N) 扫描所有进程；但进程数 < 128（`CONFIG_MAX_PROCS`），扫描成本可忽略；与 C 一致。
- 所以用**线性扫描**：与 C 一致，简单可维护，性能足够。

> 设计决策：`design.md` + design 快照 v2（致命 SELF 路径，见 19-design.v2）。实现 `os/kernel/src/syscall_signal.rs:194-322`（`cause_signal` 全函数，三路径 + 致命子路径）。

### D6. SIGSEND/SIGRETURN 架构相关代码：`#[cfg(target_arch)]` vs trait 静态分发

- 如果用 `#[cfg(target_arch = "x86_64")]` 选择寄存器字段：内核代码出现架构分支；每加一个架构（aarch64 / riscv64）需修改 `syscall_signal.rs` 多处；违反"硬件抽象为 trait"原则。
- 如果用 trait object `Box<dyn SignalContext>`：动态分发，堆分配，`no_std` 不友好。
- 所以用 **`trait SignalContext` + 关联类型 + 静态分发**：泛型 `dispatch_sigsend<A: SignalContext>(...)`，编译期单态化，零虚拟开销；各架构在 arch 层提供 impl。

> 设计决策：`design.md §3.5`。trait 已声明，三架构 arch impl 均已实现（见 §4.6）。

### D7. 信号常量：`pub const` vs `enum`

- 如果用 `enum Signal { Sigvtalrm = 26, ... }`：类型安全，但 C `signal.h` 的信号是 `int`，跨 FFI 边界时需 `as i32`；且信号编号是协议常量（非穷尽集合，可扩展）。
- 如果用 `pub const SIGVTALRM: u32 = 26`：与 C 一致，跨 FFI 友好；调用方写 `SIGVTALRM` 而非 `Signal::Sigvtalrm`。
- 所以用 **`pub const`**：与 C 一致，简单直接。

> 设计决策：`design.md §2.2`。实现 `os/kernel/src/syscall_signal.rs:39-57`。

---

## 4. 实现详解

### 4.1 核心类型

`SigSet` newtype 的权威定义在 `proc.rs:1100`（D1），全内核单一类型——`p_pending` / `s_sig_pending` / `SigMsg.mask` 均使用此 newtype，无局部别名。`syscall_signal.rs` 通过 `use crate::proc::SigSet` 导入复用，避免类型分裂。

```rust
// os/kernel/src/proc.rs:1100
/// 信号位图。对应 C 的 sigset_t，64 位容纳 64 个信号。
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SigSet(u64);

// os/kernel/src/proc.rs:1102-1153 —— 方法封装替代 C 的 sigaddset/sigismember/sigemptyset 函数族
impl SigSet {
    pub const fn empty() -> Self { Self(0) }
    /// 从原始 u64 构造（IPC 消息字段反序列化用）。`get()` 的逆运算。
    pub const fn from_raw(value: u64) -> Self { Self(value) }
    pub fn contains(self, sig: u8) -> bool { /* 位检测 */ }
    pub fn add(&mut self, sig: u8) { /* 1-based 编号 → 位设置 */ }
    pub fn remove(&mut self, sig: u8) { /* 位清除 */ }
    pub fn clear(&mut self) { self.0 = 0; }
    pub const fn is_empty(self) -> bool { self.0 == 0 }
    /// 取原始 u64，用于写入 IPC 消息（m_sigcalls.map）。
    pub fn get(self) -> u64 { self.0 }
}
```

`Endpoint` newtype (`os/libs/minix-types/src/types/endpoint.rs:44`) 替代 C 的裸 `endpoint_t`，提供 `NONE` / `SELF` / `KERNEL` 常量，防止与普通 i32 混淆。

信号位掩码辅助函数（对齐 C `sig_mask`）：

```rust
// os/kernel/src/syscall_signal.rs:72-103
/// 构造信号位掩码（1-based 编号）。C: sig_mask(sig) — signal.h
pub const fn sig_mask(sig_nr: u32) -> SigSet {
    if sig_nr == 0 || sig_nr as usize > NSIG { SigSet::empty() }
    else { SigSet::from_raw(1u64 << (sig_nr - 1)) }
}
```

### 4.2 信号常量

```rust
// os/kernel/src/syscall_signal.rs:47-103
pub const NSIG: usize = 64;        // C: _NSIG — signal.h
pub const SIGILL: u32 = 4;         // C: SIGILL — signal.h:55
pub const SIGTRAP: u32 = 5;        // C: SIGTRAP
pub const SIGABRT: u32 = 6;        // C: SIGABRT
pub const SIGEMT: u32 = 7;         // C: SIGEMT — signal.h:59
pub const SIGFPE: u32 = 8;         // C: SIGFPE — signal.h:60
pub const SIGBUS: u32 = 10;        // C: SIGBUS — signal.h:62
pub const SIGSEGV: u32 = 11;       // C: SIGSEGV — signal.h:63
pub const SIGVTALRM: u32 = 26;     // C: SIGVTALRM
pub const SIGPROF: u32 = 27;       // C: SIGPROF
/// 内核→SM 通知信号。C: SIGKSIG = 74 — signal.h:274
/// 超出 _NSIG(1-64) 范围，是内核内部通知而非 POSIX 信号。
pub const SIGKSIG: u32 = 74;

/// 致命信号判定。C: SIGS_IS_LETHAL — signal.h:280-282
pub(crate) const fn is_lethal(sig_nr: u32) -> bool {
    matches!(sig_nr, SIGILL | SIGBUS | SIGFPE | SIGSEGV | SIGEMT | SIGABRT)
}
```

> Rust 未定义常量：`SIGKSIGSM`（=73，signal.h:273）、`SC_MAGIC`（=0xc0ffee1，i386 signal.h:115）、`SIGSNDELAY`（=70，signal.h:264）——C 定义均已定位：SELF 路径通知直接经 `mini_notify_core` 实现（>64 的信号无法编码进 Rust `SigSet(u64)`，唤醒即 C 语义的全部内核动作，见 §4.3 语义澄清），`check_magic` 在 arch 层实现（§4.6），`sig_delay_done` 整体 DEFERRED（§4.7）。致命信号常量 `SIGILL`/`SIGEMT`/`SIGFPE`/`SIGBUS`/`SIGSEGV` 已定义（syscall_signal.rs:47-66），供 `is_lethal` 使用。

### 4.3 cause_signal 实现

> 设计决策：§3 D4 + design 快照 v2（D-3b 致命 SELF 路径 / D-3c `is_lethal`，见 19-design.v2）。
> 当前为 free function，design §3.2 给出 KProcess 方法的目标重构。

```rust
// os/kernel/src/syscall_signal.rs:194-327（行号随版本漂移，以 rg 为准）
pub(crate) fn cause_signal(
    target_nr: ProcNr,
    sig_nr: u32,
    proc_table: &mut ProcessTable,
    priv_table: &mut PrivTable,
) {
    // C: system.c:411-413 — rp = proc_addr(proc_nr); sig_mgr = priv(rp)->s_sig_mgr;
    //     if (sig_mgr == SELF) sig_mgr = rp->p_endpoint
    let sig_mgr = proc_table.sig_mgr(target_nr, priv_table);
    let target_endpoint = proc_table.get(target_nr).map(|p| p.p_endpoint);

    // ── SELF 路径：目标进程是自己的信号管理器 ──
    // C: system.c:416 — if (rp->p_endpoint == sig_mgr) → 直接自管理，不走外部通知
    if let (Some(ep), Some(mgr)) = (target_endpoint, sig_mgr)
        && ep == mgr
    {
        // C: system.c:417 — 致命信号（SIGS_IS_LETHAL）→ 备份管理器提升 / panic
        if is_lethal(sig_nr) {
            // C: system.c:418-420 — sig_mgr = priv(rp)->s_bak_sig_mgr;
            //     if (sig_mgr != NONE && isokendpt(sig_mgr, &sig_mgr_proc_nr))
            let pid = proc_table.get(target_nr).and_then(|p| p.priv_id);
            let backup = pid
                .and_then(|pid| priv_table.get(pid))
                .map(|priv_| priv_.signals.s_bak_sig_mgr);

            if let Some(bak_ep) = backup
                && bak_ep != Endpoint::NONE
                && let Some(bak_nr) = proc_table.endpoint_to_nr(bak_ep)
            {
                // C: system.c:421-422 — 提升：s_sig_mgr ← backup；s_bak_sig_mgr ← NONE
                if let Some(pid) = pid
                    && let Some(priv_) = priv_table.get_mut(pid)
                {
                    priv_.signals.s_sig_mgr = bak_ep;
                    priv_.signals.s_bak_sig_mgr = Endpoint::NONE;
                }
                // C: system.c:424 — RTS_UNSET(backup, RTS_NO_PRIV)：解除挂起
                proc_table.rts_unset(bak_nr, RtsFlagsBits::NO_PRIV);
                // C: system.c:425-426 — cause_sig(proc_nr, sig_nr) 递归重投（外部路径）
                cause_signal(target_nr, sig_nr, proc_table, priv_table);
                return;
            }
            // C: system.c:429-431 — 无 backup → panic（系统不可恢复）
            panic!(
                "cause_sig: sig manager {} gets lethal signal {} for itself",
                ep.0, sig_nr
            );
        }

        // C: system.c:433 — sigaddset(&priv(rp)->s_sig_pending, sig_nr)（非致命自管理）
        if let Some(pid) = proc_table.get(target_nr).and_then(|p| p.priv_id)
            && let Some(priv_) = priv_table.get_mut(pid)
        {
            priv_.signals.s_sig_pending.add(sig_nr as u8);
        }

        // C: system.c:434 — send_sig(rp->p_endpoint, SIGKSIGSM) → mini_notify(自身)
        // SIGKSIGSM=73 的内核动作仅是"唤醒自管理进程去处理自身待决状态"；
        // Rust SigSet(u64) 无法编码 >64 的信号，唤醒本身由 mini_notify 完成。
        let _ = crate::ipc::mini_notify_core(
            proc_table.procs_slice_mut(),
            priv_table,
            crate::proc::proc_nr::SYSTEM,
            ep,
        );
        return;
    }

    // ── 外部路径：目标由外部信号管理器管理 ──
    // C: system.c:439-442 — sigaddset(&rp->p_pending, sig_nr)（sigismember 门控
    // 以 RTS_SIGNALED 判定 was_signaled；sigaddset 幂等，语义等价）
    let was_signaled = proc_table.get(target_nr)
        .is_some_and(|p| p.p_rts_flags.is_set(RtsFlagsBits::SIGNALED));
    if let Some(target) = proc_table.get_mut(target_nr) {
        target.p_pending.add(sig_nr as u8);
    }

    // C: system.c:443-446 — if (!RTS_ISSET(rp, RTS_SIGNALED)) → RTS_SIGNALED|RTS_SIG_PENDING
    //     + send_sig(sig_mgr, SIGKSIG)（system.c:381 mini_notify SYSTEM → sig_mgr）
    if !was_signaled {
        proc_table.rts_set(target_nr, RtsFlagsBits::SIGNALED | RtsFlagsBits::SIG_PENDING);
        if let Some(sig_mgr_ep) = sig_mgr {
            if let Some(sig_mgr_nr) = proc_table.endpoint_to_nr(sig_mgr_ep)
                && let Some(sig_mgr_proc) = proc_table.get(sig_mgr_nr)
                    && let Some(pid) = sig_mgr_proc.priv_id
                        && let Some(sig_mgr_priv) = priv_table.get_mut(pid) {
                            sig_mgr_priv.signals.s_sig_pending.add(SIGKSIG as u8);
                        }
            let _ = crate::ipc::mini_notify_core(
                proc_table.procs_slice_mut(),
                priv_table,
                crate::proc::proc_nr::SYSTEM,
                sig_mgr_ep,
            );
        }
    }
}
```

**已实现（三路径 + 致命子路径）**：
1. **外部路径**（目标由他人管理）：写目标 `p_pending`，`RTS_SIGNALED|RTS_SIG_PENDING` 挂起目标，
   唤醒 **SM 自身**（SIGKSIG 置位 + SYSTEM 源 `mini_notify_core`）。这是 PM 管理的用户进程
   收到信号的常态路径。
2. **SELF 非致命路径**：目标即自身 SM → 信号写入目标 priv 的 `s_sig_pending`，唤醒目标自身
   （进程不被 RTS 挂起，由它自行决定何时处理）。
3. **SELF 致命路径**（system.c:417-432，design 快照 v2 D-3b）：目标即自身 SM 且信号致命
   （`is_lethal`：SIGILL/ABRT/EMT/FPE/BUS/SEGV）→ 查 `s_bak_sig_mgr`：
   - 有 backup 且 endpoint 有效 → 提升 backup 为 primary（清 `s_bak_sig_mgr`）+ 
     `RTS_UNSET(backup, RTS_NO_PRIV)` + **递归 cause_signal 走外部路径**（让新 manager 收割）；
   - 无 backup → `panic!`（自管理服务无人收割 = 系统不可恢复；Rust 省略 C 的 `proc_stacktrace`，
     见 §4.7 差异表）。

`is_lethal`（syscall_signal.rs:101-103）精确匹配 C `SIGS_IS_LETHAL`（signal.h:280-282）的六个信号。

> **语义澄清（2026-09-05）**：SELF 路径上 SIGKSIGSM/SIGKSIG（73/74）超出 Rust `SigSet(u64)`
> 位宽，add 为 no-op；二者的内核动作就是"唤醒"，由 `mini_notify_core` 完成，行为保持。
> `s_sig_pending`（≤64 位部分）的消费方在 ipc 通知投递路径——SYSTEM 源通知送达时被编码进
> `m_notify.sigset` 并清空（ipc.rs:1776-1782）。

> **DIAGCTL `send_sig(PM_PROC_NR, SIGKMESS)` 仍 DEFERRED**：DIAGCTL_CODE_REGISTER 路径（syscall.rs:2271-2315，do_diagctl.c:49-56）只设 `s_diag_sig=true`，未发 SIGKMESS 通知。原因：DIAGCTL dispatch 已持 caller/priv_table 借用，与查 PM_PROC_NR 所需的全局 proc_table 访问冲突；PM 会在下次 `getksig` 轮询时观察到内核消息（主用途已实现，PM 通知是次要副作用）。

### 4.4 dispatch_getksig / dispatch_endksig 实现

`dispatch_kill` / `dispatch_getksig` / `dispatch_endksig` 已完整实现，对齐 C 语义。

```rust
// os/kernel/src/syscall_signal.rs:269-350（dispatch_getksig）
pub fn dispatch_getksig(
    caller: &mut KProcess,
    msg: &mut Message,
    proc_table: &mut ProcessTable,
    priv_table: &PrivTable,
) -> KcallResult {
    let mut found: Option<(Endpoint, u64)> = None;

    // C: do_getksig.c:27 — for (rp = BEG_USER_ADDR; rp < END_PROC_ADDR; rp++)
    for rp in proc_table.iter() {
        if rp.p_rts_flags.get() == RtsFlagsBits::SLOT_FREE { continue; }
        if ProcessTable::is_kernel(rp.p_nr) { continue; }
        // C: do_getksig.c:28 — if (!RTS_ISSET(rp, RTS_SIGNALED)) continue
        if !rp.p_rts_flags.is_set(RtsFlagsBits::SIGNALED) { continue; }
        // C: do_getksig.c:29 — if (caller->p_endpoint != priv(rp)->s_sig_mgr) continue
        let sig_mgr = proc_table.sig_mgr(rp.p_nr, priv_table);
        if sig_mgr != Some(caller.p_endpoint) { continue; }

        found = Some((rp.p_endpoint, rp.p_pending.get()));
        break;
    }

    if let Some((endpt, map)) = found {
        let target_nr = proc_table.endpoint_to_nr(endpt).unwrap();
        // C: do_getksig.c:33 — RTS_UNSET(rp, RTS_SIGNALED)
        proc_table.rts_unset(target_nr, RtsFlagsBits::SIGNALED);
        // C: do_getksig.c:34 — sigemptyset(&rp->p_pending)
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_pending.clear();
        }
        msg.m_u.m_sigcalls = MessSigcalls { map, endpt: endpt.get(), sig: 0, sigctx: 0, _padding: [0u8; 32] };
    } else {
        // C: do_getksig.c:40 — m_ptr->m_sigcalls.endpt = NONE
        msg.m_u.m_sigcalls = MessSigcalls { map: 0, endpt: Endpoint::NONE.get(), sig: 0, sigctx: 0, _padding: [0u8; 32] };
    }
    KcallResult::Ok(OK)
}
```

> **状态对齐说明**：C 源码 do_getksig.c:34 注释 "blocked by SIG_PENDING"——GETKSIG 只清 `RTS_SIGNALED`，不清 `RTS_SIG_PENDING`（后者由 ENDKSIG 清），因此进程在 SM 处理期间仍被 `RTS_SIG_PENDING` 阻塞调度。Rust 实现一致。

`dispatch_endksig`（`syscall_signal.rs:351-406`）四步校验对齐 do_endksig.c:27-37：endpoint 校验 → sig_mgr 校验（EPERM）→ SIG_PENDING 校验（EINVAL）→ 无新 SIGNALED 则清 SIG_PENDING。

### 4.5 dispatch_sigsend / dispatch_sigreturn（已实现：data_copy_vmcheck + SignalContext）

完整流程已接入 `data_copy_vmcheck`（cross_space.rs）和 `CurrentSignalContext`（arch trait），包括 VMSUSPEND 处理和时序约束。

```rust
// os/kernel/src/syscall_signal.rs:516-689（dispatch_sigsend 节选）
pub fn dispatch_sigsend(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
) -> KcallResult {
    let sc = msg_sigcalls(msg);
    // C: do_sigsend.c:31,37 — m_sigcalls.endpt / m_sigcalls.sigctx
    // C 无 SELF 替换：endpoint 原样传 isokendpt（负 endpoint 如 SYSTEM → EINVAL）。
    let endpt = sc.endpt;
    let sigctx_addr = sc.sigctx;

    // C: do_sigsend.c:31 — isokendpt → EINVAL
    let target_nr = match proc_table.endpoint_to_nr(Endpoint(endpt)) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };
    // C: do_sigsend.c:32 — iskerneln → EPERM
    if target_nr.0 < 0 { return KcallResult::Ok(EPERM); }

    // ── Step 1: Copy sigmsg from caller's user space (may VMSUSPEND) ──
    // C: do_sigsend.c:36-39 — data_copy_vmcheck(caller, caller_ep, sigctx, KERNEL, &smsg, sizeof)
    let mut smsg: SigMsg = SigMsg::default();
    // ... data_copy_vmcheck(caller, src=Process{caller, sigctx_addr},
    //                       dst=Physical{smsg_phys}, sizeof(SigMsg), proc_cr3) ...
    // → VmSuspend | Ok(EFAULT) | Ok(())

    // ── Step 2-3: Build sigcontext + sigframe (idempotent, arch-specific) ──
    // C: do_sigsend.c:49-118 — compute stack ptr, build sigcontext, build sigframe
    let (frame, frame_addr) = {
        let target = proc_table.get(target_nr)?;
        let mut info = SignalInfo { sighandler, mask, signo, sigreturn, stkptr };
        let sctx = CurrentSignalContext::build_sigcontext(&target.cpu_context, &mut info);
        let frame_addr = info.stkptr.saturating_sub(CurrentSignalContext::sigframe_size());
        let frame = CurrentSignalContext::build_sigframe(
            &target.cpu_context, &sctx, &info, frame_addr);
        (frame, frame_addr)
    };

    // ── Step 4: Copy sigframe to target's user stack (may VMSUSPEND) ──
    // C: do_sigsend.c:120-125 — data_copy_vmcheck(caller, KERNEL, &fr, endpt, frp, sizeof)
    // ... data_copy_vmcheck(caller, src=Physical{frame_phys},
    //                       dst=Process{endpt, frame_addr}, sigframe_size(), proc_cr3) ...

    // ── Step 5: Modify target registers for handler entry ──
    // C: do_sigsend.c:130-145 — MUST be after the last data_copy_vmcheck!
    // SAFETY (D3, §1.3): VMSUSPEND 恢复后会从入口重新执行；寄存器修改
    //         非幂等，必须在最后一次 data_copy_vmcheck 成功之后。
    CurrentSignalContext::setup_handler_entry(&mut target.cpu_context, &info, frame_addr);
    KcallResult::Ok(OK)
}
```

`dispatch_sigreturn`（`syscall_signal.rs:690-776`）同样已接入完整流程：

1. **拷贝 sigcontext**：用 `data_copy_vmcheck` 从目标用户栈拷贝 `SigContext`（C 用 `data_copy` 无 vmcheck；Rust 统一用 `data_copy_vmcheck`，因为用户栈页可能未映射）
2. **恢复寄存器**：`CurrentSignalContext::restore_sigcontext` + `arch_setcontext(trap_style)`
3. **校验 magic**：`check_magic(&sctx)`（仅警告，不返回错误，对齐 C: do_sigreturn.c:83）
4. **FPU 状态恢复**：64-bit 不恢复 FPU 状态（对齐 C: do_sigreturn.c:85-93 的 `#if defined(__i386__)` gating）。64-bit 信号投递路径（`do_sigsend.c:154`）清除 `MF_FPU_INITIALIZED`，信号处理器通过 lazy trap-on-first-FP-instruction 机制获得干净 FPU；sigreturn 不恢复 pre-signal FPU 状态。`KProcess.fpu_state` 缓冲区由 SMP 迁移 SAVE_CTX 路径（`smp.rs:497-527`）使用，不参与 64-bit 信号投递/返回（与 C 行为一致）

### 4.6 SignalContext trait（已实现：三架构 arch impl）

trait 定义在 `os/arch/src/arch/signal_context.rs`，三架构实现分别在 `os/arch/src/{x86_64,arm64,riscv64}/signal.rs`。内核通过 `CurrentSignalContext` 类型别名静态分发（无 `#[cfg(target_arch)]` 泄漏到 kernel crate）。

```rust
// os/arch/src/arch/signal_context.rs — trait 定义（节选）
pub trait SignalContext: Sized + Send + Sync {
    /// 保存的寄存器状态。C: struct sigcontext — sys/arch/{i386,arm}/include/signal.h。
    type SigContext: Default + Copy + Send + Sync;
    /// 写到用户栈的信号帧。C: struct sigframe_sigcontext。
    type SigFrame: Default + Copy + Send + Sync;
    /// 架构私有的 CPU 上下文类型（= KProcess.cpu_context 的类型）。
    type CpuContext: Send + Sync;

    /// 从进程当前寄存器构建 sigcontext。C: do_sigsend.c:50-115。
    /// 幂等：VMSUSPEND 可致多次调用，但因只读 CpuContext（setup_handler_entry 前不变），结果相同。
    fn build_sigcontext(ctx: &Self::CpuContext, info: &mut SignalInfo) -> Self::SigContext;
    fn build_sigframe(ctx: &Self::CpuContext, sctx: &Self::SigContext,
                      info: &SignalInfo, frame_addr: u64) -> Self::SigFrame;

    /// 修改进程寄存器以进入信号处理器。C: do_sigsend.c:134-151。
    /// # SAFETY (D3, §1.3)
    /// **必须**在 sigframe 成功拷贝到用户空间后调用。data_copy_vmcheck 可能 VMSUSPEND，
    /// 恢复后会从入口重新执行系统调用；若在拷贝前修改寄存器，将被多次执行导致状态损坏。
    fn setup_handler_entry(ctx: &mut Self::CpuContext, info: &SignalInfo, frame_addr: u64);

    /// 从 sigcontext 恢复寄存器。C: do_sigreturn.c:44-78。
    /// x86_64: 合并 RFLAGS 用户位（保留系统位 IF 等）；aarch64: 恢复完整 SPSR。
    fn restore_sigcontext(ctx: &mut Self::CpuContext, sctx: &Self::SigContext);
    fn arch_setcontext(ctx: &mut Self::CpuContext, trap_style: i32); // C: do_sigreturn.c:81
    /// 取进程当前栈指针。C: `arch_get_sp(rp)` — do_sigsend.c:46
    fn get_sp(ctx: &Self::CpuContext) -> u64;
    fn sigframe_size() -> usize;                                     // C: sizeof(sigframe_sigcontext)
    fn check_magic(sctx: &Self::SigContext) -> bool;                 // C: do_sigreturn.c:83
    fn get_trap_style(sctx: &Self::SigContext) -> i32;               // C: do_sigreturn.c:81
    /// mcontext 结构（SYS_GETMCONTEXT / SYS_SETMCONTEXT 用，C ABI 兼容）。
    type Mcontext: Copy + core::fmt::Debug + Default + Send + Sync;
    fn mcontext_clear_flags(mc: &mut Self::Mcontext);                // C: mc.mc_flags = 0 — do_mcontext.c:47
}
```

**实现状态**：三架构均已实现：

| 架构 | impl 位置 | build_sigcontext | setup_handler_entry | restore_sigcontext |
|------|----------|-----------------|--------------------|--------------------|
| x86_64 | `os/arch/src/x86_64/signal.rs` | GS/FS/ES/DS + GP regs + RIP/RFLAGS/RSP/SS | SP=sigframe, PC=sighandler, FP=new_fp | 合并 RFLAGS 用户位（`X86_FLAGS_USER=0x0CD7`）+ 恢复 GP regs |
| aarch64 | `os/arch/src/arm64/signal.rs` | SPSR + X0-X30 + SP + LR + PC | LR=sigreturn, X0=signo, X2=sigctx | 恢复完整 SPSR + GP regs |
| riscv64 | `os/arch/src/riscv64/signal.rs` | sstatus + X0-X31 + sepc | RA=sigreturn, A0=signo, A1=sigctx | 恢复 sstatus + GP regs |

> **设计说明**：`SignalContext` trait 的关联类型 `CpuContext` 必须与 `KProcess.cpu_context` 的类型（`CurrentCpuContext`）一致。因此 `CurrentSignalContext` **没有 mock 变体**——mock 会引入类型不匹配。测试通过 `#[cfg(test)]` 下的真实架构实现进行。

> **SignalInfo vs SigMsg**：arch 层用 `SignalInfo`（`u64` mask）而非 kernel 层的 `SigMsg`（`SigSet` newtype），避免 arch crate 依赖 kernel crate。`dispatch_sigsend` 在调用 arch 方法前进行 `SigMsg → SignalInfo` 转换。

### 4.7 DEFERRED 表（诚实标注）

> 行号为 2026-09-05 快照（cause_sig 致命路径落地后），漂移以 `rg` 为准。

| 功能 | C 位置 | Rust 位置 | 状态 / DEFERRED 理由 |
|------|--------|----------|---------------------|
| SIGKSIG 通知（mini_notify） | system.c:445-446（send_sig 内 system.c:381） | syscall_signal.rs:300-325 | ✅ 已实现: SM 的 `s_sig_pending.add(SIGKSIG)`（>64 → no-op）+ `mini_notify_core` 唤醒 **SM 自身**（源 = SYSTEM） |
| cause_sig SELF 路径 | system.c:416,433-434 | syscall_signal.rs:258-278 | ✅ 已实现: 目标自身 `s_sig_pending.add(sig_nr)` + `mini_notify_core` 唤醒目标自身（SIGKSIGSM=73 >64 位宽无法编码，唤醒即 C 的全部内核动作） |
| cause_sig 去重检查 | system.c:439-441 | syscall_signal.rs:285-286 | ✅ 已实现: 以 `RTS_SIGNALED` 判定 was_signaled（等价于 C 的 sigismember 门控——sigaddset 幂等）；未置 SIGNALED 时不再重复通知 |
| **cause_sig 致命 SELF 路径（backup 切换 / panic）** | system.c:417-432 | syscall_signal.rs:210-256（`is_lethal` 101-103） | ✅ **已实现（2026-09-05，todo D-11）**: `is_lethal` 判定 → 查 `s_bak_sig_mgr`；有效则提升（`s_sig_mgr`←backup、`s_bak_sig_mgr`←NONE、`RTS_UNSET(NO_PRIV)`）并**递归走外部路径**；无 backup → `panic!("cause_sig: sig manager …")`。差异：C 在 panic 前调 `proc_stacktrace(rp)`（system.c:429）打印进程用户栈，Rust 省略——完整栈展开需 `cross_space_copy` + `StacktraceArch::walk_frames`（DIAGCTL_CODE_STACKTRACE 路径已实现，syscall.rs:2295），在致命 panic 的 BKL 临界区引入跨空间读风险大于收益；panic 消息保留 endpoint + 信号号。测试：`test_cause_signal_self_lethal_promotes_backup` / `test_cause_signal_self_lethal_no_backup_panics` |
| dispatch_sigsend 完整流程 | do_sigsend.c:19-162 | syscall_signal.rs:516-689 | ✅ 已实现: data_copy_vmcheck + SignalContext |
| dispatch_sigreturn 完整流程 | do_sigreturn.c:19-95 | syscall_signal.rs:690-776 | ✅ 已实现: data_copy_vmcheck + SignalContext |
| sigframe 构建（架构相关） | do_sigsend.c:50-118 | arch/{x86_64,arm64,riscv64}/signal.rs | ✅ 已实现: SignalContext::build_sigframe |
| SignalContext x86_64 impl | do_sigsend.c:53-89 | arch/x86_64/signal.rs | ✅ 已实现 |
| SignalContext aarch64 impl | do_sigsend.c:91-110 | arch/arm64/signal.rs | ✅ 已实现 |
| SignalContext riscv64 impl | — | arch/riscv64/signal.rs | ✅ 已实现（C 源码未实现，按 RISC-V ELF psABI 独立设计） |
| sig_delay_done | system.c:454-464 | — | DEFERRED: 需 PM 通知接口 + SIGSNDELAY |
| DIAGCTL send_sig(PM_PROC_NR, SIGKMESS) | do_diagctl.c:49-56 | syscall.rs:2271-2315 | DEFERRED: DIAGCTL dispatch 已持借用，与 PM endpoint 全局查找冲突；PM 下次 getksig 轮询可观察到 |
| FPU 状态 save/restore（信号路径） | do_sigsend.c:84-88,156, do_sigreturn.c:85-93 | syscall_signal.rs:670（sigsend 清 MF_FPU_INITIALIZED）/ 766（sigreturn 不恢复） | ✅ 已对齐 C: 64-bit C 源码 `#if defined(__i386__)` gating → 64-bit 信号路径不 save/restore FPU；`KProcess.fpu_state` 由 SMP SAVE_CTX 使用，不参与信号路径（与 C 一致） |
| trap_style 校验 | do_sigsend.c:79-82 | arch/signal_context.rs `get_trap_style` | ✅ 已实现: `SignalContext::get_trap_style` + `arch_setcontext` |

### 4.8 redox 对比

| 维度 | redox | minix-rs | 选择理由 |
|------|-------|---------|---------|
| 信号数据结构 | `signal::SignalData` 聚合 handler/mask/pending | `p_pending: SigSet` + `s_sig_pending: SigSet` 拆分 | minix-rs 对齐 C 的 per-process + per-priv 拆分；redox 是 redesign |
| 信号处理器调用 | `context::signal_handler` 直接改 context 寄存器 | `SignalContext::setup_handler_entry` trait（已实现） | minix-rs 用 trait 抽象多架构；redox 单架构（x86_64 only） |
| 信号位图 | `sigset_t` 重定义（u64） | `SigSet(u64)` newtype | newtype 防止类型混淆 |
| 投递时序 | 单线程内核无 VMSUSPEND | 保留 C 的"拷贝后修改"约束 | minix-rs 对齐 C，SM 通过 VM proxy 访问用户空间 |
| 信号返回 | `sigreturn` 直接恢复 context | `SignalContext::restore_sigcontext` trait（已实现） | minix-rs 抽象为 trait |
| FPU 状态 | `context::fxsave` 内联 | `FpuArch` trait + `KProcess.fpu_state: CurrentFpuState`（SMP SAVE_CTX 使用；64-bit 信号路径不参与，对齐 C `#if defined(__i386__)` gating） | minix-rs 抽象为 trait 可跨架构 |
| 信号管理器 | 单一 PM（无多 SM 概念） | per-process `s_sig_mgr` + `s_bak_sig_mgr` | minix-rs 对齐 C 的多 SM 支持 |

### 4.9 no_std 与 BKL 约束

- `#![no_std]`：`syscall_signal.rs` 不依赖 `std`，仅用 `core`；测试模块 `#[cfg(test)]` 可用 `std`。
- BKL 保护：`p_pending` / `p_rts_flags` / `s_sig_pending` 是共享数据，由系统调用入口获取的 BKL 保护（参考 16-smp）。`cause_signal` 必须在 BKL 下调用，BKL 保证无其他 CPU 并发修改。
- BKL 临界区禁止睡眠 / 调度 / 等待 IPC / 等待锁（spinlock 持有者睡眠致他 CPU 死锁）；`cause_sig` 内的 `mini_notify` 实现必须非阻塞——当前 `crate::ipc::mini_notify_core` 仅置 `s_notify_pending` 位 + 唤醒 RECEIVE 状态，无睡眠/调度，符合约束。

---

## 5. 测试

### 5.1 现有测试（13 个，可 grep）

| 测试函数 | 行号（2026-09-05） | 验证行为 | 对应 C 符号 |
|---------|------|---------|------------|
| `test_sig_mask` | syscall_signal.rs:792 | sig_mask 信号编号到位掩码转换 | `sig_mask` |
| `test_nsig` | syscall_signal.rs:801 | NSIG=64 与 C `_NSIG` 对齐 | `_NSIG` |
| `test_signal_constants` | syscall_signal.rs:806 | SIGVTALRM/SIGPROF/SIGABRT/SIGTRAP 数值与 C 对齐 | `SIGVTALRM` 等 |
| `test_sigsend_invalid_endpoint` | syscall_signal.rs:814 | 无效 endpoint 返回 EINVAL | `do_sigsend.c:31` |
| `test_sigsend_kernel_process` | syscall_signal.rs:825 | 内核任务返回 EPERM（或 EINVAL，见注） | `do_sigsend.c:32` |
| `test_sigreturn_invalid_endpoint` | syscall_signal.rs:838 | 无效 endpoint 返回 EINVAL | `do_sigreturn.c:28` |
| `test_sigmsg_struct` | syscall_signal.rs:848 | SigMsg `#[repr(C)]` 字段顺序与 C `struct sigmsg` 对齐 | `struct sigmsg` |
| `test_is_lethal` | syscall_signal.rs:864 | 六致命信号 true / 非致命与内核信号 false | `SIGS_IS_LETHAL` |
| `test_cause_signal_external_path_notifies_manager` | syscall_signal.rs:924 | 外部路径：p_pending + RTS_SIGNALED + 通知管理器 | `cause_sig` 外部路径 |
| `test_cause_signal_dedup_does_not_notify_twice` | syscall_signal.rs:948 | SIGNALED 期间二次投递不重复通知 | `cause_sig` 去重（system.c:443） |
| `test_cause_signal_self_path_non_lethal` | syscall_signal.rs:978 | 自管理非致命：写自身 s_sig_pending，不设 RTS_SIGNALED | `cause_sig` SELF 路径 |
| `test_cause_signal_self_lethal_promotes_backup` | syscall_signal.rs:998 | 致命 SELF：backup 提升 + NO_PRIV 解除 + 递归外部投递 | `cause_sig` 致命路径 |
| `test_cause_signal_self_lethal_no_backup_panics` | syscall_signal.rs:1030 | 致命 SELF + 无 backup → panic | `cause_sig` 致命路径 |

> 注：`test_sigsend_kernel_process` 当前因 `endpoint_to_nr` 找不到内核进程返回 EINVAL（非 EPERM），因测试用空 ProcessTable 无内核任务槽；语义上 iskerneln 应返回 EPERM，待 ProcessTable 测试基建完善后细化。

**grep 验证**:

```bash
rg "fn test_" os/kernel/src/syscall_signal.rs --type rust -n
# → 792: fn test_sig_mask
# → 801: fn test_nsig
# → 806: fn test_signal_constants
# → 814: fn test_sigsend_invalid_endpoint
# → 825: fn test_sigsend_kernel_process
# → 838: fn test_sigreturn_invalid_endpoint
# → 848: fn test_sigmsg_struct
# → 864: fn test_is_lethal
# → 924: fn test_cause_signal_external_path_notifies_manager
# → 948: fn test_cause_signal_dedup_does_not_notify_twice
# → 978: fn test_cause_signal_self_path_non_lethal
# → 998: fn test_cause_signal_self_lethal_promotes_backup
# → 1030: fn test_cause_signal_self_lethal_no_backup_panics
```

### 5.2 待补充测试

| 测试函数 | 验证行为 | 依赖 |
|---------|---------|------|
| `test_getksig_finds_signaled` | GETKSIG 扫描找到 RTS_SIGNALED 进程 | dispatch_getksig（已实现，待测试基建） |
| `test_endksig_clears_pending` | ENDKSIG 无新信号时清除 SIG_PENDING | dispatch_endksig（已实现） |
| `test_endksig_keeps_pending` | ENDKSIG 有新信号时保留 SIG_PENDING | dispatch_endksig（已实现） |
| `test_sigsend_vmsuspend_on_page_fault` | SIGSEND sigmsg 拷贝触发 VmSuspend + RTS_VMREQUEST | MockPteWalk 返回 None（已具备） |
| `test_sigreturn_vmsuspend_on_page_fault` | SIGRETURN sigcontext 拷贝触发 VmSuspend | MockPteWalk 返回 None（已具备） |

---

## 6. 参见

- [11-scheduling-primitives.md](11-scheduling-primitives.md) — `RTS_SIGNALED` / `RTS_SIG_PENDING` 定义与调度影响
- [14-exception-interrupt.md](14-exception-interrupt.md) — `exception_handler` 调用 `cause_sig`（CPU 异常 → 信号投递）
- [15-clock-timer.md](15-clock-timer.md) — `vtimer_check` 发送 SIGVTALRM / SIGPROF
- [16-smp.md](16-smp.md) — BKL 保护共享数据（`p_pending` / `s_sig_pending`）；FpuArch trait（FPU save/restore 依赖）
- [17-syscall-process.md](17-syscall-process.md) — `do_exit` 向自身发 SIGABRT；fork 时 `p_pending` 清空（不继承信号）
- [22-privilege.md](22-privilege.md) — `KPriv.s_sig_mgr` / `s_bak_sig_mgr` / `s_sig_pending` 字段定义与权限模型
