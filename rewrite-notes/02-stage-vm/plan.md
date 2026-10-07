# 02-stage-vm 文档重组计划（plan.md）

> **状态**: 生效中（2026-08-15 首版，深度 review + minix3 源码回归 review 后定稿）
> **范围**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/`
> **目标**: 以 **VM server 启动顺序为主线**重组 VM 全部文档；`fork` 系统调用降为次主线；最终覆盖 Minix3 VM server 全部语义，支撑 VM server 的彻底 Rust 重写
> **对照**: `01-stage-kernel/`（讲述结构参照）、`minix3/minix/servers/vm/`（ground truth）、`os/servers/vm/src/`（Rust 实现）

---

## 1. 背景与动机

### 1.1 旧主线（fork 主线）的问题

旧文档（已移入 `draft/`）以 fork 系统调用为主线：01~19 沿 fork 执行路径引入概念，20~26 为"补全阶段"。实践发现三类问题：

1. **前向引用被迫大量出现**——fork 路径牵涉尚未实现的页表、CoW、mmap、VFS 交互等服务逻辑，文档顺序频繁跳跃、补述未实现内容，打乱阅读流（见 `00-master-plan/README.md` 变更原因）。
2. **组件与流程错位**——`vmproc`/ACL/物理内存/页表等基础组件本应在 VM 启动时按序初始化，却被"fork 需要什么"的倒推逻辑打散；`24-vm-ipc-dispatch` 与 `26-vm-init-main`（主循环与分发，实质是 VM 运行的"心脏"）被排到补全阶段，读者先学了服务、后才知道服务如何被分派。
3. **架构演进（ARCH）标注分散**——`directMap`（Minix3 无、minix-rs 引入）散见于页表文档 §3.6，未形成统一的 ARCH 清单。

### 1.2 新主线：VM server 启动顺序

与 `01-stage-kernel` 相同，采用**读者学习顺序 = 系统实际执行顺序**的组织原则。VM 是一条严格线性的启动链：

```
kernel 启动 VM（ptproc，加载 VM ELF）
  │
  ▼  main.c:93  main()
  └─ is_first_time() → init_vm()          ← 阶段 1~5 全部文档的锚点
       ├─ sys_getkinfo()                    ← 01：启动参数
       ├─ get_mem_chunks()                  ← 05：物理内存布局
       ├─ memset(vmproc) + vm_slot 赋值     ← 02/03：进程模型
       ├─ acl_init()                        ← 04：访问控制
       ├─ map_region_init()                 ← 13：区域管理初始化
       ├─ mem_init(mem_chunks)              ← 05：物理内存分配器
       ├─ init_proc(VM_PROC_NR) + pt_init() ← 02/03：VM 自身槽 + 07/08：页表与 Direct Map
       ├─ __minix_init()                    ← 09 前置：堆可用分界线
       ├─ mem_add_total_pages()             ← 01/05：总页数校准
       ├─ exec_bootproc()（boot 进程）       ← 01：启动进程地址空间
       ├─ CALLMAP（vm_calls 表）             ← 15：IPC 分发注册
       └─ sef_local_startup()               ← 01：SEF 生命周期
  │
  ▼  main.c:112-190  主循环（运行时）
  ├─ missing_spares>0 → alloc_cycle()       ← 06：保留页池补充
  ├─ sef_receive_status(ANY)                ← 15：收消息
  ├─ vm_isokendpt()                         ← 03：caller 验证
  ├─ 分发：
  │   ├─ VFS transid → do_procctl           ← 22：VFS 事务
  │   ├─ RS_INIT     → do_sef_init_request  ← 25：RS 握手
  │   ├─ VM_PAGEFAULT→ do_pagefaults        ← 16：页错误
  │   └─ CALLMAP     → acl_check → vmc_func ← 15/18~26：服务
  └─ ipc_send() 回复（SUSPEND 除外）          ← 15
```

**每篇文档必须能回答一个问题：它位于 VM 启动时序（init_vm）或主循环（dispatch）的哪个位置。** 这是从 `01-stage-kernel/00-kernel-overview.md §3` 继承的组织原则。

### 1.3 fork 次主线

fork 不再充当"概念引入的驱动"，而是作为**阶段 7 的服务之一**展开，其路径图在 `18-vm-fork` 内部绘制：

```
VM_FORK 到达（主循环 dispatch）
  ├─ 03 进程表：vm_isokendpt / 找 slot
  ├─ 04 ACL：acl_fork 派生权限
  ├─ 06 页分配器：子进程页表页
  ├─ 07/08 页表：pt_ptmap / pt_map_in_range
  ├─ 11 物理页状态：pb_reference 引用计数
  ├─ 12 memtype：ev_copy / ev_reference 语义
  ├─ 13 区域：map_proc_copy 复制区域
  ├─ 17 CoW：写保护 + 页错误分裂
  └─ 16 页错误：CoW 触发后按需分配
