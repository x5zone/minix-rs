# 11-stage-devman 文档重建蓝图（glm）

```text
your_name(AI agent name) = glm
target_dir(关注的工作目录) = notes/rewrite/fork-syscall-rewrite/11-stage-devman
repo_root(仓库根目录) = /home/xzhao/github/minix-rs
当前提交号 = d6ecd22cae78345e746c82ad7cbe3c1612c151c2

任务 = R 相·重建蓝图：输出 11-stage-devman/doc_rerank_glm.md，不改任何正文。
约束 = 未引用 .design/ 与 tmp_design_and_todo/；未读取其它 AI 的 doc_rerank_* 产物；
       所有落盘产物仅本文件一个，带 _glm 后缀。
```

---

## 0. 元数据

- 执行者：glm；日期：2026-09-19；目标目录：`notes/rewrite/fork-syscall-rewrite/11-stage-devman/`。
- **结论先行**：本 stage 的 15 篇文档（00~13 + 99）是四个已审 stage（06-sched / 08-is / 09-init / 本 stage）中**维护得最好的一套**——2026-09-04 全部成文并 CONVERGED，2026-09-15 首轮架构审查的 10 条 stage 内条目**当日全部闭环**（Fix #1~#10，净删约 600 行、unsafe 归零），且 **Fix 的演化注记已回写进 01/02/04/05/06/07/08/09 各篇正文**（如 04 §2.4 的 DM-P1-4 预算前缀账、01 §3.1 的钩子退役史）。实测 `cargo test -p minix-devman` **80 passed / 0 failed**，clippy 本体 0 告警。因此本蓝图的操作集是**"保编号、清失效行、追基线"**：
  1. **保编号**——编号即"启动链 + 设备生命周期旅程"双主线（plan §1.2/§1.3），四条硬标准满足（§9 G3/G4）；篇内互引 ≈120 处、外部 14 个文件 40+ 处、代码注释 3 处（minix-types/com.rs→05 篇、minix-sys/lib.rs→10/11 篇）全以现有文件名为锚；
  2. **清失效行**——唯一实质缺陷：**01 与 06 两篇的 §5 测试表各含已退役测试的失效行**（`first_guard_fires_once` 随 DM-P1-3 退役、`register_resolves_index`/`dispatch_end_to_end` 随 DM-P1-5 退役，`rg` 代码零命中），且 01 缺新增的 `test_devman_sef_production_hooks` 行——这是 Gate E"声称但代码缺失"的违例（2026-09-18 刷新轮漏清）；
  3. **追基线**——99 §6 自称"终态 78 passed"与 README"78 passed（2026-09-04）"均已过时（实测 **80**；01/05/07/08/09 的"79（2026-09-15）"也差 1）；另有 19 处"（LNN，工具生成）"锚点符号错位（集中于 01×7、12×7）。
- 审查范围：
  - **正文**：`[0-9][0-9]-*.md` 15 篇（3086 行）+ `README.md`（63 行状态页）；
  - **参考材料**（不改）：`plan.md`（482 行）、`todo.md`（203 行，首轮架构审查全文）、`draft/README.md`（占位素材）；
  - **范围外**：`.design/`、`tmp_design_and_todo/`、其它 AI 的 `doc_rerank_*`、`minix3/` 原树（只读）。

### 0.1 读取清单（步骤 0 四份清单）

**清单一：文档清单**（行数实测）

| 编号 | 文件 | 行数 | 状态 | 边界摘要 |
|---|---|---|---|---|
| 00 | 00-devm-overview.md | 184 | reviewed + 双主线导航 | 四层版图/27 函数清单/三陷阱预告/ARCH 总表 |
| 01 | 01-devm-init-main.md | 410 | reviewed + Fix 注记（DM-P1-2/P1-3） | main 三步/三钩子/run_vtreefs/mount 触发/RS 加载组 |
| 02 | 02-vtreefs-framework.md | 279 | reviewed + Fix 注记（DM-P1-2/P1-5/P3-1） | 13 槽全表/inode 池/read 循环/getdents 顺序/A-1 决策 |
| 03 | 03-devm-structs.md | 188 | reviewed | 三重身份/状态机/wire 三坑/BSS accident/同名双生 |
| 04 | 04-device-tree.md | 211 | reviewed + Fix 注记（DM-P1-4） | 双树缝合/DFS/`./` 前缀寻路/预算前缀账/墓碑 |
| 05 | 05-devm-message-contract.md | 198 | reviewed + Fix 注记（DM-P2-1/P2-2） | 消息表/相位表/grant 三错/do_reply/EPERM 不回/A-3 三环证据 |
| 06 | 06-event-buf.md | 186 | reviewed + Fix 注记（DM-P1-5） | 三游标漏斗/队列方向/两步 drain/`\n` 不对称 |
| 07 | 07-devm-add-device.md | 171 | reviewed + Fix 注记（DM-P1-1/P1-4/OQ-3） | 认领-落户-广播 8 步/unwind_staged/空白名拒绝 |
| 08 | 08-devm-del-device.md | 166 | reviewed | 广播先行/ZOMBIE/引用配对表/回收八步 |
| 09 | 09-devm-bind-unbind.md | 146 | reviewed + 装配（Server::run） | 三方握手/19 容错/ZOMBIE 守卫/绑定段路径图 |
| 10 | 10-libdevman-client.md | 139 | reviewed | 镜像论/五 panic→ClientError/EPERM 不回的客户端镜像 |
| 11 | 11-usb-device-model.md | 136 | reviewed | 属性拼法/两次 ADD/双回调/remove-delete 分家 |
| 12 | 12-rs-integration.md | 159 | reviewed | publish 杀/unpublish 宽容/继承/出生权限 |
| 13 | 13-devmand-consumer.md | 149 | reviewed（外部契约不实现） | 行解析/dev_type/8 属性/9 标志匹配/DSL/脚本 |
| 99 | 99-devm-global-concepts.md | 92 | reviewed（基线数字过时，G-2） | 常量/错误码/跨服务 11 对/全局归宿/ARCH 索引 |

**清单二：C 源码清单**（plan §5.1/§5.3 的 27 函数 + 6 静态映射经 2026-08-16 回归 review 实证；本次抽验全部命中）

| C 文件 | 行数 | 承载文档 |
|---|---|---|
| `minix3/minix/servers/devman/main.c` | 93 | 01（main/hooks）+ 05（fall-through :46-58，本次 sed 复核四 case 无 break） |
| `minix3/minix/servers/devman/device.c` | 520 | 04/05/06/07/08（do_reply :213-219、next_device_id :16 本次复核） |
| `minix3/minix/servers/devman/bind.c` | 105 | 09（RS 门 :14,:63、19 特例 :85 本次复核） |
| `minix3/minix/servers/devman/buf.c` | 129 | 06 |
| `servers/devman/{devman,devinfo,proto}.h` | — | 03/04/99 |
| `minix3/minix/lib/libvtreefs/`（1642 行，使用面） | — | 02 |
| `minix3/minix/lib/libdevman/{generic,usb}.c` + `local.h` | 576+ | 10/11（generic.c:85 `type = 0 /* TODO */` 本次复核） |
| `minix3/minix/include/minix/com.h:846-866` | 常量 | 05/99（DEVMAN_BASE 0x1200 本次复核） |
| `minix3/minix/include/minix/{devman,vtreefs,rs}.h` | — | 03/02/12 |
| `minix3/minix/servers/rs/manager.c`（:840,:897,:1742 devman_id） | 对端 | 12（本次复核） |
| `minix3/minix/commands/devmand/`（1119 行）+ `etc/devmand/` | 对端 | 13（外部契约） |
| `minix3/etc/system.conf:422-429` | 启动证据 | 01/12（本次 sed 复核） |

