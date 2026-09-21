# 03-stage-rs 文档重建蓝图（HY4）

## 0. 元数据

### 0.1 执行头部

```text
your_name(AI agent name) = HY4
target_dir(关注的工作目录) = 03-stage-rs
repo_root(仓库根目录) = /home/xzhao/github/minix-rs

任务 = R 相·重建蓝图：输出 target_dir/doc_rerank_HY4.md，不改任何正文。
       只读 C 代码不够，还必须读：本目录全部文档的头部声明、os/ 目录对应入口、
       00-*-overview.md 的导航表。
约束 = 不得引用 .design/ 与 tmp_design_and_todo/；任何落盘产物必须带
       _<your_name> 后缀；不得读取或照抄其它 AI 的 doc_rerank_* 产物。
```

- **日期**：2026-09-19
- **当前提交**：`2d9d1f0aa`（`docs(edge3): S12 行随动——Open 字符设备分支与 v_sdev 归属修正，六十一条臂`）
- **阶段类型判定**：**服务事件循环型**（主），叠加**启动链型**（次）——判定理由见 §1.0。

### 0.2 审查范围

**算文档（21 篇，重建对象）**：`00-rs-overview.md` ~ `19-rs-external-interfaces.md` + `99-rs-global-concepts.md`，共 7608 行。

**算参考材料（不重建，可读取取料）**：`plan.md`（369 行）、`todo.md`（580 行）、`draft/README.md`、`00-master-plan/README.md`、`edge_todo.md`。

**范围外**：`doc_rerank_deepseek.md` / `doc_rerank_glm.md` / `doc_rerank_qwen.md`（他人中间产物，按提示词禁止读取）、`.design/`（项目规范禁止引用）。

### 0.3 读取清单

| 类别 | 实际读取 |
|------|---------|
| C 源码 | `minix3/minix/servers/rs/` 全 8 个 .c（6808 行）+ `const.h`/`type.h`/`glo.h`/`proto.h`/`inc.h`（全读）；`main.c` 全读（834 行）；`manager.c` 精读 328-530、531-730、1055-1303；`request.c` 精读 15-165、462-536、943-1090；`update.c` 精读 371-630 |
| C 头文件 | `include/minix/rs.h`（全读 210 行）、`include/minix/com.h` 的 RS 段、`include/minix/sef.h` 的 SEF_INIT_*/SEF_LU_STATE_* 段、`include/minix/ipc_filter.h` |
| 内核侧 | `kernel/table.c:46-64`（boot_image）、`kernel/main.c:197,203,257,264-265`（VM 优先加载 + RTS_VMINHIBIT） |
| 构建/非 C | `servers/rs/Makefile`、`share/mk/bsd.own.mk:1499`、`os/Cargo.toml:14`、`os/servers/rs/Cargo.toml` |
| Rust 实现 | `os/servers/rs/src/` 全部 28 个 .rs（22458 行）：`lib.rs` 头部与 `pub use` 面全读、`main.rs` 全读、`dispatch.rs` 头部全读；其余按模块名核对存在性 |
| 边界材料 | `00-master-plan/README.md`（阶段划分与启动因果链）、`03-stage-rs/plan.md` §5（覆盖契约与排除项）、`03-stage-rs/todo.md` 骨架、`02-stage-vm/00-vm-overview.md` 头部、`01-stage-kernel/09-vm-boot-protocol.md`（间接） |

### 0.4 关键命令与证据摘录

```bash
$ wc -l minix3/minix/servers/rs/*.c
    59 error.c   165 exec.c   834 main.c  2332 manager.c  1309 request.c
    50 table.c  1011 update.c   547 utility.c        # 8 个 .c，6808 行

$ grep -nE '^[A-Za-z_].*\(' manager.c      # 45 个函数，首行 16 → 末行 2300
16 run_script / 21 caller_is_root / 39 caller_can_control / 81 check_call_permission
135 copy_rs_start / 151 copy_label / 174 init_state_data / 289 build_cmd_dep
328 end_srv_init / 360 kill_service_debug / 380 crash_service_debug
405 cleanup_service_debug / 497 detach_service_debug / 531 create_service
713 clone_service / 787 publish_service / 864 unpublish_service / 923 run_service
950 start_service / 988 stop_service / 1013 activate_service / 1033 reincarnate_service
1055 terminate_service / 1246 restart_service / 1303 inherit_service_defaults
1334 get_service_instances / 1357 share_exec / 1372 read_exec / 1424 free_exec
1460 edit_slot / 1708 init_slot / 1800 clone_slot / 1856 swap_slot_pointer
1870 swap_slot / 1935..2041 lookup_slot_by_{label,pid,dev_nr,domain,flags}
2067 alloc_slot / 2088 free_slot / 2115 get_next_name / 2157 add_forward_ipc
2230 add_backward_ipc / 2300 init_privs

$ grep -n 'USE_LIVEUPDATE\|USE_PCI' minix3/share/mk/bsd.own.mk
1499:	USE_LIVEUPDATE USE_PCI USE_BITCODE USE_MAGIC USE_ASR

$ wc -l os/servers/rs/src/*.rs | tail -1
 22458 total        # 28 个模块，Rust 侧已落地的语义远多于文档叙述
```

**覆盖度事实（与 `plan.md §5.5` 的契约对照，本次独立复核）**：15 个 `RS_*` 消息类型（`com.h:463-492`）、16 个 `r_flags`（`const.h:28-43`）、13 个 `SF_*`（`rs.h:191-203`）、20 个 `RSS_*`（`rs.h:33-52`）、7 个 `SEF_INIT_*`（`sef.h:93-103`）、4 个 `SEF_LU_STATE_*`（`sef.h:213-216`）——契约数量成立。

---

## 1. C 真序

### 1.0 阶段类型判定

**服务事件循环型 + 启动链型**。理由：RS 的一生由两段拼成——

- 前段是**一次性启动链**：`main()` → `sef_local_startup()` → `sef_startup()` → `sef_cb_init_fresh()` 四步 boot（`main.c:38-494`），每步都有严格的先后因果（先建权限才能放行，先放行才能收 ready，收完 ready 才能问 PM 要 pid）。
- 后段是**永续事件循环**：`while(TRUE)` 收消息 → 分类 → 分派 → 回复（`main.c:57-131`）。

按提示词 §九，"服务事件循环型"应按"服务为什么存在 → 诞生与初始化 → 消息接口 → 核心数据结构 → 按场景分组的请求处理 → 查询与杂项 → 与邻接服务的协议"组织，且**一次请求的生命周期比源码调用顺序更适合做主线**。本蓝图因此把"服务生命周期"（`RS_UP` → 创建 → 发布 → ready → 监控 → 终止 → 复活）设为与"启动链"并列的第二条主线，并把请求面（控制/查询）放在生命周期机制之后，而不是放在主循环之后。

### 1.1 真序表（启动段）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| B1 | kernel boot 循环只加载 VM 的 ELF | `kernel/main.c:197,203,257` | `proc_nr == VM_PROC_NR` 才 `arch_boot_proc` |
| B2 | 其余进程挂 `RTS_VMINHIBIT` | `kernel/main.c:264-265` | 非 VM 且 `p_nr >= 0` 全部抑制，等 VM 建页表 |
| B3 | RS 作为 root sysproc 被放行 | `kernel/table.c:53`（登记序第 2）、`com.h:61,77` | `RS_PROC_NR=2`、`ROOT_SYS_PROC_NR=RS_PROC_NR` |
| B4 | `main()` 入口 | `servers/rs/main.c:38` | |
| B5 | `sef_local_startup()` 注册 7 个 SEF 回调 | `main.c:136-153` | init 3 + response 2 + signal 2 |
| B6 | `sef_startup()` 判定 init type（fresh/restart/lu） | `main.c:151` | 本次走 `sef_cb_init_fresh` |
| B7 | `sys_getmachine(&machine)` | `main.c:53` | 失败即 `panic` |
| B8 | `env_parse("rs_verbose")` | `main.c:179` | |
| B9 | `sys_getinfo(GET_HZ, &system_hz)` | `main.c:181` | 周期常量依赖它 |
| B10 | `cpf_grant_direct` 建 `rinit.rproctab_gid` | `main.c:185-189` | rprocpub 表对全系统可读的 grant |
| B11 | `RUPDATE_INIT()` + `shutting_down=FALSE` | `main.c:192-193` | |
| B12 | `sys_getimage(image)` 取 boot image 副本 | `main.c:196` | |
| B13 | 统计 image 服务数与 priv 表服务数，不等则 panic | `main.c:202-227` | 一致性硬校验 |
| B14 | 重置 `rproc[]`/`rprocpub[]` | `main.c:230-237` | `r_init_err=ERESTART`、`old/new_endpoint=NONE` |
| B15 | **Step 1** 逐服务落地属性（label / static priv id / flags / send mask / call mask / SET_SYS / GETPRIV / sys_flags / dev_nr / cmd / sched / endpoint / 默认值 / `RS_IN_USE|RS_ACTIVE` / `rproc_ptr[]`） | `main.c:244-346` | RS/VM 跳过 `SET_SYS`（`main.c:285-291`） |
| B16 | **Step 2** 允许运行：`sched_init_proc` + `SYS_PRIV_ALLOW` + `init_service(SEF_INIT_FRESH)`；`SF_SYNCH_BOOT` 者立即 `catch_boot_init_ready`，否则计入 `nr_uncaught_init_srvs` | `main.c:350-399` | RS/VM 已在跑，直接 `init_service`（`main.c:363-373`） |
| B17 | **Step 3** 收完剩余 init ready | `main.c:404-407` + `784-821` | `catch_boot_init_ready(ANY)`；VM 不 reply（`main.c:812-815`，防死锁） |
| B18 | **Step 4** 向 PM 取 pid（`getnpid`） | `main.c:413-430` | |
| B19 | `sys_setalarm(RS_DELTA_T, 0)` 起周期闹钟 | `main.c:433` | |
| B20 | `USE_LIVEUPDATE`：`clone_slot` + `srv_fork` + `update_service(RS_SWAP)` + `cpf_reload` + `cleanup_service` + `vm_memctl(PIN)`；父分支则 `SET_SYS` + `sched_init_proc` + `SYS_PRIV_YIELD` | `main.c:436-491` | **`USE_LIVEUPDATE` 默认 yes（`bsd.own.mk:1499`）；minix-rs 映射为 cargo feature `live-update`，默认关（`os/servers/rs/Cargo.toml`）** |

### 1.2 真序表（循环段）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| L1 | `rs_idle_period()` 空闲期处理 | `main.c:59` → `utility.c:443` | 清理 `RS_DEAD`、补 replica |
| L2 | `get_work()` = `sef_receive_status(ANY)` | `main.c:62` → `826-834` | 失败即 panic |
| L3 | `rs_isokendpt(who_e, &who_p)` 校验调用者 | `main.c:64` → `utility.c:352` | 非法源 panic |
| L4 | `is_ipc_notify` 且 `who_p == CLOCK` → `do_period()` 后 `continue` | `main.c:80-84` | 周期检查 |
| L5 | `is_ipc_notify` 且其它源 → 记 `r_alive_tm = m.m_notify.timestamp` | `main.c:85-91` | 心跳 |
| L6 | 普通请求：15 路 `switch(call_nr)` | `main.c:100-122` | `RS_UP/DOWN/REFRESH/RESTART/SHUTDOWN/UPDATE/CLONE/UNCLONE/EDIT/SYSCTL/FI/GETSYSINFO/LOOKUP/INIT/LU_PREPARE` |
| L7 | `result != EDONTREPLY` 才 `reply()` | `main.c:125-128` | 延迟回复由 `late_reply` 在别处补 |

### 1.3 真序表（一次 `RS_UP` 请求的生命周期）

| # | 动作 | C 锚点 |
|---|------|--------|
| U1 | `check_call_permission(RS_UP, NULL)` | `request.c:27` → `manager.c:81` |
| U2 | `alloc_slot(&rp)` | `request.c:31` → `manager.c:2067` |
| U3 | `copy_rs_start` + `check_request` | `request.c:39-46` → `manager.c:135`、`request.c:1265` |
| U4 | `init_slot(rp, &rs_start, src)` | `request.c:64` → `manager.c:1708` |
| U5 | 重复检查（label / dev_nr / domain） | `request.c:71-87` |
| U6 | `start_service(rp, init_flags)` | `request.c:90` → `manager.c:950` |
| U7 | └ `create_service`：`srv_fork` → `getprocnr` → `SET_SYS`+`GETPRIV` → `sched_init_proc` → `read_exec` → `srv_execve` → `setuid(0)` → RS/VM 实例特殊处理 → `vm_set_priv` | `manager.c:531-708` |
| U8 | └ `activate_service(rp, NULL)` | `manager.c:964` → `1013` |
| U9 | └ `publish_service`：DS label + mapdriver + PCI ACL + DEVMAN_BIND | `manager.c:967` → `787` |
| U10 | └ `run_service`：`SYS_PRIV_ALLOW` + `init_service(SEF_INIT_FRESH)` 发出 `RS_INIT` | `manager.c:973` → `923` → `utility.c:18` |
| U11 | `RSS_NOBLOCK` 则立即回 OK；否则置 `RS_LATEREPLY` + 记 `r_caller/r_caller_request`，返回 `EDONTREPLY` | `request.c:96-105` |
| U12 | 服务初始化完成 → 发 `RS_INIT` → `do_init_ready` → 清 `RS_INITIALIZING` + reply + `end_srv_init` | `request.c:462-528` → `manager.c:328` |
| U13 | 周期：CLOCK → `do_period` → `ipc_notify` 探测 → 服务回 notify → 更新 `r_alive_tm` | `request.c:1035-1038`、`main.c:85-91` |
| U14 | `RS_DOWN` → `stop_service(rp, RS_EXITING)`（SIGTERM + 记 `r_stop_tm`）→ `RS_LATEREPLY` | `request.c:147` → `manager.c:988` |
| U15 | 若 SIGTERM 后 `2*RS_DELTA_T` 未死 → `crash_service` | `request.c:985-989` |
| U16 | 内核发系统事件 → `sef_cb_signal_manager` → 终止信号 → `terminate_service` | `main.c:686-692` → `manager.c:1055` |
| U17 | `terminate_service` 三分支：`RS_EXITING`（全清理 + 可能 `reincarnate_service`）/ `RS_REFRESHING`（`restart_service`）/ 退避（`r_backoff` 置位，等 `do_period` 递减） | `manager.c:1119-1179` |
| U18 | 退避归零 → `do_period` 调 `restart_service` | `request.c:975-979` → `manager.c:1246` |
| U19 | `restart_service`：有脚本走 `run_script`；否则 `clone_service` + `update_service(RS_SWAP)` + `run_service(SEF_INIT_RESTART)` | `manager.c:1246-1298` |

### 1.4 关键调用边（用于 §4 依赖构图，全部经 grep 实证）

| 调用者 | 被调用 | 锚点 |
|--------|--------|------|
| `do_init_ready` | `crash_service` / `end_update` | `request.c:494` / `511` |
| `terminate_service` | `abort_update_proc` / `end_update` / `restart_service` / `reincarnate_service` / `unpublish_service` / `cleanup_service` | `manager.c:1101,1129,1157,1178,1152,1138,1143` |
| `restart_service` | `run_script` / `clone_service` / `update_service` / `run_service` / `kill_service` | `manager.c:1257,1267,1276,1283,1259` |
| `do_period` | `update_period` / `restart_service` / `crash_service` | `request.c:954,978,988,1025` |
| `do_shutdown` | `crash_service` | `request.c:494`（`do_shutdown` 区间 431-461 之外，实测为 `do_init_ready` 内；`do_shutdown` 走 `stop_service` 语义，见 §5 契约 18 的勘误要求） |
| `start_update` | `do_init_ready` / `init_service` / `run_service` | `update.c:597,674,692` |
| `start_srv_update` | `update_service` / `end_update` | `update.c:643,645` |
| `update_service` | `swap_slot` / `activate_service` | `update.c:297,318` |
| `end_srv_update` | `end_srv_init` / `late_reply` | `update.c:913,916` |
| `cleanup_service` | `run_script` / `detach_service` / `free_slot` | `manager.c:477,485,490` |
| `create_service` | `cleanup_service`（6 处错误路径） | `manager.c:603,611,627,639,666,683` |

> **勘误（供 B 相验证）**：`00-rs-overview.md §2.0` 与 `plan.md §5.3` 把 `request.c:494` 的 `crash_service` 归入 `do_shutdown`；实测该行位于 `do_init_ready`（`request.c:462-528`）区间内，属**初始化失败分支**。`do_shutdown`（`request.c:431-461`）本身不调 `crash_service`。B 相写作时必须以 C 行号为准，不得沿用旧文档的归属。

---

## 2. 知识点全集

> 编号规则：`K-{族}` + 两位序号。族：A 定位/执行模型/工程、B 外部依赖面、C 进程表与槽位、D 权限与隔离、E 启动链、F 主循环、G 生命周期与配置、H 加载/创建/发布/启停、I 终结原语、J ready 与收尾、K 请求面、L 状态数据、M Live Update、N 终止/恢复/监控、O 常量与词典。
> 「源」列：**存**＝现有文档已承载；**新**＝现有文档未承载，由 C 源码/非 C 制品/OS 理论补入。
> 「去向」列填新目录编号（§4）。

### A 族：定位、执行模型与工程（9 条）

| 编号 | 名称 | 类型 | 源 | 锚点 | 现有位置 | 去向 | 读者收益 |
|------|------|------|----|------|---------|------|---------|
| K-A01 | RS 的身份：root system process | 概念 | 存 | `com.h:61,77` | 00 §1.1 | 00 §2 | 知道 RS 编号与"服务之根"含义 |
| K-A02 | boot 两层顺序语义（登记序 vs 执行序） | 概念 | 存 | `kernel/table.c:46-64`、`kernel/main.c:197,203,257,264-265` | 00 §1.2 | 00 §2 | 区分"表内顺序"与"谁先跑" |
| K-A03 | RS 的两个状态机（启动 / 运行时） | 概念 | 存 | `main.c:38-131,158-494` | 00 §1.3 | 00 §3 | 一辈子在做什么 |
| K-A04 | RS 的可靠性模型（崩溃不拖垮系统） | 概念 | 存 | `manager.c:1246` | 00 §3.2 | 00 §3 | 理解 reincarnation 命名由来 |
| K-A05 | 单线程事件循环执行模型（`!Send/!Sync` 合理） | 约束 | 新 | `os/servers/rs/src/lib.rs:3-6` | 无（旧文档缺） | 00 §4 | 判断为何不用锁 |
| K-A06 | 构建与特性开关（Makefile → cargo feature） | 工具与工程 | 存 | `servers/rs/Makefile`、`bsd.own.mk:1499`、`os/servers/rs/Cargo.toml` | 01 §3.6、99 | 00 §5 | 知道 `live-update` 默认关 |
| K-A07 | ARCH 演进清单 A-1~A-14 与落点 | 架构演进 | 存 | `plan.md §4`、00 §5.4 | 00 §5.4 | 00 §6 | 三处一致标注的索引 |
| K-A08 | 测试基建现状（testutil 夹具 / 单测 / qemu 缺口） | 工具与工程 | 新 | `os/servers/rs/src/testutil.rs`（693 行） | 无 | 00 §7 + 各篇验收 | 知道验证手段与盲区 |
| K-A09 | RS 与邻接 stage 的边界 | 约束 | 存 | 00 §4.2、`00-master-plan/README.md` | 00 §4.2 | 00 §2 | 防止与其它 stage 重复展开 |

### B 族：外部依赖面（17 条）

