# 10-stage-mib 文档重建蓝图（qwen）

## 0. 元数据

- **执行者**：qwen
- **日期**：2026-09-19
- **目标目录**：`notes/rewrite/fork-syscall-rewrite/10-stage-mib/`
- **仓库根目录**：`/home/xzhao/github/minix-rs`
- **当前提交号**：`606e97607`
- **bagging 隔离声明**：本蓝图独立产出。未读取、未参考 `doc_rerank_deepseek.md`、`doc_rerank_glm.md`（同目录已存在，属其它 AI 产物）；未引用 `.design/` 与 `tmp_design_and_todo/`。所有落盘产物带 `_qwen` 后缀。

### 0.1 审查范围

- **算作文档**：编号文档 `00`~`22`（23 篇）+ `99-mib-global-concepts.md`（收口篇）。共 24 篇，`wc -l` 合计 3992 行。
- **算作参考材料**（不重建正文，只作线索）：`plan.md`（468 行）、`todo.md`（318 行）、`README.md`。
- **范围外**：其它 stage 目录、`minix3/` C 源码本体（只读不改）、`os/` Rust 实现（只核对不重写）。
- **范围外发现**：`grant.rs` 端点常量偏差（E-MIBGRANT）已在 `99` §1 与 `edge_todo.md` 登记，属 kernel 侧，移交对应 stage，本蓝图只在锚点/事实底线中引用，不新建篇章。

### 0.2 读取清单

- **全部编号文档**：`00`~`22`、`99`、`README`、`plan`、`todo` 逐篇读头部声明与正文骨架。
- **C 源码全量**：`minix3/minix/servers/mib/` 8 个 `.c`（main/tree/remote/kern/proc/hw/minix/vm）+ `mib.h`。逐文件用 `grep -nE '^[a-zA-Z_][a-zA-Z0-9_]*\('` 提取函数定义行，得到 82 个函数的权威定义位置（见 §0.4）。
- **非 C 制品**：`minix3/lib/libc/gen/sysctl*.c`、`minix3/minix/lib/libc/sys/__sysctl.c`、`minix3/minix/lib/libsys/rmib.c`、`minix3/minix/include/minix/rmib.h`、`sys/sys/sysctl.h`、`minix/include/minix/sysctl.h`、`com.h`。
- **边界材料**：`00-master-plan/README.md`（阶段划分与启动因果链）、`edge_todo.md`（E-RMIBWIRE / E-MIBPROD / E-MIBGRANT / E-DSWIRE / E-ISWIRE）、前一 stage `09-stage-init/00-init-overview.md`。
- **Rust 实现入口**：`os/servers/mib/src/`（41 个 `.rs`，11680 行）、`os/libs/minix-sys/src/rmib.rs`、`os/libs/minix-types/src/types/sysctl{,_abi}.rs`。
- **写法范例**：`01-stage-kernel/06-todo.md`（只学契约写法，不搬结论）。

### 0.3 使用的命令与关键输出（证据摘录）

```text
# C 函数定义清单（ground truth，逐文件）—— 共 82 个函数
main.c  : mib_inrange(91) mib_getoldlen(106) mib_copyout(121) mib_setoldlen(148)
          mib_getnewlen(159) mib_copyin(173) mib_copyin_aux(192) mib_relay_oldp(211)
          mib_relay_newp(236) mib_authed(260) mib_sysctl(278) mib_init(385)
          mib_startup(416) main(434)
tree.c  : mib_find(35) mib_copyout_node(91) mib_query(180) mib_check_name(248)
          mib_scan(278) mib_copyin_str(372) mib_upgrade(427) mib_add(457) mib_create(487)
          mib_remove(785) mib_destroy(839) mib_copyout_desc(926) mib_describe(973)
          mib_getptr(1098) mib_read(1131) mib_write(1160) mib_readwrite(1309)
          mib_dispatch(1333) mib_tree_recurse(1480) mib_tree_init(1520) mib_mount(1544)
          mib_unmount(1790)
remote.c: mib_remote_init(41) mib_down(56) mib_get_label(82) mib_do_register(110)
          mib_register(198) mib_do_deregister(240) mib_deregister(292) mib_remote_info(317)
          mib_remote_call(379)
kern.c  : securelvl(18) clockrate(36) profiling(64) hardclock_ticks(77) root_device(96)
          ccpu(120) cp_time(135) consdev(193) forkfsleep(209) drivers(223) boottime(287)
          ipc_info(307) mib_kern_init(504)
proc.c  : update_tables(47) get_mslot(144) ticks_to_timeval(164) fill_wmesg(186)
          get_lwp_stat(225) fill_lwp_common(399) fill_lwp_kern(462) fill_lwp_user(488)
          mib_kern_lwp(510) fill_proc2_common(601) fill_proc2_kern(658) fill_proc2_user(688)
          mib_kern_proc2(792) mib_kern_proc_args(919) mib_minix_proc_list(1178)
          mib_minix_proc_data(1217)
hw.c    : physmem(19) usermem(46) ncpuonline(82) mib_hw_init(136)
minix.c : mib_minix_init(85)   （其余为静态表 + #if MINIX_TEST_SUBTREE 测试子树）
vm.c    : loadavg(12) uvmexp2(79) mib_vm_init(150)

# Rust 侧存在但文档以"移交/占位/空壳"表述的执行层模块（关键发现，见 §3）
server.rs 876L  transport.rs 469L  walker.rs 1125L  heap.rs 190L  tree/arena.rs 611L  sef.rs 44L

# 锚点噪声统计
"工具生成" 锚点总数：131（分布 18 篇）
断链面：intra-stage 交叉引用 163，os/ 代码注释引用 44，跨 stage 真实文档引用 ~18

# 事实核对：doc 01 §3/§4.1 断言 "main 循环不搬进 Rust / main.rs 是占位空壳"
# 但 main.rs 现装配 SysTransport/SysIpc + server.rs::MibServer —— 循环已落地（doc-code 漂移）
```

