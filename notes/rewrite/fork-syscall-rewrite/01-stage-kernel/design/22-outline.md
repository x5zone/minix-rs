# 22-privilege-outline — 文档结构契约

> **文档**: `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/22-privilege.md`
> **C 源码**: `minix3/minix/kernel/priv.h` (105 行), `minix3/minix/include/minix/priv.h` (105 行), `minix3/minix/kernel/system.c:272-540` (do_privctl + get_priv)
> **Rust 实现**: `os/kernel/src/kpriv.rs` (860 行), `os/kernel/src/capability.rs` (356 行)
> **创建**: 2026-08-01
> **依据**: C 源码 → OS 理论 → Rust 代码对照（非反向）
> **衔接**: 11-scheduling-primitives（PREEMPTIBLE/BILLABLE）, 17-syscall-process（fork 分配 priv）, 18-syscall-copy（grant 表）, 20-syscall-device（CHECK_IO_PORT/CHECK_IRQ）

---

## 一、章节骨架与主语

### Ch1 主语：权限/特权（"内核如何控制进程能做什么？"）

核心问题：**内核如何控制进程的权限——谁能发 IPC 给谁、谁能调用哪些内核调用、谁能访问哪些 I/O 端口/IRQ/内存？**

Minix3 的回答：**每个系统进程拥有独立的 `struct priv` 结构，定义完整的权限集；所有用户进程共享单一的 `USER_PRIV` 结构，权限最小化**。权限由三类掩码控制：IPC 目标位图（`s_ipc_to`）、内核调用掩码（`s_k_call_mask`）、trap 掩码（`s_trap_mask`）；加上 I/O 端口/IRQ/内存范围表，构成进程的能力边界。

| 节 | 标题 | 灵魂本质（一句话） | 概念组 |
|----|------|-------------------|--------|
| §1.1 | struct priv 与权限模型 | "系统进程独立 priv、用户进程共享 USER_PRIV——空间效率与权限隔离的平衡" | A |
| §1.2 | s_flags 权限标志 | "11 个标志位控制抢占/计费/系统进程身份/I/O 检查/VM/LU/RST——IPC 目标过滤无标志位，无条件执行" | A, B |
| §1.3 | 三类权限掩码 | "s_ipc_to（能发给谁）+ s_k_call_mask（能调哪些 kernel call）+ s_trap_mask（能用哪些 trap 原语）——位图精确控制" | B |
| §1.4 | priv 表结构与 ID 分配 | "静态区（boot 进程）+ 动态区（运行时分配），static_priv_id(n)=NR_TASKS+n，USER_PRIV_ID 共享" | C |
| §1.5 | I/O 端口/IRQ/内存范围 | "CHECK_IO_PORT/CHECK_IRQ/CHECK_MEM 标志启用可选检查，系统进程逐条添加权限范围" | D |

### Ch2 主语：C 源码符号（file:line 锚定）

每节以 C 结构/函数为单元，附 file:line，说明语义。

### Ch3 主语：设计决策（hypothesis-driven）

采用"如果 X 设计会有 Y 问题所以用 Z"格式，禁止迭代叙事。

### Ch4 主语：Rust 实现（真实代码，非 stub）

贴 kpriv.rs / capability.rs 真实代码片段，标注 file:line。

### Ch5 主语：测试函数（可 grep 验证）

列出实际 `fn test_*` 函数名，每个测试对应一个被测行为。

---

## 二、详细大纲

### Ch1. 概念建构（concept-driven）

#### §1.1 struct priv 与权限模型

**灵魂本质**: 系统进程独立 priv、用户进程共享 USER_PRIV——空间效率与权限隔离的平衡。

**WHY → WHAT → HOW 弧线**:
- **WHY**: 内核需要为每个进程定义"能做什么"的边界——IPC 目标、内核调用、I/O 访问、中断接收。若每个进程都存完整权限集，空间浪费（用户进程权限极小但数量多）；若全部共享，系统进程无法隔离。
- **WHAT**: Minix3 用"分治"策略——系统进程（VM/PM/VFS/RS 等，≤64 个）各有独立 `struct priv`；所有用户进程共享单一 `USER_PRIV` 结构。`p_priv` 指针指向所属 priv 结构。
- **HOW**: C `priv[NR_SYS_PROCS]` 数组（priv.h:94）+ `ppriv_addr[]` 指针表（priv.h:95）实现 O(1) 按 ID 索引。`get_priv(rp, id)` (system.c:272) 分配静态或动态 slot。

