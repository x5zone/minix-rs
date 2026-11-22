DOC 01-vm-init-main.md

- 审查范围：父章 1–5（单元 ID `1`..`5`），V3.12 面板 T1 第二轮修复复核，只读。
- 判定正文 SHA-256：`d97aefe967cc96cdde2d2ae78344b3dd9a9a326b69fe72bd9970193d3ce28fd8`。上一版依据 `adbb5b098bc6e2ab235444c9c0c7105aacfb37979128794aba8bfcd9e107ed98`；本次只重读 §2.2.7 第二条，其余结论沿用。
- 结论：T1B-01..07 全部 RESOLVED，当前开放 FINDING 总数 = 0。
- 判据：PASS = 章首 TR-0 成立、章内承诺有落点、事实与锚点不破坏章级契约；P2 级事实/承诺缺口 → FAIL；P3 级卫生项不自动 FAIL，但逐条列出。
- 验证手段：`minix3/minix/servers/vm/{main.c,alloc.c,pagetable.c,utility.c,acl.c,region.c}` 与 `minix3/minix/lib/libsys/{sef.c,sef_init.c}` 逐段读取并重放 grep；`os/servers/vm/src/{main.rs,boot.rs,global.rs,vm_server.rs,ipc/dispatcher.rs}` 读取；`cd os && cargo test -p minix-vm --lib` 实测 `549 passed; 0 failed; 0 ignored`；`python3 tools/doc-publication-check.py rewrite-notes/02-stage-vm/01-vm-init-main.md` 输出“无新增违规”。
- 未复核项：无。所有 FINDING 与修复证据均在本会话重放。

## 本版判定

| 单元 | VERDICT | 开放 FINDING |
|---|---|---|
| UNIT 1（父章 1） | PASS | 0 |
| UNIT 2（父章 2） | PASS | 0 |
| UNIT 3（父章 3） | PASS | 0 |
| UNIT 4（父章 4） | PASS | 0 |
| UNIT 5（父章 5） | PASS | 0 |

## T1B 状态

| 编号 | 位置 | 状态 | 复核证据 |
|---|---|---|---|
| T1B-01 | §1.4 前提 4 | RESOLVED | 第 125 行现为“boot 条目缺 `start_addr`（`main.c:504` 的断言）都直接 panic；ELF 装载失败在 `main.c:383-385` panic（`main.c:442-452` 是前两项的断言）”。`sed -n '382,386p' main.c` 显示 383-385 是 `libexec_load_elf` 失败 panic；`grep -n 'assert(ip->start_addr)' main.c` 输出 504。两类失败已分开。 |
| T1B-02 | §2.2.6 因果段 | RESOLVED | 第 345 行现为“这个循环排在 `__minix_init` 与 `mem_add_total_pages` 之后、`CALLMAP` 之前，是 C 的源码顺序；真正的前置依赖是 `pt_init()` 建立的页表设施与 `mem_init()` 的位图——`pt_init_done=1` 之后，boot 进程的页目录/页表页由 `alloc_mem()` 从位图供给，保留页池只服务自映射的递归分配……`mem_add_total_pages` 只是记账，不构成分配门，所以它的前后位置不是分配门；`CALLMAP` 只需在主循环之前”。与 C 一致：`grep -rn '\btotal_pages\b' minix3/minix/servers/vm/*.c` 的唯一读点是 `utility.c:118`；`pagetable.c:350` 的 spare 分支只在 `level > 1 || !pt_init_done` 时进入，`pagetable.c:1005-1008` 的 `pt_new` 在 `pt_init_done=1`（1311）后经 `alloc_mem`；`main.c:165` 只在主循环查 `vm_calls[c]`。原先的“必须……之后”与自相矛盾已消除。 |
| T1B-03 | 多处函数行号范围 | RESOLVED | `grep -n '262-291\|592-676\|677-730\|731-754\|592-754\|219-240'` 无命中（exit=1）；现为 `262-286`（第 266、408、410、1000 行）、`219-239`（460）、`592-672`（542）、`677-726`（543）、`731-750`（545、745）、`592-750`（458）。与 C 的 286/239/672/726/750 行闭括号逐一对应。 |
| T1B-04 | §4.4.2/§3.3 `init_vm_slot` | RESOLVED | 第 1000 行与第 724 行现都写“`init_proc()`：`EmptySlot::activate` + `init_page_table()` + `init_regions()` + `set_boot`”；`vm_server.rs:1364/1372/1392/1394` 依次为 `activate`、`init_page_table`、`init_regions`、`set_boot`，描述与实现一致。 |
| T1B-05 | §5.4 缺 `before-read` 行 | RESOLVED | 第 1105 行已补“读启动参数前 \| `e5d: before-read-boot-params` \| `main.rs:59`”；`grep -n 'e5d:' os/servers/vm/src/main.rs` 有 59/64/78/96 四处，表中已全部列出。 |
| T1B-06 | §5.4 “四段诊断输出”计数 | RESOLVED | 第 1102 行现为“提供四段 `e5d:` 输出加一段自测结果（共五行）”；`grep -n 'e5d:' main.rs` 为 59/64/78/96 四处，`E5D-VM-SELF-PT` 在 104/114，合计五行，与表一致。 |
| T1B-07 | §2.2.7 末条 | RESOLVED | 第 391 行现为“CALLMAP 只需在主循环之前；C 把它排在 boot 进程循环之后，是因为 boot 进程的 `exec` 不查 `vm_calls`，而主循环一进来就会查”，与 §2.2.6 同口径。 |

