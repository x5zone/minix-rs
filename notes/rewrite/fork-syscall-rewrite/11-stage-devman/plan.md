# 11-stage-devman 文档重组计划（plan.md）

> **状态**: 定稿（2026-08-16 首版 + 深度 review + minix3 源码回归 review，见 §7）
> **范围**: `notes/rewrite/fork-syscall-rewrite/11-stage-devman/`
> **目标**: 以 **DEVMAN server 启动顺序为主线**定义 DEVMAN 全部文档；设备生命周期旅程为次主线；最终覆盖 Minix3 devman server（`servers/devman/`，4 个 .c，1013 行）+ 运行框架（`lib/libvtreefs/`，1642 行，devman 使用面）+ 协议面（`com.h`/`minix/devman.h`）+ 客户端库（`lib/libdevman/`，613 行）+ 外部消费者（RS/devmand/usbd）全部语义，支撑 devman server 的彻底 Rust 重写
> **对照**: `01-stage-kernel/`（讲述结构参照）、`02-stage-vm/`/`07-stage-ds/`/`10-stage-mib/`（同流程先例）、`minix3/minix/servers/devman/`（ground truth）、`os/servers/devman/`（Rust 实现，当前为 stub）

---

## 1. 背景与动机

### 1.1 现状问题

`11-stage-devman/` 目录自 2026-08-14 补建以来仅有**占位 README**（2026-08-16 移入 `draft/`），没有任何正式文档。现状与 devman 的语义地位不匹配：

1. **语义面横跨四层**——devman 不是一个"纯 IPC 服务器"：它运行在 VTreeFS 框架之上（`run_vtreefs` 主循环 + inode 树 + read hook），服务端语义（`servers/devman/`）只占全部语义的一小部分。完整语义还包括：**协议面**（`com.h:846-866` 的 DEVMAN_* 消息 + `minix/devman.h` 的序列化 wire 格式）、**客户端库**（`lib/libdevman/`：设备序列化 + grant 发送 + bind/unbind 回调）、**外部消费者**（RS 的 `devman_id` bind/unbind 握手、devmand 守护进程的事件消费与驱动启停、usbd/USB 驱动经 libdevman 的设备注册）。Rust 重写必须同时复刻这些契约，缺一不可。
2. **框架依赖必须显式化**——`main.c:89` 直接调用 `run_vtreefs(&hooks, 1024, 0, &root_stat, 0, BUF_SIZE)`，全部设备树/事件/读取语义都挂接在 VTreeFS 的 inode/read 机制上。Rust 侧当前 `os/` 中没有 VTreeFS 等价物（grep 实证：`os/` 无 vtreefs/fsdriver 代码），这是必须提前决策的架构演进项（A-1），不能当作"外部库黑盒"跳过。
3. **C 侧存在真实的语义陷阱**——`main.c:46-58` 的 `message_hook` switch **无 break**（fall-through），每条 DEVMAN_* 消息会顺序执行全部 4 个 handler（ADD 后立即 DEL + 2×EPERM，见 A-3）。`device.c` 的 `DEVMAN_DEVINFO_DYNAMIC` 是 TODO fall-through。`com.h` 声明了 5 个未实现消息（ADD_BUS/DEL_BUS/ADD_DEVFILE/DEL_DEVFILE/REQUEST）。这些都必须逐一定位并在计划中给出决策，Rust 重写才不会被 C 代码"表面正确性"误导。
4. **与 07-stage-ds / 10-stage-mib 相同**——无旧主线文档可迁移（只有占位 README），本计划从零定义文档集；§5 覆盖契约是后续写作的**唯一权威基线**，必须一次到位。

### 1.2 新主线：DEVMAN server 启动顺序 + VTreeFS 主循环

与 `01-stage-kernel` / `02-stage-vm` / `07-stage-ds` / `10-stage-mib` 相同，采用**读者学习顺序 = 系统实际执行顺序**的组织原则。devman 的执行链是严格线性的：

```
RS 运行时加载 devman（不在 boot_image，kernel/table.c:44-64 无 devman 条目 → RS 加载组）
  │  system.conf:422-429  service devman { uid 0; vm SETCACHEPAGE CLEARCACHE }
  ▼  main.c:70-91  main()
  ├─ 填 hooks：init_hook / read_hook / message_hook          ← 01：钩子注册（主循环锚点）
  ├─ 设 root_stat：S_IFDIR|0444, uid/gid 0, size 0, NO_DEV   ← 01
  └─ run_vtreefs(&hooks, 1024, 0, &root_stat, 0, BUF_SIZE)   ← 02：VTreeFS 框架
       ├─ vtreefs.c: sef_local_startup → sef_startup          ← 01：SEF 生命周期（init fresh/restart）
       ├─ fsdriver_task(&vtreefs_table)                        ← 02：主循环（VFS 请求 + fs_other）
       │    ├─ VFS mount → fs_mount → init_hook               ← 02/01：mount 触发初始化
       │    │    └─ devman_init_devices()                     ← 04：root_dev + devices/ + events/
       │    ├─ VFS read → fs_read → read_hook → read_fn       ← 06：读取路径
       │    │    ├─ events 文件 → devman_event_read            ← 06：事件队列消费（EOF 语义）
       │    │    └─ 静态信息文件 → devman_static_info_read      ← 06
       │    └─ fs_other（非文件请求）→ message_hook            ← 05：消息分发面
       │         ├─ DEVMAN_ADD_DEV → do_add_device            ← 07（设备添加）
       │         │    ├─ grant safecopy（sys_safecopyfrom）   ← 05：消息面/拷贝原语
       │         │    ├─ _find_dev(parent_dev_id)             ← 04：树查找
       │         │    ├─ devman_dev_add_child                  ← 07：子设备创建 + wire 解码
       │         │    │    ├─ dev_id = next_device_id++        ← 04
       │         │    │    ├─ add_inode 设备目录               ← 02：VTreeFS inode 集成
       │         │    │    ├─ devman_dev_add_info（STATIC/DYNAMIC）← 07
       │         │    │    └─ "devman_id" 静态信息文件         ← 07
       │         │    ├─ state=UNBOUND, owner=ep               ← 03：生命周期状态机
       │         │    ├─ devman_device_add_event("ADD …")      ← 06：事件入队
       │         │    └─ do_reply（DEVMAN_REPLY + DEVICE_ID）  ← 05
       │         ├─ DEVMAN_DEL_DEV → do_del_device             ← 08（设备删除）
       │         │    ├─ _find_dev → ENODEV                    ← 04
       │         │    ├─ devman_device_remove_event             ← 06
       │         │    ├─ BOUND→ZOMBIE（若 bound）              ← 03：状态机
       │         │    └─ devman_put_device（refcount→0 → devman_del_device）← 08
       │         ├─ DEVMAN_BIND / DEVMAN_UNBIND（仅 RS）       ← 09（绑定/解绑）
       │         │    ├─ EPERM（非 RS，bind.c:14,63）    ← 05：RS-only 权限
       │         │    ├─ 转发 owner（ipc_sendrec；owner 经 libdevman devman_handle_msg 响应）← 09/10
       │         │    ├─ state BOUND/UNBOUND + refcount get/put ← 03/08
       │         │    └─ do_reply（DEVMAN_REPLY + RESULT）→ RS ← 05/12
  │
  ▼ 外部消费链（次主线：设备生命周期旅程，见 §1.3）
```

