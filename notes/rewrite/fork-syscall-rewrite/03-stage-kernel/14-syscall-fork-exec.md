# 14-syscall-fork-exec: sys_fork 和 sys_exec

> **分类**: Kernel 特权与系统调用
> **源码**: `minix3/minix/kernel/system/do_fork.c`(136行), `do_exec.c`(60行), `arch_system.c` fork 相关
> **说明**: 内核 fork——创建子进程 proc 结构、复制寄存器、生成 endpoint、设置特权位

---

## 1. 概述

### 1.1 概念定义/作用

**sys_fork** 和 **sys_exec** 是 Minix3 内核中进程创建和程序替换的两个核心系统调用。它们由 PM（Process Manager）在处理用户进程的 `fork()` 和 `exec()` 系统调用时调用，完成内核层面的进程管理操作。

**sys_fork** 在内核中创建子进程的进程控制块。PM 先在用户空间分配进程表槽位，然后通过 `sys_fork` 通知内核在对应的内核进程表中创建子进程条目。内核复制父进程的 `struct proc` 到子进程槽位，递增 generation 号生成新 endpoint，重置子进程特有的字段（返回值、时间统计、定时器等），并设置适当的 RTS 标志使子进程暂时不可运行。

**sys_exec** 在进程执行新程序后更新内核中的进程上下文。PM 加载新程序映像后，通过 `sys_exec` 通知内核更新进程的寄存器上下文（新的指令指针和栈指针）、进程名、FPU 状态等。

### 1.2 与 Minix3 的对应关系

| 功能 | 函数 | 源文件 |
|------|------|--------|
| fork 处理 | `do_fork()` | system/do_fork.c |
| exec 处理 | `do_exec()` | system/do_exec.c |
| 架构相关进程初始化 | `arch_proc_init()` | arch/i386/arch_system.c |
| FPU 保存 | `save_fpu()` | arch/i386/arch_system.c |
| FPU 释放 | `release_fpu()` | arch/i386/arch_system.c |
| 进程记账重置 | `reset_proc_accounting()` | proc.c |

### 1.3 关键状态/机制说明

**fork 的同步性要求**：`do_fork()` 要求父进程必须处于 `RTS_RECEIVING` 状态（正在接收消息）。这是因为 fork 需要知道父进程的消息缓冲区地址（`p_delivermsg_vir`），以便将子进程的 endpoint 传递给 PM。若父进程不在接收状态，`do_fork()` 返回 `EINVAL`。

**子进程的初始不可运行状态**：fork 后子进程被设置 `RTS_NO_QUANTUM`（无时间片）和可能的 `RTS_NO_PRIV`（无特权）及 `RTS_VMINHIBIT`（等待 VM 设置页表）。这些标志确保子进程在 PM 和 VM 完成初始化前不会运行。

**系统进程 fork 的特权降级**：若父进程是系统进程（`s_flags & SYS_PROC`），子进程被降级为用户进程（`p_priv = priv_addr(USER_PRIV_ID)`），并设置 `RTS_NO_PRIV`。PM 需要在 exec 前通过 `sys_privctl` 重新设置特权。

**exec 的无回复语义**：`do_exec()` 不回复调用方（不写回返回消息）。这是因为 exec 后进程的整个地址空间已被替换，原来的消息缓冲区不再有效。PM 通过其他机制（如通知）确认 exec 完成。

### 1.4 行为规则

1. **父进程必须接收中**：`do_fork()` 要求 `RTS_ISSET(rpp, RTS_RECEIVING)` 为真
2. **子进程槽位必须空闲**：`isemptyp(rpc)` 必须为真
3. **Generation 递增**：子进程的 endpoint generation 从槽位当前值递增 1，回绕到 1
4. **子进程返回值 0**：`rpc->p_reg.retreg = 0`，使子进程从 fork 返回 0
5. **子进程不可运行**：fork 后子进程有 `RTS_NO_QUANTUM`，PM/VM 完成初始化后才可运行
6. **系统进程子进程降级**：父进程为系统进程时，子进程降级为用户特权
7. **VMINHIBIT 条件设置**：若 fork flags 含 `PFF_VMINHIBIT`，子进程设置 `RTS_VMINHIBIT`
8. **exec 不回复**：`do_exec()` 清除 `RTS_RECEIVING` 但不写回返回消息
9. **exec 清除 FPU 状态**：exec 后 FPU 标记为未初始化，下次使用时重新初始化

