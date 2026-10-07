# 02-stage-vm Rust 实现架构级 Review TODO

> 来源：2026-08-16 架构级代码审查（关注整体/分层架构，非逐函数审查）。
> 范围：`os/servers/vm/src/` 全部 Rust 代码（与 02-stage-vm 文档对应的实现）。
> 方法：整体分层分析（全局状态 → 主循环/分发 → 各子系统 → 模块），结合 Redox 实现与 Rust/OS 社区最佳实践。
> 定位：本文档是架构改进建议清单，**不同于** `draft/TODO.md`（旧 fork 主线 TODO 存档）。
> **历史归档**：第一轮至第六轮（T24+ 收尾 campaign）→ [`archive/todo-V11-archive-2026-09-08.md`](archive/todo-V11-archive-2026-09-08.md)（1487 行）；第七轮 V12 全卷（§0–§17，含 Fix #59–#62 修复记录）→ [`archive/todo-V12-archive-2026-09-09.md`](archive/todo-V12-archive-2026-09-09.md)。Fix #N 编号以对应存档文件为检索权威。
> 状态（2026-09-09，V13 轮 = 第八轮，见 §18）：**V12 后复扫**。四条工作线——① V12 全部开口条目 staleness 复核（12 条全部维持开口，无一失效；漂移锚点逐一重校，见 §1）；② Gate A 覆盖穷举重跑 + 调用号级全量对账（26/26 注册 parity，真实缺口在子路径）；③ Fix #60（缺页链写 PTE）落地后的新风险面深查——PTE/TLB 生命周期、CoW 快路语义、munmap/brk/exit 资源闭环；④ VM 消息 wire 字段宽与 ACL 权限面两个系统对账。新发现：**1 项 P1（CoW 快路 memtype 不换型的活循环）+ 6 项 P2 + 1 项 P3**，跨 stage 两条挂 edge（E-VMTLB 新登记 + E-RSWIRE 增补）。本轮未修改任何生产代码。

---

## 0. 审查结论速览

### 0.1 历史轮次闭环一览（正文见两份存档）

| 轮次 | 代表条目 | 状态 |
|------|---------|------|
| 第一轮~第六轮 | P0-1 map_lazy 语义矛盾 / P1-1 全局 bump 泄漏 / P1-2 VmContext 收敛 / P2-1 表驱动分发 / P3-1 clippy 归零 + T1–T36 campaign | ✅ 全部闭环（V11 存档） |
| V11 遗留 | V11-P1-1 / V11-P2-8 / V10-P2-3 | 开口（§1，均为通电件或判定维持） |
| V12（2026-09-08） | P0 两项（G-V12-8 缺页链写 PTE、V12-P1-3 wire 字宽）+ G-V12-10 + V12-P1-2 已修（Fix #59–#62）；开口 12 条 | 修 4 / 开 12（开口见 §1，正文在 V12 存档） |
| **V13（2026-09-09）** | **V13-P1-1 + V13-P2-1..6 + V13-P3-1** | **开口（§18）** |
| 增补（2026-09-16） | V14-P2-1（VM SEF 接线预登记，会话决议非扫描轮） | 开口（§19） |

### 0.2 V13 轮速览（本轮新增）

| 级别 | 条目 | 一句话 |
|------|------|--------|
| **P1** | **V13-P1-1** | CoW 快路 refcount≤1 只翻写位不换 memtype：MappedFile 槽位写故障 RO 原样成功 → 活循环（C cow_block 显式换 anon，§18.2）（**✅ 已修复** 2026-09-09，§18.9 Fix #63） |
| P2 | V13-P2-1 | TLB 纪律三缺口：不改 PTE 的进程运行状态不变量全隐式 + C VM 自刷四处（别名模型）无 Rust 对应且偏差未登记 + 内核 FlushTlb/InvlPg 命令零消费者（§18.2；SMP 半边挂 edge E-VMTLB） |
| P2 | V13-P2-2 | `RprocEntry.call_mask: u32` 在 E-RSWIRE 落地时必截断 wire 的 u64 掩码——+32..+48 的 9 个调用永远无法授权（§18.2） |
| P2 | V13-P2-3 | ACL 闸 fail-open：`get_active(caller)==None`（EXITING 等）时整个 acl_check 被跳过（§18.2） |
| P2 | V13-P2-4 | MAKE_VM 恒拒是有意设计收缩，但无任何偏差登记——E-RSWIRE 后可达，需文档化豁免（§18.2） |
| P2 | V13-P2-5 | RS_SET_PRIV 的 call_mask 硬编码 None：C 走 sys_datacopy，M2 wire 无法表达，safecopy 可用前权限下发链断裂（§18.2） |
| P2 | V13-P2-6 | 五个 C 消息结构（getphys/getref/info/rusage/update）无 Rust 专属 struct，overlay 解码字段语义与 C 错位 + 12 个 VM payload 仅 1 个有 56 字节断言（§18.2） |
| P3 | V13-P3-1 | VM_PAGEFAULT 伪造源检查仅 debug_assert，release 零观测（C release 保留 printf）；exit 不清 vfs_queue 挂起请求（无按 owner 取消 API）（§18.2） |
| edge | E-VMTLB（新） | kernel 侧目标进程 TLB 刷新机制（C MF_FLUSH_TLB）缺失 + 同根跳过优化耦合——真 SMP 正确性前提 |
| edge | E-RSWIRE 增补 | ACL u64 掩码对账（V13-P2-2）+ RS_SET_PRIV 真掩码（V13-P2-5）随 wire 工作面一并定稿 |

验证命令（2026-09-09 实测基线，与 V12 修复批次后记录 490/507/490 一致，无回归；Fix #63 后基线见 §18.9）：
- `cargo test -p minix-vm --lib`：**490 passed / 0 failed**
- `cargo test -p minix-vm --lib --no-default-features --features segment_tree_alloc`：**507 passed / 0 failed**
- `cargo test -p minix-vm --lib --no-default-features --features buddy_alloc`：**490 passed / 0 failed**
- `cargo clippy -p minix-vm --lib`：输出 18 条 warning 行**全部来自依赖 crate**（`grep -A2 '^warning' | grep -c 'servers/vm'` = 0），servers/vm 自身 0 警告

---

## 1. 存量 open 条目（V11/V12 遗留，2026-09-09 逐条复核）

> 复核方法（Step 0.7 staleness check，模式 70 CTOS）：每条 grep 现状 + 重读关键行。结论：**12 条全部真实开口、无一条失效**；V12 轮后代码增量（Fix #59–#62）使部分行号漂移，本节为重校后的锚点。条目正文与判定过程见 V12 存档 §17。

### V11-P1-1 【解封】KernelIpcTransport 落地路径（通电挂 E1/E2）

VM 侧逻辑完备（T9 三步全落地），rproctab 字节解码余件挂 E-RSWIRE。**剩余工作只有"真实通电"**，VM 侧无待办。验证：E1/E2 落地后 QEMU 冒烟（edge E5）。

### V11-P2-8 进程退出的中间页表页回收（余件）

x86_64 与 riscv64 destroy 回收已完成（E4 主体）。余件：(a) aarch64 同型回收；(b) VM/kernel 侧 `register_free` 接线。仍挂 edge E4。

### ⏸ V10-P2-3 DEFERRED 判定（错误枚举收敛，判定维持闭合）

不做大规模 errno 重构；`From<模块错误> for VmError` 集中表即最优。V12 新发现两处残余并入本条跟踪：`vir_region.rs:428` 第二个同名 `VmError`（单变体 `InvalidParam`）与 `CacheError`/`VfsQueueError` 无 errno 收敛路径（dispatcher.rs:1664/:1812 直接外泄 `VfsQueueError`）。复核 ✅ 2026-09-09（两处锚点均在）。

### ✅ G-V12-7（P1）`SharedMemory` 无 `ev_delete` 覆写：源区域 `remaps` 永不递减——已修复 2026-09-09（§18.9 Fix #64）

复核 ✅ 2026-09-09：`memtype.rs:34` 默认 `ev_delete` 为空实现；memtype.rs:1178 的非空 `ev_delete` 属于 **MappedFile**（清 fdref），SharedMemory 仍走默认空实现。全仓 `remaps` 写点仅 fork 复制（fork.rs:102）与递增（vmproc/table.rs:481 一带）。修复见 §18.9 Fix #64（落点设计变更：递减由删除漏斗承担而非 ev_delete，理由见 12-memtype.md §3.7）。

### ✅ G-V12-9（原 P1-design-missing）VM inhibit 机制无实现——勘误闭单 2026-09-09（§18.9 Fix #77）

复核 ✅（勘误）：登记时的 C 锚点不存在——main.c:196/:265-267 全文无 inhibit 门，全 C VM 源的 inhibit 只有 `pt_writemap` 内 CONFIG_SMP 门控的 `VMCTL_VMINHIBIT_SET/CLEAR` 包裹（pagetable.c:799-815/:928-934，每次 PTE 写批次的动态停等）。该机制 08-pagetable-ops.md（:380 + 差异表第 10 行）**早已登记**为执行模型差异；它与 16-pagefault.md §3.7 的结构化不变量互为同一保证的两种策略，E-VMTLB 立项时 VMINHIBIT 是候选方案。V12 提议的"25 偏差表新增行"不添加（前提不存在）。

### ✅ V12-P1-1（P1）三分配器对低内存约束行为不一致 + 后端选择不可观测——已修复 2026-09-09（§18.9 Fix #65）

复核 ✅ 2026-09-09：`buddy_alloc.rs:261-266` 顶层 `pop_free` 仍不检查 `max_page`；`choose_allocator_type` 与 `relocate` 仍无审计日志。修复见 §18.9 Fix #65。

### ✅ V12-P2-1（P2）模块边界：cache 业务内联在 dispatcher + reply 编码住在 vm_server——已重构 2026-09-09（§18.9 Fix #78）

两段纯移动重构完成：reply 编码三件（VmReplyForIpc/reply_to_errno/encode_reply_data + 3 测试）迁 `ipc/encode.rs`；cache 四 handler + `unmap_region_pages` 迁 `ipc/cache_handlers.rs`（固有 impl，CALLMAP/测试零改动）。dispatcher 2610→2376 行。**设计偏离说明**：V12 提议"下沉 page_cache.rs"未采纳——handler 消费整个 VmContext，放数据结构模块会倒置分层（page_cache → vm_server），ipc/ 与 dispatcher 同向无反转；C 把 do_mapcache 放 mem_cache.c 依赖的是全局数组，Rust 模块边界必须尊重所有权。明细见 §18.9 Fix #78。

### ✅ V12-P2-2（P2）`memtype` 反向依赖 `vmproc`——判定闭合（C-parity，维持现状）2026-09-09（§18.9 Fix #76）

