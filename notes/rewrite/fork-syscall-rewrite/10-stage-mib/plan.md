# 10-stage-mib 文档重组计划（plan.md）

> **状态**: 定稿（2026-08-16 首版 + 深度 review + minix3 源码回归 review，见 §7）
> **范围**: `notes/rewrite/fork-syscall-rewrite/10-stage-mib/`
> **目标**: 以 **MIB server 启动顺序为主线**定义 MIB 全部文档；`sysctl(2)` 调用旅程为次主线；最终覆盖 Minix3 MIB server（`servers/mib/`，8 个 .c，4990 行）+ 协议面（`com.h`/`ipc.h`/`sysctl.h` 两层）+ 客户端契约（`libc` sysctl(3) 系列 + `libsys/rmib.c`）+ 外部消费者（ProcFS/IPC/LWIP/UDS）全部语义，支撑 MIB server 的彻底 Rust 重写
> **对照**: `01-stage-kernel/`（讲述结构参照）、`02-stage-vm/`/`07-stage-ds/`（同流程先例）、`minix3/minix/servers/mib/`（ground truth）、`os/servers/mib/`（Rust 实现，当前为 stub）

---

## 1. 背景与动机

### 1.1 现状问题

`10-stage-mib/` 目录自 2026-08-14 补建以来仅有**占位 README**（2026-08-16 移入 `draft/`），没有任何正式文档。现状与 MIB 的 boot 地位不匹配：

1. **boot 位置关键**——MIB 是 boot_image 中直接登记的服务之一（`kernel/table.c:60`，`{MIB_PROC_NR, "mib"}` 紧跟 TTY 之后、VM 之前），因为 `init(8)` 在自身启动期间就要调用 `sysctl(2)`（`main.c:9-16` 注释），晚启动会让 sysctl 无法正确实现。同时 MIB 需要超级用户权限，因为要发起特权调用并获取其他服务的特权信息。
2. **语义量大且分散**——`servers/mib/` 共 4990 行 C（8 个 .c：main.c 492 + tree.c 1842 + remote.c 477 + kern.c 508 + proc.c 1288 + hw.c 140 + minix.c 89 + vm.c 154），是**用户态服务中最大的一类**（对比：DS 811 行、IS ~1000 行、init 1902 行）。其中 tree.c 是核心（对象树 + sysctl 分发），proc.c 是第二大模块（进程表快照 + 三个 NetBSD 兼容信息接口）。文档必须按语义模块拆细，避免"一篇 2000 行"的失控文档。
3. **跨服务契约面最广**——sysctl(2) 是用户态 ABI（`sys/sys/sysctl.h` 的 NetBSD VERS_1 布局 + `sysctlnode`/`sysctldesc` 交换格式），MIB 同时是**远程子树挂载点**（`COMMON_MIB_*` 协议，IPC/LWIP/UDS 服务注册子树）和 **ProcFS 的进程信息提供者**（`minix_proc_list`/`minix_proc_data` 布局）。这些契约不写清，Rust 重写无法落地。
4. **与 07-stage-ds 相同**——无旧主线文档可迁移（只有占位 README），本计划从零定义文档集；§5 覆盖契约是后续写作的**唯一权威基线**，必须一次到位。

### 1.2 新主线：MIB server 启动顺序 + 运行时主循环

与 `01-stage-kernel` / `02-stage-vm` / `07-stage-ds` 相同，采用**读者学习顺序 = 系统实际执行顺序**的组织原则。MIB 的启动与运行是严格线性的：

```
boot_image 登记（table.c:60：MIB 紧跟 TTY、先于 VM）
  → kernel/main.c:265-267 对所有用户进程置 RTS_VMINHIBIT（VM 建页表前禁止调度）
  → VM 建页表解除抑制后，服务可被调度（参照 09-vm-boot-protocol）
  │
  ▼  main.c:433  main()
  ├─ mib_startup()                            ← 01：SEF 生命周期
  │    ├─ sef_setcb_init_fresh(mib_init)      ← 04：初始化回调（boot 锚点）
  │    └─ sef_setcb_init_restart(mib_init)    ← 04：重启复用（动态状态全丢，静态树保留）
  ▼  mib_init()（main.c:384，boot 锚点）
  ├─ mib_kern_init(&mib_table[CTL_KERN])      ← 13：kern 子树
  ├─ mib_vm_init(&mib_table[CTL_VM])          ← 14：vm 子树
  ├─ mib_hw_init(&mib_table[CTL_HW])          ← 14：hw 子树
  ├─ mib_minix_init(&mib_table[CTL_MINIX])    ← 15：minix 子树
  ├─ mib_tree_init()                          ← 04：全树递归初始化（版本/parent/计数）
  └─ mib_remote_init()                        ← 12：endpts 表复位
  │
  ▼  main.c:447-489  主循环（运行时）
  ├─ sef_receive_status(ANY, &m_in, &ipc_status)  ← 01
  ├─ is_ipc_notify → 打印告警 + continue（不收 notify 消息体）← 01
  ├─ switch(m_in.m_type)：
  │   ├─ MIB_SYSCTL     → mib_sysctl          ← 01（消息解码）→ 10（分发）→ handler
  │   ├─ MIB_REGISTER   → mib_register        ← 12（远程子树挂载）
  │   ├─ MIB_DEREGISTER → mib_deregister      ← 12（远程子树卸载）
  │   └─ default        → ENOSYS / EDONTREPLY ← 01
  └─ r != EDONTREPLY → m_out.m_type = r; ipc_sendnb ← 01
```

**每篇文档必须能回答一个问题：它位于 MIB 启动时序（mib_init）或主循环（dispatch）的哪个位置。** 这是从 `01-stage-kernel/00-kernel-overview.md §3` 继承的组织原则。

### 1.3 次主线：一次 sysctl(2) 调用的完整旅程

MIB 的全部工作本质是把 `sysctl(2)` 名字解析到对象树节点并读写数据。次主线以"一次 sysctl(2) 调用从用户态到数据返回"的旅程贯穿阶段 2~7，其路径图在 `10-mib-dispatch` 内部绘制：

```
用户态 sysctl(3)/__sysctl（libc）           ← 21 客户端
  ├─ CTL_USER 子树 → user_sysctl 本地处理（不进 MIB）← 21
  └─ 其余 → _syscall(MIB_PROC_NR, MIB_SYSCTL)  ← 02 消息面
  ▼ MIB 主循环 mib_sysctl()                 ← 01
  ├─ namelen 校验（0 < namelen ≤ CTL_MAXNAME=12）← 01
  ├─ name 拷贝（≤8 消息内嵌 / >8 sys_datacopy）  ← 01
  ├─ oldp/newp 构造（mib_oldp/mib_newp 不透明体）← 06
  └─ mib_dispatch(&call, oldp, newp)        ← 10
       ├─ 逐层解析名字（mib_root 出发）     ← 05 查找
       ├─ 元标识符（负 id）→ QUERY/CREATE/DESTROY/DESCRIBE ← 10/11/08
       ├─ CTLFLAG_PRIVATE → mib_authed      ← 07 权限
       ├─ CTLFLAG_REMOTE → mib_remote_call（转发远程服务）← 12
       ├─ 写权限检查（READWRITE/ANYWRITE）  ← 07
       ├─ node_func（函数驱动节点）→ handler ← 13~20
       └─ mib_readwrite（常规数据叶节点）    ← 09 数据读写
  ▼ 返回
  ├─ r ≥ 0：oldlen 写回；oldaddr≠0 且 oldlen<r → ENOMEM（部分拷贝+完整长度）← 01
  └─ r < 0：call_reslen（错误时携带的 oldlen，如 create 冲突 EEXIST）← 01
```

