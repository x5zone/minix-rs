# 12-stage-input 文档重组计划（plan.md）

> **状态**: 定稿（2026-08-16 首版 + 深度 review + minix3 源码回归 review，见 §7）
> **范围**: `notes/rewrite/fork-syscall-rewrite/12-stage-input/`
> **目标**: 以 **INPUT server 启动顺序为主线**定义 INPUT 全部文档；一次输入事件旅程与驱动生命周期为次主线；最终覆盖 Minix3 input server（`servers/input/`，1 个 .c + 1 个 .h，759 行）+ 运行框架（`lib/libchardriver/`，600 行，input 使用面）+ 协议面（`com.h`/`ipc.h`/`minix/input.h`）+ 客户端库（`lib/libinputdriver/`，206 行）+ 外部消费者（TTY/pckbd/DS）全部语义，支撑 input server 的彻底 Rust 重写
> **对照**: `01-stage-kernel/`（讲述结构参照）、`02-stage-vm/`/`07-stage-ds/`/`11-stage-devman/`（同流程先例）、`minix3/minix/servers/input/`（ground truth）、`os/servers/input/`（Rust 实现，当前为 stub）

---

## 1. 背景与动机

### 1.1 现状问题

`12-stage-input/` 目录自 2026-08-14 补建以来仅有**占位 README**（2026-08-16 移入 `draft/`），没有任何正式文档。早期逐行笔记散落在 `tmp/input/`（`tmp_input.c.md`/`tmp_input.h.md`，fork 主线时代产物，素材）。现状与 input 的语义地位不匹配：

1. **语义面横跨四层**——input 不是一个"纯 IPC 服务器"：它运行在 chardriver 框架之上（`chardriver_task` 主循环 + CDEV 请求分发），服务端语义（`servers/input/`）只占全部语义的一部分。完整语义还包括：**运行框架**（`lib/libchardriver/chardriver.c`：CDEV 消息协议 m10、`chardriver_process` 分发、open_devs 门卫、reply 语义）、**协议面**（`com.h:877-893` 的 TTY_INPUT_UP/TTY_INPUT_EVENT/INPUT_CONF/INPUT_SETLEDS/INPUT_EVENT + `ipc.h` 四个消息结构 + `minix/input.h` 的 `struct input_event` 事件格式与全部事件码）、**客户端库**（`lib/libinputdriver/`：驱动侧 announce/send_event/CONF 配置），**外部消费者**（TTY 的 `keyboard.c:do_input` 消费键盘事件与 `set_leds` 请求 LED、pckbd 驱动经 libinputdriver 上报事件）。Rust 重写必须同时复刻这些契约，缺一不可。
2. **框架依赖必须显式化**——`input.c:702` 直接调用 `chardriver_task(&input_tab)`，全部 open/read/ioctl/cancel/select 语义都挂接在 chardriver 的 CDEV 协议与 open_devs 门卫机制上。Rust 侧当前 `os/` 中没有 chardriver 等价物（grep 实证：`os/` 无 chardriver/CDEV 代码），这是必须提前决策的架构演进项（A-1），不能当作"外部库黑盒"跳过。
3. **C 侧存在真实的语义陷阱**——事件 `id` 是**数组下标**而非 minor（`input_event` 直接 `devs[id]`，越界即丢弃）；`input_other` 的 `INPUT_SETLEDS` 对非 TTY 来源 **fall-through** 到 unexpected 日志（`:636` 意图行为）；`input_select` 对 `CDEV_OP_WR` 总是就绪（`/* immediate error */`）；`input_alloc_id` 不得占用"已断开但打开"的设备槽；`input_connect` 在分配失败时仍发送 `INVALID_INPUT_ID` 的 CONF（驱动被静默禁用）；环形缓冲溢出**覆盖最旧**事件。这些都必须逐一定位并在计划中给出决策，Rust 重写才不会被 C 代码"表面正确性"误导。
4. **与 07-stage-ds / 10-stage-mib / 11-stage-devman 相同**——无旧主线文档可迁移（只有占位 README + tmp 素材），本计划从零定义文档集；§5 覆盖契约是后续写作的**唯一权威基线**，必须一次到位。

### 1.2 新主线：INPUT server 启动顺序 + chardriver 主循环

与 `01-stage-kernel` / `02-stage-vm` / `07-stage-ds` / `11-stage-devman` 相同，采用**读者学习顺序 = 系统实际执行顺序**的组织原则。input 的执行链是严格线性的：

```
RS 运行时加载 input（不在 boot_image，kernel/table.c:44-64 无 input 条目 → RS 加载组）
  │  system.conf:400-403  service input { ipc SYSTEM pm vfs rs ds tty vm; priority 1; }
  ▼  input.c:696-704  main()
  ├─ input_startup()：sef_setcb_init_fresh(input_init) + sef_startup()    ← 01：SEF 生命周期
  └─ chardriver_task(&input_tab)                                          ← 02：chardriver 框架主循环
       ├─ chardriver.c:549-573：sef_receive_status(ANY) 循环              ← 02
       └─ chardriver_process() 分发                                       ← 02
            ├─ SEF init fresh → input_init                                 ← 01：初始化
            │    ├─ devs[10] 初始化（input_revmap 回填 minor）            ← 03：设备结构
            │    ├─ ds_subscribe("drv\\.inp\\..*", DSF_INITIAL)           ← 11：驱动注册契约
            │    ├─ chardriver_announce()（drv.chr.<label> + 清 open_devs）← 02：框架 announce
            │    └─ TTY_INPUT_UP → TTY（ipc_send）                        ← 13：TTY 握手
            ├─ CDEV_OPEN / CDEV_CLOSE → input_open / input_close          ← 06：打开关闭
            ├─ CDEV_READ → input_read → input_copy_events                 ← 07：读取/挂起
            ├─ CDEV_IOCTL → input_ioctl（KIOCSLEDS）                      ← 08：ioctl
            ├─ CDEV_CANCEL → input_cancel                                 ← 08：取消
            ├─ CDEV_SELECT → input_select                                 ← 08：select
            └─ notify / 其他 → input_other                                 ← 09/10/11：消息面
                 ├─ DS notify → input_check（connect/disconnect）         ← 11：驱动生命周期
                 ├─ INPUT_EVENT → input_event → input_process             ← 09：事件处理
                 └─ INPUT_SETLEDS（仅 TTY 来源）→ input_set_leds          ← 10：LED 状态
```

### 1.3 次主线：一次输入事件的旅程 + 驱动生命周期 + LED 状态流

**事件旅程**（driver → server → reader/TTY）：

```
pckbd 键盘/鼠标中断（drivers/hid/pckbd/pckbd.c）
  │  scan_keyboard → kbd_process/kbdaux_process → inputdriver_send_event   ← 14/12
  ▼  INPUT_EVENT（m_linputdriver_input_event：id/page/code/value/flags）
  input_event（input.c:376）
  │    ├─ id 校验：0..INPUT_DEV_MAX-1（越界丢弃，id == 数组下标）          ← 09
  │    ├─ owner 校验：m_source == devs[id].owner（不匹配丢弃）             ← 09
  │    └─ mux 选择：kbd → KBDMUX_DEV / 其他 → MOUSEMUX_DEV                 ← 09
  ▼  input_process（input.c:332）
  ├─ 环形缓冲：溢出覆盖最旧 → enqueue（page/code/value/flags/devid/rsvd=0）← 09
  ├─ 已打开设备 → 唤醒挂起 reader（chardriver_reply_task）/ selector       ← 07/08
  └─ 设备与 mux 均未打开 → 转发 TTY_INPUT_EVENT → TTY do_input             ← 13
       （keyboard.c:148-176：INPUT_PAGE_KEY 过滤 + NR_SCAN_CODES 边界
         + INPUT_RELEASE → RELEASE_BIT + inbuf 环形缓冲）
```

**驱动生命周期**（announce → connect → CONF → events → disconnect）：

