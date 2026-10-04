# 03-stage-rs 文档重建蓝图（qwen）

> R 相产物。只输出重建蓝图，不改任何正文。B 相按本蓝图逐篇重写。
> **重写**: 2026-10-03（GAP-2 SEF 框架归属改判同步；其余内容沿用初版蓝图）

## 0. 元数据

- **执行者**：qwen
- **日期**：2026-09-19
- **目标目录**：`notes/rewrite/fork-syscall-rewrite/03-stage-rs`
- **仓库根目录**：`/home/xzhao/github/minix-rs`
- **当前提交号**：`561cf097b`
- **修订记录（本轮，用户裁决）**：GAP-2 **SEF 框架归属改判**——原判属 `14-stage-runtime`，现取证该 stage 未讲框架本体（`14-stage-runtime/plan.md:231` 把 `sef*.c` 列入排除表，同文件 `:277` 又判给不存在的 "minix-sef stage"，00–13/99 无一讲述），故按 §9 待裁决问题 2 预留的 fallback 分支改判为**本 stage 尾部新增 `20-rs-sef-framework.md`**（尾附编号，不顺移 00–19/99，§4.1 成本裁决不破）。同步点：§2.8 K-125、§2.10、§3.2 GAP-2、§4.1、§4.2、§4.3、§5（00/01/12/18 四篇契约 + 新增 20 契约）、§7（前言例外 + GAP-2 行 + 新增 GAP-5）、§8.1、§8.2、§9。仍为 R 相产物：**未改任何正式篇章正文**。
- **阶段类型判定**：**服务事件循环型 + 启动链型混合**（RS 既有一次性 boot 四步，又有永续主循环；boot 与主循环都是线性/分派结构）。按提示词 §九，主线用"启动时序 + 一次请求生命周期"双骨架，Live Update / 自升级 / 外部依赖面归次线与支线。

### 审查范围

- **算作文档**（受本蓝图重排）：`00-rs-overview.md`、`01`~`19`、`99-rs-global-concepts.md`（共 21 篇编号文档）。
- **算参考材料**（不重排，只作知识/边界来源）：`plan.md`、`todo.md`、`draft/README.md`。
- **范围外**：`.design/`、其它 AI 的 `doc_rerank_deepseek.md` / `doc_rerank_glm.md`（按落盘规则禁止读取，本蓝图未读取其内容）；`minix3/` C 源码只读不改。

### 读取清单

| 类别 | 实际读取 |
|------|---------|
| 目标目录全部文档 | 21 篇编号文档的头部声明 + 全部 `##`/`###` 小节骨架（section 级全文清点）；`00`/`11`/`13` 正文精读；`01`/`02`/`03` 小节级精读 |
| Minix3 C 源码 | `minix3/minix/servers/rs/`：8 个 `.c`（6307 行，`wc -l` 实证：main 834 / manager 2332 / request 1309 / update 1011 / utility 547 / exec 165 / error 59 / table 50）+ 头 `const.h`/`type.h`/`glo.h`/`proto.h`/`inc.h`；协议头 `include/minix/{rs,com,sef,ipc_filter,priv}.h`；`kernel/table.c`、`kernel/priv.h`、`kernel/system/do_privctl.c`。逐函数清单见步骤 1 附录 |
| 非 C 制品 | `servers/rs/Makefile`、`table.c` boot 三表、`lib/libsys/sef*.c`、`lib/libexec/exec_elf.c`、`lib/libc/sys/stack_utils.c`、消息槽 `include/minix/ipc.h`（`mess_rs_*`/`mess_lsys_*`） |
| 阶段边界材料 | `00-master-plan/README.md`（boot 因果链、阶段表）、`edge_todo.md`/`edge1-4.md`（跨阶段）、`03-stage-rs/plan.md`（§1.2/1.3 主线、§2 编号、§5.4 排除项）、`todo.md`（EDGE 清单 §3.6） |
| 前一 stage overview | `02-stage-vm/00-vm-overview.md`（VM 是 RS 前置，页表/`RTS_VMINHIBIT` 已在 01-stage-kernel/02-stage-vm 讲，RS 不重复）、`01-stage-kernel/09-vm-boot-protocol.md`、`22-privilege.md`、`23-ipc-filter.md` |
| Rust 实现入口 | `os/servers/rs/src/`（28 个模块，逐模块对账）、`os/servers/rs/Cargo.toml`（`live-update` feature，A-11）、`os/libs/minix-types/src/ipc/rs.rs`、`os/libs/minix-sys` |

### 关键命令与证据（摘录）

- `wc -l minix3/minix/servers/rs/*.c` → 8 文件 6307 行（与 `00` §5.5 覆盖契约一致）。
- C 函数清单：`grep` 逐文件定义提取（见步骤 1 附录，共 ~120 个函数定义，与 `plan.md §7.2 R-5` 的"121 个函数"同量级）。
- 断链面统计（本蓝图实测，见 §8）：编号文档之间 `NN-rs-*.md` 文件名引用 **375 处**；`NN §` 简写章节引用 **44 处**；`os/servers/rs/src/*.rs` 代码注释对文档文件名引用 **134 处**（含 1 处失效 `10-rs-service-convert.md`）。

---

## 1. C 真序（运行时真序重建）

> 直接取自 C 源码，不从现有文档转述。RS 是"启动链 + 事件循环"混合型，故分两段。锚点为 `minix3/minix/servers/rs/` 下文件:行。

### 1.A 启动段（一次性，`main.c` + `sef_cb_init_fresh`）

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| S-01 | 进程入口 `main()` | `main.c:38` | 三段式：取活/干活/回活 |
| S-02 | `sef_local_startup()` 注册 SEF 回调 | `main.c:51`→`main.c:136` | 注册 init/restart/lu/signal/response 回调 |
| S-03 | `sys_getmachine(&machine)` | `main.c:53` | 取机器信息，失败 panic |
| S-04 | 进入 `sef_cb_init_fresh()` | `main.c:158` | boot 锚点 |
| S-05 | `env_parse("rs_verbose")` + `sys_getinfo(GET_HZ)` | `main.c:179,181` | 配置与系统频率 |
| S-06 | `cpf_grant_direct` 建 `rinit.rproctab_gid` | `main.c:185` | rprocpub 表对外读授权 |
| S-07 | `RUPDATE_INIT()` + `shutting_down=FALSE` | `main.c:192-193` | update 全局复位 |
| S-08 | `sys_getimage(image)` 读 boot_image | `main.c:196` | 从内核取 boot 映像表副本 |
| S-09 | 计数并核对 boot_image / boot_image_priv 匹配 | `main.c:200-227` | 服务数与 priv 条目数必须相等 |
| S-10 | 重置 rproc/rprocpub 表 | `main.c:230-237` | r_flags=0、r_init_err=ERESTART、pub 指针挂接 |
| S-11 | **Step 1**：逐 boot 服务建 priv（`static_priv_id`、`s_flags`、`SRV_OR_USR`、`fill_send_mask`、`fill_call_mask`）+ `sys_privctl(SYS_PRIV_SET_SYS)`（RS/VM 例外跳过）+ `sys_getpriv` 同步 | `main.c:244-296` | 权限构造，见 `03`/`05` |
| S-12 | Step 1 续：sys/dev 属性、`build_cmd_dep`、`fill_call_mask(VM)`、调度参数、默认字段、`RS_IN_USE\|RS_ACTIVE`、`rproc_ptr[]` | `main.c:298-346` | 槽位落地，见 `02`/`08` |
| S-13 | **Step 2**：允许运行（sched + `SYS_PRIV_ALLOW`；`init_service` 发 RS_INIT + `catch_boot_init_ready` 同步捕获） | `main.c:348-` | 见 `12`/`03` |
| S-14 | **Step 3**：catch 剩余 init ready | `main.c`（Step 3 段）| 见 `12` |
| S-15 | **Step 4**：`getnpid` 补动态服务 pid | `main.c`（Step 4 段）| 见 `02`/`19` |
| S-16 | `sys_setalarm(RS_DELTA_T)` 起周期检查 | `main.c`（Step 4 后）| 见 `07` |
| S-17 | `USE_LIVEUPDATE`：`clone_slot` + `srv_fork` 建 RS 自升级副本 | `main.c:436-491` | RS 自举副本，见 `18` |

### 1.B 循环段（永续，`main.c:56-130` while 主循环）

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| L-01 | `rs_idle_period()` 空闲后台（清 RS_DEAD、补 replica） | `main.c:59`→`utility.c:443` | 见 `06`/`15` |
| L-02 | `get_work(&m,&ipc_status)` → `sef_receive_status(ANY)` | `main.c:62`→`main.c:826` | 阻塞收活，见 `06` |
| L-03 | `rs_isokendpt(who_e,&who_p)` 校验来源 | `main.c:64`→`utility.c:352` | 见 `02` |
| L-04 | **notify 分支**：CLOCK→`do_period`；其它→心跳 `r_alive_tm` | `main.c:80-93`→`request.c:943` | 见 `07` |
| L-05 | **ready 分支**：`RS_INIT`→`do_init_ready`；`RS_LU_PREPARE`→`do_upd_ready` | `main.c:116-117`→`request.c:462,890` | 见 `12`（update 臂→`16`） |
| L-06 | **控制/查询分支**：`RS_UP/DOWN/REFRESH/RESTART/SHUTDOWN/UPDATE/CLONE/UNCLONE/EDIT/SYSCTL/FI/GETSYSINFO/LOOKUP` | `main.c:102-114`→`request.c` | 控制面 `13`、查询面 `14`、`RS_UPDATE`→`16` |
| L-07 | `result != EDONTREPLY` → `reply(who_e,NULL,&m)` | `main.c:125-128`→`utility.c:309` | late reply 走 `utility.c:332` |

**真序结论**：现有 `00` §2 主线图与 `plan.md §1.2` 与本重建**逐条一致**（S-01~S-17、L-01~L-07 全部命中锚点）。这说明文档的"运行时真序"没有失真——本 stage 的文档问题不在"顺序错"，而在 §3 覆盖审计列出的"重复展开 / 混杂职责 / 前向引用簇 / 编辑缺陷"。

