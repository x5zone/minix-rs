# 17-syscall-process-outline.v1.md — 文档结构契约

> **文档**: `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/17-syscall-process.md`
> **C 源码**: `minix3/minix/kernel/system/do_fork.c`, `do_exec.c`, `do_exit.c`, `do_clear.c`, `do_runctl.c`, `do_schedctl.c`, `do_statectl.c`
> **Rust 实现**: `os/kernel/src/syscall_process.rs` (1243 行)
> **创建**: 2026-08-01
> **依据**: `17-syscall-process-glm-structure.md`（知识点全集 + 诊断）
> **方法**: C 源码 → OS 理论 → Rust 对照（非反向）

---

## 一、章节骨架与主语

### Ch1 主语：进程（"内核如何管理进程的生命周期状态转换？"）

核心问题：**内核作为进程状态机引擎，如何用 7 条系统调用弧驱动进程在"未出生→就绪→运行→停止→死亡→槽位回收"之间转换，且每条弧都保持进程表/endpoint/权限/调度状态的原子一致？**

回答：每条弧都对应"读取目标 proc → 修改标志/字段 → 写回"的原子更新；fork 是同步点（父必须 RECEIVING）+ 权限降级点；exec 是映像替换（非新建）；exit 委托信号管理器；clear 是幂等回收；runctl/schedctl/statectl 是运行时控制。

| 节 | 标题 | 灵魂本质（一句话） | 概念组 |
|----|------|-------------------|--------|
| §1.1 | 进程生命周期：fork→exec→exit→clear 状态转换弧 | "进程从 fork 出生到 clear 槽位回收是一条不可逆状态链——fork 创建并降权，exec 替换映像，exit 委托信号，clear 回收槽位" | LC/IR/SS/SR |
| §1.2 | 同步 fork：为什么父进程必须 RTS_RECEIVING | "fork 是父子同步点——父必须正在接收，复制才能安全借用父的消息缓冲；endpoint 代际+1 防止旧 endpoint 复活" | SF/EG |
| §1.3 | 权限降级：SYS_PROC 父→USER 子 | "权限不继承——SYS_PROC 父的子进程必须降级为 USER_PRIV 并设 RTS_NO_PRIV，由调用者显式重新赋权" | PD |
| §1.4 | SMP 停止：跨 CPU IPI 同步 | "runctl 停止远 CPU 上的进程不能直接改标志——必须用同步 IPI 让目标 CPU 保存上下文后停止，避免竞态" | RF/H |

### Ch2 主语：C 源码符号（file:line 锚定）

每节以 C 符号为单元，附 file:line，说明语义与调用关系。

### Ch3 主语：设计决策（hypothesis-driven）

采用"如果 X 设计会有 Y 问题所以用 Z"格式，禁止"旧版/最初/后来/我们改成"迭代叙事。

### Ch4 主语：Rust 实现（真实代码，非 stub）

贴 `syscall_process.rs` 真实代码片段，标注 file:line。缺失函数诚实标注 DEFERRED + 理由。

### Ch5 主语：测试函数（可 grep 验证）

列出实际 `fn test_*` 函数名，每个测试对应一个被测行为。

---

## 二、详细大纲

### Ch1. 概念建构（concept-driven）

#### §1.1 进程生命周期：fork→exec→exit→clear 状态转换弧

**灵魂本质**: 进程从 fork 出生到 clear 槽位回收是一条不可逆状态链——fork 创建并降权，exec 替换映像，exit 委托信号，clear 回收槽位。

**WHY → WHAT → HOW 弧线**:
- **WHY**: 操作系统需要一个"进程"实体承载执行流，但进程不能凭空出现/消失——创建需要复制父状态，销毁需要回收所有资源（地址空间/IRQ/endpoint/timer/FPU/权限）。如果创建/销毁不是原子且幂等的，会导致槽位泄漏（进程表耗尽）或悬挂资源（IRQ 仍触发到已死进程）。
- **WHAT**: 内核用 4 条核心弧构成生命周期主链：fork（出生+降权）、exec（换壳不改魂）、exit（自杀信号委托）、clear（槽位回收）。每条弧都是"读目标 proc → 改标志/字段 → 写回"的原子更新，且 clear 必须幂等（已清理则直接返回 OK）。
- **HOW**: C 用 `do_fork.c:26-134`（`*rpc = *rpp` 拷贝 + RTS_NO_QUANTUM）、`do_exec.c:20-59`（arch_proc_init 设 IP/SP + 清 DELIVERMSG）、`do_exit.c:18-25`（cause_sig(SIGABRT) + EDONTREPLY）、`do_clear.c:17-78`（release_address_space → isemptyp 幂等检查 → RTS_SLOT_FREE）。