## 2. C 源码分析

### 2.1 相关定义（常量、配置等）

#### 2.1.1 sys_fork 消息字段

| 字段宏 | 消息字段 | 含义 |
|--------|---------|------|
| `m_lsys_krn_sys_fork.endpt` | — | 父进程 endpoint |
| `m_lsys_krn_sys_fork.slot` | — | 子进程槽位号 |
| `m_lsys_krn_sys_fork.flags` | — | fork 标志（PFF_VMINHIBIT 等） |
| `m_krn_lsys_sys_fork.endpt` | — | 返回：子进程 endpoint |
| `m_krn_lsys_sys_fork.msgaddr` | — | 返回：父进程消息缓冲区地址 |

#### 2.1.2 sys_exec 消息字段

| 字段宏 | 消息字段 | 含义 |
|--------|---------|------|
| `m_lsys_krn_sys_exec.endpt` | — | 目标进程 endpoint |
| `m_lsys_krn_sys_exec.stack` | — | 新栈指针 |
| `m_lsys_krn_sys_exec.name` | — | 程序名指针（调用方地址空间） |
| `m_lsys_krn_sys_exec.ip` | — | 新指令指针（入口点） |
| `m_lsys_krn_sys_exec.ps_str` | — | ps_strings 结构指针 |

#### 2.1.3 fork 标志

| 标志 | 含义 |
|------|------|
| `PFF_VMINHIBIT` | 子进程需要等待 VM 设置页表后才能运行 |

### 2.2 核心数据结构

#### 2.2.1 fork 操作涉及的 proc 字段

**复制自父进程的字段**（通过 `*rpc = *rpp` 整体复制）：

| 字段 | 复制后的值 | 后续处理 |
|------|-----------|---------|
| `p_reg` | 父进程寄存器 | `retreg` 被设为 0 |
| `p_seg` | 父进程段帧 | CR3/TTBR 被清零 |
| `p_endpoint` | 父进程 endpoint | 被 `_ENDPOINT(gen, p_nr)` 覆盖 |
| `p_priv` | 父进程特权 | 系统进程子进程被降级 |
| `p_name` | 父进程名 | 追加 "*F" 后缀 |
| `p_rts_flags` | 父进程 RTS 标志 | 被修改（见下） |

**fork 后重置的字段**：

| 字段 | 重置值 | 原因 |
|------|--------|------|
| `p_nr` | `slot` 参数 | 整体复制覆盖了，需恢复 |
| `p_endpoint` | `_ENDPOINT(gen, p_nr)` | 新 endpoint |
| `p_reg.retreg` | 0 | 子进程 fork 返回 0 |
| `p_user_time` | 0 | 子进程从 0 开始计时 |
| `p_sys_time` | 0 | 同上 |
| `p_cpu_time_left` | 0 | 无时间片 |
| `p_cycles` / `p_kcall_cycles` / `p_kipc_cycles` / `p_tick_cycles` | 0 | 清零统计 |
| `p_virt_left` / `p_prof_left` | 0 | 禁用虚拟/profile 定时器 |
| `p_misc_flags` | 清除定时器/追踪标志 | 子进程不继承定时器和追踪 |
| `p_seg.p_cr3` / `p_seg.p_cr3_v` | 0 / NULL | 页表由 VM 重新设置 |

**fork 后设置的 RTS 标志**：

| 标志 | 设置方式 | 含义 |
|------|---------|------|
| `RTS_NO_QUANTUM` | `RTS_SET` | 无时间片，等待调度器分配 |
| `RTS_NO_PRIV` | `\|=` | 系统进程子进程无特权 |
| `RTS_VMINHIBIT` | `RTS_SET`（条件） | 等待 VM 设置页表 |

**fork 后清除的 RTS 标志**：

| 标志 | 清除方式 | 含义 |
|------|---------|------|
| `RTS_SIGNALED` | `RTS_UNSET` | 不继承信号状态 |
| `RTS_SIG_PENDING` | `RTS_UNSET` | 不继承待处理信号 |
| `RTS_P_STOP` | `RTS_UNSET` | 不继承 ptrace 停止 |

### 2.3 关键函数分析

#### 2.3.1 do_fork()——内核 fork 处理

`minix3/minix/kernel/system/do_fork.c:26-134`

```c
int do_fork(struct proc *caller, message *m_ptr)
```

**功能**：在内核进程表中创建子进程条目。

