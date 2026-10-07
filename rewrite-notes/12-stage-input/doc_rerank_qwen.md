# 12-stage-input 文档重建蓝图（qwen）

> 本文件是 R 相（重建蓝图）交付物，只做蓝图设计，不改任何正文。B 相（重建相）按本蓝图逐篇取料重写。
> 本文不引用 `.design/` 与 `tmp_design_and_todo/`（项目规范：中间产物，正式文档引用即违规）；所有事实断言带 C 源码或 Rust 代码锚点，给不出锚点的判断显式标注"推测/待验证"。

---

## 0. 元数据

- **执行者**：qwen
- **日期**：2026-09-19
- **目标目录**：`rewrite-notes/12-stage-input/`
- **仓库根目录**：`/home/xzhao/github/minix-rs`
- **当前提交号**：`40dfbbeb9`

### 0.1 审查范围

- **算作文档**：编号文档 `00`、`01`–`14`、`99`（共 16 篇，均在册）。
- **算参考材料**（范围外正文，只作线索不作搬迁对象）：`plan.md`、`todo.md`、`draft/README.md`。
- **排除**：`doc_rerank_deepseek.md`、`doc_rerank_glm.md`（其它执行者产物，按约束不读不抄——本蓝图的每一条结论都由本执行者对 C/Rust 源码独立取证，见 §0.3 命令证据）。

### 0.2 读取清单

- **文档**：`00-input-overview.md`（全）、`99-input-global-concepts.md`（全）、`01-input-init-main.md`（全，含 §4.5）、`02-chardriver-framework.md`（全）、`plan.md`（全）、`todo.md`（全）；`03`–`14` 抽读头部声明与 Rust 模块引用行。
- **C 源码（ground truth）**：`minix3/minix/servers/input/input.c`（704 行，全读）、`input.h`（45 行）；核对了 `input.c` 全部 20 个函数定义行。
- **Rust 实现**：`os/servers/input/src/`（13 文件、4874 行，实测）、`os/libs/minix-chardriver/src/{driver,protocol}.rs`（判定核新权威）、`os/servers/input/src/{lib,main,serve}.rs`（全读或读关键段）。
- **边界材料**：`plan.md` §1.2/§5、`todo.md` §0/§1、`../edge_todo.md`（E-INWIRE / E-CDRCONV 接线状态，经 Rust 代码注释间接核对）。

### 0.3 命令证据（关键输出摘录）

```text
# Rust 模块现状：framework.rs 已不存在，serve.rs 是新增模块
$ ls os/servers/input/src/
connect.rs  dispatcher.rs  effects.rs  error.rs  eventbuf.rs  handlers.rs
init.rs  lib.rs  main.rs  produce.rs  serve.rs  setleds.rs  structs.rs   # 无 framework.rs

# 真实测试数：91（不是文档里的 98）
$ grep -rhcE "#\[test\]" os/servers/input/src/*.rs | 逐项相加
connect 11 / dispatcher 11 / effects 6 / error 2 / eventbuf 10 / handlers 17
init 5 / produce 17 / serve 2 / setleds 4 / structs 6 = 91
$ grep -rn "共 98 个测试" 12-stage-input/[0-9]*.md   → 01:299、02:257、03:272 三篇仍称 98

# serve.rs 已把传输与主循环落地（E-INWIRE 完成），与文档"诚实停车/空转"叙述冲突
#   main.rs:39   minix_input::serve::serve(&mut transport, self_ep, &mut server);
#   serve.rs:1   //! Transport seam + serve loop — the E-INWIRE loop shell.
#   serve.rs:66  pub struct KernelTransport { ipc: DirectTrapTransport, kernel: DirectKernelCallTransport }
# 而文档仍称传输未落地：00:40、00:91、01:280、01:284

# framework.rs 迁移到新权威后的真实符号形状（doc 02 正文用的名字多已不存在）
$ grep -rn "decide_reply\|may_park\|CharacterRequest" os/ --include=*.rs   → 0 命中
$ grep -n "pub enum Route\|pub enum ReplyDecision\|pub const fn classify\|pub fn reply_decision" \
      os/libs/minix-chardriver/src/driver.rs
   Route{Notify,BlockOpen,Stale,Request,Other}  ReplyDecision{Reply,Parked,SwallowedRestart}  # 三变体，无 InvalidParking

# 断链成本：篇号交叉引用密集
$ grep -rohE "第 [0-9]{2} 篇" 12-stage-input/[0-9]*.md | wc -l  →  666
```

---

## 1. C 真序（运行时真序重建）

