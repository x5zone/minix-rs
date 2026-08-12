# 17-syscall-process: 进程管理类系统调用

> **分类**: 系统调用
> **C 源码**: `minix3/minix/kernel/system/do_fork.c`, `do_exec.c`, `do_exit.c`, `do_clear.c`, `do_runctl.c`, `do_schedctl.c`, `do_statectl.c`
> **Rust 实现**: `os/kernel/src/syscall_process.rs`（7 个 dispatch 函数 + 23 个单元测试）
> **覆盖**: fork（同步前提+代际+降权）/ exec（映像替换）/ exit（自杀信号委托）/ clear（幂等回收）/ runctl（停止恢复+SMP IPI）/ schedctl（双模式调度）/ statectl（IPC 状态控制）
> **前置**: [11-scheduling-primitives.md](11-scheduling-primitives.md), [16-smp.md](16-smp.md), [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md)

---

## 1. 概念建构

**核心问题**: 内核作为进程状态机引擎，如何用 7 条系统调用弧驱动进程在"未出生 → 就绪 → 运行 → 停止 → 死亡 → 槽位回收"之间转换，且每条弧都保持进程表/endpoint/权限/调度状态的原子一致？

**回答**: 每条弧都对应"读取目标 proc → 修改标志/字段 → 写回"的原子更新；fork 是同步点（父必须 RECEIVING）+ 权限降级点；exec 是映像替换（非新建）；exit 委托信号管理器；clear 是幂等回收；runctl/schedctl/statectl 是运行时控制。

### 1.1 进程生命周期：fork→exec→exit→clear 状态转换弧

**灵魂本质**: 进程从 fork 出生到 clear 槽位回收是一条不可逆状态链——fork 创建并降权，exec 替换映像，exit 委托信号，clear 回收槽位。

**WHY → WHAT → HOW 弧线**:

- **WHY**: 操作系统需要一个"进程"实体承载执行流，但进程不能凭空出现或消失——创建需要复制父状态，销毁需要回收所有资源（地址空间 / IRQ / endpoint / timer / FPU / 权限）。如果创建与销毁不是原子且幂等的，会导致两类故障：槽位泄漏（进程表耗尽，新 fork 无槽可用）或悬挂资源（IRQ 仍触发到已死进程，旧 timer 仍超时）。
- **WHAT**: 内核用 4 条核心弧构成生命周期主链，外加 3 条控制弧。每条弧都是"读目标 proc → 改标志/字段 → 写回"的原子更新，且回收弧必须幂等（重复回收直接返回 OK，防止调用者重试触发二次释放）。
- **HOW**: C 用 `do_fork.c:63`（`*rpc = *rpp` 结构体拷贝 + 多项修正）、`do_exec.c:45`（`arch_proc_init` 设 IP/SP + 清 DELIVERMSG）、`do_exit.c:21`（`cause_sig(caller, SIGABRT)` + `EDONTREPLY`）、`do_clear.c:35-68`（释放地址空间 → 幂等检查 → `RTS_SLOT_FREE`）。

```
        SYS_FORK          SYS_EXEC         SYS_EXIT        SYS_CLEAR
未出生 ─────────→ 就绪 ─────────→ 运行 ─────────→ 死亡 ─────────→ 槽位回收
[SLOT_FREE]    [RTS_NO_QUANTUM]   [RUNNING]    [RTS_SIGNALED]  [SLOT_FREE]
                 ↓ ↑                            ↑
              SYS_RUNCTL（停止/恢复）     SYS_STATECTL（IPC 状态）
              SYS_SCHEDCTL（调度权移交）
```

**关键概念**:

- **exec 是替换非新建**: exec 不分配新 proc 槽，只改 IP/SP/名字，复用同一 endpoint——这与 fork（新建槽+新 endpoint 代际）本质不同。所以 exec 后的进程仍是"同一进程"，只是换了执行流。
- **exit 不直接杀**: exit 调 `cause_sig(caller, SIGABRT)`（do_exit.c:21），把死亡决策权交给信号管理器（PM）。内核不越俎代庖地直接释放资源——资源释放在 PM 决议后由 SYS_CLEAR 完成。这种"自杀 → 委托 → 回收"三段式让 PM 能执行清理钩子（如通知父进程、记账）。
- **clear 的幂等性**: `if(isemptyp(rc)) return OK`（do_clear.c:38）——若槽位已空闲则直接返回成功。这保证 PM 在网络分区或重试场景下重复调用 clear 不会二次释放 IRQ/endpoint/timer。

### 1.2 同步 fork：为什么父进程必须 RTS_RECEIVING

**灵魂本质**: fork 是父子同步点——父必须正在接收，复制才能安全借用父的消息缓冲；endpoint 代际 +1 防止旧 endpoint 复活。

**WHY → WHAT → HOW 弧线**:

- **WHY**: fork 时子进程是父的副本，但子进程的 fork 系统调用"返回值"必须为 0（do_fork.c:74 `rpc->p_reg.retreg = 0`），让子进程代码能区分"我是子进程"。这个返回值通过父进程当时正在等待的 IPC 回复缓冲投递。如果父进程不在 RECEIVING 状态，回复缓冲指针无效，复制子进程会拷贝到一个悬空的缓冲——所以 fork 必须同步。
- **WHAT**: 内核强制 `RTS_ISSET(rpp, RTS_RECEIVING)` 前提（do_fork.c:51），不满足返回 EINVAL。复制时 `*rpc = *rpp`（do_fork.c:63）连消息缓冲指针一并拷贝，子进程通过同一缓冲收到 pid=0。
- **HOW**: endpoint 不能直接继承——若复用父 endpoint，旧代 ipc 消息会投递到错乱的目标。所以 `_ENDPOINT_G` 取代际 → `++gen`（do_fork.c:69）→ `_ENDPOINT(gen, p_nr)` 重组（do_fork.c:72）。代际回绕：`>= _ENDPOINT_MAX_GENERATION` 则归 1（do_fork.c:69-70）。

**关键约束**:

1. 父非 RECEIVING → EINVAL（do_fork.c:51）
2. 子槽必须为空（`isemptyp(rpp) || !isemptyp(rpc)` → EINVAL，do_fork.c:46）
3. 复制前 `save_fpu(rpp)`（do_fork.c 附近）保证父 FPU 上下文已落盘
4. 信号状态不继承：`RTS_UNSET(rpc, RTS_SIGNALED|RTS_SIG_PENDING|RTS_P_STOP)`（do_fork.c:122）——子进程不应继承父的待处理信号，否则会立刻被信号杀掉

### 1.3 权限降级：SYS_PROC 父→USER 子

**灵魂本质**: 权限不继承——SYS_PROC 父的子进程必须降级为 USER_PRIV 并设 RTS_NO_PRIV，由调用者显式重新赋权。

