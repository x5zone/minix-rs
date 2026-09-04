# 02 — 系统信息库服务协议面：消息信封、交换格式与错误码特殊语义

> **分类**: 协议面 / wire contract
> **源码**: `minix3/minix/include/minix/com.h:597-598,613-622,1022-1030`（调用号）、`minix3/minix/include/minix/ipc.h:15,424-433,1373-1389,1548-1580`（6 种消息结构）、`minix3/sys/sys/sysctl.h:75-79,92-164,1382-1454`（名字/类型/标志/版本/元标识符/交换格式）、`minix3/minix/include/minix/sysctl.h`（全文 97 行，MINIX3 扩展）
> **说明**: 所有 handler 篇（09/13~20）的前置协议文档：信封长什么样、名字在信里怎么放、版本门、错误码方言。本文只讲"信上写了什么"，不讲"收到信后怎么办"（那是 01/06/10/12 的事）。

---

## 1 概念

### 1.1 目标读者与前置知识

面向要动手实现 MIB 某一块的读者。前置知识：01（主循环三岔路 + sysctl 六道门 verdict）、C 结构体对齐的基本想法（"字段按顺序排，必要时补空位"）。NetBSD sysctl 是什么不需要预习——把它理解成"MIB 的名字和格式方言的来源"，用到时一句话解释。

### 1.2 本章不讲什么

- 信进门之后的三岔路 verdict——那是 01 的事（本篇只给 verdict 判的**输入形状**）。
- 长名字那份内核拷贝、grant 转交——那是 06（拷贝效果）/ 12（远程转交）的事。
- 标志位在节点上的**行为**（父子矩阵、verify 回调、远程挂载）——那是 03 的事（本篇只给标志位的** wire 值**）。
- 各 handler 的数据语义——那是 09/13~20 的事。

### 1.3 为什么协议面值得独立成篇

本服务的全部对外承诺可以装进三句话：**三类请求**（系统控制问答、远端子树挂载、远端子树卸载），**两种交换格式**（节点快照 `sysctlnode`、节点描述 `sysctldesc`），**一套错误码的特殊语义**（例如缓冲区太小时返回 ENOMEM 而不是内存不足、EDONTREPLY 表示不回信的沉默标记、ERESTART 表示需重启本地处理的内部信号）。后文 22 篇里每一篇处理函数都在消费这三句话——内核子树返回时钟信息时要知道结构体怎么摆，远程挂载要知道三个授权凭证如何填，客户端（21/22）要知道消息信封怎么封。把三句话钉在一篇里，后文只引用不复述。

一次系统控制问答在消息信封层面走过的地方：

```
用户态库函数 __sysctl（21）
  │ 封包：用户到服务的系统控制请求消息（含旧缓冲区地址与长度、新数据地址与长度、名字长度、名字地址、消息内嵌名字数组八项）
  ▼
本服务主循环（01）：拆信封 → 六道检查逐项判决
  │ 判决：名字分量数 ≤ 12，名字长度 ≤ 8 则走消息内嵌名字，否则通过地址再拷贝
  ▼
底层处理函数（09/13 至 20）：读取数据 → 写入旧缓冲区 → 报告完整长度
  │ 封包：服务到用户的系统控制回复消息（仅含完整长度一项）
  ▼
用户态库：回填完整长度；若装不下则返回缓冲区太小并附带完整长度以便重试（01 §2.4）
```

远程方向（挂载与转交）的信封旅程是另一条线，终点在 12；本篇只给信封形状，旅程图在 12。

### 1.4 小结

三类请求（问答/挂载/卸载，其中挂载与卸载共用同一种信封结构）、六种消息结构（问答一对、挂载请求与挂载回复中的一半、转交请求与转交信息一对）、两种交换格式（节点快照与节点描述）、一套错误码特殊语义。记住"消息信封—交换格式—错误语义"三层，后文每篇处理函数都是"用这三层来交换数据"。

---

## 2 C 源码分析

### 2.1 调用号：三封 MIB 信 + 三个共享号

