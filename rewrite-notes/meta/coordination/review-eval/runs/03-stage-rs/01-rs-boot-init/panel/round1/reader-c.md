# READER-C R1
DOC 01-rs-boot-init.md

范围：只读本文档，追关键因果主张在概念、C 分析、Rust 设计、实现、测试中的复述。严重度口径：P0 = 文档内可证伪、直接改变核心行为叙述的矛盾；P1 = 因果链缺步、语义替换或关键边界自相矛盾；P2 = 局部不一致、证据强度或术语问题；P3 = 表述与索引瑕疵。

## FINDING READER-C-01（P1）

- 位置: §1（第 13、17 行）、§1.1（第 33 行）、§2.7 priv 表（第 535 行）
- 严重度: P1
- 问题: 自举矛盾的立论前提是"RS 是普通用户进程、没有权限表"，但后文把 RS 定为 root system process（`RSYS_F`/`ROOT_SYS_PROC`）。"普通"与"root sys proc"并置且无调和说明，"没有权限表"也未限定是 RS 本地表还是内核侧 priv 结构。
- 影响: 读者无法判断 RS 在 Step 1 之前已具备哪些内核侧权限。这决定 Step 1 的"本地建表"到底补什么、RS/VM 例外为什么成立；前提失真会让四步因果链的第一环失稳。
- 证据或推理: §1 原文"它自己启动时没有进程表、没有权限表、也没有可用的服务描述"；§1.1 原文"第一个能运行的用户服务是 **VM**，RS 作为 root system process 紧随其后"；§2.7 原文"`RSYS_F`（`SRV_F|ROOT_SYS_PROC`，root sys proc）"。
- 最小修复: 拆成两句事实：内核侧 RS 已有 `ROOT_SYS_PROC` priv（boot 期由内核按 boot 表预置）；RS 本地缺少的是 `rproc`/`rprocpub` 与三张静态表的运行时视图。删去"普通用户进程"或改为"用户态进程（非内核任务）"。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若"普通用户进程"仅指"运行在用户态、非内核任务"，且"没有权限表"仅指 RS 本地表，语义可成立；但文中必须写明，否则字面与 `RSYS_F` 冲突。

## FINDING READER-C-02（P0）

- 位置: §1（第 43 行）、§2.3.3 代码（第 316、320 行）、§2.8（第 576 行）
- 严重度: P0
- 问题: Step 1 被三次描述为"纯本地、不向内核提交任何东西"，但同一文档的 C 代码在 Step 1 里对每个非 RS/VM 服务调用 `sys_privctl(SYS_PRIV_SET_SYS)`，并对全部 12 项无条件调用 `sys_getpriv`。"除 RS/VM 例外"的括号还把例外方向写反：RS/VM 恰是被跳过提交的一方，其余 10 个都在提交。
- 影响: "先本地建表、再统一放行"必须改述为"先提交并回读 priv、再放行"才成立。Step 1 不是不触内核，而是做了 priv 的提交与同步；Step 2 的 `SYS_PRIV_ALLOW` 依赖 Step 1 已在 kernel 侧建立 priv 结构。按原文，读者会误判 boot 期的内核交互量与失败面。
- 证据或推理: §1 原文"此时**没有**向 kernel 提交任何东西（除 RS/VM 例外），纯本地准备"；§2.8 原文"Step 1 不向内核提交"；§2.3.3 原文`sys_privctl(ip->endpoint, SYS_PRIV_SET_SYS, &(rp->r_priv))`，例外 if 之后还有`sys_getpriv(&(rp->r_priv), ip->endpoint)`。
- 最小修复: 改为"Step 1 完成本地槽位与 priv 结构构造，并把 priv 提交给内核（`SYS_PRIV_SET_SYS`，RS/VM 跳过）后 `sys_getpriv` 回读；Step 1 不做的是放行（`SYS_PRIV_ALLOW`）与调度登记"。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 除非 `SYS_PRIV_SET_SYS` 不是一次内核 syscall（函数名与 §3.5 的 SysApi 归属都表明它是），否则该矛盾无法消解。

## FINDING READER-C-03（P1）

- 位置: §1.2（第 52 行）、§2.2（第 220 行）、§4.1（第 894 行）、§4.4（第 957 行）
- 严重度: P1
- 问题: 入口触发链自相矛盾。§1.2/§2.2 说 `sef_startup()` 收到第一条 `RS_INIT` 后分派 `sef_cb_init_fresh`；§4.4 又说 RS 自身 `ROOT_SYS_PROC` 早退不发消息；§4.1 的实现则用字面量 `server.init(SefInitType::Fresh)` 直接触发。按 boot 时序，RS 的 fresh 初始化必须先于所有普通服务被放行，因此不存在一条先于 boot 的 `RS_INIT` 可供 RS 自己消费。
- 影响: 启动链首环（谁触发 fresh）无法闭合。读者无法判断 `init_type` 是消息驱动还是调用参数，也无法解释 RS 与普通服务的 SEF 路径差异；后文"Step 1 为什么不能提前放行"失去前提。
- 证据或推理: §1.2 原文"再由 `sef_startup()` 把第一条 `RS_INIT` 分派到 `sef_cb_init_fresh()`"；§4.1 原文"`server.init(SefInitType::Fresh)`"；§4.4 原文"RS 自身 `ROOT_SYS_PROC` 早退不发消息"。
- 最小修复: 明确两条路径：普通服务由 RS 下发的 `RS_INIT` 驱动分派；RS 自身由 `sef_startup` 直接以 `Fresh` 调用（或写明 SEF 对 root sys proc 的合成规则），并把 §1.2/§2.2/§4.5 的措辞按"服务侧/RS 侧"拆开。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 libsys 的 `sef_startup` 对 RS 确有一条收消息路径（例如由 VM 代发），文档必须给出该路径与时序；§2.3.4 只给"VM 仍会异步发回一条"，且那发生在 Step 2 之后，仍不支持"第一条触发 fresh"。

## FINDING READER-C-04（P1）