**清单三：非 C 制品清单**

| 制品类 | 实际情况 | 归属 |
|---|---|---|
| 链接与加载/镜像布局 | 不在本 stage（RS 运行时加载；无自定义段） | 00/01 证据面 |
| 汇编入口与陷阱进入 | 系统调用面走 minix-sys（E-DMWIRE 生产接线 open） | 14-stage-runtime / edge |
| 引导链与引导协议 | 不在 boot_image（`kernel/table.c` 无 devman）→ RS 加载组 | 00/01 |
| 构建与工具链 | Makefile | WONTFIX（plan §5.5） |
| 跨模块接口与线格式 | wire 16B 头+条目+串区（03/10 双向锁定）+ DEVMAN 消息相位（05） | 本 stage 主场 |
| 错误路径 | 99 §2 错误码表 8 行（含 EPERM 不回、19 容错两个特例） | 99 + 各篇 |
| 关闭与退出 | got_signal SIGTERM / cleanup 链 | 01/02 |
| 并发与同步 | 单线程事件循环（lib.rs 单线程模型文档） | 00§3.1/各篇 |
| 测试基建 | 80 内联测试 + VecTransport/FakeTransport/RsTransport 注入；真机归 E5(h) | 各篇 §5 + 99 |
| Rust 实现入口 | `os/servers/devman/src/`（14 文件 + `ipc/` + `vtreefs/` 子目录，**4933 行**：vtreefs/inode 669、vtreefs/mod 528、server 626、add_device 457、device_tree 436、del_device 286、wire 280、bind 247、ipc/message 246、rs_contract 237、hooks 240、structs 204、buf 172、event_queue 133）+ `minix-sys/{devman_client,usb_model}.rs`（860 行） | 各篇 §3/§4 |

**清单四：引用关系清单**

| 引用方 | 处数 | 形态 |
|---|---|---|
| 篇内互引实例 | ≈120 | 被引最多：05（13）、06（11）、07/04/03（各 9） |
| `edge_todo.md` | 11 | E-DMWIRE/E-DMCLIENT（已闭）/E-REQWIRE/E-ISWIRE/E-DSWIRE/E5(h) |
| 其它 stage plan/文档 | 21 | 18-commands（8）、12-input（5）、16-drivers（4）、15-fs（3）、03-stage-rs doc_rerank（4）等，全部**目录级** |
| 代码注释 | 3 | `minix-types/src/types/com.rs:139`→05 篇、`minix-sys/src/lib.rs:90`→10 篇、`:105`→11 篇 |

### 0.2 使用的命令与关键输出（证据摘录）

```bash
# 测试基线（80 passed；todo 记 79、99/README 记 78——三级漂移）
cd os && cargo test -p minix-devman   # → test result: ok. 80 passed; 0 failed
# 分布：vtreefs/inode 12 / vtreefs/mod 8 / server 7 / device_tree 7 / del_device 6 /
#   add_device 6 / ipc/message 5 / hooks 5 / wire 4 / rs_contract 4 / buf 4 / bind 4 /
#   structs 3 / event_queue 3 / ipc/dispatch 2（合计 80）
# clippy：minix-devman 本体 0 告警（minix-types 1 + minix-sys 2 为既有遗留）
# 失效测试行（Gate E 违例证据）
grep -rn 'first_guard_fires_once\|register_resolves_index\|dispatch_end_to_end' os/servers/devman/src/
#   → 零命中（三者在代码中已退役；01§5 表 1 行 + 06§5 表 2 行仍声称）
# hooks.rs 现有测试：root_stat_matches_c_main / server_config_defaults_match_c /
#   test_devman_sef_production_hooks / sef_ok_records_lifecycle_sequence /
#   sef_failing_returns_enomem（文档表缺第 3 个新测试行）
# C 锚点抽验（全部命中）
sed -n '46,58p' minix3/minix/servers/devman/main.c      # switch 四 case 无 break（A-3）
grep -n '!= 19' minix3/minix/servers/devman/bind.c       # :85 unbind 容错
sed -n '213,219p' minix3/minix/servers/devman/device.c   # do_reply 三行
grep -n 'DEVMAN_BASE' minix3/minix/include/minix/com.h   # :846 = 0x1200
sed -n '422,429p' minix3/etc/system.conf                 # service devman 权限
```

---

## 1. C 真序

### 1.0 阶段类型判定

**服务事件循环型**（主骨架：SEF 出生 → VTreeFS 主循环 → mount 触发初始化 → 双通道分发）+ **生命周期旅程型**（次主线：一次设备注册从驱动到绑定的跨进程旅程）。判定理由与 plan §1.2 一致：devman 的执行链严格线性（main → run_vtreefs → mount → init_hook → 主循环），但它的外部语义一半在消费者（devmand/RS/驱动），故次主线以"ADD → 事件 → devmand → RS bind"贯穿 06~13。与姊妹 stage 的差异点：devman 的主循环是**双通道**（VFS 文件请求走 fsdriver 表 + DEVMAN 消息走 fs_other），这是它区别于普通 IPC 服务器（sched/is）与状态机（init）的第三种形态——"文件系统外形的服务器"。

### 1.1 真序表

**A 启动段（RS 加载 → 进主循环）**

| 步骤 | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| A1 | RS 按 `system.conf:422-429`（uid 0 + VM SETCACHEPAGE/CLEARCACHE）拉起 devman | `etc/system.conf:422-429` | 不在 boot_image（`kernel/table.c` 无条目） |
| A2 | `main`：memset 钩子表 → 填 3 钩（init/read/message）→ 定 root_stat（S_IFDIR\|0444 五行）→ `run_vtreefs(&hooks,1024,0,&root_stat,0,4097)` | `main.c:70-91`；`devman.h:39` BUF_SIZE | 三步：填表/定根/开跑 |
| A3 | `run_vtreefs`：六参数暂存全局（SEF 签名穿不过参）→ `sef_local_startup`（fresh=init_server / STATEFUL restart / got_signal）→ `init_inodes(1024,…)`+`init_buf(4097)`（失败 panic）→ `fsdriver_task(&vtreefs_table)` 主循环 | `libvtreefs/vtreefs.c` + `inode.c:31-99` | 先活（分配）再营业（mount） |

**B 主循环双通道**

| 步骤 | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| B1 | VFS mount → `fs_mount`（拒 root EINVAL）→ `init_hook`（`static int first` 守卫）→ `devman_init_devices()` 建 root_dev + devices/ + events/ | `mount.c:10-38`；`main.c:36-43`；`device.c:187-207` | 延迟初始化：进程先活，挂载才建树 |
| B2 | VFS 文件请求 17 种 → `vtreefs_table` 框架自带实现（lookup/getdents/stat 不经钩子；read → `read_hook` → 各 inode 的 `read_fn`） | `table.c:6-24`；`file.c:46-295`；`main.c:60-67` | 钩子是扩展点不是入口 |
| B3 | 非 VFS 消息 → `fs_other` → `message_hook` → **switch 四 case 无 break**（A-3：ADD 会贯穿执行 DEL+2×EPERM） | `main.c:46-58` | C 缺陷；Rust 单分派修复 |