**关键概念**:
- **exec 是替换非新建**: exec 不分配新 proc 槽，只改 IP/SP/名字，复用同一 endpoint——这与 fork（新建槽）本质不同。
- **exit 不直接杀**: exit 调 `cause_sig(caller, SIGABRT)`（do_exit.c:24），把死亡决策权交给信号管理器（PM），内核不越俎代庖。
- **clear 的幂等性**: `if(isemptyp(rc)) return OK`（do_clear.c:38）——重复 clear 安全，防止 PM 重试导致二次释放。

#### §1.2 同步 fork：为什么父进程必须 RTS_RECEIVING

**灵魂本质**: fork 是父子同步点——父必须正在接收，复制才能安全借用父的消息缓冲；endpoint 代际+1 防止旧 endpoint 复活。

**WHY → WHAT → HOW 弧线**:
- **WHY**: fork 时子进程是父的副本，但子进程的 fork 系统调用"返回值"必须为 0（do_fork.c:74 `rpc->p_reg.retreg = 0`）。这个返回值通过父进程当时正在等待的 IPC 回复缓冲投递。如果父进程不在 RECEIVING 状态，回复缓冲指针无效，复制子进程会拷贝到一个悬空的缓冲——所以 fork 必须同步。
- **WHAT**: 内核强制 `RTS_ISSET(rpp, RTS_RECEIVING)` 前提（do_fork.c:51），不满足返回 EINVAL。复制时 `*rpc = *rpp`（do_fork.c:63）连消息缓冲指针一并拷贝，子进程通过同一缓冲收到 pid=0。
- **HOW**: endpoint 不能直接继承——若复用父 endpoint，旧代 ipc 消息会投递到错乱的目标。所以 `_ENDPOINT_G` 取代际 → `++gen`（do_fork.c:69）→ `_ENDPOINT(gen, p_nr)` 重组（do_fork.c:72）。代际回绕：`>= _ENDPOINT_MAX_GENERATION` 则归 1（do_fork.c:69-70）。

**关键约束**:
1. 父非 RECEIVING → EINVAL（do_fork.c:51-54）
2. 子槽必须为空（`!isemptyp(rpc)` → EINVAL，do_fork.c:46）
3. 复制前 `save_fpu(rpp)`（do_fork.c:57）保证父 FPU 上下文已落盘
4. 信号状态不继承：`RTS_UNSET(rpc, RTS_SIGNALED|RTS_SIG_PENDING|RTS_P_STOP)`（do_fork.c:122）

#### §1.3 权限降级：SYS_PROC 父→USER 子

**灵魂本质**: 权限不继承——SYS_PROC 父的子进程必须降级为 USER_PRIV 并设 RTS_NO_PRIV，由调用者显式重新赋权。

**WHY → WHAT → HOW 弧线**:
- **WHY**: 系统服务（如 PM）是 SYS_PROC，拥有高权限（可发内核 syscall、绑 IRQ）。如果 fork 出的子进程继承 SYS_PROC，任何用户进程都能通过 fork 提权——这是安全漏洞。
- **WHAT**: fork 检查 `priv(rpp)->s_flags & SYS_PROC`（do_fork.c:105），若父是系统进程，子进程 `p_priv = priv_addr(USER_PRIV_ID)`（do_fork.c:106）+ `RTS_NO_PRIV`（do_fork.c:107）。RTS_NO_PRIV 使子进程不可调度，直到调用者通过 SYS_PRIVCTL 显式赋权。
- **HOW**: 降级是单向的——子进程从 USER_PRIV 起步，PM 在 exec 前用 sys_privctl 赋予适当权限。VM 模式下还设 `RTS_VMINHIBIT`（do_fork.c:115-116）等新页表就绪。

#### §1.4 SMP 停止：跨 CPU IPI 同步

**灵魂本质**: runctl 停止远 CPU 上的进程不能直接改标志——必须用同步 IPI 让目标 CPU 保存上下文后停止，避免竞态。

**WHY → WHAT → HOW 弧线**:
- **WHY**: 单 CPU 下 `RTS_SET(rp, RTS_PROC_STOP)`（do_runctl.c:62）立即可行，因为当前 CPU 持有 BKL，目标进程不会同时跑。但 SMP 下目标进程可能在另一 CPU 上运行——直接改标志后，目标 CPU 仍可能用陈旧上下文继续执行一个时钟周期，导致状态不一致。
- **WHAT**: SMP 路径检查 `rp->p_cpu != cpuid`（do_runctl.c:57），若目标在远 CPU，调 `smp_schedule_stop_proc(rp)`（do_runctl.c:58）发同步 IPI；否则本地 `RTS_SET`。
- **HOW**: 同步 IPI 的完整协议见 [16-smp.md](16-smp.md) §1.3——`smp_schedule_sync(STOP_PROC)` 释放 BKL → 等待目标 CPU 处理 → 重获 BKL。RC_DELAY 模式下若目标正在 SENDING，设 `MF_SIG_DELAY` 返回 EBUSY（do_runctl.c:44-49），让 PM 延后停止。