### 1.C 服务生命周期次主线（单次请求生命周期，`RS_UP` 触发的编排链）

`do_up`(`request.c:15`) → `check_call_permission`(`manager.c:81`,`04`) → `check_request`+`copy_rs_start`+`init_slot`(`request.c:1265`/`manager.c:135,1708`,`08`) → `create_service`(`manager.c:531`,11 步 fork/priv/sched/exec/VM/pin,`10`) → `read_exec`/`srv_execve`(`manager.c:1372`/`exec.c`,`09`) → `publish_service`(`manager.c:787`,`11`) → `start_service`→`run_service`→`init_service`(RS_INIT 握手,`manager.c:950,923`/`utility.c:18`,`12`) → `do_period` 心跳(`request.c:943`,`07`) → `stop_service`(`manager.c:988`,`13`) → `terminate_service`→`cleanup`/`reincarnate`/`backoff`(`manager.c:1055,405,1033`,`15`) → `RS_UPDATE`→`do_update`→prepare/update/init/end(`request.c:534`/`update.c`,`16/17/18`)。此链与 `00` §3 生命周期图一致。

---

## 2. 知识点全集（知识点池）

> 类型：概念 / 机制 / 数据结构 / 接口协议 / 约束不变量 / 架构演进 / 工具工程 / 测试。
> 来源类型：**存量**（来自现有文档）/ **新增**（现有文档缺、由覆盖审计发现）。
> 一条知识点 = 读者能回答的一个问题。粒度取"章节级去重后的概念"。

### 2.1 启动与数据底座

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-001 | RS 身份：root system process，`RS_PROC_NR=2` | 概念 | 存量 | 00 §1.1 | `com.h:RS_PROC_NR`,`:77` | RS 是谁、boot 排第几 |
| K-002 | boot 链两层顺序（登记 vs 执行）：VM→RS→其余 | 概念 | 存量 | 00 §1.2 | `kernel/table.c:44-64`,`kernel/main.c` | 为什么 VM 先于 RS |
| K-003 | 两个状态机：boot 四步 + 主循环 | 概念 | 存量 | 00 §1.3 / 01 | `main.c:158,56` | RS 一辈子在做什么 |
| K-004 | `main()` 三段式（取活/干活/回活） | 机制 | 存量 | 01 §2.1 | `main.c:38-131` | 主循环骨架 |
| K-005 | SEF 回调注册 `sef_local_startup` | 机制 | 存量 | 01 §2.2 | `main.c:136` | RS 如何挂 SEF |
| K-006 | `sef_cb_init_fresh` 四步 boot | 机制 | 存量 | 01 §2.3 | `main.c:158-494` | boot 全流程 |
| K-007 | boot 三表 boot_image/priv/sys/dev + `boot_image_info_lookup` | 数据结构 | 存量 | 01 §2.7/§2.4 | `table.c:15-50`,`main.c:709` | boot 配置从哪来 |
| K-008 | `catch_boot_init_ready` boot 同步捕获 | 机制 | 存量 | 01 §2.5/12 §2.5 | `main.c:784-821` | boot 期怎么等 ready |
| K-009 | `rproc`/`rprocpub` 双表模型 | 数据结构 | 存量 | 02 §2.3/§2.4 | `type.h:56-108`,`rs.h:rprocpub` | RS 怎么记住服务 |
| K-010 | `rproc_ptr[NR_PROCS]` 快速索引 | 数据结构 | 存量 | 02 §1.4/§2.9 | `manager.c`（lookup/alloc/free） | endpoint→slot 反查 |
| K-011 | `r_flags` 16 位（RS_IN_USE…REINCARNATE） | 数据结构 | 存量 | 02 §2.5 | `const.h:28-43` | 槽状态位 |
| K-012 | `sys_flags`/`SF_*` 13 位 + 预设组合 | 数据结构 | 存量 | 02 §2.6 | `rs.h:SF_CORE_SRV`,`const.h:65-68` | 服务策略位 |
| K-013 | 槽位原语 `lookup_slot_by_*`/`alloc_slot`/`free_slot` | 机制 | 存量 | 02 §2.9 | `manager.c:1935-2109` | 槽生命周期 |
| K-014 | `rs_isokendpt` 端点校验 | 机制 | 存量 | 02 §2.10 | `utility.c:352-359` | 来源合法性 |
| K-015 | `get_service_instances` 实例收集 | 机制 | 存量 | 02 §2.11/15 §2.7 | `manager.c:1334` | 多副本枚举 |
| K-016 | `rprocupd`/`rupdate` 更新描述符链 | 数据结构 | 存量 | 02 §2.7 **且** 17 §2.2（重复）| `type.h:30-54` | LU 链数据形状 |
| K-017 | `rinit` 全局 init 描述符 | 数据结构 | 存量 | 02 §2.8 | `sef.h:42-53`,`main.c:185` | boot 期授权上下文 |

### 2.2 权限与隔离

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-020 | `struct priv` 字段模型（RS 读写的子集） | 数据结构 | 存量 | 03 §2.1 | `kernel/priv.h:21-66` | priv 里有什么 |
| K-021 | boot Step1 构造 priv（`static_priv_id`/flags/mask） | 机制 | 存量 | 03 §2.4 | `main.c:240-345` | 权限怎么建 |
| K-022 | `privctl` 操作面（SET_SYS/UPDATE_SYS/ALLOW/DISALLOW/YIELD/SET_USER/CLEAR_IPC_REFS） | 接口协议 | 存量 | 03 §2.6 | `com.h:SYS_PRIV_*`,`do_privctl.c` | 谁能改权限 |
| K-023 | `fill_send_mask`/`fill_call_mask` | 机制 | 存量 | 03 §2.5 | `utility.c:82-141` | 掩码填充原语 |
| K-024 | `sched_init_proc` 调度初始化 | 机制 | 存量 | 03 §2.7 | `utility.c:364-382` | 新槽调度参数就位 |
| K-025 | `update_sig_mgrs` 信号管理器更新 | 机制 | 存量 | 03 §2.8/18 §2.6 | `utility.c:387-422` | 信号委派更新 |
| K-026 | 两级授权：`caller_is_root`/`caller_can_control`/`check_call_permission` | 机制 | 存量 | 04 §2 | `manager.c:21-130` | 谁有资格命令 RS |
| K-027 | 隔离策略 `r_control` 目标槽规则（5 条） | 约束不变量 | 存量 | 04 §2 | `manager.c:81-130` | 目标槽控制判定 |
| K-028 | `getnuid` root 判定外部依赖 | 接口协议 | 存量 | 04 §2 | `libsys/getepinfo.c` | uid 从哪来 |
| K-029 | IPC 发送掩码两条路径（ALL_M 快捷 vs `r_ipc_list` 解析） | 机制 | 存量 | 05 §2 | `main.c:272`,`manager.c:2112-2331` | 服务能发给谁 |
| K-030 | `init_privs`/`add_forward_ipc`/`add_backward_ipc`/`get_next_name` | 机制 | 存量 | 05 §2 | `manager.c:2157-2331` | 动态 IPC 图构建 |

### 2.3 主循环与监控

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-040 | 主循环四类消息分类（notify/ready/控制/查询） | 机制 | 存量 | 06 §2 | `main.c:70-129` | 消息如何分流 |
| K-041 | `reply`/`late_reply`/`EDONTREPLY` 回复协议 | 接口协议 | 存量 | 06 §2 | `utility.c:309-341`,`errno.h:199` | 何时不回/迟到回 |
| K-042 | `rs_idle_period`/`rs_is_idle` 空闲后台 | 机制 | 存量 | 06 §2 | `utility.c:424-479` | 空闲期做什么 |
| K-043 | `do_period` 三分支状态机（backoff 复活/SIGTERM→SIGKILL/ping 超时 crash） | 机制 | 存量 | 07 §2 | `request.c:943-1049` | 看门狗如何判活 |
| K-044 | alive/check/stop 三时间戳 + backoff 位 | 数据结构 | 存量 | 07 §2 | `const.h:34-51`,`manager.c` | 心跳状态载体 |
| K-045 | `do_sigchld` 子进程清理 | 机制 | 存量 | 07 §2 | `request.c:1051-1090` | 僵尸回收 |
| K-046 | `update_period` LU 准备超时 | 机制 | 存量 | 07 §2 | `update.c:371-397` | update 期心跳例外 |
| K-047 | init 超时与 free pass 例外 | 约束不变量 | 存量 | 07 §2 | `request.c:943-1049` | 首心跳宽限 |

