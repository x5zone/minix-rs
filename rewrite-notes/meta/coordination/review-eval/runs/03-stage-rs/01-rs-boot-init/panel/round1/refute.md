# P-REFUTE — 03-stage-rs/01-rs-boot-init.md（round 1）

> **角色**：反证（只读）。审查对象：`/home/xzhao/github/minix-rs/rewrite-notes/03-stage-rs/01-rs-boot-init.md`。
> 对照真值：C `minix3/minix/servers/rs/{main.c,table.c,utility.c,request.c}`、`minix3/minix/lib/libsys/{sef.c,sef_init.c,sched_start.c}`、`minix3/minix/include/minix/priv.h`；Rust `os/servers/rs/src/{main,lib,table,boot,sef,privilege,process_table,service_create,ready,monitor}.rs`。
> 方法：对五条核心主张各构造最强反方论据；只有经 C 与 Rust 双向取证成立的反证，才写成 `FINDING`。行号以当前工作树为准。

## 结论速览

| 主张 | 判定 | 编号 |
|---|---|---|
| ① Step 1「priv 提交但不放行」与 C 一致 | C 侧描述成立；Rust 对 INIT 条目的 priv 字段不符 | REFUTE-01（P1） |
| ② RS 的 `do_sef_rs_init` 入口与 C 一致 | 不成立 | REFUTE-02（P0） |
| ③ 四步 boot 与 C 顺序一致 | 四步次序成立；Step 2 对 INIT 的调度动作不符 | REFUTE-03（P1） |
| ④ 三张 boot 表与内核 `boot_image[]` 对齐 | 未找到成立的反证 | 无 |
| ⑤ `BootTables`/`BootInit` 边界不改变可观察行为 | 不成立（feature 尾步无生产调用点） | REFUTE-04（P1） |

---

## FINDING REFUTE-01（P1）主张①：Step 1 的 priv 提交对 INIT 条目与 C 不一致

### 主张原文

> 「Step 1（`main.c:244-346`，四步注释在 main.c:239-243）——建立全表属性：……此时它把非 RS/VM 服务的 priv 结构提交内核（`SYS_PRIV_SET_SYS` + `sys_getpriv`），RS/VM 跳过 `SET_SYS`；sys/dev/命令/调度属性在本地准备。**Step 1 不放行**。」（文档 §1.1；§2.3.3、§2.6 同义）

### 反方最强论据

C 侧「提交但不放行」这句本身经得起核对：`SYS_PRIV_SET_SYS` 只要求目标处于 `RTS_NO_PRIV`，不解除该标志；解除只由 `SYS_PRIV_ALLOW`/`SYS_PRIV_YIELD` 完成（`/home/xzhao/github/minix-rs/minix3/minix/kernel/system/do_privctl.c:56-64`、`66-73`、`86-88`）。真正站不住的是"Rust 的 Step 1 与 C 一致"这一层：Rust 的 `Privilege::boot_priv` 把「boot 表条目都是系统服务」当作不变量，而 12 项中最后一项 INIT 是 `USR_F`（`table.c:28`）。C 的 `SRV_OR_USR(rp, SRV_KC, USR_KC)` 对 INIT 取 `USR_KC = NO_C`（`priv.h:73-75`），`fill_call_mask` 得到空内核调用掩码；Rust 无条件走 `ALL_C` 分支，给 INIT 提交**全部内核调用权限**。这是权限面的实质性放大，不是字段命名差异。内核的内核调用门 `kernel_call_dispatch`（`/home/xzhao/github/minix-rs/minix3/minix/kernel/system.c:96-124`）只查 `priv(caller)->s_k_call_mask` 的对应位（`system.c:111`），INIT 获得全掩码后可以发起本不该允许的内核调用。

同时，`activate_boot_slot` 把 `SchedulerConfig::boot_defaults`（`scheduler = KERNEL`）写进每个 boot 槽，INIT 槽因此保存 `KERNEL`；C Step 1 对 INIT 写入 `USR_SCH = NONE`（`priv.h:88-100`，`main.c:320-322`）。C 的 `sched_init_proc` 自己用 `assert(rp->r_scheduler == NONE)` 钉死了这个不变量（`utility.c:369-372`），Rust 的槽值若交回 C 逻辑会直接断言失败。

### 代码证据

