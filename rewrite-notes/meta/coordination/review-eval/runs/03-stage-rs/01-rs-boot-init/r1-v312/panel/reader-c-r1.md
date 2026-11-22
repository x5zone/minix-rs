# READER-C R1
DOC 01-rs-boot-init.md

审查范围：只读本文档，追四步 boot 的依赖理由、SEF fresh/restart/LU 边界、Step 2 放行与 Step 3 ready 的先后、三张 boot 表与内核 `boot_image` 的对齐、Rust `BootInit` 与 C 顺序的对应、359/0 基线与测试描述。以下 `Lnn` 均为本文档行号。严重度口径：P0 = 文档内可证伪的事实错误或自相矛盾且直接改变行为叙述；P1 = 因果链缺步、边界混淆或关键断言证据不足；P2 = 措辞、局部一致性或证据强度问题。

---

## FINDING C-01

- 位置: L43、L67、L597，与 L359、L316 对读
- 严重度: P0
- 问题: 文档反复说 Step 1 是"纯本地准备""没有向 kernel 提交任何东西""Step 1 不向内核提交"，但同一文档的 C 摘录在 Step 1 循环里明确调用 `sys_privctl(ip->endpoint, SYS_PRIV_SET_SYS, &(rp->r_priv))`，并在 L359 说"其余服务则把 priv 结构提交给 kernel"。括号"(除 RS/VM 例外)"还把例外方向写反了：C 的例外是 RS/VM **跳过** SET_SYS，不是它们被提交。
- 影响: 四步依赖理由的核心前提被写错。读者会以为 Step 1 无内核副作用、Step 2 的 `SYS_PRIV_ALLOW` 单独建立权限；实际顺序理由是非 RS/VM 服务必须先 `SET_SYS` 再 `ALLOW`。这正好是任务要求追的"四步 boot 的依赖理由"。
- 证据: L43 "此时**没有**向 kernel 提交任何东西（除 RS/VM 例外），纯本地准备"；L597 "Step 1 不向内核提交"；L316 Step 1 摘录中的 `sys_privctl(... SYS_PRIV_SET_SYS ...)`；L359 "其余服务则把 priv 结构提交给 kernel"。
- 最小修复: 改成"Step 1 建立本地属性，并把非 RS/VM 服务的 priv 结构提交给内核（SET_SYS），随后 `sys_getpriv` 同步回本地；RS/VM 例外，跳过 SET_SYS"；删除"纯本地准备"和"Step 1 不向内核提交"。
- 置信度: 0.95
- 若我错了: 若作者把"Step 1"狭义定义为只填 `rproc` 字段、把 SET_SYS 算作另一个未编号子步骤，那么这是定义问题；但原文用了"任何东西""不向内核提交"的全称措辞，无法覆盖 SET_SYS。

---

## FINDING C-02

- 位置: L1041、L1053、L1112、L1121；L5.2/L5.3 测试表
- 严重度: P0
- 问题: 同一章给出两个基线：章首 L1041 是 **208 passed / 0 failed**，最小示例 L1053、§5.4 L1112、§5.5 L1121 都是 **359 passed / 0 failed**。范围计数也自相矛盾：L1041 说 01 范围 29 项（boot 18 + table 3 + sef 1 + dispatch 7），L1112 说 41 项（boot.rs 29 + table 3 + sef 1 + dispatch 8）。按 L5.2 表可数出 26 个 boot 测试名（24 行，其中两行各含两个），既不是 18 也不是 29；L5.3 可数出 8 个 dispatch 测试，不是 7。
- 影响: 测试基线是本章全部覆盖结论的证据底座。读者无法判断哪个数字是当前工作树读数；"测试名对账"和测试计数门无法复核；208 这一行还留着空的"（ 实测，全 crate）"，说明是未清理的旧稿残留。
- 证据: L1041 vs L1053/L1112/L1121；L1041 的 29 项 vs L1112 的 41 项；L1041 的 boot 18 vs L1112 的 boot 29 vs L5.2 表内 26 个测试名；L1041 的 dispatch 7 vs L5.3 的 8 个测试。
- 最小修复: 在当前工作树上重跑 `cargo test -p minix-rs --lib`，只保留一个有日期的一个基线；把 §5.1–§5.3 的测试表补成与所宣称的每模块计数逐名对应，删掉 208 句和空白的实测括注。
- 置信度: 0.98
- 若我错了: 若 208 指另一个 feature 组合或另一个 crate，359 指 `minix-rs --lib`，那么必须在文中写明两个命令的差异；现在两者被标成同一条命令，仍属自相矛盾。

---

## FINDING C-03