| 调用号 | 值（`com.h`） | 方向 | handler | 位置 |
|--------|--------------|------|---------|------|
| `MIB_SYSCTL` | `0x1800`（`MIB_BASE+0`，`:1026`） | 用户/libc → MIB | `mib_sysctl` | `main.c:459` |
| `MIB_REGISTER` | `0x1801`（`MIB_BASE+1`，`:1027`） | 服务 → MIB（单向） | `mib_register` | `main.c:464` |
| `MIB_DEREGISTER` | `0x1802`（`MIB_BASE+2`，`:1028`） | 服务 → MIB（单向） | `mib_deregister` | `main.c:469` |

判定宏 `IS_MIB_CALL(t)`（`com.h:1024`）：`(t & ~0xff) == MIB_BASE`——高 24 位命中 `0x1800` 即 MIB 家的信。`NR_MIB_CALLS` 是 3（`:1030`）：号空间满，无死号（与 DS 的 `DS_SNAPSHOT` 对照，见 01 §2.2）。

远程方向用**共享号**（`COMMON_RQ_BASE 0xE00`、`:597`；`COMMON_RS_BASE 0xE80`、`:598`）：

| 共享号 | 值 | 方向 | 用途 |
|--------|----|------|------|
| `COMMON_MIB_INFO` | `0xE04`（`:613`） | MIB → 服务 | 取远端子树根的名字/描述（挂载时） |
| `COMMON_MIB_CALL` | `0xE05`（`:616`） | MIB → 服务 | 转交 sysctl 请求（三 grant） |
| `COMMON_MIB_REPLY` | `0xE81`（`:622`） | 服务 → MIB | 回答上面两种（`req_id` 恒 0 回显；非 0 即 `EINVAL`，见 §2.2④） |

单向约束（M-8，`remote.c:218-224,293-297`）：挂载/卸载信若以阻塞方式（`SENDREC`）发来，直接 `ENOSYS`——回答会和 MIB→服务的请求在半路交叉，死锁；且处理完永远 `EDONTREPLY`（连成功都不回）。**能挂载的服务必须 fire-and-forget**，这是写 22（rmib 客户端）的人第一要知道的事。

### 2.2 六种消息结构（逐字段，lane 均为 4 字节）

6 个结构全是 56 字节（`_ASSERT_MSG_SIZE`），lane 全 4 字节——32 位 Minix3 的信封预算（`MESSAGE_PAYLOAD_SIZE=56`）。顺序即布局，Rust 侧原样复刻（§4）。

**① `mess_lc_mib_sysctl`**（用户 → MIB，`ipc.h:424-433`）：

| lane | 字段 | 类型 | 偏移 | 说明 |
|------|------|------|------|------|
| 0 | `oldp` | `vir_bytes` | 0 | 旧数据地址（0 = 不要旧数据） |
| 1 | `oldlen` | `size_t` | 4 | 旧槽长度 |
| 2 | `newp` | `vir_bytes` | 8 | 新数据地址 |
| 3 | `newlen` | `size_t` | 12 | 新数据长度 |
| 4 | `namelen` | `unsigned` | 16 | 名长（分量数，01 判 `0<len≤12`） |
| 5 | `namep` | `vir_bytes` | 20 | 长名字的名址（`>8` 时用） |
| 6 | `name` | `int[8]` | 24 | 信内名（`≤8` 时用，`CTL_SHORTNAME`，`ipc.h:15`） |

**② `mess_mib_lc_sysctl`**（MIB → 用户，`ipc.h:1548-1552`）：1 个 lane——`oldlen`（`size_t`，偏移 0）+ 52 字节补位。永远只报"完整长度"，成功失败都报（01 §2.4）。

**③ `mess_lsys_mib_register`**（服务 → MIB，`ipc.h:1373-1382`，挂载与卸载共用）：

| lane | 字段 | 类型 | 偏移 | 说明 |
|------|------|------|------|------|
| 0 | `root_id` | `uint32` | 0 | 远端根 id（**卸载只读它**，`remote.c:303`） |
| 1 | `flags` | `uint32` | 4 | 挂载标志 |
| 2 | `csize` | `unsigned` | 8 | 远端根槽位数 |
| 3 | `clen` | `unsigned` | 12 | 远端根孩子数 |
| 4 | `miblen` | `unsigned` | 16 | 挂载路径长（`>8` 静默丢弃，`remote.c:221-224`） |
| 5 | `mib` | `int[8]` | 20 | 挂载路径 |
| — | pad | 4B | 52 | 补位 |

