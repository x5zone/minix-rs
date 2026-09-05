# 22-privilege: 权限管理

> **分类**: 运行时基础设施
> **源码**: `minix3/minix/kernel/priv.h`, `minix3/minix/include/minix/priv.h`, `minix3/minix/kernel/system.c:274-321,918-989`
> **Rust 实现**: `os/kernel/src/kpriv.rs` (1200 行), `os/kernel/src/capability.rs` (361 行)
> **前置**: 11（调度——进程状态）, 16（进程管理——fork/privctl）, 17（syscall-process——fork 分配 priv）
> **C 总行数**: ~400 行

---

## Ch1: 概念

**核心问题**: 内核如何控制进程的权限——谁能发 IPC 给谁、谁能调用哪些内核调用、谁能访问哪些 I/O 端口/IRQ/内存？

Minix3 的权限模型基于 `struct priv`：

- **系统进程**（VM/PM/VFS/RS 等）：每个有独立的 `priv` 结构，定义了完整的权限集
- **用户进程**：共享 `USER_PRIV`，权限最小化

这种"分治"策略平衡了空间效率与权限隔离——系统进程（≤64 个）各有独立 priv 结构，所有用户进程共享单一 `USER_PRIV` 结构，`p_priv` 指针指向所属 priv。

> **redox 对照**: redox 用 scheme 命名空间 + capability token 模型，非位图。Minix3 的位图模型适合固定数量系统进程（≤64），O(1) 检查且 ID 稳定；redox 的 scheme 模型支持动态命名但需路径解析。两种模型在不同场景各有优劣。

### 1.1 struct priv 关键字段

`struct priv`（`minix3/minix/kernel/priv.h:21-66`）含 31 个字段，按职责可分为 6 组：

| 职责组 | 字段 | 语义 |
|--------|------|------|
| 身份/能力 | `s_proc_nr`, `s_id`, `s_flags`, `s_init_flags` | 关联进程、索引、标志位 |
| 信号 | `s_sig_mgr`, `s_bak_sig_mgr`, `s_notify_pending`, `s_asyn_pending`, `s_int_pending`, `s_sig_pending` | 信号管理器 + 挂起位图 |
| IPC | `s_trap_mask`, `s_ipc_to`, `s_k_call_mask` | trap/IPC目标/内核调用掩码 |
| I/O | `s_nr_io_range`, `s_io_tab`, `s_nr_irq`, `s_irq_tab` | I/O 端口 + IRQ 范围表 |
| 内存 | `s_nr_mem_range`, `s_mem_tab`, `s_ipcf`, `s_stack_guard`, `s_diag_sig` | 内存范围 + IPC filter + 栈保护 |
| 运行时 | `s_alarm_timer`, `s_grant_table`, `s_grant_entries`, `s_grant_endpoint`, `s_state_table`, `s_state_entries`, `s_asyntab`, `s_asynsize`, `s_asynendpoint` | 闹钟 + grant 表 + state 表 + 异步表 |

### 1.2 s_flags 权限标志

> C 源码: `minix/include/minix/const.h:143-154`

| 标志 | 值 | 含义 |
|------|-----|------|
| `PREEMPTIBLE` | 0x002 | 进程可被抢占 |
| `BILLABLE` | 0x004 | 进程的时间可被记账 |
| `DYN_PRIV_ID` | 0x008 | 动态分配的特权 ID |
| `SYS_PROC` | 0x010 | 系统进程（有独立 priv 结构） |
| `CHECK_IO_PORT` | 0x020 | 启用 I/O 端口访问检查 |
| `CHECK_IRQ` | 0x040 | 启用 IRQ 访问检查 |
| `CHECK_MEM` | 0x080 | 启用内存映射权限检查 |
| `ROOT_SYS_PROC` | 0x100 | 根系统进程 |
| `VM_SYS_PROC` | 0x200 | VM 系统进程 |
| `LU_SYS_PROC` | 0x400 | Live Update 系统进程 |
| `RST_SYS_PROC` | 0x800 | 重启系统进程 |

> **关键不对称**: C 源码中**不存在** `CHECK_IPC` 标志。IPC 目标过滤通过 `s_ipc_to` 位图**无条件**执行——`may_send_to()` 宏（`priv.h:86`）始终检查 `s_ipc_to`，无需标志位启用。这与 `CHECK_IO_PORT`/`CHECK_IRQ`/`CHECK_MEM` 的可选检查模式不同——后者需要标志位启用才检查。

### 1.3 三类权限掩码

权限通过三类位图掩码精确控制：

| 掩码 | C 字段 | 类型 | 语义 | 检查时机 |
|------|--------|------|------|---------|
| IPC 目标 | `s_ipc_to` | `sys_map_t` (64-bit) | 允许发送的 endpoint 位图 | 每次 IPC send（无条件） |
| 内核调用 | `s_k_call_mask` | `bitchunk_t[2]` | 允许的 kernel call 号位图 | 每次 kernel call dispatch |
| Trap 原语 | `s_trap_mask` | `short` | 允许的 IPC 原语位图 | trap 入口 |

**IPC 目标检查**: `may_send_to(rp, nr)` 宏（`priv.h:86`）= `get_sys_bit(priv(rp)->s_ipc_to, nr_to_id(nr))`。异步扩展 `may_asynsend_to(rp, nr)`（`priv.h:87`）= `may_send_to(rp, nr) || rp->p_nr == nr`（允许发给自身）。

**特殊值**（`priv.h:24-30`）:
- `NO_M`(-1) / `ALL_M`(-2): IPC 目标无/全部
- `NO_C`(-1) / `ALL_C`(-2): 内核调用无/全部

**预定义掩码组合**（`priv.h:59-75`）:
| 进程类型 | trap | IPC 目标 | 内核调用 |
|---------|------|---------|---------|
| 时钟/系统 (CSK_T) | `1<<RECEIVE` | — | — |
| 内核任务 (TSK_T/M/KC) | 0 | NO_M | NO_C |
| 系统服务 (SRV_T/M/KC) | ~0 | ALL_M | ALL_C |
| 用户进程 (USR_T/M/KC) | `1<<SENDREC` | ALL_M | NO_C |

### 1.4 priv 表结构与 ID 分配

```
priv[NR_SYS_PROCS=64]:
  索引 0..NR_STATIC_PRIV_IDS-1    静态区（boot image 进程）
  索引 NR_STATIC_PRIV_IDS..63     动态区（运行时分配的系统进程）
```