**行为**（按执行顺序）：

1. **验证父进程 endpoint**：`isokendpt(endpt, &p_proc)` → `rpp = proc_addr(p_proc)`
2. **验证子进程槽位**：`rpc = proc_addr(slot)`，父进程非空且子进程槽位空闲
3. **验证父进程接收状态**：`RTS_ISSET(rpp, RTS_RECEIVING)` 必须为真
4. **保存 FPU 上下文**：`save_fpu(rpp)` 确保父进程 FPU 状态已保存
5. **整体复制 proc 结构**：`*rpc = *rpp`
6. **恢复 FPU 保存区指针**：x86 下 `rpc->p_seg.fpu_state = old_fpu_save_area_p`（子进程需要自己的 FPU 缓冲区）
7. **复制 FPU 状态**：若父进程使用过 FPU，将 FPU 状态复制到子进程的缓冲区
8. **递增 generation**：`++gen`，若超过 `_ENDPOINT_MAX_GENERATION` 则回绕到 1
9. **恢复子进程号**：`rpc->p_nr = slot`（被整体复制覆盖了）
10. **设置新 endpoint**：`rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr)`
11. **设置子进程返回值**：`rpc->p_reg.retreg = 0`
12. **重置时间和统计**：`p_user_time = 0`, `p_sys_time = 0`, `p_cpu_time_left = 0` 等
13. **清除定时器和追踪**：`p_misc_flags &= ~(MF_VIRT_TIMER | MF_PROF_TIMER | ...)`，`p_virt_left = 0`
14. **追加进程名后缀**：`strcat(rpc->p_name, "*F")`
15. **设置不可运行标志**：`RTS_SET(rpc, RTS_NO_QUANTUM)`，`reset_proc_accounting(rpc)`
16. **系统进程子进程降级**：若 `priv(rpp)->s_flags & SYS_PROC`，`rpc->p_priv = priv_addr(USER_PRIV_ID)`，`RTS_NO_PRIV`
17. **返回子进程信息**：`m_ptr->m_krn_lsys_sys_fork.endpt = rpc->p_endpoint`，`msgaddr = rpp->p_delivermsg_vir`
18. **条件设置 VMINHIBIT**：若 `flags & PFF_VMINHIBIT`，`RTS_SET(rpc, RTS_VMINHIBIT)`
19. **清除信号和追踪标志**：`RTS_UNSET(rpc, RTS_SIGNALED | RTS_SIG_PENDING | RTS_P_STOP)`，`sigemptyset(&rpc->p_pending)`
20. **清除页表指针**：x86 下 `p_cr3 = 0`, `p_cr3_v = NULL`；ARM 下 `p_ttbr = 0`, `p_ttbr_v = NULL`

#### 2.3.2 do_exec()——内核 exec 处理

`minix3/minix/kernel/system/do_exec.c:20-59`

```c
int do_exec(struct proc *caller, message *m_ptr)
```

**功能**：进程执行新程序后更新内核中的进程上下文。

**行为**（按执行顺序）：

1. **验证进程 endpoint**：`isokendpt(endpt, &proc_nr)` → `rp = proc_addr(proc_nr)`
2. **清除待投递消息**：若 `MF_DELIVERMSG` 置位，清除之（旧地址空间的消息不再有效）
3. **复制程序名**：`data_copy()` 从调用方地址空间复制进程名到内核栈，失败则用 `"<unset>"`
4. **初始化进程上下文**：`arch_proc_init(rp, ip, stack, ps_str, name)` 设置新的寄存器上下文
5. **解除接收阻塞**：`RTS_UNSET(rp, RTS_RECEIVING)`（exec 后不再等待回复）
6. **清除 FPU 初始化标志**：`rp->p_misc_flags &= ~MF_FPU_INITIALIZED`
7. **释放 FPU 所有权**：`release_fpu(rp)`（若当前进程是 FPU 拥有者）

**arch_proc_init()** 函数（x86）：
- 设置 `p_reg.pc = ip`（新的指令指针）
- 设置 `p_reg.sp = stack`（新的栈指针）
- 设置 `p_reg.ps_str`（ps_strings 指针）
- 复制进程名到 `p_name`
- 重置段寄存器为用户态默认值

### 2.4 调用关系/调用点分析

#### 2.4.1 fork 完整路径