**④ `mess_lsys_mib_reply`**（服务 → MIB，`ipc.h:1384-1389`）：`req_id`（`uint32`，偏移 0）+ `status`（`ssize_t`，偏移 4，**可为 `ERESTART`**）+ 48 字节补位。`req_id` 是异步预留位：MIB 发请求时恒填 0（`remote.c:344,425`，"reserved for future async support"），回信必须原样拷回 0，非 0 即 `EINVAL`（`remote.c:361-362,463-464`）；服务端（`rmib.c:1078`）照收照回。今天全同步，非 0 配对是设计了但没启用的未来——读到非 0 不要脑补"另一种配对"，那是坏信（12 的 `check_reply` 同形）。

**⑤ `mess_mib_lsys_call`**（MIB → 服务，`ipc.h:1554-1569`，转交）：`req_id` 0 / `root_id` 4 / `name_grant` 8 / `name_len` 12 / `oldp_grant` 16 / `oldp_len` 20 / `newp_grant` 24 / `newp_len` 28 / `user_endpt`（`endpoint_t`）32 / `flags` 36 / `root_ver` 40 / `tree_ver` 44 + 8 字节补位。**三个 grant + 三个长度，零数据字节**——MIB 把用户区直接转授权给服务，自己不当中转仓库（grant 机制在 06/12，`endpoint_t`/`cp_grant_id_t` 都是 4 字节）。

**⑥ `mess_mib_lsys_info`**（MIB → 服务，`ipc.h:1571-1580`，挂载时取根信息）：`req_id` 0 / `root_id` 4 / `name_grant` 8 / `name_size` 12 / `desc_grant` 16 / `desc_size` 20 + 32 字节补位。

### 2.3 交换格式：`sysctlnode` / `sysctldesc`（NetBSD VERS_1）

**`struct sysctlnode**（`sys/sys/sysctl.h:1382-1407`）：枚举（`CTL_QUERY`）与创建冲突回显（`EEXIST` + 现有节点）时拷给用户的节点快照。字段按顺序：`sysctl_flags`（`uint32`，类型+标志）/ `sysctl_num`（`int32`，节点号）/ `sysctl_name[32]`（`SYSCTL_NAMELEN`，`:76`）/ `sysctl_ver`（`uint32`，版本号）/ `__rsvd`（`uint32`）/ 联合体（孩子表 `csize/clen/child` **或** 数据指针 `data/offset` **或** 别名/立即 int/quad/bool）/ `sysctl_size` / `sysctl_func` / `sysctl_parent` / `sysctl_desc`。指针与 `size_t` 经 `__sysc_pad` 宏做 LP64 填充（`:1412-1431` 取值宏）——**64 位用户态下指针是 8 字节**，偏移量随 `_LP64` 而变，所以本篇只钉字段顺序与类型，不钉绝对偏移（钉了就是编造，模式 48 的反面教材预留位）。

**`struct sysctldesc**（`:1442-1447`）：`descr_num`（`int32`）/ `descr_ver`（`uint32`）/ `descr_len`（`uint32`，含结尾 NUL）/ `descr_str[1]`（变长起点）。数组按 `__sysc_desc_roundup` 以 4 字节对齐 packing（`:1449`），`NEXT_DESCR(d)` 跳到下一条（`:1454`）。读描述流的唯一正确姿势是 `NEXT_DESCR` 循环，不是 `sizeof` 步进——`descr_str` 变长，`sizeof` 步进必错位。

版本门：`SYSCTL_VERS_MASK 0xff000000`（`:130`）、`SYSCTL_VERS_1 0x01000000`（`:132`）、`SYSCTL_VERSION = VERS_1`（`:133`）；`mib.h:331-333` 编译期拒绝非 VERS_1 的头——**NetBSD 头超前了就编译失败**，而不是运行时错乱。类型/标志取值宏：`SYSCTL_TYPEMASK 0xf` + `SYSCTL_TYPE(x)`（`:150-151`）、`SYSCTL_FLAGMASK 0x00fffff0` + `SYSCTL_FLAGS(x)`（`:152-153`）。

### 2.4 类型 / 标志 / 可创建子集