**单 CPU 退化**: `CONFIG_SMP` 未定义时无 IPI 路径，直接 RTS_SET。

---

### Ch2. C 源码分析（file:line 锚定）

#### §2.1 do_fork — 复制 proc 结构，新 endpoint

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_fork(caller, m_ptr)` | do_fork.c:26-134 | 入口 |
| `isokendpt(endpt, &p_proc)` | do_fork.c:41 | 验证父 endpoint |
| `rpp = proc_addr(p_proc)` / `rpc = proc_addr(slot)` | do_fork.c:44-45 | 父/子 proc |
| `isemptyp(rpp)\|\|!isemptyp(rpc)` → EINVAL | do_fork.c:46 | 父非空子空 |
| `RTS_ISSET(rpp, RTS_RECEIVING)` | do_fork.c:51 | 同步前提 |
| `save_fpu(rpp)` | do_fork.c:57 | 保存父 FPU |
| `*rpc = *rpp` | do_fork.c:63 | proc 结构体拷贝 |
| `gen = _ENDPOINT_G(...); ++gen; _ENDPOINT(gen, p_nr)` | do_fork.c:59,69-72 | endpoint 代际 |
| `rpc->p_reg.retreg = 0` | do_fork.c:74 | 子返回值=0 |
| `RTS_SET(rpc, RTS_NO_QUANTUM)` | do_fork.c:90 | 子不可调度 |
| `priv(rpp)->s_flags & SYS_PROC` → USER_PRIV + RTS_NO_PRIV | do_fork.c:105-107 | 权限降级 |
| `PFF_VMINHIBIT` → RTS_VMINHIBIT | do_fork.c:115-116 | 等新页表 |
| `RTS_UNSET(rpc, RTS_SIGNALED\|SIG_PENDING\|P_STOP)` | do_fork.c:122 | 信号不继承 |

#### §2.2 do_exec — 清 DELIVERMSG，设 IP/SP，清 FPU

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_exec(caller, m_ptr)` | do_exec.c:20-59 | 入口 |
| `isokendpt(endpt, &proc_nr)` | do_exec.c:27 | 验证目标 |
| `rp->p_misc_flags &= ~MF_DELIVERMSG` | do_exec.c:32-34 | 清待投递 |
| `data_copy(caller, name, KERNEL, name, ...)` | do_exec.c:37-39 | 跨空间拷名 |
| `arch_proc_init(rp, ip, stack, ps_str, name)` | do_exec.c:45-48 | 架构相关设 IP/SP |
| `RTS_UNSET(rp, RTS_RECEIVING)` | do_exec.c:51 | 解除接收（不回复 EXEC） |
| `&= ~MF_FPU_INITIALIZED` / `release_fpu(rp)` | do_exec.c:55-57 | FPU 失效 |

#### §2.3 do_exit — cause_sig(SIGABRT)，EDONTREPLY

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_exit(caller, m_ptr)` | do_exit.c:18-25 | 入口 |
| `int sig_nr = SIGABRT` | do_exit.c:22 | 自杀信号 |
| `cause_sig(caller->p_nr, sig_nr)` | do_exit.c:24 | 委托信号管理器 |
| `return(EDONTREPLY)` | do_exit.c:25 | 不回复 |

#### §2.4 do_clear — 释放资源，RTS_SLOT_FREE

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_clear(caller, m_ptr)` | do_clear.c:17-78 | 入口 |
| `isokendpt(endpt, &exit_p)` | do_clear.c:29 | 验证目标 |
| `release_address_space(rc)` | do_clear.c:35 | 释放地址空间 |
| `if(isemptyp(rc)) return OK` | do_clear.c:38 | **幂等** |
| `for irq_hooks` / `rm_irq_handler` | do_clear.c:41-46 | 释放 IRQ |
| `clear_endpoint(rc)` | do_clear.c:49 | 释放 IPC endpoint |
| `reset_kernel_timer(&priv(rc)->s_alarm_timer)` | do_clear.c:52 | 重置定时器 |
| `RTS_SETFLAGS(rc, RTS_SLOT_FREE)` | do_clear.c:57 | 标记槽空闲 |
| `release_fpu` / `&= ~MF_FPU_INITIALIZED` | do_clear.c:60-61 | 释放 FPU |
| `SYS_PROC` → `s_proc_nr = NONE` | do_clear.c:68 | 释放 priv |

