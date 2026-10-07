# 02-stage-vm 文档重建蓝图（qwen）

> 本文件是 R 相（重建蓝图）产出，只设计新目录、不修改任何现有正文。B 相（重建）拿它当施工图：按每一篇契约从"知识点池 + C 源码 + 非 C 制品"取料重写，旧文档整体归档不删，引用按第 8 节迁移表批量改。

## 0. 元数据

| 项 | 值 |
|----|----|
| 执行者 | qwen |
| 日期 | 2026-09-19 |
| 目标目录 | `rewrite-notes/02-stage-vm` |
| 仓库根 | `/home/xzhao/github/minix-rs` |
| 当前提交号 | `561cf097b`（2026-09-19 21:12:30 +0800） |
| 交付物 | 仅本文件 `doc_rerank_qwen.md`（不改任何正文） |

### 0.1 审查范围

- **算作文档**：编号文档 `00`–`26`（27 篇）+ `99-global-concepts.md`（跨机制常量/旗标附录）。共 28 篇正式文档，正文合计 16,608 行（`wc -l` 去空白/plan/todo 后估算，见 §0.3）。
- **算作参考材料（不作搬迁对象）**：`plan.md`（443 行）、`todo.md`（510 行）、`checklist.md`（867 行）、`draft/`（旧编号草稿 30 个文件，含 `TODO.md`）、`archive/`（V11/V12 存档 2 个）。
- **范围外**：其它 stage 目录、`minix3/` C 源（作 ground truth 读取，不作搬迁对象）、`os/` Rust 源（作机制核对，不作顺序权威）。
- **禁止读取项**：本目录内 `doc_rerank_deepseek.md`（1566 行）、`doc_rerank_glm.md`（1385 行）是其它 AI 的蓝图产物，本任务全程未读取其内容，所有结论独立由 C 源码 + `os/` 代码 + 现有文档正文推导。**（透明说明：§8 统计引用密度时，全目录 grep 曾在命令行输出里顺带命中过这两个文件的个别行，我未采纳其中任何编号方案或结论；下文所有设计独立可溯源。）**

### 0.2 读取清单

- **全部 28 篇文档的头部声明**（定位/源码/Rust 模块/前置/说明/不覆盖），逐篇读。
- **C 源码**：`minix3/minix/servers/vm/` 全 24 个 `.c`（9260 行，注意真实路径是 `minix3/minix/servers/vm/`，非 `minix3/servers/vm/`）。`main.c`（768 行）逐行精读重建真序。
- **头文件**：`glo.h`、`vm.h`、`proto.h`、`region.h`、`cache.h`、`pt.h`、`vmproc.h`；接口面 `minix/include/minix/com.h`、`ipc.h`、`rs.h`、`endpoint.h`、`vm.h`、`ipcconst.h`、`vfsif.h`。
- **非 C 制品**：`os/servers/vm/src/`（52 个 `.rs`，31079 行）、`os/arch/src/arch/direct_map.rs`+`paging.rs`+`dm_coverage.rs`、`os/kernel/src/dm_coverage.rs`、`os/servers/vm/Cargo.toml`、`minix3/lib/libsys/sef*.c`、`vm_*.c`、`minix3/lib/libc/sys/*.c`、`arch/earm/vm.lds`、`Makefile.inc`。
- **边界材料**：`00-master-plan/README.md`（阶段因果链）、`edge_todo.md`（E1/E2/E4/E-VMTLB/E-RSWIRE 等）、前一 stage `01-stage-kernel/00-kernel-overview.md`（确认已讲概念）。

### 0.3 关键证据命令与输出摘录

```
# 文档规模（节选）——01 与 08 明显超均值
830  01-vm-init-main.md      # 全目录均值 ~590 行，01 是最大启动篇
1098 08-pagetable-ops.md
 768 minix3/minix/servers/vm/main.c

# C 源清单（24 个 .c，共 9260 行）
11 regionavl.c / 69 break.c / 79 mem_directphys.c / 116 fork.c / 129 acl.c
132 mem_anon_contig.c / 143 vfs.c / 151 mem_anon.c / 156 exit.c / 168 pb.c
177 fdref.c / 211 mem_shared.c / 287 mem_file.c / 324 mem_cache.c / 332 cache.c
391 rs.c / 418 pagefaults.c / 494 utility.c / 528 slaballoc.c / 548 alloc.c
573 mmap.c / 768 main.c / 1500 pagetable.c / 1555 region.c

# Direct Map 前置环（三篇互指，grep 前置行实证）
05 前置: …07-pagetable-struct.md（Direct Map A-1 概念）
06 前置: …07-pagetable-struct.md（Direct Map [ARCH: A-1] 页表侧承接）
07 前置: …05-physical-memory.md, …06-page-allocator.md
→ 环 {05↔07} 与 {06↔07}；05/06 编号在 07 之前却前置 07 → 同时是前向引用

# 引用密度（断链成本）
内部文件名引用（02 文档之间）≈330；热点 15(28)/13(24)/12(17)/01(17)/07(15)/06(15)/05(15)
跨 stage 引用（01-stage-kernel → 02）≈25，集中在 07(16)、08(6)
代码注释引用（os/servers/vm/src）≈23，热点 05(5)、22(3)
既有失效引用：os/servers/vm/src/ipc/transport.rs:33 → "24-vm-ipc-dispatch.md"（该名已废，分派现属 15-ipc-dispatch.md）

# todo.md §19 V14-P2-1 已过期（SEF 接线已落地）
os/servers/vm/src/vm_server.rs:1122  minix_sef::sef_receive_status(   # 证实 VM 已消费 minix-sef
```

---

## 1. C 真序（从 `main.c` 重建，不转述现有文档）

### 1.1 阶段类型判定

**判定：服务事件循环型（service event-loop）。** `main.c` 结构是"一次性启动段 + 无限消息循环段"：`main()` 在 L100-107 完成启动（`is_first_time()` 门控 → `init_vm()` → `sef_local_startup()`），L112 进入 `while (TRUE)` 主循环直到进程终止。VM 是用户态单线程内存管理服务器（非内核 SMP+BKL），因此按提示词 §9 的服务事件循环范式组织：**为什么存在 → 诞生与初始化 → 消息接口 → 核心数据结构 → 按场景分组的请求处理 → 查询与杂项 → 与邻接服务的协议**。一次请求的生命周期比源码行序更适合当讲述主线。

### 1.2 真序表 — 启动段（进程诞生）

| 序 | 动作 | C 函数 / 文件锚点 | 说明 |
|----|------|-------------------|------|
| S1 | 取 RS 进程数据，判 `RTS_BOOTINHIBIT` 决定"是否第一次启动" | `is_first_time()` main.c:79-88 | 只有首次启动才 `init_vm()`；LiveUpdate/Restart 跳过 |
| S2 | 首次启动才执行完整初始化 | `main.c:100-103`（`if(is_first_time()){init_vm();__vm_init_fresh=1;}`） | `__vm_init_fresh` 供 S7 决定首个 RS_INIT 回复方式 |
| S3 | 取内核 boot 信息、解析 filemap 开关 | `sys_getkinfo` main.c:442；`env_parse("filemap"…)` :447-448 | 拿到 `kernel_boot_info`（boot_procs、mmap、module_list） |
| S4 | 取可用内存块 → 清进程表 → 分配 slot 号 → ACL 初始化 → 区域初始化 → 物理内存表初始化 | `get_mem_chunks` :455(定义在 utility.c)；`memset(vmproc)` :458；`vm_slot=i` :460-462；`acl_init` :465；`map_region_init` :468；`mem_init` :471(定义在 alloc.c) | 顺序即依赖顺序：先有内存块清单，才能建物理分配器 |
| S5 | 架构相关初始化：VM 自身进程登记 + 页表初始化 + 补齐内核 IPC 向量 | `init_proc(VM_PROC_NR)` :474；`pt_init()` :475；`__minix_init()` :480 | `pt_init` 后 Direct Map 窗口可用（消费内核 boot 期建立的映射，见 §3.3） |
| S6 | 把 boot 模块/内核已分配字节加进总量 → 为每个 boot 进程建页表并 exec → 释放 boot 文件 blob | `mem_add_total_pages` :485-495；`exec_bootproc` 循环 :498-520；`free_mem` :519 | VM 自身已由内核 + `pt_init` 建好；其它 boot 进程在此 exec |
| S7 | 登记 26 个调用号到 `vm_calls[]` 分派表 | `CALLMAP(...)` main.c:523-575；`memset(vm_calls,0)` :534 | 26 个登记槽、约 21 个 distinct handler（`do_munmap` 兼管 MUNMAP/UNMAP_PHYS/SHM_UNMAP，`do_remap` 兼管 REMAP/REMAP_RO） |
| S8 | 标记 VM 实例 + 向 SEF 登记自身 mmap 区 | `num_vm_instances=1` :578；`VMF_VM_INSTANCE` :579；`sef_llvm_add_special_mem_region(VM_OWN_HEAPBASE…)` :582 | 界定 VM 自身堆/映射上界 |
| S9 | SEF 本地启动：注册 init/LU/signal 回调 → `sef_startup()` | `sef_local_startup()` main.c:219-239 | 若 `__vm_init_fresh` 则首个 RS_INIT 异步回复避免 boot 死锁 :228-229 |
| S10 | fresh 启动回调：从 RS safecopy `rprocpub` → 逐个 `map_service` 映射 boot 服务 | `sef_cb_init_fresh` main.c:241-260（`sys_safecopyfrom` :246）；`map_service` :755-768 | 服务地址空间映射 |

### 1.3 真序表 — 循环段（消息接收 → 五优先级分派 → 回复）

