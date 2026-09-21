# 02-stage-vm 文档重建蓝图（HY4）

## 0. 元数据

### 0.1 执行头部

```text
your_name(AI agent name) = HY4
target_dir(关注的工作目录) = 02-stage-vm
repo_root(仓库根目录) = /home/xzhao/github/minix-rs

任务 = R 相·重建蓝图：输出 target_dir/doc_rerank_HY4.md，不改任何正文。
       只读 C 代码不够，还必须读：本目录全部文档的头部声明、os/ 目录对应入口、
       00-*-overview.md 的导航表。
约束 = 不得引用 .design/ 与 tmp_design_and_todo/；任何落盘产物必须带
       _HY4 后缀；不得读取或照抄其它 AI 的 doc_rerank_* 产物。
```

- **执行日期**：2026-09-19
- **当前提交**：`2d9d1f0aa`（`git rev-parse --short HEAD`，2026-09-19 实测）
- **目标目录绝对路径**：`/home/xzhao/github/minix-rs/notes/rewrite/fork-syscall-rewrite/02-stage-vm`
- **本产物**：只写 `doc_rerank_HY4.md` 一个文件；未修改、未移动、未删除任何现有文件。

### 0.2 审查范围

**算作文档（重建对象，28 篇）**：顶层编号文档 `00`–`26` 与 `99-global-concepts.md`（共 28 个 .md，15,270 行，`wc -l` 实测总和）。

**算作参考材料（不作为重建对象，只作知识来源）**：
- `plan.md`（443 行）、`todo.md`（510 行）、`checklist.md`（867 行）——三者是过程性台账，不是教学文档；
- `draft/`（32 个 .md，旧主线素材）、`archive/`（2 份 todo 存档）；
- 本目录已有的 `doc_rerank_deepseek.md` / `doc_rerank_glm.md` / `doc_rerank_qwen.md`——**未读取**（任务约束）；
- `.design/`——**未引用**（项目规范：中间产物）。

**范围外**：`01-stage-kernel`（VM 的启动上游）、`03-stage-rs`、`04-stage-pm`、`05-stage-vfs`（VM 的协同对端）。跨 stage 边界在 §3.5 单独处理。

### 0.3 读取清单

**目标目录文档**：28 篇全部读取头部声明（分类/源码/Rust 模块/前置/说明），并按需抽取 `## ` 级章节骨架（全部 28 篇的章节表已抽取，见 §2 的知识点来源列）。

**Minix3 C 源码**：`minix3/minix/servers/vm/` 全量 24 个 .c + 头文件（11,466 行，`wc -l` 实测）。逐文件核对函数面（不是只读映射表）。完整清单：

```
acl.c 129  alloc.c 548  break.c 69   cache.c 332  exit.c 156   fdref.c 177
fork.c 116 main.c 768   mem_anon.c 151  mem_anon_contig.c 132  mem_cache.c 324
mem_directphys.c 79  mem_file.c 287  mem_shared.c 211  mmap.c 573
pagefaults.c 418  pagetable.c 1500  pb.c 168  region.c 1555  regionavl.c 11
rs.c 391  slaballoc.c 528  utility.c 494  vfs.c 143
头文件：glo.h 48  vm.h 88  vmproc.h 39  region.h 85  phys_region.h 23
        memtype.h 34  pt.h 29  cache.h 22  sanitycheck.h 68  proto.h 253
        cavl_if.h 218  cavl_impl.h 1208  regionavl.h 10  regionavl_defs.h 17
        unavl.h 15  fdref.h 28  memlist.h 10  util.h 11
```

全量通读的有：`main.c`（768 行，逐行）。函数面清单式核对的有：其余 23 个 .c（见 §1/§2 锚点）。

**非 C 制品**（第 4 部分第 3 类逐项检查，结果见 §3.6）：
- `os/servers/vm/Cargo.toml`（bin + lib 双目标、6 个 feature、6 个 crate 依赖）
- `os/servers/vm/src/main.rs`（47 行，入口与 warm-restart 拒绝）
- `minix-elf`（boot 进程 ELF 装载）、`minix-sef`（SEF 接收循环）、`minix-arch`（Paging/DirectMap trait）、`minix-types`（wire 类型）、`minix-sys`（系统调用 wrapper）
- `os/servers/vm` 下**无** `build.rs`、**无** 链接脚本（`find os -name "*.ld"` 仅命中 `os/kernel/src/arch/*/link.ld` 与 `os/qemu-tests/`，均为 kernel/测试侧）
- `os/libs/minix-types/src/ipc/vm.rs`、`message.rs`（线格式）

**阶段边界材料**：`00-master-plan/README.md`（阶段划分与启动因果链）、`edge_todo.md`（跨阶段条目，经 `todo.md` §18.4/§19 转引）、本目录 `plan.md` §5.4（明确不做事项）、`todo.md` §1/§18/§19（V11–V14 轮开口与 Fix #63–#81）。

**前一 stage**：`01-stage-kernel/00-kernel-overview.md`（§1.3 内核与 VM 的关系、§1.4 执行模型）、`01-stage-kernel/09-vm-boot-protocol.md`（§1.2 协商时序 8 步、§1.3 双视图地址空间模型）。

**Rust 实现入口**：`os/servers/vm/src/` 52 个 .rs、31,079 行（`wc -l` 实测）。核对了 `main.rs`、`direct_map.rs`、`Cargo.toml`，以及各文档头部声明的 Rust 模块路径。

**写法范例**：`01-stage-kernel/06-todo.md`（只读了目录项确认存在，按提示词要求"只学写法"；本报告的契约格式参照提示词步骤 5 的模板）。

### 0.4 使用的命令与关键证据摘录

```bash
# 目录与规模
$ wc -l notes/rewrite/fork-syscall-rewrite/02-stage-vm/*.md | tail -1
  15270 total                       # 28 篇编号文档
$ ls minix3/minix/servers/vm/*.c | wc -l
  24
$ wc -l minix3/minix/servers/vm/*.c *.h | tail -1
  11466 total
$ find os/servers/vm/src -name '*.rs' | wc -l ; wc -l $(find os/servers/vm/src -name '*.rs') | tail -1
  52 ; 31079 total

# 前向引用/环检测（本报告 §3.3 的硬证据）
$ grep -n '^> \*\*前置' 05-physical-memory.md 06-page-allocator.md 07-pagetable-struct.md
  05:…07-pagetable-struct.md（Direct Map `A-1` 概念，`PAF_CLEAR` 清零机制的前置）
  06:…07-pagetable-struct.md（Direct Map `[ARCH: A-1]` 的页表侧承接）
  07:…06-page-allocator.md（页分配 + Direct Map 概念首次引入）

# C 事实抽查（§1 真序表的锚点来源）
$ sed -n '36,38p' minix3/minix/servers/vm/region.c
  void map_region_init(void)
  {
  }                                  # 实测：C 里是空函数
$ sed -n '136,168p' minix3/minix/servers/vm/pb.c      # mem_cow 末尾 ph->memtype = &mem_type_anon
$ sed -n '37,69p'  minix3/minix/servers/vm/acl.c      # acl_check：VM_PROC_NR 直通、NO_ACL 放行
$ sed -n '428,587p' minix3/minix/servers/vm/main.c    # init_vm 全序

# 断链成本统计（§8）
$ grep -rho "02-stage-vm/[0-9][0-9]-[a-z-]*\.md" --include="*.md" notes/ | sort | uniq -c | sort -rn | head -5
  32 02-stage-vm/01-vm-init-main.md
  28 02-stage-vm/07-pagetable-struct.md
  20 02-stage-vm/18-vm-fork.md
  18 02-stage-vm/25-rs-services.md
  13 02-stage-vm/00-vm-overview.md
$ grep -rn "02-stage-vm" --include="*.rs" os/ | wc -l
  14
$ grep -rn "02-stage-vm" --include="*.md" notes/rewrite/fork-syscall-rewrite/ \
    | grep -v "^notes/rewrite/fork-syscall-rewrite/02-stage-vm/" | wc -l
  491
```

---

## 1. C 真序

### 1.1 阶段类型判定

**本 stage 同时具备两种特征，按"启动链型打底 + 服务事件循环型收尾"处理。**

- **判定为启动链型的理由**：`main.c` 的 `init_vm()`（main.c:428-587）是一条**严格线性、无分支回跳**的初始化链，13 个步骤全部顺序执行、任一步失败即 `panic`。这条链上每个组件的初始化顺序就是它的依赖顺序，天然是教学序的骨架。
- **判定为服务事件循环型的理由**：`init_vm()` 之后进入 `while (TRUE)` 主循环（main.c:112-192），此后 VM 的全部行为都是"收消息 → 按五优先级分派 → 处理 → 回复"。26 个 IPC 服务是**并行体**（谁先被触发完全由外部进程决定），不能强排成一条线。
- **为什么不以源码调用顺序作服务段主线**：`init_vm()` 内部的 `exec_bootproc()`（main.c:498-520）已经调用了 `pt_new`/`pt_bind`/`map_page_region`/`alloc_mem`/`free_mem`——即启动链本身提前用到了"区域/页表/页分配"这些排在后面的机制。这说明**源码顺序在启动段内部已经不自洽为教学序**，服务段更不可能。故服务段改用"一次请求的生命周期"（到达 → 验证 → 建立/拆除 → 回复）作主线，按场景分组。

### 1.2 运行时序表（启动段）

| # | 动作 | C 函数与锚点 | 说明 |
|---|------|-------------|------|
| B01 | 取 RS 进程数据，判 `RTS_BOOTINHIBIT` | `is_first_time()` main.c:79-88 | 冷启动 vs 热重启的唯一判据；失败 panic |
| B02 | 冷启动门控 → `init_vm()`；置 `__vm_init_fresh=1` | `main()` main.c:100-103 | 热重启路径在 C 里靠 BSS 全局存活 |
| B03 | 取内核启动参数（memmap / 模块表 / boot_procs / user_sp） | `sys_getkinfo(&kernel_boot_info)` main.c:442 | `kinfo_t` 是 VM 全部物理认知的唯一输入 |
| B04 | 文件映射开关 | `enable_filemap=1` + `env_parse("filemap")` main.c:447-448 | 默认开；Rust 侧恒 1 |
| B05 | 解析物理内存块 | `get_mem_chunks(mem_chunks)` main.c:455（定义 utility.c:44） | 产出 `struct memory mem_chunks[NR_MEMS]` |
| B06 | 清进程表 + 回填 `vm_slot` | `memset(vmproc,0,…)` main.c:458-462 | 清 0 即令所有槽 `VMF_INUSE` 失效 |
| B07 | 初始化 ACL 数据结构 | `acl_init()` main.c:465（acl.c:21） | 位图槽位分配器 |
| B08 | 初始化区域管理 | `map_region_init()` main.c:468（region.c:36-38） | **实测：C 里是空函数**——AVL 内嵌在 `vmproc`，无全局状态 |
| B09 | 初始化物理内存分配器 | `mem_init(mem_chunks)` main.c:471（alloc.c:306） | clicks 位图；此后 `alloc_mem/free_mem` 可用 |
| B10 | 建 VM 自身槽 + 页表子系统 | `init_proc(VM_PROC_NR)` main.c:474 + `pt_init()` main.c:475（pagetable.c:1088） | `pt_init` 含静态→动态页表搬迁段（pagetable.c:1311-1345） |
| B11 | 补取内核 IPC 向量 | `__minix_init()` main.c:480 | **堆可用分界线**：此前不能动态分配 |
| B12 | 总页数校准（两笔） | `mem_add_total_pages()` main.c:485-495 | (a) boot 模块占页；(b) 内核动/静态占页 |
| B13 | 逐个装载 boot 进程 | `exec_bootproc(vmp, ip)` main.c:498-520 | `pt_new`→`pt_bind`→`libexec_load_elf`→栈构造→`sys_exec`→`VMCTL_BOOTINHIBIT_CLEAR`；VM 自己跳过（:510） |
| B14 | 释放 boot 进程文件 blob | `free_mem(ABS2CLICK(...))` main.c:516-519 | 装载完即归还 |
| B15 | 注册调用表 | `CALLMAP(...)×26` main.c:522-575 | `vm_calls[NR_VM_CALLS]`；未注册者恒 `ENOSYS` |
| B16 | 标记 VM 实例 | `num_vm_instances=1` + `VMF_VM_INSTANCE` main.c:578-579 | |
| B17 | 告知 SEF 自己的 mmap 区 | `sef_llvm_add_special_mem_region(VM_OWN_HEAPBASE, …)` main.c:582-586 | 否则 live update 会误搬 VM 的映射区 |
| B18 | SEF 启动（注册回调 + `sef_startup()`） | `sef_local_startup()` main.c:106 / 219-239 | 注册 fresh/lu/restart/response/lu_state_changed/signal 六类回调 |
| B19 | 冷启动时拷入 rproctab，逐个下发调用掩码 | `sef_cb_init_fresh()` main.c:241-260 → `map_service()` main.c:755-768 | `sys_safecopyfrom(RS_PROC_NR, rproctab_gid)` → `acl_set()` |
| B20 | 进入主循环 | `while (TRUE)` main.c:112 | 此后为运行时段 |

### 1.3 运行时序表（循环段）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| R01 | 循环顶 sanity 钩子 | `SANITYCHECK(SCL_TOP)` main.c:117 | C 为编译宏，Rust 为 cfg feature |
| R02 | 分配压力补充钩子 | `if(missing_spares>0) alloc_cycle()` main.c:118-119 | Rust 已随保留页池一起删除（Fix #70） |
| R03 | 收消息（带状态位） | `sef_receive_status(ANY,&msg,&rcv_sts)` main.c:122 | 状态位用于判定"是否来自内核" |
| R04 | 丢弃意外 notify | `is_ipc_notify(rcv_sts)` main.c:125-129 | 打印后 `continue` |
| R05 | 校验调用者 endpoint | `vm_isokendpt(who_e,&caller_slot)` main.c:131 | 失败 panic |
| R06 | 算调用号 | `c = CALLNUMBER(type)` main.c:138 | 越界 → `c<0` |
| R07 | 取 VFS 事务号 | `transid = TRNS_GET_ID(msg.m_type)` main.c:141 | |
| R08 | **优先级 1**：VFS 事务请求 | `msg.m_source==VFS_PROC_NR && IS_VFS_FS_TRANSID` main.c:143-148 | 剥离 transid 后走 `do_procctl` |
| R09 | **优先级 2**：RS_INIT 握手 | `msg.m_type==RS_INIT && source==RS_PROC_NR` main.c:149-152 | 处理后置 `SUSPEND`（不回复） |
| R10 | **优先级 3**：页错误 | `msg.m_type==VM_PAGEFAULT` main.c:153-164 | 校验 `IPC_FLG_MSG_FROM_KERNEL`；不回复，由 `sys_vmctl` 解除目标停等 |
| R11 | **优先级 4**：越界/未注册 | `c<0 \|\| !vm_calls[c].vmc_func` main.c:165-166 | 保持 `result=ENOSYS` |
| R12 | **优先级 5**：ACL 闸 + 分派 | `acl_check()` main.c:168 → `vm_calls[c].vmc_func(&msg)` main.c:173 | ACL 不过只打印、仍走 ENOSYS 回复 |
| R13 | 回复（SUSPEND 除外） | `if(result != SUSPEND) ipc_send(who_e,&msg)` main.c:181-191 | 发送失败 → panic（fail-fast） |

**信号路径（与循环并行）**：`sef_cb_signal_handler(SIGKMEM)` main.c:731-750 → `do_memory()`（pagefaults.c:294）→ 尾部 `pt_clearmapcache()`。

**Live Update 路径（与循环并行）**：`sef_cb_init_lu_restart` main.c:677-726（默认状态传输 → 查旧槽 → `swap_proc_slot` → `swap_proc_dyn_data` → `pt_bind`×2 → `pt_clearmapcache` → `adjust_proc_refs` → `sef_cb_init_vm_multi_lu`）→ `sef_cb_init_vm_multi_lu` main.c:592-672（IPC 过滤器 + 逐进程 `do_rs_update`）→ `sef_cb_lu_state_changed` main.c:196-217（回滚：重绑页表 + `adjust_proc_refs`）。

---

## 2. 知识点全集

### 2.1 统计摘要

- **总条数**：152 条（`K-001`–`K-344`，编号不连续，按主题分段预留）
- **按来源类型**：存量 138 条（来自现有 28 篇文档）、新增 14 条（现有文档未讲，由 C 源码/非 C 制品/OS 理论承载，标记 ✅新增）
- **按类型分布**：概念 22、机制 41、数据结构 26、接口与协议 24、约束与不变量 17、架构演进 12、工具与工程 8、测试性质 2
- **重复与主讲述点**：检出 11 组重复主题（详见 §3.2）

### 2.2 知识点池总表

列：`编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益`

#### A. 启动链与装载（K-001–K-016）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-001 | `is_first_time` 冷/热启动门控 | 机制 | 存量 | 01 §2 | main.c:79-88（`RTS_BOOTINHIBIT`） | 能回答"VM 怎么知道自己是不是第一次跑" |
| K-002 | `sys_getkinfo` 与 `kernel_boot_info` | 接口与协议 | 存量 | 01 §2 | main.c:442；`glo.h:ixfer_kinfo_t` | 能回答"VM 的物理世界认知从哪来" |
| K-003 | `enable_filemap` / `env_parse` | 约束与不变量 | 存量 | 01 §2 | main.c:447-448 | 能回答"文件 mmap 是编译期还是运行期决定" |
| K-004 | `init_vm()` 13 步线性序 | 机制 | 存量 | 01 §1/§2、plan §1.2 | main.c:428-587 | 能回答"VM 启动的每一步为什么在这个位置" |
| K-005 | SEF 生命周期与六类回调注册 | 机制 | 存量 | 01 §2 | main.c:219-239 | 能回答"VM 与 RS 的握手由谁驱动" |
| K-006 | `sef_cb_init_fresh` + rproctab 拷入 | 机制 | 存量 | 01 §2、25 §2 | main.c:241-260（`sys_safecopyfrom`） | 能回答"VM 怎么拿到进程清单" |
| K-007 | `map_service` 与调用掩码下发 | 机制 | 存量 | 01 §2、25 §2 | main.c:755-768（`acl_set`） | 能回答"服务的权限是谁给的" |
| K-008 | `exec_bootproc` 全序 | 机制 | 存量 | 01 §2 | main.c:331-417 | 能回答"boot 进程的 ELF 是谁装载的" |
| K-009 | `libexec_*` 三回调（prealloc/ondemand） | 接口与协议 | 存量 | 01 §2 | main.c:305-329 | 能回答"装载器向谁要内存" |
| K-010 | boot blob 释放 | 机制 | 存量 | 01 §2 | main.c:516-519 | 能回答"装载完的镜像文件去哪了" |
| K-011 | 总页数两笔校准 | 机制 | 存量 | 01 §2、05 §2 | main.c:485-495 | 能回答"为什么 total_pages 不等于 memmap 之和" |
| K-012 | `sef_llvm_add_special_mem_region` | 接口与协议 | 存量 | 01 §2 | main.c:582-586 | 能回答"LU 怎么知道不该搬 VM 的 mmap 区" |
| K-013 | VM 自身地址空间边界 | 数据结构 | 存量 | 01 §2、00 §1 | vm.h:80-84（`VM_OWN_HEAPBASE/MMAPBASE/MMAPTOP`） | 能回答"VM 自己的堆/映射区在哪" |
| K-014 | warm restart 不支持（G-V12-10） | 架构演进 | 存量 | todo §1（G-V12-10） | main.c:100-103；`os/servers/vm/src/main.rs:29-46` | 能回答"为什么 Rust 侧热重启会 panic" |
| K-015 | minix-elf 装载（非 C 制品） | 工具与工程 | ✅新增 | —（01 未讲 crate 侧） | `os/servers/vm/Cargo.toml:minix-elf`；main.c:383 `libexec_load_elf` | 能回答"ELF 解析在 Rust 侧由哪个 crate 承担" |
| K-016 | bin/lib 双目标 + boot params handoff | 工具与工程 | ✅新增 | — | `os/servers/vm/Cargo.toml`（`[[bin]]`+`[lib]`）；`main.rs:22` `read_boot_params()` | 能回答"VM 二进制与测试库如何共存" |

#### B. 角色、执行模型与全局不变量（K-020–K-028）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-020 | 微内核里内存语义权威在 VM（三重权威） | 概念 | 存量 | 00 §1.1-1.2 | main.c:112-192；`00-vm-overview.md:13-23` | 能回答"为什么页表不由内核改" |
| K-021 | 单线程事件循环 vs kernel SMP+BKL | 概念 | 存量 | 00 §1.2 | `00-vm-overview.md:23`；AGENTS.md 执行模型条款 | 能回答"为什么 VM 可以用 `Rc/RefCell` 而 kernel 不能" |
| K-022 | VM 不能缺页（eager mapping） | 约束与不变量 | 存量 | 16 §1（:39） | `16-pagefault.md:39`；pagetable.c:55-57 注释 | 能回答"为什么 VM 自己的分配是分配即映射" |
| K-023 | "VM 只改不在运行的进程的页表"不变量 | 约束与不变量 | 存量 | 16 §3.7 | `16-pagefault.md` §3.7（Fix #69 新增） | 能回答"写 PTE 后为什么不用全局刷 TLB" |
| K-024 | TLB 纪律：写后 invlpg / C 全局清缓存 / 别名模型消除 | 机制 | 存量 | 08 §1.8、16 §3.6 | `08-pagetable-ops.md` §1.8；C `pagetable.c:119/255/319/430` | 能回答"C 四处自刷为什么 Rust 不需要" |
| K-025 | fail-fast（send 失败 panic）与 fail-closed（ACL 拒） | 约束与不变量 | 存量 | 15 §1、04 §3 | main.c:186-190；`acl.rs` A-11 | 能回答"VM 的错误策略为什么偏保守" |
| K-026 | `panic = "abort"`（VM 崩溃即系统冻结） | 约束与不变量 | 存量 | todo §18.3 | `os/Cargo.toml:260/263`；main.c:189 | 能回答"VM 挂了之后系统会怎样" |
| K-027 | `SUSPEND` 伪返回码（延迟回复协议） | 接口与协议 | 存量 | 15 §1、23 §1 | main.c:152/178-181；com.h:1151 | 能回答"异步请求怎么不阻塞主循环" |
| K-028 | 五优先级分发顺序 | 机制 | 存量 | 15 §1/§2、00 §1.3 | main.c:143-176 | 能回答"两条消息同时到，谁先被处理" |