```
pckbd pckbd_init → inputdriver_announce(INPUT_DEV_KBD|INPUT_DEV_MOUSE)    ← 14
  → ds_publish_u32("drv.inp.<label>", typemask, DSF_OVERWRITE)             ← 12
  → DS 通知 input → input_check → ds_check 循环（drv.inp. 前缀过滤）        ← 11
  → input_connect（label 校验 → input_alloc_id（kbd/mouse 槽）              ← 11
     → INPUT_CONF asynsend（kbd_id/mouse_id，失败也发 INVALID 值）
     → 恢复 devs[kbd_id].leds 初始 LED）
  → 事件流（见上）→ 驱动退出 → DS label 消失
  → input_check ESRCH → input_disconnect（EIO 唤醒挂起 reader
     + select 唤醒 CDEV_OP_RD + owner=NONE）                                ← 11
```

**LED 状态流**（TTY/ioctl → input_set_leds → 保存 + 广播 → driver idr_leds）：

```
TTY set_leds（keyboard.c:369-384）→ INPUT_SETLEDS（led_mask）→ input_other → input_set_leds  ← 10/13
ioctl KIOCSLEDS（ttycom.h:174 + kbdio.h）→ input_ioctl（kio_leds_t → INPUT_LED_* 位映射）      ← 08
  → dev->leds = mask 保存（跨驱动重启）→ asynsend3 INPUT_SETLEDS 到 owner 键盘驱动            ← 10
  → pckbd idr_leds → 写键盘端口（KBD_OUT 队列 + watchdog）                                    ← 14
  → 驱动重连时 input_connect 用 devs[kbd_id].leds 恢复初始 LED 状态                            ← 11
```

---

## 2. 新文档编号与阶段划分

> 编号规则与 `01-stage-kernel` 一致：`NN-短横线语义名.md`；`00-` 为总览；`99-` 为全局概念。旧占位 README 保留在 `draft/`（素材），新编号在顶层重新建立。

### 阶段总览（16 篇）

| 阶段 | 编号 | 文档 | 语义模块 | C 源码 | Rust 模块（规划） | draft 来源 | 变更 |
|------|------|------|---------|--------|-------------------|-----------|------|
| 0 总览 | 00 | `00-input-overview.md` | input 是什么、启动主线图、事件旅程/驱动生命周期次主线、文档导航 | `servers/input/` 全部 | 全部 | `draft/README.md` | **新建**导航 |
| 1 启动入口与运行框架 | 01 | `01-input-init-main.md` | `main`/`input_startup`/SEF init fresh/`input_init`（devs 初始化 + ds_subscribe + chardriver_announce + TTY_INPUT_UP）/`chardriver_task` 调用点 | `servers/input/input.c:646-704` + `input.h` 全部 | `main.rs`、`init.rs` | `tmp/input/tmp_input.c.md`（素材） | **新建**：启动主线锚点 |
| 1 | 02 | `02-chardriver-framework.md` | chardriver 框架契约：`chardriver_task` 主循环、`chardriver_process` 分发（notify/BDEV_OPEN/CDEV RQ/other）、CDEV 消息协议 m10 字段、reply 语义（CDEV_REPLY/SEL1/SEL2/EDONTREPLY）、open_devs 门卫、`chardriver_announce`/`reply_task`/`reply_select` | `lib/libchardriver/chardriver.c` 全部（600 行，按 input 使用面）+ `minix/chardriver.h` | 等价框架（A-1，决策待定） | 无 | **新建**：框架面 |
| 2 核心数据结构与协议面 | 03 | `03-input-device-structs.md` | `struct input_dev` 全字段（minor/owner/label/eventbuf/tail/count/opened/suspended/caller/grant/req_id/selector/leds）、`devs[10]` 布局、常量（EVENTBUF_SIZE/minor 编号/DEV 下标/INPUT_DEV_MAX）、`input_map`/`input_revmap` | `servers/input/input.h` 全部 + `input.c:44-83` | `structs.rs` | `tmp/input/tmp_input.h.md`（素材） | **新建** |
| 2 | 04 | `04-input-event-format.md` | `struct input_event` wire 格式（page/code/value/flags/devid/rsvd[2]）、事件页（INPUT_PAGE_*）、事件值（PRESS/RELEASE）、事件标志（ABS/REL）、按键/LED/按钮/消费类事件码全表（HID Usage 对齐） | `minix/include/minix/input.h` 全部（`_SYSTEM` 段除外） | `minix-types`（`event.rs`） | `tmp/input/tmp_input.h.md`（素材） | **新建**：事件格式契约 |
| 2 | 05 | `05-input-message-contract.md` | 消息常量（TTY_INPUT_UP/TTY_INPUT_EVENT/INPUT_CONF/INPUT_SETLEDS/INPUT_EVENT + BASE 段）、四个消息结构字段（`m_linputdriver_input_event`/`m_input_linputdriver_input_conf`/`m_input_linputdriver_setleds`/`m_input_tty_event`）、全单向协议（无回复）、`INPUT_MAJOR=64`、`/dev` 设备节点表（MAKEDEV）、`system.conf` 权限 | `com.h:877-893`、`ipc.h:233-259,990-1000,2434-2436,2517`、`dmap.h:78`、`MAKEDEV.sh:330-343`、`system.conf:400-403` | `minix-types`（`ipc/input.rs`） | 无 | **新建**：协议面 |
| 3 字符设备操作面 | 06 | `06-input-open-close.md` | `input_open`（map/active/EBUSY/opened=TRUE）、`input_close`（ENXIO/EINVAL/清 tail+count）、`input_dev_active` 语义（mux 恒 active） | `input.c:85-128` | `handlers.rs`（open/close） | `tmp/input/tmp_input.c.md`（素材） | **新建** |
| 3 | 07 | `07-input-read-suspend.md` | `input_read`（ENXIO/EIO/event_count=size/sizeof/非阻塞 EAGAIN/挂起 EDONTREPLY 状态机）、`input_copy_events`（环形缓冲回绕 + `sys_safecopyto` 分段拷贝 + 返回字节数）、挂起字段（caller/grant/req_id）、`chardriver_reply_task` 唤醒 | `input.c:130-203` | `handlers.rs`（read）、`eventbuf.rs` | `tmp/input/tmp_input.c.md`（素材） | **新建** |
| 3 | 08 | `08-input-ioctl-cancel-select.md` | `input_ioctl`（KIOCSLEDS：`sys_safecopyfrom` + `kio_leds_t` 位映射 + ENOTTY）、`input_cancel`（EINTR/EDONTREPLY 匹配语义）、`input_select`（CDEV_OP_RD 就绪/挂起/CDEV_NOTIFY selector、CDEV_OP_WR 恒就绪） | `input.c:241-331` + `sys/kbdio.h` + `ttycom.h:174` | `handlers.rs`（ioctl/cancel/select） | `tmp/input/tmp_input.c.md`（素材） | **新建** |
| 4 事件与 LED 机制 | 09 | `09-input-event-processing.md` | `input_event`（id 边界 + owner 校验 + mux 选择 + TTY 转发）、`input_process`（溢出覆盖最旧 + enqueue + 唤醒挂起 reader/selector） | `input.c:332-429` | `event.rs` | `tmp/input/tmp_input.c.md`（素材） | **新建**：事件旅程核心 |
| 4 | 10 | `10-input-setleds.md` | `input_set_leds`（minor 匹配广播（KBDMUX 全广播）/`dev->leds` 保存/`asynsend3` 下发）、LED 状态跨驱动重启语义 | `input.c:204-240` | `setleds.rs` | `tmp/input/tmp_input.c.md`（素材） | **新建** |
| 5 驱动生命周期 | 11 | `11-input-driver-connect.md` | `input_check`（ds_check 循环 + `drv.inp.` 前缀 + 移除检测 ESRCH）、`input_connect`（label 校验 + `input_alloc_id` + CONF 回复 + LED 恢复）、`input_alloc_id`（槽位复用/不占已断开打开槽/INVALID_INPUT_ID）、`input_disconnect`（EIO 唤醒 + select 唤醒 + owner=NONE）、DS 订阅契约（`ds_subscribe`/`ds_check`/`ds_retrieve_u32`/`ds_retrieve_label_*`） | `input.c:430-645` + `libinputdriver` 发布面 + `ds.h` 使用面 | `connect.rs` | `tmp/input/tmp_input.c.md`（素材） | **新建**：驱动生命周期 |
| 6 客户端库 | 12 | `12-libinputdriver.md` | `inputdriver_announce`（`drv.inp.<label>` 发布）、`inputdriver_send_event`（阻塞 ipc_send + 崩溃检测重置 endpoint）、`do_conf`（ds label 校验 + 保存 id）、`do_setleds`（source 校验 + `idr_leds` 回调）、`inputdriver_process`（notify 分发 + 消息分发）、`inputdriver_task`/`terminate`、`struct inputdriver` 回调表 | `lib/libinputdriver/inputdriver.c` 全部（206 行）+ `minix/inputdriver.h` | `minix-sys/inputdriver.rs` | `tmp/input/tmp_input.c.md`（素材） | **新建**：客户端契约 |
| 7 外部消费者 | 13 | `13-tty-consumer.md` | TTY 侧契约：`TTY_INPUT_UP` 握手（`input_endpt` 保存 + `set_leds` 回发）、`TTY_INPUT_EVENT` 消费（INPUT_PAGE_KEY 过滤 + NR_SCAN_CODES 边界 + RELEASE_BIT + inbuf 环形缓冲）、`set_leds` 的 INPUT_SETLEDS 请求、`tty.c:209-210` 消息接入 | `drivers/tty/tty/arch/i386/keyboard.c:124-176,369-384` + `drivers/tty/tty/tty.c:209-210` | 外部契约（TTY 重写属驱动阶段，不实现） | 无 | **新建**：外部契约 |
| 7 | 14 | `14-pckbd-driver.md` | pckbd 作为 libinputdriver 模型消费者：`pckbd_init`（flags=INPUT_DEV_KBD[|MOUSE] + `inputdriver_announce`）、键盘事件上报（`kbd_process` → `inputdriver_send_event`）、鼠标事件上报（`kbdaux_process`：按钮/相对位移）、`pckbd_leds`（INPUT_LED_* → 端口位）、`pckbd_intr`/`pckbd_alarm`、主循环 `inputdriver_task` | `drivers/hid/pckbd/pckbd.c`（504 行，libinputdriver 使用面） | 外部契约（驱动阶段，不实现） | 无 | **新建**：外部契约 |
| 99 全局概念 | 99 | `99-input-global-concepts.md` | 全部常量总表（消息/事件页/事件码/LED/按钮/消费类/minor/DEV 下标）、错误码汇总（ENXIO/EBUSY/EIO/EAGAIN/EINTR/ENOTTY/EINVAL/EDONTREPLY）、endpoint/DS label 约定、全局状态（devs 数组）、跨服务引用（VFS/DS/RS/TTY/pckbd） | `com.h`/`input.h`/`inputdriver.h`/`input.h`（server 内部） | `minix-types` | `draft/README.md` + tmp 素材 | **新建** |