- 位置: L552、L590，与 L33、L556–L567 对读
- 严重度: P1
- 问题: `boot_image_priv_table` 被称为"顺序即 boot 启动顺序""priv 表是 boot 主表（顺序 = 启动顺序）"。但 L33 刚论证"数组序 ≠ 谁先启动运行"：内核 `boot_image` 里 DS 第一、RS 第二，真正第一个能运行的用户服务是 VM，RS 紧随。priv 表顺序却是 RS、VM、PM、SCHED、VFS、DS……RS 第一、DS 第六，按 L33 的模型不可能是实际启动顺序。它最多是 RS 准备/放行条目的遍历顺序，或与 `proc_nr` 对齐的槽位顺序。
- 影响: 全文的"顺序"叙事出现两种互斥含义：登记序/遍历序 vs 执行序。读者会把 priv 表的 RS→VM→PM→…当作 boot 运行序，和 L33 的 VM-first 结论冲突；也削弱了"表与内核 boot_image 对齐"的论证。
- 证据: L33 "它跟'谁先启动运行'是两码事……第一个能运行的用户服务是 VM，RS 紧随"；L552 "顺序即 boot 启动顺序"；L590 "顺序 = 启动顺序"；L556–L567 表序 RS、VM、PM、SCHED、VFS、DS……。
- 最小修复: 把该性质改名为"RS 的准备/放行遍历顺序"或"priv 表槽位顺序（与 proc_nr 对齐）"，并明确"实际运行序由内核可调度序决定"；若作者本意是 RS 的 ALLOW 顺序，就直说 ALLOW 顺序，并解释 DS 为何在第六。
- 置信度: 0.9
- 若我错了: 若这一版 Minix 的 priv 表顺序同时就是内核启动序，而 L33 的 DS-first 说的是另一张表（内核 `boot_image` vs RS priv 表），两种顺序可以并存；但文档把两者都叫"boot 顺序"且未区分，仍会误导读者的因果理解。

---

## FINDING C-04

- 位置: L192，与 L220、L172–L173 对读
- 严重度: P1
- 问题: L192 说 `sef_startup()` "通过 IPC 接收 `RS_INIT`/`RS_LU_PREPARE` 并分派到 fresh/restart/LU 回调"。但 L220 说 `sef_startup()` 接收的是启动期 `SEF_INIT` 消息（`m_type == RS_INIT`），按 `info->init_type` 分派；L172–L173 显示 `RS_LU_PREPARE` 是主循环的消息，走 `do_upd_ready`。`RS_LU_PREPARE` 是运行期握手，不是启动 init 消息，混进 `sef_startup()` 会打乱 fresh/restart/LU 的边界。
- 影响: 读者无法判断 LU prepare 握手归 SEF 启动、RS 主循环还是 12/16；"SEF 只负责 init 分派"的边界被破坏，后续 18/16 的机制归属也会跟着含混。
- 证据: L192 "接收 `RS_INIT`/`RS_LU_PREPARE` 并分派到 `sef_cb_init_fresh`……或 `sef_cb_init_restart`/`sef_cb_init_lu`"；L220 "启动期接收 `SEF_INIT` 消息（`m_type == RS_INIT`）"；L172–L173 主循环 `RS_INIT → do_init_ready`、`RS_LU_PREPARE → do_upd_ready`。
- 最小修复: 改为"`sef_startup()` 接收携带 `sef_init_info_t` 的启动 `RS_INIT`，按 `init_type` 分派到 fresh/LU/restart；`RS_LU_PREPARE` 是之后主循环的消息，由 `do_upd_ready` 处理（12/16），不经过 `sef_startup()` 的 init 分派"。
- 置信度: 0.85
- 若我错了: 若 SEF 的 `sef_startup()` 内部还跑一个消息泵、会先截获 `RS_LU_PREPARE`，那么 L192 可能成立；但 L220 和主循环代码都给出相反图景，且文档没有给 SEF 源码证据。

---

## FINDING C-05

- 位置: L917–L940，与 L1007–L1010、L697–L699 对读
- 严重度: P1
- 问题: Rust `main()` 直接 `server.init(SefInitType::Fresh)`，没有接收任何 `RS_INIT`；§4.5 却说 `RsServer::init(init_type)` 对应 `sef_startup()`，"启动期从 IPC 接收 `RS_INIT` 后按 `info.init_type` 调"。C 的 `sef_startup()` 是阻塞接收第一条 init 消息并读出 `init_type`；Rust 入口把类型硬编码为 Fresh，跳过了"接收消息 → 读 payload → 分派"的因果链。
- 影响: restart/LU 的 init 消息在描述中没有进入 `RsServer::init` 的路径；初始 `sef_init_info_t` 的 payload 也被忽略。读者会以为 fresh/restart/LU 分派已经接线，实际入口只走 Fresh。
- 证据: L934 `server.init(SefInitType::Fresh)`；L1009 "启动期从 IPC 接收 `RS_INIT` 后按 `info.init_type` 调"；L697–L699 "`RsServer::init(init_type)` 对应 `sef_startup()` 的分派"。
- 最小修复: 二选一写清：要么说明生产入口在 12 接收初始 `RS_INIT` 后调用 `init(info.init_type)`；要么明确"当前入口硬编码 Fresh，接收路径 DEFERRED 到 12"，并删掉"启动期从 IPC 接收"的现在时表述。
- 置信度: 0.85
- 若我错了: 若 `RsServer::new`/`init` 内部实际执行了未展示的 `sef_receive_status`，硬编码参数只是回退值；但 L917–L940 的代码和注释都没有这个接收动作。

---

## FINDING C-06

