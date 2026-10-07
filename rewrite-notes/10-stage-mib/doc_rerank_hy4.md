# 10-stage-mib 文档重建蓝图（HY4）

## 执行头部

```text
your_name(AI agent name) = HY4
target_dir(关注的工作目录) = 10-stage-mib
repo_root(仓库根目录) = /home/xzhao/github/minix-rs

任务 = R 相·重建蓝图：输出 target_dir/doc_rerank_hy4.md，不改任何正文。
       只读 C 代码不够，还必须读：本目录全部文档的头部声明、os/ 目录对应入口、
       00-*-overview.md 的导航表。
约束 = 不得引用 .design/ 与 tmp_design_and_todo/；任何落盘产物必须带
       _hy4 后缀；不得读取或照抄其它 AI 的 doc_rerank_* 产物。
```

> **执行声明**：本文件是 HY4 唯一落盘产物。执行过程中用 `rg`/`sed`/`wc` 采证的中间
> 结果全部写在 §0.4 与 §2 内，不另落文件。

---

## 0. 元数据

### 0.1 基本信息

| 项 | 值 |
|---|---|
| 执行者 | HY4 |
| 日期 | 2026-09-19 |
| 目标目录 | `rewrite-notes/10-stage-mib/` |
| 仓库根 | `/home/xzhao/github/minix-rs` |
| 当前提交 | `ebc8ae72b`（`git log --oneline -1` 实测，2026-09-19 采样） |
| 报告性质 | R 相·重建蓝图（不改任何正文） |

### 0.2 审查范围

**算文档（重建对象）**：`00-mib-overview.md`、`01`~`22` 共 22 篇概念文档、
`99-mib-global-concepts.md`。合计 24 篇、3372 行（`wc -l` 实测，已排除
`plan.md`/`todo.md`/`README.md`/`doc_rerank_*`）。

**算参考材料（不重建，但作为知识来源与断链统计对象）**：`plan.md`（467 行）、
`todo.md`（317 行）、`README.md`（48 行）、`draft/README.md`（1327 字节，占位）。

**算范围外**：`.review/` 下的历史 scan/structure 记录（时点快照，见 §8.3 的处置建议）、
`../edge_todo.md`（跨 stage 条目唯一入口）、其它 stage 的文档。

### 0.3 读取清单

| 类别 | 实际读取 |
|---|---|
| 文档 | 24 篇全部（`00`/`99` 全文；`01`~`22` 的头部声明 + `## ` 章节骨架 + 抽样精读） |
| C 源码 | `servers/mib/main.c`（492，全文）、`mib.h`（390，全文）、`remote.c`（477，全文）、`tree.c`（1842，`1-250` + `1332-1700` 精读 + 全文件函数行号表）、`kern.c`（表段 + 函数段）、`vm.c`、`hw.c`、`minix.c`（89，全文）、`proc.c`（函数行号表） |
| 协议头 | `minix/include/minix/com.h`（MIB 段）、`minix/include/minix/ipc.h`（六种载荷行号）、`sys/sys/sysctl.h`（关键宏行号）、`minix/include/minix/sysctl.h` |
| 客户端 | `lib/libc/gen/sysctl.c` 等 5 文件、`minix/lib/libsys/rmib.c`（1089）、`minix/include/minix/rmib.h`（188） |
| Rust 入口 | `os/servers/mib/src/` 全部 34 文件 11680 行（`lib.rs`/`main.rs`/`server.rs` 全文或头部精读，其余行号表）、`os/libs/minix-types/src/{ipc/mib.rs,types/sysctl.rs,types/sysctl_abi.rs,types/kinfo.rs,types/ps_strings.rs}`、`os/libs/minix-sys/src/rmib.rs` |
| 非 C 制品 | `servers/mib/Makefile`（21）、`kernel/table.c:52-63`、`kernel/main.c:263-267`、`minix/tests/test87.c`（3662）、`minix/tests/rmibtest/rmibtest.c`（267）、`tests/kernel/t_sysctl.c`（74）、`tests/sbin/sysctl/t_sysctl.sh`（45） |
| 边界材料 | `../00-master-plan/README.md`（stage 划分与启动因果链）、`../edge_todo.md`（MIB 相关条目）、`../09-stage-init/12-init-sysctl-interaction.md`（前序 stage 已讲内容）、`../01-stage-kernel/06-todo.md`（契约写法范例，只读写法不搬结论） |

### 0.4 关键命令与证据摘录

```bash
# 文档体量（3372 行 / 24 篇）
wc -l rewrite-notes/10-stage-mib/*.md
# 00:108 01:229 02:247 03:196 04:153 05:122 06:163 07:149 08:165 09:154
# 10:174 11:157 12:206 13:185 14:167 15:168 16:167 17:220 18:165 19:152
# 20:141 21:100 22:140 99:82

# C 真值（8 个 .c + mib.h = 5401 行）
wc -l minix3/minix/servers/mib/*
# main 492 tree 1842 remote 477 kern 508 proc 1288 hw 140 minix 89 vm 154 mib.h 390

# Rust 真值（34 文件 11680 行；执行半已落地，与文档描述严重脱节）
wc -l os/servers/mib/src/*.rs os/servers/mib/src/*/*.rs
# server 876  walker 1125  arena 611  transport 469  heap 190  proc/tables 549
# proc/lwp 692  subtree/kern 657  tree/dynamic 507  io/copy 407 ...
grep -rc "#\[test\]" os/servers/mib/src --include=*.rs | awk -F: '{s+=$2} END {print s}'
# → 152

# boot 登记（table.c:52-63 实测；MIB 在 TTY 之后、VM 之前）
sed -n '52,63p' minix3/minix/kernel/table.c
# {TTY_PROC_NR, "tty"}, {MIB_PROC_NR, "mib"}, {VM_PROC_NR, "vm"}, ...
sed -n '64,68p' minix3/minix/include/minix/com.h
# TTY_PROC_NR 5 / DS_PROC_NR 6 / MIB_PROC_NR 7 / VM_PROC_NR 8

# 协议面锚点
grep -n "MIB_BASE\|MIB_SYSCTL\|MIB_REGISTER\|MIB_DEREGISTER\|NR_MIB_CALLS\|COMMON_MIB" \
  minix3/minix/include/minix/com.h
# 613 COMMON_MIB_INFO 616 COMMON_MIB_CALL 622 COMMON_MIB_REPLY
# 1022 MIB_BASE 0x1800 1024 IS_MIB_CALL 1026-1028 三信 1030 NR_MIB_CALLS=3
grep -n "mess_lc_mib_sysctl\|mess_mib_lc_sysctl\|mess_lsys_mib_register\|mess_mib_lsys_call\|mess_mib_lsys_info\|mess_lsys_mib_reply" \
  minix3/minix/include/minix/ipc.h
# 431-433 / 1379-1382 / 1388 / 1551 / 1568 / 1579（union 臂 2455/2561-62/2581-83）
grep -n "define	CTL_MAXNAME" minix3/sys/sys/sysctl.h          # 75: 12
grep -n "define	CTL_QUERY\|define	CTL_CREATE\|define	CTL_DESTROY\|define	CTL_DESCRIBE\|define	CTL_CREATESYM\|define	CTL_MMAP" \
  minix3/sys/sys/sysctl.h                                       # 159-164
grep -n "define SYSCTL_VERS_1\|struct sysctlnode" minix3/sys/sys/sysctl.h  # 132-133 / 1382

# 执行半"文档未覆盖"的证据：Rust 侧 3271 行无任何一篇正文对应
wc -l os/servers/mib/src/server.rs os/servers/mib/src/walker.rs \
      os/servers/mib/src/tree/arena.rs os/servers/mib/src/transport.rs os/servers/mib/src/heap.rs
# 876 + 1125 + 611 + 469 + 190 = 3271
grep -rn "server.rs\|walker.rs\|arena.rs\|transport.rs\|heap.rs" \
  rewrite-notes/10-stage-mib/0*.md 1*.md 2*.md 99*.md | grep -v doc_rerank | wc -l
# → 极稀疏：仅 10 篇一句 "P1-2 补记"、04/08/13/15 四处 arena 指针、06/07 各一句 P1-4 补记

# A-4 尾随结构已钉但文档未记（新增知识点来源）
grep -n "^pub struct" os/libs/minix-types/src/types/sysctl_abi.rs
# SysctlNodeChild 51 / SysctlNodeData 65 / SysctlNodeUn 79 / SysctlNode 101
# SysctlDesc 134 / KinfoLwp 154 / KiSigset 211 / KinfoProc2 225 / Clockinfo 428 / Timeval 446
grep -n "^pub struct" os/libs/minix-types/src/types/kinfo.rs        # KinfoStruct 20
grep -n "^pub struct" os/libs/minix-types/src/types/ps_strings.rs   # PsStrings 22

# 引用关系（断链成本统计）
grep -rn "10-stage-mib/[0-9]" --include=*.md . | grep -v doc_rerank \
  | grep -v "^./rewrite-notes/10-stage-mib/" | wc -l   # → 100
#   其中 .review/ 历史记录 93 处、13-stage-ipc 活跃文档 7 处
grep -rn "10-stage-mib" os/ --include=*.rs | wc -l                          # → 5
grep -rno "[0-9][0-9]-mib-[a-z0-9-]*\.md" \
  rewrite-notes/10-stage-mib/*.md | awk -F: '{print $NF}' \
  | sort | uniq -c | sort -rn                                               # 目录内 171 处
```

---

## 1. C 真序

### 1.1 阶段类型判定

**判定：服务事件循环型（叠加"注册/挂载"第二入口）**，理由：

- `mib_startup()` → `mib_init()` → `for(;;)` 三段是严格线性的启动段；
- 循环段是标准的"收信 → 按 `m_type` 三岔 → 回信"，且 `mib_sysctl` 内部
  再嵌一层"解码 → 分发 → 逐层解析"的调用链；
- 除 `MIB_SYSCTL` 外还有 `MIB_REGISTER` / `MIB_DEREGISTER` 两条**单向**入口
  （`remote.c:210-211/296-297`），它们不是 sysctl 的分支而是独立的生命周期通道；
- 因此本 stage 的"真序"是**一条启动链 + 两条运行时通道**（一问一答 + 挂载/卸载）。

### 1.2 真序表 · 段一：启动（boot → 进入循环）

| # | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| S1 | boot_image 登记 MIB | `kernel/table.c:52-63`（`{MIB_PROC_NR,"mib"}` 在 TTY 后、VM 前）；`com.h:66` `MIB_PROC_NR=7` | 早登记的原因：`init(8)` 启动期就调 `sysctl(2)`（`main.c:15-17` 注释） |
| S2 | 全部用户进程置 `RTS_VMINHIBIT` | `kernel/main.c:263-267` | VM 建页表前禁止调度，MIB 也不例外 |
| S3 | VM 建页表、解除抑制 | `01-stage-kernel/09-vm-boot-protocol`（本 stage 不展开） | MIB 第一次获得被调度资格 |
| S4 | `main()` 入口 | `main.c:433-435` | 声明 `m_in/m_out`、`r`、`ipc_status` |
| S5 | `mib_startup()` | `main.c:415-428` | 注册两个 SEF 回调 + `sef_startup()` |
| S6 | `sef_setcb_init_fresh(mib_init)` | `main.c:419` | 冷启动锚点 |
| S7 | `sef_setcb_init_restart(mib_init)` | `main.c:425` | 重启锚点；注释明说"丢全部动态状态，只留静态树比不跑强" |
| S8 | `sef_startup()` | `main.c:427` | 回调在此被触发（即 S9 在此执行） |
| S9 | `mib_init()` 四棵子树接线 | `main.c:395-398` | `mib_kern_init` / `mib_vm_init` / `mib_hw_init` / `mib_minix_init`；注释解释"跨模块无法 `sizeof` 外部数组" |
| S10 | `mib_tree_init()` | `main.c:404` → `tree.c:1520-1536` | 计数器清零、root 版本=1、parent=NULL |
| S11 | `mib_tree_recurse(&mib_root)` | `tree.c:1480-1512` | 递归：承父版本、填 `node_parent`、累加 `mib_nodes`/`node_clen`；`node_csize = node_size` |
| S12 | `mib_remote_init()` | `main.c:407` → `remote.c:40-49` | `endpts[32]` 全清：`endpt=NONE`、`nodes=NULL` |
| S13 | 进入主循环 | `main.c:443` | `for(;;)` |

### 1.3 真序表 · 段二：主循环（三条通道）

| # | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| L1 | `sef_receive_status(ANY,&m_in,&ipc_status)` | `main.c:445` | 读的是 **status 字**，不是 call number |
| L2 | 收信失败 → `panic` | `main.c:445-446` | 与 Rust 侧"连续 64 次才死"（DS 纪律）是已知偏离 |
| L3 | `is_ipc_notify(ipc_status)` → 打印 + `continue` | `main.c:449-454` | MIB 不收任何 notify 消息体 |
| L4 | `memset(&m_out,0,...)` | `main.c:456` | 回信总是从零开始 |
| L5 | `MIB_SYSCTL` → `mib_sysctl` | `main.c:459-462` | 通道 A，见 §1.4 |
| L6 | `MIB_REGISTER` → `mib_register` | `main.c:464-467` → `remote.c:197-233` | 通道 B，见 §1.5 |
| L7 | `MIB_DEREGISTER` → `mib_deregister` | `main.c:469-472` → `remote.c:291-307` | 通道 B 的逆 |
| L8 | `default`：SENDREC→`ENOSYS`，其余→`EDONTREPLY` | `main.c:474-478` | 野号码双态 |
| L9 | `r != EDONTREPLY` → `m_out.m_type = r` + `ipc_sendnb` | `main.c:482-487` | 唯一回信规则；`sendnb` 失败只 printf |

### 1.4 真序表 · 段三：`mib_sysctl` 一次请求的完整旅程（次主线）

| # | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| A0 | 用户态 `__sysctl` 发 `MIB_SYSCTL` | `minix/lib/libc/sys/__sysctl.c`（40 行）；`libc/gen/sysctl.c:61-391` | `CTL_USER` 在 libc 本地处理，不进内核 |
| A1 | 门 1：非 SENDREC → `EDONTREPLY` | `main.c:292-293` | 只处理阻塞调用 |
| A2 | 取 `endpt/oldaddr/oldlen/newaddr/newlen/namelen` | `main.c:295-300` | |
| A3 | 门 2：`namelen==0 || namelen>CTL_MAXNAME(12)` → `EINVAL` | `main.c:302-303`；`sys/sys/sysctl.h:75` | |
| A4 | 门 3：名字取道 —— `>CTL_SHORTNAME(8)` 走一次 `sys_datacopy`，否则 `memcpy` | `main.c:309-315`；`ipc.h:15` | 8 个 int 内嵌在消息里，省一次内核拷贝 |
| A5 | 门 4：构造 `oldp`（`oldaddr!=0` 才建；`oldlen` 非零也不计较） | `main.c:322-328` | 对"未初始化 oldlen"宽宏 |
| A6 | 门 5：构造 `newp`（`newaddr!=0 && newlen!=0` 才建，否则两者皆弃） | `main.c:334-340` | 与 NetBSD 一致：一个为零则两者皆弃 |
| A7 | 构造 `mib_call`（endpt/name/namelen/flags=0/reslen=0） | `main.c:347-351` | |
| A8 | `mib_dispatch(&call, oldpp, newpp)` | `main.c:353` → `tree.c:1332-1474` | 见 §1.6 |
| A9 | 收尾：`r>=0` → `oldlen=r`；`oldaddr!=0 && oldlen<r` → `ENOMEM`（部分拷贝 + 完整长度） | `main.c:368-374` | ENOMEM 在 sysctl 协议里是"再来一次"的信号，不是纯失败 |
| A10 | 收尾：`r<0` → `oldlen = call.call_reslen` | `main.c:375-376` | create 撞名时 EEXIST 携带现有节点 |

### 1.5 真序表 · 段四：`mib_register` / `mib_deregister`（第二条次主线）

| # | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| B1 | 门：SENDREC → `ENOSYS` | `remote.c:210-211`（register）、`:296-297`（deregister） | 单向约束：防与"MIB→服务"的请求交叉死锁；副作用是用户态无法注册 |
| B2 | `mib_get_label`：向 DS 取 label 证明对方是服务 | `remote.c:216-218` → `:81-104` | 失败 → `EDONTREPLY`（静默丢弃） |
| B3 | 消息级上界检查 `miblen > 8` → `EDONTREPLY` | `remote.c:221-223` | |
| B4 | `mib_do_register`：查找/复用 eid | `remote.c:125-148` | 同端点复用；同 label 不同端点 → 视为旧端点死亡 → `mib_down` |
| B5 | 同 rid 重复挂载 → 拒绝 | `remote.c:155-165` | |
| B6 | 占位 `endpts[eid]`（label/endpt/nodes） | `remote.c:172-176` | |
| B7 | `mib_mount` 参数门：`miblen<2` → `EPERM`（禁顶层挂载） | `tree.c:1572-1579` | 唯一的安全策略（注释里有"将来改白名单"的 TODO） |
| B8 | `mib_mount` flags 门（版本/类型/允许位） | `tree.c:1583-1592` | |
| B9 | `mib_mount` csize/clen 门（≤4096、clen≤csize） | `tree.c:1594-1599` | `MIB_RC_BITS=12` |
| B10 | 路径 walk：逐层必须真实/本地/非私有/非远程/PARENT | `tree.c:1607-1647` | 负 id → `EINVAL`；找不到 → `ENOENT`；不合格 → `EPERM` |
| B11 | 目标存在 → 覆盖挂载（flags 精确匹配；有动态孩子 → `EBUSY`）+ `mib_upgrade` | `tree.c:1656-1682` | |
| B12 | 目标不存在 → 临时挂载点：`COMMON_MIB_INFO` 取向服务要 name/desc | `tree.c:1684-1780`；`remote.c:316-365` | 双 grant 后 `ipc_sendrec`，逆序 revoke |
| B13 | 挂载点入 `endpts[eid].nodes` 链头 | `remote.c:190-191` | |
| B14 | 失败回滚：若无其它挂载点则 `endpts[eid].endpt=NONE` | `remote.c:181-186` | |
| B15 | 永不回信（返回 `EDONTREPLY`） | `remote.c:232`、`remote.c:306` | |
| B16 | deregister：按 (endpt, rid) 摘链 → `mib_unmount` | `remote.c:257-285` → `tree.c:1790-1842` | 摘链在 unmount **之前**（unmount 可能释放节点） |

### 1.6 真序表 · 段五：`mib_dispatch` 逐层解析（`tree.c:1332-1474`）

| # | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| D1 | 从 `&mib_root` 出发，每轮取 `call_name[0]`，`name++`、`namelen--` | `tree.c:1346-1349` | |
| D2 | `assert` 父节点是 `CTLTYPE_NODE` 且有 `CTLFLAG_PARENT` | `tree.c:1351-1352` | |
| D3 | 负 id：必须是末位（否则 `EINVAL`），switch 四类元 op；CREATESYM/MMAP/其它 → `EOPNOTSUPP` | `tree.c:1360-1381` | |
| D4 | `mib_find(parent,id,NULL)` → 找不到 `ENOENT` | `tree.c:1385-1386` → `:34-81` | |
| D5 | `CTLFLAG_PRIVATE` 且未授权 → `EPERM` | `tree.c:1389-1390` | 每层都查 |
| D6 | 非叶且 `CTLFLAG_REMOTE` → `mib_remote_call`；先存 `can_restart = PARENT` | `tree.c:1402-1416` | ERESTART 且不能续走 → `ENOENT`；能续走 → 本地继续 |
| D7 | `is_leaf` / `has_func` / `has_verify` 判定（叶子看 `node_func != NULL`，父看有无 `PARENT`） | `tree.c:1425-1431` | 省字节的代价：两种判定方式 |
| D8 | 叶子但名字还有剩 → `ENOTDIR` | `tree.c:1437-1438` | |
| D9 | 写门：无 `READWRITE` → `EPERM`；无 `ANYWRITE` 且非特权 → `EPERM` | `tree.c:1446-1457` | ANYWRITE 不覆盖 READWRITE |
| D10 | `has_func` → `node->node_func(...)` | `tree.c:1461-1462` | 终点 |
| D11 | 叶子 → `mib_readwrite(..., has_verify ? node_verify : NULL)` | `tree.c:1465-1467` → `:1309-1330` | 终点 |
| D12 | 循环耗尽 → `EISDIR` | `tree.c:1473` | 名字用完落在节点数组上 |

