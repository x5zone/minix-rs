# 17-stage-net Rust 实现架构级 Review TODO

> 来源：2026-09-17 架构级代码扫描（code-excellence cmd + 查漏补缺优先，非逐函数审查）。
> 范围：`os/net/lwip`（22 文件）+ `os/net/uds`（4 文件）+ `os/libs/minix-netdriver`（5 文件）+ `os/libs/minix-sys/src/socket.rs`（23-libc-socket）+ `os/drivers/net`（14 crate，仅边界轻扫）。与 17-stage-net 26 篇文档一一对应（00 篇导航表）。
> 方法：分层分析（整体架构 → crate → 模块 → trait/函数），每层自问"如果今天重写会怎么设计"；对照 C 真值（`minix3/minix/net/lwip` 24,477 行、`liblwip` 编译子集 58,232 行、`net/uds` 3,660 行、`libsockdriver` 1,150 行、`libsockevent` 2,590 行、libc socket 3,173 行）+ Redox netstack/smolnetd + smoltcp 0.14 + Rust 社区实践（2026-09 联网核实）。
> 定位：架构改进与查漏补缺清单。**本轮为扫描轮，未修改任何生产代码**。
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
| sockid 五类基值 | `lwip.h:58-62` | **无对应 Rust 代码** | ❌ 计划外缺失（N1-P1-1） |
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
| P1 | **N1-P1-5** | 缓冲模型设计：pbuf 等价物与 VFS↔net↔NDEV 零拷贝链 |
| P2 | N1-P2-1 | lwip crate 内 4 组重复函数收敛（send_flags/payload_fits/组播默认值/buffer_size_allowed） |
| P2 | N1-P2-2 | driver.rs 死代码批：classify 死分支 + 三常量包装 + PolicyRow.priority + rawsock 常函数判定 |
| P2 | N1-P2-3 | legacy_fallback_applies 审计型死代码处置（[ARCH N-2]） |
| P2 | N1-P2-4 | translate 模式三处：恒等 flag 映射 / common_bits 逐位循环 / 手工位运算 vs bitflags 2.x |
| P2 | N1-P2-5 | 恒真测试分级处置（ALL_* 长度断言 + BUILD_C_FILES 文档型常量）vs 有效 wire 契约锁 |
| P2 | N1-P2-6 | 16 个 crate 声明未使用的 minix-types/minix-sys 依赖 |
| P2 | N1-P2-7 | uds 未依赖 minix-netdriver："两个网络服务的共同骨架"未接线（sdev/sockevent 全仓零消费者）✅ **已接线（2026-09-17）**：uds 依赖 minix-netdriver 并消费 socktable |
| P3 | N1-P3-1 | 文档同步：plan.md §6.1 checklist 全 ☐ 过时、§3.4 测试基线过时、00/99 两篇仍骨架 |
| P3 | N1-P3-2 | 文档模板复制段落（ipsock.rs:10-14 ≈ udpsock.rs:10-13） |
| edge | E-SDEVOWN（并行登记，本轮增补证据） | sdev/sockevent 归属 17-stage 但寄居 minix-netdriver，vfs 另有一份 923 行独立实现——本轮补充类型漂移细节（u32 enum vs u64 手抄）与 sockid 缺口 |
| edge | E-DEVWIRE（并行登记） | 设备族 wire 常量单一来源已含 NDEV（minix-netdriver/protocol.rs） |
| edge | E-NETSTART（新） | lwip/uds 双 server 缺 SEF/RS 启动握手，依赖 E-FSRUNTIME 通用框架 |

### 0.4 验证基线（2026-09-17 实测）

- `cargo test -p minix-net-lwip -p minix-net-uds -p minix-netdriver`：**87 + 9 + 30 = 126 passed / 0 failed**
- `cargo clippy`（同三 crate）：net 自身 0 警告（minix-sys pm.rs 的 3 条警告属 04-stage-pm 域，另行处理）

---

## 1. 查漏补缺矩阵（26 篇文档 → Rust 现状）

> 逐篇结论：01-24 篇映射的 .rs 文件全部存在，其声明的"本模块拥有的半边"（numbering/naming/order/常数契约）已实现且有测试；各篇 §5 声明的测试经 grep 对账全部存在（Gate E 通过）。缺口集中在"留给 service binary 的另一半"——其中四块无后续设计条目，单列如下；其余为 §2-§3 卓越度条目。

