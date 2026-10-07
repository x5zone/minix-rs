# 02-stage-vm 文档重建蓝图（deepseek）

> 本文是 R 相（重建蓝图）交付物：只定义新目录与每篇契约，不修改任何正文，不迁移任何文件。
> 第二阶段（B 相）按本蓝图逐篇重写；旧文档整体归档不删，引用按第 8 节批量迁移。

## 0. 元数据

- 执行者：deepseek
- 日期：2026-09-19
- 本轮修订（2026-09-19，补做轮）：订正 §9.2 自检门 G7 的契约计数（26→27，§4/§5 均为 27 篇）；订正 `phys_region.h:8-24` 两处锚点为 `8-21`（该头文件共 23 行，`struct phys_region` 定义在 8–21 行）。
- 目标目录：`rewrite-notes/02-stage-vm/`
- 仓库根目录：`/home/xzhao/github/minix-rs`
- 当前提交号：`124d52c48506bb0172665d9de6fea328189972f8`（工作区另有与本任务无关的未提交改动，未触碰）
- 交付物：本文件 `02-stage-vm/doc_rerank_deepseek.md`（唯一落盘产物；执行过程未写入其它文件）

### 0.1 审查范围

- **文档**（重建对象）：`00-vm-overview.md` ~ `26-vm-queries.md` 共 27 篇 + `99-global-concepts.md`，合计 16,178 行。
- **参考材料**（不作为重建对象）：`plan.md`（443 行）、`todo.md`（510 行）、`checklist.md`（867 行）、`draft/`（旧 fork 主线素材 30 篇）、`archive/`（两份旧 TODO 归档）。
- **范围外**：`.design/` 与 `tmp_design_and_todo/`（项目规范禁引，本蓝图零引用、未读取）；其它 AI 的 `doc_rerank_*` 产物（未读取）；`os/` 其它服务、`os/kernel/`、`os/arch/` 的内部实现细节（仅作跨阶段边界的引用对象，不展开）。
- 审查覆盖：27 篇正文全文 + 每篇头部声明；C 源 `minix3/minix/servers/vm/` 全部 24 个 `.c` + 20 个头文件逐文件核对；Rust `os/servers/vm/src/` 52 个 `.rs`（30,973 行）按模块入口核对；非 C 制品逐项核对（见 §0.3）；跨阶段边界材料核对（见 §0.4）。

### 0.2 读取清单

**C 源码（ground truth，24 个 .c，9,260 行）**：`acl.c` `alloc.c` `break.c` `cache.c` `exit.c` `fdref.c` `fork.c` `main.c` `mem_anon.c` `mem_anon_contig.c` `mem_cache.c` `mem_directphys.c` `mem_file.c` `mem_shared.c` `mmap.c` `pagefaults.c` `pagetable.c` `pb.c` `region.c` `regionavl.c` `rs.c` `slaballoc.c` `utility.c` `vfs.c`；头文件 `glo.h` `vm.h` `vmproc.h` `pt.h` `region.h` `phys_region.h` `memtype.h` `cache.h` `fdref.h` `proto.h` `cavl_if.h` `cavl_impl.h` `regionavl_defs.h` `unavl.h` `sanitycheck.h` `memlist.h` `util.h` `arch/i386/pagetable.h` `arch/earm/pagetable.h`。

**运行时库 C 源（边界核对）**：`minix3/minix/lib/libsys/sef.c` `sef_init.c` `sef_ping.c`；`minix3/minix/kernel/main.c` `table.c` `system/do_vmctl.c` `proc.h`。

**Rust 实现**：`os/servers/vm/src/`（`vm_server.rs` `ipc/` `region/` `phys_mem/` `vmproc/` `pagetable/` `memtype.rs` `page_cache.rs` `cow_exec_pf.rs` `mmap.rs` `munmap.rs` `brk.rs` `exit.rs` `fork.rs` `map_phys.rs` `rs.rs` `query.rs` `vfs_queue.rs` `fdref.rs` `kernel_gateway.rs` `direct_map.rs` `heap_arena.rs` `alloc_page.rs` `global.rs` `boot.rs` `acl.rs` `audit.rs` `sanity.rs` `dma.rs` `alloc_stats.rs`）；`os/libs/minix-types/src/ipc/`（`vm.rs` `message.rs` `rprocpub.rs` `rs_start.rs`）；`os/libs/minix-sef/` `os/libs/minix-elf/` `os/libs/minix-sys/`；`os/arch/src/arch/{paging,direct_map,dm_coverage}.rs`、`os/arch/src/x86_64/paging.rs`；`os/kernel/src/{vm.rs,syscall.rs,trap_dispatch.rs,page_fault.rs,cross_space.rs}`；`os/qemu-tests/`、`os/tests/`。

**边界材料**：`00-master-plan/README.md`（阶段划分与启动因果链）、`edge_todo.md`（E1/E2/E-RSWIRE/E-VFSWIRE/E-VMTLB/E5 等跨阶段条目）、本目录 `plan.md`、`todo.md`、`checklist.md`、`01-stage-kernel/00-kernel-overview.md`（前序概念去重依据）、`01-stage-kernel/06-todo.md`（契约写法参照）。

### 0.3 非 C 制品读取清单（第四部分第 3 类逐项）

| 类别 | 制品 | 位置 |
|------|------|------|
| 链接与加载 | 内核侧 VM ELF 装载、`exec_bootproc` + libexec + minix-elf 装载器、`__minix_init` libc 构造分界线 | `minix3/minix/servers/vm/main.c:294-417,474-495`；`minix3/minix/lib/libsys/sef*.c`；`os/libs/minix-elf/src/lib.rs`（1,007 行）；`os/servers/vm/src/boot.rs` |
| 镜像与内存布局 | VM/kernel Direct Map 双窗口尺寸与基址、VM 堆区间、64 位 mmap 区间、物理内存 map | `os/servers/vm/src/direct_map.rs:14-31`；`os/arch/src/arch/direct_map.rs`；`os/kernel/src/dm_coverage.rs`；`os/servers/vm/src/mmap.rs`（`MMAP_BASE/MMAP_TOP`） |
| 汇编入口与陷阱进入 | `#PF` 转发 `VM_PAGEFAULT`、`sys_call`/`SYS_VMCTL` 陷入、boot 期 `sys_exec` 切用户态 | `minix3/minix/kernel/arch/i386/exception.c:93-125`；`minix3/minix/kernel/system/do_vmctl.c`；`os/kernel/src/page_fault.rs`、`trap_dispatch.rs`；`os/arch/src/x86_64/trap_stub.rs` |
| 启动装配 | boot_image 登记顺序、RTS_VMINHIBIT/BOOTINHIBIT、RS_INIT 握手与异步回复 | `minix3/minix/kernel/table.c:44-64`；`minix3/minix/kernel/main.c:185-270`；`minix3/minix/lib/libsys/sef.c:113-200`、`sef_init.c:191-260`；`os/libs/minix-sef/src/lib.rs` |
| 构建与工具链 | workspace feature（`bitmap_alloc`/`buddy_alloc`/`segment_tree_alloc`/`sanity_checks`/`vmstats`/`vm_acl_audit`）、`panic=abort`、no_std 约束 | `os/servers/vm/Cargo.toml`；`os/Cargo.toml` |
| 跨模块接口与线格式 | `com.h` 调用号、`ipc.h` 消息结构、minix-types 专用 wire struct、transid、`rprocpub`、`SYS_VMCTL` 参数字段 | `minix3/minix/include/minix/{com.h,ipc.h,vfsif.h,rs.h}`；`os/libs/minix-types/src/ipc/{vm.rs,message.rs,rprocpub.rs}`；`os/kernel/src/vm.rs`；`os/servers/vm/src/kernel_gateway.rs` |
| 错误路径 | errno 映射、ENOSYS/EPERM 闸、fail-fast/fail-closed、审计与计数 | `os/servers/vm/src/ipc/dispatcher.rs` 错误映射表；`vm_server.rs` 计数器；`src/audit.rs` |
| 关闭与退出 | 两阶段退出、VM 实例退出、Live Update 旧实例让位、VM 崩溃策略（不可重启） | `minix3/minix/servers/vm/exit.c`；`os/servers/vm/src/exit.rs`；`os/Cargo.toml`（`panic="abort"`） |
| 并发与同步 | 单线程事件循环、SEF 信号打断、`AssumeSyncCell`、内核 BKL 边界、SMP TLB（E-VMTLB） | `os/servers/vm/src/vm_server.rs`（`run_once`）；`os/servers/vm/src/vmproc/table.rs`；`16-pagefault.md §3.7`（旧） |
| 测试基建 | `SimPaging`/`MockGateway` 注入、三后端 parity 测试、feature 矩阵、QEMU 冒烟、跨 crate 测试 | `os/servers/vm/src/pagetable/sim.rs`；`os/servers/vm/src/kernel_gateway.rs`；`os/servers/vm/src/phys_mem/allocator_tests.rs`；`os/qemu-tests/`；`os/tests/pm_vm_fork_test.rs`；`tools/coverage-extract/`、`tools/design-coverage-check.sh` |

### 0.4 使用的命令与关键输出（证据摘录）

```bash
# C 符号覆盖（只统计 28 篇正式文档，排除 draft/.design/doc_rerank_*）
python3 tools/coverage-extract/coverage-extract.py vm /tmp/opencode/vm-rerank/docs \
  --rust-dir os --c-dir minix3/minix/servers/vm \
  --semantic-map tools/coverage-extract/vm-semantic-map.json --output /tmp/opencode/vm-rerank/SYMBOLS-official.md
→ Loaded semantic map: 81 entries / Found 371 C symbols
→ Total C symbols: 371 / Doc covered: 306 (82.5%) / Rust name-match: 42 (11.3%)
→ 完全缺口（无文档提及）: 65 个，其中含 include guard 与 AVL 宏内部符号

# 规模核对
wc -l minix3/minix/servers/vm/*.c            → 24 文件 9,260 行
find os/servers/vm/src -name '*.rs' | wc -l  → 52 文件（文档 00/06 自称 51，已漂移）
wc -l [0-9][0-9]-*.md                         → 28 文档 16,178 行

# 头部声明核对（节选）
main.c:93-194    main()：is_first_time → init_vm → sef_local_startup → 主循环
main.c:428-587   init_vm()：getkinfo→filemap→chunks→vmproc→acl→region→mem→init_proc→pt_init
                 →__minix_init→total_pages→exec_bootproc 循环→CALLMAP→VM_INSTANCE→llvm 区
main.c:112-192   主循环：alloc_cycle→receive→notify 过滤→vm_isokendpt→transid→RS_INIT
                 →VM_PAGEFAULT→ACL+CALLMAP→SUSPEND/回复
pagefaults.c:76-417  handle_pagefault / handle_memory_* 状态机 / do_memory
sef.c:113-134    VM fresh 跳过启动期阻塞 RS_INIT 接收，改由自身主循环处理
sef_init.c:191-260  do_sef_init_request → process_init → 回调 → init_response
kernel/main.c:185-270  仅 kernel 任务+RS+VM 立即可调度；其余挂 RTS_VMINHIBIT|BOOTINHIBIT
kernel/table.c:44-64   boot_image 17 项登记顺序（ds→rs→…→vm→…→init）

# 引用关系
rg -o '\b[0-9]{2}-[a-z0-9-]+\.md\b' [0-9][0-9]-*.md | sort | uniq -c   → 篇间引用热点前五：
  15-ipc-dispatch(28) 13-region-mapping(24) 12-memtype(17) 01-vm-init-main(17) 07-pagetable-struct(15)
rg -c 'draft/' [0-9][0-9]-*.md           → 13 篇正文含 draft/ 引用（共 14 处，P0-process 违规）
rg -c '\.design/' [0-9][0-9]-*.md        → 0（正式文档未引 .design/）
rg -c --glob '*.md' '02-stage-vm/[0-9]{2}-[a-z0-9-]+\.md'（排除本目录）→ 外部引用热点：
  01-stage-kernel/07-cross-space-init.md(14) 05-stage-vfs/25-exec.md(4) 04-stage-pm/07-pm-fork.md(4)
```

---

## 1. C 真序（运行时真序重建）

### 1.1 阶段类型判定

本 stage 同时具备两种特征，按主从处理：

1. **启动链型（前置骨架）**：VM 是全系统第一个用户态服务。它的诞生顺序是硬性的：内核加载 VM ELF → VM 建立自己的页表 → VM 为其余 boot 服务建页表并解抑制 → 进入事件循环 → RS 握手授权。这一段必须按启动时序讲。
2. **服务事件循环型（主体）**：进入 `main()` 主循环之后，VM 是一个单线程事件循环服务。主体组织采用"一次请求的生命周期"而非源码调用顺序：收消息 → 五优先级判定 → 处理 → 回复/挂起。

因此本蓝图的叙事骨架是：**诞生（启动段）→ 循环与接口 → 数据与机制 → 按场景分组的请求处理 → 查询与跨服务协议**。

### 1.2 真序表：启动段（内核移交 → 进入主循环）

| 步 | 动作 | C 函数与文件锚点 | 说明 |
|----|------|-----------------|------|
| S1 | 内核登记 boot image 17 项（kernel task 5 + 用户服务 12） | `minix3/minix/kernel/table.c:44-64` | 登记顺序≠执行顺序；VM 是第 12 项 |
| S2 | 内核只让 kernel 任务/RS/VM 立即就绪，其余进程挂 `RTS_VMINHIBIT|RTS_BOOTINHIBIT` | `minix3/minix/kernel/main.c:185-270`；`proc.h:151,166` | VM 必须先行为其余服务建页表 |
| S3 | 内核用 `arch_boot_proc` 装载 VM ELF（ptproc）并授权 | `main.c:196-267`；`01-stage-kernel/06-proc-init-boot-proc.md` | 内核→VM 的 ELF 装载协议 |
| S4 | `main()`：`is_first_time()` 判定冷启动 | `minix3/minix/servers/vm/main.c:93-107,79-88` | 以 RS 的 `RTS_BOOTINHIBIT` 为判据；LU/restart 时跳过 |
| S5 | `init_vm()` 步骤 1-3：`sys_getkinfo` → `enable_filemap` → `get_mem_chunks` | `main.c:442-455`；`utility.c:44-79` | 拿到 memory map/模块表/内核占用；文件 mmap 默认开 |
| S6 | `init_vm()` 步骤 4-7：`memset(vmproc)`+`vm_slot` → `acl_init` → `map_region_init` → `mem_init` | `main.c:458-471`；`acl.c:21`；`region.c:36`；`alloc.c:306` | 进程表/门禁/区域索引/物理分配器依次接管 |
| S7 | `init_vm()` 步骤 8-9：`init_proc(VM_PROC_NR)` + `pt_init()` | `main.c:474-475`；`main.c:262-286`；`pagetable.c:1088-1356` | `pt_init` 自建页表：spare 池→`pt_bind`→`pt_init_done=1`→动态页重建→两次 `VMCTL_FLUSHTLB` |
| S8 | `init_vm()` 步骤 10：`__minix_init()`（堆可用分界线） | `main.c:480` | 之后 VM 自己的 libc/堆可用（Rust 侧对应 GlobalAlloc 注册） |
| S9 | `init_vm()` 步骤 11：`mem_add_total_pages` 两笔校准（boot 模块 + 内核占用） | `main.c:485-495`；`alloc.c:281-284` | 内核 freelist 不含 boot 模块；总量补齐 |
| S10 | `init_vm()` 步骤 12：遍历 boot 进程 `init_proc`+`exec_bootproc`+释放 blob | `main.c:498-520`；`main.c:331-417` | 每个 boot 服务：建页表→libexec 装载 ELF→最小栈→`sys_exec`→`BOOTINHIBIT_CLEAR` |
| S11 | `init_vm()` 步骤 13：`memset(vm_calls)`+`CALLMAP` 26 条 | `main.c:522-575` | 分发表注册；`do_munmap` 承担 3 个调用号，`do_remap` 承担 2 个 |
| S12 | `init_vm()` 步骤 14-15：`num_vm_instances=1`+`VMF_VM_INSTANCE`；SEF 特殊 mmap 区登记 | `main.c:577-586` | LU 计数起点；`%MMAP_ALL` 区 |
| S13 | `sef_local_startup()`：注册 fresh/LU/restart/signal 回调与异步回复 | `main.c:219-239` | VM fresh 时注册 `sef_cb_init_response_rs_asyn_once` 防启动死锁 |
| S14 | SEF 启动：正常服务阻塞等 RS_INIT；**VM fresh 例外**，直接进自身主循环 | `libsys/sef.c:113-134` | 这是 VM 独有的启动路径 |
| S15 | 主循环 P2 收 `RS_INIT`：`do_sef_init_request` → `process_init` → `sef_cb_init_fresh`（rproctab safecopy + `map_service` 授权）→ 异步回复 → `SUSPEND` | `main.c:149-152,241-260`；`sef_init.c:191-260`；`main.c:755-768` | VM 的 ACL 来源；回复不阻塞调用者 |
| S16 | SEF 信号路径：SYSTEM notify → `SIGKMEM` → `do_memory`；补 spare；`pt_clearmapcache` | `main.c:731-750`；`sef.c:222` | 内核借内存的主动路径入口 |

### 1.3 真序表：循环段（主循环五优先级）

| 步 | 动作 | C 函数与文件锚点 | 说明 |
|----|------|-----------------|------|
| L1 | 循环顶：`missing_spares>0 → alloc_cycle()` | `main.c:118-120`；`alloc.c:227-237` | 分配压力补充（Rust 侧链已随 A-1 删除，见 §3.2） |
| L2 | `sef_receive_status(ANY)` 收消息（ping/signal/init 由 SEF 拦截） | `main.c:122`；`sef.c:149` | 单线程阻塞点；信号会打断此调用 |
| L3 | 消息分类 1：`is_ipc_notify` → 丢弃 | `main.c:125-129` | 非 SIGNAL 类通知无意义 |
| L4 | 调用者验证：`vm_isokendpt(who_e)` 失败即 panic | `main.c:131-132`；`utility.c:84-101` | 三层验证（槽号/占用/endpoint 相等） |
| L5 | 消息分类 2：`TRNS_GET_ID` 提取 VFS 事务号 | `main.c:141`；`vfsif.h:79-81` | 仅 VFS 来源可能带 |
| L6 | 优先级 P1：VFS transid → `do_procctl(msg, transid)` | `main.c:143-148`；`exit.c:117-156` | 异步续作回连；回复带 transid |
| L7 | 优先级 P2：`RS_INIT` 且来源 RS → 握手 → `SUSPEND` | `main.c:149-152` | 不回复，由 SEF 异步应答 |
| L8 | 优先级 P3：`VM_PAGEFAULT` → 来源位检查 → `do_pagefaults` → `continue` | `main.c:153-164`；`pagefaults.c:240-243` | 不回复；成功由 `VMCTL_CLEAR_PAGEFAULT` 恢复进程 |
| L9 | 优先级 P4：`CALLMAP` 命中 → `acl_check` → handler | `main.c:165-175`；`acl.c:37-61` | 拒绝时打印告警并按 ENOSYS 回复；handler 见下表 |
| L10 | 优先级 P5：越界/未注册 → `ENOSYS` | `main.c:139,165-166` | 统一默认 |
| L11 | 回复：`result != SUSPEND` 时 `ipc_send`，失败 panic | `main.c:178-191` | SUSPEND 是伪返回码 |

### 1.4 真序表：handler 子链（请求处理）

| 请求族 | 入口 | 子链（关键函数，C 锚点） |
|--------|------|------------------------|
| 缺页 | `do_pagefaults` `pagefaults.c:240` | `handle_pagefault`(:76)→`map_lookup`→`map_pf`(`region.c:664`)→memtype 回调；异步经 `pf_cont`(:161) 重试→`VMCTL_CLEAR_PAGEFAULT`(:156) |
| 内核内存保障 | `do_memory` `pagefaults.c:294` | `sys_vmctl_get_memreq`→`handle_memory_start`(:254)→`handle_memory_step`(:336)→（VFS 路径）`handle_memory_continue`(:170)→`handle_memory_final`(:198) |
| fork | `do_fork` `fork.c:32` | 验证→子槽初始化(:54)→`pt_new`+`map_proc_copy_range`(`region.c:944`)→ACL/标志(:83)→`sys_fork` 注册(:89)→消息页 `handle_memory_once`(:97) |
| brk | `do_brk` `break.c:44` | `real_brk`(:62)→`map_region_extend_upto_v`(`region.c:1002`)→`anon_resize`(`mem_anon.c:115`) |
| mmap | `do_mmap` `mmap.c:200` | `mmap_region`(:36) 三路解析→匿名或 `mmap_file`(:84)→`mappedfile_setfile`(`mem_file.c:191`) |
| mmap 文件续作 | `do_vfs_mmap` `mmap.c:135` | `vfs_request`(`vfs.c:60`)→`SUSPEND`→`mmap_file_cont`(:160) |
| remap/map_phys | `do_remap` `mmap.c:366` / `do_map_phys` `mmap.c:310` | `map_perm_check`(:284)→`shared_setsource`(`mem_shared.c:167`) / `phys_setphys`(`mem_directphys.c:69`) |
| munmap 族 | `do_munmap` `mmap.c:512` | `munmap_vm_lin`(:488)（VM 自身）/ `map_unmap_range`(`region.c:1222`)→`map_unmap_region`(:1065)→`split_region`(:1150)→`map_subfree`(:527)/`map_free`(:568) |
| exit 族 | `do_willexit` `exit.c:100` / `do_exit` `exit.c:60` / `do_procctl` :117 | `free_proc`(:33)→`map_free_proc`(`region.c:589`)→`clear_proc`(:45)；`VMPPARAM_CLEAR`→`pt_new`+`pt_bind`；`VMPPARAM_HANDLEMEM`→`handle_memory_start` |
| VFS 回复 | `do_vfs_reply` `vfs.c:109` | `activate`(:43)→回调（如 `pf_cont`/`handle_memory_continue`/`mmap_file_cont`） |
| 缓存 | `do_mapcache`/`do_setcache`/`do_forgetcache`/`do_clearcache` `mem_cache.c:95/196/283/315` | `find_cached_page_bydev/ino`(`cache.c:177/198`)→`addcache`(:216)/`rmcache`(:259)→`cache_freepages`(:288) |
| RS | `do_rs_set_priv`/`prepare`/`update`/`memctl` `rs.c:34/71/150/349` | `acl_set`(`acl.c:70`)；`rs_memctl_*`(:218-344)；`map_proc_dyn_data`/`swap_proc_dyn_data`(`utility.c:283/312`) |
| 查询 | `do_info` `utility.c:100` / `do_get_phys` `mmap.c:438` / `do_get_refcount` :463 / `do_getrusage` `utility.c:426` | `get_usage_info(_kernel/_vm)`(`region.c:1357/1366/1395`)；`get_region_info`(:1452)；`map_get_phys`(:1323)/`map_get_ref`(:1343) |

---

## 2. 知识点全集

### 2.1 编制口径

- 知识点类型：`概念` / `机制` / `结构`（数据结构）/ `接口`（接口与协议）/ `约束`（约束与不变量）/ `演进`（架构演进）/ `工程`（工具与工程）/ `测试`（测试性质）。
- 来源类型：`存` = 来自现有 27 篇文档；`新` = 现有文档没有，由 C 源码、非 C 制品或操作系统通用概念承载（§3.1 覆盖审计发现）。
- 「现有位置」只写旧编号与小节（如 `05 §2.4`）；`draft/` 与 `.design/` 一律不作为来源。
- 「锚点」优先 C 源；非 C 主题用制品路径；操作系统理论无源码锚点的标 `理论`。
- 同一知识点在多篇重复出现的，合并为一条并在行末括号列出其余位置与「主讲述点」。

### 2.2 知识点池总表

#### 组 A：导读与诞生（新 00-01）

