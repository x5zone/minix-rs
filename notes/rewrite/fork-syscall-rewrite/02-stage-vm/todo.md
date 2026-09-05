# 02-stage-vm Rust 实现架构级 Review TODO

> 来源：2026-08-16 架构级代码审查（关注整体/分层架构，非逐函数审查）。
> 范围：`os/servers/vm/src/` 全部 Rust 代码（与 02-stage-vm 文档对应的实现）。
> 方法：整体分层分析（全局状态 → 主循环/分发 → 各子系统 → 模块），结合 Redox 实现与 Rust/OS 社区最佳实践。
> 定位：本文档是新增的架构改进建议清单，**不同于** `draft/TODO.md`（旧 fork 主线 TODO 存档）。
> 状态（2026-08-17 更新）：**P0-1 / P1-1 / V9-P0-1 / V9-P1-1 / V9-P3-1 / V10-P0-1 / V10-P0-2 / V10-P1-1 / V10-P1-2 / V10-P1-3 / V10-P2-1 / V10-P2-2 / V10-P2-4 已修复**（见 §8 + §10 + §13 修复记录）；V10-P2-3 与 P1-2/P1-3/P1-4/P2-\*/V9-P2-\*/V9-P3-2 为架构演进建议（DEFERRED/可选，依赖 kernel IPC 落地或需专项规划），供后续阶段参考。
> 状态（2026-09-06 更新，V11 轮，见 §14）：V11 轮审查完成后即进入**逐条修复 campaign**（顺序表：`notes/rewrite/fork-syscall-rewrite/edge_todo.md` §0；修复记录：§15）。**重要更正**：P1-3 与 P1-2 的"依赖 kernel IPC 落地"依据已失效——内核侧 IPC 核心已实现（`os/kernel/src/ipc.rs`），VmContext 收敛与 transport 落地均已解除阻塞（见 §14.2 复核表与 V11-P1-1/V11-P1-2）。本轮无新增 P0；新发现 3 项 P1、8 项 P2、1 项 P3，缺口登记 G-V11-1..6。已修复：**V11-P2-6**（见 §15 Fix #19）。

---

## 0. 审查结论速览

| 级别 | 条目 | 一句话 |
|------|------|--------|
| P0 | P0-1 | `map_lazy` ↔ `get_slot` 语义矛盾（**✅ 已修复** 2026-08-16，见 §8） |
| P1 | P1-1 | 全局 bump allocator 永不回收（**✅ 已修复** 2026-08-16：free-list v2，见 §10 Fix #6） |
| P1 | P1-2 | 全局可变状态散落 4 处 static + 实例字段，缺单一 Context |
| P1 | P1-3 | IPC transport 生产路径 `unimplemented!()`，主循环不可运行 |
| P1 | P1-4 | `KERNEL_LAYOUT` 硬编码 mock，boot-info 接缝未闭环 |
| P2 | P2-1 | Dispatcher 巨型 match/enum + 手写 M1 编码，建议收敛到 codec trait |
| P2 | P2-2 | 每模块一套错误 enum + errno 映射，重复维护 |
| P2 | P2-3 | `parts_mut()` 4 元组 + deferred VFS callback 战斗借用检查 |
| P2 | P2-4 | 测试依赖全局 static 修改，环境污染严重 |
| P3 | P3-1 | clippy warnings 未清理（**✅ 机械项已清** 2026-08-16：105 条四类机械项归零，见 §10 Fix #10；剩余设计项 DEFERRED） |

验证命令（2026-08-17 实测）：
- `cargo check -p minix-vm`：通过，无 error（含 `--all-features` 与 `--no-default-features --features {buddy_alloc,segment_tree_alloc,bitmap_alloc,vm_acl_audit}` 全部组合）
- `cargo test -p minix-vm --lib`：**441 passed / 0 failed**（V10 全量修复后；V10 前 437 passed）
- `cargo test -p minix-vm --lib --no-default-features --features segment_tree_alloc`：**456 passed / 0 failed**（V10-P0-1 feature 矩阵修复后）
- `cargo clippy -p minix-vm --lib`：183 → 105 → **0 warnings**（V10-P2-1 死代码收敛 + 剩余设计项 DEAD/DEFERRED 标注后）

验证命令（2026-09-06 V11 轮复核，见 §14.0）：
- `cargo test -p minix-vm --lib`：**448 passed / 0 failed**（默认 feature；较 08-17 增加 7 个测试，来自 boot.rs reconcile、vmproc reset_rusage、swap_proc_slot 等增量）
- `cargo test -p minix-vm --lib --no-default-features --features segment_tree_alloc`：**463 passed / 0 failed**
- `cargo test -p minix-vm --lib --no-default-features --features buddy_alloc`：**448 passed / 0 failed**
- `cargo clippy -p minix-vm --lib`：**1 warning**（默认）/ **7 warnings**（`--all-features`）——V10 后增量代码引入回归，条目见 V11-P2-3
- `cargo check -p minix-vm --all-features`：通过，无 error

---

## 1. P0：真实 bug（必须修复）

### P0-1 `map_lazy` 写入的页无法被 `get_slot` 读取

**问题**：`map_lazy()`（`os/servers/vm/src/region/vir_region.rs:220`）写入 `PageSlot::new(PFN_NONE, ...)`，
而 `get_slot()`（vir_region.rs:227）与 `get_slot_mut()`（vir_region.rs:232）都带 `.filter(|s| s.is_mapped())`；
`is_mapped()` 的定义是 `pfn != PFN_NONE`（`region/page_state.rs:117`）。因此 lazy 占位 slot 永远无法被读回。

**证据**：`cargo test -p minix-vm --lib region::vir_region::tests::test_map_lazy` 在
`vir_region.rs:513` `Option::unwrap()` on None 失败。测试本身正确，暴露的是实现 bug，**不应删除测试**。

**影响**：所有"先 lazy 占位、后按需实化"的路径都会 miss 该页。需 grep 确认生产调用点
（匿名 lazy 映射/CoW 预占位）是否受影响。

**建议（已按此实施，2026-08-16，详见 §8）**：
1. ~~显式化 `PageSlot` 状态机：`Empty / Reserved(lazy) / Mapped(pfn)`~~ ✅ 已实施：`PageSlot` 三态枚举（page_state.rs:90）。
2. ~~按调用方语义修 `get_slot`~~ ✅ 已实施：`get_slot`/`get_slot_mut` 保持 mapped-only 过滤；新增 `get_slot_any`/`get_slot_mut_any`（含 Reserved，vir_region.rs:268/:274）。
3. ~~修复后保留/扩展测试~~ ✅ 已实施：`test_map_lazy` 覆盖 `map_lazy → get_slot_any → 实化(map_page) → get_slot → 摘除` 全链路（vir_region.rs:548）。

---

## 2. P1：架构级问题（建议尽快规划）

### P1-1 全局 bump allocator 永不回收（最突出短板）

**问题**：`VmAllocator::dealloc()` 是 no-op（`os/servers/vm/src/global.rs:574`），
`VmAllocator` 是 bump-only 分配器（global.rs:390-420 架构注释明确 "Dealloc is a no-op"）。
所有 `Box`/`Vec` 等 heap 对象释放后内存不归还，arena 耗尽后只增不减（`refill_arena()` 持续新映射）。

**影响**：长期运行的 VM 服务器内存只涨不跌。PageCache 淘汰、VFS 请求完成、进程 exit 释放的
heap 对象（region Vec、page_cache 条目、临时消息）全部泄漏到进程生命周期结束。

**建议**：
1. **首选**：换可回收分配器。no_std 下可自实现 seglist/free-list（参考 `linked_list_allocator` crate
   的思路：size-class 链表 + 大块按需分裂），或按大小分类的 slab（小对象频繁 alloc/free 场景）。
2. **次选**：arena 预留。对已知热数据结构（`VfsRequestQueue`、PageCache 临时分配、`Message` 中转）
   预先分配固定 arena，生命周期与 server 同长，避免进入全局 bump 路径。
3. **参考 Redox**：boot 早期用 bump（无 free 需求），rmm 成熟后切 buddy/bitmap 并显式记录
   `PageInfo` 每帧状态；物理页层（`phys_mem/` 的 bitmap/buddy/segment-tree）已经有回收，
   本次问题只在 **heap 对象层** —— 两层分配器的回收语义要在文档中明确区分。

### P1-2 全局可变状态散落，缺单一 Context

**问题**：状态分散在 4 类载体：
- `global.rs`：`BOOT_INFO`(:12)、`TOTAL_PAGES`(:17)、`VM_INSTANCE_COUNT`(:22)、`KERNEL_LAYOUT`(:37)
  四个 `AssumeSyncCell` static + `PAGE_ALLOC_PTR` AtomicPtr(:408) + `HEAP_ARENA` static(:414)
- `vmproc/table.rs:59`：`VM_PROC_TABLE` static（`AssumeSyncCell<VmProc>` 数组）
- `fdref.rs:69`：`FDREF_TABLE` static（`UnsafeCell`）
- `vm_server.rs`：`VmServer` 实例字段（page_alloc / frames / cache / vfs_queue）

**影响**：
1. 借用检查被迫 `parts_mut()` 返回 4 元组（vm_server.rs:825），新增字段即扩大元组，脆弱。
2. 测试需要 `reset_slot` / `mock_vm_base` / `MOCK_BASE_MUTEX` 串行化（见 P2-4），是 static 的直接后果。
3. 同一逻辑层既有 static 又有实例字段，新增全局数据时无法判断归属，架构会继续漂移。

**建议**：
1. 收敛为单一 `VmContext`/`ServerState`：`VmProcTable`、`FdRefTable`、`PageFrames`、`PageCache`、
   `VfsRequestQueue`、`PageAllocator` 全部收为 `VmServer` 字段（或单一 `&'static VmContext` 单例）。
2. static 只保留 boot 期常量（`BOOT_INFO`、`KERNEL_LAYOUT`）与分配器基础设施（`HEAP_ARENA`）。
3. 消除 `AssumeSyncCell` 滥用：单线程事件循环内用 `&mut` 传递或 `Rc<RefCell<...>>` 即可，
   `unsafe impl Sync` 只应出现在跨模块的静态边界。
4. 测试改为构造注入（`VmContext::new(测试参数)`），替代修改 static。

### P1-3 IPC transport 生产路径未落地

**问题**：`KernelIpcTransport::receive/send` 直接 panic
（`os/servers/vm/src/ipc/transport.rs:151` / `:165`，"wiring pending kernel IPC core"）；
`vm_server.rs` 主循环的收发失败分支也是 `panic!`（vm_server.rs:459/502）。

**影响**：主循环实际不可运行，一切依赖真实 IPC 的端到端行为停留在 DEFERRED 状态。
`IpcTransport` trait 抽象本身正确，缺的是 kernel 侧实现。

**建议**：
1. 在 `plan.md`/checklist 中明确 DEFERRED 依赖链：kernel IPC core → `minix_arch` `sys_ipc_*` 接缝
   → `KernelIpcTransport` 实现 → 主循环端到端验证。
2. 落地时把 `Message` 编解码收敛到 minix-types codec trait（`EncodeToM1`/`DecodeFromM1`），
   避免 vm_server.rs 手工布局（见 P2-1）。
3. 参考 Redox：scheme + SQE/CQE 消息队列（io_uring 风格）。若未来要 async 事件循环，
   `IpcTransport` 可预留非阻塞 send/recv + poll 形态；Minix3 传统 `_sendreceive` 是阻塞语义，
   当前单线程模型下队列化 VFS 请求已部分缓解（`vfs_queue.rs`）。

### P1-4 `KERNEL_LAYOUT` 硬编码 mock 接缝未闭环

**问题**：`VmServer::init_global_state()`（vm_server.rs:298 附近）写入 mock 值：
`kernel_text_vbase = 0xFFFF_FFFF_8000_0000`、`kernel_text_pbase = 16 MiB`、
`kernel_text_pages = 8`、`kernel_data_pages = 8`，代码注释自认 "TODO (boot-info integration)"。

**影响**：与 kernel 侧 boot protocol 的接缝未闭环；真实硬件上这些值必错，当前只是"机制就位"。

**建议**：
1. 定义 boot 输入结构（`boot.rs` 的 `BootParams` 已有雏形），从 multiboot2/stivale2 或 linker symbols
   解析 kernel text 物理基址/尺寸与 direct map 大小。
2. 与 kernel 侧 boot-shim 约定同一输入格式，两侧不要各自硬编码。
3. 过渡期把 sentinel 值收敛为命名常量并加 `debug_assert` 一致性校验，防止再次散落。

---

## 3. P2：结构性改进（正确性 gate 通过后规划）

### P2-1 Dispatcher 巨型 match/enum + 手写 M1 编码

**现状**：`MessageDispatcher::dispatch_by_number`（`os/servers/vm/src/ipc/dispatcher.rs:1021`）是
巨型静态 match；`VmReplyForIpc`（vm_server.rs:546）+ `encode_reply_data`（vm_server.rs:1150，
约 130 行手写 M1 字段布局）把回复编码集中在主服务器文件。

**建议**：
1. 把 `VmReply` 编码收敛到 minix-types codec trait（`EncodeToM1`/`DecodeFromM1` 已存在），
   消除 vm_server.rs 与消息布局耦合的手工 match。
2. 或按 reply 类型拆到各服务模块（fork/mmap/brk... 自带 encode），dispatcher 只做路由。
3. enum 变体直接携带数据 + 单一 `to_message()` 实现，替代散装 match。

### P2-2 错误处理模式重复

**现状**：每个服务模块自带错误 enum：`BrkError`(brk.rs:47)、`VmExitError`(exit.rs:32)、
`MmapError`(mmap.rs:176)、`MunmapError`(munmap.rs:75)、`RsError`(rs.rs:63)、`QueryError`(query.rs:55)、
`VmProcctlError`(exit.rs:321) —— 每个都实现 `From<EndpointError>` + 各自 errno 映射，
测试侧也各有一份 `test_*_error_to_errno` 重复验证。

**建议**：
1. 分层：底层 `Errno`（minix-types）→ 服务错误（携带上下文 + 底层原因）→ dispatcher 统一转 M1 错误码。
2. errno 映射集中一处（每个错误 enum 只需声明 `Errno` 关联值，派生宏生成 `From` 与 `Into<Errno>`），
   消除各模块各自维护映射表。
3. no_std 下可用手写 derive 宏（thiserror 风格），避免依赖 proc-macro crate 时再评估。

### P2-3 `parts_mut()` 4 元组 + deferred VFS callback

**现状**：`parts_mut()`（vm_server.rs:825）返回 `(page_alloc, frames, cache, vfs_queue)` 4 元组；
mmap 文件路径通过 deferred VFS 请求（`mmap_file` → `vfs_queue`）跨字段协作，借用检查被迫散装。

**建议**：
1. VFS 请求完成改为事件入队：结果回到主循环再处理，避免跨字段借用的持久性约束。
2. `VmServer` 提供组合方法（如 `with_page_state(|frames, cache| ...)`）封装借用子集，
   替代调用方手工解构 4 元组。

### P2-4 测试环境污染

**现状**：测试通过修改全局 static 实现隔离：`reset_vm_self_pt_for_test`、
`unregister_page_alloc`、`reset_slot`、`MOCK_BASE_MUTEX` 串行化所有相关测试；
`extend_to_static_lifetime`（transmute 延长生命周期）模式脆弱。

**建议**：随 P1-2 状态收敛一并解决：测试用构造注入替代 static 修改；
静态重置函数收敛为单一 `test_teardown()`；移除生命周期 transmute。
若保留 static 形态，至少用 `#[serial]` 或统一锁显式声明串行化边界。

---

## 4. P3：代码卫生

### P3-1 clippy 105 warnings

**现状**（`cargo clippy -p minix-vm`）：
- `if let` 可折叠：20
- 手写 `is_multiple_of()`：18
- 手写 `div_ceil`：10
- `impl` 可 `derive`：8
- 未用方法：6
- 未用变量：4
- 大 enum 变体尺寸差异：3
- 函数参数过多（9 / 7）：2

**建议**：先清机械项（derive / div_ceil / is_multiple_of / if-let 折叠），
再处理设计项：大 enum 变体（与 P2-1 相关）、参数过多（提示可引入参数 struct，如 mmap 已部分做了）。

---

## 5. 对照 Redox 的架构参考

以下为 Redox 内存子系统与 IPC 实现中值得借鉴的模式（已联网核实）：

1. **帧状态集中追踪**：Redox 用 `PageInfo` 数组集中记录每物理帧的 refcount + 状态。
   minix-rs 的 `PageFrames` 类似，但 refcount 增删散落在各模块手工维护
   （`region` 的 `take_slot`/`release_slot`、`memtype` 的 `needs_cow`/`is_page_writable`、
   `page_cache` 的 IN_CACHE 交互、`sanity.rs` 全量校验兜底）。
   **建议**：把帧状态转移收敛为 `PageFrames` 上的方法族（reserve/release/share/unshare），
   模块只表达意图，refcount 一致性由单一实现保证，sanity.rs 转为 `debug_assertions` 门控的验证层。
2. **分配器分层**：Redox boot 期 bump → 运行期 buddy（rmm），且物理页层与虚拟对象层分离。
   minix-rs 物理页层已分层良好（`PhysAlloc` enum 静态分派 bitmap/buddy/segment-tree），
   heap 对象层（P1-1）是缺口。
3. **零拷贝 buffer 捕获**：Redox IPC 用 `CaptureGuard` 三部分捕获（head/middle/tail）支持
   零拷贝 pinning + fd 传递。minix-types 当前是 M1/M2 固定槽位编码，IPC 落地后（P1-3）可评估
   大缓冲区零拷贝路径。
4. **统一 scheme IPC**：Redox scheme 统一文件式 I/O + 消息队列。minix-rs 保持 Minix3 传统
   `_sendreceive` 语义是正确的（外部行为契约），但 `IpcTransport` 抽象已为未来 async 化留了缝
   （见 P1-3 建议 3）。