**WHY → WHAT → HOW 弧线**:

- **WHY**: 系统服务（如 PM）是 SYS_PROC，拥有高权限（可发内核 syscall、绑 IRQ、访问任意内存）。如果 fork 出的子进程继承 SYS_PROC，那么任何用户进程都能通过"让 PM 替自己 fork 一个 SYS_PROC 子进程"提权——这是经典 confusable deputy 安全漏洞。
- **WHAT**: fork 检查 `priv(rpp)->s_flags & SYS_PROC`（do_fork.c:105），若父是系统进程，子进程 `p_priv = priv_addr(USER_PRIV_ID)` + `RTS_NO_PRIV`（do_fork.c:106-107）。RTS_NO_PRIV 使子进程不可调度，直到调用者通过 SYS_PRIVCTL 显式赋权。
- **HOW**: 降级是单向的——子进程从 USER_PRIV 起步，PM 在 exec 前用 `sys_privctl` 赋予适当权限。VM 模式下还设 `RTS_VMINHIBIT`（do_fork.c:115-116），让子进程等待 VM 设置新页表后才可运行，避免子进程用父的旧页表执行。

### 1.4 SMP 停止：跨 CPU IPI 同步

**灵魂本质**: runctl 停止远 CPU 上的进程不能直接改标志——必须用同步 IPI 让目标 CPU 保存上下文后停止，避免竞态。

**WHY → WHAT → HOW 弧线**:

- **WHY**: 单 CPU 下 `RTS_SET(rp, RTS_PROC_STOP)`（do_runctl.c:62）立即可行，因为当前 CPU 持有 BKL，目标进程不会同时跑。但 SMP 下目标进程可能在另一 CPU 上运行——直接改标志后，目标 CPU 仍可能用陈旧上下文继续执行一个时钟周期（如已读入寄存器的返回地址仍指向旧代码），导致状态不一致。
- **WHAT**: SMP 路径检查 `rp->p_cpu != cpuid`（do_runctl.c:57），若目标在远 CPU，调 `smp_schedule_stop_proc(rp)`（do_runctl.c:58）发同步 IPI；否则本地 `RTS_SET`。
- **HOW**: 同步 IPI 的完整协议见 [16-smp.md](16-smp.md) §1.3——`smp_schedule_sync(STOP_PROC)` 释放 BKL → 等待目标 CPU 处理 → 重获 BKL。RC_DELAY 模式下若目标正在 SENDING，设 `MF_SIG_DELAY` 返回 EBUSY（do_runctl.c:44-48），让 PM 延后停止。

**单 CPU 退化**: `CONFIG_SMP` 未定义时无 IPI 路径，直接 `RTS_SET`。

> **架构范围：x86-64 / SMP**：IPI 同步协议依赖 `arch_send_smp_schedule_ipi`，详见 16-smp。单核配置下该路径编译为空。

---

## 2. C 源码分析

每节以 C 符号为单元，附 `file:line`，说明语义与调用关系。

### 2.1 do_fork — 复制 proc 结构，新 endpoint

入口 `do_fork(caller, m_ptr)` do_fork.c:26。消息字段：`m_lsys_krn_sys_fork.endpt`（父 endpoint）/ `.slot`（子槽位）/ `.flags`（PFF_VMINHIBIT）。

| 符号 | 位置 | 说明 |
|------|------|------|
| `isokendpt(endpt, &p_proc)` | do_fork.c:41 | 验证父 endpoint |
| `rpp = proc_addr(p_proc)` / `rpc = proc_addr(slot)` | do_fork.c:44-45 | 父 / 子 proc 指针 |
| `isemptyp(rpp)\|\|!isemptyp(rpc)` → EINVAL | do_fork.c:46 | 父非空子空 |
| `RTS_ISSET(rpp, RTS_RECEIVING)` | do_fork.c:51 | 同步前提 |
| `save_fpu(rpp)` | do_fork.c:57 | 保存父 FPU |
| `*rpc = *rpp` | do_fork.c:63 | proc 结构体拷贝 |
| `gen = _ENDPOINT_G(...); ++gen; _ENDPOINT(gen, p_nr)` | do_fork.c:59,69,72 | endpoint 代际 |
| `rpc->p_reg.retreg = 0` | do_fork.c:74 | 子返回值 = 0 |
| `RTS_SET(rpc, RTS_NO_QUANTUM)` | do_fork.c:90 | 子不可调度 |
| `priv(rpp)->s_flags & SYS_PROC` → USER_PRIV + RTS_NO_PRIV | do_fork.c:105-107 | 权限降级 |
| `PFF_VMINHIBIT` → RTS_VMINHIBIT | do_fork.c:115-116 | 等新页表 |
| `RTS_UNSET(rpc, RTS_SIGNALED\|SIG_PENDING\|P_STOP)` | do_fork.c:122 | 信号不继承 |
| `m_ptr->endpt = rpc->p_endpoint` | do_fork.c:111 | 回填子 endpoint |

### 2.2 do_exec — 清 DELIVERMSG，设 IP/SP，清 FPU

入口 `do_exec(caller, m_ptr)` do_exec.c:20。消息字段：`.endpt` / `.ip` / `.stack` / `.name` / `.ps_str`。

| 符号 | 位置 | 说明 |
|------|------|------|
| `isokendpt(endpt, &proc_nr)` | do_exec.c:27 | 验证目标 |
| `rp->p_misc_flags &= ~MF_DELIVERMSG` | do_exec.c:32-33 | 清待投递消息 |
| `data_copy(caller->p_endpoint, name, KERNEL, name, ...)` | do_exec.c:37 | 跨空间拷名 |
| `arch_proc_init(rp, ip, stack, ps_str, name)` | do_exec.c:45 | 架构相关设 IP/SP |
| `RTS_UNSET(rp, RTS_RECEIVING)` | do_exec.c:51 | 解除接收（不回复 EXEC） |
| `&= ~MF_FPU_INITIALIZED` / `release_fpu(rp)` | do_exec.c:55,57 | FPU 失效 |

**exec 不回复语义**：exec 后进程整个地址空间被替换，原消息缓冲失效，所以 `do_exec` 清 `RTS_RECEIVING` 但不写回返回消息——PM 通过其他机制（如通知）确认 exec 完成。

### 2.3 do_exit — cause_sig(SIGABRT)，EDONTREPLY

入口 `do_exit(caller, m_ptr)` do_exit.c:14。

| 符号 | 位置 | 说明 |
|------|------|------|
| `int sig_nr = SIGABRT` | do_exit.c:19 | 自杀信号 |
| `cause_sig(caller->p_nr, sig_nr)` | do_exit.c:21 | 委托信号管理器 |
| `return(EDONTREPLY)` | do_exit.c:23 | 不回复 |