**第二条次主线：一条远程子树的挂载/卸载旅程**（阶段 8，路径图在 `12-mib-remote-subtrees` 内部绘制）：

```
服务启动 → rmib_register（libsys/rmib.c）       ← 22 客户端
  ├─ asynsend3(MIB_PROC_NR, MIB_REGISTER)      ← 02 消息面
  ▼ MIB mib_register → mib_do_register          ← 12
  ├─ DS label 校验（mib_get_label）             ← 12
  ├─ endpts 表分配（eid）                       ← 12
  ├─ mib_mount：路径走查 → 挂载点（覆盖已有节点 / 临时创建）← 12
  │    └─ COMMON_MIB_INFO 向服务取 name/desc   ← 12
  └─ node 挂入 endpts[eid].nodes 链表           ← 12
  ▼ 用户 sysctl 命中挂载点
  ├─ mib_remote_call（COMMON_MIB_CALL 转发 + grants）← 12
  ├─ 服务死亡（IPC 失败）→ mib_down 清全部挂载点 + ERESTART 续走本地 ← 12
  └─ 服务返回 ERESTART → mib_do_deregister     ← 12
  ▼ 服务退出/重启 → rmib_deregister / MIB_DEREGISTER → mib_unmount ← 12
```

---

## 2. 新文档编号与阶段划分

> 编号规则与 `01-stage-kernel` 一致：`NN-短横线语义名.md`；`00-` 为总览；`99-` 为全局概念。`draft/` 保留旧占位 README（作素材引用），新编号在顶层重新建立。

### 阶段总览（24 篇）

| 阶段 | 编号 | 文档 | 语义模块 | C 源码 | Rust 模块 | draft 素材 | 变更 |
|------|------|------|---------|--------|-----------|-----------|------|
| 0 总览 | 00 | `00-mib-overview.md` | MIB 是什么、启动主线图、sysctl 次主线、文档导航 | `servers/mib/` 全部 | `os/servers/mib/` 全部 | `draft/README.md` | **重写**为导航 |
| 1 启动入口 | 01 | `01-mib-init-main.md` | `main`/`mib_startup`/`mib_sysctl` 消息解码/主循环骨架/notify 拒绝/EDONTREPLY/ENOMEM 语义 | `main.c:277-383,415-492` | `main.rs`、`lib.rs` | `draft/README.md` | **新增**（从零） |
| 2 协议面 | 02 | `02-mib-message-contract.md` | MIB call numbers（com.h）、6 种消息结构（ipc.h）、`sysctlnode`/`sysctldesc` 交换格式（sysctl.h）、版本/类型/标志宏、errno 特殊语义 | `com.h:613-622,1026-1028`、`ipc.h`、`sys/sysctl.h`、`minix/sysctl.h` | `minix-types`（**缺 mib.rs 消息类型，A-1**） | 无 | **新增**：所有 handler 篇的前置协议文档 |
| 3 树模型与查找 | 03 | `03-mib-node-model.md` | `struct mib_node`/`struct mib_dynode` 全字段、4 类节点（PARENT×REMOTE 矩阵）、CTLTYPE/CTLFLAG 标志系统、计数（nodes/objects/remotes）、scratch 缓冲 | `mib.h:110-280` | `tree/node.rs`、`tree/flag.rs` | 无 | **新增** |
| 3 | 04 | `04-mib-static-tree-init.md` | `mib_root` + 7 个顶层节点表、`MIB_*` 静态初始化宏、`mib_init` 回调、`mib_tree_init`/`mib_tree_recurse`、版本初始化 | `main.c:36-64,384-413`、`tree.c:1476-1536`、`mib.h` 宏 | `tree/static_tree.rs`、`tree/init.rs` | 无 | **新增** |
| 3 | 05 | `05-mib-tree-lookup.md` | `mib_find`：静态数组 O(1)（`IS_STATIC_ID` + flags≠0）+ 动态链表 O(n)（按 id 排序，可提前终止） | `tree.c:34-88` | `tree/lookup.rs` | 无 | **新增** |
| 4 拷贝与权限原语 | 06 | `06-mib-copy-io.md` | `mib_oldp`/`mib_newp` 不透明体、`mib_inrange`/`getoldlen`/`getnewlen`/`copyout`/`copyin`/`copyin_aux`/`copyin_str`/`relay_oldp`/`relay_newp`/`setoldlen`、sys_datacopy vs grant 两种传输 | `main.c:64-258`、`tree.c:361-421` | `io/copy.rs`、`io/relay.rs` | 无 | **新增** |
| 4 | 07 | `07-mib-auth-model.md` | `mib_authed`（getnuid→PM 一次查询缓存）、PRIVATE/ANYWRITE/READWRITE/PERMANENT 权限位语义、超级用户特权模型 | `main.c:259-275`、`tree.c:156-174,565-570` 等 | `auth.rs` | 无 | **新增** |
| 5 动态节点生命周期 | 08 | `08-mib-dynamic-nodes.md` | `mib_check_name`/`mib_scan`/`mib_create`/`mib_add`/`mib_remove`/`mib_destroy`/`mib_upgrade`、动态 id 分配（CREATE_BASE=1024）、版本递增、OWNDATA/OWNDESC 内存所有权 | `tree.c:242-360,426-481,483-917` | `tree/dynamic.rs`、`tree/version.rs` | 无 | **新增** |
| 6 数据访问 | 09 | `09-mib-data-access.md` | `mib_getptr`/`mib_read`/`mib_write`/`mib_readwrite`、字符串长度语义、scratch vs malloc 临时缓冲、verify 回调、bool 消毒 | `tree.c:1098-1330` | `data/readwrite.rs` | 无 | **新增** |
| 7 分发与元标识符 | 10 | `10-mib-dispatch.md` | `mib_dispatch` 名字解析主循环、元标识符 switch、叶/函数/远程三类节点判定、ENOTDIR/EISDIR、写权限检查、远程 ERESTART 续走 | `tree.c:1332-1475` | `dispatch.rs` | 无 | **新增**：sysctl 次主线路径图 |
| 7 | 11 | `11-mib-query-describe.md` | `mib_query`（CTL_QUERY 枚举）+ `mib_describe`（CTL_DESCRIBE）+ `mib_copyout_node`/`mib_copyout_desc`（sysctlnode/sysctldesc 序列化） | `tree.c:90-241,918-1096` | `query.rs`、`describe.rs` | 无 | **新增** |
| 8 远程子树 | 12 | `12-mib-remote-subtrees.md` | `mib_remote_init`/`mib_down`/`mib_get_label`/`mib_do_register`/`mib_register`/`mib_do_deregister`/`mib_deregister`/`mib_remote_info`/`mib_remote_call`/`mib_mount`/`mib_unmount`、endpts 表（MIB_ENDPTS=32）、挂载策略、ERESTART 语义 | `remote.c` 全部 + `tree.c:1538-1842` | `remote.rs`、`tree/mount.rs` | 无 | **新增**：远程子树次主线路径图 |
| 9 子系统子树 | 13 | `13-mib-subtree-kern.md` | CTL_KERN 子树：函数节点（clockrate/hardclock/ccpu/cp_time/consdev/drivers/boottime/root_device/ipc_info mock）、verify 节点（securelvl/forkfsleep）、数据节点表（KERN_* 全集，KERN_MAXID=85 槽位，含大量未实现标注） | `kern.c` 全部 | `subtree/kern.rs` | 无 | **新增** |
| 9 | 14 | `14-mib-subtree-vm-hw.md` | CTL_VM（loadavg/uvmexp2/maxslp/uspace）+ CTL_HW（machine/ncpu/byteorder/physmem/usermem/pagesize/machine_arch/physmem64/usermem64/ncpuonline）子树、vm_info_stats/vm_info_usage 依赖 | `vm.c` 全部 + `hw.c` 全部 | `subtree/vm.rs`、`subtree/hw.rs` | 无 | **新增** |
| 9 | 15 | `15-mib-subtree-minix.md` | CTL_MINIX 子树：test 测试子树（MINIX_TEST_SUBTREE 门控）、mib 统计子树（nodes/objects/remotes）、proc 子树（list/data 表定义） | `minix.c` 全部 + `minix/sysctl.h` | `subtree/minix.rs` | 无 | **新增** |
| 10 进程信息 | 16 | `16-mib-proc-tables.md` | 进程表快照：`proc_tab`/`mproc_tab`/`fproc_tab`、`update_tables`（每 tick 一次 + 失败闩锁 + magic 校验）、PID 哈希表、`get_mslot`/`ticks_to_timeval`/`fill_wmesg` | `proc.c:1-225` | `proc/tables.rs` | 无 | **新增** |
| 10 | 17 | `17-mib-proc-lwp.md` | `get_lwp_stat` 状态机（ZOMB/DEAD/STOP/RUN/SLEEP + wchan 6 类编码）、`fill_lwp_common`/`fill_lwp_kern`/`fill_lwp_user`、`mib_kern_lwp`（pid/elsz/elmax 语义） | `proc.c:224-595` | `proc/lwp.rs` | 无 | **新增** |
| 10 | 18 | `18-mib-proc2.md` | `fill_proc2_common`/`fill_proc2_kern`/`fill_proc2_user`、`mib_kern_proc2`（KERN_PROC_ALL/PID/SESSION/PGRP/TTY/UID/RUID/GID/RGID 过滤） | `proc.c:596-917` | `proc/proc2.rs` | 无 | **新增** |
| 10 | 19 | `19-mib-proc-args.md` | `mib_kern_proc_args`：ps_strings 读取、ARGV/ENV/NARGV/NENV、页游走 + copybudget 上限、截断语义 | `proc.c:918-1176` | `proc/proc_args.rs` | 无 | **新增** |
| 10 | 20 | `20-mib-minix-proc.md` | `mib_minix_proc_list`（PROC_LIST 全表）+ `mib_minix_proc_data`（PROC_DATA 单 PID，ProcFS 语义：负 PID=内核任务） | `proc.c:1177-1288` | `proc/minix_proc.rs` | 无 | **新增** |
| 11 客户端契约 | 21 | `21-mib-client-libc.md` | `sysctl(3)`/`__sysctl`/`sysctlgetmibinfo`/`sysctlbyname`/`sysctlnametomib`、CTL_USER 本地子树、ENOMEM 旧长度回写约定 | `lib/libc/gen/sysctl.c`（391 行）等 5 文件 + `minix/lib/libc/sys/__sysctl.c` | 用户态 libc（**不实现，外部契约 A-10**） | 无 | **新增** |
| 11 | 22 | `22-mib-rmib-client.md` | `libsys/rmib.c` + `rmib.h`：rmib 节点/树、`rmib_register`/`deregister`/`reregister`/`rmib_process`/`rmib_call`、稀疏节点、COMMON_MIB_* 请求处理 | `minix/lib/libsys/rmib.c`（1089 行）、`minix/include/minix/rmib.h` | `minix-sys`（**RMIB 客户端模块，A-8**） | 无 | **新增** |
| 99 全局概念 | 99 | `99-mib-global-concepts.md` | `MIB_PROC_NR=7`、CTL_*/CTLTYPE_*/CTLFLAG_* 常量总表、错误码汇总、跨服务引用（kernel/PM/VFS/VM/DS/ProcFS/IPC/LWIP/UDS） | `com.h`、`sysctl.h` 两层 | `minix-types` | `draft/README.md` | **新增** |

