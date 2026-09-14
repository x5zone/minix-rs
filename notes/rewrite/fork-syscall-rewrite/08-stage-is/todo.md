# 08-stage-is Rust 实现架构级 Review TODO

> 来源：2026-09-14 首轮架构级审查（cmd-04 形态，scope=stage，叠加 full-review 的 Gate A 覆盖率强制；本文档为首轮产物，无历史存档）。
> 范围：一等对象 `os/servers/is/src/` 全部 Rust 代码（13 文件，约 3551 行，crate 名 `minix-is`，86 个单元测试）；取数协议面的对端（minix-types 常量、kernel `do_getinfo`、PM/VFS/RS/DS 的 `getsysinfo`、VM `vm_info` 的 producer 侧）为辅——对端发现按 edge 判定规则登记 `../edge_todo.md`。
> 方法：先查漏补缺（Gate A coverage-extract + 「C 符号 ↔ 13 篇文档 ↔ Rust 文件」三向矩阵 + 常量对账），再按「组合层 → 服务器内部 → 内核接缝 → wire 层 → 测试」五层深审，对照 Redox（联网核实）/OS 理论/Rust 社区惯例。本轮只审查未修代码，修复走后续 todo-fix 单条执行。
> 定位：不复写 plan.md；跨 stage 条目唯一入口是 `../edge_todo.md`，本文档只留双向指针（§4）。
> 状态（2026-09-14，V1 轮）：审查完成，P0 为零。stage 内新发现 2 条 P1 + 2 条 P2 + 5 条 P3；结构性缺口全部在缝上（生产 transport 接线、producer 布局对齐、A-3 通道、启动链——登记 edge 四条目）。本 crate 的分类器、快照、编码器、游标、格式常量、测试各层形状稳定，与 13 篇文档契约的对账一致性好；两项 P1 分别是「文档不变量被代码违反」与「取数接缝撑不起执行面」，均可在不推翻现有设计的前提下收敛。
> 状态（2026-09-15，修复轮）：**Fix #1~#6 已闭环**（含 V1-P1-2 接缝定型与 V1-P1-3 run_dump 16 体、V1-P1-4 expand_newlines 修复，§8），测试基线 86 → **106 passed**，clippy 本体 0 告警。stage 内仅剩 V1-P3-3/V1-P3-4 两个文档批条目。按 §7 顺序逐条推进，每条一个提交。

---

## 0. 审查结论速览

一句话总结：IS 的 Rust 实现是「文档先行、代码追赶」形态的成功样本——01~10 篇定稿（2026-09-04）后落地的 3551 行与文档 §3/§4 的签名、不变量逐条对得上；但代码追到文档声明的延后线（`run_dump` 空体等 A-6/A-10/A-3 缺口）就停了，而文档自己没有把「越过这条线还缺什么」写全：五个取数通道的 trait 没有数据出口，fill 体时全部要改签名（V1-P1-2）；02 篇钉死的「注册失败要告警」不变量被 `request_fkey_map` 静默吞掉（V1-P1-1）。

| 级别 | 条目 | 一句话 | 状态 |
|------|------|--------|------|
| P0 | （无） | 三向矩阵无缺口，排除项核对通过（两处未标注除外，见 P3-3） | — |
| P1 | V1-P1-1 | fkey 注册失败告警被吞（违反 02 §4.3 不变量 2 + 偏离 C dmp.c:63-65） | ✅ 已修复 2026-09-15（Fix #1，§8） |
| P1 | V1-P1-2 | 取数五通道 trait 无数据出口——run_dump 填体的前置设计缺口（含 A-6 输出通道选型） | ✅ 已修复 2026-09-15（Fix #5，§8；执行面拆出 V1-P1-3） |
| P1 | V1-P1-3 | run_dump 16 体填实（05~10 执行面：取数→渲染，从 V1-P1-2 派生） | ✅ 已修复 2026-09-15（Fix #6，§8） |
| P1 | V1-P1-4 | expand_newlines 丢失最后一个换行（V1-P1-3 实施中发现的既有函数 bug，模式 78 无标注偏离） | ✅ 已修复 2026-09-15（Fix #6，§8） |
| P2 | V1-P2-1 | `minix-sys` 死依赖（声明但全 crate 零引用） | ✅ 已修复 2026-09-15（Fix #2，§8） |
| P2 | V1-P2-2 | `fkey_mapped` 与 `state.call_nr` 只写不读（死状态，C 无对应物） | ✅ 已修复 2026-09-15（Fix #3，§8） |
| P3 | V1-P3-1 | 16 长度隐式耦合（`matched`/`keys` 两个栈数组靠 HOOKS.len()==16 才正确） | ✅ 已修复 2026-09-15（Fix #4，§8） |
| P3 | V1-P3-2 | `request_fkey_map` 返回 `Result` 但两臂皆 `Ok`（01 时代 ENOSYS 桩的类型遗迹；随 P1-1 同批修） | ✅ 已修复 2026-09-15（Fix #1，§8） |
| P3 | V1-P3-3 | 00/99 篇 pending + `.design/` 三快照缺失（Gate H.1/H.6 FAIL×2）；99 收口时补 `DIAG_BUF_SIZE`/`_SYSTEM` 排除标注 | 开放 |
| P3 | V1-P3-4 | 01 篇 §4.1 声称 `extern crate alloc` 与实际不符（代码零分配、无 alloc） | ✅ 已修复 2026-09-15（Fix #7，§8） |
| P3 | V1-P3-5 | 五个分页游标类型并存（审查结论：维持，理由与观察记录见条目） | 维持（记录） |
| edge | E-ISWIRE | 生产 transport 接线（minix-sef/minix-sys）→ §4 | 登记于 ../edge_todo.md |
| edge | E-ISPROD | GETSYSINFO/GET_* producer 布局与 IS 快照对齐（含 kernel `ProcInfoStruct` 双源冲突）→ §4 | 登记于 ../edge_todo.md |
| edge | E-ISKMESS | A-3 GET_KMESSAGES 等价通道（kernel 侧新子请求）→ §4 | 登记于 ../edge_todo.md |
| edge | E-ISBOOT | IS 启动链（rc 条件启动 + RS 动态加载）与端到端联调 → §4 | 登记于 ../edge_todo.md |

验证命令基线（2026-09-14 实测，后续修复轮以此为对照）：