**权限模型三要素**:
| 要素 | C 字段 | 语义 | 检查时机 |
|------|--------|------|---------|
| IPC 目标 | `s_ipc_to` | 允许发送的 endpoint 位图 | 每次 IPC send（无条件） |
| 内核调用 | `s_k_call_mask` | 允许的 kernel call 号位图 | 每次 kernel call dispatch |
| Trap 原语 | `s_trap_mask` | 允许的 IPC 原语（SEND/RECEIVE/SENDREC 等） | trap 入口 |

**redox 对照**: redox 用 scheme 命名空间 + capability token 模型，非位图。Minix3 的位图模型适合固定数量系统进程（≤64）；redox 的 scheme 模型支持动态命名但需要路径解析开销。两种模型在不同场景各有优劣——Minix3 选择位图是为了 O(1) 检查和固定 ID 稳定性。

#### §1.2 s_flags 权限标志

**灵魂本质**: 11 个标志位控制抢占/计费/系统进程身份/I/O 检查/VM/LU/RST——IPC 目标过滤无标志位，无条件执行。

**WHY → WHAT → HOW**:
- **WHY**: 进程有不同类别（idle/内核任务/系统服务/用户进程），需要标志位区分行为。I/O/IRQ/内存检查是可选的（仅特定系统进程需要），用标志位启用。
- **WHAT**: `s_flags` (short) 含 11 个位：PREEMPTIBLE/BILLABLE/DYN_PRIV_ID/SYS_PROC/CHECK_IO_PORT/CHECK_IRQ/CHECK_MEM/ROOT_SYS_PROC/VM_SYS_PROC/LU_SYS_PROC/RST_SYS_PROC。
- **HOW**: C `const.h:143-154` 定义位值；`priv.h:36-49` 定义预定义组合（IDL_F/TSK_F/SRV_F/DSRV_F/RSYS_F/VM_F/USR_F/IMM_F）。

**关键不对称**（不可混淆）:
| 检查类型 | 有标志位启用? | C 宏 | 说明 |
|---------|-------------|------|------|
| I/O 端口 | ✅ CHECK_IO_PORT | `may_send_to` 无关 | 可选检查 |
| IRQ | ✅ CHECK_IRQ | `may_send_to` 无关 | 可选检查 |
| 内存映射 | ✅ CHECK_MEM | `may_send_to` 无关 | 可选检查 |
| IPC 目标 | ❌ 无标志位 | `may_send_to(rp,nr)` (priv.h:86) | **无条件**执行 |

> **注意**: C 源码中不存在 `CHECK_IPC` 标志。`may_send_to()` 宏始终检查 `s_ipc_to`，无需标志位启用。这与 CHECK_IO_PORT/CHECK_IRQ/CHECK_MEM 的可选模式不同。

**预定义组合**（priv.h:36-50）:
| 组合 | 值 | 适用 | 关键特性 |
|------|-----|------|---------|
| IDL_F | SYS_PROC \| BILLABLE | IDLE | 不可抢占 |
| TSK_F | SYS_PROC | 内核任务 | 无 IPC/kcall |
| SRV_F | SYS_PROC \| PREEMPTIBLE | 系统服务 | 可抢占 |
| DSRV_F | SRV_F \| DYN_PRIV_ID | 动态服务 | 动态 priv ID |
| RSYS_F | SRV_F \| ROOT_SYS_PROC | RS | 根系统进程 |
| VM_F | SYS_PROC \| VM_SYS_PROC | VM | VM 特权 |
| USR_F | BILLABLE \| PREEMPTIBLE | 用户进程 | 非 SYS_PROC |
| IMM_F | ROOT_SYS_PROC \| VM_SYS_PROC \| PREEMPTIBLE | 不可变 | 高特权 |

#### §1.3 三类权限掩码

**灵魂本质**: s_ipc_to + s_k_call_mask + s_trap_mask——位图精确控制 IPC 目标、内核调用、trap 原语。

**s_ipc_to（IPC 目标位图）**:
- 类型：`sys_map_t`（64-bit），每位对应一个 sys_id
- 检查：`may_send_to(rp, nr)` = `get_sys_bit(priv(rp)->s_ipc_to, nr_to_id(nr))` (priv.h:86)
- 特殊值：`NO_M`(-1) 无目标 / `ALL_M`(-2) 全部目标（priv.h:24-25）
- 异步扩展：`may_asynsend_to(rp, nr)` = `may_send_to(rp, nr) || rp->p_nr == nr` (priv.h:87)

**s_k_call_mask（内核调用掩码）**:
- 类型：`bitchunk_t[SYS_CALL_MASK_SIZE]`（priv.h:38），SYS_CALL_MASK_SIZE=2
- 每位对应一个 kernel call 号（共 ~58 个，需 2 个 u32）
- 特殊值：`NO_C`(-1) 无调用 / `ALL_C`(-2) 全部调用