复核 ✅：该依赖方向是 MINIX3 自己的架构形状——C `memtype.h:18/:21/:22` 的回调签名直接收 `struct vmproc *vmp`（策略钩子合法地需要跨进程查找与调用方内存访问）。Rust 的 `&VmProcTable`/`&mut ActiveProc` 显式传参是同形翻译且更干净。memtype.rs:7 已加判定注释。详见 §18.9 Fix #76。

### V12-P2-3（P2）错误类型两处残余（并入 V10-P2-3 收敛面）

见上文 V10-P2-3 条（已含重锚点）。

### ✅ V12-P2-4（P2）死状态/死链路批（逐项"删/接线/标注"三分处置，对照 T19 先例）——已处置 2026-09-09（§18.9 Fix #70）

V12 原表（重校后逐项处置见 §18.9 Fix #70）：
| 锚点 | 内容 |
|---|---|
| global.rs:16/:47 | `TOTAL_PAGES` 写点仍仅 init，生产读者为零 |
| mmap.rs:137/:142 | `FILEMAP_ENABLED` 生产写者缺位（`set_enable_filemap` 无生产调用方） |
| vm_server.rs:1666/:1709 | `VmReply::ExecNewmem` 死编码臂（无生产构造者） |
| vm_server.rs:802 | `mark_alloc_failure` → `missing_spares` → `alloc_cycle` 生产链悬空（:152-171 注释契约与现实不符） |
| vir_region.rs lazy 家族 | `map_lazy`/`Reserved` 态/`get_slot_any` dead_code（P0-1 修复遗留） |
| vmproc_handle.rs ExitingProc | `slot`/`endpoint`/`flags`/`regions` 四死访问器 |
| vmproc.rs `VmProc::check()` | debug 不变量检查零调用 |

处置（删/接线/标注三分）与验证见 V12 存档 §17.2。

### ✅ V12-P2-5（P2）不变量双轨：proc_table 双访问路径 + page_frames 双处置——已判定收尾 2026-09-09（§18.9 Fix #73）

复核 ✅ 2026-09-09：`VmContext.proc_table: &'static VmProcTable` 本身就是全局的别名——"双路径"是**语法不一致而非双真相源**；且原锚点（rs.rs 六处）全部位于 `#[cfg(test)]` 区，生产路径早已走 ctx。生产区 `get_global` 残留仅 vm_server.rs boot 路径三处（已统一为 `self.ctx.proc_table`）与 munmap.rs 漏斗一处（T2 有意例外）。page_frames 双处置为生命周期阶段差异，两者各自正确。判定详情见 §18.9 Fix #73。

### ✅ V12-P2-6（P2）mapcache 失败回滚顺序与 C 相反——已勘误收尾 2026-09-09（§18.9 Fix #74）

复核 ✅：V12 的前提不成立——C 的 do_mapcache 在 miss 时**同样** `map_unmap_region` 整区回滚（mem_cache.c:155-157），两侧顺序同构（区域先建 → 逐页链接 → 失败回滚）。Rust 的"表外组装、提交时插入"回滚面反而更小。真实价值 = V12 建议的中途失败注入测试，已补（`test_dispatch_mapcache_mid_failure_rolls_back_refcounts`，refcount 对称性定格）。勘误与差异表见 24-page-cache.md §3.6 第 10 行、§18.9 Fix #74。

### ✅ V12-P2-7（P2）`ContiguousAnonymous::ev_new` 质量债——已修复 2026-09-09（§18.9 Fix #79）

`PfnAllocator::alloc_contiguous(count)` 落地（默认实现 = 原逐页验证回滚，`VmPageAllocator` 覆写 = 单次 `alloc_phys(count)` 漏斗请求），`ev_new` 改单调用，windows(2) 验证与自相矛盾注释删除。测试还挖出 `active_allocations` 的**事件/页粒度记账不对称**（多页 alloc 后页粒度 free 会下溢——预先存在，非本次引入）。明细见 §18.9 Fix #79。

### ✅ V12-P2-8（P2）文档-代码同步批（注释 C 锚点漂移）——已修复 2026-09-09（§18.9 Fix #71）

V12 原表三处 + 复核补充两处；fork.rs "do_fork.c" 一项复核**已准确**（pt_new ENOMEM 确在 do_fork，fork.c:70-71，无需改）。修复明细见 §18.9 Fix #71。

### ✅ V12-P2-9（P2）region 两处：find_overlap 线性扫 + 零长区间静默替换——已修复 2026-09-09（§18.9 Fix #72）

复核 ✅（文件已迁 region_map.rs）：`find_overlap` 自 BTreeMap 首端线性扫 + 零长 insert 静默替换均确认存在。修复见 §18.9 Fix #72。

### ✅ G-V12-11（P2）页缓存域两笔语义债——已闭环 2026-09-09（§18.9 Fix #80）

- **第 1 笔（ONCE 条目统一走 NeedVfsIo）**：已作为偏差登记（24-page-cache.md §3.6 第 6 行）——端状态等价、差一次 IPC 往返，维持登记不修（消费语义依赖 transport，E-VFSWIRE 批次）。
- **第 2 笔（clearend 清零未建模）**：✅ 修复——MappedFile 命中分支对尾页（roundup(offset+clearend) ≥ length）走内联 `cow_block(…, clearend)` 等价物：私有 anon 拷贝 + 尾部清零（`copy_page_and_zero_tail`），非尾写维持 NeedCow、非尾读维持直链。C 对照 mem_file.c:131-134/:73-79。明细见 §18.9 Fix #80。

### ✅ G-V12-12（P2 doc）00/99 骨架文档 + design 快照缺失——已闭环 2026-09-09（本轮）

两篇骨架改写为全文（00-vm-overview 100 行：概念/源码地图/阅读路线/实施现状/接缝注入；99-global-concepts 74 行：调用号族/旗标族/哨兵类型化/glo 显式化，全部对账表形态）；design 快照六件套（00/99 × outline/outline-review/design v1）按 Step 0.3 从 C 源与现状代码独立推导落地。**design-coverage-check 复跑：28/28 全部 PASS（H.1/H.6 消除）**。形态对齐 pm 工作流（f3f6c456e）。

### ✅ G-V12-13（P2 doc）checklist.md 系统性刷新——Phase 1/2/3/4 完成（89f7eac1f + cea9abead + bcae8a9d9 + 本轮，2026-09-09）✅ 闭环

- **Phase 1**：横幅 + §0 系统化替代口径 + 七行直接证据修正。
- **Phase 2**：§6 IPC 表整表重写（26+4+3 证据口径）+ §2 过时行修正。
- **Phase 3**：§4 函数表 173 行脚本化 grep 对账，28 行硬过时修正（归属/PFN 内联/同步子集三分）；其余 145 行主符号全部命中现状代码。
- **Phase 4（本轮）**：§3 结构体表抽查通过（S-013/S-015 的 PFN 删除登记与 Fix #70 一致）；§5 memtype 矩阵**重导出**（脚本解析 impl 块真实覆写集：6 类 × 18 方法位，未覆写全部属 C-NULL parity 或能力位默认——旧表"7 stubs"口径作废）。§7 设计差异表、§8 历史编号说明维持。
- **结论**：G-V12-13 闭环。checklist 的现状口径 = 横幅（2026-09-09）+ 六张重写/注记表 + 175 项语义判定（todo §17.1）。后续任何行级再漂移由常规 review 的锚点纪律承接，不再单列批次。

### ✅ V12-P3-2（P3）可观测性与卫生批——③①已处置 2026-09-09（§18.9 Fix #81）；②复核免修；④维持

- **③ transport pub 收窄**：✅ 六类型（IpcStatus/IpcError/IpcTransport/KernelIpcTransport/TestIpcTransport/TestTransportHandle）`pub → pub(crate)`——lib.rs 本就无再导出，全 crate 内消费。
- **① audit 宏注释**：✅ 双宏各加文档——release 可观测性由 VmContext 计数器承载（pagefault_errors/dropped_messages/alloc_failures）。
- **② memtype 6 pub fn 文档**：复核已达标（免修）。
- **④ bitmap lastscan**：维持登记（优化提示，非缺陷）。
**P3-1（Redox 对照增强集）维持登记**：零帧批量预映射/Provider 五分类/find_free 双索引均为增强候选，需独立设计决策，不适于 campaign 内顺手做。

---

## 18. 第八轮（V13 轮，2026-09-09）：Fix #60 后 PTE/TLB 生命周期深查 + 调用号/wire/ACL 三面全量对账

> 范围：`os/servers/vm/src/` 全部 49 个 .rs + C 对照面 `minix3/minix/servers/vm/` + `minix3/minix/kernel/`（TLB 链）+ `minix3/minix/include/minix/ipc.h`（wire 面）。
> 方法：① Gate A 覆盖穷举重跑（371 C 符号）；② V12 开口 12 条逐条 staleness 复核（§1）；③ 三个定向系统对账——调用号级（C main.c CALLMAP 26 项 vs Rust dispatcher CALLMAP 26 项）、VM 消息族字段宽（ipc.h vs minix-types 逐结构逐字段）、ACL 权限面（C main.c:168 单闸门 vs Rust vm_server.rs:1266 一带）；④ Fix #60 落地后的故障链复核（cow_resolve_core 快/慢路、munmap/brk 的 PTE 闭环、exit 资源闭环、fault 消息防御、TLB 失效链 C 对照到底）。本轮未修改任何生产代码。

### 18.0 Step 0 预检与 Gate A

- Step 0 预检：design-coverage-check 复跑——26/28 篇三件套 PASS，00/99 仍缺（维持 G-V12-12）。
- Gate A：`coverage-extract.py vm … --output .review/claude/vm/v13/SYMBOLS.md`——371 C 符号，doc covered **92.5%**（V12 轮 91.9% → 文档增量），Rust name-match 11.6%（低估，语义判定以 V12 存档 §17.1.1 的 175 项为准，本轮无新增完全缺口）。

### 18.1 查漏结论：调用号级完全 parity，真实缺口在子路径

调用号对账（两表逐项对齐，证据在 gate-evidence）：C main.c:536-575 CALLMAP 注册 26 条 ↔ Rust dispatcher.rs:975-1000 注册 26 条，**集合相等**；`VM_EXEC_NEWMEM/ADDDMA/DELDMA/GETDMA` 4 条 C 本就无 handler（grep 全仓零命中）、两侧同答 ENOSYS（V11/T19 已定论，勿再"接线"）；`NR_VM_CALLS=49` 双侧一致且 Rust 已单源化（vm_server.rs:1576 与 dispatcher.rs:959 均引 `minix_types::NR_VM_CALLS`——V11 时代的双定义已消除）。Rust 侧没有注册任何 C 没有的调用号，`test_callmap_registration_matches_c` 守护在位。**真实缺口全部是调用号内部的子路径**，见 18.2 条目。