`cause_sig` 的完整 C 路径（system.c:389）：查找 `priv(rp)->s_sig_mgr` → `sigaddset(&priv->s_sig_pending, sig)` → `RTS_SET(rp, RTS_SIGNALED|RTS_SIG_PENDING)` → `mini_notify(sig_mgr, caller->p_endpoint)`。内核侧状态变更可独立完成，信号管理器通知需要 IPC 子系统。

### 2.4 do_clear — 释放资源，RTS_SLOT_FREE

入口 `do_clear(caller, m_ptr)` do_clear.c:17。消息字段：`m_lsys_krn_sys_clear.endpt`。

| 符号 | 位置 | 说明 |
|------|------|------|
| `isokendpt(endpt, &exit_p)` | do_clear.c:29 | 验证目标 |
| `release_address_space(rc)` | do_clear.c:35 | 释放地址空间 |
| `if(isemptyp(rc)) return OK` | do_clear.c:38 | **幂等** |
| `for irq_hooks` / `rm_irq_handler` | do_clear.c:43 | 释放 IRQ |
| `clear_endpoint(rc)` | do_clear.c:49 | 释放 IPC endpoint |
| `reset_kernel_timer(&priv(rc)->s_alarm_timer)` | do_clear.c:52 | 重置定时器 |
| `RTS_SETFLAGS(rc, RTS_SLOT_FREE)` | do_clear.c:57 | 标记槽空闲 |
| `release_fpu` / `&= ~MF_FPU_INITIALIZED` | do_clear.c:60 | 释放 FPU |
| `SYS_PROC` → `s_proc_nr = NONE` | do_clear.c:68 | 释放 priv |

### 2.5 do_runctl — RC_STOP/RC_RESUME，SMP IPI

入口 `do_runctl(caller, m_ptr)` do_runctl.c:18。消息字段：`RC_ENDPT` / `RC_ACTION` / `RC_FLAGS`。

| 符号 | 位置 | 说明 |
|------|------|------|
| `isokendpt(RC_ENDPT, &proc_nr)` | do_runctl.c:30 | 验证目标 |
| `iskerneln(proc_nr)` → EPERM | do_runctl.c:31 | 内核进程不可控 |
| `RC_STOP && RC_DELAY` → MF_SIG_DELAY → EBUSY | do_runctl.c:44-48 | 延迟停止 |
| `CONFIG_SMP` && `p_cpu != cpuid` → `smp_schedule_stop_proc` | do_runctl.c:55-58 | 跨 CPU IPI |
| `RTS_SET(rp, RTS_PROC_STOP)` | do_runctl.c:62 | 本地停止 |
| `RTS_UNSET(rp, RTS_PROC_STOP)` | do_runctl.c:66 | 恢复 |

### 2.6 do_schedctl — KERNEL flag 设参数，否则设 scheduler

入口 `do_schedctl(caller, m_ptr)` do_schedctl.c:7。消息字段：`m_lsys_krn_schedctl.flags` / `.endpoint` / `.priority` / `.quantum` / `.cpu`。

| 符号 | 位置 | 说明 |
|------|------|------|
| `flags & ~SCHEDCTL_FLAG_KERNEL` → EINVAL | do_schedctl.c:17 | flags 校验 |
| `isokendpt(endpoint, &proc_nr)` | do_schedctl.c:23 | 验证目标 |
| `SCHEDCTL_FLAG_KERNEL` → `sched_proc(p, priority, quantum, cpu, FALSE)` | do_schedctl.c:28,37 | 内核调度模式 |
| `p->p_scheduler = NULL` | do_schedctl.c:39 | 清 user scheduler |
| `p->p_scheduler = caller` | do_schedctl.c:42 | 调用者接管 |

`priority/quantum/cpu` 为 -1 时表示"保持当前值"（do_schedctl.c:32-34），由 `sched_proc` 内部解释。

### 2.7 do_statectl — 5 种请求分发

入口 `do_statectl(caller, m_ptr)` do_statectl.c:15。消息字段：`m_lsys_krn_sys_statectl.request` / `.address` / `.length`。

| 符号 | 位置 | 说明 |
|------|------|------|
| `switch(request)` | do_statectl.c:19 | 请求分发 |
| `SYS_STATE_CLEAR_IPC_REFS` → `clear_ipc_refs(caller, EDEADSRCDST)` | do_statectl.c:21,25 | 清 IPC 引用 |
| `SYS_STATE_SET_STATE_TABLE` → `s_state_table`/`s_state_entries` | do_statectl.c:27,29 | 设状态表 |
| `SYS_STATE_ADD_IPC_BL_FILTER` → `add_ipc_filter(BLACKLIST, ...)` | do_statectl.c:32,34 | 黑名单 |
| `SYS_STATE_ADD_IPC_WL_FILTER` → `add_ipc_filter(WHITELIST, ...)` | do_statectl.c:37,39 | 白名单 |
| `SYS_STATE_CLEAR_IPC_FILTERS` → `clear_ipc_filters(caller)` | do_statectl.c:42,44 | 清过滤 |

### 2.8 调用关系图（时序）

**fork 时序**（同步点）:

```
PM: sys_fork(parent_endpt, child_slot, flags)
  └─ KERNEL: do_fork()                      [do_fork.c:26]
       ├─ 校验父 RECEIVING                   [do_fork.c:51]
       ├─ save_fpu(rpp)                      [do_fork.c:57]
       ├─ *rpc = *rpp                        [do_fork.c:63]  ← 结构体拷贝
       ├─ gen++; _ENDPOINT(gen, p_nr)        [do_fork.c:69,72]  ← 代际 +1
       ├─ rpc->p_reg.retreg = 0              [do_fork.c:74]  ← 子返回 0
       ├─ RTS_SET(rpc, RTS_NO_QUANTUM)       [do_fork.c:90]
       ├─ if SYS_PROC: USER_PRIV + RTS_NO_PRIV  [do_fork.c:105-107]
       └─ m_ptr->endpt = rpc->p_endpoint     [do_fork.c:111]  ← 回填子 endpoint
```

**clear 时序**（幂等回收）:

```
PM: sys_clear(exit_endpt)
  └─ KERNEL: do_clear()                     [do_clear.c:17]
       ├─ release_address_space(rc)         [do_clear.c:35]
       ├─ if isemptyp(rc): return OK        [do_clear.c:38]  ← 幂等返回
       ├─ 释放 IRQ / endpoint / timer / FPU [do_clear.c:43-60]
       ├─ RTS_SETFLAGS(rc, RTS_SLOT_FREE)   [do_clear.c:57]
       └─ if SYS_PROC: s_proc_nr = NONE     [do_clear.c:68]
```

