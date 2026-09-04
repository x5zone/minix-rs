# 10-libdevman-client：客户端库契约

> **定位**：旅程起点（驱动侧）。本篇回答：驱动如何把设备描述编码成 wire、grant 发给谁、id 怎么存回来、收尾谁清理、devman 转发的 BIND 到驱动后走哪条路、驱动主循环如何分拣消息。
> **源码**：`minix3/minix/lib/libdevman/generic.c`（275 行，全部）+ `local.h`（27 行，全部）。
> **Rust 模块**：`os/libs/minix-sys/src/devman_client.rs`（编码 + 收发 + 分拣）。
> **前置依赖**：03（wire 布局—本篇编码是其镜像）、05（相位表 + EPERM 不回的客户端镜像）、09（转发的另一端）。
> **不覆盖（移交）**：USB 建模（11）、服务端行为（01~09）、DS 实现（只调接口）。

---

## 1. 概念：客户端是服务端的镜像，不是附庸

libdevman 常被误读成"helper"，其实它是协议的另一半：服务端定"怎么存"，客户端定"怎么讲"——wire 格式是两边对敲出来的（03 §2.4 的布局以 `serialize_dev` 为实证，解码以 `parse_device` 为实现，编码以本篇 `encode_device` 为实现，三处同形，任一处改都必须三处同改，单测 `roundtrip` 跨不了 crate，靠 review 时三方对读——本段即对读记录）。

客户端状态机很小：init（问 DS 要 endpoint）→ add（编码→grant→sendrec→存 id→入本地表）→ 等待（主循环分拣 BIND/UNBIND）→ del（发 id→等 OK→出表）。失败策略只有一种：**panic**（generic.c 五处 `panic`，§2.3 全列）——驱动调客户端，客户端一言不合就 abort 整个驱动进程。Rust 改 Result（§3.3），abort 的决定权还给驱动。

### 1.1 边界声明

本篇讲客户端八函数。不讲：USB 属性从哪来（11）、服务端收到后干什么（07/09）、DS 怎么实现（只调 `ds_retrieve_label_endpt` 接口，注入测试）。

---

## 2. C 源码分析

### 2.1 本地状态：endpoint + 设备表（generic.c:14-21 + local.h）

```c
static endpoint_t devman_ep;                              /* :14 */
static TAILQ_HEAD(devlist_head, devman_dev) dev_list;     /* :20 */
```

两个静态量：devman 的端点（init 时 DS 查 `devman` label 得到，:193）与已注册设备表（add 入、del 出）。`devman_dev`（local.h:9-20）：`dev_id`/`parent_dev_id`/`name[32]`/`subsys*`/`data*`/`bind_cb`/`unbind_cb`/属性表/链表链——与服务端 `devman_dev`（devinfo.h:5）**同名不同形**（03 §2.7 已登记双生；客户端多了回调与定长名数组）。

`DEVMAN_DEV_NAME_LEN` 32（local.h:7）：`name` 定长数组——`snprintf(name, 32, …)` 静默截断（11 的 `"USB%d"`/`"intf%d"` 短名永不触发，但契约上是截断，Rust `truncate_name` 同字节复刻，§3.3）。

### 2.2 编码：`save_string` + `serialize_dev`（generic.c:24-99）

`save_string`（:24-34）：拷串（含 NUL）进缓冲，偏移前移，返起始偏移——offset 分配器三行，无他。`serialize_dev`（:36-99）：算总长（头 16 + 条目 16×count + 串长，:44-53）→ `malloc`（NULL 即返 NULL，调用方 panic）→ 串区起于头条目之后（:62-64 注释原文）→ 填头（count/parent/name 三项，**无 subsystem**，:73-83）→ 逐条目（`type = 0` + TODO 注释 :88、名/数偏移，**无 req_nr**，:86-94）→ `#if 0` 的 bus 残块（:80-83，死代码）。

