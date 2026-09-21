# 03-stage-rs 文档重建蓝图(glm)

## 0. 元数据

- **执行者**: glm
- **日期**: 2026-09-19
- **目标目录**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/`
- **仓库根目录**: `/home/xzhao/github/minix-rs`
- **当前提交号**: `124d52c48506bb0172665d9de6fea328189972f8`(docs(edge3): live_update 两效果勾账)
- **任务**: R 相·重建蓝图。只产出本文件,不改任何正文。B 相按本蓝图逐篇重建,旧文档归档不删,锚点全部迁移。

### 0.1 审查范围

**范围内(正式文档,21 篇)**: `00-rs-overview.md`(349 行)、`01-rs-boot-init.md`(1044)、`02-rs-process-table.md`(904)、`03-rs-privilege.md`(706)、`04-rs-access-control.md`(396)、`05-rs-ipc-sendmask.md`(416)、`06-rs-main-loop.md`(407)、`07-rs-period-heartbeat.md`(351)、`08-rs-slot-config.md`(487)、`09-rs-exec.md`(310)、`10-rs-service-create.md`(307)、`11-rs-publish.md`(172)、`12-rs-init-run.md`(291)、`13-rs-control-requests.md`(291)、`14-rs-query-requests.md`(243)、`15-rs-terminate-restart.md`(268)、`16-rs-live-update.md`(312)、`17-rs-state-data.md`(252)、`18-rs-self-lifecycle.md`(252)、`19-rs-external-interfaces.md`(226)、`99-rs-global-concepts.md`(239)。合计约 9100 行。

**参考材料(读但不作为重建对象)**: `plan.md`(369 行,含 §5 覆盖契约与 §7 回归 review 记录)、`todo.md`(580 行,§0-§22 修复史)、`draft/README.md`(占位素材)。

**范围外**: `.design/`(项目规范禁止引用);其它 AI 的 `doc_rerank_*` 产物(未读,包括 `01-stage-kernel/doc_rerank_deepseek.md`——仅在 grep 文件名清单中出现,未打开);`edge_todo_archive.md`、`AI-chats/` 等过程材料。

### 0.2 读取清单

| 类别 | 内容 |
|------|------|
| 全部文档头部声明 | 21 篇的"本章不讲什么"边界块逐一读取(见 §2 现有位置列) |
| C 源码(直读全文) | `minix3/minix/servers/rs/main.c`(834 行,全文)、`table.c`(50,全文)、`type.h`(112,全文)、`const.h`(123,全文)、`utility.c` 关键段(1-141、223-484)、`request.c` do_period/do_sigchld 段(943-1094)、`update.c` update_service 段(262-330) |
| C 源码(函数面) | `manager.c`(2332)、`request.c`(1309)、`update.c`(1011)、`error.c`(59)、`exec.c`(165)逐函数 grep 取行号 |
| 头文件 | `glo.h`、`proto.h`、`inc.h`(函数面核对);`include/minix/{rs.h 210 行, sef.h 387, ipc_filter.h 30, com.h RS 段 463-492}` |
| 非 C 制品 | `minix/servers/rs/Makefile`;`minix/lib/libsys/sef*.c` 9 文件(约 2210 行,函数面+关键行);`etc/system.conf`(RS 段);`etc/rc.minix:49`;`minix/commands/minix-service/`(2248 行 C,含 `system.conf.5`);`usr.sbin/service/`;`minix/servers/is/dmp_rs.c`、`minix/fs/procfs/service.c`(归属判定用) |
| 阶段边界材料 | `00-master-plan/README.md`(全文)、本目录 `plan.md`/`todo.md`(全文)、`edge_todo.md`(E9/E5(c)/E-MINSYS-HYGIENE 条目) |
| 上一 stage | `02-stage-vm/00-vm-overview.md`(骨架+RS 关联段) |
| Rust 实现 | `os/servers/rs/src/` 29 模块(约 22348 行):`lib.rs`/`main.rs` 头部+模块表全读,其余按文档域映射抽查 |
| 写法范例 | `01-stage-kernel/06-todo.md`(只学"新文档契约"写法:讲什么/不讲什么/下放给谁/验收) |

### 0.3 使用的命令与关键输出(证据摘录)

```text
$ wc -l minix3/minix/servers/rs/*.c *.h
  → 6808 行(8 .c = 6307 + 6 头 = 501);与 plan.md §7.2 R-1 修正后的 6307 一致
$ wc -l os/servers/rs/src/*.rs | tail -1
  → 22348 行,29 个模块
$ grep -cE '#define RS_(UP|DOWN|REFRESH|RESTART|SHUTDOWN|UPDATE|CLONE|UNCLONE|EDIT|SYSCTL|FI|GETSYSINFO|LOOKUP|INIT|LU_PREPARE)[[:space:]]' minix3/minix/include/minix/com.h
  → 15(与 plan §7.2 R-4 精确口径一致;RS_RQ_BASE=0x700 在 com.h:463)
$ grep -ohE '[0-9]{2}-rs-[a-z-]+\.md' 0*.md 1*.md 99*.md | sort | uniq -c | sort -rn
  → 文档间交叉引用约 386 处;热点:16 号(38)、02 号(32)、08 号(27)、19 号(25)
$ grep -rn "(0[0-9]|1[0-9]|99)-rs-[a-z-]+\.md" os/servers/rs/src/*.rs | 统计
  → 代码注释引用约 138 处;热点:19 号(22)、03 号(14)、02 号(14)、12 号(12)
$ grep -rn "10-rs-service-convert" os/servers/rs/src/trap_api.rs
  → trap_api.rs:364 引用了不存在的 10-rs-service-convert.md(既有断链,见 §8.3)
$ grep -n "^## " 17-rs-state-data.md
  → "## 3. Rust 设计决策" 出现两次(约 127 行与 138 行,标题重复缺陷)
$ git log -1 --format=%H
  → 124d52c48…
```

---

## 1. C 真序

### 1.1 阶段类型判定

**服务事件循环型**(提示词第九部分第二类)。判定依据:RS 是由内核直接启动、永不退出的用户态服务器(`main.c:57` `while (TRUE)`),其全部工作由主循环消息驱动;但与 PM/VFS 等纯请求应答服务不同,RS 额外拥有一段**严格线性的 boot 序列**(`sef_cb_init_fresh` 四步,`main.c:158-494`),且运行期**周期性地主动发起**检查(ping/恢复/LU 推进),不是纯被动应答。因此真序分三段重建:**启动段**(线性)、**循环段**(事件驱动骨架)、**请求处理段**(并行体,按消息族分组)。

### 1.2 启动段真序表(从 main() 到进入主循环)

锚点全部为本次直读;行号以当前提交 `124d52c48` 的工作树为准。

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| B-01 | `main()` 进入,调 `sef_local_startup()` | `servers/rs/main.c:38,51`(定义 136-153) | 注册 7 个 SEF 回调(init_fresh/restart/lu、init_response、lu_response、signal_handler、signal_manager),`sef_startup()`(`lib/libsys/sef.c:68`)阻塞等 init 消息并回调 `sef_cb_init_fresh` |
| B-02 | `sys_getmachine(&machine)` | `main.c:53` | 取机器信息,失败 panic |
| B-03 | `sef_cb_init_fresh` 预备段:env_parse(rs_verbose) | `main.c:179` | 冗余开关 |
| B-04 | `sys_getinfo(GET_HZ)` → `system_hz` | `main.c:181` | 系统频率,失败 panic |
| B-05 | 创建 `rinit.rproctab_gid` grant | `main.c:185-189` | `cpf_grant_direct(ANY, rprocpub, CPF_READ)`,失败 panic |
| B-06 | `RUPDATE_INIT()` + `shutting_down=FALSE` | `main.c:192-193` | LU 全局复位 |
| B-07 | `sys_getimage(image)` 拷 boot 映像表 | `main.c:196-198` | 内核 boot_image[] 运行期拷贝 |
| B-08 | 统计系统服务数,与 priv 表计数一致性校验 | `main.c:201-227` | 不一致 panic(`main.c:225-227`) |
| B-09 | 复位 `rproc[]` 全表(标志/初始错误/公共槽指针) | `main.c:230-237` | `r_init_err=ERESTART` 初值在 `main.c:232` |
| B-10 | **Step 1** 逐 boot 服务建全表属性 | `main.c:239-346`(循环 244 起) | 迭代次序 = `servers/rs/table.c:15-30` boot_image_priv_table:**rs → vm → pm → sched → vfs → ds → tty → memory → mib → pfs → mfs → init**(注意:与内核 `kernel/table.c:44-64` 的登记/NOTIFY 优先序不同,后者是 ds 第一、rs 第二;两表语义不同,见 §1.6 序差 D-1) |
| B-10a | └ 设 label、`static_priv_id`、s_flags/s_init_flags/s_trap_mask | `main.c:262-271` | `static_priv_id` 调用点 `main.c:265`;定义不在 `servers/rs/*.c`(待验证:应在 syslib/kernel 头,`grep -rn static_priv_id minix3/minix` 复核) |
| B-10b | └ `fill_send_mask` + `s_sig_mgr`/`s_bak_sig_mgr` | `main.c:272-275`(原语 `utility.c:82-98`) | 全置/全清原语 |
| B-10c | └ `fill_call_mask` 内核调用掩码 | `main.c:278-280`(原语 `utility.c:100-141`) | ALL_C 全置 / 逐位 |
| B-10d | └ **RS/VM 例外**:跳过 `sys_privctl(SET_SYS)` | `main.c:285-291` | 二者已在运行 |
| B-10e | └ `sys_getpriv` 同步回本地 | `main.c:294-296` | 内核可能改写 |
| B-10f | └ sys_flags / dev_nr / r_cmd / build_cmd_dep / proc_name | `main.c:301-313`(build_cmd_dep 定义 `manager.c:289-324`) | |
| B-10g | └ vm_call_mask / 调度参数 / endpoint / 默认值 / 置 IN_USE\|ACTIVE + rproc_ptr | `main.c:315-345` | 计时器初值 `main.c:333-338` |
| B-11 | **Step 2** 逐服务允许运行 | `main.c:348-399`(循环 350 起) | 同一次序迭代 |
| B-11a | └ RS/VM 例外:直接 `init_service(SEF_INIT_FRESH)` | `main.c:362-373` | RS 免发消息(`utility.c:29-31` ROOT_SYS_PROC 早退);VM 仍会回 RS_INIT → `nr_uncaught_init_srvs++`(`main.c:368-371`) |
| B-11b | └ 其余:`sched_init_proc` + `sys_privctl(ALLOW)` | `main.c:376-381`(`sched_init_proc` 定义 `utility.c:364-385`) | |
| B-11c | └ SYS_PROC 者 `init_service`;`SF_SYNCH_BOOT` 当场收 ready,否则累积 | `main.c:386-398` | 同步/异步 boot 分叉在 `main.c:390-397` |
| B-12 | **Step 3** 收齐剩余 init ready | `main.c:401-407`(`catch_boot_init_ready` 定义 `main.c:784-821`) | while 循环逐个阻塞接收 |
| B-13 | **Step 4** `getnpid` 补 pid | `main.c:409-430` | 失败 panic(`main.c:427-429`) |
| B-14 | `sys_setalarm(RS_DELTA_T)` 挂周期闹钟 | `main.c:433-434` | 主循环心跳节拍来源 |
| B-15 | **USE_LIVEUPDATE boot 自升级** | `main.c:436-491` | `clone_slot`(441)→`srv_fork`(446)→`getprocnr`(451)→pid==0 分支:`update_service(RS_SWAP)`(460)+`cpf_reload`(464)+`cleanup_service`(467)+`vm_memctl(PIN)`(470);父分支:`SET_SYS`(478)+`sched_init_proc`(482)+`YIELD`(485)+NOT_REACHABLE(489) |
| B-16 | 返回 `main()`,进入主循环 | `main.c:57` | |

### 1.3 循环段真序表(主循环骨架)

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| L-01 | `rs_idle_period()` 空闲期后台任务 | `main.c:59`(定义 `utility.c:443-484`) | `rs_is_idle`(`utility.c:424-441`,`RS_SRV_IS_IDLE` 宏 `const.h:47`)不过则返回;shutting_down 覆盖空闲判定(`utility.c:449-452`);清 `RS_DEAD`(`utility.c:455-462`);为 `SF_USE_REPL` 且缺副本的活跃服务 `clone_service` 补副本,VM 单副本限制(`utility.c:465-479`) |
| L-02 | `get_work()` = `sef_receive_status(ANY)` | `main.c:62`(定义 826-833) | 阻塞收 |
| L-03 | `rs_isokendpt` 校验来源 | `main.c:64-66`(定义 `utility.c:352-362`) | 失败 panic |
| L-04 | notify 分支:CLOCK → `do_period` | `main.c:80-84` | 周期检查入口(§1.4) |
| L-05 | notify 分支:其他 → 心跳时间戳 | `main.c:85-91` | `rproc_ptr[who_p]->r_alive_tm = timestamp`;未知来源告警 |
| L-06 | request 分派:15 个消息号 | `main.c:100-122` | 控制面 RS_UP/DOWN/REFRESH/RESTART/SHUTDOWN/UPDATE/CLONE/UNCLONE/EDIT(102-110)、观测面 SYSCTL/FI/GETSYSINFO/LOOKUP(111-114)、ready 面 RS_INIT/RS_LU_PREPARE(116-117);default → ENOSYS(118-121) |
| L-07 | 回复(除非 EDONTREPLY) | `main.c:124-129`(`reply` 定义 `utility.c:309-330`) | RS 自身免回(`utility.c:315-317`);`late_reply`(`utility.c:332-350`)补 RS_LATEREPLY 延迟回复 |

### 1.4 请求处理段真序(并行体,按消息族)

**族 1:控制请求(RS_UP 为代表,完整链)**

| 步 | 动作 | C 锚点 |
|----|------|--------|
| U-01 | `do_up` 拷参数/查重/init_slot | `request.c:15-110`(`copy_rs_start` `manager.c:135-150`;`init_slot` `manager.c:1710 起`;`edit_slot` `manager.c:1460-1705`) |
| U-02 | `check_call_permission` 访问控制 | `manager.c:81-133`(`caller_is_root` 21-38、`caller_can_control` 39-79) |
| U-03 | `start_service` 编排:create→activate→publish→run | `manager.c:950-987` |
| U-04 | └ `create_service`:闸门→srv_fork→getprocnr→表登记→priv(SET_SYS+getpriv)→sched→read_exec→srv_execve→RS pin→VM MAKE_VM+全实例 pin→vm_set_priv | `manager.c:531-712`(闸门 547/555/563;fork 576;登记 589-596;priv 600-605;sched 609;exec 625-650;VM 656-707) |
| U-05 | └ `activate_service` 置活跃 | `manager.c:1013-1032` |
| U-06 | └ `publish_service`:DS→mapdriver→PCI→devman | `manager.c:787-858`(DS 800;mapdriver 806-824;PCI 828-835;devman 840-853) |
| U-07 | └ `run_service`:privctl(ALLOW)+`init_service` | `manager.c:923-949`(`init_service` 定义 `utility.c:18-67`:置 RS_INITIALIZING、构造 RS_INIT 消息含 rproctab_gid/old_endpoint/restarts/prealloc/prepare_state、`rs_asynsend`(`utility.c:223-243`)) |
| U-08 | 服务初始化完成回 `RS_INIT` → `do_init_ready` | `request.c:462-533` |
| U-09 | └ 成功:reply + `end_srv_init`(late reply、prev replica 清理、`rupdate_upd_move`、r_restarts++) | `manager.c:328-356`(rupdate_upd_move 定义 `update.c:164-184`) |
| U-10 | └ 失败:`crash_service` → 终局链 | `manager.c:380-404`,终局见族 3 |

**族 2:观测请求**(RS_LOOKUP/RS_GETSYSINFO/RS_SYSCTL/RS_FI,`request.c:1095-1264`、`fi_service` `utility.c:69-81`)

**族 3:终止与恢复链**

| 步 | 动作 | C 锚点 |
|----|------|--------|
| T-01 | 触发源三路:信号管理器终止信号(`main.c:686-692`)、心跳超时 crash(`request.c:1007-1030`)、控制请求(RS_DOWN/REFRESH) | |
| T-02 | `terminate_service` 决策树 | `manager.c:1055-1184`(初始化失败 rollback 分支、SF_NO_BIN_EXP refresh 分支、exiting 分支、SF_NORESTART、core 服务 `_exit(1)`、reincarnate) |
| T-03 | └ 恢复执行:`restart_service`(clone+update+run 或脚本) | `manager.c:1246-1302`;`run_script`(fork+execle sh)`manager.c:1185-1245` |
| T-04 | └ 清理执行:`cleanup_service` 两段式(标记 DEAD→真清理) | `manager.c:405-496`(sched_stop 462 附近、srv_kill 469 附近、脚本、detach、free_slot);`detach_service` `manager.c:497-530` |
| T-05 | └ RS_DEAD 统一回收在 `rs_idle_period` | `utility.c:455-462`(与 L-01 同点) |

**族 4:Live Update 链**

| 步 | 动作 | C 锚点 |
|----|------|--------|
| V-01 | `do_update` 校验+clone+init_slot+inherit+create+状态数据+入链 | `request.c:534-889`(RSS_* LU 标志解析、EBUSY/EINVAL 门;`init_state_data` `manager.c:174-288`;`rupdate_add_upd` `update.c:23-87`) |
| V-02 | `start_update_prepare(_next)` 推进 prepare | `update.c:401-531`(VM 多组件预分配;`request_prepare_update_service` `update.c:185-229`) |
| V-03 | 新实例 `RS_LU_PREPARE` → `do_upd_ready` 门与分支 | `request.c:890-942`(gate:curr 槽匹配+非 INITIALIZING) |
| V-04 | `start_update`:VM multi 时序、`rs_receive_ticks` 超时等待 | `update.c:532-620`(`rs_receive_ticks` `utility.c:247-307`:IPC filter+sys_setalarm2 双源接收) |
| V-05 | `update_service` 实例切换原语 | `update.c:262-327`(srv_update 仅 RS_SWAP 时;swap_slot;pid/endpoint 重排;getpriv×2;`activate_service`) |
| V-06 | 新实例 `RS_INIT(SEF_INIT_LU)` → `do_init_ready` update 分支 | `request.c:462-533` 分叉;`start_srv_update`/`complete_srv_update`(RS yield 特例)`update.c:621-706` |
| V-07 | `end_update` 五相位收尾 / `end_srv_update` 存活者选择 / `rollback_service` 回滚 | `update.c:744-931`、`932-1011`、`330-366`(RS 特例 `sys_whoami`+`vm_update(SF_VM_ROLLBACK)`) |
| V-08 | 周期推进:`do_period` 头部 LU 门 → `update_period` prepare 超时→end_update(EINTR, RS_CANCEL) | `request.c:950-954`、`update.c:371-397` |

**RS 自身生命周期**(贯穿):boot 自升级(B-15)、`sef_cb_init_restart`(`main.c:499-544`:STATEFUL 转移→进行中 update 则 `end_update(ERESTART)`→`update_service(RS_DONTSWAP)`→`init_service(SEF_INIT_RESTART)`→重挂闹钟)、`sef_cb_init_lu`(`main.c:549-586`:先重绑 STATEFUL(`main.c:558`;框架分派 `lib/libsys/sef_init.c:98,427`)→`SEF_CB_INIT_LU_DEFAULT`→`update_service(RS_DONTSWAP)`→四断言 580-583)、自模拟 response(`sef_cb_init_response` `main.c:591-609`、`sef_cb_lu_response` `main.c:614-626`)。

### 1.5 SEF 框架真序(新增主题,现有文档分散在 3 篇)

C 侧 SEF 是 `lib/libsys/` 的用户态框架(9 文件约 2210 行:`sef.c` 启动与接收、`sef_init.c` 483 行 init 流程、`sef_liveupdate.c` 566 行 LU 状态、`sef_signal.c` 202 行信号、`sef_st.c` 194 行状态转移、`sef_ping.c`、`sef_fi.c`、`sef_gcov.c`、`sef_llvm.c`)。关键锚点:`sef_startup`(`sef.c:68`,内部 `sys_whoami` `sef.c:81`)、`sef_receive_status`(`sef.c:150`,init 消息拦截与 EINTR 语义 `sef.c:295`)、LU_DEFAULT 分派(`sef_init.c:98`)、STATEFUL 转移(`sef_init.c:427`)。现有文档把"框架行为"(sef_startup 阻塞等 init、回调表、response 协议)与"RS 消费"(boot 四步、RS_INIT 消息)混在 01/12/18 三篇,见 §3.2 缺口表 N-1。

### 1.6 序差表(运行时序 vs 教学序)

| 编号 | 运行时序事实(锚点) | 教学序选择 | 理由 | 回指补偿 |
|------|--------------------|-----------|------|---------|
| D-1 | boot Step 1-4 迭代 `servers/rs/table.c:15-30`(rs 第一、vm 第二、ds 第六);内核登记/NOTIFY 优先序是 `kernel/table.c:44-64`(ds 第一、rs 第二) | 新 05 讲 boot 时以 priv 表迭代序为准;新 00 只讲内核登记序与"两层语义" | 两表常被混为一谈;迭代序决定初始化因果,登记序决定通知优先级 | 新 00 §1.2 显式对照两表;新 05 §2.3 声明"迭代序=priv 表序" |
| D-2 | `update_service` 在 `update.c:262`,物理上属 LU 文件 | 教学上归新 11(实例替换原语,由 swap_slot+activate_service 组成,经直读证实不含 LU 编排) | 恢复链(`restart_service` manager.c:1246)、自升级(B-15)、自重启(`main.c:526,573`)都先于 LU 编排概念使用它;消除三处前向引用 | 新 11 §收尾声明"LU 编排在新 18";新 18 引用回新 11 |
| D-3 | `fill_send_mask/fill_call_mask` 定义在 `utility.c:82-141`,boot(`main.c:273,279`)与运行时配置(`init_privs`)都用 | 原语归新 03(priv 结构的填充器);r_ipc_list 拓扑计算归新 08 | boot 在教学序上先于配置管线;原语级依赖必须后向 | 新 08 引用新 03 原语;新 05 引用新 03 |
| D-4 | `check_call_permission` 只在请求 handler 内调用(运行期),boot 不用它 | 新 14 紧贴控制/查询请求之前 | 运行期因果序:主循环(06)→请求面(14-16);消除旧 04 在主循环之前讲"请求到达"的倒置 | 新 14 前置含新 06;不需要额外补偿 |
| D-5 | 常量/标志位值(`const.h`/`rs.h`/`sef.h`/`com.h`)被所有篇章引用 | 词典篇放新 01(全文最前) | 机械消除"引用尚未出现的篇章"一类前向引用;词典是查阅件不是叙事件 | 各篇引用位值一律回指新 01;机制语义在各机制篇 |
| D-6 | `sef_cb_init_fresh` 由 SEF 框架在 `sef_startup` 内回调(`sef.c:68` 起),先于 B-03 | 新 04(SEF 框架)在新 05(boot)之前 | 读者需要先知道"谁调用了 sef_cb_init_fresh"才能理解 boot 的入口语义 | 新 05 开头回指新 04 |
| D-7 | LU 触点分散于主循环门(`request.c:950-954`)、ready 消息(`request.c:890`)、sysctl(`request.c:1181`)、恢复(`manager.c:1055` 起) | LU 编排集中在新 18(靠后);触点处只留单行"门/调用点"指针 | C 代码在 init_service↔LU 之间存在真实互相调用(`init_service` 发 LU prepare 状态;`do_upd_ready` 推进链),任何单一顺序都无法全消环;这是最小环拆解:机制(13/17)在前、编排(18)在后,触点指针成类 | 每处指针统一措辞"update 进行中的行为由新 18 裁决",列入 §9 G3 扫描白名单 |
| D-8 | `kill_service`(`manager.c:360`)被 publish 失败路径(`manager.c:813` 附近)调用,物理前于 recovery | 新 12(publish)只陈述"发布失败→kill_service"外部行为,机制在新 17 | 与旧 11 相同的既定豁免;避免把终局原语拆去配置簇 | 单函数指针,入 G3 白名单 |

---

## 2. 知识点全集

### 2.1 池总表

约定:编号 `K-###` stage 内唯一;**来源类型**=存量(来自现有文档,需给去向)/新增(现有文档没有,需给证据锚点);**去向**列给新篇章号(§4);重复条目在 §2.3 标主讲述点。锚点缩写:`main.c` 等指 `minix3/minix/servers/rs/`,协议头指 `minix3/minix/include/minix/`,旧 NN 指现有文档。

**A 簇:全局与词典**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-001 | RS 身份:root system process,RS_PROC_NR=2 | 概念 | 存量 | 旧00 §1.1 | com.h:61,77 | 新00 |
| K-002 | 登记顺序 vs 执行顺序两层语义(RTS_VMINHIBIT) | 概念 | 存量 | 旧00 §1.2 | kernel/table.c:44-64;01-stage-kernel/09 | 新00 |
| K-003 | RS 职责全景:加载/启动/监控/终止/恢复/更新 | 概念 | 存量 | 旧00 §1 | main.c:1-10 注释 | 新00 |
| K-004 | 服务生命周期次主线全景图 | 概念 | 存量 | 旧00 §3、旧13 § | plan §1.3 | 新00、新15(路径图) |
| K-005 | 文档导航/阅读路线/位置可回答性原则 | 工具 | 存量 | 旧00 §4-§6 | (自指) | 新00(重建,修 §2.3 陈旧行) |
| K-006 | ARCH 清单 A-1~A-14 概览 | 架构 | 存量 | 旧00 §5.4 | plan §4 | 新00(索引)+各篇正文 |
| K-007 | 覆盖契约概览(8 .c/15 消息/16 r_flags/13 SF/121 函数) | 工具 | 存量 | 旧00 §5.5 | plan §5,§7.2 | 新00 |
| K-010 | 15 个 RS IPC 消息类型(RS_RQ_BASE+0..24) | 接口 | 存量 | 旧99 §2.1 | com.h:463-492 | 新01 |
| K-011 | RS_SYSCTL_*/RS_FI_CRASH 子功能 | 接口 | 存量 | 旧99 §2.1、旧14 §2 | com.h:485-492 | 新01(位值)、新16(消费) |
| K-012 | r_flags 16 位全表(RS_IN_USE~RS_REINCARNATE) | 约束 | 存量 | 旧99 §2.2、旧02 §2 | const.h:26-45 | 新01(位值)、新02(生命周期角色) |
| K-013 | SF_* 13 位全表 | 约束 | 存量 | 旧99 §2.3、旧02 | rs.h | 新01(位值)、新02/新10(语义) |
| K-014 | RSS_* 请求标志全表 | 接口 | 存量 | 旧99、旧08/13/15 | rs.h | 新01(位值)、消费篇 |
| K-015 | SEF_INIT_*/SEF_LU_*/SEF_CB_* 标志 | 接口 | 存量 | 旧99 §2.4、旧12/16/18 | sef.h | 新01(位值)、新04(行为) |
| K-016 | IPCF_*/ANY_USR/SYS/TSK | 接口 | 存量 | 旧99、旧17 | ipc_filter.h(30 行) | 新01、新19 |
| K-017 | SYS_PRIV_*/SYS_STATE_* 操作码 | 接口 | 存量 | 旧99、旧03 | com.h;kernel/priv.h | 新01、新03 |
| K-018 | 时间常量 RS_DELTA_T/RS_INIT_T/MAX_BACKOFF/RS_DEFAULT_PREPARE_MAXTIME | 约束 | 存量 | 旧99 §2.2、旧07 | const.h:49-53,60 | 新01、新07/新18 |
| K-019 | endpoint 常量与 _ENDPOINT_P 语义 | 约束 | 存量 | 旧99 §2.4 | com.h:61;sys/types | 新01 |
| K-020 | 错误表 init_strerror/lu_strerror | 接口 | 存量 | 旧99 §2.5 | error.c(59 行) | 新01 |
| K-021 | RS_SRV_IS_IDLE/SRV_IS_UPDATING/RUPDATE_* 判定宏族 | 约束 | 存量 | 旧99、旧06/07/16 | const.h:47,105-121 | 新01(定义)、新06/新18(消费) |
| K-022 | 常量权威 Rust 位置决策(单点定义原则) | 架构 | 存量 | 旧99 §3 | service_slot.rs 等 | 新01 |