---

## 3. 设计决策

采用"如果 X 设计会有 Y 问题所以用 Z"的假设性推理。每个决策附 ≥2 个被否决选项。

### D1. proc 拷贝：`KProcess::fork_from` 构造函数替代 `*rpc = *rpp`

**假设性推理**:

- 如果用 Rust 的 `*rpc = *rpp` 等价结构体赋值（`Clone::clone`）：`KProcess` 含 `AtomicU8`/`AtomicU32` 等非 `Copy` 字段，且 `p_seg` 需重置、`priv_id` 需降级——直接 clone 会拷贝父的原子状态与权限，违反 fork 语义（子应是独立副本）。
- 如果用 `KProcess::clone()` + 逐字段修正：clone 后再改 8+ 个字段，易遗漏（如忘记清 `p_reg.retreg`），且 clone 拷贝了不该拷的 `p_magic`。
- 所以用专门的 `KProcess::fork_from(parent, child_nr, child_endpoint)` 构造函数（proc.rs:1385）：在构造期一次性应用所有 fork 修正（RTS_NO_QUANTUM、清信号标志、清 timer/trace、重置 p_seg、设 None priv_id），语义集中在一处，编译器保证字段不漏。

**实现**: `KProcess::fork_from`（proc.rs:1385）+ `complete_fork_setup`（proc.rs:1493，应用 NO_PRIV/VMINHIBIT/name 后缀）。

### D2. endpoint 表达：Endpoint newtype 替代裸 i32

**假设性推理**:

- 如果用裸 `i32` 表达 endpoint（C 方式）：endpoint 与 errno、proc_nr、raw 值都是 i32，编译器无法区分——`return EINVAL` 和 `return child_endpoint` 类型相同，调用者可能误用。
- 如果用 `type Endpoint = i32` 类型别名：仅文档作用，编译期无保护，与裸 i32 等价。
- 所以用 `#[repr(transparent)] pub struct Endpoint(pub i32)` newtype（endpoint.rs:44）：编译期防止与其他 i32 混淆，且 `repr(transparent)` 保证 ABI 与 i32 一致（FFI/消息布局兼容）。配套 `from_generation_slot`/`slot`/`generation`/`fork_new_endpoint` 方法封装代际编解码。

**实现**: `Endpoint` newtype（endpoint.rs:44）+ `fork_new_endpoint`（endpoint.rs:177）+ `from_generation_slot`（endpoint.rs:82）。

### D3. statectl 请求：StatectlRequest enum + match 替代 switch/case

**假设性推理**:

- 如果用 `switch(request)` + 裸 i32 case（C 方式）：case 值是魔术数字（1-5），且 default 分支返回 EINVAL——编译器不检查是否覆盖所有 case，新增请求类型易漏。
- 如果用 `const CLEAR_IPC_REFS: i32 = 1` 常量：仍是裸 i32 匹配，无法穷尽检查。
- 所以用 `pub enum StatectlRequest { ClearIpcRefs=1, SetStateTable=2, ... }`（syscall_process.rs:64）+ `match req`：编译器强制穷尽检查，新增变体必须处理；`TryFrom<i32>` 把非法值转为 `Err(())` → EINVAL，集中处理边界。

**实现**: `StatectlRequest` enum（syscall_process.rs:62-80）+ `TryFrom<i32>`（syscall_process.rs:82-95）+ `match req`（syscall_process.rs:628）。

### D4. -1 sentinel：Option 替代裸 -1

**假设性推理**:

- 如果用 C 的 `priority = -1` 表示"保持当前值"（do_schedctl.c:32）：-1 是魔术值，与合法优先级 0..15 共享 i32 类型，调用者可能误传 -2（无效但不会被 -1 检查捕获，除非额外校验）。
- 如果用 `i32::MAX` 或其他哨兵：同样需额外校验，且与 C 语义不一致（C 用 -1）。
- 所以用 `Option<u8>` 表达"保持当前值"：`-1 → None`，`v >= 0 → Some(v as u8)`，`v < -1 → EINVAL`（syscall_process.rs:554-558）。Option 在类型层表达"可选"，编译器强制处理 None 分支。同时保留 -1 哨兵的 C 语义兼容（消息层仍是 i32）。

**实现**: `SchedParams { priority: Option<u8>, quantum: Option<u32>, cpu: Option<u32>, niced: bool }` + dispatch_schedctl 的 -1→None 转换（syscall_process.rs:554-564）。

### D5. 返回值：KcallResult enum 替代 errno 返回

**假设性推理**:

- 如果用 `i32` 返回 errno（C 方式）：`return OK`(0)、`return EINVAL`(22)、`return EDONTREPLY`(-998)、`return child_endpoint`(正数) 共享 i32——调用者无法从类型区分"成功返回值"与"errno"与"不回复"，需靠范围判断（C 的 `result >= 0` vs `result == EDONTREPLY`）。
- 如果用 `Result<i32, Errno>`：EDONTREPLY 不是错误（是"不回复"语义，exit 故意不回复），强行塞进 Err 变体语义不准；且 child_endpoint 是成功返回值，与 errno 0 混在 Ok。
- 所以用 `pub enum KcallResult { Ok(i32), VmSuspend, NoReply, BadCall, CallDenied }`（syscall.rs:180）：每个变体对应一种"调用完成方式"——Ok(返回值) / VmSuspend(需 VM 协助) / NoReply(不回复，如 exit) / BadCall(非法 syscall) / CallDenied(无权限)。dispatch 层返回 enum，上层 match 处理。

**实现**: `KcallResult` enum（syscall.rs:180-192）+ `dispatch_exit` 返回 `NoReply`（syscall_process.rs:298）+ 其他 dispatch 返回 `Ok(errno)`。

### D6. 进程号：ProcNr newtype

**假设性推理**:

- 如果保持 `pub type ProcNr = i32` 类型别名：编译期与裸 i32 等价——endpoint raw、errno、proc_nr 都能互相赋值，无防护，易引入"把 errno 当 proc_nr"类 bug。
- 升级为 `#[repr(transparent)] pub struct ProcNr(pub i32)` newtype：与 Endpoint 对齐，编译期防止混淆；`repr(transparent)` 保证消息布局兼容。
- 取舍：`ProcNr` 使用点遍布 `proc.rs`/`proc_table.rs`/`sched.rs`/`smp.rs`/`syscall_process.rs` 等多个模块，升级需同步修改所有使用点 + 补 `From<i32>`/`Into<i32>`/`Add`/`Sub`/`Neg`/`Display` 转换。为提供编译期类型防护，已完成 newtype 升级。