**阶段类型判定**：`input` 是**服务事件循环型**。理由：`main`（`input.c:696-704`）先 `input_startup()` 登记回调，再进入 `chardriver_task(&input_tab)`（`input.c:701`）收信循环——存在清晰的"启动段"与"循环段"两段结构，不是线性启动链（那是 `01-stage-kernel`），也不是纯系统调用集合。以下真序直接从 `input.c` 逐函数重建，不从 `plan.md`/`overview.md` 转述。

### 1.1 启动段

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| S1 | `main` 调 `input_startup` | `input.c:696-704` | 进程入口 |
| S2 | 登记首次启动回调 `sef_setcb_init_fresh(input_init)` + `sef_startup()` | `input.c:685-691` | SEF 生命周期 |
| S3 | `input_init` 清十个槽位（回填 minor） | `input.c:652-662` | `devs[i].minor = input_revmap(i)` |
| S4 | 订阅驱动到来事件 `ds_subscribe("drv\.inp\..*", DSF_INITIAL)` | `input.c:665` | 失败即 `panic` |
| S5 | 向文件系统宣告 `chardriver_announce()` | `input.c:669` | 框架面（三效果，见 §3 越界项） |
| S6 | 向终端握手 `TTY_INPUT_UP`（`ipc_send`） | `input.c:672-677` | 唯一握手 |

### 1.2 循环段（`chardriver_task` → `chardriver_process` 三判决 → 分派）

| 步 | 到达类型 | C 处理函数（锚点） | 落位文档 |
|----|---------|-------------------|---------|
| L0 | 收一封信（`sef_receive_status(ANY)`） | `chardriver.c:549-573` | 02 |
| L1 | 分类：通知 / 块开门 / 字符请求 / 其它 | `chardriver.c:455-536` | 02 |
| L2 | 重启门卫（登记簿判定） | `chardriver.c:54-94` | 02 |
| L3 | 回信纪律（普通 / 事后 / 不回的哨兵） | `chardriver.c:195-274` | 02 |
| L4 | `CDEV_OPEN`/`CLOSE` | `input_open:85-105` / `input_close:107-127` | 06 |
| L5 | `CDEV_READ` → 拷贝/挂起 | `input_read:162-201` → `input_copy_events:130-160` | 07 |
| L6 | `CDEV_IOCTL`（`KIOCSLEDS`） | `input_ioctl:241-280` → `input_set_leds:204-239` | 08/10 |
| L7 | `CDEV_CANCEL` | `input_cancel:282-301` | 08 |
| L8 | `CDEV_SELECT` | `input_select:303-330` | 08 |
| L9 | 其它信箱 `input_other`：DS 通知 / `INPUT_EVENT` / `INPUT_SETLEDS` | `input_other:608-641` | 05 入口，分发到 09/10/11 |
| L9a | 事件旅程：`input_event`（id/owner 校验、mux 选择、TTY 转发）→ `input_process`（溢出覆盖、入队、唤醒） | `input.c:376-428` / `332-374` | 09 |
| L9b | 灯令：`input_set_leds` 广播 + 记忆 | `input.c:204-239` | 10 |
| L9c | 驱动生命线：`input_check` → `input_connect`/`input_alloc_id`/`input_disconnect` | `input.c:558-606`/`475-531`/`430-473`/`533-556` | 11 |

**结论**：C 真序与现有 16 篇编号顺序（01 启动 → 02 框架 → 03/04/05 结构与协议 → 06/07/08 操作 → 09/10 事件与灯 → 11 驱动生命线 → 12 客户端库 → 13/14 外部契约 → 99 收口）**一致**。逐函数行号与 `plan.md §5.3` 表核对基本吻合（`input.c` 20 个函数定义行全部对齐；仅 `input_init` 结束行 `plan` 记 683、实为 680，属可忽略的行尾漂移）。

---

## 2. 知识点全集（存量池 + 增量）

现有 16 篇文档已在 2026-09-04/05 逐篇 review、2026-09-15 补齐 00/99，且本蓝图判定其 **C 行为层语义准确**（§1 真序、`plan.md §5.3` 逐函数映射均经本执行者核对 `input.c` 成立）。因此本蓝图**不重排、不合并、不删除任何存量知识点**——存量池原样保留在各自篇章，覆盖天然完整（见 §9 G5）。下面只做两件事：（a）逐篇一行的存量清单（证明"没有知识点被丢弃"）；（b）列出本轮审计出的**增量**与**失真**条目。

### 2.1 存量清单（一文档一行）

