# 00-devm-overview：DEVMAN 整体架构概览

> **定位**：11-stage-devman 的导航图。本篇回答三个问题：devman 是什么（在 Minix3 的哪个位置）、全部语义有多大（四层版图）、15 篇文档按什么顺序读。细节全部移交 01~13/99，本篇只给地图，不讲机制。
> **源码版图**：`minix3/minix/servers/devman/`（4 个 .c + 3 个 .h，共 1013 行）+ `minix3/minix/lib/libvtreefs/`（10 个 .c + 4 个 .h，共 1642 行，devman 使用面）+ `minix3/minix/lib/libdevman/`（2 个 .c + 1 个 .h，共 603 行）+ 协议头（`com.h:846-866`、`minix/devman.h`）+ 外部消费者（RS/devmand/usbd）。
> **Rust 现状**：`os/servers/devman/`（01 已落地 `hooks.rs` + `main.rs`，7 测试通过；02~13 模块按本篇 §3 表格顺序落地）。
> **前置依赖**：无（本篇是入口；kernel/RS 背景概念用一句话引入，不展开）。
> **不覆盖（移交）**：一切机制细节（见 §4 导航表）。

---

## 1. 概念：设备管理器在系统中的位置

### 1.1 操作系统都要回答"设备从哪里被发现"

Linux 用 sysfs 把设备翻译成伪文件，Redox 用 scheme 把设备翻译成 URL（01 §1.1 已展开这组类比）。Minix3 的答案是 devman：一个**看起来像文件系统的用户态服务器**，把设备树与设备事件都装进文件接口里——devmand 读 `/sys/events` 拿事件，读 `<设备路径>/dev_type` 判类型（13 详述）。

理解 devman 先要摆正它的系统位置：**它不在内核启动镜像里**。`minix3/minix/kernel/table.c:44-64` 的 `boot_image` 数组没有 devman 条目（全文件 grep `devman` 空结果，01 §2.8 已验证），内核启动时不知道它的存在。是 RS（Reincarnation Server）读 `minix3/etc/system.conf:422-429`（`service devman { uid 0; vm SETCACHEPAGE CLEARCACHE }`），以 uid 0 把它 fork+exec 起来。plan 把这叫 **RS 加载组**：devman 与 IS/input/ipc 同类，都是运行时服务，不是 boot 服务。

### 1.2 本 stage 的两条主线

全部 15 篇文档按两条线组织（plan §1.2/§1.3 的 condensed 版，完整时序图见 01 §1.2）：

**主线——DEVMAN server 启动顺序**（读者学习顺序 = 系统实际执行顺序）：

```
RS 按 system.conf 加载 devman（12 前半段）
  → main() 填 hooks、定 root_stat（01）
  → run_vtreefs 进 VTreeFS 主循环（02 框架）
  → VFS mount /sys 触发 init_hook（01 §2.7）
  → devman_init_devices 建 root_dev + devices/ + events/（04）
  → 主循环分发 VFS 请求与 DEVMAN_* 消息（05/06 + 07~09）
```

**次主线——一次设备注册的完整旅程**（ADD → 事件 → devmand → RS bind）：

```
驱动经 libdevman 注册设备（10/11）
  → devman 入树 + ADD 事件 + 回复 dev_id（07/06）
  → devmand 轮询 events、匹配驱动、起驱动进程（13）
  → RS 在服务发布时握手 DEVMAN_BIND（12/09）
  → 设备状态 BOUND（03 状态机）
```

**每篇文档必须能回答它在这两条线的哪个位置**（plan §1.2 位置可回答性原则）。读任意一篇前先看 §4 导航表里它的"主线位置"列。

### 1.3 四条写作原则

1. **概念首次出现即完整解释**——后续只引用不复述（如 VTreeFS 在 01 §1.2 完整解释，02 起直接用）。
2. **禁止前向引用**——编号顺序即依赖顺序：读 07 前应已读 04/05/06（§4 表格的"前置"列保证了这点）。
3. **每篇一个语义单元**——可独立阅读（配合"过渡"节回填上下文）。
4. **ARCH 三处一致**——凡偏离 C 行为的重写决策，必须在文档、设计快照、代码注释三处同标 `[ARCH: …]`（清单见 §2.4）。

### 1.4 边界声明

