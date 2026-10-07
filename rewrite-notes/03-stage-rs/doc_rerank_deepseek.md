# 03-stage-rs 文档重建蓝图（deepseek）

## 0. 元数据

```text
your_name(AI agent name) = deepseek
target_dir(关注的工作目录) = rewrite-notes/03-stage-rs
repo_root(仓库根目录) = /home/xzhao/github/minix-rs
当前提交号 = 7bc7f0c219c4a1bca7845ffe109af0efb680f06c（2026-09-19）
任务 = R 相·重建蓝图：只产出本文件，不改任何正文。
本轮修订 = 2026-09-19（补做轮）：订正 `lib/libsys/sched_start.c:50-103` 两处锚点为 `45-98`（该文件共 98 行，`sched_start` 定义在 45–98 行）。
```

### 0.1 审查范围

- **范围内文档**：`03-stage-rs/` 下全部 21 个编号文档，即 `00`–`19` 共 20 篇编号文档与 `99-rs-global-concepts.md` 词典 1 篇（实际文件与行数见 §0.3）。
- **范围内代码**：`minix3/minix/servers/rs/`（8 个 `.c`、6 个头文件，6307 行）、RS 协议面头文件 `minix3/minix/include/minix/{com,rs,sef,ipc_filter}.h`、RS 依赖的 SEF 运行库 `minix3/minix/lib/libsys/sef*.c`、`minix3/minix/lib/libexec/`、内核 boot 表 `minix3/minix/kernel/table.c` 与 `minix3/minix/kernel/main.c`、以及 `os/servers/rs/`（29 个模块，22348 行）。
- **范围内非 C 制品**：`minix3/etc/system.conf`（全部 529 行）、服务声明客户端 `minix3/minix/commands/minix-service/`（`parse.c`/`minix-service.c`/`config.h`）、RS 的 C 构建文件 `minix3/minix/servers/rs/Makefile`、`minix3/share/mk/bsd.own.mk:1499`（`USE_LIVEUPDATE` 默认值）、Rust 构建文件 `os/servers/rs/Cargo.toml`（`live-update` feature）、测试基建 `os/qemu-tests/`、`minix3/minix/tests/`。
- **边界材料**：`../00-master-plan/README.md`（阶段划分与启动因果链）、`../edge_todo.md`（E-RSWIRE、E-RSSTART 等跨阶段条目）、`../02-stage-vm/00-vm-overview.md`、`../02-stage-vm/25-rs-services.md`（VM 侧 RS 服务契约）、`../01-stage-kernel/` 的 `06-proc-init-boot-proc.md`、`09-vm-boot-protocol.md`、`22-privilege.md`、`23-ipc-filter.md`（前序阶段已讲概念，来源为旧文档 00 §4.2 的引用表，本次只作边界确认，不逐篇精读）。
- **参考材料（不算正式文档）**：`plan.md`（369 行）、`todo.md`（580 行）、`draft/README.md`、本目录的 `checklist.md`（若存在）。它们在报告中只作为线索，结论全部另行取证。
- **明确范围外**：目标目录下的隐藏设计目录与临时设计目录（项目规范：中间产物，不读取、不引用）、`target_dir/doc_rerank_*.md`（其它 AI 的 R 相产物，未读取；本次执行期间出现的同名文件一律不引用）、`minix3/` 与原文件无关的第三方子树、运行中的其它 stage 目录正文。

### 0.2 读取清单

| 类别 | 对象 | 读取方式 |
|------|------|---------|
| 旧文档 | 21 个编号文档全部小节标题 + 头部声明 + 正文（逐篇） | 全文精读或等价抽取（00–19、99 逐篇；本次通过 4 个并行抽取任务覆盖全文并抽查回读） |
| C 源码 | `main.c`(834)、`manager.c`(2332)、`request.c`(1309)、`update.c`(1011)、`utility.c`(547)、`exec.c`(165)、`error.c`(59)、`table.c`(50) | 全文精读 |
| C 头文件 | `servers/rs/{const,glo,type,proto,inc}.h`、`include/minix/{com,rs,sef,ipc_filter,priv}.h` | 全文精读关键段 |
| 非 C 制品 | `minix3/etc/system.conf`、`commands/minix-service/{parse.c,minix-service.c}`、`servers/rs/Makefile`、`os/servers/rs/Cargo.toml`、`os/qemu-tests/*`、`minix3/minix/tests/` | 定向读取 |
| Rust | `os/servers/rs/src/` 全部 29 个模块（模块头与公开面盘点）、`os/libs/minix-types` 相关模块 | 盘点 + 抽查 |
| 边界 | master-plan README、edge_todo、02-stage-vm 概览与 25-rs-services 标题结构 | 定向读取 |

### 0.3 使用的命令与关键输出（证据摘录）

```text
$ wc -l minix3/minix/servers/rs/*.c
  59 error.c   165 exec.c   834 main.c   2332 manager.c
1309 request.c   50 table.c  1011 update.c   547 utility.c   （合计 6307）

$ ls 03-stage-rs/[0-9]*.md | wc -l     → 21（00–19 共 20 篇 + 99）
$ wc -l 03-stage-rs/[0-9]*.md          → 最小 15088 字节（11），最大 72897 字节（01）

# 旧文档之间的文件名引用（每目标计数，前 8）
16-rs-live-update.md 42；02-rs-process-table.md 34；19-rs-external-interfaces.md 32；
08-rs-slot-config.md 29；01-rs-boot-init.md 26；15-rs-terminate-restart.md 25；
10-rs-service-create.md 25；06-rs-main-loop.md 24；全部文件名引用合计 ≈ 429 处

# 仓库其它位置引用 RS 文档
含 `NN-rs-*.md` 或 `03-stage-rs` 的 .md 文件 49 个（含 plan/todo/其它 stage 的 plan 与交互篇）
含引用的 .rs 文件 32 个（模块头注释；boot.rs 23 处、sef.rs 16 处、process_table.rs 9 处）
热点：06-stage-sched（14-rs-interaction 5、plan 4、13-pm-interaction 3、其它 5）、
      14-stage-runtime、08-stage-is、11-stage-devman、07-stage-ds、edge_todo.md

# 违规引用：`03-stage-rs` 正文引用隐藏设计目录共 6 处
  02-rs-process-table.md ×3、03-rs-privilege.md ×1、00-rs-overview.md ×1（另有 01-stage-kernel/
  doc_rerank_deepseek.md 等其它 AI 产物引用 1 处，不属本 stage，迁移时忽略）

# RS 在 boot 链中的位置
minix3/minix/kernel/table.c:44-64   boot_image 登记顺序（DS 在 52 行、RS 在 53 行）
minix3/minix/kernel/main.c:196,257,265  仅 VM/RS 立即可调度，其余 RTS_VMINHIBIT
minix3/minix/include/minix/com.h:61,77  RS_PROC_NR=2、ROOT_SYS_PROC_NR=RS_PROC_NR
```

### 0.4 本次执行对旧文档的整体判断（结论先行）

旧目录（21 篇，含总览与词典）不是"有若干 bug 的文档集"，而是**一条被机制文档多次切断的学习路径**：

1. **启动链被切成三段**：`01`（boot）→ `02/03/04/05`（表、权限、访问、掩码）→ `06`（主循环）→ `12`（ready 协议）。读者沿 boot 链读到 Step 2 时，ready 协议在 11 篇之后；读 `01` 的 Step 1 时，权限与掩码的完整定义在其后的 `03/05`，于是 `01` 自己重述了一遍机制，形成全目录最大的重复源。
2. **服务生命周期顺序倒置**：监控（`07`）排在配置（`08`）、映像（`09`）、创建（`10`）、发布（`11`）、ready（`12`）之前；`07` 的崩溃分支调用 `15` 的机制，`15` 的回调又引用 `16` 的状态机。
3. **机制文档与引用方互为前后**：`02` 的 `r_priv` 字段归 `03`、`ipc_list` 归 `05`、`r_upd` 归 `16`；`16` 的 state data 调用归 `17`；`18` 的 boot 自升级归 `01`。这些"归属声明"在旧目录内部是自洽的，但对第一次阅读的读者是反复跳转。
4. **一个完整的非 C 主题整块缺失**：`system.conf` 服务声明文件与 `minix-service` 客户端（谁、以什么格式、怎样把 `rs_start` 递给 RS）在旧目录里只有 `08` 的两处脚注级锚点；boot 服务配置（编译期表 + `SRV_*` 默认值）与动态服务配置（`system.conf`）的**双源事实**没有任何一篇讲清。
5. **测试与 Rust 实现状态的叙述大量附着在正文里**（"Fix #NN""R4/R5/A4""接线落地"块），使文档随实现进度快速腐化：本次抽查即发现旧文档 12/16 引用的 `should_reply_ready` 等函数已被删除、01/04 的测试计数互相矛盾、多处 `manager.c:rproc（Lxxx，工具生成）` 符号名错误。

因此本蓝图采用**重建**而不是搬移：以 §4 的新目录为坐标，旧文档仅作为知识点来源（§2），每篇新文档的边界与验收在 §5 一次写清。

---

## 1. C 真序

### 1.0 阶段类型判定

RS 同时具备三种特征，按以下方式处理（R 提示词 §9）：

| 特征 | 证据 | 处理方式 |
|------|------|---------|
| **服务事件循环型**（主） | `main.c:57-130` 永不终止的 while 循环；消息分类后分派 | 运行时以"主循环 + 一次请求的生命周期"为主线（新 `09`/`10`/`14`/`15`） |
| **启动链型** | `_start → sef_startup → sef_cb_init_fresh` 四步（`main.c:158-494`），线性、一次性 | 单独成篇（新 `09`），严格执行序；前置机制全部前移 |
| **集合型** | 15 个 `RS_*` 消息、4 条信号分支、5 类外部依赖面 | 按"公共骨架 + 触发时机 + 分组表"组织（新 `10`/`14`/`15`/`18`），不伪造线性序 |

RS 与普通事件循环服务的关键差别是**它自己也在 boot 链里**：它是第二个运行的用户服务（`kernel/table.c:53`），却负责启动其余服务。因此本 stage 的主线是"RS 的启动链（含它替所有 boot 服务做的初始化）"，次主线是"一个被管理服务的生命周期"。

### 1.1 真序表

> 说明：这张表是后续《序差表》（§4.4）与契约事实底线的来源。表内每一步给出可核对的 C 锚点；外部前置步（E 组）由前序 stage 承担，本 stage 只引用其结论。

#### A 组：RS 进程的诞生（外部前置，kernel/VM/SEF 侧）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| A1 | boot 循环只让内核任务、RS、VM 立即可调度，其余进程挂 `RTS_VMINHIBIT` | `kernel/main.c:196`、`:265` | 执行顺序：kernel → VM → RS → 其余 |
| A2 | VM 以 `ptproc` 先运行，为 RS 等建页表并解除抑制 | `kernel/main.c:257`、`01-stage-kernel/09-vm-boot-protocol.md` | 前序 stage 结论 |
| A3 | `boot_image[]` 中 DS 先于 RS 登记（NOTIFY 投递优先级） | `kernel/table.c:36-40,52-53` | 登记顺序 ≠ 执行顺序 |
| A4 | RS 被调度后进入 SEF 运行库的 `sef_startup()`；SEF 拦截 init/ping/LU/signal 请求 | `lib/libsys/sef.c:185-230`、`sef_init.c` | RS 是 SEF 的 provider 与 consumer 双重身份 |
| A5 | 首次 init 类型为 `SEF_INIT_FRESH`，SEF 调用 RS 注册的 `sef_cb_init_fresh` | `sef_init.c`（`do_sef_init_request`）、`main.c:139` | 其余两种类型见 A 组末 |
| A6 | `main()` 先注册 7 个 SEF 回调再进入循环 | `main.c:50-51,136-153` | 回调注册早于任何服务逻辑 |
| A7 | `sys_getmachine(&machine)` | `main.c:53` | 机器信息（CPU 数/BSP id），`check_request` 消费 |
| A8 | 换一种 init 类型重入：`sef_cb_init_restart`（RS 被重启）或 `sef_cb_init_lu`（RS 被热更新） | `main.c:499-544`、`main.c:549-586` | 与 fresh 同级的入口，运行时序上发生在系统运行期 |

#### B 组：`sef_cb_init_fresh` 预备（`main.c:158-237`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| B1 | `env_parse("rs_verbose")` | `main.c:179` | 调试开关 |
| B2 | `sys_getinfo(GET_HZ)` → `system_hz` | `main.c:181` | 一切时间常数的基数（`RS_INIT_T`/`RS_DELTA_T`） |
| B3 | `cpf_grant_direct(ANY, rprocpub)` → `rinit.rproctab_gid` | `main.c:185-189` | 公开表授权；VM 在启动握手时消费 |
| B4 | `RUPDATE_INIT()`；`shutting_down = FALSE` | `main.c:192-193` | 全局状态复位 |
| B5 | `sys_getimage(image)` 取 boot image 表副本 | `main.c:196` | Step 1 的输入之一 |
| B6 | 数出 boot image 中系统服务数并与 priv 表条数交叉校验，不符即 panic | `main.c:200-227` | boot 表完整性闸门 |
| B7 | 复位 `rproc[]`/`rprocpub[]`：清标志、`r_init_err=ERESTART`、`in_use=FALSE`、端点清 `NONE` | `main.c:229-237` | 运行前表状态 |

#### C 组：Step 1 — 为每个 boot 服务建槽并装配权限（`main.c:244-346`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| C1 | 按 `boot_image_priv_table[]` 顺序遍历，跳过内核任务 | `main.c:244-250`、`table.c:15-30` | 表序 = 权限装配序 |
| C2 | `boot_image_info_lookup` 四表查找（image/priv/sys/dev，后两表带默认项） | `main.c:252-254`、`main.c:709-779` | NULL 与 DEFAULT 两种哨兵语义 |
| C3 | 写 label、静态 priv id（`static_priv_id`） | `main.c:262-266` | 静态 id 供内核 send mask 位号使用 |
| C4 | `s_flags` ← boot 表 flags；`s_init_flags`/`s_trap_mask`/`s_ipc_to`/`s_sig_mgr`/`s_bak_sig_mgr` ← `SRV_OR_USR` 默认 | `main.c:269-275`、`priv.h:45-100` | `s_ipc_to` 走 `fill_send_mask(..., ALL_M)` 快捷路径 |
| C5 | 内核调用掩码 `s_k_call_mask`：`ALL_C` 或空集 | `main.c:277-280`、`utility.c:100-137` | `fill_call_mask` |
| C6 | `sys_privctl(SET_SYS)` 提交（RS/VM 例外，已在运行）；随后 `sys_getpriv` 回读同步 | `main.c:282-296` | RS/VM 跳过提交但一样回读 |
| C7 | `sys_flags` ← sys 表；`dev_nr` ← dev 表 | `main.c:301-306` | 驱动属性 |
| C8 | `r_cmd` ← `proc_name`；`r_script` 清空；`build_cmd_dep` 解析 argv；`rpub->proc_name` | `main.c:308-313`、`manager.c:289-323` | 命令串 → argv 的唯一入口 |
| C9 | VM 调用掩码 `vm_call_mask`：`ALL_C` 或空集 | `main.c:315-317` | 与 A-8 的 VM 契约对应 |
| C10 | 调度参数 `r_scheduler`/`r_priority`/`r_quantum` ← `SRV_SCH/SRV_Q/SRV_QT` | `main.c:319-322`、`priv.h:88-100` | **boot 服务不含 `system.conf` 的 priority/quantum**（双源事实，见 §3 缺口 G-03） |
| C11 | `rpub->endpoint` ← image 表；旧/新/前/后实例链清空；uid/计时器/计数/period/exec 复位 | `main.c:325-340` | 槽位初值 |
| C12 | 置 `RS_IN_USE|RS_ACTIVE`；`rproc_ptr[slot]` 建立；`rpub->in_use=TRUE` | `main.c:342-345` | 反查索引建立 |

#### D 组：Step 2–4 — 放行、收敛 ready、进入监控（`main.c:348-434`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| D1 | 第二轮按 priv 表序遍历 | `main.c:350-356` | |
| D2 | RS/VM：直接 `init_service(SEF_INIT_FRESH)`；VM 计入待捕获（它会异步回 ready），RS 不计数 | `main.c:362-373`、`utility.c:18-64` | RS 自己不需要 RS_INIT |
| D3 | 其余服务：`sched_init_proc` → `sys_privctl(ALLOW)` | `main.c:375-381`、`utility.c:364-382` | 放行在先，初始化消息在后 |
| D4 | `if SYS_PROC`：`init_service`；`SF_SYNCH_BOOT` 时立即 `catch_boot_init_ready`，否则计数延后 | `main.c:386-397` | 同步/异步两条捕获路径 |
| D5 | Step 3：`while(nr_uncaught)` 逐个 `catch_boot_init_ready(ANY)` | `main.c:404-407` | boot 阻塞等待 |
| D6 | Step 4：对每个服务 `getnpid` 取 pid（失败 panic） | `main.c:413-430` | 供信号/清理使用 |
| D7 | `sys_setalarm(RS_DELTA_T)` 启动周期检查 | `main.c:433-434` | 监控自此驱动 |

#### E 组：boot 末尾的 RS 自升级（`USE_LIVEUPDATE`，`main.c:436-491`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| E1 | `clone_slot(RS 自己)` | `main.c:440-443`、`manager.c:1800-1851` | 新实例槽 |
| E2 | `srv_fork(0,0)` fork 新 RS；`getprocnr` 取副本端点 | `main.c:445-454` | 非阻塞 fork（PM 侧） |
| E3 | 子分支：`update_service(RS_SWAP)` → `cpf_reload` → `cleanup_service(旧)` → `vm_memctl(PIN)` | `main.c:456-473` | 新实例接管 |
| E4 | 父分支：`sys_privctl(SET_SYS)` → `sched_init_proc` → `sys_privctl(YIELD)` → 永不返回 | `main.c:474-490` | 旧实例让位 |

#### F 组：主循环（`main.c:57-130`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| F1 | `rs_idle_period()`：非空闲直接返回；空闲时清理 `RS_DEAD` 槽、补 `SF_USE_REPL` 缺副本 | `main.c:59`、`utility.c:443-480` | 关机时强制清理（避免死锁） |
| F2 | `get_work` → `sef_receive_status(ANY)` 阻塞收消息 | `main.c:62,826-833` | 唯一接收原语 |
| F3 | `rs_isokendpt(who_e)` 校验来源槽号，失败 panic | `main.c:64`、`utility.c:352-359` | |
| F4 | notify 分支：`CLOCK` → `do_period`；其他 → 写 `rproc_ptr[who_p]->r_alive_tm = timestamp`，未知来源仅告警 | `main.c:80-93` | notify 不回复 |
| F5 | 请求分支：15 个 `RS_*` 臂的 switch | `main.c:100-122` | 分派表见新 `10` 契约 |
| F6 | `result != EDONTREPLY` 时 `m.m_type = result` 并 `reply` | `main.c:124-128`、`utility.c:309-327` | EDONTREPLY 是延迟回复哨兵 |

#### G 组：周期检查与心跳（`do_period`，`request.c:943-1046`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| G1 | 更新进行中（且非初始化阶段）先 `update_period` 查 prepare 超时 | `request.c:953-955`、`update.c:371-396` | |
| G2 | 槽位闸门：`RS_ACTIVE` 且（非更新中，或更新中的初始化态） | `request.c:960-963` | |
| G3 | 有效 period：初始化中 → `RS_INIT_T` 或 LU 的 `UPD_INIT_MAXTIME`；否则 `r_period` | `request.c:965-969` | |
| G4 | 分支一（退避）：`r_backoff > 0` 每 tick 减一，归零 `restart_service` | `request.c:975-980` | |
| G5 | 分支二（停止超时）：`r_stop_tm` 超 2×`RS_DELTA_T` 且 pid>0 → `crash_service` | `request.c:985-989` | SIGTERM→SIGKILL 升级 |
| G6 | 分支三（心跳）：应答尚未回（`alive < check`）且超 2×period → free pass 判断；否则置 `RS_NOPINGREPLY` 并 `crash_service` | `request.c:1004-1029` | free pass：有别的服务正在初始化时宽限一周期 |
| G7 | 周期到期 → `ipc_notify(endpoint)` 请求存活应答，记 `r_check_tm = now` | `request.c:1035-1038` | 服务侧由 SEF 拦截并回 ping |
| G8 | 重排下一次 `sys_setalarm(RS_DELTA_T)` | `request.c:1044-1045` | 自驱动时钟 |

#### H 组：信号路径（`main.c:631-704`、`request.c:1051-1090`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| H1 | SEF 信号处理器：`SIGCHLD` → `do_sigchld`；`SIGTERM` → `do_shutdown(NULL)` | `main.c:631-642` | RS 收到 SIGTERM 即整体关机 |
| H2 | `do_sigchld`：`waitpid(-1, WNOHANG)` 排空已死子进程；有槽位（别的信号管理器负责）则清更新位、`free_slot` 全部实例，必要时 `rupdate_clear_upds` | `request.c:1051-1090` | 与 RS 作为信号管理器的路径互补 |
| H3 | SEF 信号管理器（内核代发）：查槽；已 `RS_TERMINATED` 返回 `EDEADEPT`；非 ACTIVE 忽略 | `main.c:655-674` | |
| H4 | 致命信号：`SIGS_IS_STACKTRACE` → `sys_diagctl_stacktrace`；`SIGS_IS_TERMINATION` → 置 `RS_TERMINATED` + `terminate_service` + `rs_idle_period` | `main.c:681-692`、`sys/sys/signal.h:280-286` | |
| H5 | VM 信号吞掉不投递；其余非终止信号翻译为 `SIGS_SIGNAL_RECEIVED` 消息异步投递 | `main.c:693-703` | |

#### I 组：`RS_UP` 一次完整创建（`request.c:15-106` + 创建链）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| I1 | `check_call_permission(RS_UP, NULL)` | `request.c:27`、`manager.c:81-130` | 无目标槽 → 仅 root |
| I2 | `alloc_slot` | `request.c:31`、`manager.c:2067-2083` | 满则 ENOMEM |
| I3 | `copy_rs_start` + `check_request`（调度/优先级/quantum/CPU/信号管理器校验） | `request.c:38-46`、`manager.c:135-146`、`request.c:1265-1308` | datacopy 进内核空间副本 |
| I4 | 解码 `RSS_FORCE_INIT_*` → `SEF_INIT_*` | `request.c:49-61` | 调试注入 |
| I5 | `init_slot`：DSRV 默认 + uid/dev/domain/devman + PCI ACL + 生命周期字段清零 + `edit_slot` | `request.c:64`、`manager.c:1708-1795` | |
| I6 | 重复性检查：label/dev_nr/domain | `request.c:70-87` | EBUSY |
| I7 | `start_service`：create → activate → publish → run | `request.c:90`、`manager.c:950-983` | |
| I8 | `create_service` 三闸门（NEED_REPL/NEED_COPY/空命令） | `manager.c:541-568` | |
| I9 | `srv_fork` + `getprocnr` + 槽位 8 字段登记 + `rproc_ptr` | `manager.c:574-597` | 非阻塞 fork |
| I10 | `sys_privctl(SET_SYS)` + `sys_getpriv` 回读 | `manager.c:600-606` | |
| I11 | `sched_init_proc` | `manager.c:609-614`、`utility.c:364-382` | |
| I12 | `read_exec`（无内存副本时） | `manager.c:619-631`、`manager.c:1372-1419` | |
| I13 | `srv_execve` 装入映像 → `vm_memctl(RS_MEM_PIN)` → `free_exec` | `manager.c:632-644`、`exec.c:21-59` | |
| I14 | `setuid(0)`（PM→VFS 阻塞通信 hack） | `manager.c:646-656` | 与发布阶段的同款 hack 成对 |
| I15 | RS 实例 pin / VM 实例 `MAKE_VM` + 全实例补 pin | `manager.c:658-695` | |
| I16 | `vm_set_priv`（VM 调用掩码） | `manager.c:698-702` | |
| I17 | `activate_service`（ACTIVE 位迁移） | `manager.c:1013-1028`、`manager.c:964` | |
| I18 | `publish_service`：DS label → mapdriver（有 dev/domain）→ PCI ACL（`USE_PCI`）→ devman bind | `manager.c:787-858` | 任一步失败 → `kill_service` |
| I19 | `run_service`：`sys_privctl(ALLOW)` + `init_service`（发 `RS_INIT`） | `manager.c:923-945`、`utility.c:18-64` | |
| I20 | 服务回 `RS_INIT` → `do_init_ready`：坏结果 `crash_service`（ERESTART 且非更新 → `RS_REINCARNATE`）；成功清初始化位、`reply`、`end_srv_init` | `request.c:462-529`、`manager.c:328-355` | boot 期由 `catch_boot_init_ready` 同步收（`main.c:784-821`） |
| I21 | `do_up` 尾：`RSS_NOBLOCK` 立即回 OK，否则登记 `RS_LATEREPLY` 三字段，返回 `EDONTREPLY` | `request.c:95-105` | |

#### J 组：其余控制请求（分派层，`request.c`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| J1 | `do_down`：查槽 → 权限 → 已终止则直接 unpublish+cleanup；否则 `stop_service(RS_EXITING)` + LATEREPLY | `request.c:111-155`、`manager.c:988-1008` | |
| J2 | `do_refresh`：`stop_service(RS_REFRESHING)` + LATEREPLY | `request.c:390-426` | |
| J3 | `do_restart`：只允许恢复脚本调用；暂存并清空脚本后 `restart_service` | `request.c:160-203` | |
| J4 | `do_clone`/`do_unclone`：`SF_USE_REPL` 开关 + `clone_service`/`cleanup_service_now` | `request.c:208-293` | |
| J5 | `do_edit`：copy/label → 权限 → `sys_getpriv` → `sched_stop` → `edit_slot` → `SYS_PRIV_UPDATE_SYS` → `vm_set_priv` → `sched_init_proc` → 重建副本 | `request.c:298-385` | 8 步 |
| J6 | `do_shutdown`：权限（可无 m_ptr）→ `shutting_down=TRUE` → 所有在用槽置 `RS_EXITING` | `request.c:431-457` | |
| J7 | `do_lookup`：长度闸门 → 拷名 → `lookup_slot_by_label` → 回写 endpoint | `request.c:1144-1176` | |
| J8 | `do_getsysinfo`：root-only；`SI_PROC_TAB`/`SI_PROCPUB_TAB`/`SI_PROCALL_TAB`，尺寸精确匹配（PROCALL 先拷 rproc 再 fallthrough 拷 rprocpub） | `request.c:1095-1139` | |
| J9 | `do_sysctl`：5 个子操作（状态打印 / 更新 START/RUN/STOP/STATUS），无权限检查 | `request.c:1181-1224` | |
| J10 | `do_fi`：拷 label → 权限 → `fi_service` 异步注入 `COMMON_REQ_FI_CTL` | `request.c:1229-1260`、`utility.c:69-77` | |

#### K 组：终止、清理与恢复（`manager.c`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| K1 | `terminate_service` 决策树：初始化失败处理（更新中回滚 / `SF_NO_BIN_EXP` 刷新 / 否则退出） | `manager.c:1069-1093` | |
| K2 | 更新进行中先 `abort_update_proc` | `manager.c:1099-1102` | |
| K3 | `SF_NORESTART` 强制退出；`SF_DET_RESTART` 与 `r_restarts<MAX_DET_RESTART` 置 `RS_CLEANUP_DETACH`；有脚本置 `RS_CLEANUP_SCRIPT` | `manager.c:1104-1117` | |
| K4 | `RS_EXITING` 分支：核心服务死亡 → RS `_exit(1)`；计划中的更新 abort；late reply；unpublish；逐实例 cleanup；`RS_REINCARNATE` 则 `reincarnate_service` | `manager.c:1119-1154` | |
| K5 | `RS_REFRESHING` 分支 → `restart_service` | `manager.c:1155-1158` | |
| K6 | 非预期退出：首次直接 restart；否则二进制指数退避（`1<<restarts`，cap `MAX_BACKOFF`，`SF_USE_COPY` 时回 1），回到 `do_period` 等退避走完 | `manager.c:1164-1178`、`request.c:975-980` | |
| K7 | `cleanup_service` 两阶段：第一阶段摘链 + `RS_DEAD` + `SYS_PRIV_DISALLOW/CLEAR_IPC_REFS` + 清 ACTIVE + late reply；第二阶段 `sched_stop` + `srv_kill(SIGKILL)` + 脚本 + detach 或 `free_slot` | `manager.c:405-492` | `RS_REINCARNATE` 时不 free |
| K8 | `kill_service` = 置 `RS_EXITING` + `crash_service`；`crash_service` = RS 自身 `exit(1)` 或 `sys_kill(SIGKILL)` | `manager.c:360-400` | |
| K9 | `detach_service`：唯一化 label 重发布 + 降级为普通进程 + ALLOW | `manager.c:497-526` | |
| K10 | `restart_service`：有脚本先跑脚本；否则无副本则 `clone_service` → `update_service(RS_SWAP)` → `run_service(SEF_INIT_RESTART)`；`SF_DET_RESTART` 置 detach | `manager.c:1246-1298` | |
| K11 | `reincarnate_service`：`clone_slot` 新槽 + 清 `rproc_ptr` + `start_service(SEF_INIT_FRESH)` + restarts+1 | `manager.c:1033-1050` | 新端点 |
| K12 | `run_script`：reason/incarnation 环境参数；fork+execle；父进程 `SYS_PRIV_SET_USER`、`vm_set_priv`、ALLOW、重 pin | `manager.c:1185-1241` | 无 libc 环境下的 A-1 缺口 |