| 篇 | 承载的知识点主题（存量，位置不变） |
|----|-----------------------------------|
| 00 | 事件汇概念、传达室比喻、三条边界、启动主线图、两一次主线、设计原则四条 |
| 01 | main/startup/init 四步、清表循环、订阅、宣告、握手、`input_tab` 注册表、§4.5 主循环裁决面 |
| 02 | 三判决（分类/门卫/回信纪律）、信封格式、请求 7 回信 3、EDONTREPLY/ERESTART、open_devs 门卫、announce 三效果、效应清单四条 |
| 03 | `struct input_dev` 十三字段、`devs[10]`、map/revmap、minor↔下标两套编号、三个宏 |
| 04 | 20 字节事件格式、事件页/值/标志、215 键码、LED/按钮/消费类、HID Usage 对齐（权威已迁 minix-types） |
| 05 | 五消息号与 BASE 段、四消息结构字段、全单向协议、`INPUT_MAJOR=64`、/dev 节点表、system.conf 权限 |
| 06 | open（ENXIO/EBUSY/营业检查）、close（含 MINIX3 BUG 修正）、active 语义 |
| 07 | read（三段式规划/传输/提交、挂起、EAGAIN/EIO、EDONTREPLY）、copy_events 回绕分段、挂起字段 |
| 08 | ioctl（KIOCSLEDS 位翻译）、cancel（三重匹配 EINTR/EDONTREPLY）、select（就绪/挂起/查询、WR 恒就绪） |
| 09 | event（id/owner 校验、mux 选择、TTY 转发）、process（溢出覆盖最旧、入队、唤醒两段式） |
| 10 | set_leds 广播与单键盘匹配、`dev->leds` 记忆、跨重启恢复 |
| 11 | check（新增/离去检测）、connect（label 校验/CONF/LED 恢复）、alloc_id、disconnect、DS 订阅契约 |
| 12 | inputdriver_announce/send_event/do_conf/do_setleds/process/task/terminate、回调表 |
| 13 | TTY 侧 do_input（UP 握手/EVENT 消费）、set_leds 请求、input_endpt、消息接入点 |
| 14 | pckbd announce flags、键盘/鼠标事件上报、pckbd_leds 端口、intr/alarm、主循环 |
| 99 | 六张总表（消息号/事件格式与码/设备编号/错误码/endpoint与DS键/全局状态与跨服务引用） |

### 2.2 增量知识点（现有文档缺席，代码/制品承载——来源：Rust 代码实证）

| 编号 | 名称 | 类型 | 来源类型 | 证据锚点 | 读者收益 |
|------|------|------|---------|---------|---------|
| K-N1 | 传输缝与运行循环落地（E-INWIRE 完成） | 机制/架构演进 | 新增（代码已有、文档未讲） | `serve.rs:1-24,137-200`；`main.rs:36-39` | 理解"判决如何被真正执行、进程怎么跑起来" |
| K-N2 | 生产传输双硬件腿聚合 `KernelTransport` | 数据结构/架构 | 新增 | `serve.rs:56-135`（IPC 腿 + SYSCALL 腿） | 理解一个 C 进程对应 Rust 里两条 trait 腿 |
| K-N3 | minix-sef 切换点约定 | 约束/不变量 | 新增 | `serve.rs:8-12,63-65`；`main.rs:32-35` | 理解将来 SEF 生命周期接管时唯一改动点 |
| K-N4 | C 循环收尾的分派尾（收信→classify→dispatch→reply） | 机制 | 新增 | `serve.rs:137-162`（`classify`、`is_notify`） | 把 02 的"框架契约"与真实可执行循环对上 |
| K-N5 | 回信信封手工装配（`CDEV_REPLY 0x480`/`CDEV_SEL2_REPLY 0x482`） | 接口与协议/线格式 | 新增 | `serve.rs:35-38,170-200` | 理解回信字节布局与 ipc.h 栏位对齐 |
| K-N6 | 宣告执行落地（DS 发布 `drv.chr.` + CLEAR_IPC_REFS + 阻塞 `TTY_INPUT_UP`） | 机制 | 新增（01 只讲计划，不讲执行） | `serve.rs:20-23`；对照 `input.c:669,672-677` | 理解 01 的 init 计划在传输层真正发生的时刻 |

### 2.3 失真条目（现有文档与代码真相不符——须就地更正）

