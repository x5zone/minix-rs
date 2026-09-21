# 11-stage-devman 文档重建蓝图（qwen）

## 0. 元数据

- **执行者**：qwen（R 相 · 重建蓝图）
- **日期**：2026-09-19
- **目标目录**：`notes/rewrite/fork-syscall-rewrite/11-stage-devman/`
- **仓库根**：`/home/xzhao/github/minix-rs`，当前提交 `606e97607`
- **任务规格**：`AI-chats/doc-rerank-R-prompt.md`；用户补充三-note：①单篇行数上限是软的（一个概念写到 3000 行可接受，但一般控制长度利于阅读）；②旧文档数量不是限制；③旧文档可能含错误或大量未经人工审阅的内容，真相源是 minix3 C + Rust 代码，必要时可重建。

### 0.1 审查范围

- **算文档**：编号 15 篇（`00`~`13`、`99`，共约 2814 行）+ `README.md`（状态页，63 行）。
- **算参考材料**（读、不重排）：`plan.md`（482 行，覆盖契约与 ARCH 清单的权威）、`todo.md`（203 行，2026-09-15 code-excellence 轮记录）、`../edge_todo.md`（E-DMWIRE/E-DMCLIENT/E-REQWIRE/E-ISWIRE/E-DSWIRE/E5(h) 条目）、`../00-master-plan/README.md`（stage 定位行 29）。
- **范围外**：`doc_rerank_deepseek.md`、`doc_rerank_glm.md`（其他 AI 产物，按规则未读）；`.review/` 各工具中间产物；`minix3/`（只读 ground truth）；`16-stage-drivers/`、`18-stage-commands/` 等邻 stage 文档（只统计引用关系）。
- **前阶段边界确认**：`10-stage-mib/00-mib-overview.md` 无 devman 耦合引用（grep 实测零命中）；本 stage 与前后 stage 的接缝全部走 `edge_todo.md`。

### 0.2 读取清单

- **15 篇文档全文**：00(184) / 01(410) / 02(279) / 03(188) / 04(211) / 05(198) / 06(186) / 07(171) / 08(166) / 09(146) / 10(139) / 11(136) / 12(159) / 13(149) / 99(92)，另 README/plan/todo 全文。
- **C 源码（ground truth，逐一核对锚点）**：`minix3/minix/servers/devman/`（main.c、device.c、bind.c、buf.c、devman.h、devinfo.h）、`minix3/minix/include/minix/com.h`（:846-866、:61）、`minix3/minix/lib/libvtreefs/`（vtreefs.c、inode.c、file.c、path.c、mount.c、link.c）、`minix3/minix/lib/libfsdriver/fsdriver.c`、`minix3/minix/lib/libdevman/`（generic.c、usb.c、local.h）、`minix3/minix/servers/rs/manager.c`、`minix3/minix/commands/devmand/main.c`、`minix3/etc/system.conf`（:422-429）。
- **Rust 实现**：`os/servers/devman/src/` 全 19 文件 5591 行（重点 `ipc/minix.rs` 644 行、`main.rs`、`server.rs`、`hooks.rs`、`vtreefs/`）；`os/libs/minix-sys/src/devman_client.rs`（507 行）+ `usb_model.rs`（437 行）。
- **提交历史**：`git log --oneline -- os/servers/devman/src/ipc/ os/servers/devman/src/main.rs` → `f0e02c70f feat(devman): S25 主体——生产 Transport（fsdriver 协议 + DEVMAN 同循环）+ main 去停车循环`。

### 0.3 使用的命令与关键输出（证据摘录）

```
$ cargo test -p minix-devman          # 2026-09-19 实测
test result: ok. 86 passed; 0 failed            （src/lib.rs）
test result: ok. 0 passed                        （src/main.rs，bin 无测试）
$ cargo test -p minix-sys
test result: ok. 232 passed; 0 failed
# 对照：README.md/99 §6 称 78，docs 正文多处称 79，todo.md §0 称 78→79 —— 全部过期
$ grep -ohE "<NN>-[a-z-]+\.md" *.md | wc -l      # 目录内文件名提及 ≈ 142 处
$ grep -ohE "(见|参见|详见) ?[01][0-9]" *.md     # 裸编号引用 ≈ 40 处
$ grep -rn "docs: *[0-9][0-9]-" os/servers/devman os/libs/minix-sys   # 代码注释引用 = 26 处
$ grep -rl "11-stage-devman" notes/ os/ ...      # 外部文件引用 ≈ 10 个有效文件
```

锚点抽查（详见 §8.2 勘误表）：`main.c:46-58` message_hook 无 break ✓；`com.h` 字段宏实际在 :861-866（多篇引 "859-864"，漂移）；`generic.c` 的 panic 实测 8 处（:110/:125/:129/:134/:165/:169/:174/:196，doc 10 称"五处"）；`devmand/main.c` 的 DEVICE_PROTOCOL 重复检查在 :250+:254（doc 13 引 :247-248）、EEXIST 在 :579（引文 :585）、bitmap memset 在 :888（引文 :883）；`vtreefs.c` 非文件消息转交是 `fs_other`（:67-78），doc 01/02 的"工具生成"锚写成 `sef_local_startup（L66）`，符号张冠李戴；`manager.c:840/:897` 所在函数是 `publish_service`（:787）/unpublish 路径，锚写成 `rproc`（不存在的符号）。

---

## 1. C 真序

**阶段类型判定**：复合型，按提示第九部分以**服务事件循环型**为主体处理，理由：devman 是用户态 IPC 服务器（启动段 → 接收分派循环），其特殊处在于①运行框架是伪文件系统（fsdriver + VTreeFS，"消息接口"分文件请求与 DEVMAN 消息两副面孔）、②语义半径超出进程边界（libdevman 客户端、devmand、RS 三个外部对端是本 stage 声明的契约面）。因此主线 = 启动段 + 循环段，次主线 = 一次设备注册的完整旅程（外部链），与 plan §1.2/§1.3 既定结构一致——但本篇序表**从 C 源码独立重建**，不转述文档。

### 1.1 启动段（S-1 ~ S-8）

| # | 动作 | C 函数与锚点 | 说明 |
|---|------|-------------|------|
| S-1 | RS 运行时加载 devman | `etc/system.conf:422-429`（service devman { uid 0; vm SETCACHEPAGE CLEARCACHE }）；`kernel/table.c:44-64` 无 devman 条目 | 不在 boot_image，属 RS 加载组 |
| S-2 | 进程入口 | `servers/devman/main.c:70-92` `main()` | 填 hooks 三指针 + 设 root_stat（S_IFDIR\|0444, NO_DEV）|
| S-3 | 进入框架 | `main.c:89` `run_vtreefs(&hooks, 1024, 0, &root_stat, 0, BUF_SIZE)`；`libvtreefs/vtreefs.c:88` | 六个全局变量中转 |
| S-4 | SEF 生命周期注册 | `vtreefs.c:52` `sef_local_startup`（init_server :16、got_signal :39-46、sef_setcb_signal_handler :57） | fresh/restart 回调 + SIGTERM 锁存 |
| S-5 | 进入主循环 | `libfsdriver/fsdriver.c` `fsdriver_task` | sef_receive_status 循环（:88-96），收包失败 panic |
| S-6 | VFS mount 到达 | `libvtreefs/mount.c:10-38` `fs_mount` → `hooks->init_hook` | 树在 mount 时才出生（懒初始化）|
| S-7 | 设备树出生 | `servers/devman/main.c:36-43` `init_hook` → `device.c:187-207` `devman_init_devices` | root_dev（major=-1 :193）+ `devices/` + `events/` 目录 |
| S-8 | 等待挂载完成 | `fsdriver.c:40-46` mount 门 | `fsdriver_mounted` 前除 REQ_READSUPER 全 EINVAL |

### 1.2 循环段（L-1 ~ L-6）

| # | 动作 | C 函数与锚点 | 说明 |
|---|------|-------------|------|
| L-1 | 收消息、分类 | `fsdriver.c:34-35` `TRNS_GET_ID/TRNS_DEL_ID` | m_type 高 16 位是 transid，低 16 位是调用号 |
| L-2 | 文件请求走表 | `fsdriver.c:43-44` `fsdriver_callvec[call_nr]` | lookup/getdents/read 等 REQ_* 按表分派 |
| L-3 | 读请求进钩子 | `main.c:60-67` `read_hook` → `device.c:142-168` `devman_event_read` / `:173-183` `devman_static_info_read`；缓冲 `buf.c:15-128` | events 两读排空、静态文件多一个换行 |
| L-4 | 非文件消息走钩子 | `libvtreefs/vtreefs.c:67-78` `fs_other`（拷一份再转交）→ `main.c:46-58` `message_hook` | switch **无 break**：ADD 级联执行 DEL+BIND+UNBIND（A-3 修正对象）|
| L-5 | DEVMAN 四类处理 | `device.c:223-277` do_add_device / `:424-440` do_del_device / `bind.c:7-50` do_bind_device / `:56-104` do_unbind_device | ADD 前导 grant 拷贝 `device.c:228-250`；BIND/UNBIND 有 RS-only 门 `bind.c:14/:63` |
| L-6 | 回复原语 | `device.c:213-219` `do_reply`（m_type 改 DEVMAN_REPLY、TRNS_ADD_ID 回填 transid、ipc_send 异步发回）；`fsdriver.c:48-50` 文件面同形 | EPERM 路径写了不回（bind.c:14-16）|

### 1.3 外部链（X-1 ~ X-6，设备生命周期旅程）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| X-1 | 驱动查 devman 端点 | `libdevman/generic.c:188-202` `devman_init`（DS 查名 :193） | 客户端出生 |
| X-2 | 序列化并注册 | `generic.c:24-99` `save_string`/`serialize_dev` → `:102-140` `devman_add_device`（grant + ADD_DEV 消息） | USB 一设备两 ADD：`usb.c:219-274` 设备先行接口随后 |
| X-3 | devman 入树广播 ADD | L-5 的 do_add → `device.c:75-102` `devman_device_add_event` | 事件行 `"ADD <path> 0x%08x"` |
| X-4 | devmand 消费事件、启驱动 | `commands/devmand/main.c:803-872` 行解析 → `:519-560` dev_type → `:633-690` USB id → `:238-296` 匹配 → `:82-233` `minix-service up` | 守护进程不在 boot_image，rc.minix 启动 |
| X-5 | RS publish 触发 BIND | `servers/rs/manager.c:787` `publish_service`（devman 臂 :840-851，ipc_sendrec DEVMAN_BIND） | 驱动进程发布 label → RS 查到 devman_id |
| X-6 | BIND 转发驱动、状态收口 | `bind.c:32/:80` ipc_sendrec 转发 owner → `generic.c:207-275` `devman_handle_msg`/`do_bind` 回 RESULT | 设备进入 BOUND，旅程闭环 |

**X-4 先于 X-5 是硬因果**（devmand 拉起驱动 → 驱动才发布 → RS 才发 BIND），现有文档组内顺序 12(RS)→13(devmand) 与之相反，是本蓝图唯一的顺序级修正（见 §4）。

---

## 2. 知识点全集（知识点池）

列说明：**来源类型** 存量（来自旧文档）/新增（旧文档没有、C 源码或 Rust 代码或制品承载）；**现有位置用旧编号**（`06§2.3` = 旧 doc 06 第 2.3 节），**去向用新编号**（§4 的目录）。锚点凡标 ✓ 者本次实测核对过；类型缩写：概念/机制/数据结构/协议/约束/演进/工程/测试。