### 18.2 本轮新条目

### ✅ V13-P1-1（P1 潜伏活循环）CoW 快路 refcount≤1 只翻写位、不换 memtype——MappedFile 槽位写故障将无限再故障——已修复 2026-09-09（§18.9 Fix #63）

- **Rust 现状**：`cow_resolve_core` 快路（cow_exec_pf.rs:268-273）在 `refcount <= 1` 时只调 `sync_slot_pte` 后原样返回旧 pfn。`sync_slot_pte`（:91-95）以 `region.is_page_writable` 定权限位，而 MappedFile 的 `writable()` 恒 false（memtype.rs:995-997，注释自述 "Always `false`"）→ 快路对该槽位**写出的仍是 RO PTE**，并返回 `Ok(old_pfn)`——VM 记账"成功"、内核清 RTS_PAGEFAULT、指令重执行、同址再故障：活循环。慢路正确：:282 `region.map_page(frames, offset, new_pfn, &MEM_TYPE_ANON)` 有换型。
- **C 行为**（Ground Truth）：C 的 refcount 快路只存在于 anon 型内部——`anon_pagefault`/`anon_writable`（mem_anon.c:105-113，refcount 判定是 anon_writable 语义的一部分）；mappedfile **没有** refcount 捷径，写故障恒走 `cow_block`（mem_file.c:127-130/:164），且 `cow_block` 显式 `ph->memtype = &mem_type_anon` 并注释 **"After COW we are a normal piece of anonymous memory"**（mem_file.c:70-71）。即 C 语义：CoW 之后的页就是 anon 页，与原 memtype 的可写性无关。
- **后果与可达性**：MAP_PRIVATE 文件映射的页在成为"独占引用"（缓存项被逐出、或共享方全部退出后 refcount 归 1）时，首次写故障即触发。当前不可达（无真实用户进程），E1/E2 通电后即成 G-V12-8 同族的活循环——且更隐蔽：测试若只断言 `PagefaultAction::NeedCow` 与返回 Ok，从不断言 PTE 权限位与 memtype 终态，则全绿通过。
- **修改方案**（≥2 候选）：A（选定建议）——快路加门：`refcount <= 1 && region.is_page_writable(frames, offset)` 才走翻位捷径，否则落入慢路（拷贝 + 换 `MEM_TYPE_ANON` + sync）；单点修复、语义与 C 对齐。B——把快路下沉进 `AnonymousMemory` 的 ev_pagefault 判定（完全复刻 C 的分型结构），改动面大且 Rust 已无 per-phys_region memtype 结构，否决。
- **验证**：新测试——MAP_PRIVATE 文件区、独占页（refcount==1）写故障 → 断言 PTE 变 RW、slot memtype 变 ANON、二次故障不再发生（SimPaging 可完全驱动）；`rg "refcount <= 1" os/servers/vm/src/cow_exec_pf.rs` 处伴随 is_page_writable 门。
- **边界**：与 G-V12-11（clearend 未建模）交叉——慢路补 clearend 分支时（C mem_file.c:73-79 在 cow_block 内清尾页）一并设计，勿两次打开同一函数族。

### ✅ V13-P2-1（P2）TLB 纪律三缺口：承载 Fix #60 的隐式不变量 + 未登记的 ARCH 偏差 + 死内核面——(a)(b) 已处置 2026-09-09（§18.9 Fix #69），(c) 挂 edge E-VMTLB

> **勘误（Fix #69 复核）**：扫描称"ARCH 偏差未登记"不准确——08-pagetable-ops.md §1.8 已完整论证（且揭示扫描遗漏的关键事实：`write_pte_dm` 每次 PTE 写入后立即 invlpg，写与失效绑定）。真实残余 = 不变量登记（本条 a）+ 内核死面注释（b）+ SMP 机制缺失（c，维持 edge）。

Fix #60 让 VM 直接写进程硬件 PTE，随后必然面对"C 侧如何保证 TLB 不用旧值"的问题。本轮把 C 的完整机制挖到底，结论分三半：

- **(a) 不变量全隐式（文档债，先补文档）**：整条链的安全性依赖"**VM 只修改当前未在运行的进程的页表**"——故障路径目标带 RTS_PAGEFAULT（不可运行）、munmap/brk/mmap 的调用方阻塞在 IPC 等回复、fork 的父进程阻塞在 PM、exit/RS pin 的目标未运行。这条不变量在 C 里由相同的 IPC 阻塞结构保证，但 minix-rs 的 16-pagefault.md / 02-vmproc-struct.md 均未把它写成显式不变量。sync_slot_pte、munmap/brk 的 pt.unmap、fork 的 write_page_table_mappings 全部押在它上面；未来任何"对运行中进程做 map/unmap"的新路径（如 RS live-update 的 share_mappings）若不自觉违反即静默内存腐坏。**处置**：在 16-pagefault.md（或 02-vmproc-struct.md）显式登记该不变量 + 各路径为何满足它。
- **(b) C VM 自刷四处无 Rust 对应（ARCH-EVOLVED，需登记偏差）**：C VM 在 pagetable.c 四处调 `sys_vmctl(SELF, VMCTL_FLUSHTLB)`——宿主函数分别是 `pt_assert`（:119，SANITYCHECKS 调试）、`vm_freepages` ×2（:255/:319）、`vm_pagelock`（:430，MEMPROTECT 死码）。前两类存在的根因是 C 的"把进程内存/页表**别名映射进 VM 自己的地址空间**"模型（vm_freepages 释放的就是 VM 自己的别名映射），释放别名后必须自刷。minix-rs 用 direct map（VA=物理+固定偏移，翻译恒定）消除了别名模型 → 自刷确实不需要。但：① 该偏差未在任何文档登记；② 内核已实现 `VMCTL FlushTlb`/`InvlPg`/`GetPdbr` 三命令（os/kernel/src/syscall.rs:2177-2202 一带，TlbArch 三架构齐备），而 VM 的 KernelGateway 零调用（`rg "invlpg|vmctl" os/servers/vm/src` 仅 memreq/clear_pagefault/set_addrspace 三族）——"内核实现了命令、唯一合法消费者永不调用"的死面需注释锚定（dead-until-needed，防后人误判为漏接）。**处置**：08-pagetable-ops.md（或 07）补偏差行；syscall.rs 对应命令处补 "VM 侧无消费者：direct map 模型下无需自刷，对照 C pagetable.c:119/255/319/430" 注释。
- **(c) SMP 目标进程刷新缺失（kernel 侧，挂 edge E-VMTLB）**：C 内核对"其他 CPU 上目标进程的陈旧翻译"有显式机制——`MF_FLUSH_TLB` 进程标志 + 调度点刷新（minix3/minix/kernel/proc.c:345-347 `if (p->p_misc_flags & MF_FLUSH_TLB && ptproc == p) tlb_must_refresh = 1`）。Rust 内核无任何对应物（`rg "MF_FLUSH_TLB|tlb_must_refresh" os/kernel/src` 零命中），且调度器 switch_address_space 带"同根跳过重载"优化（syscall.rs:2410-2417 注释自述 mirror 比对语义）——两者叠加在真 SMP + 共享页（CoW 父子、shm）场景下有陈旧翻译窗口。当前单核无影响；登记 edge，随 01-stage-kernel SMP 工作处置。

### V13-P2-2（P2，潜伏 wire 消费 bug）`RprocEntry.call_mask: u32` 必截断 64 位调用掩码

- **证据**：minix-types 的 wire 已是正确 64 位——`RprocPubWire.vm_call_mask: u64`（os/libs/minix-types/src/ipc/rprocpub.rs:89，注释明示 2×u32 chunks little-endian 合并）。VM 内部表示是 `RprocEntry.call_mask: u32`（vm_server.rs:1594-1599），消费点 `AclMask::from_bits_truncate(entry.call_mask as u64)`（vm_server.rs:1340）。E-RSWIRE 落地时 `ipc_call_rs_init` 解码 rproctab 必经 wire→RprocEntry 转换——u64→u32 截断使调用号 +32..+48 的 9 个调用（VM_INFO/RS_UPDATE/RS_MEMCTL/REMAP_RO/PROCCTL/VFS_MMAP/GETRUSAGE/RS_PREPARE 等）**永远无法经握手授权**（掩码高位丢弃）。
- **类型**：潜伏 bug——当前 `ipc_call_rs_init` 诚实返回 NotImplemented（vm_server.rs:1540 一带，E-RSWIRE 门控）故不可达；与 V12 轮 pattern 84 候选（wire 字宽逐字段对账）同族，但截断点在 VM 内部类型而非 wire 本身。
- **修改方案**：`RprocEntry.call_mask: u32 → u64`（随 E-RSWIRE 解码落地一并做，避免先写错再改）；`from_bits_truncate` 的 `as u64` 随之消失。**验证**：E-RSWIRE 翻转 pin 测试时补"高位调用号（如 VM_RS_PREPARE=+48）授权后可过闸"的用例。
- **edge 联动**：edge_todo.md E-RSWIRE"解锁后工作"第 3 步已增补此要求（2026-09-09）。

### ✅ V13-P2-3（P2 防御缺口）ACL 闸 fail-open：caller 槽非 active 时整个检查被跳过——已修复 2026-09-09（§18.9 Fix #66）

- **证据**：vm_server.rs:1268-1271——`if let Some(proc) = table.get_active(caller_slot) && proc.acl_check(c as u32).is_err() { …拒绝… }`。`get_active` 对 IN_USE+EXITING 等状态返回 None（vmproc/table.rs:224-235）→ ACL 检查整体跳过、调用直达 handler。
- **C 对照**：C 的 vmproc 槽永远存在（未注册进程落 NO_ACL → allow-all，acl.c:44-54 自注 "for now"），所以 C 没有"跳过检查"这个状态；Rust 已显式选择 default-deny（acl.rs:97-112，[ARCH: A-11]），闸门形状却保留了 fail-open 分支——策略与机制不自洽。当前实际风险与 C 的 NO_ACL 放行等价（非放权漏洞），但方向应统一。
- **修复**：见 §18.9 Fix #66。

### ✅ V13-P2-4（P2 设计登记）MAKE_VM 恒拒：有意收缩未登记——已处置 2026-09-09（§18.9 Fix #68，含扫描勘误）