| 编号 | 断言（旧） | 位置 | 真相（本轮实测） |
|------|-----------|------|-----------------|
| K-E1 | "传输层未落地 / 服务进程诚实停车 / 空转等待" | 00:40、00:91、01:280、01:284 | `serve.rs` 已落地传输与循环，`main.rs:39` 真实调用 `serve::serve` |
| K-E2 | "完整代码在 `framework.rs`"、`decide_reply`/`may_park`/`CharacterRequest` | 02:214、257、301、§3.4/3.5/4.1/4.3 | `framework.rs` 已删除；权威在 `minix-chardriver`，真名 `classify`/`reply_decision`/`CdevRequest`；`may_park`/`decide_reply`/`CharacterRequest` 全 0 命中 |
| K-E3 | "回信判决有 4 变体含 `InvalidParking`" | 02 §3.5、§4.3 | 真 `ReplyDecision` 仅 3 变体（`Reply`/`Parked`/`SwallowedRestart`）；违规赊账折成 `Reply(EINVAL)`（`driver.rs:96-101`） |
| K-E4 | "全 crate 共 98 个测试" | 01:299、02:257、03:272 | 实测 91；且 02 的框架测试已随判定核迁到 minix-chardriver（23 测试），不在本 crate 计数内 |
| K-E5 | 测试分布分解把 framework.rs 记 9 个 | 01:314、02:280 | 框架测试不在 input crate；新增 serve.rs 2 测试未计；分解须重算 |
| K-E6 | 锚点 `input.c:input_other（L646）`（称"本篇全部 C 依据"） | 01:5、332 | `input_other` 实为 `input.c:608`；`L646` 是 `input_init`。符号名与行号张冠李戴 |

---

## 3. 覆盖审计（防遗漏、防重复、防越界）

### 3.1 主题全集来源

四路：（1）`input.c`/`input.h`/`chardriver.c`/`inputdriver.c` 符号；（2）对应通用 OS 概念（事件多路复用、挂起读、环形缓冲、重启代际隔离、驱动注册状态机）；（3）非 C 制品（`system.conf`、`MAKEDEV.sh`、`com.h`/`ipc.h`、`kbdio.h`/`ttycom.h`）；（4）边界契约（DS/TTY/pckbd/RS/VFS 使用面）。这些主题在 `plan.md §5` 已逐项落位，本执行者对 `input.c` 复核成立——故**不重复展开覆盖矩阵，只报差异**。

### 3.2 覆盖缺口表

| 缺口 | 主题 | 现状 | 建议 |
|------|------|------|------|
| **G-1（主要）** | 传输缝 + 运行循环（K-N1…K-N6） | `serve.rs` 330 行真实落地，12 篇文档 0 引用；且 00/01 反而断言"未落地/停车"（K-E1） | **新建一篇**专述运行装配（见 §4/§5），并在 00/01 更正为指向新篇 |
| G-2 | 判定核迁移现状 | 02 头部有 `[ARCH]` 免责（第 3 行），正文却仍把 framework.rs 当实现之家（K-E2/E3） | **就地重写** 02 §3/§4/§5，对齐 `minix-chardriver` 真实符号与三变体形状 |
| G-3 | 测试基线数字 | 98/91 双值、分布分解失真（K-E4/E5） | **就地更正** 01/02/03 §5 计数与分解 |
| G-4 | 符号/行号锚点 | `input_other（L646）` 张冠李戴（K-E6） | **就地更正** 01:5、332 为 `input_init`（646）与 `input_other`（608）分列 |

无"该讲未讲"的行为性缺口（C 侧 20 函数、协议面、客户端库、外部契约均有归属，见 §2.1）。

### 3.3 重复主题表

| 主题 | 重复处 | 主讲述点 | 其余处理 |
|------|--------|---------|---------|
| 全单向消息协议 A-9 | 02 §4.5 / 05 / 99 | 05（协议面为数值权威） | 02/99 改为引用 05 |
| 事件 id==数组下标 A-3 | 00 §1.4 / 03 / 09 | 09（校验发生处） | 00/03 引用 09 |
| CDEV 回信基址 0x480 | 02 §2.2 / 99 §2 / serve.rs | 99（总表）+ 新装配篇（实现） | 02 只讲请求基址 0x400 |

均为"导航层提及 + 权威层展开"的正常引用式重复，不构成合并动因。

### 3.4 越界主题表

| 篇 | 越界主题 | 正确归属 |
|----|---------|---------|
| **02** | 判定核（classify/gate/reply_decision）的**实现语义** | 判定核已于 2026-09 迁入共享 crate `os/libs/minix-chardriver`，其 crate 内部完整语义按 E-CDRCONV 归 **16-stage-drivers**。02 应收敛为"input 作为消费者对前台三判决的**使用面** + C 行为底线（`chardriver.c`）"，不再充当判定核的实现手册。 |
| 01 §4.5 | 主循环"机械的一半"传输执行 | 交给新装配篇（G-1）；01 只保留 init 计划与裁决面指针 |

这是本轮审计除 G-1 外最重要的结构性发现：**文档 02 的责任边界因代码迁移发生了漂移**——判定核被抽走后，02 若继续按"框架实现之家"来写，就会既与 minix-chardriver 的权威文档重复、又与真实代码不符。

### 3.5 非 C 制品逐项回答（固定清单）