- `cargo test -p minix-is`：**86 passed / 0 failed**（与 10 篇 §5.3 声称一致）。
- `cargo clippy -p minix-is --all-targets`：本 crate **0 条告警**（全链 6 条告警全部来自依赖 crate minix-types 1 条 + minix-sys 5 条，已有 edge E-MINSYS-HYGIENE 收口，不属本阶段）。
- Gate E 抽验 5/5：`test_step_tty_notify_dispatches_and_suppresses`（lib.rs:322）、`test_dispatch_visits_matches_in_table_order_without_break`（dispatch.rs:329）、`test_startup_registers_fkey_map`（lib.rs:399）、`test_cursor_pages_22_skips_free_abort_replays`（dump_ds.rs:191）、`test_fold_absorb_and_flush`（dump_vm.rs:219）全部 grep 实测命中。
- Gate A：`python3 tools/coverage-extract/coverage-extract.py is notes/rewrite/fork-syscall-rewrite/08-stage-is --rust-dir os --c-dir minix3/minix/servers/is --semantic-map tools/coverage-extract/is-semantic-map.json --output .review/claude/08-stage-is/SYMBOLS.md` → 30 个 C 符号，文档覆盖 28/30（93.3%），Rust 名称匹配 8/30（26.7%，映射缺失所致，语义判定见 §1.2）；SYMBOLS.md 已落盘 `.review/claude/08-stage-is/SYMBOLS.md`。
- Step 0 预检：`tools/design-coverage-check.sh fork-syscall-rewrite --stage 08-stage-is` → 01~10 三快照全齐 PASS；00/99 缺 outline/outline-review/design 各三件（H.1+H.6 FAIL×2，入 V1-P3-3，不阻断本轮）。

---

## 1. 查漏补缺：三向覆盖矩阵与常量对账

### 1.1 C 函数全集 → Rust 三向判定

C 侧全集 = `servers/is/` 8 个 .c 的 33 个函数定义（plan §7.1 计数）+ 协议面常量。`is-semantic-map.json` 现有 25 条映射只覆盖 01 篇语义（文件头自注），02~10 的语义判定按 06-stage-sched 先例手工补齐。逐组结论：

| C 组（函数数） | 文档 | Rust 对应物 | 判定 |
|---|---|---|---|
| main.c 6 函数（main/sef_local_startup/sef_cb_init_fresh/sef_cb_signal_handler/get_work/reply） | 01 | `main.rs:12`、`IsServer::startup/step/run`（lib.rs:77-128）、`SefCallbacks::{init_fresh,signal_handler}`（lib.rs:190-219）、`SefTransport::receive`（sef.rs:107） | 语义全覆盖（seam 级；生产 transport 未接 → edge E-ISWIRE） |
| dmp.c 4 函数 | 02/03 | `request_fkey_map`（lib.rs:172）+ `map_unmap_keys`（tty_fkey.rs:201）、`handle_fkey_pressed`（lib.rs:146）、`key_name`（dispatch.rs:179）、mapping 格式常量（dispatch.rs:209-211） | 逻辑全覆盖；`mapping_dmp` 打印体缺（A-6） |
| dmp_kernel.c 13 函数 | 05 | 编码器 ×3（dump_kernel.rs:139/158/176）、PageCursor（:239）、kmess_start（:270）、expand_newlines（:283）、NameClass（:317）、12 项格式常量（:341-368）；8 个 dump 体缺 | 快照/编码/分页层全覆盖，执行体缺（A-6） |
| dmp_pm.c / dmp_fs.c / dmp_rs.c / dmp_ds.c / dmp_vm.c（3+2+2+1+2） | 06/07/08/09/10 | dump_pm/vfs/rs/ds/vm 五模块的快照 + 编码器 + 游标 + 格式常量齐备；10 个 dump 体缺 | 同上 |
| glo.h/inc.h | 99/排除 | 不适用 | `DIAG_BUF_SIZE`（glo.h:6）与 `_SYSTEM`（inc.h:7）两条 Gate A「缺口」实为排除项，plan §5.4 排除表未列名 → V1-P3-3 补标注 |

结论：**函数级零遗漏**（不含已声明排除项）；26 个「有文档无 Rust」的 SYMBOLS 条目全部落在 `run_dump` 空体（lib.rs:135）这一条延后线上，与各篇 §3 末「体延后（A-6）」决策逐一对应。缺口不是被遗忘，是被排队——但排队项的总账（A-6 通道 + 数据出口 + producer 对齐三件事互相咬合）此前没有文档收口，本轮 §2 条目与 §4 edge 条目补上。

### 1.2 常量对账表（99 篇收口的证据基础）

| 常量族 | C 出处 | Rust 权威位置 | 判定 |
|---|---|---|---|
| FKEY_MAP/UNMAP/EVENTS、F1~F12/SF1~SF12、Mess*LsysTtyFkeyCtl | com.h:874-877、keymap.h、ipc.h:1447-1454 | `minix-types::ipc::tty`（02 篇 A-1 兑现，实测存在） | ✅ 单一权威 |
| GET_*（8 项）、SI_*、DIAGCTL_CODE_STACKTRACE、SYS_GETINFO/SYS_DIAGCTL、PM/VFS_GETSYSINFO | com.h:315-345、sysinfo.h:11-17、callnr.h | `minix-types::ipc::sysinfo`（acquire.rs:14-18 全量 import，本地零定义） | ✅ 单一权威 |
| VMIW_STATS/USAGE/REGION、VM_INFO | com.h:729-734 | `minix-types::ipc::vm` | ✅ 单一权威 |
| NOTIFY_MESSAGE、TTY_PROC_SLOT | com.h:90/64 | `dispatch.rs:20/24`（01 篇 §4.2 权威位置声明与实际一致） | ✅ |
| SIGTERM | sys/sys/signal.h:67 | `sef.rs:16` | ✅ |
| EDONTREPLY | sys/errno.h:199 | `minix-types` errno.rs:101（本 crate 只 import） | ✅ |
| LINES/VM_LINES/MORE_MARKER/MORE_CR、各 dump 标题列头 | 各 dmp_*.c | 各 dump 模块内（05 篇 §4.2 体例：各篇独立不交叉） | ✅ |
| SI_PROC_TAB（**重复定义**） | sysinfo.h:11 | minix-types 权威之外，`os/servers/vfs/src/misc.rs:66` 本地再定义 `pub const SI_PROC_TAB: u32 = 2` | ⚠️ 跨 crate 重复（值一致、类型 i32/u32 漂移）→ 并入 edge E-ISPROD |

---

## 2. 本轮条目

### V1-P1-1 fkey 注册失败告警被吞：违反 02 篇不变量 + 偏离 C 行为 ✅ 已修复 2026-09-15（Fix #1，见 §8）
**问题**：C 在注册失败时打印告警——`if (s != OK) printf("IS: warning, fkey_ctl failed: %d\n", s);`（`minix3/minix/servers/is/dmp.c:63-65`，实测确认）。02 篇把这条钉成不变量：「MAP 失败（非 OK）→ 调用方告警，不 panic」（`02-is-fkey-contract.md:509`），`map_unmap_keys` 的模块注释也复述「Failure warns at the caller, never panics」（`os/servers/is/src/tty_fkey.rs:197-200`）。但唯一的生产调用方 `request_fkey_map` 把错误整体吞掉：`Err(_) => { self.fkey_mapped = false; Ok(OK) }`（`os/servers/is/src/lib.rs:182-185`）——既无告警通道可调（`SefTransport` 只有 `warn_illegal` 与 `warn_fkey_events`，sef.rs:110-118），也不把状态留给调用方。
**影响**：注册静默失败正是 02 篇 §2.8 描述的「半开状态」——IS 活着、自认已注册（`startup` 返回 `Ok(OK)`），TTY 侧却没有任何观察者，F-key 永远无通知。排障时唯一的可观测线索被这一行吞掉；文档不变量与代码行为不一致还会误导后续维护者以为告警存在。
**建议**（多方案）：① 首选：`SefTransport` 增设 `warn_fkey_ctl(&mut self, status: i32)`（与 03 篇为 dmp.c:84-86 开的 `warn_fkey_events` 同型、同属 A-6 诊断通道；分方法保持 C 调用点可 grep 的既有体例），`Err` 臂调用后照旧返回 `Ok(OK)`（C 的 void 语义保留）。② 次选：复用 `warn_illegal` 通道——否决：C 的两条告警格式串不同（`fkey_ctl failed` vs `illegal request`），合并会让日志无法按调用点区分。③ 最省：给 `Err` 臂加注释声明「告警延后至 A-6」——否决：`warn_fkey_events` 已经为同类告警开了通道，同一批 printf 只处理一半没有理由。
**验证**：修后 `rg -n "warn_fkey_ctl" os/servers/is/src/` 应命中 trait 声明 + `request_fkey_map` Err 臂 + fake 实现 ≥3 处；新增「MAP 失败 → 恰一次告警 + 仍 Ok」单测（现 `FakeFkey` 恒 OK，`lib.rs:182-185` 失败臂当前零测试覆盖，一并补）。