**s_trap_mask（trap 掩码）**:
- 类型：`short`，每位对应一个 IPC 原语（SEND/RECEIVE/SENDREC/NOTIFY 等）
- 预定义：`CSK_T`(1<<RECEIVE) / `TSK_T`(0) / `SRV_T`(~0) / `USR_T`(1<<SENDREC)（priv.h:59-63）

#### §1.4 priv 表结构与 ID 分配

**灵魂本质**: 静态区（boot 进程）+ 动态区（运行时分配），static_priv_id(n)=NR_TASKS+n，USER_PRIV_ID 共享。

**表布局**:
```
priv[NR_SYS_PROCS=64]:
  索引 0..NR_STATIC_PRIV_IDS-1    静态区（boot image 进程）
  索引 NR_STATIC_PRIV_IDS..63     动态区（运行时分配的系统进程）
```

**ID 映射**:
| 函数 | 公式 | 用途 |
|------|------|------|
| `static_priv_id(n)` | `NR_TASKS + n` (priv.h:12) | boot 进程 → priv_id |
| `is_static_priv_id(id)` | `id >= 0 && id < NR_STATIC_PRIV_IDS` (priv.h:11) | 判断是否静态 |
| `USER_PRIV_ID` | `static_priv_id(ROOT_USR_PROC_NR)` (priv.h:18) | 用户进程共享 ID |
| `NULL_PRIV_ID` | `-1` (priv.h:21) | 空 ID |

**fork 中的特权降级**（do_fork.c:104-107）:
- 用户进程 fork → 子进程继承 USER_PRIV_ID
- 系统进程 fork → 子进程降级为 USER_PRIV_ID + RTS_NO_PRIV，需 RS 重新授权

#### §1.5 I/O 端口/IRQ/内存范围

**灵魂本质**: CHECK_IO_PORT/CHECK_IRQ/CHECK_MEM 标志启用可选检查，系统进程逐条添加权限范围。

**三类范围表**:
| 类型 | 字段 | 容量 | 标志 | 添加函数 |
|------|------|------|------|---------|
| I/O 端口 | `s_io_tab[NR_IO_RANGE=64]` | 64 | CHECK_IO_PORT | `priv_add_io` |
| IRQ | `s_irq_tab[NR_IRQ=16]` | 16 | CHECK_IRQ | `priv_add_irq` |
| 内存 | `s_mem_tab[NR_MEM_RANGE=20]` | 20 | CHECK_MEM | `priv_add_mem` |

每个范围表项含 base + limit，检查时线性扫描（数量小，O(N) 可接受）。

---

### Ch2. C 源码分析（file:line 锚定）

#### §2.1 minix/include/minix/priv.h (105 行)

| 符号 | 位置 | 说明 |
|------|------|------|
| `NR_STATIC_PRIV_IDS` | priv.h:10 | = NR_BOOT_PROCS |
| `is_static_priv_id(id)` | priv.h:11 | 静态 ID 判定 |
| `static_priv_id(n)` | priv.h:12 | NR_TASKS + n |
| `USER_PRIV_ID` | priv.h:18 | 用户进程共享 ID |
| `NULL_PRIV_ID` | priv.h:21 | -1 |
| `NO_M`/`ALL_M` | priv.h:24-25 | IPC 目标特殊值 |
| `NO_C`/`ALL_C`/`NULL_C` | priv.h:28-30 | kcall 特殊值 |
| IDL_F..IMM_F | priv.h:36-50 | 预定义标志组合 |
| CSK_T..USR_T | priv.h:59-63 | trap 掩码组合 |
| TSK_M..USR_M | priv.h:66-69 | IPC 目标组合 |
| TSK_KC..USR_KC | priv.h:72-75 | kcall 掩码组合 |

#### §2.2 minix/kernel/priv.h (105 行)