| 编号 | 名称 | 类型 | 源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----|----------|------|----------|
| K-001 | VM 是内存语义权威（账本/仲裁/策略三重角色） | 概念 | 存 | 00 §1.1-1.2 | `fork.c`/`exit.c`/`pagefaults.c` 全族 | 知职责边界 |
| K-002 | 内核/VM 分工协议：内核转发 #PF、执行 sys_vmctl；VM 不碰 CR3 | 概念 | 存 | 00 §1.1、01 §2.2.1 | `kernel/arch/i386/exception.c:93-125`；`system/do_vmctl.c` | 懂跨主体协议 |
| K-003 | 单线程事件循环执行模型与借用检查器作为并发审计 | 约束 | 存 | 00 §1.2、15 §1.6 | `vm_server.rs`（VmContext/run_once） | 懂并发安全来源 |
| K-004 | 结构不变量「VM 只改不在运行的进程的页表」与 TLB 纪律 | 约束 | 存 | 16 §3.7 | `pagefaults.c:153`；`cow_exec_pf.rs` sync_slot_pte | 懂 PTE 写安全 |
| K-005 | VM 三重权威的数据结构索引（vmproc/region/PageFrames；memtype/cache/allocator） | 结构 | 存 | 00 §1.2 | `glo.h`；`region.h`；`page_state.rs` | 建整体心智模型 |
| K-006 | 内核启动移交：ptproc、boot_image 顺序、RTS_BOOTINHIBIT、VM 首个可调度 | 机制 | 存 | 00 §1.3、01 §1.1 | `kernel/main.c:185-270`；`table.c:44-64` | 懂 VM 的诞生条件 |
| K-007 | `is_first_time` 门控与 `__vm_init_fresh` | 机制 | 存 | 01 §2.1 | `main.c:79-88,100-107` | 懂冷启动/LU 分支 |
| K-008 | `init_vm()` 十五步启动时序 | 机制 | 存 | 01 §2.2 | `main.c:428-587` | 定位每步 |
| K-009 | 启动契约数据：KernelInfo/BootParams（memmap/模块表/user_sp/内核布局） | 接口 | 存 | 01 §2.2.1、§3.1 | `main.c:442-495`；`boot.rs` | 懂内核给什么 |
| K-010 | `get_mem_chunks` 字节→click 取整与 16 槽上限 | 机制 | 存 | 05 §2.2（主）、01 §2.2.2 | `utility.c:44-79` | 懂物理块清单 |
| K-011 | `__minix_init` 堆可用分界线 | 机制 | 存 | 01 §2.2.4、09 §1.1 | `main.c:480` | 懂何时能用堆 |
| K-012 | `exec_bootproc`：建页表→libexec 装载 ELF→最小栈→`sys_exec`→解抑制 | 机制 | 存 | 01 §2.3.2 | `main.c:294-417` | 懂其他服务怎么起 |
| K-013 | libexec/minix-elf 装载协议与 `vm_exec_info` 桥接结构 | 接口 | 存+新 | 01 §2.3.2（存）；`vm_exec_info` 为新 | `main.c:288-329`；`os/libs/minix-elf/src/lib.rs` | 懂 ELF 装载契约 |
| K-014 | boot blob 释放与 `mem_add_total_pages` 两笔校准 | 机制 | 存 | 01 §2.2.5、05 §2.7 | `main.c:485-520`；`alloc.c:281` | 懂真实物理总量 |
| K-015 | SEF 生命周期：回调注册、fresh/LU/restart 分支、异步回复防死锁 | 机制 | 存 | 01 §2.4、15 §2.6 | `main.c:219-239`；`sef.c:100-134`；`sef_init.c:191-260` | 懂启动握手 |
| K-016 | RS_INIT 握手与 `map_service` 授权（rproctab safecopy→`acl_set`） | 接口 | 存 | 01 §2.4.3/§2.5、25 §2.7 | `main.c:149-152,241-260,755-768` | 懂 ACL 来源 |
| K-017 | SEF 信号路径：`SIGKMEM→do_memory`、补 spare、`pt_clearmapcache` | 机制 | 存 | 01 §2.4.5 | `main.c:731-750`；`sef.c:222` | 懂信号打断 |
| K-018 | LU/restart 启动路径：槽交换、动态数据转移、`adjust_proc_refs`、multi-LU IPC filter | 机制 | 存 | 01 §2.4.4、10 §1.8、25 §2.7 | `main.c:196-217,592-726`；`utility.c:188-335` | 懂热更新起点 |
| K-019 | VM 自身 libc 接口边界与 Rust GlobalAlloc 替代（A-3 v2） | 演进 | 存 | 01 §2.6/§3.7、09 §3.1 | `utility.c:361-420`；`global.rs` | 懂 ARCH 差异 |
| K-020 | VM 崩溃策略：不可重启、`panic=abort`、fail-fast/fail-closed | 约束 | 存+新 | 00 §3.3（存）；策略成篇为新 | `os/Cargo.toml`；`vm_server.rs` run 循环 | 懂失败语义 |

#### 组 B：入口与契约（新 02-05）

| 编号 | 名称 | 类型 | 源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----|----------|------|----------|
| K-021 | vmproc 字段全集与五组职责 | 结构 | 存 | 02 §2.1 | `vmproc.h:14-32` | 懂每字段用途 |
| K-022 | 双重身份：`vm_slot` 与 `vm_endpoint`（generation<<15+slot） | 结构 | 存 | 02 §1.2、03 §2.4 | `vmproc.h:22-23`；`endpoint.h:45-69` | 懂寻址与复用防护 |
| K-023 | VMF_* 正交状态位与生命周期状态机 | 结构 | 存 | 02 §1.3/§2.2 | `vmproc.h:35-37`；`exit.c:60-113` | 懂状态组合 |
| K-024 | 进程表 VMP_NR=NR_PROCS+1、槽分配/查找/遍历、VMP_EXECTMP 保留槽 | 结构 | 存 | 03 §1.1/§2.1/§2.2 | `glo.h:17-20`；`main.c:457-462` | 懂表的集合语义 |
| K-025 | `vm_isokendpt` 三层验证与 EINVAL/EDEADEPT | 机制 | 存 | 03 §2.3 | `utility.c:84-101` | 懂门卫 |
| K-026 | `init_proc` 激活与 `clear_proc`/`free_proc` 回收 | 机制 | 存 | 02 §2.3/§2.5 | `main.c:262-286`；`exit.c:33-58` | 懂槽位生灭 |
| K-027 | `swap_proc_slot` 表级交换语义 | 机制 | 存 | 03 §2.6、10 §2.6 | `utility.c:188-216` | 懂 LU 交换 |
| K-028 | Rust typestate 视图（vacant/active/exiting）与静态表 `AssumeSyncCell` | 演进 | 存 | 02 §3.5、03 §3.1/§3.4 | `vmproc_handle.rs`；`table.rs` | 懂编译期约束 |
| K-029 | ACL 存在理由与四道防线中的位置 | 概念 | 存 | 04 §1.1 | `main.c:165-173` | 懂为何要门禁 |
| K-030 | 调用号位图（call mask）与 VM_BASIC_CALLS 默认集 | 结构 | 存 | 04 §1.3、99 §2.1 | `com.h:769-780` | 懂权限载体 |
| K-031 | DEFAULT/SYSTEM 分层与 NO_ACL/未初始化语义 | 约束 | 存 | 04 §1.2/§2.3 | `acl.c:10-12,37-61` | 懂默认策略 |
| K-032 | ACL 生命周期：`acl_init/set/check/fork/clear` | 机制 | 存 | 04 §1.4/§2.2-2.5 | `acl.c:21-129` | 懂权限流转 |
| K-033 | ARCH A-11：未初始化 fail-closed（相对 C 的语义偏移） | 演进 | 存 | 04 §3.3 | `acl.rs` AclState | 懂安全增强 |
| K-034 | 主循环骨架与五优先级判定顺序 | 机制 | 存 | 15 §1.2/§2.4 | `main.c:112-192` | 懂每轮做什么 |
| K-035 | SUSPEND 伪返回码协议 | 接口 | 存 | 15 §1.3 | `main.c:178-191`；`com.h:1151` | 懂延迟回复 |
| K-036 | VFS transid 编码与路由（含 `do_procctl_notrans`） | 接口 | 存 | 15 §1.4/§2.5、22 §2.9 | `vfsif.h:79-81`；`main.c:143-148,419-426` | 懂事务回连 |
| K-037 | `is_ipc_notify` 过滤与伪造 `VM_PAGEFAULT` 防御 | 约束 | 存 | 15 §1.5 | `main.c:125-129,153-157`；`ipcconst.h:28` | 懂来源校验 |
| K-038 | CALLMAP 分发表与 handler 复用（`do_munmap`×3、`do_remap`×2）+ 4 条 ENOSYS 调用 | 接口 | 存 | 15 §2.2/§2.3/§4.5 | `main.c:47-59,522-575`；`com.h:637-679` | 懂调用号全集 |
| K-039 | 调用者验证与「handler 首行验证」模式 | 约束 | 存 | 03 §2.5、15 §1.5 | `main.c:131-132`；各 handler | 懂防御模式 |
| K-040 | 错误路径策略：ENOSYS/EPERM、fail-fast、fail-closed、审计与计数 | 约束 | 存+新 | 15 §3.6/§3.7、04 §4.4（存） | `dispatcher.rs` errno 表；`vm_server.rs` 计数器；`audit.rs` | 懂失败语义 |
| K-041 | VM 请求码族与编号布局（VM_RQ_BASE/NR_VM_CALLS=49） | 接口 | 存 | 15 §2.1、99 §2.1 | `com.h:627-780`；`minix-types vm.rs` | 查任一调用号 |
| K-042 | 56 字节消息与 union/m1..m10 线格式 | 结构 | 存 | 15 §2.1、99 §2.1 | `minix3/minix/include/minix/ipc.h`；`minix-types message.rs` | 懂线格式基座 |
| K-043 | 专用 wire struct 纪律与 M1 overlay 陷阱（19/20/21/22 修复教训） | 约束 | 存 | 19 §3.5、20 §4.3、21 §3.1、22 §3.6（分散） | `minix-types vm.rs` 各 struct + size 断言 | 防解码错位 |
| K-044 | 回复编码两段式（errno + 载荷）与槽位约定 | 接口 | 存 | 15 §4.6、各服务 §回复 | `encode.rs`；`minix-types` | 懂回复构造 |
| K-045 | 内核调用通道：SYS_VMCTL 参数/回复全表与 KernelGateway seam | 接口 | 存+新 | 01 §3.1/09 §3.1（散）；成篇为新 | `system/do_vmctl.c:32-167`；`os/kernel/src/vm.rs:754-800`；`kernel_gateway.rs` | 懂内核接口面 |
| K-046 | BootParams/KernelInfo 与内核移交格式（详细版） | 接口 | 存 | 01 §3.1（部分） | `boot.rs`；`kernel/main.c:270-276` | 懂交接字段 |
| K-047 | VFS 对话 wire：`VFS_VMCALL`/`VM_VFS_REPLY`（mess_10）、transid | 接口 | 存 | 23 §2.2 | `com.h:694-714`；`ipc.h:85-91`；`minix-types vm.rs` | 懂 m10 错位陷阱 |
| K-048 | RS 握手 wire：`RS_INIT`/`rprocpub`/`rs_start` | 接口 | 存 | 25 §2.1/§2.7 | `rs.h:165-199`；`minix-types rprocpub.rs` | 懂权限下发格式 |
| K-049 | 线格式审计纪律：`size_of`/`offset_of` 断言与单源常量 | 工程 | 存+新 | 25 §3.10（部分）；成篇为新 | `minix-types` 断言；`test_callmap_registration_matches_c` | 防双源漂移 |

#### 组 C：内存地基（新 06-09）

| 编号 | 名称 | 类型 | 源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----|----------|------|----------|
| K-050 | 内存来源链：VM 不探测只消费（memmap→chunks） | 概念 | 存 | 05 §1.1、01 §2.2.2 | `utility.c:44-79`；`param.h:MAXMEMMAP` | 懂物理从哪来 |
| K-051 | Click 单位系统与转换宏 | 概念 | 存 | 05 §1.2 | `const.h:84-101` | 懂页号/字节换算 |
| K-052 | 连续块分配问题与位图/伙伴/段树三种表达 | 概念 | 存 | 05 §1.3/§1.6、06 §1.1 | `alloc.c:33-51`；`phys_mem/*` | 懂分配器形态 |
| K-053 | 分配约束四维：对齐/低端/清零/连续（PAF_*） | 约束 | 存 | 05 §1.5、99 §2.2 | `vm.h:22-27` | 懂分配旗标 |
| K-054 | `mem_init` 接管与初始空闲区登记（逆序扫描） | 机制 | 存 | 05 §2.3 | `alloc.c:306-335` | 懂分配器启动 |
| K-055 | `alloc_mem` 对齐预加大 + 失败重试；`alloc_pages/findbit` 位图扫描 | 机制 | 存 | 05 §2.4/§2.5 | `alloc.c:242-279,369-460` | 懂分配路径 |
| K-056 | `free_mem/free_pages` 幂等释放与缓存回填 | 机制 | 存 | 05 §2.6 | `alloc.c:289-301,465-481` | 懂释放路径 |
| K-057 | 三后端策略模式（A-5）与运行时选择规则 | 演进 | 存 | 05 §1.6/§3.1/§3.3 | `phys_mem/mod.rs`；`vm_server.rs` choose | 懂后端取舍 |
| K-058 | 记账与统计：total_pages/memstats/vsi | 接口 | 存 | 05 §1.7/§2.7/§2.8 | `glo.h:45`；`alloc.c:281,348-367` | 懂统计口径 |
| K-059 | `usedpages_*` 双分配检测与 mem_sanitycheck（A-7） | 工程 | 存+新 | 05 §2.10（部分） | `alloc.c:338-346,501-545` | 懂调试防线 |
| K-060 | 保留队列 `reservedqueue_*` 与 `missing_spares` 守恒（C） | 结构 | 存 | 05 §2.9、06 §2.8 | `alloc.c:56-237` | 懂自举供给 |
| K-061 | Rust 自举元数据放置（BumpBuf/预映射/DM） | 演进 | 存 | 05 §3.2 | `phys_mem/mod.rs` | 懂 64 位差异 |
| K-062 | 页表是 MMU 翻译数据；多级结构与地址空间宽度（A-2/A-6） | 概念 | 存 | 07 §1.1、00 §1.1 | `pt.h`；`arch/i386/vm.h` | 懂页表本质 |
| K-063 | PTE 位布局与 `ARCH_VM_*` 宏族（含 earm 变体，A-10） | 接口 | 存 | 07 §2.2/§2.3/§2.4 | `arch/i386/pagetable.h:10-45`；`arch/earm/pagetable.h` | 懂硬件位 |
| K-064 | 双视图问题：MMU 用 PA、VM 用 VA（C `pt_t`） | 概念 | 存 | 07 §1.2/§1.3 | `pt.h:11-25` | 懂访问难题 |
| K-065 | Direct Map 常量偏移（A-1）与双窗口（VM/kernel） | 演进 | 存 | 06 §1.5、07 §1.4/§3.2 | `direct_map.rs`；`arch/direct_map.rs` | 懂 VA=PA+offset |
| K-066 | VM 自身页表与自映射（A-9）、`pt_init` 结构面 | 机制 | 存 | 07 §1.5/§2.5/§2.6 | `pagetable.c:1088-1356`；`vm_self_map.rs` | 懂自己从哪来 |
| K-067 | `pt_bind`/`pt_mapkernel` 的结构语义（内核映射登记） | 机制 | 存 | 07 §2.7、08 §2.11 | `pagetable.c:1358-1489` | 懂内核映射 |
| K-068 | 生命周期三件套 `pt_new/pt_free/pt_bind` | 机制 | 存 | 08 §1.2/§2.10 | `pagetable.c:990-1026,1358-1437` | 懂页表生灭 |
| K-069 | WMF 写映射四模式与 MAP_NONE 哨兵 | 接口 | 存 | 08 §1.3/§2.4 | `vm.h:56-61` | 懂写模式 |
| K-070 | `pt_writemap` 核心写路径与 PTE 组装 | 机制 | 存 | 08 §2.4 | `pagetable.c:784-935` | 懂映射写入 |
| K-071 | 页表页按需分配 `pt_ptalloc/_in_range` 与递归副作用协议 | 机制 | 存 | 06 §2.3、08 §1.4/§2.1/§2.2 | `pagetable.c:494-584` | 懂递归链 |
| K-072 | 跨表复制 `pt_copy`/`pt_map_in_range`/`pt_ptmap`（fork/LU 用途） | 机制 | 存 | 08 §1.5/§2.7/§2.8/§2.12 | `pagetable.c:631-748,1069-1087` | 懂复制三兄弟 |
| K-073 | 查询校验 `pt_checkrange`/`pt_writable`（SANITYCHECKS 用途） | 接口 | 存 | 08 §1.6/§2.5/§2.6 | `pagetable.c:761,943` | 懂诊断 |
| K-074 | TLB 纪律：C 全局清缓存 vs Direct Map 逐条 invlpg；写与失效绑定 | 演进 | 存 | 08 §1.8/§3.6 | `pagetable.c:751`；`x86_64/paging.rs` write_pte_dm | 懂 TLB 正确性 |
| K-075 | 内核 VMINHIBIT 停等与 `VMCTL_FLUSHTLB`（SMP 缺口挂 E-VMTLB） | 约束 | 存+新 | 08 差异表（存）；成篇为新 | `pagetable.c:799-815,928-934`；`proc.c:345-347` | 懂 SMP 前提 |
| K-076 | Rust `Paging` trait 操作面与消费方 | 接口 | 存 | 08 §3.1-§3.8/§4 | `arch/paging.rs`；`x86_64/paging.rs` | 懂操作抽象 |
| K-077 | VM 自用页分配（`vm_allocpage` 族）与双地址问题 | 机制 | 存 | 06 §1.1/§2.1 | `pagetable.c:235-395` | 懂自用页 |
| K-078 | 备用页池自举两阶段（`pt_init_done`/`level`）与稳态换血 | 机制 | 存 | 06 §1.3/§1.4/§2.10 | `pagetable.c:54-57,264-292,1305-1341` | 懂自举 |
| K-079 | `vm_freepages`/`vm_mappages`/`vm_pagelock`/`vm_addrok` | 机制 | 存 | 06 §2.2/§2.4/§2.6/§2.7 | `pagetable.c:235-326,403-455` | 懂四配套 |
| K-080 | ARCH A-1：保留页池结构消除与压力计数替代 | 演进 | 存 | 06 §3.3、10 §1.7/§3.4 | `vm_server.rs`（链已删，注释在） | 懂消除边界 |
| K-081 | `vm_pt_alloc` 页表页供给链注册 | 接口 | 存 | 06 §3.5 | `alloc_page.rs`；`pt_alloc` 注册 | 懂供给注册 |
| K-082 | C slab 分配器：尺寸分类/页内位图/空闲链/反查 | 结构 | 存 | 09 §1.3/§2.1-§2.7 | `slaballoc.c:29-528` | 懂自举堆 |
| K-083 | `SLABALLOC/SLABFREE` 宏与 slab 消费方全景 | 接口 | 存 | 09 §1.5/§2.10 | `proto.h:133-134`；`pb.c`/`region.c`/`cache.c` | 懂 C 类型约定 |
| K-084 | MEMPROTECT 写保护与 JUNK 双释放检测 | 机制 | 存 | 09 §1.6/§2.8 | `slaballoc.c:42-116`；`pagetable.c:403` | 懂调试硬化 |
| K-085 | Rust 堆：HeapArena 连续 VA + VmAllocator free-list + GlobalAlloc | 演进 | 存 | 09 §3.1-§3.3/§4.1/§4.2 | `heap_arena.rs`；`global.rs` | 懂 A-3 v2 |
| K-086 | `PAGE_ALLOC_PTR` 注册协议与自举安全 | 接口 | 存 | 09 §3.6/§4.3 | `global.rs` | 懂全局分配器接线 |
| K-087 | 稳态化搬迁：C `pt_init` 搬迁段 vs Rust `relocate`（元数据） | 演进 | 存 | 10 §1.3/§1.6/§3.1 | `pagetable.c:1311-1345`；`vm_server.rs` relocate | 懂自举终点 |
| K-088 | LU 支撑原语：槽交换/动态数据/`map_setparent`/`transfer_mmap_regions` | 机制 | 存 | 10 §2.6/§2.7 | `utility.c:188-335`；`region.c:1535` | 懂 LU 基座 |

#### 组 D：地址空间账本（新 10-13）

| 编号 | 名称 | 类型 | 源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----|----------|------|----------|
| K-089 | 两层物理页对象（phys_block/phys_region）与三层结构 | 结构 | 存 | 11 §1.1-§1.3/§2.1 | `region.h:23-35`；`phys_region.h:8-21` | 懂物理侧模型 |
| K-090 | 引用计数语义与不变量（归零条件/rm 参数） | 约束 | 存 | 11 §1.3/§2.5/§2.6 | `pb.c:61-134` | 懂页生命周期 |
| K-091 | 反向引用链表与三消费者 | 概念 | 存 | 11 §1.4、17 §2.5 | `pb.c:30,96-134` | 懂谁在引用 |
| K-092 | PBF_INCACHE 标志 | 结构 | 存 | 11 §1.5、24 §2.1 | `region.h:35` | 懂缓存持有 |
| K-093 | PFN 索引模型（PageFrames/PageSlot/PageFlags）与两态化 | 演进 | 存 | 11 §1.6/§3.1-§3.4 | `page_state.rs` | 懂 Rust 重写 |
| K-094 | refcount 宽度/饱和与 `verify_refcounts` 审计 | 约束 | 存 | 11 §3.3/§4.4 | `page_state.rs`；`sanity.rs` | 懂一致性检查 |
| K-095 | `PfnAllocator` trait（含 `alloc_contiguous`）与 memtype 借页 | 接口 | 存 | 11 §3.5、12 §3.4 | `page_state.rs:33` | 懂分配抽象 |
| K-096 | 多态内存语义分发动机与框架/策略分离 | 概念 | 存 | 12 §1.1/§1.3 | `region.c:664`；`memtype.rs` | 懂为何多态 |
| K-097 | `mem_type_t` 15 字段回调表与 NULL 默认语义 | 结构 | 存 | 12 §1.2 | `memtype.h:12-31` | 懂回调面 |
| K-098 | 匿名内存语义：缺页三态/refcount 可写/resize/split/lowshrink | 机制 | 存 | 12 §1.4/§2.2/§2.3 | `mem_anon.c:56-151` | 懂 anon |
| K-099 | 直接物理映射语义：不分配不释放、不可分裂 | 机制 | 存 | 12 §1.5/§2.4、21 §2.8 | `mem_directphys.c:28-78` | 懂设备页 |
| K-100 | 共享内存语义：递归缺页/remaps/源设置/`shared_delete` | 机制 | 存 | 12 §1.6/§2.5、17 §4.5 | `mem_shared.c:49-210` | 懂 shm |
| K-101 | 连续匿名语义：预分配/拒 fork/增长 | 机制 | 存 | 12 §1.7/§2.6 | `mem_anon_contig.c:45-131` | 懂 DMA 页 |
| K-102 | cache/mappedfile 类型概览与能力差异 | 概念 | 存 | 12 §1.8 | `mem_cache.c`；`mem_file.c:20-287` | 懂文件页类型 |
| K-103 | Rust `MemType` trait 六实现/`PagefaultResult`/能力门控 | 演进 | 存 | 12 §1.9/§3.1-§3.5/§4 | `memtype.rs` | 懂策略落地 |
| K-104 | 共享源递减落点决策（G-V12-7）与 region id | 演进 | 存 | 12 §3.7 | `region/mod.rs` release_shared_remap；`vir_region.rs` id | 懂借用安全改型 |
| K-105 | 区域映射解决什么问题（地址空间分段模型与不重叠不变量） | 概念 | 存 | 13 §1.1/§1.2 | `region.h:37-78` | 懂区域模型 |
| K-106 | `vir_region` 结构与 VR_* 标志、`def_memtype` 正交 | 结构 | 存 | 13 §1.2/§2.1 | `region.h:37-78` | 懂区域属性 |
| K-107 | `physblock_get/set` 槽位契约与 `vm_total` 记账 | 接口 | 存 | 13 §2.2、11 §2.7 | `region.c:60-96` | 懂槽位读写 |
| K-108 | 有序索引五向搜索与路径栈迭代器（C AVL 模板） | 结构 | 存 | 14 §1.2-§1.4/§2.1-§2.6 | `cavl_if.h`；`cavl_impl.h` | 懂索引机制 |
| K-109 | 空槽查找：`region_find_slot_range`/`find_slot` 与 hint | 机制 | 存 | 14 §1.5/§2.7 | `region.c:302-416` | 懂地址分配 |
| K-110 | ARCH A-4：BTreeMap 替代 AVL 与 SearchType | 演进 | 存 | 14 §3.1-§3.4 | `region_map.rs` | 懂 Rust 索引 |
| K-111 | `map_lookup` 地址解析与消费面 | 机制 | 存 | 13 §2.4 | `region.c:616-641` | 懂查区域 |

#### 组 E：区域操作与运行机制（新 13-15）