### 2.1 阶段间的叙事衔接

每个阶段结尾的文档需含"过渡"小节（参照 `01-stage-kernel` 每篇 §6"过渡"），说明本阶段在启动时序/主循环中的位置与下一阶段的入口：

```
00（总览）→ 01（启动入口）→ 02（chardriver 框架）→ 03/04/05（数据结构与协议面）
→ 06/07/08（CDEV 操作面）→ 09/10（事件与 LED 机制）→ 11（驱动生命周期）
→ 12（客户端库）→ 13/14（外部消费者）→ 99（全局概念收口）
```

叙事衔接约定：01 结尾指 02（主循环由框架提供）；02 结尾指 03（设备结构挂接在 CDEV minor 上）与 06~08（handler 注册面）；05 结尾指 09/10/11（消息的实际消费者）；11 结尾指 12（驱动侧对称实现）；13/14 结尾验证 09/10 的外部可观察行为。

---

## 3. 讲述结构规范（参照 01-stage-kernel）

### 3.1 每篇文档的章节模板

与 `01-stage-kernel` 一致（见 `03-kmain-cstart.md`、`04-platform-discovery.md` 等）：

1. **概念**——为什么需要这个机制、类比、边界声明（前置依赖/本篇不覆盖什么）
2. **C 源码分析**——ground truth，必须 grep 实证，禁止凭记忆
3. **Rust 设计决策**——类型系统建模、与 C 的差异（含 ARCH 标注）
4. **实现详解**——Rust 模块结构、关键不变量
5. **测试要点**——相关测试函数 + 测试总数声明
6. **过渡**——在启动时序/主循环中的位置，下一篇入口
7. **参见**——跨文档引用（新编号）

### 3.2 组织原则（继承 01-stage-kernel §3）

1. **概念首次出现即完整解释**——后续只引用不复述
2. **禁止前向引用**——依赖机制前置（本计划的编号已保证：chardriver 前置 02、事件格式前置 04、消息面前置 05、客户端库后置 12）
3. **每篇一个语义单元**——读者可独立阅读
4. **位置可回答性**——每篇回答"它在 main → chardriver_task → 主循环的哪个位置"

### 3.3 引用规则

- 各文档之间用新编号交叉引用（如 `05-input-message-contract.md` §INPUT_EVENT）
- 与 kernel/其他 stage 文档交叉引用时用 `../NN-stage-*/` 相对路径（如 `../01-stage-kernel/NN-*.md`、`../03-stage-rs/NN-*.md`、`../07-stage-ds/NN-*.md`）
- 对 draft 素材的引用一律指向 `draft/README.md` 或 `../../../../tmp/input/`（标注"素材"）

### 3.4 每篇文档边界声明（执行级）

> 写作每篇文档前，先按本表声明**前置依赖 / 本篇职责 / 不覆盖（移交）**，防止内容交叉与遗漏。边界以 §5.3 函数清单为准绳。