### 2.1 阶段间的叙事衔接

每个阶段结尾的文档需含"过渡"小节（参照 `01-stage-kernel` 每篇 §6"过渡"），说明本阶段在启动时序/主循环中的位置与下一阶段的入口：

```
00（总览）→ 01（启动入口）→ 02（协议面）→ 03~05（树模型/查找）→ 06/07（拷贝/权限原语）
→ 08（动态节点）→ 09（数据读写）→ 10/11（分发/枚举）→ 12（远程子树）
→ 13~15（子系统子树）→ 16~20（进程信息）→ 21/22（客户端契约）
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

1. **概念首次出现即完整解释**——后续只引用不复述
2. **禁止前向引用**——依赖机制前置（本计划的编号已保证：协议面 02 先于 handler 09~20，树模型 03~05 先于动态节点 08，分发 10 先于子系统 handler 13~20）
3. **每篇一个语义单元**——读者可独立阅读
4. **位置可回答性**——每篇回答"它在 mib_init / 主循环 dispatch 的哪个位置"

### 3.3 引用规则

- 各文档之间用新编号交叉引用（如 `10-mib-dispatch.md` §远程续走 → `12-mib-remote-subtrees.md`）
- 与 kernel 文档交叉引用用 `../01-stage-kernel/NN-*.md`（boot 抑制机制 → `09-vm-boot-protocol.md`；IPC 原语 → `12-ipc-core.md`/`13-syscall-dispatch.md`；safecopy/grant → `18-syscall-copy.md`；时钟 → `15-clock-timer.md`）
- 与 VM 文档交叉引用用 `../02-stage-vm/NN-*.md`（`vm_info_stats`/`vm_info_usage` → `15-ipc-dispatch.md`/`26-vm-queries.md`）
- 与 RS/DS 文档交叉引用用 `../03-stage-rs/NN-*.md`、`../07-stage-ds/NN-*.md`（DS label → `08-ds-retrieve.md`；SEF → RS 对应篇）
- 对 draft 素材的引用一律指向 `draft/`，并标注"素材"；正式文档不引用 review 产物

### 3.4 每篇文档边界声明（执行级）

> 写作每篇文档前，先按本表声明**前置依赖 / 本篇职责 / 不覆盖（移交）**，防止内容交叉与遗漏。边界以 §5.3 函数清单为准绳。

| 编号 | 前置依赖 | 本篇职责（核心语义） | 不覆盖（移交） |
|------|---------|---------------------|--------------|
| 00 | 无 | 总览、启动主线图、sysctl 次主线、文档导航、设计原则 | 一切机制细节（01~22、99） |
| 01 | 00 + kernel `12-ipc-core`/`09-vm-boot-protocol` | `main`/`mib_startup`/`mib_sysctl`（消息解码 + ENOMEM 收尾）、主循环分类骨架、notify 拒绝、EDONTREPLY | SEF init 回调体（04）、分发核心（10）、各 handler（09/13~20） |
| 02 | 01 | call numbers、6 种消息结构字段级语义、`sysctlnode`/`sysctldesc` 交换格式、SYSCTL_VERSION 约束、errno 特殊语义（ENOMEM 不返回/EEXIST 带 oldlen） | 消息解码流程（01）、拷贝原语实现（06）、树语义（03~20） |
| 03 | 02 | `mib_node`/`mib_dynode` 全字段、4 类节点矩阵、CTLTYPE/CTLFLAG 全集与掩码、计数、scratch 缓冲、A-2（Rust 节点建模） | 查找（05）、静态树定义（04）、远程字段用途（12） |
| 04 | 03 + 01 | `mib_root`/`mib_table`/7 顶层节点、`MIB_*` 宏、`mib_init` 回调、`mib_tree_init`/`mib_tree_recurse`、版本初始化、重启语义 | 子树内部节点表（13~15）、endpts 表（12） |
| 05 | 03 | `mib_find`（静态 O(1) + 动态有序链表 O(n)） | 名字解析主循环（10）、动态插入（08） |
| 06 | 02 | 全部拷贝/长度/relay 原语、sys_datacopy vs grant 传输模型 | 消息解码（01）、远程 relay 调用面（12） |
| 07 | 02 | `mib_authed` 缓存语义、PRIVATE/ANYWRITE/READWRITE/PERMANENT 位语义、特权模型 | 具体节点的写校验调用点（09/10） |
| 08 | 03/05/06/07 | `check_name`/`scan`/`create`/`add`/`remove`/`destroy`/`upgrade`、动态 id、版本、内存所有权 | 数据读写（09）、枚举（11） |
| 09 | 03/06/07 | `getptr`/`read`/`write`/`readwrite`、字符串语义、临时缓冲、verify、bool 消毒 | 分发（10）、函数节点 handler（13~20） |
| 10 | 03/05/07/09 + 12（远程续走） | `mib_dispatch` 解析循环、元标识符 switch、三类节点判定、ENOTDIR/EISDIR、写权限检查、ERESTART 续走、**sysctl 次主线路径图** | 枚举/描述实现（11）、远程协议（12） |
| 11 | 03/06/07 | `mib_query`/`mib_describe`/`mib_copyout_node`/`mib_copyout_desc`、版本校验、私有节点过滤、描述设置路径 | 分发控制流（10） |
| 12 | 02/03/06 + DS `08` | endpts 表、register/deregister、mount/unmount 挂载策略、remote_info/remote_call relay、死亡检测、ERESTART 语义、**远程子树次主线路径图** | 客户端 rmib（22）、分发续走调用点（10） |
| 13 | 03/06/07/09 + 10 | CTL_KERN 全部函数/verify/数据节点 + 依赖原语（sys_getcputicks/getsysinfo/PMGETPARAM/cpuavg） | 其余子树（14/15）、进程信息（16~20） |
| 14 | 03/06 + VM `26` | CTL_VM + CTL_HW 子树、vm_info_stats/usage 依赖 | kern 子树（13）、minix 子树（15） |
| 15 | 03/06 + test87 | CTL_MINIX 子树（test/mib/proc 表）、MINIX_TEST_SUBTREE 门控、测试契约 | 进程信息实现（16~20） |
| 16 | 02 + kernel `06`/PM/VFS 表布局 | `update_tables`（节流/失败闩锁/magic）、proc_tab/mproc_tab/fproc_tab、PID 哈希、`get_mslot`/`ticks_to_timeval`/`fill_wmesg` | LWP/PROC2/ARGS 具体填充（17~19） |
| 17 | 16 | `get_lwp_stat` 状态机 + wchan 6 类编码、`fill_lwp_*`、`mib_kern_lwp` | 其他 PROC 接口（18~20） |
| 18 | 16 | `fill_proc2_*`、`mib_kern_proc2` 过滤面 | LWP（17）、ARGS（19） |
| 19 | 16 + VM/内核拷贝能力 | `mib_kern_proc_args` 页游走 + copybudget + 截断 | PROC2（18） |
| 20 | 16 + ProcFS `fs/procfs/` | `mib_minix_proc_list`/`mib_minix_proc_data`、ProcFS 消费者契约（A-5） | 其他 PROC 接口（17~19） |
| 21 | 02 + 全部服务器语义 | libc sysctl(3) 系列 + CTL_USER + `__sysctl` 回写约定、sysctlgetmibinfo 排序 | MIB 服务器内部（03~20）、rmib（22） |
| 22 | 02 + 12 | `rmib.c`/`rmib.h` 全部 API、稀疏节点、注册/注销/重注册、请求处理 | MIB 服务器侧协议（12） |
| 99 | 全部 | `MIB_PROC_NR`、常量表（CTL_/CTLTYPE_/CTLFLAG_/SYSCTL_）、错误码汇总、跨服务引用 | 各机制细节 |

### 3.5 测试基线

> `os/servers/mib/` 当前为 stub（`lib.rs: pub fn init() {}`，`main.rs: minix_mib::init(); loop {}`）。`cargo check -p minix-mib` 通过（2026-08-16，0 errors / 28 pre-existing warnings 来自 `minix-sys` 依赖），**无任何测试**。每篇文档 §测试 的"测试总数"声明以此为基线。C 侧行为契约以 `minix3/minix/tests/test87.c`（3662 行，root 运行）为主、`minix3/minix/tests/rmibtest/rmibtest.c`（267 行）与 `minix3/tests/kernel/t_sysctl.c`（74 行）为辅，各 handler 篇据此声明"测试要点"。

---

## 4. 架构演进（ARCH）清单

> 按 review-core 要求，ARCH 必须三处一致标注：Minix3 行为对照点 + design doc + 代码注释。以下为 10-stage-mib 范围内的 ARCH 项，写文档时必须逐项落实。

| # | ARCH 项 | Minix3 现状 | minix-rs 演进 | 涉及新文档 | 状态 |
|---|---------|------------|--------------|-----------|------|
| A-1 | **MIB 消息类型** | `mess_lc_mib_sysctl`/`mess_mib_lc_sysctl`/`mess_lsys_mib_register`/`mess_mib_lsys_call`/`mess_mib_lsys_info`/`mess_lsys_mib_reply`（`ipc.h`，字段含 `int name[CTL_SHORTNAME=8]`，全部为 32/64 位标量，56 字节消息内可容纳） | `minix-types` **尚无 mib.rs 消息类型**，需新增（参照 `vm.rs` In/Out 语义层模式 + `DecodeFromM1`/`EncodeToM1`） | 02 | **缺口**：minix-types 新增 |
| A-2 | **节点数据结构** | `struct mib_node` 巨型 union（child/remote/立即值三态）+ 4 类节点（PARENT×REMOTE 矩阵）+ 动态子节点**按 id 排序链表** + `struct mib_dynode` 内嵌 name/data/desc | Rust enum 建模：`NodeKind::{Static, Data, Func, Remote}` + `ChildMap`（静态数组→`Vec`/固定数组，链表→`BTreeMap`/`Vec`+索引）；`#[repr(C)]` 仅保留给交换格式（A-4），内部布局可自由演进 | 03/05/08/12 | 待设计 |
| A-3 | **动态内存分配** | `malloc`/`free`：dynode 单块内嵌 name+data、`strdup` 描述（OWNDESC）、写大数据的临时缓冲（`newlen+1 > SCRATCH_SIZE` 才 malloc，且非特权用户 EPERM）、分配失败一律**不返回 ENOMEM**（改 EINVAL） | `no_std` 分配策略：全局分配器 or 专用 `MibAllocator`（cap 上限）；scratch 等价物 = 栈/静态缓冲（对齐 int32） | 08/09/03 | 待设计 |
| A-4 | **交换格式布局 ABI** | `sysctlnode`/`sysctldesc`（NetBSD `SYSCTL_VERSION=VERS_1`，`sysctl.h:1382-1450`）+ `kinfo_lwp`/`kinfo_proc2`/`kinfo_drivers`/`clockinfo`/`loadavg`/`uvmexp_sysctl`/`ps_strings` 结构布局，用户态 ps(1)/top(1)/sysctl(8)/libkvm/test87 直接解释 | 保持 `#[repr(C)]` 等价布局（同 A-5 方案），SYSCTL_VERSION 必须=VERS_1；内部实现可偏离，交换面不可 | 02/11/13/17/18/19 | **待决策**（保持 vs 偏离，两案均需三处一致标注） |
| A-5 | **minix_proc_list/minix_proc_data 布局** | `minix/sysctl.h:61-86` 定义，**ProcFS 直接消费**（`fs/procfs/tree.c:87-92` PROC_LIST、`pid.c:42-48,156,183` PROC_DATA）；负 PID = 内核任务（ProcFS 语义，与 KERN_LWP 的 -1 语义不同） | 保持 `#[repr(C)]` 布局则 ProcFS 无需改动；否则声明偏离 + ProcFS 侧同步改造 | 02/20 | **待决策**（同 A-4 一并决策） |
| A-6 | **进程表快照** | `proc_tab[NR_TASKS+NR_PROCS]`/`mproc_tab[NR_PROCS]`/`fproc_tab[NR_PROCS]` 静态数组，`sys_getproctab` + `getsysinfo(SI_PROC_TAB/SI_PROCLIGHT_TAB)` 跨服务器整表拷贝，magic 校验（PMAGIC/MP_MAGIC），每 clock tick 至多一次 + 失败闩锁 | 依赖 kernel/PM/VFS 重写后的表布局契约（跨 stage）；Rust 侧 `ArrayVec`/`Vec` 快照 + 时间戳节流；magic 校验在类型化接口下可弱化（ARCH 标注） | 16 | 待设计（跨 stage 契约） |
| A-7 | **SEF / Live Update** | `sef_startup` + `sef_setcb_init_fresh(mib_init)` + `sef_setcb_init_restart(mib_init)`：重启**丢失全部动态状态**（动态节点），仅保留静态树（`main.c:415-431` 注释）；无 magic 插桩依赖 | 与 `03-stage-rs`/`07-stage-ds` 的 `sef.rs` 对齐；重启语义 = 静态树重建 + 动态节点清空（不是 STATEFUL） | 01/04 | 参照先例 |
| A-8 | **RMIB 客户端归属** | `libsys/rmib.c` 全部 `rmib_*` API（`asynsend3` 注册 + `COMMON_MIB_*` 请求处理 + `sys_safecopyto`/`sys_vsafecopy` grants） | `minix-sys` 新增 RMIB 客户端模块（当前 `minix-sys` 是 stub）；或先交付纯函数层（消息构造/解析/rmib 树遍历） | 22 | 待实施（依赖 IPC 落地） |
| A-9 | **死/未实现项排除契约** | `CTL_CREATESYM`/`CTL_MMAP` → `EOPNOTSUPP`；`kern.c` 84 槽位中大量 "not yet supported"（KERN_PROC/KERN_FILE/KERN_MBUF 等 40+ 槽）；`mib_kern_profiling` → `EOPNOTSUPP`；`vm.c` VM_METER/UVMEXP/NKMEMPAGES/ANONMIN 等未实现；`hw.c` HW_MODEL/DISKNAMES/IOSTATS 等未实现；`mib_kern_ipc_info` 为 mock（被 IPC 服务覆盖时才不可见）；KERN_PROC_SESSION 无 job control、KERN_PROC_TTY_REVOKE 未支持 | 不实现，标注排除 + 语义契约（表槽位保留，访问返回 ENOENT/对应错误） | 02/13/14/15 | 排除（grep 实证，见 §5.5） |
| A-10 | **libc 客户端（外部契约）** | `sysctl(3)` 在 libc 内处理 CTL_USER 子树（`libc/gen/sysctl.c:61-391`），其余转发 `__sysctl`；`sysctlgetmibinfo`（611 行）依赖 QUERY/DESCRIBE 元标识符 + 自排序 | minix-rs 不重写 libc：作为**外部契约文档**（消息面 + 行为约定），保证 libc 侧兼容 | 21 | 外部契约（不实现） |
| A-11 | **错误码特殊语义** | `ENOMEM` 特殊含义：sysctl(2) 数据溢出 → 部分拷贝 + 完整长度 + ENOMEM（`main.c:341-356`）；分配失败**禁止**返回 ENOMEM（`tree.c:589,1043,1207`）；create 冲突 EEXIST 时 copyout 现有节点 + `call_reslen`；`EDONTREPLY` 抑制回复；`ERESTART` 为远程内部信号 | `minix_types::Errno` 已覆盖全集（EPERM..ENOSYS + ERESTART=200 + EDONTREPLY=203，见 `errno.rs`）；Rust 侧需建模"错误 + 附带 oldlen"（`Result<usize, (Errno, Option<usize>)>` 或等价） | 01/02/09/99 | 已具备（类型建模待写） |
| A-12 | **外部依赖原语** | `sys_datacopy`/`sys_safecopyto`/`sys_vsafecopy`/`cpf_grant_*`（kernel copy/grant）、`getticks`/`sys_hz`/`sys_getcputicks`/`sys_getmachine`/`getuptime`/`cpuavg_get*`（libsys）、`getsysinfo`（PM/VFS）、`svrctl(PMGETPARAM)`、`getnuid`（PM）、`ds_retrieve_label_name`（DS）、`vm_info_stats`/`vm_info_usage`（libvmclient） | `minix-sys` 对应原语落地（多跨 stage 依赖）；写文档时标注"依赖 XX-stage 提供" | 06/12/13/14/16/19/99 | 待实施（依赖链） |

