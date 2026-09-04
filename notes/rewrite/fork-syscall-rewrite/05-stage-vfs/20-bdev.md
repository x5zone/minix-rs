# 20 — bdev：块驱动直达、重试熔断、死信分流与换人通告

本文讲清块设备层如何在“直达驱动、重试五次、死信分流、三验回复、换人通告”的五段对话中，以驱动端点为唯一对端，把打开/关闭/控制直接送到驱动进程，并在驱动更替时把旧打开重开、把挂载树通告、把根兜底——读写本身不在此，它经文件系统走另一条路。

前置阅读：`19-device-map.md`（dmap 查表与恢复 verdict）、`16-read-write.md`（`bsf` 锁语义与五路分派）、`06-vmnt-table.md`（挂载槽语义）。

> 本章不讲什么：
> - 块读写的 FS 路由（`req_breadwrite` 经挂载 FS）—— `12-request-wrappers.md`
> - `drv_sendrec` 的传输执行—— 内核侧（本篇只给重试与分类）
> - `req_newdriver` 的协议执行—— `12-request-wrappers.md`
> - `worker_signal` 的调度唤醒执行—— `08-worker-thread.md`
> - 授权（grant）的内核侧实现—— `99-global-concepts.md`
> - 字符/socket 驱动的对话—— `21-cdev.md` / `22-sdev.md`

---

## 1 概念

### 1.1 为什么块走直达

块设备的读写经文件系统（FS 缓存、预读、块分配都在 FS），但打开、关闭、控制不经过 FS——它们问的是驱动本人（“这 minor 号你认吗”“把灯闪一下”），FS 答不上来。直达因此不是捷径，而是管辖：谁拥有状态，谁回答问题。`bdev.c:1-9` 的头注释把分叉写在了第一段：读写走挂载的 FS，开/关/控直达驱动。

直达的代价是 VFS 必须自己做传输的可靠性：重试、死信、唤醒，FS 本会代劳的三件事，在此一概自理。本篇的全部机制就是这份自理的账单。

### 1.2 重试的五次封顶

驱动可能要求“重来”（`ERESTART`：驱动重启了，刚才那句不算）。VFS 的回答是恢复原报文再发，最多五次——五次还重启，说明对端不是在重启而是在抽搐，熔断报 `EIO`。五是拍脑袋的数吗？是工程权衡：太少则正常重启序列（停→起→重放）走不完，太多则把调用线程钉死在等待上。封顶的本质是“给恢复留时间，不给抽搐续命”。

### 1.3 死信的三分类

发信失败分三种：对端死了（`EDEADSRCDST/EDEADEPT`——清表，广而告之）、对端锁死了（`ELOCKED`——记一笔，报 `EIO`）、其他（不可能，按最坏处理）。死亡与死锁的区别在于后续动作：死亡要清通讯录（19 的 `dmap_unmap_by_endpt`），死锁只需报错——锁死是 transient，下次也许就好；死亡是 permanent，不清表则下次还撞墙。

第三类的 panic 改错误是本篇唯一的语义加固：C 认为“其他失败不可能”，Rust 认为“不可能的事也要有出口”。出口是 `EIO`——直达路径的上层只认 `OK` 与错误，没有第三个槽位。

### 1.4 回复的三重门

驱动的回复要过三验才认领：发信人是否在册（查表命中）、该行是否有人在服（`servicing` 有效）、等的人是否等的正是他（任务号与槽位双合）。三验缺一即丢弃并打印——丢弃不是冷漠，是精确：错领的回复比丢弃更糟，它会唤醒等别人的线程。

“绝不阻塞调用线程”是回复路径的铁律（`bdev.c:190` 的 MUST NOT）。铁律的根因是回复来自中断上下文的下游——阻塞即把驱动的门堵死，后续回复全卡住。纯谓词天然满足铁律：判定不等待任何人。

### 1.5 换人的两轮通知

驱动换人（新驱动映射进来）要通知两拨：旧打开（逐个 filp 重开——新官认旧印，每个打开重办一次）与挂载树（逐个 vmnt 通告新端点）。重开失败即全弃：第一个重开不成就收手——半新半旧的打开表比全旧更糟。通告失败则继续：单个 FS 听不见不阻断其他 FS，尽力而为。

根兜底是最后的慷慨：只要有过任何打开，就额外通告根 FS 一次，宁滥勿缺——注释承认这是“懒得精确”（`bdev.c:279-280`），懒得精确但永远正确：多通告一次无害，少通告一次则根的块路由悬空。

### 1.6 守卫的设调清

块 ioctl 全程举着守卫（`filp_ioctl_fp` 的设→调→清）：守卫声明“此次调用正在占用该 filp”，防的是 ioctl 与读写在同一 filp 上的交错。守卫是时序义务而非状态——设与清之间必须恰好一次调用，多一次是泄漏，少一次是悬空。19 已立此义务的常量，本篇只消费不重复。

