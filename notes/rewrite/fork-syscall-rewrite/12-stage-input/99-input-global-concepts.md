# 99-input-global-concepts: INPUT 全局概念

> **状态**: 已改写（2026-09-15，按 12-stage-input/todo.md IN-D2 补齐；数值权威以 `minix-types` 为准，本篇是导航总表而非第二真相源——每项都标注权威所在）
> **定位**: 常量总表与跨服务引用收口（阶段 99 全局概念）
> **源码**: `com.h:877-893`、`minix/input.h`、`minix/inputdriver.h`、`minix/chardriver.h`、`dmap.h:78`、`sys/kbdio.h`、`sys/sys/ttycom.h:174`
> **Rust 模块**: `minix-types`（`ipc::input`、`ipc::input_event`、`ipc::key_codes`、`types::errno`、`types::endpoint`）
> **draft 素材**: `draft/README.md` + `../../../../tmp/input/`（素材）

---

## 1. 本篇怎么用

前十四篇各讲一个机制，机制里出现的每个数字在这里都能查到"值是多少、权威定义在哪、哪一篇讲它的含义"。三条使用纪律：查值认权威（表格里"权威"列的文件），改值先改权威（禁止在各 crate 重述，见第 04 篇迁移的教训），查含义回机制篇（本篇不讲为什么）。

---

## 2. 消息号总表

权威：`minix-types/src/ipc/input.rs`（逐项对 C `com.h`），另有 `tty.rs` 的 `TTY_RQ_BASE` 家族。

| 消息 | 值 | C 出处 | 方向 | 载荷 | 含义在哪篇 |
|------|-----|--------|------|------|-----------|
| `TTY_INPUT_UP` | `0x1302` | com.h:879 | 服务→终端 | 无（在场即消息） | 01/13 |
| `TTY_INPUT_EVENT` | `0x1303` | com.h:880 | 服务→终端 | id/page/code/value/flags | 09/13 |
| `INPUT_CONF` | `0x1500` | com.h:890 | 服务→驱动 | kbd_id/mouse_id/rsvd1_id/rsvd2_id | 11/12 |
| `INPUT_SETLEDS` | `0x1501` | com.h:891 | 终端→服务、服务→驱动 | led_mask | 10/13 |
| `INPUT_EVENT` | `0x1580` | com.h:893 | 驱动→服务 | id/page/code/value/flags | 09/12 |

字符设备请求与回信（框架面，输入侧判决权威 `servers/input/src/framework.rs`，共享库同值面在 `os/libs/minix-chardriver/src/protocol.rs`——其 `CDEV_REPLY_BASE` 错值见第 2 节末注）：`CDEV_RQ_BASE 0x400`、七个请求 `CDEV_OPEN..CDEV_SELECT 0x400-0x406`（com.h:919-932）；应答基址 `CDEV_RS_BASE 0x480`、`CDEV_REPLY 0x480`/`CDEV_SEL1_REPLY 0x481`/`CDEV_SEL2_REPLY 0x482`（com.h:920，935-937）；块设备开门 `BDEV_RQ_BASE 0x500`（com.h:963，字符框架对它代答 ENXIO）。**注意**：`os/libs/minix-chardriver/src/protocol.rs:const CDEV_REPLY_BASE（L22，工具生成）` 的 `CDEV_REPLY_BASE = 0x500` 是已知错值（应为 0x480），登记在 edge E-CDRCONV，勿引用。

全单向纪律：五种 input 消息都没有真回信（com.h:886 "no real replies"）；服务的"回信"只发生在 CDEV 协议里，是对文件系统的回答，与驱动协议无关（A-9）。

---

## 3. 事件格式与事件码总表

权威：`minix-types/src/ipc/input_event.rs`（格式与小码表）、`minix-types/src/ipc/key_codes.rs`（键盘 215 码，机械生成）；C 出处 `minix/input.h`（333 行）。

**事件格式**（20 字节，`repr(C)` 对齐 input.h:25-32）：`page u16`、`code u16`、`value i32`、`flags u16`、`devid u16`、`rsvd[2] u32`。

