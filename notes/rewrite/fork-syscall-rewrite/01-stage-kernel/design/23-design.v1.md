# 23-ipc-filter Design v1

> 本文件是 23-ipc-filter.md 关联 Rust 实现的设计契约（Gate H.1-H.5 依据）。
> 基于 C 源码独立推导，非持久化 ground truth。

---

## Ch1: 设计决策（9 项 hypothesis-driven）

### D1: 位图操作 — 内联函数 vs 宏
- **选项 A**：宏（C 风格 `get_sys_bit(map,bit)`）
- **选项 B**：内联函数 `fn get_sys_bit(map: u64, id: u16) -> bool`
- **结论**：**B 内联函数**
- **理由**：C 用宏是因为无类型系统约束；Rust 用内联函数获得类型安全（`u64` vs `u16` 区分 map 和 index），编译期仍内联。宏版本在 Rust 中会失去类型检查且需 `unsafe` transmute。
- **C 对齐**：`const.h:19-26` `get_sys_bit/set_sys_bit/unset_sys_bit` 宏语义保持

### D2: 过滤函数位置 — 独立函数 vs 内联
- **选项 A**：内联到调用点（`mini_send` / `kernel_call_dispatch` 内直接展开）
- **选项 B**：独立函数 `fn ipc_filter_check / kcall_filter_check`
- **结论**：**B 独立函数**
- **理由**：单一职责 + 可测试性。C `may_send_to` 是宏自动内联，但 Rust 语义下独立函数 + `#[inline]` 仍可内联且可独立单元测试。
- **C 对齐**：`priv.h:86 may_send_to` 宏语义保持

### D3: s_k_call_mask 类型 — u64 vs [u32; 2]
- **选项 A**：`[u32; SYS_CALL_MASK_SIZE]`（C 直译）
- **选项 B**：`u64` 单字段
- **结论**：**B `u64`**
- **理由**：Minix3 `NR_SYS_CALLS=58`（const.h + callnr.h），1 个 u64 足够（64 位）。对齐 22-privilege IpcMask newtype 设计。C 用 `[u32; 2]` 是因为 `bitchunk_t` 为 32 位，64 位机器上仍可用 u64 表达。
- **C 对齐**：`priv.h:38 s_k_call_mask[SYS_CALL_MASK_SIZE]` → u64（语义等价）
- **实现妥协**：`kcall_filter_check` 内仍按 `[u32; 2]` 读字段（`caller_priv.ipc.s_k_call_mask[0/1]`），未来可统一到 IpcMask newtype

### D4: 过滤失败返回 — EPERM vs panic
- **选项 A**：panic（违反不变量）
- **选项 B**：返回 `false`，由调用方返回 EPERM
- **结论**：**B 返回 false + 调用方 EPERM**
- **理由**：过滤失败是正常路径（恶意/越权调用），非不变量违反。C `system.c:111-114` 返回 `ECALLDENIED` 而非 panic。
- **C 对齐**：`system.c:111-114` `ECALLDENIED` 语义

### D5: 过滤池空闲槽 — Option vs type==IPCF_NONE
- **选项 A**：`type: IpcFilterType` 字段 + `IPCF_NONE` 表示空闲（C 直译）
- **选项 B**：`Option<IpcFilterSlot>`，`None` = 空闲
- **结论**：**B Option**
- **理由**：Rust "illegal states unrepresentable" 原则。C `IPCF_NONE` 是哨兵值（模式 17），用 `type` 字段同时表达"是否分配"和"分配后的类型"是状态混用。Option 强制调用方处理 None 分支。
- **C 对齐**：`ipc_filter.h:13 IPCF_NONE` / `IPCF_POOL_IS_FREE_SLOT` 语义保持

### D6: 过滤链 next — Option<usize> vs 裸指针
- **选项 A**：`*mut ipc_filter_s` 裸指针（C 直译）
- **选项 B**：`Option<usize>` 池内索引
- **结论**：**B Option<usize>**
- **理由**：避免 `unsafe` + 索引边界检查。C 用裸指针是因为无所有权概念，Rust 用池内索引可避免悬垂指针（释放后 `next` 仍指向已释放槽位的 bug 在类型层面不可能）。
- **C 对齐**：`ipc_filter.h:47 struct ipc_filter_s *next` → `Option<usize>`（语义等价）