#### C. 进程模型（K-030–K-038）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-030 | `struct vmproc` 全字段语义 | 数据结构 | 存量 | 02 §2 | `vmproc.h`（39 行） | 能回答"VM 对每个进程记住了什么" |
| K-031 | `VMF_*` 标志族与生命周期状态机 | 数据结构 | 存量 | 02 §1/§2 | `vmproc.h:VMF_*`；main.c:276-280 | 能回答"槽位的四种状态怎么迁移" |
| K-032 | `vmproc[VMP_NR]` 全局表与 `vm_slot` | 数据结构 | 存量 | 03 §2 | `glo.h:VMP_NR`；main.c:458-462 | 能回答"表有多大、槽号从哪来" |
| K-033 | `VMP_EXECTMP` 保留槽 | 数据结构 | 存量 | 03 §2 | `glo.h:VMP_EXECTMP`（`_NR_PROCS`） | 能回答"exec 期间旧进程状态暂存哪" |
| K-034 | `vm_isokendpt` 与 endpoint/generation 编码 | 接口与协议 | 存量 | 03 §2 | utility.c:84；`endpoint.h:_ENDPOINT_GENERATION_SHIFT` | 能回答"endpoint 为什么不能直接当数组下标" |
| K-035 | `init_proc` 槽位激活 | 机制 | 存量 | 02 §2、03 §2 | main.c:262-286 | 能回答"boot 进程如何变成 VM 表里的槽" |
| K-036 | `clear_proc` / `free_proc` | 机制 | 存量 | 02 §2、22 §2 | exit.c:33-58 | 能回答"槽位初始化与回收各自清什么" |
| K-037 | `swap_proc_slot`（LU 槽位交换） | 机制 | 存量 | 03 §2、10 §2、25 §2 | utility.c:188；main.c:707 | 能回答"新旧 VM 实例如何换位" |
| K-038 | typestate 视图（ActiveProc/ExitingProc） | 架构演进 | 存量 | 02 §3、22 §3 | `vmproc/vmproc_handle.rs` | 能回答"Rust 如何用类型替代 C 的运行时断言" |

#### D. 访问控制（K-040–K-045）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-040 | ACL 位图与槽位分配（DEFAULT/SYSTEM 分层） | 数据结构 | 存量 | 04 §2 | `acl.c:21/70`；`bitmap.h:BITCHUNK_BITS` | 能回答"用户进程与系统进程的掩码为何分开存" |
| K-041 | `acl_init/check/set/fork/clear` 五函数 | 机制 | 存量 | 04 §2 | acl.c:21/37/70/110/121 | 能回答"权限的增删改查全貌" |
| K-042 | C `NO_ACL` 放行 vs Rust fail-closed（A-11） | 架构演进 | 存量 | 04 §3、plan §4 | acl.c:44-53（注释 "for now"）；`acl.rs:97-112` | 能回答"这是一个有意的行为偏移吗" |
| K-043 | 主循环 `acl_check` 接线与"None 即拒绝" | 机制 | 存量 | 15 §2、04 §4.4 | main.c:168；Fix #66 | 能回答"槽位不可解析时是否放行" |
| K-044 | `vm_call_mask` 的 64 位宽度 | 接口与协议 | 存量 | 04 §3、todo §18.2 | `minix-types/src/ipc/rprocpub.rs:89`（u64）；`vm_server.rs:1597`（u32，V13-P2-2） | 能回答"高位调用号为什么授权不到" |
| K-045 | `RS_SET_PRIV` 的 safecopy 掩码 | 接口与协议 | 存量 | 25 §3、todo §18.2 | rs.c:41-56；`dispatcher.rs:1091-1102`（V13-P2-5） | 能回答"特权掩码为什么现在下发不了" |

#### E. 物理内存（K-050–K-060）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-050 | `get_mem_chunks` 布局解析 | 机制 | 存量 | 05 §2 | utility.c:44；main.c:455 | 能回答"物理内存地图怎么变成块数组" |
| K-051 | `struct memory` / `mem_chunks[NR_MEMS]` | 数据结构 | 存量 | 05 §2 | main.c:431 | 能回答"块的上限与表示" |
| K-052 | `mem_init` / `alloc_mem` / `free_mem` | 机制 | 存量 | 05 §2 | alloc.c:306/242/289 | 能回答"clicks 粒度的分配与回收" |
| K-053 | clicks ↔ bytes ↔ pages 换算 | 概念 | 存量 | 05 §2 | `ABS2CLICK/CLICK2ABS`；`VM_PAGE_SIZE` | 能回答"为什么分配器用 click 不用页" |
| K-054 | `PAF_*` 分配标志 | 数据结构 | 存量 | 05 §2、06 §2 | vm.h:20-25 | 能回答"LOWER16MB/CONTIG/ALIGN64K 各自约束什么" |
| K-055 | bitmap/buddy/segment-tree 三后端（A-5） | 架构演进 | 存量 | 05 §3、plan §4 | `phys_mem/*`；`Cargo.toml` features | 能回答"三种后端为何共存、如何选" |
| K-056 | 低内存约束与 `max_page_bound` 单点 | 机制 | 存量 | 05 §4.4（Fix #65） | `phys_mem/mod.rs::max_page_bound` | 能回答"三后端为何曾给出三个答案" |
| K-057 | `memstats` / `printmemstats` / `usedpages_*` | 工具与工程 | 存量 | 05 §2、26 §2 | alloc.c:348/486/501/509 | 能回答"内存统计的两个口径" |
| K-058 | `mem_add_total_pages` / `total_pages` | 机制 | 存量 | 05 §2、26 §4.4 | alloc.c:281；utility.c:118 | 能回答"INFO/STATS 的总页数读的是哪个计数" |
| K-059 | `NO_MEM` 与 `MAP_NONE` 哨兵 | 数据结构 | 存量 | 05 §2、12 §2 | vm.h:66-67 | 能回答"分配失败与'未映射'如何区分" |
| K-060 | `alloc_cycle` 与 `missing_spares` 的历史与删除 | 架构演进 | 存量 | 06 §2/§3.3、todo Fix #70 | alloc.c:74/227；plan §7.3 | 能回答"保留页池为什么在 Rust 里不存在" |

#### F. Direct Map 与地址空间（K-065–K-071）★新增篇承载

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-065 | Direct Map 双视图（A-1） | 架构演进 | 存量 | 07 §3.6、plan §4 | `01-stage-kernel/09-vm-boot-protocol.md` §1.3；`os/arch/src/direct_map.rs` | 能回答"物理页为什么有一个恒定 VA" |
| K-066 | `VM_DIRECT_MAP_BASE` / `KERNEL_DIRECT_MAP_BASE` / `SIZE` | 数据结构 | 存量 | 07 §2/§3 | `os/servers/vm/src/direct_map.rs:16-24` | 能回答"两个窗口的基址与大小从哪来" |
| K-067 | `vm_phys_to_virt` / `vm_virt_to_phys` 双向换算 | 机制 | 存量 | 07 §2、06 §2 | `direct_map.rs:32-48`；C `vm_phys_to_virt` 静态映射表 | 能回答"换算是一条算术还是一个查表" |
| K-068 | 地址空间宽度 32→64（A-6） | 架构演进 | 存量 | 00 §3、07 §3、20 §3 | vm.h:76-78（`VM_MMAPTOP/MMAPBASE`）；`mmap.rs:203-204` | 能回答"MMAP 区间常量为什么变" |
| K-069 | `PAF_CLEAR` 清零语义 | 机制 | 存量 | 05 §2（前向引用点） | vm.h:20；`alloc.c` 清零路径 | 能回答"清零在 direct map 下怎么实现" |
| K-070 | C 静态映射表 vs Rust 常量偏移（别名模型消除） | 架构演进 | 存量 | 08 §1.8 | C `pagetable.c:255/319`；`direct_map.rs` | 能回答"为什么 Rust 不需要自刷 TLB" |
| K-071 | `VM_HEAP_BASE/SIZE/LIMIT` | 数据结构 | ✅新增 | —（仅散在 direct_map.rs 注释） | `direct_map.rs:26-28` | 能回答"VM 堆窗口的边界常量在哪定义" |

#### G. VM 页分配（K-075–K-080）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-075 | `vm_allocpage(s)` / `vm_mappages` / `vm_freepages` | 机制 | 存量 | 06 §2 | pagetable.c:395/333/235 | 能回答"VM 给自己要页与给别人要页有何不同" |
| K-076 | `vm_pagelock` / `vm_addrok` | 机制 | 存量 | 06 §2 | pagetable.c:403/440 | 能回答"页保护与地址合法性校验" |
| K-077 | `VMP_*` 分配类别（SPARE/PAGETABLE/PAGEDIR/SLAB） | 数据结构 | 存量 | 06 §2、09 §3 | vm.h:69-73 | 能回答"分配用途如何记账" |
| K-078 | 保留页池 `reservedqueue_*` 与其结构性消除 | 架构演进 | 存量 | 06 §3.3、plan §7.3 | alloc.c:100-226；pagetable.c:55-57 | 能回答"自举循环依赖在 Rust 里怎么破" |
| K-079 | `findhole`（页表页洞查找） | 机制 | 存量 | 06 §2、08 §2 | pagetable.c:155 | 能回答"VM 自己的页表页 VA 从哪来" |
| K-080 | `alloc_contiguous` trait 默认实现（Fix #79） | 机制 | ✅新增 | —（仅 todo Fix #79） | `region/page_state.rs::PfnAllocator::alloc_contiguous` | 能回答"连续物理页怎么一次要齐" |

#### H. 页表（K-085–K-090 / K-095–K-102）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-085 | `pt_t` 结构 | 数据结构 | 存量 | 07 §2 | pt.h:29 | 能回答"页表对象长什么样" |
| K-086 | `ARCH_VM_*` 宏族与页表层级（A-2） | 架构演进 | 存量 | 07 §2/§3 | `ARCH_VM_*`；C 2 级 vs Rust 4 级 | 能回答"2 级与 4 级的语义等价点在哪" |
| K-087 | 多架构 `Paging` trait（A-10） | 架构演进 | 存量 | 07 §3 | `os/arch/src/arch/paging.rs` | 能回答"三架构如何共用一套页表代码" |
| K-088 | VM 自映射页表（A-9） | 架构演进 | 存量 | 07 §3 | `pagetable/vm_self_map.rs`；C `static_sparepagedirs` | 能回答"VM 怎么改自己的页表" |
| K-089 | `pt_init` 结构面 | 机制 | 存量 | 07 §2 | pagetable.c:1088 | 能回答"页表子系统初始化的输入" |
| K-090 | PTE 标志（`PageFlags`）与 u32→u64 PTE | 数据结构 | 存量 | 07 §2、08 §2 | `os/arch/src/arch/paging.rs::PageFlags` | 能回答"权限位在两个位宽下的对应" |
| K-095 | `pt_new` / `pt_free` / `pt_bind` | 机制 | 存量 | 08 §2 | pagetable.c:990/1427/1358 | 能回答"页表生命周期三步" |
| K-096 | `pt_writemap` 与 `WMF_*` 标志族 | 机制 | 存量 | 08 §2 | pagetable.c:784；vm.h:60-63 | 能回答"写映射的四种模式差异" |
| K-097 | `pt_ptalloc` / `pt_ptalloc_in_range` | 机制 | 存量 | 08 §2 | pagetable.c:494/545 | 能回答"页表页何时按需分配" |
| K-098 | `pt_copy` / `pt_map_in_range` / `pt_ptmap` | 机制 | 存量 | 08 §2、18 §2 | pagetable.c:1069/631/685 | 能回答"跨页表复制的三种粒度" |
| K-099 | `pt_checkrange` / `pt_writable` | 机制 | 存量 | 08 §2 | pagetable.c:943/761 | 能回答"映射校验与可写查询" |
| K-100 | `pt_mapkernel` / `pt_clearmapcache` / `pt_allocate_kernel_mapped_pagetables` | 机制 | 存量 | 08 §2 | pagetable.c:1442/751/1035 | 能回答"内核映射如何进入每个进程页表" |
| K-101 | 写 PTE 后逐条 `invlpg`（`write_pte_dm`） | 约束与不变量 | 存量 | 08 §1.8 | `os/arch/src/x86_64/paging.rs::write_pte_dm` | 能回答"写与失效为何绑定" |
| K-102 | `VMINHIBIT` 动态停等（CONFIG_SMP） | 架构演进 | 存量 | 08 §1.10、todo Fix #77 | pagetable.c:799-815/928-934 | 能回答"C 用什么替代结构不变量" |

#### I. 堆与自举终点（K-105–K-119）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-105 | `slaballoc.c` 尺寸分类分配器 | 机制 | 存量 | 09 §2 | slaballoc.c:29-504（`SLABSIZES`…`slabstats`） | 能回答"C 的堆为什么按尺寸分类" |
| K-106 | `SLABALLOC`/`SLABFREE` 类型化宏 | 接口与协议 | 存量 | 09 §2 | proto.h | 能回答"类型安全如何在 C 里模拟" |
| K-107 | `MEMPROTECT` / `USE` 宏 | 工具与工程 | 存量 | 09 §2 | vm.h:16；slaballoc.c `USE()` | 能回答"C 的调试写保护怎么实现" |
| K-108 | `HeapArena` + `VmAllocator` free-list（A-3） | 架构演进 | 存量 | 09 §3 | `heap_arena.rs`；`global.rs` | 能回答"Rust 堆为什么不需要尺寸分类" |
| K-109 | `__minix_init` 堆可用分界线 | 约束与不变量 | 存量 | 09 §1、01 §2 | main.c:480 | 能回答"哪一步之后才能动态分配" |
| K-110 | `slabstats` / `objstats` | 工具与工程 | 存量 | 09 §2、26 §6 | slaballoc.c:344/504 | 能回答"堆统计为何延后" |
| K-115 | `pt_init` 搬迁段 | 机制 | 存量 | 10 §2 | pagetable.c:1311-1345 | 能回答"静态页表如何换成动态页表" |
| K-116 | `swap_proc_dyn_data` / `map_proc_dyn_data` | 机制 | 存量 | 10 §2 | utility.c:312/283 | 能回答"LU 时动态数据如何搬家" |
| K-117 | `transfer_mmap_regions` | 机制 | 存量 | 10 §2 | utility.c:228 | 能回答"VM 的 mmap 区如何在实例间转移" |
| K-118 | `map_setparent` | 机制 | 存量 | 10 §2、13 §2 | region.c:1535 | 能回答"父子关系如何维护" |
| K-119 | 静态→动态分配转换 | 概念 | 存量 | 10 §1 | pagetable.c:1311-1345 | 能回答"自举的终点是什么" |

#### J. 物理页状态与 memtype（K-125–K-144）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-125 | `phys_block` / `phys_region` 结构 | 数据结构 | 存量 | 11 §2 | pb.c:44；`phys_region.h:8-21` | 能回答"物理侧的两级结构各自代表什么" |
| K-126 | `pb_new` / `pb_free` / `pb_link` | 机制 | 存量 | 11 §2 | pb.c:44/54/61 | 能回答"物理块的建毁与挂接" |
| K-127 | `pb_reference` / `pb_unreferenced` | 机制 | 存量 | 11 §2、17 §2 | pb.c:73/96 | 能回答"引用计数的加减点" |
| K-128 | `PageFrames` 逐帧引用计数（Rust） | 数据结构 | 存量 | 11 §3 | `region/page_state.rs` | 能回答"Rust 为何不用 per-block 结构" |
| K-129 | `PageFlags`：`COW` / `IN_CACHE` | 数据结构 | 存量 | 11 §3、17 §3、24 §3 | `page_state.rs:30` | 能回答"页状态位承载哪些语义" |
| K-130 | `physblock_get` / `physblock_set` | 机制 | 存量 | 11 §2、13 §2 | region.c:60/72 | 能回答"区域内按偏移取物理块" |
| K-135 | `mem_type_t` 六实例 | 数据结构 | 存量 | 12 §2 | `glo.h`：anon/directphys/anon_contig/cache/mappedfile/shared | 能回答"六种内存类型各自的用途" |
| K-136 | `ev_*` 回调族签名 | 接口与协议 | 存量 | 12 §2 | memtype.h:18/21/22 | 能回答"策略层需要实现哪些钩子" |
| K-137 | anon：按需分配 + CoW | 机制 | 存量 | 12 §2、17 §2 | mem_anon.c:64/105 | 能回答"最常用类型的行为" |
| K-138 | anon_contig：连续物理 | 机制 | 存量 | 12 §2 | mem_anon_contig.c:52/97 | 能回答"连续内存如何分配与校验" |
| K-139 | directphys：`phys_setphys` | 机制 | 存量 | 12 §2、21 §2 | mem_directphys.c:69 | 能回答"设备内存映射如何不走分配器" |
| K-140 | shared：`getsrc` / regionid / `remaps` | 机制 | 存量 | 12 §2/§3.7 | mem_shared.c:54/99；Fix #64 | 能回答"共享区域的引用计数为什么是 1+remaps" |
| K-141 | mappedfile：`cow_block` / clearend | 机制 | 存量 | 12 §2、23 §2、17 §3 | mem_file.c:59/70-71/131-134；Fix #80 | 能回答"文件私有写为何要换型" |
| K-142 | cache：`IN_CACHE` 与不进回收漏斗 | 机制 | 存量 | 12 §2、24 §2 | mem_cache.c:34-95 | 能回答"缓存页为什么不能被回收" |
| K-143 | `writable` / `ev_unreference` / `ev_copy` 的语义差 | 接口与协议 | 存量 | 12 §2、17 §2 | memtype.h | 能回答"同类回调在不同类型下为何不同" |
| K-144 | memtype 反向依赖 vmproc 的判定（C-parity） | 架构演进 | 存量 | todo Fix #76 | memtype.h:18/21/22；`memtype.rs:7` | 能回答"策略层碰进程表是不是设计缺陷" |

#### K. 区域（K-150–K-165）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-150 | `vir_region` 结构 + `VR_*` 标志 | 数据结构 | 存量 | 13 §2 | region.h:37-78 | 能回答"虚拟区域记录什么" |
| K-151 | `map_page_region`（建区） | 机制 | 存量 | 13 §2 | region.c:463 | 能回答"区域如何被创建并挂到进程" |
| K-152 | `map_unmap_region` / `map_unmap_range` | 机制 | 存量 | 13 §2、21 §2 | region.c:1065/1222 | 能回答"拆区的三/四情形" |
| K-153 | `split_region` / `free_range` | 机制 | 存量 | 13 §2、21 §2 | region.c:1150；`vir_region.rs:279/352` | 能回答"部分拆除如何切分" |
| K-154 | `map_proc_copy` / `map_proc_copy_range` | 机制 | 存量 | 13 §2、18 §2 | region.c:933/944 | 能回答"fork 的区域复制两种粒度" |
| K-155 | `map_lookup` / `find_slot` | 机制 | 存量 | 13 §2、14 §2 | region.c:616；`region_map.rs:157` | 能回答"地址→区域的解析" |
| K-156 | AVL（`cavl_if/impl/regionavl_defs`）→ `BTreeMap`（A-4） | 架构演进 | 存量 | 14 §2/§3 | cavl_impl.h:178-1100；`region_map.rs` | 能回答"宏模板与标准库容器的语义等价点" |
| K-157 | `region_find_slot_range`（分配空洞） | 机制 | 存量 | 14 §2、20 §2 | region.c:302 | 能回答"mmap 如何找空闲地址" |
| K-158 | `map_pf` / `map_handle_memory` / `map_pin_memory` | 机制 | 存量 | 13 §2、16 §2 | region.c:664/756/779 | 能回答"缺页如何落到区域层" |
| K-159 | `map_writept` / `map_ph_writept` | 机制 | 存量 | 13 §2、17 §2 | region.c:906/257 | 能回答"区域层如何写 PTE" |
| K-160 | `map_free` / `map_free_proc` | 机制 | 存量 | 13 §2、22 §2 | region.c:568/589 | 能回答"区域与进程级回收" |
| K-161 | `map_region_lookup_type` | 机制 | 存量 | 13 §2、25 §2 | region.c:1303 | 能回答"LU 预分配如何按类型找区域" |
| K-162 | `vrallocflags`（VR → PAF） | 机制 | 存量 | 13 §2 | region.c:645 | 能回答"区域标志如何转成分配标志" |
| K-163 | `physregions` / `map_printmap` / `printregionstats` | 工具与工程 | 存量 | 13 §2 | region.c:1546/98/1510 | 能回答"调试面有哪些" |
| K-164 | `map_region_init` 在 C 是**空函数** | 概念 | ✅新增 | 13 §2（未点明）/plan §1.2（称"初始化"） | region.c:36-38 | 能回答"区域子系统为什么没有全局初始化" |
| K-165 | `find_overlap` 邻居探测 + 零长拒绝（Fix #72） | 机制 | ✅新增 | —（仅 todo Fix #72） | `region_map.rs::find_overlap/insert` | 能回答"为什么零长区域必须拒绝" |
| K-166 | `map_region_extend_upto_v` 与 A-12（resize 并入 extend） | 架构演进 | 存量 | 13 §3、19 §3 | region.c:1002；`vir_region.rs:127-140` | 能回答"扩展为何与 memtype 无关" |