5. **单线程 vs SMP 边界**：Redox kernel 内并发用锁；minix-rs VM 是用户态服务器，单线程事件循环 +
   `Rc/RefCell` 模型正确。当前风险点不是并发，而是"单线程模型 + 全局 static"的组合导致的状态
   不可控（P1-2）—— 建议借 Redox 的"内核/用户态边界清晰"思路，把 static 边界收敛到 boot 期。

---

## 6. 模块级观察（次要，供后续阶段参考）

- `region/region_map.rs`：`BTreeMap` 查找 O(log n) 对当前规模合理；Minix3 原始用 AVL/range 树，
  如未来 region 数量级上升再评估 interval tree，现阶段不必。
- `memtype.rs`（1739 行）：`MemType` trait 15 个回调方法。建议拆为小语义 trait
  （如 `writable/cow/fault/evict` 分组）或用 enum 分派，降低实现方负担与误实现风险。
- `page_cache.rs`：BTreeMap 双哈希 + 索引式 LRU 合理；eviction 与 refcount（IN_CACHE）交互
  已有 `sanity.rs` 校验，保持。
- `vfs_queue.rs`：串行激活模型合理；注意补请求超时/失败恢复路径的语义测试。
- `fdref.rs`：显式 refcount + 反向索引是好设计；随 P1-2 收敛到 Context 后去掉 static。
- `cow_exec_pf.rs` / `alloc_page.rs` / `heap_arena.rs` / `direct_map.rs`：逻辑闭环，
  heap_arena 与 P1-1 的 arena 预留思路一致，可扩展。
- `boot.rs`：`BootParams` 雏形好，与 P1-4 的 boot-info 接缝合并推进。

---

## 7. 建议的推进顺序

1. **P0-1**：修复 `map_lazy`/`get_slot` 语义（先 grep 生产调用点确认影响面）。
2. **P1-2 状态收敛**：单一 `VmContext` 是其余多项（P2-3/P2-4）的前置，优先设计。
3. **P1-1 heap 回收**：随状态收敛一起改分配器接线，可独立做。
4. **P1-3/P1-4**：依赖 kernel 侧落地，明确 DEFERRED 状态与依赖链。
5. **P2-* / P3-1**：正确性 gate 通过后逐步清理。

---

## 8. P0-1 修复记录（2026-08-16）

> 遵循 fix-guard：每处修复前读目标行 ±5 行 + grep 确认现状；修后 grep/测试验证。

### ✅ Fix #1: P0 — PageSlot 由 PFN_NONE 哨兵结构体改为三态枚举

- **File**: `os/servers/vm/src/region/page_state.rs`（:71-131 → :90-205）
- **Before**: `pub(crate) struct PageSlot { pub(crate) pfn: u32, ... }` + `pub(crate) const PFN_NONE: u32 = u32::MAX;` + `PageSlot::EMPTY` 哨兵
- **After**: `pub(crate) enum PageSlot { Empty, Reserved { offset, memtype }, Mapped { pfn, offset, memtype } }`；`PFN_NONE` 删除；`mapped()`/`reserved()` 构造器 + `pfn() -> Option<u32>`/`is_reserved()`/`is_empty()`/`set_memtype()` 访问器
- **Verified**: `cargo test -p minix-vm --lib region::page_state::tests::test_page_slot` → passed；`rg "PFN_NONE" os/servers/vm/src` → 0 hits

### ✅ Fix #2: P0 — `map_lazy` 写 `Reserved` 槽；`get_slot_any`/`get_slot_mut_any` 提供含 Reserved 查询

- **File**: `os/servers/vm/src/region/vir_region.rs`（:220-234 → :241-277）
- **Before**: `map_lazy` 写 `PageSlot::new(PFN_NONE, offset, def_memtype)`；`get_slot`/`get_slot_mut` 用 `is_mapped()` 过滤 → lazy 槽不可见
- **After**: `map_lazy` 写 `PageSlot::reserved(offset, def_memtype)`（[ARCH: A-13]，携带 def_memtype）；`get_slot`/`get_slot_mut` 保持 mapped-only；新增 `get_slot_any`/`get_slot_mut_any`（非 Empty 即返回）；lazy 族 `#[allow(dead_code)]` 标注（无生产调用方，测试覆盖）
- **Verified**: `rg "fn get_slot_any|fn get_slot_mut_any|fn map_lazy" os/servers/vm/src/region/vir_region.rs` → 3 matches

### ✅ Fix #3: P0 — 全部 `slot.pfn`/`slot.memtype` 字段访问改为 `pfn()`/`memtype()` 方法

- **Files**: `sanity.rs`、`fork.rs`、`exit.rs`、`query.rs`、`vmproc/vmproc_handle.rs`、`memtype.rs`、`cow_exec_pf.rs`、`ipc/dispatcher.rs`（生产路径全部位于 `is_mapped()` 守卫内，`if let Some(pfn) = slot.pfn()` 合并守卫 + 提取，行为不变）
- **Verified**: `cargo check -p minix-vm` → 无 error；`cargo test -p minix-vm --lib` → **434 passed / 0 failed**

### ✅ Fix #4: P0 — `test_map_lazy` 扩展为全链路（占位 → 查询 → 实化 → 摘除）

- **File**: `os/servers/vm/src/region/vir_region.rs`（:507-516 → :548-579）
- **Before**: `map_lazy` 后 `get_slot().unwrap()` → panic（`unwrap()` on None）
- **After**: 覆盖 `map_lazy → get_slot_any(Reserved 可见/get_slot 不可见) → map_page 实化 → get_slot(Mapped) → unmap_page(Empty)` 全链路
- **Verified**: `cargo test -p minix-vm --lib vir_region` → 11 passed / 0 failed

### ✅ Fix #5: P0 — 文档同步（13-region-mapping / 11-phys-pagestate / 14-region-lookup / plan.md）

- **Files**: `13-region-mapping.md`（§1.9/§3.2/§3.6/§4.2/§5.1/§5.3/§5.4）、`11-phys-pagestate.md`（§3.2/§4.2/§5.1/§5.4）、`14-region-lookup.md`（§5.4）、`plan.md`（§3.5）、`.design/13-design.v2.md`（新增快照）
- **Verified**: `rg "PFN 哨兵|test_map_lazy pre-existing" 13-region-mapping.md` → 0 hits（§5.3 改为 ✅ 已修复）

---

## 9. 第二轮架构级审查：分层全景（2026-08-16）

> 方法：从整体到分层（服务边界 → 主循环/分发 → 数据结构 → 测试/基础设施），结合 Redox
> kernel/rmm 实现与微内核服务器最佳实践。以下条目为第一轮（§0-§8）未覆盖，或为其给出
> 具体落地形态的新建议。编号 `V9-*` 与第一轮 `P0-1..P3-1` 独立。
> 已实测证据：主循环 panic 点 vm_server.rs:459/:471；`dispatch_pagefault` 丢弃点
> vm_server.rs:607；`find_boot_image/set_boot_image` 无任何生产调用者。

### V9-P0-1 IPC 边界对不可信输入 panic（微内核可靠性）

**问题**：主循环两个边界点直接 panic：

- `ipc_receive()` 失败 → `panic!("ipc_receive() failed")`（vm_server.rs:459）
- 未知 endpoint → `panic!("invalid caller {:?}", who_e)`（vm_server.rs:471）

VM 是系统唯一内存管理服务器，panic = 全系统内存管理停摆；且全部页面/refcount/region
状态不可恢复（不同于 Redox scheme 服务器可重启）。

**对照 C**：Minix3 `main.c` 对未知 endpoint 同样 panic（历史行为），Rust 是忠实镜像；
但"镜像 C"≠"架构最优"——用户态服务器应把 IPC 视为不可信输入。

**建议**：

1. 未知 endpoint / receive 失败改为"丢弃消息 + 审计计数"（复用 ACL 的 `vm_acl_audit`
   feature 通道），不 panic；外部可观察行为不变（该 caller 本就无法得到服务）。
2. 若坚持镜像 C 的 panic 语义（可论证：VM 状态不可重启），至少把 panic 边界集中到
   `dispatch_on_msg` 入口并文档化为显式策略，杜绝 panic 散落深层。
3. 原则：**先校验后变更**。来自 IPC 的 endpoint/地址/长度在触碰全局状态前必须完整校验。

**验证**：grep 全部 `panic!`/`expect`/`unwrap` 于 `vm_server.rs` + `ipc/`，逐点标注
"镜像 C"或"改为可恢复"。

### V9-P1-1 `dispatch_pagefault` 结果静默丢弃

**问题**：vm_server.rs:607 `let _ = self.dispatch_pagefault(msg);`。错误已算出
（`VmReply::Error`）却被丢弃，release 下完全不可观测；`debug_assert!(is_from_kernel(...))`
只在 debug 生效。

**影响**：内核发来的 pagefault 处理失败（region 不存在 / 分配失败 / CoW 失败）时 VM 无法
感知，进程可能反复 fault，问题被掩盖。

**建议**：至少加错误计数（实例字段，随 V9-P1-3 收敛），并提供与 `vm_acl_audit` 同款的
feature-gated 审计输出；syslog IPC 落地后接日志通道。

**验证**：增加"错误 pagefault 被计数"用例；grep `dispatch_pagefault` 确认无其他丢弃点。

### V9-P1-2 IpcTransport 抽象未闭环：无注入点 + 全局泄漏

**问题**：`transport()`（vm_server.rs:895-931）用模块级 `AtomicPtr` + `Box::into_raw`
构造 `&'static mut dyn IpcTransport`，主循环经自由函数 `ipc_receive()/ipc_send()` 访问。
`IpcTransport` trait 存在但 `VmServer` 不持有它，`run()` 无注入点；`into_raw` 后永不回收
（单一 long-lived 对象，泄漏可接受，但暴露了没有所有权模型）。

**影响**：trait 抽象形同虚设——测试无法在 host 上驱动主循环（P1-3 落地后这是必经之路）；
全局指针与 V9-P1-3 的状态收敛目标冲突。

**建议**：

1. `VmServer` 增加 `transport` 字段（`Rc<RefCell<dyn IpcTransport>>` 或直接持有
   `&'static mut`），由构造器注入；`run()` 改用 `self.transport`。
2. 删除 `IPC_TRANSPORT_PTR` 与自由函数包装，消除 `Box::into_raw` 泄漏路径。
3. 测试注入 mock transport（脚本化消息序列），host 上跑完整主循环集成测试。

**验证**：`rg "IPC_TRANSPORT_PTR|Box::into_raw|fn transport\("` 归零；`cargo test -p minix-vm` 通过。

### V9-P1-3 9 个 `handle_xxx` 同构包装 → 单一 `VmContext` 落地形态

**问题**：`handle_fork/brk/exit/mmap/map_phys/mapcache/setcache/forgetcache/clearcache`
（vm_server.rs:1290-1337）全是同构模板：`get_global() → page_frames.as_mut().expect →
MessageDispatcher::dispatch_xxx(table, &mut self.page_alloc, frames, ...)`。
`VmServer` 沦为薄胶水，`parts_mut()` 4 元组（P2-3）正是该症状。

**建议**（P1-2 的具体落地设计）：收敛为单一 `VmContext`：

```rust
struct VmContext {
    proc_table: VmProcTable,   // 收敛 VmProcTable::get_global() static
    page_alloc: VmPageAllocator,
    frames: PageFrames,
    cache: PageCache,
    vfs_queue: VfsRequestQueue,
    boot: BootParams,          // 收敛 KERNEL_LAYOUT / BOOT_INFO（V9-P3-1）
}
```

- `MessageDispatcher` 签名统一为 `dispatch_xxx(&mut VmContext, caller, req)`；
- `VmServer` 持 `ctx: VmContext`，删除 `parts_mut()` 与逐字段透传；
- `handle_xxx` 退化为薄转发或直接删除，由 dispatcher 收 `&mut VmContext`。

**验证**：`rg "parts_mut|page_frames.as_mut" os/servers/vm/src` 归零；`cargo test` 全绿。

### V9-P2-1 wire-format 三套并存 → per-call codec 注册表

**问题**：同一 `Message` 的解码知识散落三处：

1. M1 codec traits：`DecodeFromM1/EncodeToM1`（minix-types `ipc/vm.rs:778-803`）；
2. 专用 union 的 free `decode_message`（VmBrkIn/VmMunmapIn/VmCacheIn/VmPagefaultIn 等，
   minix-types `ipc/vm.rs:330-941`）；
3. dispatcher 手工字段访问（RS calls 直接 `m2.m2i1/m2i2/m2i3`，dispatcher.rs:1080-1098）。

**影响**：新增调用号时要判断"该用哪套"，M1/M2/专用 union 格式知识无法复用，正是
P2-1"巨型 match + 手写编码"的根因。

**建议**：建 per-call-number 解码注册表：

```rust
struct CallCodec {
    decode: fn(&Message, caller: Endpoint) -> Result<Decoded, VmError>,
    format: WireFormat, // M1 | M2 | Special
}
static CALL_CODECS: [CallCodec; N] = ...; // 以 c - VM_RQ_BASE 索引
```

decode 与业务 dispatch 解耦，测试可逐条验证"每个调用号的格式判定 + 解码正确"。

**验证**：`rg "m2i[123]" os/servers/vm/src` 归零（codec 表除外）；`cargo test` 全绿。

### V9-P2-2 `dispatch_by_number` 表驱动化

**问题**：dispatcher.rs:1035-1051 起 30+ 分支的 `_c if _c == VM_X as usize - vm_rq_base`，
每分支重复 `VM_RQ_BASE` 偏移运算，加新调用号易错。

**建议**：静态函数指针表 `[fn(&mut VmContext, ...) -> VmReply; VM_CALL_COUNT]`，
下标 = `c - VM_RQ_BASE`，越界走 ENOSYS。这是 P2-1"巨型 match"的落地形式，可与
V9-P2-1 合并实现。

**验证**：`rg "as usize - vm_rq_base" os/servers/vm/src` 归零。

### V9-P2-3 MemType trait → Provider enum（Redox 证据）

**问题**：`MemType` trait 15 方法每个都带 `&mut dyn PfnAllocator / &VmProcTable /
&mut PageCache` 大依赖注入（memtype.rs:17），实现方被迫耦合全局状态。§6 已提拆小
trait/enum，本条补 Redox 证据与具体形态。

**参考（Redox kernel `src/context/memory.rs`）**：`Provider` enum（`Allocated /
AllocatedShared / PhysBorrowed / External / Shared`）用枚举分派页故障与 CoW 语义：
页故障 = 找 Grant → 按 `Provider` 分派（demand paging / CoW / 拒绝）；每种 Provider 的
"来源 + 权限 + 生命周期"集中一处声明，而非散布在 trait 方法里。

**建议**：把 `MemType` 收敛为数据驱动 enum（`Anonymous / FileBacked / Device / Heap...`），
方法降为 `ev_pagefault/evict/writable` 等少量行为函数 + 属性字段（来源、CoW 策略、
回收策略）；trait 仅保留真正多态的接缝。

**验证**：对照 Redox 4.2 "Memory Grant Types and Providers" 文档，逐 MemType 变体核对
职责边界。

### V9-P2-4 测试基础设施 → `Paging` 软件模拟（替代 mock 全局）

**问题**：P2-4 已列测试全局污染；本条补更彻底的替代方向：Redox rmm 提供 `EmulateArch`
（`src/arch/mod.rs`，`cfg(feature="std")`，BTreeMap 模拟四级页表），host 上无硬件完整
测试页表逻辑。

**建议**：为 `os/arch` 的 `Paging` trait 提供软件模拟实现（`#[cfg(test)]`），页表读写落到
BTreeMap；`MOCK_BASE_MUTEX`（direct_map.rs）、`extend_to_static_lifetime`（vmproc/mod.rs
transmute）等 mock 全局随之消除，测试无需串行化。作为 P2-4 的演进路线，比"静态重置函数
收敛"更彻底。

**验证**：`rg "MOCK_BASE_MUTEX|extend_to_static_lifetime" os/servers/vm/src` 归零。

### V9-P3-1 BOOT_INFO 死代码清理

**问题**：`global.rs:12` `BOOT_INFO` + `find_boot_image`(:136) + `set_boot_image`(:148)，
grep 确认生产路径无任何调用者（boot.rs 的 `BootParams` 是实际载体）。

**建议**：随 V9-P1-3 的 `VmContext.boot` 收敛时删除（或先行删除，保持单一真相源）；
`KERNEL_LAYOUT`（P1-4）一并并入 `boot: BootParams`。

**验证**：`rg "find_boot_image|set_boot_image|BOOT_INFO" os/servers/vm/src --glob '!global.rs'`
为 0。

### V9-P3-2 大匿名映射懒分配 roadmap（Reserved → 逐页 fault → 稀疏表示）

**问题**：`VirRegion.physblocks: Vec<PageSlot>` 对整段映射一次性分配 Vec；lazy 族
（`map_lazy` 等）无生产调用方（P0-1 修复后标注 `#[allow(dead_code)]`）。Redox demand
paging 是逐页 fault 时分配（page_fault_handler → Grant → Provider 分派）。

**建议**（DEFERRED roadmap，现阶段正确性优先）：

1. 已具备：`PageSlot::Reserved`（P0-1 修复）；
2. 下一步：匿名大映射先填 Reserved 槽，fault 时逐页 `Reserved → Mapped` 实化；
3. 评估：超大区域（GB 级）用区间/游程表示代替逐槽 Vec，仅在实化时物化 PageSlot。

**验证**：新增"大匿名映射仅故障页分配物理页"的语义测试（先测 Reserved 占位内存占用）。

---

## 10. 第二轮修复记录（2026-08-16）