- C：`/home/xzhao/github/minix-rs/minix3/minix/servers/rs/table.c:28`（INIT 为 `USR_F`）；`/home/xzhao/github/minix-rs/minix3/minix/include/minix/priv.h:45-49`（`USR_F = BILLABLE|PREEMPTIBLE`）、`71-76`（`USR_KC = NO_C`）、`88-100`（`USR_SCH = NONE`）；`/home/xzhao/github/minix-rs/minix3/minix/servers/rs/main.c:264-280`（`calls = SRV_OR_USR(rp, SRV_KC, USR_KC) == ALL_C ? all_c : no_c`；`fill_call_mask(..., TRUE)`）、`320-322`（`r_scheduler = SRV_OR_USR(rp, SRV_SCH, USR_SCH)`）。
- Rust：`/home/xzhao/github/minix-rs/os/servers/rs/src/privilege.rs:425-451`，`boot_priv` 的注释写「boot services use the SRV_KC=ALL_C branch」，随后固定构造 `CallMask::from_calls(..., &[ALL_C, NULL_C], ...)`；`/home/xzhao/github/minix-rs/os/servers/rs/src/process_table.rs:559-566` 用 `boot_defaults(endpoint)` 覆盖 `scheduler/priority/quantum`，未按 `SRV_OR_USR` 区分 `USR_F`。
- 测试盲区：`test_init_fresh_step_order` 只数 `SchedInitProc` 次数（10 次，INIT 与系统服务混在一起断言），`test_boot_slot_populates_s2_fields` 只构造 RS 一项，因而都没碰到 INIT 的掩码/调度字段（`/home/xzhao/github/minix-rs/os/servers/rs/src/boot.rs:1467-1519`、`2899-2939`）。

### 若要推翻反方需要什么

需要证明下列任一：C 的 `SRV_OR_USR(rp, SRV_KC, USR_KC)` 对 INIT 实际取 `SRV_KC`（不成立，`USR_F` 不含 `SYS_PROC`）；或 Rust 的 `boot_priv`/`activate_boot_slot` 在别处按 `is_sys_proc` 修正了掩码和 scheduler（不存在，两处都是无条件 SRV 分支）；或文档明确把主张①收窄为「只描述 C 的调用序列，不承诺 Rust 字段值」，并给出 03 修正后的字段对照（当前 03 的 `boot_priv` 与 01 引用的代码是同一份，问题没有被任何文档接管）。

---

## FINDING REFUTE-02（P0）主张②：`do_sef_rs_init` 入口不只调用回调，Rust 缺 process_init 的本地 init-ready 模拟

### 主张原文

> 「RS 是 `ROOT_SYS_PROC`，`sef_startup()` 走 `do_sef_rs_init()` 本地合成 fresh 信息并调用 `sef_cb_init_fresh()`，不等待自己发来的 `RS_INIT`；其他服务才走接收 `RS_INIT` 的路径。」（文档 §1.2；§3.3「`RsServer::init(init_type)` 对应 `sef_startup()` 的分派」、§4.5 同义）

### 反方最强论据

C 的 `do_sef_rs_init` 并不是「构造 fresh 信息 + 调回调」就结束：它调 `process_init`，后者在回调返回后**无条件**用本进程源地址合成一条 `RS_INIT`（`m.m_source = sef_self_endpoint`，`m_rs_init.result = 回调结果`），再调用 RS 自己注册的 `sef_cb_init_response`（`/home/xzhao/github/minix-rs/minix3/minix/lib/libsys/sef_init.c:113-120`）。RS 的该回调实现是 `do_init_ready`（`main.c:591-609`），而 `do_init_ready` 的成功路径会清掉本槽的 `RS_INITIALIZING`、把 `r_check_tm` 归零、刷新 `r_alive_tm`，并调用 `end_srv_init`（`request.c:502-529`）。这条清理链是**承重的**：Step 2 的 `init_service` 先给 RS 置 `RS_INITIALIZING` 后因 `ROOT_SYS_PROC` 提前返回（`utility.c:24-31`）；如果后面的本地 ready 没有发生，`do_init_ready` 的前置检查会返回 `EINVAL`，`process_init` 一路把错误传回 `do_sef_rs_init`，`sef_startup` 直接 panic「RS unable to complete init」（`sef.c:109-113`）。也就是说，C 的 fresh 启动后置条件是「RS 槽不再是 INITIALIZING」；Rust 的 fresh 启动没有这条后置条件。