- 位置: L845、L849，与 L1092 对读
- 严重度: P1
- 问题: `RinitState { rproctab_gid: Option<u32> }` 与同句的 `GrantId = i32` 类型不一致；`Option<u32>` 不能表示 `-1`，而 `test_step2_init_service_marks_and_sends` 断言 RS_INIT 消息携带 `gid=-1`。另外，`Option<GrantId>` 只能表达"创建/未创建"，不能表达 C `GRANT_VALID()` 的"句柄在合法范围"语义；若创建调用返回 `Some(-1)`，照样会把无效 gid 发给服务。
- 影响: grant 的创建点（step0）→ 消费点（12 的 `init_service` 放入 RS_INIT）→ 服务读 `rprocpub` 这条链没有一致的类型和无效值语义。文档声称 `Option` "杜绝 GRANT_VALID 手动判定"，但测试里的 `-1` 正好说明该保证没有落地。
- 证据: L845 "`rproctab_gid: Option<u32>`（`GrantId = i32`）"；L849 "用 `Option<GrantId>` 表达……杜绝 C 的 `GRANT_VALID()` 宏手动判定"；L1092 "VM/PM 各收一条 `RS_INIT`（逐字段断言：FRESH/gid=-1/…）"。
- 最小修复: 统一为 `Option<GrantId>` 且 `GrantId = i32`；把创建失败映射为 `None`/`Err`，并规定 `init_service` 对 `None` 的行为；测试要么断言合法 gid（>=0），要么明确 `-1` 是 mock 的"未接线"哨兵并解释 Step 2 为何仍继续。
- 置信度: 0.9
- 若我错了: 若测试里的 `gid=-1` 指另一个字段（例如"没有 grant 时消息里填 -1"的独立字段），而不是 `rproctab_gid`，则矛盾只是表面；但文档说 RS_INIT 携带的就是这个 gid，且全文只有这一个 gid 字段。

---

## FINDING C-07

- 位置: L44、L411–L413、L424，与 L395、L567 对读
- 严重度: P1
- 问题: 概念章说 Step 2 "发送 `RS_INIT` 初始化消息"给服务，Step 3 "阻塞接收全部服务的 `RS_INIT` 回复"；§2.3.5 说"全部剩余服务"。但 C 摘录的 `init_service` 在 `if(boot_image_priv->flags & SYS_PROC)` 之内，而 priv 表里的 `INIT_PROC_NR` 是 `USR_F`（`BILLABLE|PREEMPTIBLE`），根本不会走 `init_service`，也不会发 RS_INIT，也不计入 `nr_uncaught_init_srvs`。所以"全部服务"对 INIT 是假的。
- 影响: Step 2→Step 3 的计数公式和"所有 boot 服务 ready"的屏障被写成普遍结论。读者会以为 INIT 也参加 RS_INIT 握手，或以为 Step 3 的循环次数等于 12；实际只覆盖 VM + 非同步的 SYS_PROC 服务。
- 证据: L411–L412 "普通服务……`init_service`（发送 RS_INIT 初始化消息）"；L395 `if(boot_image_priv->flags & SYS_PROC) {`；L567 `INIT_PROC_NR | "init" | USR_F`；L424 "全部剩余服务"；L44–L46 "全部服务"。
- 最小修复: 把"全部服务"改为"全部 SYS_PROC 服务（INIT 是 USR_F，不参加 RS_INIT 握手）"，并写出计数公式：VM + 未标 `SF_SYNCH_BOOT` 的 SYS_PROC 服务。
- 置信度: 0.9
- 若我错了: 若这一版 Minix 的 `USR_F` 也含 `SYS_PROC`，INIT 会通过闸门；但文档 L567 自己定义 `USR_F = BILLABLE|PREEMPTIBLE`，按文档证据 INIT 被排除。

---

## FINDING C-08

- 位置: L424
- 严重度: P2
- 问题: "这一步使 boot 严格串行"把屏障说成了串行化。Step 2 已经把非 RS/VM 服务全部放行，非同步服务的初始化是并发进行的；Step 3 只是让 RS 等到所有 ready 收齐再进 Step 4。RS 的推进是串行的，服务本身的初始化不是。
- 影响: 读者会误以为 Step 3 把服务一个接一个地初始化，从而误解 `nr_uncaught_init_srvs` 为什么可能大于 1、Step 3 为什么要循环收多条消息。
- 证据: L424 "这一步使 boot 严格串行：所有 boot 服务初始化完成后，RS 才进入 Step 4"；L402–L404 非同步分支只 `nr_uncaught_init_srvs++` 后继续循环，服务已经放行；L418–L421 Step 3 循环接收剩余消息。
- 最小修复: 改为"Step 3 是 boot 屏障：RS 阻塞到所有已放行服务的 init ready 收齐后才进入 Step 4；服务本身在 Step 2 之后并发初始化"。
- 置信度: 0.85
- 若我错了: 若内核在 boot 期只有一个用户进程可运行、且没有抢占，服务可能事实上一个一个跑；但那是调度实现的结果，不是 Step 3 的因果作用，"严格串行"仍属措辞过强。

---

## FINDING C-09

- 位置: L754–L758，与 L1090 对读
- 严重度: P1
- 问题: 文档把 C 的 `catch_boot_init_ready(ANY)` 阻塞接收等同于 "fail-closed"，并说 Rust Step 3 "计数 > 0 时同样 fail-closed"。C 的行为是阻塞等待消息到达，没有错误返回；Rust 的 `Err(BootError::Kernel(ENOSYS))` 是立即中止 boot。两者不是同一语义：前者是等待，后者是失败退出。
- 影响: 一个"服务慢但活着"的场景，C 会继续 boot，Rust 占位实现会失败；一个"服务永不来"的场景，C 会挂起，Rust 会报 ENOSYS。把两者都叫 fail-closed 会掩盖这是一处行为偏差，降低后续 12 落地时的验收标准。
- 证据: L756 "C 阻塞接收 = fail-closed，main.c:401-407"；L1090 "有未收 init-ready 时 Step 3 fail-closed（`Err(BootError::Kernel(ENOSYS))`，对应 main.c:401-407 的阻塞 receive 语义）"。
- 最小修复: 分开表述："C 是阻塞等待；Rust 在 12 未接线时以 ENOSYS 立即失败，这是占位期偏差；12 落地后应恢复阻塞接收（若要超时/放弃，需作为显式架构决策）"。
- 置信度: 0.9
- 若我错了: 若 `catch_boot_init_ready` 内部有超时或错误路径会返回失败，那么"fail-closed"尚可辩护；但文档展示的是无超时的裸阻塞调用。