- **证据**：C 有真实现——`rs_memctl_make_vm_instance`（minix3/minix/servers/vm/rs.c:218 定义；EPERM 仅在 `num_vm_instances == 2` 时返回，首个额外实例**会成功**）。Rust 无条件 `Err(RsError::MakeVmFailed)`→EPERM（rs.rs:504-507）。可观察行为分叉真实。
- **勘误（fix-guard 复核）**：V13 扫描称"任何 todo/文档都没有登记"**不准确**——25-rs-services.md 已把 MAKE_VM 登记为 **A-8 缺口**（§3.9/:440/:477 差异表 ❌ 行）。真实残余只有两小件：rs.rs 的拒绝注释没有指向该偏差行（后人无从发现登记）、todo 侧无豁免条目。
- **处置**：rs.rs:504 注释改为指向 25-rs-services.md §3.9/§3.10 + C 行号锚点（✅）；本条即 todo 侧登记（✅）。

### V13-P2-5（P2 权限链断裂，fail-closed 方向）RS_SET_PRIV 的 call_mask 硬编码 None

- **证据**：C 用 `sys_datacopy` 从 RS 地址空间拷入调用者提供的掩码（minix3/minix/servers/vm/rs.c:41-56）。Rust 解码层硬编码 `let mask = None`（dispatcher.rs:1091-1102，注释自述 M2 无法承载）；`handle_rs_set_priv` 对系统进程 + None 报错（rs.rs:146-148 一带）→ RS 运行时无法给系统服务授予特权掩码，live-update 后服务拿不到 C 语义的 ACL。方向是 fail-closed（偏紧），非放权漏洞。
- **修改方案**：依赖 E2（SYS_SAFECOPYFROM 用户态包装）可用后按 C 语义补真值——M2 的 `m2l1` 是掩码缓冲指针，语义即"从 RS 进程 safecopy"。**登记在 E-RSWIRE/E2 协同面**（edge_todo E-RSWIRE 增补第 2 点），本条为 VM 侧活指针。

### V13-P2-6（P2 结构缺失族）五个 C 消息结构无 Rust 专属 struct：overlay 解码与 C 字段语义错位 + size 断言缺口

- **证据**（逐结构对照 ipc.h，完整表见 gate-evidence）：
  - `mess_lc_vm_getphys`（C：endpt@0、addr@4、ret_addr@8）与 `mess_lsys_vm_getref`（同布局）——Rust 无专属结构，经 `m_m1` overlay 解码读 `m1p1@16`（dispatcher.rs:1075-1089），回复写 `m1p1@16`（vm_server.rs:1721-1722）。
  - `mess_lsys_vm_info`——C 的 `next` 游标@16（procfs 分页遍历依赖）、`ptr`（safecopy 指针）@12；Rust 读 `m2l2@24`，`ptr` 无承载（结果改走回复消息编码）。
  - `mess_lsys_vm_rusage`——C 是 endpt@0、addr@4、children@8；Rust 把 @4 槽读成 children（dispatcher.rs:1166-1172）。
  - `mess_lsys_vm_update`——偏移恰好一致（src/dst/flags 三个 i32），但同样无专属结构。
- **判定**：minix-rs 的两端（Rust 内核/libc 侧未来统一用 minix-types）自洽，**当下不是通信 bug**；错位意味着"按 C libc 语义写 minix-sys 消费方时会接错字段"——与 V12 已修的 vmmcp reply 同族风险，且丢掉了 C "一结构一 `_ASSERT_MSG_SIZE` 断言"的防线（12 个 VM payload 结构仅 MessVmmcpReply 有 56 字节断言，message.rs:3606）。
- **修改方案**：按 `minix-types::ipc::rs_start`/`rprocpub` 的既有样式（Fix #81/#85 模板）为五结构补专属 wire struct + `size_of`/`offset_of` 断言；解码函数从 overlay 切到专属结构。顺带补齐其余 VM 结构缺失的 56 字节断言（D5）。**时点**：可与 E-RSWIRE/minix-sys 消费定稿同批（edge E-RSWIRE 增补第 3 点），纯单测可先行。

### ✅ V13-P3-1（P3 观测与卫生批）之 1——已修复 2026-09-09（§18.9 Fix #67）；之 2 挂 E-VFSWIRE

1. **VM_PAGEFAULT 伪造源检查仅 debug_assert**：C 对非内核来源的故障消息 release 也 printf（main.c:154-157，之后仍处理）；Rust 仅 `debug_assert!(rcv_sts.is_from_kernel())`（vm_server.rs:1245-1248 一带）→ release 下零观测。~~修复：补一行 `audit_log!`~~ ✅ Fix #67。
2. **exit 不清 vfs_queue 挂起请求**：`VfsRequestQueue` 无按 owner 取消 API（vfs_queue.rs 全部 pub fn 清单无 purge/cancel）；进程退出后其 FdIo/FdLookup 请求留队。VFS wire 未通（E-VFSWIRE）前无实际后果；wire 落地时 `handle_reply` 需要死进程路径兜底（对照 C do_vfs_reply 的 vm_isokendpt 检查）——挂 E-VFSWIRE 时点对账，暂不立项。

### 18.3 复核认定无缺口的面（防重复扫描，本轮已查）

| 面 | 结论 |
|---|---|
| munmap 的 PTE 生命周期 | **闭合**——四个拆分臂都把 `page_table_mut()` 传入 `free_region_pages`（munmap.rs:216/:238/:261/:276），后者逐页 `pt.unmap`（region/mod.rs:31-36）再 `free_range` 归还物理页；部分区间经 head/middle/tail 拆分等价 C pt_unmap |
| brk 收缩 | **闭合**——shrink 路径同样经 `free_region_pages(…, pt, …)`（brk.rs:166/:190-191） |
| exit 资源闭环 | 页表整树 destroy + 物理页归还 + fdref deref 入队（exit.rs:54/:86-142）；余件仅 E4 的 aarch64/register_free（§1） |
| fault 消息对死进程的防御 | `dispatch_pagefault` 先 `vm_isokendpt` + `get_active` 双检（vm_server.rs:1399-1408），失败回 InvalidProcess |
| MappedFile `writable()==false` | **C parity**——C mem_file.c:173-175 "We are never writable"，不是缺口（V13-P1-1 的问题在 CoW 快路不换型，不在此处） |
| CoW 慢路 | 拷贝 + `MEM_TYPE_ANON` 换型 + sync 三步与 C cow_block 语义一致（cow_exec_pf.rs:276-285） |
| 调用号注册面 | 26/26 parity + 双侧同不注册 4 条 + 单源 NR_VM_CALLS（§18.1） |
| 已知 wire 64 位化三处 | vmmcp_reply.addr（Fix #59）、m_vm_pagefault 专属结构（写入方是自有内核，自洽）、m_lc_vm_brk（发送端 memset 归零约定）——均为文档化 ARCH 决策，勿重复报 |
| panic 语义 | `panic = "abort"`（os/Cargo.toml:260/:263）+ run 循环仅两处 fail-fast panic（transport 永久损坏 vm_server.rs:991 / ipc_send 失败 :1069）——VM 崩溃即系统冻结，与 C"VM 不可重启"等价；boot 期 expect 与 C boot panic 同族 |

### 18.4 edge 增补指针（本轮登记，详见 edge_todo.md）

| edge 编号 | 内容 | 对应本文件条目 |
|---|---|---|
| **E-VMTLB**（新） | kernel 侧目标进程 TLB 刷新机制（C MF_FLUSH_TLB + tlb_must_refresh）缺失 + switch_address_space 同根跳过优化的耦合 | V13-P2-1(c) |
| **E-RSWIRE 增补** | 解码落地时 RprocEntry.call_mask 必须 u64；RS_SET_PRIV 真掩码随 SYS_SAFECOPYFROM 补；五消息结构专属 wire struct + 断言族同批定稿 | V13-P2-2 / P2-5 / P2-6 |

### 18.5 Rule Discovery（Step 5.7）

1. **新模式候选（建议入库 pattern 85）**："共享 CoW 路径的 refcount 快路必须以 memtype 可写性为门"——快路假设"refcount≤1 ⇒ 已私有 ⇒ 翻写位即可"，该推理只对"私有即可写"的 memtype（anon 族）成立；对 writable 恒 false 的类型（mappedfile），私有页也必须走"拷贝+换型"。检查命令：对每个 `refcount <= 1`（或等价）快路分支，grep 其后续是否经 `is_page_writable`/等价 memtype 判定门控。
2. **死内核面作为接线状态检查项（候选 pattern 86）**：内核实现了完整命令面（VMCTL FlushTlb/InvlPg/GetPdbr）而唯一合法消费者零调用——不是 bug 也不是死代码可删（架构上属于"通电后按需"面），但必须显式注释"无消费者是当前模型（direct map）下的预期"，否则每个后续 reviewer 都会重查一遍。检查命令：对 kernel 侧 sys_* 命令面逐个 grep 用户态消费者，零消费者的必须有注释锚。
3. （延续 V12 候选 84）wire 字段宽度逐字段对账：本轮五结构错位再证该检查的必要性，模板已定（rs_start/rprocpub 样式）。

### 18.6 gate-evidence

```
gate-evidence-Step0:
$ bash tools/design-coverage-check.sh fork-syscall-rewrite --stage 02-stage-vm
  → 26 篇三件套全 PASS；00/99 缺 outline+outline-review+design（CRITICAL ×2）→ G-V12-12 维持

gate-evidence-A:
$ python3 tools/coverage-extract/coverage-extract.py vm rewrite-notes/02-stage-vm \
    --rust-dir os --c-dir minix3/minix/servers/vm \
    --semantic-map tools/coverage-extract/vm-semantic-map.json \
    --output .review/claude/vm/v13/SYMBOLS.md
Loaded semantic map: 81 entries / Found 371 C symbols
  Total C symbols: 371 / Doc covered: 343 (92.5%) / Rust name-match: 43 (11.6%)

gate-evidence-基线:
$ cargo test -p minix-vm --lib                → 490 passed / 0 failed
$ … --features segment_tree_alloc            → 507 passed / 0 failed
$ … --features buddy_alloc                   → 490 passed / 0 failed
$ cargo clippy -p minix-vm --lib             → 18 条 warning 行全属依赖 crate
  （grep -A2 '^warning' | grep -c 'servers/vm' → 0）

gate-evidence-调用号对账（agent 全量枚举 + 主 agent 抽查复核）:
C 注册 26 条：minix3/minix/servers/vm/main.c:536-575（CALLMAP 宏）
Rust 注册 26 条：os/servers/vm/src/ipc/dispatcher.rs:975-1000（static CALLMAP）
C 无 handler 四条：VM_EXEC_NEWMEM/ADDDMA/DELDMA/GETDMA（com.h:637/656/664/672；servers/vm 全仓 do_exec_newmem 零命中）
守护测试：test_callmap_registration_matches_c（15-ipc-dispatch.md:644 记录）

gate-evidence-关键论断复核（主 agent 亲自 grep/sed，防转述失真）:
- C VM 自刷宿主：awk 函数边界 → pagetable.c:119∈pt_assert(:115起)、:255/:319∈vm_freepages(:235起)、:430∈vm_pagelock(:403起)
- C 内核 SMP 刷新：minix3/minix/kernel/proc.c:345-347（MF_FLUSH_TLB + tlb_must_refresh）；os/kernel/src 零命中
- C cow_block 换型：sed -n '59,82p' minix3/minix/servers/vm/mem_file.c → :70-71 memtype=&mem_type_anon
- C mappedfile 恒不可写：mem_file.c:173-175
- Rust 快路：sed cow_exec_pf.rs:254-300 → :268-273 快路无 memtype 门、:282 慢路换 ANON
- munmap PTE：munmap.rs:216/238/261/276 → region/mod.rs:31-36 逐页 pt.unmap
- ACL u32：vm_server.rs:1597（RprocEntry.call_mask: u32）+ :1340（消费）；wire 侧 rprocpub.rs:89（u64）
- ACL fail-open：vm_server.rs:1268-1271 if-let 形状 + vmproc/table.rs:224-235（get_active 对 EXITING 返回 None）
- MAKE_VM：rs.rs:504-507 ↔ minix3/minix/servers/vm/rs.c:218/:370-376
- RS_SET_PRIV None：dispatcher.rs:1091-1102
- overlay 错位：dispatcher.rs:1075-1089（getphys/getref 读 m1p1@16）、:1104-1119（info next 读 m2l2@24）、:1166-1172（rusage children@4）；C 侧 ipc.h:928-934/:1486-1492/:1494-1502/:1513-1520
```