- 位置: §1.3 术语、§2.2（第 220 行）、§2.3.1（第 254 行）、§2.3.4（第 412 行）、§2.3.5（第 424 行）
- 严重度: P1
- 问题: 同一个消息名 `RS_INIT` 同时表示两个方向：RS 发给服务的初始化下发（携带 `rproctab_gid`），以及服务发给 RS 的 ready 回执。文档从未声明这两个方向共用 type，读者会把 §2.3.4 的"发送 RS_INIT"与 §2.3.5 的"接收 RS_INIT 回复"读成同一方。
- 影响: Step 2 的"发"与 Step 3 的"收"不能被连成一条协议；`RS_INIT` 到底是 init 请求还是 ready 确认，在 §1.3（"boot 期用于放行后确认"）与 §2.2（"payload 含 `sef_init_info_t`"）之间摇摆，Step 3 的计数对象因此不可判定。
- 证据或推理: §2.3.1 原文"服务通过 `RS_INIT` 消息携带此 gid 读取公开信息"；§2.3.4 原文"`init_service`（发送 RS_INIT 初始化消息，12）"；§2.3.5 原文"阻塞接收全部剩余服务的 `RS_INIT` 回复（`catch_boot_init_ready(ANY)`，机制归 12）"。
- 最小修复: 补一句协议方向说明（RS→服务：init 下发，含 `init_type` 与 grant gid；服务→RS：init ready 回执），并在术语表分别命名；若两个方向 m_type 相同，明写"同名双向"。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 C 里两个方向的 `m_type` 不同，则按实际类型改写即可；"文档未声明方向"这一缺陷仍成立。

## FINDING READER-C-05（P1）

- 位置: §1（第 44 行）、§2.3.4 代码（第 395 行）与要点（第 412 行）、§2.7 priv 表（第 531、546 行）
- 严重度: P1
- 问题: §1 把 Step 2 写成"每个服务都发送 `RS_INIT`"，§2.3.4 的代码却用 `if(boot_image_priv->flags & SYS_PROC)` 把 `init_service` 包住。§2.7 表中 `INIT_PROC_NR` 是 `USR_F`（`BILLABLE|PREEMPTIBLE`），不含 `SYS_PROC`，因此 init 从不收 `RS_INIT`、也不进 Step 3 的等待集合。§2.3.4 的要点复述同样漏掉这个守卫；§2.7 还把 12 项统称"12 个系统服务"。
- 影响: 把"12 个 boot 成员都走 init_service"当成普遍结论，会让 Step 3 的等待集合与 `nr_uncaught_init_srvs` 多算一个成员；读者的因果模型在"谁进 Step 3"这一步就错了。
- 证据或推理: §1 原文"并发送 `RS_INIT` 初始化消息（RS/VM 例外：它们已在运行，直接 `init_service` 自模拟）"；§2.3.4 原文"`if(boot_image_priv->flags & SYS_PROC) {`"；§2.7 原文"`INIT_PROC_NR` | `\"init\"` | `USR_F`（`BILLABLE|PREEMPTIBLE`）"与"12 个系统服务 + NULL 哨兵"。
- 最小修复: 在 §1 Step 2 与 §2.3.4 要点写明 `SYS_PROC` 守卫；把 Step 3 的等待集合定义为"所有含 `SYS_PROC` 且非 `SF_SYNCH_BOOT` 的 boot 成员"；把"12 个系统服务"改为"11 个系统服务 + init 用户进程"。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 `USR_F` 也包含 `SYS_PROC`，则守卫对 INIT 不成立；但 §2.7 的展开式 `BILLABLE|PREEMPTIBLE` 没有该位。

## FINDING READER-C-06（P2）

- 位置: §2.7 sys 表与常量（第 549 行）、§1（第 45 行）、§2.3.4（第 412 行）、§5.2（第 1043 行）
- 严重度: P2
- 问题: §2.3.4 的分支条件是 `SF_SYNCH_BOOT`，但 §2.7 给出的 sys 表常量 `SRVR_SF = SF_CORE_SRV|SF_NEED_REPL`、`VM_SF = SRVR_SF`、`SRV_SF = SF_CORE_SRV`、MFS 为 0，没有任何条目带 `SF_SYNCH_BOOT`。按文档自己的表，没有服务是同步 boot 的；而 §1 又写"同步 boot 的服务立即收"，§5.2 三槽夹具的"计数 = 2"则隐含 PM 不同步。
- 影响: Step 2 的同步 catch 分支在文档给出的真实表下不可达，"同步立即收/其余累积"的分工没有可指名对象。读者无法判断该分支给谁用、Step 3 到底等谁。
- 证据或推理: §2.7 原文"`SRVR_SF = SF_CORE_SRV|SF_NEED_REPL`，`VM_SF = SRVR_SF`，`SRV_SF = SF_CORE_SRV`"；§2.3.4 原文"`SF_SYNCH_BOOT` 标志（minix3/minix/include/minix/rs.h:SF_SYNCH_BOOT）决定同步等待该服务的 init ready 还是累积到 Step 3 统一收"；§5.2 原文"VM/PM 各收一条 `RS_INIT`……计数 = 2"。
- 最小修复: 在 §2.7 sys 表加一列 `SF_SYNCH_BOOT`，或在 `SRVR_SF` 展开式中显式包含该位并列出走同步 catch 的服务；若真实表确实无同步项，删去 §1 的"同步立即收"并说明该分支只由测试夹具构造。
- 置信度: 中高
- 是否需源码核实: 否
- 若我错了: 若 C 的 `SRVR_SF` 实际含 `SF_SYNCH_BOOT`，则 §2.7 的常量展开漏了一位，修复方式同上，文档缺陷仍在。

## FINDING READER-C-07（P1）

- 位置: §3.4 第二段（第 736 行）、§2.3.5（第 424 行）、§5.2（第 1041 行）
- 严重度: P1
- 问题: 文档把"阻塞接收"等同于"fail-closed"。C 的 `while(nr_uncaught_init_srvs)` 是等待语义（正常路径会等到计数归零），Rust 在 12 未接线时直接 `Err(BootError::Kernel(ENOSYS))` 并由 `main.rs` panic。这是"等待"与"中止"的语义替换，属于对 C 行为的偏离，却没有 `ARCH` 标注，反而写成等价。
- 影响: Step 3 在文档里的因果角色（建立 boot 屏障）与 Rust 实际行为（boot 必然失败）不一致。读者会以为 Rust 的 `Err` 复刻了 C 的等待；真值优先序上这是单形态结论（"未接线时 fail-closed"）冒充普遍结论（"C 阻塞即 fail-closed"）。
- 证据或推理: §3.4 原文"Step 3 在计数 > 0 时同样 fail-closed（C 阻塞接收 = fail-closed，main.c:401-407）"；§2.3.5 原文"**这一步使 boot 严格串行**：所有 boot 服务初始化完成后，RS 才进入 Step 4"；§5.2 原文"`test_step3_fails_closed_when_init_ready_pending`……对应 main.c:401-407 的阻塞 receive 语义"。
- 最小修复: 改为"Rust 在 12 未接线期间以 `Err` 代替阻塞等待（临时 fail-closed；ARCH 偏离：C 等待、Rust 中止）"，删去"C 阻塞接收 = fail-closed"的等式，并在测试描述里标为临时行为。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 C 的 `catch_boot_init_ready` 遇错误会中止而非等待，那也不是"阻塞接收"本身的语义，等式仍不成立。