## T2 复评锚点/计数修改的复核

| 位置 | 当前正文 | 源码核实 |
|---|---|---|
| §1.4 run 锚点 | `vm_server.rs:1692` | `grep -n 'pub fn run(&mut self)' os/servers/vm/src/vm_server.rs` → `1692` |
| §2.2.6 循环位置 | “是 C 的源码顺序，不是硬依赖……`CALLMAP` 只需在主循环之前” | C：480 `__minix_init`、485-495 记账、498-520 循环、522+ CALLMAP；`main.c:165` 只在主循环查表 |
| §2.2.5 `pre_init` | `pre_init.c:196-199` | `sed -n '195,200p'`：196-197 登记内核，198 `mods_with_kernel`，199 `kern_mod` |
| §2.2.7 注册计数 | “以上 26 项即 C 的全部注册；NR_VM_CALLS = 49，其余 23 个调用号未注册，主循环返回 ENOSYS” | 26 条 `CALLMAP`（main.c:537-575）；`main.c:139` `result = ENOSYS`；`main.c:165` 空槽不调 handler |
| §2.2.7 `CALLNUMBER` | `main.c:57-59` | `sed -n '57,59p'` 正是宏体三行 |
| §3.1.1 reconcile | 调用点 `boot.rs:387`，函数本体 `boot.rs:433-482` | `grep -n 'reconcile('` → 387；`fn reconcile` → 433；函数在 482 闭括号结束 |
| §3.5 blob 释放 | `C main.c:516-519` | 516 注释、517 `assert(start_addr % PAGE)`、518 `len = roundup`、519 `free_mem` |
| §4.0 最小示例 | `vm_server.rs:265`、`648`、`1692` | `grep -n` 实测 265 `install_server`、648 `init`、1692 `run` |

上表 8 个锚点/计数全部与当前源码一致；§2.2.7 末条的必要性口径已改为“只需在主循环之前”，与 §2.2.6 一致（T1B-07 RESOLVED）。UNIT 1–5 全部 PASS。

## UNIT 1

VERDICT: PASS

FINDINGS: 0

复核要点：

- TR-0 成立。§1.0 有问题入口、最小示例与失败对照；最小示例现在逐行可重放：`grep -n '^int main(void)$\|^void init_vm(void)$\|^static int is_first_time' minix3/minix/servers/vm/main.c` 实际输出 `79:static int is_first_time(void)`、`93:int main(void)`、`428:void init_vm(void)`，与正文一致；失败对照引用的 `main.c:100-107` 与 C 源一致。
- §1.1 的 A–E 五阶段与 §1.2 的启动主线图逐项对得上 C：442-444、447-452、455、458-462、465、468、471、474-475、480、485-495、498-520、522-575、577-586；§1.2 的 `577-586（含 SEF 特殊映射区）` 覆盖 578-579 与 582-583 两个操作，无范围冲突。
- §1.4 四条前提与 §1.5 小结闭合。前提 1 的 Rust 锚点已是 `vm_server.rs:1692`（实际 `pub fn run(&mut self) -> ! {`）；前提 2 的 `pagetable.c:48-57` 覆盖 spare pages 注释；前提 4 的两类失败锚点已按 T1B-01 拆开；`os/Cargo.toml:303` 确为 `panic = "abort"`。
- 本单元没有遗留的 P2/P3 问题；T1B-01 已修。