### 1.7 与其他 OS 的块直达对照

- **Linux** 以 `blk-mq`（请求队列 + 标签 + 超时重试）实现同构对话：`bdev_sendrec` 的重试循环对应块层的 `blk_mq_complete_request` 重发，`EDEADSRCDST` 清表对应 `del_gendisk` 的失效路径，`bdev_up` 的重开对应驱动重绑定（rebind）后的 `__blkdev_get` 重取。
- **Redox** 的 AHCI scheme 以 `BlockScheme::handle_packet` 同步往返对应直达：重试与超时由 scheme 内计数器实现；Redox 以 `Result` 表达死信，VFS 以 `SendFault` 三分类表达——同构不同名。
- **seL4** 无块抽象，驱动是持有 DMA 与中断能力的用户进程，直达即普通 IPC；VFS 的重试/死信/唤醒三件套在 seL4 中由客户端库复刻——状态服务器把可靠性做进服务里，无状态内核把它留给客户端。

### 1.8 小结

块直达是五段对话（解析→传输→重试→回复→换人），三组开关（major 有效/驱动在册/三验全过）决定成败，一条不变量贯穿始终：每句发出去的话都有着落——成功有状态，重启有重发，死亡有清表，死锁有记录，未知有 `EIO`。没有悬空的消息，是本篇的全部要求。

---

## 2 C 源码分析

### 2.1 `bdev_sendrec` 重试机（`bdev.c:33-73`）

类型断言开场（`39`，`IS_BDEV_RQ` 见 `minix3/minix/include/minix/com.h:966`）→ 原报文留存（`40`）→ 循环发送（`43-54`：传输失败直返，`ERESTART` 恢复重发计数，五次封顶）→ 熔断 `EIO`（`57-58`）→ 死信分流（`60-70`：死亡清表 `EIO`、死锁打印 `EIO`、余 panic）→ `OK`（`72`）。

### 2.2 `bdev_open/close` 对偶机（`bdev.c:78-138`）

调用约定：本文件不碰 `bsf` 锁（`lock_bsf` 归 16）——调用方（16 的块分支、15 的块打开）已持锁进入，直达只管对话不管串行（bsf 缓存语义归 16，本篇为边界引用）。

两函数同构：拆 major/minor（`86-87,121-122`，编解码见 18）→ 越界/缺席双门 `ENXIO`（`88-89,123-124`）→ 组包（`96-100,127-130`：清零、`BDEV_OPEN/CLOSE` 见 `com.h:970-971`、minor/access/id 三字段）→ 发送（`103,133`）→ 失败透传（`104-105,134-135`）→ 回状态字（`107,137`）。差异仅两处：open 拼访问位（`91-93`：`R→BDEV_R_BIT`、`W→BDEV_W_BIT`，见 `com.h:982-983`），close 无访问位。

### 2.3 `bdev_ioctl` 授权机（`bdev.c:143-186`）

拆号（`153-154`）→ 查表门（`157-161`，无驱动打印 `ENXIO`）→ 造授权（`164`，19 的解码）→ 组包（`167-173`：`BDEV_IOCTL` 见 `com.h:976`、minor/request/grant/user/id 五字段）→ 发送（`176`）→ 有权即撤销（`179`，`GRANT_VALID` 见 `minix3/minix/include/minix/safecopies.h:53`）→ 失败透传（`182-183`）→ 回状态字（`185`）。

### 2.4 `bdev_reply` 三重门（`bdev.c:192-220`）

查表命中否则丢弃打印（`198-202`）→ 在服有效否则丢弃打印（`204-208`）→ 工人对号（任务号与发送槽双合，`210-215`）否则丢弃打印 → 投递报文清槽唤醒（`217-219`）。

### 2.5 `bdev_up` 换人机（`bdev.c:226-282`）

越界 panic（`235`）→ 取标签（`236`）→ filp 重开轮（`243-259`：跳过无效/无 vnode/异 major/非块，取读写位重开，失败清恢复标志全弃，成功记 found）→ vmnt 通告轮（`262-269`：major 命中即投递标签，失败打印继续）→ 有打开则根兜底（`277-281`：`ROOT_FS_E` 见 `minix3/minix/servers/vfs/glo.h:21`，`makedev(maj, 0)`，失败打印继续）。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `bdev.c` 的阻塞循环，而是吸收 Linux/Redox 的队列模型后做取舍。以下决策对应 `.design/20-design.v1.md` D1-D7。

### D1 传输 trait 化