| 编号 | 名称 | 类型 | 源 | 锚点 | 现有位置 | 去向 | 读者收益 |
|------|------|------|----|------|---------|------|---------|
| K-B01 | `sys_privctl` 操作面 | 接口 | 存 | `main.c:287,379,478,485`；`manager.c:441,442,525,600` | 03 §2 | 01 §2 | 7 个操作各自改什么 |
| K-B02 | `sys_getpriv` 同步语义 | 接口 | 存 | `main.c:294`、`manager.c:601` | 03 §2 | 01 §2 | 为何 SET 后必须 GET |
| K-B03 | `sys_getimage` 与 `boot_image` 结构 | 接口 | 存 | `main.c:196` | 01 §2 | 01 §2 | boot 输入的来源 |
| K-B04 | `sys_getmachine` / `sys_getinfo(GET_HZ)` | 接口 | 存 | `main.c:53,181` | 01 §2 | 01 §2 | 两个 boot 期只读查询 |
| K-B05 | `sys_setalarm` 周期闹钟 | 接口 | 存 | `main.c:433,1044` | 01 §2 | 01 §2 | 心跳节奏由谁驱动 |
| K-B06 | `sys_kill`/`sys_diagctl_stacktrace`/`sys_statectl`/`sys_whoami`/`sys_datacopy` | 接口 | 存 | `manager.c:399`；`main.c:682`；`update.c`；`manager.c:1460+` | 19 §2 | 01 §2 | 五个低频但关键的调用 |
| K-B07 | `cpf_grant_direct/cpf_revoke/cpf_reload` | 接口 | 存 | `main.c:185`、`main.c:464` | 17 §2、01 | 01 §3 | grant 生命周期 |
| K-B08 | libsys `srv_fork`/`srv_kill`/`srv_execve` | 接口 | 存 | `manager.c:576,634,470` | 19 §2 | 01 §4 | RS 创造/杀死进程的三个入口 |
| K-B09 | `getnpid`/`getnuid`/`getprocnr`/`waitpid` | 接口 | 存 | `main.c:426`；`manager.c:584,1222`；`request.c:1063` | 19 §2、04 | 01 §4 | pid↔endpoint 换算的三个方向 |
| K-B10 | VM 面：`vm_memctl(VM_RS_MEM_*)`/`vm_set_priv`/`vm_update`/`vm_prepare` | 接口 | 存 | `manager.c:663,680,693,698`；`update.c:505` | 19 §2、10/16 | 01 §5 | RS 与 VM 的全部四个动词 |
| K-B11 | DS 面：`ds_publish_label`/`ds_delete_label`/`ds_retrieve_label_endpt` | 接口 | 存 | `manager.c:787+`、`864+`、`init_state_data` | 11、19 | 01 §5 | 名字服务的读写删 |
| K-B12 | 调度面：`sched_start`/`sched_stop` | 接口 | 存 | `utility.c:364`（`sched_init_proc`）、`manager.c:461` | 03、19 | 01 §5 | 调度器侧的进出 |
| K-B13 | DEVMAN / PCI / `mapdriver` 面 | 接口 | 存 | `manager.c:787-860` | 11、19 | 01 §5 | 设备侧三个外部依赖 |
| K-B14 | `libexec_*` / `minix_stack_*`（ELF 装载与栈帧） | 接口 | 存 | `exec.c:21-60` | 09 §2 | 01 §6 | 装载库的两个族 |
| K-B15 | 消息槽线格式 `mess_rs_*` / `mess_lsys_*` | 接口与协议 | 存 | `include/minix/ipc.h` | 19 §3 | 01 §7 | 每个 RS 消息用哪个槽 |
| K-B16 | errno 约定与 Rust `Errno` newtype | 约束 | 存 | ARCH A-12、`os/servers/rs/src/error.rs` | 99 §2.5 | 01 §8 | 错误怎么跨语言对齐 |
| K-B17 | `panic()` / fail-closed 策略 | 约束 | 新 | `main.c:54,182,188,197,226,289,295,366,377,380,388,428,434` | 无（旧文档散在） | 01 §8 + 08 §Step 各步 | boot 期失败为何直接死 |

### C 族：进程表与槽位（15 条）

| 编号 | 名称 | 类型 | 源 | 锚点 | 现有位置 | 去向 | 读者收益 |
|------|------|------|----|------|---------|------|---------|
| K-C01 | `rproc` 结构全字段 | 数据结构 | 存 | `type.h:56-108` | 02 §2.1 | 02 §2 | 一行一字段的对照表 |
| K-C02 | `rprocpub` 结构全字段 | 数据结构 | 存 | `rs.h:165-183` | 02 §2.2 | 02 §2 | 公开部分为何外露 |
| K-C03 | 双数组绑定与 `r_pub` 指针 | 机制 | 存 | `main.c:230-237` | 02 §2.3 | 02 §2 | 私有/公开两份的同步点 |
| K-C04 | `rproc_ptr[NR_PROCS]` 快速索引 | 机制 | 存 | `glo.h:35`、`main.c:344` | 02 §2.3 | 02 §3 | endpoint→槽位的 O(1) |
| K-C05 | `r_flags` 16 位全表 | 数据结构 | 存 | `const.h:28-43` | 02 §2.4 | 02 §4 | 状态与策略位的全集 |
| K-C06 | `sys_flags`（`SF_*`）13 位全表 | 数据结构 | 存 | `rs.h:191-203` | 02 §2.5 | 02 §4 | 跨实例不可变/可变之分 |
| K-C07 | `IMM_SF` 不可变标志集 | 约束 | 存 | `rs.h:205-206` | 08 §2（`inherit`） | 02 §4 | 哪些位不能改 |
| K-C08 | `RS_SRV_IS_IDLE` 宏 / `rs_is_idle` | 约束 | 存 | `const.h:45`、`utility.c:424` | 02 §2.4、06 | 02 §5 | "RS 空闲"的判定式 |
| K-C09 | 槽位查找五式（label/pid/dev_nr/domain/flags） | 机制 | 存 | `manager.c:1935,1959,1985,2013,2041` | 02 §2.6 | 02 §3 | 五种外部寻址方式 |
| K-C10 | `alloc_slot` / `free_slot` | 机制 | 存 | `manager.c:2067,2088` | 02 §2.6 | 02 §3 | 槽位的生与死 |
| K-C11 | `rs_isokendpt` 调用者校验 | 机制 | 存 | `utility.c:352` | 02 §2.6 | 02 §3 | 主循环为何敢 panic |
| K-C12 | `rinit` 全局初始化描述符 | 数据结构 | 存 | `glo.h:40` | 02 §2.7 | 02 §2 | 跨实例初始化信息的载体 |
| K-C13 | `rupdate` / `rprocupd` 结构（只讲结构） | 数据结构 | 存 | `type.h:30-52`、`glo.h:45` | 02 §2.7 / 17 | 02 §2（结构）/21（语义） | 链的形状先于语义 |
| K-C14 | 四链 `r_old_rp/r_new_rp/r_prev_rp/r_next_rp` | 数据结构 | 存 | `type.h:58-61` | 02 §2.3 | 02 §2 | 版本链与副本链的区别 |
| K-C15 | ARCH A-3/A-4：裸指针链 → `Vec<ServiceSlot>` + `Option<SlotId>` | 架构演进 | 存 | `os/servers/rs/src/service_slot.rs` | 02 §3 | 02 §6 | Rust 侧为何不用指针 |

### D 族：权限与隔离（16 条）

| 编号 | 名称 | 类型 | 源 | 锚点 | 现有位置 | 去向 | 读者收益 |
|------|------|------|----|------|---------|------|---------|
| K-D01 | `priv` 结构建模（内核侧） | 数据结构 | 存 | `kernel/priv.h` | 03 §1.2 | 03 §2 | 权限到底存在哪 |
| K-D02 | `static_priv_id` 与固定 priv id | 机制 | 存 | `main.c:265-266` | 03 §2.1 | 03 §2 | boot 服务为何要固定 id |
| K-D03 | `s_flags/s_init_flags/s_trap_mask/s_sig_mgr/s_bak_sig_mgr` | 机制 | 存 | `main.c:269-275` | 03 §2.1 | 03 §2 | 五个字段各自的语义 |
| K-D04 | `fill_call_mask`（kernel 侧 + VM 侧两次） | 机制 | 存 | `utility.c:100`；`main.c:278-280,316-317` | 03 §2.2 | 03 §3 | 两张调用掩码都怎么填 |
| K-D05 | `fill_send_mask` | 机制 | 存 | `utility.c:82`；`main.c:273` | 03 §2.2（定义） | 03 §3（定义）/04 §2（引用） | 全置快捷路径 |
| K-D06 | privctl 每操作的效果 | 接口 | 存 | 见 K-B01 锚点 | 03 §2.3 | 03 §4 | SET_SYS/ALLOW/... 逐个对照 |
| K-D07 | RS/VM 跳过 `SET_SYS` 的例外 | 约束 | 存 | `main.c:285-291` | 03 §2.1 | 03 §4 | 两个"已在跑"的服务 |
| K-D08 | `sched_init_proc` 与调度参数落地 | 机制 | 存 | `utility.c:364`；`main.c:320-322` | 03 §2.4 | 03 §5 | RS 怎么给服务定优先级 |
| K-D09 | `update_sig_mgrs` | 机制 | 存 | `utility.c:387` | 03 §2.5 | 03 §5 | 信号管理器怎么切换 |
| K-D10 | `SRV_OR_USR` 与 `SRV_*`/`USR_*` 默认族 | 约束 | 存 | `const.h:71`；`main.c:270-322` | 03 §2.1 | 03 §3 | 系统服务/用户的两套默认值 |
| K-D11 | `get_next_name`（IPC 列表解析） | 机制 | 存 | `manager.c:2115` | 05 §2.1 | 04 §2 | 名字串怎么切成列表 |
| K-D12 | `add_forward_ipc` / `add_backward_ipc` / `init_privs` | 机制 | 存 | `manager.c:2157,2230,2300` | 05 §2.2-2.4 | 04 §3 | 正向/反向补位的差别 |
| K-D13 | `IPC_ALL` / `IPC_ALL_SYS` 快捷路径 | 约束 | 存 | `rs.h:29-30` | 05 §2.4 | 04 §3 | 两个通配名 |
| K-D14 | `caller_is_root` / `caller_can_control` / `check_call_permission` | 机制 | 存 | `manager.c:21,39,81` | 04 §2 | 05 §2 | 两级授权模型 |
| K-D15 | `r_control` 隔离策略列表 | 数据结构 | 存 | `type.h:106-107`、`rs.h:55` | 04 §2.2 | 05 §3 | 非 root 如何获得授权 |
| K-D16 | `RUPDATE_IS_UPDATING` → EBUSY | 约束 | 存 | `const.h:105`；`manager.c:81+` | 04 §2.3 | 05 §3 | update 期间为何拒绝控制请求 |

### E 族：启动链（20 条）

| 编号 | 名称 | 类型 | 源 | 锚点 | 现有位置 | 去向 | 读者收益 |
|------|------|------|----|------|---------|------|---------|
| K-E01 | boot_image 三张静态表（priv/sys/dev） | 数据结构 | 存 | `table.c:15-50` | 01 §2.1 | 06 §2 | boot 的静态输入 |
| K-E02 | `boot_image_priv/sys/dev` 三个结构 | 数据结构 | 存 | `type.h:7-26` | 02 §2.7 | 06 §2 | 三个表的行格式 |
| K-E03 | `NULL_BOOT_NR` / `DEFAULT_BOOT_NR` 哨兵与默认项 | 约束 | 存 | `const.h:61-62`；`table.c:29,41,49` | 01 §2.1 | 06 §2 | 缺省回落的机制 |
| K-E04 | `boot_image_info_lookup` 四表查找 | 机制 | 存 | `main.c:709-779` | 01 §2.2 | 06 §3 | 查不到就 panic / 用默认 |
| K-E05 | 表项数量一致性校验 | 约束 | 存 | `main.c:216-227` | 01 §2.1 | 06 §3 | 两表对不上即死 |
| K-E06 | 表内顺序 = NOTIFY 优先级 | 概念 | 存 | `kernel/table.c:39-41` | 00 §1.1 | 06 §2 | DS 第一、RS 第二的原因 |
| K-E07 | SEF 框架与七类回调 | 概念/接口 | 存 | `main.c:136-153`；`sef.h:56-90` | 01 §2.1 | 07 §2 | 回调模型 |
| K-E08 | `sef_startup()` 与 init type 判定 | 机制 | 存 | `main.c:151`；`sef.h:93-95` | 01 §2.1 | 07 §3 | fresh/restart/lu 三选一 |
| K-E09 | `env_parse("rs_verbose")` 与 `rs_verbose` | 工具 | 存 | `main.c:179`、`glo.h:48` | 01 §2.3 | 08 §2 | 调试开关 |
| K-E10 | Step 1 逐服务属性落地全序列 | 机制 | 存 | `main.c:244-346` | 01 §2.4 | 08 §3 | boot 里最长的一步 |
| K-E11 | Step 2 允许运行（sched + ALLOW + init） | 机制 | 存 | `main.c:350-399` | 01 §2.5 | 08 §4 | 放行的三条动作 |
| K-E12 | `SF_SYNCH_BOOT` 与 `nr_uncaught_init_srvs` | 机制 | 存 | `main.c:390-397,404-407` | 01 §2.5 | 08 §4 | 同步/异步 boot 的分界 |
| K-E13 | `catch_boot_init_ready` 与 VM 异步例外 | 机制 | 存 | `main.c:784-821`（`812-815` 不 reply VM） | 12 §2.4 | 08 §5 | 防死锁的那一行 |
| K-E14 | Step 4 `getnpid` 补 pid | 机制 | 存 | `main.c:413-430` | 01 §2.7 | 08 §6 | pid 是最后才拿到的 |
| K-E15 | `init_service` 与 `RS_INIT` 消息构造（发送侧） | 机制/协议 | 存 | `utility.c:18-68` | 12 §2.2 | 08 §5 | 一条消息里装了什么 |
| K-E16 | `ROOT_SYS_PROC` 免发 `RS_INIT` | 约束 | 存 | `utility.c:18-68` | 12 §2.2 | 08 §5 | RS 不给自己发消息 |
| K-E17 | `rproctab_gid` 的建立与随消息下发 | 机制 | 存 | `main.c:185`；`utility.c:18-68` | 01 §2.3（建）/12（用） | 08 §5（建）/17 §2（用） | 创建点与消费点分离 |
| K-E18 | `RUPDATE_INIT` / `shutting_down` 复位 | 约束 | 存 | `main.c:192-193` | 01 §2.3 | 08 §2 | 两个全局变量初值 |
| K-E19 | boot 期 panic 语义（fail-fast） | 约束 | 新 | 见 K-B17 锚点 | 无 | 08 §2 | 与运行时 fail-closed 的区别 |
| K-E20 | ARCH A-13：boot 三表 → 静态表 `table.rs` | 架构演进 | 存 | `os/servers/rs/src/table.rs` | 01 §3 | 06 §5 | Rust 侧不解析运行时表 |

### F 族：主循环（10 条）

| 编号 | 名称 | 类型 | 源 | 锚点 | 现有位置 | 去向 | 读者收益 |
|------|------|------|----|------|---------|------|---------|
| K-F01 | 主循环骨架与 `get_work` | 机制 | 存 | `main.c:57-131`、`826-834` | 06 §2.1 | 09 §2 | 循环的四个动作 |
| K-F02 | 四类消息分类 | 机制 | 存 | `main.c:70-129` | 06 §2.2 | 09 §3 | 分类树的形状 |
| K-F03 | `is_ipc_notify` 与 IPC 状态字 | 机制 | 存 | `com.h:92` | 06 §2.2 | 09 §3 | 状态字怎么读 |
| K-F04 | `reply` 原语与 RS 自身免回 | 机制 | 存 | `utility.c:309-331` | 06 §2.4 | 09 §4 | 回复的第一条规则 |
| K-F05 | `late_reply` 与 `RS_LATEREPLY`/`r_caller`/`r_caller_request` | 机制 | 存 | `utility.c:332-351` | 06 §2.5 | 09 §4 | 异步续接的上下文放在哪 |
| K-F06 | `EDONTREPLY` 协议 | 协议 | 存 | `main.c:125-128` | 06 §2.3 | 09 §4 | 唯一的"不回"信号 |
| K-F07 | `rs_asynsend`（AMF_NOREPLY） | 机制 | 存 | `utility.c:223-246` | 06 §2.6 | 09 §4 | 只发不等的发送 |
| K-F08 | `rs_idle_period`（RS_DEAD 清理 + replica 补齐） | 机制 | 存 | `utility.c:443-484` | 06 §2.7 | 09 §5 | 空闲期偷偷做的事 |
| K-F09 | `rs_is_idle` 判定 | 约束 | 存 | `utility.c:424-442` | 06 §2.7 | 09 §5 | 与 K-C08 同式两用 |
| K-F10 | 心跳接收（`r_alive_tm = timestamp`） | 机制 | 存 | `main.c:85-91` | 07 §2 | 09 §3 | 分类树里最短的一支 |

### G 族：生命周期与配置（13 条）

| 编号 | 名称 | 类型 | 源 | 锚点 | 现有位置 | 去向 | 读者收益 |
|------|------|------|----|------|---------|------|---------|
| K-G01 | 服务生命周期全景（十站） | 概念 | 存 | 00 §3.1 | 00 §3.1 | 10 §2 | 一次请求的完整旅程 |
| K-G02 | 状态位迁移全景图 | 概念 | 新 | `const.h:28-43` + 各状态机 | 无（旧文档无全景图） | 10 §3 | 16 个位如何推进 |
| K-G03 | `rs_start` 结构全字段 | 数据结构 | 存 | `rs.h:104-151` | 08 §2.1 | 11 §2 | 请求方的声明格式 |
| K-G04 | `RSS_*` 20 位标志全表 | 数据结构 | 存 | `rs.h:33-52` | 08 §2.2、99 | 11 §2 | 请求方能开哪些开关 |
| K-G05 | `check_request` 参数校验 | 机制 | 存 | `request.c:1265-1309` | 08 §2.3 | 11 §3 | 哪些参数会被拒 |
| K-G06 | `copy_rs_start` / `copy_label`（跨地址空间拷贝） | 机制 | 存 | `manager.c:135,151` | 08 §2.4 | 11 §3 | 用户指针怎么进来 |
| K-G07 | `init_slot` | 机制 | 存 | `manager.c:1708-1799` | 08 §2.5 | 11 §4 | 首次落槽 |
| K-G08 | `edit_slot`（含 sys_flags 更新段 + `init_privs` 重跑） | 机制 | 存 | `manager.c:1460-1707` | 08 §2.6 | 11 §5 | 改槽的代价 |
| K-G09 | `RSS_*` → `SF_*` 映射与 core EPERM 例外 | 约束 | 存 | `manager.c:1618-1705` | 08 §2.6 | 11 §5 | 请求标志如何固化为策略位 |
| K-G10 | `build_cmd_dep`（argv 解析） | 机制 | 存 | `manager.c:289-327` | 08 §2.7 | 11 §4 | 命令串 → argc/argv |
| K-G11 | `inherit_service_defaults`（`IMM_SF`/`IMM_F`） | 机制 | 存 | `manager.c:1303-1333` | 08 §2.8 | 11 §6 | 新实例继承什么 |
| K-G12 | `r_period`/`r_restarts`/`r_asr_count` 覆盖规则 | 约束 | 存 | `manager.c`（edit_slot 段） | 08 §2.6 | 11 §5 | 三个计数器的可写时机 |
| K-G13 | 预分配：`rss_heap/map_prealloc_bytes` | 机制 | 存 | `rs.h:120-121`、`type.h:96-97` | 08 §2.5、16 | 11 §4 | 给 VM 的预分配请求 |

### H 族：加载 / 创建 / 发布 / 启停（22 条）

| 编号 | 名称 | 类型 | 源 | 锚点 | 现有位置 | 去向 | 读者收益 |
|------|------|------|----|------|---------|------|---------|
| K-H01 | `read_exec` / `share_exec` / `free_exec` | 机制 | 存 | `manager.c:1372,1357,1424` | 09 §2.3 | 12 §3 | 内存副本的生命周期 |
| K-H02 | `srv_execve` / `do_exec` / `exec_restart` | 机制 | 存 | `exec.c:21,62,121` | 09 §2.1-2.2 | 12 §2 | 三步装载协议 |
| K-H03 | `read_seg` 与 `exec_info` | 机制 | 存 | `exec.c:143-165` | 09 §2.2 | 12 §2 | 段怎么被读进来 |
| K-H04 | 栈帧构建（`minix_stack_params/fill`） | 机制 | 存 | `exec.c:21-60` | 09 §2.1 | 12 §4 | 参数如何摆到新栈上 |
| K-H05 | `SF_USE_COPY`/`SF_NEED_COPY` 与 `RSS_COPY`/`RSS_REUSE` | 约束 | 存 | `rs.h:34,194,195` | 09 §2.4 | 12 §3 | 有副本/需要副本的区别 |
| K-H06 | ARCH A-5/A-8（`Arc<[u8]>` / `minix-elf`） | 架构演进 | 存 | `os/servers/rs/src/exec.rs` | 09 §3 | 12 §6 | Rust 侧的替换 |
| K-H07 | `create_service` 全序列 | 机制 | 存 | `manager.c:531-708` | 10 §2.1 | 13 §2 | 诞生一个服务的 9 步 |
| K-H08 | `srv_fork` 与 fork 后重新 pin RS 内存 | 机制 | 存 | `manager.c:570-581,604` | 10 §2.1 | 13 §2 | 为什么每次 fork 都要 pin |
| K-H09 | `setuid(0)` VFS 阻塞 hack | 约束 | 存 | `manager.c:646-656` | 10 §2.1 | 13 §3 | 一个必须保留的行为怪癖 |
| K-H10 | RS 实例 pin / VM 实例 `MAKE_VM` + 全实例 pin | 机制 | 存 | `manager.c:658-695` | 10 §2.1 | 13 §3 | 两个特殊进程的处理 |
| K-H11 | `vm_set_priv` 下发 VM 调用掩码 | 机制 | 存 | `manager.c:697-702` | 10 §2.1 | 13 §3 | VM 侧权限的第二张表 |
| K-H12 | `clone_service`（replica 链接 + RS 备份 sig mgr） | 机制 | 存 | `manager.c:713-786` | 10 §2.2 | 13 §4 | 副本怎么挂到链上 |
| K-H13 | `clone_slot` 浅/深拷贝语义 | 机制 | 存 | `manager.c:1800-1855` | 10 §2.4 | 13 §5 | 哪些字段复制、哪些重置 |
| K-H14 | `swap_slot` / `swap_slot_pointer` | 机制 | 存 | `manager.c:1856-1932` | 10 §2.5 | 13 §5 | 四链+update 链+全局索引交换 |
| K-H15 | `activate_service` | 机制 | 存 | `manager.c:1013-1032` | 10 §2.3 | 13 §5 | "激活"到底改了什么 |
| K-H16 | `update_service` 置换原语 | 机制 | 存 | `update.c:262-329` | 16 §2 | 13 §6 | old→new 的通用换法 |
| K-H17 | `publish_service`（DS + mapdriver + PCI + devman） | 机制 | 存 | `manager.c:787-863` | 11 §2.1 | 14 §2 | 四步对外宣告 |
| K-H18 | `unpublish_service` | 机制 | 存 | `manager.c:864-922` | 11 §2.2 | 14 §3 | 撤销的对称操作 |
| K-H19 | ARCH A-10（PCI → `PciAcl` 占位 fail-closed） | 架构演进 | 存 | `os/servers/rs/src/publish.rs` | 11 §3、99 | 14 §5 | 没有 PCI 时的默认 |
| K-H20 | `start_service` 编排（create→activate→publish→run） | 机制 | 存 | `manager.c:950-987` | 12 §2.1 | 15 §2 | 一个入口串起四步 |
| K-H21 | `run_service` 原语（ALLOW + `init_service`） | 机制 | 存 | `manager.c:923-949` | 12 §2.2 | 15 §3 | 放行与初始化的组合 |
| K-H22 | `stop_service` 停止原语（SIGTERM + `r_stop_tm`） | 机制 | 存 | `manager.c:988-1012` | 13 §2 | 15 §4 | 停止的第一步 |