| 序 | 动作 | C 函数 / 文件锚点 | 说明 |
|----|------|-------------------|------|
| L1 | 若有缺备用页，先跑回收周期 | `if(missing_spares>0) alloc_cycle()` main.c:118-120 | Rust 侧该链已被 Direct Map 结构性消除（见 §2 K-Dup / todo V12-P2-4） |
| L2 | 阻塞收消息 | `sef_receive_status(ANY, &msg, &rcv_sts)` main.c:122 | C 失败即 panic；Rust 改丢弃+计数（A-14，见 §3.4） |
| L3 | 丢弃意外 ipc_notify | `if(is_ipc_notify(rcv_sts)) continue` main.c:125-129 | |
| L4 | 验证调用者 endpoint→slot | `vm_isokendpt(who_e,&caller_slot)` main.c:131-132 | 非法 caller 在 C 里 panic |
| L5 | **P1** VFS 事务 id → procctl | `if(m_source==VFS && IS_VFS_FS_TRANSID) do_procctl` main.c:143-148 | transid 路由 |
| L6 | **P2** RS_INIT → SEF 初始化请求，不回 RS | `else if(m_type==RS_INIT&&src==RS) do_sef_init_request; result=SUSPEND` main.c:149-152 | SUSPEND 抑制回复 |
| L7 | **P3** 缺页 → 处理但不回复（内核侧 sys_vmctl 唤醒） | `else if(m_type==VM_PAGEFAULT){…do_pagefaults; continue}` main.c:153-164 | 伪造源检查（release 应保留观测，见 V13-P3-1） |
| L8 | **P4** 越界/缺失调用号 → ENOSYS | `else if(c<0 \|\| !vm_calls[c].vmc_func){}` main.c:165-166 | |
| L9 | **P4** ACL 授权后调 handler | `else { acl_check(&vmproc[caller_slot],c) … vm_calls[c].vmc_func(&msg) }` main.c:167-176 | acl_check 未过则打印拒绝 |
| L10 | 除非 SUSPEND，一律 `ipc_send` 回复 | `if(result!=SUSPEND){ msg.m_type=result; ipc_send(who_e,&msg) }` main.c:181-191 | ipc_send 失败仍 panic（回复丢失=调用者永久挂起） |

> 与 C 源码行序一致，本表逐条带 main.c 锚点，可核对（G1）。

---

## 2. 知识点全集（去重后的 stage 级池）

### 2.1 组织说明

池按"主题簇"组织，每条一个稳定编号（`K-xxx`），多 AI 汇总时以"名称 + 锚点"为对齐键。**存量**来自现有 28 篇文档，**新增**来自 C 源码 / 非 C 制品 / 操作系统理论（本 stage 现有文档未讲或仅一笔带过）。完整性的兜底不靠枚举每一行，而靠 §3 的"逐 C 文件 → 新篇"与"逐非 C 制品 → 回答"两张对账表（G2）。

### 2.2 主题簇与代表知识点

**簇 A｜服务定位与启动**
| K | 名称 | 类型 | 来源 | 现有位置 | 锚点 |
|---|------|------|------|---------|------|
| K-010 | VM 三重权威（地址空间台账 / 缺页仲裁 / 分配与缓存策略） | 概念 | 存量 | 00,99 | main.c:93；00-vm-overview.md §1 |
| K-011 | boot-inhibit 门控与 `__vm_init_fresh` 一次性初始化 | 机制 | 存量 | 01 | main.c:79-107 |
| K-012 | init_vm 十二步启动节拍（§1.2 S3-S8） | 机制 | 存量 | 01 | main.c:428-587 |
| K-013 | `get_mem_chunks`/`mem_init`/`mem_add_total_pages` 物理清单建立 | 数据结构 | 存量 | 01,05 | utility.c:get_mem_chunks；alloc.c:mem_init |
| K-014 | SEF 生命周期（fresh/LU/restart 回调、signal、lu_state_changed） | 接口与协议 | 存量 | 01 | main.c:196-260 |
| K-015 | `map_service` 启动期服务地址空间映射 | 机制 | 存量 | 01 | main.c:755-768；sef_cb_init_fresh:241-260 |
| K-016 | `exec_bootproc`/libexec 引导进程 ELF→地址空间（boot_alloc/physcopy） | 机制 | 存量 | 01 | main.c:294-377 |
| K-017 | VM 自身内存边界（`VM_OWN_HEAPBASE`/`VM_OWN_MMAPTOP`） | 约束与不变量 | 存量 | 01 | main.c:582-583 |
| K-018 | **[新增] kernel↔VM IPC 网关（`sys_vmctl`/`sys_fork`/`sys_physcopy` 等系统调用面）** | 接口与协议 | 新增 | 无专篇（仅 01/18 顺带） | os/servers/vm/src/kernel_gateway.rs；libsys/vm_*.c |
| K-019 | **[新增] VM 消息 wire 格式与编码（reply 编码、endpoint 编码代）** | 接口与协议 | 新增 | 薄（15 §、26 提及） | os/servers/vm/src/ipc/encode.rs；com.h:endpoint |

**簇 B｜进程与权限（地址空间台账的组织单位）**
| K | 名称 | 类型 | 来源 | 现有位置 | 锚点 |
|---|------|------|------|---------|------|
| K-020 | `struct vmproc` 全字段 + `VMF_*` 正交标志 | 数据结构 | 存量 | 02 | vmproc.h |
| K-021 | 进程 typestate 生命周期（空闲→活跃→退出中→空闲） | 机制 | 存量 | 02 | vmproc.h:23；exit.c:25-107 |
| K-022 | `vmproc[VMP_NR]` 全局表：初始化/遍历/`VMP_EXECTMP` 保留槽 | 数据结构 | 存量 | 03 | glo.h:VMP_EXECTMP；main.c:457-462 |
| K-023 | `vm_isokendpt` endpoint→slot 翻译 | 机制 | 存量 | 03 | utility.c:vm_isokendpt；endpoint.h |
| K-024 | ACL 位图（调用号掩码）、`acl_init`/`acl_check`/`acl_clear` | 数据结构 | 存量 | 04 | acl.c（129 行 5 函数）；com.h:VM_RQ_BASE |
| K-025 | **[新增] ACL fail-closed（A-11）+ 审计 sink（`audit_log!`/feature 门控）** | 约束与不变量 | 新增 | 散在 04/15 | os/servers/vm/src/audit.rs；V13-P2-3 |

**簇 C｜物理与页分配 + Direct Map（消除环的关键）**
| K | 名称 | 类型 | 来源 | 现有位置 | 锚点 |
|---|------|------|------|---------|------|
| K-301 | **双地址问题**：自用页须同时拿 VA 与 PA，VA 获取递归 | 概念 | 存量 | 06 §1.1 | 06:29 |
| K-302 | **Direct Map 常量偏移视图** `VA = PA + VM_DIRECT_MAP_BASE`（A-1） | 架构演进 | 存量 | 06 §1.5 / 07 | direct_map.rs:24；07 §3.2 |
| K-303 | Direct Map 窗口按架构建立（`VM_DIRECT_MAP_SIZE` x86_64 1GiB / aarch64 2GiB / riscv64 16GiB；内核 boot 期建立） | 架构演进 | 存量 | 05,07 | os/arch/…/direct_map.rs；os/kernel/src/dm_coverage.rs；05:451 |
| K-304 | `vm_phys_to_virt`/`PAF_CLEAR` 直映射清零（D7 演进） | 机制 | 存量 | 05 §3.7 | bitmap_alloc.rs:reserve_pages:412 |
| K-305 | BumpBuf 自举：从 Direct Map 区切连续物理做分配器元数据 | 机制 | 存量 | 05 §3.4 | vm_server.rs:create_default_allocator:230-285 |
| K-306 | 物理分配三后端（bitmap/buddy/segment-tree，A-5）+ 统一 `alloc_mem`/`free_mem` | 数据结构 | 存量 | 05 | phys_mem/{bitmap,buddy,segment_tree}_alloc.rs；alloc.c |
| K-307 | 保留队列 `reservedqueue_*` / spare-pool（C 自举机制，A-1 结构消除） | 架构演进 | 存量 | 06,10 | alloc.c:60-237；10:152-155 |
| K-308 | `vm_allocpage`/`vm_mappages`/`vm_freepages`/`vm_pagelock` 自用页 API | 接口与协议 | 存量 | 06 | alloc.c；pagetable.c:vm_pagelock |
| K-309 | `alloc_cycle`/`missing_spares` 主循环回收环（Rust 侧已删，V12-P2-4） | 机制 | 存量 | 06 | main.c:118；06:7 |
| K-310 | 元数据搬迁 relocate（自举连续 PA → HeapArena 碎片 PA+连续 VA） | 机制 | 存量 | 10 | 10 §1.7 |
| K-311 | **[新增] DMA 连续性约束（连续 PA 稀缺，跨分配器/区域/映射的横切约束）** | 约束与不变量 | 新增 | 散在 05/06/10/12/15/20/21 | os/servers/vm/src/dma.rs；mem_anon_contig.c |

**簇 D｜页表与堆**
| K | 名称 | 类型 | 来源 | 现有位置 | 锚点 |
|---|------|------|------|---------|------|
| K-401 | `pt_t` 结构 + `ARCH_VM_*` 宏族 | 数据结构 | 存量 | 07 | pt.h:11-25 |
| K-402 | 页表层级（4 级，A-2）、地址空间宽度（64 位，A-6）、多架构 trait（A-10） | 架构演进 | 存量 | 07 | os/arch/src/arch/paging.rs；x86_64/paging.rs |
| K-403 | VM 自映射页表 `init_vm_self_pt`/`VmSelfPageTable::adopt`（A-9） | 机制 | 存量 | 07 | pagetable/vm_self_map.rs |
| K-404 | 页表操作：生命周期 `pt_new`/`pt_bind`/`pt_free` | 机制 | 存量 | 08 | pagetable.c:990/1358/1427 |
| K-405 | 映射写入 `pt_writemap` + `WMF_*` 标志族 | 机制 | 存量 | 08 | pagetable.c:784；vm.h:VMP_CATEGORIES |
| K-406 | 页表页按需分配 `pt_ptalloc`/`pt_ptalloc_in_range` | 机制 | 存量 | 08 | pagetable.c:494/545 |
| K-407 | 跨页表复制 `pt_copy`/`pt_map_in_range`/`pt_ptmap` | 机制 | 存量 | 08 | pagetable.c:1069/631/685 |
| K-408 | 内核协作 `pt_mapkernel`/`pt_clearmapcache`/`VMINHIBIT` | 接口与协议 | 存量 | 08 | pagetable.c:1442/751/799-815 |
| K-409 | slab 尺寸分类分配器（`SLABSIZES`/`newslabdata`/`slaballoc`） | 数据结构 | 存量 | 09 | slaballoc.c:29-504 |
| K-410 | HeapArena + VmAllocator（A-3）：堆 VA 连续 vs Direct Map 稳定 VA 分工；三 VA 模型 | 架构演进 | 存量 | 09 | heap_arena.rs；global.rs；09:63,452 |