| 编号 | 名称 | 类型 | 源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----|----------|------|----------|
| K-112 | 区域框架操作族总览（建/查/填/复制/扩/缩/释放/工具） | 概念 | 存 | 13 §1.4 | `region.c` 22 函数族 | 懂操作全景 |
| K-113 | `map_page_region`/`region_new` 建区时序 | 机制 | 存 | 13 §2.3 | `region.c:424-510` | 懂建区 |
| K-114 | `map_pf` 三阶段填充与 SUSPEND | 机制 | 存 | 13 §1.5/§2.5、16 §2.3 | `region.c:664-754` | 懂单页填充 |
| K-115 | 批量填充族：`map_handle_memory`/`map_pin_memory`/`map_writept`/`map_ph_writept` | 机制 | 存 | 13 §2.5 | `region.c:257-295,756-906` | 懂批量路径 |
| K-116 | `map_copy_region` 复制即共享与回滚 | 机制 | 存 | 13 §1.6/§2.6、17 §2.1 | `region.c:802-849` | 懂 fork 省页 |
| K-117 | `map_proc_copy(_range)` 整进程复制 | 机制 | 存 | 13 §2.6、18 §2.4 | `region.c:933-999` | 懂 fork 复制 |
| K-118 | `map_region_extend_upto_v` 扩展与冲突检查 | 机制 | 存 | 13 §2.7、19 §2.4 | `region.c:1002-1060` | 懂 brk 扩展 |
| K-119 | 拆除族：`map_unmap_region` 三情形/`split_region`/`map_unmap_range` 四情形 | 机制 | 存 | 13 §1.7/§2.8、21 §2.4-§2.6 | `region.c:1065-1294` | 懂拆映射 |
| K-120 | 释放族：`map_subfree`/`map_free`/`map_free_proc` 与顺序无关性 | 机制 | 存 | 13 §2.9、21 §2.7、22 §2.5 | `region.c:527-612` | 懂区域回收 |
| K-121 | `ev_lowshrink`/`ev_split` 能力门控与 directphys 拒绝 | 约束 | 存 | 21 §1.3/§2.5/§2.6、13 §2.8 | `region.c:1096,1164` | 懂分裂边界 |
| K-122 | 工具面：`vrallocflags`/`physregions`/调试打印/sanity | 工程 | 存+新 | 13 §2.10、05 §2.10 | `region.c:40-168,645,1510` | 懂诊断 |
| K-123 | Rust 顶层函数分散与 `free_region_pages` 统一释放入口 | 演进 | 存 | 13 §3.4/§4.3、21 §3.4 | `region/mod.rs:23-90` | 懂 Rust 落点 |
| K-124 | CoW 动机与生命周期四阶段（建立/保护/触发/分裂） | 概念 | 存 | 17 §1.1/§1.2 | `region.c:820-849`；`pb.c`；`mem_anon.c` | 懂 CoW 为何 |
| K-125 | 共享建立：`pb_reference`/`pb_link` 与 `fork_region` 的 refcount++ 与回滚 | 机制 | 存 | 17 §2.1/§3.1 | `pb.c:61-91`；`fork.rs` fork_region | 懂共享 |
| K-126 | 写保护：`prepare_cow` + `setup_cow_for_all_regions` + `write_page_table_mappings` | 机制 | 存 | 17 §2.2/§3.2 | `vir_region.rs`；`vmproc_handle.rs` | 懂 RO 设置 |
| K-127 | 可写判定：`pr_writable`/`is_page_writable` 与 memtype writable 回调 | 约束 | 存 | 17 §1.3/§2.2 | `region.c:130-134`；`memtype.rs` writable | 懂何时可写 |
| K-128 | 分裂执行 `mem_cow` 五步与 memtype 换 anon | 机制 | 存 | 17 §1.4/§2.3 | `pb.c:136-168`；`cow_exec_pf.rs` | 懂分裂 |
| K-129 | refcount≤1 快速路径的合取前提（V13-P1-1） | 约束 | 存 | 16 §3.3 | `cow_exec_pf.rs`；`mem_file.c:173-177` | 懂快路安全 |
| K-130 | 文件后备 CoW（`cow_block`/clearend）与 VFS 交互边界 | 机制 | 存 | 17 §2.4、23 §2.4 | `mem_file.c:59-82` | 懂文件页分裂 |
| K-131 | 两阶段释放 `unmap_page`/`ev_unreference` 与 IN_CACHE | 机制 | 存 | 17 §3.5、11 §4.3 | `vir_region.rs`；`page_state.rs` | 懂释放 |
| K-132 | C 缺陷修复对照（丢弃返回值/预分配泄漏）与差异表 | 工程 | 存 | 17 §2.1/§2.4/§3.6 | `region.c:836-842`；`mem_anon.c:75-92` | 懂改进点 |
| K-133 | 两条缺页入口（被动 #PF/主动 SIGKMEM）与汇合点 | 机制 | 存 | 16 §1.1-§1.4/§2.3 | `pagefaults.c:240,294`；`region.c:664` | 懂缺页全貌 |
| K-134 | 被动路径 `handle_pagefault` 流程与 SIGSEGV 分支 | 机制 | 存 | 16 §2.2 | `pagefaults.c:76-158` | 懂处理步骤 |
| K-135 | 主动路径状态机 `hm_state`/`handle_memory_*` | 机制 | 存 | 16 §2.4 | `pagefaults.c:39-49,170-289` | 懂异步保障 |
| K-136 | `handle_memory_step` 同步分支三条件与 `vfs_avail` | 约束 | 存 | 16 §2.5/§2.6 | `pagefaults.c:294-417` | 懂防死锁 |
| K-137 | SUSPEND 恢复协议（`pf_cont` 重试/`VMCTL_CLEAR_PAGEFAULT`） | 接口 | 存 | 16 §1.4/§2.2 | `pagefaults.c:140-168,161` | 懂恢复 |
| K-138 | major/minor 缺页统计 | 结构 | 存 | 16 §1.5 | `pagefaults.c:135-138` | 懂统计 |
| K-139 | `pf_errstr` 与错误码宏 PFERR_* | 工程 | 存 | 16 §2.1/§2.7 | `pagefaults.c:59-70`；`arch/i386/pagetable.h:36-39` | 懂诊断 |
| K-140 | Rust 缺页动作表/`PagefaultAction`/解码与 PTE 不变量 | 演进 | 存 | 16 §3.1-§3.7/§4 | `cow_exec_pf.rs`；`memtype.rs` | 懂 Rust 实现 |

#### 组 F：服务编排（新 16-20）

| 编号 | 名称 | 类型 | 源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----|----------|------|----------|
| K-141 | fork 三方职责（PM/VM/kernel）与次主线路径 | 概念 | 存 | 18 §1.1/§1.2 | `fork.c:32-115`；`kernel/system/do_fork.c` | 懂分工 |
| K-142 | `PFF_VMINHIBIT` 同步契约与消息页 | 接口 | 存 | 18 §1.3/§2.6 | `com.h:360`；`do_fork.c:112-116` | 懂停等 |
| K-143 | `do_fork` 七阶段编排与子进程初始化 | 机制 | 存 | 18 §2.1-§2.5 | `fork.c:32-115`；`region.c:933-999` | 懂全流程 |
| K-144 | ACL/标志继承与 endpoint 合成（`sys_fork`） | 机制 | 存 | 18 §2.5/§2.6、04 §2.5 | `fork.c:83-108`；`acl.c:110-116` | 懂继承 |
| K-145 | 两层回滚与不可回滚点 | 约束 | 存 | 18 §1.4/§3.4 | `fork.c:70-80,89-108` | 懂失败边界 |
| K-146 | `handle_memory_once` 消息页预填充 | 机制 | 存 | 18 §2.6/§4.4、16 §3.5 | `fork.c:97-108`；`pagefaults.c:245` | 懂预解析 |
| K-147 | Rust fork 编排/回滚/SAFETY 模式 | 演进 | 存 | 18 §3.1-§3.6/§4 | `fork.rs` | 懂 Rust 实现 |
| K-148 | 堆位置与 brk/sbrk 协议（`_brksize` 去重） | 概念 | 存 | 19 §1.1/§1.2 | `libc/sys/brk.c:24-34`；`break.c:3-17` | 懂堆 |
| K-149 | C「只增不减」语义与 Rust 收缩差异 | 演进 | 存 | 19 §1.3/§1.4 | `region.c:1016`；`brk.rs` shrink | 懂行为差异 |
| K-150 | `do_brk`/`real_brk` 入口与 ENOMEM 统一 | 机制 | 存 | 19 §2.2/§2.3 | `break.c:44-69` | 懂入口 |
| K-151 | `anon_resize`/`ev_resize` 只增语义（A-12） | 机制 | 存 | 19 §2.5/§3.2 | `mem_anon.c:115-130` | 懂回调 |
| K-152 | Rust 三态编排（grow/shrink/no-change）与释放物理页 | 演进 | 存 | 19 §3.1-§3.3/§4 | `brk.rs` | 懂 Rust 实现 |
| K-153 | mmap = 地址分配 + 来源绑定；三路地址解析 | 概念 | 存 | 20 §1.0-§1.2 | `mmap.c:36-83,200-269` | 懂 mmap 本质 |
| K-154 | 权限模型：`PROT_*`/execpriv/UNINITIALIZED | 约束 | 存 | 20 §1.4/§2.2 | `mmap.c:208-221`；`mman.h` | 懂权限 |
| K-155 | 匿名 vs 文件分流与异步两段式（FDLOOKUP→续作） | 机制 | 存 | 20 §1.3/§2.4 | `mmap.c:84-195,254-273` | 懂文件映射 |
| K-156 | `do_vfs_mmap`（VFS 主动映射）与 `MAP_PREALLOC` | 接口 | 存 | 20 §2.3/§2.5 | `mmap.c:46,135-158` | 懂 exec 映射 |
| K-157 | `do_remap`/`REMAP_RO` 共享区域重映射 | 机制 | 存 | 20 §2.7 | `mmap.c:366-435`；`mem_shared.c:167` | 懂 remap |
| K-158 | `map_perm_check`/`do_map_phys` 与 TTY/MEM 豁免 | 接口 | 存 | 20 §2.6、21 §3.5 | `mmap.c:284-363` | 懂物理映射 |
| K-159 | munmap 四入口统一与 target/长度语义 | 机制 | 存 | 21 §1.1/§2.1/§2.2 | `mmap.c:512-573`；`com.h:649-679,718` | 懂拆除入口 |
| K-160 | 拆除四步与引用计数兑现（四场景表） | 概念 | 存 | 21 §1.2/§1.4 | `pb.c:96-134`；`region.c:1222-1294` | 懂页命运 |
| K-161 | `munmap_vm_lin`：VM 自身页表直清 | 机制 | 存 | 21 §2.3 | `mmap.c:488-510` | 懂自身拆除 |
| K-162 | Rust `unmap_range` 四情形/`free_region_pages`/fdref 平衡 | 演进 | 存 | 21 §3.3/§3.4 | `munmap.rs`；`region/mod.rs` | 懂 Rust 实现 |
| K-163 | 两阶段退出协议与 VMF_EXITING 门 | 接口 | 存 | 22 §1.1/§2.2/§2.3 | `exit.c:60-114`；`pm/forkexit.c:332,455` | 懂退出协议 |
| K-164 | `free_proc`/`clear_proc`/`reset_vm_rusage` 分工 | 机制 | 存 | 22 §2.4 | `exit.c:25-58` | 懂回收 |
| K-165 | 释放链与 fdref 归还 | 机制 | 存 | 22 §2.5/§3.2 | `region.c:589-612`；`exit.rs` | 懂最后一引用 |
| K-166 | VM 实例计数与 `VM_INSTANCE` 语义 | 约束 | 存 | 22 §2.3/§3.2 | `exit.c:76-81`；`glo.h:46` | 懂 LU 实例 |
| K-167 | `VM_PROCCTL` 通道：CLEAR/HANDLEMEM | 接口 | 存 | 22 §1.3/§2.7 | `exit.c:117-156`；`com.h:752-760` | 懂进程控制 |
| K-168 | Rust typestate 退出/资源释放链/错误码映射 | 演进 | 存 | 22 §3.1-§3.7/§4 | `exit.rs`；`vmproc_handle.rs` | 懂 Rust 实现 |

#### 组 G：跨服务协作与查询（新 21-24）

| 编号 | 名称 | 类型 | 源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----|----------|------|----------|
| K-169 | VM↔VFS 三类请求（FDLOOKUP/FDIO/FDCLOSE）与职责分工 | 接口 | 存 | 23 §1.1/§2.1 | `com.h:694-704`；`vfs.c:60-142` | 懂跨服务链路 |
| K-170 | 串行激活请求队列（单 active 不变量）与 `do_vfs_reply` req_id 校验 | 机制 | 存 | 23 §1.2/§2.1 | `vfs.c:31-142` | 懂序列化 |
| K-171 | fdref 三职责（去重/分裂计数/末尾关 fd）与 `mayclosefd` | 结构 | 存 | 23 §1.3/§2.3 | `fdref.c:93-176` | 懂 fd 生命周期 |
| K-172 | 文件页缺页三岔路、`referenced_offset`、VMSF_ONCE | 机制 | 存 | 23 §1.4/§2.4 | `mem_file.c:98-152` | 懂文件页 |
| K-173 | `mappedfile_setfile` prefill 与回调族（split/copy/lowshrink/delete） | 机制 | 存 | 23 §2.4 | `mem_file.c:177-287` | 懂区域回调 |
| K-174 | VFS 侧 `do_vm_call` 与 PEEKING 读闭环 | 机制 | 存 | 23 §2.7 | `vfs/misc.c:380-480`；`libminixfs/cache.c:298-465` | 懂对端 |
| K-175 | Rust `VfsRequestQueue`/`FdRefTable`/`NeedVfsIo`/`PendingFdClose` | 演进 | 存 | 23 §3.1-§3.5 | `vfs_queue.rs`；`fdref.rs`；`cow_exec_pf.rs` | 懂类型化队列 |
| K-176 | 页缓存为何在 VM | 概念 | 存 | 24 §1.1 | `mem_cache.c` 全 | 懂缓存归属 |
| K-177 | `cached_page` 结构与双键模型/延迟 ino 更新（C） | 结构 | 存 | 24 §1.2/§2.1/§2.2 | `cache.h:2-21`；`cache.c:163-214` | 懂目录 |
| K-178 | 精确 LRU 双链与 O(1) touch/remove | 结构 | 存 | 24 §1.3/§2.2 | `cache.c:29-75` | 懂淘汰序 |
| K-179 | refcount/INCACHE 淘汰条件与 `cache_freepages` 压力回收 | 约束 | 存 | 24 §1.3/§2.2 | `cache.c:243-307`；`alloc.c:260-262` | 懂回收 |
| K-180 | 四个缓存 IPC：mapcache/setcache/forgetcache/clearcache | 接口 | 存 | 24 §1.5/§2.4-§2.6 | `mem_cache.c:95-324`；`com.h:682-691` | 懂缓存服务 |
| K-181 | `CacheMemory` memtype 与 VMSF_ONCE 三处消费 | 机制 | 存 | 24 §1.4/§2.3 | `mem_cache.c:39-193` | 懂缓存类型 |
| K-182 | Rust 单键重写/索引型 LRU/once flag/回滚 | 演进 | 存 | 24 §3.1-§3.8/§4 | `page_cache.rs`；`cache_handlers.rs` | 懂 Rust 实现 |
| K-183 | LU 动机与三角色协作 | 概念 | 存 | 25 §1.1 | `rs.c` 全 | 懂热更新 |
| K-184 | 四请求：SET_PRIV/PREPARE/UPDATE/MEMCTL 与消息字段 | 接口 | 存 | 25 §1.2/§2.1 | `rs.c:34-390`；`com.h:724-766` | 懂服务面 |
| K-185 | LU 窗口零分配约束与预分配（heap/map prealloc） | 约束 | 存 | 25 §1.3/§2.3/§2.6 | `rs.c:73-145,281-344` | 懂约束 |
| K-186 | UPDATE 的 SUSPEND 与手动回复（endpoint 对调） | 机制 | 存 | 25 §1.4/§2.4 | `rs.c:150-213` | 懂切换 |
| K-187 | RprocTab/`adjust_proc_refs` 与槽交换后的 parent 重绑 | 机制 | 存 | 25 §1.5/§1.6/§2.7/§2.8 | `main.c:241-260,755-768`；`utility.c:477-492` | 懂引用重绑 |
| K-188 | MAKE_VM/`VM_INSTANCE` 与 `SF_VM_*` 旗标 | 机制 | 存 | 25 §2.1/§2.6 | `rs.c:218-276`；`rs.h:198-199` | 懂 VM 自更新 |
| K-189 | A-8 缺口契约与 fail-closed（含 MAKE_VM 恒拒登记） | 演进 | 存 | 25 §4.8 | `rs.rs` MakeVm 臂注释；`25 §3.9` | 懂缺口 |
| K-190 | Rust RsError/类型化请求/落地状态 | 演进 | 存 | 25 §3.1-§3.10/§4 | `rs.rs`；`dispatcher.rs` | 懂 Rust 实现 |
| K-191 | 查询为何归 VM 与四请求面 | 概念 | 存 | 26 §1.1/§1.2 | `com.h:720-764` | 懂查询职责 |
| K-192 | `do_info` 三模式（STATS/USAGE/REGION）与 special endpoints | 机制 | 存 | 26 §1.2/§2.2 | `utility.c:100-184` | 懂 INFO |
| K-193 | GETPHYS/GETREF 真相（region id/remaps）与能力门控 | 概念 | 存 | 26 §1.3/§2.3/§2.4 | `mem_anon.c:132-145`；`mem_shared.c:99-108` | 懂查询语义 |
| K-194 | `sys_datacopy` 死锁与钉页（`handle_memory_once`） | 约束 | 存 | 26 §1.4 | `utility.c:166-171` | 懂复制安全 |
| K-195 | 用量统计 `get_usage_info(_kernel/_vm)` 与栈启发式 | 机制 | 存 | 26 §2.6 | `region.c:1357-1447` | 懂 rss |
| K-196 | 区域列表用段/跳过/游标/`MAX_VRI_COUNT` | 机制 | 存 | 26 §2.6/§3.7 | `region.c:1452-1505`；`vm.h:66` | 懂分页 |
| K-197 | GETRUSAGE PM-only 与 children 零值 | 接口 | 存 | 26 §2.5 | `utility.c:426-472` | 懂 rusage |
| K-198 | Rust QueryError/InfoQuery/编码槽位 | 演进 | 存 | 26 §3.1-§3.9/§4 | `query.rs`；`encode.rs` | 懂 Rust 实现 |

#### 组 H：工程与参考（新 25、99）

| 编号 | 名称 | 类型 | 源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----|----------|------|----------|
| K-199 | 注入缝：`KernelGateway`（Trap/Mock）与 `SimPaging` | 工程 | 存 | 00 §4.1、15 §3.3、16 §4.1 | `kernel_gateway.rs`；`pagetable/sim.rs` | 懂可测性设计 |
| K-200 | 三后端 parity 测试与 feature 矩阵 | 测试 | 存 | 05 §5、06 §5、00 §3.2 | `phys_mem/allocator_tests.rs`；`Cargo.toml` | 懂回归基线 |
| K-201 | QEMU 冒烟与跨 crate 测试 | 测试 | 新 | 无成篇 | `os/qemu-tests/`；`os/tests/pm_vm_fork_test.rs` | 懂端到端 |
| K-202 | 可观测性：audit 宏/计数器/vmstats/sanity_checks | 工程 | 存+新 | 各篇 §5 散见 | `audit.rs`；`vm_server.rs` 计数器；`sanity.rs` | 懂观测面 |
| K-203 | 测试基线纪律（单源统计、禁止各篇自报）与覆盖率工具链 | 工程 | 新 | 各篇 §5.4（问题源） | `tools/coverage-extract/`；`tools/design-coverage-check.sh` | 防数字漂移 |
| K-204 | 调用号族与常量（NR_VM_CALLS/VM_*） | 结构 | 存 | 99 §2.1 | `com.h:627-780`；`minix-types vm.rs` | 查常量 |
| K-205 | 物理分配旗标 PAF_*（含 ALIGN16K=0x40 修正） | 结构 | 存 | 99 §2.2 | `vm.h:22-27`；`phys_mem/types.rs` | 查旗标 |
| K-206 | 写映射旗标 WMF_* 与区域旗标 VR_*/MF_* | 结构 | 存 | 99 §2.2/§2.3 | `vm.h:56-61`；`region.h:69-78` | 查旗标 |
| K-207 | endpoint/generation 编码与特殊端 | 接口 | 存 | 99 §3.3、03 §2.4 | `endpoint.h:45-69`；`endpoint.rs` | 懂寻址 |
| K-208 | 全局状态显式化三档与哨兵类型化 | 演进 | 存 | 99 §1.2/§3.2 | `global.rs`；`page_state.rs` | 懂设计纪律 |
| K-209 | 单源常量纪律与权威位置判据 | 工程 | 存 | 99 §3.1/§3.4 | `minix-types`；`NR_VM_CALLS` | 防双源 |

### 2.3 统计摘要

- 知识点总数：**209**（K-001 ~ K-209）。
- 按类型：概念 22、机制 73、结构 21、接口 33、约束 22、演进 27、工程 9、测试 2。
- 按来源：存量 198、新增 11（其中 9 条为「存量补强 + 新增锚点」：K-013 装载协议、K-020 崩溃策略、K-040 错误策略、K-045 内核通道、K-049 线格式审计、K-059 双分配检测、K-075 停等机制、K-122 诊断工具、K-202 可观测性；2 条为全新增：K-201 QEMU/跨 crate 测试、K-203 统计口径与覆盖率工具链）。§3.2 缺口落实中 G-01/G-09/G-10 的内容已并入 K-013/K-045/K-009，不重复计数。
- 按旧文档分布（同一知识点可对应多篇旧文，故仅列主要素材来源篇，不求和）：篇均对应最多的为 15、13、12、05、16、17、20、22、23、25、26；其次为 01、02、03、04、06、07、08、11、21、24；09/10/14/18/19/99 各承载 4-7 条主要素材。
- 主讲述点标记：重复主题的主讲述点见 §3.3 表。

---

## 3. 覆盖审计

### 3.1 主题全集来源

主题全集由四路合成，逐条与知识点池对账：

1. **C 源码符号**：`coverage-extract` 机器枚举 371 个（189 函数 / 14 结构体 / 167 宏 / 1 枚举）。正式文档覆盖 306（82.5%），完全缺口 65。
2. **操作系统通用概念**（不依赖具体代码）：地址翻译与 TLB、按需分页/工作集、CoW、引用计数、物理帧分配算法（bitmap/buddy/segment-tree）、内存映射文件、页缓存与 LRU、IPC 事务协议、访问控制矩阵、热更新与状态迁移、单线程事件循环、失败原子性与回滚。
3. **非 C 制品**：§0.3 十类逐项。
4. **阶段边界契约**：`00-master-plan/README.md`（启动因果链）、`edge_todo.md`（E1/E2/E-RSWIRE/E-VFSWIRE/E-VMTLB/E5）、本目录 `plan.md` §2/§4/§5（ARCH A-1~A-12、明确排除项）、`todo.md` §18（V13 开口与 edge）、`01-stage-kernel/00-kernel-overview.md`（前序已讲概念：内存语义权威、内核/VM 分工、sys_vmctl 协议、VM_PAGEFAULT 转发——本 stage 不重复展开，只作前置引用）。

### 3.2 覆盖缺口表

> 机器缺口 65 个（只统计正式文档），逐类裁决如下；「主题集」列括注对应知识点编号。

| # | 主题 | 证据 | 裁决 |
|---|------|------|------|
| G-01 | `vm_exec_info` 桥接结构（libexec opaque 参数） | `main.c:288-292` | **新建并入 `01-vm-birth`**（K-013） |
| G-02 | `memlist` 链表（reservedqueue 页记录） | `memlist.h`；`alloc.c:137-149` | 并入 `06-physical-memory` 保留队列小节（K-060），不单列 |
| G-03 | `anon_contig_pt_flags`/`anon_contig_writable` | `mem_anon_contig.c:37-42,122-125` | 并入 `11-memtype` 的 contig 契约（K-101/K-103） |
| G-04 | `pt_sanitycheck`/`mem_sanitycheck`/`cache_sanitycheck_internal`/`fdref_sanitycheck`/`map_sanitycheck`/`slab_sanitycheck`/`sanitycheck_queues`/`sanitycheck_rq` | 各 `.c`；A-7 | 归入 `25-test-observability`（K-202/K-059）；各机制文档只给一句「调试校验归 25」 |
| G-05 | `map_printregion`/`printregionstats`/`printmemstats` | `region.c:40,1510`；`alloc.c:486` | 并入 `13-region-ops`/`06-physical-memory` 的诊断小节（K-122/K-058） |
| G-06 | ARM PTE 旗标 `PTF_SUPER`/`PTF_CACHEWB`/`PTF_CACHEWT`/`PTF_SHARE`/`ARCH_VM_PAGE_PRESENT` | `arch/earm/pagetable.h:14-37`；`arch/i386/pagetable.h:21` | 并入 `07-page-tables` 多架构对照（K-063） |
| G-07 | `BITS_FULL`/`WRITABLE_HEADER`（slaballoc 内部） | `slaballoc.c:82,94` | 并入 `09-vm-self-memory` slab 小节（K-082） |
| G-08 | `VMP_SPARE`（自用页分配 reason） | `vm.h:49`；`pagetable.c:333` | 并入 `09-vm-self-memory` 自用页小节（K-077） |
| G-09 | `AM_AUTO`（异步消息自动目标） | `vm.h:32` | 并入 `05-message-contracts` 异步消息小节（新增 K-049 旁支） |
| G-10 | `MINSTACKREGION`（最小栈区域常量） | `vm.h:39` | 并入 `01-vm-birth`/`07` 布局常量（K-009/K-046 旁支） |
| G-11 | `MARK`/`SCL_*`（sanitycheck 调度标记） | `sanitycheck.h` | **明确排除**：编译期调试宏，语义已由 A-7 替代（K-202 提及） |
| G-12 | 头文件 include guard（`_MEMLIST_H` 等 10 个） | 各 `.h` | **明确排除**：非语义 |
| G-13 | AVL 模板内部宏（`L__*`、`AVL_IMPL_*`、`AVL_SET_*`、`AVL_NULL`、`AVL_SEARCH_TYPE_DEFINED_`） | `cavl_impl.h`/`cavl_if.h` | **明确排除**：宏展开细节；机制级语义归 `12-region-ledger`（K-108） |
| G-14 | `VERBOSE`/`LU_DEBUG`/`VMP_*` 调试开关 | `glo.h` 等 | **明确排除**：编译期日志开关；归 `25-test-observability` 一句 |

### 3.3 重复主题表

> 只列跨文档重复展开的主题（组内重复见 §3.6 各篇问题）。「新主讲述点」是重建后唯一的展开位置，其余文档只引用。

