# Edge TODO 归档(edge_todo_archive)

> 本文件收容 `edge_todo.md` 中已闭单条目的完整原文,保留判定过程供回溯。
> 归档执行:2026-09-15,edge_todo.md 清理 campaign 第一步(文件治理)。
> 规则:条目迁入本文件后,edge_todo.md 只保留一行指针;若后续发现闭单判定有误,
> 从本文件恢复原文重新开单,不得在指针行上直接续写。

## 归档索引

| 条目 | 闭单性质 | 闭单日期 | 归档日期 |
|---|---|---|---|
| §0 02-stage-vm 实施 campaign 顺序表(T1–T36) | 全部完成(campaign 完结) | 2026-09-07 | 2026-09-15 |
| E3 VmBootHandoff 补 kernel text/data span | 完成 | 2026-09-08 | 2026-09-15 |
| E-RSSTART rs_start_t 字节 ABI pinning + copy_rs_start 解码 | 改判关单(接线转入 03-stage-rs §21 campaign) | 2026-09-07 | 2026-09-15 |
| E-VMMCPWIRE vmmcp 消息族 reply.addr u32 → 64 位 | 完成(余件转低优先扫描项) | 2026-09-08 | 2026-09-15 |

---

## §0 02-stage-vm 实施 campaign 顺序表(进度真相源)

> 归档注记(2026-09-15):T1–T36 全部闭环(见末行 T36,T24–T36 已于 2026-09-07 完结),
> 表格整体迁入归档。原表对应 `02-stage-vm/todo.md` §14 的 V11 条目;VM 侧口径:
> 依赖共享 trap 层的条目,VM 侧逻辑完备(seam + mock 测试)即标 ✅,真实通电挂对应 edge 条目(模式 60 诚实契约)。

