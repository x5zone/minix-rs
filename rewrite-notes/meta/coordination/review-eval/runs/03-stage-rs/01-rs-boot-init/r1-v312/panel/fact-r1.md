# P-FACT r1：01-rs-boot-init 定点事实核对

核对对象：`rewrite-notes/03-stage-rs/01-rs-boot-init.md`（1161 行）
真值：C `minix3/minix/servers/rs/{main.c,table.c}`、`minix3/minix/lib/libsys/{sef.c,sef_init.c}`；Rust `os/servers/rs/src/{main,lib,table,boot,sef,dispatch,process_table,trap_api}.rs`
核对方式：只读；第 6 项实际执行 `cd os && cargo test -p minix-rs --lib` 与 `cargo test -p minix-rs --lib -- --list`。

## 定点核对

| # | 项目 | 结果 | 证据（文件:行） |
|---|------|------|------------------|
| 1 | C `main` / `sef_local_startup` / `sef_cb_init_fresh` 行号与四步范围 | PASS | `main.c:38` `int main(void)`；`main.c:136` `static void sef_local_startup()`；`main.c:158` `static int sef_cb_init_fresh(...)`；四步注释 `main.c:239-243`；Step1 循环 `main.c:244-346`；Step2 `main.c:348-399`；Step3 `main.c:401-407`；Step4 `main.c:409-433`（Step4 循环 413-430、`sys_setalarm` 433） |
| 2 | SEF 7 个回调注册与 C 名称 | PASS（机制表述另见 FACT-03） | `main.c:139-149` 恰好 7 个 `sef_setcb_*`：init_fresh/init_restart/init_lu/init_response/lu_response/signal_handler/signal_manager，与文档 §2.2 代码块逐名一致；对 `minix3/minix/servers/*/` 逐个统计去重后的 `sef_setcb_*` 名称，得 RS 7、VM/VFS 6、其余更少，文档"RS 是唯一注册全量的用户态服务"成立 |
| 3 | `boot_image_priv/sys/dev` 三表条目与 table.c | PASS | `table.c:15-30` priv 12 项 + NULL 哨兵、顺序与 label/flags 与文档 §2.7 表逐项一致；`table.c:33-42` sys 6 覆盖项 + DEFAULT（RS/PM/SCHED/VFS=SRVR_SF、VM=VM_SF、MFS=0、DEFAULT=SRV_SF）；`table.c:45-50` dev（TTY=`TTY_MAJOR`=4、MEM=`MEMORY_MAJOR`=1、DEFAULT=0，`dmap.h:22,25`）。Rust `table.rs:91-152/161-186/195-204` 同序同值 |
| 4 | `boot_image_info_lookup` C 签名与查找顺序 | PASS（priv 未命中语义另见 FACT-10） | `main.c:709-715` K&R 签名 6 参（endpoint/image/ip/pp/sp/dp）；顺序 image（723-733）→ priv（735-748）→ sys（753-762）→ dev（768-777）；Step1 调用 `main.c:253-254` 实参 `&ip, NULL, &boot_image_sys, &boot_image_dev` 与文档 §2.4 一致 |
| 5 | Rust `BootTables` / `LookupError` / `BootInit` / `SefCallbacks` | FAIL（BootInit 字段） | `boot.rs:554-563` BootTables 四字段 `image/priv_table/sys_table/dev_table` 与文档 §3.1 一致；`boot.rs:743-748` LookupError 两变体一致；`sef.rs:92-121` 7 方法 trait 与文档 §3.3 一致；但 `boot.rs:878-896` BootInit 实际字段是 `table: RProcTable`（888），没有文档 §3.4 的 `slots: Vec<ServiceSlot>`（见 FACT-04） |
| 6 | `cargo test -p minix-rs --lib` 当前计数（文档写 359/0） | FAIL（359 本体正确，内部计数自相矛盾） | 实测 `test result: ok. 359 passed; 0 failed; 0 ignored`（§5.0/§5.4 的 359/0 正确）；但 §5 引言写 208/29，§5.4 的分模块合计写 41（boot 29/table 3/sef 1/dispatch 8）；`--list` 实测 boot::tests 37、table::tests 3、sef::tests 2、dispatch::tests 8（合计 50），`rg -c '#\[test\]'` 得 361（含 2 个 live-update feature-gated） |
| 7 | 文档 §4 的 Rust 函数名/模块名是否存在 | FAIL（名称存在，签名/落点与源码不符） | 存在：`main.rs:42 fn main`、`main.rs:46 real_main`、`lib.rs:129 RsServer`、`lib.rs:238 new`、`lib.rs:246 with_kernel`、`lib.rs:277 init`、`lib.rs:291 state`、`lib.rs:313 run`、`table.rs` 三表、`boot.rs` BootTables/LookupError/lookup_*/RinitState/BootInit/step0-4/self_update、`sef.rs` SefInitType/SefInitInfo/SefCallbacks、`dispatch.rs` IpcStatus/DispatchKind/classify/DispatchResult/dispatch_request；不符：§4.1 main 代码块仍是 placeholder 路径、§4.2 `run() -> !`、§4.4 BootInit.ServiceSlot、§3.6 `self_update` 参数少一个（见 FACT-01/04/05/09） |