**C 设备生命周期旅程（次主线，跨进程）**

| 步骤 | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| C1 | 驱动 `devman_init`（DS 查 label）→ `devman_add_device`（serialize_dev 编码 → grant → sendrec） | `libdevman/generic.c:188-202,102-149` | 客户端五 panic |
| C2 | 服务端 `do_add_device`：grant safecopy 三错 → wire 解码 → `add_child`（id 分配/目录/STATIC 属性/devman_id 文件/父引用）→ state=UNBOUND → ADD 事件 → 双字回复 | `device.c:223-277,314-418` | 认领-落户-广播 |
| C3 | devmand 轮询 `/sys/events` → 解析行 → 读 dev_type → 接口事件 → 8 属性生成 id → usb.y DSL 匹配 → `minix-service up` + mknod 脚本 | `commands/devmand/main.c`（13 篇契约抽取） | 外部契约，不实现 |
| C4 | RS publish（devman_id≠0 → DS 查 devman → DEVMAN_BIND 填字 sendrec，失败即 kill）；devman `do_bind_device`（RS 门 → find → 原样转发 owner → 驱动回调 → BOUND+get）→ 回 RS | `rs/manager.c:840-851`；`bind.c:7-50` | 三方握手 |
| C5 | 退出路径：UNBIND（19 容错/ZOMBIE 守卫）、DEL（REMOVE 先行 → ZOMBIE → put 归零回收八步）、unpublish（宽容不杀） | `bind.c:56-104`；`device.c:424-515`；`rs/manager.c:897-909` | 上线严格下线宽容 |

### 1.2 序差表

| # | 运行时事实 | 教学序 | 理由 | 回指补偿 |
|---|---|---|---|---|
| 序-1 | 框架（VTreeFS）先于业务代码被执行（A3 在 B1 前） | 02 在 01 之后 | 01 是调用点锚点篇，被调用方（框架）语义紧随其后 | 01§6 过渡"01 给调用点，02 给被调用方" |
| 序-2 | grant 原语/wire 解码是 ADD 的前置步骤 | 05（原语）先于 07（业务） | 原语层先行，handler 只调用 | 05§4.2 调用关系图 |
| 序-3 | 事件机制（06）在 ADD（07）首次使用前已初始化（A1 的 events 文件） | 06 在 07 之前 | 机制与生产者分离 | 06§5→07§5 契约测试映射 |
| 序-4 | 客户端（驱动侧）的 add 在运行时先于服务端 handler（消息由它发出） | 10/11 在 07~09 之后 | 服务端是本 stage 主视角；客户端是旅程起点换视角（次主线首段） | 00§4"旅程序"列（10→07→06→13→12→09） |
| 序-5 | RS 的 publish 在运行时晚于设备注册（驱动先注册、服务后发布） | 12 在 09 之后 | 09 的握手语义需要 12 的填字上游；12 引用 09 的门 | 09§1.2 路径图含 12 节点 |
| 序-6 | devmand 与 RS 是并列的两个外部消费者 | 13 压轴 | 契约篇收口；13 的约束反查表需要全部生产侧篇先行 | 13§3 反查表逐行引 06/07/08/11 |

---

## 2. 知识点全集

> 编号 K-001 起；存量为主（15 篇信息密度极高），新增 4 条均为状态/基线性。

### 2.1 知识点池总表（按域分组）

**域 A：位置与版图（00/01）**

| 编号 | 名称 | 类型 | 现有位置 | 关键锚点 | 去向 |
|---|---|---|---|---|---|
| K-001 | devman = 文件系统外形的设备管理器（sysfs/uevent/udev 微内核同构；RS 加载组非 boot） | 概念 | 00§1、01§1.1、13§3 | `system.conf:422-429`；`kernel/table.c` 无条目 | 00 |
| K-002 | 四层语义版图（服务端 1013 行/框架使用面/协议面/客户端 603 行 + 外部消费者） | 概念 | 00§2 | `wc -l` 实测表 | 00 |
| K-003 | main 三步（填表/定根/开跑）+ 六参数中转（SEF 签名限制）[ARCH:A-1-相关] | 机制 | 01§2.1/§2.5/§3.4 | `main.c:70-91`；`vtreefs.c` 注释 | 01 |
| K-004 | 三钩子直觉与退役史（init→ensure_devices、message→Server::run、read→InodeContent；三钩全部有归宿） | 机制 | 01§1.3/§3.1、06§3.4 | `main.c:36-67` | 01 |
| K-005 | RS 加载组与出生权限（uid 0 + 两项 VM 特权的机制对照） | 约束 | 01§2.8、12§2.4 | `system.conf:422-429` | 01/12 |

**域 B：VTreeFS 框架（02）**

| 编号 | 名称 | 类型 | 现有位置 | 关键锚点 | 去向 |
|---|---|---|---|---|---|
| K-006 | 13 槽钩子全表与读写缺省不对称（读缺省 EOF/写缺省 EACCES→Rust ENOSYS 取舍） | 接口与协议 | 02§2.1/§3.6 | `vtreefs.h:24-44`；`file.c:123` | 02 |
| K-007 | inode 池（1024 硬上限；三段式 ENOMEM；CHECK_INODE 不变量） | 数据结构 | 02§2.2 | `inode.c:31-99` | 02 |
| K-008 | 双限名字（NAME_MAX 511 断言 vs PNAME_MAX 24 分配策略）与 purge 对 devman 恒假（NO_INDEX-only → 池不可回收） | 约束 | 02§2.3 | `inode.c:142-249`；device.c 四处 NO_INDEX | 02 |
| K-009 | 删节点两阶段（先删孩子后置标志；目录留父链；引用归零才回收）与编号规则（ino=槽+1，0 天然无效） | 机制 | 02§2.4/§2.6 | `inode.c:369-626` | 02 |
| K-010 | read 循环五步与部分结果规则（先产出后出错=成功；短读即 EOF） | 机制 | 02§2.5 | `file.c:46-103` | 02 |
| K-011 | getdents 顺序契约（./.. 先行、树序、不检查目录位）与编码归传输分层 | 机制 | 02§2.8/§3.4 | `file.c:195-295` | 02 |
| K-012 | A-1 落地论证（框架内联 devman crate；共享 minix-vtreefs stub；procfs 复用时再抽） | 架构演进 | 02§3.1 | RS sef.rs 先例 ×5 | 02 |
| K-013 | assert→Err 映射表（8 行硬化清单）与 err_marker 哨兵的删除（错误入 Reply 载荷） | 架构演进 | 02§3.5/§3.7 | 单测映射表 | 02 |

**域 C：数据结构与设备树（03/04）**