---

## 1. C 真序（运行时真序重建）

### 1.1 阶段类型判定

MIB 是**服务事件循环型**（用户态单线程 IPC 服务器，`main.c:433-492` 的 `for(;;)` 收信循环）。依据 `main.c:1-33` 头注释与 `99` §6 执行模型声明：单线程、无锁、一次一封信、可阻塞地调用 VFS/PM/DS/VM。

按 R 提示词 §9 服务事件循环型形态，真序分**启动段**与**循环段**。

### 1.2 启动段真序表

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| S1 | SEF 注册两个 init 回调（fresh / restart 同名 `mib_init`） | `main.c:416-428`（`mib_startup`） | restart 只重建静态骨架，丢弃运行期动态节点（`:420-424`） |
| S2 | `mib_init` 依次装配四棵子树 | `main.c:385-409` | `mib_kern_init(:395)`→`mib_vm_init(:396)`→`mib_hw_init(:397)`→`mib_minix_init(:398)` |
| S3 | 遍历全树初始化杂项字段 | `main.c:404`→`tree.c:1520`（`mib_tree_init`→`mib_tree_recurse:1480`） | 建立 parent 指针、计数 `mib_nodes/mib_objects` |
| S4 | 准备远端挂载 | `main.c:407`→`remote.c:41`（`mib_remote_init`） | 初始化 mount 槽位，计数 `mib_remotes` |
| S5 | 静态顶层表在链接期就位 | `main.c:46-62`（`mib_table[]`、`mib_root`） | CTL_KERN/VM/NET/HW/USER/VENDOR/MINIX，NET 靠远端注册填充，USER 归 libc |

### 1.3 循环段真序表

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| L1 | 收信 `sef_receive_status(ANY,...)` | `main.c:445` | 失败 panic |
| L2 | 通知分类：`is_ipc_notify` 则打印跳过 | `main.c:449-454` | 本服务不期望通知 |
| L3 | 按 `m_type` 三路分发 | `main.c:458-479` | `MIB_SYSCTL`→`mib_sysctl(:278)`；`MIB_REGISTER`→`mib_register`(`remote.c:198`)；`MIB_DEREGISTER`→`mib_deregister`(`remote.c:292`)；default→SENDREC 报 `ENOSYS` 否则 `EDONTREPLY` |
| L4 | sysctl 解码六门 | `main.c:291-352` | 非 SENDREC→`EDONTREPLY`(:292)；namelen 界(:302)；短/长名取道(:309-315)；oldp 宽容配对(:322-328)；newp 严格配对(:334-340)；建 `call`(:347-351) |
| L5 | 进树分派 | `main.c:353`→`tree.c:1333`（`mib_dispatch`） | 逐级 find→可见性→判→动作；末尾 `mib_readwrite(:1309)`/handler |
| L6 | 收尾换算 | `main.c:368-377` | `r>=0` 写 oldlen，溢出改 `ENOMEM`+完整长度；错误分支 `r<0` 写 `call_reslen` |
| L7 | 回信 | `main.c:482-487` | `r!=EDONTREPLY` 才 `ipc_sendnb` |

**远端子树请求的一次完整往返（L3 SYSCTL 命中 remote 节点时）**：`mib_dispatch`→`tree.c:1333` 走到 REMOTE 节点→`remote.c:379`（`mib_remote_call`）→对远端 `ipc_sendrec`（阻塞，与 C 一致）→回 `ERESTART` 时本地续走（`tree.c:1410-1416`、`remote.c:455-459`）。注册往返：`mib_register`(`remote.c:198`)→`mib_do_register(:110)`→`mib_get_label(:82)`（问 DS）→`mib_mount`(`tree.c:1544`)。服务死亡：`mib_down`(`remote.c:56`)。

### 1.4 序差表（运行时序 vs 教学序）

| 主题 | 运行时事实（锚点） | 现教学序 | 建议教学序 | 回指补偿 |
|------|-------------------|---------|-----------|---------|
| 主循环 vs 拷贝/鉴权 | `mib_copyout/copyin/authed` 定义在 `main.c:90-272`，与循环同文件 | 拆到 06/07，主循环在 01 | 保持拆分（单篇单语义） | 01 §4.1 已声明边界 |
| 启动体 vs 静态树 | `mib_init:385` 调 `mib_kern_init` 等，装配体在 tree/子树文件 | 01 讲承诺，04 讲装配体 | 保持 | OK |
| **请求端到端** | L3→L7 是一条连续生命周期 | 无任一篇纵贯，散在 01/10 + 各"补记" | **新增 23 纵贯**（见 §4） | 23 引用 01/02/06/07/09/10 |