| 编号 | 前置依赖 | 本篇职责（核心语义） | 不覆盖（移交） |
|------|---------|---------------------|--------------|
| 00 | 无 | 总览、启动主线图、事件旅程/驱动生命周期次主线图、文档导航、设计原则 | 一切机制细节 |
| 01 | 00 + kernel 文档（RS 加载组） | `main`/`input_startup`/`sef_setcb_init_fresh`/`sef_startup`、`input_init`（devs 初始化、`ds_subscribe`、`chardriver_announce`、TTY_INPUT_UP）、`chardriver_task` 调用点、`input_tab` 注册表 | chardriver 内部机制（02）、各 handler 实现（06~10） |
| 02 | 01 | chardriver API 面（`chardriver_task`/`chardriver_process`/`chardriver_announce`/`chardriver_reply_task`/`chardriver_reply_select`/`chardriver_get_minor`）、CDEV 消息协议（m10 字段 + 常量 + reply 类型）、open_devs 门卫、notify/BDEV_OPEN 处理、**A-1 框架决策** | input 业务结构（03）、handler 业务（06~10） |
| 03 | 02（minor 挂接） | `struct input_dev` 全字段、`devs` 数组布局、常量（EVENTBUF_SIZE/KBD*/MOUSE*/DEV 下标/INPUT_DEV_MAX）、`input_map`/`input_revmap`（minor↔下标双向映射）、`input_dev_active` 宏 | 消息面（05）、各 handler 行为（06~10） |
| 04 | 03（eventbuf 元素类型） | `struct input_event` 字段语义、事件页/值/标志常量、全量事件码（KEY/LED/BUTTON/CONS/GD）、HID Usage 对齐说明 | 消息封装（05）、缓冲/唤醒机制（07/09） |
| 05 | 01 + 03（minor/设备语义）+ 04（事件字段） | 消息常量与 BASE 段、四个消息结构字段布局、全单向协议约定、`INPUT_MAJOR`/`/dev` 节点表、`system.conf` 权限、`ipc.h` 消息 union 接入 | handler 业务（06~10）、驱动生命周期（11） |
| 06 | 03（map/active）+ 02（CDEV 分发） | `input_open`/`input_close` 完整语义（ENXIO/EBUSY/EINVAL、opened 状态、close 清缓冲） | 读/挂起（07）、ioctl/cancel/select（08） |
| 07 | 03 + 02（reply 语义）+ 05 | `input_read` 全流程（EIO/EAGAIN/挂起 EDONTREPLY）、`input_copy_events`（回绕分段 safecopy）、挂起字段状态机、`chardriver_reply_task` 唤醒路径 | 唤醒的生产方（09）、ioctl/cancel/select（08） |
| 08 | 03 + 02（reply 语义）+ 05 | `input_ioctl`（KIOCSLEDS + 位映射 + ENOTTY）、`input_cancel`（匹配 → EINTR / 不匹配 → EDONTREPLY）、`input_select`（就绪/挂起/CDEV_NOTIFY selector/CDEV_OP_WR 恒就绪） | 读挂起细节（07）、事件生产（09） |
| 09 | 03 + 04 + 05 | `input_event`（id 边界/owner 校验/mux 选择/TTY 转发）、`input_process`（溢出覆盖、enqueue、唤醒 reader/selector） | LED 广播（10）、驱动连接（11）、TTY 消费（13） |
| 10 | 03（leds 字段）+ 05 | `input_set_leds`（KBDMUX 全广播/单键盘匹配、`dev->leds` 保存、`asynsend3` 下发）、跨驱动重启恢复 | LED 请求来源（08/13）、驱动侧回调（14） |
| 11 | 03 + 05 + 07/08（唤醒面）+ 12（发布面） | `input_check`（新增/移除检测）、`input_connect`（label 校验/alloc/CONF/LED 恢复）、`input_alloc_id`、`input_disconnect`、DS 订阅契约（`ds_subscribe`/`ds_check`/`ds_retrieve_*`） | DS server 内部（07-stage-ds）、驱动侧实现（12/14） |
| 12 | 05（INPUT_CONF/SETLEDS/EVENT）+ 99 | `inputdriver_announce`/`send_event`/`process`/`task`/`terminate`、`do_conf`/`do_setleds`、`struct inputdriver` 回调表、`drv.inp.` 前缀对称契约 | server 内部（01~11）、pckbd 硬件细节（14） |
| 13 | 05（TTY_INPUT_UP/EVENT）+ 09（转发格式） | TTY 侧 `do_input`（UP 握手/EVENT 消费）、`set_leds`（INPUT_SETLEDS 请求）、`input_endpt` 状态、消息接入点（tty.c:209-210） | TTY 内部完整语义（驱动阶段）、server 内部（01~12） |
| 14 | 12（libinputdriver 使用面）+ 04（事件码） | pckbd 模型驱动：announce flags、键盘/鼠标事件上报、`pckbd_leds` 端口写入、intr/alarm 回调、主循环 | 驱动硬件细节（驱动阶段）、server 内部（01~12） |
| 99 | 全部 | 常量总表、错误码汇总、endpoint/DS label 约定、全局状态、跨服务引用、`_SYSTEM` 段可见性说明 | 各机制细节（01~14） |

### 3.5 测试基线（2026-08-16）

- `cargo check -p minix-input`：通过（stub：`lib.rs` 仅 `pub fn init() {}`）
- `cargo test -p minix-input`：**0 passed / 0 failed**（无测试，2026-08-16 实测）
- 每篇改写完成时在文末更新该模块测试统计（review-doc-skill §2.4j）

### 3.6 Review gate 要求（每篇改写必检）

- 每篇新文档创建时同步生成 `.design/{NN}-outline.md` + `{NN}-outline-review.md` + `{NN}-design.md`（Step 0.3 嵌入生成，Gate H.6，不允许 N/A）
- 每篇改写后按 review 工作流跑 Blocker Gates（0/A/B/C/D/D-6/E/G/H），scan 产物写入 `.review/codex/input/{NN}-{name}/`
- P0 未清不得标完成；doc 与 code 保持同步

---

## 4. 架构演进（ARCH）清单

> 按 review-core 要求，ARCH 必须三处一致标注：Minix3 行为对照点 + design doc + 代码注释。以下为 12-stage-input 范围内的 ARCH 项，写文档时必须逐项落实。

| # | ARCH 项 | Minix3 现状 | minix-rs 演进 | 涉及文档 | 状态 |
|---|---------|------------|--------------|---------|------|
| A-1 | **chardriver 框架依赖** | `lib/libchardriver/chardriver.c`（600 行）共享框架（input + tty + 全部字符驱动共用），`chardriver_task` 主循环 + CDEV 协议 + open_devs 门卫 | `os/` 无 chardriver 等价物（grep 实证）→ input 重写需决策：新建共享框架 crate（如 `os/libs/minix-chardriver/`）或 input 内部最小等价实现；CDEV 消息常量入 `minix-types` | 02（决策）+ 01/06~08（使用点） | **决策待定**（§7.3） |
| A-2 | **DS 订阅/驱动注册契约** | `ds_subscribe("drv\\.inp\\..*", DSF_INITIAL)` + `input_check` 的 `ds_check` 轮询 + `ds_retrieve_*`；libinputdriver 侧 `ds_publish_u32` | DS server 重写（07-stage-ds）已规划；input 文档只定义**使用面契约**（前缀命名、typemask 值、DSF_INITIAL 语义、label 删除 → ESRCH）；Rust 侧经 `minix-sys` DS 客户端调用 | 11 + 12 | 契约沿用 |
| A-3 | **事件 ID == 数组下标** | `INPUT_EVENT.id` 是 `devs[]` 数组下标（非 minor）；`input_event` 校验 `0<=id<INPUT_DEV_MAX`，越界静默丢弃 | Rust：`DevId` Newtype + 边界校验不变（越界丢弃）；与 minor 的映射仅经 `input_map`/`input_revmap` 两个纯函数 | 03 + 09 | 沿用（类型化强化） |
| A-4 | **环形缓冲溢出语义** | `eventbuf[32]` 定长环形缓冲，溢出**覆盖最旧**事件（`tail++`、`count--`），tail/count 维护 | Rust：`VecDeque` 或定长环形缓冲，保持"覆盖最旧"语义；`input_copy_events` 回绕分段拷贝以迭代器/slice 表达 | 03 + 07 + 09 | 沿用（结构等价） |
| A-5 | **挂起读 + EDONTREPLY** | 每设备最多一个挂起读（`suspended` 标志 + caller/grant/req_id）；`EDONTREPLY` 伪回复由 `chardriver_reply_task` 稍后异步回复 | Rust：显式 `SuspendedRead` 状态结构 + 状态机（suspend → resume/cancel/disconnect 三出口）；EDONTREPLY 常量保留为伪回复标记 | 07 + 02 | 沿用（显式状态机） |
| A-6 | **LED 状态跨驱动重启** | `dev->leds` 保存当前掩码；`input_connect` 注册后立即 `input_set_leds(devs[kbd_id].minor, devs[kbd_id].leds)` 恢复 | Rust：`leds` 字段语义保留；广播/单键盘匹配逻辑不变 | 10 + 11 | 沿用 |
| A-7 | **minor 稀疏编号 vs 下标** | minor 稀疏（0,1-4,64,65-68）与数组下标（0-9）分离，向后兼容预留（注释明言）；`input_revmap` 非法 id `panic` | Rust：`Minor`/`DevId` Newtype + `map`/`revmap` 纯函数（revmap 失败返回 `Option`/`Result` 而非 panic） | 03 | 类型化强化 |
| A-8 | **select/CDEV_NOTIFY 契约** | `input_select`：RD 就绪/挂起即时报、空缓冲 + `CDEV_NOTIFY` → 记 selector；WR 恒就绪（`/* immediate error */`）；`chardriver_reply_select` 异步唤醒 | Rust：`selector: Option<endpoint>` + 唤醒经框架 select 回复通道；WR 恒就绪语义保留（输入设备只读，实际写返回 EIO） | 08 + 02 | 沿用 |
| A-9 | **全单向消息协议** | input 协议所有消息无回复（com.h 明言 "The input protocol has no real replies"）；server 侧 `asynsend3(AMF_NOREPLY)`、驱动侧 `ipc_send` 阻塞发送 | Rust：fire-and-forget 发送原语；`inputdriver_send_event` 的阻塞发送 + 失败重置 endpoint 语义保留（防消息堆积 + 崩溃检测） | 05 + 09/10/12 | 沿用 |
| A-10 | **input_other 消息面** | `INPUT_SETLEDS` 非 TTY 来源 fall-through 到 unexpected 日志（`:636`）；`INPUT_EVENT` 无条件处理；DS notify 单独分支 | Rust：按消息+来源显式分派；非 TTY 的 SETLEDS 记日志并忽略（fail-closed）；未知消息日志不变 | 09/10/11 + 05 | 沿用（显式化） |
| A-11 | **错误码映射** | 全部返回 errno：ENXIO/EBUSY/EIO/EAGAIN/EINTR/ENOTTY/EINVAL + 伪回复 EDONTREPLY + 内部 panic（input_copy_events 数量不足） | Rust：`Result<_, InputError>` 映射 minix errno；`input_copy_events` 数量不足改为 debug_assert + 返回错误（不 panic）；错误码入 `minix-types` | 06~08 + 99 | 沿用（错误显式化） |