### I 族：终结原语（8 条）

| 编号 | 名称 | 类型 | 源 | 锚点 | 现有位置 | 去向 | 读者收益 |
|------|------|------|----|------|---------|------|---------|
| K-I01 | `kill_service`（`RS_EXITING` + crash） | 机制 | 存 | `manager.c:360-375` | 15 §2.4 | 16 §2 | 致命与非致命的分界 |
| K-I02 | `crash_service`（`sys_kill(SIGKILL)`；RS 自身 `exit(1)`） | 机制 | 存 | `manager.c:380-400` | 15 §2.4 | 16 §2 | 模拟崩溃的两种落点 |
| K-I03 | `cleanup_service` 两段式 | 机制 | 存 | `manager.c:405-492` | 15 §2.5 | 16 §3 | 先标记后真清理 |
| K-I04 | `detach_service`（唯一 label 重发布 + 降权） | 机制 | 存 | `manager.c:497-526` | 15 §2.6 | 16 §4 | 保留尸体以便调试 |
| K-I05 | `run_script`（fork + `execle("sh")`；reason/incarnation） | 机制 | 存 | `manager.c:1185-1241` | 15 §2.3 | 16 §5 | 外部恢复脚本的协议 |
| K-I06 | `get_service_instances` | 机制 | 存 | `manager.c:1334-1356` | 15 §2.7 | 16 §6 | 遍历一条副本链 |
| K-I07 | `MAX_DET_RESTART` 上限 | 约束 | 存 | `const.h:25`；`manager.c:1108,1290` | 15 §2.6 | 16 §4 | detach 的次数闸门 |
| K-I08 | ARCH A-1（`run_script` 的 fork 语义缺口） | 架构演进 | 存 | `os/servers/rs/src/recovery.rs` | 15 §3 | 16 §7 | no_std 下 fork 怎么办 |

### J 族：ready 与初始化收尾（7 条）

| 编号 | 名称 | 类型 | 源 | 锚点 | 现有位置 | 去向 | 读者收益 |
|------|------|------|----|------|---------|------|---------|
| K-J01 | `do_init_ready` 非 update 分支 | 机制 | 存 | `request.c:462-528`（`514-526`） | 12 §2.3 | 17 §2 | 成功路径的三件事 |
| K-J02 | 失败分支的 `RS_REINCARNATE` / `r_init_err` 置位 | 机制 | 存 | `request.c:488-497` | 12 §2.3 | 17 §3 | 失败要不要复活 |
| K-J03 | `end_srv_init`（late_reply + prev replica 清理 + `r_restarts++`） | 机制 | 存 | `manager.c:328-355` | 12 §2.5 | 17 §4 | 初始化的收尾四件事 |
| K-J04 | `sef_cb_init_response` / `sef_cb_lu_response`（RS 自模拟 ready） | 机制 | 存 | `main.c:591-609,614-626` | 12 §2.6 | 17 §5 | RS 自己没有别人给它发 ready |
| K-J05 | `catch_boot_init_ready` 的运行时语义 | 机制 | 存 | `main.c:784-821` | 12 §2.4 | 08 §5（主）/17 §2（回指） | boot 专用接收器 |
| K-J06 | `init_strerror` / `lu_strerror` 错误表 | 数据结构 | 存 | `error.c:33,48,56` | 99 §2.5 | 17 §3 / 24 §6 | 初始化/LU 错误的翻译 |
| K-J07 | `SEF_INIT_*` 强制失败标志 | 数据结构 | 存 | `sef.h:98-103` | 08 §2.2、13 | 17 §3 | 调试用的五个开关 |

### K 族：请求面（15 条）

| 编号 | 名称 | 类型 | 源 | 锚点 | 现有位置 | 去向 | 读者收益 |
|------|------|------|----|------|---------|------|---------|
| K-K01 | `do_up` 全序列 | 机制 | 存 | `request.c:15-106` | 13 §2.1 | 18 §2 | 生命周期的入口 |
| K-K02 | `RSS_NOBLOCK` 立即回复 | 约束 | 存 | `request.c:49,96-98` | 13 §2.1 | 18 §2 | 不等初始化的选项 |
| K-K03 | 重复检查（label/dev_nr/domain） | 机制 | 存 | `request.c:71-87` | 13 §2.1 | 18 §3 | 三种唯一性 |
| K-K04 | `do_down` | 机制 | 存 | `request.c:111-155` | 13 §2.2 | 18 §4 | 含"脚本请求 down"的分支 |
| K-K05 | `do_restart`（仅脚本场景） | 机制 | 存 | `request.c:160-207` | 13 §2.3 | 18 §4 | 为什么只能脚本用 |
| K-K06 | `do_refresh` | 机制 | 存 | `request.c:390-430` | 13 §2.4 | 18 §4 | refresh 与 restart 之别 |
| K-K07 | `do_shutdown` 与 `shutting_down` 全局 | 机制 | 存 | `request.c:431-461` | 13 §2.5 | 18 §5 | 关机的传播 |
| K-K08 | `do_clone` / `do_unclone` | 机制 | 存 | `request.c:208-297` | 13 §2.6 | 18 §6 | 手工建立副本 |
| K-K09 | `do_edit`（UPDATE_SYS + vm_set_priv + sched 重初始化 + replica 重建） | 机制 | 存 | `request.c:298-389` | 13 §2.7 | 18 §7 | 热改一个服务的参数 |
| K-K10 | `do_lookup` | 机制 | 存 | `request.c:1144-1180` | 14 §2.1 | 19 §2 | 名字 → endpoint |
| K-K11 | `do_getsysinfo`（三张表拷贝） | 机制 | 存 | `request.c:1095-1143` | 14 §2.2 | 19 §3 | 观测面导出什么 |
| K-K12 | `do_sysctl`（`SRV_STATUS`；`UPD_*` 委派） | 机制 | 存 | `request.c:1181-1228` | 14 §2.3 | 19 §4 / 21 §7 | 状态查询与 LU 控制同槽 |
| K-K13 | `do_fi` / `fi_service`（故障注入） | 机制 | 存 | `request.c:1229-1264`；`utility.c:69-81` | 14 §2.4 | 19 §5 | 故意让服务崩 |
| K-K14 | `print_services_status` / `print_update_status` / `srv_upd_to_string` | 工具 | 存 | `utility.c:485,516,189` | 14 §2.5 | 19 §6 | 人可读输出 |
| K-K15 | `srv_to_string_gen` 与 Rust `Display` 替代 | 工具 | 存 | `utility.c:142-188` | 99、14 | 19 §6 | static 池 → Display |

### L 族：状态数据（7 条）

| 编号 | 名称 | 类型 | 源 | 锚点 | 现有位置 | 去向 | 读者收益 |
|------|------|------|----|------|---------|------|---------|
| K-L01 | `rs_state_data` / `rs_ipc_filter_el` 结构 | 数据结构 | 存 | `rs.h:88-101` | 17 §2.1 | 20 §2 | 迁移内容的格式 |
| K-L02 | `init_state_data` 解析流程 | 机制 | 存 | `manager.c:174-288` | 17 §2.2 | 20 §3 | 从请求到内部表示 |
| K-L03 | `ANY_USR/ANY_SYS/ANY_TSK` 通配 endpoint | 约束 | 存 | `ipc_filter.h:10-12` | 17 §2.2 | 20 §3 | 三类通配 |
| K-L04 | `IPCF_*` 标志与黑白名单 | 数据结构 | 存 | `ipc_filter.h:18-21` | 17 §2.3 | 20 §4 | 过滤条的语义 |
| K-L05 | grant 与状态数据缓冲区 | 机制 | 存 | `type.h:39` | 17 §2.2 | 20 §3 | 数据怎么跨地址空间 |
| K-L06 | VM 保底 RS 通信条目 | 约束 | 存 | `manager.c:174-288` | 17 §2.2 | 20 §5 | 不能把 RS 自己滤掉 |
| K-L07 | ARCH A-14（x86-64 布局 56 vs i386 28） | 架构演进 | 存 | `os/servers/rs/src/state_data.rs` | 17 §3 | 20 §6 | 线格式宽度差异 |

### M 族：Live Update（24 条）

| 编号 | 名称 | 类型 | 源 | 锚点 | 现有位置 | 去向 | 读者收益 |
|------|------|------|----|------|---------|------|---------|
| K-M01 | `do_update` 请求解析与标志面 | 机制 | 存 | `request.c:534-889` | 16 §2.1 | 21 §2 | 一次更新的参数全集 |
| K-M02 | `SEF_LU_*` 标志（SELF/ASR/MULTI/PREPARE_ONLY/NOMMAP/DETACHED） | 数据结构 | 存 | `sef.h` LU 段 | 16 §2.1、99 | 21 §2 | 六种更新形态 |
| K-M03 | 链构造：`rupdate_add_upd/upd_init/upd_clear/upd_move/clear_upds` | 机制 | 存 | `update.c:7,23,121,135,164` | 16 §2.3 | 21 §4 | 五个链操作 |
| K-M04 | `RUPDATE_*` 宏族 | 约束 | 存 | `const.h:105-120` | 16 §2.3、99 | 21 §3 | 九个判定宏 |
| K-M05 | `end_update(result, reply_flag)` 契约 | 协议 | 存 | `update.c:865-931` | 16 §2.6 | 21 §5（契约）/24 §2（实现） | 更新怎么结束与回复 |
| K-M06 | `abort_update_proc(reason)` 契约 | 协议 | 存 | `update.c:707-743` | 16 §2.7 | 21 §5 / 24 §5 | 放弃的接口形状 |
| K-M07 | VM 默认 mmap prealloc | 约束 | 存 | `const.h:83` | 16 §2.1 | 21 §3 | `8 MiB` 从哪来 |
| K-M08 | `request_prepare_update_service` | 机制 | 存 | `update.c:185-229` | 16 §2.4 | 22 §2 | 向服务发 prepare |
| K-M09 | `start_update_prepare` / `_next` | 机制 | 存 | `update.c:401-462,467-527` | 16 §2.4 | 22 §3 | 逐个通知 |
| K-M10 | `do_upd_ready` | 机制 | 存 | `request.c:890-942` | 12 §2.7 / 16 | 22 §4 | prepare 完成上报 |
| K-M11 | `RS_PREPARE_DONE` 状态位 | 约束 | 存 | `const.h:36` | 02 / 16 | 22 §4 | prepare 完成的标记 |
| K-M12 | prepare 超时（`update_period` → `end_update(EINTR, RS_CANCEL)`） | 机制 | 存 | `update.c:371-396` | 07 §2.4（散） | 22 §5 | 谁在周期里推进 prepare |
| K-M13 | `start_update` | 机制 | 存 | `update.c:532-616` | 16 §2.5 | 23 §2 | 切换阶段的入口 |
| K-M14 | `start_srv_update` / `complete_srv_update` | 机制 | 存 | `update.c:621-656,657-706` | 16 §2.5 | 23 §3 | 单个服务的两段 |
| K-M15 | `srv_update` 与 `vm_update` 分支 | 机制 | 存 | `update.c:230-261` | 16 §2.5 | 23 §3 | 什么时候真调 VM |
| K-M16 | VM 多组件时序（批量 `vm_prepare` + 等 VM 初始化） | 机制 | 存 | `update.c:483-508,585-613` | 16 §2.5 | 23 §4 | 最复杂的一段 |
| K-M17 | `rs_receive_ticks` 超时接收 | 机制 | 存 | `utility.c:247-308`；`update.c:590` | 16（A-9 提及） | 23 §4 | 唯一调用点 |
| K-M18 | `end_update` 五态辅助 | 机制 | 存 | `update.c:744-863` | 16 §2.6 | 24 §3 | 按阶段分派 |
| K-M19 | `end_srv_update`（surviving/exiting 选择） | 机制 | 存 | `update.c:932-1011` | 16 §2.6 | 24 §4 | 新旧二选一 |
| K-M20 | `rollback_service` 与 `SF_VM_ROLLBACK` | 机制 | 存 | `update.c:330-370` | 16 §2.7 | 24 §6 | 回滚怎么走 |
| K-M21 | `do_init_ready` 的 update 分支 | 机制 | 存 | `request.c:506-513` | 12 §2.3（标注） | 24 §5 | 与 K-J01 同源的另一分支 |
| K-M22 | `rupdate_upd_move` | 机制 | 存 | `update.c:164-184` | 16 §2.3 | 24 §4 | 描述符跟着实例搬家 |
| K-M23 | ARCH A-6/A-9（类型化 `UpdatePhase` / `receive_timeout`） | 架构演进 | 存 | `os/servers/rs/src/live_update.rs` | 16 §3 | 24 §7 | C 标志 → Rust 枚举 |
| K-M24 | `lu_strerror` 消费 | 工具 | 存 | `error.c:56`；`request.c:890+` | 99 | 24 §6 | LU 错误的人读形式 |

### N 族：终止 / 恢复 / 监控（18 条）

| 编号 | 名称 | 类型 | 源 | 锚点 | 现有位置 | 去向 | 读者收益 |
|------|------|------|----|------|---------|------|---------|
| K-N01 | `terminate_service` 三分支状态机 | 机制 | 存 | `manager.c:1055-1180` | 15 §2.1 | 25 §2 | 终局的决策树 |
| K-N02 | 初始化失败分支（rollback / refresh / exiting） | 机制 | 存 | `manager.c:1069-1093` | 15 §2.2 | 25 §3 | 初始化中途死的三种后果 |
| K-N03 | update 进行中 → `abort_update_proc` | 约束 | 存 | `manager.c:1099-1102` | 15 §2.2 | 25 §3 | 崩溃打断更新 |
| K-N04 | `SF_NORESTART` → `CLEANUP_DETACH`/`CLEANUP_SCRIPT` | 约束 | 存 | `manager.c:1105-1117` | 15 §2.3 | 25 §3 | 不复活时的收尾选项 |
| K-N05 | core 服务死亡 → `_exit(1)` | 约束 | 存 | `manager.c:1120-1124` | 15 §2.3 | 25 §4 | 系统级放弃 |
| K-N06 | `reincarnate_service` | 机制 | 存 | `manager.c:1033-1054` | 15 §2.8 | 25 §5 | 换 endpoint 重生 |
| K-N07 | `restart_service`（脚本 or clone+update+run） | 机制 | 存 | `manager.c:1246-1298` | 15 §2.3 | 25 §5 | 两种复活路径 |
| K-N08 | 二进制指数退避计算 | 机制 | 存 | `manager.c:1164-1175`；`const.h:50-51` | 07 §2.3、15 | 25 §6 | `1 << min(restarts, BITS-2)` |
| K-N09 | `MAX_DET_RESTART` 在 restart 路径 | 约束 | 存 | `manager.c:1290-1293` | 15 §2.3 | 25 §5 | 与 K-I07 同常量两用 |
| K-N10 | `do_period` 全序列 | 机制 | 存 | `request.c:943-1046` | 07 §2.1 | 26 §2 | 周期检查做了什么 |
| K-N11 | 心跳协议（`ipc_notify` + `r_alive_tm/r_check_tm`） | 机制 | 存 | `request.c:1035-1038` | 07 §2.2 | 26 §3 | 一问一答 |
| K-N12 | 超时 → `crash_service` 与 `RS_NOPINGREPLY` | 机制 | 存 | `request.c:1004-1029` | 07 §2.2 | 26 §3 | 判定式与后果 |
| K-N13 | free pass 逻辑 | 约束 | 存 | `request.c:1013-1023` | 07 §2.2 | 26 §3 | 有人初始化时豁免 |
| K-N14 | SIGTERM 超时 → `crash_service` | 机制 | 存 | `request.c:985-989` | 07 §2.3 | 26 §4 | 优雅退出的兜底 |
| K-N15 | `do_sigchld`（`waitpid` + `rupdate_clear_upds`） | 机制 | 存 | `request.c:1051-1090` | 07 §2.5 | 26 §5 | 别人管的进程死了 |
| K-N16 | `sef_cb_signal_manager`（转发/终止/VM 免转发/stacktrace） | 机制 | 存 | `main.c:647-704` | 06 §2.8 | 26 §6 | 系统信号的中转站 |
| K-N17 | `SIGS_SIGNAL_RECEIVED` 消息化 | 协议 | 存 | `main.c:698-702` | 06 §2.8 | 26 §6 | 信号 → 消息 |
| K-N18 | `RS_DELTA_T`/`RS_INIT_T`/`UPD_INIT_MAXTIME` | 数据结构 | 存 | `const.h:48-49,116` | 07 §2.1、99 | 26 §2 | 三个时间常量 |

### O 族：常量与词典（6 条）

| 编号 | 名称 | 类型 | 源 | 锚点 | 现有位置 | 去向 | 读者收益 |
|------|------|------|----|------|---------|------|---------|
| K-O01 | `RS_*` 15 个消息类型 | 数据结构 | 存 | `com.h:463-492` | 99 §2.1 | 99 §2（索引） | 全表指针 |
| K-O02 | `RS_SYSCTL_*` / `RS_FI_CRASH` 子功能 | 数据结构 | 存 | `com.h:485-492` | 99 §2.1 | 99 §2 | 子功能号 |
| K-O03 | endpoint 常量与进程编号区间 | 数据结构 | 存 | `com.h:61` | 99 §2.4 | 99 §2 | 0~11 的编号表 |
| K-O04 | `SYS_PRIV_*` / `SYS_STATE_*` 操作码 | 数据结构 | 存 | `com.h`、`include/minix/syslib.h` | 99 §2.4 | 99 §2 | 操作码索引 |
| K-O05 | 常量单一权威位置原则 | 约束 | 存 | 99 §1.1 | 99 §1 | 99 §1 | 防跨模块漂移 |
| K-O06 | `proto.h` / `inc.h` 依赖面 | 工具 | 存 | `servers/rs/proto.h`、`inc.h` | 00 §5.5 | 01 §1 | 函数签名来源 |

**统计摘要**：总 197 条（A 9 / B 17 / C 15 / D 16 / E 20 / F 10 / G 13 / H 22 / I 8 / J 7 / K 15 / L 7 / M 24 / N 18 / O 6）。存量 189 条、新增 8 条（K-A05、K-A08、K-B17、K-E19、K-G02 及 §3.2 追补项）。按现有文档分布：01 承载最多（20 条 E 族为主），16 次之（24 条 M 族）。

---

## 3. 覆盖审计

### 3.1 主题全集（四路来源）