| 符号 | 位置 | 说明 |
|------|------|------|
| `struct priv` | priv.h:21-66 | 完整 priv 结构（~25 字段） |
| `s_proc_nr` | priv.h:22 | 关联进程号 |
| `s_id` | priv.h:23 | priv 结构索引 |
| `s_flags` | priv.h:24 | 权限标志（short） |
| `s_asyntab` | priv.h:28 | 异步发送表地址 |
| `s_trap_mask` | priv.h:34 | trap 掩码 |
| `s_ipc_to` | priv.h:35 | IPC 目标位图 |
| `s_k_call_mask` | priv.h:38 | kernel call 掩码数组 |
| `s_sig_mgr`/`s_bak_sig_mgr` | priv.h:40-41 | 信号管理器 |
| `s_notify_pending` | priv.h:42 | 挂起通知位图 |
| `s_asyn_pending` | priv.h:43 | 挂起异步消息位图 |
| `s_int_pending` | priv.h:44 | 挂起中断 |
| `s_sig_pending` | priv.h:45 | 挂起信号 |
| `s_ipcf` | priv.h:46 | IPC filter 指针 |
| `s_alarm_timer` | priv.h:48 | 同步闹钟定时器 |
| `s_stack_guard` | priv.h:49 | 栈保护字 |
| `s_diag_sig` | priv.h:51 | 诊断信号 |
| `s_io_tab`/`s_irq_tab`/`s_mem_tab` | priv.h:54,60,57 | I/O/IRQ/内存范围表 |
| `s_grant_table`/`s_grant_entries`/`s_grant_endpoint` | priv.h:61-63 | grant 表 |
| `s_state_table`/`s_state_entries` | priv.h:64-65 | state 表 |
| `BEG_PRIV_ADDR` 等 | priv.h:72-77 | 表地址宏 |
| `priv_addr(i)` | priv.h:79 | 按 ID 取 priv 指针 |
| `priv_id(rp)`/`priv(rp)` | priv.h:80-81 | 取进程的 priv |
| `may_send_to(rp,nr)` | priv.h:86 | IPC 目标检查宏 |
| `may_asynsend_to(rp,nr)` | priv.h:87 | 异步发送检查 |
| `priv[NR_SYS_PROCS]` | priv.h:94 | 全局 priv 表 |
| `ppriv_addr[NR_SYS_PROCS]` | priv.h:95 | 直接指针表 |

#### §2.3 system.c:272-540 (do_privctl + get_priv + 辅助)

| 符号 | 位置 | 说明 |
|------|------|------|
| `get_priv(rp, id)` | system.c:272-311 | 分配 priv（静态/动态） |
| `get_priv` 静态分支 | system.c:~280 | `priv_id = static_priv_id(proc_nr)` |
| `get_priv` 动态分支 | system.c:~290 | `dyn_priv_id()` 扫描空闲 slot |
| `get_priv` 占用检查 | system.c:~285 | `s_proc_nr != NONE → EBUSY` |
| `do_privctl` | system.c:~300-540 | SYS_PRIVCTL 主函数 |
| `do_privctl` SET MASK | system.c:~320 | 设置 s_ipc_to/s_k_call_mask |
| `do_privctl` ADD IO/IRQ/MEM | system.c:~380 | 添加 I/O/IRQ/内存范围 |
| `do_privctl` GRANT TABLE | system.c:~420 | 设置 grant 表地址 |
| `do_privctl` SIG MGR | system.c:~460 | 设置信号管理器 |
| `do_privctl` FLAGS | system.c:~500 | 设置 s_flags |
| `priv_add_io`/`priv_add_irq`/`priv_add_mem` | system.c:~520-540 | 范围添加辅助 |

> **注**: system.c 行号为近似值，review 时需精确验证。

---

### Ch3. 设计决策（hypothesis-driven）

#### D1. KPriv 结构：扁平 vs 6 子结构

**假设性推理**:
- 如果用 C 的扁平 `struct priv`（~25 字段平铺）：字段过多，职责混杂（身份/信号/IPC/I/O/内存/运行时混在一起）；难以按职责分别 Default/const fn 初始化；读者无法快速定位"这个字段属于哪个职责"。
- 如果用单一 trait 抽象：priv 是数据结构而非行为，trait 不合适；且 priv 的字段是内核直接访问的内存布局，trait 会引入不必要的间接。
- 所以用 **6 子结构**（PrivCapability/PrivSignals/PrivIpc/PrivIo/PrivMem/PrivRuntime）：按职责分组，每个子结构独立 `const fn new()` 构造，PrivTable 可用 `[KPriv; NR_SYS_PROCS]` const 初始化（无堆分配）。字段名保留 C `s_` 前缀以便追溯。

**实现**: `KPriv { capability, signals, ipc, io, mem, runtime }` (kpriv.rs:307-314)。

#### D2. s_flags：裸 short vs bitflags

**假设性推理**:
- 如果用裸 `i16`（C 方式）：无类型安全，`s_flags = 0x010 | 0x020` 与 `s_flags = 0x030` 无法区分意图；无法在函数签名区分"任意 i16"与"权限标志"。
- 如果用 `enum`：位组合（如 SYS_PROC|PREEMPTIBLE）非枚举成员，需手动 match 所有组合，不可扩展。
- 所以用 **`PrivFlagsBits` bitflags (u16)**：位组合是 first-class 值，`contains(SYS_PROC)` 类型安全；`from_bits_truncate` 处理未知位；预定义组合用 `priv_flag_set` 模块的 const 项。

**实现**: `bitflags! { pub struct PrivFlagsBits: u16 { ... } }` (kpriv.rs:54-72)，值严格对齐 const.h:143-154。

#### D3. 权限授予：分散设置 vs CapabilityTemplate 模板