---

## 5. 覆盖完整性核对（对照 minix3 源码）

> 本节是 plan.md 的语义覆盖契约。所有断言以 grep 实证为准（review-core：禁止凭记忆）。逐项核对见 §7。

### 5.1 C 源文件 → 新文档映射

| C 源文件 | 行数 | 覆盖文档 | 核对 |
|---------|------|---------|------|
| `servers/input/input.c` | 704 | 01（main/startup/init）、03（map/revmap + 宏）、06（open/close）、07（read/copy_events）、08（ioctl/cancel/select）、09（process/event）、10（set_leds）、11（alloc/connect/disconnect/check/other）、05（input_tab 注册面） | 已核对（20 函数逐一落位，§5.3） |
| `servers/input/input.h` | 45 | 03（struct input_dev + 常量 + DEV 下标）、99（常量总表） | 已核对 |
| `lib/libchardriver/chardriver.c` | 600 | 02（input 使用面：task/process/announce/reply_task/reply_select/get_minor/do_* + open_devs 门卫） | 已核对（使用面裁剪，完整框架语义属驱动阶段） |
| `lib/libinputdriver/inputdriver.c` | 206 | 12（全部 7 个函数 + 静态状态） | 已核对 |
| `drivers/hid/pckbd/pckbd.c` | 504 | 14（libinputdriver 使用面：announce/send_event/leds/intr/alarm/task） | 已核对（外部契约，硬件细节裁剪） |
| `drivers/tty/tty/arch/i386/keyboard.c` | — | 13（do_input 的 UP/EVENT 分支 + set_leds 请求面） | 已核对（外部契约，TTY 内部语义裁剪） |

### 5.2 头文件/协议面覆盖

| 头文件 | 覆盖文档 | 核对 |
|--------|---------|------|
| `minix/include/minix/com.h:877-893`（TTY_INPUT_UP/TTY_INPUT_EVENT + INPUT_RQ_BASE 0x1500/INPUT_RS_BASE 0x1580 + INPUT_CONF/INPUT_SETLEDS/INPUT_EVENT） | 05/99 | 已核对 |
| `minix/include/minix/ipc.h:232-259`（conf/setleds/tty_event 三结构）、`:990-1000`（input_event 结构）、`:2434-2436,2517`（message union 接入） | 05/99 | 已核对 |
| `minix/include/minix/input.h`（struct input_event + 事件页/值/标志 + 全量事件码 + `_SYSTEM` 段） | 04/99 | 已核对 |
| `minix/include/minix/inputdriver.h`（struct inputdriver 回调表 + 5 函数原型） | 12/99 | 已核对 |
| `minix/include/minix/chardriver.h`（struct chardriver + 7 函数原型 + cdev_id_t） | 02/99 | 已核对 |
| `minix/include/minix/dmap.h:78`（INPUT_MAJOR=64） | 05/99 | 已核对 |
| `sys/sys/ttycom.h:174`（KIOCSLEDS `_IOW('k',2,kio_leds)`）+ `sys/kbdio.h`（kio_leds_t + KBD_LEDS_*） | 08/99 | 已核对 |
| `minix/include/minix/ds.h`（ds_subscribe/ds_check/ds_retrieve_u32/ds_retrieve_label_name/ds_retrieve_label_endpt/ds_publish_u32/DSF_INITIAL/DS_DRIVER_UP） | 11/12 | 已核对（使用面） |
| `etc/system.conf:400-403`（service input 权限） | 05/99 | 已核对 |
| `commands/MAKEDEV/MAKEDEV.sh:330-343`（/dev 节点表） | 05/99 | 已核对 |

### 5.3 函数/符号清单映射

> 以下为 `servers/input/` 全部顶层定义 + libinputdriver 全部导出 + chardriver input 使用面 + TTY/pckbd 契约点，逐一落入新文档。行号以 2026-08-16 工作区为准。

**input.c（20 函数 + 1 静态数组 + 1 静态表 + 3 宏）**

| 符号 | 行 | 覆盖文档 |
|------|-----|---------|
| `devs[INPUT_DEV_MAX]`（静态） | 22 | 03 |
| `input_tab`（chardriver 回调表） | 31-42 | 01（注册面）+ 05（协议面） |
| `input_dev_active` / `input_dev_buf_empty` / `input_dev_buf_full`（宏） | 24-27 | 03（active 语义）+ 06/07（使用点） |
| `input_map` | 44-64 | 03 |
| `input_revmap` | 67-82 | 03（panic 语义 → A-7） |
| `input_open` | 85-105 | 06 |
| `input_close` | 107-127 | 06 |
| `input_copy_events` | 130-160 | 07（回绕分段拷贝 + A-4/A-11） |
| `input_read` | 162-201 | 07（挂起状态机 + A-5） |
| `input_set_leds` | 204-239 | 10（A-6） |
| `input_ioctl` | 241-280 | 08（KIOCSLEDS 位映射） |
| `input_cancel` | 282-301 | 08 |
| `input_select` | 303-330 | 08（A-8） |
| `input_process` | 332-374 | 09（A-4 溢出语义 + 唤醒） |
| `input_event` | 376-428 | 09（id/owner 校验 + mux + TTY 转发） |
| `input_alloc_id` | 430-473 | 11（槽位复用 + 不占已断开打开槽） |
| `input_connect` | 475-531 | 11（label 校验 + CONF + LED 恢复） |
| `input_disconnect` | 533-556 | 11（EIO 唤醒 + select 唤醒 + owner=NONE） |
| `input_check` | 558-606 | 11（ds_check 轮询 + 移除检测） |
| `input_other` | 608-644 | 05（消息面入口）+ 09/10/11（分发目标，A-10） |
| `input_init` | 646-683 | 01（初始化序列） |
| `input_startup` | 685-694 | 01 |
| `main` | 696-704 | 01（启动锚点） |

**input.h（struct input_dev 13 字段 + 10 常量）**