---

## 5. 覆盖完整性核对（对照 minix3 源码）

### 5.1 C 源文件 → 新文档映射（8 个 .c）

| C 源文件 | 行数 | 覆盖文档 | 核对 |
|---------|------|---------|------|
| `servers/mib/main.c` | 492 | 01（main/主循环/mib_sysctl）、04（mib_init 回调）、06（拷贝原语）、07（auth）、02（消息面引用） | 已核对 |
| `servers/mib/tree.c` | 1842 | 04（tree_init）、05（find）、06（copyin_str）、07（权限检查点）、08（动态节点）、09（读写）、10（dispatch）、11（query/describe）、12（mount/unmount） | 已核对 |
| `servers/mib/remote.c` | 477 | 12（全部） | 已核对 |
| `servers/mib/kern.c` | 508 | 13（全部） | 已核对 |
| `servers/mib/proc.c` | 1288 | 16（表快照）、17（LWP）、18（PROC2）、19（PROC_ARGS）、20（MINIX_PROC） | 已核对 |
| `servers/mib/hw.c` | 140 | 14（CTL_HW 部分） | 已核对 |
| `servers/mib/minix.c` | 89 | 15（全部） | 已核对 |
| `servers/mib/vm.c` | 154 | 14（CTL_VM 部分） | 已核对 |