| 函数/常量 | 公式/值 | 位置 | 用途 |
|---------|--------|------|------|
| `static_priv_id(n)` | `NR_TASKS + n` | `priv.h:12` | boot 进程 → priv_id |
| `is_static_priv_id(id)` | `id >= 0 && id < NR_STATIC_PRIV_IDS` | `priv.h:11` | 判断是否静态 |
| `USER_PRIV_ID` | `static_priv_id(ROOT_USR_PROC_NR)` | `priv.h:18` | 用户进程共享 ID |
| `NULL_PRIV_ID` | `-1` | `priv.h:21` | 空 ID（触发动态分配） |
| `NR_STATIC_PRIV_IDS` | `NR_BOOT_PROCS` | `priv.h:10` | 静态区大小 |

**get_priv 分配逻辑**（`system.c:274-302`）:
- `priv_id == NULL_PRIV_ID` → 扫描动态区找空闲 slot（`s_proc_nr == NONE`），满则 `ENOSPC`
- `priv_id` 是静态 ID → 检查 `is_static_priv_id` + slot 未占用（否则 `EINVAL`/`EBUSY`），分配
- 分配后：`rc->p_priv = sp; sp->s_proc_nr = proc_nr(rc)`

**fork 中的特权降级**（`do_fork.c:104-107`）:
- 用户进程 fork → 子进程继承 `USER_PRIV_ID`
- 系统进程 fork → 子进程降级为 `USER_PRIV_ID` + `RTS_NO_PRIV`，需 RS 重新授权

### 1.5 I/O 端口/IRQ/内存范围

`CHECK_IO_PORT`/`CHECK_IRQ`/`CHECK_MEM` 标志启用可选检查，系统进程通过 `priv_add_io`/`priv_add_irq`/`priv_add_mem` 逐条添加权限范围：

| 类型 | 字段 | 容量 | 标志 | 添加函数 |
|------|------|------|------|---------|
| I/O 端口 | `s_io_tab[NR_IO_RANGE=64]` | 64 | CHECK_IO_PORT | `priv_add_io` (system.c:945) |
| IRQ | `s_irq_tab[NR_IRQ=16]` | 16 | CHECK_IRQ | `priv_add_irq` (system.c:918) |
| 内存 | `s_mem_tab[NR_MEM_RANGE=20]` | 20 | CHECK_MEM | `priv_add_mem` (system.c:973) |

每个范围表项含 base + limit，检查时线性扫描（数量小，O(N) 可接受）。

> **本章边界**: 本章聚焦 `priv` 结构的字段语义与权限模型；syscall 层的 `do_privctl`（运行时权限控制入口）及 `get_priv` 动态分配路径见 [17-syscall-process.md](17-syscall-process.md)，IPC 过滤机制见 [23-ipc-filter.md](23-ipc-filter.md)。

---

## Ch2: C 源码分析

### include/minix/priv.h (105 行)

预定义权限组合的"配置表"——定义各类进程的默认 flags/trap/mask/call/scheduler/queue。

| 行号 | 内容 | 说明 |
|------|------|------|
| 10-12 | `NR_STATIC_PRIV_IDS`, `is_static_priv_id`, `static_priv_id` | 静态 ID 宏 |
| 18-21 | `USER_PRIV_ID`, `NULL_PRIV_ID` | 特殊 ID |
| 24-30 | `NO_M`/`ALL_M`/`NO_C`/`ALL_C`/`NULL_C` | 掩码特殊值 |
| 36-50 | `IDL_F`..`IMM_F` | 预定义标志组合 |
| 53-56 | `TSK_I`..`USR_I` | init flags |
| 59-63 | `CSK_T`..`USR_T` | trap 掩码组合 |
| 66-69 | `TSK_M`..`USR_M` | IPC 目标组合 |
| 72-75 | `TSK_KC`..`USR_KC` | 内核调用掩码组合 |
| 83-90 | `SRV_SM`..`USR_SCH` | 信号管理器/调度器 |

### kernel/priv.h (105 行)

`struct priv` 完整定义 + 访问宏。

| 行号 | 内容 | 说明 |
|------|------|------|
| 21-66 | `struct priv` | 完整 priv 结构（31 字段） |
| 69 | `STACK_GUARD` | 栈保护字（0xDEADBEEF） |
| 72-77 | `BEG_PRIV_ADDR` 等 | 表地址宏 |
| 79 | `priv_addr(i)` | 按 ID 取 priv 指针 |
| 80-81 | `priv_id(rp)`/`priv(rp)` | 取进程的 priv |
| 83-84 | `id_to_nr`/`nr_to_id` | ID↔进程号转换 |
| 86 | `may_send_to(rp,nr)` | IPC 目标检查宏 |
| 87 | `may_asynsend_to(rp,nr)` | 异步发送检查 |
| 94-95 | `priv[]`/`ppriv_addr[]` | 全局表 + 指针表 |

### system.c:274-321, 918-989

| 行号 | 函数 | 说明 |
|------|------|------|
| 274-302 | `get_priv(rc, priv_id)` | 分配 priv（静态/动态） |
| 307-330 | `set_sendto_bit(rp, id)` | 设置 s_ipc_to 位（含自身/空目标保护） |
| 918-941 | `priv_add_irq(rp, irq)` | 添加 IRQ 到 s_irq_tab |
| 945-969 | `priv_add_io(rp, ior)` | 添加 I/O 范围到 s_io_tab |
| 973-997 | `priv_add_mem(rp, memr)` | 添加内存范围到 s_mem_tab |

---

## Ch3: Rust 设计决策