## UNIT 2

VERDICT: PASS

FINDINGS: 0

复核要点：

- 上一轮 6 条中属于本章的四条现在全部修好：T1-01（§2.0 锚点 `main.c:276`）、T1-02（§2.2.2 只先于 `mem_init` 的因果句）、T1-03（§2.0 最小示例带 `^...$` 锚点，实际输出 262、331、428 三行）、T1-04（§2.2.6 因果段，见 T1B-02 的修复证据）。
- §2.0 最小示例可重放；失败对照的 `main.c:458-462`、`main.c:276` 正确。
- §2.2.2：`get_mem_chunks()` 只把 `memmap[]` 按 click 对齐（`utility.c:44-79`），`mem_chunks` 在 `init_vm()` 只有 455、471 两个使用点；`memset(vmproc)`、`acl_init()`、`map_region_init()` 都不读它。`acl_init()`（`acl.c:21-30`）写 `NO_ACL`（-1）并清两张静态表，必须排在清零之后；`map_region_init()`（`region.c:36-38`）为空函数。三条均与 C 一致。
- §2.2.3/§2.3/§2.4 的函数范围已按 T1B-03 全部修正：`init_proc` 262-286、`sef_local_startup` 219-239、`sef_cb_init_vm_multi_lu` 592-672、`sef_cb_init_lu_restart` 677-726、`sef_cb_signal_handler` 731-750、§2.4 标题 592-750；旧范围已无残留。
- §2.2.5 的计费口径与当前 `boot.rs:263-292` 一致：module 循环排除内核伪条目，VM 条目清零后只计 1 页；Rust 对 handoff 模块列表全部计费并补 1 页，`vm_allocated_bytes` 只进 usage（`region.c:1366-1373`）。§3.6 的 `pages += 1` 与 §5.1 的 `test_boot_params_extra_pages_vm_module_row` 已同步。
- §2.2.6 的因果段已重写为“源码顺序 + 真实前置依赖 + 记账不是分配门 + CALLMAP 只需在主循环前”，与 C 的 `main.c:480/485-495/498-520/522+`、`alloc.c:319-331`、`pagetable.c:350/1005-1008/1311`、`main.c:165` 一致；T1-04 至此完全闭合。
- §2.2.7 的计数已改准：代码块末注释为“以上 26 项即 C 的全部注册；NR_VM_CALLS = 49，其余 23 个调用号未注册，主循环返回 ENOSYS”；C 侧 26 条 `CALLMAP`、`main.c:139` 的 `result = ENOSYS` 与 `main.c:165` 的空槽跳过均核实一致；`CALLNUMBER(c)` 的 `main.c:57-59` 也准确。末条的必要性口径也已改为与 §2.2.6 一致（T1B-07 RESOLVED）。
- 章内承诺与小结：§2.7 的五条与 §1.2、§2.2 一致；四个“为什么”现在都有显式落点（清表/ACL 顺序、`pt_init` 先于 `__minix_init`、boot 循环位置、RS_INIT 异步）。
- §2.2.7 的必要性口径已修：第 391 行现为“CALLMAP 只需在主循环之前；C 把它排在 boot 进程循环之后，是因为 boot 进程的 `exec` 不查 `vm_calls`，而主循环一进来就会查”，与 §2.2.6 同口径。T1B-07 RESOLVED。

## T1B-07 修复复核（RESOLVED）

- 位置：§2.2.7 末条（文档第 391 行）
- 原问题：§2.2.7 曾写“CALLMAP 必须在 boot 进程循环之后”，与 §2.2.6“只需在主循环之前”冲突。
- 复核：第 391 行现为“CALLMAP 只需在主循环之前；C 把它排在 boot 进程循环之后，是因为 boot 进程的 `exec` 不查 `vm_calls`，而主循环一进来就会查”。C 源 `main.c:165` 只在主循环查 `vm_calls[c].vmc_func`，`exec_bootproc()`（331-417）不查该表；两节口径一致。
- 状态：RESOLVED