### 2.4 服务创建与配置（生命周期创建链）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-050 | `rs_start` 请求参数结构 + `check_request` | 数据结构 | 存量 | 08 §2 | `rs.h:104-151`,`request.c:1265` | 用户声明长什么样 |
| K-051 | `copy_rs_start`/`copy_label` 拷入 | 机制 | 存量 | 08 §2 | `manager.c:135-173` | 参数进槽 |
| K-052 | `init_slot`/`edit_slot` 字段落地 | 机制 | 存量 | 08 §2 | `manager.c:1708,1460` | 声明→槽状态 |
| K-053 | `build_cmd_dep` 命令/依赖解析 | 机制 | 存量 | 08 §2 | `manager.c:289-324` | 命令串怎么拆 |
| K-054 | `inherit_service_defaults` 默认值继承 | 机制 | 存量 | 08 §2 | `manager.c:1303-1327` | 副本继承默认 |
| K-055 | `read_exec`/`share_exec`/`free_exec` 映像副本生命周期与共享 | 机制 | 存量 | 09 §2 | `manager.c:1372,1357,1424` | 二进制进内存 |
| K-056 | `srv_execve`/`do_exec`/`exec_restart`/`read_seg` exec 全链 | 机制 | 存量 | 09 §2 | `exec.c:21,62,121,143` | 映像 exec 进子进程 |
| K-057 | `SF_USE_COPY`/`SF_NEED_COPY` 共享/独占复制 | 约束不变量 | 存量 | 09 §2 | `rs.h` | 何时复用副本 |
| K-058 | `create_service` 11 步编排（fork/priv/sched/exec/VM/pin） | 机制 | 存量 | 10 §2 | `manager.c:531-786` | 服务如何诞生 |
| K-059 | `clone_service`/`activate_service`/`clone_slot`/`swap_slot` | 机制 | 存量 | 10 §2 | `manager.c:713,1013,1800,1870` | 副本与槽交换 |
| K-060 | `publish_service`/`unpublish_service` 四个"是否发布"谓词 | 机制 | 存量 | 11 §2 | `manager.c:787,864` | 服务如何被找到 |
| K-061 | 发布四目录：DS label / mapdriver / PCI ACL / devman bind | 接口协议 | 存量 | 11 §2 | `libsys/mapdriver.c`,`ipc.h:1473`,`com.h:DEVMAN_BIND` | 发布写到哪 |
| K-062 | RS_INIT ready 握手协议（boot 与运行时两处使用） | 接口协议 | 存量 | 12 §1.2/§2 | `utility.c:18-64`,`request.c:462-533` | 初始化如何上报 |
| K-063 | `do_init_ready` 四步 + `do_upd_ready` update 就绪 | 机制 | 存量 | 12 §2.3/§2.4 | `request.c:462-529,890-938` | ready 处理 |
| K-064 | `end_srv_init` 收尾 + `run_service`/`start_service` | 机制 | 存量 | 12 §2.1/§2.6 | `manager.c:923,950,328` | 启动编排 |
| K-065 | RS 自模拟 ready（`sef_cb_init_response`/`lu_response`） | 机制 | 存量 | 12 §1.3 | `main.c:591-626` | RS 给自己发 ready |

### 2.5 控制面与查询面

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-070 | 控制 handler 族 `do_up/down/restart/refresh/shutdown/clone/unclone/edit` | 机制 | 存量 | 13 §2 | `request.c:15-460` | 生命周期控制入口 |
| K-071 | `stop_service` SIGTERM→SIGKILL 升级 | 机制 | 存量 | 13 §2 | `manager.c:988-1012` | 停止如何升级 |
| K-072 | 服务生命周期次主线路径图 | 概念 | 存量 | 13 §2 | plan §1.3 | 一次旅程全景 |
| K-073 | 查询 handler 族 `do_lookup/getsysinfo/sysctl/fi` | 机制 | 存量 | 14 §2 | `request.c:1095-1264` | 只读观测面 |
| K-074 | `fi_service` 故障注入 | 机制 | 存量 | 14 §2 | `utility.c:69-81` | 把服务搞崩 |
| K-075 | `print_services_status`/`print_update_status` 输出 | 工具工程 | 存量 | 14 §2 | `utility.c:485-546` | 状态可读化 |

### 2.6 终止与恢复

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-080 | `terminate_service` 决策树 | 机制 | 存量 | 15 §2.1 | `manager.c:1055` | 死掉的服务怎么处置 |
| K-081 | `cleanup_service` 两段式清理 | 机制 | 存量 | 15 §2.2 | `manager.c:405` | 资源回收 |
| K-082 | `kill_service`/`crash_service`/`detach_service`（RS 主动处决 + 降级旁路） | 机制 | 存量 | 15 §2.3/§2.4 | `manager.c:360,380,497` | 处决与降级 |
| K-083 | `restart_service`/`reincarnate_service` 两条恢复路径 | 机制 | 存量 | 15 §2.5 | `manager.c:1246,1033` | 复活机制 |
| K-084 | `run_script` 恢复脚本执行器 | 机制 | 存量 | 15 §2.6 | `manager.c:1185` | 崩溃后跑脚本 |
| K-085 | backoff 恢复位策略 + `MAX_DET_RESTART` | 约束不变量 | 存量 | 15 §2.5 | `const.h:50-51`,`manager.c` | 重试退避 |
| K-086 | 终止/恢复状态机图 | 概念 | 存量 | 15 §1.2/§2 | plan §1.3 | 终局全景 |

### 2.7 Live Update

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-090 | LU 状态机 prepare→update→init→end/rollback | 机制 | 存量 | 16 §1.2/§2 | `update.c` | 不停机替换全景 |
| K-091 | `do_update` 入口与校验 | 机制 | 存量 | 16 §2.1 | `request.c:534-889` | LU 怎么触发 |
| K-092 | `rupdate_*` 链操作（clear/add/set/init/move） | 数据结构 | 存量 | 16 §2.2 | `update.c:7-183` | 更新链维护 |
| K-093 | `update_service`/`rollback_service` 槽交换 | 机制 | 存量 | 16 §2.3 | `update.c:262,330` | 新旧实例切换 |
| K-094 | prepare 阶段 + `start_update*` 族 | 机制 | 存量 | 16 §2.4-2.6 | `update.c:401-706` | 准备流程 |
| K-095 | `end_update` 族（按 phase 分派） | 机制 | 存量 | 16 §2.8 | `update.c:744-1011` | 收尾分派 |
| K-096 | `abort_update_proc` 中止 | 机制 | 存量 | 16 §2.7 | `update.c:707` | 中止回滚 |
| K-097 | VM multi-component 支持 | 接口协议 | 存量 | 16/18 | `update.c:230-257` | 多 VM 实例 LU |
| K-100 | `init_state_data` 状态迁移总流程 | 机制 | 存量 | 17 §2.1 | `manager.c:174-284` | 旧记忆交给新版 |
| K-101 | 三种状态载体（eval 表达式 / IPC filter 表 / 自定义数据） | 数据结构 | 存量 | 17 §1.2/§2 | `rs.h:88-100` | 迁什么状态 |
| K-102 | IPC filter 解析（ANY_USR/SYS/TSK） | 机制 | 存量 | 17 §2.4 | `ipc_filter.h` | 过滤器迁移 |
| K-103 | state grants 生命周期（cpf_grant/revoke） | 接口协议 | 存量 | 17 §2.6 | `request.c:805`,`update.c` | 授权随迁移 |
| K-104 | RS 自身 boot 自升级（USE_LIVEUPDATE 分支） | 机制 | 存量 | 01 §2.3.7 / 18 §2.1 | `main.c:436-491` | RS 换自己 |
| K-105 | `sef_cb_init_restart`/`sef_cb_init_lu` 回调 | 机制 | 存量 | 18 §2.2/§2.3 | `main.c:499,549` | RS 重启/ LU 初始化 |
| K-106 | RS rollback 特例（`sys_whoami`/vm_update ROLLBACK） | 机制 | 存量 | 18 §2.4 | `update.c:330-366` | RS 回滚异于常服务 |
| K-107 | 备份信号管理器（RS clone 的 s_bak_sig_mgr） | 约束不变量 | 存量 | 18 §2.6 | `manager.c:760-780` | 信号管理不自锁 |

### 2.8 外部接口与全局概念

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-110 | sys_* 内核面（privctl/getpriv/getimage/getinfo/kill/setalarm/statectl） | 接口协议 | 存量 | 19 §2.1 | `libsys/sys_*.c` | RS→kernel 依赖面 |
| K-111 | srv_* PM 面（`srv_fork`/`getnpid`） | 接口协议 | 存量 | 19 §2.2 | `libsys/srv_fork.c` | RS→PM 依赖面 |
| K-112 | vm_* VM 面（`vm_memctl`/`set_priv`/`prepare`/`update`） | 接口协议 | 存量 | 19 §2.3 | `com.h:VM_RQ_BASE` | RS↔VM 协议 |
| K-113 | ds_* / sched_* / libexec / mapdriver / PCI / devman 面 | 接口协议 | 存量 | 19 §2.4-2.6 | `ds.c`,`sched_*.c`,`pci_*.c` | 其余外部面 |
| K-114 | `mess_rs_*` / `mess_lsys_*` 消息槽类型化（A-2） | 接口协议 | 存量 | 19 §3 | `ipc.h:1858-1906,1048-1072` | 线格式 |
| K-120 | RS_* 15 消息类型全表 | 数据结构 | 存量 | 99 §2.1 | `com.h:463-492` | 消息常量 |
| K-121 | 时间与状态机常量族（`RS_DELTA_T`/`RS_SRV_IS_IDLE`…） | 数据结构 | 存量 | 99 §2.2 | `const.h` | 常量 |
| K-122 | 服务标志族 RSS_*/SF_* 权威位置 | 数据结构 | 存量 | 99 §2.3 | `rs.h`,`sef.h` | 标志单一来源 |
| K-123 | 系统操作码 + endpoint 模型 | 数据结构 | 存量 | 99 §2.4 | `com.h` | 操作码/端点 |
| K-124 | 错误表（`rs_strerror`/`init_strerror`/`lu_strerror`） | 数据结构 | 存量 | 99 §2.5 | `error.c:33,48,56` | 错误码可读 |
| K-125 | SEF 框架（`sef_startup`/`sef_local_startup`/四类拦截/回调生命周期/`SEF_INIT_*`/`SEF_LU_*`/出生回报三形态） | 概念 | 存量（分散）→ **新增篇主讲述点 20** | 现散在 01 §1.3/§2.2 + 12 + 18 + 19（GAP-2 改判，见 §3.2；框架原理归 20，各篇只留简述） | `libsys/sef*.c`（9 个 `.c` 共 2210 行，`wc -l` 实证）,`sef.h:1` | 读懂任何一个用户态服务之前的第 0 页：框架在启动期收谁的消息、在收信门口拦哪四类、把什么交回服务自己 |

### 2.9 ARCH 演进知识点（贯穿，来源=存量）