### 18.7 Review Progress 与收敛评估

- [✅] Step 0 预检 / Step -0.5 工具辅助 / Step 0.7 staleness（12/12 复核）/ Gate A / 调用号对账（=Gate B 覆盖缺口半边）/ wire+ACL 双对账 / PTE-TLB 生命周期深查（=Step 2 语义偏移半边）/ Rule Discovery / 收敛评估
- 本轮为本 stage 第八轮：新发现 weighted = 1×10 + 6×3 + 1×1 = 29（P1 一项为新失效模式族），远超停止规则阈值，继续开放登记；执行走 todo-fix（一次一条），P0 清零前提维持（当前 open 无 P0）。

### 18.8 建议的推进顺序（2026-09-09 campaign 第一批收尾后更新）

1. ~~**V13-P1-1**（CoW 快路加 is_page_writable 门 + SimPaging 断言）~~ ✅（§18.9 Fix #63）；~~**G-V12-7**（共享删除源 remaps 递减）~~ ✅（§18.9 Fix #64）；~~**V12-P1-1**（分配器低内存边界 + 审计）~~ ✅（§18.9 Fix #65）——**通电前语义修正批全部闭环**；
2. ~~**V13-P2-1(a)(b)**（TLB 不变量登记 + 死内核面注释）~~ ✅（§18.9 Fix #69，(c) 挂 edge）；
3. ~~**V13-P2-3**（ACL 闸 None 即拒绝）~~ ✅（Fix #66）；~~**V13-P2-4**（MAKE_VM 锚定已登记偏差 + 扫描勘误）~~ ✅（Fix #68）；~~**V13-P3-1(1)**（伪造 fault 源 audit）~~ ✅（Fix #67）；
4. **待执行批**：~~P2-4 死状态三分~~ ✅（Fix #70）；~~P2-8 注释漂移~~ ✅（Fix #71）；~~P2-9 region 两处~~ ✅（Fix #72）；~~P2-5 双路径收敛~~ ✅（Fix #73，判定收窄）；余：P2-6 mapcache 回滚序 → P2-3 错误残余 → P2-1 cache 下沉 → P2-2 memtype 解耦 → P2-7 contig，每条独立 todo-fix 周期；
5. ~~文档批~~ ✅：~~G-V12-11 clearend~~（Fix #80）、~~G-V12-12 00/99~~（28/28 PASS）、~~G-V12-13 checklist~~（四阶段）；V12-P3-1（Redox 对照增强）维持登记——需独立设计决策；
6. edge 侧（单线程执行 edge_todo.md）：E-VMTLB（新）、E-RSWIRE 批（V13-P2-2/5/6 + E-VMMOCK 余件）、E-VFSWIRE（P3-1(2) 的死进程路径对账）、E1/E2 通电件、E5 冒烟（含 V12 增补的故障完整回路验收面）。

---

### 18.9 修复记录（2026-09-09 起，逐条执行的 todo-fix campaign）

### ✅ Fix #65: V12-P1-1 — buddy 低内存边界三态处理 + max_page 换算单源化 + relocate 审计

- **问题**：三后端对 `PAF_LOWER16MB/LOWER1MB` 行为各异——bitmap 全范围限界扫描、segment-tree 先分配后校验、buddy 顶层 `pop_free` 完全无视 `max_page`（同一请求三答案）；且边界换算表（LOWER1MB→256 页 / LOWER16MB→4096 页）在三个后端各抄一份。`relocate` 的后端选择零审计，cfg 回落臂伪装成活路径。
- **设计（方案对比）**：A（选定）——buddy 三段式：界内块直接用（链表跳过完全越界的头）；**跨越边界**的块弹出向下切刻（逐级释放完全越界的上半，下半因边界的 2 的幂对齐必然逐级收缩到界内）；从上级取界内块正常分裂（发上半留下半）。边界换算上移 `mod.rs::max_page_bound` 单点。B（否决）——门面分配后校验拒绝：C 语义是"在界内**找**"而非"界外即失败"，拒绝会把可满足的请求变 ENOMEM。C（否决）——双空闲链（界内/界外分列）：boot 期分配器不值得该复杂度。Linux 同构参照：zone 约束由 zonelist 选区单点表达，buddy 机制层不复制策略。
- **Files**: `phys_mem/mod.rs`（`max_page_bound` + `PhysAllocType::name`）、`phys_mem/buddy_alloc.rs`（`alloc_block` 三段式 + `pop_free_fitting`/`pop_straddling`，原 `pop_free` 替换）、`phys_mem/bitmap_alloc.rs`/`segment_tree_alloc.rs`（换算表收敛到单点）、`vm_server.rs`（relocate 审计行 + 回落臂不可达注释）
- **测试（新增 4）**：`buddy_lower16mb_skips_high_head`（高位页后释放占链表头 → 低内存请求跳过之，pre-fix 必返越界块）、`buddy_lower1mb_carves_straddling_block`（整池块跨越 1MB 线 → 切刻低部）、`bitmap_lower16mb_skips_high_head` / `segment_tree_lower16mb_skips_high_head`（parity 矩阵对称补齐）
- **Verified**: 三矩阵 **498/516/498 passed**（默认 +3、segtree +4、buddy +3，与各 feature 的测试面一致）；clippy servers/vm 0 警告
- **Docs**: 05-physical-memory.md §4.4 buddy 低内存三段式 + 换算单点 + 可观测性段；§5.1 测试表行更新
- **边界**：segment-tree 的"先分配后检查"局限维持原登记（§4.4，实验性后端）；`pop_free` 旧名移除（唯一调用方是 `alloc_block` 顶层路径）

### ✅ Fix #64: G-V12-7 — 共享重映射删除时源区域 `remaps` 递减（`release_shared_remap` + region id 计数器）

- **问题**：共享重映射区域被删除时，源区域的 `remaps` 永不递减（C `shared_delete`，mem_shared.c:110-123）。`remaps` 支撑两个可观察行为——`anon_writable` 的 `remaps > 0` 恒可写分支与 GET_REF 的 `1 + remaps`——因此每次 shm unmap 后源的引用计数报告永久虚高。连带发现：`VirRegion::new` 的 id 恒 0（C 是 `region.c:445` 的全局计数器），使 `getsrc` 的 id 校验形同虚设。
- **设计（方案对比）**：A（否决）——`ev_delete` 增加 `&VmProcTable` 参数、逻辑住 memtype（C 同构）。否决原因非风格而是借用安全：删除漏斗持有被拆进程的 `ActiveProc`/`ExitingProc`（独占 `&mut VmProc`），自重映射形态下 memtype 钩子对表槽位的任何可变访问都与该句柄别名冲突。B（选定）——递减提到删除漏斗旁：`region::release_shared_remap` 复刻 `getsrc` 校验链后按源位置分路（同进程走手中 RegionMap、跨进程走 `table.decrement_region_remaps`，后者 SAFETY 契约要求非在持槽位）。C（否决）——延迟批量结算：引入两阶段状态，复杂度不值。
- **Files**: `region/mod.rs`（`RegionRemapsError` 六变体 + map 级 `decrement_remaps_in` + `release_shared_remap`）、`vmproc/table.rs`（`decrement_region_remaps`，镜像递增侧）、`region/vir_region.rs`（`next_region_id` 计数器 + `shared_source()` 访问器 + split 右半取新 id）、`munmap.rs`（`unmap_range` 整区臂捕获并释放——shared 无 split/low-shrink 支持，中间/头/尾臂本就 EINVAL 不可达）、`exit.rs`（`free_process_phys` 前置释放遍历，两个调用点穿 slot/table）
- **测试（新增 4）**：`test_release_shared_remap_cross_process`（跨进程递减）、`test_release_shared_remap_self_uses_own_map`（自映射走本进程映射）、`test_release_shared_remap_rejects_mismatches`（id 错位/源缺失/类型变更/下溢/未定义源五分支全部拒绝且源不动）、`test_region_ids_unique_per_new`
- **Verified**: 三矩阵 **495/512/495 passed**（+5 含 T1 后基线）；clippy servers/vm 0 警告
- **Docs**: 12-memtype.md §1.6 勘误（`shared_unreference`/`shared_delete` 职责纠正）+ §3.6 偏差行 + 新 §3.7（落点论证）+ §5.1 测试行
- **边界**：GET_REF 读取路径（query.rs）无需改动——它读的 `1 + remaps` 现在随删除自动回落；`memtype.rs` 的 ev_delete 默认实现注释已指向 12-memtype.md §3.7 的偏差说明

### ✅ Fix #63: V13-P1-1 — CoW 复用捷径加 `is_page_writable` 门（独占文件页走复制路径，C cow_block 语义）