1. **C 源码符号**：8 个 .c 的 121 个函数（main 12 / manager 45 / request 18 / update 24 / utility 17 / exec 4 / error 3 / table 0）+ 5 个 RS 自有头文件 + 6 个外部头文件。
2. **OS 通用概念**：进程状态机、权限与能力模型、IPC 可达性、心跳与故障检测、二进制指数退避、副本与故障切换、热更新（状态迁移 + 版本切换）、名字服务、故障注入。
3. **非 C 制品**：`servers/rs/Makefile`、`share/mk/bsd.own.mk:1499`、`os/Cargo.toml:14`、`os/servers/rs/Cargo.toml`（feature `live-update`）、`os/servers/rs/src/testutil.rs`、`lib/libsys/sef*.c`（9 个 SEF 库文件）。
4. **阶段边界契约**：`00-master-plan/README.md` 的 03 号条目、`plan.md §5.1-5.4`、`todo.md` 的 EDGE 清单。

### 3.2 覆盖缺口表

| # | 缺口 | 证据 | 建议 | 落实为 |
|---|------|------|------|--------|
| G-1 | RS 的执行模型声明（单线程事件循环，无锁） | `os/servers/rs/src/lib.rs:3-6` 明确写"single-threaded event loop…no cross-CPU sharing"，旧文档无一处声明 | 新建 | K-A05 → 00 §4 |
| G-2 | 测试基建现状（夹具/单测/qemu 端到端缺口） | `os/servers/rs/src/testutil.rs` 693 行；`os/qemu-tests/` 无 RS 相关脚本（grep `servers/rs` 在 `os/xtask/src` 零命中） | 新建（小，不立篇） | K-A08 → 00 §7 + 各篇验收标准 |
| G-3 | `panic()` / fail-closed 策略缺统一陈述 | `main.c` 内 13 处 panic 与运行时 fail-closed 是两种不同策略，旧文档只在 03/99 提过 errno | 并入 | K-B17 → 01 §8 |
| G-4 | 状态位迁移全景图（16 位如何推进） | 旧文档逐位列表（02 §2.4）但从未给出迁移图 | 新建 | K-G02 → 10 §3 |
| G-5 | `USE_LIVEUPDATE` / `USE_PCI` 与 cargo feature 的映射关系 | `bsd.own.mk:1499` vs `os/servers/rs/Cargo.toml` | 并入（旧文档分散在 01/99） | K-A06 → 00 §5 |
| G-6 | SEF 框架本身（回调模型、init type 判定、9 个 sef*.c 库文件） | `main.c:136-153`、`sef.h:56-95`；旧文档只在 01 §2.1 提注册 | 新建专篇 | K-E07/K-E08 → 07 |
| G-7 | boot_image 三张静态表独立成篇 | `table.c` 50 行孤立文件，旧文档夹在 01 §2.1 | 新建专篇 | K-E01~K-E06 → 06 |
| G-8 | `run_script` 归属错误（旧文档归 15，但它同时是 cleanup 路径的一部分） | `manager.c:477`（cleanup 调用）与 `1257`（restart 调用）双调用点 | 重排 | K-I05 → 16（终结原语） |
| G-9 | `do_init_ready` 的 update 分支无正式归属（旧文档只标注） | `request.c:506-513` | 归入 | K-M21 → 24 §5 |
| G-10 | `RS_SYSCTL_UPD_*` 四个子功能无正式归属 | `request.c:1181-1228`、`com.h:486-489` | 归入 | K-K12 → 21 §7 |
| G-11 | `error.c`（59 行）无正式归属 | `error.c:33,48,56` | 拆分归入 | K-J06 → 17 §3 / 24 §6 |
| G-12 | `proto.h`（152 行）无正式归属 | 全函数签名来源 | 归入 | K-O06 → 01 §1 |

### 3.3 重复主题表

| # | 重复主题 | 现有展开位置 | 主讲述点（新目录） | 其余改为 |
|---|---------|-------------|------------------|---------|
| R-1 | `fill_send_mask` 语义 | 03 §2.2、05 §2.1 | 03 §3（定义与两条路径） | 04 §2 只引用 |
| R-2 | `SF_*` 全表 | 02 §2.5、08 §2.6、99 §2.3 | 02 §4（全表与不可变性） | 11 §5 只讲 `RSS_*→SF_*` 映射；99 只给指针 |
| K-R3 | `r_flags` 全表 | 02 §2.4、15 §2、16 §2、07 §2 | 02 §4（全表） | 25/26/21 只讲各自用到的位 |
| R-4 | `RS_LATEREPLY` 与 `late_reply` | 06 §2.5、12 §2.5、13 §2、15 §2 | 09 §4（协议与字段） | 其余只写调用点 |
| R-5 | 四步 boot 时序图 | 00 §2.0、01 §2 | 08 §1（权威时序图） | 00 §3 只放缩略图并回指 |
| R-6 | `sys_privctl` 操作面 | 03 §2.3、19 §2 | 01 §2（签名与 7 个操作的效果） | 03 §4 只讲"RS 在哪些时机发哪个操作" |
| R-7 | SEF 标志面 | 01 §2、12 §2、16 §2、18 §2、99 §2 | 07 §4（`SEF_INIT_*`/`SEF_LU_*`/`SEF_CB_*` 一次讲全） | 其余只引用 |
| R-8 | 生命周期路径图 | 00 §3.0、13 §内部、15 §内部 | 10 §2（唯一权威全景图） | 00 §3 缩略回指；13/25 不再重画 |

### 3.4 越界主题表

| # | 越界内容 | 现在哪 | 应归 |
|---|---------|-------|------|
| O-1 | 01（1044 行）承载了 boot 表、SEF 框架、自升级流程三块非"启动骨架"内容 | 01 §2.1/§2.6 | 06 / 07 / 27 |
| O-2 | 12 承载 `start_service`/`run_service`（创建编排），与"初始化协议"不同语义 | 12 §2.1-2.2 | 15 |
| O-3 | 15 承载 `kill/crash/cleanup/detach`（公共原语，被 create/publish/run 复用），与"终止状态机"不同语义 | 15 §2.4-2.7 | 16 |
| O-4 | 02 承载 `rprocupd`/`rupdate` 的语义展开（属 LU） | 02 §2.7 | 21（语义）；02 只留结构 |
| O-5 | 99 承载常量定义本身（应为机制篇首次定义） | 99 §2 | 各机制篇；99 改为索引 |
| O-6 | 06 承载 `sef_cb_signal_handler/manager`（信号通道，非主循环） | 06 §2.8 | 26 |

### 3.5 非 C 主题逐项回答（固定清单）

| 主题 | 在本 stage 讲不讲 | 归属 | 依据 |
|------|-----------------|------|------|
| **链接与加载** | 讲（部分） | 12-rs-exec | RS 自身 ELF 由内核在 boot 期加载（`kernel/main.c:257`），属 01-stage-kernel；本 stage 只讲"RS 如何把**别人的** ELF 装进别人的地址空间"（`exec.c` 全文 + `manager.c:1357-1459`） |
| **镜像与内存布局** | 讲（部分） | 11 §4（预分配）、13 §3（pin/MAKE_VM）、21 §3（VM 默认 prealloc） | `rs.h:120-121`、`const.h:83`、`manager.c:658-695`；RS 自身镜像布局属 01-stage-kernel |
| **汇编入口与陷阱进入** | 不讲机制，只讲授权 | 03 §2（`s_trap_mask`） | 陷阱进入属 01-stage-kernel；`s_trap_mask = SRV_OR_USR(rp, SRV_T, USR_T)`（`main.c:271`）是 RS 侧的授权位 |
| **启动装配** | **讲（本 stage 核心）** | 06（静态表）、07（SEF）、08（四步 boot） | `table.c` + `main.c:158-494`；镜像装配（谁把 rs 放进 boot image）属 01-stage-kernel |
| **构建与工具链** | 讲（小） | 00 §5（构建与特性开关）、01 §1（`proto.h`/`inc.h`） | `servers/rs/Makefile`（59 行）+ `bsd.own.mk:1499` 两个开关；**不新建专篇**——理由：RS 的构建只有 1 个 Makefile、8 个 SRCS、2 个编译开关，无独立语义，立篇会变成清单而非教学 |
| **跨模块接口与线格式** | **讲（本 stage 核心）** | 01-rs-external-face 全篇 | `include/minix/ipc.h` 的 `mess_rs_*`、`rs.h:104-151` 的 `rs_start`、`rs.h:88-101` 的 `rs_state_data`、`com.h:724-744` 的 `VM_RS_*` |
| **错误路径** | 讲（分散） | 01 §8（errno + panic 策略）、17 §3（`init_strerror`）、24 §6（`lu_strerror`）、99 §2（错误表索引） | `error.c` 全文 59 行 |
| **关闭与退出** | 讲 | 18 §5（`do_shutdown`/`shutting_down`）、25 §4（core 死亡 `_exit(1)`）、26 §6（SIGTERM 通道） | `request.c:431-461`、`manager.c:1120-1124`、`main.c:638-640` |
| **并发与同步** | 讲（两条伪并发） | 09 §4（late reply 的异步续接）、26 §2/§6（周期驱动与信号驱动两个异步推进点）、00 §4（执行模型声明） | RS 单线程无锁；`RS_LATEREPLY + r_caller/r_caller_request` 是唯一的"挂起-续接"状态 |
| **测试基建** | 讲（小） | 00 §7 + 每篇契约的"验收标准" | `os/servers/rs/src/testutil.rs`（693 行，`#[cfg(test)]`）；`os/qemu-tests/` 无 RS 条目。**不新建专篇**，理由同构建：当前只有单元测试夹具，没有独立框架 |

---

## 4. 新目录

### 4.1 新篇章总表（29 篇）

| 编号 | 标题 | 一句话定位 | 分组 |
|------|------|-----------|------|
| 00 | `rs-overview` | RS 是谁、在 boot 链的位置、两条主线、新目录导航与阅读路径 | P0 定位 |
| 01 | `rs-external-face` | RS 能对外部做什么：内核调用面、libsys 面、邻接服务面、消息槽与 errno | P1 地基 |
| 02 | `rs-process-table` | RS 用哪张表记住每个服务：rproc/rprocpub/rproc_ptr、标志位、槽位查找与分配 | P1 地基 |
| 03 | `rs-privilege` | 服务的内核权限怎么建模与下发：priv 结构、两张调用掩码、privctl 操作面、调度初始化 | P1 地基 |
| 04 | `rs-ipc-sendmask` | 服务能向谁发消息怎么算出来：`r_ipc_list` 解析与 `s_ipc_to` 位图 | P1 地基 |
| 05 | `rs-access-control` | 谁有资格命令 RS：root 通道、隔离策略通道、目标槽规则 | P1 地基 |
| 06 | `rs-boot-tables` | boot 的输入从哪来：boot_image 与 RS 三张静态表、查找与一致性校验 | P2 启动 |
| 07 | `rs-sef-startup` | RS 怎么被框架拉起来：SEF 回调模型、七类回调注册、init type 判定 | P2 启动 |
| 08 | `rs-boot-init` | 四步 boot：建权限 → 允许运行 → 收 ready → 补全信息，以及 `init_service` 发送侧 | P2 启动 |
| 09 | `rs-main-loop` | RS 的运行时心脏：收包、四类分类、回复协议、空闲期处理 | P3 运行时 |
| 10 | `rs-lifecycle` | 一个服务的十站旅程与状态位迁移全景（生命周期组导航） | P4 生命周期·导航 |
| 11 | `rs-slot-config` | 请求参数怎么变成槽位状态：`rs_start` 校验、拷贝、`init_slot`/`edit_slot`、`RSS_*→SF_*` | P4 生命周期 |
| 12 | `rs-exec` | 二进制怎么进地址空间：`read_exec`/`share_exec`/`srv_execve`/`exec_restart`/栈帧 | P4 生命周期 |
| 13 | `rs-service-create` | 实例怎么诞生与互换：`create_service`/`clone_service`/`clone_slot`/`swap_slot`/`activate_service`/`update_service` | P4 生命周期 |
| 14 | `rs-publish` | 服务怎么被找到：DS label、mapdriver、PCI ACL、devman bind 与撤销 | P4 生命周期 |
| 15 | `rs-start-run` | 三个编排原语：`start_service`（编排）、`run_service`（放行+初始化）、`stop_service`（停止） | P4 生命周期 |
| 16 | `rs-teardown` | 放弃一个实例的公共原语：kill / crash / cleanup / detach / run_script / get_service_instances | P4 生命周期 |
| 17 | `rs-ready` | 初始化完成怎么上报：`do_init_ready` 非 update 分支、`end_srv_init`、SEF response 回调 | P4 生命周期 |
| 18 | `rs-control-requests` | 外部怎么操作生命周期：do_up/down/restart/refresh/shutdown/clone/unclone/edit | P5 请求面 |
| 19 | `rs-query-requests` | 外部怎么观测与注入：do_lookup/do_getsysinfo/do_sysctl(SRV_STATUS)/do_fi/print_* | P5 请求面 |
| 20 | `rs-state-data` | 旧实例把什么交给新实例：`rs_state_data`/IPC filter 解析/grant/`IPCF_*` | P6 Live Update |
| 21 | `rs-lu-request` | 一次更新怎么被发起：`do_update` 解析、标志面、新实例准备、rupdate 链构造、收尾契约 | P6 Live Update |
| 22 | `rs-lu-prepare` | 更新怎么被准备：`request_prepare`/`start_update_prepare(_next)`/`do_upd_ready`/超时 | P6 Live Update |
| 23 | `rs-lu-switch` | 更新怎么被切换：`start_update`/`start_srv_update`/`complete_srv_update`/`srv_update`/VM multi | P6 Live Update |
| 24 | `rs-lu-end` | 更新怎么被收尾或放弃：`end_update` 族/`end_srv_update`/`abort`/`rollback`/init ready 的 update 分支 | P6 Live Update |
| 25 | `rs-terminate-restart` | 服务出事的终局决策：`terminate_service` 分支 → 清理 / 复活 / 退避 | P7 终局 |
| 26 | `rs-monitor` | RS 怎么发现服务出事：周期检查、心跳、SIGCHLD、信号通道 | P7 终局 |
| 27 | `rs-self-lifecycle` | RS 自己怎么更新自己：restart/lu 回调、boot 期自升级、备份信号管理器、rollback 特例 | P8 自身 |
| 99 | `rs-constants-index` | 常量与接口索引（跨篇指针，不重复定义） | P9 索引 |

### 4.2 阅读路径

- **主线（必读，00 → 27 顺序）**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15 → 16 → 17 → 18 → 19 → 20 → 21 → 22 → 23 → 24 → 25 → 26 → 27
- **支线（可跳读）**：01（查阅型，首次通读后可当手册）、99（纯索引）、19（观测面，不影响理解主流程）、20（只关心热更新时才读）
- **按目的的入口**：
  - 想懂 RS 是什么 → 00
  - 想懂 boot 怎么完成 → 01 → 02 → 03 → 06 → 07 → 08
  - 想懂运行时怎么工作 → 09 → 18 → 19 → 26
  - 想懂一个服务怎么被拉起 → 10 → 11 → 12 → 13 → 14 → 15 → 17
  - 想懂崩溃恢复 → 16 → 25 → 26
  - 想懂 Live Update → 20 → 21 → 22 → 23 → 24 → 27
  - 想核对 Rust 依赖面 → 01 → 99

### 4.3 并行体的组织

本 stage 唯一的并行体是 **15 个 `RS_*` 请求类型**（`com.h:463-492`）。按"汇聚点 + 触发时机"组织：

- **统一框架篇**：09（分派骨架与回复协议）+ 10（生命周期全景）+ 05（所有请求入口的授权检查）。
- **按角色分组**：控制类（`RS_UP/DOWN/REFRESH/RESTART/SHUTDOWN/CLONE/UNCLONE/EDIT` → 18）、查询与注入类（`LOOKUP/GETSYSINFO/SYSCTL/FI` → 19）、更新类（`UPDATE` → 21，`SYSCTL_UPD_*` 同槽归入 21）、初始化就绪类（`INIT/LU_PREPARE` → 17 / 22 / 24）。
- **代表成员精讲 + 差异表**：18 精讲 `do_up`（最完整的一条：授权 → 分配 → 校验 → 落槽 → 去重 → 启动 → 延迟回复），其余 7 个按差异表收束（授权目标不同 / 是否 late reply / 是否走脚本）。
- **触发时机表**：每个请求谁在什么时候发起，写在 18 §1 与 19 §1（INIT 脚本、service 命令、recovery 脚本、内核、SEF 自身）。

---

## 5. 每篇契约

> 格式固定七要素。知识点编号引用 §2；「源」列：存＝旧文档位置，新＝C/制品锚点。

### 00-rs-overview

- **一句话定位**：让读者在开始之前知道 RS 是谁、它在系统启动的哪一刻出现、它一辈子在做哪两件事、这个目录怎么读。
- **讲什么**：K-A01、K-A02、K-A03、K-A04、K-A05、K-A06、K-A07、K-A08、K-A09
- **不讲什么**：
  - 四步 boot 的每一步（→ 08）
  - 任何数据结构字段（→ 02）
  - 任何请求 handler（→ 18/19）
  - 常量定义（→ 99 只给索引）
- **前置**：无（建议先读 `../01-stage-kernel/09-vm-boot-protocol.md` 了解 boot 链背景，非本目录）
- **后置**：全部 28 篇
- **事实底线**：`com.h:61,77`；`kernel/table.c:46-64`；`kernel/main.c:197,203,257,264-265`；`servers/rs/main.c:38-131,158-494`；`os/servers/rs/src/lib.rs:3-6`；`servers/rs/Makefile`；`share/mk/bsd.own.mk:1499`；`os/servers/rs/Cargo.toml`；`os/servers/rs/src/testutil.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-A01 | RS 的身份 | 概念 | `com.h:61,77` | 全文第一问"RS 是谁" | 存：00 §1.1 |
  | K-A02 | boot 两层顺序语义 | 概念 | `kernel/table.c:46-64`、`kernel/main.c:264-265` | 定位 RS 在链上的位置 | 存：00 §1.2 |
  | K-A03 | 两个状态机 | 概念 | `main.c:38-131,158-494` | 一生的骨架 | 存：00 §1.3 |
  | K-A04 | 可靠性模型 | 概念 | `manager.c:1246` | 解释 reincarnation | 存：00 §3.2 |
  | K-A05 | 单线程事件循环执行模型 | 约束 | `os/servers/rs/src/lib.rs:3-6` | 决定后续所有"为什么不用锁" | 新 |
  | K-A06 | 构建与特性开关 | 工具与工程 | `Makefile`、`bsd.own.mk:1499`、`Cargo.toml` | 读者需要知道 `live-update` 默认关 | 存：01 §3.6、99 |
  | K-A07 | ARCH A-1~A-14 | 架构演进 | `plan.md §4` | 三处一致标注的索引 | 存：00 §5.4 |
  | K-A08 | 测试基建现状 | 工具与工程 | `testutil.rs` | 读者要知道怎么验证 | 新 |
  | K-A09 | stage 边界 | 约束 | `00-master-plan/README.md` | 防与其它 stage 重复 | 存：00 §4.2 |

- **验收标准**：读者能回答——(1) RS 的进程号与"root system process"含义；(2) "表内第 2"和"执行第 2"分别指什么、证据在哪两行；(3) RS 的两个状态机各自什么时候跑；(4) 为什么 RS 的 Rust 代码可以不用锁；(5) `live-update` 特性默认开还是关。必须画出：boot 因果链图（Kernel → VM → RS → 其余）+ 两条主线缩略图。

### 01-rs-external-face

- **一句话定位**：把 RS 对外部世界的所有依赖一次列清，使后续每一篇都不必再解释"这个调用是什么意思"。
- **讲什么**：K-B01~K-B17、K-O06
- **不讲什么**：
  - 每个调用在什么业务时机被发出（→ 03/08/13 等各篇）
  - 常量定义（→ 各机制篇；本篇只给签名与语义）
- **前置**：00
- **后置**：02~27 全部
- **事实底线**：`include/minix/com.h:61,77,463-492,724-744`；`include/minix/ipc.h` 的 `mess_rs_*`/`mess_lsys_*`；`include/minix/syslib.h`；`servers/rs/proto.h`、`inc.h`；`lib/libsys/sef*.c`（9 个文件）；`os/servers/rs/src/error.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-B01 | `sys_privctl` 操作面 | 接口 | `main.c:287` 等 | 后续每步 boot 都用 | 存：03 §2 |
  | K-B02 | `sys_getpriv` | 接口 | `main.c:294` | 与 SET 成对 | 存：03 §2 |
  | K-B03 | `sys_getimage` | 接口 | `main.c:196` | boot 输入 | 存：01 §2 |
  | K-B04 | `sys_getmachine`/`GET_HZ` | 接口 | `main.c:53,181` | boot 期只读查询 | 存：01 §2 |
  | K-B05 | `sys_setalarm` | 接口 | `main.c:433` | 心跳节奏源 | 存：01 §2 |
  | K-B06 | 五个低频调用 | 接口 | `manager.c:399,1460+` 等 | 单列一次免得各篇重复 | 存：19 §2 |
  | K-B07 | cpf grant 三函数 | 接口 | `main.c:185,464` | grant 生命周期集中讲 | 存：17 §2 |
  | K-B08 | `srv_*` 三函数 | 接口 | `manager.c:576,634,470` | 进程生杀大权 | 存：19 §2 |
  | K-B09 | pid/endpoint 换算 | 接口 | `main.c:426` 等 | 三个方向一次讲清 | 存：19 §2 |
  | K-B10 | VM 面四动词 | 接口 | `manager.c:663` 等 | 与 VM 的全部接口 | 存：19 §2 |
  | K-B11 | DS 面 | 接口 | `manager.c:787+` | 名字服务 | 存：11/19 |
  | K-B12 | 调度面 | 接口 | `utility.c:364` | 调度器进出 | 存：03/19 |
  | K-B13 | DEVMAN/PCI/mapdriver | 接口 | `manager.c:787-860` | 设备侧依赖 | 存：11/19 |
  | K-B14 | `libexec_*`/`minix_stack_*` | 接口 | `exec.c:21-60` | 装载库族 | 存：09 §2 |
  | K-B15 | 消息槽线格式 | 接口与协议 | `include/minix/ipc.h` | 每个消息用哪个槽 | 存：19 §3 |
  | K-B16 | errno 与 `Errno` newtype | 约束 | `error.rs` | 跨语言对齐 | 存：99 §2.5 |
  | K-B17 | panic / fail-closed 策略 | 约束 | `main.c` 13 处 panic | 两种失败策略的分界 | 新 |
  | K-O06 | `proto.h`/`inc.h` | 工具 | 两文件 | 签名来源声明 | 存：00 §5.5 |

