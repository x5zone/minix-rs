# DESIGN-D2 R1
DOC 01-vm-init-main.md

审查基准：`rewrite-notes/02-stage-vm/01-vm-init-main.md` 当前内容（审查期间该文件于 2026-11-22 21:04 被并发修改，md5 `fb22b1fa0fec7b4af03e84ba50671e35`）。修改后已补入 §1.2 阶段对照表、§3.3 SEF 特殊映射行、§3.5 边界表、§3.6 四名字表、§4.4.4 收窄为 Drop；这些改动解决了本轮先前发现的若干项，本报告只保留在当前内容上仍成立的问题。对照源：`minix3/minix/servers/vm/main.c`、`pagetable.c`、`alloc.c`、`utility.c`、`minix3/minix/include/minix/param.h`、`minix3/minix/lib/libsys/sef*.c`；Rust `os/servers/vm/src/{main,boot,global,vm_server,heap_arena}.rs`、`os/servers/vm/src/ipc/transport.rs`、`os/kernel/src/{vm_handoff,lib,proc_table,syscall_signal}.rs`。

## FINDING DESIGN-D2-01（P0）

- 位置：§3.3 表格 boot 行（706 行）、§3.3 “当前实现边界”（711 行）、§3.5 表格栈建立行（735 行）与边界表（746 行）、§3.9（807 行）。
- 严重度：P0（文档事实与当前代码相反，且是本篇“已实现/未实现边界”的主结论）。
- 问题：文档反复声明 `minix_stack_*` 初始栈帧 ABI “尚未落地”“`stack`/`ps_str` 以 0 上报”。当前代码 `install_boot_stack` 已完整实现：`minix_sys::stack::stack_params` 定尺寸、`stack_fill` 布局 argc/argv/envp/ps_strings、建立 4 MiB 栈区、`handle_memory_once` 做映射校验、经 Direct Map 写入帧字节，并以真实 `vsp` 与 `filled.ps_str` 调用 `sys_exec`，随后发 `VMCTL_BOOTINHIBIT_CLEAR`。仍以 0 传递的只有 `sys_exec` 的 `name_ptr`（progname），文档完全没有提这一项。
- 影响：读者会把 boot 链判成“装载完成但用户栈不可用”，从而低估启动链完成度；§3.5 自己声明“本节唯一口径”，§3.9 又重复旧边界，错误口径会扩散到 09/25 与真机门判断。
- 证据：`os/servers/vm/src/vm_server.rs:1141`（调用 install_boot_stack）、`:1155-1181`（stack_params/stack_fill）、`:1205-1272`（4 MiB 栈区与 handle_memory_once）、`:1294-1300`（帧写入）、`:1311`（`sys_exec(endpoint, entry, vsp, 0, filled.ps_str)`）、`:1340-1342`（BOOTINHIBIT_CLEAR）；`os/libs/minix-sys/src/stack.rs:106,147`；C 对照 `minix3/minix/servers/vm/main.c:346-416`。
- 最小修复：把 §3.3/§3.5/§3.9 的相关行统一改成“栈帧、`sys_exec`、`BOOTINHIBIT_CLEAR` 已实现；唯一余项是 `name_ptr = 0` 与内核侧进程名语义”；删除“以 0 上报”的表述，或把它限定为 progname。
- 置信度：高。
- 是否需源码核实：否（函数调用与实参已在生产路径可见；`stack.rs` 无 feature 门）。
- 若我错了：只有当 `install_boot_stack` 在生产构建中被 `cfg` 排除，或 `minix_sys::stack` 的函数体是占位实现时才成立；当前 `exec_bootproc` 本身是 `#[cfg(not(test))]` 的生产路径，且 `stack.rs` 两个函数有实际实现。

## FINDING DESIGN-D2-02（P0）