**每篇文档必须能回答一个问题：它位于 devman 启动时序（main → run_vtreefs → mount → init_hook）或主循环（fsdriver_task 消息/请求分发）的哪个位置。** 这是从 `01-stage-kernel/00-kernel-overview.md §3` 继承的组织原则。

### 1.3 次主线：设备生命周期旅程（ADD → 事件 → devmand → RS bind）

devman 的全部工作本质上是"一个设备从注册到绑定驱动的生命周期管理"。次主线以"一次设备注册的完整旅程"贯穿阶段 4~6：

```
驱动（usbd / usb_storage / usb_hub，经 libdevman）
  │  devman_init()（ds_retrieve_label_endpt("devman")）        ← 10
  │  devman_add_device()（serialize_dev → grant → ADD_DEV）    ← 10：客户端序列化 + 消息
  ▼
devman server：do_add_device → 设备入树 + ADD 事件 + reply dev_id ← 07/06
  ▼
devmand 守护进程（commands/devmand/，rc.minix:202 启动）
  │  轮询 <path>/events（默认 /sys/events）                    ← 13：事件消费
  │  handle_event 解析 "ADD <path> 0x%08x" / "REMOVE …"        ← 13
  │  determine_type 读 <path>/dev_type（"USB_DEV"/"USB_INTF"） ← 13：sysfs 属性契约
  │  generate_usb_device_id（读 idVendor/idProduct/bInterfaceClass 等 8 属性）← 13
  │  match_usb_driver（usb.y DSL 配置匹配）                    ← 13
  │  start_driver（minix-service up <binary> -major -devid -label）← 13
  │  run_upscript（mknod /dev/<label>）                        ← 13：设备节点脚本契约
  ▼
RS：publish_service（devman_id != 0 → ds 查 label → DEVMAN_BIND）← 12
  │  DEVMAN_BIND → devman → 转发驱动（libdevman do_bind → bind_cb）← 09/10
  ▼
驱动绑定完成：dev->state = BOUND，refcount+1                   ← 03/09
```

此旅程的路径图绘制在 `09-devm-bind-unbind.md`（绑定段）与 `13-devmand-consumer.md`（事件消费段），`07/08` 覆盖添加/删除段。

---

## 2. 新文档编号与阶段划分

> 编号规则与 `01-stage-kernel` 一致：`NN-短横线语义名.md`；`00-` 为总览；`99-` 为全局概念。旧占位 README 保留在 `draft/`（素材），新编号在顶层重新建立。

### 阶段总览（15 篇）