| 编号 | 名称 | 类型 | 现有位置 | 关键锚点 | 去向 |
|---|---|---|---|---|---|
| K-014 | 设备三重身份（树节点/文件集合/wire 消息）与同名双生 `devman_dev`（wire 才是真契约） | 数据结构 | 03§1/§2.7 | devman.h vs local.h | 03 |
| K-015 | 三态状态机（UNBOUND/BOUND/ZOMBIE；转换仅 ADD/BIND/DEL 三处） | 约束 | 03§1.1 | `devman.h:89-91` | 03 |
| K-016 | wire 布局与三坑（subsystem_offset 不写不读、req_nr 两端皆死、type 恒 0 TODO；`#if 0` bus 残块） | 接口与协议 | 03§2.4、10§2.2 | `generic.c:36-99`（:85 本次复核） | 03/10 |
| K-017 | BSS 半初始化根（正确的 accident→Device::root() 全显式）与死字段清理（major 只写不读） | 事实 | 03§2.5/§3.3 | `device.c:187-207` | 03 |
| K-018 | 双树缝合（Device.binding: Option<Ino>；指针→句柄） | 数据结构 | 04§1/§3.1 | `devman_inode.inode` | 04 |
| K-019 | DFS 先序查找（包装蒸发；无 id 索引的论证） | 机制 | 04§2.3/§3.2 | `device.c:283-308` | 04 |
| K-020 | 寻路协议（`./` 前缀 + 尾斜杠逐字节复刻；预算参数化；**前缀账**：ADD 扣 4/REMOVE 扣 7，路径上限 112/109）[DM-P1-4 修正已入文] | 机制 | 04§2.4 | `device.c:45-70,89-91,122-124` | 04 |
| K-021 | 墓碑删法（Vec<Option> 不断号；REMOVE 只认 id 的外部理由在 13） | 数据结构 | 08§3.2、13§3 | `find(id)` 稳定性 | 04/08 |

**域 D：消息面与读写（05/06）**

| 编号 | 名称 | 类型 | 现有位置 | 关键锚点 | 去向 |
|---|---|---|---|---|---|
| K-022 | 消息总表（ADD/DEL 人人可发；BIND/UNBIND 仅 RS）+ 1 基址 10 消息 5 字段宏 | 接口与协议 | 05§1/§2.1 | `com.h:846-866`（本次复核） | 05 |
| K-023 | 相位表（m4 同字多义：GRANT_SIZE≡DEVICE_ID、GRANT_ID≡RESULT；禁裸读） | 约束 | 05§1.1/§3.1 | `com.h:859-864` | 05 |
| K-024 | grant 拷贝三错（ENOMEM/EINVAL/ENODEV；safecopy 失败映 EINVAL 非 EFAULT） | 机制 | 05§2.2 | `device.c:228-250` | 05 |
| K-025 | do_reply 原语（原地改+异步发；先填后戳隐式契约→apply_reply_with_id 双字修复 [DM-P2-1]） | 机制 | 05§2.3 | `device.c:213-219,268-270` | 05 |
| K-026 | EPERM 不回（RESULT 写了但不 send；调用者 sendrec 永远阻塞；客户端镜像对称） | 约束 | 05§2.4、10§2.5 | `bind.c:11-19` | 05/10 |
| K-027 | A-3 fall-through 三环证据链（客户端契约/双回复/错误放大）与单分派修复 | 架构演进 | 05§2.5/§3.2 | `main.c:46-58`（本次 sed 复核） | 05 |
| K-028 | 未实现消息面 fail-closed（5 个零引用码 + REPLY 码 + DYNAMIC TODO；"不新行为"原则） | 约束 | 05§2.6、07§2.3 | grep 全树空 | 05 |
| K-029 | Buf 三游标漏斗（skip=offset 化身/left=len 化身/used=成绩单；-1 NUL 位） | 机制 | 06§2.1/§2.2 | `buf.c:8-128` | 06 |
| K-030 | 队列方向（HEAD 进 LAST 出=FIFO）与两步 drain（数据读+EOF 读=一次消费；空读才删） | 机制 | 06§2.3/§2.4 | `device.c:75-168` | 06 |
| K-031 | `\n` 不对称（事件行无尾换行/静态信息有；两消费者各取所需） | 事实 | 06§2.4-2.5 | `:156` vs `:177` | 06 |
| K-032 | 内容挂节点三代演进（裸指针 cookie→下标+unsafe 单例→InodeContent 枚举；unsafe 归零）[DM-P1-5] | 架构演进 | 06§3.4 | files.rs 退役 | 06 |

**域 E：生命周期 handler（07/08/09）**

| 编号 | 名称 | 类型 | 现有位置 | 关键锚点 | 去向 |
|---|---|---|---|---|---|
| K-033 | ADD 认领-落户-广播 8 步（顺序不可换；出生 refcount=1；devman_id 文件人人有） | 机制 | 07§1/§2 | `device.c:223-397` | 07 |
| K-034 | DYNAMIC 静默跳过（返回值丢弃；发送方恒 STATIC 的完整证据链）[A-6] | 约束 | 07§2.3 | `device.c:404-418` | 07 |
| K-035 | 发布点前零残留回滚（unwind_staged + rollback_id；失败 ADD 不凿 id 洞）[DM-P1-1 修正已入文] | 架构演进 | 07§4.2 | `add_device.rs` | 07 |
| K-036 | 截断两策（静态预截断 vs 事件严拒）与空白名拒绝（OQ-3 新行为：错配比失败更坏） | 约束 | 07§3.4/§3.5 | devmand `%s` 劈行 | 07 |
| K-037 | DEL 广播先行（路径依赖活树）→ 改态 → 放引用；UNBOUND 不经 ZOMBIE | 机制 | 08§1/§2.2 | `device.c:441-448` | 08 |
| K-038 | 引用配对表（创建 1/bind +1/del −1/unbind −1/父成员账；"有孩子报错"注释无代码——账平则冗余） | 约束 | 08§2.3-2.4/§4.2、09§4.2 | `device.c:460-515` | 08/09 |
| K-039 | 回收八步（属性→目录→摘链→put 父级联→free info/dev） | 机制 | 08§2.4 | `device.c:485-515` | 08 |
| K-040 | 绑定三方握手（RS 发起/devman 转发/驱动应答；原样转交 endpoint 预填；sendrec 同步 vs 回复异步） | 机制 | 09§1/§2.1/§2.3 | `bind.c:7-50` | 09 |
| K-041 | unbind 双特例（19=驱动已删视为成功强制 OK；ZOMBIE 不开倒车） | 约束 | 09§2.2 | `bind.c:85-94`（本次复核） | 09 |
| K-042 | Server::run 统一循环与 OutAction 传输契约（FS/Devman 同表；两段式 handler 拆 IPC 线）[DM-P1-2] | 架构演进 | 09§3.1/§3.3、02§3.7 | `server.rs` | 09 |

**域 F：客户端与外部消费者（10/11/12/13）**

| 编号 | 名称 | 类型 | 现有位置 | 关键锚点 | 去向 |
|---|---|---|---|---|---|
| K-043 | 客户端镜像论（wire 三处对读：serialize/parse/encode）与五 panic→ClientError 超集 | 架构演进 | 10§1/§2.3/§3.1 | `generic.c` 五 panic 行号 | 10 |
| K-044 | 客户端响应面（do_bind 扫表回调；无回调≡失踪同 ENODEV；陌生来源静默） | 机制 | 10§2.5 | `generic.c:207-275` | 10 |
| K-045 | USB 属性拼法逐串锁定（设备 5 hex+条件串+dev_type 压轴；接口 7 项；TAIL 顺序即契约） | 接口与协议 | 11§2.1 | `usb.c:24-141` | 11 |
| K-046 | 两次 ADD 建模（设备先行接口后随；parent=server id；cb_data (dev_id,-1) 分流）与 remove/delete 分家 | 机制 | 11§2.2-2.5 | `usb.c:143-301` | 11 |
| K-047 | RS 握手不对称（publish 失败即 kill 三路/unpublish 永不杀；devman_id 三站：继承→用掉→注销） | 机制 | 12§2 | `rs/manager.c:840-851,897-909,1742`（本次复核） | 12 |
| K-048 | devmand 消费契约七条（行格式/dev_type 字节级/路径禁空格/8 属性可读/REMOVE 只认 id/major 128/两步 drain）与 quirk 即契约（REMOVE 空路径、DEVICE_CLASS 复制粘贴 bug 不修） | 接口与协议 | 13§2/§3 | `commands/devmand/main.c` 各段 | 13 |
| K-049 | /sys 与 /dev 两树分工（devman 管 sysfs 树；/dev 节点归脚本 mknod） | 概念 | 13§2.7、00§1.1 | `scripts/block` | 13 |

