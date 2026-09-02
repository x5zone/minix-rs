# 13-stage-ipc 文档重组计划（plan.md）

> **状态**: 定稿（2026-08-16 首版 + 深度 review + minix3 源码回归 review，见 §7）
> **范围**: `notes/rewrite/fork-syscall-rewrite/13-stage-ipc/`
> **目标**: 以 **IPC server 启动顺序为主线**定义 IPC 全部文档；SysV IPC 调用旅程为次主线；最终覆盖 Minix3 IPC server（`servers/ipc/`，4 个 .c，1690 行）全部语义，支撑 IPC server 的彻底 Rust 重写
> **对照**: `01-stage-kernel/`（讲述结构参照）、`02-stage-vm/` + `03-stage-rs/` + `10-stage-mib/`（同流程先例）、`minix3/minix/servers/ipc/`（ground truth）、`os/servers/ipc-server/`（Rust 实现，当前为空壳 stub）

---

## 1. 背景与动机

### 1.1 现状问题

`13-stage-ipc/` 目录自 2026-08-14 补建以来仅有**占位 README**（已移入 `draft/`），没有任何正式文档。与它的服务语义不匹配：

1. **概念边界易混淆**——"IPC"在 Minix3 中分两层：**kernel IPC 机制**（`kernel/proc.c` 的 send/receive/notify 原语，见 `01-stage-kernel/12-ipc-core.md` + `notes/rewrite/ipc-sendrec.md`）与**用户态 IPC server**（本 stage：SysV 信号量 + 共享内存的对象管理服务）。README 已声明该边界，正式文档必须继承并展开。
2. **语义复杂度中等但依赖面广**——`servers/ipc/` 共 1690 行 C（4 个 .c），但跨服务依赖 6 个方向：PM（进程事件）、VM（内存映射/引用计数）、MIB（远程子树注册）、RS（加载/SEF 生命周期）、kernel（sys_datacopy）、libc（调用方）；且 `os/libs/minix-types/` **尚无 IPC 消息类型**、`os/libs/minix-sys/` 仍是 stub，Rust 侧几乎全部待建。
3. **与 02-stage-vm 的差异**——02-stage-vm 有旧 fork 主线文档可迁移；13-stage-ipc **无旧文档可迁移**（只有占位 README）。因此本计划从零定义文档集，`draft/` 仅保留占位素材；覆盖契约（§5）是后续写作的**唯一权威基线**，必须一次到位（同 03-stage-rs / 10-stage-mib 先例）。

### 1.2 新主线：IPC server 启动顺序 + 运行时主循环

与 `01-stage-kernel` / `02-stage-vm` / `03-stage-rs` / `10-stage-mib` 相同，采用**读者学习顺序 = 系统实际执行顺序**的组织原则。IPC 的启动与运行是严格线性的：

```
RS 运行时加载 IPC（不在 boot_image，ipc.conf 定义特权面）
  │
  ▼ main.c:216  main()
  ├─ env_setargs()                           ← 01：启动参数
  ├─ sef_local_startup()                     ← 01：SEF 回调注册（init_fresh/init_restart/signal）
  │    └─ sef_cb_init_fresh()（main.c:80）    ← 01/03：rmib_register(CTL_KERN, KERN_SYSVIPC)
  │         ├─ kern_ipc_node（"kern.ipc" 子树）← 03：远程 MIB 子树
  │         └─ kern_ipc_info（sysvipc_info）   ← 03：SEM/SHM info 分派 → 05/08
  │
  ▼ main.c:239-283  主循环（运行时）
  ├─ sef_receive_status(ANY)                 ← 01
  ├─ is_ipc_notify → 忽略通知                 ← 01
  ├─ PM PROC_EVENT → got_proc_event          ← 09：进程事件（sem 阻塞取消）
  ├─ MIB 消息 → rmib_process                 ← 03：远程 MIB 请求
  ├─ call_vec 分发（IPC_BASE 索引）           ← 01
  │   ├─ IPC_SHMGET → do_shmget              ← 07
  │   ├─ IPC_SHMAT  → do_shmat               ← 08
  │   ├─ IPC_SHMDT  → do_shmdt               ← 08
  │   ├─ IPC_SHMCTL → do_shmctl              ← 08
  │   ├─ IPC_SEMGET → do_semget              ← 05
  │   ├─ IPC_SEMCTL → do_semctl              ← 05
  │   └─ IPC_SEMOP  → do_semop               ← 06
  ├─ 回复（SUSPEND 除外，m_type = errno）      ← 01
  └─ update_refcount_and_destroy()           ← 08：shm 引用计数收尾（lazy nattch）
  │
  ▼ SIGTERM（sef_cb_signal_handler，main.c:101）← 10
  ├─ is_sem_nil() && is_shm_nil() → rmib_deregister + sef_exit(0)
  └─ 否则警告 "exit with unclean state"
```

**每篇文档必须能回答一个问题：它位于 IPC server 启动时序（sef_cb_init_fresh）或主循环（dispatch）的哪个位置。** 这是从 `01-stage-kernel/00-kernel-overview.md §3` 与 `02-stage-vm/plan.md §1.2` 继承的组织原则。

### 1.3 次主线：SysV IPC 调用旅程

IPC server 的全部工作本质是 **SysV IPC 对象（信号量集合/共享内存段）的生命周期管理**。次主线以"一次用户调用从 libc 到达 IPC server 再到 VM/PM"的旅程贯穿阶段 3~4，其路径图在 `05-ipc-sem-table` / `06-ipc-semop` / `07-ipc-shm-segment` / `08-ipc-shm-attach` 内部绘制：