| 编号 | 名称 | 现有位置 | 涉及文档 |
|------|------|---------|---------|
| K-130 | A-1 fork 语义（no_std 无 libc fork） | 00 §5.4 | 10/15 |
| K-131 | A-2 `mess_rs_*`→minix-types typed enums | 00 §5.4 | 99/19 |
| K-132 | A-3 裸指针四链→`Vec`+`Option<SlotId>` | 00 §5.4 | 02/10/16 |
| K-133 | A-4 `rproc_ptr[]`→数组+索引 | 00 §5.4 | 02 |
| K-134 | A-5 exec image 共享→`Arc<[u8]>` | 00 §5.4 | 09 |
| K-135 | A-6 update flags→`UpdatePhase` enum | 00 §5.4 | 16 |
| K-136 | A-7 SEF→minix-rs 抽象 | 00 §5.4 | 01/12/18 |
| K-137 | A-8 libexec→`minix-elf` crate | 00 §5.4 | 09 |
| K-138 | A-9 超时接收 `receive_timeout` | 00 §5.4 | 16 |
| K-139 | A-10 PCI defer + `PciAcl` fail-closed | 00 §5.4 | 11/99 |
| K-140 | A-11 编译宏→cargo feature `live-update` | 00 §5.4 | 01/99 |
| K-141 | A-12 errno+panic+printf→`Errno`/`Result` | 00 §5.4 | 99/各篇 §3 |
| K-142 | A-13 boot 三表→静态 `table.rs` | 00 §5.4 | 01/02 |
| K-143 | A-14 状态数据协议常量/安全默认（56 vs 28 布局） | 00 §5.4 | 17 |

### 2.10 统计摘要

- 存量知识点约 **95 条**（K-001~K-143，去重后）；新增知识点见 §3 覆盖缺口（本 stage 覆盖度高，新增少）。
- 按现有文档分布：01(11) / 02(10) / 03(6) / 04(3) / 05(2) / 06(3) / 07(5) / 08(5) / 09(3) / 10(2) / 11(2) / 12(4) / 13(3) / 14(3) / 15(7) / 16(8) / 17(4) / 18(4) / 19(5) / 99(6)。**20 为本次修订新增篇**（K-125 主讲述点，不占存量计数）。
- 重复知识点：K-016（rprocupd：02 与 17 双展开）、K-025（update_sig_mgrs：03 与 18）、K-062/K-008（ready：01 与 12）、K-002/K-003（boot 主线/两状态机：00 与 01 双展开）、**K-125（SEF 框架：01 §1.3/12/18/19 四处分散，GAP-2 改判归 20）**。

---

## 3. 覆盖审计

### 3.1 主题全集来源（四路对账）

1. C 符号：8 文件全函数（步骤 1 附录 ~120 个定义，见 §附录）；
2. OS 通用概念：进程/服务状态机、地址空间、权限模型、看门狗、热更新状态迁移；
3. 非 C 制品：Makefile、boot 三表、SEF 库、libexec/stack_utils、消息槽；
4. 边界契约：`plan.md §5` 覆盖契约（8 .c / 15 消息 / 16 r_flags / 13 SF / 121 函数）。

对账结论：**C 源码函数面与文档映射几乎全覆盖**（`00` §5.5 覆盖契约成立）。缺口集中在"跨篇职责边界"与"非 C 主题定位"，不在"C 机制遗漏"。

### 3.2 覆盖缺口表

| 编号 | 缺口主题 | 证据 | 建议 | 落实 |
|------|---------|------|------|------|
| GAP-1 | "update-in-progress 全局状态门控"（`RUPDATE_IS_UPDATING`）被 04/07 前向引用（`04` 声明引用 `const.h:RUPDATE_IS_UPDATING`，`07` 引用 `update_period`），但完整解释迟至 16 | 04/07 头前置 + 正文引用 16 | 把"**存在一个 update 进行中标志位、置位期间对外请求返回 EBUSY / 心跳走 update_period 例外**"这一状态语义在 **02（数据底座，r_flags/update 状态位）**首次讲完整；16 只展开状态机 | 新增 K-018 |
| GAP-2 | SEF 框架作为 RS 前置概念无独立归属，分散在 01 §1.3/§2.2 + 12 + 18 + 19 | K-125 分散；**本轮取证补充**：`wc -l minix3/minix/lib/libsys/sef*.c` = 2210 行（9 个 `.c`）；`grep SEF notes/rewrite/fork-syscall-rewrite/14-stage-runtime/[0-9]*.md` 只命中 `plan.md` 的排除/映射行，正式篇章 0 讲述；`os/libs/minix-sef/src/lib.rs`（438 行）只实现 `sef_receive_status` 拦截核心 + ping 应答 | **改判：归本 stage 尾部新增 `20-rs-sef-framework.md`**（原判 `14-stage-runtime` 不成立——该 stage 实测未讲，见 §0 修订记录）。改判的两条 RS 侧正当理由：①RS 是**唯一注册全部 7 个回调**的服务（`main.c:136-152`）；②SEF 库内部**专门为 RS 开分支**（`ROOT_SYS_PROC` → `do_sef_rs_init`，sef.c:107-114），且 init/LU/ping 三条腿的对端都是 RS（sef.c:127、sef_init.c:463/477、sef.c:189、sef_liveupdate.c:559）。01 §1.3/§2.2 收缩为"RS 注册了哪些回调 + 为什么全量"，框架本体归 20 | 边界决策**就地改判**（不重编号，见 §4.1 例外）；跨 stage 简述化登记为 GAP-5 后续批次 |
| GAP-3 | Makefile / 构建-工具链在正式文档无一句交代（仅 `plan §5.4` WONTFIX） | `servers/rs/Makefile` | 在 `00` 覆盖契约末尾一句"构建系统（Makefile/bsd.own.mk）非语义，WONTFIX；`live-update` 见 A-11/01"，不建篇 | 归 00 声明 |
| GAP-4 | RS 测试基建（`testutil.rs` + in-module `#[cfg(test)]`，无独立 tests/）在文档中散落各篇 §5，无统一说明"本 stage 测试形态" | `ls os/servers/rs/tests` 空、`testutil.rs` 存在 | 在 `00` 新增"测试形态"小节：本 stage 全部为 crate 内单测 + `testutil` 夹具，集成走链归 E9 联调；不散建篇 | 归 00 声明 |

**新增知识点**（追加入池，来源=新增，带证据锚点）：
- **K-018（新增）**：update-in-progress 全局标志（`RS_*` update 状态位）语义与门控规则。锚点：`const.h`（RUPDATE 宏）、`manager.c:81`（`check_call_permission` 内 `RUPDATE_IS_UPDATING` 判定）、`request.c:943`（`do_period` 的 `update_period` 臂）。归属 **02**。

### 3.3 重复主题表

| 主题 | 现有重复位置 | 新目录主讲述点 | 其余处理 |
|------|-------------|---------------|---------|
| boot 主线图 + 两状态机（K-003） | 00 §1.3/§2 **且** 01 §1.2 双画 | **00**（唯一导航枢纽） | 01 删图，只留"boot 执行机制"正文；01 的"导航骨架/前向引用豁免"角色取消 |
| `rprocupd`/`rupdate` 更新描述符（K-016） | 02 §2.7 **且** 17 §2.2 | **17**（LU 数据之家） | 02 只保留字段名 + 指向 17；rprocupd 完整形状从 02 移出 |
| ready 协议（K-062 / catch_boot_init_ready K-008） | 01 §2.5 **且** 12 §2.5 | **12** | 01 只标 boot Step2/3 调用点，机制移交 12 |
| `update_sig_mgrs`（K-025） | 03 §2.8 **且** 18 §2.6 | **03**（定义） / 18（RS 特例引用） | 18 只讲"RS 自身备份信号管理器"的差异，不重述通用逻辑 |

### 3.4 越界主题表

| 主题 | 越界文档 | 正确归属 | 依据 |
|------|---------|---------|------|
| RS 自升级 boot 分支（USE_LIVEUPDATE clone_slot+srv_fork，K-104） | 01 §2.3.7 展开 | **18**（RS 自身生命周期） | 01 是通用 boot 骨架；RS 换自己属自升级特例，与 18 §2.1 同源，01 只标一步 |
| 外部 syscall 实现细节（sys_*/vm_*/ds_* 线格式） | 各机制文档零散引用 | **19**（引用 `lib/libsys` 动词签名的唯一权威位）。**登记例外**：20 篇按 GAP-2 改判讲述 SEF 框架本体，因此引用 `lib/libsys/sef*.c`——只限框架机制叙述，动词签名仍归 19 | `plan §3.3` 既有约定，正文有越界需回收到 19（SEF 框架本体回收至 20） |

### 3.5 非 C 主题逐项回答（固定清单）

| 主题 | 在哪讲 / 为何不在本 stage |
|------|--------------------------|
| 链接与加载 | **不在本 stage**：RS 是普通 ELF 用户进程，链接脚本/装载归 `01-stage-kernel` bootstrap + `14-stage-runtime`；RS 侧只有 `sys_getimage` 读表（01/19） |
| 镜像与内存布局 | 01 §2.7 boot 三表 + 02 表布局；x86-64 vs i386 结构体布局差异归 17（A-14） |
| 汇编入口与陷阱进入 | **不在本 stage**：`boot-shim`/trap 归 `01-stage-kernel`；RS 无自有汇编入口 |
| 启动装配 | 01（S-01~S-17）+ 12（Step2/3 ready） |
| 构建与工具链 | Makefile WONTFIX（GAP-3，归 00 一句声明）；`live-update` feature 归 01/A-11 |
| 跨模块接口与线格式 | 19（消息槽 A-2）+ 99（常量权威位置） |
| 错误路径 | 99 §2.5（error.c）+ A-12（各篇 §3）；`panic()` fail-closed 归各篇 |
| 关闭与退出 | 15（terminate/cleanup/detach）+ 13（do_shutdown） |
| 并发与同步 | RS 是单线程事件循环（用户态服务器），无跨线程；SMP 约束在 kernel/VM stage，本 stage 不涉及 |
| 测试基建 | crate 内单测 + `testutil.rs`（GAP-4，归 00 声明），集成走链归 E9 |

---

## 4. 新目录

### 4.1 核心裁决：同编号语义重建，不做整体重编号