#### §2.5 do_runctl — RC_STOP/RC_RESUME，SMP IPI

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_runctl(caller, m_ptr)` | do_runctl.c:18-73 | 入口 |
| `isokendpt(RC_ENDPT, &proc_nr)` | do_runctl.c:30 | 验证目标 |
| `iskerneln(proc_nr)` → EPERM | do_runctl.c:31 | 内核进程不可控 |
| `RC_STOP && RC_DELAY` → MF_SIG_DELAY → EBUSY | do_runctl.c:44-49 | 延迟停止 |
| `CONFIG_SMP` && `p_cpu != cpuid` → `smp_schedule_stop_proc` | do_runctl.c:55-60 | 跨 CPU IPI |
| `RTS_SET(rp, RTS_PROC_STOP)` | do_runctl.c:62 | 本地停止 |
| `RTS_UNSET(rp, RTS_PROC_STOP)` | do_runctl.c:66 | 恢复 |

#### §2.6 do_schedctl — KERNEL flag 设参数，否则设 scheduler

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_schedctl(caller, m_ptr)` | do_schedctl.c:7-46 | 入口 |
| `flags & ~SCHEDCTL_FLAG_KERNEL` → EINVAL | do_schedctl.c:17-21 | flags 校验 |
| `isokendpt(endpoint, &proc_nr)` | do_schedctl.c:23 | 验证目标 |
| `SCHEDCTL_FLAG_KERNEL` → `sched_proc(p, priority, quantum, cpu, FALSE)` | do_schedctl.c:28-38 | 内核调度模式 |
| `p->p_scheduler = NULL` | do_schedctl.c:39 | 清 user scheduler |
| `p->p_scheduler = caller` | do_schedctl.c:42 | 调用者接管 |

#### §2.7 do_statectl — 5 种请求分发

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_statectl(caller, m_ptr)` | do_statectl.c:15-51 | 入口 |
| `switch(request)` | do_statectl.c:19 | 请求分发 |
| `SYS_STATE_CLEAR_IPC_REFS` → `clear_ipc_refs(caller, EDEADSRCDST)` | do_statectl.c:21-26 | 清 IPC 引用 |
| `SYS_STATE_SET_STATE_TABLE` → `s_state_table`/`s_state_entries` | do_statectl.c:27-31 | 设状态表 |
| `SYS_STATE_ADD_IPC_BL_FILTER` → `add_ipc_filter(BLACKLIST, ...)` | do_statectl.c:32-36 | 黑名单 |
| `SYS_STATE_ADD_IPC_WL_FILTER` → `add_ipc_filter(WHITELIST, ...)` | do_statectl.c:37-41 | 白名单 |
| `SYS_STATE_CLEAR_IPC_FILTERS` → `clear_ipc_filters(caller)` | do_statectl.c:42-45 | 清过滤 |

#### §2.8 调用关系图（时序）

**fork 时序**（同步点）:
```
PM: sys_fork(parent_endpt, child_slot, flags)
  └─ KERNEL: do_fork()  [do_fork.c:26]
       ├─ 校验父 RECEIVING  [do_fork.c:51]
       ├─ save_fpu(rpp)  [do_fork.c:57]
       ├─ *rpc = *rpp  [do_fork.c:63]  ← 结构体拷贝
       ├─ gen++; rpc->p_endpoint = _ENDPOINT(gen, p_nr)  [do_fork.c:69-72]
       ├─ rpc->p_reg.retreg = 0  [do_fork.c:74]  ← 子返回 0
       ├─ RTS_SET(rpc, RTS_NO_QUANTUM)  [do_fork.c:90]
       ├─ if SYS_PROC: USER_PRIV + RTS_NO_PRIV  [do_fork.c:105-107]
       └─ m_ptr->endpt = rpc->p_endpoint  [do_fork.c:111]  ← 回填子 endpoint
```

**clear 时序**（幂等回收）:
```
PM: sys_clear(exit_endpt)
  └─ KERNEL: do_clear()  [do_clear.c:17]
       ├─ release_address_space(rc)  [do_clear.c:35]
       ├─ if isemptyp(rc): return OK  [do_clear.c:38]  ← 幂等
       ├─ 释放 IRQ/endpoint/timer/FPU  [do_clear.c:41-61]
       ├─ RTS_SETFLAGS(rc, RTS_SLOT_FREE)  [do_clear.c:57]
       └─ if SYS_PROC: s_proc_nr = NONE  [do_clear.c:68]