- 位置：§3.2（668 行“该查询发生在内核侧”、680-687 行四态真值表）、§3.9（804 行）、§4.2（884 行）。
- 严重度：P0（本篇中心设计决策建立在不存在的内核行为上）。
- 问题：文档称 `is_first_time()` 的查询“发生在内核侧（RTS 标志属内核状态）”，VM 只接收结果。实际内核构造交接页时把 `is_first_time` 无条件写成 `1`，全树没有任何代码读取 RS 的 `RTS_BOOTINHIBIT` 来填这个字段。由此四态路由在真实入口上退化：`real_main` 只执行一次且 `run_installed()` 不返回，`WarmRun`（false+有槽）与 `DoubleInit`（true+有槽）不可达；`WarmNoState`（false+空槽）也因 `is_first_time` 恒为 true 不可达。生产上只剩 `ColdInit`。另外 `read_boot_params()` 在路由之前执行且要求内核交接页已映射，C 的 false 路径（Live Update/restart，由 SEF 处理）在新 VM 实例上根本没有这张页，路由层无法表达该形态。
- 影响：文档把“Rust 复刻 C 的 is_first_time 门控”当作已解决的设计问题；实际上 C 支持的暖启动/LU 分支在 Rust 侧既不可达也未实现。将来接 LU 时，新实例要么重跑 `init_vm()`（若内核复用旧交接页并继续报 1），要么在 `read_boot_params()` 处读不到有效交接页，两条都会在启动早期出错。`WarmNoState`/`DoubleInit` 没有 C 依据，文档没有说明。
- 证据：`os/kernel/src/vm_handoff.rs:713`（`is_first_time: 1`，唯一写入点）；`os/kernel/src/lib.rs:3129`（唯一 `build_vm_handoff` 调用）；`os/servers/vm/src/main.rs:61,81`；`os/servers/vm/src/vm_server.rs:245-253`；C `main.c:79-88,100-107`；`sef.c:115-146`（C 的 false 路径是 SEF init request）。
- 最小修复：在 §3.2 明确写“内核当前恒报 fresh，四态中只有 `ColdInit` 可达”，并给四态表加“生产中是否可达 / 有无 C 依据”两列；把 `WarmRun`/`WarmNoState`/`DoubleInit` 标为诊断占位，说明 LU/restart 入口需要在 25 篇与内核交接页版本一起落地。
- 置信度：高。
- 是否需源码核实：是（是否存在分支/架构专用构造器写 0；当前 grep 只有 `vm_handoff.rs:713`）。
- 若我错了：如果内核在 VM 重启路径上重新构造交接页并按 RTS 标志写 0，或 `VmBootHandoff` 另有写入者；当前树内未见该路径。

## FINDING DESIGN-D2-03（P0）

- 位置：§2.2.5（311 行“Rust 忠实复刻”）、§3.6 代码片段（767-769 行）、§5.1（1013 行测试说明）。
- 严重度：P0（文档声称忠实复刻，实际数据形状不同，导致总页数记账偏差）。
- 问题：C 的 `module_list[]` 在 `pre_init.c` 中把内核自身追加为最后一个条目（`mods_with_kernel = mi_mods_count + 1`），所以 `mods_with_kernel - 1` 恰好排除内核伪条目。Rust 交接页的 `modules` 是“除 VM 自身模块之外的全部 boot 模块”，**没有内核条目**；`extra_pages()` 的 `saturating_sub(1)` 因此丢掉一个真实模块。此外 C 会把 VM 自己的模块也计入模块循环，Rust 的交接列表直接跳过 VM 模块，VM 映像页也没有被计入。两处叠加使 `global::total_pages()` 相对 C 少计。
- 影响：`do_info`/`vsi_total`（`26-vm-queries.md`）与任何读取全局总量的统计都低于 C 的口径；模块清单越短（真实 boot 映像只有少数模块）偏差越显著。现有测试 `test_boot_params_extra_pages_modules` 用虚构的三模块列表验证“排除最后一项”，恰好把错误形状固定下来，生产形状没有测试。
- 证据：`os/kernel/src/vm_handoff.rs:592-613`（`if i == vm_module_idx { continue; }`，循环结束没有追加内核条目）；`os/servers/vm/src/boot.rs:266-272`（`saturating_sub(1)`）；`minix3/minix/kernel/arch/i386/pre_init.c:192-200`；C `main.c:485-495`；`os/servers/vm/src/boot.rs:528-550`（测试）。
- 最小修复：二选一并写进文档。其一，交接页保持“VM 除外、无内核”的列表，`extra_pages()` 改为计入全部列表项，VM 映像页按 `vm_allocated_bytes` 或显式项补记；其二，把内核伪条目也放进交接列表，维持 `-1`。同时用交接页真实形状重写测试，并在 §2.2.5 说明 C 与 Rust 的列表差异。
- 置信度：高（形状与断言）；中（精确少计页数）。
- 是否需源码核实：是（`VmBootAllocator` 的 VM 映像帧与 `boot_alloc_used_bytes()` 是否重叠，决定 VM 模块该不该另计）。
- 若我错了：如果 `kernel_info.boot_modules` 末尾确实含内核条目，或 `vm_handoff` 在别处追加了内核；当前解码与 `build_vm_handoff` 都没有这个动作。

## FINDING DESIGN-D2-04（P1）