- **验收标准**：读者拿到任意一处 RS 的 C 代码，能在这篇查到每个外部调用的签名、作用与 Rust 对应项。必须给出：内核调用面表（调用名 / C 锚点 / 作用一句话 / Rust 侧位置）、邻接服务面表（服务 / 请求 / 消息类型 / C 锚点）、消息槽对照表（`RS_*` 类型 → `mess_*` 槽）。

### 02-rs-process-table

- **一句话定位**：给出 RS 记忆状态的唯一数据结构，以及围绕它的查找/分配/释放/校验原语。
- **讲什么**：K-C01~K-C15
- **不讲什么**：
  - `rprocupd`/`rupdate` 的**语义**（→ 21）
  - 权限字段怎么填（→ 03）
  - 谁在什么时机改这些字段（→ 08/11/13/25）
- **前置**：00、01
- **后置**：03、04、05、06、08、09、11、13、16、17、21、26
- **事实底线**：`type.h:56-108`（rproc）、`rs.h:165-183`（rprocpub）、`type.h:7-52`、`glo.h:33-45`、`const.h:28-45`、`manager.c:1935-2113`、`utility.c:352`、`os/servers/rs/src/service_slot.rs`、`process_table.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-C01 | `rproc` 全字段 | 数据结构 | `type.h:56-108` | 数据底座 | 存：02 §2.1 |
  | K-C02 | `rprocpub` 全字段 | 数据结构 | `rs.h:165-183` | 公开部分 | 存：02 §2.2 |
  | K-C03 | 双数组绑定 | 机制 | `main.c:230-237` | 私/公同步点 | 存：02 §2.3 |
  | K-C04 | `rproc_ptr` | 机制 | `glo.h:35` | 快速索引 | 存：02 §2.3 |
  | K-C05 | `r_flags` 16 位 | 数据结构 | `const.h:28-43` | 状态位权威表 | 存：02 §2.4 |
  | K-C06 | `SF_*` 13 位 | 数据结构 | `rs.h:191-203` | 策略位权威表 | 存：02 §2.5 |
  | K-C07 | `IMM_SF` | 约束 | `rs.h:205-206` | 不可变性 | 存：08 §2 |
  | K-C08 | `RS_SRV_IS_IDLE` | 约束 | `const.h:45` | 空闲判定 | 存：02/06 |
  | K-C09 | 查找五式 | 机制 | `manager.c:1935-2066` | 外部寻址 | 存：02 §2.6 |
  | K-C10 | alloc/free | 机制 | `manager.c:2067,2088` | 槽位生死 | 存：02 §2.6 |
  | K-C11 | `rs_isokendpt` | 机制 | `utility.c:352` | 主循环敢 panic 的前提 | 存：02 §2.6 |
  | K-C12 | `rinit` | 数据结构 | `glo.h:40` | 初始化信息载体 | 存：02 §2.7 |
  | K-C13 | `rupdate`/`rprocupd` 结构 | 数据结构 | `type.h:30-52` | 形状先于语义 | 存：02 §2.7 |
  | K-C14 | 四链 | 数据结构 | `type.h:58-61` | 版本链 vs 副本链 | 存：02 §2.3 |
  | K-C15 | ARCH A-3/A-4 | 架构演进 | `service_slot.rs` | Rust 侧替换 | 存：02 §3 |

- **验收标准**：读者能逐字段说出 `rproc` 每个成员的用途，能说出 16 个 `r_flags` 位各自的含义与置位者，能画出四链（`r_old_rp`/`r_new_rp`/`r_prev_rp`/`r_next_rp`）的拓扑图并说明"版本链"与"副本链"的区别。

### 03-rs-privilege

- **一句话定位**：讲清 RS 怎么为每个服务构造内核权限结构，并把"能调什么、能发给谁"写进内核。
- **讲什么**：K-D01~K-D10
- **不讲什么**：
  - `s_ipc_to` 位图的**动态**计算（→ 04）
  - "谁有资格命令 RS"（→ 05）
  - privctl 的**签名**（→ 01）
- **前置**：00、01、02
- **后置**：04、05、08、13、18、27
- **事实底线**：`kernel/priv.h`；`main.c:244-346`（Step 1 的 priv 段 258-297）；`utility.c:82-99`（fill_send_mask）、`100-140`（fill_call_mask）、`364-386`（sched_init_proc）、`387-422`（update_sig_mgrs）；`const.h:71`（SRV_OR_USR）；`manager.c:600,1224,1234`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-D01 | priv 结构建模 | 数据结构 | `kernel/priv.h` | 权限存哪 | 存：03 §1.2 |
  | K-D02 | `static_priv_id` | 机制 | `main.c:265` | boot 服务固定 id | 存：03 §2.1 |
  | K-D03 | 五个 s_* 字段 | 机制 | `main.c:269-275` | 逐字段语义 | 存：03 §2.1 |
  | K-D04 | `fill_call_mask`×2 | 机制 | `main.c:278-280,316-317` | 两张掩码 | 存：03 §2.2 |
  | K-D05 | `fill_send_mask` | 机制 | `utility.c:82` | 全置快捷路径 | 存：03 §2.2 |
  | K-D06 | privctl 每操作效果 | 接口 | `main.c:287` 等 | 逐个对照 | 存：03 §2.3 |
  | K-D07 | RS/VM 例外 | 约束 | `main.c:285-291` | 两个已在跑的服务 | 存：03 §2.1 |
  | K-D08 | `sched_init_proc` | 机制 | `utility.c:364` | 调度参数落地 | 存：03 §2.4 |
  | K-D09 | `update_sig_mgrs` | 机制 | `utility.c:387` | 信号管理器切换 | 存：03 §2.5 |
  | K-D10 | `SRV_OR_USR` 默认族 | 约束 | `const.h:71` | 两套默认值 | 存：03 §2.1 |

- **验收标准**：读者能独立写出"给一个新服务配权限"的完整步骤（static id → flags → init_flags → trap_mask → send mask → call mask ×2 → sig mgr → SET_SYS → GETPRIV → ALLOW），并说明 RS/VM 在哪一步被跳过、为什么。

### 04-rs-ipc-sendmask

- **一句话定位**：讲清"服务能向谁发消息"这张 64 位位图的**动态**计算路径。
- **讲什么**：K-D11、K-D12、K-D13（并引用 K-D05）
- **不讲什么**：
  - priv 结构其余字段与 privctl 提交（→ 03）
  - `r_ipc_list` 怎么被拷进槽（→ 11 §5）
  - 内核如何强制这张位图（→ `../01-stage-kernel/23-ipc-filter.md`）
- **前置**：00、01、02、03
- **后置**：05、11、18
- **事实底线**：`manager.c:2115-2156`（get_next_name）、`2157-2229`（add_forward_ipc）、`2230-2299`（add_backward_ipc）、`2300-2332`（init_privs）；`rs.h:29-30`；`os/servers/rs/src/ipc_mask.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-D11 | `get_next_name` | 机制 | `manager.c:2115` | 解析第一步 | 存：05 §2.1 |
  | K-D12 | forward/backward/init_privs | 机制 | `manager.c:2157,2230,2300` | 位图构造三步 | 存：05 §2.2-2.4 |
  | K-D13 | `IPC_ALL`/`IPC_ALL_SYS` | 约束 | `rs.h:29-30` | 通配名 | 存：05 §2.4 |

- **验收标准**：读者拿到一个 `r_ipc_list`（如 `"pm vfs IPC_ALL_SYS"`）能手工算出最终的 `s_ipc_to` 位图，并解释 forward 与 backward 两次遍历分别解决什么问题（反向补位是为了让被点名的服务也能回发）。

### 05-rs-access-control

- **一句话定位**：讲清一个 `RS_*` 请求到达后，RS 凭什么决定"这个调用者可以对这个服务做这个操作"。
- **讲什么**：K-D14、K-D15、K-D16
- **不讲什么**：
  - `r_control` 列表怎么被填充（→ 11）
  - 各 handler 的业务语义（→ 18/19/21）
  - 权限结构本身（→ 03）
- **前置**：00、01、02、03
- **后置**：18、19、21
- **事实底线**：`manager.c:21-38`（caller_is_root）、`39-80`（caller_can_control）、`81-134`（check_call_permission）；`type.h:106-107`；`rs.h:55`；`const.h:105`；`request.c` 11 个调用点；`os/servers/rs/src/access.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-D14 | 三个授权函数 | 机制 | `manager.c:21,39,81` | 两级授权模型 | 存：04 §2 |
  | K-D15 | `r_control` 列表 | 数据结构 | `type.h:106-107` | 非 root 的授权来源 | 存：04 §2.2 |
  | K-D16 | update 期 EBUSY | 约束 | `const.h:105` | 唯一的时间相关规则 | 存：04 §2.3 |

- **验收标准**：读者能对任意 (caller, request, target) 三元组判定结果（OK / EPERM / EBUSY），并列出判定经过的每一条规则；必须给出 11 个调用点清单表。

### 06-rs-boot-tables

- **一句话定位**：讲清 boot 的静态输入——三张表长什么样、怎么查找、不一致时会发生什么。
- **讲什么**：K-E01、K-E02、K-E03、K-E04、K-E05、K-E06、K-E20
- **不讲什么**：
  - 表里的数据怎么被用（→ 08）
  - 内核侧 boot_image 的构建（→ 01-stage-kernel）
- **前置**：00、01、02
- **后置**：08
- **事实底线**：`table.c:15-50`；`type.h:7-26`；`const.h:61-62`；`main.c:709-779`（boot_image_info_lookup）、`216-227`（一致性校验）；`kernel/table.c:39-41,46-64`；`os/servers/rs/src/table.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-E01 | 三张静态表 | 数据结构 | `table.c:15-50` | boot 输入 | 存：01 §2.1 |
  | K-E02 | 三个行结构 | 数据结构 | `type.h:7-26` | 行格式 | 存：02 §2.7 |
  | K-E03 | 哨兵与默认项 | 约束 | `const.h:61-62` | 缺省回落 | 存：01 §2.1 |
  | K-E04 | 四表查找 | 机制 | `main.c:709-779` | 查不到就 panic/默认 | 存：01 §2.2 |
  | K-E05 | 一致性校验 | 约束 | `main.c:216-227` | 硬校验 | 存：01 §2.1 |
  | K-E06 | 顺序 = NOTIFY 优先级 | 概念 | `kernel/table.c:39-41` | 解释 DS/RS 排前面 | 存：00 §1.1 |
  | K-E20 | ARCH A-13 | 架构演进 | `table.rs` | Rust 静态表 | 存：01 §3 |

- **验收标准**：读者能解释 `boot_image_sys_table` 与 `boot_image_dev_table` 的"默认项"机制为什么不会产生越界；能说出"priv 表项数 ≠ image 中系统服务数"时 RS 的行为与行号。

### 07-rs-sef-startup

- **一句话定位**：讲清 RS 是被什么框架拉起来的、它注册了哪些回调、以及 fresh/restart/lu 三条初始化路径怎么被选中。
- **讲什么**：K-E07、K-E08（顺带把 `SEF_INIT_*`/`SEF_LU_*`/`SEF_CB_*` 三族标志一次讲全，消除 R-7 重复）
- **不讲什么**：
  - 各回调体的内容（→ 08 fresh / 17 response / 26 signal / 27 restart+lu）
  - SEF 库内部实现（→ `lib/libsys/sef*.c`，只列文件面）
- **前置**：00、01、02
- **后置**：08、17、26、27
- **事实底线**：`main.c:136-153`；`sef.h:53-56`（sef_init_info_t）、`56-90`（回调类型与默认值）、`93-103`（SEF_INIT_*）、`213-216`（SEF_LU_STATE_*）；`lib/libsys/` 下 9 个 `sef*.c`；`os/servers/rs/src/sef.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-E07 | SEF 框架与七类回调 | 概念/接口 | `main.c:136-153`、`sef.h:56-90` | 框架本身此前无专篇 | 存：01 §2.1 / 新 |
  | K-E08 | init type 判定 | 机制 | `main.c:151`、`sef.h:93-95` | 三条路径的分叉点 | 存：01 §2.1 |

- **验收标准**：读者能列出 7 个回调的名字、注册函数、触发时机、RS 的实现函数名与所在篇章；能说出 `SEF_INIT_FRESH/LU/RESTART` 三个 type 分别由什么条件决定；能列出 9 个 `sef*.c` 文件各自管什么。

### 08-rs-boot-init

- **一句话定位**：讲清 RS 如何用四步把自己和其它 boot 服务启动起来，以及 `RS_INIT` 消息的发送侧。
- **讲什么**：K-E09、K-E10、K-E11、K-E12、K-E13、K-E14、K-E15、K-E16、K-E17、K-E18、K-E19
- **不讲什么**：
  - 三张表的结构与查找（→ 06）
  - SEF 回调模型（→ 07）
  - `RS_INIT` 的**接收**侧（→ 17）
  - `USE_LIVEUPDATE` 自升级块的完整语义（→ 27；本篇只标注位置与开关，见 §7 序差表 X-3）
- **前置**：00、01、02、03、04、06、07
- **后置**：09、15、17
- **事实底线**：`main.c:158-494`（主）、`179`（verbose）、`181`（HZ）、`185`（grant）、`192-193`（复位）、`196`（getimage）、`230-237`（重置表）、`244-346`（Step 1）、`350-399`（Step 2）、`404-407`（Step 3）、`413-430`（Step 4）、`433`（alarm）、`784-821`（catch_boot_init_ready）；`utility.c:18-68`（init_service）
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-E09 | `rs_verbose` | 工具 | `main.c:179` | 第一步 | 存：01 §2.3 |
  | K-E10 | Step 1 全序列 | 机制 | `main.c:244-346` | boot 最长一步 | 存：01 §2.4 |
  | K-E11 | Step 2 放行 | 机制 | `main.c:350-399` | 三条动作 | 存：01 §2.5 |
  | K-E12 | `SF_SYNCH_BOOT` | 机制 | `main.c:390-397` | 同步/异步分界 | 存：01 §2.5 |
  | K-E13 | `catch_boot_init_ready` | 机制 | `main.c:784-821` | boot 专用接收器 | 存：12 §2.4 |
  | K-E14 | Step 4 getnpid | 机制 | `main.c:413-430` | 最后一补 | 存：01 §2.7 |
  | K-E15 | `init_service` 发送侧 | 机制/协议 | `utility.c:18-68` | 首次出现即完整 | 存：12 §2.2 |
  | K-E16 | ROOT_SYS_PROC 免发 | 约束 | `utility.c:18-68` | RS 的例外 | 存：12 §2.2 |
  | K-E17 | `rproctab_gid` 建立 | 机制 | `main.c:185` | 创建点（消费在 17） | 存：01 §2.3 |
  | K-E18 | 全局复位 | 约束 | `main.c:192-193` | 初值 | 存：01 §2.3 |
  | K-E19 | boot 期 panic | 约束 | `main.c` 13 处 | fail-fast | 新 |

- **验收标准**：读者能不看代码复述四步 boot 的每一步做了什么、每步之间的因果关系（为什么 Step 1 必须在 Step 2 之前），指出 RS/VM 在两步中的 2 处例外，并解释 `catch_boot_init_ready` 为什么不 reply VM。必须给出：四步时序图 + `RS_INIT` 消息字段表。

### 09-rs-main-loop

- **一句话定位**：讲清 RS 运行时的骨架：怎么收消息、怎么分类、什么时候回复、空闲期做什么。
- **讲什么**：K-F01~K-F10
- **不讲什么**：
  - 各 handler 的业务语义（→ 17/18/19/21/22/26，本篇只给分派表指针）
  - 信号通道的处理（→ 26）
  - 周期检查内部（→ 26）
- **前置**：00、01、02、03、08
- **后置**：10、17、18、19、21、22、26
- **事实底线**：`main.c:57-131`、`826-834`（get_work）；`com.h:92`（is_ipc_notify）；`utility.c:223-246`（rs_asynsend）、`309-331`（reply）、`332-351`（late_reply）、`424-442`（rs_is_idle）、`443-484`（rs_idle_period）；`os/servers/rs/src/dispatch.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-F01 | 循环骨架 | 机制 | `main.c:57-131` | 心脏 | 存：06 §2.1 |
  | K-F02 | 四类分类 | 机制 | `main.c:70-129` | 分类树 | 存：06 §2.2 |
  | K-F03 | `is_ipc_notify` | 机制 | `com.h:92` | 状态字 | 存：06 §2.2 |
  | K-F04 | `reply` | 机制 | `utility.c:309` | 回复第一条规则 | 存：06 §2.4 |
  | K-F05 | `late_reply` | 机制 | `utility.c:332` | 异步续接 | 存：06 §2.5 |
  | K-F06 | `EDONTREPLY` | 协议 | `main.c:125-128` | 唯一不回信号 | 存：06 §2.3 |
  | K-F07 | `rs_asynsend` | 机制 | `utility.c:223` | 只发不等 | 存：06 §2.6 |
  | K-F08 | `rs_idle_period` | 机制 | `utility.c:443` | 空闲期 | 存：06 §2.7 |
  | K-F09 | `rs_is_idle` | 约束 | `utility.c:424` | 判定式 | 存：06 §2.7 |
  | K-F10 | 心跳接收 | 机制 | `main.c:85-91` | 最短一支 | 存：07 §2 |

- **验收标准**：读者能画出四类消息的判定树；能解释 `EDONTREPLY` 与 `RS_LATEREPLY` 的配合（谁置位、谁补回、补回时用什么字段找调用者）；能说出空闲期做的两件事及其触发条件。

### 10-rs-lifecycle

- **一句话定位**：给读者一张服务生命周期的全景图与状态位迁移图，作为 11~17 与 25/26 的导航。
- **讲什么**：K-G01、K-G02
- **不讲什么**：每一站的实现细节（→ 11~17、25、26）
- **前置**：00~09
- **后置**：11~19、25、26
- **事实底线**：`const.h:28-43`（状态位）；`request.c:15-106`（RS_UP 全景）；`manager.c:950-987`（start_service 编排）；`main.c:100-122`（请求入口）
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-G01 | 十站全景 | 概念 | `00 §3.1` + `manager.c:950-987` | 唯一权威全景 | 存：00 §3.1 |
  | K-G02 | 状态位迁移图 | 概念 | `const.h:28-43` | 旧文档从未给出 | 新 |

- **验收标准**：读者能对着全景图说出任意一站的前驱与后继、该站由哪个 C 函数实现、在哪一篇讲；能对着状态位迁移图说出"服务从 `RS_IN_USE` 走到 `RS_DEAD` 需要经过哪些位、每条边由哪个函数触发"。

### 11-rs-slot-config

- **一句话定位**：讲清请求方的声明（`rs_start`）如何被校验、拷贝、固化为槽位状态。
- **讲什么**：K-G03~K-G13
- **不讲什么**：
  - 二进制装载（→ 12）
  - 进程创建（→ 13）
  - `init_privs` 的算法（→ 04；本篇只标注调用点）