### V1-P1-2 取数五通道 trait 无数据出口：run_dump 填体的前置设计缺口 ✅ 已修复 2026-09-15（Fix #5，见 §8；执行面拆出 V1-P1-3）
**问题**：五个取数通道的 trait 签名全部只回状态码、不携带数据：`fn sys_getinfo(&mut self, req: GetRequest) -> i32`（`os/servers/is/src/acquire.rs:106-108`）、`fn getsysinfo(&mut self, who: Endpoint, what: SiWhat, len: usize) -> i32`（:145-147，有长度参数没有目的地参数）、`VmInfoTransport` 的 `stats/usage/region` 三个方法均无快照出参（:172-176）、`KerninfoTransport` 只有 `kmessages_available() -> bool`（:131-134）。C 语义是「数据拷到调用方」：`sys_getinfo` 的 `endpt = SELF`（04 篇 §2.1「内核永远存到调用方」）、`getsysinfo` 服务侧 `sys_datacopy(SELF → who_e)`（04 篇 §2.4）。04 篇声明的签名与此一致（`04-is-data-acquisition.md:348`），即文档与代码自洽，但这套签名表达不了「把表给我」——05~10 的 dump 体要消费 `KProcSnap`/`MProcSnap` 等快照时，从任何 trait 都拿不到数据。
**影响**：这是 A-6 之外 `run_dump` 填体的第二道前置：16 个 dump 体全部无法实现；届时五个 trait 签名必然返工，连带 `FakeAcquires`（acquire.rs:277-338）与全部相关单测。现在不定型，填体时就是无设计依据的被动改动——与用户警告的「不慎重就会设计不好或沦为 translate」正对的正是这类缝。
**建议**（多方案，与 A-6 输出通道同批定型）：① 首选：快照类型化出参——按域扩方法，如 `SysGetinfoTransport` 增 `fn get_proctab(&mut self, out: &mut [KProcSnap]) -> i32`、`GetSysinfoTransport` 的 `len: usize` 升格为 `out: &mut [MProcSnap]` 等；理由：A-4 快照本就是 wire 契约提案（#[repr(C)]），kernel/PM 的 producer 直接往调用方缓冲填充，与 C 的拷贝语义同构，且保留「长度精确匹配」契约（slice 长度即 len）。② 次选：裸字节缓冲 `out: &mut [u8]`——更贴近 C 指针语义，但把布局解释推回 dump 层，丢掉类型安全，等于在 Rust 里复刻 C 的 void*（translate 味，模式 16）。③ 否决：返回 owned `Vec<Snap>`——crate 现为零分配设计（无 `extern crate alloc`，lib.rs:1，V1-P3-4 有注），为 dump 面引入 alloc 依赖得不偿失。同批必须定案的相邻决策：A-6 输出通道选型（见 §3 L2 与 §5 对照参考：倾向 `core::fmt::Write` 形 sink trait，dump 体用 `write!` 直写，格式串常量已逐字就位）。
**验证**：设计落档后 `rg -n "&mut \[KProcSnap\]|&mut \[MProcSnap\]|&mut \[DsEntrySnap\]" os/servers/is/src/acquire.rs` 应逐域命中；`cargo test -p minix-is` 基线 86 passed 不回退（fake 同步升级后旧断言存活）。

### V1-P1-3 run_dump 16 体填实（05~10 执行面）——V1-P1-2 派生的开放条目 ✅ 已修复 2026-09-15（Fix #6，见 §8）
**问题**：接缝定型后（Fix #5），`run_dump`（lib.rs，现为空体）的 16 个 `DumpId` 臂背后没有执行体——C 侧 dmp.c/dmp_kernel.c/dmp_pm.c/dmp_fs.c/dmp_rs.c/dmp_ds.c/dmp_vm.c 的 printf 渲染面（约 700 行）在 Rust 侧零对应。这是各篇"体延后（A-6）"决策的兑现轮。
**建议**：每臂 = 取数（经 `Acquires` 类型化方法，`!= OK` 即告警续走）→ 调该域自由渲染函数（入参：诊断 sink + 快照 + 该域游标实例）→ `write!` 直写；游标实例收拢为 `IsServer` 的分域状态结构（C 各 `static prev_i`/`oldrp` 的实例化对应物）；格式串常量已逐字锁定直接复用。C 的 `%-8.8s`/`%10s`/`%08x` 等格式规格逐条映射 Rust `write!` 的宽度/精度/对齐语法，行为以渲染输出与 C 格式串语义对拍为验收。分域推进：kernel 8 体 → pm+vfs 4 体 → rs+ds+vm+mapping 4 体。
**验证**：每域落地后 `cargo test -p minix-is` 对照基线；各体至少一条渲染输出断言（fake 取数 + fake sink）；文档 05~10 各篇 §3 末"体延后"决策注记翻转。

### V1-P2-1 `minix-sys` 死依赖 ✅ 已修复 2026-09-15（Fix #2，见 §8）
**问题**：`os/servers/is/Cargo.toml:14` 声明 `minix-sys = { workspace = true }`，但全 crate `rg "minix_sys" os/servers/is/` 零命中（实测）——生产接线尚未开始（main.rs:19-23 用 fail-closed 占位），依赖先挂上了。
**影响**：构建图多一条假边；读者会误以为 IS 已消费 minix-sys 的某些面（恰与「接线 pending」的真实状态相反）。
**建议**：删除该行，E-ISWIRE 接线时按实际消费再加回（fix-guard 单条修复，删后 `cargo test -p minix-is` 对照基线 86 passed）。否决「保留作 forward reference 锚」：forward reference 由 main.rs:19-23 注释承担，依赖清单不该表达意图。
**验证**：修后 `rg -n "minix-sys" os/servers/is/Cargo.toml` 零命中；`cargo test -p minix-is` 86 passed。