| # | 决策 | 选项 | 结论 | 理由 |
|---|------|------|------|------|
| D1 | KPriv 结构 | 扁平 vs 8 子结构 | **8 子结构** | 如果用扁平 struct，31 字段职责混杂，难以按职责 const fn 初始化；8 子结构（PrivIdentity/PrivInit/PrivFlags/PrivSignals/PrivIpc/PrivIo/PrivMem/PrivRuntime）按职责分组，每子结构独立 `const fn new()`，PrivTable 可用 `[KPriv; 64]` const 初始化 |
| D2 | s_flags | 裸 short vs bitflags | **`ProcessCapability` bitflags（C wire 位布局）** | 如果用裸 i16，`s_flags = 0x030` 无法区分意图，函数签名无法区分"任意 i16"与"权限标志"；bitflags 使位组合成为 first-class 值，`contains(SYS_PROC)` 类型安全。低 11 位 = C wire 位布局，wire 边界 `from_wire`/`to_wire` 编解码 |
| D3 | 权限授予 | 分散设置 vs CapabilityTemplate | **模板 enum** | 如果用 C 的分散设置（get_priv + 逐字段），调用者需记住每类进程的掩码，易漏（如忘设 IPC 掩码→进程无法通信）；5 个模板（Idle/KernelTask/Vm/RootService/Deferred）封装正确组合，correct-by-construction |
| D4 | 能力位类型 | 统一 vs 双系统 | **单一 `ProcessCapability`** | 低 11 位采 C wire 位布局（const.h:143-154）+ priv.h:36-50 组合位，Rust 扩展位（KILL/SIGS_SYS/OWN_ID）位于 16 位 wire 范围外，`from_wire`/`to_wire` 在 `PrivUpdateRequest`/`PrivInfoStruct`/GET_WHOAMI 边界编解码；`grant_capability` 直接落位模板 flag set。若保留双系统，每次授予都要跨 bitflags 翻译，且用户态导出的 s_flags 位布局会与 C 端错位 |
| D5 | 掩码类型 | 裸整数 vs Newtype | **Newtype** | 如果用裸 u64，类型系统无法区分 IPC 目标位图与 kernel call 位图——函数签名 `fn set_mask(mask: u64)` 可传入任意 u64；`IpcMask(u64)`/`KCallMask(u64)`/`TrapMask(u32)` Newtype 使三种掩码不可互换 |
| D6 | PrivTable 存储 | 堆分配 vs 固定数组 | **固定数组** | 如果用 `Box<[KPriv]>`，boot 阶段无堆分配器（no_std + allocator 未初始化）；`[KPriv; NR_SYS_PROCS]` + `const fn new()` 编译期已知大小，匹配 C 的 BSS 布局 |
| D7 | s_proc_nr | sentinel NONE vs Option | **`Option<ProcNr>`** | 如果用 i32 + NONE(-1) sentinel，-1 是合法 i32 值，类型系统无法阻止误用；`Option<ProcNr>` 强制处理"未分配"情况 |
| D8 | s_alarm_timer | 裸 minix_timer_t vs Option | **`Option<(TimerEntry, TimerId)>`** | 如果用 `Option<TimerEntry>`，丢失 reset_timer 所需的 TimerId（C 用指针，Rust 用 id），取消闹钟时无法 O(log N) 移除；tuple 携带完整状态，"非法状态不可表达" |
| D9 | s_ipcf/s_stack_guard | 裸指针 vs Option<usize> | **Option<usize>（当前限制）** | usize 不是类型安全的指针；`Option<*mut T>` 不 Send/Sync。当前对齐 C 裸指针语义，redesign 阶段引入 `NonNull<T>`。标 P2 |
| D10 | PrivId/SysId | type alias vs newtype | **type alias（当前限制）** | 改 newtype 需更新所有 callsite，影响面大。当前 `static_priv_id`/`is_static_priv_id` 语义已清晰。标 P2 |

> **anti-translate 总结**: 8 处 Rust 惯用法替代 C 模式（bitflags/**8 子结构**/Option/CapabilityTemplate/Newtype/固定数组/Option tuple），2 处已知限制（D9/D10）标 P2 后续改进。

---

## Ch4: 实现要点

### 4.1 ProcessCapability（能力位，含 C wire 位布局）

> Rust 实现: `os/kernel/src/capability.rs:67-160`（内核唯一能力位类型；kpriv.rs `PrivFlags` 字段采用，wire 编解码见下）

```rust
// C: minix/include/minix/const.h:143-154（低 11 位 = C wire 位布局，1:1）
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct ProcessCapability: u32 {
        // C 原子位（位置 = wire 位布局）
        const PREEMPTIBLE     = 0x0000_0002;  // const.h:143
        const BILLABLE        = 0x0000_0004;  // const.h:144
        const DYN_PRIV_ID     = 0x0000_0008;  // const.h:145
        const SYS_PROC        = 0x0000_0010;  // const.h:147
        const CHECK_IO_PORT   = 0x0000_0020;  // const.h:148
        const CHECK_IRQ       = 0x0000_0040;  // const.h:149
        const CHECK_MEM       = 0x0000_0080;  // const.h:150
        const ROOT_SYS_PROC   = 0x0000_0100;  // const.h:151
        const VM_SYS_PROC     = 0x0000_0200;  // const.h:152
        const LU_SYS_PROC     = 0x0000_0400;  // const.h:153
        const RST_SYS_PROC    = 0x0000_0800;  // const.h:154
        // Rust 扩展位（C 无对应位；位于 16 位 wire 范围外，不跨 wire）
        const KILL            = 0x0001_0000;
        const SIGS_SYS        = 0x0002_0000;
        const OWN_ID          = 0x0004_0000;
        // C 预定义组合（priv.h:36-49）——原子位的 OR，非独立位
        const IDL_F           = Self::SYS_PROC.bits() | Self::BILLABLE.bits();
        const TSK_F           = Self::SYS_PROC.bits();
        const SRV_F           = Self::SYS_PROC.bits() | Self::PREEMPTIBLE.bits();
        const DSRV_F          = Self::SRV_F.bits() | Self::DYN_PRIV_ID.bits();
        const RSYS_F          = Self::SRV_F.bits() | Self::ROOT_SYS_PROC.bits();
        const VM_F            = Self::SYS_PROC.bits() | Self::VM_SYS_PROC.bits();
        const USR_F           = Self::BILLABLE.bits() | Self::PREEMPTIBLE.bits();
    }
}

// wire 编解码（PrivUpdateRequest.s_flags: u16 ↔ ProcessCapability）
impl ProcessCapability {
    pub const fn from_wire(wire: u16) -> Self { Self::from_bits_truncate(wire as u32) }
    pub const fn to_wire(self) -> u16 { (self.bits() & 0xFFFF) as u16 }
}
```

> **wire 边界约定**：`PrivUpdateRequest.s_flags` 保持裸 `u16`——位值采 C 布局是**刻意选择**而非 IPC 强制（真实硬契约是内核↔RS 内部一致；采 C 位值的理由——用户态导出路径 GET_WHOAMI/GET_PRIV + ground truth——见 [06 §3.10 协议边界](./06-proc-init-boot-proc.md)）；`ProcessCapability` 只在 kpriv.rs `update_from_request` / misc.rs `PrivInfoStruct::from_kpriv` / GET_WHOAMI 三处与 wire 互转。Rust 扩展位（KILL/SIGS_SYS/OWN_ID）高于 bit 15，`to_wire` 截断、`from_wire` 不可能读入。