- 位置：§3.1（664 行只讲 `validate()`）、§4.1（849 行只讲“读出 `BootParams`”）、§5.1（1008-1014 行测试表）。全文没有出现 `reconcile`/`deducted`/`LiveBootstrap`。
- 严重度：P1（本篇生产入口的核心启动契约缺席）。
- 问题：`read_boot_params()` 在 `validate()` 之后调用 `reconcile()`，校验 A2 消费边界：扣减记录非空、根页表在记录内、每个保留 boot 模块 blob 在记录内、空闲区与扣减区不相交。这是内核把 `memmap` 分类后交给 VM 的“边界证明”，也是 VM 侧唯一能独立复核内核记账的地方。文档只把 `read_boot_params` 描述成“从交接页读出”，读者看不到这条不变量及其失败行为（panic）。
- 影响：启动链最关键的新契约不可见；§5 测试表也因此漏掉 5 个 reconcile 测试，读者无法判断“空闲内存清单可信”这一点由谁保证。后续文档（05/06）若按本篇口径理解 free_regions，会把它当成纯数据而不是已验证的结论。
- 证据：`os/servers/vm/src/boot.rs:374-375`（调用点）、`:421-470`（reconcile 四条断言）、`:582-673`（对应测试）；内核 `os/kernel/src/vm_handoff.rs:455-560`（classify/deducted 生成）。
- 最小修复：在 §3 增加一小节“A2 消费边界：adopt 与 reconcile”，说明内核在交接页里给出 free_regions 与 deducted、VM 只 adopt 不裁剪、reconcile 在 PMM 构造前失败即 panic；§5.1 补 reconcile 测试行。
- 置信度：高。
- 是否需源码核实：否。

## FINDING DESIGN-D2-05（P1）

- 位置：§3.3 表格 `get_mem_chunks` 行（698 行）、§3.1（648 行 `free_regions` 注释、662 行 ARCH 注记）。
- 严重度：P1（职责拆分表述错误，掩盖了内核/VM 的边界迁移）。
- 问题：表格把 `get_mem_chunks` 与 `mem_init` 一起映射为 `create_default_allocator + VmPageAllocator::new`，暗示 VM 仍负责把 `memmap` 转成 click 对齐的块。实际 Rust 没有 `get_mem_chunks`：内核在 `classify` 阶段完成内存分类、裁剪与页对齐，交接页直接给出 `free_regions` 与 `deducted`；`create_default_allocator` 只做元数据选址与扣除。同理 `mmap_size`、`mods_with_kernel`、`kern_mod`、`vir_kern_start` 等 C 字段的消失与替代没有列在职责表里，`vm_allocated_bytes` 又被收进启动参数却只用于统计。
- 影响：读者会把内存分类责任记在 VM 头上，与 05/06 的实际分工冲突；也没法回答“C 的 `kern_mod` 去哪了”（被 `KernelLayout` 取代）和“`mods_with_kernel` 去哪了”（被交接列表长度取代，且形状不同，见 FINDING 03）。
- 证据：`os/servers/vm/src/boot.rs:327-331,344-373`；`os/kernel/src/vm_handoff.rs:455-560,592-613`；C `utility.c:44-79`、`param.h:14-47`、`pagetable.c:1108-1110,1290`。
- 最小修复：在 §3.1 增加一张“C `kernel_boot_info` 字段 → 内核交接页字段 / Rust 载体 → 消费者”的表，逐项标出保留、上移内核、结构替代、删除；把 §3.3 的 `get_mem_chunks` 行改成“内核 classify 已产出 free_regions，VM 只 adopt”。
- 置信度：高。
- 是否需源码核实：否。

## FINDING DESIGN-D2-06（P1）

- 位置：§4.0 失败对照（840 行）与 §4.4.1（906 行）、§4.4.2 代码（913 行）、§4.4.2 说明（950 行）。
- 严重度：P1（同一文档内部对同一注册时机给出相反结论）。
- 问题：§4.0 写“`register_page_alloc` 在构造期执行（§4.4.1）”，并据此把危险窗口描述成“构造后移动”。§4.4.1 紧接着写“`global::register_page_alloc` 不在构造期：它在 `init()` 开头……”，§4.4.2 的代码也在 `init()` 第一行注册。三种说法互斥。
- 影响：§4.0 的失败对照正是地址稳定性理由的入口；按它理解会得出“构造后不能移动”的结论，而真正契约是“装入 image slot 后才注册，注册前移动无害”。读者若按 §4.0 改代码，会把注册留在构造期，重新引入 `PAGE_ALLOC_PTR` 指向已移动对象的旧缺陷（`new_inner` 的注释记录了该缺陷的真机症状）。
- 证据：文档 840 行 vs 906 行 vs 913 行；`os/servers/vm/src/vm_server.rs:346-370`（构造期注册的旧缺陷说明）、`:659`（init 开头注册）、`:2398-2417`（Drop 注销）。
- 最小修复：重写 §4.0 失败对照为“构造 → 装入 image slot → `init()` 开头注册 → `relocate()` 原地替换字段”；删除“构造期执行”的旧句，§4.4.1/§4.4.2 保持现状。
- 置信度：高。
- 是否需源码核实：否。