```
用户进程 semget(2)/semctl(2)/semop(2)/shmget(2)/shmat(2)/shmdt(2)/shmctl(2)
  │ libc（IPC_BASE 0xD00 消息面 → IPC_PROC_NR）
  ▼ IPC server 主循环 dispatch（01）
  ├─ 02 协议面（消息结构/特权）→ 04 权限模型（check_perm）
  ├─ sem：05 表管理（semget/semctl）→ 06 原子操作（semop/等待队列/进程事件取消）
  ├─ shm：07 段创建（shmget/vm_getphys）→ 08 挂接与引用计数（shmat/shmdt/shmctl/VM 交互）
  └─ 09 进程事件（PM 订阅与处理）支撑 06 的阻塞取消
```

**位置可回答性双锚点**：每篇文档同时回答它在（a）boot/主循环序列、（b）SysV 调用旅程中的位置。

### 1.4 与 kernel IPC 机制的边界（本 stage 第一原则）

| 层 | 内容 | 归属 |
|----|------|------|
| **kernel IPC 机制** | SEND/RECEIVE/NOTIFY/SENDREC 原语、阻塞/唤醒、死锁检测、通知位图 | `01-stage-kernel/12-ipc-core.md` + `notes/rewrite/ipc-sendrec.md`（SENDREC 原子性）——**本 stage 不覆盖** |
| **IPC server** | SysV 信号量（semget/semctl/semop）与共享内存（shmget/shmat/shmdt/shmctl）对象管理，运行在用户态、通过 kernel IPC 收发消息 | 本 stage（00~10） |

命名澄清：Rust crate 名为 `ipc-server`（`os/servers/ipc-server/`，避免与 kernel ipc 概念混淆，README 已声明）。

---

## 2. 新文档编号与阶段划分

> 编号规则与 `01-stage-kernel` 一致：`NN-短横线语义名.md`；`00-` 为总览；`99-` 为全局概念。`draft/` 仅保留占位 README（素材），新编号在顶层重新建立。

### 阶段总览

| 阶段 | 编号 | 文档 | 语义模块 | C 源码 | Rust 模块（规划） | draft 来源 | 变更 |
|------|------|------|---------|--------|------------------|-----------|------|
| 0 总览 | 00 | `00-ipc-overview.md` | IPC server 是什么、kernel IPC 边界、boot 链位置、启动主线图、SysV 调用旅程次主线、文档导航 | `servers/ipc/` 全部 | 全部 | `draft/README` | **重写**（导航按启动主线叙事） |
| 1 启动入口与主循环 | 01 | `01-ipc-init-main.md` | `main`/`sef_local_startup`/`sef_cb_init_fresh` 骨架/主循环消息分类/`call_vec` 分发/回复与 `SUSPEND`/`update_refcount_and_destroy` 调用点/notify 忽略 | `main.c:216-284`（+ `sef_local_startup` :124-142） | `main.rs`、`lib.rs` | — | **新建**（骨架，映射 03~10 锚点） |
| 2 协议面与权限 | 02 | `02-ipc-message-contract.md` | `IPC_BASE` 0xD00、7 个 `IPC_*` call、7 种 `m_lc_ipc_*` 消息结构字段、`PROC_EVENT` 消息、`ipc.conf` 特权面（可接收端点 + system/vm 调用面） | `include/minix/ipc.h:355-422,1802-1813`、`include/minix/com.h:785-796,610-619`、`ipc.conf` | `minix-types`（A-1 缺口） | — | **新建**（协议面） |
| 2 | 03 | `03-ipc-mib-registration.md` | `kern.ipc` 远程 MIB 子树：`kern_ipc_table`/`kern_ipc_node`/`kern_ipc_info`、`rmib_register`/`rmib_deregister`/`rmib_process` 客户端契约、`KERN_SYSVIPC_*` 常量 | `main.c:27-47,54-79` + `lib/libsys/rmib.c`（外部客户端） | `mib_client`（A-6） | — | **新建** |
| 2 | 04 | `04-ipc-permissions.md` | `check_perm`/`prepare_mib_perm`、SysV 权限模型（uid/gid/root 绕过、0700/0070/0007 位）、各 handler 掩码调用点 | `utility.c`（49 行） | `perms.rs` | — | **新建** |
| 3 信号量 | 05 | `05-ipc-sem-table.md` | sem 集合表与生命周期：`sem_list`/`sem_find_key`/`sem_find_id`/`do_semget`/`do_semctl`/`remove_set`/`fill_seminfo`/`is_sem_nil`/`get_sem_mib_info` | `sem.c:53-162,251-293,430-468,469-653,787-865` | `sem/table.rs`、`sem/ctl.rs` | — | **新建** |
| 3 | 06 | `06-ipc-semop.md` | semop 原子性与等待队列：`do_semop`/`try_semop`/`check_set`/`iproc` 表/`inc_susp_count`/`dec_susp_count`/`complete_semop`/`send_reply`/`sem_process_event` | `sem.c:163-250,294-429,654-786,866-888` | `sem/op.rs`、`sem/waiter.rs` | — | **新建** |
| 4 共享内存 | 07 | `07-ipc-shm-segment.md` | shm 段创建：`shm_list`/`shm_find_key`/`shm_find_id`/`do_shmget`/`vm_getphys`、mmap 内存面（A-9） | `shm.c:6-12,15-49,51-129` | `shm/segment.rs` | — | **新建** |
| 4 | 08 | `08-ipc-shm-attach.md` | 挂接与引用计数：`do_shmat`/`do_shmdt`/`do_shmctl`/`update_refcount_and_destroy`/`fill_shminfo`/`is_shm_nil`/`get_shm_mib_info` | `shm.c:130-247,248-260,261-378,379-447,465-469` | `shm/attach.rs`、`shm/refcount.rs` | — | **新建** |
| 5 跨服务协作 | 09 | `09-ipc-proc-events.md` | PM 进程事件：`got_proc_event`/`update_sub`/`update_sem_sub`/`proceventmask`/`SEM_EVENTS`/`PROC_EVENT_REPLY` | `main.c:144-214` + `libsys` `proceventmask` | `events.rs` | — | **新建** |
| 5 | 10 | `10-ipc-lifecycle.md` | 生命周期：`sef_cb_signal_handler`/SIGTERM 清理/`is_sem_nil`+`is_shm_nil` 退出判定/`rmib_deregister`/`sef_exit`/`init_restart` 重启语义 | `main.c:101-122` | `lifecycle.rs` | — | **新建** |
| 99 全局概念 | 99 | `99-ipc-global-concepts.md` | 常量表（SEMMNI=10/SEMMSL=60/SEMVMX=32767/SEMOPM=100/SHMMNI=1024/SHMSEG=32/...）、errno 特殊语义（SUSPEND/EDONTREPLY/EIDRM/EINTR）、IPCID↔IX/SEQ 编码、endpoint/跨服务引用收口 | `sys/sem.h`、`sys/shm.h`、`sys/ipc.h` | `minix-types` | — | **新建** |