```

---

## 2. 新文档编号与阶段划分

> 编号规则与 `01-stage-kernel` 一致：`NN-短横线语义名.md`；`00-` 为总览；`99-` 为全局概念。原 `draft/` 文档保留旧编号（作为素材），新编号在顶层重新建立。

### 阶段总览

| 阶段 | 编号 | 文档 | 语义模块 | C 源码 | Rust 模块 | draft 来源 | 变更 |
|------|------|------|---------|--------|-----------|-----------|------|
| 0 总览 | 00 | `00-vm-overview.md` | VM 是什么、启动主线图、文档导航 | `servers/vm/` 全部 | 全部 | `draft/00` | **重写**导航（§2 改为启动主线叙事） |
| 1 启动入口与进程模型 | 01 | `01-vm-init-main.md` | main()/init_vm()/SEF/主循环骨架/exec_bootproc/mem_add_total_pages（调用点） | `main.c`（`get_mem_chunks` 定义于 utility.c，语义归 05） | `main.rs`、`global.rs` | `draft/26` | **拆分**：骨架提前，主循环细节并入 15 |
| 1 | 02 | `02-vmproc-struct.md` | vmproc 结构体、标志位、生命周期状态 | `vmproc.h` | `vmproc/*` | `draft/01` | 沿用 |
| 1 | 03 | `03-vmproc-table.md` | 进程表、endpoint 验证、slot 分配 | `glo.h`、`utility.c:vm_isokendpt` | `vmproc/table.rs` | `draft/02` | 沿用 |
| 2 访问控制与物理内存 | 04 | `04-acl.md` | ACL 初始化/检查/派生/清除 | `acl.c` | `acl.rs` | `draft/03` | 沿用 |
| 2 | 05 | `05-physical-memory.md` | 物理内存布局、bitmap/buddy/segment-tree 分配器、保留队列、memstats | `alloc.c` | `phys_mem/*`、`alloc_stats.rs` | `draft/04` | 沿用 + 补 `get_mem_chunks` |
| 3 页与页表 | 06 | `06-page-allocator.md` | vm_allocpage/vm_allocpages/vm_mappages、保留页池、alloc_cycle | `pagetable.c:295-395`、`alloc.c:reservedqueue_*` | `alloc_page.rs`（含 `vm_pt_alloc`）、`global.rs`、`vm_server.rs` | `draft/05` | 沿用 + 保留页池结构消除（A-1，见 §7.3） |
| 3 | 07 | `07-pagetable-struct.md` | pt 结构、页表层级、**Direct Map（ARCH）**、vm_self_map | `pt.h`、`arch/i386/pagetable.h`、`pagetable.c:pt_init` | `pagetable/mod.rs`、`direct_map.rs`、`pagetable/vm_self_map.rs` | `draft/06` | 沿用 + Direct Map 提升为显式章节 |
| 3 | 08 | `08-pagetable-ops.md` | pt_new/free/bind/writemap/ptmap/mapkernel/pt_checkrange/pt_ptalloc_in_range | `pagetable.c` | `pagetable/mod.rs` | `draft/07` | 沿用 |
| 4 自举的堆与元数据 | 09 | `09-slab-allocator.md` | slab 分配器 → HeapArena + VmAllocator（ARCH） | `slaballoc.c` | `heap_arena.rs`、`global.rs` | `draft/08` | 沿用（ARCH 已标注） |
| 4 | 10 | `10-vm-relocation.md` | VM 自举终点：元数据搬迁、静态→动态 | `alloc.c`、`pagetable.c` | `global.rs`、`phys_mem/mod.rs` | `draft/09` | 沿用 |
| 5 地址空间数据结构 | 11 | `11-phys-pagestate.md` | phys_block/phys_region、引用计数、页标志 | `pb.c`、`region.h`、`phys_region.h` | `region/page_state.rs` | `draft/10` | 沿用 |
| 5 | 12 | `12-memtype.md` | 6 类内存类型 × 15 回调、mem_type_t | `memtype.h`、`mem_anon.c`、`mem_anon_contig.c`、`mem_directphys.c`、`mem_shared.c` | `memtype.rs` | `draft/12` | 沿用（前移至 13 之前，消除 region→memtype 前向引用） |
| 5 | 13 | `13-region-mapping.md` | vir_region/phys_region 映射、map_page_region/unmap/proc_copy/pf | `region.c`、`region.h` | `region/region_map.rs`、`region/vir_region.rs` | `draft/11` | 沿用 |
| 5 | 14 | `14-region-lookup.md` | 区域查找：AVL → BTreeMap（ARCH） | `regionavl.c`、`cavl_*.h`、`unavl.h` | `region/region_map.rs` | `draft/13` | 改名（原 region-avl，语义=查找） |
| 6 主循环与运行时机制 | 15 | `15-ipc-dispatch.md` | CALLMAP、主循环、分发、回复、transid、acl_check 接线 | `main.c:112-190,520-580`、`com.h` | `ipc/dispatcher.rs`、`ipc/transport.rs` | `draft/24` + `draft/26` 主循环部分 | **合并**：分发 + 主循环成一篇 |
| 6 | 16 | `16-pagefault.md` | 页错误处理、handle_memory 状态机、VFS 回调 | `pagefaults.c` | `cow_exec_pf.rs`、`vm_server.rs` | `draft/15` | 沿用 |
| 6 | 17 | `17-cow-mechanism.md` | 写时复制：mem_cow、引用计数分裂 | `pb.c:mem_cow`、`mem_anon.c` | `cow_exec_pf.rs`、`region/page_state.rs` | `draft/14` | 沿用 |
| 7 IPC 服务（fork 次主线 + 进程生命周期） | 18 | `18-vm-fork.md` | VM_FORK：fork 次主线核心 | `fork.c`、`region.c:map_proc_copy` | `fork.rs`、`vm_server.rs` | `draft/16` | 沿用 + 绘制次主线路径图 |
| 7 | 19 | `19-vm-brk.md` | VM_BRK：堆扩展/收缩、DATA_CHANGED/STACK_CHANGED | `break.c` | `brk.rs` | `draft/17` | 沿用 |
| 7 | 20 | `20-vm-mmap.md` | VM_MMAP/VFS_MMAP/REMAP/REMAP_RO、map_perm_check | `mmap.c` | `mmap.rs`、`map_phys.rs` | `draft/18` | 沿用 |
| 7 | 21 | `21-vm-munmap.md` | VM_MUNMAP/MAP_PHYS/UNMAP_PHYS/SHM_UNMAP | `mmap.c:488-573`、`region.c:map_unmap_*`、`mem_directphys.c` | `munmap.rs`、`map_phys.rs` | `draft/19` | 沿用 |
| 7 | 22 | `22-vm-exit.md` | VM_EXIT/WILLEXIT/PROCCTL、free_proc/clear_proc | `exit.c` | `exit.rs`、`vmproc/vmproc.rs` | `draft/20` | 沿用 |
| 8 跨服务协作 | 23 | `23-vfs-interaction.md` | VFS 异步对话、fdref、mapped file | `vfs.c`、`fdref.c`、`mem_file.c` | `vfs_queue.rs`、`fdref.rs` | `draft/23` | 沿用 |
| 8 | 24 | `24-page-cache.md` | 页缓存：双哈希、LRU、mapcache/setcache/forgetcache/clearcache | `cache.c`、`mem_cache.c` | `page_cache.rs` | `draft/25` | 沿用 |
| 8 | 25 | `25-rs-services.md` | RS 服务：SET_PRIV/PREPARE/UPDATE/MEMCTL（Live Update） | `rs.c` | `rs.rs`、`vm_server.rs:RprocTab` | `draft/21` | 沿用 |
| 8 | 26 | `26-vm-queries.md` | 查询：INFO/GETPHYS/GETREF/GETRUSAGE/region_info/usage | `utility.c:do_info` 等、`mmap.c:do_get_*`、`region.c:get_*` | `query.rs` | `draft/22` | 沿用 |
| 99 全局概念 | 99 | `99-global-concepts.md` | endpoint/generation、常量表、全局状态 | `com.h`、`vm.h`、`glo.h` | `minix-types` | `draft/99` | 沿用 |

### 2.1 阶段间的叙事衔接

每个阶段结尾的文档需含"过渡"小节（参照 `01-stage-kernel` 每篇 §6"过渡"），说明本阶段在启动时序中的位置与下一阶段的入口：

```
01（启动骨架）→ 02/03（进程模型）→ 04/05（ACL+物理内存）→ 06~08（页与页表）
→ 09/10（堆+自举）→ 11~14（地址空间数据结构）→ 15（主循环与分发）
→ 16/17（页错误+CoW）→ 18~22（服务）→ 23~26（跨服务协作）
```

---

## 3. 讲述结构规范（参照 01-stage-kernel）

### 3.1 每篇文档的章节模板

与 `01-stage-kernel` 一致（见 `03-kmain-cstart.md`、`04-platform-discovery.md` 等）：

1. **概念**——为什么需要这个机制、类比、边界声明（前置依赖/本篇不覆盖什么）
2. **C 源码分析**——ground truth，必须 grep 实证，禁止凭记忆
3. **Rust 设计决策**——类型系统建模、与 C 的差异（含 ARCH 标注）
4. **实现详解**——Rust 模块结构、关键不变量
5. **测试要点**——相关测试函数 + 测试总数声明
6. **过渡**——在启动时序/主循环中的位置，下一篇入口
7. **参见**——跨文档引用（新编号）

### 3.2 组织原则（继承 01-stage-kernel §3）

1. **概念首次出现即完整解释**——后续只引用不复述
2. **禁止前向引用**——依赖机制前置（本计划的编号已保证：memtype 前移至 12，dispatch 前移至 15）
3. **每篇一个语义单元**——读者可独立阅读
4. **位置可回答性**——每篇回答"它在 init_vm / 主循环的哪个位置"

### 3.3 引用规则

- 各文档之间用新编号交叉引用（如 `07-pagetable-struct.md` §Direct Map）
- 与 kernel 文档交叉引用时用 `../01-stage-kernel/NN-*.md`（kernel 侧对 VM 的引用需同步更新为新编号）
- 对 draft 素材的引用一律指向 `draft/NN-*.md`，并标注"素材"；正式文档绝不引用 review 产物（`21-review-ds.md`/`23-review-ds.md`/`26-review-dsf.md`）

### 3.4 每篇文档边界声明（执行级）

> 写作每篇文档前，先按本表声明**前置依赖 / 本篇职责 / 不覆盖（移交）**，防止内容交叉与遗漏。边界以 §5.3 函数清单为准绳。

| 编号 | 前置依赖 | 本篇职责（核心语义） | 不覆盖（移交） |
|------|---------|---------------------|--------------|
| 00 | 无 | 总览、启动主线图、文档导航、设计原则 | 一切机制细节 |
| 01 | 00 + kernel `09-vm-boot-protocol` | `main`/`init_vm`/SEF 回调/`exec_bootproc`/`libexec_*`/`mem_add_total_pages`（调用点）/`is_first_time`/VM 自身 libc 接口边界声明 | 组件细节（02~15）、主循环 dispatch 细节（15） |
| 02 | 01（memset 语义） | `vmproc` 结构全字段、`VMF_*`、生命周期状态机、`init_proc` 槽语义 | 表管理/endpoint 验证（03） |
| 03 | 02 | `vmproc[VMP_NR]` 表、slot 分配查找、`vm_isokendpt`、`VMP_EXECTMP` | 结构字段细节（02） |
| 04 | 01（`acl_init` 调用点） | `acl_init/check/set/fork/clear`、ACL 位图语义、DEFAULT/SYSTEM 分层 | dispatch 中 `acl_check` 接线（15） |
| 05 | 00/01（`get_mem_chunks` 调用点） | `get_mem_chunks`、`mem_init`、`alloc_mem/free_mem`、`reservedqueue_*`、`memstats`、`usedpages_*`、`mem_add_total_pages`（定义） | `vm_allocpage` 页分配（06） |
| 06 | 05 | `vm_allocpage/pages/mappages/freepages`、`vm_pagelock/addrok`、保留页池、`alloc_cycle`、`get_vm_self_pages` | pt 结构（07） |
| 07 | 05/06 | `pt_t`、`ARCH_VM_*` 宏、**Direct Map（A-1）**、`vm_self_map`（A-9）、`pt_init` 结构面 | pt 操作（08） |
| 08 | 07 | `pt_new/free/bind/writemap/ptmap/map_in_range/mapkernel/pt_alloc_in_range/checkrange/clearmapcache`、`pt_copy`、`pt_allocate_kernel_mapped_pagetables`、`pt_writable` | 页分配（06） |
| 09 | 01（`__minix_init` 堆可用分界线） | slab→`HeapArena`/`VmAllocator`（A-3）、`SLABALLOC/FREE` 宏、MEMPROTECT | 元数据搬迁（10） |
| 10 | 09 | 自举搬迁、静态→动态、`swap_proc_slot/dyn_data`、`map_proc_dyn_data`、`transfer_mmap_regions`、`map_setparent` | RS/LU 服务流程（25） |
| 11 | 13 概念序（pb 是 region 的物理侧） | `phys_block`/`phys_region` 结构、`pb_new/free/link/reference/unreferenced`、`PBF_*` | vir_region 映射（13）、CoW 分裂（17） |
| 12 | 11 | `mem_type_t` 6 实例 × 15 回调签名与语义、`mem_cow`/`phys_setphys`/`shared_setsource` 的类型级语义 | 回调在 pagefault/fork/mmap 中的调用流程（16/17/18/20/23） |
| 13 | 11/12 | `map_region_init`、`map_page_region`、`map_unmap_*`、`map_proc_copy(_range)`、`map_pf`、`map_handle_memory`、`map_pin_memory`、`map_writept`、`map_free(_proc)`、`physblock_get/set`、`map_ph_writept`、`map_region_lookup_type`、`map_copy_region`、`vrallocflags`、`map_printmap/printregionstats` | 查找索引（14）、查询（26） |
| 14 | 13 | AVL→`BTreeMap`（A-4）、`region_find_slot*` 的索引面、O(log n) 语义 | 区域生命周期（13） |
| 15 | 01 + 02~14 全部就绪 | `vm_calls`/CALLMAP、主循环 5 优先级分发、`SUSPEND`、transid 路由、`is_ipc_notify`、`acl_check` 接线 | 各 handler 实现（16~26） |
| 16 | 13/15 | `do_pagefaults`、`handle_pagefault`、`handle_memory_start/once/step/final/continue`、`do_memory`、`pf_errstr`、VFS 回调状态机 | CoW 分裂机制（17） |
| 17 | 11/12 | `mem_cow`、写保护设置、引用计数分裂、`writable` 回调 | file-backed `cow_block`（23） |
| 18 | 02/03/04/06/07/08/11/12/13/17 | `do_fork` 全流程、`map_proc_copy`、`pt_ptmap`、`acl_fork`、endpoint 合成、**fork 次主线路径图** | CoW 机制（17）、页错误（16） |
| 19 | 13/15 | `do_brk`/`real_brk`、`DATA_CHANGED/STACK_CHANGED`、brk 与 region 边界 | mmap 地址空间分配（20） |
| 20 | 13/15/19 | `do_mmap`/`do_vfs_mmap`/`mmap_file`/`mmap_file_cont`/`mmap_region`/`map_perm_check`/`do_remap` | munmap（21）、map_phys（21） |
| 21 | 13/20 | `do_munmap`/`munmap_vm_lin`/`do_unmap_phys`/`VM_SHM_UNMAP`/`map_unmap_*`、`phys_setphys` 调用面 | mmap 建立（20） |
| 22 | 03/13/15 | `do_exit`/`do_willexit`/`do_procctl`、`free_proc`/`clear_proc`、`reset_vm_rusage`、`VMPPARAM_*`、VFS transid 路径 | 查询（26） |
| 23 | 12/13/15/20 | `vfs_request`/`do_vfs_reply`/`activate`、`fdref_new/ref/deref/dedup_or_new`、`mappedfile_setfile`、`cow_block`、异步请求队列 | 页缓存（24） |
| 24 | 12/23 | `cache.c`+`mem_cache.c` 全部、双哈希、LRU、4 个 cache IPC handler | VFS 请求队列（23） |
| 25 | 15/22（LU 需 10） | `do_rs_set_priv/prepare/update/memctl`、`rs_memctl_*`、`map_service`、`adjust_proc_refs`、`RprocTab`、A-8 缺口契约 | 查询（26） |
| 26 | 13/15 | `do_info`/`do_get_phys`/`do_get_refcount`/`do_getrusage`/`get_usage_info(_kernel)`/`get_region_info`/`map_get_phys`/`map_get_ref` | 区域生命周期（13） |
| 99 | 无 | endpoint/generation、`VM_*`/`VMP_*` 常量、全局变量表 | 一切机制 |

### 3.5 测试基线（截至 2026-08-17）

- `cargo test -p minix-vm --lib`：**441 passed / 0 failed**（基线更新：2026-08-16 26-vm-queries 轮实测复核 433/1；P0-1 修复（todo.md §8，PageSlot 三态状态机）后 `test_map_lazy` 转绿 → 434/0；P1-1 free-list + V9-* 修复（todo.md §10）后 → 437/0；V10 全量（todo.md §13，transport 注入端到端 + IpcStatus 状态位 + InfoStats 可观测性等）后 → 441/0；历史基线：360/1 → 414/1 → 433/1，增量来自 15/19/20/21/22/24/25/26 各轮新增测试）
- `cargo clippy -p minix-vm --lib`：**0 warnings**（V10-P2-1 死代码收敛后，todo.md §13 Fix #16）
- 每篇改写完成时在文末更新该模块测试统计（review-doc-skill §2.4j）

### 3.6 Review gate 要求（每篇改写必检）

- 每篇新文档创建时同步生成 `.design/{NN}-outline.md` + `{NN}-outline-review.md` + `{NN}-design.md`（Step 0.3 嵌入生成，Gate H.6，不允许 N/A）
- 每篇改写后按 review 工作流跑 Blocker Gates（0/A/B/C/D/D-6/E/G/H），scan 产物写入 `.review/codex/vm/{NN}-{name}/`
- P0 未清不得标完成；doc 与 code 保持同步



- 各文档之间用新编号交叉引用（如 `07-pagetable-struct.md` §Direct Map）
- 与 kernel 文档交叉引用时用 `../01-stage-kernel/NN-*.md`（kernel 侧对 VM 的引用需同步更新为新编号）
- 对 draft 素材的引用一律指向 `draft/NN-*.md`，并标注"素材"；正式文档绝不引用 review 产物（`21-review-ds.md`/`23-review-ds.md`/`26-review-dsf.md`）

---

## 4. 架构演进（ARCH）清单

> 按 review-core 要求，ARCH 必须三处一致标注：Minix3 行为对照点 + design doc + 代码注释。以下为 02-stage-vm 范围内的 ARCH 项，写文档时必须逐项落实。

| # | ARCH 项 | Minix3 现状 | minix-rs 演进 | 涉及新文档 | 状态 |
|---|---------|------------|--------------|-----------|------|
| A-1 | **Direct Map** | 无（`pmap.h` 的 `PMAP_DIRECT_MAP` 受 `#ifdef __HAVE_DIRECT_MAP` 保护且从未定义；VM 用 `vm_phys_to_virt` 走静态映射表） | `VM_DIRECT_MAP_BASE`/`KERNEL_DIRECT_MAP_BASE`，`DirectMapArch` trait（`os/arch/src/direct_map.rs`），VM 侧 `direct_map.rs` 双向转换 | 07 | 已实现，需文档显式章节 |
| A-2 | 页表层级 | 2 级（PD→PT，`pt_pt[1024]` 固定数组，u32 PTE） | 4 级（PML4→PDPT→PD→PT，动态分配，u64 PTE） | 07/08 | 已实现 |
| A-3 | 内核堆分配 | `slaballoc.c`（528 行） | `HeapArena` + 全局 `VmAllocator`（`heap_arena.rs`/`global.rs`；A-3 v2：bump → free-list 复用，2026-08-16） | 09 | 已实现（slab 有意省略，需理由；v2 补回收语义） |
| A-4 | 区域查找 | Walt Karas 公共域 AVL（`cavl_*.h` 宏模板） | `BTreeMap<VirBytes, VirRegion>`（`region_map.rs`） | 14 | 已实现（O(log n) 语义等价） |
| A-5 | 物理分配器 | 单一位图（`alloc.c`） | bitmap + buddy + segment-tree 三实现（`phys_mem/*`），`PhysAllocator` trait | 05 | 已实现 |
| A-6 | 地址空间宽度 | 32 位（`VM_DATATOP`/`VM_STACKTOP`/mmap 范围受限） | 64 位（`MMAP_BASE`/`MMAP_TOP` 重定义） | 00/07/20 | 已实现 |
| A-7 | sanity 宏 | `SANITYCHECKS`/`CACHE_SANITY`/`VMSTATS`（编译宏） | `#[cfg(feature = "...")]` + 单元测试 | 各文档 §测试 | 设计差异 |
| A-8 | SEF / Live Update | `rs.c` 全量实现 | `RS_PREPARE`/`RS_UPDATE` 未实现（NotImplemented，fail-closed） | 25 | **缺口**：标注 defer + 语义契约 |
| A-9 | VM 自映射页表 | 静态 `static_sparepagedirs` | `vm_self_map.rs` | 07 | 已实现 |
| A-10 | 多架构 | i386 + earm 双 arch（`arch/i386/pagetable.h`、`arch/earm/pagetable.h`） | x86-64 + arm64 + riscv64 三架构 trait | 07 | 已实现（trait 抽象） |
| A-11 | **ACL fail-closed** | `acl_check` 对 `NO_ACL` 进程放行全部调用，仅打印告警（`acl.c:44-53`，注释 "for now" 承认是临时放松） | `AclState::Uninitialized` 仅放行 `AclMask::DEFAULT` 集（`acl.rs:102-116`）——未接管进程 fail-closed，特权调用必须经 RS 显式授权 | 04 | 已实现（语义偏移，需 doc §2/§3 诚实标注） |
| A-12 | **memtype resize 回调并入 extend** | `ev_resize` 回调（memtype.h:21）+ 无回调时 `map_page_region` 追加（region.c:1037-1045）双路径 | `VirRegion::extend` memtype 无关通用扩展（`vir_region.rs:127-140`，push EMPTY 槽 + 改 length），memtype 回调族 resize 语义并入 | 19 | 已实现（结构简化，外部行为等价；doc §3.2/§3.6 + design 19-design.v1 D2 + 代码注释三处一致） |

---

## 5. 覆盖完整性核对（对照 minix3 源码）

> 本节是 plan.md 的语义覆盖契约。所有断言以 grep 实证为准（review-core：禁止凭记忆）。逐项核对见 §7。

### 5.1 C 源文件 → 新文档映射（24 个 .c）

| C 文件 | 行数 | 新文档 | 核对 |
|--------|------|--------|------|
| `acl.c` | 129 | 04 | 已核对 |
| `alloc.c` | 548 | 05/06/10 | 已核对 |
| `break.c` | 69 | 19 | 已核对 |
| `cache.c` | 332 | 24 | 已核对 |
| `exit.c` | 156 | 22 | 已核对 |
| `fdref.c` | 177 | 23 | 已核对 |
| `fork.c` | 116 | 18 | 已核对 |
| `main.c` | 768 | 01/15 | 已核对 |
| `mem_anon.c` | 151 | 12 | 已核对 |
| `mem_anon_contig.c` | 132 | 12 | 已核对 |
| `mem_cache.c` | 324 | 24 | 已核对 |
| `mem_directphys.c` | 79 | 12/21 | 已核对 |
| `mem_file.c` | 287 | 23 | 已核对 |
| `mem_shared.c` | 211 | 12 | 已核对 |
| `mmap.c` | 573 | 20/21/26 | 已核对 |
| `pagefaults.c` | 418 | 16 | 已核对 |
| `pagetable.c` | 1500 | 06/07/08 | 已核对 |
| `pb.c` | 168 | 11/17 | 已核对 |
| `region.c` | 1555 | 13/21/22/26 | 已核对 |
| `regionavl.c` | 11（+cavl 模板 1426 行） | 14 | 已核对 |
| `rs.c` | 391 | 25 | 已核对 |
| `slaballoc.c` | 528 | 09 | 已核对 |
| `utility.c` | 494 | 01/03/05/26 | 已核对 |
| `vfs.c` | 143 | 23 | 已核对 |

### 5.2 头文件覆盖

| 头文件 | 归属 | 核对 |
|--------|------|------|
| `vm.h`、`vmproc.h`、`glo.h` | 00/02/03/99 | 已核对 |
| `pt.h`、`arch/i386/pagetable.h`、`arch/earm/pagetable.h` | 07/08（earm 归 A-10） | 已核对 |
| `region.h`、`phys_region.h` | 11/13 | 已核对 |
| `memtype.h` | 12 | 已核对 |
| `cache.h` | 24 | 已核对 |
| `sanitycheck.h` | A-7（cfg 替代） | 已核对 |
| `proto.h` | 全部分布（函数签名来源） | 已核对 |
| `cavl_if.h`/`cavl_impl.h`/`unavl.h`/`regionavl.h` | 14（BTreeMap 替代） | 已核对 |
| `memlist.h`、`util.h` | 内部辅助（并入所属文档） | 已核对 |
| `Makefile.inc`、`arch/earm/vm.lds` | 构建/链接脚本（非语义，WONTFIX 标注） | 已核对 |

### 5.3 语义模块覆盖清单（函数级）

> 每篇文档必须覆盖的函数清单以 `checklist.md`（顶层保留）§4 函数表为基线，逐篇核对。以下列出跨文件的关键语义，防止"文件有映射但函数漏掉"：

- **01**：`main`、`init_vm`、`sef_local_startup`、`sef_cb_init_fresh`、`sef_cb_init_lu_restart`（同时注册 LU 与 restart 回调，main.c:223-224）、`sef_cb_lu_state_changed`、`sef_cb_init_vm_multi_lu`、`sef_cb_signal_handler`、`do_sef_init_request`、`exec_bootproc`、`libexec_*`、`map_service`、`mem_add_total_pages`
- **02/03**：`vmproc` 结构全字段、`VMF_*` 标志、`vm_slot`、`vm_isokendpt`、`vmproc[VMP_NR]` 全局表、`VMP_EXECTMP` 语义、`init_proc`（main.c:262，进程槽初始化）
- **05**：`mem_init`、`alloc_mem`、`free_mem`、`alloc_cycle`、`reservedqueue_*`、`memstats`、`printmemstats`、`usedpages_*`、`get_mem_chunks`、`mem_add_total_pages`
- **06**：`vm_allocpage`、`vm_allocpages`、`vm_mappages`、`vm_freepages`、`vm_pagelock`、`vm_addrok`、`get_vm_self_pages`
- **07**：`pt_t` 结构、`ARCH_VM_*` 宏、`pt_init`、`pt_init_mem`、`Direct Map` 双视图
- **08**：`pt_new`、`pt_free`、`pt_bind`、`pt_writemap`、`pt_ptmap`、`pt_map_in_range`、`pt_mapkernel`、`pt_ptalloc_in_range`、`pt_checkrange`、`pt_clearmapcache`、`pt_ptalloc`、`pt_copy`、`pt_allocate_kernel_mapped_pagetables`、`pt_writable`
- **09**：`slaballoc`、`slabfree`、`slabstats`、`SLABALLOC/SLABFREE` 宏、MEMPROTECT 语义
- **10**：搬迁机制、静态→动态分配转换、`swap_proc_slot`/`swap_proc_dyn_data`/`map_proc_dyn_data`（utility.c，Live Update 支撑）、`transfer_mmap_regions`（utility.c:228）、`map_setparent`
- **11**：`pb_new`、`pb_free`、`pb_link`、`pb_reference`、`pb_unreferenced`、`phys_region` 结构、`PBF_*` 标志
- **12**：6 类 `mem_type_*` 实例、15 个 `ev_*` 回调、`mem_cow`（COW 分裂）、`phys_setphys`、`shared_setsource`
- **13**：`map_region_init`、`map_page_region`、`map_region_extend*`、`map_unmap_region`、`map_unmap_range`、`map_free_proc`、`map_proc_copy`、`map_proc_copy_range`、`map_lookup`、`map_pf`、`map_handle_memory`、`map_pin_memory`、`map_writept`、`map_free`、`physblock_get/set`、`map_ph_writept`、`map_region_lookup_type`（RS Live Update 预分配查找，rs.c:177,334）、`map_copy_region`（static，map_proc_copy 内部）、`vrallocflags`（VR 标志→PAF 分配标志，mem_anon/pb/mem_file 共用）、`physregions`、`map_printmap`/`printregionstats`（调试打印）
- **14**：`region_find_slot*`、AVL 操作 → `BTreeMap` 等价语义
- **15**：`vm_calls[]`、`CALLMAP`、主循环、`acl_check` 接线、`SUSPEND` 伪返回码、transid 路由、`is_ipc_notify` 处理
- **16**：`do_pagefaults`、`handle_pagefault`、`handle_memory_start/once/step/final/continue`、`do_memory`、`pf_errstr`、VFS 回调（`vfs_callback_t`）
- **17**：`mem_cow`、写保护设置、引用计数分裂、`writable` 回调
- **18**：`do_fork`、`map_proc_copy`、`pt_ptmap`、`acl_fork`、endpoint 合成
- **19**：`do_brk`、`real_brk`、`DATA_CHANGED/STACK_CHANGED`、brk 与 region 边界
- **20**：`do_mmap`、`do_vfs_mmap`、`mmap_file`、`mmap_file_cont`（VFS 异步续作回调）、`mmap_region`、`map_perm_check`、`do_remap`、`do_map_phys`
- **21**：`do_munmap`、`munmap_vm_lin`、`do_unmap_phys`、`map_unmap_range/region`、`VM_SHM_UNMAP`
- **22**：`do_exit`、`do_willexit`、`do_procctl`、`free_proc`、`clear_proc`、`reset_vm_rusage`、VMPPARAM_* 语义
- **23**：`vfs_request`、`do_vfs_reply`、`activate`、`fdref_new/ref/deref/dedup_or_new`、`mappedfile_setfile`、`cow_block`（mem_file.c:59，file-backed COW 分裂）
- **24**：`find_cached_page_bydev/ino`、`addcache`、`rmcache`、`cache_freepages`、`clear_cache_bydev`、`get_stats_info`、`cache_lru_touch`、`do_mapcache/setcache/forgetcache/clearcache`
- **25**：`do_rs_set_priv/prepare/update/memctl`、`rs_memctl_*` 静态族、`map_service`、`adjust_proc_refs`（LU 状态切换后调整引用，main.c:215,722）
- **26**：`do_info`、`do_get_phys`、`do_get_refcount`、`do_getrusage`、`get_usage_info`、`get_usage_info_kernel`、`get_region_info`、`map_get_phys`、`map_get_ref`
- **99**：endpoint/generation、`VM_*` 常量、`VMP_*` 常量、全局变量表

### 5.4 明确排除 / 跳过的项

| 项 | 处理 | 依据 |
|----|------|------|
| `proto.h` 中 `map_memory`/`unmap_memory` | **C 死声明**（无定义文件），标注跳过 | grep 全 `servers/vm/` 无实现 |
| `region.c:copy_abs2region` | **死导出函数**（全 minix3 无调用者），标注跳过 | grep 全 `minix3/` 仅定义+proto 声明 |
| SANITYCHECKS 门控函数族（`mem_sanitycheck`/`slab_sanitycheck`/`slabsane_f`/`pt_sanitycheck`/`pt_assert`/`map_sanitycheck`/`cache_sanitycheck_internal`/`fdref_sanitycheck`/`slablock`/`slabunlock`） | A-7 cfg feature 替代 | 设计差异 |
| `sanitycheck.h` 全部 | A-7 cfg feature 替代 | 设计差异 |
| `utility.c` 的 `mmap`/`munmap`/`_brk`（VM 自身 libc 兼容接口） | 无 `servers/vm/` 内部调用者，在 01 边界声明中说明即可 | grep 实证（仅定义处 3 处） |
| `Makefile.inc`、`arch/earm/vm.lds` | 构建/链接脚本，非语义 | WONTFIX |
| `VM_ADDDMA/DELDMA/GETDMA`（checklist I-026~028） | 未实现，标注 defer + 语义契约（保留 ACL 位） | checklist §6 |
| `RS_PREPARE`/`RS_UPDATE`（I-023/024） | 未实现，标注 defer（A-8） | checklist §6 |
| `21-review-ds.md`/`23-review-ds.md`/`26-review-dsf.md` | 历史 review 产物，保留在 `draft/`，不进入新编号 | 组织原则 |

---

## 6. 实施路线

> 每篇新文档 = 基于对应 draft 素材改写（重写导航/合并拆分处除外），并遵守 §3 讲述结构 + §4 ARCH 标注 + §5 覆盖契约。所有 P0 修复完成后才可推进下一篇。

1. **00-vm-overview 重写**（导航改为启动主线叙事，含 §2 启动时序图）
2. **01-vm-init-main 拆分**（骨架 → 02~15 的锚点；主循环细节移交 15）
3. **02~14 按 draft 素材改写**（每篇补"位置可回答性"声明 + ARCH 标注核对）
4. **15-ipc-dispatch 合并改写**（draft 24 + draft 26 主循环部分）
5. **16~26 服务与协作文档改写**（fork 次主线路径图入 18）
6. **99-global-concepts 同步** + `checklist.md` 编号更新
7. **kernel 侧交叉引用同步**（`01-stage-kernel` 对 VM 的引用更新为新编号）

### 6.1 文档改写状态跟踪

> 每完成一篇，将状态改为 `reviewed`（附 scan 日期）。全部 `reviewed` 且无 P0 遗留 = 阶段完成。

| 编号 | 状态 | 首轮 review 日期 | 备注 |
|------|------|-----------------|------|
| 00 | pending | — | 导航重写，启动主线图 |
| 01 | reviewed | 2026-08-15 | draft/26 拆分；`.review/codex/vm/01-vm-init-main/scan.md` |
| 02 | reviewed | 2026-08-15 | draft/01 沿用；`.review/codex/vm/02-vmproc-struct/scan.md` |
| 03 | reviewed | 2026-08-15 | draft/02 沿用；`.review/codex/vm/03-vmproc-table/scan.md` |
| 04 | reviewed | 2026-08-15 | `.review/codex/vm/04-acl/scan.md` |
| 05 | reviewed | 2026-08-15 | `.review/codex/vm/05-physical-memory/scan.md` |
| 06 | reviewed | 2026-08-15 | 写作 + 回归；`.review/codex/vm/06-page-allocator/scan.md` |
| 07 | reviewed | 2026-08-15 | 写作 + 回归；`.review/codex/vm/07-pagetable-struct/scan.md` |
| 08 | reviewed | 2026-08-15 | 写作 + 回归；`.review/codex/vm/08-pagetable-ops/scan.md` |
| 09 | reviewed | 2026-08-15 | 写作 + 回归；`.review/codex/vm/09-slab-allocator/scan.md` |
| 10 | reviewed | 2026-08-15 | draft/09 沿用；`.review/codex/vm/10-vm-relocation/scan.md` |
| 11 | reviewed | 2026-08-16 | draft/10 沿用；`.review/codex/vm/11-phys-pagestate/scan.md` |
| 12 | reviewed | 2026-08-16 | draft/12 沿用；`.review/codex/vm/12-memtype/scan.md` |
| 13 | reviewed | 2026-08-16 | 写作 + 回归；`.review/codex/vm/13-region-mapping/scan.md` |
| 14 | reviewed | 2026-08-16 | 写作 + 回归；`.review/codex/vm/14-region-lookup/scan.md` |
| 15 | reviewed | 2026-08-16 | 写作 + 回归（RS_INIT P0 修复闭环）；`.review/codex/vm/15-ipc-dispatch/scan.md` |
| 16 | reviewed | 2026-08-16 | 写作 + 回归（wire-format P0 修复闭环）；`.review/codex/vm/16-pagefault/scan.md` |
| 17 | reviewed | 2026-08-16 | 写作 + 回归（IN_CACHE 引用错位 P2 修复）；`.review/codex/vm/17-cow-mechanism/scan.md` |
| 18 | reviewed | 2026-08-16 | 写作 + 回归（03-P1-1 P1 闭环 + 20 处行号修复）；`.review/codex/vm/18-vm-fork/scan.md` |
| 19 | reviewed | 2026-08-16 | 写作 + 回归（19-P1-1 wire-format P1 闭环 + 19-P1-2 ARCH A-12 三处一致 + 8 项差异清单 + 24 项 AI 判断）；`.review/codex/vm/19-vm-brk/scan.md` |
| 20 | reviewed | 2026-08-16 | 写作 + 回归（20-P1-1 wire-format 四消息 P1 闭环 + 7 条 errno 语义修复 + 14 项差异清单 + 6 项 backlog）；`.review/codex/vm/20-vm-mmap/scan.md` |
| 21 | reviewed | 2026-08-16 | 写作 + 回归（21-P0-1 memtype 门控 P0 + 21-P1-4/5 + 行号实证 + SYMBOLS 39 + VERIFY 23/23）；`.review/codex/vm/21-vm-munmap/scan.md` |
| 22 | reviewed | 2026-08-16 | 写作 + 回归（22-P0-1/2 P0 + 22-P1-1..5 + P2 10 组 + SYMBOLS 34 + VERIFY 24/24）；`.review/codex/vm/22-vm-exit/scan.md` |
| 23 | reviewed | 2026-08-16 | 写作 + 回归（23-P0-1/1b/1c + 23-P1-1/2 + P2 9 组 + SYMBOLS 37 + VERIFY 32/32）；`.review/codex/vm/23-vfs-interaction/scan.md` |
| 24 | reviewed | 2026-08-16 | 写作 + 回归（24-P0-5 P0-fact cache_freepages clicks 三处一致 + 24-P1-4 + P2 组 Fix #3-#8 + SYMBOLS 48 + VERIFY 29/29）；`.review/codex/vm/24-page-cache/scan.md` |
| 25~26 | pending | — | draft 素材沿用改写 |
| 99 | pending | — | 常量表同步 |
| checklist.md | pending | — | 编号/路径更新（§6 第 6 步） |


---

## 7. Review 记录

> 本节记录 plan.md 自身的 review 过程（深度 review + minix3 回归 review），与最终 plan.md 同文档交付，保证"覆盖完整性核对"可追溯。

### 7.1 深度 review（2026-08-15）

**范围**：语义全覆盖 + 语义模块合理拆分 + 叙事顺序无前向引用。

深度 review 结论与修复：

| # | 发现 | 等级 | 修复 |
|---|------|------|------|
| D-1 | 原顺序 11-region → 12-memtype 存在前向引用（`vir_region.def_memtype` 依赖 memtype 概念） | P1 | 新编号将 memtype 前移至 12，region 后移至 13 |
| D-2 | 原 24-ipc-dispatch / 26-vm-init-main 排入"补全阶段"，主循环机制后置 | P1 | 新编号将分发+主循环合并为 15，置于服务（18+）之前 |
| D-3 | `get_mem_chunks`（utility.c）仅存在于 26-init-main 素材，语义上属于物理内存 | P1 | 划入 05-physical-memory |
| D-4 | Direct Map 仅作为 06 的 §3.6，未进入统一 ARCH 清单 | P1 | §4 A-1 建立统一清单，07 中设显式章节 |
| D-5 | 原 21/23/26 的 review 产物与新编号文档编号冲突（21-review-ds vs 21-vm-rs-services） | P2 | review 产物不进入新编号，留在 draft/ |
| D-6 | 原编号无"阶段"组织，00-vm-overview 导航按 fork 主线叙事 | P1 | §2 建立阶段总览 + 00 导航重写要求 |
| D-7 | §5.3 01 函数名缩写 `sef_cb_init_fresh/lu_restart` 与 C 名不符（实际 `sef_cb_init_lu_restart`，且同时注册 LU+restart） | P2 | 改为精确 C 名 + 注册语义；补 `sef_cb_lu_state_changed`/`sef_cb_init_vm_multi_lu` |
| D-8 | §5.3 漏 `map_region_lookup_type`（region.c，RS Live Update 预分配查找） | P1 | 补入 13 清单 |
| D-9 | §5.3 漏 `mmap_file_cont`（mmap.c，VFS 异步续作）与 `adjust_proc_refs`（utility.c，LU） | P1 | 补入 20/25 清单 |
| D-10 | `utility.c` 的 `mmap`/`munmap`/`_brk`（VM 自身 libc 接口）未在任何文档中定位 | P2 | §5.4 增加定位说明（01 边界声明） |
| D-11 | §5.3 遗漏 `init_proc`/`pt_copy`/`pt_allocate_kernel_mapped_pagetables`/`pt_writable`/`transfer_mmap_regions`/`map_copy_region`/`cow_block`（跨 6 文档的静态/内部函数面） | P1 | 逐项补入 02/08/10/13/23 清单 |
| D-12 | §5.3 漏 `vrallocflags`/`physregions`/`get_usage_info_kernel`（有真实调用者的语义函数） | P1 | 补入 13/26 清单 |
| D-13 | `copy_abs2region` 全 minix3 无调用者（死导出），未标注 | P2 | 补入 §5.4 排除表 |
| D-14 | §5.3 漏调试打印函数 `map_printmap`/`printregionstats` | P2 | 补入 13 清单 |
| D-15 | §2 表 01 行 C 源码列误含 `get_mem_chunks`（与 D-3 的 05 归属矛盾） | P2 | 修正为 `main.c` 并注明归属 |
| D-16 | §1.2 时序图 `__minix_init` 锚点误标 06（应为 09 前置）、init_proc 未标进程模型文档；§3.3 引用规则误限"阶段 1~5" | P2 | 修正锚点标注与引用规则范围 |
| D-17 | 可执行性审计：draft/ 移入后 `../../concepts/` 相对路径失效（4 处） | P2 | 修正为 `../../../concepts/`（scan.md 已记录） |
| D-18 | 可执行性审计：kernel 侧 11 处散文式引用仍指向旧路径 | P2 | 全部指向 `02-stage-vm/draft/`（scan.md 已记录） |
| D-19 | 缺每篇"前置依赖/职责/不覆盖"边界声明，写作时易内容交叉 | P1 | 新增 §3.4 边界表（28 篇全列） |
| D-20 | 无测试基线，§测试 无法对账 | P2 | 新增 §3.5：`cargo test -p minix-vm --lib` = 322 passed / 15 failed（pre-existing，2026-08-15） |
| D-21 | 未声明每篇改写接入 review gate（outline/design 快照） | P1 | 新增 §3.6：Gate H.6 要求 + `.review/codex/vm/{NN}-{name}/` 产物路径 |
| D-22 | §6 无文档改写状态跟踪；checklist.md §10 旧路径失效 | P2 | 新增 §6.1 跟踪表；checklist 路径修复 |

### 7.2 minix3 源码回归 review（2026-08-15）

**方法**：对 `minix3/minix/servers/vm/` 全部 24 个 .c + 头文件逐一 grep 核对 §5.1/§5.2 映射表，并抽查 §5.3 函数级清单。

**证据**：

```bash
ls minix3/minix/servers/vm/*.c          # 24 个文件，与 §5.1 表一致
rg -n '^int do_|^void |^struct |^phys_clicks ' minix3/minix/servers/vm/*.c   # 函数面核对
rg -n 'mem_type_(anon|directphys|anon_contig|cache|mappedfile|shared)' minix3/minix/servers/vm/glo.h   # 6 类 memtype
rg -n 'CALLMAP' minix3/minix/servers/vm/main.c    # 分发注册面
rg -n 'map_memory|unmap_memory' minix3/minix/servers/vm/   # 死声明确认（仅 proto.h）
```

**结论**：24 个 .c 文件全部映射到新文档，无遗漏；6 类 memtype、IPC handler（含未实现的 DMA/RS-PREPARE/UPDATE）全部进入覆盖契约；ARCH 项（A-1~A-10）与 minix3 现状对照成立。**覆盖完整性通过**。

### 7.3 06 写作前置设计决策：保留页池结构消除（2026-08-15）

**决策**：`critical_pool.rs`（通用 `CriticalPool<T>`）删除，`reservedqueue_*` 的 Rust 消费方**不实现**，归入 `[ARCH: A-1]`（Direct Map）结构消除。

**依据**（grep 实证，见 06-page-allocator.md §3.3）：
- C 备用页池的唯一目的是打破自举循环依赖（pagetable.c:55-57 注释 "avoid a circular dependency on allocating memory and writing it into VM's page table"），且 `pt_init` 末尾被整体替换为动态页（pagetable.c:1311-1345）——它是**自举机制**，不是稳态供应。
- minix-rs 中 VA 由 Direct Map 常量偏移给出（`VM_DIRECT_MAP_BASE + phys`），页表页分配（`alloc_page::vm_pt_alloc`，注册进 `minix_arch::pt_alloc`）从 T3 起单路径可用，循环依赖被结构性打破——`level` 计数器、`pt_init_done` 阶段切换、BSS 静态页全部消失。
- 对照 Redox RMM / Linux：均无 VM 侧备用页池——内核早期内存恒等/直接映射 + 物理帧分配器不依赖映射子系统，与 minix-rs 同构。
- 保留的语义：`missing_spares`（alloc.c:74）在 Rust 中重解释为**分配压力计数**（`VmServer::mark_alloc_failure`），主循环 `alloc_cycle` 钩子（main.c:118-119）保留为补充/回收机会（体 DEFERRED 归 24-page-cache）。

**同步**：plan.md §2 06 行 Rust 模块列、checklist.md M-127-M-130/F-012/F-158/F-159 行、06-page-allocator.md §3.3 三处一致。

---

## 8. 参见

- `draft/` — 旧主线全部素材（00~26 + 99 + TODO + review 产物）
- `checklist.md` — 覆盖检查表（顶层保留，函数级基线）
- `../01-stage-kernel/00-kernel-overview.md` — 讲述结构参照（阶段划分/组织原则/过渡章节）
- `../00-master-plan/README.md` — 目录重排与新主线说明
- `minix3/minix/servers/vm/` — C 源码（ground truth）
- `os/servers/vm/src/` — Rust 实现