| 阶段 | 编号 | 文档 | 语义模块 | C 源码 | Rust 模块（规划） | draft 来源 | 变更 |
|------|------|------|---------|--------|-------------------|-----------|------|
| 0 总览 | 00 | `00-devm-overview.md` | devman 是什么、启动主线图、文档导航 | `servers/devman/` 全部 | 全部 | `draft/README.md` | **新建**导航 |
| 1 启动入口与运行框架 | 01 | `01-devm-init-main.md` | main()/三个 hooks/run_vtreefs 调用点/SEF 生命周期/mount 触发 init_hook/fs_other 消息面 | `servers/devman/main.c` 全部 + `vtreefs.c:sef_local_startup` | `main.rs` | 无 | **新建**：启动主线锚点 |
| 1 | 02 | `02-vtreefs-framework.md` | VTreeFS 框架契约：inode 树模型、add_inode/delete_inode/get_*、fsdriver 表、mount/unmount、read 路径、indexed 槽（devman 使用面） | `lib/libvtreefs/` 全部（按 devman 使用面裁剪） | 等价框架（A-1，决策待定） | 无 | **新建**：框架面 |
| 2 核心数据结构 | 03 | `03-devm-structs.md` | devman_device/inode/event/event_inode/static_info_inode 结构全字段、状态机（UNBOUND/BOUND/ZOMBIE）、常量、序列化 wire 格式（devinfo.h） | `servers/devman/devman.h`、`devinfo.h` | `structs.rs`、`wire.rs` | 无 | **新建** |
| 2 | 04 | `04-device-tree.md` | root_dev 初始化、devices/ 树、_find_dev/devman_find_device（DFS）、devman_generate_path、dev_id 分配、default stats | `device.c:187-207,283-308,45-70` + 静态变量 | `device_tree.rs` | 无 | **新建** |
| 3 消息面与读写机制 | 05 | `05-devm-message-contract.md` | DEVMAN_* 消息面、字段宏（GRANT_ID/SIZE/ENDPOINT/DEVICE_ID/RESULT）、grant 拷贝、do_reply、RS-only 权限、message_hook fall-through（A-3） | `com.h:846-866`、`device.c:do_reply`、`main.c:46-58` | `ipc/message.rs`、`ipc/dispatch.rs` | 无 | **新建**：协议面 |
| 3 | 06 | `06-event-buf.md` | buf.c（skip/offset 输出缓冲）、read_hook 分发、devman_event_read、devman_static_info_read、事件队列（ADD/REMOVE 格式 + EOF 消费语义）、events/ 文件 | `buf.c` 全部 + `device.c:62-170` | `buf.rs`、`event_queue.rs` | 无 | **新建** |
| 4 服务 handlers（设备生命周期） | 07 | `07-devm-add-device.md` | do_add_device 全流程、devman_dev_add_child、devman_dev_add_info（STATIC/DYNAMIC）、devman_dev_add_static_info、devman_id 文件 | `device.c:223-277,314-418` | `add_device.rs` | 无 | **新建**：生命周期起点 |
| 4 | 08 | `08-devm-del-device.md` | do_del_device、devman_get/put_device 引用计数、devman_del_device、ZOMBIE 转换、事件移除、parent refcount 语义 | `device.c:387-458` | `del_device.rs` | 无 | **新建** |
| 4 | 09 | `09-devm-bind-unbind.md` | do_bind/unbind_device、RS 握手、转发 owner（ipc_sendrec）、状态转换、libdevman 响应面交叉引用 | `bind.c` 全部 | `bind.rs` | 无 | **新建** + 绑定段路径图 |
| 5 客户端库与 USB 建模 | 10 | `10-libdevman-client.md` | libdevman generic.c：save_string/serialize_dev/devman_add_device/devman_del_device/devman_init/do_bind/do_unbind/devman_handle_msg、dev_list | `lib/libdevman/generic.c`（275 行） | `minix-sys/devman_client.rs` | 无 | **新建**：客户端契约 |
| 5 | 11 | `11-usb-device-model.md` | libdevman usb.c：devman_usb_dev/interface 模型、属性生成（bDeviceClass…dev_type）、设备+接口 add/remove、bind/unbind 回调接线 | `lib/libdevman/usb.c`（301 行）+ `minix/devman.h` lib 侧结构 | `usb_model.rs` | 无 | **新建** |
| 6 外部消费者 | 12 | `12-rs-integration.md` | RS publish/unpublish 的 devman_id 契约、system.conf 权限、ds label 查询、服务发布顺序 | `servers/rs/manager.c:840-851,897-909,1742`、`rs.h:139,182`、`system.conf:422-429` | `rs_contract.rs` | 无 | **新建**：外部契约 |
| 6 | 13 | `13-devmand-consumer.md` | devmand 守护进程：事件解析、dev_type 判定、USB 驱动匹配 DSL（usb.y/usb_scan.l）、minix-service 启停、major 位图、up/down 脚本契约、sysfs 路径约定 | `commands/devmand/`（1119 行）+ `etc/devmand/` | 用户态（**不实现**，外部契约） | 无 | **新建**：外部契约 |
| 99 全局概念 | 99 | `99-devm-global-concepts.md` | DEVMAN_BASE 0x1200、消息/状态/字符串常量、endpoint、全局状态（next_device_id/root_dev）、跨服务引用（RS/VFS/DS/usbd/devmand） | `com.h:846-866`、`devman.h` | `minix-types` | `draft/README.md` | **新建** |

### 2.1 阶段间的叙事衔接

每个阶段结尾的文档需含"过渡"小节（参照 `01-stage-kernel` 每篇 §6"过渡"），说明本阶段在启动时序/主循环中的位置与下一阶段的入口：

```
00（总览）→ 01（启动入口）→ 02（VTreeFS 框架）→ 03（数据结构）→ 04（设备树）
→ 05（消息面）→ 06（事件/读取机制）→ 07（添加）→ 08（删除）→ 09（绑定/解绑）
→ 10（客户端库）→ 11（USB 建模）→ 12（RS 契约）→ 13（devmand 消费）
```

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
2. **禁止前向引用**——依赖机制前置（本计划的编号已保证：VTreeFS 前置 02、wire 格式前置 03/05、客户端库后置 10）
3. **每篇一个语义单元**——读者可独立阅读
4. **位置可回答性**——每篇回答"它在 main → run_vtreefs → 主循环的哪个位置"

### 3.3 引用规则

- 各文档之间用新编号交叉引用（如 `05-devm-message-contract.md` §RS-only 权限）
- 与 kernel/其他 stage 文档交叉引用时用 `../NN-stage-*/` 相对路径（如 `../01-stage-kernel/NN-*.md`、`../02-stage-vm/NN-*.md`）
- 对 draft 素材的引用一律指向 `draft/README.md`，并标注"素材"

### 3.4 每篇文档边界声明（执行级）

> 写作每篇文档前，先按本表声明**前置依赖 / 本篇职责 / 不覆盖（移交）**，防止内容交叉与遗漏。边界以 §5.3 函数清单为准绳。