**B 簇:进程表与数据模型**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-030 | rproc 全字段(私有槽) | 数据 | 存量 | 旧02 §2 | type.h:58-112 | 新02 |
| K-031 | rprocpub 全字段(公开槽) | 数据 | 存量 | 旧02 §2 | rs.h | 新02 |
| K-032 | 双表设计(私有/公开,谁可见) | 数据 | 存量 | 旧02 §1.2 | glo.h:34 | 新02 |
| K-033 | rproc_ptr 快速索引 | 数据 | 存量 | 旧02 §2 | glo.h;main.c:86,344 | 新02 |
| K-034 | rinit 描述符与 rproctab_gid | 数据 | 存量 | 旧02(字段)、旧01(创建)、旧12(消费) | glo.h;main.c:185;utility.c:53 | 新02(结构)、新05(创建)、新13(消费) |
| K-035 | rprocupd/rupdate 描述符字段 | 数据 | 存量 | 旧02(字段)、旧16(状态机) | type.h:26-56 | 新02(结构)、新18(状态机) |
| K-036 | lookup_slot_by_label/pid/dev_nr/domain/flags | 机制 | 存量 | 旧02 §2 | manager.c:1933-2062 | 新02 |
| K-037 | alloc_slot/free_slot | 机制 | 存量 | 旧02 §2 | manager.c(§20 A 类映射) | 新02 |
| K-038 | rs_isokendpt | 机制 | 存量 | 旧02、旧06 | utility.c:352-362 | 新02 |
| K-039 | r_flags/sys_flags 的生命周期角色表 | 约束 | 存量 | 旧02 §2 | const.h:26-45;rs.h | 新02 |
| K-040 | boot_image_priv/sys/dev 三表结构 | 数据 | 存量 | 旧01 §2.7 | table.c:15-50;type.h:8-24 | 新05 |
| K-041 | Option\<SlotId\> 替代四指针链(A-3) | 架构 | 存量 | 旧02 §3 | process_table.rs | 新02 |
| K-042 | 快速索引 Option\<SlotId\>(A-4) | 架构 | 存量 | 旧02 §3 | process_table.rs | 新02 |
| K-043 | assert_consistent settled-state 不变式校验 | 测试 | 存量 | 旧02 §3/§5 | process_table.rs(Fix #67) | 新02 |
| K-044 | RS_BOOT 表 flags 位值单一权威(Fix #14 P0 教训) | 约束 | 存量 | 旧01 §3.1、旧05 | table.rs | 新05 |

**C 簇:权限**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-050 | priv 结构字段组(s_id/s_flags/s_init_flags/trap/ipc_to/k_call_mask/sig_mgr/bak_sig_mgr) | 数据 | 存量 | 旧03 §2 | kernel/priv.h:94;01-stage-kernel/22 | 新03 |
| K-051 | boot priv 构造序列(逐字段来源) | 机制 | 存量 | 旧03 §2、旧01 §2.3 | main.c:258-296 | 新05(时序)、新03(字段语义) |
| K-052 | static_priv_id 语义 | 机制 | 存量 | 旧03 §2 | 调用 main.c:265;定义待验证(§1.2 B-10a) | 新03 |
| K-053 | fill_send_mask/fill_call_mask 原语 | 机制 | 存量 | 旧05 §2(主)、旧03(引用) | utility.c:82-141 | **新03(主讲述点迁移)** |
| K-054 | sys_privctl 操作面(SET_SYS/ALLOW/UPDATE_SYS/YIELD/SET_USER/CLEAR_IPC_REFS) | 机制 | 存量 | 旧03 §2 | main.c:287,379,478,485;manager.c:1055 起 | 新03 |
| K-055 | sys_getpriv 同步语义 | 机制 | 存量 | 旧03 §2 | main.c:294;utility.c:395;update.c:291-297 | 新03 |
| K-056 | RS/VM boot 例外(跳 SET_SYS、直 init_service) | 约束 | 存量 | 旧03、旧01 | main.c:285-291,362-373 | 新05(事实)、新03(例外语义) |
| K-057 | sched_init_proc 与复合传输目标(KERNEL→sys_schedctl,否则 SCHED 消息) | 机制 | 存量 | 旧03 §2、旧01 §3.5 | utility.c:364-385;lib/libsys/sched_start.c | 新03(语义)、新21(传输面) |
| K-058 | update_sig_mgrs | 机制 | 存量 | 旧03 §2 | utility.c:387-412 | 新03 |
| K-059 | vm_call_mask 填充 | 机制 | 存量 | 旧03 §2 | main.c:315-317 | 新03 |
| K-060 | 信号管理器角色与 s_sig_mgr 语义(SELF/NONE/端点) | 概念 | 存量 | 旧03、旧06、旧18 | priv.h;main.c:647-704 | 新03(机制)、新06(转发效果) |

**D 簇:SEF 框架(新增为主)**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-070 | SEF 框架定位:用户态 init/restart/LU/信号骨架 | 概念 | **新增** | (散落,无专篇) | lib/libsys/sef*.c 9 文件约 2210 行 | 新04 |
| K-071 | sef_startup 流程:sys_whoami→等 init→回调→response | 机制 | **新增**(旧01 只讲注册) | 旧01 §2.2(注册半) | sef.c:68-150 | 新04 |
| K-072 | 回调注册表 7 项(sef_setcb_*) | 机制 | 存量 | 旧01 §2.2/§3.3 | main.c:136-153;sef.c | 新04 |
| K-073 | init 三类型 FRESH/RESTART/LU 的语义 | 概念 | 存量 | 旧01 §1.3、旧12、旧18 | sef.h;sef_init.c | 新04 |
| K-074 | SEF_CB_INIT_RESTART_STATEFUL/SEF_CB_INIT_LU_DEFAULT 通用体 | 机制 | 存量 | 旧18 §2.9b(转述) | sef_init.c:98,427(本次直读核实) | 新04 |
| K-075 | sef_receive_status:init 拦截与 EINTR 语义 | 机制 | **新增** | (RS 调用点在旧06) | sef.c:150,295 | 新04;RS 消费在旧06→新06 |
| K-076 | 信号拦截框架(handler vs manager 双回调) | 机制 | 存量 | 旧06 §2(效果体) | sef_signal.c(202 行);main.c:631-704 | 新04(框架)、新06(效果) |
| K-077 | SEF_LU 状态族与 prepare 协议 | 机制 | 存量 | 旧16、旧17 | sef_liveupdate.c(566 行) | 新04(框架)、新18/新19(编排) |
| K-078 | Rust SEF 抽象:SefCallbacks trait+RestartCb 枚举(A-7,Fix #18/#63) | 架构 | 存量 | 旧01 §3.3、旧18 §2.9b | sef.rs;lib.rs | 新04(表)、新20(重绑语义) |

**E 簇:boot**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-080 | main() 入口与主循环交接 | 机制 | 存量 | 旧01 §2.1 | main.c:38-57 | 新05(入口)、新06(循环) |
| K-081 | sef_cb_init_fresh 预备段(GET_HZ/grant/RUPDATE_INIT/getimage/计数/复位) | 机制 | 存量 | 旧01 §2.3.1-2 | main.c:178-237 | 新05 |
| K-082 | Step 1 全表属性(序列+RS/VM 例外) | 机制 | 存量 | 旧01 §2.3 | main.c:239-346 | 新05 |
| K-083 | Step 2 允许运行+SF_SYNCH_BOOT 同步/异步分叉 | 机制 | 存量 | 旧01 §2.3 | main.c:348-399 | 新05 |
| K-084 | Step 3 收齐 ready(阻塞语义) | 机制 | 存量 | 旧01 §2.3 | main.c:401-407 | 新05 |
| K-085 | Step 4 getnpid+挂闹钟 | 机制 | 存量 | 旧01 §2.3 | main.c:409-434 | 新05 |
| K-086 | boot_image_info_lookup 四表查找 | 机制 | 存量 | 旧01 §2.4 | main.c:709-779 | 新05 |
| K-087 | catch_boot_init_ready + VM 异步回复例外 | 机制 | 存量 | 旧01 §2.5、旧12 | main.c:784-821 | **新13(主讲述点)**,新05 调用点 |
| K-088 | boot 自升级流程(USE_LIVEUPDATE) | 机制 | 存量 | 旧01 §2.3.7、旧18 §2(重复讲述) | main.c:436-491 | **新20(主讲述点)**,新05 位置指针 |
| K-089 | BootTables 静态化(A-13) | 架构 | 存量 | 旧01 §3.1 | table.rs;main.rs placeholder | 新05 |
| K-090 | BootInit 状态机类型化(T1/Fix #9) | 架构 | 存量 | 旧01 §3.4 | boot.rs | 新05 |

**F 簇:主循环与监控**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-100 | 三活动结构(get/ process/ reply) | 概念 | 存量 | 旧06 §1.1 | main.c:40-43 | 新06 |
| K-101 | 四类消息分类(notify-CLOCK/notify-心跳/请求/ready) | 机制 | 存量 | 旧06 §1.2 | main.c:70-93 | 新06 |
| K-102 | reply 原语(RS 免回) | 机制 | 存量 | 旧06 §2 | utility.c:309-330 | 新06 |
| K-103 | late_reply+RS_LATEREPLY 延迟回复协议 | 机制 | 存量 | 旧06 §2 | utility.c:332-350 | 新06 |
| K-104 | rs_asynsend/AMF_NOREPLY | 机制 | 存量 | 旧06 §2 | utility.c:223-243 | 新06 |
| K-105 | EDONTREPLY 协议 | 约束 | 存量 | 旧06 §2 | main.c:125 | 新06 |
| K-106 | rs_idle_period:DEAD 回收+副本补建+VM 单副本 | 机制 | 存量 | 旧06 §2 | utility.c:443-484 | 新06 |
| K-107 | rs_is_idle 判定 | 机制 | 存量 | 旧06 §2 | utility.c:424-441 | 新06 |
| K-108 | signal_handler 效果(SIGCHLD→do_sigchld、SIGTERM→do_shutdown) | 机制 | 存量 | 旧06 §2 | main.c:631-642 | 新06 |
| K-109 | signal_manager 效果(终止→RS_TERMINATED+terminate_service+idle;VM 免转发;SIGS_SIGNAL_RECEIVED asynsend;EDEADEPT) | 机制 | 存量 | 旧06 §2 | main.c:647-704 | 新06 |
| K-110 | do_period 全表扫描三恢复路径(backoff/stop_tm 超时/ping 超时) | 机制 | 存量 | 旧07 §2 | request.c:943-1049 | 新07 |
| K-111 | 心跳双时间戳 r_alive_tm/r_check_tm 协议 | 约束 | 存量 | 旧07 §1.2 | type.h:72-73;main.c:87;request.c:1037 | 新07(协议)、新02(字段) |
| K-112 | backoff 二进制退避 | 机制 | 存量 | 旧07 §2 | request.c:979-985;const.h:52-53 | 新07 |
| K-113 | SIGTERM→SIGKILL 升级(crash_service) | 机制 | 存量 | 旧07 §2 | request.c:988-996 | 新07 |
| K-114 | ping 无答→crash+free pass+RS_NOPINGREPLY+init_err=EINTR | 机制 | 存量 | 旧07 §2 | request.c:1007-1030 | 新07 |
| K-115 | do_sigchld:waitpid 清理+实例族 free_slot+rupdate_clear_upds | 机制 | 存量 | 旧07 §2 | request.c:1051-1092 | 新07 |
| K-116 | update_period:prepare 超时→end_update(EINTR,RS_CANCEL) | 机制 | 存量 | 旧07 §2 | update.c:371-397 | 新07(调用点语义)、新18(机制) |
| K-117 | do_period 的 LU 门(updating&&!initializing,不 return) | 约束 | 存量 | 旧07 §2 | request.c:950-954 | 新07 |
| K-118 | effective_period:INIT_T/UPD_INIT_MAXTIME 初始化超时 | 约束 | 存量 | 旧07 §2 | request.c:968-975;const.h:60 | 新07 |

**G 簇:IPC 拓扑(sendmask)**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-120 | r_ipc_list 声明与 MAX_IPC_LIST | 数据 | 存量 | 旧05 §1.1 | type.h:105;const.h:22 | 新08 |
| K-121 | get_next_name 解析 | 机制 | 存量 | 旧05 §2 | manager.c:2112 起 | 新08 |
| K-122 | add_forward_ipc(SYSTEM/USER 例外+proc_name 匹配) | 机制 | 存量 | 旧05 §2 | manager.c:2155 起 | 新08 |
| K-123 | add_backward_ipc 反向补位 | 机制 | 存量 | 旧05 §2 | manager.c(后段) | 新08 |
| K-124 | init_privs(IPC_ALL/IPC_ALL_SYS 全置位) | 机制 | 存量 | 旧05 §2 | 调用点 manager.c:1700;定义 manager.c 后段(精确行 B 相复核) | 新08 |
| K-125 | s_ipc_to 内核强制(send→EPERM) | 约束 | 存量 | 旧05 §1.1 | 01-stage-kernel/23-ipc-filter | 新08(指针) |

**H 簇:slot 配置**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-130 | rs_start 结构全字段 | 数据 | 存量 | 旧08 §2 | rs.h | 新09 |
| K-131 | check_request 参数验证(调度/CPU/sigmgr) | 机制 | 存量 | 旧08 §2 | request.c:1265-1309 | 新09 |
| K-132 | copy_rs_start/copy_label(sys_datacopy 两段式) | 机制 | 存量 | 旧08 §2 | manager.c:135-173 | 新09 |
| K-133 | init_slot 默认展开(DSRV_*) | 机制 | 存量 | 旧08 §2 | manager.c:1710-1797 | 新09 |
| K-134 | edit_slot 全分支+RSS_*→SF_* 映射+core EPERM 例外 | 机制 | 存量 | 旧08 §2 | manager.c:1460-1705 | 新09 |
| K-135 | build_cmd_dep argv 解析(NUL 停止/argc 完整 token) | 机制 | 存量 | 旧08 §2 | manager.c:289-324 | 新09 |
| K-136 | inherit_service_defaults(IMM_SF/IMM_F 不可变继承) | 机制 | 存量 | 旧08 §2 | manager.c:1303-1327 | 新09 |
| K-137 | 配置三层管线(校验→拷贝→落地) | 概念 | 存量 | 旧08 §1.1 | request.c:1265;manager.c:135,1710 | 新09 |
| K-138 | system.conf 声明语言→rs_start 字段映射(uid/ipc/system/vm/io/irq/sigmgr/scheduler 指令) | 接口 | **新增** | (仅旧08:111 一处提及 parse.c) | minix/commands/minix-service/parse.c;etc/system.conf;system.conf.5 | 新09(契约节) |
| K-139 | 服务启动触发路径:rc.minix→minix-service up→RS_UP | 接口 | **新增** | (无) | etc/rc.minix:49 | 新09(契约节) |

**I 簇:exec**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-140 | srv_execve:RS 内建栈帧构建 | 机制 | 存量 | 旧09 §2 | exec.c:21 起 | 新10 |
| K-141 | do_exec:libexec 加载链+exec_info/exec_loaders | 机制 | 存量 | 旧09 §2 | exec.c:62-120 | 新10 |
| K-142 | exec_restart:PM_EXEC_RESTART 收尾 | 机制 | 存量 | 旧09 §2 | exec.c:121 起 | 新10 |
| K-143 | read_seg/read_exec(stat/open/read) | 机制 | 存量 | 旧09 §2 | exec.c;manager.c:1372-1423 | 新10 |
| K-144 | share_exec/free_exec 共享与释放 | 机制 | 存量 | 旧09 §2 | manager.c:1357-1459 | 新10 |
| K-145 | SF_USE_COPY/SF_NEED_COPY 两条映像路径 | 约束 | 存量 | 旧09 §1.2 | rs.h;manager.c:625-650,1628-1661 | 新10 |
| K-146 | Arc\<[u8]\> 镜像共享(A-5) | 架构 | 存量 | 旧09 §3 | exec.rs | 新10 |
| K-147 | minix-elf crate 接线(A-8) | 架构 | 存量 | 旧09 §3 | os/libs/minix-elf | 新10 |

**J 簇:创建与实例原语**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-150 | create_service 11 步编排(顺序即契约) | 机制 | 存量 | 旧10 §1.2/§2 | manager.c:531-712 | 新11 |
| K-151 | 前置三闸门+失败 free_slot+EPERM | 约束 | 存量 | 旧10 §2 | manager.c:547,555,563 | 新11 |
| K-152 | srv_fork(PM_SRV_FORK)与 A-1 决策 | 机制 | 存量 | 旧10 §2/§3 | manager.c:576;lib/libsys/srv_* | 新11 |
| K-153 | 表登记与计时器初始化 | 机制 | 存量 | 旧10 §2 | manager.c:589-596 | 新11 |
| K-154 | priv 提交(SET_SYS+getpriv) | 机制 | 存量 | 旧10 §2 | manager.c:600-605 | 新11 |
| K-155 | RS pin+VM MAKE_VM+全实例 pin+vm_set_priv | 机制 | 存量 | 旧10 §2 | manager.c:656-707 | 新11 |
| K-156 | setuid(0) VFS hack(行为保持点) | 约束 | 存量 | 旧10/旧11 | manager.c:656 附近,818 附近 | 新11+新12 |
| K-157 | clone_service(replica 链+RS 备份 sig mgr) | 机制 | 存量 | 旧10 §2 | manager.c:713-786 | 新11 |
| K-158 | activate_service | 机制 | 存量 | 旧10 §2 | manager.c:1013-1032 | 新11 |
| K-159 | clone_slot(浅/深拷贝语义) | 机制 | 存量 | 旧10 §2 | manager.c:1800 区段 | 新11 |
| K-160 | swap_slot/swap_slot_pointer(四链+update 链+全局索引交换) | 机制 | 存量 | 旧10 §2 | manager.c(update 区段) | 新11 |
| K-161 | **update_service 实例替换原语**(srv_update 条件执行/swap/重排/getpriv/activate) | 机制 | 存量 | 旧16 §2(物理位置) | **update.c:262-327(本次直读)** | **新11(主讲述点迁移,序差 D-2)** |
| K-162 | 失败回滚语义(创建中途失败→free_slot) | 约束 | 存量 | 旧10 §2 | manager.c:547-570 | 新11 |

**K 簇:发布**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-170 | publish 四注册(DS/mapdriver/PCI/devman)与触发谓词 | 机制 | 存量 | 旧11 §2 | manager.c:787-858 | 新12 |
| K-171 | unpublish 反向撤销 | 机制 | 存量 | 旧11 §2 | manager.c:864-922 | 新12 |
| K-172 | 发布失败→kill_service(外部行为) | 约束 | 存量 | 旧11 §1 | manager.c:813 附近 | 新12(指针)、新17(机制) |
| K-173 | PCI defer+PciAcl 占位(A-10) | 架构 | 存量 | 旧11 §3 | manager.c:828-835 | 新12 |
| K-174 | devman bind/unbind 消息 | 接口 | 存量 | 旧11 §2 | manager.c:840-853 | 新12(触发)、新21(契约) |

**L 簇:启动与初始化协议**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-180 | start_service 编排(create+activate+publish+run) | 机制 | 存量 | 旧12 §2 | manager.c:950-987 | 新13 |
| K-181 | run_service 原语(ALLOW+init_service) | 机制 | 存量 | 旧12 §2 | manager.c:923-949 | 新13 |
| K-182 | init_service:RS_INIT 消息构造(9 字段)+prealloc 清零 | 机制 | 存量 | 旧12 §2 | utility.c:18-67 | 新13 |
| K-183 | RS 免发(ROOT_SYS_PROC 早退) | 约束 | 存量 | 旧12 §2 | utility.c:29-31 | 新13、新20 |
| K-184 | do_init_ready 非 update 分支(清位+reply+end_srv_init) | 机制 | 存量 | 旧12 §2 | request.c:462-533 | 新13 |
| K-185 | do_init_ready update 分支(交 LU) | 机制 | 存量 | 旧12 §2(声明)、旧16(机制) | request.c:462-533 | 新13(分支)、新18(机制) |
| K-186 | do_upd_ready gate+四分支(peek/walk 分离,Fix #91) | 机制 | 存量 | 旧12 §2、旧16 | request.c:890-942 | 新13(协议)、新18(链) |
| K-187 | catch_boot_init_ready(阻塞收+VM 异步例外) | 机制 | 存量 | 旧01 §2.5、旧12 | main.c:784-821 | **新13(主讲述点)** |
| K-188 | end_srv_init(late reply+prev replica 清理+rupdate_upd_move+restarts++) | 机制 | 存量 | 旧12 §2 | manager.c:328-356 | 新13 |
| K-189 | RS_INIT 消息线格式(mess_rs_init) | 接口 | 存量 | 旧12、旧19、旧99 | ipc.h | 新13(语义)、新21(wire) |
| K-190 | 0/1 号文档域的 init 骨架接线(catch/init-ready,I2/Fix #70) | 工程测试 | 存量 | 旧01 §5 | boot.rs;lib.rs | 新05/新13 测试节 |

**M 簇:访问控制**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-200 | 两级授权模型(root 通道/控制列表通道) | 概念 | 存量 | 旧04 §1.1 | manager.c:21-79 | 新14 |
| K-201 | caller_is_root(PM_GETEPINFO euid==0) | 机制 | 存量 | 旧04 §2 | manager.c:21-38 | 新14 |
| K-202 | caller_can_control(r_nr_control/r_control 隔离策略+IN_USE 复核 Fix #43) | 机制 | 存量 | 旧04 §2 | manager.c:39-79 | 新14 |
| K-203 | check_call_permission 五条目标槽规则 | 约束 | 存量 | 旧04 §2 | manager.c:81-133 | 新14 |
| K-204 | 无目标槽请求(RS_UP/SHUTDOWN/GETSYSINFO)仅 root | 约束 | 存量 | 旧04 §1.1 | manager.c:81-133 | 新14 |

**N 簇:控制请求**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-210 | 八 handler 公共骨架(拷参→查槽→授权→校验→机制→回包) | 概念 | 存量 | 旧13 §1.1 | request.c:15-460 | 新15 |
| K-211 | do_up(RSS_NOBLOCK/FORCE_INIT_*、查重 label/dev_nr/domain、RS_LATEREPLY) | 机制 | 存量 | 旧13 §2 | request.c:15-110 | 新15 |
| K-212 | do_down+stop_service 停止原语(SIGTERM/SIGHUP-for-RS+r_stop_tm;RS_TERMINATED 直接 cleanup) | 机制 | 存量 | 旧13 §2 | request.c:111-159;manager.c:988-1012 | 新15 |
| K-213 | do_restart(仅 recovery script 场景) | 机制 | 存量 | 旧13 §2 | request.c:160-207 | 新15 |
| K-214 | do_refresh | 机制 | 存量 | 旧13 §2 | request.c:390-430 | 新15 |
| K-215 | do_shutdown(shutting_down+全表 RS_EXITING) | 机制 | 存量 | 旧13 §2 | request.c:431-461 | 新15 |
| K-216 | do_clone/do_unclone(SF_USE_REPL) | 机制 | 存量 | 旧13 §2 | request.c:208-297 | 新15 |
| K-217 | do_edit(UPDATE_SYS+vm_set_priv+sched 重初始化+replica 重建) | 机制 | 存量 | 旧13 §2 | request.c:298-389 | 新15 |
| K-218 | 服务生命周期次主线路径图 | 概念 | 存量 | 旧13 §(路径图) | plan §1.3 | 新15(保留) |

**O 簇:查询请求**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-220 | do_lookup(namebuf+label 查找) | 机制 | 存量 | 旧14 §2 | request.c:1144-1180 | 新16 |
| K-221 | do_getsysinfo 三表拷出(SI_PROC_TAB/PROCALL_TAB/PROCPUB_TAB) | 机制 | 存量 | 旧14 §2 | request.c:1095-1143 | 新16 |
| K-222 | do_sysctl 五子功能(SRV_STATUS/UPD_START/RUN/STOP/STATUS) | 机制 | 存量 | 旧14 §2 | request.c:1181-1228 | 新16 |
| K-223 | do_fi/fi_service(COMMON_REQ_FI_CTL) | 机制 | 存量 | 旧14 §2 | request.c:1229-1264;utility.c:69-81 | 新16 |
| K-224 | 打印族归属裁决(渲染→RS 诊断缝;IS dump→08-stage-is) | 工具 | 存量(需按 §3.4 重述) | 旧14 §2/§3 | utility.c:142-222,485-546;servers/is/dmp_rs.c | 新16(边界节) |
| K-225 | 诊断输出缝(sys_diagctl_stacktrace+rs_verbose 全家,E-11/A-3 diag) | 工具 | 存量 | 旧14 §3、旧19 | main.c:681-683;utility.c printf 族 | 新16(触发)、新21(缝) |

**P 簇:终止与恢复**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-230 | terminate_service 决策树(30+ 分支) | 机制 | 存量 | 旧15 §2 | manager.c:1055-1184 | 新17 |
| K-231 | terminate_decision 纯决策+Effects 执行体(I6/Fix #78、Fix #87/#88) | 架构 | 存量 | 旧15 §1.1/§3 | recovery.rs | 新17 |
| K-232 | restart_service(clone+update+run / 脚本两路) | 机制 | 存量 | 旧15 §2 | manager.c:1246-1302 | 新17 |
| K-233 | run_script(fork+execle sh;A-1 缺口标注) | 机制 | 存量 | 旧15 §2 | manager.c:1185-1245 | 新17 |
| K-234 | reincarnate_service(RS_REINCARNATE 恢复) | 机制 | 存量 | 旧15 §2 | manager.c:1033-1054 | 新17 |
| K-235 | kill_service/crash_service(含 RS 自身 _exit(1)) | 机制 | 存量 | 旧15 §2 | manager.c:360-404 | 新17 |
| K-236 | cleanup_service 两段式(标记 DEAD/disallow/late reply→真清理) | 机制 | 存量 | 旧15 §2 | manager.c:405-496 | 新17 |
| K-237 | detach_service(唯一 label 重发布+降权;MAX_DET_RESTART) | 机制 | 存量 | 旧15 §2 | manager.c:497-530;const.h:27 | 新17 |
| K-238 | get_service_instances 实例族遍历 | 机制 | 存量 | 旧15 §2 | manager.c:1334-1356 | 新17 |
| K-239 | 终止触发源汇总(信号/心跳/控制/LU 失败) | 概念 | 存量 | 旧15 §1.1 | 各篇 | 新17 |

**Q 簇:Live Update**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-250 | LU 核心不变量:任何时刻有一个可用版本 | 概念 | 存量 | 旧16 §1.1 | update.c | 新18 |
| K-251 | do_update 全流程(RSS_* LU 标志/EBUSY/EINVAL 门/clone+create/信号管理器/prealloc/init_state_data/入链) | 机制 | 存量 | 旧16 §2 | request.c:534-889 | 新18 |
| K-252 | rupdate 链操作族(add/clear/set_new_upd_flags/upd_init/upd_clear/upd_move) | 机制 | 存量 | 旧16 §2 | update.c:7-184 | 新18 |
| K-253 | start_update_prepare(_next):VM 多组件预分配+推进 curr | 机制 | 存量 | 旧16 §2 | update.c:401-531 | 新18 |
| K-254 | start_update:VM multi 时序+rs_receive_ticks 超时等待 | 机制 | 存量 | 旧16 §2 | update.c:532-620;utility.c:247-307 | 新18(A-9) |
| K-255 | end_update 五相位(curr/before_prepare/prepare_done/initializing/rev_iter)+prepare-only 取消 | 机制 | 存量 | 旧16 §2 | update.c:744-931 | 新18 |
| K-256 | end_srv_update(surviving/exiting 选择+cleanup/detach) | 机制 | 存量 | 旧16 §2 | update.c:932-1011 | 新18 |
| K-257 | srv_update(VM 侧)与 request_prepare_update_service | 机制 | 存量 | 旧16 §2 | update.c:185-261 | 新18、新21(VM 契约) |
| K-258 | UpdatePhase 类型化状态机(A-6)+peek/walk 分离(Fix #91)+超时回滚臂(Fix #92) | 架构 | 存量 | 旧16 §3/§5 | live_update.rs | 新18 |
| K-259 | rollback_service(RS 特例:sys_whoami+SF_VM_ROLLBACK) | 机制 | 存量 | 旧16 §2、旧18 §2 | update.c:330-366 | 新18(通用)、新20(RS 特例) |
| K-260 | Redox init 监督趋同(教学对照) | 概念 | **新增** | (无) | 03-stage-rs/todo.md §6.3(外部链接) | 新18(设计对照节,可选) |

**R 簇:LU 状态数据**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-270 | rs_state_data 三载体(eval/IPC filter/整包 grant) | 数据 | 存量 | 旧17 §1.2 | rs.h | 新19 |
| K-271 | init_state_data 解析(SEF_LU_STATE_* 分支/strtol/VM 保底条目) | 机制 | 存量 | 旧17 §2 | manager.c:174-288 | 新19 |
| K-272 | ipc_filter_el 与 ANY_USR/SYS/TSK 匹配 | 接口 | 存量 | 旧17 §2 | ipc_filter.h | 新19 |
| K-273 | cpf_grant_direct/cpf_revoke 状态授权 | 机制 | 存量 | 旧17 §2 | manager.c:246-284 | 新19 |
| K-274 | A-14:x86-64 布局常量(56B)/Endpoint::NONE 门/空 label fail-closed ESRCH | 架构 | 存量 | 旧17 §3 | plan §4 A-14 | 新19 |
| K-275 | LWT 布局见证三件套(rs_start/rprocpub/rproc,Fix #81/#85/#89) | 架构 | 存量 | 旧19 §3.3(Fix #94 后记)、旧17 | minix-types::ipc::rs_start 等 | 新21(wire)、新19(数据形状) |

**S 簇:RS 自身生命周期**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-280 | boot 自升级全程(主讲述点) | 机制 | 存量 | 旧18 §2(与旧01 重复) | main.c:436-491 | **新20(唯一主讲述点)** |
| K-281 | sef_cb_init_restart(stateful 转移+end_update+DONTSWAP+重挂闹钟) | 机制 | 存量 | 旧18 §2/§2.9b | main.c:499-544 | 新20 |
| K-282 | sef_cb_init_lu(重绑先于流程+四断言) | 机制 | 存量 | 旧18 §2/§2.9b | main.c:549-586 | 新20 |
| K-283 | 自模拟 response(init_response→do_init_ready;lu_response→do_upd_ready;EDONTREPLY 折算) | 机制 | 存量 | 旧01 §2、旧12 | main.c:591-626 | **新20** |
| K-284 | rollback RS 特例(旧版仍在跑时仅换槽) | 机制 | 存量 | 旧18 §2 | update.c:330-366 | 新20 |
| K-285 | RS 备份信号管理器(clone 时 s_bak_sig_mgr) | 约束 | 存量 | 旧18 §1、旧03 | manager.c:760-780 | 新20(动机)、新03(字段) |
| K-286 | RestartCb 重绑建模(Rs/Stateful 枚举,Fix #63) | 架构 | 存量 | 旧18 §2.9b | sef.rs;lib.rs | 新20 |

**T 簇:外部接口**

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 去向 |
|------|------|------|------|---------|------|------|
| K-290 | syslib 全签名面(sys_* 12+srv_* 3+getnpid/getnuid/getprocnr+waitpid+setuid) | 接口 | 存量 | 旧19 §2 | lib/libsys/* | 新21 |
| K-291 | 消息槽类型化(A-2)+Message 72 字节实测 pinning | 架构 | 存量 | 旧19 §3(Fix #94 后记) | minix-types;commit deec0c347 | 新21 |
| K-292 | **KernelApi 五域 trait 面**(SysApi/SchedApi/PmApi/VmApi/IpcApi supertrait 组合) | 架构 | 存量 | **旧01 §3.5(错位)** | lib.rs trait 定义;01 号 §3.5 | **新21(主讲述点迁移)** |
| K-293 | minix-sys/minix-types 依赖面清单与缺口 | 工具 | 存量 | 旧19 §4 | os/libs/minix-sys | 新21 |
| K-294 | E9 接线进度:SysApi/PmApi/SchedApi-KERNEL 已闭环;余 VmApi(挂 02 T12/T13)、SCHED 分支(挂 06)、RS 侧换装(域内) | 约束 | **新增** | (无,进度在 edge_todo) | fork-syscall-rewrite/edge_todo.md E9 进度注记(2026-09-16,commits cbb7b9571/d18be19e1/d58ef9d2e) | 新21(状态节) |
| K-295 | 时钟缝语义:C getticks 恒成功(kerninfo 共享页),Rust 侧 `?` 传播(Fix #93) | 约束 | 存量 | 旧19 §2.1(Fix #93 补) | lib/libsys/getuptime.c:9-23 | 新21 |
| K-296 | env_parse/rs_verbose 配置面 | 工具 | 存量 | 旧01 §2.3、旧19 | main.c:179 | 新21 |
| K-297 | 外部观测面归属:IS dump(dmp_rs.c)/procfs(service.c)不是 RS 域 | 约束 | **新增** | (无) | minix/servers/is/dmp_rs.c;minix/fs/procfs/service.c | 新16(边界指针) |

### 2.2 统计摘要

- 总条数:约 110 条(上表 10 簇);**存量约 102,新增 8**(K-070/K-071/K-075 SEF 框架半新增、K-138/K-139 声明语言、K-294 E9 状态、K-297 观测面归属,以及可选教学对照 K-260;其中 K-071/K-075 是"半存量半新增":RS 侧消费已有,框架行为无专篇)。
- 按类型:概念 14、机制 63、数据结构 12、接口与协议 14、约束与不变量 20、架构演进 16、工具与工程 8、测试性质 3(部分条目跨类型,按主类型计)。
- 按现有文档分布(存量主讲述点):旧01→新04/05/13/20/21 五篇承接;旧02→新02;旧03→新03;旧04→新14;旧05→新08+新03(拆);旧06→新06;旧07→新07;旧08→新09;旧09→新10;旧10→新11;旧11→新12;旧12→新13;旧13→新15;旧14→新16;旧15→新17;旧16→新18+新11(拆);旧17→新19;旧18→新20;旧19→新21;旧99→新01。

### 2.3 重复与主讲述点标记

| 主题 | 现有重复位置 | 新目录主讲述点 | 其余位置处理 |
|------|-------------|---------------|-------------|
| boot 自升级流程 | 旧01 §2.3.7 与旧18 §2 | 新20 | 新05 只留时序位置+指针 |
| fill_send_mask/fill_call_mask | 旧05 §2(主)与旧03(引用) | 新03 | 新08 引用 |
| update_service | 旧16 §2 | 新11 | 新18 引用(序差 D-2) |
| SEF 框架行为 | 旧01 §1.3/§2.2/§3.3、旧12、旧18 三处散落 | 新04 | 其余篇引用 |
| KernelApi 五域面 | 旧01 §3.5 | 新21 | 新05 留 seam 使用注记 |
| r_flags 位值 vs 语义 | 旧99 §2.2 与旧02 §2 | 新01(位值)+新02(生命周期角色) | 其余引用 |
| RS_INIT 消息结构 | 旧12(协议)与旧19(签名) | 新13(语义)+新21(签名) | 不变 |
| 心跳时间戳 | 旧02(字段)/旧06(写点)/旧07(判定) | 新07(协议语义) | 02/06 保持字段/写点角色 |

---

## 3. 覆盖审计

### 3.1 主题全集来源

1. **C 源码符号**:8 个 .c 共 121 个函数(plan §7.2 R-5 口径,本次函数面 grep 复核一致)+ 6 个 stage 内头文件的全部结构/宏(`type.h`/`glo.h`/`const.h`/`proto.h`/`inc.h`/`table.c` 三表)。
2. **OS 通用概念**:服务监督器(supervisor)模型、心跳/watchdog、二进制退避、能力位图(priv)、服务注册与发现(DS/设备表)、原子实例替换(LU)、状态迁移、自举问题。
3. **非 C 制品主题**:SEF 框架(libsys)、rs_start 线格式、system.conf 声明语言、rc 启动链、消息槽联合体、构建(Makefile/USE_PCI/USE_LIVEUPDATE)、测试基建(os/servers/rs 测试 333 项+coverage-extract+check-rs-unwired T7 门)。
4. **阶段边界契约**:master-plan 启动因果链中 RS 段;edge_todo E9(生产接线)/E5(c)(LU 联调)/E-MINSYS-HYGIENE;plan §5 覆盖契约;旧 plan WONTFIX 项(§3.5 表)。

### 3.2 覆盖缺口表

| 编号 | 缺口主题 | 证据(为何确认缺失) | 建议 | 落实 |
|------|---------|--------------------|------|------|
| N-1 | SEF 框架行为(sef_startup 流程/init 拦截/response 协议/STATEFUL 通用体) | 现有 21 篇无专篇;框架内容散落旧01 §2.2(仅注册半)、旧18 §2.9b(转述);lib/libsys/sef*.c 约 2210 行是全部服务器共用的跨模块接口 | 新建新 04 | K-070~K-078 → 新04 |
| N-2 | system.conf 声明语言与 minix-service 客户端契约(rs_start 的生产侧) | grep 全目录仅旧08:111 一处提及 parse.c;`etc/system.conf` 的 uid/ipc/system/vm/io/irq/sigmgr/scheduler 指令→rs_start 字段映射无文档;`etc/rc.minix:49` 触发链无文档 | 并入新 09(契约节,不搬命令实现) | K-138/K-139 → 新09 |
| N-3 | E9 生产接线进度状态(五域面已闭环/未闭环分域) | 旧19 成文于接线前;edge_todo E9 三条进度注记(2026-09-16)未回写文档 | 并入新 21(状态节) | K-294 → 新21 |
| N-4 | 外部观测面归属声明(IS 的 dmp_rs.c、procfs 的 service.c 读 RS 表,但属 08-stage-is/15-stage-fs) | 旧14 打印族归属在 R31(Fix #94)后与旧19/旧14 文本冲突(§3.4) | 并入新 16(边界节)+新 21 指针 | K-297/K-224 → 新16 |
| N-5 | 00 导航表陈旧行(§4.0 表 17/18 行的语义模块描述与实际文档漂移) | 旧00:226 行称 17 号讲"rupdater/sys_pids/instance 链",实际 17 号讲状态数据迁移;18 行含已不用的"SEF_RS_UPDATE_SELF" | 重建新 00 导航 | 新00 契约 |
| N-6 | Redox init 监督趋同对照(todo §6.3 已收集,文档未吸收) | todo.md §6.3 有锚点,正式文档无 | 可选教学对照,入新 18 设计节 | K-260 → 新18(可选) |

### 3.3 重复主题表

见 §2.3(8 条)。每条已给主讲述点与其余位置降为引用的处理。无新增重复。

### 3.4 越界主题表

| 编号 | 越界事实 | 证据 | 裁决 |
|------|---------|------|------|
| Y-1 | 旧01 §3.5 讲 KernelApi 五域 trait 面契约(约 60 行),其自标题已注明"外部契约归 19" | 旧01:746-806 | 迁往新 21;新 05 只留 boot seam 使用注记 |
| Y-2 | 旧18 §2.9b 是 Fix #59/#63 修复日志,混入正文 | 旧18:141-159 | 新 20 按内容吸收进 §2 相应小节;修复史留在 todo.md(项目惯例:修复记录不入正文) |
| Y-3 | 旧17 存在两个"## 3. Rust 设计决策"标题(约 127/138 行),R13 接线注记块插在两标题之间 | 旧17 直读 | 新 19 重建时合并为单节,接线注记按内容归位 |
| Y-4 | 旧14 §2/§3 与 todo R31/Fix #94 对打印族(srv_to_string_gen/print_services_status/print_update_status/srv_upd_to_string)归属冲突:文档说"归 19 诊断输出面",todo 说"归 08-stage-is" | 旧14:116-128 vs todo.md §3.2 R31 | **裁决(以 C 为据)**:四个函数物理在 `utility.c`(RS 进程内 printf),由 RS_SYSCTL 子功能与 rs_verbose 日志触发——渲染语义属 RS 域(新 16 边界节+新 21 诊断缝);R31 所指"08-stage-is"仅适用于 IS 自己的 dump 命令(`servers/is/dmp_rs.c`,经 RS_GETSYSINFO 拉表自行打印)。B 相按此改写新 16,并回写 todo 判定表注记 |
| Y-5 | 旧00 §4.0 导航表 17/18 行语义描述陈旧(同 N-5) | 旧00:226-227 | 新 00 重建 |

### 3.5 非 C 主题逐项回答(固定 10 项清单)

| 主题 | 在哪里讲 | 依据 |
|------|---------|------|
| 链接与加载 | 不在本 stage。RS ELF 的加载属 01-stage-kernel(boot 协议);RS 内 exec 加载(服务 ELF)在新 10 | 主线因果链;旧09 已按此划界 |
| 镜像与内存布局 | 新 10(服务映像两条路径)+新 21(Message 72B/LWT wire 布局) | K-145/K-275/K-291 |
| 汇编入口与陷阱进入 | 不在本 stage。trap 层是 minix-sys 域(edge E1);RS 只消费 IpcApi | 旧19 划界;edge_todo E9 |
| 启动装配 | 新 05(boot_image 三表+四步 boot);内核侧装配归 01-stage-kernel/06、09 | K-040/K-082 |
| 构建与工具链 | 新 21(一节):Makefile(libsys+libexec 链接、USE_PCI 条件)、cargo feature(A-11/A-10) | `minix/servers/rs/Makefile` 直读;plan WONTFIX 项升格为一节契约 |
| 跨模块接口与线格式 | 新 21(全集)+各机制篇的触发谓词;SEF 框架线格式在新 04 | K-290~K-293、K-075 |
| 错误路径 | 各机制篇内联(每篇事实底线含失败分支)+新 01 错误表(error.c)+新 21(errno 映射 A-12) | plan §3 模板第 2 章 |
| 关闭与退出 | 新 15(do_shutdown)、新 17(全表 RS_EXITING 清理、core 服务陪葬)、新 20(RS 自身退出) | K-215/K-236/K-235 |
| 并发与同步 | 新 06 开篇声明:单线程事件循环(AGENTS.md 执行模型),无锁;内核侧 SMP 不入本 stage | main.c:40-43 注释;旧06 §1.1 |
| 测试基建 | 新 00(现状概览)+各篇 §5;专项工具(coverage-extract、check-rs-unwired T7 门)在新 21 工程节 | todo.md §0/§6.0 |

---

## 4. 新目录

### 4.1 新篇章总表(22 篇)

判定:服务事件循环型,按"词典 → 数据底账 → 权限底账 → 框架 → 启动主线 → 运行时骨架 → 请求面(门→配置→创建→协议)→ 命令面 → 恢复 → LU 全链 → 契约"组织。

| 新编号 | 标题 | 一句话定位 | 分组 | 旧来源 |
|--------|------|-----------|------|--------|
| 00 | rs-overview:RS 全景与导航 | RS 是谁、boot 链哪里、一辈子做什么、文档怎么读 | 总览 | 旧00(重建导航) |
| 01 | rs-concepts:常量与标志词典 | 每个常量族的位值、语义一句话、权威 Rust 位置 | 词典 | 旧99 |
| 02 | rs-process-table:服务登记表 | rproc/rprocpub 双表、查找/分配/回收、不变式 | 数据底账 | 旧02 |
| 03 | rs-privilege:权限结构与 privctl | priv 字段语义、掩码原语、privctl 每操作的效果 | 权限底账 | 旧03(+旧05 原语半) |
| 04 | rs-sef-framework:SEF 框架 | 服务初始化/重启/更新/信号的用户态协议骨架 | 框架 | **新建**(旧01/12/18 抽出+libsys) |
| 05 | rs-boot-init:boot 四步 | main→sef_cb_init_fresh 逐步时序与 boot 三表 | 启动主线 | 旧01(瘦身) |
| 06 | rs-main-loop:主循环与回复协议 | 取活/分类/回活三活动+信号效果+空闲期 | 运行时骨架 | 旧06 |
| 07 | rs-period-heartbeat:看门狗 | do_period 三恢复路径+backoff+sigchld | 运行时骨架 | 旧07 |
| 08 | rs-ipc-sendmask:IPC 拓扑 | r_ipc_list→s_ipc_to 两条计算路径 | 请求面·配置 | 旧05(-原语半) |
| 09 | rs-slot-config:配置管线 | rs_start 校验→拷贝→落地(init_slot/edit_slot)+声明语言契约 | 请求面·配置 | 旧08(+新增 N-2) |
| 10 | rs-exec:服务映像加载 | 两条映像路径+RS 自定义 exec 链 | 请求面·创建 | 旧09 |
| 11 | rs-service-create:创建与实例原语 | create_service 编排+clone/activate/swap/update_service | 请求面·创建 | 旧10(+旧16 update_service) |
| 12 | rs-publish:发布与发现 | 四个外部目录的注册/撤销 | 请求面·创建 | 旧11 |
| 13 | rs-init-run:初始化握手协议 | RS_INIT/RS_LU_PREPARE 消息协议+boot/运行时共用原语 | 请求面·协议 | 旧12(+旧01 catch) |
| 14 | rs-access-control:访问控制 | 两级授权+五条目标槽规则 | 命令面·门 | 旧04(位置迁移) |
| 15 | rs-control-requests:控制请求 | 八 handler+停止原语+生命周期路径图 | 命令面 | 旧13 |
| 16 | rs-query-requests:观测与注入 | 四个查询/注入 handler+打印族边界 | 命令面 | 旧14 |
| 17 | rs-terminate-restart:终局与恢复 | terminate 决策树+恢复执行器+清理执行器 | 恢复 | 旧15 |
| 18 | rs-live-update:LU 状态机 | prepare→update→init→end/rollback 编排 | LU | 旧16(-update_service) |
| 19 | rs-state-data:LU 状态迁移 | rs_state_data 三载体+init_state_data+grants | LU | 旧17(修结构) |
| 20 | rs-self-lifecycle:RS 自身生命周期 | 自升级/restart/LU/rollback 四特例+自模拟 | LU | 旧18(吸收 §2.9b) |
| 21 | rs-external-interfaces:外部接口契约 | 五域 trait 面+syslib 签名+wire+E9 状态+构建 | 契约 | 旧19(+旧01 §3.5) |

(99 号取消——词典前移为新 01;旧编号整体退役,B 相归档。)

### 4.2 阅读路径

- **主线(线性)**:00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15 → 16 → 17 → 18 → 19 → 20 → 21。
- **按目的(可跳读)**:
  - 理解 boot:00 → 05(回查 02/03/04)→ 13(K-087)。
  - 理解"服务怎么起来":09 → 10 → 11 → 12 → 13 → 15(do_up 例)。
  - 理解崩溃恢复:07 → 17。
  - 理解 LU:18 → 19 → 20(前置 11/13)。
  - wire-up 施工:21 → 01(位值)→ 各机制篇事实底线。
- **支线/工具书**:01(词典)、21(契约)按查读;16 §打印边界、09 §声明语言为支线节。

### 4.3 并行主题的分组与代表成员

- **请求处理并行体**(15 个消息号):框架 = 新 06(分派骨架)+新 14(授权门);分组 = 控制(15)、观测(16)、ready 协议(13);代表成员 = do_up(15 内精讲全链),其余 handler 差异表收束。
- **LU 三篇**(18/19/20):18 = 编排状态机(主),19 = 状态载体机制,20 = RS 自指特例;组内阅读序 18 → 19 → 20,交叉引用均为后向(19/20 回指 18)。
- **创建并行体**(10/11/12):线性依赖(exec→create→publish),不设代表成员,按因果序。

---

## 5. 每篇契约

> 知识点清单的"锚点"列引用 §2 池编号(K-xxx),池中已带锚点;来源列 S=存量(附旧文档位置)/N=新增(附证据)。

### 00-rs-overview

- 一句话定位:让读者 20 分钟内建立 RS 的全局心智模型,并能按目的选对入口篇。
- 讲什么:K-001~K-007(身份/两层顺序/职责全景/生命周期次主线/ARCH 索引/覆盖契约/阅读路线)。
- 不讲什么:一切机制细节(各自归 02~21);C 真序表的完整版(以图+关键锚点呈现,逐条表在新 05/06)。
- 前置:无(允许引用 `../01-stage-kernel/09-vm-boot-protocol.md` 等外部 stage 文档)。
- 后置:全部篇章(导航)。
- 事实底线:main.c:1-10(职责注释);com.h:61,77;kernel/table.c:44-64 与 servers/rs/table.c:15-30 两表对照;plan §4 ARCH 清单;todo §0 测试基线。
- 知识点清单:K-001(S,旧00§1.1)|K-002(S,旧00§1.2)|K-003(S,旧00§1)|K-004(S,旧00§3)|K-005(S,旧00§4-6,重建)|K-006(S,旧00§5.4)|K-007(S,旧00§5.5)。
- 验收标准:①两表(登记序/迭代序)对照表出现且行号正确;②启动主线图能回答"每篇在四步 boot 或主循环哪个位置";③导航表每行的"语义模块"列与对应新篇头部声明一致(修复 N-5 类漂移);④阅读路线覆盖 §4.2 五条路径。

### 01-rs-concepts(常量与标志词典)

- 一句话定位:一册可查的位值词典,使任何篇章引用常量都回指此处,杜绝位值漂移。
- 讲什么:K-010~K-022(六族常量+错误表+宏族+权威位置决策)。
- 不讲什么:常量如何驱动状态机(07/15/16/17/18);消息如何被消费(13/15/16)。
- 前置:00。
- 后置:全部机制篇。
- 事实底线:const.h 全文(123 行);rs.h 全表;sef.h 标志段;ipc_filter.h;com.h:463-492;error.c。
- 知识点清单:K-010~K-021(S,旧99 各节)|K-022(S,旧99§3)。
- 验收标准:①15 消息/16 r_flags/13 SF/RSS_*/SEF_* 四族条目数与 grep 计数一致;②每条带 C 行锚点+Rust 权威位置;③错误表逐条 errno↔描述可对上 error.c。

### 02-rs-process-table

- 一句话定位:讲清 RS 一切操作的锚点——服务槽的数据结构、生命周期角色与槽位原语。
- 讲什么:K-030~K-043(双表/全字段/查找/分配回收/isokendpt/标志角色/指针链 Rust 化/不变式)。
- 不讲什么:priv 字段语义(03);心跳字段读写时机(07);update 描述符状态机(18);exec 镜像(10);r_control 填充(09)。
- 前置:00、01(标志位值)。
- 后置:03/05/06/07/08/09/11/13/15/16/17/18/19(几乎全部)。
- 事实底线:type.h:58-112(rproc)、type.h:26-56(rprocupd/rupdate)、glo.h、rs.h(rprocpub)、manager.c:1933-2062(lookup 族)、manager.c alloc/free 段、utility.c:352-362。
- 知识点清单:K-030~K-039(S,旧02 各节)|K-041/K-042(S,旧02§3)|K-043(S,旧02§3/§5,Fix #67)|K-111(字段半,主讲述点在 07)。
- 验收标准:①rproc 全字段表逐字段有 C 锚点+读/写方标注;②"查这三类问题"(在不在/什么状态/还能否分配)各对应到具体原语;③四指针链→Option\<SlotId\> 对照图出现;④assert_consistent 校验的不变式清单成列。

### 03-rs-privilege

- 一句话定位:讲清"槽位里要提交给内核的权限结构"每个字段是什么、由谁填、privctl 每个操作对内核做了什么。
- 讲什么:K-050~K-060(+K-053 原语迁移)。
- 不讲什么:r_ipc_list 拓扑计算(08);调用者权限检查(14);创建/终止对 priv 的消费(11/17);消息签名(21)。
- 前置:00、01、02。
- 后置:05(boot Step 1 引用)、08(init_privs 用原语)、11(SET_SYS)、17(sched_stop/UPDATE_SYS)、21(syslib 签名)。
- 事实底线:kernel/priv.h(priv 结构)、main.c:258-296(boot 构造)、utility.c:82-141(双 fill 原语)、utility.c:364-412(sched_init_proc/update_sig_mgrs)、main.c:287/379/478/485(privctl 调用点)、main.c:315-317(vm_call_mask)。
- 知识点清单:K-050~K-059(S,旧03+旧05§2 原语半)|K-060(S,旧03)|K-052(S,定义位置标待验证)。
- 验收标准:①privctl 六操作各有"调用点→内核效果"小表;②fill 双原语从旧 05 迁入且旧 05 引用改写;③RS/VM 例外(跳 SET_SYS)显式成节;④sched 复合传输(KERNEL vs SCHED 消息)对照出现。

### 04-rs-sef-framework(新建)

- 一句话定位:讲清所有 Minix3 服务共用的用户态生命周期框架——谁调用了服务里的 sef_cb_init_fresh,init/restart/LU 三类型与 response 协议如何流转。
- 讲什么:K-070~K-078(框架定位/sef_startup/回调表/三 init 类型/STATEFUL 与 LU_DEFAULT 通用体/sef_receive_status 拦截/信号双回调/Rust SefCallbacks 抽象)。
- 不讲什么:RS 的四步 boot 正文(05);RS_INIT 消息字段语义(13);RS 自身的 restart/LU 回调体(20);SEF_* 位值(01);LU 编排(18)。
- 前置:00、01、03(回调表里 sig mgr 概念浅引)。
- 后置:05(boot 入口)、06(信号拦截)、13(init 协议)、18(LU 状态)、20(restart/LU 回调)。
- 事实底线:lib/libsys/sef.c:68-150,295(sef_startup/sef_receive_status/EINTR)、sef_init.c:98,427(通用体分派,直读核实)、sef_signal.c(202 行)、sef_liveupdate.c(566 行)、sef.h(SEF_CB_*/SEF_INIT_*/SEF_LU_*)、main.c:136-153(RS 注册面)。
- 知识点清单:K-070(N,sef*.c 9 文件)|K-071(N,sef.c:68-150)|K-072(S,旧01§2.2)|K-073(S,旧01§1.3)|K-074(S,旧18§2.9b 转述→直读核实)|K-075(N,sef.c:150,295)|K-076(S,旧06 效果半)|K-077(S,旧16/17 框架半)|K-078(S,旧01§3.3/旧18)。
- 验收标准:①sef_startup 从阻塞到回调再到 response 的时序图(带 sef.c 行锚);②"init 消息如何被拦截"一节存在且引用 sef_receive_status;③STATEFUL/LU_DEFAULT 两通用体各有直读锚点(不得转引旧文档);④Rust SefCallbacks trait+RestartCb 枚举对照表;⑤明示"框架为全体服务器共用,RS 先行实现"[ARCH: A-7]。

### 05-rs-boot-init

- 一句话定位:按运行时序讲完 RS 如何把自己初始化好——四步 boot 的每一步为什么在那个位置。
- 讲什么:K-080~K-089(+K-040 三表+K-044)。
- 不讲什么:SEF 框架行为(04);priv 字段语义(03,只讲序列);boot 自升级细节(20,只留位置);KernelApi trait 面(21);ready 消息协议(13,catch 只留调用点)。
- 前置:00、01、02、03、04。
- 后置:06(主循环接管)、13(boot 复用 init_service)、20(自升级详篇)。
- 事实底线:main.c:38-57(入口)、158-237(预备)、239-346(Step1)、348-399(Step2)、401-434(Step3/4+闹钟)、709-779(四表查找)、784-821(catch 调用点)、table.c:15-50(三表)。
- 知识点清单:K-080~K-087(S,旧01 各节)|K-088(S,仅位置指针)|K-089/K-090(S,旧01§3)|K-040(S,旧01§2.7)|K-044(S,旧01§3.1)。
- 验收标准:①§1.1 的鸡生蛋问题链保留并逐条对锚;②四步各小节首行有行号区间;③两表迭代序声明(序差 D-1)在 Step 1 开头;④RS/VM 双例外出现在 Step1(跳 SET_SYS)与 Step2(直 init_service)两处;⑤自升级仅一小节+指针到新 20;⑥BootTables 静态化与 A-13 对照。

### 06-rs-main-loop

- 一句话定位:RS 的运行时心脏——取活/分类/回活骨架、回复协议族与信号效果。
- 讲什么:K-100~K-109。
- 不讲什么:do_period 决策(07);各 handler 语义(13/15/16/18);SEF 信号拦截机制(04)。
- 前置:00、01、02、04、05。
- 后置:07(周期)、13/15/16/18(handler 均经此入口)。
- 事实底线:main.c:40-131、utility.c:223-243(asynsend)、309-350(reply/late_reply)、424-484(is_idle/idle_period)、631-704(信号双回调体)、sef.c:150(拦截回指 04)。
- 知识点清单:K-100~K-109(S,旧06 各节)。
- 验收标准:①四类消息分类表含分派行号;②reply/late_reply/EDONTREPLY/rs_asynsend 四原语语义+RS 免回例外;③rs_idle_period 双职责(DEAD 回收+副本补建)与 VM 单副本限制;④signal_manager 的 EDEADEPT/免转发/asynsend 转发三分支;⑤"主循环 dispatch 表只列 handler 名"的导航豁免声明成文。

### 07-rs-period-heartbeat

- 一句话定位:微内核看门狗——每次时钟 tick 对每个服务的三条恢复路径决策。
- 讲什么:K-110~K-118。
- 不讲什么:restart/crash/cleanup 机制(17,只留调用点);end_update(18,update_period 只留调用点豁免);分派骨架(06)。
- 前置:00、01、02、05、06。
- 后置:17(恢复)、18(LU 超时门)。
- 事实底线:request.c:943-1092(do_period/do_sigchld 直读)、update.c:371-397(update_period)、const.h:49-53,60、type.h:72-73。
- 知识点清单:K-110~K-118(S,旧07 各节)。
- 验收标准:①三路径触发条件表(backoff 递减→restart;stop_tm 超 2*RS_DELTA_T→crash;alive<check 超 2*period→crash)行号精确;②free pass 逻辑(lookup RS_INITIALIZING 让路)成段;③心跳双时间戳协议图;④update_period 门与"不 return 继续"扫表语义;⑤Fix #92 的超时回滚测试面在测试节引用。

### 08-rs-ipc-sendmask

- 一句话定位:服务间消息拓扑如何从名字列表变成内核位图。
- 讲什么:K-120~K-125。
- 不讲什么:fill 原语(03);priv 提交(03);r_ipc_list 的拷贝校验(09);内核强制实现(01-stage-kernel/23)。
- 前置:00、01、02、03。
- 后置:09(init_slot/edit_slot 调 init_privs)、15(do_edit 重跑)。
- 事实底线:type.h:105、const.h:22、manager.c:2112 起(get_next_name)、2155 起(add_forward_ipc)、后段(add_backward_ipc/init_privs,定义精确行 B 相以 rg 复核并回填)。
- 知识点清单:K-120~K-124(S,旧05)|K-125(S,旧05§1.1 指针)。
- 验收标准:①两条计算路径表(boot 全置 vs init_privs 解析)保留;②SYSTEM/USER 例外与 proc_name 匹配规则带锚;③IPC_ALL/IPC_ALL_SYS 语义对照;④原语引用改指新 03。

### 09-rs-slot-config

- 一句话定位:从服务声明(rs_start)到可创建槽位的三层管线,外加声明语言的生产侧契约。
- 讲什么:K-130~K-139。
- 不讲什么:掩码计算(08,只留调用点);映像读入(10);创建/发布(11/12);minix-service 命令实现(18-stage-commands,本篇只讲格式契约)。
- 前置:00、01、02、03、08。
- 后置:10/11/12(消费就绪槽位)、15(do_edit 复用 edit_slot)。
- 事实底线:request.c:1265-1309、manager.c:135-173/289-324/1303-1327/1460-1797、rs.h(rs_start)、**新增:minix/commands/minix-service/parse.c(指令→字段映射)、etc/system.conf(RS 段直读)、etc/rc.minix:49、system.conf.5**。
- 知识点清单:K-130~K-137(S,旧08)|K-138/K-139(N,新增契约节)。
- 验收标准:①三层管线图(校验→拷贝→落地)保留;②init_slot vs edit_slot 差异表;③RSS_*→SF_* 映射表含 core EPERM 例外;④**新增**:system.conf 指令→rs_start 字段映射表(至少 uid/ipc/system/vm/io/irq/sigmgr/scheduler 八指令)与 rc.minix 触发链各一节;⑤build_cmd_dep 的 NUL/argc 语义(旧 Fix #2/#23/#34 结论)在测试节引用。

### 10-rs-exec

- 一句话定位:谁把服务二进制放进地址空间——RS 自定义 exec 链与两条映像路径。
- 讲什么:K-140~K-147。
- 不讲什么:槽位落地(09);创建编排(11,只留三调用点);ELF 解析器内部(os/libs/minix-elf);PM 侧 exec(04-stage-pm)。
- 前置:00、01、02、09。
- 后置:11(read_exec/srv_execve/free_exec 调用点)、17(restart 复用常驻副本)。
- 事实底线:exec.c 全文(165 行)、manager.c:625-650/1357-1459/1628-1661、rs.h(SF_USE_COPY/NEED_COPY)。
- 知识点清单:K-140~K-147(S,旧09)。
- 验收标准:①两条映像路径图(SF_USE_COPY 常驻 vs 按需读入即释放)保留;②srv_execve→do_exec→exec_restart 三段链各带行锚;③Arc<[u8]> 与 free_exec 引用计数扫描的对照;④SF_NEED_COPY 缺副本→EPERM 的闸门。

### 11-rs-service-create

- 一句话定位:服务进程的"诞生"——create_service 的顺序即契约,外加实例替换四原语(clone/activate/swap/update_service)。
- 讲什么:K-150~K-162(含 K-161 update_service 迁入)。
- 不讲什么:配置(09)、映像细节(10)、发布(12)、RS_INIT 握手(13)、LU 编排(18,引用 update_service 时回指本篇)、清理机制(17,失败路径只留外部行为)。
- 前置:00、01、02、03、08、09、10。
- 后置:12(publish)、13(start_service 调 create)、17(restart 用 clone+update_service)、18(LU 用 update_service)、20(自升级用 clone_slot+update_service)。
- 事实底线:manager.c:531-712/713-786/1013-1032/1800 区段(swap_slot)、**update.c:262-327(update_service,直读)**、rs.h(A-1 相关)、plan A-1/A-3。
- 知识点清单:K-150~K-160/K-162(S,旧10)|K-161(S,旧16→迁入,序差 D-2)|K-156(S,行为保持点)。
- 验收标准:①11 步编排图每步带行号与失败出口;②update_service 小节含"srv_update 仅 RS_SWAP 时执行"与 getpriv×2 panic 语义(直读证据);③clone_slot 浅/深拷贝语义表;④A-1(srv_fork 无 libc)与 A-3(指针链)对照;⑤四链+update 链+全局索引的 swap 交换图。

### 12-rs-publish

- 一句话定位:让服务可以被找到——四个外部目录的注册与撤销。
- 讲什么:K-170~K-174(+K-156 第二出现点)。
- 不讲什么:DS/VFS/devman/PCI 的消息布局(21);kill_service 机制(17);detach 的重发布(17,语义不同处对照);命令侧(18-stage-commands)。
- 前置:00、01、02、11。
- 后置:13(start_service 编排 publish)、17(unpublish/detach 消费)。
- 事实底线:manager.c:787-922(直读区间与旧11 一致)、setuid(0) hack 两处注释(10/11 各自标注)。
- 知识点清单:K-170~K-174(S,旧11)。
- 验收标准:①四注册的触发谓词表(dev_nr>0||nr_domain>0 等);②"发布为什么独立于创建"两理由;③失败→kill_service 外部行为声明(豁免入 G3 白名单);④PCI defer+PciAcl fail-closed 占位。

### 13-rs-init-run

- 一句话定位:用消息握手完成初始化——RS_INIT 协议、boot 与运行时共用的启动原语、ready 消息仲裁。
- 讲什么:K-180~K-190。
- 不讲什么:cleanup/crash 机制(17,end_srv_init 内只留单函数豁免);LU 链机制(18,update 分支只留裁决指针);消息签名(21)。
- 前置:00、01、02、04、09、10、11、12。
- 后置:15(do_up/do_down 用 start/stop)、16、17(reincarnate/restart 用 run_service)、18(do_upd_ready 链)。
- 事实底线:manager.c:328-356/923-1012、utility.c:18-67、request.c:462-533/890-942、main.c:784-821(catch 主讲述点迁入)、ipc.h(mess_rs_init)。
- 知识点清单:K-180~K-189(S,旧12+旧01§2.5)|K-190(S,旧01§5 测试面)。
- 验收标准:①异步握手时序图(RS_INIT 下行/ready 上行)含 rproctab_gid 创建(05)与消费(本篇)闭环;②init_service 九字段表;③RS 免发+VM 异步回复双例外;④do_upd_ready 的 gate 四分支与 peek/walk 分离(Fix #91)设计注记;⑤catch_boot_init_ready 从旧 01 迁入且新 05 只留调用点。

### 14-rs-access-control

- 一句话定位:谁有资格命令 RS——两级授权与五条目标槽规则。
- 讲什么:K-200~K-204。
- 不讲什么:priv 结构(03,只消费 SYS_PROC 位);r_control 填充(09,只读);各 handler 语义(15/16/18);RUPDATE 状态机(18,只陈述 EBUSY 规则)。
- 前置:00、01、02、03、06、09(r_control 字段来源回指)。
- 后置:15/16/18(全部 handler 的入口检查)。
- 事实底线:manager.c:21-133(直读函数面)。
- 知识点清单:K-200~K-204(S,旧04)。
- 验收标准:①两级授权图+无目标槽请求清单(RS_UP/SHUTDOWN/GETSYSINFO 仅 root);②五条规则逐条带行号(EBUSY 三态/TERMINATED 例外/core 禁 RS_DOWN);③"root=euid(PM_GETEPINFO)"术语框;④IN_USE 复核(Fix #43)在测试节引用。

### 15-rs-control-requests

- 一句话定位:外部如何操作服务的一生——八个控制 handler 与生命周期路径图。
- 讲什么:K-210~K-218。
- 不讲什么:授权判定(14,只调用);机制内部(09/11/13/17 各自归);LU(18)。
- 前置:00-14 全线(06/09/11/13/14 为强依赖)。
- 后置:17(do_down/do_refresh 触发停止→恢复链)、18(RS_UPDATE 入口指 18)。
- 事实底线:request.c:15-460、manager.c:988-1012(stop_service)。
- 知识点清单:K-210~K-218(S,旧13)。
- 验收标准:①公共骨架六步图;②do_up 全链精讲(代表成员);③其余 handler 差异表;④stop_service 停止原语(SIGTERM/SIGHUP-for-RS+r_stop_tm)独立成节;⑤服务生命周期次主线路径图保留并更新编号。

### 16-rs-query-requests

- 一句话定位:观测与注入——四个只读/注入 handler 与打印族的归属边界。
- 讲什么:K-220~K-225(+K-297)。
- 不讲什么:授权(14);LU 链(18,UPD_* 只委托);恢复(17,FI 只是触发源);渲染实现(21 诊断缝);IS dump(08-stage-is)。
- 前置:00、01、02、06、13、14。
- 后置:18(UPD_START/STOP 入口)、21(诊断缝)。
- 事实底线:request.c:1095-1264、utility.c:69-81/142-222/485-546、servers/is/dmp_rs.c(归属判定)、fs/procfs/service.c(归属判定)。
- 知识点清单:K-220~K-224(S,旧14,其中 K-224 按越界裁决 Y-4 重写)|K-297(N)。
- 验收标准:①四 handler 各一小节带行号;②do_getsysinfo 三表裸拷贝→Rust 侧约束(Fix #85/#89 live 面);③打印族边界节:渲染=RS 诊断缝、IS dump=08-stage-is,双侧锚点齐全(裁决 Y-4 落地);④do_fi 不改槽位状态的声明。

### 17-rs-terminate-restart

- 一句话定位:一个服务的终局——决策树、恢复执行器、清理执行器、降级旁路。
- 讲什么:K-230~K-239。
- 不讲什么:控制入口(15);心跳触发(07);abort_update_proc 机制(18,单函数豁免);RS 自身 rollback(20)。
- 前置:00-13 强依赖(11 的 clone/update_service、12 的 unpublish、13 的 run_service)。
- 后置:18(abort 被引用)、20(cleanup 被自升级消费)。
- 事实底线:manager.c:360-530/1033-1353、const.h:27。
- 知识点清单:K-230~K-239(S,旧15)。
- 验收标准:①决策树图(30+ 分支压缩为决策点+行为臂);②terminate_decision 纯决策+Effects 执行体(Fix #78/#87/#88)对照;③两段式 cleanup 的时序(标记 DEAD→idle_period 真清理);④detach 的 MAX_DET_RESTART 与重发布;⑤core 服务死亡→RS _exit(1) 的陪葬语义。

### 18-rs-live-update

- 一句话定位:不停机的服务替换——LU 编排状态机全程。
- 讲什么:K-250~K-259(+可选 K-260)。
- 不讲什么:update_service 原语(11,回指);init 协议(13);状态数据机制(19,只留编排调用点);RS 自身特例(20);SEF 框架(04)。
- 前置:00-17(强依赖 11/13/16/17)。
- 后置:19/20。
- 事实底线:update.c 全文(1011 行;262-327 已迁 11)、request.c:534-889、utility.c:247-307。
- 知识点清单:K-250~K-258(S,旧16,其中 K-161 已迁出)|K-259(S,通用半)|K-260(N,可选)。
- 验收标准:①四阶段状态机图(prepare→update→init→end/rollback);②rupdate 链与 curr 推进图;③rs_receive_ticks 双源(IPC filter+CLOCK)超时机制(A-9);④end_update 五相位族+prepare-only 取消;⑤UpdatePhase 类型化对照(A-6)与 Fix #91/#92 注记;⑥(可选)Redox init 对照节。

### 19-rs-state-data

- 一句话定位:把旧实例的"记忆"交给新版本——rs_state_data 三载体与迁移机制。
- 讲什么:K-270~K-275。
- 不讲什么:LU 编排(18);SEF_LU 状态框架半(04);cpf 内核机制(01-stage-kernel);RS 自升级 cpf_reload(20)。
- 前置:00、01、02、04、18。
- 后置:20(cpf_reload 语境)、21(契约)。
- 事实底线:manager.c:174-288、rs.h(rs_state_data)、ipc_filter.h、type.h:38(prepare_state_data)。
- 知识点清单:K-270~K-275(S,旧17;K-275 主讲述点在新 21,本篇引数据形状)。
- 验收标准:①三载体表(eval/IPC filter/整包 grant);②init_state_data 逐分支(SEF_LU_STATE_*+strtol+VM 保底条目);③A-14 三项安全默认(56B/Endpoint::NONE/空 label ESRCH)逐条对照;④**结构修复**:单一"## 3"标题(消除旧17 重复缺陷 Y-3),R13 接线注记按内容归位。

### 20-rs-self-lifecycle

- 一句话定位:RS 自己怎么更新自己——四个自指特例与自模拟 response。
- 讲什么:K-280~K-286。
- 不讲什么:通用 LU(18);SEF 框架(04,只讲 RS 侧回调体);create 原语(11,回指)。
- 前置:00-18(强依赖 04/11/13/18)。
- 后置:21(契约)。
- 事实底线:main.c:436-626(直读全文)、update.c:330-366、utility.c:387-412、manager.c:760-780。
- 知识点清单:K-280~K-286(S,旧18+旧01§2.3.7+旧12 部分;旧18§2.9b 内容按 Y-2 吸收)。
- 验收标准:①boot 自升级成为唯一主讲述点(新 05 只留指针);②四特例表(自升级/restart init/LU init/rollback)各带 C 锚点;③自模拟 response 双回调(init_response/lu_response)从旧 01/12 迁入;④RestartCb 重绑建模(Rs/Stateful)含"重绑先于流程、失败不回滚"断言;⑤§2 无 Fix 日志残留(修复史在 todo.md)。

### 21-rs-external-interfaces

- 一句话定位:RS 的全部外部依赖契约——五域 trait 面、syslib 签名、消息 wire、构建与接线状态。
- 讲什么:K-290~K-296(+K-292 迁入)。
- 不讲什么:各机制语义(01-20 对应篇);内核/PM/VM 侧实现(对端 stage)。
- 前置:00(可跳读;签名表不依赖机制篇,但消费它需先读对应篇)。
- 后置:无(终端篇;被全 stage 引用)。
- 事实底线:lib/libsys/*(相关文件)、include/minix/{com,ipc,rs,sef}.h、os/libs/minix-sys、minix-types::ipc::rs、edge_todo E9 进度注记、minix/servers/rs/Makefile、getuptime.c:9-23。
- 知识点清单:K-290~K-296(S,旧19+旧01§3.5)|K-292(S,旧01§3.5 迁入,Y-1)|K-294(N,E9 状态节)|K-295(S,Fix #93)。
- 验收标准:①五域 trait 面定义(SysApi/SchedApi/PmApi/VmApi/IpcApi+supertrait 组合)完整迁入;②签名表按"函数→传输目标域→消息槽"三列;③E9 分域状态表(SysApi✅/PmApi✅/SchedApi-KERNEL✅/VmApi 待/SCHED 待/RS 换装域内)带 edge_todo 锚点;④LWT 三实例与 72B pinning 后记保留;⑤构建节(USE_PCI/USE_LIVEUPDATE→feature 映射 A-10/A-11)。

---

## 6. 变更表

| 操作号 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向/来源 |
|--------|------|--------|--------|------|-----------|----------|
| C-01 | 重排 | 旧99 | 新01 | 词典前置消除一类前向引用(序差 D-5) | K-010~K-022 | 整篇迁 |
| C-02 | 原样迁 | 旧02 | 新02 | 数据底账位置提前一位(boot 之后)——实际编号不变仅顺延 | K-030~K-043 | 整篇迁 |
| C-03 | 原样迁+吸收 | 旧03+旧05§2 原语半 | 新03 | fill 原语是 priv 的填充器,boot(新05)后向引用(序差 D-3) | K-050~K-060、K-053 | 合并迁 |
| C-04 | **新建** | 旧01§1.3/§2.2/§3.3+旧12/18 散落+libsys sef*.c | 新04 | SEF 框架无专篇(N-1);消除 01 过载 | K-070~K-078 | 新建(来源已列) |
| C-05 | 拆分瘦身 | 旧01(1044 行) | 新05 | 剥离 SEF 框架(→04)/KernelApi(→21)/自升级详篇(→20)/catch(→13);boot 回归单语义 | K-080~K-090、K-040、K-044 | 逐节去向见 §8.1 |
| C-06 | 原样迁 | 旧06 | 新06 | 运行时骨架位置保持 | K-100~K-109 | 整篇迁 |
| C-07 | 原样迁 | 旧07 | 新07 | 同上 | K-110~K-118 | 整篇迁 |
| C-08 | 重排 | 旧05(余部) | 新08 | 拓扑计算靠近消费方(09 init_slot) | K-120~K-125 | 整篇迁 |
| C-09 | 原样迁+扩 | 旧08 | 新09 | 配置管线位置顺延+新增声明语言契约节(N-2) | K-130~K-139 | 迁+扩 |
| C-10 | 原样迁 | 旧09 | 新10 | 顺延 | K-140~K-147 | 整篇迁 |
| C-11 | 合并 | 旧10+旧16§update_service | 新11 | update_service 是实例原语非 LU 编排(序差 D-2,update.c:262-327 直读证实) | K-150~K-162 | 合并迁 |
| C-12 | 原样迁 | 旧11 | 新12 | 顺延 | K-170~K-174 | 整篇迁 |
| C-13 | 合并 | 旧12+旧01§2.5(catch) | 新13 | ready 协议与 catch 是同一协议两半(K-087 主讲述点迁移) | K-180~K-190 | 合并迁 |
| C-14 | 重排 | 旧04 | 新14 | 授权门贴近消费方(15/16/18),消除"主循环前讲请求"倒置(序差 D-4) | K-200~K-204 | 整篇迁 |
| C-15 | 原样迁 | 旧13 | 新15 | 顺延 | K-210~K-218 | 整篇迁 |
| C-16 | 原样迁+改写边界节 | 旧14 | 新16 | 顺延+打印族裁决 Y-4 落地+IS/procfs 归属指针(N-4) | K-220~K-225、K-297 | 迁+改 |
| C-17 | 原样迁 | 旧15 | 新17 | 顺延 | K-230~K-239 | 整篇迁 |
| C-18 | 原样迁-单节 | 旧16(-update_service) | 新18 | 顺延;update_service 迁出(序差 D-2) | K-250~K-259 | 迁 |
| C-19 | 结构修复迁 | 旧17 | 新19 | 顺延+修重复标题(Y-3) | K-270~K-275 | 迁+修 |
| C-20 | 吸收修复日志迁 | 旧18 | 新20 | 顺延+§2.9b 按内容归位(Y-2);自升级成为唯一主讲述点 | K-280~K-286 | 迁+吸 |
| C-21 | 合并 | 旧19+旧01§3.5 | 新21 | 五域 trait 面归契约篇(Y-1);新增 E9 状态节(N-3) | K-290~K-296 | 合并迁+扩 |
| C-22 | 重建 | 旧00 | 新00 | 导航表按新目录重建,修陈旧行(N-5/Y-5) | K-001~K-007 | 重写 |
| C-23 | 归档 | 旧 21 篇全部 | `archive`(B 相执行) | 重建以新目录为坐标,旧文档只作知识来源 | — | B 相归档不删 |

---

## 7. 缺漏新篇落实(非 C 主题与缺口逐项)

| 缺口 | 主题 | 为什么重要 | 原料 | 落实 | 验收 |
|------|------|-----------|------|------|------|
| N-1 | SEF 框架 | 全部服务器的初始化/重启/更新骨架;"谁调用 sef_cb_init_fresh"是 boot 叙事的缺失前提 | lib/libsys/sef*.c(约 2210 行)+main.c:136-153 | 新 04(完整契约见 §5) | §5 新 04 验收五条 |
| N-2 | system.conf 声明语言+触发链 | rs_start 的生产侧;没有它,"服务从哪来"只有半边 | minix-service/parse.c、etc/system.conf、rc.minix:49、system.conf.5 | 新 09 §声明语言契约 | 八指令映射表+触发链一节 |
| N-3 | E9 接线状态 | wire-up 施工的唯一进度权威,文档不更新会误导接线者 | edge_todo.md E9 三条进度注记+三个 commit 号 | 新 21 §状态节 | 分域状态表带锚 |
| N-4 | 外部观测面归属 | 打印族归属冲突(Y-4)悬置会再次产生漂移 | utility.c 打印族+servers/is/dmp_rs.c+fs/procfs/service.c | 新 16 §边界节 | 双侧锚点+裁决成文 |
| N-5/Y-5 | 00 导航漂移修复 | 导航是入口,错行直接误导读者 | 新目录 §4.1 | C-22 | 导航行=新篇头部声明 |
| N-6 | Redox 监督趋同对照 | 教学价值(LU/恢复设计的行业对照) | todo §6.3(外部链接锚点) | 新 18 可选节 | 有锚即可,非强制 |
| 否决项 | minix-service 命令实现详篇 | 命令本体属 18-stage-commands;RS 只需格式契约 | — | 不新建,新 09 指针 | — |
| 否决项 | Makefile 详篇 | 构建非语义(plan WONTFIX);升格为新 21 一节即可 | minix/servers/rs/Makefile | 不新建 | — |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表(按旧文档逐篇;节级)

| 旧位置 | 旧内容(一句话) | 新位置 | 迁移类型 | 断链风险 |
|--------|----------------|--------|---------|---------|
| 00 §1-§3 | RS 身份/两序/职责/生命周期次主线 | 新00 §1-§3 | 改写 | 低(自包含) |
| 00 §4.0 导航表 | 21 篇导航(17/18 行陈旧) | 新00 §4 | 重写 | 低 |
| 00 §5-§6 | 原则/ARCH/覆盖/路线 | 新00 §5-§6 | 改写(编号更新) | 低 |
| 01 §1.3/§2.2/§3.3 | SEF 概念/回调注册/SefCallbacks | 新04 | 原样搬移+框架补全 | 中(01 被引 23 处需改指 04 或 05) |
| 01 §2.1-§2.6 | main/预备/四步/四表查找/catch/get_work | 新05 §2(catch 移新13) | 改写 | 中 |
| 01 §2.7 | table.c 三表 | 新05 | 原样搬移 | 低 |
| 01 §2.3.7 | boot 自升级 | 新20(主)+新05(位置) | 拆分 | 中(与新20 去重) |
| 01 §3.1/§3.2/§3.4 | BootTables/lookup Result 化/BootInit | 新05 §3 | 原样搬移 | 低 |
| 01 §3.5 | KernelApi 五域面 | 新21 | **整节迁移** | 高(代码注释 7 处引用 01 需改指 21) |
| 01 §3.6-§3.8 | LU feature/rinit grant/主循环骨架类型化 | 新05 §3(骨架类型化移新06) | 拆分 | 低 |
| 01 §4-§5 | 实现/测试 | 新05 §4-§5(骨架类型化测试随新06) | 拆分 | 低 |
| 02 全篇 | 登记表 | 新02 | 原样搬移 | 低(被引 32 处按新号替换) |
| 03 全篇 | 权限 | 新03 | 原样搬移+原语吸收 | 低 |
| 04 全篇 | 访问控制 | 新14 | 原样搬移 | 低(后置引用全部后向) |
| 05 §2 原语半 | fill_send_mask/fill_call_mask | 新03 | 拆分 | 中(旧03 引用改指新03) |
| 05 余部 | IPC 掩码 | 新08 | 原样搬移 | 低 |
| 06 全篇 | 主循环 | 新06 | 原样搬移 | 低 |
| 07 全篇 | 心跳 | 新07 | 原样搬移 | 低 |
| 08 全篇+新增 | 配置管线+声明语言 | 新09 | 搬移+扩 | 低 |
| 09 全篇 | exec | 新10 | 原样搬移 | 低 |
| 10 全篇 | 创建 | 新11 | 原样搬移 | 低 |
| 11 全篇 | 发布 | 新12 | 原样搬移 | 低 |
| 12 全篇 | init-run | 新13(+吸收旧01§2.5) | 合并 | 中 |
| 13 全篇 | 控制请求 | 新15 | 原样搬移 | 低 |
| 14 §2/§3 | 查询+打印族 | 新16(边界节按 Y-4 重写) | 改写 | 中(与 todo R31 表述对齐) |
| 15 全篇 | 终止恢复 | 新17 | 原样搬移 | 低 |
| 16 §update_service | 实例替换原语 | 新11 | 拆分 | 高(代码注释 8 处引 16 的 update_service 段需改指 11) |
| 16 余部 | LU 状态机 | 新18 | 原样搬移 | 低 |
| 17 全篇 | 状态数据 | 新19(修重复标题 Y-3) | 修后迁 | 低 |
| 18 §2.9b | Fix #59/#63 日志 | 新20 §2 相应小节 | 吸收 | 低 |
| 18 余部 | 自身生命周期 | 新20 | 原样搬移(自升级为主讲述点) | 低 |
| 19 全篇 | 外部接口 | 新21(+吸收旧01§3.5,新增 E9 节) | 合并+扩 | 低 |
| 99 全篇 | 词典 | 新01 | 原样搬移(位置前移) | 低(被引 5 处) |

### 8.2 引用迁移表(按引用类)

| 引用类 | 总量 | 热点 | 迁移方式 | 验证 |
|--------|------|------|---------|------|
| stage 内文档互引(`NN-rs-*.md`) | 约 386 处 | 旧16(38)/旧02(32)/旧08(27)/旧19(25) | B 相按 §8.1 映射逐篇重写时替换;旧→新映射表机械执行(见下) | `grep -ohE '[0-9]{2}-rs-[a-z-]+\.md'` 复扫为零旧号 |
| 代码注释引用(`os/servers/rs/src/*.rs`) | 约 138 处 | 19(22)/03(14)/02(14)/12(12) | B 相每篇完成后批量 sed 对应模块;或最后统一一轮(映射为纯函数表) | `rg "0[0-9]-rs-|1[0-9]-rs-" os/servers/rs/src` 复扫;含既有断链 trap_api.rs:364(`10-rs-service-convert.md`→新11) |
| 外部 stage 文档引用 03-stage-rs | 约 15 个正式文件(edge3、edge_todo、00-master-plan/README、02/04/07/09/10/12/17-stage 各 plan、07-stage-ds/06、04-stage-pm/08、13-stage-ipc/10) | — | 不在 B 相本 stage 范围内强改;登记为 edge 条目"03-stage-rs 重编号后外部引用批量迁移",按 §8.1 映射表一次性 sed | `grep -rl "03-stage-rs" fork-syscall-rewrite --include=*.md` 复扫 |
| 中间产物目录内引用 | 按项目规范不入统计、不迁移(随重建自然过期) | — | 不迁移 | 不适用 |

**旧→新编号映射(机械执行表)**:
```
00→00  99→01  02→02  03→03  01→05(boot 部)  06→06  07→07  05→08  08→09
09→10  10→11  11→12  12→13  04→14  13→15  14→16  15→17  16→18(update_service 节→11)
17→19  18→20  19→21
```
语义歧义处理:引用 `01-rs-boot-init.md` 且上下文为 SEF 框架/KernelApi/catch/自升级的,分别改指 `04/21/13/20`(B 相按 §8.1 节级表判定,不许盲替);引用 `16-rs-live-update.md` 且上下文为 update_service/swap/activate 的,改指 `11`。

### 8.3 断链成本摘要

- **受影响引用总数**:约 386(stage 内文档)+约 138(代码注释)+约 15 个外部文件(每文件 1~N 处)≈ **540 处量级**。
- **热点文件**:文档侧旧16/旧02/旧08/旧19;代码侧 `lib.rs`、`boot.rs`、`trap_api.rs`、`shell_request.rs`(按 todo 注记密度推断,B 相以 rg 实测);外部侧 `edge_todo.md`、`02-stage-vm/00-vm-overview.md`、各 stage plan.md。
- **既有断链(重建顺带修复)**:`os/servers/rs/src/trap_api.rs:364` 引用不存在的 `10-rs-service-convert.md` → 新 11。
- **建议批量修改方式**:①B 相写每篇新正文时,该篇引用全部用新号(源头零旧号);②代码注释分两轮——先按映射表 sed 全量替换,再人工核对 §8.2 的语义歧义两类(01→04/21/13/20、16→11);③外部 stage 引用登记 edge,统一轮次 sed;④每轮跑 `rg` 复扫旧号归零。
- **成本判断**:断链成本集中且可机械执行(约 540 处,映射表完备),显著低于"在 21 篇旧结构内互相搬移平衡边界"的关系网代价;符合 R 相"先算清成本再重建"的要求。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**:§4.1 新目录 22 篇,逐篇核对契约"前置"字段,全部指向更小编号——**通过**。存留的"导航指针"(非前置依赖)共 4 类,已在 §1.6 序差 D-7/D-8 与 §5 各契约"不讲什么"声明为豁免类:①新06 dispatch 表指向 07/13/15/16/18(handler 名);②新07 update_period/sigchld 指向 18(单函数);③新12 publish 失败指向 17(kill_service 单函数);④新13 end_srv_init 指向 17/18(单函数);⑤各篇→21(签名契约,终端篇性质同工具书);⑥新05→20(自升级位置指针)、新05→13(rproctab_gid 消费点)、新18→19(相邻下一篇)。以上每处都是单行指针,不展开机制,与旧版的实质差异:旧版同类豁免 9 处+01 号整篇骨架豁免,新版全部为单点且集中可数。
2. **依赖关系图检查**:前置关系构图(DAG):00→01→02→03→04→05→{06,07}→08→09→10→11→12→13→14→15→16→17→18→19→20→21,线性无环;跳读路径仅经"前置"字段回指更早篇——**通过,无环,无需拆解**。
3. **覆盖率检查**:§2.1 池 110 条,每条"去向"列非空(新编号+小节角色);明确删除项:**无**(旧 plan WONTFIX 项 Makefile 降为新 21 一节,非删除)。新增 8 条(其中 K-260 为可选教学对照)均有 C 制品或边界材料锚点——**通过**。
4. **断链成本统计**:§8.3,约 540 处,热点与批量方式已列——**完成**。

### 9.2 自检门逐门结果

| 门 | 结果 | 证据 |
|----|------|------|
| G1 C 真序逐条可核对 | **通过** | §1.2-§1.4 表全部锚点来自本次直读(main.c/utility.c/request.c/update.c/table.c/type.h/const.h/sef.c/sef_init.c 抽样)。抽十核对:B-05(main.c:185 grant)、B-10d(285-291 例外)、B-11c(386-398 SYNCH_BOOT)、B-15(441-489 自升级)、L-04(80-84 CLOCK)、L-07(125 EDONTREPLY)、U-09(manager.c:328 end_srv_init)、V-04(update.c:532 start_update)、V-08(request.c:950-954 LU 门)、1.5 节(sef_init.c:98,427)——全部与直读文本一致 |
| G2 知识点池完整 | **通过** | 8 个 .c 121 函数按 plan §7.2 基线+本次函数面复核全部落入池(K 簇覆盖);6 个 stage 内头文件全入池;非 C 制品 10 项清单 §3.5 逐项回答;3 个待验证项显式标注(static_priv_id 定义位置、init_privs 定义精确行、manager.c 1800 区段精确行) |
| G3 前向引用为零 | **通过(带声明豁免类)** | 见 §9.1 检查 1;豁免均为单行指针类且逐处列出 |
| G4 依赖图无环 | **通过** | 线性主链,无环;C 代码固有的 init_service↔LU 互相调用以"机制在前/编排在后+指针类"拆解(序差 D-7) |
| G5 覆盖率 100% | **通过** | 池每条有去向;新增条目 8 条均有锚点;删除项为零 |
| G6 拆合去向/新建来源 | **通过** | 抽查十处:C-03(原语半去向新03)、C-04(新建来源四路齐列)、C-05(01 六个拆出去向逐节列于 §8.1)、C-11(旧16 节去向新11)、C-13(旧01§2.5 去向新13)、C-14(整篇去向新14)、C-16(改写依据 Y-4)、C-21(旧01§3.5 来源)、C-22(重建依据 N-5)、K-294(新增来源 edge_todo)——全部两向齐备 |
| G7 契约七要素 | **通过** | §5 共 22 份契约,每份含定位/讲什么/不讲什么/前置/后置/事实底线/知识点清单+验收标准(逐份核对无缺) |
| G8 迁移表覆盖 | **通过** | §8.1 覆盖旧 21 篇全部(每篇至少一行,重点篇节级);§8.2 覆盖文档互引/代码注释/外部引用三类 |
| G9 断言带锚点 | **通过(3 项显式待验证)** | 全报告锚点为直读或 grep 产物;推测/待验证共 3 处,均已原地标注:①static_priv_id 定义位置(§1.2 B-10a);②init_privs 定义精确行(§2.1 K-124);③manager.c swap_slot 区段精确行(§5 新11 事实底线) |

### 9.3 结论与待用户裁决的问题

**结论**:蓝图完成。新目录 22 篇(00-21),相对旧目录的实质变化五点:①SEF 框架独立成篇(新04,修复 01 号过载与框架知识无主);②词典前移(新01,机械消除位值前向引用);③访问控制后移至命令面门前(新14,消除主循环前讲请求的因果倒置);④update_service 迁归实例原语篇(新11,消除 15→16/20→16 前向);⑤补齐四个缺口(SEF 框架、system.conf 声明语言、E9 接线状态、打印族归属裁决)并修复五处既有结构缺陷(00 导航陈旧、17 重复标题、18 Fix 日志混入、01/14 归属错位、trap_api 断链)。

**待用户裁决**:
1. 词典位置(新01 在最前)与替代方案(维持旧 99 位置在末尾+词典型豁免)二选一——本蓝图推荐前置,若汇总收敛选择后者,仅需将 C-01 改为"原位保留",其余设计不受影响。
2. K-260(Redox 对照)为可选教学增强,是否纳入 B 相由汇总裁决。
3. 外部 stage 引用迁移(约 15 文件)按本蓝图登记 edge、独立轮次执行;若用户希望 B 相一并完成,工期另计。

---

## 附:执行头部回执

```text
your_name(AI agent name) = glm
target_dir(关注的工作目录) = 03-stage-rs
repo_root(仓库根目录) = /home/xzhao/github/minix-rs
任务 = R 相·重建蓝图:输出 03-stage-rs/doc_rerank_glm.md,不改任何正文 ✅
约束遵守 = 未引用中间产物目录(.design/ 与 tmp_design_and_todo/);仅写入本文件(带 _glm 后缀);
           未读取任何其它 AI 的 doc_rerank_* 产物(范围声明见 §0.1)。
```
