# 12-stage-input 文档重建蓝图（HY4）

## 0. 元数据

### 0.1 执行头部

```text
your_name(AI agent name) = HY4
target_dir(关注的工作目录) = 12-stage-input
repo_root(仓库根目录) = /home/xzhao/github/minix-rs

任务 = R 相·重建蓝图：输出 target_dir/doc_rerank_HY4.md，不改任何正文。
       只读 C 代码不够，还必须读：本目录全部文档的头部声明、os/ 目录对应入口、
       00-*-overview.md 的导航表。
约束 = 不得引用 .design/ 与 tmp_design_and_todo/；任何落盘产物必须带
       _HY4 后缀；不得读取或照抄其它 AI 的 doc_rerank_* 产物。
```

- **执行日期**：2026-09-19
- **当前提交号**：`ebc8ae72b`（`git rev-parse --short HEAD` 实测）
- **产物**：仅本文件 `12-stage-input/doc_rerank_HY4.md`

### 0.2 审查范围

**算文档（重建对象）**：目录下 16 篇编号文档

| 编号 | 文件 | 行数 |
|------|------|------|
| 00 | `00-input-overview.md` | 107 |
| 01 | `01-input-init-main.md` | 334 |
| 02 | `02-chardriver-framework.md` | 301 |
| 03 | `03-input-device-structs.md` | 306 |
| 04 | `04-input-event-format.md` | 298 |
| 05 | `05-input-message-contract.md` | 380 |
| 06 | `06-input-open-close.md` | 220 |
| 07 | `07-input-read-suspend.md` | 279 |
| 08 | `08-input-ioctl-cancel-select.md` | 268 |
| 09 | `09-input-event-processing.md` | 307 |
| 10 | `10-input-setleds.md` | 187 |
| 11 | `11-input-driver-connect.md` | 355 |
| 12 | `12-libinputdriver.md` | 364 |
| 13 | `13-tty-consumer.md` | 273 |
| 14 | `14-pckbd-driver.md` | 276 |
| 99 | `99-input-global-concepts.md` | 132 |

合计 4187 行。

**算参考材料（不重建，只作线索与约束来源）**：`plan.md`（483 行）、`todo.md`（191 行）、`draft/`（占位）、`.review/`（历史扫描快照，B 相不回改）。

**算范围外**：`../16-stage-drivers/`（tty/pckbd 实现篇所有权）、`../07-stage-ds/`（DS 内部）、`../15-stage-fs/`（VFS 侧）、`../edge_todo.md`（跨阶段接线）。

### 0.3 读取清单

**C 源码（本 stage 全部，逐文件读全）**

| 文件 | 行数 | 角色 |
|------|------|------|
| `minix3/minix/servers/input/input.c` | 704 | 服务本体，20 个函数 |
| `minix3/minix/servers/input/input.h` | 45 | 十槽结构与编号常量 |
| `minix3/minix/lib/libchardriver/chardriver.c` | 600 | 字符驱动前台（全读） |
| `minix3/minix/lib/libinputdriver/inputdriver.c` | 206 | 驱动侧客户端库（全读） |
| `minix3/minix/include/minix/input.h` | 333 | 事件词汇（全读） |
| `minix3/minix/include/minix/inputdriver.h` | 33 | 驱动回调表 |
| `minix3/minix/include/minix/chardriver.h` | 36 | 字符驱动回调表 |
| `minix3/minix/include/sys/kbdio.h` | 22 | `kio_leds_t` 与灯位 |
| `minix3/minix/include/minix/com.h` | 节选 870-937 | 消息编号 |
| `minix3/minix/include/minix/dmap.h` | 节选 :78 | `INPUT_MAJOR 64` |
| `minix3/minix/drivers/tty/tty/tty.c` | 节选 :205-214 | TTY 消息接入 |
| `minix3/minix/drivers/tty/tty/arch/i386/keyboard.c` | 节选 :124-176, :369-384 | `do_input`/`set_leds` |
| `minix3/minix/drivers/hid/pckbd/pckbd.c` | 507（结构读） | 驱动侧模型用户 |

**非 C 制品**

| 文件 | 位置 | 承载主题 |
|------|------|---------|
| `minix3/etc/system.conf` | :400-403 | `service input` 声明（RS 加载） |
| `minix3/minix/commands/MAKEDEV/MAKEDEV.sh` | :330-343 | `/dev` 节点表 |
| `minix3/minix/servers/input/Makefile` | 10 行 | 链接 libchardriver + libsys |
| `minix3/minix/lib/libchardriver/Makefile` | 10 行 | `-D_MINIX_SYSTEM` |
| `minix3/minix/lib/libinputdriver/Makefile` | 9 行 | 同上 |
| `minix3/minix/kernel/table.c` | grep 阴性 | 证明 input 不在 boot image |

**Rust 入口**

| 路径 | 行数 | 角色 |
|------|------|------|
| `os/servers/input/src/` | 4874（13 文件） | 服务器 |
| `os/servers/input/src/serve.rs` | 329 | **传输与主循环（无文档）** |
| `os/servers/input/src/dispatcher.rs` | 948 | **裁决流水线（无独立文档）** |
| `os/servers/input/src/effects.rs` | 344 | **效应出口（无独立文档）** |
| `os/libs/minix-chardriver/src/` | 968（3 文件） | 共享前台库 |
| `os/libs/minix-sys/src/inputdriver.rs` | 486 | 驱动侧客户端 |
| `os/libs/minix-types/src/ipc/{input,input_event,key_codes}.rs` | 529 + 事件/码表 | 协议与词汇 |
| `os/drivers/hid/pckbd/src/` | 6 文件 | pckbd Rust 实现 |

**边界材料**：`../00-master-plan/README.md:30`（阶段定位）、`../edge_todo.md`（E-INWIRE/E-CDRCONV/E-TTYEVENT/E-PCKBDREG）、`12-stage-input/plan.md`、`12-stage-input/todo.md`（13 条 IN-* 全部 ✅）。

**前一 stage**：`../11-stage-devman/00-*-overview.md`（已读 §1.1 启动位置、§1.2 两条主线、§1.3 四条写作原则）。

**写法范例**：`../01-stage-kernel/06-todo.md`（已读 §1-§3，学其"目标文档结构 + 分步执行 + 验收标准"三段式，未搬任何结论）。

### 0.4 使用的命令与关键输出（证据摘录）

```sh
# 1. input 不在 boot image
rg -i "input" minix3/minix/kernel/table.c            # 无输出

# 2. RS 加载声明
sed -n '400,404p' minix3/etc/system.conf
# service input { ipc SYSTEM pm vfs rs ds tty vm; priority 1; };

# 3. 主设备号与节点表
grep -n "INPUT_MAJOR" minix3/minix/include/minix/dmap.h   # 78: 64
sed -n '330,343p' minix3/minix/commands/MAKEDEV/MAKEDEV.sh
# kbdmux c 64 0 / mousemux c 64 64 / kbd0-3 c 64 1-4 / mouse0-3 c 64 65-68

# 4. 消息编号
sed -n '872,893p' minix3/minix/include/minix/com.h
# TTY_RQ_BASE 0x1300 / TTY_INPUT_UP +2 / TTY_INPUT_EVENT +3
# INPUT_RQ_BASE 0x1500 / INPUT_CONF +0 / INPUT_SETLEDS +1
# INPUT_RS_BASE 0x1580 / INPUT_EVENT +0
# 886 行注释原文："The input protocol has no real replies. All messages are one-way."

# 5. C 侧全部 panic 点（19 处，跨 4 文件）
grep -n "panic(" minix3/minix/servers/input/input.c \
  minix3/minix/lib/libchardriver/chardriver.c \
  minix3/minix/lib/libinputdriver/inputdriver.c \
  minix3/minix/drivers/tty/tty/arch/i386/keyboard.c
# input.c:79 (revmap) / :138 (copy 存货不足) / :666 (subscribe 失败)
# chardriver.c:90,112,116,120,138,162,215(#if 0),221,226,270,565
# inputdriver.c:30,34,201 ; keyboard.c:177

# 6. Rust 测试分布（实测 grep -c "#[test]"）
# connect 11 / dispatcher 11 / effects 6 / error 2 / eventbuf 10
# handlers 17 / init 5 / produce 17 / serve 2 / setleds 4 / structs 6 = 91
# minix-sys/inputdriver.rs 7 ; minix-types ipc/input.rs 13 / input_event.rs 6 / key_codes.rs 2

# 7. 代码注释里的文档锚点（20 处 / 15 文件）
grep -rn "input-init-main\|input-device-structs\|input-event-format\|..." --include=*.rs os/ | wc -l   # 20
```

---

## 1. C 真序

### 1.1 阶段类型判定

**判定：服务事件循环型（主）+ 集合型（次）。**

- 主判据：主体是 `chardriver_task` 无限消息循环（`chardriver.c:549-570`），一次到达进来、一次处理出去，符合"服务事件循环型"。
- 次判据：循环内有一组并行的字符设备请求（OPEN/CLOSE/READ/IOCTL/CANCEL/SELECT 六个），符合"集合型"的并行体特征，按 §5.2 用"统一框架篇 + 按场景分组 + 代表成员讲透 + 差异表"组织。
- 另有启动链特征（`main → input_startup → input_init → chardriver_task` 严格线性），按 §九 启动链型处理为骨架第一段。

本蓝图按"服务事件循环型"的推荐骨架组织（服务为什么存在 → 诞生与初始化 → 消息接口 → 核心数据结构 → 按场景分组的请求处理 → 查询与杂项 → 与邻接服务的协议），并把集合型的并行体规则用在请求处理组。

### 1.2 运行时真序表（启动段）

| # | 动作 | 锚点 | 说明 |
|---|------|------|------|
| S1 | RS 按 `system.conf:400-403` 运行时加载 input | `minix3/etc/system.conf:400-403`；`kernel/table.c` grep `input` 阴性 | 不在 boot image，属"RS 加载组"（与 IS/DEVMAN/IPC 同类） |
| S2 | `main()` 调 `input_startup()` | `input.c:696-704`；`:699` | 三行入口：登记回调、启动握手 |
| S3 | `sef_setcb_init_fresh(input_init)` 只登记首次诞生回调 | `input.c:688` | **重起不重跑 init**——C 只登记 `fresh` 一种，没有 `init_lu`/`init_restart` |
| S4 | `sef_startup()` 交出控制权，回调 `input_init` 在首次启动时被触发 | `input.c:690` | SEF 框架在此吸收 RS ping |
| S5 | 初始化第 1 步：清十槽位表，逐槽 `minor = input_revmap(i)`、`owner=NONE`、`tail=count=0`、`opened=FALSE`、`suspended=FALSE`、`selector=NONE`、`leds=0` | `input.c:652-662` | 依赖反查函数 `input.c:67-80` |
| S6 | 初始化第 2 步：`ds_subscribe("drv\\.inp\\..*", DSF_INITIAL)`，失败即 `panic` | `input.c:665-666` | `DSF_INITIAL` 意味着订阅时补发存量键 |
| S7 | 初始化第 3 步：`chardriver_announce()` | `input.c:669` → `chardriver.c:99-124` | 三件事：`sys_statectl(SYS_STATE_CLEAR_IPC_REFS)`、`ds_publish_u32("drv.chr.input", DS_DRIVER_UP, DSF_OVERWRITE)`、`clear_open_devs()` |
| S8 | 初始化第 4 步：造空消息 `m_type = TTY_INPUT_UP`，`ipc_send(TTY_PROC_NR, &m)`，失败仅 `printf` | `input.c:671-677` | 全阶段唯一"失败只记日志"的初始化步 |
| S9 | 返回 `OK`，`main` 调 `chardriver_task(&input_tab)`，**永不返回** | `input.c:701` → `chardriver.c:549-570` | 无 `sef_setcb_*_terminate` 注册，服务不主动退出 |

### 1.3 运行时真序表（循环段）

| # | 动作 | 锚点 | 说明 |
|---|------|------|------|
| L1 | `sef_receive_status(ANY, &mess, &ipc_status)`；`EINTR && !running` 才退出，其余 `panic` | `chardriver.c:560-566` | 无限循环 |
| L2 | 通知优先：`is_ipc_notify` → `HARDWARE`/`CLOCK`/default 三路，**一律不回信** | `chardriver.c:463-482` | input 的 `cdr_intr`/`cdr_alarm` 均为 NULL（`input.c:31-39` 只填 7 槽），硬件/时钟通知被静默丢弃 |
| L3 | 块设备开门信代答 `BDEV_REPLY` + `ENXIO` | `chardriver.c:488-492`, `:438-450` | 防止 VFS 线程永久阻塞 |
| L4 | `IS_CDEV_RQ` → `chardriver_get_minor` 取次设备号 | `chardriver.c:494-501`, `:575-600` | 取不到直接返回 |
| L5 | 重启门：`!is_open_dev(minor)` 时，非 `CDEV_OPEN` 一律丢弃不回信；`CDEV_OPEN` 则 `set_open_dev` | `chardriver.c:506-513`, `:70-94` | 门集合容量 `MAX_NR_OPEN_DEVICES = 256`（`driver.h:41`），溢出 `panic`（`chardriver.c:89-90`） |
| L6 | 分派到六个搬运函数，分别调 `input_tab` 的钩子 | `chardriver.c:517-529`, `:279-433` | input 未注册 `cdr_write` → `do_transfer` 走 `r = EIO`（`chardriver.c:352-355`） |
| L7 | 回信判决 `chardriver_reply`：`EDONTREPLY` 仅 READ/WRITE/IOCTL/CANCEL 合法（否则 `panic`）；`SUSPEND` `panic`；`ERESTART` 静默不回 | `chardriver.c:195-274` | 三种下场 + 三种崩溃 |
| L8 | `send_reply`：`IPC_STATUS_CALL == SENDREC` → `ipc_sendnb`；否则 `asynsend3(AMF_NOREPLY)` | `chardriver.c:177-190` | 同步/异步双通道 |
| L9 | 未知 m_type（含 `INPUT_EVENT`/`INPUT_SETLEDS`/DS 通知）落到 `cdr_other = input_other` | `chardriver.c:525-528` → `input.c:608-641` | input 的第二个入口 |
| L10 | `input_other` 通知分支：来源 `DS_PROC_NR` → `input_check()`；其余来源只 `printf` | `input.c:611-620` | 唯一订阅的通知来源 |
| L11 | `input_other` 消息分支：`INPUT_EVENT` → `input_event(m)` | `input.c:625-628` | 驱动上报 |
| L12 | `input_other` 消息分支：`INPUT_SETLEDS` 且 `m_source == TTY_PROC_NR` → `input_set_leds(KBDMUX_MINOR, mask)`；**其余来源 fall-through 到 default 日志** | `input.c:630-639` | 灯令的单一来源过滤 |

### 1.4 运行时真序表（四条核心子链）

**A. 一次读请求**

| # | 动作 | 锚点 |
|---|------|------|
| A1 | `input_map(minor)` 失败 → `ENXIO` | `input.c:169-171` |
| A2 | `!active \|\| suspended` → `EIO`（单读者，已挂起不再收第二个） | `input.c:172-174` |
| A3 | `event_count = size / 20`；为 0 → `EIO` | `input.c:176-179` |
| A4 | 队列空：`CDEV_NONBLOCK` → `EAGAIN`；否则写挂起纸条四件套（`suspended/caller/grant/req_id`）并返回 `EDONTREPLY` | `input.c:181-193` |
| A5 | 有货：`event_count = min(event_count, count)` | `input.c:195-196` |
| A6 | `input_copy_events`：`count < event_count` → `panic`；算回绕 `wrap_left`；`sys_safecopyto` 两段；推进 `tail`、`count -= event_count`；返回 `20 * event_count` | `input.c:130-157` |

**B. 一次事件上报**

| # | 动作 | 锚点 |
|---|------|------|
| B1 | `id = m->...id`（**id 即数组下标**，与 minor 不是一套） | `input.c:382-383` |
| B2 | `id < 0 \|\| >= INPUT_DEV_MAX` → 静默 return | `input.c:384-385` |
| B3 | `devs[id].owner != m_source` → 静默 return | `input.c:388-390` |
| B4 | 选多路器：minor 落在键盘窗口 → `KBDMUX_DEV`；否则一律 `MOUSEMUX_DEV` | `input.c:393-397` |
| B5 | 三级跳板：`dev->opened` → 入本槽；否则 `mux->opened` → 入多路器；否则转交 TTY | `input.c:404-421` |
| B6 | 转交：造 `TTY_INPUT_EVENT`，复制 id/page/code/value/flags 五字段，**不翻译**；`ipc_send(TTY_PROC_NR)` 阻塞，失败只 `printf` | `input.c:409-420` |
| B7 | `input_process`：满则丢弃最旧（`tail+1`，`count--`）；写入 `page/code/value/flags/devid=id`、`rsvd[0]=rsvd[1]=0`；`count++` | `input.c:338-355` |
| B8 | 唤醒：**挂起优先于选择者**；挂起则 `input_copy_events(caller, grant, 1, ...)` **恰好一个事件** + `chardriver_reply_task` + `suspended=FALSE`；否则 `selector != NONE` → `chardriver_reply_select(selector, minor, CDEV_OP_RD)` + 清 selector | `input.c:357-370` |

**C. 一盏灯**

| # | 动作 | 锚点 |
|---|------|------|
| C1 | 调用者 `kio_leds_t.kl_bits` 三位（NUM/CAPS/SCROLL = 0x1/0x2/0x4，`sys/kbdio.h`） | `sys/kbdio.h:16-19` |
| C2 | `input_ioctl` 仅认 `KIOCSLEDS`，其余 → `ENOTTY`；先 `sys_safecopyfrom` 取结构体 | `input.c:256-276` |
| C3 | 位翻译 → `INPUT_LED_NUMLOCK/CAPSLOCK/SCROLLLOCK` 掩码 | `input.c:262-268` |
| C4 | `input_set_leds(minor, mask)`：遍历 `FIRST_KBD_DEV..LAST_KBD_DEV`；`minor != KBDMUX_MINOR && minor != dev->minor` 则跳过（**循环范围即过滤器，鼠标蒸发无分支**） | `input.c:221-235` |
| C5 | **先记后发**：`dev->leds = mask` 无条件执行；`owner != NONE` 才 `asynsend3(owner, &m, AMF_NOREPLY)`，失败仅 `printf` | `input.c:227-234` |
| C6 | 跨重启恢复：新驱动连接后立刻 `input_set_leds(devs[kbd_id].minor, devs[kbd_id].leds)` 补发 | `input.c:525-527` |
| C7 | 反向来源：TTY 发来的 `INPUT_SETLEDS` 以 `KBDMUX_MINOR` 广播（非 TTY 来源 fall-through 到日志） | `input.c:630-635` |

**D. 一个驱动的一生**