## UNIT 3

VERDICT: PASS

FINDINGS: 0

复核要点：

- P-REFUTE 的两处修改保持正确且无新矛盾。§3.5 的两张表都写 `minix_stack_*` 已实现（`install_boot_stack`、4 MiB 栈区、`handle_memory_once`、真实 `vsp`/`ps_str`，唯一余项 `name_ptr = 0`）；代码 `vm_server.rs:1147` 在生产路径调用 `install_boot_stack`，`1175-1187` 用 `minix_sys::stack::stack_params`/`stack_fill`，`1211-1226` 建 4 MiB 区域，`1317` 以真实 `vsp`、`filled.ps_str` 调 `sys_exec`；`vm_server.rs` 715/846/898 的过期注释已改为“已实现、仅 `name_ptr = 0`”，文档与代码口径一致。
- §3.10 小结的“装载、栈帧、`sys_exec` 与 `VMCTL_BOOTINHIBIT_CLEAR` 已实现；`name_ptr` 仍以 0 传递”与 §3.5、§3.3、§5.2 一致。
- §3.8 的“49 个调用号空间、C 注册的 26 个调用有 handler、其余 `None` 并返回 ENOSYS”与 `ipc/dispatcher.rs:849-883` 的 `static CALLMAP: [Option<CallHandler>; NR_VM_CALLS_IDX]`/`build_callmap()` 的 26 个 `Some(...)` 项一致；`dispatch_by_number` 对 `None` 返回 `VmError::NotImplemented`（ENOSYS）。
- §3.4 A-14 的“先校验全部 rproctab 行、再一次性写 ACL”与 `rs_handshake` 的两段式实现一致（`vm_server.rs:2155-2185` 第一遍只收集、第二遍才 `set_acl`）；失败臂先按 `VmError::to_errno()` 回 `sef_init_reply`（`2014-2017`）。
- §3.1 的 `BootParams` 字段、`validate()` 的根页表/内核信息页/空闲区/`total_pages`/boot 进程号/VM 槽约束与 `boot.rs:61-292` 一致；§3.2 的四态表、`vm_handoff.rs:713` 的 `is_first_time: 1`、`boot_route` 的 `server_installed()` 两输入与代码一致；§3.6 的四本账表与 `boot.rs`/`global` 一致；§3.7 的 A-3 v2 三处标注与 `global.rs`/`plan.md`/`09-slab-allocator.md` 对齐。
- 非 FINDING 观察：§3.0 最后一句仍问“`exec_bootproc` 的……哪些仍受内核 IPC 能力约束”，而 §3.5 的答案是“全部实现、仅 `name_ptr = 0`”；按“可回答”标准仍闭合（答案是没有剩余子步骤受 IPC 约束），建议把问句改为“五步分别落在哪些函数、唯一余项是什么”，避免读者以为还有 IPC 缺口。§3.8 标题的“编译时 match”是简写，正文计数已说清，不计问题。
- 小结闭合：§3.10 的五条与 §3.1–§3.7 一一对应，P-REFUTE 改动未引入新矛盾。

## UNIT 4

VERDICT: PASS

FINDINGS: 0

复核要点：

- §4.0 最小示例可重放：`grep -n 'fn real_main\|match boot_route\|pub fn install_server\|pub fn init(&mut self)\|pub fn run(&mut self)' os/servers/vm/src/main.rs os/servers/vm/src/vm_server.rs` 实际输出恰为 `main.rs:41`、`main.rs:81`、`vm_server.rs:265`、`vm_server.rs:648`、`vm_server.rs:1692`，与正文一致；`1692` 确为 `pub fn run(&mut self) -> ! {`。
- §4.0 失败对照与 §4.4.1 的“先装 image slot、再 `register_page_alloc`、随后 `relocate()`”与 `main.rs:81-118`、`vm_server.rs:648-681` 一致。
- §4.2 的入口顺序 `read_boot_params` → `BootRoute` → `install_server` → `init` → `run_installed` 与 `main.rs:61-134`、`vm_server.rs:298` 一致；入口代码块是节选式改写，不改变语义。
- §4.4.2 的 `init_vm_slot()` 条目已补 `init_page_table()` 与 `init_regions()`（第 1000 行），§3.3 表第 724 行同步；与 `vm_server.rs:1364/1372/1392/1394` 的操作序列一致。T1B-04 修复。
- §4.4.2 的 Phase 1/2a/2b/2c/2d 与 `vm_server.rs:684-730` 一致；`PageFrames::new` 确在 `init_boot_procs()` 之前（702-717）；`mark_vm_instance()` 的 `VM_INSTANCE` + `inc_vm_instance` 与 `vmproc.rs`/`vmproc_handle.rs` 的配对一致；§4.4.4 的 Drop 清理与 `vm_server.rs:2416-2428` 一致。
- Phase 2d 的 `main.c:577-579` 与 SEF 登记 `582-583` 正确；25 篇 A-8 第 594 行已承接 SEF 特殊映射区，T1-05 的跨文档指针闭合。
- 小结与术语闭合：§4.5 四条与正文一致；`image slot`、`BootRoute`、Phase 2a–2d、`run_installed`、`rs_handshake` 均有定义。