---

## FINDING C-10

- 位置: L1072，与 L320、L389–L398 对读
- 严重度: P1
- 问题: `test_init_fresh_step_order` 声称用 mock 记录调用序列并断言"顺序与 C 一致"，但列出的期望序列只有 `privctl(SetSys)×10 → sched×10+Allow×10 → getnpid×12+setalarm(100)`。C 的 Step 1 还对全部 12 个服务调用 `sys_getpriv`（含 RS/VM），Step 2 还对 SYS_PROC 服务调用 `init_service`（发 RS_INIT）。这些调用（尤其 `getpriv` 是 Step 1 内核提交的同步回读）都不在期望序列里。并且 `sched×10+Allow×10` 的合并写法掩盖了 C 的逐服务交错顺序（sched→allow→sched→allow…）。
- 影响: 顺序测试是四步对应关系的主要证据。它要么漏记了载荷关键的 `getpriv`/`init_service`，要么"顺序一致"只对记录子集成立；两种情况下都不能支撑"与 C 顺序一致"的强结论。
- 证据: L1072 期望序列；L320 `sys_getpriv(&(rp->r_priv), ip->endpoint)` 位于 Step 1 循环内；L389–L398 每个普通服务依次 `sched_init_proc`、`sys_privctl(ALLOW)`、`init_service`。
- 最小修复: 写明 mock 实际记录的调用子集及为何省略项与顺序无关；或把期望序列补成 `getpriv×12` 和逐服务的 `sched→allow→init_service` 三元组，`×10+×10` 改为真实交错序。
- 置信度: 0.85
- 若我错了: 若 `MockKernelApi` 故意不记录 `getpriv`/`init_service`，测试只断言已记录子集的相对顺序，那么"顺序与 C 一致"应改成"已记录子集的顺序与 C 的同类调用一致"，原文的全称结论仍过强。

---

## FINDING C-11

- 位置: L726–L733、L996、L1002、L833–L841，与 L448–L483 对读
- 严重度: P1
- 问题: C 的 `sef_cb_init_fresh` 在 Step 4 之后还有 `#if USE_LIVEUPDATE` 自升级块（main.c:436–491）。Rust `init_fresh()` 的编排只调用 `step0_prepare`→`step4_finish` 然后 `Ok(())`，没有调用 `self_update`；`self_update` 只在方法清单里出现，文中找不到调用点。于是 §4.4 的 CSSCM 断言"机制步骤一一对应（无增减）"不成立，除非 `self_update` 在未展示的其他位置被调用。
- 影响: feature 打开时，自升级动作没有描述清楚的调用时机；feature 关闭（默认）时，Rust 默认 boot 比 C 默认 boot 少一个动作。这直接影响"Rust BootInit 与 C 顺序的对应"和默认行为是否等价。
- 证据: L726–L733 `init_fresh` 只到 `step4_finish` 后 `Ok(())`；L1002 `self_update` 作为独立方法列出；L996 "机制步骤一一对应（无增减）"；L833 "cargo feature `live-update`（默认关）"；L483 C 默认 yes；L451–L480 `#if USE_LIVEUPDATE` 块。
- 最小修复: 在 `init_fresh` 末尾补 `#[cfg(feature = "live-update")] self.self_update(sys)?;`（或明确说明移到 `RsServer::init` 的哪个位置）；把"无增减"改成"四步无增减；USE_LIVEUPDATE 是 feature-gated 的额外动作"。
- 置信度: 0.9
- 若我错了: 若 `RsServer::init` 在 `BootInit::init_fresh` 返回后、`into_state` 之前调用 `self_update`，那么调用链存在但未展示；文档仍需写出这个调用点，默认关与 C 默认 yes 的差异也仍然成立。

---

## FINDING C-12

- 位置: L746–L750，与 L122、L216、L124 对读
- 严重度: P1
- 问题: N3 注说 `step0_prepare` 调 `sys.get_machine()` "对齐 C main.c:53 的位置——`sef_local_startup` 之后、主循环之前"。但 C 的 `main()` 先调 `sef_local_startup()`，而 `sef_local_startup()` 末尾就是 `sef_startup()`（L216），`sef_startup()` 又分派到 `sef_cb_init_fresh()`（L192/L699）。所以 C 的 `sys_getmachine` 在四步 boot **之后**；Rust 的 `step0_prepare` 在四步 boot **之前**。两端位置相反，不是对齐。
- 影响: machine 快照的获取时机改变：Rust 在 Step 1–4 期间就持有 machine，C 在 boot 完成后才取。失败时序也变（`test_boot_failure_propagates_from_step0` 显示 get_machine 失败会让后续步骤零调用）。文档把这一变化说成对齐，掩盖了依赖图差异。
- 证据: L748 "对齐 C main.c:53 的位置——`sef_local_startup` 之后、主循环之前"；L122 `sef_local_startup();`；L216 `sef_startup();` 位于 `sef_local_startup` 末尾；L192/L699 `sef_startup()` 分派到 `sef_cb_init_fresh`；L727–L731 Rust step0 在 step1 之前。
- 最小修复: 二选一：把 Rust 的 `get_machine` 移到 `init_fresh` 之后（匹配 C），或明确写"Rust 把 machine 快照提前到 step0，这是 A-? 的时序变化，原因是……"，删除"对齐"。
- 置信度: 0.85
- 若我错了: 若 C 的 `sef_startup()` 只做注册并立即返回、`sef_cb_init_fresh` 是主循环第一次接收时才调，那么 C 的 main.c:53 可能先于回调；但文档 L192/L699 自己说 `sef_startup()` 分派到 `sef_cb_init_fresh`，按文档自身模型，回调在 `sef_local_startup` 内。

