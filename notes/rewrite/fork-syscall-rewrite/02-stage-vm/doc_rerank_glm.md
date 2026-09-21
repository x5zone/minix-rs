# 02-stage-vm 文档重建蓝图（glm）

```text
your_name(AI agent name) = glm
target_dir(关注的工作目录) = notes/rewrite/fork-syscall-rewrite/02-stage-vm
repo_root(仓库根目录) = /home/xzhao/github/minix-rs

任务 = R 相·重建蓝图：输出 02-stage-vm/doc_rerank_glm.md，不改任何正文。
       只读 C 代码不够，还必须读：本目录全部文档的头部声明、os/ 目录对应入口、
       00-*-overview.md 的导航表。
约束 = 不得引用 .design/ 与 tmp_design_and_todo/；任何落盘产物必须带
       _glm 后缀；不得读取或照抄其它 AI 的 doc_rerank_* 产物。

用户补充约束（本轮生效，写入 §4 设计原则）：
1. 单篇长度为软限制——单个概念若极为复杂，允许一篇独占且长达约 3000 行；
   一般情况仍须控制篇幅以利于阅读。
2. 旧文档数量不是限制——新目录允许增加或减少篇数；本蓝图 28 → 31
   （合并 1 篇、新建 4 篇、其余重编号）。
3. 本任务为修正既有文档而设，必要时可重建；存量文档可能含有错误或大量
   未经人工审阅的内容——真相源是 minix3 C 源码 + Rust 代码，存量文档只是
   声明载体与知识线索。落实于 §2.1（池的认知定位）、§3.6（可信度抽样
   核查，16 样本）与 §4.1 原则 8（B 相核对纪律）。
```

---

## 0. 元数据

### 0.1 基本信息

| 项 | 值 |
|----|-----|
| 执行者 | glm |
| 日期 | 2026-09-19 |
| 目标目录 | `notes/rewrite/fork-syscall-rewrite/02-stage-vm/` |
| 仓库根目录 | `/home/xzhao/github/minix-rs` |
| 当前提交号 | `b5bdfd88e935c73c85bc2904213a963bbc86774a`（2026-09-19 04:38 +0800，branch `rewrite`） |
| 交付物 | 本文件（唯一落盘产物，`_glm` 后缀） |
| 正文名词约定 | "旧 NN"指现行编号文档；"新 NN"指本蓝图 §4 的新目录编号 |

### 0.2 审查范围（含范围外说明）

**范围内（正式文档，27 篇编号 + 1 篇全局）**：`00-vm-overview.md`、`01-vm-init-main.md` ～ `26-vm-queries.md`、`99-global-concepts.md`（行数与头部声明全量登记于 §0.4）。

**范围内（参考材料，只作证据与边界来源，不进入重编号体系）**：`plan.md`（443 行，现行编号的规划契约）、`todo.md`（510 行，V13 轮 + V14-P2-1 架构审查账本）、`checklist.md`（867 行，C↔Rust 覆盖基线，头部横幅已声明行级数字失效、以 todo §17.1 的 175 项语义判定为准）、`draft/`（旧 fork 主线素材，归档不删）、`archive/`（todo 历史轮次存档）。

**范围外**：`.design/` 与 `tmp_design_and_todo/`（项目规范禁止正式引用，本蓝图零引用）；其它 AI 的 `doc_rerank_*` 产物（未读取）；`os/servers/` 其它服务、`os/kernel/`、`os/arch/` 的实现细节（仅作为跨阶段边界的引用目标，不展开）；`minix3/minix/kernel/`、`minix3/minix/lib/`（同上）。

### 0.3 读取清单

按 R 相提示词第四部分的七类必读输入逐项登记：

1. **目标目录全部文档**：27+1 篇的头部声明（分类/源码/Rust 模块/前置/说明/不覆盖）全部读完；`00-vm-overview.md`、`99-global-concepts.md`、`plan.md`、`todo.md` 全文精读；其余正文按章节骨架与知识点抽样精读。
2. **Minix3 C 源码全量**：`minix3/minix/servers/vm/` 24 个 .c + 头文件（11,466 行）。`main.c`（768 行）全文精读；`pagefaults.c:handle_pagefault`（:76-158）、`fork.c:do_fork`（:32-115）一手核对；其余文件以逐文件清单核对（§0.4）+ 各篇头部已验证锚点为据。
3. **非 C 制品**：`arch/earm/vm.lds`（链接脚本）、`arch/i386/pagetable.h` 与 `arch/earm/pagetable.h`（架构页表宏）、`Makefile.inc` ×3（构建）、`minix3/minix/include/minix/com.h`（调用号族）、`ipc.h`（消息结构）、`endpoint.h`（endpoint 代际编码）、`vfsif.h`（transid）、`minix/rs.h`（rprocpub）、libc 封装（`lib/libsys/vm_exit.c`、`vm_procctl.c`、`vm_info.c`、`libc/sys/brk.c`、`libc/sys/mmap.c`）、Rust 侧测试基建（`TestIpcTransport`/`SimPaging`/`MockGateway`/`allocator_tests`）。
4. **阶段边界材料**：`notes/rewrite/fork-syscall-rewrite/00-master-plan/README.md`（阶段划分与启动因果链）、`edge_todo.md`（E1/E2/E4/E5/E-VFSWIRE/E-RSWIRE/E-VMTLB/E-ISWIRE/E-DMABUF 等 VM 相关条目，2026-09-16 全量对账）、本目录 `plan.md`/`todo.md`（含 WONTFIX 与 defer 清单）。
5. **前一 stage overview**：`01-stage-kernel/00-kernel-overview.md` 全文——已讲过不许重复展开的：boot 链与 `09-vm-boot-protocol`（VM boot 协议内核侧）、调度与 IPC 原语、syscall 分发、异常转发、`sys_vmctl` 内核实现、FPU。
6. **Rust 实现入口**：`os/servers/vm/src/`（33 个顶层模块 + ipc/region/vmproc/phys_mem/pagetable 五个子目录，30,973 行）；`os/servers/vm/src/main.rs`（`read_boot_params` 消费 `VmBootHandoff`）；`os/servers/vm/src/dma.rs`（E-DMABUF 契约实现，C 无对应）；`os/libs/minix-types`（wire 单源）。
7. **写法范例**：`01-stage-kernel/06-todo.md`——只学其"新文档契约"写法（讲什么/不讲什么/边界矩阵/目标结构/验收标准），不搬内容。

### 0.4 使用的命令与关键输出（证据摘录）

```text
$ git rev-parse HEAD && git log -1 --format='%ci'
b5bdfd88e935c73c85bc2904213a963bbc86774a / 2026-09-19 04:38:44 +0800

$ ls notes/rewrite/fork-syscall-rewrite/02-stage-vm/
→ 00~26 + 99 共 28 篇正式文档；plan.md 443 行；todo.md 510 行；
  checklist.md 867 行；draft/、archive/、.design/(禁引)

$ wc -l notes/rewrite/fork-syscall-rewrite/02-stage-vm/[0-9]*.md
→ 正式文档合计 17,344 行（不含 plan/todo/checklist）；最大 08（1098 行），
  最小 99（74 行）

$ ls minix3/minix/servers/vm/ 与 wc -l
→ 24 个 .c + 头文件，合计 11,466 行；最大 region.c 1555、pagetable.c 1500、
  cavl_impl.h 1208、main.c 768

$ find os/servers/vm -name '*.rs' | xargs wc -l | tail -1
→ 30,973 行（含测试）

# 文档互引（stage 内，含自引）
$ for f in [0-9]*.md; do n=$(basename $f .md); c=$(grep -o "$n\.md" [0-9]*.md | wc -l); echo "$c $n"; done | sort -rn
→ 合计 324 处；热点：15-ipc-dispatch 28、13-region-mapping 24、
  12-memtype 17、01-vm-init-main 17、07/06/05 各 15

# 跨 stage 引用（其它目录 .md 引用本 stage 文档）
$ grep -rEo "02-stage-vm/[0-9]{2}-[a-z-]+\.md" --include='*.md' . | grep -v '^\./02-stage-vm/' | wc -l
→ 240 处；热点：01(29)、07(26)、18-vm-fork(17)；含失效旧名
  20-vm-exit.md ×3、07-pagetable-ops.md ×1（draft 时代编号）

# 代码注释引用（os/ 仓库 Rust 代码引用本 stage 文档名）
$ grep -rEo "[0-9]{2}-[a-z-]+\.md" os/servers/vm/src os/libs/minix-types/src
→ 约 24 处（另含其它 stage 同名文档）；其中 1 处已失效：
  os/servers/vm/src/ipc/transport.rs:33 引用旧名 24-vm-ipc-dispatch.md
  （现为 15-ipc-dispatch，重建后为新 14）

# 一手 C 抽查（G1 用）
$ sed -n '76,158p' minix3/minix/servers/vm/pagefaults.c   # handle_pagefault 全函数
$ sed -n '32,115p' minix3/minix/servers/vm/fork.c        # do_fork 全函数
$ Read minix3/minix/servers/vm/main.c                    # 768 行全文
```

---

## 1. C 真序

### 1.1 阶段类型判定

**判定：服务事件循环型**（R 相提示词第九部分第二类）。依据：VM 是用户态服务器，`main()` 的主体是 `while(TRUE)` 消息循环（`minix3/minix/servers/vm/main.c:112-192`）；启动段（`is_first_time` → `init_vm` → `sef_local_startup`）与循环段（五优先级分发）边界清晰。因此真序表分两段——**启动段**（表 1-1）与**循环段**（表 1-2），另有**事件驱动回调段**（表 1-3，SEF 信号/LU 回调不在主循环直线内）与**代表性请求生命周期**（表 1-4，一手核对）。并行服务集合按提示词 5.2 的"汇聚点 + 触发时机"组织（§4.4）。

所有锚点为本轮直接读取 C 源码所得（`main.c` 全文、`pagefaults.c:76-158`、`fork.c:32-115` 一手；其余函数的行号锚点沿用各篇头部声明中已经过 review 轮行号校验的登记）。

### 1.2 真序表·启动段（main() 到进入主循环）

| 步骤 | 动作 | C 锚点 | 说明 |
|------|------|--------|------|
| S1 | `is_first_time()`：`sys_getproc` 取 RS 的 proc 结构，检查 `RTS_BOOTINHIBIT` | main.c:79-88 | fresh boot 判定门；非 fresh（LU 重启）跳过 init_vm |
| S2 | `main()` 调 `init_vm()`，置 `__vm_init_fresh=1` | main.c:100-103 | |
| S3 | `sys_getkinfo(&kernel_boot_info)` 取内核 boot 参数 | main.c:442 | 内核交接的 boot 协议消费起点 |
| S4 | `enable_filemap=1` + `env_parse("filemap",…)` | main.c:447-448 | boot 参数可关文件映射 |
| S5 | `get_mem_chunks(mem_chunks)` 内存清单 → click 取整 | main.c:455；utility.c:get_mem_chunks | VM 不探测硬件，只消费内核给的 memmap |
| S6 | `memset(vmproc,0,…)` + 逐槽 `vm_slot=i` | main.c:458-462 | 全表清零即全槽空闲（VMF_INUSE 清除） |
| S7 | `acl_init()` | main.c:465 | ACL 位图初始化 |
| S8 | `map_region_init()` | main.c:468 | 区域管理初始化 |
| S9 | `mem_init(mem_chunks)` 物理分配器接管 | main.c:471；alloc.c:mem_init | 物理内存成为可分配账本 |
| S10 | `init_proc(VM_PROC_NR)`：查 boot_procs 表、`clear_proc`、置 VMF_INUSE/endpoint/vm_boot | main.c:474（实现在 :262-286） | VM 给自己建 PCB |
| S11 | `pt_init()`：继承内核映射 + 自举资源 + 备用页 | main.c:475；pagetable.c:pt_init | VM 自身页表就绪 |
| S12 | `__minix_init()`：堆可用分界线 | main.c:480 | 此前堆分配不可用（libc IPC 向量未取得） |
| S13 | `mem_add_total_pages` ×2：boot 模块 blob + kernel 动/静态占用 | main.c:485-495 | 总页数校准（内核 freelist 不含这两块） |
| S14 | 遍历 boot_procs：`init_proc(ip->proc_nr)` → `exec_bootproc` → `free_mem(blob)` | main.c:498-520 | 除 VM 外每个 boot 进程：建 PCB、装 ELF、启运行 |
| S14a | └ `exec_bootproc`：`pt_new`→`pt_bind`→`sys_physcopy` 拷头部→`libexec_load_elf`→`minix_stack_params/fill` 组栈→`handle_memory_once` 映栈→`sys_datacopy` 拷栈→`sys_exec`→`sys_vmctl(VMCTL_BOOTINHIBIT_CLEAR)` | main.c:331-417 | boot 进程地址空间五步装配；回调 `boot_alloc`（:305-315）走 `map_page_region(VR_ANON\|VR_WRITABLE\|VR_UNINITIALIZED)` |
| S15 | CALLMAP 清零 + 26 项 `CALLMAP(code,func)` 注册 | main.c:522-575 | 分发表就绪（26 注册 + 4 无 handler 见 todo §18.1） |
| S16 | `num_vm_instances=1`；`vmproc[VM_PROC_NR].vm_flags \|= VMF_VM_INSTANCE` | main.c:578-579 | VM 实例记账（RS live-update 用） |
| S17 | `sef_llvm_add_special_mem_region(VM_OWN_HEAPBASE,…)` 向 SEF 申报自身映射窗口 | main.c:582-586 | 防 LU 把 VM 自身堆当可迁移状态 |
| S18 | `sef_local_startup()`：注册 init_fresh/init_lu/init_restart/init_response(async once)/lu_state_changed/signal_handler 六回调 → `sef_startup()` | main.c:106, 219-239 | SEF 生命周期开始；首帧 RS_INIT 回复走异步（防 boot 死锁，:228-229） |
| S19 | RS_INIT 握手在主循环内完成（见 L6） | main.c:149-152 | `do_sef_init_request` 触发 C1 回调后 SUSPEND |

### 1.3 真序表·循环段（主循环一轮）

| 步骤 | 动作 | C 锚点 | 说明 |
|------|------|--------|------|
| L1 | `if(missing_spares > 0) alloc_cycle()` | main.c:118-120 | C 侧备用页补充机会；Rust 已随 [ARCH: A-1] 结构性删除（V12-P2-4，Fix #70） |
| L2 | `sef_receive_status(ANY,&msg,&rcv_sts)` | main.c:122-123 | 失败即 panic |
| L3 | `is_ipc_notify(rcv_sts)` → 忽略并 continue | main.c:125-129 | 非 SYSTEM notify 不入分发 |
| L4 | `vm_isokendpt(who_e,&caller_slot)` 失败即 panic | main.c:130-132 | caller 身份验证是全部分发的前提 |
| L5 | **优先级 1**：`m_source==VFS && IS_VFS_FS_TRANSID` → 剥 transid → `do_procctl(&msg,transid)` | main.c:141-148 | VFS 事务路由（异步续作回复） |
| L6 | **优先级 2**：`RS_INIT && m_source==RS` → `do_sef_init_request`（失败 panic）→ SUSPEND | main.c:149-152 | 不回复 RS（SEF 协议） |
| L7 | **优先级 3**：`VM_PAGEFAULT` → 非内核来源 printf 告警 → `do_pagefaults` → **continue 不回复** | main.c:153-164 | 调用方由 `do_pagefaults` 内 `sys_vmctl(VMCTL_CLEAR_PAGEFAULT)` 解阻 |
| L8 | **优先级 4**：`c=CALLNUMBER(type)` 命中且 `vm_calls[c].vmc_func` 非空 → `acl_check` 通过 → `vmc_func(&msg)` | main.c:165-175 | ACL 闸是注册调用的唯一门禁；未授权仅 printf 后落回复 ENOSYS 语义（result 保持初值路径，:139） |
| L9 | **优先级 5**：越界或未注册 → `result=ENOSYS`（初值） | main.c:139, 165-167 | |
| L10 | `result != SUSPEND` → `msg.m_type=result` → `ipc_send(who_e,&msg)`；失败 printf + panic | main.c:181-191 | 回复失败不可恢复——VM 崩溃即系统冻结 |

### 1.4 真序表·事件驱动回调（SEF 信号与 Live Update）

| 步骤 | 动作 | C 锚点 | 说明 |
|------|------|--------|------|
| C1 | `sef_cb_init_fresh`：`sys_safecopyfrom(RS, rproctab_gid,…)` 整表拷入 → 遍历 `NR_BOOT_PROCS` 对 `in_use` 项调 `map_service` | main.c:241-260 | `map_service`（:755-768）= `vm_isokendpt` + `acl_set(&vmproc[proc_nr], rpub->vm_call_mask, !IS_RPUB_BOOT_USR(rpub))` |
| C2 | `sef_cb_lu_state_changed`（回滚）：LU 失败回 `SEF_LU_STATE_NULL` → `pt_bind` + `pt_clearmapcache` + `adjust_proc_refs` | main.c:196-217 | 两实例部分共享内存，回滚必须显式 |
| C3 | `sef_cb_init_lu_restart`：默认状态迁移 → `vm_isokendpt(old_e)` → `swap_proc_slot(old,new)` → `swap_proc_dyn_data` → `pt_bind`×2 + `pt_clearmapcache` → `adjust_proc_refs` → 进 C4 | main.c:677-726 | LU 身份交换的执行序 |
| C4 | `sef_cb_init_vm_multi_lu`：重建 rproctab → 构造 IPC 白名单过滤（仅放行 RS/VM_BRK/VM_INFO）→ `sys_statectl(ADD_IPC_WL_FILTER)` → 对 `SF_VM_UPDATE` 项批量 `do_rs_update` | main.c:592-672 | 多组件 LU 期间缺页与 handle_memory 被刻意阻塞（注释明言，:622-627） |
| C5 | `sef_cb_signal_handler`：`SIGKMEM` → `do_memory()`（内核主动内存请求）；`missing_spares>0` → `alloc_cycle`；`pt_clearmapcache` | main.c:731-750 | 信号路径是缺页之外的第二个 VM 入口 |

### 1.5 真序表·代表性请求生命周期（一手核对）

**VM_FORK（fork.c:32-115，全函数核对）**：

| 步 | 动作 | 锚点 |
|----|------|------|
| F1 | `vm_isokendpt(msg->VMF_ENDPOINT,&proc)` 失败 → EINVAL | fork.c:38-44 |
| F2 | `childproc=msg->VMF_SLOTNO` 越界 → EINVAL；父子 PCB 定位 | fork.c:46-54 |
| F3 | 结构体整体复制：`origpt=vmc->vm_pt; *vmc=*vmp;` 再恢复 slot/新 region 树/endpoint=NONE/原 pt | fork.c:56-64 |
| F4 | `pt_new(&vmc->vm_pt)` 失败 → ENOMEM | fork.c:69-71 |
| F5 | `map_proc_copy(vmc,vmp)` 失败 → `pt_free` + ENOMEM（回滚） | fork.c:74-79 |
| F6 | `vmc->vm_flags &= VMF_INUSE`（只继承在用位）+ `acl_fork(vmc)` | fork.c:82-87 |
| F7 | `sys_fork(parent, childproc, &vmc->vm_endpoint, PFF_VMINHIBIT, &msgaddr)`；失败 panic | fork.c:89-94 |
| F8 | `pt_bind(&vmc->vm_pt,vmc)` | fork.c:96-98 |
| F9 | `handle_memory_once` ×2：把父子双方的内核应答消息页（msgaddr）变为可写（优化，返回值忽略即 panic 级保障） | fork.c:100-111 |
| F10 | `msg->VMF_CHILD_ENDPOINT = vmc->vm_endpoint` 返回 OK | fork.c:113-114 |

**VM_PAGEFAULT（pagefaults.c:76-158，全函数核对）**：

| 步 | 动作 | 锚点 |
|----|------|------|
| P1 | `vm_isokendpt(ep,&p)` + `assert(VMF_INUSE)` | pagefaults.c:81-84 |
| P2 | `map_lookup(vmp,addr,NULL)` 无区域 → 保护错/坏地址两分支 printf（坏地址附 `sys_diagctl_stacktrace`）→ `sys_kill(SIGSEGV)` + `sys_vmctl(VMCTL_CLEAR_PAGEFAULT)` 返回 | pagefaults.c:87-102 |
| P3 | 区域存在但 `!(VR_WRITABLE) && wr`（写只读区）→ SIGSEGV + 清页故障返回 | pagefaults.c:105-113 |
| P4 | `offset = addr - region->vaddr`；`retry` 走同步 `map_pf`（assert 非 SUSPEND），否则打包 `pf_state` 走异步 `map_pf(pf_cont)` | pagefaults.c:115-131 |
| P5 | `io` 与否记 `vm_major_page_fault++` / `vm_minor_page_fault++` | pagefaults.c:132-135 |
| P6 | `result==SUSPEND` → 返回（等 VFS 异步续作）；`result!=OK` → SIGSEGV + 清页故障 | pagefaults.c:137-151 |
| P7 | 成功 → `pt_clearmapcache()` + `sys_vmctl(VMCTL_CLEAR_PAGEFAULT)` 解阻进程 | pagefaults.c:153-157 |

### 1.6 真序事实对后续章节的三点约束

1. **启动段严格线性**（S1→S19）：教学序与运行序可以完全一致——这是新目录第一带到第十三篇的骨架，也是 plan.md §1.2 主线的 C 侧再验证（本蓝图独立重走，非转述）。
2. **循环段是"汇聚点 + 并行体"**：L5-L9 五个优先级分支之下挂着 26 个注册调用，它们之间**无运行时序**（任意时刻任意顺序到达）。讲述必须按并行体规则组织（§4.4），不得伪造线性序。
3. **两个非直线路径必须显式**：信号回调 C5（`SIGKMEM → do_memory`，主动内存保障）与 VFS 事务 L5（异步续作）不在"收到消息→处理→回复"的直线上；它们是本 stage 仅有的两处控制流分叉，新目录各自给独立小节（新 17 页错误的双路径、新 24 VFS 异步对话）。

---

## 2. 知识点全集

### 2.1 池的构建方法与口径