03 §2.4 的三坑 Addendum：坑的源头全在这一个函数（不写 subsystem、不写 req_nr、type 恒 0、`#if 0`）——读 03 时若疑惑"谁的错"，答案：编码器。Rust `encode_device` 逐项镜像（含 `#if 0` 的"不实现"——连死代码都不继承，只继承布局），subsystem/req_nr 填 0（C 留垃圾，Rust 填零——字节兼容因为 server 从不读，03 §2.4 论证已闭环）。

### 2.3 收发：三 panic 套餐（generic.c:102-183）

`devman_add_device`（:102-149）：编码 → malloc NULL panic（:110）→ grant direct（CPF_READ，:112-114）→ sendrec → 发送失败 panic（:125）→ 非 REPLY panic（:129）→ RESULT 非零 panic（原文缺了 n't 的 "could add device"，:134 sic）→ **存 id 回设备**（:139 `dev->dev_id = DEVICE_ID`——server 分配的 id 流回客户端，11 的 parent 链就靠它）→ revoke → free → 入本地表 → 返 0。

`devman_del_device`（:154-183）：id 消息 → 同款三 panic → 出本地表。无 grant（id 够了）。

三 panic 的共同点：**客户端把任何失败都当世界末日**。驱动进程 abort 重启（RS 会拉起来，但设备注册状态全丢——dev_list 是内存表）。Rust 映射（§3.3）：`ClientError::{Transport, BadReply, Rejected}`，abort 权交还驱动（驱动可重试可降级，库不替它死）。

### 2.4 初始化：DS 查名 + 表清零（generic.c:188-202）

`devman_init`：`ds_retrieve_label_endpt("devman", &ep)`，失败 panic——panic 串写的是 `"usb_init: …"`（:196，函数名抄错，usb.c 的残留；行为无影响，记录在案不继承）→ `TAILQ_INIT` → 返码。Rust `init` 取注入式 DS 查询（`impl FnOnce(&str) -> Result<Endpoint, Errno>`），单测直给（`init_resolves_endpoint`）。

### 2.5 响应面：`do_bind/do_unbind` + `devman_handle_msg`（generic.c:207-275）

`do_bind`（:207-228）：按 DEVICE_ID 扫本地表 → 有回调则调（`bind_cb(data, ENDPOINT)`）→ 回 REPLY + 回调返回值 → `ipc_send(devman_ep)`（**异步**回，与 server 的转交 `sendrec` 配对：server 同步等，客户端异步回——跨进程的请求/响应不对称，05 §2.3 的"转发要等，回复不等"在此闭环）。找不着**或**有设备无回调 → 同一句话：REPLY + ENODEV（:224-226——"没这设备"与"有设备没绑回调"不可区分，调用方只认 ENODEV；Rust 原样，单测锁"无回调 ≡ 失踪"）。

`do_unbind`（:233-253）镜像。

`devman_handle_msg`（:258-275）：来源非 devman → **静默返 0**（:261-264，"we don't honor requests from others by answering them"——05 §2.4 EPERM 不回的**客户端镜像**：server 对陌生人不回，客户端对陌生人也不回，对称美学）；BIND/UNBIND → 处理返 1；其他 → 返 0。返回值是"办了/没办"，供驱动主循环分拣（办了的就别再当普通消息处理）。

---

## 3. Rust 设计决策

### 3.1 panic → `ClientError` 三变体（[ARCH:A-7] 客户端实例）

`Transport`（传输失败）/ `BadReply`（非 REPLY）/ `Rejected(errno)`（server 说不）。C 的五 panic 一一有主（§2.3/§2.4 行号对照表放 scan）。驱动拿到 `Err` 可重试（DEL 后重 ADD 是合法恢复，C panic 则连重试资格都没有——行为**超集**，fail-safe 方向，§4.3 记"演进（能力超集）"）。

### 3.2 传输注入：`ClientTransport`（grant/sendrec/revoke 三件套）

grant 机制（`cpf_grant_direct`）与 `sendrec` 皆内核穿越，`minix-sys` 现为 `todo!()`——注入 trait，测试用 `FakeTransport`（剧本式回复 + 日志 + grant 计数）。`add_device` 的 grant/revoke 配对由单测锁（`revoked == [1]`——C 若中途 panic 会漏 revoke，Rust `?` 前 revoke 必执行，注释点名）。