Rust 侧：`main.rs` 固定 `server.init(SefInitType::Fresh)`（`/home/xzhao/github/minix-rs/os/servers/rs/src/main.rs:97`），没有 `sys_whoami`、没有 `LU_SYS_PROC`/`RST_SYS_PROC` 的类型选择，也没有 `process_init` 的等价步骤；`impl SefCallbacks::init_fresh` 只做 `BootInit::init_fresh` 与 `into_state`（`/home/xzhao/github/minix-rs/os/servers/rs/src/lib.rs:616-627`），`BootInit::init_fresh` 只调用 step0 到 step4（`boot.rs:942-949`）。`init_service` 给 RS 置 INITIALIZING 后早退（`service_create.rs:1729-1740`），此后没有任何代码为 RS 调 `do_init_ready`/`init_response`；测试甚至把「RS 在 step2 之后仍是 INITIALIZING」写成了断言（`boot.rs:2285-2290`）。

可观察后果落在主循环：`do_period` 把 `another_initializing` 算成「表里任一槽带 INITIALIZING」（`lib.rs:469`，`process_table.rs:349-357` 只要求 `IN_USE`），RS 槽永久带该标志，于是 `period_decision` 对任何超时未心跳的服务都走 `FreePass` 而不是 `PingTimeoutCrash`（`monitor.rs:193-205`）。C 的稳态里 RS 槽已被清掉，`another_initializing` 为假，死服务会被 crash/restart；Rust 的稳态把这个看门狗永久关掉。

### 代码证据

- C：`sef_init.c:113-120`（合成 `RS_INIT` 并调 response 回调）、`150-188`（`do_sef_rs_init`：类型选择 158-165、`vm_memctl` 172-182、`process_init` 184-185）；`main.c:591-609`（RS 的 response 回调 → `do_init_ready`，`EDONTREPLY` 归一为 OK）；`request.c:462-529`（前置 `RS_INITIALIZING` 检查、清理与 `end_srv_init`）；`utility.c:18-31`（`init_service` 先置 `RS_INITIALIZING`，ROOT 早退）。
- Rust：`main.rs:46-111`（入口只有 `acquire_from`/`init(Fresh)`，无 whoami/类型选择）；`lib.rs:277-284`、`613-627`（init 分派与 fresh 收尾，无 `init_response` 调用）；`boot.rs:1055-1148`（step2 对 RS 调 `init_service`）、`1155-1165`（step3 只收外部 ready）；`ready.rs:202-262`（`do_init_ready` 的清理决策存在，但没有 boot 期的 RS 自调用者）；`monitor.rs:193-205` + `lib.rs:469`（free pass 的长期化）。
- 测试现状：`boot.rs:2252-2380` 断言 RS 在 step2 后 `INITIALIZING`、且 RS 零发送；没有任何 fresh boot 后「RS 槽已清 INITIALIZING」的断言。

### 若要推翻反方需要什么

需要证明以下任一：C 的 `process_init` 在 `ROOT_SYS_PROC` 上不调用 `sef_cb_init_response`（不成立，`sef_init.c:113-120` 无分支）；或 C 的 `do_init_ready` 对 RS 自消息不清标志（不成立，`request.c:514-529`）；或 Rust 在 boot 之后另有路径清 RS 槽（不存在；`run()` 进入循环前没有任何 `do_init_ready` 调用）；或 `another_initializing` 不包含 RS 槽（`lookup_by_flags` 只查 `IN_USE`，RS 槽满足）。要修掉反证，需要把 12 的 `do_init_ready`（或 `init_response`）接到 fresh boot 的收尾，并在测试里钉住 RS 槽的 boot 后状态。

---

## FINDING REFUTE-03（P1）主张③：四步次序成立，但 Step 2 对 INIT 的调度动作不是 C 的动作

### 主张原文

> 「四步顺序是：priv 提交与本地属性 → 放行 → 收齐 ready → pid+alarm；Step 1 不放行。」（文档 §2.6；§1.1 的四步清单同义）

### 反方最强论据

