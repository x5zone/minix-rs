# DESIGN-D2 r1：01-rs-boot-init 六项定点设计核对

核对对象：`rewrite-notes/03-stage-rs/01-rs-boot-init.md`（1114 行）
真值：C `minix3/minix/servers/rs/{main.c,table.c,const.h}`、`minix3/minix/lib/libsys/sef.c`、`minix3/minix/include/minix/sef.h`、`minix3/share/mk/bsd.own.mk`；Rust `os/servers/rs/src/{boot,sef,lib,table,process_table,trap_api,main}.rs`、`os/servers/rs/Cargo.toml`。
方式：只读，六项定点核对，不做全篇 review。

## 定点核对

| # | 项目 | 结果 | 证据（文件:行） |
|---|------|------|------------------|
| 1 | `BootTables` 与 C `sys_getimage` + table.c 三表的职责对应 | PASS | `boot.rs:554-563` 四字段 `image/priv_table/sys_table/dev_table`，`boot.rs:565-574` 把三张 RS 自有表绑为 `'static`；`boot.rs:600-620` `acquire_from` 用内核 `GET_IMAGE` 填 17 行 image（`boot.rs:603-614`），失败返回 errno、由 `main.rs:62-63` 转 panic；`table.rs:96-176` priv 12 项（MFS label=`fs_imgrd`）、`table.rs:159-190` sys 6 项 + `DEFAULT_SYS`、`table.rs:197-212` dev 2 项 + `DEFAULT_DEV`，与 `table.c:13-50` 同序同值。C 的 image 是运行期拷贝（`main.c:196`），三表是 RS 静态表（`table.c`），Rust 的职责切分一致；`placeholder()` 仅测试/宿主（`boot.rs:576-585`），生产走 `acquire_from` |
| 2 | `LookupError`/`BootError` 与 C panic/默认条目的边界 | PASS | `boot.rs:743-748` 只有 image/priv 两种未命中；`boot.rs:815-833` image/priv 返回 `Err`，对应 C `panic`（`main.c:731/746`）；`boot.rs:839-854` sys/dev 未命中回落 `DEFAULT_SYS`/`DEFAULT_DEV`，对应 C 的 "accept the default entry"（`main.c:753-777`）；`boot.rs:760-780` 把内核调用失败 `Kernel(Errno)` 与不变式违例分开，`boot.rs:794-810` 前者保留 errno、后者映射 `EINVAL`，最终由 `main.rs:99-104` panic，外部结局与 C 的 `panic` 族一致（`main.c:225-227/427-429`）。旁注（不影响本项判定）：`lookup_priv` 无生产调用点，只有测试用（`boot.rs:825`、`boot.rs:1902/1908`），C 的 Step 1 也确实传 `pp=NULL`（`main.c:253`）；`BootError::InvalidPid` 经 `TrapKernelApi` 不可达，因为 `getnpid` 的负应答已在 `os/libs/minix-sys/src/pm.rs:877-880` 转成 `Err` |
| 3 | `SefCallbacks` 与 C 7 个 `sef_setcb_*` 的覆盖/丢失 | PASS | `sef.rs:92-121` 恰好 7 个方法：`init_fresh/init_restart/init_lu/init_response/lu_response/signal_handler/signal_manager`；C `main.c:139-149` 恰好这 7 个注册；回调类型见 `sef.h:60-63,176,269-274`；`RsServer` 的 7 个实现齐全（`lib.rs:613-820`，分别为 616/640/718/767/777/790/820）。无丢失。旁注：文档 §3.3 L685-686 仍写"5 个方法未落地、`signal_handler` 保留 `unimplemented!`"，与当前实现不符（7 个全部已实现），这是文档陈旧而非覆盖丢失，不计入本项 FAIL |
| 4 | `BootInit` 四步与 C `sef_cb_init_fresh` 的对应及结构变化 | FAIL | 映射本身成立：`boot.rs:937-944` 依次 `step0_prepare/step1_set_attrs/step2_allow_run/step3_catch_init_ready/step4_finish`，对应 C 的前置 `main.c:178-237` 与 Step 1–4 `main.c:244-433`。`step0` 作为显式第 0 步、`RProcTable` 取代 `rproc[]/rprocpub[]/rproc_ptr[]` 均已登记（文档 L958-959、L703；字段语义归 02）。两处结构偏差未落地：`self_update` 生产调用点为零（D2-01）；Step 3 的接收丢掉 C `sef_receive_status` 的信号/控制消息拦截（D2-02）。旁注：文档 §3.4 L696 写"调用四个私有方法"，实际调用五个（`boot.rs:938-942`），同节代码块 L709-716 已列出 step0，属措辞瑕疵 |
| 5 | `KernelApi` 五域面是否覆盖 §3.5 声称的外部调用（含 `get_image`/`grant_direct`） | FAIL | `SysApi`（`boot.rs:71-116`）只有 `get_machine/get_hz/get_ticks/privctl/getpriv/setalarm/sys_kill/sys_update/diagctl_stacktrace/diag_write`，没有 `get_image`，也没有 `grant_direct`/`grant_read`；`grant_read` 实际落在 `IpcApi`（`boot.rs:294-301`，实现于 `trap_api.rs:530/639-643`）。`KernelApi` 只是五个域面的并集（`boot.rs:337-338`）。而文档 §3.5 L761 把 `get_image`/`grant_direct` 列在 `SysApi` 注释里，L755 还写"boot 模块不直接调用 `minix-sys`"，但 `boot.rs:614` 在 `BootTables::acquire_from` 里直接调 `minix_sys::syscall::sys_getimage`；§3.7 L831 又把 `grant_read` 说成 `SysApi` 面。见 D2-03 |
| 6 | §3.6 的 `USE_LIVEUPDATE` 默认关与 C 默认 yes 是否写成行为差异 | PASS | C 默认确为 yes：`bsd.own.mk:1493-1501` 的 MINIX 变量组按 "corresponding MK* 不等于 no 时 `?= yes`" 处理，`USE_LIVEUPDATE` 在 `bsd.own.mk:1499`；Rust feature `live-update` 默认关（`Cargo.toml:20` `default = []`，`Cargo.toml:28` `live-update = []`）。文档 L816 写明 C 默认 yes、Rust feature 默认关，L825 明确写"默认 Rust boot 比 C 默认 yes 少这一尾步，这是接 18 前的已知差异"，已构成行为差异声明。该"已知差异"是否允许保留属 D2-01，不在本项判定 |