### 5.2 头文件/协议面覆盖

| 头文件 | 覆盖文档 | 核对 |
|--------|---------|------|
| `minix/include/minix/com.h:613-622,1026-1028`（MIB_BASE 0x1800、MIB_SYSCTL/REGISTER/DEREGISTER、NR_MIB_CALLS=3、IS_MIB_CALL、COMMON_MIB_INFO/CALL/REPLY） | 02/12/22 | 已核对 |
| `minix/include/minix/ipc.h`（6 种 mess_mib_* 结构） | 02 | 已核对 |
| `sys/sys/sysctl.h`（CTL_* 顶层 id、CTLTYPE_*/CTLFLAG_*/SYSCTL_* 宏、sysctlnode/sysctldesc、元标识符、KERN_* 全集） | 02/03/11/13/17/18/19 | 已核对 |
| `minix/include/minix/sysctl.h`（CTL_MINIX、MINIX_*/TEST_* id、minix_proc_* 结构、__sysctl） | 02/15/20 | 已核对 |
| `minix/include/minix/rmib.h` + `minix/lib/libsys/rmib.c`（RMIB 客户端） | 22 | 已核对 |
| `lib/libc/gen/sysctl.c` + `sysctlgetmibinfo.c` + `sysctlbyname.c` + `sysctlnametomib.c` + `minix/lib/libc/sys/__sysctl.c`（libc 客户端） | 21 | 已核对 |
| `servers/mib/mib.h`（内部结构/宏/原型） | 03/04/02（原型面） | 已核对 |

