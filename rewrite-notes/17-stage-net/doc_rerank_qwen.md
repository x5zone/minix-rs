# 17-stage-net 文档重建蓝图（qwen）

## 0. 元数据

- **执行者**：qwen
- **日期**：2026-09-19
- **目标目录**：`notes/rewrite/fork-syscall-rewrite/17-stage-net`
- **仓库根目录**：`/home/xzhao/github/minix-rs`
- **当前提交号**：`4c99bc2`（`git rev-parse HEAD` = `4c99bc2e785fa23ad3586b0267d561005fa0e409`）
- **交付物**：本文件；不改任何正文，不重命名/移动/删除任何现有文件。

### 0.1 审查范围

**算作文档（纳入知识点池）**：编号文档 `00`、`01`–`24`、`99`（共 26 篇）。
**算作参考材料（不作为搬迁对象，只作线索）**：`plan.md`（460 行）、`todo.md`（78 行）、`archive/todo-N1-archive-2026-09-17.md`、`draft/README.md`。
**范围外（明确排除）**：
- `doc_rerank_deepseek.md`（2597 行）、`doc_rerank_glm.md`（377 行）——其它 AI 的产物，按 bagging 规则**未读取、不参照**。
- `.design/`、`tmp_design_and_todo/`——项目规范视为中间产物，正式结论**不引用**。
- 相邻阶段正文（`05-stage-vfs`、`16-stage-drivers` 等）——只读其 `00-*-overview.md` 用于定边界，不改其内容。

### 0.2 读取清单

- **本目录全部文档**：26 篇（`00`/`01`–`24`/`99`）头部声明全部读取，正文按知识点池需要精读；`plan.md`、`todo.md` 全读。
- **Minix3 C 源码**：`minix3/minix/net/lwip/`（27 个 `.c`）、`minix3/minix/net/uds/`（3 个 `.c`）、`minix3/minix/lib/libsockdriver/sockdriver.c`（1150）、`minix3/minix/lib/libsockevent/sockevent.c`（2590）+`sockevent_proc.c`（52）、`minix3/minix/lib/libc/sys/socket.c` 等 15 文件。本蓝图独立核对的入口：`lwip.c`（全读，1–383）、`uds.c`（grep 主循环/分派）、`sockevent.c`（grep 全函数签名）。
- **非 C 制品**：`minix3/minix/net/lwip/Makefile`、`minix3/minix/net/lwip/lwip.conf`、`minix3/minix/net/uds/Makefile`；liblwip 胶水头 `lwipopts.h`/`lwiphooks.h`/`arch/cc.h` + `patches/`（4 个，由 `24` 篇承载）。本 stage 是**用户态服务器**，无链接脚本、无引导汇编、无陷阱入口（这些属于 `01-stage-kernel`）。
- **边界材料**：`00-master-plan/README.md`（阶段 17 = net，执行序在 `16-stage-drivers` 之后）、`16-stage-drivers/00-drivers-overview.md`（网卡驱动 22/23 → 协议栈；`minix-netdriver` 框架与 edge E-DMABUF 归属驱动 stage）。
- **Rust 实现入口**：`os/net/lwip/src/`（25 个 `.rs`，含 `main.rs`/`startup.rs`/`server.rs`/`lwip_port.rs`）、`os/net/uds/src/`（`core.rs`/`io.rs`/`main.rs`/`server.rs`）、`os/libs/minix-netdriver/src/`（`sockid.rs`/`socktable.rs`/`protocol.rs`/`service.rs` 等）、`os/libs/minix-sys/src/socket.rs`。
- **写法范例**：`01-stage-kernel/06-todo.md`（只学"新文档契约"的写法：讲什么/不讲什么/下放给谁/验收，不搬结论）。

### 0.3 使用的命令与关键证据（摘录）

```text
# C 源码清单（30 个 .c）
$ glob minix3/minix/net/**/*.c  →  lwip 27 + uds 3
$ glob minix3/**/libsock*/*.c   →  sockdriver.c 1150 / sockevent.c 2590 / sockevent_proc.c 52

# lwip 运行时入口（真序权威）
$ read minix3/minix/net/lwip/lwip.c 1-383
  main:293 → startup:269（sef_setcb_init_fresh(init) + sef_startup:287）
  init:195 → 20 个离散 init 调用（L203-263，见 §1）
  主循环:301-379：ifdev_poll → check_lwip_timer → sef_receive_status → 4 路分派
  alloc_socket:151-190：domain 分流 PF_INET/INET6→{STREAM,_DGRAM,RAW(root 门)}、PF_ROUTE、PF_LINK

# uds 运行时入口
$ grep uds.c: while(uds_running||uds_in_use>0)@1393 → sef_receive_status@1394
  → MIB_PROC_NR@1407 / sockevent_process@1410（2 路分派）

# sockevent 框架函数（全签名已核）
$ grep sockevent.c: sockhash_init/slot/get/add/del@36-99（256 槽）
  sockevent_socket@263（回调入参 = alloc_socket）、pump@858、expire@986、alarm@2517、
  init@2548、process@2572

# 构建/服务配置（非 C）
$ cat minix3/minix/net/lwip/Makefile → SRCS 27 个 .c；DPADD: -llwip -lsockevent -lsockdriver -lchardriver -lsys -ltimers
$ head minix3/minix/net/lwip/lwip.conf → domain INET INET6 ROUTE LINK; system KILL(SIGPIPE); ipc SYSTEM vfs rs vm mib

# 覆盖率核对
$ grep -rlni mibtree|rmib_process → 仅 03/21/99/plan（无专篇）  ← 覆盖缺口 G1
$ grep -rln pchain → 04（已并入 mempool）

# 断链成本
$ grep -rlE "17-stage-net" os/ notes/（排除本目录）→ 205 个文件引用本 stage
$ 本目录内部对 "NN-xxx.md" 的引用计数：03→26、06→19、02→19、99→17、01→17 ...
```

---

## 1. C 真序（运行时真序重建）

### 1.1 阶段类型判定

本 stage **同时具备三种特征**，按提示词第九部分择主处理：

- **服务事件循环型**（主特征）：lwip、uds 两个 SEF 服务器进程——按"为什么存在 → 诞生与初始化 → 消息接口 → 核心数据结构 → 按场景分组的请求处理 → 查询与杂项 → 与邻接服务的协议"组织。
- **库与框架型**（次特征）：libsockdriver / libsockevent / liblwip 三套框架库——先讲抽象层与接口契约，再讲框架骨架，最后按实现族展开。
- **集合型（弱）**：多种 socket 家族（tcp/udp/raw/lnk/rt）——归入服务型的"按场景分组请求处理"，不单独成集。