**假设性推理**:
- 如果用 C 的分散设置（`get_priv` + 逐字段 `s_flags=...; s_ipc_to=...; s_k_call_mask=...`）：调用者需记住每个进程类别该设什么掩码，易漏（如忘记设 IPC 掩码 → 进程无法通信）；7 个参数的 `configure_boot_priv(priv_id, flags, init_flags, trap_mask, ipc_to, k_call_mask, sig_mgr)` 重复且易错。
- 如果用 builder pattern：链式调用 `.flags(...).ipc_to(...).build()` 仍需调用者知道每类进程该填什么；builder 适合可变配置，但 boot 阶段是固定 5 类进程。
- 所以用 **`CapabilityTemplate` enum + `grant_capability(proc_nr, template)`**：5 个模板（Idle/KernelTask/Vm/RootService/Deferred）封装正确的 flag+mask 组合，correct-by-construction——调用者选模板即可，无法漏设掩码。

**实现**: `enum CapabilityTemplate { Idle, KernelTask, Vm, RootService, Deferred }` (capability.rs:128-142)，`PrivTable::grant_capability()` (kpriv.rs:500-554)。

**redox 对照**: redox 的 capability 模型更细粒度（per-resource capability token），Minix3 是 per-process bitmap。CapabilityTemplate 是 Rust 侧的"配置模板"抽象，不改变 Minix3 的位图语义，只是让配置正确性由类型保证。

#### D4. ProcessCapability (u32) vs PrivFlagsBits (u16) 双系统

**现状**: capability.rs 定义 `ProcessCapability: u32`（值 0x01..0x200，与 C 的 0x002..0x800 不同），kpriv.rs 定义 `PrivFlagsBits: u16`（值对齐 C）。`grant_capability` 在两者间手动映射（kpriv.rs:514-532）。

**假设性推理**:
- 如果统一为单一系统（ProcessCapability 替代 PrivFlagsBits）：ProcessCapability 的值不对齐 C，FFI/调试时无法直接对照 C 源码；且 ProcessCapability 含 KILL/SIGS_SYS/OWN_ID 等 C s_flags 中不存在的位（Rust 侧扩展）。
- 如果统一为 PrivFlagsBits：丢失 ProcessCapability 的扩展位和语义分类。
- 所以**当前保留双系统**：PrivFlagsBits 是 C `s_flags` 的忠实映射（用于存储和 FFI 对齐），ProcessCapability 是 Rust 侧的语义扩展（含 C 没有的 KILL/SIGS_SYS 等）。`grant_capability` 中的映射是单一转换点。**长期**应评估是否合并，但当前 rewrite 阶段保持双系统以对齐 C ground truth。

**实现**: 映射在 kpriv.rs:514-532，注释说明"two different encoding spaces; this mapping is the single source of truth"。

#### D5. 掩码类型：裸整数 vs Newtype

**假设性推理**:
- 如果用裸 `u64`（s_ipc_to）/ `[u32; 2]`（s_k_call_mask）/ `u16`（s_trap_mask）：类型系统无法区分"IPC 目标位图"与"kernel call 位图"——函数签名 `fn set_mask(mask: u64)` 可传入任意 u64，IPC 掩码和 kcall 掩码可互换，编译期无法捕获。
- 如果用 bitflags：位图本身不是"标志集合"而是"允许的目标集合"，bitflags 的 `contains` 语义不直接匹配"是否允许发给 target X"。
- 所以用 **Newtype**: `IpcMask(u64)` / `KCallMask(u64)` / `TrapMask(u32)` (capability.rs:211-275)：类型系统区分三种掩码，`may_send_to(sys_id)` / `contains(other)` 方法封装位操作，构造函数 `from_bits` 是唯一入口。

**实现**: capability.rs:211-275。`grant_capability` 用 Newtype 接口，内部转回裸位存入 KPriv（兼容期）。

#### D6. PrivTable 存储：堆分配 vs 固定数组

**假设性推理**:
- 如果用 `Box<[KPriv]>`：boot 阶段无堆分配器（`#![no_std]` + allocator 未初始化），无法构造。
- 如果用 `Vec<KPriv>`：同上，且 Vec 有增长/缩容开销，priv 表大小固定 NR_SYS_PROCS=64。
- 所以用 **固定数组 `[KPriv; NR_SYS_PROCS]` + `const fn new()`**：编译期已知大小，const 初始化无堆分配，匹配 C 的 `EXTERN struct priv priv[NR_SYS_PROCS]` BSS 布局。

**实现**: `PrivTable { privs: [KPriv; NR_SYS_PROCS] }` (kpriv.rs:385-387)，`const fn new()` (kpriv.rs:395-403) 用 `[const { KPriv::new_zeroed(0) }; NR_SYS_PROCS]` + while 循环设 s_id。