## FINDING READER-C-08（P1）

- 位置: §2.3.5（第 424 行）、§2.3.6（第 445 行）、§3.5 PmApi（第 769 行）
- 严重度: P1
- 问题: "Step 3 之后才进 Step 4"只给了结论，没给因果中项。Step 4 的 `getnpid` 是发往 PM 的 `_taskcall(PM_PROC_NR, ...)`，要求 PM 已完成初始化并能应答；文档从未说明 Step 4 依赖 PM（以及各服务在 PM 侧已登记）就绪，因而 Step 3 的必要性只能靠断言。
- 影响: 读者能背出"先收 ready 再查 pid"，但答不出"为什么不能先查 pid"。Step 3→Step 4 的依赖链缺一环，正是本章承诺解释的"为什么按这个顺序"。
- 证据或推理: §2.3.6 原文"**`getnpid`**（main.c:426）：向 PM 查询每个服务的 pid 存入 slot"；§3.5 原文"getnuid/getnpid/getprocnr → PM_GETEPINFO（lib/libsys/getepinfo.c:15-47）"；§2.3.5 原文"所有 boot 服务初始化完成后，RS 才进入 Step 4"。
- 最小修复: 在 §2.3.5/§2.3.6 补一句：`getnpid` 走 PM 消息面，PM 未 ready 则查询会失败或阻塞，故 Step 3 的等待是 Step 4 的前置条件。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 `getnpid` 实际不经 PM（直接读内核进程表），则中项不成立；但 §3.5 明写它归 PM 面。

## FINDING READER-C-09（P1）

- 位置: §1.2 失败对照（第 67 行）、§2.0 失败对照（第 108 行）、§2.3.3/§2.3.4 两个独立整表循环（第 290、368 行）
- 严重度: P1
- 问题: 给出的理由只证明"同一服务的属性必须先于它自己的放行"，无法证明"整表 Step 1 必须先于任何服务的 Step 2"。C 用两个独立整表循环，即便把建属性与放行合并成单循环也能满足该理由；文档没有解释相位级 barrier 的真实依据，却把它写成"不是风格选择，而是依赖顺序"。
- 影响: 本章的核心命题是"按什么顺序、为什么按这个顺序"；Step 1/Step 2 的分相理由停留在单槽级别，相位级因果其实是空的。读者会把一个实现选择误当依赖顺序。
- 证据或推理: §1.2 原文"如果跳过 Step 1 的本地属性准备、直接进入 Step 2 放行，`sys_privctl(SYS_PRIV_ALLOW)` 放行的将是一个没有 priv/send-mask/调度参数的槽"；§2.0 原文"所以“先本地建表、再统一放行”不是风格选择，而是依赖顺序"；§2.3.3 与 §2.3.4 分别是两个从头遍历 priv 表的循环。
- 最小修复: 补 C 层相位级理由（全表 priv 一致后才允许任何成员运行、或 rproctab grant 需覆盖全表、或计数/授权完整性），否则承认这是实现选择并删去"不是风格选择"。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 C 注释或设计契约给出相位级理由，引用即可；"文档未给出"仍成立。

## FINDING READER-C-10（P1）

- 位置: §2.3.3 代码与要点（第 320、359 行）、§4.4 step1（第 956 行）、§5.2 顺序测试（第 1023 行）
- 严重度: P1
- 问题: C 的 `sys_getpriv` 写在 RS/VM 例外 if 之外，对全部 12 项无条件调用；§4.4 却写"其余经 `privctl(SET_SYS)` + `getpriv`"，暗示 RS/VM 不查；§5.2 的调用序列"step1 privctl(SetSys)×10"完全不提 `getpriv`。三处对同一机制的描述不一致。
- 影响: 读者无法确定 Rust Step 1 是否对 RS/VM 回读 priv。若设计真的跳过，RS/VM 的 `r_priv` 与内核侧来源不一致（C 在 294 行正是靠回读保证一致）；若只是文字遗漏，则 §5.2 缺一个应断言的调用（`getpriv`×12）。这是 Step 1"属性就绪"含义的关键分歧。
- 证据或推理: §2.3.3 原文`if ((s = sys_getpriv(&(rp->r_priv), ip->endpoint)) != OK)`（位于例外 if 之后，缩进同级）；§4.4 原文"其余经 `privctl(SET_SYS)` + `getpriv`"；§5.2 原文"step1 privctl(SetSys)×10（RS/VM 跳过）→ step2 sched×10+Allow×10"。
- 最小修复: 统一为"C 对所有 12 项调用 `sys_getpriv`，Rust 同"，或在 §3/§4 明示跳过 RS/VM 并说明其本地 priv 的来源；§5.2 序列补 `getpriv` 计数。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 Rust 实现确实 12 次而仅文字漏写，则只需改文字；三处不一致仍属文档缺陷。

## FINDING READER-C-11（P1）

- 位置: §1 Step 4（第 46 行）、§2.3.7（第 483 行）、§3.4 编排（第 706-711 行）、§4.4（第 954、960 行）、§1 覆盖声明（第 7 行）
- 严重度: P1
- 问题: §1 说 Step 4 之后"进入主循环"，但 §2.3.7 的 C 代码位于 `sef_cb_init_fresh` 内、在 Step 4 之后、"返回主循环"之前，且 `USE_LIVEUPDATE` 默认为 yes（§3.6 自述）。按 C 默认，boot 实际是"step0 + 四步 + 自升级"五段。Rust 侧 §3.4 的编排只有 step0..step4 后 `Ok(())`，§4.4 虽列 `self_update` 却没有调用点；同时 §4.4 还宣称"机制步骤一一对应（无增减）"。
- 影响: 四步模型在 Step 4 处截断，自升级的"结果"（副本存在、旧实例让出）无法落到 boot 链上；而 Rust 默认关 feature 又让 boot 结果与 C 默认不同。读者既看不到第五段，也看不到偏离声明；"无增减"的断言与事实不符。
- 证据或推理: §1 原文"并设置周期检查闹钟（`sys_setalarm(RS_DELTA_T)`），之后进入主循环"；§2.3.7 原文"**语义**：boot 完成后，RS 立即 fork 一个自身副本（`srv_fork(0,0)` 经 PM，自升级复制）"；§3.4 原文"self.step4_finish(sys)?;       // main.c:409-433" 后紧接"Ok(())"；§4.4 原文"机制步骤一一对应（无增减）"。
- 最小修复: §1 四步后补"第五段（编译期可选）：自升级"；§3.4 `init_fresh` 末尾显式加 `#[cfg(feature = "live-update")] self_update(sys)?`，或写明由 `main.rs` 在 init 之后调用，并声明 Rust 默认关与 C 默认 yes 的 boot 结果差异。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 18 的接线计划明确由 `main.rs` 调用 `self_update`，给出行号/契约即可；当前三处都没有该调用点。