```

---

### Ch3. 设计决策（hypothesis-driven）

#### D1. proc 拷贝：`KProcess::fork_from` 替代 `*rpc = *rpp`

**假设性推理**:
- 如果用 Rust 的 `*rpc = *rpp` 等价结构体赋值（`Clone::clone`）：`KProcess` 含 `AtomicU8`/`AtomicU32` 等非 `Copy` 字段，且 `p_seg` 需重置、`priv_id` 需降级——直接 clone 会拷贝父的原子状态与权限，违反 fork 语义（子应是独立副本）。
- 如果用 `KProcess::clone()` + 逐字段修正：clone 后再改 8+ 个字段，易遗漏（如忘记清 `p_reg.retreg`），且 clone 拷贝了不该拷的 `p_magic`。
- 所以用专门的 `KProcess::fork_from(parent, child_nr, child_endpoint)` 构造函数（proc.rs:1385）：在构造期一次性应用所有 fork 修正（RTS_NO_QUANTUM、清信号标志、清 timer/trace、重置 p_seg、降级 priv_id），语义集中在一处，编译器保证字段不漏。

**实现**: `KProcess::fork_from` (proc.rs:1385) + `complete_fork_setup`（应用 NO_PRIV/VMINHIBIT/name 后缀）。

#### D2. endpoint 表达：Endpoint newtype 替代裸 i32

**假设性推理**:
- 如果用裸 `i32` 表达 endpoint（C 方式）：endpoint 与 errno、proc_nr、raw 值都是 i32，编译器无法区分——`return EINVAL` 和 `return child_endpoint` 类型相同，调用者可能误用。
- 如果用 `type Endpoint = i32` 类型别名：仅文档作用，编译期无保护，与裸 i32 等价。
- 所以用 `#[repr(transparent)] pub struct Endpoint(pub i32)` newtype（endpoint.rs:44）：编译期防止与其他 i32 混淆，且 `repr(transparent)` 保证 ABI 与 i32 一致（FFI/消息布局兼容）。配套 `from_generation_slot`/`slot`/`generation` 方法封装代际编解码。

**实现**: `Endpoint` newtype (endpoint.rs:44) + `Endpoint::fork_new_endpoint` (endpoint.rs) + `from_generation_slot`/`generation`/`slot`。

#### D3. statectl 请求：StatectlRequest enum + match 替代 switch/case

**假设性推理**:
- 如果用 `switch(request)` + 裸 i32 case（C 方式）：case 值是魔术数字（1-5），且 default 分支返回 EINVAL——编译器不检查是否覆盖所有 case，新增请求类型易漏。
- 如果用 `const CLEAR_IPC_REFS: i32 = 1` 常量：仍是裸 i32 匹配，无法穷尽检查。
- 所以用 `pub enum StatectlRequest { ClearIpcRefs=1, SetStateTable=2, ... }` (syscall_process.rs:64) + `match req`：编译器强制穷尽检查，新增变体必须处理；`TryFrom<i32>` 把非法值转为 `Err(())` → EINVAL，集中处理边界。

**实现**: `StatectlRequest` enum (syscall_process.rs:62-80) + `TryFrom<i32>` (82-95) + `match req` (628)。

#### D4. -1 sentinel：Option 替代裸 -1

**假设性推理**:
- 如果用 C 的 `priority = -1` 表示"保持当前值"（do_schedctl.c:32）：-1 是魔术值，与合法优先级 0..15 共享 i32 类型，调用者可能误传 -2（无效但不会被 -1 检查捕获，除非额外校验）。
- 如果用 `i32::MAX` 或其他哨兵：同样需额外校验，且与 C 语义不一致（C 用 -1）。
- 所以用 `Option<u8>` 表达"保持当前值"：`-1 → None`，`v >= 0 → Some(v as u8)`，`v < -1 → EINVAL`（syscall_process.rs:554-558）。Option 在类型层表达"可选"，编译器强制处理 None 分支。同时保留 -1 哨兵的 C 语义兼容（消息层仍是 i32）。

**实现**: `SchedParams { priority: Option<u8>, quantum: Option<u32>, cpu: Option<u32>, niced: bool }` (sched) + dispatch_schedctl 的 -1→None 转换 (syscall_process.rs:554-564)。

#### D5. 返回值：KcallResult enum 替代 errno 返回

**假设性推理**:
- 如果用 `i32` 返回 errno（C 方式）：`return OK`(0)、`return EINVAL`(22)、`return EDONTREPLY`(-998)、`return child_endpoint`(正数) 共享 i32——调用者无法从类型区分"成功返回值"与"errno"与"不回复"，需靠范围判断（C 的 `result >= 0` vs `result == EDONTREPLY`）。
- 如果用 `Result<i32, Errno>`：EDONTREPLY 不是错误（是"不回复"语义），强行塞进 Err 变体语义不准；且 child_endpoint 是成功返回值，与 errno 0 混在 Ok。
- 所以用 `pub enum KcallResult { Ok(i32), VmSuspend, NoReply, BadCall, CallDenied }` (syscall.rs:180)：每个变体对应一种"调用完成方式"——Ok(返回值) / VmSuspend(需 VM 协助) / NoReply(不回复，如 exit) / BadCall(非法 syscall) / CallDenied(无权限)。dispatch 层返回 enum，dispatch 上层 match 处理。