### 5.3 函数/符号清单映射

> 以下为 `servers/mib/` 全部顶层定义（82 函数 + 13 静态表 + 3 全局计数，grep 实证见 §7.2），逐一落入新文档。行号以 2026-08-16 工作区为准。

**main.c（14 函数 + mib_root/mib_table 数据）**

| 符号 | 行 | 覆盖文档 |
|------|----|---------|
| `mib_inrange`/`mib_getoldlen`/`mib_copyout`/`mib_setoldlen`/`mib_getnewlen`/`mib_copyin`/`mib_copyin_aux`/`mib_relay_oldp`/`mib_relay_newp` | 90-258 | 06 |
| `mib_authed` | 259-275 | 07 |
| `mib_sysctl` | 277-383 | 01 |
| `mib_init` | 384-413 | 04 |
| `mib_startup` | 415-431 | 01 |
| `main` | 433-491 | 01 |
| `mib_root`/`mib_table[]` | 36-64 | 04 |

**tree.c（22 函数 + scratch/mib_nodes/mib_objects/mib_remotes）**

| 符号 | 行 | 覆盖文档 |
|------|----|---------|
| `mib_find` | 34-88 | 05 |
| `mib_copyout_node`/`mib_copyout_desc` | 90-174,918-967 | 11 |
| `mib_query` | 179-241 | 11 |
| `mib_check_name`/`mib_scan` | 247-360 | 08 |
| `mib_copyin_str` | 371-421 | 06 |
| `mib_upgrade`/`mib_add` | 426-481 | 08 |
| `mib_create` | 486-777 | 08 |
| `mib_remove`/`mib_destroy` | 784-917 | 08 |
| `mib_describe` | 972-1091 | 11 |
| `mib_getptr`/`mib_read`/`mib_write`/`mib_readwrite` | 1098-1330 | 09 |
| `mib_dispatch` | 1332-1475 | 10 |
| `mib_tree_recurse`/`mib_tree_init` | 1479-1536 | 04 |
| `mib_mount`/`mib_unmount` | 1543-1842 | 12 |

**remote.c（9 函数）**

| 符号 | 行 | 覆盖文档 |
|------|----|---------|
| `mib_remote_init`/`mib_down`/`mib_get_label`/`mib_do_register`/`mib_register`/`mib_do_deregister`/`mib_deregister`/`mib_remote_info`/`mib_remote_call` | 40-477 | 12 |

**kern.c（13 函数 + 2 表）**

| 符号 | 行 | 覆盖文档 |
|------|----|---------|
| `mib_kern_securelvl`/`mib_kern_forkfsleep`（verify） | 17-33,208-220 | 13 |
| `mib_kern_clockrate`/`mib_kern_profiling`/`mib_kern_hardclock_ticks`/`mib_kern_root_device`/`mib_kern_ccpu`/`mib_kern_cp_time`/`mib_kern_consdev`/`mib_kern_drivers`/`mib_kern_boottime`/`mib_kern_ipc_info` | 35-315 | 13 |
| `mib_kern_ipc_table`/`mib_kern_table`/`mib_kern_init` | 316-508 | 13 |

**proc.c（16 函数 + 3 表）**

| 符号 | 行 | 覆盖文档 |
|------|----|---------|
| `update_tables`/`get_mslot`/`ticks_to_timeval`/`fill_wmesg` | 46-217 | 16 |
| `get_lwp_stat`/`fill_lwp_common`/`fill_lwp_kern`/`fill_lwp_user`/`mib_kern_lwp` | 224-595 | 17 |
| `fill_proc2_common`/`fill_proc2_kern`/`fill_proc2_user`/`mib_kern_proc2` | 600-917 | 18 |
| `mib_kern_proc_args` | 918-1176 | 19 |
| `mib_minix_proc_list`/`mib_minix_proc_data` | 1177-1288 | 20 |
| `proc_tab`/`mproc_tab`/`fproc_tab` | 34-38 | 16 |

**hw.c（4 函数 + 1 表）/ vm.c（3 函数 + 1 表）/ minix.c（1 函数 + 5 表）**

| 符号 | 行 | 覆盖文档 |
|------|----|---------|
| `mib_hw_physmem`/`mib_hw_usermem`/`mib_hw_ncpuonline`/`mib_hw_table`/`mib_hw_init` | 18-140 | 14 |
| `mib_vm_loadavg`/`mib_vm_uvmexp2`/`mib_vm_table`/`mib_vm_init` | 20-154 | 14 |
| `mib_minix_test_*`/`mib_minix_mib_table`/`mib_minix_proc_table`/`mib_minix_table`/`mib_minix_init` | 14-89 | 15 |

### 5.4 外部消费者与跨服务契约

| 消费者 | 位置 | 消费内容 | 覆盖文档 |
|--------|------|---------|---------|
| ProcFS | `minix/fs/procfs/tree.c:87-92`、`pid.c:42-48,156,183` | `MINIX_PROC` PROC_LIST/PROC_DATA（布局 ABI，A-5） | 20/02 |
| IPC server | `servers/ipc/main.c:27-112,249` | `kern.ipc` 远程子树（RMIB，覆盖 MIB 的 mock） | 12/22 |
| LWIP | `net/lwip/mibtree.c:42-66` | `net.inet`/`net.inet6`/`minix.lwip` 远程子树 | 12/22 |
| UDS | `net/uds/stat.c:78-147`、`uds.c` | `net.local` 远程子树 | 12/22 |
| libc sysctl(3) | `lib/libc/gen/sysctl.c` 等 5 文件 | 消息面 + CTL_USER + ENOMEM 回写（A-10） | 21 |
| test87 | `minix/tests/test87.c`（3662 行） | MIB 全行为契约（类型/权限/元标识符/远程/动态节点） | 各 handler 篇 + 15 |
| rmibtest | `minix/tests/rmibtest/rmibtest.c`（267 行）+ `testrmib.sh` | RMIB 客户端/服务器行为契约 | 12/22 |
| t_sysctl | `minix3/tests/kernel/t_sysctl.c`（74 行）+ `tests/sbin/sysctl/t_sysctl.sh` | sysctl(2) 基础面 | 01/21 |
| 用户态工具 | ps(1)/top(1)/sysctl(8)/libkvm | `kinfo_lwp`/`kinfo_proc2`/`kinfo_drivers` 等布局（A-4） | 17/18/13 |

### 5.5 明确排除 / 跳过的项