> 范围：P1-1（free-list）、V9-P0-1 / V9-P1-1 / V9-P3-1、P3-1 clippy 机械项。
> 原则：文档与代码同步修复；谨防 translate（对照 C 语义 + Redox/OS 最佳实践，非逐行翻译）；
> 用户态服务器 = 单线程事件循环，IPC 视为不可信输入。

### ✅ Fix #6: P1 — 全局 bump allocator 改为 free-list 可回收分配器（[ARCH: A-3 v2]）

- **File**: `os/servers/vm/src/global.rs`（`VmAllocator` :604-895；架构注释 :567）
- **Before**: bump-only（`dealloc` no-op，文档自认 "Dealloc is a no-op"），arena 耗尽后 `refill_arena()` 只增不减 → 长期运行内存只涨不跌
- **After**: free-list + bump 补充 + 尾块回收（参考 `linked_list_allocator` 思路）：
  - `free_head: AssumeSyncCell<*mut FreeBlock>`（:607）；`free_list_insert`（:630）地址有序 + 相邻合并
  - `try_alloc_from_free_list`（:672）首适配 + 分裂（对齐 padding 块也入表）
  - `alloc_bump`（:729）对齐 gap 入表；`free_tail`（:765）bump 尾块回收；`refill_arena`（:779）
  - `dealloc`（:858）由相同 `Layout` 重建 `FreeBlock` 入表；payload == 块起点，**alloc 期零写入**（MockPaging 下 HeapArena VA 不可解引用，链式 refill 测试可跑）
  - 超大 fail-fast 保留；测试辅助 `free_block_count()`（:877）
- **语义分层**（Redox boot-bump → rmm-buddy 对照）：物理页层（`phys_mem/` bitmap/buddy/segment-tree）早有回收；本次修复的是 **heap 对象层**，两层回收语义在 09-slab-allocator.md §1 明确区分
- **Verified**: `rg "Dealloc is a no-op" os/servers/vm/src` → 0；`cargo test -p minix-vm --lib` → **437 passed / 0 failed**（含 4 个新测试：reuses_freed_block / alignment_variants / split_and_coalesce / reuse_cycles_without_refill）；`cargo check -p minix-vm` 无 error
- **Docs**: 09-slab-allocator.md（§1/§1.8-1.10/§3.1/§4.2/§4.5/§5.1-5.4/§6）、10-vm-relocation.md（§1.9/§4.7）、01-vm-init-main.md（§3.7）、plan.md（A-3 v2）、.design/09-design.v2.md（新增快照，v1 保留）

### ✅ Fix #7: V9-P1-1 — pagefault 失败结果不再静默丢弃

- **File**: `os/servers/vm/src/vm_server.rs`（字段 :84、accessor :457-460、P3 分支计数 :667-676、测试 :1807-1830）
- **Before**: `let _ = self.dispatch_pagefault(msg);`（旧 :607）——`VmReply::Error` 被丢弃，release 下完全不可观测；`debug_assert!(is_from_kernel)` 只在 debug 生效
- **After**: `VmReply::Error` → `pagefault_errors` 饱和计数 + `cfg(any(test, feature="vm_acl_audit"))` 审计 eprintln!（含 err/endpoint/vaddr）；新增 `test_pagefault_errors_counted` 直接驱动 `dispatch_on_msg` P3 分支
- **Verified**: `cargo test -p minix-vm --lib vm_server::tests::test_pagefault_errors_counted` → passed；`rg "let _ = .*dispatch_pagefault" os/servers/vm/src` → 0
- **Docs**: 16-pagefault.md（§3.6#3/§4.1/§5.3/§5.4）、15-ipc-dispatch.md（§3.4 P3 行、§5.1/§5.3/§5.4）

### ✅ Fix #8: V9-P0-1 — IPC 边界对不可信输入不再 panic（[ARCH: A-14]）

- **File**: `os/servers/vm/src/vm_server.rs`（`run()` :491-566，边界分支 :501-532；字段 :97、accessor :462-465）
- **Before**: `ipc_receive()` 失败 → `panic!("ipc_receive() failed")`；未知 endpoint → `panic!("invalid caller {:?}")`——忠实镜像 C main.c:122-123/:131-132，但 VM 是系统唯一内存管理服务器，panic = 全系统内存管理停摆且状态不可恢复
- **After**: 两处边界改为**丢弃消息 + `dropped_messages` 饱和计数 + 审计 eprintln! + `continue`**；外部可观察行为不变（该 caller 本就无法得到服务）；`ipc_send` 失败仍 panic（回复丢失 = 调用者永久挂起，A-14 只覆盖输入边界）；原则：先校验后变更，IPC 在触碰全局状态前完整校验
- **Verified**: `rg "panic!\(\"ipc_receive|panic!\(\"invalid caller" os/servers/vm/src/vm_server.rs` → 0；`cargo test -p minix-vm --lib` → 437 passed
- **Docs**: 15-ipc-dispatch.md（§3.7 差异 #9、§4.1 代码块 + 边界差异段、行号全量刷新）

### ✅ Fix #9: V9-P3-1 — BOOT_INFO 死代码清理

- **File**: `os/servers/vm/src/global.rs`（:9/:138 注释标注 V9-P3-1）
- **Before**: `BOOT_INFO` static + `find_boot_image` + `set_boot_image` + 2 个 boot-image 测试；grep 确认生产路径无调用者（boot.rs `BootParams` 是实际载体）
- **After**: 全部删除，单一真相源；global.rs imports 收敛为 `Endpoint, KernelLayout, AssumeSyncCell`
- **Verified**: `rg "find_boot_image|set_boot_image|BOOT_INFO" os/servers/vm/src --glob '!global.rs'` → 0；测试计数 439 → 437
- **Docs**: 01-vm-init-main.md（§4.3 删除说明）

### ✅ Fix #10: P3-1 — clippy 机械项清理

- **Files**: `fork.rs` / `ipc/dispatcher.rs` / `ipc/transport.rs` / `vm_server.rs` / `query.rs` / `munmap.rs` / `memtype.rs` / `heap_arena.rs` / `phys_mem/*` / `region/*` / `page_cache.rs` / `sanity.rs` / `fdref.rs` / `acl.rs` / `region_map.rs` / `Cargo.toml`
- **机械项清零**（minix-vm 内）：collapsible if（20）、manual `is_multiple_of`（18）、manual `div_ceil`（10）、derivable impl（2，vm 内）+ 安全项：redundant `is_err`（3）、clone-on-Copy（3）、多余 cast（2）、`map_or(false,..)` → `is_some_and`（2）、drop-ref → `let _`（1）、match → if let（1）、identity op（2）、manual flatten（1）
- **附带修复**：`os/servers/vm/Cargo.toml` 补 `vm_acl_audit = []` feature 声明（原缺失——`#[cfg(feature="vm_acl_audit")]` 审计通道在非 test 构建中是死的，代码注释已承诺该 feature）；`fdref.rs` `mut_from_ref` deny error 按 vmproc/table.rs 既有约定加 `#[allow]`
- **注意**: clippy --fix 曾误删 test-only imports（fork.rs / cow_exec_pf.rs / vir_region.rs），已恢复为测试模块内 `#[cfg(test)]` 局部 import（不污染 no_std 生产构建）
- **Verified**: `cargo clippy -p minix-vm --lib` → 183 → **105 warnings**（剩余为设计项：never-used methods / 大 enum 变体 / 参数过多 / 未用常量等，DEFERRED，见 §3/§4）；`cargo test -p minix-vm --lib` → **437 passed / 0 failed**；`cargo check -p minix-vm` 无 error
- **Docs**: 无行为变化，clippy 清理不改变语义，无需文档同步；行号引用已在 15-ipc-dispatch.md / 16-pagefault.md 全量刷新

---

## 11. 第三轮架构级审查：分层全景（2026-08-16）

> 范围：`os/servers/vm/src/` 全部 Rust 代码（含测试），从整体到分层（全局状态 → 主循环/分发 → 子系统 → 模块 → 测试）审查；
> 方法：实测构建矩阵 + `cargo clippy` 全量归因 + 与 Redox rmm/kernel（Provider enum、PageInfo、io_uring RFC）对照。
> 基线：`cargo test -p minix-vm --lib` → **437 passed / 0 failed**；`cargo clippy -p minix-vm --lib` → **105 warnings**（与 §4 数量一致，本轮按模块归因）。
> 与第二轮（V9）不重叠；已修条目（P0-1/P1-1/V9-P0-1/V9-P1-1/V9-P3-1 + clippy 机械项）不再列入。
> **本轮最重要的两个新发现**：① 三物理分配器后端的 feature 组合根本无法构建（V10-P0-1）；② 测试 transport 的注入点选错了——`ipc_transport_for_build()` 从未被调用，主循环不可测且生产路径是忙循环（V10-P0-2）。

### ✅ V10-P0-1 三后端 feature 矩阵无法构建（build 级 P0，P0-design-deviation）——已修复 2026-08-17（见 §13 Fix #11）

**问题**：`phys_mem/mod.rs` 头注释与 05-physical-memory.md 宣称三个物理分配器后端（`bitmap_alloc` 默认 / `buddy_alloc` / `segment_tree_alloc`）"boot-time 经 Cargo feature 互换"，但实测**除默认 `bitmap_alloc` 外全部无法编译**：

- `cargo check -p minix-vm --lib --no-default-features --features buddy_alloc` → **3 errors**：
  - `vm_server.rs:222` `if total_pages > BUDDY_THRESHOLD_PAGES` — `total_pages`/`BUDDY_THRESHOLD_PAGES` 未导入（参数名是 `_total_pages`，cfg 块引用的是裸 `total_pages`）；
  - `vm_server.rs:270` `BuddyAllocator::init(...)` — `BuddyAllocator` 未导入。
- `cargo check -p minix-vm --lib --no-default-features --features segment_tree_alloc` → **17 errors**：
  - `segment_tree_alloc.rs:114/115/142/146` 等引用 `CLICK_SIZE`；`:93` 引用 `BumpBuf`；`:83` 引用 `MemStats`；还有 `METADATA_ALIGN_PADDING` —— 该文件顶部 import（`:26-28`）缺这四项（`buddy_alloc.rs:33`/`bitmap_alloc.rs:31` 都有 `use super::{BumpBuf, CLICK_SIZE, BootMemRegion, METADATA_ALIGN_PADDING}`）。
- `cargo check -p minix-vm --lib --no-default-features --features bitmap_alloc,vm_acl_audit` → **4 errors**：`vm_server.rs:508/528/674/705` 在 `#[cfg(any(test, feature="vm_acl_audit"))]` 下用 `eprintln!`，但非 test 构建是 `no_std`，宏不可用 —— 注释宣称的 "--features vm_acl_audit → eprintln! in dev builds"（`:694-700`）从未真正构建过。

**连带影响**：

1. `relocate()`（vm_server.rs:238-276）的 buddy 分支即使补上 import 也无法选到 SegmentTree：`choose_allocator_type`（`:219`）只可能返回 `Buddy`（feature 开）或 `Bitmap`，`PhysAllocType::SegmentTree` 永远无法构造（clippy：`variants Buddy and SegmentTree are never constructed`）。
2. `segment_tree_alloc` 在默认 CI 下 **0 测试**（`allocator_tests.rs` 的 segment-tree 用例全部 `#[cfg(feature="segment_tree_alloc")]`），buddy 的 27 个 parity 测试默认跑（模块恒编译），但 buddy 的运行时选择路径被 broken feature 挡住 —— "测试里最熟的分配器在生产永远选不上"。
3. `vm_acl_audit` 是 V9-P0-1/V9-P1-1 新增审计通道的唯一出口，broken 意味着 release 下审计失效（test 构建可用，掩盖了问题）。

**建议**：修复为最小改动（补 import + `_total_pages` 引用修正 + `eprintln!` 改条件导入日志宏），并在 CI 加 feature 矩阵：`cargo check -p minix-vm --all-features` + `--no-default-features --features {buddy_alloc,segment_tree_alloc,vm_acl_audit}` 逐一构建；同时决定 `choose_allocator_type` 是否应支持 SegmentTree（若支持，`relocate()` 需补 `PhysAlloc::SegmentTree` 构造分支）。

**验证**：上述 3 条 `cargo check` 命令从 errors 归零；`cargo test -p minix-vm --lib --features segment_tree_alloc` 能跑 segment-tree parity 测试。

### ✅ V10-P0-2 `transport()` 注入点选错：TestIpcTransport 从未接线，主循环不可测且生产为忙循环（P0-code-bug）——已修复 2026-08-17（见 §13 Fix #12）

**问题**：`vm_server.rs:966` `transport()` **无条件**构造 `KernelIpcTransport`（`#[cfg(test)]` 也一样）；`ipc/transport.rs:243/248` 的构建选择器 `ipc_transport_for_build()` 从未被调用（clippy：`struct TestIpcTransport is never constructed`、`function ipc_transport_for_build is never used`）。

**后果**：

1. **主循环不可测**：`transport.rs:219` 的 mock 声称 "so tests can drive the main loop"，但 `run()`（vm_server.rs:491，`-> !`）在 test 下走 `KernelIpcTransport::receive` → 恒 `Err(IpcError::Unimplemented)` → V9-P0-1 的 drop+continue 路径 → 无法注入消息、无法退出 —— 目前 `run()` 只有 `#[should_panic]` 的 init-assert 测试（vm_server.rs:1618），无任何端到端测试。
2. **生产忙循环风险**：`mark_initialized`（transport.rs:128）无任何生产调用点（仅测试用），`KernelIpcTransport.initialized` 恒 false → `receive` 恒 `Err(Unimplemented)` → 一旦 `run()` 启动即 100% CPU 忙等 + `dropped_messages` 饱和计数。C 的 `sef_receive_status` 是阻塞调用，V9-P0-1 把 panic 换成 continue 后，**"错误即重试"的语义缺了"阻塞"这半**。

**建议**：

1. `transport()` 内 `#[cfg(test)]` 分支改用 `ipc_transport_for_build()`（消灭死代码 + 打通主循环测试）；`VmServer::init()` 调用 `mark_initialized()`。
2. 接收失败路径加**连续失败退避/上限**：如连续 N 次 `Err` 后 `panic!("transport permanently broken")`（保持 V9-P0-1 的"不可信输入不 panic"，但区分"输入错误"与"transport 本身坏了"），避免无限忙循环。
3. 参考 Redox io_uring RFC（gitlab.redox-os.org/4lDO2/rfcs/text/0000-io_uring.md，SQE/CQE 双环、用户态不阻塞主循环）：把 `receive` 的"阻塞等消息"与"非阻塞轮询"语义显式建模进 `IpcTransport` trait，而不是靠 mock 的 `Err(Unimplemented)` 表达"暂无消息"。

**验证**：`rg "ipc_transport_for_build" os/servers/vm/src` 出现调用点；新增"TestIpcTransport 驱动 run() 一轮 dispatch→reply"的端到端测试；`rg "mark_initialized" os/servers/vm/src/vm_server.rs` 非零。

### ✅ V10-P1-1 接收状态语义未建模：`is_ipc_notify`/`is_from_kernel` 硬编码（P1）——已修复 2026-08-17（见 §13 Fix #13）

**问题**：`vm_server.rs:1005-1012` `is_ipc_notify(_)` 恒 `false`、`is_from_kernel(_)` 恒 `true`；`IpcStatus`（transport.rs:43-48）只有 `flags: u32` 裸字段，无方法。C 的 `is_ipc_notify(rcv_sts)` / `IPC_STATUS_FLAGS_TEST(rcv_sts, IPC_FLG_MSG_FROM_KERNEL)` 语义未落地。

**影响**：内核通知（notify）到达时不会走 C 的 `continue`，而是落入 caller 校验被当无效调用丢弃（行为可接受但分类错误）；`is_from_kernel` 目前只服务 pagefault 审计分支（vm_server.rs:663）的 debug 输出。接收状态语义是 V9-P1-2（transport 未闭环）的一个未覆盖切面。

**建议**：`IpcStatus` 提供 `is_notify()` / `is_from_kernel()` 方法（按 Minix3 `ipc.h` `IPC_STATUS_*` 位解析），由 `KernelIpcTransport::receive` 真实填充；kernel IPC 落地前保留 const 版但标注 `// DEAD until kernel IPC core`，并补一条"notify 消息在 `dispatch_on_msg` 之前被跳过"的单测（当前不可写，因为恒 false）。

**验证**：`rg "fn is_ipc_notify|fn is_from_kernel" os/servers/vm/src/vm_server.rs` 不再是常量返回；新增 notify 分类测试。

### ✅ V10-P1-2 SUSPEND/resume 机制整体不可达：live update 骨架无生产者（P1）——已修复 2026-08-17（pin 测试，见 §13 Fix #14）

**问题**：`rs.rs:285-340` `handle_rs_update` 在 steps 3-7 处恒 `Err(RsError::UpdateNotImplemented)`（注释自认 DEFERRED，依赖 kernel `sys_update`）；因此 `dispatcher.rs:861-873` 的 `Ok(RsUpdateResult::Ok) => VmReply::Ok` / `Ok(Suspend) => VmReply::Suspend` 不可达（clippy：`variants Ok and Suspend are never constructed`）；`vm_server.rs:563` `DispatchAction::Suspend => {}` 空语句；VFS transid 的 resume 路径（handle_vfs_transid）没有生产者。

**影响**：主循环的"悬挂-恢复"半边是纯骨架（~100 行含 `VmReplyForIpc` 静态排除、resume 队列、`encode_reply_data` 分支），无测试覆盖、无真实调用，且 `DispatchAction::Suspend` 的空 arm 是静默 no-op —— 将来 live update 落地时容易踩"忘记恢复"的坑。

**建议**：

1. 把整条 SUSPEND 链标注为 live-update scaffolding，指向单一 TODO（如 `live_update` feature gate），避免"看起来已实现"；
2. 加一条 pin 不变式测试：`dispatch_rs_update` 当前必须返回 `Error(NotImplemented)`（实现时翻转该测试）；
3. 设计层面（Redox 对照）：SUSPEND 的"回复延迟到 VFS reply"机制与 Redox io_uring 的 completion 队列同构，建议落地时把"挂起的回复"建模为显式 `PendingReply` 队列而非散落的主循环分支。