- **存量条目（来源类型 = 存量）的认知定位（用户约束 3）**：存量条目是从 28 篇现有文档提取的**声明（claim）**，不是已核实的事实——存量文档可能含错或未经人工审阅。因此每条的"代表锚点"列一律给**代码侧定位**（C 文件/函数或 Rust 模块），不给"某文档某节"作为事实依据；条目的含义 = "文档声称此处有此机制，B 相写正文前必须先按锚点在代码里核对成立，再决定怎么讲"。锚点核对中发现错误的，按 §3.6 的错误清单处置（修正锚点/修正结论），不允许照抄存量正文。
- 粒度 = **语义知识点**（一个可独立回答"是什么/为什么/怎么表达"的单元），不是函数级清单——函数级对账已有 `checklist.md` §4 函数表（173 行）与 todo §17.1 的 175 项语义判定承接，本池不重复该 mechanically 可查的账，只登记"文档教给读者的东西"。
- **新增条目**（来源类型 = 新增）：由 §3.2 覆盖缺口表产生，编号 `K-N*`，锚点给 C 源码 / 非 C 制品 / Rust 现状 / 操作系统理论出处。新增条目与存量条目池内平等，只是 B 相要回答的问题不同（存量："声明是否成立、搬到哪里"；新增："证据锚点在哪里"）。
- **对齐键**：名称 + 锚点（多 AI 汇总用）。
- **去向记法**：`新NN§x` 指新目录篇章（§4.2）；"主点"标记 = 该知识点在多文档重复时的唯一权威讲述位置。
- **去重规则**：同一知识点多文档出现 → 合并一条、登记全部现有位置、标主点（明细见 §2.3/§3.3）。
- **可信度抽样**：池锚点的整体可信度以 §3.6 的 16 样本核查为准——结论：正式文档锚点抽样 13/13 命中（其中 1 处暴露数值事实错误），todo 参考材料锚点 3/3 行号漂移；B 相不允许以"抽样通过"替代逐条核对。

### 2.2 知识点池·存量（按现有文档分组）

> 列：编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向。类型缩写：概=概念、机=机制、数=数据结构、接=接口与协议、约=约束与不变量、演=架构演进、工=工具与工程、测=测试性质。

**旧 00（vm-overview，7 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-001 | 微内核中内存语义权威归用户态 VM（内核只留调度+IPC 原语） | 概 | 00§1.1 | 00 头部；main.c 全部 | 新00§1.1（主点） |
| K-002 | 三重权威：地址空间账本 / 缺页仲裁 / 分配与缓存策略 | 概 | 00§1.2 | 00§1.2 | 新00§1.2 |
| K-003 | 单线程事件循环执行模型；Rust 借用检查器即并发审计器；`VmContext`/`AssumeSyncCell` | 约 | 00§1.2 | vm_server.rs | 新00§1.3（主点；新29 引用） |
| K-004 | TLB 结构不变量一句话摘要（"VM 只改不在运行的进程的页表"） | 约 | 00§1.2 | 16§3.7 | 新00§1.3 摘要 + 新16 主点 |
| K-005 | 主循环骨架五优先级 + SUSPEND 特判 + fail-fast/fail-closed 回复 | 机 | 00§1.3 | main.c:112-192 | 新00§1.4 摘要 + 新14 主点 |
| K-006 | CALLMAP 26/26 注册对齐 C + NR_VM_CALLS=49 单源 | 事 | 00§3.1 | dispatcher.rs:681（static CALLMAP）+ :682 起 build_callmap（§3.6 E4 重锚） | 新00§3 + 新14§主点 |
| K-007 | 跨阶段依赖 edge 索引（E1/E2/E5/E-RSWIRE/E-VFSWIRE/E-VMTLB） | 工 | 00§3.3 | edge_todo.md | 新00§4（指针化） |

**旧 01（vm-init-main，12 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-010 | VM 自举的"鸡生蛋"问题（内存管理器自身要先有内存） | 概 | 01§1.1 | main.c:93-107 | 新01§1.1 |
| K-011 | `is_first_time()` 门控：RS 的 `RTS_BOOTINHIBIT` 判 fresh boot | 机 | 01§（头部/正文） | main.c:79-88 | 新01§2.1 |
| K-012 | `init_vm()` 十三步顺序与因果（为何此序不可换） | 机 | 01 全篇 | main.c:428-587 | 新01§2（主点） |
| K-013 | `kernel_boot_info` boot 协议消费（`sys_getkinfo`） | 接 | 01§2 | main.c:442 | 新01§2.2 |
| K-014 | Rust 侧 `VmBootHandoff` 页消费（`read_boot_params`；A1 根身份+A2 扣除对账） | 机 | 01§（Rust 面） | main.rs:17-30 | 新01§2.3 |
| K-015 | SEF 生命周期：六回调注册、`sef_startup`、首帧 RS_INIT 异步回复防死锁 | 机 | 01§3 | main.c:219-239 | 新01§3（主点） |
| K-016 | RS_INIT 握手折叠进主循环优先级 2（Rust 执行形状偏差，已有文档登记） | 演 | 01§3.4 | vm_server.rs | 新01§3.4 |
| K-017 | `exec_bootproc` 五步：boot 进程地址空间装配（libexec ELF 加载/栈组装/sys_exec/解抑制） | 机 | 01§4 | main.c:331-417 | 新01§4（主点） |
| K-018 | `init_proc` 槽位激活语义（boot 表查找 + clear_proc + VMF_INUSE） | 机 | 01/02 | main.c:262-286 | 新02（K-030 主点）；新01 保留调用点 |
| K-019 | `map_service`/rproctab 授权链（sef_cb_init_fresh → acl_set） | 接 | 01§2.4 + 25 | main.c:241-260,755-768 | 新01§3.2 + 新26 引用 |
| K-020 | VM 自身 libc 接口边界（`utility.c` 的 mmap/munmap/_brk 无内部调用者） | 约 | 01 边界声明 | plan §5.4 | 新01§5（边界） |
| K-021 | `sef_llvm_add_special_mem_region`：VM 向 SEF 申报自身映射窗口 | 机 | 01 | main.c:582-586 | 新01§3.5 |

> 旧 01 另承载一条**新增**知识点：K-N05（SEF 协议面双实现，见 §2.3）——归位新01§3.6，不设存量编号（避免与 K-N05 双计）。

**旧 02（vmproc-struct，8 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-030 | PCB 概念与 `struct vmproc` 全字段五组（身份/地址空间/权限/启动/统计） | 数 | 02§1-2 | vmproc.h | 新02§2（主点） |
| K-031 | `VMF_*` 正交位图而非枚举的理由（多状态叠加） | 数 | 02§2 | vmproc.h 宏族 | 新02§3 |
| K-032 | 生命周期状态机：空闲→活跃→退出中→空闲（typestate 表达） | 机 | 02§3 | vmproc_handle.rs | 新02§4 |
| K-033 | `VMP_NR = _NR_PROCS + 1` 与 exec 临时槽的存在理由 | 约 | 02§1.1 | glo.h:17-20 | 新02§2 + 新03（K-044 主点） |
| K-034 | endpoint→槽翻译是所有消息处理的第一动作 | 概 | 02§1.0 | — | 新02§1 |
| K-035 | `init_proc`/`clear_proc` 槽位激活与清场语义 | 机 | 02 | main.c:262-286；exit.c | 新02§4 |
| K-036 | 进程级失败审计与计数（fail-closed 侧） | 工 | 02/00 | vm_server.rs | 新14§10（K-N06 主点） |
| K-037 | 退出中间态的 `ExitingProc` typestate 视图与 `reap` | 机 | 02/22 | vmproc_handle.rs | 新02§4 + 新23（K-213 主点） |

**旧 03（vmproc-table，7 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-040 | 进程表 `vmproc[VMP_NR]` 集合语义：怎么找/怎么分/怎么数 | 数 | 03§1 | glo.h:20 | 新03§1（主点） |
| K-041 | `vm_isokendpt` endpoint 校验门（代际+槽界） | 接 | 03§2 | utility.c:vm_isokendpt；endpoint.h | 新03§2（主点；99 有编码条目 K-042 分工） |
| K-042 | endpoint 代际编码 `_ENDPOINT_GENERATION_SHIFT` 与编解码互逆 | 数 | 03/99 | endpoint.h；minix-types | 新99§3.3（主点）+ 新03 引用 |
| K-043 | slot 分配（fork 子槽由消息指定、boot 槽静态） | 机 | 03§3 | fork.c:46-54 | 新03§3 |
| K-044 | `VMP_EXECTMP` 保留槽的诚实定位（exec 换空实现，仅槽位保留） | 事 | 03§4 | glo.h:17 | 新03§4 |
| K-045 | 表级交换 `swap_slots`（LU 身份交换的表半，T13） | 机 | 03/10 | vmproc/table.rs；utility.c:swap_proc_slot | 新10（K-123 主点）+ 新03 引用 |
| K-046 | caller 槽非活跃态的 ACL 三分（None 即拒绝，Fix #66） | 约 | 03/04/14 | vm_server.rs 闸门 | 新04§4.4（K-055 主点） |

**旧 04（acl，7 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-050 | VM 为何要自己的访问控制（VMAP_PHYS/SET_PRIV/GETPHYS 攻击面） | 概 | 04§1.1 | — | 新04§1 |
| K-051 | ACL 位图语义：调用号→允许位（`BITCHUNK_BITS` 宽度） | 数 | 04§2 | acl.c；bitmap.h | 新04§2 |
| K-052 | DEFAULT/SYSTEM 分层授权与 RS 授予链 | 机 | 04§3 | acl.c:acl_set | 新04§3 |
| K-053 | acl_init/check/set/fork/clear 五函数族 | 接 | 04§2-4 | acl.c 全 | 新04§2-4（主点） |
| K-054 | [ARCH: A-11] fail-closed：`Uninitialized` 仅放行 DEFAULT 集（C 的 NO_ACL allow-all 是临时放松） | 演 | 04§ | acl.c:44-53；acl.rs | 新04§5（主点） |
| K-055 | None 即拒绝：caller 槽非活跃时闸门三分（Fix #66） | 约 | 04§4.4 | vm_server.rs | 新04§4.4（主点） |
| K-056 | ACL fork 派生与退出回收时机（与 fork/exit 流程的接口） | 接 | 04/18/22 | acl.c:110-130 | 新04§4 + 新19/23 引用 |