| 项 | 处理 | 依据 |
|----|------|------|
| `CTL_CREATESYM`/`CTL_MMAP` 元标识符 | 不实现，dispatch 返回 `EOPNOTSUPP`，标注 A-9 | `tree.c:1437-1441` |
| `KERN_PROC`/`KERN_FILE`（kern.c 注释槽位） | 不实现，表槽位保留 + 注释 | `kern.c:361-363` 等 |
| kern.c/vm.c/hw.c 全部 "not yet supported" 槽位（40+） | 不实现，A-9 排除契约 | `kern.c` 表注释、`vm.c:118-130`、`hw.c:57-116` |
| `mib_kern_profiling` | 保持 `EOPNOTSUPP` 行为（MINIX3 有独立 profiling API） | `kern.c:63-74` |
| KERN_PROC_SESSION job control / KERN_PROC_TTY_REVOKE | 保持 TODO 现状（`arg != mp_procgrp` / `continue`） | `proc.c:826-844` |
| `mib_kern_ipc_info` mock | 保留为 fallback（IPC 服务在线时被远程子树覆盖） | `kern.c:306-330` |
| CTL_USER 子树 | 属于 libc 内部（不进 MIB），外部契约文档 21 描述 | `libc/gen/sysctl.c:80-86` |
| `Makefile`（构建/链接脚本） | 非语义，WONTFIX | — |
| libc 侧 `__cvt_node_out`/`sysctlgetmibinfo` 内部排序 | 并入 21 行为契约（不单独立文档） | 组织原则 |

### 5.6 覆盖结论与拆分答案

**1. 是否确保全部覆盖？** 是。§5.1~§5.4 已逐层核对：8 个 .c（4990 行）全部映射到新文档（§5.1）；5 个协议/客户端头文件 + libc/RMIB 客户端库进入覆盖契约（§5.2）；82 个顶层函数 + 13 静态表 + 3 全局计数逐一落入 01~20（§5.3）；6 类外部消费者 + 5 组测试契约进入 21/22 与各 handler 篇（§5.4）。§7.2 以命令证据复核，无遗漏。架构演进项（A-1~A-12，含 directMap 同类的交换格式 ABI A-4/A-5 与死 API 排除 A-9）全部单列，不混入行为文档。

**2. 计划拆分为多少个文档，简述如何拆分？** **24 篇**：`00` 总览 + `01~22` 语义模块 + `99` 全局概念，按 **12 个阶段**组织——阶段 1 启动入口（01）→ 阶段 2 协议面（02）→ 阶段 3 树模型与查找（03~05）→ 阶段 4 拷贝/权限原语（06/07）→ 阶段 5 动态节点（08）→ 阶段 6 数据读写（09）→ 阶段 7 分发与元标识符（10/11）→ 阶段 8 远程子树（12）→ 阶段 9 子系统子树（13~15，按 init 顺序 kern→vm+hw→minix）→ 阶段 10 进程信息（16~20，按依赖序表→LWP→PROC2→ARGS→MINIX_PROC）→ 阶段 11 客户端契约（21/22）。拆分原则：每篇一个语义单元 + 位置可回答性 + 禁止前向引用；以函数清单（§5.3）为唯一边界准绳。

---

## 6. 实施路线

> 每篇新文档 = 依据 §3 讲述结构 + §4 ARCH 标注 + §5 覆盖契约从零写作。所有 P0 修复完成后才可推进下一篇。当前 10-stage-mib 无历史主线素材（仅占位 README），全部为新建。

1. **00-mib-overview 新建**（导航，含 §1.2 启动时序图 + sysctl 次主线）
2. **01-mib-init-main 新建**（主循环 + 消息解码 + ENOMEM 语义）
3. **02-mib-message-contract 新建**（协议面，A-1 缺口标注）
4. **03~05 树模型三篇新建**（A-2/A-3 标注；05 依赖 03）
5. **06/07 原语两篇新建**（拷贝 + 权限，A-11/A-12 标注）
6. **08~11 动态节点/读写/分发/枚举四篇新建**（sysctl 次主线路径图入 10）
7. **12-mib-remote-subtrees 新建**（远程子树次主线路径图）
8. **13~15 子系统子树三篇新建**（A-9 排除契约）
9. **16~20 进程信息五篇新建**（A-4/A-5/A-6 标注）
10. **21/22 客户端契约两篇新建**（A-8/A-10 标注）
11. **99-mib-global-concepts 新建**（常量/错误码/跨服务引用收口）
12. **README.md 重建**（文档清单 + 启动链路位置，参照 09-stage-init/README.md 模式）

### 6.1 文档写作状态跟踪

> 每完成一篇，将状态改为 `reviewed`（附 scan 日期）。全部 `reviewed` 且无 P0 遗留 = 阶段完成。

| 编号 | 状态 | 首轮 review 日期 | 备注 |
|------|------|-----------------|------|
| 00 | pending | — | 新建导航 |
| 01 | reviewed（2026-09-04） | 2026-09-04 | 新建（dispatch.rs verdict 层 9 tests + com.rs MIB 号段，scan CONVERGED） |
| 02 | reviewed（2026-09-04） | 2026-09-04 | 新建协议面（A-1 兑现，13 tests，scan CONVERGED） |
| 03 | reviewed（2026-09-04） | 2026-09-04 | 新建（A-2 兑现，9 tests，scan CONVERGED） |
| 04 | reviewed（2026-09-04） | 2026-09-04 | 新建（6 tests，scan CONVERGED；arena 缺口→13） |
| 05 | reviewed（2026-09-04） | 2026-09-04 | 新建（3 tests，scan CONVERGED，零自修） |
| 06 | reviewed（2026-09-04） | 2026-09-04 | 新建（7 tests，scan CONVERGED；执行→transport） |
| 07 | reviewed（2026-09-04） | 2026-09-04 | 新建（4 tests，scan CONVERGED） |
| 08 | reviewed（2026-09-04） | 2026-09-04 | 新建（10 tests，scan CONVERGED；arena→后续） |
| 09 | reviewed（2026-09-04） | 2026-09-04 | 新建（7 tests，scan CONVERGED） |
| 10 | reviewed（2026-09-04） | 2026-09-04 | 新建 + 次主线路径图（5 tests，scan CONVERGED） |
| 11 | reviewed（2026-09-04） | 2026-09-04 | 新建（7 tests，scan CONVERGED） |
| 12 | reviewed（2026-09-04） | 2026-09-04 | 新建 + 远程次主线（7 tests，scan CONVERGED） |
| 13 | reviewed（2026-09-05） | 2026-09-05 | 教学重写（185行+Redox对照；OQ-13-1闭环为声明；快照v2） |
| 14 | reviewed（2026-09-05） | 2026-09-05 | 教学重写（167行+算例；MACH ARCH行；快照v2） |
| 15 | reviewed（2026-09-05） | 2026-09-05 | 教学重写（164行；arena收官声明；快照v2） |
| 16 | reviewed（2026-09-05） | 2026-09-05 | 白话重写（代码不动；快照v2） |
| 17 | reviewed（2026-09-05） | 2026-09-05 | 白话重写（代码不动；快照v2） |
| 18 | reviewed（2026-09-05） | 2026-09-05 | 新建文档+Rust proc2.rs（7测试+值表；快照v1嵌入生成） |
| 19 | reviewed（2026-09-05） | 2026-09-05 | 新建文档+Rust proc_args.rs（3测试；快照v1嵌入生成） |
| 20 | reviewed（2026-09-05） | 2026-09-05 | 新建文档+Rust minix_proc.rs（3测试+排布值表；快照v1嵌入生成） |
| 21 | reviewed（2026-09-05） | 2026-09-05 | 新建外部契约文档（不实现；快照v1嵌入生成） |
| 22 | reviewed（2026-09-05） | 2026-09-05 | 新建文档+Rust rmib.rs（minix-sys，3测试；快照v1嵌入生成） |
| 99 | pending | — | 新建全局概念 |

