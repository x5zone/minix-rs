# 99-devm-global-concepts：全局概念收口

> **定位**：全 stage 的常量/错误码/跨服务总表。本篇回答：DEVMAN 有哪些数、哪些码、跟谁说话、全局状态在哪、前 14 篇的 ARCH 葬在哪。只收口，不立新机制（新机制出现即本篇越位，打回对应篇）。
> **源码**：`com.h:846-866`（已收 com.rs，05）+ `devman.h:39-51`（常量枚举，03）+ `system.conf:422-429`（权限，12）+ rs.h（端点，12）+ 各篇行号（交叉引用）。
> **Rust 落点**：`minix-types`（com.rs DEVMAN 块 + Errno）+ `minix_devman` 各模块（states/consts，见表）。
> **前置依赖**：全部（00~13）。
> **不覆盖（移交）**：各机制细节（见 01~13）。

---

## 1. 常量总表（定义 → Rust 落点，一行一证据）

| 常量 | C 值/位置 | Rust 落点 | 归属篇 |
|---|---|---|---|
| `DEVMAN_BASE` | 0x1200（com.h:846） | `minix_types::DEVMAN_BASE` | 05 |
| `DEVMAN_ADD_DEV` … `DEVMAN_UNBIND`（10 个） | +0…+9（com.h:848-859） | `minix_types::DEVMAN_*`（A-6 五个注释标 dead） | 05 |
| 字段宏 5 个（GRANT/ENDPOINT/DEVICE/RESULT） | m4 字复用（com.h:859-864） | `ipc::message` 相位视图（禁裸读） | 05 |
| `RS_PROC_NR` | Endpoint 2（com.h:61） | `minix_types::RS_PROC_NR` | 05/12 |
| `BUF_SIZE` | 4097（devman.h:39） | `hooks::BUF_SIZE` | 01 |
| `DEVMAN_STRING_LEN` | 128（devman.h:41） | `structs::DEVMAN_STRING_LEN` | 03 |
| `ADD_STRING`/`REMOVE_STRING` | `"ADD "`/`"REMOVE "`（devman.h:44-45） | 事件行字面量（06/07/08 单测锁串） | 06 |
| 状态 0/1/2 | devman.h:89-91 | `DeviceState` 枚举 | 03 |
| entry 类型 0/1/2 | devman.h:47-51 | `EntryType` 枚举 | 03 |
| `NO_DEV` | 0（const.h:132） | `hooks::NO_DEV` | 01 |
| `S_IFDIR`/`S_IRALL` | POSIX | `hooks::S_IFDIR/S_IRALL` + vtreefs `S_IFMT/S_IFREG` | 01/02 |
| `PNAME_MAX`/`NAME_MAX` | 24 / 511（vtreefs.h:14/syslimits.h:57） | 24 注释 / `NAME_MAX_LEN` | 02 |
| `DEV_NAME_LEN` | 32（local.h:7） | `devman_client::DEV_NAME_LEN` | 10 |
| `DEVMAN_TYPE_NAME` | `"dev_type"`（devmand :16） | 契约（无 Rust 对应，外部读） | 13 |
| 死宏 `DEVMAN_DEFAULT_MODE` | devman.h:41（零引用） | 挂名不建模 | 03 |
| 死结构 `devman_device_file` | devman.h:56-59（零引用） | 挂名不建模 | 03 |

对象形状 `devman_device` / `devman_dev`（同名双生，服务端/客户端各一）见 03 §2.1/§2.7；`main`（进程入口）见 01（devman 侧）——本篇只收常量与码，形状与入口各归其篇（`main` 在 12/13 语义域系跨文件同名 artifact，非缺口，见 scan）。

## 2. 错误码总表（码 → 含义 → 生产篇）

