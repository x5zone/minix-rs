# 10-stage-mib 文档重建蓝图（glm）

## 0. 元数据

- **执行者**：glm
- **日期**：2026-09-19
- **目标目录**：`notes/rewrite/fork-syscall-rewrite/10-stage-mib/`
- **仓库根目录**：`/home/xzhao/github/minix-rs`
- **当前提交号**：`927af52abb242578387faa49f656d737ac6a9adf`（2026-09-19）
- **任务**：R 相·重建蓝图。只产出本文件，不修改任何正文。多 AI bagging：未读取任何其它 AI 的 `doc_rerank_*` 产物（含同名后缀的他 stage 产物，仅统计引用计数）；未读取 `.design/` 与 `tmp_design_and_todo/`。

### 0.1 审查范围

- **文档（重建对象）**：编号文档 24 篇——`00-mib-overview.md`、`01`～`22`、`99-mib-global-concepts.md`。**全部 24 篇均为 reviewed 状态**（plan.md §6.1：00/99 于 2026-09-15 成文，01~12 于 2026-09-04，13~22 于 2026-09-05 教学化/白话化重写）。这是本次三个 rerank 目标（05-vfs、07-ds、10-mib）中成熟度最高的 stage。
- **参考材料（不重建，只作证据与边界）**：`plan.md`（467 行，2026-08-16 定稿 + 2026-09-15 P3-1 入档批）、`todo.md`（317 行，2026-09-15 首轮架构审查；**P1-1~P1-5、P2-1~P2-4、P3-1、P3-2 十一条全部 ✅ 闭环**，执行半已落地）、`README.md`（48 行）、`draft/`（旧占位素材）。
- **范围外**：`.design/`（规范禁止引用）；其它 AI 的 `doc_rerank_*`（bagging 约束）；其它 stage 文档正文（只读 09-stage-init 的 00 总览与对本 stage 的引用）。

### 0.2 读取清单

| 类别 | 内容 |
|------|------|
| 目标文档 | 24 篇编号文档全文、plan.md、todo.md、README.md |
| C 源码（全文精读） | `servers/mib/` 8 个 .c 共 4990 行：`main.c`（492）、`tree.c`（1842）、`remote.c`（477）、`kern.c`（508）、`proc.c`（1288）、`hw.c`（140）、`minix.c`（89）、`vm.c`（154）；`mib.h`（390）；`Makefile`（21 行，仅清点） |
| 协议头文件 | `include/minix/com.h`（MIB_PROC_NR:66、COMMON_RQ_BASE:597、COMMON_MIB_INFO:613/CALL:616/REPLY:622、MIB_BASE:1022、IS_MIB_CALL:1024、MIB_SYSCTL/REGISTER/DEREGISTER:1026-1028、NR_MIB_CALLS:1030）；`ipc.h`（CTL_SHORTNAME:15、mess_lc_mib_sysctl:424-433、mess_lsys_mib_register/reply:1373-1389、mess_mib_lc_sysctl/call/info:1548-1580）；`sys/sys/sysctl.h`（CTL_MAXNAME:75、类型/标志/元标识符:92-164、sysctlnode/sysctldesc:1382-1454）；`include/minix/sysctl.h`（97 行，CTL_MINIX/MINIX_*/TEST_*/minix_proc_*）；`include/minix/rmib.h`（188 行） |
| 客户端与消费者 | `lib/libsys/rmib.c`（1089 行，函数面清点）；libc 五文件（`gen/sysctl.c` 391、`sysctlgetmibinfo.c` 611、`sysctlbyname.c`/`sysctlnametomib.c`、`libc/sys/__sysctl.c` 40）；消费者锚点：`fs/procfs/tree.c:87`（PROC_LIST）、`fs/procfs/pid.c:44`（PROC_DATA）、`servers/ipc/main.c:91`、`net/lwip/mibtree.c:56,61,66`、`net/uds/stat.c:174`（rmib_register 五处） |
| 测试基建 | `tests/test87.c`（3662 行）、`tests/rmibtest/rmibtest.c`（267 行）、`tests/kernel/t_sysctl.c`（74 行）——行数与存在性核实 |
| boot 锚点 | `kernel/table.c:44-64`（boot_image，MIB 在 :60，TTY :59 之后、VM :61 之前）、`kernel/main.c:264-267`（双重抑制，07-ds 轮已实测） |
| Rust 实现 | `os/servers/mib/src/` 35 文件（含 tree/io/proc/subtree 四个子目录）——`lib.rs`、`dispatch.rs`、`sef.rs`、`server.rs`、`walker.rs`、`transport.rs`、`heap.rs`、`tree/arena.rs` 关键符号锚点核验；`os/libs/minix-types/src/types/sysctl_abi.rs`（23284 字节、55 处 offset_of）；`os/libs/minix-sys/src/rmib.rs` |
| 阶段边界材料 | `00-master-plan/README.md`（07-ds 轮已读）、`edge_todo.md`（E-RMIBWIRE/E-MIBPROD/E-MIBGRANT/E5(g) 条目）、`09-stage-init/00-init-overview.md`（boot 链前序：init 是 boot_image 最后一项 table.c:64，MIB 早于它） |

### 0.3 使用的命令与关键输出（证据摘录）

```text
wc -l minix3/minix/servers/mib/*.c
  → main 492 + tree 1842 + remote 477 + kern 508 + proc 1288 + hw 140 + minix 89 + vm 154 = 4990（与 00 篇头部声明一致）
sed -n '60,70p' com.h → MIB_PROC_NR = 7（:66）；sed -n '1020,1032p' → MIB_BASE 0x1800（:1022）、三信（:1026-1028）、NR_MIB_CALLS=3（:1030）
wc -l rmib.c rmib.h sysctl.c sysctlgetmibinfo.c __sysctl.c test87.c rmibtest.c t_sysctl.c
  → 1089 / 188 / 391 / 611 / 40 / 3662 / 267 / 74（plan §5.4 全部数字核实）
(ulimit -v 3145728; cargo test -p minix-mib --lib) → 152 passed; 0 failed（实测 2026-09-19；
  todo.md 记录的执行轮序列 107→108→113→116→126→136→150，本轮 152 为其后又增 2）
逐文件 #[test] 计数：server 12 / walker 10 / io/copy 10 / dynamic 8 / dispatch 8 / tables 7 /
  proc2 7 / lwp 7 / readwrite 7 / node 6 / tree-dispatch 5 / query 5 / relay 5 / heap 5 / auth 5
  / mount 4 / init 4 / version 3 / lookup 3 / flag 3 / arena 3 / vm 3 / kern 3 / remote 3
  / proc_args 3 ……（子目录合计 152）
grep -n 关键符号：server.rs MibIpc:65 / MibServer:85 / run_once:142 / run:547 /
  MAX_CONSECUTIVE_RECV_FAILURES=64:82；walker.rs sysctl():141；heap.rs MIB_HEAP_BUDGET=256KiB:35；
  sef.rs MibInitKind:24；arena.rs MibTree:293/init():335；
  dispatch.rs MibCall:41/Incoming:70/triage:86/default_outcome:103/should_reply:114/
  check_namelen:124/classify_name:151/pair_oldp:173/pair_newp:195/SysctlOutcome:217/
  map_sysctl_reply:254（注意：01/02 篇引用的 :217 已漂移——P2-2 重构所致，见 §3.2 E4）；
  tree/dispatch.rs LevelFacts:98/judge_level:125/resolve_shape:154/judge_remote_result:190/terminal_code:206
ls os/libs/minix-types/src/types/sysctl_abi.rs → 存在（23284 字节，55 处 offset_of）
引用统计：入站文档引用 19 文件 52 处（含其它 AI rerank 5 处，内容未读）；
  stage 内互引 ≈180 处（10 篇被引 16 次为枢纽）；os/ 代码注释 4 文件 5 处；
  出站跨 stage 引用 5 个目标（../01-stage-kernel/09、12；../07-stage-ds/01、02、08）全部实测存在
os/servers/mib/src/types_test_placeholder → 0 字节杂物文件（2026-09-15 遗留，范围外发现）
```

---

## 1. C 真序

### 1.1 阶段类型判定

**服务事件循环型**（主体）：MIB 是单线程按名查询服务器（`main.c:443` `for (;;)`），一生 = 启动（SEF 两回调注册同体 `mib_init`、建七顶层槽、四线 wiring、全树点名、远端表复位）+ 主循环（收信、notify 拒绝、三信分派、回信）。按 R 相提示词 §九的服务事件循环型模板组织。

**附带两个次级形态**：① **分发树遍历型**——`mib_dispatch` 是"名字逐分量消耗"的循环（`tree.c:1346`），一次 sysctl 的旅程是本 stage 的次主线（10 篇为家）；② **客户端库面**——libc sysctl(3) 五函数（21 篇，外部契约）与 rmib.c 挂载库（22 篇，15 API 级集合），按"统一框架 + 分组"组织。

判定依据：`main.c:1-33` 头注释自述"one and only task is to implement the sysctl(2) system call"；无自有汇编入口、不管理硬件；超级用户权限要求（`:17-19` 注释）。

### 1.2 启动段真序表

| 步骤 | 动作 | C 锚点 | 说明 |
|------|------|--------|------|
| S1 | boot 映像登记：`image[]` 中 `{MIB_PROC_NR,"mib"}` 在 :60，紧跟 TTY（:59）、先于 VM（:61） | `kernel/table.c:44-64` | 排早的原因：init(8) 自身启动就要调 sysctl（`main.c:15-19` 注释）；且需要超级用户权限 |
| S2 | 内核 boot 循环对全部用户进程置 `RTS_VMINHIBIT` + `RTS_BOOTINHIBIT`；仅内核任务/RS/VM 立即可调度 | `kernel/main.c:196,264-267` | VM 建页表、RS 设特权后 MIB 才可被调度 |
| S3 | MIB 进程从 `main` 进入：先 `mib_startup()` | `servers/mib/main.c:433-440` | |
| S4 | SEF 注册：`sef_setcb_init_fresh(mib_init)` + `sef_setcb_init_restart(mib_init)`——**两回调挂同一函数体**；`sef_startup()` | `servers/mib/main.c:415-428` | 重启语义在 `:420-424` 注释：动态节点全丢，"只剩静态树也比不跑强"（与 DS 的 STATEFUL 保表相反） |
| S5 | `mib_init` 六拍：四线 wiring（kern :395 / vm :396 / hw :397 / minix :398，每行 `MIB_INIT_ENODE` 补 size+表指针）→ `mib_tree_init()`（:404）→ `mib_remote_init()`（:407） | `servers/mib/main.c:384-410` | "先空后填"是跨文件 `sizeof` 无奈（`:388-393` 注释） |
| S6 | 全树点名 `mib_tree_recurse`：空位跳过（flags==0）、`mib_nodes++`+`clen++`、版本继承父、链父指针、NODE+PARENT 递归；`mib_tree_init` 置基线 nodes=1（根自计）、objects=0、根版本 1、根父 NULL | `servers/mib/tree.c:1479-1536` | `csize=size` 先记（:1493）——**遍历静态表用 `size` 不用 `csize`**（动态结点将来也计入 csize） |
| S7 | 远端端点表复位：32 槽（`MIB_ENDPTS=1<<5`）endpt=NONE、nodes=NULL | `servers/mib/remote.c:40-49` | label 槽 16 字节（`MIB_LABEL_MAX=16`，:28） |
| S8 | 静态树形状定格：七顶层槽 id 稀疏（1/2/4/6/8/11/32），vendor 唯一可写；根可写、无名、用户态不可达 | `servers/mib/main.c:46-62` | net/user 两槽永远空等（net 等远端挂载、user 住 libc）；vendor 空但可写 |

### 1.3 循环段真序表

| 步骤 | 动作 | C 锚点 | 说明 |
|------|------|--------|------|
| L1 | `sef_receive_status(ANY)` 阻塞收信，失败 panic；**status 字随信而来** | `servers/mib/main.c:445-446` | 与 DS 的 `sef_receive`（拿调用号猜 notify）不同：MIB 听内核判决（`is_ipc_notify(ipc_status)`） |
| L2 | notify 一律拒绝：打印来源后 `continue`（不回信、不经过沉默标记） | `servers/mib/main.c:449-454` | |
| L3 | 三信分派：MIB_SYSCTL（0x1800）→ `mib_sysctl`；MIB_REGISTER（0x1801）→ `mib_register`；MIB_DEREGISTER（0x1802）→ `mib_deregister`；号空间满（NR_MIB_CALLS=3）无死号 | `servers/mib/main.c:458-472`；`com.h:1022-1030` | 对照 DS 的 DS_SNAPSHOT 死号：MIB 无死号 |
| L4 | default：SENDREC 来的回 ENOSYS，其余回 EDONTREPLY（单向发送者没有等回信的槽位） | `servers/mib/main.c:474-479` | |
| L5 | 回信：非 EDONTREPLY 则 `m_type=r` 后 `ipc_sendnb`；发送失败只打印（收不到才 panic——轻重分明） | `servers/mib/main.c:482-487` | `m_out` 每轮 `memset` 清零复用（:456） |

### 1.4 一次 sysctl(2) 的真序（次主线）

| 步骤 | 动作 | C 锚点 | 说明 |
|------|------|--------|------|
| Q1 | 六道解码门：① 非 SENDREC 直接 EDONTREPLY（:292-293）② namelen 界 0<n≤12 → EINVAL（:302-303）③ 取名字 ≤8 信内 memcpy / >8 `sys_datacopy`（:309-315）④ oldp 配对：有地址才开，长度无地址被原谅（:322-328）⑤ newp 配对：地址长度双非零才开，半份丢弃（:334-340，NetBSD 同款）⑥ 组 `mib_call` 并调 `mib_dispatch`（:347-353） | `servers/mib/main.c:291-353` | 消息字段来自 `mess_lc_mib_sysctl`（ipc.h:424-433） |
| Q2 | 收尾换算：r≥0 报完整长度，给了槽装不下 → ENOMEM（部分字节已拷）；没给槽 → OK；r<0 原码 + `call_reslen`（今天唯一产生处：create 撞车 EEXIST 带现有节点） | `servers/mib/main.c:368-377` | **ENOMEM 是"信封太小"不是"内存不够"**（:355-367 NetBSD 注释） |
| Q3 | `mib_dispatch` 逐层：每轮吃一个分量（:1347-1349）→ 元标识符判（负数须末位 :1360-1382：QUERY/CREATE/DESTROY/DESCRIBE 四投，CREATESYM/MMAP/野负 EOPNOTSUPP）→ find 落空 ENOENT（:1385）→ 私有门 EPERM（:1389）→ 远端挂载（:1404-1416：先快照 can_restart，转交非 ERESTART 原样回；ERESTART+无本地 ENOENT；ERESTART+有本地续走）→ 叶/函数形状三问（:1402,1425-1431：叶看 VERIFY/func，非叶看缺 PARENT 即函数）→ 叶+名剩 ENOTDIR（:1437）→ 写两道杠（:1446-1458：先 READWRITE 再 ANYWRITE-或-root）→ 函数调用 / 叶子 readwrite（verify 传递）/ 下钻 | `servers/mib/tree.c:1332-1474` | 名走完落父上 EISDIR（:1472-1473） |
| Q4 | 常规叶读写 `mib_readwrite`：读（getptr 取 lane → 串报 live+1 其余报宽 → 钳制拷出 → 永远报全长）；写（空写免 → 尺寸判 → stage 选草稿或特权堆（败 EINVAL 非 ENOMEM）→ 拷入 → verify → bool 消毒/串补终结落子 → 释 stage） | `servers/mib/tree.c:1097-1325` | 不在位更新禁令（:1171-1181）；字符串三规则（:1203,1274-1287） |
| Q5 | 元操作四本体：QUERY 枚举（先静态后动态、不排序、内部三位剥离、私有不可见值清零、远端报缓存窗口、函数结点置假地址 SYSCTL_NODE_FN）；CREATE 十四道校验链（先廉后贵，分配前全查完；撞车 EEXIST 拷出现有节点+setoldlen；分配失败 EINVAL 非 ENOMEM）；DESTROY 守卫链（特权→父可写→非永久→非挂载点 EBUSY→非函数 EPERM→无孩子 ENOTEMPTY→版本/名字匹配→**先拷贝后删除**，拷失败照删）；DESCRIBE（设描述六杠一次性；读描述私有零长非错；4 字节对齐打包间隙是用户数据） | `servers/mib/tree.c:179-239,486-775,784-916,972-1090` | |
| Q6 | 函数 handler 面（13~20）：kern 十函数 + verify 双星 + 45 填 39 空表；vm 两函数 4 填 9 空；hw 三函数 10 填 6 空；minix 测试子树 12 字面量 + 统计三指针叶 + proc 两门；proc.c 五接口（表快照 → LWP → PROC2 → ARGS → MINIX_PROC） | `kern.c`/`vm.c`/`hw.c`/`minix.c`/`proc.c` 全文 | 函数结点在分发里是终点（名字有剩即 ENOTDIR，:1437 先于一切） |

### 1.5 远程子树真序（第二条次主线）