类型（`CTLTYPE_*`，`:92-97`）：`NODE 1 / INT 2 / STRING 3 / QUAD 4 / STRUCT 5 / BOOL 6`；`LONG` 随位宽（64 位 = QUAD，`:99-103`）。

标志（`CTLFLAG_*`，`:108-125`）：`READONLY 0x0`（注意：**0 表示"无写位"，不是一位**）/ `READWRITE 0x70` / `ANYWRITE 0x80` / `PRIVATE 0x100` / `PERMANENT 0x200` / `OWNDATA 0x400` / `IMMEDIATE 0x800` / `HEX 0x1000` / `ROOT 0x2000` / `ANYNUMBER 0x4000` / `HIDDEN 0x8000` / `ALIAS 0x10000` / `MMAP 0x20000` / `OWNDESC 0x40000` / `UNSIGNED 0x80000`。其中 `ROOT/ALIAS/MMAP` 三位被 MIB 内部重命名为 `PARENT/VERIFY/REMOTE`（`mib.h:72-74`）——**只改名，不改值，也不暴露给用户态**；行为语义在 03，本篇只钉 wire 值。

用户可创建子集（`SYSCTL_USERFLAGS`，`:139-145`）：`READWRITE|ANYWRITE|PRIVATE|OWNDATA|IMMEDIATE|HEX|HIDDEN`——`PERMANENT` 不在其中（用户建的节点默认可销毁）；`ROOT/ALIAS/MMAP` 三位也不在其中（用户拿不到内部重命名位）。

### 2.5 元标识符：负数即操作码

（`sys/sys/sysctl.h:158-164`）：`CTL_EOL -1`（向量终结）/ `CTL_QUERY -2`（枚举孩子，11）/ `CTL_CREATE -3`（建节点，08）/ `CTL_CREATESYM -4`（带符号创建，**未实现 → `EOPNOTSUPP`**）/ `CTL_DESTROY -5`（删节点，08）/ `CTL_MMAP -6`（**未实现 → `EOPNOTSUPP`**）/ `CTL_DESCRIBE -7`（取描述，11）。负数天然与孩子 id（≥0）不碰撞——分发时"名字分量是负数"本身就是操作码（10）。`CREATESYM/MMAP` 的 `EOPNOTSUPP` 在 `tree.c:1376-1380`（`case CTL_CREATESYM: case CTL_MMAP: default: return EOPNOTSUPP`）——A-9 排除契约的 wire 证据。

### 2.6 顶层 id 与 MINIX3 扩展

顶层（`:169-183`）：`UNSPEC 0 / KERN 1 / VM 2 / VFS 3 / NET 4 / DEBUG 5 / HW 6 / MACHDEP 7 / USER 8 / DDB 9 / PROC 10 / VENDOR 11 / EMUL 12 / SECURITY 13`，`MAXID 14`。注意 MIB 静态树（`main.c:46-54`）只占其中 7 席：kern/vm/net/hw/user/vendor/minix——VFS/DEBUG/MACHDEP 等在 MIB 树里**没有位置**，问它们走分发查表，查不到即 `ENOENT`（10）。

MINIX3 扩展（`minix/sysctl.h`）：`CTL_MINIX 32`（`:17`，躲开 NetBSD 未来 id，是 ABI 的一部分；`:19-21` 编译期断言 `MAXID ≤ MINIX`，NetBSD 加 id 加到 32 就编译失败而不是静默碰撞）；`MINIX_TEST 0 / MINIX_MIB 1 / MINIX_PROC 2 / MINIX_LWIP 3`（`:28-31`）；test87 契约 `TEST_INT 0 … TEST_DESTROY2 11` + `SECRET_VALUE 0`（`:37-50`，15 的测试体）；`MIB_NODES 1 / MIB_OBJECTS 2 / MIB_REMOTES 3`（`:53-55`，15 的统计口）；`PROC_LIST 1 / PROC_DATA 2`（`:58-59`，20 的 ProcFS 口，布局在 20）。

### 2.7 错误方言词典（本篇只收口，行为各归其主）