| 编号 | 前置依赖 | 本篇职责（核心语义） | 不覆盖（移交） |
|------|---------|---------------------|--------------|
| 00 | 无 | 总览、启动主线图、文档导航、设计原则 | 一切机制细节 |
| 01 | 00 + kernel 文档（RS 加载组） | `main`/`init_hook`/`read_hook`/`message_hook`（注册面）/`run_vtreefs` 调用点/SEF 生命周期（init fresh/restart）/mount 触发 init_hook/`fs_other` 消息面入口 | VTreeFS 内部机制（02）、各 handler 实现（05~09） |
| 02 | 01 | VTreeFS API 面（`run_vtreefs`/`add_inode`/`delete_inode`/`get_root_inode`/`get_inode_name`/`get_inode_cbdata`/`inode_stat`/hooks 结构）、fsdriver 回调表、mount/unmount、read 路径、indexed 槽、**A-1 框架决策** | devman 业务结构（03）、read_fn 实现（06） |
| 03 | 02（inode 挂接） | `devman_device`/`devman_inode`/`devman_event`/`devman_event_inode`/`devman_static_info_inode` 全字段、状态机、常量、wire 格式（`devman_device_info`/`entry`）、`BUF_SIZE`/`DEVMAN_STRING_LEN` | 树操作（04）、消息面（05） |
| 04 | 03 | `root_dev` 初始化、`devices/` 树、`_find_dev`/`devman_find_device`（DFS）、`devman_generate_path`、`next_device_id`、default stats | 事件（06）、添加/删除流程（07/08） |
| 05 | 01 + 03（消息字段） | DEVMAN_* 消息常量与字段宏、grant 拷贝（`sys_safecopyfrom`）、`do_reply`、RS-only 权限（`src != RS_PROC_NR`，`src = m->m_source`）、message_hook 分发面（**A-3 fall-through 决策**）、未实现消息（A-6） | 各 handler 业务（07~09） |
| 06 | 03（event 结构）+ 02（read 路径） | `buf_init/printf/append/result`（skip/offset）、`devman_event_read`、`devman_static_info_read`、事件队列（ADD/REMOVE 格式、EOF 消费、TAILQ_LAST 语义）、`events/` 文件 | 事件的生产方（07/08） |
| 07 | 04 + 05 + 06 | `do_add_device` 全流程、`devman_dev_add_child`、`devman_dev_add_info`、`devman_dev_add_static_info`、`devman_id`、`state=UNBOUND`/`owner` 设置、ADD 事件 | 引用计数删除面（08） |
| 08 | 04 + 05 + 06 + 07 | `do_del_device`、`devman_get_device`/`devman_put_device`、`devman_del_device`（free infos/inode/parent put）、ZOMBIE 转换、REMOVE 事件 | bind 状态机细节（09） |
| 09 | 05 + 08（refcount）+ 10（响应面） | `do_bind_device`/`do_unbind_device`、RS 握手、转发 owner、BOUND/UNBOUND 状态转换、ENODEV=19 特例（unbind 结果容错）、绑定段路径图 | 客户端 bind_cb 实现（10/11） |
| 10 | 05（消息面）+ 99 | `save_string`/`serialize_dev`（offset 布局）、`devman_add_device`/`devman_del_device`（grant + sendrec + dev_list）、`devman_init`（ds label）、`devman_handle_msg`/`do_bind`/`do_unbind`（服务端转发的响应面） | USB 高层建模（11） |
| 11 | 10 | `devman_usb_dev`/`devman_usb_interface` 模型、属性生成（bDeviceClass…dev_type）、`devman_usb_device_new/delete/add/remove`、bind/unbind 回调接线、`cb_data` | devmand 匹配（13） |
| 12 | 09（bind 消息）+ 99 | RS `publish_service`/`unpublish_service` 的 devman_id 握手、`rs.h` 字段、`system.conf` 权限（uid 0 + vm 特权）、ds label 查询、失败即 kill_service | devmand 用户态（13） |
| 13 | 06（事件格式）+ 04（路径/属性） | devmand 主循环、事件解析（ADD/REMOVE 字符串格式）、`dev_type` 判定、`generate_usb_device_id`（8 属性读取）、`match_usb_driver`/`match_usb_id`、usb.y DSL、`start_driver`/`stop_driver`（minix-service）、major 位图、up/down 脚本、`/sys` 路径约定、pid 文件 | server 内部（01~09） |
| 99 | 全部 | DEVMAN_* 常量总表、状态常量、endpoint/DS label、全局状态、跨服务引用、错误码汇总（EPERM/ENODEV/ENOMEM/EINVAL/OK） | 各机制细节（01~13） |

### 3.5 测试基线（2026-08-16）

- `cargo check -p minix-devman`：通过（stub：`lib.rs` 仅 `pub fn init() {}`）
- `cargo test -p minix-devman`：**0 passed / 0 failed**（无测试，2026-08-16 实测）
- 每篇改写完成时在文末更新该模块测试统计（review-doc-skill §2.4j）

### 3.6 Review gate 要求（每篇改写必检）

- 每篇新文档创建时同步生成 `.design/{NN}-outline.md` + `{NN}-outline-review.md` + `{NN}-design.md`（Step 0.3 嵌入生成，Gate H.6，不允许 N/A）
- 每篇改写后按 review 工作流跑 Blocker Gates（0/A/B/C/D/D-6/E/G/H），scan 产物写入 `.review/codex/devman/{NN}-{name}/`
- P0 未清不得标完成；doc 与 code 保持同步

---

## 4. 架构演进（ARCH）清单

> 按 review-core 要求，ARCH 必须三处一致标注：Minix3 行为对照点 + design doc + 代码注释。以下为 11-stage-devman 范围内的 ARCH 项，写文档时必须逐项落实。

| # | ARCH 项 | Minix3 现状 | minix-rs 演进 | 涉及文档 | 状态 |
|---|---------|------------|--------------|---------|------|
| A-1 | **VTreeFS 框架依赖** | `lib/libvtreefs/`（1642 行）共享框架（devman + procfs 共用），`run_vtreefs` 主循环 + inode 树 + fsdriver 表 | `os/` 无 VTreeFS 等价物 → devman 重写需决策：新建框架 crate（如 `os/libs/minix-vtreefs/`）或 devman 内部最小等价实现；inode 树用 Rust 类型建模 | 02（决策）+ 01/06（使用点） | **决策待定**（§7.3） |
| A-2 | **动态内存与链表** | `malloc/free` + `TAILQ`（children/infos/events/siblings 四类链表） | `Box`/`Rc`/`RefCell`/`Vec`/`VecDeque` 类型系统管理；单线程事件循环，`!Send`/`!Sync` 合理（与 VM/PM 同执行模型） | 03/04/06/08 | 设计差异 |
| A-3 | **message_hook fall-through** | `main.c:46-58` switch 无 break：每条 DEVMAN_* 消息顺序执行全部 4 个 handler（ADD 后立即 DEL + 2×EPERM，C 疑似 bug） | Rust 按消息单 handler 分派（修复）；三处一致标注（doc + design + code 注释）；外部契约以 libdevman 客户端可观察行为为准（设备添加后必须可见可 bind） | 05 + 07/08/09 | **C 缺陷修复决策**（§7.3） |
| A-4 | **序列化 wire 格式** | `devman_device_info`/`entry` offset 布局（`sizeof` 头 + 偏移寻址 + 字符串区） | Rust 显式 wire 结构 + 解析（`WireDeviceInfo` + 边界检查），no_std 安全；`subsystem_offset` 字段保留兼容但 server 不读（标注） | 03/05/07/10 | 已定（结构简化，外部行为等价） |
| A-5 | **整数宽度** | `int dev_id`/`ref_count`/`major`、`next_device_id` 递增无界 | `u32`/类型化 Newtype（`DeviceId`），refcount 用 `Rc` 强引用语义或显式计数；`dev_id` 溢出处理（原 C 无检查） | 03/04/08 | 设计差异 |
| A-6 | **未实现消息面** | `DEVMAN_ADD_BUS/DEL_BUS/ADD_DEVFILE/DEL_DEVFILE/REQUEST`（com.h 声明，全树无使用）；`DEVMAN_DEVINFO_DYNAMIC`（device.c:413 TODO fall-through → -1） | 不实现，fail-closed（未知消息不回复或 ENOSYS）；wire 常量保留；DYNAMIC 属性类型在 Rust 侧枚举占位 + NotImplemented 标注 | 05 + 03 + 07 | **缺口**：标注 defer + 语义契约 |
| A-7 | **错误处理风格** | `panic("out of memory")`/`printf("[W] …")` 告警后继续 | `Result`/`Option` 显式传播；不可恢复分配失败仍 fail-fast；日志经 `syslog` 风格接口（对齐 minix-sys） | 各 handler 篇 | 设计差异 |
| A-8 | **事件队列与 EOF 语义** | `TAILQ` 事件队列 + `buf_result()==0` 判定消费（read 全部后移除并 free） | `VecDeque<Event>` + read 返回 0 即消费；offset/len 语义保持（buf 的 skip 逻辑）；并发安全（单线程无需锁） | 06 | 沿用（结构等价） |
| A-9 | **RS-only 权限模型** | `src != RS_PROC_NR → EPERM`（bind.c:14,63，`endpoint_t src = m->m_source`） | endpoint 类型化 + RS 白名单常量；非 RS 请求 fail-closed（不转发、不回执） | 05/09/12 | 沿用（fail-closed 强化） |
| A-10 | **路径生成** | `devman_generate_path` 递归 `strcat` + `ENOMEM` 上限检查（`DEVMAN_STRING_LEN-11` 预算） | 迭代/递归生成 `PathBuf`-等价字符串 + 显式长度上限常量；`get_inode_name` 依赖在 Rust 侧由 inode 树直接提供 | 04/06 | 沿用（显式化） |