| # | 动作 | 锚点 |
|---|------|------|
| D1 | 驱动 `inputdriver_announce(type)`：取自己 label，`ds_publish_u32("drv.inp."+label, type, DSF_OVERWRITE)`（都是 `panic` 级失败） | `inputdriver.c:20-37` |
| D2 | 服务被 DS 通知唤醒 → `input_check()` 到来半：`while (ds_check(...) == OK)` → `ds_retrieve_u32` → `strncmp(key, "drv.inp.", 8)` 过滤 → `input_connect(owner, label, value)` | `input.c:570-585` |
| D3 | `input_connect` 先 `ds_retrieve_label_name` 反查发送方真实 label 并与键尾比对，不符则静默忽略 | `input.c:487-495` |
| D4 | `input_alloc_id`：按 typemask 分键盘/鼠标窗口；**同 label 已占槽则更新 owner 复用**；否则记第一个 `owner == NONE && !opened` 的空槽（断开但仍开着的槽不给）；无槽则记日志并返回 `INVALID_INPUT_ID` | `input.c:430-470` |
| D5 | 回信 `INPUT_CONF`：`kbd_id`/`mouse_id` + `rsvd1_id`/`rsvd2_id` 固定 `INVALID_INPUT_ID`；`asynsend3` 发出 | `input.c:514-523` |
| D6 | 离去半：遍历有主槽位 → `ds_retrieve_label_endpt(label, &owner)`；`OK` 则回写 owner；`ESRCH` → `input_disconnect`；其余只 `printf` | `input.c:588-602` |
| D7 | `input_disconnect`：挂起读者以 `EIO` 唤醒；选择者以 `CDEV_OP_RD` 通知；`owner = NONE`；**不动 `tail`/`count`（行李留下）** | `input.c:533-553` |
| D8 | 驱动侧收配置：`do_conf` 反查 label `"input"` 验明正身，存 `input_endpt`/`kbd_id`/`mouse_id`；两个 ID 都无效则打印"driver disabled" | `inputdriver.c:82-111` |
| D9 | 驱动侧上报：`inputdriver_send_event` 两道门（`input_endpt != NONE`、本类 `id != INVALID`）；`ipc_send` **阻塞**（不限流 + 测生死）；失败则 `input_endpt = NONE` 复位 | `inputdriver.c:42-74` |
| D10 | 驱动侧调灯：`do_setleds` 只认 `input_endpt`，调 `idr_leds(mask)`（可空） | `inputdriver.c:119-135` |

---

## 2. 知识点全集

> 编号规则：`K-0NN`。来源类型：**存**=现有文档已有；**新**=现有文档未讲，由 C 源码 / 非 C 制品 / OS 理论承载，本轮追加入池。
> 「现有位置」列：存量条目标注主讲述点（★）与复现点。

### 2.1 A 组 · 角色、进程模型与启动（K-001 ~ K-016）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-001 | 输入服务是"事件汇/传达室"：三矛盾（抢读、丢失、耦合） | 概念 | 存 | 00 §1.1★ | `input.c:1` 注释；`input.c:376` | 回答"为什么需要这个服务" |
| K-002 | 三条边界：不碰硬件 / 不解释按键 / 不做块设备 | 约束 | 存 | 00 §1.2★ | `input.c:2-8` include 面无 drivers 硬件头 | 回答"什么不该找它" |
| K-003 | 不在 boot image，由 RS 运行时加载 | 机制 | 存 | 00 §1.3、01 §1.1 | `system.conf:400-403`；`table.c` grep 阴性 | 回答"谁把它拉起来" |
| K-004 | 服务声明的 IPC 授权面（SYSTEM/pm/vfs/rs/ds/tty/vm） | 接口 | 存 | 99 §7 | `system.conf:402` | 回答"它能和谁说话" |
| K-005 | SEF 启动框架：`sef_setcb_init_fresh` + `sef_startup` | 机制 | 存（浅） | 01 §2.2 | `input.c:685-691`；`sef.h:9,60` | 回答"启动握手由谁驱动" |
| K-006 | **只登记 `fresh`：重起不重跑 `input_init`** | 约束 | **新** | 无 | `input.c:688`（无 `init_lu`/`init_restart` 注册） | 回答"驱动重起时表会不会被清空"（不会） |
| K-007 | 四步初始化及顺序依赖 | 机制 | 存 | 01 §1.4★、01 §2.4 | `input.c:646-680` | 回答"启动做了几件事" |
| K-008 | 清表循环（十槽、逐槽回填 minor） | 机制 | 存 | 01 §2.4、03 §4.5 | `input.c:652-662` | 回答"开机第一件事" |
| K-009 | `ds_subscribe("drv\\.inp\\..*", DSF_INITIAL)`，失败即崩溃 | 约束 | 存 | 01 §2.4、11 §2.5 | `input.c:665-666` | 回答"怎么知道驱动来了" |
| K-010 | `DSF_INITIAL` = 订阅时补发存量键 | 接口 | 存 | 99 §6 | `input.c:665`；`minix/ds.h` | 回答"订阅前就存在的驱动会不会漏" |
| K-011 | `chardriver_announce` 三件事 | 机制 | 存 | 02 §2.8★、01 §2.4 | `chardriver.c:99-124` | 回答"什么叫'向文件系统宣告'" |
| K-012 | `SYS_STATE_CLEAR_IPC_REFS`：释放上一代阻塞调用者 | 机制 | 存 | 02 §2.8 | `chardriver.c:111-112` | 回答"重起后谁来救旧调用者" |
| K-013 | `TTY_INPUT_UP` 握手，失败只记日志 | 接口 | 存 | 01 §2.4、05 §2、13 §2.3 | `input.c:671-677` | 回答"终端怎么知道我起来了" |
| K-014 | 主循环永不返回；无 terminate 注册 | 约束 | 存（散） | 01 §1.3 | `input.c:701`；`chardriver.c:549-570` | 回答"服务会退出吗"（不会） |
| K-015 | 单线程事件循环执行模型：无锁、无 `Arc`/`Mutex` | 架构演进 | 存（仅代码） | 无文档正文 | `os/servers/input/src/lib.rs:9-14` | 回答"为什么这些类型不能跨线程" |
| K-016 | 回调表七槽，`cdr_intr`/`cdr_alarm`/`cdr_write` 空缺 → 硬件与时钟通知静默丢弃、写请求得 `EIO` | 约束 | **新** | 无 | `input.c:31-39`；`chardriver.c:148-149,352-355,463-482` | 回答"input 会收到中断吗"（不会） |

### 2.2 B 组 · 事件词汇（K-020 ~ K-030）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-020 | 20 字节事件六字段（page/code/value/flags/devid/rsvd[2]） | 数据结构 | 存 | 04 §1.2★、99 §3 | `minix/input.h:25-32` | 回答"一次按键长什么样" |
| K-021 | 五个事件页（借 USB HID，号码不连续 1/7/8/9/C） | 概念 | 存 | 04 §1.3★、04 §2.2 | `minix/input.h:34-39` | 回答"码为什么分页" |
| K-022 | 值与标志：`INPUT_RELEASE/PRESS`、`INPUT_FLAG_ABS/REL` | 数据结构 | 存 | 04 §2.3★ | `minix/input.h:41-47` | 回答"按下松开怎么表示" |
| K-023 | 通用桌面码五个 | 数据结构 | 存 | 04 §2.4★ | `minix/input.h:50-57` | 回答"鼠标位移用什么码" |
| K-024 | 键盘码 215 个（美国布局整张编号） | 数据结构 | 存 | 04 §2.5★ | `minix/input.h:59-290` | 回答"空格是哪个码" |
| K-025 | LED 码 / 按键码 / 消费类码 | 数据结构 | 存 | 04 §2.7★ | `minix/input.h:292-331` | 回答"灯和多媒体键在哪页" |
| K-026 | 系统段三常量（`INPUT_DEV_KBD/MOUSE`、`INVALID_INPUT_ID`） | 常量 | 存 | 04 §2.6★ | `minix/input.h:6-15`（`#ifdef _SYSTEM`） | 回答"只有系统组件能看见的名字" |
| K-027 | 三层责任：词汇归规范 / 翻译归驱动 / 本地化归读者 | 架构 | 存 | 04 §1.1★ | `minix/input.h:17-22` 注释 | 回答"谁来把扫描码翻成事件码" |
| K-028 | `rsvd[2]` 全零纪律（入队时强制清零） | 约束 | 存 | 04 §3.4 | `input.c:353-354` | 回答"保留字段要不要管" |
| K-029 | 码表机械生成 + 生成器进仓库 + 逐值锁测试 | 工具 | 存 | 04 §3.2★ | `os/libs/minix-types/src/ipc/key_codes.rs:16` | 回答"215 个码怎么保证没错" |
| K-030 | 词汇迁出 server crate、落 `minix-types` 单一权威（ARCH IN-P2-2） | 架构演进 | 存 | 04 头部★、99 §1 | `input_event.rs:14`、`key_codes.rs:16` | 回答"驱动和终端去哪引这份词汇" |

### 2.3 C 组 · 设备表与编号（K-031 ~ K-042）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-031 | 十槽布局：键盘总机 + 4 键盘 + 鼠标总机 + 4 鼠标 | 数据结构 | 存 | 03 §1.2★ | `input.h:18-26` | 回答"一共能接几块键盘" |
| K-032 | 两套编号：稀疏 minor（留白买兼容）vs 连续 index | 概念 | 存 | 03 §1.1★、99 §4 | `input.c:47-51` 注释；`input.h:9-26` | 回答"为什么门牌号跳号" |
| K-033 | `input_map` 正查：minor → 槽位，查不到返回 NULL | 机制 | 存 | 03 §2.4★ | `input.c:44-62` | 回答"打开 /dev/kbd2 找哪间房" |
| K-034 | `input_revmap` 反查：index → minor，非法则 `panic`（A-7 改显式错误） | 机制+演进 | 存 | 03 §2.5★、03 §3.4 | `input.c:67-80`（`panic` 在 :79） | 回答"反查失败会怎样" |
| K-035 | `struct input_dev` 十三字段逐字段语义 | 数据结构 | 存 | 03 §1.3★、03 §2.3 | `input.h:29-43` | 回答"每个槽记住什么" |
| K-036 | 三判断宏 `active`/`buf_empty`/`buf_full` | 机制 | 存 | 03 §1.4★、03 §2.6 | `input.c:24-28` | 回答"营业吗 / 空的吗 / 满的吗" |
| K-037 | `Minor` / `DeviceIndex` 两个新类型（而非两个整数） | 架构演进 | 存 | 03 §3.1★、03 §4.2 | `structs.rs:80,103` | 回答"两套编号怎么不混" |
| K-038 | `InputTable::fresh()` 与 `InputDevice` 初值 | 数据结构 | 存 | 03 §4.5★ | `structs.rs:310,323` | 回答"干净的表长什么样" |
| K-039 | label 截断保留终结符 / 尾部清零 | 约束 | 存 | 03 §3.5 | `structs.rs:282` | 回答"驱动名超长怎么办" |
| K-040 | `NONE` 哨兵与 `Endpoint::NONE` 收拢 | 约束 | 存 | 03 §3.3 | `input.h:31`；`minix-types/src/types/endpoint.rs` | 回答"没有主人怎么表示" |
| K-041 | `DEVICE_COUNT=10` / `EVENT_BUFFER_SIZE=32` / `EVENT_BYTES=20` | 常量 | 存 | 99 §4★、03 §2.1 | `structs.rs:27,35,42`；`input.h:7` | 回答"容量是多少" |
| K-042 | `INPUT_MAJOR=64` 与 `/dev` 节点表（kbdmux/kbd0-3/mousemux/mouse0-3） | 接口 | 存 | 05 §2.5★、99 §4 | `dmap.h:78`；`MAKEDEV.sh:330-343` | 回答"/dev 下的名字怎么来的" |

### 2.4 D 组 · 三面消息契约（K-050 ~ K-066）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-050 | CDEV 七请求三回信的编号与 m10 字段布局 | 接口 | 存 | 02 §1.4★、02 §2.1-2.2 | `com.h:915-937`；`chardriver.c:1-44` 头注释 | 回答"文件系统发来的信长什么样" |
| K-051 | input 五消息编号与方向（CONF/SETLEDS/EVENT/UP/EVENT-TTY） | 接口 | 存 | 05 §2.1★、99 §2 | `com.h:872-893` | 回答"消息号是多少" |
| K-052 | 全单向纪律：input 协议无真回信 | 约束 | 存 | 05 §1.2★、12 §1.1、00 §3 | `com.h:886` 注释原文 | 回答"驱动上报要不要等回答"（不要） |
| K-053 | `INPUT_CONF` 载荷（kbd_id/mouse_id + 两个保留槽固定 INVALID） | 接口 | 存 | 05 §2.2★ | `ipc.h:232-240`；`input.c:519-520` | 回答"服务告诉驱动什么" |
| K-054 | `INPUT_SETLEDS` 载荷（led_mask） | 接口 | 存 | 05 §2.2★ | `ipc.h:242-247` | 回答"灯令装什么" |
| K-055 | `TTY_INPUT_EVENT` 载荷（id/page/code/value/flags） | 接口 | 存 | 05 §2.2★ | `ipc.h:249-258` | 回答"转交事件装什么" |
| K-056 | `INPUT_EVENT` 载荷（同上五字段） | 接口 | 存 | 05 §2.3★ | `ipc.h:993-1001` | 回答"驱动上报装什么" |
| K-057 | 联合体接入四条（`ipc.h:2434-2436`、`:2517`） | 接口 | 存 | 05 §2.4★ | `ipc.h:2434-2436,2517` | 回答"消息怎么塞进 union" |
| K-058 | `_ASSERT_MSG_SIZE` 尺寸断言（56 字节载荷） | 约束 | 存 | 05 §2.2 | `ipc.h:241,248,259,1001` | 回答"载荷尺寸谁来保证" |
| K-059 | DS 面：`drv.inp.<label>` 键与 `DSF_OVERWRITE` | 接口 | 存 | 11 §1.1★、12 §2.2、99 §6 | `inputdriver.c:23-34`；`input.c:562-584` | 回答"驱动怎么宣告" |
| K-060 | DS 面：`drv.chr.<label>`（字符驱动上线标记） | 接口 | 存 | 02 §2.8、99 §6 | `chardriver.c:105-120` | 回答"文件系统怎么发现字符驱动" |
| K-061 | `KIOCSLEDS` 构造与 `kio_leds_t` | 接口 | 存 | 05 §2.6★、08 §2.1 | `sys/kbdio.h`；`ttycom.h:174`；`input.rs:225` | 回答"调灯的控制号怎么算" |
| K-062 | 灯位三位 `KBD_LEDS_NUM/CAPS/SCROLL` | 常量 | 存 | 05 §2.6★、08 §1.2 | `sys/kbdio.h:16-19` | 回答"调用者那三位是什么" |
| K-063 | 五组编解码 helper 与"保留槽必须显式填 INVALID"的纪律 | 约束 | 存 | 05 §3.3★、05 §4.5 | `input.rs:238-380` | 回答"构造消息时容易漏什么" |
| K-064 | `Endpoint::TTY=5` / `VFS=1` 等常量 | 常量 | 存 | 99 §6★ | `minix-types/src/types/endpoint.rs:66,68` | 回答"终端/文件系统的端点号" |
| K-065 | `IS_CDEV_RQ` / `IS_CDEV_RS` 位掩码判定 | 机制 | 存 | 02 §2.3 | `com.h:919,920` | 回答"怎么认出这是字符设备信" |
| K-066 | **三面划分**：CDEV 面（同步请求/回信）/ input 面（全单向）/ DS 面（键与通知） | 架构 | **新** | 无（现混在 05 与 02） | `input.c:608-641` 三分支；`chardriver.c:494-529` | 回答"服务到底在跟几种对象说话" |

### 2.5 E 组 · 前台框架（K-070 ~ K-082）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-070 | 框架存在理由：十几个驱动共用同一个前台 | 概念 | 存 | 02 §1★ | `chardriver.c:1-44`；`input.c:31-39` | 回答"为什么要有 libchardriver" |
| K-071 | 通知优先且从不回信 | 约束 | 存 | 02 §2.4★ | `chardriver.c:463-482` | 回答"敲门和送信怎么区分" |
| K-072 | 块设备开门信代答 `ENXIO` | 机制 | 存 | 02 §2.4★ | `chardriver.c:438-450,488-492` | 回答"送错大厅怎么办" |
| K-073 | 重启门：`open_devs` 集合 + `is_open_dev`/`set_open_dev`/`clear_open_devs` | 机制 | 存 | 02 §1.2★、02 §2.4、02 §3.3 | `chardriver.c:54-94,494-514` | 回答"重起后旧请求为什么被丢" |
| K-074 | `MAX_NR_OPEN_DEVICES = 256` 与溢出 `panic` | 常量+约束 | 存（浅） | 02 §2.4 | `driver.h:41`；`chardriver.c:89-90` | 回答"能同时开多少个次设备号" |
| K-075 | 回信判决三形态：正常回信 / `EDONTREPLY` 挂起 / `ERESTART` 静默 | 机制 | 存 | 02 §1.3★、02 §2.5 | `chardriver.c:195-274` | 回答"什么时候不回信" |
| K-076 | `EDONTREPLY` 合法性检查（仅 READ/WRITE/IOCTL/CANCEL，其余 `panic`） | 约束 | 存 | 02 §2.5★ | `chardriver.c:203-222` | 回答"不回信是随便用的吗" |
| K-077 | `send_reply` 双通道：`SENDREC` → `ipc_sendnb`；否则 `asynsend3` | 机制 | 存 | 02 §2.5★ | `chardriver.c:177-190` | 回答"回信走哪条路" |
| K-078 | `chardriver_reply_task` 与 `CDEV_REPLY` | 接口 | 存 | 02 §2.5★、09 §2.4 | `chardriver.c:129-148` | 回答"挂起的请求怎么被回答" |
| K-079 | `chardriver_reply_select` 与 `CDEV_SEL2_REPLY` | 接口 | 存 | 02 §2.5★、08 §2.3 | `chardriver.c:153-172` | 回答"预约的查询怎么被通知" |
| K-080 | `chardriver_get_minor` 按消息类型提取次设备号 | 机制 | 存 | 02 §2.7★ | `chardriver.c:575-600` | 回答"次设备号从信封哪格拿" |
| K-081 | `minix-chardriver` 共享库单点权威（ARCH E-CDRCONV，2026-09-19 ✅ `bd06cab21`） | 架构演进 | 存 | 02 头部★ | `os/libs/minix-chardriver/src/driver.rs`；`edge_todo.md` E-CDRCONV | 回答"框架判定核在哪" |
| K-082 | 框架 11 处 `panic` 清单与 Rust 处置（命名不崩溃 / 保持 wire 沉默） | 约束+演进 | 存（散） | 02 §3.5 | `chardriver.c:90,112,116,120,138,162,215,221,226,270,565` | 回答"C 在哪里会崩" |

### 2.6 F 组 · 打开与关闭（K-090 ~ K-096）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-090 | 开门三问：有这个号吗 / 营业吗 / 已经开了吗 | 机制 | 存 | 06 §1.1★、06 §2.1 | `input.c:85-102` | 回答"打开被拒的三种理由" |
| K-091 | 两种失败共用 `ENXIO` 的故意设计 | 约束 | 存 | 06 §1.2★ | `input.c:91,94` | 回答"为什么不分两个错误号" |
| K-092 | 独占打开（`EBUSY`），一间房一位客人 | 约束 | 存 | 06 §1.3★ | `input.c:96-97` | 回答"能两个人同时读一块键盘吗" |
| K-093 | 关闭三打扫 + C 漏两间（`suspended`/`selector` 不清） | 机制 | 存 | 06 §1.4★、06 §2.2 | `input.c:107-125` | 回答"关闭到底打扫了什么" |
| K-094 | 关闭即清空队列（`tail=0; count=0`） | 机制 | 存 | 06 §2.2★ | `input.c:121-122` | 回答"关闭后旧事件还在吗" |
| K-095 | 定性为 bug 的证据链：VFS 关闭前不 cancel | 测试性质 | 存 | 06 §2.3★ | `minix3/minix/servers/vfs/filedes.c:close_filp` | 回答"为什么说 C 漏了" |
| K-096 | ARCH `close-cleanup`：Rust 多打扫两间，与 `cancel` 只清标志刻意不对称 | 架构演进 | 存 | 06 §3.2★、06 §3.3 | `handlers.rs:94`；`todo.md` IN-D3 | 回答"Rust 为什么改这里" |

