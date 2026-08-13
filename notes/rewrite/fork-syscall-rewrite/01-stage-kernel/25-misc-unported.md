# 25-misc-unported: 杂项与未移植系统调用

> **分类**: 杂项
> **源码**: `minix3/minix/kernel/system/do_getinfo.c`, `do_trace.c`, `do_update.c`, `do_sprofile.c`, `do_unused.c`
> **关联 Rust**: `os/kernel/src/misc.rs`
> **前置**: 16-24 全部前序文档
> **C 总行数**: ~900 行

---

## Ch1: 概念

**核心问题**: 一个微内核有数十个系统调用，重写过程不可能一次性完成全部。内核如何对待"尚未移植"或"故意延迟"的调用，才能既不破坏用户态兼容性，又能渐进推进？

### 1.1 目标读者与前置知识

本篇面向已读 16-24 的读者。前置知识：
- `dispatch_*` 分发模式与 `KcallResult` 返回类型（13-syscall-dispatch）
- `data_copy_vmcheck` 跨地址空间拷贝（17/24）
- `ProcessTable`/`PrivTable`/`KProcess`（16-fork-exit、22-privilege）
- `RtsFlags`/`MiscFlags` 原子标志位（16）

### 1.2 "未移植"的三种语义

重写过程中，系统调用相对于 C 源码有三种状态：

| 状态 | 含义 | 返回值 | 示例 |
|------|------|--------|------|
| **完全实现** | 输入验证 + 核心逻辑都对齐 C | 正常返回 | `SYS_UNUSED`→`ENOSYS`；`T_STEP`→`MF_STEP`；`T_GETINS`→`data_copy_vmcheck` |
| **部分实现** | 输入验证完整对齐 C，核心数据搬运 DEFERRED | 验证通过后 `ENOSYS` | `T_GETUSER` priv-struct 分支（对齐检查后 `EFAULT`） |
| **完全未实现** | 调用号存在但无处理逻辑 | `ENOSYS` | `GET_IMAGE` 等 |

> **关键区分**: "部分实现"不是"占位 stub"。它真实执行了 C 的所有前置检查（endpoint 合法性、权限、对齐、状态机等），只是最后的数据搬运步骤依赖尚未就绪的子系统（Direct Map / arch trait）。这种"前置验证 + 后置 DEFERRED"模式让用户态提前发现参数错误（`EINVAL`/`EPERM`/`EFAULT`），而不是等到功能完整时才暴露。

### 1.3 系统调用的四类分组

| 分组 | 调用 | 共性 | C 处理函数 |
|------|------|------|-----------|
| 信息查询 | `SYS_GETINFO` | 多子请求，按 request 分派，data_copy 到用户 | `do_getinfo()` |
| 进程追踪 | `SYS_TRACE` | 多子请求，跨地址空间拷贝 + 进程表字段读写 | `do_trace()` |
| 进程更新 | `SYS_UPDATE` | RS 专用，进程槽位交换（live update） | `do_update()` |
| 性能分析 | `SYS_SPROF` | 统计采样，依赖时钟/NMI 子系统 | `do_sprofile()` |

### 1.4 redox 对照

不同微内核对"未实现调用"的处理策略不同：

- **redox**: scheme-based 架构——每个 scheme 自管理请求，未实现的请求自然返回错误；内核不集中维护"未实现调用表"。scheme 是独立的用户态进程，其请求处理失败直接反馈给调用方。
- **Minix3**: 集中式 `call_vec[NR_SYS_CALLS]` 分派表（system.c:52）——每个 `SYS_*` 通过 `map(call_nr, handler)` 宏注册到 `call_vec[]`，未注册的调用号在 `kernel_call_dispatch` 中检查 `call_vec[call_nr]` 为 NULL 时由 `do_unused` 兜底返回 `ENOSYS`。内核是系统调用的唯一入口。
- **minix-rs**: 沿用 Minix3 集中分派模型（对齐微内核架构），但用 Rust `enum` + `match` 替代 C 的函数指针表 + `switch`。"未实现"通过 `ENOSYS` 显式表达，"部分实现"通过"验证 + `ENOSYS`"表达。

### 1.5 本章不讲什么

- 具体子请求的字段语义（Ch2 详述）
- Rust 类型设计（Ch3 详述）
- 已在前序文档覆盖的调用（见下表）

### 1.6 已覆盖的系统调用

| 文档 | 覆盖的系统调用 |
|------|--------------|
| 16 | SYS_FORK, SYS_EXEC, SYS_EXIT, SYS_CLEAR, SYS_RUNCTL, SYS_SCHEDCTL, SYS_STATECTL |
| 17 | SYS_VIRCOPY, SYS_PHYSCOPY, SYS_SAFECOPYFROM, SYS_SAFECOPYTO, SYS_VSAFECOPY, SYS_UMAP, SYS_UMAP_REMOTE, SYS_VUMAP, SYS_MEMSET, SYS_SAFEMEMSET |
| 18 | SYS_KILL, SYS_GETKSIG, SYS_ENDKSIG, SYS_SIGSEND, SYS_SIGRETURN |
| 19 | SYS_IRQCTL, SYS_DEVIO, SYS_VDEVIO |
| 20 | SYS_TIMES, SYS_SETALARM, SYS_STIME, SYS_SETTIME, SYS_VTIMER |
| 22 | SYS_PRIVCTL |
| 23 | SYS_DATACOPY（IPC 过滤） |
| 24 | 跨地址空间运行时（VMREQUEST 机制） |

---

## Ch2: C 源码分析

### 2.1 文件清单与规模

| 文件 | 行数 | 核心函数 | 子请求数 |
|------|------|---------|---------|
| `system/do_getinfo.c` | 227 | `do_getinfo()` + `update_idle_time()` | 19 |
| `system/do_trace.c` | 208 | `do_trace()` + COPYFROMPROC/COPYTOPROC 宏 | 13 |
| `system/do_update.c` | 338 | `do_update()` + 7 helper | — |
| `system/do_sprofile.c` | 131 | `do_sprofile()` + `clean_seen_flag()` | 2 |
| `do_unused` | ~9 | `do_unused()` | — |

> 子请求数 = C `switch` 中实际 `case` 分支数（grep `^\s*case` 验证）。com.h 定义 24 个 `GET_*` 宏、ptrace.h 定义 19 个 `T_*`/`PT_*` 宏，但 C `do_*` 只处理其中一部分，其余落入 `default: return EINVAL`。
> `do_unused` 在 C 源码中可能内联于 system.c 分派表或由 `#if USE_*` 守卫，未找到独立文件。其行为是返回 `ENOSYS`。

### 2.2 do_getinfo：信息查询分派

`do_getinfo()` 是一个大型 switch，按 `request` 字段分派到 19 个子请求。每个子请求的核心模式是：**设置 `src_vir` + `length` → `data_copy_vmcheck` 拷贝到用户空间**。

**子请求全集（19 个，C 实际处理）**：