## FINDING D2-01（P0-design-deviation；若 DECISIONS 无 defer-approved）

- 位置：`os/servers/rs/src/boot.rs:937-944`（`init_fresh` 在 `step4_finish` 后结束）、`os/servers/rs/src/boot.rs:1258-1327`（`self_update`）；C `minix3/minix/servers/rs/main.c:436-491`；`os/servers/rs/Cargo.toml:20,28`；文档 `rewrite-notes/03-stage-rs/01-rs-boot-init.md:825`
- 问题：`self_update` 在非测试代码里没有任何调用点。`BootInit::init_fresh` 的五步走完就返回，`RsServer::init_fresh`（`lib.rs:616-629`）只做状态移交；唯一调用者是两个 `live-update` feature 测试（`boot.rs:1963`、`boot.rs:2020`）。因此即使打开 `--features live-update`，生产 boot 也不会执行 C 在 `#if USE_LIVEUPDATE` 块里默认执行的 RS 自升级尾步（clone、fork、update、cleanup、pin 或 priv/sched/yield）。feature 当前名存实亡。
- 证据：
  - C 的块位于 `sef_cb_init_fresh` 内、Step 4 之后（`main.c:436-491`），默认编译进去（`bsd.own.mk:1499`，"USE_* options which default to yes unless MK* is no" 组）。
  - Rust `init_fresh` 的调用序列到 `step4_finish` 为止（`boot.rs:937-944`）；`self_update` 定义在 `BootInit` 上但无人调用（`grep -rn "self_update" os/servers/rs/src/` 只命中定义、两个测试与 shell_request 的同名概念，生产 boot 路径零命中）。
  - 文档自己承认这一点（L825："当前 `init_fresh` 末尾没有调用 `self_update`，它只由 feature 测试覆盖"）。