---

## FINDING C-13

- 位置: L284
- 严重度: P1
- 问题: 这一段在"C 源码分析"里讲 Rust 的 `validate_tables`、placeholder 和 `boot_img` 名钳制，并把内核 boot 表端点的 `_ENDPOINT(0, proc_nr)` 派生引到 `main.c:196`。但同一文档 L247 明确把 main.c:196 标为 `sys_getimage(image)`，不是端点派生；`validate_tables` 是 Rust 侧检查，不是 C 的 `sef_cb_init_fresh` 行为。
- 影响: 三表与内核 `boot_image` 对齐是本篇的核心因果主张，而这一句的锚点指错文件位置；读者按 main.c:196 去核对会读到 `sys_getimage`，无法验证端点派生。C 章与 Rust 章的证据边界也被打乱。
- 证据: L284 "内核 boot 表端点由 `_ENDPOINT(0, proc_nr)` 派生（main.c:196）"；L247 "`sys_getimage(image)`（main.c:196）"；L284 同时出现 `validate_tables`、`placeholder`、`boot_img` 等 Rust 术语。
- 最小修复: 把 `validate_tables`/placeholder/`boot_img` 三句移到 §4.4 的 Rust 实现说明；`_ENDPOINT(0, proc_nr)` 改引内核 boot 表生成处（kernel/table.c）或删掉该括号。
- 置信度: 0.9
- 若我错了: 若本文档的行号取自另一版 main.c、196 恰好是端点派生，那么锚点可能对；但同一文档 L247 已把 196 定义为 `sys_getimage`，文档内部仍不一致。

---

## FINDING C-14

- 位置: L33
- 严重度: P1
- 问题: 文档断言内核表头注释所说的 NOTIFY 投递优先级"内核里并无代码实现这句话"，真实的 NOTIFY 挑选序按 proc 号升序扫描、DS 排在第七；但全文没有给出任何 C 文件/行号、grep 或代码摘录，只把"六序对照"指向 `../00-master-plan/README.md` 和 `00-rs-overview.md`。这是一个强内核行为断言，却只有跨文档指针，没有 ground truth 锚点。
- 影响: 该断言是"登记序 ≠ 执行序"论证的支点。若 NOTIFY 扫描序或 DS 的位置不对，后面的"表序只是意图声明"推论仍可能成立，但具体证据是错的；也违反文档自己声明的真值优先序（C 源码 > 设计/技术文档）。
- 证据: L33 "内核里并无代码实现这句话——真实的 NOTIFY 挑选序按 proc 号升序扫描，DS 反而排在第七（六序对照见 `../00-master-plan/README.md`……）"；同段没有 `minix3/...` 文件行号或代码引用。
- 最小修复: 补内核侧 NOTIFY 选择循环的文件与行号，并附 grep/代码摘录；若暂不能给，就弱化为"NOTIFY 实际选择序需在 kernel 侧核对"，删掉"DS 第七"这种精确断言。
- 置信度: 0.95（证据缺失）；0.6（事实是否错误）
- 若我错了: 若引用的 README 已含完整 C 锚点、本篇只是有意委派，那么证据在别处存在；但按本篇自己的引用标准，跨文档指针不能替代 C 锚点，且材料隔离下读者无法在此验证。

---

## FINDING C-15

- 位置: L220
- 严重度: P2
- 问题: "RS 注册全部 7 个回调，是**唯一注册全量的用户态服务**（其他服务只注册自己需要的子集）"是对所有其他用户态服务的全称断言，但文档只展示了 RS 的 `sef_local_startup()`，没有其他服务的注册表，也没有 grep 证据。
- 影响: "RS 特殊，因为它注册全量"可能不是事实；若 PM/VFS 也注册 7 个，则"唯一"错误。更重要的是，RS 的独特地位来自它是授权放行者，不是回调数量，这句话即使为真也不是因果理由。
- 证据: L220 全称断言；全文无其他服务 `sef_local_startup` 的引用或跨服务 grep。
- 最小修复: 删掉"唯一"，或补一条 grep（例如对 `minix3/minix/servers/*/main.c` 的 `sef_setcb_*` 统计）作为证据；同时说明回调数量不是 RS 角色的原因。
- 置信度: 0.9（证据缺失）；0.5（事实是否错误）
- 若我错了: 若项目已在别处验证并把这句当摘要，仍需给锚点；按本篇的"所有行号以 grep 实证为准"标准，这句没有实证。

---