| 制品主题 | 在哪讲 / 为何不在本 stage |
|---------|--------------------------|
| 链接与加载 | 不在本 stage：input 由 RS 运行时加载（`system.conf:400-403`），链接脚本属内核/引导阶段。05 只登记 RS 权限事实。 |
| 镜像与内存布局 | 不在本 stage：无独立镜像，用户态 ELF 由 RS 装载。 |
| 汇编入口与陷阱进入 | 部分：收信走 int-33 IPC 腿（`serve.rs` 的 `DirectTrapTransport`），**新装配篇 K-N2 讲**；trap 入口本体属内核 stage。 |
| 启动装配 | 在 01（init 计划）+ **新装配篇**（宣告在传输层的真实执行，K-N6）。 |
| 构建与工具链 | 不在本 stage 展开（Cargo workspace 约定）；键码生成器归 04（`key_codes.rs` 机械生成）。 |
| 跨模块接口与线格式 | 在 05（协议）+ 99（总表）；**新装配篇补回信信封字节布局**（K-N5）。 |
| 错误路径 | 在 03/06/07/08 + 99（错误码汇总表）。 |
| 关闭与退出 | 在 06（close）+ 12（terminate）；运行循环退出路径（`running`/EINTR）归**新装配篇**。 |
| 并发与同步 | 不适用：单线程事件循环（`lib.rs:11-16` 明确 no Arc/Mutex/atomics）。 |
| 测试基建 | 各篇 §5；本轮须按 91 重算（G-3）。 |

---

## 4. 新目录

### 4.1 战略裁决：定向重建（零重编号），不全局重排

依据 §0.3 断链成本：篇号交叉引用 **666 处**（16 篇，均值 ~42/篇，`11`/`05`/`08`/`01` 为热点）。全局重编号需改写全部 666 引用 + 代码注释回指，成本压倒收益；且现有编号已满足四条硬标准（§5.1）：顺序合 C 真序、无前向引用、单篇单语义、首次出现即完整。**决定：保留 00–14、99 全部编号与位置不变**，仅：

- **新建 1 篇**（承载 G-1 的传输/运行循环，K-N1…K-N6）——按仓库既有先例（04-stage-pm 加 `21-*-wire-codec`）取**尾部编号 `15`**，不挤占既有号；
- **就地重写/更正 4 篇**：`00`（导航与主线叙述）、`01`（§4.5 + 计数 + 锚点）、`02`（判定核迁移后的责任收敛 + 符号对齐 + 计数）、`03`（计数）；
- 其余 `04`–`14`、`99` **不动**（除交叉引用新篇的可选增补）。

### 4.2 新目录总表

| 编号 | 标题 | 一句话定位 | 状态 |
|------|------|-----------|------|
| 00 | input 整体架构概览 | 全文档导航与启动/事件/驱动三条主线图 | 更正（主线图去"停车"） |
| 01 | 进程怎么诞生 | init 四步 + 裁决面就位 | 更正（§4.5/计数/锚点） |
| 02 | 字符驱动框架契约 | input 对前台三判决的使用面 + C 底线 | 重写（收敛为使用面） |
| 03 | 十槽两编号 | 设备结构与两套编号 | 更正（计数） |
| 04 | 20 字节事件格式与事件码 | 共享词汇（minix-types） | 不动 |
| 05 | 五种消息与全单向协议 | 协议数值权威 | 不动 |
| 06 | 打开与关闭 | open/close 判决 | 不动 |
| 07 | 读与挂起 | 三段式读 + 挂起状态机 | 不动 |
| 08 | 控制/取消/查询 | ioctl/cancel/select | 不动 |
| 09 | 事件旅程核心 | 路由/入队/唤醒 | 不动 |
| 10 | 灯令广播与记忆 | set_leds | 不动 |
| 11 | 驱动生命线 | connect/alloc/disconnect/check | 不动 |
| 12 | 驱动侧客户端库 | libinputdriver | 不动 |
| 13 | 终端消费契约 | TTY 侧 | 不动 |
| 14 | pckbd 驱动契约 | 使用面 | 不动 |
| **15** | **`15-input-serve-loop.md`（新建）** | **运行装配：传输缝与主循环，让进程真正跑起来** | 新建 |
| 99 | 全局概念收口 | 六张总表 | 更正（可选：回指 15） |

### 4.3 阅读路径

- **主线**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11（服务本体，与 C 真序一致）。
- **换视角支线**：12（驱动侧）→ 13 → 14（两个邻居的契约面）。
- **收口**：99。
- **装配支线（新）**：**15 建议在 11 之后、12 之前阅读**（读者已握有全部判决与效应，此处看它们如何被传输层执行成一个进程）。15 依赖 01–11，无任何篇依赖 15，故**不产生前向引用**（编号尾置只是编号技巧，非阅读序倒退）。

### 4.4 序差表（运行时序事实 vs 教学序选择）