| 迭代 | 批次 | 内容 | 对应条目 | 状态 |
|---|---|---|---|---|
| T1 | 0 | 文档测试名 4 处 + §5.4 计数刷新 | V11-P2-6 | ✅ 2026-09-06(todo.md §15 Fix #19) |
| T2 | 0 | 过时注释 4 处(X86_64Paging 已实现) | V11-P2-4 | ✅ 2026-09-06(todo.md §15 Fix #20) |
| T3 | 0 | clippy 回归收敛 + 卫生批次 | V11-P2-3 + V11-P3-1 | ✅ 2026-09-06(todo.md §15 Fix #21;all-features 剩 :301 归 T4) |
| T4 | 0 | 删 DefaultAllocator 双真相源 + 组合语义测试 | V11-P1-3 | ✅ 2026-09-06(todo.md §15 Fix #22) |
| T5 | 1 | VmContext 第一步:parts_mut 消灭 | V11-P1-2(1/2) | ✅ 2026-09-06(todo.md §15 Fix #23) |
| T6 | 1 | VmContext 第二步:dispatcher 收 &mut VmContext + fdref/table 收敛 | V11-P1-2(2/2) | ✅ 2026-09-06(todo.md §15 Fix #24;fdref 收敛归 T10) |
| T7 | 1 | per-call codec 注册表 + dispatch 表驱动化 | V9-P2-1 + V9-P2-2 | ✅ 2026-09-06(todo.md §15 Fix #26) |
| T8 | 1 | 错误枚举收敛 → **判定闭合:现有 From 集中表即最优** | V10-P2-3 + P2-2 | ✅ 2026-09-06(todo.md §15 Fix #27,WONTFIX 级设计判定) |
| T9 | 2 | KernelIpcTransport VM 侧完备 + KernelGateway seam | V11-P1-1(通电→E1/E2) | 🔄 step1 ✅(Fix #28)step2 ✅(Fix #29)step3 ✅(Fix #34:grant 贯通 + 假成功消灭 + fail-closed;rproctab 字节解码→**E-RSWIRE**) |
| T10 | 2 | VFS_FDCLOSE 发送 + region close 入队 | (V11-P1-1 建议 2 / P1-3 链) | 🔄 入队半 ✅(Fix #33);发送半 → **E-VFSWIRE** |
| T11 | 2 | fork.rs sys_fork 真实语义 | fork.rs stub(通电→E2) | ✅ 2026-09-07(stub 删除随 Fix #29;eager-CoW 相随 Fix #53;通电→E2+E-FORKMSG) |
| T12 | 2 | RS_PREPARE map_proc_dyn_data | rs.rs:250 DEFERRED | ✅ 2026-09-06(todo.md §15 Fix #40) |
| T13 | 2 | RS_UPDATE 步骤 5-7(VM 侧)+ 步骤 4 走 Gateway | rs.rs:328 DEFERRED(通电→E2) | ✅ 2026-09-06(todo.md §15 Fix #42) |
| T14 | 2 | exec_bootproc(minix-elf + VM 映射 + Gateway.sys_exec) | vm_server.rs:385 DEFERRED(通电→E2) | 🔄 装载半+sys_exec wire ✅(Fix #41);栈帧 ABI → **E-BOOTFRAME** |
| T15 | 2 | audit 日志转发(Gateway.diagctl) | audit.rs:16(通电→E2) | ✅ 2026-09-06(todo.md §15 Fix #32) |
| T16 | 2 | sanity_checks feature + usedpages 等价物 | V10-P2-1 sanity 行 + G-V11-2 | ✅ 2026-09-06(todo.md §15 Fix #38;usedpages 语义由 verify_refcounts 覆盖) |
| T17 | 2 | bitmap cache_freepages 三步路径 → **判定闭合:语义已被双层覆盖,钩子删除** | bitmap_alloc.rs:347 DEFERRED | ✅ 2026-09-06(todo.md §15 Fix #31) |
| T18 | 2 | alloc 失败计数接入 InfoStats(周期循环判定不采纳) | alloc_stats.rs:46 DEFERRED | ✅ 2026-09-06(todo.md §15 Fix #30) |
| T19 | 2 | exec_newmem / DMA 三条 parity 处置(删 dead stub,不实现) | dispatcher.rs:820/:1223 | ✅ 2026-09-06(todo.md §15 Fix #25) |
| T20 | 2 | 大匿名映射懒分配 → **判定闭合:demand paging 已是现状**(稀疏表示=证据门控优化) | V9-P3-2 | ✅ 2026-09-06(todo.md §15 Fix #35) |
| T21 | 3 | 页表可注入化 + VM 内 SimPaging | V11-P2-1(QEMU 冒烟→E5) | ✅ 2026-09-06(todo.md §15 Fix #39) |
| T22 | 3 | MemType / PhysAllocator 方法级补测 + buddy reserve 语义修正 | V11-P2-2 | ✅ 2026-09-06(todo.md §15 Fix #36) |
| T23 | 3 | rs_handshake/init pin 测试 + run_once 分支补测 + CI 矩阵 | V11-P2-5 | ✅ 2026-09-06(todo.md §15 Fix #37;rs_init pin 已随 Fix #34) |
| T24 | 4 | 残留标注清理 + parity/死代码判定批次 + 新缺口登记(G-V12-1..4) | todo.md §16 | ✅ 2026-09-07(todo.md §16 Fix #43 pre + Fix #44 清理批次;五篇文档同步) |
| T25 | 4 | pt=None → SimPaging 翻转 ×6(munmap×4/brk×2) | V11-P2-1 收尾 | ✅ 2026-09-07(todo.md §16 Fix #45;mmap helper 连带修复) |
| T26 | 4 | MOCK_BASE_MUTEX + extend_to_static_lifetime 归零(线程本地窗口) | V11-P1-2/V9-P2-4 验收锚点 | ✅ 2026-09-07(todo.md §16 Fix #46;新登记 G-V12-5 归 E3) |
| T27 | 4 | dispatcher 4 函数 happy-path 补测 | G-V12-3 | ✅ 2026-09-07(todo.md §16 Fix #47;四矩阵 476/493/476/476) |
| T28 | 5 | CacheMemory::ev_pagefault 缓存查找 + PbCache 接线 → **判定闭合:邮箱机制删除,契约 fail-closed 化** | G-V12-1 | ✅ 2026-09-07(todo.md §16 Fix #48) |
| T29 | 5 | SIGKMEM 信号 seam + do_memory 排空循环(kernel 对端已落地;通电挂 E1) | G-V12-2 + G-V11-1 | ✅ 2026-09-07(todo.md §16 Fix #49;三矩阵 480/497/480) |
| T30 | 5 | 分配漏斗回收-重试(alloc_pfn_reclaiming;C alloc_mem do-while 语义) | "24-page-cache" 停泊项 | ✅ 2026-09-07(todo.md §16 Fix #50;三矩阵 484/501/484) |
| T31 | 5 | 缺页计数生产者接线 + InfoUsage 槽位判定(Getrusage 为出口,VM_INFO wire C-parity) | vmproc_handle.rs:305 | ✅ 2026-09-07(todo.md §16 Fix #51;新登记 G-V12-6) |
| T32 | 6 | VFS transid 路径 C-parity 修复(真 bug:clean_type 门拒绝真实 transid 消息) | vm_server.rs:1227 | ✅ 2026-09-07(todo.md §16 Fix #52;三矩阵 486/503/486) |
| T33 | 6 | fork eager CoW——VM 侧完成(借用两相 + msgaddr 经 gateway Option);kernel 缺 msgaddr 出参 → **E-FORKMSG 登记** | T11 收尾 | ✅ 2026-09-07(todo.md §16 Fix #53;三矩阵 488/505/488) |
| T34 | 6 | MemType 收敛设计 → **判定闭合:保留 trait(C vtable 直接对应物;Redox Provider 类比不成立)** | V9-P2-3 | ✅ 2026-09-07(todo.md §16 Fix #55) |
| T35 | 7 | 剩余判定批次——注记批+失真批+per-backend 查询判定 ✅;余 heap-shrink 删除、G-V12-4 errno 直传(下轮,理由见 Fix #54) | todo.md §16 | 🔄 主体 ✅ 2026-09-07(todo.md §16 Fix #54) |
| T36 | 7 | 收尾回归:todo/edge 对账 + checklist §8 刷新 + Gate E + 四矩阵全绿 | 收敛审计 | ✅ 2026-09-07(todo.md §16 Fix #56;campaign 完结——T24–T36 全部闭环) |

---

## E3 VmBootHandoff 补 kernel text/data span(= 02-stage-vm V11-P2-7)

> **进度(2026-09-08,完成)**:`VmBootHandoff` 增 `kern_virt_base/kern_phys_base/kern_text_pages/kern_data_pages` 四字段(version 2 → 3;size 断言 ≤ 4096 仍通过);kernel `build_vm_handoff` 从 `kern_virt_base()/kern_phys_base()/kern_size()` 填充(minix-rs 内核映像为单一连续 span——text_pages = 全映像页数,data_pages = 0);VM `read_boot_params` 解析为 `BootParams.kernel_layout: Option<KernelLayout>`(handoff v≥3 → `kernel_layout()`,v≤2 → None);`init_global_state` 消费——`Some` 用真值,`None`(pre-E3 handoff/宿主测试)保留 mock 常量 + 审计告警。minix-types 访问器测试 ×2(v3 报告 span / v2 None)。**P1-4 实质闭环**:真实硬件上 `init_page_table` 的内核映射来自 boot handoff 而非硬编码 mock。余件:riscv64 Sv39 的 `VM_BOOT_HANDOFF_VA`(0x1_0000_0000 < 2^38 ✓ 已兼容)。

---

## E-RSSTART rs_start_t 字节 ABI pinning + copy_rs_start 解码(03-stage-rs RS_UP/RS_EDIT 臂,2026-09-07 登记)

> **改判(2026-09-07,用户批准并入 03-stage-rs campaign 后关单)**:阻塞前提
> ("`bitchunk_t`/`uid_t` 在本树无 typedef,rs_start_t 字节 ABI 不可 pinning")经独立
> 核实**不成立**——minix3/ 是完整 NetBSD 式全树:`bitchunk_t = uint32_t`
> (`minix3/sys/sys/types.h:124`,固定宽度、无架构依赖)、`uid_t = uint32_t`
> (types.h:221 + ansi.h:46)、`struct rs_start` 完整(`minix3/minix/include/minix/rs.h:104-151`)。
> 当初的 grep 只覆盖了 `minix3/minix/` 子树而漏掉 `minix3/sys/`。wire 解码面已落地
> (`minix-types::ipc::rs_start`,Fix #81:偏移常量单点表 + repr(C) 布局见证 +
> 39 个 offset_of 编译期断言 + x86-64 LP64 数据模型声明);**RS_UP/RS_EDIT/
> RS_UPDATE 三臂接线转入 03-stage-rs/todo.md §21 campaign 执行(R4-R6),本条目
> 关单**。原文的"约 230 字节"与 ILP32 假设作废(实际 `sizeof(struct rs_start)` = 920,LP64)。

**问题**:RS 的 `RS_UP`(do_up,request.c:15-106)与 `RS_EDIT`(do_edit,request.c:298-385)第一步都是 `copy_rs_start`——把调用方内存里的完整 `struct rs_start`(rs.h:107-166,约 230 字节:rss_flags/rss_cmd/rss_uid/位图数组/irq·io·pci 表/rss_label/…)按 C ABI 整结构拷入 RS。该结构含 `bitchunk_t rss_system[SYS_CALL_MASK_SIZE]`、`bitchunk_t rss_vm[VM_CALL_MASK_SIZE]` 与 `uid_t rss_uid`,而 `bitchunk_t` 在本 minix3 子树**只有使用没有 typedef**(bitmap.h:12 引用 `sizeof(bitchunk_t)`,全树 grep 无定义),`uid_t` 亦属 sys/types.h 外部类型——字节偏移无法从本树 pinning,猜偏移违反 Ground Truth 链(同 E-RSWIRE 判据)。

**影响**:13-rs-control-requests 的 `do_up`/`do_edit` 两臂停在缝上:权限/查槽/编排(create_service/edit_slot/run_service——决策与编排已全就绪,Fix #46-#52)就等这条解码;RS 侧其余 label 型控制臂(down/refresh/restart/clone/unclone/lookup/fi/getsysinfo/sysctl)已全部 live(Fix #71/#74/#75/#76),不依赖本条。

**解锁后工作(约一个完整迭代)**:
1. 从完整 Minix3 源码树 pin `bitchunk_t`/`uid_t` 尺寸 → 计算 `rs_start_t` 偏移表(逐字段断言测试锚定字节布局,风格同 E-RSWIRE 的 RprocpubWire);
2. minix-types 增 `RsStartWire`(repr(C))+ `decode` + 偏移断言;
3. rs 侧 `RsServer::do_up`/`do_edit` 接线:label 改取 rs_start 内的 rss_label,编排消费 `check_create_preconditions`/`create_service`/`edit_slot`/`run_service` 全链(sched_stop→edit_slot→privctl(UpdateSys)→sched_init 序列含 E-7 的类型化锚)。

**依赖**:~~完整 Minix3 C 源码参照(或补全本树头文件中 `bitchunk_t`/`uid_t` 的定义链)~~(已解除——定义在树内);无 E1/E2 依赖(解码纯单测可验证)。

---

## E-VMMCPWIRE vmmcp 消息族字段宽度修正:reply.addr u32 → 64 位(02-stage-vm V12 轮登记,2026-09-08)

**问题**:minix-types 的 `MessVmmcpReply.addr` 是 `u32`(`os/libs/minix-types/src/ipc/message.rs:2512-2515`),而 C 的对应字段是 `void *addr`(`minix3/minix/include/minix/ipc.h:2395-2400`,x86_64 上 64 位;C 赋值 `msg->m_vmmcp_reply.addr = (void *) vr->vaddr`,mem_cache.c:170)。VM 侧编码随之截断:`reply.addr = addr.0 as u32`(`os/servers/vm/src/vm_server.rs:1704`),而 mapcache 的分配地址走 MMAP 窗口(`MMAP_BASE = 0x1_0000_0000`,`os/servers/vm/src/mmap.rs:204`)——**高 32 位恒非零,截断恒发生**,属必现 wire bug 而非边角。同簇疑点:`mmap.rs:373` 的 `length: aligned_len.0 as u32`(>4GB 映射静默截断),以及 `mess_vmmcp` 请求方向字段宽的逐字段核查。

**为何 edge**:minix-types 消息布局是共享契约(edge 判定①类)——字段加宽是 wire ABI 变更,消费面(未来 minixfs/lib 的 vm_map_cacheblock 等价物,C 侧 libsys/vm_cache.c:47-54)尚未存在,现在改零成本、通电后改即破坏二进制契约。

**建议**:(1) `MessVmmcpReply.addr: u32 → u64`(对齐 C `void *`),VM 编码去截断;(2) 对照 `mess_vmmcp`/`mess_vmmcp_reply` 原始结构逐字段核查请求/回复两个方向(含 `_ASSERT_MSG_SIZE` 对应的 56 字节 payload 断言);(3) wire 回放测试断言大地址高位保全;(4) 顺手按"pattern 84 候选"(02-stage-vm/todo.md §17.6)对 VM 消息族做一次系统性字段宽度对账,同类问题一次清完。

**解锁**:02-stage-vm/todo.md V12-P1-3(VM 侧半边);E5(b) VFS 缓存协作链的正确性前提。

> **进度(2026-09-08,✅ 闭单)**:建议 (1)(3) 已落地——`MessVmmcpReply.addr: u64`(`addr @0, flags @8, padding[47]`,56 字节保持),VM `encode_reply_data` 去截断,`VfsRequest.length` 同批拓宽 u64;测试 `test_vmmcp_reply_layout_64bit_addr`(minix-types)+ `test_encode_mapcache_reply_preserves_high_addr_bits`(vm_server)。依据记录:02-stage-vm/todo.md §17.9 Fix #59。**余件转入低优先**:建议 (4) 的 VM 消息族系统性字段宽度对账(pattern 84 候选)——`mess_vmmcp` 请求方向初查字段类型与 C 一致(dev/off/ino 皆 64 位 + block/flags_ptr 指针宽待 minix-sys 消费时定),留作后续扫描项(edge_todo.md 收尾批跟踪),不阻塞通电。