### 1.7 真序表 · 段六：远程转发（`remote.c:378-477`）

| # | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| R1 | 建 name grant（`cpf_grant_direct`, CPF_READ） | `remote.c:396-399` | 无效 → `EINVAL` |
| R2 | `mib_relay_oldp`（CPF_WRITE） | `remote.c:401-405` → `main.c:211-227` | 失败 → 撤 name |
| R3 | `mib_relay_newp`（CPF_READ） | `remote.c:407-413` → `main.c:236-252` | 失败 → 撤 oldp、name |
| R4 | 组 `COMMON_MIB_CALL`：三 grant + 三 len + `user_endpt` + `flags`(是否 root) + `root_ver` + `tree_ver` | `remote.c:422-436` | `req_id=0`（为将来的异步留位） |
| R5 | `ipc_sendrec(endpt,&m)` —— **阻塞** | `remote.c:439` | 与 C 一致，不设计超时 |
| R6 | 逆序 revoke：newp → oldp → name | `remote.c:442-446` | 后授先撤 |
| R7 | IPC 失败 → `mib_down(eid)`（清该端点全部挂载点，可能释放当前 node）+ `ERESTART` | `remote.c:455-459` | |
| R8 | 回信类型/`req_id` 校验 | `remote.c:461-464` | |
| R9 | 服务返回 `ERESTART` → `mib_do_deregister(endpt, rid)` | `remote.c:473-474` | 交叉场景：deregister 信还在路上 |
| R10 | 返回 `m.m_lsys_mib_reply.status` | `remote.c:476` | |

### 1.8 序差事实（供 §4 的教学序决策使用）

| 事实 | 运行时序 | 说明 |
|---|---|---|
| 用户态 `sysctl(3)` 永远在 MIB 之前 | A0 < S1 | 但旧目录把它放在 21（倒数第二） |
| `mib_init` 的四棵子树接线早于 `mib_tree_init` | S9 < S10 | 但子树内容篇（旧 13~15）在旧目录的 13~15，晚于树骨架（03~05）——一致 |
| 远程挂载（`MIB_REGISTER`）与 sysctl 是并行的两条通道 | B 与 A 无先后 | 旧目录只有 12 一篇承载 B 的全部 |
| `mib_mount` 在 `tree.c` 不在 `remote.c` | B7-B12 ∈ tree.c | 旧目录已在 plan.md M-3 修正，但 12 篇仍要同时讲两个文件 |
| Rust 的执行半（server/walker/arena/transport/heap） | 无 C 对应物 | 是 ARCH 演进产物，只能按 Rust 代码重建 |

---

## 2. 知识点全集

> 编号规则：`K-NNN` 在本蓝图内唯一；汇总对齐键 = **名称 + 锚点**。
> 类型取值：概念 / 机制 / 数据结构 / 接口与协议 / 约束与不变量 / 架构演进 / 工具与工程 / 测试性质。
> 来源：**存量** = 现有 24 篇文档里有；**新增** = 现有文档没讲，但 C 源码 / 非 C 制品 /
> Rust 代码 / OS 理论承载。

### 2.1 域 A —— 提问方：sysctl(3) 与外部契约（存量：旧 21）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-001 | sysctl(2) 的 API 形状（name 数组 + oldp/newp + 长度） | 接口与协议 | 存量 | 21 §1/§2 | `lib/libc/gen/sysctl.c:61-391` | 学会"怎么问" |
| K-002 | CTL_USER 子树在 libc 本地处理，不进 MIB | 约束与不变量 | 存量 | 21 §1.3/§2 | `lib/libc/gen/sysctl.c:80-86` | 知道哪些名字 MIB 看不见 |
| K-003 | `sysctlbyname` / `sysctlnametomib` 名字↔编号互转 | 接口与协议 | 存量 | 21 §2 | `sysctlbyname.c`(67)、`sysctlnametomib.c`(72) | 理解名字是给人看的、编号是给机器看的 |
| K-004 | `sysctlgetmibinfo` 的 QUERY + 自排序 | 机制 | 存量 | 21 §2 | `sysctlgetmibinfo.c`(611) | 理解"服务端不保证顺序"（`tree.c:207-212` 注释） |
| K-005 | ENOMEM 两次调用事务（部分拷贝 + 完整长度 + 重试） | 机制 | 存量 | 21 §1.3、01 §2/§3 | `main.c:341-356`、`main.c:368-374` | 学会"ENOMEM 不是纯失败" |
| K-006 | `__sysctl` 真正发消息（libc 里唯一进内核的那一层） | 接口与协议 | 存量 | 21 §2 | `minix/lib/libc/sys/__sysctl.c`(40) | 分清 libc 与内核的接缝 |
| K-007 | libc 侧不重写的契约边界（A-10） | 架构演进 | 存量 | 21 头部、plan A-10 | `plan.md:226` | 知道 minix-rs 的实现边界在哪 |
| K-008 | sysctl(8)/ps(1)/top(1)/libkvm 是这套 ABI 的消费者 | 概念 | 存量 | 21 §1、13 §1 | `sys/sys/sysctl.h:1382-1450` | 知道改布局就是破坏用户态 |

### 2.2 域 B —— 协议面（存量：旧 02）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-009 | MIB 三信的 call number 与号段 | 接口与协议 | 存量 | 02 §2 | `com.h:1022-1030`（`MIB_BASE=0x1800`、`NR_MIB_CALLS=3`） | 能读懂任何一条 MIB 消息的类型字 |
| K-010 | `COMMON_MIB_{INFO,CALL,REPLY}` 跨服务号 | 接口与协议 | 存量 | 02 §2、12 §2 | `com.h:613/616/622` | 分清"用户↔MIB"与"MIB↔服务"两套号 |
| K-011 | 六种 `mess_mib_*` 消息结构与 56 字节约束 | 数据结构 | 存量 | 02 §2 | `ipc.h:431-433/1379-1382/1388/1551/1568/1579`；union 臂 `2455/2561-62/2581-83` | 知道消息能装多少 |
| K-012 | `CTL_SHORTNAME=8` 内嵌名字与长名一次 datacopy | 机制 | 存量 | 02 §2、01 §2 | `ipc.h:15`、`main.c:309-315` | 理解 8 这个魔数的由来 |
| K-013 | `CTL_MAXNAME=12` 名字分量上界 | 约束与不变量 | 存量 | 02 §2、01 §2 | `sys/sys/sysctl.h:75`、`main.c:302-303` | 知道名字最多几层 |
| K-014 | `SYSCTL_VERSION = SYSCTL_VERS_1` 版本门（编译期 #error） | 约束与不变量 | 存量 | 02 §2 | `sysctl.h:132-133`、`mib.h:331-333` | 理解 NetBSD ABI 兼容性约束 |
| K-015 | 元标识符全集（-2~-7）与两类未实现 | 接口与协议 | 存量 | 02 §2、10 §2 | `sysctl.h:159-164`、`tree.c:1377-1380` | 能读懂 QUERY/CREATE/DESTROY/DESCRIBE |
| K-016 | `CTLTYPE_*` / `CTLFLAG_*` 宏与 `SYSCTL_{TYPE,VERS,FLAGS}` 抽取 | 数据结构 | 存量 | 02 §2、03 §2 | `sys/sys/sysctl.h:92-164` | 会手算一个 flags 字的含义 |
| K-017 | `sysctlnode` / `sysctldesc` 交换格式 | 数据结构 | 存量 | 02 §2、11 §2 | `sys/sys/sysctl.h:1382-1450` | 知道用户态拿到的字节长什么样 |
| K-018 | 顶层 `CTL_*` id 与七个顶层节点的对应 | 数据结构 | 存量 | 02 §2、04 §2 | `sysctl.h`、`main.c:46-54` | 会对着名字找编号 |
| K-019 | `minix/sysctl.h`（97 行）Minix3 扩展：`CTL_MINIX`、`MINIX_*`、`minix_proc_*` | 接口与协议 | 存量 | 02 §2、15 §2、20 §2 | `minix/include/minix/sysctl.h` | 分清 NetBSD 原生与 Minix 扩展 |
| K-020 | Rust：`ipc/mib.rs` + `message.rs` 六载荷 56 字节断言 | 架构演进 | 存量 | 02 §3/§4 | `os/libs/minix-types/src/ipc/mib.rs`(420) | 知道 wire 层已被钉死 |

### 2.3 域 C —— 服务诞生与主循环（存量：旧 01）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-021 | boot_image 登记位与 `MIB_PROC_NR=7` | 概念 | 存量 | 00 §2、99 §1 | `kernel/table.c:52-63`、`com.h:66` | 知道 MIB 为什么排在那个位置 |
| K-022 | 早启动的因果（init 启动期就要 sysctl） | 概念 | 存量 | 00 §2、plan §1.1 | `main.c:15-17` | 理解 boot 顺序不是随意的 |
| K-023 | 需要超级用户特权的原因 | 概念 | 存量 | 00 §1、99 §1 | `main.c:17-19` | 理解"路由器而非仓库"的代价 |
| K-024 | `RTS_VMINHIBIT` 抑制与 VM 解除 | 概念 | 存量 | 00 §2 | `kernel/main.c:263-267` | 知道 MIB 何时第一次可调度 |
| K-025 | SEF 双回调注册（fresh / restart） | 机制 | 存量 | 01 §2、04 §2 | `main.c:415-428` | 学会用户态服务的标准开机姿势 |
| K-026 | 重启语义 = 丢动态、保静态（RestartLossy） | 约束与不变量 | 存量 | 01 §2、04 §2、08 §2 | `main.c:420-424` 注释 | 知道重启后什么东西会消失 |
| K-027 | `sef_receive_status` 读 status 字（而非 call number） | 机制 | 存量 | 01 §2 | `main.c:445` | 理解 notify 判定是精确的 |
| K-028 | notify 一律拒绝（打印 + continue） | 机制 | 存量 | 01 §2 | `main.c:449-454` | 知道 MIB 不订阅任何事件 |
| K-029 | 三信 switch + default 双态（ENOSYS / EDONTREPLY） | 机制 | 存量 | 01 §2 | `main.c:458-479` | 能画出主循环骨架 |
| K-030 | `EDONTREPLY` 抑制回信的唯一规则 | 约束与不变量 | 存量 | 01 §2、11 §? | `main.c:482-487` | 知道什么时候不该回信 |
| K-031 | `mib_sysctl` 六道解码门 | 机制 | 存量 | 01 §2/§3 | `main.c:292-351` | 能对每一个错误码指出是哪道门 |
| K-032 | oldp/newp 配对的不对称（旧宽宏、新严格） | 约束与不变量 | 存量 | 01 §2、06 §2 | `main.c:322-340` | 理解"一个为零则两者皆弃" |
| K-033 | 回信 `oldlen` 的双义（成功=长度 / 错误=`call_reslen`） | 机制 | 存量 | 01 §3、02 §2 | `main.c:368-376` | 理解错误也能携带数据 |
| K-034 | Rust：`dispatch.rs` 的 triage / `default_outcome` / `should_reply` / `MibCall::from_raw` | 架构演进 | 存量 | 01 §4 | `os/servers/mib/src/dispatch.rs`(373) | 知道判定层与执行层的切分点 |
| K-035 | 收信失败的处理：C 是 `panic`，Rust 是 64 次上限（DS 纪律） | 架构演进 | 新增 | —（Rust 有，文档无） | `main.c:445-446` vs `server.rs:MAX_CONSECUTIVE_RECEIVE_FAILURES=64` | 知道这是一处有意偏离，不是 bug |

### 2.4 域 D —— 节点模型（存量：旧 03）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-036 | `struct mib_node` 全字段与三个 union（值/指针/辅助） | 数据结构 | 存量 | 03 §2 | `mib.h:188-235` | 能读懂任何一个节点的内存 |
| K-037 | 四类节点矩阵（PARENT × REMOTE） | 概念 | 存量 | 03 §1/§2 | `mib.h:107-126` 注释表 | 看到 flags 就知道这节点是哪种 |
| K-038 | 三个 NetBSD 标志位被 Minix 重新赋义（ROOT/ALIAS/MMAP → PARENT/VERIFY/REMOTE） | 约束与不变量 | 存量 | 03 §2、11 §2 | `mib.h:52-74` | 理解"内部专用、绝不外泄" |
| K-039 | 立即值三态（bool / int / quad） | 数据结构 | 存量 | 03 §2 | `mib.h:204-206` | 知道小数据可以不占指针 |
| K-040 | `node_size` 的双重语义（数据字节数 / 静态孩子槽数） | 约束与不变量 | 存量 | 03 §2 | `mib.h:91-99`、`:128-134` | 避免读代码时踩坑 |
| K-041 | `csize/clen` 与远程复用同一块内存（`eid/rcsize/rclen/rid`） | 数据结构 | 存量 | 03 §2、12 §2 | `mib.h:140-156`、`:193-207` | 理解"挂载时本地计数被覆盖" |
| K-042 | `MIB_EID_BITS=5` / `MIB_RC_BITS=12` 位域与上限（32 服务 / 4096 孩子） | 约束与不变量 | 存量 | 03 §2、12 §2 | `mib.h:181-186` | 知道容量上限从哪来 |
| K-043 | `struct mib_dynode` 变长尾巴（name + data 内嵌单块） | 数据结构 | 存量 | 03 §2、08 §2 | `mib.h:244-249` | 理解一次 malloc 承载三样东西 |
| K-044 | OWNDATA / OWNDESC 所有权 | 约束与不变量 | 存量 | 08 §2/§3、11 §1.2 | `tree.c`（create/destroy 分配释放处） | 知道谁负责释放 |
| K-045 | `scratch` 缓冲（`MAXDESCLEN=1024` + PAGE_SIZE 取大） | 数据结构 | 存量 | 03 §2 | `tree.c:21-23` | 理解"临时区"的三重用处 |
| K-046 | 三计数器 `mib_nodes` / `mib_objects` / `mib_remotes` | 数据结构 | 存量 | 03 §2、15 §2 | `tree.c:25-27` | 知道 `minix.mib.*` 三个节点的数据源 |
| K-047 | `node_ver` 版本与乐观锁 | 机制 | 存量 | 03 §2、08 §2、11 §2 | `tree.c:1505`、`tree.c:194-204` | 理解"版本不对就拒绝" |
| K-048 | `IS_STATIC_ID(parent,id)` 宏 | 机制 | 存量 | 03 §2、05 §2 | `tree.c:12` | 理解静态/动态的第一道分界 |
| K-049 | "空槽 = flags 为 0" 的不变量 | 约束与不变量 | 存量 | 04 §2、05 §2 | `tree.c:51`、`:1498`、`main.c:37-45` 注释 | 能判断一个静态槽有没有在用 |
| K-050 | Rust：`node.rs` 的 `ChildWindow` / `RemotePack` 拒错构造器 | 架构演进 | 存量 | 03 §4 | `os/servers/mib/src/tree/node.rs`(284) | 学会用类型消灭非法状态 |