## FINDING C-16

- 位置: L357，与 L525 对读
- 严重度: P2
- 问题: L357 说 `boot_image_info_lookup` "一次调用同时查找 image/priv/sys/dev 四张表"；L525 紧接着说 Step 1 实际传 `&ip, NULL, &boot_image_sys, &boot_image_dev`，"只查 image/sys/dev 三表，priv 表条目由外层循环直接索引"。函数支持四个输出，不等于 Step 1 查了四张表。
- 影响: 读者会以为 Step 1 对 priv 表也做了 fail-closed 查找，实际 priv 条目的取得方式是外层数组索引；priv 不匹配的检测路径因此不同。
- 证据: L357 "一次调用同时查找 image/priv/sys/dev 四张表"；L525 "只查 image/sys/dev 三表"。
- 最小修复: 改为"`boot_image_info_lookup` 支持 image/priv/sys/dev 四路输出；Step 1 只传 image/sys/dev 三个输出，priv 由外层循环按索引取得"。
- 置信度: 0.95
- 若我错了: 若"一次调用"指函数 API 的能力而非本次调用的实参，句子技术上不假，但仍会误导，修法不变。

---

## FINDING C-17

- 位置: L552、L1064，与 L567 对读
- 严重度: P2
- 问题: priv 表被称为"12 个系统服务"，但其中 `INIT_PROC_NR` 标的是 `USR_F`（`BILLABLE|PREEMPTIBLE`），是用户进程；C 的计数核对实际统计的是"非内核 boot 条目"（`if(iskerneln(...)) continue; nr_image_srvs++`），不是"系统服务"。
- 影响: 这个术语混淆直接喂养 C-07：如果把 INIT 算进"系统服务"，读者自然期待它参加 RS_INIT 握手。计数核对的实际谓词是"不是内核任务"，不是"是系统服务"。
- 证据: L552 "12 个系统服务 + NULL 哨兵"；L567 `INIT_PROC_NR | "init" | USR_F`；L265 `if(iskerneln(_ENDPOINT_P(ip->endpoint))) continue; nr_image_srvs++;`。
- 最小修复: 改为"12 个非内核 boot 条目（11 个系统服务 + init 用户进程）"，计数核对描述改用"非内核条目"。
- 置信度: 0.9
- 若我错了: 若本文档把 boot 映像里的 USR_F 进程也统称"系统服务"，那是本地命名约定；但文档同时区分 RS/VM/SRV/USR flags，应该把这个区分说出来。

---

## FINDING C-18

- 位置: L301–L309、L349
- 严重度: P2（若宏体依赖 `r_flags` 则为 P1）
- 问题: Step 1 摘录在 L303–L309 用 `SRV_OR_USR(rp, ...)` 选择系统/用户值，但唯一给 `rp->r_flags` 赋值的语句在 L349（`RS_IN_USE | RS_ACTIVE`），且不含 `RS_SYS_PROC`。如果 `SRV_OR_USR` 的判据是 `rp->r_flags & RS_SYS_PROC`（该宏的常见定义），按摘录字面执行，所有服务都会被当成用户进程；如果它是别的判据，文档没有给出宏体，读者无法验证"表 flags → 每服务 priv flags"这一步。
- 影响: priv/send mask/call mask 是 Step 1 和 Step 2 的载荷；这条链缺一步，整个"逐服务设置属性"的因果解释就不可复核。`test_init_fresh_populates_table` 只查槽位，不查每服务的 priv 派生值，测试也补不上这个缺口。
- 证据: L303–L309 `rp->r_priv.s_init_flags = SRV_OR_USR(rp, SRV_I, USR_I);` 等；L349 `rp->r_flags = RS_IN_USE | RS_ACTIVE;`；摘录中无 `RS_SYS_PROC` 赋值；L556–L567 表里有 `SYS_PROC` 位。
- 最小修复: 补回缺失派生（例如先 `rp->r_flags = RS_IN_USE | RS_ACTIVE; if (boot_image_priv->flags & SYS_PROC) rp->r_flags |= RS_SYS_PROC;` 再调用 `SRV_OR_USR`），或注明摘录有删节并给出 `SRV_OR_USR` 宏体。
- 置信度: 0.65
- 若我错了: 若 `SRV_OR_USR` 读的是 `boot_image_priv->flags` 或 `rpub->endpoint` 而非 `r_flags`，那么顺序无害；但文档没给宏定义，按现有证据只能判定为缺步。

---

## FINDING C-19

- 位置: L483、L833–L841
- 严重度: P2
- 问题: C 的 `USE_LIVEUPDATE` 默认 yes；Rust 的 `live-update` feature 默认关。文档把它解释为"机制落地前的 fail-closed，不产生半吊子代码路径"，但没有明确这是一个默认构建的外部行为差异：默认 Rust RS 不自升级，默认 C RS 自升级。
- 影响: 若"Rewrite 保持外部行为"是项目的基线，这是一个架构演进/阶段性取舍，而不是纯构建卫生。读者比较默认构建会看到不同的 boot 末尾行为，文档没有把这个差异写成结论。
- 证据: L483 "默认 yes"；L833 "cargo feature `live-update`（默认关）"；L840 "在机制落地前，feature 关闭时该流程不编译（fail-closed……）"。
- 最小修复: 补一句"默认构建下 Rust 不做 RS 自升级，与 C 默认 yes 存在外部行为差异；这是 A-11 的阶段性取舍，开启 feature 后恢复 §2.3.7 调用链"，并说明生产构建是否启用该 feature。
- 置信度: 0.85
- 若我错了: 若生产构建总是启用 `live-update`，默认关只是开发默认，那么差异不落地；文档应写明哪个 build profile 是生产口径。

