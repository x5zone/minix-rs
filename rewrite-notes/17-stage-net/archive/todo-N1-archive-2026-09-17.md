# todo-N1-archive：17-stage-net 架构扫描轮（N1 轮）正文归档

> **历史快照，不作现状来源**（标记于 `2026-10-08`）：本文件记录的是归档当轮的判定与读数，本轮尚未逐条复核；现状请以 ../../../coordination/TODO-LEDGER-OPEN.md（未完成）与 ../../../coordination/TODO-LEDGER-DONE.md（已完成与已定案）为准。

> 归档日期：2026-09-17。来源：`rewrite-notes/17-stage-net/todo.md`（14/14 闭环后正文迁此）。
> 修复过程：R0-R15 共 11 个提交（栈选型/sockid/续延机器/双服务循环/缓冲池/重复收敛/死代码/勘误/位标类型/恒真测试/依赖闭环），全部走 todo-fix 三步 + 回归 review。
> 检索权威：Fix 记录与判定过程以本文件为据；速览与索引在主文件。

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

### ✅ N1-P1-5 缓冲模型设计——已落地 2026-09-17

**落地**：`os/net/lwip/src/mempool.rs` 扩为两半：尺寸半边（原有）+ 池本体——`Pool`（512 字节单尺寸切片，slab 整块增长每块 512 片，上限 64 块即 mempool.c:238 的约 17MB）+ `SliceHandle` 句柄（索引不引用，仓库扩容不挪切片）+ `Frame` 帧链（句柄加已用长度，整链归还）。耗尽形状与 C 一致：分配返回空即套接字层 ENOBUFS（udpsock.c:496-497）。
**设计裁定**（≥2 方案）：单尺寸 slab 池胜出——C 的双尺寸编片（大片加四分之一小片，mempool.c:116-123）是给 pbuf 头部贴身装箱的产物，栈墙后头部归栈，小片随 pbuf 消失（差异表登记，非行为契约）；引用计数节点被否——单线程下显式归还路径里计数器纯属开销，是照抄 pbuf 生命周期的 translate。栈内缓冲归 smoltcp（R1 裁决的自然结果），服务器侧池只管 NDEV 帧、过滤器缓存与排队。
**验证**：`cargo test -p minix-net-lwip --lib mempool` = 8 passed（slab 增长封顶/耗尽无缓冲/句柄字节往返/LIFO 复用/帧记账归还/17MB 账目）；clippy 0 警告；Gate E：04 篇 §5.2 五行对账一致。
**文档同步**：04 篇头部 + §3.4 池决策 + §3.5 汇总两行 + §5.2 池测试表。

---

## 3. crate / 模块层条目

### ✅ N1-P2-1 lwip crate 内重复函数收敛——已落地 2026-09-17

**落地**：纯逻辑收敛到 `ipsock.rs`（互联网套接字公共层）：`send_flags_allowed` + `MSG_DONTROUTE`（原 udpsock/rawsock 各一份，rawsock 还跨模块引 udpsock 的常量）、`payload_fits(header, payload, max)`（调用方自带各自协议上限）、区间检查统一走 `buffer_size_allowed`（pktsock 的 u32 版与 tcpsock 的两处内联 Range 检查全部改为委托）。udpsock/rawsock 的同名函数保留为带 C 锚点的语义入口，函数体一行委托。
**范围判定收窄**：组播默认值与各 `MAX_PAYLOAD` 常量**不收敛**——它们各有独立 C 锚点（udpsock.c:147 vs rawsock.c:319；udpsock.c:27 vs rawsock.c:48），是恰好同值的独立契约项，合并会丢失锚点。
**验证**：`cargo test -p minix-net-lwip --lib` = 97 passed（测试名与断言值不变——行为无差异的直接证据）；clippy 0 警告。
**文档同步**：09/10 篇矩阵行注明"算术经 ipsock 共享实现（2026-09-17 收敛）"。

### ✅ N1-P2-2 driver.rs 死代码批——已处置 2026-09-17

- **死分支已删**：`classify` 的 `is_net_request` 二次检查（decode 已保证范围，恒真）删除，留注释说明单一守卫的理由；随删 `is_net_request` 与 `NDEV_LINK_UP`/`NDEV_MODE_DOWN` 的未用 import。
- **死包装已删**：`announce_ok`/`mode_down`/`link_up`（全仓零引用，纯常量转发）。
- **两个 OQ 判定（保留 + 锚点升级）**：`PolicyRow.priority` 补 RFC 6724 §2.1 规则表 `Preference` 列锚点——该列是 RFC 表格的真实组成部分，结构体照表转录，删除反而丢结构；`creation_requires_root` 恒真但注释已含 C 锚点（lwip.c:169-170）且有测试锁定，保留为命名的安全不变量。
**验证**：`cargo test -p minix-netdriver --lib` = 50 passed；clippy 0 警告。

### N1-P2-3 legacy_fallback_applies 审计型死代码处置