### 2.5 域 E —— 静态树与初始化（存量：旧 04）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-051 | `mib_root` 是内部根、可写（好让 init 建顶层条目） | 概念 | 存量 | 04 §1/§2 | `main.c:56-62` | 理解"根不可见但可写" |
| K-052 | `mib_table[]` 七个顶层 `MIB_ENODE` | 数据结构 | 存量 | 04 §2 | `main.c:46-54` | 能列出整棵树的第一层 |
| K-053 | `MIB_*` 初始化宏族（NODE/ENODE/BOOL/INT/QUAD/*PTR/STRING/STRUCT/FUNC/INTV） | 数据结构 | 存量 | 04 §2 | `mib.h:252-312` | 会写静态节点 |
| K-054 | `MIB_INIT_ENODE` 补 `size`+`scptr`（跨模块 `sizeof` 不可见） | 机制 | 存量 | 04 §2 | `mib.h:315-319`、`main.c:388-394` 注释 | 理解四行 wiring 为什么存在 |
| K-055 | `mib_init` 的四棵子树接线顺序 | 机制 | 存量 | 04 §2、00 §2 | `main.c:395-398` | 知道子树接入的时点 |
| K-056 | `mib_tree_init` / `mib_tree_recurse` 递归初始化 | 机制 | 存量 | 04 §2 | `tree.c:1479-1536` | 知道父链/版本/计数何时填 |
| K-057 | `CTL_NET` 靠远程注册填充、`CTL_VENDOR` 可写给第三方 | 概念 | 存量 | 04 §2、12 §1 | `main.c:37-45` 注释 | 理解"空着的节点也是设计" |
| K-058 | Rust：`static_tree.rs` 的 `RootSpec` + 四线 `build()` | 架构演进 | 存量 | 04 §4 | `tree/static_tree.rs`(161)、`subtree/*.rs` | 知道静态表是编译期常量表 |
| K-059 | Rust：静态窗口 `reserve(max_id+1)` + `place` 模型 | 架构演进 | 新增 | —（Rust 有，文档无） | `todo.md P1-2` 复核段、`tree/arena.rs` | 理解"惰性 push 会被子树侵占"这起事故 |

### 2.6 域 F —— 查找（存量：旧 05）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-060 | `mib_find` 静态 O(1)（数组索引 + flags≠0） | 机制 | 存量 | 05 §2 | `tree.c:48-57` | 会对静态节点算查找代价 |
| K-061 | 动态按 id 排序链表 O(n) + 早停 | 机制 | 存量 | 05 §2 | `tree.c:69-78` | 理解为什么选链表（用户自选 id） |
| K-062 | `prevpp` 双星输出（为删除服务） | 机制 | 存量 | 05 §2、08 §2 | `tree.c:34-35`、`:71-75` | 理解"查找即定位" |
| K-063 | 负 id 直接返回 NULL（元标识符不参与查找） | 约束与不变量 | 存量 | 05 §2 | `tree.c:40-41` | 分清元 op 与查找的边界 |
| K-064 | Rust：`lookup.rs` 的 `find` | 架构演进 | 存量 | 05 §4 | `os/servers/mib/src/tree/lookup.rs`(139) | 知道判定的 Rust 落点 |

### 2.7 域 G —— 拷贝与 relay 原语（存量：旧 06）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-065 | `mib_oldp` / `mib_newp` 不透明体（endpt + addr + len） | 数据结构 | 存量 | 06 §1/§2 | `main.c:64-83` | 理解"不透明体"的设计意图（可换内嵌回信） |
| K-066 | `mib_inrange`：先问"值不值得准备" | 机制 | 存量 | 06 §2 | `main.c:90-98` | 理解按需计算 |
| K-067 | `mib_copyout` 钳制 + 报全长 | 机制 | 存量 | 06 §2 | `main.c:120-141` | 理解"报的是全长，拷的是能装的" |
| K-068 | `mib_copyin` 精确长度匹配（不等即 EINVAL） | 约束与不变量 | 存量 | 06 §2 | `main.c:172-185` | 理解"写比读严" |
| K-069 | `mib_copyin_aux`（用户指针二次拷入） | 机制 | 存量 | 06 §2/§4 | `main.c:191-202` | 知道二级指针怎么拷 |
| K-070 | `mib_copyin_str` 逐页块 + NUL 扫描 + 耗尽 EINVAL | 机制 | 存量 | 06 §2 | `tree.c:372-421` | 理解字符串拷入为什么要分页 |
| K-071 | `mib_relay_oldp` / `mib_relay_newp`：grant 创建（WRITE / READ） | 机制 | 存量 | 06 §2、12 §2 | `main.c:211-252` | 理解"把用户的缓冲区安全地借给别人" |
| K-072 | 分配/grant 失败**绝不**返回 ENOMEM（改 EINVAL） | 约束与不变量 | 存量 | 06 §2、08 §2、12 §2 | `main.c:207-208` 注释、`remote.c:391-394` | 理解 ENOMEM 在 sysctl 里被征用了 |
| K-073 | `mib_setoldlen` / `call_reslen`：错误携带长度通道 | 机制 | 存量 | 06 §2、01 §3 | `main.c:147-152`、`:375-376` | 理解 EEXIST 也能带回数据 |
| K-074 | 两种传输模型：`sys_datacopy`（自己拷）vs grant（给别人） | 概念 | 存量 | 06 §1/§2 | `main.c:136-138` vs `:216-217` | 分清"拷"与"借" |
| K-075 | Rust：`io/copy.rs` 的 `Oldp` / `Newp` 类型化 | 架构演进 | 存量 | 06 §4（P1-4 补记） | `os/servers/mib/src/io/copy.rs`(407) | 知道 C 不透明体的 Rust 等价物 |
| K-076 | Rust：`io/relay.rs` 的 `RelayRequest::open` / `RelayGrant::close`（所有权防双撤） | 架构演进 | 新增 | —（Rust 有，文档仅一句补记） | `os/servers/mib/src/io/relay.rs`(269) | 学会用所有权表达"必须撤销" |

### 2.8 域 H —— 权限模型（存量：旧 07）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-077 | `mib_authed`：每 call 一次 PM 查询并缓存 | 机制 | 存量 | 07 §2 | `main.c:259-275` | 理解缓存粒度 |
| K-078 | `MIB_FLAG_AUTH` / `MIB_FLAG_NOAUTH` 位 | 数据结构 | 存量 | 07 §2 | `mib.h:49-50` | 知道结果存在哪 |
| K-079 | `CTLFLAG_PRIVATE` 可见性过滤（看也看不得） | 约束与不变量 | 存量 | 07 §2、10 §2 | `tree.c:1389-1390` | 分清"不可见"与"不可写" |
| K-080 | `CTLFLAG_READWRITE` / `CTLFLAG_ANYWRITE` 写门 | 约束与不变量 | 存量 | 07 §2、10 §2 | `tree.c:1446-1457` | 能判一个写请求会不会被拒 |
| K-081 | `CTLFLAG_PERMANENT` 不可销毁 | 约束与不变量 | 存量 | 07 §2、08 §2 | `mib.h:324`、`tree.c`（destroy 检查处） | 知道哪些节点删不掉 |
| K-082 | 鉴权失败即 fail-closed | 约束与不变量 | 存量 | 07 §3 | `main.c:263-268` | 理解安全默认 |
| K-083 | Rust：`auth.rs` 的 `CallAuth` 枚举替代位标志 | 架构演进 | 存量 | 07 §4 | `os/servers/mib/src/auth.rs`(219) | 学会用枚举消灭"两个 bool 的四种组合" |

### 2.9 域 I —— 动态节点生命周期（存量：旧 08）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-084 | `mib_check_name` 名字合法性（非空/NUL 结尾/字符集） | 机制 | 存量 | 08 §2 | `tree.c:247-...` | 知道名字能用什么字符 |
| K-085 | `mib_scan` 定位插入点（含"同名需越位重扫"） | 机制 | 存量 | 08 §2 | `tree.c:278-...`（`:345-354` 注释） | 理解早停会漏同名 |
| K-086 | `mib_create` 的校验按成本由低到高排序 | 机制 | 存量 | 08 §1.3/§2 | `tree.c:486-777` | 理解"便宜的门先判" |
| K-087 | 动态 id 分配与 `CREATE_BASE=1024` | 机制 | 存量 | 08 §2 | `tree.c`（create 内） | 知道动态号从哪开始 |
| K-088 | 三处 C 判定门：csize 溢出 / create 版本 / query 拷入版本 | 约束与不变量 | 存量 | 08 §2/§4（P2-1 补记）、11 §5 | `tree.c:517-519`、`:536-538`、`:194-195` | 知道这三个门是后补的 |
| K-089 | 撞名 EEXIST 且携带现有节点内容 | 机制 | 存量 | 08 §2、10 §3 | `tree.c:652-664` 一带、`main.c:341-356` 注释 | 理解"错误也带数据" |
| K-090 | `mib_upgrade`（父版本递增） | 机制 | 存量 | 08 §2 | `tree.c:427-...` | 理解版本是"整棵子树变了"的信号 |
| K-091 | `mib_add` / `mib_remove` 链表手术与 `RemoveDelta` 差量 | 机制 | 存量 | 08 §2/§4 | `tree.c:457-...`、`:785-...` | 知道计数器怎么回退 |
| K-092 | `mib_destroy` 与 PERMANENT 拒绝 | 机制 | 存量 | 08 §2 | `tree.c:839-...` | 能判一个销毁请求的结果 |
| K-093 | Rust：`dynamic.rs` 的 `scan` / `create_*` / `remove` 判定 | 架构演进 | 存量 | 08 §4 | `os/servers/mib/src/tree/dynamic.rs`(507) | 知道判定层落点 |

### 2.10 域 J —— 树竞技场与内存预算（**新增域**）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-094 | `NodeId(u32)` 句柄替代裸指针（跨 slab 扩容安全） | 架构演进 | 新增 | — | `os/servers/mib/src/tree/arena.rs`(611) | 理解为什么不能照抄 C 的指针 |
| K-095 | `ChildMap = BTreeMap<i32, NodeId>`（有序迭代直接兑现 QUERY 契约） | 架构演进 | 新增 | 03 §4.4、08 §D 行（仅选型结论） | `tree/arena.rs` + `todo.md P2-3` 复核 | 学会"选容器即选算法" |
| K-096 | 否决链表直译（translate 防线）的理由 | 架构演进 | 新增 | 08 §D 行 | `todo.md P2-3` | 知道什么时候"不像 C"是对的 |
| K-097 | 静态窗口 `reserve` + `place`（防父窗口被子树生长侵占） | 机制 | 新增 | — | `todo.md P1-2` 复核段 | 理解一起真实事故 |
| K-098 | `MibBudget` 256 KiB 记账式字节预算（claim 先于分配） | 机制 | 新增 | 04/08/13/15 的"arena 缺口"指针 | `os/servers/mib/src/heap.rs`(190) | 理解"预算即 C 的分配失败" |
| K-099 | 耗尽 → `BudgetExhausted` → 调用方按 C 规则映射 EINVAL（临时挂载点例外 ENOMEM） | 约束与不变量 | 新增 | — | `heap.rs`；C 侧 `tree.c:1741-1744` | 理解语义映射点 |
| K-100 | `reset()` 承接 RestartLossy 全清 | 机制 | 新增 | 01/04 的重启语义 | `heap.rs::reset` + `sef.rs` | 知道重启时预算怎么处理 |
| K-101 | `Dynode` 三段独立所有权（name / data / desc） | 数据结构 | 新增 | 08 §2.3/§2.4（部分） | `tree/arena.rs::Dynode` | 理解 OWNDATA/OWNDESC 的 Rust 表达 |
| K-102 | 否决 bumpalo / 裸全局分配器的理由 | 架构演进 | 新增 | — | `todo.md P2-4` | 学会比较内存策略 |

### 2.11 域 K —— 数据读写（存量：旧 09）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-103 | `mib_getptr` 的四条 lane（立即值 / 数据指针 / 结构 / 字符串） | 机制 | 存量 | 09 §2 | `tree.c:1098-...` | 知道数据到底存在哪 |
| K-104 | `mib_read` / `mib_write` | 机制 | 存量 | 09 §2 | `tree.c:1131-...`、`:1160-...` | 能读读写路径 |
| K-105 | `mib_readwrite` 先读旧再写新 | 机制 | 存量 | 09 §2 | `tree.c:1309-1330` | 理解顺序不可换 |
| K-106 | 字符串长度语义（最大长度 vs 实际长度） | 约束与不变量 | 存量 | 09 §2 | `mib.h:97-99` | 避免 off-by-one |
| K-107 | stage 缓冲：先拷到一边、验好再落子 | 机制 | 存量 | 09 §1.3/§2 | `tree.c:1207` 一带 | 理解"原子性"从哪来 |
| K-108 | verify 回调（`CTLFLAG_VERIFY`） | 机制 | 存量 | 09 §2、13 §2 | `mib.h:73`、`mib.h:169-170` | 知道校验挂钩在哪 |
| K-109 | bool 消毒（非 0/1 → 1） | 约束与不变量 | 存量 | 09 §2 | `tree.c`（readwrite 内） | 理解对外契约的整洁性 |
| K-110 | 非特权用户大数据写入 EPERM（`newlen+1 > SCRATCH_SIZE` 不分配） | 约束与不变量 | 存量 | 09 §2（M-7 补入） | `tree.c`（write 内） | 理解"不给你分配"也是一种拒绝 |
| K-111 | Rust：`data/readwrite.rs` | 架构演进 | 存量 | 09 §4 | `os/servers/mib/src/data/readwrite.rs`(269) | 知道判定落点 |

### 2.12 域 L —— 枚举与描述（存量：旧 11）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-112 | `mib_query` 的版本门（flags 版本 + 节点版本） | 机制 | 存量 | 11 §2 | `tree.c:189-205` | 理解"版本不对就拒" |
| K-113 | 私有节点过滤与立即值隐藏 | 约束与不变量 | 存量 | 11 §2 | `tree.c:114-131` | 理解"能列出来 ≠ 能看到值" |
| K-114 | `mib_copyout_node`：内部标志在出口被剥离 | 机制 | 存量 | 11 §2 | `tree.c:107-108` | 知道哪些标志绝不出门 |
| K-115 | 远程节点报 `rcsize/rclen` 而不是本地计数 | 机制 | 存量 | 11 §2、12 §2 | `tree.c:157-161` | 理解"不为取数去调远程" |
| K-116 | 函数节点报 `sysctl_func = SYSCTL_NODE_FN`（防下钻 + 防 ASR 泄漏） | 约束与不变量 | 存量 | 11 §2 | `tree.c:167-168` | 理解 trace(1) 的约定 |
| K-117 | `mib_copyout_desc`：12 字节头 + 对齐 | 数据结构 | 存量 | 11 §2 | `tree.c:926-966` | 会算描述流偏移 |
| K-118 | `mib_describe` 读/写描述与"只写一次" | 机制 | 存量 | 11 §2 | `tree.c:973-1091` | 理解描述设置的约束 |
| K-119 | Rust：`SysctlDesc` 派生 `DESC_HEADER`/`DESC_ALIGN`（消灭双真相源） | 架构演进 | 新增 | —（Rust 有，文档仅 P1-3 补记） | `describe.rs` + `types/sysctl_abi.rs:134` | 学会从结构派生常量 |

### 2.13 域 M —— 传输接缝（**新增域**）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-120 | `MibKernel` trait（datacopy 双向 / grant_magic / grant_revoke / getproctab / getticks / hz） | 接口与协议 | 新增 | — | `os/servers/mib/src/transport.rs`(469) | 知道内核动词的形状 |
| K-121 | `MibServices` trait（getnuid / getsysinfo / ds_retrieve_label / vm_info / pm_getparam） | 接口与协议 | 新增 | — | `transport.rs` | 知道对端服务的形状 |
| K-122 | `SysTransport` 真实端全 `-EIO` 诚实桩（E1/E2 通电即换体） | 架构演进 | 新增 | — | `transport.rs` + `todo.md P1-4` 复核 | 理解"先定形状后通电" |
| K-123 | `Recorder` 录制 mock（动词序列断言） | 工具与工程 | 新增 | — | `transport.rs::recording` | 学会断言"调用了什么、按什么序" |
| K-124 | 嵌套 `sendrec` 的阻塞语义 = C parity（不设计超时） | 约束与不变量 | 新增 | 12 §3（一句） | `remote.c:439`；`todo.md §4 L2` | 知道这不是缺陷 |
| K-125 | MIB seam 形状（grant + sendrec + 三服务查询）为何不照搬 SCHED | 架构演进 | 新增 | — | `todo.md §4 L0` | 理解 seam 设计的可移植性边界 |
| K-126 | "不可移动约束"不适用于 MIB（对比 DS 的池内指针） | 架构演进 | 新增 | — | `todo.md §4 L0` | 知道哪些先例不能照抄 |
| K-127 | `MibIpc` 的四个动词（receive_status / send_nb / send_rec / notify） | 接口与协议 | 新增 | — | `server.rs::MibIpc` | 知道消息接缝的形状 |

### 2.14 域 N —— 远程子树：注册与挂载（存量：旧 12 上半）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-128 | `endpts[32]` 表（endpt / nodes 链头 / label） | 数据结构 | 存量 | 12 §2 | `remote.c:24-35` | 能画出端点表 |
| K-129 | 注册信必须单向（SENDREC → ENOSYS） | 约束与不变量 | 存量 | 12 §2、22 §1.2 | `remote.c:210-211`、`:296-297` | 理解防死锁与"用户态不能注册"的副作用 |
| K-130 | `mib_get_label`：DS label 即"我是服务"的证明 | 机制 | 存量 | 12 §2 | `remote.c:81-104`、`:216-218` | 理解身份校验方式 |
| K-131 | 同 label 不同端点 = 旧端点死亡 → `mib_down` | 机制 | 存量 | 12 §2 | `remote.c:128-135` | 理解重启服务的处理 |
| K-132 | 同 rid 重复挂载被拒 | 约束与不变量 | 存量 | 12 §2 | `remote.c:155-165` | 知道冲突策略 |
| K-133 | 挂载参数门：`miblen<2` → EPERM（禁顶层挂载） | 约束与不变量 | 存量 | 12 §2 | `tree.c:1572-1579` | 理解唯一的安全策略 |
| K-134 | 挂载 flags 门（版本/类型/允许位） | 约束与不变量 | 存量 | 12 §2 | `tree.c:1583-1592` | 能判一个挂载请求的 flags 合法性 |
| K-135 | csize/clen 门（≤4096，clen≤csize） | 约束与不变量 | 存量 | 12 §2 | `tree.c:1594-1599` | 知道容量上限 |
| K-136 | 路径 walk 的四条逐层要求（真实/本地/非私有/非远程 + PARENT） | 机制 | 存量 | 12 §2 | `tree.c:1607-1647` | 会对一条挂载路径逐层判合法性 |
| K-137 | 覆盖挂载：flags 精确匹配 + 无动态孩子（否则 EBUSY）+ `mib_upgrade` | 机制 | 存量 | 12 §2 | `tree.c:1656-1682` | 理解"盖章"语义 |
| K-138 | 临时挂载点：`COMMON_MIB_INFO` 取向服务要 name/desc | 机制 | 存量 | 12 §2 | `tree.c:1684-1780`、`remote.c:316-365` | 理解"缺什么问对方要" |
| K-139 | 挂载点入链头 + 失败回滚 | 机制 | 存量 | 12 §2 | `remote.c:189-191`、`:181-186` | 知道失败不会留半成品 |
| K-140 | 无主动死亡检测（DS 无订阅 API）→ 死锁风险 | 约束与不变量 | 存量 | 12 §1（部分） | `remote.c:5-21` TODO | 知道一条已知未解的设计债 |
| K-141 | Rust：`remote.rs` 的 `EndptSlot` / `SlotVerdict` + `tree/mount.rs` 的 `check_head` / `check_target` | 架构演进 | 存量 | 12 §4 | `remote.rs`(303)、`tree/mount.rs`(242) | 知道判定落点 |

### 2.15 域 O —— 远程子树：转发与死亡恢复（存量：旧 12 下半）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-142 | `COMMON_MIB_CALL` 消息字段（三 grant + 三 len + user_endpt + flags + 双版本号） | 接口与协议 | 存量 | 12 §2 | `remote.c:422-436` | 会组一条转发消息 |
| K-143 | 三 grant 创建顺序 name→oldp→newp，撤销逆序（后授先撤） | 约束与不变量 | 存量 | 12 §2、06 §2 | `remote.c:396-446` | 理解顺序不可换 |
| K-144 | grant 失败恒 EINVAL（`!GRANT_VALID`） | 约束与不变量 | 存量 | 12 §2 | `remote.c:398-399`、`:407-413` | 同 K-072 |
| K-145 | 名字永不在消息内嵌（不为性能牺牲统一） | 约束与不变量 | 存量 | 12 §2 | `remote.c:415-421` 注释 | 理解"方便对端 > 省一次拷贝" |
| K-146 | IPC 失败 → `mib_down(eid)` 清全部挂载点 + ERESTART | 机制 | 存量 | 12 §2、10 §2 | `remote.c:455-459` | 理解死亡检测的实际手段 |
| K-147 | 服务返回 ERESTART → `mib_do_deregister`（交叉场景） | 机制 | 存量 | 12 §2 | `remote.c:473-474` | 理解" deregister 信还在路上" |
| K-148 | 分发续走三去向（成功 / ERESTART+能续走 / ERESTART+不能续走→ENOENT） | 机制 | 存量 | 10 §2、12 §1.2 | `tree.c:1408-1416` | 能判续走结果 |
| K-149 | `mib_unmount` 恢复被覆盖节点的 csize/clen | 机制 | 存量 | 12 §2 | `tree.c:1790-1842` | 理解"摘下来要还原" |
| K-150 | `COMMON_MIB_INFO` 的双 grant 与逆序撤销 | 机制 | 存量 | 12 §2 | `remote.c:330-354` | 同上 |

### 2.16 域 P —— RMIB 客户端库（存量：旧 22）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-151 | rmib 节点/树与**稀疏节点**（Minix 对 NetBSD 的扩展） | 数据结构 | 存量 | 22 §1.3/§2 | `minix/lib/libsys/rmib.c`(1089) | 理解客户端侧的数据模型 |
| K-152 | `rmib_register` / `rmib_deregister` / `rmib_reregister` | 接口与协议 | 存量 | 22 §2 | `rmib.h`(188) | 会用挂载库 |
| K-153 | `asynsend3` 单向注册（与 K-129 对偶） | 接口与协议 | 存量 | 22 §2 | `rmib.c` | 理解单向约束的客户端形态 |
| K-154 | `rmib_process` 请求处理主循环 | 机制 | 存量 | 22 §2 | `rmib.c` | 知道服务端怎么应转发 |
| K-155 | `rmib_call` 与 `sys_safecopyto` / `sys_vsafecopy` | 机制 | 存量 | 22 §2 | `rmib.c` | 理解 grant 消费侧 |
| K-156 | `COMMON_MIB_{INFO,CALL,REPLY}` 三类请求的处理面 | 接口与协议 | 存量 | 22 §2 | `rmib.c` | 能对上服务端三种调用 |
| K-157 | 三个真实消费者：IPC(`kern.ipc`)、LWIP(`net.inet`)、UDS(`net.local`) | 概念 | 存量 | 22 §1/§2、12 §1 | `servers/ipc/main.c`、`net/lwip/mibtree.c`、`net/uds/stat.c` | 知道这套协议谁在用 |
| K-158 | Rust：`minix-sys/src/rmib.rs`(1540) 的簿记与协议半 | 架构演进 | 存量 | 22 §4（现状已变化） | `os/libs/minix-sys/src/rmib.rs` | 知道 Rust 侧进度 |
| K-159 | `MINIX_LWIP` 不在静态表里（运行时挂载） | 概念 | 存量 | 15 §2、22 §1 | `minix.c:83` 注释 | 理解"空槽也是设计"的第二个例子 |

### 2.17 域 Q —— 分发与名字解析（存量：旧 10）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-160 | 逐层消耗名字（`call_name++` / `call_namelen--`） | 机制 | 存量 | 10 §2 | `tree.c:1346-1349` | 能对一条名字走一遍循环 |
| K-161 | 元标识符必须末位，否则 EINVAL | 约束与不变量 | 存量 | 10 §2 | `tree.c:1360-1366` | 知道元 op 的位置约束 |
| K-162 | 四类元 op 的 switch + CREATESYM/MMAP → EOPNOTSUPP | 机制 | 存量 | 10 §2、02 §2 | `tree.c:1368-1381` | 能判元 op 结果 |
| K-163 | `is_leaf` / `has_func` / `has_verify` 的两种判定方式 | 约束与不变量 | 存量 | 10 §2、03 §2 | `tree.c:1425-1431` | 理解"省字节的代价" |
| K-164 | ENOTDIR（叶子还有剩余名字）与 EISDIR（名字用完落在节点数组） | 机制 | 存量 | 10 §2 | `tree.c:1437-1438`、`:1473` | 能区分这两个错误码 |
| K-165 | 写门（READWRITE 必须 + ANYWRITE 或特权；ANYWRITE 不覆盖 READWRITE） | 约束与不变量 | 存量 | 10 §2、07 §2 | `tree.c:1446-1457` | 能判写权限 |
| K-166 | ERESTART 续走判定（先存 `can_restart`） | 机制 | 存量 | 10 §2、12 §2 | `tree.c:1404-1416` | 理解"先算再调" |
| K-167 | Rust：`tree/dispatch.rs` 的 `judge_level` + `LevelFacts` 九参聚合 | 架构演进 | 存量 | 10 §4（P1-2 补记） | `tree/dispatch.rs`(386) | 学会用结构消灭"相邻 bool 可互换" |
| K-168 | Rust：`SysctlOutcome` 枚举（`Done` / `ErrWithResLen`） | 架构演进 | 新增 | —（Rust 已改，10 篇仅补记一句） | `dispatch.rs::SysctlOutcome` | 理解"错误 + 附带长度"的类型化 |
| K-169 | sysctl 次主线完整路径图 | 概念 | 存量 | 10 §1.3/§2、00 §3 | `tree.c:1332-1474` + `main.c:277-383` | 能把整条路一次讲完 |

### 2.18 域 R —— 服务器装配与执行 walker（**新增域**）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-170 | `MibServer`（tree / budget / endpts / roots 链头 / tables 闩锁） | 数据结构 | 新增 | — | `server.rs`(876) | 知道服务器持有什么状态 |
| K-171 | `run_once`：triage → 三信臂 → `should_reply` | 机制 | 新增 | 01 §4（仅 verdict） | `server.rs::run_once` | 能画出执行版主循环 |
| K-172 | 长名 fetch（`kernel.datacopy_from`） | 机制 | 新增 | 01 §2（判定半） | `server.rs` sysctl 臂 | 知道 K-012 的执行半 |
| K-173 | `walker::sysctl` 执行循环（find → can_see → judge_level → 动作） | 机制 | 新增 | 10 §4（一句补记） | `walker.rs`(1125) | 知道判定怎么被执行 |
| K-174 | func registry（handler 注册表） | 机制 | 新增 | — | `walker.rs` | 理解 handler 怎么被找到 |
| K-175 | meta 四 op 的执行 | 机制 | 新增 | — | `walker.rs` | 知道 QUERY/CREATE/DESTROY/DESCRIBE 的落点 |
| K-176 | Readwrite 执行（stage 两半 + verify + bool 消毒 + buf 钳制） | 机制 | 新增 | 09 §4（判定半） | `walker.rs` | 知道 K-105 的执行半 |
| K-177 | RemoteCall 执行臂（三 grant 开撤 + 三去向 + `mib_down` 等价去章） | 机制 | 新增 | 12 §4（判定半） | `walker.rs` remote 臂 | 知道 K-142~149 的 Rust 执行体 |
| K-178 | `main.rs` 组装（SysTransport + SysIpc + SysServices + `Server::run`） | 工具与工程 | 新增 | — | `os/servers/mib/src/main.rs`(34) | 知道二进制入口 |
| K-179 | 连续 receive 失败上限 64（DS 纪律） | 约束与不变量 | 新增 | — | `server.rs:64` | 同 K-035 |
| K-180 | `MibServer` 不需要"创建后永不移动"约束（对比 DS） | 架构演进 | 新增 | — | `todo.md §4 L0` | 知道先例的适用边界 |

### 2.19 域 S —— CTL_KERN 子树（存量：旧 13）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-181 | KERN 表 84 槽 = 45 填 + 39 排除 | 数据结构 | 存量 | 13 §2 | `kern.c:332-498` | 知道这棵树有多大、多空 |
| K-182 | 两类 verify 节点（securelvl / forkfsleep） | 机制 | 存量 | 13 §2 | `kern.c:17-33`、`:208-220` | 理解 verify 的真实用例 |
| K-183 | 十个现场计算的函数节点（clockrate / ccpu / cp_time / consdev / drivers / boottime / root_device …） | 机制 | 存量 | 13 §2 | `kern.c:35-315` | 学会写函数节点 |
| K-184 | `kern.ipc` 占位子树（mock，等 IPC 服务覆盖） | 概念 | 存量 | 13 §1/§2、12 §1 | `kern.c:319-330` | 理解"占位等覆盖"的模式 |
| K-185 | A-9 排除契约（40+ "not yet supported" 槽位保留 + 访问回报） | 约束与不变量 | 存量 | 13 §2、14 §2 | `kern.c:385/386/409/…` 注释 | 知道空槽不是遗忘 |
| K-186 | `KERN_PROF` → EOPNOTSUPP（Minix3 有独立 profiling API） | 约束与不变量 | 存量 | 13 §2 | `kern.c:63-74`、`:364` | 理解"故意不支持" |
| K-187 | 跨服务取值原语（`sys_getcputicks` / `getsysinfo` / `svrctl(PMGETPARAM)` / `cpuavg_get*`） | 接口与协议 | 存量 | 13 §1.2/§2 | `kern.c` 各函数体 | 知道数据的真正来源 |
| K-188 | Rust：`subtree/kern.rs` 的 45+39 逐槽 parity + 值表 | 架构演进 | 存量 | 13 §4 | `os/servers/mib/src/subtree/kern.rs`(657) | 知道排除清单已被类型化 |

### 2.20 域 T —— CTL_VM 与 CTL_HW 子树（存量：旧 14）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-189 | CTL_VM 13 槽填 4 | 数据结构 | 存量 | 14 §2 | `vm.c:120-144` | 知道这棵树有多空 |
| K-190 | loadavg 数学（`_LOAD_HISTORY` 环形、欠填补偿、FSCALE 定点） | 机制 | 存量 | 14 §1.3/§2 | `vm.c:20-80` | 会手算一分钟负载 |
| K-191 | `uvmexp2` 与借用 `unused1` 暴露最大连续物理内存 | 机制 | 存量 | 14 §2 | `vm.c:82-118` | 理解"借 NetBSD 的未用字段"的风险 |
| K-192 | CTL_HW 16 槽填 6 | 数据结构 | 存量 | 14 §2 | `hw.c:98-130` | 同上 |
| K-193 | physmem / usermem 的 32/64 双宽与 `UINT_MAX` 饱和 | 约束与不变量 | 存量 | 14 §2 | `hw.c:18-56` | 理解窄宽截断语义 |
| K-194 | pageshift 循环求对数 | 机制 | 存量 | 14 §2 | `vm.c:100-115` | 会算页偏移位数 |
| K-195 | machine / arch 字符串按架构分支（i386 / evbarm，未知架构 #error） | 约束与不变量 | 存量 | 14 §2 | `hw.c:5-14` | 知道架构分支点 |
| K-196 | `vm_info_stats` / `vm_info_usage` 依赖 | 接口与协议 | 存量 | 14 §1.2/§2 | `hw.c`、`vm.c` | 知道数据来自 VM |

### 2.21 域 U —— CTL_MINIX 子树（存量：旧 15）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-197 | `MINIX_TEST_SUBTREE` 门控与"关了 test87 就失败" | 约束与不变量 | 存量 | 15 §2 | `mib.h:17-23` | 理解编译开关的行为后果 |
| K-198 | test 表 12 个节点字面量级精确（对齐是被测对象） | 测试性质 | 存量 | 15 §2 | `minix.c:9-51` | 理解"描述串长度也是契约" |
| K-199 | 私有子树 secret（PRIVATE 父 + PRIVATE 子） | 约束与不变量 | 存量 | 15 §2、07 §2 | `minix.c:9-13`、`:44-47` | 理解两层私有 |
| K-200 | `minix.mib.*` 三个统计节点（nodes / objects / remotes） | 机制 | 存量 | 15 §2、03 §2 | `minix.c:53-63` | 知道自报家门的三个数 |
| K-201 | `minix.proc.*` 两个函数节点（list / data） | 机制 | 存量 | 15 §2、20 §1 | `minix.c:65-72` | 知道 ProcFS 入口在这 |
| K-202 | Rust：`subtree/minix.rs` | 架构演进 | 存量 | 15 §4 | `subtree/minix.rs`(396) | 知道落点 |

### 2.22 域 V —— 进程信息（存量：旧 16~20）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-203 | 三表快照 `proc_tab` / `mproc_tab` / `fproc_tab` | 数据结构 | 存量 | 16 §2 | `proc.c:34-38` | 知道数据从三家来 |
| K-204 | `update_tables` 每 tick 至多一次 + **失败闩锁**（不再重试） | 机制 | 存量 | 16 §2 | `proc.c:47-143` | 理解"一次失败就永久放弃"的设计 |
| K-205 | 双魔数校验 `PMAGIC` / `MP_MAGIC` | 约束与不变量 | 存量 | 16 §2 | `proc.c`（表扫描处） | 知道快照有效性怎么判 |
| K-206 | PID 哈希表（`HASH_SLOTS`） | 数据结构 | 存量 | 16 §2/§4 | `proc.c:33` | 理解加速查找 |
| K-207 | `get_mslot`（PID → mproc 槽） | 机制 | 存量 | 16 §2 | `proc.c:144-185` | 会对 PID 定位 |
| K-208 | `ticks_to_timeval` / `fill_wmesg` | 机制 | 存量 | 16 §2 | `proc.c:186-224` | 知道两个小换算 |
| K-209 | `get_lwp_stat` 状态机（ZOMB / DEAD / STOP / RUN / SLEEP） | 机制 | 存量 | 17 §2 | `proc.c:225-398` | 会判一个进程的状态 |
| K-210 | wchan 六类编码 | 机制 | 存量 | 17 §2 | `proc.c:225-398` | 知道"睡在哪"怎么表达 |
| K-211 | `fill_lwp_common` / `fill_lwp_kern` / `fill_lwp_user` 三段分工 | 机制 | 存量 | 17 §2 | `proc.c:399-509` | 理解内核侧/PM 侧分别填什么 |
| K-212 | `mib_kern_lwp` 的 pid / elsz / elmax 语义 | 接口与协议 | 存量 | 17 §2 | `proc.c:510-595` | 知道 -1 = 全部 |
| K-213 | `kinfo_lwp` 128 字节布局 | 数据结构 | 存量 | 17 §1.2/§3（P1-3 补记） | `types/sysctl_abi.rs:154` | 知道输出字节怎么排 |
| K-214 | `fill_proc2_common` / `fill_proc2_kern` / `fill_proc2_user` | 机制 | 存量 | 18 §2 | `proc.c:601-791` | 同上三段分工 |
| K-215 | KERN_PROC2 过滤面（ALL / PID / SESSION / PGRP / TTY / UID / RUID / GID / RGID） | 接口与协议 | 存量 | 18 §2 | `proc.c:792-917` | 能判一条 PROC2 查询 |
| K-216 | `kinfo_proc2` 680 字节 + growth-only 尾部契约 | 数据结构 | 存量 | 18 §3（P1-3 补记） | `types/sysctl_abi.rs:225` | 知道为什么尾部可以加字段 |
| K-217 | job control / TTY_REVOKE 的 TODO 现状 | 约束与不变量 | 存量 | 18 §2 | `proc.c:826-844` | 知道哪里是已知未做 |
| K-218 | `ps_strings` 读取（LP64 下 32 字节） | 数据结构 | 存量 | 19 §2 | `types/ps_strings.rs:22`；C `sys/sys/exec.h:111-116` | 知道 argv/envp 在哪 |
| K-219 | 页游走 + `copybudget` 上限 + 截断语义 | 机制 | 存量 | 19 §1.3/§2 | `proc.c:919-1176` | 理解"读别人内存要有预算" |
| K-220 | `mib_minix_proc_list`（整表） | 机制 | 存量 | 20 §2 | `proc.c:1178-1216` | 知道整表怎么填 |
| K-221 | `mib_minix_proc_data`（单 PID；**负 PID = 内核任务**，与 LWP 的 -1 不同） | 机制 | 存量 | 20 §1.3/§2 | `proc.c:1217-1288` | 记住这条易混语义 |
| K-222 | ProcFS 是这两个节点的消费者（布局 ABI，A-5） | 接口与协议 | 存量 | 20 §1/§2 | `fs/procfs/tree.c:87-92`、`pid.c:42-48/156/183` | 知道改布局会砸到谁 |
| K-223 | Rust：`proc/tables.rs` 拉取纪律判定 + 执行 `Tables::update` | 架构演进 | 存量 | 16 §4 + P1-5 补记 | `proc/tables.rs`(549) | 知道拉取的执行半已落地 |

### 2.23 域 W —— 验证、构建与外部依赖（**新增域**，含存量碎片）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-224 | test87（3662 行，root 运行）是 MIB 的主行为契约 | 测试性质 | 存量（碎片） | 15 §5、21 §5、plan §3.5 | `minix/tests/test87.c` | 知道"什么算对"由谁定义 |
| K-225 | rmibtest（267 行 + `testrmib.sh`）的 8 个远端拒绝场景 | 测试性质 | 存量（碎片） | todo §4 L4（一句） | `minix/tests/rmibtest/rmibtest.c` | 知道挂载协议的行为边界 |
| K-226 | `t_sysctl.c`（74）+ `t_sysctl.sh`（45）基础面 | 测试性质 | 存量（碎片） | plan §5.4 | `tests/kernel/t_sysctl.c`、`tests/sbin/sysctl/t_sysctl.sh` | 知道最小冒烟面 |
| K-227 | Rust 测试地图（152 个 `#[test]`，按模块分布） | 测试性质 | 新增 | 各篇 §5（按篇分散） | `os/servers/mib/src/**` | 知道 Rust 侧覆盖在哪 |
| K-228 | Gate E 测试名对账纪律（文档声称的测试必须逐名命中） | 工具与工程 | 新增 | todo §1.1 | `todo.md §1.1` | 知道文档与测试的对账方式 |
| K-229 | `servers/mib/Makefile`：8 源 + libsys + `minix.service.mk` | 工具与工程 | 新增 | —（plan §5.5 判 WONTFIX） | `minix3/minix/servers/mib/Makefile`(21) | 知道服务怎么被编出来 |
| K-230 | 跨服务依赖链与 edge 条目（E-RMIBWIRE / E-MIBPROD / E-MIBGRANT / E-DSWIRE / E-ISWIRE / E5(g)） | 接口与协议 | 存量 | 99 §5、todo §5 | `../edge_todo.md` | 知道本 stage 的对外债务在哪 |
| K-231 | A-4 尾随结构已钉：`Clockinfo` / `Timeval` / `PsStrings` / `KinfoStruct` | 架构演进 | 新增 | —（Rust 有，文档仍写"待钉"） | `types/sysctl_abi.rs:428/446`、`types/ps_strings.rs:22`、`types/kinfo.rs:20` | 修正文档里过期的"待钉"结论 |
| K-232 | `SysctlNode` 96 字节跨数据模型稳定（`__sysc_pad` 的设计意图） | 约束与不变量 | 新增 | 02 §3（改判行） | `sysctl.h:1367-1373`、`types/sysctl_abi.rs:101` | 理解"为什么敢用 repr(C)" |

### 2.24 统计摘要

| 维度 | 数 |
|---|---|
| 知识点总数 | 232 |
| 存量 / 新增 | 存量 199 / 新增 33（新增集中在：竞技场与内存 9、传输接缝 8、装配与 walker 11、验证与工程 5） |
| 按类型 | 机制 78、数据结构 40、约束与不变量 47、架构演进 34、接口与协议 22、概念 12、工具与工程 4、测试性质 5 |
| 按现有文档分布（存量项主讲述点，前六） | 12 篇 17、10 篇 13、08 篇 11、03/06 篇 10、16/22 篇 9 |

---

## 3. 覆盖审计

### 3.1 主题全集的四路来源（已逐路核对）

1. **C 源码符号**：`servers/mib/` 8 个 .c（5401 行含 mib.h）+ 协议头 + 客户端库
   —— 以 `plan.md §5.3` 的 82 函数 / 13 静态表 / 3 计数为准，本次用
   `grep -n "^mib_\|^static.*mib_"` 逐文件复核行号（`tree.c` 22 函数、`proc.c` 16 函数、
   `remote.c` 9 函数全部命中）。
2. **OS 通用概念**：名字空间树、元操作枚举、版本乐观锁、可见性与写权限分离、
   跨服务请求转发、挂载/卸载、字节预算、句柄竞技场。
3. **非 C 制品**：`Makefile`、`kernel/table.c`、test87 / rmibtest / t_sysctl(.sh)、
   libc sysctl 五文件、rmib.h。
4. **阶段边界契约**：`../00-master-plan/README.md`（MIB 为 boot_image 登记第 8 项）、
   `../edge_todo.md` 六条、`../09-stage-init/12-init-sysctl-interaction.md`。

### 3.2 覆盖缺口表

| # | 缺口 | 证据 | 建议 |
|---|---|---|---|
| G-1 | **Rust 执行半整层无文档**：`server.rs`(876) + `walker.rs`(1125) + `transport.rs`(469) + `heap.rs`(190) = 2660 行，24 篇里只有 10 篇一句"P1-2 补记"、06/07 各一句"P1-4 补记" | §0.4 的 grep 计数 | **新建两篇**：`13 传输接缝`、`26 服务器装配与执行 walker`；并把 arena/heap 归入新建的 `10 树竞技场与内存预算` |
| G-2 | **树竞技场与内存预算无主篇**：`arena.rs`(611) + `heap.rs`(190)，现有文档只有 04/08/13/15 四处互相矛盾的"arena 缺口/承诺"指针（todo P3-1 已统一到"15 §4.4 为单一真值"，但那仍是"缺口声明"而非正文） | `plan.md:219-220`（A-2/A-3）、`todo.md §P2-3/P2-4` | **新建** `10 树竞技场与内存预算`（K-094~K-102） |
| G-3 | **C 行为契约（test87 3662 + rmibtest 267 + t_sysctl 74 + .sh 45 = 4048 行）无归属**：只在 15 §5、21 §5、plan §5.4 各露一行；rmibtest 的 8 个远端拒绝场景在 Rust 侧零对应（todo L4 判归 E5(g)，但那是联调面不是知识归属） | `wc -l` 实测 + grep | **新建** `27 行为契约与验证` |
| G-4 | **构建与部署无归属**：`Makefile`(21) 在 plan §5.5 被直接判 WONTFIX；boot 登记散在 00/99/plan；libsys 依赖与跨服务依赖链散在 99 §5 | `plan.md:351`、`99 §5` | **新建** `28 构建、部署与外部依赖` |
| G-5 | **A-4 尾随结构文档结论过期**：`Clockinfo` / `Timeval` / `PsStrings` / `KinfoStruct` 已在 `minix-types` 落 `#[repr(C)]` + `offset_of!` 断言，但 plan A-4 行仍写"随各自 handler 首消费时钉" | `types/sysctl_abi.rs:428/446`、`ps_strings.rs:22`、`kinfo.rs:20` | 并入 `02` §交换格式现状表 + `18/19/24` 各自的"布局锚点"小节（K-231、K-232） |
| G-6 | **用户态入口（sysctl(3)）位置错误**：旧 21 在倒数第二篇，但 K-005 的 ENOMEM 事务在旧 01 就要用，K-001/K-002 是 00 §3 旅程图的前提 | `00-mib-overview.md:49-67` | **上移为 `01 用户态入口`**（K-001~K-008） |
| G-7 | **远程子树一篇承载两个文件四件事**：`remote.c`(477) 的注册/注销 + `tree.c:1544-1842`(299) 的挂载/卸载 + `remote.c:378-477` 的转发/死亡 | `12-mib-remote-subtrees.md` 头部 | **拆两篇** `14 注册与挂载` / `15 转发与死亡恢复` |
| G-8 | **旧 12 且兼挂"客户端库"**：`rmib.c`(1089) + Rust `rmib.rs`(1540) 合计 2629 行压在一篇 140 行的文档里 | `22-mib-rmib-client.md`(140) | 独立为 `16 RMIB 客户端库`，并上移到远程子树之后（对端相邻） |
| G-9 | **`MINIX_TEST_SUBTREE` 与 test87 的耦合没有一处讲清**：开关关掉 test87 就失败，这条因果只在 `mib.h:17-23` 注释里 | `mib.h:17-23` | 并入 `20 CTL_MINIX` §1 + `27 验证` 交叉引用 |
| G-10 | **六处"执行半待建"类声明已过期**（arena / transport / IPC 壳 / walker 四处 follow-up 标记） | `todo.md §P1-1~P1-5` 全部 ✅ | 归入 `26 装配与 walker` 的"现状纪律"小节；B 相写作时以 Rust 现状为准，不得照抄旧声明 |

### 3.3 重复主题表

| # | 重复主题 | 现有位置 | 处置 |
|---|---|---|---|
| D-1 | ENOMEM 的特殊语义（部分拷贝 + 完整长度） | 00 §3、01 §3、02 §2、21 §1.3、99 §4 | 主讲述点 = **新 01**（用户视角，最先用到）；01(旧)/02/99 改为引用 |
| D-2 | `CTLFLAG_PRIVATE` 可见性过滤 | 07 §2、10 §2、11 §2、12 §1 | 主讲述点 = **新 08 权限模型**；其余改引用 |
| D-3 | 分配失败不返回 ENOMEM | 06 §2、08 §2、12 §2、todo P2-4 | 主讲述点 = **新 07 拷贝与 relay**；其余改引用 |
| D-4 | 版本乐观锁（`node_ver`） | 03 §2、08 §2、11 §2、tree.c 多处 | 主讲述点 = **新 04 节点模型**；08/11 只讲各自门 |
| D-5 | 四类节点矩阵 | 03 §2、10 §2、12 §1 | 主讲述点 = **新 04** |
| D-6 | `mib_mount` 的参数门 | 12 §2（与 remote.c 混讲） | 拆出后归 **新 14**，remote.c 侧只讲端点表与生命周期 |
| D-7 | 重启丢动态状态 | 01 §2、04 §2、08 §2、sef.rs | 主讲述点 = **新 03 服务的一生**；其余改引用 |
| D-8 | boot 登记位与特权必要性 | 00 §2、99 §1、plan §1.1 | 主讲述点 = **新 03**；99 只留一行表 |

### 3.4 越界主题表

| # | 越界 | 现有位置 | 正确归属 |
|---|---|---|---|
| O-1 | 旧 01 的 §2 顺带讲了 `mib_inrange`/`mib_copyout` 的调用效果（`:90-258` 整段） | 01 §2 | 归 **新 07**；旧 01 只保留"解码门用到了哪些原语"的接口面 |
| O-2 | 旧 12 的 §2 顺带展开 `cpf_grant_magic` / `cpf_revoke` 的 grant 语义 | 12 §2 | grant 语义归 **新 07**（relay 原语）；12 拆出的两篇只讲"用哪三个 grant、什么序" |
| O-3 | 旧 13 §1.2 讲了"向别的服务要数据怎么执行"的排除声明，实际那是 seam 的事 | 13 §1.2 | 归 **新 13 传输接缝**；13(旧) → 新 18 只讲"拿到数据后怎么算" |
| O-4 | 旧 04 讲了 `mib_tree_init` 又讲了 arena 承诺 | 04 §4 | `tree_init` 留 **新 05**；arena 承诺整段删除，归 **新 10** |
| O-5 | 旧 22 讲了 `COMMON_MIB_*` 的服务端语义 | 22 §2 | 服务端语义归 **新 15**；22 → 新 16 只讲客户端侧的收发与树 |
| O-6 | 旧 99 §1 讲了"kernel grant.rs 曾把端点写错"的偏差史 | 99 §1 注 | 属跨 stage 事故记录，归 `../edge_todo.md` E-MIBGRANT；99 只留"端点单一真值在 com.h"一行 |

### 3.5 非 C 主题逐项回答（固定清单，逐项给落点或排除理由）

| 主题 | 本 stage 讲不讲 | 落点 | 依据 |
|---|---|---|---|
| 链接与加载 | 讲（薄） | **新 28** §构建 | `servers/mib/Makefile:9-11`（8 源）、`:15-17`（DPADD/LDADD `-lsys`） |
| 镜像与内存布局 | 讲（薄） | **新 28** §boot 登记 + **新 03** §1 | `kernel/table.c:52-63`；`kernel/main.c:263-267` |
| 汇编入口与陷阱进入 | **不讲** | 归 01-stage-kernel / 14-stage-runtime | 用户态服务无汇编入口；`main()` 是 C 入口 |
| 启动装配 | 讲 | **新 03** §2（SEF 双回调） | `main.c:415-428` |
| 构建与工具链 | 讲 | **新 28** | `Makefile`(21) + `minix.service.mk` |
| 跨模块接口与线格式 | 重点讲 | **新 02**（消息 + 交换格式全表）+ 新 11/12（`sysctlnode`/`sysctldesc` 序列化） | `ipc.h` 六载荷、`sysctl.h:1382-1450` |
| 错误路径 | 讲（分散 + 汇总） | 各篇自己的门 + **新 99** §错误码汇总 | `main.c` 各 return |
| 关闭与退出 | **不讲**（MIB 无退出路径） | 排除 | `main.c:443` `for(;;)` + `:490` NOTREACHED |
| 并发与同步 | 讲（薄，单线程 + 阻塞 sendrec） | **新 13** §阻塞语义 + **新 99** §执行模型 | `remote.c:439`；`lib.rs` 模块注释"Single-threaded event loop" |
| 测试基建 | 讲 | **新 27** | test87 / rmibtest / t_sysctl(.sh) / Rust 152 tests |

---

## 4. 新目录

### 4.1 新篇章总表（30 篇）

| 编号 | 标题 | 一句话定位 | 分组 |
|---|---|---|---|
| 00 | `00-mib-overview.md` | 全目录导航 + "路由器而非仓库"的定位 | P0 入口 |
| 01 | `01-mib-user-entry.md` | 提问方视角：sysctl(2)/sysctl(3) 怎么用、CTL_USER 在哪、ENOMEM 为什么是信号 | P1 提问方 |
| 02 | `02-mib-protocol.md` | 信封与线格式：三信、六种消息、交换格式、版本与标志宏 | P1 提问方 |
| 03 | `03-mib-service-lifecycle.md` | MIB 的一生：boot 登记 → SEF 双回调 → mib_init → 主循环三信 → 回信规则 → 六道解码门 | P2 服务骨架 |
| 04 | `04-mib-node-model.md` | 一个结点长什么样：mib_node 三 union、四类矩阵、位域、计数、scratch | P3 树的形状 |
| 05 | `05-mib-static-tree.md` | 静态树怎么建：七个顶层、MIB_* 宏、四行 wiring、tree_init 递归 | P3 树的形状 |
| 06 | `06-mib-lookup.md` | 怎么找：静态 O(1) + 动态有序链 O(n) + prevpp | P3 树的形状 |
| 07 | `07-mib-copy-relay.md` | 字节怎么动：oldp/newp 不透明体、九个原语、两种传输、grant | P4 原语 |
| 08 | `08-mib-auth.md` | 谁许可：mib_authed 缓存、PRIVATE / READWRITE / ANYWRITE / PERMANENT | P4 原语 |
| 09 | `09-mib-dynamic-nodes.md` | 树怎么长：check_name / scan / create / add / remove / destroy / upgrade | P5 生长 |
| 10 | `10-mib-arena-budget.md` | 长出来的东西存在哪：NodeId 竞技场、ChildMap、静态窗口、256 KiB 预算 | P5 生长 |
| 11 | `11-mib-data-access.md` | 叶子怎么读写：四条 lane、stage、verify、bool 消毒、大数据写 EPERM | P6 数据与元操作 |
| 12 | `12-mib-query-describe.md` | 怎么看目录：QUERY 枚举、DESCRIBE 描述、两次序列化与标志剥离 | P6 数据与元操作 |
| 13 | `13-mib-transport-seams.md` | 对外动词怎么接：MibKernel / MibServices / MibIpc 三 seam + 诚实桩 | P7 接缝与远程 |
| 14 | `14-mib-remote-mount.md` | 别人的子树怎么挂上来：endpts 表、注册/注销、五道门、覆盖与临时挂载 | P7 接缝与远程 |
| 15 | `15-mib-remote-relay.md` | 命中挂载点之后怎么办：COMMON_MIB_CALL、三 grant、ERESTART、死亡清场 | P7 接缝与远程 |
| 16 | `16-mib-rmib-client.md` | 挂载方那一侧：rmib 库、稀疏节点、注册/注销/应答，以及三个真实消费者 | P7 接缝与远程 |
| 17 | `17-mib-dispatch.md` | 名字怎么一层一层走完：逐层解析、元标识符、三类终点、五个错误码 | P8 分发 |
| 18 | `18-mib-subtree-kern.md` | CTL_KERN：45 填 + 39 排除、函数节点族、verify 节点、ipc 占位子树 | P9 数据源 |
| 19 | `19-mib-subtree-vm-hw.md` | CTL_VM + CTL_HW：loadavg 数学、uvmexp2、双宽 physmem、pageshift | P9 数据源 |
| 20 | `20-mib-subtree-minix.md` | CTL_MINIX：test 测试子树、mib 自报家门、proc 两入口、LWIP 缺席 | P9 数据源 |
| 21 | `21-mib-proc-tables.md` | 进程数据从哪来：三表快照、tick 节流、失败闩锁、魔数、PID 哈希 | P10 进程信息 |
| 22 | `22-mib-proc-lwp.md` | KERN_LWP：状态机、wchan 六类、三段填充、kinfo_lwp 布局 | P10 进程信息 |
| 23 | `23-mib-proc2.md` | KERN_PROC2：九种过滤、三段填充、kinfo_proc2 growth-only 尾部 | P10 进程信息 |
| 24 | `24-mib-proc-args.md` | KERN_PROC_ARGS：ps_strings、页游走、copybudget、截断 | P10 进程信息 |
| 25 | `25-mib-minix-proc.md` | MINIX_PROC：整表与单进程、负 PID = 内核任务、ProcFS 消费契约 | P10 进程信息 |
| 26 | `26-mib-assembly-walker.md` | 把全部判定接成一台服务器：MibServer、run_once、walker 执行体、main 组装 | P11 装配 |
| 27 | `27-mib-verification.md` | 什么算对：test87 / rmibtest / t_sysctl / Rust 152 tests 的地图与对账纪律 | P12 支线 |
| 28 | `28-mib-build-deploy.md` | 怎么编出来、怎么进 boot 镜像、依赖谁 | P12 支线 |
| 99 | `99-mib-global-concepts.md` | 查询手册：常量、错误码、跨服务引用各归一位 | P13 收口 |

### 4.2 阅读路径

- **主线（必读，按顺序）**：`00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15 → 17 → 26`
  —— 这条线走完，读者能从"用户在 shell 敲一行 sysctl"一直追到"Rust 服务器把字节拷回用户缓冲区"。
- **数据源支线（可跳读，按兴趣选读）**：`18 → 19 → 20`（三棵静态子树）；`21 → 22 → 23 → 24 → 25`（进程信息，需先看 11 与 13）。
- **客户端支线**：`16`（要给服务挂子树的人才需要；依赖 14/15）。
- **工程支线**：`27`（要改代码、要加测试的人）、`28`（要构建/部署的人）。
- **随手查**：`99`。

### 4.3 并行体的组织规则落实

- **三棵静态子树（18/19/20）** 是并行体：先由 `11` 给出"数据节点 / 函数节点 / verify 节点"的统一框架，
  再按顶层 id 分三篇；其中 `18` 是代表性成员（最大、三类节点俱全），`19`/`20` 按"差异表"收束
  （19 讲两个小表的空槽分布 + 三个计算，20 讲一个开关 + 两个入口）。
- **进程信息五篇（21~25）** 是并行体：先由 `21` 给出统一框架（三表快照 + 拉取纪律 + 定位原语），
  再由 22/23/24/25 各自讲一种输出格式；`22`（LWP）是代表性成员（状态机最复杂），其余按差异表收束。
- **远程子树（14/15/16）** 是"一个机制的三个视角"：服务端注册挂载 → 服务端转发 → 客户端库。
- **若干"未实现槽位"** 不是并行体而是"排除清单"，集中在 18/19/20 各自的 §排除契约，不单独立篇。

### 4.4 序差表（教学序 ≠ 运行时序的地方）

| # | 运行时序事实（带锚点） | 教学序选择 | 理由 | 回指补偿位置 |
|---|---|---|---|---|
| X-1 | 用户态 `sysctl(3)` 在 MIB 启动之前就已存在（`__sysctl.c`） | 放在 `01`（服务骨架 03 之前） | 读者先知道"谁在问、问的形状"，才知道服务端在答什么；且 01 不依赖任何 MIB 内部机制 | 03 §1 开头一句"提问方见 01" |
| X-2 | `mib_mount` 在 `tree.c`，与 `remote.c` 的注册同属一次挂载流程 | 拆为 14（tree.c 侧）/15（remote.c 侧 + 转发） | 挂载是"树手术"，转发是"协议往返"，是两个语义单元 | 14 §1.2 声明"转发见 15"；15 §1 回指 14 |
| X-3 | Rust 的 walker（26）在代码里先于 handler 存在（是骨架） | 放在 handler（18~25）之后 | walker 消费 handler，先有 handler 才有可讲的执行 | 26 §1.3 给"registry 接口"声明，18~25 各自 §过渡指向 26 |
| X-4 | `arena`/`heap`（10）是 create（09）的前提 | 放在 09 之后 | 09 是 C 语义单元（可独立讲），10 是 Rust 的承载决策（A-2/A-3 ARCH），属"实现决策"后置 | 09 §1.2 声明"内存承载见 10" |
| X-5 | 传输接缝（13）在 C 里是散落的系统调用 | 集中成一篇放在远程（14/15）之前 | 远程 relay 的 grant 动词来自 13；13 本身不依赖远程 | 07 §1.2 声明"执行动词见 13" |

---

## 5. 每篇契约

> 契约是 B 相写正文的任务书。格式：`定位 / 讲什么 / 不讲什么 / 前置 / 后置 /
> 事实底线 / 知识点清单 / 验收标准`。

### 00-mib-overview

- **一句话定位**：让读者在五分钟内知道 MIB 是什么、为什么不拥有数据、以及 30 篇该怎么读。
- **讲什么**：K-008（外部消费者）、K-021/K-022/K-023（坐标与两个因果）、K-169（次主线图，缩略版）、新目录导航表、四条设计纪律。
- **不讲什么**：任何机制细节（01~28、99）；跨 stage 对端工作（归 `../edge_todo.md`）。
- **前置**：无。**后置**：全部。
- **事实底线**：`main.c:1-33` 文件头注释（"must not be called directly from other services, with the exception of ProcFS"）；`kernel/table.c:52-63`；`com.h:66`。
- **知识点清单**：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-008 | 外部消费者 | 概念 | `sysctl.h:1382-1450` | 定位需要 | 存量：旧 21 §1 |
  | K-021 | boot 登记位 | 概念 | `table.c:52-63` | 坐标 | 存量：旧 00 §2 |
  | K-022 | 早启动因果 | 概念 | `main.c:15-17` | 定位需要 | 存量：旧 00 §2 |
  | K-023 | 特权必要性 | 概念 | `main.c:17-19` | 定位需要 | 存量：旧 00 §1 |
  | K-169 | 次主线路径图 | 概念 | `tree.c:1332-1474` | 全图导航 | 存量：旧 10 §1.3 |
- **验收标准**：读者能回答：MIB 拥有哪些数据？（答：几乎不拥有，它是路由器）；它为什么必须是 boot_image 里的服务？；我要实现"读一个 int 节点"该看哪三篇？（答：01/02 + 11 + 26）。导航表覆盖全部 30 篇且无死链。

### 01-mib-user-entry

- **一句话定位**：从"提问的人"这一侧讲清 sysctl 怎么用，以及 ENOMEM 为什么是"再来一次"的信号而不是失败。
- **讲什么**：K-001、K-002、K-003、K-004、K-005、K-006、K-007、K-008。
- **不讲什么**：MIB 内部如何解析（02 起）；消息字段含义（02）；Rust 实现（无，libc 不重写）。
- **前置**：无（00 可选）。**后置**：02、03、27。
- **事实底线**：`lib/libc/gen/sysctl.c`(391)、`sysctlbyname.c`(67)、`sysctlnametomib.c`(72)、`sysctlgetmibinfo.c`(611)、`minix/lib/libc/sys/__sysctl.c`(40)；`sys/sys/sysctl.h:75`（CTL_MAXNAME=12）；`com.h:66`（MIB_PROC_NR=7）。
- **知识点清单**：K-001~K-008（8 条，全部来自旧 21，不新增）。
- **验收标准**：读者能写出"查 `hw.ncpu` 的完整两次调用"；能说清 `sysctl kern.ostype` 与 `sysctl user.cs_path` 走的路径有什么不同；能对 `sysctl -a` 的输出顺序为什么不是编号序给出解释（引用 `tree.c:207-212` 注释）。

### 02-mib-protocol

- **一句话定位**：把"信上写了什么"和"用户态拿到的字节长什么样"一次讲清，作为所有 handler 篇的前置。
- **讲什么**：K-009~K-019（协议面全集）+ 新增 K-231、K-232（A-4 现状订正）。
- **不讲什么**：收到信之后的三岔路（03）；拷贝与 relay 的执行（07）；标志位在节点上的**行为**（04）。
- **前置**：01。**后置**：03、04、07、11、12、14、15、16、99。
- **事实底线**：`com.h:613/616/622/1022-1030`；`ipc.h:15/431-433/1379-1382/1388/1551/1568/1579` + union 臂 `2455/2561-62/2581-83`；`sys/sys/sysctl.h:75/132-133/159-164/1382-1450`；`minix/include/minix/sysctl.h`(97)；`os/libs/minix-types/src/ipc/mib.rs`(420)、`types/sysctl.rs`(965)、`types/sysctl_abi.rs`(599)。
- **知识点清单**：K-009~K-019、K-231、K-232。
- **验收标准**：给一个 flags 字（`0x01000000 | CTLTYPE_INT | CTLFLAG_READWRITE | CTLFLAG_IMMEDIATE`），读者能手算出版���、类型、可写性、是否立即值；能列出六种消息结构各自用在哪一次往返；能说出 `sysctlnode` 为什么是 96 字节且跨数据模型稳定。

### 03-mib-service-lifecycle

- **一句话定位**：MIB 的一生：从 boot 镜像里的一行登记，到主循环的三条通道与一条回信规则，以及一次 sysctl 请求进门要过的六道门。
- **讲什么**：K-021~K-033、K-035。
- **不讲什么**：`mib_init` 的接线体（05）；拷贝与鉴权原语的实现（07/08）；树怎么解析（06/17）。
- **前置**：01、02。**后置**：05、07、17、26、28。
- **事实底线**：`kernel/table.c:52-63`、`kernel/main.c:263-267`；`main.c:415-428`（startup）、`:443-492`（main）、`:277-383`（mib_sysctl）、`:64-83`（oldp/newp 结构声明）、`:90-98/:147-152`（两个原语的**接口面**，实现归 07）。
- **知识点清单**：K-021~K-033、K-035。
- **验收标准**：读者能默画出主循环的七个步骤；能对每个 `mib_sysctl` 的返回码指出是哪一道门（namelen / name 取道 / oldp / newp / dispatch / ENOMEM 换算）；能解释"为什么 MIB 不收 notify"和"为什么野号码对 SENDREC 回 ENOSYS 而对 send 不回"。

### 04-mib-node-model

- **一句话定位**：一个结点在内存里长什么样，以及"看一眼 flags 就知道它是哪种节点"这件事怎么做到的。
- **讲什么**：K-036~K-050。
- **不讲什么**：静态表的定义与宏展开（05）；查找顺序（06）；动态节点的创建销毁（09）。
- **前置**：02。**后置**：05、06、09、10、11、12、14、17、18。
- **事实底线**：`mib.h:52-74`（标志重定义）、`:181-186`（位域）、`:188-235`（mib_node）、`:244-249`（mib_dynode）；`tree.c:12`（IS_STATIC_ID）、`:21-23`（scratch）、`:25-27`（计数器）、`:1505`（版本赋值）；`os/servers/mib/src/tree/node.rs`(284)、`tree/flag.rs`(243)。
- **知识点清单**：K-036~K-050。
- **验收标准**：给一组 flags（`CTLTYPE_NODE | CTLFLAG_PARENT | CTLFLAG_REMOTE | _RO | _P`），读者能说出这是"覆盖挂载点"并解释挂载期间 `csize/clen` 被谁占用；能画出 `mib_node` 的三个 union 各自在四类节点下装什么。

### 05-mib-static-tree

- **一句话定位**：七个顶层节点是怎么用宏铺出来的，四行 wiring 为什么必须存在，递归初始化到底填了哪些字段。
- **讲什么**：K-051~K-058、K-059（新增窗口模型）。
- **不讲什么**：子树内部的节点表（18/19/20）；端点表（14）；动态节点（09）。
- **前置**：03、04。**后置**：06、09、18、19、20。
- **事实底线**：`main.c:37-45`（顶层注释）、`:46-54`（mib_table）、`:56-62`（mib_root）、`:384-413`（mib_init）、`mib.h:252-319`（宏族 + MIB_INIT_ENODE）；`tree.c:1479-1536`；四棵子树的 `*_init`（`kern.c`/`vm.c`/`hw.c`/`minix.c` 末段）；`tree/static_tree.rs`(161)、`tree/init.rs`(187)。
- **知识点清单**：K-051~K-059。
- **验收标准**：读者能解释"为什么不能用 `sizeof` 外部数组"（`main.c:388-394` 注释）；能说出 `mib_tree_recurse` 对每个节点写了哪四个字段；能说出 Rust 侧为什么必须先 `reserve(max_id+1)` 再 `place`（K-059 的事故）。

### 06-mib-lookup

- **一句话定位**：怎么从父节点和 id 找到子节点，以及为什么静态是 O(1)、动态是 O(n) 却仍能早停。
- **讲什么**：K-060~K-064。
- **不讲什么**：名字解析主循环（17）；动态插入（09）。
- **前置**：04。**后置**：09、14、17。
- **事实底线**：`tree.c:12`（IS_STATIC_ID）、`:34-81`（mib_find）；`os/servers/mib/src/tree/lookup.rs`(139)。
- **知识点清单**：K-060~K-064。
- **验收标准**：读者能解释"为什么用户自选 id 就排除了动态数组"（`tree.c:59-67` 注释）；能说出 `prevpp` 存在的唯一用途；能判 `id = -2` 会走到哪里（答：不进 `mib_find`，在 17 的元标识符分支）。

### 07-mib-copy-relay

- **一句话定位**：字节怎么在用户、MIB、远程服务之间移动：两个不透明体、九个原语、两种传输模型。
- **讲什么**：K-065~K-076。
- **不讲什么**：解码门怎么用它们（03）；远程转发的调用面（15）；内核动词的接线（13）。
- **前置**：02。**后置**：11、12、13、14、15、19、24、26。
- **事实底线**：`main.c:64-83`（结构）、`:90-258`（九个原语）；`tree.c:372-421`（copyin_str）；`os/servers/mib/src/io/copy.rs`(407)、`io/relay.rs`(269)。
- **知识点清单**：K-065~K-076。
- **验收标准**：读者能说出"拷"和"借"的分界（sys_datacopy vs grant）；能对 `mib_copyin(newp, buf, len)` 的三种 `len` 取值说出结果；能解释 grant 创建失败为什么回 EINVAL 而不是 ENOMEM；能说出 Rust 侧 `RelayGrant::close` 为什么消费所有权。

### 08-mib-auth

- **一句话定位**：谁可以看、谁可以写：一次 PM 查询的缓存，与四个权限位的语义。
- **讲什么**：K-077~K-083。
- **不讲什么**：具体节点的写校验回调点（11/18）；拷贝（07）。
- **前置**：02。**后置**：09、11、12、17、18、19、20。
- **事实底线**：`main.c:259-275`；`mib.h:40-50`（mib_call + flags）；`tree.c:114-131`（可见性）、`:1389-1390`（PRIVATE 门）、`:1446-1457`（写门）；`os/servers/mib/src/auth.rs`(219)。
- **知识点清单**：K-077~K-083。
- **验收标准**：给一个节点 flags 与调用者身份（root / 普通用户），读者能判定"能否看到值""能否看到节点本身""能否写入"三个问题的答案；能解释 ANYWRITE 为什么不能代替 READWRITE。

### 09-mib-dynamic-nodes

- **一句话定位**：树怎么在运行时长出来又缩回去：名字检查、插入点定位、创建的校验成本排序、销毁与版本递增。
- **讲什么**：K-084~K-093。
- **不讲什么**：内存承载（10）；数据拷贝进新节点（07）；建成后拷给用户的快照（12）。
- **前置**：04、06、07、08。**后置**：10、12、17、26。
- **事实底线**：`tree.c:247-360`（check_name/scan）、`:426-481`（upgrade/add）、`:486-777`（create）、`:784-917`（remove/destroy）；`os/servers/mib/src/tree/dynamic.rs`(507)、`tree/version.rs`(82)。
- **知识点清单**：K-084~K-093。
- **验收标准**：读者能按"成本由低到高"背出 create 的校验顺序；能解释 `mib_scan` 为什么要二次越位重扫（`tree.c:345-354`）；能对"create 撞名"说出返回给用户的错误码与附带数据。

### 10-mib-arena-budget（新建）

- **一句话定位**：动态节点存在哪：NodeId 竞技场、有序 ChildMap、静态窗口的保留与落位、256 KiB 字节预算，以及 C 的"分配失败报 EINVAL"是怎么被兑现的。
- **讲什么**：K-094~K-102。
- **不讲什么**：动态节点的 C 语义与判定（09）；拷贝（07）；handler（18~25）。
- **前置**：09。**后置**：17、26。
- **事实底线**：`os/servers/mib/src/tree/arena.rs`(611)、`heap.rs`(190)、`lib.rs`（`extern crate alloc` 位置）；C 对照点 `mib.h:244-249`（dynode）、`tree.c:69-78`（有序链表）、`tree.c:589/1043/1207/1741-1744`（分配失败的四种回法）。
- **知识点清单**：K-094~K-102。
- **验收标准**：读者能说出三个被否决的方案（直译链表 / bumpalo / 裸全局分配器）各自的否决理由；能解释"预算耗尽"在 C 里对应哪几行、映射成什么错误码、临时挂载点为什么例外；能解释 `reset()` 在重启时清掉了什么。

### 11-mib-data-access

- **一句话定位**：常规数据叶子的读与写：四条 lane、先 stage 后落子、verify 回调、bool 消毒、大数据写的 EPERM。
- **讲什么**：K-103~K-111。
- **不讲什么**：函数驱动节点（18~25）；分发怎么调到这里（17）；拷贝执行（07/13）。
- **前置**：04、07、08。**后置**：17、18、19、20、26。
- **事实底线**：`tree.c:1098-1130`（getptr）、`:1131-1159`（read）、`:1160-1308`（write）、`:1309-1330`（readwrite）；`os/servers/mib/src/data/readwrite.rs`(269)。
- **知识点清单**：K-103~K-111。
- **验收标准**：读者能对 `CTLTYPE_STRING` 节点说出"读出来的是实际长度还是最大长度"；能说出 bool 消毒的输入输出；能解释"非特权用户写大数据"为什么不分配而是直接 EPERM。

### 12-mib-query-describe

- **一句话定位**：用户怎么"列目录"：QUERY 枚举的两趟遍历与版本门、DESCRIBE 的读与写、以及出口处被剥离的三个内部标志。
- **讲什么**：K-112~K-119。
- **不讲什么**：分发怎么投到这里（17）；描述的归属（09/10）；字符串拷入（07）。
- **前置**：02、04、07、08。**后置**：17、26。
- **事实底线**：`tree.c:90-174`（copyout_node）、`:179-239`（query）、`:926-966`（copyout_desc）、`:973-1091`（describe）；`os/servers/mib/src/query.rs`(148)、`describe.rs`(199)、`types/sysctl_abi.rs:134`。
- **知识点清单**：K-112~K-119。
- **验收标准**：读者能说出为什么服务端不保证编号升序（`tree.c:207-212`）；能说出函数节点为什么报一个假函数地址；能说出 `DESC_HEADER` 现在从哪派生（不再是手写常量）。

### 13-mib-transport-seams（新建）

- **一句话定位**：MIB 对外要用的动词都长什么样：三个 seam、真实端为什么是诚实桩、以及阻塞 sendrec 为什么不是缺陷。
- **讲什么**：K-120~K-127。
- **不讲什么**：拷贝原语的判定（07）；grant 在远程转发里的用法（15）；具体 handler 的数据语义（18~25）。
- **前置**：07。**后置**：14、15、19、21、26。
- **事实底线**：`os/servers/mib/src/transport.rs`(469)（`MibKernel`/`MibServices`/`SysTransport`/`Recorder`）、`server.rs::MibIpc`；C 对照点 `main.c:136-138`（datacopy）、`remote.c:330-354`（grant）、`:439`（sendrec）、`proc.c:66-106`（getticks/getproctab/getsysinfo）、`kern.c` 各跨服务调用。
- **知识点清单**：K-120~K-127。
- **验收标准**：读者能列出三个 trait 各自的动词清单；能解释"为什么 MIB 的 seam 形状不能直接照搬 SCHED"；能对嵌套 `sendrec` 说出"与 C 一致，故不设计超时"的依据；能说出真实端当前返回什么（`-EIO`）以及通电条件（edge E1/E2）。

### 14-mib-remote-mount

- **一句话定位**：一个服务怎么把它的子树挂到这棵树上：端点表、单向约束、五道参数门、覆盖挂载与临时挂载。
- **讲什么**：K-128~K-141。
- **不讲什么**：转发与死亡清场（15）；客户端怎么写注册信（16）；分发怎么命中挂载点（17）。
- **前置**：04、06、07、13。**后置**：15、16、17、26。
- **事实底线**：`remote.c:24-35`（endpts）、`:40-49`（init）、`:55-74`（down）、`:81-104`（get_label）、`:109-192`（do_register）、`:197-233`（register）、`:239-286`（do_deregister）、`:291-307`（deregister）；`tree.c:1544-1786`（mount）、`:1790-1842`（unmount）；`os/servers/mib/src/remote.rs`(303)、`tree/mount.rs`(242)。
- **知识点清单**：K-128~K-141。
- **验收标准**：给一条挂载请求（`mib` 路径 + flags + csize/clen），读者能逐层判出 EPERM / EINVAL / ENOENT / EBUSY 中的哪一个会被触发；能解释"摘链在 unmount 之前"的原因（`remote.c:272-285` 注释）；能说出禁顶层挂载是唯一安全策略及其 TODO 走向。

### 15-mib-remote-relay

- **一句话定位**：请求命中挂载点之后：三 grant 的开与撤、一次阻塞往返、以及"对方死了"怎么变成 ERESTART 并清场。
- **讲什么**：K-142~K-150。
- **不讲什么**：挂载点怎么来的（14）；续走判定本身（17）；客户端侧怎么应答（16）。
- **前置**：07、13、14。**后置**：16、17、26。
- **事实底线**：`remote.c:316-365`（remote_info）、`:378-477`（remote_call）；`tree.c:1404-1416`（续走）；`os/servers/mib/src/walker.rs` remote 臂。
- **知识点清单**：K-142~K-150。
- **验收标准**：读者能按正确顺序说出三个 grant 的创建与撤销序列；能区分"IPC 失败导致的 ERESTART"与"服务主动返回的 ERESTART"分别触发什么动作；能解释为什么名字永不在消息内嵌（`remote.c:415-421`）。

### 16-mib-rmib-client

- **一句话定位**：挂载方那一侧：rmib 库的数据模型、稀疏节点、注册/注销/应答，以及 IPC / LWIP / UDS 三个真实消费者。
- **讲什么**：K-151~K-159。
- **不讲什么**：服务端收到注册信之后怎么走（14/15）；传输执行（13）；libc 那一侧（01）。
- **前置**：02、15。**后置**：26、27。
- **事实底线**：`minix/lib/libsys/rmib.c`(1089)、`minix/include/minix/rmib.h`(188)；消费者 `servers/ipc/main.c`、`net/lwip/mibtree.c`、`net/uds/stat.c`；`os/libs/minix-sys/src/rmib.rs`(1540)。
- **知识点清单**：K-151~K-159。
- **验收标准**：读者能说出"稀疏节点"相对 NetBSD 增加了什么；能对 `COMMON_MIB_INFO` / `COMMON_MIB_CALL` / `COMMON_MIB_REPLY` 三类请求说出客户端各做什么；能说出三个消费者各自挂载了哪个名字空间。

### 17-mib-dispatch

- **一句话定位**：名字怎么一层一层被消耗掉：元标识符的四路分支、三类终点的判定、五个错误码各自由谁触发。
- **讲什么**：K-160~K-169。
- **不讲什么**：单层查找（06）；叶子读写实现（11）；元 op 的本体（09/12）；远程调用本体（15）；walker 执行体（26）。
- **前置**：06、07、08、11、12、15。**后置**：26。
- **事实底线**：`tree.c:1332-1474`（mib_dispatch 全段）；`os/servers/mib/src/tree/dispatch.rs`(386)（`judge_level` / `LevelFacts`）、`dispatch.rs::SysctlOutcome`。
- **知识点清单**：K-160~K-169。
- **验收标准**：给一条名字与一棵树，读者能走完循环并说出终止在哪一类终点、返回什么；能区分 ENOTDIR 与 EISDIR 的触发条件；能对"远程服务死亡"说出三种续走结果（成功 / 本地续走 / ENOENT）。

### 18-mib-subtree-kern

- **一句话定位**：CTL_KERN：84 个槽位里 45 个填了什么、39 个为什么空着、函数节点与 verify 节点怎么写、以及 `kern.ipc` 这个"等着被覆盖"的占位子树。
- **讲什么**：K-181~K-188。
- **不讲什么**：进程相关的三个查询（22/23/24）；跨服务取值的执行（13）；其余两棵子树（19/20）。
- **前置**：04、07、08、11。**后置**：26。
- **事实底线**：`minix3/minix/servers/mib/kern.c`(508) 全文；`os/servers/mib/src/subtree/kern.rs`(657)；`types/sysctl_abi.rs:428`（Clockinfo）。
- **知识点清单**：K-181~K-188。
- **验收标准**：读者能说出 45 个填充槽里函数节点 / verify 节点 / 数据节点各有多少；能解释 `KERN_PROF` 为什么是 EOPNOTSUPP 而不是"未实现"；能说出 `kern.ipc` 什么时候可见、什么时候被覆盖。

### 19-mib-subtree-vm-hw

- **一句话定位**：CTL_VM 与 CTL_HW：两棵小表的空槽分布，以及三个纯计算（负载均值、页偏移位数、宽窄截断）。
- **讲什么**：K-189~K-196。
- **不讲什么**：取值执行（13）；其余子树（18/20）。
- **前置**：04、07、08、11、13。**后置**：26。
- **事实底线**：`vm.c`(154) 全文、`hw.c`(140) 全文；`os/servers/mib/src/subtree/vm.rs`(258)、`hw.rs`(270)。
- **知识点清单**：K-189~K-196。
- **验收标准**：读者能手工算出"一分钟负载"的三个步骤（环形回看、欠填补偿、定点换算）；能说出 `uvmexp2` 借 `unused1` 暴露了什么、以及这个借用有什么风险；能对 `physmem` / `physmem64` 说出同一份数据在窄宽两种节点上的饱和行为。

### 20-mib-subtree-minix

- **一句话定位**：CTL_MINIX：一个编译开关撑起的测试子树、三个自报家门的计数器、两个 ProcFS 入口，以及"LWIP 不在这张表里"的设计。
- **讲什么**：K-197~K-202 + G-9（开关与 test87 的耦合）。
- **不讲什么**：进程信息的填充（21~25）；测试细节（27）。
- **前置**：04、11。**后置**：25、26、27。
- **事实底线**：`minix.c`(89) 全文；`mib.h:17-23`（MINIX_TEST_SUBTREE）；`minix/include/minix/sysctl.h`；`os/servers/mib/src/subtree/minix.rs`(396)。
- **知识点清单**：K-197~K-202。
- **验收标准**：读者能解释"关掉 `MINIX_TEST_SUBTREE` 会让 test87 失败"这条因果；能说出 test 表里哪一个节点是用来测私有子树的、哪两个是留给 destroy 测试的；能说出 `minix.mib.*` 三个数的 C 来源变量。

### 21-mib-proc-tables

- **一句话定位**：进程数据的源头：三张表怎么拉、为什么每 tick 最多一次、失败为什么永久闩锁、以及 PID 怎么定位到槽。
- **讲什么**：K-203~K-208、K-223。
- **不讲什么**：三种输出格式怎么填（22/23/24）；整表快照（25）；拷贝（07）。
- **前置**：02、13。**后置**：22、23、24、25、26。
- **事实底线**：`proc.c:33-38`（三表 + HASH_SLOTS）、`:47-143`（update_tables）、`:144-185`（get_mslot）、`:186-224`（fill_wmesg / ticks_to_timeval）；`os/servers/mib/src/proc/tables.rs`(549)。
- **知识点清单**：K-203~K-208、K-223。
- **验收标准**：读者能说出"一次拉取失败后会不会重试"（答：不会，闩锁）；能说出两个魔数各自校验哪张表；能解释为什么要自己建 PID 哈希。

### 22-mib-proc-lwp

- **一句话定位**：KERN_LWP：一个进程状态是怎么从内核与 PM 两边的比特拼出来的，wchan 六类编码，以及 128 字节输出怎么排。
- **讲什么**：K-209~K-213。
- **不讲什么**：其它三种输出（23/24/25）；表怎么来（21）。
- **前置**：21、04。**后置**：25、26。
- **事实底线**：`proc.c:225-398`（get_lwp_stat）、`:399-509`（fill_lwp_*）、`:510-595`（mib_kern_lwp）；`os/servers/mib/src/proc/lwp.rs`(692)；`types/sysctl_abi.rs:154`（KinfoLwp）。
- **知识点清单**：K-209~K-213。
- **验收标准**：给一组进程标志，读者能说出 LWP 状态（ZOMB/DEAD/STOP/RUN/SLEEP）判定的先后顺序；能说出 `elsz`/`elmax` 的语义与 -1 的含义；能说出 `kinfo_lwp` 现在钉在哪、多少字节。

### 23-mib-proc2

- **一句话定位**：KERN_PROC2：九种过滤条件怎么走，三段填充分别向谁要数据，以及 680 字节输出的 growth-only 尾部为什么可以加字段。
- **讲什么**：K-214~K-217。
- **不讲什么**：LWP（22）、ARGS（24）、整表（25）。
- **前置**：21。**后置**：26。
- **事实底线**：`proc.c:601-791`（fill_proc2_*）、`:792-917`（mib_kern_proc2）、`:826-844`（job control TODO）；`os/servers/mib/src/proc/proc2.rs`(449)；`types/sysctl_abi.rs:225`（KinfoProc2）。
- **知识点清单**：K-214~K-217。
- **验收标准**：读者能列出九种过滤；能说出 `fill_proc2_kern` 与 `fill_proc2_user` 各自的数据来源；能解释"尾部契约"允许什么、禁止什么。

### 24-mib-proc-args

- **一句话定位**：KERN_PROC_ARGS：怎么去另一个进程的地址空间里把命令行和环境变量读出来，以及预算用完时怎么截断。
- **讲什么**：K-218、K-219。
- **不讲什么**：PROC2（23）、整表（25）、拷出（07）。
- **前置**：21、07。**后置**：26。
- **事实底线**：`proc.c:919-1176`；`types/ps_strings.rs:22`（PsStrings 32B）；C `sys/sys/exec.h:111-116`；`os/servers/mib/src/proc/proc_args.rs`(242)。
- **知识点清单**：K-218、K-219。
- **验收标准**：读者能说出 `ps_strings` 在 LP64 下的字节数与四个字段偏移；能解释页游走为什么要一块一块来；能说出预算耗尽时输出是什么形状（截断而非报错）。

### 25-mib-minix-proc

- **一句话定位**：MINIX_PROC：ProcFS 要的两份数据怎么填，以及"负 PID 表示内核任务"这条与别处不一样的规矩。
- **讲什么**：K-220、K-221、K-222。
- **不讲什么**：状态怎么定（22）；PROC2/ARGS（23/24）；ProcFS 怎么把数据摆成文件（15-stage-fs）。
- **前置**：21、22。**后置**：26。
- **事实底线**：`proc.c:1178-1216`（list）、`:1217-1288`（data）；`minix/include/minix/sysctl.h`（两个结构 + 两组标志）；`fs/procfs/tree.c:87-92`、`pid.c:42-48/156/183`；`os/servers/mib/src/proc/minix_proc.rs`(192)。
- **知识点清单**：K-220、K-221、K-222。
- **验收标准**：读者能说出 `PROC_LIST` 与 `PROC_DATA` 的查法差异；能对 PID = -3 说出查的是谁；能说出改这两个结构的布局会砸到哪个服务。

### 26-mib-assembly-walker（新建）

- **一句话定位**：把前面全部判定接成一台真正会收信、会走树、会回信的服务器：MibServer 的状态、run_once 的三臂、walker 的执行循环、main 的组装。
- **讲什么**：K-170~K-180 + G-10（旧"执行半待建"声明的作废纪律）。
- **不讲什么**：各 handler 的数据语义（18~25，只消费它们的 registry 接口）；远程协议本身（15，只调它的执行臂）；传输 trait 的定义（13，只接线）。
- **前置**：17、13、15、18~25。**后置**：27。
- **事实底线**：`os/servers/mib/src/server.rs`(876)、`walker.rs`(1125)、`main.rs`(34)、`transport.rs`、`heap.rs`；C 对照点 `main.c:433-492`、`main.c:277-383`。
- **知识点清单**：K-170~K-180。
- **验收标准**：读者能画出 `run_once` 的三条臂与回信分支；能说出 walker 每一轮的四件事与全部终止态；能解释"连续 receive 失败 64 次"相对 C 的 `panic` 是有意偏离还是缺陷；能指出旧文档里哪四处"执行半待建"声明已被本篇作废。

### 27-mib-verification（新建）

- **一句话定位**：什么算对：四份 C 契约各自定义了哪些行为、Rust 侧 152 个测试分布在哪、以及"文档声称的测试必须逐名命中"这条对账纪律。
- **讲什么**：K-224~K-228 + K-197 的 test87 面。
- **不讲什么**：服务器内部机制（03~26）；跨 stage 联调（归 `../edge_todo.md` E5(g)，本篇只给指针）。
- **前置**：全部（建议读完主线后再读）。**后置**：无。
- **事实底线**：`minix/tests/test87.c`(3662)、`minix/tests/rmibtest/rmibtest.c`(267) + `testrmib.sh`、`tests/kernel/t_sysctl.c`(74)、`tests/sbin/sysctl/t_sysctl.sh`(45)；`os/servers/mib/src/**` 152 个 `#[test]`；`todo.md §1.1`（Gate E 对账）。
- **知识点清单**：K-224~K-228、K-197。
- **验收标准**：读者能对"我改了 create 的某个门"说出该跑哪一份 C 测试与哪一组 Rust 测试；能列出 rmibtest 的 8 个远端拒绝场景；能说出 Rust 测试数与各模块分布。

### 28-mib-build-deploy（新建）

- **一句话定位**：这个服务怎么被编出来、怎么进 boot 镜像、依赖谁、以及本 stage 对其它 stage 的六条债务在哪登记。
- **讲什么**：K-229、K-230 + K-021/K-024 的部署面。
- **不讲什么**：服务内部机制；Rust 侧 seam 通电（归 edge）。
- **前置**：03。**后置**：无。
- **事实底线**：`minix3/minix/servers/mib/Makefile`(21)；`kernel/table.c:52-63`；`../edge_todo.md` 六条（E-RMIBWIRE / E-MIBPROD / E-MIBGRANT / E-DSWIRE / E-ISWIRE / E5(g)）；`os/servers/mib/Cargo.toml`。
- **知识点清单**：K-229、K-230。
- **验收标准**：读者能列出 8 个源文件与链接的库；能说出 MIB 在 boot_image 里的位置及其因果；能对"我要给 MIB 加一个依赖"说出该查哪份文件、该在 edge 的哪一条登记。

### 99-mib-global-concepts

- **一句话定位**：查询手册：常量、错误码、跨服务引用各归一位，机制细节一律只给指针。
- **讲什么**：端点号与号段、CTL_*/CTLTYPE_*/CTLFLAG_* 常量总表、错误码汇总（含 ENOMEM / ERESTART / EDONTREPLY 的特殊语义）、跨服务引用表、执行模型声明。
- **不讲什么**：各机制细节（01~28）。
- **前置**：全部（随手查）。**后置**：无。
- **事实底线**：`com.h:66/613-622/1022-1030`；`sys/sys/sysctl.h`；`minix/include/minix/sysctl.h`；`os/libs/minix-types/src/types/{com.rs,sysctl.rs,errno.rs,endpoint.rs}`。
- **知识点清单**：（全部为存量汇总，不新增编号；每项必须给出"权威位置"。）
- **验收标准**：每一张常量表后都有一行"权威位置"；错误码表能区分"普通 errno"与"sysctl 方言里的特殊语义"三类（ENOMEM / ERESTART / EDONTREPLY）；不出现任何"机制解释"段落。