### D7: IPCF_MATCH_M_SOURCE/M_TYPE — bitflags vs 裸 u32
- **选项 A**：`flags: u32` 裸整数（C 直译）
- **选项 B**：`bitflags! struct IpcFilterElFlags: u32`
- **结论**：**B bitflags**
- **理由**：类型安全 + 可组合（`MATCH_M_SOURCE | MATCH_M_TYPE`）。C 裸 int 用 `&` 位运算，Rust bitflags 提供编译期检查 + `contains`/`insert` 等 API。
- **C 对齐**：`include/minix/ipc_filter.h:18-19 IPCF_MATCH_M_SOURCE/M_TYPE` 语义保持

### D8: filter_type — enum vs int
- **选项 A**：`type: int`（C 直译，0/1/2 表示 NONE/BLACKLIST/WHITELIST）
- **选项 B**：`enum IpcFilterType { Blacklist, Whitelist }`（NONE 由 Option 表达，见 D5）
- **结论**：**B enum**
- **理由**：穷尽匹配 + 编译器检查。C `IPCF_NONE=0` 用 int 表示，但 Rust 中 `Option<IpcFilterSlot>` 已表达 NONE，enum 只需 Blacklist/Whitelist 两个变体。
- **C 对齐**：`ipc_filter.h:13-15 IPCF_NONE/BLACKLIST/WHITELIST` 语义保持（NONE 迁移到 Option）

### D9: IPC_STATUS 机制 — DEFERRED
- **选项 A**：实现 `IPC_STATUS_ADD/ADD_CALL/ADD_FLAGS` 宏语义
- **选项 B**：DEFERRED，标 TODO
- **结论**：**B DEFERRED**
- **理由**：IPC_STATUS 在 RECEIVE 时设置状态码，当前 Rust RECEIVE 路径未完整实现（12-ipc-core P0-12-2），实现 IPC_STATUS 无消费方。诚实标注 DEFERRED 优于假装实现。
- **C 对齐**：`ipc.h:40-48 IPC_STATUS_*` 语义未实现，标 DEFERRED + 依赖（12-ipc-core RECEIVE）

---

## Ch2: Minix3 对齐（行为契约表，5 函数 × 8 字段）

| 函数 | C 行为 | Rust 行为 | 差异类型 | 严重度 | C 证据 | Rust 证据 | 备注 |
|------|--------|----------|---------|--------|--------|----------|------|
| `may_send_to` | `get_sys_bit(priv(rp)->s_ipc_to, nr_to_id(nr))` 查 s_ipc_to 位图 | `caller_priv.may_send_to(target_sys_id)` 委派 IpcMask | 一致 | — | priv.h:86 | kpriv.rs may_send_to | — |
| `may_asynsend_to` | `may_send_to(rp,nr) \|\| rp->p_nr == nr` 允许 self-send | 未实现（当前用 may_send_to 替代） | 覆盖缺口 | P1 | priv.h:87 | — | 异步 IPC 允许 self-send |
| `GET_BIT(s_k_call_mask, call_nr)` | 位图查 call_nr 是否允许 | `kcall_filter_check(caller_priv, call_nr)` u64 位测试 | 一致 | — | system.c:111 | ipc_filter.rs:52-60 | — |
| `allow_ipc_filtered_msg` | 遍历 s_ipcf 过滤链，按 m_source/m_type 匹配，blacklist 默认 allow | 未实现 | 覆盖缺口 | P1 | system.c:803-874 | — | 细粒度过滤 |
| `IPCF_POOL_ALLOCATE_SLOT` | 扫描池找 `type==IPCF_NONE` 槽位 | `IpcFilterPool::allocate` 找 `None` 槽位 | 一致（语义等价） | — | ipc_filter.h:59-70 | ipc_filter.rs:189-197 | D5 Option 替代哨兵 |

---

## Ch3: Rust 类型设计

### 3.1 核心类型

```rust
// 位图原语（内联函数，对齐 const.h:19-26）
pub fn set_sys_bit(map: &mut u64, id: u16);
pub fn unset_sys_bit(map: &mut u64, id: u16);
pub fn get_sys_bit(map: u64, id: u16) -> bool;

// 过滤函数（独立函数，对齐 priv.h:86 + system.c:111）
pub fn ipc_filter_check(caller_priv: &KPriv, target_sys_id: u16) -> bool;
pub fn kcall_filter_check(caller_priv: &KPriv, call_nr: u32) -> bool;

// 过滤池（对齐 ipc_filter.h）
pub(crate) struct IpcFilterPool {
    slots: [Option<IpcFilterSlot>; IPCF_POOL_SIZE],
}
pub(crate) struct IpcFilterSlot {
    pub filter_type: IpcFilterType,
    pub num_elements: usize,
    pub flags: i32,
    pub next: Option<usize>,  // D6: 池内索引替代裸指针
    pub elements: [IpcFilterElement; IPCF_MAX_ELEMENTS],
}
pub(crate) enum IpcFilterType { Blacklist, Whitelist }  // D8: NONE 由 Option 表达
```