#### D7. s_proc_nr：sentinel NONE vs Option<ProcNr>

**假设性推理**:
- 如果用 C 的 `proc_nr_t s_proc_nr` + `NONE`(-1) sentinel：Rust 中 `i32` 无法区分"有效 proc_nr"与"未分配"；`-1` 是合法 i32 值，类型系统无法阻止误用。
- 如果用 `i32` + 运行时检查 `if s_proc_nr == NONE`：错误延迟到运行时，且每个 callsite 需重复检查。
- 所以用 **`Option<ProcNr>`**：`None` 表示未分配，`Some(nr)` 表示已关联；类型系统强制处理"未分配"情况；`is_some()` / `is_none()` 替代 sentinel 比较。

**实现**: `PrivCapability.s_proc_nr: Option<ProcNr>` (kpriv.rs:128)。

#### D8. s_alarm_timer：裸 minix_timer_t vs Option<(TimerEntry, TimerId)>

**假设性推理**:
- 如果用 C 的 `minix_timer_t s_alarm_timer`（内联结构 + `TMR_NEVER` sentinel）：Rust 无内联链表节点，且 sentinel 需运行时检查；C 的 `tmr_inittimer` 设 exp_time=TMR_NEVER，但无法表达"timer 已设置但未入队"等中间状态。
- 如果用 `Option<TimerEntry>`：丢失 reset_timer 所需的 TimerId（C 用指针，Rust 用 id），取消闹钟时无法 O(1) 从时钟队列移除。
- 所以用 **`Option<(TimerEntry, TimerId)>`**：`None` = 未设置；`Some((entry, id))` = 已设置且 id 用于 reset_timer(id) O(log N) 移除。tuple 携带完整状态，"非法状态不可表达"。

**实现**: `PrivRuntime.s_alarm_timer: Option<(TimerEntry, TimerId)>` (kpriv.rs:279)。

#### D9. s_ipcf / s_stack_guard：裸指针 vs Option<usize>（当前限制）

**现状**: `s_ipcf: Option<usize>` (kpriv.rs:246) / `s_stack_guard: Option<usize>` (kpriv.rs:247) 用 usize 存地址。

**问题**: usize 不是类型安全的指针；`Option<*mut ipc_filter_t>` 也不 `Send`/`Sync`。

**当前判定**: 这是已知限制。rewrite 阶段优先对齐 C 语义（C 用裸指针），后续 redesign 阶段可引入 `NonNull<T>` 或专用类型。标 P2（不阻塞 rewrite 收敛）。

#### D10. PrivId / SysId：type alias vs newtype

**现状**: `pub type PrivId = u16; pub type SysId = u16;` (kpriv.rs:7-8)。

**问题**: type alias 无类型安全——`PrivId` 和 `SysId` 可互换，函数签名 `fn get(id: PrivId)` 可传入 `SysId`。

**当前判定**: 改为 newtype 需更新所有 callsite，影响面大。标 P2（后续改进）。当前 `static_priv_id` / `is_static_priv_id` 的语义已足够清晰。

---

### Ch4. 实现详解（真实代码）

#### §4.1 PrivFlagsBits bitflags (kpriv.rs:54-72)

贴真实 bitflags 定义，标注每个位对齐 const.h:143-154。

#### §4.2 priv_flag_set 预定义组合 (kpriv.rs:77-93)

贴 IDL_F/TSK_F/SRV_F/DSRV_F/RSYS_F/VM_F/USR_F const 定义，标注对齐 priv.h:36-49。

#### §4.3 KPriv 6 子结构 (kpriv.rs:126-314)

贴 6 个子结构定义（PrivCapability/PrivSignals/PrivIpc/PrivIo/PrivMem/PrivRuntime）+ KPriv 聚合结构，标注：
- 每个子结构的职责
- `const fn new()` 可行性
- 字段 C 前缀 `s_` 保留追溯

#### §4.4 PrivTable (kpriv.rs:385-554)

贴 `PrivTable` struct + `const fn new()` + `assign_static` + `configure_boot_priv` + `grant_capability`，标注：
- 固定数组存储（D6）
- `assign_static` 对应 C `get_priv` 静态分支
- `grant_capability` 模板授予（D3）

#### §4.5 CapabilityTemplate + ProcessCapability (capability.rs)

贴 `ProcessCapability` bitflags + `CapabilityTemplate` enum + `TrapMask`/`IpcMask`/`KCallMask` newtype，标注：
- 模板到 capability 的映射（capabilities() 方法）
- Newtype 掩码的 may_send_to/contains 方法

#### §4.6 辅助函数