---

## 6. 变更表

### 6.1 统一变更表

| 操作编号 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 / 来源 |
|---|---|---|---|---|---|---|
| C-01 | 重排（大） | `21-mib-client-libc.md` | `01-mib-user-entry.md` | 提问方是理解服务端的起点；旧位置造成 00 §3 的旅程图前向引用 21 | K-001~K-008 | 8 条整篇上移，无拆分 |
| C-02 | 保持 + 增补 | `02-mib-message-contract.md` | `02-mib-protocol.md` | 编号恰好不变；补 A-4 现状 | K-009~K-019 + 新 K-231/K-232 | 存量 11 条原地；新增 2 条来源 = `types/sysctl_abi.rs:428/446`、`ps_strings.rs:22`、`kinfo.rs:20` |
| C-03 | 重排 + 拆分 | `01-mib-init-main.md` | `03-mib-service-lifecycle.md` | 01 让位给用户态入口；本篇保留 C 真序骨架 | K-021~K-033、K-035 | `:90-258` 拷贝原语段 → 07（O-1）；`:36-64`/`:384-413` → 05 |
| C-04 | 重排 | `03-mib-node-model.md` | `04-mib-node-model.md` | 顺延一位 | K-036~K-050 | 15 条整篇移动 |
| C-05 | 重排 + 删段 | `04-mib-static-tree-init.md` | `05-mib-static-tree.md` | 顺延；删掉 arena 承诺段 | K-051~K-059 | arena 承诺段 → 10（O-4）；新增 K-059 来源 = `tree/arena.rs` |
| C-06 | 重排 | `05-mib-tree-lookup.md` | `06-mib-lookup.md` | 顺延 | K-060~K-064 | 5 条整篇移动 |
| C-07 | 重排 + 增补 | `06-mib-copy-io.md` | `07-mib-copy-relay.md` | 顺延；补 relay 的所有权模型 | K-065~K-076 | 新增 K-076 来源 = `io/relay.rs` |
| C-08 | 重排 | `07-mib-auth-model.md` | `08-mib-auth.md` | 顺延 | K-077~K-083 | 7 条整篇移动 |
| C-09 | 重排 | `08-mib-dynamic-nodes.md` | `09-mib-dynamic-nodes.md` | 顺延 | K-084~K-093 | 10 条整篇移动；内存所有权 → 10 |
| C-10 | **新建** | — | `10-mib-arena-budget.md` | G-2：arena(611)+heap(190) 无主篇 | K-094~K-102 | 来源：旧 04 §4 / 08 §2.3-2.4 / 13 §? / 15 §4.4 的 arena 指针 + `arena.rs`/`heap.rs` + `todo.md P2-3/P2-4` |
| C-11 | 重排 | `09-mib-data-access.md` | `11-mib-data-access.md` | 顺延（10 插入） | K-103~K-111 | 9 条整篇移动 |
| C-12 | 重排 | `11-mib-query-describe.md` | `12-mib-query-describe.md` | 顺延 | K-112~K-119 | 新增 K-119 来源 = `describe.rs` + `sysctl_abi.rs:134` |
| C-13 | **新建** | — | `13-mib-transport-seams.md` | G-1：transport(469) + seam 形状无归属 | K-120~K-127 | 来源：`transport.rs`、`server.rs::MibIpc`、`todo.md §4 L0/L2` |
| C-14 | 拆分（上） | `12-mib-remote-subtrees.md` §注册/挂载 | `14-mib-remote-mount.md` | G-7：一篇两个文件四件事 | K-128~K-141 | 存量 14 条：`remote.c:24-192/197-307` 的端点表与生命周期 + `tree.c:1544-1842` 的挂载手术 |
| C-15 | 拆分（下） | `12-mib-remote-subtrees.md` §转发/死亡 | `15-mib-remote-relay.md` | 同上 | K-142~K-150 | 存量 9 条：`remote.c:316-477`；grant 语义本身 → 07（O-2） |
| C-16 | 重排 + 上移 | `22-mib-rmib-client.md` | `16-mib-rmib-client.md` | G-8：客户端库应紧贴远程子树对端 | K-151~K-159 | 9 条整篇上移；服务端语义段 → 15（O-5） |
| C-17 | 重排 + 拆段 | `10-mib-dispatch.md` | `17-mib-dispatch.md` | 判定半留下，执行半拆给 26 | K-160~K-169 | walker 执行体 → 26；新增 K-168 来源 = `dispatch.rs::SysctlOutcome` |
| C-18 | 重排 | `13-mib-subtree-kern.md` | `18-mib-subtree-kern.md` | 顺延 | K-181~K-188 | 8 条整篇移动 |
| C-19 | 重排 | `14-mib-subtree-vm-hw.md` | `19-mib-subtree-vm-hw.md` | 顺延 | K-189~K-196 | 8 条整篇移动 |
| C-20 | 重排 | `15-mib-subtree-minix.md` | `20-mib-subtree-minix.md` | 顺延 | K-197~K-202 | 6 条整篇移动 |
| C-21 | 重排 | `16-mib-proc-tables.md` | `21-mib-proc-tables.md` | 顺延 | K-203~K-208、K-223 | 7 条整篇移动 |
| C-22 | 重排 | `17-mib-proc-lwp.md` | `22-mib-proc-lwp.md` | 顺延 | K-209~K-213 | 5 条整篇移动 |
| C-23 | 重排 | `18-mib-proc2.md` | `23-mib-proc2.md` | 顺延 | K-214~K-217 | 4 条整篇移动 |
| C-24 | 重排 | `19-mib-proc-args.md` | `24-mib-proc-args.md` | 顺延 | K-218、K-219 | 2 条整篇移动 |
| C-25 | 重排 | `20-mib-minix-proc.md` | `25-mib-minix-proc.md` | 顺延 | K-220~K-222 | 3 条整篇移动 |
| C-26 | **新建** | — | `26-mib-assembly-walker.md` | G-1：server(876)+walker(1125) 无归属 | K-170~K-180 | 来源：`server.rs`/`walker.rs`/`main.rs` + 旧 01 §4 的 verdict 层 + 旧 10 §4 的一句补记 |
| C-27 | **新建** | — | `27-mib-verification.md` | G-3：4048 行 C 契约 + 152 Rust 测试无归属 | K-224~K-228 | 来源：旧 15 §5、21 §5、plan §5.4 的碎片 + `tests/` 四件 + `todo.md §1.1` |
| C-28 | **新建** | — | `28-mib-build-deploy.md` | G-4：Makefile/boot/依赖链无归属 | K-229、K-230 | 来源：`Makefile`(21) + `table.c` + 旧 99 §5 + `../edge_todo.md` |
| C-29 | 重写 | `00-mib-overview.md` | `00-mib-overview.md` | 导航表换全套编号 | K-008、K-021~K-023、K-169 | 存量 5 条保留；导航表按 §4.1 重排 |
| C-30 | 重写 | `99-mib-global-concepts.md` | `99-mib-global-concepts.md` | 编号不变；删越界段 | 常量/错误码汇总 | 端点常量偏差史 → `../edge_todo.md`（O-6） |
| C-31 | 归档（B 相） | 全部 24 篇旧正文 | `archive/` | 内容被吸收后退出正式目录（不删） | 全部存量条目 | 每个知识点在 §5 契约里都有新落点，无丢弃 |