---

## 2. 知识点全集（去重后的存量池 + 审计新增）

### 2.1 存量知识点池（按现有文档主轴，去重后归并）

> 类型：C=概念，M=机制，DS=数据结构，IP=接口/协议，IN=约束/不变量，ARCH=架构演进，TE=工具/工程，T=测试。

| 编号 | 名称 | 类型 | 来源 | 现有主位置 | 锚点 | 读者收益 |
|------|------|------|------|-----------|------|---------|
| K-001 | MIB 服务定位与启动映像约束 | C | 存量 | 00,01 | `main.c:1-33` | 为什么 MIB 必须早于 init 启动 |
| K-002 | SEF 双 init 承诺（fresh/restart 丢动态） | M | 存量 | 01 | `main.c:416-428` | 重启后树上只剩什么 |
| K-003 | 主循环三段（收信/分发/回信）+ 两拒绝 | M | 存量 | 01 | `main.c:433-492` | 服务一生在干什么 |
| K-004 | sysctl 进门六门（解码 verdict） | M | 存量 | 01 | `main.c:291-352` | 一次请求要过哪些检查 |
| K-005 | 收尾换算与 ENOMEM 特殊语义 | IP | 存量 | 01,09 | `main.c:368-377` | 缓冲区太小≠内存不够 |
| K-006 | 三类调用号/共享编号/五种线结构 | IP | 存量 | 02 | `com.h:1022-1030,613-622`;`ipc.h` | 消息面契约 |
| K-007 | sysctlnode/sysctldesc VERS_1 交换格式 | IP/ARCH | 存量 | 02,11 | `sysctl.h:130-134` | 对外 ABI 不可变（A-4） |
| K-008 | mib_node 形状与 flag/type 词汇 | DS | 存量 | 03 | `mib.h:72-74,110-280` | 一个节点是什么 |
| K-009 | 静态子树装配（四 _init + MIB_ENODE） | M | 存量 | 04 | `main.c:395-398`;`kern.c:504` | 树怎么长出来 |
| K-010 | 树遍历初始化（parent/计数） | M | 存量 | 04 | `tree.c:1480-1543` | 计数节点数据从哪来 |
| K-011 | 逐级查找 find/scan（含提前停） | M | 存量 | 05 | `tree.c:35-89,278` | 名字→节点 |
| K-012 | copyout/copyin/range/aux | M | 存量 | 06 | `main.c:90-202` | 数据怎么跨地址空间搬运 |
| K-013 | relay_old/newp 授权转交 | M | 存量 | 06,07 | `main.c:210-252` | 远端请求怎么代持内存 |
| K-014 | 鉴权缓存与权限门（authed/ANYWRITE/PRIV） | M | 存量 | 07 | `main.c:259-272` | 谁能读写特权节点（无 EACCES） |
| K-015 | 动态节点 create/destroy 生命周期 | M | 存量 | 08 | `tree.c:487-925` | 运行期建树/拆树 |
| K-016 | 动态节点存储底座：arena + 字节预算(A-3) | DS/M | **部分存量+新增** | 08(薄) | `heap.rs`;`tree/arena.rs`;`tree.c:679-685` | 树在 Rust 里物理怎么存、分配失败为何报 EINVAL |
| K-017 | 叶子读写 read/write/readwrite | M | 存量 | 09 | `tree.c:1131-1332` | 立即值/指针/结构三类叶子存取 |
| K-018 | 分派 walk 主循环 judge_level→act | M | 存量 | 10 | `tree.c:1333-1479` | 名字逐级降落到 handler |
| K-019 | QUERY/DESCRIBE 元标识符 | M | 存量 | 11 | `tree.c:180-247,973-1097` | 遍历/自省树的两个负号 |
| K-020 | CREATE/DESTROY/MMAP/CREATESYM 排除(EOPNOTSUPP) | IN | 存量 | 11 | `tree.c`(meta) | 哪些元操作不支持（A-9） |
| K-021 | 远端子树挂载/遮蔽/转交/死亡清理 | M | 存量 | 12 | `tree.c:1544-1842`;`remote.c` | 别的服务挂自己子树 |
| K-022 | kern 子树（13 handler + 大 ipc 表） | M/DS | 存量 | 13 | `kern.c:*` | 内核信息节点族 |
| K-023 | vm/hw 子树 | M | 存量 | 14 | `vm.c`;`hw.c` | 内存/CPU 信息 |
| K-024 | minix 私有子树（mib 计数 + test 子树） | M | 存量 | 15 | `minix.c:47-89` | Minix 扩展信息、test87 场地 |
| K-025 | 进程三表快照与拉取(pull) | M | 存量 | 16 | `proc.c:47-224` | 表怎么拿、PID hash |
| K-026 | LWP 状态机 + wchan 低八位类别 | M/DS | 存量 | 17 | `proc.c:225-593` | 进程状态/睡在哪 |
| K-027 | KERN_PROC2 九过滤 + kinfo_proc2 | M | 存量 | 18 | `proc.c:601-913` | ps 第二套格式 |
| K-028 | KERN_PROC_ARGS 逐页走 + copybudget | M | 存量 | 19 | `proc.c:919-1177` | 读别家命令行 |
| K-029 | minix proc 整表/单进程快照（负号=任务） | M | 存量 | 20 | `proc.c:1178-1288` | ProcFS 要的两份数据 |
| K-030 | libc sysctl(3) 客户端契约（本地子树/失败也抄/排序） | IP | 存量 | 21 | `lib/libc/gen/sysctl.c` | 不重写只履约（A-10） |
| K-031 | rmib 挂载库（稀疏节点/槽表/栈缓冲） | IP/M | 存量 | 22 | `rmib.c`;`rmib.h` | 服务侧登记应答 |
| K-032 | 全局常量/错误码/跨服务引用索引 | IP/ARCH | 存量 | 99 | `com.h`;`sysctl.h` | 查询手册 |
| K-033 | 跨服务传输缝 MibKernel/MibServices/MibIpc(A-12) | ARCH/M | **新增** | 99§5(薄表) | `transport.rs:1-20`;`remote.c:379` | 服务对外调用怎么建模、fail-closed |
| K-034 | Rust 装配循环 + 三臂接线 | M | **新增(纠漂移)** | 01(陈旧) | `main.rs`;`server.rs:1-20` | verdict 模块如何拼成可运行循环 |

