# 02-stage-vm Rust 实现架构级 Review TODO

> 来源：2026-08-16 架构级代码审查（关注整体/分层架构，非逐函数审查）。
> 范围：`os/servers/vm/src/` 全部 Rust 代码（与 02-stage-vm 文档对应的实现）。
> 方法：整体分层分析（全局状态 → 主循环/分发 → 各子系统 → 模块），结合 Redox 实现与 Rust/OS 社区最佳实践。
> 定位：本文档是新增的架构改进建议清单，**不同于** `draft/TODO.md`（旧 fork 主线 TODO 存档）。
> 状态（2026-08-17 更新）：**P0-1 / P1-1 / V9-P0-1 / V9-P1-1 / V9-P3-1 / V10-P0-1 / V10-P0-2 / V10-P1-1 / V10-P1-2 / V10-P1-3 / V10-P2-1 / V10-P2-2 / V10-P2-4 已修复**（见 §8 + §10 + §13 修复记录）；V10-P2-3 与 P1-2/P1-3/P1-4/P2-\*/V9-P2-\*/V9-P3-2 为架构演进建议（DEFERRED/可选，依赖 kernel IPC 落地或需专项规划），供后续阶段参考。

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