### 6.2 统计

| 类型 | 数量 | 编号 |
|---|---|---|
| 重排（含顺延） | 19 | C-01、C-03~C-09、C-11、C-12、C-16~C-25 |
| 拆分 | 2 | C-14、C-15 |
| 合并 | 0（无合并需求：每篇已是最小语义单元，唯一"两文件一篇"的 12 已拆开） |
| 新建 | 5 | C-10、C-13、C-26、C-27、C-28 |
| 重写 | 2 | C-29、C-30 |
| 归档 | 1（批） | C-31 |
| 明确删除 | 1 段 | 旧 04 的 arena 承诺段（并入 C-10）；旧 99 §1 的端点偏差史（移交 edge） |

---

## 7. 缺漏新篇

按提示词 §步骤 6 的固定清单逐项落实（此节不留空、不写"待定"）。

| 非 C 主题 | 重要性与原料 | 归哪一篇 | 验收标准 |
|---|---|---|---|
| **链接与加载** | MIB 如何被链接成可执行服务决定了"新增一个 C 文件要改哪里"；原料 `Makefile:9-17` | `28` §1 | 能列出 8 源 + `-lsys` + `minix.service.mk` 三件事 |
| **镜像与内存布局** | boot 位置决定了"为什么不能晚启动"；原料 `table.c:52-63`、`main.c:263-267` | `28` §2 + `03` §1 | 能说出 MIB 在 boot_image 的第几位、前后是谁、抑制何时解除 |
| **汇编入口与陷阱进入** | 用户态服务无此环节；陷阱层归 14-stage-runtime / 01-stage-kernel | **明确否决**：不在本 stage | 蓝图已给否决理由（无汇编入口，入口是 `main()`） |
| **启动装配** | SEF 双回调是全部用户态服务的共同姿势；原料 `main.c:415-428` | `03` §2 | 能说出 fresh/restart 两个回调各承诺什么状态 |
| **构建与工具链** | 决定"加一个源文件改哪里"；原料 `Makefile`(21) | `28` §1 | 同上"链接与加载" |
| **跨模块接口与线格式** | 这是 MIB 最大的契约面（六载荷 + sysctlnode/sysctldesc + kinfo 族）；原料 `ipc.h`、`sysctl.h:1382-1450`、`minix/sysctl.h` | `02`（全表）+ `12`（序列化）+ `22`/`23`/`25`（各自输出布局） | 能手算一个 flags 字；能说出 `sysctlnode` 96B 的稳定性来源 |
| **错误路径** | sysctl 的错误码方言是行为契约的核心；原料 `main.c` 各 return + `errno.rs` | 各篇自门 + `99` §4 汇总 | 能区分"普通 errno"与三类特殊语义 |
| **关闭与退出** | MIB 无此路径（`for(;;)` + NOTREACHED） | **明确否决**：不在本 stage | 否决依据 `main.c:443/490` 已写入蓝图 |
| **并发与同步** | 单线程事件循环 + 嵌套阻塞 sendrec 是 MIB 的并发模型全貌；原料 `lib.rs` 注释、`remote.c:439` | `13` §阻塞语义 + `99` §执行模型 | 能说出"为什么阻塞不是缺陷"以及 Rust 侧为何不需要锁 |
| **测试基建** | 4048 行 C 契约 + 152 Rust 测试是"什么算对"的唯一定义；原料 `tests/` 四件 + `os/servers/mib/src/**` | `27` | 见 `27` 的验收标准 |