| 符号 | 行 | 覆盖文档 |
|------|-----|---------|
| `EVENTBUF_SIZE` | 7 | 03 |
| `KBDMUX_MINOR`/`KBD0_MINOR`/`KBD_MINORS`/`MOUSEMUX_MINOR`/`MOUSE0_MINOR`/`MOUSE_MINORS` | 9-15 | 03/99 |
| `KBDMUX_DEV`/`FIRST_KBD_DEV`/`LAST_KBD_DEV`/`MOUSEMUX_DEV`/`FIRST_MOUSE_DEV`/`LAST_MOUSE_DEV`/`INPUT_DEV_MAX` | 18-26 | 03/99 |
| `struct input_dev`（minor/owner/label/eventbuf/tail/count/opened/suspended/caller/grant/req_id/selector/leds） | 29-43 | 03 |

**lib/libinputdriver/inputdriver.c（7 函数 + 4 静态变量）**

| 符号 | 行 | 覆盖文档 |
|------|-----|---------|
| `input_endpt`/`kbd_id`/`mouse_id`/`running`（静态） | 11-15 | 12 |
| `inputdriver_announce` | 20-39 | 12（`drv.inp.<label>` 发布 + A-2） |
| `inputdriver_send_event` | 42-79 | 12（阻塞 ipc_send + 失败重置 endpoint + A-9） |
| `do_conf` | 82-117 | 12（ds label 校验 + 保存 id） |
| `do_setleds` | 119-138 | 12（source 校验 + idr_leds 回调） |
| `inputdriver_process` | 141-174 | 12（notify 分发 + 消息分发） |
| `inputdriver_terminate` | 177-186 | 12 |
| `inputdriver_task` | 188-206 | 12（主循环） |

**chardriver input 使用面（lib/libchardriver/chardriver.c）**

| 符号 | 行 | 覆盖文档 |
|------|-----|---------|
| `chardriver_task` | 549-573 | 02（主循环） |
| `chardriver_process` | 455-536 | 02（分发面 + open_devs 门卫 + BDEV_OPEN） |
| `chardriver_announce` | 99-127 | 02（drv.chr.<label> + sys_statectl CLEAR_IPC_REFS + 清 open_devs） |
| `chardriver_reply_task` | 129-151 | 02/07（挂起读唤醒） |
| `chardriver_reply_select` | 153-174 | 02/08（selector 唤醒） |
| `chardriver_get_minor` | 575-598 | 02（minor 提取） |
| `do_open`/`do_close`/`do_transfer`/`do_ioctl`/`do_cancel`/`do_select`（279/310/328/363/392/416）、`send_reply`/`chardriver_reply`（177/195）、`is_open_dev`/`set_open_dev`/`clear_open_devs`（70/85/61）、`do_block_open`（438） | 61-453 | 02（分发实现 + EDONTREPLY/ERESTART 过滤 + BDEV_OPEN） |

**外部消费者契约点**

| 符号 | 位置 | 覆盖文档 |
|------|------|---------|
| TTY `do_input`（TTY_INPUT_UP/TTY_INPUT_EVENT 分支） | `drivers/tty/tty/arch/i386/keyboard.c:124-176` | 13 |
| TTY `set_leds`（INPUT_SETLEDS 请求） | `keyboard.c:369-384` | 13 |
| TTY 消息接入（TTY_INPUT_UP/EVENT case） | `drivers/tty/tty/tty.c:209-210` | 13 |
| pckbd `pckbd_init`（flags + announce） | `pckbd.c:465-487` | 14 |
| pckbd 键盘事件上报 | `pckbd.c:365`（kbd_process 路径） | 14 |
| pckbd 鼠标事件上报（按钮/位移） | `pckbd.c:394,407`（kbdaux_process） | 14 |
| pckbd `pckbd_leds`（LED 位 → 端口） | `pckbd.c:418-431` | 14 |
| pckbd `pckbd_intr`/`pckbd_alarm` | `pckbd.c:434-463` | 14 |
| pckbd 主循环 | `pckbd.c:500-504` | 14 |
| 设备节点表（kbdmux/kbd0-3/mousemux/mouse0-3） | `commands/MAKEDEV/MAKEDEV.sh:330-343` | 05/99 |

### 5.4 外部消费者与跨服务契约

| 外部服务 | 契约点 | 覆盖文档 | 核对 |
|---------|--------|---------|------|
| VFS | CDEV_OPEN/READ/IOCTL/CANCEL/SELECT 请求 + grant safecopy + `/dev/kbd*`/`/dev/mouse*` 节点（INPUT_MAJOR=64） | 02/05/06~08 | 已核对 |
| DS | `drv.inp.<label>` 订阅/发布 + label 删除 → ESRCH（驱动死亡检测） | 11/12 | 已核对（使用面；DS server 内部属 07-stage-ds） |
| TTY | TTY_INPUT_UP 握手 + TTY_INPUT_EVENT 事件消费 + INPUT_SETLEDS 请求（仅 TTY 来源被接受） | 13/05/10 | 已核对 |
| pckbd | 驱动侧 announce/send_event/leds/intr/alarm（libinputdriver 全使用面） | 14/12 | 已核对 |
| RS | `system.conf:400-403` 权限（ipc: SYSTEM pm vfs rs ds tty vm）+ 运行时加载（不在 boot_image） | 05/00 | 已核对 |
| kernel | 无直接契约（input 不在 boot_image，不持有硬件中断——硬件中断由 pckbd 驱动处理） | 00 | 已核对 |

### 5.5 明确排除 / 跳过的项

| 项 | 决策 | 证据 |
|----|------|------|
| `lib/libchardriver/` 非 input 使用面（block 驱动 do_block_open 之外的其他字符驱动专属逻辑、`MAX_NR_OPEN_DEVICES` 上限的通用语义） | 02 按 input 使用面裁剪；框架完整语义属驱动阶段（tty 等），不重复 | chardriver 为共享框架（input + tty + i2c/pci/fb 等 10+ 驱动共用） |
| `drivers/tty/` 完整实现（TTY 自身字符驱动语义、键盘映射、console 等） | 13 只覆盖 INPUT 相关契约面（UP/EVENT/SETLEDS）；TTY 重写属驱动阶段 | 范围声明（§1.1） |
| `drivers/hid/pckbd/` 硬件细节（键盘端口 I/O、watchdog、扫描码映射表、IRQ 钩子） | 14 只覆盖 libinputdriver 使用面；pckbd 重写属驱动阶段 | 范围声明 |
| `INPUT_DEV_KBD`/`INPUT_DEV_MOUSE`/`INVALID_INPUT_ID` 的 `_SYSTEM` 可见性 | 04/99 说明 `#ifdef _SYSTEM` 段（仅系统组件可见，libinputdriver/驱动使用）；用户态库不可见 | `input.h:6-15` `#ifdef _SYSTEM` |
| `rsvd[2]`（未来时间戳） | wire 兼容字段：server 置 0、驱动不填；Rust 保留字段 + 标注（与 devman `subsystem_offset` 同策略） | `input.h:31` `uint32_t rsvd[2]` |
| `rsvd1_id`/`rsvd2_id`（CONF 消息预留 joystick/未来） | wire 常量保留，固定 `INVALID_INPUT_ID`；Rust 保留字段 + 标注 | `ipc.h:235-236` + `input.c:520-521` |
| `INPUT_DEBUG` 条件编译 | 行为文档不展开；Rust 侧以 `#[cfg(feature = "debug")]` 等价（若保留） | `input.c:24` |
| USB HID 输入设备（libusbhid 等） | 无 libinputdriver 使用（grep 实证仅 pckbd）；如有 USB HID 驱动属驱动阶段，协议面不变 | `grep -rln inputdriver_ minix3/minix/` 仅 pckbd |
| `input_copy_events` 的 `panic`（count < event_count 不可能路径） | A-11：Rust 改 debug_assert + 返回 EIO（不 panic 到生产路径） | `input.c:136` |

### 5.6 覆盖结论与拆分答案