**验证**：`rg "RsUpdateResult::Suspend|DispatchAction::Suspend" os/servers/vm/src` 处新增测试引用；`cargo test -p minix-vm --lib rs` 覆盖 `dispatch_rs_update` 恒 NotImplemented。

### ✅ V10-P1-3 `MemType` trait：`Send + Sync` 由存储机制驱动 + 默认方法静默成功（P1）——已修复 2026-08-17（见 §13 Fix #15）

**问题**：

1. `memtype.rs:11` `pub(crate) trait MemType: Send + Sync` —— 单线程用户态服务器（lib.rs 明确 `!Send/!Sync` 可接受）却要求所有 impl `Send + Sync`。根因是 `&'static dyn MemType` 存入 `static`（memtype.rs:1173-1178 `MEM_TYPE_*`）需要 `Sync`：**存储机制泄漏到 API 契约**，任何想持有 `Rc<RefCell>` 的 MemType 实现被排除。
2. 15 个方法中 10+ 个有默认实现：`ev_new`→`Ok(())`、`ev_pagefault`→`Ok(Handled)`、`ev_reference`→`Ok`、`writable`→`false`……C 的 NULL 回调 = "该类型无此钩子"（memtype.h），Rust 默认实现 = **"忘记实现也静默成功"**。`ev_pagefault` 默认 `Ok(Handled)` 意味着漏实现的 MemType 会把缺页"处理"成无事发生（不做映射、不报错）。

**建议**（与 V9-P2-3 Provider enum 方向一致，本条给具体约束修正）：

1. 若保留 trait：把 `Send + Sync` 从 supertrait 移除，仅在 `MEM_TYPE_*` static 处局部 `unsafe impl Sync`（与 `AssumeSyncCell` 同一套论证，见 global.rs），恢复单线程模型一致性；
2. `ev_pagefault` 改为必实现（或 debug 构建断言 `name()` 在已知 MemType 集合内 + 未走默认路径），杜绝静默成功；
3. 对照 Redox Provider enum（`Allocated/AllocatedShared/PhysBorrowed/External`，deepwiki.com/redox-os/kernel/4.2-memory-grant-types-and-providers、4.5-page-fault-handling）：MemType 的"属性"（CoW 策略、回收策略、来源）应数据化，行为（pagefault 分派、evict）才留 trait/函数。

**验证**：新增"漏实现 `ev_pagefault` 的 MemType 在 debug 下断言失败"的测试；`rg "trait MemType" os/servers/vm/src/memtype.rs` 无 `Send + Sync`。

### ✅ V10-P2-1 死代码/死 API 盘点表（clippy 105 条的按模块归因）——已收敛 2026-08-17（见 §13 Fix #16）

**问题**：105 条 clippy 警告中约 60 条是 dead-code/dead-API 类，散落 30+ 文件，无统一 DEAD/DEFERRED 判定。逐项判定如下（供收敛时对照，避免"看着像实现了一半"误判为缺口）：

| 位置 | 死项 | 判定 |
|---|---|---|
| `region/region_map.rs:19` | `SearchType::Greater/LessEqual/GreaterEqual`（`Less` 仅 `LessEqual` 内部用；`find_less/find_greater/find_less_equal/find_greater_equal` 四个包装无生产调用） | **DEAD**：生产只用 `find/find_mut/find_by_end`；收敛为 Equal + 私有辅助 |
| `ipc/transport.rs:53` | `IpcError::WouldBlock/Kernel` | **DEFERRED**：kernel IPC 落地后才有语义 |
| `ipc/transport.rs:178/243` | `TestIpcTransport` / `ipc_transport_for_build` | **P0**：见 V10-P0-2 |
| `memtype.rs:163` | `MemTypeError::IoError/CopyFailed` | **DEAD/DEFERRED 待定**：VFS IO 错误实际走 `CowError`/`VfsQueueError` 传播 |
| `region/vir_region.rs:405` | `VmError::NoMemory/NotFound` | **DEAD**：上层用 `VmError::OutOfMemory` 等表达 |
| `region/vir_region.rs:49` | `VrParam::PbCache` | **DEAD in prod**：仅测试构造（memtype.rs 测试）；`dispatch_mapcache` 生成的 MEM_TYPE_CACHE 区域 param 实际是默认 `Direct{0}` —— 需对照 C `phys_region.pf` 确认 cached PFN 记录位置是否应改为 PbCache |
| `vfs_queue.rs:41` | `VfsRequestState::FdClose` | **DEAD**：mmap 用的是 `VfsRequestType::FdClose`（不同 enum） |
| `vfs_queue.rs:80` | `VfsQueueError::NoCallback` 等 | **DEAD**（构造点为零） |
| `query.rs:52` | `QueryError::InvalidQuery` | **DEAD** |
| `exit.rs:285` | `VmProcctlError::PermissionDenied` | **DEAD** |
| `rs.rs:87` | `RsUpdateResult::Ok/Suspend` | **DEFERRED**：见 V10-P1-2 |
| `phys_mem/mod.rs:222` | `PhysAllocType::Buddy/SegmentTree` 构造 | **P0**：见 V10-P0-1 |
| `phys_mem/buddy_alloc.rs`（9 项） | `ORDER_MASK/ORDER_INVALID/MAX_ORDER/FREE_LIST_SENTINEL/FLAG_ALLOCATED` 等 | **DEFERRED**：buddy 运行时选择路径被 broken feature 挡住；模块恒编译（27 测试默认跑） |
| `vmproc/table.rs:444` | `VmProcIter` | **DEAD**（可用 `table.iter()` 替代？无构造点） |
| `sanity.rs:54` | `verify_refcounts` | **DEFERRED**：C 的 SANITYCHECKS 是编译开关；Rust 无生产入口，建议加 `sanity_checks` feature + 主循环周期调用 |
| `fork.rs:429` / `cow_exec_pf.rs:326` | `cow_copy_page` / `cow_resolve_region` | **DEFERRED**：CoW 机制已实现但 fork/exec 生产路径未落地，仅测试驱动 |
| `pagetable/vm_self_map.rs:140` | `vm_self_query` | **DEFERRED**：仅测试（heap_arena）使用 |
| `ipc/dispatcher.rs:820` | `dispatch_exec_newmem` | **DEFERRED**：注释已明确两路由二选一，未接线 |
| `alloc_stats.rs:45/53` | `active_allocations` / `check_leak` | **DEFERRED**（泄漏检测入口未接） |
| `acl.rs:175/179` | `acl_clear` / `mask` | **DEFERRED**（`mask` 可随 V9-P2-3 Provider 化收敛） |
| `vmproc/vmproc_handle.rs:183/723` | `force_clear` / `ExitingProc` accessors | **DEFERRED**（swap_proc_slot 路径未落地） |
| `direct_map.rs`（5 项） | `VM_DIRECT_MAP_BASE`/`KERNEL_DIRECT_MAP_BASE`/`virt_to_phys`/`kernel_phys_to_virt`/`is_direct_map_virt` | **DEFERRED/DEAD**：生产只用 `vm_phys_to_virt`；见 V10-P2-2 |

**建议**：把上表并入 `cargo clippy` 收敛工作：DEAD 的直接删/收敛，DEFERRED 的在代码注释加 TODO 指针；目标是把 105 条降到"只有 DEFERRED + 明确标注"的集合（预计 <30 条）。

**验证**：`cargo clippy -p minix-vm --lib 2>&1 | grep -c "never"` 按表逐行核对。

### ✅ V10-P2-2 DirectMap 布局常量分裂 + 半死 API（P2）——已修复 2026-08-17（见 §13 Fix #17）

**问题**：

1. `direct_map.rs:14` `VM_DIRECT_MAP_SIZE = 1 << 30` 硬编码在 VM 服务器，而 `DirectMapArch` trait（os/arch/src/arch/direct_map.rs）只暴露 base/heap 常量不暴露 size —— 布局演进（如 AArch64/RISC-V 改 direct map 窗口）时硬编码会静默漂移；
2. `vm_server.rs:187` `create_default_allocator` 用 `r.base < VM_DIRECT_MAP_SIZE` 当"direct map 内空闲区"的判据（C 语义：metadata 必须在 direct map 内可解引用）；
3. `is_direct_map_virt`（direct_map.rs:38）kernel 分支无上界：任何 `>= KERNEL_DIRECT_MAP_BASE` 的高半区地址都判为 direct map —— 当前三架构常量（0xFFFF_8000_0000_0000 / 0x0000_0000_8000_0000 等）互不相交，**无实际 bug**，但语义不精确且该函数生产未用；
4. 上述 5 个 const/fn 生产零调用（clippy 证实），`test_is_direct_map_virt` 只覆盖 3 个点。

**建议**：`DirectMapArch` 增加 `VM_DIRECT_MAP_SIZE`（与 `VM_HEAP_BASE = VM_DIRECT_MAP_BASE + VM_DIRECT_MAP_SIZE` 作为布局不变式，测试断言）；生产未用的 `virt_to_phys`/`is_direct_map_virt` 要么接上（如 heap_arena 断言自映射范围）要么标 DEAD；`is_direct_map_virt` 若保留需补上界与边界测试。

**验证**：`rg "VM_DIRECT_MAP_SIZE|is_direct_map_virt" os/servers/vm/src --glob '!direct_map.rs'` 与布局文档一致。

### ⏸ V10-P2-3 错误枚举 12+ 套盘点（P2-2 的量化补充）——**DEFERRED 2026-08-17**（见 §13 判定）

**问题**：P2-2 已列"错误处理模式重复"，本轮量化：`VmExitError`/`BrkError`/`MmapError`/`MunmapError`/`RsError`/`QueryError`/`MemTypeError`/`CacheError`/`VfsQueueError`/`EndpointError`/`CowError`/`HeapArenaError`/`VmForkError`/`VmProcctlError`/`PageTableError` 等 15 个 crate 内错误 enum，每套自带 `From<EndpointError>` + errno 映射 + 测试（如 query.rs:43-57 注释说明 `QueryError::ProcessNotFound` 在 getrusage 上下文映射 ESRCH、其余 EINVAL）。

**建议**：维护一张**errno 映射总表**（minix-types 的 `VmError` 为唯一对外出口），crate 内错误统一实现一个 `fn to_vm_error(&self) -> VmError` trait 或宏生成 From 样板；上下文相关映射（getrusage）保留为显式函数（query.rs 已做），但把"每个文件手写 match + Display + tests"收敛为共享样板。

**验证**：`rg "pub(crate) enum .*Error" os/servers/vm/src --glob '*.rs'` 清单收敛到 1 个对外 + 少量内部。

### ✅ V10-P2-4 主循环可观测性缺口（P2）——已修复 2026-08-17（见 §13 Fix #18）

**问题**：

1. `dropped_messages` / `pagefault_errors`（V9-P0-1/V9-P1-1 新增）饱和计数只在 `#[cfg(any(test, feature="vm_acl_audit"))]` 下 eprintln 输出，release 无任何出口（VM_INFO 查询不暴露）；
2. `alloc_cycle`（vm_server.rs:480-489）把 `missing_spares` 无条件清零，即使 `free_pages` 未真正释放（C 是"清后下一轮再充"，见 alloc.c:242-279；注释已 DEFERRED 归 24-page-cache）—— 作为 P3 观察，建议后续在 24-page-cache 落地时改为"回收不足则保留计数"；
3. `run()` 无端到端测试（见 V10-P0-2）。

**建议**：`VmReply::InfoStats` 增加 dropped/pagefault-error 计数（对齐 C `vsi_*` 扩展）；`alloc_cycle` 的 one-shot 语义在 24-page-cache 文档中标注为已知差异。

**验证**：`cargo test -p minix-vm --lib vm_server::tests` 增加"连续 receive 失败 N 次后计数可经 InfoStats 观测"的测试。

---

## 12. 第三轮修复记录（2026-08-16）

> 本轮只审查 + 追加 todo，未修复代码。修复顺序建议：V10-P0-1 → V10-P0-2 → V10-P1-1/2/3 → V10-P2-x。
> 修复原则不变：先读目标行 ±5 行 + grep 确认；一次一条；修后 `cargo test -p minix-vm --lib`（437 基线）+ `cargo check` + 重跑受影响 Gate。

---

## 13. 第四轮修复记录（2026-08-17）

> 范围：V10 全量（P0-1/P0-2/P1-1/P1-2/P1-3/P2-1/P2-2/P2-4 修复 + P2-3 DEFERRED 判定）。
> 修复原则不变：先读目标行 ±5 行 + grep 确认；一次一条；修后 `cargo test -p minix-vm --lib`（437 基线）+ `cargo check` + `cargo clippy` + 重跑受影响 Gate。
> 文档同步：15-ipc-dispatch.md / 16-pagefault.md / 26-vm-queries.md / 01-vm-init-main.md / 14-region-lookup.md / 05-physical-memory.md 全量行号与语义刷新，保持文档-代码一致。

### ✅ Fix #11: V10-P0-1 — 三后端 feature 矩阵构建修复

- **Files**: `os/servers/vm/src/vm_server.rs`（`choose_allocator_type` :286-299 / `relocate` :300-369）、`os/servers/vm/src/phys_mem/segment_tree_alloc.rs`（补 `use super::{BumpBuf, CLICK_SIZE, BootMemRegion, METADATA_ALIGN_PADDING}`）、`os/servers/vm/src/lib.rs`（`audit_log!` 宏 :40-54）、`os/servers/vm/src/audit.rs`（no_std sink；`vm_acl_audit` feature 声明在 §10 Fix #10 已补）
- **Before**: `cargo check --no-default-features --features buddy_alloc` → 3 errors（`_total_pages` 引用错、`BuddyAllocator` 未导入）；`--features segment_tree_alloc` → 17 errors（`segment_tree_alloc.rs` 缺 4 项 import）；`--features vm_acl_audit` 非 test 构建 no_std 下用 `eprintln!` → 4 errors
- **After**: 补 import + `_total_pages` 修正；`choose_allocator_type` 在 `segment_tree_alloc` feature 下直接选 `SegmentTree`（此前永远无法构造）；`audit_log!` 三态宏：`#[cfg(test)]` → `std::eprintln!`、`vm_acl_audit` → `crate::audit::emit`（no_std 兼容 sink，格式化后丢弃，syslog 接线点显式）、release 无 feature → 编译消除
- **Verified**: `cargo check -p minix-vm --lib --no-default-features --features {buddy_alloc,segment_tree_alloc,bitmap_alloc,vm_acl_audit}` + `--all-features` 全部通过；`cargo test -p minix-vm --lib --no-default-features --features segment_tree_alloc` → **456 passed**
- **Docs**: 05-physical-memory.md（§3.3 D3 表 / `choose_allocator_type` 注释 / 行号刷新 / `VM_DIRECT_MAP_SIZE` 来源 / V10-P0-1 修复说明）、15-ipc-dispatch.md（§3.7 #7 audit_log! 三态）

### ✅ Fix #12: V10-P0-2 — transport 构造器注入 + 主循环可测

- **Files**: `os/servers/vm/src/vm_server.rs`（`VmServer.transport` 字段 :134-136 / `new_with_boot_params` :149 / `new_for_test` :157 / `kernel_transport` :168 / `init()` mark_initialized :413 / `run` :588 / `run_once` :623 / `MAX_CONSECUTIVE_RECV_FAILURES` :717）、`os/servers/vm/src/ipc/transport.rs`（`TestIpcTransport` :215 / `TestTransportHandle` :269）
- **Before**: 自由函数 `transport()` + 进程全局 `IPC_TRANSPORT_PTR: AtomicPtr` + `Box::into_raw`（泄漏路径）；`TestIpcTransport`/`ipc_transport_for_build()` 从未接线（clippy：never constructed）；主循环不可测；生产路径对 `Err(Unimplemented)` 忙循环
- **After**: `VmServer` 以 `Rc<RefCell<Box<dyn IpcTransport>>>` 持有 transport（V9-P1-2 落地），构造器注入；`run()` 拆出 `run_once() -> RunStep`（测试逐轮驱动）；连续 64 次 receive 失败 → `panic!("IPC transport permanently broken…")`（防忙等，C 的 receive 是阻塞语义）；`IPC_TRANSPORT_PTR`/`ipc_transport_for_build`/自由函数包装全部删除
- **Verified**: `rg "IPC_TRANSPORT_PTR|Box::into_raw|ipc_transport_for_build" os/servers/vm/src` → 0；新增 `test_run_once_dispatch_reply_round`（:1672）/ `test_run_once_receive_failure_counts`（:1770）/ `test_run_busy_loop_protection`（:1849）；`cargo test -p minix-vm --lib` → **441 passed**
- **Docs**: 15-ipc-dispatch.md（§3.3 D3 重写 / §4.1 run+run_once / §4.7 接线 / §5 测试清单）、01-vm-init-main.md（§2.2.4 / §3.3 / §4.4.3）

### ✅ Fix #13: V10-P1-1 — IpcStatus 方法化（is_notify / is_from_kernel）