#### L 组：Live Update（`request.c:534-889` + `update.c` 全部）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| L1 | `do_update`：copy/label → 旗标解码（SELF/ASR/PREPARE_ONLY/DETACH/NOMMAP/FORCE_INIT_*，VM 默认 mmap 预分配 8 MiB）→ target label → 权限 | `request.c:552-644` | |
| L2 | 纯校验五查（state NULL、maxtime 默认、更新中 EBUSY、已调度约束、prepare-only 端点限制、RS 不支持 prepare-only） | `request.c:648-686` | |
| L3 | 建新实例：self → `clone_service(LU_SYS_PROC)`；普通 → `alloc_slot`+`init_slot`+`inherit_service_defaults`+`create_service`+新旧互链；RS 更新时 `update_sig_mgrs` 备份；heap/mmap 预分配 | `request.c:691-810` | |
| L4 | 状态数据：`init_state_data` + 三笔 `cpf_grant_direct` | `request.c:813-847`、`manager.c:174-284` | |
| L5 | 填描述符 + `rupdate_add_upd`（部分排序：普通 → VM → RS；标志全链传播） | `request.c:849-854`、`update.c:23-86` | |
| L6 | batch → 立即回；否则 `start_update_prepare` → 无则 OK/ESRCH，noblock OK，否则 LATEREPLY + EDONTREPLY | `request.c:859-884` | |
| L7 | prepare：逐服务 `request_prepare_update_service` 发 `RS_LU_PREPARE`；VM multi 时在 VM 条目位置先做 `vm_prepare` | `update.c:401-527` | |
| L8 | 服务回 `RS_LU_PREPARE` → `do_upd_ready`：链门校验 → `RS_PREPARE_DONE` → 失败 `end_update(RS_REPLY)`；否则下一个或 `start_update` | `request.c:890-938` | |
| L9 | `start_update`：`RS_INITIALIZING`；取消 prepare-only；逐服务 `start_srv_update`（RS 跳过 update，VM multi 额外 `rs_receive_ticks` 等待 VM init） | `update.c:532-620` | A-9 超时接收 |
| L10 | `complete_srv_update`：RS 条目 `init_service(SEF_INIT_LU)` + YIELD（失败回滚）；普通 `run_service(SEF_INIT_LU)`，失败 rollback | `update.c:657-702` | |
| L11 | `end_update`：失败且新 RS 已 init → `exit(1)`；反向遍历按角色结束（curr/before-prepare/prepare-done/initializing；VM 最后）；成功逐描述符 `end_srv_init`；late reply；清链与端点/旗标 | `update.c:744-931` | |
| L12 | `end_srv_update`：surviving/exiting 选择；清链与更新位；REPLY/CANCEL 回复；exiting 逐实例 cleanup（DETACHED 时先置 detach）；VM multi 成功改 CANCEL | `update.c:932-1011` | |
| L13 | `rollback_service`：RS 特例（`sys_whoami` + `vm_update(SF_VM_ROLLBACK)` + 全部 `r_check_tm=0`）；普通 `update_service(SWAP/DONTSWAP, SF_VM_ROLLBACK)` | `update.c:330-366` | |
| L14 | `update_period` 超时 → `end_update(EINTR, RS_CANCEL)`；`abort_update_proc` 三阶段分派 | `update.c:371-396,707-739` | |

#### M 组：RS 自身重启/热更新回调（`main.c:499-626`）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| M1 | `sef_cb_init_restart`：默认状态迁移 → 找新旧 RS 槽 → 若更新中 `end_update` → `update_service(DONTSWAP)` → `init_service(SEF_INIT_RESTART)` → 重排 alarm | `main.c:499-544` | |
| M2 | `sef_cb_init_lu`：默认 LU 迁移 → `update_service(DONTSWAP)` → 四个断言（更新中/初始化中/有描述符/有 pending） | `main.c:549-586` | |
| M3 | `sef_cb_init_response` / `sef_cb_lu_response`：把 SEF 回调的 `RS_INIT`/`RS_LU_PREPARE` 消息喂给同一套 `do_init_ready`/`do_upd_ready`（EDONTREPLY 归一） | `main.c:591-626` | RS 自模拟 ready |

### 1.2 真序核对记录（G1 抽样十条）

| 抽样 | 断言 | 核对命令 | 结果 |
|------|------|---------|------|
| 1 | DS 在 RS 之前登记 | `sed -n '52,53p' minix3/minix/kernel/table.c` | 一致（ds、rs） |
| 2 | Step 2 里 RS/VM 走 `init_service` 且 VM 计入 uncaught | `sed -n '362,373p' minix3/minix/servers/rs/main.c` | 一致 |
| 3 | `RS_INIT_T = system_hz*10` | `grep RS_INIT_T servers/rs/const.h` | 一致（const.h:48） |
| 4 | 退避公式含 `SF_USE_COPY` 回 1 | `sed -n '1164,1173p' servers/rs/manager.c` | 一致（1168-1169） |
| 5 | `do_sigchld` 用 `waitpid(-1, WNOHANG)` | `sed -n '1063p' servers/rs/request.c` | 一致 |
| 6 | `do_getsysinfo` PROCALL 先拷后 fallthrough | `sed -n '1116,1129p' servers/rs/request.c` | 一致 |
| 7 | `rupdate_add_upd` 的 RS 最后、VM 紧前 | `sed -n '42,48p' servers/rs/update.c` | 一致 |
| 8 | `complete_srv_update` RS 分支 `SYS_PRIV_YIELD` | `sed -n '673,683p' servers/rs/update.c` | 一致 |
| 9 | `catch_boot_init_ready` 对 VM 不回复 | `sed -n '809,815p' servers/rs/main.c` | 一致 |
| 10 | 信号终止集合定义 | `sed -n '280,286p' minix3/sys/sys/signal.h` | 一致（LETHAL 6 个 + KILL/PIPE） |

---

## 2. 知识点全集

### 2.0 编号与列说明

- 编号 `K-<新篇章号>-<序号>`：stage 内唯一，且直接编码去向（例如 `K-03-*` 全部进新 03 篇）。这样 §5 契约的知识点清单可以只引用编号。
- **类型**：概念 / 机制 / 数据结构 / 接口协议 / 约束不变量 / 架构演进 / 工具工程 / 测试。
- **来源**：`存量`（来自旧文档）或 `新增`（旧文档没有，由 §3 覆盖审计发现并追加入池）。两类在池内平等，区别只是存量条目要回答"搬到哪里去"，新增条目要回答"证据锚点在哪里"。
- **现有位置**：旧文档与小节号；`—` 表示旧文档没有。
- **锚点**：C 源码（`文件:行` 或符号）、非 C 制品路径、或操作系统理论出处；文档类锚点仅在"该知识点确实是文档组织型"时使用。
- **主讲述点**：同一知识点在旧目录多处出现时，标注保留哪一处作为新目录的主讲述点（其余位置在新目录中不再展开）。

### 2.1 新 00（总览）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-00-01 | RS 身份：root system process | 概念 | 存量 | 00 §1.1 | `include/minix/com.h:61,77` | 回答"RS 是谁、为什么编号 2" | 主讲述点 |
| K-00-02 | boot 链两层顺序语义（登记序 vs 执行序） | 概念 | 存量 | 00 §1.2 | `kernel/table.c:36-64`；`kernel/main.c:196,265` | 回答"RS 为什么是第二个运行的，却是登记表里第二个之外的顺序" | 主讲述点；boot 篇引用不复述 |
| K-00-03 | 两个状态机与生命周期次主线 | 概念 | 存量 | 00 §1.3、§3.0-3.2 | `servers/rs/main.c:57-130`；`manager.c:950-983` | 建立全 stage 心智模型 | 主讲述点 |
| K-00-04 | RS 为何需要"复活"能力 | 概念 | 存量 | 00 §3.2 | `rs.h:191-203`（SF_*） | 回答"名字从哪来、可靠性模型是什么" | 归 00 §1 |
| K-00-05 | 新目录导航表与依赖图 | 工具工程 | 存量 | 00 §4.0-4.2 | 新目录 TOC（本蓝图 §4） | 提供逐篇入口 | 主讲述点 |
| K-00-06 | 组织原则（位置可回答性/禁前向/单语义） | 概念 | 存量 | 00 §5.1-5.3 | `plan.md:131-161` | 告诉读者本文档集按什么规则编排 | 主讲述点 |
| K-00-07 | ARCH A-1..A-14 索引 | 架构演进 | 存量 | 00 §5.4 | `plan.md:166-181` | 提供架构演进的一览与定位 | 主讲述点；细节留各篇 |
| K-00-08 | 覆盖契约（8 .c/15 消息/16 位/13 SF） | 约束不变量 | 存量 | 00 §5.5 | `wc -l` 与 `grep -c` 证据（本节 §0.3） | 给出覆盖承诺 | 主讲述点 |
| K-00-09 | 阅读路线（按目的入口） | 工具工程 | 存量 | 00 §6 | 新目录 TOC | 帮助跳读 | 主讲述点 |

### 2.2 新 01（SEF 运行时与进程骨架）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-01-01 | SEF 是什么；RS 是唯一注册全套回调的服务 | 概念 | 存量 | 01 §1.3；18 §1.1 | `servers/rs/main.c:136-153`；`include/minix/sef.h:41-95` | 回答"RS 为什么不直接写 while 循环" | 主讲述点 |
| K-01-02 | 7 个回调的注册顺序与语义 | 接口协议 | 存量 | 01 §2.2 | `main.c:138-149` | 回答"SEF 会回调哪些函数" | 主讲述点 |
| K-01-03 | 三种 init 类型（FRESH/RESTART/LU）与两个 response 回调 | 接口协议 | 存量 | 01 §1.3；18 §2.2-2.3；12 §1.3 | `sef.h:93-95`；`main.c:499,549,591,614` | 回答"同一个服务如何面对三种被启动方式" | 主讲述点 |
| K-01-04 | `main()` 三活动骨架与消息分类轮廓 | 机制 | 存量 | 01 §2.1；06 §1.1 | `main.c:38-131` | 给出 RS 主程序的全貌 | 主讲述点；分派细节归 10 |
| K-01-05 | reply / late_reply / EDONTREPLY 回复约定 | 接口协议 | 存量 | 06 §2.2；01 §2.1；12 §1.2 | `utility.c:309-347`；`sys/sys/errno.h:199` | 回答"RS 的回复为什么可以延迟" | 主讲述点；全局公共原语 |
| K-01-06 | rs_asynsend / AMF_NOREPLY | 接口协议 | 存量 | 06 §2.4 | `utility.c:223-242` | 回答"RS 怎么发不需回复的消息" | 主讲述点 |
| K-01-07 | 单线程事件循环执行模型（`!Send`/`Rc` 合理） | 约束不变量 | 存量 | 06 §1.1 | `main.c:57-130`；AGENTS.md 执行模型 | 回答"Rust 实现为什么可以单线程" | 主讲述点 |
| K-01-08 | SEF ping 拦截：服务侧对心跳的应答 | 机制 | 新增 | — | `lib/libsys/sef_ping.c:21-38`；`sef.c:212` | 补全心跳闭环的服务侧 | 新增；监控篇引用 |
| K-01-09 | `sef_cb_init_response`/`lu_response` 自模拟 ready | 机制 | 存量 | 01 §2.3.7；12 §1.3 | `main.c:591-626` | 回答"RS 自己怎么走一遍握手" | 主讲述点 |
| K-01-10 | 信号回调注册与两条信号通路的分工 | 机制 | 存量 | 06 §2.6 | `main.c:631-704` | 区分 SIGCHLD/SIGTERM 与内核信号管理器 | 行为归 13，注册归本篇 |
| K-01-11 | KernelApi 五域面：纯决策与 shell 执行分离 | 架构演进 | 存量 | 01 §3.5；99 §3.4 | `boot.rs` 五个 trait；`trap_api.rs` | 回答"Rust 代码为什么把决策与外部调用分开" | 主讲述点；外部实现归 18 |
| K-01-12 | 入口装载链（crt0 → sef_startup → main） | 机制 | 新增 | — | `lib/libsys/sef.c:185-230`；`sef_init.c` | 回答"RS 的 main 是谁调用的" | 新增；前序 stage 只给结论 |

### 2.3 新 02（服务登记表）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-02-01 | 双表结构 rproc/rprocpub 与 `r_pub` 自指针 | 数据结构 | 存量 | 02 §1.2、§2.1 | `glo.h:33-34`；`type.h:57` | 回答"私有状态与公开面怎么分" | 主讲述点 |
| K-02-02 | `struct rproc` 全字段与字段归属表 | 数据结构 | 存量 | 02 §2.3、§3.10 | `type.h:56-108` | 回答"一个槽位里有什么" | 主讲述点 |
| K-02-03 | `struct rprocpub` 全字段与 `NO_DEV` | 数据结构 | 存量 | 02 §2.4 | `include/minix/rs.h:165-183`；`include/minix/const.h:132` | 回答"外部能看见什么" | 主讲述点 |
| K-02-04 | `r_flags` 16 位状态/策略旗标 + `RS_SRV_IS_IDLE` | 数据结构 | 存量 | 02 §2.5 | `servers/rs/const.h:28-45` | 回答"服务现在处于什么状态" | 主讲述点；其余篇只引用 |
| K-02-05 | `sys_flags` 13 位、`IMM_SF` 不可变位与预设组合 | 数据结构 | 存量 | 02 §2.6 | `include/minix/rs.h:191-206`；`const.h:65-68` | 回答"服务的策略属性有哪些、哪些继承时不能改" | 主讲述点 |
| K-02-06 | `rprocupd`/`rupdate` 更新描述符形状 | 数据结构 | 存量 | 02 §2.7 | `type.h:30-52` | 为 LU 篇准备数据底座 | 主讲述点；状态机归 16 |
| K-02-07 | `rinit` 描述符与 `rproctab_gid` | 数据结构 | 存量 | 02 §2.8 | `glo.h:40`；`sef.h:41-53` | 回答"服务怎么读到公开表" | 主讲述点 |
| K-02-08 | `rproc_ptr` O(1) 反查索引 | 数据结构 | 存量 | 02 §1.4、§3.5 | `glo.h:35`；`main.c:86,344` | 回答"主循环怎么快速找到发送方" | 主讲述点 |
| K-02-09 | 实例链四指针（old/new/prev/next） | 数据结构 | 存量 | 02 §1.4、§2.3 | `type.h:58-61` | 回答"一个服务的多个实例怎么串起来" | 主讲述点；使用归 11/16 |
| K-02-10 | 表容量常量（NR_PROCS/NR_SYS_PROCS/NR_BOOT_PROCS） | 约束不变量 | 存量 | 02 §2.2 | `minix/sys_config.h:9`；`minix/config.h:31-32`；`com.h:56` | 回答"最多几个服务" | 主讲述点 |
| K-02-11 | `r_pid = -1` 的"无进程"语义 | 约束不变量 | 存量 | 02 §2.3 | `type.h:63`；`manager.c:1965,2106` | 回答"槽位没有对应进程时怎么表示" | 主讲述点 |
| K-02-12 | `r_io_tab`/`r_irq_tab` 备份字段（单权威原则） | 约束不变量 | 存量 | 02 §2.3 | `type.h:99-103`；`manager.c:1498,1519` | 回答"为什么有两份 I/O/IRQ 表" | 主讲述点 |
| K-02-13 | `lookup_slot_by_*` 五种过滤差异 | 机制 | 存量 | 02 §2.9 | `manager.c:1935-2062` | 回答"按 label/pid/dev/domain/flags 查找各有什么前提" | 主讲述点 |
| K-02-14 | `alloc_slot`/`free_slot` 槽位生命周期 | 机制 | 存量 | 02 §2.9 | `manager.c:2067-2109` | 回答"槽位何时分配/释放、释放时清什么" | 主讲述点 |
| K-02-15 | `rs_isokendpt` 端点范围校验 | 机制 | 存量 | 02 §2.10 | `utility.c:352-359` | 回答"哪些来源消息合法" | 主讲述点 |
| K-02-16 | `get_service_instances` 实例收集与顺序 | 机制 | 存量 | 02 §2.11 | `manager.c:1334-1352` | 回答"一个服务的实例集合是谁" | 主讲述点 |
| K-02-17 | Rust 类型化模型（RFlags/SysFlags/索引链/by_endpoint） | 架构演进 | 存量 | 02 §3.1-3.9 | `service_slot.rs`；`process_table.rs` | 回答"裸指针表在 Rust 里怎么表达" | 主讲述点 |

### 2.4 新 03（权限结构）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-03-01 | 内核 priv 表与 RS 的本地草稿副本 | 概念 | 存量 | 03 §1.1 | `kernel/priv.h:94`；`type.h:88` | 回答"权限到底存在哪、谁是权威" | 主讲述点 |
| K-03-02 | `struct priv` 字段分组 | 数据结构 | 存量 | 03 §2.1 | `kernel/priv.h:21-66` | 回答"一个权限结构包含什么" | 主讲述点 |
| K-03-03 | 白名单模型（位图而非角色） | 概念 | 存量 | 03 §1.2 | `kernel/priv.h:34-38` | 回答"权限用什么形式表达" | 主讲述点 |
| K-03-04 | boot Step 1 的 priv 装配公式 | 机制 | 存量 | 03 §2.4；01 §2.3.3 | `main.c:262-280` | 回答"每个 boot 服务的权限字段从哪来" | 主讲述点；boot 篇只引用 |
| K-03-05 | `SRV_OR_USR` 三态选择与 `SRV_*/USR_*` 默认宏 | 机制 | 存量 | 03 §1.3、§2.2 | `const.h:71`；`include/minix/priv.h:45-100` | 回答"系统服务与用户进程的默认权限差异" | 主讲述点 |
| K-03-06 | RS/VM 的 `SET_SYS` 例外与 `getpriv` 回读 | 约束不变量 | 存量 | 03 §2.4 | `main.c:282-296` | 回答"为什么 RS/VM 不需要提交权限" | 主讲述点 |
| K-03-07 | RS 使用的 7 个 privctl 操作 | 接口协议 | 存量 | 03 §2.6 | `com.h:342-353`；`kernel/system/do_privctl.c` | 回答"提交/更新权限有哪些动作" | 主讲述点 |
| K-03-08 | `SYS_PRIV_UPDATE_SYS` 的内核覆盖规则 | 机制 | 存量 | 03 §2.6 | `do_privctl.c:280-368` | 回答"编辑服务时哪些字段内核允许改" | 主讲述点 |
| K-03-09 | `fill_send_mask`/`fill_call_mask` 两个位图原语 | 机制 | 存量 | 03 §2.5 | `utility.c:82-137` | 回答"掩码怎么批量置位" | 主讲述点；send mask 语义归 04 |
| K-03-10 | `sched_init_proc` 与 `sched_start` 分支 | 机制 | 存量 | 03 §2.7 | `utility.c:364-382`；`lib/libsys/sched_start.c:45-98` | 回答"调度注册怎么到达内核或 SCHED" | 主讲述点 |
| K-03-11 | `update_sig_mgrs` 三步序 | 机制 | 存量 | 03 §2.8 | `utility.c:387-419` | 回答"信号管理器怎么安全换人" | 主讲述点 |
| K-03-12 | `s_flags` 11 位定义 | 数据结构 | 存量 | 03 §2.3 | `include/minix/const.h:143-154` | 回答"每个权限旗标什么含义" | 主讲述点 |
| K-03-13 | 静态 priv id 与 `USER_PRIV_ID` | 数据结构 | 存量 | 03 §2.2 | `include/minix/priv.h:10-21` | 回答"send mask 的位号是什么" | 主讲述点 |
| K-03-14 | 驱动面 privctl 操作不在本 stage 使用 | 架构演进 | 存量 | 03 §1.4、§3.3 | `do_privctl.c:187-251`；A-10 | 回答"为什么文档只讲 7 个操作" | 主讲述点 |
| K-03-15 | Rust `Privilege`/`PrivCtlOp`/`CallMask`/`SysMap`/sched 原语 | 架构演进 | 存量 | 03 §3 | `privilege.rs`；`sched.rs` | 回答"权限模型在 Rust 里怎么建" | 主讲述点 |

### 2.5 新 04（IPC 可达性）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-04-01 | 两条计算路径：boot 全置与动态精确计算 | 概念 | 存量 | 05 §1.2 | `main.c:272-273`；`manager.c:2300-2331` | 回答"发送掩码什么时候算" | 主讲述点 |
| K-04-02 | forward/backward 两次扫描及其原因 | 机制 | 存量 | 05 §1.3 | `manager.c:2157-2294` | 回答"服务启动先后不一，怎么保证双向可达" | 主讲述点 |
| K-04-03 | `get_next_name` 分词器（空白/NUL/超长） | 机制 | 存量 | 05 §2.3 | `manager.c:2115-2152` | 回答"IPC 列表文本怎么解析" | 主讲述点 |
| K-04-04 | `SYSTEM`/`USER` 伪名经 `sys_getpriv` 解析 | 机制 | 存量 | 05 §2.4 | `manager.c:2177-2222` | 回答"特殊目标名怎么落到位号" | 主讲述点 |
| K-04-05 | backward 的 `IPC_ALL`/`IPC_ALL_SYS` 补齐 | 机制 | 存量 | 05 §2.5 | `manager.c:2261-2274` | 回答"全通配服务如何反向授权" | 主讲述点 |
| K-04-06 | `init_privs` 总入口与清位 | 机制 | 存量 | 05 §2.6 | `manager.c:2300-2331` | 回答"掩码重算的入口在哪" | 主讲述点 |
| K-04-07 | `r_ipc_list` 拷贝、长度上限与空列表拒绝 | 约束不变量 | 存量 | 05 §2.7；08 §2.4 | `manager.c:1476-1483` | 回答"列表从哪来、多长" | 主讲述点 |
| K-04-08 | 调用链 `init_slot → edit_slot → init_privs` | 机制 | 存量 | 05 §2.7 | `manager.c:1794,1700` | 回答"新服务/改配置后掩码何时算" | 主讲述点 |
| K-04-09 | Rust `IpcListIterator`/`SysMap` | 架构演进 | 存量 | 05 §3 | `ipc_mask.rs` | 回答"分词与位图在 Rust 里怎么表达" | 主讲述点 |

### 2.6 新 05（调用者访问控制）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-05-01 | 两级授权：root 或隔离策略 | 概念 | 存量 | 04 §1.1、§1.3 | `manager.c:91-97` | 回答"谁有资格命令 RS" | 主讲述点 |
| K-05-02 | `caller_is_root` 经 `getnuid` 与 fail-closed | 机制 | 存量 | 04 §2.1 | `manager.c:21-34`；`lib/libsys/getepinfo.c:34-44` | 回答"root 判定怎么做的、PM 失败怎么办" | 主讲述点 |
| K-05-03 | `caller_can_control`：按 `proc_name` 匹配控制列表 | 机制 | 存量 | 04 §2.2 | `manager.c:39-76` | 回答"隔离策略怎么授权" | 主讲述点 |
| K-05-04 | 五条目标槽规则与判定顺序 | 机制 | 存量 | 04 §1.2、§2.3 | `manager.c:99-127` | 回答"哪些请求在哪些状态下被拒" | 主讲述点 |
| K-05-05 | 11 个调用点、ready 绕过、`do_getsysinfo` 的 call=0 | 接口协议 | 存量 | 04 §2.4 | `request.c:27,133,183,232,277,329,412,643,1104,1253`；`request.c:462,890` | 回答"入口检查安插在哪里" | 主讲述点 |
| K-05-06 | `RUPDATE_IS_UPDATING` 在访问层的消费 | 约束不变量 | 存量 | 04 §2.5 | `const.h:105`；`manager.c:108-110` | 回答"为什么更新中控制调用 EBUSY" | 主讲述点；状态机归 16 |
| K-05-07 | Rust 纯决策函数与 euid 注入 | 架构演进 | 存量 | 04 §3 | `access.rs` | 回答"授权判定怎么可测" | 主讲述点 |

### 2.7 新 06（初始化握手）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-06-01 | 为什么用消息握手而非阻塞调用 | 概念 | 存量 | 12 §1.1 | `request.c:462`；`main.c:57-130` | 回答"单线程服务如何等别人初始化" | 主讲述点 |
| K-06-02 | boot 同步捕获 vs 运行时异步分派 | 机制 | 存量 | 12 §1.2 | `main.c:390-407`；`request.c:462` | 回答"同一个 ready 为什么两种收法" | 主讲述点 |
| K-06-03 | `RS_INIT` 载荷 9 字段 | 数据结构 | 存量 | 12 §2.2 | `utility.c:49-61`；`ipc.h:1855-1866` | 回答"服务初始化时收到什么" | 主讲述点 |
| K-06-04 | `RS_INITIALIZING` 位与两个时间戳的设置 | 机制 | 存量 | 12 §2.2 | `utility.c:24-26` | 回答"等待 ready 期间槽位处于什么状态" | 主讲述点 |
| K-06-05 | `ROOT_SYS_PROC` 不向自己发 `RS_INIT` | 约束不变量 | 存量 | 12 §2.2 | `utility.c:28-31` | 回答"RS 自己的初始化为何特殊" | 主讲述点 |
| K-06-06 | `old_endpoint`/`prepare_state` 推导 | 机制 | 存量 | 12 §2.2 | `utility.c:33-42` | 回答"新实例怎么知道旧实例是谁" | 主讲述点 |
| K-06-07 | `do_init_ready` 四分支 | 机制 | 存量 | 12 §2.3 | `request.c:462-529` | 回答"ready 消息怎么仲裁" | 主讲述点 |
| K-06-08 | 失败语义：`ERESTART`→`RS_REINCARNATE`、失败路径不回复 | 约束不变量 | 存量 | 12 §2.3、§4.2 | `request.c:492-497,513,528` | 回答"初始化失败后会发生什么" | 主讲述点 |
| K-06-09 | `do_upd_ready` 链门与三路派发 | 机制 | 存量 | 12 §2.4；16 §2.8 | `request.c:890-938` | 回答"LU prepare 完成如何推进" | 主讲述点；LU 机器归 16 |
| K-06-10 | `catch_boot_init_ready` 与 VM 异步例外 | 机制 | 存量 | 12 §2.5 | `main.c:784-821` | 回答"boot 怎么阻塞等某个服务" | 主讲述点 |
| K-06-11 | `end_srv_init` 收尾（late reply/旧副本/restarts） | 机制 | 存量 | 12 §2.6 | `manager.c:328-355` | 回答"一次初始化成功后的簿记" | 主讲述点 |
| K-06-12 | RS 自模拟 ready 的两个回调 | 机制 | 存量 | 12 §1.3 | `main.c:591-626` | 回答"RS 自己的 ready 走哪条路" | 主讲述点 |
| K-06-13 | `run_service` 放行原语（ALLOW + `init_service`） | 机制 | 存量 | 12 §2.1 | `manager.c:923-945` | 回答"服务从已创建到开始初始化的那一步做什么" | 主讲述点 |

### 2.8 新 07（服务声明：system.conf 与 minix-service）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-07-01 | `system.conf` 指令集与语义（uid/ipc/system/vm/io/irq/sigmgr/scheduler/priority/quantum/control/pci） | 接口协议 | 新增 | — | `minix3/etc/system.conf:5-529` | 回答"一个服务的权限与调度声明长什么样" | 新增，主讲述点 |
| K-07-02 | `all`/`none`/`basic` 关键字与前缀语法 | 接口协议 | 新增 | — | `commands/minix-service/parse.c:852-900,658-720` | 回答"指令值的三种写法" | 新增 |
| K-07-03 | boot 服务与动态服务两类声明块 | 数据结构 | 新增 | — | `system.conf:1-4,238-240` | 回答"文件为什么分两段" | 新增 |
| K-07-04 | `minix-service` 解析产物：`struct rs_start` | 接口协议 | 新增 | — | `parse.c:1229`（`parse_config`）；`minix-service.c:756` | 回答"声明如何变成请求" | 新增 |
| K-07-05 | `rs_start` 字节布局与字段 | 接口协议 | 存量 | 08 §3.5 | `include/minix/rs.h:104-151` | 回答"请求的线格式是什么" | 主讲述点 |
| K-07-06 | 默认值与 memset 契约（解析器先零化） | 机制 | 存量 | 08 §3.5、§5 | `parse.c:1160-1169` | 回答"未声明的字段默认是什么" | 主讲述点 |
| K-07-07 | `service` 命令族（up/down/restart/refresh/update/...） | 工具工程 | 新增 | — | `commands/minix-service/minix-service.c`（命令表） | 回答"运维怎么触发 RS 请求" | 新增 |
| K-07-08 | `control`/`pci` 指令 | 接口协议 | 新增 | — | `system.conf:389-398,308-350`；`rs.h:154-163` | 回答"隔离策略与 PCI ACL 从哪声明" | 新增 |
| K-07-09 | `SERVICE_UID`/`SERVICE_LOGIN` | 数据结构 | 存量 | 99 §2.3 | `include/minix/rs.h:12,22` | 回答"服务默认 uid 999 的来历" | 主讲述点 |
| K-07-10 | 解析失败 fail-fast（fatal 退出） | 约束不变量 | 新增 | — | `parse.c`（`fatal` 调用族） | 回答"配置写错会怎样" | 新增 |
| K-07-11 | Rust 侧缺口：没有 `system.conf` 解析器（仅 `svcsched` 的命令行解析与 `slot.rs` 注释引用 parse.c） | 架构演进 | 新增 | — | `os/servers/rs/src/slot.rs:227,721`（注释）；`os/commands/usr-sbin/svcsched/src/service.rs:6-9,56`；`os/etc/README.md`（占位） | 明确实现边界 | 新增，记 §7 |