## FINDING FACT-01（P0）

- 位置：`rewrite-notes/03-stage-rs/01-rs-boot-init.md:638-640`（§3.1 第 1 条）、`:773`（§3.5 边界段）、`:826-828`（§3.5 fail-closed 条）、`:917-945`（§4.1 整段代码块与 bullet）、`:968`（§4.2 `new` 注释）、`:1161`（§7）
- 问题：文档把生产接线描述成"尚未发生"：`minix-sys` 是 stub、生产路径用 `BootTables::placeholder()`、`KernelApi` 生产占位是 `UnimplementedKernelApi`、`main.rs` 的 boot 失败原因是 "kernel API wiring pending"。当前工作树上这些都已落地且生产入口不再走占位。
- 证据：
  - `os/libs/minix-sys/src/syscall.rs:1156` 已实现 `pub fn sys_getimage(transport, out)`；`:156/:166` 定义 `KernelCallTransport`/`DirectKernelCallTransport`；`os/libs/minix-sys/src/lib.rs:3-5` 自述"communication primitives and the system call protocol are fully implemented"。
  - `boot.rs:600-620` `BootTables::acquire_from` 用内核 GET_IMAGE 装载真实表；`boot.rs:576-585` `placeholder()` 注释明确"values for tests and host development"。
  - `main.rs:62-63` 生产入口调用 `BootTables::acquire_from(&minix_sys::syscall::DirectKernelCallTransport)`；`:86` `RsServer::new(tables)`；`:109-111` `run()` 返回 `Err` 才 panic。
  - `lib.rs:238-243` `RsServer::new` 的默认 `kernel` 是 `trap_api::TrapKernelApi::new()`；`trap_api.rs:1-4` 自述 "each method delegates to a real minix-sys wrapper (no stubs remain)"；`UnimplementedKernelApi` 仅剩测试引用（`sef.rs:131`、`testutil.rs:559`、`boot.rs:1389`）。
  - `main.rs:46` 的真实入口是 `real_main()`（`fn main` 只是按 target 形态转调），§4.1 代码块把整段逻辑画在 `fn main` 直体里且保留 `BootTables::placeholder()` 与 "kernel API wiring pending" 的 panic 文案，与 `main.rs:41-113` 不符。
- 最小修复：重写 §3.1 第 1 条与 §3.5 边界段，改为"`acquire_from` 已从内核 GET_IMAGE 装载，生产 `KernelApi` 为 `TrapKernelApi`；`placeholder()` 仅测试/宿主开发用"；按 `main.rs:46-113` 重写 §4.1 代码块与 bullet；把 §4.2 `new` 注释的 `UnimplementedKernelApi` 改为 `TrapKernelApi`；更新 §7 的 minix-sys 行。
- 置信度：高（生产入口、构造函数与 syscall 实现三处直接取证）。

## FINDING FACT-02（P0）