```
用户进程调用 fork()
  └─ PM 处理 fork 请求
       ├─ PM 分配进程表槽位
       ├─ PM 调用 sys_fork(parent_ep, child_slot, flags)
       │    └─ do_fork()
       │         ├─ 验证父进程和子进程槽位
       │         ├─ 复制 proc 结构
       │         ├─ 递增 generation
       │         ├─ 设置 RTS_NO_QUANTUM / RTS_NO_PRIV / RTS_VMINHIBIT
       │         └─ 返回子进程 endpoint
       ├─ PM 通知 VM 设置子进程页表
       │    └─ VM: sys_vmctl(VMCTL_VMINHIBIT_CLEAR)
       ├─ PM 设置子进程特权（系统进程）
       │    └─ sys_privctl()
       └─ PM 调度子进程
            └─ sys_schedule()
```

#### 2.4.2 exec 完整路径

```
用户进程调用 execve()
  └─ PM 处理 exec 请求
       ├─ PM 加载新程序映像
       ├─ PM 设置新的栈和内存映射
       ├─ PM 调用 sys_exec(proc_ep, stack, name, ip, ps_str)
       │    └─ do_exec()
       │         ├─ arch_proc_init() 设置新上下文
       │         ├─ RTS_UNSET(RTS_RECEIVING) 解除阻塞
       │         └─ 清除 FPU 状态
       └─ PM 通知内核新进程可运行
```

#### 2.4.3 fork 与 VM 的协作

```
do_fork() 设置 RTS_VMINHIBIT
  └─ 子进程不可运行
       └─ VM 创建子进程地址空间
            ├─ 复制父进程页表
            ├─ 设置子进程 CR3
            └─ sys_vmctl(VMCTL_VMINHIBIT_CLEAR)
                 └─ RTS_UNSET(RTS_VMINHIBIT)
                      └─ 子进程可运行（若其他标志也清除）
```

### 2.5 设计要点/特殊处理

#### 2.5.1 整体复制 + 选择性重置

`do_fork()` 使用 `*rpc = *rpp` 整体复制 proc 结构，然后选择性重置子进程特有的字段。这种"先复制后修改"的策略简单高效，但需要仔细追踪哪些字段需要重置——遗漏任何字段都可能导致子进程行为异常。

关键的重置包括：进程号（被覆盖了需恢复）、endpoint（需新 generation）、返回值（子进程返回 0）、时间统计（从 0 开始）、FPU 缓冲区指针（子进程需要独立缓冲区）、页表指针（由 VM 重新设置）。

#### 2.5.2 FPU 上下文的独立缓冲区

x86 下，每个进程的 `p_seg.fpu_state` 指向独立的 FPU 状态保存区。整体复制 proc 结构时，子进程的 `fpu_state` 指针被覆盖为父进程的缓冲区。`do_fork()` 在复制前保存子进程原有的 `fpu_state` 指针，复制后恢复它，然后将父进程的 FPU 状态内容复制到子进程的缓冲区。这确保了父子进程有独立的 FPU 状态保存区。

#### 2.5.3 系统进程 fork 的特权降级

系统进程 fork 时，子进程被降级为用户进程特权（`USER_PRIV_ID`），并设置 `RTS_NO_PRIV`。这是因为系统进程的特权结构包含敏感的权限信息（IPC 目标、内核调用掩码、I/O 端口等），不应自动继承给子进程。PM 需要在 exec 前通过 `sys_privctl` 为子进程设置新的特权。

#### 2.5.4 exec 的无回复设计

`do_exec()` 不写回返回消息，而是通过 `RTS_UNSET(rp, RTS_RECEIVING)` 解除进程的接收阻塞。这是因为 exec 后进程的地址空间已完全替换，原来的消息缓冲区地址不再有效。PM 通过其他机制（如发送通知消息）确认 exec 完成。

#### 2.5.5 fork 标志 PFF_VMINHIBIT

`PFF_VMINHIBIT` 标志告诉内核子进程需要等待 VM 设置页表后才能运行。这是 fork 与 VM 协作的关键机制：fork 后子进程的页表指针被清零，VM 需要为子进程创建新的地址空间（通常复制父进程的页表），完成后通过 `VMCTL_VMINHIBIT_CLEAR` 通知内核。

#### 2.5.6 进程名 "*F" 后缀

fork 后子进程的进程名追加 "*F" 后缀（如 "init*F"），用于调试和 `ps(1)` 输出中区分 fork 后尚未 exec 的子进程。exec 后进程名被替换为新程序名。