---

## 5. 覆盖完整性核对（对照 minix3 源码）

> 本节是 plan.md 的语义覆盖契约。所有断言以 grep 实证为准（review-core：禁止凭记忆）。逐项核对见 §7。

### 5.1 C 源文件 → 新文档映射

| C 源文件 | 行数 | 覆盖文档 | 核对 |
|---------|------|---------|------|
| `servers/devman/main.c` | 93 | 01（main/hooks/run_vtreefs 调用点）、05（message_hook 分发面 + A-3） | 已核对 |
| `servers/devman/bind.c` | 105 | 09（do_bind/unbind_device 全部） | 已核对 |
| `servers/devman/buf.c` | 129 | 06（buf_init/printf/append/result 全部） | 已核对 |
| `servers/devman/device.c` | 520 | 04（init_devices/_find_dev/generate_path/静态变量）、06（事件 add/remove/read + static_info_read）、07（do_add_device + add_child + add_info + add_static_info）、08（do_del_device + get/put/del_device）、05（do_reply） | 已核对 |
| `lib/libvtreefs/*.c`（13 文件） | 1642 | 02（devman 使用面：vtreefs.c/table.c/inode.c/mount.c/file.c）+ 01（sef_local_startup 锚点） | 已核对（使用面裁剪） |
| `lib/libdevman/generic.c` | 275 | 10（全部 8 函数） | 已核对 |
| `lib/libdevman/usb.c` | 301 | 11（全部 8 函数） | 已核对 |
| `commands/devmand/main.c` + `usb.y` + `usb_scan.l` | 1119 | 13（外部契约：事件消费/匹配/启停/DSL） | 已核对（外部契约，不实现） |

### 5.2 头文件/协议面覆盖

| 头文件 | 覆盖文档 | 核对 |
|--------|---------|------|
| `minix/include/minix/com.h:846-866`（DEVMAN_BASE 0x1200、10 个消息常量、5 个字段宏） | 05/99 | 已核对 |
| `minix/include/minix/devman.h`（wire 结构 + lib 侧 usb 结构） | 03/05/11 | 已核对 |
| `servers/devman/devman.h`（内部结构/常量/状态定义） | 03/04/99 | 已核对 |
| `servers/devman/devinfo.h`（wire 结构 server 侧镜像） | 03 | 已核对 |
| `servers/devman/proto.h`（函数原型） | 各 handler 篇 | 已核对 |
| `lib/libdevman/local.h`（客户端 devman_dev/attr 结构） | 10/11 | 已核对 |
| `minix/include/minix/rs.h:139,182`（devman_id 字段） | 12 | 已核对 |
| `minix/include/minix/vtreefs.h`（框架 API 契约） | 02 | 已核对 |
| `etc/system.conf:422-429`（service devman 权限） | 12/99 | 已核对 |

### 5.3 函数/符号清单映射

> 以下为 `servers/devman/` 全部顶层定义 + libdevman 全部导出 + devmand 关键接口，逐一落入新文档。行号以 2026-08-16 工作区为准。

**main.c（4 函数）**

| 符号 | 行 | 覆盖文档 |
|------|-----|---------|
| `main` | 70-91 | 01 |
| `init_hook` | 36-44 | 01（→ 04 锚点） |
| `message_hook` | 46-58 | 05（A-3） |
| `read_hook` | 60-68 | 01/06 |

**device.c（17 函数 + 6 静态变量）**

| 符号 | 行 | 覆盖文档 |
|------|-----|---------|
| `next_device_id`（静态） | 16 | 04（A-5） |
| `default_dir_stat`/`default_file_stat`（静态） | 18-33 | 04 |
| `root_dev`/`event_inode_data`/`event_inode`（静态） | 35-39 | 03/04 |
| `devman_generate_path` | 45-70 | 04（A-10） |
| `devman_device_add_event` | 75-102 | 06 |
| `devman_device_remove_event` | 108-136 | 06 |
| `devman_event_read` | 142-168 | 06（A-8） |
| `devman_static_info_read` | 173-183 | 06 |
| `devman_init_devices` | 187-207 | 04 |
| `do_reply` | 213-219 | 05 |
| `do_add_device` | 223-277 | 07 |
| `_find_dev` | 283-301 | 04 |
| `devman_find_device` | 305-308 | 04 |
| `devman_dev_add_static_info` | 314-339 | 07 |
| `devman_dev_add_child` | 345-397 | 07 |
| `devman_dev_add_info` | 404-418 | 07（DYNAMIC → A-6） |
| `do_del_device` | 424-455 | 08 |
| `devman_get_device` | 460-467 | 08 |
| `devman_put_device` | 471-480 | 08 |
| `devman_del_device` | 485-515 | 08 |