- 位置：`rewrite-notes/03-stage-rs/01-rs-boot-init.md:43`（§1.1 Step 1）、`:597`（§2.8 小结第 2 条）
- 问题：文档说 Step 1"没有向 kernel 提交任何东西（除 RS/VM 例外），纯本地准备"/"Step 1 不向内核提交"。C 的 Step 1 对除 RS/VM 外的每个服务调用 `sys_privctl(SYS_PRIV_SET_SYS)`，并对所有服务调用 `sys_getpriv`，不是纯本地准备。
- 证据：`main.c:282-296`：`if (boot_image_priv->endpoint != RS_PROC_NR && != VM_PROC_NR) sys_privctl(ip->endpoint, SYS_PRIV_SET_SYS, &(rp->r_priv));`（287-290），随后 `sys_getpriv(&(rp->r_priv), ip->endpoint)`（294）。文档 §2.3.3 自己写的是"RS/VM 例外：`SYS_PRIV_SET_SYS` 对 RS/VM 跳过……其余服务则把 priv 结构提交给 kernel"（`01-rs-boot-init.md:359`），与 §1.1/§2.8 直接矛盾。
- 最小修复：§1.1 改为"Step 1 建立本地槽位并提交 priv 结构：非 RS/VM 服务走 `SYS_PRIV_SET_SYS` + `sys_getpriv`，RS/VM 跳过 `SET_SYS`；sys/dev/命令/调度属性在本地准备"；§2.8 同步改为"Step 1 完成 priv 提交与本地属性准备"。
- 置信度：高（C 调用点与文档自身 §2.3.3 双向取证）。

## FINDING FACT-03（P1）

- 位置：`rewrite-notes/03-stage-rs/01-rs-boot-init.md:52`（§1.2）、`:220`（§2.2）、`:1009-1010`（§4.5）
- 问题：文档说 `sef_startup()` 会"把第一条 `RS_INIT` 分派到 `sef_cb_init_fresh()`"，并说 SEF 启动期"接收 `SEF_INIT` 消息（`m_type == RS_INIT`）……按 `info->init_type` 分派"。RS 自身不走这条接收路径：它在 `sef_startup()` 里因 `ROOT_SYS_PROC` 直接调用本地合成函数 `do_sef_rs_init()`，不会等待自己发来的 RS_INIT（否则死锁）；且 `sef_init_info_t` 没有 `init_type` 字段，init 类型是回调参数 `type` / 消息字段 `m_rs_init.type`。
- 证据：`minix3/minix/lib/libsys/sef.c:109-115`（`if(sef_self_priv_flags & ROOT_SYS_PROC) do_sef_rs_init(old_endpoint)`）；`sef.c:120-131`（其他服务才 `ipc_receive(RS_PROC_NR, ...)` 等待初始化消息）；`sef_init.c:150-171`（`do_sef_rs_init` 本地 `memset(&info)` 并按 `LU_SYS_PROC`/`RST_SYS_PROC` 选 `SEF_INIT_FRESH/LU/RESTART`）；`sef_init.c:202`（接收路径取 `type = m_ptr->m_rs_init.type`）；`sef.h:41-53`（`sef_init_info_t` 无 `init_type` 字段）、`sef.h:56`（`sef_cb_init_t(int type, sef_init_info_t *info)`）。
- 最小修复：把"接收 RS_INIT"限定为"其他被 RS 初始化的服务"；RS 单独说明为 `do_sef_rs_init()` 本地构造 Fresh 信息后调用 `sef_cb_init_fresh`；`info->init_type` 改为 `type`（回调参数）/`m_rs_init.type`。§4.5 的 "按 `info.init_type`" 同步改。
- 置信度：高（sef.c/sef_init.c/sef.h 三处直接取证）。

## FINDING FACT-04（P1）

- 位置：`rewrite-notes/03-stage-rs/01-rs-boot-init.md:716-724`（§3.4 BootInit 代码块，尤其 720）、`:994`（§4.4 `ServiceSlot` 条）
- 问题：文档的 `BootInit` 字段清单写 `slots: Vec<ServiceSlot>`，实际字段是 `table: RProcTable`（`boot.rs:888`）；`ServiceSlot` 定义在 `service_slot.rs:421`，由 `RProcTable` 内部以 `slots: Vec<ServiceSlot>` 持有（`process_table.rs:72-74`）。§4.4 把 `ServiceSlot` 说成 boot.rs 的"最小生命周期视图"也不准确：boot 模块现在只编排，槽位建立落在 `RProcTable::activate_boot_slot`（`boot.rs:1028-1037`）。
- 证据：`os/servers/rs/src/boot.rs:878-896`（字段 `tables/machine/rinit/table/shutting_down/system_hz/nr_uncaught_init_srvs`）；`os/servers/rs/src/process_table.rs:72-92`；`os/servers/rs/src/service_slot.rs:421-429`。
- 最小修复：§3.4 字段改为 `table: RProcTable`；§4.4 改为"`RProcTable`（内部 `Vec<ServiceSlot>`）承接 C 的 `rproc[]/rprocpub[]`，Step 1 经 `activate_boot_slot` 建槽；字段语义归 02/service_slot.rs"。
- 置信度：高（结构体定义逐字段核对）。