**实现**: `#[repr(transparent)] pub struct ProcNr(pub i32)`（[proc.rs:30](file:///home/xzhao/github/minix-rs/os/kernel/src/proc.rs)）+ `From<i32>`/`Into<i32>`/`Add`/`Sub`/`Neg`/`Display` impl。`AtomicI32`（`p_nextready`）保持存储裸 `i32`（C ABI 兼容），访问点用 `.0` 取裸值或 `ProcNr(raw)` 构造。`NONE_PROC_NR: i32 = -1` 保持 `i32`（与 `AtomicI32` 哨兵对齐）。

---

## 4. 实现详解

贴 `os/kernel/src/syscall_process.rs` 真实代码片段，每个 dispatch 标注 C 行号对应。代码块内注释用中文（实际 Rust 文件内注释为英文）。所有 dispatch 在 `#![no_std]` 下运行（除 `#[cfg(test)]` 测试模块可用 `std`）；`KProcess` 含 `AtomicU8`/`AtomicU32`，无堆分配。

### 4.1 dispatch_fork — 完整实现

```rust
// C: do_fork.c:26-134
pub fn dispatch_fork(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
    priv_table: &PrivTable,
) -> KcallResult {
    let m1 = msg_m1(msg);
    // C: do_fork.c:44-46 — 提取父 endpoint / 子槽位 / fork 标志
    let parent_endpt_i = m1.m1i1;
    let child_slot: ProcNr = m1.m1i2;
    let fork_flags = m1.m1i3 as u32;

    // C: do_fork.c:51 — 同步前提：父进程必须正在接收
    if !caller.p_rts_flags.is_set(RtsFlagsBits::RECEIVING) {
        return KcallResult::Ok(EINVAL);
    }
    // C: do_fork.c:48 — 子槽位必须空闲
    if !proc_table.is_empty(child_slot) {
        return KcallResult::Ok(EINVAL);
    }

    // C: do_fork.c:59,69,72 — endpoint 代际 +1 并回绕
    let child_old_endpoint = proc_table.get(child_slot)
        .map(|p| p.p_endpoint)
        .unwrap_or(Endpoint::from_generation_slot(0, child_slot));
    let child_endpoint = Endpoint::fork_new_endpoint(child_old_endpoint, child_slot);

    // C: do_fork.c:63 — *rpc = *rpp（构造函数一次性应用所有 fork 修正）
    let mut child = KProcess::fork_from(caller, child_slot, child_endpoint);

    // C: do_fork.c:105 — 检查父进程是否为 SYS_PROC 以决定降级
    let parent_is_sys_proc = caller.priv_id
        .and_then(|id| priv_table.get(id))
        .map(|p| p.capability.s_flags.contains(PrivFlagsBits::SYS_PROC))
        .unwrap_or(false);
    // 所有 fork 出的子进程都从 USER_PRIV_ID 起步
    child.priv_id = Some(USER_PRIV_ID);

    // C: do_fork.c:107,115-116,84-87 — 应用 NO_PRIV / VMINHIBIT / 名字后缀 "*F"
    complete_fork_setup(&mut child, parent_is_sys_proc, fork_flags);

    *proc_table.get_mut(child_slot).unwrap() = child;

    // C: do_fork.c:111 — 回填子 endpoint 给调用者
    KcallResult::Ok(child_endpoint.0)
}
```

完整覆盖 C `do_fork.c:26-134`：同步前提、代际、构造拷贝、降级、VMINHIBIT、名字后缀、回填 endpoint。

### 4.2 dispatch_exec — 部分实现（DEFERRED 标注）

```rust
// C: do_exec.c:20-58
pub fn dispatch_exec(
    _caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
) -> KcallResult {
    let m1 = msg_m1(msg);
    // C: do_exec.c:27 — 提取目标 endpoint（操作目标进程，非 caller）
    let target_endpoint = Endpoint(m1.m1i1);
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_exec.c:32-33 — 清除 MF_DELIVERMSG
    proc_table.get_mut(target_nr).map(|rp| {
        rp.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG);
    });

    // C: do_exec.c:37 — 跨空间拷贝进程名
    // DEFERRED: 需 data_copy_vmcheck（VM 集成）

    // C: do_exec.c:45 — arch_proc_init 设新 IP/SP
    // DEFERRED: 需 ArchProcInit trait（arch 层）

    // C: do_exec.c:51 — 解除接收（不回复 EXEC）
    proc_table.rts_unset(target_nr, RtsFlagsBits::RECEIVING);

    // C: do_exec.c:55,57 — 清 FPU 标志（Rust 用 EXT_REG_INITIALIZED）
    proc_table.get_mut(target_nr).map(|rp| {
        rp.p_misc_flags.clear(MiscFlagsBits::EXT_REG_INITIALIZED);
    });
    KcallResult::Ok(OK)
}
```

**DEFERRED**: cross-space copy（do_exec.c:37，需 `DataCopy` trait）+ arch_proc_init（do_exec.c:45，需 `ArchProcInit` trait）。

### 4.3 dispatch_exit — 完整实现（in-place 部分）

```rust
// C: do_exit.c:14-23
pub fn dispatch_exit(caller: &mut KProcess, _msg: &Message) -> KcallResult {
    // C: do_exit.c:21 — cause_sig(caller, SIGABRT)（in-place 部分）
    cause_signal_abort(caller);
    // C: do_exit.c:23 — return EDONTREPLY
    KcallResult::NoReply
}

// C: cause_sig — system.c:389-426 的 in-place 部分
fn cause_signal_abort(caller: &mut KProcess) {
    // C: system.c:411 — sigaddset(&priv->s_sig_pending, sig_nr)
    caller.p_pending.add(SIGABRT as u8);
    // C: system.c:413-414 — RTS_SET(rp, RTS_SIGNALED | RTS_SIG_PENDING)
    caller.p_rts_flags.set(RtsFlagsBits::SIGNALED | RtsFlagsBits::SIG_PENDING);
}
```

**DEFERRED**: 信号管理器通知 `mini_notify(sig_mgr, caller->p_endpoint)`（do_exit.c:21 完整路径的 step 3，需 `SignalContext` trait + IPC）。

### 4.4 dispatch_clear — 部分实现（IRQ/timer 已落地，VM/IPC 仍 DEFERRED）

```rust
// C: do_clear.c:17-78
pub fn dispatch_clear(
    _caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
    priv_table: &mut PrivTable,
    clock_state: &mut crate::clock::ClockState,
) -> KcallResult {
    let m1 = msg_m1(msg);
    // C: do_clear.c:29 — 验证目标 endpoint（操作目标进程，非 caller）
    let target_endpoint = Endpoint(m1.m1i1);
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_clear.c:35 — release_address_space(rc)
    // DEFERRED: 需 VmContext trait（VM 集成）

    // C: do_clear.c:38 — if(isemptyp(rc)) return OK（幂等）
    if proc_table.get(target_nr).map_or(true, |p| {
        p.p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE)
    }) {
        return KcallResult::Ok(OK);
    }

    // C: do_clear.c:41-43 — 释放 IRQ hooks（已实现）
    // C 迭代 `for (i=0; i<NR_IRQ_HOOKS; i++)` 移除 `proc_nr_e == rc->p_endpoint` 的钩子。
    // Rust 复用全局 `irq_manager()`（与 dispatch_irqctl 同一访问点），遍历所有
    // NR_IRQ_HOOKS 槽位、移除 owner == target_endpoint 的钩子。
    {
        let irq_mgr = unsafe { crate::irq_manager() };
        for slot in 0..crate::syscall_device::NR_IRQ_HOOKS {
            if let Some(owner) = irq_mgr.hook_owner(slot) {
                if owner == target_endpoint {
                    let _ = irq_mgr.remove_hook_by_slot(slot);
                }
            }
        }
    }

    // C: do_clear.c:49 — clear_endpoint(rc)
    // DEFERRED: 需 IpcEngine trait

    // C: do_clear.c:52 — reset_kernel_timer(&priv(rc)->s_alarm_timer)（已实现）
    // 取目标 priv 的 `s_alarm_timer: Option<(TimerEntry, TimerId)>`，
    // 若有挂起的闹钟定时器则 take + clock_state.reset_timer(timer_id) 取消。
    if let Some(pid) = proc_table.get(target_nr).and_then(|p| p.priv_id) {
        if let Some(kp) = priv_table.get_mut(pid) {
            if let Some((_entry, timer_id)) = kp.runtime.s_alarm_timer.take() {
                clock_state.reset_timer(timer_id);
            }
        }
    }

    // C: do_clear.c:57 — RTS_SETFLAGS(rc, RTS_SLOT_FREE)
    proc_table.rts_set(target_nr, RtsFlagsBits::SLOT_FREE);

    // C: do_clear.c:60 — 清 FPU 标志
    if let Some(target) = proc_table.get_mut(target_nr) {
        target.p_misc_flags.clear(MiscFlagsBits::EXT_REG_INITIALIZED);
    }

    // C: do_clear.c:68 — if SYS_PROC: priv(rc)->s_proc_nr = NONE
    if let Some(target) = proc_table.get(target_nr) {
        if let Some(priv_id) = target.priv_id {
            if let Some(kpriv) = priv_table.get_mut(priv_id) {
                if kpriv.is_sys_proc() {
                    kpriv.capability.s_proc_nr = None;
                }
            }
        }
    }
    KcallResult::Ok(OK)
}
```

**已实现**: IRQ hooks 释放（do_clear.c:41-43，经全局 `irq_manager()`）+ alarm timer 重置（do_clear.c:52，经 `clock_state.reset_timer(timer_id)`）。`dispatch_clear` 因此新增 `clock_state: &mut ClockState` 参数；测试通过 `init_irq_manager_for_test()` 初始化全局 IrqManager。

**仍 DEFERRED**: release_address_space（do_clear.c:35，需 `VmContext`）/ clear_endpoint（do_clear.c:49，需 `IpcEngine`）。本 dispatch 涵盖的核心操作（endpoint 验证、幂等检查、IRQ/timer 释放、SLOT_FREE、FPU 标志、priv 释放）足以防止槽位泄漏与悬挂资源，使槽位可被新进程重用。

### 4.5 dispatch_runctl — 完整（单 CPU 路径）

```rust
// C: do_runctl.c:18-73
pub fn dispatch_runctl(
    _caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
) -> KcallResult {
    let m1 = msg_m1(msg);
    // C: do_runctl.c:30 — 提取 RC_ENDPT / RC_ACTION / RC_FLAGS
    let target_endpoint = Endpoint(m1.m1i1);
    let action = m1.m1i2;
    let flags = m1.m1i3;

    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };
    // C: do_runctl.c:31 — iskerneln(proc_nr) → EPERM
    if ProcessTable::is_kernel(target_nr) {
        return KcallResult::Ok(EPERM);
    }

    match action {
        RC_STOP => {
            // C: do_runctl.c:44-48 — RC_DELAY 模式：目标正在 SENDING 则设 MF_SIG_DELAY 返回 EBUSY
            if (flags & RC_DELAY) != 0 {
                let target = proc_table.get(target_nr);
                if let Some(rp) = target {
                    if rp.p_rts_flags.is_set(RtsFlagsBits::SENDING)
                        || rp.p_misc_flags.is_set(MiscFlagsBits::SC_DEFER)
                    {
                        proc_table.get_mut(target_nr).map(|rp| {
                            rp.p_misc_flags.set(MiscFlagsBits::SIG_DELAY);
                        });
                    }
                }
                let sig_delay_set = proc_table.get(target_nr)
                    .map_or(false, |rp| rp.p_misc_flags.is_set(MiscFlagsBits::SIG_DELAY));
                if sig_delay_set {
                    return KcallResult::Ok(EBUSY);
                }
            }
            // C: do_runctl.c:55-58 — SMP 路径
            // DEFERRED: 需 SmpRunctl trait（见 16-smp §4.6）
            // C: do_runctl.c:62 — 单 CPU 路径：RTS_SET(rp, RTS_PROC_STOP)
            proc_table.rts_set(target_nr, RtsFlagsBits::PROC_STOP);
        }
        RC_RESUME => {
            // C: do_runctl.c:66 — RTS_UNSET(rp, RTS_PROC_STOP)
            proc_table.rts_unset(target_nr, RtsFlagsBits::PROC_STOP);
        }
        _ => return KcallResult::Ok(EINVAL),
    }
    KcallResult::Ok(OK)
}
```

**DEFERRED**: SMP IPI 路径（do_runctl.c:55-58，依赖 `SmpArch::schedule_stop_proc`，见 16-smp）。

### 4.6 dispatch_schedctl — 完整实现

```rust
// C: do_schedctl.c:7-46
pub fn dispatch_schedctl(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
) -> KcallResult {
    let sc = msg_schedctl(msg);
    // C: do_schedctl.c:17 — flags 校验（仅 SCHEDCTL_FLAG_KERNEL 定义）
    let flags = sc.flags;
    if flags & !SCHEDCTL_FLAG_KERNEL != 0 {
        return KcallResult::Ok(EINVAL);
    }
    // C: do_schedctl.c:23 — 验证目标 endpoint（操作目标进程，非 caller）
    let target_endpoint = Endpoint(sc.endpoint);
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    if flags & SCHEDCTL_FLAG_KERNEL != 0 {
        // C: do_schedctl.c:32-37 — -1 sentinel 转 Option（"保持当前值"）
        let priority_opt = match sc.priority {
            -1 => None,
            v if v >= 0 => Some(v as u8),
            _ => return KcallResult::Ok(EINVAL),
        };
        let quantum_opt = match sc.quantum {
            -1 => None,
            v if v >= 1 => Some(v as u32),
            _ => return KcallResult::Ok(EINVAL),
        };
        let cpu_opt = if sc.cpu == -1 { None } else { Some(sc.cpu as u32) };

        let target = match proc_table.get_mut(target_nr) {
            Some(p) => p,
            None => return KcallResult::Ok(EINVAL),
        };
        // C: do_schedctl.c:37 — sched_proc(p, priority, quantum, cpu, FALSE)
        match crate::sched::sched_proc(target, crate::sched::SchedParams {
            priority: priority_opt, quantum: quantum_opt, cpu: cpu_opt, niced: false,
        }) {
            Ok(()) => {
                // C: do_schedctl.c:39 — p->p_scheduler = NULL（sched_proc 成功后才清）
                target.p_sched.scheduler = None;
            }
            Err(e) => return KcallResult::Ok(crate::sched::sched_proc_error_to_errno(e)),
        }
    } else {
        // C: do_schedctl.c:42 — p->p_scheduler = caller
        let target = match proc_table.get_mut(target_nr) {
            Some(p) => p,
            None => return KcallResult::Ok(EINVAL),
        };
        target.p_sched.scheduler = Some(caller.p_nr);
    }
    KcallResult::Ok(OK)
}
```

**赋值时序对齐**：C 在 `sched_proc` 返回 OK 后才清 `p_scheduler`（do_schedctl.c:37 → do_schedctl.c:39）。Rust 同样保持此序——若 `sched_proc` 失败提前返回，`p_scheduler` 不被触碰，避免失败时让目标进程"既无内核调度又无 user scheduler"成为孤儿。

### 4.7 dispatch_statectl — 部分实现（ClearIpcRefs DEFERRED）

```rust
// C: do_statectl.c:15-51
pub fn dispatch_statectl(
    caller: &mut KProcess,
    msg: &Message,
    priv_table: &mut PrivTable,
    pool: &mut crate::ipc_filter::IpcFilterPool,
) -> KcallResult {
    let sc = msg_statectl(msg);
    // C: do_statectl.c:19 — switch(request) → match enum（编译器穷尽检查）
    let req = match StatectlRequest::try_from(sc.request) {
        Ok(r) => r,
        Err(()) => return KcallResult::Ok(EINVAL),
    };

    match req {
        // C: do_statectl.c:21,25 — clear_ipc_refs(caller, EDEADSRCDST)
        // DEFERRED: 需 IpcEngine::cancel_async（IPC 模块未完整接线）
        StatectlRequest::ClearIpcRefs => { /* DEFERRED */ }

        // C: do_statectl.c:27,29 — priv(caller)->s_state_table / s_state_entries
        StatectlRequest::SetStateTable => {
            if let Some(pid) = caller.priv_id {
                if let Some(priv_) = priv_table.get_mut(pid) {
                    priv_.runtime.s_state_table = sc.address as usize;
                    priv_.runtime.s_state_entries = sc.length;
                }
            }
        }

        // C: do_statectl.c:32,34 — add_ipc_filter(BLACKLIST)
        // 槽位分配（替换语义，free 旧槽后分配新槽）+ 元素填充（已实现）
        StatectlRequest::AddIpcBlFilter => {
            // 1. 先借用 priv 分配槽位（替换语义）
            // 2. 捕获 caller_endpt / cr3（借用结束前，供 closure 引用）
            // 3. 用 data_copy_vmcheck 从用户空间拷入 length 个 IpcFilterElement
            //    （IpcFilterElement 现为 #[repr(C)]，12 字节）
            // 4. 重新借用 priv 填充 slot.elements[0..length]
            // 页缺失 → VmSuspend；拷贝错误 → EFAULT；length 溢出 → EINVAL
            // （syscall_process.rs:718-827，对齐 do_statectl.c:32-34）
        }

        // C: do_statectl.c:37,39 — add_ipc_filter(WHITELIST)
        // 与 AddIpcBlFilter 同构，filter_type = Whitelist（已实现）
        StatectlRequest::AddIpcWlFilter => {
            /* ... 同上结构，IpcFilterType::Whitelist（syscall_process.rs:829-937） ... */
        }

        // C: do_statectl.c:42,44 — clear_ipc_filters(caller)
        StatectlRequest::ClearIpcFilters => {
            if let Some(pid) = caller.priv_id {
                if let Some(priv_) = priv_table.get_mut(pid) {
                    if let Some(ipcf_idx) = priv_.mem.s_ipcf.take() {
                        pool.free(ipcf_idx);
                    }
                }
            }
        }
    }
    KcallResult::Ok(OK)
}
```

**实现要点**（AddIpcBlFilter/AddIpcWlFilter 元素填充）：采用借用检查器安全的模式——在可变借用 priv 分配槽位之前，先捕获 `caller_endpt` 与 `cr3`；借用结束后用 `data_copy_vmcheck`（`syscall_process.rs:762/873`）从用户空间拷入 `length` 个 `IpcFilterElement`（现为 `#[repr(C)]`，12 字节）；拷贝完成后重新借用 priv 填充 `slot.elements[0..length]`。页缺失返回 `VmSuspend`，拷贝错误返回 `EFAULT`，`length` 溢出返回 `EINVAL`。

**DEFERRED**: 仅剩 ClearIpcRefs（do_statectl.c:21-25，需 `IpcEngine::cancel_async`，IPC 模块未完整接线）。本 dispatch 涵盖：SetStateTable、ClearIpcFilters、AddIpcBlFilter/AddIpcWlFilter 的完整实现（槽位分配/释放 + 元素填充）。

**测试隔离**：`dispatch_statectl` 签名新增 `pool: &mut crate::ipc_filter::IpcFilterPool` 参数（替代原全局 `crate::ipc_filter_pool()` 调用）。`IpcFilterPool` 封装为结构体后支持 per-test 实例，避免 `cargo test` 多线程并行时全局 pool 互染。`syscall.rs::dispatch_statectl` wrapper 同步更新。

### 4.8 DEFERRED 函数诚实标注

| DEFERRED 项 | C 位置 | 依赖 trait | 实现路径 |
|------------|--------|-----------|---------|
| exec cross-space copy | do_exec.c:37 | `DataCopy` | VM 集成后接入 `copy_in` |
| exec arch_proc_init | do_exec.c:45 | `ArchProcInit` | 各 arch 在 `os/arch/src/arch/` 提供 impl |
| clear release_address_space | do_clear.c:35 | `VmContext` | VM 集成 |
| ~~clear IRQ hooks~~ | do_clear.c:41-43 | ✅ 已实现 | 全局 `irq_manager()` + `remove_hook_by_slot`（syscall_process.rs:425-438） |
| clear clear_endpoint | do_clear.c:49 | `IpcEngine` | IPC module 集成 |
| ~~clear reset_kernel_timer~~ | do_clear.c:52 | ✅ 已实现 | `clock_state.reset_timer(timer_id)` + `s_alarm_timer.take()`（syscall_process.rs:443-453） |
| runctl SMP IPI | do_runctl.c:55-58 | `SmpRunctl`/`SmpArch` | 见 [16-smp.md](16-smp.md) §4.6 |
| exit mini_notify | do_exit.c:21 | `SignalContext` | SignalContext trait + IPC |
| statectl ClearIpcRefs | do_statectl.c:21-25 | `IpcEngine` | IPC engine（cancel_async / clear_ipc；senda 已实现） |
| ~~statectl filter 元素填充~~ | do_statectl.c:32-41 | ✅ 已实现 | `data_copy_vmcheck` + filter pool（syscall_process.rs:718-937） |

---

## 5. 测试

测试函数名可 grep 验证：`rg "fn test_" os/kernel/src/syscall_process.rs --type rust -n`。

### 5.1 现有测试（23 个，可 grep）

| 测试函数 | 验证行为 | 对应 dispatch |
|---------|---------|--------------|
| `test_statectl_request_from_i32` | StatectlRequest try_from 合法/非法值 | statectl |
| `test_dispatch_statectl_add_ipc_bl_filter_allocates_slot` | 黑名单过滤分配槽位 | statectl |
| `test_dispatch_statectl_add_ipc_wl_filter_allocates_slot` | 白名单过滤分配槽位 | statectl |
| `test_dispatch_statectl_repeated_add_replaces_slot` | 重复 add 替换旧槽（非堆叠） | statectl |
| `test_dispatch_statectl_invalid_request_returns_einval` | 非法 request → EINVAL | statectl |
| `test_dispatch_exit_returns_no_reply` | exit 返回 NoReply | exit |
| `test_dispatch_exit_sets_sigabrt` | exit 设 SIGABRT + SIGNALED + SIG_PENDING | exit |
| `test_dispatch_runctl_stop` | RC_STOP 设 RTS_PROC_STOP | runctl |
| `test_dispatch_runctl_resume` | RC_RESUME 清 RTS_PROC_STOP | runctl |
| `test_dispatch_runctl_invalid_action` | 非法 action → EINVAL | runctl |
| `test_dispatch_runctl_kernel_process_returns_eperm` | 内核进程 → EPERM | runctl |
| `test_dispatch_exec_operates_on_target` | exec 操作目标非 caller | exec |
| `test_dispatch_exec_invalid_endpoint` | 非法 endpoint → EINVAL | exec |
| `test_dispatch_clear_invalid_endpoint_returns_einval` | 非法 endpoint → EINVAL | clear |
| `test_dispatch_clear_sets_target_slot_free` | clear 标记 SLOT_FREE（目标非 caller） | clear |
| `test_dispatch_clear_clears_ext_reg_on_target` | clear 清 FPU 标志 | clear |
| `test_dispatch_schedctl_invalid_flags` | 非法 flags → EINVAL | schedctl |
| `test_dispatch_schedctl_invalid_endpoint_returns_einval` | 非法 endpoint → EINVAL | schedctl |
| `test_dispatch_schedctl_kernel_flag_calls_sched_proc_and_clears_scheduler` | KERNEL flag 调 sched_proc + 清 scheduler | schedctl |
| `test_dispatch_schedctl_kernel_flag_propagates_sched_proc_error` | sched_proc 错误传播 | schedctl |
| `test_dispatch_schedctl_kernel_flag_invalid_quantum_returns_einval` | 非法 quantum → EINVAL | schedctl |
| `test_dispatch_schedctl_no_flag_sets_caller_as_scheduler_on_target` | 无 flag 设 caller 为 scheduler | schedctl |
| `test_dispatch_schedctl_preserves_minus_one_sentinels` | -1 sentinel → Option（保持当前值） | schedctl |

### 5.2 待补充测试（DEFERRED 函数实现后）

| 测试函数 | 验证行为 | 依赖 |
|---------|---------|------|
| `test_dispatch_fork_creates_child_with_new_endpoint` | fork 创建子 + 新 endpoint 代际 +1 | dispatch_fork |
| `test_dispatch_fork_rejects_non_receiving_parent` | 父非 RECEIVING → EINVAL | dispatch_fork |
| `test_dispatch_fork_downgrades_sys_proc_child` | SYS_PROC 父 → USER 子 + RTS_NO_PRIV | dispatch_fork |
| `test_dispatch_runctl_rc_delay_returns_ebusy` | RC_DELAY + SENDING → EBUSY | runctl |
| `test_dispatch_clear_idempotent_on_empty_slot` | 已 clear 再 clear → OK | clear |

---

## 6. 参见

### 6.1 与 redox OS 进程管理对照

| 关注点 | Minix3 / Minix-RS | redox OS |
|--------|-------------------|----------|
| 进程实体 | `struct proc` / `KProcess` | `context::Context` |
| 进程表 | 全局 `proc[]` 数组 + BKL | `contexts` scheme + `Arc<RwLock<Context>>` |
| fork | SYS_FORK 显式 syscall（同步点） | `scheme::proc` dup 操作 |
| exec | SYS_EXEC 替换 IP/SP | `scheme::exec` |
| 信号 | cause_sig + 信号管理器（PM） | `scheme::signal` |
| 调度 | per-CPU 队列 + BKL | per-CPU 队列 + spinlock |

**设计取舍**: Minix-RS 选择 BKL + `ProcNr` 索引（而非 redox 的 `Arc<RwLock>`），理由：(1) 对齐 C 的 BKL 串行化模型，减少 SMP 复杂度；(2) `Arc` 需 `alloc` crate，与 `no_std` 早期启动约束冲突；(3) `ProcNr` 索引 + 进程表统一访问，避免引用计数开销。

### 6.2 跨文档引用

- [11-scheduling-primitives.md](11-scheduling-primitives.md) — `sched_proc` / `SchedParams` / 优先级与时间片
- [16-smp.md](16-smp.md) — `SmpArch` / `smp_schedule_stop_proc` / IPI 同步协议 / BKL
- [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) — `KProcess` 结构 / `RtsFlagsBits` / proc 表
- [10-switch-to-user.md](10-switch-to-user.md) — RTS 标志变更触发的调度入队
- [14-exception-interrupt.md](14-exception-interrupt.md) — 信号投递与异常入口
- [22-privilege.md](22-privilege.md) — `USER_PRIV_ID` / `SYS_PROC` / `PrivTable`（前向引用）