- **Files**: `os/servers/vm/src/ipc/transport.rs`（`IpcStatus` :46 / `is_notify` :58 / `is_from_kernel` :69）、`os/servers/vm/src/vm_server.rs`（notify 跳过 :639 / P3 debug_assert :813）
- **Before**: 裸 `flags` 字段 + 自由桩函数 `is_ipc_notify`（恒 false）/ `is_from_kernel`（恒 true）——状态字语义未建模，notify 分支不可达
- **After**: `IpcStatus::is_notify()` 按 C `IPC_STATUS_CALL(status) == NOTIFY`（`(flags & 0x3F) == 4`）解析；`is_from_kernel()` 按 `IPC_STATUS_FLAGS_TEST(status, IPC_FLG_MSG_FROM_KERNEL)`（`((flags >> 16) & 1) != 0`）解析；`TestIpcTransport::queue_receive` 可携带真实 `IpcStatus`；主循环在 endpoint 校验前跳过通知；旧桩函数删除
- **Verified**: 新增 `ipc_status_call_bits_match_minix3`（transport.rs:325）/ `test_run_once_notify_skipped_before_dispatch`（vm_server.rs:1735）；`rg "fn is_ipc_notify|fn is_from_kernel" os/servers/vm/src/vm_server.rs` → 0
- **Docs**: 15-ipc-dispatch.md（§1.5 Rust 对应 / §3.4 P3 / §3.7 #3/#4 / §5.1/§5.3）、16-pagefault.md（§4.5 / §5.3 / §7）

### ✅ Fix #14: V10-P1-2 — SUSPEND/live-update 骨架 pin 测试

- **Files**: `os/servers/vm/src/ipc/dispatcher.rs`（`test_dispatch_rs_update_pins_not_implemented` :1561）、`os/servers/vm/src/vm_server.rs`（`DispatchAction::Suspend` 空 arm 注释 :694-697）
- **Before**: `dispatch_rs_update` 恒 `Err(RsError::UpdateNotImplemented)`，`RsUpdateResult::Ok/Suspend` 与 `DispatchAction::Suspend` 空 arm 不可达——"看起来已实现"的骨架
- **After**: 新增 pin 测试断言 `dispatch_rs_update` 当前必须返回 `Error(NotImplemented)`（live-update 落地时翻转该测试）；SUSPEND 链在代码注释中标注为 live-update scaffolding（kernel `sys_update` 依赖）
- **Verified**: `cargo test -p minix-vm --lib ipc::dispatcher` → **29 passed**
- **Docs**: 15-ipc-dispatch.md（§5.1 dispatcher 表新增 pin 测试行）

### ✅ Fix #15: V10-P1-3 — MemType trait 移除 Send + Sync supertrait

- **Files**: `os/servers/vm/src/memtype.rs`（`trait MemType` :16）
- **Before**: `pub(crate) trait MemType: Send + Sync`——单线程用户态服务器却被要求所有 impl `Send + Sync`，根因是 `&'static dyn MemType` 存入 `static` 的存储机制泄漏到 API 契约
- **After**: supertrait 移除；`MEM_TYPE_*` static 存储边界按 `AssumeSyncCell` 同一论证局部处理；单线程模型一致性恢复（lib.rs 明确 `!Send/!Sync` 可接受）
- **Verified**: `rg "trait MemType" os/servers/vm/src/memtype.rs` → 无 `Send + Sync`；`cargo test -p minix-vm --lib` 全绿
- **Docs**: 12-memtype.md（既有 trait 描述核对；无行为变化，行号已核对）

### ✅ Fix #16: V10-P2-1 — 死代码/死 API 收敛（clippy 105 → 0）

- **Files**: `vfs_queue.rs`（`VfsRequestState::FdClose`、`VfsQueueError::NoCallback` 删除）、`region/region_map.rs`（`find_by_end`/`find_greater`/`find_less_equal`/`find_greater_equal` 删除；`SearchType` 收敛为 `{ Equal, Less }`；`find_mut_by_end` 保留——brk.rs:107 在用）、`ipc/transport.rs`（`ipc_transport_for_build` 两 cfg 版本删除）、`region/vir_region.rs`（`VmError::NoMemory/NotFound` 删除，仅留 `InvalidParam`）、`pagetable/mod.rs`（`page_align`/`page_align_down` 移入测试模块）、`global.rs`（`total_pages()` cfg(test) 门控）、`fdref.rs`（`create`/`find_by_dev_ino`/`len`/`is_empty` cfg(test) 门控）、`vfs_queue.rs`（`has_active`/`active_req_id`/`queued_count` cfg(test) 门控）、`vm_server.rs`（`page_cache`/`vfs_queue`/`vfs_queue_mut`/`is_initialized` cfg(test) 门控；`handle_fork/brk/exit` 移入 `#[cfg(test)] impl`）
- **DEAD/DEFERRED 标注**（`#[allow(dead_code)]` + 注释 TODO 指针）：`sanity.rs::verify_refcounts`、`alloc_stats.rs::active_allocations/check_leak`、`acl.rs::acl_clear/mask`、`cow_exec_pf.rs::cow_resolve_region`、`fork.rs::cow_copy_page`、`dispatcher.rs::dispatch_exec_newmem`、`vm_self_map.rs::vm_self_query`、`vmproc/table.rs::VmProcIter` 等 30+ 处
- **Verified**: `cargo clippy -p minix-vm --lib` → **0 warnings**（`grep -c "never"` → 0）；`cargo test -p minix-vm --lib` → **441 passed**（feature 矩阵含 `segment_tree_alloc` 456 / `buddy_alloc` 441）
- **Docs**: 14-region-lookup.md（§3.6/§4.1/§4.2/§5.1-§5.4 删除项同步）、todo.md §11 V10-P2-1 表逐行核对

### ✅ Fix #17: V10-P2-2 — DirectMap 布局常量收敛

- **Files**: `os/arch/src/arch/direct_map.rs`（`DirectMapArch::VM_DIRECT_MAP_SIZE` :45）、`os/servers/vm/src/direct_map.rs`（`VM_DIRECT_MAP_SIZE` :20 从 `CurrentDirectMap::VM_DIRECT_MAP_SIZE` 取；`is_direct_map_virt` 补上界 :55）
- **Before**: `VM_DIRECT_MAP_SIZE = 1 << 30` 硬编码在 VM 服务器，`DirectMapArch` 不暴露 size；布局演进（AArch64/RISC-V 改 direct map 窗口）会静默漂移；`is_direct_map_virt` kernel 分支无上界
- **After**: `DirectMapArch` 增加 `VM_DIRECT_MAP_SIZE`（x86_64/aarch64 1 GiB、riscv64 16 GiB）；`VM_HEAP_BASE == VM_DIRECT_MAP_BASE + VM_DIRECT_MAP_SIZE` 作为布局不变式测试断言（direct_map.rs:149）；`is_direct_map_virt` 补 `virt < VM_DIRECT_MAP_BASE + VM_DIRECT_MAP_SIZE` 上界与边界测试（:139-140）
- **Verified**: `cargo test -p minix-vm --lib direct_map` → passed；`rg "VM_DIRECT_MAP_SIZE" os/servers/vm/src` 与布局文档一致
- **Docs**: 05-physical-memory.md（§3.3 元数据切分判据、`VM_DIRECT_MAP_SIZE` 来源）、10-vm-relocation.md（既有引用核对）

### ✅ Fix #18: V10-P2-4 — 主循环可观测性（InfoStats 扩展字段）

- **Files**: `os/libs/minix-types/src/ipc/vm.rs`（`VmReply::InfoStats` 增 `dropped_messages: u64` :673 / `pagefault_errors: u64` :677）、`os/servers/vm/src/vm_server.rs`（字段 :90/:103、访问器 :547/:553）、`os/servers/vm/src/ipc/dispatcher.rs`（`dispatch_info` 传参 :954-968、`dispatch_by_number` 取数 :1041-1042、CALLMAP 构造 :1172-1173）、`encode_reply_data`（:1299-1314，C wire 无槽位 → 丢弃，与 InfoUsage 的 minflt/majflt 同策略）
- **Before**: `dropped_messages`/`pagefault_errors` 饱和计数只在 `#[cfg(any(test, feature="vm_acl_audit"))]` eprintln 输出，release 无出口（VM_INFO 查询不暴露）
- **After**: 两个计数经 `VmReply::InfoStats` 扩展字段在进程内可观测（minix-rs 扩展，[ARCH: A-14]/[ARCH: A-15]；C `struct vm_stats_info` wire 布局不变）
- **Verified**: 新增 `test_dropped_messages_observable_via_info_stats`（vm_server.rs:1792）——3 次 receive 失败 → `dropped_messages==3` 经 VMIW_STATS 可观测；`cargo test -p minix-vm --lib` → **441 passed**
- **Docs**: 26-vm-queries.md（§3.4 扩展字段 / §4.8 encode 表 / §5.1 测试行 / §5.2 统计）

### ⏸ V10-P2-3 DEFERRED 判定（2026-08-17）

**判定：DEFERRED，不做大规模 errno 重构。**

**理由**：
1. V10-P2-1 已把 `cargo clippy -p minix-vm --lib` 归零（105 → 0），错误枚举本身不是构建/正确性/可观测性问题；
2. 15 个 crate 内错误 enum 收敛（P2-2）属于架构演进，正确形态是"底层 `Errno`（minix-types）→ 服务错误（携带上下文）→ dispatcher 统一转 M1 错误码"，需 kernel IPC 落地后统一对外出口；
3. getrusage 的上下文相关映射（`ProcessNotFound` → getrusage 上下文 ESRCH、其余 EINVAL，`query_rusage_error_to_vm_error` dispatcher.rs:1370）已证明"单一 `From` 无法表达"，统一需要专项设计（derive 宏 / `to_vm_error` trait），不宜在 P2 仓促做；
4. 每模块自带的 `From<EndpointError>` + errno 映射 + 测试（如 query.rs:43-57 注释）当前可维护，随 P2-2 专项规划推进。

**验证锚点（保留）**：`rg "pub(crate) enum .*Error" os/servers/vm/src --glob '*.rs'` 清单——收敛到 1 个对外（`minix_types::VmError`）+ 少量内部时关闭本条。

---

## 14. 第五轮架构级审查：查漏补缺 + 分层全景（2026-09-06）

> 范围：`os/servers/vm/src/` 全部（46 个 .rs，约 25,600 行，含测试）+ 接口面核对（`os/libs/minix-types/src/ipc/vm.rs` wire 层、`os/arch` 的 paging/direct_map/pt_alloc、`os/kernel` 的 ipc.rs / vm_handoff.rs 对接面、`os/libs/minix-sys` 桩库）。
> 方法：先查漏补缺后架构审视，五步——① Gate A 覆盖穷举（`tools/coverage-extract/coverage-extract.py` 重跑，371 个 C 符号，产物 `.review/claude/vm/v11/SYMBOLS.md`）；② IPC 调用号三方对账（minix-types 常量表 ↔ dispatcher 路由 ↔ C com.h/CALLMAP）；③ 18 项存量嫌疑点的 staleness 逐条核查（上轮 2026-08-17 之后有 a4c6c9141 / c2600a1b2 / 3a0cc39f0 三个提交、约 1,400 行增量）；④ 核心 trait × 测试矩阵 + Gate E 文档测试名对账（3 篇 94 个具名测试）；⑤ 分层架构审视（L0 crate 边界 / L1 服务器核心 / L2 子系统），对照 Redox 与 Rust no_std 社区惯例。
> 去重声明：V9/V10 已修与未修条目不再重复列入。本轮聚焦三件事：(a) 08-17 之后增量代码的首轮审查；(b) 存量 DEFERRED 的依赖依据复核——两条依赖链（kernel IPC、arch paging）都有实质进展，多个旧判定的前提已变化；(c) 测试与文档缺口。
> **本轮最重要的两个发现**：① **P1-3 的 DEFERRED 依据已失效**——内核侧 IPC 核心已落地（`os/kernel/src/ipc.rs` 2,906 行，send/receive/notify/sendrec/do_ipc 全部有真实实现），用户态桩库 `minix-sys` 也已成形（5,738 行，含 IPC trap 桩与 VM 客户端桩），VM 侧 transport 落地的材料已经齐备；② V10 收敛后的增量改动引入了 clippy 回归（默认 1 条 / all-features 7 条）与 4 处过时注释，但增量代码本体（boot.rs / vm_self_map.rs / vmproc）质量良好、零 stub 标记。
> 本轮只审查 + 追加 todo，未修复代码（与 §12 先例一致）。修复顺序建议见 §14.7。

### 14.0 基线复核（2026-09-06 实测）

| 命令 | 结果 | 与 2026-08-17 对比 |
|---|---|---|
| `cargo test -p minix-vm --lib` | **448 passed / 0 failed** | 441 → 448（增量 7 个测试：boot reconcile 系列、`test_swap_proc_slot_preserves_identities` 等） |
| `cargo test … --features segment_tree_alloc` | **463 passed / 0 failed** | 456 → 463 |
| `cargo test … --features buddy_alloc` | **448 passed / 0 failed** | 新增基线项（与默认一致） |
| `cargo clippy -p minix-vm --lib` | **1 warning**（默认）/ **7 warnings**（all-features） | 0 → 1 / 7，**回归**，见 V11-P2-3 |
| `cargo check -p minix-vm --all-features` | 通过，无 error | 持平 |

08-17 之后的**功能增量**（本轮逐项核实）：
- `alloc_cycle` 接入有界页缓存回收：`vm_server.rs:580` 调用 `page_cache.free_pages(FREE_CACHE_BATCH=1024, …)`（常量 `vm_server.rs:47`）——原 DEFERRED 的"回收"半边已落地；"回收后重试原分配"仍未实现，且 `:566-572` 的文档注释还写着"replenishment body is DEFERRED"，已轻度过时（见 V11-P2-4 附带项）。
- Getrusage 补编码 minflt/majflt：`vm_server.rs:1417-1418`（标注 FIX VMI-3）；`UsageInfo` 字段填充在 `query.rs:391-394`。`VmReply::InfoUsage` 路径的 minflt/majflt 仍无 M1 槽位（`vm_server.rs:1356-1357`），维持原 DEFERRED。
- `VM_UNMAP_PHYS` 接线（`dispatcher.rs:1061`，对应 21-P1-1）。
- arch 侧 `X86_64Paging::new`/`destroy` 落地（`os/arch/src/x86_64/paging.rs:464-485` / `:487-497`）——由此引发 VM 侧 4 处注释过时（V11-P2-4）。

### 14.1 查漏补缺总表（Gate A）

#### 14.1.1 coverage-extract 重跑与 C 符号判定

```
$ python3 tools/coverage-extract/coverage-extract.py vm notes/rewrite/fork-syscall-rewrite/02-stage-vm \
    --rust-dir os --c-dir minix3/minix/servers/vm \
    --semantic-map tools/coverage-extract/vm-semantic-map.json \
    --output .review/claude/vm/v11/SYMBOLS.md
Loaded semantic map: 81 entries
Found 371 C symbols (189 funcs, 14 structs, 167 macros, 1 enums)
Coverage Summary for vm:
  Total C symbols: 371
  Doc covered: 339 (91.4%)
  Rust covered (name-match): 43 (11.6%)
  完全缺口 (无文档无Rust): 32
```

判定结论：
1. **32 个"完全缺口"中 28 个是 `cavl_impl.h` 的 AVL 内部宏**（`AVL_SET_GREATER`/`L__BIT_ARR_0` 等）——`regionavl.c` 已被 `region/region_map.rs` 的 BTreeMap 有序结构替代（[ARCH] 登记，14-region-lookup.md），属合理缺席，不是缺口。其余 4 个同类（`L__` 宏族）同源。
2. **146 个"有文档无 Rust（按名字匹配）"经抽查判定绝大多数为语义吸收**：`do_brk`→`brk.rs::handle_brk`、`real_brk`→brk.rs 内部收缩/扩展路径、`lru_add/lru_rm/lru_touch`→`PageCache` 的 LRU 序维护、`makehash`→`by_dev: BTreeMap` 主键（`page_cache.rs:204`）、`get_stats_info`→`query.rs:302` + `page_cache.rs:419`、`byino/bydev`→`page_cache.rs:204-208` 的 `by_dev`/`by_ino` 双索引、`do_procctl_notrans`→dispatcher P4 普通路由。**未发现新的函数级语义缺失**。
3. Rust 名称匹配率 11.6% 是"名字匹配"的机械结果（C→Rust 普遍改名 + 行为吸收进结构体方法），语义映射表（81 条）与 26 篇 CONVERGED 文档兜底，不构成缺口信号。相比 2026-06-12 `checklist.md` 的 59% 总覆盖基线，文档侧覆盖已实质提升。

#### 14.1.2 IPC 调用号矩阵（49 调用全量对账）

三方对账（`os/libs/minix-types/src/ipc/vm.rs` ↔ `os/servers/vm/src/ipc/dispatcher.rs` ↔ C `com.h:627-773` + `main.c:137-176`）结论：

| 维度 | 结果 |
|---|---|
| 基准值 | `VM_RQ_BASE=0xC00`、`NR_VM_CALLS=49` 两侧一致（vm.rs:45/:149 ↔ com.h:627/:769） |
| 常量集合 | Rust 31 个 `VM_*` 调用号与 C 完全一一对应，无 Rust 独有扩展、无遗漏 |
| 路由对齐 | `dispatch_by_number` 26 个 arm 与 C CALLMAP 26 条严格 1:1：号值、handler 语义、ACL 拒绝回 ENOSYS（对齐 C main.c:145）、非法调用号 ENOSYS 全部一致 |
| 两侧都不注册 | `VM_EXEC_NEWMEM`(+3)、`VM_ADDDMA/DELDMA/GETDMA`(+12/13/14)：C 落 `result=ENOSYS`（main.c:139/165），Rust 落 `_` 臂 → ENOSYS（dispatcher.rs:1229）——可观察行为一致 |
| 特殊路径 | VFS transid（P1）/ RS_INIT（P2）/ VM_PAGEFAULT（P3）三分支与 C main.c:137-176 的裁决顺序结构一致；`VM_PAGEFAULT` 不经 dispatch_by_number（vm_server.rs:822-842） |