### 2.2 统计摘要

- 存量知识点：32 条主概念簇（对应现有 24 篇，去重合并）。
- 审计新增：3 条（K-016 部分、K-033、K-034），均有 C/Rust 锚点，非凭空。
- C 函数覆盖：82 个函数全部映射到 K-001~K-032（见 §3.1 覆盖对账）。

---

## 3. 覆盖审计

### 3.1 C 符号 → 文档对账（防遗漏）

| C 文件 | 函数数 | 覆盖文档 | 结论 |
|--------|--------|---------|------|
| main.c | 14 | 01(循环/解码) 04(init) 06(copy 90-202) 07(auth/relay 210-272) | 全覆盖 |
| tree.c | 22 | 04/05/08/09/10/11/12 | 全覆盖 |
| remote.c | 9 | 12 | 全覆盖 |
| kern.c | 13 | 13 | 全覆盖 |
| proc.c | 16 | 16/17/18/19/20 | 全覆盖 |
| hw.c | 4 | 14 | 全覆盖 |
| vm.c | 3 | 14 | 全覆盖 |
| minix.c | 1(+静态表) | 15/20 | 全覆盖 |

**C 侧无覆盖缺口**（82/82 有归属）。这与 `plan.md §5.3` 的映射独立复核后一致（此处以自提取的函数定义行为准，非照抄 plan）。

### 3.2 覆盖缺口表（缺口在"代码已实现 / 请求生命周期"，不在 C 符号）

| 缺口 | 证据 | 严重度 | 建议 |
|------|------|--------|------|
| G-1 请求端到端纵贯线缺失 | L3→L7 无任一篇讲透；仅 01/10 各讲一段 + "补记" | 高（服务事件循环型主线要求） | **新建 23-mib-request-lifecycle** |
| G-2 跨服务传输缝(A-12) 只有一张表 | `transport.rs` 469L 两 trait + fail-closed 实端；`99` §5 仅索引 | 高 | **新建 24-mib-transport-seams** |
| G-3 物理存储底座(A-3) 薄 | `heap.rs`190+`arena.rs`611；08 只讲 dynode 生命周期不讲容器决策/字节预算 | 中 | **并入 08（扩契约，非新篇）** |
| G-4 doc 01 与代码漂移 | 01 §3"搬进 Rust=空壳被否决"、§4.1"main.rs 占位" 与 `main.rs`(装 SysTransport/SysIpc)+`server.rs`(876L 已装配循环) 矛盾 | 高（事实错误） | **纠偏 01**（见 §6） |

### 3.3 重复主题表

| 主题 | 出现处 | 主讲述点裁决 |
|------|--------|-------------|
| ENOMEM 收尾换算 | 01 K-005, 09, 21 | 主：01；09/21 只引用 |
| 拷贝/授权语义 | 06,07,09,22 | 主：06/07；09/22 引用 |
| 状态判定链（僵死>停>可运行） | 16,17,20 | 主：17；20 明说复用 17（已是引用，合规） |
| 负进程号语义 | 17/18（-1=全部）vs 20（负号=任务） | 二者语义不同，非重复，保留（20 §1.3 已澄清区别） |

重复均为"主讲述点 + 下游引用"结构，**无越界重复展开**，不需合并。

### 3.4 越界主题表

逐篇头部均带"本章不讲什么 + 去向"，未发现越界。唯一隐患：01/10 用"移交要数据的代码"表述执行层，读者找不到承接处（代码已实现但文档未回收）——由 G-4/新建 23/24 收口，不判越界。

### 3.5 非 C 主题逐项回答（固定清单）