## FINDING FACT-05（P1）

- 位置：`rewrite-notes/03-stage-rs/01-rs-boot-init.md:972`（§4.2 代码块 `pub fn run`）、`:940`（§4.1 `server.run();`）
- 问题：文档签名写 `pub fn run(&mut self) -> !`，实际是 `pub fn run(&mut self) -> Result<(), Errno>`；`main.rs` 在 `run()` 返回 `Err` 时 panic，而不是发散返回。
- 证据：`os/servers/rs/src/lib.rs:313`（`pub fn run(&mut self) -> Result<(), Errno>`）；`os/servers/rs/src/main.rs:109-111`（`if let Err(e) = server.run() { panic!(...) }`）；`lib.rs:295-309` 的 `run` 注释也说明"leaving the loop means a transport/boot failure and is fatal"。
- 最小修复：§4.2 签名改为 `pub fn run(&mut self) -> Result<(), Errno>`；§4.1 的 `server.run();` 改为 `if let Err(e) = server.run() { panic!(...) }` 的形态或注明由 main 处理。
- 置信度：高（函数定义与调用点）。

## FINDING FACT-06（P1）

- 位置：`rewrite-notes/03-stage-rs/01-rs-boot-init.md:856-863`（§3.8 `DispatchKind` 代码块，尤其 857）
- 问题：文档写 `ClockNotify,`（无字段），实际变体是 `ClockNotify { timestamp: Clock }`；`classify()` 的 `timestamp` 参数正是写入该字段，`do_period` 需要该内核通知时间戳。
- 证据：`os/servers/rs/src/dispatch.rs:56-59`（`ClockNotify { timestamp: Clock }`）；`dispatch.rs:105-108`（`return DispatchKind::ClockNotify { timestamp };`）；`dispatch.rs:52-55` 注释说明该时间戳是 `do_period` 的 `now`（`request.c:948`）。
- 最小修复：§3.8 代码块改为 `ClockNotify { timestamp: Clock },`，并在注释中说明该字段的来源与消费点（07）。
- 置信度：高（枚举定义与 classify 实现）。

## FINDING FACT-07（P1）

- 位置：`rewrite-notes/03-stage-rs/01-rs-boot-init.md:1041`（§5 引言）、`:1112`（§5.4）
- 问题：§5 引言写"208 passed / 0 failed""四模块 29 项（boot 18 + table 3 + sef 1 + dispatch 7）"，§5.4 写"四模块共 41 项：boot.rs 29、table.rs 3、sef.rs 1、dispatch.rs 8"，两处互相矛盾且都与当前工作树不符。§5.0/§5.4 的 headline 359/0 是正确的。
- 证据：
  - `cd os && cargo test -p minix-rs --lib` → `test result: ok. 359 passed; 0 failed; 0 ignored`。
  - `cargo test -p minix-rs --lib -- --list` 精确计数：`^boot::tests::` 37、`^table::tests::` 3、`^sef::tests::` 2、`^dispatch::tests::` 8（合计 50）。
  - `rg -c '#\[test\]' os/servers/rs/src` 合计 361；其中 `boot.rs:1934/1991` 两个测试受 `#[cfg(feature = "live-update")]` 门控（默认 feature 关闭，`os/servers/rs/Cargo.toml:27-28`），实际运行 359。
- 最小修复：删除或更新 §5 引言的 208/29/18/3/1/7；§5.4 改为"四模块 50 项：boot 37、table 3、sef 2、dispatch 8"；把"rg 合计 359"改为"361 个 `#[test]` 属性，其中 2 个 live-update-gated，实际运行 359"。
- 置信度：高（命令读数两次一致）。

## FINDING FACT-08（P1）

- 位置：`rewrite-notes/03-stage-rs/01-rs-boot-init.md:1072`（§5.2 `test_init_fresh_step_order` 行）、`:1092`（§5.2 `test_step2_init_service_marks_and_sends` 行）
- 问题：两条测试覆盖描述与测试体相反/不符。
  - `test_init_fresh_step_order` 的描述说"step1 ×10 → step2 sched×10+Allow×10 → step4 getnpid×12+setalarm(100)"，暗示 boot 跑完四步；实际测试断言 boot 在 Step 3 以 `Err(BootError::Kernel(ENOSYS))` 失败，并显式断言 Step 4 的 `GetNpid` 与 `SetAlarm` 均未发生。
  - `test_step2_init_service_marks_and_sends` 的描述说 RS_INIT 的 `gid=-1`；实际断言 `init.rproctab_gid == 1`（step0 的 mock grant id）。