---

## 7. Review 记录

### 7.1 深度 review（2026-08-16）

**方法**：按 review-process Step 1-5 对计划本身做语义全覆盖审计——逐函数/逐消息/逐标志/逐消费者核对 §2 文档拆分能否承载全部语义，检查模块边界是否重叠或漏项。

**发现的问题与修复**：

| # | 问题 | 级别 | 修复 |
|---|------|------|------|
| M-1 | 初版将 `mib_copyout_node`/`mib_copyout_desc`（序列化）与 `mib_query`/`mib_describe`（枚举）合并进 10-mib-dispatch，导致 dispatch 语义单元过载（解析 + 枚举 + 描述三件事） | P1 | 拆出 `11-mib-query-describe` 为独立"节点枚举与描述"单元（§2/§5.3 归位） |
| M-2 | 初版漏掉 `mib_sysctl` 的 **ENOMEM 特殊语义**（部分拷贝 + 完整长度 + ENOMEM）与 create 冲突 EEXIST 携带 oldlen 的细节 | P1 | 补入 01 职责 + A-11（§2/§4） |
| M-3 | 初版未识别 **`mib_mount` 是 tree.c 而非 remote.c 的函数**（挂载点创建/覆盖逻辑在 tree.c:1543-1842），原计划全部划给 remote 篇 | P1 | 12 职责明确含 `mib_mount`/`mib_unmount`（§2/§5.3 行号修正） |
| M-4 | 初版未识别 **`proc.c` 的 ProcFS 语义**（负 PID = 内核任务，与 KERN_LWP 的 -1 全列表语义不同）与 `minix_proc_*` 布局 ABI 的 ProcFS 消费者 | P1 | 新增 A-5 + 20 职责（§4/§3.4/§5.4） |
| M-5 | 初版漏掉 `mib_remote_call` 的 **grant relay 细节**（name/oldp/newp 三 grant + 死亡判定 mib_down + ERESTART 续走 + ERESTART 交叉时 mib_do_deregister） | P1 | 补入 12 职责声明（§3.4） |
| M-6 | 初版漏掉 `update_tables` 的 **失败闩锁**（tabs_valid=FALSE 后不再重试）与 magic 校验（PMAGIC/MP_MAGIC） | P2 | 补入 16 职责（§3.4/§5.3） |
| M-7 | 初版漏掉 `mib_write` 的**非特权用户大数据写入 EPERM**（`newlen+1 > SCRATCH_SIZE` 时未授权返回 EPERM 而非分配） | P2 | 补入 09 + A-3（§3.4/§4） |
| M-8 | 初版未明确 `mib_register`/`mib_deregister` 的**单向消息约束**（SENDREC 返回 ENOSYS，避免与 MIB→服务请求交叉死锁） | P1 | 补入 02/12（§3.4/§5.3） |
| M-9 | 初版漏掉 `sysctl(3)` 的 **CTL_USER 本地子树**（libc 内部，不进 MIB）与 `__sysctl` 的 oldlen 回写约定 | P2 | 补入 21 + A-10（§2/§4/§5.5） |
| M-10 | 测试基线未声明（stub 无测试，`cargo check -p minix-mib` 通过；C 侧行为契约 = test87/rmibtest/t_sysctl） | P2 | 新增 §3.5 |

**结论**：修复后按 §5.3 函数清单反向核对——`servers/mib/` 全部 82 个顶层函数 + 13 静态表 + 3 全局计数逐一落入 01~20；协议面 5 头文件 + 客户端库（libc 5 文件 + rmib.c）落入 02/21/22；外部消费者（ProcFS/IPC/LWIP/UDS）与测试契约落入 12/20/21/22 与各 handler 篇。**语义全覆盖，无遗漏**。

### 7.2 minix3 源码回归 review（2026-08-16）

**方法**：对 `minix3/minix/servers/mib/` 全部 .c/.h + 协议头文件 + 客户端库 + 外部消费者逐一 grep 核对 §5.1/§5.2/§5.3 映射，并抽查主循环 dispatch 面与死声明。

**证据**：

```bash
wc -l minix3/minix/servers/mib/*.c          # 8 个 .c 合计 4990，与 §1 一致（main 492+tree 1842+remote 477+kern 508+proc 1288+hw 140+minix 89+vm 154）
rg -n "MIB_SYSCTL|MIB_REGISTER|MIB_DEREGISTER" minix3/minix/servers/mib/main.c   # switch 面 3 case + default，与 §1.2 一致
rg -n "case CTL_QUERY|case CTL_CREATE|case CTL_DESTROY|case CTL_DESCRIBE|case CTL_CREATESYM|case CTL_MMAP" minix3/minix/servers/mib/tree.c   # 元标识符 6 分支 + EOPNOTSUPP，与 §5.5 一致
rg -n "if \(miblen < 2\)" minix3/minix/servers/mib/tree.c   # 挂载策略：禁止顶层挂载（:1572），与 12 职责一致
rg -n "proc_tab\[|mproc_tab\[|fproc_tab\[" minix3/minix/servers/mib/proc.c | head -3   # 三表快照（:34-38），与 16 职责一致
rg -n "MINIX_PROC|PROC_LIST|PROC_DATA" minix3/minix/fs/procfs/tree.c minix3/minix/fs/procfs/pid.c   # ProcFS 消费者（tree.c:87-92, pid.c:42-48,156,183），A-5 成立
rg -n "rmib_register" minix3/minix/servers/ipc/main.c minix3/minix/net/lwip/mibtree.c minix3/minix/net/uds/stat.c   # RMIB 消费者三处，与 §5.4 一致
rg -n "not yet supported" minix3/minix/servers/mib/kern.c minix3/minix/servers/mib/vm.c minix3/minix/servers/mib/hw.c | wc -l   # 40+ 未实现槽位 → A-9 排除契约成立
sed -n '58,62p' minix3/minix/kernel/table.c + sed -n '265,267p' minix3/minix/kernel/main.c   # boot 证据（table.c:60 + RTS_VMINHIBIT）
rg -n "user_sysctl|CTL_USER" minix3/lib/libc/gen/sysctl.c | head -3   # CTL_USER 在 libc 本地处理（:80-86），A-10 成立
```

**结论**：8 个 .c 文件全部映射到新文档，无遗漏；82 个顶层函数逐一定位；5 个协议头文件 + libc/RMIB 客户端库 + 4 个外部消费者全部进入覆盖契约；A-1~A-12 与 minix3 现状对照成立。**覆盖完整性通过**。

---

## 8. 参见

- `draft/` — 旧占位 README（素材）
- `../00-master-plan/README.md` — 目录重排与新主线说明（MIB boot 地位）
- `../01-stage-kernel/00-kernel-overview.md` — 讲述结构参照（阶段划分/组织原则）
- `../02-stage-vm/plan.md` 与 `../07-stage-ds/plan.md` — 同流程先例（DS 亦为从零定义文档集）
- `minix3/minix/servers/mib/` — C 源码（ground truth）
- `minix3/sys/sys/sysctl.h` + `minix3/minix/include/minix/sysctl.h` + `com.h` + `ipc.h` — 协议面
- `minix3/minix/lib/libsys/rmib.c` + `minix3/lib/libc/gen/sysctl.c` — 客户端契约
- `os/servers/mib/` — Rust 实现（当前 stub）