### 2.1 旧 00-devm-overview（11 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向（新） |
|---|---|---|---|---|---|---|
| K-001 | devman 定位：文件系统外形的用户态设备管理器 | 概念 | 存量 | 00§1.1/01§1 | main.c:70-92✓、system.conf:422-429✓ | 00§1 |
| K-002 | "设备从哪里被发现"的 OS 通用问题（对照 sysfs/uevent/udev） | 概念 | 存量 | 00§1.1 | OS 理论（不依赖代码） | 00§1 |
| K-003 | 两条主线：启动链 + 设备生命周期旅程 | 概念 | 存量 | 00§1.2 | 本蓝图 §1 序表 | 00§2 |
| K-004 | 四条写作原则 + 边界声明范式 | 工程 | 存量 | 00§1.3/1.4 | — | 00§2 |
| K-005 | 四层语义版图：服务端 1013 / libvtreefs 使用面 1642 / libdevman 603 / 协议面 | 工程 | 存量 | 00§2.1 | wc -l 实测 | 00§3 |
| K-006 | 服务端 27 函数 + 6 静态变量全清单分配 | 工程 | 存量 | 00§2.2 | plan §5 覆盖契约 | 00§3 |
| K-007 | 三处 C 语义陷阱预告（fall-through / 5 未实现消息 / DYNAMIC TODO） | 约束 | 存量 | 00§2.3 | main.c:46-58✓、com.h:848-859✓、device.c:396✓ | 00§3 → 各归属篇 |
| K-008 | ARCH A-1~A-10 总表 | 演进 | 存量 | 00§2.4/99§5 | plan §4 | 00§3（主）、99§5（索引） |
| K-009 | 五条重写 non-negotiable 原则（语义保真/errno 映射/…） | 概念 | 存量 | 00§3.1 | AGENTS.md 项目约束 | 00§4 |
| K-010 | 模块落地顺序与文档顺序同构 | 工程 | 存量 | 00§3.2 | os/servers/devman/src/ 实测 | 00§4（重排为新目录） |
| K-011 | 测试基线数字（78/79） | 测试 | 存量 | 00§5/99§6/README | 过期（实测 86，见 §0.3） | **删除**：状态数字退出正文篇，唯一入口 README 状态页（防再犯原则，见 §3 重复表 R-7） |

### 2.2 旧 01-devm-init-main（15 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向（新） |
|---|---|---|---|---|---|---|
| K-012 | main() 三步：填 hooks / 定 root_stat / 开跑 | 机制 | 存量 | 01§2.1 | main.c:70-92✓ | 01§2 |
| K-013 | init_hook 一次性出生（懒初始化入口） | 机制 | 存量 | 01§2.2 | main.c:36-43✓ | 01§2 |
| K-014 | read_hook 分发器：events 与静态信息两分支 | 机制 | 存量 | 01§2.3 | main.c:60-67✓ | 01§2（细节归新 07） |
| K-015 | message_hook switch 无 break：ADD 级联全部 handler | 约束 | 存量 | 01§2.4 | main.c:46-58✓（A-3） | 01§2 现象记录，裁决主讲述点在新 06§3 |
| K-016 | run_vtreefs：六全局中转 + BUF_SIZE 传入 | 机制 | 存量 | 01§2.5 | vtreefs.c:88✓ | 01§2 |
| K-017 | sef_local_startup：三行注册一行启动 | 机制 | 存量 | 01§2.6 | vtreefs.c:52-65✓ | 01§2 |
| K-018 | mount 触发 init_hook（树在挂载时出生） | 机制 | 存量 | 01§2.7 | mount.c:10-38✓ | 01§2（与 02§1.3 去重，主讲述点 02） |
| K-019 | 启动来源：RS 加载组、不在 boot_image | 概念 | 存量 | 01§2.8 | system.conf:422-429✓ | 01§2（出生面，权限细节新 14） |
| K-020 | Rust 钩子表：13 NULL 槽 → 3 Option 字段 | 演进 | 存量 | 01§3.1 | hooks.rs | 01§3 |
| K-021 | Rust 懒建树守卫：static int first → Option\<DeviceTree\>（DM-P1-3） | 演进 | 存量 | 01§3.2 | server.rs | 01§3 |
| K-022 | Rust RootStat 一次构造 + ServerConfig 显式传参 | 演进 | 存量 | 01§3.3/3.4 | config 模块（A-1 相关） | 01§3 |
| K-023 | Rust SEF 生命周期：三回调 → 枚举 + trait | 演进 | 存量 | 01§3.5 | hooks.rs:113-141（DevmanSef/SefHooks） | 01§3 |
| K-024 | SEF restart 语义：状态随 RS 镜像恢复、init_server 仅 fresh 触发 | 机制 | **新增** | — | hooks.rs:113 注释自认 + edge_todo.md:579（DevmanSef 落地记录） | 01§3（旧文只说"三回调"，未讲 restart 分叉） |
| K-025 | 生产装配状态叙事（"park 循环 / P1-6 未接线"） | 工程 | 存量 | 01§3.4/§4.1 | 已过期：f0e02c70f 起 main.rs 已接线 | **删除**（被 K-155 承接）：状态叙事退出正文篇 |