## FINDING DESIGN-D2-07（P1）

- 位置：§3.4 表格第一行（719 行）与结论（725 行）。
- 严重度：P1（“等价”结论覆盖面远大于实际实现，且丢失项未列）。
- 问题：`rs_handshake()` 只做两件事：通过 grant 取 rproctab、逐条写 ACL。C 的 `process_init` 还按 `init_type` 分派 fresh/LU/restart、清理 IPC 过滤器、建立状态传输 grant、清理 init buffer（`vm_munmap`）、`cpf_reload`/`senda_reload`，并依据 `flags` 处理调试型 init。Rust 侧 `RsInit` 的 `init_type`、`flags`、`old_endpoint`、`restarts`、`buff_addr`、`buff_len`、`prepare_state` 全部被解码后丢弃，优先级 2 分支对任何 RS_INIT 都执行 fresh 握手并回 OK。文档却把这一收缩说成“承担 SEF 的协议语义”。另外“C 没有标准服务初始化协议”是错的：RS_INIT 加 rproctab 正是 C 的标准协议，SEF 是它的框架实现。
- 影响：LU/restart 的 RS_INIT 会被误当作 fresh 握手并成功应答，状态恢复机制没有入口；读者也无法从文档判断哪些 SEF 语义被有意删除、哪些尚未实现。
- 证据：`minix3/minix/lib/libsys/sef_init.c:43-140`（process_init 全部动作）、`:193-215`（按 type 组装 info）；`os/servers/vm/src/vm_server.rs:2142-2166`（rs_handshake）、`:1962-2035`（优先级 2 不检查 type）；`os/libs/minix-types/src/ipc/rs.rs:97-116`（被丢弃的字段）；`os/libs/minix-sef/src/lib.rs:280-310`（库侧能提供 init_type，VM 未消费）。
- 最小修复：把“保留/丢失”按 SEF 组件逐条列出；明确写“当前只实现 fresh，`init_type` 未校验”；把 LU/restart 行改成“未实现，不能称等价”；修正“C 没有标准协议”的表述。
- 置信度：高。
- 是否需源码核实：否。

## FINDING DESIGN-D2-08（P1）

- 位置：§3.2（689 行“单臂 `send`”）与 §3.4 第三行（721 行“`IpcTransport::send` 单臂发出；asynsend 语义等价”）。
- 严重度：P1（把会死锁的实现方式写成等价，且与代码实际调用不符）。
- 问题：C 在这里用 `asynsend3(RS_PROC_NR, m_ptr, AMF_NOREPLY)`，原因正是普通同步发送在 RS 仍处 `PAGEFAULT|SENDING` 时会形成二循环 EDEADLOCK。代码中的同段注释也明确说“a blocking send would be judged a 2-cycle EDEADLOCK”，实际实现调用的是 `transport.asynsend(...)`。文档却说走 `send`，并声称 asynsend 只是“语义等价——单条无期待回复的消息”，没有说明为什么不能用 send。
- 影响：这是启动死锁的唯一防线；文档给出的规则如果被照做（改回 send），会在真机启动时卡死。等价性论证也掩盖了 SENDA 队列生命周期（槽位必须比调用活得久）这一实现约束。
- 证据：C `main.c:225-229`、`sef_init.c:471-482`；`os/servers/vm/src/ipc/transport.rs:129-141`（asynsend 契约与死锁说明）；`os/servers/vm/src/vm_server.rs:1976-1995`（实际调用 asynsend）。
- 最小修复：两处都改成 `asynsend`（AMF_NOREPLY），补一句“send 在此会 EDEADLOCK，不可替换”；保留“第一次之后 C 改回同步、Rust 无此状态”的差异说明。
- 置信度：高。
- 是否需源码核实：否。

## FINDING DESIGN-D2-09（P1）

- 位置：§4.4.3（960 行）。
- 严重度：P1（Rust 侧行为描述与代码相反）。
- 问题：文档写“`missing_spares` 检查保持既有实现”。当前 `run()` 的注释明确说 C 的 `if(missing_spares > 0) alloc_cycle()` 补页钩子“is deleted with its chain”，`handle_signal` 也说明 `alloc_cycle` 尾巴被删除、`pt_clearmapcache` 无对应物。
- 影响：读者会以为主循环仍有物理页压力补充路径；这与真实设计（Direct Map 消除保留页池、回收改在 `alloc_pfn_reclaiming`）相反，也会误导 06/24 的边界判断。
- 证据：`os/servers/vm/src/vm_server.rs:1427-1468`（alloc_cycle 与 handle_signal 删除说明）、`:1691`（run 注释）；C `main.c:118-119,745-747`。
- 最小修复：改为“`missing_spares`/`alloc_cycle` 链已删除（[ARCH: A-1]：保留页池被 Direct Map 取代；回收在 `alloc_pfn_reclaiming`）；`pt_clearmapcache` 无对应物”。
- 置信度：高。
- 是否需源码核实：否。