**额外缺口**（不在固定清单内，但由 §3.2 识别，一并落实）：

| 缺口 | 归哪一篇 | 验收标准 |
|---|---|---|
| Rust 执行半（server/walker）无文档 | `26` | 见 `26` 验收标准 |
| 树竞技场与字节预算无主篇 | `10` | 见 `10` 验收标准 |
| A-4 尾随结构结论过期 | `02` §交换格式现状 + `18`/`19`/`24` 的"布局锚点"小节 | 三篇各自能说出自己消费的结构钉在哪、多少字节 |
| 六处"执行半待建"声明过期 | `26` §现状纪律 | 能指出旧声明的作废位置与当前状态 |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表（逐篇逐节）

> 迁移类型：原样搬移 / 改写 / 合并 / 拆分 / 删除。
> 旧篇章节名取自各篇 `##` 骨架（实测）。

| 旧位置 | 旧内容（一句话） | 新位置 | 类型 | 断链风险 |
|---|---|---|---|---|
| `00` §1 概念 | 为什么需要"管着一切却不拥有数据"的服务 | `00` §1 | 原样 | 低（编号不变） |
| `00` §2 主线 | 启动时序图 | `03` §2 + `00` §2（缩略） | 拆分 | 中：启动图被 03 接管 |
| `00` §3 次主线 | sysctl 调用旅程 | `17` §1.3（全图）+ `00` §3（缩略） | 拆分 | 中 |
| `00` §4 第二条次主线 | 远程子树旅程 | `14` §1 + `15` §1 | 拆分 | 中 |
| `00` §5 文档导航 | 编号表 | `00` §5 | 改写（换全套编号） | **高：本表是全目录的引用热点** |
| `00` §6 设计原则 | 四条纪律 | `00` §6 | 原样 | 低 |
| `00` §7/§8 | 边界/参见 | `00` §7/§8 | 改写 | 低 |
| `01` §1 概念 | 主循环为什么长这样 | `03` §1 | 改写 | 中 |
| `01` §2 C 源码分析 | main/startup/loop/mib_sysctl + 拷贝鉴权段 | `03` §2（前）+ `07` §2（拷贝段）+ `08` §2（鉴权段） | 拆分 | **高：本节被切成三块** |
| `01` §3 Rust 决策 | verdict/执行分离 | `03` §3 | 改写 | 低 |
| `01` §4 实现详解 | dispatch.rs 判定 | `03` §4 | 改写 | 低 |
| `01` §5 测试要点 | 9 个测试 | `27` §测试地图（汇总）+ `03` §5 | 合并 | 中 |
| `01` §6/§7 | 过渡/参见 | `03` §6/§7 | 改写编号 | 中 |
| `02` §1~§7 | 协议面全部 | `02` §1~§7 | 原样 + §2 增补 | 低 |
| `03` §1~§7 | 节点模型 | `04` §1~§7 | 原样 | 低 |
| `04` §1~§3 | 静态树概念/源码/决策 | `05` §1~§3 | 原样 | 低 |
| `04` §4 实现详解（含 arena 承诺） | static_tree.rs + arena 缺口 | `05` §4（去 arena）+ `10` §2（arena） | 拆分 | **高：arena 承诺四处指针的收口点** |
| `04` §5~§7 | 测试/过渡/参见 | `05` §5~§7 | 改写编号 | 中 |
| `05` §1~§7 | 查找 | `06` §1~§7 | 原样 | 低 |
| `06` §1~§7 | 拷贝原语 | `07` §1~§7（§2 吸收 `01` §2 的拷贝段） | 合并 | 中 |
| `07` §1~§7 | 权限 | `08` §1~§7（§2 吸收 `01` §2 的鉴权段） | 合并 | 中 |
| `08` §1~§7 | 动态节点 | `09` §1~§7；§2.3/§2.4 所有权 → `10` §3 | 拆分 | 中 |
| `09` §1~§7 | 数据读写 | `11` §1~§7 | 原样 | 低 |
| `10` §1~§3 | 分发概念/源码/决策 | `17` §1~§3 | 原样 | 低 |
| `10` §4 实现详解（含 walker 补记） | 判定 + walker 一句 | `17` §4（判定）+ `26` §3（walker 执行） | 拆分 | **高：walker 唯一文字记载** |
| `10` §5~§7 | 测试/过渡/参见 | `17` §5~§7 | 改写编号 | 中 |
| `11` §1~§7 | 枚举与描述 | `12` §1~§7 | 原样 | 低 |
| `12` §1~§3 | 远程概念/源码/决策 | `14` §1~§3（注册挂载）+ `15` §1~§3（转发死亡） | 拆分 | **高：本篇唯一承载 remote.c + tree.c mount** |
| `12` §4 实现详解 | remote.rs + mount.rs | `14` §4 + `15` §4 | 拆分 | 高 |
| `12` §5~§7 | 测试/过渡/参见 | `14`/`15` 各自 §5~§7 | 拆分 | 中 |
| `13` §1~§7 | kern 子树 | `18` §1~§7 | 原样 | 低 |
| `14` §1~§7 | vm+hw 子树 | `19` §1~§7 | 原样 | 低 |
| `15` §1~§7 | minix 子树 | `20` §1~§7（§1 补 test87 耦合） | 改写 | 低 |
| `16` §1~§7 | 进程表快照 | `21` §1~§7 | 原样 | 低 |
| `17` §1~§7 | LWP | `22` §1~§7 | 原样 | 低 |
| `18` §1~§7 | PROC2 | `23` §1~§7 | 原样 | 低 |
| `19` §1~§7 | PROC_ARGS | `24` §1~§7 | 原样 | 低 |
| `20` §1~§7 | MINIX_PROC | `25` §1~§7 | 原样 | 低 |
| `21` §1~§7 | libc 客户端 | `01` §1~§7 | 原样（整篇上移） | **高：编号从 21 变 01** |
| `22` §1~§7 | rmib 客户端 | `16` §1~§7；§2 的服务端语义 → `15` §2 | 拆分 | **高：编号从 22 变 16** |
| `99` §1 服务坐标 | 端点号/boot/特权 | `99` §1（删偏差史）+ `28` §2 | 拆分 | 中 |
| `99` §2~§8 | 常量/错误码/跨服务引用 | `99` §2~§8 | 改写（更新引用编号） | 中 |