**处理决定**：以**服务型**为主骨架，一次请求的生命周期作为主线（第九部分明示"一次请求的生命周期通常比源码调用顺序更适合作主线"）；框架库作为主线的**前置地基**先讲（读者不看懂 SDEV 协议与 sock 对象，无法读懂服务分派）。

### 1.2 lwip 服务真序表（锚点：`minix3/minix/net/lwip/lwip.c`）

| # | 动作 | C 函数 / 锚点 | 说明 |
|---|------|---------------|------|
| L1 | 进程诞生，注册初始化回调 | `main:293` → `startup:269` → `sef_setcb_init_fresh(init):273` | 不设 `_restart` 回调（无状态重启，L274-285 注释） |
| L2 | 进入 SEF 启动 | `sef_startup():287` | 由 SEF 框架回调 `init` |
| L3 | 播种弱随机 | `srand48(clock_time):203` | 见 `lwip_hook_rand` 注释 L126-143 |
| L4 | 初始化第三方栈 | `lwip_init():206` | liblwip（`24` 篇） |
| L5 | 初始化事件框架，注册 socket 分配回调 | `sockevent_init(alloc_socket):209` | 框架库 `02` 篇；回调 = L23 分流器 |
| L6 | 辅助模块 | `mempool_init():212`、`tcpisn_init():213`、`mcast_init():214` | 缓冲/ISN/组播（`04`/`08`/`12`） |
| L7 | 高层 socket 模块 | `ipsock_init():217`、`tcpsock_init():218`、`udpsock_init():219`、`rawsock_init():220` | 家族（`06`/`08`/`09`/`10`） |
| L8 | 网络接口模块 | `ifdev_init():223`、`loopif_init():224`、`ethif_init():225` | 接口对象（`14`/`15`） |
| L9 | 网络设备驱动模块 | `ndev_init():228` | NDEV 消费侧（`13`） |
| L10 | 低层 socket 模块 | `rtsock_init():231`、`lnksock_init():232` | 路由/链路 socket（`20`/`11`） |
| L11 | 路由模块 | `route_init():235` | 路由表（`19`） |
| L12 | 其它设备 | `bpfdev_init():238` | BPF（`18`） |
| L13 | MIB 子树 | `mibtree_init():244` | **各模块先注册子树，MIB 最后初始化**（L240-244 注释）→ 缺口篇（见 §3） |
| L14 | 默认配置 | `ifconf_init():250` | 建 lo0 回环（`17`） |
| L15 | 定时器 + 置运行标志 | `init_timer(&lwip_timer):257`、`recheck_timer=TRUE:259`、`running=TRUE:261` | 进入主循环前的最后一步 |
| L16 | 主循环·轮询回环 | `while(running):301` → `ifdev_poll():307` | 回环延迟投递处理 |
| L17 | 主循环·检查定时器 | `check_lwip_timer():315` | 依赖 `sys_now:26` 设的 `recheck_timer` 标志 |
| L18 | 主循环·阻塞收消息 | `sef_receive_status(ANY,&m,&ipc_status):317` | 单线程事件循环 |
| L19 | 分派 A·notify | `is_ipc_notify:325`→ CLOCK:`expire_timers:328` / DS:`ndev_check:334` | 时钟到期 / 网卡上下线 |
| L20 | 分派 B·MIB | `case MIB_PROC_NR: rmib_process(&m):347` | 查询接口 → 缺口篇 |
| L21 | 分派 C·VFS | `IS_SDEV_RQ:354`→`sockevent_process:355`；`IS_CDEV_RQ/BDEV:361`→`bpfdev_process:362` | socket 请求 / BPF 字符设备请求 |
| L22 | 分派 D·NDEV | default: `IS_NDEV_RS:370`→`ndev_process:371` | 网卡驱动响应 |
| L23 | socket 分配分流（被 L5 注册） | `alloc_socket:151` → PF_INET/INET6:`tcpsock_socket:163`/`udpsock_socket:166`/`rawsock_socket:172`(先 `util_is_root:169` 门) / PF_ROUTE:`rtsock_socket:179` / PF_LINK:`lnksock_socket:182` | sockid 命名空间的源头 |

### 1.3 uds 服务真序表（锚点：`minix3/minix/net/uds/uds.c`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| U1 | 主循环条件 | `while (uds_running || uds_in_use > 0):1393` | 与 lwip 不同：无 socket 时可立即退出（`21` 篇） |
| U2 | 阻塞收消息 | `sef_receive_status(ANY,&m,&ipc_status):1394` | |
| U3 | 分派 A·MIB | `if (m.m_source == MIB_PROC_NR):1407` | 查询接口 |
| U4 | 分派 B·SDEV | `sockevent_process(&m, ipc_status):1410` | socket 请求（同一 `02` 框架） |
| U5 | socket 类型分流 | `switch:239` SOCK_STREAM/SOCK_SEQPACKET/SOCK_DGRAM | 本地域三型（`21`/`22` 篇） |
| U6 | 选项读写 | `SOL_SOCKET/UDSPROTO_UDS` switch `:987`/`:1080` | LOCAL_CREDS/CONNWAIT/PEEREID |

**关键事实**：lwip 与 uds **共用同一套 sockdriver/sockevent 框架**（都调 `sockevent_process`、`sockevent_init`），这是把框架篇（`01`/`02`）置于服务篇之前的因果依据。

### 1.4 框架库真序（锚点：`sockdriver.c` / `sockevent.c`）

| 库 | 入口 | 锚点 | 说明 |
|----|------|------|------|
| libsockdriver | announce/process/terminate/task | `sockdriver.c` 4 个 SEF 入口 + 19 个 `sdr_*` 回调（`01` 篇） | SDEV 请求协议机器（17 请求/6 响应，SDEV_RQ_BASE 0x1900） |
| libsockevent | `sockevent_init:2548`→注册 socket_cb；`sockevent_process:2572`→分派；`sockevent_pump:858`→续作泵；`sockevent_expire:986`/`alarm:2517`→定时 | 256 槽哈希 `sockhash_slot:48`；21 个 `sop_*` 回调表 | sock 对象生命周期 + 挂起/恢复/select |

### 1.5 序差表（运行时序 vs 教学序）