### 2.1 阶段间的叙事衔接

每个阶段结尾的文档需含"过渡"小节（参照 `01-stage-kernel` 每篇 §6"过渡"），说明本阶段在启动时序中的位置与下一阶段的入口：

```
00（总览）→ 01（启动 + 主循环）→ 02/03/04（协议面/MIB 子树/权限模型）
→ 05/06（信号量：表 → 操作）→ 07/08（共享内存：段 → 挂接）
→ 09（进程事件）→ 10（生命周期收口）→ 99（全局概念收口）
```

---

## 3. 讲述结构规范（参照 01-stage-kernel）

### 3.1 每篇文档的章节模板

与 `01-stage-kernel` 一致（见 `03-kmain-cstart.md`、`04-platform-discovery.md` 等）：

1. **概念**——为什么需要这个机制、类比、边界声明（前置依赖/本篇不覆盖什么）
2. **C 源码分析**——ground truth，必须 grep 实证，禁止凭记忆
3. **Rust 设计决策**——类型系统建模、与 C 的差异（含 ARCH 标注）
4. **实现详解**——Rust 模块结构、关键不变量
5. **测试要点**——相关测试函数 + 测试总数声明（基线见 §3.5）
6. **过渡**——在启动时序/主循环中的位置，下一篇入口
7. **参见**——跨文档引用（新编号）

### 3.2 组织原则（继承 01-stage-kernel §3）

1. **概念首次出现即完整解释**——后续只引用不复述（如 `check_perm` 在 04 首次完整解释，05/06/07/08 只引用掩码调用点）
2. **禁止前向引用**——依赖机制前置（本计划的编号已保证：协议面 02 先于 handler 05~08，权限 04 先于各 handler，MIB 子树 03 先于 info 实现 05/08）
3. **每篇一个语义单元**——读者可独立阅读（sem/shm 各拆"表/生命周期"与"操作语义"两篇）
4. **位置可回答性**——每篇回答"它在 sef_cb_init_fresh / 主循环 dispatch 的哪个位置"

### 3.3 引用规则

- 各文档之间用新编号交叉引用（如 `06-ipc-semop.md` §进程事件取消 → `09-ipc-proc-events.md`）
- 与 kernel 文档交叉引用用 `../01-stage-kernel/NN-*.md`（IPC 原语 → `12-ipc-core.md`；safecopy/grant → `18-syscall-copy.md`）
- 与 VM 文档交叉引用用 `../02-stage-vm/NN-*.md`（VM_MMAP/VM_MUNMAP → `20-vm-mmap.md`/`21-vm-munmap.md`；VM_GETPHYS/VM_GETREF → `26-vm-queries.md`；VM_REMAP → `20-vm-mmap.md`）
- 与 MIB/RS 文档交叉引用用 `../10-stage-mib/NN-*.md`（RMIB 客户端 → `22-mib-rmib-client.md`）、`../03-stage-rs/NN-*.md`（SEF/加载生命周期）
- 对 draft 素材的引用一律指向 `draft/`，并标注"素材"；正式文档不引用 review 产物

### 3.4 每篇文档边界声明（执行级）

> 写作每篇文档前，先按本表声明**前置依赖 / 本篇职责 / 不覆盖（移交）**，防止内容交叉与遗漏。边界以 §5.3 函数清单为准绳。

| 编号 | 前置依赖 | 本篇职责（核心语义） | 不覆盖（移交） |
|------|---------|---------------------|--------------|
| 00 | 无 | 总览、kernel IPC 边界、启动主线图、SysV 调用旅程次主线、文档导航、设计原则 | 一切机制细节（01~10、99） |
| 01 | 00 + kernel `12-ipc-core` | `main`/`sef_local_startup`/`sef_cb_init_fresh` 骨架、主循环消息分类、`call_vec` 分发、回复与 `SUSPEND`、notify 忽略、`update_refcount_and_destroy` 调用点 | 各 handler 实现（05~08）、MIB 子树细节（03）、进程事件（09）、SIGTERM（10） |
| 02 | 01 | `IPC_BASE`/7 call numbers、7 种消息结构字段级语义、`PROC_EVENT` 消息、`ipc.conf` 特权面、A-1 缺口标注 | 消息处理流程（01）、各 handler 使用（05~08） |
| 03 | 02 + MIB `22`（rmib 客户端） | `kern_ipc_table`/`kern_ipc_node`/`kern_ipc_info` 分派、`rmib_register`/`deregister`/`process` 调用契约、`KERN_SYSVIPC_*` 常量、A-6 | sysvipc_info 具体实现（05/08）、MIB 服务内部（10-stage-mib） |
| 04 | 02 | `check_perm` 全语义（uid/gid/root/0700-0070-0007）、`prepare_mib_perm`、各 handler 掩码调用点（semctl 三类/semop/shmget/shmat/shmctl） | 消息字段（02）、handler 流程（05~08） |
| 05 | 02/04 + 03（info 面） | `sem_list`/`sem_list_nr`/`sem_find_key`/`sem_find_id`/`do_semget`/`do_semctl` 全 cmd/`remove_set`/`fill_seminfo`/`is_sem_nil`/`get_sem_mib_info` | semop 原子性（06）、订阅生命周期（09） |
| 06 | 05 + 04 | `iproc` 表/`try_semop` 原子性（乐观执行+回滚）/`check_set` FIFO 重试/`inc_dec_susp_count`/`complete_semop`/`send_reply`/`sem_process_event` 取消 | 表管理（05）、事件订阅机制（09） |
| 07 | 02/04 | `shm_list`/`shm_find_key`/`shm_find_id`/`do_shmget`（mmap + `vm_getphys`）、字段初始化、A-3/A-9 | 挂接/引用计数（08）、权限掩码细节（04） |
| 08 | 07 + 04 | `do_shmat`/`do_shmdt`/`do_shmctl` 全 cmd/`update_refcount_and_destroy`（lazy nattch + SHM_DEST 延迟销毁）/`fill_shminfo`/`is_shm_nil`/`get_shm_mib_info`、A-3 决策点 | 段创建（07）、权限模型（04） |
| 09 | 01 + 06（`sem_process_event` 消费方） | `got_proc_event`/`update_sub`/`update_sem_sub`/`proceventmask` 订阅生命周期/`SEM_EVENTS`/`PROC_EVENT_REPLY` asynsend3 | 阻塞取消内部（06） |
| 10 | 01 + 03 + 05/08（`is_sem_nil`/`is_shm_nil`） | SIGTERM 处理、退出判定、`rmib_deregister`、`sef_exit`、`init_restart` 重启语义（动态状态丢失） | 订阅掩码（09）、MIB 树结构（03） |
| 99 | 无 | 常量表、errno 特殊语义、IPCID 编码、跨服务引用收口 | 各机制细节（01~10） |

