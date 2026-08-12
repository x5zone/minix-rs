# 22-privilege Design（设计文档）

> **状态**: 完整设计（基于 22-outline.md 经 outline-review 批准）
> **创建**: 2026-08-01
> **作者**: Trae (GLM-5.2)
> **前置**: 11-scheduling-primitives.md, 17-syscall-process.md, 18-syscall-copy.md, 20-syscall-device.md, 21-syscall-clock.md
> **C 源码**: `minix3/minix/kernel/priv.h` (105 行), `minix3/minix/include/minix/priv.h` (105 行), `minix3/minix/kernel/system.c:272-540`
> **Rust 实现**: `os/kernel/src/kpriv.rs` (860 行), `os/kernel/src/capability.rs` (356 行)

---

## §1. 设计目标与约束

### 1.1 目标

重写 `22-privilege.md` 文档，使其：
1. **对齐 C ground truth**: `priv.h` (kernel + include) + `system.c:272-540` 的完整语义
2. **修复 doc-code 背离**: §4.2 扁平字段列表→6 子结构；§4.3 IoRange u16→u32；s_alarm_timer 类型；缺失 capability.rs
3. **避免 translate**: 用 Rust 类型系统重新表达 C 的裸 short（bitflags）、sentinel（Option）、分散设置（CapabilityTemplate）、裸位图（Newtype）
4. **删除 tmp 引用**: §补充 来源改为 C 源码直接引用

### 1.2 约束

- `#![no_std]`（除 `#[cfg(test)]`）
- PrivTable 用固定数组 `[KPriv; 64]` + const fn 初始化（无堆分配）
- 字段名保留 C `s_` 前缀以便追溯
- 代码注释引用 C 源码 `file:line`
- 无迭代叙事；无 tmp 文件引用
- Ch3 hypothesis-driven；Ch5 测试函数可 grep

### 1.3 Ground Truth 验证

| C 符号 | 位置 | Rust 归属 |
|--------|------|----------|
| `struct priv` | priv.h:21-66 | `KPriv` (6 子结构) ✅ kpriv.rs:307 |
| `s_flags` | priv.h:24 | `PrivFlagsBits` bitflags ✅ kpriv.rs:59 |
| `s_ipc_to` | priv.h:35 | `PrivIpc.s_ipc_to: u64` ✅ kpriv.rs:193 |
| `s_k_call_mask` | priv.h:38 | `PrivIpc.s_k_call_mask: [u32; 2]` ✅ kpriv.rs:194 |
| `s_trap_mask` | priv.h:34 | `PrivIpc.s_trap_mask: u16` ✅ kpriv.rs:192 |
| `may_send_to` | priv.h:86 | `KPriv::may_send_to()` ✅ kpriv.rs:365 |
| `get_priv` | system.c:272 | `PrivTable::assign_static()` ✅ kpriv.rs:441 |
| `static_priv_id` | priv.h:12 | `static_priv_id()` ✅ kpriv.rs:102 |
| `USER_PRIV_ID` | priv.h:18 | `USER_PRIV_ID` ✅ kpriv.rs:98 |
| `IDL_F..USR_F` | priv.h:36-49 | `priv_flag_set::*` ✅ kpriv.rs:77-93 |
| `s_alarm_timer` | priv.h:48 | `PrivRuntime.s_alarm_timer: Option<(TimerEntry, TimerId)>` ✅ kpriv.rs:279 |
| `s_io_tab` | priv.h:54 | `PrivIo.s_io_tab: [IoRange; 64]` ✅ kpriv.rs:218 |
| `s_irq_tab` | priv.h:60 | `PrivIo.s_irq_tab: [i32; 16]` ✅ kpriv.rs:220 |
| `s_mem_tab` | priv.h:57 | `PrivMem.s_mem_tab: [MemRange; 20]` ✅ kpriv.rs:245 |
| `s_ipcf` | priv.h:46 | `PrivMem.s_ipcf: Option<usize>` ⚠️ kpriv.rs:246 (D9 限制) |
| `s_stack_guard` | priv.h:49 | `PrivMem.s_stack_guard: Option<usize>` ⚠️ kpriv.rs:247 (D9 限制) |
| `s_grant_table` | priv.h:61 | `PrivRuntime.s_grant_table: usize` ✅ kpriv.rs:280 |
| (Rust 扩展) | — | `CapabilityTemplate` + `grant_capability()` ✅ capability.rs:128, kpriv.rs:500 |
| (Rust 扩展) | — | `ProcessCapability` bitflags ✅ capability.rs:60 |
| (Rust 扩展) | — | `IpcMask`/`KCallMask`/`TrapMask` Newtype ✅ capability.rs:211-275 |