| 语义 | 运行时事实（锚点） | 教学序选择 | 回指补偿 |
|------|-------------------|-----------|---------|
| 传输/循环 | 编译期在 `main` 最早就绑好，运行期与判决交替发生（`main.rs:21-40`、`serve.rs`） | 延后到 15（学完判决再看装配） | 01 §4.5 与 02 正文用一句指针"执行面见 15" |

---

## 5. 每篇契约

> 只给**发生变化**的篇写契约；`04`–`14`（除 02）与 `99` 无正文改动，不重述契约（其现有契约在 `plan.md §3.4` 有效）。

### 5.1 15-input-serve-loop（新建）

- **一句话定位**：把前三判决与全部判决效应**接上真实收发**，回答"这个服务器到底怎么作为一个进程跑起来"。
- **讲什么**：K-N1 传输缝与循环、K-N2 `KernelTransport` 双硬件腿（IPC + SYSCALL）、K-N3 minix-sef 切换点、K-N4 收信→classify→dispatch→reply 尾、K-N5 回信信封字节装配、K-N6 宣告在传输层的执行。
- **不讲什么**：各判决的业务语义（06–11）、回信纪律的规则本体（02/minix-chardriver）、消息结构字段定义（05）；本篇只讲"执行/装配"。去向：判决 → 02/06–11；协议数值 → 05/99。
- **前置**：01（`handle_arrival`/`Outcome`/`GrantCopy`）、02（三判决与效应出口）、07（挂起读的两段式拷贝）、09（唤醒组合）。全部为更早编号，无前向引用。
- **后置**：99（可选登记 `CDEV_REPLY=0x480` 与 serve.rs 的一致性）、16-stage（`minix-chardriver` 框架文档与本篇对称）。
- **事实底线**：
  - Rust：`os/servers/input/src/serve.rs`（`Transport` trait:41、`KernelTransport`:66、`classify`:143、`is_notify`:166、`reply_task_msg`:173、`reply_select_msg`:190、`serve` 循环体）；`main.rs:21-40`；`lib.rs:53` 声明 `pub mod serve`。
  - C：`chardriver.c:455-573`（收信分发尾）、`input.c:608-641`（`input_other`）、`chardriver.c:127-174`（回信助手）。
- **知识点清单**：见 §2.2 K-N1…K-N6（每条锚点已在表内）。
- **验收标准**：读者能回答——(a) 一条到达从 `receive` 到 `Outcome`/`Effect` 到实际发出的完整数据流；(b) 为什么一个进程需要两条 trait 腿、SEF 接管时改哪一处；(c) `CDEV_REPLY`/`CDEV_SEL2_REPLY` 消息号与 ipc.h 栏位如何对齐；(d) 拷贝失败时"规划已出、推进未发"如何在 `complete_grant_copy` 收口。测试：`cargo test -p minix-input` 中 serve.rs 的 2 个测试须点名，并说明"传输装配的可测部分=classify/reply 构造，收发本身挂真实内核"。

### 5.2 02-chardriver-framework（重写：责任收敛 + 符号对齐）

- **一句话定位**：不变——input 作为消费者对字符驱动"前台"的使用面 + `chardriver.c` 行为底线。
- **讲什么**：三判决的**契约与 C 行为**（分类/门卫/回信纪律，§2 保留）；input 如何登记回调表、如何被门卫放行、回信经哪些哨兵；A-1 框架决策的**现状**（判定核单点）。
- **不讲什么**（本次关键收缩）：判定核的 **Rust 实现内部**（`classify`/`gate_character_request`/`reply_decision` 的函数体、`Route`/`GateVerdict`/`ReplyDecision` 的变体设计）——已迁 `minix-chardriver`，其权威文档归 16-stage。02 只描述"这些判决存在、语义如此、input 是消费者"。
- **前置**：01。**后置**：03/06–08、15、99。
- **事实底线**：
  - C（不变）：`chardriver.c:1-44`（协议注释）、`:455-536`（process 三判决）、`:54-94`（open_devs）、`:195-274`（reply）、`:99-124`（announce）、`com.h:915-937`。
  - Rust（**须按真名重写 §3/§4**）：`os/libs/minix-chardriver/src/driver.rs` — `classify`(:357)、`Route{Notify,BlockOpen,Stale,Request,Other}`(:47-59)、`gate_character_request`(:139)、`GateVerdict{Serve,RecordAndServe,DropAsStale}`(:124-131)、`reply_decision`(:90)、`ReplyDecision{Reply,Parked,SwallowedRestart}`(:68-77)、`announce_effects`(:167)、`ReplyPlan`(:183)；`protocol.rs` — `CdevRequest`(:41)、`OpenDeviceSet`、`MAX_OPEN_DEVICES=256`(:30)。