---

## FINDING C-20

- 位置: L754–L758、L1000、L1090、L1093
- 严重度: P1
- 问题: Step 3 的契约在文档里有两个版本。L756 说"Step 3 在计数 > 0 时同样 fail-closed"，读起来是只要还有未收 ready 就一定报错；L1000 说"step3_catch_init_ready：循环调用 `catch_boot_init_ready`（12 机制，receive 缝已接线）"，读起来是能收消息的正常循环；L1090 的测试断言 pending 时 `Err(Kernel(ENOSYS))`，L1093 的测试又断言 mock inbox 有 VM ready 时 step3 能清 `INITIALIZING`、计数归零。文档没有给出区分条件（例如"生产 `IpcApi` 未实现，mock 有消息"）。
- 影响: 读者无法判断 Step 3 到底是"永远 fail-closed 的占位"还是"已接线的接收循环"，也无法判断两个测试是否互斥。Step 2 放行 → Step 3 ready 的边界因此不可验收。
- 证据: L756 "Step 3 在计数 > 0 时同样 fail-closed"；L1000 "循环调用 `catch_boot_init_ready`（12 机制，receive 缝已接线）"；L1090 `test_step3_fails_closed_when_init_ready_pending` 断言 `Err`；L1093 `test_step3_vm_init_roundtrip_clears_initializing` 断言成功清零。
- 最小修复: 写成条件式契约："Step 3 循环 `catch_boot_init_ready(ANY)`；生产 `IpcApi` 未实现时第一次 catch 返回 `Err(Kernel(ENOSYS))`；mock inbox 有消息时消费并递减。两个测试分别覆盖这两条路径。"然后据此统一 L756 的措辞。
- 置信度: 0.85
- 若我错了: 若 L756 的"fail-closed"本意就是"生产占位 fail-closed，mock 路径已接线"，两句话可以共存；但原文没有这个限定，读起来像普遍设计规则，仍应改写。

---

## FINDING C-21

- 位置: L867、L1016，与 L1106 对读
- 严重度: P2
- 问题: `dispatch_request` 对"已定义归属但 handler 未实现"的调用号返回 `ENOSYS`，文档说这"对应 C default 分支 main.c:118-121 的 fail-closed 语义"。C 的 default 分支处理的是**未知调用号**；已定义的 RS_UP 等调用号在 C 里是已实现的，不会走 default。Rust 的"已知但未实现"是临时 stub，不是 C 的 default。
- 影响: 读者会以为 C 的 default 分支覆盖了 RS_UP 等已知调用；也模糊了"未知请求 → ENOSYS"与"已知请求尚未接线 → ENOSYS"两种不同原因，测试 `test_classify_request_unknown` 才是 C default 的真正对应。
- 证据: L867 "对已定义归属的调用号返回 `DispatchResult(ENOSYS)`……对应 C default 分支"；L1016 同样表述；L1106 `test_classify_request_unknown` 测的是未知调用号。
- 最小修复: 分两句写："未知调用号 → C default 的 ENOSYS；已知调用号但 handler 未实现 → Rust stub 的 ENOSYS（线值相同，原因不同，13/14/16 落地后逐项替换）"。
- 置信度: 0.9
- 若我错了: 若 Rust 故意把所有未接线调用当成"未知"处理，线值一样，但语义类别仍不同；文档应说明"同线值、不同原因"。

---

## FINDING C-22

- 位置: L1112
- 严重度: P2
- 问题: 文档说"全部测试可 grep 验证：`rg -c "#[test]"` 全 crate 合计 359"。`rg -c` 输出的是每个文件的匹配行数，不是一个总数；要得到 359 还需要额外的求和管道。这个命令按字面执行不能直接产出所宣称的合计，也没有给出输出。
- 影响: 基线证据强度被高估。项目要求 Gate 证据附命令与输出；这句只给了一个不完整的命令，属于 L2/L3 证据而非 L1。
- 证据: L1112 原文；全文没有 `rg -c` 的实际输出或求和命令。
- 最小修复: 给出可直接复现的完整命令与输出（例如 `rg -c '#\[test\]' os/servers/rs/src | awk -F: '{s+=$2} END{print s}'`），或删掉 grep 断言，只保留 `cargo test` 的输出。
- 置信度: 0.9
- 若我错了: 若项目有 `rg -c` 的包装脚本会打印总计，或 359 另有独立验证，那么这句话只是表述不精确；但文档仍应展示命令与输出。

---

## FINDING C-23