| 主题 | 运行时序事实（锚点） | 教学序选择 | 回指补偿位置 |
|------|----------------------|-----------|--------------|
| 框架库（sockdriver/sockevent） | 在 `init` 里被服务调用（L5），运行时"服务先于框架被想到" | 框架（`01`/`02`）先讲，服务（`03`+）后讲 | `03` §1 回指 `01`/`02` |
| 一次 socket 请求全生命周期 | 分散在 libc(23)→VFS(阶段5)→sockdriver(01)→sockevent(02)→alloc_socket(L23)→家族(06-11)→suspend/resume(02) | **新增集成主线篇（`25`）**，在读完框架+一个家族+libc 后收束 | `00` 阅读路径表标注 |
| MIB 查询（L20/U3） | 服务在 socket 家族全部注册子树后最后 `mibtree_init`（L244） | **新增查询/杂项篇（`26`）**，放在路由组之后 | `99` 常量表引用 |

**判定**：现有 `00→01→02→03→04→05→(06-12 家族)→(13-17 接口)→(18 bpf)→(19-20 路由)→(21-22 uds)→23 libc→24 port→99` 的模块序与上述真序**高度一致**，仅缺 (a) 请求生命周期集成主线、(b) MIB 查询专篇。序差本身可接受，不需要重排。

---

## 2. 知识点全集（存量池）

编号规则：`K-nnn` stage 内唯一。类型：概念/机制/数据结构/接口协议/约束不变量/架构演进/工具工程/测试。**来源类型全部为"存量"**（本池来自现有 26 篇；"新增"条目在 §3 覆盖审计后追加）。多 AI 汇总时以"名称 + 锚点"为对齐键。

### 2.1 总览与全局（00 / 99）

| 编号 | 名称 | 类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|------|----------|
| K-001 | 网络子系统 = 2 server + 3 框架库 + 1 libc ABI 面 | 概念 | 00、99 | `lwip.c`+`uds.c`；`libsock*`；`libc/sys/socket.c` | 知道本 stage 覆盖什么 |
| K-002 | SDEV 请求常量族 SDEV_RQ_BASE 0x1900 / SDEV_RS_BASE 0x1980 | 接口协议 | 99、01 | `minix3/include/minix/sdev.h` | 查请求码 |
| K-003 | sockid 命名空间 TCP 0x0/UDP 0x100000/RAW 0x200000/RT 0x400000/LNK 0x800000 | 数据结构 | 99、03 | `alloc_socket`、sockid 宏 | 理解 socket id 高位编码域 |
| K-004 | 端点约定（endpoint / user_endpt 如何随请求携带） | 概念 | 99 | `alloc_socket(...,endpoint_t user_endpt)` | 理解回信目标 |
| K-005 | 跨阶段边界（VFS 服务端在阶段 5、netdriver 框架+网卡驱动在阶段 16、edge E-DMABUF） | 约束不变量 | 99、00 | `16-stage-drivers/00`、`05-stage-vfs/24` | 知道哪些不在本 stage |

### 2.2 框架库（01 sockdriver / 02 sockevent）

| 编号 | 名称 | 类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|------|----------|
| K-010 | SDEV 协议：17 请求 / 6 响应，逐请求语义 | 接口协议 | 01 | `sockdriver.c` | 知道 socket 设备层的消息目录 |
| K-011 | 8 挂起 / 8 立即 / 1 特殊（CANCEL）分类规则 | 约束不变量 | 01 | `sockdriver.c` | 判断哪些请求可 suspend |
| K-012 | copy 方向（请求/响应的用户↔服务拷贝边界） | 机制 | 01 | `sockdriver.c` sdr_* | 理解数据搬运责任 |
| K-013 | 4 个 SEF 入口 announce/process/terminate/task | 机制 | 01 | `sockdriver.c:47/1061/1120/1132` | 框架生命周期切点 |
| K-014 | 19 个 `sdr_*` 服务回调表（服务需实现的钩子） | 接口协议 | 01 | `sockdriver.c` | 知道实现一个 sock 服务要填什么 |
| K-015 | struct sock 核心对象 + 256 槽哈希 `(id+(id>>16))%256` | 数据结构 | 02 | `sockevent.c:36-99` | 定位 socket 状态 |
| K-016 | 21 个 `sop_*` 操作回调表（connect/bind/... 家族实现面） | 接口协议 | 02 | `sockevent.c` | 家族如何注册操作 |
| K-017 | 续作池 continuation + select 机制 | 机制 | 02 | `sockevent.c` | 挂起请求怎么恢复 |
| K-018 | 事件泵 `sockevent_pump` + 定时器 expire/alarm | 机制 | 02 | `sockevent.c:858/986/2517` | 服务如何驱动非阻塞事件 |
| K-019 | suspend/unsuspend/resume + has_suspended 语义 | 机制 | 02 | `sockevent.c:434/453/480` | 一次挂起-唤醒闭环 |
| K-020 | sockevent 与 sockdriver 的分工（谁管消息、谁管对象） | 概念 | 01、02 | 两文件对照 | 框架分层 |

### 2.3 lwIP 服务启动与设施（03 / 04 / 05）

| 编号 | 名称 | 类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|------|----------|
| K-030 | 服务 13 段（20 离散调用）启动链 | 机制 | 03 | `lwip.c:195-263` | 初始化顺序与依赖 |
| K-031 | 4 路主循环分派（notify/MIB/SDEV+CDEV/NDEV） | 机制 | 03 | `lwip.c:301-379` | 消息去向总图 |
| K-032 | alloc_socket 域分流（含 RAW 的 root 门） | 机制 | 03 | `lwip.c:151-190` | socket 落到哪个家族 |
| K-033 | 无状态重启（不设 _restart 回调） | 约束不变量 | 03 | `lwip.c:274-285` | 为何重启用新端点 |
| K-034 | lwIP 定时器：sys_now 打标志 + 每轮检查 | 机制 | 03 | `lwip.c:26/99/315` | 栈无主动通知的补偿 |
| K-035 | pbuf 定制内存池（512B 切片，slab 增长上限 64 ≈17MB） | 数据结构 | 04 | `mempool.c` | 缓冲从何而来 |
| K-036 | 报文链工具（pchain.c 并入 mempool） | 机制 | 04 | `pchain.c` | 链式缓冲操作 |
| K-037 | 栈错误↔Minix errno 双射（16 项表） | 接口协议 | 05 | `util.c` | 错误映射底线 |
| K-038 | sockaddr 解析（v4/v6、长度校验） | 机制 | 05 | `addr.c` | 地址入参处理 |
| K-039 | 源地址选择 addrpol（RFC6724 9 行策略表） | 机制 | 05 | `addrpol.c` | 选源地址规则 |
| K-040 | timeval↔ticks 换算 + util_is_root | 工具工程 | 05 | `util.c` | 时间与权限小工具 |

### 2.4 socket 家族（06–12，并行组）