- **01/02 篇**：sdev.rs + sockevent.rs 只覆盖"编号 + 可挂起表 + 事件位命名"。C 的实体机制（sockdriver.c 的 call 对象续延池、sockevent.c 的 256 槽 sock 表、定时器链、select 的 SELECT1/SELECT2 双段语义）零建模，且**没有设计文档**（01 篇 :30 只写 "suspended-call continuation" 一句带过）→ N1-P1-2。
- **03 篇（lwip-main-init）**：startup.rs 建模了顺序与 4 路分发，但 main.rs 是占位、SEF/RS 握手无着落 → N1-P1-4 + edge E-NETSTART。
- **13-16 篇（ndev/ifdev/ethif/ifaddr）**：策略常量有了，与 minix-netdriver/driver.rs（驱动侧框架）的对接——谁构造 NetServer、谁实现 NetDriver——无消费代码。NDEV 消费侧抽象即 [ARCH N-4]（plan.md:207+ "待设计"）→ 并入 N1-P1-4 的设计面。
- **23 篇（libc-socket）**：socket.rs 是纯 policy；陷入层按文档归"调用者实现"（23 篇 §2.4），但 os/ 侧尚无任何调用者示范（commands/netconfig 系走自有路径）→ 记录为 18-stage-commands 联调面，不单列条目。
- **99 篇**：承诺 constant-value ownership 归 minix-types（99 篇 :7-14），实际 minix-types 零 net 类型，SDEV 契约数字在 netdriver 与 VFS 两处各自手抄 → edge E-SDEVOWN + N1-P1-1。

---

## 2. 整体架构层条目（如果今天重写会怎么设计）

### ✅ N1-P1-1 sockid 命名空间零 Rust 归属（计划外缺失）——已落地 2026-09-17

**落地**：`os/libs/minix-netdriver/src/sockid.rs`——`SockId` 新类型（包 `int32_t`）+ `SockClass` 五类枚举。设计要点：① 负数是错误通道（`sockdriver.h:27-28`），`from_raw` 拒绝负值，标识与错误在类型上分家；② 铸造同 C 形状"类基值或下标"（`tcpsock.c:140` 等）；③ 下标字段 20 位（类基值以 0x00100000 步进推得），溢出拒绝；④ **不做按类枚举**——uds 裸下标（`uds.c:97-101`）与 TCP 类共用数值区间，枚举会对裸下标撒谎，这是本条最重要的一次防 translate 判断。
**home 决策**：minix-netdriver（与 sdev/sockevent 同居，uds 依赖路径 N1-P2-7 打通）；E-SDEVOWN 若裁定建 minix-sockdriver，三个模块整体迁移。99 篇"归属 minix-types"承诺随 E-SDEVOWN 的 wire 收敛一并处置，本条不再单独等待。
**验证**：`cargo test -p minix-netdriver --lib sockid` = 5 passed（类基值锁值/铸造同形/溢出与负数拒绝/线上往返/哈希互通）；全量 `cargo test -p minix-netdriver --lib` = 36 passed；Gate E：01 篇 §5.2 表与 `rg "fn test_" sockid.rs` 对账一致。
**文档同步**：01 篇新增 §3.4 决策 + §2.7/§2.8 矩阵与差异行 + §5.2 测试表；plan.md N-6 行状态与涉及文档修正（02/99 → 01/99，`sockid_t` 在 sockdriver.h，C 真值优先）。

### ✅ N1-P1-2 sockdriver/sockevent 实体机制无设计条目——已落地 2026-09-17