### 8.2 引用迁移表

**A. 目录内交叉引用（171 处，实测）**

热点（按被引用次数）：`10-mib-dispatch`(17)、`02-mib-message-contract`(14)、
`06-mib-copy-io`(12)、`12-mib-remote-subtrees`(11)、`08-mib-dynamic-nodes`(10)、
`03-mib-node-model`(10)、`16-mib-proc-tables`(9)、`04-mib-static-tree-init`(9)。

| 旧引用 | 新目标 | 验证方式 |
|---|---|---|
| `01-mib-init-main.md` | `03-mib-service-lifecycle.md` | `rg -c "01-mib-init-main" *.md` 归零 |
| `02-mib-message-contract.md` | `02-mib-protocol.md` | 同上 |
| `03-mib-node-model.md` | `04-mib-node-model.md` | 同上 |
| `04-mib-static-tree-init.md` | `05-mib-static-tree.md` | 同上 |
| `05-mib-tree-lookup.md` | `06-mib-lookup.md` | 同上 |
| `06-mib-copy-io.md` | `07-mib-copy-relay.md` | 同上 |
| `07-mib-auth-model.md` | `08-mib-auth.md` | 同上 |
| `08-mib-dynamic-nodes.md` | `09-mib-dynamic-nodes.md` | 同上 |
| `09-mib-data-access.md` | `11-mib-data-access.md` | 同上 |
| `10-mib-dispatch.md` | `17-mib-dispatch.md`（判定）/ `26-mib-assembly-walker.md`（执行） | 同上 + 人工判上下文是判定还是执行 |
| `11-mib-query-describe.md` | `12-mib-query-describe.md` | 同上 |
| `12-mib-remote-subtrees.md` | `14-mib-remote-mount.md` / `15-mib-remote-relay.md` | 同上 + 人工判注册还是转发 |
| `13-mib-subtree-kern.md` | `18-mib-subtree-kern.md` | 同上 |
| `14-mib-subtree-vm-hw.md` | `19-mib-subtree-vm-hw.md` | 同上 |
| `15-mib-subtree-minix.md` | `20-mib-subtree-minix.md` | 同上 |
| `16-mib-proc-tables.md` | `21-mib-proc-tables.md` | 同上 |
| `17-mib-proc-lwp.md` | `22-mib-proc-lwp.md` | 同上 |
| `18-mib-proc2.md` | `23-mib-proc2.md` | 同上 |
| `19-mib-proc-args.md` | `24-mib-proc-args.md` | 同上 |
| `20-mib-minix-proc.md` | `25-mib-minix-proc.md` | 同上 |
| `21-mib-client-libc.md` | `01-mib-user-entry.md` | 同上 |
| `22-mib-rmib-client.md` | `16-mib-rmib-client.md` | 同上 |
| `99-mib-global-concepts.md` | 不变 | — |