| 分类 | 子请求 | C 宏值 | C 数据源 | 长度 |
|------|--------|--------|---------|------|
| 内核信息 | `GET_KINFO` | 0 | `&kinfo` | `sizeof(struct kinfo)` |
| 内核信息 | `GET_MACHINE` | 12 | `&machine` | `sizeof(struct machine)` |
| 内核信息 | `GET_LOADINFO` | 15 | `&kloadinfo` | `sizeof(struct loadinfo)` |
| 内核信息 | `GET_CPUINFO` | 23 | `&cpu_info` | `sizeof(cpu_info)` |
| 内核信息 | `GET_HZ` | 18 | `&system_hz` | `sizeof(system_hz)` |
| 进程表 | `GET_PROC` | 11 | `proc_addr(nr)` | `sizeof(struct proc)` |
| 进程表 | `GET_PROCTAB` | 2 | `proc[]` | `sizeof(struct proc) * (NR_PROCS+NR_TASKS)` |
| 进程表 | `GET_REGS` | 24 | `&p->p_reg` | `sizeof(p->p_reg)` |
| 进程表 | `GET_PRIV` | 17 | `priv_addr(nr_to_id(nr))` | `sizeof(struct priv)` |
| 进程表 | `GET_PRIVTAB` | 8 | `priv[]` | `sizeof(struct priv) * NR_SYS_PROCS` |
| 调度/资源 | `GET_IMAGE` | 1 | `image` | `sizeof(struct boot_image) * NR_BOOT_PROCS` |
| 调度/资源 | `GET_MONPARAMS` | 4 | `kinfo.param_buf` | `sizeof(kinfo.param_buf)` |
| 调度/资源 | `GET_IRQHOOKS` | 6 | `irq_hooks` | `sizeof(struct irq_hook) * NR_IRQ_HOOKS` |
| 调度/资源 | `GET_IRQACTIDS` | 16 | `irq_actids` | `sizeof(irq_actids)` |
| 随机数 | `GET_RANDOMNESS` | 3 | `&copy`（先拷贝再清零原数据） | `sizeof(struct k_randomness)` |
| 随机数 | `GET_RANDOMNESS_BIN` | 20 | `&krandom.bin[bin]`（按 bin 索引） | `sizeof(krandom.bin[bin])` |
| 计时 | `GET_IDLETSC` | 21 | `&idl->p_cycles`（先 `update_idle_time`） | `sizeof(idl->p_cycles)` |
| 计时 | `GET_CPUTICKS` | 25 | `ticks[]`（`get_cpu_ticks(cpu)`） | `sizeof(uint64_t[MINIX_CPUSTATES])` |
| 身份 | `GET_WHOAMI` | 19 | **特殊：直接写 reply message**，不走 data_copy | — |

> **宏存在但 C 未处理**（落入 `default: return EINVAL`）：`GET_KENV=5`、`GET_KADDRESSES=9`、`GET_SCHEDINFO=10`、`GET_LOCKTIMING=13`、`GET_BIOSBUFFER=14`。这些值在 com.h:316-339 中有定义但 do_getinfo.c 无对应 `case`。Rust `GetInfoRequest` 也省略这些值，`TryFrom<i32>` 返回 `Err(())` → `EINVAL`，与 C 一致。
> 历史误植：早期文档列出 `GET_PROC2` 和 `GET_BIOSCTRS`，但 C 源码 grep 不到这两个名称——它们在 Minix3 中不存在（64-bit 重写时已移除 `GET_PROC2`；`GET_BIOSCTRS` 从未存在，仅有 `GET_BIOSBUFFER`）。

**特殊路径——GET_WHOAMI**：
`GET_WHOAMI` 不走 `data_copy_vmcheck`，而是直接写入 reply message 的 `m_krn_lsys_sys_getwhoami` union 成员：
```c
m_ptr->m_krn_lsys_sys_getwhoami.endpt = caller->p_endpoint;
m_ptr->m_krn_lsys_sys_getwhoami.privflags = priv(caller)->s_flags;
m_ptr->m_krn_lsys_sys_getwhoami.initflags = priv(caller)->s_init_flags;
strncpy(m_ptr->m_krn_lsys_sys_getwhoami.name, caller->p_name, len);
return OK;
```
这是因为 WHOAMI 数据量小（endpt + 2 flags + name），直接放 reply 比起一次 data_copy 更高效。

**验证逻辑**：
对 `GET_PROC`/`GET_PRIV`/`GET_REGS`，C 先检查 `val_len2_e == SELF` 则替换为 `caller->p_endpoint`，再用 `isokendpt` 验证 endpoint。验证失败返回 `EINVAL`。`GET_RANDOMNESS_BIN` 的 `val_len2_e` 是 bin 索引（非 endpoint），需检查 `0 ≤ bin < RANDOM_SOURCES`。`GET_CPUTICKS` 的 `val_len2_e` 是 CPU 索引，需检查 `cpu < CONFIG_MAX_CPUS`。

**长度检查**：
所有走 data_copy 的子请求，在拷贝前检查 `val_len > 0 && length > val_len` → 返回 `E2BIG`。

### 2.3 do_trace：进程追踪分派

`do_trace()` 处理 ptrace 请求。核心模式是：**验证 endpoint → 按 request 分派 → 跨地址空间拷贝或进程表字段读写**。

**子请求全集（13 个，C 实际处理）**：

| 分类 | 子请求 | C 宏值 | C 行为 | 数据流向 |
|------|--------|--------|--------|---------|
| 控制流 | `T_STOP` | -1 | `RTS_SET(PROC_STOP)` + 清 `MF_SC_TRACE\|MF_STEP` | 内核→内核 |
| 控制流 | `T_RESUME` | 7 | `RTS_UNSET(PROC_STOP)` + 写 `data=0` | 内核→内核 |
| 控制流 | `T_STEP` | 104 | `MF_STEP` + `RTS_UNSET(PROC_STOP)` + 写 `data=0` | 内核→内核 |
| 控制流 | `T_SYSCALL` | 14 | `MF_SC_TRACE` + `RTS_UNSET(PROC_STOP)` + 写 `data=0` | 内核→内核 |
| 控制流 | `T_DETACH` | 10 | 清 `MF_SC_ACTIVE` + fall through 到 `T_RESUME` | 内核→内核 |
| 内存读写 | `T_GETINS` | 1 | `COPYFROMPROC(tr_addr, &tr_data, sizeof(long))` | 目标进程→内核→reply |
| 内存读写 | `T_GETDATA` | 2 | `COPYFROMPROC(tr_addr, &tr_data, sizeof(long))` | 目标进程→内核→reply |
| 内存读写 | `T_SETINS` | 4 | `COPYTOPROC(tr_addr, &tr_data, sizeof(long))` | 内核→目标进程 |
| 内存读写 | `T_SETDATA` | 5 | `COPYTOPROC(tr_addr, &tr_data, sizeof(long))` | 内核→目标进程 |
| 内存读写 | `T_READB_INS` | 100 | `COPYFROMPROC(tr_addr, &ub, 1)` | 目标进程→内核→reply |
| 内存读写 | `T_WRITEB_INS` | 101 | `COPYTOPROC(tr_addr, &ub, 1)` | 内核→目标进程 |
| 进程表 | `T_GETUSER` | 102 | `*(long*)((char*)rp + tr_addr)` 或 `*(long*)((char*)rp->p_priv + offset)` + 对齐检查 | 进程表→reply |
| 进程表 | `T_SETUSER` | 103 | 写 `rp->p_reg` 字段 + 对齐检查 + **架构特定段寄存器保护** | reply→进程表 |

> **PM 处理而非内核**（不在 `do_trace.c` 中）：`T_OK=0`、`T_EXIT=8`（即 `PT_KILL`）、`T_ATTACH=9`、`T_SETOPT=105`、`T_GETRANGE=106`、`T_SETRANGE=107`。这些请求由 PM 处理，若到达内核 syscall 路径则落入 `TryFrom::Err` → `EINVAL`，与 C `default: return(EINVAL)` 一致。
> **宏值与 `PT_*` 别名**：`T_GETINS=PT_READ_I=1`、`T_GETDATA=PT_READ_D=2`、`T_SETINS=PT_WRITE_I=4`、`T_SETDATA=PT_WRITE_D=5`、`T_RESUME=PT_CONTINUE=7`、`T_DETACH=PT_DETACH=10`、`T_SYSCALL=PT_SYSCALL=14`。ptrace.h:227-247。

**COPYFROMPROC/COPYTOPROC 宏**：
两个宏封装了 `virtual_copy_vmcheck`，设置 `fromaddr`/`toaddr` 的 `proc_nr_e` 和 `offset`，调用 `virtual_copy_vmcheck(caller, &fromaddr, &toaddr, length)`。

**T_SETUSER 架构特定保护**：
`T_SETUSER` 写进程的 `p_reg` 字段时，x86 禁止写段寄存器（`cs/ds/es/gs/fs/ss`），arm 处理 `psr`。这是架构特定的安全约束——允许用户态改段寄存器会让进程逃逸到内核态。

