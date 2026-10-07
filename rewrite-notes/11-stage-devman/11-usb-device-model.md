# 11-usb-device-model：USB 设备建模

> **定位**：旅程起点的货源。本篇回答：USB 描述符怎么变成属性串、设备与接口为何是两次 ADD、bind 回调的 `(dev_id, interface)` 从哪来、remove 与 delete 为何分家。
> **源码**：`minix3/minix/lib/libdevman/usb.c`（301 行，全部）+ `minix/devman.h:22-69`（lib 侧结构与 API 声明）。
> **Rust 模块**：`os/libs/minix-sys/src/usb_model.rs`（描述符→属性→两次 ADD/DEL + 回调注册）。
> **前置依赖**：10（`add/del_device` + 回调签名 + 传输注入）。
> **不覆盖（移交）**：USB 协议解析（`minix-usb` stub，描述符已解码假设）、服务端行为（01~09）、devmand 匹配（13）。

---

## 1. 概念：一设备多接口，所以两次 ADD

USB 的物理现实：一个设备（如 U 盘）分出多个接口（如存储接口等），每个接口可绑不同驱动。devman 的模型与之同构：**设备是一次 ADD，接口是另一次 ADD**（`parent_dev_id` 指向设备 server id）——树上是父子，驱动侧是两次 `devman_add_device` 调用。接口的 `cb_data` 带 `(dev_id, bInterfaceNumber)`，设备的带 `(dev_id, -1)`——`-1` 即"这是设备本身，不是某接口"，bind 回调凭此分流。

属性串是第二条线：描述符的数字字段按 `0x%02x`/`0x%04x` 格式化成文本属性（`idVendor=0x1234` 之类），devmand 靠读这些文本匹配驱动（13 详述匹配 DSL）。**描述符→文本→匹配**，USB 即插即用的全部魔法都在这条线上，本篇锁定文本拼法（devmand 的正则与之一一对应，错一个字符就匹配不上——单测逐串锁定）。

### 1.1 边界声明

本篇讲描述建模（属性拼法、两次 ADD、回调接线、remove/delete 分家）。不讲：描述符从哪解析（USB 栈）、server 收到 ADD 后干什么（07）、devmand 匹配细节（13）。

---

## 2. C 源码分析

### 2.1 属性拼法：5 + 6 + 条件串 + dev_type（usb.c:24-141）

`devman_usb_add_attr`（:24-42）：两次 `malloc`（名/数，`CHECKOUTOFMEM` 宏 panic，:18-20）+ `memcpy`（含 NUL，`strlen+1`）+ **TAILQ_INSERT_TAIL**（注意是 TAIL，不同于 server 侧的 HEAD——属性顺序即添加顺序，device 先 5 数后条件串，dev_type 压轴；顺序即契约，单测锁 `last == dev_type`）。

设备属性（`add_device_attributes`，:44-91）：`bDeviceClass/SubClass/Protocol`（`0x%02x`）、`idVendor/idProduct`（`0x%04x` + `UGETW` 取值——小端解码在 USB 栈侧完成，此处只管格式化）、条件三串（`Product/Manufacturer/SerialNumber`，指针非空才加，:83-88）、`dev_type=USB_DEV`（:90 压轴）。

接口属性（`add_interface_attributes`，:93-141）：`bInterfaceNumber/AlternateSetting/NumEndpoints/Class/SubClass/Protocol`（全 `0x%02x`）+ `dev_type=USB_INTF`（:140 压轴）。`snprintf` 返负即 panic（每串都查，:64-66 等 6 处同形——Rust `core::write!` 对 `String` 不失败，`let _ =` 即等价，注释说明）。

### 2.2 建模对象：`devman_usb_dev` 全字段（devman.h:36-55）