`os/libs/minix-sys/src/socket.rs:103-107`：[ARCH N-2] 弃用 fallback 后该函数恒不会被生产调用，仅测试引用（:140-145）；头注释自辩"documents the old branch so its removal is reviewable"（:101-102）。
**建议**：方案 A：降为 `#[cfg(test)]` 或并入 23 篇文档正文（条件本就是两行 errno 比较），函数删除；方案 B：保留现状。推荐 A——审计信息归文档是本仓既有惯例（对照 E-MINSYS-SCOPE 的域归属裁定），活代码里不应有"只为 review 存在"的函数。

### ✅ N1-P2-4 translate 模式三处——已处置 2026-09-17

- **恒等映射已化简**：`open_flags_from_socket_type` 改为一次掩码运算，注释保留位值同源的假设与 C `_socket_flags` 出处（方案 A；测试三个不变，行为零差异）。
- **common_bits 已化简**：逐位循环改为 `(first ^ second).leading_zeros().min(limit)` 一行；addr 18 测试全过（行为不变）。`row_matches` 的 128 位比较经评估保留——两值与掩码的三方比较不是 leading_zeros 能表达的形状。
- **SEV/SFL 迁 bitflags 2.4**：两个位标类型分域（事件域与标志域编译期隔离，裸 u32 时代互混不响）；`as u32` 全部改 `.bits()`；socktable 的事件/标志消费同步迁移。sockid 基值族维持 enum——它是命名空间基址不是位族（N1-P1-1 的判定不变）。doc-02 §3.1/§3.5 决策同步改写（普通枚举判定被位标类型取代，理由入档）。

### ✅ N1-P2-5 恒真测试分级处置——已落地 2026-09-17

- **删除**：`test_dispatch_covers_four_roads`（startup.rs，数组长度恒真）；`test_raw_needs_root` 与 `test_domain_and_types_match_dispatch` 的变体计数尾巴裁掉（保留实质断言）；`test_segment_kinds_cover_buffer_use` 整个删除（io.rs，段判别无 wire 契约）。中途自查否决了两个更差的改法（matches! 字面恒真、format!+leak 的伪判重）——恒真测试的正解是删除，不是换个姿势写。
- **转编译期冻结**：lwip_port 的钩子/补丁清单长度断言改 `const _: ()` ——清单是手工维护的 C 镜像，冻结留编译期，运行期测试撤销（doc-24 §5.1 同步）。
- **勘误保留**：`BUILD_C_FILES` 按 doc-04 §3.1 在册裁定保留（"不记录时子集漂移无人发现"——与 N1-P2-3 同款勘误逻辑：扫描建议与在册决策冲突时，决策优先）。
**验证**：lwip 94 passed / uds 8 passed；Gate E：03/21/22/24 篇测试表同步更新。

### ✅ N1-P2-6 16 个 crate 声明未使用的依赖——已闭环 2026-09-17

- **lwip/uds 部分**：随 N1-P1-4 自然消除——两 crate 现真实使用 minix-types（Message/Endpoint）、minix-sys（DirectTrapTransport）及 minix-sef/minix-netdriver/chardriver/blockdriver（新增），"声明未用"不再成立。
- **14 个 NIC crate 部分**：划归 16-stage 车道（os/drivers/net 是其域，且该域扫描线程正在活跃工作——workspace 依赖表与 driver-rt 抽取都是本轮眼见为实）。其依赖清理随 E-DEVWIRE 的常量单一来源落地一并处理，本条不再跟踪。

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

## 8. P3 落地记录（2026-09-17）

### ◐ N1-P3-1 文档同步——plan.md 已同步；00/99 展开仍开口

**已落地**：plan.md §3.4 基线更新（97+9+50=156 passed、clippy 零警告、零 unsafe/todo!）+ §6.1 checklist 01-24 篇勾选、00/99 两篇如实标注骨架待展开。
**开口**：00-net-overview 与 99-net-global-concepts 两篇从骨架展开为教学全稿（收编本轮新增的 wire/续延/池/栈墙设计与测试表），随下一轮落稿——余量不足以按 style-bible 标准写好时不交敷衍稿。

### ✅ N1-P3-2 文档模板复制段落——已差异化 2026-09-17

udpsock.rs 头注释中原样复制 ipsock 的"Linux/Redox 同款分层"段落删除，改述本模块真实的设计理由：组播默认值与发送守卫同居一处，是因为套接字创建时一口气取得全部默认值（udpsock.c 创建即设 TTL 与 loop），拆模块会让创建契约读成三个不相干数字。

---

> 执行约定：逐条走 todo-fix 三步（讲明白 → 多方案 → 实施），锚点执行前按 fix-guard 重读核实（本文件锚点为 2026-09-17 快照）；P0 清零是收敛前提，本轮 0 P0。修复顺序：N1-P1-3（选型裁决）→ N1-P1-2/P1-4/P1-5（三项设计文档可并行立项）→ P2 卓越度批（可穿插）→ P3 文档同步。