**前置验证**：
```c
if(!isokendpt(tr_proc_nr_e, &tr_proc_nr)) return(EINVAL);  // endpoint 合法
if (iskerneln(tr_proc_nr)) return(EPERM);                   // 禁止追踪内核进程
rp = proc_addr(tr_proc_nr);
if (isemptyp(rp)) return(EINVAL);                            // 槽位非空
```

### 2.4 do_update：进程槽位交换

`do_update()` 用于 RS（Reincarnation Server）的 live update：将新版本系统服务的进程槽位替换旧版本。

**7 步流程**：

1. **endpoint 验证**：`isokendpt(src_e, &src_p)` + `isokendpt(dst_e, &dst_p)` → `EINVAL`
2. **SYS_PROC 权限**：`src_privp->s_flags & SYS_PROC` + `dst_privp->s_flags & SYS_PROC` → `EPERM`
3. **不可运行断言**：`assert(!proc_is_runnable(src) && !proc_is_runnable(dst))`
4. **updatable 检查**：`proc_is_updatable(src) && proc_is_updatable(dst)` → `EBUSY`
5. **权限继承**：`inherit_priv_irq/io/mem(src, dst)` — 将 src 的 IRQ/I/O/内存范围转移到 dst
6. **目标掩码继承**：遍历 `s_ipc_to` 位图，将 src 的 sendto 权限复制到 dst
7. **槽位交换**：
   - 保存原始状态（`orig_src_proc`/`orig_src_priv`/`orig_dst_proc`/`orig_dst_priv`）
   - `adjust_asyn_table` 双向调整异步消息表
   - 若 `SYS_UPD_ROLLBACK`：`abort_proc_ipc_send(src)` 中止 src 的 pending send
   - 交换槽位内容（`*src_rp = orig_dst_proc` 等）
   - `adjust_proc_slot`/`adjust_priv_slot` 保留 endpoint/nr/priv/caller_q/scheduler
   - `swap_proc_slot_pointer` 更新 per-CPU `ptproc` 指针
   - `swap_memreq` 更新 VM request 链表中的进程指针
   - SMP：`bits_fill(p_stale_tlb)` 标记 TLB 需刷新

**`proc_is_updatable` 宏**：
```c
#define proc_is_updatable(p) \
    (RTS_ISSET(p, RTS_NO_PRIV) || RTS_ISSET(p, RTS_SIG_PENDING) \
    || (RTS_ISSET(p, RTS_RECEIVING) && !RTS_ISSET(p, RTS_SENDING)))
```
含义：进程必须处于"静止"状态——要么无内核权限（用户态）、要么有 pending 信号、要么在 receive 但不在 send。正在执行内核代码的进程不能被交换。

### 2.5 do_sprofile：统计性能分析状态机

`do_sprofile()` 是一个简单的状态机，控制统计采样的启停。

**PROF_START 流程**：
1. 检查 `sprofiling` 是否已运行 → `EBUSY`
2. `isokendpt(endpt, &proc_nr)` → `EINVAL`
3. 设置参数（`sprof_ep`/`sprof_info_addr_vir`/`sprof_data_addr_vir`/`sprof_mem_size`）
4. 重置计数器（`sprof_info` 各字段清零）
5. 按 `intr_type` 初始化：`PROF_RTC`→`init_profile_clock(freq)`；`PROF_NMI`→`nmi_watchdog_start_profiling(freq)`；default→`EINVAL`
6. `sprofiling = 1`
7. `clean_seen_flag()` 清除所有进程的 `MF_SPROF_SEEN`

**PROF_STOP 流程**：
1. 检查 `!sprofiling` → `EBUSY`
2. `sprofiling = 0`
3. 按 `sprofiling_type` 停止：`PROF_RTC`→`stop_profile_clock()`；`PROF_NMI`→`nmi_watchdog_stop_profiling()`
4. `data_copy` 将 `sprof_info` 和采样缓冲区拷贝到用户空间
5. `clean_seen_flag()`

**全局状态**：
- `int sprofiling`：全局运行标志（C 隐式依赖 BKL 保护）
- `int sprofiling_type`：当前采样中断源
- `endpoint_t sprof_ep`：调用方 endpoint
- `vir_bytes sprof_info_addr_vir`/`sprof_data_addr_vir`：用户空间缓冲区地址
- `struct sprof_info sprof_info`：采样统计
- `char *sprof_sample_buffer`：采样数据缓冲区

### 2.6 do_unused：兜底

`do_unused()` 对所有未实现的系统调用号返回 `ENOSYS`。这是 C 分派表的兜底项，确保未知调用号有确定行为而非崩溃。

---

## Ch3: Rust 设计决策

### 3.1 D1: 子请求用 enum + TryFrom<i32>

**C**: `switch(m_ptr->request)` 用裸 int，default 分支返回 `EINVAL`。

**Rust**: `GetInfoRequest`/`TraceRequest`/`ProfAction`/`ProfIntrType` enum + `impl TryFrom<i32>`。

**理由**：
- 编译期穷尽性检查——新增子请求时 `match` 必须覆盖，否则编译失败
- 无效值在入口被拒绝（`Err(())` → `EINVAL`），不会落入 default 分支
- 类型安全——`GetInfoRequest::KInfo` 是独立类型，不会与 `TraceRequest::GetIns` 混淆

> design.md §D1 ↔ misc.rs:54-124

### 3.2 D2: 未实现调用返回 ENOSYS（对齐 C do_unused）

**C**: `do_unused()` 返回 `ENOSYS`。

**Rust**: `dispatch_unused()` 返回 `KcallResult::Ok(ENOSYS)`。

**理由**：用户态可据此判断"功能是否存在"。`ENOSYS` 是 POSIX 标准的"功能未实现"错误码，用户态库（如 libc）会据此决定是否回退到其他实现。

> design.md §D2 ↔ misc.rs:962-964

### 3.3 D3: "前置验证 + 后置 DEFERRED" 模式

**适用**: `SYS_TRACE`/`SYS_UPDATE`/`SYS_SPROF`。

**C**: 验证 + 核心逻辑一体，无"部分实现"概念。

**Rust**: 验证完整对齐 C（endpoint/权限/对齐/状态机），核心数据搬运（`data_copy_vmcheck`/槽位交换/时钟初始化）返回 `ENOSYS`。

**理由**：
- 让参数错误尽早暴露——用户态立即收到 `EINVAL`/`EPERM`/`EFAULT`，而非等到功能完整时才发现
- 避免静默成功——返回 `OK` 但无数据会让用户态误以为成功
- 测试前置验证——验证逻辑可独立测试，不依赖未就绪子系统

> design.md §D3 ↔ misc.rs:584-641 (trace DEFERRED) / 764 (update DEFERRED) / 913,929 (sprof DEFERRED)

### 3.4 D4: 宏存在但 C 未处理的 GET_* 统一返回 EINVAL

**调用**: `GET_KENV=5`/`GET_KADDRESSES=9`/`GET_SCHEDINFO=10`/`GET_LOCKTIMING=13`/`GET_BIOSBUFFER=14`。

**理由**：这些宏在 com.h:316-339 中有定义但 C `do_getinfo` 无对应 `case`，落入 `default: return EINVAL`。Rust `GetInfoRequest` 也省略这些值，`TryFrom<i32>` 返回 `Err(())` → `EINVAL`，与 C 一致。**不是"x86-only"**：这些值在所有架构下都返回 `EINVAL`，与 x86-only 的 `SYS_READBIOS`/`SYS_IOPENABLE`/`SYS_SDEVIO`（那些返回 `ENOSYS`）语义不同。

### 3.5 D5: SYS_TRACE 部分实现（非完全延迟）

**C**: `do_trace()` 13 个子请求一体实现。