## FINDING READER-C-12（P1）

- 位置: §2.3.7 代码与语义段（第 466-483 行）、§3.6（第 820 行）、§3.6 签名（第 816 行）
- 严重度: P1
- 问题: C 的自升级按 `srv_fork` 返回值分两支（`if(pid == 0)` 新实例、`else` 旧实例 + `NOT_REACHABLE`），§3.6 却把全部调用平铺成"一条链"：`update_service`/`cpf_reload`/`cleanup_service`/`vm_memctl pin` 与 `privctl SET_SYS`+`sched_init_proc`+`YIELD` 并列。按字面，同一进程会执行两个互斥分支。§3.6 的签名 `Result<(), Errno>` 还允许旧实例从 `self_update` 返回并继续 `init_fresh`→主循环，而 C 的旧实例分支是 `NOT_REACHABLE`。
- 影响: feature 打开后，"谁接管、谁不得继续 boot"这个自升级成立的前提没有写进设计；平铺链会让新 RS 给自己 `YIELD` 或旧 RS 清理自己。启动链的终点（哪个实例进入主循环）无法从文档判定。
- 证据或推理: §2.3.7 原文"`if(pid == 0) {`……`else {`……`NOT_REACHABLE;`"；§3.6 原文"`self_update` 内部按 §2.3.7 调用链编排（`clone_slot`/`srv_fork`/`update_service`/`cpf_reload`/`cleanup_service`/`vm_memctl pin` / `privctl SET_SYS`+`sched_init_proc`+`YIELD`）"。
- 最小修复: 把调用链按 `srv_fork` 返回值写成两段互斥分支；规定父分支在 `YIELD` 之后不得返回（返回即不变式违例），或改用不返回的类型表达。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 `update_service` 被设计成两方幂等调用，需写明；§2.3.7 抄录的 C 代码不是这样。

## FINDING READER-C-13（P2）

- 位置: §3.4 N3 引注（第 726-731 行）、§2.1（第 122-124 行）、§4.4 step0 清单（第 955 行）
- 严重度: P2
- 问题: N3 声称 `step0_prepare` 的 `get_machine` "对齐 C main.c:53 的位置——`sef_local_startup` 之后、主循环之前"。但 C 的 `sys_getmachine` 在 `sef_local_startup()` 返回之后执行，而 `sef_local_startup` 内部才是 `sef_startup()`→fresh boot；Rust 把它放进 step0，等于在 Step 1 之前，与 C 的位置正好相反（C 在 boot 之后取快照）。§4.4 的 step0 清单也没有列 `get_machine`。
- 影响: "对齐位置"掩盖了一次调用前移；若读者据此认为两者时序一致，就无法评估 `Machine` 快照是否可能在 boot 期失效。step0 的三处描述（§3.4 代码注释、§3.4 N3、§4.4）只有一处提到它。
- 证据或推理: §3.4 N3 原文"`step0_prepare` 在启动期一次性 `sys.get_machine()?`（对齐 C main.c:53 的位置——`sef_local_startup` 之后、主循环之前）"；§2.1 原文`sef_local_startup();` 之后紧接`if (OK != (s=sys_getmachine(&machine)))`；§4.4 原文"`step0_prepare`：`env_parse` 配置注入（参数）、`get_hz`、`RUPDATE_INIT` 等价……"未含 `get_machine`。
- 最小修复: 改写为"位置移动（ARCH：C 在 fresh boot 返回后取快照，Rust 在 step0 取；理由是快照值在 boot 期不变）"，并在 step0 清单补上 `get_machine`。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 C 的 `sef_local_startup` 只在注册回调后立即返回、fresh boot 不在其内，则"C 在 boot 后取"需要重述；但它仍晚于 Rust 的 step0，仍是前移。

## FINDING READER-C-14（P2）

- 位置: §2.3.2 要点（第 284 行）、§2.3.1（第 256 行）
- 严重度: P2
- 问题: "C 源码分析"节混入 Rust 专有构造：`validate_tables`、`boot_img` 的 `len.min(16)` 钳制、`placeholder` 都不是 C 符号，没有"Rust 侧新增（C 无对应）"标注。同一段还把 `main.c:196` 同时当作 `sys_getimage` 调用点和"内核 boot 表端点由 `_ENDPOINT(0, proc_nr)` 派生"的依据。
- 影响: 读者会把 Rust 的启动期校验和名称钳制误当 C 行为，违背真值优先序；`main.c:196` 的双重归属无法同时成立（端点派生属于内核建表，不在 RS 的 `main.c`）。
- 证据或推理: §2.3.2 原文"`validate_tables` 另校验每行 `endpoint.slot() == proc_nr`——内核 boot 表端点由 `_ENDPOINT(0, proc_nr)` 派生（main.c:196），手写 placeholder 若错位会 boot 错端点；`boot_img` 的 16 字节名钳制（`len.min(16)` 的 const 求值版本）"；§2.3.1 原文"**`sys_getimage(image)`**（main.c:196）"。
- 最小修复: 把这两句移到 §4.4 或标注"Rust 侧新增（C 无对应）"；修正第二个 `main.c:196` 引用（给出内核侧行号或删去）。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 C 里确有同名校验/钳制，只需标明来源；两个引用仍不能都落在一行。

## FINDING READER-C-15（P2）

- 位置: §2.3.1（第 256 行）、§2.7 表语义（第 569 行）、§3.1 标题（第 615 行）
- 严重度: P2
- 问题: "表静态化"被同时用于两件相反的事：§2.3.1 把 `sys_getimage`（运行期拷贝）称为"表静态化的关键"；§2.7 又把 RS 自带的三张静态表称为"静态化的基础"；§3.1 在一个 ARCH 标签下同时收进运行期 `&[BootImage]` 与编译期三表。运行期拷贝不可能"静态化"。
- 影响: `sys_getimage` 与三张静态表的关系被术语掩盖：前者是运行期注入，后者是编译期常量，二者只通过 endpoint 查找与计数核对发生绑定。按原文，读者无法确定哪个才是 ARCH 的对象。
- 证据或推理: §2.3.1 原文"**这是 表静态化 的关键**：boot 映像来源在 minix-rs 中如何建模见 §3.1"；§3.1 原文"boot 表来源显式化：BootTables（对应 §2.3.1/§2.7，ARCH 表静态化）"；§2.7 原文"这是 表静态化 的静态化基础（§3.1）"。
- 最小修复: §2.3.1 那句改为"这是 boot 表建模的输入点"；"表静态化"只用于三张静态表，并补一句分工：内核拷贝提供 endpoint/name，RS 三表提供 priv/sys/dev 语义，靠 endpoint 对齐 + 计数核对绑定。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若作者本意是"运行期表也按静态表方式建模"，需明说该词有两种用法；术语冲突仍在。