### 2.7 G 组 · 读、挂起与环形缓冲（K-100 ~ K-113）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-100 | 读三岔路：给 / 等 / 不给 | 机制 | 存 | 07 §1.1★、07 §2.1 | `input.c:162-199` | 回答"读有几种结果" |
| K-101 | 胃口换算 `size / 20`，不足 1 个 → `EIO` | 约束 | 存 | 07 §1.5★ | `input.c:176-179` | 回答"缓冲区太小会怎样" |
| K-102 | 非阻塞空读 → `EAGAIN` | 约束 | 存 | 07 §2.1★ | `input.c:183-184` | 回答"不想等怎么表达" |
| K-103 | 挂起纸条四件套 + 返回 `EDONTREPLY` | 机制 | 存 | 07 §1.2★、07 §4.2 | `input.c:186-192` | 回答"等的时候服务记住什么" |
| K-104 | 挂起时不通知 selector（C 自认 "that's lame"） | 约束 | 存 | 07 §3.2★ | `input.c:191` 注释原文 | 回答"C 的一个已知缺憾" |
| K-105 | 已挂起再读 → `EIO`（单读者限制） | 约束 | 存 | 07 §2.1★ | `input.c:172-174` | 回答"能挂两个读者吗" |
| K-106 | 给多少取 `min(胃口, 存货)` | 机制 | 存 | 07 §1.3★ | `input.c:195-196` | 回答"一次读几个" |
| K-107 | 入队与溢出：满了丢弃最旧（`tail+1`, `count--`） | 机制 | 存 | 09 §1.6★、09 §2.3 | `input.c:338-346` | 回答"事件满了丢新的还是旧的" |
| K-108 | 出队两段拷贝几何（回绕拆两次 `sys_safecopyto`） | 机制 | 存 | 07 §1.4★、07 §2.2 | `input.c:130-157` | 回答"环形缓冲怎么搬出去" |
| K-109 | 拷贝失败不推进队列指针（ARCH IN-P1-3） | 架构演进 | 存 | 07 §4.4★ | `input.c:144-154`；`eventbuf.rs` | 回答"拷贝失败事件会不会丢" |
| K-110 | 存货不足 `panic` → 统一 `EIO`（A-11），所有构建行为一致 | 架构演进 | 存 | 07 §3.3★、99 §5 | `input.c:137-138` | 回答"C 崩溃处 Rust 怎么做" |
| K-111 | 授权与请求号原样透传（不解读） | 约束 | 存 | 07 §3.5★ | `input.c:187-189` | 回答"服务认识 grant 吗" |
| K-112 | 规划 / 提交分离：`plan_read_copy` → `commit_read_copy` | 架构演进 | 存 | 07 §3.1★、07 §4.4 | `eventbuf.rs` | 回答"顺序纪律怎么变成类型纪律" |
| K-113 | **环形缓冲是一件事：入队半与出队半应同篇** | 架构 | **新**（组织判断） | 入队在 09、出队在 07 | `input.c:338-355` 与 `:130-157` | 回答"同一个数据结构为什么要翻两篇" |

### 2.8 H 组 · 事件上报与路由（K-120 ~ K-130）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-120 | `id` 即数组下标（与 minor 不是一套） | 概念 | 存 | 09 §2.1★、03 §1.1 | `input.c:382-383` 注释原文 | 回答"上报里的 id 是什么号" |
| K-121 | id 越界静默丢弃 | 约束 | 存 | 09 §1.2★ | `input.c:384-385` | 回答"坏 id 会报错吗"（不会） |
| K-122 | 主人校验：`owner != m_source` 静默丢弃 | 约束 | 存 | 09 §1.2★、09 §2.1 | `input.c:388-390` | 回答"冒名上报怎么办" |
| K-123 | 三级跳板：具体设备 → 多路器 → 终端 | 机制 | 存 | 09 §1.3★、09 §2.2 | `input.c:404-421` | 回答"没人打开时事件去哪" |
| K-124 | 多路器选择：键盘窗口判定 vs 一律鼠标兜底 | 机制 | 存 | 09 §3.2★ | `input.c:393-397` | 回答"鼠标总机怎么选" |
| K-125 | 转交是复制粘贴不是翻译 | 约束 | 存 | 09 §1.4★、09 §4.4 | `input.c:409-418` | 回答"转交给终端的事件变了没有" |
| K-126 | 转交用阻塞 `ipc_send`（测生死），失败只记日志 | 机制 | 存 | 09 §2.2★ | `input.c:419-420` | 回答"终端死了会怎样" |
| K-127 | 入队时 `rsvd` 清零、`devid` 回填为槽位 id | 约束 | 存 | 09 §2.3★ | `input.c:348-354` | 回答"事件里的设备号谁填" |
| K-128 | 路由是纯函数（返回"去哪"而非"去做"） | 架构演进 | 存 | 09 §3.1★、09 §4.1 | `produce.rs` `route_event`/`multiplexer_for` | 回答"为什么路由可测" |
| K-129 | 窄化转换集中一处并加注释 | 约束 | 存 | 09 §3.5★ | `produce.rs` | 回答"u16/i32 转换在哪" |
| K-130 | 入队返回"是否溢出"而非溢出计数 | 架构演进 | 存 | 09 §3.3★ | `produce.rs` `enqueue` | 回答"溢出信息怎么用" |

### 2.9 I 组 · 挂起的三个出口（K-140 ~ K-150）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-140 | 唤醒顺序：挂起优先于选择者，二者互斥 | 机制 | 存 | 09 §1.5★、09 §2.4 | `input.c:361-370` | 回答"有货了先叫谁" |
| K-141 | 唤醒时恰好拷一个事件 | 机制 | 存 | 09 §2.4★ | `input.c:362` | 回答"唤醒给几个"（一个） |
| K-142 | 取消三重匹配（`suspended` + `caller` + `req_id`） | 机制 | 存 | 08 §1.3★、08 §2.2 | `input.c:290-291` | 回答"取消怎么认领" |
| K-143 | 匹配 → `EINTR` 回答**原读请求**；不匹配 → `EDONTREPLY` | 约束 | 存 | 08 §2.2★ | `input.c:292-297` | 回答"取消错了会怎样" |
| K-144 | 取消只清标志，不清纸条（与关闭修正刻意不对称） | 架构演进 | 存 | 06 §3.3★、08 §4.2 | `handlers.rs:266` | 回答"取消和关闭为什么不一样" |
| K-145 | 查询即问即答：三种"是" | 机制 | 存 | 08 §1.4★、08 §2.3 | `input.c:303-326` | 回答"select 什么时候报就绪" |
| K-146 | "就绪即报错"：不营业/已挂起也算读就绪（为了让调用者来读并拿到错误） | 概念 | 存 | 08 §1.4★、08 §3.4 | `input.c:314-316` | 回答"为什么没货反而说就绪" |
| K-147 | 查询预约：`CDEV_NOTIFY` 记下 selector | 机制 | 存 | 08 §1.5★、08 §3.3 | `input.c:319-320` | 回答"想被叫醒怎么留名" |
| K-148 | selector 单槽，后来者覆盖前者 | 约束 | 存 | 08 §2.3★ | `input.c:320,369` | 回答"能有几个人预约" |
| K-149 | 写操作永远"就绪"（`CDEV_OP_WR` 无条件置位） | 约束 | 存 | 08 §2.3★ | `input.c:323` | 回答"能往键盘写吗" |
| K-150 | 断开时以 `EIO` 唤醒挂起读者 + `CDEV_OP_RD` 通知选择者 | 机制 | 存 | 11 §1.4★、11 §2.3 | `input.c:540-550` | 回答"驱动走了等待的人怎么办" |

### 2.10 J 组 · 灯（K-160 ~ K-171）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-160 | 两套灯语言：`kio` 三位 ↔ `INPUT_LED_*` 位掩码 | 概念 | 存（分家） | 08 §1.2★ + 10 §1.1 | `sys/kbdio.h:16-19`；`input.c:262-268` | 回答"两个掩码是一回事吗" |
| K-161 | 位翻译函数 `led_mask_from_kio_bits` | 机制 | 存 | 08 §4.1★ | `handlers.rs` | 回答"翻译在哪发生" |
| K-162 | 广播目标规划：总机→四键盘槽；单键盘→一槽；鼠标→空集 | 机制 | 存 | 10 §1.1★、10 §4.1 | `input.c:221-235` | 回答"调灯发给谁" |
| K-163 | 鼠标蒸发无分支：循环范围即过滤器 | 约束 | 存 | 10 §3.4★ | `input.c:221`（只遍历键盘窗口） | 回答"为什么不用判断是不是鼠标" |
| K-164 | 先记住后发送，顺序即语义 | 约束 | 存 | 10 §1.4★、10 §2.1 | `input.c:227-231` | 回答"记和发谁先" |
| K-165 | 灯记忆存在槽位上，跨 owner 变更保持 | 数据结构 | 存 | 10 §1.2★、10 §4.2 | `input.h:42`；`input.c:228` | 回答"灯态存在哪" |
| K-166 | 新驱动连接即补发灯态 | 机制 | 存 | 10 §1.2★、11 §2.2 | `input.c:525-527` | 回答"换了驱动灯还亮吗" |
| K-167 | 发送失败只记日志：灯不值得崩溃 | 约束 | 存 | 10 §1.5★ | `input.c:231-234` | 回答"发不出去会怎样" |
| K-168 | 来源过滤：只有 TTY 能下发灯令，其余 fall-through 到意外日志 | 约束 | 存 | 10 §1.6★ | `input.c:630-639` | 回答"谁能调全系统的灯" |
| K-169 | 灯令走 `asynsend3(AMF_NOREPLY)` | 接口 | 存 | 10 §2.1★ | `input.c:231` | 回答"灯令要不要等回答" |
| K-170 | 目标规划返回定长数组，所有权装进目标（不二次查表） | 架构演进 | 存 | 10 §3.1★、10 §3.2 | `setleds.rs` | 回答"为什么不用 Vec" |
| K-171 | **灯是一个完整语义单元：翻译、广播、记忆、来源过滤、硬件回写应同篇** | 架构 | **新**（组织判断） | 现散在 05/08/10/13/14/99 | `input.c:204-236,256-276,630-635` | 回答"查灯要翻几篇"（现在 5 篇） |

### 2.11 K 组 · 驱动生命周期（K-180 ~ K-192）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-180 | 驱动宣告：`drv.inp.<label>` = 类型掩码 | 接口 | 存 | 12 §2.2★、11 §1.1 | `inputdriver.c:20-37` | 回答"驱动怎么报到" |
| K-181 | 布告栏间接性：服务看的是键的状态，不是驱动的状态（有延迟） | 概念 | 存 | 11 §1.1★ | `input.c:558-603` | 回答"驱动死了服务立刻知道吗"（不立刻） |
| K-182 | 到来检查：`ds_check` 循环 + 前缀过滤 | 机制 | 存 | 11 §1.6★、11 §2.4 | `input.c:570-585` | 回答"新驱动怎么被发现" |
| K-183 | 离去检查：`ds_retrieve_label_endpt` + `ESRCH` | 机制 | 存 | 11 §2.4★ | `input.c:588-602` | 回答"驱动走了怎么被发现" |
| K-184 | 分房三规则：认牌子 / 找空房 / 满房回 INVALID | 机制 | 存 | 11 §1.2★、11 §2.1 | `input.c:430-470` | 回答"槽位怎么分" |
| K-185 | 断开但仍开着的槽不可再分配 | 约束 | 存 | 11 §1.3★ | `input.c:451` | 回答"为什么留着空房不给" |
| K-186 | label 比对用 `strcmp` 零字节语义（不简化为字符串相等） | 约束 | 存 | 11 §3.5★ | `input.c:446,492` | 回答"标签怎么比" |
| K-187 | 配置回信保留槽固定 `INVALID_INPUT_ID` | 约束 | 存 | 11 §2.2★ | `input.c:519-520` | 回答"保留槽填什么" |
| K-188 | 分配失败仍回信（可能两个 ID 都无效 → 驱动自残式禁用） | 约束 | 存 | 11 §2.2★ | `input.c:500-512` 注释原文 | 回答"没房了为什么还发配置" |
| K-189 | 断开清理：唤醒 + 通知 + 撕牌子，**不动 `tail`/`count`** | 机制 | 存 | 11 §1.4★、11 §2.3 | `input.c:533-553` | 回答"退房打扫了什么"（不打扫队列） |
| K-190 | `typemask` 位（`INPUT_DEV_KBD=0x01` / `MOUSE=0x02`） | 常量 | 存 | 11 §2.2、99 §3 | `minix/input.h:9-10` | 回答"驱动怎么声明自己是键盘还是鼠标" |
| K-191 | Rust 两阶段驱动连接：`driver_connect` / `departure_candidates` + `driver_departed` | 架构演进 | **新** | 无 | `dispatcher.rs` 模块头注释；`serve.rs` | 回答"Rust 里 DS 通知怎么被展开" |
| K-192 | `key_is_new_driver` 纯字符串过滤（循环归传输层） | 架构演进 | 存 | 11 §3.4★、11 §4.1 | `connect.rs` | 回答"过滤和循环为什么分开" |

### 2.12 L 组 · 裁决、效应与传输（K-200 ~ K-215）—— **本轮最大新增组**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-200 | `Arrival` 五种到达分类（CDEV 请求 / DS 通知 / INPUT_EVENT / INPUT_SETLEDS / 其他） | 数据结构 | **新** | 无 | `dispatcher.rs` `Arrival` | 回答"一条消息进来先被分成几类" |
| K-201 | `handle_arrival`：一次到达 → 状态变化 + 效应清单 | 机制 | **新** | 无（01 §4.5 只提一句） | `dispatcher.rs` `handle_arrival` | 回答"裁决流水线长什么样" |
| K-202 | `Server { table, opened }` 收拢全局状态 | 数据结构 | **新** | 无（99 §7 提一句） | `dispatcher.rs` `Server` | 回答"服务的全部可变状态在哪" |
| K-203 | `Effect` 四种出口：ReplyTask / ReplySelect / SendDriverAsync / SendTerminalBlocking | 架构 | **新** | 无（02 §4.5 提一句） | `effects.rs` 模块头 + `Effect` | 回答"服务对外能做的全部动作" |
| K-204 | `ReplyValue` 单位诚实：字节数 vs 状态码分开 | 架构演进 | **新** | 无（todo IN-P2-1 提过） | `effects.rs` `ReplyValue` | 回答"读回信的单位是什么" |
| K-205 | 效应携带已构造好的 wire 消息，`m_source` 留给传输回填 | 约束 | **新** | 无 | `effects.rs` 模块头注释 | 回答"为什么效应里带完整消息" |
| K-206 | `GrantCopy` 两段式：规划 → `complete_grant_copy` 提交 | 机制 | **新** | 无 | `dispatcher.rs` `GrantCopy`/`complete_grant_copy` | 回答"拷贝失败怎么不推进" |
| K-207 | `Transport` trait 五动词：receive / send / asynsend / write_grant / publish_label | 接口 | **新** | 无 | `serve.rs:44-58` | 回答"传输层要提供什么" |
| K-208 | `KernelTransport` 双硬件腿聚合（IPC 腿 + SYSCALL 腿） | 架构 | **新** | 无 | `serve.rs:60-80` | 回答"一个 C 进程为什么有两条腿" |
| K-209 | minix-sef 切换点：`receive` 换成 `sef_receive_status` | 机制 | **新** | 无（仅 serve.rs 注释） | `serve.rs:9-11,63-64`；`os/libs/minix-sef/src/lib.rs:130` | 回答"SEF 接线后改哪里" |
| K-210 | `serve()` 主循环骨架 | 机制 | **新** | 无 | `serve.rs` `serve` | 回答"服务今天是怎么跑起来的" |
| K-211 | DS 客户端驱动两阶段（排空键 + 反查 label） | 机制 | **新** | 无 | `serve.rs`；`minix_sys::ds::DsClient` | 回答"DS 通知只是提示，真扫描在哪" |
| K-212 | `self_ep = Endpoint::NONE` 待 RS 分配（E5 真机联调） | 约束 | **新** | 无 | `main.rs` 末段 | 回答"为什么自己的端点还没定" |
| K-213 | `CDEV_REPLY=0x480` / `CDEV_SEL2_REPLY=0x482` 在 `serve.rs` 的落位与 `CDEV_REPLY_BASE` 历史错值 | 常量 | **新** | 99 §2 注释（只说错值） | `serve.rs:33-34`；`com.h:920,935-937` | 回答"回信号在哪，错值修了没" |
| K-214 | 异步发送槽 `AsyncSlot` / `AMF_NOREPLY` | 接口 | **新** | 无 | `serve.rs`；`minix_sys::ipc` | 回答"asynsend3 在 Rust 里叫什么" |
| K-215 | 判决纯函数化 ⇒ 91 个单元测试无需传输在场 | 架构 | 存（散于各篇 §5） | 各篇 §5★ | 全 crate；`lib.rs:9-14` | 回答"为什么这些测试能跑" |

### 2.13 M 组 · 驱动侧客户端库（K-220 ~ K-232）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-220 | 驱动四幕：宣告 / 上报 / 听命 / 循环 | 概念 | 存 | 12 §1.1★ | `inputdriver.c:20,42,82,119,188` | 回答"驱动作者要写什么" |
| K-221 | 三静态变量 → `DriverRegistration` 结构体（一进程一驱动的限制可见） | 架构演进 | 存 | 12 §2.1★、12 §3.1 | `inputdriver.c:11-15`；`inputdriver.rs:37` | 回答"库能不能服务两个驱动" |
| K-222 | 阻塞发送两理由：不限流 + 测生死 | 概念 | 存 | 12 §1.2★ | `inputdriver.c:65-73` 注释原文 | 回答"为什么不用异步" |
| K-223 | 发送失败复位 `input_endpt = NONE` | 机制 | 存 | 12 §2.3★ | `inputdriver.c:72-73` | 回答"服务死了驱动怎么做" |
| K-224 | 上报两道门（服务器已知？本类有槽位？） | 机制 | 存 | 12 §2.3★、12 §4.3 | `inputdriver.c:49-54` | 回答"什么时候上报被吞掉" |
| K-225 | 配置验明正身：反查 label `"input"` 比对来源 | 约束 | 存 | 12 §1.3★、12 §2.4 | `inputdriver.c:88-101` | 回答"伪造配置怎么办" |
| K-226 | 调灯只认已记录的 `input_endpt` | 约束 | 存 | 12 §1.3★、12 §2.5 | `inputdriver.c:124-129` | 回答"伪造灯令怎么办" |
| K-227 | 四钩子表（`idr_leds`/`intr`/`alarm`/`other`），无出生钩 | 接口 | 存 | 12 §1.4★、12 §2.7 | `inputdriver.h:9-14` | 回答"驱动要填几个回调" |
| K-228 | 槽位号库自动填，驱动不记 | 约束 | 存 | 12 §1.5★ | `inputdriver.c:52,59` | 回答"驱动要记住自己是几号吗" |
| K-229 | 通知三路分发（HARDWARE/CLOCK/其他） | 机制 | 存 | 12 §2.6★、12 §4.7 | `inputdriver.c:144-163` | 回答"中断通知走哪个钩子" |
| K-230 | 驱动不维护队列、不记灯（手脚 vs 脑袋） | 架构 | 存 | 12 §1.1★ | `inputdriver.c` 全文无缓冲 | 回答"驱动要不要缓存事件" |
| K-231 | 主循环与终止（`inputdriver_task` / `terminate` + `sef_cancel`） | 机制 | 存 | 12 §2.8★ | `inputdriver.c:177-206` | 回答"驱动怎么退出" |
| K-232 | 配置幂等（可多次收到） | 约束 | 存 | 12 §3.5★ | `inputdriver.c:76-81` 注释 | 回答"收到两次配置会出错吗" |