| 主题 | 在哪讲 / 为何不在本 stage |
|------|--------------------------|
| 链接与加载 | 不在本 stage（MIB 是被 RS 加载的用户服务，加载归 03-stage-rs/01-stage-kernel） |
| 镜像与内存布局 | 不在本 stage（同上） |
| 汇编入口/陷阱进入 | 不在本 stage（kernel 侧；MIB 用 `minix-sef`/`minix-sys` 抽象，见 24 的接缝边界声明） |
| 启动装配 | 01（SEF 双 init）+ 04（静态树装配） |
| 构建与工具链 | 范围外（workspace 在 `os/`，`cargo build/test`，无 MIB 专属脚本） |
| 跨模块接口与线格式 | 02（消息/VERS_1）+ **24（跨服务缝，新增）** + `minix-types::sysctl_abi`（钉布局） |
| 错误路径 | 99 §4（ENOMEM/EDONTREPLY/ERESTART 三特例）+ 各篇 §4.3 不变量 |
| 关闭与退出 | 无对等（服务常驻；重启丢动态在 01 K-002；死亡清理在 12 K-021） |
| 并发与同步 | 99 §6（单线程事件循环声明，与 kernel BKL 隔离） |
| 测试基建 | 各篇 §5 + `todo.md` 基线；C 侧 `rmibtest`/`test87`/`t_sysctl`（15/21/22 引用） |

---

## 4. 新目录

### 4.1 总原则

**保持 00~22 + 99 现有编号不变**（断链面：intra 163 + 代码注释 44 + 跨 stage ~18，改号收益远低于代价，与 `06-todo.md` I-14 裁决一致）。新增篇章用**尾部编号 23/24**（不与既有 99 冲突），扩写只在 08/01 就地改契约，不动号。

### 4.2 新篇章总表

| 编号 | 标题 | 定位（一句话） | 分组 | 状态 |
|------|------|---------------|------|------|
| 00~22 | （保持现状） | 启动→接口→结构→数据操作→分派→子树→进程→客户端 | — | 保留（01/08 内容纠偏/扩容） |
| 99 | 全局概念收口 | 常量/错误码/跨服务引用查询手册 | 收口 | 保留（§5 让位给 24，改为一句话+指针） |
| **23** | **请求生命周期纵贯线** | 一封 sysctl 信从进来到回信的端到端走查 | 主干（backbone） | **新建** |
| **24** | **跨服务传输缝** | MIB 如何对 kernel/PM/VFS/DS/VM 五个对端开口（A-12） | 邻接服务协议 | **新建** |

### 4.3 阅读路径

- **主干（必读，按序）**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 09 → 10 → **23**（在此收拢前十一篇为一条线）→ **24**。
- **子树场景（读完 10+23 后按兴趣）**：11 → 12 → 13 → 14 → 15 → 16 → 17 → 18 → 19 → 20。
- **客户端契约（可跳读，服务实现读完后）**：21、22。
- **速查**：99（任何阶段回查常量/错误码）。
- **并行体规则**：13~20 是并行子树族，不强行线性化；12 的远端子树与 22 的挂载库是一对（服务侧/客户端侧），21/22 归客户端组。代表成员：子树精讲以 13（kern，最大）为准，14/15 按差异表收束；进程族以 17（状态机最难）为准，18/19/20 按"复用 17 结论 + 换输出/换过滤"收束。

---

## 5. 每篇契约（仅对新建 / 变更篇章给完整契约；保留篇见 §6）

### 23-mib-request-lifecycle — 请求生命周期纵贯线

- **一句话定位**：让读者能独立回答"一次 `sysctl(2)` 调用，从消息进门到回信离开，经过了哪些已有篇章的哪一段，接缝在哪里"——把 01~12 拆散的判定串成一条可执行的路径。
- **讲什么**：K-003(三段) + K-004(六门) + K-018(walk judge_level→act) + K-012/K-013(拷出/授权在路径上的位置) + K-014(鉴权门在 walk 中的插入点) + K-005(收尾换算) + K-021(REMOTE 臂的去程/回程/ERESTART) + K-034(Rust 三臂装配)。**视角是"路径与接缝"，不是重复各段的判定细节。**
- **不讲什么**：六门各自的 verdict 细节（→01）；节点形状（→03/05）；各 handler 内部（→13~20）；拷贝原语实现（→06）；传输缝 trait 定义（→24，23 只在路径上引用）。
- **前置**：01、02、05、06、07、09、10、12（全部更早）。
- **后置**：24（沿 23 的接缝展开对端）；各子树篇（13~20 都站在 23 的"命中叶子/handler"终点上）。
- **事实底线**：
  - C：`main.c:278-379`（`mib_sysctl` 六门 + 收尾）、`tree.c:1333-1479`（`mib_dispatch` walk）、`main.c:433-492`（循环三臂）、`remote.c:379-477`（`mib_remote_call` 去回）。
  - Rust：`os/servers/mib/src/main.rs`（真端装配）、`server.rs`（三臂 + 两缝）、`walker.rs`（`mib_dispatch` 执行体）、`dispatch.rs`（六门 verdict）。