### 2.3 旧 02-vtreefs-framework（20 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向（新） |
|---|---|---|---|---|---|---|
| K-026 | 伪文件系统样板问题：框架为何存在 | 概念 | 存量 | 02§1.1 | libvtreefs 1642 行复用面 | 02§1 |
| K-027 | 两类请求：文件请求走表、非文件走钩子 | 机制 | 存量 | 02§1.2 | fsdriver.c:43-44✓、vtreefs.c:67-78✓ | 02§1（框架视角）；传输视角在新 03 |
| K-028 | 延迟初始化：树在 mount 时才出生（框架机制面） | 机制 | 存量 | 02§1.3 | mount.c:10-38✓ | 02§1（主讲述点，K-018 归并于此） |
| K-029 | fs_hooks 钩子全表：13 槽、devman 用 3 | 数据结构 | 存量 | 02§2.1 | vtreefs.h:fs_hooks✓ | 02§2 |
| K-030 | inode 池：固定数组 + 空闲表 + 两张哈希表 | 机制 | 存量 | 02§2.2 | inode.c:31-99✓ | 02§2 |
| K-031 | add_inode：7 断言 + PNAME_MAX/NAME_MAX 双限 + indexed purge | 机制 | 存量 | 02§2.3 | inode.c:185-249✓ | 02§2 |
| K-032 | delete_inode：两阶段 + 目录父链保留 | 机制 | 存量 | 02§2.4 | inode.c:544-604✓ | 02§2 |
| K-033 | 读循环 5 步 + 部分结果规则 | 机制 | 存量 | 02§2.5 | file.c:46-103✓ | 02§2 |
| K-034 | 编号规则：ino = 槽位 + 1，0 永远无效 | 约束 | 存量 | 02§2.6 | inode.c:369-375✓ | 02§2 |
| K-035 | lookup 的完整形状 | 机制 | 存量 | 02§2.7 | path.c:9-59✓ | 02§2 |
| K-036 | getdents：顺序是契约、编码是传输 | 约束 | 存量 | 02§2.8 | file.c:195-295✓ | 02§2（编码半移交新 03） |
| K-037 | mount/unmount 两函数 + 一个恒假分支 | 机制 | 存量 | 02§2.9 | mount.c:10-56✓ | 02§2 |
| K-038 | 非文件消息：fs_other 拷一份再转交 | 机制 | 存量 | 02§2.10 | vtreefs.c:67-78✓（旧锚"sef_local_startup（L66，工具生成）"是错的，见 §8.2） | 02§2（进钩子后分派面详述在新 03） |
| K-039 | A-1 落地：框架内联进 devman crate（src/vtreefs/） | 演进 | 存量 | 02§3.1 | os/servers/devman/src/vtreefs/（19 文件实测） | 02§3 |
| K-040 | Rust 树表示：Vec 池 + 空闲栈 + 线性扫描（BTreeMap-free 刻意） | 演进 | 存量 | 02§3.2 | vtreefs/inode.rs:669 行/12 测试 | 02§3 |
| K-041 | assert → Err 映射表（A-7 第二实例） | 演进 | 存量 | 02§3.5 | vtreefs/*.rs | 02§3 |
| K-042 | 未实现槽显式 ENOSYS（对 C 不对称默认的取舍） | 演进 | 存量 | 02§3.6 | vtreefs/mod.rs | 02§3 |
| K-043 | InodeContent：内容挂节点（DM-P1-5：files.rs/cookie/unsafe 退役） | 演进 | 存量 | 02§3.4/06§3.4 | vtreefs/inode.rs；crate 内 unsafe 归零（todo.md §0） | 02§3 |
| K-044 | 信号与退出：got_signal SIGTERM 锁存、unmount 后停机 | 机制 | **新增** | —（旧 02 未展开） | vtreefs.c:39-46✓/:57/:85；hooks.rs:139-141（DevmanSef.terminate） | 02§2（覆盖审计缺口表 GA-1） |
| K-045 | 传输注入测试面：VecTransport（脚本进、回复/发送出） | 测试 | 存量 | 02§3.7 | vtreefs 测试模块 | 03§4（与生产传输对照） |

### 2.4 新 03-fsdriver-transport（拆分 + 新增，8 条；原料见 §6 操作 C-03）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向（新） |
|---|---|---|---|---|---|---|
| K-046 | transid 打包：TRNS_GET_ID/TRNS_DEL_ID/TRNS_ADD_ID（m_type 高低 16 位） | 协议 | **新增** | 02§3.7 一句带过 | fsdriver.c:34-35/:50✓ | 03§2 |
| K-047 | mount 门：未挂载除 ReadSuper 外 EINVAL | 约束 | **新增** | 02§3.7 | fsdriver.c:40-46✓ | 03§2 |
| K-048 | 数据面走 grant：fsdriver_copyout / fsdriver_getname | 机制 | **新增** | 02§3.7 | libvtreefs/file.c:84✓、link.c:36✓；ipc/minix.rs | 03§2（grant 概念首讲在此） |
| K-049 | 请求分类器：raw m_type → is_fs_rq → Request / DevmanMsg 双车道 | 机制 | **新增** | 02§3.7 | ipc/minix.rs:644 行实测；minix-types::ipc/fs_driver.rs（E-REQWIRE 方案 A 裁决，edge_todo.md:433） | 03§3 |
| K-050 | 生产 Transport：MinixTransport（SysKernel 内核面 + safecopyfrom/to） | 演进 | **新增** | 02§3.7（2026-09-19 更新） | ipc/minix.rs；main.rs 装配 | 03§3 |
| K-051 | main 装配闭环：Server::new → run(&mut transport) → 收包失败 panic | 演进 | **新增** | 02§3.7/01§4（park 叙事已过期） | main.rs 实测；失败折回 panic 对照 vtreefs.c:16-33、fsdriver.c:92✓ | 03§3 |
| K-052 | DEVMAN 与文件请求同循环分派（A-3 单分派的传输侧） | 演进 | **新增** | 02§3.7 | ipc/minix.rs + server.rs:628 行 | 03§3 |
| K-053 | 载荷域偏移单一权威：三张 *_req_off 表（VFS 编码器与 FS 解码器共用） | 协议 | **新增** | — | minix-types ipc/fs_driver.rs（edge_todo.md:433 闭单记录） | 03§2 |

### 2.5 旧 03-devm-structs → 新 04（16 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向（新） |
|---|---|---|---|---|---|---|
| K-054 | 设备对象的三重身份（状态主体/目录项/事件参与者） | 概念 | 存量 | 03§1 | devman.h:82-107✓ | 04§1 |
| K-055 | 状态机 UNBOUND/BOUND/ZOMBIE：三个整数一条生命周期 | 数据结构 | 存量 | 03§1.1/2.1 | devman.h:89-91✓ | 04§2 |
| K-056 | devman_device 全字段逐字段 | 数据结构 | 存量 | 03§2.1 | devman.h:82-107✓ | 04§2 |
| K-057 | 文件三件套（entry/event_inode/static_info_inode） | 数据结构 | 存量 | 03§2.2 | devman.h:61-80✓ | 04§2 |
| K-058 | 常量：BUF_SIZE 4097 / DEVMAN_STRING_LEN 128 / ADD_STRING/REMOVE_STRING | 数据结构 | 存量 | 03§2.3 | devman.h:39/:42/:44-45✓ | 04§2（收口总表在 99§1） |
| K-059 | EntryType 枚举 0/1/2 | 数据结构 | 存量 | 03§2.3 | devman.h:47-51✓ | 04§2 |
| K-060 | 同名双生：服务端/客户端两个 devman_dev，布局不同、wire 才是契约 | 概念 | 存量 | 03§2.7 | devman.h:8-20 ≡ devinfo.h✓（旧文小节排在 §2.4 之前，重排修正见 §8.1） | 04§2 |
| K-061 | wire 格式：头 + 条目 + 字符串区（含 subsystem_offset/req_nr 写一次读不到） | 协议 | 存量 | 03§2.4 | devman.h:8-20✓；卫生处理 devman_client.rs:101✓ | 04§2 |
| K-062 | 根设备 BSS 半初始化：零值恰好是对的 | 约束 | 存量 | 03§2.5 | device.c:187-207✓ 读法 | 04§2 |
| K-063 | 死结构 devman_device_file + 死宏 DEVMAN_DEFAULT_MODE（零引用挂名不建模） | 约束 | 存量 | 03§2.6 | devman.h:56-59✓/:41✓ | 04§2 |
| K-064 | Rust 三身份 → 三类型拆分 | 演进 | 存量 | 03§3.1 | structs.rs | 04§3 |
| K-065 | DeviceState 枚举 + 未知值拒绝 | 演进 | 存量 | 03§3.2 | structs.rs | 04§3 |
| K-066 | 字段省略与显式化清单 | 演进 | 存量 | 03§3.3 | structs.rs | 04§3 |
| K-067 | A-4：wire 解析显式边界检查 | 演进 | 存量 | 03§3.4 | wire.rs | 04§3 |
| K-068 | Event::new 长度守卫不静默截断 | 约束 | 存量 | 03§3.5 | structs.rs（答案揭晓在 08§3） | 04§3 |
| K-069 | Rust 类型映射表（C 惯用法 → Rust，自旧 02§3.3 归并） | 演进 | 存量 | 02§3.3 | vtreefs/*.rs、structs.rs | 04§3（数据结构篇集中讲类型映射，旧放框架篇是错位） |

### 2.6 旧 04-device-tree → 新 05（8 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向（新） |
|---|---|---|---|---|---|---|
| K-070 | 两棵树一次出生：设备树 + inode 树的对应关系 | 概念 | 存量 | 04§1 | device.c + inode.c | 05§1 |
| K-071 | 静态区：6 全局 + 2 套默认属性 | 数据结构 | 存量 | 04§2.1 | device.c:16-39✓ | 05§2 |
| K-072 | devman_init_devices：root_dev + devices/ + events/ 出生序列 | 机制 | 存量 | 04§2.2 | device.c:187-207✓（major=-1 :193✓） | 05§2 |
| K-073 | _find_dev：DFS 先序、孩子按序 | 机制 | 存量 | 04§2.3 | device.c:283-308✓ | 05§2 |
| K-074 | generate_path：递归拼串 + 预算检查 | 机制 | 存量 | 04§2.4 | device.c:45-70✓ | 05§2 |
| K-075 | Rust 双树缝合：binding: Option\<Ino\> | 演进 | 存量 | 04§3.1 | device_tree.rs + vtreefs/inode.rs | 05§3 |
| K-076 | alloc_id 顺序分配 + 溢出 ENOMEM（A-5）；id 单调不复用 | 演进 | 存量 | 04§3.3 | device_tree.rs:155✓/:170-174✓ | 05§3 |
| K-077 | insert 归属裁决：linking 归树、校验归业务、线画在 id 上 | 演进 | 存量 | 04§3.4 | device_tree.rs | 05§3 |

### 2.7 旧 05-devm-message-contract → 新 06（12 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向（新） |
|---|---|---|---|---|---|---|
| K-078 | 消息是 devman 唯一的写通道（文件读面是只读投影） | 概念 | 存量 | 05§1 | com.h:846-866 + device.c | 06§1 |
| K-079 | 同词多义：m4 三个字（GRANT_ID/SIZE/ENDPOINT/DEVICE_ID/RESULT）两副面孔 | 协议 | 存量 | 05§1.1/2.1 | com.h:861-866✓（**行号勘误**：旧文多处引 "859-864"） | 06§2 |
| K-080 | DEVMAN 常量块：1 基址 0x1200 + 10 消息 | 协议 | 存量 | 05§2.1 | com.h:846/:848-859✓ | 06§2 |
| K-081 | grant 拷贝三错三码（ENOMEM/EINVAL/断言路径） | 机制 | 存量 | 05§2.2 | device.c:228-250✓ | 06§2（grant 概念首讲在新 03，本篇是协议侧用法） |
| K-082 | 回复原语协议面：改同一条消息、异步发回、残留字清零 | 机制 | 存量 | 05§2.3/3.4 | device.c:213-219✓；message.rs | 06§2（ipc_reply 传输原语在新 03，见重复表 R-3） |
| K-083 | RS-only 门：EPERM 写了但不回 | 约束 | 存量 | 05§2.4 | bind.c:11-19/:60-68✓ | 06§2（与 A-9 呼应，10/14 引用） |
| K-084 | A-3 单分派裁决：match 即证明（fall-through 的 Rust 解） | 演进 | 存量 | 05§2.5/3.2 | dispatch.rs:1-13✓ | 06§3（主讲述点，K-015 归并） |
| K-085 | A-6：5 个未实现消息（ADD_BUS/DEL_BUS/ADD_DEVFILE/DEL_DEVFILE/REQUEST）fail-closed | 约束 | 存量 | 05§2.6/3.x | com.h:848-859✓ 中 5 个零引用 | 06§3 |
| K-086 | 相位表：同词不同命（message.rs 模块头大表） | 数据结构 | 存量 | 05§3.1 | ipc/message.rs | 06§3 |
| K-087 | EPERM 不回用测试锁"无发送"（check_rs） | 测试 | 存量 | 05§3.3/5 | message.rs | 06§5 |
| K-088 | DevmanMsg::classify 类型化载荷（DM-P2-2：word2/3 裸参入类型系统） | 演进 | 存量 | 05§3（部分）| dispatch.rs / server.rs（todo.md DM-P2-2） | 06§3 |
| K-089 | ADD 回复双字 RESULT + DEVICE_ID（DM-P2-1 apply_reply_with_id） | 协议 | 存量 | 05/07 各半 | device.c:213-219 + todo.md Fix#3 | 06§2（协议面），08§4（使用时） |

### 2.8 旧 06-event-buf → 新 07（10 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向（新） |
|---|---|---|---|---|---|---|
| K-090 | 缓冲三游标：skip/left/used | 数据结构 | 存量 | 06§2.1 | buf.c:8-28✓ | 07§2 |
| K-091 | printf 子集 %s/%% 与 append 同一漏斗两种进料 | 机制 | 存量 | 06§2.2/3.2 | buf.c:33-117✓（等价性论证保留） | 07§2 |
| K-092 | 队列方向：头进尾出 | 机制 | 存量 | 06§2.3 | device.c:75-140✓ | 07§2 |
| K-093 | 两读排空：空读才删（EOF 消费语义，A-8） | 约束 | 存量 | 06§2.4 | device.c:142-168✓ | 07§2（devmand 消费侧回看 13） |
| K-094 | 静态信息读：多一个换行 | 机制 | 存量 | 06§2.5 | device.c:173-183✓ | 07§2 |
| K-095 | 事件行格式 "ADD <path> 0x%08x"（含 :84 panic 串名实不符的 C 瑕疵） | 协议 | 存量 | 06§2.3/03§3.5 | device.c:84✓ + devman.h:44-45✓ | 07§2（主讲述点；08/09 生产侧、13 消费侧引用） |
| K-096 | read_hook 分发：内容挂节点私房数据的另一端 | 机制 | 存量 | 06§2.6/3.4 | main.c:64-66✓；inode.rs（DM-P1-5 终态，K-043） | 07§2/03 |
| K-097 | Buf 归 VTreeFs 持有复用（DM-P3-3：读路径无分配、无吞错） | 演进 | 存量 | 06§3.1 | buf.rs + vtreefs/mod.rs（todo.md Fix#8） | 07§3 |
| K-098 | VecDeque 队列前后对应（A-8/A-2） | 演进 | 存量 | 06§3.3 | event_queue.rs | 07§3 |
| K-099 | 旧 §5 测试名 register_resolves_index / dispatch_end_to_end | 测试 | 存量 | 06§5 | **虚构**：指向已退役 files.rs（代码 grep 零命中，todo.md DM-P1-5 退役记录） | **删除**（B 相重写测试节按代码实测） |

### 2.9 旧 07-devm-add-device → 新 08（11 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向（新） |
|---|---|---|---|---|---|---|
| K-100 | 注册 = 认领 + 落户 + 广播（全景） | 概念 | 存量 | 07§1 | device.c:223-277✓ | 08§1 |
| K-101 | 认领三错：grant ENOMEM / EINVAL / 父 ENODEV | 机制 | 存量 | 07§2.1 | device.c:228-256✓ | 08§2 |
| K-102 | devman_dev_add_child 落户七步（id 分配、inode、属性、devman_id 文件） | 机制 | 存量 | 07§2.2 | device.c:345-397✓（next_device_id++ :371✓） | 08§2 |
| K-103 | 条目循环：STATIC 落户、DYNAMIC 静默跳过 + FUTURE TODO | 约束 | 存量 | 07§2.3 | device.c:404-418✓（TODO :396✓，旧引 ":399" 勘误） | 08§2 |
| K-104 | 收尾：state=UNBOUND、owner=ep、ADD 事件、双字回复 | 机制 | 存量 | 07§2.4 | device.c:268-277✓ | 08§2 |
| K-105 | Rust do_add 全流程（两函数合一，边界在传输不在函数） | 演进 | 存量 | 07§3.1 | add_device.rs:36✓ | 08§3 |
| K-106 | 成员账保留：get(parent) 不省略 | 演进 | 存量 | 07§3.2 | add_device.rs | 08§3 |
| K-107 | 不可能分支的类型级删除 | 演进 | 存量 | 07§3.3 | add_device.rs | 08§3 |
| K-108 | 截断两策：静态信息预截断 vs 事件行严拒 | 约束 | 存量 | 07§3.4（03§3.5 的答案） | add_device.rs + event_queue.rs | 08§3 |
| K-109 | 设备名空格拒绝（OQ-3 决议，用户批准的新行为） | 演进 | 存量 | 07§3.5 | add_device.rs（todo.md OQ-3） | 08§3 |
| K-110 | 失败回滚：unwind_staged + rollback_id（DM-P1-1，一次重名堵死此后所有 ADD 的修复） | 演进 | 存量 | 04§4/07§4 | add_device.rs（todo.md Fix#1）；C 对照 device.c:371 先 ++ 后 :373 add_inode 不检查 | 08§3（自旧 04/07 散述集中；事件预算前缀记账 DM-P1-4 同节） |

### 2.10 旧 08-devm-del-device → 新 09（7 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向（新） |
|---|---|---|---|---|---|---|
| K-111 | 删除 = 广播先行、回收随后（全景） | 概念 | 存量 | 08§1 | device.c:424-440✓ | 09§1 |
| K-112 | 入口三步：找得到 → REMOVE 事件 → 改态放引用 | 机制 | 存量 | 08§2.1/2.2 | device.c:441-448✓ | 09§2 |
| K-113 | get/put 计数器四行不对称 | 约束 | 存量 | 08§2.3 | device.c:460-480✓ | 09§2 |
| K-114 | devman_del_device 回收八步 | 机制 | 存量 | 08§2.4 | device.c:485-515✓ | 09§2 |
| K-115 | BOUND→ZOMBIE 单向转换 + REMOVE 先于状态变更 | 约束 | 存量 | 08§2/03 | device.c:441-448✓ | 09§2 |
| K-116 | Rust 回收跨四文件缝合、墓碑 vs 摘除、引用配对表、计数显式化 | 演进 | 存量 | 08§3/4 | del_device.rs + device_tree.rs + inode.rs | 09§3/04 |
| K-117 | 旧 §3.1 对"06 unregister"的引用 | 工程 | 存量 | 08§3.1 | **过期**：files.rs unregister 已随 DM-P1-5 退役 | **删除**（改写为 InodeContent 终态描述） |

### 2.11 旧 09-devm-bind-unbind → 新 10（8 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向（新） |
|---|---|---|---|---|---|---|
| K-118 | 绑定是三方握手（RS/devman/驱动）不是赋值 + 绑定段路径图 | 概念 | 存量 | 09§1/1.2 | bind.c + manager.c:840✓ | 10§1（次主线收官段） |
| K-119 | do_bind 四步：门→找→转发→收尾 | 机制 | 存量 | 09§2.1 | bind.c:7-50✓（ipc_sendrec :32✓） | 10§2 |
| K-120 | do_unbind 五步 + 两处不对称（owner=ANY、ENODEV=19 容错） | 机制 | 存量 | 09§2.2 | bind.c:56-104✓（!=19 :85✓） | 10§2 |
| K-121 | 转发语义：原样转交、endpoint 字 RS 预填 | 协议 | 存量 | 09§2.3 | bind.c:48/:102✓ | 10§2 |
| K-122 | 驱动回复 RESULT → BOUND + refcount get（状态收口） | 机制 | 存量 | 09§2 | bind.c:34-49✓ | 10§2（客户端响应半归 11） |
| K-123 | Rust 两段式 handler：请求半 + 应答半；应答错误即原文 | 演进 | 存量 | 09§3.1/3.2 | bind.rs | 10§3 |
| K-124 | 旧 §3.3 "P1-6 单例未建 / 装配待定"状态叙事 | 工程 | 存量 | 09§3.3 | 过期（f0e02c70f） | **删除**（Server 状态机机制面保留、状态叙事退出；K-050/K-051 承接） |
| K-125 | 引用配对表右半边（BIND/UNBIND 两笔账） | 约束 | 存量 | 09§4.2 | bind.rs/del_device.rs | 10§4 |

### 2.12 旧 10-libdevman-client → 新 11（9 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向（新） |
|---|---|---|---|---|---|---|
| K-126 | 客户端是服务端的镜像不是附庸（为什么驱动侧要一层库） | 概念 | 存量 | 10§1 | generic.c | 11§1 |
| K-127 | 客户端本地状态：devman_ep + 设备链表 | 数据结构 | 存量 | 10§2.1 | generic.c:14-21✓ + local.h（DEV_NAME_LEN 32 :7✓） | 11§2 |
| K-128 | save_string + serialize_dev 编码 | 机制 | 存量 | 10§2.2 | generic.c:24-99✓ | 11§2 |
| K-129 | 收发失败即 panic（**实测 8 处**：:110/:125/:129/:134/:165/:169/:174/:196；旧文"三 panic 套餐/五处"计数勘误） | 约束 | 存量 | 10§1/2.3 | generic.c✓ | 11§2 |
| K-130 | devman_init：DS 查名 + 表清零 | 机制 | 存量 | 10§2.4 | generic.c:188-202✓（DS :193✓） | 11§2 |
| K-131 | 响应面：do_bind/do_unbind + devman_handle_msg | 机制 | 存量 | 10§2.5 | generic.c:207-275✓ | 11§2 |
| K-132 | ENODEV 不可分辨语义（删了还是没见过） | 约束 | 存量 | 10§2.3 | generic.c → devman_client.rs:224✓ | 11§2 |
| K-133 | Rust：ClientError 三变体（A-7）/ ClientTransport 注入 / 回调签名 (dev_id, ep) / 本地表调用方 Vec | 演进 | 存量 | 10§3 | devman_client.rs:507 行/8 测试实测 | 11§3 |
| K-134 | 客户端超集的 fail-closed 纪律（Rust 不照抄 panic 但语义映射 errno） | 演进 | 存量 | 10§3.1 | devman_client.rs | 11§3 |

### 2.13 旧 11-usb-device-model → 新 12（8 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向（新） |
|---|---|---|---|---|---|---|
| K-135 | 一设备多接口所以两次 ADD | 概念 | 存量 | 11§1 | usb.c:219-274✓ | 12§1 |
| K-136 | 属性拼法：5 + 6 + 条件串 + dev_type | 机制 | 存量 | 11§2.1 | usb.c:24-141✓ | 12§2 |
| K-137 | devman_usb_dev / 接口结构全字段 | 数据结构 | 存量 | 11§2.2 | devman.h:36-55✓（lib 侧） | 12§2 |
| K-138 | 两次 ADD：设备先行、接口挂父 | 机制 | 存量 | 11§2.3 | usb.c:219-274✓ | 12§2 |
| K-139 | 双回调：全局注册 + 缺席 ENODEV | 机制 | 存量 | 11§2.4 | usb.c:19-20/:280-301✓ | 12§2 |
| K-140 | remove 与 delete 分家（摘接口 ≠ 删设备） | 约束 | 存量 | 11§2.5 | usb.c:276-291✓ vs :174-197✓ | 12§2 |
| K-141 | Rust：描述符已解码假设（与 minix-usb 分界）/ UsbStack 回调注册表 / 定长数组→Vec / delete=drop | 演进 | 存量 | 11§3 | usb_model.rs:437 行/5 测试实测 | 12§3 |
| K-142 | USB 属性拼写字节级锁定测试 | 测试 | 存量 | 11§5 | usb_model.rs:301✓ | 12§5 |

### 2.14 旧 12-rs-integration → 新 14（7 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向（新） |
|---|---|---|---|---|---|---|
| K-143 | RS = 出生证明与婚姻介绍所（加载者 + BIND 发起者双重身份） | 概念 | 存量 | 12§1 | manager.c + system.conf | 14§1 |
| K-144 | publish 握手：查 ds → 填字 → ipc_sendrec → 不成即杀 | 机制 | 存量 | 12§2.1 | manager.c:787 publish_service✓、devman 臂 :840-851✓（旧锚"rproc（L840，工具生成）"符号名错误，§8.2） | 14§2 |
| K-145 | unpublish 握手：同形不同命 | 机制 | 存量 | 12§2.2 | manager.c:864/:897-909✓ | 14§2 |
| K-146 | 继承：init_slot 抄 devman_id | 机制 | 存量 | 12§2.3 | manager.c:1742✓ | 14§2 |
| K-147 | 出生权限：uid 0 + VM SETCACHEPAGE/CLEARCACHE 的语义 | 约束 | 存量 | 12§2.4 | system.conf:422-429✓ | 14§2 |
| K-148 | PublishOutcome/UnpublishOutcome 决策枚举 + RsTransport（devman 视角的 RS） | 演进 | 存量 | 12§3 | rs_contract.rs:24-26✓ | 14§3 |
| K-149 | RS 侧生产臂现状（publish.rs 只有决策半，通电挂 E-DMWIRE 第 4 缺） | 工程 | 存量 | 12§3/README | edge_todo.md:712 | 14§3（**带 edge 指针的状态例外**：属跨 stage 契约未决面，留正文但标注日期） |

### 2.15 旧 13-devmand-consumer → 新 13（12 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向（新） |
|---|---|---|---|---|---|---|
| K-150 | devmand 是事件行的第一个读者（契约抽取篇定位） | 概念 | 存量 | 13§1 | commands/devmand/main.c | 13§1 |
| K-151 | sscanf 双格式行解析（ADD/REMOVE） | 机制 | 存量 | 13§2.1 | main.c:803-872✓（:815/:844） | 13§2 |
| K-152 | 类型判定：读 dev_type 文件（DEVMAN_TYPE_NAME "dev_type"） | 机制 | 存量 | 13§2.2 | main.c:519-560✓、:16 | 13§2 |
| K-153 | 分流：USB_DEV 忽略、USB_INTF 干活 | 机制 | 存量 | 13§2.3 | main.c:821-835✓ | 13§2 |
| K-154 | generate_usb_device_id：8 次属性读、设备段为零 | 机制 | 存量 | 13§2.4 | main.c:633-690✓ | 13§2 |
| K-155 | 匹配 9 标志 + DEVICE_PROTOCOL 复制粘贴 bug（重复检查） | 约束 | 存量 | 13§2.5 | main.c:238-296✓、**实测重复检查在 :250+:254**（旧引 :247-248，§8.2）；usb_driver.h | 13§2 |
| K-156 | usb.y DSL 配置语法与 etc/devmand/*.cfg | 接口 | 存量 | 13§2.6 | usb.y + usb_scan.l + cfg 样例 | 13§2（文法深度收窄，见越界表 OA-2） |
| K-157 | 启停契约：minix-service up <binary> -major -devid -label + mknod 脚本 | 接口 | 存量 | 13§2.7 | main.c:82-233✓ | 13§2（"up 之后发生什么"前指新 14，标支线交接点） |
| K-158 | major 位图 16×8 分配（EEXIST） | 机制 | 存量 | 13§2.8 | main.c:596-631✓、EEXIST :579（旧引 :585）、memset :888（旧引 :883），§8.2 | 13§2 |
| K-159 | 主循环与杂项（事件读循环、fork 出的子进程管理） | 机制 | 存量 | 13§2.9 | main.c:876-942/:418-430✓（942 行实测） | 13§2 |
| K-160 | 契约篇决策：不实现、"devman 侧如何满足"对照表 | 工程 | 存量 | 13§3/§4 | plan 边界 + todo.md §1.2 | 13§3/04 |
| K-161 | devmand 消费者外部性确认（本 stage 只对其输出契约负责） | 边界 | 存量 | 13§1.1 | todo.md §1.2 末条 | 13§1.1 |

### 2.16 旧 99-devm-global-concepts → 新 99（5 条）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向（新） |
|---|---|---|---|---|---|---|
| K-162 | 常量总表（定义→Rust 落点→归属篇，一行一证据） | 数据结构 | 存量 | 99§1 | 99§1 各行（com.h 行号勘误同 §8.2） | 99§1 |
| K-163 | 错误码总表（EPERM 不回复 / ENODEV 19 容错 / …） | 数据结构 | 存量 | 99§2 | 各生产篇锚点 | 99§2 |
| K-164 | 跨服务引用表（谁跟谁说话、载体、篇目） | 概念 | 存量 | 99§3 | 99§3 各行 | 99§3（编号列改新目录） |
| K-165 | 全局状态表：C 静态 → Rust 归属 | 数据结构 | 存量 | 99§4 | 各模块 | 99§4 |
| K-166 | 测试基线终态表（78/13/139） | 测试 | 存量 | 99§6 | 过期（§0.3） | **删除**（同 K-011，状态数字唯一入口 README/todo） |

### 2.17 覆盖审计新增条目（已并入上文 K-024、K-044~K-053、K-167~K-168）

| 编号 | 名称 | 类型 | 来源 | 锚点 | 去向（新） |
|---|---|---|---|---|---|
| K-167 | 宿主测试基建：wire::testutil::serialize 单点、CannedTransport/VecTransport 回放 | 测试 | **新增** | todo.md DM-P3-2（Fix#10）；ipc/minix.rs 6 测试 | 03§4 + 99§6′（README 口径说明） |
| K-168 | 与邻接框架的对比：libfsdriver（C 通用）↔ 内联 vtreefs（Rust）↔ 共享 minix-vtreefs stub（A-1 三分） | 演进 | **新增** | vtreefs/mod.rs:50-56 自注（edge_todo.md:435） | 02§3 |

### 2.18 统计摘要

- 总条数 **168**：存量 155、新增 13（K-024、K-044~K-053、K-167、K-168）。
- 按类型：概念 14 / 机制 58 / 数据结构 22 / 协议 11 / 约束 21 / 演进 28 / 工程 8 / 测试 6。
- 明确删除 5 条（K-011、K-025、K-099、K-117、K-124、K-166 中 K-011/K-166 同理由并计）——全部为**状态叙事或虚构项**，机制内容零删除。
- 按旧文档分布：00→11、01→14、02→21、03→16、04→8、05→12、06→10、07→11、08→7、09→8、10→9、11→8、12→7、13→12、99→5。

---

## 3. 覆盖审计

### 3.1 主题全集与来源

四路对账：① C 符号全集 = devman 服务端 4 .c + devman.h/devinfo.h + com.h DEVMAN 块 + libvtreefs 使用面 + **libfsdriver/fsdriver.c**（旧目录只引用未成篇，见缺口 GA-2）+ libdevman 2 .c + local.h + rs/manager.c devman 臂 + devmand main.c/usb.y + system.conf；② OS 通用概念 = 设备发现与热插拔（udev/sysfs/uevent 对照）、伪文件系统、IPC 请求-回复协议、引用计数生命周期、事件驱动消费者；③ 非 C 制品 = plan §4 ARCH 表、todo.md DM-P1-1~P3-3 演进记录、edge_todo E-DMWIRE/E-REQWIRE/E-DSWIRE/E5(h)、Rust 实测（ipc/minix.rs、DevmanSef）；④ 边界契约 = plan §2 表"不覆盖"列 + master-plan 行 29。

### 3.2 覆盖缺口表

| # | 缺口主题 | 证据 | 判定与建议 |
|---|---|---|---|
| GA-1 | 信号与退出路径（got_signal → 停机旗 → unmount → 退出） | vtreefs.c:39-46✓/:57/:85；hooks.rs:139-141；旧 15 篇无一处展开 | 并入新 02（框架 C 面），Rust 落点 DevmanSef.terminate；非 C 清单"关闭与退出"由此有归属 |
| GA-2 | libfsdriver 分派协议整层（transid 打包、mount 门、grant 数据面、分类器、生产 Transport、main 装配） | fsdriver.c:34-50/:88-96✓；ipc/minix.rs 644 行；旧文只有 02§3.7 两段 + 05§2.3 半段 | **新建篇章**（新 03），K-046~K-053 落实 |
| GA-3 | SEF restart 与 fresh 分叉（状态随 RS 镜像恢复、init_server 仅 fresh） | hooks.rs:113-128✓；edge_todo.md:579 | 并入新 01（K-024） |
| GA-4 | 测试基建叙事（注入 seam、CannedTransport、testutil 单点） | todo.md DM-P3-2；ipc/minix.rs 测试 6 个 | 并入新 03 §测试节 + 99 收口说明（K-167）；不单开篇（规模不到） |
| GA-5 | 各篇 Rust 状态叙事整体过期（park/单例未建/78 测试） | §0.3 实测 86 vs 各篇 78/79；f0e02c70f | 不是缺内容而是**错内容**：B 相按新 03 重写 + 状态叙事退出原则处理（操作 C-06/C-07） |

### 3.3 重复主题表

| # | 主题 | 出现处 | 主讲述点（新） | 其余处理 |
|---|---|---|---|---|
| R-1 | message_hook fall-through 与 A-3 | 01§2.4、05§2.5 | 新 06§3（协议裁决面） | 新 01 留一行现象记录指向 06 |
| R-2 | 懒建树（mount 触发 / Option\<DeviceTree\> 守卫） | 01§2.7、02§1.3、04§1 | 新 02§1（框架机制） | 01 讲调用点、05 讲被触发内容 |
| R-3 | 回复原语（do_reply vs ipc_reply） | 05§2.3、02§3.7 | 协议语义新 06§2；传输原语新 03§2 | 互引一行 |
| R-4 | 事件行格式 | 06§2.3、07§2.4、08§2.2、13§2.1 | 新 07§2（格式定义者） | 08/09 生产者、13 消费者只引用 |
| R-5 | ARCH 总表 | 00§2.4、99§5、plan §4 | 新 00§3（文档内总表）、plan（权威源） | 99 保留"葬在哪"索引行 |
| R-6 | 常量定义 | 03§2.3、05§2.1、99§1 | 首讲各归其篇（04/06），收口 99§1 | 正文不重复整表 |
| R-7 | 测试基线数字 | 00§5、99§6、README、todo | **README 状态页（唯一）** | 正文篇一律不带状态数字（防再犯） |
| R-8 | devman_id 文件 | 07§2.2、12§2.1 | 新 08§2（生产者） | 新 14 引用 |
| R-9 | RS-only 门 | 05§2.4、09§2.1 | 新 06§2（协议约束） | 新 10 使用处引用 |

### 3.4 越界主题表

| # | 越界 | 现位置 | 正确归属 |
|---|---|---|---|
| OA-1 | 状态叙事（"本轮修正/P1-6 待定/review 中纠正/OQ-3 用户批准"过程话术）散布正文 | 01§3.4、02§4.4、06§3.4、07§3.2/3.5、09§3.3 等 | 机制结论留下（改写为终态陈述），过程叙事退出到 todo.md；主线支线分离原则 |
| OA-2 | usb.y DSL 文法细节 | 13§2.6 | 属 18-stage-commands 的深水区；新 13 只保留 devmand 匹配输入输出契约 + 指向 18-stage-commands/04-device-database.md |
| OA-3 | RS 内部（init_slot 全流程、ds 内部机制） | 12§2.1/2.3 边缘 | 保持现深度（只讲 devman 臂），越界部分一行指向 03-stage-rs/07-stage-ds |
| OA-4 | `.design/`、scan.md 等隐藏中间产物引用 | 01§3.4/§4.1、04§2.4 | 违反项目隐藏文件夹约定（AGENTS.md）：B 相一律改引 doc/代码/C 源绝对路径 |

### 3.5 非 C 主题逐项显式回答

| 主题 | 在哪里讲 / 为什么不在本 stage |
|---|---|
| 链接与加载 | 新 01§2（RS 加载组出身，K-019）+ 新 14§2（system.conf 权限，K-147）；ELF 装载机制本身属 01-stage-kernel/03-stage-rs |
| 镜像与内存布局 | 不适用（用户态普通 ELF，无自定义 linker script；kernel 阶段的 per-process memory map 属 02-stage-vm）——明确排除 |
| 汇编入口与陷阱进入 | E1 trap 桥属 kernel stage + edge_todo；新 03 只讲消费面（SysKernel/KernelIpc 接口），机制指针给 edge E1 |
| 启动装配 | 新 01（C 侧 main+SEF）+ 新 03§3（Rust main.rs 装配，K-051） |
| 构建与工具链 | 不在本 stage（cargo workspace 通用约定属 14-stage-runtime/工具文档）；新 00 只给模块-文档映射 |
| 跨模块接口与线格式 | 新 03（REQ wire）、新 04（devman wire）、新 06（DEVMAN 消息）、新 14（RS 握手）——四处均有锚 |
| 错误路径 | 各篇 C 分析含 errno 路径；99§2 错误码总表收口；fail-closed 纪律新 06§3 |
| 关闭与退出 | 新 02（GA-1：SIGTERM/unmount/停机旗）+ 新 14（unpublish 清理） |
| 并发与同步 | 新 00§4（单线程事件循环原则，!Send/RefCell 合法性来源）；无跨线程状态，不单开篇 |
| 测试基建 | 新 03§4（seam 双实现：VecTransport/CannedTransport）+ K-167；数字状态在 README |

---

## 4. 新目录

### 4.1 新篇章总表（16 篇正文 + README 状态页）

| 新 | 文件名 | 一句话定位 | 分组 | 与旧关系 |
|---|---|---|---|---|
| 00 | 00-devm-overview.md | 版图、两主线、导航与 ARCH 总表 | 总览 | 00 改写（导航重排、数字退出） |
| 01 | 01-devm-init-main.md | devman 的出生：main、三钩子、SEF 生命周期 | 启动与框架 | 01（去状态叙事、补 SEF restart） |
| 02 | 02-vtreefs-framework.md | 伪文件系统框架契约：钩子表、inode 池、读写路径、信号退出 | 启动与框架 | 02 拆分保留（框架面） |
| 03 | 03-fsdriver-transport.md | 字节怎么走：fsdriver 协议、分类器、生产 Transport、装配 | 启动与框架 | **新建**（旧 02§3.7 + 旧 05 半段 + C/Rust 实测） |
| 04 | 04-devm-structs.md | 设备对象的三重身份与 wire 契约 | 核心数据结构 | 旧 03 平移 |
| 05 | 05-device-tree.md | 两棵树一次出生：init_devices、DFS、寻路、id 分配 | 核心数据结构 | 旧 04 平移 |
| 06 | 06-devm-message-contract.md | DEVMAN 消息面：字段复用、门、回复、A-3/A-6 裁决 | 接口面 | 旧 05 平移 |
| 07 | 07-event-buf.md | 事件与读路径：buf、队列、两读排空、事件行格式 | 接口面 | 旧 06 平移 |
| 08 | 08-devm-add-device.md | 注册：认领 + 落户 + 广播（含失败回滚） | 生命周期 handlers | 旧 07 平移 |
| 09 | 09-devm-del-device.md | 删除：广播先行、回收随后、引用级联 | 生命周期 handlers | 旧 08 平移 |
| 10 | 10-devm-bind-unbind.md | 绑定：三方握手与转发语义 | 生命周期 handlers | 旧 09 平移 |
| 11 | 11-libdevman-client.md | 驱动侧客户端：编码、收发、响应面 | 客户端与外部 | 旧 10 平移 |
| 12 | 12-usb-device-model.md | USB 建模：一设备两 ADD、属性拼法、remove/delete 分家 | 客户端与外部 | 旧 11 平移 |
| 13 | 13-devmand-consumer.md | 事件消费者：解析、匹配、启停契约 | 客户端与外部 | 旧 13（编号不变，位置前移于 RS） |
| 14 | 14-rs-integration.md | RS 侧：publish/unpublish/inherit 握手与出生权限 | 客户端与外部 | 旧 12（后移，符合 X-4→X-5 因果） |
| 99 | 99-devm-global-concepts.md | 全 stage 常量/错误码/跨服务/全局状态收口 | 收口 | 99（数字表退出、编号列更新） |

### 4.2 阅读路径

- **主线（启动段一口气读）**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07，对应 §1.1/§1.2 序表 S-1~L-6。
- **次主线（设备旅程）**：08（ADD）→ 07 回看（事件入队）→ 13（消费）→ 14（publish 触发）→ 10（BIND）；路径图主图仍在 00§2、绑定段在 10§1。
- **支线（可跳读）**：11/12 客户端组（只需 04/06 前置）、13/14 外部组；99 纯查阅。
- **并行主题分组与汇聚点**：文件请求面（03/07）与消息请求面（06/08/09/10）在分类器（03§2）汇聚，触发时机不同（VFS 读 vs 驱动 IPC）；服务端（04-10）与客户端（11/12）在 wire 契约（04§2）汇聚。

### 4.3 为什么不是"不动编号"

旧目录在四个硬判据下只有一处顺序违例（RS/devmand 组内因果倒置）与一处语义混杂（旧 02 把框架契约与生产传输两个语义体挤在一篇，且后者是 644 行已落地代码 + 一整个 C 协议层）。修这两处即触发 03 之后的 +1 平移——断链成本 §8.4 实测约 208 处目录内 + 26 处代码注释 + 10 个外部文件，全部机械可 sed。若用户裁决"宁不动编号"，备选方案 B：**02 不拆、12/13 不换**，把新 03 的内容并进旧 02（约 420 行单篇）并接受顺序违例——两案知识点完全等值，仅编号布局不同。默认推荐主方案（拆分 + 换序），理由：单篇单语义是硬规则，执行与因果序优先于编辑惯性。

---

## 5. 每篇契约

契约格式：定位 / 讲什么（K 编号）/ 不讲什么（去向）/ 前置 / 后置 / 事实底线 / 知识点清单 / 验收标准。清单中"来源"列：存量填旧位置，新增填证据锚点。

### 00-devm-overview

- **一句话定位**：进目录的第一站——devman 在系统里是什么、本 stage 两条主线怎么走、16 篇各管什么。
- **讲什么**：K-001 K-002 K-003 K-004 K-005 K-006 K-007 K-008 K-009 K-010。
- **不讲什么**：任何机制细节（→01~14）；任何状态数字（→README）；ARCH 明细（→plan §4，本篇只总表）。
- **前置**：无。**后置**：全部。
- **事实底线**：`servers/devman/`（1013 行）、`libvtreefs/` 使用面 1642 行、`libdevman/` 603 行、`com.h:846-866`、`system.conf:422-429`；plan §2/§4/§5；§1 真序表。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-001 | 定位：文件系统外形的服务器 | 概念 | main.c:70-92 | 版图首答 | 00§1.1 |
| K-002 | 设备发现的 OS 老问题 | 概念 | sysfs/uevent 对照 | 动机层 | 00§1.1 |
| K-003 | 两条主线 | 概念 | 本文 §1 | 导航骨架 | 00§1.2 |
| K-004 | 写作原则+边界范式 | 工程 | — | 读者契约 | 00§1.3 |
| K-005 | 四层语义版图 | 工程 | wc 实测 | 范围声明 | 00§2.1 |
| K-006 | 27 函数+6 静态分配表 | 工程 | plan §5 | 覆盖承诺 | 00§2.2 |
| K-007 | 三陷阱预告 | 约束 | main.c:46-58 等 | 防误导 | 00§2.3 |
| K-008 | ARCH A-1~A-10 总表 | 演进 | plan §4 | 索引 | 00§2.4 |
| K-009 | 五条非协商原则 | 概念 | AGENTS.md | 重写纪律 | 00§3.1 |
| K-010 | 模块-文档同构映射 | 工程 | src/ 实测 | 导航 | 00§3.2（按新目录重绘） |

- **验收标准**：读者能不看任何后续篇就回答"devman 跟谁说话、说几类话、代码在哪、文档去哪找"；导航表前置列与 §9.1 前向扫描一致（零前向）；正文出现"passed/未接线"类状态词 0 次。

### 01-devm-init-main

- **一句话定位**：进程的出生——main 填什么、钩子何时被调、SEF 生命周期（fresh/restart/信号）如何接管。
- **讲什么**：K-012~K-024（K-025 已删除）。
- **不讲什么**：VTreeFS 内部（→02）；字节传输与装配终态（→03）；init_devices 内部（→05）；EPERM 门语义（→06）。
- **前置**：00。**后置**：02、03、05、06。
- **事实底线**：`main.c:36-43/:46-58/:60-67/:70-92`；`vtreefs.c:16-33/:39-46/:52-65/:88`；`mount.c:10-38`；`system.conf:422-429`；`hooks.rs:113-141`（DevmanSef）。
- **知识点清单**：K-012~K-024 逐条按 §2.2 表（锚点同）；"为什么归本篇"统一为：位于 S-2~S-6 启动时序的 devman 侧代码。
- **验收标准**：能默画 main→run_vtreefs→sef_local_startup→mount→init_hook 链并标锚点；restart 分叉有 hooks.rs 证据；无 park/待接线字样。

### 02-vtreefs-framework

- **一句话定位**：框架契约——devman 借用 VTreeFS 时看到什么（钩子表、inode 池、读写路径、mount/信号），Rust 内联版（A-1）怎么落。
- **讲什么**：K-026~K-044、K-168。
- **不讲什么**：生产传输/transid/grant 数据面（→03）；DEVMAN 消息语义（→06）；事件行格式（→07）。
- **前置**：01。**后置**：03、05、07。
- **事实底线**：`vtreefs.h:fs_hooks`；`inode.c:31-99/:185-249/:369-375/:544-604`；`file.c:46-103/:195-295`；`path.c:9-59`；`mount.c:10-56`；`vtreefs.c:39-46/:67-78`；Rust `src/vtreefs/`（inode.rs 669 行/12 测试）。
- **知识点清单**：K-026~K-044、K-168（锚点同 §2.3；K-038 用修正锚 fs_other :67-78；K-044 新增）。
- **验收标准**：钩子 13 槽表与 devman 用 3 槽的差集解释"未实现槽 ENOSYS"；ino=槽+1 不变量有反例测试对照；退出路径（K-044）成节且有 C/Rust 双锚。

### 03-fsdriver-transport（新建）

- **一句话定位**：字节怎么走——libfsdriver 分派协议、请求分类器、MinixTransport 生产实现与 main 装配，测试 seam 与生产同形。
- **讲什么**：K-046~K-053、K-045、K-167。
- **不讲什么**：框架 inode 内部（→02）；DEVMAN 字段复用与门（→06）；grant 表机制本体（→内核 stage，本篇只讲用户侧用法）。
- **前置**：01、02。**后置**：06、07、13、14（凡涉及"消息怎么进出"都回引）。
- **事实底线**：`minix3/minix/lib/libfsdriver/fsdriver.c:34-50/:88-96`；`libvtreefs/file.c:84`、`link.c:36`；`minix-types::ipc/fs_driver.rs`（REQ_*/FS_BASE/NREQS 单权威）；`os/servers/devman/src/ipc/minix.rs`（644 行/6 测试）；`main.rs`（装配实录）；提交 `f0e02c70f`。
- **知识点清单**：