| 错误 | 含义（MIB 方言） | 主篇 |
|------|-----------------|------|
| `ENOMEM` | 信封太小（部分拷贝 + 完整长度），**不是**内存不够 | 01 §2.4 |
| `EEXIST` + staged 长度 | 建节点撞车，同时拷出现有节点 | 08（`mib_create`） |
| `EDONTREPLY`（203） | 沉默哨兵：单向信的正常收尾 | 01 §2.2、12 |
| `ENOSYS` | 阻塞方式发了单向信（挂载/卸载） | 12（M-8） |
| `ERESTART`（200） | 内部信号：远端死了续走本地 / 服务请辞挂载 | 10/12（`tree.c:1410-1411`、`remote.c:473`） |
| `EOPNOTSUPP` | `CREATESYM`/`MMAP` 与 40+ 未实现槽位 | 13/14（A-9） |
| `EINVAL` 代替一切分配失败 | `strdup`/大缓冲分配失败**禁止**报 `ENOMEM`（`tree.c:684,1046` 注释 `do not return ENOMEM`） | 08/09 |
| `EPERM` | 非特权用户写大数据（`newlen+1 > SCRATCH` 不给分配） | 09（A-3） |

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | 线格式原样 32 位 | 32 位 C 结构体（lane 全 4 字节） | 6 个 `#[repr(C)]` 结构，`vir_bytes`/`size_t` 取 `u32`，各 56 字节（`message.rs:2595,2618,2655,2679,2725,2760`） | 56 字节预算塞不下 64 位地址 + `name[8]`；发送者（libc/libsys）今天就是 32 位 C。 stealth 加宽会一次性失配所有发送者——64 位用户态是 A-4 的显式 ABI 决策，不是本篇能顺手做的 |
| D2 | 语义/线分离 | `mib_sysctl` 里判断与拷贝混写 | `message.rs` 只管形状（`size_of==56` 钉死），`ipc/mib.rs` 只管 verdict（`SysctlRequest::decode` 等，`mib.rs:77`），映射（01 `map_sysctl_reply`）只判一次 | 照 `vm.rs` In/Out + `decode_message` 先例（`VmBrkIn` 从专用 union 臂解，不走 M1）；`SysctlReply` 只打包不映射——映射判两次必分叉（模式 59 的反面） |
| D3 | 单向约束进类型 | 注释 + `return EDONTREPLY` 散写 | `MountRequest::decode_register` 越界回 `Err(EDONTREPLY)`（`mib.rs:171`，`remote.c:221-224`）；`decode_deregister` 只读 `root_id`（`mib.rs:190`，`remote.c:303`） | C 的"静默丢弃"是最容易被"好心改成报错"的语义——把它写进返回类型，改的人必须先改签名 |
| D4 | 版本与上限编译期钉死 | `#error` 守卫 + 运行时 `SYSCTL_VERS` 宏 | `SYSCTL_VERSION` 常量（`sysctl.rs`）+ `const _: () = assert!(CTL_MAXID <= CTL_MINIX)`（`sysctl.rs:74`，对 `minix/sysctl.h:19-21`）+ `sysctl_vers/type/flags` 三个 `const fn` | C 用预处理器看门，Rust 用编译期断言看门——门的位置变了，看门这件事没变 |
| D5 | 三个重命名位保留 NetBSD 原名 | `mib.h` 内部 `#define PARENT ROOT` 等 | `sysctl.rs` 只收 `ROOT/ALIAS/MMAP` 原名（`sysctl.rs:188,196,198`），重命名语义留给 03 的 `tree::flag` | 重命名是 MIB 内部契约（`mib.h` 注释：可随时改，不破坏任何东西）——放进全服务共享的 `minix-types` 等于把内部事广播成 ABI |

替代方案及否决：把 `sysctlnode`/`sysctldesc` 也做成 `#[repr(C)]` Rust 结构——否决，因为指针 lane 在 64 位是 8 字节而 `__sysc_pad` 随 `_LP64` 变化，且交换发生在 11（枚举/描述序列化）；11 落地时按 VERS_1 逐字段序列化（`mib_copyout_node/desc` 的行为），比现在定一个"看着对"的结构更诚实。`minix_proc_*` 同理留给 20（A-5 一并决策）。

---

## 4 实现详解

### 4.1 模块结构