| # | 重复主题 | 现有重复位置 | 新主讲述点 | 其余处理 |
|---|---------|-------------|-----------|---------|
| D-01 | Direct Map（A-1） | 06 §1.5/§3.2、07 §1.4/§3.2/§4.2/§4.4、05 §3.2/§3.7、08 §1.8/§3.6 | `07-page-tables` | 全篇引用 07，不再重述 |
| D-02 | 保留队列/备用页池 | 05 §2.9、06 §1.3-1.4/§2.8-2.10、10 §1.5/§2.3 | `09-vm-self-memory`（含 C 保留队列） | 06 的 C 数据结构细节并入 09；05 只留 `free_mem` 回填钩子 |
| D-03 | `missing_spares`/`alloc_cycle` 主循环链 | 05 §3.8 S-5/§4.5、06 §2.9/§3.3/§4.3、15 §4.1、24 §3.7 | `09-vm-self-memory`（A-1 消除） | 修正三处过期叙述（链已删） |
| D-04 | `pt_ptalloc` 递归副作用 | 06 §2.3、07 §1.2/§2.5、08 §1.4/§2.1 | `08-page-table-ops` | 06/07 删除逐函数重述 |
| D-05 | `vm_pt_alloc` 页表页供给 | 06 §3.5、07 §4.4、08 §3.2/§4.2 | `09-vm-self-memory` | 07/08 只引用 |
| D-06 | VM 自映射页表（A-9） | 06 §3.6/§4.4、07 §1.5/§3.3/§4.3、08 §4.5 | `07-page-tables` | 08 只讲 adoption 调用点 |
| D-07 | WMF/写映射 / Paging 方法族 | 07 §3.4、08 §1.3/§2.4/§3.1 | `08-page-table-ops` | 07 只给结构面 `Paging` trait |
| D-08 | `pt_mapkernel` | 07 §2.7、08 §1.7/§2.11/§3.4 | `08-page-table-ops`（操作） | 07 只给登记语义一句 |
| D-09 | `pt_bind`/页表生命周期 | 07 §2.7、08 §1.2/§2.10、22 §2.6 | `08-page-table-ops` | 22 只讲 procctl CLEAR 调用点 |
| D-10 | TLB 失效策略 | 06 §3.2、07 D2、08 §1.8/§3.6 | `08-page-table-ops` | 06 只提「自举期整表刷」 |
| D-11 | boot 页表交接（adopt/establish_boot_dm/SetAddrSpace） | 05 §3.5、06 §3.5、07 §2.5/§3.4、08 §3.3/§4.3 | `07-page-tables`（VM 自身页表）；`01-vm-birth`（时序） | 05/06/08 清理 |
| D-12 | `init_vm` 启动时序图 | 01、09 §1.0、10 §1.0 | `01-vm-birth` | 09/10 删除时序图 |
| D-13 | HeapArena / `PAGE_ALLOC_PTR` | 09 §3.2/§3.6/§4、10 §3.1/§4.1/§4.3 | `09-vm-self-memory` | 10 只讲搬迁 |
| D-14 | PageSlot/PageFrames/refcount | 11 §3.2-§3.5/§4、12 §4.6、13 §3.2-§3.3、21 §1.4/§3.4、22 §1.2/§3.2 | `10-physical-page-state` | 13/20 只讲释放语义对它的消费 |
| D-15 | `physblock_get/set` + `vm_total` | 11 §2.7、13 §2.2 | `12-region-ledger` | 11 删除 |
| D-16 | memtype 框架/策略与回调清单 | 12 §1.2/§1.3、13 §1.1/§1.4、24 §2.3 | `11-memtype` | 13/24 只引用 |
| D-17 | `map_pf` 三阶段 | 13 §1.5/§2.5、16 §2.3 | `13-region-ops`（机制） | 15 只讲入口与状态机 |
| D-18 | AVL→BTreeMap（A-4）与 find_slot | 13 §2.3/§3.1/§4.1、14 §1.5-1.6/§3/§4.3 | `12-region-ledger` | 13 只讲操作调用 |
| D-19 | RS LU 支撑面 | 10 §1.8/§2.6-2.7/§3.5、25 §2.7/§2.8 | `09-vm-self-memory`（原语）+ `23-rs-live-update`（服务） | 双方各取一半，交叉引用 |
| D-20 | `map_sanitycheck`→`verify_refcounts` | 11 §4.4、13 §2.10 | `25-test-observability`（机制一句在 10） | 删除重复展开 |
| D-21 | SUSPEND 协议 | 15 §1.3/§2.5、16 §1.4、20 §3.3、21 §3.2、22 §3.5、25 §1.4 | `04-ipc-dispatch` | 各服务只讲「本请求何时 SUSPEND」 |
| D-22 | 主循环 P3/VM_PAGEFAULT 分支与来源校验 | 15 §2.4/§3.4/§1.5、16 §1.2/§2.1 | `04-ipc-dispatch` | 15 只讲消息格式 |
| D-23 | refcount 可写判定与快速路径 | 16 §3.3、17 §1.3/§2.2/§3.4 | `14-cow` | 15 只讲动作接口 |
| D-24 | `mem_cow`/`cow_resolve_core` 分裂步骤 | 16 §4.3、17 §2.3/§3.3/§4.3 | `14-cow` | 15 只引用 |
| D-25 | C 缺陷 1/2（丢弃返回值/预分配泄漏） | 17 §2.1/§2.4/§3.6、18 §2.4 | `14-cow` | 16 删除 |
| D-26 | 写保护 PTE 设置 | 16 §3.6#11、17 §2.2/§3.2、18 §3.4/§5.3 | `14-cow` | 16 只讲调用 |
| D-27 | `handle_memory_once` 同步变体 | 16 §3.5、18 §2.6/§4.4 | `15-pagefault` | 16 只讲 fork 的调用点 |
| D-28 | `setup_cow_for_all_regions`/`write_page_table_mappings` | 17 §3.2/§4.2、18 §3.1/§4.1 | `14-cow` | 16 只讲编排顺序 |
| D-29 | ACL 继承规则 | 04 §2.5、15 §3.6、18 §2.5/§3.3 | `03-acl-gate` | 16 只讲调用点 |
| D-30 | wire-format 修复记录（M1 overlay 教训） | 19 §3.5/§4.3、20 §4.3、21 §3.1、22 §3.6、23 §3.4 | `05-message-contracts`（纪律与教训） | 各服务只留「本消息的专用 struct」一句 |
| D-31 | errno 映射决策表 | 19 §3.4、20 §3.2、21 §3.2/§3.5、22 §3.3 | `04-ipc-dispatch`（统一表） | 各服务只列本服务特例 |
| D-32 | region split/unmap/free 原语 | 13 §2.8/§2.9、19 §3.3/§4.3、21 §2.4-§2.7/§3.3/§3.4、22 §2.5 | `13-region-ops` | 19/21/22 只讲服务编排 |
| D-33 | `free_region_pages`/`free_process_phys` 释放漏斗 | 13 §4.3、19 §3.3、21 §3.4、22 §3.2 | `13-region-ops` | 三篇只引用 |
| D-34 | 引用计数兑现表（页命运四场景） | 21 §1.4、22 §1.2 | `13-region-ops` | 22 删除 |
| D-35 | `map_phys`/`map_perm_check` | 20 §1.4/§2.6/§3.5、21 §2.8/§3.5 | `19-vm-munmap` | 18 只讲建立入口；00 声明 |
| D-36 | VMSF_ONCE | 23 §1.2/§3.5/§3.6#2、24 §1.4/§2.4/§3.4 | `22-page-cache` | 21 只引用 |
| D-37 | 缓存命中→FDIO→重试闭环 | 23 §1.1/§1.4/§2.7、24 §2.7/§4.4 | `21-vfs-file-mapping` | 22 只讲缓存侧 |
| D-38 | `VmReply::InfoRegion` 编码决策 | 23 §3.4（越界）、26 §3.7 | `24-vm-queries` | 21 删除 |
| D-39 | VFS transport 缺口与 `take_pending_vfs_call` | 23 §3.6#1/§4.4、24 §5.3、25 §4.6/§4.8、15 §3.3 | `21-vfs-file-mapping`（缺口清单主位）+ `05-message-contracts`（wire） | 以 `edge_todo.md E-VFSWIRE` 为唯一进度源 |
| D-40 | `map_service`/RS_INIT 握手 | 01 §2.4/§2.5、25 §1.5/§2.7 | `01-vm-birth` | 23 只引用 |
| D-41 | `adjust_proc_refs`/槽交换 | 10 §2.6/§4.3、25 §1.6/§2.8 | `09-vm-self-memory`（原语）+ `23-rs-live-update`（服务） | — |
| D-42 | 测试统计数字 | 15/16/17/18/19/20/21/22/23/24/25/26 §5.4 各报一套 | `25-test-observability`（单源口径） | 各机制文档删除统计节 |
| D-43 | Linux/Redox 对照节 | 09/12/13/14/15/16/17/18/19/20/21/22 各设一节 | 不设独立章节；对照文字只在「设计决策需要外部依据」处保留一段，并明确外部依据不冒充 ground truth | 删除十二处重复节 |

### 3.4 越界主题表

| # | 越界文档 | 越界内容 | 正确归属（新编号） |
|---|---------|---------|------------------|
| O-01 | 01 | §3.7 Drop 修复、§4.5 端点常量修正、§5.3 他篇测试失败 | `25-test-observability`；删除修复日志 |
| O-02 | 02 | §2.7 展开 `vm_acl`/`acl_init`/`acl_fork`/`acl_clear` | `03-acl-gate` |
| O-03 | 03 | §4.5/§4.6 各 handler 消费细节 | 各服务篇 |
| O-04 | 04 | dispatch 中 `acl_check` 细节重复 02 §4.4 | `03-acl-gate`（检查语义）/`04-ipc-dispatch`（接线一句） |
| O-05 | 05 | §3.2 aarch64 DM 可采纳性推导；§2.4/§3.8 S-3 cache LRU | `07-page-tables` / `22-page-cache` |
| O-06 | 05 | §3.8 S-5/§4.5 `missing_spares` 主循环细节 | `09-vm-self-memory` |
| O-07 | 06 | §2.3 递归链完整协议；§2.10/§1.4 页表重建；§4.1 PfnAllocator 集成 | `08-page-table-ops` / `07-page-tables` / `10-physical-page-state` |
| O-08 | 07 | §2.5⑥/§2.12 `pt_copy` 逐函数；§3.4/§4.4 walk_alloc；§4.4 kernel boot 覆盖 | `08-page-table-ops` / `07`（结构一句） |
| O-09 | 08 | §3.3/§3.4 A1 adoption/VMCTL 通道；§3.8/§4.3 kernel boot 覆盖；§4.5/§4.6 fork/exit/munmap 消费链 | `07-page-tables` / `01-vm-birth` / 各服务篇 |
| O-10 | 09 | §1.1/§4.4 `__minix_init` 启动链；§1.4/§2.4 `vm_freepages` | `01-vm-birth` / `09`（自用页小节点到为止） |
| O-11 | 10 | §2.2 `vm_freepages`/`vm_self_pages`；§4.5 RS 七步 | `07`/`08`；`23-rs-live-update` |
| O-12 | 11 | §1.5/§4.3 PBF_INCACHE/addcache/rmcache；§2.8 mem_cow | `22-page-cache`；`14-cow` |
| O-13 | 12 | §3.7 `release_shared_remap` 借用安全与 region id | `12-region-ledger`（一句） |
| O-14 | 13 | §2.3/§2.4/§3.1/§4.1/§5.1 查找语义与 RegionMap 测试 | `12-region-ledger` |
| O-15 | 14 | §2.8/§3.5/§4.4 区域生命周期与 brk/munmap 消费接线 | `13-region-ops` / 各服务篇 |
| O-16 | 15 | §2.6 SEF 生命周期；§4.3 `do_procctl` 分发；§4.6 回复编码；§5.1 各 handler 测试 | `01-vm-birth`；`20-vm-exit`；`05-message-contracts`；`25-test-observability` |
| O-17 | 16 | §3.3 CoW 快路语义；§3.6/§4.4 VFS 回调/TLB 现状；§4.2 MappedFile 缓存分流 | `14-cow`；`21-vfs-file-mapping`；`22-page-cache` |
| O-18 | 17 | §4.5 SharedMemory 源链路页错误；§2.1/§2.2 fork 上下文尾部双写 | `15-pagefault`（memtype 表行）；`16-vm-fork` |
| O-19 | 18 | §3.5/§4.4 handle_memory_once 安全论证；§1.3/§2.6 内核 sys_fork 内部 | `15-pagefault`；`01`/`05`（内核侧只引用） |
| O-20 | 19 | §3.2 find_mut/find_overlap；§5.3 跨服务缺口 | `12-region-ledger`；`25-test-observability` |
| O-21 | 20 | §3.3/§3.4 VfsRequestQueue/fdref 全表；§2.8 符号表列入 munmap/get_phys | `21-vfs-file-mapping`；`19`/`24` |
| O-22 | 21 | §2.7 `map_free_proc`；§3.4 FdRefTable 细节；§5.3 B5 缓存拆除 | `20-vm-exit`；`21-vfs-file-mapping`；`22-page-cache` |
| O-23 | 22 | §2.6 pt_free/pt_new/pt_bind；§2.8 handle_memory_start；§3.2/§4.3 VFS_FDCLOSE 设计 | `08-page-table-ops`；`15-pagefault`；`21-vfs-file-mapping` |
| O-24 | 23 | §3.4 VmReply::InfoRegion；§3.5/§3.6#2 VMSF_ONCE；§5.1 引 20 测试行号 | `24-vm-queries`；`22-page-cache`；删除 |
| O-25 | 24 | §2.3 CacheMemory 全回调；§1.4/§3.4 mappedfile ONCE 分流；§3.7 alloc_cycle | `11-memtype`；`21-vfs-file-mapping`；`09`/`22` |
| O-26 | 25 | §1.5/§2.7 map_service；§1.6/§2.8 `adjust_proc_refs`；§3.6/§3.7 dyn_data/fork_region 对比 | `01-vm-birth`；`09`；`23` 内部各取半 |
| O-27 | 26 | §2.2 memstats 位图细节；§3.5 分配器记账；§3.7 transport 决策 | `06-physical-memory`；`09`；`04`/`21` |
| O-28 | 00 | §4.1 分配器 parity/SimPaging 细节 | `25-test-observability`（00 只留一句「可注入」） |
| O-29 | 99 | §1.2/§4 的 Direct Map/分配器示例细节 | `07`/`06`（99 只列常量与权威位置） |
| O-30 | 各篇 | 引用 `draft/`（13 篇 14 处）与 design 快照（如 06 §3.2） | 全部删除；改引 C 源/正式文档/代码 |

### 3.5 非 C 主题逐项回答（固定清单）

| 主题 | 在哪里讲 | 内容要点与锚点 |
|------|---------|---------------|
| 链接与加载 | `01-vm-birth`（主）+ `07-page-tables`（自举页表） | 内核装载 VM ELF（`kernel/main.c:255-276`）；`exec_bootproc`+libexec（`main.c:294-417`）；minix-elf 解析（`os/libs/minix-elf/src/lib.rs`）；`__minix_init` 分界线（`main.c:480`） |
| 镜像与内存布局 | `01-vm-birth`（移交布局）+ `07-page-tables`（DM 窗口/地址宽度）+ `18-vm-mmap`（mmap 区间） | KernelInfo memmap/module list；VM/kernel Direct Map 基址与尺寸（`direct_map.rs:14-31`）；`VM_OWN_HEAPBASE/MMAPTOP`（`main.c:582-583`）；64 位 `MMAP_BASE/TOP`（`mmap.rs`）；`MINSTACKREGION`（`vm.h:39`） |
| 汇编入口与陷阱进入 | `04-ipc-dispatch`（入环约定一句）+ `15-pagefault`（#PF 路径）+ `05-message-contracts`（sys_call/VMCTL wire） | `sys_call` 陷入与 `SYS_VMCTL`（`system/do_vmctl.c:20-167`）；#PF 由 `exception.c:93-125` 组 `VM_PAGEFAULT`；Rust `page_fault.rs`/`trap_stub.rs`；boot 期 `sys_exec` 切用户态（`main.c:408-416`） |
| 启动装配 | `01-vm-birth` | boot_image（`table.c:44-64`）、RTS_VMINHIBIT/BOOTINHIBIT（`kernel/main.c:185-270`）、CALLMAP（`main.c:522-575`）、SEF/RS_INIT（`sef.c:113-134`；`sef_init.c:191-260`）、`map_service`（`main.c:755-768`） |
| 构建与工具链 | `25-test-observability`（feature 矩阵）+ `06-physical-memory`/`09-vm-self-memory`（后端选择） | `os/servers/vm/Cargo.toml` 六 feature；`panic=abort`（`os/Cargo.toml`）；no_std 与 `#[cfg(test)]` 边界；`tools/` 脚本 |
| 跨模块接口与线格式 | `05-message-contracts`（主）+ `01-vm-birth`（内核移交格式）+ `21`/`23`（VFS/RS 对端） | 消息 union 与 56 字节；VM_* 请求码；`minix-types` 专用 struct 与断言；transid；`rprocpub`；`SYS_VMCTL` 字段；`KernelGateway` |
| 错误路径 | `04-ipc-dispatch`（策略与统一 errno 表） | ENOSYS/EPERM/EFAULT/ENOMEM 映射；ACL fail-closed；transport fail-fast；失败计数与审计（`audit.rs`、`VmContext` 计数器） |
| 关闭与退出 | `20-vm-exit`（主）+ `01-vm-birth`（VM 崩溃策略）+ `23-rs-live-update`（旧实例让位） | 两阶段 WILLEXIT/EXIT；`free_proc/clear_proc`；VM 实例与 `num_vm_instances`；LU 槽交换；VM 不可重启（`panic=abort`） |
| 并发与同步 | `00-vm-overview`（模型）+ `04-ipc-dispatch`（信号打断）+ `15-pagefault`（PTE 不变量）+ `07`/`08`（SMP TLB，挂 E-VMTLB） | 单线程事件循环 + `AssumeSyncCell` 边界；SEF 信号打断 `sef_receive_status`；内核 BKL 只在 VMCTL 路径交互；`pt_writemap` 的 VMINHIBIT 包裹（`pagetable.c:799-815`）；C `MF_FLUSH_TLB`（`proc.c:345-347`）与 Rust 缺口 |
| 测试基建 | `25-test-observability`（主）+ 各篇只给「行为—测试」对应一句 | `SimPaging`/`MockGateway`/`TestIpcTransport` 注入；三后端 parity（`allocator_tests.rs`）；feature 三矩阵；QEMU 冒烟（`os/qemu-tests/`）；`os/tests/pm_vm_fork_test.rs`；覆盖率工具链 |

### 3.6 旧文档事实存疑与必须修正项（B 相第一批修正输入）

> 逐条来自对 27 篇正文与 C/Rust 现码的核对；B 相重写时不得原样继承。类型：`F` 事实错、`C` 与代码漂移、`S` 结构/流程自相矛盾、`P` 流程违规。

| # | 类型 | 位置 | 存疑内容 | 证据 |
|---|------|------|---------|------|
| X-001 | P | 13 篇正文 14 处 | 引用 `draft/` 素材（如 05 §7、07 §5.3、08 §7 附录 A） | `rg -c 'draft/'` |
| X-002 | P | 06 §3.2 | 引用 `.design/*` 设计快照作为「三处一致」依据 | 项目规范禁引 |
| X-003 | C | 00 头部 | 「24 个 .c，11,466 行」两处均错（9,260 行；含 .h 才 11,565） | `wc -l` |
| X-004 | C | 00 头部 | 「51 个 .rs」实为 52；「build_callmap 镜像 main.c:543-575」与 01 的 522-573 及实际 537-575 三处不一致 | `find`；`main.c` |
| X-005 | F | 01 §3.1/§3.3 | `KernelIpcTransport` 被称 `unimplemented!`/minix-sys stub；实为委托 trap 后端，仅未初始化返 Err | `transport.rs:24,182-235` |
| X-006 | F | 01 §4.2/§5.1 | `BootParams::placeholder()`/`VM_BOOT_IMAGE` 不存在；入口是 `read_boot_params()`；boot.rs 测试清单含不存在的 `test_boot_params_placeholder_has_vm_slot` | `boot.rs:257`；`lib.rs:100` |
| X-007 | F | 02 §1.2 | endpoint 编码括注写成 `(slot<<8)|generation`；C 是 `(gen<<15)+slot` | `endpoint.h:45`；`endpoint.rs:82` |
| X-008 | S | 02 §4.5 vs 01 §3.3 | 「exec_bootproc/free_mem DEFERRED」与「已落地」矛盾 | `vm_server.rs:593-607` |
| X-009 | F | 04 §2.3/§5.2 | `result = ENOSYS` 标 `main.c:145`（实为 :139） | `main.c` |
| X-010 | F | 04 §5.1 | 声称 13 个 ACL 测试且含 `test_acl_clear`；实际 12 个且无该测试（clear 折进 `VmProc::clear`） | `acl.rs` |
| X-011 | F | 04 §2.4/§3.7 | 「多个系统进程共享同一槽位」与实现（首个空闲槽独占）不符 | `acl.c:77-97` |
| X-012 | F | 05 §2.3 | 「`lastscan` 初始化为 `mem_high`」；C 是 `static int lastscan = -1` | `alloc.c:410` |
| X-013 | S | 05 §3.8 S-5/§4.5、06 §3.3/§4.3、10 §1.7/§4.4 | `missing_spares`/`mark_alloc_failure`/`alloc_cycle`/压力计数叙述过期（生产链已删，仅注释） | `vm_server.rs:983-984,1079` |
| X-014 | F | 06 §5.1 | 行号与函数名系统性错配（`alloc_pages`→`memstats_internal` 等，详见 §3.6 组表） | C 源行号复检 |
| X-015 | F | 07 §3.2/§4.2 | `DirectMapArch` trait 样例漏 `VM_DIRECT_MAP_SIZE`；§3.3 把 `VM_SELF_PT_STORAGE` 类型写错 | `direct_map.rs:45`；`vm_self_map.rs:144` |
| X-016 | F | 07/08 §4.3/§4.4 | 「测试构建用 `MockPaging`/零 stub」；实为 `sim::SimPaging` 与 `PageTable::new()` | `pagetable/mod.rs:27-29`；`vmproc_handle.rs:349-360` |
| X-017 | F | 08 §3.5 vs §5.1 | 「`clone_range` 0 测试」与「按新增 4 测试计入」自相矛盾；实际已有 4 测试 | `arch/paging.rs:1280-1344` |
| X-018 | F | 08 §2.10 | 把 `main.c:211` 标为「VM 自身初始化」；实为 LU 回滚 `pt_bind` | `main.c:211` |
| X-019 | S | 09 §4.1/§5.1/§5.3 | `HeapArena::shrink` 被当已实现并配测试；实际 DEFERRED，无函数无测试 | `heap_arena.rs:48` |
| X-020 | F | 09 §2.1 | `sizeof(sdh)=112/120`、498/497 对象/页按 64 位布局计算，与 32 位目标（`phys_bytes=unsigned long`）不符 | `type.h:20`；`slaballoc.c:81-109` |
| X-021 | S | 10 §1.7/§4.4 | `VmServer::mark_alloc_failure`/`alloc_pressure` 全仓 0 命中 | `rg` |
| X-022 | F | 10 §5.3 | 「Buddy 阈值无单测」；实际 3 个 `choose_allocator_type` 测试 | `vm_server.rs:1805-1829` |
| X-023 | S | 11 §3.2/§4.2/§5.3、12 §4.6、13 §3.6 | PageSlot `Reserved`/`map_lazy` 三态叙述过期（现两态，V12-P2-4 删除） | `page_state.rs:123-132`；`vir_region.rs:578` |
| X-024 | F | 12 §1.5/§2.4 | `dp_pagefault` 名称错误（实为 `phys_pagefault`）；「pt_writemap 映射」与实现（仅赋 phys）不符 | `mem_directphys.c:50-61` |
| X-025 | S | 12 §1.9 vs §3.1 | 代码块给 `ev_pagefault` 默认实现，与「必须实现」及实码（无默认体）矛盾 | `memtype.rs:66-76` |
| X-026 | F | 12 §5.3 | contig「未完成面」过期；`alloc_contiguous` 与 `ev_new` 已落地 | `page_state.rs:33`；`memtype.rs:776-790` |
| X-027 | F | 14 §1.6 | `AVL_GREATER_EQUAL` 映射错误（写成严格大于）且与 `AVL_GREATER` 重复；§4.3 声称与 C 分配结果相同不成立（C 高→低扫，Rust 低→高扫） | `region_map.rs:167-178`；`region.c:277-287` |
| X-028 | S | 15 §1.5 vs §3.4/§4.2 | 伪造 fault 源检查「显式 if」与「debug_assert」两说；实际为 `if !... { debug_assert!(false); audit_log! }` | `vm_server.rs:1340-1345` |
| X-029 | S | 15 §2.6/§3.4 | 「无 SEF 框架」过期；V14-P2-1 已用 `minix_sef` 接收分类 | `vm_server.rs:1123-1139` |
| X-030 | S | 15 §4.1/§5.1 | 主循环代码含已删 `missing_spares` 钩子与不存在的测试 | `vm_server.rs:1079` |
| X-031 | S | 16 §3.6/§4.1/§5.3 | 同一机制三处口径矛盾（major/minor、SIGSEGV/clear、do_memory DEFERRED）；实况：SSIGSEGV 仅违规路径接线，恢复路径缺 | `vm_server.rs:1527-1543,1570-1574` |
| X-032 | S | 16 §3.6#10/§4.4 | `enqueue_fdio`/`mappedfile_pf_cont`「无消费者」过期（已入队并注册回调） | `cow_exec_pf.rs:111,159` |
| X-033 | F | 17 §3.6#4/§5.3 | 写保护判据「`is_writable() && refcount==1`」过期（V12-P1-2 改 `is_page_writable()`） | `vmproc_handle.rs:546-550` |
| X-034 | S | 17 §1.3 | 「refcount==1 是可写的充要条件」漏 `remaps>0` 例外 | `mem_anon.c:105-111`；`memtype.rs:266` |
| X-035 | S | 18 §3.5/§3.6#5/#6/§4.4 | sys_fork stub / handle_memory_once DEFERRED 过期（已实现含 msgaddr 预填充） | `fork.rs:344-390` |
| X-036 | S | 18 §4.1 | 分发形态描述过期（match 分支 → 编译期 CALLMAP 表查） | `dispatcher.rs:639-648,723` |
| X-037 | F | 19 §4.3 | 「split 失败重插原区域不丢元数据」与 `VirRegion::new` 重建丢失 memtype/param 不符 | `brk.rs:173-177` |
| X-038 | S | 19 §3.2 | `find_mut_by_end` 锚点/描述错（实按 `contains_addr` 命中） | `region_map.rs:81` |
| X-039 | S | 20/21 §2.6/§1.4 | 「MEM 仅自身」是注释意图，C/Rust 实现均无 target 校验 | `mmap.c:294-297`；`map_phys.rs:110` |
| X-040 | S | 22 §3.2/§5.3 | fdref「本地暂存/未接线 VFS」过期（已入 `VfsRequestQueue`） | `exit.rs:177-193`；`region/mod.rs:68-88` |
| X-041 | S | 23 头部/§4.1 | `page_cache.rs` 锚点错且 `CacheKey` 已删（与 24 §3.1 单键模型矛盾） | `page_cache.rs:42,201` |
| X-042 | S | 23 §3.6#1 | 「activate 不发 IPC/IpcSender 移除」过期；`take_pending_vfs_call` 与 `IpcTransport::send` 已实现 | `vfs_queue.rs:162`；`transport.rs:119-126` |
| X-043 | F | 24 §4.1 | `MessVmmcp` 仍写 i386 布局；现为 x86_64 加宽（E-VMMCPWIRE） | `message.rs:2644,2689` |
| X-044 | S | 24 §3.4/§4.2 | handler 位置过期（已迁 `ipc/cache_handlers.rs`）；`handle_*cache` wrapper 已不存在 | `cache_handlers.rs:55,176` |
| X-045 | S | 25 §3.2 vs §3.7/§4.4 | A-8「UPDATE 切换 DEFERRED」与「七步全链已落地」不能同真；实况支持后者（内核流量挂 E2） | `rs.rs`；`edge_todo.md E2` |
| X-046 | F | 25 §3.4/§3.9 | `RsUpdateResult{Suspend}`、`[RprocEntry;32]`、`is_user` 恒 false 均过期 | `rs.rs:89-97`；`vm_server.rs:1749` |
| X-047 | S | 25 §3.5/§4.8 | SET_PRIV mask 恒 None 过期（位图传输已实现） | `dispatcher.rs:805-844` |
| X-048 | S | 26 §3.3 vs §4.8 | `what=1/2/3` 已修 vs `what=0/1/2` 仍写，文内矛盾 | `vm_server.rs:1882` |
| X-049 | F | 99 §2.2 | `PAF_ALIGN16K` 写 0x20；C 与 Rust 均为 0x40 | `vm.h:27`；`types.rs:92` |
| X-050 | F | 99 §1.1 | 「addcache 以 `NO_DEV` 拒绝」；C 是 `assert(dev != NO_DEV)` | `cache.c:231` |
| X-051 | C | 全目录 | Rust 行号锚点系统性漂移（「（L###，工具生成）」大量指向邻接函数/旧行，例：15 全篇、16 全篇、17 §4.x、22 §3.4、26 §3-§4） | 逐篇复检见 §8.2；B 相一律改用「文件 + 符号名」，行号只作为辅助 |
| X-052 | C | 全目录 §5.4 | 测试统计互斥（441/448/360/361/434/437/503…），无统一快照基准 | 删除，改由 25 单源 |