**域 G：全局（99）**

| 编号 | 名称 | 类型 | 现有位置 | 关键锚点 | 去向 |
|---|---|---|---|---|---|
| K-050 | 常量总表 16 行（含死宏死结构挂名）与错误码表 8 行（EPERM 不回/19 容错特例标注） | 约束 | 99§1-2 | 各篇行号 | 99 |
| K-051 | 跨服务引用 11 对（谁跟谁说话/方向/载体）与全局状态归宿表 | 概念 | 99§3-4 | — | 99 |
| K-052 | 测试基线与分布（**80**：inode 12/vtreefs-mod 8/server 7/device_tree 7/del 6/add 6/message 5/hooks 5/wire 4/rs_contract 4/buf 4/bind 4/structs 3/event_queue 3/dispatch 2；minix-sys devman 侧 12 = client 7 + usb 5） | 测试性质 | **新增**（99§6"终态 78"与 README"78"已过时） | `cargo test` 实测 | 99/README/各篇 §5.1 |
| K-053 | E-DMWIRE 生产接线四缺状态（server transport/请求分类器/装配半/client-RS 生产传输；open，前置 E1；main.rs park 语义已被 02§4.4 收窄为"传输接线"） | 工具与工程 | **新增**（01§4.1 表述早于收窄） | edge_todo:705 | 01/99 状态指针 |
| K-054 | A-3/A-6/A-9 等 10 项 ARCH 的三处一致落点索引 | 架构演进 | 99§5、00§2.4 | plan §4 | 99 |

### 2.2 统计摘要

- 总条数 **54**：概念 5、机制 24、数据结构 6、接口与协议 7、约束 8、架构演进 6、事实 2、工具与工程 1（跨域条目按主域计）。
- 来源：存量 50、新增 4（K-052/K-053 为状态账，K-020/K-035 的修正事实已入正文故仍算存量承载）。
- 主讲述点重复：见 §3.3。

---

## 3. 覆盖审计

### 3.1 主题全集与来源

四路：① C 符号全集（devman 27 函数 + 6 静态、libvtreefs 使用面、libdevman 16 函数、devmand 契约面——plan §5.3/§7.2 回归实证，本次 C 锚点抽验全部命中）；② OS 通用概念（sysfs/uevent/udev 同构、伪文件系统框架、设备生命周期）；③ 非 C 制品（清单三）；④ 边界契约（E-DMWIRE/E-REQWIRE/E-ISWIRE/E-DSWIRE/E5(h) 五条 edge + RS/VFS/DS/devmand 四个对端）。

### 3.2 覆盖缺口表

| # | 缺口 | 证据 | 建议 | 处理 |
|---|---|---|---|---|
| G-1 | **01/06 两篇 §5 测试表含退役测试失效行（Gate E 违例）**：01§5 表首行 `first_guard_fires_once`（FirstGuard 随 DM-P1-3 退役，rg 代码零命中），且缺 2026-09-15 后新增的 `test_devman_sef_production_hooks` 行（hooks.rs 现有 5 测试，表列 6 行中 1 失效）；06§5 表末两行 `register_resolves_index`/`dispatch_end_to_end`（files.rs 随 DM-P1-5 整体退役）——06§4.1 已自注"files.rs 已删除"，但测试表未跟 | §0.2 grep 零命中；hooks.rs/event_queue.rs 现有测试名清单 | 01：删失效行 + 补新测试行；06：删两失效行（保留 buf 4 + queue 3 = 7 行）；两篇 §5 计数行同步 | **采纳（本轮核心修正）** |
| G-2 | **基线数字三级漂移**：99§6"测试基线（**终态**）78 passed"（自称终态，实际 80 且分布表按 78 口径已错位）；README:42"78 passed（2026-09-04）"（状态页数字过时）；01/05/07/08/09 的"79 passed（2026-09-15）"差 1；10/11 的 minix-sys"13 passed"按现树 devman 侧为 12（client 7 + usb 5，"旧 1"归属需复核） | §0.2 实测 | 99§6 重写为 80 + 新分布表（K-052）并去掉"终态"措辞（改为"截至日期"口径）；README 数字刷新；79 处改为 80 或加"截至"日期限定 | 采纳 |
| G-3 | **19 处"（LNN，工具生成）"锚点符号错位**（01×7、12×7、04×2、02×2、03×1）：行号多对但符号常错——如 02§2.10 `vtreefs.c:sef_local_startup（L66）`（:66 实为 fs_other 区）、12 定位行 `manager.c:rproc（L840）`（:840 是 publish 握手段，rproc 是另一函数）、04 头部 `device.c:devman_event_read（L16）`（:16 是 next_device_id）。与 06/08/09 三 stage 同源的工具产物 | 逐条 sed 抽验 | B 相统一改纯行号锚或正确符号（与其它 stage 同一修法，一次批量） | 采纳 |
| G-4 | **01§4.1 的 P1 表述早于收窄**："该 park 是 02-owned 前向缺口（P1 跟踪）"——02§4.4 已把它收窄为"传输接线"（循环已就绪）；01 的句子是收窄前旧文 | 01§4.1 vs 02§4.4 对照 | 01§4.1 补半句"（后收窄为传输接线，见 02§4.4）"或直接改按收窄后口径 | 采纳（轻） |
| G-5 | **00§5 测试基线节过时**：写"截至 2026-09-04，7 passed（01 子集）"——作为带日期快照可保留，但该节未像 01/05 那样追加后续更新注记，读者需自行拼合 7→31→…→80 的演化链 | 00§5 vs 各篇 §5 | 00§5 追加一行"当前全 crate 口径见 99§6"指针即可（不重写历史快照） | 采纳（轻） |
| G-6 | **todo.md 头部范围行过时**（记录不改）：todo 记"18 文件 4694 行，78 测试"（审查时点），现 4933 行/80 测试——todo 是 scan-only 存档，按其定位不回写；基线命令块的"79"与 §0 表是审计时点事实 | todo 头部 vs 实测 | 记录；B 相交付说明带一句对账 | 记录 |
| G-7 | **E-DMWIRE 状态指针分散度可接受但缺汇总**：01§4.1/02§3.7/§4.4/09§3.3 各有一角，00/99 无汇总行（对照 08-is 的 G-9 同型缺口） | edge_todo:705 对照正文 | 99 补一行 edge 状态汇总（E-DMWIRE open 四缺 + E5(h) 验收出口；E-DMCLIENT 已闭） | 采纳（轻） |

### 3.3 重复主题表（主讲述点裁决）