## UNIT 5

VERDICT: PASS

FINDINGS: 0

复核要点：

- §5.3 的当前基线属实：本会话在 `os/` 下执行 `cargo test -p minix-vm --lib`，输出 `test result: ok. 549 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`，与文档一致。
- §5.1 的 14 个 `boot.rs` 测试全部存在且名称逐字匹配（`test_boot_params_extra_pages_vm_module_row` 已随 `boot.rs` 的重命名更新；`test_boot_params_extra_pages_modules/kernel` 与实际一致）。
- §5.2 的 9 个 `vm_server.rs` 测试全部存在；`test_install_boot_stack_writes_frame_and_exec_values`（`vm_server.rs:4102`）确实断言 `sys_exec` 收到 `stack = vsp`、`ps_str = placement.ps_str`，与 §3.5 的栈帧边界一致；“`exec_bootproc` 完整路径在 test 构建被 `#[cfg(not(test))]` 跳过、栈帧子步骤由该测试单独覆盖”与 `vm_server.rs:871-872/915` 一致。
- §5.4 的旧问题（漏 `before-read` 行）已修：表中现在有 `e5d: before-read-boot-params`（`main.rs:59`）一行，四个 `e5d:` 输出全部列出。
- §5.4 的计数不一致已修：第 1102 行现为“四段 `e5d:` 输出加一段自测结果（共五行）”，与表中 4 个 `e5d:` 行加 1 个自测行一致。T1B-06 RESOLVED。
- 本章小结与术语（宿主测试、负控、真机门、基线读数）与正文一致。

## T1B-06 修复复核（RESOLVED）

- 位置：§5.4 真机门清单首句（文档第 1102 行）
- 原问题：正文“四段诊断输出（前三段带 `e5d:` 前缀，第四段是自测结果）”与表中 5 行（4 个 `e5d:` 输出 + 1 个自测）不一致。
- 复核：第 1102 行现为“提供四段 `e5d:` 输出加一段自测结果（共五行）”；`grep -n 'e5d:' os/servers/vm/src/main.rs` 输出 59/64/78/96，`grep -n 'E5D-VM-SELF-PT'` 输出 104/114，合计五行，与表逐行对应。
- 状态：RESOLVED

## 上一轮 6 条与作者中途修复的最终状态

- T1-01：已修（§2.0 锚点 `main.c:276`）。
- T1-02：已修（§2.2.2 只先于 `mem_init`）。
- T1-03：已修（§2.0 命令带 `^...$` 锚点，输出 262、331、428 逐行一致）。
- T1-04：已修（§2.2.6 因果段重写，与 §2.2.5 的“只是记账”一致，不再自相矛盾）。
- T1-05：已修（25 篇 §4.8 A-8 第 594 行 + §说明承接）。
- T1-06：已修（§4.0 五行输出与 `vm_server.rs:1692` 一致；§1.4 的 `1693` 也已改 `1692`）。
- 中途四项：`428:void init_vm(void)` 与两条带锚点命令可重放；`1693`→`1692`；`vm_server.rs` 715/846/898 注释已改；§3.8 的 26 handler/ENOSYS 计数属实。以上均已复核通过。
- T2 复评的 7 处锚点/计数修改见下节，均已核实通过。

HARD-REJECTIONS: 0