## FINDING DESIGN-D2-10（P1）

- 位置：§3.4 信号行（723 行）。
- 严重度：P1（设计理由引用的内核限制已经不存在）。
- 问题：文档称“本树内核的 `SigSet` 仍为 64 位、位 70..73 发不出来”，引用 `os/kernel/src/syscall_signal.rs:88-94`。当前该文件说明 `SigSet` 已扩到 128 位（与 C 的 `sigset_t` 一致），位 70 与 74 都可表示。`effective_kernel_signal` 的兜底今天仍可能触发，但真实原因是 `vm_enqueue_and_notify_vm` 只发裸通知、没有把 SIGKMEM 置入 `p_pending`，不是位宽不足。
- 影响：根因错误会导向错误的后续动作（以为“等内核加宽位图”即可删除兜底）；同时 `run_once` 与 `transport` 里的同类注释也建立在旧前提上，文档引用它们会形成自我确认。
- 证据：`os/kernel/src/syscall_signal.rs:70-104`（128 位与 SIGSNDELAY=70、SIGKSIG=74）；`os/kernel/src/proc_table.rs:342-381`（只做 mini_notify）；`os/servers/vm/src/vm_server.rs:1761-1771,1795-1804`（旧说法）；文档 723 行。
- 最小修复：把理由改成“内核通知只带唤醒、不置 pending 位，minix-sef 递 `SEF_SIGNAL_REQUEST_TYPE`”；若内核未来改为置位，再按真号分派。顺带更新 `vm_server.rs` 的同段注释。
- 置信度：高（引用过时）；中（实际投递形状是否另有置位路径）。
- 是否需源码核实：是（`mini_notify` 之外是否还有给 VM 置 SIGKMEM 位的路径）。
- 若我错了：如果存在另一条 `p_pending.add(71)` 的 VM 通知路径，且它仍受某个 64 位字段截断；当前 `syscall_signal.rs` 的 128 位声明与之矛盾。

## FINDING DESIGN-D2-11（P1）

- 位置：§3.3 表格 `init_proc(VM_PROC_NR)` 行（702 行）、§4.4.2 `init_vm_slot` 说明（952 行）。
- 严重度：P1（把静默跳过说成等价 `init_proc`，违背本篇的 fail-fast 前提）。
- 问题：`init_vm_slot()` 用 `if let Some(ip) = ...` 查找 VM 条目，找不到时直接返回；C 的 `init_proc` 在查不到时 `panic("no init_proc")`。`BootParams::validate()` 只断言 VM 条目“不超过 1 个”，没有断言“至少 1 个”，所以缺 VM 条目的交接页能通过校验并进入 `init()`，随后 `mark_vm_instance()` 也是空操作。
- 影响：交接契约缺项被静默吞掉，与 §1.4 第 4 条“启动期 fail-fast、没有降级路径”直接冲突；真要出错会在更晚的 `vm_isokendpt`/ACL 阶段以更难诊断的形式暴露。
- 证据：C `main.c:262-286`；`os/servers/vm/src/vm_server.rs:824-833`；`os/servers/vm/src/boot.rs:243-252`。
- 最小修复：`init_vm_slot()` 在找不到条目时 panic（或 `validate()` 断言恰好 1 个），并在 §3.3/§4.4.2 说明这是 C `panic("no init_proc")` 的对应。
- 置信度：高。
- 是否需源码核实：否。

## FINDING DESIGN-D2-12（P1）

- 位置：§3.7 第一段（787 行）。
- 严重度：P1（把 VM 自身堆的地址来源说反，与 09 篇和代码注释冲突）。
- 问题：文档写“通过 `HEAP_ARENA` 从直接映射区申请页”。实际分层是：`HeapArena` 保留一段连续虚拟地址（`VM_HEAP_BASE..LIMIT`），物理页由 `VmPageAllocator` 分配，再用 `vm_self_mappages()` 一页页映射进这段连续 VA；`global.rs` 的注释明确写“The arena VA comes from HeapArena (contiguous), NOT from Direct Map (has holes). Direct Map is used only for physical page management”。
- 影响：Direct Map 与 HeapArena 的职责被混为一谈；读者会误以为堆页的 VA 是 `PA + DM_BASE`，从而无法理解为什么还需要一个 arena、为什么 arena 页不收缩，也会与 09 篇的“连续 VA + 独立物理帧分配器”模型对不上。
- 证据：`os/servers/vm/src/heap_arena.rs:1-35`；`os/servers/vm/src/global.rs:790-796`；文档 787 行。
- 最小修复：改为“VA 来自 HeapArena 的连续窗口，帧来自 VmPageAllocator，经 `vm_self_mappages` 映射；Direct Map 只用于元数据与页表页访问”。
- 置信度：高。
- 是否需源码核实：否。