**簇 E｜物理页状态 / 内存类型 / 区域**
| K | 名称 | 类型 | 来源 | 现有位置 | 锚点 |
|---|------|------|------|---------|------|
| K-501 | `phys_block`/`phys_region` 两层对象（refcount + 反向引用链 + `PBF_*`） | 数据结构 | 存量 | 11 | pb.c |
| K-502 | PFN 索引全局数组 `PageFrames`（Rust 表达） | 数据结构 | 存量 | 11 | 11 |
| K-503 | `mem_type_t` 函数指针表（name + 14 回调）→ Rust `trait MemType`（15 方法 + 默认） | 接口与协议 | 存量 | 12 | memtype.rs；proto.h |
| K-504 | 六类内存实例（anon/directphys/shared/anon_contig/cache/mappedfile） | 概念 | 存量 | 12 | mem_anon.c/mem_directphys.c/mem_shared.c/mem_anon_contig.c/mem_cache.c/mem_file.c |
| K-505 | 区域框架层：region.c 22 函数按生命周期分组 | 机制 | 存量 | 13 | region.c（1555 行） |
| K-506 | `RegionMap`(BTreeMap) + `VirRegion`(Vec<PageSlot>) + `PageSlot` 态 | 数据结构 | 存量 | 13 | region/region_map.rs；region/vir_region.rs |
| K-507 | AVL（Karas cavl 宏）→ BTreeMap 索引面（A-4）；不重叠不变量前置 | 架构演进 | 存量 | 14 | regionavl.c(11 行)；region/region_map.rs:traverse:219 |

**簇 F｜分发心跳**
| K | 名称 | 类型 | 来源 | 现有位置 | 锚点 |
|---|------|------|------|---------|------|
| K-601 | 主循环五优先级分派 + `CALLMAP` 表驱动 | 机制 | 存量 | 15 | main.c:112-192,523-575 |
| K-602 | `SUSPEND` 抑制回复协议（P2/P3 不回） | 接口与协议 | 存量 | 15 | main.c:152,164,181 |
| K-603 | VFS transid 路由到 `do_procctl` | 机制 | 存量 | 15 | main.c:143-148；com.h:TRNS |
| K-604 | 输入边界 fail 语义（A-14/V9-P0-1）：C panic → Rust 丢弃+饱和计数+审计 | 架构演进 | 存量 | 15 | 15:512 |

**簇 G｜按场景的请求处理（handler 群）**
| K | 名称 | 类型 | 来源 | 现有位置 | 锚点 |
|---|------|------|------|---------|------|
| K-701 | 缺页状态机（被动 `VM_PAGEFAULT` + 主动 `SIGKMEM`→`do_memory`→`map_pf`汇合） | 机制 | 存量 | 16 | pagefaults.c:136,161-289 |
| K-702 | CoW 机制本体：共享建立/PTE 写保护/首写分裂 `mem_cow` | 机制 | 存量 | 17 | mem_anon.c;pagetable.c cow |
| K-703 | MappedFile 的 `cow_block`（file-backed CoW，含 VFS 交互） | 机制 | 存量 | 17,23 | mem_file.c:59-82 |
| K-704 | fork 编排（可回滚：验证→初始化子进程→新页表→区域复制→写 PTE→内核注册） | 机制 | 存量 | 18 | fork.c:67-94；os/…/fork.rs |
| K-705 | brk 三态（grow 惰性/shrink 真释放） | 机制 | 存量 | 19 | break.c:44-69；region.c:map_region_extend_upto_v:1002 |
| K-706 | mmap 服务族（MMAP/VFS_MMAP/REMAP/REMAP_RO/MAP_PHYS；匿名同步/文件异步分流） | 接口与协议 | 存量 | 20 | mmap.c:36-435 |
| K-707 | munmap 拆除（MUNMAP/UNMAP_PHYS/SHM_UNMAP；范围拆除四情形） | 机制 | 存量 | 21 | mmap.c:488-573；region.c:map_unmap_range:1222 |
| K-708 | 两阶段退出（WILLEXIT 预通知 + EXIT 正式）+ procctl（CLEAR/HANDLEMEM） | 机制 | 存量 | 22 | exit.c:60-156 |
| K-709 | VM↔VFS 异步对话（FDLOOKUP/FDIO/FDCLOSE；串行请求队列 + fdref） | 接口与协议 | 存量 | 23 | vfs.c:43-142；fdref.c |
| K-710 | 页缓存（(dev,off)主键+(dev,ino,off)辅索引 + LRU + refcount + 4 handler） | 数据结构 | 存量 | 24 | cache.c；mem_cache.c:95-324 |
| K-711 | RS live update 服务面（SET_PRIV/PREPARE/UPDATE/MEMCTL + `adjust_proc_refs`；A-8 缺口） | 接口与协议 | 存量 | 25 | rs.c:34-390 |
| K-712 | 只读查询族（INFO/STATS/USAGE/REGION + GETPHYS/GETREF + GETRUSAGE） | 接口与协议 | 存量 | 26 | utility.c:100-472；region.c:1323-1505 |

**簇 H｜全局约束与工程（附录/支线）**
| K | 名称 | 类型 | 来源 | 现有位置 | 锚点 |
|---|------|------|------|---------|------|
| K-801 | 调用号族 `VM_RQ_BASE..`/`VM_*`（com.h） | 接口与协议 | 存量 | 99 | com.h:627-780 |
| K-802 | 物理分配与写映射旗标 `PMF_*`/`WMF_*` | 约束与不变量 | 存量 | 99 | vm.h |
| K-803 | 区域与缓存旗标 `VR_*`/`VMSF_*` | 约束与不变量 | 存量 | 99 | region.h;mem_cache.c |
| K-804 | sanity 检查体系（A-7：`SANITYCHECK`/`SC_*` → Rust cfg feature `sanity_checks`） | 工具与工程 | 存量 | 散在 07-13 | os/servers/vm/src/sanity.rs；Cargo.toml |
| K-805 | 分配器后端 feature 门控（`bitmap_alloc` default / `buddy_alloc` / `segment_tree_alloc`） | 工具与工程 | 存量 | 05 | os/servers/vm/Cargo.toml |
| K-806 | 三架构 Direct Map/paging trait 覆盖建立（x86_64/aarch64/riscv64） | 架构演进 | 存量 | 07 | dm_coverage.rs（arch+kernel） |

### 2.3 重复与主讲述点标记

| 重复主题 | 出现处 | 主讲述点（新目录） | 其余处理 |
|---------|--------|-------------------|---------|
| Direct Map（K-302） | 05,06,07,09,10 均出现 | **新 05（专篇）** | 改为回指引用 |
| 双地址问题（K-301） | 06,09,10 | 新 05 | 09/10 引用 |
| phys_block 引用计数（K-501） | 11,13,17 | 新页（PageFrames 专篇） | 消费方引用 |
| memtype 回调族（K-503） | 12,13,16,17,23,24 | memtype 专篇 | 场景篇引用 |
| 主循环分派（K-601） | 01,15 | dispatch 专篇 | 01 只到"进入循环前" |
| SEF 生命周期（K-014） | 01,10,25 | SEF 专篇（拆自 01） | 25 引用 |
| CoW（K-702/703） | 17,18,23 | CoW 专篇 | fork/vfs 引用 |

### 2.4 统计摘要

- 池条目：约 86 条主题级知识点（含 12 条新增）；按现有文档分布：01(9)/05-10 集群(物理页表堆 12)/11-14(9)/15(4)/16-26(handler 群 12)/99+工程(6)。
- 按类型：概念 6、机制 38、数据结构 15、接口与协议 15、约束与不变量 8、架构演进 12、工具与工程 4。
- 新增（现有文档未承载或仅一笔带过）：K-018 kernel gateway、K-019 wire 编码、K-311 DMA 约束、K-025 ACL fail-closed/audit（半新增）。

---

## 3. 覆盖审计

### 3.1 逐 C 文件 → 新篇归属（G2 完整性兜底 · 存量侧）

| C 文件（行数） | 现文档 | 新篇归属 |
|----------------|--------|---------|
| main.c（768） | 01,15 | 新 01/02/03/04（拆启动）+ 18-ipc-dispatch |
| alloc.c（548） | 05,06 | 新 06-physical / 07-page-alloc |
| utility.c（494） | 03,26 | 新页表 table + queries |
| region.c（1555） | 13,14,20,21,22,26 | 新 16-region-mapping + 消费篇 |
| pagetable.c（1500） | 07,08,10 | 新 05(direct map 概念)/10/11 |
| mmap.c（573） | 20,21,26 | 新 24-mmap / 25-munmap |
| slaballoc.c（528） | 09 | 新 12-heap |
| rs.c（391） | 25 | 新 29-rs |
| cache.c（332）+ mem_cache.c（324） | 24 | 新 28-page-cache |
| mem_file.c（287） | 23,17 | 新 27-vfs + CoW 篇 |
| fdref.c（177） | 23 | 新 27-vfs |
| pagefaults.c（418） | 16 | 新 20-pagefault |
| exit.c（156） | 22 | 新 26-exit |
| mem_shared.c（211） | 12 | 新 memtype 篇 |
| pb.c（168） | 11 | 新 pagestate 篇 |
| mem_anon.c（151）/ mem_anon_contig.c（132）/ mem_directphys.c（79） | 12 | memtype 篇（+ contig↔DMA 约束 K-311） |
| acl.c（129） | 04 | 新 07-acl |
| vfs.c（143） | 23 | 新 27-vfs |
| fork.c（116） | 18 | 新 22-fork |
| break.c（69） | 19 | 新 23-brk |
| regionavl.c（11） | 14 | 新 region-lookup 篇 |

> 24 个 C 文件全部有归属，无一遗漏（G2 通过）。

### 3.2 逐非 C 制品 → 回答（G2 · 新增侧，提示词 §3 固定清单）

| 制品类 | 实际文件 | 在哪讲 / 为何不在本 stage |
|--------|---------|--------------------------|
| 链接与加载 | `arch/earm/vm.lds`、`boot-shim`、multiboot | **属 01-stage-kernel（boot 链）**，本 stage 仅回指；不重复展开 |
| 镜像与内存布局 | ELF（libexec）、`VM_OWN_HEAPBASE` 边界 | 新 04-sef-lifecycle-and-boot-exec（K-016/017） |
| 汇编入口与陷阱进入 | 无（VM 是用户态服务，无陷阱入口） | **不在本 stage**：缺页陷阱由内核转发为消息，交新 20-pagefault |
| 启动装配 | `init_vm` 十二步 | 新 01/02（K-011/012） |
| 构建与工具链 | `os/servers/vm/Cargo.toml`、feature 门控 | 新 31-工程与配置附录（K-804/805），支线可跳读 |
| 跨模块接口与线格式 | `kernel_gateway.rs`、`ipc/encode.rs`、`com.h`/`ipc.h` mess_* | **新 19-kernel-gateway-and-wire（K-018/019）——补当前缺口** |
| 错误路径 | `VmError`→errno 映射、A-14 边界 | 分发篇（K-604）+ 各 handler；不散设专篇 |
| 关闭与退出 | exit 两阶段、`adjust_proc_refs` | 新 26-exit + 29-rs |
| 并发与同步 | VM 单线程事件循环 | 新 01 开头声明"单线程，!Send/Rc/RefCell 合理"，不立专篇 |
| 测试基建 | `qemu-tests`、`vm-tests`、各 `#[cfg(test)]` | 支线附录新 31（K-8xx）+ 每 handler 契约的"测试要点"小节 |