---

## §2. 核心数据结构设计

### 2.1 PrivFlagsBits bitflags (D2 — 已实现)

```rust
// C: minix/include/minix/const.h:143-154
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PrivFlagsBits: u16 {
        const PREEMPTIBLE     = 0x002;  // const.h:143
        const BILLABLE        = 0x004;  // const.h:144
        const DYN_PRIV_ID     = 0x008;  // const.h:145
        const SYS_PROC        = 0x010;  // const.h:147
        const CHECK_IO_PORT   = 0x020;  // const.h:148
        const CHECK_IRQ       = 0x040;  // const.h:149
        const CHECK_MEM       = 0x080;  // const.h:150
        const ROOT_SYS_PROC   = 0x100;  // const.h:151
        const VM_SYS_PROC     = 0x200;  // const.h:152
        const LU_SYS_PROC     = 0x400;  // const.h:153
        const RST_SYS_PROC    = 0x800;  // const.h:154
    }
}
```

**anti-translate**: C `short s_flags` → Rust `PrivFlagsBits` bitflags。位组合是 first-class 值，`contains(SYS_PROC)` 类型安全。

### 2.2 priv_flag_set 预定义组合 (已实现)

```rust
// C: minix/include/minix/priv.h:36-49
pub mod priv_flag_set {
    use super::PrivFlagsBits as F;
    pub const IDL_F: F = F::from_bits_truncate(F::SYS_PROC.bits() | F::BILLABLE.bits());
    pub const TSK_F: F = F::from_bits_truncate(F::SYS_PROC.bits());
    pub const SRV_F: F = F::from_bits_truncate(F::SYS_PROC.bits() | F::PREEMPTIBLE.bits());
    pub const DSRV_F: F = F::from_bits_truncate(SRV_F.bits() | F::DYN_PRIV_ID.bits());
    pub const RSYS_F: F = F::from_bits_truncate(SRV_F.bits() | F::ROOT_SYS_PROC.bits());
    pub const VM_F: F = F::from_bits_truncate(F::SYS_PROC.bits() | F::VM_SYS_PROC.bits());
    pub const USR_F: F = F::from_bits_truncate(F::BILLABLE.bits() | F::PREEMPTIBLE.bits());
}
```

### 2.3 KPriv 6 子结构 (D1 — 已实现)

```rust
// 6 substructures by responsibility. Each is const fn new()-constructible
// so PrivTable can be `[KPriv; NR_SYS_PROCS]` const-init (no heap alloc).

pub(crate) struct PrivCapability {
    pub(crate) s_proc_nr: Option<ProcNr>,   // D7: Option vs sentinel NONE
    pub(crate) s_id: SysId,
    pub(crate) s_flags: PrivFlagsBits,       // D2: bitflags vs short
    pub(crate) s_init_flags: i32,
}

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

pub(crate) struct PrivIpc {
    pub(crate) s_trap_mask: u16,
    pub(crate) s_ipc_to: u64,
    pub(crate) s_k_call_mask: [u32; SYS_CALL_MASK_SIZE],
}

pub(crate) struct PrivIo {
    pub(crate) s_nr_io_range: i32,
    pub(crate) s_io_tab: [IoRange; NR_IO_RANGE],
    pub(crate) s_nr_irq: i32,
    pub(crate) s_irq_tab: [i32; NR_IRQ],
}

pub(crate) struct PrivMem {
    pub(crate) s_nr_mem_range: i32,
    pub(crate) s_mem_tab: [MemRange; NR_MEM_RANGE],
    pub(crate) s_ipcf: Option<usize>,        // D9: raw ptr (limitation)
    pub(crate) s_stack_guard: Option<usize>,  // D9: raw ptr (limitation)
    pub(crate) s_diag_sig: bool,
}

pub(crate) struct PrivRuntime {
    pub(crate) s_alarm_timer: Option<(TimerEntry, TimerId)>,  // D8: tuple vs sentinel
    pub(crate) s_grant_table: usize,
    pub(crate) s_grant_entries: i32,
    pub(crate) s_grant_endpoint: Endpoint,
    pub(crate) s_state_table: usize,
    pub(crate) s_state_entries: i32,
}

pub(crate) struct KPriv {
    pub(crate) capability: PrivCapability,
    pub(crate) signals: PrivSignals,
    pub(crate) ipc: PrivIpc,
    pub(crate) io: PrivIo,
    pub(crate) mem: PrivMem,
    pub(crate) runtime: PrivRuntime,
}
```