## FINDING DESIGN-D2-13（P1）

- 位置：§3.2 四态表（680-687 行）、§3.4 失败臂（720 行）、§4.4.2 `relocate()`（915 行）。
- 严重度：P1（架构演进没有在文档侧标注，三处一致无法审计）。
- 问题：文档把三处已裁决的架构演进当作普通实现细节叙述，正文没有任何 `[ARCH: ...]` 标注：镜像槽与四态路由（代码标 `[ARCH: V20-P1-1]`）、RS_INIT 失败臂存活（代码标 `[ARCH: A-14]`）、启动期分配器 `relocate()`（plan.md 的 A-5）。本篇是这三项唯一的散文载体；不标就无法满足“文档+设计+代码三处一致”的硬约束。
- 影响：读者无法区分“批准的演进”与“无意的差异”；审计时只能去代码里找标签，文档侧对不上。A-14 尤其重要，因为它改变了 C 的外部行为（C panic，Rust 存活）。
- 证据：`os/servers/vm/src/vm_server.rs:214,225,242`（V20-P1-1）、`:2021-2025`（A-14）、`:536-647`（relocate，A-5）；`rewrite-notes/02-stage-vm/plan.md:214`（A-3/A-5 表）；文档 680-687、720、915 行。
- 最小修复：在三处正文加 `[ARCH: V20-P1-1]`、`[ARCH: A-14]`、`[ARCH: A-5]`，并在 §3 末加一张“本阶段 ARCH 决定”表（编号、C 行为、Rust 行为、裁决依据、受影响测试）。
- 置信度：高。
- 是否需源码核实：否。

## FINDING DESIGN-D2-14（P1）

- 位置：§3.3 表格 `pt_init()` 行（703 行）、§4.4.1 第 3 步（903 行）、§4.4.2 时机说明（945 行）。
- 严重度：P1（把结构替代写成“等价”，C 侧依赖没有交代）。
- 问题：表格把 `pt_init()` 的 Rust 等价写成 `init_vm_self_pt()`。C 的 `pt_init` 用 `vmprocess`（依赖 `init_proc(VM_PROC_NR)` 已建立槽）、`kernel_boot_info.kern_mod`/`module_list` 找内核并映射、并建立保留页池；Rust 的 `init_vm_self_pt` 只是采用内核已建好的引导页表根（A1），不建页表、不映射内核、没有保留页池，也不依赖 VM 槽。这是结构替代，不是函数移植；文档只在 `root_paddr` 字段注释里提了 A1。
- 影响：读者会以为 `pt_init` 的语义被完整保留；也无法从本篇理解为什么 Rust 不需要保留页池、为什么 `vmprocess` 依赖消失、为什么顺序可以与 C 相反。与 §1.4 第 2 条只讲“Direct Map 承担职责”相比，这里缺少同一结论的落地说明。
- 证据：C `pagetable.c:1088-1110`（kern_mod/mods_with_kernel）、`:1290`（`&vmprocess->vm_pt`）；`os/servers/vm/src/pagetable/vm_self_map.rs:175-182`（adopt）；C `main.c:474-475`；`os/servers/vm/src/boot.rs:62-71`（root_paddr 注释）。
- 最小修复：把该行改成“结构替代 [ARCH: A1]：C 建自己的页表并预置保留页池；Rust 采用内核引导根，保留页池由 Direct Map 消除”；补一句 C 的 `pt_init` 依赖 VM 槽、Rust 的 adopt 不依赖，因此构造期执行是安全的。
- 置信度：高。
- 是否需源码核实：否。

## FINDING DESIGN-D2-15（P2）