**实现**: `KcallResult` enum (syscall.rs:180-192) + `dispatch_exit` 返回 `NoReply` (syscall_process.rs:298) + 其他 dispatch 返回 `Ok(errno)`。

#### D6. 进程号：ProcNr newtype 升级（建议）

**假设性推理**:
- 当前 `pub type ProcNr = i32`（proc.rs:24）是类型别名，编译期与裸 i32 等价——endpoint raw、errno、proc_nr 都能互相赋值，无防护。
- 如果升级为 `#[repr(transparent)] pub struct ProcNr(pub i32)` newtype：与 Endpoint 对齐，编译期防止混淆；`repr(transparent)` 保证消息布局兼容。
- 如果保持类型别名：anti-translate 弱化，未来易引入"把 errno 当 proc_nr"类 bug。
- 所以建议升级为 newtype（design.md 落地）。注意：升级需同步修改所有 `ProcNr` 使用点（约 N 处），且 `From<i32>`/`into()` 转换需补。

**实现**: 当前 `type ProcNr = i32` (proc.rs:24)；design.md 建议升级为 newtype。

---

### Ch4. 实现详解（真实代码）

#### §4.1 dispatch_fork — 完整实现

贴 `syscall_process.rs:141-220` 真实代码片段：endpoint 验证 → 同步前提检查 → `Endpoint::fork_new_endpoint` 代际 → `KProcess::fork_from` 构造 → `priv_table` 查 SYS_PROC → `complete_fork_setup` → 写回 proc_table。标注 C 行号对应。

#### §4.2 dispatch_exec — 部分实现（DEFERRED 标注）

贴 `syscall_process.rs:236-277`：清 DELIVERMSG（L253）→ RTS_UNSET(RECEIVING)（L267）→ 清 EXT_REG_INITIALIZED（L272）。**DEFERRED**: cross-space copy (L259, 需 data_copy_vmcheck) + arch_proc_init (L263, 需 ArchProcInit trait)。

#### §4.3 dispatch_exit — 完整实现

贴 `syscall_process.rs:285-299` + `cause_signal_abort` 助手 (L308-320)：`p_pending.add(SIGABRT)` + `RTS_SIGNALED|SIG_PENDING` + 返回 `KcallResult::NoReply`。**DEFERRED**: 信号管理器通知（mini_notify，需 SignalContext trait）。

#### §4.4 dispatch_clear — 部分实现（DEFERRED 标注）

贴 `syscall_process.rs:349-414`：endpoint 验证 → 幂等检查（SLOT_FREE）→ RTS_SLOT_FREE → 清 EXT_REG_INITIALIZED → SYS_PROC 释放 priv（s_proc_nr=None）。**DEFERRED**: release_address_space (L372, VM) + IRQ hooks (L382) + clear_endpoint (L385, IPC) + reset_kernel_timer (L388)。

#### §4.5 dispatch_runctl — 完整（单 CPU）

贴 `syscall_process.rs:423-493`：endpoint 验证 → is_kernel→EPERM → RC_STOP（RC_DELAY→MF_SIG_DELAY→EBUSY）→ RTS_PROC_STOP → RC_RESUME→RTS_UNSET。**DEFERRED**: SMP IPI 路径（依赖 SmpArch::schedule_stop_proc，见 16-smp）。

#### §4.6 dispatch_schedctl — 完整

贴 `syscall_process.rs:519-603`：flags 校验 → endpoint 验证 → KERNEL flag 分支（-1→Option 转换 + sched_proc + 清 scheduler）→ 否则设 caller 为 scheduler。含 -1 sentinel→Option 转换 (L554-564)。

#### §4.7 dispatch_statectl — 部分（DEFERRED 标注）

贴 `syscall_process.rs:619-732`：`StatectlRequest::try_from` + match。SetStateTable（L663）+ ClearIpcFilters（L717）已实现；AddIpcBlFilter/AddIpcWlFilter 槽位分配已实现（L674/700），**DEFERRED**: filter 元素填充（需 data_copy_vmcheck）；ClearIpcRefs DEFERRED（需 IPC engine）。

#### §4.8 DEFERRED 函数诚实标注