### 2.14 N 组 · 邻居契约（K-240 ~ K-250）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-240 | TTY 双向角色：吃事件 + 产灯 | 概念 | 存 | 13 §1.1★ | `tty.c:209-211`；`keyboard.c:369-384` | 回答"终端和输入服务什么关系" |
| K-241 | TTY 两道筛子：只要键盘页 + 只认识识的码 | 机制 | 存 | 13 §1.2★、13 §3.2 | `keyboard.c:148-176` | 回答"鼠标事件会被终端吃掉吗" |
| K-242 | TTY 握手应答：记下服务端点 + 回发当前灯态 | 机制 | 存 | 13 §1.3★、13 §2.2 | `keyboard.c:131-146` | 回答"收到 UP 之后终端做什么" |
| K-243 | TTY 私房位掩码卫生（内部位不出境） | 约束 | 存 | 13 §1.4★、13 §2.3、13 §3.4 | `keyboard.c:369-384` | 回答"终端的私有位会污染服务吗" |
| K-244 | 一对反义词：终端满缓冲丢新，服务满缓冲盖旧 | 概念 | 存 | 13 §1.5★、13 §3.3 | `keyboard.c` vs `input.c:338-346` | 回答"两边的溢出策略为什么不同" |
| K-245 | TTY 未知消息 `panic`（分发错误的哨兵） | 约束 | 存 | 13 §3.5★ | `keyboard.c:177` | 回答"终端收到别的消息会怎样" |
| K-246 | pckbd 三钩子表（无 `idr_other`） | 接口 | 存 | 14 §2.2★ | `pckbd.c:37-41` | 回答"pckbd 填了几个钩子" |
| K-247 | pckbd 键盘翻译：状态机 + 查表（`scan_keyboard`/`kbd_process`） | 机制 | 存 | 14 §2.3★ | `pckbd.c:111,328` | 回答"扫描码怎么变成事件码" |
| K-248 | pckbd 鼠标翻译：攒三字节包、比变、拆位移 | 机制 | 存 | 14 §2.4★ | `pckbd.c:374` | 回答"鼠标包怎么变成事件" |
| K-249 | pckbd 灯掩码 → 端口位回写 | 机制 | 存 | 14 §2.5★ | `pckbd.c:418` | 回答"灯最后怎么亮起来" |
| K-250 | 契约篇与实现篇的所有权划分（实现归 16-stage-drivers） | 约束 | 存 | 13/14 头部★ | `../16-stage-drivers/06-tty-driver.md`、`13-pckbd-driver.md` | 回答"这篇为什么不讲实现" |

### 2.15 O 组 · 失败、不变量与 ARCH（K-260 ~ K-274）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-260 | errno 全表：ENXIO/EINVAL/EBUSY/EAGAIN/EIO/EINTR/ENOTTY/EDONTREPLY | 常量 | 存 | 99 §5★、03/06/07/08 | `input.c` 各 return；`error.rs` | 回答"会返回哪些错误" |
| K-261 | `InputError` 十变体 + `to_errno` 单点映射 | 架构演进 | 存 | 99 §5★、03 §3.1 | `error.rs` | 回答"错误码在哪翻译" |
| K-262 | **C 侧 19 处 panic 全清单与 Rust 逐条处置** | 约束+演进 | **新**（现只有零散 4 条） | 03 §3.4、07 §3.3、02 §3.5、99 §5 | 见 §0.4 命令 5 输出 | 回答"C 一共崩几处，Rust 各怎么处理" |
| K-263 | A-7：反查失败返回 `None` 而非崩溃 | 架构演进 | 存 | 03 §3.4★ | `input.c:79`；`structs.rs:155` | 回答"第一处演进" |
| K-264 | A-11：存货不足返回 `EIO`，调试/发布一致 | 架构演进 | 存 | 07 §3.3★、99 §5 | `input.c:138` | 回答"第二处演进" |
| K-265 | `InvalidParking`：违规赊账命名不崩溃但保持 wire 沉默 | 架构演进 | 存 | 02 §3.5★ | `minix-chardriver/src/driver.rs` | 回答"第三处演进" |
| K-266 | 开门集合溢出显式错误而非崩溃 | 架构演进 | 存 | 02 §3.3★（浅） | `chardriver.c:89-90`；`OpenDeviceSet` | 回答"第四处演进" |
| K-267 | 不变量：`count <= EVENT_BUFFER_SIZE` | 约束 | **新**（散在 03/07/09） | 03 §1.4、09 §2.3 | `input.c:27-28` | 回答"队列长度上限由谁保证" |
| K-268 | 不变量：`suspended ⇒ opened`（挂起的读者必然开过门） | 约束 | **新** | 无 | `input.c:96,186`；`input.c:115-118` | 回答"没开门能挂起吗" |
| K-269 | 不变量：`leds` 跨 owner 变更保持，仅 `input_init` 清零 | 约束 | **新** | 10 §1.2（隐含） | `input.c:661,228,552-553` | 回答"灯记忆什么时候会丢" |
| K-270 | 不变量：`selector` 至多一个；`owner` 与 `opened` 独立 | 约束 | **新** | 08 §2.3、11 §1.3 | `input.c:320,451` | 回答"两个预约者会怎样" |
| K-271 | "失败即日志"清单：`asynsend` 失败、`ipc_send` 失败、`ds_retrieve` 失败 | 约束 | **新**（散） | 10 §1.5、09 §2.2、11 §2.4 | `input.c:232,420,573,600` | 回答"哪些失败不会传播" |
| K-272 | 服务永不退出（无 terminate 回调、无 `sef_cancel` 调用点） | 约束 | **新** | 无 | `input.c:685-691`；`chardriver.c:537-544` | 回答"关闭与退出路径存在吗" |
| K-273 | ARCH 三处一致纪律（doc + design + code 同标 `[ARCH: …]`） | 约束 | 存 | 06 §3.2、00 §3、02 头部 | `AGENTS.md` Key Constraints | 回答"改 C 行为要标几处" |
| K-274 | **错误路径总账**：每个 errno 的产生点、传播路径、调用者可见性 | 架构 | **新** | 99 §5（只有表，无路径） | `input.c` 全部 return + `chardriver_reply` | 回答"错误从哪来到哪去" |

### 2.16 P 组 · 测试基建（K-280 ~ K-287）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-280 | 判决即数据 ⇒ 测试无需传输在场 | 架构 | 存 | 各篇 §5★、00 §3 | 全 crate | 回答"为什么能测" |
| K-281 | 测试地图：模块 → 测试族（91 个 in-server + 7 in-sys + 21 in-types） | 工具 | **新** | 各篇 §5.1（计数已过时） | §0.4 命令 6 实测 | 回答"哪个模块有多少测试" |
| K-282 | 计数易腐：文档不写死总数，写"用命令查" | 约束 | **新** | 无（todo IN-D1 曾因过时而修） | `todo.md` IN-D1 | 回答"为什么这篇不再报数字" |
| K-283 | wire 断言：56 字节载荷锁（`message.rs:3899-3902`） | 测试 | 存 | 05 §5 | `os/libs/minix-types/src/ipc/message.rs` | 回答"线格式谁来保证" |
| K-284 | 节点表互锁测试（/dev 名字 ↔ minor） | 测试 | 存 | 05 §4.3★ | `input.rs` `INPUT_NODES` | 回答"节点表会不会离线" |
| K-285 | 键码表逐值锁测试 | 测试 | 存 | 04 §3.2★ | `key_codes.rs` | 回答"215 个码错了能发现吗" |
| K-286 | 边界族：回绕、槽位耗尽、selector 覆盖、单侧断连 | 测试 | 存 | todo IN-P3-2 | 各模块 | 回答"边界测了没" |
| K-287 | `cargo test -p minix-input` 等四条验证命令与基线 | 工具 | 存 | todo §0 验证基线 | `os/` workspace | 回答"怎么跑" |

### 2.17 统计摘要

- 总条数：**152**（K-001~K-274 实际编号 152 条）
- 来源类型分布：存量 **128**，新增 **24**
- 新增条目分布：启动与进程模型 2（K-006、K-016）、消息契约 1（K-066）、环形缓冲组织 1（K-113）、灯组织 1（K-171）、驱动生命周期 1（K-191）、**裁决/效应/传输 16（K-200~K-215）**、失败与不变量 5（K-262、K-267~K-272、K-274）、测试 2（K-281、K-282）
- 按类型分布：概念 14 / 机制 41 / 数据结构 16 / 接口与协议 30 / 约束与不变量 33 / 架构演进 24 / 工具与工程 6 / 测试性质 8
- **重复知识点（同一主题在多篇展开，需收主讲述点）**：
  - 灯：08 §1.2 + 10 + 05 §2.6 + 13 §2.3 + 14 §2.5 + 99 §3 → 主讲述点定为新 10
  - 环形缓冲：07（出队）+ 09 §2.3（入队）→ 主讲述点定为新 07
  - 挂起生命周期：07（建立）+ 08（取消/查询）+ 09 §2.4（唤醒）+ 11 §2.3（断开唤醒）→ 主讲述点定为新 09
  - 编号换算：03 + 99 §4 + 05 §2.5 → 主讲述点定为新 03
  - 全单向纪律：05 §1.2 + 12 §1.1 + 00 §3 → 主讲述点定为新 04
  - `input_revmap` panic：03 §2.5 + 99 §5 → 新 03（机制）与新 16（总账）分工

---

## 3. 覆盖审计

### 3.1 主题全集（四路来源）

1. **C 源码符号**：`input.c` 20 个函数全覆盖；`chardriver.c` 15 个函数；`inputdriver.c` 8 个函数；`input.h` / `minix/input.h` / `inputdriver.h` / `chardriver.h` / `sys/kbdio.h` 全部宏与结构体。
2. **OS 通用概念**：事件多路复用、环形缓冲与溢出策略、阻塞/非阻塞 I/O、挂起与唤醒、就绪通知（select）、字符设备模型与次设备号、独占打开、跨进程授权拷贝（grant）、服务重启与状态恢复、客户端库回调表。
3. **非 C 制品**：见 §0.3 表格（system.conf、MAKEDEV.sh、三个 Makefile、table.c 阴性证据）。
4. **阶段边界契约**：`00-master-plan/README.md:30`（RS 加载组）、`edge_todo.md` 四条（E-INWIRE / E-CDRCONV / E-TTYEVENT / E-PCKBDREG）。

### 3.2 覆盖缺口表

| # | 缺口主题 | 证据锚点 | 现状 | 建议 | 落地为 |
|---|---------|---------|------|------|--------|
| G-1 | 传输与主循环装配（E-INWIRE 已落地） | `serve.rs` 329 行；`main.rs:39` 已调 `serve()` | **零文档** | 新建篇章 | 新 12（K-200~K-214） |
| G-2 | 裁决流水线 `dispatcher.rs`（948 行，全 crate 最大） | `dispatcher.rs` | 只在 01 §4.5 提一句 | 新建篇章 | 新 12 |
| G-3 | 效应出口 `effects.rs`（344 行） | `effects.rs` | 只在 02 §4.5 提一句 | 新建篇章 | 新 12 |
| G-4 | SEF 生命周期与"只登记 fresh"的语义 | `input.c:688`；`sef.h:60`；`minix-sef/src/lib.rs:130` | 只在 01 §2.2 一句带过，未讲重起语义 | 并入启动篇 | 新 01（K-006） |
| G-5 | C 侧 19 处 panic 全清单 | 见 §0.4 命令 5 | 只有 4 处被提到 | 新建总账篇 | 新 16（K-262） |
| G-6 | 状态不变量（count 上限、suspended⇒opened、leds 持久、selector 唯一） | `input.c:27-28,96,186,320,451,661` | 无一处系统陈述 | 新建总账篇 | 新 16（K-267~K-270） |
| G-7 | 测试基建与测试地图（计数易腐问题） | todo IN-D1；实测 91+7+21 | 计数散在 15 篇且已过时 | 新建篇章 | 新 17（K-281~K-282） |
| G-8 | 三面消息划分（CDEV / input / DS） | `input.c:608-641` 三分支 | 无 | 并入消息契约篇 | 新 04（K-066） |
| G-9 | 回调表空缺的后果（无 intr/alarm/write） | `input.c:31-39` | 无 | 并入启动篇 | 新 01（K-016） |
| G-10 | `CDEV_WRITE` 得 `EIO`、select 的写永远就绪 | `chardriver.c:352-355`；`input.c:323` | 无 | 并入集合篇 | 新 06 / 新 09 |
| G-11 | 单线程执行模型（无锁、无 `Arc`/`Mutex`） | `lib.rs:9-14` | 只在代码注释 | 并入总览 | 新 00 |
| G-12 | `self_ep = NONE` 待 RS 分配（E5 联调） | `main.rs` 末段 | 无 | 并入传输篇 | 新 12（K-212） |
| G-13 | minix-sef 切换点 | `serve.rs:9-11,63-64` | 只在代码注释 | 并入传输篇 | 新 12（K-209） |
| G-14 | 构建与链接（三个 Makefile + Cargo.toml） | `servers/input/Makefile:5-6` | 无 | 并入总览 | 新 00 |
| G-15 | 灯的完整语义单元（翻译→广播→记忆→过滤→回写） | `input.c:204-236,256-276,630-635` | 散在 5 篇 | 合并为新 10 | 新 10（K-171） |
| G-16 | 环形缓冲的完整语义单元 | `input.c:130-157` + `:338-355` | 分在 2 篇 | 合并为新 07 | 新 07（K-113） |
| G-17 | 挂起的完整生命周期（建立→唤醒→取消→查询→断开） | `input.c:186,290,361,540` | 分在 4 篇 | 合并为新 09 | 新 09 |
| G-18 | pckbd 的 Rust 实现已存在（`os/drivers/hid/pckbd`，6 文件 17 测试） | `os/drivers/hid/pckbd/src/` | 14 篇仍称"契约篇，实现见 16-stage" | 契约篇加一句回指 | 新 15 |

**判定为其它 stage / 明确不做**：

| # | 主题 | 判定 | 理由 |
|---|------|------|------|
| X-1 | DS 服务内部实现 | 归 `../07-stage-ds/` | 本 stage 只用 `ds.h` 使用面（input.c 只用 6 个 DS API） |
| X-2 | TTY 行规程、控制台、keymap 本地化 | 归 `../16-stage-drivers/06-tty-driver.md` | 13 篇已声明边界 |
| X-3 | pckbd 硬件端口与扫描码全表 | 归 `../16-stage-drivers/13-pckbd-driver.md`；登记 edge E-PCKBDREG | 14 篇已声明裁剪清单 |
| X-4 | VFS 侧如何发起 CDEV 请求 | 归 `../05-stage-vfs/` | 本 stage 只见请求到达后的处理 |
| X-5 | RS 如何 fork+exec 服务进程 | 归 `../03-stage-rs/` | 本 stage 只引 `system.conf:400-403` 作为"谁加载我"的证据 |

### 3.3 重复主题表

| # | 主题 | 现有展开处 | 新目录主讲述点 | 其余处理 |
|---|------|-----------|---------------|---------|
| R-1 | 灯位翻译与灯掩码 | 08 §1.2、08 §2.1、08 §4.1 | **新 10（灯）** | 08 改为"控制通道只做路由与翻译，翻译规则见 10" |
| R-2 | 灯控制号 `KIOCSLEDS` 构造 | 05 §2.6、08 §2.1 | 新 04（消息契约）留编号构造；新 10 留语义 | 05 只留编号；08 不复述构造 |
| R-3 | 环形缓冲入队与溢出 | 09 §1.6、09 §2.3 | **新 07（读、挂起与环形缓冲）** | 08 只引用"入队规则见 07" |
| R-4 | 唤醒挂起读者 | 09 §2.4、08 §1.3、11 §2.3 | **新 09（挂起的三个出口）** | 08（事件侧）不重复讲唤醒，只讲"入队后叫人，见 09" |
| R-5 | 编号换算 | 03 §2.4-2.5、99 §4、05 §2.5 | **新 03** | 99 只留值表；05 只留节点表（引用 03 的换算结论） |
| R-6 | 全单向纪律 | 05 §1.2、12 §1.1、00 §3 | **新 04** | 12 只引用；00 只在三条边界里一句带过 |
| R-7 | 设备节点表 | 05 §2.5、99 §4 | **新 04**（与 INPUT_MAJOR 一起） | 99 只留值 |
| R-8 | 测试计数 | 15 篇 §5.1 | **新 17** | 各篇 §5 不再报总数，只列本篇测试族名 |
| R-9 | `input_revmap` 的 panic 与演进 | 03 §3.4、99 §5 | 新 03（机制与演进理由） | 16 只在 panic 总账里列一行并回指 03 |

### 3.4 越界主题表

| # | 越界处 | 越界内容 | 正确归属 |
|---|--------|---------|---------|
| V-1 | 13-tty-consumer.md §2 | 逐行分析 `keyboard.c:124-176`（终端内部实现） | 实现归 `../16-stage-drivers/06-tty-driver.md`；本篇只留契约三件事 |
| V-2 | 14-pckbd-driver.md §2.1-2.6 | 逐行分析 `pckbd.c` 六个函数 | 实现归 `../16-stage-drivers/13-pckbd-driver.md`；本篇只留"库使用面 + 契约" |
| V-3 | 02-chardriver-framework.md §2 | 600 行 libchardriver 全量分析，其中块设备部分与 input 无关 | 保留 C 分析但明确标注"input 消费面"；块设备处理压缩为一句 + 回指 |
| V-4 | 99-input-global-concepts.md §5 | "C 侧还有一处内部崩溃"的机制说明 | 机制说明下放新 16；99 只留值表 |
| V-5 | 05-input-message-contract.md §2.6 | 灯位与灯控制号的语义解释 | 语义下放新 10；05 只留编号与位值 |
| V-6 | 01-input-init-main.md §2.3 | 回调表七槽逐项解释（含框架职责） | 回调表是框架篇（新 05）的概念；01 只留"登记了七件事" |
| V-7 | 09-input-event-processing.md §2.3 | 入队与溢出（属数据结构） | 下放新 07 |

### 3.5 非 C 主题逐项回答（固定清单）

| 主题 | 在本 stage 的形态 | 归属 | 理由 |
|------|-----------------|------|------|
| **链接与加载** | input 是普通用户态进程，无独立链接脚本 | 新 00（一句）+ `../14-stage-runtime/`（机制） | `servers/input/Makefile` 只有 10 行，链接 `libchardriver`+`libsys`；加载由 RS 完成（`system.conf:400-403`） |
| **镜像与内存布局** | 不在 boot image | 新 01（一句 + grep 命令证据） | `kernel/table.c` grep `input` 阴性；本 stage 无镜像布局内容 |
| **汇编入口与陷阱进入** | 无 | **不在本 stage** | input 是用户态 C/Rust 进程，入口是 `main`；陷阱进入属 `01-stage-kernel` |
| **启动装配** | RS 加载 + SEF 握手 + 四步 init | 新 01（主讲述点） | `system.conf:400-403`、`input.c:646-704` |
| **构建与工具链** | 三个 Makefile + `os/servers/input/Cargo.toml` | 新 00（构建面一节） | 只需说清"产出物是什么、依赖哪些库"，不展开构建系统 |
| **跨模块接口与线格式** | 事件格式、五个消息、CDEV 协议、DS 键约定 | 新 02（事件词汇）+ 新 04（三面消息契约） | 本 stage 的核心协议内容 |
| **错误路径** | 8 个 errno + 19 处 C panic + "失败即日志"清单 | 新 16（主讲述点） | 需要一个总账篇，否则错误语义散在 8 篇 |
| **关闭与退出** | 服务永不退出；驱动侧有 `inputdriver_terminate` | 新 01（服务侧）+ 新 13（驱动侧） | `input.c:685-691` 无 terminate 注册；`inputdriver.c:177-184` 有 |
| **并发与同步** | 单线程事件循环，无锁无原子 | 新 00（执行模型一节） | `lib.rs:9-14` 明写 `!Send`/无 `Arc`/`Mutex`；一句话即可，不单独立篇 |
| **测试基建** | 91 + 7 + 21 个测试，纯函数判决可测 | 新 17（主讲述点） | 各篇 §5 只列本篇测试族名，地图与命令集中到 17 |