- 位置：§3.6 正文（774 行）与其后的四名字表（776-781 行）。
- 严重度：P2（文档内部口径不一致）。
- 问题：正文说“分配器的空闲页池是‘可分配页数’。C 两者分离，Rust 保持同一模型”；紧随其后的表格却把 `VmPageAllocator::total_pages()` 定义为“后端总容量（位图/伙伴分配器自己的计数）”，并说明 `PageFrames::new` 用它。两者不是同一口径：`total_pages()` 返回 `phys_alloc.total_count()`（最高页号容量），真正的空闲页数是 `memstats().free_pages`。另外 C 只有一本 `total_pages` 账加一张覆盖整个 32 位物理空间的固定位图，说“C 两者分离”也不准确。
- 影响：这个四名字表本来是全篇最需要的消歧工具，正文的旧句会让读者退回错误理解。
- 证据：`os/servers/vm/src/alloc_page.rs:127-129`；`os/servers/vm/src/phys_mem/bitmap_alloc.rs:407-409,448-451`；`os/servers/vm/src/global.rs:57-66`；C `alloc.c:33-35,306-331`；文档 774 与 780 行。
- 最小修复：正文改成“分配器侧是位图容量（不是空闲页数，空闲页数在 `memstats().free_pages`）；C 的对应物是一张固定满地址空间位图加一本 `total_pages` 账”，与表格保持一致。
- 置信度：高。
- 是否需源码核实：否。

## FINDING DESIGN-D2-16（P2）

- 位置：§3.5 表格 `pt_new + pt_bind` 行（733 行）、§3.5 末句（752 行）。
- 严重度：P2（实现位置与“保留的保护”清单不准确）。
- 问题：`pt_bind` 的实质是 `sys_vmctl_set_addrspace`，它不在 `init_proc`/`init_page_table` 里，而在 `exec_bootproc` 中（`vm_server.rs:962-970`）；`init_proc` 只建页表。另 `init_boot_procs` 除了“跳过负 proc_nr、assert(start_addr)”之外还跳过 `endpoint.is_none()`（定长数组填充项），并且 C 的 `assert(ip->start_addr)` 对 VM 条目也执行，Rust 在跳过 VM 之后才断言。
- 影响：读者会把绑定时机记错；三处“保护”的说法也会让人以为 Rust 只做了两项检查。
- 证据：`os/servers/vm/src/vm_server.rs:962-970`（set_addrspace）、`:843-861`（三处跳过与断言顺序）；C `main.c:352-356,502-510`。
- 最小修复：表格拆成两行（建表在 `init_proc`，绑定在 `exec_bootproc`）；末句补第三项跳过与断言顺序差异。
- 置信度：高。
- 是否需源码核实：否。

## FINDING DESIGN-D2-17（P2）

- 位置：§3.7 “标注三处一致”第二条（791 行）。
- 严重度：P2（ARCH 三处一致的“设计”落点不是可寻址文档）。
- 问题：“design：`global.rs` 的 `VmAllocator` 设计文档”不是文件路径，树内也没有这样一个设计文档；实际的设计源是 `rewrite-notes/02-stage-vm/plan.md:214`（A-3 行）与 `09-slab-allocator.md:130-132`。本条自己第一条又引用了 09 篇 §3.1，三处清单与正文引用重复且不一致。
- 影响：三处一致检查无法按锚点复核；读者不知道哪一份是设计权威。
- 证据：文档 789-792 行；`rewrite-notes/02-stage-vm/plan.md:214`；`rewrite-notes/02-stage-vm/09-slab-allocator.md:130-132`；`os/servers/vm/src/global.rs:773`。
- 最小修复：第二条改为 `rewrite-notes/02-stage-vm/plan.md`（A-3）与 `09-slab-allocator.md` §3.1 的绝对路径，删掉“global.rs 的设计文档”这一说法。
- 置信度：高。
- 是否需源码核实：否。

## FINDING DESIGN-D2-18（P2）

- 位置：§3.7 末段（794 行）。
- 严重度：P2（“可观测行为上等价”的结论超出论证）。
- 问题：文档承认“arena 页不收缩”，却仍断言与 C 的 `munmap` 显式归还在可观测行为上等价。C 的 `vm_freepages` 会把页还给 VM 自身分配器，`get_vm_self_pages()` 会下降；Rust 的 free-list 只复用块，arena 页常驻，`self_page_count()` 不会随之下降。两者对“可分配内存复用”等价，对“VM 自身驻留页统计”不等价。
- 影响：`get_usage_info_vm` 一类的自身占用统计会与 C 有系统性差异；文档把它说成等价，后续查询篇可能据此得出错误断言。
- 证据：C `utility.c:376-380,385-420`；`os/servers/vm/src/global.rs:772-800`（arena 不收缩）；文档 794 行。
- 最小修复：把“可观测行为上等价”限定为“可分配内存的复用等价”；明确驻留页统计不等价，并把该差异指向统计篇。
- 置信度：高。
- 是否需源码核实：否。

## FINDING DESIGN-D2-19（P2）