断链成本实测（§8）：编号文档之间文件名引用 **375 处** + 章节简写引用 **44 处** + 代码注释引用 **134 处** = 约 **553 处内部引用**，另有外部 stage（07-ds、14-runtime、01-kernel、edge_*）引用。整体重编号（如合并/插入导致后续全部顺移）将触发 500+ 处机械改写 + 代码注释同步，**收益 < 成本**，命中提示词 §二"看不清成本的重建不做"与历史教训 I-14。

同时：现有 00→19→99 的**线性执行/生命周期顺序本身是对的**（步骤 1 真序与文档顺序一致），四条硬标准里唯一系统性失守的是"无前向引用"，而前向引用簇的根因是 Live Update（16）机制晚于其状态门控（04/07）——这是**语义归位问题（GAP-1）不是编号问题**。

**因此新目录保持编号 00–19、99 不变**（唯一例外：**尾部追加 `20-rs-sef-framework.md`**，不顺移任何现有编号、不改写 553 处存量引用，故与本裁决不冲突；缘由见 §3.2 GAP-2 与 §7 前言例外），"重建"落在四件事：
1. **重排职责**：00 收拢全部导航（唯一主线图）；01 去导航骨架化、收缩 boot 自升级到调用点；02 移 rprocupd→17；
2. **补齐前向引用根**：02 首次讲完整 update-in-progress 状态标志语义（K-018）；
3. **除编辑缺陷 + 去开发文档味**：17 重复 H2、18 错位 H2、全库 patch-note 式小节标题（Fix #NN / R6 / D-16 / 接线落地 / S18）改写为叙述或下沉 `todo.md`。
4. **SEF 框架本体归位**（GAP-2 改判）：新增尾篇 20 承载库侧半与跨服务视角（K-125）；01 §1.3/§2.2 与 12/18/19 收缩为简述 + 外链；跨 stage 的 `01-*-init-main.md` 简述化列为 GAP-5 后续批次。

### 4.2 新篇章总表（编号不变，标注变更类型）

| 编号 | 标题（保持） | 变更 | 一句话定位 |
|------|-------------|------|-----------|
| 00 | rs-overview | 强化（收拢导航 + 测试形态 + 构建声明） | RS 全局心智模型与唯一导航枢纽 |
| 01 | rs-boot-init | **收缩**（去主线图/去导航豁免/自升级→18/SEF 本体→20） | RS 从 main() 到进主循环的 boot 执行机制 |
| 02 | rs-process-table | **调整**（移 rprocupd→17，补 update 状态位语义） | RS 数据底座：登记表/标志/槽位原语 |
| 03 | rs-privilege | 保持（清 patch-note） | priv 建模与 privctl 操作面 |
| 04 | rs-access-control | 保持 | 谁有资格命令 RS |
| 05 | rs-ipc-sendmask | 保持 | 服务能向谁发消息 |
| 06 | rs-main-loop | 保持 | 主循环与消息分类 |
| 07 | rs-period-heartbeat | 保持（update 门控改回指 02） | 周期检查与心跳 |
| 08 | rs-slot-config | 保持 | 声明到槽位的配置落地 |
| 09 | rs-exec | 保持 | 服务二进制加载与共享 |
| 10 | rs-service-create | 保持 | create_service 11 步编排 |
| 11 | rs-publish | 保持 | 发布四目录与撤销 |
| 12 | rs-init-run | 保持（做 ready 唯一主讲述点） | RS_INIT ready 握手 |
| 13 | rs-control-requests | 保持（清 patch-note 小节） | 控制面分派层 |
| 14 | rs-query-requests | 保持 | 只读观测面 |
| 15 | rs-terminate-restart | 保持 | 终止与恢复状态机 |
| 16 | rs-live-update | 保持（清 patch-note 小节） | Live Update 全状态机 |
| 17 | rs-state-data | **接收 rprocupd + 修重复 H2** | LU 数据形状与状态迁移 |
| 18 | rs-self-lifecycle | **接收 boot 自升级 + 修 H2 层级** | RS 自身更新自己 |
| 19 | rs-external-interfaces | 保持 | RS 对外依赖面契约 |
| **20** | **rs-sef-framework** | **新增**（GAP-2 改判，K-125 主讲述点） | SEF 框架本体：启动期收谁的消息、收信门口拦哪四类、回调全集与出生回报三形态 |
| 99 | rs-global-concepts | 保持 | 常量词典 |

### 4.3 阅读路径

- **主线（必读，按编号）**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 →（生命周期）08 → 09 → 10 → 11 → 12 → 13 → 14 → 15
- **次线（Live Update 专题）**：15 之后 16 → 17 → 18（三者内部线性；04/07 的 update 门控统一回指 02 的 K-018，不再前指 16）
- **参考线（可跳读）**：19（外部依赖面）、99（常量词典），任意时刻回查；**20（SEF 框架本体）既是参考线也是前置篇**——读者进入任何服务 stage 的 01 篇前先读它（各消费 stage 的外链见 §8.2 后续批次）。
- **并行体**：控制 handler（13）与查询 handler（14）是"分派表成员"型并行，用统一框架篇（06 分类 + 04 授权）汇聚，不强行排序内部。

---

## 5. 每篇契约（仅对**语义发生变化**的篇章给完整契约；保持篇给"清理契约"）

> 未列出完整契约的篇（03/04/05/06/08/09/10/11/13/14/15/16/19/99）语义与顺序均保持不变，仅需执行 §6"清理契约"（去 patch-note / 修 stale 引用），B 相不重写正文，只做归档前的一致性清洗。以下给发生变化的 00/01/02/12/17/18 完整契约，加上新增篇 **20**（GAP-2 改判的落点）的完整契约。

### 00-rs-overview（强化：唯一导航枢纽）

- **一句话定位**：让读者一图看懂 RS 是谁、boot 排第几、一辈子做什么，并据一张图定位任意机制归属。
- **讲什么**：K-001 RS 身份 / K-002 两层 boot 顺序 / K-003 boot+主循环两状态机（**唯一主线图**）/ 服务生命周期次主线图 / 文档导航表 / 覆盖契约 / 阅读路线；新增 GAP-3 构建声明、GAP-4 测试形态声明。
- **不讲什么**：任何机制细节（一律标归属编号）。SEF 框架本体 → **20**（GAP-2 本轮改判：原判 `14-stage-runtime`，该 stage 实测未讲）；boot 链前置 → `01-stage-kernel/09`。
- **前置**：无（入口）。
- **后置**：全部。
- **事实底线**：`com.h:RS_PROC_NR/:77`、`kernel/table.c:44-64`、`main.c:38,158`。
- **知识点清单**：K-001、K-002、K-003（主讲述点）。
- **验收标准**：§2 主线图与 §1 真序表逐条可核对（S-01~S-17/L-01~L-07 全覆盖）；文档导航表每步的 NN 归属与本蓝图新目录一致；不得再出现"机制展开"段落。

### 01-rs-boot-init（收缩：去导航骨架，boot 执行机制本体）

- **一句话定位**：RS 进程从 `main()` 到进入主循环之前，把自己启动好的完整机制。
- **讲什么**：K-004 main 三段式 / K-005 SEF 回调注册 / K-006 sef_cb_init_fresh 四步 / K-007 boot 三表与 `boot_image_info_lookup`。
- **不讲什么**：
  - boot 主线图与全局导航 → **00**（删 §1.2 重复图与 §2 的"映射 02~18 锚点"导航职能，**取消前向引用豁免**）；
  - Step1 权限构造细节 → **03/05**（只标调用点）；Step2/3 ready → **12**；update 状态 → **02**；
  - boot 自升级 clone_slot+srv_fork 细节 → **18**（01 只保留 S-17 一步占位）；
  - **SEF 框架本体**（`sef_startup` 状态机 / `sef_receive_status` 四类拦截 / 回调全集与 `SEF_CB_*` 预定义族 / 出生回报三形态）→ **20**。01 §1.3/§2.2 只答两件事："SEF 是什么"（**一句简述** + 绝对路径外链 20）与"RS 注册了哪 7 个回调、为什么 RS 要注册全量"，不再重述框架原理。
- **前置**：00。
- **后置**：02、03、12、18。
- **事实底线**：`main.c:38-131,136,158-494,709-779`、`table.c:15-50`。
- **知识点清单**：K-004、K-005、K-006、K-007（主讲述点）；K-008/K-104 降级为调用点标注。
- **验收标准**：正文不再出现"见 NN §x 展开"的导航段落堆（导航归 00）；删除 §1.2 主线图后正文仍自洽；行数从 1044 降至 ≤750；§3.7 等小节标题去 `D-16` 标记；**框架原理段全部外链 20**（本篇 `SEF` 命中只允许出现在"简述一句 + 注册表"两处）。

### 02-rs-process-table（调整：数据底座瘦身 + 补 update 状态语义）

- **一句话定位**：RS 用来"记住每个服务"的数据底座：登记表、标志位、槽位原语。
- **讲什么**：K-009 rproc/rprocpub 双表 / K-010 rproc_ptr 索引 / K-011 r_flags / K-012 sys_flags / K-013 槽位原语 / K-014 rs_isokendpt / K-015 get_service_instances / K-017 rinit / **K-018（新增）update-in-progress 标志语义**。
- **不讲什么**：`rprocupd`/`rupdate` 完整形状 → **17**（02 只列字段名并指向 17）；每个字段的机制读写时机 → 各机制篇。
- **前置**：01。
- **后置**：03、04、07、10、16、17。
- **事实底线**：`type.h:56-108`、`rs.h:rprocpub`、`const.h:28-43`、`manager.c:1935-2109`、`utility.c:352`。
- **知识点清单**：K-009~K-015、K-017、K-018（主讲述点）；K-016 移出。
- **验收标准**：新增 K-018 小节须让 04（`check_call_permission` 的 update 门控）与 07（`update_period` 例外）能**回指 02**而非前指 16；rprocupd 仅在 17 完整展开（全库 grep `rprocupd` 主定义唯一）；行数从 904 降至 ≤650。

### 12-rs-init-run（保持 + 升为 ready 唯一主讲述点）