本篇是地图：§2 只列 C 版图（文件/行数/陷阱预告，不分析函数），§3 只列 Rust 版图（模块规划 + 原则，不贴实现），§4 是 15 篇导航表。任何机制问题（"fall-through 到底什么后果""wire 格式每个字节是什么"）请按 §4 跳对应篇，本篇故意不答。

---

## 2. C 源码版图：四层语义

devman 的"全部语义"横跨四层——这是本 stage 篇幅最大的结构性事实：服务端代码只占一小半。

### 2.1 四层一览（行数均为 `wc -l` 实测）

| 层 | 内容 | 规模 | 归属文档 |
|---|---|---|---|
| 服务端 | `servers/devman/`：main.c（93）/ bind.c（105）/ buf.c（129）/ device.c（520）+ devman.h/devinfo.h/proto.h | 4 个 .c + 3 个 .h，共 **1013 行** | 01/03/04/05/06/07/08/09 |
| 运行框架 | `lib/libvtreefs/`：vtreefs.c/table.c/inode.c/mount.c/file.c 等（devman 使用面） | 10 个 .c + 4 个 .h，共 **1642 行** | 02（使用面裁剪；完整框架属 procfs stage，不重复） |
| 协议面 | `com.h:846-866`（DEVMAN_BASE 0x1200 + 10 消息常量 + 5 字段宏）+ `minix/devman.h`（wire 结构 + USB 模型） | 约 60 行 | 03/05/99 |
| 客户端库 | `lib/libdevman/`：generic.c（275）/ usb.c（301）/ local.h（27，客户端 `devman_dev` 结构归 10） | 2 个 .c + 1 个 .h，共 **603 行** | 10/11 |
| 外部消费者 | RS（manager.c devman_id 握手 5 处）/ devmand（main.c 942 + usb.y 134 + usb_scan.l 43 = 1119）/ usbd 等 USB 驱动（仅契约面） | — | 12/13（外部契约；devmand/usbd **不实现**） |

### 2.2 服务端函数清单（27 个顶层定义 + 6 静态变量，全部分配完毕）

| 文件 | 符号 | 归属 |
|---|---|---|
| main.c | `main` / `init_hook` / `message_hook` / `read_hook` | 01（+05 分发面） |
| device.c | `devman_generate_path` / `devman_device_add_event` / `devman_device_remove_event` / `devman_event_read` / `devman_static_info_read` / `devman_init_devices` / `do_reply` / `do_add_device` / `_find_dev` / `devman_find_device` / `devman_dev_add_static_info` / `devman_dev_add_child` / `devman_dev_add_info` / `do_del_device` / `devman_get_device` / `devman_put_device` / `devman_del_device` + 6 静态（`next_device_id`/`default_dir_stat`/`default_file_stat`/`root_dev`/`event_inode_data`/`event_inode`）+ 核心结构（`devman_device` / `devman_event`，全字段归 03） | 04/05/06/07/08 |
| bind.c | `do_bind_device` / `do_unbind_device` | 09 |
| buf.c | `buf_init` / `buf_printf` / `buf_append` / `buf_result` | 06 |

逐行行号与完整映射见 plan §5.3（本 stage 覆盖契约的唯一权威基线）。

### 2.3 C 侧语义陷阱预告（三处，判定各归其篇）

1. **`message_hook` switch 无 break**（main.c:46-58）：每条消息贯穿执行全部 4 个 handler。现象归 01 §2.4，定性与修复归 **05（A-3）**。
2. **5 个声明无使用的消息**（`DEVMAN_ADD_BUS/DEL_BUS/ADD_DEVFILE/DEL_DEVFILE/REQUEST`，仅 com.h 定义）+ `DEVMAN_DEVINFO_DYNAMIC` TODO fall-through（device.c:413）。归 **05（A-6 defer + fail-closed）**。
3. **死结构与死宏**（`devman_device_file`、`DEVMAN_DEFAULT_MODE`，仅头文件定义）+ server 不读的 wire 字段（`subsystem_offset`）+ 遗留字段（`major` 仅 root 置 -1）。归 **03/05/99**（标注跳过/兼容保留）。

### 2.4 架构演进（ARCH）总表（A-1~A-10，明细与三处标注见 plan §4）

