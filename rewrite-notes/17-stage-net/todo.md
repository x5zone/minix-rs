# 17-stage-net Rust 实现架构级 Review TODO

> **台账状态摘要**（对照代码核实于 `2026-10-08`，快照 `9f752834c`；`2026-10-09` 回勾）：未闭合 `open` 2：协议栈本体与传输未实现、缺设计条目。本行旧记的第三项"套接字标识五类基值缺 Rust 侧权威"（`N1-P1-1`）经现场核实为**账面滞后**——`os/libs/minix-netdriver/src/sockid.rs` 已落地（`2026-09-17`，与本文件 §0.3 同一条目行自洽），五个 `SOCKID_*` 基值逐个对 `minix3/minix/net/lwip/lwip.h:58-62` 有钉定测试，`cargo test -p minix-netdriver --lib sockid` → 5 passed/0 failed（核于 `2026-10-09`，快照见本轮提交）。§0.2 常量对账表里同一件事此前记的"❌ 计划外缺失"已同步回勾为 ✅。
> **账面滞后校正**：网络字节序与套接字事件类型的两份手抄已裁决归共享类型库单点权威（`PD-13`），属执行未竟。
> 跨阶段联动项的状态权威在 `../coordination/TODO-LEDGER-OPEN.md`（其 §6 给本阶段索引行）；本文件的条目描述与修法仍是权威，本轮只加本摘要不改条目。状态词按 `../coordination/TODO-LEDGER-INDEX.md` §3 的六值词表折叠。

> 来源：2026-09-17 架构级代码扫描（code-excellence cmd + 查漏补缺优先，非逐函数审查）。
> 范围：`os/net/lwip`（22 文件）+ `os/net/uds`（4 文件）+ `os/libs/minix-netdriver`（5 文件）+ `os/libs/minix-sys/src/socket.rs`（23-libc-socket）+ `os/drivers/net`（14 crate，仅边界轻扫）。与 17-stage-net 26 篇文档一一对应（00 篇导航表）。
> 方法：分层分析（整体架构 → crate → 模块 → trait/函数），每层自问"如果今天重写会怎么设计"；对照 C 真值（`minix3/minix/net/lwip` 24,477 行、`liblwip` 编译子集 58,232 行、`net/uds` 3,660 行、`libsockdriver` 1,150 行、`libsockevent` 2,590 行、libc socket 3,173 行）+ Redox netstack/smolnetd + smoltcp 0.14 + Rust 社区实践（2026-09 联网核实）。
> 定位：架构改进与查漏补缺清单。**14/14 条目全部闭环（2026-09-17）；正文归档于 archive/todo-N1-archive-2026-09-17.md，本文件保留速览与索引。**
> 历史归档：无（首建）。

---

## 0. 审查结论速览

### 0.1 总体判定

当前实现是**"policy 半边"骨架**：全部 26 篇文档映射的 .rs 文件存在且质量良好（编号/界限/状态机/哈希等纯逻辑面，126 测试全绿，clippy 零警告，零 `unsafe`、零 `todo!`），常量对账全部命中 C 真值（见 §0.2）。但协议栈本体、消息传输、硬件路径**全部未实现**——其中绝大多数是文档显式声明的分期（各模块头注释均声明 "message packing / object storage stays in the service binary"），属计划内缺失；真正的缺口是：**这些分期件至今没有设计条目和排期**（N1-P1-2、N1-P1-4），以及两个计划外缺失（N1-P1-1 sockid 命名空间、§1 wire 契约分裂挂 edge E-SDEVOWN/E-DEVWIRE）。本轮 **0 P0**。

| 分层 | Rust 现状 | C 真值 | 缺口定性 |
|------|----------|--------|---------|
| policy/校验面（编号、界限、哈希、状态枚举） | ~4,800 行，126 测试 | 分散于各 .c | ✅ 已覆盖，常量全对上 |
| sockdriver 挂起/续延 + sockevent 对象/定时器（C 3,740 行） | 仅命名与可挂起表（sdev.rs 162 行 + sockevent.rs 102 行） | sockdriver.c + sockevent.c | 计划内分期，**无设计条目**（N1-P1-2） |
| 消息传输 + 主循环 + SEF/RS 启动 | main.rs 均为 `loop {}` 占位（main.rs:4） | lwip.c main + uds.c main | 计划内分期，**无设计条目**（N1-P1-4，启动挂 edge E-NETSTART） |
| 协议栈本体 [ARCH N-1] | 仅契约常量记录（lwip_port.rs） | liblwip 68 .c / 58,232 行 | 选型决策迫近（N1-P1-3） |
| 缓冲/pbuf 等价物 | 仅常量（PBUF_POOL_SIZE=0 契约等） | pbuf 定制池 | 无设计条目（N1-P1-5） |
| NIC 驱动数据路径 | 9/14 纯 stub，5 个仅环形算术 | drivers/net 22,712 行 | 归 16-stage（§6 边界节） |