### 3.2 DEFERRED 类型（未实现，标 TODO）

```rust
// TODO(P1): allow_ipc_filtered_msg — 细粒度过滤（system.c:803-874）
// 依赖：12-ipc-core RECEIVE 路径实现后才有消费方
// fn allow_ipc_filtered_msg(rp: &KProcess, src_e: Endpoint, msg: &Message) -> bool;

// TODO(P1): may_asynsend_to — 异步 IPC 不对称权限（priv.h:87）
// 当前 may_send_to 替代，asyncsend 路径未完整接入
// fn may_asynsend_to(caller_priv: &KPriv, target_nr: ProcNr) -> bool;

// TODO(P2): IPC_STATUS 机制（ipc.h:40-48）
// 依赖：12-ipc-core RECEIVE 路径实现
```

---

## Ch4: 限制与约束

| 约束 | 说明 | C 证据 |
|------|------|--------|
| 过滤池大小固定 | `IPCF_POOL_SIZE = 2 * NR_SYS_PROCS` | ipc_filter.h:53 |
| 元素数上限 | `IPCF_MAX_ELEMENTS = NR_SYS_PROCS * 2` | include/minix/ipc_filter.h:15 |
| call_nr 范围 | 0..64（u64 位测试） | NR_SYS_CALLS=58 |
| sys_id 范围 | 0..64（u64 位测试） | NR_SYS_PROCS ≤ 64 |
| 无 unsafe | 过滤池用 `Option` + 索引，无裸指针 | — |
| 无硬件抽象 | 过滤是纯软件机制，无 arch 差异 | — |

---

## Ch5: 差异矩阵（design ↔ code ↔ C）

| 项 | design | code | C | 一致? |
|----|--------|------|---|------|
| s_k_call_mask 类型 | u64（D3） | `[u32; 2]` 字段 + u64 读 | `bitchunk_t[2]` | ⚠️ code 仍用 [u32;2] |
| 过滤池空闲表达 | Option（D5） | Option（实际） | type==IPCF_NONE | ✅ design↔code |
| filter_type | enum Blacklist/Whitelist（D8） | enum Blacklist/Whitelist | int 0/1/2 | ✅ design↔code |
| next 字段 | Option<usize>（D6） | Option<usize> | `*mut struct` | ✅ design↔code |
| allow_ipc_filtered_msg | DEFERRED（D9 衍生） | 未实现 | system.c:803 | ✅ design↔code |
| may_asynsend_to | DEFERRED | 未实现 | priv.h:87 | ✅ design↔code |

**注**：s_k_call_mask 字段类型 `[u32; 2]` 是历史遗留（对齐 C 布局），kcall_filter_check 内部组合为 u64 读取。未来可统一到 IpcMask newtype（22-privilege 已有 IpcMask，但 s_k_call_mask 未迁移）。

---

## 附录 A: 关键事实 grep 证据

| 事实 | grep 命令 | 结果 |
|------|----------|------|
| may_send_to 在 priv.h | `rg "may_send_to" minix3/minix/kernel/priv.h` | priv.h:86 |
| may_asynsend_to 在 priv.h | `rg "may_asynsend_to" minix3/minix/kernel/priv.h` | priv.h:87 |
| s_ipcf 在 priv.h | `rg "s_ipcf" minix3/minix/kernel/priv.h` | priv.h:46 |
| allow_ipc_filtered_msg 在 system.c | `rg "allow_ipc_filtered_msg" minix3/minix/kernel/system.c` | system.c:803 |
| get_sys_bit 在 const.h | `rg "get_sys_bit" minix3/minix/kernel/const.h` | const.h:19 |
| IPCF_POOL_SIZE 在 ipc_filter.h | `rg "IPCF_POOL_SIZE" minix3/minix/kernel/ipc_filter.h` | ipc_filter.h:53 |
| GET_BIT s_k_call_mask 在 system.c | `rg "s_k_call_mask" minix3/minix/kernel/system.c` | system.c:111 |
| CANRECEIVE 在 ipc.h | `rg "CANRECEIVE" minix3/minix/kernel/ipc.h` | ipc.h:19 |