### 3.5 测试基线

> 2026-08-16 现状：`os/servers/ipc-server/` 为空壳 stub（`lib.rs` 仅 `pub fn init() {}`，`main.rs` 为 `init(); loop {}`），无测试。各文档"测试要点"章在实现期对账；`cargo test -p minix-ipc-server` 基线待实现后建立（参照 `02-stage-vm/plan.md §3.5` 的先例：`cargo test -p minix-vm --lib` = 322 passed / 15 failed）。

### 3.6 Review gate 要求

> 每篇改写完成后接入 review gate：`outline.v*.md` / `design.v*.md` 快照入 `.review/codex/ipc/{NN}-{name}/`（Gate H.6 要求），scan.md 记录 Skill Invocation Log。缺失即 Gate H.6 FAIL（见 `prompt/` 与 `.codex/skills/` 的 review-process-skill）。

---

## 4. 架构演进（ARCH）清单

> 按 review-core 要求，ARCH 必须三处一致标注：Minix3 行为对照点 + design doc + 代码注释。以下为 13-stage-ipc 范围内的 ARCH 项，写文档时必须逐项落实。

| # | ARCH 项 | Minix3 现状 | minix-rs 演进 | 涉及新文档 | 状态 |
|---|---------|------------|--------------|-----------|------|
| A-1 | **IPC 消息类型缺失** | `mess_lc_ipc_*` 7 种（`ipc.h:355-422`）+ `mess_pm_lsys_proc_event`（`ipc.h:1802-1813`），字段为标量 + 用户指针（`void *ops`/`const void *addr` 等，56 字节消息内可容纳） | `minix-types` **尚无 ipc.rs 消息类型**，需新增 `IpcSemgetIn/Out`、`IpcSemctlIn/Out`、`IpcSemopIn`、`IpcShmgetIn/Out`、`IpcShmatIn/Out`、`IpcShmdtIn`、`IpcShmctlIn/Out` + `ProcEvent`（参照 `vm.rs` In/Out 语义层模式 + `DecodeFromM1`/`EncodeToM1`） | 02 | **缺口**：minix-types 新增 |
| A-2 | **等待队列与 iproc 表** | `struct iproc iproc[NR_PROCS]` 静态数组、slot 索引 = `_ENDPOINT_P(endpt)`（断言 `slot < NR_PROCS`）+ 每 sem 集合一个 TAILQ 双向链表（FCFS 公平性）；`assert(ip->ip_sem == NULL)` 保证单挂起 | Rust 演进：`iproc` 表 → 按 endpoint 索引的 `Option<SemWaiter>`（typestate：`None` = 无挂起，`Some` = 挂起中）；TAILQ → `VecDeque`；`assert` 由类型系统（Option 占用检查）表达 | 06 | 待设计 |
| A-3 | **shm 引用计数（lazy nattch）** | `update_refcount_and_destroy()` 每次主循环收尾**全表轮询** `vm_getrefcount(sef_self(), page)`（O(n)），`nattch = rc - 1`；`SHM_DEST` 段在 nattch 归零后才真正销毁（延迟销毁） | Rust 演进候选：attach/detach 时**显式维护引用计数**（O(1)，`shm_nattch` 即时更新），保留 SHM_DEST 延迟销毁语义；或保持 lazy 轮询。**决策点**（directMap 同类结构演进） | 08 | **待决策** |
| A-4 | **libc 依赖面替换** | `mmap`/`munmap`/`memset`（IPC 自身地址空间）、`malloc`/`free`（semop sops 拷贝、`SEMOPM` 上限）、`clock_time(NULL)`（ctime/otime/atime/dtime）、`printf`（警告/调试） | `no_std`：映射 → VM 服务原语（A-9）；分配 → 全局分配器/专用 `SemAllocator`（cap 上限）；时间 → 内核时钟服务（`sys_times`/时钟抽象）；打印 → 日志服务 | 05/06/07/08/10 | 待设计 |
| A-5 | **minix-sys 依赖面（stub）** | `getnuid`/`getngid`/`getnpid`（`libsys/getepinfo.c`）、`proceventmask`、`sys_datacopy`、`ipc_sendnb`、`asynsend3`、`sef_*`（`sef_receive_status` 等）、`vm_remap`/`vm_unmap`/`vm_getphys`/`vm_getrefcount`（`libc/sys/mmap.c`） | `minix-sys` 当前为 stub（`todo!()`），全部待实现；按 `os/servers/` 既有服务先例（RS/VM）逐项落地，错误码映射 Minix3 errno | 01/04/05/06/07/08/09 | 待实施 |
| A-6 | **RMIB 客户端归属** | `libsys/rmib.c` 客户端（`rmib_register`/`rmib_deregister`/`rmib_process`）供 IPC 注册 kern.ipc 子树（`asynsend3(MIB_PROC_NR)` + `COMMON_MIB_*` 请求） | 归属 `minix-sys`（rmib 模块）或复用 `10-stage-mib/22-mib-rmib-client.md` 契约（MIB plan A-8）；**IPC 是 rmib 客户端的首个落地消费者** | 03 | 待实施（依赖 10-stage-mib） |
| A-7 | **SEM_UNDO 排除契约** | `do_semop` 对 `SEM_UNDO` 返回 EINVAL + 警告（`sem.c:713-722`）；`seminfo.semmnu/semume/semusz = 0`（TODO 注释） | minix-rs **保持同样契约**（SEM_UNDO → EINVAL），**不作为演进**（Minix3 本身就是"不支持"）；显式写入行为契约 | 05/06 | 契约（非演进） |
| A-8 | **死/未实现项排除契约** | `#if 0 list_shm_ds`（`shm.c:448-464`）；`kern_ipc_table` 中 KERN_SYSVIPC_SHMMAX/SHMMNI/SHMSEG/SHMMAXPGS/SHMUSEPHYS 槽位 "not yet supported"（`main.c:67-75`）；`KERN_SYSVIPC_MSG = 0`（无 SysV 消息队列）；`DEBUG_SEM` 调试宏；`verbose` 打印 | 不实现，标注排除 + 槽位保留语义（`KERN_SYSVIPC_MSG=0` 保持）；调试宏 → cfg 特性 | 03/05/06/07/08 | 排除（grep 实证，见 §5.5） |
| A-9 | **VM 服务契约（跨 stage）** | `vm_remap`(VM_REMAP)/`vm_unmap`(VM_UNMAP)/`vm_getphys`(VM_GETPHYS)/`vm_getrefcount`(VM_GETREF) + `mmap`(VM_MMAP)/`munmap`(VM_MUNMAP)（`libc/sys/mmap.c` → VM_PROC_NR） | 与 02-stage-vm 重写后的 VM 消息面对齐（`minix-types/vm.rs` 已有 VM_REMAP/VM_GETPHYS/VM_GETREF 等）；映射原语在 `minix-sys` 封装；`ipc.conf` vm 特权面（REMAP/REMAP_RO/SHM_UNMAP/GETPHYS/GETREF）必须逐项落地 | 07/08 | 跨 stage 契约 |
| A-10 | **单线程事件循环执行模型** | 主循环 `sef_receive_status(ANY)` 串行处理；`SUSPEND` 后由 `check_set`/`sem_process_event` 异步恢复（无并发） | 与 02-stage-vm 相同：单线程事件循环，`Rc`/`RefCell`/`!Send`/`!Sync` 合理；无内核级并发，不需要 `Arc`/`Mutex` | 01/06 | 参照先例 |