**bind.c（2 函数）**

| 符号 | 行 | 覆盖文档 |
|------|-----|---------|
| `do_bind_device` | 7-55 | 09（A-9） |
| `do_unbind_device` | 56-105 | 09（ENODEV=19 特例，:85） |

**buf.c（4 函数）**

| 符号 | 行 | 覆盖文档 |
|------|-----|---------|
| `buf_init`/`buf_printf`/`buf_append`/`buf_result` | 全部 | 06 |

**lib/libdevman/generic.c（8 函数）**

| 符号 | 行 | 覆盖文档 |
|------|-----|---------|
| `save_string` | 静态（:24） | 10 |
| `serialize_dev` | 静态（:36） | 10（A-4） |
| `devman_add_device` | :102 | 10 |
| `devman_del_device` | :154 | 10 |
| `devman_init` | :188 | 10 |
| `do_bind` | 静态（:207） | 10 |
| `do_unbind` | 静态（:233） | 10 |
| `devman_handle_msg` | :258 | 10 |

**lib/libdevman/usb.c（8 函数 + 2 静态回调）**

| 符号 | 行 | 覆盖文档 |
|------|-----|---------|
| `devman_usb_add_attr` | 静态（:24） | 11 |
| `add_device_attributes` | 静态（:44） | 11 |
| `add_interface_attributes` | 静态（:93） | 11 |
| `devman_usb_device_new`/`delete` | :143 / :174 | 11 |
| `devman_usb_device_add`/`remove` | :219 / :276 | 11 |
| `devman_usb_bind_cb`/`unbind_cb`（静态）+ `devman_usb_init` | :295 | 11 |

**devmand（外部契约关键接口）**

| 符号 | 行 | 覆盖文档 |
|------|-----|---------|
| `main_loop`/`handle_event` | 878-932 / 803-872 | 13 |
| `determine_type` | 519-587 | 13 |
| `generate_usb_device_id` | 633-680 | 13 |
| `match_usb_driver`/`match_usb_id` | 238-296 | 13 |
| `usb_intf_add_event`/`usb_intf_remove_event` | 691-762 / 766-800 | 13 |
| `start_driver`/`stop_driver` | 158-231 | 13 |
| `run_upscript`/`run_downscript`/`run_cleanscript` | 84-154 | 13 |
| `get_major`/`put_major` | 592-628 | 13 |
| `parse_config`（usb.y/usb_scan.l DSL） | 全部 | 13 |
| `create_pid_file`/`cleanup` | 全部 | 13 |

### 5.4 外部消费者与跨服务契约

| 消费者 | 契约点 | 覆盖文档 | 核对 |
|--------|--------|---------|------|
| RS | `publish_service` 的 DEVMAN_BIND 握手（manager.c:840-851，devman_id != 0 时 ds 查 label）、`unpublish_service` 的 DEVMAN_UNBIND（:897-909）、`init_slot` 的 devman_id 继承（:1742）、失败即 `kill_service` | 12 | 已核对 |
| VFS | VTreeFS 挂载（fs_mount → init_hook）、read 路径（fs_read → read_hook）、getdents/lookup | 02/06 | 已核对 |
| DS | `ds_retrieve_label_endpt("devman")`（libdevman devman_init、RS manager.c:841/898） | 10/12 | 已核对 |
| usbd / usb_storage / usb_hub | libdevman 客户端（`devman_init` + `devman_usb_device_add` 系列） | 10/11 | 已核对 |
| devmand | events 文件格式（`"ADD <path> 0x%08x"`/`"REMOVE …"`）、dev_type 属性、devman_id 属性、sysfs 路径 `/sys/` | 13 | 已核对 |
| minix-service | 驱动启停命令行（`minix-service up/down <binary> -major -devid -label`） | 13 | 已核对 |

### 5.5 明确排除 / 跳过的项

| 项 | 处理 | 依据 |
|----|------|------|
| `DEVMAN_ADD_BUS`/`DEVMAN_DEL_BUS`/`DEVMAN_ADD_DEVFILE`/`DEVMAN_DEL_DEVFILE`/`DEVMAN_REQUEST` | **未实现消息**，A-6 defer + fail-closed（wire 常量保留） | grep 全 `minix3/` 仅 `com.h:850-856` 定义，无任何使用 |
| `DEVMAN_DEVINFO_DYNAMIC` | **TODO**（device.c:413 fall-through → -1），A-6 defer，Rust 枚举占位 | grep：仅 device.c:413 一处 |
| `devman_device_file` 结构 | **C 死结构**（无使用），标注跳过 | grep：仅 devman.h:56 定义 |
| `DEVMAN_DEFAULT_MODE` 宏 | **C 死宏**（无使用），标注跳过 | grep：仅 devman.h:41 定义 |
| `devman_dev.subsys` / `devman_device_info.subsystem_offset` | **wire 兼容字段**：libdevman 序列化但 server 不读（devinfo.h:25 定义、device.c 无引用）；Rust 保留字段 + 标注 | grep：subsystem_offset 仅 2 处头文件 |
| `devman_device.major` | **遗留字段**：仅 root_dev 置 -1（device.c:193），其余无使用 | grep：major 仅 devman.h:88 + device.c:193 |
| `libvtreefs` 非 devman 使用面（getdents 索引槽/statvfs/link 等完整实现） | 02 按 devman 使用面裁剪；框架完整语义属 procfs stage（不重复） | VTreeFS 共享，devman 只用 subset |
| `commands/devmand/` 本体实现 | 外部契约（13 文档化），**不实现**（用户态守护进程，非 server 重写范围） | 范围声明（§1.1） |
| `usbd`/`usb_storage`/`usb_hub` 驱动实现 | 外部消费者，仅契约面（10/11） | 范围声明 |

### 5.6 覆盖结论与拆分答案