| 主题 | 出现位置 | 主讲述点 | 其余处理 |
|---|---|---|---|
| fall-through 现象 vs 定性 | 01§2.4（现象）/05§2.5（定性） | 05 | 01 明示"只记录不定性" |
| wire 布局 | 03§2.4（解码侧主）/10§2.2（编码镜像） | 03 | 10 逐项镜像+指回 |
| 引用配对表 | 08§4.2（左半）/09§4.2（右半） | 08 收总 | 09 只列 bind 半 |
| 预算前缀账 | 04§2.4（主）/06§2.3/07§3.4/08§2.2 | 04 | 三处引用具体数值 |
| EPERM 不回 | 05§2.4（服务端主）/10§2.5（客户端镜像） | 05 | 10 对称声明 |
| `\n` 有无 | 06§2.4-2.5（主）/13§2.1（消费侧回声） | 06 | 13 反向约束表述 |
| 双生 devman_dev | 03§2.7（主）/10§2.1（客户端复述指回） | 03 | 10 指回 |
| 状态机三态 | 03§1.1（数值主）/07/08/09（转换各归其篇） | 03 | 转换点各篇 |

### 3.4 越界主题表

| 越界描述 | 所在 | 裁决 |
|---|---|---|
| 02 篇讲 libvtreefs 框架全语义（1642 行中使用面裁剪） | 02 | 合规：A-1 内联后框架是 devman 语义的一部分；"完整框架属 procfs stage"边界已声明；保持 |
| 13 篇讲 devmand 1119 行外部进程行为 | 13 | 合规：plan D-8 明确"外部契约文档化不实现"，篇内边界声明到位；保持 |
| 12 篇讲 RS manager.c 握手段 | 12§2 | 合规：契约面必需，RS 内部实现归 RS stage；保持 |
| 06/07/08 篇互相引用退役机制的历史注记 | 各篇 | 合规：DM-P1-5 演进史是"为什么现在长这样"的教学资产；保持（失效测试行除外，归 G-1） |

### 3.5 非 C 主题逐项回答

| 主题 | 在哪讲 / 为什么不在本 stage |
|---|---|
| 链接与加载 | RS fork+exec 语义归 RS stage；本 stage 持 system.conf 证据（01§2.8/12§2.4） |
| 镜像与内存布局 | 不在本 stage（用户态服务，无自定义段） |
| 汇编入口与陷阱进入 | 系统调用面走 minix-sys（E-DMWIRE）；trap 层归 14-stage-runtime |
| 引导链与引导协议 | 不在 boot_image 是本 stage 的位置事实（00§1.1），协议本体归 kernel stage |
| 启动装配 | A2 三步（01）+ Rust Server::run 装配（09）；生产传输缺（E-DMWIRE，G-7 指针） |
| 构建与工具链 | Makefile WONTFIX（plan §5.5） |
| 跨模块接口与线格式 | 本 stage 主场：wire 三处对读（03/10）、DEVMAN 相位（05）、fsdriver 表（02）、事件行（06/13） |
| 错误路径 | 99§2 错误码表 + 各 handler 失败路径（07 回滚/08 半拆停机位/09 三分支） |
| 关闭与退出 | got_signal/cleanup（01/02）；设备侧退出即 DEL/UNBIND（08/09） |
| 并发与同步 | 单线程事件循环（00§3.1 原则 2；lib.rs 模型文档）；信号上下文不存在（无自设信号 handler 业务） |
| 测试基建 | 80 内联 + 三种 Transport 注入（各篇 §5 + 99）；真机归 E5(h) |

---

## 4. 新目录

### 4.1 新篇章总表（编号不变，15 篇）

> **裁决**：编号即"启动链 + 旅程"双主线，四条硬标准满足（§9 G3/G4）。这是四 stage 中唯一**没有结构性修正需求**的一套——Fix 演化注记已在正文，只需清 3 行失效测试 + 追基线 + 批量锚点修正。**不重排、不重编号、不拆分、不合并、不新建篇章。**

| 编号 | 标题 | 分组 | 操作 |
|---|---|---|---|
| 00 | DEVMAN 整体架构概览 | 阶段 0 | 保持+轻补（G-5 指针） |
| 01 | 启动入口与主循环锚点 | 阶段 1 | 保持+修正（**G-1 表**/G-2/G-3/G-4） |
| 02 | VTreeFS 框架契约 | 阶段 1 | 保持+修正（G-3） |
| 03 | 核心数据结构与 wire 格式 | 阶段 2 | 保持+修正（G-3） |
| 04 | 设备树 | 阶段 2 | 保持+修正（G-3） |
| 05 | 消息面契约 | 阶段 3 | 保持+修正（G-2） |
| 06 | 事件、缓冲与读取 | 阶段 3 | 保持+修正（**G-1 表**/G-2） |
| 07 | 设备添加全流程 | 阶段 4 | 保持+修正（G-2） |
| 08 | 设备删除与回收 | 阶段 4 | 保持+修正（G-2） |
| 09 | 绑定、解绑与装配 | 阶段 4 | 保持+修正（G-2） |
| 10 | 客户端库契约 | 阶段 5 | 保持+修正（G-2） |
| 11 | USB 设备建模 | 阶段 5 | 保持+修正（G-2） |
| 12 | RS 集成契约 | 阶段 6 | 保持+修正（G-3） |
| 13 | devmand 消费契约 | 阶段 6 | 保持（无修正项） |
| 99 | 全局概念收口 | 全局 | 保持+修正（**G-2 表**/G-7） |

### 4.2 阅读路径

- **主线（启动链，全读）**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 99。
- **旅程序（只关心设备生命周期）**：00 → 10 → 07 → 06 → 13 → 12 → 09（00§4 已声明，与主线并行成立，无前向依赖问题——旅程序各篇的前置均在旅程下游篇之前被主线覆盖，跳读者的前置由各篇"过渡"节回填）。
- **并行体声明**：无并行体——双主线交汇于同一编号链；07/08/09 三 handler 是"按生命周期阶段分组"的近并行组，07 为代表成员（最复杂、含回滚与两策）。

---

## 5. 每篇契约

> B 相执行说明：15 篇全部【保持+修正】型——不重写正文。全局性修法声明一次：
> - **锚点修法（G-3）**：`<file>:<错符号>（LNN，工具生成）` → 纯行号或正确符号（与 06/08/09 三 stage 同一修法）。
> - **基线修法（G-2）**：快照型数字（"截至 2026-09-04：59 passed"）保留日期不改；**现状型数字**（99"终态"/README/79 系）刷新为 80 + 分布（K-052）。

### 00-devm-overview【保持+修正】

- 修正清单：§5 追加一行"全 crate 当前口径见 99§6（80 passed，2026-09-19 实测）"（G-5）；其余不动。
- 验收标准：00§5 无"终态"类措辞；导航表与实况一致。

### 01-devm-init-main【保持+修正】

- 修正清单：
  1. **G-1**：§5 测试表删 `first_guard_fires_once` 行（FirstGuard 已随 DM-P1-3 退役；其"一次性语义由 Option<DeviceTree> 承载"正文已有），补 `test_devman_sef_production_hooks` 行（hooks.rs 现测）；
  2. **G-2**：§5.1 "79 passed（截至 2026-09-15）"补"当前 80（见 99§6）"；
  3. **G-3**：7 处工具生成锚点修正（`vtreefs.c:run_vtreefs（L108）`→`vtreefs.c:88-110`、`fs_hooks（L29）`→`vtreefs.h:24-44` 等逐条 sed 定位）；
  4. **G-4**：§4.1 "02-owned 前向缺口（P1 跟踪）"补"（后收窄为传输接线，02§4.4）"。
- 验收标准：§5 表 5 行与 hooks.rs 现测逐一对应；`rg first_guard_fires_once 01-*.md` 零命中。