- **一句话定位**：创建+发布后，服务"活着但没初始化"——RS 用 RS_INIT 消息握手收尾。
- **讲什么**：K-062 ready 握手（boot 与运行时两处，**唯一完整讲述**）/ K-063 do_init_ready/do_upd_ready / K-064 end_srv_init/run_service/start_service / K-065 RS 自模拟 ready / K-008 catch_boot_init_ready（自 01 收拢）。
- **不讲什么**：do_upd_ready 之后的 update 状态机 → **16**；库侧半（`sef_startup` 等 `RS_INIT`、`process_init` 分派、`sef_receive_status` 拦截、出生回报的 libsys 实现）→ **20**（12 只讲 **RS 侧半**：发出 RS_INIT、接住出生回报、ready 编排）。
- **前置**：10、11、06、02。
- **后置**：13、16、18。
- **事实底线**：`manager.c:923,950,328`、`utility.c:18-68`、`request.c:462-533,890-942`、`main.c:591-626,784-825`。
- **知识点清单**：K-008、K-062、K-063、K-064、K-065（主讲述点）。
- **验收标准**：01 的 ready 内容并入后仍单篇单语义；grep 确认 catch_boot_init_ready 不再在 01 展开。

### 17-rs-state-data（接收 rprocupd + 修重复 H2 缺陷）

- **一句话定位**：LU 把旧实例的"记忆"（状态数据 + 更新描述符形状）交给新版本。
- **讲什么**：K-100 init_state_data / K-101 三种状态载体 / K-102 IPC filter 解析 / K-103 state grants / **K-016 rprocupd/rupdate 数据形状（自 02 迁入）**。
- **不讲什么**：LU 状态机控制流 → **16**；cpf_* 外部契约 → **19**。
- **前置**：16、02。
- **后置**：18。
- **事实底线**：`manager.c:174-284`、`type.h:30-54`、`rs.h:88-100`、`ipc_filter.h`。
- **知识点清单**：K-016（迁入，主讲述点）、K-100~K-103。
- **验收标准**：**删除重复的 `## 3. Rust 设计决策`（现 127 与 138 两行）**，合并为一节；rprocupd 形状迁入后 §2.2 数据结构与正文一致；无 H2 重号。

### 18-rs-self-lifecycle（接收 boot 自升级 + 修 H2 层级）

- **一句话定位**：RS 是唯一的"能换掉自己"的服务——它的 restart/LU 初始化/回滚/备份信号管理器都异于常服务。
- **讲什么**：K-104 boot 自升级（自 01 迁入完整展开）/ K-105 sef_cb_init_restart/init_lu / K-106 RS rollback 特例 / K-107 备份信号管理器 / K-097 VM multi-component 的 RS 时序。
- **不讲什么**：通用 update 状态机 → **16**；update_sig_mgrs 通用逻辑 → **03**（18 只讲 RS 差异）；SEF 框架本体 → **20**（GAP-2 改判，原写 "14-runtime/19" 作废）；libsys 动词签名 → **19**。
- **前置**：01、16、12、02。
- **后置**：19。
- **事实底线**：`main.c:436-585`、`update.c:230-366`、`utility.c:387-412`、`manager.c:760-780`。
- **知识点清单**：K-104~K-107（主讲述点）。
- **验收标准**：`## 2.9b`（现 141 行）降为 `### 2.9`（并入 §2，编号连续）；`## 接线（S18）`（现 256 行）内容并入 §4 实现详解，删除该游离 H2；boot 自升级从 01 迁入后 01 不再展开。

### 20-rs-sef-framework（新增：SEF 框架本体——库侧半与跨服务视角）

> **新增来源**：GAP-2 改判（§3.2）。原判属 `14-stage-runtime`，取证不成立：`14-stage-runtime/plan.md:231` 将 `sef*.c` 列入排除表，同文件 `:277` 又判给不存在的 "minix-sef stage"，正式篇章 00–13/99 无一讲述框架本体。**为何归本 stage**（两条带锚点的理由）：①RS 是唯一注册全部 7 个回调的服务（`minix3/minix/servers/rs/main.c:136-152`）；②SEF 库内部专门为 RS 开了分支（`ROOT_SYS_PROC` → `do_sef_rs_init`，`minix3/minix/lib/libsys/sef.c:107-114`），且 init / LU / ping 三条腿的对端都是 RS（sef.c:127、sef_init.c:463 与 :477、sef.c:189、sef_liveupdate.c:559）。

- **一句话定位**：读者读懂**任何一个**用户态服务之前需要的第 0 页——SEF 这个链接进每个服务进程的库，启动期到底在收谁的消息、在收信门口拦下哪四类、又把什么交回服务自己处理。
- **讲什么**（K-125 主讲述点，八节）：
  1. 身份与名字：System Event Framework（权威展开：`minix3/minix/include/minix/sef.h:1`）；库本体 9 个 `.c` 共 **2210 行**分账（sef.c 400 / sef_init.c 483 / sef_liveupdate.c 566 / sef_llvm.c 241 / sef_signal.c 202 / sef_st.c 194 / sef_ping.c 63 / sef_fi.c 31 / sef_gcov.c 30，`wc -l` 实证）；它是**库不是服务**，无自有进程。
  2. `sef_startup()` 状态机：`sys_whoami` 取自身身份（sef.c:78-82）→ `ROOT_SYS_PROC` 特例走 `do_sef_rs_init`（sef.c:109-114）→ 其余服务 `ipc_receive(RS_PROC_NR)` 并**丢弃上一世残留杂消息**直到 init 请求抵达（sef.c:120-131；判据 `IS_SEF_INIT_REQUEST` 要求 `m_type==RS_INIT && m_source==RS_PROC_NR`，sef.h:31-34）→ `do_sef_init_request`（sef.c:134）。
  3. `sef_receive_status()` 四类拦截（sef.c:150-260）：**INIT** / **PING**（来源 `RS_PROC_NR` → sef.c:189-190，pong `ipc_notify(source)` 后吞掉：sef_ping.c:21-38,60-63 + sef.c:208-216）/ **LU** / **SIGNAL**（来源 `SYSTEM` → sef.c:186-187，`sys_getksig`/`sys_endksig`：sef_signal.c:36,60）。只有普通消息才回到调用者手里——这就是"服务门口看见的消息已被框筛过"的根源（与 06 主循环、跨 stage 的 `02-sched-message-surface.md §D10` 同一回事）。
  4. 回调注册全集：init 三型（`SEF_INIT_FRESH/LU/RESTART`，sef.h:93-95）+ init_response + lu_response/lu_state_save + ping_reply + signal_handler/signal_manager + state transfer；预定义族 `SEF_CB_*`（sef.h:80-91，含 `SEF_CB_INIT_RESTART_STATEFUL`、`SEF_CB_INIT_RESPONSE_DEFAULT`）。
  5. **出生回报三形态**：默认阻塞 `ipc_sendrec(RS_PROC_NR)`（sef_init.c:458-466）/ VM 的一次性异步 `asynsend3(RS_PROC_NR, m, AMF_NOREPLY)`（sef_init.c:471-483，避开 boot 期缺页互等）/ null；并对本项目已做过的相关缺陷记由（send 与 sendrec 之分导致野消息与 `receive` 回 EIO，见 `notes/rewrite/fork-syscall-rewrite/NK4C-WORKLOG.md` B9/B9b 节）。
  6. LU 与 State Transfer 家族**概览**：只讲库侧有几条腿、`SEF_LU_STATE_*` 从哪来（sef.h）；控制流一律回指 **16/17/18**，不重述状态机。
  7. **Rust 侧现状对账**：`os/libs/minix-sef/src/lib.rs`（438 行）只实现拦截核心（对位 sef.c:149-260）+ ping 应答；`sef_startup`/`process_init` 在各服务简化为显式握手（如 `os/servers/pm/src/init.rs` 与 VM 的 `rs_handshake`），回调集改为 trait 方法（A-7，K-136）；消费方已有 13 个 crate（`grep -rn minix-sef os/**/Cargo.toml` 实测）。
  8. 术语纪律：Rewrite（协议保持外部行为）与"框架消除"（Rust 侧不建同名库壳）分开标注，不混为 Refactor。
- **不讲什么**：ready 编排与 RS 侧半 → **12**；LU 全状态机 → **16**；LU 数据形状 → **17**；RS 自身 restart/LU 初始化 → **18**；`SEF_*` 常量的权威 Rust 位置 → **99 §2.3**（本篇只讲机制，不抢常量权威）；`sys_whoami`/`sys_getksig` 等 libsys 动词签名 → **19**；驱动侧 `chardriver_task`/`fsdriver_task`/`blockdriver_task` 的事件循环 → **16-stage-drivers / 15-stage-fs**。
- **前置**：00（心智模型）、**06**（主循环收信形状——不理解"收信门口"就读不懂"门口拦截"）、12（对端协议）。**本篇属 §4.3 参考线**（与 19/99 同类）：主线各篇（01/12/18）对它的引用是**横向外链**，不计入机制前指簇（见 §9 检查 1）。
- **后置**：无（本 stage 内为叶子篇）；**被外部消费 stage 反向引用**（GAP-5 批次）。
- **事实底线**：`sef.h:1,31-34,80-95`、`sef.c:68-145,150-260`、`sef_init.c:458-483`、`sef_ping.c:21-63`、`sef_signal.c:36-117`、`sef_liveupdate.c:559`、`servers/rs/main.c:136-152`、`include/minix/com.h:50,61`（`SYSTEM`=-2、`RS_PROC_NR`=2）、`include/minix/type.h:22`（`endpoint_t` 为 `int`，4 字节）。
- **知识点清单**：K-125（主讲述点）；跨引 K-005（01 注册表）、K-065（12 自模拟 ready）、K-105（18 restart/LU）、K-122（99 标志权威位）、K-136（A-7 SEF → minix-rs 抽象）。
- **验收标准**：① 01 与 20 一分为二不重叠——01 只答"RS 注册哪 7 个、为何全量"，20 只答"框架怎么调它们、对端是谁、拦了什么"；② 四类拦截的每一条判据均可 `grep` 到 C 锚点；③ 分账行数与 `wc -l minix3/minix/lib/libsys/sef*.c` 一致；④ 全篇不出现"本服务初始化做了什么"（那是各机制篇的活）；⑤ GAP-5 落地后，`grep -rn "SEF" notes/rewrite/fork-syscall-rewrite/*/01-*.md` 命中处均为简述 + 指向本篇的绝对路径链接；⑥ 本篇无对 `.design/` / `doc_rerank_*` 的引用（AGENTS.md 隐藏目录约定）。
- **已知错位与缓解**（不代用户决断，仅登记）：`minix-sef` 这个 crate 物理上住在 `os/libs/`（属 14-stage-runtime 的 crate 家族），而权威文档定在本 stage——形成"代码在 libs/minix-sef、主叙述在 03-stage-rs/20"的错配。缓解：本篇定位已写为"协议与框架本体的唯一权威讲述点（含 C 库侧 + Rust crate 现状对账）"；并在 GAP-5 批次里把 `14-stage-runtime/plan.md:277` 的 "minix-sef stage" 幻影归属改判为指向本篇（**本轮未动该文件**）。