| 编号 | 名称 | 类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|------|----------|
| K-050 | ipsock IP 公共层（地址族、选项边界、src/dst 校验） | 机制 | 06 | `ipsock.c` | v4/v6 socket 共用逻辑 |
| K-051 | pktsock 包公共层（容量门、默认缓冲、头部标志） | 机制 | 07 | `pktsock.c` | 收发帧的公共底座 |
| K-052 | TCP 5 个连接标志 + 缓冲边界 | 约束不变量 | 08 | `tcpsock.c` | TCP 状态选项 |
| K-053 | 破裂管道 broken pipe + close 语义 | 机制 | 08 | `tcpsock.c` | SIGPIPE/关闭路径 |
| K-054 | ISN 生成（SHA256 + tcpisn.c） | 机制 | 08 | `tcpisn.c` | 初始序列号来源 |
| K-055 | UDP 协议白名单 + 发送标志 + 组播默认 | 约束不变量 | 09 | `udpsock.c` | UDP 特有条款 |
| K-056 | RAW 0-255 协议范围 + root 门 + v6 ICMP 校验和 | 约束不变量 | 10 | `rawsock.c` | 原始套接字权限 |
| K-057 | AF_LINK lnksock（type=DGRAM/protocol=0/容量 4）+ lldata | 接口协议 | 11 | `lnksock.c`/`lldata.c` | 链路层 socket |
| K-058 | 组播限额：全局 128(64+64)、每 socket 8；join 检查顺序 | 约束不变量 | 12 | `mcast.c`、`lwipopts.h:211/538` | 组播上限 |
| K-059 | 家族差异表（以 TCP 为代表，其余按差异收束） | 概念 | 06-12 | 各 sock | 家族间对照 |

### 2.5 设备与接口层（13–17）

| 编号 | 名称 | 类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|------|----------|
| K-070 | NDEV 消费侧：8 槽、队列 2/2/8/40、active=发送深度>0 | 数据结构 | 13 | `ndev.c` | 服务如何驱动网卡设备 |
| K-071 | 接口对象 ifdev：16 操作表 + 硬件地址 3 槽 | 接口协议 | 14 | `ifdev.c` | 一个网络接口的抽象 |
| K-072 | 回环 loopif | 机制 | 14 | `loopif.c` | lo0 特殊路径 |
| K-073 | 以太网 ethif：MTU 1500、组播容量 8、发送预留 8 | 约束不变量 | 15 | `ethif.c` | 以太接口参数 |
| K-074 | IPv6 地址标志 + scope-then-label 选择 | 机制 | 16 | `ifaddr.c` | v6 地址优先级 |
| K-075 | 默认配置 ifconf：lo0 + 8 类 ioctl + 2 MINIX 扩展 | 机制 | 17 | `ifconf.c` | 开机接口装配 |

### 2.6 特殊设备与路由（18–20）

| 编号 | 名称 | 类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|------|----------|
| K-090 | BPF 设备 /dev/bpf + bpf_filter 解释器（缓冲钳位/对齐、512 指令限、版本 1.1） | 机制 | 18 | `bpfdev.c`/`bpf_filter.c` | 抓包过滤器 |
| K-091 | 路由树 rttree 前缀假设 + 3 个位公式 | 数据结构 | 19 | `rttree.c` | 路由查找结构 |
| K-092 | 弱符号路由覆盖 + 网关钩子 | 机制 | 19 | `route.c`、`lwiphooks.h` | 服务如何改写栈选路 |
| K-093 | PF_ROUTE rtsock：先版本检查、发 512、收 0-65536、地址隔离 | 接口协议 | 20 | `rtsock.c` | 路由 socket |

### 2.7 UDS 本地域服务（21–22）

| 编号 | 名称 | 类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|------|----------|
| K-110 | uds 256 对象 + 64 哈希 + 5 状态机 | 数据结构 | 21 | `uds.c` | 本地 socket 状态 |
| K-111 | LOCAL_CONNWAIT 语义 | 约束不变量 | 21 | `uds.c:1038/1101` | accept 何时返回 |
| K-112 | uds 主循环存活条件（uds_in_use，无 socket 即退） | 机制 | 21 | `uds.c:1393` | 与 lwip 的差异 |
| K-113 | 单接收缓冲 32768 + 环形取余/饱和算术 | 数据结构 | 22 | `io.c`、`uds.h:33-36` | 本地数据面无发送缓冲 |
| K-114 | 4 段类型 + 附带数据 4096 上限 + 凭据/描述符 | 机制 | 22 | `io.c` | 传递 fd 与凭据 |
| K-115 | 三类型边界语义（STREAM/SEQPACKET/DGRAM 读取差异） | 概念 | 22 | `io.c` | 消息边界 |

### 2.8 用户态与移植面（23–24）

| 编号 | 名称 | 类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|------|----------|
| K-130 | libc 15 调用主路径（构造消息→一次陷入） | 接口协议 | 23 | `libc/sys/socket.c:44-55` | 用户如何发起 |
| K-131 | SOCK_CLOEXEC/NONBLOCK/SIGPIPE 三位逐位映射 | 机制 | 23 | `_socket_flags` | 类型标志→打开标志 |
| K-132 | [ARCH N-2] 旧式回退只在 EAFNOSUPPORT/ENOSYS 触发，重写后丢弃 | 架构演进 | 23 | `socket.c` 回退段 | 遗留路径的处置 |
| K-133 | liblwip 编译子集 68 文件 58232 行 | 工具工程 | 24 | `dist/src`、`Makefile` | 移植面规模 |
| K-134 | 关键选项（单线程/池 0/MSS 1460/窗口 16384/发送 11×MSS） | 约束不变量 | 24 | `lwipopts.h` | 内存与吞吐契约 |
| K-135 | 4 钩子（ISN/v4 路由/v6 路由/网关） | 接口协议 | 24 | `lwiphooks.h` | 栈留给服务的影响点 |
| K-136 | 4 补丁（弱符号/转发/通告忽略/大分配避免） | 工具工程 | 24 | `patches/` | 上游改动 |
| K-137 | [ARCH N-1] smoltcp 一族 + Stack/StackHooks trait 墙 | 架构演进 | 24 | `lwip_port.rs` | 替代栈裁决 |

### 2.9 架构演进条目（N-1…N-14，横切）

| 编号 | 名称 | 类型 | 现有位置 | 读者收益 |
|------|------|------|----------|----------|
| K-150 | N-1..N-14 清单（N-1 替代栈、N-2 回退丢弃、N-6 SockId newtype 等） | 架构演进 | plan §4、各篇 [ARCH] 标记 | 知道哪些是有意偏离 |

### 2.10 池统计