- **前置**：00~05、10
- **后置**：12、13、18、21
- **事实底线**：`rs.h:104-151`（rs_start）、`33-52`（RSS_*）；`request.c:1265-1309`（check_request）；`manager.c:135-150`（copy_rs_start）、`151-173`（copy_label）、`289-327`（build_cmd_dep）、`1303-1333`（inherit_service_defaults）、`1460-1707`（edit_slot）、`1708-1799`（init_slot）；`os/servers/rs/src/slot.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-G03 | `rs_start` 全字段 | 数据结构 | `rs.h:104-151` | 声明格式 | 存：08 §2.1 |
  | K-G04 | `RSS_*` 20 位 | 数据结构 | `rs.h:33-52` | 请求方开关 | 存：08 §2.2 |
  | K-G05 | `check_request` | 机制 | `request.c:1265` | 校验 | 存：08 §2.3 |
  | K-G06 | 跨地址空间拷贝 | 机制 | `manager.c:135,151` | 用户指针进入 | 存：08 §2.4 |
  | K-G07 | `init_slot` | 机制 | `manager.c:1708` | 首次落槽 | 存：08 §2.5 |
  | K-G08 | `edit_slot` | 机制 | `manager.c:1460` | 改槽 | 存：08 §2.6 |
  | K-G09 | `RSS_*→SF_*` 映射 | 约束 | `manager.c:1618-1705` | 固化规则 | 存：08 §2.6 |
  | K-G10 | `build_cmd_dep` | 机制 | `manager.c:289` | argv 解析 | 存：08 §2.7 |
  | K-G11 | `inherit_service_defaults` | 机制 | `manager.c:1303` | 继承 | 存：08 §2.8 |
  | K-G12 | 三个计数器覆盖 | 约束 | edit_slot 段 | 可写时机 | 存：08 §2.6 |
  | K-G13 | 预分配请求 | 机制 | `rs.h:120-121` | 给 VM 的输入 | 存：08/16 |

- **验收标准**：读者能给出完整的 `RSS_*` → `SF_*` 映射表（含 core 服务设置 `RSS_NORESTART` 会 EPERM 的例外）；能对一份 `rs_start` 逐字段指出它由 `init_slot` 的哪一行落到哪个槽位字段。

### 12-rs-exec

- **一句话定位**：讲清服务的二进制怎么被读进来、怎么被装进新进程的地址空间。
- **讲什么**：K-H01~K-H06
- **不讲什么**：
  - 进程怎么被创建（→ 13）
  - ELF 格式本身（→ `minix-elf` crate / 14-stage-runtime）
- **前置**：00~05、10、11
- **后置**：13
- **事实底线**：`exec.c:21-60`（srv_execve）、`62-120`（do_exec）、`121-142`（exec_restart）、`143-165`（read_seg）；`manager.c:1357-1371`（share_exec）、`1372-1423`（read_exec）、`1424-1459`（free_exec）；`rs.h:33-35`；`os/servers/rs/src/exec.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-H01 | read/share/free_exec | 机制 | `manager.c:1372,1357,1424` | 内存副本生命周期 | 存：09 §2.3 |
  | K-H02 | srv_execve/do_exec/exec_restart | 机制 | `exec.c:21,62,121` | 三步装载 | 存：09 §2.1-2.2 |
  | K-H03 | `read_seg`/`exec_info` | 机制 | `exec.c:143` | 段读取 | 存：09 §2.2 |
  | K-H04 | 栈帧构建 | 机制 | `exec.c:21-60` | 参数摆放 | 存：09 §2.1 |
  | K-H05 | COPY/NEED_COPY | 约束 | `rs.h:34,194,195` | 副本策略 | 存：09 §2.4 |
  | K-H06 | ARCH A-5/A-8 | 架构演进 | `exec.rs` | Rust 替换 | 存：09 §3 |

- **验收标准**：读者能说出 `RSS_COPY` 与 `SF_NEED_COPY` 分别在哪一步被消费、`RSS_REUSE` 如何避免重复读取、以及装载失败时 `exec_restart` 走什么协议回到 PM。

### 13-rs-service-create

- **一句话定位**：讲清一个服务进程怎么被真正造出来，以及两个实例之间怎么互换。
- **讲什么**：K-H07~K-H16
- **不讲什么**：
  - 装载细节（→ 12）
  - 对外发布（→ 14）
  - 更新的**阶段状态机**（→ 21~24；本篇只讲 `update_service` 这个置换原语本身）
- **前置**：00~05、10、11、12
- **后置**：14、15、21、23、24、25、27
- **事实底线**：`manager.c:531-708`（create_service）、`713-786`（clone_service）、`1800-1855`（clone_slot）、`1856-1932`（swap_slot）、`1013-1032`（activate_service）；`update.c:262-329`（update_service）；`main.c:441`；`os/servers/rs/src/service_create.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-H07 | `create_service` 全序列 | 机制 | `manager.c:531-708` | 诞生九步 | 存：10 §2.1 |
  | K-H08 | fork 后重新 pin | 机制 | `manager.c:570-581,604` | 内存一致性 | 存：10 §2.1 |
  | K-H09 | `setuid(0)` hack | 约束 | `manager.c:646-656` | 必须保留的怪癖 | 存：10 §2.1 |
  | K-H10 | RS/VM 实例特殊处理 | 机制 | `manager.c:658-695` | 两个特殊进程 | 存：10 §2.1 |
  | K-H11 | `vm_set_priv` | 机制 | `manager.c:697-702` | 第二张权限表 | 存：10 §2.1 |
  | K-H12 | `clone_service` | 机制 | `manager.c:713-786` | 副本链接 | 存：10 §2.2 |
  | K-H13 | `clone_slot` | 机制 | `manager.c:1800` | 深浅拷贝 | 存：10 §2.4 |
  | K-H14 | `swap_slot` | 机制 | `manager.c:1856-1932` | 链交换 | 存：10 §2.5 |
  | K-H15 | `activate_service` | 机制 | `manager.c:1013` | 激活语义 | 存：10 §2.3 |
  | K-H16 | `update_service` 原语 | 机制 | `update.c:262-329` | old→new 通用换法（25/27 依赖） | 存：16 §2 |

- **验收标准**：读者能画出 `swap_slot` 交换的五个对象（四链 + update 链 + `rproc_ptr` 全局索引）并说明漏掉任一会导致什么；能解释 `RS_SWAP` 与 `RS_DONTSWAP` 的差别及各自的使用者（25 vs 27）。

### 14-rs-publish

- **一句话定位**：讲清服务怎么让全系统知道它的存在，以及撤销的对称操作。
- **讲什么**：K-H17、K-H18、K-H19
- **不讲什么**：
  - DS 服务本身（→ 07-stage-ds）
  - DEVMAN 内部（→ 11-stage-devman）
  - `setuid(0)` hack 的来龙去脉（→ 13 §3，本篇只标注同款）
- **前置**：00~05、10、13
- **后置**：15、16、18、25
- **事实底线**：`manager.c:787-863`（publish_service）、`864-922`（unpublish_service）；`include/minix/ds.h`；`os/servers/rs/src/publish.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-H17 | `publish_service` | 机制 | `manager.c:787-863` | 四步宣告 | 存：11 §2.1 |
  | K-H18 | `unpublish_service` | 机制 | `manager.c:864-922` | 对称撤销 | 存：11 §2.2 |
  | K-H19 | ARCH A-10（PCI 占位） | 架构演进 | `publish.rs` | fail-closed 默认 | 存：11 §3 |

- **验收标准**：读者能列出发布的四个动作、每个动作失败时 RS 的处理（都是 `kill_service`），以及撤销时四个动作的逆序；能说明没有 PCI 支持时 `PciAcl` 占位为何必须是 fail-closed。

### 15-rs-start-run

- **一句话定位**：讲清三个编排原语——把一个槽位真正跑起来、以及让它停下。
- **讲什么**：K-H20、K-H21、K-H22
- **不讲什么**：
  - 创建/装载/发布的细节（→ 12/13/14）
  - 初始化握手（→ 17）
- **前置**：00~05、10、13、14、08（`init_service` 已在 08 首次出现）
- **后置**：17、18、25、27
- **事实底线**：`manager.c:950-987`（start_service）、`923-949`（run_service）、`988-1012`（stop_service）；`main.c:1048`（reincarnate 调用点）；`utility.c:18-68`（init_service 引用）
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-H20 | `start_service` 编排 | 机制 | `manager.c:950-987` | 四步串起 | 存：12 §2.1 |
  | K-H21 | `run_service` 原语 | 机制 | `manager.c:923-949` | 放行+初始化 | 存：12 §2.2 |
  | K-H22 | `stop_service` 原语 | 机制 | `manager.c:988-1012` | 停止第一步 | 存：13 §2 |

- **验收标准**：读者能说出 `start_service` 的四步及每步失败后的清理责任归属；能说明 `stop_service` 为什么对 RS 自己用 SIGHUP 而不是 SIGTERM。

### 16-rs-teardown

- **一句话定位**：讲清 RS 放弃一个服务实例的全部公共原语——它们被创建失败、发布失败、初始化失败、终止、清理等所有路径复用。
- **讲什么**：K-I01~K-I08
- **不讲什么**：
  - 什么时候该走哪条终局分支（→ 25）
  - 谁周期性地触发它们（→ 26）
- **前置**：00~05、10、13、14、15、09（`late_reply`）
- **后置**：17、18、24、25、27
- **事实底线**：`manager.c:360-375`（kill_service）、`380-400`（crash_service）、`405-492`（cleanup_service）、`497-526`（detach_service）、`1185-1241`（run_script）、`1334-1356`（get_service_instances）；`const.h:25`；`os/servers/rs/src/recovery.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-I01 | `kill_service` | 机制 | `manager.c:360` | 致命/非致命分界 | 存：15 §2.4 |
  | K-I02 | `crash_service` | 机制 | `manager.c:380` | 模拟崩溃 | 存：15 §2.4 |
  | K-I03 | `cleanup_service` 两段式 | 机制 | `manager.c:405` | 先标记后清理 | 存：15 §2.5 |
  | K-I04 | `detach_service` | 机制 | `manager.c:497` | 保留尸体 | 存：15 §2.6 |
  | K-I05 | `run_script` | 机制 | `manager.c:1185` | 双调用点（cleanup 477 / restart 1257） | 存：15 §2.3 |
  | K-I06 | `get_service_instances` | 机制 | `manager.c:1334` | 遍历副本链 | 存：15 §2.7 |
  | K-I07 | `MAX_DET_RESTART` | 约束 | `const.h:25` | detach 闸门 | 存：15 §2.6 |
  | K-I08 | ARCH A-1 | 架构演进 | `recovery.rs` | fork 缺口 | 存：15 §3 |

- **验收标准**：读者能解释 `cleanup_service` 为什么必须分两次调用（第一次只标记 `RS_DEAD` 并 DISALLOW、第二次才真杀），以及这个两段式如何避免 IPC 调用者永久阻塞；能说出 `run_script` 传给脚本的三个参数（label / reason / incarnation）及三种 reason。

### 17-rs-ready

- **一句话定位**：讲清服务初始化完成如何上报给 RS，以及 RS 如何收尾。
- **讲什么**：K-J01、K-J02、K-J03、K-J04、K-J06、K-J07（K-J05 回指 08）
- **不讲什么**：
  - `RS_INIT` 的发送侧（→ 08）
  - `do_init_ready` 的 update 分支（→ 24 §5）
  - `do_upd_ready`（→ 22 §4）
- **前置**：00~05、08、09、10、15、16
- **后置**：18、23、24
- **事实底线**：`request.c:462-528`（do_init_ready，`514-526` 非 update 分支）；`manager.c:328-355`（end_srv_init）；`main.c:591-609,614-626`（两个 response 回调）；`error.c:33,48`；`sef.h:98-103`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-J01 | 非 update 分支 | 机制 | `request.c:514-526` | 成功路径三件事 | 存：12 §2.3 |
  | K-J02 | 失败分支置位 | 机制 | `request.c:488-497` | 复活决策 | 存：12 §2.3 |
  | K-J03 | `end_srv_init` | 机制 | `manager.c:328-355` | 收尾四件事 | 存：12 §2.5 |
  | K-J04 | 两个 response 回调 | 机制 | `main.c:591,614` | RS 自模拟 ready | 存：12 §2.6 |
  | K-J06 | `init_strerror` 表 | 数据结构 | `error.c:48` | 初始化错误翻译 | 存：99 §2.5 |
  | K-J07 | `SEF_INIT_*` 强制失败标志 | 数据结构 | `sef.h:98-103` | 五个调试开关 | 存：08/13 |

- **验收标准**：读者能说出 `do_init_ready` 成功后做的三件事及其顺序（为什么 reply 必须在 `end_srv_init` 之前）；能解释 `r_restarts` 在哪里自增、为什么；能说出 `end_srv_init` 里 `rupdate_upd_move` 的作用（并知道它在 24 展开）。

### 18-rs-control-requests

- **一句话定位**：讲清外部通过 8 个控制类请求操作服务生命周期的完整语义。
- **讲什么**：K-K01~K-K09
- **不讲什么**：
  - 授权检查算法（→ 05）
  - 各原语的实现（→ 13/15/16）
  - 查询类请求（→ 19）
- **前置**：00~05、09、10、11、13、14、15、16、17
- **后置**：19、25
- **事实底线**：`request.c:15-106`（up）、`111-155`（down）、`160-207`（restart）、`208-252`（clone）、`253-297`（unclone）、`298-389`（edit）、`390-430`（refresh）、`431-461`（shutdown）
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-K01 | `do_up` | 机制 | `request.c:15-106` | 生命周期入口 | 存：13 §2.1 |
  | K-K02 | `RSS_NOBLOCK` | 约束 | `request.c:49,96` | 不等初始化 | 存：13 §2.1 |
  | K-K03 | 重复检查 | 机制 | `request.c:71-87` | 三种唯一性 | 存：13 §2.1 |
  | K-K04 | `do_down` | 机制 | `request.c:111-155` | 含脚本分支 | 存：13 §2.2 |
  | K-K05 | `do_restart` | 机制 | `request.c:160-207` | 仅脚本场景 | 存：13 §2.3 |
  | K-K06 | `do_refresh` | 机制 | `request.c:390-430` | 与 restart 之别 | 存：13 §2.4 |
  | K-K07 | `do_shutdown` | 机制 | `request.c:431-461` | 关机传播 | 存：13 §2.5 |
  | K-K08 | clone/unclone | 机制 | `request.c:208-297` | 手工副本 | 存：13 §2.6 |
  | K-K09 | `do_edit` | 机制 | `request.c:298-389` | 热改参数 | 存：13 §2.7 |

- **验收标准**：读者能给出 8 个请求的差异表（授权目标 / 是否 late reply / 是否走脚本 / 主要副作用），并能独立推导 `do_up` 的七步（授权 → 分配 → 拷贝 → 校验 → 落槽 → 去重 → 启动 → 延迟回复）。**必须纠正旧文档的 `crash_service` 归属错误**（见 §1.4 勘误：`request.c:494` 属 `do_init_ready` 而非 `do_shutdown`）。

### 19-rs-query-requests

- **一句话定位**：讲清外部如何观测 RS 状态与注入故障。
- **讲什么**：K-K10、K-K11、K-K12（`SRV_STATUS` 部分）、K-K13、K-K14、K-K15
- **不讲什么**：
  - `RS_SYSCTL_UPD_*` 四个子功能（→ 21 §7）
  - 各控制请求的语义（→ 18）
- **前置**：00~05、09、10、13、14
- **后置**：无（末端）
- **事实底线**：`request.c:1095-1143`（getsysinfo）、`1144-1180`（lookup）、`1181-1228`（sysctl）、`1229-1264`（fi）；`utility.c:69-81`（fi_service）、`142-222`（srv_to_string_gen / srv_upd_to_string）、`485-546`（print_*）
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-K10 | `do_lookup` | 机制 | `request.c:1144` | 名字→endpoint | 存：14 §2.1 |
  | K-K11 | `do_getsysinfo` | 机制 | `request.c:1095` | 三张表导出 | 存：14 §2.2 |
  | K-K12 | `do_sysctl`（SRV_STATUS） | 机制 | `request.c:1181` | 状态查询 | 存：14 §2.3 |
  | K-K13 | `do_fi`/`fi_service` | 机制 | `request.c:1229`、`utility.c:69` | 故障注入 | 存：14 §2.4 |
  | K-K14 | print_* 三函数 | 工具 | `utility.c:485,516,189` | 人读输出 | 存：14 §2.5 |
  | K-K15 | `srv_to_string_gen` → Display | 工具 | `utility.c:142` | static 池替换 | 存：99/14 |

- **验收标准**：读者能说出 `do_getsysinfo` 导出的三张表分别是什么、谁消费；能用 `do_fi` 构造一次服务崩溃并说出它最终落到的函数（→ 16 §2）。

### 20-rs-state-data

- **一句话定位**：讲清 Live Update 中被迁移的那份数据长什么样、怎么解析。
- **讲什么**：K-L01~K-L07
- **不讲什么**：
  - 谁发起迁移、什么时候迁移（→ 21~24）
  - IPC filter 在内核如何生效（→ 01-stage-kernel）
- **前置**：00~05、10
- **后置**：21、23
- **事实底线**：`rs.h:88-101`（rs_state_data / rs_ipc_filter_el）；`manager.c:174-288`（init_state_data）；`ipc_filter.h:10-12`（ANY_*）、`15-21`（IPCF_*）；`type.h:39`；`os/servers/rs/src/state_data.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-L01 | 两个结构 | 数据结构 | `rs.h:88-101` | 迁移内容格式 | 存：17 §2.1 |
  | K-L02 | `init_state_data` | 机制 | `manager.c:174-288` | 解析流程 | 存：17 §2.2 |
  | K-L03 | `ANY_*` 通配 | 约束 | `ipc_filter.h:10-12` | 三类通配 | 存：17 §2.2 |
  | K-L04 | `IPCF_*` 标志 | 数据结构 | `ipc_filter.h:18-21` | 黑白名单 | 存：17 §2.3 |
  | K-L05 | grant 缓冲区 | 机制 | `type.h:39` | 跨地址空间 | 存：17 §2.2 |
  | K-L06 | VM 保底条目 | 约束 | `manager.c:174-288` | 不能滤掉 RS 自己 | 存：17 §2.2 |
  | K-L07 | ARCH A-14 布局差异 | 架构演进 | `state_data.rs` | 线格式宽度 | 存：17 §3 |

- **验收标准**：读者能手工把一条 `"pm:3 whitelist"` 形式的过滤串解析成 `rs_ipc_filter_el`；能说明为什么必须保留一条允许 RS 通信的保底条目；能说出 x86-64 与 i386 下结构体宽度的差异数字与后果。

### 21-rs-lu-request

- **一句话定位**：讲清一次 Live Update 怎么被发起：请求解析、标志面、新实例准备、链构造，以及"本次更新如何结束"的契约。
- **讲什么**：K-M01、K-M02、K-M03、K-M04、K-M05（契约）、K-M06（契约）、K-M07；并承接 K-K12 的 `UPD_*` 部分（G-10）
- **不讲什么**：
  - prepare / switch / end 三个阶段的执行（→ 22/23/24）
  - 状态数据格式（→ 20）
- **前置**：00~05、09、11、13、16、20
- **后置**：22、23、24、19（回指）
- **事实底线**：`request.c:534-889`（do_update）、`1181-1228`（do_sysctl 的 UPD_*）；`update.c:7-22`（clear_upds）、`23-87`（add_upd）、`88-120`（set_new_upd_flags）、`121-134`（upd_init）、`135-163`（upd_clear）；`const.h:83,105-120`；`sef.h` LU 段
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-M01 | `do_update` 解析 | 机制 | `request.c:534-889` | 请求入口 | 存：16 §2.1 |
  | K-M02 | `SEF_LU_*` 六种形态 | 数据结构 | `sef.h` LU 段 | 形态分类 | 存：16 §2.1 |
  | K-M03 | 链构造五操作 | 机制 | `update.c:7,23,121,135,164` | 链的形状 | 存：16 §2.3 |
  | K-M04 | `RUPDATE_*` 宏族 | 约束 | `const.h:105-120` | 九个判定 | 存：16 §2.3 |
  | K-M05 | `end_update` 契约 | 协议 | `update.c:865-931` | 后续三篇都要调用它 | 存：16 §2.6 |
  | K-M06 | `abort_update_proc` 契约 | 协议 | `update.c:707-743` | 25 需要从这里进 | 存：16 §2.7 |
  | K-M07 | VM 默认 prealloc | 约束 | `const.h:83` | 8 MiB 来源 | 存：16 §2.1 |
  | K-K12(部分) | `RS_SYSCTL_UPD_*` | 机制 | `request.c:1181`、`com.h:486-489` | 同槽的另一半 | 存：14 §2.3 |

- **验收标准**：读者能说出 `end_update(result, reply_flag)` 的三个参数含义、`RS_REPLY`/`RS_CANCEL` 两种回复形态分别让调用者看到什么；能列出 `rupdate` 链的五个操作各自的调用时机；能解释 `SYSCTL_UPD_START/RUN/STOP/STATUS` 四个子功能与 `RS_UPDATE` 的关系。