| 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-046 | transid 打包三宏 | 协议 | fsdriver.c:34-35/:50 | 传输层定义 | 新增（旧 02§3.7） |
| K-047 | mount 门 EINVAL | 约束 | fsdriver.c:40-46 | 同上 | 新增（旧 02§3.7） |
| K-048 | grant 数据面 copyout/getname | 机制 | file.c:84、link.c:36、ipc/minix.rs | 同上 | 新增（旧 02§3.7） |
| K-049 | 分类器双车道 is_fs_rq/DevmanMsg | 机制 | ipc/minix.rs、fs_driver.rs | 循环入口 L-1 | 新增 |
| K-050 | MinixTransport（SysKernel+safecopy） | 演进 | ipc/minix.rs | 生产件本体 | 新增（旧 02§3.7 2026-09-19 版） |
| K-051 | main 装配闭环与失败 panic | 演进 | main.rs、vtreefs.c:16-33、fsdriver.c:92 | 出生收口 | 新增（替换旧 01§4 过期叙事） |
| K-052 | DEVMAN 与文件请求同循环（A-3 传输侧） | 演进 | ipc/minix.rs+server.rs | 汇聚点 | 新增 |
| K-053 | *_req_off 偏移表单权威 | 协议 | minix-types fs_driver.rs | 防错位契约 | 新增 |
| K-045 | VecTransport 测试注入 | 测试 | vtreefs 测试 | 与生产对照 | 旧 02§3.7 |
| K-167 | testutil/CannedTransport 基建 | 测试 | todo.md Fix#10 | 同上 | 新增 |