---

## 5. 覆盖完整性核对（对照 minix3 源码）

### 5.1 C 源文件 → 新文档映射（4 个 .c）

| C 源文件 | 行数 | 覆盖文档 | 核对 |
|---------|------|---------|------|
| `servers/ipc/main.c` | 284 | 01/03/09/10（+02 消息面） | 已核对 |
| `servers/ipc/sem.c` | 888 | 05/06（+03 sysvipc_info 面） | 已核对 |
| `servers/ipc/shm.c` | 469 | 07/08（+03 sysvipc_info 面） | 已核对 |
| `servers/ipc/utility.c` | 49 | 04 | 已核对 |

### 5.2 头文件/协议面覆盖

| 头文件/库面 | 覆盖文档 | 核对 |
|------------|---------|------|
| `minix/include/minix/com.h:785-796`（`IPC_BASE` 0xD00、IPC_SHMGET..IPC_SEMOP）、`:610-619`（`PROC_EVENT`/`PROC_EVENT_REPLY`）、`:1151`（`SUSPEND`） | 02/09 | 已核对 |
| `minix/include/minix/ipc.h:355-422`（7 种 `mess_lc_ipc_*`）、`:1802-1813`（`mess_pm_lsys_proc_event`） | 02 | 已核对 |
| `servers/ipc/ipc.conf`（特权：system `UMAP`/`VIRCOPY`；uid 0；ipc 可接收端点 SYSTEM USER pm rs log tty ds vm；vm `REMAP`/`REMAP_RO`/`SHM_UNMAP`/`GETPHYS`/`GETREF`） | 02 | 已核对 |
| `sys/sys/ipc.h`（`ipc_perm`/`ipc_perm_sysctl`、IPC_R/W/M、IPC_CREAT/EXCL/NOWAIT、IPC_PRIVATE、IPC_RMID/SET/STAT、IPC_INFO=500、`IXSEQ_TO_IPCID`） | 02/04/99 | 已核对 |
| `sys/sys/sem.h`（`semid_ds`/`sembuf`/`seminfo`/`sem_sysctl_info`、`SEM_UNDO`/`SEM_ALLOC`/`SEM_A`/`SEM_R`、GETVAL..SETALL、SEM_STAT=18/SEM_INFO=19、SEMMNI=10/SEMMNS=60/SEMMSL/SEMMNU=30/SEMUME=10/SEMOPM=100/SEMVMX=32767/SEMAEM=16384） | 02/05/06/99 | 已核对 |
| `sys/sys/shm.h`（`shmid_ds`/`shminfo`/`shm_info`/`shm_sysctl_info`、SHM_RDONLY/SHM_RND/SHM_DEST=0x0400/SHM_LOCKED、SHMMNI=1024/SHMSEG=32、`SHM_ALLOC`=0x0800 私有标志） | 02/07/08/99 | 已核对 |
| `sys/sys/sysctl.h:275,684-705`（`KERN_SYSVIPC`=82、INFO=1、MSG=2、SEM=3、SHM=4、SEM_INFO=5、SHM_INFO=6） | 03 | 已核对 |
| `minix/include/minix/rmib.h` + `minix/lib/libsys/rmib.c`（RMIB 客户端：register/deregister/process） | 03 | 已核对 |
| `minix/include/minix/syslib.h:289-294`（`PROC_EVENT_EXIT`=0x01/`PROC_EVENT_SIGNAL`=0x02、`proceventmask`） | 09 | 已核对 |
| `minix/lib/libc/sys/mmap.c`（`vm_remap`/`vm_unmap`/`vm_getphys`/`vm_getrefcount`、`mmap`/`munmap`） | 07/08 | 已核对 |
| `minix/lib/libsys/getepinfo.c`（`getnuid`/`getngid`/`getnpid`）、`minix/lib/libsys/sef.c`（SEF 生命周期）、`sys/sys/errno.h`（EIDRM=82/EDONTREPLY=203/EOPNOTSUPP=45） | 01/04/05/06/07/08/10/99 | 已核对 |