### 2.9 新 08（槽位配置与映像准备）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-08-01 | 三段配置管线：校验 → 拷入 → 落地 | 概念 | 存量 | 08 §1.1 | `request.c:43-47`；`manager.c:135,1460,1708` | 回答"声明到槽位之间发生什么" | 主讲述点 |
| K-08-02 | `check_request` 校验组（调度/优先级/quantum/CPU/信号管理器） | 机制 | 存量 | 08 §2.1 | `request.c:1265-1308` | 回答"哪些参数非法" | 主讲述点 |
| K-08-03 | `copy_rs_start`/`copy_label` 的拷入与截断 | 机制 | 存量 | 08 §2.2 | `manager.c:135-169` | 回答"调用者的指针怎么安全取用" | 主讲述点 |
| K-08-04 | `init_slot` 默认值与生命周期字段清零 | 机制 | 存量 | 08 §2.3 | `manager.c:1708-1795` | 回答"新服务从什么状态起步" | 主讲述点 |
| K-08-05 | `edit_slot` 字段覆盖表与八条规则 | 机制 | 存量 | 08 §2.4 | `manager.c:1460-1703` | 回答"每个可编辑字段怎么落" | 主讲述点 |
| K-08-06 | IRQ/IO 哨兵值与 `CHECK_IRQ/CHECK_IO_PORT` | 机制 | 存量 | 08 §2.4 | `rs.h:25-28`；`manager.c:1485-1524` | 回答"ALL/NONE 与显式表怎么区分" | 主讲述点 |
| K-08-07 | 内核/VM 调用掩码的 basic 叠加 | 机制 | 存量 | 08 §2.4 | `manager.c:1527-1540` | 回答"BASIC 关键字加了哪些调用" | 主讲述点 |
| K-08-08 | 绝对路径约束与 argv 构建 | 约束不变量 | 存量 | 08 §2.4-2.5 | `manager.c:1577-1586,289-323` | 回答"命令串格式" | 主讲述点 |
| K-08-09 | label 默认取 `proc_name` | 机制 | 存量 | 08 §2.4 | `manager.c:1595-1615` | 回答"没给 label 时叫什么" | 主讲述点 |
| K-08-10 | 恢复脚本仅限非核心服务 | 约束不变量 | 存量 | 08 §2.4 | `manager.c:1617-1626` | 回答"谁能有脚本" | 主讲述点 |
| K-08-11 | `RSS_COPY`/`RSS_REUSE` 分支与 `SF_USE_COPY` 置位 | 机制 | 存量 | 08 §2.4；09 §2.3 | `manager.c:1629-1661` | 回答"内存副本何时建/复用" | 主讲述点 |
| K-08-12 | 行为旗标 `RSS_REPLICA`/`NO_BIN_EXP`/`DETACH`/`NORESTART` | 机制 | 存量 | 08 §2.4 | `manager.c:1662-1682` | 回答"四个策略旗标的作用" | 主讲述点 |
| K-08-13 | period/restarts/asr 覆盖规则（RS 自身除外） | 机制 | 存量 | 08 §2.4 | `manager.c:1684-1697` | 回答"这些字段什么时候被覆盖" | 主讲述点 |
| K-08-14 | `inherit_service_defaults` 只继承不可变位 | 机制 | 存量 | 08 §2.6 | `manager.c:1303-1329`；`rs.h:205` | 回答"副本/新版本能继承什么" | 主讲述点 |
| K-08-15 | `read_exec` 全链与错误面 | 机制 | 存量 | 09 §2.1 | `manager.c:1372-1419` | 回答"二进制怎么读进 RS" | 主讲述点 |
| K-08-16 | `share_exec`/`free_exec` 最后持有者扫描 | 机制 | 存量 | 09 §2.2 | `manager.c:1357-1367,1424-1455` | 回答"共享副本何时释放" | 主讲述点 |
| K-08-17 | `srv_execve` 栈帧与 `ps_strings` | 机制 | 存量 | 09 §2.4 | `exec.c:21-59` | 回答"新进程的初始栈怎么搭" | 主讲述点 |
| K-08-18 | `do_exec` 装配与 loader 表 | 机制 | 存量 | 09 §2.5 | `exec.c:62-116` | 回答"ELF 怎么装进子进程地址空间" | 主讲述点 |
| K-08-19 | `exec_restart`/`read_seg` 边界 | 机制 | 存量 | 09 §2.6 | `exec.c:121-165` | 回答"装载失败怎么回滚、越界怎么防" | 主讲述点 |
| K-08-20 | `Arc<[u8]>` 副本语义与 `validate_image` | 架构演进 | 存量 | 09 §3 | `exec.rs` | 回答"Rust 如何表达共享映像" | 主讲述点 |

### 2.10 新 09（启动主线）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-09-01 | 引导自举问题（先把自己启动好） | 概念 | 存量 | 01 §1.1 | `main.c:158-494` | 回答"RS 的 boot 难在哪" | 主讲述点 |
| K-09-02 | 四步 boot 分解与每步产出 | 机制 | 存量 | 01 §2.3 | `main.c:239-243`（注释）、244/348/401/409 | 回答"boot 分几步、步与步的依赖" | 主讲述点 |
| K-09-03 | boot 三表内容与默认项语义 | 数据结构 | 存量 | 01 §2.7 | `table.c:15-50` | 回答"每个 boot 服务的初始属性" | 主讲述点 |
| K-09-04 | `boot_image_info_lookup` 四表查找与两种哨兵 | 机制 | 存量 | 01 §2.4 | `main.c:709-779`；`const.h:61-62` | 回答"属性如何按端点归并" | 主讲述点 |
| K-09-05 | boot 表条数交叉校验与 fail-fast | 约束不变量 | 存量 | 01 §2.3.2 | `main.c:200-227` | 回答"表不一致会怎样" | 主讲述点 |
| K-09-06 | 表复位与初值 | 机制 | 存量 | 01 §2.3.2 | `main.c:229-237` | 回答"boot 前表是什么状态" | 主讲述点 |
| K-09-07 | Step 1 全字段装配（跨引 02/03/04） | 机制 | 存量 | 01 §2.3.3 | `main.c:244-346` | 回答"一个 boot 槽位怎么装配出来" | 主讲述点；字段语义在各自机制篇 |
| K-09-08 | Step 2 放行序与同步/异步 ready 捕获 | 机制 | 存量 | 01 §2.3.4 | `main.c:348-399` | 回答"服务何时被允许运行" | 主讲述点；协议在 06 |
| K-09-09 | Step 3/4 与首个 alarm | 机制 | 存量 | 01 §2.3.5-2.3.6 | `main.c:401-434` | 回答"boot 如何收尾" | 主讲述点 |
| K-09-10 | `USE_LIVEUPDATE` 自升级块 | 机制 | 存量 | 01 §2.3.7；18 §2.1 | `main.c:436-491` | 回答"开 feature 时 boot 末尾多做什么" | 主讲述点；细节归 17 |
| K-09-11 | VM 异步 ready 例外（同步回复会死锁） | 约束不变量 | 存量 | 01 §2.5 | `main.c:809-815` | 回答"为什么只有 VM 不回复" | 主讲述点 |
| K-09-12 | `BootTables`/`BootInit` 状态机 | 架构演进 | 存量 | 01 §3.1、§3.4 | `table.rs`；`boot.rs` | 回答"四步顺序怎么用类型锁住" | 主讲述点 |
| K-09-13 | `sys_getimage`/`sys_getmachine`/`sys_setalarm` 依赖 | 接口协议 | 存量 | 01 §2.3.1、§2.3.6 | `main.c:53,196,433` | 回答"boot 依赖内核什么" | 主讲述点 |

### 2.11 新 10（主循环与消息分派）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-10-01 | 四类消息分类树 | 机制 | 存量 | 06 §1.2 | `main.c:70-121` | 回答"一条消息走哪个分支" | 主讲述点 |
| K-10-02 | notify 不回复；心跳时间戳写入 | 机制 | 存量 | 06 §2.1；07 §1.2 | `main.c:80-93` | 回答"心跳如何被记录" | 主讲述点 |
| K-10-03 | 15 臂分派表与未知消息 | 接口协议 | 存量 | 06 §2.1 | `main.c:100-122` | 回答"每个请求由谁处理" | 主讲述点 |
| K-10-04 | `EDONTREPLY` 与 `m_type = result` | 接口协议 | 存量 | 06 §2.1-2.2 | `main.c:124-128` | 回答"回复何时发出、内容是什么" | 主讲述点 |
| K-10-05 | `reply` 非阻塞且跳过 RS 自身端点 | 机制 | 存量 | 06 §2.2 | `utility.c:309-327` | 回答"为什么回复不会卡住 RS" | 主讲述点 |
| K-10-06 | idle period 两件事与关机覆盖 | 机制 | 存量 | 06 §2.5 | `utility.c:443-480` | 回答"RS 空闲时做什么" | 主讲述点 |
| K-10-07 | VM 单副本例外 | 约束不变量 | 存量 | 06 §2.5 | `utility.c:470-472` | 回答"为什么 VM 不补第二个副本" | 主讲述点 |
| K-10-08 | 未知识别告警与坏来源 panic 的边界 | 约束不变量 | 存量 | 06 §2.1、§2.3 | `main.c:64-66,88-91` | 回答"哪些异常是致命的" | 主讲述点 |
| K-10-09 | Rust `DispatchKind` 分类器 | 架构演进 | 存量 | 06 §3.1 | `dispatch.rs:99-136` | 回答"分类如何类型化" | 主讲述点 |

### 2.12 新 11（服务创建与实例切换）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-11-01 | 创建编排：`start_service` 四步与 `create_service` 十一步、顺序即契约 | 机制 | 存量 | 10 §1.1-1.2；12 §2.1（start_service） | `manager.c:950-983,531-707` | 回答"创建一个服务到底做几件事、为什么这个顺序" | 主讲述点 |
| K-11-02 | 三个前置闸门（NEED_REPL/NEED_COPY/空命令） | 约束不变量 | 存量 | 10 §2.1 | `manager.c:541-568` | 回答"什么情况下创建直接被拒" | 主讲述点 |
| K-11-03 | `srv_fork` + `getprocnr` + 槽位登记 | 机制 | 存量 | 10 §2.2 | `manager.c:574-597` | 回答"子进程怎么来、槽位如何绑定它" | 主讲述点 |
| K-11-04 | 权限提交回读与调度初始化两步 | 机制 | 存量 | 10 §2.3 | `manager.c:600-614` | 回答"内核侧进程何时获得权限与调度" | 主讲述点 |
| K-11-05 | 映像装载三连与 RS 重 pin | 机制 | 存量 | 10 §2.3 | `manager.c:619-644,570-573,636` | 回答"fork 后 RS 为什么要重钉内存" | 主讲述点 |
| K-11-06 | `setuid(0)` 的 PM→VFS 阻塞 hack（两处成对） | 机制 | 存量 | 10 §2.4；11 §2.2 | `manager.c:646-656,818` | 回答"为什么创建和发布都要强制阻塞通信" | 主讲述点 |
| K-11-07 | RS 实例 pin / VM 实例 `MAKE_VM` 与补 pin | 机制 | 存量 | 10 §2.4 | `manager.c:658-695` | 回答"RS 与 VM 的副本内存为什么必须驻留" | 主讲述点 |
| K-11-08 | `vm_set_priv` 收尾 | 接口协议 | 存量 | 10 §2.5 | `manager.c:698-702` | 回答"VM 怎么知道这个进程能调什么" | 主讲述点 |
| K-11-09 | `clone_service` 六步 | 机制 | 存量 | 10 §2.6 | `manager.c:713-782` | 回答"副本怎么造" | 主讲述点 |
| K-11-10 | VM 单副本限制与先清理 | 约束不变量 | 存量 | 10 §2.6 | `manager.c:727-734` | 回答"VM 的副本为什么特殊" | 主讲述点 |
| K-11-11 | LU/副本两套链方向 | 数据结构 | 存量 | 10 §2.6 | `manager.c:742-756` | 回答"new/old 与 next/prev 各自服务谁" | 主讲述点 |
| K-11-12 | RS 副本的备份信号管理器 | 机制 | 存量 | 10 §2.6；18 §2.6 | `manager.c:765-779` | 回答"RS 挂掉时信号谁接" | 主讲述点 |
| K-11-13 | `activate_service` 的 ACTIVE 位迁移 | 机制 | 存量 | 10 §2.7 | `manager.c:1013-1028` | 回答"哪个实例算'当前服务'" | 主讲述点 |
| K-11-14 | `clone_slot` 浅拷贝 + 深修正 | 机制 | 存量 | 10 §2.8 | `manager.c:1800-1851` | 回答"克隆槽位必须重置什么" | 主讲述点 |
| K-11-15 | `swap_slot` 八步整槽交换 | 机制 | 存量 | 10 §2.9 | `manager.c:1870-1930` | 回答"新旧实例如何原子换位" | 主讲述点 |
| K-11-16 | `update_service`：`srv_update` + 槽交换 + pid/端点重排 | 机制 | 存量 | 16 §2.3；18 §2.5 | `update.c:230-325` | 回答"重启与 LU 共用的切换动作" | 主讲述点（从 16 前移） |
| K-11-17 | 创建失败的回滚边界 | 约束不变量 | 存量 | 10 §3.5 | `manager.c:550,558,566,603-701` | 回答"失败时谁来清理" | 主讲述点 |
| K-11-18 | Rust 值语义克隆与索引交换 | 架构演进 | 存量 | 10 §3.3-3.4 | `service_create.rs`；`process_table.rs` | 回答"A-3 如何落地" | 主讲述点 |

### 2.13 新 12（服务发布与撤销）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-12-01 | 发布 = 向四个外部目录登记 | 概念 | 存量 | 11 §1.1 | `manager.c:787-858` | 回答"发布到底做了什么" | 主讲述点 |
| K-12-02 | 任一步失败即 `kill_service` | 约束不变量 | 存量 | 11 §1.1 | `manager.c:800-853` | 回答"注册失败怎么办" | 主讲述点 |
| K-12-03 | DS label 无条件注册与覆盖语义 | 机制 | 存量 | 11 §2.1 | `manager.c:800-803` | 回答"服务怎么被按名字找到" | 主讲述点 |
| K-12-04 | mapdriver 触发条件（`dev_nr>0` 或 `nr_domain>0`） | 机制 | 存量 | 11 §2.2 | `manager.c:805-824` | 回答"什么算驱动服务" | 主讲述点 |
| K-12-05 | PCI ACL 发布（`USE_PCI` 条件编译） | 机制 | 存量 | 11 §2.3 | `manager.c:826-838` | 回答"PCI 设备授权怎么下发" | 主讲述点；A-10 fail-closed |
| K-12-06 | devman bind/unbind 三步 | 接口协议 | 存量 | 11 §2.4-2.5 | `manager.c:840-853,897-911` | 回答"设备绑定怎么建立与撤销" | 主讲述点 |
| K-12-07 | 撤销的宽松语义与错误聚合 | 机制 | 存量 | 11 §2.5、§3.4 | `manager.c:864-917` | 回答"撤销失败会怎样" | 主讲述点 |
| K-12-08 | Rust 四纯谓词与 PCI fail-closed | 架构演进 | 存量 | 11 §3 | `publish.rs` | 回答"发布判定如何可测" | 主讲述点 |

### 2.14 新 13（存活监控与故障恢复）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-13-01 | RS 是看门狗：内核不检测服务死亡 | 概念 | 存量 | 07 §1.0-1.1 | `request.c:943-1046` | 回答"服务挂了谁先发现" | 主讲述点 |
| K-13-02 | 心跳双时间戳协议 | 数据结构 | 存量 | 07 §1.2 | `type.h:72-73`；`main.c:87`；`request.c:1037` | 回答"存活/检查两个时间戳怎么配合" | 主讲述点 |
| K-13-03 | `do_period` 三分支状态机 | 机制 | 存量 | 07 §2.1 | `request.c:975-1038` | 回答"每个 tick 对每个服务做什么判断" | 主讲述点 |
| K-13-04 | 时间常数族与有效 period 计算 | 数据结构 | 存量 | 07 §2.2 | `const.h:48-51,58,116` | 回答"超时窗口各是多少" | 主讲述点 |
| K-13-05 | free pass 例外 | 机制 | 存量 | 07 §2.3 | `request.c:1011-1022` | 回答"为什么有时不杀迟到的服务" | 主讲述点 |
| K-13-06 | `RS_NOPINGREPLY` 终态标记 | 约束不变量 | 存量 | 07 §2.3 | `const.h:31`；`request.c:1006,1024` | 回答"为什么不会反复杀同一个服务" | 主讲述点 |
| K-13-07 | SIGTERM→SIGKILL 升级 | 机制 | 存量 | 07 §2.1 | `request.c:985-989`；`manager.c:1007` | 回答"停止请求没人理怎么办" | 主讲述点 |
| K-13-08 | `update_period` prepare 超时 | 机制 | 存量 | 07 §2.6；16 §2.9 | `update.c:371-396` | 回答"LU 卡住谁来收尾" | 主讲述点；状态机归 16 |
| K-13-09 | `do_sigchld` 子进程回收 | 机制 | 存量 | 07 §2.5 | `request.c:1051-1090` | 回答"别家信号管理器负责的进程死了怎么办" | 主讲述点 |
| K-13-10 | `terminate_service` 决策树 | 机制 | 存量 | 15 §2.1 | `manager.c:1055-1180` | 回答"服务死了之后根据什么决定处置" | 主讲述点 |
| K-13-11 | `cleanup_service` 两阶段协议 | 机制 | 存量 | 15 §2.2 | `manager.c:405-492` | 回答"清理为什么分两次、各自做什么" | 主讲述点 |
| K-13-12 | `kill_service` vs `crash_service` | 机制 | 存量 | 15 §2.3 | `manager.c:360-400` | 回答"处决与模拟崩溃的区别" | 主讲述点 |
| K-13-13 | `detach_service` 降级旁路 | 机制 | 存量 | 15 §2.4 | `manager.c:497-526` | 回答"如何把一个服务移出管理" | 主讲述点 |
| K-13-14 | `restart_service`/`reincarnate_service` 两条恢复路径 | 机制 | 存量 | 15 §2.5 | `manager.c:1246-1298,1033-1050` | 回答"脚本恢复与直接重启各怎么做" | 主讲述点 |
| K-13-15 | `run_script` 与 A-1 缺口 | 机制 | 存量 | 15 §2.6 | `manager.c:1185-1241` | 回答"恢复脚本收到什么、怎么被拉起" | 主讲述点 |
| K-13-16 | 二进制指数退避公式 | 机制 | 存量 | 15 §2.1；07 §2.1 | `manager.c:1164-1178` | 回答"反复崩溃时重试间隔怎么增长" | 主讲述点 |
| K-13-17 | 核心服务死亡导致 RS `_exit(1)` | 约束不变量 | 存量 | 15 §2.1 | `manager.c:1119-1124` | 回答"什么故障是 RS 救不了的" | 主讲述点 |
| K-13-18 | 信号分类集合（LETHAL/TERMINATION/STACKTRACE） | 数据结构 | 存量 | 06 §2.6 | `sys/sys/signal.h:280-286` | 回答"哪些信号算终止" | 主讲述点 |
| K-13-19 | `sys_diagctl_stacktrace` 栈回溯 | 机制 | 存量 | 06 §2.6 | `main.c:681-683` | 回答"崩溃时怎么拿到现场" | 主讲述点 |
| K-13-20 | Rust 决策切片（monitor/recovery） | 架构演进 | 存量 | 07 §3；15 §3 | `monitor.rs`；`recovery.rs` | 回答"故障决策如何可测" | 主讲述点 |

### 2.15 新 14（控制请求）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-14-01 | 控制请求公共骨架 | 机制 | 存量 | 13 §1.1 | `request.c:15-457` | 回答"每个 handler 的共同结构" | 主讲述点 |
| K-14-02 | `do_up` 全流程 | 机制 | 存量 | 13 §2.1 | `request.c:15-106` | 回答"启动请求怎么走完创建链" | 主讲述点 |
| K-14-03 | 强制 init 旗标映射 | 机制 | 存量 | 13 §2.1 | `request.c:49-61`；`rs.h:42-45` | 回答"调试注入口是什么" | 主讲述点 |
| K-14-04 | 重复性检查三种 | 机制 | 存量 | 13 §2.1 | `request.c:70-87` | 回答"什么时候拒绝新服务" | 主讲述点 |
| K-14-05 | 回复双模式（NOBLOCK vs LATEREPLY） | 机制 | 存量 | 13 §2.1 | `request.c:95-105` | 回答"调用者何时被解锁" | 主讲述点 |
| K-14-06 | `do_down` 与已终止快路径 | 机制 | 存量 | 13 §2.2 | `request.c:111-155` | 回答"停止请求怎么处理正在死的服务" | 主讲述点 |
| K-14-07 | `do_restart` 仅限恢复脚本、脚本保护 | 机制 | 存量 | 13 §2.3 | `request.c:160-203` | 回答"谁能触发重启、如何防递归" | 主讲述点 |
| K-14-08 | `do_clone`/`do_unclone` 与 `SF_USE_REPL` | 机制 | 存量 | 13 §2.4 | `request.c:208-293` | 回答"副本策略怎么开关" | 主讲述点 |
| K-14-09 | `do_edit` 八步重配置 | 机制 | 存量 | 13 §2.5 | `request.c:298-385` | 回答"运行中改配置做哪些事" | 主讲述点 |
| K-14-10 | `do_refresh` | 机制 | 存量 | 13 §2.6 | `request.c:390-426` | 回答"刷新与停止的区别" | 主讲述点 |
| K-14-11 | `do_shutdown` 全局关机 | 机制 | 存量 | 13 §2.6 | `request.c:431-457` | 回答"关机怎么让所有服务进入不重启状态" | 主讲述点 |
| K-14-12 | `stop_service` 原语 | 机制 | 存量 | 13 §2.7 | `manager.c:988-1008` | 回答"停止一个服务的标准动作" | 主讲述点 |
| K-14-13 | `mark_late_reply` 三字段原子写 | 约束不变量 | 存量 | 13 §3.3 | `request.c:101-103` | 回答"延迟回复登记为什么必须一起写" | 主讲述点 |
| K-14-14 | Rust `StopSignal` 与决策化 stop | 架构演进 | 存量 | 13 §3 | `request.rs` | 回答"停止决策如何纯化" | 主讲述点 |

### 2.16 新 15（查询请求）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-15-01 | 观测与注入两个面 | 概念 | 存量 | 14 §1.0-1.1 | `request.c:1095-1260` | 回答"谁可以看、谁可以注入故障" | 主讲述点 |
| K-15-02 | `do_lookup` 与 100 字节名缓冲 | 机制 | 存量 | 14 §2.1 | `request.c:1144-1176` | 回答"名字到端点怎么解析" | 主讲述点 |
| K-15-03 | `do_getsysinfo` 三种模式/root-only/尺寸精确 | 机制 | 存量 | 14 §2.2 | `request.c:1095-1139` | 回答"进程表怎么导出、为什么必须整表" | 主讲述点 |
| K-15-04 | `do_sysctl` 五子操作与无权限检查的事实 | 机制 | 存量 | 14 §2.3 | `request.c:1181-1224` | 回答"运维控制入口有哪些" | 主讲述点 |
| K-15-05 | `UPD_RUN` 的 LATEREPLY | 机制 | 存量 | 14 §2.3；16 §2.9 | `request.c:1204-1207` | 回答"运行更新何时回复" | 主讲述点 |
| K-15-06 | `do_fi` 故障注入 | 机制 | 存量 | 14 §2.4 | `request.c:1229-1260`；`utility.c:69-77` | 回答"怎么让服务自己崩溃" | 主讲述点 |
| K-15-07 | 状态打印族 | 机制 | 存量 | 14 §2.5 | `utility.c:142-218,485-546` | 回答"运维能看到什么" | 主讲述点；未来下放 15-stage? 见 §3 越界 |
| K-15-08 | Rust `GetsysinfoTable` 与分类 | 架构演进 | 存量 | 14 §3 | `query.rs` | 回答"查询分类如何类型化" | 主讲述点 |

### 2.17 新 16（Live Update：状态机与状态迁移）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-16-01 | LU 不变量：任何时刻有可用版本 | 概念 | 存量 | 16 §1.1 | `update.c:932-1010` | 回答"LU 与重启的本质差别" | 主讲述点 |
| K-16-02 | 四阶段状态机与 `UpdatePhase` | 机制 | 存量 | 16 §1.2 | `const.h:105-113`；`update.c:865` | 回答"一次 LU 的生命周期" | 主讲述点；ARCH A-6 |
| K-16-03 | `do_update` 十步入口 | 机制 | 存量 | 16 §2.1 | `request.c:534-889` | 回答"更新请求从哪进来" | 主讲述点 |
| K-16-04 | 旗标解码与 VM 默认 mmap 预分配 | 机制 | 存量 | 16 §2.1 | `request.c:574-623`；`const.h:83` | 回答"`rss_flags` 怎么变成 LU 旗标" | 主讲述点 |
| K-16-05 | 纯校验五查 | 约束不变量 | 存量 | 16 §2.1 | `request.c:648-686` | 回答"哪些更新请求直接拒绝" | 主讲述点 |
| K-16-06 | rpupd 链的部分排序插入 | 机制 | 存量 | 16 §2.2 | `update.c:23-86` | 回答"多组件更新为什么普通→VM→RS" | 主讲述点 |
| K-16-07 | 旗标全链传播与 VM/RS 指针 | 机制 | 存量 | 16 §2.2 | `update.c:64-82` | 回答"后加入的组件如何影响整条链" | 主讲述点 |
| K-16-08 | 链清空/初始化/移动 | 机制 | 存量 | 16 §2.2 | `update.c:7-21,121-183` | 回答"描述符生命周期" | 主讲述点 |
| K-16-09 | `rollback_service` 回滚 | 机制 | 存量 | 16 §2.3；18 §2.4 | `update.c:330-366` | 回答"更新失败怎么退回旧版本" | 主讲述点 |
| K-16-10 | `srv_update` 三分支与 VM multi 时序 | 机制 | 存量 | 16 §2.3；18 §2.5 | `update.c:230-256` | 回答"VM 在更新里为什么特殊" | 主讲述点 |
| K-16-11 | prepare 阶段与 `RS_LU_PREPARE` 请求 | 机制 | 存量 | 16 §2.4 | `update.c:401-527,185-225` | 回答"更新前为什么先让服务准备" | 主讲述点 |
| K-16-12 | `start_update` 与初始化窗口 | 机制 | 存量 | 16 §2.5 | `update.c:532-620` | 回答"切换何时发生" | 主讲述点；A-9 |
| K-16-13 | `start_srv_update`/`complete_srv_update` | 机制 | 存量 | 16 §2.6 | `update.c:621-706` | 回答"单个组件更新的两半" | 主讲述点 |
| K-16-14 | `abort_update_proc` 三阶段分派 | 机制 | 存量 | 16 §2.7 | `update.c:707-739` | 回答"中止更新在不同阶段各做什么" | 主讲述点 |
| K-16-15 | `end_update` 族四角色与反向遍历 | 机制 | 存量 | 16 §2.8 | `update.c:744-931` | 回答"结束更新时每个组件按什么角色收尾" | 主讲述点 |
| K-16-16 | `end_srv_update` 存活/退场选择 | 机制 | 存量 | 16 §2.8 | `update.c:932-1011` | 回答"成功与失败各清理哪个版本" | 主讲述点 |
| K-16-17 | `init_state_data` 总流程 | 机制 | 存量 | 17 §2.1 | `manager.c:174-284` | 回答"旧实例的状态怎么搬到新实例" | 主讲述点 |
| K-16-18 | `rs_state_data`/IPC filter 线格式 | 数据结构 | 存量 | 17 §2.2 | `rs.h:88-100`；`ipc_filter.h` | 回答"状态数据长什么样" | 主讲述点；A-14 |
| K-16-19 | label 四级回退解析（DS→ANY_*→十进制→ESRCH） | 机制 | 存量 | 17 §2.4 | `manager.c:246-266` | 回答"IPC filter 里的名字怎么变端点" | 主讲述点 |
| K-16-20 | VM 保底 filter 条目 | 机制 | 存量 | 17 §2.5 | `manager.c:273-277` | 回答"VM 更新窗口内如何仍能联系 RS" | 主讲述点 |
| K-16-21 | 状态数据的 grant 生命周期 | 机制 | 存量 | 17 §2.6 | `request.c:820-847`；`update.c:135-163` | 回答"谁授权、谁撤销、何时释放" | 主讲述点 |
| K-16-22 | LU 回复旗标三值 `RS_DONTREPLY/RS_REPLY/RS_CANCEL` | 数据结构 | 新增 | 16 §3.1 提及 | `const.h:74-76`；`update.c:952-956,981-990` | 回答"结束更新时回复策略如何选" | 新增（旧 16 有散点，未成节） |
| K-16-23 | ASR 更新旗标与 RS 侧边界 | 机制 | 新增 | 16 §1.1 一句带过 | `rs.h:39`；`sef.h:236,244`；`type.h:65`；`manager.c:1694-1696`；`lib/libmagicrt/magic_util.c:82`；`fs/procfs/service.c:347` | 回答"ASR 更新 RS 到底负责什么" | 新增，显式边界 |
| K-16-24 | `SF_VM_UPDATE/ROLLBACK/NOMMAP` 与 VM 交互旗标 | 数据结构 | 存量 | 16 §2.4；18 §2.5 | `update.c:442-449,637-639,925` | 回答"哪些旗标告诉 VM 怎么搬内存" | 主讲述点 |
| K-16-25 | Rust `UpdateChain`/`UpdatePhase`/镜像同步 | 架构演进 | 存量 | 16 §3；17 §3 | `live_update.rs`；`state_data.rs` | 回答"指针链与位旗标如何类型化" | 主讲述点 |