---

## 4. 新目录

### 4.1 新篇章总表（19 篇）

| 新编号 | 标题（文件名） | 一句话定位 | 分组 |
|--------|--------------|-----------|------|
| 00 | `00-input-overview.md` | 输入服务在系统中的位置、三条边界、三条主线、本套文档的阅读路径与构建面 | A 地图 |
| 01 | `01-input-startup-and-sef.md` | 进程怎么诞生：RS 加载、SEF 生命周期、四步初始化、进入永不返回的主循环 | B 诞生 |
| 02 | `02-input-event-vocabulary.md` | 全系统统一的输入语言：20 字节事件格式与全部事件码 | C 词汇 |
| 03 | `03-input-device-table.md` | 服务记住什么：十个槽位、两套编号与换算、十三字段、三个判断宏 | C 词汇 |
| 04 | `04-input-message-contract.md` | 跟谁说话、说什么：CDEV 面 / input 单向面 / DS 面三面契约与节点表 | D 协议 |
| 05 | `05-chardriver-front-door.md` | 十几个驱动共用的前台：分类、重启门、回信纪律，以及它在 Rust 里的单点权威 | D 协议 |
| 06 | `06-input-open-close.md` | 打开三问与关闭打扫（含关闭遗留挂起读的修正） | E 请求面 |
| 07 | `07-input-read-park-ringbuffer.md` | 读的三岔路、挂起纸条，以及环形缓冲的完整几何（入队、溢出、出队、推进） | E 请求面 |
| 08 | `08-input-event-intake.md` | 一次事件怎么进来：上报校验、三级跳板路由、转交终端 | F 事件流 |
| 09 | `09-input-wake-cancel-select.md` | 挂起的三条出口：被事件唤醒、被取消、被查询（含预约与就绪即报错） | F 事件流 |
| 10 | `10-input-leds.md` | 一盏灯的完整旅程：位翻译、广播、先记后发、跨重连恢复、来源过滤 | G 反方向流 |
| 11 | `11-input-driver-lifecycle.md` | 驱动的一生：宣告、到来检查、槽位分配、配置回信、离去检测与清理 | G 反方向流 |
| 12 | `12-input-dispatch-effects-transport.md` | 服务自身怎么装配：一条消息的裁决流水线、四种效应出口、传输层与主循环 | H 合拢 |
| 13 | `13-libinputdriver-client.md` | 驱动作者视角：宣告、上报、听命、循环四幕与四钩子表 | I 邻居视角 |
| 14 | `14-input-tty-contract.md` | 终端侧契约：消费事件、握手应答、灯光回授 | I 邻居视角 |
| 15 | `15-input-pckbd-contract.md` | 键盘鼠标驱动契约：库使用面、翻译责任、灯回写 | I 邻居视角 |
| 16 | `16-input-failure-invariants-arch.md` | 失败与不变量总账：errno 全表、19 处 panic、四条不变量、ARCH 清单 | J 收口 |
| 17 | `17-input-test-infrastructure.md` | 测试基建：为什么判决可测、测试地图、运行命令、易腐内容的处理纪律 | J 收口 |
| 99 | `99-input-constants.md` | 常量总表（只查值，含义回指机制篇） | K 索引 |

### 4.2 阅读路径

**主线（服务本体，13 篇，按顺序读）**
`00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12`

- 00 给动机与坐标
- 01 诞生（启动链）
- 02/03 语言与记忆（两个词汇表）
- 04/05 协议与前台（消息怎么进来）
- 06/07/08/09 四条核心子链（开门、读、事件、叫人）
- 10/11 反方向的两条流（灯、驱动生命）
- 12 合拢（服务自己怎么跑）

**支线一 · 邻居视角（3 篇，按读者角色选）**
`13（驱动作者）→ 14（终端作者）→ 15（键盘鼠标驱动作者）`
前置：04 + 11（13）、04 + 08 + 10（14）、02 + 13（15）。

**支线二 · 收口（2 篇，写完主线再读）**
`16（失败与不变量）→ 17（测试）`

**可跳读**
- `99`：纯查表，任何时候跳进来都行。
- `14` / `15`：只对外围实现者必需；只读服务本体可跳过。
- `17`：只在要改测试或加测试时读。

### 4.3 并行体的组织（集合型部分）

- **统一框架篇**：`05`（前台三判决、回信纪律、门卫），所有请求共用。
- **按场景分组**：`06`（开门关门）/ `07`（读与缓冲）/ `09`（取消与查询）——三组都是"调用者发起、服务回答"的请求族。
- **代表成员讲透**：`07` 的读是请求族里最复杂的成员（三岔路 + 挂起 + 回绕拷贝），讲透；`06` 与 `09` 的差异表收束（三问 vs 三重匹配 vs 三种"是"）。
- **主线/支线分离**：六个 CDEV 请求里，READ 是主线，IOCTL/CANCEL/SELECT 是支线——但 CANCEL/SELECT 因与"挂起的出口"同构，改挂到 `09`（事件流组），理由是它们的主语是"挂起"而不是"请求"。

### 4.4 序差表（教学序 ≠ 运行时序的地方）

| # | 运行时序事实（带锚点） | 教学序选择 | 理由 | 回指补偿位置 |
|---|---------------------|-----------|------|-------------|
| D-1 | `chardriver_announce()`（框架篇主题）发生在初始化第 3 步（`input.c:669`），早于任何请求 | 框架篇排在新 05，晚于启动篇（01） | 宣告的三件事里，门卫与回信纪律只有看到具体请求才有意义；启动篇只需知道"第 3 步做了宣告" | 01 §宣告步骤一句带过 + 回指 05；05 §宣告节回指 01 的调用点 |
| D-2 | `input_revmap`（换算，属 03）在启动清表时就被调用（`input.c:654`） | 换算放新 03，晚于 01 | 01 只需"给每槽填上门牌号"，号码体系的解释在 03 | 01 用 00 已建立的"门牌号/房间号"直觉词，不引用 03 的机制 |
| D-3 | `TTY_INPUT_UP` 编号（属 04）在启动第 4 步就被使用（`input.c:674`） | 编号放新 04，晚于 01 | 01 只需"向终端发一条'我起来了'" | 01 回指 04 的编号表；04 在编号表下注"启动第 4 步使用" |
| D-4 | `ds_subscribe("drv\\.inp\\..*")`（属 04/11）在启动第 2 步（`input.c:665`） | 订阅正则与键约定放新 04；扫描机制放 11 | 01 只需"向布告栏订阅输入驱动分区" | 01 回指 04（键与正则）与 11（扫描两阶段） |
| D-5 | `input_copy_events` 同时被读路径（`input.c:198`）与唤醒路径（`input.c:362`）调用 | 两处合到新 07（环形缓冲一篇） | 同一个数据结构的两半，分开讲会各讲一半几何 | 08/09 只引用"入队与出队见 07" |
| D-6 | `input_set_leds` 有三个调用点：`input_ioctl:270`、`input_other:632`、`input_connect:527` | 三个调用点全部收进新 10 | 灯是一个完整语义单元，按调用点分散会重复讲三遍广播规则 | 06/08/11 各自只说"转交置灯逻辑，见 10" |
| D-7 | `input_process` 的入队（`:338-355`）与唤醒（`:357-370`）是同一个函数的前后两半 | 入队归 07，唤醒归 09（分成两篇） | 入队属于数据结构（谁都能读），唤醒属于"挂起的出口"（要先有挂起与事件两个前提） | 07 在入队节末注"入队之后叫人，见 09"；09 开头回指 07 的入队规则 |
| D-8 | Rust 的 `serve()` 主循环是最先执行的代码路径（`main.rs:39`） | 传输与装配排在新 12（最后一篇机制篇） | 装配篇要引用全部裁决函数，放在前面会前向引用 06-11 | 01 在"进入主循环"处明确回指 12；12 开头回指 01 的入口 |

---

## 5. 每篇契约

### 00-input-overview

- **一句话定位**：让读者在十分钟内知道输入服务是什么、不是什么、本套文档按什么顺序读。
- **讲什么**：K-001、K-002、K-003、K-004、K-011（一句）、K-015、K-014（一句）、构建面（非 C 主题）。
- **不讲什么**：
  - 任何机制的执行细节 → 下放 01-12；
  - 事件格式 → 02；槽位结构 → 03；消息编号 → 04；框架判定 → 05；
  - 终端与 pckbd 的实现 → 14/15 只留契约，实现归 `../16-stage-drivers/`。
- **前置**：无（唯一前置是"Minix3 是微内核，进程之间靠消息说话"这一句外部常识）。
- **后置**：全部 18 篇。
- **事实底线**：
  - C：`input.c:1`（文件头注释）、`input.c:376-422`（事件分发）、`input.c:608-641`（消息面）；
  - 非 C：`system.conf:400-403`、`servers/input/Makefile`、`os/servers/input/Cargo.toml`、`os/libs/minix-sys/src/lib.rs:33`；
  - Rust：`os/servers/input/src/lib.rs:9-14`（单线程模型）、`lib.rs:19-33`（模块地图）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-001 | 事件汇/传达室 | 概念 | `input.c:1` | 全 stage 的动机 | 存：00 §1.1 |
| K-002 | 三条边界 | 约束 | `input.c:2-8` | 决定后面每篇的裁剪 | 存：00 §1.2 |
| K-003 | RS 运行时加载 | 机制 | `system.conf:400-403` | 读者要知道它不是 boot 服务 | 存：00 §1.3、01 §1.1 |
| K-004 | IPC 授权面 | 接口 | `system.conf:402` | 一张表说清邻居 | 存：99 §7 |
| K-015 | 单线程执行模型 | 架构 | `lib.rs:9-14` | 决定 Rust 侧类型选择 | **新**：`lib.rs:9-14` |
| K-016a | 三个 Makefile + Cargo.toml | 工具 | `servers/input/Makefile:5-6` | 构建面 | **新**：三个 Makefile |

- **验收标准**：读者读完能回答"输入服务碰不碰硬件""谁加载它""为什么 Rust 侧没有 `Mutex`""这套文档我先读哪篇"；篇内必须有一张完整的 19 篇导航表与三条阅读路径。

### 01-input-startup-and-sef

- **一句话定位**：讲清输入服务进程从被 RS 拉起，到把自己交给永不返回的主循环之间发生了什么。
- **讲什么**：K-003（证据命令）、K-005、K-006、K-007、K-008、K-009、K-010、K-011（一句）、K-012、K-013、K-014、K-016。
- **不讲什么**：
  - 设备表每个字段 → 03；
  - 框架判定核（门卫/回信纪律）→ 05；
  - 订阅之后怎么扫描 → 11；
  - 终端收到握手后做什么 → 14；
  - 消息编号的具体数值 → 04。
- **前置**：00。
- **后置**：04（订阅与握手在编号表里的落位）、05（宣告的框架侧）、11（驱动到来检查）、12（主循环的装配）、14（握手接收方）。
- **事实底线**：
  - C：`input.c:696-704`（main）、`:685-691`（input_startup）、`:646-680`（input_init）、`:31-39`（input_tab 七槽）、`:665-666`（订阅 + panic）；
  - 非 C：`system.conf:400-403`；`minix3/minix/kernel/table.c` grep 阴性；`sef.h:9,60`；
  - Rust：`init.rs`（`InitStep`、`StartupRegistration`、`HandlerSlot`）、`main.rs:28-40`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-005 | SEF 启动框架 | 机制 | `input.c:685-691` | 启动握手的执行者 | 存：01 §2.2 |
| K-006 | 只登记 fresh | 约束 | `input.c:688`（无 lu/restart 注册） | **重起语义的唯一权威** | **新**：`input.c:688` |
| K-007 | 四步初始化 | 机制 | `input.c:646-680` | 本篇主干 | 存：01 §1.4、§2.4 |
| K-008 | 清表循环 | 机制 | `input.c:652-662` | 第 1 步 | 存：01 §2.4、03 §4.5 |
| K-009 | DS 订阅 | 约束 | `input.c:665-666` | 第 2 步 + panic 策略 | 存：01 §2.4 |
| K-010 | `DSF_INITIAL` | 接口 | `input.c:665` | 解释"存量驱动不漏" | 存：99 §6 |
| K-013 | TTY 握手 | 接口 | `input.c:671-677` | 第 4 步 | 存：01 §2.4 |
| K-014 | 主循环永不返回 | 约束 | `input.c:701` | 收尾 | 存：01 §1.3 |
| K-016 | 回调表空缺后果 | 约束 | `input.c:31-39` | 解释"input 不收中断" | **新**：`input.c:31-39` |

- **验收标准**：读者能按序列出四步并说出每步失败会怎样（崩溃 / 崩溃 / 崩溃 / 只记日志）；能回答"驱动重起时设备表会不会被清空"（不会，因为只登记 fresh）；篇内必须给出 `rg -i input minix3/minix/kernel/table.c` 无输出的证据。

### 02-input-event-vocabulary

- **一句话定位**：讲清全系统统一的输入语言——一次按键或一次鼠标移动，用二十个字节怎么描述。
- **讲什么**：K-020、K-021、K-022、K-023、K-024、K-025、K-026、K-027、K-028、K-029、K-030。
- **不讲什么**：
  - 事件在消息里的封装 → 04；
  - 事件进队列出队列的规则 → 07；
  - 扫描码到事件码的翻译 → 15（驱动侧）；
  - 事件码到字符的本地化映射 → 14（终端侧）；
  - 灯掩码（区别于灯"码"）→ 10。
- **前置**：00。
- **后置**：04（载荷字段）、07（队列元素类型）、08（上报字段）、15（翻译目标）、14（消费词汇）。
- **事实底线**：
  - C：`minix3/minix/include/minix/input.h` 全文 333 行（`:6-15` 系统段、`:17-22` 责任注释、`:25-32` 结构、`:34-39` 页、`:41-47` 值与标志、`:50-57` GD 码、`:59-290` 键码、`:292-331` 其它码）；
  - Rust：`minix-types/src/ipc/input_event.rs`、`key_codes.rs`；`input.c:348-354`（rsvd 清零）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-020 | 20 字节六字段 | 数据结构 | `input.h:25-32` | 本篇主体 | 存：04 §1.2 |
| K-021 | 五个事件页 | 概念 | `input.h:34-39` | 码的组织方式 | 存：04 §1.3 |
| K-022 | 值与标志 | 数据结构 | `input.h:41-47` | 按下/松开、绝对/相对 | 存：04 §2.3 |
| K-024 | 键盘码 215 个 | 数据结构 | `input.h:59-290` | 最大的一张表 | 存：04 §2.5 |
| K-026 | 系统段三常量 | 常量 | `input.h:6-15` | 只有系统可见 | 存：04 §2.6 |
| K-027 | 三层责任 | 架构 | `input.h:17-22` | 决定后面两篇的裁剪 | 存：04 §1.1 |
| K-028 | rsvd 清零 | 约束 | `input.c:353-354` | 与入队耦合 | 存：04 §3.4 |
| K-030 | crate 归属演进 | 架构演进 | `input_event.rs:14` | 单一权威 | 存：04 头部 |

- **验收标准**：读者能画出 20 字节的字节布局图；能说出"0x2C 是空格"这句话属于哪一层（读者的本地化层，不是本篇）；篇内必须说明 215 个码是机械生成且每个值都有测试锁。

### 03-input-device-table

- **一句话定位**：讲清输入服务记住的全部状态——十个槽位、两套编号、十三字段、三个判断宏。
- **讲什么**：K-031、K-032、K-033、K-034、K-035、K-036、K-037、K-038、K-039、K-040、K-041、K-042（节点表只留值，语义归 04）。
- **不讲什么**：
  - 事件结构体字段含义 → 02；
  - 打开/读/选择如何使用这些字段 → 06/07/09；
  - owner 的变更 → 11；
  - 灯字段的语义 → 10。
- **前置**：00。
- **后置**：04（编号与节点表）、06（活跃判断）、07（队列与判断宏）、08（槽位寻址）、11（owner 与 label）。
- **事实底线**：
  - C：`input.h:6-26`（常量）、`input.h:29-43`（结构）、`input.c:22`（静态数组）、`input.c:24-28`（三宏）、`input.c:44-62`（正查）、`input.c:67-80`（反查 + panic）；
  - Rust：`structs.rs:27-72`（常量）、`:80-160`（类型与换算）、`:196-330`（结构与表）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-031 | 十槽布局 | 数据结构 | `input.h:18-26` | 本篇主体 | 存：03 §1.2 |
| K-032 | 两套编号 | 概念 | `input.c:47-51` 注释 | 全 stage 最容易混的点 | 存：03 §1.1 |
| K-033 | 正查 | 机制 | `input.c:44-62` | 请求面第一步 | 存：03 §2.4 |
| K-034 | 反查 + A-7 | 机制+演进 | `input.c:67-80` | C 崩溃第一处 | 存：03 §2.5、§3.4 |
| K-035 | 十三字段 | 数据结构 | `input.h:29-43` | 服务记住的全部 | 存：03 §1.3 |
| K-036 | 三判断宏 | 机制 | `input.c:24-28` | 被 06/07/08 反复用 | 存：03 §1.4 |
| K-037 | 两个新类型 | 架构演进 | `structs.rs:80,103` | 类型纪律的代表 | 存：03 §3.1 |
| K-041 | 三个容量常量 | 常量 | `structs.rs:27,35,42` | 后面所有边界的基础 | 存：99 §4 |

- **验收标准**：读者能画出十槽布局图并写出 `minor=2 → index=3` 的换算；能说出"为什么 5 到 63 号空着"；能说出反查失败在 C 里崩、在 Rust 里返回错误的演进编号（A-7）。

### 04-input-message-contract

- **一句话定位**：讲清输入服务跟三类对象（文件系统、驱动、数据存储）各说什么语言，以及 `/dev` 下的名字怎么对应到号码。
- **讲什么**：K-042、K-050、K-051、K-052、K-053、K-054、K-055、K-056、K-057、K-058、K-059、K-060、K-061、K-062、K-063、K-064、K-065、**K-066**。
- **不讲什么**：
  - 框架如何分类与回信 → 05；
  - 各处理函数收到后做什么 → 06-11；
  - 灯掩码的语义与广播 → 10（本篇只留灯位值与控制号构造）；
  - 终端收到后如何消费 → 14；
  - 驱动侧如何组装 → 13。
- **前置**：00、02、03。
- **后置**：05（编号的使用）、06-09（请求面编号）、10（灯令编号）、11（CONF 与 DS 键）、13（编解码 helper）、14（TTY 两个编号）。
- **事实底线**：
  - C：`com.h:872-893`（input 五编号 + "no real replies" 注释）、`com.h:915-937`（CDEV 编号）、`ipc.h:232-258`（三个发出载荷）、`ipc.h:993-1001`（上报载荷）、`ipc.h:2434-2436,2517`（联合体接入）、`dmap.h:78`、`MAKEDEV.sh:330-343`、`sys/kbdio.h:16-19`、`sys/sys/ttycom.h:174`；
  - Rust：`minix-types/src/ipc/input.rs:37-65`（编号）、`:82-115`（节点表）、`:164-225`（类型掩码、灯位、KIOCSLEDS）、`:238-380`（五组 helper）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-050 | CDEV 编号与 m10 布局 | 接口 | `com.h:915-937` | 请求面的字典 | 存：02 §2.1-2.2 |