- 位置：§3.4 表格第一行“C 中的必要性”列（719 行）。
- 严重度：P2（设计理由的前提错误）。
- 问题：“C 没有标准服务初始化协议，必须通过 IPC 从 RS 取 rproctab”不成立：RS_INIT 消息加 rproctab grant 就是 C 的标准协议，SEF 是它的统一框架；Rust 保留的正是这个协议，去掉的只是回调状态机。真正需要说明的是“协议保留、框架删除”，而不是“C 没有协议”。
- 影响：这条错误前提会让读者以为 Rust 发明了协议，也会掩盖 SEF 框架中与协议无关但被删除的部分（状态传输、init buffer 清理等，见 FINDING 07）。
- 证据：C `sef.c:68-146`、`sef_init.c:193-215`；`os/servers/vm/src/vm_server.rs:2142-2166`。
- 最小修复：改为“C 用 SEF 框架承载 RS_INIT 协议；Rust 保留协议、删除回调状态机”。
- 置信度：高。
- 是否需源码核实：否。

## SUGGESTION

1. 给 §3 加一张“C→Rust 启动步骤差异表”，每行至少四列：C 位置、Rust 落点、差异类型（等价 / 完成时机不同 / 结构替代 / 推迟 / 删除）、复核门（测试名或真机路标）。当前 §3.3 表把“等价”和“结构替代”混在一列，是 FINDING 05/14 的根源。
2. `BootRoute` 的四态表加“生产中可达性”与“C 依据”两列；如果短期内不接 LU，就把枚举写成“冷启动路由 + 三个诊断拒绝”，不要用“四种启动形态”这种覆盖性措辞。
3. A2 消费边界（adopt → reconcile → PMM 使能）值得一个小节和一张失败对照；它比 `validate()` 更能代表本篇新增的启动契约。
4. §5.4 说真机门缺口“需要补断言，或明确登记为尚未覆盖的风险”。“或登记”是一个没有落点的待办，按零欠账标准应改为已登记的判决行（`DECISIONS.md` 的 `defer-approved`）或直接删掉这句话，只保留“宿主测试不证明真机终点”的事实陈述。
5. `PageFrames` 在 §4.4.2 只用“exec_bootproc 需要它”解释顺序，可以补一句它与 C 的对应关系：C 的 page cache/帧状态是全局静态结构，Rust 的 `PageFrames` 是 Rust 侧新表，`total_phys` 取分配器总容量，因此必须晚于分配器构造、早于段页分配。这样读者不会误以为它是 C 的移植。
6. §1.4 第 4 条写“boot 进程表为空……直接 panic（main.c:504）”，C 在 504 行只断言单个条目的 `start_addr`，并未检查表是否为空；建议改成“boot 条目缺 `start_addr` 时 panic”，避免给 C 添加不存在的检查。

## SUMMARY

- 计数：P0 = 3；P1 = 11；P2 = 5；P3 = 0；合计 19 条。审查期间文档被并发修订，先前关于 SEF 特殊映射行缺失、四名字表缺失、`register_page_alloc` 构造期归属（§4.4.1 侧）、测试窗口并行理由、§3.9 未承认两处时机差异的意见已在当前内容中解决，未计入。
- 最严重三条：
  1. DESIGN-D2-02（P0）：`is_first_time` 在内核侧被恒写为 1，C 的 RTS_BOOTINHIBIT 查询不存在；四态路由在真实入口只剩 `ColdInit`，`WarmRun`/`WarmNoState`/`DoubleInit` 不可达且无 C 依据，LU/restart 入口也无法经过 `read_boot_params`。
  2. DESIGN-D2-01（P0）：`exec_bootproc` 的“已实现/未实现”边界与代码相反——初始栈帧、真实 `vsp`/`ps_str`、`VMCTL_BOOTINHIBIT_CLEAR` 都已实现，文档仍说 `minix_stack_*` 未落地、以 0 上报。
  3. DESIGN-D2-03（P0）：`extra_pages()` 的“末条目排除”在 Rust 交接列表上丢掉一个真实模块（列表既无内核伪条目，也不含 VM 模块），文档却称“忠实复刻”，导致 `total_pages` 相对 C 少计，且测试用虚构形状固定了错误。
- 对“01 是否讲清了 VM 的诞生”的总判断：C 半场（§1–§2）基本讲清了，依赖链、SEF、RS_INIT 和 `exec_bootproc` 的 C 语义都有可复核的锚点；Rust 半场在三个承重结论上不可信（路由的输入是常量、装载边界写反、模块记账形状不符），并且完全漏掉了 A2 `reconcile` 这条生产入口契约。按当前内容不能判 CONVERGED：读者读完会得到一个比代码更“完整”的暖启动设计和比代码更“残缺”的 boot 装载链。先修三条 P0、补 A2 小节与 ARCH 标注，再把 Rust 边界表改写为可复核口径，01 才算讲清 VM 的诞生。