**1. 是否确保全部覆盖？** 是。§5.1~§5.4 已逐层核对：input server 1 个 .c + 1 个 .h（759 行）全部映射到 01~11（§5.1/§5.3，20 函数 + 1 数组 + 1 注册表 + 3 宏逐一落位）；chardriver 框架 input 使用面（600 行中 input 相关面）进入 02；协议面（com.h + ipc.h + input.h + inputdriver.h + chardriver.h + dmap.h + ttycom.h）进入 03/04/05/99；客户端库（libinputdriver，206 行）全部落入 12；外部消费者（DS/TTY/pckbd/RS/VFS）进入 11/12/13/14/05。§7.2 以命令证据复核，无遗漏。架构演进项（A-1~A-11，含 chardriver 框架 A-1、事件 id 语义 A-3、环形缓冲 A-4、挂起读状态机 A-5、LED 恢复 A-6、全单向协议 A-9）全部单列，不混入行为文档。

**2. 计划拆分为多少个文档，简述如何拆分？** **16 篇**：`00` 总览 + `01~14` 语义模块 + `99` 全局概念，按 **7 个阶段**组织——阶段 1 启动入口（01）→ 阶段 2 运行框架（02）→ 阶段 3 核心数据结构与协议面（03/04/05）→ 阶段 4 字符设备操作面（06/07/08，按 open→read→ioctl/cancel/select 顺序）→ 阶段 5 事件与 LED 机制（09/10）→ 阶段 6 驱动生命周期（11）→ 阶段 7 客户端库与外部消费者（12/13/14）。拆分原则：每篇一个语义单元 + 位置可回答性 + 禁止前向引用；以函数清单（§5.3）为唯一边界准绳。

---

## 6. 实施路线

> 每篇新文档 = 依据 §3 讲述结构 + §4 ARCH 标注 + §5 覆盖契约从零写作。所有 P0 修复完成后才可推进下一篇。当前 12-stage-input 无历史主线文档（仅占位 README + `tmp/input/` 逐行笔记素材），全部为新建。

1. **00-input-overview 新建**（导航，含 §1.2 启动时序图 + §1.3 事件旅程/驱动生命周期次主线）
2. **01-input-init-main 新建**（启动主线锚点：main/startup/init 序列 + chardriver_task 调用点）
3. **02-chardriver-framework 新建**（框架契约 + A-1 决策）
4. **03/04/05 数据结构与协议面三篇新建**（设备结构 + 事件格式 + 消息契约；A-3/A-4/A-7 标注）
5. **06/07/08 字符设备操作面三篇新建**（open/close、read/挂起、ioctl/cancel/select；A-5/A-8/A-11 标注）
6. **09/10 事件与 LED 机制两篇新建**（事件旅程核心 + LED 广播；A-4/A-6/A-9 标注）
7. **11 驱动生命周期新建**（connect/alloc/disconnect + DS 契约；A-2 标注）
8. **12 客户端库新建**（libinputdriver 全函数；A-9 标注）
9. **13/14 外部消费者两篇新建**（TTY 契约 + pckbd 契约）
10. **99-input-global-concepts 新建**（常量/错误码/跨服务引用收口）
11. **README.md 重建**（文档清单 + 启动链路位置，参照 09-stage-init/README.md 模式）

### 6.1 文档写作状态跟踪

> 每完成一篇，将状态改为 `reviewed`（附 scan 日期）。全部 `reviewed` 且无 P0 遗留 = 阶段完成。

| 编号 | 状态 | 首轮 review 日期 | 备注 |
|------|------|-----------------|------|
| 00 | pending | — | 新建导航 |
| 01 | reviewed | 2026-09-04 | 新建（启动锚点；scan: .review/codex/fork-syscall-rewrite/scans/01-input-init-main/，CONVERGED） |
| 02 | reviewed | 2026-09-04 | 新建（A-1 框架契约；scan: scans/02-chardriver-framework/，CONVERGED） |
| 03 | reviewed | 2026-09-04 | 新建（A-3/A-7；scan: scans/03-input-device-structs/，CONVERGED） |
| 04 | reviewed | 2026-09-04 | 新建（事件格式契约；scan: scans/04-input-event-format/，CONVERGED） |
| 05 | reviewed | 2026-09-05 | 新建（协议面；scan: scans/05-input-message-contract/，CONVERGED） |
| 06 | reviewed | 2026-09-05 | 新建（含关闭修正 MINIX3 BUG；scan: scans/06-input-open-close/，CONVERGED） |
| 07 | reviewed | 2026-09-05 | 新建（A-5/A-11；scan: scans/07-input-read-suspend/，CONVERGED） |
| 08 | reviewed | 2026-09-05 | 新建（A-8；scan: scans/08-input-ioctl-cancel-select/，CONVERGED） |
| 09 | pending | — | 新建（A-4/A-9，事件旅程核心） |
| 10 | pending | — | 新建（A-6） |
| 11 | pending | — | 新建（A-2） |
| 12 | pending | — | 新建（A-9） |
| 13 | pending | — | 新建外部契约 |
| 14 | pending | — | 新建外部契约 |
| 99 | pending | — | 新建全局概念 |

---

## 7. Review 记录

> 本节记录 plan.md 自身的 review 过程（深度 review + minix3 回归 review），与最终 plan.md 同文档交付，保证"覆盖完整性核对"可追溯。

### 7.1 深度 review（2026-08-16）

**方法**：以 §5.3 函数清单为唯一准绳，对 input.c 全部 20 个函数 + input.h 全部符号 + libinputdriver 全部 7 个函数做"符号 → 文档"双射检查；再对 §5.1/§5.2/§5.4 逐条核对行号与 grep 证据；最后对照 review-patterns-skill 典型错误模式（跨文档/覆盖缺口/语义陷阱/ARCH 遗漏）。

**初版问题与修复（D-1 ~ D-11）**：

| # | 初版问题 | 级别 | 修复 |
|---|---------|------|------|
| D-1 | 初版将 `input_other` 完全归入 09（事件处理），未声明它是消息面入口（含 DS notify 分支与 INPUT_SETLEDS fall-through） | P1 | 05 增加"消息面入口"职责；09/10/11 仅承载 handler 目标；A-10 单列 |
| D-2 | 初版遗漏 `input_dev_active`/`input_dev_buf_empty`/`input_dev_buf_full` 三个宏的落位（03 定义、06/07 使用） | P1 | §5.3 增加 3 宏条目；03 职责含 active 语义 |
| D-3 | 初版未明确 mux 选择规则（kbd → KBDMUX_DEV / 其他 → MOUSEMUX_DEV）的边界条件（`input_dev->minor >= KBD0_MINOR && < KBD0_MINOR + KBD_MINORS`） | P1 | 09 职责写明 mux 选择判定；§1.3 事件旅程图补充 |
| D-4 | 初版把 `input_disconnect` 的唤醒语义（EIO 回复挂起 reader + select CDEV_OP_RD）只放在 11，未与 07/08 的唤醒通道交叉引用 | P2 | 11 职责增加"唤醒面 07/08 交叉引用"；07 边界补充三出口（resume/cancel/disconnect） |
| D-5 | 初版遗漏 `input_revmap` 的 panic 语义（非法 id → `panic("reverse-mapping invalid ID")`）的 Rust 决策 | P1 | A-7 单列（revmap 失败返回 `Option`/`Result`）；03 职责含 revmap |
| D-6 | 初版未声明 `rsvd1_id`/`rsvd2_id`（CONF 预留 joystick）与 `rsvd[2]`（未来时间戳）的兼容策略 | P2 | §5.5 排除表新增两项（wire 保留 + 标注，与 devman 同策略） |
| D-7 | 初版将 `KIOCSLEDS` 位映射（KBD_LEDS_NUM→INPUT_LED_NUMLOCK 等）只放在 08，未指明 `kio_leds_t` 结构来源（`sys/kbdio.h`）与 ioctl 常量来源（`sys/sys/ttycom.h:174`） | P1 | §5.2 增加 ttycom.h/kbdio.h 条目；08 职责补位映射细节 |
| D-8 | 初版遗漏 `chardriver_announce` 的 `sys_statectl(SYS_STATE_CLEAR_IPC_REFS)`（重启后解阻塞旧调用者）语义 | P2 | 02 职责补充（drv.chr.<label> + CLEAR_IPC_REFS + 清 open_devs） |
| D-9 | 初版未将 `input_tab`（chardriver 回调注册表）落位——它同时是 01 的注册面与 05 的协议面锚点 | P2 | §5.3 增加 `input_tab` 条目（01 + 05） |
| D-10 | 初版未声明 pckbd 的 `INPUT_DEV_MOUSE` 条件（`aux_available != 0` 时 flags 含 MOUSE）与 typemask 语义 | P2 | 14 职责补 flags 生成条件；11 的 typemask 处理已覆盖 |
| D-11 | 初版缺少测试基线（stub 无测试，需与 11-stage-devman §3.5 先例一致） | P2 | 新增 §3.5（`cargo check`/`cargo test` 实测） |