| 码 | 值 | 含义 | 生产篇 |
|---|---|---|---|
| OK | 0 | 成功（回复 RESULT） | 全 handler |
| EPERM | 1 | 非 RS 发 BIND/UNBIND（**不回复**） | 05/09 |
| ENODEV | 19 | 找不着设备（含 19 容错：unbind 驱动侧先删） | 07/08/09 |
| EINVAL | 22 | grant 非法/未知 state/下溢/解析畸形 | 05/07/08 |
| ENOMEM | 12 | 分配败/池满/id 上溢 | 全篇（A-7） |
| ENOSYS | 78 | 未实现槽/无回调（客户端）/未知 wire 类型 | 02/05/10 |
| ENAMETOOLONG | 63 | 名超长/事件行超长 | 02/03/07 |
| ENOTDIR | 20 | 非目录 lookup | 02 |

## 3. 跨服务引用（谁跟谁说话）

| 对 | 方向 | 载体 | 篇 |
|---|---|---|---|
| RS → devman | BIND/UNBIND（publish/unpublish 时） | IPC sendrec | 12→09 |
| 驱动 → devman | ADD/DEL（grant 载荷） | IPC sendrec | 10→07/08 |
| devman → 驱动 | BIND/UNBIND 转发 | IPC sendrec | 09→10 |
| 驱动 → devman | BIND 应答 RESULT | IPC send（异步回） | 10→09 |
| devman → RS | REPLY（RESULT [+DEVICE_ID]） | IPC send（异步） | 07/08/09→12 |
| devman → devmand | 事件行（ADD/REMOVE） | `/sys/events` 文件读 | 06→13 |
| devmand → minix-service | up/down 命令行 | `system()` | 13 |
| 脚本 → /dev | mknod | shell | 13 |
| 驱动 → DS | devman 端点查询（客户端 init） | DS label | 10 |
| RS → DS | devman 端点查询（publish 时） | DS label | 12 |
| VFS ↔ devman | mount/lookup/read/getdents | fsdriver 表 | 02 |

## 4. 全局状态（运行时单例，C 静态 → Rust 归属）

| C 静态 | Rust 归属 | 篇 |
|---|---|---|
| `next_device_id` | `DeviceTree.next_id`（owned） | 04 |
| `root_dev` | `DeviceTree.devices[0]`（owned） | 04 |
| `event_inode_data`（队列） | events `FileEntry` 内队列（进程表） | 06 |
| `vtreefs_hooks` 等六全局 | `VTreeFs` + `ServerConfig`（owned） | 01/02 |
| `devman_ep`（客户端） | 调用方传入（无全局） | 10 |
| `bind_cb`（客户端 USB） | `UsbStack` 值 | 11 |
| `major_bitmap[16]`（devmand） | 外部进程，不管 | 13 |

## 5. ARCH 索引（A-1~A-10 落点，明细见 plan §4）

A-1（02 内部模块）/ A-2（03/04/06/08 所有权）/ A-3（05 单分派）/ A-4（03/10 wire）/ A-5（03/04 id 与上溢）/ A-6（05 defer + 07 跳过）/ A-7（全篇 panic→Result，客户端超集）/ A-8（06 队列）/ A-9（05/09/12 RS 门）/ A-10（04 预算参数）。

## 6. 测试基线（终态）

- `cargo test -p minix-devman`：**78 passed / 0 failed**（01:7 / 02:24 / 03:7 / 04:7 / 05:5 / 06:9 / 07:4 / 08:5 / 09:6 / 12:4；00/13/99 doc-only）。
- `cargo test -p minix-sys`：**13 passed**（旧 1 + 10:7 + 11:5）。
- `cargo test -p minix-types`：**139 passed**（含 DEVMAN/Errno 新增）。
- `cargo clippy`：devman + 新文件 0 警告（minix-sys/minix-types 遗留与本 stage 无关）。

---

## 7. 参见

- `00-devm-overview.md` — 版图（本篇是其常量镜像）
- `plan.md` §4/§5 — ARCH 清单与覆盖契约（本篇的编制依据）
- 各篇 §2 — 行号证据（本篇所有断言的上游）