- **知识点更正**（对应 §2.3）：删 `decide_reply`→`reply_decision`；删 `may_park`/`CharacterRequest`/`classify_request` 旧名；删 §3.5"第四变体 `InvalidParking`"论述，改述为"违规/过期挂起 → `Reply(EINVAL)`"（`driver.rs:96-103`）；§5 框架 9 测试从 minix-input 计数移除、改注"判定核测试在 `minix-chardriver`（23 测试）"。
- **验收标准**：02 正文出现的每个 Rust 符号名都能在 `os/libs/minix-chardriver/src/` 逐字找到（消 K-E2）；不再出现 `framework.rs` 作为"实现之家"的表述（保留一句历史注记说明它已删除即可）；§2 C 分析一字不改仍成立。

### 5.3 00 / 01 / 03（就地更正，非结构变更）

- **00**：§1.3 与 §3 删除"服务进程诚实停车/空转/E-INWIRE 未落地"叙述（K-E1），改为"传输与运行循环已落地（`serve.rs`），见 15"；导航表加 15 行。
- **01**：§4.5"机械的一半…归将来的接线"改为"机械的一半已落地于 `serve.rs`，详见 15"；§5 计数 98→91 重算（K-E4）、分布分解重列并含 serve 2（K-E5）；元数据锚点 `input_other（L646）` 更正为 `input_init`（646）与 `input_other`（608）分列（K-E6）。
- **03**：§5 计数 98→91。

---

## 6. 变更表

| 操作 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 |
|------|------|--------|--------|------|-----------|------|
| OP-1 | 新建 | — | `15-input-serve-loop.md` | 传输/循环已落地却无文档（G-1） | K-N1…K-N6 | 新篇 |
| OP-2 | 改写 | 02 §3、§4、§5 | 02 同节 | 判定核迁移后正文与代码不符、责任越界（G-2/§3.4） | K-E2/K-E3 | 留在 02（收缩为使用面） |
| OP-3 | 改写 | 00 §1.3/§3、01 §4.5 | 原节 | 更正"未落地/停车"假陈述（K-E1） | — | 00/01 |
| OP-4 | 改写 | 01/02/03 §5 计数与分解 | 原节 | 98→91、框架测试迁出、serve 未计（G-3） | K-E4/K-E5 | 原篇 |
| OP-5 | 更正 | 01:5、332 锚点 | 原处 | 符号/行号张冠李戴（G-4） | K-E6 | 原篇 |
| OP-6 | 增补（可选） | 99 §2 | 99 §2 | 登记 serve.rs `CDEV_REPLY=0x480` 与总表一致 | K-N5 | 99 |

**无删除、无合并、无重排、无重编号**——存量 16 篇逐篇原位保留（§2.1 存量清单即"零丢失"证明）。

---

## 7. 缺漏新篇（G-1 落实）

- **主题**：input 的运行装配层——传输缝 + 主循环（`serve.rs`）。
- **为什么重要**：这是 todo.md §0 判定的"最大缺口——可运行实体缺席"的兑现点；代码已从"诚实停车"推进到"真收发"（`main.rs:39`），文档若不改，整套 input 文档的中心叙事（00 §3 设计原则 4"诚实占位"）就成了与代码相反的假陈述。
- **原料来源**：Rust `os/servers/input/src/serve.rs`（330 行）+ `main.rs`；C `chardriver.c:455-573`、`input.c:608-641`；无旧文档段落（属新增方向，按 §"双方向规则"看来源=代码锚点，不受去向约束）。
- **归哪篇**：新建 15。
- **验收标准**：见 §5.1。

---

## 8. 锚点迁移与断链成本

### 8.1 符号迁移表（framework.rs → minix-chardriver；02 正文用）

| 旧名（02 正文） | 新权威 | 文件锚点 |
|----------------|--------|---------|
| `framework.rs`（整模块） | `os/libs/minix-chardriver/src/{driver,protocol}.rs` | lib.rs:22-24 明确 |
| `classify_request` | `classify` | driver.rs:357 |
| `CharacterRequest` | `CdevRequest` | protocol.rs:41 |
| `decide_reply` | `reply_decision` | driver.rs:90 |
| `ReplyDecision{Send,Park,RestartSilence,InvalidParking}` | `ReplyDecision{Reply,Parked,SwallowedRestart}`（违规/过期挂起→`Reply(EINVAL)`） | driver.rs:68-77,96-103 |
| `may_park` 方法 | 取消（并入 `reply_decision` 内联 match） | driver.rs:96-101 |
| `Incoming`（3 变体） | `Route`（5 变体） | driver.rs:47-59 |

### 8.2 引用迁移表（断链）