- 存量知识点：约 **70 条**（K-001…K-150，按主题编号段分布）。
- 按现有文档分布：00/99 各 5、01/02 各 5-6、03-05 各 5-6、06-12 家族 10、13-17 接口 6、18-20 路由/BPF 4、21-22 uds 6、23-24 8、ARCH 1（横切）。
- 主讲述点冲突预判：sockid 命名空间（03 vs 99）、错误双射（05 vs 99）、框架分工（01 vs 02）——见 §3 重复主题表。

---

## 3. 覆盖审计

### 3.1 主题全集来源（四路对账）

1. **C 源码符号**：30 个 `.c` 全部映射到篇（§0.3 glob + `Makefile` SRCS 逐条核对）。
2. **操作系统通用概念**：socket 抽象、事件循环、非阻塞挂起/唤醒、地址选择策略、缓冲池、抓包过滤、路由 FIB、fd 传递——均有篇承载或列于缺口。
3. **非 C 制品**：Makefile 依赖图、lwip.conf 服务声明、liblwip 胶水/补丁——见 §3.4。
4. **阶段边界契约**：VFS↔SDEV（阶段 5）、netdriver↔NDEV（阶段 16）、MIB↔rmib（阶段 10）。

### 3.2 覆盖缺口表

| 缺口 | 主题 | C 锚点 | 现状 | 建议 |
|------|------|--------|------|------|
| **G1** | **MIB 查询接口**：各模块注册子树、`rmib_process`、v4/v6/tcp/udp 统计口径 | `mibtree.c`(141)、`lwip.c:347`、`uds.c:1407` | 仅 03/21/99 顺带提及，**无专篇**；服务型明确列有"查询与杂项"类目 | **新建 `26` 篇**（新增知识点 K-160~K-163） |
| **G2** | **一次 socket 请求端到端生命周期**：libc→VFS→SDEV→sockdriver→sockevent→alloc_socket→家族→回复/挂起/恢复 | 跨 `socket.c`/`sockdriver.c`/`sockevent.c`/`lwip.c` | 分散在 01/02/03/06-11/23，无集成主线 | **新建 `25` 篇**（收束型集成，引用既有 K-id，不新增事实） |
| G3 | 服务构建与运行配置（Makefile DPADD 依赖、lwip.conf 的 domain/ipc/KILL 声明、SIGPIPE 需要 system KILL 权限） | `net/lwip/Makefile`、`lwip.conf` | 未文档化 | **并入 `00`**（新增 K-164，一小节，非整篇） |

无"判定属于其它 stage"的新增缺口：网卡设备驱动本体、`minix-netdriver` 框架、DMA 缓冲（edge E-DMABUF）属 `16-stage-drivers`；VFS socket 服务端属 `05-stage-vfs`——本 stage 只讲消费/服务侧。

**追加入池的新增知识点**（来源类型=新增，带证据锚点）：

| 编号 | 名称 | 类型 | 锚点（C 源码/制品/理论） | 归属 |
|------|------|------|--------------------------|------|
| K-160 | MIB 子树注册机制（模块在 init 时向 mibtree 挂子树） | 机制 | `mibtree.c`；`lwip.c:240-244`（MIB 最后 init） | 26 |
| K-161 | `rmib_process` 查询分派（MIB_PROC_NR 消息处理） | 机制 | `lwip.c:347`、`uds.c:1407` | 26 |
| K-162 | 网络 MIB 统计口径（if/tcp/udp/ip 计数树） | 数据结构 | `mibtree.c` + lwip mib 头 | 26 |
| K-163 | 两服务共用 MIB 面（lwip/uds 各注册自己的子树） | 概念 | 两 `mibtree_init` 调用点 | 26 |
| K-164 | 服务构建/运行契约（DPADD 六库依赖 + lwip.conf domain/ipc/system 声明） | 工具工程 | `net/lwip/Makefile`、`lwip.conf` | 00（新小节） |

### 3.3 重复主题表

| 主题 | 重复位置 | 主讲述点（保留） | 其余处理 |
|------|----------|------------------|----------|
| sockid 命名空间（K-003） | 03、99 | **03**（alloc_socket 源头首次出现） | 99 降为常量索引表 + 引用 03 |
| 栈错误↔errno 双射（K-037） | 05、99 | **05**（util.c 首讲） | 99 引用 05 |
| 框架分工（K-020） | 01、02 | **01**（sockdriver 先讲，含对照） | 02 引用 01 |
| 定时器机制（K-018/K-034） | 02、03 | **02**（框架层通用）；03 只讲 lwIP 特有补偿 | 03 引用 02 |

以上均为"首次出现即完整 + 后面只引用"的**正常索引式重复**（99 是全局索引，本就应重复列出常量），**不是概念混杂**，不需拆并，只需在 §5 契约中钉死主讲述点归属。

### 3.4 越界主题表

| 篇 | 疑似越界内容 | 正确归属 | 处置 |
|----|--------------|----------|------|
| 23 libc | VFS socket 服务端细节 | 阶段 5 | 已有"不覆盖"声明，合规 |
| 24 port | lwIP 内部协议算法 | 非本项目代码 | 已声明只讲调用面/配置面，合规 |
| 13 ndev | 网卡驱动本体 | 阶段 16 | 应确保只讲 NDEV 消费侧；契约中钉死 |

未发现真正越界展开（各篇头部"不覆盖"清单已划界）。

### 3.5 非 C 主题逐项回答（固定清单）

| 非 C 主题 | 本 stage 是否有 | 在哪讲 | 理由 |
|-----------|----------------|--------|------|
| 链接与加载 | 否 | — | 用户态服务器，加载由 RS/VM 负责（阶段 2/3） |
| 镜像与内存布局 | 否 | — | 同上 |
| 汇编入口与陷阱进入 | 否 | — | 无自有汇编入口；陷入经 libc（阶段 14/内核） |
| 启动装配 | 是 | 03（服务 init 链）+ 00 新小节（conf） | SEF 启动属服务侧 |
| 构建与工具链 | 是 | **00 新小节（K-164）** | Makefile 依赖 + lwip.conf |
| 跨模块接口与线格式 | 是 | 01/02（SDEV 协议）、99（常量）、24（栈墙 trait） | 已覆盖 |
| 错误路径 | 是 | 05（双射）、各篇第 4 节 | 已覆盖 |
| 关闭与退出 | 是 | 08（close/broken pipe）、21（uds_in_use 退出） | 已覆盖 |
| 并发与同步 | 部分 | 02（单线程事件循环续作/select） | 服务器单线程，无 SMP；已在框架篇 |
| 测试基建 | 是 | 各篇第 5 节 + `os/net/*/` 宿主机测试 | 分散于各篇，无需专篇 |

---

## 4. 新目录

### 4.1 核心裁决：**保留编号脊柱，只增不改**