**结论**：修复后按 §5.3 函数清单反向核对——`servers/input/` 20 个函数 + 1 数组 + 1 注册表 + 3 宏逐一落入 01~11；libinputdriver 7 个函数落入 12；chardriver input 使用面落入 02；协议面 6 头文件 + 4 消息结构落入 03/04/05/99；外部消费者（VFS/DS/TTY/pckbd/RS）落入 02/05/11/12/13/14。**语义全覆盖，无遗漏**。

### 7.2 minix3 源码回归 review（2026-08-16）

**方法**：对 `minix3/minix/servers/input/` 全部 .c/.h + 协议头文件 + libinputdriver + chardriver + TTY/pckbd 契约点逐一 grep 核对 §5.1/§5.2/§5.3/§5.4 映射，并抽查 mux 选择、fall-through、id 越界、owner 校验、溢出覆盖、LED 恢复、CONF 失败语义。

**证据**：

```bash
wc -l minix3/minix/servers/input/*.c minix3/minix/servers/input/*.h   # 704 + 45 = 759 行，与 §1 一致
wc -l minix3/minix/lib/libinputdriver/inputdriver.c                   # 206 行，与 §1 一致
grep -nE "^(static )?(void|int|ssize_t|devminor_t|struct input_dev \*)" minix3/minix/servers/input/input.c   # 20 函数起始行，与 §5.3 行号表一致
grep -n "INPUT_DEV_MAX\|id < 0\|id >= INPUT_DEV_MAX" minix3/minix/servers/input/input.c   # :22 devs 定义 + :384 越界丢弃 → A-3 成立
grep -n "mux_dev" minix3/minix/servers/input/input.c                   # :379-407（:395/:397 KBD0_MINOR 判定）→ mux 选择规则成立
grep -n "tail + 1\|count--\|Overflow" minix3/minix/servers/input/input.c   # :339-341 溢出覆盖最旧 → A-4 成立
grep -n "dev->leds = mask\|devs\[kbd_id\].leds" minix3/minix/servers/input/input.c   # :228 保存 + :527 恢复 → A-6 成立
grep -n "FALLTHROUGH" minix3/minix/servers/input/input.c               # :636 INPUT_SETLEDS fall-through → A-10 成立
grep -n "INVALID_INPUT_ID" minix3/minix/servers/input/input.c          # alloc 失败仍发 CONF（:516-522）→ 语义陷阱 3 成立
grep -n "src != RS\|m_source == TTY_PROC_NR\|m_source" minix3/minix/servers/input/input.c | head   # :631 TTY-only SETLEDS + :388 owner 校验
grep -n "no real replies" minix3/minix/include/minix/com.h             # :886 → A-9 成立
grep -n "ds_subscribe\|ds_check\|ds_retrieve_u32\|ds_retrieve_label_name\|ds_retrieve_label_endpt" minix3/minix/servers/input/input.c   # :665/:571/:572/:488/:593 → 11 契约成立
sed -n '400,403p' minix3/etc/system.conf                               # service input 权限
sed -n '330,343p' minix3/minix/commands/MAKEDEV/MAKEDEV.sh             # /dev 节点表（kbdmux/mousemux/kbd0-3/mouse0-3）
sed -n '124,176p' minix3/minix/drivers/tty/tty/arch/i386/keyboard.c    # do_input UP/EVENT 分支 → 13 成立
sed -n '369,384p' minix3/minix/drivers/tty/tty/arch/i386/keyboard.c    # set_leds INPUT_SETLEDS 请求 → 13 成立
grep -n "inputdriver_" minix3/minix/drivers/hid/pckbd/pckbd.c          # announce/send_event/task 使用点 → 14 成立
grep -rln "inputdriver_" minix3/minix/drivers/                         # 仅 pckbd → §5.5 USB HID 排除成立
grep -rn "chardriver" os/                                               # os/ 无 chardriver → A-1 成立
```

**结论**：input server 的 .c/.h 全部映射到新文档，无遗漏；20 个函数 + 全部符号逐一定位；协议面 + libinputdriver + chardriver 使用面 + TTY/pckbd 契约全部进入覆盖契约；A-1~A-11 与 minix3 现状对照成立（id 越界、owner 校验、mux 选择、溢出覆盖、LED 恢复、fall-through、CONF 失败语义均以 grep 实证）。**覆盖完整性通过**。

### 7.3 写作前置决策（2026-08-16 记录，待实现阶段确认）

| 决策点 | 建议 | 依据 |
|--------|------|------|
| A-1 chardriver 框架 | 倾向新建共享框架 crate（`os/libs/minix-chardriver/`），因 TTY/其余字符驱动（i2c/pci/fb/log/random 等 10+）同样依赖；CDEV 消息常量入 `minix-types`；不阻塞 input 重写（内部最小等价实现可先行） | `os/` 无 chardriver；chardriver 为多驱动共享框架（grep 实证） |
| A-3 事件 id 越界 | 保持静默丢弃（外部契约：驱动只应使用 CONF 分配的 id） | C 行为 `if (id < 0 || id >= INPUT_DEV_MAX) return;`（:384-385） |
| A-4 环形缓冲 | 保持覆盖最旧语义（驱动持续上报时 reader 不得丢新事件） | C 行为（:339-341），TTY 转发路径依赖 |
| A-9 全单向协议 | 保持无回复约定；server 侧 asynsend、驱动侧阻塞 ipc_send（防堆积 + 崩溃检测） | com.h:886 明言 + inputdriver.c:42-79 |
| A-10 SETLEDS 来源 | 仅 TTY 来源接受（`:631`）；其余记日志忽略（fail-closed） | C 行为（fall-through 到 unexpected 日志） |
| A-11 错误码 | 全部 errno 映射；`input_copy_events` 不可能 panic 改 debug_assert + EIO | review-core 错误码要求 |

---

## 8. 参见

- `draft/README.md` — 旧占位 README（素材）
- `../../../../tmp/input/` — 早期逐行笔记（`tmp_input.c.md`/`tmp_input.h.md`，fork 主线时代素材，供改写参考）
- `../00-master-plan/README.md` — 目录重排与新主线说明（input 属 RS 加载组）
- `../01-stage-kernel/00-kernel-overview.md` — 讲述结构参照（阶段划分/组织原则）
- `../02-stage-vm/plan.md`、`../11-stage-devman/plan.md` — 同流程先例（plan 结构与覆盖契约格式）
- `../07-stage-ds/plan.md` — DS server 契约先例（input 的 DS 使用面依赖 07-stage-ds）
- `minix3/minix/servers/input/` — ground truth（服务端）
- `minix3/minix/lib/libinputdriver/`、`minix3/minix/lib/libchardriver/` — 客户端库与框架
- `minix3/minix/drivers/tty/tty/arch/i386/keyboard.c`、`minix3/minix/drivers/hid/pckbd/pckbd.c` — 外部消费者
- `os/servers/input/` — Rust 实现（当前 stub）