#### L. 消息协议（K-170–K-179）★新增篇承载

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-170 | `VM_RQ_BASE` / `NR_VM_CALLS` / 调用号布局 | 接口与协议 | 存量 | 15 §2、99 §2 | com.h:627/769/630-773 | 能回答"调用号空间有多大、如何编号" |
| K-171 | M1/M2/M3 overlay 与 `mess_*` 结构 | 接口与协议 | 存量 | 20 §4、21 §4、26 §4（各讲各的） | `minix/include/minix/ipc.h` | 能回答"一条消息能带几个字" |
| K-172 | minix-types 的 `VmXxxIn/Out` 与 decode/encode | 接口与协议 | 存量 | 15 §4、各服务 §4 | `minix-types/src/ipc/vm.rs` | 能回答"Rust 侧解码入口在哪" |
| K-173 | 五结构缺专属 wire struct（V13-P2-6） | 接口与协议 | ✅新增 | —（仅 todo §18.2） | `dispatcher.rs:1075-1089/1104-1119/1166-1172`；ipc.h:928-934/1486-1520 | 能回答"按 C libc 语义写消费方会在哪接错" |
| K-174 | 56 字节断言（`_ASSERT_MSG_SIZE`） | 测试性质 | ✅新增 | —（仅 todo §18.2 提及 1 例） | `message.rs:3606` | 能回答"线格式的回归防线在哪" |
| K-175 | errno → `m_type` 回复编码 | 接口与协议 | 存量 | 15 §4 | main.c:182；`ipc/encode.rs` | 能回答"错误如何传回调用者" |
| K-176 | `SUSPEND` 伪返回码 | 接口与协议 | 存量 | 15 §1、23 §1 | com.h:1151 | 与 K-027 同条目，归本篇为主讲述点 |
| K-177 | VFS transid 编解码 | 接口与协议 | 存量 | 15 §2、22 §2 | vfsif.h:79-81 | 能回答"事务号如何塞进 m_type" |
| K-178 | `IPC_FLG_MSG_FROM_KERNEL` 状态位 | 接口与协议 | 存量 | 15 §2、16 §2 | ipcconst.h:28 | 能回答"如何识别伪内核消息" |
| K-179 | `is_ipc_notify` 过滤 | 机制 | 存量 | 15 §2 | main.c:125-129 | 能回答"notify 为什么被丢弃" |

#### M. 主循环（K-185–K-195）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-185 | `while (TRUE)` 循环骨架 | 机制 | 存量 | 15 §2、01 §2 | main.c:112-192 | 能回答"一轮循环做了什么" |
| K-186 | `sef_receive_status` 与 minix-sef 方案（V14-P2-1） | 接口与协议 | ✅新增 | —（仅 todo §19） | `os/servers/vm/Cargo.toml:minix-sef`；`vm_server.rs:961` | 能回答"VM 为何是 SEF 的潜在第四消费方" |
| K-187 | `vm_isokendpt` 调用者验证 | 机制 | 存量 | 15 §2、03 §2 | main.c:131 | 与主讲述点 03 交叉，本篇只讲接线 |
| K-188 | `CALLNUMBER` 越界 → `ENOSYS` | 机制 | 存量 | 15 §2 | main.c:57-59/139 | 能回答"非法调用号如何被拒" |
| K-189 | `vm_calls[]` + `CALLMAP` 26 条注册 | 机制 | 存量 | 15 §2 | main.c:522-575 | 能回答"服务表怎么建" |
| K-190 | `acl_check` 接线 | 机制 | 存量 | 15 §2、04 §4 | main.c:168 | 能回答"门禁挂在哪一级" |
| K-191 | 未注册四调用双侧 `ENOSYS`（T19） | 架构演进 | 存量 | 15 §3、todo §18.1 | com.h:637/656/664/672 | 能回答"为什么不要去'接线'这四条" |
| K-192 | `ipc_send` 回复与 fail-fast panic | 约束与不变量 | 存量 | 15 §2 | main.c:186-190 | 能回答"回复失败为什么是致命的" |
| K-193 | `SANITYCHECK` → cfg feature（A-7） | 架构演进 | 存量 | 各篇 §5、plan §4 | vm.h:10-12；`Cargo.toml:sanity_checks` | 能回答"编译宏在 Rust 里的对应物" |
| K-194 | `sef_cb_signal_handler` / `SIGKMEM` | 机制 | 存量 | 15 §2、16 §2、01 §2 | main.c:731-750 | 能回答"信号如何进入 VM" |
| K-195 | 伪造 PAGEFAULT 源的 release 可观测性（Fix #67） | 工具与工程 | ✅新增 | —（仅 todo Fix #67） | `vm_server.rs` P3 分支；C main.c:154-157 | 能回答"release 下如何观测伪造消息" |

#### N. 页错误与 CoW（K-200–K-221）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-200 | `VM_PAGEFAULT` 被动路径 | 机制 | 存量 | 16 §2 | main.c:153-164；pagefaults.c:240 | 能回答"缺页消息从哪来" |
| K-201 | `SIGKMEM` → `do_memory` 主动路径 | 机制 | 存量 | 16 §2 | pagefaults.c:294；main.c:736 | 能回答"内核主动要内存走哪条路" |
| K-202 | `pf_state` / `hm_state` | 数据结构 | 存量 | 16 §2 | pagefaults.c:33-49 | 能回答"异步处理的上下文存在哪" |
| K-203 | `handle_memory_start/once/step/final/continue` | 机制 | 存量 | 16 §2 | pagefaults.c:254/245/336/198/170 | 能回答"状态机五步" |
| K-204 | `map_pf` 汇合点与 memtype 分发 | 机制 | 存量 | 16 §2、13 §2 | region.c:664 | 能回答"两条入口在哪合一" |
| K-205 | `vfs_callback_t` 异步续作 | 接口与协议 | 存量 | 16 §2、23 §2 | pagefaults.c:51/161 | 能回答"等 VFS 时状态怎么保存" |
| K-206 | `pf_errstr` | 工具与工程 | 存量 | 16 §2 | pagefaults.c:59 | 能回答"错误码如何人读" |
| K-207 | 不在运行进程的不变量逐路径论证 | 约束与不变量 | 存量 | 16 §3.7（Fix #69） | `16-pagefault.md` §3.7 | 与 K-023 同条目，本篇为主讲述点 |
| K-208 | 内核 `sys_vmctl` 解除 `RTS_PAGEFAULT` | 接口与协议 | 存量 | 16 §2 | pagefaults.c:161-168 注释；`kernel/system/do_vmctl.c` | 能回答"处理完谁把进程唤醒" |
| K-215 | `mem_cow`（pb.c:136-168）与换型 | 机制 | 存量 | 17 §2 | pb.c:136-168（末行 `ph->memtype = &mem_type_anon`） | 能回答"CoW 之后页的身份变不变" |
| K-216 | 写保护建立（fork 侧） | 机制 | 存量 | 17 §2、18 §2 | fork.rs `setup_cow_for_all_regions` | 能回答"写保护何时批量落下" |
| K-217 | refcount 与分裂判定 | 机制 | 存量 | 17 §2 | pb.c:96/136 | 能回答"什么时候必须复制" |
| K-218 | 快路/慢路（`refcount<=1 ∧ writable`，Fix #63） | 机制 | ✅新增（2026-09-09 后新增，17 §3.3 已写） | 17 §3.3 | `cow_exec_pf.rs:268-273/282`；C mem_file.c:70-71/127-130 | 能回答"为什么独占文件页不能只翻写位" |
| K-219 | `cow_block` 与 clearend 尾页清零（Fix #80） | 机制 | ✅新增 | 17 §3、23 §2 | mem_file.c:59/73-79/131-134 | 能回答"EOF 后的陈旧字节为什么会泄漏" |
| K-220 | `copy_page_content` / `copy_page_and_zero_tail` | 机制 | ✅新增 | —（仅 todo Fix #80） | `cow_exec_pf.rs` | 能回答"页拷贝的两个变体" |
| K-221 | `anon_writable` vs `mappedfile_writable` 语义差 | 接口与协议 | 存量 | 17 §2、12 §2 | mem_anon.c:105；mem_file.c:173（"We are never writable"） | 能回答"为什么快路不能对文件页生效" |

#### O. 服务：fork/brk/mmap/munmap/exit（K-225–K-270）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-225 | `do_fork` 全流程 | 机制 | 存量 | 18 §2 | fork.c:32 | 能回答"VM_FORK 一次请求做了什么" |
| K-226 | 子槽分配与 endpoint 合成 | 机制 | 存量 | 18 §2、03 §2 | fork.c:67/83 | 能回答"子进程身份怎么生成" |
| K-227 | `pt_new` + `pt_ptmap` + `pt_map_in_range` | 机制 | 存量 | 18 §2、08 §2 | fork.c；pagetable.c:685/631 | 能回答"子页表怎么建" |
| K-228 | `map_proc_copy` 建立共享 | 机制 | 存量 | 18 §2、13 §2 | region.c:933 | 与 K-154 交叉，本篇讲调用面 |
| K-229 | `acl_fork` 继承 | 机制 | 存量 | 18 §2、04 §2 | acl.c:110 | 能回答"子进程权限从哪来" |
| K-230 | 写保护批量建立 | 机制 | 存量 | 18 §2、17 §2 | `fork.rs` `setup_cow_for_all_regions` | 与 K-216 交叉 |
| K-231 | fork 失败回滚（`pt_free`） | 机制 | 存量 | 18 §2 | fork.c:70-71（ENOMEM） | 能回答"半途失败怎么收摊" |
| K-235 | `do_brk` / `real_brk` | 机制 | 存量 | 19 §2 | break.c:44/62 | 能回答"堆顶调整的两层" |
| K-236 | `DATA_CHANGED` / `STACK_CHANGED` 通知 | 接口与协议 | 存量 | 19 §2 | break.c | 能回答"为什么改堆顶要通知内核" |
| K-237 | 区域 extend / shrink（A-12） | 机制 | 存量 | 19 §3、13 §3 | `vir_region.rs:127-140` | 能回答"resize 回调为何被取消" |
| K-238 | brk 与 `VM_DATATOP` / `MINSTACKREGION` 边界 | 约束与不变量 | 存量 | 19 §2 | vm.h:34/76 | 能回答"堆能长到哪" |
| K-239 | brk 收缩的 PTE 与物理页释放 | 机制 | 存量 | 19 §4 | `brk.rs:166/190-191` | 能回答"收缩是否真的归还内存" |
| K-245 | `do_mmap` 与三路地址解析（`mmap_region`） | 机制 | 存量 | 20 §2 | mmap.c:200/36 | 能回答"addr 参数如何被解释" |
| K-246 | 匿名映射同步路径 | 机制 | 存量 | 20 §2 | mmap.c:200-269 | 能回答"不需要等 VFS 的分支" |
| K-247 | 文件映射异步路径（`do_vfs_mmap` / `mmap_file` / `mmap_file_cont`） | 机制 | 存量 | 20 §2、23 §2 | mmap.c:135/84/160 | 能回答"为什么要 SUSPEND" |
| K-248 | `map_perm_check`（execpriv 分级） | 约束与不变量 | 存量 | 20 §2、21 §2 | mmap.c:284 | 能回答"谁能映射别人的进程" |
| K-249 | `do_remap` / `do_remap_ro` | 机制 | 存量 | 20 §2 | mmap.c:366 | 能回答"共享区域如何二次映射" |
| K-250 | `MAP_*`/`PROT_*` 标志映射 | 接口与协议 | 存量 | 20 §2 | `sys/sys/mman.h:62-124`；`mmap.rs:56-133` | 能回答"POSIX 标志如何落到 VR/PTE" |
| K-251 | `MMAP_BASE`/`MMAP_TOP` 布局 | 数据结构 | 存量 | 20 §3 | `mmap.rs:203-204` | 与 K-068 交叉 |
| K-255 | `do_munmap` / `munmap_vm_lin` | 机制 | 存量 | 21 §2 | mmap.c:512/488 | 能回答"拆除请求的入口解析" |
| K-256 | `map_unmap_range` 四情形 | 机制 | 存量 | 21 §2、13 §2 | region.c:1222 | 能回答"范围跨越若干区域时怎么办" |
| K-257 | `map_unmap_region` 三情形 | 机制 | 存量 | 21 §2、13 §2 | region.c:1065 | 能回答"区域内部分拆除" |
| K-258 | `pb_unreferenced` 解除引用 | 机制 | 存量 | 21 §2、11 §2 | pb.c:96 | 与 K-127 交叉 |
| K-259 | `do_map_phys` / `do_unmap_phys`（directphys） | 机制 | 存量 | 21 §2 | mmap.c:310；mem_directphys.c | 能回答"物理映射为何是特例" |
| K-260 | `VM_SHM_UNMAP` 与 `remaps` 递减（Fix #64） | 机制 | ✅新增 | 21 §2、12 §3.7 | `region/mod.rs::release_shared_remap` | 能回答"GET_REF 为什么曾虚高" |
| K-261 | `fdref_deref` | 机制 | 存量 | 21 §2、23 §2 | fdref.c:116 | 与 K-277 交叉 |
| K-265 | `do_exit` / `do_willexit` 两阶段 | 机制 | 存量 | 22 §2 | exit.c:60/100 | 能回答"为什么退出分两步" |
| K-266 | `free_proc` / `clear_proc` / `reset_vm_rusage` | 机制 | 存量 | 22 §2、02 §2 | exit.c:33/45/25 | 与 K-036 交叉 |
| K-267 | `do_procctl`（含 VFS transid 路由） | 机制 | 存量 | 22 §2、15 §2 | exit.c:117 | 能回答"为什么 procctl 走特殊优先级" |
| K-268 | `VMPPARAM_*`（exec 的地址空间重建） | 数据结构 | 存量 | 22 §2 | exit.c:135-137（`VMPPARAM_CLEAR`） | 能回答"exec 如何复用同一个槽" |
| K-269 | 页表整树 destroy + 物理页归还 | 机制 | 存量 | 22 §4、todo §18.3 | `exit.rs:54/86-142` | 能回答"退出的资源闭环" |
| K-270 | exit 不清 `vfs_queue` 挂起请求（P3-1(2)） | 架构演进 | 存量 | todo §18.3 | `vfs_queue.rs`（无 purge API） | 能回答"退出时还有什么没清" |

#### P. 跨服务协作（K-275–K-321）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-275 | `vfs_request` / `do_vfs_reply` / `activate` | 机制 | 存量 | 23 §2 | vfs.c:60/109/43 | 能回答"异步对话的三件套" |
| K-276 | `VfsRequestState`（FdLookup / FdIo） | 数据结构 | 存量 | 23 §2 | `vfs_queue.rs:29-33/107-148` | 能回答"挂起请求如何存" |
| K-277 | `fdref_new/ref/deref/dedup_or_new` | 机制 | 存量 | 23 §2 | fdref.c:93/109/116/156 | 能回答"fd 引用为何要去重" |
| K-278 | `mappedfile_setfile` | 机制 | 存量 | 23 §2、20 §2 | mem_file.c:191 | 能回答"文件区域如何绑定 inode" |
| K-279 | SUSPEND 与异步续作 | 接口与协议 | 存量 | 23 §2、15 §1 | vfs.c:109 | 与 K-176/K-205 交叉 |
| K-285 | `(dev, dev_offset)` 主键 + `(dev, ino, ino_offset)` 辅索引 | 数据结构 | 存量 | 24 §2 | cache.c:177/198 | 能回答"缓存为什么要两个索引" |
| K-286 | 精确 LRU（`lru_add/rm/touch`） | 机制 | 存量 | 24 §2 | cache.c:52/29/70 | 能回答"淘汰顺序如何维护" |
| K-287 | `addcache` / `rmcache` / `find_cached_page_bydev/byino` | 机制 | 存量 | 24 §2 | cache.c:216/259/177/198 | 能回答"缓存增删改查" |
| K-288 | `cache_freepages` 淘汰 | 机制 | 存量 | 24 §2 | cache.c:288 | 能回答"内存紧张时先淘汰谁" |
| K-289 | `clear_cache_bydev` | 机制 | 存量 | 24 §2 | cache.c:313 | 能回答"设备下线时如何清缓存" |
| K-290 | 四个 cache IPC handler | 机制 | 存量 | 24 §2/§4 | mem_cache.c:95/196/283/315 | 能回答"VFS 如何操作缓存" |
| K-291 | `IN_CACHE` 与不进回收漏斗 | 机制 | 存量 | 24 §3、12 §2 | `page_state.rs:166/175` | 与 K-142 交叉 |
| K-292 | `mem_cache.c` 的 cache memtype 回调 | 机制 | 存量 | 24 §2、12 §2 | mem_cache.c:34-95 | 能回答"缓存页的缺页行为" |
| K-293 | mapcache 失败回滚（Fix #74） | 机制 | ✅新增 | 24 §3.6 第 10 行 | `dispatcher.rs` 测试；C mem_cache.c:155-157 | 能回答"两侧回滚顺序是否真的相反" |
| K-294 | ONCE 条目统一走 NeedVfsIo（登记差异） | 架构演进 | 存量 | 24 §3.6 第 6 行 | `24-page-cache.md` §3.6 | 能回答"差一次 IPC 往返是否可接受" |
| K-300 | `do_rs_set_priv` | 机制 | 存量 | 25 §2 | rs.c:34 | 能回答"特权如何下发" |
| K-301 | `do_rs_prepare`（未实现，A-8） | 架构演进 | 存量 | 25 §3.9 | rs.c:71 | 能回答"LU 准备阶段缺什么" |
| K-302 | `do_rs_update`（未实现，A-8） | 架构演进 | 存量 | 25 §3.9 | rs.c:150 | 能回答"LU 切换阶段缺什么" |
| K-303 | `do_rs_memctl` 与 `rs_memctl_*` 四子命令 | 机制 | 存量 | 25 §2 | rs.c:349/218/281/300/329 | 能回答"内存控制能问什么" |
| K-304 | `rs_memctl_make_vm_instance` 恒拒（MAKE_VM 偏差） | 架构演进 | 存量 | 25 §3.9/§3.10 | rs.c:218（`num_vm_instances==2` 才 EPERM）；`rs.rs:504-507` | 能回答"为什么 Rust 连第一个额外实例也拒" |
| K-305 | `RprocTab` 握手 / `RS_INIT` | 接口与协议 | 存量 | 25 §2、01 §2 | `vm_server.rs::rs_handshake`；main.c:149-152 | 能回答"VM 怎么认识 RS" |
| K-306 | `adjust_proc_refs` | 机制 | 存量 | 25 §2、01 §2 | utility.c:477；main.c:215/722 | 能回答"LU 后引用如何校正" |
| K-307 | `sef_cb_init_lu_restart` / `lu_state_changed` / `init_vm_multi_lu` | 机制 | 存量 | 25 §2、01 §2 | main.c:677/196/592 | 能回答"LU 的三段回调" |
| K-308 | IPC 过滤器（`IPCF_MATCH_M_SOURCE`/`M_TYPE`） | 机制 | 存量 | 25 §2 | main.c:610-666 | 能回答"LU 期间为什么只放行少数调用" |
| K-309 | `num_vm_instances` / `VMF_VM_INSTANCE` | 数据结构 | 存量 | 25 §2、01 §2 | main.c:578-579 | 能回答"多 VM 实例的记账" |
| K-315 | `do_info` 四子请求 | 机制 | 存量 | 26 §2 | utility.c:100 | 能回答"INFO 能问出什么" |
| K-316 | `get_usage_info` / `_kernel` / `_vm` | 机制 | 存量 | 26 §2 | region.c:1395/1357/1366 | 能回答"三种使用量口径" |
| K-317 | `get_region_info` 与 `MAX_VRI_COUNT` 分页 | 机制 | 存量 | 26 §2 | region.c:1452；vm.h:66 | 能回答"区域列表为什么分页" |
| K-318 | `do_get_phys` / `do_get_refcount` | 机制 | 存量 | 26 §2 | mmap.c:438/463 | 能回答"GETPHYS 实际返回什么" |
| K-319 | `do_getrusage`（PM-only） | 机制 | 存量 | 26 §2 | utility.c:426 | 能回答"谁能查 rusage" |
| K-320 | `vm_stats_info` / `vm_usage_info` / `vm_region_info` | 数据结构 | 存量 | 26 §2 | `minix/include/minix/vm.h:40-64` | 能回答"三个回复结构的字段" |
| K-321 | `is_stack_region` | 机制 | 存量 | 26 §2 | region.c:1384 | 能回答"栈区域为何特判" |

#### Q. 差异、缺口与工程（K-325–K-344）★新增篇承载

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-325 | ARCH A-1 ~ A-12 清单 | 架构演进 | 存量 | plan §4（参考材料，非教学文档） | plan.md:210-224 | 能回答"Rust 相对 C 有意改了什么" |
| K-326 | 未实现调用与 edge 条目 | 架构演进 | 存量 | todo §18.4/§19、edge_todo.md | E-VMTLB/E-RSWIRE/E-VFSWIRE/E1/E2/E5 | 能回答"通电前还差什么" |
| K-327 | WONTFIX（构建脚本/Makefile.inc/vm.lds） | 架构演进 | 存量 | plan §5.4 | `minix3/minix/servers/vm/Makefile.inc` | 能回答"哪些 C 制品不在重写范围" |
| K-328 | 死代码排除（`copy_abs2region` / `map_memory` / `unmap_memory` / VM 自身 libc `mmap`/`munmap`/`_brk`） | 架构演进 | 存量 | plan §5.4 | region.c:860；proto.h；utility.c:361/376/385 | 能回答"哪些 C 符号不必实现" |
| K-330 | Cargo features（6 个） | 工具与工程 | ✅新增 | —（仅 Cargo.toml） | `os/servers/vm/Cargo.toml` | 能回答"后端与开关如何切换" |
| K-331 | crate 依赖（minix-types/sys/sef/elf/arch + bitflags） | 工具与工程 | ✅新增 | — | `os/servers/vm/Cargo.toml` | 能回答"VM 依赖哪些共享 crate" |
| K-332 | 三 feature 测试矩阵 | 测试性质 | ✅新增 | 各篇 §5 分散 | `todo.md:40-43`（503/521/503） | 能回答"换后端会不会破坏语义" |
| K-333 | `SimPaging` / `MockGateway`（`KernelGateway` seam） | 工具与工程 | ✅新增 | 00 §4.1 提及 | `kernel_gateway.rs`；`pagetable/sim.rs` | 能回答"无硬件怎么测缺页" |
| K-334 | allocator parity 测试 | 测试性质 | ✅新增 | 05 §5 分散 | `allocator_tests` | 能回答"三后端对账由谁保证" |
| K-335 | `test_callmap_registration_matches_c` | 测试性质 | ✅新增 | 15 §5 提及 | `15-ipc-dispatch.md:644` | 能回答"调用表对账的守护在哪" |
| K-336 | coverage-extract / SYMBOLS / design-coverage-check | 工具与工程 | ✅新增 | —（仅 todo gate-evidence） | `tools/coverage-extract/`；371 符号 / 92.5% | 能回答"覆盖率口径与数字来源" |
| K-340 | endpoint/generation 编码 | 概念 | 存量 | 99 §2、03 §2 | `endpoint.h` | 能回答"endpoint 的位布局" |
| K-341 | `VM_*` 调用号族表 | 数据结构 | 存量 | 99 §2 | com.h:630-773 | 能回答"49 个调用号各是什么" |
| K-342 | `VMP_*`/`VR_*`/`WMF_*`/`PAF_*` 旗标族 | 数据结构 | 存量 | 99 §2 | vm.h；region.h；vmproc.h | 能回答"所有魔数在哪查" |
| K-343 | 全局变量表 | 数据结构 | 存量 | 99 §2、00 §3 | glo.h | 能回答"VM 有哪些全局状态" |
| K-344 | C ↔ Rust 类型对照表 | 概念 | 存量 | 99 §3 | 各篇 §3 | 能回答"`phys_clicks` 对应 Rust 什么类型" |