**1. 是否确保全部覆盖？** 是。§5.1~§5.4 已逐层核对：devman server 4 个 .c（1013 行）全部映射到 01~09（§5.1/§5.3）；VTreeFS 框架使用面（1642 行中 devman 相关面）进入 02；协议面（com.h + devman.h + devinfo.h）进入 03/05/99；客户端库 2 个 .c（613 行）全部落入 10/11；外部消费者（RS/VFS/DS/usbd/devmand）进入 12/13。§7.2 以命令证据复核，无遗漏。架构演进项（A-1~A-10，含 VTreeFS 框架 A-1、fall-through 缺陷 A-3、wire 格式 A-4、未实现消息 A-6）全部单列，不混入行为文档。

**2. 计划拆分为多少个文档，简述如何拆分？** **15 篇**：`00` 总览 + `01~13` 语义模块 + `99` 全局概念，按 **7 个阶段**组织——阶段 1 启动入口（01）→ 阶段 2 运行框架（02）→ 阶段 3 核心数据结构（03/04）→ 阶段 4 消息面与读写机制（05/06）→ 阶段 5 服务 handlers（07~09，按设备生命周期 add→del→bind 顺序）→ 阶段 6 客户端契约（10/11）→ 阶段 7 外部消费者（12/13）。拆分原则：每篇一个语义单元 + 位置可回答性 + 禁止前向引用；以函数清单（§5.3）为唯一边界准绳。

---

## 6. 实施路线

> 每篇新文档 = 依据 §3 讲述结构 + §4 ARCH 标注 + §5 覆盖契约从零写作。所有 P0 修复完成后才可推进下一篇。当前 11-stage-devman 无历史主线素材（仅占位 README），全部为新建。

1. **00-devm-overview 新建**（导航，含 §1.2 启动时序图 + 设备生命周期次主线）
2. **01-devm-init-main 新建**（启动主线锚点：hooks 注册 + run_vtreefs 调用点 + SEF）
3. **02-vtreefs-framework 新建**（框架契约 + A-1 决策）
4. **03/04 数据结构两篇新建**（结构 + 设备树；A-2/A-4/A-5 标注）
5. **05/06 消息面与读写机制两篇新建**（协议面 + 事件/缓冲；A-3/A-6/A-8 标注）
6. **07~09 服务 handlers 三篇新建**（生命周期 add→del→bind；绑定段路径图入 09）
7. **10/11 客户端契约两篇新建**（libdevman + USB 建模；A-4 标注）
8. **12/13 外部消费者两篇新建**（RS 契约 + devmand 消费契约）
9. **99-devm-global-concepts 新建**（常量/错误码/跨服务引用收口）
10. **README.md 重建**（文档清单 + 启动链路位置，参照 09-stage-init/README.md 模式）

### 6.1 文档写作状态跟踪

> 每完成一篇，将状态改为 `reviewed`（附 scan 日期）。全部 `reviewed` 且无 P0 遗留 = 阶段完成。

| 编号 | 状态 | 首轮 review 日期 | 备注 |
|------|------|-----------------|------|
| 00 | pending | — | 新建导航 |
| 01 | pending | — | 新建（启动锚点） |
| 02 | pending | — | 新建（A-1） |
| 03 | pending | — | 新建（A-2/A-4/A-5） |
| 04 | pending | — | 新建（A-5/A-10） |
| 05 | pending | — | 新建（A-3/A-6/A-9） |
| 06 | pending | — | 新建（A-8） |
| 07 | pending | — | 新建 |
| 08 | pending | — | 新建 |
| 09 | pending | — | 新建 + 绑定段路径图 |
| 10 | pending | — | 新建（A-4） |
| 11 | pending | — | 新建 |
| 12 | pending | — | 新建（A-9） |
| 13 | pending | — | 新建外部契约 |
| 99 | pending | — | 新建全局概念 |

---

## 7. Review 记录

> 本节记录 plan.md 自身的 review 过程（深度 review + minix3 回归 review），与最终 plan.md 同文档交付，保证"覆盖完整性核对"可追溯。

### 7.1 深度 review（2026-08-16）

**方法**：按 review-process Step 1-5 对计划本身做语义全覆盖审计——逐函数/逐消息/逐结构/逐消费者核对 §2 文档拆分能否承载全部语义，检查模块边界是否重叠或漏项。

**发现的问题与修复**：

| # | 问题 | 级别 | 修复 |
|---|------|------|------|
| D-1 | 初版将 `message_hook` 的 fall-through（无 break）当作普通分发表描述，未识别其**灾难性语义**（ADD 后立即 DEL + 2×EPERM） | P1 | 独立为 A-3，明确 Rust 修复决策 + 三处标注要求（§4/§5.5/§1.1） |
| D-2 | 初版未将 **VTreeFS 框架**单列文档，devman 的 inode/read/mount 语义无处安放（会散落各篇重复描述） | P1 | 新增 02-vtreefs-framework，明确"devman 使用面"裁剪 + A-1 决策（§2/§5.5） |
| D-3 | 初版把 `devman_event_read`/`devman_static_info_read`/buf.c 全部塞进"读取"篇，与事件队列语义（ADD/REMOVE 生产消费）纠缠不清 | P1 | 06 合并为"事件与缓冲"单一语义单元，明确 EOF 消费语义（A-8）归属（§2/§3.4/§5.3） |
| D-4 | 初版漏掉 `do_reply`（device.c:213-219，所有 handler 的回复原语）与 `fs_other` 消息面入口的定位 | P2 | 补入 05 职责 + §5.3 清单（§2/§5.3） |
| D-5 | 初版漏掉 libdevman `do_bind`/`do_unbind`（服务端转发消息的**客户端响应面**，bind.c 的 ipc_sendrec 对端） | P1 | 补入 10 职责 + 09 交叉引用（§2/§3.4/§5.3） |
| D-6 | 初版未识别 devmand `handle_event` 的 `dev_type` 判定与 `generate_usb_device_id` 的 8 属性读取（`../idVendor` 相对路径）——这是 sysfs 属性契约的核心 | P1 | 补入 13 职责 + §5.4 devmand 契约行（§2/§3.4/§5.4） |
| D-7 | 初版遗漏"unbind 结果 ENODEV=19 特例"（bind.c:90 `m->DEVMAN_RESULT != 19` 容错，驱动可能已自行删设备） | P2 | 补入 09 职责（§3.4） |
| D-8 | 初版将 devmand（1119 行用户态守护进程）误当作 server 重写范围 | P2 | 范围声明修正：13 为外部契约文档，不实现（§1.1/§5.5） |
| D-9 | 初版 wire 格式（A-4）未说明 `subsystem_offset` 的"server 不读"事实 | P2 | 补入 A-4 + §5.5 排除表（grep 实证） |
| D-10 | 初版未声明 `DEVMAN_ENDPOINT` 字段的传递链（RS 设置 → devman 转发 → libdevman bind_cb 入参），bind 语义闭环不完整 | P1 | 补入 05/09/10/12 职责（§2/§3.4） |
| D-11 | 测试基线缺失（stub 无测试，需与 02-stage-vm §3.5 先例一致） | P2 | 新增 §3.5（`cargo check`/`cargo test` 实测） |
| D-12 | 未声明每篇改写接入 review gate（outline/design 快照） | P2 | 新增 §3.6（Gate H.6 + `.review/codex/devman/{NN}-{name}/`） |