```
os/libs/minix-types/src/
├── types/sysctl.rs   — 本篇：CTL_* / CTLTYPE_* / CTLFLAG_*（NetBSD 原名）/
│                        SYSCTL_VERSION / 掩码 const fn / 元标识符 /
│                        MINIX_* / TEST_* / MIB_* / PROC_*（id 层）
├── types/com.rs      — 本篇补：COMMON_MIB_INFO/CALL/REPLY（号段，01 已有 MIB_BASE 块）
├── ipc/message.rs    — 本篇补：6 种 Mess 结构 + 6 个 union 臂（线形状）
└── ipc/mib.rs        — 本篇新：SysctlRequest/Reply / MountRequest/RegisterView /
                         RemoteReply / RelayedCall / InfoFetch（线 verdict）
```

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 顶层 id/MINIX 扩展 | `sysctl.h:169-183` + `minix/sysctl.h:17-59` | `sysctl.rs:37,70,81,94,125,137` | 值钉死 + `MAXID≤MINIX` 编译期断言 |
| 类型/版本/掩码 | `sysctl.h:92-103,130-153` | `sysctl.rs:524,532,551`（`sysctl_type/flags/vers` const fn） | 取值逻辑与 C 宏同式 |
| 标志全集 | `sysctl.h:108-125` | `sysctl.rs:481,512`（含 `SYSCTL_USERFLAGS`） | 原名保留；`USERFLAGS` 组合值同 C |
| 元标识符 | `sysctl.h:158-164` | `sysctl.rs:562` 起 | 七个负数 |
| 6 种线结构 | `ipc.h:424-433,1373-1389,1548-1580` | `message.rs:2595,2618,2655,2679,2725,2760` + union 臂 `:198-208` | `size_of==56` 全钉 |
| 请求 verdict | `main.c:302-340` | `mib.rs:77`（`SysctlRequest::decode`） | 长度/取道/配对，与 01 同判 |
| 回复打包 | `ipc.h:1548-1552` | `mib.rs:130`（`SysctlReply::encode`） | 只打包，不映射 |
| 挂载 verdict | `remote.c:221-224,303` | `mib.rs:171,190` | 越界静默丢；卸载只读 root |
| 回信路由 | `remote.c:359-364,461-464` | `mib.rs:211,222`（`RemoteReply::decode` + `is_reserved_zero`） | 0 回显，非 0 拒（verdict 在 12 `check_reply`） |

### 4.3 不变量

| 不变量 | 守卫 | 证据 |
|--------|------|------|
| 线结构恒 56 字节 | `size_of` 断言 ×6 | `ipc.h` `_ASSERT_MSG_SIZE` ×6 |
| 号空间满且无死号 | `MIB_*` 测试 + `NR_MIB_CALLS=3`（01） | `com.h:1026-1030` |
| 越界挂载静默丢 | `decode_register → Err(EDONTREPLY)` | `remote.c:221-224` |
| 映射只判一次 | `SysctlReply` 无映射逻辑 | 01 `map_sysctl_reply` 唯一 |
| 版本钉死 VERS_1 | `SYSCTL_VERSION` + 类型测试 | `sysctl.h:133` + `mib.h:331-333` |

### 4.4 与 C 的差异说明（模式 72 CSSCM）

§2 七节 vs §4 四模块：节多是因为 C 的"协议面"散在四个头文件（com/ipc/sys sysctl/minix sysctl），Rust 按"值表（sysctl.rs）/形状（message.rs）/ verdict（mib.rs）/号段（com.rs）"四模块收拢——**收拢是文件组织差异，语义零差异**：常量值、字段顺序、lane 宽度、错误码逐一同 C。唯一-category 为设计决策（D1 线宽保持 32 位，A-4 未来演进时三处标注）。

---

## 5 测试要点

> 基线：`cargo test -p minix-types --lib`（workspace 根在 `os/`），本篇 15 个测试（7 sysctl + 6 mib + 1 layout + 1 common）。