- **知识点清单**：见 §2.1 K-003/004/005/012/013/014/018/021/034（来源=存量，主位置在 23 收敛；旧文档段落按 §8 迁移表剪贴）。
- **验收标准**：画出"一封 SYSCTL 信"的完整泳道图（进门→六门→walk 每轮 find/see/judge/act→叶子读 or handler or remote 去回→收尾→回信），每个节点标注对应篇章编号与 C 锚点；读者据此能定位任一 errno 是在路径哪一步产生的。**不得**重新展开各段判定细节（那是 01/10 的职责），只做缝合与定位。

### 24-mib-transport-seams — 跨服务传输缝（A-12）

- **一句话定位**：讲清 MIB 对外的两条通道（kernel 原语 / 邻端服务）如何抽象成 trait、为何"最终形状先行"、真端 fail-closed 与测试替身怎么分。
- **讲什么**：K-033（`MibKernel`：`sys_datacopy` 双向 + `cpf_grant_magic/revoke` + `sys_getproctab` + `getticks/sys_hz/sys_getcputicks`；`MibServices`：`getnuid`(PM) + `getsysinfo`(PM/VFS) + `ds_retrieve_label_name`(DS) + `vm_info_stats/usage`(VM)）；`MibIpc`（与邻端收发消息）；两缝分离理由（一个测试替身不必脚本另一个）；fail-closed 实端（power 随 trap 层到位，edge E1）；与 C 的 `sys_datacopy`/`ipc_sendrec` 对应。
- **不讲什么**：拷贝/授权的**语义**（→06/07，24 讲它们经哪条缝落地）；进程表数据内容（→16）；远端挂载协议（→12/22，24 只讲 `ds_retrieve_label_name` 这一步的缝）；线格式（→02）。
- **前置**：06、07、09、12、16（全部更早）；23（路径上下文，23 编号在前则依赖 23）。
- **后置**：无（叶子篇）。
- **事实底线**：
  - C：`main.c:136,183,201`（`sys_datacopy`）、`main.c:216,241`（`cpf_grant_magic`）、`remote.c:379-477`（`ipc_sendrec` 到远端）、`kern.c`（`getticks`/`sys_hz`）、`proc.c:47-143`（`sys_getproctab`/`getsysinfo`）。
  - Rust：`os/servers/mib/src/transport.rs:1-469`（`MibKernel`/`MibServices` trait + fail-closed 实端）、`server.rs`（`MibIpc`）、`os/libs/minix-sys/src/{ipc,grant}.rs`。
  - 跨 stage：`../edge_todo.md` E1/E-RMIBWIRE/E-DSWIRE/E-MIBPROD（对端交付条目）。
- **知识点清单**：K-033（新增，来源=`transport.rs` 头注释 `:1-20` + C 锚点）；引用 K-012/K-013/K-025 作对照。
- **验收标准**：列出每一条跨服务调用（≥9 条），逐条给"谁发起（哪篇/哪个 handler）→ 走哪条缝（kernel/service）→ C 对应物 → 对端 stage（带 edge 编号）"；说明为何两缝必须分开、为何真端 fail-closed 不算空实现。**明确声明**：trap 层与对端实装不在本 stage（移交边条目）。

### 08-mib-dynamic-nodes（扩容契约，编号不变）

- **一句话定位**：在原"运行期建树/拆树生命周期"上，补齐**物理存储底座**（A-3）。
- **新增讲什么**：K-016——arena 容器决策（`BTreeMap<i32,NodeId>` vs 排序 `Vec`+二分 vs C 侵入式链表，`tree/arena.rs:1-20` 三选一的取舍）；字节预算堆（`heap.rs:1-20`：每笔预扣、缓冲区记长、释放精确归还；无槽宽对齐）；**分配失败报 EINVAL 不报 ENOMEM** 这条 C 规矩在堆层的落点（`tree.c:589,1043,1207`）与 temp mount 唯一例外（`:1741-1744`）。
- **不讲什么**：传输缝（→24）；拷贝语义（→06）。
- **事实底线新增**：Rust `heap.rs`(190L)、`tree/arena.rs`(611L)；C `tree.c:679-685`（dynode header+name+data 一块分配）、`:1043-1048`（desc strdup）、`:1235-1238`（超大写临时缓冲）、`:1739`（temp mount head+name+desc）。
- **验收标准**：读者能回答"为什么 Rust 用 BTreeMap 而 C 用链表"、"为什么 dynode 三块合一分而 arena 无槽宽"、"堆耗尽为何报 EINVAL"；容器三选一的否决须各带一条量化理由（O(log n)/一次分配/unsafe 别名）。

---

## 6. 变更表