### 0.2 常量对账 Gate（本轮通过，零偏差）

| 对账项 | C 真值 | Rust 落点 | 结果 |
|--------|--------|----------|------|
| SDEV 17 请求 / 6 回复编号、IS_SDEV 守卫、OP_RD/WR/ERR/NOTIFY 位 | `com.h:1037-1078` | `minix-netdriver/src/sdev.rs:18-91` | ✅ |
| 可挂起表（8 是 / 8 否，CANCEL=n/a 建模为否） | `sockdriver.c:8-26` | `sdev.rs:96-116` | ✅ |
| SEV_* 0x01-0x20、SFL_* 0x01-0x10、hash `(id+(id>>16))%256` | `sockevent.h:7-19`、`sockevent.c:57` | `sockevent.rs:22-57` | ✅ |
| NDEV 0x1A00/0x1A80 族、multicast fallback | `com.h:1085+` | `protocol.rs` + `driver.rs:412-414` | ✅（有 `test_request_indices_match_c_offsets` 锁定） |
| sockid 五类基值 | `lwip.h:58-62` | `minix-netdriver/src/sockid.rs` | ✅ 已落地（本行旧记"无对应 Rust 代码"属**账面滞后**，见 §0.3 对 `N1-P1-1` 的行；核于 `2026-10-09`，`cargo test -p minix-netdriver --lib sockid` → 5 passed/0 failed，五个 `SOCKID_*` 基值逐个对 `lwip.h:58-62` 有钉定测试） |
| errno 双射（STACK_* → ERR_*，17 臂） | `liblwip util.c` | `lwip/src/util.rs:120-141` | ✅（测试锁定 wire 值） |
| lwip 胶水契约（NO_SYS=1 / POOL=0 / MSS=1460 / WND=16384 / SND=11×MSS） | `lwipopts.h` | `lwip_port.rs:22-35` | ✅ |
| uds 常量（256/64/0/32768/4096/5）与 hash `(dev^ino)%64` | `uds.h:15-36`、`uds.c:38-47` | `uds/src/core.rs:19-57`、`io.rs:18-25` | ✅ |

### 0.3 本轮新增条目总表