| 函数/路径 | C 位置 | DEFERRED 理由 |
|----------|--------|--------------|
| exec cross-space copy | do_exec.c:37-39 | 需 data_copy_vmcheck（VM 集成） |
| exec arch_proc_init | do_exec.c:45-48 | 需 ArchProcInit trait（arch 层） |
| clear release_address_space | do_clear.c:35 | 需 VM 集成 |
| clear IRQ hooks | do_clear.c:41-46 | 需 IRQ manager |
| clear clear_endpoint | do_clear.c:49 | 需 IPC module |
| clear reset_kernel_timer | do_clear.c:52 | 需 timer + PrivTable |
| runctl SMP IPI | do_runctl.c:55-60 | 需 SmpArch::schedule_stop_proc（见 16-smp） |
| exit mini_notify | do_exit.c:24 | 需 SignalContext trait + IPC |
| statectl ClearIpcRefs | do_statectl.c:21-26 | 需 IPC engine（senda/cancel_async） |
| statectl filter 元素填充 | do_statectl.c:32-41 | 需 data_copy_vmcheck |

---

### Ch5. 测试（可 grep 函数名）

#### §5.1 现有测试（已实现，22 个）

| 测试函数 | 验证行为 | 对应 dispatch |
|---------|---------|--------------|
| `test_statectl_request_from_i32` | StatectlRequest try_from 合法/非法值 | statectl |
| `test_dispatch_statectl_add_ipc_bl_filter_allocates_slot` | 黑名单过滤分配槽位 | statectl |
| `test_dispatch_statectl_add_ipc_wl_filter_allocates_slot` | 白名单过滤分配槽位 | statectl |
| `test_dispatch_statectl_repeated_add_replaces_slot` | 重复 add 替换旧槽 | statectl |
| `test_dispatch_statectl_invalid_request_returns_einval` | 非法 request → EINVAL | statectl |
| `test_dispatch_exit_returns_no_reply` | exit 返回 NoReply | exit |
| `test_dispatch_exit_sets_sigabrt` | exit 设 SIGABRT+SIGNALED | exit |
| `test_dispatch_runctl_stop` | RC_STOP 设 RTS_PROC_STOP | runctl |
| `test_dispatch_runctl_resume` | RC_RESUME 清 RTS_PROC_STOP | runctl |
| `test_dispatch_runctl_invalid_action` | 非法 action → EINVAL | runctl |
| `test_dispatch_runctl_kernel_process_returns_eperm` | 内核进程 → EPERM | runctl |
| `test_dispatch_exec_operates_on_target` | exec 操作目标非 caller | exec |
| `test_dispatch_exec_invalid_endpoint` | 非法 endpoint → EINVAL | exec |
| `test_dispatch_clear_invalid_endpoint_returns_einval` | 非法 endpoint → EINVAL | clear |
| `test_dispatch_clear_sets_target_slot_free` | clear 标记 SLOT_FREE | clear |
| `test_dispatch_clear_clears_ext_reg_on_target` | clear 清 FPU 标志 | clear |
| `test_dispatch_schedctl_invalid_flags` | 非法 flags → EINVAL | schedctl |
| `test_dispatch_schedctl_invalid_endpoint_returns_einval` | 非法 endpoint → EINVAL | schedctl |
| `test_dispatch_schedctl_kernel_flag_calls_sched_proc_and_clears_scheduler` | KERNEL flag 调 sched_proc+清 scheduler | schedctl |
| `test_dispatch_schedctl_kernel_flag_propagates_sched_proc_error` | sched_proc 错误传播 | schedctl |
| `test_dispatch_schedctl_kernel_flag_invalid_quantum_returns_einval` | 非法 quantum → EINVAL | schedctl |
| `test_dispatch_schedctl_no_flag_sets_caller_as_scheduler_on_target` | 无 flag 设 caller 为 scheduler | schedctl |
| `test_dispatch_schedctl_preserves_minus_one_sentinels` | -1 sentinel → Option | schedctl |

#### §5.2 待补充测试（DEFERRED 函数实现后）

| 测试函数 | 验证行为 | 依赖 |
|---------|---------|------|
| `test_dispatch_fork_creates_child_with_new_endpoint` | fork 创建子+新 endpoint | fork 测试补全 |
| `test_dispatch_fork_rejects_non_receiving_parent` | 父非 RECEIVING → EINVAL | dispatch_fork |
| `test_dispatch_fork_downgrades_sys_proc_child` | SYS_PROC 父→USER 子+NO_PRIV | dispatch_fork |
| `test_dispatch_runctl_rc_delay_returns_ebusy` | RC_DELAY+SENDING→EBUSY | runctl |
| `test_dispatch_clear_idempotent_on_empty_slot` | 已 clear 再 clear→OK | clear |