| 家族 | 值域/代表成员 | C 出处 | 含义在哪篇 |
|------|---------------|--------|-----------|
| 事件页 `INPUT_PAGE_*` | GD `0x0001`、KEY `0x0007`、LED `0x0008` | input.h:35-39 | 04 §2.2 |
| 按下松开 | `INPUT_PRESS 1`、`INPUT_RELEASE 0` | input.h:42-43 | 04 §2.3 |
| 相对标志 | `INPUT_FLAG_REL`（鼠标位移携带） | input.h:46-47 | 04/14 |
| 通用桌面码 | GD_X/GD_Y 等 | input.h:50-57 | 04 §2.4 |
| 键盘码 | 215 个 `INPUT_KEY_*`（`KeyCode` newtype） | input.h:59-290 | 04 §2.5 |
| 灯码 | NUMLOCK/CAPSLOCK/SCROLLLOCK | input.h:292-296 | 04 §2.7 |
| 按键/消费类 | BUTTON_1..3、CONS_* | input.h:298-331 | 04 §2.7 |
| 驱动类型位 | `INPUT_DEV_KBD 0x01`、`INPUT_DEV_MOUSE 0x02`（`_SYSTEM` 段） | input.h:9-10 | 11/12/14 |
| 无效槽标记 | `INVALID_INPUT_ID -1` | input.c:443 使用 | 11/12 |

灯位换算链（三套语言，第 08/10/14 篇各自讲一段）：调用者 `KBD_LEDS_NUM/CAPS/SCROLL`（minix3/minix/include/sys/kbdio.h:kio_leds（L15，工具生成））→ 服务掩码 `1 << INPUT_LED_*`（input.c:262-268）→ 驱动端口位 `0x02/0x04/0x01`（minix3/minix/drivers/hid/pckbd/pckbd.h:LED_SCROLL_LOCK）。控制号 `KIOCSLEDS = 0x80046B02`（ttycom.h:174）。

---

## 4. 设备编号总表

权威：`servers/input/src/structs.rs`（Rust 侧 `Minor`/`DeviceIndex` 及换算函数），C `input.h:9-26`。

| 名称 | 值 | 说明 |
|------|-----|------|
| `KBDMUX_MINOR` | 0 | 键盘总机（打开它=听所有键盘） |
| `KBD0_MINOR..KBD3` | 1-4 | 单个键盘（共 `KBD_MINORS 4` 个） |
| `MOUSEMUX_MINOR` | 64 | 鼠标总机 |
| `MOUSE0..MOUSE3` | 65-68 | 单个鼠标 |
| `KBDMUX_DEV` | 0 | 槽位下标与 minor 是两套编号（A-3/A-7） |
| `FIRST/LAST_KBD_DEV` | 1 / 4 | 键盘槽窗口 |
| `MOUSEMUX_DEV` | 5 | |
| `FIRST/LAST_MOUSE_DEV` | 6 / 9 | 鼠标槽窗口 |
| `INPUT_DEV_MAX` | 10 | 事件 id 的合法上界（id 即下标，input.c:384） |
| `EVENTBUF_SIZE` | 32 | 每槽事件环形缓冲（input.h:7） |
| `EVENT_BYTES` | 20 | 单事件字节数（读回信的单位，input.c:156） |
| `INPUT_MAJOR` | 64 | `/dev/kbd*`、`/dev/mouse*` 的设备主号（dmap.h:78） |

`/dev` 节点表见 `MAKEDEV.sh:330-343`（kbdmux/kbd0-3/mousemux/mouse0-3，第 05 篇 2.6 节）。

---

## 5. 错误码汇总

权威：`servers/input/src/error.rs`（`InputError` 十变体，`to_errno` 单点映射，测试锁值 :107-119）；errno 数值权威 `minix-types/src/types/errno.rs`。

| 变体 | errno | 值 | 在哪篇产生 |
|------|-------|-----|-----------|
| `UnknownMinor` / `DeviceNotActive` | ENXIO | 6 | 06（C 有意不区分这两种） |
| `InvalidDeviceIndex` / `NotOpened` | EINVAL | 22 | 03/06 |
| `DeviceBusy` | EBUSY | 16 | 06（重复打开） |
| `WouldBlock` | EAGAIN | 35 | 07（非阻塞空读） |
| `InputOutput` | EIO | 5 | 07（拷贝失败/不营业/胃口不足） |
| `Interrupted` | EINTR | 4 | 08（取消匹配，回答原读） |
| `NotATypewriterControl` | ENOTTY | 25 | 08（未知控制号） |
| `CancelMismatch` | EDONTREPLY | 203 | 08（伪回复哨兵：不回信的合法形态） |