## FINDING READER-C-16（P2）

- 位置: §2.2 末句（第 220 行）、§2.3.1 `env_parse` 要点（第 252 行）、§4.4（第 955 行）
- 严重度: P2
- 问题: 两处悬空引用。§2.2 说"7 个回调的机制归属见 §1.3 表"，但 §1.3 只有小结与术语、没有表；实际的方法归属表在 §3.3。§2.3.1 说 `rs_verbose` 的 Rust 建模是"cargo feature 或 env 注入（feature 门控，见 §3.6）"，而 §3.6 只讲 `live-update`，§4.4 给出的实际做法是"参数注入"。
- 影响: 读者按引用找不到映射；`rs_verbose` 的建模在"feature 门控"与"参数注入"之间摇摆，读者无法确定该配置的 Rust 载体。
- 证据或推理: §2.2 原文"7 个回调的机制归属见 §1.3 表"；§2.3.1 原文"Rust 侧构建配置宏 → cargo feature 或 env 注入（feature 门控，见 §3.6）"；§4.4 原文"`env_parse` 配置注入（参数）"。
- 最小修复: §2.2 引用改指 §3.3（trait 方法的 `→ 18/12/06` 注释）；§2.3.1 改指 §4.4，并把"feature 门控"改为"参数注入"。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 §1.3 在别的版本里有这张表，当前文本确实没有，按当前版本成立。

## FINDING READER-C-17（P2）

- 位置: §1 Step 3（第 45 行）、§2.3.5（第 424 行）、§3.4 引注（第 733-735 行）
- 严重度: P2
- 问题: §1 把"同步 boot 的服务立即收"写进只标 `main.c:401-407` 的 Step 3，但同步 catch 位于 Step 2 的循环内（§3.4 引注给出 `main.c:390-392`）。同时 §2.3.5 把"使 boot 严格串行"单归 Step 3，而串行化的实质是 Step 2 的同步 catch 与 Step 3 的等待共同建立。
- 影响: 步骤边界与行号错位，读者会把 Step 2 的行为记到 Step 3；"严格串行"的因果归属错误，导致 Step 2/Step 3 的分工被误解。
- 证据或推理: §1 原文"Step 3（`main.c:401-407`）——收齐初始化完成：阻塞接收全部服务的 `RS_INIT` 回复（同步 boot 的服务立即收，其余累积后统一收）"；§2.3.5 原文"**这一步使 boot 严格串行**：所有 boot 服务初始化完成后，RS 才进入 Step 4"；§3.4 原文"（C 是同步 `catch_boot_init_ready`，main.c:390-392）"。
- 最小修复: §1 Step 3 改为"收齐 Step 2 未同步捕获的剩余 ready"；串行化描述改为"Step 2 同步 catch + Step 3 等待共同构成 barrier"。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 `main.c:239-243` 的四步注释把同步 catch 划进 Step 3，则按注释改行号；代码位置仍在 Step 2 循环内。

## FINDING READER-C-18（P2）

- 位置: §5 章首（第 992 行）、§5.2 表（第 1023-1046 行）、§5.3 表（第 1052 行）、§5.4（第 1063 行）
- 严重度: P2
- 问题: 52 的算术自洽（39+3+2+8），但分项无法由正文核对：§5.2 表实际只列出 26 个 boot 测试名，§5.3 只列 1 个 sef 测试名（声称 sef 2）。§5.4 说"全部测试可 grep 验证：`rg -c \"#[test]\"` 全 crate 合计 359"，而 `rg -c` 逐文件输出计数、不产生合计；同段又提到 minix-types 的 1 项，基线命令却是 `-p minix-rs --lib`，跨 crate 不在同一范围。
- 影响: 359/0 与"01 范围 52 项"之间的分项关系不可审计；读者无法从文档核对 boot 39 与 sef 2 从何而来，"全 crate"与单包 `--lib` 的口径也不一致。
- 证据或推理: §5 原文"01 范围四模块 52 项（boot 39 + table 3 + sef 2 + dispatch 8）"；§5.4 原文"全部测试可 grep 验证：`rg -c \"#[test]\"` 全 crate 合计 359。01 范围四模块共 52 项：boot.rs 39、table.rs 3、sef.rs 2、dispatch.rs 8"；§5.3 仅列 `test_deferred_callbacks_fail_closed` 一个 sef 测试名。
- 最小修复: 给出可复现的逐文件计数命令与汇总输出（或贴 `cargo test` 输出），把 §5.2 表补全或标明"节选"并列出 39−26 的差集；minix-types 那 1 项标明"不计入 359"。
- 置信度: 高（26 与 1 为按正文表逐行计数，斜杠并列名按两个计）
- 是否需源码核实: 否
- 若我错了: 若 §5.2 表本就是节选，补一句"节选，完整 39 项见代码"即可；但"全部测试可 grep 验证"仍不成立。

## FINDING READER-C-19（P2）

- 位置: §3.4（第 692 行）、§3.4 第二段（第 736-737 行）、§5.2（第 1023 行）
- 严重度: P2
- 问题: §3.4 声称"顺序不可重排由编排方法保证"，同一页又写"boot 测试改走私有 step 方法直接驱动各步"。若测试都绕过 `init_fresh`，这个核心不变式没有任何测试覆盖；名为 `test_init_fresh_step_order` 的测试与实际"step 直调"驱动方式也相互矛盾。
- 影响: 本章最看重的"四步不可乱序"只剩设计断言，没有可执行的负控。读者会以为顺序被测过。
- 证据或推理: §3.4 原文"**顺序不可重排由编排方法保证**"；§3.4 原文"boot 测试改走私有 step 方法直接驱动各步"；§5.2 原文"`test_init_fresh_step_order` | `MockKernelApi` 记录调用序列"。
- 最小修复: 至少保留一个测试走 `init_fresh` 公共入口并断言四步次序；其余可直调 step，并把"boot 测试"限定为"除顺序测试外的 boot 测试"。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 `test_init_fresh_step_order` 确实调用 `init_fresh`，只需收窄 §3.4 那句话的范围；矛盾仍是文字层面的。

## FINDING READER-C-20（P2）