### 02-vtreefs-framework【保持+修正】

- 修正清单：**G-3**（§2.10 `vtreefs.c:sef_local_startup（L66）`→`vtreefs.c:fs_other` 或纯行号——:66 实为 fs_other；§7 参见同类）；**G-2** §5 末补"当前全 crate 口径 80（99§6）"。
- 验收标准：13 槽表与 §3.5 映射表锚点不变。

### 03-devm-structs【保持+修正】

- 修正清单：**G-3**（§2.4 `devinfo.h:devman_device_info_entry（L32）` 复核）；**G-2** 轻补。
- 验收标准：wire 三坑锚点不变。

### 04-device-tree【保持+修正】

- 修正清单：**G-3**（头部 `device.c:devman_event_read（L16，工具生成）`→`device.c:16-39（静态区）`——:16 是 next_device_id；§7 参见同行）；**G-2** 轻补。
- 验收标准：预算前缀账（112/109）与三串字节级锚点不变。

### 05-devm-message-contract【保持+修正】

- 修正清单：**G-2**（§5.1 "79 passed"补当前口径）。其余不动（本篇是互引中心，正文零改动）。
- 验收标准：三环证据链/相位表锚点不变。

### 06-event-buf【保持+修正】

- 修正清单：
  1. **G-1**：§5 测试表删 `register_resolves_index`/`dispatch_end_to_end` 两行（files.rs 已退役，§4.1 已自注）；表标题"9 测试"改"7 测试（buf 4 + queue 3）"；
  2. **G-2**：§5.1 "59 passed（截至 2026-09-04）"为带日期快照保留，追加当前口径指针。
- 验收标准：`rg register_resolves_index 06-*.md` 零命中；两步 drain/`\n` 不对称锚点不变。

### 07-devm-add-device【保持+修正】

- 修正清单：**G-2**（§5.1 "79"补当前口径）。验收：8 步/unwind_staged/OQ-3 锚点不变。

### 08-devm-del-device【保持+修正】

- 修正清单：**G-2**（同上）。验收：广播先行/ZOMBIE/配对表锚点不变。

### 09-devm-bind-unbind【保持+修正】

- 修正清单：**G-2**（§5.1 "79"补当前口径）。验收：19 容错/ZOMBIE 守卫/路径图锚点不变。

### 10-libdevman-client【保持+修正】

- 修正清单：**G-2**（§5.1 "minix-sys 13 passed"补"其中 devman 侧 12：client 7 + usb 5"现状复核）。验收：五 panic 映射/镜像论锚点不变。

### 11-usb-device-model【保持+修正】

- 修正清单：**G-2**（同 10）。验收：属性拼法/两次 ADD 锚点不变。

### 12-rs-integration【保持+修正】

- 修正清单：**G-3**（7 处工具生成锚点：定位行 `manager.c:rproc（L840）`→`manager.c:840-851（publish 握手）`；`rs_start（L1742）`→`manager.c:1742（init_slot 抄写行）` 等）；**G-2** §5.1 "77 passed（截至 2026-09-04）"保留 + 当前口径指针。
- 验收标准：publish 杀/unpublish 宽容/继承三站锚点不变。

### 13-devmand-consumer【保持+修正】

- 修正清单：无（契约篇，正文与 §4 对照表全部有效；OQ-3 已决标注与 07 一致）。
- 验收标准：反查表 7 行与生产侧测试映射不变。

### 99-devm-global-concepts【保持+修正】

- 修正清单：
  1. **G-2（核心）**：§6 "测试基线（终态）"重写——去掉"终态"措辞，改为"截至 2026-09-19 实测：minix-devman **80 passed**（分布：vtreefs/inode 12、vtreefs/mod 8、server 7、device_tree 7、del 6、add 6、ipc/message 5、hooks 5、wire 4、rs_contract 4、buf 4、bind 4、structs 3、event_queue 3、ipc/dispatch 2）+ minix-sys devman 侧 12（client 7 + usb 5）"；旧 78 分布表删除；
  2. **G-7**：补 edge 状态汇总行（E-DMWIRE open 四缺/E5(h) 验收/E-DMCLIENT 已闭/E-REQWIRE·E-ISWIRE·E-DSWIRE 增补）；
  3. 常量表/错误码表复核（本次 com.h:846 等已验，预计零改动）。
- 验收标准：§6 数字与实测一致且可复跑；无"终态"措辞。

---

## 6. 变更表

| 操作 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向/来源 |
|---|---|---|---|---|---|---|
| N-1 | 修正 | 01§5 测试表 | 原位 | **G-1**：删 1 失效行 + 补 1 新行 | K-052 | hooks.rs 现测清单 |
| N-2 | 修正 | 06§5 测试表 | 原位 | **G-1**：删 2 失效行 | K-052 | event_queue/buf 现测清单 |
| N-3 | 修正 | 99§6 | 原位 | **G-2**：终态 78→80 + 新分布 + 去"终态" | K-052 | cargo 实测 |
| N-4 | 修正 | README:42 | 原位 | G-2：78→80（带日期） | K-052 | 同上 |
| N-5 | 修正 | 01/05/07/08/09/10/11 §5.1 现状型数字 | 原位 | G-2：79→加当前口径；minix-sys 13→12+复核 | K-052 | 同上 |
| N-6 | 修正 | 01/02/03/04/12 工具生成锚点（19 处） | 原位 | G-3 | — | sed 逐条定位 |
| N-7 | 修正 | 01§4.1 | 原位 | G-4：P1 表述补收窄指针 | K-053 | 02§4.4 |
| N-8 | 修正 | 00§5、99（edge 行） | 原位 | G-5/G-7：口径指针 + edge 汇总 | K-053 | edge_todo:705 |
| N-9 | 对账（不改） | todo.md 头部 4694/78 | — | G-6：scan-only 存档不回写 | — | B 相交付说明记录 |

**未执行的操作类型**：重排/拆分/合并/新建/归档均 0 处。draft/ 与 plan/todo 维持现状。

---

## 7. 缺漏新篇（非 C 主题逐项落实）

> §3.5 已逐项回答；本 stage **无需新建篇章**——非 C 主场（wire/协议/契约）已由 03/05/10/11/13 承载，其余各项均有归属或显式排除。

| 主题 | 裁决 | 承载位置 | 验收 |
|---|---|---|---|
| 链接与加载/镜像布局 | 不在本 stage | 00/01 证据面 | 已有 |
| 汇编入口与陷阱进入 | 不在本 stage（E-DMWIRE 面） | 02§3.7/09§3.3 状态表述 + G-7 汇总 | 99 补行 |
| 引导链与引导协议 | 位置事实在本 stage，本体不在 | 00§1.1/01§2.8 | 已有 |
| 启动装配 | 在本 stage | 01/09 | 已有 |
| 构建与工具链 | WONTFIX | plan §5.5 | 不新增 |
| 跨模块接口与线格式 | 本 stage 主场 | 02/03/05/06/10/11/13 | 已有 |
| 错误路径 | 在本 stage | 99§2 + 各 handler 篇 | 已有 |
| 关闭与退出 | 在本 stage | 01/02/08/09 | 已有 |
| 并发与同步 | 在本 stage | 00§3.1 + lib.rs | 已有 |
| 测试基建 | 在本 stage（注入传输）；真机归 E5(h) | 各篇 §5 + 99 | 99 刷新（G-2） |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表