- **验收标准**：能画出"raw 消息 → 分类 → 两车道 handler → 回复回填 transid"全链并逐步带锚；给出 VecTransport 与 MinixTransport 的同形对照表；明确声明对 E-REQWIRE/E1 的消费边界（引用 edge_todo 行号）。

### 04-devm-structs

- **一句话定位**：设备对象的三重身份与两副同名 struct——wire 格式才是真契约。
- **讲什么**：K-054~K-069。
- **不讲什么**：树操作（→05）；消息字段复用（→06）；事件格式（→07）。
- **前置**：00、01（对象随出生出现）。**后置**：05~14。
- **事实底线**：`devman.h:8-20/:39-51/:56-59/:61-107`；`devinfo.h`；`device.c:187-207`（BSS 读法）；Rust structs.rs/wire.rs。
- **知识点清单**：K-054~K-068 按 §2.5（旧 03）；K-069（类型映射表自旧 02§3.3 归并——数据结构概念集中讲）。小节顺序修正：K-060（同名双生）恢复在 K-061（wire）之后的正常位置（旧 §2.7 插在 §2.4 前是编号乱序，§8.1）。
- **验收标准**：devman_device 逐字段表 100% 覆盖 devman.h:82-107；死结构/死宏给出"零引用"grep 证据；wire 头+条目+字符串区三段的字节图与 serialize_dev 字节一致。