### V1-P2-2 只写不读的两个状态位：`fkey_mapped` 与 `state.call_nr` ✅ 已修复 2026-09-15（Fix #3，见 §8）
**问题**：`IsServer.fkey_mapped`（`os/servers/is/src/lib.rs:65`）生产代码只写不读（:179/:183/:217 三处写，读仅测试 :405/:420）；`IsServerState.call_nr`（`os/servers/is/src/state.rs:30`）同病（lib.rs:95 写入后，:97/:105 分类用的都是局部变量）。C 没有这两个状态的对应物——C 的 `callnr` 全局就是被主循环读的，Rust 侧分类改用局部量后，state 字段成了遗迹；`fkey_mapped` 则是 C 全然没有的 Rust 发明（01 篇 §2.8 的「先 unmap 后 exit」顺序由 `signal_handler` 的语句序保证，不需要标志位）。
**影响**：读代码的人会以为存在「注册在案」的语义消费（例如防止重复注册或 shutdown 判断），实际没有任何行为依赖——死状态比没有状态更误导。
**建议**：首选：删除两处（`fkey_mapped` 连带 lib.rs:71/:179/:183/:217 与测试 :405/:420 的断言改为断言 `fkey.calls` 的调用记录——`test_signal_term_requests_shutdown` 改查 FakeFkey 收到过 UNMAP 请求，行为等价且更贴 C 锚点）。次选：若想保留注册可见性，让 `step`/`signal_handler` 消费它（如 TERM 时 `if self.fkey_mapped` 才发 unmap）——否决：这是无 C 依据的行为发明，01 篇 §2.8 的 C 语义是无条件 unmap。
**验证**：修后 `rg -n "fkey_mapped" os/servers/is/src/` 零命中；`rg -n "pub call_nr" os/servers/is/src/state.rs` 删除后 `IsServerState` 仅剩 inbox/reply_buf/caller 三个有读方的字段；86 passed 基线保持。

### V1-P3-1 两个栈数组的 16 长度隐式耦合 ✅ 已修复 2026-09-15（Fix #4，见 §8）
**问题**：`handle_fkey_pressed` 用 `let mut matched = [DumpId::Proctab; 16]` + 手工计数承接分派（`os/servers/is/src/lib.rs:151-158`），`request_fkey_map` 用 `let mut keys = [FkeyId::F1; 16]` 预填后 zip HOOKS（:173-176）。两处的 16 都来自「HOOKS 恰好 16 项」（dispatch.rs:127）这一事实，但没有任何编译期检查：HOOKS 缩到 15 项时 `request_fkey_map` 会静默多注册一个 F1（预填残留），扩到 17 项时 `matched[n]` 越界 panic。
**影响**：当前正确，但耦合靠人记；hooks 表是「注册集合 = 转储能力集合」的心脏（02 篇 §2.6），改表时的静默漂移最危险。
**建议**：首选：`request_fkey_map` 改用 `let keys: [FkeyId; 16] = HOOKS.map(|h| h.key);`（`tty_fkey.rs:365` 的测试里已有同款先例，无预填、无 zip、长度由类型携带）；`matched` 处加 `const _: () = assert!(HOOKS.len() <= 16);` 把耦合变成编译期断言。次选：`matched` 改为遍历内直接 `self.run_dump(hook.dump)`——否决：闭包借 `self` 与 `dispatch_each` 的访问者签名冲突，绕开要么引入 alloc 要么 unsafe，不值。
**验证**：修后 `rg -n "HOOKS.map" os/servers/is/src/lib.rs` 命中；临时把 HOOKS 注释掉一项跑 `cargo test -p minix-is`，`request_fkey_map` 相关测试应编译期失败或断言失败（验证后还原）。

### V1-P3-2 `request_fkey_map` 的 `Result` 类型谎言（随 P1-1 同批修）✅ 已修复 2026-09-15（Fix #1，见 §8）
**问题**：签名 `fn request_fkey_map(&mut self, map: bool) -> Result<i32, Errno>`（lib.rs:172）两臂都返回 `Ok(OK)`（:178-185）——这是 01 篇时代的 ENOSYS 桩遗迹（当时失败臂返回 `Err(ENOSYS)`），02 篇填实后错误信号被降级进 `fkey_mapped` 标志，`Result` 外壳成了永不 `Err` 的类型谎言。
**影响**：调用方（`init_fresh`/`signal_handler`）被迫写 `let _ = ...`，错误路径的类型表达名存实亡；「签名不再描述行为」与 V1-P2-2 同根。
**建议**：与 V1-P1-1 同一个 todo-fix 批次处理（fix-guard 一次一条、但同一函数的两处病变同批最省重读成本）：P1-1 落地后该函数对外语义仍是 C void（注册失败不影响 boot），故首选把签名改为 `-> Result<i32, Errno>` 保留但让 `Err` 臂真正传播 `Errno`（`init_fresh` 侧 `let _` 显式降级，注释引 dmp.c:63-65）——或者更诚实：改返回 `()`，告警即出口。两案在实施时按 02 篇签名锚（`01-is-init-main.md:594` 声明 `startup -> Result` 不受影响）择一并同步文档。
**验证**：`rg -n "fn request_fkey_map" os/servers/is/src/lib.rs` 签名与最终行为一致；文档 01 §4.2 签名行同步。

### V1-P3-3 00/99 篇骨架与 `.design/` 快照缺失 + 排除表两处漏标
**问题**：`tools/design-coverage-check.sh fork-syscall-rewrite --stage 08-stage-is` 实测 00/99 各缺 outline/outline-review/design 三快照（H.1+H.6 FAIL×2）；两篇正文也是 pending 最小骨架（各 20 行左右）。同时 Gate A 的 30 个 C 符号里仅有的两个「无文档无 Rust」缺口——`DIAG_BUF_SIZE`（glo.h:6）与 `_SYSTEM`（inc.h:7）——属于排除项，但 plan §5.4 排除表只列了 `diag_buf` 等 5 个死 extern，没列这两个。
**影响**：Gate H 对 00/99 判 FAIL（模式 69 PSMD 风险面）；99 篇收口后 Gate A 才能拿到 30/30 的干净对账。
**建议**：随 99 篇改写轮一次做完：① 00/99 按 plan §6 路线成文（99 收口时把 §1.2 常量对账表直接吸收为正文）；② 成文时按 Step 0.3 补三快照（sched 先例：06-stage-sched todo P3-3 → Fix #14 同款）；③ plan §5.4 排除表补 `DIAG_BUF_SIZE`、`_SYSTEM` 两行（依据：`rg -rn "diag_buf" minix3/minix` 仅 glo.h 声明，与同表已排除的死 extern 同族；`_SYSTEM` 是 C 头文件包含协议宏，无运行时语义）。
**验证**：`tools/design-coverage-check.sh fork-syscall-rewrite --stage 08-stage-is` 全 PASS；重跑 Gate A「完全缺口」归零。