### 5.3 函数/符号清单映射

> 以下为 `servers/ipc/` 全部顶层定义（37 函数 + 7 数据/表，grep 实证见 §7.2），逐一落入新文档。行号以 2026-08-16 工作区为准。

**main.c（8 函数 + 3 数据）**

| 符号 | 行 | 覆盖文档 |
|------|----|---------|
| `call_vec[]`（call table） | 12-25 | 01 |
| `kern_ipc_info` | 27-47 | 03 |
| `kern_ipc_table[]` / `kern_ipc_node` | 54-79 | 03 |
| `sef_cb_init_fresh` | 80-99 | 01/03 |
| `sef_cb_signal_handler` | 101-122 | 10 |
| `sef_local_startup` | 124-142 | 01 |
| `update_sub` | 144-174 | 09 |
| `update_sem_sub` | 176-189 | 09 |
| `got_proc_event` | 191-214 | 09 |
| `main` | 216-284 | 01 |
| `event_mask` / `verbose`（全局） | 4-6 | 09/01 |

**sem.c（16 函数 + 3 数据）**

| 符号 | 行 | 覆盖文档 |
|------|----|---------|
| `struct iproc` + `iproc[NR_PROCS]` | 6-14 | 06 |
| `struct semaphore` | 16-23 | 05/06 |
| `struct sem_struct` + `sem_list[SEMMNI]` + `sem_list_nr` | 39-46 | 05 |
| `sem_find_key` | 53-71 | 05 |
| `sem_find_id` | 72-92 | 05 |
| `do_semget` | 93-162 | 05 |
| `inc_susp_count` / `dec_susp_count` | 163-205 | 06 |
| `send_reply` | 207-221 | 06 |
| `complete_semop` | 223-249 | 06 |
| `remove_set` | 251-293 | 05 |
| `try_semop` | 294-380 | 06 |
| `check_set` | 381-429 | 06 |
| `fill_seminfo` | 430-468 | 05 |
| `do_semctl` | 469-653 | 05 |
| `do_semop` | 654-786 | 06 |
| `get_sem_mib_info` | 787-853 | 05 |
| `is_sem_nil` | 854-865 | 05/10 |
| `sem_process_event` | 866-888 | 06 |

**shm.c（10 函数 + 1 死代码 + 2 数据）**

| 符号 | 行 | 覆盖文档 |
|------|----|---------|
| `struct shm_struct` + `shm_list[SHMMNI]` + `shm_list_nr` | 6-12 | 07 |
| `shm_find_key` | 15-32 | 07 |
| `shm_find_id` | 33-50 | 07 |
| `do_shmget` | 51-129 | 07 |
| `do_shmat` | 130-172 | 08 |
| `update_refcount_and_destroy` | 173-208 | 08 |
| `do_shmdt` | 209-247 | 08 |
| `fill_shminfo` | 248-260 | 08 |
| `do_shmctl` | 261-378 | 08 |
| `get_shm_mib_info` | 379-447 | 08 |
| `list_shm_ds`（`#if 0` 死代码） | 448-464 | §5.5 排除 |
| `is_shm_nil` | 465-469 | 08/10 |

**utility.c（2 函数）**

| 符号 | 行 | 覆盖文档 |
|------|----|---------|
| `check_perm` | 4-36 | 04 |
| `prepare_mib_perm` | 38-49 | 04 |

### 5.4 外部消费者与跨服务契约

| 方向 | 消费者/契约 | 覆盖文档 |
|------|------------|---------|
| libc → IPC | `semget`/`semop`/`semctl`/`shmget`/`shmat`/`shmdt`/`shmctl`（`IPC_BASE` 0xD00 消息面，`lib/libc/sys/*.c`） | 02（消息面契约） |
| PM → IPC | `PROC_EVENT` 进程事件（exit=0x01/signal=0x02），IPC 以 `PROC_EVENT_REPLY` 回执（asynsend3） | 09 |
| MIB ↔ IPC | `rmib_register`/`rmib_deregister`（IPC → MIB）、`COMMON_MIB_*` 请求（MIB → IPC 的 `rmib_process`） | 03 |
| VM ↔ IPC | `vm_remap`/`vm_unmap`/`vm_getphys`/`vm_getrefcount` + `mmap`/`munmap`（IPC → VM）；ipc.conf vm 特权面 5 项 | 07/08 |
| IPC → kernel | `sys_datacopy`（SELF↔m_source 数据拷贝）、system 特权面 `UMAP`/`VIRCOPY` | 05/06/07/08 |
| IPC → PM | `proceventmask`（订阅/退订进程事件） | 09 |
| ipcs(1)/sysctl(8) → IPC | `kern.ipc.sysvipc_info`（SEM_INFO=5/SHM_INFO=6，**无权限要求**，NetBSD 语义） | 03/05/08 |
| RS ↔ IPC | RS 运行时加载 + SEF `init_restart`/signal 生命周期（03-stage-rs） | 10 |
| kernel IPC 机制 | send/receive/notify 原语（01-stage-kernel/12-ipc-core）——**非本 stage 范围** | 00（边界声明） |