### 05-device-tree

- **一句话定位**：两棵树一张地图——root_dev 出生、DFS 查找、递归寻路、id 单调分配。
- **讲什么**：K-070~K-077。
- **不讲什么**：add/del 流程（→08/09）；inode 池内部（→02）；id 回滚场景（→08§3）。
- **前置**：02、04。**后置**：07、08、09。
- **事实底线**：`device.c:16-39/:45-70/:187-207/:283-308`；Rust device_tree.rs（:155/:170-174）、vtreefs/inode.rs。
- **知识点清单**：K-070~K-077 按 §2.6。
- **验收标准**：能复述 DFS 先序对"孩子按序"的依赖及 getdents 契约（K-036）联系；alloc_id 溢出 ENOMEM 有 A-5 标注；双树缝合图（设备树×inode 树，binding 字段）可独立看懂。

### 06-devm-message-contract

- **一句话定位**：协议语义——m4 三个字的两副面孔、三类门与回复纪律、A-3 单分派与 A-6 fail-closed 裁决。
- **讲什么**：K-078~K-089。
- **不讲什么**：ipc_reply 传输原语（→03）；BIND 转发流程（→10）；ADD 落户（→08）。
- **前置**：03、04、05。**后置**：08、09、10、11、14。
- **事实底线**：`com.h:61/:846-866`（字段宏实测 :861-866，勘误见 §8.2）；`device.c:213-219/:228-250`；`bind.c:11-19/:60-68`；`main.c:46-58`；Rust message.rs/dispatch.rs。
- **知识点清单**：K-078~K-089 按 §2.7。
- **验收标准**：五字段宏在请求/回复两相位的使用表无空格；EPERM 不回有"无发送"测试证据（K-087）；10 消息 × 实现状态 × A-6 处置三列全。

### 07-event-buf

- **一句话定位**：读机制与事件队列——buf 三游标、两读排空、事件行格式的出处。
- **讲什么**：K-090~K-098。
- **不讲什么**：事件生产者（→08/09）；消费者（→13）；read_hook 之外的钩子（→01/02）。
- **前置**：02、03、04、05。**后置**：08、09、13。
- **事实底线**：`buf.c:8-28/:33-117`；`device.c:75-140/:142-168/:173-183`；`devman.h:39/:44-45`；Rust buf.rs/event_queue.rs。
- **知识点清单**：K-090~K-098；旧 §5 虚构测试清单（K-099）删除，B 相按 event_queue.rs/buf.rs 实测重写。
- **验收标准**：两读排空能用时序图讲出"第一次读给出事件、第二次空读才删"及 ENOMEM 不再被吞成 EOF 的 A-8 语义；事件行格式的字节图与 C :84 panic 串名实不符的注记并存。

### 08-devm-add-device

- **一句话定位**：一次注册的完整旅程：认领三错 → 落户七步 → 条目循环 → 收尾四连（含失败回滚）。
- **讲什么**：K-100~K-110。
- **不讲什么**：消息面字段（→06）；DEL 级联（→09）；客户端编码（→11）。
- **前置**：04、05、06、07。**后置**：09、13、14。
- **事实底线**：`device.c:223-277/:345-418`（:371/:396）；Rust add_device.rs（unwind_staged/rollback_id、空格拒绝、预算前缀）。
- **知识点清单**：K-100~K-110 按 §2.9。
- **验收标准**：能列出 ADD 失败的全部回滚点（id/目录 inode/属性条目）与 DM-P1-1 修复对照；成员引用两笔账（device.c:365/:394）在引用配对表左半边可追。

### 09-devm-del-device