---

## 6. 清理契约（对"保持"篇章强制执行，B 相不重写正文，只清洗）

1. **去开发文档味小节标题**（style-bible 违规）：以下小节标题中的 `Fix #NN`/`R6`/`D-16`/`S18`/`todo §NN`/`接线落地`/`已实现（R20b）` 一律改写为读者视角叙述，或将变更历史下沉 `todo.md`：
   - `01 §3.7`（"…，D-16"）、`08 §3.4`（"已实现（R20b）；…（DEFERRED→19）"）、`13 §2.1b/§2.1c/§2.1d/§2.1e`（四处，含 R4/R5/Fix #57/#52/#82/#83）、`16 §3.0b`（"接线落地（R6…Fix #84…）"）、`18 §2.9b`、`18 接线（S18）`。**证据见 §9 G9 抽查**。
   - 附带修正 `13` 小节乱序（现为 2.1d→2.1e→2.1c→2.1b，应按 2.1a..e 重排）。
2. **修 stale 代码引用**：`os/servers/rs/src/trap_api.rs:373` 引用不存在的 `10-rs-service-convert.md`，应改指 `10-rs-service-create.md`（B 相在代码注释迁移一并处理，见 §8 引用迁移表）。
3. **前向引用统一回指 02**：04、07 正文中"见 16 的 `RUPDATE_IS_UPDATING`"改述为"见 02 的 update-in-progress 状态位（K-018）"。

---

## 7. 缺漏新篇（GAP 落实）

> 本 stage 覆盖度高，无重大机制缺口；缺口均落实为"归位/声明"，**不新建编号篇章**（新建会触发 §8 断链且收益不足）。
>
> **例外裁决（本轮，用户确认）**：仅为本 stage **尾部追加一篇 `20-rs-sef-framework.md`**（GAP-2）。追加与"新建"的区别已量化：尾附编号**不顺移任何现有编号**，§8 实测的 375/44/134（≈553）处存量引用零改动，断链成本仅发生在"新增指向 20 的链接"这一侧；且本篇是本蓝图 §9 待裁决问题 2 已预留的 fallback 分支（"若该 stage 实际未讲 SEF 框架，则需在 03-stage-rs 或 14 补一篇"），14 侧已取证为未讲。

| GAP | 主题 | 处理 | 归属 | 验收 |
|-----|------|------|------|------|
| GAP-1 | update-in-progress 门控前置 | 新增 K-018，首次讲完整 | 02 | 04/07 回指 02，无前指 16 |
| GAP-2 | SEF 框架归属 | **改判**：不再推给 `14-stage-runtime`（实测未讲），本 stage **尾部新增 `20-rs-sef-framework.md`** 承载框架本体（八节完整契约见 §5） | 20（K-125 主讲述点）；01 §1.3/§2.2 只留"RS 注册了哪些回调 + 为什么全量" | 01 不再重述 SEF 生命周期；20 篇四类拦截逐锚点可 grep；`SEF_*` 常量权威位仍在 99（不抢定义） |
| GAP-3 | 构建/工具链 | 一句声明 WONTFIX | 00 | 00 覆盖契约末尾有构建声明 |
| GAP-4 | 测试形态 | 集中声明 crate 内单测 + testutil | 00 | 00 有"测试形态"小节 |
| GAP-5 | SEF 简述化（**跨 stage 后续批次**，20 篇落盘后才能执行） | 各消费 stage 的 `01-*-init-main.md`：框架原理段改为"一句简述 + 绝对路径外链 03-stage-rs/20"，只保留"本服务注册了哪几个回调"的差异表；同时把 `14-stage-runtime/plan.md:277` 的 "minix-sef stage" 幻影归属改判为指向 20 | 主讲述点在 20；各 stage 自身保留本地注册差异（命中量实测清单见 §8.2） | 20 存在且外链可达；复扫 `grep -c SEF` 后各 01 篇无独立框架原理节；引用不触及 `.design/` 与 `doc_rerank_*` |

---

## 8. 锚点迁移表与断链成本

### 8.1 锚点迁移表（仅发生内容/位置变化的条目）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 断链风险 |
|--------|--------|---------|---------|---------|
| 00 §2 主线图 / 01 §1.2 主线图 | 两张重复的 boot 主线图 | 00 §2（唯一）；01 §1.2 **删** | 合并 | 低（00 保留图，引用 00 的不动；引用 01 §1.2 的需改指 00） |
| 01 §2.5 catch_boot_init_ready | boot 同步捕获 | 12 §2.5 | 迁移 | 中（01 保留调用点标注；外部引用 01 §2.5 的改指 12） |
| 01 §2.3.7 boot 自升级（USE_LIVEUPDATE） | clone_slot+srv_fork 展开 | 18 §2.1 | 迁移 | 中（A-11 feature 标注留 01 §3.6 一句） |
| 02 §2.7 rprocupd/rupdate | 更新描述符完整形状 | 17 §2.2 | 迁移 | 高（02 §2.7 被多处引用，需保留字段名 + 改指 17） |
| 02（新增小节） | update-in-progress 标志语义 | 02（K-018） | 新增 | 无（新内容） |
| 03 §2.8 update_sig_mgrs | 通用逻辑 | 03（保持主讲述点） | 原样 | 无 |
| 18 §2.6 update_sig_mgrs | RS 差异 | 18 §2.6（收缩，去重述） | 改写 | 低 |
| 13 §2.1b~e 顺序 | 乱序补丁小节 | 13 §2.1a~e | 重排 | 低（编号不变，内部小节序） |
| 17 `## 3.`（127/138 重号） | 重复 H2 | 17 `## 3.`（合并） | 删除 | 无（编辑缺陷修复） |
| 18 `## 2.9b` / `## 接线（S18）` | 错位 H2 | 18 `### 2.9` / §4 | 改写 | 无（编辑缺陷修复） |
| （无旧位置，新建） | SEF 框架本体：`sef_startup` 状态机 / `sef_receive_status` 四类拦截 / 回调全集 / 出生回报三形态（K-125，现散在 01 §1.3+§2.2 / 12 / 18 / 19 + 各消费 stage 的 01 篇） | **20**（新增主讲述点） | 新增 + 各篇收缩为简述外链 | 中（本 stage 内仅新增指向 20 的链接；跨 stage 简述化归 GAP-5，见 §8.2） |

### 8.2 引用迁移表（分类批量，编号保持不变是最大降本）

| 引用类别 | 数量（实测） | 迁移处理 | 验证方式 |
|---------|-------------|---------|---------|
| 编号文档之间文件名引用（`NN-rs-*.md`） | 375 | 编号全部保留 → **绝大多数零改**；仅"迁出内容"相关引用改指新家（01→00 图、01→12 ready、01→18 自升级、02→17 rprocupd） | 迁移后 `grep -rn` 目标小节存在 |
| 章节简写引用（`NN §`） | 44 | 同上，仅迁移条目涉及的小节号变化 | 抽查迁移点 §号有效 |
| 代码注释引用（`os/servers/rs/src/*.rs`） | 134 | 保留；**修 1 处 stale**（`trap_api.rs:373` `10-rs-service-convert.md`→`10-rs-service-create.md`） | `grep -rE '10-rs-service-convert'` 归零 |
| 外部 stage 引用（07-ds/14-runtime/01-kernel/edge_*） | 多处 | 编号不变 → **零改**；若引用被迁小节，随锚点迁移表更新 | 跨目录 `grep -rl 03-stage-rs` 复核 |
| **后续批次（GAP-5）**：跨 stage 的 `01-*-init-main.md` SEF 简述化 | 实测 `SEF`/`sef_` 命中量（只计编号正式篇）：02-vm 161 / 04-pm 96 / 05-vfs 93 / 08-is 71 / 06-sched 38 / 11-devman 33 / 07-ds 30 / 13-ipc 22 / 12-input 14 / 10-mib 13 / 15-fs 12 / 16-drivers 11 / 17-net 9 / 09-init 3；**01-stage-kernel 正式篇 0 命中（无需处理）** | 框架原理段改为"一句简述 + 绝对路径外链 `notes/rewrite/fork-syscall-rewrite/03-stage-rs/20-rs-sef-framework.md`"，各篇保留本地注册差异表；`14-stage-runtime/plan.md:277` 的 "minix-sef stage" 幻影归属同批改判 | 20 落盘后逐 stage 复扫 `grep -c SEF` + 链接可达性检查；不得引用 `.design/` 或 `doc_rerank_*` |

### 8.3 断链成本摘要

- **若整体重编号**：约 553 处内部引用 + 代码注释需机械改写，且 `os/servers/rs/src/` 与多篇外部 stage 文档需同步——**成本极高、零读者收益**（现有编号顺序本身正确）。
- **本蓝图（同编号语义重建）**：实际改写集中在
  - 4 处内容迁移（00/01/02/18 交叉引用），预计触达约 **40~60 处**受影响引用（<11% of 553）；
  - 5 篇正文重排（00/01/02/17/18）；
  - 约 **11 个小节** patch-note 标题清洗 + 1 处 stale 代码引用修复 + 1 处 17 重复 H2 修复。