- **问题**：`cow_resolve_core` 快路仅凭 `refcount <= 1` 就翻 PTE 写位返回。对 `MappedFile`（`writable()` 恒 false，C "We are never writable"，mem_file.c:173-175）的独占持有页，这会写下只读 PTE 并报成功 → 指令重执行同址再故障，无限活循环。C 无此捷径：`mappedfile_pagefault` 写故障恒 `cow_block`，复制后显式 `ph->memtype = &mem_type_anon`（mem_file.c:70-71）。
- **设计（方案对比）**：A（选定）——快路条件改合取 `refcount <= 1 && region.is_page_writable(...)`，复用前提 = 独占持有 ∧ 私有即可写；与 Linux `do_wp_page`（仅 PageAnon 独占才 `wp_page_reuse`，file 页恒 `wp_page_copy`）、Redox（refcount One 且 grant 语义允许写才 reprotect）同构。B（否决）——删除共享捷径、完全复刻 C 的 per-memtype 分型：行为等价但丢失复用判据的显式落点。C（否决）——memtype 内自拷贝：Rust 无 per-phys_region memtype 变更点，slot 变更归 cow_resolve 所有。
- **Files**: `os/servers/vm/src/cow_exec_pf.rs`（快路合取门 + `verify_cow_consistency` 删除对旧帧 refcount≥1 的单形状断言——独占页被拷贝后合法归 0）
- **测试（新增 1 + 修正 3）**：新增 `test_cow_mappedfile_sole_page_copies_and_retypes_to_anon`（独占文件页写故障 → CowResolved + 换帧 + slot memtype 变 ANON + PTE RW + 旧帧 refcount 归 0）；`test_cow_resolve_no_sharing`/`test_cow_resolve_core_refcount_one_fast_path`/fork 的 `test_cow_copy_page_no_sharing` 三个既有测试原用 `VrFlags::empty()` 的不可写 region 编码旧快路的过宽语义，修正为 `WRITABLE`（快速路径成立的前提形状，test-audit 第 2 维"测试自身过宽"）
- **Verified**: 三矩阵 **491/508/491 passed**；`cargo clippy -p minix-vm --lib` servers/vm 自身 0 警告
- **Docs**: 16-pagefault.md §3.3 重写（复用前提的合取语义 + 活循环因果链）+ §5 测试表刷新；17-cow-mechanism.md §1.5 Linux 对照更新 + §5 测试表刷新；18-vm-fork.md §5 测试表行更新
- **边界**：慢路仍不做 clearend 尾页清零（G-V12-11 维持开口，C mem_file.c:73-79 的 clearend 分支在该条一并设计）；本门不影响 anon 的 Handled 快路（那在 `AnonymousMemory::ev_pagefault` 内部，与 Fix #60 测试 `test_cow_fast_path_flips_pte_writable_without_recopy` 覆盖的路径相同）

### ✅ Fix #66: V13-P2-3 — ACL 闸"None 即拒绝"（EXITING 调用者不再跳过校验）

- **问题**：分发闸 `if let Some(proc) = table.get_active(..) && check` 的 let-chain 形状让"槽位非 active"（IN_USE+EXITING 等）直接跳过 ACL 校验直达 handler——与模块自身的 default-deny 策略（[ARCH: A-11]）不自洽的 fail-open 缺口。
- **设计**：显式三分——`match get_active { Some(p) => check.is_err(), None => true }`，不可解析即按已拒绝处理（同一条审计 + ENOSYS 回复路径）。Linux 参照：LSM 钩子对无法解析凭证的主体默认拒绝，不存在"查无此人即放行"的旁路。
- **Files**: `vm_server.rs`（闸门 match 化）
- **测试（新增 1）**：`test_run_once_exiting_caller_denied_enosys`——EXITING 调用者发 VM_INFO → 恰好一条 ENOSYS 回复、handler 未执行
- **Verified**: `cargo test -p minix-vm --lib` → **499 passed / 0 failed**；clippy servers/vm 0 警告
- **Docs**: 04-acl.md §4.4 门禁代码块更新 + "None 即拒绝"设计说明段

### ✅ Fix #67: V13-P3-1(1) — 伪造 fault 源的 release 可观测性（audit 对齐 C 的 release printf）

- **问题**：`debug_assert!(rcv_sts.is_from_kernel())` 在 release 下连条件带断言一起编译掉——伪造 fault 消息静默通过；C 的 release 保留 printf（main.c:154-157）后照常处理。行为面两侧等价（都容忍伪造），差距纯在可观测性。
- **设计**：显式 `if !is_from_kernel()` 块——测试构建保持 debug_assert 硬失败语义，release 走 `audit_log!` 记 `[VM PF] faked pagefault source`。
- **Files**: `vm_server.rs`（P3 分支改写）
- **测试**：无新增——test 构建下行为与原 debug_assert 完全一致（audit 通道在 test/no-feature 下编译剔除，无可断言面；诚实标注 UNVERIFIED-for-test，验证为 grep + clippy + 499 全绿回归）
- **Verified**: `cargo test -p minix-vm --lib` → **499 passed / 0 failed**；clippy servers/vm 0 警告
- **Docs**: 15-ipc-dispatch.md §1.5 增补 release 可观测性说明段

### ✅ Fix #68: V13-P2-4 — MAKE_VM 拒绝注释锚定到已登记偏差（含扫描勘误）

- **勘误**：V13 扫描称 MAKE_VM"无任何偏差登记"不准确——25-rs-services.md §3.9/§3.10 已登记为 A-8 缺口（scan 时未深查该文档的差异表，Step 1.0e 教训）。真实残余：rs.rs 拒绝注释无指向锚。
- **Files**: `rs.rs`（MakeVmInstance 臂注释指向 25-rs-services.md §3.9/§3.10 + C rs.c:218 行为描述）
- **测试**：无新增（纯注释变更）；`cargo test -p minix-vm --lib` → 499 passed 回归
- **Verified**: clippy servers/vm 0 警告
- **Docs**: 25-rs-services.md 已有登记，无需改动；本条 + todo V13-P2-4 条目勘误即登记闭环

### ✅ Fix #69: V13-P2-1(a)(b) — TLB 不变量登记 + 内核死 VMCTL 面注释（含扫描勘误；(c) 挂 edge E-VMTLB）

- **勘误**：扫描称"ARCH 偏差未登记"不准确——08-pagetable-ops.md §1.8 已完整论证 C 全局清缓存 vs Direct Map 逐条 invlpg，且明确 `write_pte_dm` 写后立即 invlpg（扫描遗漏，Step 1.0e 教训第二例）。真实残余收窄为两件。
- **处置**：(a) "VM 只改不在运行的进程的页表"不变量正式登记为 16-pagefault.md §3.7（逐路径的停等机制表 + 违反后果 + 新路径准入门槛）；§3.6 新增第 12 行（TLB 一致性状态）并修正第 3 行过时状态（clear_pagefault 已随 Fix #60 接线）。(b) `os/kernel/src/syscall.rs` 的 FlushTlb/InvlPg/GetPdbr 臂补注释：VM 侧零调用是设计使然（direct map 恒定翻译 + 写后 invlpg），勿读作未接线 edge。
- **Files**: `16-pagefault.md`（§3.6 两行 + 新 §3.7）、`os/kernel/src/syscall.rs`（注释，零行为变更）
- **测试**：无新增（注释/文档变更）；kernel 侧 `cargo check` 通过（注释不触编译面）+ VM 499 全绿回归
- **边界**：(c) SMP 目标进程刷新（C MF_FLUSH_TLB，proc.c:345-347）维持 edge E-VMTLB；E-VMTLB 的"VM 自刷四处"引用即 08 §1.8 的已登记内容，无需重复

### ✅ Fix #70: V12-P2-4 — 死状态/死链路批（七子项三分处置：三删二接线二标注/已达标）

- **逐项处置**：
  1. **TOTAL_PAGES** → **接线**（推翻 V12 的"删"建议）：C 的 `vsi_total = total_pages`（utility.c:118）读的是**含 boot 附加页的全局计数**，Rust 的 VMIW_STATS 此前读分配器总量（缺 boot extras）——`global::total_pages()` 去掉 `#[cfg(test)]` 重新投产，`query.rs` Stats 改读全局。26-vm-queries.md §4.4 同步。
  2. **FILEMAP_ENABLED** → **已达标（标注态）**：setter 本就 `#[cfg(test)]`，静态量文档注释完整（C 的 env_parse 设定面属 boot 协议扩展，当前恒 1 = C 默认）。
  3. **`VmReply::ExecNewmem`** → **删**：minix-types 变体 + `VmExecNewmemOut` 载荷 + vm_server 两编码臂（两侧都 ENOSYS 的死回复形态，T19 判定的残余）。
  4. **spares 链** → **删**：`missing_spares` 字段/`mark_alloc_failure`/`missing_spares()`/run 循环与 handle_signal 钩子/`alloc_cycle`/`FREE_CACHE_BATCH`/对应测试。理由：spare-pool 机制被 A-1 结构性消除；"压力再解释"的预期生产者被 T30 内联回收取代，接线即双重回收。
  5. **lazy 家族** → **删**：`PageSlot::Reserved` 态 + `map_lazy`/`get_slot_any`/`get_slot_mut_any`/`is_reserved`/`is_empty`/`reserved()` + `test_map_lazy`（改写为 `test_map_page_lifecycle`）。理由：P0-1 修复后从未有生产调用方，需求分页经 NeedNewPage 落地；双查询 API 只为绕开 Reserved 而存在。git 历史 + 13-region-mapping.md §3.2/§3.6/§4/§5 保留设计底稿。
  6. **ExitingProc 四访问器** → **删**：`slot`/`endpoint`/`flags`/`regions`（exit 路径只需 `regions_mut` + `reap`）；两处测试改用 `get_exiting` 解析断言（语义等价：该视图仅对 IN_USE+EXITING 槽解析）。
  7. **`VmProc::check`** → **删**（"接线"选项被实践证伪）：试接线到 `ActiveProc::new`/`ExitingProc::new` 后三个测试立即触发——`activate_relaxed` 合法支持 NONE-endpoint 槽（fork/exec 临时槽），"IN_USE ⇒ endpoint 非 NONE"不是本项目的不变量。删除并留注释说明。
- **Files**: `minix-types/src/ipc/vm.rs`、`vm_server.rs`、`global.rs`、`query.rs`、`region/page_state.rs`、`region/vir_region.rs`、`vmproc/vmproc_handle.rs`、`vmproc/table.rs`、`vmproc/vmproc.rs`、`memtype.rs`（测试）
- **Verified**: 三矩阵 **498/516/498 passed**；`cargo test -p minix-types` → 192 passed；clippy servers/vm 0 警告
- **Docs**: 13-region-mapping.md（§3.2 两态化 + §3.6 行 + §4 + §5 测试行）、24-page-cache.md（§3.7 重写 + 结构图 + backlog 行）、26-vm-queries.md（§4.4 代码块）、06-page-allocator.md（头部注记）

### ✅ Fix #71: V12-P2-8 — 注释 C 锚点漂移批（五处修正 + 一处复核免修）