| 步骤 | 动作 | C 锚点 | 说明 |
|------|------|--------|------|
| R1 | 注册（单向）：SENDREC 来 → ENOSYS（防交叉死锁）；label 获取失败或路径 >8 → EDONTREPLY 静默；`mib_do_register` 槽定位四去向（同端点复用 / 同 label 异端点=老的死了先 `mib_down` / 首空槽 / 满则打印丢弃）；同端点同根 id 拒；`mib_mount` 盖章 | `servers/mib/remote.c:197-233,109-192` | label 经 `mib_get_label`（DS `ds_retrieve_label_name`，:81-104）；单向信永不回（:231-232） |
| R2 | 挂载 `mib_mount`：顶层禁挂 miblen<2 → EPERM（唯一安全类限制）；标志窗/孩子窗（csize≤4096）→ EINVAL；路径 walk 父必真本地非私有 → EPERM；盖旧结点需标志精确匹配+无动态孩子（EBUSY）；造临时点向服务现问名述（`COMMON_MIB_INFO` 双 grant）→ 名体检 → mib_scan 查名冲 → **一块分配名+述（败 ENOMEM——MIB 唯一配说 ENOMEM 的分配**）→ 链入；置 REMOTE+四字段、`mib_remotes++` | `servers/mib/tree.c:1543-1780` | |
| R3 | 转交 `mib_remote_call`：三 grant（名直授 READ + 06 两 relay，失败 EINVAL；**后授先撤**）→ `COMMON_MIB_CALL`（req_id 恒 0、user_endpt、flags=!!authed、双版本快照）→ `ipc_sendrec` → 先 revoke 三 grant → IPC 错 = 服务死了：`mib_down` 清该端点全部挂载 + 回 ERESTART；回信检查（类型/req_id）；status==ERESTART → `mib_do_deregister`（服务请辞） | `servers/mib/remote.c:378-477` | 嵌套 sendrec 是阻塞的——单线程事件循环内同步调用，与 C 行为一致，不设计超时 |
| R4 | 注销/死亡：`mib_do_deregister` 按根 id 摘链（先摘链后 unmount，unmount 可能释放节点）；`mib_unmount` 遮蔽点去 REMOTE+重数静态 clen+版本 bump / 临时点 `mib_remove` 释放；`mib_down` 遍历链表逐个卸载（**先存 next 再卸**） | `servers/mib/remote.c:239-307,55-74`；`tree.c:1789-1842` | 死亡检测无主动通知（remote.c:6-22 TODO：DS 缺订阅原语）——转交失败即死亡 |

### 1.6 客户端与消费者面

| 步骤 | 动作 | C 锚点 | 说明 |
|------|------|--------|------|
| C1 | libc sysctl(3)：CTL_USER 子树本地自办（不进 MIB）；其余 `__sysctl` 填六件套发 MIB_PROC_NR；**失败也抄回 oldlen**（NetBSD 怪规矩，sysctl(8) 靠它）；`sysctlgetmibinfo`（611 行）用 QUERY+DESCRIBE 走全树并**自行排序**（服务不保证顺序） | `lib/libc/gen/sysctl.c`（391）、`libc/sys/__sysctl.c`（40）、`sysctlgetmibinfo.c`（611） | 外部契约 A-10，minix-rs 不重写 |
| C2 | rmib 挂载库：服务侧主循环/树代码的轻量复刻 + 稀疏节点（升序/无重复/有标志三规矩）+ 16 槽表（槽号即线上根 id）+ 257 字节栈缓冲规矩 + 无死亡检测（主服务负责重登记） | `lib/libsys/rmib.c`（1089）、`rmib.h`（188） | |
| X1 | ProcFS 消费：`{CTL_MINIX, MINIX_PROC, PROC_LIST}` 整表 / `{CTL_MINIX, MINIX_PROC, PROC_DATA, pid}` 单进程；**负 PID=内核任务**（ProcFS 语义，与 KERN_LWP 的 -1=全列表不同） | `fs/procfs/tree.c:87`、`pid.c:44` | minix_proc_list 16B/行、minix_proc_data 72B——布局即契约（A-5） |
| X2 | IPC/LWIP/UDS 挂载：`kern.ipc`（盖 MIB 的 mock）、`net.inet`/`net.inet6`/`minix.lwip`、`net.local` 三家经 `rmib_register` 挂载 | `servers/ipc/main.c:91`、`net/lwip/mibtree.c:56,61,66`、`net/uds/stat.c:174` | 12 篇的挂载协议消费者 |
| X3 | 行为契约测试：test87（3662 行，root 运行，全行为：类型/权限/元标识符/远程/动态节点/对齐）、rmibtest（267 行，8 个远端拒绝场景+注册顺序）、t_sysctl（74 行，基础面） | `minix/tests/test87.c` 等 | 15 篇的测试子树即 test87 的考题面 |

---

## 2. 知识点全集

### 2.1 知识点池总表

说明：**类型**取 概念/机制/数据结构/接口协议/约束不变量/架构演进/工具工程/测试性质 八类；**来源**取 存量/新增。**主** = 唯一主讲述点。锚点均为本次执行实测。相关知识点已按"共生死"原则并row（如同函数族的四原语并一行），保迁移粒度同时控表宽。

#### 来自 00（3 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-001 | MIB 定位：sysctl(2) 归口服务，路由器而非仓库（自己能答的答、别人答的转发、谁都不答的诚实回错） | 概念 | 存量 | 00 §1 | `main.c:1-33`；`kern.c` 40+ 未实现槽 | 00 |
| K-002 | boot 两层语义 + MIB 排位理由（init 要用 sysctl、需超级用户权限） | 机制 | 存量 | 00 §2；01 §2.5 | `table.c:60`；`main.c:15-19,264-267` | 06 承抑制链，00 导航 |
| K-003 | 阶段设计原则（位置可回答/verdict-执行分离/禁前向/ARCH 三处一致）与文档导航 | 工具工程 | 存量 | 00 §5-§6 | 本蓝图 §4 | 00 |

#### 来自 01（10 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-010 | 主循环三拍 + 收发轻重（收失败 panic、发失败只打印、m_out 清零复用、无优雅关闭——唯一退出是 panic） | 机制 | 存量 | 01 §1.3 | `main.c:443-492`（:445-446,456,482-487） | 01 |
| K-011 | SEF 两回调**同体不同命**：fresh/restart 都挂 `mib_init`；重启=动态全丢仅保静态（与 DS STATEFUL 对照） | 机制 | 存量 | 01 §1.3、04 §1.3 | `main.c:415-428`（:420-424 注释） | 01 |
| K-012 | notify 拒绝：`is_ipc_notify(status)` 听内核判决（对照 DS 用 `is_notify` 宏猜号码）——**接收原语决定分发第一行** | 机制 | 存量 | 01 §1.3 | `main.c:449-454`；`com.h:92` | 01 |
| K-013 | 分发表：三信满号空间无死号；default 判调用形状（SENDREC→ENOSYS 否则 EDONTREPLY） | 接口协议 | 存量 | 01 §2.2 | `main.c:458-479`；`com.h:1022-1030` | 01 |
| K-014 | sysctl 六道解码门（形状/界/取道/old 配对宽容/new 配对严格/组装） | 机制 | 存量 | 01 §1.4、§2.3 | `main.c:291-353` | 01 |
| K-015 | ENOMEM 收尾换算方言：报完整长度+部分拷贝；错误+staged reslen（EEXIST 唯一产生处） | 约束不变量 | 存量 | 01 §1.4、§2.4 | `main.c:368-377`（:355-367 注释）；`mib.h:45`（call_reslen） | 01 |
| K-016 | EDONTREPLY（203）沉默哨兵语义 | 约束不变量 | 存量 | 01 §1.3 | `main.c:293,478,482` | 01 |
| K-017 | Rust：MibCall/Incoming/triage（收 status bool）/default_outcome/should_reply/五解码 verdict/SysctlOutcome+map_sysctl_reply（P2-2 结构化） | 架构演进 | 存量 | 01 §3-§4 | `dispatch.rs:41-254`（map_sysctl_reply 现 :254——01/02 篇引 :217 已漂移，E4） | 01 |
| K-018 | Rust：MibInitKind::{Fresh, RestartLossy}（同体不同命写进类型名） | 架构演进 | 存量 | 01 §3 D6 | `sef.rs:24`；`main.c:419-425` | 01 |
| K-019 | **新增**：Rust 主循环装配已落地——MibIpc trait（receive_status/send_nb/send_rec）/MibServer/run_once/run/MAX_CONSECUTIVE_RECV_FAILURES=64（SCHED/DS 共享纪律）+ transport.rs 双 seam（MibKernel/MibServices/SysTransport EIO 诚实桩/Recorder mock）——01 篇无此补记（E1/G1） | 架构演进 | **新增** | （缺） | `server.rs:65,82,85,142,547`；`transport.rs`；todo.md P1-1 ✅ | 01 |

#### 来自 02（13 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-030 | 三信 + IS_MIB_CALL 判定宏 + 号空间满无死号 | 接口协议 | 存量 | 02 §2.1 | `com.h:1022-1030` | 01 |
| K-031 | 共享号三件：COMMON_MIB_INFO 0xE04 / CALL 0xE05（MIB→服务）、REPLY 0xE81（服务→MIB） | 接口协议 | 存量 | 02 §2.1 | `com.h:597-598,613-622` | 02 |
| K-032 | `mess_lc_mib_sysctl` 七 lane 字段级 | 接口协议 | 存量 | 02 §2.2① | `ipc.h:424-433` | 02 |
| K-033 | `mess_mib_lc_sysctl` 单 lane（永远只报完整长度） | 接口协议 | 存量 | 02 §2.2② | `ipc.h:1548-1552` | 02 |
| K-034 | `mess_lsys_mib_register` 六 lane（挂载/卸载共用；卸载只读 root_id） | 接口协议 | 存量 | 02 §2.2③ | `ipc.h:1373-1382`；`remote.c:303` | 02 |
| K-035 | `mess_lsys_mib_reply`（req_id 恒 0 回显，非 0 即 EINVAL；status 可为 ERESTART） | 接口协议 | 存量 | 02 §2.2④ | `ipc.h:1384-1389`；`remote.c:344,361-362,425,463-464` | 12 |
| K-036 | `mess_mib_lsys_call` 十二 lane（三 grant+三长度零数据字节+user_endpt+flags+双版本） | 接口协议 | 存量 | 02 §2.2⑤ | `ipc.h:1554-1569` | 02 |
| K-037 | `mess_mib_lsys_info`（挂载时问名述双 grant） | 接口协议 | 存量 | 02 §2.2⑥ | `ipc.h:1571-1580` | 02 |
| K-038 | 交换格式 sysctlnode/sysctldesc（NetBSD VERS_1；字段顺序钉死、偏移随数据模型由 `__sysc_pad` 保证跨模型一致；NEXT_DESCR 循环非 sizeof 步进） | 接口协议 | 存量 | 02 §2.3 | `sys/sys/sysctl.h:1382-1454`；Rust：`minix-types/types/sysctl_abi.rs`（P1-3，96B/16B/128B/680B offset 锚定） | 02 |
| K-039 | 类型/标志/USERFLAGS/版本门/三内部重命名位（PARENT/VERIFY/REMOTE=ROOT/ALIAS/MMAP 改名不改值不暴露） | 接口协议 | 存量 | 02 §2.4 | `sysctl.h:92-153`；`mib.h:72-74` | 02（值）/03（行为） |
| K-040 | 元标识符七负数 + CREATESYM/MMAP → EOPNOTSUPP（A-9 wire 证据） | 接口协议 | 存量 | 02 §2.5 | `sysctl.h:158-164`；`tree.c:1377-1381` | 02 |
| K-041 | 顶层 id 与 MINIX3 扩展（CTL_MINIX=32 躲 NetBSD 未来 id；MAXID≤MINIX 编译期断言；TEST_*/MIB_*/PROC_* 子 id） | 接口协议 | 存量 | 02 §2.6 | `sysctl.h:169-183`；`minix/sysctl.h:17-59` | 02 |
| K-042 | 错误方言词典（ENOMEM 信封小/EEXIST 带 staged/EDONTREPLY/ENOSYS 单向/ERESTART 内部信号/EOPNOTSUPP 排除/分配失败禁 ENOMEM 报 EINVAL/EPERM 大数据写拒） | 约束不变量 | 存量 | 02 §2.7 | `tree.c:684,1046,1235-1238`；`main.c:208,236` | 02（词典）/99（收口） |
| K-043 | Rust：minix-types 四模块落位（sysctl.rs 值表/com.rs 号段/message.rs 六载荷 56B/ipc/mib.rs verdict）+ 单向约束进类型（decode_register 越界→Err(EDONTREPLY)） | 架构演进 | 存量 | 02 §3-§4 | `minix-types` 四文件；`message.rs` 六结构（02 篇引行号 3014-3196 经 P3-1 修正） | 02 |