### 2.18 新 17（RS 自身生命周期）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-17-01 | RS 是"自己的 RS"：自引用特例总览 | 概念 | 存量 | 18 §1.0-1.2 | `main.c:436` | 回答"为什么 RS 的更新与重启需要特例" | 主讲述点 |
| K-17-02 | boot 自升级的角色分派（pid==0） | 机制 | 存量 | 18 §2.1 | `main.c:436-491` | 回答"两个 RS 实例谁接管" | 主讲述点 |
| K-17-03 | `sef_cb_init_restart` 流程 | 机制 | 存量 | 18 §2.2 | `main.c:499-544` | 回答"RS 被重启时做什么" | 主讲述点 |
| K-17-04 | `sef_cb_init_lu` 流程与四断言 | 机制 | 存量 | 18 §2.3 | `main.c:549-586` | 回答"RS 被热更新时做什么" | 主讲述点 |
| K-17-05 | RS 回滚特例（`sys_whoami` + `vm_update`） | 机制 | 存量 | 18 §2.4 | `update.c:330-366` | 回答"新 RS 初始化失败怎么退" | 主讲述点 |
| K-17-06 | 信号管理器备份对 | 机制 | 存量 | 18 §2.6 | `utility.c:387-414`；`manager.c:765-779` | 回答"RS 不可用时信号谁处理" | 主讲述点 |
| K-17-07 | RS YIELD 交接 | 机制 | 存量 | 16 §2.6；18 §2.3 | `update.c:673-689`；`main.c:485` | 回答"控制权如何交给新实例" | 主讲述点 |
| K-17-08 | 两实例模型与端点/pid 互换 | 机制 | 存量 | 18 §2.9b | `main.c:450-454`；`update.c:299-305` | 回答"为什么 RS 槽位与进程不是一一对应不变" | 主讲述点 |
| K-17-09 | RS 自身结束路径（`_exit(1)`/`EDEADEPT`/SIGTERM） | 约束不变量 | 存量 | 18 §3.3；15 §2.1 | `update.c:883-887`；`main.c:631-642` | 回答"RS 什么时候选择退出" | 主讲述点 |
| K-17-10 | Rust `self_lifecycle` 纯切片 | 架构演进 | 存量 | 18 §3 | `self_lifecycle.rs` | 回答"特例如何类型化" | 主讲述点 |

### 2.19 新 18（外部接口契约）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-18-01 | 单一签名权威面的理由 | 概念 | 存量 | 19 §1.0-1.1 | 全篇设计 | 回答"为什么其它篇不重复写签名" | 主讲述点 |
| K-18-02 | 依赖面总图 | 概念 | 存量 | 19 §1.2 | 全篇 | 回答"RS 依赖谁、谁依赖 RS" | 主讲述点 |
| K-18-03 | `sys_*` 内核面契约表 | 接口协议 | 存量 | 19 §2.1 | `main.c:53,181,196,433`；`utility.c:266,402,412`；`update.c:243,342` | 回答"RS 调用内核的完整清单" | 主讲述点 |
| K-18-04 | `srv_*` PM 面契约表 | 接口协议 | 存量 | 19 §2.2 | `main.c:446,451`；`manager.c:470,576,584,634`；`request.c:1063` | 回答"进程生命周期调用清单" | 主讲述点 |
| K-18-05 | `vm_*` VM 面契约表与 `VM_RS_MEM_*` | 接口协议 | 存量 | 19 §2.3 | `request.c:361,779,802`；`main.c:470`；`update.c:249,505`；`com.h:741-745` | 回答"内存与更新调用清单" | 主讲述点 |
| K-18-06 | `ds_*` DS 面契约 | 接口协议 | 存量 | 19 §2.4 | `manager.c:247,513,800,841,878` | 回答"名字与数据存储调用清单" | 主讲述点 |
| K-18-07 | `sched_*` 调度面契约 | 接口协议 | 存量 | 19 §2.5 | `utility.c:375`；`request.c:342`；`manager.c:461` | 回答"调度调用清单" | 主讲述点 |
| K-18-08 | libexec/mapdriver/PCI/devman 契约表 | 接口协议 | 存量 | 19 §2.6 | `exec.c:17,34,49`；`manager.c:820,833,846,889,903` | 回答"装载与驱动注册调用清单" | 主讲述点 |
| K-18-09 | 消息槽类型化 `mess_rs_*`/`mess_lsys_*` | 数据结构 | 存量 | 19 §3.1-3.2 | `ipc.h:1858-1906,1048-1072,1420-1428,1466-1474` | 回答"union 载荷怎么安全解码" | 主讲述点 |
| K-18-10 | 56/64 字节消息快照不一致 | 约束不变量 | 存量 | 19 §3.3 | `ipc.h:2672-2675`；`ipcconst.h:17-19` | 回答"线格式尺寸为什么有差异" | 主讲述点 |
| K-18-11 | `ipc_sendrec` 与 `asynsend` 的使用边界 | 机制 | 新增 | — | `manager.c:849,906`（阻塞）；`utility.c:231`（异步） | 回答"哪些外部调用能阻塞 RS" | 新增，补一句话规则 |
| K-18-12 | RS↔VM 启动握手（公开表 grant 的消费侧） | 接口协议 | 新增 | 02 §2.8（只写创建点） | `main.c:185-189`；`02-stage-vm/25-rs-services.md` §1.5 | 回答"VM 怎么拿到全系统进程表" | 新增，跨 stage 对账 |
| K-18-13 | Rust minix-types/minix-sys 接缝 | 架构演进 | 存量 | 19 §4 | `minix-types/src/ipc/rs.rs`；`minix-sys` | 回答"Rust 侧实现边界在哪" | 主讲述点 |

### 2.20 新 99（常量、错误与诊断词典）知识点

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 备注 |
|------|------|------|------|---------|------|---------|------|
| K-99-01 | 词典定位与"同一定义只有一处权威"规则 | 概念 | 存量 | 99 §1 | 全篇 | 回答"常量该去哪查" | 主讲述点 |
| K-99-02 | 15 个 `RS_*` 消息类型与 10–19 空隙 | 数据结构 | 存量 | 99 §2.1 | `com.h:463-492` | 回答"消息号是多少" | 主讲述点 |
| K-99-03 | 时间与状态机常量 | 数据结构 | 存量 | 99 §2.2 | `servers/rs/const.h:25,48-51,58,75-76,79-80,83` | 回答"周期/退避/上限是多少" | 主讲述点 |
| K-99-04 | `RSS_*` 20 个请求旗标 | 数据结构 | 存量 | 99 §2.3 | `include/minix/rs.h:33-52` | 回答"请求能带哪些旗标" | 主讲述点 |
| K-99-05 | `SF_*` 13 个系统旗标 | 数据结构 | 存量 | 99 §2.3 | `include/minix/rs.h:191-203` | 回答"服务策略位含义" | 主讲述点 |
| K-99-06 | `SEF_INIT_*`/`SEF_LU_*`/`SEF_LU_STATE_*` | 数据结构 | 存量 | 99 §2.3 | `include/minix/sef.h:93-101,213-242` | 回答"初始化/更新旗标全表" | 主讲述点 |
| K-99-07 | `IPCF_*` 与 `ANY_USR/SYS/TSK` | 数据结构 | 存量 | 99 §2.3 | `include/minix/ipc_filter.h` | 回答"IPC filter 旗标与特殊来源" | 主讲述点 |
| K-99-08 | `SYS_PRIV_*`/`SYS_STATE_*` 操作码 | 数据结构 | 存量 | 99 §2.4 | `com.h:342-353,442-446` | 回答"内核操作码全表" | 主讲述点 |
| K-99-09 | 端点常量（PM/VFS/RS/VM/INIT…） | 数据结构 | 存量 | 99 §2.4 | `com.h:55-67`；`endpoint.h` | 回答"固定端点是几" | 主讲述点 |
| K-99-10 | `VM_RS_MEM_*`/`VM_RS_UPDATE` | 数据结构 | 存量 | 99 §2.4 | `com.h:627,736,741-745` | 回答"VM 子操作码" | 主讲述点 |
| K-99-11 | init/lu 错误表 | 数据结构 | 存量 | 99 §2.5 | `error.c:15-28,48,56` | 回答"`init_strerror`/`lu_strerror` 覆盖哪些" | 主讲述点；诊断输出归 15 |
| K-99-12 | 常量权威映射（每个常量在 Rust 的唯一落点） | 约束不变量 | 存量 | 99 §3.1 | `minix-types` 各模块 | 回答"Rust 从哪 import" | 主讲述点 |
| K-99-13 | hz 相关公式不硬编码 | 约束不变量 | 存量 | 99 §3.2 | `main.c:181`；`const.h:48-49` | 回答"时间常量为什么是函数" | 主讲述点 |
| K-99-14 | `Errno(i32)` newtype 与 A-12 | 架构演进 | 存量 | 99 §4 | `minix-types/src/types/errno.rs` | 回答"错误怎么跨模块统一" | 主讲述点 |
| K-99-15 | 显式排除项：`RS_USE_PAGING`、`DEBUG`/`PRIV_DEBUG`、C 构建文件 | 约束不变量 | 存量 | plan §5.4 | `servers/rs/const.h:6-15,84`；`Makefile` | 回答"为什么这些不建模" | 主讲述点 |

### 2.21 池统计与重复标记

- **总条数 266 条**（脚本对账：§2 表内 `^| K-` 行 266，唯一编号 266）：存量 252 条，新增 14 条（K-01-08、K-01-12、K-07-01…K-07-04、K-07-07、K-07-08、K-07-10、K-07-11、K-16-22、K-16-23、K-18-11、K-18-12；其中 K-07 组 8 条属同一新主题）。
- **按类型分布**（脚本对账）：机制 135、数据结构 34、约束不变量 30、接口协议 23、概念 21、架构演进 20、工具工程 3、测试 0（测试条目在各篇验收标准里表述，不单列池条目；旧文档中的测试计数属易腐信息，重建时不进正文池）。
- **按现有位置分布**（脚本对账，含 11 条无旧位置的纯新增与 1 条只引 plan 的组织型条目）：old 00×9、01×19、02×18、03×15、04×7、05×9、06×15、07×10、08×16、09×6、10×17、11×8、12×12、13×14、14×8、15×8、16×22、17×5、18×9、19×11、99×15、plan×1、—×11。
- **合并说明**：同一知识点在旧目录多处的，池内只保留一条并在备注标"主讲述点"。典型合并：`fill_send_mask`（03 §2.5 定义 / 05 §2.1 引用）合并为 K-03-09（send mask 语义归 K-04-01）；`rs_isokendpt`（02 §2.10 / 06 §2.3）合并为 K-02-15；`update_service`（10 §3.2 提及 / 16 §2.3 / 18 §2.5）合并为 K-11-16；`do_upd_ready`（12 §2.4 / 16 §2.8）合并为 K-06-09；`end_srv_init`（12 §2.6 / 16 §2.8）合并为 K-06-11；`signal handler`（01 §1.3 / 06 §2.6 / 18 §3.3）合并为 K-01-10（注册）+ K-13-18/19（行为）。
- **删除条目**：无。旧文档中所有实质知识点都有去向；仅以下两类不进入新正文：① 测试计数与 "Fix #NN / 接线落地" 过程叙述（易腐，删除，理由见 §6 删除条款）；② 旧文档对隐藏设计目录的引用（违规，删除）。

---

## 3. 覆盖审计

### 3.1 主题全集与来源

主题全集由四路合成，再与知识点池、旧文档对账：

1. **C 源码符号**（逐文件核对，覆盖 121 个函数）：`main.c` 入口/SEF/四步 boot/四表查找/ready 捕获/信号回调；`manager.c` 槽位原语/访问控制/创建/克隆/交换/发布/清理/终止/重启/脚本/继承/状态数据/发送掩码；`request.c` 13 个请求 handler/周期检查/信号子进程；`update.c` 全 LU 状态机；`utility.c` init_service/fi/掩码原语/字符串/回复/isokendpt/调度/信号管理器/idle/打印；`exec.c` 装载链；`error.c` 错误表；`table.c` 三张 boot 表。
2. **操作系统通用概念**：root system process 与 boot 链自举、服务注册表、权限白名单、IPC 可达性、事件循环、心跳/看门狗、两阶段清理、二进制指数退避、原子实例切换（A/B 更新）、状态迁移、多组件协同更新、故障注入、单线程服务与阻塞边界。
3. **非 C 制品主题**：`system.conf` 声明语言、`minix-service` 客户端与命令族、`rs_start` 线格式、boot image 表与镜像、SEF 运行库的请求拦截循环、`USE_LIVEUPDATE`/`USE_PCI` 构建开关、Cargo `live-update` feature、测试基建（Rust 单测与 QEMU 冒烟）。
4. **阶段边界契约**：`../00-master-plan/README.md` 的阶段划分；`../edge_todo.md` 的 E-RSWIRE（RS 生产接线五域面）、E-RSSTART（rs_start 字节 ABI）、E-ISBOOT（RS 动态加载 IS）、E9（联调验收）；`../02-stage-vm/25-rs-services.md` 的 VM 侧 RS 服务契约。

### 3.2 覆盖缺口表

| 编号 | 主题 | 为什么重要 | 原料与锚点 | 建议落实为 |
|------|------|-----------|-----------|-----------|
| G-01 | `system.conf` 声明语言与指令语义 | RS 的请求从哪来是控制面的起点；旧目录零覆盖 | `minix3/etc/system.conf:1-529`；`commands/minix-service/parse.c` | **新建篇章 07**（K-07-01/02/08） |
| G-02 | `minix-service` 客户端：声明 → `rs_start` → `RS_UP` | 没有它，07 的字段就是无源之水；`service` 命令族是运维入口 | `parse.c:1229`；`minix-service.c:756`；`rs.h:104-151` | **新建篇章 07**（K-07-04/05/06/07） |
| G-03 | boot 服务配置双源事实 | 读者会假设 `system.conf` 的 priority/quantum 在 boot 生效，实际 boot 用 `SRV_Q/SRV_QT` | `main.c:319-322`；`priv.h:93-100`；`table.c:15-50` vs `system.conf:21-22` | 07 §3 与 09 §2 显式对比（新增 K-09-14 并入 K-09-07 的事实底线） |
| G-04 | SEF 运行库内部：请求拦截循环与 init 分派 | 旧目录只讲"注册回调"，不讲 SEF 何时调用；是"main 之前发生什么"的答案 | `lib/libsys/sef.c:185-230`；`sef_init.c`（`do_sef_init_request`） | 01 §2（K-01-12） |
| G-05 | SEF ping 拦截（心跳服务侧） | 旧 07 说"服务回复心跳"但没有服务侧机制；SEF 库才是应答者 | `sef_ping.c:21-38`；`sef.c:212` | 01 §4；13 引用（K-01-08） |
| G-06 | ASR 更新的 RS 侧职责边界 | 旧文档只有旗标散点，读者会高估 RS 作用；实际语义在 SEF/libmagicrt | `rs.h:39`；`sef.h:236,244`；`type.h:65`；`magic_util.c:82`；`procfs/service.c:347` | 16 §6（K-16-23） |
| G-07 | RS 自身关机路径全貌 | `do_shutdown`、`shutting_down` 对 cleanup/unpublish 的例外、core death `_exit(1)`、RS SIGTERM 散在四处 | `request.c:431-457`；`manager.c:1119-1124`；`utility.c:449-464`；`manager.c:879,890`；`main.c:638-640` | 14 §（控制）+ 13 §（收尾）各一节；K-14-11/K-13-17 |
| G-08 | RS↔VM 启动握手（grant 消费侧） | 公开表 grant 是 RS 与 VM 的跨模块契约，旧 02 只写创建点 | `main.c:185-189`；`02-stage-vm/25-rs-services.md` §1.5/§2.7 | 18 §（外部契约）一节（K-18-12） |
| G-09 | 信号分类与栈回溯的完整集合 | 旧 06 只列 `SIGS_IS_TERMINATION`，漏 `SIGS_IS_LETHAL`/`SIGS_IS_STACKTRACE` 的差别与 VM 豁免原因 | `sys/sys/signal.h:280-286`；`main.c:681-696` | 13 §（K-13-18/19） |
| G-10 | 诊断输出族（srv_to_string/print_*） | 旧 14 有散点但无归属；`procfs` 还在消费 `r_asr_count` | `utility.c:142-218,485-546`；`fs/procfs/service.c:347` | 15 §（K-15-07），并注明未来可能下放（§3.4 越界项） |
| G-11 | `error.c` 错误表与错误命名面 | 旧 99 有表，但没有和 `Errno`/输出族连起来 | `error.c:15-28,48,56` | 99 §4（K-99-11/14） |
| G-12 | LU 回复旗标三值语义 | `end_update`/`end_srv_update` 的 `reply_flag` 三值决定谁收到回复，旧 16 只散点 | `const.h:74-76`；`update.c:952-956,981-990` | 16 §（K-16-22） |
| G-13 | VM 交互旗标三兄弟 | `SF_VM_UPDATE/SF_VM_ROLLBACK/SF_VM_NOMMAP` 的置位与清除点分散 | `update.c:442-449,637-639,925` | 16 §（K-16-24） |
| G-14 | 多变组件 LU 的 VM 时序 | `rs_receive_ticks` 超时等待 + VM 先 init + 其余 complete + VM 取消回复，是 LU 最难的一段 | `update.c:483-508,585-613,952-956` | 16 §（并入 K-16-10/12/16） |
| G-15 | heap/mmap 预分配与 `r_map_prealloc_*` | 旧 16 只提一句；这是 VM 更新不丢内存的关键 | `request.c:591-599,768-810`；`const.h:83`；`utility.c:56-60` | 16 §（K-16-04 展开） |
| G-16 | 操作员视角的 LU 控制面 | `RS_SYSCTL_UPD_START/RUN/STOP/STATUS` 是运维入口，旧 14 只有表 | `request.c:1189-1215`；`utility.c:516-546` | 15 §（K-15-04/05） |
| G-17 | 入口装载链（crt0→sef_startup） | 旧文档从 `main()` 讲起，`_start` 到 `main` 之间是空白 | `lib/libsys/sef.c:185-230` | 01 §2（K-01-12） |
| G-18 | 死配置与调试宏的显式排除 | `RS_USE_PAGING=0`、`DEBUG`/`PRIV_DEBUG` 需要明确"为什么不讲" | `servers/rs/const.h:6-15,84`；`plan.md` §5.4 | 99 §7.1（K-99-15） |
| G-19 | 驱动面 privctl 操作与 `update_priv` 内核规则 | 旧 03 §2.6 有半页，但"RS 不用哪些、为什么"没有结论 | `do_privctl.c:187-251,280-368`；A-10 | 03 §（K-03-08/14） |
| G-20 | 状态数据协议上限常量 | `RS_MAX_IPC_FILTERS`/`RS_MAX_IPCF_STR_LEN`/`IPCF_MAX_ELEMENTS` 决定缓冲大小与失败语义 | `rs.h:58-59`；`sys_config.h:9`；`manager.c:178-232` | 16 §（K-16-18/21） |
| G-21 | 阻塞与非阻塞 IPC 的使用规则 | `mapdriver`/devman 用 `sendrec`（会阻塞 RS），prepare 用 asynsend；旧文档没有这条规则 | `manager.c:849,906`；`utility.c:231` | 18 §（K-18-11） |
| G-22 | PM 查询面差异（getnuid/getnpid/getprocnr） | 旧 04/19 混用说法，实际前两者 `PM_GETEPINFO`、后者 `PM_GETPROCNR` | `lib/libsys/getepinfo.c`；`lib/libsys/getprocnr.c` | 18 §（K-18-03） |
| G-23 | RS 两实例模型对"槽位↔进程"关系的破坏 | boot 自升级后 RS 槽的 pid/endpoint 与进程号不再恒定，影响初学者理解 `RS_PROC_NR` 特判 | `main.c:450-454`；`update.c:293-305` | 17 §（K-17-08） |
| G-24 | 测试基建现状与边界 | 旧文档把实现期测试计数写进正文（易腐）；需要一条明确归属 | `os/qemu-tests/`；`minix3/minix/tests/`（无 rs 目录）；`edge_todo.md` E9 | 各篇验收标准 + §3.5 第 10 项 |
| G-25 | Rust 侧 `shell_request.rs`/`shell_update.rs`/`trap_api.rs` 的归属 | 这三块是"19 接线层"落地形态，旧文档 19 没有覆盖 shell 拆分 | `os/servers/rs/src/shell_request.rs`（1917 行）；`shell_update.rs`；`trap_api.rs` | 18 §（K-18-13）；19 号契约的"Rust 实现形态"小节 |

### 3.3 重复主题表

| 主题 | 旧目录全部出现位置 | 新目录主讲述点 | 其余位置处置 |
|------|------------------|---------------|-------------|
| boot Step 1 权限装配 | 01 §2.3.3、03 §2.4、05 §2.2 | 09 §3（流程）+ 03 §（字段公式） | 01/03/05 三处细节合并为一处；09 只讲时序，字段语义引 03 |
| `fill_send_mask`/`ALL_M` 快捷路径 | 01 §2.3.3、03 §2.5、05 §1.2/§2.2 | 03 §（原语）+ 04 §（语义） | 三处压缩为两处，boot 篇只写调用点 |
| `r_priv` 字段模型 | 02 §2.3、03 §2.1/§3.2 | 03 §2 | 02 只列字段名与归属指针 |
| `rs_isokendpt` | 02 §2.10、06 §2.3 | 02 §（原语）+ 10 §（调用点） | 06 的重复实现说明删除 |
| `reply`/`late_reply`/`EDONTREPLY` | 01 §2.1、06 §2.2、12 §3.4、13 §3.3 | 01 §3（约定） | 其余篇只写"登记 LATEREPLY 三字段"的调用点 |
| 心跳时间戳读写下 | 02 §2.3、06 §2.1、07 §1.2 | 10 §（写）+ 13 §（读/判） | 02 只声明字段；07 的三处复述合一 |
| `r_period` 的写与用 | 07 §2.2、08 §2.4 | 13 §（有效 period 计算）+ 08 §（edit_slot 写入） | 08 只写字段覆盖规则 |
| `clone_service`/`swap_slot` | 10 §2.6-2.9、15 §2.5、16 §2.3 | 11 §（机制） | 13/16 只写调用点 |
| `update_service` | 10 §3.2、16 §2.3、18 §2.5 | 11 §（机制） | 16/17 只写场景差异 |
| `end_srv_init` | 12 §2.6、16 §2.8 | 06 §（收尾） | 16 只写"成功后逐描述符调用" |
| `do_upd_ready` | 12 §2.4、16 §2.8 | 06 §（协议）+ 16 §（状态机侧） | 12 的三路派发细节合并进 06，16 只讲链推进 |
| `get_service_instances` | 02 §2.11、10 §2.4、15 §2.7 | 02 § | 其余引用 |
| `init_slot`/`inherit_service_defaults` | 08 §2.3/2.6、16 §2.1 | 08 § | 16 只写调用 |
| VM 特例（单副本/pin/MAKE_VM/时序） | 10 §2.4/2.6、16 §2.5、18 §2.5 | 11 §（实例侧）、16 §（更新侧）、17 §（rollback） | 三处按场景分工，末尾互引 |
| 信号处理 | 01 §1.3、06 §2.6、18 §3.3 | 01 §（注册）+ 13 §（行为） | 18 的重复叙述删除 |
| 外部签名 | 01 §3.5、03 §2.6/4.5、09 §3.4、19 全篇、99 §3.4 | 18 § | 其余篇只给调用点与语义，签名不复写（单点权威规则） |
| 常量表 | 99 全篇与各篇散点 | 99 § | 各篇只引用名字与含义 |
| RS 自升级 | 01 §2.3.7、18 §2.1 | 09 §（boot 内调用点）+ 17 §（机制） | 09 用"开 feature 时多做什么"一段，细节归 17 |
| `cleanup_service` 调用点 | 12 §2.6、13 §2.2、15 §2.2、16 §2.8 | 13 §（机制） | 其余只写调用时机 |
| `getnuid` | 04 §2.1、19 §2.1 | 05 §（机制）+ 18 §（签名） | 单点权威 |

### 3.4 越界主题表（按旧文档声明边界判定）

| 旧位置 | 越界内容 | 正确归属 | 新目录处置 |
|--------|---------|---------|-----------|
| 01 §2.3.3（Step 1 段） | 权限字段逐字段讲解 | 03（机制）+ 05（掩码） | 09 只保留流程与调用点 |
| 01 §2.3.7 | boot 自升级机制（`update_service`/YIELD） | 17（RS 自身）+ 11（实例切换） | 09 保留调用点与 feature 门 |
| 02 §2.7 | `rprocupd` 状态语义（更新阶段） | 16 | 02 只留结构形状 |
| 06 §2.6 | 终止信号触发的 `terminate_service` 细节 | 13 | 10 只留信号分类与路由 |
| 07 §2.6 | `update_period` 的 LU 超时机制 | 16 | 13 只留"prepare 超时会调用谁" |
| 10 §3.2 | `KernelApi` 五域面接口清单 | 18 | 11 只留使用点 |
| 12 §3.0b/§2.4 | 更新链推进（`start_update_prepare_next`/`start_update`） | 16 | 06 只留协议门与旗标 |
| 14 §2.3 | `RS_SYSCTL_UPD_*` 触发的更新机器细节 | 16 | 15 只留子操作与控制流 |
| 14 §2.5 | 状态打印族的未来归属（08-stage-is） | 15-stage-is（跨 stage） | 15 保留正文，加"未来下放"注记（R31 residual） |
| 16 §2.1 | `init_state_data` 数据格式细节 | 16 自身合篇（本蓝图合并后不越界） | 合并后自然归位 |
| 18 §2.1 | boot 自升级的 boot 位置 | 09 | 17 只留角色分派与机制 |
| 19 §3 | 消息槽类型化的常量权威 | 99 | 18 只留载荷视图，数值归 99 |
| 99 §3.4 | `KernelApi` 注入边界（实现形态） | 18 | 99 只留常量映射规则 |
| 05 §3.1 | `r_ipc_list` 的拷入与校验 | 08 | 04 只留列表语义与计算 |
| 04 §2.5 | `RUPDATE_IS_UPDATING` 全局状态 | 16 | 05 只留 EBUSY 规则所需的一行 |

### 3.5 非 C 主题逐项回答

| # | 主题 | 在本 stage 哪里讲（或为什么不在） |
|---|------|--------------------------------|
| 1 | 链接与加载 | RS 自身映像由 boot image 装载（前序 `01-stage-kernel`/`02-stage-vm` 承担，本 stage 01 只写入口契约 K-01-12）；RS 装载别的服务的映像在 08（`srv_execve`/`read_seg`/libexec 契约）；ELF 解析细节以 `minix-elf` crate 与 A-8 为界，不在本 stage 重讲。 |
| 2 | 镜像与内存布局 | boot image 表与 RS 的位置在 00/09（`kernel/table.c`）；RS 静态表与 x86-64 尺寸（56/64 字节消息、56 字节 state data）在 02/16/18；新进程栈帧与 `ps_strings` 在 08；mmap/heap 预分配在 16。 |
| 3 | 汇编入口与陷阱进入 | 明确不在本 stage：RS 从 SEF 的 `_start` 进入（01 只写契约）；用户态 trap 桥属 `01-stage-kernel` 的用户态入口专题；RS 不直接执行 trap 指令。 |
| 4 | 启动装配 | 09 全篇（四步 boot + 自升级调用点）；boot image 的组装工具链见第 5 项。 |
| 5 | 构建与工具链 | C 侧 `servers/rs/Makefile`、`minix.service.mk` 属构建系统，按 plan §5.4 判 **WONTFIX**（不承载语义），99 §7.1 显式列出；Rust 侧 `Cargo.toml` 的 `live-update` feature 归 99（A-11）；boot image 装载例外在 09。 |
| 6 | 跨模块接口与线格式 | 18 全篇（`sys_*`/`srv_*`/`vm_*`/`ds_*`/`sched_*`/libexec/mapdriver/devman + 消息槽）；07 讲 `system.conf` 文本与 `rs_start` 字节布局；99 讲常量数值。三处分工在 18 §1 说明。 |
| 7 | 错误路径 | 每篇的"失败/边界"小节 + 99 的错误表（K-99-11）；fail-closed/panic 策略在 01（执行模型）与 09（boot 表校验）；错误路径不单独成篇（否则会与各机制篇重复）。 |
| 8 | 关闭与退出 | 14 §（`do_shutdown` 控制面）+ 13 §（core death、cleanup、detach、RS 收到 SIGTERM 的收尾）；RS 自身退出在 17（K-17-09）。 |
| 9 | 并发与同步 | RS 是单线程事件循环（01 声明执行模型，10 主循环不阻塞）；LU 用全局 `rupdate` 状态做互斥（16）；RS 不持有跨 CPU 共享结构，SMP 同步不在本 stage。 |
| 10 | 测试基建 | Rust 单测随各篇验收标准（不写易腐计数）；宿主端到端与真机联调属 E9（`../edge_todo.md`，19-stage-integration）；C 侧 `minix3/minix/tests/` 没有 RS 专属测试程序，这一空白在本节显式声明，不在正文假装有。 |

---

## 4. 新目录

### 4.1 新篇章总表

旧目录 21 篇 → 新目录 20 篇。文件名沿用 `NN-rs-<语义名>.md`；`00` 为总览、`99` 为词典的约定不变。**编号含义有变，详见 §8.2 引用迁移表**。