---

## 4. 新目录

### 4.1 设计原则

1. **一个语义单元一篇**：概念教学、机制协议、服务编排、工程基建各自成篇；同一机制只有一个展开位置（主讲述点）。
2. **无前向引用**：每篇的「前置」只指向更早编号；`00-vm-overview` 负责在第一篇就给出全部顶层术语的一句话定义与全阶段地图，后续篇章出现新术语时必须在本篇内讲完。
3. **执行与因果序优先**：启动段严格按运行时序（01）；循环段以「一次请求的生命周期」为主线（04 → 各服务篇），不再按 C 文件位置排列。
4. **C 源是事实底线，Rust 现状是重写现场**：每篇既讲 C 语义，也讲 Rust 落点与 ARCH 差异；行号锚点让位于符号锚点；禁止引用 `draft/`、`.design/`、`tmp_design_and_todo/`、其它 AI 产出。
5. **对照只在决策处**：Redox/Linux 对照不再逐篇设节，只在某设计决策需要外部依据时保留一小段，并标注「外部依据」而非 ground truth。
6. **测试与统计集中**：机制篇只保留「行为—测试函数」一句索引，测试清单、feature 矩阵、统计口径统一归 `25-test-observability`。

### 4.2 新篇章总表

| 新编号 | 标题 | 一句话定位 | 分组 | 主要旧来源（素材，非搬运对象） |
|--------|------|-----------|------|-------------------------------|
| 00 | vm-overview：内存语义权威与阅读地图 | VM 是什么、为什么存在、三重权威模型、执行模型与结构不变量、全阶段地图 | 导读 | 00（重写） |
| 01 | vm-birth：从内核移交到进入主循环 | 内核如何把 VM 带起来，VM 如何自建页表、装载其余 boot 服务、完成 RS 握手并进入事件循环 | 诞生与入口 | 01 + 06/07/10 的启动段 + SEF 库 |
| 02 | vmproc-identity：VM 眼里的进程 | 槽位表、endpoint 身份、VMF 状态与生命周期；一次身份验证的完整语义 | 诞生与入口 | 02 + 03 |
| 03 | acl-gate：调用门禁 | VM 如何按调用号位图对调用者分级授权；默认策略与 fail-closed | 诞生与入口 | 04 |
| 04 | ipc-dispatch：主循环与五优先级分发 | 一次请求从收消息到回复/挂起的完整生命周期与错误策略 | 诞生与入口 | 15（+16 入口片段、04 §4.4） |
| 05 | message-contracts：消息契约与内核调用通道 | VM 对外全部线格式：IPC union、专用 wire struct、SYS_VMCTL、KernelGateway | 诞生与入口 | 15 §2.1、16 §3.4、19/20/21/22/23 的 wire 段、99 §2.1 |
| 06 | physical-memory：物理内存账本 | memmap 到分配器：click、位图/伙伴/段树、约束、记账与调试检测 | 内存地基 | 05 |
| 07 | page-tables：页表、Direct Map 与 VM 自身页表 | 地址翻译的数据结构，VM 访问它的双视图难题与 ARCH A-1/A-9 解法 | 内存地基 | 07 + 06 的结构段 |
| 08 | page-table-ops：页表操作与 TLB 纪律 | 建/绑/写/拆/复制/查询页表的操作面，与写后失效绑定 | 内存地基 | 08 + 06 的操作段 |
| 09 | vm-self-memory：VM 自己的内存 | 自用页分配、备用页池自举与消除、堆（slab→HeapArena）、稳态化搬迁与 LU 原语 | 内存地基 | 06 + 09 + 10 |
| 10 | physical-page-state：物理页状态与引用计数 | 从 phys_block/phys_region 到 PageFrames：页的共享、计数与生命周期 | 地址空间账本 | 11 |
| 11 | memtype：内存类型系统 | 六类内存策略与回调契约：匿名/直物/共享/连续/缓存/文件 | 地址空间账本 | 12 |
| 12 | region-ledger：区域账本与有序索引 | vir_region 模型、地址解析、AVL→BTreeMap 的空槽与查找语义 | 地址空间账本 | 13 结构段 + 14 |
| 13 | region-ops：区域操作族与释放漏斗 | 建/填/复制/扩/缩/拆/释放的全部框架操作与能力门控 | 地址空间账本 | 13 操作段 + 21/22 的 region 段 |
| 14 | cow：写时复制 | 共享如何建立、写保护如何设置、首次写如何分裂、memtype 如何换型 | 运行机制 | 17 + 16 §3.3 + 18 §2.4 |
| 15 | pagefault：缺页处理与异步恢复 | 被动 #PF 与主动 SIGKMEM 两条路径、异步状态机、SIGSEGV 与 PTE 写不变量 | 运行机制 | 16 |
| 16 | vm-fork：fork 编排 | VM 如何把「复制地址空间」组织成可回滚的服务调用 | 服务：进程生命周期 | 18 |
| 17 | vm-brk：堆的生长与收缩 | brk 三态编排、区域扩展与真正释放收缩 | 服务：进程生命周期 | 19 |
| 18 | vm-mmap：映射建立 | 地址分配 + 来源绑定：匿名、文件、物理、remap 四条路径 | 服务：进程生命周期 | 20 |
| 19 | vm-munmap：映射拆除与物理映射 | 四个拆除入口的统一处理与物理映射的权限面 | 服务：进程生命周期 | 21 |
| 20 | vm-exit：两阶段退出与进程控制 | WILLEXIT/EXIT 的资源清算与 PROCCTL 的 CLEAR/HANDLEMEM | 服务：进程生命周期 | 22 |
| 21 | vfs-file-mapping：VM 与 VFS 的异步对话 | FDLOOKUP/FDIO/FDCLOSE 三条链路、串行队列与 fdref | 跨服务协作 | 23 |
| 22 | page-cache：页缓存 | 磁盘块缓存目录、LRU 淘汰与四个缓存 IPC | 跨服务协作 | 24 |
| 23 | rs-live-update：RS 热更新服务面 | SET_PRIV/PREPARE/UPDATE/MEMCTL 与 VM 实例化 | 跨服务协作 | 25 |
| 24 | vm-queries：查询服务 | 只读窗口：INFO/GETPHYS/GETREF/GETRUSAGE | 跨服务协作 | 26 |
| 25 | vm-test-observability：测试基建与可观测性 | 注入缝、parity 矩阵、观测面与统计口径 | 工程附录 | 各篇 §5 + §4.1 |
| 99 | global-concepts：常量、旗标与全局状态 | 魔数的权威定义、消费点与单源纪律 | 参考附录 | 99 |

### 4.3 阅读路径

- **主线（顺序阅读）**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15 → 16 → 17 → 18 → 19 → 20。
- **服务支线（按需）**：只关心某个服务时，从 04 进入，前置读 10/11/12/13/14/15，再读 16-20 中的目标篇。
- **跨服务支线**：21/22/23/24 依赖 04/10/11/13/15（21 另需 18）。
- **附录（随时查）**：99（常量/旗标/调用号）；25（测试与观测）。
- **可跳读**：09（自举细节，理解稳态后可略）、22（页缓存，只做查询/服务可不读）、23（仅热更新相关）。

### 4.4 并行主题的分组与代表成员

- **六个 memtype**：`11-memtype` 给统一回调契约与能力矩阵；匿名内存为代表成员讲透；其余按差异表收束；文件/缓存型指向 21/22 深讲。
- **二十余个 IPC 服务 handler**：`04-ipc-dispatch` 给统一生命周期；16-20 按场景分组；21-24 为跨服务组。
- **页表操作十四函数**：`08-page-table-ops` 按「生命周期 / 写映射 / 按需分配 / 复制 / 查询 / 内核协作」六组收束，`pt_writemap` 为精讲代表。
- **物理分配三后端**：`06-physical-memory` 以 bitmap 为代表（与 C 最接近），buddy/segment-tree 按差异表收束。
- **SUSPEND 的多个用例**：协议本体在 04；各服务篇只写「何时挂起、谁恢复」。

### 4.5 每篇内部结构模板（B 相写作规范）

机制类篇（06-15）：

1. **概念**：要解决的问题、为什么需要、本篇边界（讲什么/不讲什么）。
2. **机制真序**：运行时序或数据流，逐条带 C 锚点（符号名优先，行号辅助）；必要时配关系图。
3. **设计决策**：Rust 如何建模；与 C 的差异逐条标注 `[ARCH: ...]`（三处一致：C 对照点 + 本篇 + 代码注释）；外部对照只在决策处。
4. **不变量与失败路径**：必须成立的条件、违反后果、错误码。
5. **验证**：行为—测试函数索引（一行一条）+ 指向 25。
6. **过渡**：在启动/循环中的位置；下一篇入口。

服务类篇（16-20）：以「一次请求的生命周期」为骨架——入口与校验 → 编排步骤 → 失败与回滚 → 回复语义 → 与相邻服务/机制的边界；C 与 Rust 同序对照。

入口类篇（00-05）：以读者问题为骨架，不用「C 源码分析 / Rust 设计决策」这样的实现手册分章；C 与 Rust 出现在论证中而不是分章里。

### 4.6 序差表（教学序 vs 运行时序）

| # | 运行时序事实（锚点） | 教学序选择 | 理由 | 回指补偿 |
|---|---------------------|-----------|------|---------|
| Q-01 | `init_vm` 顺序：vmproc→ACL→region→mem→init_proc→pt_init（`main.c:458-475`） | 02/03（进程/ACL）→ 04/05（接口）→ 06-09（内存地基）→ 10-13（账本） | 读者要先知道「谁在调用」，再理解「调用需要哪些地基」；region 语义依赖页表与 memtype，不能先讲 | 01 §时序表保留真实顺序；04 开头声明「本篇讲循环，数据机制在 06 之后」 |
| Q-02 | 主循环 P3 缺页先于普通调用（`main.c:153-165`） | 14 Cow → 15 缺页 → 16-20 服务 | Cow 是缺页与 fork/mmap 的共同底层；先讲底层协议，再讲触发它的状态机与服务 | 04 §五优先级给出运行时顺序；15 开头说明「P3 优先级最高但依赖 13/14」 |
| Q-03 | 触发链是「缺页 → CoW 分裂」 | 先 14（共享/写保护/分裂）后 15（缺页状态机） | 15 的 memtype 决策表需要 Cow 的 writable 语义；14 不含状态机，无前向依赖 | 14 末尾一句「触发入口在 15」 |
| Q-04 | RS_INIT 在主循环 P2 处理（`main.c:149-152`） | 握手协议放 01（诞生），handler 细节放 23 | 握手是启动链最后一环，必须与 01 连读；23 只讲服务语义 | 01 给协议，23 给 UPDATE/PREPARE 语义并回引 01 |
| Q-05 | `do_memory` 由信号打断触发（`main.c:731-750`），不经过消息队列 | 15 与被动路径合并讲；04 只讲信号打断机制 | 主动/被动共用 `map_pf`，合讲一次最省重复 | 04 §并发段给「信号打断」模型 |
| Q-06 | fork 在运行时使用 CoW 原语（`fork.c:70-80`） | 14（原语）→ 15（缺页）→ 16（fork 编排） | fork 编排需要理解原语与缺页后果 | 14 声明「主要调用者是 fork，见 16」 |
| Q-07 | VM 自身页表在 `pt_init` 内自举建立（`pagetable.c:1305-1356`） | 07 结构 + 08 操作 + 09 自举内存，01 只留时序 | 自举细节对首次读者过载；先建立机制模型再看自举的例外处理 | 01 的时序表标注「机制见 07/08/09」 |
| Q-08 | `map_phys` 在 C 中与 `do_munmap` 同文件、Rust 中独立模块 | 18 只讲建立路径，19 讲拆除与物理映射 | 按读者场景（建 vs 拆）分组，不按 C 文件分组 | 18/19 开头互指 |
| Q-09 | VFS 回复 `do_vfs_reply` 在主循环 CALLMAP 分发（`main.c:551`） | 21 独立成篇于服务之后 | 异步对话是跨服务协议，需要先懂 VFS 队列与 memtype | 04 §CALLMAP 标注回复入口，21 展开 |
| Q-10 | 页缓存四 IPC 在 CALLMAP 中（`main.c:569-572`） | 22 独立成篇 | 缓存是独立子系统，且被 21 消费 | 21 引用 22 |

---

## 5. 每篇契约

> 每篇契约七要素齐全：定位、讲什么、不讲什么（含去向）、前置（只指更早编号）、后置、事实底线、知识点清单 + 验收标准。契约里出现的旧编号只用于素材定位，不构成引用。B 相对事实底线的核对以 C 源与 Rust 现码为准；旧文档文本只作线索。

### 00-vm-overview

- 一句话定位：让第一次接触 VM 的读者建立「VM 是什么、为什么必须有它、它的三条职责和一条执行模型」的整体心智模型，并知道后面每一篇解决哪个问题。
- 讲什么：K-001~K-005：内存语义权威的三重角色；内核/VM 分工；单线程事件循环与借用审计；「只改不在运行进程的页表」结构不变量；权威模型的数据结构索引。另讲：全阶段阅读地图（§4.2 总表）、主/支/附录路径、术语一句话定义表（region/memtype/CoW/SUSPEND/transid/endpoint）。
- 不讲什么：任何机制细节（页表、分配器、缺页、各服务）→ 交给 06-24；实施现状、测试矩阵、覆盖率数字 → 25；常量权威值 → 99。
- 前置：无。
- 后置：全阶段所有篇章。
- 事实底线：`minix3/minix/servers/vm/main.c:93-194`（事件循环）、`kernel/arch/i386/exception.c:93-125`（#PF 转发）、`kernel/system/do_vmctl.c`（VMCTL 通道）；`os/servers/vm/src/vm_server.rs`（`VmContext`/`run_once`）；`01-stage-kernel/00-kernel-overview.md`（前序概念边界）。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-001 | 三重权威角色 | `fork.c`/`exit.c`/`pagefaults.c` | 回答「VM 管什么」 | 旧 00 §1.1 |
| K-002 | 内核/VM 分工协议 | `exception.c:93-125`；`do_vmctl.c` | 界定 VM 的能力边界 | 旧 00 §1.1 |
| K-003 | 单线程事件循环模型 | `vm_server.rs` VmContext | 所有并发讨论的前提 | 旧 00 §1.2 |
| K-004 | TLB 结构不变量 | `pagefaults.c:153`；`16 §3.7` | 所有 PTE 写安全的根 | 旧 00/16 |
| K-005 | 权威模型与阅读地图 | `glo.h`；`region.h`；`page_state.rs` | 导航 | 旧 00 §1.2/§2 |

- 验收标准：读者读完能回答——VM 与内核各做什么、为什么 VM 崩溃等于系统冻结、为什么单线程反而安全、后面每篇属于哪一层；文中不出现未定义术语；不复制现行文档的「实施现状/测试基线」表。

### 01-vm-birth

- 一句话定位：讲清 VM 从内核把控制权交来，到它能为别的服务建好页表、完成 RS 握手、进入事件循环的全过程。
- 讲什么：K-006~K-020：boot_image/RTS 抑制协议；`is_first_time`；`init_vm` 全时序；KernelInfo/BootParams 移交数据；`get_mem_chunks`；`__minix_init` 分界线；`exec_bootproc` 与 minix-elf/libexec；blob 释放与总量校准；SEF 生命周期与异步回复；RS_INIT 握手与 `map_service`；SIGKMEM 信号路径；LU/restart 启动路径；VM 自身 libc 边界；崩溃策略。
- 不讲什么：页表机制（07/08）、物理分配器内部（06）、区域与 memtype（10-13）、缺页状态机（15）、RS 服务语义（23）；SEF 的 C 库实现只讲 VM 用到的分支，SEF 库其它功能不展开。
- 前置：00。
- 后置：02（进程表初始化）、09（自举内存）、16/23（LU 与 RS）、25（启动测试）。
- 事实底线：`main.c:79-107,262-286,294-417,428-587,592-768`；`utility.c:44-79`；`alloc.c:281-284,306`；`libsys/sef.c:100-260`；`libsys/sef_init.c:191-260`；`kernel/main.c:185-276`；`kernel/table.c:44-64`；Rust `boot.rs`（`read_boot_params`/`BootParams`）、`main.rs`、`vm_server.rs`（`init`/`rs_handshake`/`exec_bootproc`）、`os/libs/minix-elf/src/lib.rs`、`os/libs/minix-sef/src/lib.rs`、`os/kernel/src/vm.rs`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-006 | 内核移交与 boot_image/RTS | `kernel/main.c:185-270`；`table.c:44-64` | VM 的诞生条件 | 存（00/01） |
| K-007 | `is_first_time` 门控 | `main.c:79-88,100-107` | 启动第一分支 | 存 |
| K-008 | `init_vm` 十五步 | `main.c:428-587` | 本篇骨架 | 存 |
| K-009 | KernelInfo/BootParams | `main.c:442-495`；`boot.rs` | 移交数据契约 | 存 |
| K-010 | `get_mem_chunks` | `utility.c:44-79` | 启动第一步输入 | 存（05 移入） |
| K-011 | `__minix_init` 分界线 | `main.c:480` | 堆可用时刻 | 存（01/09） |
| K-012 | `exec_bootproc` | `main.c:294-417` | 其余服务如何起 | 存 |
| K-013 | libexec/minix-elf 与 `vm_exec_info` | `main.c:288-329`；`minix-elf` | 装载协议 | 存+新 |
| K-014 | blob 释放与总量校准 | `main.c:485-520`；`alloc.c:281` | 物理账本闭环 | 存 |
| K-015 | SEF 生命周期与异步回复 | `sef.c:100-134`；`sef_init.c` | 握手底座 | 存 |
| K-016 | RS_INIT 与 `map_service` | `main.c:149-152,241-260,755-768` | ACL 来源 | 存 |
| K-017 | SIGKMEM 信号路径 | `main.c:731-750` | 主动缺页入口 | 存 |
| K-018 | LU/restart 启动路径 | `main.c:196-217,592-726`；`utility.c:188-335` | 热更新起点 | 存 |
| K-019 | VM libc 边界与 GlobalAlloc | `utility.c:361-420`；`global.rs` | ARCH 差异 | 存 |
| K-020 | 崩溃策略 | `os/Cargo.toml`；`vm_server.rs` | 失败语义 | 存+新 |

- 验收标准：能画出并解释从 boot_image 到「进入 while(TRUE)」的完整时序（含 S1-S16）；能解释 VM fresh 为什么不能像别的服务那样在启动期阻塞等 RS_INIT；能指出 `__minix_init` 前后各有哪些操作不可用；所有行号锚点核对通过（抽查十条）。

### 02-vmproc-identity

- 一句话定位：讲 VM 如何用一张固定槽位表记住每个进程，如何从 endpoint 判定「来者是谁」，以及一个槽位从激活到回收的完整生命周期。
- 讲什么：K-021~K-028：vmproc 字段与五组职责；`vm_slot`+`vm_endpoint` 双身份与 generation 防复用；VMF 正交状态位与状态机；`VMP_NR` 表、slot 分配/查找/遍历、`VMP_EXECTMP`；`vm_isokendpt` 三层验证；`init_proc`/`clear_proc`/`free_proc`；`swap_proc_slot`；Rust typestate 与静态表。
- 不讲什么：ACL 权限语义（03）；`vm_total`/缺页计数等字段的消费细节（10/13/15）；fork 全流程（16）；LU 服务流程（23）。
- 前置：00、01。
- 后置：03、04、16、20、23。
- 事实底线：`vmproc.h:14-37`；`glo.h:17-20`；`main.c:262-286,457-462,577-579`；`exit.c:25-58`；`utility.c:84-101,188-216`；`fork.c:47-52,59-88`；`endpoint.h:45-69`；Rust `vmproc/{vmproc.rs,flags.rs,table.rs,vmproc_handle.rs}`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-021 | vmproc 字段全集 | `vmproc.h:14-32` | 个体模型 | 存（02） |
| K-022 | 双身份与 generation | `vmproc.h:22-23`；`endpoint.h` | 寻址与校验 | 存（02/03） |
| K-023 | VMF 状态位与状态机 | `vmproc.h:35-37`；`exit.c:60-113` | 生命周期 | 存（02） |
| K-024 | 进程表集合语义 | `glo.h:17-20`；`main.c:457-462` | 表的集合层 | 存（03） |
| K-025 | `vm_isokendpt` | `utility.c:84-101` | 门卫 | 存（03） |
| K-026 | `init_proc`/回收 | `main.c:262-286`；`exit.c:33-58` | 槽位生灭 | 存（02） |
| K-027 | `swap_proc_slot` | `utility.c:188-216` | LU 交换 | 存（03/10） |
| K-028 | Rust typestate/静态表 | `table.rs`；`vmproc_handle.rs` | Rust 建模 | 存（02/03） |

- 验收标准：能解释为什么 `vm_endpoint` 与 `vm_slot` 必须并存、generation 如何阻止旧 endpoint 命中新进程；能列出状态机的合法迁移并说明「退出中」槽位为何不能被新服务调用；H2 typestate 方法名与实现一致。

### 03-acl-gate

- 一句话定位：讲清 VM 用什么数据表达「谁可以调用哪个服务」，以及这套权限从启动授权到进程退出释放的完整流转。
- 讲什么：K-029~K-033：ACL 存在理由与四道防线位置；调用号位图与默认集；DEFAULT/SYSTEM/NO_ACL 分层；`acl_init/set/check/fork/clear` 与位图宏（`BITS_FULL`）；A-11 fail-closed 偏移。
- 不讲什么：ACL 在分发闸的接线细节（04 一句）；fork 编排（16）；RS_SET_PRIV 的服务语义（23）；ACL 槽位表在 Rust 中为何消失（03 本篇内一段说明即可，不展开 02）。
- 前置：00、02。
- 后置：04、16、23。
- 事实底线：`acl.c:10-129`；`vmproc.h:23`；`com.h:769-780`；`bitmap.h`；`sys_config.h`；Rust `acl.rs`（`AclMask`/`AclState`）、`vmproc/vmproc.rs`（`vm_acl` 字段）、`vm_server.rs` 分发闸。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-029 | ACL 位置与理由 | `main.c:165-173` | 动机 | 存（04） |
| K-030 | 调用号位图/默认集 | `com.h:769-780`；`bitmap.h` | 权限载体 | 存（04） |
| K-031 | 分层与未初始化 | `acl.c:37-61` | 默认策略 | 存（04） |
| K-032 | ACL 生命周期 | `acl.c:21-129` | 权限流转 | 存（04） |
| K-033 | A-11 fail-closed | `acl.rs` | ARCH 偏差 | 存（04） |