三条**已声明的语义级偏差**（均有登记，列表备查，不算新缺口）：
1. `VM_RS_SET_PRIV` 的 call_mask 位图传输依赖 safecopy，dispatcher 解码恒传 `None`（dispatcher.rs:1093-1097；25-rs-services.md:399 已登记 A-8 契约与 fail-closed 论证：user proc 得默认 ACL、sys proc 回 EINVAL）。
2. `VM_PROCCTL` HANDLEMEM 的 transid 异步续作被同步化（dispatcher.rs:266-272；22-vm-exit.md:405 偏差表登记，backlog B1）。
3. `VM_PAGEFAULT` 用 64 位专用 `m_vm_pagefault` overlay 替代 C 的 `m1_i1`（16-pagefault.md:371-386 [ARCH] 登记，含 P0 解码错位修复记录）。

顺带刷新 V9-P2 的两个表驱动化锚点计数：`as usize - vm_rq_base` ×26（dispatcher.rs:1054-1219，每 arm 一处）、`m2.m2i1/m2i2/m2i3` 手工字段访问 ×14（另有 `m1.m1i1` 直接访问 ×3：dispatcher.rs:1116/:1132/:1138）。

#### 14.1.3 缺口登记（本轮新发现，编号 G-V11-\*）

| 编号 | 缺口 | C 锚点 | Rust 现状 | 判定 |
|---|---|---|---|---|
| G-V11-1 | `sef_cb_signal_handler`（SIGTERM/SIGSTOP 等信号回调，RS live-update 的前置机制） | main.c:731 | 无对应物；`rs_handshake`（vm_server.rs:900）只替代了 `sef_startup` 半边 | DEFERRED：依赖内核信号交付，归 A-8 live-update 依赖链，随 RS_UPDATE 步骤 4-7 一并规划 |
| G-V11-2 | `usedpages_reset` / `usedpages_add_f` 双重分配运行时检测 | alloc.c:495-530（`#if SANITYCHECKS` 块） | 无对应物 | 并入 V10-P2-1 已登记的 sanity_checks feature 建议（`sanity.rs:58-59` verify_refcounts 同源）；本条补 C 锚点 |
| G-V11-3 | PM↔VM 集成测试整体停用 | —（测试基建） | `os/tests/pm_vm_fork.rs`（96 行）与 `pm_vm_fork_test.rs`（105 行）正文整体被注释，文件头自注 "DEPRECATED: permanently disabled"；属独立 crate `minix-tests`，`cargo test -p minix-vm` 不覆盖 | 见 V11-P2-5 |
| G-V11-4 | segment_tree 后端在默认 CI 下 0 条测试执行 | —（测试基建） | `allocator_tests.rs` 的 segment-tree parity 测试全部 `#[cfg(feature = "segment_tree_alloc")]` 门控（:14 起）；默认 feature 只跑 bitmap+buddy parity（463−448=15 条不执行） | 见 V11-P2-5（CI feature 矩阵，V10-P0-1 建议的延续） |
| G-V11-5 | vfs_queue 无超时/失败恢复机制 | vfs.c（143 行全文无 timeout/revive/alarm） | 与 C **语义一致，不是缺口**。更正本文 §6 旧表述"注意补请求超时/失败恢复路径的语义测试"——C 的 VFS 挂起请求本无超时语义，补超时属 [ARCH] 扩展，须先立项登记再实现，不应默认实现 | 更正记录（反"无脑加码"） |
| G-V11-6 | 文档 §5 测试名漂移 3 处 + 1 处 dispatcher 层测试真缺失 | —（15-ipc-dispatch.md §5） | 94 个具名测试 90 个精确命中；3 个实际名与文档名不一致（见 V11-P2-6）；`test_dispatch_setcache_rejects_zero_dev_and_ino` 无近似命中（该 NO_DEV 守卫改由 `page_cache::tests::test_addcache_rejects_no_device` 覆盖，dispatcher.rs:1958-1960 注释声明） | 文档侧 P0-fact + 测试侧可选补充，见 V11-P2-6 |

### 14.2 存量 DEFERRED 依据复核（本轮核心工作之一）

上轮（2026-08-17）之后，两条关键依赖链有实质进展。逐条复核存量 DEFERRED 判定的前提：

| 条目 | 原 DEFERRED 依据 | 2026-09-06 事实（锚点） | 判定 |
|---|---|---|---|
| P1-3 transport 生产路径 | "depends on kernel IPC core"（transport.rs:183/:197 `unimplemented!()`） | **内核侧已落地**：`os/kernel/src/ipc.rs`（2,906 行）实现 `send`(:814)、`receive`(:934)、`notify`(:1280)、`sendrec`(:1305)、`senda`(:1356)、`do_ipc`(:1668)、死锁检测(:739)；:914 注释自证"旧实现是 30 行 stub"已被替换。用户态入口：`kernel/src/syscall.rs:548-556` IPC_VECTOR 33 中断门 + IPC 调用号 1-16 分发。用户态桩库：`minix-sys/src/ipc.rs` 提供 `TrapVector`(:82)、`IpcStatus`(:136)、`DirectTrapTransport`(:527)，`minix-sys/src/vm.rs`（619 行）提供 VM 客户端桩（mmap_via/break_via 等） | **依据失效 → 建议解封**，见 V11-P1-1 |
| P1-2 / V9-P1-3 VmContext 状态收敛 | "需专项规划"，且测试无法驱动主循环 | Fix #12（2026-08-17）后 transport 已构造器注入，`TestIpcTransport` 可驱动 `run_once`（现有 4 个 run_once 测试：vm_server.rs:1684/:1726/:1770/:1865）；`parts_mut()` 4 元组仍在（调用 `vm_server.rs:1018`，定义 `:1056`） | **阻塞已解除**，见 V11-P1-2 |
| P1-4 KERNEL_LAYOUT mock | "boot-info 接缝未闭环" | `boot.rs:46-53` 已建模 `KernelAllocated { static_bytes, dynamic_bytes }`（对齐 main.c:492-495），但只有**字节总数**，无 kernel text/data 的基址与页数；`vm_server.rs:437-450` 的 mock 值（含 `:450` 的 `0xFFFF_FFFF_8000_0000`）仍在 | 依据部分成立：需 `VmBootHandoff` 补 span 字段，见 V11-P2-7 |
| V10-P2-3 错误枚举收敛 | "需 kernel IPC 落地后统一对外出口" | kernel IPC 已落地（见 P1-3 行） | 可启动专项；维持 DEFERRED 但依据更新，宜随 V11-P1-1 的 transport 落地一并做 |
| 过时注释（非 DEFERRED，staleness） | — | `brk.rs:236`、`vmproc/vmproc.rs:188`、`vmproc/vmproc_handle.rs:352-354`、`region/mod.rs:21-22` 四处仍声称 `X86_64Paging::new()`/方法"是 todo!() / 未实现"，实际 `os/arch/src/x86_64/paging.rs:464-497` 已实现 new/destroy（全文件 grep `todo!` 零命中） | 见 V11-P2-4 |
| 其余存量 DEFERRED | 各自依赖未变 | 逐条 grep 核实仍成立：`dispatch_exec_newmem` stub 且未路由（dispatcher.rs:820-833）；`sys_fork` 假端点 stub（fork.rs:398-408，调用点 :321）；`cow_resolve_region`/`cow_copy_page` 仅测试驱动（cow_exec_pf.rs:327-330）；VFS_FDCLOSE 丢弃（exit.rs:140-147）；RS_UPDATE 步骤 4-7（rs.rs:328-330）与 PREPARE `map_proc_dyn_data`（rs.rs:250-251）；DMA 三条不入 match（dispatcher.rs:1223-1229）；`ipc_call_rs_init` 返回空表（vm_server.rs:1124-1157）；sanity/alloc_stats 无生产入口（sanity.rs:58-59、alloc_stats.rs:46/:57-58）；bitmap `cache_freepages` 三步路径（bitmap_alloc.rs:309-351）；region close 入队（region/mod.rs:56-82）；audit syslog 未接线（audit.rs:16-19）；arch `destroy()` 只清零不回收中间页表页（paging.rs:487-497） | 维持 DEFERRED，锚点已刷新 |

### 14.3 V11 条目

#### V11-P1-1 【解封】KernelIpcTransport 落地路径（原 P1-3，DEFERRED 依据失效）

**问题**：`ipc/transport.rs` 的 `KernelIpcTransport::receive/send` 仍是 `unimplemented!()`（transport.rs:183/:197），主循环生产路径不可运行。但该条目的 DEFERRED 依据（"wiring pending kernel IPC core"）已不成立：内核 IPC 核心与用户态桩库都已落地。

**证据**：
- 内核侧：`os/kernel/src/ipc.rs`（2,906 行）——`send`(:814)、`receive`(:934)、`notify`(:1280)、`sendrec`(:1305)、`do_ipc`(:1668)、死锁检测 `detect_deadlock`(:739)；`os/kernel/src/syscall.rs:548-556` 记载 IPC 经 IDT 向量 33 进入（对齐 C `protect.c:147` 与 `mpx.S:183-184`），IPC 调用号 1-16 分发。
- 用户态桩库：`os/libs/minix-sys/src/ipc.rs`（`TrapVector`:82、`IpcStatus`:136、`AsyncSendQueue`:280、`DirectTrapTransport`:527、测试用 `CannedTransport`:568）；`os/libs/minix-sys/src/vm.rs`（619 行，`mmap_via`:171、`break_via`:241、`fork_address_space_via`:266 等 VM 客户端桩）。
- VM 侧现状：`os/servers/vm/Cargo.toml:14` 声明了 `minix-sys` 依赖，但 `os/servers/vm/src/` 全目录 grep `minix_sys::` 零命中——依赖闲置。

**影响**：P1-3 是最大的单一阻塞咽喉——`rs_handshake` stub（vm_server.rs:900 起）、`ipc_call_rs_init` stub（:1124-1157）、VFS_FDCLOSE 发送（exit.rs:140-147）、region close 入队（region/mod.rs:56-82）、audit syslog 转发（audit.rs:16-19）全部依赖同一 transport 落地。依赖已就位而接线未动，缺口会在陈旧注释的掩护下继续沉睡。

**建议**（三步，每步可独立交付并验证）：
1. **首选**：`KernelIpcTransport` 基于 `minix_sys::ipc` 桩实现——`receive` 走 `TrapVector`/`IpcStatus` 路径（与 V10-P1-1 已落地的 `is_notify()`/`is_from_kernel()` 位解析对接），`send` 走 sendrec 语义；VM 的 `Cargo.toml` 依赖从闲置转为实际使用。
2. 落地顺序上先接 `rs_handshake`（VM 启动的第一件事，C main.c:149-152），再接 VFS_FDCLOSE/close 入队（把 region/mod.rs:75 与 exit.rs:145 的两处 `let _close` 换成真实的 `VfsRequestQueue` 入队 + 发送）。
3. 每步同步刷新 15-ipc-dispatch.md 与 23-vfs-interaction.md 的接线状态标注；V10-P2-3（错误枚举收敛）与 V11-P1-2（VmContext）宜在 transport 可运行之后做，因为统一对外出口与状态收敛都会改动 dispatch 签名，分两轮做返工少。

**验证**：`rg "unimplemented!" os/servers/vm/src/ipc/transport.rs` 归零；`rg "minix_sys::" os/servers/vm/src` 非零；新增 TestIpcTransport 之外的"真实 trap 路径"集成测试（可先在 `minix-sys` 的 `CannedTransport` 语义上做半实物回放）。

#### V11-P1-2 【解封】VmContext 状态收敛（原 P1-2 / V9-P1-3，阻塞已解除）

**问题**：P1-2（全局可变状态散落 4 处 static + 实例字段）当年搁置的理由是"需专项规划 + 主循环不可测"。现在后半条已不成立：Fix #12 之后 transport 构造器注入，`TestIpcTransport` 可逐轮驱动 `run_once`。重构可以在每一步都有测试兜底的情况下进行。

**证据**：`parts_mut()` 4 元组仍在（调用 `vm_server.rs:1018`，定义 `:1056`）；`global.rs` 的 `BOOT_INFO`/`TOTAL_PAGES`/`KERNEL_LAYOUT` static 与 `vmproc/table.rs`、`fdref.rs` 的全局表仍在；V9-P1-3 列出的 9 个 `handle_xxx` 同构包装仍存在（部分已移入 `#[cfg(test)] impl`，见 Fix #16，但生产侧模式未变）。

**影响**：P2-3（parts_mut 脆弱性）、P2-4（测试全局污染）都挂着等这一步；每新增一个共享子系统，4 元组就扩大一维，重构成本随时间上升。

**建议**：
1. **首选**：按 V9-P1-3 给出的 `VmContext` 设计直接实施，分三小步落地（每步 `cargo test -p minix-vm --lib` 448 基线全绿再进下一步）：第一步把 `PageFrames`/`PageCache`/`VfsRequestQueue`/`PageAllocator` 收进 `VmServer` 直接字段（消灭 parts_mut）；第二步 `MessageDispatcher` 签名统一收 `&mut VmContext`（消灭 handle_xxx 透传）；第三步 `fdref`/`vmproc table` 收敛，static 只留 boot 期常量。
2. **次选**：若想更小步，先只做第一步（parts_mut 消灭），它独立可交付、风险最低、且是 V9-P2-2 表驱动化（dispatcher 收 `&mut VmContext`）的前置。
3. Redox 对照：Redox 用户态 scheme daemon 把全部状态组织为 context 结构体、主循环持 `&mut Context` 单一所有权；static 只用于 boot 期一次性常量。V9-P1-3 的设计与社区惯例一致，本轮补足"测试已可驱动"的新证据。

**验证**：`rg "parts_mut" os/servers/vm/src` 归零；`rg "AssumeSyncCell" os/servers/vm/src` 收敛到 boot 期常量与分配器基础设施；测试不再需要 `reset_vm_self_pt_for_test`/`MOCK_BASE_MUTEX` 串行化（`rg "MOCK_BASE_MUTEX" os/servers/vm/src` 归零）。

#### V11-P1-3 物理分配器后端选择语义"双真相源"矛盾

**问题**：后端优先级在两处声明且**方向相反**：
- `phys_mem/mod.rs:107-110`：`DefaultAllocator` 类型别名的注释宣称优先级为 "buddy > segment-tree > bitmap"；
- `vm_server.rs:290-302`：`choose_allocator_type` 的实际选择是 **segment_tree feature 开启即直接胜出**（`:297-300` 的 cfg 块无条件 `return PhysAllocType::SegmentTree`），只有未开 segment_tree 时才按阈值考虑 buddy。
两处都是"文档型声明"（别名无构造器、纯 `#[allow(dead_code)]`），但语义冲突。`cargo clippy --all-features` 的 `unreachable expression` 警告（vm_server.rs:301）正是矛盾的症状：两个 feature 同时开启时 `:301` 的 `PhysAllocType::Bitmap` 回落分支不可达。

**证据**：`cargo clippy -p minix-vm --lib --all-features --message-format short` → `servers/vm/src/vm_server.rs:301:9: warning: unreachable expression`；05-physical-memory.md §3.3（:469）记录的是 `choose_allocator_type` 的行为（与代码一致），即**矛盾方是 mod.rs 的别名注释**。clippy all-features 下的 7 条 minix-vm 警告中有 4 条是同一根源的连带（见 V11-P2-3）。

**影响**：组合 feature（`--all-features`）的选择语义靠阅读 cfg 块才能确定；将来若有人"照注释实现" `DefaultAllocator` 的构造器，会得到与 `choose_allocator_type` 相反的选择。这正是 V10-P0-1（feature 矩阵无法构建）的同族风险——组合态没人看。

**建议**：
1. **首选**：删除 `DefaultAllocator` 别名（mod.rs:107-125，四个 cfg 分支共 19 行，零使用者）——它是"无构造器的文档别名"，其声明职责已由 `choose_allocator_type` + 05 文档 §3.3 承担；删除后一并消除 all-features 下与 alias 相关的歧义。
2. **次选**：保留别名但把注释改为与 `choose_allocator_type` 一致（segment-tree > buddy > bitmap），并在 `choose_allocator_type` 加一条组合 feature 的单元测试（`--all-features` 下断言选中 SegmentTree）。
3. 无论哪种，`choose_allocator_type` 的文档注释（vm_server.rs:280-289）应补一句"组合 feature 下 segment_tree 优先"的显式声明。

**验证**：`rg "DefaultAllocator" os/servers/vm/src` 归零（或注释已改 + 组合测试存在）；`cargo clippy -p minix-vm --lib --all-features 2>&1 | grep -c unreachable` → 0。

#### V11-P2-1 X86_64Paging 零契约测试：真实页表路径完全未验证

**问题**：`Paging` trait 的契约测试（os/arch `arch/paging.rs` 36 个测试）全部跑在 `MockPaging`（arch/paging.rs:544 定义、:574 impl）上；`X86_64Paging`（x86_64/paging.rs:168，impl :369）自身的 11 个测试全是 PTE 标志位 roundtrip 与索引数学——**没有任何测试构造真实页表执行 map/unmap/query**。VM 侧测试则整体跳过页表操作（region/mod.rs:21-22 注释声明；`pagetable/mod.rs:23` 的 `type PageTable = minix_arch::CurrentPaging` 让 VM 与真实实现耦合，但测试里传 `None`/零桩）。也就是说：从 `X86_64Paging::new`（paging.rs:464，经 pt_alloc 分配 PML4）到 `map`/`unmap`/`query`/`switch` 的真实链路，当前零测试覆盖，而 VM 的全部语义（CoW 权限翻转、brk 映射、munmap 摘除）最终都压在这条链上。

**影响**：这是本轮测试盘点中最大的结构性缺口。trait 契约只对 mock 验证 = "接口形状正确"≠"硬件行为正确"；一旦 V11-P1-1 把 transport 接通、VM 真正跑起来，页表链路的 bug 将第一次获得被执行的机会，且以最难排查的形式（二级错误：页错误风暴、陈旧 TLB）出现。