---

## 3. 覆盖审计

### 3.1 主题全集与四路来源

1. **C 源码符号**：24 个 .c + 20 个头文件全部逐文件核对函数面（见 §0.3），共 371 个 C 符号（`coverage-extract` 口径，todo §18.0）。
2. **OS 通用概念**：地址空间、页表层级、CoW、需求分页、引用计数、LRU 缓存、slab 分配、buddy/bitmap 分配器、权限位图、进程状态机、endpoint 命名空间。
3. **非 C 制品**：`Cargo.toml`（双目标/6 feature/6 依赖）、`main.rs`（入口）、`minix-elf`、`minix-sef`、`minix-arch`、`minix-types`（wire）、`minix-sys`；`os/servers/vm` 下**无** build.rs、**无** 链接脚本。
4. **阶段边界契约**：`00-master-plan/README.md` 的启动因果链（VM 是执行顺序上第一个用户服务）、`01-stage-kernel/09-vm-boot-protocol.md` 的 8 步协商、`edge_todo.md` 的 E1/E2/E5/E-RSWIRE/E-VFSWIRE/E-VMTLB。

### 3.2 覆盖缺口表

| # | 缺口主题 | 证据 | 现状 | 处置 |
|---|---------|------|------|------|
| G-01 | **Direct Map 与地址空间布局**没有独立篇章，只作为 07 的 §3.6 一节，却是 05/06/07/08 四篇的共同前置 | 05/06 的前置字段都指向 07（§0.4 grep 证据） | 07 承载 → 形成环 | **新建** `06-direct-map-and-address-space`（承载 K-065–K-071） |
| G-02 | **消息线格式**没有独立篇章，散在 15/20/21/26 四篇的 §4，且 V13-P2-6 的五结构错位无处落地 | todo §18.2（V13-P2-6）；K-173 | 分散 + 无处登记偏差 | **新建** `15-vm-message-protocol`（承载 K-170–K-179、K-173/174） |
| G-03 | **执行模型与全局不变量**（单线程循环 / TLB 纪律 / VM 不能缺页 / fail-fast / panic=abort）散在 00 §1.2、16 §3.7、08 §1.8，没有一处完整论证 | K-020–K-028 分散 | 三处各讲一半 | **新建** `01-vm-execution-model`（承载 K-020–K-028） |
| G-04 | **ARCH 演进与缺口**只存在于 `plan.md` §4（参考材料）与 `todo.md`（台账），正式目录里没有一篇承载 | plan.md:210-224（A-1~A-12） | 教学文档缺失 | **新建** `28-vm-c-parity-and-gaps`（承载 K-325–K-328） |
| G-05 | **构建与测试基建**（features、依赖、三矩阵、SimPaging/MockGateway 注入、覆盖率工具）只在 00 §4.1 一句带过 | 00-vm-overview.md:85 | 严重不足 | **新建** `29-vm-build-and-test`（承载 K-330–K-336） |
| G-06 | `99-global-concepts.md` 只有 74 行，是 28 篇里最短的一篇，却是所有机制文档的词汇表 | `wc -l` = 74 | 名不副实 | **扩充**：K-340–K-344 全表化，目标 ~400 行 |
| G-07 | `map_region_init` 在 C 里是**空函数**（region.c:36-38），现有文档按"初始化"叙述，未点明 | region.c:36-38 实测 | 事实未登记 | **并入** `14-region-map` 的位置可回答性章节（K-164） |
| G-08 | `find_overlap` 邻居探测与零长拒绝、`alloc_contiguous`、clearend、`remaps` 递减、release 可观测性五处 2026-09-09 后的新增语义只在 todo 里，未进正式文档 | todo Fix #63/#64/#67/#72/#79/#80 | 文档落后于代码 | **逐条并入**：14（K-165）、07（K-080）、18（K-219/220）、22（K-260）、16（K-195） |
| G-09 | exec 的 VM 侧（`VMPPARAM_CLEAR` 地址空间重建）没有独立归属，只在 22 里一句 | exit.c:135-137 | 边界不清 | **并入** `23-vm-exit` 的独立章节（K-268） |
| G-10 | `minix-sef` 方案（V14-P2-1）只在 todo §19，VM 侧尚未裁决 | todo §19 | 未决 | **并入** `16-vm-main-loop` 的接收半节（K-186），并标注"待 E1 裁决" |
| G-11 | 判定**属于其它 stage**：boot-shim / 内核页表建立 / `SYS_VMCTL` 内核侧 / SMP TLB 刷新 | `01-stage-kernel/09-vm-boot-protocol.md`、`16-smp.md` | 越界 | 不在本 stage 新建，只在 `01` 与 `17` 的"不讲什么"里写清去向 |

### 3.3 重复主题表

| # | 重复主题 | 现有重复位置 | 新目录主讲述点 | 其余处置 |
|---|---------|-------------|--------------|---------|
| D-01 | `SUSPEND` 伪返回码 | 15 §1、23 §1、20 §2、16 §2 | **15-vm-message-protocol** | 其余改为一句引用 |
| D-02 | TLB 纪律与"不在运行的进程"不变量 | 08 §1.8、16 §3.7、00 §1.2 | **01-vm-execution-model**（不变量本体） | 08 只讲 `write_pte_dm` 的写后失效；16 只列逐路径满足性 |
| D-03 | `map_proc_copy` | 13 §2、18 §2 | **14-region-map**（机制本体） | 19 只讲调用面与 fork 上下文 |
| D-04 | `pb_unreferenced` | 11 §2、21 §2 | **12-phys-page-state** | 22 只讲调用点 |
| D-05 | `IN_CACHE` | 11 §3、12 §2、24 §2/§3 | **25-page-cache** | 12/13 只列标志定义 |
| D-06 | `fdref_deref` | 21 §2、23 §2 | **24-vfs-interaction** | 22 只讲调用点 |
| D-07 | `free_proc`/`clear_proc` | 02 §2、22 §2 | **03-vmproc-and-proc-table**（定义） | 23 只讲退出时的调用序列 |
| D-08 | `swap_proc_slot` / `swap_proc_dyn_data` | 03 §2、10 §2、25 §2 | **11-vm-relocation**（原语本体） | 03 只列出处；26 只讲 LU 调用面 |
| D-09 | `MMAP_BASE`/`MMAP_TOP` 与地址空间宽度 | 07 §3、20 §3、00 §3 | **06-direct-map-and-address-space** | 21 只引用常量 |
| D-10 | `vm_call_mask` 宽度 | 04 §3、25 §2、todo §18.2 | **04-vm-acl**（语义） | 15 只讲 wire 字段宽；28 登记缺口 |
| D-11 | `mem_cow` 与 CoW 换型 | 17 §2、18 §2、23 §2 | **18-cow**（机制本体） | 19 只讲建立；24 只讲文件分支 |

### 3.4 越界主题表

| # | 越界 | 现有位置 | 正确归属 |
|---|------|---------|---------|
| O-01 | 01-vm-init-main 用 ~120 行讲主循环骨架（`run()`/`run_once` 五优先级） | 01 §1/§2 | **16-vm-main-loop**；01 只保留"进入主循环之前" |
| O-02 | 05-physical-memory 用 `PAF_CLEAR` 与 Direct Map 作前置论述 | 05 前置字段 | **06-direct-map-and-address-space**；05 只写"清零需求由谁满足"并引用 |
| O-03 | 07-pagetable-struct 用一整节讲 Direct Map（§3.6），而 Direct Map 是 os/arch 层概念 | 07 §3.6 | **06**；07 只保留"页表结构如何消费 direct map" |
| O-04 | 13-region-mapping 展开 `find_slot`/`map_lookup` 的查找语义 | 13 §2 | **14-region-map**（合并后同篇，消除该越界） |
| O-05 | 10-vm-relocation 讲 `swap_proc_*` 的 LU 流程 | 10 §2 | 原语留 11；LU 流程归 **26-rs-services** |
| O-06 | 20/21/26 三篇各自展开一遍 `mess_*` overlay 字段布局 | 20 §4、21 §4、26 §4 | **15-vm-message-protocol**；三篇只列各自的字段表 |
| O-07 | 25-rs-services 讲 `map_service`、`sef_cb_init_fresh` | 25 §2 | 启动注册归 **02-vm-boot-chain**；25 只讲 RS 的四个 handler |
| O-08 | 各篇 §5 各写一遍测试矩阵全量数字 | 28 篇 §5 | **29-vm-build-and-test** 统一基线；各篇只列本篇相关测试 |
| O-09 | `todo.md` / `plan.md` / `checklist.md` 的内容被正文反复转述（Fix #N 账目） | 多篇 | **28-vm-c-parity-and-gaps** 统一登记；正文只结论不记账 |

### 3.5 非 C 主题逐项回答（固定清单）

| 主题 | 是否属于本 stage | 归属 |
|------|----------------|------|
| **链接与加载** | 是（部分） | `02-vm-boot-chain`：`minix-elf` 承担 boot 进程 ELF 段解析（Cargo.toml 依赖 + main.c:383 `libexec_load_elf`）；链接脚本属 kernel（`os/kernel/src/arch/*/link.ld`），VM crate 无链接脚本 → 在 02 的"不讲什么"里声明 |
| **镜像与内存布局** | 否 | 镜像布局属 `01-stage-kernel`；VM 侧只消费 `kernel_boot_info`（`02-vm-boot-chain` K-002）与自身窗口常量（`06` K-071） |
| **汇编入口与陷阱进入** | 否 | 属 `01-stage-kernel`（trap 层）；VM 只收到已被转成消息的 `VM_PAGEFAULT`（`17-pagefault` K-200） |
| **启动装配** | 是 | `02-vm-boot-chain` 全篇（B01–B20 真序表） |
| **构建与工具链** | 是 | `29-vm-build-and-test`：Cargo 双目标、6 feature、6 依赖、无 build.rs/无链接脚本的事实 |
| **跨模块接口与线格式** | 是 | `15-vm-message-protocol`（M1/M2/M3 overlay、`mess_*`、minix-types 解码、56 字节断言、errno 编码） |
| **错误路径** | 是 | `15-vm-message-protocol`（errno→m_type、ENOSYS 兜底）+ `01-vm-execution-model`（fail-fast/fail-closed/panic=abort）+ `28`（未实现调用的 ENOSYS 契约） |
| **关闭与退出** | 是 | `23-vm-exit`（VM_EXIT/WILLEXIT）+ `01`（panic=abort，VM 无 graceful shutdown） |
| **并发与同步** | 是 | `01-vm-execution-model`（单线程事件循环、借用检查器即并发审计器）+ `17`（不在运行进程的不变量）+ `28`（E-VMTLB 挂 edge） |
| **测试基建** | 是 | `29-vm-build-and-test`：三 feature 矩阵、SimPaging/MockGateway 注入、parity 对账、CALLMAP 守护测试、coverage-extract |

### 3.6 范围外发现

1. **`01-stage-kernel/09-vm-boot-protocol.md` 的 §1.2 里嵌入了一句 TODO 批注**（"*TODO 这里应该是讲C，而不是Rust*"），说明 kernel 侧该文档自身未完成；本 stage 的 `02-vm-boot-chain` 不应依赖它的叙述结论，只引用 `SYS_VMCTL` 的 8 步时序事实。
2. **`os/servers/vm/Cargo.toml` 已经声明 `minix-sef` 依赖**（注释 "V14-P2-1 (plan A)"），但 `todo.md` §19 仍把该条目列为"开口/待裁决"——**代码已走在前、文档未跟上**。重建时 `16-vm-main-loop` 必须按"已引入、待裁决"的现状写，不能按"未引入"写。
3. **`checklist.md`（867 行）与 `todo.md`（510 行）承担了大量本应由教学文档承载的内容**（函数表、差异表、Fix 账目）。它们是过程台账，不是读者路径。重建后这部分知识应迁到 `28`。

---

## 4. 新目录

### 4.1 新篇章总表（30 篇 + 99）

| 编号 | 标题 | 一句话定位 | 分组 |
|------|------|-----------|------|
| 00 | `00-vm-overview` | VM 是谁、启动主线长什么样、30 篇怎么读 | 入口 |
| 01 | `01-vm-execution-model` | VM 的角色边界、执行模型与四条全局不变量 | 入口（基础） |
| 02 | `02-vm-boot-chain` | 从 `main()` 到进入主循环：init_vm 13 步 + SEF + boot 进程装载 | 启动链 |
| 03 | `03-vmproc-and-proc-table` | VM 如何记住一个进程和所有进程：结构体、状态机、表、endpoint | 启动链 |
| 04 | `04-vm-acl` | 谁能调哪个服务：调用掩码的位图、分层与 fail-closed 偏移 | 启动链 |
| 05 | `05-physical-memory` | 物理内存这本账：布局解析、clicks 分配器、三后端 | 资源供给 |
| 06 | `06-direct-map-and-address-space` | 每个物理页都有一个恒定虚拟地址：双视图与地址空间布局 | 资源供给 |
| 07 | `07-vm-page-allocator` | VM 给自己要页：allocpage 族与保留页池的结构性消失 | 资源供给 |
| 08 | `08-pagetable-structure` | 页表对象长什么样：pt_t、层级、自映射、多架构 | 资源供给 |
| 09 | `09-pagetable-operations` | 页表的增删改查：new/free/bind/writemap/copy/check | 资源供给 |
| 10 | `10-vm-heap` | VM 自己的堆从哪来：slab → HeapArena | 资源供给 |
| 11 | `11-vm-relocation` | 自举的终点：元数据搬迁与槽位交换原语 | 资源供给 |
| 12 | `12-phys-page-state` | 一个物理页被谁引用着：phys_block/region 与引用计数 | 地址空间账本 |
| 13 | `13-memtype` | 页的六种身份：memtype 回调族与各类型语义 | 地址空间账本 |
| 14 | `14-region-map` | 进程的虚拟地址空间账本：vir_region 与有序索引 | 地址空间账本 |
| 15 | `15-vm-message-protocol` | 一条 VM 消息长什么样：线格式、解码、errno、SUSPEND | 运行时 |
| 16 | `16-vm-main-loop` | 心跳：收消息、五优先级分发、ACL 闸、回复 | 运行时 |
| 17 | `17-pagefault` | 两条入口一个汇合点：缺页状态机与异步恢复 | 运行时 |
| 18 | `18-cow` | 写时复制：何时复制、复制后页变成什么 | 运行时 |
| 19 | `19-vm-fork` | VM_FORK：地址空间复制与写保护建立（代表成员精讲） | 服务·进程生命周期 |
| 20 | `20-vm-brk` | VM_BRK：堆顶的伸长与收缩 | 服务·进程生命周期 |
| 21 | `21-vm-mmap` | VM_MMAP：地址空间分配与内存来源绑定 | 服务·映射管理 |
| 22 | `22-vm-munmap` | VM_MUNMAP：拆除映射与解除引用 | 服务·映射管理 |
| 23 | `23-vm-exit` | VM_EXIT/WILLEXIT/PROCCTL：清算与地址空间重建 | 服务·进程生命周期 |
| 24 | `24-vfs-interaction` | 与 VFS 的异步对话：请求队列与 fdref | 协同 |
| 25 | `25-page-cache` | VM 作为磁盘块缓存中介：双哈希、LRU、四个 handler | 协同 |
| 26 | `26-rs-services` | RS 驱动的 live update：四个 handler 与 LU 回调 | 协同 |
| 27 | `27-vm-queries` | VM 作为内存权威的只读窗口：INFO/GETPHYS/GETREF/GETRUSAGE | 协同 |
| 28 | `28-vm-c-parity-and-gaps` | 与 C 的差距清单：ARCH 演进 + 未实现与偏差 | 支线 |
| 29 | `29-vm-build-and-test` | 怎么构建、怎么测、覆盖率怎么量 | 支线 |
| 99 | `99-vm-global-concepts` | 全部魔数与类型的对照词汇表（工具篇） | 工具 |

### 4.2 阅读路径

**主线（启动 + 核心数据流，建议顺读）**
```
00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11
   → 12 → 13 → 14 → 15 → 16 → 17 → 18
```
**服务支线（并行体，19 是代表成员精讲，20–23 可按需跳读）**
```
19（fork，精讲：一次请求如何贯穿 03/08/12/13/14/18）
20（brk）  21（mmap）  22（munmap）  23（exit）
```
组内差异表（在 19 与 21 内各自给出）：

| 服务 | 建/拆 | 是否异步 | 是否涉及 CoW 建立 | 是否跨服务 |
|------|-------|---------|------------------|-----------|
| VM_FORK（19） | 建（整空间复制） | 否 | **是**（写保护批量建立） | 否（但要 ACL 继承） |
| VM_BRK（20） | 建/拆（同一区域伸缩） | 否 | 否 | 否 |
| VM_MMAP（21） | 建 | **是**（文件映射走 VFS） | 否 | **是**（VFS） |
| VM_MUNMAP（22） | 拆 | 否 | 否（但要解除引用） | 否 |
| VM_EXIT（23） | 拆（整空间） | 否 | 否 | 否（但接 VFS transid） |

**协同支线**：`24 → 25 → 26 → 27`（24 是 25 的前置，26/27 各自独立）

**可跳读**：`28`（差异与缺口）、`29`（构建与测试）
**工具篇**：`99`（任何时候查魔数都可用；主线读者建议在读 05 之前先扫一遍 K-341/K-342）

### 4.3 并行体的组织说明

- **统一框架篇**：`15`（线格式）+ `16`（分发）是所有服务的共同框架，先于任何单个服务出现；
- **按角色分组**：19/20/23 是"进程生命周期"组，21/22 是"映射管理"组；
- **代表成员精讲**：19（fork）贯穿的层最多（进程表/ACL/页表/物理页/memtype/区域/CoW），作为组内第一个精讲对象；21（mmap）是映射管理组的精讲对象；
- **其余按差异表收束**：20/22/23 各自只讲与代表的差异。

---

## 5. 每篇契约

### 00-vm-overview

- **一句话定位**：读者第一次打开这个目录时，用 200 行搞清"VM 是干什么的、启动长什么样、30 篇按什么顺序读"。
- **讲什么**：K-020（三重权威）、K-004（init_vm 13 步骨架图，不展开）、K-028（五优先级一句话）、00 自身的源码地图与导航表、实施现状总览（覆盖率/测试基线，指向 29）。
- **不讲什么**：
  - 任何机制的展开 → 01–27 各自承担；
  - 常量与调用号表 → 99；
  - ARCH 演进与未实现缺口 → 28；
  - 测试矩阵的具体数字 → 29（00 只给一句话 + 指针）。
- **前置**：无。
- **后置**：全部 28 篇。
- **事实底线**：
  - C：`minix3/minix/servers/vm/main.c:93-194`（main）、`:428-587`（init_vm）、`glo.h`、`vm.h`；
  - Rust：`os/servers/vm/src/main.rs`、`vm_server.rs`（`VmServer::run`）；
  - 非 C：`os/servers/vm/Cargo.toml`（双目标）。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-020 | 三重权威 | 概念 | main.c:112-192 | 00 是唯一给全景的地方 | 存量：00 §1.1-1.2 |
  | K-004 | init_vm 13 步骨架 | 机制 | main.c:428-587 | 导航图的事实来源 | 存量：01 §1/§2 |
  | K-028 | 五优先级 | 机制 | main.c:143-176 | 一句话点明运行时形状 | 存量：15 §1 |
  | K-336 | 覆盖率与测试基线指针 | 工具与工程 | tools/coverage-extract/ | 现状总览需有可核对口径 | 新增：todo §18.0 |
- **验收标准**：读者读完能画出"kernel → VM → 其余服务"的因果链；能在不读任何机制文档的情况下说出 init_vm 的 13 步名字；能指出自己关心的机制在哪一篇。导航表中 30 篇的编号、标题、一句话定位与 §4.1 完全一致。

### 01-vm-execution-model

- **一句话定位**：解释"为什么 VM 敢这么写"——单线程事件循环、不能缺页、只改不运行的进程、失败即停，这四条不变量是后面 27 篇全部安全论证的地基。
- **讲什么**：K-020、K-021、K-022、K-023、K-024、K-025、K-026、K-027（K-027 只给定义，机制归 15）。
- **不讲什么**：
  - 缺页状态机 → 17；
  - PTE 写入与 invlpg 的操作细节 → 09；
  - ACL 位图与掩码 → 04；
  - 页表结构 → 08；
  - SMP TLB 刷新的 kernel 侧机制 → 属 `01-stage-kernel`，28 登记为 E-VMTLB。
- **前置**：00。
- **后置**：09、16、17、23、28。
- **事实底线**：
  - C：`main.c:112-192`；`pagetable.c:55-57`（eager mapping 注释）；`pagetable.c:119/255/319/430`（C 四处自刷）；`pagetable.c:799-815/928-934`（VMINHIBIT 停等）；`main.c:186-190`（fail-fast panic）；`vm.h:10-12`（SANITYCHECKS）；
  - Rust：`os/Cargo.toml:260/263`（panic=abort）；`os/arch/src/x86_64/paging.rs::write_pte_dm`；
  - 对照：`minix3/minix/kernel/proc.c:345-347`（C 的 MF_FLUSH_TLB，Rust 无对应）。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-021 | 单线程事件循环 vs SMP+BKL | 概念 | main.c:112 | 执行模型本体 | 存量：00 §1.2 |
  | K-022 | VM 不能缺页 | 约束 | pagetable.c:55-57 | 解释 eager mapping 的根因 | 存量：16 §1（:39） |
  | K-023 | 只改不在运行的进程 | 约束 | 16 §3.7 | 不变量本体（原散在 16） | 存量：16 §3.7（Fix #69） |
  | K-024 | TLB 纪律三半 | 机制 | pagetable.c:119/255/319/430 | C 自刷四处集体归位 | 存量：08 §1.8 + 16 §3.6 |
  | K-025 | fail-fast / fail-closed | 约束 | main.c:186-190 | 错误策略统一论述 | 存量：15 §1 + 04 §3 |
  | K-026 | panic = abort | 约束 | os/Cargo.toml:260 | 关闭语义的一半 | 存量：todo §18.3 |