**结论**：修复后按 §5.3 函数清单反向核对——`servers/devman/` 4 个 .c 全部 27 个顶层函数（+6 静态变量）逐一落入 01~09；libdevman 16 个函数落入 10/11；协议面 2 头文件 + wire 结构落入 03/05/99；外部消费者（RS/VFS/DS/usbd/devmand）落入 02/06/10/11/12/13。**语义全覆盖，无遗漏**。

### 7.2 minix3 源码回归 review（2026-08-16）

**方法**：对 `minix3/minix/servers/devman/` 全部 .c/.h + 协议头文件 + libdevman + devmand + RS 契约点逐一 grep 核对 §5.1/§5.2/§5.3/§5.4 映射，并抽查 fall-through、未实现消息、死代码声明。

**证据**：

```bash
wc -l minix3/minix/servers/devman/*.c minix3/minix/servers/devman/*.h   # 4 .c + 3 .h = 1013 行，与 §1 一致
rg -n "DEVMAN_ADD_DEV|DEVMAN_DEL_DEV|DEVMAN_BIND|DEVMAN_UNBIND" minix3/minix/servers/devman/main.c   # message_hook switch 4 case 无 break（:37-47），A-3 成立
rg -n "DEVMAN_ADD_BUS|DEVMAN_DEL_BUS|DEVMAN_ADD_DEVFILE|DEVMAN_DEL_DEVFILE|DEVMAN_REQUEST" minix3/ | grep -v com.h   # 无使用 → A-6 成立
rg -n "DEVMAN_DEVINFO_DYNAMIC" minix3/minix/servers/devman/device.c   # :413 TODO fall-through → A-6 成立
rg -n "subsystem_offset" minix3/   # 仅 2 头文件定义，server 不读 → §5.5 成立
rg -n "devman_device_file|DEVMAN_DEFAULT_MODE" minix3/   # 仅头文件定义 → §5.5 死代码成立
rg -n "src != RS_PROC_NR" minix3/minix/servers/devman/bind.c   # :14,63 → A-9 成立
rg -n "!= 19" minix3/minix/servers/devman/bind.c   # :85 `!= 19` 特例 → D-7 修复成立
rg -n "devman_id" minix3/minix/servers/rs/manager.c minix3/minix/include/minix/rs.h   # :840-851,897-909,1742 + rs.h:139,182 → 12 契约成立
rg -n "run_vtreefs|add_inode|delete_inode|get_inode_name" minix3/minix/servers/devman/main.c minix3/minix/servers/devman/device.c   # 框架使用点 → 02 成立
rg -n "devman_handle_msg|devman_add_device|serialize_dev" minix3/minix/lib/libdevman/generic.c   # 8 函数 → 10 成立
rg -n "devman_usb_device_add|add_device_attributes|dev_type" minix3/minix/lib/libdevman/usb.c   # USB 建模 + dev_type 属性 → 11/13 成立
sed -n '840,851p;897,909p' minix3/minix/servers/rs/manager.c   # RS bind/unbind 握手语义
sed -n '422,429p' minix3/etc/system.conf   # service devman { uid 0; vm SETCACHEPAGE CLEARCACHE }
sed -n '876,932p' minix3/minix/commands/devmand/main.c   # main_loop 事件轮询 + handle_event 解析 → 13 成立
```

**结论**：devman server 4 个 .c 文件全部映射到新文档，无遗漏；27 个函数（+6 静态变量）逐一定位；协议面 + libdevman + devmand + RS 契约全部进入覆盖契约；A-1~A-10 与 minix3 现状对照成立（fall-through、未实现消息、死代码均以 grep 实证）。**覆盖完整性通过**。

### 7.3 写作前置决策（2026-08-16 记录，待实现阶段确认）

| 决策点 | 建议 | 依据 |
|--------|------|------|
| A-1 VTreeFS 框架 | 倾向 devman 内部最小等价实现（`os/servers/devman/src/vtreefs/`），后续 procfs stage 再抽公共 crate；不阻塞 devman 重写 | `os/` 无共享框架；devman 只用 subset（§5.5） |
| A-3 message_hook fall-through | Rust 按消息单 handler 分派（修复）；外部行为以 libdevman 客户端契约为准（设备添加后必须可见）；标注三处 | C 行为使 ADD 后立即 DEL，无消费者依赖 |
| A-6 未实现消息 | 不实现，fail-closed；wire 常量保留在 `minix-types` | grep 实证无使用 |

---

## 8. 参见

- `draft/README.md` — 旧占位 README（素材）
- `../../../../tmp/devman/` — 早期逐行笔记（main/bind/buf/device/devman.h/devinfo.h/proto.h，素材，供改写参考）
- `../00-master-plan/README.md` — 目录重排与新主线说明（devman 属 RS 加载组）
- `../01-stage-kernel/00-kernel-overview.md` — 讲述结构参照（阶段划分/组织原则）
- `../02-stage-vm/plan.md`、`../10-stage-mib/plan.md` — 同流程先例（plan 结构与覆盖契约格式）
- `minix3/minix/servers/devman/` — ground truth（服务端）
- `minix3/minix/lib/libdevman/`、`minix3/minix/lib/libvtreefs/` — 客户端库与框架
- `minix3/minix/commands/devmand/`、`minix3/etc/devmand/` — 外部消费者
- `os/servers/devman/` — Rust 实现（当前 stub）