- 证据：`os/servers/rs/src/boot.rs:1488-1492`（`boot.init_fresh(&mut sys)` 期望 `Err(Kernel(ENOSYS))`）、`:1550-1556`（断言无 `SetAlarm`/`GetNpid`）；`boot.rs:2350-2352`（`assert_eq!(init.rproctab_gid, 1, ...)`）。
- 最小修复：把 `test_init_fresh_step_order` 格改为"step1 SetSys×10/getpriv×12、step2 sched×10+Allow×10；Step 3 空邮箱 fail-closed，Step 4 不执行"；把 gid 描述改为 `gid=1`（mock 的 step0 grant）。
- 置信度：高（测试体逐行核对）。

## FINDING FACT-09（P2）

- 位置：`rewrite-notes/03-stage-rs/01-rs-boot-init.md:837`（§3.6 代码块）
- 问题：文档的 `self_update` 签名是 `fn self_update(&mut self, sys: &mut dyn KernelApi) -> Result<(), Errno>`，实际多一个 `update: &mut crate::live_update::UpdateState` 参数。
- 证据：`os/servers/rs/src/boot.rs:1258-1264`（`pub fn self_update(&mut self, sys: &mut dyn KernelApi, update: &mut crate::live_update::UpdateState) -> Result<(), Errno>`）。
- 最小修复：§3.6 代码块补 `update: &mut UpdateState` 参数（并说明它承接 C 的 `rupdate` 全局）。
- 置信度：高（函数定义）。

## FINDING FACT-10（P2）

- 位置：`rewrite-notes/03-stage-rs/01-rs-boot-init.md:524`（§2.4 要点第 1 条）
- 问题：文档说"image 表 / priv 表：未命中 → panic（启动协议错误，fail-closed）"。image 分支确实 panic；priv 分支的 panic 条件写错了：`if (i == NULL_BOOT_NR)` 把循环下标与哨兵值比较，priv 表只有 12 项、哨兵下标是 12，而 `NULL_BOOT_NR = NR_BOOT_PROCS = 17`，该 panic 实际不可达，未命中时 `*pp` 不被赋值。
- 证据：`main.c:738-748`（循环条件 `boot_image_priv_table[i].endpoint != NULL_BOOT_NR`，panic 条件 `i == NULL_BOOT_NR`）；`const.h:61`（`NULL_BOOT_NR NR_BOOT_PROCS`）；`minix3/minix/kernel/table.c:44-63`（`image[NR_BOOT_PROCS]` 共 17 行）；`main.c:730-732`（image 分支的 `i == NR_BOOT_PROCS` 才是有效 panic）。
- 最小修复：§2.4 注明 C priv 分支这个比较缺陷（意图 fail-closed 但未生效）；Rust 用 `Err(LookupError::PrivTable)` 保留语义即可，但不要把"priv 未命中 → panic"归给当前 C 实现。
- 置信度：高（C 表长度与哨兵值可数）。

## FINDING FACT-11（P2）

- 位置：`rewrite-notes/03-stage-rs/01-rs-boot-init.md:997`（§4.4 `step0_prepare` 条）
- 问题：文档把 `env_parse` 配置注入列为 `step0_prepare` 的组成；实际 `step0_prepare` 只做 `get_machine`/`get_hz`/`shutting_down=false`/`validate_tables`/`RProcTable::new`/`grant_read`，`rs_verbose` 的 env_parse 等价在 `main.rs` 里经 `parse_rs_verbose` 读取。
- 证据：`os/servers/rs/src/boot.rs:949-981`（step0 全流程，无 env_parse）；`os/servers/rs/src/main.rs:65-75`（`sys_getmonparams` + `minix_rs::boot::parse_rs_verbose`）；`boot.rs:686-692`（`parse_rs_verbose`）。
- 最小修复：从 step0 描述删去 env_parse，或在 §4.4 注明"配置参数由 main.rs 的 `parse_rs_verbose` 读取后注入，不在 step0"。
- 置信度：高（step0 函数体逐行核对）。

## FINDING FACT-12（P2）