### V1-P3-4 文档声称 `extern crate alloc` 与实际不符 ✅ 已修复 2026-09-15（Fix #7，见 §8）
**问题**：01 篇 §4.1 写「`lib.rs` 头部与 RS 对齐（`#![cfg_attr(not(test), no_std)]` + `extern crate alloc`，`os/servers/rs/src/lib.rs:1-29` 同款）」（`01-is-init-main.md:563`），但实际 `os/servers/is/src/lib.rs` 没有 `extern crate alloc`（grep 实测零命中），全 crate 零分配（全部定长栈数组）。
**影响**：文档描述了一个不存在的依赖事实；后续按文档接线的人可能引入不必要的 alloc。
**建议**：修文档：该句改为「no_std 门同款，且本 crate 无 `extern crate alloc`——全部逻辑零分配（03 篇 §3.3「回调式零分配」的实现面兑现）」。否决反向修代码（补 alloc）：现状更优，V1-P1-2 方案③ 的否决理由也依赖这一点。
**验证**：`rg -n "extern crate alloc" notes/rewrite/fork-syscall-rewrite/08-stage-is/01-is-init-main.md` 修后不再声称 IS 有 alloc。

### V1-P3-5 五个分页游标类型并存（审查结论：维持）
**问题**：`PageCursor`（dump_kernel.rs:239，先比较 `>=22`）、`PmCursor`（dump_pm.rs:99，候选断 `>22`）、`VfsCursor`（dump_vfs.rs:156，与 PmCursor 逐行同构）、`DsCursor`（dump_ds.rs:103，条件界 + 早返重放）、`BatchCursor`+`FoldState`（dump_vm.rs:101/162，双游标）五种分页状态机并存。`VfsCursor` 与 `PmCursor` 的重复已在注释里声明是有意的（dump_vfs.rs:152-154：「06 is CONVERGED and its `Pm`-prefixed name would lie here. Logic intentionally duplicated」）。
**影响**：约 40 行 ×2 的逻辑重复；未来若 C 侧游标语义修正需双改。
**建议**：维持现状。理由：三种断点形状（先比较/候选断/条件界）与两种续跑语义（回绕/重放）是 C 三份源码的忠实镜像，抽象成带策略参数的统一 `Pager` 会把 C 的怪味藏进配置项——对拍审计时反而要同时读抽象层和 C 源（translate 防线的反面教材形态）。Redox 的做法可参照：其 scheme 实现同样允许每个 handler 持有自己的状态机而非强制共享基类。若未来 PM/VFS 游标再次同变，再立合并条目（触发条件写入本条目，复查轮次：下一次触碰任一 dump_*.rs 的轮次顺带复核）。
**验证**：无（维持性记录，不修代码）。

---

## 3. 五层架构深审记录

### L0 组合层：crate 在 workspace 中的位置
- 依赖面健康：仅 `minix-types`（workspace path）+ 死依赖 `minix-sys`（V1-P2-1）。常量消费全部来自 minix-types 单一权威（§1.2 对账），无本地重复——比 VFS crate 本地重定义 `SI_PROC_TAB` 的做法干净（该问题归 edge E-ISPROD）。
- 消费面为零：`rg -rln "minix_is|minix-is" os` 除 crate 自身与 Cargo.lock 外零命中（实测）——IS 未被任何启动路径引用，与 C 的「条件性 debug 服务」定位一致（A-9/A-11），启动链缺口归 edge E-ISBOOT。
- crate 组织与其它 server 惯例一致（lib.rs 编排 + 分域模块 + main.rs test 门 + fail-closed 占位，RS/DS 同款）；模块头注释均带文档锚与执行模型声明（单线程事件循环），review-core 执行模型检查通过。

### L1 服务器内部：编排与状态
- `IsServer<T, F>` 双泛型编排落实了 01 篇 D1-D5：四静态收拢进 `IsServerState`、分类器纯函数、回复门抑制、transport panic / 业务 warn 分界（lib.rs:88-119 注释逐条引 C 行号，实测与 main.c 对应段一致）。
- `step` 的 Suppress 臂正确复原了 C 的不对称告警（非 notify 告警、非 TTY notify 静默，lib.rs:99-109），并有 T11a/T11b 双测试钉住——这是 C 侧 FIXME 的忠实保留，注释明确警告「不要好心补日志」。
- 发现集中于 `request_fkey_map`（P1-1/P2-2/P3-1/P3-2 四条病变聚在一个函数——修复时按 fix-guard 逐条处理但建议同一轮做）与 `handle_fkey_pressed` 的数组耦合（P3-1）。
- `run_dump` 空体（lib.rs:135）是文档化延后而非遗漏；其解锁前置 = A-6 输出通道（plan.md:165「待设计」）+ V1-P1-2 数据出口 + edge E-ISPROD producer 对齐，三者缺一不可。

### L2 接缝层：trait 粒度与缺失的第 8 个缝
- 7 个传输 trait（Sef/FkeyCtl/SysGetinfo/Diagctl/Kerninfo/GetSysinfo/VmInfo）的细缝选择维持：04 篇 §3 D2 的论证（各转储域各取子集、请求码枚举限域）成立，且 `UnimplementedAcquires` 打包实现五通道作为 fail-closed 占位并无张力。
- 真正的缺口是**接缝清单不全**：A-6 诊断输出通道至今没有 trait——16 个 dump 体 + 3 处告警 printf 都悬在它上面。设计倾向（供后续 todo-fix 展开）：`trait DiagOut { fn write_str(&mut self, s: &str) -> ... }` 或直接实现 `core::fmt::Write`，dump 体 `write!(out, "{}", TITLE)` 直写，格式串常量（已逐字锁定 12+ 项）零改动复用；C 的 22 行分页 `--more--` 标记本身就是写进输出流的字符串（MORE_MARKER/MORE_CR 常量已就位），分页游标产 Emit/More 动作、通道只管写字——游标与通道正交，两层可独立单测。对照与备选见 §5。

### L3 wire 层：常量、快照、消息契约
- 常量权威位置核查通过（§1.2，9 族里 8 族单一权威）。
- A-4 快照提案的执行风险被本轮坐实：kernel 已有 `ProcInfoStruct`（`os/kernel/src/misc.rs:371` 起，GET_PROCTAB 的生产布局）与 IS 的 `KProcSnap`（dump_kernel.rs:28）是两个二进制不兼容的 `#[repr(C)]`——字段序完全不同、时间字段 u64 vs i32、kernel 侧多 p_misc_flags/p_cpu/p_cpu_time_left/p_cycles/p_pending 五个 IS 未建模的字段。快照提案写作时（05 篇定稿 2026-08-16）kernel producer 尚未就位，现在双源已成事实。对齐方案与决策点（快照权威上收 minix-types vs 各 crate 对齐约定）登记 edge E-ISPROD。
- DS 侧：`os/servers/ds/src/lib.rs:70` 已有 `getsysinfo` 模块（07-stage-ds A-10 生产侧就位），对齐核查归入 E-ISPROD 同批。