### 3.3 回调签名：`(dev_id, ep)`（11 需求倒逼的升级）

C 回调 `(data, ep)` 的 data 是 lib 内置 `cb_data`（id + interface）。Rust 分两层：10 的 `BindCallback = fn(dev_id, ep)`（通用：所有设备都有 id），11 的 shim 查注册表补 interface（USB 专有）。初稿曾用 `fn(ep)` 单参，11 设计时发现 interface 无处安放——升级记录见 10 scan P2-1（跨篇设计倒逼实例）。

### 3.4 本地表：调用方 `Vec`（A-2）

`dev_list` TAILQ → 调用方拥有的 `Vec`/`[HandledDevice]`（`handle_msg` 取 `&[HandledDevice]` 只读——C 扫表不改表，只读签名诚实）。增删是调用方的 `push`/`retain`（测试即示例）。

---

## 4. 实现详解

### 4.1 模块结构

```
os/libs/minix-sys/src/devman_client.rs — ClientDevice/BindCallback/ClientError/ClientTransport/encode/init/add/del/HandledDevice/handle_msg（+7 测试）
```

### 4.2 关键不变量

1. `encode_device` 输出恒可被 03 `parse_device` 解码（布局同源；跨 crate 对读见 10 scan 交叉项）。
2. grant/revoke 配对（成功失败皆 revoke，单测锁）。
3. 非 devman 来源零回复零处理（静默 false）。
4. 找不着 ≡ 无回调（同 ENODEV）。

### 4.3 与 C 的差异说明

| C | Rust | 分类 |
|---|---|---|
| 五 panic | ClientError 三变体 | A-7（能力超集：可重试） |
| grant 裸 id（cp_grant_id_t） | trait 关联 Grant（测试 i32） | 注入（传输未落地） |
| TAILQ 本地表 | 调用方 Vec | A-2 |
| 回调 (data, ep) | (dev_id, ep) + 11 解析 interface | 分层（通用/专用分离） |
| usb_init 串名/缺 n't | 不继承（行为无影响） | 去瑕（注释记录） |

---

## 5. 测试要点

| 测试 | 断言 | 对应 |
|---|---|---|
| `encode_matches_serialize_dev` | 头/条目/零字/串尾 | §2.2 |
| `name_truncates_at_31` | 40→31/短名原样 | §2.1 |
| `add_happy_path` | id 存回 + grant 配对 + 请求形 | §2.3 |
| `add_maps_failures` | Rejected/BadReply + id 不动 | §2.3 三 panic 映射 |
| `del_happy_path` | DEL 形 + OK | §2.3 |
| `handle_msg_gate_and_dispatch` | 陌生静默/BIND 回复/失踪 ENODEV | §2.5 |
| `init_resolves_endpoint` | 注入直给/错透传 | §2.4 |

截至 2026-09-04：minix-sys `cargo test -p minix-sys` **13+ passed**（本篇 7 + 11 的 5 + 旧 1；终验见 11 §5汇总）。

---

## 6. 过渡

驱动侧 toolchain 通了：编码、收发、分拣、错误映射。但"属性从哪来"还没答——`dev_type`、`idVendor` 这些串谁拼的？USB 设备与接口的描述符怎么变成属性表？谁调 `add_device` 两次（一设备一接口）？11 把 usb.c 的建模（描述符→属性→两次 ADD→两次 DEL）讲完，旅程起点的拼图才齐。读 11 时若忘记 wire 布局，回看 §2.2 的逐项镜像表。

---

## 7. 参见

- `03-devm-structs.md` — wire 解码（本篇编码的镜像）
- `05-devm-message-contract.md` — 相位表 + EPERM 不回的服务端镜像
- `09-devm-bind-unbind.md` — 转发的另一端（本篇 `handle_msg` 的上游）
- `11-usb-device-model.md` — 属性来源与 USB 建模（本篇 API 的首个大客户）
- C 源：`minix3/minix/lib/libdevman/generic.c`、`local.h`