| 编号 | 标题（文件名） | 一句话定位 | 分组 | 前置 | 承载知识点 |
|------|---------------|-----------|------|------|-----------|
| 00 | `00-rs-overview.md` 总览与阅读地图 | RS 是谁、在 boot 链哪里、两个状态机、新目录怎么读 | 0 导航 | 无 | K-00-01…09 |
| 01 | `01-rs-sef-runtime.md` SEF 运行时与进程骨架 | RS 的 main 从哪里被调用、注册哪些回调、用什么回复约定 | I 运行框架 | 00 | K-01-01…12 |
| 02 | `02-rs-process-table.md` 服务登记表 | 一个服务槽位里有什么、怎么找、怎么分配 | II 数据与权限 | 00 | K-02-01…17 |
| 03 | `03-rs-privilege.md` 权限结构 | 服务的权限结构由什么组成、怎么装配与提交 | II | 02 | K-03-01…15 |
| 04 | `04-rs-ipc-sendmask.md` IPC 可达性 | 服务能向谁发消息、掩码怎么算 | II | 02、03 | K-04-01…09 |
| 05 | `05-rs-access-control.md` 调用者访问控制 | 谁有资格命令 RS、五条规则 | II | 02、03 | K-05-01…07 |
| 06 | `06-rs-init-handshake.md` 初始化握手 | RS 发 RS_INIT、服务回 ready，boot 与运行时两种收法 | III 协议与配置 | 01、02 | K-06-01…13 |
| 07 | `07-rs-service-declaration.md` 服务声明 | `system.conf` 与 `minix-service` 如何生成 `rs_start` 请求 | III | 02、03、04 | K-07-01…11 |
| 08 | `08-rs-slot-image.md` 槽位配置与映像准备 | 声明怎么落成槽位字段、映像怎么备好 | III | 02、03、04、07 | K-08-01…20 |
| 09 | `09-rs-boot-init.md` 启动主线 | `sef_cb_init_fresh` 四步：建槽、放行、收敛 ready、进入监控 | IV 启动 | 01–08 | K-09-01…13 |
| 10 | `10-rs-main-loop.md` 主循环与消息分派 | 四类消息怎么分类、怎么回复、空闲时做什么 | V 运行时 | 01、02、06、09 | K-10-01…09 |
| 11 | `11-rs-service-create.md` 服务创建与实例切换 | fork → 权限 → 调度 → 映像 → VM 交互 → 激活；副本与槽交换 | V | 03、04、06、08 | K-11-01…18 |
| 12 | `12-rs-publish.md` 服务发布与撤销 | DS label、驱动映射、PCI、devman 四目录的注册与撤销 | V | 02、11 | K-12-01…08 |
| 13 | `13-rs-monitor-recovery.md` 存活监控与故障恢复 | 心跳如何发现故障，终止/清理/退避/重启如何处置 | V | 01、02、06、10、11、12 | K-13-01…20 |
| 14 | `14-rs-control-requests.md` 控制请求 | 外部如何 up/down/restart/refresh/clone/edit/shutdown 一个服务 | VI 请求面 | 05、06、08、11、13 | K-14-01…14 |
| 15 | `15-rs-query-requests.md` 查询与故障注入 | lookup/getsysinfo/sysctl/fi 与状态打印 | VI | 05、08、13 | K-15-01…08 |
| 16 | `16-rs-live-update.md` Live Update：状态机与状态迁移 | 不中断替换：链、prepare/update/init/end/rollback、状态数据迁移 | VII Live Update | 02、06、08、11、13 | K-16-01…25 |
| 17 | `17-rs-self-lifecycle.md` RS 自身生命周期 | RS 自己的重启、热更新、回滚与信号管理器备份 | VII | 06、09、11、13、16 | K-17-01…10 |
| 18 | `18-rs-external-interfaces.md` 外部接口契约 | 全部外部调用点与消息槽的唯一签名权威面 | VIII 参考 | 01–17 | K-18-01…13 |
| 99 | `99-rs-global-concepts.md` 常量、错误与诊断词典 | 常量全表、错误表与 Rust 落点权威 | VIII | 无（词典） | K-99-01…15 |

新目录的**总依赖图**（只画跨部分依赖，篇内小节依赖在 §5 各契约中给全）：

```
00
└─► 01 ─────────────────────────────┐
    ├─► 02 ─► 03 ─► 04 ─┐           │
    │         └─► 05    │           │
    │         └─► 07 ─► 08 ─┐       │
    ├─► 06 ─────────────────┴─► 09 ─┴─► 10
                                        ├─► 11 ─► 12
                                        │    └───► 13
                                        ├─────────► 13
                                        ├─► 14（另需 05、08、13）
                                        └─► 15（另需 05、08、13）
11 ─► 16 ─► 17（16 另需 06、08、13）
18：全部篇的外部签名汇总（无前置，按需查）
99：常量词典（无前置，按需查）
```

**无环检查**：以上依赖图按编号从小到大只出现"指向更大编号"的边（00→…→17），18/99 无出边 → 无环（G4 通过）。

### 4.2 阅读路径

- **完整首读主线**（建议顺序）：`00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15 → 16 → 17`，最后按需查 `18`/`99`。
- **只跟 boot 链**（从 02-stage-vm 过来的读者）：`00 → 01 → 02 → 03 → 04 → 06 → 08 → 09 → 10`；`07` 可跳（声明面），`05` 可跳（不在 boot 路径）。
- **只理解运行时服务生命周期**：`10 → 11 → 12 → 06 → 13 → 14`；`02/03/04` 作查询手册。
- **只理解崩溃恢复**：`10 → 13`（含终止/退避/重启），`14` 的 down/restart/refresh 为控制入口。
- **只理解 Live Update**：`06 → 11 → 13 → 16 → 17`；`07 §PCI` 与 `12` 的发布顺序在 16 §更新时序里用到时回查。
- **权限与隔离移植**：`02 → 03 → 04 → 05`。
- **支线与可跳读**：`07`（工具/客户端面，纯运维读者可最后看）；`15 §状态打印`（诊断输出）；`16 §VM multi`（多组件时序，首读可跳过但必须知道存在）；`17`（RS 自引用特例，首读可跳，读 LU 后再回来）；`99`（词典，随用随查）。

### 4.3 并行主题的分组与代表成员

本 stage 有三类并行体，按"统一框架 + 分组 + 代表成员 + 差异表"组织（不伪造线性序）：

| 并行体 | 规模 | 统一框架 | 分组方式 | 代表成员（讲透） | 差异表 |
|--------|------|---------|---------|----------------|--------|
| RS 请求 | 15 个消息 + 4 条信号分支 | 10（分类/回复骨架） | ① 内核 notify：13；② 服务自报：06/16；③ 外部控制：14；④ 观测/注入：15 | `RS_UP`（14+11）、`RS_INIT`（06）、`RS_UPDATE`（16）、终止信号（13） | 14 §请求差异表、15 §查询差异表、13 §信号分类表 |
| boot 服务 | `boot_image_priv_table` 13 条（其中系统服务 12） | 09 四步（表驱动，不逐服务） | 按表中顺序，无分组 | RS/VM 例外（D2）、`SF_SYNCH_BOOT` 同步项（D4） | 09 §三表差异表（priv/sys/dev 默认项语义） |
| 外部依赖面 | 6 类调用面（sys/srv/vm/ds/sched/其他） | 18 §1 总图 | 按被调用者分组 | `sys_privctl`（03）、`srv_fork`/`srv_execve`（11）、`vm_memctl`/`vm_update`（16）、`ds_publish_label`（12） | 18 §各表内的"调用点"列即是差异表 |

### 4.4 序差表（运行时序 vs 教学序）

| # | 运行时序事实（锚点） | 教学序选择 | 理由 | 回指补偿 |
|---|---------------------|-----------|------|---------|
| S-1 | `minix-service`/`system.conf` 在系统启动之后才被使用 | 07 排在 boot（09）之前 | `rs_start` 线格式由 07 定义，08（解码）与 14（`do_up`）都依赖它；先定义接口再讲消费 | 07 §1 开头声明"这是运行期声明格式，boot 服务不使用它，boot 服务的配置源见 09" |
| S-2 | 08 的 `srv_execve`/`edit_slot` 全部在运行期使用；仅 `build_cmd_dep` 在 boot Step 1 被调用（`main.c:311`） | 08 排在 boot（09）之前 | 同一份命令串解析同时服务 boot 与运行期，提前可消除重复；boot 篇只需引用 | 08 §boot 小节标注"boot Step 1 调用点（`main.c:311`）"，09 契约的事实底线引用 08 |
| S-3 | 运行期握手发生在创建（11）之后 | 06 排在 09/11 之前 | 握手协议有三个使用场景（boot 09、创建 11、LU 16），先协议后场景是单点权威的必然结果 | 06 §1 给出"三个场景一张表"；11 §run_service 处回指 06 |
| S-4 | `update_service` 既被重启（13）又被 LU（16）使用 | 机制提前到 11 | 避免 10/16/18 三处重复描述；`update_service` 本质是"实例切换" | 13 §restart 与 16 §start_srv_update 处回指 11 |
| S-5 | 控制请求（14）与查询（15）可在任意时刻到达，与生命周期无固定先后 | 14/15 排在生命周期机制之后 | 请求 handler 的语义依赖全部机制；先机制后接口可让 14/15 只写"编排 + 权限 + 回复" | 14 §1 骨架先给"每个 handler = 权限 + 机制调用 + 回复"公式 |
| S-6 | boot 自升级块在 `sef_cb_init_fresh` 末尾执行（`main.c:436-491`） | 机制在 17，调用点在 09 | 自升级依赖 LU（16）与实例切换（11）全部概念 | 09 §4 只给"feature 开启时多做什么"与执行序，17 §2 展开 |
| S-7 | `do_upd_ready` 同属握手（06）与 LU 状态机（16） | 协议语义归 06，链推进归 16 | 避免一处讲两遍；`RS_LU_PREPARE` 在消息分类表中与 `RS_INIT` 并列 | 06 §3 给协议门与旗标，16 §5 给 `start_update_prepare_next`/`start_update` 的推进 |
| S-8 | 13 合并了"监控（先）"与"终止恢复（后）"两篇旧文档 | 合并为一篇，顺序保持检测→处置 | 旧 07 的崩溃分支调用旧 15 的机制、旧 15 又引用旧 16，拆开必然互相前向引用；合篇后一个故障的旅程一次讲完 | 13 §1 给"检测 → 处置"总图，§2 起按序展开 |

---

## 5. 每篇契约

> 说明：每篇契约的七要素齐全——定位、讲什么、不讲什么、前置、后置、事实底线、知识点清单 + 验收标准。知识点清单采用"编号 + 名称 + 为什么归本篇"三列；类型、来源、锚点、读者收益见 §2 对应编号的完整行（避免两处维护易腐信息）。

### 00-rs-overview：总览与阅读地图

- **一句话定位**：让读者在一篇之内建立 RS 的全局心智模型——它是什么、在 boot 链的哪一环、一生在做什么——并知道遇到任何问题该翻哪一篇。
- **讲什么**：RS 的身份与 root system process 语义（K-00-01）；登记序与执行序两层顺序（K-00-02，含 `RTS_VMINHIBIT` 的因果链）；两个状态机与生命周期次主线（K-00-03/K-00-04）；新目录导航表、依赖图与阅读路线（K-00-05/K-00-09）；文档组织原则（K-00-06）；ARCH A-1…A-14 索引（K-00-07）；覆盖契约（K-00-08）。
- **不讲什么**：任何机制的内部（全部下放：boot 细节 09、登记表 02、权限 03/04/05、握手 06、声明 07、映像 08、循环 10、创建 11、发布 12、监控恢复 13、控制 14、查询 15、LU 16、自身 17、外部 18、常量 99）；不重复 `01-stage-kernel` 的 boot 协议细节，只引用其结论（VM 解除抑制）；不展开 ARCH 各项的设计论证据（各篇自述）。
- **前置**：无（可独立阅读；强烈建议先读 `../02-stage-vm/00-vm-overview.md` 了解上游）。
- **后置**：全部 19 篇都以本篇的导航表与依赖图为入口。
- **事实底线（ground truth）**：`include/minix/com.h:61,77`（RS_PROC_NR/ROOT_SYS_PROC_NR）；`kernel/table.c:36-64`（登记序与注释语义）；`kernel/main.c:196,265`（执行序与 VMINHIBIT）；`servers/rs/main.c:57-130`（主循环）；`manager.c:950-983`（`start_service` 生命周期汇聚点）；`../02-stage-vm/25-rs-services.md`（VM 侧 RS 服务，双向核对）；新目录 TOC 本身。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-00-01 | RS 身份与 root system process | 全 stage 的第一个问题 |
| K-00-02 | 两层顺序语义 | 解释 RS 在 boot 链的实际位置，是 09 的导航前提 |
| K-00-03 | 两个状态机 | 给出全篇的骨架图 |
| K-00-04 | 复活能力与可靠性模型 | 解释 RS 名字与存在理由 |
| K-00-05 | 新目录导航表与依赖图 | 导航枢纽 |
| K-00-06 | 组织原则 | 让读者理解文档为何这样排 |
| K-00-07 | ARCH 索引 | 架构演进的唯一汇总点 |
| K-00-08 | 覆盖契约 | 明确讲什么、覆盖到什么粒度 |
| K-00-09 | 阅读路线 | 按目的分流 |

- **验收标准**：① 读者读完后能不看目录回答"RS 是第几个执行的服务、为什么""两个状态机分别是什么""权限/掩码/握手/创建分别在第几篇"；② 文中必须出现 boot 链位置图与生命周期图各一张；③ ARCH A-1…A-14 一张全表，每项给出承载篇号；④ 阅读路线不少于 5 条（boot 链、生命周期、恢复、LU、权限、运维）；⑤ 所有指向其它篇的引用只使用新编号。

### 01-rs-sef-runtime：SEF 运行时与进程骨架

- **一句话定位**：回答"RS 的 `main` 是谁调用的、它在进入永不退出的事件循环之前注册了什么、之后用什么约定对外说话"。
- **讲什么**：SEF 运行库的请求拦截循环与 init 分派（K-01-01/K-01-12）；RS 注册的 7 个回调与三种 init 类型（K-01-02/K-01-03）；`main()` 三活动骨架（K-01-04）；全 stage 共用的回复约定 `reply`/`late_reply`/`EDONTREPLY` 与 `rs_asynsend`（K-01-05/K-01-06）；单线程执行模型（K-01-07）；SEF ping 拦截（K-01-08）；RS 自模拟 ready 的两个 response 回调（K-01-09）；信号回调的注册与分工（K-01-10）；`KernelApi` 五域面与纯决策/shell 分层（K-01-11）。
- **不讲什么**：四步 boot 的字段装配与顺序细节（09）；主循环四类分类与分派表（10）；SIGCHLD/SIGTERM/终止信号的处置（13）；SEF 的 LU 状态机与 `sef_liveupdate.c`（16/17）；外部调用签名（18）；libc `_start` 与陷阱入口的机器细节（不在本 stage，见 §3.5 第 3 项）。
- **前置**：00。
- **后置**：全部；其中 06/09/10/13/14/15/16/17 直接引用。
- **事实底线（ground truth）**：`servers/rs/main.c:38-153`（main/sef_local_startup）、`main.c:499-626`（三 init 与两 response 回调）、`main.c:631-704`（信号回调）；`utility.c:223-242,309-347`（asynsend/reply/late_reply）；`include/minix/sef.h:41-101,213-242`（回调类型与 init/LU 旗标）；`lib/libsys/sef.c:185-230`（拦截循环）；`sef_init.c`（init 请求分派）；`sef_ping.c:21-38`（ping 应答）；`sys/sys/errno.h:199`（EDONTREPLY）；Rust：`boot.rs` 五 trait、`sef.rs`、`trap_api.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-01-01 | SEF 与 RS 的双重身份 | 全篇前提 |
| K-01-02 | 7 个回调注册 | 骨架的第一块 |
| K-01-03 | 三种 init 类型 | 解释同一个服务三种被启动方式 |
| K-01-04 | main() 骨架 | 骨架的第二块 |
| K-01-05 | 回复约定 | 全 stage 公共原语，必须最早定义 |
| K-01-06 | rs_asynsend | 异步发送是握手与更新的基础 |
| K-01-07 | 执行模型 | 约束 Rust 实现形态 |
| K-01-08 | SEF ping 拦截 | 心跳闭环的服务侧 |
| K-01-09 | 自模拟 ready | RS 自己也是服务 |
| K-01-10 | 信号回调注册 | 信号面入口 |
| K-01-11 | KernelApi 分层 | 全 stage Rust 代码的骨架 |
| K-01-12 | 入口装载链 | 补上 `_start` 到 `main` 的空白 |

- **验收标准**：① 能画出 `_start → sef_startup（拦截循环）→ init 回调 → main 循环` 的时序图；② 7 个回调逐个说明"何时被 SEF 调用"；③ `reply`（立即）与 `late_reply`（登记后由别处触发）用一张对比表讲清，并说明 `EDONTREPLY` 的哨兵值；④ 明确指出 RS 与普通 SEF 服务的差别（注册全套回调、自模拟 ready、自己就是 SEF provider）；⑤ 引用外部签名一律指向 18，不在本篇复制。

### 02-rs-process-table：服务登记表

- **一句话定位**：把"RS 管理的一个服务"变成一个可检视的数据结构——槽位里有什么、状态怎么表示、怎么找到它、怎么分配与释放。
- **讲什么**：双表结构 `rproc`/`rprocpub` 与 `r_pub`（K-02-01）；`rproc` 全字段与字段归属（K-02-02）；`rprocpub` 公开面与 `NO_DEV`（K-02-03）；`r_flags` 16 位状态与 `sys_flags` 13 位策略（K-02-04/K-02-05）；更新描述符 `rprocupd`/`rupdate` 的形状（K-02-06）；`rinit`（K-02-07）；`rproc_ptr` 反查索引（K-02-08）；四实例链（K-02-09）；容量常量（K-02-10）；三个反直觉字段规则：`r_pid=-1`、I/O/IRQ 备份、`NO_DEV`（K-02-11/K-02-12/K-02-03）；五类查找与槽位生命周期（K-02-13/K-02-14）；`rs_isokendpt`（K-02-15）；实例收集（K-02-16）；Rust 类型化模型（K-02-17）。
- **不讲什么**：`r_priv` 字段语义（03）；`r_ipc_list` 的计算（04）；`r_control` 的消费（05）；`r_upd` 的状态机与 state data（16）；`r_period`/时间戳的读写时机（13）；命令/脚本字段的填充（08）；`r_exec` 的装载（08）；槽位在创建/清理中的使用（11/13）；`rinit.rproctab_gid` 的消费侧（18）。
- **前置**：00。**后置**：03/04/05/06/07/08/09/10/11/12/13/14/15/16/17/18 全部引用。
- **事实底线（ground truth）**：`servers/rs/glo.h:33-35`；`type.h:30-108`；`include/minix/rs.h:165-183,191-206`；`servers/rs/const.h:28-45,61-68`；`manager.c:1334-1352,1935-2109`；`utility.c:352-359`；`main.c:229-237,342-345`；`minix/sys_config.h:9`；`minix/config.h:31-32`；`com.h:56`；Rust：`service_slot.rs`、`process_table.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-02-01 | 双表与 `r_pub` | 数据模型的门面 |
| K-02-02 | `rproc` 全字段与归属 | 槽位字典 |
| K-02-03 | `rprocpub` 与 `NO_DEV` | 公开面与边界值 |
| K-02-04 | `r_flags` 16 位 + IS_IDLE | 状态机的基础词汇 |
| K-02-05 | `sys_flags` 13 位 + IMM_SF | 策略位与继承规则 |
| K-02-06 | 更新描述符形状 | 16 的数据底座 |
| K-02-07 | `rinit` | 服务读公开表的授权载体 |
| K-02-08 | `rproc_ptr` | 主循环与信号路径的 O(1) 索引 |
| K-02-09 | 实例链四指针 | 11/16 的数据底座 |
| K-02-10 | 容量常量 | 资源边界 |
| K-02-11 | `r_pid=-1` | 边界值语义 |
| K-02-12 | I/O/IRQ 备份字段 | 反直觉字段，防误读 |
| K-02-13 | 五类查找差异 | 使用手册 |
| K-02-14 | 槽位生命周期 | 分配与释放规则 |
| K-02-15 | `rs_isokendpt` | 来源校验原语 |
| K-02-16 | 实例收集 | 多实例语义 |
| K-02-17 | Rust 类型化模型 | 实现映射 |

- **验收标准**：① 字段归属表覆盖 `type.h:56-108` 与 `rs.h:165-183` 的每一个字段，并标注"谁填充/谁读取"；② `r_flags`/`sys_flags` 两张位表与 C 宏逐位对齐（可用 grep 对账）；③ 能解释 `r_pub` 自指针、`rproc_ptr`、四实例链三者的分工；④ 能区分 `lookup_slot_by_label`（只 ACTIVE）与 `lookup_slot_by_pid`（IN_USE）等过滤差异；⑤ Rust 表给出 C 字段 → Rust 字段的对应，并说明 `Arc<[u8]>` 与索引链如何替代裸指针。

### 03-rs-privilege：权限结构

- **一句话定位**：讲清 RS 交付给内核的"权限信封"——`struct priv` 的每个字段是什么、boot 和动态两条路径怎么装配它、RS 用哪些操作提交或修改它。
- **讲什么**：内核 priv 表与 RS 的本地副本（K-03-01）；`struct priv` 字段分组（K-03-02）；白名单模型（K-03-03）；boot Step 1 装配公式（K-03-04）；`SRV_OR_USR` 与默认宏（K-03-05）；RS/VM 例外与回读（K-03-06）；7 个 privctl 操作与内核覆盖规则（K-03-07/K-03-08）；两个位图原语（K-03-09）；`sched_init_proc`/`update_sig_mgrs`（K-03-10/K-03-11）；`s_flags` 位表（K-03-12）；静态 priv id（K-03-13）；驱动面操作与 A-10 边界（K-03-14）；Rust 实现（K-03-15）。
- **不讲什么**：`s_ipc_to` 的具体计算（04）；`r_control` 与访问控制（05）；`edit_slot` 如何为字段收集输入（08）；创建/编辑时的调用编排（11/14）；内核 `do_privctl` 的完整实现（只讲 RS 相关覆盖规则）；`SYS_PRIV_YIELD` 的 LU 场景（16/17）。
- **前置**：02。**后置**：04/05/06/08/09/11/13/14/16/17/18。
- **事实底线（ground truth）**：`kernel/priv.h:21-66,94`；`include/minix/priv.h:10-21,45-100`；`include/minix/const.h:143-154`；`main.c:258-296`；`utility.c:82-137,364-419`；`com.h:342-353`；`kernel/system/do_privctl.c:88,187-251,280-368`；`lib/libsys/sched_start.c:45-98`；Rust：`privilege.rs`、`sched.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-03-01 | 内核表与本地副本 | 权威关系 |
| K-03-02 | `struct priv` 字段组 | 全篇字典 |
| K-03-03 | 白名单模型 | 权限表达方式 |
| K-03-04 | boot 装配公式 | 首个完整使用场景 |
| K-03-05 | 默认宏体系 | 每个字段的默认来源 |
| K-03-06 | RS/VM 例外 | 反直觉规则 |
| K-03-07 | 7 个 privctl 操作 | 操作面 |
| K-03-08 | 内核覆盖规则 | 编辑语义 |
| K-03-09 | 两个位图原语 | 公共工具 |
| K-03-10 | 调度初始化 | 伴随原语 |
| K-03-11 | 信号管理器更新 | 伴随原语 |
| K-03-12 | `s_flags` 位表 | 旗标字典 |
| K-03-13 | 静态 priv id | 与掩码位号的接口 |
| K-03-14 | 驱动面操作边界 | A-10 演进说明 |
| K-03-15 | Rust 权限模型 | 实现映射 |

- **验收标准**：① `struct priv` 全字段表，每个字段给出"谁写、内核怎么用"；② boot Step 1 的每个赋值语句都有一行解释（`main.c:262-280` 逐条）；③ 7 个操作一表：操作码、内核动作、RS 调用点、失败语义；④ 能解释 `static_priv_id` 与 send mask 位号的关系；⑤ 明确"RS 不直接改内核表，只通过 privctl"这条边界。

### 04-rs-ipc-sendmask：IPC 可达性

- **一句话定位**：回答"一个服务能向谁发消息"——`s_ipc_to` 位图的两条计算路径、两种扫描方向、一个分词器。
- **讲什么**：两条计算路径（boot 全置 vs 动态精确，K-04-01）；forward/backward 两次扫描的原因与方法（K-04-02）；`get_next_name` 分词规则（K-04-03）；`SYSTEM`/`USER` 伪名解析（K-04-04）；`IPC_ALL`/`IPC_ALL_SYS` 的 backward 补齐（K-04-05）；`init_privs` 总入口（K-04-06）；`r_ipc_list` 的拷贝与上限（K-04-07）；调用链（K-04-08）；Rust 实现（K-04-09）。
- **不讲什么**：`s_ipc_to` 字段在 `struct priv` 中的位置与提交（03）；`r_ipc_list` 的文本拷贝/校验落点（08，本篇只引用 `edit_slot` 的调用点）；`do_edit` 的重算编排（14）；内核 send mask 强制与对称性（`01-stage-kernel/23-ipc-filter.md`）；与访问控制的关系（05，只做对比）。
- **前置**：02、03。**后置**：08/09/11/14/18。
- **事实底线（ground truth）**：`manager.c:2115-2331`（`get_next_name`/`add_forward_ipc`/`add_backward_ipc`/`init_privs`）；`main.c:272-273`（ALL_M 快捷）；`include/minix/rs.h:29-30`（`RSS_IPC_ALL*`）；`include/minix/priv.h:18,67-69`（`USER_PRIV_ID`、`SRV_M`）；`type.h:105-107`；`manager.c:1476-1483,1700`；Rust：`ipc_mask.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-04-01 | 两条计算路径 | 全篇框架 |
| K-04-02 | forward/backward | 核心算法 |
| K-04-03 | 分词器 | 输入解析 |
| K-04-04 | 伪名解析 | 特殊值 |
| K-04-05 | ALL/ALL_SYS 补齐 | 通配语义 |
| K-04-06 | `init_privs` | 总入口 |
| K-04-07 | 列表字段约束 | 输入边界 |
| K-04-08 | 调用链 | 时序 |
| K-04-09 | Rust 实现 | 实现映射 |

- **验收标准**：① 一张表对比 boot 全置与动态精确计算（输入、算法、结果、调用点）；② 用示例（如 `ipc SYSTEM vfs rs`）手工走一遍 forward + backward，说明为什么两次都需要；③ 分词器规则表：空白、NUL、超长条目的处置；④ 明确"位图存的是 priv id 不是 endpoint"；⑤ 与 03 的边界一句话写清（03 定义字段与提交，本篇只算值）。

### 05-rs-access-control：调用者访问控制