**落地**：`os/libs/minix-netdriver/src/socktable.rs`（对象表 256 槽 + `Continuation` + 事件泵 + 选择登记 + 双闹钟）。
**核心设计**（≥2 方案对比后裁定，见 02 篇 §3.3-3.4）：**动作出列代替回调**——`raise` 返回 `WakeAction` 清单（Resume/RetestSelect/Alarm/TimedOut）交服务逐条执行，C 的回调重入保护（`sockevent_working` 旗标 + 待处理队列，sockevent.c:915-941）在动作模型下没有保护对象，不复存在。记账进库、就绪判定留服务（C 里两件事本就分开：挂起结构是纯记账，试选回调每次问协议族水位）。续延显式携带唤醒掩码与截止时刻（C 由框架按请求类型定事件、`spr_time` 记超时——`sockevent_proc.h:4-19`）；错误唤醒集与 C 逐位一致（BIND|CONNECT|SEND|RECV，接客除外——测试曾假设错误唤醒接客，被证伪后按 C 真值修正，sockevent.c:963）；关闭绝不定时（独立方法，`raise` 收不到关闭位，sockevent.c:899-909 语义）。
**顺带补齐**：sdev.rs 头注释声称但缺失的 `SDEV_NONBLOCK`/`SDEV_OP_*` 常量（com.h:1071-1078）+ `may_suspend` 常量化。
**防 translate 自查**：固定池 → 随对象生灭的 Vec（池上界改为对象寿命，差异表登记）；回调分发 → 动作出列；测试曾被错误假设打红一次（set_error 唤醒集），以 C 真值裁决——正是"卓越建立在正确之上"的实证。
**验证**：`cargo test -p minix-netdriver --lib` = 46 passed（机器 9 个：挂起唤醒/联动/错误集/立即回收/选择重测/撤单/双闹钟/同槽共存/非挂起拒绝）；clippy 0 警告；Gate E：02 篇 §5.2 与 grep 对账一致。
**文档同步**：02 篇（头部模块行、§2.7 矩阵五增行、§2.8 差异六行、§3.3-3.5 决策重写、§4 错误表、§5.2 测试表、§6/§7）；01 篇（§2.7 标志行、§5.1 标志测试、§5.3 计数）；lib.rs 模块清单。

### ✅ N1-P1-3 [ARCH N-1] 协议栈本体选型——已裁决 2026-09-17

**裁决**：smoltcp 一族 Rust 栈（0.14）+ 自研语义垫片；`lwip_port.rs` 新增 `Stack`/`StackHooks` trait 墙，服务代码只依赖墙，FFI 路线墙后可回退（用户否决点保留）。
**关键证据**：偏差面比直觉小——SACK/时间戳/紧急指针 C 侧同样缺失或零引用（lwipopts.h 无配置、tcpsock.c/lwip.h/libc grep 零命中）＝零偏差；pktsock/bpfdev/rtsock/ifconf 两边都自研；唯一实现差异是 ISN 注入点（值非行为契约）；路由真相在服务（C 路由覆盖钩子被调即 panic，lwiphooks.h:17-19），墙上只留网关解析。
**三处标注**：24-liblwip-port.md §1.5（裁决记录 + parity 表 + 三方案对比）、plan.md §4 N-1 行、lwip_port.rs 头注释。
**验证**：`cargo test -p minix-net-lwip --lib` = 89 passed（新增 `test_wall_composes_hooks_lifecycle_and_poll`、`test_wall_frame_seam_moves_bytes_both_ways`）；Gate E：24 篇 §5.1 表与 `rg "fn test_" os/net/lwip/src/lwip_port.rs` 对账一致。
**残留**：smoltcp 依赖在缓冲轮次（N1-P1-5）才进 Cargo；适配层为后续独立轮次。

### ✅ N1-P1-4 传输层与主循环设计——已落地 2026-09-17