**Rust**: enum 覆盖全部 13 个变体，按依赖关系分三层：
- **已实现**（纯 flag/RTS 操作 + reply data 写回）：`T_STOP`/`T_RESUME`/`T_STEP`/`T_SYSCALL`/`T_DETACH`
- **已实现**（跨地址空间拷贝，接入 `data_copy_vmcheck`）：`T_GETINS`/`T_GETDATA`/`T_SETINS`/`T_SETDATA`/`T_READB_INS`/`T_WRITEB_INS`（字节级拷贝，无对齐要求，支持 VMSUSPEND 恢复路径）
- **对齐检查 only**（字段访问 DEFERRED）：`T_GETUSER`（proc-struct + priv-struct 分支均已实现：经 `ProcInfoStruct`/`PrivInfoStruct` 快照 + `read_word_at_offset` 读取，对齐 C do_trace.c:108-123）+ `T_SETUSER`（已实现：arch trait `write_user_register` 负责段寄存器保护与 PSW 位掩码）

**理由**：
- flag 操作（`MF_STEP`/`RTS_P_STOP`/`MF_SC_TRACE`/`MF_SC_ACTIVE`）通过 `MiscFlags::set`/`RtsFlags::clear` 原子 API 即可实现，无需跨地址空间拷贝。reply `data=0` 通过 `write_trace_reply_data(msg, 0)` 写回。
- `T_GETUSER`/`T_SETUSER` 的对齐检查是 C 显式前置检查（`do_trace.c:106`/`137`），独立于字段访问，可单独实现并测试。
- 内存读写 `T_GETINS` 等的 `COPYFROMPROC`/`COPYTOPROC` 是字节级 `virtual_copy`，无对齐要求。Rust 通过 `data_copy_vmcheck` 实现相同语义（Direct Map + PTE walk），并额外支持 VMSUSPEND（C 的 `virtual_copy` 在页未映射时返回 EFAULT；`data_copy_vmcheck` 请求 VM 处理页缺失后重试）。

> design.md §D5 ↔ misc.rs:474-644

### 3.6 D6: GET_WHOAMI 直接写 reply message

**C**: 直接写 `m_ptr->m_krn_lsys_sys_getwhoami.*`，不走 data_copy。

**Rust**: `unsafe { msg.m_u.m_krn_lsys_sys_getwhoami = MessKrnLsysSysGetwhoami { ... } }`。

**理由**：对齐 C 特殊路径。WHOAMI 数据量小（endpt + 2 flags + 44 字节 name），直接放 reply union 比起一次 data_copy 更高效。

> design.md §D6 ↔ misc.rs:270-296

### 3.7 D7: GET_KINFO 用 M4 格式返回关键字段（临时方案）

**C**: 整个 `struct kinfo` 通过 `data_copy_vmcheck` 拷贝到用户空间。

**Rust**: 当前用 `MessageM4` 返回 5 个关键字段（`nr_procs`/`nr_tasks`/`user_sp`/`freepde_start`/`vir_kern_start`）。

**缺口**：待 `data_copy_vmcheck` 到用户空间的路径落地后，改为完整 `struct kinfo` 拷贝。

**理由**：PM 启动需要 `kinfo` 关键字段（`nr_procs`/`nr_tasks` 用于进程表大小，`user_sp` 用于栈顶，`freepde_start`/`vir_kern_start` 用于地址空间布局），无需等完整 data_copy 路径。

> design.md §D7 ↔ misc.rs:298-328

### 3.8 D8: SPROFILING 用 AtomicBool（SMP 安全）

**C**: `int sprofiling`（隐式 BKL 保护）。

**Rust**: `pub static SPROFILING: AtomicBool` + `compare_exchange` 状态机。

**理由**：
- 显式 SMP 安全——`AtomicBool` 的 `compare_exchange` 提供原子 check-and-set
- 不依赖隐式 BKL 假设——即使未来 BKL 拆分，状态机仍然正确
- 状态转换原子化——`PROF_START` 的"检查 + 设置"和 `PROF_STOP` 的"检查 + 清除"都是原子的

> design.md §D8 ↔ misc.rs:955 + 866-936

### 3.9 D9: 消息字段类型化访问（禁 m1 overlay）

**C**: `m_ptr->m_lsys_krn_sys_trace.*` / `m_ptr->m_lsys_krn_sys_getinfo.*`。

**Rust**: `msg.m_u.m_lsys_krn_sys_trace` / `msg.m_u.m_lsys_krn_sys_getinfo`。

**禁止**: `msg.m_u.m_m1` overlay。

**理由**：`mess_lsys_krn_sys_trace` 的字段布局（`request@0`/`endpt@4`/`address@8`/`data@16`）与 `MessageM1`（`m1i1@0`/`m1i2@4`/`m1i3@8`/`m1p1@16`）不同。用 `m1` overlay 会把 `request` 读成 `endpt`（P0 字段映射 bug）。类型化访问通过 union 成员名显式选择正确布局。

> design.md §D9 ↔ misc.rs:208-247

---

## Ch4: 实现详解

### 4.1 GetInfoRequest enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum GetInfoRequest {
    KInfo = 0,          // GET_KINFO
    Image = 1,          // GET_IMAGE
    ProcTab = 2,        // GET_PROCTAB
    Randomness = 3,     // GET_RANDOMNESS
    MonParams = 4,      // GET_MONPARAMS
    IrqHooks = 6,       // GET_IRQHOOKS
    PrivTab = 8,        // GET_PRIVTAB
    Proc = 11,          // GET_PROC
    Machine = 12,       // GET_MACHINE
    LoadInfo = 15,      // GET_LOADINFO
    IrqActids = 16,     // GET_IRQACTIDS
    Priv = 17,          // GET_PRIV
    Hz = 18,            // GET_HZ
    WhoAmI = 19,        // GET_WHOAMI
    RandomnessBin = 20, // GET_RANDOMNESS_BIN
    IdleTsc = 21,       // GET_IDLETSC
    CpuInfo = 23,       // GET_CPUINFO
    Regs = 24,          // GET_REGS
    CpuTicks = 25,      // GET_CPUTICKS
}
```

> design.md §D1 ↔ misc.rs:54-95

**覆盖说明**：enum 列出 19 个变体，与 C `do_getinfo` 实际处理的 19 个 `case` 一一对应。`#[repr(i32)]` 值与 com.h:316-339 中的 `GET_*` 宏严格匹配（**IPC 协议约束**：用户态 libsys 以裸 int 传递，mismatch 会导致错误数据或 EINVAL）。

**宏存在但 enum 省略**（D4）：`GET_KENV=5`/`GET_KADDRESSES=9`/`GET_SCHEDINFO=10`/`GET_LOCKTIMING=13`/`GET_BIOSBUFFER=14` — 这些值在 `TryFrom<i32>` 中返回 `Err(())` → `EINVAL`，与 C `default` 分支一致。

**实现状态**：
- `WhoAmI`：已完整实现（D6，直接写 reply）
- `KInfo`：临时用 M4 返回 5 字段（D7）
- `Hz`/`LoadInfo`/`Machine`/`CpuInfo`/`CpuTicks`：已实现（经 `copy_struct_to_caller<T>` + `data_copy_vmcheck` 拷贝到用户空间；详见 §4.2）。`Hz` 拷 `system_hz`；`LoadInfo` 构造 `LoadInfoStruct`（180 项 `u16` + `u16` + `u64`，`#[repr(C)]`）取自 `clock_state.load_history()`；`Machine` 构造 `MachineStruct`（`#[repr(C)]`，processors_count + bsp_id + padding + apic_enabled + acpi_rsdp + board_id）取自 SMP 状态；`CpuInfo` 构造 `CpuInfoEntry` 数组（`#[repr(C)]`，cpu_id + cpu_cycles + cpu_load）；`CpuTicks` 拷 `[u64; MINIX_CPUSTATES=5]`（当前零值，待 `get_cpu_ticks` 接线）
- `Proc`/`Priv`/`Regs`：endpoint 验证已实现，data_copy DEFERRED（需 C 兼容 `struct proc`/`struct priv`/`reg_t`/CpuContext 布局）
- `Randomness`/`RandomnessBin`：✅ 已实现（`misc.rs:1008-1054`，经 `crate::krandom::try_krandom()` + `wipe_all`/`wipe_bin` 拷贝后清零，详见 §4.7）
- 其余 6 个（`Image`/`ProcTab`/`MonParams`/`IrqHooks`/`PrivTab`/`IrqActids`/`IdleTsc`）：直接 `ENOSYS`（待各自基础设施；其中 `ProcTab`/`PrivTab` 有显式 `case` 但无验证，其余落入 `_ =>` catch-all）