- **一句话定位**：删除的不对称：广播先行、回收随后、引用计数级联。
- **讲什么**：K-111~K-116（K-117 删除）。
- **不讲什么**：事件机制（→07）；ZOMBIE 后续 BIND 拒绝（→10）；树结构（→05）。
- **前置**：05、07、08。**后置**：10、13。
- **事实底线**：`device.c:424-515`（:441-448/:460-480/:485-515）；Rust del_device.rs + device_tree.rs + inode.rs。
- **知识点清单**：K-111~K-116；旧 §3.1 对已退役 unregister 的引用（K-117）改写为 InodeContent 终态。
- **验收标准**：get/put 四行不对称能各举一例；回收八步与 Rust 四文件缝合的映射表完整；引用配对表左半边（出生 1、成员 1）与 08 一致。

### 10-devm-bind-unbind

- **一句话定位**：三方握手：RS 发起、devman 转发、驱动应答——绑定段路径图的终点。
- **讲什么**：K-118~K-123、K-125（K-124 删除）。
- **不讲什么**：RS 侧流程（→14）；驱动响应半（→11）；EPERM 门语义（→06，本篇是使用者）。
- **前置**：04、06、08、09。**后置**：11、14。
- **事实底线**：`bind.c:7-104`（:14/:32/:48/:63/:80/:85/:102）；`manager.c:840-851`；Rust bind.rs（两段式 handler）。
- **知识点清单**：K-118~K-125 按 §2.11。
- **验收标准**：BIND/UNBIND 各自的请求半/应答半能分别画时序；ENODEV=19 容错的语义场景（驱动先删）举实例；§3.3 装配叙事清退、只留机制。

### 11-libdevman-client

- **一句话定位**：服务端镜像——驱动进程里的 devman 客户端：本地表、编码、收发、响应面。
- **讲什么**：K-126~K-134。
- **不讲什么**：服务端处理（→08/09/10）；USB 建模（→12）；DS 机制（→07-stage-ds，本篇讲消费）。
- **前置**：04（wire）、06（消息）。**后置**：12、13。
- **事实底线**：`generic.c:14-21/:24-99/:102-183/:188-202/:207-275`（panic 8 处实测）；`local.h:7`；Rust devman_client.rs（507 行/8 测试）。
- **知识点清单**：K-126~K-134 按 §2.12；K-129 计数按实测 8 处修正"五处"旧文。
- **验收标准**：serialize_dev ↔ encode_device 字节对照表；handle_msg 的 ENODEV 不可分辨语义有前后行为对照；panic→ClientError 映射逐点带 C 行号。

### 12-usb-device-model

- **一句话定位**：USB 设备的属性拼法与两次 ADD 节奏——libdevman 之上的建模层。
- **讲什么**：K-135~K-142。
- **不讲什么**：描述符解码（→minix-usb 分界声明）；通用客户端机制（→11）；devmand 匹配（→13）。
- **前置**：04、11。**后置**：13。
- **事实底线**：`usb.c:24-141/:174-197/:219-301`；`devman.h:36-55`；Rust usb_model.rs（437 行/5 测试，:301 字节锁定）。
- **知识点清单**：K-135~K-142 按 §2.13。
- **验收标准**：11+1 属性清单与 usb.c 逐字节一致；设备-接口父子图能解释两 ADD 顺序与 remove/delete 分家。

### 13-devmand-consumer

- **一句话定位**：事件行的第一个读者——devmand 全部行为对 devman 提出的输出契约（只契约，不实现）。
- **讲什么**：K-150~K-161。
- **不讲什么**：usb.y 文法实现（→18-stage-commands，OA-2）；服务启动机制（→03-stage-rs）；minix-service 内部。
- **前置**：07（事件格式）、08、11、12（属性消费）。**后置**：14（启停交接）。
- **事实底线**：`commands/devmand/main.c:16/:82-233/:238-296/:418-430/:519-560/:579/:596-690/:803-942`；`usb_driver.h`；`etc/devmand/*.cfg`；行号修正按 §8.2。
- **知识点清单**：K-150~K-161 按 §2.15。
- **验收标准**："devman 侧如何满足"对照表逐行有 devman 锚；DEVICE_PROTOCOL bug 按新行号 :250/:254 陈述；启停节末明确"up 之后交给 14"的交接点。

### 14-rs-integration

- **一句话定位**：RS 是 devman 的出生证明与婚姻介绍所——publish/unpublish/inherit 握手中的 devman 臂。
- **讲什么**：K-143~K-149。
- **不讲什么**：RS 全机制（→03-stage-rs）；DS 内部（→07-stage-ds）；BIND 服务端处理（→10）。
- **前置**：06、08（devman_id 文件）、10（BIND 语义）、13（启停因果）。**后置**：99。
- **事实底线**：`manager.c:787/:840-851/:864/:897-909/:1742`；`system.conf:422-429`；`com.h:61`；Rust rs_contract.rs。
- **知识点清单**：K-143~K-149 按 §2.14；旧"rproc"错误锚全部改 publish_service/unpublish 实名（§8.2）。
- **验收标准**：publish 握手四步每步有 manager.c 行号；"不成即杀"与 devman 侧 EPERM 不回构成对照表；K-149 状态带日期与 edge 编号。

### 99-devm-global-concepts

- **一句话定位**：全 stage 收口查询表：常量、错误码、跨服务、全局状态、ARCH 葬处索引。
- **讲什么**：K-162~K-165。
- **不讲什么**：任何机制复述（各归属篇）；状态数字（K-166 删除，→README）。
- **前置**：全部 00~14。**后置**：无。
- **事实底线**：99§1-§5 各行对应 C/Rust 锚（com.h 行号按 :861-866 修正；归属篇列全部改新编号）。
- **知识点清单**：K-162~K-165 按 §2.16。
- **验收标准**：表内每个"归属篇"编号在新目录里存在且该篇确有此内容（逐行抽验）；正文出现状态数字 0 次。

---

## 6. 变更表

| 操作 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 存量去向 |
|---|---|---|---|---|---|---|
| C-01 | 重排（+1 平移） | 旧 03/04/05/06/07/08/09/10/11 | 新 04/05/06/07/08/09/10/11/12 | 为新建 03 腾号；组内相对顺序本就读者正确序 | K-054~K-142 | 整篇平移，仅编号与交叉引用更新（§8.1 T-a 组） |
| C-02 | 重排（换序） | 旧 12-rs-integration / 旧 13-devmand-consumer | 新 14 / 新 13 | §1.3 X-4 先于 X-5 是硬因果：devmand 拉起驱动→驱动发布→RS 才发 BIND；现目录把 RS 放在消费者之前违反执行序 | K-143~K-161 | 两篇整篇平移（§8.1 T-b 组） |
| C-03 | 拆分 + 新建 | 旧 02-vtreefs-framework | 新 02（框架契约面）+ 新 03-fsdriver-transport（字节面层） | 单篇单语义：框架契约与生产传输是两个语义体；后者已有 644 行落地代码 + 一整个 C 协议层（libfsdriver），旧文只有 §3.7 两段承载 | K-026~K-045、K-046~K-053 | 去向逐节见 §8.1 表 A；新增条来源全带锚（§2.4） |
| C-04 | 新建内容（不新建文件） | — | 01 补 K-024（SEF restart）；02 补 K-044（信号退出）、K-168（框架三分对比）；04 收 K-069（类型映射表自旧 02§3.3）；03 补 K-167（测试基建） | 覆盖审计缺口 GA-1/GA-3/GA-4 落实 | K-024 K-044 K-069 K-167 K-168 | 新增方向：证据锚点见 §2 各行 |
| C-05 | 改写（状态叙事清退 + 锚点修正） | 旧 00§5、01§3.4/§4.1/§5、02§4.4、06§5、08§3.1、09§3.3、13 各锚、99§6、README | 对应新篇同节 | 代码已前进（f0e02c70f 生产 Transport 落地、86 测试实测），旧叙事过期且部分虚构（§3.2 GA-5）；行号漂移与错误锚逐处修正（§8.2） | K-011 K-025 K-099 K-117 K-124 K-166 删除；其余改写 | 删除理由逐条：状态数字唯一入口 README（R-7）；虚构测试不存在于代码；退役引用指向已删模块 |
| C-06 | 归档 | 无 | 无 | 15 篇全部有归宿，无退出篇 | — | — |

**删除项单列（G5 要求）**：K-011（00§5 基线数字）、K-166（99§6 同）——理由：状态数字散布正文必然过期，收口 README 状态页；K-025（01 park 叙事）、K-124（09 §3.3 单例未建）——理由：与已落地代码矛盾，机制部分由 K-050/K-051 承接；K-099（06§5 虚构测试名）——理由：所指 files.rs 已退役（todo.md DM-P1-5）；K-117（08§3.1 退役 unregister 引用）——理由：同上，改写为 InodeContent 终态。

---

## 7. 缺漏新篇（非 C 主题清单逐项落实）

| 主题 | 落实 | 为什么重要 | 原料 | 验收 |
|---|---|---|---|---|
| 链接与加载 | 新 01§2（K-019）+ 新 14§2（K-147） | devman 不在 boot_image 是本 stage 第一个事实 | system.conf:422-429、kernel/table.c | 读者能答"谁 exec 了 devman、带什么权限" |
| 镜像与内存布局 | **明确不做**：用户态普通 ELF，无自定 linker script；per-process 内存映射归 02-stage-vm | — | — | — |
| 汇编入口与陷阱进入 | **边界声明**进新 03：SysKernel/KernelIpc 消费面 + E1 指针 | 传输层踩在 trap 桥上 | edge_todo E1 条目 | 03 有"机制在 E1、本篇只讲用法"声明句 |
| 启动装配 | 新 01（C 侧）+ 新 03§3（Rust 装配，K-051） | 服务器"出生"两半 | main.c、main.rs | 装配链每步带文件行号 |
| 构建与工具链 | **不做**：workspace 通用约定归 14-stage-runtime/工具文档 | 非本 stage 语义 | — | — |
| 跨模块接口与线格式 | 新 03（REQ wire）/04（devman wire）/06（DEVMAN 字段）/14（RS 握手） | 四处线格式是一个冲突高发面 | §5 各契约 | 四篇各有字节级对照表 |
| 错误路径 | 各篇 C 分析 + 99§2 收口 | errno 映射是重写硬约束 | 99§2 表 | 码→生产篇逐行可追 |
| 关闭与退出 | 新 02§2（K-044）+ 新 14（unpublish） | 旧目录完全没讲——审计实捉缺口 GA-1 | vtreefs.c:39-46、hooks.rs:139-141 | K-044 成节双锚 |
| 并发与同步 | 新 00§4 原则条目 | 单线程循环是 !Send/RefCell 合法性的根据 | AGENTS.md 执行模型 | 00 有声明句 |
| 测试基建 | 新 03§4（K-045/K-167） | seam 双实现是"测试跑在纯函数上"的机制解释 | todo.md Fix#10 | seam 对照表 |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表

**表 A：旧 02 拆分（逐节）**

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 断链风险 |
|---|---|---|---|---|
| 02§1.1/1.2/1.3 | 框架动机/两类请求/懒初始化 | 新 02§1 | 原样+去重（K-027 传输视角摘出） | 低 |
| 02§2.1~2.9 | 钩子表、inode 池、增删、读循环、lookup、getdents、mount/unmount | 新 02§2 | 原样搬移（K-036 编码半加"详 03"指针） | 低 |
| 02§2.10 | 非文件消息转交（错误锚 sef_local_startup L66） | 新 02§2 末节 | 改写 + 锚修正为 fs_other:67-78 | 中（锚错） |
| 02§3.1/3.2/3.3/3.5/3.6 | A-1 内联、Vec 池、类型映射、assert→Err、ENOSYS 槽 | 新 02§3 | 原样（§3.3 类型映射迁往新 04§3，K-069） | 低 |
| 02§3.4 | 两条分层线：引用计数与编码归传输层 | 新 02§3 + 新 03§1 | 拆分（分层宣言留 02，落点讲在 03） | 低 |
| 02§3.7 | 传输注入 +（2026-09-19 版）MinixTransport 落地 | 新 03§2/§3 | 迁出并扩写成篇（K-045~K-053） | 高（本篇主内容源） |
| 02§4.4 | P1-6 收窄记录 | **删除** | 状态叙事退出（C-05；机制终态在 03） | 低 |
| 02§4.1~4.3、§5~§7 | 模块结构/不变量/差异/测试/过渡/参见 | 新 02 同名节 | 原样+引用编号更新 | 中 |