`dev`（lib `devman_dev*`）/ `dev_id`（注释"server 侧 id" devman.h:38-39——**名不副实的注释**：`device_new` 时填的是调用方 USB id，`add` 成功后才被 server id 覆盖（§2.3），注释写的是终态不是初态）/ `desc`（描述符指针）/ `configuration` / `manufacturer/product/serial`（条件串来源）/ `intf_count` / `interfaces[32]`（定长 32——USB 规范接口数上限，Rust `Vec`，32 只作注释）/ `cb_data`（lib 自有）。

### 2.3 两次 ADD：设备先行，接口随后（usb.c:219-274）

`devman_usb_device_new(dev_id)`（:143-172）：双 malloc（`CHECKOUTOFMEM`）→ `parent_dev_id = 0`（直挂根，注释"For now"——TODO 式注释，行为即契约）→ `snprintf(name, 32, "USB%d")`（定长截断，10 §2.1）→ 属性表初始化（描述符等调用方填）。

`devman_usb_device_add`（:219-274）：设备属性生成 → `cb_data{dev_id, -1}` → 接线（`bind_cb = devman_usb_bind_cb` 等，:232-235）→ `devman_add_device`（失败 panic，:239-241）→ **逐接口**：malloc + `name "intf%d"` + 接口属性 + `parent_dev_id = 设备 server id`（:255——第一次 ADD 存回的 id 在此用上，10 §2.3 的"存 id 回设备"闭环）+ `cb_data{dev_id, bInterfaceNumber}` + 接线 + `devman_add_device`（失败 panic，:268-270）。

顺序即契约：设备先（父 id 方有来源），接口后（parent 指 server id）。Rust `add_usb` 同序（单测锁 server id 41/42/43 递增 + parent 链隐含）。

### 2.4 双回调：全局注册 + 缺席 ENODEV（usb.c:19-20/280-301）

静态 `bind_cb`/`unbind_cb` 函数指针（:19-20）+ `devman_usb_init` 赋值（:293-301）。`devman_usb_bind_cb`（:280-287）：有注册则调（传 lib 的 `cb_data`），无则 `ENODEV`（:285——与 10 §2.5"无回调 ≡ 失踪"同构，客户端通用规则）。unbind 镜像（:289-294）。（`devman_dev` 客户端结构见 10 §2.1，本篇只用其 USB 侧实例。）

### 2.5 remove 与 delete 分家（usb.c:276-291 vs :174-197）

`remove`：逐接口 `devman_del_device` + 设备 `devman_del_device`（失败 panic，:283-289）——**调 server**。`delete`：纯本地 `free` 链（属性名/数/体、接口 dev、设备 dev、udev，:174-197）——**不调 server**。协议：先 remove（server 摘牌），再 delete（本地收尸）；顺序反了就 use-after-free（server 还指着，本地已释放——注释写进 Rust `remove_usb` 文档，`delete` 在 Rust 里就是 `drop`，连函数都不必有，11 §3.5）。

---

## 3. Rust 设计决策

### 3.1 描述符已解码假设（与 `minix-usb` 的分界）

`UsbDeviceDesc/UsbInterfaceDesc` 是纯数值结构（`UGETW` 已应用）。USB 协议解析（描述符字节流→数值）归 `minix-usb`（stub），本模块从数值起——分界写进模块头（11 的输入契约）。

### 3.2 回调上下文：注册表解析（10 §3.3 的兑现）

10 的 `BindCallback(dev_id, ep)` + 11 的 `UsbStack` 注册表：shim 凭 dev_id 查设备/接口（`server_id`/`intf_server_ids` 双表），组 `BindData{dev_id, interface}`（设备 −1）调驱动回调。C 的 `data` 指针（lib 内置）→ Rust 的查表（类型安全，无悬空——C 的 `data` 指向的 `cb_data` 活在 udev 里，udev 释放即悬空，Rust 无此类）。

### 3.3 定长数组 → `Vec` + 注释（C 数字的去留）

`interfaces[32]` → `Vec`（32 作注释：USB 规范上限，非检查——C 也不检查越界写，`intf_count` 调用方保证；Rust 若超 32 照收，注释说明"规范上限，实现不限"，忠实且诚实）。