### 4.2 KPriv 8 子结构

> Rust 实现: `os/kernel/src/kpriv.rs:140-400`

KPriv 按 8 个职责组拆分为子结构（与 Ch3 D1 决策一致），每个子结构独立 `const fn new()` 构造：

```rust
// 1. 身份绑定（"我绑定哪个 ProcNr + 我的 slot id"）
pub(crate) struct PrivIdentity {
    pub(crate) s_proc_nr: Option<ProcNr>,   // D7: Option vs sentinel NONE
    pub(crate) s_id: SysId,
}

// 2. init 阶段标志（运行期逐步清零）
pub(crate) struct PrivInit {
    pub(crate) s_init_flags: i32,
}

// 3. 能力位掩码（5 种角色 + C wire 位布局，见 §4.1）
pub(crate) struct PrivFlags {
    pub(crate) s_flags: ProcessCapability,   // D2: bitflags vs short
}

// 4. 信号簿记（异步表、管理器、挂起信号）
pub(crate) struct PrivSignals {
    pub(crate) s_asyntab: u64,
    pub(crate) s_asynsize: usize,
    pub(crate) s_asynendpoint: Endpoint,
    pub(crate) s_sig_mgr: Endpoint,
    pub(crate) s_bak_sig_mgr: Endpoint,
    pub(crate) s_notify_pending: u64,
    pub(crate) s_asyn_pending: u64,
    pub(crate) s_int_pending: u32,
    pub(crate) s_sig_pending: SigSet,
}

// 5. IPC 允许列表（trap、ipc-to、kernel-call 掩码）——D5 Newtype
pub(crate) struct PrivIpc {
    pub(crate) s_trap_mask: TrapMask,        // D5: wire 宽度（u16）保留在 PrivUpdateRequest
    pub(crate) s_ipc_to: IpcMask,            // D5: wire 宽度（u64）保留在 PrivUpdateRequest
    pub(crate) s_k_call_mask: KCallMask,     // D5: wire 宽度（[u32; 2]）保留在 PrivUpdateRequest
}

// 6. I/O 端口 + IRQ 允许列表
pub(crate) struct PrivIo {
    pub(crate) s_nr_io_range: i32,
    pub(crate) s_io_tab: [IoRange; NR_IO_RANGE],
    pub(crate) s_nr_irq: i32,
    pub(crate) s_irq_tab: [i32; NR_IRQ],
}

// 7. 内存范围允许列表 + 跨空间 IPC + 栈保护
pub(crate) struct PrivMem {
    pub(crate) s_nr_mem_range: i32,
    pub(crate) s_mem_tab: [MemRange; NR_MEM_RANGE],
    pub(crate) s_ipcf: Option<usize>,        // D9: raw ptr (limitation)
    pub(crate) s_stack_guard: Option<usize>,  // D9: raw ptr (limitation)
    pub(crate) s_diag_sig: bool,
}

// 8. 运行时状态（闹钟 + grant 表 + state 表）
pub(crate) struct PrivRuntime {
    pub(crate) s_alarm_timer: Option<(crate::clock::TimerEntry, crate::clock::TimerId)>,  // D8
    pub(crate) s_grant_table: usize,
    pub(crate) s_grant_entries: i32,
    pub(crate) s_grant_endpoint: Endpoint,
    pub(crate) s_state_table: usize,
    pub(crate) s_state_entries: i32,
}

pub(crate) struct KPriv {
    pub(crate) identity: PrivIdentity,
    pub(crate) flags: PrivFlags,
    pub(crate) init: PrivInit,
    pub(crate) signals: PrivSignals,
    pub(crate) ipc: PrivIpc,
    pub(crate) io: PrivIo,
    pub(crate) mem: PrivMem,
    pub(crate) runtime: PrivRuntime,
}
```