- 位置: §3.5 开头与 SysApi 注释（第 752-760 行）、§3.7（第 826 行）、§4.1（第 887 行）
- 严重度: P2
- 问题: §3.5 声称把 §2 各调用点建模为五个窄域面、`KernelApi` 是并集；但 `sys_getimage` 在 §4.1 走 `BootTables::acquire_from(&minix_sys::syscall::DirectKernelCallTransport)` 直连内核、绕过五面，而 §3.7 说 `grant_read` 在 `SysApi` 面上，SysApi 注释却没列它。于是"全部外部边界都在 KernelApi"与实现不符。
- 影响: boot 对内核的取表依赖落在可注入边界之外，测试无法用 `MockKernelApi` 脚本化 GET_IMAGE 失败；读者也无法判断 `sys_getimage` 归哪个面。
- 证据或推理: §3.5 原文"把这四个传输目标加自身 IPC 建模为五个窄域面，`KernelApi` 是它们的并集；生产入口在 `main.rs` 用 `TrapKernelApi`（直接陷入内核）组装这些域面"；§4.1 原文"`BootTables::acquire_from(&minix_sys::syscall::DirectKernelCallTransport)`"；§3.7 原文"`step0_prepare` 经 `KernelApi::grant_read`（`SysApi` 面，实现在 `trap_api::TrapKernelApi`）"。
- 最小修复: 在 SysApi 注释补 `grant_read`/`get_image`，或增设 `ImageApi` 面并说明 `acquire_from` 为何绕过 `KernelApi`；在 §3.5 的覆盖声明里排除 `acquire_from`。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 `acquire_from` 内部最终转调 `KernelApi`，则 §4.1 的写法改掉；两处必须一致。

## FINDING READER-C-21（P2）

- 位置: §3.5 fail-closed 条（第 807 行）、§4.1（第 894 行）、§5.2（第 1033 行）
- 严重度: P2
- 问题: §3.5 用"返回 `Err` 让缺口在调用点可见且进程存活"为占位 `KernelApi` 辩护，但 boot 路径上任一 `ENOSYS` 都会让 `server.init` 失败并触发 `main.rs` 的 `panic!`，正是它想避免的结局。"进程存活"只在 boot 完成后的调用点成立，文档没有界定这个适用范围。
- 影响: 读者会以为生产占位能保证 RS 起得来；实际上 boot 期占位 = 必 panic。fail-closed 的设计收益被高估，且与 §4.1 的致命启动策略在措辞上相互抵消。
- 证据或推理: §3.5 原文"返回 `Err` 让缺口在调用点可见且进程存活"；§4.1 原文"if let Err(e) = server.init(SefInitType::Fresh) { panic!"；§5.2 原文"`test_unimplemented_kernel_api_fails_closed` | 生产占位 `KernelApi` 全接口 fail-closed（`Err(ENOSYS)`）"。
- 最小修复: 把"进程存活"限定为"boot 完成后的调用点"；明确 boot 期占位即 panic（与 C 的 boot panic 同结局），并把这条写进 §4.1 的注释。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若入口在 `ENOSYS` 下有降级路径（例如继续进主循环），则 §4.1 的 panic 与之冲突，两处仍需统一。

## FINDING READER-C-22（P2）

- 位置: §2.7 表语义（第 531 行）、§1.1（第 33 行）
- 严重度: P2
- 问题: §2.7 说 priv 表"顺序即 boot 启动顺序"，§1.1 又说"真正决定谁先跑的是 kmain 的可调度序"，且第一个能跑的用户服务是 VM、RS 第二；而 priv 表第 1 项是 RS。两套"顺序"（表序、可调度序、放行序）没有对齐说明。
- 影响: 读者会把 Step 2 的遍历次序当成内核实际调度次序，从而误判"RS 为什么能给别人授权而自己还没被授权"这一自举解法的关键。
- 证据或推理: §2.7 原文"boot_image_priv_table（table.c:15-30，12 个系统服务 + NULL 哨兵；顺序即 boot 启动顺序）"；§1.1 原文"除内核任务外，第一个能运行的用户服务是 **VM**，RS 作为 root system process 紧随其后"。
- 最小修复: §2.7 改为"顺序即 RS 的放行/授权顺序（Step 2 遍历序），不是内核可调度序"。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若表序确实决定实际启动次序，则 §1.1 的"可调度序"说法需要收窄；两者必有一处需改。

## FINDING READER-C-23（P2）

- 位置: §3.4 错误域二分引注（第 739-744 行）、§2.3.1 代码（第 239、248 行）
- 严重度: P2
- 问题: §3.4 写"boot 的失败在 C 里只有一种结局——`panic()`"，紧跟的枚举只给 4 个站点（计数不符、拿不到 pid、两处查表未命中）。§2 自己的代码块还列出 grant 创建（185-189）、`sys_getimage`（196）、`SET_SYS`（316-318）、`getpriv`（320-322）、`sched`/`ALLOW`（389-394）、`init_service`（396-398）、`setalarm`（433-434）等 panic 点。
- 影响: 错误域二分的 C 依据被削弱。读者会以为其余 syscall 失败有别的处理，而 §5.2 恰有 `setalarm` 与 `get_machine` 失败测试。
- 证据或推理: §3.4 原文"boot 的失败在 C 里只有一种结局——`panic()`（main.c:226 计数不符、main.c:427-429 拿不到 pid、main.c:731/746 查表未命中）"；§2.3.1 原文"panic(\"unable to create rprocpub table grant: %d\", rinit.rproctab_gid)"与"panic(\"unable to get copy of boot image table: %d\", s)"。
- 最小修复: 把枚举改为"包括但不限于"，或按 §2 的 panic 点补全列表。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若括号本意只是举例，加"例如"即可；"只有一种结局"仍成立。

## FINDING READER-C-24（P2）

- 位置: §2.3.3 要点（第 357 行）、§2.4 要点（第 525 行）、§2.4 标题（第 485 行）
- 严重度: P2
- 问题: §2.3.3 说 `boot_image_info_lookup` "一次调用同时查找 image/priv/sys/dev 四张表（§2.4）"，并把函数锚在 `main.c:253-254`；§2.4 明确 Step 1 传 `NULL` 给 priv 参数、"只查 image/sys/dev 三表"，函数本体在 `main.c:709-779`。相邻两节的调用关系与行号互斥。
- 影响: 这是直接影响"Sys_getimage 与三张表关系"的分歧：priv 表到底经查找还是经下标直接索引，文档给了两个答案。读者无法确定 lookup 的输入输出契约，也无法理解 §3.2 为何要保留 `lookup_priv`。
- 证据或推理: §2.3.3 原文"**`boot_image_info_lookup`**（main.c:253-254）：一次调用同时查找 image/priv/sys/dev 四张表（§2.4）"；§2.4 原文"Step 1 调用时传 `&ip, NULL, &boot_image_sys, &boot_image_dev`（main.c:253-254）——只查 image/sys/dev 三表，priv 表条目由外层循环直接索引"。
- 最小修复: §2.3.3 改为"查找 image/sys/dev 三表；priv 表不经 lookup"；把函数锚点改为 `main.c:709-779`，`253-254` 保留为调用点。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 C 的 Step 1 实际传了 priv 指针，则 §2.4 的要点错；两处必须统一。