现有 26 篇的模块序与 §1 运行时真序高度一致（序差表 §1.5），且三篇关键约束同时成立：

1. **文档不臃肿**：全部 84–256 行，无 01-stage-kernel/06 那种 2192 行概念混杂病症（体检见 `06-todo.md §3.2`），**没有"必须拆"的对象**。
2. **断链成本压倒性**：205 个外部文件引用 `17-stage-net`，内部对 `NN-xxx.md` 引用密集（03 被引 26 次）。重编号将触发 §二"历史教训 I-14"——收益（微调顺序）远小于代价（205 文件 + 数百内部引用迁移）。
3. **真问题在缺不在序**：审计（§3）发现的是**两个覆盖缺口**（G1 MIB、G2 生命周期主线），不是顺序错误。

因此新目录 = **现有 00–24、99 的编号/边界全部保留** + **新增 2 篇补口** + **1 处小节并入（00）**。这是对"读者最友好 + 成本可控"的最优解，不是保守——它精确修掉了审计发现的真实缺陷，且不制造断链。

### 4.2 新目录总表

| 编号 | 标题 | 一句话定位 | 分组 | 变化 |
|------|------|-----------|------|------|
| 00 | net-overview | 子系统全景 + 阅读路径 + 构建/服务契约 | 定位与框架 | **改**（加 K-164 小节 + 更新阅读路径表含 25/26） |
| 01 | sockdriver-framework | SDEV 请求协议机器 | 定位与框架 | 保留 |
| 02 | sockevent-framework | sock 对象与事件泵框架 | 定位与框架 | 保留 |
| 03 | lwip-main-init | 服务诞生与 4 路分派 | lwIP 启动与设施 | 保留 |
| 04 | lwip-mempool | 缓冲池与报文链 | lwIP 启动与设施 | 保留 |
| 05 | lwip-util-addr | 错误双射/地址/策略工具 | lwIP 启动与设施 | 保留 |
| 06–12 | ip/pkt/tcp/udp/raw/lnk/mcast | socket 家族（并行组，TCP 为代表） | socket 家族 | 保留 |
| 13–17 | ndev/ifdev/ethif/ifaddr/ifconf | 设备与接口层 | 设备接口 | 保留 |
| 18 | bpfdev | 抓包过滤器设备 | 特殊设备与路由 | 保留 |
| 19–20 | route/rtsock | 路由树与路由 socket | 特殊设备与路由 | 保留 |
| **26** | **lwip-mib-query** | **MIB 查询接口（两服务共用）** | 特殊设备与路由（路由后） | **新增** |
| 21–22 | uds-core/uds-io | 本地域服务 | UDS 服务 | 保留 |
| 23 | libc-socket | 用户态 15 调用封装 | 用户态与移植面 | 保留 |
| 24 | liblwip-port | 第三方栈移植面与替代裁决 | 用户态与移植面 | 保留 |
| **25** | **socket-request-lifecycle** | **一次 socket 请求端到端主线（集成收束）** | 主线集成 | **新增** |
| 99 | net-global-concepts | 常量全集索引 + 边界 | 全局 | 保留（重复项降为引用，见 §3.3） |

> 编号 25/26 取"追加高位"以避免扰动 01–24 脊柱；**阅读位置 ≠ 编号**：`00` 阅读路径表把 `25` 标在"读完 01/02/03+一个家族+23 之后"、`26` 标在"19/20 之后"。B 相按阅读路径写，文件名按编号落。

### 4.3 阅读路径

- **主线（服务型脊柱）**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08（代表家族讲透）→ **25（用一条完整请求把所有模块串起来）**。
- **支线（按需/并列）**：09–12（其余家族，看差异表）→ 13–17（设备接口）→ 18–20（BPF/路由）→ **26（MIB 查询）**。
- **可跳读路径**：21–22（UDS 独立服务，只需前置 01/02）、23–24（用户态/移植面，偏工程）。
- **并行组成员**：socket 家族组 {06 ipsock, 07 pktsock 为公共层；08/09/10/11 为具体族；12 mcast 为横切}，代表成员 = **08 tcpsock**，其余以 K-059 差异表收束。

---

## 5. 每篇契约

> 契约 = B 相写正文的任务书。新增篇（25/26）给完整七要素；保留篇给"契约裁决卡"（七要素压缩为一行式，因现有正文头 already 承载这七要素，B 相只需按 §3 微调主讲述点/新增小节，不重写结构）；改动篇（00、99）给完整要素。

### 5.1 新增篇（完整契约）

#### 25-socket-request-lifecycle

- **一句话定位**：用一次真实的 `socket()`/`connect()` 请求，把 libc→VFS→sockdriver→sockevent→alloc_socket→家族 ops→回复/挂起/唤醒整条链一次讲透，让读者脑中有"主线"而非 24 个孤立模块。
- **讲什么**：K-130（libc 主路径）+ K-010/K-013（SDEV 请求进 sockdriver）+ K-019（挂起/恢复）+ K-031（服务 4 路分派里的 SDEV 分支）+ K-032（alloc_socket 域分流）+ K-015/K-016（sock 对象定位 + sop 回调）+ K-052（家族一例：TCP connect）。全部为**引用式收束**，不重述各篇细节。
- **不讲什么**：任何单模块的内部实现（交给 01/02/03/06–11/23）；MIB 查询分支（交 26）；NDEV 收发数据面（交 13）；BPF 分支（交 18）。
- **前置**：00、01、02、03、08、23（全部编号更早或已在主线读毕）。
- **后置**：99（在索引表把"生命周期各环节锚点"指向本篇）；26（MIB 是同一条循环的另一分支）。
- **事实底线**：
  - C：`libc/sys/socket.c:44-55`（陷入）→ `sockdriver.c` process 入口 → `sockevent.c:2572 sockevent_process` → `lwip.c:354 IS_SDEV_RQ`→`:355`→ `lwip.c:151 alloc_socket`→`:163 tcpsock_socket`；挂起 `sockevent.c:434/453/480`，唤醒 `:858 pump`。
  - Rust：`os/libs/minix-sys/src/socket.rs`、`os/net/lwip/src/server.rs`+`startup.rs`、`os/libs/minix-netdriver/src/service.rs`。
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-130 | libc 主路径 | 接口协议 | socket.c:44 | 链的起点 | 存量(23) |
  | K-010 | SDEV 请求码 | 接口协议 | sockdriver.c | 消息进服务 | 存量(01) |
  | K-031 | 4 路分派·SDEV 分支 | 机制 | lwip.c:354 | 请求落到处理 | 存量(03) |
  | K-032 | alloc_socket 分流 | 机制 | lwip.c:151 | 域→家族 | 存量(03) |
  | K-019 | 挂起/恢复闭环 | 机制 | sockevent.c:434-480 | 非阻塞主线 | 存量(02) |
  | K-052 | TCP connect 一例 | 约束不变量 | tcpsock.c | 代表家族落地 | 存量(08) |