**B. 目录外活跃文档引用（7 处，实测）**

| 旧引用（文件:行） | 新目标 | 验证方式 |
|---|---|---|
| `13-stage-ipc/03-ipc-mib-registration.md`（2 处） | 分别指向 `16-mib-rmib-client.md`（原 22）与 `14-mib-remote-mount.md` / `15-mib-remote-relay.md`（原 12） | `rg -n "10-stage-mib/" 13-stage-ipc/*.md` 逐条替换 |
| `13-stage-ipc/01-ipc-init-main.md`、`02-ipc-message-contract.md`、`09-ipc-proc-events.md`、`plan.md`（各 1 处） | 按上下文映射到新编号 | 同上 |

**C. 代码注释引用（5 处，实测）**

| 文件:行 | 旧引用 | 新目标 |
|---|---|---|
| `os/servers/mib/src/main.rs:4` | `10-stage-mib/01-mib-init-main.md` | `10-stage-mib/03-mib-service-lifecycle.md` |
| `os/servers/mib/src/lib.rs:8` | `10-stage-mib/01-mib-init-main.md` | `10-stage-mib/03-mib-service-lifecycle.md`（lib.rs 的模块清单还应补 `server`/`walker`/`arena`/`heap` 四行） |
| `os/libs/minix-sys/src/rmib.rs:832` | 提及"已在 10-stage-mib 侧声明" | 指向 `16-mib-rmib-client.md` |
| `os/libs/minix-sys/src/lib.rs:30` | `10-stage-mib (22-mib-rmib-client.md)` | `10-stage-mib (16-mib-rmib-client.md)` |
| `os/libs/minix-sys/src/lib.rs:101` | `10-stage-mib/22-mib-rmib-client.md` | `10-stage-mib/16-mib-rmib-client.md` |

**D. `.review/` 历史记录（93 处）**

建议**不改**。理由：`.review/` 下的 scan/structure/VERIFY-CHECK 是"某时点对某篇文档的审查记录"，
其中的文件名指向的是历史对象；改了会让历史记录指向一个当时并不存在的文件，反而失真。
替代做法：在 `archive/` 目录（B 相归档旧正文的地方）放一份 `README` 说明"旧编号 → 新编号"对照表，
让历史记录的可追溯性由对照表承载。**此条提请用户裁决**（见 §9.3 问题 Q-3）。

### 8.3 断链成本摘要

| 项 | 数量 | 说明 |
|---|---|---|
| 目录内交叉引用 | 171 处 | 全部落在 24 篇旧文件名上，可用一条 `sed` 批量替换（22 条规则） |
| 目录外活跃文档引用 | 7 处 | 分布在 `13-stage-ipc/` 5 个文件，需人工判上下文（10/12/22 各被拆或改名） |
| 代码注释引用 | 5 处 | 分布在 3 个文件，逐条替换即可 |
| `.review/` 历史记录 | 93 处 | 建议不改（见 §8.2 D） |
| **需人工判断的引用** | **约 29 处** | 旧 10（拆判定/执行）、旧 12（拆注册/转发）、旧 22（拆服务端语义）三篇的引用需按上下文分流；其余均为机械替换 |
| 热点文件 | `00-mib-overview.md`（导航表）、`README.md`（清单表）、`plan.md`（§2/§3.4/§5.3 三张表）、`todo.md`（大量编号引用） | 这四份不是"概念文档"，但断链后最影响导航 |
| 建议批量方式 | ① 先写一张 `旧文件名 → 新文件名` 的 22 条映射表；② 对目录内 `*.md` 跑一次机械替换；③ 对旧 10/12/22 的引用先做一次 `rg -n` 列出上下文，人工分流后再替换；④ 代码注释 5 处单独替换 | — |

---

## 9. 验证与自检门

### 9.1 四种机械检查

| 检查 | 方法 | 结果 |
|---|---|---|
| ① 前向引用扫描 | 逐篇读 §5 契约的"前置"字段，确认只指向更小编号 | **通过**。逐篇前置表：`01:{}`、`02:{01}`、`03:{01,02}`、`04:{02}`、`05:{03,04}`、`06:{04}`、`07:{02}`、`08:{02}`、`09:{04,06,07,08}`、`10:{09}`、`11:{04,07,08}`、`12:{02,04,07,08}`、`13:{07}`、`14:{04,06,07,13}`、`15:{07,13,14}`、`16:{02,15}`、`17:{06,07,08,11,12,15}`、`18:{04,07,08,11}`、`19:{04,07,08,11,13}`、`20:{04,11}`、`21:{02,13}`、`22:{21,04}`、`23:{21}`、`24:{21,07}`、`25:{21,22}`、`26:{17,13,15,18..25}`、`27:{全部}`、`28:{03}`、`99:{全部}` —— 全部严格小于自身编号 |
| ② 依赖关系图无环 | 由①的前置关系构图 | **通过**。图为分层 DAG：P1→P2→P3→P4→P5→P6→P7→P8→P9→P10→P11→P12；唯一的"跨层回边"是 `26` 依赖 `18~25`（P9/P10），而 P9/P10 的前置不含 26，无环 |
| ③ 覆盖率 100% | 知识点池 232 条逐条查 §5 契约的"知识点清单" | **通过**。232 条全部出现在至少一篇契约中，其中 20 条（K-005、K-008、K-021~K-023、K-169、K-197 等）出现在两篇（主讲述点 + 引用点），已按 §3.3 重复表指定主讲述点。删除项 1 段（旧 04 的 arena 承诺段，理由：承诺已兑现，内容并入 `10` §2），已在 §6.1 C-31 单列 |
| ④ 断链成本统计 | §8.3 | **已统计**：目录内 171、目录外活跃 7、代码注释 5、历史记录 93（建议不改）；需人工判断 29 处 |

### 9.2 自检门 G1~G9

| 门 | 检查内容 | 结果 | 说明 |
|---|---|---|---|
| G1 | C 真序逐条可核对（随机抽十条） | **通过** | 抽 S1(`table.c:52-63`)、S7(`main.c:425`)、S9(`main.c:395-398`)、S11(`tree.c:1480-1512`)、A3(`main.c:302-303`)、A6(`main.c:334-340`)、B1(`remote.c:210-211`)、B7(`tree.c:1572-1579`)、D5(`tree.c:1389-1390`)、R6(`remote.c:442-446`) —— 十条均为本次执行中 `sed`/`grep` 亲见，非转述 |
| G2 | 知识点池完整（每个 C 文件、每个非 C 制品都有归属） | **通过** | 8 个 .c + mib.h：main.c→03/07/08、tree.c→04/05/06/07/09/10/11/12/14/15/17、remote.c→14/15、kern.c→18、proc.c→21~25、hw.c/vm.c→19、minix.c→20；非 C 制品：Makefile→28、table.c→28/03、libc 五文件→01、rmib.c/h→16、test87/rmibtest/t_sysctl→27；Rust：34 文件全部有归属（server/walker→26、arena/heap→10、transport→13、io→07、auth→08、data→11、query/describe→12、remote/mount→14/15、subtree→18/19/20、proc→21~25、dispatch→03/17、tree 其余→04/05/06/09、sef→03、main→26） |
| G3 | 新目录前向引用为零 | **通过** | 见 §9.1 ① |
| G4 | 依赖图无环 | **通过** | 见 §9.1 ② |
| G5 | 覆盖率 100% + 新增有锚点 + 删除单列 | **通过** | 见 §9.1 ③；33 条新增全部带 Rust 文件路径或 C 行号；删除项 1 段已单列 |
| G6 | 拆分/合并写清存量去向；新建写清新增来源（抽查十处） | **通过** | 抽查 C-03（01 拆三块，逐段注明去向）、C-05（04 删 arena 段→10）、C-07（06 吸收 01 §2 拷贝段）、C-08（08 吸收 01 §2 鉴权段）、C-10（新建，来源四项）、C-13（新建，来源三项）、C-14/C-15（12 拆两篇，14+9 条各归其位）、C-16（22 上移，服务端语义→15）、C-26（新建，来源四项）、C-27（新建，来源三项） |
| G7 | 每篇契约七要素齐全 | **通过** | 30 篇（00~28 + 99）逐篇含：定位、讲什么、不讲什么、前置、后置、事实底线、知识点清单、验收标准 |
| G8 | 锚点迁移表覆盖所有变化文档的每一节；引用迁移表覆盖文档与代码注释 | **通过** | §8.1 覆盖 24 篇旧文档的全部 `##` 节（实测骨架）；§8.2 覆盖目录内 171、目录外 7、代码注释 5、历史记录 93 四类 |
| G9 | 事实断言都有锚点（随机抽十条）；推测项已标注 | **通过** | 抽：`MIB_PROC_NR=7`(`com.h:66`)、`CTL_MAXNAME=12`(`sysctl.h:75`)、`CTL_SHORTNAME=8`(`ipc.h:15`)、`SYSCTL_VERS_1`(`sysctl.h:132`)、`miblen<2 → EPERM`(`tree.c:1572-1579`)、`三 grant 逆序撤销`(`remote.c:442-446`)、`Rust 152 tests`（grep 实测）、`Rust mib 11680 行`（wc 实测）、`test87 3662 行`（wc 实测）、`arena 611/heap 190`（wc 实测）—— 十条全部为本次执行亲测。**推测项标注**：K-035（C panic vs Rust 64）的依据是 `server.rs` 常量 + `todo.md`，两者一致，非推测；G-10（六处过期声明）的"六处"由 `todo.md §P1-1~P1-5` + 四处 follow-up 标记共同支撑，已在正文写为"六处"并在 §0.4 给出 grep 计数偏低的事实，标注为**待 B 相逐条核对** |

### 9.3 结论与待用户裁决的问题

**结论**：蓝图完成。新目录 = 30 篇（`00` + `01`~`28` + `99`），其中 19 篇重排、
2 处拆分、5 篇新建、2 篇重写、1 批归档。知识点池 232 条（存量 199 / 新增 33）全部有落点。
覆盖缺口 10 项（G-1~G-10）逐项落实，非 C 主题固定清单十项逐项回答（2 项明确否决并给依据）。
断链成本已量化：机械替换 176 处、人工分流 29 处、建议不改 93 处。

**待用户裁决**：

- **Q-1（重编号幅度）**：本蓝图选择了"大幅重编号"（24 篇里 21 篇换了号）。
  替代方案是"保号重建"（只做拆分/新建，旧号尽量不动），代价是新目录无法保证
  "编号即阅读顺序"。我选前者的理由是：本目录当前最大的问题恰恰是**顺序错**（用户态入口
  在倒数第二、执行层无家、远程子树一篇顶四件事），保号修不好顺序。**请裁决是否接受大幅重编号。**
- **Q-2（用户态入口上移）**：把 `21` 整篇上移为 `01`，意味着新目录的第一篇讲的是
  **不重写的东西**（libc）。这是有意的——它是理解"MIB 在答什么"的前提。
  若用户认为"外部契约不该占开篇"，可把它降为 `02` 并让协议面回到 `01`。
- **Q-3（`.review/` 历史记录）**：§8.2 D 建议不改 93 处历史引用，改由归档目录的
  对照表承载可追溯性。请确认。
- **Q-4（旧 `plan.md` / `todo.md` 的处置）**：这两份是"过程账本"，本蓝图把它们当作
  **知识来源**而非重建对象。B 相是否要同步重写（它们包含大量编号引用，断链后导航会很难用），
  还是随旧正文一并归档？我的建议是：归档旧的，按新目录重写一份精简的 `README.md` 作导航，
  `plan.md`/`todo.md` 的内容并入 `todo.md`（新）继续作为实现账本。
- **Q-5（A-4 尾随结构的文档结论）**：`Clockinfo` / `Timeval` / `PsStrings` / `KinfoStruct`
  已在 `minix-types` 落 `#[repr(C)]` + `offset_of!`，但 `plan.md` A-4 行仍写"随首消费时钉"。
  本蓝图按"代码真相源优先"处理（用户的第 3 条提醒）。请确认这条判定口径适用于本目录全部文档。