- 验收标准：能说出 `VM_PAGEFAULT` 为何不在位图里；能复述 `acl_check` 的三层判定并指出 C 与 Rust 在「未初始化」上的行为差异及理由；`PAF/WMF` 之外的位图宏（`BITS_FULL`）在本篇有名字与用途。

### 04-ipc-dispatch

- 一句话定位：把「一次请求的一生」讲完整：收消息、判优先级、验身份、过门禁、调 handler、回复或挂起。
- 讲什么：K-034~K-040：主循环骨架与五优先级；SUSPEND 协议；transid 与路由；通知过滤与伪造 fault 防御；CALLMAP 与 ENOSYS；调用者验证模式；错误策略（errno 统一表/fail-fast/fail-closed/审计计数）。另讲：SEF 信号打断语义（与 01/15 的分工）与单线程并发模型在循环中的体现。
- 不讲什么：各 handler 实现（16-24）；SEF 库完整生命周期（01）；ACL 数据语义（03）；消息线格式细节（05）；测试清单（25）。
- 前置：00、02、03。
- 后置：05、15、16-24 全部服务篇。
- 事实底线：`main.c:112-192,522-575`；`com.h:627-780,1151`；`vfsif.h:79-81`；`ipcconst.h`；Rust `vm_server.rs`（`run`/`run_once`/`dispatch_on_msg`/`handle_vfs_transid`）、`ipc/dispatcher.rs`、`ipc/transport.rs`、`ipc/encode.rs`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-034 | 五优先级主循环 | `main.c:112-192` | 本篇主干 | 存（15） |
| K-035 | SUSPEND 协议 | `main.c:178-191` | 生命周期关键分支 | 存（15/16/…) |
| K-036 | transid 路由 | `vfsif.h:79-81`；`main.c:143-148` | 优先级 P1 | 存（15/22） |
| K-037 | 通知过滤/伪造防御 | `main.c:125-129,153-157` | 入口安全 | 存（15/16） |
| K-038 | CALLMAP 与 ENOSYS | `main.c:522-575` | 分发表 | 存（15） |
| K-039 | 调用者验证模式 | `main.c:131-132`；各 handler | 统一防御习惯 | 存（03/15） |
| K-040 | 错误策略与审计 | `dispatcher.rs`；`audit.rs` | 失败语义 | 存+新 |

- 验收标准：给定任意一条 VM 请求消息，读者能说出它在五优先级中的位置、谁会验它、被拒时回什么、什么时候 SUSPEND；能解释「VM_PAGEFAULT 不回复」与「SUSPEND 不回复」的区别；统一 errno 表覆盖 16-24 用到的全部错误码。

### 05-message-contracts

- 一句话定位：VM 对外的全部数据契约：IPC 消息线格式、内核调用格式、以及两者历史上踩过的偏移陷阱。
- 讲什么：K-041~K-049：请求码族与 49 号上界；56 字节消息与 union/m1..m10；专用 wire struct 纪律（含 M1 overlay 教训与 size/offset 断言）；回复编码两段式；SYS_VMCTL 全表与 KernelGateway seam；BootParams/KernelInfo 格式；VFS mess_10 与 transid；RS rprocpub；单源常量与审计纪律。另讲 `AM_AUTO` 等异步消息约定。
- 不讲什么：请求的语义与编排（16-24）；调度/循环（04）；boot 的时序（01）；minix-types 之外的 crate 内部设计。
- 前置：04。
- 后置：15、16-24（凡涉及消息的篇章）。
- 事实底线：`minix3/minix/include/minix/{com.h,ipc.h,vfsif.h,rs.h,ipcconst.h}`；`kernel/system/do_vmctl.c:20-167`；`os/kernel/src/vm.rs:454-870`；`os/libs/minix-types/src/ipc/{vm.rs,message.rs,rprocpub.rs,rs_start.rs}`；`os/servers/vm/src/kernel_gateway.rs`；`os/libs/minix-sys/src/{ipc.rs,syscall.rs}`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-041 | 请求码族/49 上界 | `com.h:627-780` | 契约索引 | 存（15/99） |
| K-042 | 56B 消息与 union | `ipc.h`；`message.rs` | 线格式基座 | 存（15） |
| K-043 | 专用 wire struct 纪律 | `vm.rs` 各 struct | 纠正历史错误 | 存（19-23 汇总） |
| K-044 | 回复编码两段式 | `encode.rs` | 回复契约 | 存（15/各服务） |
| K-045 | SYS_VMCTL/KernelGateway | `do_vmctl.c`；`kernel/vm.rs`；`kernel_gateway.rs` | 内核通道 | 存+新 |
| K-046 | BootParams/KernelInfo | `boot.rs`；`kernel/main.c` | 启动契约 | 存（01） |
| K-047 | VFS mess_10 wire | `ipc.h:85-91`；`vfsif.h` | 跨服务 wire | 存（23） |
| K-048 | RS 握手 wire | `rs.h:165-199`；`rprocpub.rs` | 跨服务 wire | 存（25） |
| K-049 | 线格式审计纪律 | `minix-types` 断言；callmap 测试 | 防漂移 | 存+新 |

- 验收标准：能对照 C `ipc.h` 与 `minix-types` 说出每个 VM 消息的字段偏移；能解释「为什么禁止从 `m1` overlay 解码多字段消息」并举出至少两个历史案例；`SYS_VMCTL` 每个参数的请求/回复字段与 `do_vmctl.c` 一致。

### 06-physical-memory

- 一句话定位：VM 的物理内存账本是怎么建起来、怎么分配、怎么记账、怎么防双分配的。
- 讲什么：K-050~K-061：内存来源链与 click；三种分配器形态与选择；PAF 约束四维；`mem_init`/`alloc_mem`/`findbit`/`free_mem`；统计与 `usedpages` 检测；C 保留队列数据结构（机制消费归 09）；Rust BumpBuf/预映射元数据。
- 不讲什么：自用页分配与备用页池消费（09）；页表页（07/08）；缓存回收（22）；`missing_spares` 主循环（09）；buddy/segment-tree 的完整算法推导（只给差异表与选用建议）。
- 前置：00、05（消息契约中 BootParams 一段）。
- 后置：07、09、16、18。
- 事实底线：`alloc.c` 全（548 行）；`utility.c:44-79`；`minix3/minix/include/minix/{type.h,param.h,const.h}`；`vm.h:22-62`；Rust `phys_mem/{mod,types,alloc_trait,bitmap_alloc,buddy_alloc,segment_tree_alloc,stats}.rs`、`boot.rs`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-050 | 内存来源链 | `utility.c:44-79` | 输入 | 存（05） |
| K-051 | Click 单位 | `const.h:84-101` | 单位制 | 存（05） |
| K-052 | 分配问题与三表达 | `alloc.c:33-51`；`phys_mem/*` | 分配器形态 | 存（05/06） |
| K-053 | PAF 约束四维 | `vm.h:22-27` | 接口 | 存（05） |
| K-054 | `mem_init` | `alloc.c:306-335` | 接管 | 存（05） |
| K-055 | 分配路径 | `alloc.c:242-279,369-460` | 核心机制 | 存（05） |
| K-056 | 释放路径 | `alloc.c:289-301,465-481` | 核心机制 | 存（05） |
| K-057 | 三后端与选择 | `phys_mem/*`；`vm_server.rs` | A-5 | 存（05） |
| K-058 | 记账与统计 | `alloc.c:281,348-367,486` | 可观测 | 存（05） |
| K-059 | 双分配检测 | `alloc.c:338-346,501-545` | 调试防线 | 存+新 |
| K-060 | 保留队列结构 | `alloc.c:56-237`；`memlist.h` | 自举供给的数据结构 | 存（05/06） |
| K-061 | Rust 元数据放置 | `phys_mem/mod.rs` | 64 位差异 | 存（05） |

- 验收标准：能解释位图「1=空闲」与 `findbit` 的双扫描；给出 `alloc_mem` 对齐多要的浪费量算例；能说出三后端在同一请求上的行为差异与选择来源；`usedpages` 检测能发现哪类 bug。

### 07-page-tables

- 一句话定位：页表是什么、VM 为什么不能直接读写它、Direct Map 如何把「两个地址」变成「一个地址」。
- 讲什么：K-062~K-067：MMU 翻译与多级页表；PTE 位与 `ARCH_VM_*` 宏族（含 earm 变体）；双视图问题与 C `pt_t`；Direct Map 常量偏移与双窗口（A-1）；VM 自身页表与自映射（A-9）；`pt_init` 结构面与静态自举资源；`pt_bind`/`pt_mapkernel` 的结构语义。
- 不讲什么：页表操作（08）；自用页分配器与备用页池消费（09）；页表页的物理记账（06）；用户区域的建/拆（13）；内核 boot 期 DM 建立全过程（只讲 VM 侧窗口来源与 `establish_boot_dm` 的接缝）。
- 前置：00、06。
- 后置：08、09、15。
- 事实底线：`pt.h`；`arch/i386/pagetable.h`；`arch/earm/pagetable.h`；`arch/i386/include/vm.h`；`pagetable.c:1088-1356,1358-1489`；Rust `pagetable/mod.rs`、`direct_map.rs`、`pagetable/vm_self_map.rs`、`os/arch/src/arch/{paging,direct_map,dm_coverage}.rs`、`os/kernel/src/dm_coverage.rs`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-062 | 多级页表与宽度 | `pt.h`；`arch/i386/vm.h` | 结构基础 | 存（07） |
| K-063 | PTE 位与宏族 | `arch/*/pagetable.h` | 硬件接口 | 存（07） |
| K-064 | 双视图问题 | `pt.h:11-25` | 本篇核心问题 | 存（07） |
| K-065 | Direct Map | `direct_map.rs`；`arch/direct_map.rs` | A-1 解法 | 存（06/07） |
| K-066 | VM 自身页表与自映射 | `pagetable.c:1088-1356`；`vm_self_map.rs` | A-9 | 存（07） |
| K-067 | `pt_bind`/`pt_mapkernel` 结构语义 | `pagetable.c:1358-1489` | 内核映射 | 存（07） |

- 验收标准：能用一张图说明 C 的 `pt_t` 双视图与 Rust 常量偏移的映射关系；能解释「自举页表为什么必须先于动态分配存在」；`DirectMapArch` 的三个窗口常量与三架构实现在文中列出；抽查行号通过。

### 08-page-table-ops

- 一句话定位：VM 对页表做的全部事情：建、绑、写、拆、复制、查询，以及写完必须让 TLB 忘掉旧值。
- 讲什么：K-068~K-076：三件套生命周期；WMF 四模式；`pt_writemap`；按需分配与递归副作用；跨表复制三兄弟；查询校验；内核协作；TLB 纪律与写后 invlpg；Rust `Paging` 操作面；VMINHIBIT/SMP 停等。
- 不讲什么：页表结构（07）；自用页物理来源（06/09）；各服务如何调用（16-20 只给调用顺序一句）；内核 TLB 机制实现（挂 `edge_todo.md E-VMTLB`）。
- 前置：07。
- 后置：09、13、16、18、20。
- 事实底线：`pagetable.c` 操作族（`pt_ptalloc:494`、`pt_ptalloc_in_range:545`、`pt_map_in_range:631`、`pt_ptmap:685`、`pt_clearmapcache:751`、`pt_writable:761`、`pt_writemap:784`、`pt_checkrange:943`、`pt_new:990`、`pt_allocate_kernel_mapped_pagetables:1035`、`pt_copy:1069`、`pt_bind:1358`、`pt_free:1427`、`pt_mapkernel:1442`）；`vm.h:56-61`；Rust `os/arch/src/arch/paging.rs`、`os/arch/src/x86_64/paging.rs`（`walk_alloc`/`write_pte_dm`）、`vmproc/vmproc_handle.rs`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-068 | 生命周期三件套 | `pagetable.c:990-1437` | 操作族首领 | 存（08） |
| K-069 | WMF 四模式 | `vm.h:56-61` | 写映射契约 | 存（08） |
| K-070 | `pt_writemap` | `pagetable.c:784-935` | 精讲代表 | 存（08） |
| K-071 | 按需分配/递归副作用 | `pagetable.c:494-584` | 难点机制 | 存（06/07/08） |
| K-072 | 跨表复制三兄弟 | `pagetable.c:631-748,1069` | fork/LU 基座 | 存（08） |
| K-073 | 查询校验 | `pagetable.c:761,943` | 诊断 | 存（08） |
| K-074 | TLB 纪律 | `pagetable.c:751`；`x86_64/paging.rs` | 正确性根基 | 存（08） |
| K-075 | VMINHIBIT/SMP 停等 | `pagetable.c:799-815`；`proc.c:345-347` | 真 SMP 前提 | 存+新 |
| K-076 | Rust Paging 操作面 | `paging.rs`；`vmproc_handle.rs` | 落点 | 存（08） |

- 验收标准：能默写 `pt_writemap` 的输入校验顺序与两阶段写；能解释 `pt_ptalloc` 内层已分配时的释放返回协议；能说明「写 PTE 后立即 invlpg」为何在单线程模型下已足够、SMP 还缺什么（挂 E-VMTLB）。

### 09-vm-self-memory

- 一句话定位：VM 管理别人的内存之前，先得把自己的内存问题解决掉：自用页、自举池、堆和稳态化搬迁。
- 讲什么：K-077~K-088：双地址问题与 `vm_allocpage` 族；备用页池两阶段自举与稳态换血；四个配套（mappages/freepages/pagelock/addrok）；A-1 保留池消除；`vm_pt_alloc` 供给链；C slab 分配器；Rust HeapArena+VmAllocator；`PAGE_ALLOC_PTR`；`relocate` 稳态化；LU 支撑原语。
- 不讲什么：物理分配器（06）；页表结构与操作（07/08）；区域/ memtype（10-13）；RS 服务流程（23）；`alloc_pfn_reclaiming` 的缓存侧（22 一句）。
- 前置：08。
- 后置：10、23。
- 事实底线：`pagetable.c:59-110,235-455,1116-1345`；`alloc.c:56-237`；`slaballoc.c` 全；`utility.c:188-335`；`region.c:1535`；`vm.h:39,49`；Rust `alloc_page.rs`、`heap_arena.rs`、`global.rs`、`vm_self_map.rs`、`vmproc/table.rs`（swap_slots）、`vm_server.rs`（relocate）、`phys_mem/{mod,bitmap_alloc}.rs`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-077 | 自用页分配 | `pagetable.c:235-395`；`vm.h:49` | 双地址问题 | 存（06） |
| K-078 | 备用页池自举 | `pagetable.c:54-57,1305-1341` | 自举核心 | 存（06） |
| K-079 | 四配套操作 | `pagetable.c:235-455` | 配套语义 | 存（06） |
| K-080 | A-1 结构消除 | `vm_server.rs` 注释 | ARCH | 存（06/10） |
| K-081 | `vm_pt_alloc` 供给 | `alloc_page.rs` | 页表供给 | 存（06） |
| K-082 | C slab 分配器 | `slaballoc.c` 全；`slaballoc.c:82,94` | 自举堆 | 存（09） |
| K-083 | SLABALLOC 宏与消费 | `proto.h:133-134` | 类型约定 | 存（09） |
| K-084 | MEMPROTECT/JUNK | `slaballoc.c:42-116` | 调试硬化 | 存（09） |
| K-085 | Rust 堆 | `heap_arena.rs`；`global.rs` | A-3 v2 | 存（09） |
| K-086 | `PAGE_ALLOC_PTR` | `global.rs` | 分配器接线 | 存（09） |
| K-087 | 稳态化搬迁 | `pagetable.c:1311-1345`；`vm_server.rs` | 自举终点 | 存（10） |
| K-088 | LU 支撑原语 | `utility.c:188-335` | 热更新基座 | 存（10） |

- 验收标准：能解释「映射页表页需要页表」的递归链以及 C 与 Rust 各自的破环方式；能对照 `pt_init` 搬迁段与 Rust `relocate` 说明搬迁对象的变化；slab 与 HeapArena 的取舍有明确理由（A-3 三处一致）；不再出现已删除的 `missing_spares` 生产链叙述。

### 10-physical-page-state

- 一句话定位：一个物理页被谁引用、被引用几次、什么时候能回收——VM 账本的物理侧。
- 讲什么：K-089~K-095：两层物理页对象与三层结构；引用计数语义与归零条件；反向引用链表；PBF_INCACHE；PFN 索引模型与两态 PageSlot；refcount 宽度/饱和与验证；PfnAllocator。
- 不讲什么：CoW 分裂细节（14）；页缓存持有面（22）；memtype 回调（11）；区域槽位（12/13）；`pb_unreferenced` 的调用场景（13 讲释放漏斗，14 讲 CoW 保槽）。
- 前置：09。
- 后置：11、13、14。
- 事实底线：`region.h:23-35`；`phys_region.h:8-21`；`pb.c:32-168`；`region.c:60-96,168-250`；Rust `region/page_state.rs`、`region/mod.rs`、`sanity.rs`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-089 | 两层对象与三层结构 | `region.h:23-35`；`phys_region.h` | 模型 | 存（11） |
| K-090 | refcount 语义/不变量 | `pb.c:61-134` | 生命周期 | 存（11） |
| K-091 | 反向引用链表 | `pb.c:30,96-134` | 需要「谁在引用」 | 存（11） |
| K-092 | PBF_INCACHE | `region.h:35` | 缓存持有区分 | 存（11） |
| K-093 | PFN 索引与两态槽 | `page_state.rs` | Rust 重写 | 存（11） |
| K-094 | refcount 校验 | `page_state.rs`；`sanity.rs` | 一致性 | 存（11） |
| K-095 | PfnAllocator | `page_state.rs:33` | 借页接口 | 存（11/12） |

- 验收标准：能证明「refcount==0 ⟺ 可回收」并说明链表为空是前提；能解释 Rust 为什么把分散对象收敛为全局数组以及两态 PageSlot 为什么足够；`verify_refcounts` 能发现哪类错误（举一例）。

### 11-memtype

- 一句话定位：同样是「一段虚拟区域」，匿名页、设备页、共享页、文件页的行为为什么必须不同，以及 VM 如何让它们不同。
- 讲什么：K-096~K-104：框架/策略分离；`mem_type_t` 回调表与 NULL 默认；六类策略（anon/directphys/shared/contig/cache/mappedfile）的完整契约与差异；Rust trait 六实现、`PagefaultResult`、能力门控；共享源递减落点（G-V12-7）。
- 不讲什么：区域框架调用点（12/13）；页错误状态机（15）；CoW 分裂（14）；文件与缓存的协议细节（21/22）；六类的具体服务场景（16-24）。
- 前置：10。
- 后置：13、14、15、18、21、22。
- 事实底线：`memtype.h:12-31`；`mem_anon.c`、`mem_directphys.c`、`mem_shared.c`、`mem_anon_contig.c`、`mem_cache.c`、`mem_file.c` 各文件；Rust `memtype.rs`；`region/page_state.rs`（挂载点）。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-096 | 多态动机 | `region.c:664`；`memtype.rs` | 为什么 | 存（12） |
| K-097 | 回调表与默认 | `memtype.h:12-31` | 契约面 | 存（12） |
| K-098 | 匿名语义 | `mem_anon.c:56-151` | 代表实现 | 存（12） |
| K-099 | 直物语义 | `mem_directphys.c:28-78` | 差异实现 | 存（12/21） |
| K-100 | 共享语义 | `mem_shared.c:49-210` | 差异实现 | 存（12） |
| K-101 | 连续语义 | `mem_anon_contig.c:37-131` | 差异实现 | 存（12） |
| K-102 | 缓存/文件概览 | `mem_cache.c`；`mem_file.c` | 指向 21/22 | 存（12） |
| K-103 | Rust trait/门控 | `memtype.rs` | 落点 | 存（12） |
| K-104 | 共享源递减落点 | `region/mod.rs`；`vir_region.rs` | 借用安全决策 | 存（12） |

- 验收标准：能用一张 6×N 能力矩阵说清每类支持哪些操作（分裂/低缩/引用计数/region id/fork 共享）；能解释「NULL 回调=框架默认」在 Rust 中如何表达；contig 的 `ev_new`、`pt_flags`、`writable` 均有现状锚点。

### 12-region-ledger

- 一句话定位：地址空间如何被切成分段区域、如何按地址找到区域、如何为新映射找到空位。
- 讲什么：K-105~K-111：区域模型与不重叠不变量；`vir_region` 结构与标志；`physblock_get/set` 与 `vm_total`；C AVL 五向搜索与路径栈迭代器；空槽查找算法与 hint；A-4 BTreeMap 替代；`map_lookup`。
- 不讲什么：区域操作族（13）；memtype 回调（11）；索引在服务中的消费（16-20 只给一句）；AVL 宏展开细节（明确排除）。
- 前置：11。
- 后置：13、17、18、20、24。
- 事实底线：`region.h:37-78`；`region.c:302-416,463-510,616-641`；`cavl_if.h`、`cavl_impl.h`、`regionavl_defs.h`、`unavl.h`、`regionavl.c`；Rust `region/region_map.rs`、`region/vir_region.rs`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-105 | 区域模型/不重叠 | `region.h:37-78` | 概念 | 存（13） |
| K-106 | vir_region 与 VR_* | `region.h:37-78` | 结构 | 存（13） |
| K-107 | 槽位契约/vm_total | `region.c:60-96` | 记账 | 存（11/13） |
| K-108 | AVL 与迭代器 | `cavl_*.h` | C 索引 | 存（14） |
| K-109 | 空槽查找 | `region.c:302-416` | 分配策略 | 存（14） |
| K-110 | A-4 BTreeMap/SearchType | `region_map.rs` | Rust 索引 | 存（14） |
| K-111 | `map_lookup` | `region.c:616-641` | 地址解析 | 存（13） |

- 验收标准：能画出「地址 → 区域 → 槽位 → 物理页」的查询链；能说明 C 的 AVL 五向搜索与 Rust `SearchType` 的对应与已删项；能解释空槽查找为何需要 hint 与页对齐；C 与 Rust 选槽方向差异有明确登记。

### 13-region-ops

- 一句话定位：区域账本上允许做的全部操作：建、填、复制、扩、缩、拆、释放，以及每一步的能力门控。
- 讲什么：K-112~K-123：操作族总览；建区；`map_pf`/批量填充；写保护组装；复制即共享；整进程复制；扩展；拆除三/四情形；释放漏斗与顺序；能力门控；工具与诊断；Rust 落点与 `free_region_pages`。
- 不讲什么：索引与查找（12）；memtype 策略（11）；CoW 分裂（14）；各服务入口与错误语义（16-20）；缓存回收（22）。
- 前置：12。
- 后置：14、16、17、18、19、20。
- 事实底线：`region.c` 全（22 个框架函数，锚点见 §1.4 与 K 表）；`region.h`；`phys_region.h`；Rust `region/region_map.rs`、`region/vir_region.rs`、`region/mod.rs`、消费模块 `fork.rs`/`munmap.rs`/`brk.rs`/`rs.rs`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-112 | 操作族总览 | `region.c` | 主干 | 存（13） |
| K-113 | 建区 | `region.c:424-510` | 建 | 存（13） |
| K-114 | `map_pf` 填充 | `region.c:664-754` | 填 | 存（13/16） |
| K-115 | 批量填充族 | `region.c:257-295,756-906` | 填 | 存（13） |
| K-116 | `map_copy_region` | `region.c:802-849` | 复制 | 存（13/17） |
| K-117 | 整进程复制 | `region.c:933-999` | 复制 | 存（13/18） |
| K-118 | 扩展 | `region.c:1002-1060` | 扩 | 存（13/19） |
| K-119 | 拆除三/四情形 | `region.c:1065-1294` | 拆 | 存（13/21） |
| K-120 | 释放漏斗 | `region.c:527-612` | 释放 | 存（13/21/22） |
| K-121 | 能力门控 | `region.c:1096,1164` | 边界 | 存（21） |
| K-122 | 工具与诊断 | `region.c:40-168,645,1510` | 工具 | 存+新 |
| K-123 | Rust 落点 | `region/mod.rs` 等 | 落点 | 存（13/21） |

- 验收标准：能复述 `map_unmap_range` 的四情形与 `map_unmap_region` 的三情形，并说明 `split_region` 时引用计数如何变化；释放漏斗的顺序无关性有论证；directphys 为什么拒绝分裂/低缩有锚点；`free_region_pages` 是正文中唯一的释放入口叙事。

### 14-cow

- 一句话定位：fork 之后父子共享同一物理页，谁来保证写时不互相踩：引用计数、只读 PTE 与首次写分裂的三方协议。
- 讲什么：K-124~K-132：CoW 四阶段；共享建立（`pb_reference`/`pb_link`/`fork_region`）；写保护两步；可写判定与 memtype writable；`mem_cow` 五步与换型；refcount≤1 快路合取；文件后备 `cow_block`/clearend；两阶段释放；C 缺陷与 Rust 修复。
- 不讲什么：缺页状态机（15）；fork 编排（16）；VFS 队列与 fdref（21）；共享内存的完整服务语义（11/18 已给契约，本篇只讲 CoW 相关面）。
- 前置：10、11、13。
- 后置：15、16、18。
- 事实底线：`pb.c:61-168`；`mem_anon.c:105-113`；`mem_file.c:59-82,173-177`；`mem_shared.c:122-165`；`region.c:130-134,257-295,820-849`；Rust `fork.rs`（`fork_region`）、`region/vir_region.rs`（`prepare_cow`/`needs_cow`）、`region/page_state.rs`（`PageFlags::COW`）、`vmproc/vmproc_handle.rs`（`setup_cow_for_all_regions`/`write_page_table_mappings`）、`cow_exec_pf.rs`（`cow_resolve_core`）。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-124 | CoW 四阶段 | `region.c:820-849`；`pb.c` | 概念骨架 | 存（17） |
| K-125 | 共享建立 | `pb.c:61-91`；`fork.rs` | 阶段一 | 存（17） |
| K-126 | 写保护两步 | `vir_region.rs`；`vmproc_handle.rs` | 阶段二 | 存（17） |
| K-127 | 可写判定 | `region.c:130-134`；`memtype.rs` | 判据 | 存（17） |
| K-128 | `mem_cow` 分裂 | `pb.c:136-168` | 阶段四 | 存（17） |
| K-129 | 快路合取 | `cow_exec_pf.rs`；`mem_file.c:173-177` | 安全前提 | 存（16） |
| K-130 | 文件 CoW/clearend | `mem_file.c:59-82` | 文件页分裂 | 存（17/23） |
| K-131 | 两阶段释放 | `vir_region.rs`；`page_state.rs` | 生命周期尾 | 存（17） |
| K-132 | C 缺陷与修复 | `region.c:836-842`；`mem_anon.c:75-92` | 重写价值 | 存（17） |