### 5.5 明确排除 / 跳过的项

| 项 | 位置 | 处理 |
|----|------|------|
| `list_shm_ds` | `shm.c:448-464`（`#if 0`） | 不实现（死代码） |
| KERN_SYSVIPC_SHMMAX/SHMMNI/SHMSEG/SHMMAXPGS/SHMUSEPHYS 槽位 | `main.c:67-75`（"not yet supported"） | 不实现，槽位保留（访问返回对应错误） |
| `KERN_SYSVIPC_MSG` | `main.c:54`（=0，无 SysV 消息队列） | 保持 0（Minix3 无消息队列支持） |
| `SEM_UNDO` | `sem.c:713-722`（EINVAL + 警告）；`seminfo.semmnu/semume/semusz=0` | 保持同样契约（EINVAL），写入行为契约（A-7） |
| `DEBUG_SEM` 调试宏 | `sem.c:579-582,618-621,628-632` | 不实现（cfg 特性或省略） |
| `verbose` 打印 | `main.c:6` | 可选 cfg（设计差异） |
| `_sem_base`/`_shm_internal` 私有字段 | `sys/sem.h`、`sys/shm.h` | Rust 内部布局自由演进（不保留 C 布局） |

### 5.6 覆盖结论与拆分答案

**1. 是否确保全部覆盖？** 是。§5.1~§5.5 已逐层核对：4 个 .c（1690 行）全部映射到新文档（§5.1）；12 个头文件/库面进入覆盖契约（§5.2）；37 个顶层函数 + 7 数据/表项逐一落入 01~10（§5.3）；9 组外部消费者与跨服务契约进入 02/03/07/08/09（§5.4）；SEM_UNDO/死代码/未实现槽位进入排除契约（§5.5）。§7.2 以命令证据复核，无遗漏。架构演进项（A-1~A-10，含 directMap 同类的引用计数结构演进 A-3 与 libc 依赖替换 A-4）全部单列，不混入行为文档。

**2. 计划拆分为多少个文档，简述如何拆分？** **12 篇**：`00` 总览 + `01~10` 语义模块 + `99` 全局概念，按 **6 个阶段**组织——阶段 1 启动入口与主循环（01）→ 阶段 2 协议面/权限/MIB 子树（02/03/04）→ 阶段 3 信号量（05 表管理 + 06 semop 原子性与等待队列）→ 阶段 4 共享内存（07 段创建 + 08 挂接与引用计数）→ 阶段 5 跨服务协作（09 进程事件 + 10 生命周期）→ 99 全局概念。拆分原则：每篇一个语义单元 + 位置可回答性 + 禁止前向引用；以函数清单（§5.3）为唯一边界准绳。**sem/shm 各拆两篇**的原因：C 中"表/生命周期管理"（semget/semctl/shmget）与"操作语义"（semop 原子性、shmat 引用计数）是两个可独立阅读的语义单元，且行数足够（sem.c 888 行拆 05/06，shm.c 469 行拆 07/08）。

**3. plan.md 是否完备、可直接作为后续工作依据？** 是。§2 给出编号/阶段/归属的完整映射，§3 给出写作模板与边界声明，§4 给出 ARCH 清单，§5 给出函数级覆盖契约（含行号），§6 给出实施顺序与状态跟踪。后续每篇文档依据 §3 + §4 + §5 从零写作即可。

---

## 6. 实施路线

> 每篇新文档 = 依据 §3 讲述结构 + §4 ARCH 标注 + §5 覆盖契约从零写作。所有 P0 修复完成后才可推进下一篇。当前 13-stage-ipc 无历史主线素材（仅占位 README），全部为新建。

1. **00-ipc-overview 新建**（导航，含 §1.2 启动时序图 + §1.3 SysV 调用旅程次主线 + §1.4 kernel IPC 边界）
2. **01-ipc-init-main 新建**（主循环 + 消息分类 + dispatch + SUSPEND 回复）
3. **02-ipc-message-contract 新建**（协议面，A-1 缺口标注 + minix-types 新增任务）
4. **03-ipc-mib-registration 新建**（kern.ipc 子树 + rmib 客户端契约，A-6）
5. **04-ipc-permissions 新建**（check_perm 全语义 + 掩码调用点）
6. **05-ipc-sem-table 新建**（semget/semctl/remove_set/fill_seminfo/get_sem_mib_info）
7. **06-ipc-semop 新建**（try_semop 原子性 + 等待队列 + sem_process_event，A-2）
8. **07-ipc-shm-segment 新建**（shmget + vm_getphys，A-3/A-9）
9. **08-ipc-shm-attach 新建**（shmat/shmdt/shmctl + 引用计数，A-3 决策落地）
10. **09-ipc-proc-events 新建**（订阅生命周期 + 事件处理 + 回执）
11. **10-ipc-lifecycle 新建**（SIGTERM 清理 + 重启语义）
12. **99-ipc-global-concepts 新建**（常量/错误码/跨服务引用收口）
13. **README.md 重建**（文档清单 + 启动链路位置，参照 `10-stage-mib/README.md` 模式）
14. **checklist.md 创建**（函数级基线，参照 `02-stage-vm/checklist.md` 模式；实现期创建，顶层保留不入 draft）

### 6.1 文档写作状态跟踪

> 每完成一篇，将状态改为 `reviewed`（附 scan 日期）。全部 `reviewed` 且无 P0 遗留 = 阶段完成。