| K-051 | input 五编号 | 接口 | `com.h:872-893` | 驱动面的字典 | 存：05 §2.1 |
| K-052 | 全单向纪律 | 约束 | `com.h:886` 注释 | 协议性格 | 存：05 §1.2 |
| K-053~056 | 四个载荷 | 接口 | `ipc.h:232-258,993-1001` | 本篇主体 | 存：05 §2.2-2.3 |
| K-059/060 | DS 两个键前缀 | 接口 | `inputdriver.c:23`；`chardriver.c:105` | DS 面 | 存：11 §1.1、02 §2.8 |
| K-062 | 灯位三位 | 常量 | `sys/kbdio.h:16-19` | 值的权威 | 存：05 §2.6 |
| K-066 | **三面划分** | 架构 | `input.c:608-641` 三分支 | 让读者一次看清三类对话 | **新**：`input.c:608-641` |

- **验收标准**：读者能画出"驱动 / 服务 / 终端 / 文件系统 / 数据存储"五方消息图并标出五个编号与方向；能回答"驱动上报要不要等回答"（不要，附 `com.h:886` 原文）；能说出 `INPUT_CONF` 的两个保留槽填什么。

### 05-chardriver-front-door

- **一句话定位**：讲清消息从到达进程到被 input 的处理函数看到之间，前台做了哪三个判断，以及这个前台在 Rust 里的单点权威在哪。
- **讲什么**：K-070、K-071、K-072、K-073、K-074、K-075、K-076、K-077、K-078、K-079、K-080、K-081、K-082。
- **不讲什么**：
  - input 七个处理函数的业务逻辑 → 06-09；
  - 块设备协议的其它请求 → 压缩为一句（BDEV_OPEN 代答 ENXIO）+ 回指；
  - 消息在线路上的传输 → 12；
  - input 侧的具体判决 → 06-09。
- **前置**：00、03、04。
- **后置**：06、07、09（三个判决的直接消费者）、12（循环里串起三个判决）。
- **事实底线**：
  - C：`lib/libchardriver/chardriver.c` 全文（`:1-44` 头注释、`:54-94` 门集合、`:99-124` announce、`:129-172` 两个 reply helper、`:177-190` send_reply、`:195-274` 回信判决、`:279-433` 六个搬运、`:438-450` 块设备代答、`:455-532` 分发、`:549-570` 主循环、`:575-600` 取 minor）；`driver.h:41`（`MAX_NR_OPEN_DEVICES`）；
  - Rust：`os/libs/minix-chardriver/src/driver.rs`、`protocol.rs`；`edge_todo.md` E-CDRCONV（2026-09-19 ✅ `bd06cab21`）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-070 | 共用前台的理由 | 概念 | `chardriver.c:1-44` | 动机 | 存：02 §1 |
| K-071 | 通知优先不回信 | 约束 | `chardriver.c:463-482` | 判决一 | 存：02 §2.4 |
| K-073 | 重启门 | 机制 | `chardriver.c:54-94,494-514` | 判决二 | 存：02 §1.2、§3.3 |
| K-075 | 回信三形态 | 机制 | `chardriver.c:195-274` | 判决三 | 存：02 §1.3、§2.5 |
| K-076 | EDONTREPLY 合法性 | 约束 | `chardriver.c:203-222` | 挂起基石的边界 | 存：02 §2.5 |
| K-078/079 | 两个 reply helper | 接口 | `chardriver.c:129-172` | 被 09/11 调用 | 存：02 §2.5 |
| K-081 | 共享库单点权威 | 架构演进 | `minix-chardriver/src/driver.rs` | Rust 侧落位 | 存：02 头部 |
| K-082 | 11 处 panic | 约束 | `chardriver.c:90...565` | 与 16 的总账对照 | 存：02 §3.5 |

- **验收标准**：读者能按序列出前台三判断，并说出每判断的失败形态（丢弃 / 代答 / 崩溃）；能说出"重起后旧请求为什么被丢"（门集合被清空）；能说出 Rust 侧判定核在哪个 crate 的哪个文件。

### 06-input-open-close

- **一句话定位**：讲清打开一个输入设备时服务问的三个问题，以及关闭时打扫了哪几间房、漏了哪几间。
- **讲什么**：K-090、K-091、K-092、K-093、K-094、K-095、K-096；外加 K-010 的另一半（写请求得 `EIO`，来自 `chardriver.c:352-355`）。
- **不讲什么**：
  - 读与挂起 → 07；
  - 控制、取消、查询 → 09；
  - 次设备号换算细节 → 03（只引用结论）；
  - 关闭时唤醒挂起读者的具体动作 → 09（只说"交给 09 的出口"）。
- **前置**：00、03、05。
- **后置**：07（开门之后才谈读）、09（关闭修正与取消的不对称）。
- **事实底线**：
  - C：`input.c:85-102`（open）、`:107-125`（close）、`:24-26`（active 宏）；`minix3/minix/servers/vfs/filedes.c:close_filp`（关闭前不 cancel 的证据）；`chardriver.c:352-355`（无 write 钩子 → EIO）；
  - Rust：`handlers.rs:42-55`（decide/apply_open）、`:94`（apply_close）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-090 | 开门三问 | 机制 | `input.c:85-102` | 本篇主体 | 存：06 §1.1、§2.1 |
| K-091 | 共用 ENXIO | 约束 | `input.c:91,94` | 有意的信息隐藏 | 存：06 §1.2 |
| K-092 | 独占打开 | 约束 | `input.c:96-97` | 与 07 的单读者呼应 | 存：06 §1.3 |
| K-093 | 关闭漏两间 | 机制 | `input.c:107-125` | C 的缺陷 | 存：06 §1.4、§2.2 |
| K-095 | bug 证据链 | 测试性质 | `filedes.c:close_filp` | 证明"漏了"不是猜测 | 存：06 §2.3 |
| K-096 | ARCH close-cleanup | 架构演进 | `handlers.rs:94` | 全 stage 第一处修正性偏离 | 存：06 §3.2 |

- **验收标准**：读者能背出三问的顺序与每个答案的错误码；能解释"为什么查无此设备和门后没人用同一个错误号"；能说出 Rust 多打扫了哪两间、以及为什么与 cancel 的处理刻意不对称。

### 07-input-read-park-ringbuffer

- **一句话定位**：讲清一次读请求的三种下场，以及事件队列这个环形缓冲的完整几何（怎么进、满了怎么办、怎么出、失败怎么不推进）。
- **讲什么**：K-100、K-101、K-102、K-103、K-104、K-105、K-106、**K-107（入队与溢出，从旧 09 移入）**、K-108、K-109、K-110、K-111、K-112、**K-113**。
- **不讲什么**：
  - 谁来唤醒挂起的读者 → 09；
  - 谁来取消 → 09；
  - 事件上报的校验与路由 → 08；
  - `sys_safecopyto` 的传输实现 → 12（本篇只定几何契约）。
- **前置**：00、02、03、05、06。
- **后置**：08（入队之后叫人）、09（唤醒与取消的对象）、12（拷贝的传输执行）。
- **事实底线**：
  - C：`input.c:162-199`（read）、`:130-157`（copy）、`:338-355`（入队与溢出）、`:27-28`（空/满宏）；
  - Rust：`handlers.rs:113-185`（ReadVerdict / decide_read / park_read）、`eventbuf.rs` 全文（CopyPlan / ReadCopyPlan / plan_copy / plan_read_copy / commit_read_copy / apply_copy）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-100 | 读三岔路 | 机制 | `input.c:162-199` | 本篇主体 | 存：07 §1.1、§2.1 |
| K-103 | 挂起纸条 | 机制 | `input.c:186-192` | 09 的前提 | 存：07 §1.2、§4.2 |
| K-107 | 入队与溢出 | 机制 | `input.c:338-346` | **与出队同属一个数据结构** | 存：09 §1.6、§2.3（迁入） |
| K-108 | 两段拷贝几何 | 机制 | `input.c:130-157` | 最难的一段算术 | 存：07 §1.4、§2.2 |
| K-109 | 拷贝失败不推进 | 架构演进 | `input.c:144-154` | 顺序纪律的类型化 | 存：07 §4.4 |
| K-110 | 存货不足 A-11 | 架构演进 | `input.c:137-138` | C panic 第二处 | 存：07 §3.3 |
| K-113 | 环形缓冲是一件事 | 架构 | `input.c:130-157` + `:338-355` | 本篇合并的理由 | **新**：组织判断 |

- **验收标准**：读者能画出环形缓冲的 `tail`/`count`/回绕三段图，并手算一次跨回绕的两段拷贝字节数；能说出"满了丢新的还是旧的"（旧的）；能说出拷贝失败时 `tail`/`count` 动不动（不动）。

### 08-input-event-intake

- **一句话定位**：讲清驱动上报一个事件之后，服务怎么校验它、决定它落到哪个队列、以及在没人要时怎么转交给终端。
- **讲什么**：K-120、K-121、K-122、K-123、K-124、K-125、K-126、K-127、K-128、K-129、K-130。
- **不讲什么**：
  - 入队的几何与溢出 → 07（只引用）；
  - 入队之后叫醒谁 → 09；
  - 挂起的建立 → 07；
  - 终端收到后做什么 → 14；
  - 驱动侧怎么组装 → 13。
- **前置**：00、02、03、07。
- **后置**：09（叫人）、14（转交的接收方）、12（路由在裁决流水线里的位置）。
- **事实底线**：
  - C：`input.c:376-422`（input_event）、`:332-355`（input_process 前半）、`:393-397`（多路器选择）、`:409-420`（转交）；
  - Rust：`produce.rs`（`EventIntake` / `route_event` / `multiplexer_for` / `stored_event` / `enqueue` / `ForwardedEvent` / `forward_to_terminal`）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-120 | id 即下标 | 概念 | `input.c:382-383` 注释 | 与 minor 的区分 | 存：09 §2.1 |
| K-122 | 主人校验 | 约束 | `input.c:388-390` | 安全边界 | 存：09 §1.2 |
| K-123 | 三级跳板 | 机制 | `input.c:404-421` | 本篇主体 | 存：09 §1.3、§2.2 |
| K-125 | 转交不翻译 | 约束 | `input.c:409-418` | 与 14 的契约 | 存：09 §1.4 |
| K-126 | 阻塞发送测生死 | 机制 | `input.c:419-420` | 与 13 的对称 | 存：09 §2.2 |
| K-128 | 路由纯函数 | 架构演进 | `produce.rs` | 可测性的代表 | 存：09 §3.1 |

- **验收标准**：读者能画出三级跳板图并说出每一级的触发条件；能说出"坏 id 和冒名上报有没有日志"（没有）；能说出转交的事件与原始事件在字段上有没有差别（只少了 devid 之外的东西——实际是五字段原样复制）。

### 09-input-wake-cancel-select

- **一句话定位**：讲清一个挂起的读请求有哪三条出路——被事件叫醒、被取消打断、被查询通知。
- **讲什么**：K-140、K-141、K-142、K-143、K-144、K-145、K-146、K-147、K-148、K-149、K-150。
- **不讲什么**：
  - 挂起怎么建立 → 07（只引用）；
  - 事件怎么入队 → 07/08（只引用）；
  - 事件队列的几何 → 07；
  - 控制请求（ioctl）→ 10（本篇只保留"控制是另一条通道"的一句）；
  - 传输怎么发回信 → 12。
- **前置**：00、03、05、07、08。
- **后置**：10（断开时的清理引用本篇的唤醒）、11（断开唤醒）、12（效应出口）。
- **事实底线**：
  - C：`input.c:357-370`（唤醒）、`:282-298`（取消）、`:303-326`（查询）、`:540-550`（断开时的唤醒与通知）；`chardriver.c:129-172`（两个 reply helper）；
  - Rust：`produce.rs`（`WakeDirective` / `decide_wake` / `wake_on_event` / `apply_wake_*`）、`handlers.rs:240-380`（`CancelVerdict` / `apply_cancel` / `cancel_parked_read` / `SelectOutcome` / `decide_select` / `apply_select_record`）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-140 | 挂起优先于选择者 | 机制 | `input.c:361-370` | 出口一的规则 | 存：09 §1.5、§2.4（迁入） |
| K-141 | 恰好拷一个 | 机制 | `input.c:362` | 最容易错记的细节 | 存：09 §2.4（迁入） |
| K-142 | 取消三重匹配 | 机制 | `input.c:290-291` | 出口二 | 存：08 §1.3、§2.2 |
| K-145 | 查询三种"是" | 机制 | `input.c:303-326` | 出口三 | 存：08 §1.4、§2.3 |
| K-146 | 就绪即报错 | 概念 | `input.c:314-316` | 反直觉点 | 存：08 §1.4 |
| K-149 | 断开时的唤醒 | 机制 | `input.c:540-550` | 与 11 的交界 | 存：11 §1.4、§2.3（迁入） |

- **验收标准**：读者能说出三条出口各自的触发条件与返回的数值（字节数 / EINTR / 就绪掩码）；能解释"为什么没货反而报就绪"；能说出取消与关闭在清理范围上的刻意不对称及其理由。

### 10-input-leds

- **一句话定位**：讲清一盏键盘灯从调用者的三位、到服务的掩码、到广播目标、到跨重连恢复的完整旅程。
- **讲什么**：K-160、K-161、K-162、K-163、K-164、K-165、K-166、K-167、K-168、K-169、K-170、**K-171**。
- **不讲什么**：
  - 灯控制号的数值构造 → 04（只引用）；
  - 终端为什么有权调灯、终端怎么维护锁定位 → 14（本篇只给来源过滤的理由与锚点）；
  - 驱动收到掩码后怎么写端口 → 15（契约面）；
  - 灯令的传输执行 → 12。
- **前置**：00、02、03、04、08。
- **后置**：11（连接时补发灯态）、14（终端侧回授）、15（驱动侧回写）、12（异步发送效应）。
- **事实底线**：
  - C：`input.c:204-236`（input_set_leds）、`:256-276`（ioctl 翻译）、`:262-268`（位映射）、`:525-527`（连接补发）、`:630-635`（TTY 来源过滤）；`sys/kbdio.h:16-19`；
  - Rust：`handlers.rs:187-240`（`IoctlVerdict` / `decide_ioctl` / `led_mask_from_kio_bits`）、`setleds.rs` 全文（`LightTarget` / `plan_light_targets` / `apply_light_save` / `remembered_lights`）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-160 | 两套灯语言 | 概念 | `kbdio.h:16-19`；`input.c:262-268` | 从 08 迁入，合并为完整语义 | 存：08 §1.2 + 10 §1.1 |
| K-162 | 广播目标规划 | 机制 | `input.c:221-235` | 本篇主体 | 存：10 §1.1、§4.1 |
| K-164 | 先记后发 | 约束 | `input.c:227-231` | 顺序即语义 | 存：10 §1.4 |
| K-166 | 连接补发 | 机制 | `input.c:525-527` | 跨重连恢复的落点 | 存：10 §1.2、11 §2.2 |
| K-168 | 来源过滤 | 约束 | `input.c:630-639` | fall-through 语义 | 存：10 §1.6 |
| K-171 | 灯是一个语义单元 | 架构 | `input.c:204-236` 等 | 本篇合并的理由 | **新**：组织判断 |

- **验收标准**：读者能画出"调用者三位 → 服务掩码 → 目标集合 → 记住 + 发送"的四段图；能说出鼠标槽为什么收不到灯令（循环范围即过滤器，无分支）；能说出驱动换了一代之后灯怎么回来（连接时补发 `devs[kbd_id].leds`）；能说出非 TTY 来源的灯令去哪了（fall-through 到意外消息日志）。

### 11-input-driver-lifecycle

- **一句话定位**：讲清驱动从在布告栏贴条、到分到槽位、到收到配置、到条子消失被退房的完整一生。
- **讲什么**：K-180、K-181、K-182、K-183、K-184、K-185、K-186、K-187、K-188、K-189、K-190、**K-191**、K-192。
- **不讲什么**：
  - 驱动侧的宣告与组装代码 → 13（本篇只对照服务端读到的键）；
  - 数据存储服务内部 → `../07-stage-ds/`；
  - 事件上报 → 08；
  - 灯令发送 → 10（本篇只读灯记忆）。
- **前置**：00、03、04、10。
- **后置**：12（两阶段连接在传输层的驱动方式）、13（驱动的对照视角）。
- **事实底线**：
  - C：`input.c:430-470`（alloc_id）、`:475-528`（connect）、`:533-553`（disconnect）、`:558-603`（check）；`inputdriver.c:20-37`（宣告）；`minix/ds.h:25-69`（使用面）；
  - Rust：`connect.rs` 全文（`key_is_new_driver` / `alloc_id` / `labels_match` / `wants_from_typemask` / `ConnectReport` / `connect_driver` / `DisconnectEffects` / `disconnect_device`）、`dispatcher.rs` 的两阶段方法。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-181 | 布告栏间接性 | 概念 | `input.c:558-603` | 解释延迟与对账 | 存：11 §1.1 |
| K-182/183 | 到来半与离去半 | 机制 | `input.c:570-585` / `:588-602` | 本篇主体 | 存：11 §2.4 |
| K-184 | 分房三规则 | 机制 | `input.c:430-470` | 分配算法 | 存：11 §1.2、§2.1 |
| K-185 | 断开仍开着不给 | 约束 | `input.c:451` | 反直觉点 | 存：11 §1.3 |
| K-188 | 满房仍回信 | 约束 | `input.c:500-512` 注释 | C 的自残式设计 | 存：11 §2.2 |
| K-189 | 退房不动行李 | 机制 | `input.c:533-553` | 与关闭的对照 | 存：11 §1.4、§2.3 |
| K-191 | Rust 两阶段连接 | 架构演进 | `dispatcher.rs` | **新增** | **新**：`dispatcher.rs` 模块头 |

- **验收标准**：读者能画出"驱动 → 布告栏 → 服务"的三角图并说明服务从不与驱动直接谈生命周期；能列出分房三规则与满房时的行为；能说出 `ESRCH` 在这里的含义；能说出退房时队列清不清（不清）。

### 12-input-dispatch-effects-transport

- **一句话定位**：讲清服务自己是怎么装配起来的——一条消息进来之后走哪条裁决流水线、产生哪几种效应、由谁把这些效应真正做出来。
- **讲什么**：K-200、K-201、K-202、K-203、K-204、K-205、K-206、K-207、K-208、K-209、K-210、K-211、K-212、K-213、K-214、K-215。
- **不讲什么**：
  - 单个判决的业务语义（开门三问、路由规则等）→ 06-11（本篇只讲它们怎么被串起来）；
  - `minix-sys` 的 IPC 原语实现 → `../14-stage-runtime/`；
  - SEF 框架本身 → `01-stage-kernel` / `minix-sef` crate（本篇只标切换点）；
  - DS 服务内部 → `../07-stage-ds/`。
- **前置**：00、01、03、04、05、06、07、08、09、10、11。
- **后置**：13（驱动侧的对称结构）、16（效应失败的处理）、17（无传输测试的前提）。
- **事实底线**：
  - C：`chardriver.c:455-532`（分发主干）、`:549-570`（主循环）、`:127-174`（两个 reply helper）；`input.c:608-641`（input_other 三分支）；
  - Rust：`dispatcher.rs` 全文（`Server` / `Arrival` / `CdevCall` / `GrantCopy` / `Outcome` / `handle_arrival` / `complete_grant_copy`）、`effects.rs` 全文（`Effect` / `ReplyValue`）、`serve.rs` 全文（`Transport` / `KernelTransport` / `serve`）、`main.rs:28-40`；
  - edge：`edge_todo.md` E-INWIRE。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-200 | Arrival 五分类 | 数据结构 | `dispatcher.rs` | 流水线第一步 | **新** |