- 验收标准：能解释「refcount 决定语义、PTE 决定谁触发」；能用状态图描述一次写故障从 PTE 只读到分裂完成的全部动作；能说明文件页即使独占也不能直接翻写位的原因；`remaps>0` 例外有明确写出。

### 15-pagefault

- 一句话定位：缺页是 VM 被内核停住进程后的仲裁流程，也是 VM 主动向内核保证内存可用的反向流程。
- 讲什么：K-133~K-140：两条入口与汇合点；`VM_PAGEFAULT` 消息与转发协议；`handle_pagefault` 全流程与 SIGSEGV；主动路径状态机；同步分支三条件；SUSPEND 恢复；统计；错误串；Rust 动作表与 PTE 写不变量。
- 不讲什么：CoW 分裂本体（14）；VFS 队列与 fdref（21）；页缓存（22）；mmap 建立（18）；内核异常入口实现（01-stage-kernel 边界，只给协议）。
- 前置：14。
- 后置：16、20、21。
- 事实底线：`pagefaults.c` 全（418 行）；`region.c:664-774`；`main.c:153-164,731-750`；`kernel/arch/i386/exception.c:93-125`；`arch/i386/pagetable.h:36-39`；Rust `cow_exec_pf.rs`、`vm_server.rs`（`dispatch_pagefault`/`handle_signal`）、`memtype.rs`、`minix-types/src/ipc/vm.rs`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-133 | 两入口与汇合点 | `pagefaults.c:240,294`；`region.c:664` | 主干 | 存（16） |
| K-134 | `handle_pagefault` | `pagefaults.c:76-158` | 被动主流程 | 存（16） |
| K-135 | 主动状态机 | `pagefaults.c:39-49,170-289` | 主动主流程 | 存（16） |
| K-136 | 同步三条件 | `pagefaults.c:294-417` | 防死锁 | 存（16） |
| K-137 | SUSPEND 恢复 | `pagefaults.c:140-168` | 协议 | 存（16） |
| K-138 | major/minor | `pagefaults.c:135-138` | 统计 | 存（16） |
| K-139 | `pf_errstr`/PFERR | `pagefaults.c:59-70` | 诊断 | 存（16） |
| K-140 | Rust 动作表/不变量 | `cow_exec_pf.rs`；`vm_server.rs` | 落点 | 存（16） |

- 验收标准：能画出被动/主动两条路径到 `map_pf` 的汇合图；能解释 `vfs_avail` 与 retry 语义如何防止无限循环；能说明「成功恢复」路径当前 Rust 还缺什么（不夸大为已实现）；PTE 写不变量列出违反后果与准入约束。

### 16-vm-fork

- 一句话定位：VM_FORK 如何把「复制一个地址空间」组织成一次带停等、可回滚、最后向内核注册新进程的服务调用。
- 讲什么：K-141~K-147：三方职责与路径图；VMINHIBIT 与消息页同步契约；七阶段编排与子进程初始化；ACL/标志继承与 endpoint 合成；两层回滚；消息页预填充；Rust 编排。
- 不讲什么：CoW 机制（14）；缺页状态机（15）；区域复制原语（13）；exit 逆向清算（20）；VFS fd 复制（05-stage-vfs）。
- 前置：04、05、13、14、15。
- 后置：20（对照）、23（LU 中 fork 语义）。
- 事实底线：`fork.c:32-115`；`region.c:933-999`；`acl.c:110-116`；`utility.c:84-101`；`kernel/system/do_fork.c`；`com.h:360,633-635`；`pagefaults.c:245-252`；Rust `fork.rs`、`vmproc/vmproc_handle.rs`（`init_from_fork`/`init_page_table`/`write_page_table_mappings`）、`kernel_gateway.rs`（`sys_fork`）、`minix-types`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-141 | 三方职责/路径图 | `fork.c`；`do_fork.c` | 主干 | 存（18） |
| K-142 | VMINHIBIT/消息页 | `com.h:360`；`do_fork.c:112-116` | 同步契约 | 存（18） |
| K-143 | 七阶段编排 | `fork.c:32-115` | 主干 | 存（18） |
| K-144 | ACL/标志继承与注册 | `fork.c:83-108` | 继承 | 存（18） |
| K-145 | 两层回滚 | `fork.c:70-80,89-108` | 失败 | 存（18） |
| K-146 | 消息页预填充 | `fork.c:97-108`；`pagefaults.c:245` | 细节 | 存（18） |
| K-147 | Rust 编排 | `fork.rs` | 落点 | 存（18） |

- 验收标准：能复述七个阶段与每阶段的失败处理；能解释为什么 `sys_fork` 之后不可回滚；能说明消息页预填充存在与缺失时各发生什么；Rust 的「先 bind 后注册」顺序改进有理由。

### 17-vm-brk

- 一句话定位：堆是数据段顶上一条向上生长的虚拟区间；brk 只移动它的边界，物理页惰性跟上。
- 讲什么：K-148~K-152：堆位置与 brk/sbrk 协议；C 只增语义与 Rust 收缩差异；入口与错误统一；扩展编排与冲突；`anon_resize`；Rust 三态编排与释放。
- 不讲什么：mmap 分配（18）；区域操作原语（13）；用户态 malloc（阶段 14）；栈增长（C 不处理，明确记录）。
- 前置：12、13。
- 后置：18（区间下界参考）。
- 事实底线：`break.c` 全（69 行）；`region.c:1002-1060`；`mem_anon.c:115-130`；`libc/sys/brk.c:24-34`；`com.h:636`；`ipc.h:918-926`；Rust `brk.rs`、`ipc/dispatcher.rs`（call table）、`minix-types`（`VmBrkIn`/`Out`）。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-148 | 堆位置与 brk 协议 | `break.c:3-17`；`libc/sys/brk.c` | 概念 | 存（19） |
| K-149 | C/Rust 收缩差异 | `region.c:1016`；`brk.rs` | 演进 | 存（19） |
| K-150 | 入口与错误 | `break.c:44-69` | 服务入口 | 存（19） |
| K-151 | `anon_resize`（A-12） | `mem_anon.c:115-130` | 回调 | 存（19） |
| K-152 | Rust 三态编排 | `brk.rs` | 落点 | 存（19） |

- 验收标准：能解释扩展为什么只改元数据、收缩为什么真正释放；能说明 Rust 收缩后堆顶页对齐的后果（与扩展互补）；C 的「收缩静默忽略」与 Rust 的「真正释放」在文档中有明确差异声明。

### 18-vm-mmap

- 一句话定位：mmap 做两件事——在地址空间找一段空位，并给这段空间绑定内存来源；文件来源要走异步对话。
- 讲什么：K-153~K-158：地址分配 + 来源绑定模型；三路地址解析；权限模型（PROT/execpriv/UNINITIALIZED）；匿名与文件分流、异步两段式；`do_vfs_mmap` 与 `MAP_PREALLOC`；`do_remap`；`map_perm_check` 与 `do_map_phys` 摘要（权限面归 19 精讲）。
- 不讲什么：拆除（19）；页缓存（22）；VFS 队列与 fdref 机制（21）；区域原语（13）；物理映射权限检查完整版（19）。
- 前置：05、12、13。
- 后置：19、21。
- 事实底线：`mmap.c:36-435`；`region.c:302-416,463-510`；`mem_file.c:191-246`；`mem_shared.c:167-205`；`vfs.c`；`libc/sys/mmap.c`；`sys/sys/mman.h`；`ipc.h:1582-1592,2369-2380`；Rust `mmap.rs`、`map_phys.rs`（摘要）、`ipc/dispatcher.rs`、`vfs_queue.rs`、`minix-types`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-153 | 分配+绑定模型 | `mmap.c:200-269` | 概念 | 存（20） |
| K-154 | 权限模型 | `mmap.c:208-221` | 概念 | 存（20） |
| K-155 | 匿名/文件分流 | `mmap.c:84-195,254-273` | 核心 | 存（20） |
| K-156 | `do_vfs_mmap`/prealloc | `mmap.c:46,135-158` | 接口 | 存（20） |
| K-157 | remap | `mmap.c:366-435` | 接口 | 存（20） |
| K-158 | map_perm_check 摘要 | `mmap.c:284-363` | 权限 | 存（20/21） |

- 验收标准：三路地址解析每种都能给出落点与失败 errno；文件映射的 SUSPEND→续作完整时序可复述；execpriv 分级有具体位与调用者；`MAP_SHARED` 匿名在 fork 时的实际行为（按私有 CoW）有明确写出。

### 19-vm-munmap

- 一句话定位：拆除映射的统一入口：定位、分裂、解除引用、回收结构，并把设备物理映射的特殊性讲清。
- 讲什么：K-159~K-162：四入口统一与 target/长度语义；拆除四步与页命运四场景；`munmap_vm_lin`；Rust `unmap_range`/释放漏斗/fdref 平衡；物理映射 `map_perm_check`/`do_map_phys` 精讲与 directphys 回调矩阵。
- 不讲什么：mmap 建立（18）；区域原语（13）；exit 整进程释放（20）；页缓存（22）；查询（24）。
- 前置：13、18。
- 后置：无（20 的释放语义由 13 承担）。
- 事实底线：`mmap.c:284-363,488-573`；`region.c:1065-1294`；`mem_directphys.c:28-78`；`pb.c:96-134`；`mem_file.c:280-287`；`fdref.c:116-154`；`com.h:649-651,677-679,718`；Rust `munmap.rs`、`map_phys.rs`、`region/mod.rs`、`region/vir_region.rs`、`memtype.rs`（`DirectPhysical`）、`minix-types`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-159 | 四入口/target 语义 | `mmap.c:512-573` | 服务入口 | 存（21） |
| K-160 | 拆除四步/页命运 | `pb.c:96-134`；`region.c:1222` | 核心 | 存（21/22） |
| K-161 | `munmap_vm_lin` | `mmap.c:488-510` | VM 自身 | 存（21） |
| K-162 | Rust 四情形/释放 | `munmap.rs`；`region/mod.rs` | 落点 | 存（21） |

- 验收标准：能解释为什么 `VM_MAP_PHYS` 与拆除共用一个 handler 文件但语义相反；页命运四场景表完整；`do_unmap_phys` 死声明这类 C 事实有明确登记；Rust 能力门控（`supports_split`/`supports_low_shrink`）有测试锚点。

### 20-vm-exit

- 一句话定位：进程生命周期的最后一站：先冻结，再清算，最后把页表、物理页、ACL 与统计全部归还；以及 VFS/RS 如何在运行中遥控清场。
- 讲什么：K-163~K-168：两阶段退出与 EXITING 门；`free_proc`/`clear_proc`/`reset_vm_rusage`；释放链与 fdref 归还；VM 实例计数；PROCCTL 的 CLEAR/HANDLEMEM；Rust typestate 与错误码。
- 不讲什么：区域释放原语（13）；缺页状态机（15）；fork（16）；fdref 表结构（21）；查询（24）。
- 前置：02、13、15。
- 后置：23（LU 后旧实例退出）。
- 事实底线：`exit.c` 全（156 行）；`region.c:527-612`；`pagetable.c:990-1026,1358-1437`；`pb.c`；`pagefaults.c:245-289`；`main.c:131-148,419-426`；`acl.c:121-129`；`glo.h:46`；`com.h:630-644,752-760`；`ipc.h:77-83`；`vfs/comm.c:198-217`；`pm/forkexit.c:332,455`；Rust `exit.rs`、`vmproc/vmproc_handle.rs`、`vmproc/table.rs`、`ipc/dispatcher.rs`、`minix-types`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-163 | 两阶段退出 | `exit.c:60-114` | 服务主干 | 存（22） |
| K-164 | 三个清理函数 | `exit.c:25-58` | 清算 | 存（22） |
| K-165 | 释放链与 fdref | `region.c:589-612`；`exit.rs` | 清算 | 存（22） |
| K-166 | VM 实例计数 | `exit.c:76-81` | LU | 存（22） |
| K-167 | PROCCTL 通道 | `exit.c:117-156` | 控制通道 | 存（22） |
| K-168 | Rust typestate/错误 | `exit.rs`；`vmproc_handle.rs` | 落点 | 存（22） |

- 验收标准：能解释「未 WILLEXIT 直接 EXIT 会被拒」的运行时代码位置；能说明 EXIT 时页表与物理页分别由谁释放；HANDLEMEM 的 SUSPEND 偏差有诚实标注；CLEAR 后 rusage 为何必须归零。

### 21-vfs-file-mapping

- 一句话定位：文件后备映射的两条链路：映射建立要先向 VFS 查文件，缺页要请 VFS 供页；中间靠一条串行队列与一张 fd 引用表。
- 讲什么：K-169~K-175：三类请求与职责；串行激活队列与 req_id 校验；fdref 三职责与 `mayclosefd`；文件页缺页三岔路/`referenced_offset`/ONCE；`mappedfile_setfile` 与回调族；VFS 侧 `do_vm_call`/PEEK；Rust 队列/fdref/NeedVfsIo；wire 修复与运输缺口（挂 E-VFSWIRE）。
- 不讲什么：页缓存内部（22）；mmap 区域建立全貌（18）；CoW 分裂本体（14）；SUSPEND 框架（04）。
- 前置：11、13、18。
- 后置：22。
- 事实底线：`vfs.c` 全（143 行）；`fdref.c` 全（177 行）；`mem_file.c` 全（287 行）；`mmap.c:84-195`；`pagefaults.c:161-235,336-417`；`minix3/minix/servers/vfs/misc.c:380-480`；`libminixfs/cache.c:298-465`；`com.h:694-714`；`ipc.h:85-91`；Rust `vfs_queue.rs`、`fdref.rs`、`mmap.rs`（文件路径）、`memtype.rs`（MappedFile）、`cow_exec_pf.rs`、`ipc/dispatcher.rs`、`minix-types`（`MessVmVfsReply`/`VmVfsReplyIn`）。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-169 | 三类请求 | `com.h:694-704`；`vfs.c` | 协议 | 存（23） |
| K-170 | 串行队列/req_id | `vfs.c:31-142` | 队列机制 | 存（23） |
| K-171 | fdref 三职责 | `fdref.c:93-176` | 引用管理 | 存（23） |
| K-172 | 缺页三岔路/ONCE | `mem_file.c:98-152` | 文件页 | 存（23） |
| K-173 | setfile/prefill/回调 | `mem_file.c:177-287` | 区域回调 | 存（23） |
| K-174 | VFS 侧闭环 | `vfs/misc.c`；`libminixfs/cache.c` | 对端 | 存（23） |
| K-175 | Rust 队列/fdref | `vfs_queue.rs`；`fdref.rs` | 落点 | 存（23） |

- 验收标准：能画出 FDLOOKUP/FDIO/FDCLOSE 三条链路的完整时序（含 SUSPEND 与恢复）；能解释为什么队列必须串行、fdref 为什么按 `(owner, ino, dev, fd)` 去重；ONCE 页的语义与消费点有锚点；缺口（真正 IPC 发送）以 edge 条目为准，不写成已实现。

### 22-page-cache

- 一句话定位：所有文件系统共用的磁盘块缓存：用键找到页、用 LRU 淘汰页、用四个 IPC 让 FS 把页挂进自己的地址空间。
- 讲什么：K-176~K-182：缓存为何在 VM；`cached_page` 结构与不变量；双键模型与延迟 ino 更新；LRU 双链；refcount/INCACHE 淘汰与压力回收；目录操作；四个缓存 IPC；`CacheMemory` 与 ONCE；Rust 单键重写与索引型 LRU。
- 不讲什么：VFS 队列（21）；memtype 回调体系（11）；物理分配器（06）；主循环分发（04）；ONCE 的消费闭环（21 一句、本篇给缓存侧）。
- 前置：11、21。
- 后置：无。
- 事实底线：`cache.c` 全（332 行）；`cache.h`；`mem_cache.c` 全（324 行）；`alloc.c:260-262`；Rust `page_cache.rs`、`ipc/cache_handlers.rs`、`memtype.rs`（`CacheMemory`）、`region/page_state.rs`（IN_CACHE）、`minix-types`（`MessVmmcp`/`MessVmmcpReply`）。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-176 | 缓存为何在 VM | `mem_cache.c` | 动机 | 存（24） |
| K-177 | cached_page/双键 | `cache.h:2-21`；`cache.c:163-214` | 目录 | 存（24） |
| K-178 | LRU 双链 | `cache.c:29-75` | 淘汰序 | 存（24） |
| K-179 | 淘汰/压力回收 | `cache.c:243-307` | 回收 | 存（24） |
| K-180 | 四个缓存 IPC | `mem_cache.c:95-324` | 服务面 | 存（24） |
| K-181 | CacheMemory/ONCE | `mem_cache.c:39-193` | 类型 | 存（24） |
| K-182 | Rust 重写 | `page_cache.rs`；`cache_handlers.rs` | 落点 | 存（24） |

- 验收标准：能解释 `(dev, offset)` 与 `(dev, ino, offset)` 两个查询入口的关系；能说明哪些页不可淘汰；mapcache 的失败回滚与 C 同序有证据；x86_64 wire 加宽（E-VMMCPWIRE）在契约中定位准确。

### 23-rs-live-update

- 一句话定位：RS 热更新时 VM 的四件事：授权、钉住、交换、预分配，以及 VM 自身实例化的特殊路径。
- 讲什么：K-183~K-190：LU 动机与三角色；四请求协议与字段；窗口零分配约束与预分配；UPDATE 的 SUSPEND/手动回复；RprocTab 与槽交换后的引用重绑；MAKE_VM 与 `SF_VM_*`；A-8 缺口契约；Rust 落点。
- 不讲什么：SEF 生命周期与握手完整叙事（01）；`swap_proc_slot`/`swap_proc_dyn_data` 原语机制（09）；ACL 位图语义（03）；分发框架（04）；查询（24）。
- 前置：09、20。
- 后置：无。
- 事实底线：`rs.c` 全（391 行）；`main.c:215,241-260,722,755-768`；`utility.c:477-492`；`region.c:1535`；`rs.h:165-199`；`com.h:724-766`；`ipc.h:1529-1534`；Rust `rs.rs`、`vm_server.rs`（`rs_handshake`/`ipc_call_rs_init`/`RprocTab`）、`ipc/dispatcher.rs`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-183 | LU 动机/三角色 | `rs.c` | 概念 | 存（25） |
| K-184 | 四请求协议 | `rs.c:34-390` | 服务面 | 存（25） |
| K-185 | 零分配约束/预分配 | `rs.c:73-145,281-344` | 约束 | 存（25） |
| K-186 | UPDATE SUSPEND | `rs.c:150-213` | 切换 | 存（25） |
| K-187 | RprocTab/重绑 | `main.c`；`utility.c:477-492` | 握手 | 存（25） |
| K-188 | MAKE_VM/SF_VM_* | `rs.c:218-276`；`rs.h:198-199` | 实例化 | 存（25） |
| K-189 | A-8 缺口 | `rs.rs` 注释 | 诚实登记 | 存（25） |
| K-190 | Rust 落点 | `rs.rs`；`dispatcher.rs` | 落点 | 存（25） |

- 验收标准：能解释为什么 UPDATE 不能用主循环自动回复；能说明 PREPARE 对堆与 mmap 区各做什么准备；缺口状态（MAKE_VM 恒拒、SUSPEND 删除、pinning 缺失）逐条与代码注释一致，且每条都能指向 `edge_todo.md` 的对应条目。

### 24-vm-queries

- 一句话定位：VM 作为内存权威的只读窗口：统计、用量、区域列表、物理地址与引用计数，以及一个 PM 专用的 rusage。
- 讲什么：K-191~K-198：查询职责；四请求面；INFO 三模式与特殊端点；GETPHYS/GETREF 真相与能力门控；钉页纪律；用量统计与栈启发式；区域列表分页；GETRUSAGE；Rust 类型化查询与编码。
- 不讲什么：区域生命周期（13）；页缓存统计口径（22）；rusage 清零时机（20）；分发框架（04）；memtype 定义本体（11）。
- 前置：12、13。
- 后置：无。
- 事实底线：`utility.c:100-184,426-472`；`mmap.c:438-483`；`region.c:1323-1505`；`cache.c:328-331`；`com.h:720-764`；`minix3/minix/include/minix/vm.h:40-66`；`libsys/vm_info.c`；Rust `query.rs`、`ipc/dispatcher.rs`、`ipc/encode.rs`、`boot.rs`、`memtype.rs`（能力门控）、`minix-types`（`VmRegionInfo`/`VmReply`）。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-191 | 查询职责 | `com.h:720-764` | 概念 | 存（26） |
| K-192 | INFO 三模式 | `utility.c:100-184` | 主干 | 存（26） |
| K-193 | GETPHYS/GETREF 真相 | `mem_anon.c:132-145`；`mem_shared.c:99-108` | 语义澄清 | 存（26） |
| K-194 | 钉页纪律 | `utility.c:166-171` | 约束 | 存（26） |
| K-195 | 用量统计/栈启发式 | `region.c:1357-1447` | 统计 | 存（26） |
| K-196 | 区域列表/分页 | `region.c:1452-1505`；`vm.h:66` | 接口 | 存（26） |
| K-197 | GETRUSAGE | `utility.c:426-472` | 特例 | 存（26） |
| K-198 | Rust 查询/编码 | `query.rs`；`encode.rs` | 落点 | 存（26） |

- 验收标准：能说清 `VM_INFO` 的 `what` 取值与三种回复的关系；能解释 GETPHYS 返回的其实不是物理地址；`MAX_VRI_COUNT` 分页的游标语义与越界行为有锚点；所有查询的 errno 与 C 一致。

### 25-test-observability

- 一句话定位：VM 是如何被测试和被观测的：注入缝、parity 矩阵、feature 组合、审计计数与调试校验。
- 讲什么：K-199~K-203：`KernelGateway`/`SimPaging`/`TestIpcTransport` 注入；三后端 parity 与 feature 矩阵；QEMU 冒烟与跨 crate 测试；audit/计数器/sanity/vmstats；测试统计与覆盖率工具链口径。
- 不讲什么：任何机制语义；单个测试的逐条清单（B 相在各机制篇保留「行为—测试函数」索引，本篇汇总矩阵与运行方式）。
- 前置：00（工程附录，可在任意时机阅读）。
- 后置：无。
- 事实底线：`os/servers/vm/src/{kernel_gateway.rs,pagetable/sim.rs,ipc/transport.rs,audit.rs,sanity.rs,phys_mem/allocator_tests.rs}`；`os/servers/vm/Cargo.toml`；`os/Cargo.toml`；`os/qemu-tests/`；`os/tests/pm_vm_fork_test.rs`；`tools/coverage-extract/coverage-extract.py`；`tools/design-coverage-check.sh`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-199 | 注入缝 | `kernel_gateway.rs`；`pagetable/sim.rs` | 可测性设计 | 存（00/15/16） |
| K-200 | parity/feature 矩阵 | `allocator_tests.rs`；`Cargo.toml` | 回归 | 存（05/06） |
| K-201 | QEMU/跨 crate 测试 | `os/qemu-tests/`；`os/tests/` | 端到端 | 新 |
| K-202 | 可观测性 | `audit.rs`；`sanity.rs` | 观测面 | 存+新 |
| K-203 | 统计口径/覆盖率工具 | `tools/` | 纪律 | 新 |

- 验收标准：能复现三 feature 测试矩阵与 clippy 命令；能指出每条注入缝的生产实现与测试实现；全目录不再出现互相矛盾的测试数字（统计只在本文与各篇「行为—测试索引」引用）。

### 99-global-concepts

- 一句话定位：查得到、对得上的常量与全局状态词典：每个魔数的值、来源、消费者和单源权威。
- 讲什么：K-204~K-209：调用号族；PAF/WMF/VR/MF 旗标；endpoint 编码与特殊端；全局状态显式化三档；哨兵类型化；单源纪律。另收录 `AM_AUTO`/`MINSTACKREGION`/`VMP_SPARE` 等无处安放的常量。
- 不讲什么：任何机制展开（只给「定义处 + 消费点」两列）；与机制篇重复的表格。
- 前置：无（参考性阅读；机制篇出现常量时只引用本篇条目名，不要求先读）。
- 后置：全阶段（作为查询表）。
- 事实底线：`com.h`、`vm.h`、`region.h`、`cache.h`、`glo.h`、`endpoint.h`、`pt.h`；Rust `minix-types`、`phys_mem/types.rs`、`region/vir_region.rs`、`page_cache.rs`、`global.rs`、`mmap.rs`、`vmproc/table.rs`。
- 知识点清单：