## FINDING READER-C-25（P2）

- 位置: §1.1（第 33 行）
- 严重度: P2
- 问题: 同一段先按登记序说"DS 摆在第 1 个用户服务、RS 紧随其后（第 2）"，随后说"真实的 NOTIFY 挑选序按 proc 号升序扫描，DS 反而排在第七"。若 proc 号随 boot 映像模块序分配（文档自己的"登记序"叙述），DS 应最先被扫描；要得到"第七"，必须有"proc 号另行分配"的中间步骤。该断言也没有给出内核代码锚点。
- 影响: 这是 §1.1 的核心论证（内核表头注释是空话）。中间步骤缺失，断言无法复核；读者无法判断 NOTIFY 的选择规则是否真的与表序无关。
- 证据或推理: §1.1 原文"它把 DS 摆在第 1 个用户服务、RS 紧随其后（第 2），而这张序由 GRUB 交来的模块顺序硬性决定"；同段原文"真实的 NOTIFY 挑选序按 proc 号升序扫描，DS 反而排在第七"。
- 最小修复: 补 proc 号分配规则与 NOTIFY 扫描的 C 文件/行号；或删去"第七"，只保留"按 proc 号升序、与数组序无关"这一结论。
- 置信度: 中
- 是否需源码核实: 否
- 若我错了: 若 proc 号确实独立于模块序分配，给出一句规则即可；当前缺口是"未解释"而非"必错"。

## FINDING READER-C-26（P2）

- 位置: §2.3.2 要点（第 284 行）、§2.7 表语义（第 569 行）、§5.2（第 1039 行）
- 严重度: P2
- 问题: 文档把 `nr_image_srvs != nr_image_priv_srvs` 的计数核对称为"这是'RS 的表与内核的表一致'的启动协议完整性检查"，§2.7 又说 priv 表"条目数必须与内核 boot_image 表系统服务数一致"。计数相等只证明非内核条目**数量**相等；端点集合不同但数量相同的表仍能通过 Step 0。真正的端点集合一致性要到 Step 1 的 `boot_image_info_lookup` 未命中 panic 才建立。
- 影响: 因果链少了"计数核对只查基数，集合对齐在 Step 1"这一步。读者会以为 Step 0 就能发现所有表不一致，低估"同数错位"的失败位置；这也直接影响对 `sys_getimage` 拷贝与三张静态表如何绑定的理解。
- 证据或推理: §2.3.2 原文"boot 映像表中系统服务数必须等于 RS 自身 priv 表中系统服务数——这是“RS 的表与内核的表一致”的启动协议完整性检查"；§5.2 原文"`test_step1_lookup_failure_propagates` | 计数一致但成员错位的表 → step1 `Err(BootError::Lookup(ImageTable))`"；§2.4 原文"image 表 / priv 表：未命中 → `panic`（启动协议错误，fail-closed）"。
- 最小修复: 改为"Step 0 的计数核对只保证非内核条目数量相等；端点集合的一致性由 Step 1 的查找未命中 panic 保证，两者合起来才是表对齐"。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 Rust 的 `validate_tables` 在计数之外还校验完整端点集合（文档只提到 `endpoint.slot()==proc_nr` 和计数），则 Rust 的 Step 0 比 C 强；但把它归因给"C 的计数核对"仍过强。

## FINDING READER-C-27（P2）

- 位置: §3.4 编排注释（第 706 行）、§3.7（第 826 行）、§4.4 step0（第 955 行）
- 严重度: P2
- 问题: step0 的内部次序有三套说法。§3.4 的注释按 C 行号列出"配置/HZ/grant/复位/计数核对/表重置"（grant 在表重置之前）；§3.7 说"表重置先于授权，因为 Rust 授权钉的是镜像地址"；§4.4 说"计数核对 → 表重置 → rproctab 授权创建"。C 的实际次序是 grant（185）在前、表重置（230-237）在后；Rust 反转为 reset 在前、grant 在后。这是一次未标 ARCH 的次序反转，而三处描述互不一致。
- 影响: 读者无法确定 `rproctab_gid` 在何时覆盖哪个地址；若表重置涉及重新分配而非原地清零，授权可能指向失效地址。自举链的第一段（Step 0）因此不可复核。
- 证据或推理: §3.4 原文"self.step0_prepare(sys)?;      // main.c:178-237（前置：配置/HZ/grant/复位/计数核对/表重置）"；§3.7 原文"表重置先于授权，因为 Rust 授权钉的是镜像地址，行内容由 step1 的 `sync_pub_wire` 填充"；§4.4 原文"计数核对（`validate_tables`，对应 main.c:225-227）→ 表重置（`slots` 重建，对应 main.c:230-237）→ rproctab 授权创建（§3.7，main.c:185-189；Rust 侧表重置先行……）"。
- 最小修复: 统一写成 Rust 的实序（计数核对 → 表重置 → 授权创建），并标 `ARCH`：C 先授权后复位，Rust 先复位后授权，理由是授权钉地址；删去 §3.4 注释里照抄 C 的次序。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 Rust 的 `RProcTable` 是固定地址的静态数组、复位只是原地清零，那么两种次序等价，次序反转无风险；但文档仍需统一三处描述。

## FINDING READER-C-28（P3）

- 位置: §4.2 `lib.rs` 模块列表（第 912-918 行）
- 严重度: P3
- 问题: "其余 pub mod（…18 个名字…）为 02-19 对应模块的 forward-declare，共 22 个"：括号内实际 18 个名字，加上前文 4 个模块才是 22；"共 22 个"紧跟在"其余"后，字面读成 18 个之外还有 22 个。
- 影响: 读者核对模块总数时会算错，也无法确认是否有 4 个模块漏列。
- 证据或推理: §4.2 原文"其余 pub mod（access/exec/ipc_mask/live_update/monitor/privilege/process_table/publish/query/ready/recovery/request/sched/self_lifecycle/service_create/service_slot/slot/state_data）为 02-19 对应模块的 forward-declare，共 22 个"；同段前文列出"pub mod table; … pub mod dispatch;"四个模块。
- 最小修复: 改为"共 18 个（加上前 4 个模块合计 22 个）"。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若有 4 个未列出的模块，补齐名单即可。