**anti-translate**: C 扁平 `struct priv` (25 字段) → Rust 6 子结构。按职责分组，每子结构独立 const fn new()。

### 2.4 PrivTable 固定数组 (D6 — 已实现)

```rust
pub const NR_SYS_PROCS: usize = 64;

pub struct PrivTable {
    privs: [KPriv; NR_SYS_PROCS],
}

impl PrivTable {
    pub const fn new() -> Self {
        let mut privs = [const { KPriv::new_zeroed(0) }; NR_SYS_PROCS];
        let mut i = 0;
        while i < NR_SYS_PROCS {
            privs[i].capability.s_id = i as SysId;
            i += 1;
        }
        Self { privs }
    }
}
```

**anti-translate**: C `EXTERN struct priv priv[NR_SYS_PROCS]` BSS → Rust `PrivTable` 固定数组 + const fn。无堆分配，boot 阶段可用。

### 2.5 CapabilityTemplate + grant_capability (D3 — 已实现)

```rust
// capability.rs
pub enum CapabilityTemplate {
    Idle,         // IDL_F: SYS_PROC|BILLABLE, no IPC/kcall
    KernelTask,   // TSK_F: SYS_PROC, no IPC/kcall
    Vm,           // VM_F: SYS_PROC|VM_SYS_PROC, ALL_M/ALL_C
    RootService,  // RSYS_F: SRV_F|ROOT_SYS_PROC, ALL_M/ALL_C
    Deferred,     // empty, NO_M/NO_C
}

impl CapabilityTemplate {
    pub fn capabilities(self) -> ProcessCapability { /* ... */ }
    pub const fn trap_mask(self) -> TrapMask { /* ... */ }
    pub const fn ipc_mask(self) -> IpcMask { /* ... */ }
    pub const fn kcall_mask(self) -> KCallMask { /* ... */ }
}

// kpriv.rs
impl PrivTable {
    pub fn grant_capability(
        &mut self,
        proc_nr: ProcNr,
        template: CapabilityTemplate,
    ) -> Result<PrivId, CapabilityError> {
        let priv_id = self.assign_static(proc_nr)
            .ok_or(CapabilityError::SlotOccupied)?;
        let template_caps = template.capabilities();
        // Map ProcessCapability → PrivFlagsBits (D4 dual system)
        let mut flags = PrivFlagsBits::empty();
        if template_caps.contains(ProcessCapability::SYS_PROC) { flags |= PrivFlagsBits::SYS_PROC; }
        // ... (kpriv.rs:514-532)
        if let Some(priv_) = self.get_mut(priv_id) {
            priv_.capability.s_flags = flags;
            priv_.ipc.s_trap_mask = template.trap_mask().bits() as u16;
            priv_.ipc.s_ipc_to = template.ipc_mask().bits();
            priv_.ipc.s_k_call_mask = [
                (template.kcall_mask().bits() & 0xFFFF_FFFF) as u32,
                (template.kcall_mask().bits() >> 32) as u32,
            ];
            priv_.signals.s_sig_mgr = Endpoint::from_generation_slot(0, proc_nr);
        }
        Ok(priv_id)
    }
}
```

**anti-translate**: C 分散设置（get_priv + 逐字段）→ Rust CapabilityTemplate 模板。Correct-by-construction，无法漏设掩码。

### 2.6 Newtype 掩码 (D5 — 已实现)