| 测试名 | 覆盖 C 位置 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_top_level_ids` | `sysctl.h:169-183` + `minix/sysctl.h:17` | 15 个顶层值 + `MAXID≤MINIX` | `sysctl.rs` |
| `test_minix_subtree_ids` | `minix/sysctl.h:28-59` | MINIX/TEST/MIB/PROC 四组 id | `sysctl.rs` |
| `test_kern_ids` | `sysctl.h:194-278`（接缝抽查） | KERN 表接缝值（全表钉在 13） | `sysctl.rs` |
| `test_vm_hw_ids` | `uvm_param.h` + `sysctl.h:893-909`（接缝抽查） | VM/HW 表接缝值（全表钉在 14） | `sysctl.rs` |
| `test_types_versions_limits` | `sysctl.h:75-79,92-103,130-134` | 限/类型/版本/三取值函数 | `sysctl.rs` |
| `test_flag_bits_and_userflags` | `sysctl.h:108-125,139-145` | 15 标志位 + USERFLAGS 组合 + 重命名位与类型半字节正交 | `sysctl.rs` |
| `test_meta_identifiers` | `sysctl.h:158-164` | 七负数 | `sysctl.rs` |
| `test_mib_wire_layouts` | `ipc.h` 六处 + `_ASSERT` | 六结构 56 字节 + 字段顺序（名/挂载/转交各一例） | `message.rs` |
| `test_sysctl_decode_bounds` | `main.c:302-303` | 0 拒、13 拒 | `mib.rs` |
| `test_sysctl_name_paths` | `main.c:309-315` | 3 信内、9 取址 | `mib.rs` |
| `test_sysctl_pairing_rules` | `main.c:322-340` | old 原谅 + new 半份三组合 + 全份 | `mib.rs` |
| `test_sysctl_reply_encode` | `ipc.h:1548-1552` | 打包 + 56 字节 | `mib.rs` |
| `test_mount_register_bound` | `remote.c:221-224,303` | 8 过、9 静默丢、卸载只读 root | `mib.rs` |
| `test_remote_reply_routing` | `remote.c:359-364,461-464` | 0 回显过、41 非零拒（`ERESTART=200` 值直通） | `mib.rs` |
| `test_common_mib_messages` | `com.h:597-598,613-622` | 三共享号（基址复用 `ipc::event`） | `com.rs` |

测试策略：值表用"抄头文件"式全量断言（改一个数就红）；verdict 用边界值（0/8/9/12/13）；挂载用"过线即沉默"反直觉断言。

### 5.1 测试统计（截至 2026-09-05）

- `cargo test -p minix-types --lib`：**139 passed**（全 crate；其中本篇 15 个，见上表）
- 完整清单：`rg "fn test_" os/libs/minix-types/src/{types/sysctl.rs,ipc/mib.rs}` + `test_mib_wire_layouts`（`ipc/message.rs`）+ `test_common_mib_messages`（`types/com.rs`，`test_mib_messages` 归 01）

---

## 6 过渡

信封钉完了：三种信、六种结构、两种交换格式、一本方言。下一站是 03——树本身：`mib_node` 长什么样、四类节点矩阵、标志位的行为语义（本篇钉的 `ROOT/ALIAS/MMAP` 值在那里第一次被"使用"）。

## 7 参见

- C 源：`minix3/minix/include/minix/com.h:597-598,613-622,1022-1030`、`minix3/minix/include/minix/ipc.h:15,424-433,1373-1389,1548-1580`、`minix3/sys/sys/sysctl.h:75-79,92-164,1382-1454`、`minix3/minix/include/minix/sysctl.h`（97 行全文）、`minix3/minix/servers/mib/main.c:291-351`（解码 verdict）、`minix3/minix/servers/mib/remote.c:218-224,293-310,361-364`（单向约束与回信路由）、`minix3/minix/servers/mib/tree.c:1376-1380,1410-1411`（EOPNOTSUPP/ERESTART）
- 阶段文档：`01-mib-init-main.md`（上一站，verdict 层）、`03-mib-node-model.md`（下一站，标志行为）、`12-mib-remote-subtrees.md`（单向约束的行为主篇）、`21-mib-client-libc.md`（信封的另一端）、`../07-stage-ds/02-ds-message-contract.md`（同形状先例：号段先行 + 载荷后补）
- Rust 实现：`os/libs/minix-types/src/types/sysctl.rs`、`os/libs/minix-types/src/types/com.rs`（COMMON_MIB 段）、`os/libs/minix-types/src/ipc/message.rs`（6 Mess + 6 union 臂）、`os/libs/minix-types/src/ipc/mib.rs`
- 对端：`../01-stage-kernel/12-ipc-core.md`（`SENDREC` vs 单向语义）