**建议**（对应 V9-P2-4 的具体落地形态，两档）：
1. **首选（纯 Rust 层）**：在 `os/arch` 提供一个 `#[cfg(test)]` 的软件模拟 `SimPaging`（BTreeMap 模拟四级表，行为契约对齐 `Paging` trait 文档），把现有 36 个契约测试从只跑 `MockPaging` 扩为"MockPaging + SimPaging 双实现参数化"；VM 侧测试改注入 `SimPaging`，使 region/brk/munmap 的测试真正走到"写 PTE→查询→摘除"路径。Redox rmm 的 `EmulateArch`（`cfg(feature="std")` 下 BTreeMap 模拟）是同一思路的先例。
2. **次选（硬件层冒烟）**：workspace 已有 `os/qemu-tests/` 基建（test-memmap / test-paging-enable / test-kernel-map 等 bootstrap 测试内核）。为 VM 加一条 QEMU 冒烟：boot shim 拉起 VM → VM 建自身页表（`init_vm_self_pt`）→ 对测试页执行 map/query/unmap → 结果写串口。x86_64 一条即可，先覆盖 `new/map/query/unmap` 四个最高风险方法。
3. 两档不互斥：第 1 档管回归密度，第 2 档管"真实硬件对不对"。若资源只够一个，先做第 2 档——它验证的是 SimPaging 无法替代的前提假设（PTE 位布局、NX、TLB 行为）。

**验证**：`rg "struct SimPaging|impl Paging for SimPaging" os/arch/src` 非零；`cargo test -p minix-arch` 测试数从 36 增加；或 `os/qemu-tests/` 出现 vm-paging 冒烟目标且 CI 可跑。

#### V11-P2-2 MemType / PhysAllocator 方法级测试缺口矩阵

**问题**：按"核心 trait 方法 ≥3 测试（正常/边界/错误）"的项目标准盘点，两个核心 trait 的缺口集中：
- **MemType**（memtype.rs，6 实现 × 19 方法）：`ContiguousAnonymous` 六实现中唯一 0 直测（其 `ev_new`:747、`ev_resize`:723、`ev_pagefault`:831 等 12 个 override 全无测试）；trait 级完全无测试的方法（所有实现）：`ev_new`、`ev_resize`、`ev_sanitycheck`(:119)、`ev_delete`、`ev_low_shrink`、`ev_reference`（fork.rs:82 只用 failing mock 测过回滚，六个真实现无直测）。`DirectPhysical` 除 `name` 外全部 override 无测试（`ev_pagefault`:402、`ev_copy`:438、`pt_flags`:452）。
- **PhysAllocator**（phys_mem/alloc_trait.rs，5 方法 × 3 后端）：`allocator_tests.rs`（42 测试）只覆盖 `alloc_mem`/`free_mem` 两方法；`reserve_pages` 仅 bitmap 有测试（bitmap_alloc.rs:735），buddy（:384）/segment_tree（:342）实现无测试；`available_regions` 仅 bitmap 有 3 个测试（:755/:792/:840）；`memstats`/`total_count` 无任何直接测试（grep 全 src 仅实现处与 vm_server.rs:328 生产调用）。

**影响**：`ev_resize` 是 brk 收缩/扩展路径的核心回调（19-vm-brk.md 主线），`ev_delete` 是 exit 释放路径的核心回调（22-vm-exit.md 主线）——两个生产主线的核心回调当前靠间接覆盖；`available_regions` 是 `relocate()` 元数据搬迁的输入（vm_server.rs:327-329），换后端时该路径零兜底。

**建议**：补测优先级按生产暴露面排序——① `ContiguousAnonymous` 的 `ev_resize`/`ev_pagefault`（brk 主线）；② `ev_delete` × 六实现（exit 主线，可表驱动一次覆盖）；③ `reserve_pages`/`available_regions` 的 buddy/segment_tree 补齐（与 V11-P2-5 的 CI feature 矩阵联动，segment_tree 的补测天然要求该矩阵存在）；④ 其余按机会补。不建议一次性铺满 6×19 矩阵——按主线风险投递。

**验证**：`cargo test -p minix-vm --lib memtype` 与 `allocator_tests` 用例数增加；`rg "fn test_.*contiguous" os/servers/vm/src/memtype.rs` 非零。

#### ✅ V11-P2-3 clippy 回归收敛 + 死代码增量（V10-P2-1 的增量复盘）——已修复 2026-09-06（见 §15 Fix #21）

**问题**：V10 收敛时 `cargo clippy -p minix-vm --lib` 为 0 warnings；2026-09-06 实测默认 feature **1 条**、`--all-features` **7 条**。全部来自 08-17 之后的增量代码：

| 锚点 | 警告 | 归因 |
|---|---|---|
| vm_server.rs:301 | unreachable expression（all-features） | V11-P1-3 的症状，随该条处理 |
| vmproc/vmproc_handle.rs:433 | method `page_table` never used（默认即可见，即那条"1"） | 新增的不可变 getter，孪生 `page_table_mut` 有 8 处调用、它自己 0 处 |
| phys_mem/mod.rs:196-207 | `as_buddy`/`as_buddy_mut` never used（all-features） | 与 `is_bitmap`/`as_bitmap_mut`（:188-193 已标注）不同，这对没有加 `#[allow(dead_code)]` |
| phys_mem/buddy_alloc.rs:38 | `MAX_ORDER` never used（all-features） | V10-P2-1 表中 buddy 9 项 DEFERRED 标注的漏网项 |
| phys_mem/buddy_alloc.rs:94、segment_tree_alloc.rs:135 | `metadata_size` 等 4 项 never used（all-features） | 同上：`PhysAllocType::metadata_size`（mod.rs:249）承担了元数据计算，allocator impl 上的同名方法成死代码 |
| phys_mem/segment_tree_alloc.rs:263 | `1 * 1024 * 1024` identity op（all-features） | 照搬 C 字面量风格，纯噪音 |

**影响**：轻——无行为问题；但"all-features 态无人看"的模式与 V10-P0-1 同源，放任会再次积累到构建破坏才发现。

**建议**：一次收敛清零：`page_table()` 按 vmproc_handle.rs 既有惯例补 `#[cfg_attr(not(test), allow(dead_code))]` 或直接删（有调用者再加）；`as_buddy*` 与 `metadata_size` 系按 V10-P2-1 的 DEAD/DEFERRED 标注惯例处理或删除；identity op 改 `1024 * 1024`；V11-P1-3 处理后 unreachable 归零。同时建议 CI 固定跑 `cargo clippy -p minix-vm --lib --all-features`（与 V11-P2-5 的测试矩阵合并为一条 CI 步骤）。

**验证**：`cargo clippy -p minix-vm --lib` 与 `--all-features` 均回到 0 warnings。

#### ✅ V11-P2-4 过时注释 4 处：X86_64Paging 已实现而注释仍称 todo!()——已修复 2026-09-06（见 §15 Fix #20）

**问题**：arch 侧 `X86_64Paging::new`（paging.rs:464-485，经 `pt_alloc::alloc_pt_page` 分配 PML4 并清零）与 `destroy`（:487-497）均已实现，全文件 grep `todo!` 零命中；但 VM 侧 4 处注释仍以"未实现"为前提写理由：

| 锚点 | 注释内容 | 实际情况 |
|---|---|---|
| brk.rs:236 | "(X86_64Paging::new() is todo!(), so a zero-initialized stub is used)" | `new()` 已实现；该处测试桩的真实理由应改为"测试环境无 VM DM 窗口/未跑 pt_alloc" |
| vmproc/vmproc.rs:188 | "In test builds, skip destroy() since X86_64Paging::destroy() is todo!()" | `destroy` 已实现（:487-497） |
| vmproc/vmproc_handle.rs:352-354 | "X86_64Paging::new() is todo!() and new_from_page(0) would dereference null" | 同上；且非测试路径 `init_page_table`（:363-365）已在调用真实 `new()`，注释自相矛盾 |
| region/mod.rs:21-22 | "In test builds, page table operations are skipped since X86_64Paging methods are not yet implemented" | 方法已实现；真实理由是测试不接真实页表（与 V11-P2-1 相关） |

**影响**：误导维护者——比如会让人以为"真实 `new()` 还不能调"而继续加零桩，或低估 V11-P2-1 测试缺口的紧迫性。V11-P2-1 落地后这些注释应随测试策略一起改写。

**建议**：四处注释按"真实理由"改写（锚点如上）；改写时在 `vmproc_handle.rs:352-354` 处顺带指向 V11-P2-1 的 SimPaging 方案，说明测试桩的退出条件。属 P0-fact 族（描述与事实不符），修复成本低，可与 V11-P2-3 一次清掉。

**验证**：`rg "todo!\(\)" os/servers/vm/src` 归零；`rg "not yet implemented" os/servers/vm/src/region/ os/servers/vm/src/brk.rs os/servers/vm/src/vmproc/` 归零。

#### V11-P2-5 测试基建三条：CI feature 矩阵、rs_handshake 链路、PM↔VM 集成测试复活

**问题**（对应 G-V11-3/G-V11-4）：
1. segment_tree 后端的 15 条 parity 测试默认不执行（allocator_tests.rs:14 起 feature 门控），CI 无 feature 矩阵步骤——V10-P0-1 的"组合态无人看"风险仍在测试维度延续；
2. `rs_handshake`（vm_server.rs:900）与 `ipc_call_rs_init`（:1124-1157）零测试（tests 区 grep 命中 0），`run_once` 的 reply 编码失败分支（:663-680）与 invalid caller 丢弃分支（:652-660）也无专门测试；
3. PM↔VM 集成测试 `os/tests/pm_vm_fork{,_test}.rs` 正文整体注释停用（自注 "DEPRECATED: permanently disabled"，原因：`vmproc::fork` 等 `pub(crate)` 类型已不可从外部访问）。

**影响**：fork 主线（18-vm-fork.md）当前没有任何跨 crate 端到端验证；主循环分支覆盖有盲区；后端矩阵靠手工自觉。

**建议**：
1. CI 加一步 feature 矩阵：`cargo test -p minix-vm --lib --no-default-features --features {segment_tree_alloc,buddy_alloc}` + `cargo clippy --all-features`（与 V11-P2-3 合并）；这是一条命令的事，优先做。
2. `rs_handshake` 补 2 个测试（正常握手返回 Suspend、RS_INIT 消息分类），`ipc_call_rs_init` 在 V11-P1-1 落地前补一个 pin 测试（断言当前返回空表——防"看起来已实现"）；reply 编码失败与 invalid caller 分支各补 1 个（经 TestIpcTransport 驱动）。
3. 集成测试复活的正确形态不是恢复旧文件（它依赖的 pub(crate) 边界是刻意收紧的），而是等 V11-P1-1 落地后，在 `minix-sys` 桩层写"伪 PM 驱动 VM"的集成测试——消息层是稳定契约（minix-types wire 格式），比旧的 crate 内访问更符合边界。在此之前，在旧文件头加一行指向 V11-P1-1 的"复活条件"注释，避免下轮又被当成死代码清理掉。

**验证**：CI 配置含 feature 矩阵；`cargo test -p minix-vm --lib vm_server` 新增 ≥4 个用例；pm_vm_fork 文件头有复活条件注释。

#### ✅ V11-P2-6 文档-代码同步批次（Gate E 产物）——已修复 2026-09-06（见 §15 Fix #19）

**问题**：15-ipc-dispatch.md §5 有 3 个测试名与代码不一致（文档侧 P0-fact）+ 1 个声称的 dispatcher 层测试实际不存在：

| 文档声称 | 实际（锚点） | 处置 |
|---|---|---|
| `test_dispatch_procctl_unknown_param_returns_invalid_address` | 实际名 `test_dispatch_procctl_unknown_param_returns_einval`（dispatcher.rs:1711） | 改文档名 |
| `test_dispatch_vfs_reply_rejects_no_active_request` | 实际名 `test_dispatch_vfs_reply_no_active_request_returns_error`（dispatcher.rs:1830） | 改文档名 |
| `test_dispatch_vfs_reply_rejects_negative_reqid` | 实际名 `test_dispatch_vfs_reply_negative_reqid_returns_error`（dispatcher.rs:1851） | 改文档名 |
| `test_dispatch_setcache_rejects_zero_dev_and_ino`（声称 :1835） | 无近似命中；NO_DEV 守卫由 `page_cache::tests::test_addcache_rejects_no_device` 覆盖（dispatcher.rs:1958-1960 注释声明） | 文档改为指向 page_cache 测试，或按 V11-P2-2 精神补 dispatcher 层用例 |

**影响**：Gate E 对账失败 4/94；后续按文档名 grep 验证的人会误判"测试被删了"。

**建议**：按 fix-guard 逐条改 15-ipc-dispatch.md §5（4 处），顺带把 §5.4 的 "441 passed" 总数声明更新为 448（并注明 feature 差值）。其余 22 篇文档本轮抽查对账良好（21-vm-munmap 26/26、26-vm-queries 20/20 具名全命中），无需批次刷新。

**验证**：对 4 个新文档名逐一 `rg "fn {name}" os/servers/vm/src` 命中（或指向说明成立）。

#### V11-P2-7 P1-4 依赖精确化：VmBootHandoff 需补 kernel text/data span 字段

**问题**：P1-4（KERNEL_LAYOUT mock，vm_server.rs:437-450）的现有 DEFERRED 表述是"boot-info 接缝未闭环"，但本轮发现接缝的一半已经闭环：`boot.rs` 已完整建模 C `kernel_boot_info`（BootParams:61-103，含 `kernel_allocated` 字节总数、`vm_allocated_bytes`、`root_paddr`）。真正缺的只剩：**kernel text/data 的基址与页数**（mock 里的 `kernel_text_vbase=0xFFFF_FFFF_8000_0000`、`pbase=16MiB`、`pages=8` 这四个值）在 `minix_types::VmBootHandoff` 里没有对应字段。

**影响**：DEFERRED 理由从模糊的"接缝未闭环"收窄为明确的"handoff 布局缺 4 个字段"——依赖方（kernel vm_handoff.rs）与消费方（vm_server.rs init_global_state）可以各自立项，不用互相等。

**建议**：在 `VmBootHandoff`（kernel/src/vm_handoff.rs）增加 kernel text/data 的 `(paddr, pages)` 字段（对齐 C 侧 kernel 布局符号），kernel 侧填充、`read_boot_params` 解析、`init_global_state` 消费，三处一次改齐并加 `debug_assert`（mock 值与真实值不得共存）。在 P1-4 条目上补本条的精确依赖描述。

**验证**：`rg "kernel_text_vbase" os/kernel/src os/servers/vm/src` 出现 handoff 字段定义与解析（而非仅 mock）。

#### V11-P2-8 进程退出的中间页表页确定性泄漏（arch 侧 destroy 只清零不回收）

**问题**：`X86_64Paging::destroy`（`os/arch/src/x86_64/paging.rs:487-497`）只把根 PML4 清零，注释自述 "accept the intermediate-table leak"——因为 `pt_alloc` 只注册了 alloc 没有注册 free。即**每次进程退出，该进程的中间层页表页（PDP/PD/PT，最多 1+1+512 量级）物理页不归还**。调用链已在位：`exit.rs:190` → `vmproc_handle.rs:411-418` `free_page_table()` → `destroy()`；"页表不在任何 CPU 上活跃"的安全前提已由 `exit.rs:188-189` 的 SAFETY 注释与 `free_page_table` 的 `# Safety` 文档显式声明（契约是诚实的，本条只针对泄漏）。

**C 对照**：Minix3 的 `pt_free`（pagetable.c:1427-1437）会归还页表页——本条同时是一处 C↔Rust 语义偏差（C 回收、Rust 泄漏）， magnitude 有界（每退出 1-3 页）所以评 P2，但长期运行下与 P1-1（heap 泄漏，已修）同类。

**外部对照（本轮联网核实）**：
- Redox：进程退出路径 `AddrSpace::drop` → `inner_drop()` 逐 grant 归还数据页，`Drop for Table` 先切到 `empty_cr3()` 再 `deallocate_frame` 回收**根表帧**，中间层由 rmm `PageMapper` 的 unmap 路径归还——没有"只清零"的捷径（redox-os/kernel 镜像 `src/context/memory.rs`）。
- Linux：`exit_mmap()` → `tlb_gather_mmu_fullmm` → 逐 VMA unmap → `free_pgtables()`，页表页经 `tlb_remove_table`/`__p*_free_tlb` 批量回收，且强制"unhook → TLB invalidate → free"顺序（torvalds/linux `include/asm-generic/tlb.h` 权威注释、`mm/mmap.c:1288-1312`）。
- 两者的共同点：中间页表页是**一等公民资源**，退出路径显式回收；差异点：Linux 需要显式 TLB 顺序（SMP+PCID 环境），Redox 用 RAII Drop 让类型系统保证顺序。

**建议**：
1. **首选**：给 `pt_alloc` 注册 free（arch crate 侧小改动），`destroy` 改为完整四级遍历：逐级回收中间页并归还分配器，最后回收根帧；保持现有"先清零根"的 UAF 防护语义不变。
2. **次选**：若 arch 侧改动暂不立项，至少把 destroy 的泄漏契约从"注释自述"升级为显式 DEFERRED 标记（编号 + 指向本条），并补一个"退出 N 个进程后中间页计数不增"的 pin 测试（实现后翻转）——防泄漏规模被无感知放大。
3. 修复跨 stage 边界（arch crate 属 07-paging 域），实施时按 `[ARCH]` 流程在 doc + design + code 三处一致标注。

**验证**：`rg "fn free" os/arch/src/pt_alloc.rs`（或对应文件）非零；新增"destroy 后中间页归还"测试；长跑测试退出 N 进程后 `available_regions` 可用页不下降异常量。

#### ✅ V11-P3-1 卫生项批次（一次清掉）——已修复 2026-09-06（见 §15 Fix #21）