### L4 测试层：86 测试的五维抽查
- 完备性：分类器真值表、生命周期、fkey 状态机镜像、游标边界（21/22/24、跳过不计数、早返重放）、编码器含 SENDA 负掩码边角——核心分支覆盖良好；盲区：`request_fkey_map` 失败臂零测试（V1-P1-1 连带补）、run_dump 后需要分页断点续跑的整链路测试（A-6 落地轮补）。
- 自身正确性：抽验的断言均与 C 锚点对拍（`test_s_traps_full_and_zero` 显式论证了 short 整型提升与 `as u32` 的等价性，dump_kernel.rs:384-387；`test_22_emit_then_more` 钉住 `>22` 与 05 的 `>=22` 差异）——未发现 assert 永真或测试与描述不符。
- 冗余/无效：`FakeTty` 镜像与 `FakeFkey` 容纳双职工（既有协议镜像又有编排断言）但职责分层清楚；`DsCursor::resume_for_test` 是 `#[cfg(test)]` 专用构造器（dump_ds.rs:115-118），未泄漏到生产面。未见 `#[ignore]`/空测试。
- 虚构（文档-代码对账）：10 篇 §5.3 声称 86 passed 与基线一致；Gate E 抽验 5/5 命中（§0）。
- 无集成测试目录：可接受——单线程语义已由 fake 接缝覆盖，跨进程联调归 edge E-ISBOOT（判定③），不应在 stage 内造半吊子集成测试。

---

## 4. 边界条目双向指针（唯一入口：`../edge_todo.md`）

| edge 条目 | 来源 | 一句话 | 判定依据 | 08 侧关联 |
|---|---|---|---|---|
| E-ISWIRE | 本轮 V1 | IS 生产 transport 接线：minix-sef 实装（sef_startup/sef_receive ping 拦截/ipc_send）+ minix-sys `_taskcall`（fkey_ctl）+ 替换 `UnimplementedTransport`/`UnimplementedFkeyCtl`（main.rs:17-23、sef.rs:120-148、tty_fkey.rs:176-182） | ①（minix-sef/minix-sys 共享基建） | V1-P2-1 随接线复原依赖 |
| E-ISPROD | 本轮 V1 | GETSYSINFO/GET_*/VM_INFO producer 与 IS 快照对齐：kernel `ProcInfoStruct`↔`KProcSnap` 双源冲突裁决、PM/VFS/RS/DS 布局对齐（6 个 TODO(P1)）、VFS misc.rs:66 常量重复收敛 | ②（对方 stage 生产代码） | V1-P1-2 的快照类型化出参以其为前提 |
| E-ISKMESS | 本轮 V1 | A-3 的 kernel 侧新通道：`do_getinfo` 增 GET_KMESSAGES 等价子请求 + minix-types 常量 + IS `KerninfoTransport` 填实（acquire.rs:195-199，现 fail-closed） | ①+②（共享契约 + kernel 生产代码）；与 E-KERNINFO（kerninfo 共享家族）交叉引用，按 04 篇既定决策走 sys_getinfo 子请求、不走 usermapped | 05 篇 KmessagesSnap/kmess_start 已备好消费面 |
| E-ISBOOT | 本轮 V1 | IS 启动链与端到端联调：rc/system.conf 等价物（debug_fkeys 条件启动 + `-period 5HZ` 存活 ping）、RS 动态加载 IS、TTY 侧 fkey 观察者（TTY crate 未 staging） | ③（多进程联调） | 依赖 E5（联调包）+ E-ISWIRE |

---

## 5. 对照参考（联网核实，2026-09-14）