## FINDING READER-C-29（P3）

- 位置: §3.7（第 826 行）、§7 参见末条（第 1112 行）
- 严重度: P3
- 问题: §7 说 `os/libs/minix-sys/src/lib.rs` 是 syscall 库、"`KernelApi` 的生产实现由 `TrapKernelApi` 提供"，读起来把 `TrapKernelApi` 放在 minix-sys 的 lib.rs；§3.7 说它实现在 `trap_api::TrapKernelApi`，即本服务器 crate 的模块。
- 影响: 读者按 §7 找错文件；生产实现与底层 syscall 库的边界被混淆。
- 证据或推理: §7 原文"`os/libs/minix-sys/src/lib.rs` — syscall 库；`KernelApi` 的生产实现由 `TrapKernelApi` 提供"；§3.7 原文"实现在 `trap_api::TrapKernelApi` → `minix_sys::grant::GrantTable::grant_direct`"。
- 最小修复: §7 改为"`TrapKernelApi` 在 `os/servers/rs/src/trap_api.rs`，底层调用 `minix-sys`"。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 `TrapKernelApi` 真在 minix-sys 里，§3.7 的路径写错，两处仍需统一。

## FINDING READER-C-30（P3）

- 位置: §4.4 四查找函数（第 950 行）、§2.4 要点（第 525 行）、§5.2（第 1030 行）
- 严重度: P3
- 问题: `lookup_priv` 被列为四个设计查找函数之一并有命中/未命中两个测试，但 Step 1 传 `NULL` 给 priv 参数、priv 表按下标直接索引，文档没有给出 `lookup_priv` 的任何生产调用点。
- 影响: 读者会以为 priv 表在 boot 路径上经 lookup 访问，与 §2.3.3 的下标访问模型冲突；该函数也有死代码嫌疑。
- 证据或推理: §4.4 原文"`LookupError` + 四个查找函数：§3.2（`lookup_image` 对应 main.c:723-733，`lookup_priv` 对应 main.c:738-748，……）"；§2.4 原文"只查 image/sys/dev 三表，priv 表条目由外层循环直接索引（`rp = &rproc[boot_image_priv - boot_image_priv_table]`）"。
- 最小修复: 说明 `lookup_priv` 的唯一用途（例如 `validate_tables` 复用）或标为仅测试/预留。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 `validate_tables` 确实调用它，写明调用点即可。

## FINDING READER-C-31（P3）

- 位置: §2.8 小结（第 579 行）、§2.4 要点（第 524 行）
- 严重度: P3
- 问题: §2.8 写"任何表数/端点不一致都在 boot 期 panic，不留到运行期"，而 §2.4 明确 sys/dev 表未命中是"正常"的、走默认条目。字面上"任何……不一致"覆盖了查找未命中，与默认兜底冲突。
- 影响: 读者会把 sys/dev 未命中误记为 boot panic 点，错误域二分的"正常路径"与"panic 路径"边界被抹平。
- 证据或推理: §2.8 原文"任何表数/端点不一致都在 boot 期 panic，不留到运行期"；§2.4 原文"sys/dev 未命中是**正常**的（服务无覆盖配置），采用默认条目"。
- 最小修复: 改为"计数不符与 `endpoint.slot` 错位在 boot 期 panic；sys/dev 查找未命中走默认条目"。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若"表数/端点不一致"专指 §2.3.2 的两类检查，加"（计数与 slot）"即可消歧。

## FINDING READER-C-32（P3）

- 位置: §3.3 末段（第 681 行）、§3.5 fail-closed 条（第 807 行）、§3.6 签名（第 816 行）
- 严重度: P3
- 问题: 同一文档既规定 RS"不 panic"（因 root sys process，panic = 整机不可用），又在 `signal_handler` 保留 `unimplemented!`（panic）；四步 boot 用 `BootError`，`self_update` 却用 `Errno`，"错误域二分"的契约在 feature 路径上不成立。
- 影响: fail-closed 契约在细节上自相抵消：读者无法判断 panic 到底被禁止还是被允许，也无法判断错误分类是否适用于自升级路径。
- 证据或推理: §3.3 原文"`signal_handler` 保留 `unimplemented!`（无 Result 通道，契约归 06）"；§3.5 原文"**不 panic**——RS 是 root system process，panic = 整机不可用（内核不重启 RS，`RSYS_F`）"；§3.6 原文"fn self_update(&mut self, sys: &mut dyn KernelApi) -> Result<(), Errno>"。
- 最小修复: 为 `signal_handler` 定义不 panic 的降级路径，或显式标注这是唯一允许的 panic 点及理由；`self_update` 统一用 `BootError`（或说明为何保留 `Errno`）。
- 置信度: 高
- 是否需源码核实: 否
- 若我错了: 若 06 的接线会替换 `signal_handler`，把"临时代码"标注清楚即可；错误类型分裂仍需说明。

## SUMMARY

计数：P0 = 1（RC-02）；P1 = 10（RC-01、RC-03、RC-04、RC-05、RC-07、RC-08、RC-09、RC-10、RC-11、RC-12）；P2 = 16（RC-06、RC-13 至 RC-27）；P3 = 5（RC-28 至 RC-32）；合计 32 条。

最严重 3 条：
1. RC-02：Step 1 被反复写成"纯本地、不向内核提交"，与同书 §2.3.3 的 `SYS_PRIV_SET_SYS`/`sys_getpriv` 直接冲突，且例外括号写反；"先本地建表、再统一放行"的因果表述因此失真。
2. RC-11：四步模型在 Step 4 截断，丢掉了 C 默认开启的自升级第五段；`self_update` 在 Rust 编排里没有调用点，"机制步骤一一对应（无增减）"不成立。
3. RC-07：把 C 的阻塞接收等价为 fail-closed，Rust 的 `Err(ENOSYS)` 实为"等待→中止"的语义替换，未标 ARCH；Step 3 的因果角色与实现行为不一致。

最影响因果理解 3 处：
1. RC-02：Step 1 的真实内核动作被抹掉，导致 Step 1 与 Step 2 的依赖关系整段错位。
2. RC-04：`RS_INIT` 双向未区分，Step 2 的"发"与 Step 3 的"收"无法连成一条协议，中间缺了服务的初始化回执环。
3. RC-09：Step 1 必须先于 Step 2 的相位级理由只有单槽级论证，本章承诺的"为什么按这个顺序"在最关键的一环没有依据。