**表 B：状态/锚点级改写（其余变化节）**

| 旧位置 | 内容一句话 | 新位置 | 类型 | 备注 |
|---|---|---|---|---|
| 00§4 | 15 篇导航表（含矛盾前置） | 新 00§5 | 改写 | 前置列与 §9.1 扫描表对齐（09←10、10←99 两处前向引用即此表遗留） |
| 00§5、99§6 | 测试基线 78/13/139 | README 状态页 | 迁出+更新（实测 86/232） | K-011/K-166 |
| 01§3.4/§4.1、09§3.3、02§4.4 | park/单例未建/P1-6 叙事 | 新 03§3 终态描述 | 删除+承接 | K-025/K-124 |
| 01§5 | first_guard_fires_once 等测试名（hooks.rs 实测无） | 新 01§5 按实测重写 | 改写 | 同 K-099 性质 |
| 03§2.x | §2.7 排在 §2.4 前 | 新 04§2 顺排 | 重排 | 无内容变化 |
| 05§2.1、99§1 | com.h 字段宏 "859-864" | 新 06/99 :861-866 | 锚修正 | §8.2 E-1 |
| 06§5 | 指向已退役 files.rs 的测试名 | 新 07§5 实测重写 | 删除+重写 | K-099 |
| 07§2.3 | ":399" | 新 08 :396 | 锚修正 | §8.2 E-5 |
| 08§3.1 | "06 unregister" 退役引用 | 新 09§3 InodeContent 终态 | 改写 | K-117 |
| 10§1/§2.3 | "五处 panic" | 新 11 实测 8 处逐个带行号 | 改写 | §8.2 E-4 |
| 12§2.1/2.2/2.3 | manager.c 工具生成错误锚 ×3 | 新 14 实名锚 | 锚修正 | §8.2 E-2/E-3 |
| 13§2.5/2.8 | devmand :247-248/:585/:883 | 新 13 :250+:254/:579/:888 | 锚修正 | §8.2 E-6~E-8 |
| 01§3.4/§4.1、04§2.4 | `.design/`/scan.md 隐藏产物引用 ×3 | 对应新篇改引 doc/代码/C 绝对路径 | 改写 | OA-4，P0-process 违例修正 |
| 全目录 | 参见/过渡节中的旧文件名与裸编号 | 新编号 | 批量替换 | 热点见 §8.4 |

**T-a 组（+1 平移）**：旧 03→04、04→05、05→06、06→07、07→08、08→09、09→10、10→11、11→12——整篇小节结构不变，仅文件编号与内部编号引用更新；**T-b 组（换序）**：旧 12→14、旧 13→13（编号不变，位置在组内前移，仅参见关系变）。

### 8.2 勘误锚点表（写作缺陷修正清单，B 相逐条落实）

| # | 错误锚（出处） | 实测真相 |
|---|---|---|
| E-1 | com.h 字段宏 :859-864（05§2.1、99§1 等） | `DEVMAN_GRANT_ID` 等 5 宏在 com.h:861-866 |
| E-2 | vtreefs.c:sef_local_startup（L66，工具生成）（01/02） | L67-78 是 `fs_other`；sef_local_startup 在 :52-65 |
| E-3 | manager.c:rproc（L840/L897，工具生成）（12§2.1/2.2） | L840/L897 所在函数为 `publish_service`（:787）/unpublish 路径；仓库无 `rproc` 符号 |
| E-4 | generic.c "五处 panic"（10§1） | 8 处：:110/:125/:129/:134/:165/:169/:174/:196 |
| E-5 | device.c ":399" TODO（07） | `FUTURE` TODO 在 :396 |
| E-6 | devmand main.c :247-248（13§2.5） | DEVICE_PROTOCOL 重复检查在 :250 与 :254 |
| E-7 | devmand :585 EEXIST（13§2.8） | :579 |
| E-8 | devmand :883 memset（13§2.8） | :888 |
| E-9 | device.c:devman_event_read（L16，工具生成）（04 头部） | L16 是 `next_device_id` 静态变量；event_read 在 :142-168 |
| E-10 | plan.md §7.2 "message_hook :37-47" | main.c:46-58（函数体）——参考材料内部漂移，随 B 相报 plan.md 修订 |
| E-11 | manager.c:rs_start（L1742，工具生成）（12§2.3） | L1742 `rpub->devman_id = rs_start->devman_id;` 所在函数是 `init_slot`（定义于 :1708）；`rs_start` 只是形参名——正文"init_slot 抄写"的说法本来就对，锚的符号归属错了 |

### 8.3 引用迁移表（按类）

| 类别 | 旧引用形态 | 新目标 | 数量（实测） | 验证方式 |
|---|---|---|---|---|
| 目录内文件名 | `NN-语义名.md` 全文提及 | §4.1 映射表逐条 | ≈142 | 迁移后 `grep -rE "0[3-9]-[a-z]|1[0-4]-[a-z]" *.md` 对 §4.1 全对账 |
| 目录内裸编号 | "见 03 §2.7"式 | 新编号（99§1/§3 归属篇列同此） | ≈40 | 逐条人工过（含 § 号者对照表 A/B 防小节号漂移） |
| 代码注释 | `docs: 06-event-buf.md`、`covered in NN` | 新文件名 | 26（os/servers/devman + os/libs/minix-sys） | `grep -rn "docs" os/servers/devman/src os/libs/minix-sys/src/devman_client.rs` 零旧名 |
| 外部文档 | 16-stage-drivers/plan.md、12-gpio-devman.md；12-stage-input/plan.md；15-stage-fs/plan.md；18-stage-commands/{plan,01,04}.md | 具体篇名改新名 | ~8 处 | 各文件 grep `11-stage-devman/[0-9]` 对账 |
| 状态材料 | edge_todo.md、todo.md 内 "doc NN"式引用 | 加注"按 2026-09-19 蓝图换号"映射行（历史文本不追溯改写） | ~6 | 只验证新号可解析 |
| 目录名引用 | `11-stage-devman/` 出现处（master-plan、edge 系列） | 不变 | 0 改动 | — |

### 8.4 断链成本摘要

- **受影响引用总数 ≈ 196**（目录内 182 + 代码注释 26 + 外部正文 ~8，其中部分重叠计数以 grep 实录为准）。
- **热点**：被引最多为旧 05-message-contract（15）、06-event-buf（13）、03-structs / 04-device-tree（各 12）——恰是 +1 平移波及面；代码注释热点在 `add_device.rs`、`vtreefs/`（每文件 3-5 处 `docs:` 引用）。
- **批量方式**：文件名 sed **必须降序**执行避免连锁替换：`12→14` → `11→12` → `10→11` → `09→10` → `08→09` → `07→08` → `06→07` → `05→06` → `04→05` → `03→04`，随后新建 `03-fsdriver-transport.md`；裸编号引用不套 sed（与 § 小节号、C 行号混形），逐条人工改。
- **人工量最大项**：旧 02 的拆分（表 A 逐节）与 §8.2 勘误 11 条——合计预计 1.5 个工作日，其余为机械替换。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**（契约前置逐篇列边，验证全部指向更早编号）：00→∅；01→00；02→01；03→01,02；04→00,01；05→02,04；06→03,04,05；07→02,03,04,05；08→04..07；09→05,07,08；10→04,06,08,09；11→04,06；12→04,11；13→07,08,11,12；14→06,08,10,13；99→全部。**违例 0**（旧目录的两处违例——00 导航表 09←10、plan §3.4 的 10←99——在新契约中消除）。
2. **依赖图无环**：上表边集与编号序同向（每条边 前置<本篇），拓扑序即编号序，**无环**，无需拆解方案。
3. **覆盖率**：池 168 条 = 去向 162 + 删除 6（逐条理由见 §6 删除项单列），**100%**；13 条新增全部带证据锚。
4. **断链统计**：§8.4 实测 ≈196 处，热点与批量方案已给。

### 9.2 自检门

| 门 | 结果 | 依据 |
|---|---|---|
| G1 C 真序逐条可核对 | **通过** | 随机抽 10 条实核：main.c:36-43/:46-58/:60-67/:70-92、com.h:846-866、device.c:75-102/:142-168/:213-219/:345-397、bind.c:7-104、vtreefs.c:52/:67-78/:88、fsdriver.c:34-50/:88-96、manager.c:787/:840-851——全部与源码一致（本次会话逐条 grep/sed 实测） |
| G2 知识点池完整 | **通过** | C 文件逐一归属：服务端 4 .c + 2 .h（04~10）、com.h DEVMAN 块（06）、libvtreefs 8 文件（02/03）、fsdriver.c（03）、libdevman 3 文件（11/12）、manager.c+system.conf（14）、devmand 4 件（13）；非 C 制品：plan §4/§5（00/99）、todo DM 系列（各演进条）、edge 4 条（03/13/14 指针）、Rust 19 文件（各篇 Rust 节）——无"既无归属又无排除"项 |
| G3 前向引用为零 | **通过** | §9.1-1 |
| G4 依赖图无环 | **通过** | §9.1-2 |
| G5 覆盖率 100% | **通过** | §9.1-3；删除项单独成列（§6） |
| G6 拆分/合并/新建双方向规则 | **通过** | 表 A 覆盖旧 02 每一节去向；新增 13 条逐一带 C/Rust/制品锚（抽查 K-046→fsdriver.c:34-50、K-044→vtreefs.c:39-46、K-024→hooks.rs:113 实核在案）；无凭空条目 |
| G7 契约七要素齐全 | **通过** | 16 篇逐篇：定位/讲/不讲/前置/后置/事实底线/清单+验收 全有（04/05/06/07/08/09/10/11/12/13/14/99 的清单引用 §2 池行并注差异，行级信息完整） |
| G8 迁移表覆盖 | **通过（一处从简声明）** | 唯一实质拆分（旧 02）逐节成表；纯平移篇以 T-a/T-b 组行声明"节结构不变"，内容变化节全部入表 B；引用迁移含文档 + 代码注释两类 |
| G9 事实断言带锚 | **通过** | 抽 10 条核对（含"86 passed"、"f0e02c70f"、"字段宏 :861-866"、"8 处 panic"、"fsdriver.c 在 libfsdriver 而非 libvtreefs"、"manager.c:1742 在 init_slot（:1708）内"——均实测）；终稿无待验证标注项 |

### 9.3 结论与待用户裁决

**结论：蓝图完成。** 核心判断：本 stage 旧文档成熟度高（已 CONVERGED + 一轮 code-excellence），**问题不在篇章顺序而在三类缺陷**——①状态叙事与代码脱节（生产 Transport 已落地、测试数 86 vs 文中 78/79），②锚点系统性漂移与"工具生成"假锚（11 处勘误表），③旧 02 一框架一篇承载两个语义体。蓝图对应给出：结构手术三处（C-01/C-02/C-03）、内容手术一套（C-04/C-05 + §8.2），机制知识 100% 保池。

**待裁决 4 项**：
1. **主案（拆分 + 换号 + 换序）vs 方案 B（02 不拆、12/13 不换、零平移）**——两案知识等值，差在 §4.3 所述取舍：接受约 196 处机械迁移换结构正确性，还是保号换结构妥协。默认推荐主案。
2. **状态叙事退出原则**（正文篇一律不带测试计数/接线状态，唯一入口 README——R-7/K-011 处置的依据）是否批准为全 stage 通例。
3. **新 13 对 usb.y DSL 的收窄深度**（OA-2：文法细节移交 18-stage-commands）是否同意——涉及 18-stage 是否已准备承接。

（原第 4 项待裁决——E-11 `manager.c:1742` 函数归属——已在本蓝图收尾时实测解决：所在函数为 `init_slot`（:1708），见 §8.2。）