四步的**次序**逐条对得上（Step 1 建槽并提交 priv → Step 2 对非 RS/VM 服务 `sched_init_proc` + `ALLOW` + 条件 `init_service` → Step 3 收剩余 ready → Step 4 `getnpid` + `setalarm`）。这一点反方无法推翻。但同一段落的实际动作对 INIT 不一致：C 的 `sched_init_proc(rp)` 读取槽里的 `r_scheduler = USR_SCH = NONE`，`sched_start(NONE, ...)` 在发任何消息前就 `return OK`（`/home/xzhao/github/minix-rs/minix3/minix/lib/libsys/sched_start.c:57-60`），所以 INIT 的 Step 2 没有任何调度器调用。Rust 的 Step 2 不读槽值，直接合成 `SchedulerConfig::boot_defaults(priv_.endpoint)`；对 INIT 该配置的 `scheduler = KERNEL`，`TrapKernelApi::sched_init_proc` 于是走 `sys_schedctl` 真发内核调用（`trap_api.rs:339-362`）。C 的「NONE 短路」在 Rust 的 boot 路径里被完全绕过：文档 §3.5 声称 shell 会执行 `sched_decision` 的 `Skip` 分支，但 `sched_decision` 在 boot Step 2 没有调用点（`boot.rs:1108` 直接调 `sched_init_proc`；`sched.rs:119` 只被测试使用）。

后果不止多一次系统调用：INIT 的调度器登记值从 `NONE` 变成 `KERNEL`，而这正是 C 用 `assert(rp->r_scheduler == NONE)` 钉死的用户进程不变量（`utility.c:369-372`）。内核 `do_schedctl` 会为该端点执行 `sched_proc`（`do_schedctl.c:28-39`），把 INIT 纳入内核调度参数设置，而 C 明确把用户进程留给 PM（`priv.h:88-90` 注释「user processes」）。

### 代码证据

- C：`main.c:375-381`（非 RS/VM 的 `sched_init_proc` + `ALLOW`）；`utility.c:364-382`（读 `rp->r_scheduler`、用户进程断言 NONE）；`priv.h:88-100`（`USR_SCH = NONE`）；`sched_start.c:57-60`（NONE 直接返回）。
- Rust：`boot.rs:1107-1109`（`sys.sched_init_proc(&SchedulerConfig::boot_defaults(priv_.endpoint))?;`）；`process_table.rs:559-566`（槽里同样写 KERNEL）；`sched.rs:81-90`（`boot_defaults` 无条件 KERNEL）；`trap_api.rs:339-351`（KERNEL → `sys_schedctl`）；`sched.rs:93-119`（`sched_decision` 的 `Skip` 语义存在，但 boot 不调用）。
- 测试：`boot.rs:1507-1519` 只断言 `SchedInitProc` 计数为 10，不检查第 10 项（INIT）的配置；`test_boot_slot_populates_s2_fields` 只覆盖 RS。

### 若要推翻反方需要什么

需要证明 C 对 INIT 的 `sched_init_proc` 实际传入 KERNEL（不成立，`SRV_OR_USR` 取 `USR_SCH = NONE`，`sched_start` 在 NONE 处短路）；或 Rust Step 2 在调用 `sched_init_proc` 前经过 `sched_decision`/读取槽的 `scheduler`（不存在，`boot.rs:1108` 是直接调用，`boot_defaults` 无条件 KERNEL）；或证明 INIT 不在 priv 表的非 RS/VM 集合里（它在，`table.c:28`，step2 的 `is_kernel_task` 过滤不排除它）。

---

## 主张④：三张 boot 表与内核 `boot_image[]` 对齐（反证未成立）

### 主张原文

> 「**表语义**：priv 表是 boot 主表（顺序是 RS 的准备/放行遍历顺序，实际运行序由内核可调度序决定，条目数必须与内核 boot_image 表系统服务数一致，§2.3.2）；sys 表只列**覆盖默认 sys 属性**的服务……」以及「`table.rs` 用静态切片复刻 table.c 的内容」（文档 §2.5、§3.1、§4.3）。

### 反方最强论据（逐条攻击后均被驳回）