- **一句话定位**：回答"谁有资格命令 RS"——root 与隔离策略两条通道、五条目标槽规则、11 个入口检查点。
- **讲什么**：两级授权模型（K-05-01）；`caller_is_root` 与 fail-closed（K-05-02）；`caller_can_control` 的控制列表匹配（K-05-03）；五条目标槽规则与判定顺序（K-05-04）；调用点分布（K-05-05）；与 LU 全局状态的交互（K-05-06）；Rust 纯决策实现（K-05-07）。
- **不讲什么**：`r_control` 的填充（08）；各请求 handler 的语义（14/15/16，本篇只列检查点）；`getnuid` 的 PM 侧实现细节（18 给签名）；更新状态机本身（16，本篇只用 `RUPDATE_IS_UPDATING` 一条规则）。
- **前置**：02、03。**后置**：08/10/11/13/14/15/16/17。
- **事实底线（ground truth）**：`manager.c:21-130`（三个函数全文）；`request.c` 11 个调用点（27、133、183、232、277、329、412、439、643、1104、1253）；`lib/libsys/getepinfo.c:34-44`；`servers/rs/const.h:105`；`manager.c:52-53,64`；Rust：`access.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-05-01 | 两级授权 | 模型总述 |
| K-05-02 | root 判定 | 通道一 |
| K-05-03 | 控制列表 | 通道二 |
| K-05-04 | 五条规则与顺序 | 核心判定 |
| K-05-05 | 调用点分布 | 使用地图 |
| K-05-06 | 更新中 EBUSY | 与 16 的接缝 |
| K-05-07 | Rust 纯决策 | 实现映射 |

- **验收标准**：① 五条规则一表：规则、判定条件、拒绝码、C 行；② 给出"无目标槽请求（如 `RS_UP`）为什么只查 root"的因果解释；③ 明确 `do_lookup`/`do_sysctl` 没有入口检查这一**事实**（不夸大"每个 handler 都有"）；④ 能画出三层判定流程（root/control → 目标槽规则 → 返回码）；⑤ fail-closed 行为（`getnuid` 失败按非 root 处理）有原文锚点。

### 06-rs-init-handshake：初始化握手

- **一句话定位**：把"服务何时算真正启动完成"定义清楚——`RS_INIT` 消息、ready 回执的四分支、boot 与运行时两种收集方式、RS 自模拟。
- **讲什么**：握手为什么是消息而非阻塞调用（K-06-01）；boot 同步与运行时异步两种收法（K-06-02）；`RS_INIT` 九字段（K-06-03）；初始化中的槽位状态（K-06-04）；RS 自身的例外（K-06-05）；新旧实例信息推导（K-06-06）；`do_init_ready` 四分支（K-06-07）；失败语义（K-06-08）；`do_upd_ready` 协议门（K-06-09）；boot 同步捕获与 VM 例外（K-06-10）；`end_srv_init` 收尾（K-06-11）；RS 自模拟 ready（K-06-12）；`run_service` 放行原语（K-06-13）。
- **不讲什么**：四步 boot 的完整流程（09，本篇只给 Step 2/3 的调用点）；LU 链推进的三个动作（16，本篇只写 gate 与旗标）；`crash_service`/`cleanup_service` 的机制（13）；`reply`/`late_reply` 原语（01）；`run_service` 放行动作的权限/调度前置（11/03）。
- **前置**：01、02。**后置**：09/10/11/13/14/15/16/17 直接引用。
- **事实底线（ground truth）**：`utility.c:18-64`（`init_service`）；`request.c:462-529`（`do_init_ready`）、`request.c:890-938`（`do_upd_ready`）；`main.c:591-626`（自模拟）、`main.c:784-821`（`catch_boot_init_ready`）；`manager.c:328-355`（`end_srv_init`）、`manager.c:923-945`（`run_service`）；`ipc.h:1855-1866`（`mess_rs_init`）；`include/minix/sef.h:93-101`；Rust：`ready.rs`、`shell_update.rs` 的 ready 部分。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-06-01 | 握手的理由 | 概念前提 |
| K-06-02 | 两种收集方式 | 全篇主线 |
| K-06-03 | `RS_INIT` 载荷 | 协议内容 |
| K-06-04 | 初始化槽位状态 | 状态规则 |
| K-06-05 | RS 自身例外 | 边界 |
| K-06-06 | 旧实例信息推导 | LU/重启的输入 |
| K-06-07 | ready 四分支 | 核心判定 |
| K-06-08 | 失败语义 | 错误路径 |
| K-06-09 | LU prepare 门 | 与 16 的接缝 |
| K-06-10 | boot 同步捕获 | boot 场景 |
| K-06-11 | 收尾簿记 | 成功路径 |
| K-06-12 | RS 自模拟 | 自引用闭合 |
| K-06-13 | `run_service` 放行原语 | 创建与握手的接缝 |

- **验收标准**：① 一张"三种场景"对比表（boot、运行期、RS 自己）——谁发、谁收、怎么回复、失败怎么办；② `RS_INIT` 九字段逐字段解释（含 `rproctab_gid` 与 `old_endpoint`）；③ `do_init_ready` 四分支判定图，明确哪条路径回 `EDONTREPLY`、哪条回 `EINVAL`；④ 失败路径链到 13 的 `crash_service`/`RS_REINCARNATE`（只给接口）；⑤ VM 例外给出死锁原因（同步回复语义）。

### 07-rs-service-declaration：服务声明（system.conf 与 minix-service）

- **一句话定位**：补上控制面的起点——一个服务的权限与调度声明写在 `/etc/system.conf`，由 `minix-service` 解析成 `rs_start` 字节请求发给 RS。
- **讲什么**：`system.conf` 的指令集与语义（K-07-01）；三个关键字与前缀语法（K-07-02）；boot 与动态两类声明块（K-07-03）；解析产物 `struct rs_start`（K-07-04）；字节布局与字段（K-07-05）；默认值与零化契约（K-07-06）；`service` 命令族（K-07-07）；`control`/`pci` 指令（K-07-08）；服务身份常量（K-07-09）；解析失败的 fail-fast（K-07-10）；Rust 侧缺口（K-07-11）。
- **不讲什么**：`rs_start` 在 RS 侧的校验与落地（08）；`RS_UP` 请求的处理流程（14）；send mask（04）、priv 字段（03）、调度（03）的机制细节，只给"这个指令最终进哪个字段"；PCI 内核侧行为（A-10，12/99）。
- **前置**：02、03、04。**后置**：08/09/14/18/99。
- **事实底线（ground truth，非 C 制品为主）**：`minix3/etc/system.conf:1-529`；`minix3/minix/commands/minix-service/parse.c`（指令处理 406-900、`parse_config` 1229、默认值 1160-1169）；`minix-service.c:756` 与命令表；`include/minix/rs.h:12,22,33-52,104-163`；`servers/rs/Makefile`（服务安装）；Rust 侧 `os/etc/README.md`（占位）与 `os/servers/rs/` 全源 grep（无解析器）。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-07-01 | 指令集与语义 | 文件格式主体 |
| K-07-02 | 关键字语法 | 值域 |
| K-07-03 | 两类声明块 | 文件结构 |
| K-07-04 | 解析产物 | 客户端行为 |
| K-07-05 | `rs_start` 字节布局 | 接口契约 |
| K-07-06 | 默认值与零化 | 未声明字段语义 |
| K-07-07 | `service` 命令族 | 运维入口 |
| K-07-08 | `control`/`pci` 指令 | 隔离与 ACL 来源 |
| K-07-09 | 服务身份常量 | 默认 uid |
| K-07-10 | fail-fast | 错误语义 |
| K-07-11 | Rust 缺口 | 实现边界声明 |

- **验收标准**：① 一张"指令 → `rs_start` 字段 → RS 侧消费点（新篇 §）"的三列表，至少覆盖 uid/ipc/system/vm/io/irq/sigmgr/scheduler/priority/quantum/control；② 用 `service input` 与 `service rs` 两个真实条目各走读一遍；③ 明确写出 boot 服务**不使用** `system.conf` 的字段（priority/quantum 来自 `SRV_Q/SRV_QT`，见 09），并给出双源对照表；④ `rs_start` 字节布局与 Rust 解码侧（08）的偏移表互查；⑤ 结尾明确"Rust 尚未实装解析器"为已知缺口而非遗漏。

### 08-rs-slot-image：槽位配置与映像准备

- **一句话定位**：把一份 `rs_start` 声明变成一个填好的槽位和一份可装载的映像——校验、拷入、字段落地、argv 解析、副本继承、ELF 读取与共享。
- **讲什么**：三段配置管线（K-08-01）；`check_request` 校验（K-08-02）；拷入与截断（K-08-03）；`init_slot` 默认与清零（K-08-04）；`edit_slot` 字段覆盖表（K-08-05）；IRQ/IO sentinel（K-08-06）；call mask basic 叠加（K-08-07）；绝对路径与 argv（K-08-08）；label 默认（K-08-09）；脚本约束（K-08-10）；`RSS_COPY`/`REUSE` 分支（K-08-11）；四个行为旗标（K-08-12）；period/restarts/asr 覆盖（K-08-13）；副本继承（K-08-14）；`read_exec`（K-08-15）；`share_exec`/`free_exec`（K-08-16）；`srv_execve`（K-08-17）；`do_exec`（K-08-18）；`exec_restart`/`read_seg`（K-08-19）；Rust 映像与校验（K-08-20）。
- **不讲什么**：创建流程如何调用本篇的阶段（11）；`RS_UP`/`RS_EDIT` 的 handler 编排（14）；boot Step 1 的调用点（09，本篇标注）；LU 的 `inherit_service_defaults` 使用场景（16）；libexec/PM 的内部实现（18）。
- **前置**：02、03、04、07。**后置**：09/11/13/14/16/18。
- **事实底线（ground truth）**：`request.c:1265-1308`；`manager.c:135-169,289-323,1303-1329,1372-1455,1460-1795`（含 `init_slot`/`edit_slot`/`clone` 的配置面）；`exec.c:21-165`；`include/minix/rs.h:33-52,104-151,165-183`；`lib/libexec/exec_elf.c:127-165`；`libc/sys/stack_utils.c:76,119`；Rust：`slot.rs`、`exec.rs`、`minix-elf`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-08-01 | 三段管线 | 全篇框架 |
| K-08-02 | 校验组 | 输入合法性 |
| K-08-03 | 拷入与截断 | 安全边界 |
| K-08-04 | `init_slot` | 默认状态 |
| K-08-05 | 字段覆盖表 | 核心字典 |
| K-08-06 | 资源哨兵 | 特殊值 |
| K-08-07 | basic 调用叠加 | 旗标语义 |
| K-08-08 | 命令与 argv | 解析规则 |
| K-08-09 | label 默认 | 命名规则 |
| K-08-10 | 脚本约束 | 边界 |
| K-08-11 | 内存副本分支 | 映像来源一 |
| K-08-12 | 行为旗标 | 策略集合 |
| K-08-13 | 运行时字段覆盖 | 覆盖规则 |
| K-08-14 | 继承不可变位 | 副本/新版本 |
| K-08-15 | `read_exec` | 映像来源二 |
| K-08-16 | 共享与释放 | 引用语义 |
| K-08-17 | 栈帧构建 | 装载接口 |
| K-08-18 | loader 装配 | 装载主体 |
| K-08-19 | 边界与回滚 | 错误路径 |
| K-08-20 | Rust 映像模型 | 实现映射 |

- **验收标准**：① `edit_slot` 字段覆盖表覆盖全部可覆盖字段，每行给"输入字段、值域、落点、校验失败码"；② 两张图：配置管线（声明→槽位）与映像路径（RSS_COPY/REUSE/命令 → RS 缓冲 → 子进程）；③ 能解释 `SF_USE_COPY` 的唯一置位点与释放扫描；④ argv 解析的边界（空格、NUL、上限、绝对路径）有测试级清单；⑤ 错误面（`ENOEXEC`/`EIO`/`ENOMEM`/`E2BIG`）逐项对应锚点。

### 09-rs-boot-init：启动主线

- **一句话定位**：RS 把自己启动好并让所有 boot 服务跑起来的一次性过程——`sef_cb_init_fresh` 的四步与末尾的自升级调用点。
- **讲什么**：引导自举问题（K-09-01）；四步分解与每步产出（K-09-02）；三张 boot 表（K-09-03）；四表查找与哨兵语义（K-09-04）；表条数校验（K-09-05）；表复位（K-09-06）；Step 1 全字段装配（K-09-07）；Step 2 放行与同步/异步 ready（K-09-08）；Step 3/4 收尾（K-09-09）；自升级调用点（K-09-10）；VM 异步例外（K-09-11）；Rust 状态机（K-09-12）；外部依赖（K-09-13）。
- **不讲什么**：`priv`/`s_ipc_to`/写掩码字段的语义（03/04）；`RS_INIT` 协议细节（06）；`sys_getimage`/`privctl` 的内核实现（18）；自升级机制（17）；周期检查机制（13，只写"启动首个 alarm"）。
- **前置**：01、02、03、04、06、07、08。**后置**：10/13/18。
- **事实底线（ground truth）**：`main.c:158-494`（`sef_cb_init_fresh` 全文）、`main.c:709-779`（四表查找）；`table.c:15-50`；`servers/rs/const.h:61-62`；`kernel/table.c:44-64`；`kernel/main.c:196,257,265`；`include/minix/rs.h:165-183`；`include/minix/priv.h:45-100`；Rust：`boot.rs`、`table.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-09-01 | 自举问题 | 动机 |
| K-09-02 | 四步分解 | 主线骨架 |
| K-09-03 | 三张 boot 表 | 数据输入 |
| K-09-04 | 四表查找 | 归并规则 |
| K-09-05 | 表校验 | 完整性 |
| K-09-06 | 表复位 | 起点状态 |
| K-09-07 | Step 1 装配 | 最大步骤 |
| K-09-08 | Step 2 放行 | 服务变可运行 |
| K-09-09 | Step 3/4 收尾 | 进入监控 |
| K-09-10 | 自升级调用点 | feature 门 |
| K-09-11 | VM 异步例外 | 边界规则 |
| K-09-12 | Rust 状态机 | 实现映射 |
| K-09-13 | 外部依赖 | 接口面 |

- **验收标准**：① 四步各一张"输入→动作→产出"小表；② 能逐条解释 Step 1 的每条赋值来自哪张表的哪个字段；③ 明确 boot 服务的调度参数来自 `SRV_Q/SRV_QT`，与 07 的 `system.conf` 值无关（双源对照表）；④ `NULL_BOOT_NR` 与 `DEFAULT_BOOT_NR` 两种哨兵的差别有原文锚点与后果；⑤ 读者能回答"VM 为什么计入未捕获 ready 而 RS 不计入"。

### 10-rs-main-loop：主循环与消息分派

- **一句话定位**：RS 运行期的心脏——每次循环取一条消息、分成四类、分派、回复，空闲时做两件背景事。
- **讲什么**：四类消息分类（K-10-01）；notify 与心跳写入（K-10-02）；15 臂分派表（K-10-03）；`EDONTREPLY` 与结果回填（K-10-04）；非阻塞回复（K-10-05）；空闲期两件事（K-10-06）；VM 单副本例外（K-10-07）；告警与 panic 边界（K-10-08）；Rust 分类器（K-10-09）。
- **不讲什么**：每个 handler 的语义（06/13/14/15/16）；周期检查三分支（13）；信号处置（13）；`rs_idle_period` 调用的 `cleanup_service`/`clone_service` 机制（11/13，只写调用意图）；`reply` 原语实现（01）。
- **前置**：01、02、06、09。**后置**：13/14/15/16/17/18。
- **事实底线（ground truth）**：`main.c:38-131`（main 全文）、`main.c:826-833`（`get_work`）；`utility.c:309-359,424-480`；`servers/rs/const.h:28-45`；Rust：`dispatch.rs`、`main.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-10-01 | 四类分类 | 核心结构 |
| K-10-02 | notify 与心跳写入 | 第一类分支 |
| K-10-03 | 分派表 | 导航枢纽 |
| K-10-04 | EDONTREPLY | 回复协议 |
| K-10-05 | 非阻塞回复 | 正确性约束 |
| K-10-06 | 空闲期两件事 | 背景任务 |
| K-10-07 | VM 单副本 | 边界 |
| K-10-08 | 告警/panic 边界 | 错误策略 |
| K-10-09 | Rust 分类器 | 实现映射 |

- **验收标准**：① 分类树覆盖 `main.c:70-121` 全部分支；② 15 臂分派表每行给"消息、handler、承载篇"；③ 能解释 notify 为何不回复、`EDONTREPLY` 为何不是错误；④ 空闲期两件事与关机覆盖（`shutting_down`）的原因一句话说清；⑤ 坏来源 panic 与未知消息 ENOSYS 的差别有锚点。

### 11-rs-service-create：服务创建与实例切换

- **一句话定位**：一个服务进程如何诞生，以及 RS 如何在新旧实例之间做原子切换（重启与 LU 共用）。
- **讲什么**：创建编排 `start_service` 四步 + `create_service` 十一步（K-11-01）；三个闸门（K-11-02）；fork 与登记（K-11-03）；权限/调度（K-11-04）；映像与重 pin（K-11-05）；`setuid(0)` hack（K-11-06）；RS/VM 内存特例（K-11-07）；`vm_set_priv`（K-11-08）；`clone_service`（K-11-09/K-11-10/K-11-12）；链方向（K-11-11）；激活（K-11-13）；`clone_slot`（K-11-14）；`swap_slot`（K-11-15）；`update_service`（K-11-16）；回滚边界（K-11-17）；Rust 实现（K-11-18）。
- **不讲什么**：字段从哪来（08）；发布的四个目录（12）；ready 握手的协议（06）；清理与终止（13）；LU 状态机（16）；`srv_fork`/`vm_memctl` 的签名（18）。
- **前置**：03、04、06、08。**后置**：12/13/14/16/17。
- **事实底线（ground truth）**：`manager.c:531-786`（创建+克隆）、`manager.c:1013-1028,1334-1352,1800-1930`（激活/实例/克隆槽/交换）、`update.c:230-325`（`srv_update`/`update_service`）；`utility.c:364-382`；`exec.c:21-59`；Rust：`service_create.rs`、`process_table.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-11-01 | 创建编排（含 `start_service`） | 主线 |
| K-11-02 | 三闸门 | 入口约束 |
| K-11-03 | fork 与登记 | 进程诞生 |
| K-11-04 | 权限/调度 | 内核侧准备 |
| K-11-05 | 映像与重 pin | 地址空间 |
| K-11-06 | `setuid` hack | 反直觉 hack |
| K-11-07 | RS/VM 内存特例 | 边界 |
| K-11-08 | `vm_set_priv` | 收尾 |
| K-11-09 | 克隆六步 | 副本 |
| K-11-10 | VM 单副本 | 边界 |
| K-11-11 | 链方向 | 数据语义 |
| K-11-12 | RS 备份信号管理器 | 高可用 |
| K-11-13 | 激活 | 当前实例定义 |
| K-11-14 | 克隆槽 | 基础原语 |
| K-11-15 | 槽交换 | 原子切换 |
| K-11-16 | `update_service` | 重启/LU 共用 |
| K-11-17 | 回滚边界 | 错误路径 |
| K-11-18 | Rust 值语义 | 实现映射 |

- **验收标准**：① 十一步表，每步给"失败时的回滚动作"；② 一张"新旧实例"图：槽、`r_pub`、`rproc_ptr`、pid/endpoint 在 `swap_slot` 前后的变化；③ `setuid(0)` 两次出现的同一原因写清（PM→VFS 阻塞通信）；④ `clone_slot` 深拷贝修正清单（清 ACTIVE、pid=-1、endpoint=-1、链清空、DYN_PRIV_ID、实例旗标清）；⑤ 读者能区分 `r_prev_rp`（副本）与 `r_old_rp`（旧版本）并说出各自出现场景。

### 12-rs-publish：服务发布与撤销

- **一句话定位**：服务创建好之后如何"被世界找到"——向 DS、VFS、PCI、devman 四个目录登记，以及反向的宽松撤销。
- **讲什么**：四目录模型（K-12-01）；失败即杀（K-12-02）；DS label（K-12-03）；mapdriver 条件（K-12-04）；PCI ACL（K-12-05）；devman 绑定（K-12-06）；撤销语义（K-12-07）；Rust 谓词（K-12-08）。
- **不讲什么**：创建流程（11）；`kill_service` 的机制（13）；`detach_service` 的重发布（13）；PCI 内核实现（A-10）；各外部调用的签名（18）。
- **前置**：02、11。**后置**：13/14/16/18。
- **事实底线（ground truth）**：`manager.c:787-921`（`publish_service`/`unpublish_service`）；`include/minix/rs.h:154-163`（`rs_pci`）；`com.h:858-865`（DEVMAN_*）；`include/minix/const.h:132`（`NO_DEV`）；Rust：`publish.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-12-01 | 四目录模型 | 全篇框架 |
| K-12-02 | 失败即杀 | 错误策略 |
| K-12-03 | DS label | 目录一 |
| K-12-04 | 驱动映射 | 目录二 |
| K-12-05 | PCI ACL | 目录三（A-10） |
| K-12-06 | devman 绑定 | 目录四 |
| K-12-07 | 撤销语义 | 反向路径 |
| K-12-08 | Rust 谓词 | 实现映射 |

- **验收标准**：① 四目录表：触发条件、动作、失败后果、C 行；② 发布失败与撤销失败的**不对称**（发布失败杀服务，撤销失败仅记录）写清；③ `dev_nr>0 || nr_domain>0` 与 `devman_id != 0` 是两组独立条件；④ 撤销在 `shutting_down` 时的错误压制有锚点；⑤ 明确 `setuid(0)` 在发布路径第二次出现的原因。

### 13-rs-monitor-recovery：存活监控与故障恢复

- **一句话定位**：一个服务从"被怀疑"到"被处置"的完整旅程——心跳怎么发现故障，终止决策怎么选，清理怎么做，如何重启或退避。
- **讲什么**：看门狗定位（K-13-01）；心跳双时间戳（K-13-02）；`do_period` 三分支（K-13-03）；时间常数（K-13-04）；free pass（K-13-05）；`RS_NOPINGREPLY`（K-13-06）；SIGTERM→SIGKILL（K-13-07）；`update_period` 超时（K-13-08）；`do_sigchld`（K-13-09）；终止决策树（K-13-10）；两阶段清理（K-13-11）；处决与模拟崩溃（K-13-12）；detach（K-13-13）；两条恢复路径（K-13-14）；恢复脚本（K-13-15）；退避（K-13-16）；核心服务死亡（K-13-17）；信号分类（K-13-18）；栈回溯（K-13-19）；Rust 决策切片（K-13-20）。
- **不讲什么**：`stop_service` 的发起侧（14，本篇只接收 SIGTERM 后的路径）；LU 状态机（16，只写超时调用 `end_update`）；`clone_service`/`update_service` 机制（11，只写调用）；`unpublish_service` 机制（12）；`sys_kill`/`waitpid` 签名（18）；RS 自身退出（17）。
- **前置**：01、02、06、10、11、12。**后置**：14/15/16/17/18。
- **事实底线（ground truth）**：`request.c:943-1090`（`do_period`/`do_sigchld`）、`request.c:1051-1090`；`update.c:371-396`（`update_period`）；`manager.c:360-526,1033-1050,1055-1180,1185-1298`；`servers/rs/const.h:25,31,45,48-51,58,116`；`sys/sys/signal.h:280-286`；`main.c:631-704`；Rust：`monitor.rs`、`recovery.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-13-01 | 看门狗定位 | 动机 |
| K-13-02 | 双时间戳 | 协议 |
| K-13-03 | 三分支 | 核心决策 |
| K-13-04 | 时间常数 | 阈值 |
| K-13-05 | free pass | 例外 |
| K-13-06 | NOPINGREPLY | 状态标记 |
| K-13-07 | 停止升级 | 超时处置 |
| K-13-08 | 更新超时 | 与 16 接缝 |
| K-13-09 | SIGCHLD 回收 | 旁路清理 |
| K-13-10 | 终止决策树 | 处置主体 |
| K-13-11 | 两阶段清理 | 清理协议 |
| K-13-12 | 处决/模拟崩溃 | 原语 |
| K-13-13 | detach | 降级旁路 |
| K-13-14 | 两条恢复 | 复活 |
| K-13-15 | 恢复脚本 | 外部恢复 |
| K-13-16 | 退避 | 防抖 |
| K-13-17 | 核心死亡 | 系统级边界 |
| K-13-18 | 信号分类 | 信号面 |
| K-13-19 | 栈回溯 | 诊断 |
| K-13-20 | Rust 决策切片 | 实现映射 |

- **验收标准**：① 一张"一个故障的旅程"总图：超时/崩溃/停止请求 → 检测 → 处决 → 清理 → 重启/退避/放弃/detach；② `do_period` 三分支判定表（条件、动作、进入下一 tick 的状态）；③ 两阶段清理的"第一/第二阶段各做什么、`RS_DEAD` 作为分界"讲清，并说明 `RS_REINCARNATE` 为什么不释放槽；④ `SF_NORESTART`/`SF_DET_RESTART`/`SF_USE_SCRIPT`/`SF_NO_BIN_EXP` 四个策略位在终止决策中的作用各一行；⑤ 信号分类表覆盖 LETHAL 6 个 + KILL/PIPE，并说明栈回溯只在非 ABRT 的致命信号上做。

### 14-rs-control-requests：控制请求

- **一句话定位**：外部世界操作服务生命周期的八个入口——每个 handler 都是"权限 → 机制调用 → 回复（或延迟回复）"的同构结构。
- **讲什么**：公共骨架（K-14-01）；`do_up` 全流程（K-14-02）；强制 init 旗标（K-14-03）；重复性检查（K-14-04）；回复双模式（K-14-05）；`do_down`（K-14-06）；`do_restart`（K-14-07）；clone/unclone（K-14-08）；`do_edit` 八步（K-14-09）；`do_refresh`（K-14-10）；`do_shutdown`（K-14-11）；`stop_service`（K-14-12）；延迟回复三写（K-14-13）；Rust 实现（K-14-14）。
- **不讲什么**：每个 handler 调用的机制（08/11/13/16）；访问控制细节（05）；`start_service`/`run_service` 机制（06/11）；LU 的 `RS_UPDATE`（16，本篇只在 `do_up` 的旗标映射里引用同款 `SEF_INIT_*`）。
- **前置**：05、06、08、11、13。**后置**：15/16/17/18。
- **事实底线（ground truth）**：`request.c:15-457`（八个 handler 全文）；`manager.c:988-1008`（`stop_service`）；`include/minix/rs.h:33-52`；`include/minix/sef.h:93-101`；Rust：`request.rs`、`shell_request.rs` 的控制臂。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-14-01 | 公共骨架 | 全篇结构 |
| K-14-02 | `do_up` | 最重要入口 |
| K-14-03 | 旗标映射 | 注入面 |
| K-14-04 | 重复检查 | 输入约束 |
| K-14-05 | 回复双模式 | 协议 |
| K-14-06 | `do_down` | 停止 |
| K-14-07 | `do_restart` | 重启 |
| K-14-08 | clone/unclone | 副本策略 |
| K-14-09 | `do_edit` | 重配置 |
| K-14-10 | `do_refresh` | 刷新 |
| K-14-11 | `do_shutdown` | 关机 |
| K-14-12 | `stop_service` | 公共原语 |
| K-14-13 | 延迟回复三写 | 并发/原子性 |
| K-14-14 | Rust 实现 | 实现映射 |

- **验收标准**：① 八个 handler 一张总表：入口校验、主要动作、回复方式、错误码；② `do_up` 与 `do_edit` 的流程可以对照 11/08 的机制调用讲清；③ 三种回复行为（立即 OK、`EDONTREPLY` + LATEREPLY、直接错误）的区别表；④ `do_down` 对已终止服务的快路径有锚点；⑤ `do_restart` 只允许恢复脚本触发这一约束与脚本保护代码有锚点。

### 15-rs-query-requests：查询与故障注入

- **一句话定位**：RS 的只读观测面与故障注入口——按名字查端点、导出进程表、系统控制、让服务自己崩溃。
- **讲什么**：观测/注入两面（K-15-01）；`do_lookup`（K-15-02）；`do_getsysinfo`（K-15-03）；`do_sysctl` 五子操作（K-15-04）；`UPD_RUN` 的延迟回复（K-15-05）；`do_fi`（K-15-06）；状态打印族（K-15-07）；Rust 分类（K-15-08）。
- **不讲什么**：权限判定（05）；LU 状态机（16，只写调用点）；`copy_label` 的拷入（08）；`sys_datacopy` 签名（18）；打印族的未来下放（在 §3.4 注记）。
- **前置**：05、08、13。**后置**：16/18。
- **事实底线（ground truth）**：`request.c:1095-1260`（四个 handler）；`utility.c:69-77,142-218,485-546`；`include/minix/com.h:485-492`；`include/minix/sysinfo.h:11,15,16`；Rust：`query.rs`、`shell_request.rs` 的查询臂。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-15-01 | 观测/注入两面 | 定位 |
| K-15-02 | `do_lookup` | 名字解析 |
| K-15-03 | `do_getsysinfo` | 表导出 |
| K-15-04 | `do_sysctl` | 系统控制 |
| K-15-05 | `UPD_RUN` 回复 | 与 16 接缝 |
| K-15-06 | `do_fi` | 故障注入 |
| K-15-07 | 打印族 | 诊断输出 |
| K-15-08 | Rust 分类 | 实现映射 |

- **验收标准**：① 四个请求一张表：消息、入参、返回、权限、承载小节；② `do_getsysinfo` 三种模式与"尺寸必须精确相等"的原因（进程表是 ABI）讲清；③ 如实写出 `do_sysctl` **没有权限检查**这一事实，不美化；④ `do_fi` 只发一条异步消息、真正的崩溃发生在服务侧（回指 13 的终止路径）；⑤ 打印族格式示例包含 active/version 标记与更新旗标位串。

### 16-rs-live-update：Live Update：状态机与状态迁移