### 4.2 dispatch_getinfo 分派

```rust
pub fn dispatch_getinfo(
    caller: &mut KProcess,
    msg: &mut Message,
    priv_table: &PrivTable,
    proc_table: &ProcessTable,
    clock_state: &ClockState,
) -> KcallResult
```

> design.md §D6/D7/D9 ↔ misc.rs:349-559

**分派逻辑**：
- `WhoAmI`：直接写 `m_krn_lsys_sys_getwhoami` reply（D6）
- `KInfo`：用 `MessageM4` 返回 5 字段（D7 临时方案）
- `Hz`：从 `clock_state.system_hz()` 取 `i32`，经 `copy_struct_to_caller` 拷贝（misc.rs:528-529）
- `LoadInfo`：从 `clock_state.load_history()` 构造 `LoadInfoStruct`（`#[repr(C)]`，misc.rs:455-466），经 `copy_struct_to_caller` 拷贝
- `Machine`：从 SMP 状态构造 `MachineStruct`（`#[repr(C)]`，misc.rs:537-542），经 `copy_struct_to_caller` 拷贝
- `CpuInfo`：构造 `CpuInfoEntry` 数组（`#[repr(C)]`，misc.rs:547-557），经 `copy_struct_to_caller` 拷贝
- `CpuTicks`：拷 `[u64; MINIX_CPUSTATES=5]`（当前零值，待 `get_cpu_ticks` 接线，misc.rs:511-524），经 `copy_struct_to_caller` 拷贝
- `Proc`/`Priv`/`Regs`：验证 endpoint（`SELF` 替换 + `isokendpt`）→ `ENOSYS`（需 C 兼容 `struct proc`/`struct priv`/`reg_t` 布局）
- `Randomness`：✅ 已实现（`misc.rs:1008-1024`）—— 快照整个 `KRandomness`（2184 字节）后 `wipe_all()` 清零所有 bin，再 `copy_struct_to_caller` 拷贝快照到用户空间。`try_krandom()` 返回 `None` 时返回 `EINVAL`（boot 未完成）
- `RandomnessBin`：✅ 已实现（`misc.rs:1026-1054`）—— 验证 `0 ≤ bin < RANDOM_SOURCES(16)` → `EINVAL`；`r_size < RANDOM_ELEMENTS` 时返回 `ENOENT`（bin 未满）；快照单 bin 后 `wipe_bin(bin_idx)` 清零，再 `copy_struct_to_caller` 拷贝
- `ProcTab`/`PrivTab`/`Image`/`MonParams`/`IrqHooks`/`IrqActids`/`IdleTsc`：直接 `ENOSYS`（待各自基础设施）

**`copy_struct_to_caller<T>` 通用 helper**（misc.rs:313）：封装 `GET_*` 子请求共有的"E2BIG 检查 + `data_copy_vmcheck` 从内核栈拷到用户空间"模式。`dispatch_getinfo` 现接收 `clock_state: &ClockState` 参数（与 `dispatch_setalarm` 对齐），供 `Hz`/`LoadInfo` 读取时钟状态。

**消息访问**：用 `msg_getinfo(msg)` 辅助函数读取 `m_lsys_krn_sys_getinfo` union 成员（D9）。

### 4.3 dispatch_trace 分派

```rust
pub fn dispatch_trace(
    _caller: &mut KProcess,
    msg: &mut Message,
    proc_table: &mut ProcessTable,
    priv_table: &PrivTable,
) -> KcallResult
```

> design.md §D3/D5/D9 ↔ misc.rs:474-644

**前置验证**（对齐 C do_trace.c:83-87）：
1. `TraceRequest::try_from(request)` → `EINVAL`
2. `proc_table.endpoint_to_nr(target_endpoint)` → `EINVAL`
3. `ProcessTable::is_kernel(target_nr)` → `EPERM`
4. 槽位非空（`endpoint_to_nr` 成功即保证）

**分派逻辑**（按 D5 三层）：
- `Stop`：`target.p_rts_flags.set(PROC_STOP)` + `clear(SC_TRACE|STEP)`（对齐 C do_trace.c:89-93）
- `Resume`：`clear(PROC_STOP)` + `write_trace_reply_data(msg, 0)`（对齐 C do_trace.c:174-177）
- `Step`：`set(STEP)` + `clear(PROC_STOP)` + `write_trace_reply_data(msg, 0)`（对齐 C do_trace.c:179-183）
- `Syscall`：`set(SC_TRACE)` + `clear(PROC_STOP)` + `write_trace_reply_data(msg, 0)`（对齐 C do_trace.c:185-189）
- `Detach`：`clear(SC_ACTIVE)` + `clear(PROC_STOP)` + `write_trace_reply_data(msg, 0)`（对齐 C do_trace.c:170-177，C 中 fall through 到 `T_RESUME`）
- `GetIns`/`GetData`：`data_copy_vmcheck` 从目标进程拷贝 `sizeof(long)` 字节到内核 buffer，结果写入 reply `data` 字段（对齐 C do_trace.c:95-103 COPYFROMPROC）。页缺失时返回 `VmSuspend`（C 返回 EFAULT）。
- `SetIns`/`SetData`：`data_copy_vmcheck` 从内核 buffer 拷贝 `sizeof(long)` 字节到目标进程，reply `data=0`（对齐 C do_trace.c:126-134 COPYTOPROC）。
- `ReadBIns`/`WriteBIns`：同上但拷贝 1 字节（对齐 C do_trace.c:191-200）。
- `GetUser`：对齐检查（`tr_addr & WORD_MASK != 0` → `EFAULT`，对齐 C do_trace.c:106）→ proc-struct 分支从 `ProcInfoStruct` 快照读取 `u64`（已实现）→ priv-struct 分支从 `PrivInfoStruct` 快照读取 `u64`（已实现：`dispatch_trace` 签名新增 `priv_table: &PrivTable` 参数，经 `rp.priv_id` → `priv_table.get(pid)` → `PrivInfoStruct::from_kpriv` 构造快照，对齐 C do_trace.c:117-123 的 `sizeof(struct proc)` 向上对齐 + 偏移减法逻辑）
- `SetUser`：对齐检查（`tr_addr & WORD_MASK != 0` → `EFAULT`，对齐 C do_trace.c:137）→ 调用 `CpuContextArch::write_user_register(&mut rp.cpu_context, tr_addr, tr_data)` 写入寄存器保存区（已实现，arch 层负责段寄存器保护与 PSW 位掩码）

**`write_trace_reply_data`**：写入 reply 消息的 `data` 字段。C 使用 `m_krn_lsis_sys_trace.data`（reply union 成员）；Rust 写入 `msg.m_u.m_lsys_krn_sys_trace.data`（request union 成员），二者在 `#[repr(C)]` union 中位于相同字节偏移（`data@16`），因此等价。`msg` 必须 `&mut` 以支持此写回。

**WORD_SIZE**：`const WORD_SIZE: u64 = 8`（minix-rs 64-bit only，C 的 `sizeof(long)` 在 64 位下也是 8）。

### 4.4 dispatch_update（完整实现）

```rust
pub fn dispatch_update(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
    priv_table: &mut PrivTable,
) -> KcallResult
```

> design.md §D3 ↔ misc.rs:1553-1797 (`dispatch_update`) + misc.rs:1819-1831 (`proc_is_updatable`)

**7 步验证**（对齐 C do_update.c:55-79）：
1. `isokendpt(src_e)` → `EINVAL`
2. `src.is_sys_proc()` → `EPERM`
3. `isokendpt(dst_e)` → `EINVAL`
4. `dst.is_sys_proc()` → `EPERM`
5. `src == dst` → `EINVAL`（**Rust 新增**：C 无此检查；C 的 `assert(!proc_is_runnable(src) && !proc_is_runnable(dst))` 检查的是可运行性而非自交换，且仅 debug 构建生效。Rust 省略可运行性断言因为 `proc_is_updatable` 是更严格条件）
6. `proc_is_updatable(src) && proc_is_updatable(dst)` → `EBUSY`
7. 提取 `SYS_UPD_ROLLBACK` flag