| K-201 | handle_arrival | 机制 | `dispatcher.rs` | 本篇主体 | **新** |
| K-202 | Server 状态收拢 | 数据结构 | `dispatcher.rs` | 全局状态的唯一位置 | **新** |
| K-203 | Effect 四种出口 | 架构 | `effects.rs` | 全部对外动作 | **新** |
| K-206 | GrantCopy 两段式 | 机制 | `dispatcher.rs` | 拷贝失败不推进的落点 | **新** |
| K-207 | Transport 五动词 | 接口 | `serve.rs:44-58` | 传输契约 | **新** |
| K-208 | 双硬件腿聚合 | 架构 | `serve.rs:60-80` | 一个进程两条腿 | **新** |
| K-209 | minix-sef 切换点 | 机制 | `serve.rs:9-11,63-64` | 未来接线位 | **新** |
| K-211 | DS 两阶段驱动 | 机制 | `serve.rs` | 通知只是提示 | **新** |
| K-213 | CDEV 回信号落地 | 常量 | `serve.rs:33-34`；`com.h:920` | 历史错值的正本 | **新** |

- **验收标准**：读者能画出"receive → 分类 → 门 → 裁决 → 效应清单 → 传输执行 → 拷贝提交"的完整流水线图；能说出四种效应各自的 C 对应函数与发送语义（阻塞/异步）；能说出 `Transport` 的五个动词各自对应 C 的哪个调用；能说出 SEF 接线后要改哪一行。

### 13-libinputdriver-client

- **一句话定位**：站在驱动作者的位置，讲清接入输入服务要做的四件事与四个钩子。
- **讲什么**：K-220、K-221、K-222、K-223、K-224、K-225、K-226、K-227、K-228、K-229、K-230、K-231、K-232。
- **不讲什么**：
  - 服务侧的连接处理 → 11（只对照，不重复）；
  - 具体驱动的硬件细节 → 15；
  - 传输实现（查名、发布、发送、收信）→ `../14-stage-runtime/`；
  - 数据存储服务内部 → `../07-stage-ds/`。
- **前置**：00、02、04、11。
- **后置**：15（pckbd 是本库的模型用户）。
- **事实底线**：
  - C：`lib/libinputdriver/inputdriver.c` 全文（`:11-15` 静态状态、`:20-37` announce、`:42-74` send_event、`:82-111` do_conf、`:119-135` do_setleds、`:141-172` process、`:177-206` task/terminate）；`minix/inputdriver.h` 全文；
  - Rust：`os/libs/minix-sys/src/inputdriver.rs` 全文（`DriverRegistration` / `announce_key` / `announce_type` / `ReportVerdict` / `decide_report` / `ConfOutcome` / `verify_conf_sender` / `SetledsOutcome` / `accept_setleds` / `DriverHooks` / `DriverIncoming` / `NotifyKind` / `classify_incoming`）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-220 | 驱动四幕 | 概念 | `inputdriver.c` 全文 | 本篇骨架 | 存：12 §1.1 |
| K-222 | 阻塞发送两理由 | 概念 | `inputdriver.c:65-73` 注释 | 反直觉点 | 存：12 §1.2 |
| K-223 | 失败复位 | 机制 | `inputdriver.c:72-73` | 生死检测 | 存：12 §2.3 |
| K-225 | 配置验明正身 | 约束 | `inputdriver.c:88-101` | 与服务端 label 校验对称 | 存：12 §2.4 |
| K-227 | 四钩子表 | 接口 | `inputdriver.h:9-14` | 驱动作者要填的 | 存：12 §2.7 |
| K-230 | 手脚 vs 脑袋 | 架构 | 全文无缓冲 | 责任边界 | 存：12 §1.1 |

- **验收标准**：读者能列出驱动四幕与每幕调用的库函数；能说出阻塞发送的两个理由（附 `inputdriver.c:65-73` 注释原文）；能说出驱动要不要缓存事件与灯态（不要）。

### 14-input-tty-contract

- **一句话定位**：站在终端作者的位置，讲清它消费转交事件、应答握手、回授灯光的三条契约。
- **讲什么**：K-240、K-241、K-242、K-243、K-244、K-245。
- **不讲什么**：
  - 终端内部的键盘映射、控制台、行规程 → `../16-stage-drivers/06-tty-driver.md`（本篇明确标注所有权）；
  - 服务侧的转交条件 → 08（只引用）；
  - 服务侧的灯广播 → 10（只引用）；
  - 驱动侧 → 13、15。
- **前置**：00、02、04、08、10。
- **后置**：无（本 stage 末端的契约篇）。
- **事实底线**：
  - C：`tty.c:205-214`（消息接入）、`keyboard.c:30-61`（缓冲与位定义）、`:124-176`（do_input）、`:369-384`（set_leds）、`:177`（未知类型 panic）；`minix/keymap.h:NR_SCAN_CODES`；
  - Rust：无（本篇是契约篇，所有权在 16-stage-drivers；需加一句回指 `os/drivers/tty/`）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-240 | TTY 双向角色 | 概念 | `tty.c:209-211`；`keyboard.c:369` | 本篇动机 | 存：13 §1.1 |
| K-241 | 两道筛子 | 机制 | `keyboard.c:148-176` | 消费契约 | 存：13 §1.2、§3.2 |
| K-242 | 握手应答 | 机制 | `keyboard.c:131-146` | 与 01 的对称 | 存：13 §1.3、§2.2 |
| K-243 | 私房位卫生 | 约束 | `keyboard.c:369-384` | 掩码边界 | 存：13 §1.4、§3.4 |
| K-244 | 丢新 vs 盖旧 | 概念 | `keyboard.c` vs `input.c:338-346` | 与 07 的对照 | 存：13 §1.5 |

- **验收标准**：读者能说出终端收到的两类消息分别走哪条路；能说出终端的溢出策略与服务相反；能说出"私房位"为什么不能出境；篇首必须标注"实现归 16-stage-drivers"。

### 15-input-pckbd-contract

- **一句话定位**：站在键盘鼠标驱动作者的位置，讲清它作为客户端库的模型用户要承担的三件翻译责任。
- **讲什么**：K-246、K-247、K-248、K-249、K-250；外加"Rust 实现已存在于 `os/drivers/hid/pckbd/`"的回指（G-18）。
- **不讲什么**：
  - 硬件端口编程细节 → `../16-stage-drivers/13-pckbd-driver.md`；
  - 扫描码全表 → 同上（登记 edge E-PCKBDREG）；
  - 库本体 → 13（只作使用面）；
  - 服务侧消费 → 08。
- **前置**：00、02、13。
- **后置**：无。
- **事实底线**：
  - C：`pckbd.c:37-41`（回调表）、`:111`（scan_keyboard）、`:328`（kbd_process）、`:374`（kbdaux_process）、`:418`（pckbd_leds）、`:434`（pckbd_intr）、`:456`（pckbd_alarm）、`:465-507`（init/startup/main）；
  - Rust：`os/drivers/hid/pckbd/src/{lib,led,mouse,scancode,tables,char_face}.rs`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-246 | 三钩子表 | 接口 | `pckbd.c:37-41` | 契约面 | 存：14 §2.2 |
| K-247 | 键盘翻译 | 机制 | `pckbd.c:328` | 翻译责任一 | 存：14 §2.3 |
| K-248 | 鼠标翻译 | 机制 | `pckbd.c:374` | 翻译责任二 | 存：14 §2.4 |
| K-249 | 灯回写 | 机制 | `pckbd.c:418` | 与 10 的接线点 | 存：14 §2.5 |
| K-250 | 所有权划分 | 约束 | `../16-stage-drivers/13-pckbd-driver.md` | 裁剪声明 | 存：14 头部 |

- **验收标准**：读者能说出 K-027（三层责任）里"翻译归驱动"这一层在 pckbd 里具体是什么；能说出灯掩码最终变成什么（端口位）；篇首必须标注实现归 16-stage-drivers 并给出 Rust 侧已存在实现的回指。

### 16-input-failure-invariants-arch

- **一句话定位**：把散落在各篇的失败语义收成一张总账——每个错误码从哪来到哪去、C 一共在哪些地方会崩、四条不变量怎么成立、四处架构演进各标在哪。
- **讲什么**：K-260、K-261、K-262、K-263、K-264、K-265、K-266、K-267、K-268、K-269、K-270、K-271、K-272、K-273、K-274。
- **不讲什么**：
  - 每个错误在具体机制里的完整推导 → 06-10（只引用）；
  - 常量的数值 → 99（本篇只讲语义）；
  - 测试怎么写 → 17。
- **前置**：00-12（全部机制篇）。
- **后置**：17（不变量是测试族的来源）。
- **事实底线**：
  - C：19 处 panic（见 §0.4 命令 5 的完整清单）；`input.c` 全部 errno return；`chardriver.c:195-274`；
  - Rust：`error.rs`（`InputError` 十变体 + `to_errno`）、`structs.rs:155`、`eventbuf.rs`、`minix-chardriver/src/driver.rs`；
  - 规范：`AGENTS.md` Key Constraints 的 `[ARCH: …]` 三处一致纪律。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-262 | 19 处 panic 全清单 | 约束+演进 | 见 §0.4 命令 5 | **新增总账** | **新** |
| K-267 | count 上限不变量 | 约束 | `input.c:27-28` | **新增** | **新** |
| K-268 | suspended ⇒ opened | 约束 | `input.c:96,186` | **新增** | **新** |
| K-269 | leds 跨 owner 保持 | 约束 | `input.c:661,228,552` | **新增** | **新** |
| K-271 | 失败即日志清单 | 约束 | `input.c:232,420,573,600` | **新增** | **新** |
| K-272 | 服务永不退出 | 约束 | `input.c:685-691` | **新增** | **新** |
| K-274 | 错误路径总账 | 架构 | `input.c` 全部 return | **新增** | **新** |

- **验收标准**：读者能用一张表回答"C 里 input 相关代码一共崩几处、每处 Rust 怎么处置"；能说出四条不变量并指出破坏它们的后果；能说出改一处 C 行为要在几处标 `[ARCH: …]`。

### 17-input-test-infrastructure

- **一句话定位**：讲清这套 Rust 代码为什么能在没有传输的情况下测，以及测试地图、运行命令、易腐内容的处理纪律。
- **讲什么**：K-280、K-281、K-282、K-283、K-284、K-285、K-286、K-287。
- **不讲什么**：
  - 各判决的测试内容细节 → 各篇（本篇只给地图与族名）；
  - 传输层的集成测试 → 12 的 E-INWIRE 联调（本篇只说明现状）。
- **前置**：00-12。
- **后置**：无。
- **事实底线**：
  - 实测（2026-09-19）：`os/servers/input/src` 91 个 `#[test]`；`minix-sys/src/inputdriver.rs` 7 个；`minix-types/src/ipc/{input,input_event,key_codes}.rs` 13 + 6 + 2 个；
  - `todo.md` IN-D1（计数过时曾被修一次，本篇据此立纪律）；
  - `todo.md` IN-P3-2 / IN-P3-3（边界族与 wire 族的来源）。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|------|------|------|------|-------------|------|
| K-280 | 判决即数据 | 架构 | 全 crate | 可测性的理由 | 存：各篇 §5 |
| K-281 | 测试地图 | 工具 | 实测 grep | **新增** | **新** |
| K-282 | 计数易腐纪律 | 约束 | `todo.md` IN-D1 | **新增** | **新** |
| K-284 | 节点表互锁 | 测试 | `input.rs` `INPUT_NODES` | 从 05 迁入 | 存：05 §4.3 |
| K-286 | 边界族 | 测试 | 各模块 | 从 todo IN-P3-2 落入 | 存：todo |

- **验收标准**：读者能按地图找到任一模块的测试族；能说出"为什么文档不再写死测试总数"；篇内必须给出 `cargo test -p minix-input` 等四条命令与"用 `grep -c "#\[test\]"` 取当前数"的做法。

### 99-input-constants

- **一句话定位**：纯查表页——每个常量的值、权威定义位置、含义在哪一篇。
- **讲什么**：全部常量（消息号、事件页与码的摘要、设备编号、容量、errno、端点与 DS 键）。
- **不讲什么**：任何"为什么"——一律回指机制篇。
- **前置**：无（任何时候可跳读）。
- **后置**：无。
- **事实底线**：`minix-types/src/ipc/{input,input_event,key_codes}.rs`、`types/{errno,endpoint}.rs`、`structs.rs`、`com.h:872-937`、`input.h`（服务侧）、`minix/input.h`、`dmap.h:78`、`sys/kbdio.h`、`MAKEDEV.sh:330-343`。
- **知识点清单**：K-026、K-041、K-042、K-051、K-061、K-062、K-064、K-190、K-260、K-213（均以"值 + 权威 + 含义篇号"三列形式出现）。
- **验收标准**：每个常量都能在一屏内查到值、权威文件、含义篇号；任一行都不出现机制解释；与机制篇冲突时以本篇"权威"列为准并要求回改机制篇。

---

## 6. 变更表

### 6.1 总表

| 操作编号 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向（存量）/ 来源（新增） |
|---------|------|--------|--------|------|-----------|--------------------------|
| CH-01 | 重排 | 04-input-event-format | **02**-input-event-vocabulary | 共享词汇无前置，应早于一切服务端机制 | K-020~K-030 | 全部原篇搬移，合并 99 §3 的值表指针 |
| CH-02 | 重排 | 02-chardriver-framework | **05**-chardriver-front-door | 前台三判决只有看到具体请求才有意义；放在 06 之前 | K-070~K-082 | 全部原篇搬移；块设备部分压缩为一句 |
| CH-03 | 重排 | 05-input-message-contract | **04**-input-message-contract | 契约是词汇，应早于前台 | K-050~K-066 | 全部原篇搬移；并入 02 §2.2 的 CDEV 编号 |
| CH-04 | 拆分+合并 | 09 §2.3（入队与溢出） | **07**-input-read-park-ringbuffer | 环形缓冲是一个数据结构的两半 | K-107、K-113 | 07 的"入队与溢出"一节；08 只保留引用 |
| CH-05 | 拆分+合并 | 09 §2.4（唤醒） | **09**-input-wake-cancel-select | 唤醒属于"挂起的出口"，与取消/查询同构 | K-140、K-141 | 09 的"出口一"一节；08 只保留"入队后叫人"一句 |
| CH-06 | 拆分+合并 | 08 §1.2/§2.1/§4.1（灯位翻译） | **10**-input-leds | 灯是一个完整语义单元 | K-160、K-161 | 10 的"位翻译"一节；08 只保留"控制通道只做路由，翻译见 10" |
| CH-07 | 拆分+合并 | 08 §2.2/§2.3（取消与查询） | **09**-input-wake-cancel-select | 与唤醒同属挂起的出口 | K-142~K-149 | 09 的"出口二/三"；08 不再有请求面内容 |
| CH-08 | 拆分 | 05 §2.6（灯位语义） | 灯位值留 04，语义归 **10** | 消除越界 | K-061、K-062 | 04 留编号与位值；10 讲语义 |
| CH-09 | 拆分 | 11 §2.3（断开唤醒） | 机制留 **09**，生命周期主叙述留 **11** | 断开唤醒是"挂起的出口"之一 | K-149 | 09 出口一节；11 只保留"退房叫醒，见 09" |
| CH-10 | 合并 | 99 §2/§4 的值表 | **99**-input-constants（保留） | 值表天然是索引页 | K-041、K-042、K-051、K-061、K-064、K-260 | 99 保留，按新篇号更新"含义在哪篇"列 |
| CH-11 | 拆分 | 99 §5（错误码） | 值留 99，语义与路径归 **16** | 消除越界（99 不该讲机制） | K-260、K-261、K-262、K-274 | 16 的 errno 与 panic 总账；99 只留值 |
| CH-12 | 新建 | —（无） | **12**-input-dispatch-effects-transport | 最大覆盖缺口：传输已落地、dispatcher 是全 crate 最大模块、effects 无独立文档 | K-200~K-215 | 新增：C `chardriver.c:455-570`、`input.c:608-641`；Rust `dispatcher.rs`/`effects.rs`/`serve.rs`/`main.rs` |
| CH-13 | 新建 | —（无） | **16**-input-failure-invariants-arch | 19 处 panic、四条不变量、错误路径总账无处安放 | K-262、K-267~K-274 | 新增：C 全部 panic 点 + `error.rs`；OS 通用概念（服务不变量） |
| CH-14 | 新建 | —（无） | **17**-input-test-infrastructure | 测试计数已过时一次（IN-D1），且散在 15 篇 | K-281、K-282 | 新增：实测 grep 数据 + `todo.md` IN-D1/IN-P3-2 |
| CH-15 | 重排 | 12-libinputdriver | **13**-libinputdriver-client | 让 12 承载"合拢篇"，邻居视角后移 | K-220~K-232 | 全部原篇搬移 |
| CH-16 | 重排 | 13-tty-consumer | **14**-input-tty-contract | 同上 | K-240~K-245 | 全部原篇搬移；逐行 C 分析压缩（V-1） |
| CH-17 | 重排 | 14-pckbd-driver | **15**-input-pckbd-contract | 同上 | K-246~K-250 | 全部原篇搬移；逐行 C 分析压缩（V-2） |
| CH-18 | 改写 | 00-input-overview | **00**（同名） | 补执行模型、构建面、三条阅读路径 | K-001~K-004、K-015、K-016a | 原篇 + 新增两节 |
| CH-19 | 改写 | 01-input-init-main | **01**-input-startup-and-sef | 补 SEF 生命周期（K-006）与回调表空缺（K-016） | K-005~K-014、K-016 | 原篇 + 新增两节；回调表逐项解释下放 05 |
| CH-20 | 改写 | 03-input-device-structs | **03**-input-device-table | 改名更准（不只结构，还有编号体系） | K-031~K-041 | 原篇搬移 |
| CH-21 | 改写 | 06-input-open-close | **06**（同名） | 补 `CDEV_WRITE` 得 EIO 的说明 | K-090~K-096 | 原篇 + 一句 |
| CH-22 | 改写 | 07-input-read-suspend | **07**-input-read-park-ringbuffer | 并入入队半（CH-04） | K-100~K-113 | 原篇 + 新节 |
| CH-23 | 改写 | 08-input-ioctl-cancel-select | 拆出到 **09**，本篇标题作废 | 请求面的三个配角改挂到事件流组 | K-142~K-149 | 全部拆出（无残留） |
| CH-24 | 改写 | 09-input-event-processing | **08**-input-event-intake | 拆出入队与唤醒后，本篇只剩上报与路由 | K-120~K-130 | 原篇减去 CH-04/CH-05 |
| CH-25 | 改写 | 10-input-setleds | **10**-input-leds | 并入位翻译（CH-06） | K-160~K-171 | 原篇 + 新节 |
| CH-26 | 改写 | 11-input-driver-connect | **11**-input-driver-lifecycle | 补 Rust 两阶段连接（K-191） | K-180~K-192 | 原篇 + 新节 |
| CH-27 | 归档 | 全部 16 篇旧编号文档 | 归档（不删） | B 相归档不删，锚点按 §8 迁移 | — | 全部知识点均有去向，无删除项 |

### 6.2 删除项（明确列出，G5 要求）

**存量知识点无删除项。** 全部 128 条存量知识点都在新目录中找到去向（见 §2 各表的"现有位置"列与 §6.1 的"去向"列）。