**落地**：lwip `server.rs`（`run` 循环：启动链门控七步 → SEF 拦截收信 → 四路分发 → 回复非阻塞发送）+ uds `server.rs`（通知路 + 套接字设备路 + 排空退出）+ 共享分类器 `minix-netdriver/src/service.rs`（来源端点优先、类型其次，与 C 同序）。两个 main.rs 的 `loop {}` 占位与 TODO 注释消除：生产传输为 `KernelIpc` 适配器（minix-sys 陷阱指令），SEF 经 `minix-sef`，`EINTR` 回环、连续三次传输损坏带错误退出。
**设计要点**：① 传输接缝拆成 `SefIpc`（收+通知，SEF ping 内部吞掉）+ `ReplyIpc`（非阻塞回复），生产/测试双实现（ipc-server 的事件循环先例）；② 每条路的业务体挂 `NetHandler`/`UdsHandler` 特征，循环零业务逻辑——处理中再触发事件只是又一次调用（对照 R3 的动作出列，风格一致）；③ 非 CLOCK/DS 的通知在 C 里就是意外分支（printf 后丢弃，lwip.c:336-343），不另设路；④ 启动粒度定案：保留 8 态粗粒度，逐步映射 RS 调用属 translate；⑤ uds 主循环条件 `loop_keeps_running`（排空语义）接入 `keep_running`。
**接线红利**：minix-types/minix-sys 在 lwip/uds 两 crate 从"声明未用"转为真实使用；uds 接上 minix-netdriver（N1-P2-7 并入本轮闭环）；发现并修正一处分发偏差（VFS 未知号曾误入套接字路，C 的 fallthrough 语义是意外）。
**验证**：`cargo test -p minix-net-lwip -p minix-net-uds -p minix-netdriver` = 92+9+50 = 151 passed；新增循环测试 4 个（启动门/六到达分发与回复送达/范围判定锁/启动前零分发），脚本化传输 + 记录型处理器，SEF ping 吞掉断言在案；clippy 0 警告。
**残留**：RS 域注册/特权协议与 QEMU 真机冒烟挂 edge E-NETSTART（本轮循环已可注入测试全链路跑通，非 DEFERRED 逃避——跨 stage 边界项按规则归 edge）；各路实现体随 R5 与 21/22 篇轨道。

### N1-P1-5 缓冲模型设计（pbuf 等价物与零拷贝链）

**问题**：C 侧 PBUF_POOL_SIZE=0 用定制池替代（lwip_port.rs:24-26 契约已记录），pbuf 分层引用与 UDS 的"数据+元数据单环"（uds io.c:122，Rust io.rs:11-13 已建模意图）都只有常量无数值结构；NDEV 侧 SEND_QUEUE_BOUND=8/RECV=2（protocol.rs:37,42）如何与栈缓冲衔接未设计。
**建议**（≥2 方案）：方案 A：`Pool` 定制池 + 引用计数分片（pbuf 语义的 Rust 化，零拷贝留在栈内）；方案 B：smoltcp 路线则用其 `RxToken/TxToken::consume` 闭包免拷贝（仅栈内），VFS↔net 边界首版接受一次拷贝（Redox buffer_pool 先例），零拷贝移交挂后续（对照 E-FSVMCACHE 在 fs 的页移交先例）。裁定随 N1-P1-3 选型联动。

---

## 3. crate / 模块层条目

### N1-P2-1 lwip crate 内 4 组重复函数收敛

锚点：`send_flags_allowed`（udpsock.rs:51-53 ≡ rawsock.rs:64-66，后者跨模块引 `crate::udpsock::MSG_DONTROUTE`，rawsock.rs:65）；`payload_fits`（udpsock.rs:64-66 ≡ rawsock.rs:70-72，两个独立 `MAX_PAYLOAD`=65535 常量，另 pktsock.rs:24,40 还有 DATAGRAM/RAW 两份 65535）；组播默认值（rawsock.rs:26,29 ≡ udpsock.rs 组播常量）；`buffer_size_allowed` 双类型版本（ipsock.rs:81 usize vs pktsock.rs:81 u32）+ tcpsock.rs:63-71 内联第三份同型区间检查。
**建议**：C 侧 udpsock.c/rawsock.c 各自实现是 C 的复制惯例；同一 Rust crate 内应收敛——方案 A：datagram 公共小模块（或 ipsock 层）承载四组，常量单点 `pub use`；方案 B：仅合并函数、常量留原地。推荐 A（65535 的四份手抄是 drift 隐患）。

### N1-P2-2 driver.rs 死代码批（删/接线/标注三分）

- **死分支**：`classify` 中 `NdevRequest::decode` 成功后 `is_net_request` 恒真（decode 只在 base+0..=5 上 Some，均通过 mask 检查；driver.rs:395-400）→ 删 :398-400 两行或删 decode 改手写，二选一。
- **死包装**：`announce_ok`/`mode_down`/`link_up` 常函数纯转发常量、生产零调用（driver.rs:416-429）→ 删（为何死：仅为命名阅读性；消除影响：无，常量本身已有语义名）。
- **文档型字段/函数判定**：`PolicyRow.priority` 自述"kept for documentation"（addr.rs:39-41）；`creation_requires_root` 恒真（rawsock.rs:44-49，有显式辩护注释）。二者属"审计型存活"，判定：保留但把辩护注释升级为指向 C 锚点的单一来源，或删除由测试承载（OQ 留给执行轮，不擅删）。