- 位置: L45，与 L399–L404 对读
- 严重度: P2
- 问题: 概念章把"同步 boot 的服务立即收"放在 Step 3 的描述里，C 章却显示 `catch_boot_init_ready(rpub->endpoint)` 在 Step 2 的循环内、`init_service` 之后立即执行。也就是说，同步服务的 ready 是在 Step 2 完成的，Step 3 只收剩余的非同步服务。"Step 2 放行、Step 3 收齐"不是严格两阶段。
- 影响: 读者会把所有 ready 都归到 Step 3，从而误解 Step 2 的阻塞点和 `nr_uncaught_init_srvs` 的计数时机；也会误判 Step 2 的失败模式（同步服务没 ready 时，失败发生在 Step 2，不在 Step 3）。
- 证据: L45 "Step 3……（同步 boot 的服务立即收，其余累积后统一收）"；L399–L401 `if(rpub->sys_flags & SF_SYNCH_BOOT) catch_boot_init_ready(rpub->endpoint); else nr_uncaught_init_srvs++;` 位于 Step 2 循环内。
- 最小修复: 在 Step 2 描述里明确"SF_SYNCH_BOOT 服务在放行后立即同步收 ready"；Step 3 改为"只收剩余非同步服务的 ready"。
- 置信度: 0.9
- 若我错了: 若作者把 Step 2 的同步 catch 视为"Step 2 放行的一部分、Step 3 描述只是汇总"，那么两处可以并存；但概念章把它归入 Step 3 的括号里，仍会造成边界误读。

---

## FINDING C-24

- 位置: L284、L590，与 L269–L270、L1088 对读
- 严重度: P1
- 问题: 文档把 `nr_image_srvs != nr_image_priv_srvs` 的计数核对称为"这是'RS 的表与内核的表一致'的启动协议完整性检查"，§2.7 又说"条目数必须与内核 boot_image 表系统服务数一致"。计数相等只证明非内核条目**数量**相等；端点集合不同但数量相同的表仍能通过 Step 0。真正的端点集合一致性要到 Step 1 的 `boot_image_info_lookup` 未命中 panic 才建立；L1088 的 `test_step1_lookup_failure_propagates` 正是"计数一致但成员错位 → step1 才报错"的证据。
- 影响: 因果链少了"计数核对只查基数，集合对齐在 Step 1"这一步。读者会以为 Step 0 就能发现所有表不一致，低估"同数错位"的失败位置；这也影响对 Rust `validate_tables` 应检查什么的理解。
- 证据: L284 "计数核对……这是'RS 的表与内核的表一致'的启动协议完整性检查"；L269–L270 `if(nr_image_srvs != nr_image_priv_srvs) panic("boot image table and boot image priv table mismatch");`；L1088 "计数一致但成员错位的表 → step1 `Err(BootError::Lookup(ImageTable))`"。
- 最小修复: 改为"Step 0 的计数核对只保证非内核条目数量相等；端点集合的一致性由 Step 1 的 `boot_image_info_lookup` 未命中 panic 保证。两者合起来才是表对齐"。
- 置信度: 0.9
- 若我错了: 若 Rust 的 `validate_tables` 在计数之外还校验了完整端点集合（文档只提到 `endpoint.slot()==proc_nr` 和计数），那么 Rust 的 Step 0 比 C 强；但文档把"一致性检查"归给 C 的计数核对，这一归因仍然过强。

---

## SUMMARY

- 总体：文档结构完整，四步骨架、三表描述、Rust 类型化与测试清单都在；但有两处 P0 级硬伤，使"四步 boot 的依赖理由"和"当前测试基线"这两条最吃重的因果链不可直接采信。
- P0（2 条）：C-01 Step 1 被反复写成"不向内核提交"，与文档自己的 C 摘录（SET_SYS/getpriv）直接矛盾，且例外方向写反；C-02 同一章给出 208/0 与 359/0 两个基线，01 范围计数 29 与 41 互斥，测试表只能数出 26 个 boot 测试，无法对账。
- P1（13 条）：核心是边界与缺步。C-03 priv 表序被当成 boot 运行序；C-04/C-05 SEF 启动消息与 `RS_LU_PREPARE`、`RsServer::init(Fresh)` 的边界含混；C-06 grant 类型 `Option<u32>` 与 `GrantId=i32`、测试 `gid=-1` 不一致；C-07 Step 2/3 的"全部服务"忽略了 `SYS_PROC` 闸门与 INIT 的 `USR_F`；C-09/C-20 C 的阻塞接收被当作 fail-closed、Step 3 契约两说；C-10 顺序测试漏掉 `getpriv`/`init_service`；C-11 `self_update` 无调用点却宣称"无增减"；C-12 machine 快照位置与 C 相反；C-13 C 章混入 Rust `validate_tables` 且锚点指错；C-14 NOTIFY 顺序断言无 C 锚点；C-24 计数核对只查基数，不查端点集合。
- P2（9 条）：C-08 "严格串行"是屏障不是串行；C-15 "唯一注册全量"无证据；C-16 四表查找与三表实参不一致；C-17 "12 个系统服务"包含 USR_F 的 INIT；C-18 `SRV_OR_USR` 与 `r_flags` 的赋值顺序缺解释；C-19 `USE_LIVEUPDATE` 默认 yes→Rust 默认关未写成行为差异；C-21 已知未实现 handler 被当成 C default；C-22 `rg -c` 不是总数；C-23 同步 ready 实际在 Step 2 收，概念章归到 Step 3。
- 单形态写成普遍结论的典型：C-08（一个 RS 屏障写成服务串行）、C-15（RS 的回调集写成所有服务的唯一性）、C-19（开发默认写成等价默认）、C-24（计数相等写成表一致）。
- 建议优先修：先修 C-01 与 C-02（P0，直接改叙述与基线），再修 C-07/C-11/C-12/C-20（这四条决定四步与 C 的对应是否成立），最后处理 C-03/C-24 的表序与对齐语义。修完后重跑测试计数并把 §5.1–§5.3 的测试表逐名对齐。