| # | 项 | 落点 |
|---|---|---|
| A-1 | VTreeFS 框架依赖（新建 crate 还是内部最小实现，02 决策） | 02 + 01/06 使用点 |
| A-2 | 动态内存与链表（malloc/TAILQ → Box/Rc/RefCell/Vec/VecDeque，单线程） | 03/04/06/08 |
| A-3 | message_hook fall-through 修复（单 handler 分派） | 05 + 07/08/09 |
| A-4 | 序列化 wire 格式（显式结构 + 边界检查，no_std） | 03/05/07/10 |
| A-5 | 整数宽度（int → u32/DeviceId 新类型；dev_id 溢出处理） | 03/04/08 |
| A-6 | 未实现消息面（不实现，fail-closed，wire 常量保留） | 05 + 03 + 07 |
| A-7 | 错误处理（panic/printf → Result/Option 显式传播） | 各 handler 篇（01 已实例：SefHooks） |
| A-8 | 事件队列与 EOF 语义（TAILQ → VecDeque，offset/len 保持） | 06 |
| A-9 | RS-only 权限（endpoint 类型化 + 白名单，fail-closed 强化） | 05/09/12 |
| A-10 | 路径生成（递归 strcat → 显式长度上限） | 04/06 |

---

## 3. Rust 重写版图：原则与模块规划

### 3.1 五条非 negotiable 原则

1. **Rewrite 非 Translate**：外部行为等价，内部用 Rust 类型重表达（newtype 收窄整数、Option 消灭哨兵值、Result 替代 errno 传递、RAII 替代手动 free）。1:1 直译一经发现即 P0。
2. **单线程事件循环**（CLAUDE.md 执行模型）：devman 是用户态服务器，`Cell`/`RefCell`/`Rc` 合理，不需要 `Arc`/`Mutex`；`bool` 守卫不需要 `AtomicBool`（01 FirstGuard 已示范）。
3. **`#![no_std]`**（测试除外）：生产代码只用 `core` + `alloc`。
4. **错误码对齐 Minix3 errno**：复用 `minix_types::Errno`，不自创码；C 的 panic  speech 转显式 `Result`（A-7）。
5. **硬件/框架 trait 化**：VTreeFS 侧能力经 trait 注入（02 决策 A-1 时定形状），OS 层不见裸指针穿越。

对照 Redox：scheme 的"缺失方法=默认错误"思想对应本实现的"缺失 hook=`None` 安全默认"（01 §3.1）；Redox 的 URL 名字空间思想对应"设备树即文件树"（01 §1.1）。对照站内：VM server 的单线程论证是本 crate 并发模型的直接先例（01 §3.1/§3.2 引用）。

### 3.2 模块落地顺序（与文档顺序同构）

| 文档 | Rust 模块 | 详述位置 |
|---|---|---|
| 01 | `hooks.rs`（FsHooks/RootStat/ServerConfig/FirstGuard/SefLifecycle）+ `main.rs` | 01 §4（7 测试通过） |
| 02 | `vtreefs/`（框架数据平面 + `run`/`Transport`，A-1 决策已落地；main park 收窄为传输接线） | 见 02 |
| 03 | `structs.rs`（状态机/常量）+ `wire.rs`（A-4 解析） | 见 03 |
| 04 | `device_tree.rs`（树/DFS/路径/dev_id） | 见 04 |
| 05 | `ipc/message.rs` + `ipc/dispatch.rs`（A-3 单分派/A-9 白名单/A-6 fail-closed） | 见 05 |
| 06 | `buf.rs` + `event_queue.rs`（A-8） | 见 06 |
| 07/08/09 | `add_device.rs` / `del_device.rs` / `bind.rs` | 见 07~09 |
| 10/11 | `minix-sys/devman_client.rs` + `usb_model.rs` | 见 10/11 |
| 12 | `rs_contract.rs`（契约，mock 测试为主） | 见 12 |
| 13 | 无（devmand 是外部用户态进程，**不实现**，只文档化契约） | 见 13 |
| 99 | 常量/错误码收口进 `minix-types` | 见 99 |

---

## 4. 文档导航：15 篇

阅读顺序默认按编号（= 启动链顺序）；只关心"设备从注册到可用"一条旅程的读者可按"旅程序"列跳读 10→07→06→13→12→09。