> **协议结构字段序**：跨空间载荷 `PrivUpdateRequest`（`SYS_PRIV_SET_SYS`/`UPDATE_SYS` 的 `data_copy` 载荷，[kpriv.rs](file:///os/kernel/src/kpriv.rs)）字段序镜像上述子结构序（去内核私有的 `PrivRuntime`：identity → flags → init → signal managers → IPC 掩码 → I/O → IRQ → memory）；RS 侧填写结构 `Privilege` 同序对照。契约边界（位值跟 C、布局跟自己）见 [06 §3.10](./06-proc-init-boot-proc.md)。

> **三子结构分离的原因（D1 修订说明）**：初版曾把 `PrivIdentity + PrivInit + PrivFlags` 合并为 `PrivCapability`，按"读写时机 / 锁粒度"切为身份 / init / 能力三域。后改为3 个独立子结构——原因是每子结构类型独立、`const fn new()` 接口对齐，且三者的「关联进程」「init 阶段」「能力位」在域语义上确实独立（读 `s_proc_nr` 与读 `s_flags` 的调用栈完全不同）。`PrivCapability` 是历史命名，已统一为 `PrivIdentity` / `PrivInit` / `PrivFlags`。

**IoRange**（`kpriv.rs:32-35`，base/limit 为 u32 对齐 C `struct io_range`）:

```rust
pub struct IoRange {
    pub base: u32,
    pub limit: u32,
}
```

**关键方法**（谓词收口在 `PrivFlags`，位图检查收口在 newtype）:

```rust
impl PrivFlags {
    pub(crate) fn is_sys_proc(&self) -> bool {
        self.s_flags.contains(ProcessCapability::SYS_PROC)
    }
    #[allow(dead_code)]
    pub(crate) fn is_preemptible(&self) -> bool {
        self.s_flags.contains(ProcessCapability::PREEMPTIBLE)
    }
    // ... (更多谓词方法)
}

impl KPriv {
    pub fn may_send_to(&self, target_id: SysId) -> bool {
        if target_id as usize >= 64 { return false; }
        self.ipc.s_ipc_to.may_send_to(target_id as u8)
    }
}
```

### 4.3 PrivTable 固定数组

> Rust 实现: `os/kernel/src/kpriv.rs:633-894`

```rust
pub const NR_SYS_PROCS: usize = 64;

pub struct PrivTable {
    privs: [KPriv; NR_SYS_PROCS],  // D6: 固定数组，无堆分配
}

impl PrivTable {
    /// const fn 初始化（boot 阶段无分配器可用）
    pub const fn new() -> Self {
        let mut privs = [const { KPriv::new_zeroed(0) }; NR_SYS_PROCS];
        let mut i = 0;
        while i < NR_SYS_PROCS {
            privs[i].identity.s_id = i as SysId;
            i += 1;
        }
        Self { privs }
    }

    /// 静态 priv 分配。C: get_priv() — system.c:274-302
    pub fn assign_static(&mut self, proc_nr: ProcNr) -> Option<PrivId> {
        // C: priv_id = static_priv_id(proc_nr) = NR_TASKS + proc_nr
        let priv_id = if proc_nr.0 < 0 {
            (NR_TASKS as i32 + proc_nr.0) as PrivId
        } else {
            (NR_TASKS as PrivId + proc_nr.0 as PrivId) as PrivId
        };
        let priv_ = self.get_mut(priv_id)?;
        if priv_.identity.s_proc_nr.is_some() { return None; }  // EBUSY
        priv_.identity.s_proc_nr = Some(proc_nr);
        Some(priv_id)
    }

    /// 模板授予（D3）。correct-by-construction，无法漏设掩码。
    pub fn grant_capability(
        &mut self,
        proc_nr: ProcNr,
        template: CapabilityTemplate,
    ) -> Result<PrivId, CapabilityError> {
        let priv_id = self.assign_static(proc_nr)
            .ok_or(CapabilityError::SlotOccupied)?;

        // capabilities() 返回的就是 C flag set（priv.h:36-49 组合），
        // 直接落位 KPriv，无跨 bitflags 转换。
        let flags = template.capabilities();

        // 角色默认 trap_mask + CLOCK/SYSTEM 的 CSK_T 例外（main.c:218-219）。
        let mut trap_mask = template.trap_mask();
        if matches!(template, CapabilityTemplate::KernelTask)
            && (proc_nr == crate::proc::proc_nr::CLOCK
                || proc_nr == crate::proc::proc_nr::SYSTEM)
        {
            trap_mask = TrapMask::RECEIVE;
        }

        // sig_mgr 默认指向自身 endpoint（匹配 C init）。
        let sig_mgr = Endpoint::from_generation_slot(0, proc_nr.0);

        if let Some(priv_) = self.get_mut(priv_id) {
            priv_.flags.s_flags = flags;
            priv_.init.s_init_flags = 0;
            priv_.ipc.s_trap_mask = trap_mask;
            priv_.ipc.s_ipc_to = template.ipc_mask();
            priv_.ipc.s_k_call_mask = template.kcall_mask();
            priv_.signals.s_sig_mgr = sig_mgr;
        }
        Ok(priv_id)
    }
}
```

### 4.4 CapabilityTemplate + Newtype 掩码

> Rust 实现: `os/kernel/src/capability.rs`

```rust
// D3: 5 个模板封装正确的 flag+mask 组合
pub enum CapabilityTemplate {
    Idle,         // IDL_F: SYS_PROC|BILLABLE, no IPC/kcall
    KernelTask,   // TSK_F: SYS_PROC, no IPC/kcall
    Vm,           // VM_F: SYS_PROC|VM_SYS_PROC, ALL_M/ALL_C
    RootService,  // RSYS_F: SRV_F|ROOT_SYS_PROC, ALL_M/ALL_C
    Deferred,     // empty, NO_M/NO_C（等待 RS 运行时授权）
}

// D5: Newtype 掩码，类型系统区分三种掩码
pub struct TrapMask(u32);   // was C s_trap_mask
pub struct IpcMask(u64);    // was C s_ipc_to
pub struct KCallMask(u64);  // was C s_k_call_mask

impl IpcMask {
    pub const fn may_send_to(self, sys_id: u8) -> bool {
        if sys_id >= 64 { return false; }
        (self.0 & (1u64 << sys_id)) != 0
    }
}
```

**D4 统一**: `ProcessCapability`（[capability.rs:67](file:///os/kernel/src/capability.rs#L67)，u32）是唯一能力位类型——低 11 位采 C wire 位布局（const.h:143-154），`*_F` 为 priv.h:36-50 组合位，`from_wire`/`to_wire` 在 wire 边界编解码；Rust 扩展位（KILL/SIGS_SYS/OWN_ID）位于 16 位 wire 范围外、永不跨 wire。`grant_capability` 直接落位模板 flag set，无转换点。

### 4.5 辅助函数

> Rust 实现: `os/kernel/src/kpriv.rs:111-129`

```rust
// C: priv.h:18 — USER_PRIV_ID = static_priv_id(ROOT_USR_PROC_NR)
pub const USER_PRIV_ID: PrivId = NR_TASKS as PrivId + INIT_PROC_NR;

// C: priv.h:12 — static_priv_id(n) = NR_TASKS + n
pub fn static_priv_id(proc_nr: ProcNr) -> PrivId {
    (NR_TASKS as i32 + proc_nr.0) as PrivId
}

// C: priv.h:11 — is_static_priv_id(id)
pub fn is_static_priv_id(id: PrivId) -> bool {
    let nr_static = NR_BOOT_PROCS as PrivId;
    id < nr_static
}

// C: priv.h:21 — NULL_PRIV_ID = -1
pub const NULL_PRIV_ID: PrivId = u16::MAX;
```

> **覆盖状态（2026-08-13 Phase 6 更新）**: `NULL_PRIV_ID` 在 C 中是动态分配的触发值（`get_priv` 见 `NULL_PRIV_ID` → 扫描动态区）。Rust 的动态分配路径已由 `KPriv::get_priv`（kpriv.rs:764-795）实现，覆盖 `NULL_PRIV_ID` → 扫描 `[NR_BOOT_PROCS .. NR_SYS_PROCS)` 动态 slot + 静态 slot 校验 + `EBUSY`/`ENOSPC`/`EINVAL` 错误码（详见 §4.6）。

### 4.6 C 函数覆盖状态

以下 C 函数在 Ch2 中分析，覆盖状态如下（syscall 层 `dispatch_privctl` 在 [13-syscall-dispatch.md](13-syscall-dispatch.md) 覆盖；`do_update` 路径在 [25-misc-unported.md](25-misc-unported.md) 覆盖）。

> **2026-08-13 Phase 6 更新**：`get_priv` 动态分支已由 `KPriv::get_priv`（kpriv.rs）实现，覆盖 `NULL_PRIV_ID` 扫描动态区 + 静态 slot 校验 + `EBUSY`/`ENOSPC`/`EINVAL` 错误码。
>
> **2026-09-05 D-49 更新**：`set_sendto_bit` 全链路落地，本表全部条目已实现。实现要点与设计动机见 §4.6.1。

| C 函数 | C 位置 | 用途 | Rust 状态 |
|--------|--------|------|----------|
| `get_priv` 动态分支 | system.c:284-288 | `NULL_PRIV_ID` → 扫描动态区 | ✅ 已实现（`KPriv::get_priv` kpriv.rs，2026-08-13 Phase 6 落地） |
| `set_sendto_bit` | system.c:307-330 | 运行时设置 `s_ipc_to` 位 | ✅ 已实现（2026-09-05 D-49：`PrivTable::set_sendto_bit` / `unset_sendto_bit` / `fill_sendto_mask` 三原语 + `PrivTable::update_priv` 组合入口（kpriv.rs）；接线 SET_SYS 默认掩码、UPDATE_SYS、`dispatch_update` 掩码继承三处；boot 模板路径差异见 §4.6.1） |
| `priv_add_irq` | system.c:918-940 | 添加 IRQ 到 `s_irq_tab` | ✅ 已实现（`KPriv::add_irq` 方法，被 `dispatch_update` 的 `inherit_priv_irq` 调用 + `SYS_PRIV_ADD_IRQ` syscall 路径通过 `copy_struct_from_user` 调用，2026-08-13 Phase 6 落地） |
| `priv_add_io` | system.c:945-968 | 添加 I/O 范围到 `s_io_tab` | ✅ 已实现（`KPriv::add_io` 方法，被 `dispatch_update` 的 `inherit_priv_io` 调用 + `SYS_PRIV_ADD_IO` syscall 路径通过 `copy_struct_from_user` 调用，2026-08-13 Phase 6 落地） |
| `priv_add_mem` | system.c:973-996 | 添加内存范围到 `s_mem_tab` | ✅ 已实现（`KPriv::add_mem` 方法，被 `dispatch_update` 的 `inherit_priv_mem` 调用 + `SYS_PRIV_ADD_MEM` syscall 路径通过 `copy_struct_from_user` 调用，2026-08-13 Phase 6 落地） |

#### 4.6.1 sendto 掩码：为什么"置位"是全表操作

把 C 的 `set_sendto_bit` 读成"把某一位置 1"是一个很容易掉进去的陷阱——它的名字确实在说这件事，但它实际维护的是整个特权表上的一条**不变量**：`s_ipc_to` 掩码是**成对对称**的。A 能发消息给 B，B 就必须能回复 A；反过来，撤销 A→B 时 B→A 也必须一起消失。C 用三个函数从三个方向维护这条不变量：

- **`set_sendto_bit(rp, id)`**（system.c:307-329）授予 rp→id。两个守卫决定这次授予是否"退化成撤销"（system.c:316-319）：目标 slot 没有绑定进程（`id_to_nr(id) == NONE`），或者目标就是自己——两种情况下 rp 自己的这一位被显式清掉。守卫通过后，除了授予 rp→id（system.c:321），还要看目标的 trap 掩码（system.c:327-328）：一个只能 RECEIVE 的端点（CLOCK/SYSTEM 的 `CSK_T`）永远无法回复，给它回执位毫无意义，所以 C 跳过 `id→rp` 的对称授予。
- **`unset_sendto_bit(rp, id)`**（system.c:335-344）撤销 rp→id 的同时无条件撤销 id→rp。
- **`fill_sendto_mask(rp, map)`**（system.c:349-358）把整张掩码逐位重新计算——每一"置位"走 `set_sendto_bit`，每一"清位"走 `unset_sendto_bit`。

逐位重算听起来多余（直接赋值 `s_ipc_to = map` 不就完了？），但它恰恰是安全性的关键：**清位方向会顺手修复别家掩码里的陈旧回执位**。想象 RS 先授权 A↔B，后来通过 `SYS_PRIV_UPDATE_SYS` 收回 A 掩码中的 B 位——如果只赋值 A 的掩码，B→A 那一位就成了 C 的模型里永远不会出现的**单向授权**：B 仍然可以主动发消息给 A。对称不变量被打破，能力模型"授权必经 RS"的承诺出现缺口。

Rust 在 D-49 之前的实现正是踩在这个坑里：`update_from_request` 把 `req.s_ipc_to` 直接赋给 `s_ipc_to`（裸拷贝，无守卫、无回执、无修复），SET_SYS 默认路径写 `IpcMask::ALL`（包含自位与未绑定 slot），`dispatch_update` 用 union 合并 src 掩码。三处都在"位图赋值"这个更简单的模型上运行——简单，但每一处都偏离了 C 的不变量。

**为什么落在 `PrivTable` 上**。C 的 `update_priv`（do_privctl.c:280-368）操作的是"进程 + 特权表"这个系统状态：字段拷贝发生在单个 priv slot 上，掩码 fill 却要触碰全表（回执位写在别的 slot 里）。这决定了 Rust 的落点几乎没有选择——`PrivTable` 级方法。把 fill 塞进 `KPriv::update_from_request` 是不可能的（单 slot 方法拿不到表）；退而求其次的方案是让字段拷贝方法返回掩码、调用者记得补一次 fill——这种"靠调用者纪律"的契约在内核代码里是定时炸弹。最终形态是组合入口：`PrivTable::update_priv(rp, req)` 先做字段拷贝（`KPriv::apply_fields_from_request`，计数越界返回命名的 `PrivUpdateError`、不碰掩码——对齐 C 的提前返回），再对请求掩码跑 `fill_sendto_mask`。调用者想"只拷字段不修掩码"在结构上就做不到。

**守卫的宽度问题在 Rust 里不存在**。C 的 `id_to_nr(id) == NONE` 靠 `s_proc_nr` 哨兵值判断，Rust 直接用 `Option<ProcNr>::is_none()`；`IpcMask` 补了 `set_bit` / `unset_bit` / `has_bit`（对应 `set_sys_bit` / `unset_sys_bit` / `get_sys_bit`，kernel/const.h:24/26/20），越界索引安全降级——内核里不该存在的"授予"既不会被静默接受，也不会 panic。

**对照其他系统**。seL4 的能力模型里，撤销是层级传播的（CNode revoke 沿派生树回收），代价是能力存储的复杂性；Minix3 的掩码模型是**扁平能力表**，没有派生树可走，`fill_sendto_mask` 的全量重算就是它对"撤销完整性"的务实回答。Redox 没有按目标的发送位图——它的能力边界是 scheme 命名空间（进程只能访问被授予的 scheme），粒度更粗，也不需要对称性维护，因为"能否通信"由路径解析时的 scheme 归属决定。Linux 根本没有 IPC 发送掩码——DAC 挂在管道/socket 等对象上，发送权由对象打开权限间接决定。三者共同反衬出 Minix3 模型的处境：微内核里**内核是唯一 IPC 仲裁者**，逐目标的位图检查在 send 路径上廉价且完备，掩码模型才成为可能。

**boot 路径的差异（已知残留）**。C 的 boot 序列（main.c:244）对每个 schedulable boot 进程也走 `fill_sendto_mask`——但配对补全是增量的：进程 j 初始化时，之后才绑定的 slot 拿不到 j 的位，等后者初始化时通过回执位反向补全。Rust boot 走的是文档化的模板设计（§3 D3 `CapabilityTemplate`）：VM/RS 直接持有 `IpcMask::ALL`。对"绑定期内完成互相连接"的 boot 进程对，两种路径的终态一致（全部已绑定 slot 互通）；对运行期才绑定的动态服务 S，C 中老进程→S 的位由 S 自己 SET_SYS fill 的回执补全（S 绑定那一刻生效），Rust 中 `ALL` 已预先覆盖——注意"预授权"在 S 绑定前**不可观察**：slot 未绑定时 S 连 endpoint 都没有，IPC 层根本解析不出目标，所以这一差异是机制差异而非行为差异。真正可观察的残留只有一点：模板 `ALL` 含自位——boot 服务 SEND 自己的 endpoint 时，C 在掩码层拒绝（`ECALLDENIED`，proc.c:536-541 经 `may_send_to`），Rust 放行后进入阻塞路径。它只影响 boot 模板持有者（VM/RS），runtime 路径（SET_SYS/UPDATE_SYS/do_update）已完全对齐 C。是否把模板路径也改为"模板掩码 + 守卫 fill"属于 boot 能力语义决策，单独记录、不在 D-49 范围内处理。

### 4.7 SYS_PRIVCTL 子命令实现状态（FIX-25, Phase 5; Phase 6 完成 6 DEFERRED 项 2026-08-13）

`dispatch_privctl`（os/kernel/src/syscall.rs）实现了 `do_privctl`（C: `system/do_privctl.c:26-275`）的 11 个子命令中的全部 11 个。原本 Phase 5 标记为 DEFERRED 的 6 个子命令（SET_SYS/ADD_IO/ADD_MEM/ADD_IRQ/UPDATE_SYS/CLEAR_IPC_REFS）于 2026-08-13 全部落地，使用 `data_copy_vmcheck` 跨地址空间拷贝 + `PrivTable::update_priv`（D-49 前：`KPriv::update_from_request`）/ `KPriv::get_priv` / `clear_ipc_refs` 完成。

| 子命令 | C 位置 | Rust 实现 | 状态 |
|--------|--------|----------|------|
| `SYS_PRIV_ALLOW` (1) | do_privctl.c:56-64 | 检查 `RTS_NO_PRIV` + `s_proc_nr` → 清 `RTS_NO_PRIV` | ✅ 已实现 |
| `SYS_PRIV_DISALLOW` (2) | do_privctl.c:75-79 | 设置 `RTS_NO_PRIV` | ✅ 已实现 |
| `SYS_PRIV_SET_SYS` (3) | do_privctl.c:86-174 | `KPriv::get_priv` 动态分配 slot + 双向链接（`p.priv_id` 回链，2026-09-05 修复）+ `reset_pending_ipc` + `reset_resources` + 默认掩码 `fill_sendto_mask(ALL)` + 可选 `update_priv` | ✅ 已实现（Phase 6, 2026-08-13；掩码语义 D-49 2026-09-05） |
| `SYS_PRIV_SET_USER` (4) | do_privctl.c:176-185 | 链接 target 到 `USER_PRIV_ID` + 更新 `s_proc_nr` | ✅ 已实现 |
| `SYS_PRIV_ADD_IO` (5) | do_privctl.c:187-204 | `copy_struct_from_user` 读取 `io_range` + `KPriv::add_io` | ✅ 已实现（Phase 6, 2026-08-13） |
| `SYS_PRIV_ADD_MEM` (6) | do_privctl.c:206-216 | `copy_struct_from_user` 读取 `mem_range` + `KPriv::add_mem` | ✅ 已实现（Phase 6, 2026-08-13） |
| `SYS_PRIV_ADD_IRQ` (7) | do_privctl.c:218-230 | `copy_struct_from_user` 读取 `irq` + `KPriv::add_irq` | ✅ 已实现（Phase 6, 2026-08-13） |
| `SYS_PRIV_QUERY_MEM` (8) | do_privctl.c:232-251 | 检查 `phys_start/len` 落在 `s_mem_tab` 范围 | ✅ 已实现 |
| `SYS_PRIV_UPDATE_SYS` (9) | do_privctl.c:253-268 | `copy_struct_from_user` 读取 `PrivUpdateRequest` + `PrivTable::update_priv`（字段拷贝 + `fill_sendto_mask` 全表掩码维护） | ✅ 已实现（Phase 6, 2026-08-13；掩码语义 D-49 2026-09-05） |
| `SYS_PRIV_YIELD` (10) | do_privctl.c:66-73 | target 清 `RTS_NO_PRIV` + caller 设 `RTS_NO_PRIV` | ✅ 已实现 |
| `SYS_PRIV_CLEAR_IPC_REFS` (11) | do_privctl.c:81-84 | 调用 `clear_ipc_refs`（syscall.rs:893）清 `s_notify_pending` / `s_asyn_pending` + 唤醒 `P_BLOCKEDON == target_ep` 的进程 | ✅ 已实现（Phase 6, 2026-08-13） |

**Phase 6 设计要点（2026-08-13）**：

1. **跨地址空间拷贝**：新增 `copy_struct_from_user` 助手（syscall.rs:828-857），封装 `data_copy_vmcheck` 路径，将用户空间 `io_range` / `mem_range` / `irq` / `PrivUpdateRequest` 拷贝到内核栈缓冲。`VmSuspend` 结果由 `dispatch_privctl` 转译为 `KcallResult::Suspend`，与 C 的 `SUSPEND` 语义对齐。
2. **`KPriv::update_from_request`**（kpriv.rs）：对应 C `update_priv()` (do_privctl.c:280-368)，按 `CHECK_IRQ` / `CHECK_IO_PORT` / `CHECK_MEM` 标志位 gate 复制 IRQ/IO/MEM 表，超范围返回 `Err(())` → `EINVAL`。
   > **2026-09-05 D-49 更新**：该单 slot 方法已拆分——字段拷贝部分更名为 `KPriv::apply_fields_from_request`（私有，错误类型升级为命名的 `PrivUpdateError`），掩码部分上移为全表操作：`PrivTable::update_priv` 组合"字段拷贝 + `fill_sendto_mask`"，对称性/守卫语义见 §4.6.1。SET_SYS 默认路径同期发现 `p.priv_id` 未回链（C `get_priv` 的 `rc->p_priv = sp`，system.c:298），已补双向链接。
3. **`KPriv::get_priv`**（kpriv.rs:764-795）：对应 C `get_priv()` (system.c:274-302)，实现 `NULL_PRIV_ID` → 扫描 `[NR_BOOT_PROCS .. NR_SYS_PROCS)` 动态 slot + 静态 slot 校验 + `EBUSY` / `ENOSPC` / `EINVAL` 错误码。
4. **`clear_ipc_refs`**（syscall.rs:893-942）：对应 C `clear_ipc_refs()` (system.c:577-607)，跨所有 `NR_SYS_PROCS` slot 清除 target 的 `s_notify_pending` / `s_asyn_pending` 位 + 唤醒 `blocked_on == target_ep` 的进程（清 `RTS_SENDING | RTS_RECEIVING`）。**Design gap**：C 设置 `rp->p_reg.retreg = caller_ret` 让被唤醒进程看到 `EDEADSRCDST`；Rust 未建模 register save area，`_error_code` 参数仅保留 API 完整性（与正常 IPC 唤醒路径相同 gap）。Rust `senda` 不持久化 async table，故无需 `cancel_async` 循环，bit 清除等价。

**FIX-25 latent bug 修复**：`dispatch_privctl` 原使用 legacy `caller_has_sys_proc(caller)`，该函数内部构建空 `PrivTable::new()`，导致所有 `SYS_PRIVCTL` 调用都被错误拒绝（EPERM）。已改用 `caller_has_sys_proc_with_table(caller, priv_table)` 正确识别 `SYS_PROC` 权限。`dispatch_schedule`、`dispatch_vmctl`（2026-08-14 修复，VMCTL 权限检查同款 latent bug）存在相同 bug，均已同步修复（新增 `priv_table: &PrivTable` 参数）。

---

## Ch5: 测试

> Rust 实现: `os/kernel/src/kpriv.rs:903-1200` (27 测试) + `os/kernel/src/capability.rs:283-361` (10 测试)

### kpriv.rs 测试

| 测试函数 | 验证行为 | 对应 C 符号 |
|---------|---------|------------|
| `test_kpriv_new` | KPriv::new 初始化 | `get_priv` |
| `test_kpriv_is_sys_proc` | SYS_PROC 标志判定 | `s_flags & SYS_PROC` |
| `test_kpriv_flag_predicates` | PREEMPTIBLE/BILLABLE 谓词 | `s_flags` 位 |
| `test_priv_flag_set_idl` | IDL_F = SYS_PROC\|BILLABLE | `IDL_F` priv.h:36 |
| `test_priv_flag_set_usr_no_sys_proc` | USR_F 无 SYS_PROC | `USR_F` priv.h:49 |
| `test_priv_flag_set_vm` | VM_F = SYS_PROC\|VM_SYS_PROC | `VM_F` priv.h:48 |
| `test_priv_table_new` | 表初始化 + 边界 | `priv[NR_SYS_PROCS]` |
| `test_priv_table_const_init_sets_per_slot_s_id` | 每 slot s_id 正确 | `ppriv_addr` |
| `test_priv_table_assign_static` | 静态分配 + EBUSY | `get_priv` system.c:294 |
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

### capability.rs 测试

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

### syscall.rs `dispatch_privctl` 测试（Phase 6, 2026-08-13）

> Rust 实现: os/kernel/src/syscall.rs:2748-2995 — 9 个测试覆盖 5 个原有子命令 + 4 个 Phase 6 新落地子命令的边界路径。

| 测试函数 | 验证行为 | 对应 C 符号 |
|---------|---------|------------|
| `test_dispatch_privctl_rejects_non_sys_proc_caller` | 非 SYS_PROC caller → EPERM | `do_privctl.c:47` caller check |
| `test_dispatch_privctl_unknown_request_returns_einval` | 未知子命令 → EINVAL | `default: result = EINVAL` |
| `test_dispatch_privctl_disallow_sets_no_priv` | DISALLOW 设 RTS_NO_PRIV | `do_privctl.c:75-79` |
| `test_dispatch_privctl_disallow_already_set_returns_eperm` | target 已无 priv → EPERM | `do_privctl.c:77` `!RTS_NO_PRIV` |
| `test_dispatch_privctl_query_mem_returns_eperm_no_ranges` | target 无 s_mem_tab → EPERM | `do_privctl.c:232-251` |
| `test_dispatch_privctl_set_sys_without_no_priv_returns_eperm` | SET_SYS 但 target 非 RTS_NO_PRIV → EPERM | `do_privctl.c:88` |
| `test_dispatch_privctl_add_io_without_priv_id_returns_eperm` | ADD_IO 但 target priv_id=None → EPERM | Rust 附加检查（C ADD_IO 无 s_id check；仅 RTS_NO_PRIV gate @188-190） |
| `test_dispatch_privctl_update_sys_without_arg_ptr_returns_einval` | UPDATE_SYS 但 arg_ptr=0 → EINVAL | `do_privctl.c:253-258` NULL check |
| `test_dispatch_privctl_clear_ipc_refs_returns_ok` | CLEAR_IPC_REFS 对合法 target 返回 OK | `do_privctl.c:81-84` |

---

## 参见

- [11-scheduling-primitives.md](11-scheduling-primitives.md) — PREEMPTIBLE/BILLABLE 与调度
- [17-syscall-process.md](17-syscall-process.md) — fork 分配 priv、RTS_NO_PRIV
- [18-syscall-copy.md](18-syscall-copy.md) — grant 表用于 safecopy
- [20-syscall-device.md](20-syscall-device.md) — CHECK_IO_PORT/CHECK_IRQ
- [21-syscall-clock.md](21-syscall-clock.md) — SYS_PROC 权限位、s_alarm_timer
- [23-ipc-filter.md](23-ipc-filter.md) — s_ipc_to 与 IPC 过滤