- **验收标准**：读者遮住所有函数名，能口述"一次非阻塞 connect 从用户态到服务再回到用户态"经过的**模块顺序**与**挂起/唤醒触发点**；每个环节能说出对应篇章号（可索引）。必须含一张端到端时序图（libc|VFS|sockdriver|sockevent|alloc|family 六道泳道）。

#### 26-lwip-mib-query

- **一句话定位**：讲清 lwip/uds 两服务如何把统计子树注册进 MIB、又如何处理来自 MIB 进程的查询消息——服务型组织里明确要求的"查询与杂项"类目。
- **讲什么**：K-160（子树注册机制）、K-161（`rmib_process` 查询分派）、K-162（网络 MIB 统计口径）、K-163（两服务共用 MIB 面）。
- **不讲什么**：各家族统计字段语义细节（属协议，随家族篇）；MIB 服务本体（阶段 10）；socket 请求分派（回指 03）。
- **前置**：03（看到 `mibtree_init` 与 `rmib_process` 分派位）、05（若统计字段涉及地址结构）。
- **后置**：99（常量/索引）。
- **事实底线**：
  - C：`lwip.c:244 mibtree_init`（在所有模块注册子树后最后初始化，L240-244 注释）、`lwip.c:347 rmib_process`、`uds.c:1407 MIB 分支`、`mibtree.c`(141 行)。
  - 制品：`lwip.conf` 的 `ipc ... mib`（授权与 MIB 通信）。
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-160 | MIB 子树注册 | 机制 | mibtree.c | 唯一专讲处 | **新增**(C 源) |
  | K-161 | rmib_process 分派 | 机制 | lwip.c:347/uds.c:1407 | 查询入口 | **新增**(C 源) |
  | K-162 | 网络统计口径 | 数据结构 | mibtree.c + mib 头 | 数据结构 | **新增**(C 源) |
  | K-163 | 两服务共用 MIB | 概念 | 两处 init/process | 跨服务事实 | **新增**(C 源) |

- **验收标准**：读者能回答"为什么 `mibtree_init` 必须排在所有 socket 模块 init 之后"（锚点 L240-244）、"一条 MIB 查询进来走哪一路分派"；含一张"模块→注册子树→MIB 查询回填"关系图。

### 5.2 改动篇（完整契约要点）

#### 00-net-overview（改）

- 在现有 84 行导航基础上：**(a)** 新增"构建与服务契约"小节承载 K-164（Makefile DPADD 六库 + `lwip.conf` domain=INET/INET6/ROUTE/LINK、ipc=vfs rs vm mib、system KILL 用于 SIGPIPE）；**(b)** 阅读路径表加入 25（主线收束位）、26（支线路由后）；**(c)** 一张全局数据流图（libc↔VFS↔[lwip|uds]↔liblwip↔NDEV↔网卡驱动阶段16）。**前置**：无；**后置**：全 stage。事实底线：`net/lwip/Makefile`、`lwip.conf`、`uds/Makefile`。验收：读者能从 00 判断任一 C 文件属于哪一篇。
- 不改编号、不改其余内容。

#### 99-net-global-concepts（改，最小）

- 按 §3.3 把 sockid（K-003）、错误双射（K-037）从"重复展开"降为"常量索引 + 引用 03/05"；新增一行索引指向 25、26。其余保留。

### 5.3 保留篇（契约裁决卡，七要素压缩）

> 现有 24 篇头部已含"讲什么/不讲什么/前置/边界"七要素且经 v1 收敛（todo 记 0 P0）。裁决 = **保留编号与结构**，仅执行下方"落地动作"。

| 篇 | 定位（保留） | 主讲述点（本篇拥有） | 落地动作（B 相） | 验收 |
|----|--------------|----------------------|------------------|------|
| 01 | SDEV 协议机器 | K-010~K-014、K-020 | 保留；§分工处补引用 02 降为回指 | 能说 17 请求 6 响应 |
| 02 | sock 对象与事件泵 | K-015~K-019 | 保留；框架分工引用 01 | 能说挂起-唤醒闭环 |
| 03 | 服务诞生与分派 | K-030~K-034、K-003 | 保留；sockid 定为**主讲述点**，99 转引用；末尾指向 25 | 能画 4 路分派 |
| 04 | 缓冲池与链 | K-035、K-036 | 保留（pchain 已并此）| 能说 512B/slab 上限 |
| 05 | 工具与地址 | K-037~K-040 | 保留；错误双射定为**主讲述点** | 能说 16 项映射 |
| 06 | IP 公共层 | K-050 | 保留 | — |
| 07 | 包公共层 | K-051 | 保留 | — |
| 08 | TCP（代表家族）| K-052~K-054 | 保留；标为家族组代表成员 | 能说 5 连接标志 |
| 09 | UDP | K-055 | 保留；引用 08 差异 | — |
| 10 | RAW | K-056 | 保留；root 门回指 03 alloc | — |
| 11 | AF_LINK | K-057 | 保留 | — |
| 12 | 组播横切 | K-058 | 保留；上限回指 24 选项 | — |
| 13 | NDEV 消费 | K-070 | 保留；**契约钉死**"只讲消费侧，驱动本体属阶段 16" | — |
| 14 | 接口对象 | K-071、K-072 | 保留 | 能说 16 操作表 |
| 15 | 以太接口 | K-073 | 保留 | — |
| 16 | v6 地址 | K-074 | 保留 | — |
| 17 | 默认配置 | K-075 | 保留 | — |
| 18 | BPF | K-090 | 保留 | 能说 512 指令限/版本 |
| 19 | 路由树 | K-091、K-092 | 保留 | — |
| 20 | 路由 socket | K-093 | 保留；末尾指向 26（查询同循环）| — |
| 21 | uds 核心 | K-110~K-112 | 保留；存活条件对照 lwip | — |
| 22 | uds 数据面 | K-113~K-115 | 保留 | 能说 32768/4096/5 |
| 23 | libc 封装 | K-130~K-132 | 保留；末尾指向 25（生命周期起点）| 能说 15 调用/3 标志 |
| 24 | 移植面 | K-133~K-137 | 保留 | 能说 68/58232/选项 |

---

## 6. 缺漏新篇（步骤 3 缺口落实）