贴 `static_priv_id` / `is_static_priv_id` / `USER_PRIV_ID` / `NULL_PRIV_ID`，标注对齐 priv.h:10-21。

---

### Ch5. 测试（可 grep 函数名）

#### §5.1 现有测试（已实现，kpriv.rs:563-859）

| 测试函数 | 验证行为 | 对应 C 符号 |
|---------|---------|------------|
| `test_kpriv_new` | KPriv::new 初始化 s_id/s_proc_nr/s_flags | `get_priv` |
| `test_kpriv_is_sys_proc` | SYS_PROC 标志判定 | `priv(rp)->s_flags & SYS_PROC` |
| `test_kpriv_flag_predicates` | PREEMPTIBLE/BILLABLE/SYS_PROC 谓词 | `s_flags` 位 |
| `test_priv_flag_set_idl` | IDL_F = SYS_PROC\|BILLABLE | `IDL_F` priv.h:36 |
| `test_priv_flag_set_usr_no_sys_proc` | USR_F 无 SYS_PROC | `USR_F` priv.h:49 |
| `test_priv_flag_set_vm` | VM_F = SYS_PROC\|VM_SYS_PROC | `VM_F` priv.h:48 |
| `test_priv_table_new` | 表初始化 + 边界 | `priv[NR_SYS_PROCS]` |
| `test_priv_table_const_init_sets_per_slot_s_id` | 每 slot s_id 正确 | `ppriv_addr` |
| `test_priv_table_assign_static` | 静态分配 + 重复检测 | `get_priv` EBUSY |
| `test_priv_table_assign_static_user_proc` | 用户进程分配 | `static_priv_id` |
| `test_priv_table_configure_boot_priv` | 配置 boot priv | `main.c:178-248` |
| `test_k_call_mask_constants` | NO_C/ALL_C 常量 | `NO_C`/`ALL_C` priv.h:28-29 |
| `test_ipc_to_constants` | NO_M/ALL_M 常量 | `NO_M`/`ALL_M` priv.h:24-25 |
| `test_configure_boot_priv_sets_masks` | RSYS_F + ALL_M + ALL_C | `SRV_M`/`SRV_KC` |
| `test_may_send_to` | IPC 目标位图检查 | `may_send_to` priv.h:86 |
| `test_static_priv_id` | static_priv_id 公式 | `static_priv_id` priv.h:12 |
| `test_is_static_priv_id` | 静态 ID 判定 | `is_static_priv_id` priv.h:11 |
| `test_user_priv_id` | USER_PRIV_ID = NR_TASKS+11 | `USER_PRIV_ID` priv.h:18 |
| `test_io_range_new` | IoRange 零初始化 | `io_range` |
| `test_mem_range_new` | MemRange 零初始化 | `minix_mem_range` |
| `test_kpriv_alarm_timer_default_none` | 闹钟默认 None | `tmr_inittimer` |
| `test_kpriv_alarm_timer_some_carries_action` | 闹钟携带 TimerId | `set_kernel_timer` |
| `test_grant_capability_idle` | Idle 模板 | `IDL_F` |
| `test_grant_capability_vm` | Vm 模板 | `VM_F` |
| `test_grant_capability_root_service` | RootService 模板 | `RSYS_F` |
| `test_grant_capability_deferred_no_flags` | Deferred 空模板 | 无 flag |
| `test_grant_capability_duplicate_fails` | 重复分配失败 | `EBUSY` |

#### §5.2 现有测试（capability.rs:278-356）

| 测试函数 | 验证行为 |
|---------|---------|
| `capability_is_kernel_task_only_tsk_f` | KernelTask 模板 |
| `capability_idle_has_idl_f_and_billable` | Idle 模板 |
| `capability_vm_has_vm_f_and_is_system_service` | Vm 模板 |
| `capability_root_service_has_rsys_f` | RootService 模板 |
| `capability_deferred_is_empty` | Deferred 模板 |
| `template_kcall_mask_idle_is_none` | Idle/KernelTask 无 kcall |
| `template_kcall_mask_vm_is_all` | Vm/RootService 全 kcall |
| `ipc_mask_may_send_to` | IpcMask 位检查 |
| `trap_mask_contains` | TrapMask 包含 |
| `kcall_mask_default_is_none` | KCallMask 默认 |

#### §5.3 待补充测试

| 测试函数 | 验证行为 | 依赖 |
|---------|---------|------|
| `test_grant_capability_kernel_task` | KernelTask 模板授予 | `TSK_F` |
| `test_ipc_to_no_m_all_m_constants` | NO_M/ALL_M 语义 | priv.h:24-25 |
| `test_priv_table_get_out_of_range_returns_none` | 越界 ID 返回 None | NR_SYS_PROCS |
| `test_may_send_to_out_of_range` | sys_id≥64 返回 false | `may_send_to` |