- 位置：`rewrite-notes/03-stage-rs/01-rs-boot-init.md:33`、`:220`、`:284`、`:570`、`:845`、`:993`
- 问题：六处引用锚点/类型标注与源码不符：
  1. `:33` 写 `minix3/minix/kernel/table.c:boot_image`；实际数组变量名是 `image`（`table.c:44` `struct boot_image image[NR_BOOT_PROCS]`），`boot_image` 是结构体类型名。
  2. `:220` 写"7 个回调的机制归属见 §1.3 表"；§1.3 只有小结/术语，没有表（机制归属表在 §3.3 与 `sef.rs:20-28` 的文档表）。
  3. `:284` 把内核 boot 端点由 `_ENDPOINT(0, proc_nr)` 派生标为 `main.c:196`；该行是 RS 的 `sys_getimage`，派生点在 `minix3/minix/kernel/proc.c:133`。
  4. `:570` 写"基 flag 在 minix3/minix/include/minix/rs.h:SF_CORE_SRV/195"；`SF_CORE_SRV` 在 `rs.h:191`，`195` 是 `SF_NEED_REPL`。
  5. `:845` 写 `minix-types id.rs:75`；实际路径是 `os/libs/minix-types/src/types/id.rs:75`（`GrantId = i32`）。
  6. `:993` 写 `RinitState { rproctab_gid: Option<GrantId> }`；实际字段是 `Option<u32>`（`boot.rs:871`，step0 以 `gid as u32` 写入，`boot.rs:980`），与 §3.7 自己的 `Option<u32>` 也不一致。
- 证据：上述各源码位置逐一 `grep -n`。
- 最小修复：按上述六条逐项改锚点/类型；§1.3 的引用改指 §3.3 或 "sef.rs 的 callback 表"。
- 置信度：高（均为可解析的符号/行号核对）。

## FINDING FACT-13（P2）

- 位置：`rewrite-notes/03-stage-rs/01-rs-boot-init.md:1016`（§4.6 `dispatch_request` 条）
- 问题：文档说 `dispatch_request(call_nr)` 承载"15 个调用号 → handler 归属表（`RS_UP → 13` 等）"；实际函数没有 `RS_UP` 分支：`RS_UP`/`RS_EDIT` 等已经 live 走 `RsServer::do_request`，永不进入该表；`dispatch_request` 只剩 `RS_DOWN/RS_REFRESH/RS_RESTART/RS_SHUTDOWN/RS_CLONE/RS_UNCLONE`（13）、`RS_UPDATE`（16）、`RS_SYSCTL/RS_FI/RS_GETSYSINFO/RS_LOOKUP`（14）与 `_ => ENOSYS` 分支。
- 证据：`os/servers/rs/src/dispatch.rs:144-150`（注释明确 "RS_UP/RS_EDIT ... RS_LOOKUP run live through RsServer::do_request — they never reach this table"）；`dispatch.rs:151-164`（实际 match 分支）。
- 最小修复：把该条改为"`dispatch_request` 只承载尚未 live 的调用号；`RS_UP`/`RS_EDIT` 等经 `RsServer::do_request` 直达 13，不在此表"，归属表与代码注释对齐。
- 置信度：高（函数体与注释）。

## SUMMARY

- PASS：4 项（CHECK 1、2、3、4；CHECK 4 的 priv 未命中语义另有 FACT-10）
- FAIL：3 项（CHECK 5 BootInit 字段；CHECK 6 内部计数；CHECK 7 §4 签名/落点）
- FINDING：13 条
  - P0：2 条（FACT-01 生产接线状态整段过期；FACT-02 Step 1"不向内核提交"与 C 相反）
  - P1：6 条（FACT-03 RS 的 SEF 启动路径；FACT-04 BootInit 字段；FACT-05 `run` 返回类型；FACT-06 `ClockNotify` 字段；FACT-07 §5 计数自相矛盾且与实测不符；FACT-08 §5.2 两条测试描述与测试体不符）
  - P2：5 条（FACT-09 `self_update` 参数；FACT-10 C priv 未命中 panic 不可达；FACT-11 step0 不含 env_parse；FACT-12 六处引用/类型锚点；FACT-13 `dispatch_request` 归属表述）
- 基线复核：`cd os && cargo test -p minix-rs --lib` 实测 **359 passed / 0 failed / 0 ignored**，与文档 §5.0/§5.4 的 headline 一致；§5 引言的 208 与 §5.4 的 41 项分模块计数不成立（实测四模块 37+3+2+8=50）。