> 编号不变 ⇒ 篇间锚点零迁移。变化全部为原位修正：

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 断链风险 |
|---|---|---|---|---|
| 01§5 / 06§5 测试表 | 3 行退役测试 | 删行 + 1 补行 | 原样替换（G-1） | 无（代码零命中，纯文档内修正） |
| 99§6 / README:42 / 七篇 §5.1 | 78/79 基线 | 80 + 分布 | 原样替换（G-2） | 无 |
| 5 篇 19 处工具生成锚点 | 错符号 | 行号/正确符号 | 原样替换（G-3） | 无（本就解析失败） |
| 01§4.1 / 00§5 / 99 | 过时表述 | 补指针 | 扩写（G-4/G-5/G-7） | 无 |

### 8.2 引用迁移表

| 引用方 | 旧引用 | 新目标 | 验证方式 |
|---|---|---|---|
| `edge_todo.md`（11 处） | 目录/条目名 | 不变 | `rg -c '11-stage-devman' edge_todo.md` 前后一致 |
| `18-commands/12-input/16-drivers/15-fs plan` 等（21 处） | 目录级 | 不变 | 目录名不动 |
| 代码注释 3 处（com.rs:139→05、minix-sys/lib.rs:90→10、:105→11） | 篇名 | 不变 | 文件名不动 |
| 篇内互引 ≈120 处 | `NN-*.md` | 不变 | 重建后 `rg -c` 复跑计数不减 |

### 8.3 断链成本摘要

- **本蓝图方案（保编号）**：受影响外部引用 **0**；代码注释 **0**；篇间互引 **0**。
- **反事实（重编号/重排）**：篇内互引 ≈120 + 外部 14 文件 ≈40 处 + 代码注释 3 处 ≈ **163 处**（四个已审 stage 中最高）；热点 05（13）、06（11）、07/04/03（各 9）。收益为零——双主线结构即状态。维持不动。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**：00 无前置；01→00；02→01；03→02；04→03+02+01；05→01+03；06→01+02+03+05；07→03+04+05+06；08→04+05+06+07；09→05+07+08+10（10 的响应面为后置篇，09§1.1 已声明"交叉引用"性质，主阅读线不依赖）；10→03+05+09；11→10；12→09+05；13→06+04+11+05；99→全部。**注**：09→10 是唯一的前向引用实例，其性质是"转交的另一端"契约对端引用（非阅读依赖），且 10§1 的镜像论回指 09——与既各篇"移交（10/11）"边界声明一致，判定**通过**（与 08-is 的 05→06 声明同型；若汇总轮按最严格口径判定，方案为把 09§1.2 路径图中的 10/11 节点降为纯锚点——列为待裁决 2）。
2. **依赖图检查**：链式偏序 00→01→02→03→04→05→06→{07→08→09}→{10→11}→{12,13}→99；09→10 交叉边不构成环（10 无回指 09 的阅读依赖，仅契约对端引用），**通过**。
3. **覆盖率检查**：§2 池 54 条全部有去向（50 条原篇保留、4 条新增落 99/01/README）；删除项 0（plan §5.5 排除表五项已由 03/05/99 以"挂名不建模"承载）；新增 4 条带实测/edge 锚点，**通过**。
4. **断链成本**：§8.3——方案内 0 断链；反事实 ≈163 处，**通过**。

### 9.2 自检门逐门结果

| 门 | 结果 | 证据 |
|---|---|---|
| G1 C 真序逐条可核对 | **通过** | 抽 10 条：main.c:46-58 fall-through（sed 原文）、bind.c:14/:63/:85（grep）、device.c:213-219 do_reply（sed）、device.c:16 next_device_id=1（grep）、com.h:846 DEVMAN_BASE 0x1200（grep）、generic.c:85 type=0 TODO（grep）、rs/manager.c:840/:897 devman_id（grep）、system.conf:422-429（sed）、vtreefs.h 13 槽（02§2.1 计数核对 `grep -c "(\*"`=13 与正文一致）、device.c:89-91/:122-124 前缀预算（04§2.4 行文与 C 结构吻合，sed 抽验 :91 区） |
| G2 知识点池完整 | **通过** | plan §5.3 的 27 函数+6 静态（回归实证）+ libdevman 16 + libvtreefs 使用面 + devmand 契约面全部入池；排除表五项（A-6 五消息/DYNAMIC/死结构死宏/subsystem/major）由 03/05/99 挂名承载；非 C 制品 10 类逐项有归属 |
| G3 前向引用为零 | **通过（含 1 项待裁决）** | §9.1 第 1 条；09→10 契约对端引用已声明，最严格口径下的降级方案已列 |
| G4 依赖图无环 | **通过** | §9.1 第 2 条 |
| G5 覆盖率 100% | **通过** | §9.1 第 3 条；明确删除项 0 |
| G6 拆合去向/新建来源 | **通过** | 拆分/合并/新建均 0；9 条操作全部写明事实源（rg 零命中、cargo 实测、sed 定位、edge 行号） |
| G7 契约七要素齐全 | **通过** | 15 份契约均为保持型，含定位/讲什么/不讲什么（沿正文边界声明）/前置/后置/事实底线/修正清单+验收标准；共同修法声明一次 |
| G8 迁移表覆盖 | **通过** | §8.1 覆盖全部变化节；§8.2 覆盖外部与代码注释引用（全部 0 迁移） |
| G9 事实断言有锚点 | **通过** | 抽 10 条：80 实测（cargo）、失效行零命中（rg）、hooks.rs 现测 5 名（grep）、C 四处锚点（sed/grep 原文）、19 处锚点分布（grep -c 按篇）、E-DMWIRE open（edge_todo:705）、minix-sys devman 侧 12（grep -c 两文件）、README:42 旧数（grep）、互引 top（rg -c 循环）、plan §6.1 全 reviewed（读表）。推测项：无；待复核项：10/11 的 minix-sys"旧 1"测试归属（写入契约修正清单） |

### 9.3 结论与待用户裁决的问题

**结论**：本蓝图已完成，可交付 B 相：15 篇全部保持+定向修正（合计 9 条操作、约 30 处落点，全部带锚点与验收标准），0 篇重写，0 处结构变动。这是四个已审 stage 中修正量最小的一套——核心工作只有 01/06 两篇的 3 行失效测试行清理与 99/README 基线刷新。

**待用户裁决**：

1. **"保编号、清失效行、追基线"方案是否采纳**（glm 建议采纳）：本 stage 是"文档-代码同步纪律"的最佳样本（Fix 注记当日回写正文），唯一实质缺陷是 2026-09-18 刷新轮漏清了 3 行已退役测试的表格行——属机械遗漏而非方法缺陷。
2. **09→10 前向引用的口径**（G3 门注）：glm 建议维持现状（契约对端引用 + 双向回指，且 00§4 导航表已声明"旅程序"），不降级为纯锚点；若汇总轮按最严格"零前向"口径执行，按 §9.1 的降级方案处理 09§1.2 一处即可。
3. **失效测试行的修复归属**：glm 建议随 B 相文档批一次执行（3 行删除 + 1 行补增，机械操作）；不属代码缺陷，无需开 todo 条目。
4. **plan.md / todo.md 不改**（G-6 同型裁决，与前三轮一致）：由 B 相交付说明记录对账（todo 4694/79 为审计时点事实）。

---

*（蓝图完。执行者 glm，2026-09-19，基线 commit d6ecd22ca。本文件是 11-stage-devman 目录内唯一的 `_glm` 产物。）*