**`proc_is_updatable` 纯函数**：
```rust
pub fn proc_is_updatable(p: &KProcess) -> bool {
    let flags = &p.p_rts_flags;
    if flags.is_set(NO_PRIV) || flags.is_set(SIG_PENDING) { return true; }
    if flags.is_set(RECEIVING) && !flags.is_set(SENDING) { return true; }
    false
}
```
对齐 C 宏 `proc_is_updatable(p)`（do_update.c:18-20）。Rust 用纯函数便于测试——不需要构造完整 `ProcessTable` 即可验证逻辑。

**已实现**：槽位交换体（步骤 8-12）全部完成：
- `inherit_priv_irq/io/mem`：`KPriv::add_irq/add_io/add_mem` 方法（dedup + CHECK_* flag）
- `s_ipc_to` 目标掩码：OR 合并到 dst 的 `ipc.s_ipc_to`
- `abort_proc_ipc_send`：清除 `RTS_SENDING` + `SenderQueue::remove_by_nr` 从 target 的 caller_q 移除
- 槽位交换：`ProcessTable::swap_slots` + `PrivTable::swap_slots`（`core::mem::swap` + `split_at_mut`）
- `adjust_proc_slot`：恢复 endpoint/nr/priv_id/caller_q/scheduler/cpu/cpu_mask（`caller_q` 通过 `mem::replace` 提取/恢复）
- `adjust_priv_slot`：恢复 s_id/s_proc_nr/pending bits/diag_sig
- `swap_proc_slot_pointer`（ptproc）no-op：两进程均非 runnable，ptproc 不指向它们
- `swap_memreq` no-op：vmrequest 全局链未实现；两进程非 runnable，VMREQUEST 通常未设置
- `adjust_asyn_table` 跳过：C 中失败仅打印 warning（非致命），需跨地址空间 `data_copy`，仅在 live update 场景触发

### 4.5 dispatch_profile 状态机

```rust
pub fn dispatch_profile(
    _caller: &mut KProcess,
    msg: &Message,
    proc_table: &ProcessTable,
) -> KcallResult
```

> design.md §D3/D8 ↔ misc.rs:866-936

**PROF_START**：
1. `SPROFILING.compare_exchange(false, true)` 失败 → `EBUSY`（对齐 C do_sprofile.c:50-53）
2. `isokendpt(endpt)` 失败 → rollback + `EINVAL`（对齐 C do_sprofile.c:56-57）
3. `ProfIntrType::try_from(intr_type)` 失败 → rollback + `EINVAL`（对齐 C do_sprofile.c:84-86）
4. ✅ `PROF_RTC` → `crate::clock::init_profile_clock(freq)`（misc.rs:1146，经 `ClockArch` 接线）；`PROF_NMI` → rollback + `ENOSYS`（NMI 子系统超范围，§6.3 排除）
5. Rollback `SPROFILING`（验证失败 / `init_profile_clock` 失败 / PROF_NMI 时回滚）

**PROF_STOP**：
1. `SPROFILING.compare_exchange(true, false)` 失败 → `EBUSY`（对齐 C do_sprofile.c:101-104）
2. ✅ `crate::clock::stop_profile_clock()`（misc.rs:1179，经 `ClockArch` 接线）
3. DEFERRED：`data_copy` 将 `sprof_info` + 采样缓冲区拷到用户空间（需采样缓冲区基础设施，misc.rs:1181-1184 返回 `ENOSYS`）

**Rollback 机制**：验证失败时 `SPROFILING.store(false)` 回滚，避免后续 `PROF_START` 被毒化。这是 Rust 相对 C 的改进——C 在验证失败后直接 return，`sprofiling` 仍是 0（因为还没到 `sprofiling = 1`），但 Rust 用 `compare_exchange` 提前设置了 true，需要显式回滚。

### 4.6 dispatch_unused 兜底

```rust
pub fn dispatch_unused() -> KcallResult {
    KcallResult::Ok(ENOSYS)
}
```

> design.md §D2 ↔ misc.rs:962-964

### 4.7 krandom 子系统接入（GET_RANDOMNESS / GET_RANDOMNESS_BIN）

`dispatch_getinfo` 的 `Randomness`/`RandomnessBin` 两条 case 已完整接入 `crate::krandom` 子系统（`os/kernel/src/krandom.rs`，327 行）。本节简述该子系统与 dispatch 的契约；详细设计与 C 行为对照见 [14-exception-interrupt.md §4.4](14-exception-interrupt.md)（IRQ 路径调用 `get_randomness` 的入口）与 [08-system-init-boot-finish.md §3](08-system-init-boot-finish.md)（`krandom::init()` 调用点）。

**Minix3 C 源码映射**：

| C 符号 | C 位置 | Rust 对应 | 说明 |
|--------|--------|----------|------|
| `struct k_randomness_bin` | include/minix/type.h:189-193 | `KRandomnessBin`（`#[repr(C)]`，136 字节） | `r_next`/`r_size`/`r_buf[64]`，字段顺序与大小严格对齐 |
| `struct k_randomness` | include/minix/type.h:187-194 | `KRandomness`（`#[repr(C)]`，2184 字节） | `random_elements`/`random_sources`/`bin[16]` |
| `krandom` 全局 | kernel/glo.h | `KRANDOM: SyncUnsafeCell<KRandomness>` | BKL 保护，与 `PROC_TABLE`/`PRIV_TABLE`/`IRQ_MANAGER` 同模式 |
| `krandom_init()` | main.c:48-49（`krandom.random_sources`/`random_elements` 直接赋值，**无此函数**） | `krandom::init()`（`lib.rs:387` 调用） | 设置 `KRANDOM_INIT` 标志，`const fn new()` 已初始化字段 |
| `get_randomness(&krandom, irq)` | do_irqctl.c:154 | `krandom::get_randomness(source)` | **no-op stub**，匹配 C i386/earm 实现 |
| `GET_RANDOMNESS` | do_getinfo.c:148-160 | `dispatch_getinfo::Randomness`（misc.rs:1008-1024） | 快照 + `wipe_all` + 拷贝 |
| `GET_RANDOMNESS_BIN` | do_getinfo.c:161-178 | `dispatch_getinfo::RandomnessBin`（misc.rs:1026-1054） | 索引检查 + `r_size<RANDOM_ELEMENTS→ENOENT` + `wipe_bin` |

**设计决策**（krandom.rs 文件头 D1-D4）：

- **D1**: `#[repr(C)]` 结构体严格对齐 C ABI（字段顺序/大小/对齐），因为用户态 `random` 驱动通过原始字节解释这些结构。
- **D2**: `KRANDOM` 全局用 `SyncUnsafeCell` + `addr_of_mut!`，与 `PROC_TABLE`/`PRIV_TABLE`/`IRQ_MANAGER` 同模式。BKL 保护单写（IRQ 路径）单读（syscall 路径）。
- **D3**: `get_randomness` 是 no-op stub，匹配 C 的 i386/earm 实现。**实际熵采集由用户态 `random` 驱动完成**（drivers/system/random/），内核仅提供 bin 容器与 `GET_RANDOMNESS` 导出接口。Rust 不在内核侧实现 RDRAND/RTSC 采集，避免架构特定代码泄漏到 kernel crate（与项目"硬件抽象为 trait"原则一致；x86 RDRAND 应在 `os/arch/src/x86_64/` 实现，当前 deferred）。
- **D4**: `RANDOM_SOURCES = 16`、`RANDOM_ELEMENTS = 64`，匹配 `include/minix/type.h:182-183`。

**dispatch_getinfo 接入点**：