- **C**：`drv_sendrec` 真实传输内嵌重试循环（`bdev.c:44`）。
- **Rust**：`SendTransport` trait（`ScriptedTransport` 按脚本回放 vs `DeadTransport` 常死亡）+ `transact` 泛型（`os/servers/vfs/src/bdev.rs:77,194`）。
- **为什么**：传输是唯一的不可测点；脚本化使四剧本（直达成功/重启后成功/熔断/死信）可单测。替代方案（函数指针回调）被否决：脚本下标状态自包含，函数指针需外部可变状态。

### D2 重试状态机

- **C**：`retry_count` 裸计数 + `do/while`（`bdev.c:36-54`）。
- **Rust**：`RetryState(u8)` + `step(status) -> RetryVerdict::{Done, Again, Exhausted}` + `MAX_RETRIES=5`（`os/servers/vfs/src/bdev.rs:140,42`）。
- **为什么**：循环条件与熔断收敛为一函数；原报文恢复义务留调用点（报文所有权归调用者）。替代方案（迭代器 `take(5)`）被否决：重试条件含状态字判断，非纯计数。

### D3 死信分类

- **C**：死亡清表、死锁打印、余 panic（`bdev.c:60-70`）。
- **Rust**：`classify_send(status) -> SendFault::{Dead, Locked, Fatal}`（`os/servers/vfs/src/bdev.rs:179`）；`Fatal→EIO`。
- **为什么**：三类是三种后续动作的知识；panic→`EIO` 是 ARCH 加固（直达路径无上层可接 panic）。状态码以本地常量表达（202/208/215，三处证据，注释列明）。

### D4 驱动解析与访问位

- **C**：越界/缺席双门 + 访问位拼合散在 open/close（`bdev.c:86-93,121-124`）。
- **Rust**：`resolve_driver(major_valid, driver) -> Result` + `access_bits(read, write) -> u8`（`os/servers/vfs/src/bdev.rs:216,224`）。
- **为什么**：双门是同一“有人管”知识的两面；拼合是位运算纯知识。位值 sync 树可验证，不编造。

### D5 回复三验

- **C**：三验嵌套 + 投递唤醒（`bdev.c:198-219`）。
- **Rust**：`ReplyCheck{known, servicing, worker_ok}` + `check_reply -> Result<(), ReplyIgnore>`（`os/servers/vfs/src/bdev.rs:237,260`）；投递留 09。
- **为什么**：合取谓词使 2³ 组合可测；`MUST NOT block` 天然成立（纯判定不等待）。拒因三值使丢弃可观测（C 只打印，本篇可测）。

### D6 换人谓词与中止

- **C**：重开轮遇错全弃 vs 通告轮遇错继续的不对称（`bdev.c:251-269`）。
- **Rust**：`reopen_candidate` 四元合取 + `notify_vmnt` + `root_notify` + `reopen_failed_aborts`（`os/servers/vfs/src/bdev.rs:276,281,289,295`）。
- **为什么**：两种失败语义必须显式区分；根兜底的“宁滥勿缺”以文档声明。表扫描留调用点（04/06 管辖）。

### D7 守卫义务复用