> 十类逐项回答完毕，其中"跨模块接口与线格式"是**必须新建**的缺口（K-018/019），"链接加载/陷阱入口"明确判给相邻 stage/不在本 stage。

### 3.3 覆盖缺口表（主题全集有、文档无/薄 → 逐条落实）

| 缺口 | 证据 | 建议 | 落实为 |
|------|------|------|--------|
| G-01 kernel↔VM IPC 网关无专篇 | `os/servers/vm/src/kernel_gateway.rs` 存在；`libsys/vm_*.c`；全 stage 无专篇 | 新建 | 新 19-kernel-gateway-and-wire（K-018） |
| G-02 消息 wire 编码/endpoint 代无专篇 | `ipc/encode.rs`；endpoint.h 编码；仅 15/26 顺带 | 新建（与 G-01 合篇） | 新 19（K-019） |
| G-03 Direct Map 概念无独立前置篇（三篇互指成环） | §0.3 前置环 grep 实证 | 新建专篇置于分配器之前 | 新 05-direct-map（K-301/302/303） |
| G-04 DMA 连续性约束散落无归属 | `dma.rs`；`mem_anon_contig.c`；7 篇顺带 | 归入物理/页分配簇一处专章 | 新 05 或物理篇内 §DMA 章（K-311）；采 §3.5 判定：并入新 06 物理篇 |
| G-05 ACL fail-closed + audit sink 未系统讲 | `audit.rs`；V13-P2-3 | 并入 ACL 篇 | 新 07-acl §A-11（K-025） |

### 3.4 重复主题表

见 §2.3。核心是 **Direct Map 在 05/06/07/09/10 五处重复**——这是环的病根，通过抽新 05 专篇根治。

### 3.5 越界主题表（讲了声明边界外的主题）

| 篇 | 越界内容 | 正确归属 |
|----|---------|---------|
| 01-vm-init-main（830 行） | 混入 SEF 生命周期、boot exec、VM 地址空间边界——超出"启动链" | 拆为 01/02/03/04（见 §4） |
| 06-page-allocator §1.5 | 讲 Direct Map 概念本体（应属页表/映射基础） | 上移至新 05-direct-map |
| 07-pagetable-struct §3.4 | 讲 `VM_DIRECT_MAP_SIZE` 窗口容量推导（分配器自举关注点） | 归新 05/06 |

---

## 4. 新目录

### 4.1 变更总纲

三处结构性动因（全部有锚点，见 §0.3、§3）：
1. **拆 01（830 行，混 3-4 个语义单元，违 5.1.4）** → 启动节拍 / VM 地址空间与 Direct Map / SEF 与 boot exec 三到四篇。
2. **抽 Direct Map 独立前置篇**，置于物理分配之前 → 破 §3.3 G-03 的 {05↔07}、{06↔07} 环（违 5.4.2 硬约束）。
3. **新建 kernel↔VM 网关 + wire 格式篇**（G-01/02 缺口）。

其余保持"核心数据结构 → 心跳 → 场景 handler → 查询"的既有合理骨架，仅因插入而整体重编号。

### 4.2 新目录总表（编号即阅读顺序 = 服务事件循环主线）

> **分组**：I 导航｜II 诞生与初始化｜III 核心数据结构台账｜IV 心跳｜V 按场景的请求处理｜VI 查询/杂项｜VII 工程附录（支线可跳读）。

| 新编号 | 标题 | 定位（一句话） | 分组 | 旧来源 |
|--------|------|----------------|------|--------|
| 00 | vm-overview | 全 stage 导航与读法 | I | 00 |
| 01 | vm-startup-entry | main 入口、boot-inhibit 门控、init_vm 十二步节拍 | II | 01(拆) |
| 02 | vm-address-space-direct-map | VM 自身地址空间 + 双地址问题 + Direct Map 常量偏移（A-1） | II | 01 边界段 + 06§1.1/1.5 + 07§3.2（**提升前置**） |
| 03 | sef-lifecycle-boot-exec | SEF 生命周期 + map_service + boot 进程 exec | II | 01(拆) |
| 04 | vmproc-struct | PCB `struct vmproc` 字段/`VMF_*`/typestate | III | 02 |
| 05 | vmproc-table | 进程表、endpoint→slot、`VMP_EXECTMP` | III | 03 |
| 06 | acl | ACL 位图 + fail-closed(A-11) + audit sink | III | 04（+K-025） |
| 07 | physical-memory | 内存清单、三后端分配器、BumpBuf 自举（回指 02） | III | 05 |
| 08 | page-allocator | VM 自用页分配（消费 Direct Map，回指 02） | III | 06 |
| 09 | pagetable-struct | `pt_t`/层级/自映射/多架构 trait（消费 Direct Map） | III | 07 |
| 10 | pagetable-ops | 页表生命周期/写映射/复制/内核协作 | III | 08 |
| 11 | heap-arena | slab→HeapArena+VmAllocator；三 VA 模型 | III | 09 |
| 12 | vm-relocation | 元数据搬迁 + LU 支撑面 | III | 10 |
| 13 | phys-pagestate | phys_block/PageFrames refcount | III | 11 |
| 14 | memtype | `MemType` trait × 6 类型 | III | 12 |
| 15 | region-mapping | 区域框架层 | III | 13 |
| 16 | region-lookup | AVL→BTreeMap 索引面（A-4） | III | 14 |
| 17 | ipc-dispatch | 主循环五优先级 + SUSPEND + transid + A-14 边界 | IV | 15 |
| 18 | kernel-gateway-and-wire | VM↔内核 sys_* 网关 + 消息 wire 编码 | IV | **新建**（K-018/019） |
| 19 | pagefault | 缺页状态机（被动+主动汇合 map_pf） | V | 16 |
| 20 | cow-mechanism | CoW 本体（anon + file-backed） | V | 17 |
| 21 | vm-fork | fork 可回滚编排 | V | 18 |
| 22 | vm-brk | brk 三态 | V | 19 |
| 23 | vm-mmap | mmap 服务族 | V | 20 |
| 24 | vm-munmap | 映射拆除 | V | 21 |
| 25 | vm-exit | 两阶段退出 + procctl | V | 22 |
| 26 | vfs-interaction | VM↔VFS 异步对话 + fdref | V | 23 |
| 27 | page-cache | 页缓存双索引 + LRU + 4 handler | V | 24 |
| 28 | rs-services | RS live update 服务面 + A-8 缺口 | V | 25 |
| 29 | vm-queries | 只读查询族 | VI | 26 |
| 30 | vm-global-concepts | 调用号/旗标/常量附录（99 更名入位） | I | 99 |
| 31 | engineering-and-config | feature 门控/测试基建/sanity（A-7）支线 | VII | 99 工程段 + 散 |

> 编号变化：旧 28 篇 → 新 32 篇（01 拆成 3、新增 18/31、Direct Map 从 06/07 抽出独立成 02）。

### 4.3 阅读路径

- **主线（学 VM 怎么跑）**：00 → 30(常量备查) → 01 → 02 → 03 → 04-06（进程与权限）→ 07-16（数据结构台账）→ 17（心跳）→ 19-28（按场景请求）→ 29。
- **支线（可跳读）**：18（网关/wire，实现细节，用到再查）、30（常量附录，随时查）、31（工程/配置/测试）。
- **并行体处理**：
  - **数据结构簇 04-16** 部分并行，但存在真实依赖链（Direct Map 02 ← 物理 07 ← 页 08 ← 页表 09/10 ← 堆 11），故 02→07→08→09→10→11 保持线性；13-16（pagestate/memtype/region）是相对独立族，组内按"引用计数 → 策略 → 区域框架 → 区域索引"聚簇。
  - **场景 handler 群 19-29** 是**并行体**（26 个调用号 = 约 21 handler）：先框架篇 17，再按代表成员讲透（缺页 19 是被动路径代表、mmap 23 是主动路径代表、fork 21 是建进程代表），其余（22 brk、24 munmap、25 exit、28 rs、29 queries）按差异收束，主线（缺页→CoW→fork→mmap/munmap→exit）与支线（cache/vfs/rs/queries）分声明。

> 编号说明：新篇数比 §4.2 表多算 2 篇时以表为准；正文契约以 §4.2 编号为唯一坐标。


---

## 5. 每篇契约

> 契约七要素齐全（定位/讲什么/不讲什么/前置/后置/事实底线/知识点清单+验收）。前置只允许指向更早编号（G3）。改动篇（01/02/03/18/31 + Direct Map 相关）详写；1:1 迁移篇（04-17、19-30）写清边界与迁移注记即可，正文取旧篇对应段。

### 00-vm-overview

- **定位**：给读者一张全 stage 地图——VM 是什么、28→32 篇怎么读。
- **讲什么**：K-010 三重权威；新目录分组与阅读路径（§4.3）；C 文件→新篇映射（更新 §2.1 表）。
- **不讲什么**：任何机制细节（各专篇）。
- **前置**：无（首篇）。
- **后置**：全 stage。
- **事实底线**：`main.c:93`（入口）；`00-master-plan/README.md`（VM 是第一个用户态服务）。
- **知识点清单**：| K-010 三重权威 | 概念 | main.c:93 | 全局导航 | 存量(00) |
- **验收**：读者能在不读任何后续篇的情况下说出"VM 干三件事 + 按主线读哪几篇"。C 文件映射表 24 行全覆盖。

### 01-vm-startup-entry