- **批量方式**：内容迁移用 §8.1 表逐条 `grep` 定位旧引用→按新家改指；标题清洗人工逐条（叙述改写不可机械替换）。

---

## 9. 验证与自检门

**四种机械检查**
1. 前向引用扫描：新目录仅保持编号 + 02 补 K-018 后，04/07 的 update 门控回指 02；01 去导航豁免 → 前向引用簇消除。**20 属 §4.3 参考线（与 19/99 同类），主线篇（01/12/18）对它的引用是横向外链，不计入机制前指簇**。✅（01 迁移后需复扫）
2. 依赖图无环：新契约前置全部指向更早编号（01→00、02→01、12→10/11/06/02、17→16/02、18→01/16/12/02、**20→00/06/12**），无环。✅
3. 覆盖率 100%：知识点池 K-001~K-143 每条有去向或标"边界外"（**GAP-2 SEF → 20**，原判 14-runtime 本轮已改判）；新增 K-018 带锚点。✅
4. 断链成本：已量化（375/44/134/外部），裁决"同编号"避开 553 处改写。✅

**自检门逐项**

| 门 | 结果 |
|----|------|
| G1 C 真序逐条可核对 | ✅ 抽 S-01/S-11/S-13、L-04/L-05 锚点均已 `Read`/`grep` 命中 main.c |
| G2 知识点池完整：每 C 文件/非 C 制品有归属或排除理由 | ✅ 8 .c 全归位；Makefile→GAP-3；**SEF→20（新增篇，主讲述点）**/stack_utils/exec_elf→19/09/14 |
| G3 前向引用为零 | ✅ 经 §5/§6 归位后成立（01 豁免取消、04/07 回指 02） |
| G4 依赖图无环 | ✅ §9 检查 2 |
| G5 覆盖率 100%，新增带锚点，删除单列 | ✅ rprocupd 迁移、**SEF 由"边界外"改判为 20 主讲述点**均记由 |
| G6 拆分/合并写清存量去向；新建写清新增来源 | ✅ rprocupd 02→17、boot 自升级 01→18、ready 01→12；K-018 带三锚点；**20 新建来源 = GAP-2 改判 + §9 待裁决 2 的 fallback 分支（八节锚点齐）** |
| G7 每篇契约七要素齐全 | ✅ §5 对变化篇给全要素；保持篇走 §6 清理契约 |
| G8 锚点迁移覆盖所有变化节；引用迁移覆盖文档+代码 | ✅ §8.1/§8.2 含代码注释 stale 修复 |
| G9 事实断言带锚点 | ✅ 缺陷均可核对：17 重复 H2（127/138）、18 错位 H2（141/256）、patch-note 标题（01/08/13/16/18 共 11 小节）、stale 引用（trap_api.rs:373） |

**结论**：本蓝图给出**可执行的重建规格**——保持 00–19/99 编号（尾部新增 20 承载 SEF 框架本体），对 00/01/02/12/17/18 六篇做语义重排与瘦身，对全部"保持篇"执行 §6 清理契约。核心读者价值：消除 00/01 导航重复、消除 01 前向引用豁免、斩断 Live-Update 前向引用簇（经 02 回指）、**给 SEF 框架一个唯一权威讲述点（治 K-125 四处分散）**、修复 3 处编辑缺陷与 1 处失效代码引用。**不建议整体重编号**（成本 553 处、收益为负）。

**待用户裁决的问题**
1. rprocupd 02→17 迁移：02 作为"数据底座"集中所有数据结构 vs 17 作为"LU 数据之家"就近——本蓝图取后者（兼减 02 体量）。若用户偏好"数据结构全部集中在 02"，则反向把 K-016 留 02、17 改回指，代价是 02 体量不减。
2. SEF 判定属 `14-stage-runtime`（GAP-2）：若该 stage 实际未讲 SEF 框架，则需在 03-stage-rs 或 14 补一篇；本蓝图基于"跨服务器复用、非 RS 专属"判为边界外，需确认 14 是否覆盖。→ **✅ 已裁决（本轮，用户选定方案 A）：fallback 分支触发，改判为在 03-stage-rs 尾部新增 `20-rs-sef-framework.md`**。取证：14-stage-runtime 未讲框架本体（`plan.md:231` 将 `sef*.c` 列入排除表、`:277` 判给不存在的 "minix-sef stage"、00–13/99 无一讲述）；本 stage 承接的两条理由与 C 锚点见 §3.2 GAP-2 与 §5 的 20 契约前言。**遗留的两件后续**：① GAP-5 跨 stage 简述化（需 20 落盘后才能建立外链）；② `14-stage-runtime/plan.md:277` 幻影归属的改判（本轮未动该文件）。原"非 RS 专属"的理由并未被推翻——SEF 确实是跨服务共享库（消费方 13 个 crate），只是**共享库需要一个权威讲述点**，而 RS 侧证据（注册全 7 回调 + 库内为 RS 开特例分支 + init/LU/ping 三腿对端都是 RS）使其适合作为该讲述点的宿主；发现性由 GAP-5 的外链网络保证。
3. `boot 自升级 01→18` 与 `ready 01→12` 会让 01 更薄：确认是否接受 01 降为"boot 执行机制"而非现"导航+boot"双重身份。

---

## 附录 A：C 函数清单（真序/覆盖对账用，逐文件，`grep` 定义提取）

> 用途：证明步骤 1 真序与 §2 知识点池对 C 源码函数面的覆盖。行号为定义所在。

- **main.c**（12）：`main`:38 / `sef_local_startup`:136 / `sef_cb_init_fresh`:158 / `sef_cb_init_restart`:499 / `sef_cb_init_lu`:549 / `sef_cb_init_response`:591 / `sef_cb_lu_response`:614 / `sef_cb_signal_handler`:631 / `sef_cb_signal_manager`:647 / `boot_image_info_lookup`:709 / `catch_boot_init_ready`:784 / `get_work`:826 → 归 01/06/12/18。
- **manager.c**（45）：`caller_is_root`:21 / `caller_can_control`:39 / `check_call_permission`:81 / `copy_rs_start`:135 / `copy_label`:151 / `build_cmd_dep`:289 / `end_srv_init`:328 / `kill_service_debug`:360 / `crash_service_debug`:380 / `cleanup_service_debug`:405 / `detach_service_debug`:497 / `create_service`:531 / `clone_service`:713 / `publish_service`:787 / `unpublish_service`:864 / `run_service`:923 / `start_service`:950 / `stop_service`:988 / `activate_service`:1013 / `reincarnate_service`:1033 / `terminate_service`:1055 / `run_script`:1185 / `restart_service`:1246 / `inherit_service_defaults`:1303 / `get_service_instances`:1334 / `share_exec`:1357 / `read_exec`:1372 / `free_exec`:1424 / `edit_slot`:1460 / `init_slot`:1708 / `clone_slot`:1800 / `swap_slot`:1870 / `lookup_slot_by_label`:1935 / `_by_pid`:1959 / `_by_dev_nr`:1985 / `_by_domain`:2013 / `_by_flags`:2041 / `alloc_slot`:2067 / `free_slot`:2088 / `get_next_name`:2115 / `add_forward_ipc`:2157 / `add_backward_ipc`:2230 / `init_privs`:2300（+ `init_state_data`、`swap_slot_pointer`、`rs_start`/`rproc` 表等工具生成段）→ 归 02/04/05/08/09/10/11/12/15/17。
- **request.c**（18）：`do_up`:15 / `do_down`:111 / `do_restart`:160 / `do_clone`:208 / `do_unclone`:253 / `do_edit`:298 / `do_refresh`:390 / `do_shutdown`:431 / `do_init_ready`:462 / `do_update`:534 / `do_upd_ready`:890 / `do_period`:943 / `do_sigchld`:1051 / `do_getsysinfo`:1095 / `do_lookup`:1144 / `do_sysctl`:1181 / `do_fi`:1229 / `check_request`:1265 → 归 07/12/13/14/16/08。
- **update.c**（24）：`rupdate_clear_upds`:7 / `rupdate_add_upd`:23 / `rupdate_set_new_upd_flags`:88 / `rupdate_upd_init`:121 / `rupdate_upd_clear`:135 / `rupdate_upd_move`:164 / `srv_update`:230 / `update_service`:262 / `rollback_service`:330 / `update_period`:371 / `start_update_prepare`:401 / `start_update_prepare_next`:467 / `start_update`:532 / `start_srv_update`:621 / `complete_srv_update`:657 / `abort_update_proc`:707 / `end_update_curr`:744 / `end_update_before_prepare`:763 / `end_update_prepare_done`:780 / `end_update_initializing`:795 / `end_update_rev_iter`:816 / `end_update_debug`:865 / `end_srv_update`:932 → 归 07/16/17/18。
- **utility.c**（17）：`init_service`:18 / `fi_service`:69 / `fill_send_mask`:82 / `fill_call_mask`:100 / `srv_to_string_gen`:142 / `srv_upd_to_string`:189 / `rs_asynsend`:223 / `reply`:309 / `late_reply`:332 / `rs_isokendpt`:352 / `sched_init_proc`:364 / `rs_is_idle`:424 / `rs_idle_period`:443 / `print_services_status`:485 / `print_update_status`:516（+ `update_sig_mgrs`:387 段）→ 归 03/06/07/12/14/15。
- **exec.c**：`srv_execve`:21 / `do_exec`:62 / `exec_restart`:121 / `read_seg`:143 → 归 09。
- **error.c**（3）：`rs_strerror`:33 / `init_strerror`:48 / `lu_strerror`:56 → 归 99。
- **table.c**：`boot_image_priv_table` / `boot_image_sys_table` / `boot_image_dev_table`（静态表）→ 归 01 §2.7/02。

全部定义函数在 §2 知识点池中均有归属；未见"代码已实现但文档未讲"的孤儿函数（`srv_to_string_gen`/`srv_upd_to_string` 归 14 §2 输出、A-12 static 池处理见 `plan §5.4`）。