- **C**：`filp_ioctl_fp` 设→调→清（`device.c:36-40`，19 已覆盖）。
- **Rust**：复用 19 的 `BLOCK_NEEDS_GUARD`，本篇不重复常量。
- **为什么**：single-source（模式 A 规避）；本篇只消费守卫义务，不重建模。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-1 单线程事件循环（mthread→状态机） | 阻塞传输→trait 脚本 + verdict | `bdev.rs:77,194` + 本文档 D1 + 20 正文 §1.1 |
| 未知失败 panic→EIO（直达加固） | `SendFault::Fatal` | `bdev.rs:169,179` + 本文档 D3 + 20 正文 §1.3 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── bdev.rs               — 本篇：传输/重试/死信/回复/换人判定
├── device_map.rs         — DmapTable 查表 + BLOCK_NEEDS_GUARD（19）
├── mount.rs              — DevCodec 编解码对照（18）
└── read_write.rs         — bsf 锁语义（16，块分支持锁调用本篇）
```

> 设计决策：§3 D1（传输 trait 化）/ D3（死信分类）/ D5（回复三验）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `BDEV_RQ_BASE` | `com.h:963` | `bdev.rs:16` | 0x500 |
| `BDEV_OPEN/CLOSE/IOCTL` | `com.h:970-976` | `bdev.rs:19,21,23,46` | 选择子 |
| `BDEV_R/W_BIT` | `com.h:982-983` | `bdev.rs:26,28,224` | 访问位 |
| 发送状态码 | `errno.h:198-215` | `bdev.rs:34,36,39` | 分类输入 |
| 重试机 | `bdev.c:33-73` | `bdev.rs:77,87,119,140,194` | 脚本 + 熔断 + 死信 |
| 解析门 | `bdev.c:86-89` | `bdev.rs:216,224` | 双门 + 拼合 |
| 回复三验 | `bdev.c:192-220` | `bdev.rs:237,248,260` | 合取 + 拒因 |
| 换人谓词 | `bdev.c:226-282` | `bdev.rs:276,281,289,295` | 重开/通告/兜底/中止 |
| 错误族 | `bdev.c` 全文件 | `bdev.rs:304,313 BdevError::to_errno` | 2 变体→errno，无自创 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 重试封顶 | `RetryState` | 五次熔断 | `bdev.c:41-58` |
| 死信清表 | `SendFault::Dead` | 死亡才清 | `bdev.c:61-64` |
| 回复对号 | `check_reply` | 三验合取 | `bdev.c:198-215` |
| 换人中止 | `reopen_failed_aborts` | 重开败即弃 | `bdev.c:251-256` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **256 passed / 0 failed**（既有 248 + 本篇新增 8；`minix-types` 独立）。
> 本章直接影响 8 项新增。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_op_selectors` | `com.h:963-983` + `bdev.c:91-93` | 选择子 + 访问位拼合 | `bdev.rs:326` |
| `test_resolve_gate_pair` | `bdev.c:88-89,123-124` | 双门 ENXIO | `bdev.rs:339` |
| `test_retry_fuse` | `bdev.c:41-58` | 直达 + 四次又熔断 | `bdev.rs:348` |
| `test_send_fault_classes` | `bdev.c:60-70` | 三分类 + 加固 | `bdev.rs:364` |
| `test_transact_scripts` | `bdev.c:43-72` | 四剧本 + 轮次计数 | `bdev.rs:374` |
| `test_reply_triple_gate` | `bdev.c:198-215` | 三验拒因 | `bdev.rs:406` |
| `test_swap_predicates` | `bdev.c:243-281` | 重开/通告/兜底/中止 | `bdev.rs:419` |
| `test_errno_map_covers_bdev_c` | `bdev.c` 全文件 | 2 变体→errno 全映射 | `bdev.rs:438` |

测试策略：选择子以位值锁定覆盖；门以双 ENXIO 覆盖；重试以直达/四又一熔断覆盖；死信以三分类覆盖；对话以四剧本 + 轮次计数覆盖；回复以 2³ 拒因覆盖；换人以四谓词覆盖；错误以全映射覆盖。

### 5.1 测试统计（截至 2026-09-03）

- `cargo test -p minix-vfs --lib`：**256 passed / 0 failed**
- 本节列出与本模块直接相关的 8 个（子集）
- 完整测试清单：`rg "fn test_" os/servers/vfs/src/bdev.rs`

---

## 6 过渡

本篇在 19（查表）之后、21（字符执行）之前，是“直达执行”的归属层：19 只管查到谁，本篇管查到之后怎么谈（重试/死信/回复/换人）；没有本篇，查到的端点只是个数字，发出去的话没有着落。

```
19-device-map: get_dmap_by_major → 端点（查到谁）
   │
   └─► 本篇：resolve → transact（重试/死信）→ check_reply（三验）→ swap predicates（换人）
          │                        │                    │
          ├─► 09-main-loop：IS_BDEV_RS 分流与回复路由（bdev_reply 的调用点）
          ├─► 10-pm-protocol：服务启停时的映射更替（换人的上游）
          └─► 12-request-wrappers：req_newdriver 的协议执行（通告的载体）
```

阅读顺序提示：若关心“回复到了谁来收”，下一站 `09-main-loop.md`（`IS_BDEV_RS → bdev_reply` 分流）；若关心“字符设备的对话”，下一站 `21-cdev.md`（`cdev_open` 与 `cdev_map`）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/bdev.c:1-282`（`bdev_sendrec/bdev_open/bdev_close/bdev_ioctl/bdev_reply/bdev_up`）、`minix3/minix/include/minix/com.h:963-983`（`BDEV_RQ_BASE/OPEN/CLOSE/IOCTL/R/W_BIT`、`IS_BDEV_RQ`）、`minix3/sys/sys/errno.h:198-215`（`ERESTART/EDEADSRCDST/ELOCKED/EDEADEPT`）、`minix3/minix/include/minix/safecopies.h:53`（`GRANT_VALID`）
- 阶段文档：`19-device-map.md`（查表与恢复 verdict）、`16-read-write.md`（`bsf` 锁与块分支）、`09-main-loop.md`（回复分流）、`06-vmnt-table.md`（通告对象）、`04-filp-table.md`（重开对象）
- Rust 实现：`os/servers/vfs/src/bdev.rs:1`（本篇判定层）、`os/servers/vfs/src/device_map.rs:1`（查表层）、`os/servers/vfs/src/mount.rs:1`（编解码对照）、`os/libs/minix-types/src/types/errno.rs:15`（errno 值）
- 内核侧：`../01-stage-kernel/18-syscall-copy.md`（驱动消息拷贝语义）