`name[32]` → `truncate_name` 31 截断（10 §2.1，同字节）。

### 3.4 失败 panic → `Err` 透传（A-7，10 同款）

`add/remove` 的 `panic("…failed.")` → `ClientError` 透传（`?` 直传，不包装——哪一步败的一看便知，调用栈即证据）。

### 3.5 delete = drop（协议注释代替函数）

C `delete` 的 free 链 = Rust 所有权 drop——**无对应函数是正确的**（有函数反而暗示"要调点什么"）。协议（先 remove 后 drop）写进 `remove_usb` 文档 + 单测 `delete_is_drop_after_remove`（drop 本身无行为可断言，断言协议前半 + 注释后半——诚实测试，不造"drop 测试"）。

---

## 4. 实现详解

### 4.1 模块结构

```
os/libs/minix-sys/src/usb_model.rs — BindData/UsbBindCallback/UsbDeviceDesc/UsbInterfaceDesc/UsbInterface/UsbDevice/device_attributes/interface_attributes/UsbStack/add_usb/remove_usb/device_name（+5 测试）
```

### 4.2 关键不变量

1. 属性拼法逐串锁定（单测全集，devmand 正则对照见 13）。
2. 设备先加、接口后加（parent 链不断）。
3. remove 接口先行（server 摘牌序）。
4. 缺席回调 ≡ ENODEV（10 同构）。

### 4.3 与 C 的差异说明

| C | Rust | 分类 |
|---|---|---|
| 描述符指针 | 已解码数值（minix-usb 分界） | 分层（输入契约） |
| `interfaces[32]` | `Vec`（32 注释） | 去定长（不检查与 C 同） |
| static 全局回调 | `UsbStack` 值类型 | 去全局（可多实例测试） |
| 双 panic（add/remove） | `Err` 透传 | A-7 |
| delete free 链 | drop（无函数）+ 协议注释 | 所有权（RAII） |
| `data` 野指针风险 | 注册表解析 | 悬空消除 |

---

## 5. 测试要点

| 测试 | 断言 | 对应 |
|---|---|---|
| `attributes_match_c_spellings` | 拼法全集 + 条件串缺席 + dev_type 压轴 + 接口 7 项 | §2.1 |
| `add_remove_roundtrip` | USB3 名 + id 41/42/43 + 调用 6 次 | §2.3 |
| `stack_shims_default_enodev` | 缺席 ENODEV + 注册 OK + interface −1 | §2.4 |
| `delete_is_drop_after_remove` | 协议（无 server 交互） | §2.5 |
| `client_device_binds_through_registry` | 10/11 联动（id 路由） | §3.2 |

截至 2026-09-04：`cargo test -p minix-sys` **13 passed / 0 failed**（旧 1 + 10 的 7 + 本篇 5）。`cargo clippy` 新文件 0 警告。

---

## 6. 过渡

旅程起点齐了：驱动调 10 编码注册，USB 形态由本篇拼属性，server 侧 07 落户，13 按文本匹配起驱动。但"谁决定起哪个驱动"（devmand 主循环 + usb.y DSL + major 位图 + 脚本）是 13 的，"RS 何时替服务握手"（publish 时机）是 12 的。读 12/13 时若忘记属性串长什么样，回看 §2.1 的拼法表（devmand 的正则与之一一对应）。

---

## 7. 参见

- `10-libdevman-client.md` — 传输与分拣（本篇调的 API）
- `03-devm-structs.md` — wire 解码（本篇编码的镜像）
- `07-devm-add-device.md` — server 落户（`parent_dev_id` 的消费者）
- `09-devm-bind-unbind.md` — 转发目标（bind_cb 的上游）
- `13-devmand-consumer.md` — 属性文本的消费者（拼法对照）
- C 源：`minix3/minix/lib/libdevman/usb.c`、`minix/devman.h:22-69`、`local.h`