| 缺口 | 处置 | 篇号 | 原料来源 | 验收 |
|------|------|------|----------|------|
| G1 MIB 查询 | 新建 | **26** | `mibtree.c` 全量 + `lwip.c:244/347` + `uds.c:1407` + `lwip.conf` ipc | 见 §5.1 契约 |
| G2 请求生命周期主线 | 新建 | **25** | 跨 socket.c/sockdriver.c/sockevent.c/lwip.c 引用式收束（不新增事实） | 见 §5.1 契约 |
| G3 构建/服务契约 | 并入 00 | 00 §新 | `net/lwip/Makefile`、`lwip.conf` | 00 能定位任一 C 文件归属 |

无否决项——三缺口都有 C/制品证据且属本 stage，全部落实为新建或并入。

---

## 7. 锚点迁移与断链成本

### 7.1 锚点迁移表

**因"保留编号脊柱"，01–24、99 无重编号**，迁移只发生在：新增两篇的落盘 + 00/99 的内容微调。

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 断链风险 |
|--------|--------|--------|----------|----------|
| 03 §MIB 顺带提及 | mibtree_init/rmib 一句话 | 保留一句 + 指向 26 | 引用改写 | 低（03 锚点不删） |
| 03/21 §主循环分派 | 4 路/2 路分派 | 原样；25/26 回指 | 原样保留 | 无 |
| 99 §sockid 表、§错误表 | 与 03/05 重复展开 | 降为索引 + 引用 | 精简 | 低 |
| （无）| 端到端请求链 | 新 25 | 新建 | 无（新文件） |
| （无）| MIB 查询 | 新 26 | 新建 | 无（新文件） |
| 00 §导航 | 阅读路径 | 加 25/26 行 + 构建契约小节 | 追加 | 低 |

### 7.2 引用迁移表（若未来有人重编号——本蓝图明确不做，此处仅登记账）

| 引用点 | 出现处 | 若改号需迁移 | 验证方式 |
|--------|--------|--------------|----------|
| `17-stage-net` 目录名 | 205 个外部文件（os/ 代码注释 + notes/）| 目录不renamed → 0 迁移 | `grep -rl "17-stage-net"` 前后计数一致 |
| 内部 `NN-xxx.md` | 03 被引 26×、06/02 19×… | 编号保留 → 0 迁移 | 编号不变即免 |
| `../05-stage-vfs/24-socket.md` 等跨阶段引用 | 23 等 | 不动 | — |

**结论**：本蓝图通过"不重编号"把外部 205 文件 + 内部数百引用的迁移量降为**近零**。新增 25/26 只需在 00 与 99 各加若干引用（受控的增量），不改任何既有锚点。

### 7.3 断链成本摘要

- 受影响既有引用：**0**（无重编号、无删除、无移动）。
- 新增引用：约 6–10 处（00 阅读路径 2、99 索引 2、25/26 回指若干）。
- 批量修改方式：仅"新增文件 + 编辑 00/99 两篇 + 在 03/20 各加一行指向新篇"，人工可做，无需脚本批改。

---

## 8. 验证与自检门

### 8.1 四种机械检查

1. **前向引用扫描**：新目录中，25 前置 {00,01,02,03,08,23}、26 前置 {03,05} 全部指向更早或主线已读者；99 引用 03/05 属"索引在后、事实在前"，合规。**PASS**。
2. **依赖图无环**：新增边仅为 25→{01,02,03,08,23}、26→{03}、00→{25,26}(阅读路径指引非语义依赖)。无回边成环。**PASS**。
3. **覆盖率 100%**：知识点池 §2（存量约 70 条）逐条在 §5 契约中拥有主讲述点篇；新增 K-160~K-164 均带 C/制品锚点并入契约。**PASS**。
4. **断链成本**：已算（§7），重编号成本压倒性 → 裁决不重编号。**PASS**。

### 8.2 自检门 G1–G9

| 门 | 结果 | 说明 |
|----|------|------|
| G1 C 真序逐条可核对 | ✅ | §1 全表带 `lwip.c`/`uds.c`/`sockevent.c` 行号锚点；lwip.c 全读、uds/sockevent grep 实证 |
| G2 知识点池完整（每 C 文件/制品有归属） | ✅ | 30 个 .c + Makefile/conf/胶水补丁全部映射；mibtree/pchain 明确归属（缺口 G1/并入 04） |
| G3 前向引用为零 | ✅ | 见 8.1(1) |
| G4 依赖图无环 | ✅ | 见 8.1(2) |
| G5 覆盖率 100% + 新增有锚点 | ✅ | 存量全有主讲述点；新增 K-160~164 带锚点；删除项：无 |
| G6 拆/合写清存量去向、新建写清新增来源 | ✅ | 本次无拆分/合并；两新建篇来源在 §5.1/§6 |
| G7 每篇契约七要素齐全 | ✅ | 新篇 25/26 完整七要素；改动篇 00/99 给要点；保留篇给契约裁决卡（定位/主讲述点/动作/验收，其正文已含不讲什么/前置/边界） |
| G8 迁移表覆盖变化文档 + 引用迁移 | ✅ | §7.1 覆盖 00/03/20/99 + 新篇；§7.2 登记 205 文件账 |
| G9 事实断言有锚点 | ✅ | 全文 C 断言带行号；ARCH 项带 [ARCH N-x]；无未标注推测（如有均属"待验证"级，见 8.3） |

### 8.3 结论与待裁决问题

**结论**：蓝图**完成**，可交 B 相执行。核心裁决 = **保留 01–24/99 编号与结构，新增 25（请求生命周期主线）+ 26（MIB 查询），00/99 最小改**。这是对 §一"重建而非搬移"的**诚实回应**：经代码真相核对，本 stage 的旧文档不存在概念混杂型质量缺陷（区别于 01-stage-kernel/06），真正的缺陷是**两个覆盖缺口 + 缺一条集成主线**，用增量修复优于高成本重排。

**待用户/评审裁决的 3 点**：
1. **是否接受"不重编号"**：若评审坚持理想教学序（把生命周期主线前置为 01 之前），成本 = 205 文件 + 内部数百引用迁移，需显式批准。
2. **25/26 编号方式**：现取"高位追加 + 阅读路径表标注"；若偏好"阅读位置=编号"，需接受重编号（回到问题 1）。
3. **正文级纠错**：本蓝图的覆盖/顺序结论基于 v1-landed 文档 + 入口 C 核对；各家族篇（06–12）**逐行 C 语义纠错**不在 R 相范围（todo 记 0 P0，但那是实现侧对账，非文档逐行重审）。B 相写 25/26 与微调 00/99 时，凡引用家族数字（1460/16384/128/32768 等）须按"事实底线"回 `minix3/` 复核。