- **修正**（全部先 sed 验证 C 实际行号再改，fix-guard）：
  1. dispatcher.rs mapcache 文档注释：对齐检查 `:99-101` → **:108-110**；EINVAL `:102-103` → **:116**；map_page_region `:128-134` → **:131-134**。
  2. dispatcher.rs mapcache 函数体三处内联注释：同上三锚 + 逐页循环 `:136-168` → **:144-167**。
  3. dispatcher.rs 测试注释 `mem_cache.c:107` → **:116**。
  4. 16-pagefault.md §3.6 第 5 行：缺页计数"生产路径未调用"已过时——`inc_major_fault`/`inc_minor_fault` 在 vm_server.rs 生产接线（G-V12-6 批次），状态 ⚠️缺口 → ✅。
- **复核免修**：fork.rs `PageTableInitFailed` 注释 "ENOMEM from pt_new() in do_fork()"——C fork.c:70-71 确认函数与行为均准确（V12 原判"文件名错"指向的旧文本已不存在）。
- **Files**: `ipc/dispatcher.rs`（7 处注释）、`16-pagefault.md`（1 行）
- **测试**：无新增（注释/文档）；`cargo test -p minix-vm --lib` → 498 passed 回归
- **Docs**: 16-pagefault.md §3.6 第 5 行

### ✅ Fix #72: V12-P2-9 — `find_overlap` 邻居判定 + 零长 insert 拒绝

- **问题**：(a) `find_overlap` 自 BTreeMap 首端 `range(..end)` 线性扫到尾（最坏 O(n)），与 `insert` 注释"只查两个最近邻居"自相矛盾；(b) 零长 region 的 span 为空 → overlaps 恒 false → 绕过重叠守卫直达 `BTreeMap::insert`，同 vaddr 时**静默替换**既有 region（未请求的销毁）。
- **设计**：(a) 前驱探测（`range(..=start).next_back()`，其跨度可能越过 start）+ 区间内首键探测（`range(start..end).next()`）——O(log n)，与注释声明一致；(b) `insert` 入口拒绝 `length == 0`（返回 `Err(region)`，与重叠拒绝同通道）。
- **Files**: `region/region_map.rs`（find_overlap 重写 + insert 守卫）
- **测试（新增 2）**：`test_find_overlap_predecessor_straddle`（8 个低位 region + 高位跨越者，查询起点落在跨越者腹内——线性扫与邻居探测在正确性上等价、在探测路径上区分）；`test_insert_zero_length_rejected`（拒绝 + 原区域存活）。首版测试两次断言失败均为**测试自身十六进制算术错误**（0x100_0000 + 0x1_0000 = 0x101_0000），代码无误——修正的是测试。
- **Verified**: 三矩阵 **500/518/500 passed**（+2）；clippy servers/vm 0 警告
- **Docs**: 本条即判定记录（13-region-mapping 的 §3.2 已随 Fix #70 两态化更新，find_overlap 行为属内部实现细化）

### ✅ Fix #73: V12-P2-5 — proc_table 访问语法统一 + 双轨判定收窄（勘误：非双真相源）

- **勘误**：V12/V13 的"rs.rs 六处生产双路径"锚点全部位于 `#[cfg(test)]` 区——生产路径早已随 T5/T6 的 VmContext 收敛走 ctx。且 `VmContext.proc_table: &'static VmProcTable`（vm_server.rs:62）本身就是 `get_global()` 返回对象的别名——两个"路径"是同一对象的两种写法，不存在第二个真相源。
- **处置**：(a) vm_server.rs boot 路径三处 `get_global()`（init_vm_slot/init_boot_procs/exec_bootproc）统一为 `self.ctx.proc_table`——生产区现仅存 munmap.rs 漏斗一处有意例外（T2 的 release_shared_remap 表访问，带注释）。(b) page_frames 的 `ok_or` 软错误（init 期）与 `expect`（初始化后不变量）判定为**阶段正确形态**，维持现状。
- **Files**: `vm_server.rs`（三处语法统一，零行为变更）
- **测试**：无新增；`cargo test -p minix-vm --lib` → 500 passed 回归
- **边界**：munmap.rs 漏斗的 get_global 是 T2 设计的一部分（缺表则共享释放无法路由），勿当漂移清除

### ✅ Fix #74: V12-P2-6 — mapcache 回滚勘误 + 中途失败注入测试

- **勘误**：V12 称"C 先查 cache/注册再 map_pf，失败天然无需回滚"不成立——C do_mapcache 同样是"区域先建（map_page_region，:131-134）→ 逐页 find+链接 → 失败 map_unmap_region 整区回滚"（miss :155-157、map_pf 败 :165-166）。两侧顺序同构；V12 把"回滚代码的存在"误读成了"顺序相反"。
- **真实差异（更优）**：Rust 区域在调用者映射表**外**组装、提交时插入——失败回滚只退 refcount（缓存帧受 IN_CACHE 保护不进回收漏斗），无需回退 PTE（区域从未进页表）。
- **Files**: `ipc/dispatcher.rs`（新增测试，零生产代码变更）
- **测试（新增 1）**：`test_dispatch_mapcache_mid_failure_rolls_back_refcounts`——3 页请求、前 2 页命中链接、第 3 页 miss → ENOENT；断言两帧 refcount 回到 cache-only 基线且 IN_CACHE 保持
- **Verified**: 三矩阵 **501/519/501 passed**（+1）；clippy servers/vm 0 警告
- **Docs**: 24-page-cache.md §3.6 新增第 10 行（回滚语义 C-parity + unwind 面更小）

### ✅ Fix #75: V12-P2-3 / V10-P2-3 — 错误类型残余闭环（改名 + 两项勘误）

- **改名**：`region::vir_region::VmError`（单变体 `InvalidParam`）→ **`VirRegionError`**——消除与 `minix_types::VmError` 的同名异型碰撞（`rg "enum VmError"` 从此全仓唯一）。纯重命名，extend/split 的四个生产调用方（brk/munmap）本就经各自错误类型转换，零行为变更。
- **勘误 1**："dispatcher.rs:1664/:1812 直接外泄 VfsQueueError"——两处均为 `#[cfg(test)]` 区的 nop 回调签名，非生产路径。生产面 VfsQueueError 的三个归宿各有设计：`enqueue_fdio` 映入 `CowError`、`free_region_pages` 失败 audit+drop（fdclose 丢失可观测）、VFS 回调边界由 `handle_reply` 消费——无 errno 外泄。
- **勘误 2**：`CacheError` 无需 From——唯一生产消费点 `dispatch_setcache` 已按 C cache.c:222-226 映射 EINVAL；为映射而映射的 From 是无调用方的死代码（本批自己的 pattern 86 教训）。
- **Files**: `region/vir_region.rs`（重命名，7 处）
- **测试**: 无新增（重命名）；三矩阵 501/519/501 + clippy 0 警告回归
- **边界**: V10-P2-3 的"验证锚点"（`rg "pub(crate) enum .*Error"` 收敛到 1 个对外）达成：对外唯一 `minix_types::VmError`，crate 内剩模块级类型（VirRegionError/CacheError/VfsQueueError/CowError/MemTypeError/MunmapError 等），各有明确消费边界

### ✅ Fix #76: V12-P2-2 — memtype 反向依赖判定闭合（C-parity，维持现状）

- **判定**：不解耦。C 的 memtype 回调签名本就收 `struct vmproc *vmp`（memtype.h:18 ev_pagefault / :21 ev_resize / :22 ev_split——策略钩子需要跨进程查找（shared getsrc）与调用方内存访问（split/resize）），"策略层不碰进程表"的纯化理想与 ground truth 的结构相悖。Rust 的 `&VmProcTable`/`&mut ActiveProc` 显式传参是同形翻译，且比 C 的全局数组直捅更干净；解耦需引入 region-lookup 间接层，与单线程事件循环的简单性原则冲突。
- **Files**: `memtype.rs`（:7 import 处加判定注释，零行为变更）
- **测试**：无新增；501/519/501 + clippy 0 警告回归
- **边界**：`use crate::vmproc` 的 import 语句保留——它服务的是合法的签名依赖，非历史残留

### ✅ Fix #77: G-V12-9 — inhibit 条目勘误闭单（C 锚点不存在，机制已由 08 登记）

- **勘误**：登记锚点 main.c:196/:265-267 无 inhibit 门（全文 grep 证实）；全 C VM 源的 inhibit 仅 `pt_writemap` 的 CONFIG_SMP 动态停等包裹（pagetable.c:799-815/:928-934，自带 FIXME）。V12 把它误读为"main.c 主循环的 RS 更新服务门"，并提议在 25-rs-services.md 加偏差行——前提不存在，偏差行不添加。
- **真实状态**：机制本体已由 08-pagetable-ops.md（§1.10 + 差异表第 10 行）登记为执行模型差异；与 16-pagefault.md §3.7 的结构化不变量互为两种实现策略（C 运行时停等 vs Rust 结构纪律），§3.7 已补互链。E-VMTLB 立项时 VMINHIBIT 为候选方案之一。
- **Files**: `16-pagefault.md`（§3.7 补对照段）；todo 本条勘误
- **测试**：无（文档）；501/519/501 回归
- **Step 5.7 教训**：登记条目时锚点必须当场 sed 验证——G-V12-9 与 P2-6、P2-5 同为"复核阶段发现登记前提失真"的第三、四例，加固模式 83 锚点纪律的必要性

### ✅ Fix #78: V12-P2-1 — 模块边界两段纯移动重构（encode.rs + cache_handlers.rs）

- **Phase A（e86d52e85）**：`VmReplyForIpc` + `reply_to_errno` + `encode_reply_data`（含 3 个测试）→ **`ipc/encode.rs`**（337 行）——纯线格式代码归 ipc 域；vm_server.rs 2944 行（-210）。
- **Phase B（77b0edb2f）**：`dispatch_mapcache/setcache/forgetcache/clearcache` + `unmap_region_pages` → **`ipc/cache_handlers.rs`**（312 行，`impl MessageDispatcher` 固有块）——dispatcher.rs 2610→2376（-234，达 V12 验证线 ≥200）；CALLMAP 与既有测试经固有方法解析零改动。
- **设计偏离（对照 V12 提议）**：未采纳"下沉 page_cache.rs"——四 handler 消费整个 `VmContext`，迁入数据结构模块将倒置分层（page_cache → vm_server）；选 `ipc/cache_handlers.rs` 与 dispatcher 同向依赖，模块文档已记录该取舍与 C 归属（mem_cache.c）的对照。
- **Verified**: 三矩阵 501/519/501 全绿；clippy servers/vm 0 警告；`wc -l` 验证行数
- **Docs**: 24-page-cache.md 头部/结构图更新新路径；15-ipc-dispatch.md 头部加迁移注记（行号锚点标注为迁移前时点）；checklist 归 G-V12-13 批

### ✅ Fix #79: V12-P2-7 — `PfnAllocator::alloc_contiguous` + `ev_new` 单调用化（质量债清偿）