1. **顺序不同**：内核 `table.c:44-65` 把 DS 放在 RS 之前，RS priv 表把 RS 放第一。反方本想用「顺序不一致」推翻「对齐」；但文档 §2.5 明确区分「登记序/模块摆放序」与「RS 的准备/放行遍历顺序」，真正被校验的是服务集合与计数，而不是两表同序。此攻击不成立。
2. **MFS 名称不同**：内核 image 名是 `"mfs"`（`table.c:63`），RS priv 表 label 是 `"fs_imgrd"`（`table.c:27`）。但 C 本来就把两者放在不同字段（`rpub->label` ← `boot_image_priv->label`，`rpub->proc_name` ← `ip->proc_name`，`main.c:262`、`313`）；Rust 也分开保存（`process_table.rs:535-536`）。不是错位。
3. **默认条目的 endpoint**：Rust 的 `DEFAULT_SYS`/`DEFAULT_DEV` 用 `Endpoint::NONE`（`table.rs:155-158`、`189-192`），C 的默认条目用 `DEFAULT_BOOT_NR`（`table.c:41`、`49`）。反方试图把它算成对齐失败；但两个查找函数的语义是「未命中返回默认条目的**内容**」（`boot.rs:839-854`），从不比较条目的 `endpoint`，C 的 `boot_image_info_lookup` 同样只取内容（`main.c:753-777`）。当前没有可观察差异。
4. **Rust 多出的 `EndpointMismatch` 检查**：`validate_tables` 要求每行 `endpoint.slot() == proc_nr`（`boot.rs:629-632`），C 没有该检查。反方试图论证真实内核 image 会被误拒；核对后不成立：内核 `proc_init` 用 `_ENDPOINT(0, p_nr)` 设端点（`proc.c:129-133`），`kmain` 把 `rp->p_endpoint` 抄进 image（`kernel/main.c:174`），`_ENDPOINT_P` 与 Rust `Endpoint::slot()` 公式一致（`include/minix/endpoint.h:65-69`、`endpoint.rs:88-90`），五个内核任务（端点 -5..-1）也满足等式，因此真实 GET_IMAGE 不会被误拒；这是更严的 fail-closed 检查。
5. **常量值**：`RSYS_F=0x112`、`VM_F=0x210`、`SRV_F=0x012`、`USR_F=0x006` 与 `priv.h:45-49` 一致（`table.rs:241-263`）；`SRVR_SF`/`VM_SF`/`SRV_SF` 与 `const.h:65-68`、`rs.h:191-195` 一致（`service_slot.rs:117-153`）；`TTY_MAJOR=4`、`MEMORY_MAJOR=1`（`dmap.h:22-25`）与 `table.rs:195-204` 一致；12 项服务集合与内核 image 的 12 个非内核任务一致（`kernel/table.c:52-64`）。

### 代码证据

见上述各条内的文件行号；核心是 `/home/xzhao/github/minix-rs/minix3/minix/kernel/table.c:44-65`、`/home/xzhao/github/minix-rs/minix3/minix/servers/rs/table.c:15-50`、`/home/xzhao/github/minix-rs/os/servers/rs/src/table.rs:91-204`、`/home/xzhao/github/minix-rs/os/servers/rs/src/boot.rs:629-647`。

### 若要推翻反方需要什么

要立起这条反证，需要拿出至少一项：priv 表与内核 image 的非内核任务集合或计数不等；某个 `SF_*`、priv flag、主设备号或 `*_PROC_NR` 的数值不等；或真实 GET_IMAGE 中存在 `endpoint.slot() != proc_nr` 的行（这会让生产 boot 被 `validate_tables` 误拒）。逐项核对未发现，故本条不写 FINDING。

---

## FINDING REFUTE-04（P1）主张⑤：`self_update` 没有生产调用点，feature 打开也复刻不了 C 的 boot 尾步

### 主张原文

> 「BootTables 把 `sys_getimage` 的拷贝与三张静态表放在一个显式输入里」「BootInit 状态机保证 Step 1→4 的顺序」（文档 §3.8）；以及「feature 打开时 `init_fresh` 末尾调用 `self_update`；feature 默认关，因此默认 Rust boot 比 C 默认 yes 少这一尾步，这是当前默认构建的行为差异」（文档 §3.6；§4.4「`self_update`（`#[cfg(feature = "live-update")]`，`init_fresh` 末尾调用）」）。

### 反方最强论据