### N1-P2-3 legacy_fallback_applies 审计型死代码处置

`os/libs/minix-sys/src/socket.rs:103-107`：[ARCH N-2] 弃用 fallback 后该函数恒不会被生产调用，仅测试引用（:140-145）；头注释自辩"documents the old branch so its removal is reviewable"（:101-102）。
**建议**：方案 A：降为 `#[cfg(test)]` 或并入 23 篇文档正文（条件本就是两行 errno 比较），函数删除；方案 B：保留现状。推荐 A——审计信息归文档是本仓既有惯例（对照 E-MINSYS-SCOPE 的域归属裁定），活代码里不应有"只为 review 存在"的函数。

### N1-P2-4 translate 模式三处（模式 16/17 对照）

- `open_flags_from_socket_type`（minix-sys/socket.rs:80-95）：三支 if/else 做恒等映射（FLAG_* 与 O_* 位值同为 0x01/0x02/0x04），整个函数等价 `socket_type & 0x07`。方案 A：改掩码 + 注释保留 C `_socket_flags` 语义与位值假设；方案 B：保留显式映射作为"位值分叉时自动出错"的契约文档。倾向 A 但属风格判定（OQ）。
- `common_bits` 逐位循环（addr.rs:106-119）→ `(a ^ b).leading_zeros()` 一行；`row_matches` 的 128 位手移（addr.rs:47-53）同步评估。
- 手工 `u32` 位族（sockevent.rs:22-51、protocol.rs 位常量区）→ workspace 已有 bitflags 2.x 依赖可选：SEV/SFL 是真 bitflag 语义（C 本就是宏位族），建议引入；sockid 基值族不是（是命名空间基址，enum 更合适）。

### N1-P2-5 恒真测试分级处置（Gate E 附带发现）

判定标准：断言对象若为编译期事实（本地 enum 的 variant 数组长度），测试恒真、无效。清单：`test_dispatch_covers_four_roads`（startup.rs:141-150，构造数组断言 len==5）、`test_domain_and_types_match_dispatch` 尾部（core.rs:115-120，len==3）、`test_segment_kinds_cover_buffer_use`（io.rs:96-104，len==4）、`test_hooks_cover_glue_header`（lwip_port.rs:83-85）、`test_patches_cover_patch_directory`（lwip_port.rs:88-90，断言字面量数组长度）。**与之相反，wire 契约锁是有效测试**：`test_sdev_bases_match_com_header`、`test_seventeen_requests_numbered_in_order`（sdev.rs:123-134）、`test_key_options_match_glue_header`（lwip_port.rs:73-80，锁 lwipopts 契约值）、`test_request_indices_match_c_offsets`——断言对象是跨语言 wire 事实，编译期不可知，保留。另：`BUILD_C_FILES: usize = 68`（lwip_port.rs:18）是构建事实非运行时常量，建议降为文档（24 篇已载）+ 删常量。
**处置**：恒真组删除或改 `const { assert!() }` 编译期断言；逐条走 fix-guard。

### N1-P2-6 16 个 crate 声明未使用的依赖

`os/net/lwip`、`os/net/uds`、14 个 NIC crate 的 Cargo.toml 均声明 minix-types + minix-sys，代码 grep 零引用（仅 minix-netdriver/driver.rs:14 真实使用 minix_types 三常量）。
**建议**：与 E-SDEVOWN/E-DEVWIRE 联动——若 wire 类型落点裁定为 minix-sys/minix-types 的 net 模块，这些依赖将来会真实化，可留但应加注释或 `#[cfg]`；否则删除。不与 E-MINSYS-SCOPE 的 ds 先例（edge_todo.md ds 依赖卫生条款）重复立项，net 侧收敛随 wire 裁定一次处理。

### ✅ N1-P2-7 uds 未依赖 minix-netdriver——已接线 2026-09-17（随 N1-P1-4 并入本轮）

uds 的 Cargo 依赖补上 minix-netdriver + minix-sef；`server.rs` 的套接字设备路消费 `sdev::is_sdev_request` 与 `socktable::SockTable`，sdev/sockevent/sockid/socktable 四模块自此有了真实消费者。与 E-SDEVOWN 的关系不变：若其裁定建独立 minix-sockdriver，四模块整体迁移。