### 22-rs-lu-prepare

- **一句话定位**：讲清 prepare 阶段：RS 如何逐个通知服务准备、如何收齐、超时怎么办。
- **讲什么**：K-M08、K-M09、K-M10、K-M11、K-M12
- **不讲什么**：
  - 请求解析与链构造（→ 21）
  - 切换与收尾（→ 23/24）
- **前置**：21
- **后置**：23、24、26（回指）
- **事实底线**：`update.c:185-229`（request_prepare_update_service）、`371-396`（update_period）、`401-462`（start_update_prepare）、`467-527`（prepare_next）；`request.c:890-942`（do_upd_ready）；`const.h:36`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-M08 | `request_prepare_update_service` | 机制 | `update.c:185` | 单个通知 | 存：16 §2.4 |
  | K-M09 | prepare / prepare_next | 机制 | `update.c:401,467` | 逐个推进 | 存：16 §2.4 |
  | K-M10 | `do_upd_ready` | 机制 | `request.c:890` | 完成上报 | 存：12/16 |
  | K-M11 | `RS_PREPARE_DONE` | 约束 | `const.h:36` | 状态标记 | 存：02/16 |
  | K-M12 | prepare 超时 | 机制 | `update.c:371-396` | 谁在周期里推进它 | 存：07 §2.4（散） |

- **验收标准**：读者能说出 prepare 阶段的推进循环（谁调 `start_update_prepare_next`、什么条件结束、结束时的两种出口），以及超时路径（`update_period` → `end_update(EINTR, RS_CANCEL)`）为什么必须由周期检查驱动。

### 23-rs-lu-switch

- **一句话定位**：讲清切换阶段：真正的实例替换如何发生，以及 VM 参与的 multicomponent 时序。
- **讲什么**：K-M13、K-M14、K-M15、K-M16、K-M17
- **不讲什么**：
  - `update_service` 原语本身（→ 13 §6）
  - 收尾与回滚（→ 24）
- **前置**：13、15、17、21、22
- **后置**：24
- **事实底线**：`update.c:532-616`（start_update）、`621-656`（start_srv_update）、`657-706`（complete_srv_update）、`230-261`（srv_update）、`483-508`（VM 批量 prepare）、`585-613`（等 VM 初始化）；`utility.c:247-308`（rs_receive_ticks，唯一调用点 `update.c:590`）
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-M13 | `start_update` | 机制 | `update.c:532` | 切换入口 | 存：16 §2.5 |
  | K-M14 | start/complete_srv_update | 机制 | `update.c:621,657` | 单服务两段 | 存：16 §2.5 |
  | K-M15 | `srv_update`/vm_update 分支 | 机制 | `update.c:230` | 何时真调 VM | 存：16 §2.5 |
  | K-M16 | VM multi 时序 | 机制 | `update.c:483-508,585-613` | 最复杂一段 | 存：16 §2.5 |
  | K-M17 | `rs_receive_ticks` | 机制 | `utility.c:247` | 唯一调用点在此 | 存：16（A-9） |

- **验收标准**：读者能解释"为什么必须在请求 VM 准备之前，先为其它服务批量调 `vm_prepare`"；能说出 VM 初始化超时后 RS 构造了什么消息、走了哪个函数（→ 17 §2 的 `do_init_ready`）。

### 24-rs-lu-end

- **一句话定位**：讲清一次更新如何收尾、如何放弃、如何回滚，以及 init ready 的 update 分支。
- **讲什么**：K-M18、K-M19、K-M20、K-M21、K-M22、K-M23、K-M24（并承接 K-J06 的 `lu_strerror`）
- **不讲什么**：
  - 前面三个阶段的推进（→ 21/22/23）
  - 状态数据格式（→ 20）
- **前置**：16、17、21、22、23
- **后置**：25、27
- **事实底线**：`update.c:330-370`（rollback_service）、`707-743`（abort）、`744-863`（五态辅助）、`865-931`（end_update）、`932-1011`（end_srv_update）、`164-184`（upd_move）；`request.c:506-513`（do_init_ready update 分支）；`error.c:56`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-M18 | 五态辅助 | 机制 | `update.c:744-863` | 按阶段分派 | 存：16 §2.6 |
  | K-M19 | `end_srv_update` | 机制 | `update.c:932` | 新旧二选一 | 存：16 §2.6 |
  | K-M20 | `rollback_service` | 机制 | `update.c:330` | 回滚 | 存：16 §2.7 |
  | K-M21 | init ready 的 update 分支 | 机制 | `request.c:506-513` | 旧文档只标注 | 存：12 §2.3（标注） |
  | K-M22 | `rupdate_upd_move` | 机制 | `update.c:164` | 描述符搬家 | 存：16 §2.3 |
  | K-M23 | ARCH A-6/A-9 | 架构演进 | `live_update.rs` | 标志→枚举 | 存：16 §3 |
  | K-M24 | `lu_strerror` | 工具 | `error.c:56` | LU 错误翻译 | 存：99 |

- **验收标准**：读者能对"prepare 前 / prepare 后 / initializing / init done"四个阶段说出 `end_update` 各自走哪个辅助函数、做了什么；能说出 `abort_update_proc` 的两种调用来源（25 的崩溃路径、19/21 的 `UPD_STOP`）与不同 reason。

### 25-rs-terminate-restart

- **一句话定位**：讲清服务出事之后的终局决策树：清理、复活、退避，三选一。
- **讲什么**：K-N01~K-N09
- **不讲什么**：
  - 清理原语本身（→ 16）
  - 谁触发终止事件（→ 26）
- **前置**：13、14、15、16、17、24
- **后置**：26
- **事实底线**：`manager.c:1055-1180`（terminate_service）、`1033-1054`（reincarnate_service）、`1246-1298`（restart_service）；`const.h:25,50-51`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-N01 | 三分支状态机 | 机制 | `manager.c:1055-1180` | 终局决策 | 存：15 §2.1 |
  | K-N02 | 初始化失败分支 | 机制 | `manager.c:1069-1093` | 三种后果 | 存：15 §2.2 |
  | K-N03 | update 期 abort | 约束 | `manager.c:1099-1102` | 崩溃打断更新 | 存：15 §2.2 |
  | K-N04 | `SF_NORESTART` 收尾 | 约束 | `manager.c:1105-1117` | detach/script | 存：15 §2.3 |
  | K-N05 | core 死亡 `_exit(1)` | 约束 | `manager.c:1120-1124` | 系统级放弃 | 存：15 §2.3 |
  | K-N06 | `reincarnate_service` | 机制 | `manager.c:1033` | 换 endpoint 重生 | 存：15 §2.8 |
  | K-N07 | `restart_service` | 机制 | `manager.c:1246` | 两条复活路径 | 存：15 §2.3 |
  | K-N08 | 退避计算 | 机制 | `manager.c:1164-1175` | 指数退避 | 存：07/15 |
  | K-N09 | `MAX_DET_RESTART` | 约束 | `manager.c:1290-1293` | 次数闸门 | 存：15 §2.3 |

- **验收标准**：读者能对任意终止现场（初始化中/更新中/正常运行/`SF_NORESTART`/core 服务/第 n 次崩溃）判定 RS 走哪条分支、最终调用哪个函数；能手工计算第 0/1/2/5 次崩溃后的 `r_backoff` 值并说明 `SF_USE_COPY` 为什么把退避压到 1。

### 26-rs-monitor

- **一句话定位**：讲清 RS 发现服务出事的两条通道：周期轮询与信号事件。
- **讲什么**：K-N10~K-N18
- **不讲什么**：
  - 出事之后的决策（→ 25）
  - 主循环的分类骨架（→ 09）
- **前置**：09、16、17、22、25
- **后置**：无（末端）
- **事实底线**：`request.c:943-1046`（do_period）、`1051-1090`（do_sigchld）；`main.c:631-642`（signal_handler）、`647-704`（signal_manager）、`85-91`（心跳接收）；`const.h:48-49,116`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-N10 | `do_period` 全序列 | 机制 | `request.c:943` | 周期检查 | 存：07 §2.1 |
  | K-N11 | 心跳协议 | 机制 | `request.c:1035-1038` | 一问一答 | 存：07 §2.2 |
  | K-N12 | 超时 → crash | 机制 | `request.c:1004-1029` | 判定与后果 | 存：07 §2.2 |
  | K-N13 | free pass | 约束 | `request.c:1013-1023` | 初始化期豁免 | 存：07 §2.2 |
  | K-N14 | SIGTERM 超时 | 机制 | `request.c:985-989` | 优雅退出兜底 | 存：07 §2.3 |
  | K-N15 | `do_sigchld` | 机制 | `request.c:1051` | 别人管的进程死了 | 存：07 §2.5 |
  | K-N16 | `sef_cb_signal_manager` | 机制 | `main.c:647-704` | 系统信号中转 | 存：06 §2.8 |
  | K-N17 | `SIGS_SIGNAL_RECEIVED` | 协议 | `main.c:698-702` | 信号→消息 | 存：06 §2.8 |
  | K-N18 | 三个时间常量 | 数据结构 | `const.h:48-49,116` | 节奏来源 | 存：07/99 |

- **验收标准**：读者能画出"一次心跳超时如何最终导致服务被杀"的完整链路（含每个函数的行号）；能说出为什么 `do_period` 里必须先看 `RUPDATE_IS_UPDATING() && !RUPDATE_IS_INITIALIZING()`；能解释信号通道里"VM 免转发"与"非终止信号转成消息"两条规则。

### 27-rs-self-lifecycle

- **一句话定位**：讲清 RS 自己怎么更新自己——这是唯一一个"被管理者管理自己"的路径。
- **讲什么**：boot 期自升级块（`main.c:436-491`）、`sef_cb_init_restart`（`main.c:499-544`）、`sef_cb_init_lu`（`549-586`）、RS 备份信号管理器、RS rollback 特例
- **不讲什么**：
  - `update_service`/`clone_slot` 的实现（→ 13）
  - SEF 回调模型（→ 07）
- **前置**：01、07、08、13、16、24
- **后置**：无（末端）
- **事实底线**：`main.c:436-491`（自升级）、`499-544`（init_restart）、`549-586`（init_lu）、`591-626`（两个 response）；`update.c:330-370`（rollback 的 RS 特例）；`utility.c:387-422`（update_sig_mgrs）
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-E01x | boot 期自升级块 | 机制 | `main.c:436-491` | 旧文档在 01，依赖未就绪 | 存：01 §2.6 |
  | K-N19 | `sef_cb_init_restart` | 机制 | `main.c:499-544` | RS 重启路径 | 存：18 §2.1 |
  | K-N20 | `sef_cb_init_lu` | 机制 | `main.c:549-586` | RS 更新路径 | 存：18 §2.2 |
  | K-N21 | RS 备份信号管理器 | 机制 | `utility.c:387-422` | 交接期不断信号 | 存：03/18 |
  | K-N22 | RS rollback 特例 | 机制 | `update.c:330-370` | `sys_whoami` + `SF_VM_ROLLBACK` | 存：18 §2.4 |

- **验收标准**：读者能解释 `srv_fork` 之后 `pid == 0` 与 `pid != 0` 两个分支各自做什么、为什么父分支最后必须 `SYS_PRIV_YIELD` 且标注 `NOT_REACHABLE`；能说出 `sef_cb_init_restart` 与 `sef_cb_init_lu` 的差别（前者要 `end_update`、后者有四条断言）。

### 99-rs-constants-index

- **一句话定位**：给实现者一个跨篇查找表，指向每个常量的权威定义位置，本身不定义任何常量。
- **讲什么**：K-O01、K-O02、K-O03、K-O04、K-O05
- **不讲什么**：常量的**定义**（→ 各机制篇，本篇只给指针）
- **前置**：00
- **后置**：无
- **事实底线**：`com.h:61,463-492,485-492`；`const.h:28-51,83`；`rs.h:33-52,191-206`；`sef.h:93-103,213-216`；`ipc_filter.h:10-21`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 源 |
  |------|------|------|------|------------|----|
  | K-O01 | `RS_*` 15 类型索引 | 数据结构 | `com.h:463-492` | 查表入口 | 存：99 §2.1 |
  | K-O02 | 子功能号索引 | 数据结构 | `com.h:485-492` | 查表入口 | 存：99 §2.1 |
  | K-O03 | endpoint 常量 | 数据结构 | `com.h:61` | 查表入口 | 存：99 §2.4 |
  | K-O04 | 操作码索引 | 数据结构 | `com.h`/`syslib.h` | 查表入口 | 存：99 §2.4 |
  | K-O05 | 单一权威位置原则 | 约束 | 99 §1.1 | 防漂移 | 存：99 §1.1 |

- **验收标准**：对池内任一常量，读者能在本篇查到"定义在哪个新篇章的哪一节、C 锚点在哪一行"，且本篇不出现任何一处与机制篇重复的定义性叙述。

---

## 6. 变更表

| 操作 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 / 来源 |
|------|------|--------|--------|------|-----------|------------|
| C-01 | 重排 | `19-rs-external-interfaces.md`（226 行，代码注释引用 23 次，最热） | `01-rs-external-face.md` | 它是全 stage 的依赖面，放在 19 使 01~18 全部前向引用它 | K-B01~K-B17、K-O06 | 全部原样迁移 + 补邻接服务面（来自 10/11/19） |
| C-02 | 拆分 | `01-rs-boot-init.md`（1044 行）§2.1 表部分 | `06-rs-boot-tables.md` | `table.c` 是孤立文件，与"启动骨架"不同语义 | K-E01~K-E06、K-E20 | 01 §2.1 → 06 §2/§3；01 §3 → 06 §5 |
| C-03 | 拆分 | `01-rs-boot-init.md` §2.1 SEF 段 + §1.4 | `07-rs-sef-startup.md` | SEF 框架此前无专篇（G-6），且 17/26/27 都要引用 | K-E07、K-E08 | 01 §2.1 SEF 段 → 07 §2/§3；补 `sef.h` 三族标志（新） |
| C-04 | 重排 | `01-rs-boot-init.md` §2.6 自升级段 | `27-rs-self-lifecycle.md` | 该段依赖 13 的置换原语与 27 的 RS 回调（序差 X-3） | 新增 K-E01x | 01 §2.6 → 27 §2 |
| C-05 | 保留+瘦身 | `01-rs-boot-init.md` 剩余 | `08-rs-boot-init.md` | 只留四步 boot 与 init_service 发送侧 | K-E09~K-E19 | 01 §2.3-2.7 → 08 §2-§6 |
| C-06 | 拆分 | `12-rs-init-run.md` §2.1-2.2（`start_service`/`run_service`） | `15-rs-start-run.md` | 与"初始化协议"不同语义（O-2） | K-H20、K-H21 | 12 §2.1-2.2 → 15 §2/§3 |
| C-07 | 重排 | `13-rs-control-requests.md` 的 `stop_service` | `15-rs-start-run.md` §4 | 与 start/run 同属编排原语三件套 | K-H22 | 13 §2 → 15 §4 |
| C-08 | 拆分 | `12-rs-init-run.md` §2.3-2.7 | `17-rs-ready.md` | 只留接收侧与收尾 | K-J01~K-J07 | 12 §2.3-2.7 → 17 §2-§5；update 分支 → 24 §5 |
| C-09 | 拆分 | `15-rs-terminate-restart.md` §2.4-2.7（kill/crash/cleanup/detach/run_script/get_service_instances） | `16-rs-teardown.md` | 这些原语被 create/publish/run/ready 复用，不是"终止状态机"（O-3、G-8） | K-I01~K-I08 | 15 §2.4-2.7 → 16 §2-§7 |
| C-10 | 保留 | `15-rs-terminate-restart.md` §2.1-2.3、§2.8 | `25-rs-terminate-restart.md` | 只留终局决策树 | K-N01~K-N09 | 15 §2.1-2.3/2.8 → 25 §2-§6 |
| C-11 | 重排+合并 | `07-rs-period-heartbeat.md` + `06-rs-main-loop.md` §2.8（信号） | `26-rs-monitor.md` | 两条通道同答"RS 怎么发现服务出事"（O-6） | K-N10~K-N18 | 07 全部 → 26 §2-§5；06 §2.8 → 26 §6 |
| C-12 | 拆分 | `16-rs-live-update.md`（312 行，覆盖 1011 行 C） | `21`/`22`/`23`/`24` 四篇 | 单篇装四个阶段必然每阶段都讲不透 | K-M01~K-M24 | 16 §2.1 → 21 §2-§4；§2.4 → 22；§2.5 → 23；§2.6-2.7 → 24 |
| C-13 | 重排 | `17-rs-state-data.md` | `20-rs-state-data.md` | 移到 LU 组首位，因为 21 的 `do_update` 调用 `init_state_data` | K-L01~K-L07 | 原样迁移（序差 X-6） |
| C-14 | 重排 | `04-rs-access-control.md` 与 `05-rs-ipc-sendmask.md` 互换 | `05` / `04` | 发送掩码是权限的组成部分，应先讲；访问控制消费它 | K-D11~K-D13 ↔ K-D14~K-D16 | 各自原样迁移 |
| C-15 | 重排 | `02-rs-process-table.md` §2.7 的 `rprocupd`/`rupdate` 语义 | `21-rs-lu-request.md` §4 | 结构留 02，语义归 LU（O-4） | K-C13 | 02 §2.7 结构留 02 §2；语义 → 21 §4 |
| C-16 | 新建 | — | `10-rs-lifecycle.md` | 生命周期全景与状态位迁移图（G-4），消除 00/13/15 三处重复画路径图（R-8） | K-G01、K-G02 | 来源：00 §3.1 + 13/15 内部图 + `const.h:28-43`（部分新增） |
| C-17 | 降级 | `99-rs-global-concepts.md` | `99-rs-constants-index.md` | 常量定义散入机制篇，99 只做索引（O-5、R-2/R-3/R-7） | K-O01~K-O05 | 99 §2 各表 → 对应机制篇；99 保留指针表 |
| C-18 | 合并 | `06-rs-main-loop.md` §2.1-2.7 | `09-rs-main-loop.md` | 只留骨架/分类/回复协议/空闲期 | K-F01~K-F10 | 原样迁移 |
| C-19 | 保留 | `02` / `03` / `08` / `09` / `10` / `11` / `14` / `18` | `02` / `03` / `11` / `12` / `13` / `14` / `17` / `27` | 语义单元成立，只改编号 | 见各篇契约 | 原样迁移 |
| C-20 | 归档 | 21 篇旧文档 | `archive/`（B 相） | 内容已被吸收 | — | 不删除 |

---

## 7. 缺漏新篇（非 C 主题清单逐项落实）

| 主题 | 是否新立篇 | 落实 | 原料来源 | 验收 |
|------|-----------|------|---------|------|
| 链接与加载 | 否（并入） | 12 §2-§4 | `exec.c` 全文、`manager.c:1357-1459`、旧 09 §2 | 见 12 验收 |
| 镜像与内存布局 | 否（并入） | 11 §4、13 §3、21 §3 | `rs.h:120-121`、`const.h:83`、`manager.c:658-695` | 见各篇验收 |
| 汇编入口与陷阱进入 | 否（越界） | 03 §2 只讲 `s_trap_mask` 授权位 | `main.c:271` | 能说出 `SRV_T`/`USR_T` 二选一的依据 |
| 启动装配 | **是（部分）** | 06 / 07 / 08 三篇 | `table.c`、`main.c:136-494`、`sef.h` | 见三篇验收 |
| 构建与工具链 | 否（并入） | 00 §5 | `servers/rs/Makefile`（59 行）、`bsd.own.mk:1499`、`os/servers/rs/Cargo.toml`、`os/Cargo.toml:14` | 能列出 8 个 SRCS、2 个编译开关与 cargo feature 对应关系 |
| 跨模块接口与线格式 | **是** | 01 全篇 | `include/minix/ipc.h`、`rs.h:104-151`、`rs.h:88-101`、`com.h:724-744` | 见 01 验收 |
| 错误路径 | 否（分散） | 01 §8、17 §3、24 §6、99 §2 | `error.c:33,48,56` | 能说出 init/LU 两套错误表的翻译入口 |
| 关闭与退出 | 否（分散） | 18 §5、25 §4、26 §6 | `request.c:431-461`、`manager.c:1120-1124`、`main.c:638-640` | 能说出三条关机路径 |
| 并发与同步 | 否（并入） | 00 §4、09 §4、26 §2/§6 | `lib.rs:3-6`、`utility.c:332`、`main.c:647-704` | 能说出两个"伪并发"点 |
| 测试基建 | 否（并入） | 00 §7 + 各篇验收标准 | `testutil.rs`（693 行）、`os/qemu-tests/`（无 RS 条目） | 能说出当前验证手段与 qemu 端到端缺口 |
| SEF 框架 | **是** | 07 全篇 | `main.c:136-153`、`sef.h:53-103`、`lib/libsys/sef*.c`（9 文件） | 见 07 验收 |
| boot 静态表 | **是** | 06 全篇 | `table.c:15-50`、`type.h:7-26`、`main.c:709-779` | 见 06 验收 |
| 生命周期全景 | **是** | 10 全篇 | 00 §3.1、13/15 内部图、`const.h:28-43` | 见 10 验收 |
| 终结原语 | **是** | 16 全篇 | `manager.c:360-526,1185-1356` | 见 16 验收 |
| 初始化收尾 | **是** | 17 全篇 | `request.c:462-528`、`manager.c:328-355`、`main.c:591-626` | 见 17 验收 |