#### 来自 03（10 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-050 | `mib_node` 全字段与三联合（指针/非指针分组为 live update；`mib_call` 五字段；AUTH/NOAUTH 位即鉴权缓存） | 数据结构 | 存量 | 03 §2.1 | `mib.h:40-50,188-220` | 03 |
| K-051 | 四格矩阵：PARENT×REMOTE（函数树/真爹/临时挂载/遮蔽挂载）——"先看类型再读字段"第一课 | 数据结构 | 存量 | 03 §1.4 | `mib.h:111-125` | 03 |
| K-052 | 挂载点内存复用：REMOTE 期间 csize/clen 位改存 eid/rcsize/rclen/rid（读错=把端口号当孩子数）；Rust ChildWindow/RemotePack 不相交建模 | 数据结构 | 存量 | 03 §1.4、§3 D3 | `mib.h:150-156,181-186`；`tree/node.rs` | 03 |
| K-053 | `mib_dynode` 变长尾巴：一块 malloc 装节点+名字+数据（或临时挂载点的描述）——OWNDATA/OWNDESC 归属的物理基础 | 数据结构 | 存量 | 03 §2.2 | `mib.h:238-249` | 03 |
| K-054 | 初始化宏族设位一览（MIB_NODE/ENODE/BOOL/INT/QUAD/*PTR/STRING/STRUCT/FUNC/INTV/INIT_ENODE/_RO/_RW/_P） | 接口协议 | 存量 | 03 §2.5 | `mib.h:252-324` | 04（体）/03（设位） |
| K-055 | 三计数器（nodes/objects/remotes）基线 1/0/0；增减点收敛 TreeCounts | 数据结构 | 存量 | 03 §1.5 | `tree.c:25-27,474,829,1501,1524-1525,1776`；`tree/node.rs` | 03 |
| K-056 | scratch 一页暂存（4096=int32 对齐；四处用途；单线程下全局安全） | 约束不变量 | 存量 | 03 §1.5 | `tree.c:21-23` | 03 |
| K-057 | 版本乐观锁：node_ver 根 1/建链继承/0=不检查/四处比对——"我看到的还是那棵树吗" | 约束不变量 | 存量 | 03 §1.6 | `tree.c:111,203,545,889,1030,1505,1531` | 03 |
| K-058 | IS_STATIC_ID 无符号判定语义（负 id 落链表路） | 约束不变量 | 存量 | 03 §2.4 | `tree.c:12` | 05（所有权） |
| K-059 | Rust：NodeType/NodeRole 枚举 + 11 谓词 + flag.rs/node.rs 全套词汇（A-2） | 架构演进 | 存量 | 03 §3-§4 | `tree/flag.rs`、`tree/node.rs` | 03 |

#### 来自 04（7 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-070 | 七顶层槽稀疏 id（1/2/4/6/8/11/32）、vendor 唯一可写、根可写无名内部 | 数据结构 | 存量 | 04 §2.1 | `main.c:46-62` | 04 |
| K-071 | 先空后填两阶段（跨文件 sizeof 无奈）→ Rust TOP_SLOTS 一步到位 | 架构演进 | 存量 | 04 §1.3、§3 D1 | `main.c:388-393`；`tree/static_tree.rs` | 04 |
| K-072 | mib_init 六拍线序（四线 kern→vm→hw→minix + tree_init + remote_init） | 机制 | 存量 | 04 §2.2 | `main.c:384-410` | 04 |
| K-073 | 点名四件事（跳空位/计数/版本继承/链父递归）+ 不碰动态链表 + 遍历用 size 不用 csize | 机制 | 存量 | 04 §1.3、§2.3 | `tree.c:1479-1536` | 04 |
| K-074 | 重启=静态树重建在此兑现（点名只数静态） | 约束不变量 | 存量 | 04 §1.3 | `tree.c:1479-1512`；`main.c:420-424` | 04 |
| K-075 | Rust：WireStep/WIRE_ORDER/judge_child/fold_static/check_parent verdict 化 | 架构演进 | 存量 | 04 §3 | `tree/init.rs` | 04 |
| K-076 | **新增**：arena 兑现现状——`MibTree::init()` 四线 build（镜像 C mib_*_init）、106 静态结点、静态窗口 reserve(max_id+1)+place 模型（防子树生长侵占父窗口的布局修正）、TreeCounts 转真实计数器——15 §4.4 是单一真值但 04 自身无兑现标记（轻） | 架构演进 | **新增** | （04 指向 15） | `tree/arena.rs:293,335`；todo.md P1-2 ✅ | 15（真值）/04（指向） |

#### 来自 05（5 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-080 | 查找双通道：静态 O(1) 一跳 + 动态有序链表 O(n) 早停；三检查序（负数直接未找到→静态命中即返→空位穿透不终止） | 机制 | 存量 | 05 §1.3、§2.1 | `tree.c:34-81` | 05 |
| K-081 | 动态链表存在的理由（用户自选号→稠密数组不可能→排序换早停）——约束推导设计范本 | 概念 | 存量 | 05 §2.1 注释解读 | `tree.c:59-67` | 05 |
| K-082 | prevpp 删链环钩子（NULL 兼"落空"与"不删"双职）→ Rust Lookup 三变体 | 机制 | 存量 | 05 §2.2、§3 D1 | `tree.c:35,53-54,73-74`；`tree/lookup.rs` | 05 |
| K-083 | 落空附赠插入点（Missing{insert_at}）——C 重走一遍的等价演进 | 架构演进 | 存量 | 05 §3 D2 | `tree/lookup.rs`；`tree.c:326-343`（scan 同理） | 05 |
| K-084 | Rust：find_static/scan_dynamic/find 总装（walker 生产消费已转正，10 补记） | 架构演进 | 存量 | 05 §4 | `tree/lookup.rs`；`walker.rs:141` | 05 |

#### 来自 06（9 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-090 | 两条核心约束：旧缓冲是上限非目标量（钳制截断）；新数据长度精确匹配否则视为未提供 | 约束不变量 | 存量 | 06 §1.3 | `main.c:120-141,172-185` | 06 |
| K-091 | 旧侧四原语（inrange/getoldlen/copyout 报 size 非 len/setoldlen） | 机制 | 存量 | 06 §2.1 | `main.c:90-152` | 06 |
| K-092 | 新侧三原语（getnewlen/copyin 精确/copyin_aux 断言即契约） | 机制 | 存量 | 06 §2.2 | `main.c:158-202` | 06 |
| K-093 | 字符串分页猜（页界取块、memchr 找终结、耗尽 EINVAL"装不下的字符串不是字符串"；buf=NULL 纯量长走 scratch） | 机制 | 存量 | 06 §2.3 | `tree.c:371-420` | 06 |
| K-094 | relay 两原语（旧授写 CPF_WRITE/新授读 CPF_READ；失败 EINVAL 永不 ENOMEM——方向从本服务视角命名） | 机制 | 存量 | 06 §2.4 | `main.c:204-252`（:208,236 两处注释） | 06 |
| K-095 | Rust io/copy.rs：Option 消哨兵/CopySpan 两回事有名/next_chunk 纯函数 | 架构演进 | 存量 | 06 §3 | `io/copy.rs` | 06 |
| K-096 | Rust io/relay.rs：RelayDir/RelayRegion/RELAY_FAIL 常量（注释会撒谎常量不会） | 架构演进 | 存量 | 06 §3 | `io/relay.rs` | 06 |
| K-097 | **新增**（补记已有）：执行半落地——Oldp/Newp 类型化（endpt/base/len/cursor）+ 动词方法 + RelayRequest::open/RelayGrant::close（消费所有权使双重 revoke 不可表示）+ present: bool 占位退役 | 架构演进 | 新增（已补记） | 06 P1-4 补记 | `io/copy.rs`；`io/relay.rs`；`transport.rs`；todo.md P1-4 ✅ | 06 |
| K-098 | mib_oldp/mib_newp 不透明体设计动机（可为小结果走回信消息免内核拷贝——预留演进） | 架构演进 | 存量 | 06 §2.1 前 | `main.c:64-83` | 06 |

#### 来自 07（6 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-100 | mib_authed 问一次到处用（双位皆空才问 PM getnuid；call_flags 即鉴权缓存别无他用） | 机制 | 存量 | 07 §1.3、§2.1 | `main.c:259-272`；`mib.h:49-50` | 07 |
| K-101 | 三种门：看的门（可见性=!PRIVATE‖authed）、写的门（READWRITE 杠→ANYWRITE-或-root 杠，ANYWRITE 不代 RW）、改结构门（先认人后拓扑） | 机制 | 存量 | 07 §1.4 | `tree.c:115,934,1389,1446-1458,500-511,848-853` | 07 |
| K-102 | 九处权限检查全表 + 拒绝词典只有 EPERM（EACCES 全树零出现——见到即外来户） | 约束不变量 | 存量 | 07 §2.2-§2.3 | `tree.c` 九处；`rg EACCES servers/mib/` 零命中 | 07 |
| K-103 | 私有的不对称：枚举可到名字（值清零），描述零长——静默隐藏不给枚举攻击留信号 | 约束不变量 | 存量 | 07 §1.4；11 §2.1 | `tree.c:115,121-132,934-935` | 07（门）/11（序列化侧） |
| K-104 | Rust：CallAuth 三态+resolve 恒等（惯例变结构）+ 三类门各一函数 | 架构演进 | 存量 | 07 §3 | `auth.rs` | 07 |
| K-105 | **新增**（补记已有）：auth::ask 执行半（PM 往返，fail-closed No） | 架构演进 | 新增（已补记） | 07 P1-4 补记 | `auth.rs`；todo.md P1-4 ✅ | 07 |

#### 来自 08（11 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-110 | 名字规则：C 符号风（非空/界内终结/字母数字下划线/首字符非数字） | 约束不变量 | 存量 | 08 §1.4、§2.1 | `tree.c:247-266` | 08 |
| K-111 | mib_scan 三段（静态撞车→动态边走边判→插入点之后尾查名——早停省编号搜索不省名字搜索）；自动号 max(1024, size) 躲静态区 | 机制 | 存量 | 08 §1.4、§2.2 | `tree.c:277-359`（:293-297,321-324,345-354） | 08 |
| K-112 | 创建十四道校验按成本升序（分配前能查的全查完——为减释放路径非省时间）；撞车 EEXIST 拷现有节点+setoldlen | 机制 | 存量 | 08 §1.3、§2.3 | `tree.c:486-665`（:529-535,657-664） | 08 |
| K-113 | 创建校验细节族：版本门（父或根或零）/标志白名单（USERFLAGS+UNSIGNED）/IMMEDIATE-OWNDATA 组合/RW 消毒/类型-尺寸匹配/func-parent 必空 | 约束不变量 | 存量 | 08 §2.3 | `tree.c:536-640` | 08 |
| K-114 | 分配一块装头+名+数据；失败 **EINVAL 非 ENOMEM**（:684 注释）；bool 消毒两处（创建 :707,745-748） | 约束不变量 | 存量 | 08 §2.3 | `tree.c:679-762` | 08 |
| K-115 | mib_add/mib_remove 差量（csize/clen/nodes/--；动态摘链 free、静态原地清零；**数据区永不单独释放——禁止检查 OWNDATA**） | 机制 | 存量 | 08 §1.5、§2.4 | `tree.c:456-481,784-833`（:799-804 must not） | 08 |
| K-116 | 销毁守卫链（特权→父可写→newp 指认→版本门→存在→非永久→NODE 三查：非挂载 EBUSY/非函数 EPERM/无孩子 ENOTEMPTY→版本→名字）+ 先拷贝后删除（拷失败照删，NetBSD 行为） | 机制 | 存量 | 08 §1.5、§2.4 | `tree.c:838-916`（:901-905 注释） | 08 |
| K-117 | mib_upgrade 版本递增（根+1 跳零沿父链写） | 机制 | 存量 | 08 §1.5 | `tree.c:426-449`（:440-441） | 08 |
| K-118 | Rust：check_name→Option/ScanOutcome 三态/DestroyRefusal 六变体+码/RemoveDelta 四项/next_root_ver 回绕跳零 | 架构演进 | 存量 | 08 §3 | `tree/dynamic.rs`、`tree/version.rs` | 08 |
| K-119 | **新增**：P2-1 补齐的三处门（create 溢出门 create_csize_ok/create 版本门 staged_vers_ok 与 query 拷入门共实现）——08/11 篇正文有符号行但"曾经缺过"的修复记录只在 todo | 架构演进 | 新增 | 08 §4.2 表（部分） | `tree/dynamic.rs`；`tree/version.rs`；todo.md P2-1 ✅ | 08 |
| K-120 | **新增**：A-3 内存策略——`heap.rs::MibBudget` 记账式 256KiB 字节预算（claim 先于分配/耗尽 BudgetExhausted 调用方映射 EINVAL/临时挂载点例外映射 ENOMEM/reset 承接 RestartLossy；有意偏离 DS 定长槽池：分配尺寸跨三个数量级）——plan A-3 行与 todo P2-4 有，stage 文档无承载（G2） | 架构演进 | **新增** | （缺） | `heap.rs:35,51`；todo.md P2-4 ✅ | 08 |

#### 来自 09（8 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-130 | 不在位更新禁令（stage 先暂存验好再落子——拷贝中途失败不损坏节点） | 约束不变量 | 存量 | 09 §1.3 | `tree.c:1171-1181` | 09 |
| K-131 | lane 判定（getptr：立即体内/外存指针/串+立即不可能/未知类型——NULL 双职） | 机制 | 存量 | 09 §2.1 | `tree.c:1097-1124` | 09 |
| K-132 | 读：串报 live+1 其余报宽、超 SSIZE_MAX 拒、永远报全长 | 机制 | 存量 | 09 §2.2 | `tree.c:1130-1154` | 09 |
| K-133 | 写：尺寸判（非串精确/串封顶/未知拒）→ stage 选择（newlen+1>scratch 非特权 EPERM；特权堆败 EINVAL）→ 拷入 → verify → 落子（bool 消毒单字节中转+编译期 sizeof 断言；串补终结三条规则）→ 释 stage | 机制 | 存量 | 09 §1.3、§2.3 | `tree.c:1159-1300`（:1220-1242,1255-1264,1265,1274-1287） | 09 |
| K-134 | 合成 readwrite（读短路/写替换/报读长）——handler 复用唯一入口 | 机制 | 存量 | 09 §2.4 | `tree.c:1308-1325` | 09 |
| K-135 | Rust：PtrLane/Stage 枚举/sanitize_bool/finalize_string/readwrite_combine | 架构演进 | 存量 | 09 §3-§4 | `data/readwrite.rs` | 09 |
| K-136 | **新增**（补记已有）：walker 的 Readwrite 终态执行（stage 两半+verify+bool 消毒+buf 钳制全接线） | 架构演进 | 新增（已补记） | 10 P1-2 补记（9 的执行面） | `walker.rs`；todo.md P1-2 ✅ | 10 |
| K-137 | 数值细节：cp_time 三形状（带 cpu 号/无号装得下一份=求和/更大=逐个）；kern_drivers 的 pts 兼容别名（bmajor=-1 行）；root_device 截断一行细读——13 篇承载（见 K-150 族） | 机制 | 存量 | 13 §2.2 | `kern.c:134-187,222-281,95-114` | 13 |

#### 来自 10（8 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-140 | 每轮恰好消耗一个分量（元标识符轮除外且须末位）——三条离开路径（调出去/报错/名尽落父 EISDIR） | 机制 | 存量 | 10 §1.3 | `tree.c:1346-1349,1360-1382,1472-1473` | 10 |
| K-141 | sysctl 次主线全景路径图（本篇为家） | 概念 | 存量 | 10 §1.3 | 本蓝图 §1.4 | 10 |
| K-142 | 元 switch 四投 + CREATESYM/MMAP/野负 EOPNOTSUPP | 机制 | 存量 | 10 §2.1 | `tree.c:1368-1382` | 10 |
| K-143 | 三问定形状（叶看 VERIFY 优先/非叶缺 PARENT 即函数——省字节双关）；函数叶也查剩余名字（:1437 先于一切） | 机制 | 存量 | 10 §1.4、§2.1 | `tree.c:1402,1425-1431,1437-1438` | 10 |
| K-144 | 快照先行（can_restart 在调用前记——节点可能调用中消失）+ ERESTART 三去向 | 约束不变量 | 存量 | 10 §1.4 | `tree.c:1404-1416` | 10（判）/12（本体） |
| K-145 | 注释里三句实话（handler 子路可用负 id 如 PROC2/写门时机商榷的诚实债/省字节双关） | 概念 | 存量 | 10 §2.2 | `tree.c:1354-1359,1441-1444,1418-1424` | 10 |
| K-146 | Rust：MetaOp/judge_meta/LevelFacts+judge_level（九参聚合防转置，P2-2）/resolve_shape/judge_remote_result/terminal_code/EISDIR_EMPTY | 架构演进 | 存量 | 10 §3-§4 | `tree/dispatch.rs:25,98,125,154,190,206` | 10 |
| K-147 | **新增**（补记已有）：walker.rs 执行体落地——sysctl() 每轮 find→can_see→judge_level→动作；meta 四 op/Readwrite/CallFunc registry/RemoteCall 三去向全接线；10 集成测试 | 架构演进 | 新增（已补记） | 10 P1-2 补记 | `walker.rs:141`；todo.md P1-2 ✅ | 10 |

#### 来自 11（8 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-150 | 节点快照序列化（copyout_node）：越界长度照计/标志剥内部三位/版本随快照/立即值可见才给/NODE 特则（报 sizeof(scn)、远端报缓存窗口不进服务问、函数结点置 SYSCTL_NODE_FN 假地址防 ASR 泄漏） | 机制 | 存量 | 11 §2.1 | `tree.c:90-173`（:107-108,115,135-169） | 11 |
| K-151 | QUERY 枚举：先静态后动态**不排序**（libc 自排）、版本门（拷入侧 VERS_1+父或根或零） | 机制 | 存量 | 11 §2.2 | `tree.c:179-239`（:194-204,207-212） | 11 |
| K-152 | 描述流序列化（copyout_desc）：私有零长非错/长=串+1 无串亦 1/scratch 断言装下/4 字节对齐间隙是用户数据 | 机制 | 存量 | 11 §2.3 | `tree.c:925-966`（:934-935,937-941,961-965） | 11 |
| K-153 | DESCRIBE：设描述六杠一次性（可见性→特权→非挂载 EBUSY→无旧述 EPERM→非永久 EPERM→版本匹配）+ strdup 败 EINVAL 非 ENOMEM + OWNDESC 标记 + 回显新述；读流静态动态两段 | 机制 | 存量 | 11 §2.4 | `tree.c:972-1090`（:994-1051,1043-1048） | 11 |
| K-154 | Rust：export_flags/版本门/desc_len/packed_size/SetDescRefusal 六拒/SYSCTL_NODE_FN 双钉（minix-types 值+本篇行为） | 架构演进 | 存量 | 11 §3-§4 | `query.rs`、`describe.rs`、`minix-types sysctl.rs` | 11 |
| K-155 | **新增**：sysctl_abi.rs 消费关系——copyout_node/copyout_desc 的执行体组装字节应消费 `SysctlNode`/`SysctlDesc`（P1-3 落地；describe.rs 的 DESC_HEADER/DESC_ALIGN 已改从 SysctlDesc 派生消灭双真相源）——11 篇正文未点名（G7，轻） | 架构演进 | **新增** | （11 未点名） | `minix-types/types/sysctl_abi.rs`；todo.md P1-3 ✅ | 11 |
| K-156 | 版本门双实现同形不同址（query :194-204 vs create :536-546 各调各处行为同；Rust 侧 staged_vers_ok 共一条实现 P2-1） | 约束不变量 | 存量 | 11 §4.4；08 §4.2 | `tree.c` 两处；`tree/version.rs` | 08（实现）/11（引用） |
| K-157 | 顺序不保证是契约（用户态 sysctlgetmibinfo 自排——21 的履约点） | 约束不变量 | 存量 | 11 §2.2；21 §2.4 | `tree.c:207-212`；`sysctlgetmibinfo.c` | 11（服务侧）/21（客户端侧） |

#### 来自 12（12 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-160 | 端点表 32 槽三字段（endpt/nodes 链头/label 16B）；mib_remote_init 复位 | 数据结构 | 存量 | 12 §2.1 | `remote.c:24-49`（:25,28） | 12 |
| K-161 | 死亡是常态（无主动通知——DS 缺订阅原语的 TODO；转交失败即死亡；同 label 新端点=老的死了） | 概念 | 存量 | 12 §1.4 | `remote.c:6-22,128-136` | 12 |
| K-162 | mib_down 清场（先存 next 再卸——unmount 可能释放当前节点） | 机制 | 存量 | 12 §2.1 | `remote.c:55-74`（:64-68） | 12 |
| K-163 | mib_get_label 经 DS（ds_retrieve_label_name；init 有 label 故非严格 is-service 测试 TODO；超 16 ENAMETOOLONG） | 机制 | 存量 | 12 §2.2 | `remote.c:81-104`（:87 TODO） | 12 |
| K-164 | 注册两函数分层（信封层单向门 SENDREC→ENOSYS 防交叉死锁+label 败/路径>8 静默 EDONTREPLY；策略层槽定位四去向/同根 id 拒/先占位/mount 败无其他挂载则清槽/头插链） | 机制 | 存量 | 12 §2.2 | `remote.c:197-233,109-192` | 12 |
| K-165 | 注销两函数（按根 id 摘链先摘后卸；链空槽释放；deregister 同款单向门只读 root_id） | 机制 | 存量 | 12 §2.3 | `remote.c:239-307`（:272-275,303） | 12 |
| K-166 | remote_info 问名述（双 grant CPF_WRITE 败一撤一；COMMON_MIB_INFO；回信检查） | 机制 | 存量 | 12 §2.4 | `remote.c:316-365` | 12 |
| K-167 | remote_call 转交（三 grant 后授先撤；req_id 恒 0；flags=!!authed TODO 未定义标志集；短名也走 grant 不优化；IPC 错→mib_down+ERESTART；回信检查；ERESTART→do_deregister 请辞交叉） | 机制 | 存量 | 12 §2.4 | `remote.c:378-477`（:396-413,415-421,425,434,441-446,455-459,461-464,473-474） | 12 |
| K-168 | mib_mount 挂载策略（顶层禁挂唯一安全类限制 TODO 白名单制；标志窗四位子集/孩子窗 4096；路径 walk 真本地非私有防服务拦截特权写；盖旧需标志精确匹配+无动态孩子 EBUSY；临时点 remote_info 问名述+名体检+scan 名冲+一块分配**ENOMEM 唯一合法点**；置 REMOTE 四字段 remotes++） | 机制 | 存量 | 12 §2.5 | `tree.c:1543-1780`（:1572-1576,1583-1598,1610-1640,1654-1680,1681-1767,1741-1744,1769-1778） | 12 |
| K-169 | mib_unmount 恢复（遮蔽点去 REMOTE+csize=size+重数静态 clen+dcptr=NULL+版本 bump；临时点找父链环 mib_remove；remotes-- 断言>0） | 机制 | 存量 | 12 §2.6 | `tree.c:1789-1842`（:1804-1823,1829-1837,1840-1841） | 12 |
| K-170 | Rust：SlotVerdict 四去向/Label 定长值/MountHead 三拒/ReplyCheck 三态/recount_clen+is_obscuring 恢复数学 | 架构演进 | 存量 | 12 §3-§4 | `remote.rs`、`tree/mount.rs` | 12 |
| K-171 | **新增**：mib_get_label 执行半现状——`MibServices::ds_retrieve_label` seam 已备（transport.rs），真实端委托 minix-sys ds.rs，通电挂 E-DSWIRE/E1——12 篇正文有 seam 声明但无执行现状（轻，随 E1 批） | 架构演进 | **新增** | （12 未点名执行现状） | `transport.rs`；`os/libs/minix-sys/src/ds.rs`；todo.md P1-1 ✅ | 12 |

#### 来自 13（9 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-180 | 子树三分类写法（常量叶/函数节点/占位子树）——13 是写法篇，14/15 照做 | 概念 | 存量 | 13 §1.3 | `kern.c` 全文 | 13 |
| K-181 | verify 双星：securelvl 只升不降（假实现方向对，TODO）+ forkfsleep 0..=20000ms（MAXSLP×1000，NetBSD 规则） | 约束不变量 | 存量 | 13 §2.1 | `kern.c:17-30,208-217`；`sys/param.h:447` | 13 |
| K-182 | 十个函数节点族（clockrate 四项同值+tickadj"我觉得" TODO/profiling 恒 EOPNOTSUPP 有效约定/hardclock_ticks 单调/ root_device 截断+补零/ccpu 直拷/cp_time 三形状/consdev 合成+注释代码细微出入/drivers 列运行中+pts 别名/boottime timeval/ipc_info mock 单层检查） | 机制 | 存量 | 13 §2.2 | `kern.c:35-315` | 13 |
| K-183 | kern 数据表 45 填 39 空（排除约定四类：未实现/废弃/设备方案不兼容/NetBSD 内部）；问到空位=ENOENT 不是"没实现"——排除约定只需稳定回答 | 数据结构 | 存量 | 13 §2.3-§2.4 | `kern.c:332-498` | 13 |
| K-184 | kern.ipc 占位子树（mock 四节点恒 EOPNOTSUPP；等 IPC 服务远端覆盖——12 的覆盖机制消费者） | 机制 | 存量 | 13 §1.3、§2.2 | `kern.c:301-329`；`servers/ipc/main.c:91` | 13 |
| K-185 | Rust：KernEntry/KernKind 形状枚举/KERN_ENTRIES 45 行/KERN_UNIMPLEMENTED 39 清单互斥测试（45+39=84）/构建期值只记名字 | 架构演进 | 存量 | 13 §3-§4 | `subtree/kern.rs:114,355` | 13 |
| K-186 | 对照参照：NetBSD 编号来源/Linux /proc/sys 弱化/Redox scheme 分层（路径语义与数据来源分开——本篇拆"要"与"算"的依据） | 概念 | 存量 | 13 §1.4、§3 | 类比无锚 | 13 |
| K-187 | **新增**：kern handler 执行现状——walker 的 func registry 已实连 hardclock_ticks/clockrate/profiling 三纯本地函数；跨服务拉取型（root_device/drivers/boottime/cp_time）经 MibServices 诚实 EIO 挂对端（E-MIBPROD 等）——13 篇无执行现状（轻） | 架构演进 | **新增** | （13 未记） | `walker.rs` func registry；todo.md P1-2 ✅ | 13 |
| K-188 | consdev 注释与代码细微出入（"不支持 32 位旧请求"注释但无长度检查）——已声明的诚实记录 | 约束不变量 | 存量 | 13 §4.4 | `kern.c:201-202` | 13 |

#### 来自 14（7 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-190 | 负载均值数学：6s×150 槽=15 分钟史（接口定死改了断消费端）；窗口 10/50/150 槽；欠填扣减；刻度 100；取模绕回 | 机制 | 存量 | 14 §1.3、§2.1 | `vm.c:12-73`（:37-39,43,55,65-70）；`minix/type.h:88-95` | 14 |
| K-191 | uvmexp2 七字段部分填充（"top 够用先"声明现状）+ 页位移搜索（2 的幂否则 0）+ 两个借空位字段（filepages/unused1 最大连续物理——NetBSD 征用就得搬家） | 机制 | 存量 | 14 §1.4、§2.2 | `vm.c:78-117`（:93-97,100-104,108-114） | 14 |
| K-192 | 宽窄双版本三步（先转 64 位再乘防溢出/用户内存减内核占用饱和到零/32 位截到上限）——顺序不能错 | 机制 | 存量 | 14 §1.5、§2.4 | `hw.c:18-76`（:29,57,62-65,32-35） | 14 |
| K-193 | ncpu 配置数（构建宏）与在线数（运行时问 machine）两事实不合并不互推；架构字符串条件编译第三种编译失败→Rust 新增 x86_64 行（A-ARCH） | 约束不变量 | 存量 | 14 §1.5、§2.5 | `hw.c:81-95,5-13,102` | 14 |
| K-194 | vm 表 4 填 9 空 + hw 表 10 填 6 空（排除约定；maxslp 同源 20/uspace=0 有理由的零——Minix 进程无内核栈） | 数据结构 | 存量 | 14 §2.3、§2.5 | `vm.c:120-144`；`hw.c:98-130` | 14 |
| K-195 | Rust：vm.rs 窗口常量+四函数/位移搜索参数化指针宽度/hw.rs 宽乘饱和减截断/版本判断/机器字符串常量 | 架构演进 | 存量 | 14 §3-§4 | `subtree/vm.rs`、`subtree/hw.rs` | 14 |
| K-196 | 数据源依赖：sys_getloadinfo/vm_info_stats/vm_info_usage/sys_getmachine/sys_hz——执行归 transport/services（A-12），本篇只算 | 约束不变量 | 存量 | 14 §1.2、§4.4 | `vm.c:27,87`；`hw.c:26,54,59,89` | 14 |

#### 来自 15（7 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-200 | 测试子树 12 节点字面量级（int 0x01020304 字节序/bool/quad/string 16B/struct 12B/private -5375/anywrite/deleteme/secret 私有子表 value=12345 行李箱玩笑/permanent/destroy1/destroy2）——每节点是 test87 断言 | 测试性质 | 存量 | 15 §1.3、§2.1 | `minix.c:9-43` | 15 |
| K-201 | 空描述对齐自检（"连描述数组的对齐本身都被测，轻动既有字段"；加新字段可以改旧字段变红）——考题自身也是被测对象 | 测试性质 | 存量 | 15 §1.3 | `minix.c:14-18` | 15 |
| K-202 | MINIX_TEST_SUBTREE 编译期开关（关=子树不存在非拒绝访问；生产可关但 test87 通不过） | 约束不变量 | 存量 | 15 §1.3、§2.1 | `mib.h:23`；`minix.c:5,69-73` | 15 |
| K-203 | 统计三指针叶（nodes/objects/remotes 照实时值——新鲜与一致不可兼得树选新鲜） | 机制 | 存量 | 15 §1.4、§2.2 | `minix.c:47-57`；`tree.c:25-27` | 15 |
| K-204 | proc 两门（list/data）本体在 20——握手点；顶层三位置 + **lwip 不留位置是设计**（运行时挂载，静态留位会冲突） | 机制 | 存量 | 15 §1.5-§1.6、§2.3 | `minix.c:59-79`（:78 注释） | 15 |
| K-205 | Rust：TestKind 带值枚举/ABSENT_LWIP 断言/MibStat/ProcDoor/MinixSlot | 架构演进 | 存量 | 15 §3-§4 | `subtree/minix.rs` | 15 |
| K-206 | **新增**：arena 兑现单一真值（15 §4.4 已记"已兑现 P1-2"）——04/08/13 的承诺全部指此；本篇已是正确一方 | 架构演进 | 存量（已对） | 15 §4.4 | `tree/arena.rs`；todo.md P1-2 ✅ | 15 |

#### 来自 16（7 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-210 | 拿表三规矩：每 tick 至多一次（几百 KB 拷贝贵）/失败闩锁永不重试（"基本不可能自己好"——写进代码的判断）/拿前先记失效（不出现半新半旧） | 机制 | 存量 | 16 §1.3、§2.2 | `proc.c:46-137`（:53-58,66-69,72） | 16 |
| K-211 | 三张表（proc_tab NR_TASKS+NR_PROCS/mproc_tab/fproc_tab NR_PROCS）+ 四常量（EXTRA_PROCS=8 两次调用间出生余量/HASH_SLOTS=NR_PROCS/4/NO_SLOT=-1/tabs_updated+tabs_valid） | 数据结构 | 存量 | 16 §2.1 | `proc.c:18-39` | 16 |
| K-212 | 双魔数逐行检查（PMAGIC 0xC0FFEE1/MP_MAGIC 0xC0FFEE0；fproc_light 无魔数不查） | 约束不变量 | 存量 | 16 §2.2 | `proc.c:81-87,97-103`；`const.h:164`；`pm/mproc.h` | 16 |
| K-213 | PID 哈希（pid≤0 不进索引/取模/头插链/顺链比对） | 机制 | 存量 | 16 §2.2-§2.3 | `proc.c:121-158` | 16 |
| K-214 | ticks_to_timeval（64 位中间算）+ fill_wmesg（ANY/SELF/NONE 三特殊端点+槽位有效性+带括号 ipc 直连/不带 VFS 转接） | 机制 | 存量 | 16 §2.4-§2.5 | `proc.c:163-216` | 16 |
| K-215 | Rust：PullVerdict 三态/哈希纯函数/链条过期防御（切片建模附带好处） | 架构演进 | 存量 | 16 §3 | `proc/tables.rs` | 16 |
| K-216 | **新增**（补记已有）：Tables::update 执行半（三源拉取序 kernel→PM SI_PROC_TAB→VFS SI_PROCLIGHT_TAB+闩锁+快照拷贝+scratch 启动分配）；kinfo 行填充挂 E-MIBPROD | 架构演进 | 新增（已补记） | 16 P1-5 补记 | `proc/tables.rs`；todo.md P1-5 ✅ | 16 |

#### 来自 17（9 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-220 | 五状态判定优先级（ZOMB→DEAD→STOP→RUN→SLEEP；顺序即优先级——又僵死又能运行先报僵死） | 机制 | 存量 | 17 §1.3、§2.2 | `proc.c:243-253` | 17 |
| K-221 | wchan 低八位类别编码（0x00 内核任务/0x01 RTS/0x02 PM/0x03 VFS/0x04 MIB 装饰/0xff 端点；高位类内信息；值无所谓只要非零不重叠） | 接口协议 | 存量 | 17 §1.4 | `proc.c:264-274,287-389` | 17 |
| K-222 | 表间不一致现实（三表各自内部一致、彼此不必一致）→ PM/VFS 先说、IPC/RTS 后说；**后算覆盖地址、先添标记保留（L_SINTR 只或不清）** | 约束不变量 | 存量 | 17 §1.3、§2.2 | `proc.c:279-286,324-329,337-373`（:340,349 覆盖；:328,371 添标） | 17 |
| K-223 | C 重复条件笔误（:352-353 RTS_SIGNALED 写两遍第二遍不可达——不影响结果；Rust 按或一次） | 约束不变量 | 存量 | 17 §2.2 | `proc.c:350-363`（:352-353） | 17 |
| K-224 | 三填写函数（common：lid=端点号/swtime/slptime 内核任务零/priority 双处/rtime/cpuavg 三值承认与 NetBSD 不同；kern：负 pid 回绕/wchan 高位补零非符号扩展/名字统一 "kernel"；user：flag 常驻+状态带出） | 机制 | 存量 | 17 §2.3 | `proc.c:398-504` | 17 |
| K-225 | 查询入口规矩（namelen=3/pid<-1 等参数错/elsz 取小向前兼容/内核段 pid≤0 且 =0 早返——"内核任务是零号进程的线程"/僵尸不进普通段 ESRCH/预留仅全量估计+8 步） | 机制 | 存量 | 17 §2.4 | `proc.c:509-593`（:520-528,536-541,544,551,564-566,580,586,589-590） | 17 |
| K-226 | kinfo_lwp 布局已钉（P1-3：128B offset 断言 sysctl_abi.rs）——17 §1.2 有补记 | 接口协议 | 存量+新增 | 17 §1.2 P1-3 补记 | `sysctl_abi.rs`；`sys/sys/sysctl.h:647-673` | 17 |
| K-227 | Rust：清醒四 bool 参/vfs 阻塞解码枚举/等谁三情况/零频率回零/参数检查 | 架构演进 | 存量 | 17 §3-§4 | `proc/lwp.rs` | 17 |
| K-228 | LSIDL(1)/LSONPROC(7) 永不出（前者每 CPU 自事/后者一律报 RUN） | 约束不变量 | 存量 | 17 §1.3 | `proc.c:243-253` 全路径；`sys/sys/lwp.h:279-285` | 17 |

#### 来自 18（8 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-230 | 先算 LWP 再复制（:610-618 注释——两边都有字段以 17 为准不另算） | 机制 | 存量 | 18 §1.3 | `proc.c:600-652`（:616-617） | 18 |
| K-231 | proc2 增量三样：内存用量（vm_info_usage 失败忽略填零；howmany 折页）、身份信息（pid/ppid/sid/pgid/tty/四 uid/gid/补充组截 16/信号三表/nice+NZERO）、内核伪进程（pid=0/stat=SLEEP/nlwps=NR_TASKS/KERNEL 槽——CPU 均值才对） | 机制 | 存量 | 18 §1.3、§2.1-§2.2 | `proc.c:619-651,657-682`（:621,632 TODO iticks,677-681） | 18 |
| K-232 | user 填写细节族（boottime 拿不到 panic/僵尸 tty=NO_DEV/eflag 两项/job control TODO/exitsig TODO/TAINTED→P_SUGID/tracer→P_TRACED/启动时间=boottime+started/僵尸 nlwps=0/状态映射五支：**DEAD 显示成 ZOMB 因 ps 不认**） | 机制 | 存量 | 18 §2.3 | `proc.c:687-786`（:699-700,707,712,715,734-736,745-747,753,760-782,777） | 18 |
| K-233 | 九种过滤（ALL/PID/SESSION/PGRP/TTY/UID/RUID/GID/RGID）+ 内核匹配规则（六种 arg==0 出/TTY 无终端出/未知 EINVAL）+ 行过滤（SESSION 比 procgrp TODO/tty 三规则：REVOKE 一行不出 TODO/僵尸不读 fproc 槽按 NODEV——**见 E7：C 代码方向反了**） | 机制 | 存量 | 18 §1.3、§2.4 | `proc.c:815-833,854-907`（:865,871-875） | 18 |
| K-234 | 参数与预留（namelen=4/elsz 取小/单 pid 不留 EXTRA——最多一行不会变多） | 约束不变量 | 存量 | 18 §2.4 | `proc.c:802-836,842,909-910` | 18 |
| K-235 | Rust：过滤枚举解码/内核与行匹配分函数/哨兵值参数化/状态映射函数/两组标志组装/ProcIdentity 聚合（P3-2 补记） | 架构演进 | 存量 | 18 §3-§4 | `proc/proc2.rs` | 18 |
| K-236 | kinfo_proc2 布局已钉（P1-3：680B offset 断言） | 接口协议 | 存量+新增 | 18 §3 P1-3 补记 | `sysctl_abi.rs`；`sysctl.h:383-495` | 18 |
| K-237 | **新增**（勘误载体 E7）：proc.c:875 的 `tty = (zombie) ? fproc_tab[mslot].fpl_tty : NO_DEV` 与 :707 `(!zombie) ? fp->fpl_tty : NO_DEV` 方向相反——:873 注释与 :707 正确实现都表明意图是"僵尸不读 fproc 槽"，:875 实际在 zombie 时才读（**C 源疑似 bug，模式 78 未标注**；18 篇按意图记录未标注实况；Rust proc2.rs 取法需核对并留字据） | 约束不变量 | **新增** | （缺标注） | `proc.c:873-875` vs `proc.c:707` | 18 |

#### 来自 19（7 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-240 | 攻击模型与预算（两字节串跨两页可骗拷 1G→字符串页预算 (ARG_MAX/PAGE_SIZE)×2；向量页线性不计） | 约束不变量 | 存量 | 19 §1.3、§2.3 | `proc.c:1023-1029,1051` | 19 |
| K-241 | 四查询两走法（NARGV/NENV 只读 ps_strings 两数字；ARGV/ENV 三步：pss→向量→字符串）+ 估计长度直接报上限不走（ARGV/ENV）/查个数报 4 字节 | 机制 | 存量 | 19 §1.3、§2.1-§2.2 | `proc.c:933-1002`（:939-947 EOPNOTSUPP 非 EINVAL，:958-960,984-998） | 19 |
| K-242 | 上限数学（min(帧长, ARG_MAX) 按页取整——setproctitle 指针出帧不超 2 页被取整吸收；比 NetBSD blindly 报 ARG_MAX 准） | 机制 | 存量 | 19 §2.2 | `proc.c:967-981` | 19 |
| K-243 | 逐页走七停条件 + 三页缓冲（向量/字符串/输出攒批）+ 页复用三情况（向量页/已存串页/花预算拷新页）+ 分段（memchr 含零截断/不含整页续）+ 段截到剩余 | 机制 | 存量 | 19 §2.3 | `proc.c:1004-1171`（:1014-1021,1040-1041 零页回 0 非错,1065-1070,1086-1102,1114-1125） | 19 |
| K-244 | 截断方言：**唯此一调用截断返回实际长度而非 ENOMEM+全长（libkvm 依赖）**；截断数据结尾未必有零（调用方自理）；TODO 串应跟向量后可从向量头拷 | 约束不变量 | 存量 | 19 §2.3 | `proc.c:1031-1038,1062-1063` | 19 |
| K-245 | Rust：查询枚举/上限预算函数/起步检查/页定位三变体/分段截断函数（页长上限取参） | 架构演进 | 存量 | 19 §3-§4 | `proc/proc_args.rs` | 19 |
| K-246 | ps_strings 位置（帧顶 mp_frame_addr+mp_frame_len-sizeof(pss)）与 exec 帧语义 | 数据结构 | 存量 | 19 §2.2 | `proc.c:962-965`；`sys/exec.h` | 19 |

#### 来自 20（6 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-250 | 整表快照（PROC_LIST）：扫 PM 表/在用且 pid>0/四字段（MPLF_IN_USE+ZOMBIE/pid/euid/egid）/估计报整表大小不拿表 | 机制 | 存量 | 20 §2.1 | `proc.c:1177-1211` | 20 |
| K-251 | 单进程快照（PROC_DATA）：namelen=1/十一字段/**负 PID=内核任务**（pid<0→kslot=pid+NR_TASKS 超 ESRCH；pid=0 ESRCH——与 KERN_LWP 的 -1=全列表两套规矩并存不可混） | 约束不变量 | 存量 | 20 §1.3、§2.2 | `proc.c:1216-1288`（:1238-1242 注释） | 20 |
| K-252 | 标记优先级链（ZOMBIE→STOPPED→RUNNABLE）与 17 同一套；系统服务位独立（PRIV_PROC→MPDF_SYSTEM）；名字两源（普通抄 PM 名/任务抄内核名）；管理标记 pid>0 才用本行 | 机制 | 存量 | 20 §1.3、§2.2 | `proc.c:1261-1285` | 20 |
| K-253 | 两结构布局契约（16B/72B；"不是对外接口的一部分，只给 ProcFS 用"≠可随便改——ProcFS 逐字段读）；ProcFS 消费者锚点 | 接口协议 | 存量 | 20 §2.3 | `minix/sysctl.h:61-86`；`fs/procfs/tree.c:87`、`pid.c:44`；`minix-types sysctl.rs:177-220`（offset 级锚定 P1-3） | 20 |
| K-254 | Rust：行取舍/负号判断/槽位换算/标记组装/名字来源函数 + 排布值表大小测试 | 架构演进 | 存量 | 20 §3-§4 | `proc/minix_proc.rs`；`minix-types sysctl.rs` | 20 |
| K-255 | 多一层设计辩护（ProcFS 不自读三家表——拿表规矩复用不值得每消费者写一遍；Linux/Redox 对照） | 概念 | 存量 | 20 §1.4、§3 | 类比无锚 | 20 |

#### 来自 21（6 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-260 | 外部契约定位（A-10 不重写 libc；履约点在服务侧 01/02/10/11） | 概念 | 存量 | 21 §1.2、§4 | plan A-10 | 21 |
| K-261 | CTL_USER 本地子树分流（第一个数=USER 本地办不出进程；编译期常量不值得跨服务消息；本地表不收写） | 机制 | 存量 | 21 §1.3、§2.2 | `libc/gen/sysctl.c:78-109`（:80-86） | 21 |
| K-262 | 长度规矩两边一致（ENOMEM+完整长度在 libc 侧同式）——两边不一致则本地能跑的远端跑不通 | 约束不变量 | 存量 | 21 §1.3、§2.1 | `sysctl.c:78-109` | 21 |
| K-263 | 失败也抄 oldlen（__sysctl NetBSD 怪规矩；消息层失败抄回的是垃圾但成功与 ENOMEM 两种情况是准的；sysctl(8) 靠它） | 约束不变量 | 存量 | 21 §1.4、§2.3 | `libc/sys/__sysctl.c` 尾部 | 21 |
| K-264 | sysctlgetmibinfo（611 行最重）：QUERY+DESCRIBE 走全树+自排序（服务顺序不保证）——查询与描述必须能遍历整棵树是服务侧履约点；sysctlnametomib 的 FreeBSD 兼容包袱吐槽 | 机制 | 存量 | 21 §1.4、§2.4 | `sysctlgetmibinfo.c`（611）；`sysctlnametomib.c` | 21 |
| K-265 | 履约清单四条（USER 号服务侧必须查不到/ENOMEM 附全长/失败抄长度/QUERY+DESCRIBE 可遍历） | 约束不变量 | 存量 | 21 §4 | 行为对照 | 21 |

#### 来自 22（7 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-270 | 轻量复刻定位（服务侧主循环+树代码的轻量版；两边能互相解释）+ 不知道服务死没死（死亡检测责任在主服务——重登记由主服务发现重启后调） | 概念 | 存量 | 22 §1.3 | `rmib.c:1-22` | 22 |
| K-271 | 节点形状精简版（flags+type/size/四立即三指针/func/名/述可空）+ 八构造宏 + 指针宏类型错编译器不拦警告 | 数据结构 | 存量 | 22 §2.1 | `rmib.h`（:152-159 警告） | 22 |
| K-272 | 稀疏节点（编号+孩子对；三规矩升序/无重复/孩子非空有标志由构造者守；Rust 加构造检查函数） | 数据结构 | 存量 | 22 §1.3、§2.1 | `rmib.h`（稀疏段）；`os/libs/minix-sys/src/rmib.rs` | 22 |
| K-273 | 16 槽表（槽号即线上根 id；登记占首空/注销摘/满丢单向无处报错/重登记重发还占的/复位只给测试） | 机制 | 存量 | 22 §2.2 | `rmib.c:52-57,973-994` | 22 |
| K-274 | 读写镜像六函数同 06 语义（结构名故意不同；两边语义必须一致否则同一查询本地远端两样） | 约束不变量 | 存量 | 22 §2.3 | `rmib.c:64` 起 | 22 |
| K-275 | 请求处理四组（info/call/读写/注销前后）+ 授权三件套对照 12 + 版本快照对照 10 + 短名也走 grant | 机制 | 存量 | 22 §2.4 | `rmib.c:196-544,1037` | 22 |
| K-276 | 栈缓冲规矩（257 字节=256+1 补零；非特权超拒特权走堆——与 08/09"非特权大数据写拒"同一条：不受信的人不许花服务的内存）+ 调用人上下文七字段 + 特权位明线未统一（先认不定义） | 约束不变量 | 存量 | 22 §1.3、§2.1、§2.5 | `rmib.c:36-44`；`rmib.h:22-30,32-38` | 22 |

#### 来自 99 + 纯新增（9 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主 |
|------|------|------|------|---------|------|----|
| K-280 | 服务坐标（MIB_PROC_NR=7/boot :60/特权要求；**E-MIBGRANT 已修**：kernel grant.rs 曾硬编码 VFS=4/MIB=8，现走 Endpoint 枚举单一真值——99 §1 已记） | 约束不变量 | 存量 | 99 §1 | `com.h:66`；edge E-MIBGRANT ✅ | 99 |
| K-281 | 号段与消息面索引（99 §2 表：MIB_BASE/三信/NR_MIB_CALLS/COMMON 三件/六载荷 message.rs:3014-3196+56B 断言 :3784） | 接口协议 | 存量 | 99 §2 | `com.h`；`message.rs` | 99 |
| K-282 | 常量总表索引（尺寸族/顶层 id/元标识符/类型标志族/版本 VERS_1——权威在 minix-types sysctl.rs 钉值测试） | 接口协议 | 存量 | 99 §3 | `types/sysctl.rs` | 99 |
| K-283 | 错误码汇总（普通 errno 按篇 + 三特殊常客单列：ENOMEM/EDONTREPLY/ERESTART） | 约束不变量 | 存量 | 99 §4 | `main.c:341-356,474-487`；`tree.c:1410-1416`；`remote.c:455-459` | 99 |
| K-284 | 跨服务引用依赖链（kernel/PM/VFS/VM/DS/ProcFS/IPC-LWIP-UDS/libc 八行交付条目表） | 接口协议 | 存量 | 99 §5 | plan §5.4；edge 各条 | 99 |
| K-285 | 执行模型声明（单线程事件循环 + 阻塞 sendrec 是 C parity 不设计超时；与 kernel SMP+BKL 两套纪律不许混判） | 概念 | 存量 | 99 §6 | `review-core.md`；`remote.c:341-355` | 99 |
| K-290 | **新增**：C 行为契约逐 case（test87 3662 行全行为契约/test 子树考题面已在 15；rmibtest 267 行 8 个远端拒绝场景+注册顺序契约——Rust 零对应，归 E5(g)；t_sysctl 74 行基础面）——plan §5.4 说"各 handler 篇据此声明"但逐 case 无归属（G5） | 测试性质 | **新增** | （散见 15/21） | `tests/test87.c`、`tests/rmibtest/rmibtest.c:126-201`、`tests/kernel/t_sysctl.c`；todo.md §4 L4 | 23 |
| K-291 | **新增**：Rust 测试地图与对账门（152 测试五层：verdict 单元/arena+walker 集成/server 装配/subtree 表 parity/abi 断言；各篇 §5.1 快照 94/101/107 已声明时点；Gate E 121/121 零虚构的复核方法） | 测试性质 | **新增** | （各篇分散） | 逐文件 #[test] 计数（§0.3）；todo.md Gate E | 23 |
| K-292 | **新增**：联调验收面 E5(g)（MIB_SYSCTL 往返+rmibtest 注册/转发契约+ERESTART 续走三链；前置 E1/E2 通电+E-MIBPROD 对端） | 测试性质 | **新增** | （只在 edge_todo） | `edge_todo.md` E5(g)、E-RMIBWIRE、E-MIBPROD | 23 |

### 2.2 统计摘要

- **总条数**：约 150 条（存量约 137 + 新增约 13；新增中 2 条同时是勘误载体 K-237/E7、K-019/E1）。
- **按类型分布**：概念 10、机制 71、数据结构 15、接口协议 24、约束不变量 22、架构演进 15、工具工程 1、测试性质 5（约数，并row 后按主类型计）。
- **按现有文档分布**：00:3、01:10、02:13、03:10、04:7、05:5、06:9、07:6、08:11、09:8、10:8、11:8、12:12、13:9、14:7、15:7、16:7、17:9、18:8、19:7、20:6、21:6、22:7、99:6；纯新增 3 条（K-290~292）无现有归属。
- **重复标记**：K-002、K-015、K-030、K-035、K-039、K-054、K-058、K-103、K-137、K-144、K-156、K-157、K-171 等存在跨篇讲述，主讲述点已标（详 §3.3）。

---

## 3. 覆盖审计

### 3.1 主题全集与来源

四路来源：① C 符号面——`servers/mib/` 全部 82 个顶层函数 + 13 静态表 + 3 全局计数（plan §5.3 清单本次按源码逐文件重核：main.c 14 函数+mib_root/mib_table、tree.c 22 函数+scratch+三计数、remote.c 9 函数、kern.c 13 函数+2 表、proc.c 16 函数+3 表、hw.c 4+1、vm.c 3+1、minix.c 1+5）+ 协议头 5 文件 + 客户端库（libc 五文件 1713 行、rmib.c+rmib.h 1277 行）；② 操作系统通用概念——按名查询树、乐观并发版本锁、跨服务挂载转发、整表快照缓存、按格式填充；③ 非 C 制品——SEF 启动、交换格式 ABI、C 测试三件套、boot 登记；④ 阶段边界契约——plan §4 A-1~A-12、edge E-RMIBWIRE/E-MIBPROD/E-MIBGRANT/E5(g)。

**三向对账结论**（承接 todo.md §1.2 的判定，本轮独立复核）:判定层与执行层符号与 C 逐函数对得上（含静态树槽位 parity 45+39/4+9/10+6 三表逐槽一致——todo Gate A 与本轮 grep 双证）；执行半（server/walker/transport/arena/heap/tables/sysctl_abi）已于 2026-09-15 全部落地；真实通电仍挂 E1/E2/E-MIBPROD/E-RMIBWIRE。

### 3.2 覆盖缺口表

| # | 缺口/失真主题 | 证据 | 建议归属 | 处置 |
|---|--------------|------|---------|------|
| G1 | **01 篇缺执行半补记**：§3 替代方案仍写"把 main 循环搬进 Rust——否决……现在搬只能搬个空壳"，但 server.rs 已落地（MibIpc:65/MibServer:85/run_once:142/run:547/64 轮上限:82，12 个装配测试）；06/07/10/16/17/18 各篇都有 P1-x 补记，唯独 01 没有 | `server.rs` 实测；01 §3 末行 vs todo.md P1-1 ✅ | 01 §3/§4 | **勘误 E1 + 增补 K-019** |
| G2 | **heap.rs MibBudget 无 stage 文档承载**：A-3 预算策略（256KiB 记账/claim 先于分配/EINVAL 映射/临时挂载点例外 ENOMEM/reset 承接 RestartLossy/有意偏离 DS 定长池的理由）只活在 plan A-3 行与 todo P2-4 | `heap.rs:35,51`；plan §4 A-3；todo P2-4 ✅ | 08（分配是 create/remove 的业务） | 增补（K-120） |
| G3 | **00 篇现状句过时**：§2 末行"循环本体与传输接缝的落地条目见 todo.md P1-1"——P1-1 已闭环，应指向 server.rs 现状 | 00 §2 vs todo P1-1 ✅ | 00 §2 | 勘误 E2 |
| G4 | **plan.md 两处执行半状态过时**：:6 对照行"判定层 7187 行已落地，执行半待建"与 §3.5（:207）"执行半……待建"；执行半 2026-09-15 已随 P1-1~P1-5 闭环，测试基线 107→152 | plan.md:6,:207 vs todo.md §0 状态列 | plan.md（B 相回写） | 勘误 E3 |
| G5 | **C 行为契约无逐 case 归属**：test87（3662）+rmibtest（267）+t_sysctl（74）共 4003 行，plan §5.4 说"各 handler 篇据此声明测试要点"，实际各篇 §5 是 Rust 测试、C 契约只在 15（test 子树面）与 21 §5（一行）露头；rmibtest 的 8 个远端拒绝场景 Rust 零对应（todo L4 已判归 E5(g)） | `tests/` 三件行数实测；15/21 篇 | **新建 23-mib-verification** | 建篇（K-290~292） |
| G6 | **dispatch.rs 行号漂移**（P2-2 重构后）：01 §4.2 与 02 §4.2 引 `dispatch.rs:217`（map_sysctl_reply）——现 :254；SysctlOutcome 现 :217（旧 map 位置）。结构存在仅行号过时（模式 77） | `dispatch.rs:217,254` 实测 | 01/02 §4.2 | 勘误 E4 |
| G7 | 11 篇未点名 sysctl_abi.rs 消费关系：copyout_node/copyout_desc 的执行体组装应消费 `SysctlNode`/`SysctlDesc`（P1-3 落地、describe.rs 已改派生），11 正文只有 02 改判行的间接关联 | `sysctl_abi.rs`；11 §4 | 11 §4 | 增补一句（K-155，轻） |
| G8 | 13 篇无 handler 执行现状（func registry 已实连三纯本地函数、跨服务型诚实 EIO） | `walker.rs` func registry；todo P1-2 ✅ | 13 §4 | 增补（K-187，轻） |
| G9 | 12 篇无 label 查询执行现状（MibServices::ds_retrieve_label seam 已备，真实端委托 minix-sys ds.rs） | `transport.rs`；todo P1-1 ✅ | 12 §4 | 增补（K-171，轻） |
| G10 | 杂物文件：`os/servers/mib/src/types_test_placeholder`（0 字节，2026-09-15 遗留，非 .rs 不影响编译） | `ls -la` 实测 | （范围外发现） | 建议卫生轮删除；B 相不代删 |

### 3.3 重复主题表

| # | 主题 | 出现处 | 主讲述点 | 其余处置 |
|---|------|--------|---------|---------|
| R1 | ENOMEM 方言 | 01 §1.4/§2.4、02 §2.7 词典、09（分配失败禁 ENOMEM）、12（挂载点唯一例外）、99 §4 | 01（收尾换算）+02（词典）；例外记录在 08（分配）与 12（挂载）各自语境 | 99 收口引用 |
| R2 | 单向约束 M-8 | 02 §2.1（wire 规则）、12 §2.2（行为主篇）、22（客户端须知） | 12 | 02 给信封级一句；22 提"第一要知道的事" |
| R3 | req_id 恒 0 | 02 §2.2④、12 §2.4 | 12（回信路由 verdict） | 02 给 wire 形状 |
| R4 | 三 grant | 06 §2.4（relay 原语主）、12 §2.4（调用序与逆撤） | 06（原语）/12（旅程） | 分层清晰维持 |
| R5 | can_restart 快照 | 10 §1.4（判定主）、12 §1.4/§2.4（语境） | 10 | 12 引用 |
| R6 | 版本乐观锁 | 03 §1.6（概念主）、08（upgrade 机制）、11（query/describe 门）、12（unmount bump） | 03+08 | 11/12 引用 |
| R7 | ERESTART 三去向 | 10 §1.4（判定主）、12 §2.4（产生侧）、02 §2.7（词典行） | 10 | 维持 |
| R8 | label 机制 | 12 §2.2（mib_get_label 主）、22（客户端对照）、99 §5（DS 依赖行） | 12 | 维持 |
| R9 | boot 两层语义 | 00 §2、01 §2.5、06（07-ds 承载抑制链） | 00（导航）/01（MIB 位置理由） | 维持 |
| R10 | 逐篇 §5 测试表 | 01~22 各篇 | 各篇保留与自家模块对应的最小面 | 聚合地图与 C 契约归 23（G5 联动） |
| R11 | 静态树槽位 parity（45+39/4+9/10+6） | 13/14/15 各篇 §1、99 §3 索引 | 13/14/15 各自表 | 23 对账门复核 |

### 3.4 越界主题表

| # | 越界/可疑内容 | 判定 | 处置 |
|---|--------------|------|------|
| Y1 | 01 §1.3/§3 与 DS 的三处对照（保资产 vs 保骨架、猜 notify vs 听 notify、七信 vs 三信） | 跨 stage 教学对照，带显式交叉引用 `../07-stage-ds/01` | 保留 |
| Y2 | 13 §1.4/§3、14 §1.6、15 §1.7、16~20 各篇的 NetBSD/Linux/Redox 对照 | 教学性对照（13~15 教学化重写的产物），位于概念章职责内；22 篇 §3 明确"对照不是依据" | 保留 |
| Y3 | 22 涉 minix-sys 归属（A-8） | 必要（客户端半归属声明），协议半缺挂 E-RMIBWIRE 已声明 | 保留 |
| Y4 | 12 §7 外部消费者 C 文件引用（ipc/lwip/uds） | 必要的消费者地图 | 保留 |
| Y5 | 01 §2.3 步骤 4 混入日语字符"その"（"失败回その错误码"） | 输入残留，非术语 | **勘误 E5**：改"回该错误码" |
| Y6 | 15 §2.3 标题"指向进程信息的表（minix.c:59-66）加密顶层表（minix.c:68-79）"——"加密"为笔误（应为"加顶层表"或"加上顶层表"），且两个小节挤进一行 | 笔误 | **勘误 E6**：拆为两小节或改"加顶层表" |
| Y7 | 18 §1.3/§2.4 记录了 KERN_PROC_TTY 的"僵尸不读 fproc 槽"意图，但未标注 C 代码 :875 实际方向相反 | 意图与实况脱节（模式 78：MINIX3 BUG 未标注） | **勘误 E7**：18 §2.4 补 MINIX3 BUG 标注（详见 K-237） |

### 3.5 非 C 主题逐项回答（固定清单）

| 主题 | 在哪讲 / 为什么不在本 stage |
|------|---------------------------|
| 链接与加载 | DS 无自有链接脚本；`servers/mib/Makefile`（21 行）纯构建脚本 **WONTFIX 维持**（plan §5.5 既有裁决；MIB 无 magic 插桩依赖——A-7 明言，与 DS 的 regcomp 弱符号 hack 不同源，无翻案理由）。ELF 装载归 03-stage-rs/14-stage-runtime |
| 镜像与内存布局 | **交换格式布局（sysctlnode 96B/sysctldesc 16B/kinfo_lwp 128B/kinfo_proc2 680B/minix_proc 16B+72B）是本 stage 的核心镜像主题**——02 定格式、sysctl_abi.rs 锚定（P1-3）、11/17/18/20 消费。内核内存布局不属本 stage |
| 汇编入口与陷阱进入 | 用户态服务无自有汇编入口；IPC 陷阱归 01-stage-kernel（12-ipc-core）。本 stage 只消费 `sef_receive_status`/`ipc_sendnb`/`ipc_sendrec` 语义 |
| 启动装配 | boot_image 登记（:60）与抑制链：00/01；SEF 两回调同体：01/04；RS 侧握手归 03-stage-rs |
| 构建与工具链 | Makefile WONTFIX 维持（同链接行）；MINIX_TEST_SUBTREE 编译开关是语义（15 承载，非构建脚本） |
| 跨模块接口与线格式 | 六载荷+共享号：02；远程协议 COMMON_MIB_*：02/12；ProcFS 布局：20/02；rmib.h 节点形状：22 |
| 错误路径 | ENOMEM 方言/EDONTREPLY/ERESTART/EOPNOTSUPP/分配失败禁 ENOMEM：01/02/99 + 各篇；启动 panic vs 运行 errno：01（收发轻重） |
| 关闭与退出 | 无优雅关闭：`for(;;)` 永不返回（`main.c:443-490` NOTREACHED），唯一退出是 receive 失败 panic（:445-446）——01 已有（K-010），不缺 |
| 并发与同步 | 单线程事件循环 + 阻塞 sendrec 是 C parity 不设计超时：99 §6 已声明（K-285）；鉴权缓存单次语义：07 |
| 测试基建 | C 三件套（test87/rmibtest/t_sysctl）：**新建 23 承载**；Rust 152 测试地图与对账门：23；test87 考题面（test 子树）：15；Gate E 方法：todo §7 |

---

## 4. 新目录

### 4.1 总判决与理由

**编号 00-22 与 99 全部保持不变；新建 23-mib-verification 一篇；00/99 及全部 22 篇按本蓝图契约做"保号重建"（勘误 + 执行半增补 + 契约化取料）。** 不做任何重排、拆分、合并、归档。

理由（三条，均带证据）：

1. **这是三个 rerank 目标中唯一"文档-代码双向已完成"的 stage**：24 篇全部 reviewed（plan §6.1）、首轮架构审查十一条全部闭环（todo §0）、152 测试全绿（本轮实测）、Gate E 121/121 零虚构（todo §1.1）。目录顺序 = 启动真序（§1.2-§1.3）+ 学习依赖（协议先于 handler、树模型先于动态、分发先于子系统、进程系按依赖序），与 R 相提示词 §九模板逐项对得上。病只剩三类：**执行半落地后各篇补记不齐**（G1/G3/G7-G9）、**C 契约无聚合归属**（G5）、**少量笔误与一行号漂移**（E4-E7）。治法是补全与勘误，不是重排。
2. **断链成本 here 最高**：stage 内互引约 180 处（10 篇被引 16 次为枢纽、02 篇 13 次、06 篇 11 次），入站 52 处（8 个 stage + edge），代码注释 5 处。重排换不来对等收益——现存内容质量已在 review 轮验证。
3. **唯一结构性缺口是测试与验证面**：4003 行 C 契约 + 152 个 Rust 测试 + E5(g) 联调面横切全部 handler，23 号位空着，追加零重编号成本（与 07-ds 的 13 篇同一裁决逻辑）。

### 4.2 新篇章总表（25 篇）

| 编号 | 标题 | 一句话定位 | 分组 |
|------|------|-----------|------|
| 00 | MIB 整体架构概览 | 回答"MIB 是什么、为什么 boot 里排这么早、25 篇怎么读" | 总览 |
| 01 | 启动入口与主循环 | 三类请求、两种拒绝、一条回信规则 + sysctl 六道门 | 诞生与骨架 |
| 02 | 协议面 | 三封信、六种结构、两种交换格式、一本错误方言 | 协议面 |
| 03 | 节点模型 | 五态节点、四格矩阵、三计数器、一页草稿、版本乐观锁 | 树模型 |
| 04 | 静态树初始化 | 七顶层槽、四线 wiring、一次全树点名 | 树模型 |
| 05 | 树查找 | 静态一跳 + 有序链表早停，落空附赠插入点 | 树模型 |
| 06 | 拷贝输入输出 | 钳制、精确、分页猜、grant 转交 | 原语 |
| 07 | 鉴权模型 | 问一次到处用、三种门、拒绝词典只有 EPERM | 原语 |
| 08 | 动态节点 | 创建多重校验、销毁多重守卫、版本递增、内存预算 | 树的生长 |
| 09 | 数据读写 | 先暂存再落盘、字符串三规则、bool 消毒 | 树的生长 |
| 10 | 请求分发 | 逐层解析、元标识符、三类节点判定、次主线路径图 | 分发 |
| 11 | 枚举与描述 | 两次序列化（节点快照+描述流）与一次性描述设置 | 分发 |
| 12 | 远程子树 | 认领+盖章+转交+清场，死亡是常态 | 边界 |
| 13 | kern 子树 | 操作系统的自我介绍：常量叶/函数节点/占位子树/排除约定 | 数据源 |
| 14 | vm+hw 子树 | 两棵小表和三个要算清楚的数 | 数据源 |
| 15 | minix 子树 | 测试子树、统计指针叶、proc 两门、lwip 不留位 | 数据源 |
| 16 | 进程表快照 | 先把三家的表拿过来再建索引（三规矩+双魔数+PID 哈希） | 进程信息 |
| 17 | LWP 快照 | 状态怎么定、睡在哪里怎么写 | 进程信息 |
| 18 | PROC2 | 同样的状态另一套输出结构 + 九种过滤 | 进程信息 |
| 19 | 进程参数 | 读别人家的内存：逐页走、带预算、超了截断 | 进程信息 |
| 20 | MINIX_PROC | ProcFS 要的两份数据，负 PID=内核任务 | 进程信息 |
| 21 | libc 客户端 | 自家的事本地办（外部契约，不实现） | 客户端与验证 |
| 22 | rmib 客户端 | 服务怎么把自己的子树挂进这棵树 | 客户端与验证 |
| **23** | **验证契约（新建）** | **C 行为契约（test87/rmibtest/t_sysctl）、Rust 152 测试地图、联调出口与对账门的一张图** | 客户端与验证 |
| 99 | 全局概念收口 | 服务坐标、号段、常量索引、错误码、跨服务依赖、执行模型 | 收口 |

### 4.3 阅读路径与序差表

**主线**（00→99 顺序阅读，编号即依赖序）：协议面（02）先于一切 handler，树模型（03~05）先于动态节点（08）与分发（10），分发先于子系统（13~20），客户端（21/22）与验证（23）殿后。全局常量随用随查 99。

**支线（可跳读）**：① 只写挂载方的服务作者：`00 → 02 → 12 → 22 → 23`（单向约束与 req_id 是第一课）；② 只消费 sysctl 的应用作者：`00 → 01（ENOMEM 方言）→ 02 → 21 → 23`；③ 只关心进程信息的 ps/top 移植者：`00 → 16 → 17 → 18 → 20 → 99`。

**序差表**（教学序 ≠ 运行时序之处；运行时事实见 §1）：

| # | 运行时序事实（锚点） | 教学序位置 | 偏差理由 | 回指补偿 |
|---|---------------------|-----------|---------|---------|
| D1 | `mib_init`（树初始化）先于主循环执行（`main.c:384-410` vs `:443`） | 初始化拆两处：骨架在 01（第 2 篇），落地在 04（第 5 篇） | `mib_init` 六拍涉及四子树表与远端表，提前讲必前向引用 13~15/12 | 01 已声明"mib_init 本体在 04"；04 头部声明位置 |
| D2 | 协议/树模型/原语（02~09）在运行时无独立时点，是解码与分发的静态知识 | 插在 01 与 10 之间 | 首次出现即完整：六道门、节点形状、拷贝鉴权语义必须先于"一次查询的旅程"讲透 | 各篇头部位置可回答声明（既有原则） |
| D3 | 远程注册发生在 IPC/LWIP/UDS 启动时（服务生命周期），转交发生在任意查询命中挂载点时 | 12（第 13 篇）在 handler 面（13~15）之前 | 13 的 kern.ipc mock 需要"等着被覆盖"的语境（12 的挂载机制）才讲得通 | 12 自足（本篇含完整路径图）；13 §1.3 回指 12 |
| D4 | 10 的 ERESTART 续走判定的本体（转交执行）在 12 | 10（第 11 篇）先讲三去向判定 | 分发循环是每查询热路径，先讲判定；grant/sendrec 执行机制属 12 | 10 §1.4 已自足（快照先行+三去向），无前向阻塞——**无需新补偿** |
| D5 | 客户端在任意时刻运行 | 21/22/23 殿后 | 回信形状与履约点需要 handler 语义先讲完 | 02 已把信封与方言前置 |

**并行体的组织**：13~15 三棵子树按 mib_init 线序（kern→vm+hw→minix）排列并共用"表+清单+函数枚举"写法（13 是写法篇）；16~20 按依赖序（表→LWP→PROC2→ARGS→MINIX_PROC）；21/22 两类客户端按"提问方/挂载方"分组；23 按"统一对账门 + 三族（C 契约/Rust 地图/联调出口）"分组。

---

## 5. 每篇契约

> 格式：定位 / 讲什么（K 编号）/ 不讲什么（去向）/ 前置 / 后置 / 事实底线 / 验收标准。知识点清单 = §2.1 对应 K 行（每篇契约不重复抄表，以 K 编号指涉；B 相按 K 行的锚点与"主"标记取料）。

### 00-mib-overview

- **定位**：总览导航——MIB 是什么、boot 排位理由、双次主线、25 篇导航与设计原则。
- **讲什么**：K-001、K-002（导航面）、K-003；启动主线图（§1.2 S1-S8 图形化）；sysctl 次主线与远程次主线各一张旅程图（引 10/12 为家）；导航表（§4.2）。
- **不讲什么**：一切机制（01~22）；跨 stage 对端（edge_todo）。
- **前置**：无。**后置**：全部。
- **事实底线**：`main.c:1-33`；`table.c:60`；`main.c:264-267`；`os/servers/mib/src/lib.rs`。
- **验收**：三问可答（存在理由/排位理由/阅读顺序）；**勘误 E2**——§2 末行改为指向 server.rs 现状（"循环本体与传输接缝已落地：`server.rs`（P1-1），真实通电挂 E1"）。≤130 行。

### 01-mib-init-main

- **定位**：骨架——两回调同体不同命、三信两拒一规则、sysctl 六道门 verdict。
- **讲什么**：K-010~K-018；**增补 K-019**（server.rs 装配现状：MibIpc 三方法/MibServer/run_once/run/64 轮上限/transport 双 seam——补 P1-1 补记，格式随 06/07 篇先例）。
- **不讲什么**：消息字段（02）；分发（10）；拷贝/鉴权（06/07）；静态表（03/04）。
- **前置**：00（+kernel 09/12 声明）。**后置**：02/04/06/10、13（E2E 装配参照）、23。
- **事实底线**：`main.c:277-379,384-428,433-492`；`com.h:1022-1030`；`dispatch.rs:41-254`（**勘误 E4：§4.2 表 map_sysctl_reply 行号 :217→:254**）；`sef.rs:24`；`server.rs:65,82,85,142,547`。
- **验收**：六道门逐条带锚；ENOMEM 四行换算表全保；**勘误 E1**——§3 末"替代方案及否决"改写为"执行半已落地"补记（搬循环的否决已成历史，现状是 server.rs 落地+SysIpc EIO 诚实桩+通电挂 E1）；**勘误 E5**——§2.3 "その"改"该"。

### 02-mib-message-contract

- **定位**：协议面——三封信、六种结构、两种交换格式、错误方言词典。
- **讲什么**：K-030~K-043。
- **不讲什么**：解码流程（01）；标志行为（03）；挂载行为（12）；客户端（21/22）。
- **前置**：01。**后置**：03/06/08/11/12/17/18/20/21/22/23。
- **事实底线**：`com.h:597-622,1022-1030`；`ipc.h` 六处；`sys/sys/sysctl.h:75-164,1382-1454`；`minix/sysctl.h` 全文；`minix-types` 四文件。
- **验收**：标志表/元标识符/USERFLAGS 逐值对锚；六载荷 56 字节断言面保留；P1-3 改判行保留；§4.2 表 `map_sysctl_reply` 行号随 E4 同步（:217→:254）；§5.1 快照加"基线日期 + 23 篇对账门指针"。

### 03-mib-node-model

- **定位**：节点形状——五态、四格矩阵、三计数、一页草稿、版本锁；"先看类型再读字段"第一课。
- **讲什么**：K-050~K-059。
- **不讲什么**：宏体（04）；查找（05）；创建销毁（08）；挂载行为（12）。
- **前置**：02。**后置**：04/05/08/10/12/15。
- **事实底线**：`mib.h:40-50,72-74,111-125,150-156,181-186,188-249,252-324`；`tree.c:12,21-27,25-27 计数`；`tree/flag.rs`、`tree/node.rs`。
- **验收**：四格矩阵表逐格对 `mib.h:111-125`；RemotePack 位排布（5+12×12≤32）有断言锚；BTreeMap 裁决指针（15 §4.4 单一真值）保留。

### 04-mib-static-tree-init

- **定位**：静态 wiring——七槽、四线、点名。
- **讲什么**：K-070~K-076。
- **不讲什么**：子树表内容（13~15）；查找（05）；远端表复位细节（12）。
- **前置**：01、03。**后置**：05/13/14/15/23。
- **事实底线**：`main.c:36-64,384-410`；`tree.c:1479-1536`；`tree/static_tree.rs`、`tree/init.rs`。
- **验收**：线序整序断言保留；"遍历用 size 不用 csize"的因果保留；arena 兑现指针（15 §4.4）保留——**可选增补 K-076 一句**（04 自身补"已兑现"标记，轻）。

### 05-mib-tree-lookup

- **定位**：单层查找双通道。
- **讲什么**：K-080~K-084。
- **不讲什么**：多层解析（10）；插入删除（08）；静态表内容（04/13~15）。
- **前置**：03、04。**后置**：08/10/12（mount 路径 walk 复用）。
- **事实底线**：`tree.c:34-81`；`tree/lookup.rs`。
- **验收**：三检查序与空位穿透有专测锚；约束推导（:59-67 注释）保留；**可选增补**一句"walker 生产消费已转正（10 P1-2 补记）"（轻）。

### 06-mib-copy-io

- **定位**：字节进出的判与走。
- **讲什么**：K-090~K-098。
- **不讲什么**：鉴权（07）；转交旅程（12）；handler 调用面（09/13~20）。
- **前置**：01、02。**后置**：08/09/11/12。
- **事实底线**：`main.c:64-83,90-252`；`tree.c:371-420`；`io/copy.rs`、`io/relay.rs`、`transport.rs`。
- **验收**：两条核心约束各配反例；P1-4 补记保留（K-097）；"永不 ENOMEM"两处注释锚保留。

### 07-mib-auth-model

- **定位**：问一次到处用 + 三种门 + 只有 EPERM。
- **讲什么**：K-100~K-105。
- **不讲什么**：PM 怎么答（transport）；各 handler 调门上下文（08~11）。
- **前置**：02、03。**后置**：08/09/10/11/12。
- **事实底线**：`main.c:259-272`；`mib.h:49-50`；`tree.c` 九门；`auth.rs`。
- **验收**：九门全表逐行带锚；EACCES 零命中证据保留；P1-4 补记（auth::ask fail-closed）保留。

### 08-mib-dynamic-nodes

- **定位**：动态节点生死——创建校验链、销毁守卫链、版本递增、内存归属与预算。
- **讲什么**：K-110~K-120。
- **不讲什么**：插入指针手术细节（arena）；回显序列化（11）；门谓词本体（07）。
- **前置**：03/05/06/07。**后置**：10/11/12（临时挂载点复用 scan/add/remove）/15（测试子树消费者）/23。
- **事实底线**：`tree.c:247-266,277-359,426-481,486-775,784-916`；`tree/dynamic.rs`、`tree/version.rs`、`tree/arena.rs`。
- **验收**：十四道校验按 C 序排列；"分配前全查完""数据区永不单独释放（must not）"两条禁令保留；**增补 K-120（G2）**——A-3 预算策略一小节（MibBudget 256KiB 记账/ENOMEM 例外仅临时挂载点/reset 承接 RestartLossy/偏离 DS 定长池的理由），[ARCH A-3] 三处一致核对 plan/本篇/heap.rs；P2-1 修复记录（K-119）保留。

### 09-mib-data-access

- **定位**：叶子读写——stage 先暂存、字符串三规则、bool 消毒。
- **讲什么**：K-130~K-137。
- **不讲什么**：函数 handler（13~20）；分发（10）；拷贝执行（06/transport）。
- **前置**：03/06/07。**后置**：10/13~20（公共地基）。
- **事实底线**：`tree.c:1097-1325`；`data/readwrite.rs`。
- **验收**：stage 选择两半（草稿/特权堆败 EINVAL）与 C 行锚一致；字符串三规则（可不带终结/补加/满槽无终结拒）成对出现；SSIZE_MAX 加宽的 ARCH 声明保留。

### 10-mib-dispatch

- **定位**：分发循环——每轮一分量、元 switch、三问形状、续走三去向；sysctl 次主线为家。
- **讲什么**：K-140~K-147。
- **不讲什么**：单层查找（05）；叶子读写（09）；枚举/创建/销毁本体（11/08）；转交执行（12）。
- **前置**：03/05/07/09（+12 声明）。**后置**：11/12/13~20/23。
- **事实底线**：`tree.c:1332-1474`；`tree/dispatch.rs:25-206`；`walker.rs:141`。
- **验收**：次主线路径图与 §1.4 一致；三句实话注释解读保留；P1-2 补记（K-147）保留。

### 11-mib-query-describe

- **定位**：两次序列化与一次性描述设置。
- **讲什么**：K-150~K-157。
- **不讲什么**：分发投递（10）；描述内存归属（08 RemoveDelta）；字符串拷入（06）。
- **前置**：02/03/06/07/10。**后置**：12（缓存窗口复用）/15（对齐考题）/21（排序消费方）/23。
- **事实底线**：`tree.c:90-239,925-1090`；`query.rs`、`describe.rs`；`sysctl_abi.rs`。
- **验收**：内部三位零暴露 + SYSCTL_NODE_FN 假地址防 ASR 的论证保留；设置六杠顺序对 C；**增补 K-155（G7）**——§4 点名 sysctl_abi.rs 消费关系与 describe.rs 常量派生。

### 12-mib-remote-subtrees

- **定位**：远程子树全旅程——认领、盖章、转交、清场；死亡是常态；远程次主线为家。
- **讲什么**：K-160~K-171。
- **不讲什么**：续走判定（10）；客户端写注册信（22）；DS label 机制本体（07-ds/08）。
- **前置**：02/03/06（+DS 08 声明）。**后置**：13（kern.ipc 覆盖语境）/22/23。
- **事实底线**：`remote.c` 全文；`tree.c:1543-1842`；`remote.rs`、`tree/mount.rs`。
- **验收**：两条次主线路径图全保；三 grant 后授先撤、req_id 恒 0、顶层禁挂、ENOMEM 唯一例外点逐条带锚；C 四个上游 TODO（死亡通知/init label/白名单/flags 定义）如实保留；**可选增补 K-171 一句**（label 查询执行现状，轻）。

### 13-mib-subtree-kern

- **定位**：kern 子树——写法篇（常量叶/函数节点/占位子树/排除约定）。
- **讲什么**：K-180~K-188。
- **不讲什么**：进程三查询本体（17~19）；跨服务拉取执行（transport）；另两棵子树（14/15）。
- **前置**：03/06/07/09/10。**后置**：14/15（照写法）/17~19（登记节点指向）/23。
- **事实底线**：`kern.c` 全文；`subtree/kern.rs`；`minix-types sysctl.rs`（KERN 值表）。
- **验收**：45+39=84 互斥测试锚保留；verify 双星与 pts 别名细读保留；consdev 注释-代码出入的诚实记录保留；**可选增补 K-187 一句**（func registry 实连现状，轻）。

### 14-mib-subtree-vm-hw

- **定位**：vm+hw 两棵小表与三个数（窗口槽/页位移/宽窄截断）。
- **讲什么**：K-190~K-196。
- **不讲什么**：统计读取执行（transport）；kern/minix 子树。
- **前置**：13（写法）。**后置**：15/16（统计读取同源）/23。
- **事实底线**：`vm.c`、`hw.c` 全文；`minix/type.h:88-95`；`subtree/vm.rs`、`subtree/hw.rs`。
- **验收**：三个数各有独立函数与测试锚；"先转 64 位再乘""饱和到零""截到上限"顺序论证保留；x86_64 新增行的 ARCH 三处标注保留。

### 15-mib-subtree-minix

- **定位**：minix 子树——test87 考题面、统计指针叶、proc 两门、lwip 不留位。
- **讲什么**：K-200~K-206。
- **不讲什么**：test87 怎么跑（23）；进程本体（20）；创建销毁（08，消费者引用）。
- **前置**：02/03/08。**后置**：20/23。
- **事实底线**：`minix.c` 全文；`mib.h:23`；`subtree/minix.rs`；`tree/arena.rs`（兑现真值）。
- **验收**：12 字面量逐值断言锚保留；空描述对齐自检论证保留；lwip 不留位断言保留；arena 单一真值声明保留；**勘误 E6**——§2.3 标题"加密顶层表"改正并拆分小节。

### 16-mib-proc-tables

- **定位**：拿表三规矩 + 双魔数 + PID 哈希 + 两个工具函数。
- **讲什么**：K-210~K-216。
- **不讲什么**：表内容语义（17~20 按需引用对方服务文档）；输出格式（17~20）。
- **前置**：02/06/10。**后置**：17~20/23。
- **事实底线**：`proc.c:1-216`；`const.h:164`；`pm/mproc.h`；`proc/tables.rs`。
- **验收**：三规矩各配代码锚（:53-58,:66-69,:72）；EXTRA_PROCS=8 的两次调用推理保留；P1-5 补记（K-216）保留。

### 17-mib-proc-lwp

- **定位**：LWP 快照——状态优先级、wchan 类别编码、三填写函数、入口规矩。
- **讲什么**：K-220~K-228。
- **不讲什么**：拿表（16）；拷出（06）；PROC2/ARGS/MINIX_PROC（18~20）。
- **前置**：16。**后置**：18（状态复用）/23。
- **事实底线**：`proc.c:224-593`；`sys/sys/lwp.h:279-285`；`proc/lwp.rs`；`sysctl_abi.rs`（128B）。
- **验收**：五状态判定序、覆盖/添标规则（:340,349 vs :328,371）成对出现；C 重复条件笔误的诚实记录保留；kinfo_lwp 128B 补记保留。

### 18-mib-proc2

- **定位**：PROC2——先算 LWP 再复制、九种过滤、内核伪进程。
- **讲什么**：K-230~K-237。
- **不讲什么**：状态判定本体（17）；拿表（16）；内存用量来源（VM）。
- **前置**：16/17。**后置**：19（槽位检查复用）/23。
- **事实底线**：`proc.c:600-913`；`proc/proc2.rs`；`sysctl_abi.rs`（680B）。
- **验收**：先算后复制的设计意图（:610-618）保留；九种过滤与内核匹配规则逐条带锚；kinfo_proc2 680B 补记保留；**勘误 E7（K-237）**——§2.4 KERN_PROC_TTY 处补 MINIX3 BUG 标注（:875 与 :707 方向相反，:873 注释与 :707 是意图证据），并核对 `proc2.rs` 的 Rust 取法（按意图实现则在代码留 [ARCH] 字据，按 C 实况实现则注明 bug parity——B 相裁决后两处一致）。

### 19-mib-proc-args

- **定位**：进程参数——读别人家内存的预算与截断。
- **讲什么**：K-240~K-246。
- **不讲什么**：拿表（16）；拷出循环执行（transport）。
- **前置**：16/18（槽位检查复用）。**后置**：23。
- **事实底线**：`proc.c:918-1172`；`proc/proc_args.rs`；`sys/exec.h`。
- **验收**：攻击模型（:1023-1029）与预算公式保留；"截断返回实际长度不报 ENOMEM（libkvm 依赖）"的独特方言保留；零页返回 0 非错、预算花完不报错的两个否决记录保留。

### 20-mib-minix-proc

- **定位**：ProcFS 两份数据与负 PID 语义。
- **讲什么**：K-250~K-255。
- **不讲什么**：状态判定（17）；ProcFS 文件面（15-stage-fs）。
- **前置**：16/17。**后置**：23。
- **事实底线**：`proc.c:1177-1288`；`minix/sysctl.h:61-86`；`fs/procfs/tree.c:87`、`pid.c:44`；`proc/minix_proc.rs`。
- **验收**：负 PID=内核任务与 KERN_LWP 的 -1=全列表两套规矩对比成段；16B/72B 布局锚（P1-3 offset 级）保留。

### 21-mib-client-libc

- **定位**：libc 外部契约——分流、长度规矩、失败抄长度、排序（不实现）。
- **讲什么**：K-260~K-265。
- **不讲什么**：服务内部（01~20）；rmib（22）。
- **前置**：02 + 服务器语义。**后置**：23（t_sysctl 契约对应）。
- **事实底线**：`libc/gen/sysctl.c`（391）、`sysctlgetmibinfo.c`（611）、`libc/sys/__sysctl.c`（40）。
- **验收**：履约清单四条保留；"失败也抄长度"的怪规矩与依赖方（sysctl(8)）保留；无 Rust 实现的声明保留。

### 22-mib-rmib-client

- **定位**：挂载库——轻量复刻、稀疏节点、槽表、栈缓冲规矩（传输半挂 E-RMIBWIRE）。
- **讲什么**：K-270~K-276。
- **不讲什么**：服务侧挂载（12）；libc（21）。
- **前置**：02/12/06。**后置**：23（rmibtest 契约对应）。
- **事实底线**：`rmib.c`（1089）、`rmib.h`（188）；`os/libs/minix-sys/src/rmib.rs`。
- **验收**：稀疏三规矩与构造检查分离保留；16 槽状态机（占/摘/重发/全清）测试锚保留；"不知道服务死没死"的诚实声明保留。

### 23-mib-verification（新建）

- **定位**：验证资产一张图——C 行为契约逐 case、Rust 152 测试分层地图、联调出口、对账门。
- **讲什么**：K-290（C 三件套逐 case：test87 的类型/权限/元标识符/创建销毁/远程/对齐分组契约，test 子树考题面回指 15；rmibtest 的 8 个远端拒绝场景 + 注册顺序契约 :126-201；t_sysctl 基础面）；K-291（Rust 地图：verdict 单元/arena+walker 集成/server 装配 12 测试/subtree parity/abi 断言五层 + 各篇 §5.1 快照的对账方法）；K-292（E5(g) 三链：MIB_SYSCTL 往返/rmibtest 注册转发契约/ERESTART 续走；前置 E1/E2 通电 + E-MIBPROD/E-RMIBWIRE 对端）。
- **不讲什么**：各测试断言细节（各篇与测试代码）；内核 trap 机制（edge E1）。
- **前置**：07~20（handler 语义）、21/22（客户端契约）。**后置**：99 引用。
- **事实底线**：`tests/test87.c`（3662）、`tests/rmibtest/rmibtest.c`（267，:126-201）、`tests/kernel/t_sysctl.c`（74）；`edge_todo.md` E5(g)/E-RMIBWIRE/E-MIBPROD；逐文件 `#[test]` 计数（§0.3）；`cargo test -p minix-mib --lib` 基线。
- **验收**：rmibtest 八个拒绝场景逐条有一行契约摘要与"Rust 零对应 → E5(g)"标注；Rust 计数表合计 = 当次实测；对账门给出可执行 grep 模板（沿 todo Gate E 方法：22 篇 §5 声明逐名 `rg "fn {name}"` 对账）。

### 99-mib-global-concepts

- **定位**：速查收口——服务坐标、号段、常量索引、错误码、依赖链、执行模型。
- **讲什么**：K-280~K-285。
- **不讲什么**：一切机制；测试细节（引 23）。
- **前置**：全部（导航声明）。**后置**：无。
- **事实底线**：`com.h:66,597-622,1022-1030`；`types/sysctl.rs`、`types/endpoint.rs`；`message.rs` 六载荷；`main.c:341-356,474-487`；`tree.c:1410-1416`；`remote.c:455-459`。
- **验收**：五节结构保留；E-MIBGRANT 修复记录保留；§5 依赖链表与 edge 条目一一对应；**增补一句**——测试面索引指向 23。

---

## 6. 变更表

| 操作编号 | 操作类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向/来源 |
|---------|---------|--------|--------|------|-----------|----------|
| C1 | 重建·保号 | 00 | 00 | E2 现状句更新 + 导航对齐 25 篇 | K-001~003 | 存量保留 |
| C2 | 重建·保号 | 01 | 01 | E1 执行半补记 + E4 行号 + E5 その | K-010~019 | 存量保留 + K-019 增补 |
| C3 | 重建·保号 | 02 | 02 | E4 行号同步 + §5.1 对账指针 | K-030~043 | 存量保留 |
| C4 | 重建·保号 | 03 | 03 | 无事实改动，锚点复验 | K-050~059 | 存量保留 |
| C5 | 重建·保号 | 04 | 04 | 可选兑现标记（K-076） | K-070~076 | 存量保留 |
| C6 | 重建·保号 | 05 | 05 | 可选 walker 转正一句 | K-080~084 | 存量保留 |
| C7 | 重建·保号 | 06 | 06 | 无事实改动（P1-4 补记已在） | K-090~098 | 存量保留 |
| C8 | 重建·保号 | 07 | 07 | 无事实改动（P1-4 补记已在） | K-100~105 | 存量保留 |
| C9 | 重建·保号 | 08 | 08 | **G2 增补 A-3 预算小节（K-120）** + P2-1 记录保留 | K-110~120 | 存量保留 + 增补 |
| C10 | 重建·保号 | 09 | 09 | 无事实改动 | K-130~137 | 存量保留 |
| C11 | 重建·保号 | 10 | 10 | 无事实改动（P1-2 补记已在） | K-140~147 | 存量保留 |
| C12 | 重建·保号 | 11 | 11 | **G7 增补 sysctl_abi 消费句（K-155）** | K-150~157 | 存量保留 + 增补 |
| C13 | 重建·保号 | 12 | 12 | 可选执行现状一句（K-171） | K-160~171 | 存量保留 |
| C14 | 重建·保号 | 13 | 13 | 可选 func registry 现状（K-187） | K-180~188 | 存量保留 |
| C15 | 重建·保号 | 14 | 14 | 无事实改动 | K-190~196 | 存量保留 |
| C16 | 重建·保号 | 15 | 15 | **E6"加密"笔误修正** | K-200~206 | 存量保留 |
| C17 | 重建·保号 | 16 | 16 | 无事实改动（P1-5 补记已在） | K-210~216 | 存量保留 |
| C18 | 重建·保号 | 17 | 17 | 无事实改动（P1-3 补记已在） | K-220~228 | 存量保留 |
| C19 | 重建·保号 | 18 | 18 | **E7 MINIX3 BUG 标注（K-237）** | K-230~237 | 存量保留 + 勘误 |
| C20 | 重建·保号 | 19 | 19 | 无事实改动 | K-240~246 | 存量保留 |
| C21 | 重建·保号 | 20 | 20 | 无事实改动 | K-250~255 | 存量保留 |
| C22 | 重建·保号 | 21 | 21 | 无事实改动 | K-260~265 | 存量保留 |
| C23 | 重建·保号 | 22 | 22 | 无事实改动 | K-270~276 | 存量保留 |
| C24 | **新建** | — | 23-mib-verification.md | G5：C 契约/Rust 地图/联调出口/对账门无归属 | K-290~292 | 来源：tests/ 三件 + 逐文件计数 + edge E5(g) |
| C25 | 重建·保号 | 99 | 99 | 测试面索引指向 23 | K-280~285 | 存量保留 |
| C26 | 回写（B 相附带） | plan.md:6,:207 | plan.md | E3：执行半状态与 152 基线更新 | — | doc-plan 同步 |

**没有的操作**：重排 0、拆分 0、合并 0、归档 0（§4.1 理由 1/2；24 篇边界经 plan §3.4 与首轮 review 双重验证，无交叉无遗漏）。

---

## 7. 缺漏新篇（非 C 主题清单逐项落实）

| 主题 | 落实 | 验收标准 |
|------|------|---------|
| 链接与加载 | 明确不建篇：Makefile WONTFIX 维持（无 magic 插桩依赖，纯构建脚本）；ELF 装载归 RS/runtime stage | plan §5.5 裁决维持，无新篇 |
| 镜像与内存布局 | 已有归属：02（格式）+ sysctl_abi.rs（锚定）+ 11/17/18/20（消费）——本 stage 核心镜像主题不缺 | P1-3 落地已在，契约钉消费关系（K-155） |
| 汇编入口与陷阱进入 | 明确不在本 stage：无自有汇编入口；声明归 01-stage-kernel | 00/01 前置声明已有一句 |
| 启动装配 | 已有归属：00/01（登记+抑制+SEF）；RS 侧归 03-stage-rs | K-002/K-011 已含 |
| 构建与工具链 | 一句话制：MINIX_TEST_SUBTREE 是语义归 15；Makefile 非语义不展开 | 15 已含 |
| 跨模块接口与线格式 | 已有归属：02（六载荷+共享号）/12（远程协议）/20+02（ProcFS 布局）/22（rmib.h 形状） | 各契约已含 |
| 错误路径 | 已有归属：01（ENOMEM 方言+轻重）/02（词典）/99（收口）；分配失败禁 ENOMEM：08 | K-015/K-042/K-283 |
| 关闭与退出 | 已有归属：01（K-010 无优雅关闭一句）——本 stage 不缺（DS 的 G7 同型项在此已存在） | 01 已含 |
| 并发与同步 | 已有归属：99 §6（K-285 单线程+阻塞 sendrec 是 C parity）；07（鉴权缓存） | 99 已含 |
| 测试基建 | **新建 23 承载**（C 三件套/Rust 152 地图/E5(g)/对账门）——本蓝图唯一结构性新增 | §5 契约 23 的验收标准 |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表

编号与文件名全部不变；迁移发生在节级（B 相重建时旧内容去向）。逐篇变化量远小于 DS：

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 断链风险 |
|--------|--------|--------|---------|---------|
| 00 §2 末行 | "落地条目见 todo.md P1-1" | 00 §2（指向 server.rs 现状） | 改写（E2） | 低 |
| 01 §3 末行 | "否决搬 main 循环……只能搬空壳" | 01 §3/§4（P1-1 补记） | 改写（E1）+增补 | 中：该句是"verdict-first 否决"论证的一部分，改写时保留 verdict-first 的历史论证框架、只更新结论段 |
| 01/02 §4.2 | dispatch.rs:217（map_sysctl_reply） | 同位（:254） | 行号校正（E4） | 低（模式 77 行号漂移） |
| 02 §5.1 | 测试快照声明 | 同位 + 23 对账门指针 | 增补 | 低 |
| 08（新小节） | （无） | 08 §2/§3 之间插 A-3 预算小节 | 增补（K-120/G2） | 低（纯新增） |
| 11 §4 | （无 sysctl_abi 句） | 11 §4 末尾一句 | 增补（K-155/G7） | 低 |
| 15 §2.3 | "加密顶层表"标题 | 15 §2.3（改正拆分） | 改写（E6） | 低 |
| 18 §2.4 | KERN_PROC_TTY 行（无 bug 标注） | 同位补 MINIX3 BUG 标注 + proc2.rs 取法字据 | 增补（E7/K-237） | 中：需与代码侧一致裁决（B 相先核 proc2.rs 再落笔） |
| 各篇 §5.1 | 94/101/107 快照 | 同位（加基线日期与 23 指针） | 增补 | 低 |
| 99 §5 后 | （无测试索引） | 99 增一句指向 23 | 增补 | 低 |
| plan.md :6/:207 | "执行半待建"/107 基线 | 同位（执行半已落地/152） | 回写（E3，B 相附带） | 低 |

### 8.2 引用迁移表

**入站引用**（编号不变 ⇒ 文件名级零断链；列热点供 B 相核对语义未变）：

| 引用方 | 处数 | 引用对象 | 核对要点 |
|--------|------|---------|---------|
| `edge_todo.md` | 23 | E-RMIBWIRE/E-MIBPROD/E-MIBGRANT/E5(g)/E-ISWIRE/E-MINTYPES-SYS 等 | 事实性引用；B 相不回改 edge（edge 自治） |
| `13-stage-ipc/`（plan 8 + 03 篇 3 + 99/01 各 1） | 13 | **22 篇点名 4 次**（最多）+ 目录级 | ipc-server 声称复用 rmib MountTable 但零代码引用（todo P3-2 记录的 Claimed-Reuse）——22 篇语义面保持稳定 |
| `17-stage-net/`（plan 4 + 99 篇 2 + 00 篇 1） | 7 | 12/22（lwip 挂载方视角） | 12/22 协议面保持 |
| `11-stage-devman/plan.md` | 3 | 目录级（devman 客户端经 DS 查 label，不直用 MIB） | 无需动作 |
| `04-stage-pm`、`05-stage-vfs` 的 doc_rerank_* | 5 | 目录级 | 其它 AI 产物（bagging 约束未读）；目录引用安全 |
| `00-master-plan/README.md` | 2 | boot 排位（table.c:60） | 00/01 表述保持 |
| `14-stage-runtime`（plan 2 + todo 1）、`12-stage-input/plan.md`、`edge3.md`、`09-stage-init/draft` | 各 1~3 | 目录级 | 无需动作 |
| `07-stage-ds/doc_rerank_glm.md`（本执行者前一轮产物） | 1 | 目录级 | 自家产物，无需动作 |

**出站引用**：`../01-stage-kernel/09-vm-boot-protocol.md`、`../01-stage-kernel/12-ipc-core.md`、`../07-stage-ds/01/02/08` 共 5 个目标全部实测存在，零断链。

**代码注释引用**（4 文件 5 处）：`os/libs/minix-sys/src/lib.rs`（2）、`os/libs/minix-sys/src/rmib.rs`（1，指 22 篇）、`os/servers/mib/src/lib.rs`（1）、`os/servers/mib/src/main.rs`（1）——编号不变零迁移；B 相改各篇标题措辞时同步核对。

### 8.3 断链成本摘要

- **文件名级断链：0 处**（保号方案直接收益；入站 8 个 stage 中 13-stage-ipc 对 22 篇点名 4 次是最热点）。
- **节级引用**：stage 内互引约 180 处，全部是"同编号文档 §N/D-N"形态且各篇节骨架（§1-§7 七节模板）不变；需人工核对的约 15 处（E1/E4/E6/E7 五处勘误的邻近引用 + 各篇 §5.1 计数引用）。批量方式：不适用 sed（无编号变化）；B 相每篇完成时 `rg "NN-mib-[a-z-]+\.md" notes/ os/` 抽验目标节存在。
- **热点文件**：`10`（被引 16）、`02`（13）、`06`（11）、`12`（10）——B 相排期上 02/10 宜先做（它们是协议与分发枢纽，勘误 E4 也落在这两篇）。
- **外部成本**：52 处入站引用零回改；约束是 02（线格式）/12（协议）/17/18（kinfo 布局）/22（rmib 形状）的语义面不得在重建中变调——契约"事实底线"已钉死。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**：逐篇契约前置——00∅ → 01{00} → 02{01} → 03{02} → 04{01,03} → 05{03,04} → 06{01,02} → 07{02,03} → 08{03,05,06,07} → 09{03,06,07} → 10{03,05,07,09(+12 声明)} → 11{02,03,06,07,10} → 12{02,03,06(+DS 声明)} → 13{03,06,07,09,10} → 14{13} → 15{02,03,08} → 16{02,06,10} → 17{16} → 18{16,17} → 19{16,18} → 20{16,17} → 21{02+全服务器} → 22{02,06,12} → 23{07~22} → 99{全部}。**全部指向更早编号，无前向**。（口径：前置=内容依赖；"不讲什么→去向"的边界指针允许指后篇——R 相提示词契约模板自身强制字段。10 对 12 的 ERESTART 依赖已由 10 §1.4 自足解释，D4 判定无需补偿。）
2. **依赖关系图无环**：上述前置边构成 DAG（从任一节点沿前置边必达 00∅ 终止；13→14 链与 16→17→18→19/20 链均为线序）。**通过。**
3. **覆盖率检查**：知识点池约 150 条逐一对照 §5 契约——00:K-001~003；01:K-010~019；02:K-030~043；03:K-050~059；04:K-070~076；05:K-080~084；06:K-090~098；07:K-100~105；08:K-110~120；09:K-130~137；10:K-140~147；11:K-150~157；12:K-160~171；13:K-180~188；14:K-190~196；15:K-200~206；16:K-210~216；17:K-220~228；18:K-230~237；19:K-240~246；20:K-250~255；21:K-260~265；22:K-270~276；99:K-280~285；23:K-290~292。**全部有去向，删除项 0**；新增约 13 条全部带锚点（K-237 的 C 源锚 :875/:707 本轮实测）。**通过。**
4. **断链成本统计**：见 §8.3——文件名级 0、节级约 15 处人工核对、外部 52 处零回改。**已算清。**

### 9.2 自检门逐门结果

| 门 | 检查与结果 |
|----|-----------|
| G1 | C 真序逐条可核对：随机抽十条——S1（table.c:60 ✓ 本轮 sed 实测）、S4（main.c:419-425 ✓）、S6（tree.c:1493 csize=size 注释 ✓）、L2（main.c:449-454 ✓）、Q1⑤（main.c:334-340 半份丢弃 ✓）、Q2（:368-377 ✓）、Q3 快照先行（:1405-1406 ✓）、R2 ENOMEM 例外（tree.c:1741-1744 ✓）、R3 后授先撤（remote.c:441-446 ✓）、E7 反向三元（proc.c:875 vs :707 ✓ 本轮读源发现）。**通过** |
| G2 | 池完整性：C 侧——8 个 .c 逐文件有归属（main→01/04/06/07、tree→03~12、remote→12、kern→13、proc→16~20、hw/vm→14、minix→15、mib.h→03/04、Makefile→明确排除）；协议头 5 文件→02/12/22；客户端 libc 五文件→21、rmib→22；非 C 制品——test87/rmibtest/t_sysctl→23（新建）、boot 锚→00/01、交换格式→02/11/17/18/20；include guard 类非语义项沿 todo Gate A 判定排除（5 个机器缺口逐一有判定）。**通过** |
| G3 | 前向引用为零：见 §9.1 检查 1。**通过** |
| G4 | 依赖图无环：见 §9.1 检查 2，无需拆解。**通过** |
| G5 | 覆盖率 100%：见 §9.1 检查 3，删除项 0、新增锚点齐全。**通过** |
| G6 | 拆合去向/新建来源抽查（本蓝图无拆合；抽新建与勘误十处）：23 篇新建来源=K-290~292 三条各带锚（tests/ 行数实测 + edge 条目）✓；C24 来源=todo L4 的 E5(g) 判定 ✓；C2 的 E1 来源=server.rs:65-547 实测 + 01 §3 原文 ✓；C9 的 K-120 来源=heap.rs:35 实测 + plan A-3 行 ✓；C19 的 E7 来源=proc.c:875 vs :707 读源实测 ✓；C16 的 E6 来源=15 §2.3 原文 ✓；C2 的 E5 来源=01 §2.3 原文 ✓；C26 的 E3 来源=plan.md:6,:207 原文 + todo 状态列 ✓；C12 的 K-155 来源=sysctl_abi.rs 存在性 + describe.rs 派生（todo P1-3 复核记录）✓；C3 的 E4 来源=dispatch.rs:217,254 grep 实测 ✓。10/10 有来源且全部实测。**通过** |
| G7 | 契约七要素：25 篇契约逐篇含定位/讲什么/不讲什么/前置/后置/事实底线/验收标准（知识点清单以 K 编号指涉 §2.1——每条 K 行自带类型/锚点/主标记，等效于逐篇清单）。**通过** |
| G8 | 迁移表覆盖：§8.1 逐条列到节级（11 行，覆盖全部变化点）；§8.2 覆盖文档引用（入站 19 文件 52 处分类）与代码注释（4 文件 5 处逐条）。**通过** |
| G9 | 事实断言锚点抽查十条：MIB_PROC_NR=7（com.h:66 ✓ 实测）、MIB_BASE=0x1800（:1022 ✓）、NR_MIB_CALLS=3（:1030 ✓）、COMMON_MIB_CALL=0xE05（:616 ✓）、rmib.c=1089 行（wc 实测 ✓）、test87=3662 行（wc 实测 ✓）、152 测试（cargo test 实测 ✓）、MibBudget 256KiB（heap.rs:35 ✓）、64 轮上限（server.rs:82 ✓）、sysctl_abi 55 处 offset_of（grep 实测 ✓）。推测项：E7 的"C 源疑似 bug"判定——代码反 direction 本轮读源实证，"疑似"定性保守（不排除有意行为，但 :707 对照与 :873 注释使笔误概率极高），已在 K-237 标注需 B 相与代码侧一致裁决。**通过** |

### 9.3 结论与待用户裁决的问题

**结论**：蓝图完成。新目录 = 保号 24 篇 + 新建 23-mib-verification，共 25 篇；知识点池约 150 条全覆盖；勘误 7 项（E1 执行半补记/E2 现状句/E3 plan 回写/E4 行号漂移/E5 その/E6 加密/E7 **C 源 bug 标注**）+ 结构性缺口 1 项（G5 测试面建篇）+ 轻量增补 4 项（G2 预算/G7 abi/G8 func registry/G9 label 现状）；断链成本 = 文件名级零、节级约 15 处、外部 52 处零回改。四项机械检查与 G1-G9 全部通过。

**待裁决**：

1. **23 篇新建是否成立**（本蓝图判定成立：4003 行 C 契约 + 152 Rust 测试 + E5(g) 联调面横切全部 handler；若否决，K-290~292 按 §5 契约 23 反向拆入 13~22 各篇 §5 与 99——C 契约逐 case 将分散且 rmibtest 拒绝场景无单一挂点）。
2. **E7 的裁决方向**（proc.c:875 反向三元）：本蓝图判定 B 相先核 `proc2.rs` 现状——若 Rust 按意图实现（!zombie 读 fproc），在 18 篇与代码双处补 MINIX3 BUG 标注（超集修复字据）；若按 C 实况实现，注明 bug parity 并建议后续修复轮。两种方向都要两处一致，不许只改一边。
3. **plan.md 回写（E3）的时机**（本蓝图判定随 B 相首篇重建顺带回写，不单独开工；若共识蓝图另有 doc-plan 同步批，归并即可）。