| 编号 | 文档 | 主线位置 | 前置 | 移交 |
|---|---|---|---|---|
| 00 | 本篇（总览导航） | — | 无 | 一切机制细节 |
| 01 | 启动入口与主循环锚点 | main→run_vtreefs→mount→init_hook | 00 | 框架内部（02）、handler（05~09） |
| 02 | VTreeFS 框架契约 | 主循环内部（fsdriver 表/inode 树/read 路径） | 01 | 业务结构（03）、read_fn 实现（06） |
| 03 | 核心数据结构与 wire 格式 | 全阶段底座 | 02 | 树操作（04）、消息面（05） |
| 04 | 设备树（root/DFS/路径/dev_id） | init_hook 触发的建树 | 03 | 事件（06）、add/del（07/08） |
| 05 | 消息面契约（常量/grant/do_reply/权限/fall-through 判定） | fs_other 分发面 | 01 + 03 | 各 handler 业务（07~09） |
| 06 | 事件与缓冲（buf/事件队列/read_hook 分发） | VFS read 路径 | 02 + 03 | 事件生产方（07/08） |
| 07 | 设备添加（do_add 全流程） | 生命起点 | 04 + 05 + 06 | 删除面（08） |
| 08 | 设备删除（refcount/ZOMBIE） | 生命终点 | 04 + 05 + 06 + 07 | bind 细节（09） |
| 09 | 绑定/解绑（RS 握手/转发 owner） | 生命就绪态 | 05 + 08 + 10 | 客户端 bind_cb（10/11） |
| 10 | 客户端库 libdevman（序列化/grant/响应面） | 旅程起点（驱动侧） | 05 | USB 建模（11） |
| 11 | USB 设备建模（属性生成/add/remove/回调） | 旅程起点（USB 侧） | 10 | devmand 匹配（13） |
| 12 | RS 集成契约（publish/unpublish 握手/权限） | 进程生死两端 | 09 | devmand（13） |
| 13 | devmand 消费契约（事件解析/DSL/启停脚本） | 旅程中段（外部进程） | 06 + 04 | server 内部（01~09） |
| 99 | 全局概念（常量/错误码/跨服务引用收口） | 全阶段底座 | 全部 | 各机制细节 |

---

## 5. 测试基线

- `cargo test -p minix-devman`：截至 2026-09-04，**7 passed / 0 failed**（01 子集：guard/root/config/hook 默认/sef 双路径）。
- 约定（review-doc-skill §2.4j）：每篇落地时在该篇 §5 列出本篇测试，并在本节累积总数；本节数字随 stage 推进单调增长，终态覆盖全部核心语义（L1 对偶 + L2 契约为主，no_std 下无 doctest 运行位——单元测试承载）。
- Gate D/E 对导航篇的适用性：本篇无专属测试函数与 trait 声明（导航篇只引用 01 的 7 测试作基线），D-1/D-2 记 N/A（理由本句），D-3/D-4/D-5 按 §2/§3 引用验证通过（见 scan）。

---

## 6. 过渡

地图在手，下一站是 01：devman 进程的第一行代码。01 会带你走完 `main` 的三步（填表→定根→开跑），钉死"挂载触发初始化"这个延迟，并把 VTreeFS 主循环的门推开一条缝——门后的世界（17 种请求的分发、inode 树的分配查找、read 循环）是 02 的。若你是从设备旅程来的（"一个 USB 设备插上去之后发生什么"），读完 01 §1.3 的钩子直觉后可直接跳 10，再沿旅程序走。

---

## 7. 参见

- `01-devm-init-main.md` — 启动入口（主线第一站）
- `02-vtreefs-framework.md` — 主循环内部（A-1 决策正文）
- `05-devm-message-contract.md` — fall-through 定性（A-3）与未实现消息（A-6）
- `13-devmand-consumer.md` — 外部消费全景（次主线中段）
- `99-devm-global-concepts.md` — 常量/错误码收口
- `plan.md` — 本 stage 覆盖契约 §5 + ARCH 清单 §4（写作基线）
- `draft/README.md` — 旧占位素材（历史背景，非规范）
- C 源：`minix3/minix/servers/devman/`、`minix3/minix/lib/libvtreefs/`、`minix3/minix/lib/libdevman/`、`minix3/minix/commands/devmand/`、`minix3/etc/system.conf:422-429`