- **一句话定位**：不停机替换一个或多个服务——从 `RS_UPDATE` 入口到四阶段状态机，以及 prepare 阶段的状态数据迁移。
- **讲什么**：LU 不变量（K-16-01）；四阶段（K-16-02）；`do_update` 十步（K-16-03）；旗标与 VM 预分配（K-16-04）；五查（K-16-05）；链部分排序（K-16-06）；旗标传播与 VM/RS 指针（K-16-07）；链操作（K-16-08）；回滚（K-16-09）；`srv_update` 与 VM 时序（K-16-10）；prepare（K-16-11）；`start_update`（K-16-12）；单组件两半（K-16-13）；abort（K-16-14）；`end_update` 四角色（K-16-15）；`end_srv_update`（K-16-16）；state data 流程（K-16-17）；线格式（K-16-18）；label 回退（K-16-19）；VM 保底（K-16-20）；grant 生命周期（K-16-21）；回复三值（K-16-22）；ASR 边界（K-16-23）；VM 旗标三兄弟（K-16-24）；Rust 实现（K-16-25）。
- **不讲什么**：`clone_service`/`update_service` 的机制（11）；`run_service`/`end_srv_init`（06）；`cleanup_service`（13）；RS 自身的 LU 特例（17）；`vm_update`/`vm_prepare` 的 VM 侧实现（`../02-stage-vm/25-rs-services.md` 与 18）；`rs_receive_ticks` 的 IPC filter 原语实现（18，本篇只写用途）。
- **前置**：02、06、08、11、13。**后置**：17/18。
- **事实底线（ground truth）**：`request.c:534-938`（`do_update`/`do_upd_ready`）；`update.c:1-1011` 全篇；`manager.c:174-284`（`init_state_data`）；`include/minix/rs.h:58-59,88-100`；`include/minix/ipc_filter.h`；`servers/rs/const.h:58,74-76,83,105-120`；`include/minix/sef.h:213-242`；`lib/libmagicrt/magic_util.c:82`；Rust：`live_update.rs`、`state_data.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-16-01 | LU 不变量 | 动机 |
| K-16-02 | 四阶段 | 骨架 |
| K-16-03 | `do_update` | 入口 |
| K-16-04 | 旗标与预分配 | 解码 |
| K-16-05 | 五查 | 校验 |
| K-16-06 | 链排序 | 数据结构 |
| K-16-07 | 旗标传播 | 多组件语义 |
| K-16-08 | 链操作 | 生命周期 |
| K-16-09 | 回滚 | 失败路径 |
| K-16-10 | VM 时序 | 最难分支 |
| K-16-11 | prepare | 阶段二 |
| K-16-12 | `start_update` | 阶段三入口 |
| K-16-13 | 单组件两半 | 执行单元 |
| K-16-14 | abort | 中止 |
| K-16-15 | end 四角色 | 收尾分类 |
| K-16-16 | surviving/exiting | 收尾动作 |
| K-16-17 | 状态迁移流程 | 阶段二数据 |
| K-16-18 | 线格式 | 协议 |
| K-16-19 | label 解析 | 输入解析 |
| K-16-20 | VM 保底 | 可用性 |
| K-16-21 | grant 生命周期 | 资源 |
| K-16-22 | 回复三值 | 协议 |
| K-16-23 | ASR 边界 | 范围澄清 |
| K-16-24 | VM 旗标 | VM 交互 |
| K-16-25 | Rust 状态机 | 实现映射 |

- **验收标准**：① 四阶段状态机图，标注每个阶段允许的 abort 行为与产出；② 链排序规则用"先普通、再 VM、最后 RS"的例子（含后插入一个普通服务）演示；③ `end_update` 四角色判定表覆盖反向遍历的累积状态；④ 多组件含 VM 的时序图必须出现（prepare 顺序、`vm_prepare`、`rs_receive_ticks` 等待、VM 先 init、其余 complete、VM 取消回复）；⑤ state data 解析的失败码（`E2BIG`/`EINVAL`/`ESRCH`/`ENOMEM`）逐项对应锚点；⑥ ASR 一节明确"RS 只传旗标与计数，语义在 SEF/服务侧"。

### 17-rs-self-lifecycle：RS 自身生命周期

- **一句话定位**：RS 管理所有服务，那谁来管 RS——它自己的重启、热更新、回滚、信号管理器备份与两实例模型。
- **讲什么**：自引用特例总览（K-17-01）；boot 自升级角色分派（K-17-02）；`sef_cb_init_restart`（K-17-03）；`sef_cb_init_lu`（K-17-04）；RS 回滚特例（K-17-05）；信号管理器备份对（K-17-06）；YIELD 交接（K-17-07）；两实例模型（K-17-08）；退出路径（K-17-09）；Rust 实现（K-17-10）。
- **不讲什么**：LU 通用状态机（16）；boot 四步（09，只写自升级调用点）；`update_service` 机制（11）；信号管理器原语细节（03）；`sys_whoami`/`vm_update` 签名（18）。
- **前置**：06、09、11、13、16。**后置**：18/99。
- **事实底线（ground truth）**：`main.c:436-491,499-586`；`update.c:230-256,330-366,673-689,883-887,952-956`；`utility.c:387-414`；`manager.c:765-779`；`include/minix/const.h:151,154`（`ROOT_SYS_PROC`/`RST_SYS_PROC`）；Rust：`self_lifecycle.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-17-01 | 自引用特例 | 定位 |
| K-17-02 | 自升级角色 | boot 场景 |
| K-17-03 | `init_restart` | 重启入口 |
| K-17-04 | `init_lu` | 热更新入口 |
| K-17-05 | RS 回滚 | 失败特例 |
| K-17-06 | 信号管理器备份 | 高可用 |
| K-17-07 | YIELD 交接 | 控制权转移 |
| K-17-08 | 两实例模型 | 槽位/进程关系 |
| K-17-09 | 退出路径 | 终局 |
| K-17-10 | Rust 实现 | 实现映射 |

- **验收标准**：① 四个特例（boot 自升级、restart、LU、rollback）一张对比表：触发、输入、关键动作、失败后果；② 两实例模型图：RS 槽、新实例槽、pid/endpoint、`r_old_rp`/`r_new_rp` 在交接前后的状态；③ `sef_cb_init_lu` 的四个断言逐条解释；④ 为什么 RS 的回滚可能只需槽交换（`me != RS_PROC_NR` 分支）讲清；⑤ 信号管理器备份对（RS→副本，副本→NONE）与 `0x900` 判定掩码有锚点。

### 18-rs-external-interfaces：外部接口契约

- **一句话定位**：RS 的外依赖与消息槽的**唯一签名权威面**——其它篇只写"调用点与语义"，不重复签名。
- **讲什么**：单点权威的理由（K-18-01）；依赖面总图（K-18-02）；六类调用面契约表（K-18-03…K-18-08）；消息槽类型化（K-18-09/K-18-10）；阻塞/非阻塞使用规则（K-18-11）；RS↔VM 启动握手（K-18-12）；Rust 接缝与 shell 拆分（K-18-13）。
- **不讲什么**：任何机制的语义（各篇自述）；内核/VM/DS 侧的实现（前序或后续 stage）；常量数值（99）；`rs_start` 文本来源（07）。
- **前置**：01–17（签名汇总；单独阅读也可，但只有配合机制篇才有意义）。**后置**：无（参考面）。
- **事实底线（ground truth）**：`lib/libsys/*`（`sys_privctl.c`、`sched_start.c`、`srv_fork.c`、`getepinfo.c`、`getprocnr.c`、`vm_update` 等）；`include/minix/ipc.h` 消息槽区段（1855-1906、1048-1072、1420-1428、1466-1474、2672-2675）；各机制篇的调用点锚点（见 §2 K-18-* 行）；`../02-stage-vm/25-rs-services.md` §1.5/§2.7（VM 侧握手）；Rust：`minix-types/src/ipc/rs.rs`、`minix-sys`、`shell_request.rs`/`shell_update.rs`/`trap_api.rs`。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-18-01 | 单点权威理由 | 文档学规则 |
| K-18-02 | 依赖面总图 | 导航 |
| K-18-03 | `sys_*` 面 | 内核契约 |
| K-18-04 | `srv_*` 面 | PM 契约 |
| K-18-05 | `vm_*` 面 | VM 契约 |
| K-18-06 | `ds_*` 面 | DS 契约 |
| K-18-07 | `sched_*` 面 | 调度契约 |
| K-18-08 | 其它面 | 装载/驱动契约 |
| K-18-09 | 消息槽类型化 | 载荷契约 |
| K-18-10 | 尺寸差异 | 线格式边界 |
| K-18-11 | 阻塞规则 | 使用约束 |
| K-18-12 | RS↔VM 握手 | 跨模块契约 |
| K-18-13 | Rust 接缝 | 实现边界 |

- **验收标准**：① 六类面每类一张表：函数/消息、签名要点、RS 调用点（篇 + 函数 + 行）、失败语义；② 每个外部调用在机制篇都有对应"调用点"提示，且本篇是唯一写签名的地方（单点权威自检）；③ 消息槽类型化给出 `mess_rs_*`/`mess_lsys_*` 清单与 Rust 类型对应；④ 阻塞/非阻塞规则至少覆盖 `ipc_sendrec`（发布/devman）与 `asynsend`（握手/发布 prepare/信号投递）两类；⑤ RS↔VM 握手说明公开表 grant 的创建（09）与消费（VM 侧）两端，并标注消费侧在 `02-stage-vm`。

### 99-rs-global-concepts：常量、错误与诊断词典

- **一句话定位**：本 stage 的查表终点——所有常量、旗标、错误码与 Rust 权威落点集中一处，杜绝"双真相源"。
- **讲什么**：词典规则（K-99-01）；消息类型（K-99-02）；时间/状态机常量（K-99-03）；请求/系统/SEF 旗标（K-99-04…K-99-06）；IPC filter 常量（K-99-07）；内核操作码与端点（K-99-08/K-99-09）；VM 子操作（K-99-10）；错误表（K-99-11）；Rust 权威映射（K-99-12）；hz 公式（K-99-13）；`Errno`（K-99-14）；显式排除项（K-99-15）。
- **不讲什么**：任何机制语义（各篇自述）；诊断输出的实现（15）；构建系统细节（只写排除理由与 feature 开关）。
- **前置**：无（词典；但正确使用需要相应机制篇）。**后置**：全部篇的常量引用都指向本篇。
- **事实底线（ground truth）**：`include/minix/com.h:55-67,342-353,442-446,463-492,627,736,741-745`；`servers/rs/const.h` 全篇；`include/minix/rs.h:12,22,29-30,33-52,58-59,191-206`；`include/minix/sef.h:85-101,213-242`；`include/minix/ipc_filter.h`；`include/minix/endpoint.h`；`error.c:15-28,48,56`；`os/servers/rs/Cargo.toml`；`minix3/share/mk/bsd.own.mk:1499`；`minix3/minix/servers/rs/Makefile`；Rust：`minix-types` 各常量模块。
- **知识点清单**：

| 编号 | 名称 | 为什么归本篇 |
|------|------|-------------|
| K-99-01 | 词典规则 | 使用前提 |
| K-99-02 | `RS_*` 消息 | 分派基础 |
| K-99-03 | 时间/状态常量 | 参数来源 |
| K-99-04 | `RSS_*` | 请求旗标 |
| K-99-05 | `SF_*` | 策略旗标 |
| K-99-06 | `SEF_*` | 协议旗标 |
| K-99-07 | `IPCF_*`/`ANY_*` | 过滤常量 |
| K-99-08 | `SYS_*` 操作码 | 内核面 |
| K-99-09 | 端点常量 | 寻址 |
| K-99-10 | `VM_RS_*` | VM 子操作 |
| K-99-11 | 错误表 | 错误语义 |
| K-99-12 | Rust 权威映射 | 防漂移 |
| K-99-13 | hz 公式 | 时间正确性 |
| K-99-14 | `Errno` | 错误类型 |
| K-99-15 | 排除项 | 范围声明 |

- **验收标准**：① 每个常量族一张表，列名、值、C 锚点、Rust 落点；② 覆盖率可用 grep 对账（15 个 `RS_*`、16 位 `r_flags`、13 位 `SF_*`、20 个 `RSS_*`）；③ 每个 Rust 落点唯一（无重复定义）；④ 排除项逐条给理由（`RS_USE_PAGING` 死配置、调试宏、C 构建文件）；⑤ 错误表覆盖 `init_strerror` 与 `lu_strerror` 的全部条目，并说明 `EGENERIC` 的来历。

---

## 6. 变更表

> 双方向规则：**存量方向看去向**——拆分/合并涉及的旧知识点逐条给出新位置（本表"去向"列 + §2 的编号分组）；**新增方向看来源**——现有文档没有的知识点给证据锚点（§3.2 缺口表 + §7）。所有旧文档在新目录落地后整体归档不删（B 相执行）。

| 操作编号 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点（去向） |
|---------|------|--------|--------|------|------------------|
| O-01 | 重写保留 | 00 全篇 | 新 00 | 导航枢纽职责不变，去掉与 01/09 的重复细节与旧编号表 | K-00-01…09（全部留在 00） |
| O-02 | 拆分 | 01（1044 行） | 新 01（SEF/main/reply 约定）、新 09（四步 boot）、新 06 §（catch ready 与自模拟）、新 17 §（自升级细节）、新 00 §（自举概述） | 旧 01 同时承担导航、机制重述与 boot 全流程，职责越界；按"运行框架 / 启动主线 / 协议 / 自引用"四类归属拆开 | K-01-*、K-09-*、K-06-*、K-17-*；旧 01 的机制复述（priv/send mask/目录表）不搬，只留引用 |
| O-03 | 重写保留 | 02 全篇 | 新 02 | 数据底座职责不变；`r_upd` 状态语义移交 16，字段归属表更新 | K-02-01…17 |
| O-04 | 重写保留 | 03 全篇 | 新 03 | 权限机制职责不变；删除对隐藏设计目录的引用；boot 装配保留在本篇（09 只给调用点） | K-03-01…15 |
| O-05 | 重排（+1） | 04 全篇 | 新 05 | 访问控制在机制组中后置到 send mask 之后（先"服务能对谁说"再"谁能对 RS 说"） | K-05-01…07 |
| O-06 | 重排（−1） | 05 全篇 | 新 04 | 与 03 的 `s_ipc_to` 字段连续，构成完整的"权限信封"故事 | K-04-01…09 |
| O-07 | 拆分+重排 | 06 全篇 | 新 10（主循环/分类/回复/空闲）、新 01 §（回复约定与信号注册）、新 13 §（信号行为与心跳写入） | 旧 06 同时含骨架、心跳写点、信号处置三类内容 | K-10-*、K-01-05/06/10、K-13-18/19 |
| O-08 | 合并 | 07 全篇 + 15 全篇 | 新 13 | 检测（监控）与处置（终止/恢复）互为因果，拆开必互相前向引用；合篇后一个故障的旅程一次讲完 | K-13-01…20（旧 07 的 K-13-01…09 + 旧 15 的 K-13-10…19 + 新增 K-13-20） |
| O-09 | 拆分+新建 | 08 全篇 | 新 08（主）+ 新 07（新增客户端面） | 旧 08 缺"声明从哪来"；新 07 补 `system.conf`/`minix-service`，新 08 承接 `rs_start` 落地与映像 | K-08-01…20；K-07-01…11（新增） |
| O-10 | 合并 | 09 全篇 | 新 08（合并入配置篇） | 映像分支（`RSS_COPY/REUSE`）与 `edit_slot` 互相引用，合并消除循环依赖 | K-08-11、K-08-15…20 |
| O-11 | 重写+吸收 | 10 全篇 | 新 11（+旧 16 §2.3 的 `update_service`） | 创建与实例切换同属"实例生命周期原语"，集中到一篇；`update_service` 为重启/LU 共用 | K-11-01…18（K-11-16 从旧 16 吸收） |
| O-12 | 重写保留 | 11 全篇 | 新 12 | 发布职责不变；补"四目录"总模型与错误不对称说明 | K-12-01…08 |
| O-13 | 拆分+重排 | 12 全篇 | 新 06（协议本体）、新 09 §（boot 调用点） | ready 协议是 boot 与创建共用的接口，提前到机制组末尾 | K-06-01…13 |
| O-14 | 重写保留 | 13 全篇 | 新 14 | 控制请求职责不变；去掉与 15 的更新机器细节，只留调用点 | K-14-01…14 |
| O-15 | 重写保留 | 14 全篇 | 新 15 | 查询职责不变；打印族加"未来下放"注记 | K-15-01…08 |
| O-16 | 合并+拆分 | 16 全篇 + 17 全篇 | 新 16（状态机+状态迁移）、新 17 §（RS 特例） | 旧 16 引用旧 17 的数据格式，拆开必然前向；合并后 LU 一篇讲透；RS 自引用抽到 17 | K-16-01…25；K-17-01…10 |
| O-17 | 重写保留 | 18 全篇 | 新 17 | RS 自身生命周期职责不变；boot 自升级调用点回指 09 | K-17-01…10 |
| O-18 | 重写保留 | 19 全篇 | 新 18 | 外部契约职责不变；补 shell 拆分形态与 RS↔VM 握手 | K-18-01…13 |
| O-19 | 重写保留 | 99 全篇 | 新 99 | 词典职责不变；补错误表连接、排除项、Rust 落点权威映射 | K-99-01…15 |
| O-20 | 新建 | —（旧 08 仅两处脚注） | 新 07 | 覆盖审计 G-01/G-02/G-03/G-07/G-25 的载体 | K-07-01…11（全部新增/扩容） |
| O-21 | 归档 | 旧 `00`–`19`、`99` 共 21 个文件 | — | 重建完成后旧文件整体移入归档目录（B 相执行，不删） | 全部；迁移表见 §8.1 |

**删除条款（不进新正文，逐条给理由）**：

| 删除内容 | 旧位置 | 理由 |
|---------|--------|------|
| 测试计数与"已落地 N 项" | 01 §5.4、02 §5.5、03 §5.7、04 §5、05 §5、06 §5、07 §5、08 §5、09 §5、10 §5、11 §5、12 §5、13 §5、14 §5、15 §5、16 §5、17 §5、18 §5、19 §5、99 §5 | 易腐信息（抽查发现 01/04/06/07/10/12/15/16/17 与实现不符）；新篇只保留"验收标准"，测试清单由代码承担 |
| "Fix #NN / R4 / A4 / 接线落地"过程块 | 01 §3.4、08 §3.4、10 §3.5、12 §3.0b/§2.9b、13 §2.1b-2.1e、16 §3.0b、17 §3、99 §3.4 等 | 实现进度叙述不属于语义文档；其有效结论已并入 `os/` 代码注释与 todo.md |
| 隐藏设计目录的设计文档引用 | 00、02（3 处）、03 | AGENTS.md 隐藏目录约定：正式文档不得引用中间产物 |
| 旧文档编号与文件名引用 | 全篇 | 重建后编号含义变化，按 §8.2 迁移 |
| 裸 `manager.c:rproc（Lxxx，工具生成）` 类锚点 | 02/03/04/05/07/08/09/10/11/15 等大量 | 符号名错误（`rproc` 是全局表不是函数）；新篇锚点一律写成"函数名 + 行号 + 文件" |

---

## 7. 缺漏新篇（缺口逐项落实）

> §3.2 的 25 项缺口全部落实，无"待定"项。每项给出主题、重要性、原料、归属与验收；跨 stage 的显式排除项单独标注。

| 缺口 | 主题 | 为什么重要 | 原料（锚点） | 归哪一篇 | 验收标准 |
|------|------|-----------|-------------|---------|---------|
| G-01 | `system.conf` 指令集 | RS 请求的来源；旧目录零覆盖 | `minix3/etc/system.conf:1-529`；`parse.c:406-900` | 新 07 §2-§3 | 指令表覆盖 11 类指令；两个真实条目走读 |
| G-02 | `minix-service` 客户端与命令族 | 声明→请求→运维入口的完整链 | `parse.c:1229`；`minix-service.c:756` 与命令表 | 新 07 §4-§5 | 解析产物字段表；命令族清单 |
| G-03 | boot 配置双源事实 | 防止把 `system.conf` 的 priority/quantum 误当 boot 生效值 | `main.c:319-322`；`priv.h:93-100`；`table.c:15-50` vs `system.conf:21-22` | 新 07 §6 + 新 09 §3 | 双源对照表；boot 服务字段来源逐条 |
| G-04 | SEF 运行库内部 | `_start`→`main` 的空白与 init 分派 | `sef.c:185-230`；`sef_init.c` | 新 01 §2 | 时序图；拦截类型表 |
| G-05 | SEF ping 拦截 | 心跳闭环缺服务侧 | `sef_ping.c:21-38`；`sef.c:212` | 新 01 §4（13 回指） | 一次心跳的端到端时序 |
| G-06 | ASR 职责边界 | 防止把 ASR 算成 RS 功能 | `rs.h:39`；`sef.h:236,244`；`type.h:65`；`magic_util.c:82`；`procfs/service.c:347` | 新 16 §6 | RS 侧只做旗标传递与计数 |
| G-07 | RS 关机路径全貌 | 关机语义与 `shutting_down` 例外散在四处 | `request.c:431-457`；`manager.c:1119-1124`；`utility.c:449-464`；`manager.c:879,890`；`main.c:638-640` | 新 14 §关机 + 新 13 §收尾 | 一张关机影响表 |
| G-08 | RS↔VM 启动握手 | 公开表 grant 的消费侧是跨模块契约 | `main.c:185-189`；`02-stage-vm/25-rs-services.md` §1.5/§2.7 | 新 18 §握手 | 两端各一句、锚点各一处 |
| G-09 | 信号分类与栈回溯 | 旧文档漏两档差别与 VM 豁免原因 | `signal.h:280-286`；`main.c:681-696` | 新 13 §信号 | 分类表 + 栈回溯条件 |
| G-10 | 诊断输出族 | 运维可见性的全部出口 | `utility.c:142-218,485-546` | 新 15 §打印 | 输出格式族表 |
| G-11 | 错误表与命名面 | `init_strerror`/`lu_strerror` 的完整语义 | `error.c:15-28,48,56` | 新 99 §错误 | 两表条目 + `EGENERIC` 来源 |
| G-12 | LU 回复三值 | 决定谁收到回复的协议细节 | `const.h:74-76`；`update.c:952-956,981-990` | 新 16 §end | 三值判定表 |
| G-13 | VM 交互旗标 | 内存迁移的开关 | `update.c:442-449,637-639,925` | 新 16 §旗标 | 置位/清位点各一处 |
| G-14 | 多组件 VM 时序 | LU 最难分支 | `update.c:483-508,585-613,952-956` | 新 16 §VM multi | 时序图（6 步） |
| G-15 | heap/mmap 预分配 | 更新不丢内存的关键 | `request.c:591-599,768-810`；`const.h:83`；`utility.c:56-60` | 新 16 §入口 | 两个预分配路径各一段 |
| G-16 | LU 操作员控制面 | 运维入口 | `request.c:1189-1215`；`utility.c:516-546` | 新 15 §sysctl | 五子操作表 |
| G-17 | 入口装载链 | 补齐 `_start`→`main` | `sef.c:185-230` | 新 01 §2（同 G-04） | 时序图覆盖 |
| G-18 | 死配置与调试宏排除 | 明确不建模的边界 | `servers/rs/const.h:6-15,84`；`plan.md` §5.4 | 新 99 §排除 | 逐项理由 |
| G-19 | 驱动面 privctl 与内核覆盖规则 | 旧 03 有半页但无结论 | `do_privctl.c:187-251,280-368` | 新 03 §操作面 | "用哪些/不用哪些"两表 |
| G-20 | 状态数据上限常量 | 缓冲与失败语义 | `rs.h:58-59`；`sys_config.h:9`；`manager.c:178-232` | 新 16 §状态数据 | 上限表 + 越界错误码 |
| G-21 | 阻塞/非阻塞规则 | 防止读者以为所有外部调用都异步 | `manager.c:849,906`；`utility.c:231` | 新 18 §规则 | 一句话规则 + 全部反例 |
| G-22 | PM 查询面差异 | 旧 01/04/19 说法不一 | `lib/libsys/getepinfo.c`；`getprocnr.c` | 新 18 §表 | 三 wrapper 归属表 |
| G-23 | 两实例模型对槽位关系的影响 | 初学者对 `RS_PROC_NR` 特判的理解前提 | `main.c:450-454`；`update.c:293-305` | 新 17 §模型 | 交接前后对照图 |
| G-24 | 测试基建现状 | 旧文档计数易腐；需要明确归属 | `os/qemu-tests/`；`minix3/minix/tests/`（无 rs）；`edge_todo.md` E9 | 各篇验收标准 + §3.5 第 10 项 | 无易腐计数；端到端指向 E9 |
| G-25 | Rust shell 层归属 | `shell_request.rs`/`shell_update.rs`/`trap_api.rs` 是 19 接线落地形态 | `os/servers/rs/src/{shell_request,shell_update,trap_api}.rs`；`testutil.rs` | 新 18 §Rust 接缝 | 模块职责表 |

**跨 stage 显式排除（不算缺口）**：trap 桥与用户态入口（`01-stage-kernel`）；VM 侧 `do_rs_*` 四个请求实现（`02-stage-vm/25-rs-services.md`）；RS 动态加载 IS/DEVMAN/INPUT 的注册面（`08-stage-is` 等，`edge_todo.md` E-ISBOOT）；端到端联调（19-stage-integration / E9）；`/etc/rc` 启动脚本（18-stage-commands，`edge4.md` OQ-3 已裁决盘上文件面）。

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表（旧文档逐节目录 → 新位置）

> 阅读方法：同去向的连续小节合并为一行。迁移类型：原样搬移 / 改写 / 合并 / 拆分 / 删除。"删除"仅指不进入新正文（§6 删除条款），旧文件仍归档保留。

**旧 00（总览）**

| 旧位置 | 旧内容一句话 | 新位置 | 迁移类型 | 备注 |
|--------|-------------|--------|---------|------|
| §1.0-1.3 | RS 是什么/身份/boot 位置/两个状态机 | 新 00 §1-§3 | 改写 | 机制细节不再展开 |
| §2.0-2.2 | 启动主线图与两阶段 | 新 00 §2 | 改写 | 图以新 09 为准 |
| §3.0-3.2 | 生命周期图与语义单元 | 新 00 §3 | 改写 | 机制指向新 11/12/13/16 |
| §4.0 | 21 篇文档总览表 | 新 00 §4 | 改写 | 换新导航表 |
| §4.1-4.2 | 依赖图与跨 stage 交叉引用 | 新 00 §4 | 改写 | 依赖图重画后无环 |
| §5.1-5.3 | 组织原则 | 新 00 §5 | 原样搬移 | |
| §5.4 | ARCH 一览 | 新 00 §5 | 改写 | 变索引，细节归各篇 |
| §5.5 | 覆盖契约 | 新 00 §5 | 改写 | 数字用 grep 可复核 |
| §6 | 阅读路线 | 新 00 §6 | 改写 | 路线按新目录重排 |
| §7-8 | 过渡与参见 | 新 00 §7 | 改写 | |

**旧 01（boot-init，1044 行）**

| 旧位置 | 旧内容一句话 | 新位置 | 迁移类型 | 备注 |
|--------|-------------|--------|---------|------|
| §1.0-1.2 | 自举问题与 boot 主线图 | 新 00 §2 + 新 09 §1 | 拆分/改写 | 概述留 00 |
| §1.3 | SEF 生命周期 | 新 01 §2 | 原样搬移 | 扩写内部机制 |
| §1.4 | 四类消息 | 新 10 §2 | 原样搬移 | |
| §2.1 | main() 逐段 | 新 01 §3 + 新 10 §2 | 拆分 | 骨架→01，分派→10 |
| §2.2 | sef_local_startup | 新 01 §2 | 原样搬移 | |
| §2.3.1 | 预备（verbose/hz/grant/复位/getimage） | 新 09 §2 | 原样搬移 | |
| §2.3.2 | 表计数校验与复位 | 新 09 §2 | 原样搬移 | |
| §2.3.3 | Step 1 全字段装配 | 新 09 §3 | 拆分/改写 | 字段语义引 03/04 |
| §2.3.4 | Step 2 放行与捕获 | 新 09 §4 | 改写 | 协议引 06 |
| §2.3.5-2.3.6 | Step 3/4 | 新 09 §4 | 原样搬移 | |
| §2.3.7 | USE_LIVEUPDATE 自升级 | 新 09 §4 + 新 17 §2 | 拆分 | 调用点/机制分离 |
| §2.4 | boot_image_info_lookup | 新 09 §2 | 原样搬移 | |
| §2.5 | catch_boot_init_ready 调用点 | 新 06 §3 | 合并 | 与 12 §2.5 合一 |
| §2.6 | get_work | 新 10 §3 | 原样搬移 | |
| §2.7 | table.c 三表 | 新 09 §2 | 原样搬移 | |
| §3.1-3.8 | Rust 设计决策 | 新 01/06/09/10/17 对应节 | 拆分 | 过程注记删除 |
| §4.1-4.6 | 实现详解 | 新 01 §5、新 09 §5 | 改写 | 只留形态与不变量 |
| §5.1-5.4 | 测试要点与计数 | — | 删除 | 改验收标准 |
| §6-7 | 过渡/参见 | 新 09 §7 | 改写 | |

**旧 02（process-table）**

| 旧位置 | 旧内容一句话 | 新位置 | 迁移类型 | 备注 |
|--------|-------------|--------|---------|------|
| §1.0-1.4 | 登记表概念/双表/旗标/生命周期 | 新 02 §1 | 原样搬移 | |
| §2.1-2.2 | 全局表与尺寸 | 新 02 §2 | 原样搬移 | |
| §2.3 | rproc 全字段 | 新 02 §2 | 改写 | 归属列更新（字段→新篇号） |
| §2.4 | rprocpub 全字段 | 新 02 §2 | 原样搬移 | |
| §2.5-2.6 | 两旗标表与预设 | 新 02 §3 | 原样搬移 | |
| §2.7 | rprocupd/rupdate 形状 | 新 02 §3 | 改写 | 状态语义移 16 |
| §2.8 | rinit | 新 02 §3 | 原样搬移 | |
| §2.9-2.11 | 查找/分配/释放/isokendpt/实例 | 新 02 §4 | 原样搬移 | |
| §2.12 | boot 消费点 | 新 09 §3 | 原样搬移 | |
| §3.1-3.10 | Rust 设计决策 | 新 02 §5 | 改写 | 去过程注记 |
| §4 | 实现详解 | 新 02 §5 | 改写 | |
| §5 | 测试要点 | — | 删除 | |
| §6-7 | 过渡/参见 | 新 02 §7 | 改写 | 隐藏设计目录引用删除 |

**旧 03（privilege）**

| 旧位置 | 旧内容一句话 | 新位置 | 迁移类型 | 备注 |
|--------|-------------|--------|---------|------|
| §1.0-1.5 | 权限概念/字段/装配/操作面/伴随原语 | 新 03 §1 | 改写 | 装配保留、引用方去重 |
| §2.1-2.4 | struct priv/默认宏/s_flags/boot Step1 | 新 03 §2-§3 | 原样搬移 | boot 装配仍是本篇正文 |
| §2.5-2.6 | 位图原语/privctl 操作面 | 新 03 §3 | 改写 | 补驱动面结论 G-19 |
| §2.7-2.8 | sched/信号管理器 | 新 03 §4 | 原样搬移 | |
| §3 | Rust 设计决策 | 新 03 §5 | 改写 | 隐藏设计目录引用删除 |
| §4 | 实现详解 | 新 03 §5 | 改写 | |
| §5 | 测试要点与计数 | — | 删除 | |
| §6-7 | 过渡/参见 | 新 03 §7 | 改写 | |

**旧 04（access-control）与旧 05（ipc-sendmask）**