---

### Ch6. 参见

- [11-scheduling-primitives.md](11-scheduling-primitives.md) — PREEMPTIBLE/BILLABLE 与调度
- [17-syscall-process.md](17-syscall-process.md) — fork 分配 priv、RTS_NO_PRIV
- [18-syscall-copy.md](18-syscall-copy.md) — grant 表用于 safecopy
- [20-syscall-device.md](20-syscall-device.md) — CHECK_IO_PORT/CHECK_IRQ
- [21-syscall-clock.md](21-syscall-clock.md) — SYS_PROC 权限位、s_alarm_timer
- [23-ipc-filter.md](23-ipc-filter.md) — s_ipc_to 与 IPC 过滤

---

## 三、知识点覆盖矩阵

| 概念组 | Ch1 | Ch2 | Ch3 | Ch4 | Ch5 |
|--------|-----|-----|-----|-----|-----|
| A. priv 模型 | §1.1 | §2.2 struct priv | D1 (6 子结构), D6 (固定数组) | §4.3, §4.4 | test_priv_table_* |
| B. s_flags | §1.2 | §2.1 IDL_F.., §2.2 s_flags | D2 (bitflags), D4 (双系统) | §4.1, §4.2, §4.5 | test_priv_flag_set_*, test_kpriv_flag_* |
| C. ID 分配 | §1.4 | §2.1 static_priv_id, §2.3 get_priv | D7 (Option) | §4.4, §4.6 | test_static_priv_id, test_assign_static |
| D. I/O/IRQ/MEM | §1.5 | §2.2 s_io_tab/s_irq_tab/s_mem_tab | — | §4.3 PrivIo/PrivMem | test_io_range, test_mem_range |
| E. 掩码 | §1.3 | §2.1 NO_M/ALL_M, §2.2 s_ipc_to/s_k_call_mask | D5 (Newtype) | §4.5 IpcMask/KCallMask | test_may_send_to, test_ipc_to_constants |
| F. 模板授予 | (贯穿) | §2.3 do_privctl | D3 (CapabilityTemplate) | §4.4 grant_capability, §4.5 | test_grant_capability_* |
| G. anti-translate | (贯穿) | (贯穿) | D1-D10 | §4 全部 | (贯穿) |

---

## 四、断裂修复表

| 断裂点 | 修复方案 |
|--------|---------|
| doc §4.2 扁平字段列表 vs 代码 6 子结构 | Ch4 §4.3 重写为 6 子结构，标注 D1 |
| doc 未提 capability.rs/CapabilityTemplate | Ch3 D3 新增模板设计；Ch4 §4.5 贴 capability.rs |
| doc §4.3 IoRange base/limit 写 u16 | 修正为 u32（对齐 kpriv.rs:32-33） |
| doc §4.2 s_alarm_timer 写 Option<TimerEntry> | 修正为 Option<(TimerEntry, TimerId)>（D8） |
| doc §补充 引用 tmp 文件 | 删除 tmp 来源，改为 C 源码直接引用 |
| doc Ch3 决策表平庸（无 hypothesis） | Ch3 改为 hypothesis-driven（D1-D10） |
| doc 未提 ProcessCapability 双系统 | Ch3 D4 新增双系统说明 |
| doc 未提 Newtype 掩码 | Ch3 D5 新增 Newtype 推理；Ch4 §4.5 |
| doc §1.2 CHECK_IPC 注释位置 | 保留，移至 §1.3 三类掩码处更合适 |
| doc 未对照 redox | Ch1 §1.1 新增 redox scheme/capability 对照 |

---

## 五、自检

- [x] Ch1 主语是权限/特权，非函数名/结构体名
- [x] Ch1 每节有"灵魂本质"一句话
- [x] Ch1 采用 WHY→WHAT→HOW 弧线
- [x] Ch2 每个符号带 file:line
- [x] Ch3 采用 hypothesis-driven（D1-D10 每个"如果 X 会有 Y 问题所以用 Z"）
- [x] Ch3 无迭代叙事
- [x] Ch4 贴真实代码（6 子结构 + grant_capability + capability.rs）
- [x] Ch5 测试函数可 grep 验证（37 个 fn test_*）
- [x] 知识点覆盖矩阵完整（A-G 七组）
- [x] 断裂修复表完整（10 处断裂 + 修复方案）
- [x] 无 tmp 文件引用
- [x] 无内部 review ID
- [x] 无迭代叙事日期
- [x] anti-translate 体现（bitflags/Newtype/Option/CapabilityTemplate/6子结构）
- [x] 与 21-syscall-clock 衔接（SYS_PROC 位、s_alarm_timer）
- [x] redox 对照（§1.1 scheme/capability 模型）