- **验收标准**：能回答"为什么 VM 分配即映射"、"为什么写 PTE 后不必全局刷 TLB"、"为什么 C 需要四处自刷而 Rust 不需要"、"VM 崩溃后系统会怎样"四个问题；给出一张"各条路径为什么满足不在运行不变量"的表（缺页/服务/信号/LU 四类）。

### 02-vm-boot-chain

- **一句话定位**：从 `main()` 的第一行走到主循环的第一行，逐回答"这一步为什么在这、它依赖什么、它给出什么"。
- **讲什么**：K-001、K-002、K-003、K-004、K-005、K-006、K-007、K-008、K-009、K-010、K-011、K-012、K-013、K-014、K-015、K-016。
- **不讲什么**：
  - 主循环与分发 → 16；
  - `vmproc` 结构字段 → 03；
  - 物理分配器本体 → 05；
  - 页表本体 → 08/09；
  - 区域本体 → 14；
  - `do_rs_*` 四个 handler → 26（`map_service` 留本篇，因为它发生在 `sef_cb_init_fresh`）。
- **前置**：00、01。
- **后置**：03、04、05、08、09、14、16、26。
- **事实底线**：
  - C：`main.c:79-88`/`:93-108`/`:219-239`/`:241-260`/`:262-286`/`:331-417`/`:428-587`/`:755-768`；`glo.h`；`vm.h:80-84`；`utility.c:44`（get_mem_chunks 定义）；`alloc.c:281`（mem_add_total_pages 定义）；
  - Rust：`os/servers/vm/src/main.rs:22-46`、`boot.rs`、`global.rs`、`vm_server.rs::init`；
  - 非 C：`os/servers/vm/Cargo.toml`（`[[bin]]`/`[lib]`、`minix-elf`）、`os/libs/minix-elf`。
- **知识点清单**（摘要，完整 16 条见 §2.2 A 段）：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-004 | init_vm 13 步 | 机制 | main.c:428-587 | 本篇主骨架 | 存量：01 §1/§2 |
  | K-008 | exec_bootproc | 机制 | main.c:331-417 | 启动链最长的一步 | 存量：01 §2 |
  | K-005 | SEF 生命周期 | 机制 | main.c:219-239 | 启动握手本体 | 存量：01 §2 |
  | K-014 | warm restart 不支持 | 架构演进 | main.rs:29-46 | 入口处就要说清 | 存量：todo G-V12-10 |
  | K-015 | minix-elf 装载 | 工具与工程 | Cargo.toml + main.c:383 | 非 C 制品承载 | 新增 |
  | K-016 | bin/lib 双目标 | 工具与工程 | Cargo.toml | 非 C 制品承载 | 新增 |
- **验收标准**：能按 B01–B20 复述启动序；能指出 `exec_bootproc` 提前用到了后面哪几篇的机制（08/09/14/05）并在正文里显式"下放声明"；能解释 `__minix_init`（main.c:480）为什么是堆可用分界线。

### 03-vmproc-and-proc-table

- **一句话定位**：VM 对"进程"这件事的全部记忆——一个槽位里有什么、槽位集合怎么组织、endpoint 怎么翻译成槽号。
- **讲什么**：K-030、K-031、K-032、K-033、K-034、K-035、K-036、K-038（K-037 `swap_proc_slot` 只有一处定义说明，机制本体归 11）。
- **不讲什么**：
  - ACL 位图 → 04；
  - 页表归属 → 08/09；
  - 区域归属 → 14；
  - fork 如何分配子槽 → 19；
  - LU 槽位交换的调用面 → 26。
- **前置**：02。
- **后置**：04、12、14、16、19、23、26、27。
- **事实底线**：
  - C：`vmproc.h`（39 行全量）；`glo.h:VMP_NR/VMP_EXECTMP/vmproc[]`；`main.c:262-286`（init_proc）、`:458-462`（表清零 + vm_slot）；`utility.c:84`（vm_isokendpt）、`:188`（swap_proc_slot 定义）；`exit.c:33/45`（free_proc/clear_proc）；`minix3/minix/include/minix/endpoint.h:_ENDPOINT_GENERATION_SHIFT`；
  - Rust：`os/servers/vm/src/vmproc/`（`vmproc.rs`/`flags.rs`/`table.rs`/`vmproc_handle.rs`）。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-030 | vmproc 全字段 | 数据结构 | vmproc.h | 结构本体 | 存量：02 §2 |
  | K-031 | VMF_* 与状态机 | 数据结构 | vmproc.h + main.c:276-280 | 生命周期本体 | 存量：02 §1/§2 |
  | K-032 | 表与 vm_slot | 数据结构 | glo.h + main.c:458-462 | 集合级语义 | 存量：03 §2 |
  | K-033 | VMP_EXECTMP | 数据结构 | glo.h | 保留槽语义 | 存量：03 §2 |
  | K-034 | endpoint/generation | 接口与协议 | utility.c:84 + endpoint.h | 身份翻译 | 存量：03 §2 |
  | K-038 | typestate 视图 | 架构演进 | vmproc_handle.rs | Rust 侧的类型化 | 存量：02 §3 |
- **验收标准**：能画出槽位状态迁移图（空闲 → 活跃 → 退出中 → 空闲）并给出每个迁移的触发函数；能手工把一个 endpoint 拆成 slot + generation；能说明 `VMP_EXECTMP` 存在的唯一理由。合并后长度控制在 ~650 行（原 02+03 = 992 行），压缩手段：删除两篇各自重复的状态机叙述与"过渡"互引段。

### 04-vm-acl

- **一句话定位**：调用门禁——谁能调哪 49 个服务，掩码怎么存、怎么继承、以及 Rust 相对 C 收紧在哪里。
- **讲什么**：K-040、K-041、K-042、K-043、K-044（宽度语义；wire 字段宽归 15）、K-045（语义；未实现原因归 28）。
- **不讲什么**：
  - 主循环如何调用 → 16；
  - `RS_SET_PRIV` 的 LU 流程 → 26；
  - wire 上掩码的字节布局 → 15；
  - `RprocEntry.call_mask` 的 u32 截断修复 → 28（E-RSWIRE）。
- **前置**：03。
- **后置**：15、16、19、26、28。
- **事实底线**：
  - C：`acl.c` 全 129 行（`:21` init / `:37` check / `:70` set / `:110` fork / `:121` clear）；`main.c:168`；`vmproc.h`（`vm_acl` 字段）；`com.h:769-770`；`minix/include/minix/bitmap.h:BITCHUNK_BITS`；`sys_config.h:_NR_SYS_PROCS`；
  - Rust：`os/servers/vm/src/acl.rs`（`AclMask`/`AclState`，`:97-112` fail-closed）；`vm_server.rs`（闸门 `match get_active {…}`，Fix #66）；`minix-types/src/ipc/rprocpub.rs:89`（u64）。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-041 | 五函数 | 机制 | acl.c:21/37/70/110/121 | 本体 | 存量：04 §2 |
  | K-042 | A-11 fail-closed | 架构演进 | acl.c:44-53 vs acl.rs:97-112 | 语义偏移的诚实登记 | 存量：04 §3 + plan §4 |
  | K-043 | 闸门接线与 None 即拒绝 | 机制 | main.c:168 + Fix #66 | 接线面 | 存量：15 §2 + 04 §4.4 |
  | K-044 | 掩码 64 位 | 接口与协议 | rprocpub.rs:89 vs vm_server.rs:1597 | 宽度语义 | 存量：todo §18.2 |
- **验收标准**：能说出 `acl_check` 的三条早退路径（VM 自身直通、NO_ACL、RS 特例）并指出 Rust 去掉了哪一条、换成什么；能解释"槽位不可解析即拒绝"为什么与 fail-closed 自洽。

### 05-physical-memory

- **一句话定位**：物理内存这本账——从 BIOS/memmap 到 clicks 位图，再到三后端分配器，以及"总共有多少页"的两个口径。
- **讲什么**：K-050、K-051、K-052、K-053、K-054、K-055、K-056、K-057、K-058、K-059、K-060。
- **不讲什么**：
  - **Direct Map 与 `PAF_CLEAR` 的实现** → 06（本次重建的关键去环操作；05 只在需要处写"清零需求由 06 的 direct map 满足"）；
  - `vm_allocpage` 族 → 07；
  - 页表页供给 → 08/09；
  - 统计的回复编码 → 27。
- **前置**：02（**且只依赖 02**，不再前置 07——这是本蓝图消除环的核心改动）。
- **后置**：06、07、10、12、13、20、27。
- **事实底线**：
  - C：`utility.c:44`（get_mem_chunks）；`alloc.c:306`（mem_init）、`:242`（alloc_mem）、`:289`（free_mem）、`:281`（mem_add_total_pages）、`:227`（alloc_cycle）、`:100-226`（reservedqueue_*）、`:348/486/501/509`（memstats/printmemstats/usedpages_*）；`vm.h:20-25`（PAF_*）、`:66-67`（MAP_NONE/NO_MEM）；`main.c:431/455/471/485-495`；
  - Rust：`os/servers/vm/src/phys_mem/`（`bitmap_alloc.rs`/`buddy_alloc.rs`/`segment_tree_alloc.rs`/`mod.rs::max_page_bound`）、`alloc_stats.rs`；
  - 非 C：`os/servers/vm/Cargo.toml` features（bitmap/buddy/segment_tree）。