- `NR_VM_CALLS` 双重定义：`minix-types/src/ipc/vm.rs:149`（`pub const u32`）与 `vm_server.rs:1173`（私有 `usize`）并存——VM 侧改用 minix-types 常量做 `as usize` 转换，删除私有副本。
- `minix-types/src/ipc/event.rs:150`：doc 注释后空行（clippy `empty line after doc comment`）——相邻 crate 顺手修。
- `os/kernel/src/arch/x86_64/paging.rs:977`：unnecessary unsafe block（arch crate，V11-P2-1 触及该文件时顺手修）。
- `minix-sys` 依赖闲置（servers/vm/Cargo.toml:14）不单独删：V11-P1-1 落地后自然消解；若 P1-1 长期不动再降级为"删依赖声明"。

### 14.4 08-17 后增量代码首轮审查结论（基线确认，无需动的部分）

按"确认良好项"惯例登记本轮核过的增量（供下轮免检）：

- **boot.rs**（549 行，净 +322）：`BootParams`/`BootModule`/`KernelAllocated` 对 C `kernel_boot_info`（glo.h）+ module list（main.c:485-489）+ kernel footprint（main.c:492-495）的建模完整，`reconcile()` 内存对账逻辑清晰；grep TODO/DEFERRED/stub 零命中；12 个测试覆盖 validate 与 reconcile 主分支。
- **pagetable/vm_self_map.rs**（290 行，净 +165）：`VmSelfPageTable` 的 adopt/map/unmap/query 面正确收敛，`init_vm_self_pt` 已被 `VmServer::init` 调用（生产接线）；仅 2 处 test-only 标注（:125/:230）符合 V10-P2-1 惯例。
- **vmproc 增量**：`reset_rusage`（vmproc.rs:122，vmproc_handle.rs:469）对齐 C exit.c:25-31 的 `reset_vm_rusage`，由 `free_proc`/`clear_proc` 共享，语义正确；`test_swap_proc_slot_preserves_identities` 等 7 个新测试有效。
- **alloc_cycle 回收半边**：`free_pages(FREE_CACHE_BATCH)` 有界回收实现正确（回收跳过已映射页有测试 `test_free_pages_skips_mapped_frames`，page_cache.rs:604）；"回收后重试"缺口维持原 DEFERRED（24-page-cache 阶段），仅注释需随 V11-P2-4 微调。
- **Getrusage 修复（VMI-3）**：编码路径 vm_server.rs:1417-1418 与 `UsageInfo` 字段（query.rs:391-394）一致，负数保护（faults_to_i32）在位。
- **kernel 侧 ipc.rs**（对接面）：2,906 行实现含死锁检测（:739）与 IPC 权限检查（:1591），`sendrec`/`notify` 语义与 C 对齐；本轮仅核对接面对接充分性，深审属 01-stage-kernel 范围。
- **VmReply 大枚举不 Box 的决策**（minix-types vm.rs:645-652）：注释给出完整论证（Boxing 只添 alloc 依赖无可测收益 + 数组 transport 编码 DEFERRED 契约指向 26-vm-queries.md §3.7）——已知已登记，不重复立项。

### 14.5 对照 Redox / Linux / Rust 社区的架构参考（本轮新增，全部 URL 实际核实）

> 来源说明：Redox 官方 GitLab 被 Cloudflare 拦截，以下证据取自 GitHub 官方镜像 `redox-os/kernel` / `redox-os/syscall` / `redox-os/redoxfs`（仓库描述自证镜像关系）；docs.rs 上的同名 `rmm` crate 与 Redox 无关（Redox 的 rmm 以 `path = "rmm"` 内嵌于 kernel 仓库），不采信。

**① 物理分配器后端策略——"三后端 feature 互斥"在 OS 内核中没有先例**
- Redox rmm 的 Cargo features 只有 `std`，没有任何"编译期选分配器"机制；帧分配器只有 bump 与 buddy 两个实现（`rmm/src/allocator/frame/`），且两者是**生命周期接力**（bump 建立直接映射 → buddy 接管全部帧分配，`rmm/src/main.rs`），不是候选方案。Linux 是单一 buddy（`mm/page_alloc.c`）+ per-cpu 分层，从不提供"bitmap vs buddy"二选一。
- 对 V11-P1-3 的补充：`DefaultAllocator` 别名只是矛盾的一半；更上层的问题是 [ARCH: A-5] 的"三后端 feature 互斥"策略本身在业界无对应物——Rust 生态的 feature 门控惯例是能力开关（std/no_std、架构），不是算法策略切换。**候选方向（属 Architectural Evolution，需用户批准，不在本轮执行）**：bitmap 定为唯一生产后端，buddy/segment-tree 降级为 parity 测试参照（`allocator_tests.rs` 已证明三者可共存编译，feature 互斥层可以整体删除）。保留现状也可辩护（教学价值 + 05 文档 §3.3 的权衡论证），但"双真相源"必须先修。

**② 用户态服务器状态组织——context struct 是社区共识**
- redoxfs 守护进程主循环：全部状态在局部 `FileScheme` 结构中传递（`req.handle_sync(&mut scheme, &mut state)`），唯一全局是退出标志 AtomicU32；Redox kernel 自己也在 clippy 中压制 `static_mut_refs`（往去裸 static 方向迁移）。
- embassy 生态 `static_cell` crate 的梯队（static + 运行期 init 借出 `&'static mut` → `Box::leak` → `OnceLock`）与 `os/libs/minix-platform/src/global.rs:29-49` 对 `AssumeSyncCell` 的论证一致。
- 对 V11-P1-2 的补充：现状方向正确（主状态已走 `VmServer` 结构体），收敛终点是"全局只留启动期写一次、运行期只读的项"。

**③ 错误处理——本地中间路线成立，"thiserror 不可 no_std"是误解**
- Redox `syscall` crate 用统一 `Error { errno }` + errno 常量直传（`src/error.rs`）；本地是"每模块小 enum → 统一 `VmError` → 单一 `to_errno()` 出口"，属于社区常见模式与 Redox 极端方案之间的中间路线，且保留了 errno 语义区分文档（`InvalidEndpoint→ESRCH` vs `InvalidProcess→EINVAL`），**不建议**改成裸 errno 直传。
- 事实更正（影响 P2-2 建议 3 与 V10-P2-3 的论据）：thiserror 2.x **支持 no_std**（`core::error::Error`，`default-features = false`），"避免 proc-macro 依赖"不再是不用它的理由——V10-P2-3 启动专项时可将其纳入候选方案对比。

**④ 架构相关代码的测试策略——rmm 的宿主模拟正是 V11-P2-1 第 1 档的先例**
- rmm README 明言 "testing memory management with software emulation"：`rmm/src/arch/emulate.rs` 的 `EmulateArch` 用 std 的 Box/BTreeMap 造假机器、实现与 `X8664Arch` 相同的 `Arch` trait，`rmm/src/main.rs`（`required-features = ["std"]`）跑"真实分配 + 模拟翻译"的端到端宿主测试——**不是 mock 单测也不是 QEMU，是第三档：真实算法 + 模拟存储**。
- QEMU 档的社区惯例：blog_os 用 `isa-debug-exit` 设备带出退出码（os.phil-opp.com/testing/）；rust-osdev/uefi-rs 的 `uefi-test-runner` 在 QEMU+OVMF 中跑集成测试。
- 对 V11-P2-1 的补充：workspace 已有 `os/qemu-tests/`（test-paging-enable / test-kernel-map 等）对应 QEMU 档；缺的中间档（SimPaging/EmulateArch 式）有 Redox 一手先例，两档建议均非发明。

**⑤ IPC 大 enum——本地"先测量不 Box"的决策与 clippy 官方口径一致**
- `VmReply::InfoRegion` 内嵌 64 项数组（约 1.5 KiB）的 `#[allow(clippy::large_enum_variant)]` 有文档论证（minix-types vm.rs:645-660），与 clippy 官方"boxing 可能反而亏，先测量"的警告一致——维持原判，不立项。
- 真正的对照启示在传输层：Redox `syscall/src/data.rs` 是 per-call `#[repr(C)]` 小结构 + `Deref<Target=[u8]>` 字节视图，"线上格式"与"进程内类型"分离；本地的 `VmReply` 目前两者兼职（transport 编码是已登记的 DEFERRED 契约，26-vm-queries.md §3.7）。将来 M1 之外的消息格式落地时，V9-P2-1 的 codec 注册表应按 per-call 小结构方向设计，而非扩大统一 enum。

**⑥ 页表销毁回收——Redox/Linux 都把中间页表页当一等公民资源**
- 证据与建议已落条目 V11-P2-8（Redox `Drop for Table` + `empty_cr3()` + `deallocate_frame`；Linux `exit_mmap` → `free_pgtables`/`tlb_remove_table` 的 "unhook → invalidate → free" 强制顺序）。

**对存量条目的证据更新汇总**：V11-P1-2（+redoxfs/static_cell 佐证）、V11-P1-3（+[ARCH: A-5] 无业界先例，候选演进需用户批准）、V11-P2-1（+rmm EmulateArch 先例）、V11-P2-8（新增）、V10-P2-3/P2-2（thiserror no_std 事实更正）、V9-P2-1（codec 方向补充 per-call 小结构）。

### 14.6 Rule Discovery（Step 5.7）

本轮发现一个可注册的新检查模式：

**模式候选：DEFERRED 依据时效（stale-DEFERRED premise）**——DEFERRED 条目的"依赖未解除"论证本身会过时。本轮实证：P1-3 的阻塞依赖（kernel IPC core）在别的工作流里悄悄落地了（kernel/src/ipc.rs 2,906 行），而 VM 侧的 DEFERRED 标记、注释、文档共约 10 处仍按"依赖不存在"行事；同族还有 4 处 X86_64Paging 过时注释。与既有模式 70（CTOS：TODO 陈旧）的区别：70 查"TODO 指向的现状是否已变"，本模式查"**DEFERRED 的依赖论证是否仍成立**"——它要求跨 crate 反查依赖目标，而不是本 crate grep。

**建议落地**：在 review-process 的 Step 0.7（TODO 验证）中加一个子步骤：对目标模块的每个 DEFERRED 标记，提取其声称的依赖物（crate/模块/函数），grep 依赖物是否已存在；已存在 → 列入"解封复核"清单。本条待规则集评审后收录（prompt/review-rules/review-patterns.md）。

### 14.7 建议的推进顺序（更新）

1. **V11-P2-6（文档名 4 处）+ V11-P2-3/V11-P2-4（clippy 回归 + 过时注释）**：低成本、防误导，一次清掉；顺带把 CI feature 矩阵（V11-P2-5 第 1 项）加上。
2. **V11-P1-3（DefaultAllocator 双真相源）**：同属低风险清理，与上一步共用一次 clippy --all-features 验证。
3. **V11-P1-1（KernelIpcTransport 落地）**：本轮最重要的解封项；按三步走，第一步只接 transport 本体 + `rs_handshake`。
4. **V11-P1-2（VmContext）**：transport 可测之后做，按三小步；随后 V10-P2-3（错误枚举）与 V9-P2-1/P2-2（codec/表驱动）在同一条 dispatch 签名上一次收敛。
5. **V11-P2-1（页表测试策略）**：与 V11-P1-1 并行启动第 2 档（QEMU 冒烟），transport 落地后补第 1 档（SimPaging）；**V11-P2-8（中间页表页回收）** 与本条同属 arch 域，宜一并规划（首选方案共用 pt_alloc free 注册）。
6. **V11-P2-2（MemType/PhysAllocator 补测）与 V11-P2-5（链路测试）**：随上述各项的触碰面机会性补齐，不单独立项。
7. 原 V9/V10 未修项的优先级不变，唯两处更新：P1-2 并入 V11-P1-2 执行；V10-P2-3 的启动时机挂到 V11-P1-1 之后。

---

## 15. 第五轮修复记录（2026-09-06 起，V11 条目逐条闭环）

> 执行方式：02-stage-vm TODO 实施 campaign（顺序表见 `notes/rewrite/fork-syscall-rewrite/edge_todo.md` §0，跨 stage 条目登记于同文件 E1-E5）。
> 每条遵循：fix-guard（读目标行 ±5 + grep 现状）→ 设计对比（≥2 方案，对照 C/Redox/Linux）→ 代码+文档+测试一并改 → 回归（`cargo test -p minix-vm --lib` 基线 448 不回退 + clippy --all-features + 受影响 Gate）→ 单条单 commit。
> 通电口径：依赖共享 trap 层（minix-sys）的条目，VM 侧逻辑完备 + mock 测试即标 ✅，真实通电挂 edge E1/E2。

### ✅ Fix #19: V11-P2-6 — 文档测试名对账 + 15-ipc-dispatch.md §5 行号/计数全量刷新

- **File**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/15-ipc-dispatch.md`（§5.1 两张表、§5.3 行号、§5.4 统计块）
- **Before**: 4 个测试名与代码不符（`..._returns_invalid_address` 实为 `..._returns_einval`；`rejects_no_active_request`/`rejects_negative_reqid` 实为 `..._returns_error` 后缀；setcache 行的 `zero_dev_and_ino` 无对应测试）；§5.1 全部行号较代码漂移约 12-110 行；§5.4 声称 441 passed / clippy 0 warnings
- **After**: 4 名修正为实际名；setcache 行补 NO_DEV 守卫归属说明（由 `page_cache::tests::test_addcache_rejects_no_device` :522 覆盖，dispatcher 层不重复）；dispatcher 13 行 / vm_server 11 行行号逐一按 `grep -n "fn test_"` 刷新；补录 `test_decode_rs_memctl_unknown_req_einval / all_valid_codes`（:2121/:2131）；§5.4 更新为 448 passed（2026-09-06）+ clippy 1/7 warning 回归事实（指向 V11-P2-3）
- **Verified**: 刷新后表中 36 个测试名逐一 `grep -c "fn {name}("` 全部恰好 1 次命中；旧名 4 个在文档中 0 残留；`cargo test -p minix-vm --lib` → 448 passed / 0 failed
- **Docs**: 本条即文档修复，无代码改动

### ✅ Fix #20: V11-P2-4 — 过时注释改写（X86_64Paging 已实现，注释按真实依赖理由重写）

- **Files**: `os/servers/vm/src/brk.rs`（:235-238）、`os/servers/vm/src/vmproc/vmproc.rs`（:188-189）、`os/servers/vm/src/vmproc/vmproc_handle.rs`（:352-360）、`os/servers/vm/src/region/mod.rs`（:20-23）、`os/servers/vm/src/vm_server.rs`（`alloc_cycle` 文档注释 :569-575，§14.4 登记的附带微调）
- **Before**: 4 处注释以 "X86_64Paging::new()/destroy() is todo!()" / "methods are not yet implemented" 为由解释测试桩（实际 `os/arch/src/x86_64/paging.rs:464-497` 的 new/destroy 均已实现）；`alloc_cycle` 文档注释仍称 "replenishment body is DEFERRED"（回收半边已落地）
- **After**: 按真实依赖理由改写——测试桩的存在原因是"宿主单元测试无 VM direct-map 窗口、未注册 pt_alloc"；`vmproc.rs` 的 destroy 跳过原因是"测试桩无真实页表可清零"；`region/mod.rs` 说明测试调用方传 `None` 的机制与生产传 `pt` 的对照；三处测试桩注释统一指向退出条件（可注入 Paging，V11-P2-1）；`alloc_cycle` 注释改为"回收半边已实现（有界批回收），重试半边仍 DEFERRED 归 24-page-cache"
- **Verified**: `rg "todo!\(\)" os/servers/vm/src` → 0；`rg "not yet implemented" region/ brk.rs vmproc/` → 0；`cargo test -p minix-vm --lib` → 448 passed；clippy 默认/all-features 警告数与改动前持平（1/7，无新增）
- **Docs**: 注释即文档载体；NN-*.md 无引用这些注释文本（grep 核实），无需同步

### ✅ Fix #21: V11-P2-3 + V11-P3-1 — clippy 回归收敛（默认 1→0）+ 死代码/卫生批次

- **Files**: `os/servers/vm/src/vmproc/vmproc_handle.rs`（删 `page_table()` 只读访问器，:431-440，全仓零调用）、`os/servers/vm/src/phys_mem/buddy_alloc.rs`（删 `MAX_ORDER` 死常量 :38；`metadata_size` 改标 `#[cfg_attr(not(test), allow(dead_code))]`——测试 `make_test_metadata` 在用，与生产权威 `PhysAllocType::Buddy::metadata_size` 公式有别；`total_memory`/`free_memory`/`is_under_pressure` 补 DEFERRED 标注）、`os/servers/vm/src/phys_mem/segment_tree_alloc.rs`（删零调用的 `metadata_size` 与私有死助手 `pull_up`；`total_memory`/`free_memory` 补 DEFERRED 标注；identity op `(1*1024*1024)`→`(1024*1024)`；删失效 import `METADATA_ALIGN_PADDING`）、`os/servers/vm/src/phys_mem/mod.rs`（`as_buddy`/`as_buddy_mut` 补 DEFERRED 标注，对称于 `is_bitmap` 惯例）、`os/servers/vm/src/vm_server.rs`（`NR_VM_CALLS` 改为 `minix_types::NR_VM_CALLS as usize` 派生，消除双真相源）、`os/libs/minix-types/src/ipc/event.rs`（doc 注释空行）
- **Before**: clippy 默认 1 / all-features 7（vm_server.rs:301 unreachable、page_table 死方法、as_buddy 对、buddy/segtree 各 4-5 项、identity op）；NR_VM_CALLS 在 minix-types 与 vm_server 各一份
- **After**: clippy 默认 **0 warnings**；all-features 仅剩 `vm_server.rs:301` unreachable（属 V11-P1-3 双真相源条目，下一迭代清除）；NR_VM_CALLS 单一来源派生
- **Verified**: `cargo test -p minix-vm --lib` 三矩阵 **448 / 463 / 448 passed**；`cargo check -p minix-vm --all-features` 通过；clippy 两档实测如上
- **Docs**: 死代码删除项中 `metadata_size` 的公式差异已在代码注释说明单一权威归属；文档无引用被删项，无需同步