| 级别 | 条目 | 一句话 |
|------|------|--------|
| P1 | **N1-P1-1** | sockid 命名空间（lwip.h:58-62）零 Rust 归属 ✅ **已落地（2026-09-17）**：`minix-netdriver/src/sockid.rs` |
| P1 | **N1-P1-2** | sockdriver/sockevent 实体机制（续延池/事件对象/定时器/select 语义，C 3,740 行）无设计条目 ✅ **已落地（2026-09-17）**：`socktable.rs` + 02 篇机器章节 |
| P1 | **N1-P1-3** | [ARCH N-1] 栈本体选型 ✅ **已裁决（2026-09-17）**：smoltcp 一族 + 自研语义垫片，墙后可回退 FFI |
| P1 | **N1-P1-4** | 传输层与主循环设计：DispatchRoad 4 路对接真实 IPC + SEF/RS（edge E-NETSTART）+ 启动粒度决策 ✅ **已落地（2026-09-17）**：lwip/uds 双 server 真实事件循环 |
| P1 | **N1-P1-5** | 缓冲模型设计：pbuf 等价物与 VFS↔net↔NDEV 零拷贝链 ✅ **已落地（2026-09-17）**：单尺寸 slab 池 + 帧链（mempool.rs） |
| P2 | `N1-P2-1` | lwip crate 内 4 组重复函数收敛（send_flags/payload_fits/组播默认值/buffer_size_allowed） ✅ **已闭环（`2026-10-10` @ 快照 `b93c8d29a`，`claim/N1-P2-1-qoder`）**：逐组现状核——`send_flags_allowed`/`payload_fits`/`buffer_size_allowed` 早已各自转发到 `ipsock` 单实现（本轮 grep 坐实：udpsock.rs/rawsock.rs 的体均为 `crate::ipsock::…`，pktsock.rs 转发 `buffer_size_allowed`），漂移源本已消除；唯第四组「组播默认值」仍是 udp/raw 两份各自 `pub const MULTICAST_TTL_DEFAULT=1`/`MULTICAST_LOOP_DEFAULT=true`（raw 份且零引用），本轮上移到 `ipsock.rs` 单点、udp/raw 改 `pub use crate::ipsock::{…}` 再导出保 API 与测试不变。忠于 C 真源：二者镜像同一系统宏 `IP_DEFAULT_MULTICAST_TTL/LOOP`（`minix3/sys/netinet/in.h:305-306`），udp/raw 创建处（`udpsock.c:147`、`rawsock.c:319`）均内联同值 1，非异源独立缺省、可安全单点化；`MAX_PAYLOAD`（UDP/RAW 各一，`udpsock.c:27`/`rawsock.c:48`）系逐协议不同界值、不属本条四组、不并。核实命令与读数（容器 `minix-ci:1.94` `-m 2g -j 1`）：`cargo test -p minix-net-lwip` → 137 passed/0 failed（含 udp `test_multicast_defaults_match_creation` 仍绿）；`cargo clippy -j 1 -p minix-net-lwip --tests` → `lib test` 告警 `6→6`（自文件仅 `udpsock.rs:113` 存量 const-assert，未改行、值仍 const-true）、ipsock/rawsock 零告警（`2→` 零新增）；`rustfmt --edition 2024 --check` 三文件净（crate `main.rs` 存量 stable/nightly 漂移不在本改动面） |
| P2 | N1-P2-2 | driver.rs 死代码批：classify 死分支 + 三常量包装 + PolicyRow.priority + rawsock 常函数判定 |
| P2 | N1-P2-3 | legacy_fallback_applies 审计型死代码处置（[ARCH N-2]） |
| P2 | N1-P2-4 | translate 模式三处：恒等 flag 映射 / common_bits 逐位循环 / 手工位运算 vs bitflags 2.x |
| P2 | N1-P2-5 | 恒真测试分级处置（ALL_* 长度断言 + BUILD_C_FILES 文档型常量）vs 有效 wire 契约锁 |
| P2 | N1-P2-6 | 16 个 crate 声明未使用的 minix-types/minix-sys 依赖 ✅ **已闭环（2026-09-17）**：lwip/uds 随 N1-P1-4 真实化；NIC 部分归 16-stage/E-DEVWIRE |
| P2 | N1-P2-7 | uds 未依赖 minix-netdriver："两个网络服务的共同骨架"未接线（sdev/sockevent 全仓零消费者）✅ **已接线（2026-09-17）**：uds 依赖 minix-netdriver 并消费 socktable |
| P3 | N1-P3-1 | 文档同步 ✅ **全部落地（2026-09-17）**：plan.md + 00/99 两篇 v1 落稿 |
| P3 | N1-P3-2 | 文档模板复制段落 ✅ **已差异化（2026-09-17）**：udpsock 改述组播创建契约理由 |
| edge | E-SDEVOWN（并行登记，本轮增补证据） | sdev/sockevent 归属 17-stage 但寄居 minix-netdriver，vfs 另有一份 923 行独立实现——本轮补充类型漂移细节（u32 enum vs u64 手抄）与 sockid 缺口 |
| edge | E-DEVWIRE（并行登记） | 设备族 wire 常量单一来源已含 NDEV（minix-netdriver/protocol.rs） |
| edge | E-NETSTART（新） | lwip/uds 双 server 缺 SEF/RS 启动握手，依赖 E-FSRUNTIME 通用框架 |

### 0.4 验证基线（2026-09-17 实测）

- `cargo test -p minix-net-lwip -p minix-net-uds -p minix-netdriver`：**87 + 9 + 30 = 126 passed / 0 failed**
- `cargo clippy`（同三 crate）：net 自身 0 警告（minix-sys pm.rs 的 3 条警告属 04-stage-pm 域，另行处理）

---


---

## 归档

2026-09-17：14/14 条目闭环，正文（查漏补缺矩阵、逐条目设计与修复记录、死代码清单、测试面、边界节、edge 交叉引用）迁
[`archive/todo-N1-archive-2026-09-17.md`](archive/todo-N1-archive-2026-09-17.md)。

---

> 执行约定：后续新发现一律新开条目（新一轮编号 N2-*），逐条走 todo-fix 三步；锚点执行前按 fix-guard 重读核实；跨 stage 边界项追加 edge_todo.md，不并发修改其它 stage 的 todo.md。