| K | 名称 | 锚点 | 归本篇理由 | 来源 |
|---|------|------|-----------|------|
| K-204 | 调用号族 | `com.h:627-780` | 词典 | 存（99） |
| K-205 | PAF_*（修正 ALIGN16K=0x40） | `vm.h:22-27` | 词典 | 存（99） |
| K-206 | WMF_*/VR_*/MF_* | `vm.h:56-61`；`region.h:69-78` | 词典 | 存（99） |
| K-207 | endpoint 编码 | `endpoint.h`；`endpoint.rs` | 词典 | 存（99/03） |
| K-208 | 全局状态三档/哨兵类型化 | `global.rs`；`page_state.rs` | 设计纪律 | 存（99） |
| K-209 | 单源纪律 | `minix-types` 等 | 设计纪律 | 存（99） |

- 验收标准：`PAF_ALIGN16K` 等已知错误全部修正；每个条目有「C 权威值 / Rust 权威值 / 消费点」三列；常量在别处不再重复定义，各机制篇引用本篇条目编号。

---

## 6. 变更表

> 操作类型：`重排`（整篇换位/换号）、`拆分`、`合并`、`新建`、`归档`。存量知识点去向受「写不出去向不许拆」约束；新增知识点来源已在 §2.2 标 `新`。

| 操作 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 |
|------|------|--------|--------|------|-----------|------|
| M-01 | 重排 | 00 | 00 | 导航改为「概念地图 + 阅读路径」，删除实施现状/统计 | K-001~005 | 现状表 → 25 |
| M-02 | 拆分 | 01 | 01（骨架）+ 04/05（主循环与 wire 细节） | 启动链与循环段职责分离 | K-006~020 | 主循环段 → 04；wire → 05 |
| M-03 | 合并 | 02 + 03 | 02 | 个体槽位与表集合语义同属「身份」一个语义单元且互相依赖 | K-021~028 | 表管理不再单篇 |
| M-04 | 重排 | 04 | 03 | ACL 先于分发（04 要引用门禁） | K-029~033 | — |
| M-05 | 重排+拆分 | 15 | 04（分发）+ 05（契约） | 循环生命周期与线格式各自成篇 | K-034~049 | wire 表 → 05；RS 握手 → 01 |
| M-06 | 重排 | 05 | 06 | 物理内存先于页表 | K-050~058 | 保留队列 → 09；`get_mem_chunks` 提纲 → 01 |
| M-07 | 拆分 | 06 | 07（结构/DM）+ 08（操作）+ 09（自用页/池） | 结构、操作、自举内存是三个语义单元 | K-060/K-062/K-065/K-071/K-077~081 | 启动时序 → 01 |
| M-08 | 重排+拆分 | 07 | 07（结构）+ 08（`pt_bind`/`mapkernel` 操作） | 按结构面/操作面重划边界，消除双向依赖 | K-062~067 | 操作细节 → 08 |
| M-09 | 重排 | 08 | 08 | 编号随 07 顺延，内容重划 | K-068~076 | 消费链细节 → 16-20 |
| M-10 | 合并 | 09 + 10 | 09 | slab/HeapArena/relocate 同属「VM 自己的内存」 | K-082~088 | LU 启动路径 → 01；RS 服务 → 23 |
| M-11 | 重排 | 11 | 10 | 物理页状态紧随自举内存 | K-089~095 | 缓存持有 → 22；CoW → 14 |
| M-12 | 重排 | 12 | 11 | 内存类型先于区域框架（消除前向引用） | K-096~104 | — |
| M-13 | 拆分 | 13 | 12（模型/索引）+ 13（操作族） | 账本模型与操作族分属概念与机制 | K-105~107、K-109、K-111~123 | 查找语义 → 12 |
| M-14 | 合并 | 14 | 12 | 索引是账本的一部分，单篇过薄 | K-108、K-110 | AVL 宏内部 → 明确排除 |
| M-15 | 重排 | 17 | 14 | CoW 前置于缺页（先共享协议后触发状态机） | K-124~132 | VFS 交互 → 21 |
| M-16 | 重排 | 16 | 15 | 缺页后置于 CoW | K-133~140 | CoW 快路 → 14；VFS 状态 → 21 |
| M-17 | 重排 | 18 | 16 | 服务篇从 fork 起 | K-141~147 | — |
| M-18 | 重排 | 19 | 17 | 服务篇顺序 | K-148~152 | — |
| M-19 | 重排+拆分 | 20 | 18 | 建立与拆除分离；map_phys 权限面归 19 | K-153~158 | 拆除/map_phys → 19 |
| M-20 | 重排 | 21 | 19 | 拆除篇 | K-159~162 | 区域原语 → 13；引用兑现 → 13 |
| M-21 | 重排 | 22 | 20 | 退出篇 | K-163~168 | 释放链 → 13；页表操作 → 08 |
| M-22 | 重排 | 23 | 21 | 跨服务组 | K-169~175 | ONCE 细节 → 22；InfoRegion 编码 → 24 |
| M-23 | 重排 | 24 | 22 | 跨服务组 | K-176~182 | CacheMemory 回调 → 11 |
| M-24 | 重排 | 25 | 23 | 跨服务组 | K-183~190 | 握手 → 01；槽交换原语 → 09 |
| M-25 | 重排 | 26 | 24 | 跨服务组 | K-191~198 | — |
| M-26 | **新建** | 无 | 25 | 非 C 主题「测试基建/可观测性/构建」此前无成篇，散落各篇且统计失真 | K-199~203（新+存量线索） | — |
| M-27 | 重排 | 99 | 99 | 改为纯查询表；修正已知错误 | K-204~209 | 机制示例 → 各机制篇 |
| M-28 | 归档 | `draft/` 30 篇 + `archive/` 2 份 | 归档（不删） | 旧 fork 主线素材与旧 TODO；本蓝图不引用 | — | 外部引用重指向新篇 |
| M-29 | 归档 | 旧 27 篇正文 | 重建后整目录归档（不删） | B 相完成后旧文本退出正式目录 | — | 断链迁移见 §8 |

**存量知识点去向完整性**：§2.2 全表 209 条均有新位置（K 编号即新契约归属）。**明确删除项**（不是遗失，是判定不进入新目录）：D-43 的十二处重复「Redox/Linux 对照节」（内容压缩为决策处一段）、G-11~G-14（调试宏/宏内部/include guard/日志开关）、旧文档中的修复日志与实现流水账（如 01 §4.5、21 §4.7「review 轮修复记录」），以及所有「实施现状/测试数字」节（K-203 收录口径）。

---

## 7. 缺漏新篇

### 7.1 非 C 主题落实（对应 §3.5）

| 主题 | 落实 |
|------|------|
| 链接与加载 | 已落实为 `01-vm-birth` 的知识点 K-012/K-013（含 minix-elf 与 `vm_exec_info` 新 KP） |
| 镜像与内存布局 | K-009（移交布局）落 `01`；K-065（DM 窗口）落 `07`；K-154 相关 `MMAP_BASE/TOP` 落 `18`；`MINSTACKREGION` 落 `99` |
| 汇编入口与陷阱进入 | K-134/K-133 落 `15`；K-045 落 `05`；`sys_exec` 切换落 `01` |
| 启动装配 | K-006~K-016 全部落 `01` |
| 构建与工具链 | feature 与 `panic=abort` 落 K-200/K-020（`25`/`01`）；无单篇 |
| 跨模块接口与线格式 | K-041~K-049 落 `05`（新篇） |
| 错误路径 | K-040 落 `04`（新系统性小节） |
| 关闭与退出 | K-163~K-167 落 `20`；VM 崩溃策略 K-020 落 `01` |
| 并发与同步 | K-003/K-004 落 `00`；信号打断落 `04`；PTE 不变量落 `15`；E-VMTLB 落 `08` |
| 测试基建 | K-199~K-203 落 `25`（新篇） |

### 7.2 覆盖缺口落实（对应 §3.2）

- G-01/G-09/G-10 作为新增 KP 落对应篇；G-02~G-08 并入既有 KP；G-04 独立成篇于 `25`；G-11~G-14 明确排除并说明理由。
- 机器缺口中的 20 个纯宏/内部符号（AVL 宏族、include guard 等）判定为非语义，不追加入池；余下 45 个全部获得归属（`25-test-observability` 或对应机制篇）。
- 无「待定」项：本节列出的每条缺口均有唯一去向。

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表

> 覆盖 27 篇编号正文（00-26）加 `99`，共 28 篇。对于「整篇同向迁移」的旧文档给一行并标注其内部拆出去的节；跨篇移动的节逐条列出。旧节以 `## / ###` 级语义单元为准。

| 旧位置 | 旧内容（一句话） | 新位置 | 迁移类型 | 断链风险 |
|--------|-----------------|--------|---------|---------|
| 00 全篇 | 总览/导航/实施现状 | 00（概念部分）+ 25（现状/测试） | 改写+拆分 | 高（导航表全变） |
| 01 全篇 | 启动链与 SEF | 01 | 改写 | 高 |
| 01 §2.4.3/§2.5 RS 握手 | RS_INIT/授权 | 01（保留）+ 23（服务语义） | 拆分 | 中 |
| 01 §3.7/§4.4/§5.3 | ARCH/修复日志/他篇测试 | 25；删除日志 | 归档 | 低 |
| 02 §1-§2 | vmproc 字段/标志/生命周期 | 02 | 改写 | 高 |
| 02 §2.7 | `vm_acl`/ACL 细节 | 03 | 越界归位 | 中 |
| 03 §1-§2 | 进程表/endpoint/验证 | 02（表集合并入） | 合并 | 高 |
| 04 §1-§2 | ACL 数据与生命周期 | 03 | 改写 | 高 |
| 04 §4.4 | 分发闸接线 | 04 一句 + 03 语义 | 拆分 | 低 |
| 05 §1-§2 | 物理分配器 | 06 | 改写 | 高 |
| 05 §2.2 | `get_mem_chunks` | 01（调用提纲）+ 06（取整语义） | 拆分 | 中 |
| 05 §2.9/§3.8-S5/§4.5 | 保留队列与 missing_spares | 09（结构）+ 22（回收） | 拆分 | 高（旧叙述已过期） |
| 05 §3.2/§3.7 DM 段 | aarch64 DM 推导/PAF_CLEAR | 07（DM）/06（PAF_CLEAR） | 拆分 | 中 |
| 06 §1-§2 | 自用页/备用池/递归 | 09（页分配/池）+ 08（递归协议）+ 07（DM 引入） | 拆分 | 高 |
| 06 §1.5/§3.2 DM | Direct Map 概念 | 07 | 合并 | 中 |
| 06 §2.9/§4.3 | 主循环钩子 | 09（判定/删除说明） | 改写 | 高（已删） |
| 07 §1-§2 | 页表结构/宏/`pt_init` | 07 | 改写 | 高 |
| 07 §2.7/§2.10(pt_bind)/§2.11 | 绑定/内核映射 | 08 | 拆分 | 中 |
| 07 §3.4/§4.4 | Paging 操作面/walk_alloc | 08 | 拆分 | 中 |
| 08 §1-§2 | 页表操作族 | 08 | 改写 | 高 |
| 08 §4.5/§4.6 | 消费链 | 各服务篇 | 越界归位 | 低 |
| 09 §1-§4 | slab/HeapArena | 09 | 改写 | 高 |
| 09 §1.1/§4.4 | `__minix_init` 链 | 01 | 拆分 | 低 |
| 10 §1-§2 | 搬迁/spare/LU 原语 | 09（原语）+ 01（LU 启动） | 拆分 | 高 |
| 10 §4.5 | RS 七步状态 | 23 | 越界归位 | 中 |
| 11 §1-§3 | phys_block/refcount/PFN | 10 | 改写 | 高 |
| 11 §1.5/§4.3 | INCACHE/addcache | 22（持有面） | 拆分 | 低 |
| 12 §1-§3 | memtype 六类/回调 | 11 | 改写 | 高 |
| 12 §3.7 | 共享源递减落点 | 11（保留）+ 12（region id 一句） | 拆分 | 低 |
| 13 §1.2-1.5/§2.1-2.5 | 区域模型/建区/填充 | 12（模型）+ 13（操作） | 拆分 | 高 |
| 13 §2.3/§2.4 查找 | find_slot/map_lookup | 12 | 拆分 | 中 |
| 13 §2.6-§2.9 | 复制/扩展/拆除/释放 | 13 | 改写 | 高 |
| 14 全篇 | AVL/BTreeMap/空槽 | 12 | 合并 | 高 |
| 15 §1-§2 | 主循环/分发表/请求码 | 04（循环）+ 05（wire/请求码） | 拆分 | 高 |
| 15 §2.6 | RS 握手 SEF 形态 | 01 | 越界归位 | 中 |
| 15 §4.1/§5 | 主循环实现/测试 | 04 + 25 | 拆分 | 中 |
| 16 §1-§2/§4.1 | 缺页两路径/状态机 | 15 | 改写 | 高 |
| 16 §3.3/§4.3 | CoW 快路/分裂 | 14 | 拆分 | 中 |
| 16 §3.6/§4.4 | VFS 回调/TLB 现状 | 21/08 | 越界归位 | 中 |
| 17 §1-§3 | CoW 机制 | 14 | 改写 | 高 |
| 17 §2.1/§2.4/§3.6 | C 缺陷/fork 上下文 | 14 | 保留 | 低 |
| 18 §1-§4 | fork 编排 | 16 | 改写 | 高 |
| 18 §2.6/§4.4 | handle_memory_once | 15 | 越界归位 | 中 |
| 19 §1-§3 | brk | 17 | 改写 | 高 |
| 19 §3.3/§4.3 | region split/释放 | 13 | 越界归位 | 中 |
| 20 §1-§3 | mmap 建立 | 18（保留）+ 19（map_phys 精讲） | 拆分 | 高 |
| 20 §3.3/§3.4 | VFS 队列/fdref | 21 | 越界归位 | 中 |
| 21 §1-§3 | munmap/map_phys | 19（服务）+ 13（原语） | 拆分 | 高 |
| 21 §3.4 | fdref 平衡 | 21（协议）+ 13（释放） | 拆分 | 中 |
| 22 §1-§3 | exit/procctl | 20（服务）+ 13（释放链）+ 08（页表） | 拆分 | 高 |
| 23 §1-§3 | VFS 异步对话 | 21 | 改写 | 高 |
| 23 §3.4 | InfoRegion 编码 | 24 | 越界归位 | 低 |
| 24 §1-§3 | 页缓存 | 22（保留）+ 11（CacheMemory 契约） | 拆分 | 高 |
| 25 §1-§3 | RS 服务 | 23（服务）+ 01（握手）+ 09（原语） | 拆分 | 高 |
| 26 §1-§3 | 查询 | 24 | 改写 | 中 |
| 99 全篇 | 常量表 | 99 | 改写+纠错 | 中 |

### 8.2 引用迁移表

**A. 篇间引用（旧编号 → 新目标）**

| 旧引用目标（被引次数） | 新目标 | 备注 |
|----------------------|--------|------|
| 15-ipc-dispatch（28） | 04（循环/分发）+ 05（请求码/wire） | 引用最多；拆成两篇后需按语义分派 |
| 13-region-mapping（24） | 12（模型/索引）+ 13（操作） | 按引用语境拆分 |
| 12-memtype（17） | 11 | 同构顺延 |
| 01-vm-init-main（17） | 01 | 同号保留 |
| 07-pagetable-struct（15） | 07（结构）+ 08（操作） | 按语境拆分 |
| 06-page-allocator（15） | 09（自举页/池）+ 07（DM） | 按语境拆分 |
| 05-physical-memory（15） | 06 | 同构顺延 |
| 03-vmproc-table（14） | 02 | 合并 |
| 17-cow-mechanism（13） | 14 | 顺延 |
| 16-pagefault（13） | 15 | 顺延 |
| 08-pagetable-ops（12） | 08 | 同号（内容重划） |
| 26-vm-queries（12） | 24 | 顺延 |
| 04-acl（10） | 03 | 顺延 |
| 11-phys-pagestate（10） | 10 | 顺延 |
| 25-rs-services（9） | 23 | 顺延 |
| 24-page-cache（9） | 22 | 顺延 |
| 22-vm-exit（9） | 20 | 顺延 |
| 20-vm-mmap（9） | 18 | 顺延 |
| 21-vm-munmap（8） | 19 | 顺延 |
| 18-vm-fork（8） | 16 | 顺延 |
| 02-vmproc-struct（7） | 02 | 合并 |
| 00/14/09/10/99/19 等（3-7） | 00/12/09/09/99/17 | 顺延；14 并入 12 |
| `draft/NN-*.md`（14 处，13 篇） | 删除或改指新篇 | P0-process 违规，B 相清零 |
| `.design/*`（0 处正式文档命中） | 不引用 | 已合规 |

**B. 外部文档引用（其它 stage / 工程文档 → 本目录）**

| 外部位置 | 次数 | 旧目标 | 新目标 |
|---------|------|--------|--------|
| `01-stage-kernel/07-cross-space-init.md` | 14 | 07-pagetable-struct §3.2、08-pagetable-ops §3.4 | 07 §Direct Map；08 §map_kernel（保留跨阶段引用并重取小节号） |
| `01-stage-kernel/00-kernel-overview.md` | 6 | `draft/00`/`draft/26`/`draft/15` | 01（启动）、15（缺页）、00 |
| `01-stage-kernel/01-boot-shim-bootstrap.md` | 4 | `draft/06-pagetable-struct §3.4` | 07（Paging/DM 结构） |
| `01-stage-kernel/12-ipc-core.md` | 2 | `draft/24-vm-ipc-dispatch` | 04（VmReply::Suspend 模式） |
| `01-stage-kernel/02-higher-half-kernel.md` | 1 | `draft/06-pagetable-struct §1.1` | 07 |
| `00-master-plan/06-phase2-vm-guide.md` | 2 | `draft/vm-fork-mock.md` | 16（fork）或标记 defer |
| `20-redesign/tocutou-and-distributed-consistency.md` | 1 | `draft/vmproc-design.md` | 02 |
| `04-stage-pm/07-pm-fork.md` | 4 | `18-vm-fork.md` | 16 |
| `04-stage-pm/{01-pm-init-main,08-pm-srv-fork,20-misc-queries,plan,03-mproc-table}.md` | 各 1-2 | `01-vm-init-main`/`18-vm-fork`/`20-vm-exit`/`25-rs-services`/`03-vmproc-table` | 01/16/20/23/02 |
| `05-stage-vfs/{01-vfs-init-main,25-exec,plan,31-misc-queries,02-fproc-struct}.md` | 各 1-4 | `01-vm-init-main`/`20-vm-mmap` | 01/18/21 |
| `03-stage-rs/{00-rs-overview,01-rs-boot-init,02-rs-process-table,plan}.md` | 各 1-3 | `25-rs-services`/`01-vm-init-main` | 23/01 |
| `08-stage-is/{04,10}.md`、`13-stage-ipc/{02,07,08}.md`、`15-stage-fs/plan.md`、`edge_todo.md`、`prompt/todo_plan.md` | 各 1-10 | 不同 | 按上表逐条替换（B 相批量脚本） |

**C. 代码注释引用（`.rs`）**

| 位置 | 旧引用 | 新目标 |
|------|--------|--------|
| `os/servers/vm/src/vm_server.rs:53` | `02-stage-vm todo` | `25-test-observability` / `todo.md` |
| `vm_server.rs:1479`、`exit.rs:104,450` | `22-vm-exit.md §3.2/§3/D5` | 20（exit） |
| `vir_region.rs:159` | `[ARCH: A-12], 19-vm-brk.md §3.2` | 17 §A-12 |
| `munmap.rs:125` | `21-vm-munmap.md §3.6` | 19 |
| `transport.rs:33` | `Doc 24-vm-ipc-dispatch.md §3` | 04（旧 24 已是废弃名，须修正一并重指向） |
| `main.rs:53` | `doc 01-vm-init-main §3.4` | 01 |
| `boot.rs:14,145` | `01-stage-kernel/09-vm-boot-protocol` | 不变（kernel 侧编号） |
| `rs/src/boot.rs:560` | `02-stage-vm/01-vm-init-main.md §4.1` | 01 |
| `os/libs/minix-types/src/ipc/vm.rs:660` | `26-vm-queries.md` | 24 |
| `message.rs:2685` | `02-stage-vm doc 16 §3.6 #2` | 05（E-VMMCPWIRE 条目） |
| kernel 侧 `cross_space.rs/globals.rs/vm.rs/syscall.rs` | `09-vm-boot-protocol.md` | 不变（kernel 文档） |

### 8.3 断链成本摘要

- **篇间引用总量**：27 篇编号正文（00-26）+ 99 互相引用，机器统计 `NN-*.md` 文件名引用 **337 处**（另有大量裸编号引用未计入）。受影响 100%：所有引用目标都换了编号或拆篇。
- **热点**：15（28 次被引）、13（24）、12（17）、01（17）、07（15）。这五篇的引用迁移必须按语境区分目标（拆篇后一个旧编号对应两个新篇）。
- **外部引用**：其它 stage/工程文档约 **68 处**（另有 `prompt/todo_plan.md` 为工具演练引用，随工具文档更新）；其中指向 `draft/` 的 10 处必须清零。
- **代码注释**：约 **20 处**（上表）；另有 kernel 侧引用不受影响。
- **批量修改建议**：B 相完成后写一次性迁移脚本，按「先长名后短名」（`13-region-mapping.md` 先于 `13`）、「同号优先」（01/02/08/99 同号或合并号先处理）、「按语境人工复核」的次序执行；迁移脚本必须生成 diff 报告（每处旧→新），并跑 `tools/doc-code-map.sh` 与 `tools/anchor-resolve.sh` 验证。
- **旧文档归档**：整目录归档保留；外部引用在归档前完成重指向，避免指向归档文本。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**：逐篇检查契约的「前置」，全部为更早编号（00 无；01→00；02→00,01；03→00,02；04→00,02,03；05→04；06→00,05；07→00,06；08→07；09→08；10→09；11→10；12→11；13→12；14→10,11,13；15→14；16→04,05,13,14,15；17→12,13；18→05,12,13；19→13,18；20→02,13,15；21→11,13,18；22→11,21；23→09,20；24→12,13；25→00；99→无）。**结论：零前向引用。**
2. **依赖关系图检查**：以上「前置」构图（边 N→前置编号），拓扑序即编号序，**无环**。旧文档中实际存在的三处环（05↔07、06↔07、11↔13、16↔17）在新编排中消除：DM 归 07 单点、区域索引归 12、CoW 先于缺页。
3. **覆盖率检查**：§2.2 共 209 条知识点全部有唯一新归属（文档 + K 编号）；§3.2 的 14 组缺口全部落实；明确删除项在 §6 末尾单列。**结论：100% 有去向。**
4. **断链成本统计**：见 §8.3（篇间约 260 处、外部约 60 处、代码注释约 20 处、热点五篇）。

### 9.2 自检门（G1-G9）

| 门 | 结果 | 说明 |
|----|------|------|
| G1 | 通过 | C 真序表 S1-S16/L1-L11/handler 表逐条含 C 锚点；抽查十条（`main.c:480`、`main.c:131`、`sef.c:113`、`kernel/main.c:265`、`pagetable.c:1309`、`pagefaults.c:254`、`fork.c:70`、`mmap.c:512`、`exit.c:76`、`cache.c:288`）与源码一致 |
| G2 | 通过 | 24 个 `.c` 全部有归属（§5 事实底线逐篇列举）；非 C 十类全部回答（§3.5）；明确排除项 4 组有理由 |
| G3 | 通过 | 见 §9.1.1，逐篇扫描「前置」全为更早编号 |
| G4 | 通过 | 见 §9.1.2，依赖图无环；旧文档存在过的环已给拆解方案（07 单点 DM、12 索引、14 先于 15） |
| G5 | 通过 | 209 条知识点全部有去向；新增 11 条全部有证据锚点（C/制品）；明确删除项单列于 §6 末 |
| G6 | 通过 | 抽查十处：M-03（02+03→02）、M-05（15→04/05）、M-07（06→07/08/09）、M-10（09+10→09）、M-13（13→12/13）、M-15/16（17/16→14/15）、M-19（20→18/19）、M-21（22→20/13）、M-26（新建 25）、M-25（26→24）均有存量去向与新增来源 |
| G7 | 通过 | 27 份契约均含定位/讲什么/不讲什么/前置/后置/事实底线/知识点清单+验收标准 |
| G8 | 通过 | §8.1 覆盖 27 篇（整篇+跨篇节）；§8.2 覆盖篇间、外部文档与代码注释；draft/ 引用清零列入 B 相任务 |
| G9 | 通过 | 本蓝图事实断言均带锚点；唯一的推断项（旧文档「92.5%」等实施数字）标为「待验证/以 25 为口径单源」；无编造 |

### 9.3 结论与待裁决问题

- **结论**：蓝图完成，可执行。核心变化：28 篇 → 27 篇（编号篇 27 → 26，`99` 保留），编号顺序按「诞生 → 接口 → 内存地基 → 账本 → 运行机制 → 服务 → 协作 → 工程/参考」重排；机制主讲述点唯一化；非 C 主题补齐（新增 05/25 两篇承载契约与测试基建）；旧文档 52 项事实/漂移/违规问题进入 B 相首批修正清单。
- **待用户裁决**：
  1. `25-test-observability` 是否接受为正式编号篇（若倾向更薄，可并入 `99` 作为附录，但会削弱「测试基建」主题的独立性）。
  2. `05-message-contracts` 独立成篇是否接受（若倾向压缩，可并入 `04`，代价是 04 篇幅与概念混合度上升）。
  3. B 相归档时旧 27 篇是保留在原目录（`.archive/`）还是迁入现有 `02-stage-vm/archive/`（后者已有 TODO 归档）；引用迁移脚本按哪种布局生成路径。