文档把可观察差异限定为「默认 feature 关」这一条，并承诺打开 `live-update` 后 `init_fresh` 末尾会调用 `self_update`，从而复刻 C 默认的 USE_LIVEUPDATE 尾步。代码不支持这个承诺：`BootInit::init_fresh` 的函数体只有五个 step 调用后 `Ok(())`（`/home/xzhao/github/minix-rs/os/servers/rs/src/boot.rs:942-949`），`self_update` 虽是 `#[cfg(feature = "live-update")]` 的公开方法（`boot.rs:1263-1332`），但全 crate 的调用点只有两处测试（`boot.rs:1968`、`2025`）。`RsServer::init`/`SefCallbacks::init_fresh` 也没有在 feature 下追加调用（`lib.rs:277-284`、`616-627`）。因此即便 `--features live-update`，生产 boot 仍不做 `clone_slot`/`srv_fork`/`update_service(RS_SWAP)`/`cpf_reload`/`cleanup_service`/`vm_memctl(PIN)`，也不做旧实例的 `SetSys`/`sched_init_proc`/`YIELD`；C 默认构建的 boot 后置状态（RS 自升级出一个副本并切换端点）在 Rust 的任何 feature 组合下都不会出现。文档「默认关才少这一尾步」的归因因此不成立：尾步在两种配置下都缺。

这直接推翻「Rust 边界不改变可观察行为」的汇总主张：`BootTables` 的显式化本身可以做到行为中性，但 `BootInit` 承载的 boot 编排在 C 默认路径上少了一整段，且文档承诺的开关并不存在。文档 §3.4 另行承认 `machine` 快照从 C 的 `main()`（`main.c:53`，四步之后）提前到 `step0`（`boot.rs:954-958`）是「时序变化」；这一条至少被披露，可作为「边界并非行为中性」的旁证，但不是本反证的主证据。

### 代码证据

- C：`main.c:436-491`（`USE_LIVEUPDATE` 尾步：`clone_slot`/`srv_fork`/`getprocnr`/`update_service(RS_SWAP)`/`cpf_reload`/`cleanup_service`/`vm_memctl(PIN)`，旧实例 `SetSys`+`sched_init_proc`+`YIELD`）；`minix3/share/mk/bsd.own.mk:1499`（`USE_LIVEUPDATE` 在默认 yes 的选项列表里）。
- Rust：`boot.rs:937-949`（`init_fresh` 只有 step0..step4）；`boot.rs:1263-1332`（`self_update` 定义）；`grep -rn 'self_update(' os/servers/rs/src/` 只命中定义与两处测试（`boot.rs:1968`、`2025`）；`Cargo.toml:28`（`live-update = []`）；`lib.rs:616-627`（fresh 收尾不按 feature 追加调用）。
- 文档自身：`01-rs-boot-init.md:824` 与 `:963` 是上述承诺的原文。

### 若要推翻反方需要什么

需要指出 `BootInit::self_update` 的生产调用点（当前不存在；测试调用不算），或证明另有路径在 feature 打开时执行了等价尾步（`lib.rs`/`main.rs`/`shell_update.rs` 均无 `BootInit::self_update` 调用），或把文档 §3.6/§4.4 的承诺改成「未接线」并说明默认与非默认配置都与 C 不同（那仍然推翻「不改变可观察行为」的汇总主张，只是把缺陷降为已披露的欠账）。

---

## SUMMARY

- 成立的反证 4 条：REFUTE-01（P1，主张①，INIT 的 priv 内核调用掩码与调度字段）、REFUTE-02（P0，主张②，缺 `process_init` 的 RS 自 init-ready 模拟，RS 槽永久 `INITIALIZING` 并让主循环的 free-pass 永久化）、REFUTE-03（P1，主张③，四步次序成立但 Step 2 对 INIT 发了 C 不会发的 `sys_schedctl`）、REFUTE-04（P1，主张⑤，`self_update` 无生产调用点，feature 打开也无法复刻 C 的 boot 尾步）。
- 未成立的反证 1 条：主张④的三张表与内核 `boot_image[]` 对齐经集合、顺序语义、名称字段、默认条目、常量值与端点派生逐项攻击后仍站得住，不写 FINDING。
- 两条最重的后果都落在 boot 的后置状态而非四步的写法：REFUTE-02 让 C 与 Rust 的「boot 完成」定义不同（RS 槽是否仍 `INITIALIZING`），REFUTE-01/03 让 INIT 这一个 `USR_F` 条目在两处做了系统服务才该做的事。文档的 Step 1–4 骨架描述（C 侧）基本准确；失效的是「Rust 入口/边界与 C 等价」这层汇总断言。