```rust
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

**anti-translate**: C 裸 `sys_map_t`/`bitchunk_t[]` → Rust Newtype。类型系统区分三种掩码，无法互换。

---

## §3. Minix3 对齐检查

| Minix3 概念 | design 中对应 | code 中对应 | 缺失? |
|------------|-------------|-----------|-------|
| struct priv 25 字段 | §2.3 KPriv 6 子结构 | kpriv.rs:307 | ✅ |
| s_flags 11 位 | §2.1 PrivFlagsBits | kpriv.rs:59 | ✅ |
| 预定义组合 8 个 | §2.2 priv_flag_set | kpriv.rs:77 | ✅ |
| may_send_to 宏 | §2.6 IpcMask::may_send_to | capability.rs:247 | ✅ |
| get_priv | §2.5 assign_static | kpriv.rs:441 | ✅ |
| static_priv_id | §2.5 | kpriv.rs:102 | ✅ |
| USER_PRIV_ID | §2.5 | kpriv.rs:98 | ✅ |
| s_alarm_timer | §2.3 PrivRuntime | kpriv.rs:279 | ✅ |
| s_io_tab/s_irq_tab/s_mem_tab | §2.3 PrivIo/PrivMem | kpriv.rs:218,220,245 | ✅ |
| s_ipcf | §2.3 PrivMem (Option<usize>) | kpriv.rs:246 | ⚠️ D9 限制 |
| s_grant_table | §2.3 PrivRuntime | kpriv.rs:280 | ✅ |
| (Rust 扩展) CapabilityTemplate | §2.5 | capability.rs:128 | ✅ 新增 |

**P0 缺失**: 0
**限制**: D9 (s_ipcf/s_stack_guard 用 usize) 标 P2，不阻塞。

---

## §4. 限制与已知问题

| ID | 严重度 | 问题 | 计划 |
|----|--------|------|------|
| D9 | P2 | s_ipcf/s_stack_guard 用 Option<usize> 存裸指针 | redesign 阶段引入 NonNull<T> |
| D10 | P2 | PrivId/SysId 是 type alias 非 newtype | 后续改进 |
| D4 | P2 | ProcessCapability vs PrivFlagsBits 双系统 | 当前保留对齐 C，长期评估合并 |

---

## §5. anti-translate 检查

| C 模式 | Rust 设计 | 决策 |
|--------|----------|------|
| `short s_flags` | `PrivFlagsBits` bitflags | D2 |
| 扁平 struct priv | 6 子结构 | D1 |
| `proc_nr_t s_proc_nr` + NONE sentinel | `Option<ProcNr>` | D7 |
| `minix_timer_t s_alarm_timer` + TMR_NEVER | `Option<(TimerEntry, TimerId)>` | D8 |
| 分散 get_priv + 逐字段设置 | `CapabilityTemplate` + `grant_capability` | D3 |
| 裸 `sys_map_t s_ipc_to` | `IpcMask(u64)` Newtype | D5 |
| 裸 `bitchunk_t[] s_k_call_mask` | `KCallMask(u64)` Newtype | D5 |
| `EXTERN priv[]` BSS | `PrivTable` 固定数组 + const fn | D6 |

**结论**: 8 处 anti-translate，无 C 模拟。✅

---

## §6. redox 对照

| Minix3 概念 | redox 对应 | 差异 |
|------------|-----------|------|
| s_flags 位图 | capability token + scheme | redox 细粒度，Minix3 固定位图 |
| s_ipc_to 目标位图 | scheme 命名空间 + 路径解析 | redox 动态命名，Minix3 固定 ID |
| priv[NR_SYS_PROCS] 固定表 | 动态 scheme 注册 | redox 灵活，Minix3 O(1) |
| CapabilityTemplate | redox 无等价（runtime 授权） | Rust 侧 boot 配置抽象 |

**判定**: Minix3 位图模型适合固定数量系统进程（≤64），redox scheme 适合动态环境。rewrite 阶段保持 Minix3 语义，CapabilityTemplate 是 Rust 侧的配置正确性抽象，不改变位图语义。

---

## §7. 测试覆盖

见 outline §5.1-§5.3（37 个已实现 + 4 个待补充）。

L1（C-Rust 对齐）: test_priv_flag_set_*, test_static_priv_id, test_user_priv_id
L2（trait 契约）: test_grant_capability_*, test_may_send_to
L3（集成）: test_priv_table_assign_static + configure_boot_priv 组合