```rust
// misc.rs:1008-1024 — GET_RANDOMNESS
GetInfoRequest::Randomness => {
    // C: do_getinfo.c:148-160 — copy entire krandom struct, then wipe all bins.
    // SAFETY: BKL is held by kernel_call_dispatch (syscall.rs:245).
    let krandom_snapshot = match unsafe { crate::krandom::try_krandom() } {
        Some(kr) => { let snapshot = *kr; kr.wipe_all(); snapshot }
        None => return KcallResult::Ok(EINVAL),
    };
    return copy_struct_to_caller(caller, &krandom_snapshot, val_ptr, val_len);
}

// misc.rs:1026-1054 — GET_RANDOMNESS_BIN
GetInfoRequest::RandomnessBin => {
    let bin = val_len2_e;
    if bin < 0 || bin >= crate::krandom::RANDOM_SOURCES as i32 {
        return KcallResult::Ok(EINVAL);
    }
    let bin_snapshot = match unsafe { crate::krandom::try_krandom() } {
        Some(kr) => {
            let bin_idx = bin as usize;
            if kr.bin[bin_idx].r_size < crate::krandom::RANDOM_ELEMENTS as i32 {
                return KcallResult::Ok(ENOENT);  // bin not yet full
            }
            let snapshot = kr.bin[bin_idx];
            kr.wipe_bin(bin_idx);
            snapshot
        }
        None => return KcallResult::Ok(EINVAL),
    };
    return copy_struct_to_caller(caller, &bin_snapshot, val_ptr, val_len);
}
```

**语义对齐验证**：
- C `do_getinfo.c:153-156` 在拷贝后 `wipe` 原数据：Rust `wipe_all()`/`wipe_bin()` 在快照后立即调用，语义一致。
- C `do_getinfo.c:171-174` 检查 `r_size < RANDOM_ELEMENTS` 返回 `ENOENT`（bin 未满）：Rust 同样检查并返回 `ENOENT`。
- C 用 `static struct k_randomness copy` 保留计数器：Rust 用栈上 `snapshot` 变量（BKL 保护下安全）。

**测试覆盖**（krandom.rs `mod tests`，6 个）：

| 测试 | 覆盖点 |
|------|--------|
| `test_krandomness_bin_layout` | `size_of = 136` 严格匹配 C `struct k_randomness_bin` |
| `test_krandomness_layout` | `size_of = 2184` 严格匹配 C `struct k_randomness` |
| `test_krandomness_new` | `random_elements = 64`、`random_sources = 16`、所有 bin 零初始化 |
| `test_wipe_bin` | 单 bin `wipe` 后 `r_next = r_size = 0` |
| `test_wipe_all` | 所有 bin `wipe_all` 后清零 |
| `test_get_randomness_is_noop` | `get_randomness(3)` / `get_randomness(15)` 不 panic、不修改状态 |

---

## Ch5: 测试要点

### 5.1 测试覆盖矩阵

| 测试类别 | 测试数 | 覆盖的 dispatch | 覆盖的 C 行为 |
|---------|--------|----------------|--------------|
| enum TryFrom | 2 | GetInfoRequest(19 变体)/TraceRequest(13 变体) | 边界值 + 无效值 |
| dispatch_trace flag 操作 | 6 | Stop/Resume/Step/Syscall/Detach/Exit(invalid) | RTS_P_STOP/MF_STEP/MF_SC_TRACE/MF_SC_ACTIVE 副作用 + reply data 写回 |
| dispatch_trace 内存读写 | 7 | GetIns(aligned+unaligned)/GetData/SetIns/SetData/GetUser/SetUser | aligned→ENOSYS, unaligned→EFAULT |
| dispatch_trace 验证 | 3 | invalid request/endpoint/kernel target | EINVAL/EINVAL/EPERM |
| dispatch_update 验证+swap | 4 | none src/self swap/busy/quiescent | EINVAL/EINVAL/EBUSY/OK(0) (swap identity preserved) |
| proc_is_updatable | 3 | user_mode/receiving_only/kernel_blocked | true/true/false |
| dispatch_getinfo | 5 | Proc invalid/self/valid, Priv invalid, Regs invalid | EINVAL/ENOSYS/ENOSYS/EINVAL/EINVAL |
| dispatch_profile 状态机 | 9 | unknown action/invalid endpoint/unknown intr/valid/double start/stop without start/stop after start/rollback/stop when not running | EINVAL/EINVAL/EINVAL/ENOSYS/EBUSY/EBUSY/ENOSYS/EINVAL/EBUSY |
| dispatch_unused | 1 | ENOSYS | ENOSYS |
| **总计** | **40** | — | — |

> 实际测试位置：`os/kernel/src/misc.rs:2014-2825`（`#[cfg(test)] mod tests`）。验证命令：`cargo test -p minix-kernel --lib misc`。
> **测试范围说明**：上表 40 个测试属 `misc::tests` 模块；`cargo test --lib misc` 还会匹配 5 个跨模块测试（`proc::tests::test_fork_from_misc_flags_corrections` + 4 个 `proc_table::tests::test_process_misc_flags_*`），实际执行 45 个。

### 5.2 关键测试说明

**`test_dispatch_trace_step_clears_proc_stop_and_sets_step_flag`**：
验证 `T_STEP` 的两个副作用——设置 `MF_STEP` 和清除 `RTS_P_STOP`。这是 L1 对偶测试，验证 Rust 行为与 C do_trace.c:179-183 一致。

**`test_dispatch_trace_getins_unaligned_returns_enosys`**：
验证 `T_GETINS` 无对齐检查——`tr_addr = 0x1001`（1 字节偏离 8 字节边界）→ `ENOSYS`（非 `EFAULT`）。C 的 `COPYFROMPROC` 调用 `virtual_copy_vmcheck`（字节级拷贝，无对齐要求），Rust 不添加 C 没有的检查。

**`test_sprof_double_start_returns_ebusy`**：
验证 `SPROFILING` 状态机——预先设置 `sprofiling=true`，再次 `PROF_START` → `EBUSY`。对齐 C do_sprofile.c:50-53。

**`test_sprof_start_rollback_on_invalid_endpoint`**：
验证 rollback 机制——`PROF_START` 验证失败后 `SPROFILING` 必须回滚为 false。这是 Rust 特有的测试（C 无此逻辑，因为 C 不提前设置 `sprofiling`）。

**`test_dispatch_update_rejects_self_swap`**：
验证 `src == dst` → `EINVAL`。这是 Rust 的改进——C 用 `assert` 会在 debug 构建崩溃，release 构建静默错误。Rust 显式返回 `EINVAL`。

### 5.3 测试隔离机制

`SPROFILING` 是全局 static，并行测试会竞争。用 `SPROF_TEST_LOCK: AtomicBool` 自旋锁序列化 sprof 相关测试：
- `sprof_test_setup()`：获取锁 + 重置 `SPROFILING`
- `sprof_test_teardown()`：重置 `SPROFILING` + 释放锁

这是 `no_std` 环境下的测试隔离模式——不依赖 `std::sync::Mutex`。

---

## Ch6: 参见

- [22-privilege.md](22-privilege.md) — SYS_PRIVCTL（权限控制，`is_sys_proc` 检查来源）
- [17-cross-space-copy.md](17-cross-space-copy.md) — `data_copy_vmcheck`（GETINFO/TRACE 数据拷贝依赖）
- [24-cross-space-runtime.md](24-cross-space-runtime.md) — 跨地址空间运行时（VMREQUEST 机制，TRACE 跨地址空间拷贝依赖）
- [20-syscall-device.md](20-syscall-device.md) — x86-only 调用处理（D4 一致策略）
- [16-fork-exit.md](16-fork-exit.md) — `RtsFlags`/`MiscFlags` 原子标志位（TRACE flag 操作依赖）

---

## 附录: DEFERRED 清单