- 最小修复：在 boot 尾部接上调用点——`BootInit::init_fresh` 在 `step4_finish` 后按 `#[cfg(feature = "live-update")]` 调 `self_update`（可把 `UpdateState` 作为参数从上层传入），或在 `RsServer::init_fresh` 完成 `into_state` 后按 C 的位置调用。若本轮不做，按 V3.7 在 `DECISIONS.md` 写 `defer-approved` 判决行（日期、具体条目、到期条件、不做后果），并把文档 L825 从"已知差异"改为指向该判决；仅以"已知差异"留在正文不满足零欠账。
- 置信度：高（调用点缺失与 C 默认值均可逐行取证）

## FINDING D2-02（P1-design-deviation）

- 位置：`os/servers/rs/src/boot.rs:1167-1181`（尤其 1174-1181 的自述注释与 `sys.receive`）；C `minix3/minix/servers/rs/main.c:795`；`minix3/minix/lib/libsys/sef.c:233-238`；`minix3/minix/include/minix/sef.h:263-264`；文档 `01-rs-boot-init.md:739`、`12-rs-init-run.md:145`
- 问题：C 的 `catch_boot_init_ready` 用 `sef_receive_status(endpoint, &m, &ipc_status)`（`main.c:795`）。`sef_receive_status` 在把消息交给调用者之前会拦截 SYSTEM 通知（信号请求）、RS ping、LU/init 请求等控制消息（`sef.c:233-238` 的 `do_sef_signal_request` 等分支；`sef.h:263-264` 的 `IS_SEF_SIGNAL_REQUEST`）。Rust 侧改用裸 `IpcApi::receive`，并把 `ipc_status` 直接丢弃（`boot.rs:1181` 的 `_ipc_status`），遇到 SYSTEM 通知或信号请求会落到 `m.m_type != RS_INIT` 并 panic。代码注释自认这是 "deliberate leftover … moving the wiring up is a later decision"（`boot.rs:1174-1180`），设计文档没有登记该偏差；相反，文档 §3.4 L739 仍停留在"生产 `IpcApi` 未接线、mock inbox 时 fail-closed"的旧状态，与已接线的 `TrapKernelApi` 不符。
- 证据：
  - C：`main.c:795` 的 `sef_receive_status` 是阻塞等待指定 ready 的入口；`sef.c:150-238` 展示其拦截循环，信号请求分支调用 `do_sef_signal_request`；`sef.h:263-264` 定义拦截判据。
  - Rust：`boot.rs:1181` `let (m, _ipc_status, _ts) = sys.receive(endpoint)...`，随后只做 `m_type`/result/VM 免回复检查（1182-1216），没有任何信号或控制消息分派；`lib.rs:790-808` 的 `signal_handler` 只在主循环路径被用，boot 机器拿不到。
  - 文档：`12-rs-init-run.md:145` 仍写 boot Step 2/3 用 `sef_receive_status(endpoint, &m, &ipc_status)`，与 `01-rs-boot-init.md:739` 的旧前提和代码实现三方不一致。
- 最小修复：给 `IpcApi` 增加带状态的接收面（或复用 `minix_sef::sef_receive_status`），在 `catch_boot_init_ready` 的循环里先分派可拦截消息（信号走 `signal_handler`/`signal_manager`，ping/LU 按 SEF 语义处理）再继续等待目标 `RS_INIT`；若本轮有意不支持，必须在文档 §2.3.3/§3.4 以 `[ARCH: ...]` 登记该偏差并同步 12 §2.5，不能只留代码注释。
- 置信度：高（C 语义与 Rust 注释双向取证）；实际触发概率中等（boot 期信号发送者少，但 C 明确支持）

## FINDING D2-03（P1：设计文档与代码的外部边界拓扑不一致）

