# 12-stage-input Rust 实现架构级 Review TODO

> 来源：2026-09-15 首轮架构审查（`code-excellence` + coverage 查漏补缺，双目标一次扫描）。
> 范围：`os/servers/input/src/`（13 文件 4641 行）、`os/libs/minix-sys/src/inputdriver.rs`（486 行）、`os/libs/minix-types/src/ipc/input.rs`（529 行）及 `message.rs` 的 input 联合体成员（message.rs:248-255）。邻接只读（所有权在 16-stage-drivers，只登记移交，不当场处置）：`os/drivers/hid/pckbd`、`os/drivers/tty/tty`、`os/libs/minix-chardriver`。
> 方法：三层对账（C 语义 → 决策核心 → 可运行实体）+ 分层设计审视（整体 → crate 边界 → trait/模块 → 函数），对照 Minix3 C 源（ground truth）、Redox 的中断分发 → `ps2d` → `inputd` 分层（[This Month in Redox 2026-01](https://www.redox-os.org/news/this-month-260131/)、[System Components](https://redox-os-redox.mintlify.app/architecture/components)）、Linux 的 serio → input core → evdev 分层（[内核输入子系统文档](https://www.kernel.org/doc/html/v4.16/input/input.html)）、Rust 社区实践（纯函数决策核 + 效应输出、`embedded-hal` 式硬件 trait 边界）。
> 定位：查漏补缺与架构改进建议清单。**本清单不修正确性 bug**——正确性问题登记后交 `full-review` / `todo-fix` 处置（卓越建立在正确之上）。
> 跨 stage 条目不在此展开，登记于 [`../edge_todo.md`](../edge_todo.md)：E-INWIRE（传输接线）、E-CDRCONV（框架收敛 + minix-chardriver 常量错值）、E-TTYEVENT（TTY 消费侧）、E-PCKBDREG（pckbd 移交）。
> 本文件为首轮新建（此前 12-stage-input 目录无 todo.md），无历史条目需要清理。

---

## 0. 审查结论速览

三条总体判断：

1. **决策核心层的质量是高的。** `input.c` 全部 20 个函数的行为语义几乎都有纯函数对应（对账表见 §1.0），66 个测试全部通过；文档 01-12 声称的每个测试函数名都能在代码中逐字找到（测试名对账门通过，无虚构测试）；四处"C 崩溃 → Rust 显式错误"的架构演进（A-7 反向映射、A-11 拷贝计数、InvalidParking、开门集合溢出）都有带编号的注释。
2. **可运行实体三层缺席是最大缺口，而且不被别人阻塞。** 事件循环（main.rs:31-32 是空 `loop {}`）、生产状态构造（`InputTable::fresh()` 的调用者全部是测试）、传输装配（`minix-types` 的五个消息编解码器在 server 内零消费）都不存在。但 `minix-sys` 的 `send`/`receive`/`sendrec`/`notify`（minix-sys/src/lib.rs:79-103）、DS 客户端全套餐（ds.rs:128-271 的 publish/retrieve/subscribe/check）、异步发送表（ipc.rs 的 `AMF_NOREPLY` 面）都已就绪——缺口是这个 stage 自己的组装工作，不是在等别的 stage。
3. **平行实现开始分叉，收敛决策到了必须收口的时候。** C 里只有一个 `libchardriver`，Rust 里出现了两套（servers/input/framework.rs 与零依赖方的 os/libs/minix-chardriver）；pckbd 的 `InputBridge` 与 minix-sys 的 `DriverRegistration` 又编码了同一份 C 逻辑。plan.md §7.3 的 A-1 决策（"倾向共享框架 crate，input 内部最小等价实现可先行"）的前半句已成立、后半句已执行，但两半从未合拢。

| 级别 | 条目 | 一句话 |
|------|------|--------|
| P1 | IN-P1-1 | 可运行实体缺席：主循环策略、`input_other` 两个消息面分支、驱动离去检测、label 校验、CONF 回信构造、生产态构造（✅ 2026-09-15，见 §1） |
| P1 | IN-P1-2 | "唤醒时恰好拷一个事件"的组合缺失：C 最核心的运行时链路没有承载函数，也没有测试（✅ 2026-09-15，见 §1） |
| P1 | IN-P1-3 | 拷贝失败不推进的挂钩点缺失：`serve_copy` 无条件推进队列指针，传输失败分支不存在（✅ 2026-09-15，见 §1） |
| P2 | IN-P2-1 | `serve_copy` 返回单位分歧：Rust 返回事件数，C 返回字节数，签名层无提示（✅ 2026-09-15，见 §2） |
| P2 | IN-P2-2 | 事件码词汇的 crate 归属：C 是共享头文件，Rust 锁在 server crate，驱动侧与 TTY 侧将来要么重复定义要么反向依赖（✅ 2026-09-15，见 §2） |
| P2 | IN-P2-3 | 组合层设计未定：纯函数散件与"一条消息的完整效应"之间缺一层约定，传输落地前必须裁决（✅ 2026-09-15，见 §2） |
| P2 | IN-D1 | 文档测试计数过时：01-04 篇称 28 个、06-08 篇称 48 个、12 篇称 22 个，实际 66 / 66 / 136（✅ 2026-09-15，见 §3） |
| P2 | IN-D2 | 00 总览与 99 全局概念两篇文档仍处于 pending 状态（✅ 2026-09-15，见 §3） |
| P3 | IN-P3-1 | 死代码与仅测试消费项清单（分"等通电""真死""上交裁决"三类） |
| P3 | IN-P3-2 | 边界测试族缺口（极限回绕、槽位耗尽、selector 覆盖、单侧断连等） |
| P3 | IN-P3-3 | wire 测试盲区（`tty_up_msg` 拒收、KIOCSLEDS 截断、保留槽、通知携带消息号） |
| P3 | IN-D3 | close 全清偏离未按三层术语标注 `[ARCH: ...]`（doc + design + code 三处一致） |
| P3 | IN-D4 | 语义映射表只覆盖 01-12 篇；14 篇契约文档与 16-stage 实现文档互不回指 |
| edge | E-INWIRE | input 服务器生产传输接线四缺（挂 minix-sef 落地方式决策 + 联调挂靠 E5） |
| edge | E-CDRCONV | chardriver 框架双实现收敛（A-1 收口）+ minix-chardriver `CDEV_REPLY_BASE = 0x500` 错值 |
| edge | E-TTYEVENT | `TTY_INPUT_UP` / `TTY_INPUT_EVENT` 消费侧零实现（16-stage 06-tty-driver 范围） |
| edge | E-PCKBDREG | pckbd 移交登记：状态 3 行为分歧、FLAG_RELATIVE 死常量、扫描码全表缺席、双轨重编码 |

验证基线（2026-09-15 实测，os/ 下）：`cargo test -p minix-input` 66 通过；`cargo test -p minix-driver-pckbd` 17 通过；`cargo test -p minix-types` 207 通过；`cargo test -p minix-sys` 136 通过。全部零失败。

---

## 1. 查漏补缺（P1：行为与结构缺口）

### 1.0 对账总表（本轮三层对账的结论底座）

C 语义层：`input.c` 20 个函数中，18 个有 Rust 决策函数或数据化对应（如 `input_map` → structs.rs:119 `map_minor_to_index`、`input_process` → produce.rs:157 `enqueue` + produce.rs:228 `decide_wake`、`input_alloc_id` → connect.rs:58 `alloc_id`）；2 个只有局部对应（`input_check` 缺离去半、`input_other` 缺消息面两分支），见 IN-P1-1。wire 层：五个消息（`TTY_INPUT_UP` 0x1302、`TTY_INPUT_EVENT` 0x1303、`INPUT_CONF` 0x1500、`INPUT_SETLEDS` 0x1501、`INPUT_EVENT` 0x1580）的消息号、字段顺序、字段宽与 C（com.h:879-893、ipc.h:236-259、:993-1001）逐项一致，56 字节载荷有断言锁（message.rs:3899-3902），联合体成员与 C ipc.h 的四条接入一一对应，无遗漏消息类型。可运行实体层：主循环、生产状态、传输装配三者全缺（见 §0 判断 2）。

### IN-P1-1 可运行实体缺席（六件 stage 内组装工作）【✅ 已完成 2026-09-15】

**是什么**：input 服务器今天无法作为进程运行——不是因为有半成品跑不通，而是"服务器"这个实体根本还没被组装起来。六件缺失的工作全部可以先用纯函数决策的形式落地，不依赖传输层：

1. **主循环策略**：接收一条消息后走"分类 → 重启门 → 处理函数 → 回信判决"的完整流水线。三个判决函数都已存在（framework.rs:190 `classify_request`、framework.rs:228 `gate_character_request`、framework.rs:335 `decide_reply`），但没有任何函数把它们串起来；main.rs:31-32 是带注释的空 `loop {}`。
2. **`input_other` 的两个消息面分支**：DS 通知到达时触发驱动到来/离去检查（C input.c:613-615）；`INPUT_SETLEDS` 只接受 TTY 来源、其余来源按 A-10 决策记日志忽略（C input.c:630-635 的 fall-through 语义）。两者在 Rust 里连纯函数都没有。
3. **驱动离去检测（`input_check` 的离去半）**：遍历有主槽位、向 DS 反查 label、查无此人（`ESRCH`）则断连并唤醒挂起读者（C input.c:588-602）。Rust 只有到来半（connect.rs:37 `key_is_new_driver`），离去半整体缺失。
4. **label 校验决策**：驱动注册时从 DS 取发送方真实 label 并与发布键比对，不符则静默忽略（C input.c:488-495）。Rust 侧只有文档注释声明"归 dispatcher"（connect.rs:116-119），决策本体不存在。
5. **`INPUT_CONF` 回信构造**：分配槽位后向驱动发配置消息，保留槽固定填 `INVALID_INPUT_ID`（C input.c:514-523）。`minix-types` 的 `conf_msg` 构造器已备（ipc/input.rs:238 区域），server 未消费；决策输出 `ConnectReport`（connect.rs:123-130）也没有保留槽字段，接线时容易漏填（另见 IN-P3-1）。
6. **生产态构造**：main 需要构造 `InputTable::fresh()`（structs.rs:315）与开门集合（framework.rs:246），目前生产代码零状态。

**为什么这么切**：1-6 里除主循环的收发动作外都是纯决策，先落决策再接传输，与这个 crate 已验证的开发顺序一致。实际收发与 DS 客户端消费属于传输接线，登记在 edge E-INWIRE，不在此重复。

**处置建议**：按 2 → 3 → 4 → 5 → 6 → 1 的顺序逐项经 `todo-fix` 落地（每项独立可测）；主循环策略与 IN-P2-3 的组合层设计合并裁决后一次落地。

**修复记录（2026-09-15，全部六件落地）**：新增 `dispatcher.rs`——`Server { table, opened }` 生产态装配（第 6 件，`Server::fresh` 对应 input.c:652-662 的清表加宣告期清登记簿）；`handle_arrival(server, Arrival) -> Outcome` 主循环裁决（第 1 件）：字符设备请求分支串起门卫（GateVerdict，未登记只放开门、登记簿满答 EAGAIN 并注明不可达依据）、第 06-08 篇全部判决与效应；`input_other` 两个消息面分支（第 2 件）——`Arrival::TerminalSetleds` 内做仅 TTY 来源检查（`Endpoint::TTY`，input.c:630-635 的 A-10 fail-closed）后走 `broadcast_lights`（存全部、发有主），DS 通知则按两段式驱动驱动生命周期；离去检测（第 3 件）——`Server::departure_candidates`（列出有主槽位，input.c:588-602 的循环半）加 `Server::driver_departed`（ESRCH 断开：读者 EIO、选择者可读通知、只清主人）；label 校验（第 4 件）——`connect.rs` 新增 `labels_match`（input.c:492-495 的 strcmp 语义）加 `Server::driver_connect`（不符静默忽略）；CONF 回信构造（第 5 件）——`Effect::input_conf` 消费 minix-types 的 `conf_msg`（保留槽 rsvd1/rsvd2 由构造器按 C 填 INVALID，IN-P2-2 关联项就此消解）。唤醒与读两条"拷贝横在判决与提交之间"的路径统一为 `Outcome::GrantCopy` + `complete_grant_copy`（唤醒件另有无条件解挂，input.c:365）。新增 11 条组合测试（开门登记、陈旧请求静默、未知次设备号、读的拷贝/挂起/唤醒往返、转交、灯令来源与广播、ioctl、驱动连接与离去、取消）。文档 01（§4.5 分发小节）、11（§4.3 增补）同步；main.rs 诚实占位注释更新为指向 dispatcher.rs。验证：`cargo test -p minix-input` 94 通过 0 失败；clippy 本 crate 零告警。

### IN-P1-2 "唤醒时恰好拷一个事件"的组合缺失【✅ 已完成 2026-09-15】

**是什么**：C 在 `input_process` 里，当设备有挂起读者时，以恰好一个事件的计数调用 `input_copy_events` 并立刻回信（input.c:361-364）——这是整个服务器最核心的运行时行为：读者醒来时保证拿到最新的一条事件、队列计数减一、回信值是字节数或负错误码。Rust 侧这条链路拆成了三个散件（produce.rs:157 `enqueue` → produce.rs:228 `decide_wake` → handlers.rs:181 `serve_copy`），"挂起读者恰好收一个事件"这个契约没有任何函数承载，也没有任何测试触碰（66 个测试里没有一条从 park 走到 wake）。

**连带问题**：取消匹配时 C 对**原读请求**回 `EINTR`（chardriver_reply 以原请求的 `req_id` 发 `CDEV_REPLY(EINTR, …)`，chardriver.c:255-261）；Rust 的 `InputError::Interrupted`（error.rs:73-76）在取消路径上没有消费者——唤醒组合函数正是承载这个语义的地方。

**方案对比**：
- 方案一（推荐）：新增组合函数 `wake_suspended_reader(device, event) -> 回信计划`，输入挂起读者三元组（caller/grant/req_id，structs.rs 已有）与刚入队的事件，输出"拷一个事件 + 对 req_id 回字节值/EINTR"的完整效应；配端到端纯函数测试（park → 事件到达 → count 减一 → 回信值断言）。
- 方案二：不新增函数，由传输落地时在主循环里手工串联三步。问题：最核心的契约退回"注释里的知识"，正是这轮审查发现的最大测试盲区，违背本 crate"判决皆纯函数、皆可测"的既有风格。

**修复记录（2026-09-15，方案一落地，形状按 C 语义细化）**：produce.rs 新增两段式组合——`wake_on_event`（入队 + 判断 + 规划：挂起分支产出 `WakeAction::AnswerReader { caller, request_id, plan }`，plan 是恰好一个事件的 `ReadCopyPlan`；**队列此刻不推进**，因为 C 只在拷贝成功后推进，input.c:153-154）与 `complete_answered_reader`（传输尝试后收尾：成功则提交队列并以字节数回信，失败则以错误回信，两种结局都无条件复位挂起——input.c:365 的 C 行为）。选择者分支无授权拷贝，通知加擦名一步完成（input.c:367-369）。连带：handlers.rs 新增 `cancel_parked_read` 组合——匹配则产出 `CancelledRead` 信封（联系方式 + 语义上承载 `InputError::Interrupted`，该变体自此有了消费者），不匹配则不产生任何回答（EDONTREPLY 语义）；取消路径没有授权拷贝，故一步完成。测试新增八个（恰拷一个、空队列直送、失败保队列、选择者、无人、满队溢出、取消匹配、取消不匹配），其中"拷贝失败队列不动、读者照走"是 C 失败路径的首次锁死。文档 07（§3.1）、08（§1.3 关联、§4.2、§5）、09（§4.3、§5）同步。验证：`cargo test -p minix-input` 77 通过 0 失败；clippy 本 crate 零告警。

### IN-P1-3 拷贝失败不推进的挂钩点缺失【✅ 已完成 2026-09-15】

**是什么**：C 的 `sys_safecopyto` 失败时返回错误码且 `tail`/`count` 原地不动（input.c:144-151）——事件留在队列里，读者下次重试。Rust 的 `serve_copy`（handlers.rs:181-187）先计划（`plan_copy`）后无条件执行（`apply_copy`），传输失败分支不存在，"先拷后推"只存在于注释（handlers.rs:183-184、eventbuf.rs:89-90）。传输接线（E-INWIRE）落地时如果直接调 `serve_copy`，会把"拷贝失败但队列已推进"的丢事件 bug 直接写进生产路径。

**方案对比**：
- 方案一（推荐）：`serve_copy` 改为返回"拷贝计划 + 待回信值"，把 `apply_copy` 的调用权交给传输层——传输成功才推进。计划/执行分离的原料已经存在（eventbuf.rs:65 与 :91 本来就是两个函数），只是被 `serve_copy` 焊死了。
- 方案二：`serve_copy` 接受传输回调闭包，内部先拷后推。保留单函数入口，但把传输细节引入纯函数签名，测试需要 mock 闭包，可测性下降。

**修复记录（2026-09-15，方案一落地）**：`serve_copy` 删除；eventbuf.rs 新增 `plan_read_copy`（只读设备引用，产出 `ReadCopyPlan { plan, bytes }`——分段几何加按字节计的回信值）与 `commit_read_copy`（按值消费规划后推进，同一份规划推不动两次队尾）。"失败的拷贝拿不到推进许可"从注释纪律升级为类型纪律：规划只读（拒绝即设备一个比特不动），推进必须交出规划。新增测试三个（规划只读、拒绝不碰设备、按值消费推进一次）；同步文档 07（§3.1/§4.4/§4.5/§5）与 `.design/07-design.v1.md`。验证：`cargo test -p minix-input` 69 通过 0 失败；clippy 本 crate 零告警。

---

## 2. 架构与设计（P2/P3：卓越度）

### IN-P2-1 `serve_copy` 返回单位分歧【✅ 已完成 2026-09-15】

C 的 `input_copy_events` 返回**字节数**（`event_size * event_count`，input.c:156），这是 CDEV 读请求的回信语义；Rust `serve_copy` 返回**事件数**（`plan.event_total()`，handlers.rs:181-187）。两套单位数值上常常不同（一个事件 20 字节），传输落地时若把事件数当字节数回给 VFS，就是静默错值。签名层没有任何提示。

**方案对比**：
- 方案一（推荐）：返回值改 newtype（`EventCount`/`ByteCount` 各自表达），或直接返回字节数对齐 C。类型化方案让"单位"成为编译期事实，与项目既有 `DeviceMinor`/`RequestId` 风格一致（protocol.rs:148/:155 同款做法）。
- 方案二：维持事件数、在签名与文档双向加粗声明。零改动成本，但防线只有注释。

**修复记录（2026-09-15，方案一落地，与 IN-P1-3 同一轮——同一函数契约无法分两次改）**：`EventCount`/`ByteCount` newtype 落在 eventbuf.rs；`decide_read` 的 `Serve` 变体携带 `EventCount`（钳制后的胃口），回信字节值由 `plan_read_copy` 规划期算好，分发层不再手工乘事件大小。`EVENT_BYTES` 常量从 handlers.rs 移到 structs.rs（与 `EVENT_BUFFER_SIZE` 同居的布局事实）。文档 07（§3.1/§4.1/§4.4/§5）同步。

### IN-P2-2 事件码词汇的 crate 归属（归属迁移）【✅ 已完成 2026-09-15】

**修复记录（2026-09-15，方案一落地，`[ARCH: New]` 三处一致）**：`event.rs`（事件格式与小枚举）与 `key_codes.rs`（215 键码，机械生成）整体迁入 `os/libs/minix-types/src/ipc/{input_event,key_codes}.rs`，逐字未动（值、测试名、生成器注释全部保留，8 个测试随迁）；输入服务经 `minix_input` 的 re-export 消费，crate 内 `crate::event`/`crate::key_codes` 引用路径全部改指 `minix_types`。随迁时删除了原 event.rs 的三个"声明式副本"常量（`DEVICE_TYPE_KEYBOARD/MOUSE`、`INVALID_INPUT_ID`）——`minix-types` 的 `INPUT_DEV_KBD/INPUT_DEV_MOUSE/INVALID_INPUT_ID` 本就是权威，迁移后第二轮副本失去存在理由（这正是单一权威收敛的意义：副本只会活一轮）。`[ARCH]` 标注三处：文档 04 frontmatter 与 §4（理由与位置）、`ipc/input_event.rs`/`ipc/key_codes.rs` 模块文档、两侧代码路径。pckbd 侧的本地重述清理归 edge E-PCKBDREG（16-stage 域）。验证：`cargo test -p minix-input` 86 通过、`-p minix-types` 220 通过（含随迁 8 个）、clippy 零告警。

### IN-P2-2 事件码词汇的 crate 归属

**是什么**：C 的 `minix/include/minix/input.h` 是三方共享头——input 服务器（事件格式与常量）、pckbd（产生事件码）、TTY 的 keyboard.c（消费 `INPUT_PAGE_KEY` 过滤与 `NR_SCAN_CODES` 边界，keyboard.c:148-176）。Rust 把 20 字节事件格式与 215 个 `INPUT_KEY_*` 键码放在了 `os/servers/input/src/event.rs` 与 `key_codes.rs`（文档 04 的 frontmatter 如此声明）。后果已经在发生：pckbd 侧开始本地重述常量（scancode.rs 的 `KEY_ESCAPE`/`KEY_ENTER`、bridge.rs:18-27 的设备类型常量），16-stage 的 TTY 将来要么重复定义 215 个键码、要么让驱动 crate 依赖一个服务器 crate——后者是层级倒置。

**方案对比**：
- 方案一（推荐）：事件格式与事件码词汇迁入 `minix-types`（与 `ipc/input.rs` 同居），server/pckbd/tty 三方消费。这与项目既有的单一权威收敛先例同构（SYS_ 调用号收敛 E-MINTYPES-SYS、REQ_* 契约收敛 E-REQWIRE 都是把分散定义收到 minix-types）。属架构演进，需 `[ARCH: ...]` 三处一致（文档 04 frontmatter、minix-types 模块文档、两侧代码）。
- 方案二：维持 server crate 所有，驱动依赖 `minix_input` lib。层级倒置（驱动 → 服务器依赖），且 minix-input 是 bin+lib 双目标 crate，被驱动依赖会拖入无关编译单元。
- 方案三：各 crate 自持一份。违背单一权威，现状的零散重述就是方案三的演化结果，应制止而非追认。

### IN-P2-3 组合层设计未定（效应输出模式）【✅ 已完成 2026-09-15】

**是什么**：这个 crate 的全部生产代码是 `decide_*`/`apply_*` 纯函数对，但"处理一条消息产生的全部对外效应"（回信、转发 TTY、向驱动发 LED 命令、唤醒 selector）没有统一的表达。谁在什么时机执行效应，是传输落地前必须裁决的接口契约——裁决晚了，主循环（IN-P1-1 第 1 项）就会把效应调用硬编码在自己身上，将来难以测试。

**方案对比**：
- 方案一（推荐）：效应列表模式——决策层输出 `Vec<Effect>`（`Reply { req_id, status }` / `ForwardTerminal(msg)` / `SendLeds(ep, mask)` / `SelectNotify(ep, minor)` 之类的枚举），主循环只做"收消息 → 调决策 → 逐条执行效应"。纯决策面完全不变（现有 `decide_wake` 的 `WakeDirective`（produce.rs:202）已经是这个形状的雏形），测试断言效应列表即可，无需传输 mock。这与 Linux 把"中断上半部/下半部/消费者"分开的分层动机一致：决策与执行分离，各自的测试都不需要硬件。
- 方案二：minix-chardriver 式 `CharServer` + trait 回调，效应在回调里直接执行（driver.rs:326 的形状）。与 E-CDRCONV 的收敛方向耦合——若收敛裁决为"input 迁入共享库"，此方案顺路；代价是效应路径离开纯函数世界，回信/转发都要 mock 传输才能测。
- 方案三：C 式在主循环里直接写。与本 crate 风格冲突，不赘述。

**与 Redox 的对照**：Redox 的 `inputd` 是单一 daemon 直接实现事件流整合与键盘布局（[2026-01 月报](https://www.redox-os.org/news/this-month-260131/)），没有决策/执行分离——它的规模（一个整合器）撑得住直接写；input 服务器的判决面（十个槽位、挂起读、选择器、LED 记忆、驱动生命周期）比它大，分离的收益为正。

**修复记录（2026-09-15，方案一落地）**：新增 `effects.rs` 模块——四种效应的统一出口词表：`ReplyTask`（普通任务回信，值分 `ReplyValue::Bytes`/`Code` 两单位）、`ReplySelect`（查询就绪通知）、`SendDriverAsync`（对驱动的发后不管：配置与灯令，`asynsend3 AMF_NOREPLY` 纪律）、`SendTerminalBlocking`（对终端的阻塞单向：事件转交与握手，阻塞理由同 `inputdriver.c:65-73`）。每条效应自带 minix-types 构造器装好的线上消息（m_source 留白由传输回填），构造函数一一对应 C 的发送点（input.c:231/:419/:514-523/:522/:672-677）。领域产出到效应的映射（`ConnectReport → input_conf`、`ForwardedEvent → tty_event`、`CancelledRead → reply_interrupted` 等）都是带 C 锚点的构造函数；唤醒路径保持 R2 的两段式领域 API（其回信在 `complete_answered_reader` 产出后果后由分发层装成 `ReplyTask`），不做强行归一。`Effect` 不派生相等比较（Message 含 union，填充域无语义），测试改为对解码后的载荷断言。文档 02 增补 §4.5 与 §5（效应六测试入表）。验证：`cargo test -p minix-input` 83 通过 0 失败；clippy 本 crate 零告警。

### IN-P3-1 死代码与仅测试消费项（三类处置）

前提事实：仓库内没有任何 crate 依赖 `minix-input`；下列各项的"消费者"判定来自全仓库 grep（2026-09-15）。按卓越度规则分三类，"真死"项消除前需说明影响，拿不准的标 OQ 上交，不擅自删。

- **等通电（保留，通电后复核）**：`Cargo.toml` 声明的 `minix-sys` 依赖（src/ 当前零 `use`，但 IN-P1-1/E-INWIRE 落地后即成为真实边）；connect.rs / produce.rs / init.rs 的全部出口（驱动生命周期与事件路由是通电主路径）；`ConnectReport`（IN-P1-1 第 5 项会消费它）；main.rs 的空 `loop {}`（通电时删除）。
- **真死（建议消除）**：lib.rs:83 的 `pub fn init(){}`——空函数，main.rs 不调它，全仓库零调用；handlers.rs:334 的 `let _ = SELECT_ERROR;`——用丢弃绑定消警告来表达"C 无此分支"，应改为注释或 `#[allow]` 加说明；event.rs 中 `GeneralDesktopCode`/`ButtonCode`/`ConsumerCode`/`EventPage`/`PressState`/`ValueMode` 六个枚举仅自身测试消费——若 IN-P2-2 迁移裁决为"迁 minix-types"，随迁移一并安置，不单独删。
- **OQ 上交**：key_codes.rs 的 215 个键码常量生产消费者为零——它们是 IN-P2-2 迁移的主体，删与留随迁移裁决，不单独处置；`NO_SLOT`（connect.rs:228）仅自身测试引用——随 IN-P1-1 第 5 项落地复核。

### IN-P3-2 边界测试族缺口

以下分支当前无测试（66 个测试未触达，2026-09-15 grep 核对）：`plan_copy` 的两个极限回绕（tail=31 取 32 个、tail=0 整环一次拷，eventbuf.rs:65）；鼠标槽位窗（槽 6-9）耗尽与"同牌复用 + opened 跳过"组合耗尽（connect.rs:58）；开门集合 256 溢出返回 false（framework.rs:279-286）；第二个 selector 挤掉第一个（handlers.rs:343-347）；断连的单侧场景（只有挂起读者、只有 selector，connect.rs:202-222）；入队落位下标 31 再回绕（produce.rs:157）；`route_event` 对 mux 槽位 id（0/5）、`stored_event` 越界截断、`set_label` 空名、`drain_ordered` 计数为零。建议随 IN-P1-1/IN-P1-2 的落地一并补齐（同一批函数的边界），不单独开轮。

### IN-P3-3 wire 测试盲区

`minix-types`/`minix-sys` 侧：`tty_up_msg` 被四个 `decode_*` 拒收（应返回 None）未测；`KIOCSLEDS` 的 size > 0xFFF 截断分支未测；`decode_conf` 只测了 rsvd1 非法、rsvd2 未测（input.rs:265-267）；通知携带 `INPUT_CONF` 消息号时的优先级（C 是"先判通知再判消息号"，inputdriver.c:144-163，现有测试只用了 `INPUT_EVENT` 作通知号，inputdriver.rs:331-348）。均为纯函数测试，补齐成本低。

---

## 3. 文档同步（P2/P3）

### IN-D1 文档测试计数过时【✅ 已完成 2026-09-15】

文档 01:295、02:251、03:272、04:261 声称"全 crate 共 28 个测试"；06:189、07:236、08:232 声称"当前 48 个"——实际 66 个。12:331 声称 minix-sys"当前 22 个"——实际全 crate 136 个（inputdriver.rs 内 6 个）。05:338 的表列 10 个测试，实有 11 个（`test_driver_key_prefix_matches_both_sides`，ipc/input.rs:519-528，不在文档表中）。测试名对账全部通过（无虚构），缺口集中在计数漂移。处置：逐篇更新计数（模式 59 字段计数漂移的常规修复）。

**修复记录（2026-09-15）**：11 篇文档的 §5 头部计数与 §5.1 统计全部按 2026-09-15 实测重写——minix-input 86（含本轮新增的 effects 6、dispatcher 11，及第 04 篇迁出 8 个）、minix-types 220、minix-sys 149；各篇"其余分属"分解行按统一口径重写（共用测试 `test_event_bytes_match_c` 归第 06 篇提及、第 07 篇不计入），每篇分解加总均等于 86，可复核。文档 05 的测试表补上第 11 个测试 `test_driver_key_prefix_matches_both_sides`（此前声称 10 个、实有 11 个）。测试名对账维持零虚构。

### IN-D2 两篇 pending 文档待写【✅ 已完成 2026-09-15】

`00-input-overview.md`（导航 + 启动主线图）与 `99-input-global-concepts.md`（常量总表、错误码汇总、跨服务引用）仍为 pending（两文件 frontmatter 自述）。99 的常量总表是 IN-P2-2 迁移裁决的文档面前提——先定权威归属，再写总表，避免写完即改。

**修复记录（2026-09-15，IN-P2-2 落地后撰写）**：00 篇以传达室比喻立"事件汇"概念，启动主线图按 Rust 现状两半划分（dispatcher 裁决就位、传输挂 E-INWIRE），事件旅程与驱动生命周期两条次主线带 C 行号，导航表 16 行逐一对应现文件；99 篇六张总表（消息号、事件格式与码、设备编号、错误码、endpoint/DS 键、全局状态与跨服务引用）每项标注权威文件与机制篇出处，CDEV_REPLY_BASE 错值显式注记。两篇均按 plan §3.6 补齐 .design 三件套（outline/outline-review/design，v1），plan.md §6.1 状态表同步为已改写。文风按 style-bible：Ch1 主语为机制，无开发文档味。

### IN-D3 close 全清偏离缺三层一致标注

Rust 的 `apply_close` 在关闭时额外清理挂起读者与 selector（handlers.rs:94-103），C 只清三项留下脏状态（input.c:120-122）——这是有意的 MINIX3 BUG 修复，注释与文档 06 都有论证（证据链 filedes.c:453 核实无误）。但按项目三层术语，这是改变外部可观察行为的架构演进，应按 `[ARCH: ...]` 格式在 doc + design + code 三处一致标注（现在代码只有 "MINIX3 BUG" 字样，无标准格式）。处置：补标注，不改行为。

### IN-D4 语义映射表与文档互引缺口

`tools/coverage-extract/input-semantic-map.json` 自述只覆盖文档 01-12（文件头 `_note`），13（TTY 契约）与 14（pckbd 契约）未入表——两篇是外部契约篇，映射表应覆盖其契约符号面（`do_input`、`TTY_INPUT_EVENT` 消费字段、`pckbd_init` 的 flags 生成等），否则覆盖度检查对这两篇恒报缺失或恒豁免，两失。另：14 篇自称"Rust 模块： 无"（14-pckbd-driver.md:6）而 `os/drivers/hid/pckbd`（892 行）已存在且由 16-stage 的 13-pckbd-driver.md 承载——两篇互不回指，读者无从知道契约篇与实现篇的分工。处置：映射表补 13/14 契约符号；14 篇 frontmatter 或 §1 补一行"实现篇见 ../../16-stage-drivers/13-pckbd-driver.md"。

---

## 4. 本轮明确未发现（防止误以为漏查）

- **无 P0**：五个消息的消息号/字段序/字段宽全对齐；`input.c` 无行为性遗漏（除登记于 IN-P1-1 的两处局部缺失，均有文档 defer 依据）；测试名对账零虚构。
- **错误码**：`InputError` 十变体经 `to_errno` 单点映射且数值有测试锁（error.rs:86-119），无自造错误码。
- **no_std**：生产代码无 `std::` 引用；`unsafe` 在三组件生产代码中零出现。
- **SMP 约束**：input 是单线程事件循环服务器，无跨 CPU 共享面，模式 26-29 不适用。
- 漏检自检：本轮 P0 为零但 P1 三条、P2 四条非零，按收敛规则无需触发"随机抽三项重跑"。

## 5. 规则发现（Step 5.7）

本轮沉淀一条候选模式（待 review-patterns 收编评审）："**孤儿共享库**"——共享基础设施库建成之后没有任何生产 crate 依赖它，而消费方各自内嵌了等价实现（minix-chardriver 零依赖方 + framework.rs 平行判决；pckbd InputBridge + minix-sys DriverRegistration 双轨）。判定信号：`grep -rl {lib} os/*/*/Cargo.toml` 仅命中 workspace 成员声明。它与模式 80（为 mock 预建抽象）互补：模式 80 管的是"抽象没有真实实现"，本条管的是"实现没有真实消费者"。