| 编号 | 状态 | 首轮 review 日期 | 备注 |
|------|------|-----------------|------|
| 00 | pending | — | 新建导航 |
| 01 | pending | — | 新建主循环骨架 |
| 02 | pending | — | 新建协议面（A-1） |
| 03 | pending | — | 新建（A-6） |
| 04 | pending | — | 新建权限模型 |
| 05 | pending | — | 新建 |
| 06 | pending | — | 新建（A-2） |
| 07 | pending | — | 新建（A-3/A-9） |
| 08 | pending | — | 新建（A-3 决策落地） |
| 09 | pending | — | 新建 |
| 10 | pending | — | 新建 |
| 99 | pending | — | 新建全局概念 |

---

## 7. Review 记录

### 7.1 深度 review（2026-08-16）

**方法**：对照 §1~§5 逐项自检——① 拆分合理性（每篇一个语义单元、无职责重叠）；② 前向引用检查（编号序 vs 概念依赖序）；③ 函数面完整性（对照 §5.3 逐行核对）；④ 与 02-stage-vm/03-stage-rs/10-stage-mib 先例的一致性（编号规则/边界表/ARCH 表/覆盖契约格式）。

| # | 发现 | 等级 | 处置 |
|---|------|------|------|
| D-1 | 初版将 `sem_process_event` 归入 09（进程事件），但它是 sem.c 内等待队列的直接操作者（TAILQ_REMOVE + complete_semop），概念上属 semop 语义 | P1 | 归入 06；09 仅保留服务级订阅/路由（got_proc_event/update_sub/update_sem_sub） |
| D-2 | 初版将 `get_sem_mib_info`/`get_shm_mib_info` 归入 03（MIB 子树），但它们是 sem.c/shm.c 内的模块信息导出，与模块数据（sem_list/shm_list）强耦合 | P1 | 归入 05/08；03 只覆盖注册/分派（kern_ipc_info → 05/08 引用） |
| D-3 | `KERN_SYSVIPC_MSG=0`（无消息队列）与"not yet supported"槽位未区分——前者是 Minix3 既有语义，后者是未实现位 | P2 | §5.5 分开列项：MSG 保持 0；SHMMAX 等槽位保留 |
| D-4 | 编号 02/03/04 与 05~08 的依赖序初版有前向引用风险（03 的 kern_ipc_info 分派需要 05/08 的 info 实现） | P2 | 03 边界声明"info 实现 → 05/08"，与 01 骨架 defer 模式一致（先例：10-stage-mib 01/10 defer） |
| D-5 | ARCH 表初版 11 项过细（时间精度、printf 等单列），与先例粒度不一致 | P2 | 合并为 A-1~A-10（A-4 收 libc 面、A-5 收 minix-sys 面、A-8 收排除契约） |
| D-6 | §3.5 测试基线缺失（IPC 无测试基线可对账） | P2 | 新增 §3.5：声明现状为空壳 stub + 参照 02-stage-vm 先例 |

### 7.2 minix3 源码回归 review（2026-08-16）

**方法**：对 `minix3/minix/servers/ipc/` 全部 4 个 .c + inc.h + ipc.conf 逐一 grep 核对 §5.1/§5.2 映射表，并逐函数核对 §5.3 函数级清单。

**证据**：

```bash
ls minix3/minix/servers/ipc/                 # 4 个 .c + inc.h + ipc.conf + Makefile
wc -l minix3/minix/servers/ipc/*.c           # 284 + 888 + 469 + 49 = 1690 行，与 §5.1 一致
rg -n '^[a-zA-Z_][a-zA-Z0-9_ *]*\([^;]*$' minix3/minix/servers/ipc/*.c   # 38 匹配 = 37 函数 + call_vec（数据表）
rg -n 'IPC_BASE|IPC_SHMGET|IPC_SEMOP' minix3/minix/include/minix/com.h    # call numbers 0xD00..0xD07
rg -n 'm_lc_ipc_' minix3/minix/include/minix/ipc.h                        # 7 种消息结构
rg -n 'SEMMNI|SEMMSL|SEMVMX|SEMOPM|SEM_ALLOC' minix3/sys/sys/sem.h        # sem 常量
rg -n 'SHMMNI|SHM_ALLOC|SHM_DEST|SHM_RND' minix3/sys/sys/shm.h            # shm 常量
cat minix3/minix/servers/ipc/ipc.conf                                     # 特权面 3 段
rg -n 'KERN_SYSVIPC' minix3/sys/sys/sysctl.h                              # 82/1..6
rg -n 'rmib_register|rmib_process' minix3/minix/lib/libsys/rmib.c         # RMIB 客户端调用面
```

**结论**：4 个 .c 文件全部映射到新文档，无遗漏；38 个 grep 匹配 = 37 个顶层函数 + `call_vec`（数据表），+ 7 数据/表项逐一落入 01~10（§5.3）；9 组跨服务契约（PM/MIB/VM/kernel/libc/ipcs/RS）进入覆盖契约（§5.4）；ARCH 项（A-1~A-10）与 minix3 现状对照成立（A-1 消息类型缺口、A-3 引用计数决策点、A-6 rmib 客户端归属均已实证）。**回归复核修正**：§2/§5.3 共 12 处行号按 grep 实证修正（`event_mask`/`verbose` 4-6、`kern_ipc_table` 54-79、`iproc` 6-14、`sem_find_key` 53-71、sem 结构 39-46、shm 结构 6-12、`shm_find_key` 15-32、`shm_find_id` 33-50、§2 表 03/07 行）。**覆盖完整性通过**。

---

## 8. 参见

- `draft/` — 旧占位 README（素材）
- `../01-stage-kernel/00-kernel-overview.md` — 讲述结构参照（阶段划分/组织原则/过渡章节）
- `../02-stage-vm/plan.md`、`../03-stage-rs/plan.md`、`../10-stage-mib/plan.md` — 同流程先例（编号规则/ARCH 表/覆盖契约格式）
- `../10-stage-mib/22-mib-rmib-client.md` — RMIB 客户端契约（A-6 依赖）
- `../00-master-plan/README.md` — 目录重排与新主线说明
- `minix3/minix/servers/ipc/` — C 源码（ground truth）
- `os/servers/ipc-server/` — Rust 实现（当前为空壳 stub）