---

## 8. 锚点迁移与断链成本

### 8.1 编号映射表（旧 → 新）

| 旧 | 新 | 旧 | 新 |
|----|----|----|----|
| 00-rs-overview | 00-rs-overview | 10-rs-service-create | 13-rs-service-create |
| 01-rs-boot-init | 06/07/08/27 | 11-rs-publish | 14-rs-publish |
| 02-rs-process-table | 02-rs-process-table | 12-rs-init-run | 15/17（拆） |
| 03-rs-privilege | 03-rs-privilege | 13-rs-control-requests | 18-rs-control-requests |
| 04-rs-access-control | 05-rs-access-control | 14-rs-query-requests | 19-rs-query-requests |
| 05-rs-ipc-sendmask | 04-rs-ipc-sendmask | 15-rs-terminate-restart | 16/25（拆） |
| 06-rs-main-loop | 09-rs-main-loop | 16-rs-live-update | 21/22/23/24（拆） |
| 07-rs-period-heartbeat | 26-rs-monitor | 17-rs-state-data | 20-rs-state-data |
| 08-rs-slot-config | 11-rs-slot-config | 18-rs-self-lifecycle | 27-rs-self-lifecycle |
| 09-rs-exec | 12-rs-exec | 19-rs-external-interfaces | 01-rs-external-face |
| — | 10-rs-lifecycle（新） | 99-rs-global-concepts | 99-rs-constants-index |

### 8.2 锚点迁移表（节级，覆盖全部发生变化者）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 断链风险 |
|--------|--------|--------|---------|---------|
| 00 §1.1-1.3 | RS 身份/位置/两状态机 | 00 §2-§3 | 原样 | 低（编号未变） |
| 00 §2.0 主线图 | 四步 boot 缩略图 | 08 §1 | 改写（缩略图回指） | 中：00 §2.0 被 01 §1 引用 |
| 00 §3.0 生命周期图 | 十站全景图 | 10 §2 | 改写 | 中：被 13/15 引用 |
| 00 §4.0 导航表 | 21 篇映射 | 00 §4 | 改写（换成 29 篇） | 高：全目录入口 |
| 00 §5.4 ARCH 表 | A-1~A-14 | 00 §6 | 改写（重排落点） | 中 |
| 01 §2.1 boot 表 | 三张静态表 | 06 §2 | 原样 | 中：01 被代码注释引用 8 次 |
| 01 §2.1 SEF 段 | 回调注册 | 07 §2 | 原样 | 中 |
| 01 §2.2 四表查找 | `boot_image_info_lookup` | 06 §3 | 原样 | 低 |
| 01 §2.3-2.7 boot 步骤 | 四步 boot | 08 §2-§6 | 改写 | 高：01 是最大的一篇 |
| 01 §2.6 自升级 | USE_LIVEUPDATE 块 | 27 §2 | 原样 | 中 |
| 02 §2.7 rupdate 语义 | 更新描述符语义 | 21 §4 | 拆分 | 中 |
| 04 全篇 | 访问控制 | 05 全篇 | 原样（换号） | 低（仅 1 处代码引用） |
| 05 全篇 | 发送掩码 | 04 全篇 | 原样（换号） | 中：tools/ 引用 9 次 |
| 06 §2.8 信号处理 | 两个 signal 回调 | 26 §6 | 原样 | 中 |
| 12 §2.1-2.2 | start/run_service | 15 §2-§3 | 原样 | 高：12 被代码注释引用 12 次 |
| 12 §2.3 do_init_ready | 接收侧 | 17 §2 | 拆分 | 高 |
| 12 §2.5 end_srv_init | 收尾 | 17 §4 | 原样 | 中 |
| 13 §2 stop_service | 停止原语 | 15 §4 | 原样 | 低 |
| 15 §2.4-2.7 | kill/crash/cleanup/detach | 16 §2-§7 | 原样 | 中 |
| 16 §2.1 | do_update 解析 | 21 §2-§4 | 原样 | 高：16 被引用 13 次 |
| 16 §2.4 | prepare 阶段 | 22 §2-§5 | 原样 | 中 |
| 16 §2.5 | 切换阶段 | 23 §2-§4 | 原样 | 中 |
| 16 §2.6-2.7 | 收尾与回滚 | 24 §2-§7 | 原样 | 中 |
| 17 全篇 | 状态数据 | 20 全篇 | 原样（换号） | 中 |
| 18 全篇 | RS 自身 | 27 全篇 | 原样 | 中 |
| 19 全篇 | 外部接口 | 01 全篇 | 改写（扩邻接服务面） | **最高**：26 处引用 |
| 99 §2 各常量表 | 常量定义 | 各机制篇 | 拆分 | 中 |

### 8.3 引用迁移表

**统计方法**：
```bash
grep -rhoE '[0-9]{2}-rs-[a-z-]+\.md' os/servers os/libs os/kernel os/xtask os/tests | sort | uniq -c
grep -rhoE '[0-9]{2}-rs-[a-z-]+\.md' notes/ --exclude-dir=03-stage-rs | sort | uniq -c
grep -rhoE '[0-9]{2}-rs-[a-z-]+\.md' tools/ | sort | uniq -c
```

**每篇被引用总数（外部）**：

| 旧文档 | os 源码注释 | notes 其它 stage | tools | 合计 | 新目标 |
|--------|-----------|----------------|-------|------|--------|
| 19-rs-external-interfaces | 23 | 2 | 1 | **26** | 01-rs-external-face |
| 02-rs-process-table | 14 | 4 | 2 | **20** | 02（编号不变） |
| 03-rs-privilege | 14 | 1 | 0 | **15** | 03（编号不变） |
| 12-rs-init-run | 12 | 1 | 1 | **14** | 15（start/run）或 17（ready）——**需逐条判定** |
| 16-rs-live-update | 8 | 5 | 0 | **13** | 21/22/23/24——**需逐条判定** |
| 05-rs-ipc-sendmask | 3 | 0 | 9 | **12** | 04-rs-ipc-sendmask |
| 01-rs-boot-init | 8 | 3 | 0 | **11** | 06/07/08/27——**需逐条判定** |
| 06-rs-main-loop | 9 | 1 | 1 | **11** | 09-rs-main-loop |
| 08-rs-slot-config | 1 | 8 | 0 | **9** | 11-rs-slot-config |
| 18-rs-self-lifecycle | 8 | 0 | 1 | **9** | 27-rs-self-lifecycle |
| 07-rs-period-heartbeat | 6 | 2 | 0 | **8** | 26-rs-monitor |
| 99-rs-global-concepts | 7 | 1 | 0 | **8** | 99（改为索引，需复核每处是指定义还是索引） |
| 13-rs-control-requests | 6 | 0 | 0 | **6** | 18-rs-control-requests |
| 00-rs-overview | 0 | 5 | 0 | **5** | 00（编号不变） |
| 09-rs-exec | 5 | 0 | 0 | **5** | 12-rs-exec |
| 11-rs-publish | 4 | 1 | 0 | **5** | 14-rs-publish |
| 10-rs-service-create | 4 | 0 | 0 | **4** | 13-rs-service-create |
| 17-rs-state-data | 5 | 0 | 0 | **5** | 20-rs-state-data |
| 14-rs-query-requests | 3 | 0 | 0 | **3** | 19-rs-query-requests |
| 15-rs-terminate-restart | 1 | 1 | 0 | **2** | 16 或 25——**需逐条判定** |
| 04-rs-access-control | 1 | 0 | 0 | **1** | 05-rs-access-control |
| **合计** | **142** | **35** | **15** | **192** | |

**内部交叉引用**：03-stage-rs 目录内文档互引约 130 处（按 §8.2 的分节映射逐条迁移）。

### 8.4 断链成本摘要

- **受影响引用总数**：外部 192 处 + 内部约 130 处 ≈ **322 处**。
- **热点文件**（需逐条人工判定，机器无法批量）：
  1. `19-rs-external-interfaces.md`（26 处，其中 `os/servers/rs/src/` 23 处）→ 目标唯一（01），可批量替换。
  2. `12-rs-init-run.md`（14 处）→ 拆成 15/17 两篇，**必须逐条判定**（看引用的是 start_service/run_service 还是 do_init_ready/end_srv_init）。
  3. `16-rs-live-update.md`（13 处）→ 拆成 21/22/23/24 四篇，**必须逐条判定**。
  4. `01-rs-boot-init.md`（11 处）→ 拆成 06/07/08/27 四篇，**必须逐条判定**。
  5. `02-rs-process-table.md`（20 处，编号不变）→ 零迁移成本。
- **可批量替换**（目标唯一）：19→01、05→04、04→05、06→09、08→11、09→12、10→13、11→14、13→18、14→19、17→20、18→27、07→26，共 13 条规则，覆盖约 118 处。
- **需人工判定**：01、12、15、16 四篇的拆分目标，覆盖约 40 处。
- **建议的批量修改方式**：`sed -i` 按"最长匹配优先"顺序应用 13 条唯一映射规则，再对 01/12/15/16 的 40 处逐个 grep 上下文判定；最后跑一次全仓 `grep -rn 'rs-[a-z-]*\.md' os/ notes/ tools/` 确认零残留旧编号。

---

## 9. 序差表（教学序 vs 运行时序）

| # | 运行时序事实（锚点） | 教学序选择 | 理由 | 回指补偿位置 |
|---|------------------|-----------|------|------------|
| X-1 | SEF 回调在 `main.c:136-153` 注册，回调体分布在 `main.c:158/499/549/591/614/631/647` | 07 只讲注册与回调模型，回调体留在 08/17/26/27 | 注册顺序就是运行时序；回调**体**不是本篇的依赖 | 08/17/26/27 各篇开头声明"本篇是 07 注册的第 N 个回调的实现" |
| X-2 | 主循环 `main.c:57-131` 先于所有 handler 执行 | 09 先讲骨架与回复协议，handler 语义后置 | 分派表指针不承载依赖：理解"怎么分类"不需要先懂 handler 内部 | 17/18/19/21/22/26 开头声明"本篇是 09 分派表的第 N 类" |
| X-3 | `USE_LIVEUPDATE` 自升级块在 boot 末尾（`main.c:436-491`） | 移到 27 | 该块依赖 13 的置换原语与 27 的 RS 回调；且 minix-rs 中对应 feature **默认关闭**（ARCH A-11） | 08 §6 末尾标注"此处有一段自升级代码（`main.c:436-491`），开关为 `USE_LIVEUPDATE`，完整语义见 27 §2" |
| X-4 | `do_period`（`request.c:943`）先触发崩溃，再走 `terminate_service` | 25（终局）在 26（监控）之前 | 26 需要 25 的 `restart_service` 与 16 的 `crash_service`；反过来 25 不需要 26 | 26 §1 声明"本篇是 25 的触发器"，并列出三条触发边 |
| X-5 | 先初始化（ready）才谈清理 | 16（终结原语）在 17（ready）之前 | 17 的失败分支直接调 `crash_service`（`request.c:494`），16 是它的依赖 | 17 §3 声明"失败分支调用 16 §2 的崩溃注入" |
| X-6 | `do_update`（`request.c:534`）先到，再解析状态数据 | 20（状态数据）在 21（lu-request）之前 | 21 调用 `init_state_data`（`manager.c:174`），20 是它的依赖 | 20 §1 用一句说明它在哪个环节被构造与消费 |
| X-7 | `do_init_ready` 是一个函数、两个分支（`request.c:506-526`） | 非 update 分支在 17，update 分支在 24 | 两分支的依赖方向相反（非 update→16；update→21/23） | 17 §2 声明"update 分支见 24 §5"；24 §5 完整给出 |
| X-8 | `RS_UP`（`request.c:15`）是生命周期的**入口** | 18 在创建流水线（11~17）之后 | `do_up` 依赖 `init_slot`(11) 与 `start_service`(15) | 18 §1 声明"本篇是 10 全景图的入口站"，并回指 10 |

---

## 10. 验证与自检门

### 10.1 四种机械检查

1. **前向引用扫描**（逐篇扫"前置"字段）：00(无) → 01(00) → 02(00,01) → 03(00,01,02) → 04(…,03) → 05(…,03) → 06(…,02) → 07(…,02) → 08(02,03,04,06,07) → 09(02,03,08) → 10(02) → 11(02,03,04,05) → 12(11) → 13(11,12) → 14(13) → 15(08,13,14) → 16(09,15,13,14) → 17(08,09,15,16) → 18(05,11,13,14,15,16,17) → 19(05,13,14) → 20(02,05) → 21(11,13,16,20) → 22(21) → 23(13,15,17,21,22) → 24(16,17,21,22,23) → 25(13,14,15,16,17,24) → 26(09,16,17,22,25) → 27(01,07,08,13,16,24) → 99(00)。**全部指向更早编号，通过**。唯一例外是 §9 的 8 处"声明性指针"，均已在序差表登记且证明不承载依赖。
2. **依赖关系图无环**：按 §1.4 的调用边 + 各篇"前置"构图，已通过拓扑排序验证（编号即拓扑序）。原先存在的三处环（terminate↔restart、do_init_ready↔LU、monitor↔terminate）已分别通过"合篇"（25）、"按分支拆分"（17/24）、"触发器后置"（26）拆解。
3. **覆盖率**：知识点池 197 条，逐条在 §5 契约中出现于某篇的"讲什么"或知识点清单；无一条无去向；无删除项。**新增条目 8 条全部带 C/制品锚点**（K-A05、K-A08、K-B17、K-E19、K-G02 + §7 落实项）。
4. **断链成本统计**：见 §8.4，总量 322 处，热点 5 个文件已列出，批量规则 13 条 + 人工判定 40 处。

### 10.2 自检门

| 门 | 检查内容 | 结果 |
|----|---------|------|
| G1 | C 真序逐条可核对（随机抽十条核对锚点） | **通过**。抽样：`main.c:185`（grant）、`main.c:244-346`（Step 1）、`main.c:350-399`（Step 2）、`main.c:404-407`（Step 3）、`main.c:413-430`（Step 4）、`main.c:433`（alarm）、`request.c:943-1046`（do_period）、`manager.c:1055-1180`（terminate）、`manager.c:1246-1298`（restart）、`update.c:532-616`（start_update）——十处行号均已实测 |
| G2 | 知识点池完整（每个 C 文件、每个非 C 制品都有归属或明确排除） | **通过**。8 个 .c 全部有归属（main→07/08/09/27；manager→02/05/11/13/14/15/16/17/25；request→18/19/21/22/26；update→13/21/22/23/24/27；utility→03/04/09/26；exec→12；error→17/24；table→06）；5 个自有头文件、6 个外部头文件、4 类非 C 制品均在 §3.5 逐项回答 |
| G3 | 新目录前向引用为零 | **通过**（10.1 第 1 项），8 处声明性指针已登记并论证 |
| G4 | 依赖图无环 | **通过**，三处环已拆解（10.1 第 2 项） |
| G5 | 覆盖率 100%，新增条目有锚点，删除项单列 | **通过**。无删除项；新增 8 条均带锚点 |
| G6 | 拆分/合并写清存量去向，新建写清来源（抽查十处） | **通过**。抽查 C-02、C-03、C-06、C-08、C-09、C-12、C-13、C-15、C-16、C-17 十处，均已在 §6 给出 `旧 → 新` 与知识点编号 |
| G7 | 每篇契约七要素齐全 | **通过**。29 篇（含 99）均含：一句话定位 / 讲什么 / 不讲什么 / 前置 / 后置 / 事实底线 / 知识点清单 / 验收标准 |
| G8 | 锚点迁移表覆盖所有变化文档的每一节；引用迁移表覆盖文档与代码注释 | **部分通过**。节级迁移表覆盖全部 21 篇中发生变化的 20 篇（00 编号不变但内容改写，已在 §8.2 列出 5 节）；引用迁移表覆盖 os 源码注释（142）、notes（35）、tools（15）。**未完成项**：目录内 130 处互引未逐条列出（仅给出分节映射规则），B 相需按规则逐条执行 |
| G9 | 事实断言都有锚点（随机抽十条）；推测项已标注 | **通过**。抽样：`com.h:61,77`、`kernel/table.c:46-64`、`kernel/main.c:264-265`、`const.h:28-43`、`rs.h:191-203`、`manager.c:1055`、`manager.c:1246`、`update.c:532`、`utility.c:247`、`bsd.own.mk:1499`——十处均已 grep 实测。本报告无未标注的推测项 |

### 10.3 结论

**结论：完成（G8 一项标注为"部分完成"）**。

还缺的两件事，均属 B 相执行细节，不影响蓝图可用性：

1. §8.3 中 01/12/15/16 四篇拆分的 40 处引用需逐条判定新目标（机器无法批量，必须看上下文）。
2. 目录内 130 处互引需按 §8.2 的分节映射逐条执行。

### 10.4 待用户裁决的问题

| # | 问题 | 我的倾向 | 影响 |
|---|------|---------|------|
| Q-1 | 外部依赖面（旧 19）是**整篇前移到 01**，还是**拆成"内核面 01 + 邻接服务面 12"**？ | 整篇前移。理由：邻接服务面的四个动词（`vm_*`/`ds_*`/DEVMAN/mapdriver）在 boot 之后、创建之前集中出现，拆开会让 13/14/15 三篇各引用一半，不如一次讲完 | 影响 01 的篇幅（预估 450-550 行）与 C-01 的迁移量 |
| Q-2 | 终结原语（旧 15 的 kill/crash/cleanup/detach/run_script）**独立成篇（16）**还是**留在 25 内部**？ | 独立成篇。理由：它有 6 个调用方分布在 create/publish/run/ready/terminate/LU，留在 25 会造成 12/13/14/17/24 五篇前向引用 25 | 影响 X-5 的序差是否需要 |
| Q-3 | 监控（旧 07）与信号通道（旧 06 §2.8）**合并成 26**还是**分成两篇**？ | 合并。理由：两者同答"RS 怎么发现服务出事"，且分开后信号篇会因调用 `terminate_service` 而被迫排到 25 之后 | 影响 26 的篇幅（预估 400 行） |
| Q-4 | 99 是**保留为常量定义篇**还是**降级为索引**？ | 降级为索引。理由：R-2/R-3/R-7 三处重复都源于 99 与各机制篇都定义同一批常量 | 影响 8 处代码注释引用（引用 99 的地方需改指机制篇） |
| Q-5 | `request.c:494` 的 `crash_service` 归属**旧文档写错**（写在 do_shutdown），是否授权 B 相以 C 行号为准纠正？ | 是。已在 §1.4 与 18 的验收标准中写明 | 影响 13-rs-control-requests 的旧内容正确性 |

---

## 11. 范围外发现

1. **`os/servers/rs/src/` 有两个异常大的模块**：`boot.rs`（2338 行）、`lib.rs`（2253 行）、`shell_request.rs`（1786 行）、`service_create.rs`（1917 行）、`live_update.rs`（2432 行）——Rust 侧总 22458 行，是 C 侧（6808 行）的 3.3 倍。其中 `shell_request.rs`/`shell_update.rs`/`trap_api.rs`/`testutil.rs` 在 C 侧没有对应物，是 minix-rs 自造的中间层。**建议**：B 相写作时若发现某篇契约的"Rust 侧"无法与这些模块对应，应在该篇显式标注"Rust 侧存在 C 没有的 shell 层，本 stage 文档不覆盖其设计"（记为其它 stage 或 20-redesign 的议题），不要把它的实现细节塞进机制篇。
2. **`os/servers/rs/src/lib.rs` 的模块 doc 声称"当前实现只覆盖 01-rs-boot-init 的 boot/init 骨架"**，但同目录已有 `live_update.rs`（2432 行）、`recovery.rs`（1098 行）等 28 个模块。该 doc 注释**已过期**（与实际代码不符）。建议在 B 相第一篇落地时同步修正，或在 00 §7（测试与实施现状）中用实测数据纠正。
3. **`tools/check-rs-unwired.sh:79`** 硬编码了 4 个 RS 文档编号（`06`/`12`/`18`/`19`），重建后必须同步更新，否则脚本会指向不存在的目标。
4. **`os/servers/sched/src/client.rs:6` 引用 `14-rs-interaction.md`**、**`os/servers/rs/src/trap_api.rs:373` 引用 `10-rs-service-convert.md`**——这两个文件名在本目录不存在（是其它 stage 或历史遗留的编号）。重建时不要把它们误当作本目录文档的引用计入迁移。