| 旧位置 | 旧内容一句话 | 新位置 | 迁移类型 | 备注 |
|--------|-------------|--------|---------|------|
| 04 §1 | 谁有资格命令 RS | 新 05 §1 | 原样搬移 | 编号 04→05 |
| 04 §2.1-2.4 | 三个判定函数与调用点 | 新 05 §2-§4 | 原样搬移 | |
| 04 §2.5 | RUPDATE_IS_UPDATING | 新 05 §4（一行）+ 新 16 | 拆分 | |
| 04 §3-4 | Rust 决策与实现 | 新 05 §5 | 改写 | |
| 04 §5 | 测试要点 | — | 删除 | |
| 04 §6-7 | 过渡/参见 | 新 05 §7 | 改写 | |
| 05 §1 | 服务能向谁发消息 | 新 04 §1 | 原样搬移 | 编号 05→04 |
| 05 §2.1-2.6 | 两条路径/分词/两次扫描/ALL | 新 04 §2-§4 | 原样搬移 | |
| 05 §2.7 | init_slot→edit_slot→init_privs | 新 04 §4 + 新 08 | 拆分 | 拷贝点归 08 |
| 05 §3-4 | Rust 与实现 | 新 04 §5 | 改写 | |
| 05 §5 | 测试要点 | — | 删除 | |
| 05 §6-7 | 过渡/参见 | 新 04 §7 | 改写 | |

**旧 06（main-loop）与旧 07（period-heartbeat）**

| 旧位置 | 旧内容一句话 | 新位置 | 迁移类型 | 备注 |
|--------|-------------|--------|---------|------|
| 06 §1.0-1.2 | 运行时心脏/三活动/四类消息 | 新 10 §1-§2 | 原样搬移 | |
| 06 §2.1 | main() 逐段 | 新 10 §2-§3 | 原样搬移 | |
| 06 §2.2 | reply 协议 | 新 01 §3 | 拆分 | 公共原语前移 |
| 06 §2.3 | rs_isokendpt | 新 02 §4 + 新 10 §3 | 合并/改写 | 原语归 02 |
| 06 §2.4 | rs_asynsend | 新 01 §3 | 拆分 | |
| 06 §2.5 | idle/is_idle/DEAD 清理/副本补位 | 新 10 §4 | 原样搬移 | 机制引 11/13 |
| 06 §2.6 | 信号回调 | 新 01 §2（注册）+ 新 13 §6（行为） | 拆分 | |
| 06 §3-4 | Rust 分类器与实现 | 新 10 §5 | 改写 | |
| 06 §5 | 测试要点与计数 | — | 删除 | |
| 06 §6-7 | 过渡/参见 | 新 10 §7 | 改写 | |
| 07 §1.0-1.2 | 看门狗/心跳协议 | 新 13 §1-§2 | 原样搬移 | |
| 07 §2.1 | do_period 三分支 | 新 13 §3 | 原样搬移 | 崩溃路径机制在同篇 §5 起 |
| 07 §2.2 | 周期常量 | 新 13 §2 + 新 99 | 拆分 | 数值表归 99 |
| 07 §2.3 | 心跳检查与 free pass | 新 13 §3 | 原样搬移 | |
| 07 §2.4 | setalarm 重排 | 新 13 §3 | 原样搬移 | 首个 alarm 在 09 |
| 07 §2.5 | do_sigchld | 新 13 §6 | 原样搬移 | |
| 07 §2.6 | update_period | 新 16 §8 + 新 13 §3（调用点） | 拆分 | |
| 07 §3-5 | Rust 与实现/测试 | 新 13 §7 | 改写/删除 | 计数删除 |
| 07 §6-7 | 过渡/参见 | 新 13 §7 | 改写 | |

**旧 08（slot-config）与旧 09（exec）**

| 旧位置 | 旧内容一句话 | 新位置 | 迁移类型 | 备注 |
|--------|-------------|--------|---------|------|
| 08 §1.0-1.2 | 配置层/三阶段/init vs edit | 新 08 §1 | 原样搬移 | |
| 08 §2.1 | check_request | 新 08 §2 | 原样搬移 | |
| 08 §2.2 | copy 原语 | 新 08 §2 | 原样搬移 | |
| 08 §2.3 | init_slot | 新 08 §3 | 原样搬移 | |
| 08 §2.4 | edit_slot 覆盖表 | 新 08 §3 | 原样搬移 | |
| 08 §2.5 | build_cmd_dep | 新 08 §3 | 原样搬移 | boot 调用点标注 |
| 08 §2.6 | 继承默认 | 新 08 §3 | 原样搬移 | |
| 08 §3.1-3.4 | Rust 决策 | 新 08 §4 | 改写 | |
| 08 §3.5 | rs_start 字节 ABI | 新 07 §3（定义）+ 新 08 §4（解码） | 拆分 | |
| 08 §4 | 实现/不变量 | 新 08 §4 | 改写 | |
| 08 §5 | 测试要点与计数 | — | 删除 | |
| 08 §6-7 | 过渡/参见 | 新 08 §7 | 改写 | |
| 09 §1.0-1.3 | 两条映像路径/引用语义 | 新 08 §5 | 原样搬移 | |
| 09 §2.1-2.6 | read/share/free/srv_execve/do_exec/read_seg | 新 08 §5 | 原样搬移 | |
| 09 §3 | Arc/校验/延迟契约 | 新 08 §4 | 改写 | |
| 09 §4-5 | 实现/测试 | 新 08 §4 / — | 改写/删除 | |
| 09 §6-7 | 过渡/参见 | 新 08 §7 | 改写 | |

**旧 10（service-create）与旧 11（publish）**

| 旧位置 | 旧内容一句话 | 新位置 | 迁移类型 | 备注 |
|--------|-------------|--------|---------|------|
| 10 §1.0-1.3 | 为什么亲自创建/十一步/生命周期位置 | 新 11 §1 | 原样搬移 | |
| 10 §2.1-2.5 | 闸门/fork/三连/hack/vm_set_priv | 新 11 §2-§4 | 原样搬移 | |
| 10 §2.6-2.9 | clone/activate/clone_slot/swap | 新 11 §5-§6 | 原样搬移 | |
| 10 §3.1-3.5 | Rust 决策 | 新 11 §7 | 改写 | |
| 10 §4-5 | 实现/测试计数 | 新 11 §7 / — | 改写/删除 | |
| 10 §6-7 | 过渡/参见 | 新 11 §8 | 改写 | |
| 11 §1.0-1.3 | 四目录/独立理由/宽松撤销 | 新 12 §1 | 原样搬移 | |
| 11 §2.1-2.5 | DS/mapdriver/PCI/devman/unpublish | 新 12 §2-§4 | 原样搬移 | |
| 11 §3 | Rust 谓词 | 新 12 §5 | 改写 | |
| 11 §4-5 | 实现/测试 | 新 12 §5 / — | 改写/删除 | |
| 11 §6-7 | 过渡/参见 | 新 12 §7 | 改写 | |

**旧 12（init-run）与旧 13（control）**

| 旧位置 | 旧内容一句话 | 新位置 | 迁移类型 | 备注 |
|--------|-------------|--------|---------|------|
| 12 §1.0-1.3 | 握手理由/两处使用/自模拟 | 新 06 §1-§2 | 原样搬移 | |
| 12 §2.1 | run_service/start_service | 新 06 §2（run）+ 新 11 §2（start） | 拆分 | |
| 12 §2.2 | init_service 与 RS_INIT | 新 06 §2 | 原样搬移 | |
| 12 §2.3 | do_init_ready | 新 06 §3 | 原样搬移 | |
| 12 §2.4 | do_upd_ready | 新 06 §3 + 新 16 §5 | 拆分 | 链推进归 16 |
| 12 §2.5 | catch_boot_init_ready | 新 06 §3 | 合并 | 与 01 §2.5 合一 |
| 12 §2.6 | end_srv_init | 新 06 §4 | 原样搬移 | |
| 12 §3.1-3.4 | Rust 决策 | 新 06 §5 | 改写 | 已删函数的描述删除 |
| 12 §4-5 | 实现/测试 | 新 06 §5 / — | 改写/删除 | |
| 12 §6-7 | 过渡/参见 | 新 06 §7 | 改写 | |
| 13 §1.0-1.2 | 控制面/公共骨架/生命周期路径图 | 新 14 §1-§2 | 原样搬移 | 路径图重画 |
| 13 §2.1 | do_up | 新 14 §2 | 原样搬移 | |
| 13 §2.1b-2.1e | 四个 Fix 接线块 | — | 删除 | 结论并入正文 |
| 13 §2.2-2.7 | do_down/restart/clone/edit/refresh/shutdown/stop | 新 14 §3-§5 | 原样搬移 | |
| 13 §3 | Rust 决策 | 新 14 §6 | 改写 | |
| 13 §4-5 | 实现/测试 | 新 14 §6 / — | 改写/删除 | |
| 13 §6-7 | 过渡/参见 | 新 14 §7 | 改写 | |

**旧 14（query）、旧 15（terminate-restart）与旧 16（live-update）**

| 旧位置 | 旧内容一句话 | 新位置 | 迁移类型 | 备注 |
|--------|-------------|--------|---------|------|
| 14 §1.0-1.2 | 观测/注入/主循环位置 | 新 15 §1-§2 | 原样搬移 | |
| 14 §2.1-2.2 | lookup/getsysinfo | 新 15 §2-§3 | 原样搬移 | |
| 14 §2.3 | do_sysctl | 新 15 §3 | 原样搬移 | 更新机器引 16 |
| 14 §2.4 | do_fi | 新 15 §4 | 原样搬移 | |
| 14 §2.5 | 打印族 | 新 15 §5 | 原样搬移 | 加下放注记 |
| 14 §3 | Rust 分类 | 新 15 §6 | 改写 | |
| 14 §4-5 | 实现/测试 | 新 15 §6 / — | 改写/删除 | |
| 14 §6-7 | 过渡/参见 | 新 15 §7 | 改写 | |
| 15 §1.0-1.2 | 终局问题/状态机图 | 新 13 §1、§4 | 原样搬移 | |
| 15 §2.1 | terminate_service | 新 13 §4 | 原样搬移 | |
| 15 §2.2 | cleanup 两阶段 | 新 13 §5 | 原样搬移 | |
| 15 §2.3-2.4 | kill/crash/detach | 新 13 §5 | 原样搬移 | |
| 15 §2.5-2.7 | restart/reincarnate/run_script/instances | 新 13 §6-§7 | 原样搬移 | |
| 15 §3-4 | Rust 决策/实现 | 新 13 §8 | 改写 | |
| 15 §5 | 测试要点 | — | 删除 | 有重复编号，一并清掉 |
| 15 §6-7 | 过渡/参见 | 新 13 §9 | 改写 | |
| 16 §1.0-1.2 | LU 理由/状态机图 | 新 16 §1-§2 | 原样搬移 | |
| 16 §2.1 | do_update 十步 | 新 16 §3 | 原样搬移 | |
| 16 §2.2 | 链操作 | 新 16 §4 | 原样搬移 | |
| 16 §2.3 | update_service/rollback | 新 11 §6（机制）+ 新 16 §5（LU 场景） | 拆分 | |
| 16 §2.4-2.8 | prepare/start/srv/abort/end | 新 16 §5-§9 | 原样搬移 | |
| 16 §3.0b/3.1-3.4 | Rust 决策（含 Fix 块） | 新 16 §10 | 改写/删除过程叙述 | |
| 16 §4-5 | 实现/测试 | 新 16 §10 / — | 改写/删除 | |
| 16 §6-7 | 过渡/参见 | 新 16 §11 | 改写 | |

**旧 17（state-data）、旧 18（self-lifecycle）、旧 19（external）、旧 99（dictionary）**

| 旧位置 | 旧内容一句话 | 新位置 | 迁移类型 | 备注 |
|--------|-------------|--------|---------|------|
| 17 §1-2 | 状态迁移理由/三种载体/流程/结构 | 新 16 §7 | 合并/原样搬移 | 重复 H2 修正 |
| 17 §2.3-2.6 | eval/ipcf/VM 保底/grants | 新 16 §7 | 原样搬移 | |
| 17 §3 | Rust 决策（重复 H2） | 新 16 §10 | 改写/合并 | |
| 17 §4-5 | 实现/测试 | 新 16 §10 / — | 改写/删除 | |
| 17 §6-7 | 过渡/参见 | 新 16 §11 | 改写 | |
| 18 §1.0-1.2 | 自引用/四个特例 | 新 17 §1 | 原样搬移 | |
| 18 §2.1 | boot 自升级 | 新 09 §4（调用点）+ 新 17 §2 | 拆分 | |
| 18 §2.2-2.6 | restart/lu/rollback/srv_update/sigmgr | 新 17 §3-§6 | 原样搬移 | |
| 18 §2.9b | 接线落地块 | — | 删除 | 结论并入 §3-§6 |
| 18 §3-4 | Rust 决策/实现 | 新 17 §7 | 改写 | 错误旗标锚点修正 |
| 18 §5 | 测试要点 | — | 删除 | 重复编号 |
| 18 §6-7 | 过渡/参见 | 新 17 §8 | 改写 | |
| 19 §1.0-1.2 | 单点权威/依赖面 | 新 18 §1 | 原样搬移 | |
| 19 §2.1-2.6 | 六类调用面 | 新 18 §2-§4 | 原样搬移 | 补 G-21/G-22/G-08 |
| 19 §3.1-3.2 | 消息槽类型化 | 新 18 §5 | 原样搬移 | |
| 19 §3.3 | 56/64 差异与决策后记 | 新 18 §5 + 新 99 §6 | 拆分/改写 | 常量归 99 |
| 19 §4 | Rust 依赖面 | 新 18 §6 | 改写 | 补 shell 拆分 |
| 19 §5 | 测试要点 | — | 删除 | |
| 19 §6-7 | 过渡/参见 | 新 18 §7 | 改写 | |
| 99 §1 | 词典规则 | 新 99 §1 | 原样搬移 | |
| 99 §2.1-2.5 | 五组常量 | 新 99 §2-§4 | 原样搬移 | |
| 99 §3.1-3.3 | Rust 映射/hz/可验证性 | 新 99 §5 | 原样搬移 | |
| 99 §3.4 | 纯决策/shell 边界 | 新 18 §6 | 原样搬移 | |
| 99 §4 | minix-types 缺口补齐 | 新 99 §6 | 改写 | |
| 99 §5 | 测试要点 | — | 删除 | |
| 99 §6-7 | 过渡/参见 | 新 99 §7 | 改写 | |

### 8.2 引用迁移表

#### 8.2.1 文件名映射（批量替换的主键）

| 旧文件名 | 新文件名 | 说明 |
|---------|---------|------|
| `00-rs-overview.md` | `00-rs-overview.md` | 同名 |
| `01-rs-boot-init.md` | `09-rs-boot-init.md` | **编号变化**；若上下文指 SEF/main/ready，则分别改指 `01`/`06`（按 §8.1） |
| `02-rs-process-table.md` | `02-rs-process-table.md` | 同名 |
| `03-rs-privilege.md` | `03-rs-privilege.md` | 同名 |
| `04-rs-access-control.md` | `05-rs-access-control.md` | **编号 +1** |
| `05-rs-ipc-sendmask.md` | `04-rs-ipc-sendmask.md` | **编号 −1** |
| `06-rs-main-loop.md` | `10-rs-main-loop.md` | **编号 +4**；回复/信号小节分别 → `01`/`13` |
| `07-rs-period-heartbeat.md` | `13-rs-monitor-recovery.md` | **改名+合并**；`update_period` 小节 → `16` |
| `08-rs-slot-config.md` | `08-rs-slot-image.md` | 改名；`rs_start` ABI 小节 → `07` |
| `09-rs-exec.md` | `08-rs-slot-image.md` | **合并** |
| `10-rs-service-create.md` | `11-rs-service-create.md` | **编号 +1** |
| `11-rs-publish.md` | `12-rs-publish.md` | **编号 +1** |
| `12-rs-init-run.md` | `06-rs-init-handshake.md` | **改名+大改** |
| `13-rs-control-requests.md` | `14-rs-control-requests.md` | **编号 +1** |
| `14-rs-query-requests.md` | `15-rs-query-requests.md` | **编号 +1** |
| `15-rs-terminate-restart.md` | `13-rs-monitor-recovery.md` | **改名+合并** |
| `16-rs-live-update.md` | `16-rs-live-update.md` | 同名（内容吸收旧 17） |
| `17-rs-state-data.md` | `16-rs-live-update.md` | **合并** |
| `18-rs-self-lifecycle.md` | `17-rs-self-lifecycle.md` | **编号 −1** |
| `19-rs-external-interfaces.md` | `18-rs-external-interfaces.md` | **编号 −1** |
| `99-rs-global-concepts.md` | `99-rs-global-concepts.md` | 同名 |

批量替换命令（B 相执行，只改引用不改正文语义；执行后必须重跑验证）：

```bash
# 在仓库根执行；先 dry-run 统计，再正式替换
# 1) 文件名引用（文档 + Rust 注释）
rg -l '01-rs-boot-init\.md'   --glob '!rewrite-notes/03-stage-rs/archive/**' | \
  xargs sed -i 's/01-rs-boot-init\.md/09-rs-boot-init.md/g'
# ……对 §8.2.1 表内 20 条逐一执行（建议写成映射文件循环）
# 2) 目录前缀（若引用带路径）
sed -i 's#03-stage-rs/07-rs-period-heartbeat\.md#03-stage-rs/13-rs-monitor-recovery.md#g'
# 3) 删除隐藏设计目录引用（本次核对 6 处）
#    按仓库约定的隐藏目录名做一次 rg，逐处人工删除；不把路径写进新正文
```

#### 8.2.2 编号引用（无文件名）与热点外部引用

- 形如"见 15""（16）§3"的**裸编号**引用无法机械替换，必须按主题重定向。规则：先在旧文档定位该句所谈主题，再按 §8.1 的逐节表落到新篇。典型歧义：旧 `15` 可能指监控恢复（新 `13`）或终止（同为新 `13`）；旧 `01` 可能指 SEF（新 `01`）、ready（新 `06`）或 boot（新 `09`）；旧 `16` 可能指 LU（新 `16`）或状态数据（同为新 `16`，已合并）。
- 外部文档热点（文件名引用，按文件）：
  - `06-stage-sched/14-rs-interaction.md`（5）、`06-stage-sched/plan.md`（4）、`06-stage-sched/13-pm-interaction.md`（3）、`06-stage-sched/07-stop-scheduling.md`（2）、`06-stage-sched/12-kernel-interface.md`（1）；
  - `14-stage-runtime/11-misc-syscalls.md`（2）、`todo.md`（1）、`plan.md`（1）、`13-constants-abi.md`（1）；
  - `08-stage-is/01-is-init-main.md`（2）；
  - `11-stage-devman/{plan,README,13-devmand-consumer,09-devm-bind-unbind,01-devm-init-main}.md`（各 1）；
  - `07-stage-ds/{plan,06-ds-boot-mapping}.md`（各 1）；
  - `edge_todo.md`（2）、`edge3.md`（1）。
- Rust 代码注释热点（32 个文件）：`os/servers/rs/src/boot.rs`（23）、`sef.rs`（16）、`process_table.rs`（9）、`service_slot.rs`（8）、`publish.rs`（7）、`lib.rs`（7）、`dispatch.rs`（7）、`state_data.rs`（6）、`service_create.rs`（6）、`privilege.rs`（6），其余文件 2–5 处。迁移方式是替换文件名 + 按 §8.1 修正小节号；代码注释同时要修正其中已失效的函数名（如 `should_reply_ready`）。
- 隐藏设计目录引用（违规，删除）：`00-rs-overview.md` ×1、`02-rs-process-table.md` ×3、`03-rs-privilege.md` ×1（另有 01-stage-kernel 的其它 AI 产物 1 处，与本 stage 无关）。
- 验证方式：迁移后执行
  一是对文件名引用做一次全仓 rg，期望全部命中 §8.2.1 的新名集合；二是对仓库约定的隐藏设计目录名做一次 rg，期望除 `doc_rerank_*` 外零命中。

### 8.3 断链成本摘要

| 指标 | 数量 | 说明 |
|------|------|------|
| 旧文档之间的文件名引用 | ≈429 处 | 逐目标计数见 §0.3；全部需要改写 |
| 裸编号引用 | 未机械统计（估计与文件名引用同量级） | 需按主题人工重定向；这是最大的人工成本项 |
| 仓库其它文档的引用 | 49 个文件、41 处文件名引用（含 plan/todo） | 热点集中在 06-stage-sched、14-stage-runtime、08-stage-is、11-stage-devman |
| Rust 代码注释引用 | 32 个文件 | 模块头注释为主；改名后语义已变（如旧 01→09） |
| 违规隐藏设计目录引用 | 6 处 | 直接删除 |
| **合计** | **≈500+ 处** | 其中可机械替换 ≈470 处（文件名），需人工判断 ≈30–60 处（裸编号 + 章节号） |

**风险热点**：① 旧 `01`/`15`/`16`/`17`/`07` 五个编号的目标变化最大，任何只按数字替换的做法都会错；② `06-stage-sched` 与 `11-stage-devman` 的引用多数按主题（RS 的调度接口、devman 绑定），应转到新 `18` 或新 `13`，不能只换文件名；③ Rust 注释里的引用是"文档-代码同步"检查的证据来源，必须与正文同步改。

**建议的批量修改方式**：先应用 §8.2.1 的 20 条文件名映射（可机械替换，含 Rust 注释）；再用 `rg '见 [0-9]|（[0-9]+）|\([0-9]+\)'` 扫裸编号引用逐处人工重定向；最后按 §8.2.2 的验证命令对账，并把新目录写入 `00-rs-overview.md` 的导航表作为引用唯一入口。

---

## 9. 验证与自检门

### 9.1 四种机械检查结果

**检查一：前向引用扫描（逐篇查"前置"字段）**

| 新篇 | 前置（只允许更小编号） | 结果 |
|------|----------------------|------|
| 00 | 无 | 通过 |
| 01 | 00 | 通过 |
| 02 | 00 | 通过 |
| 03 | 02 | 通过 |
| 04 | 02、03 | 通过 |
| 05 | 02、03 | 通过 |
| 06 | 01、02 | 通过 |
| 07 | 02、03、04 | 通过 |
| 08 | 02、03、04、07 | 通过 |
| 09 | 01–08（实际用 02/03/04/06/07/08） | 通过 |
| 10 | 01、02、06、09 | 通过 |
| 11 | 03、04、06、08 | 通过 |
| 12 | 02、11 | 通过 |
| 13 | 01、02、06、10、11、12 | 通过 |
| 14 | 05、06、08、11、13 | 通过 |
| 15 | 05、08、13 | 通过 |
| 16 | 02、06、08、11、13 | 通过 |
| 17 | 06、09、11、13、16 | 通过 |
| 18 | 01–17（签名汇总） | 通过（参考面，无理解依赖） |
| 99 | 无 | 通过 |

结论：**零前向引用**（不含"不讲什么"里的去向提示与 §4.4 序差表标注的机制 deferral；这两类不是理解依赖）。

**检查二：依赖关系图检查**

依赖边全部从小号指向大号（§4.1 图），18/99 无出边 → **无环**。若把"机制 deferral"（如 10 的空闲期调用 11/13、14 的 `do_sysctl` 调用 16）视为软边，需注意它们不构成重新进入更早篇的学习路径。

**检查三：覆盖率检查**

知识点池 266 条（§2.21）全部有去向（按编号分组直接对应新篇）；新增 14 条均有证据锚点（K-01-08/K-01-12/K-07-*/K-16-22/K-16-23/K-18-11/K-18-12）；明确删除项列于 §6 删除条款（测试计数、Fix 块、隐藏设计目录引用、错误锚点）。**覆盖率 100%**。

**检查四：断链成本统计**

见 §8.3：文件名引用 ≈429 处、仓库其它文档 41 处、Rust 32 个文件、裸编号未计量（估计同量级），合计 ≈500+ 处；可机械替换 ≈470 处。

### 9.2 自检门（G1–G9）

| 门 | 检查内容 | 结果与证据 |
|----|---------|-----------|
| G1 | C 真序逐条可核对 | 通过；随机抽 10 条见 §1.2，全部与原文件行一致 |
| G2 | 知识点池完整：每个 C 文件、每个非 C 制品都有归属或明确排除 | 通过；C 文件归属：`main.c`→01/06/09/10/13，`manager.c`→02/03/05/06/08/11/12/13/14，`request.c`→06/13/14/15/16，`update.c`→11/13/16/17，`utility.c`→01/03/04/06/09/10/11/13/15，`exec.c`→08，`error.c`→99，`table.c`→09；头文件：`type/glo/const/proto/inc.h`→02/03/13/16/18/99，`rs.h`→02/05/06/16/99，`sef.h`→01/06/14/16/17/99，`com.h`→03/05/06/14/15/16/99，`ipc_filter.h`→16/99，`kernel/priv.h` 与 `include/minix/priv.h`→03；非 C 制品：`system.conf`/`parse.c`/`minix-service.c`→07，`servers/rs/Makefile` 与 `minix.service.mk`→99（WONTFIX），`Cargo.toml`→99，`bsd.own.mk:1499`→99，`kernel/table.c`/`main.c`→09，`lib/libsys/sef*.c`→01/13，`lib/libexec/*`→08/18，`os/qemu-tests/`→§3.5 第 10 项 |
| G3 | 新目录前向引用为零 | 通过；见 §9.1 检查一 |
| G4 | 依赖图无环 | 通过；边只向大号；18/99 无出边 |
| G5 | 覆盖率 100%，新增有锚点，删除项单列 | 通过；池 266 条全部按编号分组有去向；新增 14 条锚点见 §2；删除项见 §6 |
| G6 | 拆分/合并去向与新建来源抽查 | 通过；抽查十处：①旧 07+15 合并→13（K-13-01…19）；②旧 08+09 合并→08（K-08-01…20）；③旧 16+17 合并→16（K-16-01…25）；④旧 01 拆 01/06/09/17（§8.1 逐节表）；⑤新建 07 来源 G-01/G-02/G-03；⑥信号面 G-09→13；⑦ASR G-06→16 §6；⑧RS↔VM 握手 G-08→18；⑨排除项 G-18→99 §7.1；⑩SEF ping G-05→01 §4 |
| G7 | 每篇契约七要素齐全 | 通过；20 篇契约均含定位/讲什么/不讲什么/前置/后置/事实底线/知识点清单+验收标准；知识点清单以"编号+名称+归属理由"压缩引用 §2 完整行（信息不丢失） |
| G8 | 锚点迁移表覆盖所有变化文档每一节；引用迁移覆盖文档与代码注释 | 通过；§8.1 覆盖旧 00–19、99 全部小节目录（同去向连续小节合并行）；§8.2 含 20 条文件名映射、裸编号规则、外部文档热点、Rust 32 文件、隐藏设计目录引用 6 处 |
| G9 | 事实断言都有锚点；推测项标注 | 通过；另抽 10 条核对：①`RS_PROC_NR=2`（`com.h:61`）；②`ROOT_SYS_PROC_NR=RS_PROC_NR`（`com.h:77`）；③DS 先于 RS（`kernel/table.c:52-53`）；④`RS_DELTA_T=system_hz`（`const.h:49`）；⑤RS 消息 15 个（grep=15）；⑥`r_flags` 16 位（grep=16）；⑦`SF_*` 13 个（grep=13）；⑧`RSS_*` 26 个 define、其中 20 个旗标（grep=26，剔除 6 个容量/字符串常量）；⑨`rs_receive_ticks` 唯一调用点（`update.c:590`）；⑩信号终止集合（`signal.h:280-286`）。**标注为待验证/推测的项**：(a) `struct rs_state_data` 的 x86-64 56 字节为字段算术推得，未编译断言；(b) "C 侧无 RS 专属测试程序"按 `minix3/minix/tests/` 目录核对，未做全树按名穷举；(c) 旧目录测试计数的"过期"判断基于抽样（01/04/12/16/17），未逐篇复核 |

### 9.3 结论与待用户裁决的问题

**结论**：本蓝图给出的新目录（20 篇）满足四条硬标准与 G1–G9 全部门；旧目录的实质知识点 266 条全部有去向，新增 14 条全部有锚点；重建的主要代价（引用迁移 ≈500 处）已列成可执行的映射。蓝图达到"B 相拿到即可按篇施工"的程度。

**待用户裁决的问题**：

1. **编号大改是否接受**：本蓝图将 04/05 互换、01 的语义从 boot 改为 SEF 运行框架、15 从终止改为 LU（与 16 合并）、18/19 各前移一号。若要求最小化编号变动，替代方案是保留旧编号但接受"boot 篇前向引用权限/掩码"或"机制重复"之一——两者都与四条硬标准冲突。建议采纳本蓝图方案。
2. **`system.conf` 是否独立成篇**：本蓝图独立为 07（约 300–400 行）。若倾向更少篇数，可并入 08 或 14，但会把"客户端声明面"与"服务端落地"混在一篇。
3. **三组合并的篇幅**：新 08（≈700–800 行）、13（≈650–750 行）、16（≈700–850 行）均超旧单篇均值但仍在软限制内（概念单一）。若要拆，拆点已注明，但会重新引入前向引用。
4. **打印族的下放时机**：`srv_to_string*`/`print_*` 按 `todo.md` R31 最终应去 08-stage-is；本蓝图暂留 15 并注记，是否需要现在就划走。
5. **过程叙述删除政策**：测试计数、Fix #块、接线落地块一律不进新正文（§6 删除条款）；若 B 相希望保留"实现状态快照"，建议集中到 `todo.md` 而不是正文。
6. **旧文档归档目录名**：建议 `03-stage-rs/archive/`（B 相执行）；若已有约定（如 `archive_bak/`）按仓库现状调整。
7. **G9 三条待验证项**：`rs_state_data` 56B 建议由 B 相补一条编译期尺寸断言；C 测试穷举与旧文档计数复核可按需追加。