| 旧引用 | 新目标 | 验证方式 |
|--------|--------|---------|
| 02:214/257/301 "framework.rs" | 删/改指 minix-chardriver；保留一句历史注记 | `grep framework.rs 02-*.md` 命中仅剩历史注记 |
| 00/01 "E-INWIRE 未落地/停车" | "已落地 serve.rs，见 15" | `grep "停车\|空转\|未落地" 0[01]-*.md` 为空 |
| 01/02/03 "共 98 个测试" | "共 91 个测试（+ minix-chardriver 23）" | 对账 `grep -c '#\[test\]'` |
| 01:5/332 `input_other（L646）` | `input_init（646）` / `input_other（608）` | 读 `input.c:608,646` |
| 各处指向新循环叙述 | 增补"→ 15" | 15 就位后 |

### 8.3 断链成本摘要

- 篇号引用总量 **666**（§0.3），代码注释回指另计；这是**否决全局重编号**的主因。
- 本方案的改动**不触碰任何篇号**（新建用空闲尾号 15），故 666 引用全部无需迁移；实际改写集中在符号名/计数/叙事三类，属**篇内替换**，热点文件 `02`（12 处 framework.rs）+ `01`/`00`（停车叙事）。
- 建议批量方式：02 全文按 §8.1 一次性符号替换 + §3/§4/§5 局部重写；00/01 用精确串替换更正叙事；计数用 `rg` 定位后逐处手改（避免全局 98→91 误伤页码/其它数字）。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**：15 依赖 01–11（更早），无篇依赖 15 → 前向引用为零 ✓；00–14 编号未动，原有无前向引用性质保持 ✓。
2. **依赖图无环**：仅新增 01→15、02→15 单向指针（01/02 的"机械一半见 15"），15 不回指 01/02 的实现 → 无环 ✓。
3. **覆盖率 100%**：存量 16 篇全部原位保留（§2.1），零删除 → 存量知识点全部有去向 ✓；6 条新增（K-N1…K-N6）全带代码锚点 ✓。
4. **断链成本已统计**：666 引用，方案零重编号 → 迁移成本限于 4 类篇内替换（§8.3）✓。

### 9.2 自检门逐门

| 门 | 结果 |
|----|------|
| G1 C 真序逐条可核对 | 通过——§1 真序 20 函数行号已对 `input.c` 复核（`input_read:162`、`input_event:376`、`input_other:608` 等抽样命中） |
| G2 知识点池完整 | 通过——C 六文件 + 非 C 制品逐项有归属（§2.1/§3.5）；未讲项即 serve 层，已由 G-1 追加入池 |
| G3 前向引用为零 | 通过（§9.1.1） |
| G4 依赖图无环 | 通过（§9.1.2） |
| G5 覆盖率 100% | 通过（§9.1.3，零删除即零丢失） |
| G6 拆分/合并写清去向、新建写清来源 | 通过——本轮无拆分/合并；唯一新建（15）来源为 `serve.rs`/`main.rs` 代码锚点 |
| G7 每篇契约七要素齐全 | 通过——变化的 15/02 给全契约，00/01/03 给更正契约（§5） |
| G8 迁移表覆盖变化文档 | 通过——§8.1 符号 + §8.2 引用（含代码回指）+ 断链成本 |
| G9 事实断言有锚点 | 通过——§2.3/§3 每条失真都附实测锚点；无锚点者已标"推测/待验证"（见 §9.3） |

### 9.3 结论与待裁决

**结论：未完成态=否；可交付执行。** 本蓝图为**定向重建**：零重编号、新建 1 篇（15）、就地重写/更正 4 篇（00/01/02/03），余 11 篇不动。C 行为层与整体结构经独立取证判定**健康**，问题集中在**代码迁移后的文档同步债**（framework.rs 符号、transport-landed 叙事、测试计数、一处锚点）。

**待用户/B 相裁决的项（本轮已给建议，留一处需人确认）**：
1. 文档 02 的**责任收缩幅度**（§3.4 越界项）：02 保留"使用面 + C 底线"、把判定核实现让给 16-stage 的 minix-chardriver 文档——此为跨 stage 边界判断，建议人工确认 16-stage/01 是否已承接判定核实现文档；若未承接，02 需暂保留更完整的实现面（仍须按 §8.1 更正符号名）。**标注：推测（未读 16-stage，越范围）。**
2. `plan.md §7.2/§3.5` 仍称 Rust 为"stub/0 测试"——`plan.md` 属参考材料，不在 R 相正文范围，建议 B 相顺带把该处更新为 4874 行 / 91 测试的现状。**标注：范围外发现。**
3. minix-chardriver `protocol.rs` 的 `CDEV_REPLY_BASE=0x500` 错值（99 §2 已登记 edge E-CDRCONV）与本 stage `serve.rs` 的 `CDEV_REPLY=0x480` 并存——属 edge 线程，不在本蓝图处置。