---

### Ch6. 参见

- [11-scheduling-primitives.md](11-scheduling-primitives.md) — sched_proc / SchedParams / 优先级与时间片
- [16-smp.md](16-smp.md) — smp_schedule_stop_proc / IPI 同步协议 / BKL
- [06-proc-init-boot-proc.md](06-proc-init-boot-proc.md) — KProcess 结构 / RTS 标志 / proc 表
- [10-switch-to-user.md](10-switch-to-user.md) — RTS 标志变更触发的调度入队
- [14-exception-interrupt.md](14-exception-interrupt.md) — 信号投递与异常入口
- [22-privilege.md](22-privilege.md) — USER_PRIV_ID / SYS_PROC / priv 表

---

## 三、知识点覆盖矩阵

| 概念组 | Ch1 | Ch2 | Ch3 | Ch4 | Ch5 |
|--------|-----|-----|-----|-----|-----|
| A. 生命周期弧 LC | §1.1 | §2.1-2.4 | D1 | §4.1-4.4 | test_dispatch_clear_* |
| B. 同步 fork SF | §1.2 | §2.1 | D1 | §4.1 | (§5.2 待补) |
| C. endpoint 代际 EG | §1.2 | §2.1 | D2 | §4.1 | (§5.2 待补) |
| D. 权限降级 PD | §1.3 | §2.1 | — | §4.1 | (§5.2 待补) |
| E. 映像替换 IR | §1.1 | §2.2 | — | §4.2 | test_dispatch_exec_* |
| F. 自杀信号 SS | §1.1 | §2.3 | D5 | §4.3 | test_dispatch_exit_* |
| G. 槽位回收 SR | §1.1 | §2.4 | — | §4.4 | test_dispatch_clear_* |
| H. 停止/恢复 RF | §1.4 | §2.5 | — | §4.5 | test_dispatch_runctl_* |
| I. 调度权移交 SC | §1.1 | §2.6 | D4 | §4.6 | test_dispatch_schedctl_* |
| J. IPC 状态控制 ST | §1.1 | §2.7 | D3 | §4.7 | test_dispatch_statectl_* |
| K. ProcNr newtype | — | — | D6 | (建议) | — |

---

## 四、断裂修复表

| 断裂点 | 修复方案 |
|--------|---------|
| Ch1 feature-listing（非 concept-driven） | Ch1 §1.1-1.4 用状态机视角重写，feature-listing 移到 Ch2 |
| 迭代叙事（L7/L73/L130/L134 日期+P0-XX） | 重写时全部删除，实现状态用客观描述 |
| tmp 引用（L233 tmp-14） | 删除，来源仅限 C 源码+Rust 代码+OS 理论 |
| stub `{ ... }`（L207-213） | Ch4 贴 syscall_process.rs 真实代码 + DEFERRED 标注 |
| 测试不可 grep（Ch5 bullet） | Ch5 §5.1 列 22 个 `fn test_*` 函数名 |
| Ch3 无假设性推理 | Ch3 D1-D6 用"如果 X 会有 Y 问题所以用 Z" |
| ProcNr 弱 anti-translate | Ch3 D6 建议 newtype 升级 + design 落地 |
| fork 无测试 | Ch5 §5.2 待补 3 个 fork 测试 |
| DEFERRED 隐藏 | Ch4 §4.8 DEFERRED 表（10 项+理由） |

---

## 五、自检

- [x] Ch1 主语是进程（状态机视角），非函数名/结构体名
- [x] Ch1 每节有"灵魂本质"一句话
- [x] Ch1 采用 WHY→WHAT→HOW 弧线（§1.1-1.4 均有）
- [x] Ch2 每个符号带 file:line
- [x] Ch3 采用 hypothesis-driven（D1-D6 均有"如果 X 会有 Y 问题所以用 Z"）
- [x] Ch3 无迭代叙事（"旧版/最初/后来/我们改成"）
- [x] Ch4 贴真实代码，缺失函数诚实标注 DEFERRED + 理由
- [x] Ch5 测试函数可 grep 验证（`fn test_*`，22 个）
- [x] 知识点覆盖矩阵完整（A-K 十一组）
- [x] 断裂修复表完整（9 处断裂 + 修复方案）
- [x] 无 tmp 文件引用
- [x] 无内部 review ID（P0-XX/P1-XX）
- [x] 无迭代叙事日期（2026-XX-XX）
- [x] anti-translate 体现（Endpoint newtype / fork_from / StatectlRequest enum / Option sentinel / KcallResult enum / ProcNr newtype 建议）
- [x] 参见 redox OS（context::Context + scheme::proc 对照，见 structure.md §8）
- [x] no_std 约束（design.md 落地）