C 侧还有一处内部崩溃（存货不足 panic，input.c:137-138），Rust 按 A-11 统一改 `EIO` 返回（第 07 篇 3.3 节）。

---

## 6. endpoint 与数据存储键约定

| 名字 | 值/形态 | 权威 | 用途 |
|------|---------|------|------|
| `Endpoint::TTY` | 5 | minix-types/src/types/endpoint.rs:66 | 灯令唯一合法来源（input.c:631）、握手与转交目的地 |
| `Endpoint::VFS` / `MIB` | 1 / 7 | os/libs/minix-types/src/types/endpoint.rs:const VFS/:68 | 字符设备请求的来源（文件系统） |
| DS 通知来源 | DS 服务端点 | input.c:613（按来源认通知） | 触发驱动到来/离去检查 |
| `drv.inp.<label>` | u32 类型掩码 | DRIVER_KEY_PREFIX（minix-types；minix3/minix/lib/libinputdriver/inputdriver.c:inputdriver_announce（L23，工具生成）） | 驱动上线宣告（第 11/12 篇） |
| `drv.chr.<label>` | label | minix3/minix/lib/libchardriver/chardriver.c:chardriver_announce | 字符驱动向文件系统宣告上线（第 02 篇） |
| `"input"` | 服务自身 label | input.c:488 取回比对 | 驱动侧核验配置消息来源（minix3/minix/lib/libinputdriver/inputdriver.c:do_conf（L89，工具生成）） |
| `DSF_INITIAL` | 订阅标志 | ds.h（input.c:665 使用） | 订阅时补发存量键 |

---

## 7. 全局状态与跨服务引用

**全局状态只有一份**：十个槽位的设备表（C `devs[10]`，input.c:22；Rust `InputTable`，structs.rs:302）加重启门的已开门集合（`open_devs`，chardriver.c:54-94；Rust `OpenDeviceSet`）。跨请求存续的字段——`opened`（开过）、`suspended`（挂起读者）、`selector`（查询者）、`leds`（灯记忆）——分布在 `struct input_dev` 的十三个字段里（input.h:29-43，第 03 篇逐字段讲）。除此之外服务没有第二份要紧状态：单线程事件循环，一次只处理一条到达（Rust 侧收拢为 `Server { table, opened }`，第 01 篇 4.5 节）。

**跨服务引用**（谁和输入服务说话）：VFS 以 CDEV 协议送来开/读/控等请求并持有 `/dev` 节点（第 02、05-08 篇）；数据存储服务承载驱动注册键与订阅通知（第 11 篇）；终端驱动收握手与转交事件、回发灯令（第 13 篇，消费侧属驱动阶段，见 edge E-TTYEVENT）；pckbd 驱动作为客户端库的模型用户上报事件、执行灯令（第 14 篇，实现篇在 `../16-stage-drivers/13-pckbd-driver.md`，移交项见 edge E-PCKBDREG）；RS 负责加载与权限（`system.conf:400-403`：ipc 对 SYSTEM/pm/vfs/rs/ds/tty/vm 开放）。内核无直接契约——服务不持有中断，中断在驱动那一侧。

---

## 8. 过渡

本篇是整套文档的索引末页：查到值，回机制篇看含义。若某个值在本篇与机制篇说法不一，以本篇的权威列为准修机制篇——并按锚点纪律在修复记录里留痕。

---

## 9. 参见

- `minix-types/src/ipc/{input,input_event,key_codes}.rs`、`types/{errno,endpoint}.rs`：全部数值的 Rust 权威。
- `minix3/minix/include/minix/{com.h,input.h,inputdriver.h,chardriver.h}` 等：C 权威。
- 第 01-14 篇：各常量的机制含义。
- `../edge_todo.md`：E-INWIRE / E-CDRCONV / E-TTYEVENT / E-PCKBDREG——本阶段与邻居的接线现状。