唯一被**压缩**而非删除的是：
- 02 篇中 libchardriver 的块设备部分（V-3）：从独立小节压缩为一句 + 回指，理由是与 input 无涉；**压缩不等于删除**，跳转目标为新 05 的一句说明。

---

## 7. 缺漏新篇

按 §3.5 固定的非 C 主题清单逐项落实：

| 主题 | 重要性与原料 | 归哪一篇 | 验收标准 |
|------|-------------|---------|---------|
| **链接与加载** | input 是普通用户态进程，无独立链接脚本；不懂这一点会误以为它有 boot 镜像布局。原料：`servers/input/Makefile:5-6`（`DPADD+= ${LIBCHARDRIVER} ${LIBSYS}`）、`os/servers/input/Cargo.toml` | **00**（构建面一节），机制归 `../14-stage-runtime/` | 读者能说出 input 的产出物链接了哪两个库 |
| **镜像与内存布局** | 决定读者对"它什么时候存在"的认知。原料：`kernel/table.c` grep 阴性 + `system.conf:400-403` | **01**（一句 + 证据命令） | 读者能自己跑命令验证它不在 boot image |
| **汇编入口与陷阱进入** | **明确不在本 stage**（用户态 C/Rust 进程，入口是 `main`） | 归 `01-stage-kernel` | 本 stage 无需验收；00 一句话说明即可 |
| **启动装配** | 本 stage 的骨架第一段。原料：`system.conf:400-403`、`input.c:646-704`、`sef.h:9,60` | **01**（主讲述点） | 见 01 契约的验收标准 |
| **构建与工具链** | 三个 Makefile + Cargo.toml | **00**（构建面一节） | 见上 |
| **跨模块接口与线格式** | 本 stage 最核心的协议内容 | **02**（事件词汇）+ **04**（三面消息） | 见 02/04 契约的验收标准 |
| **错误路径** | 散在 8 篇会导致错误语义不一致 | **16**（主讲述点，K-260~K-274） | 见 16 契约的验收标准 |
| **关闭与退出** | C 的 input 永不退出，驱动侧有 terminate；不说清会让人误找退出路径 | **01**（服务侧 K-014、K-272）+ **13**（驱动侧 K-231） | 读者能回答"服务会退出吗"（不会）与"驱动怎么退出" |
| **并发与同步** | 单线程模型决定 Rust 类型选择；只有代码注释讲了 | **00**（执行模型一节，K-015） | 读者能回答"为什么没有 `Mutex`" |
| **测试基建** | 计数已过时一次（IN-D1）；散在 15 篇 | **17**（主讲述点，K-280~K-287） | 见 17 契约的验收标准 |

**三项新篇章（CH-12/13/14）的原料与验收已在 §5 的对应契约里给出**，此处不重复。

---

## 8. 锚点迁移与断链成本

### 8.1 编号映射与"含义变化"判定

| 旧编号 | 旧主题 | 新编号 | 新主题 | 编号含义是否变化 |
|--------|--------|--------|--------|-----------------|
| 00 | 总览 | 00 | 总览 | 否 |
| 01 | 启动 | 01 | 启动 + SEF | 否（扩） |
| 02 | 字符框架 | **05** | 字符框架 | **是** |
| 03 | 设备结构 | 03 | 设备表 | 否 |
| 04 | 事件格式 | **02** | 事件词汇 | **是** |
| 05 | 消息契约 | **04** | 消息契约 | **是** |
| 06 | 打开关闭 | 06 | 打开关闭 | 否 |
| 07 | 读与挂起 | 07 | 读、挂起与环形缓冲 | 否（扩） |
| 08 | 控制取消查询 | **09** | 挂起的三个出口 | **是** |
| 09 | 事件处理 | **08** | 事件上报与路由 | **是** |
| 10 | 置灯 | 10 | 灯 | 否（扩） |
| 11 | 驱动连接 | 11 | 驱动生命周期 | 否（扩） |
| 12 | 客户端库 | **13** | 客户端库 | **是** |
| 13 | 终端消费 | **14** | 终端契约 | **是** |
| 14 | pckbd 契约 | **15** | pckbd 契约 | **是** |
| — | — | **12** | 裁决/效应/传输 | 新建 |
| — | — | **16** | 失败与不变量 | 新建 |
| — | — | **17** | 测试基建 | 新建 |
| 99 | 全局概念 | 99 | 常量总表 | 否（缩） |

**含义变化的编号：8 个（02、04、05、08、09、12、13、14）。**

### 8.2 引用量实测

| 引用类别 | 实测命令 | 总量 | 受影响量 |
|---------|---------|------|---------|
| 文档间"第 NN 篇"互引 | `grep -o "第 NN 篇"` 逐编号统计 | **898** | **477**（8 个变化编号的引用数：02=89、04=51、05=76、08=66、09=72、12=44、13=43、14=36） |
| 文档间文件名形式引用 | `grep -o "[0-9][0-9]-[a-z-]*\.md" *.md` | **133** | 约 60（plan.md/todo.md 占 33，会被 B 相同步更新或标注归档） |
| `os/` 代码注释引用 | `grep -rn "<doc>.md" --include=*.rs os/` | **20 处 / 15 文件** | **20**（全部文件名都变） |
| 其它 stage 对本目录的文件级引用 | `grep -rn "12-stage-input/NN-*.md"` | 约 **8 处**（16-stage 5、17-stage 1、13-stage 1、15-stage 1） | 约 **3 处**（`12-libinputdriver.md` → 13、`14-pckbd-driver.md` → 15） |
| `.review/` 历史扫描快照 | 同上 | 约 **28 处** | 0（**不回改**：历史快照按项目规范保持原样） |

### 8.3 锚点迁移表（逐篇）

| 旧位置（文档与小节） | 旧内容（一句话） | 新位置 | 迁移类型 | 备注（断链风险） |
|---------------------|-----------------|--------|---------|-----------------|
| 01 §2.3（回调表逐项） | 七个回调槽逐项解释 | 05 §回调表 | 原样搬移 | 01→05 编号变，需改引用 |
| 01 §4.5（主循环裁决就位） | dispatcher 一句话 | 12 §裁决流水线 | 改写扩充 | 编号 01 不变，但内容扩 10 倍 |
| 02 §2.2（CDEV 编号） | 七请求三回信号 | 04 §编号与字段布局 | 合并 | 02→05、05→04 双重编号变化 |
| 02 §4.5（效应） | 效应出口一句 | 12 §效应出口 | 改写扩充 | 02→05 |
| 03 §2.4/§2.5（正反查） | 换算函数 | 03 §换算（保留） | 原样 | 编号不变 |
| 04 全部 | 事件格式与码表 | 02（保留） | 原样搬移 | 04→02，引用最多需改（51 处） |
| 05 §2.5（节点表） | `/dev` 节点表 | 04 §节点表 | 合并 | 05→04 |
| 05 §2.6（灯位语义） | 灯位与控制号 | 04（值）+ 10（语义） | 拆分 | 语义部分越界，拆两段 |
| 06 §2.3（bug 证据链） | 关闭前不 cancel | 06（保留） | 原样 | 编号不变 |
| 07 §2.2（拷贝几何） | 两段搬运 | 07 §出队几何 | 原样 + 并入入队 | 编号不变 |
| 08 §1.2/§2.1/§4.1 | 灯位翻译 | 10 §位翻译 | 拆分 | 08→09，同时内容去 10 |
| 08 §2.2/§2.3 | 取消与查询 | 09 §出口二/三 | 拆分 | 08→09 |
| 09 §2.3（入队） | 入队与溢出 | 07 §入队与溢出 | 拆分 | 09→08，同时内容去 07 |
| 09 §2.4（唤醒） | 唤醒挂起与选择者 | 09 §出口一 | 拆分 | 09→08，内容去 09（新） |
| 10 全部 | 置灯广播与记忆 | 10（保留）+ 位翻译新节 | 合并 | 编号不变 |
| 11 §2.3（断开清理） | 唤醒与通知 | 11（主叙述）+ 09（唤醒机制） | 拆分 | 编号不变 |
| 12 全部 | 客户端库 | 13（保留） | 原样搬移 | 12→13 |
| 13 全部 | 终端消费 | 14（保留，压缩 C 分析） | 改写 | 13→14 |
| 14 全部 | pckbd 契约 | 15（保留，压缩 C 分析） | 改写 | 14→15 |
| 99 §2/§4 | 消息号与设备编号 | 99（保留） | 原样 | 编号不变，只改"含义在哪篇"列 |
| 99 §5 | 错误码汇总 | 99（值）+ 16（语义与路径） | 拆分 | 编号不变 |

### 8.4 引用迁移表（代码注释，20 处）

| 文件 | 旧引用 | 新目标 | 验证方式 |
|------|--------|--------|---------|
| `os/servers/input/src/main.rs:4` | `01-input-init-main.md` | `01-input-startup-and-sef.md` | `grep -n "01-input-startup-and-sef" os/servers/input/src/main.rs` |
| `os/servers/input/src/init.rs:17` | `01-input-init-main.md` | 同上 | 同上 |
| `os/servers/input/src/structs.rs:15` | `03-input-device-structs.md` | `03-input-device-table.md` | grep 新名 |
| `os/servers/input/src/error.rs:11` | `03-input-device-structs.md` | 同上 | grep 新名 |
| `os/servers/input/src/error.rs:12` | `02-chardriver-framework.md` | `05-chardriver-front-door.md` | grep 新名 |
| `os/servers/input/src/dispatcher.rs:24-27` | `01/02/06-08/09/10/11` | `01/05/06/07/09/08/10/11`（02→05、08→09、09→08） | 逐行核对 |
| `os/servers/input/src/handlers.rs:21-22` | `06/07/08` | `06/07/09` | 逐行核对 |
| `os/servers/input/src/eventbuf.rs:15` | `07-input-read-suspend.md` | `07-input-read-park-ringbuffer.md` | grep 新名 |
| `os/servers/input/src/produce.rs:14` | `09-input-event-processing.md` | `08-input-event-intake.md` | grep 新名 |
| `os/servers/input/src/setleds.rs:20` | `10-input-setleds.md` | `10-input-leds.md` | grep 新名 |
| `os/servers/input/src/connect.rs:20` | `11-input-driver-connect.md` | `11-input-driver-lifecycle.md` | grep 新名 |
| `os/servers/input/src/effects.rs:32-34` | `02/10/11/13` | `05/10/11/14` | 逐行核对 |
| `os/libs/minix-types/src/ipc/input_event.rs:23` | `04-input-event-format.md` | `02-input-event-vocabulary.md` | grep 新名 |
| `os/libs/minix-types/src/ipc/key_codes.rs:20` | `04-input-event-format.md` | 同上 | grep 新名 |
| `os/libs/minix-sys/src/inputdriver.rs:18` | `12-libinputdriver.md` | `13-libinputdriver-client.md` | grep 新名 |
| `os/libs/minix-sys/src/lib.rs:33,95` | `12-libinputdriver.md` | 同上 | grep 新名 |
| `os/libs/minix-chardriver/src/lib.rs:12` | `01-chardriver-framework.md`（其它 stage） | 需与 16-stage 同步确认；12-stage 侧新目标为 `05-chardriver-front-door.md` | 跨 stage 协商 |

### 8.5 断链成本摘要

- **受影响引用总数**：约 **560 处**（文档间互引 477 + 代码注释 20 + 外部文件级 3 + plan/todo 内部 60）。
- **零成本部分**：文档间互引的 477 处**全部落在 B 相要重写的 16 篇正文内部**——B 相本来就要重写每一篇的正文，重排引用是同一次编辑的副产品，边际成本接近零。这是"重建而非搬移"的关键收益。
- **真正需要单独动作的**：
  1. **代码注释 20 处 / 15 文件**（热点：`os/servers/input/src/` 13 处、`os/libs/` 7 处）。建议方式：B 相末尾一次性 `sed` + 逐文件 `grep` 确认 + 单条提交，按锚点纪律在 fix-status 留痕。
  2. **外部 3 处**（16-stage `12-libinputdriver.md`/`14-pckbd-driver.md`、17-stage `12-libinputdriver.md`）：需在对应 stage 的重建里同步，或本 stage 完成后发一条跨 stage 通知。
  3. **`plan.md` / `todo.md` 内部 60 处**：B 相把这两份标记为"参考材料（已归档）"并加一句"编号映射见 `doc_rerank_*` 共识蓝图"，无需逐条改。
- **热点文件**：`os/servers/input/src/dispatcher.rs`（4 行注释指向 5 个编号）、`os/servers/input/src/effects.rs`（3 行）、`os/servers/input/src/handlers.rs`（2 行）。
- **不回改**：`.review/` 下的 28 处历史扫描快照（按项目规范，历史产物保持原样）。

---

## 9. 验证与自检门

### 9.1 四种机械检查

| 检查 | 方法 | 结果 |
|------|------|------|
| **1. 前向引用扫描** | 逐篇检查 §5 契约的"前置"字段，确认只指向更早编号 | **通过**。全部 19 篇的前置集合：00=∅；01={00}；02={00}；03={00}；04={00,02,03}；05={00,03,04}；06={00,03,05}；07={00,02,03,05,06}；08={00,02,03,07}；09={00,03,05,07,08}；10={00,02,03,04,08}；11={00,03,04,10}；12={00,01,03,04,05,06,07,08,09,10,11}；13={00,02,04,11}；14={00,02,04,08,10}；15={00,02,13}；16={00…12}；17={00…12}；99=∅。**无一指向更大编号** |
| **2. 依赖关系图无环** | 由上述前置关系构图 | **通过**。图是分层的：00 → {01,02,03} → {04,05} → {06,07} → {08} → {09} → {10,11} → {12} → {13,14,15} → {16,17}，99 孤立。无环，且存在一个拓扑序与编号序一致 |
| **3. 覆盖率** | 知识点池 152 条逐条查去向 | **通过**。存量 128 条：127 条有明确新篇章 + 小节；1 条（libchardriver 块设备部分）明确压缩并给出跳转目标，**无删除项**。新增 24 条：全部有 C 源码锚点 / 非 C 制品路径 / 组织判断理由（K-066、K-113、K-171 三条"组织判断"类已显式标注为组织判断而非事实断言） |
| **4. 断链成本统计** | §8.2 实测 | **完成**。受影响约 560 处，其中 477 处边际成本为零，20 处代码注释需批量改，3 处跨 stage，60 处参考材料内部 |

### 9.2 九门自检

| 门 | 检查内容 | 结果 | 证据 |
|----|---------|------|------|
| **G1** | C 真序是否逐条可核对（随机抽十条核对锚点） | **通过** | 抽查：S5=`input.c:652-662`（实测该区间确为清表循环）、S6=`input.c:665`（实测 `ds_subscribe`）、L5=`chardriver.c:506-513`（实测门判断）、A4=`input.c:186-192`（实测挂起四件套）、A6=`input.c:140-153`（实测回绕算术）、B3=`input.c:388-390`（实测 owner 校验）、B5=`input.c:404-421`（实测三级跳板）、B8=`input.c:362`（实测"恰好一个"）、C4=`input.c:221-235`（实测键盘窗口遍历）、D4=`input.c:451`（实测"断开但开着不给"）。十条全部对上 |
| **G2** | 知识点池是否完整：每个 C 文件、每个非 C 制品都有归属或明确排除加理由 | **通过** | C 文件：`input.c`/`input.h`/`chardriver.c`/`inputdriver.c`/`minix/input.h`/`inputdriver.h`/`chardriver.h`/`sys/kbdio.h`/`pckbd.c`/`tty.c`/`keyboard.c` 全部有知识点归属。非 C 制品：`system.conf`、`MAKEDEV.sh`、三个 Makefile、`table.c` 阴性证据、Cargo.toml 全部有归属（见 §3.5）。排除项 X-1~X-5 各带理由与归属 stage |
| **G3** | 新目录是否满足前向引用为零 | **通过** | 见检查 1 |
| **G4** | 依赖关系图是否无环；有环是否给出拆解 | **通过（无环）** | 见检查 2。唯一潜在环是"01 需要 03 的换算 / 03 需要 01 的清表"——已用"00 建立直觉 + 01 只作直觉层使用"化解（序差表 D-2），不是环 |
| **G5** | 覆盖率 100%；新增条目都有证据锚点；明确删除项单独列出 | **通过** | 见检查 3 与 §6.2（删除项：无） |
| **G6** | 每处拆分/合并是否写清存量知识点去向；每处新建是否写清新增知识点来源（抽查十处） | **通过** | 抽查十处：CH-04（K-107→07）、CH-05（K-140/141→09）、CH-06（K-160/161→10）、CH-07（K-142~149→09）、CH-08（K-061/062→04+10）、CH-09（K-149→09）、CH-11（K-260/274→16）、CH-18（K-015/016a→00）、CH-19（K-006/016→01）、CH-12（K-200~215 全部带 `dispatcher.rs`/`effects.rs`/`serve.rs` 锚点） |
| **G7** | 每篇契约是否七要素齐全 | **通过** | 19 篇逐篇含：一句话定位 / 讲什么 / 不讲什么 / 前置 / 后置 / 事实底线 / 知识点清单 + 验收标准 |
| **G8** | 锚点迁移表是否覆盖所有变化文档的每一节；引用迁移表是否覆盖文档与代码注释 | **部分通过** | 锚点迁移表（§8.3）覆盖 16 篇旧文档中**发生变化的 21 个小节**；未列的是"原样搬移且编号不变"的整篇（03、06、10、11 的多数小节），这些不变动因此无需迁移。引用迁移表（§8.4）覆盖 `os/` 下实测到的全部 20 处代码注释。**缺口**：`os/libs/minix-chardriver/src/lib.rs:12` 指向的是另一个 stage 的文档名，需跨 stage 协商，已在表中标注 |
| **G9** | 事实断言是否都有锚点；推测项是否已标注 | **通过** | 抽查十条断言：①"input 不在 boot image"—`table.c` grep 阴性 + 命令；②"协议无真回信"—`com.h:886` 原文；③"三级跳板"—`input.c:404-421`；④"唤醒恰好一个"—`input.c:362`；⑤"先记后发"—`input.c:227-231`；⑥"断开但开着不给"—`input.c:451`；⑦"退房不动队列"—`input.c:533-553`（无 tail/count 写入）；⑧"只登记 fresh"—`input.c:688`（无 lu/restart）；⑨"`MAX_NR_OPEN_DEVICES=256`"—`driver.h:41`；⑩"input 无 write 钩子"—`input.c:31-39`。全部有锚点。**推测/待验证项**：K-213 的"CDEV_REPLY_BASE 历史错值"来自 99 §2 的注释与 edge E-CDRCONV 记录，未逐字节复核 `minix-chardriver/src/protocol.rs` 当前值——已在此标注为**待 B 相验证** |

### 9.3 结论

**结论：完成（可作为共识蓝图的候选输入）。**

一处遗留需用户/共识裁决：

1. **跨 stage 引用 `os/libs/minix-chardriver/src/lib.rs:12`** 指向的不是 12-stage 的文档名（它写的是 `01-chardriver-framework.md`，属 16-stage-drivers 的编号空间）。本蓝图不越界改别 stage 的锚点，建议共识蓝图把它登记为一条跨 stage 同步项。
2. **K-213 的错值状态需 B 相实测复核**（见 G9 第 10 条）。
3. **13/14 两篇契约篇是否保留在本 stage**——本蓝图选择保留但压缩（理由：它们是 input 协议的邻居契约，读者在本 stage 就能看到完整三方图；实现篇所有权仍归 16-stage）。若共识倾向"彻底移交给 16-stage"，则本目录缩为 17 篇，13/14 改为一句回指；本蓝图保留当前的 19 篇方案。