| 操作 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 |
|------|------|--------|--------|------|-----------|------|
| C-1 | 新建 | — | 23 | 服务事件循环型主干缺纵贯线（G-1） | K-003/004/005/012/013/014/018/021/034 | 新增，来源 01/10/12 段落 + 代码 |
| C-2 | 新建 | — | 24 | A-12 跨服务缝只有一张表（G-2） | K-033 | 新增，来源 transport.rs + C 锚点 |
| C-3 | 扩容 | 08 | 08（就地） | 物理存储底座薄（G-3） | K-016 | 存量 dynode 生命周期保留 + 新增 K-016 |
| C-4 | 纠偏 | 01 §3/§4.1 | 01（就地） | 文档称循环"空壳被否决/占位"与已落地 `server.rs`+`main.rs` 矛盾（G-4，事实错误 P0-code-bug 类） | K-034 | 改写为"循环已随 P1-1 落地，本处讲装配，端到端见 23" |
| C-5 | 让位 | 99 §5 | 99 §5→指针 | 跨服务引用表升级为 24 主讲，99 收成一句话+指针 | K-032/K-033 | 99 保留索引，机制移 24 |
| C-6 | 横切 | 全部 18 篇 | 各篇锚点 | 131 条"工具生成"锚点须重解析（§8） | — | 不新建篇，B 相批量执行 |
| C-7 | 横切 | 13~20 风格 | 各篇 | 01~12 简练 vs 13~20 教学重写口吻不一致 | — | B 相按统一文风标准收敛 |

**未采用**：重编号 01~22（断链成本 > 收益）；合并 01~12 简练篇（它们各自单语义，合规）；为 test 子树/`rmibtest` 单独建篇（15/21/22 已在测试要点覆盖，不属"机制"缺口）。

---

## 7. 缺漏新篇（非 C 主题落实）

- §3.5 十项中，唯一"代码/理论承载但文档缺位"的两项已落实为新建：**跨模块接口（传输缝）→ 24**；**请求生命周期主线（服务事件循环型的核心组织要求）→ 23**。
- 物理存储底座（A-3，介于数据结构与工具之间）→ 并入 08（C-3），不单建篇，理由：与 dynode 生命周期同一语义单元，拆出会与 08 边界重叠。
- 其余非 C 主题（链接/镜像/汇编/构建）明确**不在本 stage**，理由见 §3.5（MIB 是被动加载的用户服务，引导/构建归 kernel/rs stage）。
- 本节无"待定"项。

---

## 8. 锚点迁移与断链成本

### 8.1 锚点重解析（横切，覆盖 18 篇 131 条）

**问题（已核实）**：`（L###，工具生成）` 后缀 131 条，三类缺陷：

1. **C 函数边界串名**（名字指向相邻函数的尾行）——实证：
   - `kern.c:mib_kern_ipc_info（L503）` 用于指 `mib_kern_init`（真实定义 `mib_kern_init` 在 504，503 是 `ipc_info` 尾行）。
   - `kern.c:mib_kern_consdev（L208）` 用于指 `forkfsleep`（`forkfsleep` 真实在 209，208 是 consdev 尾行）。
   - `kern.c:mib_kern_boottime（L306）` 用于指 `ipc_info`（`ipc_info` 真实在 307）。
   - `proc.c` 系列 `fill_wmesg（L224）`/`mib_kern_proc2（L918）`/`mib_kern_proc_args（L1177）` 等：行号落在目标函数内但函数名取的是"上一条定义"，属半错位。
2. **非存在符号碎片（死链）**——实证：`mproc.h:sigaction（L82/L28）`（doc16/17，mproc.h 无 `sigaction` 定义）、`rmib.h:ssize_t（L48/L52）`（以类型为锚名）、`id.rs:type GrantId（L74）`（可核，若行内无该 type 即死链）。
3. **Rust 源锚点带噪**：如 `relay.rs:struct RelayRegion（L51，工具生成）`——符号名对，`（L51，工具生成）` 为机器噪声，应降级为 `relay.rs::RelayRegion`。

**迁移规格（B 相执行）**：

| 旧锚形态 | 新锚形态 | 验证方式 |
|----------|----------|---------|
| `file.c:sym（L###，工具生成）` | `file.c:function_name` | 用 §0.4 提取的 82 函数定义行核对：符号名必须等于 `行号所属函数` |
| `sym（L###）` 且 L 落在别的函数体内 | 改为 L 所属真函数名，或按内容意图改指正确函数 | 双向核对（名 ↔ 行范围） |
| 非存在符号（sigaction/ssize_t 等） | 删除锚名，改指真实定义位置或标注"待验证" | `grep` 目标文件确认符号存在 |
| Rust 源 `path.rs:sym（L###，工具生成）` | `path.rs::sym` | `grep -n "sym" path.rs` 确认 |

### 8.2 引用迁移表（因新增 23/24 + 99§5 让位产生）

| 旧引用 | 出现处（实测计数） | 新目标 | 验证 |
|--------|-------------------|--------|------|
| `见 99 §5 跨服务引用` | 99 内部 + 下游 | 24（99§5 保留一句话指针） | grep `99 §5`/`§5` |
| "主循环空壳/占位"（01 §3 line140,§4.1） | 01 自身 + 代码注释 `main.rs` 头（引 01） | 改写 + 指向 23 | grep `占位|空壳` in 01；核对 `os/servers/mib/src/main.rs` 注释 |
| intra-stage `NN-mib-*.md` 交叉引用 | 163 处 | 编号不变→0 改动 | 保持号即免迁移 |
| 代码注释 `NN-mib-*.md` | os/ 44 处（01×6 最热，03/06/08/10/11/12 各 3） | 编号不变→0 改动；新增模块注释可指 23/24 | grep（§0.3） |
| 跨 stage 引用 `10-stage-mib` | edge_todo、master-plan README、13-stage-ipc(6 篇,含 03-ipc-mib-registration)、17-stage-net(3 篇)、12/14-stage plan | 目录名不变→0 改动；13-stage-ipc 对 12/22 的引用不受影响 | grep 已列 |