| 缺口 | C 位置 | 阻塞原因 | 解除条件 |
|------|--------|---------|---------|
| ~~GET_HZ/LOADINFO/MACHINE/CPUINFO/CPUTICKS~~ | do_getinfo.c 各 case | ✅ 已实现: `copy_struct_to_caller<T>` + `data_copy_vmcheck`（`LoadInfoStruct`/`MachineStruct`/`CpuInfoEntry` 均为 `#[repr(C)]`） | — |
| GETINFO GET_PROC/GET_PROCTAB | do_getinfo.c:GET_PROC/PROCTAB | 需 C 兼容 `struct proc` 布局（KProcess→proc 转换，~100+ 字段） | `#[repr(C)]` struct proc 设计 |
| GETINFO GET_PRIV/GET_PRIVTAB | do_getinfo.c:GET_PRIV/PRIVTAB | 需 C 兼容 `struct priv` 布局 | `#[repr(C)]` struct priv 设计 |
| GETINFO GET_REGS | do_getinfo.c:GET_REGS | 需 C 兼容 `reg_t`/CpuContext 布局 | `#[repr(C)]` reg_t 设计 |
| ~~GETINFO GET_RANDOMNESS~~ | do_getinfo.c:148-160 | ✅ 已实现（`misc.rs:1008-1024`，快照 + `wipe_all` + `copy_struct_to_caller`） | — |
| ~~GETINFO GET_RANDOMNESS_BIN~~ | do_getinfo.c:161-178 | ✅ 已实现（`misc.rs:1026-1054`，索引检查 + `r_size<RANDOM_ELEMENTS→ENOENT` + `wipe_bin`） | — |
| GETINFO 其余 | GET_IMAGE/MONPARAMS/IRQHOOKS/IRQACTIDS/IDLETSC | 各自特定基础设施 | 子系统落地 |
| ~~TRACE 跨地址空间拷贝~~ | do_trace.c COPYFROMPROC/COPYTOPROC | ✅ 已实现: `data_copy_vmcheck` | — |
| ~~TRACE T_SETUSER 段寄存器保护~~ | do_trace.c:141-166 | ✅ 已实现: `CpuContextArch::write_user_register` trait 方法 + 三架构实现（x86_64 段寄存器保护 + PSW 用户位掩码；arm64/riscv64 偏移映射） | — |
| ~~TRACE T_GETUSER priv-struct 分支~~ | do_trace.c:117-123 | ✅ 已实现: `dispatch_trace` 签名新增 `priv_table: &PrivTable`；经 `rp.priv_id` → `priv_table.get(pid)` → `PrivInfoStruct::from_kpriv` 构造快照 + `read_word_at_offset` 读取；对齐 C 的 `sizeof(struct proc)` 向上对齐 + 偏移减法逻辑；2 个测试覆盖（正常读取 + 越界 EFAULT） | — |
| ~~UPDATE 槽位交换~~ | do_update.c:129-147 | ✅ 已实现: `ProcessTable::swap_slots` + `PrivTable::swap_slots` (`core::mem::swap` + `split_at_mut`) + `adjust_proc_slot`/`adjust_priv_slot` 恢复 identity 字段 | — |
| ~~UPDATE inherit_priv_*~~ | do_update.c:94-105 | ✅ 已实现: `KPriv::add_irq/add_io/add_mem` (dedup + CHECK_* flag) | — |
| ~~UPDATE abort_proc_ipc_send~~ | do_update.c:220-236 | ✅ 已实现: `SenderQueue::remove_by_nr` + `RTS_SENDING` clear + `MF_SENDING_FROM_KERNEL` clear | — |
| UPDATE swap_memreq | do_update.c:313-337 | VM request 链表 | VmRequestQueue |
| ~~SPROF 时钟初始化（PROF_RTC）~~ | do_sprofile.c:75-82 | ✅ 已实现: `ClockArch::init_profile_clock(freq)` / `stop_profile_clock()` | — |
| SPROF PROF_NMI | do_sprofile.c | NMI 子系统超范围（§6.3 排除），返回 `ENOSYS` | N/A（设计排除） |
| SPROF 数据拷贝 | do_sprofile.c:117-120 | data_copy 结果到用户 | Direct Map |
| SPROF clean_seen_flag | do_sprofile.c:25-31 | MF_SPROF_SEEN flag | MiscFlags 扩展 |

> **已解除的 DEFERRED**（本轮修复）：
> - `GetInfoRequest` enum 已扩展到全部 19 个变体，与 C `do_getinfo` 一一对应（此前缺失 9 个变体）
> - `TraceRequest` enum 已扩展到全部 13 个变体，与 C `do_trace` 一一对应（此前缺失 5 个变体：T_STOP/T_DETACH/T_SYSCALL/T_READB_INS/T_WRITEB_INS）
> - `dispatch_trace` 已实现 5 个 flag 操作变体（Stop/Resume/Step/Syscall/Detach），此前仅实现 Step/Cont/Kill
> - `dispatch_trace` 签名从 `&Message` 改为 `&mut Message`，支持 reply data 字段写回
> - `dispatch_trace` 6 个跨地址空间拷贝变体（T_GETINS/T_GETDATA/T_SETINS/T_SETDATA/T_READB_INS/T_WRITEB_INS）已接入 `data_copy_vmcheck`，支持 VMSUSPEND 恢复路径（此前返回 ENOSYS）
> - `dispatch_getinfo` GET_KINFO 已返回关键字段到 reply message（m_m4 格式）；GET_WHOAMI 已写入 reply message
> - `dispatch_getinfo` 新增 `clock_state: &ClockState` 参数（与 `dispatch_setalarm` 对齐）
> - GET_HZ/GET_LOADINFO/GET_MACHINE/GET_CPUINFO/GET_CPUTICKS 已实现：经 `copy_struct_to_caller<T>` + `data_copy_vmcheck` 拷贝到用户空间；`LoadInfoStruct`/`MachineStruct`/`CpuInfoEntry` 均为 `#[repr(C)]`（此前返回 ENOSYS）
> - SPROF START（PROF_RTC）→ `ClockArch::init_profile_clock(freq)` 已接线；SPROF STOP → `ClockArch::stop_profile_clock()` 已接线；PROF_NMI → `ENOSYS`（NMI 子系统 §6.3 排除）
> - GETINFO DEFERRED 注释已更新：`data_copy_vmcheck` 已就绪，实际阻塞于 C 兼容结构体布局（struct proc/priv/reg_t 等）
> - **T_SETUSER 已实现**: 新增 `CpuContextArch::write_user_register` trait 方法（`os/arch/src/arch/boot.rs:214`），三架构均覆盖：
>   - x86_64 (`os/arch/src/x86_64/boot.rs`): 段寄存器（cs/ds/es/fs/gs/ss）禁止写入返回 `Err(())`；PSW (RFLAGS) 应用 `PSW_USER_MASK=0x0DD5` 用户位掩码（CF/PF/AF/ZF/SF/TF/DF/OF/IF）；其余通用寄存器按偏移直接写入
>   - arm64 (`os/arch/src/arm64/boot.rs`): psr/pc/sp/r0 直接写入；gp_regs[0..30] 按 `(offset-32)/8` 索引写入
>   - riscv64 (`os/arch/src/riscv64/boot.rs`): sstatus/sepc/sp/a0 直接写入；gp_regs[0..30] 同 arm64 偏移映射
> - `dispatch_trace` 签名从 `&ProcessTable` 改为 `&mut ProcessTable`，支持 `proc_table.get_mut(target_nr)` 获取 `&mut KProcess` 用于寄存器写入；`syscall.rs::dispatch_trace` wrapper 同步更新
> - **T_GETUSER proc-struct 分支已实现**: 通过 `ProcInfoStruct::from_kprocess` 构造快照后 `read_word_at_offset` 读取 `u64`（对齐 C do_trace.c:108-111）
> - **T_GETUSER priv-struct 分支已实现**: `dispatch_trace` 签名新增 `priv_table: &PrivTable` 参数；经 `rp.priv_id` → `priv_table.get(pid)` → `PrivInfoStruct::from_kpriv` 构造快照 + `read_word_at_offset` 读取（对齐 C do_trace.c:117-123 的 `sizeof(struct proc)` 向上对齐 + 偏移减法逻辑）；2 个测试覆盖（正常读取 s_proc_nr + 越界 EFAULT）
> - **测试隔离修复**: `BootAlloc` / `IpcFilterPool` 全局状态封装为结构体，支持 per-test 实例（避免 `cargo test` 多线程并行时 `BOOT_PT_NEXT` / `ipc_filter_pool()` 全局状态互染）