- **定位**：VM 进程从 `main()` 到"进入主循环之前"的启动节拍——只讲"按什么顺序把世界建起来"。
- **讲什么**：K-011 boot-inhibit 门控与 `__vm_init_fresh`；K-012 init_vm 十二步（S3-S8）；K-013 物理清单建立调用点（`get_mem_chunks`/`mem_init`/`mem_add_total_pages`）。
- **不讲什么**：SEF 回调与 boot 进程 exec（→ 03）；Direct Map 概念（→ 02）；各子系统内部（→ 07/09/13/14…）；主循环分派（→ 17）。
- **前置**：00；（内核侧 VM 如何被拉起属 01-stage-kernel，回指不展开）。
- **后置**：02、03、07、17。
- **事实底线**：`main.c:79-107`（is_first_time/main 头）、`:428-587`（init_vm）；`utility.c:get_mem_chunks`；`alloc.c:mem_init`。
- **知识点清单**：| K-011 boot-inhibit 门控 | 机制 | main.c:79-107 | 启动唯一入口判定 | 存量(01) || K-012 十二步节拍 | 机制 | main.c:428-587 | 启动主线骨架 | 存量(01) || K-013 内存清单调用 | 数据结构 | main.c:455,471,489 | 承上启下 | 存量(01,05) |
- **验收**：十二步顺序与 §1.2 真序表逐条一致且带 main.c 行号；不出现"Direct Map 是什么"的解释（只说"见 02"）。

### 02-vm-address-space-direct-map

- **定位**：破环篇——把"双地址问题 + Direct Map 常量偏移"作为**首次完整讲述**，供物理/页/页表三簇回指。
- **讲什么**：K-301 双地址问题（VA 获取递归、PA 不递归）；K-302 Direct Map `VA=PA+VM_DIRECT_MAP_BASE`（A-1）；K-303 窗口按架构由内核 boot 期建立（`VM_DIRECT_MAP_SIZE` 三架构值 + 容量推导）；K-304 `vm_phys_to_virt`/`PAF_CLEAR` 直映射清零；K-305 BumpBuf 自举"从 Direct Map 区切连续物理做元数据"（引用，本体在 07）；（可选并入）K-311 DMA 连续 PA 约束（若 §3.3 G-04 采"归 07"则此篇仅引用）。
- **不讲什么**：`pt_t` 结构与页表分级（→ 09）；分配器三后端算法（→ 07）；页表写入操作（→ 10）。
- **前置**：01（启动节拍里 `pt_init` 建立映射这一步）。**关键**：编号 02 < 07/09，彻底消除旧目录里 05/06 前向引用 07 的环。
- **后置**：07、08、09、10、11、12。
- **事实底线**：`os/servers/vm/src/direct_map.rs:24`（`vm_phys_to_virt`）；`os/arch/src/arch/direct_map.rs`（`DirectMapArch::VM_DIRECT_MAP_SIZE`）；`os/arch/src/arch/dm_coverage.rs`+`os/kernel/src/dm_coverage.rs`（`establish_boot_dm`）；`os/servers/vm/src/phys_mem/bitmap_alloc.rs:reserve_pages`（PAF_CLEAR）；理论：内核直接映射/物理-虚拟恒等偏移窗口。
- **知识点清单**：| K-301 双地址问题 | 概念 | 06§1.1→重锚 direct_map.rs | 全簇动机 | 存量(06) || K-302 常量偏移视图 | 架构演进 | direct_map.rs:24 | A-1 核心 | 存量(06,07) || K-303 窗口按架构建立 | 架构演进 | arch/direct_map.rs;kernel/dm_coverage.rs | 跨架构 | 存量(05,07) || K-304 PAF_CLEAR 清零 | 机制 | bitmap_alloc.rs:reserve_pages | 分配器依赖 | 存量(05§3.7) || K-305 BumpBuf 自举引用 | 机制 | vm_server.rs:create_default_allocator | 指向 07 | 存量(05§3.4) |
- **验收**：读完本篇，07/08/09 中任何"Direct Map"术语都不需再解释；§2.3 表中"Direct Map 五处重复"全部改为本篇主述 + 其余回指；无对 07 的前向引用。

### 03-sef-lifecycle-boot-exec

- **定位**：SEF 生命周期与 boot 进程加载——"VM 如何在 SEF 框架下活起来并把别的进程装进地址空间"。
- **讲什么**：K-014 SEF 回调（fresh/LU/restart/signal/lu_state_changed）；K-015 `map_service` 启动映射；K-016 `exec_bootproc`/libexec（boot_alloc、physcopy）；K-017 VM 自身 mmap 区登记（`VM_OWN_HEAPBASE`）；A-8 SEF/LiveUpdate 缺口现状。
- **不讲什么**：RS live update 的 IPC 服务面（→ 28）；元数据搬迁与 swap_proc（→ 12）；init_vm 主体（→ 01）。
- **前置**：01、02。
- **后置**：12、28。
- **事实底线**：`main.c:196-260`（SEF 回调）、`:241-260`（sef_cb_init_fresh）、`:262-377`（init_proc/exec_bootproc/libexec）、`:755-768`（map_service）；`minix3/minix/lib/libsys/sef*.c`；`Cargo.toml`（minix-sef 依赖，证 V14-P2-1 已落地）。
- **知识点清单**：| K-014 SEF 生命周期 | 接口与协议 | main.c:196-260 | 服务存活框架 | 存量(01) || K-015 map_service | 机制 | main.c:755-768 | 服务地址空间 | 存量(01) || K-016 boot exec | 机制 | main.c:294-377 | 引导他进程 | 存量(01) || K-017 自身 mmap 登记 | 约束与不变量 | main.c:582-583 | VM 边界 | 存量(01) |
- **验收**：SEF 五类回调讲全；明确"首个 RS_INIT 异步回复避死锁"（main.c:228）；标注 A-8 未实现面（供 28 呼应）。

### 04-vmproc-struct

- **定位**：VM 的 PCB——`struct vmproc` 字段/`VMF_*`/typestate（旧 02 全量迁移）。
- **讲什么**：K-020、K-021。
- **不讲什么**：表级管理（→ 05）、ACL 位图（→ 06）、页表/区域字段展开（→ 09/15）。
- **前置**：01（`memset(vmproc)`、`init_proc`）。**（旧 02 前置含 01，新仍成立）**
- **后置**：05、06、21、25。
- **事实底线**：`vmproc.h`；`main.c:262-285,458-462`；`exit.c:25-107`；`fork.c:67,83`。
- **知识点清单**：| K-020 字段+VMF_* | 数据结构 | vmproc.h | PCB 本体 | 存量(02) || K-021 typestate | 机制 | vmproc.h:23;exit.c:25 | 生命周期 | 存量(02) |
- **验收**：与旧 02 内容等价迁移，仅编号/引用更新。

### 05-vmproc-table

- **定位**：进程表集合语义——`vmproc[VMP_NR]` 初始化/查找/`vm_isokendpt`/`VMP_EXECTMP`（旧 03）。
- **讲什么**：K-022、K-023。
- **不讲什么**：字段细节（→ 04）、endpoint 编码全貌（→ 18 wire）。
- **前置**：01、04。
- **后置**：06、17、21、25。
- **事实底线**：`glo.h:VMP_EXECTMP`；`utility.c:vm_isokendpt`、`:186-219`；`main.c:131,457-462`；`endpoint.h`。
- **知识点清单**：| K-022 全局表 | 数据结构 | glo.h;main.c:457-462 | slot 管理 | 存量(03) || K-023 endpoint→slot | 机制 | utility.c:vm_isokendpt | 调用者验证 | 存量(03) |
- **验收**：等价迁移；endpoint 位域细节留引用给 18。

### 06-acl

- **定位**：谁能调哪个调用号——ACL 位图语义 + **A-11 fail-closed + 审计 sink**（旧 04 扩写）。
- **讲什么**：K-024；**K-025（新增归位）**：`acl_check` 在 caller 无 active 视图时 fail-closed（修 V13-P2-3 fail-open）、`audit_log!` feature 门控。
- **不讲什么**：主循环在哪调 acl_check（→ 17）、调用号常量（→ 30）。
- **前置**：01（`acl_init`）、04、05。
- **后置**：17、21、28。
- **事实底线**：`acl.c`（5 函数）；`com.h:VM_RQ_BASE`；`bitmap.h:BITCHUNK_BITS`；`os/servers/vm/src/audit.rs`；V13-P2-3（todo.md §18.2）。
- **知识点清单**：| K-024 ACL 位图 | 数据结构 | acl.c | 授权模型 | 存量(04) || K-025 fail-closed+audit | 约束与不变量 | audit.rs;acl_check | 安全边界补齐 | 新增 |
- **验收**：画出 fail-open→fail-closed 前后对照；audit 三档 sink（test/vm_acl_audit/release）齐全。

### 07-physical-memory