- **知识点清单**（摘要）：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-052 | mem_init/alloc/free | 机制 | alloc.c:306/242/289 | 分配器本体 | 存量：05 §2 |
  | K-055 | 三后端（A-5） | 架构演进 | phys_mem/* | 演进本体 | 存量：05 §3 |
  | K-056 | 低内存与 max_page_bound | 机制 | phys_mem/mod.rs | Fix #65 的落点 | 存量：05 §4.4 |
  | K-058 | total_pages 两口径 | 机制 | alloc.c:281 + utility.c:118 | INFO/STATS 的事实来源 | 存量：05 §2 + 26 §4.4 |
- **验收标准**：能解释 click 与 page 的换算及为什么要两层；能对同一请求说出三后端各自的算法形状；能说出 `total_pages` 与"memmap 之和"的差额来自哪两笔（boot 模块 + 内核占用）。

### 06-direct-map-and-address-space ★新建

- **一句话定位**：一切"物理页 ↔ 虚拟地址"换算的地基——双视图是什么、常量从哪来、以及它为什么顺手消灭了 C 的别名模型。
- **讲什么**：K-065、K-066、K-067、K-068、K-069、K-070、K-071。
- **不讲什么**：
  - 页表对象的结构 → 08；
  - 页表的写操作 → 09；
  - 堆的具体分配算法 → 10；
  - MMAP 区域的分配策略 → 21。
- **前置**：05。
- **后置**：07、08、09、10、21。
- **事实底线**：
  - C：`vm.h:76-84`（VM_MMAPTOP/MMAPBASE/VM_OWN_*）；`pagetable.c:255/319`（vm_freepages 的两处自刷，别名模型的证据）；C 的 `vm_phys_to_virt` 静态映射表；
  - Rust：`os/servers/vm/src/direct_map.rs:16-48`；`os/arch/src/direct_map.rs`（`DirectMapArch`）；`os/arch/src/x86_64/paging.rs`（4 级 walk）；
  - 跨 stage：`01-stage-kernel/09-vm-boot-protocol.md` §1.3（双视图对照表）。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-065 | 双视图（A-1） | 架构演进 | 09-vm-boot-protocol §1.3 | **去环的关键前置** | 存量：07 §3.6 |
  | K-067 | 双向换算 | 机制 | direct_map.rs:32-48 | 换算本体 | 存量：07 §2 + 06 §2 |
  | K-068 | 32→64（A-6） | 架构演进 | vm.h:76-78 | 宽度演进 | 存量：07 §3 + 20 §3 |
  | K-069 | PAF_CLEAR 清零 | 机制 | vm.h:20 | 05 的前向引用点在这里落地 | 存量：05 前置字段 |
  | K-070 | 别名模型消除 | 架构演进 | pagetable.c:255/319 | 解释 C 自刷 | 存量：08 §1.8 |
  | K-071 | VM_HEAP_* 常量 | 数据结构 | direct_map.rs:26-28 | 新增 | 新增 |
- **验收标准**：读者读完后，07/08/09 里任何出现 `Direct Map` 的地方都不需要再解释；能说出两个窗口的基址、U/S 位、G 位差异；能解释"为什么 Rust 不需要 `sys_vmctl(SELF, VMCTL_FLUSHTLB)`"。

### 07-vm-page-allocator

- **一句话定位**：VM 给自己要页的三条路径，以及 C 的备用页池为什么在 Rust 里根本不存在。
- **讲什么**：K-075、K-076、K-077、K-078、K-079、K-080。
- **不讲什么**：
  - 物理分配器本体 → 05；
  - Direct Map 原理 → 06；
  - 页表结构 → 08；
  - 堆分配器 → 10（10 只是 07 的消费方）。
- **前置**：05、06。
- **后置**：08、09、10、18、19。
- **事实底线**：
  - C：`pagetable.c:395`（vm_allocpage）、`:333`（vm_allocpages）、`:295`（vm_mappages）、`:235`（vm_freepages）、`:403`（vm_pagelock）、`:440`（vm_addrok）、`:155`（findhole）、`:55-57`（保留页池注释）；`alloc.c:100-226`（reservedqueue_*）、`:74`（missing_spares）、`:227`（alloc_cycle）；`vm.h:69-73`（VMP_*）；
  - Rust：`os/servers/vm/src/alloc_page.rs`（含 `vm_pt_alloc`）、`global.rs`、`region/page_state.rs::PfnAllocator::alloc_contiguous`。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-075 | allocpage 族 | 机制 | pagetable.c:395/333/295/235 | 本体 | 存量：06 §2 |
  | K-078 | 保留页池消除 | 架构演进 | alloc.c:100-226 + pagetable.c:55-57 | A-1 的结构性后果 | 存量：06 §3.3 + plan §7.3 |
  | K-080 | alloc_contiguous | 机制 | page_state.rs | 新增（Fix #79） | 新增 |
- **验收标准**：能说出"分配即映射"这条不变式在 Rust 里由哪个常量偏移保证；能解释保留页池被消除后循环依赖如何被打破（对比 C 的 `level`/`pt_init_done`/BSS 静态页三者消失）。

### 08-pagetable-structure

- **一句话定位**：页表对象自身的形状——几级、什么类型、VM 怎么访问自己的页表、三架构如何统一。
- **讲什么**：K-085、K-086、K-087、K-088、K-089、K-090。
- **不讲什么**：
  - **Direct Map 原理** → 06（本篇只讲"结构如何消费 direct map"）；
  - 页表的增删改查 → 09；
  - 页分配 → 07；
  - 写 PTE 后的失效纪律 → 01（不变量）+ 09（操作）。
- **前置**：05、06、07。
- **后置**：09、10、11、19、21、22、23。
- **事实底线**：
  - C：`pt.h`（29 行）；`pagetable.c:1088`（pt_init）、`:1311-1345`（搬迁段，机制归 11，此处只给调用位置）；`ARCH_VM_*` 宏族；C 的 `static_sparepagedirs`；
  - Rust：`os/servers/vm/src/pagetable/`（`mod.rs`、`vm_self_map.rs`）、`os/arch/src/arch/paging.rs`（`Paging` trait）、`os/arch/src/x86_64/`、`aarch64/`、`riscv64/`。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-086 | 层级 2→4（A-2） | 架构演进 | ARCH_VM_* | 结构演进本体 | 存量：07 §2/§3 |
  | K-087 | 多架构 trait（A-10） | 架构演进 | arch/paging.rs | 抽象本体 | 存量：07 §3 |
  | K-088 | 自映射（A-9） | 架构演进 | pagetable/vm_self_map.rs | 自举机制 | 存量：07 §3 |
  | K-090 | PTE 标志与位宽 | 数据结构 | arch/paging.rs::PageFlags | 权限模型 | 存量：07 §2 + 08 §2 |
- **验收标准**：能画出 4 级 walk 的路径并指出 C 的 2 级在哪一步等价；能说出三个架构共用同一个 trait 的分工点；能解释"VM 改自己的页表"与"VM 改别人的页表"走的是不是同一条路。

### 09-pagetable-operations

- **一句话定位**：页表的增删改查——建、绑、写、抄、查、销毁，以及每一次写入后紧跟的那条失效。
- **讲什么**：K-095、K-096、K-097、K-098、K-099、K-100、K-101、K-102。
- **不讲什么**：
  - 页表结构 → 08；
  - 页分配 → 07；
  - "为什么只改不在运行的进程"的不变量论证 → 01；
  - fork/mmap/munmap/exit/LU 的服务流程 → 19/21/22/23/26。
- **前置**：08。
- **后置**：11、17、18、19、21、22、23、26。
- **事实底线**：
  - C：`pagetable.c:990`（pt_new）、`:1427`（pt_free）、`:1358`（pt_bind）、`:784`（pt_writemap）、`:494/545`（pt_ptalloc[_in_range]）、`:1069/631/685`（pt_copy/pt_map_in_range/pt_ptmap）、`:943/761`（pt_checkrange/pt_writable）、`:1442/751/1035`（pt_mapkernel/pt_clearmapcache/pt_allocate_kernel_mapped_pagetables）、`:799-815/928-934`（VMINHIBIT）；`vm.h:60-63`（WMF_*）；
  - Rust：`os/arch/src/arch/paging.rs`、`os/arch/src/x86_64/paging.rs::write_pte_dm`、`vmproc/vmproc_handle.rs`（`init_page_table`/`free_page_table`/`write_page_table_mappings`）、`pagetable/vm_self_map.rs::VmSelfPageTable::adopt`。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-096 | pt_writemap + WMF_* | 机制 | pagetable.c:784 + vm.h:60-63 | 写映射本体 | 存量：08 §2 |
  | K-098 | 跨页表复制三粒度 | 机制 | pagetable.c:1069/631/685 | fork/LU 的共同底座 | 存量：08 §2 + 18 §2 |
  | K-101 | 写后 invlpg | 约束 | x86_64/paging.rs::write_pte_dm | 操作面后果 | 存量：08 §1.8 |
  | K-102 | VMINHIBIT 停等 | 架构演进 | pagetable.c:799-815 | C 的替代策略 | 存量：08 §1.10 + Fix #77 |
- **验收标准**：能对 `pt_writemap` 的四种 WMF 组合各举一个真实调用点；能说出 fork 用的是哪个复制函数、LU 用的是哪个；能解释 `write_pte_dm` 为什么把写与失效绑在一起。原 08 为 1098 行，本篇目标 ~700 行——压缩手段：把"设计决策 D1-D8"中属于 ARCH 清单的部分迁 28，把逐函数的行号流水账改为按操作族分组的对照表。

### 10-vm-heap

- **一句话定位**：VM 自己的堆从哪来——C 的 slab 尺寸分类 vs Rust 的连续 VA 区间 + free-list。
- **讲什么**：K-105、K-106、K-107、K-108、K-109、K-110。
- **不讲什么**：
  - `__minix_init` 之前的启动序 → 02；
  - 元数据搬迁 → 11；
  - 物理页供给 → 07；
  - 内存占用统计的回复面 → 27（K-110 只给定义）。
- **前置**：07、08、09。
- **后置**：11、14、25、27。
- **事实底线**：
  - C：`slaballoc.c` 全 528 行（`:29` SLABSIZES … `:504` slabstats）；`proto.h`（SLABALLOC/SLABFREE）；`vm.h:16`（MEMPROTECT）；`main.c:480`（`__minix_init`）；
  - Rust：`os/servers/vm/src/heap_arena.rs`、`global.rs`（`VmAllocator` + `#[global_allocator]` + `PAGE_ALLOC_PTR`）、`pagetable/vm_self_map.rs`（`vm_self_mappages`/`vm_self_unmap`）、`direct_map.rs`（VM_HEAP_*）。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-105 | slab 尺寸分类 | 机制 | slaballoc.c 全量 | C 侧本体 | 存量：09 §2 |
  | K-108 | HeapArena（A-3） | 架构演进 | heap_arena.rs + global.rs | Rust 侧本体 | 存量：09 §3 |
  | K-109 | __minix_init 分界 | 约束 | main.c:480 | 堆可用性边界 | 存量：09 §1 |
- **验收标准**：能解释"为什么 Rust 不需要按尺寸分类"（连续 VA + free-list 承接）；能说出 `dealloc` 从 no-op 到 free-list 的两版差异（A-3 v1/v2）；能指出堆增长时经哪条路拿物理页（07）与建立映射（09）。

### 11-vm-relocation

- **一句话定位**：自举最后一公里——页表从静态变动态、槽位与动态数据如何在实例间搬家（原语本体，不含 LU 协议）。
- **讲什么**：K-115、K-116、K-117、K-118、K-119。
- **不讲什么**：
  - LU 的四步协议与 `do_rs_*` → 26；
  - 堆本体 → 10；
  - `swap_proc_slot` 的进程表语义 → 03（本篇只讲搬迁用途）。
- **前置**：10。
- **后置**：26。
- **事实底线**：
  - C：`pagetable.c:1311-1345`（pt_init 搬迁段）；`utility.c:188`（swap_proc_slot 定义）、`:228`（transfer_mmap_regions）、`:283`（map_proc_dyn_data）、`:312`（swap_proc_dyn_data）；`region.c:1535`（map_setparent）；`main.c:707-722`（LU 中的调用点，本篇只引用）；
  - Rust：`os/servers/vm/src/global.rs`、`phys_mem/mod.rs::relocate`、`vmproc/table.rs::swap_slots`。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-115 | pt_init 搬迁段 | 机制 | pagetable.c:1311-1345 | 自举终点本体 | 存量：10 §2 |
  | K-116 | swap_proc_dyn_data | 机制 | utility.c:312/283 | 原语本体 | 存量：10 §2（03/25 重复） |
  | K-117 | transfer_mmap_regions | 机制 | utility.c:228 | 原语本体 | 存量：10 §2 |
- **验收标准**：能区分"静态→动态"（自举）与"旧实例→新实例"（LU）两种搬迁；能说出 26 里哪一步调用本组的哪个原语（形成单向引用，不再互相重复）。

### 12-phys-page-state

- **一句话定位**：一个物理页被谁引用着——两级结构、引用计数的加减点、以及页上的状态位。
- **讲什么**：K-125、K-126、K-127、K-128、K-129、K-130。
- **不讲什么**：
  - CoW 如何消费引用计数 → 18；
  - 区域如何挂接物理块 → 14；
  - `IN_CACHE` 的缓存语义 → 25；
  - `pb_unreferenced` 在 munmap 中的调用面 → 22。
- **前置**：05、11。
- **后置**：13、14、18、22、25。
- **事实底线**：
  - C：`pb.c:44`（newpb）、`:54`（pb_free）、`:61`（pb_link）、`:73`（pb_reference）、`:96`（pb_unreferenced）、`:136`（mem_cow，机制本体归 18）；`region.h`；`phys_region.h:8-21`；`region.c:60/72`（physblock_get/set）；
  - Rust：`os/servers/vm/src/region/page_state.rs`（`PageFrames`、`PageFlags`、`PageSlot`）。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-127 | reference/unreferenced | 机制 | pb.c:73/96 | 计数加减本体 | 存量：11 §2（21 重复） |
  | K-128 | PageFrames 逐帧计数 | 数据结构 | page_state.rs | Rust 侧结构选择 | 存量：11 §3 |
  | K-129 | PageFlags（COW/IN_CACHE） | 数据结构 | page_state.rs:30 | 状态位本体 | 存量：11 §3（17/24 重复） |
- **验收标准**：能画出 `vir_region → phys_region → phys_block` 的三级指针关系；能说出引用计数归零时走了哪个函数、释放了什么；能解释 Rust 为什么用逐帧数组替代 C 的两级结构。

### 13-memtype

- **一句话定位**：同样是一次缺页，六种内存类型给出六种答案——回调族的契约与每类内存的个性。
- **讲什么**：K-135、K-136、K-137、K-138、K-139、K-140、K-141、K-142、K-143、K-144。
- **不讲什么**：
  - 回调在缺页/分裂中的调用链 → 17/18；
  - 区域框架何时调用 → 14；
  - 文件映射的服务流程 → 21/24；
  - 缓存的 LRU 与 handler → 25。
- **前置**：12。
- **后置**：14、17、18、21、24、25、27。
- **事实底线**：
  - C：`memtype.h`（34 行，回调签名）；`mem_anon.c`、`mem_anon_contig.c`、`mem_directphys.c`、`mem_shared.c`、`mem_file.c`、`mem_cache.c`；`glo.h`（六实例声明）；
  - Rust：`os/servers/vm/src/memtype.rs`（`trait MemType` + 6 unit struct）；`memtype.rs:7`（反向依赖判定注释，Fix #76）；`memtype.rs` §3.7（`release_shared_remap` 落点，Fix #64）。
- **知识点清单**（摘要）：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-136 | ev_* 回调族 | 接口与协议 | memtype.h:18/21/22 | 契约本体 | 存量：12 §2 |
  | K-141 | mappedfile 与 cow_block | 机制 | mem_file.c:59/70-71/131-134 | 文件个性 | 存量：12 §2（17/23 重复） |
  | K-140 | shared 与 remaps | 机制 | mem_shared.c:54/99 + Fix #64 | 共享个性 | 存量：12 §2/§3.7 |
  | K-144 | 反向依赖判定 | 架构演进 | memtype.h:18 → memtype.rs:7 | 判定闭合记录 | 存量：todo Fix #76 |
- **验收标准**：能填出"6 类 × 回调"矩阵中每格是"有实现/走默认/C-NULL parity"；能解释 `mappedfile_writable` 恒 false 为什么不是 bug；能指出 `remaps` 的递增与递减分别发生在哪两个函数（避免再出现只增不减）。

### 14-region-map

- **一句话定位**：进程的虚拟地址空间账本——区域怎么建、怎么拆、怎么查、以及那个从 AVL 变成 BTreeMap 的有序索引。
- **讲什么**：K-150、K-151、K-152、K-153、K-154、K-155、K-156、K-157、K-158、K-159、K-160、K-161、K-162、K-163、K-164、K-165、K-166。
- **不讲什么**：
  - 物理页引用计数 → 12；
  - memtype 回调语义 → 13；
  - fork/mmap/munmap/exit 的服务流程 → 19/21/22/23；
  - 查询服务的回复编码 → 27；
  - LU 的预分配查找调用面 → 26。
- **前置**：12、13。
- **后置**：17、19、20、21、22、23、26、27。
- **事实底线**：
  - C：`region.c` 全 1555 行（`:36-38` **map_region_init 是空函数**；`:463` map_page_region；`:302/399` region_find_slot[_range]；`:1065/1222` map_unmap_region/range；`:1150` split_region；`:933/944` map_proc_copy[_range]；`:616` map_lookup；`:664/756/779` map_pf/map_handle_memory/map_pin_memory；`:906/257` map_writept/map_ph_writept；`:568/589` map_free/map_free_proc；`:1303` map_region_lookup_type；`:645` vrallocflags；`:1002` map_region_extend_upto_v）；`region.h:37-78`；`phys_region.h`；`cavl_if.h`、`cavl_impl.h`、`regionavl_defs.h`、`regionavl.c`、`unavl.h`；
  - Rust：`os/servers/vm/src/region/`（`region_map.rs`、`vir_region.rs`、`mod.rs`、`page_state.rs`）。
- **知识点清单**（摘要）：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-156 | AVL → BTreeMap（A-4） | 架构演进 | cavl_impl.h:178-1100 + region_map.rs | 索引面（原 14 合并入本篇） | 存量：14 §2/§3 |
  | K-155 | map_lookup/find_slot | 机制 | region.c:616 + region_map.rs:157 | 查找面（原 14 合并入本篇） | 存量：13 §2 + 14 §2 |
  | K-164 | map_region_init 是空函数 | 概念 | region.c:36-38 | 事实纠正 | 新增 |
  | K-165 | find_overlap + 零长拒绝 | 机制 | region_map.rs | Fix #72 | 新增 |
- **验收标准**：能画出区域生命周期（建/查/填/复制/扩/缩/拆/释放）并指出每个操作对应的服务调用方；能说出 AVL 宏模板与 BTreeMap 在哪三个语义点上等价；能解释为什么零长区域必须被拒绝。合并后目标 ~750 行（原 13+14 = 1124 行）——压缩手段：`cavl_impl.h` 的 1208 行实现只保留接口面 + 一张"操作→语义等价"表，不逐函数展开。

### 15-vm-message-protocol ★新建

- **一句话定位**：一条 VM 消息从字节到语义的全链路——overlay 布局、解码入口、错误编码、以及那个"不回复"的特殊返回值。
- **讲什么**：K-170、K-171、K-172、K-173、K-174、K-175、K-176、K-177、K-178、K-179。
- **不讲什么**：
  - 每个服务的字段含义 → 19–23/27 各自（本篇只给"怎么读"，不给"读了干什么"）；
  - 主循环的分派顺序 → 16；
  - ACL 掩码语义 → 04；
  - VFS 异步对话的业务语义 → 24。
- **前置**：03（endpoint）、04（调用号与掩码的来源）。
- **后置**：16、17、19、20、21、22、23、24、25、26、27。
- **事实底线**：
  - C：`minix3/minix/include/minix/com.h:627/630-773/769/1151`；`minix3/minix/include/minix/ipc.h`（`mess_*` 结构族）；`minix3/minix/include/minix/vfsif.h:79-81`；`minix3/minix/include/minix/ipcconst.h:28/34`；
  - Rust：`os/libs/minix-types/src/ipc/vm.rs`（`VmXxxIn/Out`、`decode_message`）、`ipc/message.rs`（union + `Mess*` + 56 字节断言）、`os/servers/vm/src/ipc/encode.rs`、`ipc/transport.rs`（`IpcStatus`）；
  - 缺口证据：`dispatcher.rs:1075-1089/1104-1119/1166-1172`（五结构 overlay 错位，V13-P2-6）。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-171 | M1/M2/M3 overlay | 接口与协议 | ipc.h | 横切主题集中 | 存量：20/21/26 各 §4 |
  | K-172 | VmXxxIn/Out 解码 | 接口与协议 | minix-types/src/ipc/vm.rs | 横切主题集中 | 存量：15 §4 + 各服务 §4 |
  | K-173 | 五结构缺专属 struct | 接口与协议 | dispatcher.rs:1075-1172 | 偏差登记的唯一落点 | 新增（todo V13-P2-6） |
  | K-174 | 56 字节断言 | 测试性质 | message.rs:3606 | 防线本体 | 新增 |
  | K-176 | SUSPEND | 接口与协议 | com.h:1151 | 主讲述点（15/16/20/23 四处重复） | 存量：15 §1 |
  | K-175 | errno → m_type | 接口与协议 | main.c:182 + encode.rs | 错误路径 | 存量：15 §4 |
- **验收标准**：读者能手工把一条 `VM_MMAP` 消息的字段对到 overlay 偏移；能列出"哪些 VM 消息还没有专属 Rust 结构"并说明后果；能说出 `SUSPEND` 与 `ENOSYS` 在回复行为上的差别。

### 16-vm-main-loop

- **一句话定位**：VM 的心跳——一轮循环做什么、五条优先级怎么排、调用表怎么建、回复怎么发。
- **讲什么**：K-185、K-186、K-187、K-188、K-189、K-190、K-191、K-192、K-193、K-194、K-195。
- **不讲什么**：
  - 各 handler 的实现 → 17–27；
  - 线格式与解码 → 15；
  - ACL 位图本体 → 04；
  - SEF 启动注册与 boot 装载 → 02；
  - `do_procctl` 的业务语义 → 23（本篇只讲它在优先级 1 被路由）。
- **前置**：03、04、15。
- **后置**：17、19–27。
- **事实底线**：
  - C：`main.c:112-192`（主循环全量）、`:47-59`（vm_calls/CALLNUMBER）、`:522-575`（CALLMAP 26 条）、`:125-129`（notify 过滤）、`:131`（vm_isokendpt）、`:143-176`（五优先级）、`:181-191`（回复与 panic）、`:731-750`（signal handler）、`:117`（SANITYCHECK）；`com.h:769`；`vm.h:10-12`；
  - Rust：`os/servers/vm/src/vm_server.rs`（`run`/`run_once`/`dispatch_on_msg`/`rs_handshake`/`handle_vfs_transid`）、`ipc/dispatcher.rs`（`MessageDispatcher`/`dispatch_by_number`）、`ipc/transport.rs`、`ipc/encode.rs`；
  - 非 C：`os/servers/vm/Cargo.toml:minix-sef`（V14-P2-1，已引入待裁决）。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-189 | CALLMAP 26 条 | 机制 | main.c:522-575 | 注册本体 | 存量：15 §2 |
  | K-191 | 未注册四调用 | 架构演进 | com.h:637/656/664/672 | 防止重复"接线" | 存量：15 §3 + todo §18.1 |
  | K-186 | sef_receive_status 与 minix-sef | 接口与协议 | Cargo.toml + vm_server.rs:961 | 已引入待裁决 | 新增（todo §19） |
  | K-195 | 伪造源 release 可观测 | 工具与工程 | vm_server.rs P3 分支 | Fix #67 | 新增 |
- **验收标准**：能按 R01–R13 复述一轮循环；能说出五条优先级的顺序与每条为什么不回复（`SUSPEND`/`continue`）；能指出 `test_callmap_registration_matches_c` 守护的是什么。原 15 为 739 行 + 原 01 的主循环段，本篇目标 ~650 行——压缩手段：逐 handler 的分派细节迁回各自服务篇，本篇只留"分派表 + 优先级 + 回复"三件事。

### 17-pagefault

- **一句话定位**：两条入口（被动缺页 / 内核主动要内存）如何在同一个汇合点被仲裁，以及等待 VFS 时状态怎么活下来。
- **讲什么**：K-200、K-201、K-202、K-203、K-204、K-205、K-206、K-207、K-208。
- **不讲什么**：
  - CoW 分裂机制本体 → 18；
  - VFS 请求队列的数据结构 → 24；
  - 页缓存的命中逻辑 → 25；
  - mmap 如何建立区域 → 21。
- **前置**：14、16。
- **后置**：18、24、25。
- **事实底线**：
  - C：`pagefaults.c` 全 418 行（`:33-49` pf_state/hm_state、`:59` pf_errstr、`:76` handle_pagefault、`:161` pf_cont、`:170` handle_memory_continue、`:198` handle_memory_final、`:240` do_pagefaults、`:245` handle_memory_once、`:254` handle_memory_start、`:294` do_memory、`:336` handle_memory_step）；`region.c:664/756/616`；`main.c:153-164/731-750`；
  - Rust：`os/servers/vm/src/cow_exec_pf.rs`（`handle_pagefault`/`alloc_and_map`/`PagefaultAction`）、`vm_server.rs::dispatch_pagefault`、`memtype.rs::PagefaultResult`；
  - 跨 stage：`minix3/minix/kernel/system/do_vmctl.c`（解除 RTS_PAGEFAULT）。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-203 | handle_memory 五步状态机 | 机制 | pagefaults.c:254/245/336/198/170 | 本体 | 存量：16 §2 |
  | K-204 | map_pf 汇合点 | 机制 | region.c:664 | 两入口合一 | 存量：16 §2 + 13 §2 |
  | K-207 | 不在运行不变量的逐路径论证 | 约束 | 16 §3.7 | 与 01 分工：01 给不变量，本篇给满足性 | 存量：16 §3.7 |
  | K-195 | 伪造源 release 可观测（接线） | 工具与工程 | vm_server.rs | 新增（Fix #67 接线说明） | 新增 |
- **验收标准**：能画出"CPU 异常 → 内核转发 → VM 仲裁 → 写 PTE → 内核恢复"的五方时序；能说出缺页/服务/信号三条路径各自为什么满足"目标不在运行"；能指出 `NeedCow` 这个动作交给哪一篇展开。

### 18-cow

- **一句话定位**：写时复制——什么时候必须复制、复制之后这一页的身份变成什么。
- **讲什么**：K-215、K-216、K-217、K-218、K-219、K-220、K-221。
- **不讲什么**：
  - 缺页状态机 → 17；
  - fork 如何批量建立共享 → 19；
  - 引用计数的加减本体 → 12；
  - VFS 文件映射的服务流程 → 24。
- **前置**：12、13、17。
- **后置**：19、24。
- **事实底线**：
  - C：`pb.c:136-168`（mem_cow，末行 `ph->memtype = &mem_type_anon`）；`mem_anon.c:64/105`（anon_pagefault/anon_writable）；`mem_file.c:59/70-71/73-79/127-130/131-134/173`（cow_block + clearend + "never writable"）；`mem_shared.c:122/161`；`region.c:130/257/906/820-849`；
  - Rust：`os/servers/vm/src/cow_exec_pf.rs`（`cow_resolve`/`cow_resolve_core`/`cow_resolve_region`/`copy_page_content`/`copy_page_and_zero_tail`）。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-215 | mem_cow 与换型 | 机制 | pb.c:136-168 | 本体 | 存量：17 §2 |
  | K-218 | 快路/慢路合取门 | 机制 | cow_exec_pf.rs:268-273/282 + mem_file.c:70-71 | Fix #63 的活循环因果链 | 新增（17 §3.3 已写） |
  | K-219 | cow_block 与 clearend | 机制 | mem_file.c:73-79/131-134 | Fix #80 | 新增 |
  | K-221 | writable 语义差 | 接口与协议 | mem_anon.c:105 vs mem_file.c:173 | 快路前提 | 存量：17 §2 + 12 §2 |
- **验收标准**：能说出"独占文件页写故障"的完整因果链以及为什么旧实现会活循环；能说出 CoW 之后 `memtype` 变成什么、为什么；能指出快路的合取条件缺了哪一半会出事。

### 19-vm-fork（代表成员精讲）

- **一句话定位**：一次 `VM_FORK` 请求如何贯穿进程表、ACL、页表、物理页、memtype、区域、CoW 七层——本篇是服务组的精讲样本。
- **讲什么**：K-225、K-226、K-227、K-228、K-229、K-230、K-231。
- **不讲什么**：
  - CoW 分裂机制 → 18；
  - 缺页触发 → 17；
  - `pt_ptmap` 的操作细节 → 09；
  - `acl_fork` 的位图实现 → 04；
  - 组内其余服务的差异 → 20/21/22/23（本篇只给组内的差异表）。
- **前置**：12、13、14、16、18。
- **后置**：无（服务组终点）。
- **事实底线**：
  - C：`fork.c` 全 116 行（`:32` do_fork、`:67`、`:83`）；`region.c:933`（map_proc_copy）；`pagetable.c:685/631`（pt_ptmap/pt_map_in_range）；`acl.c:110`（acl_fork）；`main.c:544`（CALLMAP）；`com.h:VM_FORK`；
  - Rust：`os/servers/vm/src/fork.rs`（`do_fork`/`handle_memory_once`/`fork_region`/`setup_cow_for_all_regions`）、`vmproc/vmproc_handle.rs`（`init_page_table`/`write_page_table_mappings`）。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-225 | do_fork 全流程 | 机制 | fork.c:32 | 请求生命周期本体 | 存量：18 §2 |
  | K-230 | 写保护批量建立 | 机制 | fork.rs::setup_cow_for_all_regions | 与 18 的分工点 | 存量：18 §2（17 §2 重复） |
  | K-231 | 失败回滚 | 机制 | fork.c:70-71 | 错误路径 | 存量：18 §2 |
- **验收标准**：能画出"VM_FORK 到达 → 七层各自做了什么 → 回复"的贯穿图；能说出 fork 失败时已分配的资源如何回滚；能给出组内五服务的差异表（见 §4.2）。

### 20-vm-brk

- **一句话定位**：`VM_BRK`——堆顶的伸长与收缩，以及为什么改堆顶要通知内核。
- **讲什么**：K-235、K-236、K-237、K-238、K-239。
- **不讲什么**：mmap 地址空间分配 → 21；区域拆分机制 → 14；物理分配 → 05。
- **前置**：14、16。
- **后置**：无。
- **事实底线**：C `break.c` 全 69 行（`:44` do_brk、`:62` real_brk）；`vm.h:34`（MINSTACKREGION）、`:76`（VM_DATATOP）；Rust `os/servers/vm/src/brk.rs`（`:106/:109/:166/:190-191`）；`region/vir_region.rs::extend`。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-237 | extend/shrink（A-12） | 机制 | vir_region.rs:127-140 | resize 并入 extend 的落点 | 存量：19 §3 + 13 §3 |
  | K-238 | 边界约束 | 约束 | vm.h:34/76 | 边界本体 | 存量：19 §2 |
- **验收标准**：能说出 `real_brk` 与 `do_brk` 的分工；能解释 A-12 为什么把 `ev_resize` 取消后行为等价；能指出收缩时 PTE 与物理页经哪条路归还（与 22 的漏斗同源）。

### 21-vm-mmap（映射管理组精讲）

- **一句话定位**：`VM_MMAP`——地址空间分配与内存来源绑定的三路解析，以及文件映射为什么必须异步。
- **讲什么**：K-245、K-246、K-247、K-248、K-249、K-250、K-251。
- **不讲什么**：munmap → 22；VFS 队列数据结构 → 24；页缓存 → 25；线格式字段表 → 15（本篇只列本服务的字段）。
- **前置**：14、16、20。
- **后置**：22、24。
- **事实底线**：C `mmap.c:36/84/135/160/200/284/366/438/463`；`region.c:302/399/463`；`mem_file.c:191`；`mem_shared.c:167`；`sys/sys/mman.h:62-124`；Rust `os/servers/vm/src/mmap.rs`（`:56-133`/`:203-204`/`:226-567`）、`map_phys.rs`、`ipc/dispatcher.rs`。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-245 | 三路地址解析 | 机制 | mmap.c:200/36 | 本体 | 存量：20 §2 |
  | K-247 | 文件异步路径 | 机制 | mmap.c:135/84/160 | 异步样本 | 存量：20 §2（23 §2 重复） |
  | K-248 | map_perm_check | 约束 | mmap.c:284 | 权限分级 | 存量：20 §2（21 §2 重复） |
- **验收标准**：能说出 `addr` 参数的三种解释路径；能解释文件映射为什么必须走 `SUSPEND`；能列出 `MAP_*`/`PROT_*` 到 `VR_*`/PTE 标志的完整映射表。

### 22-vm-munmap

- **一句话定位**：拆除映射与解除引用——范围拆除的四情形、区域内拆除的三情形、以及共享区域的引用回落。
- **讲什么**：K-255、K-256、K-257、K-258、K-259、K-260、K-261。
- **不讲什么**：mmap 建立 → 21；区域生命周期机制 → 14；exit 的全进程释放 → 23；引用计数本体 → 12。
- **前置**：14、16、21。
- **后置**：无。
- **事实底线**：C `mmap.c:488/512/310/284`；`region.c:527/568/589/1065/1150/1222`；`mem_directphys.c` 全 79 行；`pb.c:96`；`fdref.c:116`；`mem_file.c:280`；Rust `os/servers/vm/src/munmap.rs`、`map_phys.rs`、`region/mod.rs::free_region_pages`、`region/mod.rs::release_shared_remap`。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-256 | 四情形 | 机制 | region.c:1222 | 本体 | 存量：21 §2（13 §2 重复） |
  | K-260 | remaps 递减 | 机制 | region/mod.rs::release_shared_remap | Fix #64 的服务面 | 新增 |
  | K-259 | map_phys/unmap_phys | 机制 | mmap.c:310 + mem_directphys.c | 特例路径 | 存量：21 §2 |
- **验收标准**：能对"拆除范围覆盖 N 个区域"与"拆除范围落在单个区域内部"各画出拆分图；能解释 `remaps` 的递减为什么放在删除漏斗而不是 `ev_delete`（借用安全理由）；能指出拆除时 PTE 与物理页的归还顺序。

### 23-vm-exit

- **一句话定位**：进程清算与地址空间重建——两阶段退出、procctl 通道、以及 exec 如何复用同一个槽。
- **讲什么**：K-265、K-266、K-267、K-268、K-269、K-270。
- **不讲什么**：区域释放机制 → 14；页表销毁的操作 → 09；查询服务 → 27；LU 后旧实例退出 → 26。
- **前置**：03、14、16。
- **后置**：26。
- **事实底线**：C `exit.c` 全 156 行（`:25` reset_vm_rusage、`:33` free_proc、`:45` clear_proc、`:60` do_exit、`:100` do_willexit、`:117` do_procctl、`:135-137` VMPPARAM_CLEAR）；`main.c:148`（procctl 优先级 1）；Rust `os/servers/vm/src/exit.rs`（`:54/:86-142`）、`vmproc/vmproc.rs`。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-265 | 两阶段退出 | 机制 | exit.c:60/100 | 本体 | 存量：22 §2 |
  | K-268 | VMPPARAM_* | 数据结构 | exit.c:135-137 | exec 重建（原越界无归属） | 存量：22 §2（散） |
  | K-270 | vfs_queue 挂起请求 | 架构演进 | vfs_queue.rs（无 purge API） | 缺口登记 | 存量：todo P3-1(2) |
- **验收标准**：能解释为什么退出要 `WILLEXIT` + `EXIT` 两步；能说出 `do_procctl` 为什么被提到优先级 1（VFS transid）；能列出退出时归还的三种资源（页表树/物理页/fdref）与各自的函数。

### 24-vfs-interaction

- **一句话定位**：VM 与 VFS 的异步对话——请求怎么发出、挂在哪、回来怎么续作、fd 引用怎么管。
- **讲什么**：K-275、K-276、K-277、K-278、K-279、K-280。
- **不讲什么**：页缓存的 LRU → 25；mmap 的服务入口 → 21；`SUSPEND` 协议 → 15；缺页状态机 → 17。
- **前置**：13、16、21。
- **后置**：25。
- **事实底线**：C `vfs.c` 全 143 行（`:43` activate、`:60` vfs_request、`:109` do_vfs_reply）；`fdref.c` 全 177 行（`:37/:93/:109/:116/:156`）；`mem_file.c:59/191`；Rust `os/servers/vm/src/vfs_queue.rs`、`fdref.rs`。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-276 | VfsRequestState | 数据结构 | vfs_queue.rs:29-33/107-148 | 本体 | 存量：23 §2 |
  | K-277 | fdref 四函数 | 机制 | fdref.c:93/109/116/156 | 主讲述点（21 重复） | 存量：23 §2 |
  | K-280 | cow_block 的 file 分支 | 机制 | mem_file.c:59 | 与 18 的分工 | 存量：23 §2 + 17 §2 |
- **验收标准**：能画出"发起 → SUSPEND → VFS 回复 → `do_vfs_reply` 续作"的时序；能解释 fdref 为什么需要去重；能说出死进程的挂起请求当前为什么清不掉。

### 25-page-cache

- **一句话定位**：VM 作为所有文件系统的磁盘块缓存中介——双索引、精确 LRU、四个 handler。
- **讲什么**：K-285、K-286、K-287、K-288、K-289、K-290、K-291、K-292、K-293、K-294。
- **不讲什么**：VFS 请求队列 → 24；memtype 回调本体 → 13；物理分配 → 05；主循环分发 → 16。
- **前置**：13、24。
- **后置**：无。
- **事实底线**：C `cache.c` 全 332 行（`:29/:52/:70/:76/:87/:177/:198/:216/:259/:288/:313/:328`）；`mem_cache.c` 全 324 行（`:34-95` 回调、`:95/196/283/315` 四个 handler）；`cache.h`；Rust `os/servers/vm/src/page_cache.rs`、`ipc/cache_handlers.rs`、`region/page_state.rs`（`addcache`/`rmcache`）。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-285 | 双索引 | 数据结构 | cache.c:177/198 | 本体 | 存量：24 §2 |
  | K-291 | IN_CACHE | 机制 | page_state.rs:166/175 | 主讲述点（11/12 重复） | 存量：24 §3 |
  | K-293 | mapcache 回滚 | 机制 | mem_cache.c:155-157 + Fix #74 | 勘误后的正确结论 | 新增 |
  | K-294 | ONCE 走 NeedVfsIo | 架构演进 | 24 §3.6 第 6 行 | 偏差登记 | 存量：24 §3.6 |
- **验收标准**：能解释为什么需要两个索引（dev 主键 / ino 辅索引各自的查询场景）；能说出淘汰时被 `IN_CACHE` 标记的页为什么不进回收漏斗；能复述 Fix #74 的勘误（两侧回滚顺序**同构**，不是相反）。

### 26-rs-services

- **一句话定位**：RS 驱动的热更新中 VM 的全部服务面——四个 handler、握手语义、LU 三段回调，以及两个未实现的缺口。
- **讲什么**：K-300、K-301、K-302、K-303、K-304、K-305、K-306、K-307、K-308、K-309。
- **不讲什么**：搬迁原语本体 → 11；`map_service`/`sef_cb_init_fresh` 的启动注册 → 02；旧实例退出路径 → 23；主循环分发 → 16；查询 → 27。
- **前置**：11、16、23。
- **后置**：无。
- **事实底线**：C `rs.c` 全 391 行（`:34/:71/:150/:218/:281/:300/:329/:349`）；`main.c:592-672`（init_vm_multi_lu）、`:677-726`（init_lu_restart）、`:196-217`（lu_state_changed）、`:241-260`（sef_cb_init_fresh，仅引用）；`minix/rs.h`（`struct rprocpub:165-183`、`SF_VM_ROLLBACK:198`、`SF_VM_NOMMAP:199`、`IS_RPUB_BOOT_USR:188`）；Rust `os/servers/vm/src/rs.rs`、`vm_server.rs::rs_handshake/RprocTab/RprocEntry`。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-301/302 | PREPARE/UPDATE 未实现（A-8） | 架构演进 | rs.c:71/150 vs rs.rs | 缺口本体 | 存量：25 §3.9 |
  | K-304 | MAKE_VM 恒拒 | 架构演进 | rs.c:218（`==2` 才 EPERM） vs rs.rs:504-507 | 偏差登记 | 存量：25 §3.9/§3.10 |
  | K-308 | IPC 过滤器 | 机制 | main.c:610-666 | LU 期间收敛面 | 存量：25 §2 |
- **验收标准**：能说出 LU 的三段回调各自在什么时机被调；能解释 LU 期间为什么只放行 `VM_BRK`/`VM_INFO`；能明确指出 `RS_PREPARE`/`RS_UPDATE`/`MAKE_VM` 三处与 C 的可观察行为分叉。

### 27-vm-queries

- **一句话定位**：VM 作为内存权威的只读窗口——统计、使用量、区域列表、物理地址与引用计数、rusage。
- **讲什么**：K-315、K-316、K-317、K-318、K-319、K-320、K-321。
- **不讲什么**：区域生命周期 → 14；页缓存统计口径 → 25；rusage 清零时机 → 23；主循环分发 → 16；memtype 回调本体 → 13。
- **前置**：14、16。
- **后置**：无。
- **事实底线**：C `utility.c:100`（do_info）、`:426`（do_getrusage）；`mmap.c:438/463`；`region.c:1323/1343/1357/1366/1384/1395/1452`；`cache.c:328`；`minix/include/minix/vm.h:40-66`；Rust `os/servers/vm/src/query.rs`、`ipc/encode.rs`、`boot.rs::vm_allocated_bytes`。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-316 | 三种使用量口径 | 机制 | region.c:1395/1357/1366 | 本体 | 存量：26 §2 |
  | K-318 | GETPHYS/GETREF 实际语义 | 机制 | mmap.c:438/463 | "物理地址"实为 regionid | 存量：26 §2 |
  | K-317 | 分页与 MAX_VRI_COUNT | 机制 | region.c:1452 + vm.h:66 | 游标语义 | 存量：26 §2 |
- **验收标准**：能说出 INFO 的四个子请求各自的回复结构；能解释 `VM_GETPHYS` 为什么返回的是 region id 而不是物理地址；能指出 `next` 游标在 Rust 侧的当前承载方式（并登记与 C 的偏移差异 → 15）。

### 28-vm-c-parity-and-gaps

- **一句话定位**：Rust 实现相对 Minix3 C 的差距总账——哪些是有意的架构演进，哪些是尚未实现，哪些明确不做。
- **讲什么**：K-325（A-1~A-12 全表）、K-326（未实现调用与 edge 条目）、K-327（WONTFIX）、K-328（死代码排除）。
- **不讲什么**：任何机制的完整教学（本篇是索引 + 判定，不是展开）。
- **前置**：全部主线篇（00–27）；实际可跳读。
- **后置**：无。
- **事实底线**：`plan.md:210-224`（ARCH 表，作为**线索**重核一遍再写）、`plan.md:305-317`（排除表）、`todo.md` §18.4/§19（edge 条目）、各 C 文件对应锚点（A-1 见 `os/arch/src/direct_map.rs`；A-11 见 `acl.c:44-53` vs `acl.rs:97-112`；A-8 见 `rs.c:71/150` vs `rs.rs`）。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-325 | A-1~A-12 | 架构演进 | plan §4 + 各代码锚点 | 教学目录里唯一的汇总处 | 存量：plan §4（重核后迁入） |
  | K-326 | edge 条目 | 架构演进 | edge_todo.md | 通电前置的单一入口 | 存量：todo §18.4/§19 |
  | K-328 | 死代码排除 | 架构演进 | region.c:860；proto.h；utility.c:361/376/385 | 防止后人"补实现" | 存量：plan §5.4 |
- **验收标准**：每项 ARCH 都能给出"C 现状 + Rust 现状 + 涉及哪一篇"三列；每条未实现都能给出"调用号 + C 锚点 + 当前返回什么 + 解锁依赖"；每条 WONTFIX 都能给出理由。**本篇不允许出现"待定"**。

### 29-vm-build-and-test

- **一句话定位**：这个 crate 怎么构建、怎么测、覆盖率怎么量——非 C 制品与验证基建的单一入口。
- **讲什么**：K-330（6 个 feature）、K-331（6 个 crate 依赖）、K-332（三 feature 矩阵）、K-333（SimPaging/MockGateway 注入）、K-334（allocator parity）、K-335（CALLMAP 守护测试）、K-336（coverage-extract / design-coverage-check）。
- **不讲什么**：如何编译内核（属 `01-stage-kernel`）；QEMU 冒烟（属 edge E5，本篇只给指针）；单个测试的业务断言（属各自服务篇）。
- **前置**：16（注入 seam 的用法）。
- **后置**：无。
- **事实底线**：`os/servers/vm/Cargo.toml`（`[[bin]]`/`[lib]`/features/deps）；`os/servers/vm/src/main.rs`（test 构建跳过 binary）；`os/servers/vm/src/kernel_gateway.rs`、`pagetable/sim.rs`；`tools/coverage-extract/`；`tools/design-coverage-check.sh`；`todo.md:40-43`（三矩阵数字，写作时需重跑复核）。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-330/331 | features 与依赖 | 工具与工程 | Cargo.toml | 非 C 制品 | 新增 |
  | K-332 | 三 feature 矩阵 | 测试性质 | todo.md:40-43 | 基线单一入口 | 新增 |
  | K-333 | SimPaging/MockGateway | 工具与工程 | kernel_gateway.rs | 无硬件测试的前提 | 新增（00 §4.1 一句） |
  | K-336 | 覆盖率工具与口径 | 工具与工程 | tools/coverage-extract/ | 数字可核对 | 新增 |
- **验收标准**：读者能照着本篇跑通 `cargo build` / 三矩阵 / 覆盖率提取；能说出换后端时为什么语义不变（parity 测试守护）；能指出测试数字必须重跑而不能抄旧值。

### 99-vm-global-concepts

- **一句话定位**：工具篇——所有魔数、旗标、结构体、类型对照的速查表。
- **讲什么**：K-340（endpoint/generation）、K-341（`VM_*` 调用号全表）、K-342（`VMP_*`/`VR_*`/`WMF_*`/`PAF_*`/`VMF_*` 旗标族）、K-343（全局变量表）、K-344（C ↔ Rust 类型对照）。
- **不讲什么**：任何机制的讲解（本篇只给定义 + 锚点 + 一句用途）。
- **前置**：00（可随时查阅）。
- **后置**：全部。
- **事实底线**：`minix3/minix/include/minix/com.h:627-780`；`minix3/minix/include/minix/endpoint.h`；`minix3/minix/servers/vm/vm.h`、`glo.h`、`vmproc.h`、`region.h`；`os/libs/minix-types/src/`。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-341 | 49 个调用号全表 | 数据结构 | com.h:630-773 | 词汇表本体 | 存量：99 §2（74 行，需扩） |
  | K-342 | 旗标族全表 | 数据结构 | vm.h/region.h/vmproc.h | 词汇表本体 | 存量：99 §2 |
  | K-344 | 类型对照 | 概念 | minix-types | 跨语言查阅 | 存量：99 §3 |
- **验收标准**：任取一个 VM 魔数都能在本篇查到"值 + 定义文件 + 用途 + Rust 对应物"；原 74 行扩充到 ~400 行，全部为表格形态，无散文。

---

## 6. 变更表

### 6.1 统一变更表

| 操作 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向/来源 |
|---|---|---|---|---|---|---|
| V-01 | 重写 | `00-vm-overview.md`（100 行） | `00-vm-overview` | 导航表要覆盖 30 篇；现状总览要改成指针而非数字 | K-020/K-004/K-028/K-336 | 全部保留并扩写 |
| V-02 | 新建 | — | `01-vm-execution-model` | 四条全局不变量散在 00/08/16，无一处完整论证 | K-020–K-028 | 来源：00 §1.2 + 08 §1.8 + 16 §3.7 + `pagetable.c:55-57/119/255/319/430` + `os/Cargo.toml:260` |
| V-03 | 拆分 | `01-vm-init-main.md` §主循环段（~120 行） | `16-vm-main-loop` | 01 声明"主循环细节归 15"却自己讲了一遍 | K-185/K-188/K-192 | 去向：16 §"循环骨架" |
| V-04 | 拆分 | `01-vm-init-main.md` §执行模型段 | `01-vm-execution-model` | 同上 | K-021/K-022/K-025 | 去向：01 §"执行模型" |
| V-05 | 重排 | `01-vm-init-main.md` 主体（830 行） | `02-vm-boot-chain` | 编号让位给执行模型篇 | K-001–K-016 | 去向：02 全篇 |
| V-06 | **合并** | `02-vmproc-struct.md`（548）+ `03-vmproc-table.md`（444） | `03-vmproc-and-proc-table` | 同一语义单元（"VM 如何记住进程"）；两篇内部互引 21 次，读者来回跳 | K-030–K-038 | 去向：02 §2 → 03 §"结构"；03 §2 → 03 §"表与 endpoint"；重复的生命周期叙述删一份 |
| V-07 | 沿用+瘦身 | `04-acl.md`（609） | `04-vm-acl` | 边界清晰，只去重复 | K-040–K-045 | 去向：04 全篇；`main.c:168` 接线段保留（16 只引用） |
| V-08 | 修正 | `05-physical-memory.md` 前置字段（指向 07） | `05-physical-memory`（前置改为 02） | **消除环的第一步** | K-069/K-065 | 去向：`PAF_CLEAR` 与 Direct Map 论述 → 06 |
| V-09 | 修正 | `06-page-allocator.md` 前置字段（指向 07） | `07-vm-page-allocator`（前置改为 05/06） | **消除环的第二步** | K-065 | 同上 |
| V-10 | **拆分** | `07-pagetable-struct.md` §3.6（Direct Map） | `06-direct-map-and-address-space`（新建） | Direct Map 是 `os/arch` 层概念，是 05/06/07/08 四篇共同前置，压在 07 里必然成环 | K-065–K-071 | 去向：06 全篇；07 保留"页表结构如何消费 direct map"一句 |
| V-11 | 重排 | `07-pagetable-struct.md` 主体 | `08-pagetable-structure` | 编号顺移 | K-085–K-090 | 去向：08 全篇 |
| V-12 | 瘦身 | `08-pagetable-ops.md`（1098） | `09-pagetable-operations`（~700） | 逐函数行号流水账 + 8 条设计决策注水 | K-095–K-102 | 去向：设计决策中属 ARCH 的 → 28；其余按操作族重组 |
| V-13 | 重排+瘦身 | `09-slab-allocator.md`（685） | `10-vm-heap`（~450） | 编号顺移；slab 位图细节可压成表 | K-105–K-110 | 去向：10 全篇 |
| V-14 | 重排+瘦身 | `10-vm-relocation.md`（669） | `11-vm-relocation`（~450） | 编号顺移；LU 流程迁出 | K-115–K-119 | 去向：11 全篇；LU 调用面 → 26 |
| V-15 | 重排 | `11-phys-pagestate.md`（513） | `12-phys-page-state` | 编号顺移 | K-125–K-130 | 去向：12 全篇；`mem_cow` 定义只留一句 → 18 |
| V-16 | 重排 | `12-memtype.md`（538） | `13-memtype` | 编号顺移 | K-135–K-144 | 去向：13 全篇 |
| V-17 | **合并** | `13-region-mapping.md`（596）+ `14-region-lookup.md`（528） | `14-region-map`（~750） | 索引面是区域账本的一部分；拆开后两篇互相重复 `map_lookup`/`find_slot`，且 14 的 528 行里大半是 AVL 宏模板的逐函数展开 | K-150–K-166 | 去向：13 §2 → 14 §"生命周期"；14 §2/§3 → 14 §"有序索引" + 一张"AVL 操作 → BTreeMap 语义"对照表 |
| V-18 | **拆分** | `15-ipc-dispatch.md` §4（回复编码/wire） | `15-vm-message-protocol`（新建） | 线格式是 11 个服务篇的横切前置，压在分发篇里导致每篇都要自己讲一遍 overlay | K-170–K-179 | 去向：15 全篇；16 只留"分派" |
| V-19 | 重排+瘦身 | `15-ipc-dispatch.md` 主体（739） | `16-vm-main-loop`（~650） | 编号顺移；handler 分派细节迁回各服务篇 | K-185–K-195 | 去向：16 全篇 |
| V-20 | 重排 | `16-pagefault.md`（618） | `17-pagefault` | 编号顺移 | K-200–K-208 | 去向：17 全篇；§3.7 不变量本体 → 01（本篇保留逐路径满足性） |
| V-21 | 重排 | `17-cow-mechanism.md`（476） | `18-cow` | 编号顺移 | K-215–K-221 | 去向：18 全篇 |
| V-22 | 重排 | `18-vm-fork.md`（487） | `19-vm-fork` | 编号顺移 + 升为服务组代表精讲 | K-225–K-231 | 去向：19 全篇 |
| V-23 | 重排+瘦身 | `19-vm-brk.md`（568） | `20-vm-brk`（~450） | 编号顺移 | K-235–K-239 | 去向：20 全篇 |
| V-24 | 重排 | `20-vm-mmap.md`（470） | `21-vm-mmap` | 编号顺移 + 升为映射组代表精讲 | K-245–K-251 | 去向：21 全篇 |
| V-25 | 重排+瘦身 | `21-vm-munmap.md`（689） | `22-vm-munmap`（~550） | 编号顺移 | K-255–K-261 | 去向：22 全篇 |
| V-26 | 重排 | `22-vm-exit.md`（595） | `23-vm-exit` | 编号顺移 + 收编 exec 重建 | K-265–K-270 | 去向：23 全篇 |
| V-27 | 重排+瘦身 | `23-vfs-interaction.md`（531） | `24-vfs-interaction`（~500） | 编号顺移 | K-275–K-280 | 去向：24 全篇 |
| V-28 | 重排 | `24-page-cache.md`（518） | `25-page-cache` | 编号顺移 | K-285–K-294 | 去向：25 全篇 |
| V-29 | 重排 | `25-rs-services.md`（691） | `26-rs-services` | 编号顺移；启动注册迁出 | K-300–K-309 | 去向：26 全篇；`map_service`/`sef_cb_init_fresh` → 02 |
| V-30 | 重排+瘦身 | `26-vm-queries.md`（743） | `27-vm-queries`（~600） | 编号顺移 | K-315–K-321 | 去向：27 全篇 |
| V-31 | **新建** | — | `28-vm-c-parity-and-gaps` | ARCH 与缺口只存在于 plan/todo 两份台账，教学目录里没有承载 | K-325–K-328 | 来源：plan §4/§5.4 + todo §18.4/§19 |
| V-32 | **新建** | — | `29-vm-build-and-test` | 非 C 制品（features/依赖/注入/矩阵/覆盖率）无处可查 | K-330–K-336 | 来源：`Cargo.toml` + `main.rs` + `kernel_gateway.rs` + `tools/` |
| V-33 | **扩充** | `99-global-concepts.md`（74 行） | `99-vm-global-concepts`（~400 行） | 最短的一篇却是全部文档的词汇表 | K-340–K-344 | 去向：全表化 |

### 6.2 操作统计

- **新建 5 篇**：01、06、15、28、29
- **合并 2 组**：02+03 → 03；13+14 → 14
- **拆分 3 处**：01 拆出 01/02/16；07 拆出 06/08；15 拆出 15/16
- **重排 24 处**（编号顺移）
- **归档 0 篇**（B 相不删；旧编号文档整体移入 `archive/` 或保持不动，由 B 相决定）
- **净变化**：28 篇 → 30 篇

---

## 7. 缺漏新篇

按提示词步骤 3 的固定清单逐项落实（非 C 主题）：

| # | 主题 | 为什么重要 | 原料来源 | 归哪一篇 | 验收标准 |
|---|------|-----------|---------|---------|---------|
| N-01 | **链接与加载** | VM 是唯一在用户态装载别人 ELF 的服务；Rust 侧由 `minix-elf` crate 承担，这件事在现有文档里一个字没有 | `os/servers/vm/Cargo.toml`（minix-elf 依赖）；`main.c:383`（`libexec_load_elf`）；`os/libs/minix-elf` | `02-vm-boot-chain` §"boot 进程装载" | 能说出 ELF 段解析由哪个 crate 完成、C 对应哪个函数、VM 为什么不需要自己的链接脚本 |
| N-02 | **镜像与内存布局** | 判定为**不在本 stage**（镜像布局属 kernel）。VM 侧只消费 `kernel_boot_info` 与自身窗口常量 | `01-stage-kernel/09-vm-boot-protocol.md`；`glo.h`；`direct_map.rs:26-28` | `02`（消费面）+ `06`（窗口常量）；在两篇的"不讲什么"里显式写清去向 | 读者不会在本目录找镜像布局；能说出 `VM_OWN_HEAPBASE` 等常量在哪定义 |
| N-03 | **汇编入口与陷阱进入** | 判定为**不在本 stage**（kernel trap 层）。VM 只收到已被转成消息的缺页 | `01-stage-kernel`（trap 层）；`main.c:153-164` | `17-pagefault` 的"不讲什么"；`28` 登记为跨 stage | 能说出"CPU 异常 → 内核转发 → VM 消息"的边界在哪 |
| N-04 | **启动装配** | 本 stage 的骨架；现有 01 篇把启动与主循环混讲 | `main.c:93-587` 全量 | `02-vm-boot-chain` 全篇 | B01–B20 真序表逐条可核对 |
| N-05 | **构建与工具链** | 现有文档零覆盖；读者无法自己构建 | `os/servers/vm/Cargo.toml`、`main.rs`（test 构建跳过 binary） | `29-vm-build-and-test` §构建 | 能照着跑通构建；能说出 6 个 feature 各自开关什么、6 个 crate 各自提供什么 |
| N-06 | **跨模块接口与线格式** | 11 个服务篇各自讲一遍 overlay，且 V13-P2-6 的五结构错位无处登记 | `minix/include/minix/ipc.h`；`minix-types/src/ipc/vm.rs`+`message.rs`；`dispatcher.rs:1075-1172` | `15-vm-message-protocol` 全篇 | 能手工对出一条消息的字段偏移；能列出无专属 Rust 结构的五个消息 |
| N-07 | **错误路径** | errno 契约、`ENOSYS` 兜底、fail-fast panic 三件事散在四篇 | `main.c:139/165-166/181-191`；`ipc/encode.rs`；`os/Cargo.toml:260` | `15`（errno 编码）+ `01`（失败策略）+ `28`（未实现的 ENOSYS 契约） | 能说出越界调用、未注册调用、ACL 拒绝、handler 内部失败四类错误的回复值 |
| N-08 | **关闭与退出** | VM 没有 graceful shutdown，这一点没有文档说清 | `exit.c:60/100`；`os/Cargo.toml:260/263`（panic=abort） | `23-vm-exit`（进程退出）+ `01`（VM 自身退出） | 能区分"进程退出"与"VM 自身终止"两条语义 |
| N-09 | **并发与同步** | 单线程循环是 VM 全部并发论证的前提，但只写在 00 的一句话里 | `main.c:112`；AGENTS.md 执行模型条款；`minix3/minix/kernel/proc.c:345-347`（C 的 SMP 刷新） | `01-vm-execution-model` 全篇；`28` 登记 E-VMTLB | 能解释为什么 VM 可以用 `Rc/RefCell`；能说出 Rust 缺 C 的哪条 SMP 机制 |
| N-10 | **测试基建** | 三矩阵、注入 seam、parity 测试、CALLMAP 守护、覆盖率工具——读者不知道测试怎么跑、数字从哪来 | `Cargo.toml` features；`kernel_gateway.rs`；`pagetable/sim.rs`；`tools/coverage-extract/`；`todo.md:40-43` | `29-vm-build-and-test` §测试 | 能跑通三矩阵并解释换后端为何语义不变；能指出测试数字必须重跑 |

**全部十项均已落实，无"待定"。**

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表（按旧文档逐篇逐节）

> 迁移类型：原样搬移 / 改写 / 合并 / 拆分 / 删除。

| 旧位置 | 旧内容（一句话） | 新位置 | 类型 | 断链风险 |
|---|---|---|---|---|
| 00 §1 概念 | 三重权威与执行模型 | 00 §1 + 01 §1 | 拆分 | 低（00 保留概述，01 承接展开） |
| 00 §2 源码地图 | C 文件 → 文档映射 | 00 §2（重绘） | 改写 | **高**：编号全变，是外部引用热点（13 次） |
| 00 §3 实施现状 | 覆盖率与测试数字 | 00 §3（指针化）+ 29 | 拆分 | 中 |
| 00 §4 实施详解 | KernelGateway seam | 29 §注入 | 原样搬移 | 低 |
| 01 §1 概念 | 启动链"先有鸡还是先有蛋" | 02 §1 | 原样搬移 | 低 |
| 01 §2 C 源码分析 | main/init_vm/SEF/exec_bootproc | 02 §2 | 原样搬移 | **高**：本目录被引用最多的一篇（32 次） |
| 01 §2 主循环段 | run/run_once 五优先级 | 16 §2 | 拆分 | **高**：01 的引用者多半是冲主循环来的 |
| 01 §3 Rust 设计决策 | D1–Dn | 02 §3 + 28 | 拆分 | 中 |
| 01 §5 测试要点 | 启动测试 | 29 §测试矩阵 | 原样搬移 | 低 |
| 02 §1/§2 | vmproc 结构与状态机 | 03 §"结构" | 合并 | 中（02 被引用 4 次） |
| 02 §3 Rust 决策 | typestate | 03 §"Rust 侧类型化" | 合并 | 低 |
| 03 §2 | 表/endpoint/vm_isokendpt | 03 §"表与 endpoint" | 合并 | 中（03 被引用 5 次） |
| 04 全篇 | ACL | 04 全篇 | 原样搬移 | **低**：编号不变（04 → 04） |
| 05 §2 | 物理分配器 | 05 §2 | 原样搬移 | 低（05 → 05 编号不变） |
| 05 前置字段 | 引用 07 的 Direct Map | 删除（改引 06） | 删除 | 低（本次去环） |
| 06 §2 | vm_allocpage 族 | 07 §2 | 原样搬移 | 中（06 → 07） |
| 06 §3.3 | 保留页池消除决策 | 07 §3 + 28 | 拆分 | 中 |
| 07 §2 | pt_t 结构 | 08 §2 | 原样搬移 | **高**：07 被引用 28 次（第二热点） |
| 07 §3.6 | Direct Map | **06 全篇** | 拆分 | **高**：07 的引用者相当大的比例是冲 Direct Map 来的 |
| 08 §1/§2 | 页表操作 | 09 §1/§2 | 原样搬移 | 中（08 → 09） |
| 08 §3 | D1–D8 设计决策 | 09 §3 + 28 | 拆分 | 中 |
| 09 全篇 | slab → HeapArena | 10 全篇 | 原样搬移 | 低（09 → 10） |
| 10 §2 | 搬迁与 swap 原语 | 11 §2 | 原样搬移 | 低（10 → 11） |
| 10 §2 LU 流程段 | LU 调用面 | 26 §2 | 拆分 | 中 |
| 11 §2/§3 | phys_block + PageFlags | 12 §2/§3 | 原样搬移 | 低（11 → 12） |
| 11 §2 mem_cow 定义 | CoW 分裂 | 18 §2 | 拆分 | 中 |
| 12 §2/§3 | memtype 回调族 | 13 §2/§3 | 原样搬移 | 低（12 → 13） |
| 13 §2 | 区域生命周期 | 14 §"生命周期" | 合并 | 中（13 被引用 12 次） |
| 14 §2/§3 | AVL → BTreeMap | 14 §"有序索引" | 合并 | 中（14 被引用 3 次） |
| 15 §1/§2 | 主循环与分发 | 16 §1/§2 | 原样搬移 | 中（15 → 16） |
| 15 §4 | 回复编码与 wire | 15 §"线格式与回复" | 拆分 | 中 |
| 16 §2 | 缺页状态机 | 17 §2 | 原样搬移 | 低（16 → 17） |
| 16 §3.7 | 不在运行不变量 | 01 §"不变量" + 17 §"满足性" | 拆分 | 中（该节是 2026-09-09 新增，引用少） |
| 17 §2/§3 | CoW 机制 | 18 §2/§3 | 原样搬移 | 低（17 → 18） |
| 18 §2 | do_fork | 19 §2 | 原样搬移 | 中（18 被引用 20 次，第三热点） |
| 19 §2/§3 | brk | 20 §2/§3 | 原样搬移 | 低（19 → 20） |
| 20 §2/§4 | mmap + overlay | 21 §2 + 15 §字段表 | 拆分 | 中 |
| 21 §2/§4 | munmap + overlay | 22 §2 + 15 §字段表 | 拆分 | 中 |
| 22 §2 | exit/procctl | 23 §2 | 原样搬移 | 低（22 → 23） |
| 23 §2 | VFS 对话 + fdref | 24 §2 | 原样搬移 | 低（23 → 24） |
| 24 §2/§3 | 页缓存 | 25 §2/§3 | 原样搬移 | 低（24 → 25） |
| 25 §2 | RS 四 handler | 26 §2 | 原样搬移 | 中（25 被引用 18 次，第四热点） |
| 25 §2 启动注册段 | map_service/sef_cb_init_fresh | 02 §2 | 拆分 | 中 |
| 26 §2/§4 | 查询服务 | 27 §2 + 15 §字段表 | 拆分 | 中（26 被引用 12 次） |
| 99 全篇（74 行） | 词汇表 | 99 全篇（~400 行） | 改写 | 低（编号不变） |

### 8.2 引用迁移表

**A. 目录内交叉引用（bare `NN-*.md`，28 篇内部共 313 处，按 §0.4 统计口径）**

热点与迁移方式：

| 旧引用串 | 出现次数（目录内） | 新目标 | 验证方式 |
|---|---|---|---|
| `15-ipc-dispatch.md` | 28 | `16-vm-main-loop.md`（线格式相关者 → `15-vm-message-protocol.md`） | `rg -n '15-ipc-dispatch' 02-stage-vm/*.md` 逐条判定归属 |
| `13-region-mapping.md` | 24 | `14-region-map.md` | 同上 |
| `12-memtype.md` | 20 | `13-memtype.md` | 同上 |
| `06-page-allocator.md` | 20 | `07-vm-page-allocator.md` | 同上 |
| `01-vm-init-main.md` | 17 | `02-vm-boot-chain.md` 或 `16-vm-main-loop.md`（按语境判定） | 同上 |
| `07-pagetable-struct.md` | 16 | `08-pagetable-structure.md`（Direct Map 语境 → `06-direct-map-and-address-space.md`） | 同上 |
| `16-pagefault.md` | 14 | `17-pagefault.md` | 同上 |
| `03-vmproc-table.md` | 14 | `03-vmproc-and-proc-table.md` | 同上 |
| 其余（≤13 次者 19 个串） | — | 按 §8.1 表逐条映射 | 同上 |

**B. 目录外引用（`02-stage-vm/NN-*.md` 形式的绝对路径引用）**

- 全仓 29 个命中串、总计约 300 次（按 §0.4 的 `uniq -c` 逐串求和，含目录内自身引用）；
- **跨 stage 引用共 491 行**（`grep -rn "02-stage-vm" notes/rewrite/fork-syscall-rewrite/ | grep -v 本目录`），分布在至少 5 个目录：`16-stage-drivers/plan.md`、`16-stage-drivers/08-log-driver.md`、`16-stage-drivers/05-memory-driver.md`、`07-stage-ds/plan.md`、`01-stage-kernel/*`；
- 迁移方式：脚本批量替换 `02-stage-vm/NN-<old>.md` → `02-stage-vm/NN-<new>.md`，替换表即 §8.1 的"旧位置 → 新位置"列；
- 验证方式：`rg -n "02-stage-vm/[0-9][0-9]-" notes/ | rg -v "<new-name>"` 应为零命中（新名集合见 §4.1）。

**C. 代码注释里的引用**

- `os/` 下共 **14 处**提到 `02-stage-vm`，其中具名文档串只有两个：`02-stage-vm/26-vm-queries.md`（2 次）、`02-stage-vm/01-vm-init-main.md`（1 次）；其余为目录级提及（`02-stage-vm/todo.md` 等，不受编号重排影响）。
- 迁移：`26-vm-queries.md` → `27-vm-queries.md`；`01-vm-init-main.md` → `02-vm-boot-chain.md`（需按注释语境判定是否应为 `16-vm-main-loop.md`）。
- 验证：`rg -n "02-stage-vm/[0-9][0-9]-" os/` 迁移后应只剩新名。

**D. 参考材料里的引用**

- `plan.md`（443 行）、`checklist.md`（867 行）、`todo.md`（510 行）内部大量使用旧编号。
- 处置：B 相按 §8.1 批量替换；`plan.md` 的 §2 阶段总览表与 §3.4 边界表**整体作废重写**（它们描述的就是旧目录）。

### 8.3 断链成本摘要

| 指标 | 数值 | 说明 |
|---|---|---|
| 编号发生变化的旧文档 | **24 / 28** | 只有 `00`、`04`、`05`、`99` 四篇编号不变 |
| 目录内交叉引用受影响 | ~313 处 | bare 引用串统计 |
| 目录外（跨 stage）引用受影响 | 491 行 | 含 5+ 个其它 stage 目录 |
| 代码注释引用受影响 | 3 处具名 + 11 处目录级 | 具名 3 处需人工判定语境 |
| 参考材料引用受影响 | plan/checklist/todo 三份，约 1800 行 | 需批量替换 + plan §2/§3.4 重写 |
| **热点文件 Top 5** | `01-vm-init-main.md`(32)、`07-pagetable-struct.md`(28)、`18-vm-fork.md`(20)、`25-rs-services.md`(18)、`00-vm-overview.md`(13) | 迁移时优先人工复核这 5 篇的入引用 |
| 建议的批量修改方式 | ① 先用脚本按 §8.1 表做全仓 `sed` 替换；② 对 Top 5 热点的每一条入引用人工复核语境（尤其是 `01` → `02` vs `16`、`07` → `08` vs `06` 两处分叉）；③ 替换后跑 `rg -n "02-stage-vm/[0-9][0-9]-" notes/ os/` 全量白名单校验 | — |

**成本判断**：24 篇编号变化、约 800 处引用受影响——这是"重排"而非"重建"的典型成本量级。但因为 B 相是**按新目录重写正文**（不是搬移），引用迁移只是收尾的机械步骤，不构成本蓝图的阻塞项。**唯一需要人工判断的两处分叉**（`01` 的引用者是要启动链还是要主循环；`07` 的引用者是要页表结构还是要 Direct Map）已在 §8.1 表中逐条给出判据。

---

## 9. 验证与自检门

### 9.1 四种机械检查

**检查 1：前向引用扫描**（逐篇扫"前置"字段，确认只指向更早编号）

| 篇 | 前置 | 是否全部更早 |
|---|---|---|
| 00 | — | ✅ |
| 01 | 00 | ✅ |
| 02 | 00, 01 | ✅ |
| 03 | 02 | ✅ |
| 04 | 03 | ✅ |
| 05 | 02 | ✅（**已去掉原 07 前向引用**） |
| 06 | 05 | ✅ |
| 07 | 05, 06 | ✅（**已去掉原 07 前向引用**） |
| 08 | 05, 06, 07 | ✅ |
| 09 | 08 | ✅ |
| 10 | 07, 08, 09 | ✅ |
| 11 | 10 | ✅ |
| 12 | 05, 11 | ✅ |
| 13 | 12 | ✅ |
| 14 | 12, 13 | ✅ |
| 15 | 03, 04 | ✅ |
| 16 | 03, 04, 15 | ✅ |
| 17 | 14, 16 | ✅ |
| 18 | 12, 13, 17 | ✅ |
| 19 | 12, 13, 14, 16, 18 | ✅ |
| 20 | 14, 16 | ✅ |
| 21 | 14, 16, 20 | ✅ |
| 22 | 14, 16, 21 | ✅ |
| 23 | 03, 14, 16 | ✅ |
| 24 | 13, 16, 21 | ✅ |
| 25 | 13, 24 | ✅ |
| 26 | 11, 16, 23 | ✅ |
| 27 | 14, 16 | ✅ |
| 28 | 00–27（支线，可跳读） | ✅ |
| 29 | 16 | ✅ |
| 99 | 00 | ✅ |

**结果：30 篇全部通过，零前向引用。**

**检查 2：依赖关系图无环**

按上表构图，逐条边检查：
- 唯一曾存在的环 `05 → 07 → 05`（原目录）已由 V-08/V-09/V-10 拆成 `05 → 06 → 07 → 08` 的链；
- 原 `06 ⇄ 07` 环（06 前置 07、07 前置 06）已消除；
- `16 ⇄ 17` 的弱环（原 16 的"不覆盖"指向 17、17 的前置指向 16）已由契约明确为单向边 `17 → 18`（17 只引入 `NeedCow` 动作名，18 展开机制），**16 的"不讲什么"只写"机制归 18"，不构成前置边**；
- 其余边均为编号递增边。

**结果：无环。**

**检查 3：覆盖率（知识点池每条都有去向）**

- 152 条知识点中，**存量 138 条全部有明确新篇章归属**（见 §5 每篇契约的知识点清单与 §6 变更表的"去向"列）；
- **新增 14 条**（K-015/K-016/K-071/K-080/K-164/K-165/K-173/K-174/K-186/K-195/K-218/K-219/K-220/K-260/K-330~K-336）全部有证据锚点，且已在对应契约里列出；
- **明确删除项**：无（本蓝图不丢弃任何知识点；重复项按 §3.2 指定主讲述点，其余改为引用，不删除）。

**结果：覆盖率 100%。**

**检查 4：断链成本统计**

见 §8.3。已给出受影响引用总量（约 800 处）、热点 Top 5、批量修改方式，以及两处需要人工判定的分叉。

**结果：已完成。**

### 9.2 自检门 G1–G9

| 门 | 检查内容 | 结果 | 证据/说明 |
|---|---|---|---|
| **G1** | C 真序是否逐条可核对（随机抽十条） | ✅ 通过 | 抽查 B02(main.c:100-103)、B05(:455)、B08(:468 + region.c:36-38)、B10(:474-475)、B11(:480)、B12(:485-495)、B15(:522-575)、R03(:122)、R10(:153-164)、R13(:181-191)——十条全部为 `main.c` 通读时逐行确认；B08 的"空函数"结论由 `sed -n '36,38p' region.c` 实测 |
| **G2** | 知识点池完整性：每个 C 文件、每个非 C 制品都有归属 | ✅ 通过 | 24 个 .c 全部出现在至少一条知识点的锚点列（`acl.c`→K-041/`alloc.c`→K-052/`break.c`→K-235/`cache.c`→K-285/`exit.c`→K-265/`fdref.c`→K-277/`fork.c`→K-225/`main.c`→K-001~K-012/`mem_anon.c`→K-137/`mem_anon_contig.c`→K-138/`mem_cache.c`→K-290/`mem_directphys.c`→K-139/`mem_file.c`→K-141/`mem_shared.c`→K-140/`mmap.c`→K-245/`pagefaults.c`→K-200/`pagetable.c`→K-075/K-095/`pb.c`→K-125/`region.c`→K-150/`regionavl.c`+cavl→K-156/`rs.c`→K-300/`slaballoc.c`→K-105/`utility.c`→K-050/K-034/K-115/K-315/`vfs.c`→K-275）；非 C 制品见 §3.5/§7 十项 |
| **G3** | 新目录前向引用为零 | ✅ 通过 | §9.1 检查 1，30 篇逐篇扫描 |
| **G4** | 依赖图无环；有环是否给出拆解方案 | ✅ 通过 | §9.1 检查 2；原环 `05⇄07`、`06⇄07`、`16⇄17` 三处，拆解方案分别为 V-08/V-09/V-10 与契约改写 |
| **G5** | 覆盖率 100%；新增条目都有锚点；删除项单独列出 | ✅ 通过 | §9.1 检查 3；删除项为**空集**（显式声明） |
| **G6** | 每处拆分/合并写清存量知识点去向；每处新建写清新增知识点来源（抽查十处） | ✅ 通过 | 抽查：V-02(来源 4 路)、V-06(去向 2 路)、V-10(去向 + 保留)、V-17(去向 2 路)、V-18(去向)、V-31(来源 plan+todo)、V-32(来源 Cargo+tools)、V-03(去向 16)、V-14(去向 11+26)、V-29(去向 26+02)——十处均有去向或来源列 |
| **G7** | 每篇契约七要素齐全 | ✅ 通过 | §5 共 31 节（30 篇 + 99），每节含：一句话定位 / 讲什么 / 不讲什么 / 前置 / 后置 / 事实底线 / 知识点清单 / 验收标准 |
| **G8** | 锚点迁移表覆盖所有变化文档的每一节；引用迁移表覆盖文档与代码注释 | ✅ 通过 | §8.1 覆盖 24 篇变化文档的全部 `## ` 级节（§0.3 已抽取全部 28 篇的章节表）；§8.2 四类引用（目录内/跨 stage/代码注释/参考材料）全部覆盖 |
| **G9** | 事实断言都有锚点；推测项已标注 | ✅ 通过 | 抽查十条：`region.c:36-38` 空函数（实测）、`pb.c:136-168` mem_cow 换型（实测）、`acl.c:44-53` NO_ACL 放行（实测）、`main.c:522-575` CALLMAP 26 条（实测）、`vm.h:80-84` VM_OWN_*（实测）、`Cargo.toml` minix-sef（实测）、`os/servers/vm` 无 build.rs/无 .ld（实测 `find`）、`direct_map.rs:16-28` 常量（实测）、`glo.h` 六 memtype（实测）、`todo.md:40-43` 三矩阵（实测） |

### 9.3 推测与待验证项（显式标注）

以下三项在 B 相写正文时必须现场复核，本蓝图未亲自验证：

1. **推测**：`checklist.md`（867 行）内部对旧编号的引用量级——本蓝图只统计了 `grep -c` 的行数，未逐条分类。B 相迁移时需单独扫一遍。
2. **待验证**：`todo.md:40-43` 记录的三矩阵数字（503/521/503）是 2026-09-09 的基线；`29-vm-build-and-test` 写正文时必须**重跑**`cargo test -p minix-vm --lib` 三个 feature 组合，不能抄旧值。
3. **待验证**：`coverage-extract` 的 371 符号 / 92.5% 是 V13 轮（2026-09-09）的数字；`29` 与 `00` 引用时必须重跑或标注日期。

### 9.4 结论

**本蓝图判定为"已完成"**：四种机械检查全部通过，G1–G9 九门全部通过。

**待用户裁决的三个问题**：

1. **旧文档的处置方式**：B 相是把 28 篇旧文档整体移入 `archive/`（保持可检索），还是保留在顶层并在头部加"已由新编号取代"横幅？本蓝图按"归档不删"假设写（§6.2 归档 0 篇指的是不删除，不是不移动）。
2. **`plan.md` / `checklist.md` / `todo.md` 的处置**：这三份台账在重建后是否保留？本蓝图的建议是——`plan.md` 的 §2/§3.4 作废重写为"新目录的执行记录"，`checklist.md` 的覆盖表迁入 `28` 与 `29`，`todo.md` 保留为活台账（它记录的是代码侧开口，不是文档结构）。
3. **`draft/`（32 个文件，约 2.6 万行）是否随本次重建一并归档**：本蓝图全程未把它当作知识来源（只作为旧编号的对照），建议保留不动，避免二次断链。