**旧 05（physical-memory，11 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-060 | 内存来源链：VM 不探测只消费（boot memmap → mem_chunks → 账本） | 概 | 05§1.1 | main.c:455;471 | 新05§1 |
| K-061 | `get_mem_chunks` click 取整与 `struct memory` | 机 | 05§2 | utility.c；type.h | 新05§2 |
| K-062 | `mem_init`/`alloc_mem`/`free_mem` 分配器本体 | 机 | 05§3 | alloc.c | 新05§3（主点） |
| K-063 | [ARCH: A-5] 三后端（bitmap/buddy/segment-tree）+ `PhysAllocator` trait + feature 切换 | 演 | 05§4 | phys_mem/* | 新05§4 |
| K-064 | buddy 低内存三段式 + `max_page_bound` 单点换算（Fix #65） | 机 | 05§4.4 | buddy_alloc.rs | 新05§4.4 |
| K-065 | PAF_* 分配旗标语义（CLEAR/LOWER16MB/LOWER1MB/ALIGN…；PAF_CONTIG 两侧零消费） | 接 | 05/99 | vm.h:22-27 | 新99§2.2（K-242 主点）+ 新05 消费面 |
| K-066 | C 侧 reservedqueue 备用队列（本体在 06 讲，05 只登记边界） | 边界 | 05 头部 | alloc.c:60-237 | 新06（K-072 主点） |
| K-067 | memstats/usedpages 记账与诊断 | 工 | 05§5 | alloc.c | 新05§5 |
| K-068 | `mem_add_total_pages` 总页数校准（boot 模块+kernel 占用） | 机 | 05/01 | main.c:485-495 | 新01§2.6（调用点）+ 新05§5（定义，K-068 主点在此行保留） |
| K-069 | 分配器 parity 测试（allocator_tests 对账三后端） | 测 | 05§5 | allocator_tests.rs | 新29§3（汇总）+ 新05§5 引用 |
| K-059 | 两层物理页管理之分界（裸物理页 vs 映射物理页，`alloc.c` 零引用 pb_new 实证） | 概 | 05/11 | 11§1.1 | 新11§1.1（K-130 主点）；新05§1 引用 |

**旧 06（page-allocator，9 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-070 | 双地址问题：自用页必须同时拿 VA 与 PA | 概 | 06§1.1 | pagetable.c | 新06§1（主点） |
| K-071 | `vm_allocpage/pages/mappages/freepages` 自用页函数族 | 接 | 06§2 | pagetable.c:_SYSTEM 族 | 新06§2 |
| K-072 | 备用页池（C reservedqueue/spare page）作为自举机制 + `pt_init` 末尾换血 | 机 | 06§1.4/§3.3 | pagetable.c:1151-1161,1311-1345 | 新06§3（C 侧）+ 新10 引用 |
| K-073 | [ARCH: A-1] Direct Map 结构性消除备用页池（Rust 单路径 `vm_pt_alloc`；对照 Redox RMM/Linux 无 VM 备用池） | 演 | 06§3.3 | alloc_page.rs | 新06§3.3（主点）+ 新07 §Direct Map |
| K-074 | `missing_spares`/`alloc_cycle` 链在 Rust 的删除判定（压力计数无生产者，Fix #70） | 演 | 06 头部注记 | alloc.c:74 | 新06§4（现状语义；判定叙事按 O2 规则移 todo 引用） |
| K-075 | `vm_pagelock`/`vm_addrok` 写保护与校验（MEMPROTECT 消费面） | 接 | 06§ | pagetable.c | 新06§2 + 新09（MEMPROTECT 消费） |
| K-076 | `get_vm_self_pages` 自用页计数 | 接 | 06§ | pagetable.c | 新06§2 |
| K-077 | `vm_self_query`（VM 自身映射查询，Rust 侧） | 接 | 06 头部 | vm_self_map.rs | 新06§4 |
| K-078 | 回收重试 `alloc_pfn_reclaiming`（V11/T30 承接 C alloc_cycle 语义） | 机 | 06 头部注记 | alloc_page.rs | 新06§4 |

**旧 07（pagetable-struct，10 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-080 | 页表 = MMU 翻译数据；每进程地址空间的结构骨架 | 概 | 07§1.1 | — | 新07§1 |
| K-081 | `pt_t` 结构与 `ARCH_VM_*` 宏族（i386/earm 双变体） | 数 | 07§2 | pt.h；arch/*/pagetable.h | 新07§2 |
| K-082 | [ARCH: A-2] 页表层级 2 级→4 级（动态分配，u64 PTE） | 演 | 07§ | paging.rs | 新07§3 |
| K-083 | [ARCH: A-1] Direct Map 双视图（`VM_DIRECT_MAP_BASE`/`DirectMapArch` trait/kernel 侧 `establish_boot_dm`） | 演 | 07§（显式章节） | os/arch direct_map.rs | 新07§4（主点） |
| K-084 | [ARCH: A-9] VM 自映射页表：`static_sparepagedirs` → `vm_self_map.rs` adopt | 演 | 07§ | pagetable.c:112；vm_self_map.rs | 新07§5 |
| K-085 | [ARCH: A-6] 地址空间宽度 32→64（MMAP_BASE 窗口重定义） | 演 | 07/00/20 | mmap.rs:MMAP_BASE | 新07§6（主点）+ 新21 引用 |
| K-086 | [ARCH: A-10] 多架构 trait（x86_64/arm64/riscv64 `Paging`/`DirectMapArch`/`DmCoverageArch`） | 演 | 07§ | os/arch/* | 新07§7 |
| K-087 | `pt_init` 结构面：继承内核映射 + 自举资源 + 登记内核映射 | 机 | 07§ | pagetable.c:1088-1349 | 新07§8 |
| K-088 | 4 级 walk 与 `write_pte_dm` 逐条 invlpg（写后失效绑定） | 机 | 07/08§1.8 | x86_64/paging.rs | 新08§1.8（机制主点）+ 新16（不变量主点） |
| K-089 | `pt_bind` 结构语义（CR3 换绑）与 `pt_mapkernel` 结构面 | 机 | 07§ | pagetable.c:1358-1489 | 新07§8 + 新08（操作主点 K-091/K-096） |

**旧 08（pagetable-ops，11 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-090 | 页表操作面总览：建/改/删/绑/抄/查六动词与消费方矩阵 | 概 | 08§1 | pagetable.c | 新08§1 |
| K-091 | 生命周期 `pt_new`/`pt_free`/`pt_bind` | 机 | 08§2 | pagetable.c:990-1437 | 新08§2（主点） |
| K-092 | `pt_writemap` + WMF 四旗标（OVERWRITE/WRITEFLAGSONLY/FREE/VERIFY） | 接 | 08§3 | pagetable.c:784；vm.h:56-59 | 新08§3（主点；旗标词汇 K-242 归 99） |
| K-093 | 页表页按需分配 `pt_ptalloc`/`pt_ptalloc_in_range`（消费 `vm_pt_alloc`） | 机 | 08§4 | pagetable.c:494-586 | 新08§4 |
| K-094 | 跨页表复制 `pt_copy`/`pt_map_in_range`/`pt_ptmap`（fork/exec/LU 消费） | 机 | 08§5 | pagetable.c:631-1068 | 新08§5 |
| K-095 | 查询校验 `pt_checkrange`/`pt_writable` | 机 | 08§6 | pagetable.c:761-942 | 新08§6 |
| K-096 | 内核协作 `pt_mapkernel`/`pt_clearmapcache`/`pt_allocate_kernel_mapped_pagetables` | 机 | 08§7 | pagetable.c:1028-1499 | 新08§7 |
| K-097 | C 全局清缓存 vs Rust 逐条 invlpg + VM 自刷四处（pt_assert/vm_freepages×2/vm_pagelock 宿主）的对照论证 | 机 | 08§1.8 | pagetable.c:119,255,319,430 | 新08§1.8（C 对照主点）+ 新16（不变量主点） |
| K-098 | [ARCH: VMINHIBIT] `pt_writemap` 的 CONFIG_SMP 动态停等包裹（C 执行模型差异，已登记行） | 演 | 08§1.10+差异表 | pagetable.c:799-815,928-934 | 新08§差异表 + 新16（与结构不变量互为策略） |
| K-099 | fork/exec/mmap/munmap/pagefault/LU 六路径对 pt 操作面的消费地图 | 概 | 08§1 开头 | — | 新08§1（主点） |
| K-101 | `Paging` trait 操作面 + `clone_range`/`map_kernel`（Rust 操作面形状） | 数 | 08 头部 | os/arch/paging.rs | 新08§3 |

> 旧 08 范围另有一条**新增**知识点：K-N03（内核死 VMCTL 面，见 §2.3）——正文未讲、todo 有账，归位新16，不设存量编号（避免与 K-N03 双计）。

**旧 09（slab-allocator，8 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-110 | VM 堆的自举性：`__minix_init` 前堆不可用的分界线 | 约 | 09§1.1 | main.c:480 | 新09§1（主点） |
| K-111 | C slab 尺寸分类分配器（SLABSIZES/位图/sdh/slabdata） | 机 | 09§1.3-1.5 | slaballoc.c 全 | 新09§2（主点） |
| K-112 | `SLABALLOC`/`SLABFREE` 类型化宏 | 接 | 09§ | proto.h | 新09§2 |
| K-113 | [ARCH: A-3 v2] `HeapArena` + `VmAllocator` free-list 替代 slab（slab 有意省略的理由 + v2 回收语义） | 演 | 09§1.8 | heap_arena.rs；global.rs | 新09§3（主点） |
| K-114 | MEMPROTECT 调试写保护（`vm_pagelock` 消费） | 工 | 09§1.6 | vm.h:VMP_SLAB；pagetable.c | 新09§4 |
| K-115 | `vm_self_mappages`/`vm_self_unmap`（堆 VA 的页表操作面） | 接 | 09 头部 | vm_self_map.rs | 新09§3 |
| K-116 | `VM_HEAP_BASE/SIZE/LIMIT` 堆窗口（Direct Map 消费） | 约 | 09 头部 | direct_map.rs | 新09§3 + 新00 布局总图 |
| K-117 | slabstats 统计与 VM_INFO 的口径衔接 | 工 | 09/26 | slaballoc.c:slabstats | 新09§5 + 新27 引用 |

**旧 10（vm-relocation，8 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-120 | 自举终点问题：静态/临时分配为何不能长期用（大小固定+PA 不可控） | 概 | 10§1.1 | — | 新10§1 |
| K-121 | C 搬迁：spare page 池 + 页表结构 BSS→动态（`pt_init` 搬迁段 + `pt_init_done`） | 机 | 10§1.3-1.4 | pagetable.c:1311-1345 | 新10§2（C 侧主点） |
| K-122 | Rust 搬迁：分配器元数据 BumpBuf→HeapArena 语义化迁移 | 机 | 10§1.6-1.7 | vm_server.rs:relocate；global.rs | 新10§3 |
| K-123 | `swap_proc_slot`/`swap_proc_dyn_data`（LU 身份与动态数据交换） | 机 | 10§（LU 支撑面） | utility.c:186-219,312 | 新10§4（主点）+ 新26 引用 |
| K-124 | `transfer_mmap_regions`/`map_proc_dyn_data`（LU mmap 区域过户） | 机 | 10§ | utility.c:228,283 | 新10§4 |
| K-125 | `map_setparent`（区域父指针，CoW/remap 记账的支撑） | 机 | 10 | region.c:1535 | 新10§4 |
| K-126 | `relocate` 后端选择与审计（Fix #65 附带） | 工 | 10/05 | vm_server.rs | 新05§4.4 + 新10 引用 |
| K-127 | LU 全景中本篇的位置（10 支撑面 ↔ 26 服务面 ↔ 01 生命周期回调三分） | 边界 | 10/25/01 | — | 三篇边界声明（各契约落字） |

**旧 11（phys-pagestate，8 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-130 | 两层物理页管理与三层结构系统（vir_region→phys_region→phys_block） | 数 | 11§1.1-1.2 | region.h:23；phys_region.h | 新11§1（主点） |
| K-131 | `pb_new/free/link/reference/unreferenced` 生命周期五函数 | 接 | 11§2 | pb.c 全 | 新11§2 |
| K-132 | PBF_* 标志（INCACHE 等）与引用计数不变量 | 约 | 11§ | region.h:35 | 新11§3 |
| K-133 | `mem_cow` 在 pb.c 中的位置（CoW 分裂入口登记，本体在 CoW 篇） | 边界 | 11 头部 | pb.c:136 | 新18（CoW 主点 K-190）；新11 引用 |
| K-134 | [ARCH] PFN 索引全局数组 `PageFrames`（C 堆对象 → Rust 全局槽） | 演 | 11§3 | region/page_state.rs | 新11§4（主点） |
| K-135 | `PageState`/`PageSlot`/`PageFlags` 三态建模（Empty/Mapped；IN_CACHE/COW 位） | 数 | 11§3 | page_state.rs | 新11§4 |
| K-136 | `verify_refcounts` 引用计数一致性自检 | 测 | 11/29 | sanity.rs:54 | 新29§4（汇总）+ 新11 引用 |
| K-137 | `PfnAllocator` 分配原语与 `alloc_contiguous` 漏斗（Fix #79） | 接 | 11/06 | page_state.rs | 新06§2 + 新11 引用 |

**旧 12（memtype，9 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-140 | 策略模式：区域层管"何时调用"、内存类型管"做什么" | 概 | 12§1.0 | — | 新12§1 |
| K-141 | `mem_type_t` 15 字段回调表 × 6 实例（C 函数指针表） | 数 | 12§2 | memtype.h；mem_*.c | 新12§2 |
| K-142 | [ARCH] `trait MemType` 15 方法 + 6 unit struct（Rust 重表达） | 演 | 12§3 | memtype.rs:10 | 新12§3（主点） |
| K-143 | 六类内存各自的语义（anon/directphys/shared/anon_contig/cache/mappedfile 的分配/CoW/释放策略差） | 机 | 12§3 各节 | mem_*.c | 新12§3（组内代表+差异表） |
| K-144 | `PagefaultResult`（Handled/NeedVfsIo/NeedCow/NeedNewPage…）缺页分发契约 | 接 | 12§ | memtype.rs:152 | 新12§3（主点）+ 新17 消费 |
| K-145 | memtype 反向依赖 vmproc 的 C-parity 判定（回调签名收 `struct vmproc *`，Fix #76） | 约 | 12§ | memtype.h:18-22 | 新12§5 |
| K-146 | `ev_delete` 默认空实现与共享重映射 `remaps` 递减的落点论证（Fix #64） | 机 | 12§3.7 | memtype.rs:34 | 新12§3.7 |
| K-147 | MappedFile `writable()` 恒 false（C "We are never writable"）与 CoW 门控的含义 | 约 | 12/16/17 | mem_file.c:173-175 | 新12（语义主点）+ 新18（机制消费） |
| K-148 | `shared_setsource`/`phys_setphys`/`anon_resize` 等类型级回调语义 | 接 | 12§ | mem_shared.c:167 等 | 新12§3 |

**旧 13+旧 14（region-mapping + region-lookup，合并为 K-150 组，15 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-150 | 区域框架层 vs 索引层 vs 策略层的分工（12 管做什么、13 管何时调用、14 管怎么找） | 概 | 13§/14§ 头部 | — | 新13§1（主点） |
| K-151 | `vir_region` 结构 + VR_* 标志 + `VrParam` | 数 | 13§2 | region.h:37-78 | 新13§2 |
| K-152 | `RegionMap`（BTreeMap）+ `VirRegion`（Vec<PageSlot>）Rust 形状 | 数 | 13/14 | region_map.rs；vir_region.rs | 新13§2 |
| K-153 | `map_page_region`/`map_region_extend*`/`map_free(_proc)` 生命周期族 | 接 | 13§3 | region.c | 新13§3 |
| K-154 | `map_pf`/`map_handle_memory`：区域-物理桥接与缺页框架入口 | 机 | 13§4 | region.c:664-774 | 新13§4（框架主点）+ 新17（状态机消费） |
| K-155 | `map_proc_copy(_range)`：fork 的区域复制框架 | 机 | 13/18 | region.c:933-999 | 新13§3 + 新19（编排主点） |
| K-156 | `map_unmap_range` 四情形 + `map_unmap_region` 三情形 + `split_region` | 机 | 13/21 | region.c:1065-1294 | 新13§3 + 新22（服务编排） |
| K-157 | Walt Karas AVL 宏模板机制（cavl_if/cavl_impl 实例化、unavl 清理） | 机 | 14§ | cavl_impl.h:1208 行 | 新13§5（C 侧） |
| K-158 | [ARCH: A-4] AVL→BTreeMap 等价论证（O(log n)、SearchType、find_slot 语义） | 演 | 14§ | region_map.rs | 新13§5（主点） |
| K-159 | `find_overlap` 前驱探测 + 零长 insert 拒绝（Fix #72） | 机 | 13/14 | region_map.rs | 新13§5 |
| K-160 | region id 全局计数器与 `release_shared_remap`（Fix #64 落点） | 机 | 13 | vir_region.rs；region/mod.rs | 新13§3 |
| K-161 | `vrallocflags`：VR 标志→PAF 分配旗标的映射 | 接 | 13 | region.c | 新13§3 |
| K-162 | `map_writept`/`map_ph_writept`：区域层写 PTE 的收口 | 机 | 13/17 | region.c:257-295,906 | 新13§4 |
| K-163 | `map_printmap`/`printregionstats`/`map_sanitycheck` 调试与自检面 | 工 | 13 | region.c:168-250 | 新13§6 + 新29 引用 |
| K-164 | 惰性分配家族的删除判定（`map_lazy`/Reserved 态，Fix #70.5） | 演 | 13§3.2 | vir_region.rs | 新13§3（现状两态语义；判定叙事归 todo） |

**旧 15（ipc-dispatch → 新 14，9 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-170 | 五优先级分发结构（VFS transid → RS_INIT → VM_PAGEFAULT → ACL 闸+CALLMAP → ENOSYS） | 机 | 15§1 | main.c:137-176 | 新14§2（主点） |
| K-171 | CALLMAP 注册表与守护测试 `test_callmap_registration_matches_c` | 接 | 15§ | dispatcher.rs:681/:682 起（§3.6 E4 重锚） | 新14§3（主点） |
| K-172 | SUSPEND 伪返回码协议（不回复、等续作） | 约 | 15§ | com.h:1151；main.c:181 | 新14§4（主点） |
| K-173 | transid 路由（TRNS_GET_ID/DEL_ID 与 VFS 事务） | 接 | 15§ | vfsif.h:79-81 | 新14§5（机制主点）+ 新15（编码规范主点 K-N01） |
| K-174 | `is_ipc_notify` 过滤与伪造源防御（debug_assert→audit，Fix #67） | 约 | 15§1.5 | main.c:125-129,154-157 | 新14§6 |
| K-175 | ACL 闸接线（与 K-055 分工：04 管策略、14 管接线） | 机 | 15§ | main.c:168 | 新14§7 |
| K-176 | 回复编码与 `ipc/encode.rs`（reply_to_errno/encode_reply_data 迁移后形状，Fix #78） | 接 | 15 头部注记 | ipc/encode.rs | 新14§8 + 新15（wire 编码规范） |
| K-177 | transport 抽象（`IpcTransport`/`KernelIpcTransport`/`TestIpcTransport`；fail-fast 连续失败上限） | 接 | 15 头部 | ipc/transport.rs | 新14§9（机制）+ 新29（测试注入主点） |
| K-179 | 信号臂 `handle_signal`（SIGKMEM 的 Rust 入口，wired at E1 标注） | 机 | 15/01 | vm_server.rs:827 | 新14§6 + 新01§3.6 |

> 旧 15 范围另有一条**新增**知识点：K-N06（panic=abort 崩溃模型，见 §2.3，锚点 os/Cargo.toml:249/:252、vm_server.rs:1095/:1206）——正文未系统化，归位新14§10，不设存量编号（避免与 K-N06 双计）。

**旧 16（pagefault → 新 17，9 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-180 | 缺页是"内核停住进程→VM 仲裁→内核恢复"的三方协议 | 概 | 16§1 | pagefaults.c | 新17§1 |
| K-181 | 被动路径：VM_PAGEFAULT 消息（PF 状态机 pf_state + handle_pagefault 七步） | 机 | 16§2 | pagefaults.c:76-158 | 新17§2（主点） |
| K-182 | 主动路径：SIGKMEM → do_memory → handle_memory 状态机（start/once/step/final/continue） | 机 | 16§3 | pagefaults.c:170-417 | 新17§3（主点） |
| K-183 | `map_pf` 区域框架消费（查区域→权限→memtype 分发→写 PTE） | 机 | 16/13 | region.c:664-754 | 新17§4 + 新13 框架引用 |
| K-184 | SUSPEND/异步恢复与 major/minor 缺页计数 | 约 | 16§ | pagefaults.c:132-135 | 新17§5 |
| K-185 | TLB 结构不变量 §3.7（"VM 只改不在运行的进程的页表"+逐路径停等表+违反后果+准入门槛） | 约 | 16§3.7 | — | **迁出**→新16（K-N02 主点）；新17 引用 |
| K-186 | `PagefaultAction`/`cow_resolve` 的缺页消费入口（Rust） | 机 | 16 头部 | cow_exec_pf.rs | 新17§6 + 新18（CoW 机制主点） |
| K-187 | 缺页错误路径三分：SIGSEGV kill + 清页故障（无区域/写只读/处理失败） | 机 | 16§2 | pagefaults.c:87-151 | 新17§2 |
| K-188 | 缺页链写 PTE（Fix #60）后的故障链面（快/慢路、munmap/brk/exit 闭环复核结论） | 事 | todo§18.3 | cow_exec_pf.rs | 新17§6（现状）+ 新16（不变量） |

**旧 17（cow-mechanism → 新 18，8 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-190 | CoW 协议三要素：共享建立（refcount++）/写保护（PTE RO）/首次写分裂（mem_cow） | 概 | 17§1 | pb.c | 新18§1（主点） |
| K-191 | `mem_cow` 分裂与本权转移 | 机 | 17§2 | pb.c:136-168 | 新18§2 |
| K-192 | `setup_cow_for_all_regions`+`write_page_table_mappings`（fork 时的预写保护） | 机 | 17/18 | vmproc_handle.rs:488-542 | 新18§3 + 新19（编排） |
| K-193 | 快路/慢路：refcount≤1 复用捷径必须以 `is_page_writable` 为门（V13-P1-1 活循环教训，Fix #63） | 约 | 17§1.5/16§3.3 | cow_exec_pf.rs:268-285 | 新18§4（主点） |
| K-194 | CoW 后换型 anon（C cow_block "After COW we are a normal piece of anonymous memory"） | 约 | 17§ | mem_file.c:70-71 | 新18§4 |
| K-195 | Linux `do_wp_page`/Redox refcount 对照 | 演 | 17§1.5 | Linux mm/memory.c | 新18§1.5 |
| K-196 | file-backed `cow_block` 与 VFS 交互的边界（本体在 VFS 协作篇） | 边界 | 17 头部 | mem_file.c:59-77 | 新24（K-222 主点）；新18 引用 |
| K-197 | `pr_writable`/`map_writept`/`map_copy_region` 的区域层支撑 | 机 | 17§ | region.c:130-134,820-849,906 | 新18§3 |

**旧 18～22（fork/brk/mmap/munmap/exit → 新 19～23，逐篇 5～7 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-200 | `do_fork` 十步编排（验证→结构体复制→pt_new→map_proc_copy→flags→acl_fork→sys_fork→pt_bind→handle_memory_once→返回）与可回滚性 | 机 | 18§ 全 | fork.c:32-115 | 新19§2（主点） |
| K-201 | fork 次主线路径图（fork 触及 03/04/06/07/08/11/12/13/17/19 十篇的地图） | 概 | 18§（plan §1.3） | — | 新19§1 |
| K-202 | `sys_fork` gateway 出参语义（endpt+msgaddr 应答臂，E-FORKMSG 修复后的 eager-CoW 相） | 接 | 18 头部 | kernel_gateway.rs；E-FORKMSG | 新19§3 + 新15（wire 例证） |
| K-203 | fork 回滚路径（`free_forked_regions` + pt_free） | 机 | 18 | fork.rs:160-175 | 新19§2 |
| K-204 | `do_brk`/`real_brk` 三态编排（grow/shrink/no-change）+ 惰性 vs 真释放 | 机 | 19§ | break.c:44-69 | 新20§2（主点） |
| K-205 | brk 与 libc sbrk 的分层（libc 封装在调用方） | 接 | 19 头部 | lib/libc/sys/brk.c | 新20§1 + 新15（客户端视角） |
| K-206 | DATA_CHANGED/STACK_CHANGED 声明面与 do_brk 不处理栈增长的事实 | 事 | 19 头部 | break.c:38-39 | 新20§4 |
| K-207 | `mmap_region` 三路解析 + 匿名同步/文件异步分流 + `map_perm_check` execpriv 分级 | 机 | 20§ | mmap.c:36-307 | 新21§2（主点） |
| K-208 | `do_map_phys`/`do_remap(_ro)` 辅助路径与 VM_GETPHYS 的区分（K-235 分工） | 机 | 20/21/26 | mmap.c:310-435 | 新21§4 + 新27（查询） |
| K-209 | MMAP_BASE/MMAP_TOP 64 位窗口与 addr 分配（A-6 消费面） | 约 | 20§ | mmap.rs:203-204 | 新21§3 + 新07（A-6 主点） |
| K-210 | `do_munmap` 统一入口（MUNMAP/UNMAP_PHYS/SHM_UNMAP 三号一线）+ 范围拆除四情形 | 机 | 21§ | mmap.c:488-573；region.c:1222-1294 | 新22§2（主点） |
| K-211 | 物理页引用解除（`pb_unreferenced` 漏斗）与 fdref deref 入队 | 机 | 21/23 | pb.c:96-134；fdref.c:116-154 | 新22§3 + 新24 引用 |
| K-212 | `VM_MAP_PHYS` 特权映射（与 munmap 同篇的对称性：map_perm_check 共用） | 机 | 20/21 | mmap.c:310-363 | 新21§4 + 新22§4 |
| K-213 | 两阶段退出协议（VM_WILLEXIT 预通知 + VM_EXIT 正式）与 typestate 联动 | 机 | 22§ | exit.c:60-114 | 新23§2（主点） |
| K-214 | `free_proc`/`clear_proc`/`reset_vm_rusage` 资源清算顺序 | 机 | 22§ | exit.c:25-58 | 新23§3 |
| K-215 | `VM_PROCCTL` 双语义（VMPPARAM_CLEAR 清场重建页表 = exec 的 VM 半；VMPPARAM_HANDLEMEM 地址可达保障） | 机 | 22§ | exit.c:117-156 | 新23§4（主点） |
| K-216 | VFS transid 驱动的 procctl 续作（L5 优先级的消费方） | 接 | 22/15 | main.c:143-148 | 新23§5 + 新14 引用 |
| K-217 | 退出资源闭环（页表树 destroy + 物理页归还 + fdref 收尾；E4 余件：aarch64+register_free） | 约 | 22/todo§1 | exit.rs:98-142 | 新23§6（edge 指针） |

**旧 23～26（vfs/page-cache/rs/queries → 新 24～27，逐篇 6～8 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-220 | VFS 异步对话协议：FDLOOKUP/FDIO/FDCLOSE 三链路 + 串行激活请求队列 | 接 | 23§1-2 | vfs.c:60-104 | 新24§2（主点） |
| K-221 | fdref 引用计数表（new/ref/deref/dedup_or_new）与延迟 FDCLOSE | 数 | 23§3 | fdref.c 全 | 新24§3 |
| K-222 | mappedfile memtype 的缺页分流（缓存命中/ONCE/NeedVfsIo/尾页 clearend） | 机 | 23/24 | mem_file.c:84-155 | 新24§4 + 新25（缓存主点） |
| K-223 | `mmap_file_cont` VFS 异步续作与 SUSPEND 衔接 | 机 | 23/20 | mmap.c:160-190 | 新24§5 + 新21 引用 |
| K-224 | VFS_VMCALL wire（E-VFSWIRE 已通：MessVmVfsCall 56B、排水步重试） | 接 | 23 头部 | minix-types；E-VFSWIRE | 新24§6 + 新15（wire 主点） |
| K-225 | 页缓存双哈希目录（dev,offset 主键 + dev,ino,ino_offset 辅索引）+ 精确 LRU + refcount 淘汰 | 数 | 24§1-2 | cache.c 全；cache.h | 新25§2（主点） |
| K-226 | 四个缓存 IPC（mapcache/setcache/forgetcache/clearcache）与回滚语义（Fix #74） | 接 | 24§3 | mem_cache.c:95-324 | 新25§3 |
| K-227 | `VMSF_ONCE` 一次性页与往返差偏差登记（Fix #80/G-V12-11） | 约 | 24§3.6 | mem_cache.c:149；libminixfs | 新25§5 |
| K-228 | `cache_freepages` 分配器回收重试（C 侧；Rust 已删 FREE_CACHE_BATCH 链，Fix #70.4） | 机 | 24§3.7 | cache.c:288-307；alloc.c:260-262 | 新25§6（C 对照）+ 新06 引用 |
| K-229 | RS 服务四 IPC：SET_PRIV/PREPARE/UPDATE/MEMCTL + RprocTab 握手 + `adjust_proc_refs` | 接 | 25§ | rs.c 全 | 新26§2（主点） |
| K-230 | [ARCH: A-8] LU 缺口契约（RS_PREPARE 第 5 步/RS_UPDATE 切换未实现；MAKE_VM 恒拒登记） | 演 | 25§3.9 | rs.rs:504 | 新26§4（主点） |
| K-232 | `sef_cb_init_vm_multi_lu` 的 IPC 白名单与批量 update（C 机制；Rust 未实现登记） | 机 | 25/01 | main.c:592-672 | 新26§3 + 新01 生命周期引用 |

> 旧 25 范围另有一条**新增**知识点：K-N07（E-RSWIRE/E-VFSWIRE/E1 通电现状面，见 §2.3）——正文薄于 todo，归位新26§5（RS 域主点）与新24§6（VFS 域），不设存量编号（避免与 K-N07 双计）。
| K-233 | 四个只读查询：INFO(STATS/USAGE/REGION)/GETPHYS/GETREF/GETRUSAGE | 接 | 26§ | utility.c:100-184；mmap.c:438-483 | 新27§2（主点） |
| K-234 | usage 统计口径（`get_usage_info_kernel`/`get_usage_info_vm` 特判 + `TOTAL_PAGES` 全局口径，Fix #70.1） | 机 | 26§4.4 | region.c:1357-1447 | 新27§3 |
| K-235 | GETPHYS/GETREF 实为 region id 与 remaps 计数（非物理地址）的诚实定位 | 约 | 26§ | mmap.c:438-483 | 新27§4 |
| K-236 | region_info 分页游标（MAX_VRI_COUNT 与 next 游标） | 接 | 26§ | region.c:1452-1505 | 新27§5 |
| K-237 | getrusage PM-only 与计数器清零时机（exit 联动） | 约 | 26/22 | utility.c:426-472 | 新27§6 + 新23 引用 |

**旧 99（global-concepts，6 条）**

| 编号 | 名称 | 类型 | 现有位置 | 代表锚点 | 新去向 |
|------|------|------|---------|---------|--------|
| K-240 | 常量语义半径方法论（每个魔数的权威定义与消费点） | 概 | 99§1.1 | — | 新99§1 |
| K-241 | 调用号族（VM_RQ_BASE=0xC00/NR_VM_CALLS=49/VM_PAGEFAULT 区外编码） | 接 | 99§2.1 | com.h:627-780 | 新99§2.1（主点） |
| K-242 | PAF/WMF 旗标族 | 接 | 99§2.2 | vm.h | 新99§2.2（主点） |
| K-243 | VR/VMSF/VMC 旗标与哨兵（VMSF_ONCE 条目级、VMC_NO_INODE=0 依赖 dev=0 非法） | 接 | 99§2.3 | region.h；cache.h | 新99§2.3（主点） |
| K-244 | glo 全局显式化三档（VmContext/global.rs/模型消除） | 演 | 99§1.2 | global.rs | 新99§3.1 |
| K-245 | 常量权威位置判据（跨 crate → minix-types；单源化先例） | 工 | 99§3 | — | 新99§3.2 |

### 2.3 知识点池·新增（来源：§3.2 覆盖缺口；证据锚点强制）

| 编号 | 名称 | 类型 | 证据锚点 | 读者收益 | 新去向 |
|------|------|------|---------|---------|--------|
| K-N01 | VM 消息线格式系统化：`minix/ipc.h` 消息结构族 ↔ `minix-types` 专属 wire struct、overlay 解码错位三例（getphys/getref addr 16→8、rusage children 4→16）、56 字节 LP64 见证断言纪律、M1/M2/M10 载荷约定 | 接 | os/libs/minix-types/src/ipc/message.rs；ipc/vm.rs:837-1011；todo V13-P2-6；E-RSWIRE 进度 | 按 C libc 语义写消费方时不再接错字段；理解 wire 单源纪律 | 新15（主点） |
| K-N02 | TLB 纪律不变量系统化：不变量陈述 + 逐路径停等表（fault/munmap/brk/mmap/fork/exit/RS pin）+ 违反后果 + 新路径准入门槛 + C VMINHIBIT 动态停等对照 + SMP 半边（E-VMTLB） | 约 | 16§3.7（现有正文）；pagetable.c:799-815；minix3/minix/kernel/proc.c:345-347；edge E-VMTLB | 任何新的"对运行中进程做 map/unmap"路径先过这道门，防静默内存腐坏 | 新16（主点） |
| K-N03 | 内核死 VMCTL 面：FlushTlb/InvlPg/GetPdbr 三命令已实现而唯一合法消费者零调用（direct map 下预期），dead-until-needed 注释锚 | 工 | os/kernel/src/syscall.rs:2177-2202；todo Fix #69(b) | 防 reviewer 反复误判为漏接 | 新16§ |
| K-N04 | DMA 连续内存服务：`DmaMemory`/`DmaRegion` 契约、`VmDmaMemory` 簿记+纯算术实现、Direct Map 恒定翻译免建映射、连续页漏斗 `alloc_contiguous` | 机 | os/servers/vm/src/dma.rs 头部；edge E-DMABUF；对照 Redox common/src/dma.rs | 用户态驱动服务器如何合法获得总线可用连续内存 | 新28（主点） |
| K-N05 | SEF 协议面双实现：VM 内联自持（握手折叠/notify 一律 continue/信号臂）vs `minix-sef` 共享库（SefEvent 分类+ping 拦截）；E1 信号投递通道裁决点 | 演 | todo V14-P2-1；main.rs:51-54；vm_server.rs:961,973,827 | 理解 VM 为何有两份 SEF 形状、切换裁决何时做 | 新01§3.6（主点） |
| K-N06 | 崩溃与失败模型：panic=abort、两处 fail-fast panic（transport 永久损坏/ipc_send 失败）、fail-fast 连续计数与进程级 fail-closed 审计两套语义的分工 | 约 | os/Cargo.toml:249/:252；vm_server.rs:1095（transport 永久损坏）/:1206（ipc_send 失败）——todo §18.3 的旧行号 :991/:1069 已漂移，本蓝图一手重锚（§3.6 E2/E3） | 理解 VM 错误处理的不可恢复边界与可观测兜底 | 新14§10（主点） |
| K-N07 | E-RSWIRE/E-VFSWIRE/E1 通电后的实施现状面：rproctab 解码实体、sys_safecopyfrom 落地、VFS 排水步、E1 真机验证状态——文档现状与 todo 判定的同步责任 | 演 | edge_todo.md E-RSWIRE（2026-09-16 进度）；E-VFSWIRE（d136491ee）；E1 切片 5（6d8e52c3d） | 读文档即知哪些路径已真实通电、哪些仍是 mock 形状 | 新26§5 + 新24§6（各自领域） |
| K-N08 | 测试接缝体系：`KernelGateway`（生产 Trap/Mock 双实现）、`SimPaging` 注入、`TestIpcTransport` 回放、分配器三后端 parity 矩阵、feature 组合（默认/segment_tree_alloc/buddy_alloc） | 工 | kernel_gateway.rs；pagetable/sim.rs；ipc/transport.rs:215；allocator_tests.rs；todo §0.2 基线 | 无真实硬件即可驱动缺页/CoW/页表语义单测；理解三矩阵基线 | 新29（主点） |
| K-N09 | 可观测性体系：`audit_log!` 双宏（feature 版 no_std sink / no-op 版）、`VmContext` 计数器（pagefault_errors/dropped_messages/alloc_failures）、`verify_refcounts`、`map_sanitycheck` 面对照 | 工 | lib.rs audit 宏（Fix #81）；vm_server.rs；sanity.rs:54；region.c:168-250 | release 下仍有观测面；测试与自检的边界 | 新29§4（主点） |
| K-N10 | VM 自身内存布局总图：VM_OWN_HEAPBASE/VM_OWN_MMAPTOP 申报窗口、VM_HEAP_* 堆窗口、MMAP_BASE 用户窗口、Direct Map 窗口、VmBootHandoff 页的地址选择（KERNINFO_USER_VA 同 PD 约束） | 约 | main.c:582-586；direct_map.rs；mmap.rs:203-204；edge E-KERNINFO（kernel 侧） | 一张图回答"VM 的地址空间是怎么切的" | 新00§新增布局节（主点）+ 各篇引用 |

### 2.4 重复与主讲述点标记（汇总；明细论证见 §3.3）

| 知识点组 | 现有重复位置 | 主点（新目录） |
|---------|-------------|---------------|
| TLB/invlpg/自刷（K-004/K-088/K-097/K-185） | 00§1.2、07、08§1.8、16§3.7 | **新16**；08 保留 C 对照机制论证并引用 |
| CALLMAP 表（K-006/K-171） | 00§1.3/§3.1、15、99 | **新14**；00/99 指针化 |
| SUSPEND 协议（K-172） | 15 主讲 + 17/21/22 各自提及 | **新14**；服务篇只引用 |
| 测试基线数字（00§3.2 与各篇§5） | 00 汇总数字 + 28 篇 §5 | 各篇 §5 主点自己的；**新00 数字指针化**；新29 汇总口径 |
| Direct Map（K-073/K-083） | 06 引入、07 显式章节、08/dma 消费 | **新07**；06/08/28 引用 |
| `vm_isokendpt`/endpoint 编码（K-041/K-042） | 03 机制 + 99 词汇 | **新03 管校验机制、新99 管编码常量**（边界写入两契约） |
| fork 路径图（K-201） | plan §1.3 + 18 | **新19** |
| writable 恒 false（K-147） | 12 语义 + 16/17 消费 | **新12**；新18 引用 |
| 可观测性计数（K-N09） | 各篇零散 | **新29** 汇总表 |
| memtype 反向依赖判定（K-145） | 12 + todo Fix #76 | **新12** |

### 2.5 统计摘要

- 存量条目：**197 条**（按旧文档分布：00:7、01:12、02:8、03:7、04:7、05:11、06:9、07:10、08:11、09:8、10:8、11:8、12:9、13+14:15、15:9、16:9、17:8、18-22:18、23-26:17、99:6；其中 5 条在旧目录内已标"迁出/主点转移"；另有 3 处"正文未讲、todo 有账"的知识点不设存量编号，直接归新增 K-N03/K-N06/K-N07，避免双计）。
- 新增条目：**10 条**（K-N01～K-N10，全部带证据锚点）。
- 池合计 **207 条**。类型分布（约）：概念 25、机制 75、数据结构 27、接口与协议 40、约束与不变量 19、架构演进 17、工具与工程 12、测试性质 8、事实/边界/其它 4（按各表"类型"列汇总，允许一条多义时取主类型；合计按主类型近似 207）。
- 去向覆盖：每条存量与新增条目的"新去向"列均已落实（G5 核对见 §9）。

---

## 3. 覆盖审计

### 3.1 主题全集与来源

主题全集四路来源：

1. **C 源码符号面**：24 个 .c 的函数/结构体/宏/状态机/错误路径（机器口径 371 个 C 符号，coverage-extract 2026-09-09；语义判定以 todo §17.1 的 175 项为准——COVERED 103 / ARCH-EVOLVED 32 / DEBUG-ONLY 27 / COMPILE-FLAG 4 / OBSOLETE 8 / REAL-GAP 0）。本审计不重复符号枚举，只回答"每个符号簇在文档里是否有讲处"。
2. **OS 通用概念**：进程控制块、进程表、地址空间、页表层级、按需分页、CoW、引用计数、伙伴/位图分配器、slab、LRU 缓存、缺页状态机、写时复制的 SMP 语义、TLB 一致性、微内核 IPC 协议、访问控制、Live Update/热更新。
3. **非 C 制品承载的主题**：链接脚本、页表架构宏、调用号与消息线格式、endpoint 编码、transid、boot 协议、libc 封装、Rust 测试基建、Cargo feature 矩阵、minix-sef 库、minix-types wire 单源。
4. **阶段边界契约里属于本 stage 的主题**：master-plan README 的"VM 为全系统建页表"角色、edge_todo 的 VM 侧活指针（E-VMTLB/E-RSWIRE 余项/E-VFSWIRE 现状/E-DMABUF）、01-stage-kernel 已讲概念的对端（缺页转发、sys_vmctl）。

### 3.2 覆盖缺口表（每条落实为新增知识点或明确不做）

| # | 缺口主题 | 证据 | 建议 | 落实 |
|---|---------|------|------|------|
| G1 | TLB 纪律与页表写入不变量系统化（现分散在 08§1.8 机制论证与 16§3.7 不变量，且 08 在 16 之前形成前向引用） | 16§3.7；08§1.8；todo V13-P2-1 | 新建专篇 | K-N02 → **新16** |
| G2 | 消息线格式与客户端契约系统化（wire 结构族、overlay 教训、断言纪律、libc/PM/VFS/RS 客户端视角）散落在 15/18-26 各篇头部 | V13-P2-6；minix-types；ipc.h | 新建专篇 | K-N01 → **新15** |
| G3 | 测试基建与可观测性（Gateway/SimPaging/TestTransport/parity/feature 矩阵/audit/计数器）散落在 00§4.1 与各篇§5 | 各篇§5；todo §0.2 | 新建专篇 | K-N08/K-N09 → **新29** |
| G4 | DMA 连续内存服务（`dma.rs`，E-DMABUF；C 无对应调用，属 minix-rs 服务扩展） | dma.rs 头部 | 新建专篇（支线） | K-N04 → **新28** |
| G5 | SEF 协议面双实现（V14-P2-1 接线债）正文零覆盖 | todo §19 | 并入启动篇 SEF 节 | K-N05 → **新01§3.6** |
| G6 | 崩溃/失败模型（panic=abort、fail-fast/fail-closed 分工）正文零系统化 | todo §18.3 | 并入分发篇 | K-N06 → **新14§10** |
| G7 | E-RSWIRE/E-VFSWIRE 通电后的现状面（rproctab 解码实体、排水步）正文落后于 todo | edge_todo 2026-09-16 进度 | 并入 RS/VFS 两篇现状节 | K-N07 → **新26§5**、新24§6 |
| G8 | 内核死 VMCTL 面（dead-until-needed）正文零覆盖 | todo Fix #69(b) | 并入 TLB 篇 | K-N03 → **新16§** |
| G9 | VM 自身内存布局总图（窗口/申报/Handoff 地址选择）散落五篇 | main.c:582-586；direct_map.rs；mmap.rs | 并入总览新增布局节 | K-N10 → **新00** |
| G10 | 既有失效引用（代码 1 处旧名 `24-vm-ipc-dispatch`；跨 stage 旧名 `20-vm-exit`×3、`07-pagetable-ops`×1） | §0.4 统计 | §8.3 引用迁移表批量修复 | 非知识点，迁移任务 |
| G11 | `VM_ADDDMA/DELDMA/GETDMA`、`RS_PREPARE/UPDATE` 缺口 | plan §5.4；checklist §6 | **明确不做**（保留 defer + 语义契约，A-8；G7 落地后由新26§5 更新状态） | 维持 defer 登记 |

### 3.3 重复主题表

| # | 主题 | 重复展开位置 | 新目录主讲述点 | 其余处置 |
|---|------|-------------|---------------|---------|
| R1 | TLB/invlpg | 00§1.2 摘要、07 结构、08§1.8 C 对照、16§3.7 不变量 | 新16 | 00 改一句话+引用；07 保留结构侧（Direct Map 为什么免除自刷）；08§1.8 保留机制对照并引用新16 |
| R2 | CALLMAP 注册表 | 00§3.1、15、99§2.1 | 新14 | 00 改指针；99 保留调用号数值表（词汇） |
| R3 | SUSPEND | 15 + 17/21/22 提及 | 新14 | 服务篇仅引用 |
| R4 | 测试数字 | 00§3.2 + 各篇§5 | 各篇§5 | 00 §3.2 改为指针（数字易漂移） |
| R5 | Direct Map | 06/07/08/28 | 新07 | 06 保留"结构性消除备用池"论证（那是 06 自身机制的因） |
| R6 | endpoint 编码 vs 校验 | 03 与 99 | 新03 机制、新99 常量 | 两契约互写边界 |
| R7 | fork 路径图 | plan §1.3 与 18 | 新19 | plan 侧随重建被取代 |
| R8 | memtype writable/换型 | 12 与 16/17/18 | 新12 语义、新18 机制 | 17（新）消费处引用 |
| R9 | LU 三分（10 支撑/25 服务/01 回调） | 10/25/01 | 三分维持，边界写入三契约 | 交叉引用显式化 |
| R10 | 可观测性 | 零散 | 新29 汇总 | 各篇保留自己的机制细节 |

### 3.4 越界主题表

| # | 越界/错位 | 现状 | 处置 |
|---|----------|------|------|
| O1 | 00§3.2 携带具体测试数字（503/521/503），随每轮漂移 | 已两次刷新 | B 相改指针（指向新29 与 todo 基线节） |
| O2 | 多篇正文携带修复史叙事（"V12-P2-4 注记(2026-09-09)""Fix #70"等散布于 06/13/16/24 等正文与头部） | 违反"无迭代史"文风规范 | B 相统一：正文只写现状语义；判定与账目留在 todo/差异表，文档侧仅保留"差异登记表"行（登记≠叙事） |
| O3 | 08 承载 TLB 不变量的完整论证与 16 形成"08→16 前向引用" | 结构性错位 | 08 保留 C 对照（那是页表操作自身的语义），不变量主点移交新16；08 引用之 |
| O4 | 16§3.7 一节承载全 stage 结构不变量 | 单篇内过载 | 整节迁新16（K-185），16（新17）改引用 |
| O5 | 25 讲 `sef_cb_init_lu_restart` 流程细节与 01/10 分界模糊 | 边界重叠 | 三分边界写入契约：01=生命周期叙事、10=机制本体、26=服务面 |
| O6 | 各服务篇头部携带 `os/libs/minix-types` 逐结构行号清单（wire 细节） | 讲述点错位：wire 规范应集中 | 服务篇保留"本服务用什么消息"的最小锚点；布局/断言/overlay 规范移新15 |

### 3.5 非 C 主题逐项回答（固定十项清单）

| # | 主题 | 在哪里讲 / 为什么不在本 stage |
|---|------|------------------------------|
| 1 | 链接与加载 | `arch/earm/vm.lds` 是 C 构建制品，语义 WONTFIX（plan §5.4 判定维持）；boot 进程 ELF **装载语义**（libexec 回调族）在新01§4；Rust 侧 `load_vm_elf` 属 kernel stage（01-stage-kernel/06）。 |
| 2 | 镜像与内存布局 | 新00 新增布局总图（K-N10）；细节：堆窗口新09、用户 MMAP 窗口新21、Direct Map 窗口新07、SEF 申报新01§3.5。 |
| 3 | 汇编入口与陷阱进入 | 不在本 stage：用户态入口 `crt0`/`arch_trap` 属 14-stage-runtime；内核侧陷阱桥属 01-stage-kernel（14-exception/12-ipc）。VM 侧仅在 新01§2.3 承接 Handoff 消费边界。 |
| 4 | 启动装配 | 生产侧（内核装 VM ELF、VMINHIBIT）= 01-stage-kernel/06 与 09-vm-boot-protocol；消费侧（`VmBootHandoff` 读取、A1 adoption、A2 对账）= 新01§2.3。 |
| 5 | 构建与工具链 | `Makefile.inc` WONTFIX；Cargo feature 矩阵（默认/segment_tree_alloc/buddy_alloc 三测试矩阵）= 新29§3。 |
| 6 | 跨模块接口与线格式 | 新15（K-N01）：消息结构族、transid 编码、M1/M2/M10 载荷、断言纪律、客户端（PM/VFS/RS/kernel/libc）视角。 |
| 7 | 错误路径 | errno 映射与 fail-fast/fail-closed/panic=abort = 新14§10（K-N06）；各服务的错误表留在各服务篇。 |
| 8 | 关闭与退出 | 新23（exit/procctl/资源闭环）；内核侧 `register_free` 余件挂 edge E4（指针登记，不在本 stage 展开）。 |
| 9 | 并发与同步 | 单线程事件循环 + 借用检查 = 新00§1.3 主点；`AssumeSyncCell` 跨槽位 = 新02；TLB/SMP 半边 = 新16 + edge E-VMTLB 指针。 |
| 10 | 测试基建 | 新29（K-N08/K-N09）。 |

### 3.6 存量文档可信度抽样核查（用户约束 3 的落实，2026-09-19 一手执行）

**方法**：从各篇头部声明与 todo 参考材料中抽取 16 个锚点/断言样本，本轮直接对 C 源码与 Rust 代码核对（`sed`/`grep` 一手输出），覆盖 03/04/05/06/07/08/09/11/12/13/25 共 11 篇正式文档 + 3 个 todo 记录。样本不是随机公开抽签，而是"每篇头部声明的首行锚点 + 此前唯一挂起项（rs.c）+ 蓝图自身引用过的 todo 行号"——后者是本蓝图最可能照抄传播的漂移。

**抽样结果**：

| # | 样本（文档声称） | 代码实际 | 判定 |
|---|----------------|---------|------|
| 1 | 03：`VMP_EXECTMP` glo.h:17、`VMP_NR` :18、`vmproc[VMP_NR]` :20 | 全部精确命中 | ✅ |
| 2 | 04：NO_ACL allow-all + "for now" 注释 acl.c:44-53 | 精确命中（:44 注释、:45-53 分支） | ✅ |
| 3 | 05/99：PAF_CLEAR 0x01 :22、CONTIG 0x02 :23、ALIGN64K 0x04 :24、LOWER16MB 0x08 :25、LOWER1MB 0x10 :26、ALIGN16K :27 | 行号全中；**但 doc 99 §2.2 写 ALIGN16K=0x20，实际 vm.h:27 = 0x40（0x20 是未定义空洞）** | ⚠️ 值错误 |
| 4 | 06：备用页池自举注释 pagetable.c:55-57 | 注释实际跨 :54-57（±1 行） | ✅ |
| 5 | 07：`pt_t` 结构 pt.h:11-25 | 精确命中 | ✅ |
| 6 | 08：`pt_writemap` :784 / `pt_new` :990 / `pt_copy` :1069 / `pt_bind` :1358 / `pt_free` :1427 / `pt_mapkernel` :1442 | 六个全部精确命中 | ✅ |
| 7 | 09：SLABSIZES :29 / `slaballoc` :259 / `slabfree` :406 / `slabstats` :504 | 全部精确命中 | ✅ |
| 8 | 11：`mem_cow` pb.c:136、`phys_block` region.h:23、`PBF_INCACHE` :35 | 全部精确命中 | ✅ |
| 9 | 12：换型注释 mem_file.c:70-71、"never writable" :173-175 | 命中（注释实际 :70/:174，±1 行） | ✅ |
| 10 | 12：`mem_type_t` name+14 回调 = 15 字段 | 逐字段点数成立 | ✅ |
| 11 | 13：`map_page_region` :463 / `map_pf` :664 / `map_proc_copy` :933 / `split_region` :1150 / `map_unmap_range` :1222 | 五个全部精确命中 | ✅ |
| 12 | 25/todo：`rs_memctl_make_vm_instance` rs.c:218；EPERM 条件 | 函数 :218 精确命中；**函数体一手核对：`num_vm_instances == 2` 才 EPERM（:231-234），首实例成功**——todo V13-P2-4 勘误转述证实，G9 挂起项就此闭单 | ✅ |
| 13 | 15/99：`VM_RQ_BASE` 0xC00 com.h:627、`NR_VM_CALLS` 49 :769、`VM_PAGEFAULT` :773 | 全部精确命中 | ✅ |
| 14 | todo：vm_server.rs 两处 fail-fast panic :991/:1069 | **漂移**——实际 :1095（transport 永久损坏）/ :1206（ipc_send 失败）；Fix #78 移出 encode 代码后行号位移 | ⚠️ 行号漂移 |
| 15 | todo：panic=abort os/Cargo.toml:260-263 | **漂移**——实际 :249（[profile.dev]）/:252（[profile.release]） | ⚠️ 行号漂移 |
| 16 | todo：static CALLMAP dispatcher.rs:975-1000 | **漂移**——实际 `static CALLMAP` 在 :681，注册体在 `const fn build_callmap()` :682 起 | ⚠️ 行号漂移 |

**结论与错误率**：16 样本中——正式文档锚点 13/13 命中（含 3 处 ±1 行的微偏），但其中 1 个样本暴露**数值事实错误**（样本 3，doc 99）；todo 参考材料锚点 3/3 行号漂移。两类错误的成因不同：前者是内容写错无人复核，后者是代码演进后记录未重锚（V13 轮自己登记过"锚点必须当场 sed 验证"的教训，Fix #77 一带）。**抽样通过不构成免检理由**——16 样本只覆盖数百条存量断言的一角，这正是 §2.1 认知定位与 §4.1 原则 8 强制逐条核对的原因。

**存量错误与漂移清单（B 相必修）**：

| # | 位置 | 问题 | 处置 |
|---|------|------|------|
| E1 | doc 99 §2.2（K-242 行） | `PAF_ALIGN16K` 值写 0x20，实际 0x40（vm.h:27） | B 相重写新99 时修正；本蓝图池内无此错误值（未抄值） |
| E2 | todo §18.3 漂移（K-178/K-N06 曾引） | panic 锚点 :991/:1069 → 实际 :1095/:1206 | **本蓝图已重锚**（见 §2.2/§2.3 对应行） |
| E3 | todo §0.2 漂移（K-178 曾引） | Cargo.toml :260-263 → 实际 :249/:252 | **本蓝图已重锚** |
| E4 | todo §18.1 漂移（K-006/K-171 曾引） | CALLMAP :975-1000 → 实际 :681/:682 起 | **本蓝图已重锚** |

> 注：E2–E4 的原始漂移在 todo.md（参考材料），不在 28 篇正式文档内；但本蓝图初稿曾照抄，恰好实证了"存量记录不能直接采信"——审计首先抓到的是蓝图自己。

---

## 4. 新目录

### 4.1 设计原则与操作汇总

1. **主线 = 启动因果链 + 运行时汇聚点**（四条硬标准之 2）：启动段 13 篇严格按 init_vm 执行序排列；运行时从汇聚点（分发）展开并行服务。运行时序事实全部来自 §1 真序表（一手 C 读取），教学序偏离逐条记入 §4.5 序差表。
2. **无前向引用**（硬标准 1）：新编号下每篇前置只指向更小编号；两处既有前向引用（08→16 的 TLB、服务篇→wire 规范）通过新篇插入消除。
3. **并行体不伪造线性**（5.2 规则）：分发 = 统一框架篇；服务按"进程生命周期 / 地址空间维护 / 跨服务协作 / 查询与扩展"四组成篇；组内代表成员精讲 + 差异表。
4. **篇幅软限制**（用户约束 1）：单篇目标 300～900 行；复杂单概念（新08 页表操作、新17 页错误、新21 mmap）允许到 ~1300 行；**超过 1300 行先检查是否混入第二个语义单元，而不是机械拆分**；不为凑长度稀释、不为省事碎片化。
5. **篇数不设限**（用户约束 2）：本次 28 → 31（合并 1、新建 4）；后续任何覆盖审计发现新主题，优先**插入语义带并接受重编号**（断链成本按 §8 的批量迁移法摊销），不为保编号而牺牲讲述顺序。
6. **去迭代史**（O2 全局规则）：B 相重写时正文不携带修复史；差异登记表（❌/⚠️ 行）保留，叙事性注记删除。
7. **知识零丢失**（硬标准 3 的前提）：§2 池 207 条每条有去向；合并篇的每节映射见 §8.2。
8. **真相源优先（用户约束 3）**：存量文档是声明载体不是事实权威——B 相写每篇正文前，必须按契约"事实底线"逐条在 C/Rust 代码里 grep/sed 核对（fix-guard：读目标行 ±5 行，不凭记忆与转述）；存量锚点核对不符时以代码为准修正文档，并按 §3.6 清单回填；契约中的行号锚点视为"定位提示"而非"已验证事实"。

**操作汇总**：重排（重编号）13 篇；合并 2 篇为 1 篇；新建 4 篇；改写 2 篇（00/01 增量重组）；保留编号 13 篇（00 之外的 01-12、99 计 14 篇中 01 另有增节）；归档 1 篇（旧 14 被吸收）。合计新目录 **31 篇 + 99**。

### 4.2 新篇章总表

| 新编号 | 新标题 | 一句话定位 | 操作 | 旧来源 |
|--------|--------|-----------|------|--------|
| 00 | vm-overview：内存语义权威、执行模型与阅读路线 | 入口：VM 是什么、地址空间怎么切、单一主线导航 | 改写重组 | 旧 00 |
| 01 | vm-init-main：启动链、SEF 生命周期与 boot 进程装配 | main() 到主循环之前的一切，含 handoff 消费与 SEF 双实现登记 | 保留 + 增节（§3.6） | 旧 01 |
| 02 | vmproc-struct：进程控制块——字段、标志与生命周期 | VM 如何记住"一个"进程 | 保留 | 旧 02 |
| 03 | vmproc-table：进程表——slot、endpoint 校验与保留槽 | VM 如何记住"所有"进程 | 保留 | 旧 03 |
| 04 | acl：调用权限——门禁位图与 fail-closed | 谁被允许调用 VM 的哪些服务 | 保留 | 旧 04 |
| 05 | physical-memory：物理内存账本——清单、三后端分配器 | 谁拥有哪段物理内存 | 保留 | 旧 05 |
| 06 | page-allocator：VM 自用页分配——双地址问题的解 | VM 如何给自己分配页（VA+PA 一次拿全） | 保留 | 旧 06 |
| 07 | pagetable-struct：页表结构——层级、Direct Map 与多架构 trait | 页表长什么样、VM 怎么读写它 | 保留 | 旧 07 |
| 08 | pagetable-ops：页表操作——生命周期、写映射与跨表复制 | 页表怎么建/改/删/绑/抄/查 | 保留（TLB 边界调整） | 旧 08 |
| 09 | slab-allocator：堆分配——slab 遗产与 HeapArena/VmAllocator | VM 自己的堆从哪来 | 保留 | 旧 09 |
| 10 | vm-relocation：自举终点——从临时分配到动态稳态 | 鸡生蛋问题如何收尾 + LU 支撑面 | 保留 | 旧 10 |
| 11 | phys-pagestate：物理页状态——引用计数与 PFN 索引模型 | 每个物理页被谁负责、被谁共享 | 保留 | 旧 11 |
| 12 | memtype：内存类型系统——六类内存的策略分发 | 一页内存"行为"由谁定义 | 保留 | 旧 12 |
| 13 | region-ledger：区域账本——生命周期、查找与索引选型 | 进程地址空间的区域框架 + AVL→BTreeMap | **合并** | 旧 13 + 旧 14 |
| 14 | ipc-dispatch：主循环与五优先级分发——VM 的心脏 | 消息如何进来、如何路由、如何回复 | 重编号 | 旧 15 |
| 15 | wire-clients：消息线格式与客户端契约 | 跨进程消息的字节布局、解码纪律与调用方全景 | **新建** | K-N01 + 旧 15 拆出 |
| 16 | tlb-discipline：TLB 纪律——页表写入的结构不变量 | 为什么 VM 改页表不需要广播失效 | **新建** | K-185/K-N02/K-N03 + 旧 08§1.8 分工 |
| 17 | pagefault：页错误——被动缺页与主动内存保障 | 缺页如何被仲裁、进程如何被解阻 | 重编号（§3.7 迁出） | 旧 16 |
| 18 | cow-mechanism：写时复制——引用计数与页表权限的协作 | 共享页首次写入时发生什么 | 重编号 | 旧 17 |
| 19 | vm-fork：VM_FORK——地址空间复制的编排 | fork 的 VM 半：可回滚的十步 | 重编号 | 旧 18 |
| 20 | vm-brk：VM_BRK——堆的扩展与收缩 | brk/sbrk 的服务侧 | 重编号 | 旧 19 |
| 21 | vm-mmap：VM_MMAP/VFS_MMAP/REMAP——地址空间分配与内存来源绑定 | mmap 家族的建立路径 | 重编号 | 旧 20 |
| 22 | vm-munmap：VM_MUNMAP/MAP_PHYS/UNMAP_PHYS/SHM_UNMAP——映射拆除 | 拆除与特权物理映射 | 重编号 | 旧 21 |
| 23 | vm-exit：VM_EXIT/WILLEXIT/PROCCTL——退出与进程控制 | 进程生命周期的终点与 exec 的 VM 半 | 重编号 | 旧 22 |
| 24 | vfs-interaction：VFS 异步对话——fdref、文件映射与缺页 I/O | 两个用户态服务器如何跳双人舞 | 重编号 | 旧 23 |
| 25 | page-cache：页缓存——磁盘块目录、LRU 与四个缓存 IPC | 所有文件系统共享的缓存中介 | 重编号 | 旧 24 |
| 26 | rs-services：RS 服务——Live Update 的内存状态迁移执行者 | SET_PRIV/PREPARE/UPDATE/MEMCTL + 通电现状 | 重编号（增节 §5） | 旧 25 |
| 27 | vm-queries：查询服务——内存权威的只读窗口 | INFO/GETPHYS/GETREF/GETRUSAGE | 重编号 | 旧 26 |
| 28 | dma-contig：DMA 连续内存——总线地址翻译（minix-rs 扩展） | 用户态驱动如何合法拿 DMA 内存 | **新建**（支线） | K-N04 |
| 29 | test-infra：测试基建与可观测性 | 无硬件驱动全部内存语义的接缝体系 | **新建**（支线） | K-N08/K-N09 |
| 99 | vm-global-concepts：跨机制共享的常量、旗标与全局状态 | 词汇表（随时查阅） | 保留 | 旧 99 |

### 4.3 阅读路径

- **主线（启动因果链，编号即顺序）**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15 → 16 → 17 → 18 → 19 → 20 → 21 → 22 → 23 → 24 → 25 → 26 → 27。99 在 00 之后即可随时查阅（词汇表，无前置依赖）。
- **第二环（跨服务协作，可整体延后）**：24/25/26/27 四篇假定第一环读完；只关心单机地址空间的读者可从 23 直接跳到 29。
- **支线（可跳读）**：28（DMA，minix-rs 扩展，不读不影响主线）、29（测试基建，工程视角）。
- **专题路线（有目标读者的快速通道，00 中声明）**：① 缺页专题 = 99 → 02/03 → 11 → 12 → 13 → 16 → 17 → 18；② fork 专题 = 99 → 02/03/04 → 13 → 18 → 19；③ LU 专题 = 01§3 → 10 → 23 → 26。专题路线是**索引**不是另一种编号序——正式顺序唯一，以编号为准（消除旧 00§2.3 与编号序并存的二元状态）。

### 4.4 并行主题的分组与代表成员

服务带（新 19～28）是并行体，统一框架 = **新14（分发）+ 新15（wire）**：

| 组 | 成员 | 代表成员（精讲） | 其余（差异表收束） |
|----|------|-----------------|-------------------|
| 进程生命周期类 | 19 fork、20 brk、23 exit/procctl | 19（最复杂、可回滚编排、次主线核心） | 20（最小完整服务，先读热身）、23（逆向清算） |
| 地址空间维护类 | 21 mmap 族、22 munmap 族、17 缺页、18 CoW | 17+18（缺页是 VM 存在理由的核心） | 21/22 建立与拆除互为镜像 |
| 跨服务协作类 | 24 VFS、25 页缓存、26 RS | 24（异步协议最完整） | 25（24 的缓存特化）、26（RS 特化） |
| 查询与扩展类 | 27 查询、28 DMA | 27（只读窗口） | 28（写式扩展，支线） |

主线路径 = 00→27 全序；支线路径 = 28/29；可跳读路径 = 24-27 延后（见 §4.3）。

### 4.5 与四条硬标准的对照（序差表）

| # | 运行时序事实（锚点） | 教学序选择 | 回指补偿 |
|---|---------------------|-----------|---------|
| 1 | 常量与旗标编译期定死，无运行时位置（vm.h/com.h） | 99 放最后编号但声明"00 后随时可读"（词汇表无前置） | 00 导航首行声明 |
| 2 | 进程模型（S6-S10）在 ACL（S7）/物理内存（S9）之间交错执行 | 02/03 整体先于 04/05：先"谁"后"地盘"，概念递进（先具体后抽象反例：此处先个体后资源，因 04/05 的语义依赖 PCB/表概念） | 05 头部声明 init_vm 锚点位置 |
| 3 | `alloc_cycle`/missing_spares 在主循环每轮头部（main.c:118-120），属运行期 | 其机制归 06（启动带）讲 C 侧，其 Rust 删除现状同篇登记 | 06 头部"位置可回答性"声明 |
| 4 | 26 个服务 handler 注册于 S15，之后**任意时刻**可被调用（无运行序） | 按 5.2 并行体分组编排（§4.4），不伪造线性 | 新14 的 CALLMAP 表逐项标注"详见新NN" |
| 5 | 页缓存 handler 在启动后即可用；但其理解依赖 memtype/VFS | 25 编号置于 24 之后（依赖序优先于"可用时点"） | 新25 头部声明运行时可用时点 |
| 6 | TLB 不变量永真（结构性），不是时间点 | 新16 置于分发（14/15）之后、缺页（17）之前：其论证原料 = 14 的阻塞结构 + 15 的消息语义 | 新08/新17 引用 |
| 7 | wire 格式编译期定死 | 新15 置于分发之后：先看消费它的控制流，再看字节布局 | 新14 解码处引用新15 |
| 8 | fork 可发生在任意运行时刻，但其教学理解横跨十篇 | 新19 置于服务带首位，内置次主线路径图回指全部前置 | 新19§1 路径图 |
| 9 | 信号回调（C5）与 VFS 事务（L5）不在直线控制流上 | 新14 显式小节 + 新17§3/新24 各自展开 | §1.6 约束 3 |
| 10 | `vm_isokendpt` 在每次消息到达时执行（L4） | 机制主讲在新03（启动带） | 新14 的 L4 步引用新03 |

四条硬标准复核：① 前向引用零（新编号下逐一扫描各契约"前置"字段，§9 G3）；② 执行/因果序优先且偏离入表（本表）；③ 首次出现即完整（各契约"不讲什么+去向"字段保证）；④ 单篇单语义（合并 13+14 的理由正是把它们还原为一个语义单元，见 §6.3）。

### 4.6 知识点归属与讲述顺序总视图（本节直接回答"哪条知识归哪篇、按什么顺序讲"）

> 记法：`§n K-xxx 名称` = 该篇第 n 节讲该知识点；`(引)` = 该篇只放指针/摘要，**主点**在括号内标注的篇章；`(半)` = 一条知识点按侧面拆到两篇（各承载一半）。每篇内部顺序即 B 相的章节展开顺序；篇间顺序即编号序（§4.3 主线）。本视图与 §2 池、§5 契约三处一致：池给"唯一去向"，契约给"任务书"，本节给"顺序"。

| 新篇 | 篇内知识点讲述顺序 |
|------|-------------------|
| **00** | §1.1 K-001 内存语义权威 → §1.2 K-002 三重权威 → §1.3 K-003 事件循环执行模型、K-004 TLB 摘要（引，主点新16）→ §1.4 K-005 主循环骨架（引，主点新14）→ §2 K-N10 地址空间布局总图 → §3 K-006 parity（指针，主点新14）→ §4 K-007 edge 索引 |
| **01** | §1.1 K-010 鸡生蛋 → §2.1 K-011 fresh 门控 → §2.2 K-013 kinfo 消费 → §2.3 K-014 VmBootHandoff → §2 全程 K-012 init_vm 十三步（主）→ §2.5 K-018 init_proc 调用点（主点新02）→ §2.6 K-068 总页数调用点（主点新05）→ §3 K-015 SEF 六回调（主）→ §3.2 K-019 授权链（主）→ §3.4 K-016 握手折叠偏差 → §3.5 K-021 SEF 申报窗口 → §3.6 K-N05 SEF 双实现 → §4 K-017 exec_bootproc 五步（主）→ §5 K-020 libc 边界 |
| **02** | §1 K-034 endpoint→槽第一动作 → §2 K-030 字段五组（主）、K-018 init_proc 槽激活（主）→ §3 K-031 VMF 位图 → §4 K-032 状态机、K-035 init/clear_proc（主）、K-037 ExitingProc 类型面（半，主点新23）；（K-036 审计引，主点新14） |
| **03** | §1 K-040 表集合语义（主）→ §2 K-041 vm_isokendpt（主）、K-042 代际编码（引，主点新99）→ §3 K-043 slot 分配 → §4 K-044 VMP_EXECTMP（主）、K-033 VMP_NR（主）、K-045 swap_slots（引，主点新10）、K-046 None 表语义面（主点新04） |
| **04** | §1 K-050 攻击面 → §2 K-051 位图、K-053 五函数族（主）→ §3 K-052 分层授权 → §4 K-046 None 即拒绝（主）、K-056 派生回收、K-055 None 闸门三分（主，§4.4）→ §5 K-054 A-11 fail-closed（主） |
| **05** | §1 K-060 来源链、K-059 两层分界（引，主点新11）→ §2 K-061 get_mem_chunks → §3 K-062 分配器本体（主）、K-065 PAF 消费面（主点新99）→ §4 K-063 三后端、K-064 buddy 三段式、K-126 relocate 审计（引，主点新10）→ §5 K-067 记账、K-068 总页数（主）、K-069 parity（引，主点新29）；K-066 reservedqueue 边界（引，主点新06） |
| **06** | §1 K-070 双地址问题（主）→ §2 K-071 vm_* 函数族、K-075 pagelock、K-076 自用页计数、K-137 alloc_contiguous 漏斗（主）→ §3 K-066 reservedqueue（主承接）、K-072 备用页池（主）、K-073 A-1 结构消除（主）→ §4 K-074 spares 删除现状、K-077 vm_self_query、K-078 回收重试 |
| **07** | §1 K-080 页表本质 → §2 K-081 pt_t/宏 → §3 K-082 A-2 四级 → §4 K-083 Direct Map（主）、K-073（引）→ §5 K-084 A-9 自映射 → §6 K-085 A-6 宽度（主）→ §7 K-086 A-10 trait → §8 K-087 pt_init 结构面、K-089 pt_bind/mapkernel 结构面（操作主点新08） |
| **08** | §1 K-090 六动词、K-099 六路径消费地图（主）→ §1.8 K-088 write_pte_dm 逐条 invlpg（机制主点，不变量半→新16）、K-097 C 自刷四处对照（C 对照主点）→ §1.10 K-098 VMINHIBIT 停等（主）→ §2 K-091 生命周期（主）、K-089 操作面（主）→ §3 K-092 pt_writemap+WMF（主）、K-101 Paging trait → §4 K-093 ptalloc → §5 K-094 跨表复制 → §6 K-095 查询校验 → §7 K-096 内核协作（主） |
| **09** | §1 K-110 堆分界线（主）→ §2 K-111 slab 机制（主）、K-112 SLABALLOC 宏 → §3 K-113 A-3 v2（主）、K-115 vm_self_map 消费、K-116 堆窗口 → §4 K-114 MEMPROTECT → §5 K-117 slabstats 衔接 |
| **10** | §1 K-120 为何不能长期用 → §2 K-121 C 搬迁（主）→ §3 K-122 Rust 搬迁 → §4 K-123 swap 族（主）、K-124 transfer/过户、K-125 map_setparent、K-045 swap_slots（主）、K-126 relocate 审计（引）→ §5 K-127 LU 三分边界（主） |
| **11** | §1 K-130 两层/三层结构（主）、K-059 两层分界（主）→ §2 K-131 pb 五函数 → §3 K-132 PBF 与不变量 → §4 K-134 PageFrames PFN 模型（主）、K-135 三态建模；K-133 mem_cow 位置（引，主点新18）、K-136 verify_refcounts（引，主点新29） |
| **12** | §1 K-140 策略模式 → §2 K-141 mem_type_t → §3 K-142 trait MemType（主）、K-143 六类型语义、K-144 PagefaultResult（主）、K-148 类型级回调、K-146 ev_delete/remaps（§3.7）、K-147 writable 恒 false（主）→ §5 K-145 反向依赖判定 |
| **13** | §1 K-150 三层分工（主）→ §2 K-151 vir_region 结构、K-152 Rust 形状 → §3 K-153 生命周期族、K-161 vrallocflags、K-155 map_proc_copy（引，编排主点新19）、K-156 拆除拆分（引，服务编排新22）、K-160 region id/release_shared_remap、K-164 惰性删除现状 → §4 K-154 map_pf 框架（主）、K-162 writept 收口 → §5 K-157 AVL 模板、K-158 A-4 BTreeMap（主）、K-159 find_overlap/零长 → §6 K-163 调试自检面 |
| **14** | §2 K-170 五优先级（主）→ §3 K-171 CALLMAP+守护测试（主）→ §4 K-172 SUSPEND（主）→ §5 K-173 transid 机制（机制主点，编码规范半→新15）→ §6 K-174 notify/伪造源、K-179 信号臂 → §7 K-175 ACL 接线 → §8 K-176 回复编码 → §9 K-177 transport（机制，注入主点新29）→ §10 K-N06 崩溃模型（主）、K-036 进程级审计（主） |
| **15** | §1 K-N01① C union 模型与载荷约定 → §2 K-N01② wire struct+断言纪律、K-202 sys_fork 出参（例证引，主点新19）→ §3 K-N01③ overlay 三错位案例 → §4 K-N01④ transid 编码、K-173 编码规范承接（半）→ §5 K-N01⑤ 客户端全景表、K-205 libc 分层（引，主点新20）；K-224/K-N07 VFS wire 现状（引） |
| **16** | §1 K-N02① 不变量陈述（K-185 §3.7 正文迁入底稿）、K-004 摘要呼应 → §2 K-N02② 逐路径停等表 → §3 K-N02③ 违反后果+准入门槛 → §4 K-N02④ VMINHIBIT 对照、K-098（引）→ §5 K-N02⑤ SMP 半边（E-VMTLB 指针）、K-N03 死 VMCTL 面（主）；K-088/K-097（引） |
| **17** | §1 K-180 三方协议 → §2 K-181 被动路径（主）、K-187 错误三分 → §3 K-182 主动路径（主）→ §4 K-183 map_pf 消费 → §5 K-184 SUSPEND/计数 → §6 K-186 Rust 消费入口、K-188 现状故障链 |
| **18** | §1 K-190 CoW 三要素（主）、K-195 Linux/Redox 对照 → §2 K-191 mem_cow → §3 K-192 预写保护、K-197 区域层支撑 → §4 K-193 快路 writable 门（主）、K-194 换型 anon；K-133（引）、K-196 cow_block（引，主点新24） |
| **19** | §1 K-201 次主线路径图（主）→ §2 K-200 十步编排（主）、K-203 回滚路径 → §3 K-202 sys_fork 出参（主） |
| **20** | §1 K-205 libc 分层（主）→ §2 K-204 三态编排（主）→ §4 K-206 STACK_CHANGED 事实 |
| **21** | §2 K-207 三路解析+分流（主）→ §3 K-209 64 位窗口 → §4 K-208 辅助路径（主）、K-212 MAP_PHYS（建立半） |
| **22** | §2 K-210 统一入口+四情形（主）→ §3 K-211 引用解除漏斗 → §4 K-212 MAP_PHYS（拆除半） |
| **23** | §2 K-213 两阶段退出（主）、K-037 ExitingProc/reap（主）→ §3 K-214 清算顺序 → §4 K-215 PROCCTL 双语义（主）→ §5 K-216 transid 续作（主）→ §6 K-217 资源闭环+E4 指针 |
| **24** | §2 K-220 三链路+队列（主）→ §3 K-221 fdref → §4 K-222 缺页分流（引，主点新25）、K-196 cow_block（主承接面）→ §5 K-223 mmap_file_cont → §6 K-224 VFS_VMCALL 现状、K-N07 通电现状（VFS 域） |
| **25** | §2 K-225 双哈希+LRU（主）→ §3 K-226 四缓存 IPC → §4 K-222 分流+clearend（主）→ §5 K-227 VMSF_ONCE → §6 K-228 cache_freepages C 对照 |
| **26** | §2 K-229 四 IPC+握手（主）、K-019 授权链（引）→ §3 K-232 multi_lu 白名单、K-123/K-124 swap/过户（引，主点新10）→ §4 K-230 A-8 缺口（主）→ §5 K-N07 通电现状（主，RS 域） |
| **27** | §2 K-233 四查询（主）→ §3 K-234 统计口径 → §4 K-235 诚实定位（主）→ §5 K-236 分页游标 → §6 K-237 PM-only |
| **28** | §1 K-N04① 为何用户态化后要新契约 → §2 K-N04② DmaMemory 契约分工 → §3 K-N04③ 簿记+纯算术 → §4 K-N04④ alloc_contiguous 漏斗 |
| **29** | §2 K-N08①② 注入缝与 transport → §3 K-N08②④ feature 矩阵与诚实标注、K-069 parity（汇总引）→ §4 K-N09 可观测性（主）、K-136/K-163/K-177（汇总引） |
| **99** | §1 K-240 方法论 → §2.1 K-241 调用号族（主）→ §2.2 K-242 PAF/WMF（主）、K-065（承接）→ §2.3 K-243 VR/VMSF/VMC（主）→ §3.1 K-244 三档显式化 → §3.2 K-245 位置判据 → §3.3 K-042 endpoint 代际编码（主） |

**完整性声明**：上表覆盖 §2 池全部 207 条——每条在其主去向篇恰出现一次；`(引)`/`(半)` 标记与 §2.2/§2.3"新去向"列逐一对应。机器校验命令（B 相开工前可复跑）：

```bash
# 池 ID 与总视图 ID 的差集应为空
diff <(grep -oE '^\| K-[0-9N]+' doc_rerank_glm.md | sort -u) \
     <(sed -n '/^### 4.6/,/^---/p' doc_rerank_glm.md | grep -oE 'K-[0-9N]+' | sort -u)
```

---

## 5. 每篇契约

> 每份契约是 B 相写正文的任务书。格式：定位 / 讲什么（池编号）/ 不讲什么（去向）/ 前置 / 后置 / 事实底线 / 知识点清单 / 验收标准。"前置"只允许指向更小编号（G3）。所有契约共同遵守 §4.1 原则 4/6（篇幅软限制、去迭代史）与 O1（00 数字指针化）；**篇内知识点的先后以 §4.6 总视图为准**（契约的"知识点清单"是集合视图，§4.6 是顺序视图）。
>
> **核对纪律（§4.1 原则 8，用户约束 3）**：各契约"事实底线"中的行号锚点是**定位提示**，不是已验证事实——B 相动笔前必须逐条 grep/sed 复核（读目标行 ±5 行），以代码实际为准；存量正文与本文档的转述一律不得直接采信（抽样依据见 §3.6）。

### 00-vm-overview

- **一句话定位**：回答"VM 在微内核里管什么、它的地址空间怎么切、按什么顺序读这套文档"。
- **讲什么**：K-001/002/003/004/005/006/007、K-N10。
- **不讲什么**：任何机制细节（→各机制篇）；测试基线数字（→新29，本篇只留指针，落实 O1）；启动链步骤（→新01，本篇只给一张 init_vm 骨架图）。
- **前置**：无。
- **后置**：全部篇章。
- **事实底线**：main.c:93-194（主循环）；vm_server.rs 模块镜像；direct_map.rs/mmap.rs/main.c:582-586（布局）；C 文件清单 24 个 11,466 行；Rust 30,973 行。
- **知识点清单**：K-001 微内核内存语义权威（概）；K-002 三重权威（概）；K-003 单线程事件循环（约）；K-004 TLB 不变量一句话（约→新16）；K-005 主循环骨架（机→新14）；K-006 CALLMAP parity（事→新14）；K-007 edge 索引（工）；K-N10 地址空间布局总图（约）。
- **验收标准**：①导航表与新编号一一对应且主线唯一；②布局总图能回答"VM_OWN_HEAPBASE/VM_HEAP/MMAP_BASE/Direct Map/Handoff 页各在哪个窗口"；③全文无具体测试数字；④读者按本篇能说出三重权威各自的数据结构落点。

### 01-vm-init-main

- **一句话定位**：讲完 main() 到主循环之前的全部启动链——含内核交接消费、SEF 生命周期与 boot 进程地址空间装配。
- **讲什么**：K-010～K-021、K-N05、K-012/K-017/K-015 主点。
- **不讲什么**：主循环分发细节（→新14）；`init_proc` 字段语义本体（→新02，本篇只留调用点）；`pt_init` 结构（→新07）；`mem_add_total_pages` 定义（→新05，本篇留调用点）；boot 协议生产侧（→01-stage-kernel/09）。
- **前置**：00；01-stage-kernel/09-vm-boot-protocol.md（跨 stage 前置，唯一允许的外部前置）。
- **后置**：02/03/04/05/06/07/09/10/14/26 引用本篇的调用点。
- **事实底线**：main.c:79-107（is_first_time/main）、:262-286（init_proc）、:331-417（exec_bootproc）、:428-587（init_vm）、:219-239（sef_local_startup）；utility.c:get_mem_chunks；main.rs:17-30（read_boot_params）；vm_server.rs（rs_handshake/init）；os/libs/minix-sef（对照面）。
- **知识点清单**：K-010 鸡生蛋（概）；K-011 fresh 门控（机）；K-012 init_vm 十三步（机）；K-013 kinfo 消费（接）；K-014 VmBootHandoff（机）；K-015 SEF 六回调（机）；K-016 握手折叠偏差（演）；K-017 exec_bootproc 五步（机）；K-018 init_proc 调用点（机→新02 主点）；K-019 授权链（接）；K-020 libc 边界（约）；K-021 SEF 申报窗口（机）；K-N05 SEF 双实现（演）。
- **验收标准**：①读者能复述 S1-S19 顺序并说明三处"为什么在此序"（memset 先于 acl_init、pt_init 先于 __minix_init、CALLMAP 后于全部子系统）；②SEF 双实现一节给出 V14-P2-1 的两个候选方案与裁决时点，不预设立场；③exec_bootproc 的 sys_exec/BOOTINHIBIT_CLEAR 因果链完整。

### 02-vmproc-struct

- **一句话定位**：VM 如何记住一个进程——PCB 字段、正交标志与生命周期状态机。
- **讲什么**：K-030～K-037（K-018 主点并入）。
- **不讲什么**：表级语义（→新03）；ACL 位图细节（→新04）；页表字段（→新07）。
- **前置**：00、01。
- **后置**：03/04/14/17/19/23。
- **事实底线**：vmproc.h 全；main.c:262-286；exit.c:25-107；vmproc/{vmproc,flags,vmproc_handle}.rs。
- **知识点清单**：K-030 字段五组（数）；K-031 VMF 正交位图（数）；K-032 生命周期状态机（机）；K-033 VMP_NR（约）；K-034 endpoint→槽第一动作（概）；K-035 init/clear_proc（机，主点）；K-036 fail-closed 审计（工→新14 主点，此处消费面）；K-037 ExitingProc/reap（机→新23 主点，此处类型面）。
- **验收标准**：①"为什么位图不是枚举"有正反论证；②状态机图覆盖 fork 子槽/exec 临时槽/退出中三特例；③typestate 表达（ActiveProc/ExitingProc）与 C 位检查的差异讲透。

### 03-vmproc-table

- **一句话定位**：VM 如何记住所有进程——表的集合语义：怎么找、怎么分、怎么数。
- **讲什么**：K-040、K-041、K-043、K-044、K-045（引用）。
- **不讲什么**：PCB 字段（→新02）；endpoint 编码常量（→新99，本篇讲校验机制并引用 99 的编码条目，落实 R6 分工）；swap 机制本体（→新10）。
- **前置**：00、01、02。
- **后置**：04/14/19/23/26。
- **事实底线**：glo.h:17-20；utility.c:vm_isokendpt、:186-219（swap_proc_slot）；endpoint.h；vmproc/table.rs。
- **知识点清单**：K-040 表集合语义（数）；K-041 vm_isokendpt（接，主点）；K-043 slot 分配（机）；K-044 VMP_EXECTMP 诚实定位（事）；K-045 swap_slots 引用（机→新10）。
- **验收标准**：①endpoint 代际校验的失败模式（旧代际消息被拒）有具体场景；②VMP_EXECTMP"保留但不用"的现状与 C exec 路径的差别如实；③K-046 的 None 即拒绝在闸门层（新04）讲、本篇讲 get_active 返回 None 的表语义——边界不重叠。

### 04-acl

- **一句话定位**：调用权限——位图门禁、RS 授予链与 fail-closed 演进。
- **讲什么**：K-050～K-056。
- **不讲什么**：dispatch 接线（→新14）；fork/exit 流程（→新19/23，本篇只讲 acl_fork/acl_clear 本体）。
- **前置**：00、01、02、03。
- **后置**：14/19/26。
- **事实底线**：acl.c 全 129 行；com.h:VM_RQ_BASE/769-770；bitmap.h；acl.rs（AclMask/AclState）；vm_server.rs 闸门（Fix #66 后形状）。
- **知识点清单**：K-050 攻击面（概）；K-051 位图语义（数）；K-052 DEFAULT/SYSTEM 分层（机）；K-053 五函数族（接，主点）；K-054 A-11 fail-closed（演，主点）；K-055 None 即拒绝（约，主点）；K-056 派生与回收时机（接）。
- **验收标准**：①C 的 NO_ACL allow-all（acl.c:44-53 注释 "for now"）与 Rust fail-closed 的语义偏移如实标注且三处一致（doc/design/代码注释）；②门禁代码块与 vm_server.rs 现状一致；③读者能画出"RS 启动服务→map_service→acl_set→后续调用过闸"全链。

### 05-physical-memory

- **一句话定位**：物理内存账本——从 boot memmap 到可分配页池，三后端分配器。
- **讲什么**：K-060～K-062、K-064、K-067、K-068、K-069（引用）、K-063、K-059（引用）、K-126。
- **不讲什么**：VM 自用页（→新06）；映射物理页引用计数（→新11）；页缓存回收（→新25）。
- **前置**：00、01。
- **后置**：06/07/08/11/12/19/28。
- **事实底线**：alloc.c 全 548 行；utility.c:get_mem_chunks；type.h:memory；vm.h:22-27（PAF）；phys_mem/ 全目录。
- **知识点清单**：K-060 来源链（概）；K-061 get_mem_chunks（机）；K-062 分配器本体（机，主点）；K-063 A-5 三后端（演）；K-064 buddy 三段式（机）；K-067 记账诊断（工）；K-068 mem_add_total_pages（机，定义主点）；K-069 parity 测试引用（测→新29）；K-059 两层分界引用（概→新11）。
- **验收标准**：①三后端差异表含"同一请求三答案"的修复史结论（只写现状：max_page_bound 单点）；②PAF_LOWER16MB/LOWER1MB 的消费链完整到 buddy 分支；③PAF_CONTIG 两侧零消费的事实如实。

### 06-page-allocator

- **一句话定位**：VM 自用页分配——双地址问题与 Direct Map 的结构消除。
- **讲什么**：K-070～K-078。
- **不讲什么**：物理分配器本体（→新05）；pt_t 结构（→新07）；堆消费流程（→新09）。
- **前置**：00、01、05。
- **后置**：07/08/09/10。
- **事实底线**：pagetable.c:_SYSTEM 族（vm_allocpage:333-394 等）；alloc.c:60-237（reservedqueue）；main.c:118-119（调用点）；alloc_page.rs；global.rs；direct_map.rs。
- **知识点清单**：K-070 双地址问题（概，主点）；K-071 vm_* 函数族（接）；K-072 备用页池 C 机制（机）；K-073 A-1 结构消除（演，主点）；K-074 spares 链删除现状（演，只写现状+todo 指针）；K-075 pagelock/addrok（接）；K-076 get_vm_self_pages（接）；K-077 vm_self_query（接）；K-078 alloc_pfn_reclaiming（机）。
- **验收标准**：①"C 为什么需要备用页池（pagetable.c:55-57 注释）→ Rust 为何不需要"因果完整；②`alloc_cycle` 主循环钩子的 C/Rust 对照表（C 有/Rust 无及原因）。

### 07-pagetable-struct

- **一句话定位**：页表的结构语义——层级、Direct Map 双视图、自映射与三架构 trait。
- **讲什么**：K-080～K-087、K-083 主点。
- **不讲什么**：pt 逐操作（→新08）；页分配（→新06）；TLB 不变量（→新16，本篇只讲 Direct Map 为何免除自刷的结构面）。
- **前置**：00、01、05、06。
- **后置**：08/09/16/21/28。
- **事实底线**：pt.h:11-25；arch/i386/pagetable.h、arch/earm/pagetable.h；i386/include/vm.h PTE 位布局；pagetable.c:1088-1349（pt_init 结构面）、:112、:1358-1489；os/arch/{direct_map,paging,dm_coverage}.rs；kernel/dm_coverage.rs。
- **知识点清单**：K-080 页表本质（概）；K-081 pt_t/ARCH_VM_*（数）；K-082 A-2 四级（演）；K-083 A-1 Direct Map（演，主点）；K-084 A-9 自映射（演）；K-085 A-6 宽度（演，主点）；K-086 A-10 trait（演）；K-087 pt_init 结构面（机）。
- **验收标准**：①C 2 级 pt_pt[1024] 与 Rust 4 级动态分配的结构对照图；②Direct Map 双向转换（phys↔virt）在 2×2 表（C 有/无 × Rust 有/无）中定位；③A-1 的 kernel 侧 establish_boot_dm 联动讲清（这是跨 stage 知识点的 VM 侧主点）。

### 08-pagetable-ops

- **一句话定位**：页表的操作面——生命周期、写映射（WMF）、跨表复制与内核协作。
- **讲什么**：K-090～K-096、K-098、K-099、K-101、K-088/K-097（C 对照主点留此）。
- **不讲什么**：页表结构（→新07）；TLB **不变量**陈述与逐路径表（→新16；本篇 §1.8 保留"C 自刷四处 vs 逐条 invlpg"的机制对照并引用新16，落实 R1/O3 分工）；fork/exit 服务编排（→新19/23）。
- **前置**：00、01、05、06、07。
- **后置**：16/17/19/21/22/23。
- **事实底线**：pagetable.c 全函数锚点（pt_ptalloc:494、pt_writemap:784、pt_new:990、pt_copy:1069、pt_bind:1358、pt_free:1427、pt_mapkernel:1442 等）；vm.h:55-62；os/arch/paging.rs；x86_64/paging.rs（walk_alloc/write_pte_dm）。
- **知识点清单**：K-090 六动词总览（概）；K-091 生命周期（机，主点）；K-092 pt_writemap+WMF（接，主点）；K-093 ptalloc 族（机）；K-094 跨表复制（机）；K-095 查询校验（机）；K-096 内核协作（机）；K-097 VM 自刷四处对照（机，C 对照主点）；K-098 VMINHIBIT 动态停等（演）；K-099 六路径消费地图（概）；K-101 Paging trait（数）。
- **验收标准**：①WMF 四旗标逐一给消费场景；②§1.8 结尾显式写"结构不变量的陈述与逐路径论证见新16"；③fork/exec/mmap/munmap/pagefault/LU 六路径各能指到本篇的具体函数。

### 09-slab-allocator

- **一句话定位**：VM 自己的堆——C slab 尺寸分类分配器的遗产与 Rust HeapArena/VmAllocator。
- **讲什么**：K-110～K-117。
- **不讲什么**：元数据搬迁（→新10）；页分配（→新06）；内存统计服务（→新27）。
- **前置**：00、01、06、07、08。
- **后置**：10/27。
- **事实底线**：slaballoc.c 全 528 行锚点（SLABSIZES:29、slaballoc:259、slabfree:406、slabstats:504）；proto.h:SLABALLOC；heap_arena.rs；global.rs；direct_map.rs:VM_HEAP_*。
- **知识点清单**：K-110 堆分界线（约，主点）；K-111 slab 机制（机，主点）；K-112 SLABALLOC/FREE 宏（接）；K-113 A-3 v2（演，主点）；K-114 MEMPROTECT（工）；K-115 vm_self_map 消费（接）；K-116 堆窗口（约）；K-117 slabstats 衔接（工）。
- **验收标准**：①"slab 有意省略"理由成立且对照 Redox/Linux 用户态堆形态；②free-list v2 的分配/回收/增长三语义与 K-116 窗口约束一致；③__minix_init 分界线两侧"什么能用什么不能用"列表化。

### 10-vm-relocation

- **一句话定位**：自举的终点——静态/临时分配如何切换到动态稳态；LU 的数据支撑面。
- **讲什么**：K-120～K-127。
- **不讲什么**：RS 服务流程（→新26）；SEF 生命周期叙事（→新01）；分配器本体（→新05/09）。
- **前置**：00、01、05、06、07、08、09。
- **后置**：26。
- **事实底线**：pagetable.c:1311-1345（搬迁段）、:328（pt_init_done）、:59-110（spare 池）；alloc.c:60-237；utility.c:186-219/:228/:283/:312；region.c:1535；vm_server.rs:relocate；phys_mem/mod.rs。
- **知识点清单**：K-120 为什么不能长期用（概）；K-121 C 搬迁（机，C 侧主点）；K-122 Rust 搬迁（机）；K-123 swap 族（机，主点）；K-124 transfer/map_dyn_data（机）；K-125 map_setparent（机）；K-126 relocate 审计（工）；K-127 LU 三分边界（边界，主点）。
- **验收标准**：①C 搬迁段的"先建后换"次序图；②Rust 侧与 C 的结构差（BumpBuf→HeapArena vs BSS→动态）对照表；③K-127 三分表（01 叙事/10 机制/26 服务）在三篇契约中一致出现。

### 11-phys-pagestate

- **一句话定位**：物理页的状态账——引用计数、反向引用与 PFN 索引模型。
- **讲什么**：K-130～K-136、K-137（引用）。
- **不讲什么**：CoW 分裂机制（→新18）；页缓存持有面（→新25）；区域挂接（→新13）。
- **前置**：00、05、10。
- **后置**：12/13/17/18/25。
- **事实底线**：pb.c 全 168 行；region.h:23-35；phys_region.h；region.c:60-72（physblock_get/set）、:168-250（map_sanitycheck）；region/page_state.rs；sanity.rs:54。
- **知识点清单**：K-130 两层/三层结构（数，主点）；K-131 pb 五函数（接）；K-132 PBF 与不变量（约）；K-133 mem_cow 位置（边界→新18）；K-134 PageFrames PFN 模型（演，主点）；K-135 三态建模（数）；K-136 verify_refcounts（测→新29 汇总）。
- **验收标准**：①"alloc.c 零引用 pb_new"的分界 grep 证据呈现；②refcount 的四个变化时机（fork 共享/CoW 分裂/缓存持有/释放）各给调用点；③C 堆对象链 vs Rust 全局槽的内存布局对照。

### 12-memtype

- **一句话定位**：内存类型系统——区域管"何时"、类型管"做什么"的策略分发。
- **讲什么**：K-140～K-148。
- **不讲什么**：框架调用流程（→新13/新17）；文件/缓存域细节（→新24/25）。
- **前置**：00、05、10、11。
- **后置**：13/17/18/21/24/25。
- **事实底线**：memtype.h 15 字段；mem_anon.c/mem_anon_contig.c/mem_directphys.c/mem_shared.c/mem_cache.c/mem_file.c 六文件；memtype.rs:10-1366。
- **知识点清单**：K-140 策略模式（概）；K-141 mem_type_t（数）；K-142 trait MemType（演，主点）；K-143 六类型语义（机，代表+差异表）；K-144 PagefaultResult（接，主点）；K-145 反向依赖判定（约）；K-146 ev_delete/remaps（机）；K-147 writable 恒 false（约，主点）；K-148 类型级回调（接）。
- **验收标准**：①15 回调 × 6 实例矩阵全覆盖（哪些覆写、哪些默认、默认语义是什么——对齐 checklist §5 重导出口径）；②PagefaultResult 各变体的产生者与消费者闭环；③K-145 的 C-parity 判定作为设计约束呈现（非历史叙事）。

### 13-region-ledger（合并篇）

- **一句话定位**：进程地址空间的区域账本——生命周期框架操作、有序查找与索引选型（AVL→BTreeMap）。
- **讲什么**：K-150～K-164 全部。
- **不讲什么**：缺页状态机（→新17，本篇只讲 map_pf 框架面）；fork 编排（→新19）；munmap 服务面（→新22）；查询（→新27）。
- **前置**：00、05、10、11、12。
- **后置**：14/17/18/19/20/21/22/23/25/27。
- **事实底线**：region.c 1555 行全锚点（map_page_region:463、map_pf:664、map_proc_copy:933、map_unmap_range:1222、split_region:1150 等）；region.h:37-78；phys_region.h；cavl_if.h/cavl_impl.h/regionavl_defs.h/unavl.h/regionavl.c；region_map.rs/vir_region.rs/region/mod.rs。
- **知识点清单**：K-150 三层分工（概）；K-151 vir_region 结构（数）；K-152 Rust 形状（数）；K-153 生命周期族（接）；K-154 map_pf 框架（机）；K-155 map_proc_copy（机）；K-156 拆除与拆分（机）；K-157 AVL 模板（机）；K-158 A-4 BTreeMap（演，主点）；K-159 find_overlap/零长（机）；K-160 region id/release_shared_remap（机）；K-161 vrallocflags（接）；K-162 writept 收口（机）；K-163 调试自检面（工）；K-164 惰性家族删除现状（演，只写现状）。
- **验收标准**：①22 个框架函数逐一有归属（生命周期/查找/填页/复制/拆分/工具六组）；②AVL 宏模板的实例化机制讲清（一个编译单元如何长出类型安全的树）与 BTreeMap 等价论证成对出现；③find_slot/SearchType 的区间语义（FIRST_FIT/LOWER 地址查找）有图。

### 14-ipc-dispatch（原 15 重编号）

- **一句话定位**：VM 的心脏——消息如何进来、按什么优先级路由、如何回复、失败如何处理。
- **讲什么**：K-170～K-177、K-179、K-N06。
- **不讲什么**：各 handler 语义（→新17～27）；wire 布局与断言纪律（→新15，本篇只讲控制流）；ACL 策略（→新04，本篇讲接线）。
- **前置**：00、01、02、03、04、13（区域面就绪即可理解 handler 存在）。
- **后置**：15/16/17～28 全部。
- **事实底线**：main.c:47-59（vm_calls/CALLNUMBER）、:112-192（主循环）、:522-580（CALLMAP）；com.h:627-780；vfsif.h:79-81；ipcconst.h:28/:34；dispatcher.rs/transport.rs/encode.rs；vm_server.rs:run/run_once/dispatch_on_msg。
- **知识点清单**：K-170 五优先级（机，主点）；K-171 CALLMAP+守护测试（接，主点）；K-172 SUSPEND（约，主点）；K-173 transid 机制面（接；编码规范→新15）；K-174 notify 过滤与伪造源（约）；K-175 ACL 接线（机）；K-176 回复编码（接）；K-177 transport（接；测试注入→新29）；K-179 信号臂（机）；K-N06 崩溃模型（约，主点）。
- **验收标准**：①五优先级的顺序不可换的理由逐条成立；②CALLMAP 26 项表逐项标注"详见新NN"；③fail-fast/fail-closed/panic=abort 三层各自给一个具体触发场景；④SUSPEND 的三个消费方（RS_INIT/页故障 VFS 续作/procctl）各给完整回路。

### 15-wire-clients（新建）

- **一句话定位**：跨进程消息的字节级契约——结构布局、解码纪律、调用方全景；读完不再怕 overlay 错位。
- **讲什么**：K-N01、K-173（编码规范半）、K-202（wire 例证）、K-224（引用）、K-N07（现状引用）。
- **不讲什么**：分发控制流（→新14）；各服务语义（→新17～27）；minix-sef 的 SEF 协议（→新01§3.6，此处只在客户端清单里点名）。
- **前置**：00、01、03、14、99。
- **后置**：17～27 全部服务篇。
- **事实底线**：minix/ipc.h 全部 VM 相关结构（mess_lc_vm_*、mess_lsys_vm_*、mess_vm_vfs_*、mess_9/mess_10）；com.h 调用号；minix-types/src/ipc/{message,vm,rprocpub}.rs；libc 封装 lib/libsys/{vm_exit,vm_procctl,vm_info}.c、libc/sys/{brk,mmap}.c；vfsif.h TRNS_*；todo V13-P2-6 错位表。
- **知识点清单**：K-N01 五个核心点：①C union 消息模型与 M1/M2/M10 载荷约定；②minix-types 专属 wire struct + size_of/offset_of 断言纪律（12 个 payload 仅 1 个有断言的现状）；③overlay 解码三错位案例与修复（getphys/getref/rusage）；④transid 编码（TRNS_GET_ID/DEL_ID 位操作）与 m_type 复用；⑤客户端全景表（谁→什么调用→什么结构：PM/VFS/RS/kernel/libc）。
- **验收标准**：①给一个"从 C ipc.h 结构到 Rust wire struct 到 decode 函数"的端到端推导示例（以 VmForkIn 为例）；②56 字节断言的作用（布局漂移即编译失败）有演示；③客户端全景表覆盖全部 26 个注册调用 + VM_PAGEFAULT + RS_INIT；④通篇无"m1p1@16"式不可读表述，错位案例用字段名+偏移+类型三元组表达。

### 16-tlb-discipline（新建）

- **一句话定位**：为什么 VM 改页表不需要广播 TLB 失效——一条结构不变量的完整陈述与逐路径论证。
- **讲什么**：K-N02、K-N03、K-098（对照）、K-185（正文从旧 17§3.7 迁入）、K-088/K-097（引用）。
- **不讲什么**：invlpg 指令与 MMU 机制本体（→新07/新08 的结构/操作面）；SMP 调度与 IPI 机制（→01-stage-kernel/16 + edge E-VMTLB，本篇只登记缺口指针）。
- **前置**：00、02、03、07、08、14、15。
- **后置**：17～23、26、28。
- **事实底线**：16-pagefault.md §3.7（现有正文迁入底稿）；pagetable.c:119/:255/:319/:430（C 自刷四处）；:799-815/:928-934（VMINHIBIT 包裹）；minix3/minix/kernel/proc.c:345-347（MF_FLUSH_TLB）；os/kernel/src/syscall.rs:2177-2202（死 VMCTL 面）；x86_64/paging.rs:write_pte_dm。
- **知识点清单**：K-N02 五个部分：①不变量陈述（"VM 只修改当前未在运行的进程的页表"）；②逐路径停等表（fault 目标带 RTS_PAGEFAULT/munmap-brk-mmap 调用方阻塞在 IPC/fork 父阻塞在 PM/exit 与 RS pin 目标未运行——每行给机制锚点）；③违反后果（静默内存腐坏的具体场景）与新路径准入门槛；④C 的两种替代策略对照（VMINHIBIT 动态停等 = 08 已登记的执行模型差异；本篇讲两者互为同一保证的两种实现）；⑤SMP 半边缺口 = E-VMTLB 指针 + K-N03 死内核面注释锚。
- **验收标准**：①停等表七行（fault/munmap/brk/mmap/fork/exit/RS）每行有"目标为何必不在运行"的机制级理由；②读者能回答"如果要实现对运行中进程 unmap（如未来 LU share_mappings），必须先补什么"；③E-VMTLB 与死 VMCTL 面的指针措辞与 edge_todo.md 一致。

### 17-pagefault（原 16，迁出 §3.7）

- **一句话定位**：缺页仲裁——被动缺页与主动内存保障双路径，进程如何被停住与解阻。
- **讲什么**：K-180～K-184、K-186～K-188。
- **不讲什么**：TLB 不变量（→新16，引用）；CoW 分裂机制本体（→新18）；VFS FDIO 队列（→新24）。
- **前置**：00、02、11、12、13、14、15、16。
- **后置**：18/19/24。
- **事实底线**：pagefaults.c 全 418 行锚点（pf_state:33、handle_pagefault:76、pf_cont:161、handle_memory_*:170-417、do_memory:294）；region.c:664-774；main.c:153-164/:731-750；cow_exec_pf.rs；minix-types ipc/vm.rs:VM_PAGEFAULT。
- **知识点清单**：K-180 三方协议（概）；K-181 被动路径（机，主点）；K-182 主动路径（机，主点）；K-183 map_pf 消费（机）；K-184 SUSPEND/计数（约）；K-186 Rust 消费入口（机）；K-187 错误三分（机）；K-188 现状故障链（事）。
- **验收标准**：①§1.5 真序表 P1-P7 七步在正文有逐一对映；②被动/主动两路径的汇合点（map_pf）与分叉点（SIGKMEM 信号）各有一张时序图；③"内核停住→VM 仲裁→内核恢复"三方消息图完整。

### 18-cow-mechanism（原 17）

- **一句话定位**：写时复制——共享如何建立、写如何被拦截、首次写如何分裂。
- **讲什么**：K-190～K-197。
- **不讲什么**：缺页状态机（→新17）；fork 编排（→新19，本篇讲机制、19 讲流程）；file-backed VFS 交互（→新24）。
- **前置**：00、11、12、13、16、17。
- **后置**：19。
- **事实底线**：pb.c:32-168；mem_anon.c:33-113；region.c:130-134/:257-295/:820-849/:906；mem_file.c:59-82/:173-177；cow_exec_pf.rs；fork.rs:92-140；vmproc_handle.rs:488-542。
- **知识点清单**：K-190 三要素（概，主点）；K-191 mem_cow（机）；K-192 预写保护（机）；K-193 快路 writable 门（约，主点）；K-194 换型 anon（约）；K-195 Linux/Redox 对照（演）；K-196 cow_block 边界（边界→新24）；K-197 区域层支撑（机）。
- **验收标准**：①"快路必须以 is_page_writable 为门"以设计约束呈现（对 writable 恒 false 类型私有页也必须拷贝换型）并给反例场景；②refcount 生命周期图覆盖共享建立→写故障→分裂→释放；③Linux do_wp_page 对照讲清"reuse 仅限 PageAnon 独占"的同构。

### 19-vm-fork（原 18）

- **一句话定位**：fork 的 VM 半——把"复制地址空间"组织成一次可回滚的服务调用；fork 次主线的收束点。
- **讲什么**：K-200～K-203、K-201 主点。
- **不讲什么**：CoW 机制本体（→新18）；PM 侧 fork（→04-stage-pm）；内核 sys_fork 实现（→01-stage-kernel/17，本篇讲 gateway 出参语义）。
- **前置**：00、02、03、04、06、07、08、11、12、13、14、15、16、17、18。
- **后置**：23。
- **事实底线**：fork.c:32-115；region.c:933-999；acl.c:110-116；com.h:633-635；kernel/system/do_fork.c（内核侧对照）；fork.rs 全；kernel_gateway.rs:sys_fork；E-FORKMSG 进度（edge_todo）。
- **知识点清单**：K-200 十步编排（机，主点）；K-201 次主线路径图（概，主点——回指 03/04/06/07/08/11/12/13/17/18 十篇的分工）；K-202 sys_fork 出参（接）；K-203 回滚路径（机）。
- **验收标准**：①§1.5 真序表 F1-F10 逐一对映且每步的失败出口（EINVAL/ENOMEM/panic）标注；②次主线路径图让读者不读全文也能定位"fork 触及 X 语义→去新NN"；③eager-CoW 相（msgaddr 出参激活后）的行为差异讲清。

### 20-vm-brk（原 19）

- **一句话定位**：brk/sbrk 的服务侧——堆区域的三态编排（扩展/收缩/不变）。
- **讲什么**：K-204～K-206。
- **不讲什么**：libc malloc/sbrk 上层（→14-stage-runtime）；mmap 地址分配（→新21）；栈增长（WONTFIX 事实，K-206 如实登记）。
- **前置**：00、02、05、13、14、15。
- **后置**：21。
- **事实底线**：break.c:44-69；region.c:1002-1060；mem_anon.c:115-130；lib/libc/sys/brk.c:24-34；com.h:636；ipc.h:918-926；brk.rs。
- **知识点清单**：K-204 三态编排（机，主点）；K-205 libc 分层（接）；K-206 STACK_CHANGED 事实（事）。
- **验收标准**：①grow（惰性）/shrink（真释放，经 free_region_pages→pt.unmap）的物理页流向图；②"最小完整服务"的示范价值成立（本篇是服务带最短路径样本）。

### 21-vm-mmap（原 20）

- **一句话定位**：mmap 家族的建立路径——地址空间分配、匿名/文件分流与特权物理映射。
- **讲什么**：K-207～K-209、K-212（map_phys 建立半）。
- **不讲什么**：munmap（→新22）；页缓存（→新25）；VFS 队列基础设施（→新24）；GETPHYS 查询（→新27）。
- **前置**：00、02、05、07、11、12、13、14、15、20。
- **后置**：22/24。
- **事实底线**：mmap.c:36-307/:310-363/:366-435；region.c:302-510；mem_file.c:191-246；mem_shared.c:167-205；mman.h:62-124；ipc.h 四结构；mmap.rs/map_phys.rs；libc/sys/mmap.c。
- **知识点清单**：K-207 三路解析+分流（机，主点）；K-208 辅助路径（机）；K-209 64 位窗口（约）。
- **验收标准**：①mmap_region 三路（提示给地址/提示不给/固定地址）决策图；②同步匿名与异步文件的分界（FILEMAP_ENABLED/mmap_file_cont）完整；③map_perm_check 的 execpriv 分级表。

### 22-vm-munmap（原 21）

- **一句话定位**：映射拆除——四情形范围拆除、三情形区域拆分与物理页归还。
- **讲什么**：K-210、K-211、K-212（拆除半）。
- **不讲什么**：exit 全区域释放（→新23）；fdref 表本体（→新24）；页缓存交互（→新25）。
- **前置**：00、02、07、11、12、13、14、15、21。
- **后置**：23/24。
- **事实底线**：mmap.c:488-573；region.c:527-641/:1065-1294；mem_directphys.c 全 79 行；pb.c:96-134；fdref.c:116-154；main.c:538-540/:564；munmap.rs/map_phys.rs。
- **知识点清单**：K-210 统一入口+四情形（机，主点）；K-211 引用解除漏斗（机）；K-212 MAP_PHYS 对称性（机）。
- **验收标准**：①unmap_range 四情形（整区/头/中/尾）各给 split 行为图；②与 C 的逐页 pt.unmap 闭环（munmap.rs:216/238/261/276→region/mod.rs:31-36）呈现为现状；③SHM_UNMAP 与 remaps 递减（release_shared_remap）闭环。

### 23-vm-exit（原 22）

- **一句话定位**：进程生命周期的终点——两阶段退出协议、资源清算与 PROCCTL 进程控制（exec 的 VM 半）。
- **讲什么**：K-213～K-217。
- **不讲什么**：PM 侧 wait/signal（→04-stage-pm）；查询（→新27）；fdref 细节（→新24）。
- **前置**：00、02、03、07、11、12、13、14、15。
- **后置**：26/27。
- **事实底线**：exit.c 全 156 行；region.c:527-612；pagetable.c:990-1437（pt_new/bind/free 调用面）；acl.c:121-130；com.h:630-760；libsys/vm_exit.c、vm_procctl.c；vfs/comm.c:198-217；pm/forkexit.c:332/:455（调用方对照）；exit.rs；E4 edge。
- **知识点清单**：K-213 两阶段协议（机，主点）；K-214 清算顺序（机）；K-215 PROCCTL 双语义（机，主点）；K-216 transid 续作（接）；K-217 资源闭环+E4 指针（约）。
- **验收标准**：①WILLEXIT/EXIT 两步各自的"谁在何时调用、不调用会怎样"成立；②VMPPARAM_CLEAR 重建页表五步（pt_free→pt_new→pt_bind→VMPPARAM_CLEAR→恢复）与 exec 的关系讲明；③资源清算顺序图（区域→物理页→页表→ACL→fdref→rusage）与 C 函数序一致。

### 24-vfs-interaction（原 23）

- **一句话定位**：VM↔VFS 的异步对话——文件后备映射的三条链路与串行请求队列。
- **讲什么**：K-220～K-224、K-N07（VFS 半现状）。
- **不讲什么**：页缓存内部结构（→新25）；mmap 建立全貌（→新21）；SUSPEND 机制（→新14）。
- **前置**：00、11、12、13、14、15、17、21。
- **后置**：25。
- **事实底线**：vfs.c 全 143 行；fdref.c 全 177 行；mem_file.c 全 287 行；mmap.c:84-195；pagefaults.c:161-417（回调面）；vfs/misc.c:do_vm_call:383-473；libminixfs/cache.c PEEK/ONE_SHOT；com.h:694-714；vfs_queue.rs/fdref.rs；E-VFSWIRE 进度（d136491ee）。
- **知识点清单**：K-220 三链路+队列（接，主点）；K-221 fdref（数）；K-222 分流与 clearend 引用（机→新25 主点）；K-223 mmap_file_cont（机）；K-224 VFS_VMCALL wire 现状（接）；K-N07 排水步/重试现状（演）。
- **验收标准**：①FDLOOKUP/FDIO/FDCLOSE 三条完整时序图（含 SUSPEND 与续作）；②串行激活（ID_MAX 上限内单请求）的理由与后果；③E-VFSWIRE 落地前后行为差（永不发送→发送+重试）如实。

### 25-page-cache（原 24）

- **一句话定位**：磁盘块页缓存——目录、LRU、淘汰与四个缓存 IPC。
- **讲什么**：K-222（主点）、K-225～K-228。
- **不讲什么**：VFS 队列（→新24）；分配器（→新05/06）。
- **前置**：00、11、12、14、15、24。
- **后置**：27。
- **事实底线**：cache.c 全 332 行；cache.h；mem_cache.c 全 324 行；mem_file.c:104-138/:210-240；alloc.c:260-262；libminixfs/cache.c:565/:704；page_cache.rs/ipc/cache_handlers.rs；E-VFSWIRE（ONCE 偏差依赖）。
- **知识点清单**：K-225 双哈希+LRU（数，主点）；K-226 四 IPC+回滚（接）；K-227 VMSF_ONCE（约）；K-228 cache_freepages C 对照（机）；K-222 clearend 尾页清零（机，主点）。
- **验收标准**：①主键/辅索引双查询的语义差（dev,offset 精确 vs dev,ino 遍历）；②淘汰条件（refcount 归 0 才真释放）与 IN_CACHE 保护闭环；③尾页 clearend 的"陈旧字节不可见"因果链。

### 26-rs-services（原 25，增节）

- **一句话定位**：RS 驱动的服务面——权限下发、Live Update 准备/更新/内存控制与通电现状。
- **讲什么**：K-229、K-230、K-232、K-123/K-124（引用）、K-N07（主点）、K-019（引用）。
- **不讲什么**：SEF 生命周期叙事（→新01）；swap/transfer 机制本体（→新10）；旧实例退出（→新23）。
- **前置**：00、01、02、03、04、10、13、14、15、23。
- **后置**：27。
- **事实底线**：rs.c 全 391 行；main.c:241-260/:592-672/:755-768；utility.c:477-492；minix/rs.h:165-199；minix-types rprocpub.rs:89；rs.rs/vm_server.rs（RprocTab/rs_handshake）；edge E-RSWIRE 进度（403e47022）。
- **知识点清单**：K-229 四 IPC+握手（接，主点）；K-230 A-8 缺口契约（演，主点）；K-N07 通电现状（演，主点——rproctab safecopy 17×420B 解码、call_mask u64、RS_SET_PRIV 真掩码、五 wire struct 落地；rs_handshake decode 失败 fail-closed 与 C panic 的语义差登记）；K-232 multi_lu 白名单（机）。
- **验收标准**：①A-8 缺口表（哪些步骤未实现、可观察行为差、解锁条件）更新到 E-RSWIRE 落地后现状；②map_service 授权链与新01§3.2 互引不重复；③MAKE_VM 恒拒的偏差行保持三处一致（doc/design/rs.rs 注释）。

### 27-vm-queries（原 26）

- **一句话定位**：VM 作为内存权威的四个只读窗口。
- **讲什么**：K-233～K-237。
- **不讲什么**：区域生命周期（→新13）；rusage 清零时机（→新23，引用）；页缓存统计口径（→新25，引用）。
- **前置**：00、02、11、13、14、15、25。
- **后置**：无（终端服务篇）。
- **事实底线**：utility.c:100-184/:426-472；mmap.c:438-483；region.c:1323-1505；cache.c:328-331；minix/include/minix/vm.h:40-66；libsys/vm_info.c；query.rs。
- **知识点清单**：K-233 四查询（接，主点）；K-234 统计口径（机）；K-235 诚实定位（约）；K-236 分页游标（接）；K-237 PM-only（约）。
- **验收标准**：①usage 统计的 kernel/VM 自身特判逻辑（get_usage_info_kernel/vm）与 TOTAL_PAGES 全局口径一致；②GETPHYS 返回 region id 而非物理地址的语义用一问一答讲死；③REGION 分页的游标续传协议完整。

### 28-dma-contig（新建，支线）

- **一句话定位**：minix-rs 服务扩展——用户态驱动如何通过 VM 契约合法获得 DMA 连续内存与总线地址。
- **讲什么**：K-N04。
- **不讲什么**：驱动侧消费（→16-stage-drivers 后续接线，本篇登记 edge 指针）；内核 alloc_contig 遗产（→01-stage-kernel/18 对照）。
- **前置**：00、05、06、07、13、15。
- **后置**：无（支线终端）。
- **事实底线**：os/servers/vm/src/dma.rs 全；minix-types DmaMemory/DmaRegion 契约；edge2.md L9（契约定稿）；对照 Redox common/src/dma.rs、rcore virtio-drivers Hal；C 对照 minix3/minix/drivers/lib/libvirtio/virtio.c:319（驱动直要内核内存的旧世界）。
- **知识点清单**：K-N04 四部分：①为什么用户态化后驱动不能再直要内存（物理分配器在 VM 手里）；②DmaMemory 契约（minix-types 半）与 VM 实现半的分工；③"簿记+纯算术"实现（Direct Map 恒定翻译免建映射；窗外/外区返回 None 的归属簿记）；④alloc_contiguous 漏斗与回收重试自带。
- **验收标准**：①C 世界 vs minix-rs 世界的 DMA 内存获取路径对照图；②"翻译只对本生产者发出的区域成立"的安全论据完整；③C 无对应调用的事实（VM_ADDDMA 等 ENOSYS，todo §18.1）与本扩展的关系讲清（契约是新接口，不是 C 移植）。

### 29-test-infra（新建，支线）

- **一句话定位**：无真实硬件如何驱动全部内存语义——接缝注入、测试矩阵与可观测性。
- **讲什么**：K-N08、K-N09、K-069/K-136/K-163/K-177（汇总引用）。
- **不讲什么**：各机制的测试用例明细（→各机制篇§5，本篇只讲基建与汇总口径）。
- **前置**：00、14、15。
- **后置**：无（支线终端）。
- **事实底线**：kernel_gateway.rs（TrapKernelGateway/MockGateway）；pagetable/sim.rs（SimPaging）；ipc/transport.rs（TestIpcTransport/TestTransportHandle）；allocator_tests.rs（三后端 parity）；lib.rs（audit_log! 双宏）；vm_server.rs（VmContext 计数器 pagefault_errors/dropped_messages/alloc_failures）；sanity.rs:54；cargo 三矩阵基线（todo §0.2：490/507/490→Fix 后 503/521/503）；os/Cargo.toml feature 段。
- **知识点清单**：K-N08 四部分：①两个注入缝（KernelGateway 管内核能力、SimPaging 管页表面）+ transport 注入（V10 端到端）；②三后端 feature 矩阵与 parity 对账方法；③测试如何驱动"缺页→CoW→PTE 写"全链（SimPaging 断言 PTE 位与 memtype 终态——V13-P1-1 的测试教训）；④诚实标注纪律（host 桩 no-op 时 target-only 验证的 UNVERIFIED 标注法）。K-N09 三部分：①audit 双宏与计数器分工（release 可观测由 VmContext 承载）；②verify_refcounts/sanity 面对照；③基线数字的唯一权威位置（本篇 §汇总 + todo，杜绝 00 式数字漂移）。
- **验收标准**：①一张"机制 → 注入缝 → 代表测试"映射表覆盖 17～28 全部服务篇；②三矩阵命令原文与当前基线可执行复现；③全篇不含未带命令的基线断言。

### 99-global-concepts

- **一句话定位**：跨机制词汇表——常量、旗标、全局状态的权威定义与消费点索引。
- **讲什么**：K-042（endpoint 代际编码，R6 分工的常量半）、K-240～K-245。
- **不讲什么**：任何机制流程；调用号→handler 的映射（→新14 表）。
- **前置**：00。
- **后置**：全部（随时可查）。
- **事实底线**：glo.h/vm.h；com.h:627-780；ipc.h；minix-types。
- **知识点清单**：K-042 endpoint 代际编码（数，主点）；K-240 方法论（概）；K-241 调用号族（接，主点）；K-242 PAF/WMF（接，主点）；K-243 VR/VMSF/VMC（接，主点）；K-244 三档显式化（演）；K-245 位置判据（工）。
- **验收标准**：①每个常量条目含"值/C 锚点/语义半径/消费文档"四列；②与 R6 的 endpoint 分工（编码在此、校验在新03）写明。

---

## 6. 变更表

### 6.1 统一变更表

| 操作 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 |
|------|------|--------|--------|------|-----------|------|
| A01 | 改写重组 | 00-vm-overview | 新00 | 导航二元状态（编号序 vs 推荐路线）收敛为单一主线；新增布局总图（K-N10）；测试数字指针化（O1） | K-001~007、K-N10 | 全部保留，仅重组 |
| A02 | 保留+增节 | 01-vm-init-main | 新01 | 正文补 K-N05 SEF 双实现与 K-N07 消费边界；其余结构已优 | K-010~021、K-N05 | 不变 |
| A03 | 保留 | 02 → 新02 | 同号 | 结构无需动 | K-030~037 | 不变 |
| A04 | 保留 | 03 → 新03 | 同号 | 同上 | K-040~046 | 不变 |
| A05 | 保留 | 04 → 新04 | 同号 | 同上 | K-050~056 | 不变 |
| A06 | 保留 | 05 → 新05 | 同号 | 同上 | K-059~069 | 不变 |
| A07 | 保留 | 06 → 新06 | 同号 | 同上 | K-070~078 | 不变 |
| A08 | 保留 | 07 → 新07 | 同号 | 同上 | K-080~089 | 不变 |
| A09 | 保留+边界调整 | 08 → 新08 | 同号 | §1.8 保留 C 对照、不变量主点移交新16（O3/R1） | K-090~099、K-101 | K-185 相关内容→新16 |
| A10 | 保留 | 09 → 新09 | 同号 | 结构无需动 | K-110~117 | 不变 |
| A11 | 保留 | 10 → 新10 | 同号 | 同上 | K-120~127 | 不变 |
| A12 | 保留 | 11 → 新11 | 同号 | 同上 | K-130~137 | 不变 |
| A13 | 保留 | 12 → 新12 | 同号 | 同上 | K-140~148 | 不变 |
| A14 | **合并** | 13 + 14 | 新13 region-ledger | 14 单篇低于一个语义单元的自然重量；"账本+索引"是一个概念（§6.3） | K-150~164 | 见 §6.3 节级映射 |
| A15 | **重编号** | 15-ipc-dispatch | 新14 | 插入 15/16 两新篇后顺序位移；同时拆出 wire 规范性内容到新15 | K-170~177、K-179 | wire 编码规范→新15；其余不变 |
| A16 | **新建** | — | 新15 wire-clients | 覆盖缺口 G2：线格式无系统讲处 | K-N01 等 | §7 |
| A17 | **新建** | — | 新16 tlb-discipline | 覆盖缺口 G1/G8：不变量分散 + 前向引用 | K-N02/K-N03、K-185 | §7 |
| A18 | **重编号** | 16-pagefault | 新17 | 顺延；§3.7 迁出至新16 | K-180~188 | §3.7→新16 |
| A19 | **重编号** | 17-cow-mechanism | 新18 | 顺延 | K-190~197 | 不变 |
| A20 | **重编号** | 18-vm-fork | 新19 | 顺延 | K-200~203 | 不变 |
| A21 | **重编号** | 19-vm-brk | 新20 | 顺延 | K-204~206 | 不变 |
| A22 | **重编号** | 20-vm-mmap | 新21 | 顺延 | K-207~209 | 不变 |
| A23 | **重编号** | 21-vm-munmap | 新22 | 顺延 | K-210~212 | 不变 |
| A24 | **重编号** | 22-vm-exit | 新23 | 顺延 | K-213~217 | 不变 |
| A25 | **重编号** | 23-vfs-interaction | 新24 | 顺延；增通电现状节 | K-220~224 | +K-N07 VFS 半 |
| A26 | **重编号** | 24-page-cache | 新25 | 顺延 | K-225~228 | 不变 |
| A27 | **重编号** | 25-rs-services | 新26 | 顺延；增通电现状节（K-N07） | K-229/K-230/K-232、K-N07 | +K-N07 |
| A28 | **重编号** | 26-vm-queries | 新27 | 顺延 | K-233~237 | 不变 |
| A29 | **新建** | — | 新28 dma-contig | 覆盖缺口 G4：dma.rs 零文档 | K-N04 | §7 |
| A30 | **新建** | — | 新29 test-infra | 覆盖缺口 G3：测试基建/可观测性无系统讲处 | K-N08/K-N09 | §7 |
| A31 | 保留 | 99-global-concepts | 新99 | 编号语义（词汇表后缀位）保持，跨 stage 引用零破坏 | K-240~245 | 不变 |
| A32 | **归档** | 14-region-lookup（旧文件） | `archive/`（B 相执行） | 内容并入新13 后旧篇退出正式目录；归档不删 | K-157~159 | 新13§5 |
| A33 | 计划同步 | plan.md | plan.md | §2 编号表被本蓝图取代：B 相开工时在 plan.md 头部加"编号体系已由 doc_rerank 共识蓝图接管"横幅（不删原文） | — | — |
| A34 | 计划同步 | checklist.md | checklist.md | 全部旧编号路径按 §8.2 映射批量替换 | — | — |

### 6.2 重排说明（重编号带：旧 15～26 → 新 14～17 位移 + 18～27 顺延）

重编号仅发生在旧 13/14 合并点之后：旧 01～12 与 99 全部保持编号（这正是选择合并点与插入点都在后段的原因——**13 篇零断链**）。受影响的是旧 14（并入新13）与旧 15～26（顺延为 14～17 与 18～27）。逐篇内容无重排（各篇内部结构经八轮 review 已稳定，B 相按契约做增量改写而非重写）。

### 6.3 合并说明（旧 13 + 旧 14 → 新 13）

**为什么合并**：旧 14（528 行）的独立语义重量不足——它的全部内容服务于"区域怎么被找到"，与 13 的"区域怎么被操作"是同一个语义单元（区域账本）的两面。拆开的代价是读者在概念中途被迫换篇（13 的 find_slot 引用 14，14 的 RegionMap 又是 13 的字段）。合并后约 1100～1200 行，在篇幅软限制内（单概念复杂者可更长）。

**存量知识点去向（双方向规则之"存量方向"逐条）**：

| 旧位置（14-region-lookup 小节） | 旧内容 | 新位置（新 13） | 迁移类型 |
|-------------------------------|--------|----------------|---------|
| §1 概念（AVL 是什么、为何需要有序索引） | 有序索引的动机 | 新13§5.1 | 改写并入 |
| §2 C 源码（cavl_if.h 接口面） | 模板接口 | 新13§5.2 | 原样搬移 |
| §3 C 源码（cavl_impl.h 实现机制：init/balance/insert/search/remove/iter） | 1208 行宏模板机制 | 新13§5.3 | 原样搬移 |
| §4 regionavl_defs 实例化 + unavl 清理 | 宏实例化 | 新13§5.4 | 原样搬移 |
| §5 Rust 设计（BTreeMap 封装、A-4 等价论证） | 索引选型 | 新13§5.5 | 原样搬移 |
| §6 SearchType/find_slot 语义 | 查找语义 | 新13§3（并入生命周期族的查找小节，K-159 随迁） | 改写并入 |
| §5 测试要点 | region_map 测试 | 新13§测试节 | 原样搬移 |
| 旧 13 全部小节 | 区域框架 | 新13§1～§4、§6 | 结构保持（原 13 已是合并篇主体） |

### 6.4 新建说明（四篇的原料来源）

| 新篇 | 原料来源（旧文档 / C / 非 C 制品） |
|------|----------------------------------|
| 新15 wire-clients | 从旧 15 §解码/transid/回复编码三小节取规范性内容（A15 拆出）；新增料：ipc.h 结构族、minix-types 断言纪律、V13-P2-6 错位案例、libc 封装面（各服务篇头部现存的逐结构清单收敛于此，O6） |
| 新16 tlb-discipline | 从旧 16 §3.7 整节迁入（A18）；新增料：E-VMTLB 登记、K-N03 死内核面、08 §1.8 的互引分工 |
| 新28 dma-contig | 全新增料：dma.rs 模块（331 行）、minix-types DmaMemory 契约、edge2.md L9、Redox/rcore 对照 |
| 新29 test-infra | 从 00 §4.1（接缝与注入）与各篇 §5 的基建性内容汇聚；新增料：三矩阵口径、audit 双宏、VmContext 计数器、V13-P1-1 测试教训 |

### 6.5 归档说明

仅旧 14-region-lookup.md 一个文件退出正式目录（B 相 `git mv` 至 `archive/doc-rerank/`，保留原文）。draft/ 与 archive/ 现有内容不动。

---

## 7. 缺漏新篇落实（非 C 主题清单逐项定案）

| # | 主题 | 为什么重要 | 原料在哪里 | 归哪一篇 | 验收标准 |
|---|------|-----------|-----------|---------|---------|
| 1 | 跨模块接口与线格式 | 全部服务依赖 wire；错位即跨进程通信 bug（V13-P2-6 实证） | ipc.h、minix-types、libc 封装、todo 错位表 | **新15** | §5 契约验收四条 |
| 2 | 并发与同步（TLB 半） | 无广播失效的正确性前提；SMP 化前置 | 旧16§3.7、pagetable.c、kernel/proc.c | **新16** | 停等表七行完整 |
| 3 | 测试基建 | 三矩阵 500+ 测试的基建无系统讲处 | 各接缝模块、allocator_tests | **新29** | 机制→注入缝→测试映射表 |
| 4 | 镜像与内存布局 | 读者第一个"地址空间怎么切"问题 | main.c:582-586、direct_map.rs、mmap.rs | **新00** 布局节 | 一图四窗口 |
| 5 | 错误路径系统化 | fail-fast/fail-closed/panic=abort 三层无汇总 | vm_server.rs、Cargo.toml | **新14§10** | 三层各一触发场景 |
| 6 | DMA 连续内存 | 已实现零文档的唯一 Rust 扩展服务 | dma.rs、minix-types、edge2 L9 | **新28** | C/Rust 对照图 |
| 7 | SEF 协议面双实现 | E1 通电前的必答裁决（V14-P2-1） | main.rs、vm_server.rs、minix-sef | **新01§3.6** | 两候选+裁决时点 |
| 8 | 链接与加载 | vm.lds WONTFIX 维持；libexec 装载语义已有归属 | plan §5.4、main.c:331-417 | **新01§4**（维持） | 现有验收 |
| 9 | 启动装配 | 消费侧 handoff 已在 01，边界需写明 | main.rs:17-30 | **新01§2.3**（维持） | 现有验收 |
| 10 | 构建与工具链 | feature 矩阵是测试基线的前提 | Cargo.toml、todo §0.2 | **新29§3** | 命令可复现 |
| 11 | 关闭与退出 | 已覆盖（新23）；E4 余件指针登记 | exit.c、edge_todo E4 | **新23**（维持） | 指针与 edge 一致 |
| 12 | 汇编入口与陷阱进入 | 明确不在本 stage | 01-stage-kernel/14、14-stage-runtime | **否决**（跨 stage 指针：新00 导航注明） | 00 导航行 |

> 上表不允许留空或"待定"：1～7 为新建/增节，8～11 为已有归属确认，12 为明确否决并给出指针位置。

---

## 8. 锚点迁移与断链成本

### 8.1 断链成本盘点（引用统计，证据见 §0.4）

| 引用类别 | 总量 | 受重编号影响 | 不受影响 |
|---------|------|-------------|---------|
| stage 内文档互引（28 篇之间） | 324 处（含自引） | 139 处（指向旧 14～26 的引用） | 185 处（指向 00～13、99） |
| 跨 stage .md 引用 | 约 240 处 | 约 97 处（指向旧 14～26） | 约 143 处 |
| Rust 代码注释引用（os/servers/vm + minix-types） | 约 24 处（VM 文档） | 11 处（旧 15/19/21/22/23/25/26 的当前名引用） | 13 处（00～13、99 名） |
| 已失效旧名引用（重建前就断） | 5 处 | 全部随本次一并修复 | — |

**热点文件**：stage 内 `15-ipc-dispatch`（28）→ 新14；跨 stage `01-vm-init-main`（29）、`07-pagetable-struct`（26）、`18-vm-fork`（17）。**成本结论**：总迁移面 ≈ 250 处字符串替换，全部可脚本化（§8.4），风险集中在跨 stage 的 97 处（对方 stage 的 review 锚点漂移——但那些引用本就随时间漂移，fix-guard 纪律本来就要求引用前复核）。

### 8.2 锚点迁移表（文件级 + 变化篇的节级）

**文件级（整篇迁移，内容不重排）**：

| 旧位置 | 新位置 | 迁移类型 | 断链风险 |
|--------|--------|---------|---------|
| 15-ipc-dispatch.md | 14-ipc-dispatch.md | 改写（wire 三小节拆出至新15） | 高（stage 内被引 28 + 跨 9 + 代码 1） |
| 16-pagefault.md | 17-pagefault.md | 改写（§3.7 迁出至新16） | 中（13+8） |
| 17-cow-mechanism.md | 18-cow-mechanism.md | 原样迁移 | 中（13+9） |
| 18-vm-fork.md | 19-vm-fork.md | 原样迁移 | 高（8+17） |
| 19-vm-brk.md | 20-vm-brk.md | 原样迁移 | 低（4+1+1 代码） |
| 20-vm-mmap.md | 21-vm-mmap.md | 原样迁移 | 中（9+10） |
| 21-vm-munmap.md | 22-vm-munmap.md | 原样迁移 | 低（8+3+1 代码） |
| 22-vm-exit.md | 23-vm-exit.md | 原样迁移 | 中（9+3+3 代码） |
| 23-vfs-interaction.md | 24-vfs-interaction.md | 改写（增通电现状节） | 中（11+6+2 代码） |
| 24-page-cache.md | 25-page-cache.md | 原样迁移 | 低（9+7） |
| 25-rs-services.md | 26-rs-services.md | 改写（增通电现状节） | 低（9+9+2 代码） |
| 26-vm-queries.md | 27-vm-queries.md | 原样迁移 | 低（12+12+1 代码） |
| 14-region-lookup.md | 并入 13-region-ledger.md；原文件归档 archive/ | 合并 | 低（6+3） |

**节级（三篇有内部结构变化）**：

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|--------|--------|--------|---------|------|
| 16-pagefault §3.7 | TLB 结构不变量+停等表 | 新16 主体 | 原样搬移+扩写（K-N02） | 新17 留一行引用 |
| 16-pagefault §3.6 第 12 行 | TLB 一致性状态行 | 新16 §状态表 | 搬移 | |
| 15-ipc-dispatch §（解码模式/transid 编码/回复编码三小节） | wire 规范性内容 | 新15 §2～§4 | 拆分改写 | 新14 留控制流视角+引用 |
| 15-ipc-dispatch §1.5 | 伪造源 audit 说明 | 新14 §6 | 原样搬移 | |
| 00-vm-overview §3.2 | 测试基线数字 | 删除（指针指向新29§汇总） | 删除+理由（O1：数字漂移） | |
| 00-vm-overview §2.3 | 推荐阅读路线（与编号序并存的第二顺序） | 重写为专题路线（索引非顺序） | 改写 | 消除二元状态 |
| 08-pagetable-ops §1.8 | C 自刷四处 vs 逐条 invlpg | 保留于新08§1.8 + 结尾加"不变量见新16" | 改写（边界） | R1/O3 |
| 06-page-allocator 头部 V12-P2-4 注记 | 修复史叙事 | 判定移 todo 引用，正文留现状语义 | 改写（O2 样板） | O2 规则全文推广 |

### 8.3 引用迁移表

**文件名映射（sed 批量替换表，顺序敏感——先长名后短名，避免子串误伤）**：

| 旧名（含旧引用） | 新名 | 验证方式 |
|-----------------|------|---------|
| `14-region-lookup.md` | `13-region-ledger.md`（内容并入者改指新13§5） | grep 零残留 |
| `15-ipc-dispatch.md` | `14-ipc-dispatch.md` | 同上 |
| `16-pagefault.md` | `17-pagefault.md` | 同上 |
| `17-cow-mechanism.md` | `18-cow-mechanism.md` | 同上 |
| `18-vm-fork.md` | `19-vm-fork.md` | 同上 |
| `19-vm-brk.md` | `20-vm-brk.md` | 同上 |
| `20-vm-mmap.md` | `21-vm-mmap.md` | 同上 |
| `21-vm-munmap.md` | `22-vm-munmap.md` | 同上 |
| `22-vm-exit.md` | `23-vm-exit.md` | 同上 |
| `23-vfs-interaction.md` | `24-vfs-interaction.md` | 同上 |
| `24-page-cache.md` | `25-page-cache.md` | 同上 |
| `25-rs-services.md` | `26-rs-services.md` | 同上 |
| `26-vm-queries.md` | `27-vm-queries.md` | 同上 |

**已失效旧名一并修复**（重建前已断，任何方案下都要修）：

| 失效引用 | 位置 | 修复目标 |
|---------|------|---------|
| `24-vm-ipc-dispatch.md` | os/servers/vm/src/ipc/transport.rs:33 | 新14-ipc-dispatch.md §3 |
| `20-vm-exit.md` | 跨 stage .md ×3（grep 定位） | 新23-vm-exit.md |
| `07-pagetable-ops.md` | 跨 stage .md ×1 | 新08-pagetable-ops.md |

**代码注释引用（11 处受影响）**：05-physical-memory×5、22-vm-exit→23 ×3、25-rs-services→26 ×2、23-vfs-interaction→24 ×2、09×2、06×2、26→27 ×1、21→22 ×1、19→20 ×1、15→14 ×1、13/12 ×1 各——批量 sed 后 `cargo check -p minix-vm` 与 `grep -rEo "[0-9]{2}-[a-z-]+\.md" os/ | 核对` 双验证（注释改动不触编译面，check 防手滑）。

**新建篇引入的新引用**：新15/16/28/29 落地后，新14/17 等篇头部互引、00 导航、99 参见同步更新——B 相每完成一篇即更新引用，不攒批。

### 8.4 断链成本摘要与批量修改方式

```bash
# B 相执行骨架（每步后 grep 验证零残留；顺序敏感）
STEP1="14-region-lookup:13-region-ledger 15-ipc-dispatch:14-ipc-dispatch \
16-pagefault:17-pagefault 17-cow-mechanism:18-cow-mechanism \
18-vm-fork:19-vm-fork 19-vm-brk:20-vm-brk 20-vm-mmap:21-vm-mmap \
21-vm-munmap:22-vm-munmap 22-vm-exit:23-vm-exit 23-vfs-interaction:24-vfs-interaction \
24-page-cache:25-page-cache 25-rs-services:26-rs-services 26-vm-queries:27-vm-queries"
# 注意：必须按"从大号到小号"或临时占位符法执行，避免 15→14 后又被 16→15 覆盖。
# 推荐两阶段：先全部加后缀 .tmpnew，再统一改回 .md。

# 范围（按序）：
# 1) notes/rewrite/fork-syscall-rewrite/02-stage-vm/[0-9]*.md + plan.md + todo.md + checklist.md
# 2) notes/rewrite/fork-syscall-rewrite/其它 stage（97 处跨 stage）
# 3) os/servers/vm/src 与 os/libs（11 处代码注释 + 失效旧名 5 处）
# 4) 验证：grep 零残留 + cargo test -p minix-vm --lib 三矩阵全绿 + clippy 对账
```

**成本摘要**：约 250 处字符串替换 + 2 个 `git mv`（旧 14 归档、新 13 改名）+ 4 个新文件 + 1 个重命名文件；无代码语义变更；测试面零影响（文档名不出现在断言中）。**这是把历史上 I-14 条目"重编号断链风险大于收益"的裁决反转的可量化依据**：当年无迁移表故停在原地；本次账已算清、批量法已给出，重建的断链成本从"不可知"变为"一个下午的脚本"。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**：新目录 31+1 篇的契约"前置"字段逐一核对——全部指向更小编号；唯一外部前置 = 新01 → 01-stage-kernel/09-vm-boot-protocol.md（跨 stage，允许且声明）。**通过**。依赖图无环（G4 一并核验：00→01→…→27→28/29，99 与全部连通但无回边；专题路线是索引不是依赖）。
2. **依赖关系图检查**：线性主链 + 99 通用汇聚点，无环。**通过**。
3. **覆盖率检查**：§2 池 207 条（存量 197 + 新增 10）逐条有"新去向"列；明确删除 1 项（00§3.2 测试数字，O1 有理由）。C 文件 24/24 有归属（plan §5.1 表经本次头部声明复核继续成立）；非 C 制品逐项落 §3.5 十项清单。**通过**。
4. **断链成本统计**：§8.1 总量 324+240+24、热点、批量法齐备。**通过**。

### 9.2 自检门 G1–G9

| 门 | 检查与结果 |
|----|-----------|
| G1 | C 真序逐条可核对——随机抽十条：①S1 is_first_time main.c:79-88 ✓（通读）；②S11 pt_init main.c:475 ✓；③S14a exec_bootproc 链 main.c:331-417 ✓（sys_exec :408、BOOTINHIBIT_CLEAR :415）；④L3 notify 过滤 main.c:125-129 ✓；⑤L5 transid main.c:143-148 ✓；⑥L7 伪造源 main.c:154-157 ✓；⑦F3 结构体复制 fork.c:56-64 ✓（一手）；⑧F7 sys_fork PFF_VMINHIBIT fork.c:89-94 ✓（一手）；⑨P2 SIGSEGV 分支 pagefaults.c:87-102 ✓（一手）；⑩P7 清页故障 :153-157 ✓（一手）。**通过**。 |
| G2 | 知识点池完整性：C 24 文件全部映射（§0.4+plan §5.1 复核）；非 C 制品 vm.lds/Makefile/宏头/com.h/ipc.h/libc 封装/测试基建逐项有归属或 WONTFIX 理由（§3.5）。**通过**。 |
| G3 | 前向引用为零：§9.1 检查 1。**通过**。 |
| G4 | 依赖图无环：线性主链+汇聚点。**通过**（无需拆解方案）。 |
| G5 | 覆盖率 100%：池 207 条每条有去向；新增 10 条全部带证据锚点；删除项 1 条单独列出（00§3.2 数字，理由 O1）。**通过**。 |
| G6 | 拆分/合并去向 + 新建来源抽查十处：①A14 合并七节映射（§6.3 全列）✓；②A15 wire 三小节→新15 ✓；③A18 §3.7→新16 ✓；④K-185 迁移 ✓；⑤K-097 双主点分工 ✓；⑥新15 原料四路 ✓；⑦新16 原料 ✓；⑧新28 全新增料带锚 ✓；⑨新29 汇聚来源 ✓；⑩A32 归档去向 ✓。**通过**。 |
| G7 | 契约七要素：31+1 篇契约逐篇含定位/讲什么/不讲什么/前置/后置/事实底线/知识点清单+验收标准（§5 全部按模板）。**通过**。 |
| G8 | 锚点迁移表覆盖所有变化文档的每一节（§8.2 文件级 13 行 + 节级 8 行）；引用迁移表覆盖文档与代码注释（§8.3 三段）。**通过**。 |
| G9 | 事实断言锚点核查：① §3.6 的 **16 样本全部一手核对**（sed/grep 原始输出）——正式文档锚点 13/13 命中，暴露 1 处数值事实错误（doc 99 的 `PAF_ALIGN16K` 0x20→实际 0x40，登记 E1 由 B 相修正），todo 参考材料 3 处行号漂移（E2–E4，本蓝图已重锚）；② 上一轮唯一挂起项已闭单：`rs_memctl_make_vm_instance`（rs.c:218）函数体一手核对，`num_vm_instances == 2` 才 EPERM（:231-234），首实例成功；③ 其余引用：371 符号/92.5%（todo §18.0）、三矩阵基线（todo §0.2）、transport.rs:33 失效引用、cross-stage 240、dma.rs 无 C 对应、A-1~A-12（plan §4）、V14-P2-1（todo §19）、主循环五优先级（main.c 一手）✓。**通过（新增项：1 个存量错误已登记 B 相必修清单）**。 |

### 9.3 结论与待用户裁决的问题

**结论：蓝图完成。** 新目录 31+1 篇；操作 = 合并 1、新建 4、重编号 12、改写重组 2、增节 4、保留 13、归档 1；知识池 207 条全部有去向且归属与篇内讲述顺序由 §4.6 总视图显式给定；断链成本 ≈ 250 处可脚本化替换。按用户约束 3 完成 16 样本可信度核查（§3.6）：抓到 1 处存量数值错误（E1，B 相必修）与 3 处记录漂移（蓝图已重锚），并实证了"存量文档与记录不能直接采信"——B 相必须按 §4.1 原则 8 对每条事实底线逐条代码核对后再动笔。B 相拿到本蓝图后无需再做取舍判断，按 §5 契约逐篇施工、按 §8.4 批量迁移引用即可。

**待用户裁决**（多 AI 蓝图汇总时的分歧点，预判如下）：

1. **重编号是否执行**（本蓝图最大胆的决策）：插入新15/16 必然使旧 16～26 顺延。备选 = 新篇挂尾（27～30），代价是 17～27 服务篇对新篇的前向引用（违反硬标准 1）。本蓝图立场：执行重编号，成本已量化（§8）。若汇总时多数蓝图选挂尾，本蓝图的契约内容不变，仅新目录编号表调整。
2. **旧 13+14 合并**：若汇总倾向保持拆分，新13 恢复为两篇（新13/新14），后续全部顺延 +1——契约可机械拆回，无内容损失。
3. **新28（DMA）/新29（测试基建）的体裁**：本蓝图定为支线正式篇；备选 = 并入新00 附录或独立 README。本蓝图立场：正式篇（两者各有 300+ 行独立语义，进 README 会复刻"文档坏在职责越界"的老路）。
4. **新16（TLB）是否并入新08**：若并入，08 达 ~1350 行且需 14/15 前置（反向破坏启动带顺序）。本蓝图立场：独立成篇。