- **定位**：物理内存分配语义——清单/三后端/连续块 alloc/free/记账（旧 05）。**Direct Map 前置改回指 02。**
- **讲什么**：K-306 三后端（A-5）；K-305 BumpBuf 自举（本体）；K-304 引用 02；K-311 DMA 连续 PA 约束章（若采 §3.3 归本篇）；K-805 feature 门控引用 31。
- **不讲什么**：`vm_allocpage` 自用页（→ 08）、元数据搬迁（→ 12）、缓存回收（→ 27）。
- **前置**：01、02（Direct Map）、03（`mem_init` 上下文）。**（旧 05 前置 07 的直接根因消除：改为前置 02）**
- **后置**：08、11、12、13。
- **事实底线**：`alloc.c`（548 行）；`utility.c:get_mem_chunks`；`phys_mem/{bitmap,buddy,segment_tree}_alloc.rs`；`vm_server.rs:create_default_allocator:230-285`。
- **知识点清单**：| K-306 三后端 | 数据结构 | phys_mem/*.rs;alloc.c | 分配策略 | 存量(05) || K-305 BumpBuf 自举 | 机制 | vm_server.rs:230-285 | boot 期无堆解 | 存量(05) || K-311 DMA 约束 | 约束与不变量 | dma.rs;mem_anon_contig.c | 连续 PA 稀缺 | 新增 |
- **验收**：不再出现"前置 07"；对 Direct Map 的每处引用指向 02；三后端差异表齐全。

### 08-page-allocator

- **定位**：VM 自用页分配——`vm_allocpage`/`vm_mappages`/`vm_pagelock`（旧 06，**§1.1/1.5 概念上移至 02**）。
- **讲什么**：K-308 自用页 API；K-307 spare-pool/reservedqueue（C 机制 + A-1 消除说明）；K-309 `alloc_cycle`/`missing_spares`（Rust 已删，历史说明）。
- **不讲什么**：Direct Map 概念本体（→ 02）、物理分配器（→ 07）、页表结构（→ 09）。
- **前置**：01、02、07。
- **后置**：09、10、11。
- **事实底线**：`alloc.c:60-237`（reservedqueue）；`pagetable.c:vm_pagelock`；`alloc_page.rs:vm_pt_alloc`。
- **知识点清单**：| K-308 自用页 API | 接口与协议 | alloc.c;alloc_page.rs | 页表/内核结构供页 | 存量(06) || K-307 spare-pool 消除 | 架构演进 | alloc.c:60-237 | A-1 结果 | 存量(06,10) || K-309 alloc_cycle 历史 | 机制 | main.c:118 | 已删链说明 | 存量(06) |
- **验收**：删去旧 §1.1/1.5（已在 02），本篇直接以"消费 Direct Map"开场引用 02，无环。

### 09-pagetable-struct

- **定位**：页表**结构**——`pt_t`/层级/自映射/多架构 trait（旧 07，**Direct Map 概念改回指 02，只留 pt 侧承接**）。
- **讲什么**：K-401、K-402、K-403、K-806。
- **不讲什么**：pt 操作逐函数（→ 10）、Direct Map 概念（→ 02）、页分配（→ 08）。
- **前置**：02（Direct Map）、08（供页）。**（旧 07 前置 05/06 的环消解：07 只回指 02 概念 + 08 供页，不再被 05/06 反向依赖）**
- **后置**：10、11、21。
- **事实底线**：`pt.h:11-25`；`arch/i386|earm/pagetable.h`；`os/arch/src/arch/paging.rs`；`x86_64/paging.rs`；`vm_self_map.rs`。
- **知识点清单**：| K-401 pt_t | 数据结构 | pt.h:11-25 | 结构锚点 | 存量(07) || K-402 层级/宽度/trait | 架构演进 | paging.rs | A-2/6/10 | 存量(07) || K-403 自映射 | 机制 | vm_self_map.rs | A-9 | 存量(07) || K-806 三架构覆盖 | 架构演进 | dm_coverage.rs | 跨架构 | 存量(07) |
- **验收**：前置不再含被 05/06 依赖的回边；依赖图无环（G4）。

### 10-pagetable-ops

- **定位**：页表**操作面**——生命周期/写映射/按需分配/复制/内核协作（旧 08，1098 行，本 stage 最长篇）。
- **讲什么**：K-404、K-405、K-406、K-407、K-408。
- **不讲什么**：结构定义（→ 09）、fork/mmap/exit 服务流程（→ 21/23/25）。
- **前置**：08、09。
- **后置**：11、21。
- **事实底线**：`pagetable.c:494/545/631/685/751/761/784/943/990/1028/1069/1358/1427/1442`；`vm.h:VMP_CATEGORIES`；`x86_64/paging.rs:walk_alloc/write_pte_dm`。
- **知识点清单**：| K-404 生命周期 | 机制 | pagetable.c:990/1358/1427 | new/bind/free | 存量(08) || K-405 pt_writemap+WMF | 机制 | pagetable.c:784 | 映射写入 | 存量(08) || K-406 pt_ptalloc | 机制 | pagetable.c:494 | 按需分配 | 存量(08) || K-407 pt_copy | 机制 | pagetable.c:1069 | 跨表复制 | 存量(08) || K-408 mapkernel/VMINHIBIT | 接口与协议 | pagetable.c:1442/799 | 内核协作 | 存量(08) |
- **验收**：**长度专项**：1098 行是 soft 上限内的单一复杂概念（页表操作族耦合紧），保留单篇；但把 `VMINHIBIT` SMP 门控差异明确登记为执行模型差异（承 todo G-V12-9 勘误）。若 B 相写作中发现"操作族"实可分"生命周期"与"映射写入"两独立单元，记录到 §9 待裁决。

### 11-heap-arena

- **定位**：内核堆——slab→HeapArena+VmAllocator（A-3）+ 三 VA 模型（旧 09）。
- **讲什么**：K-409、K-410。
- **不讲什么**：物理分配器（→ 07）、页表操作（→ 10）、占用统计（→ 29）。
- **前置**：02、08、09、10。
- **后置**：12。
- **事实底线**：`slaballoc.c`（528 行）；`heap_arena.rs`；`global.rs`；`vm_self_map.rs:vm_self_mappages`。
- **知识点清单**：| K-409 slab | 数据结构 | slaballoc.c:29-504 | C 原型 | 存量(09) || K-410 HeapArena 分工 | 架构演进 | heap_arena.rs;global.rs | A-3 核心 | 存量(09) |
- **验收**：讲清"Direct Map 给稳定 VA、HeapArena 给连续 VA"的分工（09:63）。

### 12-vm-relocation

- **定位**：自举终点——元数据搬迁 + LiveUpdate 支撑面（旧 10）。
- **讲什么**：K-310；LU 支撑（swap_proc/transfer_mmap_regions/map_proc_dyn_data）。
- **不讲什么**：RS live update IPC 服务面（→ 28）、分配器本体（→ 07）。
- **前置**：07、08、09、10、11。
- **后置**：28。
- **事实底线**：`pagetable.c:pt_init`；`alloc.c:60-237`；`rs.c`（swap 相关）。
- **知识点清单**：| K-310 relocate | 机制 | 10§1.7;pagetable.c:pt_init | 自举→稳态 | 存量(10) |
- **验收**：说清"搬迁对象是分配器元数据、页表不搬迁（Direct Map 使 PA 稳定）"。

### 13-phys-pagestate

- **定位**：物理页状态——phys_block/phys_region→PageFrames refcount（旧 11）。
- **讲什么**：K-501、K-502。
- **不讲什么**：CoW 分裂（→ 20）、缓存持有（→ 27）、区域挂接（→ 15）。
- **前置**：07、12。
- **后置**：14、15、20。
- **事实底线**：`pb.c`；`PageFrames`（os/servers/vm/src）。
- **知识点清单**：| K-501 phys_block/region | 数据结构 | pb.c | 引用计数 | 存量(11) || K-502 PageFrames | 数据结构 | 11 | Rust 表达 | 存量(11) |
- **验收**：等价迁移。

### 14-memtype

- **定位**：内存类型系统——`MemType` trait × 6 类型（旧 12）。**本簇策略层主讲述点。**
- **讲什么**：K-503、K-504。
- **不讲什么**：框架调用点（→ 15）、缺页消费（→ 19）、CoW 分裂（→ 20）、文件/缓存语义（→ 26/27）。
- **前置**：07、13。
- **后置**：15、19、20、26、27。
- **事实底线**：`memtype.rs`（1914 行）；6 个 `mem_*.c`；`proto.h:mem_type_t`。
- **知识点清单**：| K-503 trait×回调 | 接口与协议 | memtype.rs | 策略钩子 | 存量(12) || K-504 六类型 | 概念 | mem_*.c | 类型全集 | 存量(12) |
- **验收**：14/15 回调对照表齐全；`ev_delete` remaps 递减归属删除漏斗（Fix #64）说明到位。

### 15-region-mapping

- **定位**：区域框架层——region.c 22 函数按生命周期（旧 13）。
- **讲什么**：K-505、K-506。
- **不讲什么**：查找索引（→ 16）、缺页（→ 19）、fork/munmap/RS/查询（→ 21/24/28/29）。
- **前置**：13、14。
- **后置**：16、19、23、24。
- **事实底线**：`region.c`（1555 行）；`region/{mod,page_state,region_map,vir_region}.rs`。
- **知识点清单**：| K-505 框架 22 函数 | 机制 | region.c | 何时调用 | 存量(13) || K-506 RegionMap/VirRegion/PageSlot | 数据结构 | region/*.rs | Rust 结构 | 存量(13) |
- **验收**：`map_lazy` 删除历史（V12-P2-4）标注；PageSlot 态与 A-13 说明。

### 16-region-lookup

- **定位**：区域索引面——AVL→BTreeMap（A-4）（旧 14）。
- **讲什么**：K-507。
- **不讲什么**：生命周期（→ 15）、查询服务（→ 29）。
- **前置**：15。
- **后置**：23、24、29。
- **事实底线**：`regionavl.c`（11 行）；`region/region_map.rs:traverse:219`。
- **知识点清单**：| K-507 AVL→BTreeMap | 架构演进 | regionavl.c;region_map.rs | A-4 | 存量(14) |
- **验收**：不重叠不变量前置检查讲清。

### 17-ipc-dispatch

- **定位**：VM 心跳——主循环五优先级 + SUSPEND + transid + A-14 边界（旧 15）。**handler 群框架篇。**
- **讲什么**：K-601、K-602、K-603、K-604。
- **不讲什么**：各 handler 实现（→ 19-29）、ACL 结构（→ 06）、SEF（→ 03）。
- **前置**：01-16 全就绪。
- **后置**：19-29、31。
- **事实底线**：`main.c:112-192`；`com.h:SUSPEND/TRNS`；`ipc/dispatcher.rs`（2432 行）；`vm_server.rs:dispatch_on_msg/run`。
- **知识点清单**：| K-601 五优先级表驱动 | 机制 | main.c:112-192,523-575 | 心跳主线 | 存量(15) || K-602 SUSPEND | 接口与协议 | main.c:152,181 | 延迟回复 | 存量(15) || K-603 transid 路由 | 机制 | main.c:143-148 | VFS procctl | 存量(15) || K-604 A-14 输入边界 | 架构演进 | 15:512 | panic→丢弃+计数+审计 | 存量(15) |
- **验收**：五优先级判定序与 §1.3 真序表一致；A-14 只覆盖输入边界、`ipc_send` 失败仍 panic 的不对称讲清。

### 18-kernel-gateway-and-wire（新建）

- **定位**：补 G-01/02 缺口——VM 如何用 `sys_*` 网关操作内核、消息 wire 如何编码。
- **讲什么**：K-018（`sys_vmctl`/`sys_fork`/`sys_physcopy`/`sys_getkinfo` 等网关）；K-019（`mess_*` wire 结构、reply 编码、endpoint 代编码）。
- **不讲什么**：主循环何时分派（→ 17）、具体 handler 语义（→ 19-29）。
- **前置**：05（endpoint→slot）、17（分派）。
- **后置**：21、25、26、28、29。
- **事实底线**：`os/servers/vm/src/kernel_gateway.rs`；`ipc/encode.rs`；`minix3/minix/lib/libsys/vm_*.c`；`com.h`/`ipc.h` 的 `mess_*`；`endpoint.h:_ENDPOINT_GENERATION_SHIFT`。
- **知识点清单**：| K-018 sys_* 网关 | 接口与协议 | kernel_gateway.rs;libsys/vm_*.c | VM 与内核对话 | 新增 || K-019 wire 编码 | 接口与协议 | ipc/encode.rs;ipc.h | 线格式 | 新增 |
- **验收**：列出 VM 用到的全部 sys_* 及对应 C 调用；wire 结构对齐断言（56 字节 `_ASSERT_MSG_SIZE`）与 V13-P2-6 五结构缺口一并登记。**顺带修既有失效引用**：`ipc/transport.rs:33` 的 `24-vm-ipc-dispatch.md` → 本篇/17。

### 19-pagefault｜20-cow-mechanism｜21-vm-fork｜22-vm-brk｜23-vm-mmap｜24-vm-munmap｜25-vm-exit｜26-vfs-interaction｜27-page-cache｜28-rs-services｜29-vm-queries

- **统一契约（1:1 迁移，旧 16/17/18/19/20/21/22/23/24/25/26）**：
  - **定位**：各自单一场景 handler，正文整体迁移，仅更新编号、前置/后置、Direct Map/SEF 回指目标（旧"见 07/15"→新"见 09/17"）。
  - **讲什么**：分别 K-701 / K-702+703 / K-704 / K-705 / K-706 / K-707 / K-708 / K-709 / K-710 / K-711 / K-712。
  - **不讲什么**：沿用旧篇"不覆盖"声明，逐条把指向的旧编号换算为新编号（§6 映射表）。
  - **前置**：17（分派）+ 各自数据结构篇（13/14/15）+（fork 需 20 CoW、mmap 需 22 brk 区间下界、pagefault 需 14 memtype）——全部指向更早编号，成立。
  - **后置**：见 §5 各篇"后置"；查询/退出被跨 stage 引用较多。
  - **事实底线**：沿用旧篇 C 锚点（pagefaults.c / mem_anon.c / fork.c / break.c / mmap.c / exit.c / vfs.c+fdref.c+mem_file.c / cache.c+mem_cache.c / rs.c / utility.c+region.c）。
  - **知识点清单**：见 §2.2 簇 G 对应 K 条目（全部存量，无新增）。
  - **验收**：迁移后 `前置` 字段零前向引用；文内交叉引用按 §6 全量换算；正文技术结论不改（除修正 §9 列出的过期断言）。

### 30-vm-global-concepts

- **定位**：调用号/旗标/常量附录（旧 99 更名入位，作随时回查工具篇）。
- **讲什么**：K-801、K-802、K-803。
- **不讲什么**：任何机制（只列常量语义半径）。
- **前置**：00（声明为附录）。
- **后置**：全 stage 引用。
- **事实底线**：`com.h:627-780`；`vm.h`；`region.h`。
- **知识点清单**：| K-801 调用号族 | 接口与协议 | com.h | 分派常量 | 存量(99) || K-802 PMF/WMF | 约束与不变量 | vm.h | 分配/写映射旗标 | 存量(99) || K-803 VR/VMSF | 约束与不变量 | region.h;mem_cache.c | 区域/缓存旗标 | 存量(99) |
- **验收**：作为工具篇允许被任意靠前篇章引用（附录豁免线性约束，需在 00 声明）。

### 31-engineering-and-config（新建 · 支线）

- **定位**：把散在文档里的 feature 门控/测试基建/sanity 汇成工程附录。
- **讲什么**：K-804（sanity A-7）、K-805（三分配器后端 feature）、后端选择可观测性（V12-P1-1/Fix #65）。
- **不讲什么**：各分配器算法（→ 07）。
- **前置**：00、30。
- **后置**：无（终端附录）。
- **事实底线**：`os/servers/vm/Cargo.toml`（`bitmap_alloc` default / `buddy_alloc` / `segment_tree_alloc` / `sanity_checks` / `vmstats` / `vm_acl_audit`）；`sanity.rs`；`qemu-tests/`、`vm-tests`。
- **知识点清单**：| K-804 sanity A-7 | 工具与工程 | sanity.rs;Cargo.toml | 调试门控 | 存量(散)→归位 || K-805 后端 feature | 工具与工程 | Cargo.toml | 编译期选择 | 存量(05)→归位 |
- **验收**：列出全部 feature 及默认组合；对应测试基线（490/507/490）标注为历史快照。

---

## 6. 变更表

| 操作 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 存量去向 |
|------|------|--------|--------|------|-----------|---------|
| OP-01 | 拆分 | 01-vm-init-main（830 行） | 01-vm-startup-entry | 混装启动/SEF/地址空间，违 5.1.4 | K-011/012/013 | → 新 01 |
| OP-02 | 拆分 | 01-vm-init-main | 03-sef-lifecycle-boot-exec | SEF+boot exec 独立单元 | K-014/015/016/017 | → 新 03 |
| OP-03 | 拆分+提升 | 01-vm-init-main 地址空间段 | 02-vm-address-space-direct-map | 建 Direct Map 前置篇破环 | K-301/302/303/304/305 | 01 边界段 + 06§1.1/1.5 + 07§3.2 → 新 02 |
| OP-04 | 新建 | —（散落 05/06/07/09/10） | 02-vm-address-space-direct-map | Direct Map 提升为独立前置（G-03） | K-301~305 | 各篇改为回指 02 |
| OP-05 | 合并新建 | 无专篇（kernel_gateway.rs/encode.rs） | 18-kernel-gateway-and-wire | 补 G-01/02 缺口 | K-018/019 | 新增（来源=C/os 代码，不受去向约束） |
| OP-06 | 拆分+归位 | 99 工程段 + 散在 05/07-13 | 31-engineering-and-config | 工程/测试/sanity 集中成支线 | K-804/805 | → 新 31 |
| OP-07 | 重编号 | 02-vmproc-struct | 04-vmproc-struct | 前置改指新 01 | K-020/021 | 原样搬移 |
| OP-08 | 重编号 | 03-vmproc-table | 05-vmproc-table | — | K-022/023 | 原样搬移 |
| OP-09 | 扩写 | 04-acl | 06-acl | 补 A-11 fail-closed | K-024/025 | 原样 + 新增段 |
| OP-10 | 重编号+改前置 | 05-physical-memory | 07-physical-memory | 前置 07→02（破环） | K-306/305/311 | 原样，去 Direct Map 解释改回指 |
| OP-11 | 重编号+裁剪 | 06-page-allocator | 08-page-allocator | §1.1/1.5 上移 02 | K-308/307/309 | 概念段迁 02，余原样 |
| OP-12 | 重编号+改前置 | 07-pagetable-struct | 09-pagetable-struct | 前置 05/06→02/08（消环） | K-401/402/403/806 | 原样，Direct Map 改回指 |
| OP-13 | 重编号 | 08-pagetable-ops | 10-pagetable-ops | — | K-404~408 | 原样 |
| OP-14 | 重编号 | 09-slab-allocator | 11-heap-arena | — | K-409/410 | 原样 |
| OP-15 | 重编号 | 10-vm-relocation | 12-vm-relocation | — | K-310 | 原样 |
| OP-16 | 重编号 | 11-phys-pagestate | 13-phys-pagestate | — | K-501/502 | 原样 |
| OP-17 | 重编号 | 12-memtype | 14-memtype | — | K-503/504 | 原样 |
| OP-18 | 重编号 | 13-region-mapping | 15-region-mapping | — | K-505/506 | 原样 |
| OP-19 | 重编号 | 14-region-lookup | 16-region-lookup | — | K-507 | 原样 |
| OP-20 | 重编号 | 15-ipc-dispatch | 17-ipc-dispatch | — | K-601~604 | 原样 |
| OP-21 | 重编号 | 16-pagefault | 19-pagefault | — | K-701 | 原样 |
| OP-22 | 重编号 | 17-cow-mechanism | 20-cow-mechanism | — | K-702/703 | 原样 |
| OP-23 | 重编号 | 18-vm-fork | 21-vm-fork | — | K-704 | 原样 |
| OP-24 | 重编号 | 19-vm-brk | 22-vm-brk | — | K-705 | 原样 |
| OP-25 | 重编号 | 20-vm-mmap | 23-vm-mmap | — | K-706 | 原样 |
| OP-26 | 重编号 | 21-vm-munmap | 24-vm-munmap | — | K-707 | 原样 |
| OP-27 | 重编号 | 22-vm-exit | 25-vm-exit | — | K-708 | 原样 |
| OP-28 | 重编号 | 23-vfs-interaction | 26-vfs-interaction | — | K-709 | 原样 |
| OP-29 | 重编号 | 24-page-cache | 27-page-cache | — | K-710 | 原样 |
| OP-30 | 重编号 | 25-rs-services | 28-rs-services | — | K-711 | 原样 |
| OP-31 | 重编号 | 26-vm-queries | 29-vm-queries | — | K-712 | 原样 |
| OP-32 | 更名入位 | 99-global-concepts | 30-vm-global-concepts | 附录前置为工具篇 | K-801/802/803 | 原样 |

- **拆分去向核对（G6）**：OP-01/02/03 三块覆盖旧 01 全部 K-011~017，无知识点悬空。
- **合并去向核对**：OP-06 从 99 抽工程段 + 从 05/07-13 抽 feature/sanity 提及，源段列明。
- **新建来源核对**：OP-04/05/06 的新增知识点 K-311/K-018/K-019 均带 C/os 代码锚点（§2.2、§3.2）。

---

## 7. 缺漏新篇（§3.3 缺口逐条落实，不留空）

| 缺口 | 主题 | 为什么重要 | 原料在哪里 | 归哪篇 | 验收标准 |
|------|------|-----------|-----------|--------|---------|
| G-01 | kernel↔VM sys_* 网关 | VM 一切动作经此落地内核，无专篇=接口黑箱 | `kernel_gateway.rs`；`libsys/vm_*.c`；`main.c:442/301` | 新 18 | 枚举 VM 用到的全部 sys_*，每个给 C 锚点 |
| G-02 | 消息 wire 编码 | 线格式错=跨模块 ABI 崩，V13-P2-6 五结构缺口 | `ipc/encode.rs`；`ipc.h:mess_*`；`ipcconst.h` | 新 18 | 56 字节对齐断言 + 五缺口表 |
| G-03 | Direct Map 前置专篇 | 消除 {05↔07}{06↔07} 环（硬约束 5.4.2） | 旧 06§1.1/1.5 + 07§3.2 + `direct_map.rs` | 新 02 | 07/08/09 对 Direct Map 无再解释、无前向引用 |
| G-04 | DMA 连续 PA 约束 | 横切 7 篇却无归属，连续 PA 稀缺是真约束 | `dma.rs`；`mem_anon_contig.c` | 新 07 §DMA | 一处主述，其余回指 |
| G-05 | ACL fail-closed/audit | V13-P2-3 fail-open 是安全缺口，须成文 | `audit.rs`；`acl_check` | 新 06 §A-11 | fail-open→fail-closed 对照图 |
| G-06 | sanity/feature 工程附录 | A-7/后端门控散落，读者无法一览配置面 | `sanity.rs`；`Cargo.toml` | 新 31 | 全 feature 清单 |

> 无"待定/否决留空"项：六缺口全部落实为新建或并入。

---

## 8. 锚点迁移与断链成本

### 8.1 旧→新文档编号映射（批量改引用用）

| 旧 | 新 | | 旧 | 新 |
|----|----|-|----|----|
| 01 | 01/02/03（一拆三） | | 14 | 16 |
| 02 | 04 | | 15 | 17 |
| 03 | 05 | | 16 | 19 |
| 04 | 06 | | 17 | 20 |
| 05 | 07 | | 18 | 21 |
| 06 | 08 | | 19 | 22 |
| 07 | 09 | | 20 | 23 |
| 08 | 10 | | 21 | 24 |
| 09 | 11 | | 22 | 25 |
| 10 | 12 | | 23 | 26 |
| 11 | 13 | | 24 | 27 |
| 12 | 14 | | 25 | 28 |
| 13 | 15 | | 26 | 29 |
| 99 | 30 | | （新建） | 02/18/31 |

### 8.2 锚点迁移表（节级 · 覆盖变化文档；1:1 迁移篇只改编号不改小节）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 断链风险 |
|--------|--------|--------|---------|---------|
| 01 §init_vm | 启动十二步 | 新 01 §1 | 拆分 | 低（内部） |
| 01 §Direct Map/自身边界 | VM 自身内存边界 | 新 02 | 提升 | 中：05/06/07 引用改指 02 |
| 01 §SEF | SEF 生命周期 | 新 03 | 拆分 | 中 |
| 01 §boot exec | 引导进程 exec | 新 03 | 拆分 | 低 |
| 06 §1.1 双地址问题 | 概念 | 新 02 | 合并上移 | 高：06→08 引用改 02 |
| 06 §1.5 Direct Map | 常量偏移 | 新 02 | 合并上移 | 高 |
| 07 §3.2 Direct Map 双视图 | 概念 | 新 02 | 合并上移 | 高 |
| 07 §3.4 DM 窗口容量 | 推导 | 新 02 | 越界归位 | 中 |
| 05 §3.4 BumpBuf 自举 | 引用 Direct Map | 新 07（回指 02） | 改写引用 | 中 |
| 05 §3.7 PAF_CLEAR | 直映射清零 | 新 07（回指 02） | 改写引用 | 中 |
| 各篇"不覆盖/前置"声明 | 旧编号 | 按 §8.1 换算 | 全量替换 | 高（面广） |

> 1:1 迁移篇（OP-07~32）：小节结构保留，仅 `前置`/`不覆盖`/正文内 `见 NN-doc.md` 按 §8.1 换算。

### 8.3 引用迁移表（检索实测 · 含代码注释）

| 引用来源 | 旧引用 | 新目标 | 验证方式 |
|----------|--------|--------|---------|
| os/servers/vm/src/ipc/transport.rs:33 | `24-vm-ipc-dispatch.md`（**已失效**） | 新 17-ipc-dispatch | `grep -n 24-vm-ipc-dispatch os/` 归零 |
| os/servers/vm/src 代码注释（23 处热点 05×5/22×3） | 旧文件名 | §8.1 换算 | 逐处 grep 重定向后 `grep -c` 对账 |
| 01-stage-kernel → 07-pagetable-struct（16 处） | 07 | 新 09 | 跨 stage 批量替换 + `check_references.sh` |
| 01-stage-kernel → 08-pagetable-ops（6 处） | 08 | 新 10 | 同上 |
| 01-stage-kernel/12-ipc-core.md | `draft/24-vm-ipc-dispatch` | 新 17 或标注 draft 存档 | 人工确认（指向 draft 的属历史引用，可保留注记） |
| 内部文件名引用（≈330 处） | 旧 NN-name | 新 NN-name | `tools/anchor-migrate.sh` + `anchor-resolve.sh` |

### 8.4 断链成本摘要

- **受影响引用总量 ≈ 378**：内部 330 + 跨 stage 25（01→02，集中 07/08）+ 代码注释 23。
- **热点**：`15-ipc-dispatch`（28）、`13-region-mapping`（24）、`07-pagetable-struct`（15 内 + 16 外）、`05-physical-memory`（15 内 + 5 代码）。
- **批量方式**：§8.1 映射表喂给 `tools/anchor-migrate.sh`（按旧→新文件名批量替换）→ `tools/anchor-resolve.sh` 解析 → `check_references.sh` 校验断链 → 代码注释单列人工过一遍（量小、语义敏感，尤其 A-14/A-1 引用）。
- **净增断链风险点**：Direct Map 相关的"高"风险迁移（02 §1.1/1.5、07 §3.2/3.4 → 新 02）是**为消环而必须支付的代价**，一次性做完即永久消除环与前向引用。
- **成本判定**：378 处以脚本可批量、且集中在少数热点文件，成本可控；相较"消除唯一依赖环 + 拆开最严重违规模板混装篇 + 补三处接口缺口"的正确性收益，符合"算清成本再重建"的门槛。若裁决者认为跨 stage 07/08 的 22 处引用不宜动，见 §9 待裁决的最小改动备选。

---

## 9. 验证与自检门

### 9.1 四种机械检查（§5.4）

1. **前向引用扫描**：逐篇 `前置` 只指向更早编号或附录 30（工具篇豁免）。01→00；02→01；03→01,02；04→01；05→01,04；06→01,04,05；07→01,02,03；08→01,02,07；09→02,08；10→08,09；11→02,08,09,10；12→07-11；13→07,12；14→07,13；15→13,14；16→15；17→01-16；18→05,17；19-29→17+更早；30/31→00/30。**全部指向更早或豁免 → 通过。**
2. **依赖图无环**：Direct Map 抽为 02（置于物理 07 之前）后，旧 {05↔07}、{06↔07} 两环断裂，三处对 Direct Map 的依赖统一回指 02，02 不回指任何更晚篇 → **无环，通过**（对比旧目录：存在 2 个环，未通过）。
3. **覆盖率 100%**：§2 池每条有去向（§6）；新增 K-018/019/311/025 有 C/os 锚点（§2.2、§3.2）；明确删除项——无（旧知识点全部有去处）。24 C 文件 + 10 非 C 制品类全归属（§3.1、§3.2）→ **通过**。
4. **断链成本**：见 §8.4，≈378 处，脚本可批量 → **已量化，通过**。

### 9.2 自检门 G1–G9

| 门 | 结果 |
|----|------|
| G1 C 真序逐条可核 | ✅ §1.2/1.3 全部带 main.c 行号锚点，随机抽 S3-S8/L1-L10 可核 |
| G2 池完整（每 C 文件/制品有归属或排除理由） | ✅ §3.1（24 C 全归属）+ §3.2（10 制品类逐答） |
| G3 新目录前向引用为零 | ✅ §9.1 检查 1（30 作附录豁免，已在 00/契约声明） |
| G4 依赖图无环 | ✅ §9.1 检查 2（旧 2 环已断） |
| G5 覆盖率 100% + 新增带锚点 | ✅ §9.1 检查 3 |
| G6 拆分/合并写清去向、新建写清来源 | ✅ §6（OP-01~03 覆盖旧 01 全量；OP-04/05/06 来源=C/os） |
| G7 每篇契约七要素齐全 | ✅ §5（32 篇，含 1:1 迁移统一契约块） |
| G8 迁移表覆盖变化文档每节 + 引用含代码注释 | ✅ §8.2 节级 + §8.3 含 23 代码注释 + 跨 stage |
| G9 事实断言带锚点、推测标注 | ✅ 全文锚点；**推测/待验证项见下 §9.3** |

### 9.3 推测与待验证项（诚实标注）

- **[待验证]** plan.md §1.2 启动顺序是否与 §1 真序表冲突——未逐条比对（本蓝图独立从 main.c 重建，不依赖 plan 结论），交 B 相核对。
- **[推测]** 新 10（pagetable-ops，1098 行）是否可进一步分"生命周期"与"映射写入"两篇——单一复杂概念暂定保留单篇（soft 上限允许），B 相实测再定，见 §9.4-D3。
- **[过期事实，须 B 相修正]**：① `todo.md §19 V14-P2-1` 称"VM 非 minix-sef 消费者"——已过期，`vm_server.rs:1122 minix_sef::sef_receive_status` + `Cargo.toml` minix-sef 依赖证实已接线；② `ipc/transport.rs:33` 引 `24-vm-ipc-dispatch.md` 为死链；③ 若干篇前置仍写 07（见 §8.2）。

### 9.4 结论与待用户/共识裁决的问题

**结论：蓝图完成**，四项机械检查 + G1–G9 全过。核心重建动作三条：拆 01（违规模板混装）、抽 Direct Map 前置篇（消唯一依赖环，属硬约束必改）、补 kernel-gateway/wire 接口缺口。

**待裁决（不影响蓝图可执行性，供共识/B 相定夺）：**
- **D1｜是否接受整体重编号**：本蓝图采**方案 A（干净重编号，旧 28→新 32，≈378 引用迁移）**。备选**方案 B（最小改动）**：不整体重编号，改为——把 Direct Map 概念在旧 06 内**自足化**并新增一篇附录级 `04a-direct-map.md`（插在 04 与 05 之间，仅后续 05-10 前置改指它），01 拆分为 `01a/01b` 后缀而非重排。方案 B 断链成本更低（约 60-80 处）但编号出现 `04a/01a` 后缀，牺牲线性可读性。**若跨 stage 07/08 的 22 处引用不可动 → 选 B；若可动且要干净编号 → 选 A（推荐）。**
- **D2｜kernel-gateway（新 18）独立成篇 vs 并入 17**：独立更合"单篇单语义"，但增一篇；若读者群偏实现，可并入 17 末章。
- **D3｜pagetable-ops（新 10）单篇 vs 拆分**：见 §9.3，B 相按实际写作复杂度定，契约已留判定点。