- 位置：文档 `01-rs-boot-init.md:755`（boot 模块不直接调 minix-sys、生产入口在 main.rs 组装 `TrapKernelApi`）、`:761`（`SysApi` 注释列 `get_image`/`grant_direct`）、`:831`（`grant_read` 属 `SysApi` 面）；`os/servers/rs/src/boot.rs:71-116`（`SysApi` 无这两个方法）、`:228-301`（`grant_read` 在 `IpcApi`）、`:337-338`（`KernelApi` 并集）、`:600-620`（`acquire_from` 直接调 `minix_sys::syscall::sys_getimage`，614）；`os/servers/rs/src/trap_api.rs:530`（`impl IpcApi`）、`:639-643`（`grant_read` 实现）；`os/servers/rs/src/main.rs:86`、`os/servers/rs/src/lib.rs:238-241`（`TrapKernelApi` 实际组装点）
- 问题：§3.5 把 `KernelApi` 定义为唯一外部边界，并声称 boot 模块不直接调用 `minix-sys`；实际 `get_image` 不在五个域面中的任何一个，`BootTables::acquire_from` 直接调用 `minix_sys::syscall::sys_getimage`，绕过了 `KernelApi`。`grant_direct` 以 `grant_read` 的形式落在 `IpcApi`，而 §3.5/§3.7 把它归到 `SysApi`。同一份"外部边界"在文档和代码里是两套拓扑，读者按 §3.5 去找 `SysApi::get_image`/`SysApi::grant_read` 会找不到。另外 L755 说生产入口在 `main.rs` 用 `TrapKernelApi` 组装，实际 `main.rs:86` 只调用 `RsServer::new(tables)`，组装在 `lib.rs:238-241`。
- 证据：
  - `boot.rs:71-116` 的 `SysApi` 方法清单完整列出，无 `get_image`、无 `grant_*`；`boot.rs:294-298` 的 `grant_read` 位于 `IpcApi`（该 trait 从 228 行开始）。
  - `boot.rs:600-620` 的 `acquire_from` 参数是 `&impl KernelCallTransport`，`boot.rs:614` 直接调 `minix_sys::syscall::sys_getimage`；`main.rs:62-63` 用 `DirectKernelCallTransport` 调用它。
  - 文档 L755 的"boot 模块不直接调用 `minix-sys`"与上述调用点相反；L761、L831 的归属与 trait 定义相反。
- 最小修复：二选一，并把所有引用改到同一版本。优先按文档已声明的设计修代码：`get_image` 加入 `SysApi`，`grant_read`/`grant_revoke` 从 `IpcApi` 移入 `SysApi`（二者都是内核 grant 系统调用面），`BootTables::acquire_from` 改收 `&mut dyn KernelApi`；或者明确降级文档：§3.5 注明 image 采集是 `BootInit` 之前的构造期 `KernelCallTransport` 例外，§3.7 把 `grant_read` 改回 `IpcApi` 面，并删除"boot 模块不直接调用 `minix-sys`"的绝对表述、把组装点改为 `RsServer::new`。
- 置信度：高（trait 方法清单、调用点、文档三处可逐行核对）

## SUMMARY

- PASS：4 项（1 BootTables、2 LookupError/BootError、3 SefCallbacks 覆盖、6 USE_LIVEUPDATE 行为差异声明）
- FAIL：2 项（4 BootInit 四步对应、5 KernelApi 五域面覆盖）
- FINDING：3 条
- 最严重 3 条：
  1. **D2-01（P0）**：`self_update` 生产零调用点，C 默认 yes 的 RS 自升级尾步在 Rust 任何 feature 组合下都不会执行；文档以"已知差异"记录，未走 `defer-approved`。
  2. **D2-03（P1）**：§3.5 宣称的唯一外部边界 `KernelApi` 不覆盖 `get_image`，`grant_read` 又落在 `IpcApi` 而非 `SysApi`；`boot.rs:614` 还直接调用 `minix-sys`，与文档 L755 相反；`TrapKernelApi` 的组装点也写成 main.rs 而实际在 lib.rs。
  3. **D2-02（P1）**：Step 3 的 `catch_boot_init_ready` 用裸 `IpcApi::receive`，丢掉 C `sef_receive_status` 的信号/控制消息拦截，代码注释自认延后而设计文档未登记。
- 其他非本项定的陈旧点（不计入 FAIL 计数）：文档 §3.3 L685-686 仍称 5 个 SEF 回调未落地、`signal_handler` 为 `unimplemented!`，实际 7 个全部已实现（`lib.rs:616/640/718/767/777/790/820`）；§3.4 L739 的"生产 `IpcApi` 未接线"与 `TrapKernelApi` 已落地矛盾（后者已并入 D2-02 证据）；§3.4 L696 的"四个私有方法"应为五个（含 step0）。