- **Redox `sys:` scheme**：Redox 内核自带 `sys` scheme（`src/scheme/sys/` 下 context/cpu/iostat/log 等模块），把进程表、CPU、打开文件、内核日志做成可 `cat` 的虚拟文件（`cat /scheme/sys/iostat` 是官方排障手段）。它是 Minix3 IS 的现代对应物：同样「内核状态不直接碰」，但消费面从「专用服务器 + 功能键」演进为「万物皆文件」。对本轮的启示：① A-6 输出通道落定后，可把「诊断作为 scheme/文件」列为远期 Architectural Evolution 候选（需三处一致标注 + 用户显式批准，不在 rewrite 范围）；② IS 现在的 trait 化接缝（V1-P1-2 的类型化出参）恰好是未来把 dump 面改挂 scheme 的最小改动路径——trait 边界就是两种消费形态的切换点。来源：[Redox Book — Schemes](https://doc.redox-os.org/book/schemes.html)、[Redox Book — Troubleshooting](https://doc.redox-os.org/book/troubleshooting.html)、[FOSDEM 2025 Redox slides](https://archive.fosdem.org/2025/events/attachments/fosdem-2025-5973-redox-os-a-microkernel-based-unix-like-os/slides/238806/redoxos-a_VThTapJ.pdf)。
- **Linux `/proc`/debugfs 对照**：04/05 篇已用「内核把表拷给用户态 vs 用户态直接读虚拟文件」讲清了 IS 与 procfs 的取舍，本轮无新发现、不重复。
- **Rust 社区惯例（A-6 输出通道选型输入）**：no_std 生态的输出事实标准是 `core::fmt::Write`（`write!` 直落 sink，embedded-hal/embedded-io 均按此分层）；反面参照是 James Munns 对 `core::fmt` 在裸机上代码体积与耗时开销的量化（格式化不便宜）——对 IS 这种低频调试路径完全可接受，但提示 sink 实现应避免热路径格式化。IS 的格式常量已逐字锁定（`%10s` 宽度、`\r` 分页符等），用 `write!` 复刻 C 格式串是直译安全区；动态字段（`%-8.8s` 名字截断、`%lu kB` 缩除）在 dump 体落地时逐个处理。来源：[embedded-io](https://docs.rs/embedded-io)、[Formatting is Unreasonably Expensive for Embedded — James Munns](https://jamesmunns.com/blog/fmt-unreasonably-expensive/)、[rust-embedded/discovery#593（core::fmt::Write over UART 实例）](https://github.com/rust-embedded/discovery/issues/593)。

---

## 6. Rule Discovery（Step 5.7）

本轮发现两个候选新模式（各 1 例，未达「≥2 例才立项」门槛，先记录待复现）：

1. **「填桩必审签名」（SFS, Stub-Fill Signature re-audit）**：01 篇时代的 ENOSYS 桩填实后，`request_fkey_map` 的 `Result` 外壳、`fkey_mapped` 标志、`state.call_nr` 都成了遗迹（V1-P1-1/P2-2/P3-2 三条同源）。候选规则：todo-fix 填实任何 fail-closed 桩时，强制重审该函数签名与相邻状态位的存续必要性（grep 读写点）。目标文件：prompt/skill/review-patterns-skill.md（代码族）。
2. **「契约提案先查现存布局」（WCE, Wire-Contract Existing-layout scan）**：IS 六组快照提案未对照 kernel 已落地的 `ProcInfoStruct`，形成双源冲突（edge E-ISPROD 的根因）。候选规则：任何文档声明 `#[repr(C)]` wire 契约提案前，先 `rg "repr\(C\)" os/ -t rust` 对同语义结构（对端生产者/消费者）做存在性扫描，命中即须在提案里写「对照/收编/并存」三分决策。目标文件：同上。

---

## 7. 建议推进顺序

1. **V1-P1-1 + V1-P3-2 同批**（同一函数的告警与签名，一次 todo-fix；连带补失败臂测试）。
2. **V1-P2-1 + V1-P2-2 + V1-P3-1 清理批**（死依赖、死状态、长度耦合；三条互相独立，可分三次 todo-fix，也可用户显式说「修 3 个」一批）。
3. **V1-P1-2 设计轮**（A-6 输出通道 + 取数数据出口 + 游标/通道正交性，一次 todo-fix 先出设计对比再动代码；这是 run_dump 实施的总闸）。
4. **edge 四条目**按依赖序单线程执行：E-ISWIRE（等 E1/E2）→ E-ISPROD（可与 3 并行设计）→ E-ISKMESS → E-ISBOOT（等 E5）。
5. **V1-P3-3/V1-P3-4 文档批**（00/99 成文 + 快照 + 排除表补漏 + alloc 声称修正；与代码修复无依赖，随时可插队）。

---

## 8. 修复记录（2026-09-15 起，一次一个 TODO，每条一个提交）

### ✅ Fix #1: V1-P1-1 + V1-P3-2 — 注册失败告警通道化 + 签名诚实化

**设计对比**（两设计点各两案，摘要——全文见条目）：告警通道首选 `SefTransport::warn_fkey_ctl(status)` 新方法（与 03 的 `warn_fkey_events` 同型，C 调用点保持可 grep），否决复用 `warn_illegal`（两条 C 格式串不同，合并丢调用点区分）；签名首选改 void（C `map_unmap_fkeys` 本身 void，失败处理完整发生在函数内部，告警即出口），否决保留 `Result` 真传播（调用方语义是 C void，传播后还得 `let _` 丢弃，谎言移位不消除）。

**Files**：`os/servers/is/src/sef.rs`（trait 增 `warn_fkey_ctl` + `UnimplementedTransport` fail-closed 臂）、`os/servers/is/src/lib.rs`（`request_fkey_map` 改 void + Err 臂告警；`init_fresh`/`signal_handler` 调用点去 `let _`；FakeTransport 增 `ctl_warnings`；FakeFkey 增可失败 `map_status`；新增测试）。

**测试**：新增 `test_fkey_map_failure_warns_and_boot_survives`（lib.rs——EPERM 拒 MAP：告警恰一次 + `startup` 仍 `Ok(OK)` + 无注册在案 + 恰一次 MAP 尝试）；基线 86 → **87 passed / 0 failed**；clippy 本体 0 告警。

**Verified**：`rg -n "warn_fkey_ctl" os/servers/is/src/` 命中 trait 声明 + fail-closed 臂 + fake 实现 + Err 臂 4 处；`rg -n "Result<i32, Errno>" os/servers/is/src/lib.rs` 仅剩 `startup`/`init_fresh`（SEF 契约，正确保留）。

**Docs**：01 篇 §3 D3 增 V1 轮更新注记（签名演进史补全）+ §4.2 SefTransport 签名行补 `warn_fkey_ctl` + §5.2 增 T14 行（Gate E 对账）。02 篇 §4.3 不量 2 无需改动——文档本来就是对的那半，这次是代码追上文档。

### ✅ Fix #2: V1-P2-1 — minix-sys 死依赖删除

**设计对比**：首选删除 + Cargo.toml 留一行 forward-reference 注释（意图由注释表达，依赖清单只表达事实）；否决保留作「接线锚」（main.rs:19-23 的注释已承担 forward-reference 职责，依赖清单不该表达意图——条目原文的否决理由）。
**Files**：`os/servers/is/Cargo.toml`（删 `minix-sys = { workspace = true }`，留注释指向 E-ISWIRE）。
**测试**：`cargo test -p minix-is` **87 passed / 0 failed**（基线不动）；`os/Cargo.lock` 中 minix-is 依赖表已只剩 minix-types（实测一致）。
**Verified**：`rg -n "minix-sys = " os/servers/is/Cargo.toml` 零命中；src/ 内 `minix_sys::` 代码引用本就为零（注释里的 forward reference 保留）。
**Docs**：无需正文同步——05~10 篇「producer 对齐待办」讲的是布局契约，与该依赖无涉；E-ISWIRE 的解锁后工作已写明「按实际消费加回」。

### ✅ Fix #3: V1-P2-2 — 删除只写不读的 `fkey_mapped` 与 `state.call_nr`

**设计对比**：首选全删（读代码的人会以为存在"注册在案"的语义消费，死状态比没有状态更误导；C 无对应物）；否决让 `step`/`signal_handler` 消费标志位（如 TERM 时有注册才 unmap）——这是无 C 依据的行为发明，01 §2.8 的 C 语义是无条件 unmap。TERM 测试的可观察面改为断言 UNMAP 请求到达 fkey transport（比标志位更贴 C 锚点 main.c:113）。
**Files**：`os/servers/is/src/lib.rs`（删字段 + 三处写点；startup/TERM/failure 三测试改为断言 transport 调用记录）、`os/servers/is/src/state.rs`（删 `call_nr` 字段 + 模块文档说明第四个 C 全局为何无对应物 + 测试改断言 inbox/reply_buf 初值）。
**测试**：`cargo test -p minix-is` **87 passed / 0 failed**（基线不动——纯删除 + 断言迁移）；clippy 本体 0 告警。
**Verified**：`rg -n "fkey_mapped" os/servers/is/src/` 零命中；`rg -n "call_nr" os/servers/is/src/state.rs` 仅剩文档注释里的 C 对照说明；`self.state.caller` 保留（回复门 lib.rs 有读方，非死状态——编辑中曾误删其写入点，回归 diff 复核时发现并当场修正，在此诚实记录）。
**Docs**：01 篇 §3 D1 结构体代码块删 `call_nr` 行 + 增 V1 轮更新注记；§4.2 IsServerState 签名行同步。

### ✅ Fix #4: V1-P3-1 — 16 长度隐式耦合消除（keys 位派生化，matched 位整体消除）

**设计对比**（matched 位三案）：① 整段消除中间缓冲，访问闭包内直接 `self.run_dump(hook.dump)`（已实施）——C 本来就是在匹配循环里直接调 `hooks[h].function()`（dmp.c:93-94），两阶段"先收集再执行"是 Rust 侧发明的幽灵结构；`dispatch_each` 不持有 self，闭包独占捕获 `&mut self` 借用成立，越界 panic 在构造上不可能。② `[DumpId::Proctab; HOOKS.len()]` 保长度耦合（否决：保留了 C 没有的两阶段结构，只修了长度不修形状）。③ const assert（否决：断言只在编译期检查容量，prefill/手工计数的脆弱模式还在）。keys 位两案：① `HOOKS.map(|hook| hook.key)`（已实施，长度随 HOOKS 类型走，prefill F1 残留在构造上不可能；tty_fkey.rs 测试先例）② 保留 prefill+zip 加 const assert（否决：断言挡不住缩短表时的静默残留模式本身）。
**Files**：`os/servers/is/src/lib.rs`（`handle_fkey_pressed` 删 matched 缓冲；`request_fkey_map` 改 HOOKS 派生键表）。
**测试**：`cargo test -p minix-is` **87 passed / 0 failed**（基线不动——行为等价重构，多匹配表序无 break 由既有 `test_step_tty_notify_dispatches_and_suppresses` 与 dispatch 层 T2 钉住）；clippy 本体 0 告警。
**Verified**：`rg -n "; 16\]" os/servers/is/src/lib.rs` 零命中（16 只活在 dispatch.rs 的 HOOKS 类型标注与 C 对账测试里）。
**Docs**：无需同步——03 篇 §4.2 的 `HOOKS: &[Hook; 16]` 类型标注未变（单一权威仍在），被删的字面量无文档锚。

### ✅ Fix #5: V1-P1-2 — 取数接缝定型（类型化数据出口 + A-6 诊断 sink + Acquires 超特质）

**设计对比**（三设计点，摘要——全文见条目）：A-6 输出通道首选 `SefTransport::diag_out() -> &mut dyn core::fmt::Write`（C 的 printf 走 libc stdio→log 驱动，本就是传输接线；no_std 无 stdio；Redox 的进程诊断同样走进程既有 debug 接线而非独立服务面），否决独立 DiagOut trait 第三个泛型参数（一个方法的 trait 换泛型传染）与 log 式全局宏（所有权模型相性差）；数据出口首选**按 C 速记宏 1:1 的类型化出参方法**（`sys_getproctab(dst)` ↔ `get_proctab(out: &mut [KProcSnap])`，slice 长度即容量声明），`GetRequest` 枚举被方法取代（只选方法的枚举无事可做；IS 未发的请求结构性不可表达），否决裸 `&mut [u8]`（void* 复刻，translate）与 owned Vec（引入 alloc）；编排器侧首选 `Acquires` 超特质 blanket 打包（RS `KernelApi` 五域先例），`IsServer` 升三泛型，dump 体仍是只吃"快照 + 游标 + sink"的自由函数，04 §3 D2 细缝决策不破坏。RS 双拉（dmp_rs.c:33-34 全有或全无）收敛为 `rs_tables` 单方法双出参。
**Files**：`os/servers/is/src/acquire.rs`（五 trait 重写为类型化出参；`GetRequest` 删除；`SiWhat`/`getsysinfo_call`/`IS_GETSYSINFO_CALLS` 降格为 wire 助手；`Acquires` 超特质 + blanket impl；Unimplemented/fake 全量同步，fake 改为 OK 时实填出参）、`os/servers/is/src/sef.rs`（`diag_out` 声明 + fail-closed 臂）、`os/servers/is/src/lib.rs`（`IsServer<T, F, A: Acquires>` 三泛型 + 重导出更新 + sink 测试；`acquires` 字段带 forward-reference 注记的临时 `#[allow(dead_code)]`，V1-P1-3 落地时移除）、`os/servers/is/src/main.rs`（三参构造）。
**测试**：新增 `test_diag_out_sink_receives_writes`（A-6 通道写穿断言）+ acquire 侧 `test_monparams_outlet_writes`/出参实填与 ERR 不改写出参断言；基线 87 → **88 passed / 0 failed**；clippy 本体 0 告警。
**Verified**：`rg -n "GetRequest" os/servers/is/src/` 零命中；`rg -n "&mut \[KProcSnap\]" os/servers/is/src/acquire.rs` 命中（类型化出参在位）；`cargo test -p minix-is` 88 passed。
**Docs**：04 篇 §3 D2 增 V1 定型注记 + §4.1/§4.2/§4.3 全量重写（类型化签名 + 新不变量）+ §5 测试表 T1/T3/T5/T6 重排；01 篇 §3 D4 增 A-6 通道定型注记。05~10 篇的"体延后"翻转归 V1-P1-3。

### ✅ Fix #6: V1-P1-3 + V1-P1-4 — run_dump 16 体填实（分域）+ expand_newlines C 忠实修复

**设计对比**（渲染架构三案）：① 首选（已实施）：各域自由渲染函数，入参 = 诊断 sink + 已取快照 + 该域游标——`run_dump` 臂只做取数与分发，体不碰 transport（唯一例外 procstack/vm：C 体本身在循环内调栈回溯/region 拉取，签名按需持通道，其余 14 体保持"先取后渲"）；② 体直接持 Acquires——否决：破坏 04 §3 D2 细缝；③ 输出层用堆缓冲组装——否决：零分配纪律。C 的 `%-8.8s`/`%10s`/`%08x` 等格式规格经 `PCStr`（字节串 Display，手工实现宽度/精度/对齐，lib.rs）逐条映射 `write!`；C 各 static 游标实例化为 `DumpState` 字段。签名改动全部同步：`VfsCursor::push` bool → 三态 `VfsAction`（接线时暴露"跳过 vs 满页"不可区分）；`RsCursor` 新增（IN_USE 跳过形态第四游标）；`RprocSnap` 增 `r_args[512]`（尾列 `%s` 的数据源——原"不进快照"决策与执行面冲突，A-4 契约扩展）；`FProcSnap` 增 `nfds`/`fp_cdev_endpt`（producer 侧计数与 CDEV 端点）；`KPrivSnap` 增 `s_ipc_to`/`s_k_call_mask` 位图字；`ClockTransport::uptime` 新通道（getticks 语义，sigaction 告警列）。
**V1-P1-4（实施中发现的既有函数 bug）**：`expand_newlines` 丢最后一个换行——C 的 `do { e += strlen(e); *e++ = '\n'; } while (*e != 0)` 对 `"a\0b\0\0"` 产出 `"a\nb\n"`（每个字符串终止 NUL 都改写），旧实现与旧测试把 `"a\nb"` 错钉为预期。按 C 语义重写 + 旧测试期望修正。
**Files**：`os/servers/is/src/{lib,dump_kernel,dump_pm,dump_vfs,dump_rs,dump_ds,dump_vm,dispatch,acquire}.rs`（16 臂 + 8 渲染函数组 + PCStr + DumpState + 快照扩展 + Cursor 修订 + V1-P1-4）。
**测试**：基线 88 → **106 passed / 0 failed**（kernel +13、pm +2、vfs +3、rs +1、ds +1、mapping +1、sink 1 等；每体至少一条渲染输出断言，proctab 含 --more-- 断点续跑验证）；clippy 本体 0 告警；`#[allow(dead_code)]`（acquires 字段）已随接线移除。
**Verified**：`rg -n "fn run_dump" -A2 os/servers/is/src/lib.rs` 全臂在位；`rg -n "fn render_" os/servers/is/src/ | wc -l` = 16；TODO(P1) 注释保留（producer 对齐仍归 edge E-ISPROD）。
**Docs**：03/05/06/07/08/09/10 各篇 §3 末增"V1 执行轮更新"注记（体延后翻转 + 契约修订逐条点名）；04 篇 V1 注记已于 Fix #5 落。

### ✅ Fix #7: V1-P3-4 — 01 篇 alloc 声称与实际对齐
**Files**：`01-is-init-main.md` §4.1（"RS 同款 + extern crate alloc"改为"no_std 门同款，但无 alloc——零分配是有意偏离"）。
**Verified**：`rg -n "extern crate alloc" 01-is-init-main.md` 仅剩否定句语境；`rg -n "extern crate alloc" os/servers/is/src/` 零命中（事实侧不变）。
**Docs**：即本修复。否决反向修代码（补 alloc）：零分配是更优状态，V1-P1-2 方案③的否决理由依赖它。