- **问题**：`ContiguousAnonymous::ev_new` 逐页分配后 `windows(2)` 事后验证连续性 + 失败回滚，注释声称"非连续 fallback"实际返回 ENOMEM（自相矛盾）；根因是 `PfnAllocator` trait 缺多页分配原语，而三个后端的 `alloc_mem(count, …)` 本就返回连续 run（bitmap 扫 run、buddy 整块、segtree first-fit）——C 的 `alloc_mem(pages, allocflags)`（mem_anon_contig.c:79）同理。
- **设计（方案对比）**：A（选定）——trait 加 `alloc_contiguous(count)` **带默认实现**（逐页+验证+回滚，即原语义），`VmPageAllocator` 覆写为单次 `alloc_phys(count)` 漏斗请求（回收重试 V11/T30 免费随之），20 个测试桩实现零改动。B（否决）——必选方法：破坏 20 个测试实现。C（否决）——`PageAllocFlags::CONTIG` 旗标路线：C 的 PAF_CONTIG（vm.h:23）定义后零消费（alloc.c/region.c 均不检查），两侧连续性都来自原生多页分配，旗标属集合 parity 不引入语义。
- **Files**: `region/page_state.rs`（trait 默认实现）、`alloc_page.rs`（VmPageAllocator 覆写 + 新测试）、`memtype.rs`（ev_new 重写，删 windows(2)/TODO/矛盾注释）
- **测试（新增 1 + 既有 2 转轨）**：`test_alloc_contiguous_override_returns_consecutive_run`（真实 bitmap 分配器：run 分离 + 整 run 释放后可复得——**顺带挖出 `active_allocations` 事件/页粒度记账不对称**：多页 alloc 后页粒度 free_pfn 会下溢 debug_assert，预先存在、非本次引入，登记待后续处置）；`test_contig_ev_new_preallocates_consecutive_pfns`/`rolls_back_on_gap` 经 trait 默认实现转轨零改动通过
- **Verified**: 三矩阵 **502/520/502 passed**（+1）；clippy servers/vm 0 警告
- **边界**：contig 域的 fork 仍不支持（ev_reference 拒绝，V12/13 已登记）；记账不对称修复若需要，属 alloc_stats/alloc_page 的独立小条目

### ✅ Fix #80: G-V12-11(2) — clearend 尾页清零建模（MappedFile 命中分支内联 cow_block）

- **问题**：文件尾页（`roundup(offset+clearend) ≥ length`）在 C 里经 `cow_block(…, clearend)`（mem_file.c:131-134）拷成私有 anon 页并清零 EOF 之后的 `clearend` 字节（:73-79）——缓存的陈旧尾部绝不能暴露给进程。Rust 此前对尾页返回 NeedCow（拷贝但不清零），EOF 后的陈旧字节会出现在进程的私有页里。
- **设计**：MappedFile 命中分支三分重构（对齐 C 的分支序）：尾页 → **内联**私有拷贝 + `copy_page_and_zero_tail`（分配私页 → 拷贝 → 清零尾部 → 以 ANON 挂载 → Handled，镜像 C 在 memtype 回调内联 cow_block 的结构，clearend 语义留在知道它的 memtype 层，而非塞进通用 cow_resolve_core）；非尾写 → NeedCow（原样）；非尾读 → 直链（原样）。
- **Files**: `cow_exec_pf.rs`（`copy_page_and_zero_tail` 双 cfg 变体：target 走 direct map 写零，host 桩 no-op——与 copy_page_content 同分split）、`memtype.rs`（命中分支三分 + `_alloc` 参数启用）
- **测试（新增 1）**：`test_mapped_file_tail_page_private_copy_with_clearend`——2 页区域 + clearend=100：非尾读直链缓存帧、尾读得私有 ANON 新帧、cache 条目存活。字节级清零在 host 无法断言（桩 no-op），诚实标注 target 验证。
- **Verified**: 三矩阵 **503/521/503 passed**（+1）；clippy servers/vm 0 警告
- **边界**：24-page-cache.md §3.6 第 6 行（ONCE 往返差）维持登记（第 1 笔，E-VFSWIRE 批次）；尾页已私有化后二次写不再触发清零（PTE 已 RW，无故障）——与 C 的 phys != MAP_NONE 早退一致

### ✅ Fix #81: V12-P3-2 — 卫生批（transport pub 收窄 + audit 宏文档）

- **③** `ipc/transport.rs` 六类型 `pub → pub(crate)`（lib.rs 无再导出、消费全在 crate 内——公开面是过宽的残留）。
- **①** lib.rs 双 audit_log! 宏加文档：feature 版指向 no_std sink，no-op 版显式声明 release 可观测性由 VmContext 计数器（pagefault_errors/dropped_messages/alloc_failures）承载——V12 问句"计数器面是否足够"的答复：足够（VM_INFO 可达）。
- **② 免修**：memtype pub fn 文档抽查已达标；**④ 维持**：bitmap lastscan 优化登记；**P3-1 维持**：Redox 增强集需独立设计决策。
- **Files**: `lib.rs`、`ipc/transport.rs`
- **测试**: 无新增；503 passed + clippy 0 警告回归

---

## 19. 增补登记（2026-09-16，V14 轮预登记）

> 来源：2026-09-16 会话决议（minix-sef 消费方盘点），非完整 review 轮——无 Step 0 / Gate A 产出，不改变 V13 轮已记录的收敛评估；编号预占 V14，下一次完整扫描轮顺延使用。
> 背景：`os/libs/minix-sef` 已随 E-ISWIRE(1)（609e73f22，2026-09-16）从 5 行占位实装为 SEF 接收循环库（`SefIpc` 动词注入 + `sef_receive_status` 分类核 + RS ping 拦截，对照 C libsys sef.c:149-260 / sef_ping.c:21-38）；edge_todo.md E-ISWIRE 登记的生产接线消费方为 IS（首个深度消费方）、MIB（第二）、devman（第三），**VM 不在名单中**——而 VM 恰是 SEF 协议面实现得最完整的一个（内联自持，见下条证据）。

### V14-P2-1（P2 接线债）VM 的 SEF 协议面内联自持、未列入 minix-sef 消费方——同一 C 协议存在两份 Rust 实现

- **证据**：VM 无 minix-sef 依赖（`os/servers/vm/Cargo.toml` 无声明），SEF 等价逻辑全部内联并各自锚定 C：
  - 启动握手：main.rs:51-54（自述 "C: sef_local_startup()"，握手折叠进主循环优先级 2 分派——该形状本身有文档登记：01-vm-init-main.md §3.4）+ `rs_handshake`（vm_server.rs:1234，对照 C sef_startup + sef_cb_init_fresh）+ RS_INIT 解包（vm_server.rs:1114-1122，对照 do_sef_init_request）；
  - 接收循环：`run_once` 内联 sef_receive_status 等价接收（vm_server.rs:961）+ 对全部 notify 一律 continue（:973，对照 C main.c:126-129）+ 信号分派体 `handle_signal`（:827，C sef_cb_signal_handler 体，生产入口标注 wired at E1，:825）；
  - minix-sef 侧对应面：`SefEvent::{Signal, Init, Call}` 分类 + ping 拦截——与 VM 内联版是同一 C 单源（libsys/sef.c）的两份 Rust 翻译。
- **影响**：当前无行为 bug（VM 内联版有测试承载）。三笔真实成本：① 协议实现双源——ping 拦截/notify 分类的任何修正要改两处，随消费方增多漂移风险上升（V13-P2-6 的 overlay 错位已证明"同一协议两份解码"就是错位温床）；② **E1 通电时的信号抵达裁决**：C 侧信号经 sef_receive_status 拦截直达 handler；Rust 内联版 run_once 对 notify 一律 continue，若 E1 走 SYSTEM notify 通道投递，现循环形状会把它当普通 notify 吞掉——minix-sef 的 `SefEvent::Signal` 分类正是这段语义的现成实现，切换与否必须在 E1 接线时点显式裁决，不能两不靠；③ 与 IS/MIB/devman 的 SEF 面形成两种形状（事件上浮 vs 内联跳过），联调（E5）对账成本增加。
- **修改方案**（候选）：A（建议）——VM 引入 minix-sef 依赖成为第四消费方：`KernelIpcTransport` 薄适配 `SefIpc`，`run_once` 接收+分类半改走 `sef_receive_status`，`SefEvent::Call` 进既有优先级分发、`Signal` → `handle_signal`、`Init` → `rs_handshake`；与 E-RSWIRE VM 侧解码批（V13-P2-2/P2-5/P2-6）同批开——`ipc_call_rs_init` 目前诚实返回 NotImplemented（vm_server.rs:1454/:1473，E-RSWIRE 门控），握手路径本来就要重开，避免两次打开同一函数族。B（若裁决维持内联）——在 run_once 接收处 + checklist 登记有意偏差（理由 + 复核时点），防止后续 reviewer 重复登记本条（pattern 86 同族教训：有共享替代品的内联实现必须显式注释）。
- **时点与依赖**：不早于 E-ISWIRE(2)(3)（避免 VM 单独承受 minix-sef API 摇摆）；信号半与 E1 接线强耦合（见影响 ②）。minix-sef 的 LU/ST 拦截路径登记不实装，不阻塞 VM（VM 不消费 LU/ST）。
- **验证**：切换后 `cargo test -p minix-vm --lib` 三矩阵全绿；既有 run_once mock transport 测试族（V10-P0-2）适配 `SefIpc`；补映射测试（`SefEvent::Init` → rs_handshake 恰一次、`SefEvent::Signal` → handle_signal、ping 拦截不上浮）。
- **edge 联动**：E-ISWIRE（edge_todo.md）——VM 为候选消费方，本条是 VM 侧活指针（V13-P2-2/P2-5 先例），共享 crate 侧工作唯一入口仍在 edge_todo.md；E1——信号投递通道裁决（影响 ②）；E5——联调对账面。

---

## 存档指引

- 第一轮至第六轮全部条目与修复记录：[`archive/todo-V11-archive-2026-09-08.md`](archive/todo-V11-archive-2026-09-08.md)
- 第七轮 V12 全卷（含 Fix #59–#62 修复记录原文）：[`archive/todo-V12-archive-2026-09-09.md`](archive/todo-V12-archive-2026-09-09.md)
- 本轮 SYMBOLS 全量清单：`.review/claude/vm/v13/SYMBOLS.md`（中间产物，正式引用以本文件 §18.1/§1 汇总为准）
- 跨 stage 条目唯一入口：`rewrite-notes/coordination/edge_todo.md`（§18.4 两条增补；§19 V14-P2-1 为 VM 侧活指针，联动 E-ISWIRE / E1 / E5）