### 8.3 断链成本摘要

- **免改动**：所有 00~22/99 编号保持稳定 → intra 163 + 代码 44 + 跨 stage ~18 全部**零改动**（这是本蓝图首要成本决策：用尾部编号 23/24，不重排）。
- **必改动**：99 §5 一处让位、01 两处纠偏、18 篇锚点重解析（131 条）、23/24 两篇新目录项 + 00-overview 导航表加两行。
- **热点**：`00-mib-overview.md` 导航表、`99`、`01`；代码注释热点 `main.rs`（引 01）。
- **批量方式**：锚点重解析用脚本（对 §0.4 函数行表做名↔行核对，产出候选清单，人工裁决死链）；引用迁移因编号不变仅需处理 99§5/01 两处。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**：23 前置={01,02,05,06,07,09,10,12}、24 前置={06,07,09,12,16,23}，均指向更早或本次新增且置后；08 扩项前置不变。✅ 零前向引用。
2. **依赖图无环**：01~22 现有 DAG 不变；23 汇聚 01~12（下游，无回边）；24 依赖 23（单向）；99 指向 24（单向）。✅ 无环。
3. **覆盖率 100%**：82/82 C 函数有归属（§3.1）；34 条知识点（31 存量 + 3 新增）全部有去向（§2.1/§4）；删除项 0。✅
4. **断链成本**：编号不变策略下，必改仅 99§5 + 01 两处 + 131 锚点重解析 + 2 新篇目录登记。✅ 已量化（§8.3）。

### 9.2 自检门 G1~G9

| 门 | 结果 | 证据 |
|----|------|------|
| G1 C 真序逐条可核对 | ✅ | §0.4 抽 10 条（mib_sysctl278/mib_dispatch1333/mib_register198/mib_mount1544/mib_kern_init504/mib_kern_lwp510/mib_kern_proc2792/mib_kern_proc_args919/mib_minix_proc_list1178/mib_minix_proc_data1217）均与 grep 定义行一致 |
| G2 知识点池完整（每 C 文件/制品有归属或排除） | ✅ | §3.1 八文件全覆盖；§3.5 非 C 制品逐项回答 |
| G3 前向引用为零 | ✅ | §9.1(1) |
| G4 依赖图无环 | ✅ | §9.1(2) |
| G5 覆盖率 100% + 新增带锚点 | ✅ | 新增 K-016/K-033/K-034 均带 `heap.rs`/`arena.rs`/`transport.rs`/`server.rs` + C 锚点 |
| G6 拆分/合并写清去向、新建写清来源 | ✅ | C-1/C-2 来源明；C-3/C-4/C-5 存量为"就地保留/让位"，无知识点丢失 |
| G7 契约七要素齐全 | ✅ | §5 三篇（23/24/08）七要素齐；保留篇沿用原契约 |
| G8 锚点迁移覆盖变化文档 + 引用迁移 | ⚠️ 部分 | §8 覆盖 18 篇 131 锚点 + 引用面；逐条 L→符号的最终裁决留 B 相（本蓝图给规则 + 已核实样本，不逐条穷举 131） |
| G9 事实断言带锚点 | ✅ | 关键新断言（G-4 漂移、锚点串名）均带 `main.rs`/`server.rs`/`kern.c` 行号；推测项无 |

### 9.3 结论与待裁决问题

**结论**：现有 24 篇质量成熟、单篇单语义、顺序合规（C 侧零覆盖缺口）。本蓝图不做重排/重编号，只做**四处定向重建**：新建 23（请求生命周期主干）、新建 24（跨服务缝）、扩容 08（存储底座）、纠偏 01（doc-code 漂移），加两项横切（锚点重解析、风格收敛）。

**待用户/共识相裁决**：

1. **24（传输缝）是否本 stage 主讲**：其"实端 fail-closed + trap 层"依赖边条目 E1/E-RMIBWIRE（跨 stage）。本蓝图判定 trait 抽象形状是 MIB 本地设计、值得主讲，实端移交。若共识倾向"跨 stage 一律不在 stage 内建篇"，24 可降级为 99§5 扩容。**推荐：建 24**（代码 469L 已是 MIB 本地设计，不建反而留最大的一片"有码无文"）。
2. **08 扩容 vs 单建 25**：本蓝图选择并入 08（同一语义单元）。若 08 超软长度上限（现约 ?行），可拆出 `25-mib-dynamic-storage`。B 相按实际行数定。
3. **G-4 纠偏 01 的定级**：`01 §3「搬进 Rust=空壳被否决」`与 `§4.1「main.rs 占位」` 与现状矛盾，属 P0-code-bug（文档事实错误）。是否连带核查 03/08/10/15 内同类"移交/占位"陈旧表述——建议 B 相对全部"移交要数据的代码"字样做一次代码存在性复核（本文档已抽查 server/walker/transport/heap/arena 均已落地）。