---

## 4. 测试面结论

- Gate E 测试名对账：26 篇文档 §5 声明的测试与实际 grep 一致（本轮以 lwip_port/startup/sdev/sockevent/uds 五处直读复核）；126 个测试命名统一 `test_*` snake_case。
- 有效性分级见 N1-P2-5；无"测试代码本身是错的"发现（对照 test-audit 第二维）。
- 基线：`cargo test -p minix-net-lwip -p minix-net-uds -p minix-netdriver` = 126 passed（2026-09-17）。

## 5. 死代码清单（汇总）

| 锚点 | 内容 | 为何死 + 消除影响 |
|------|------|------------------|
| driver.rs:398-400 | classify 内 is_net_request 二次检查 | decode 已保证；删除无行为影响 |
| driver.rs:416-429 | announce_ok/mode_down/link_up | 生产零调用常量转发；删除无影响 |
| socket.rs:103-107 | legacy_fallback_applies | [ARCH N-2] 后生产不可达；审计信息转文档 |
| addr.rs:39-41 | PolicyRow.priority | 自述仅文档用途；OQ（转注释或保留） |
| rawsock.rs:44-49 | creation_requires_root 恒真 | 有辩护注释；OQ（同上） |
| lwip_port.rs:18 | BUILD_C_FILES=68 | 构建事实非运行时常量；转 24 篇文档 |
| 16 crate Cargo.toml | 未用依赖 | 见 N1-P2-6 |

## 6. 边界节（非本 stage，仅登记指向）

- `os/drivers/net` 14 个 NIC crate 归 16-stage-drivers：9 个纯 stub、5 个仅环形/游标算术（virtio_net queues.rs、e1000 desc.rs、rtl8139 txrx.rs、dp8390 ring.rs、lance ring.rs）。硬件原语已由 minix-platform 承载（arch 层 inb/outb/MMIO 已存在），实装不缺跨 stage 基础设施，不立 edge。本轮轻扫未发现 5 个已实装 crate 间的公共 ring 抽象缺失以外的结构问题；深度 review 归 16-stage todo。
- 18-stage-commands 的 netconfig/netservices 已是自含实现；23 篇"调用者示范"在联调轮对表（§1）。

## 7. edge 交叉引用

- **E-SDEVOWN**（16-stage-drivers 全量扫描同日登记）：sdev/sockevent 语义归 17-stage 却寄居 minix-netdriver、vfs 持第二份 923 行独立实现、方案 A 主张认领时双删并在 minix-sys 客户端域或新库单点重建。**本轮独立复核证实其判定并增补证据**：两副本类型已漂移（minix-netdriver/sdev.rs:18-21 u32 族 + enum repr u32 vs os/servers/vfs/src/sdev.rs:32-75 u64 手抄族），且 99 篇承诺的 sockid 归属 minix-types 同样未兑现（N1-P1-1）——认领重建时 sockid 一并落位。
- **E-DEVWIRE**（同日登记）：设备族线上常量单一来源，范围已含 NDEV（minix-netdriver/protocol.rs:10-60）——本 stage 不重复立项，N1-P2-6 的依赖去留随其裁定联动。
- **E-NETSTART（本轮新登记）**：lwip/uds SEF/RS 启动握手，依赖并跟随 E-FSRUNTIME（fs 8 server 先例）的通用框架；解锁后 N1-P1-4 主循环才能真实通电。
- **E-RMIBWIRE**（已有）：lwip/uds 的 MIB 注册与转发在其解锁条款内（edge_todo.md lwip/uds 子树条款），不重复立项。
- **E-MINSYS-SCOPE**（已闭）：socket.rs 的 feature 域归属已裁（socket feature，default on）。

---

> 执行约定：逐条走 todo-fix 三步（讲明白 → 多方案 → 实施），锚点执行前按 fix-guard 重读核实（本文件锚点为 2026-09-17 快照）；P0 清零是收敛前提，本轮 0 P0。修复顺序：N1-P1-3（选型裁决）→ N1-P1-2/P1-4/P1-5（三项设计文档可并行立项）→ P2 卓越度批（可穿插）→ P3 文档同步。
