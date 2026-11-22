DOC 01-vm-init-main.md

正文 SHA-256：`d97aefe967cc96cdde2d2ae78344b3dd9a9a326b69fe72bd9970193d3ce28fd8`
（`sha256sum rewrite-notes/02-stage-vm/01-vm-init-main.md`，全部修复后的最终冻结版本。修复轮次：`c0764c82…` → `3f56fd70…`（§5.4 计数）→ `adbb5b09…`（T2B-01 至 T2B-07 的七处）→ `d97aefe9…`（§2.2.7 第二句与 §2.2.6 统一）。本报告已逐处重读并据此判定。）

对照代码版本：`os/servers/vm/src/vm_server.rs` SHA-256 `c0c62076234696a7c0632659a0e7d932ed180b6c8b9675641d1c9093313dbad1`；`os/servers/vm/src/boot.rs` SHA-256 `66adb59fdd15fca507613311ebb09ccc15349c56c079b4262eae7d929a6de2e1`。本报告里所有 `vm_server.rs` 行号以上述哈希为准。

复核方式：只读；逐条把正文断言与 `minix3/` 的 C 源、`os/` 的 Rust 源对齐；所有命令与输出均在本工作树重放；未引用任何历史面板报告的结论。

范围：`1.0`、`1.2`、`1.4`、`2.0`、`2.2`、`2.3`、`2.4`、`3.1`、`3.3`、`3.4`、`3.5`、`3.6`、`3.8`、`4.0`、`4.4`、`5.1`、`5.2`、`5.4`。其中 `1.0`、`1.4`、`2.3`、`2.4`、`3.3`、`3.8`、`5.4` 由协调者在复核过程中补入。

---

## UNIT 1.0

VERDICT: PASS

FINDINGS: 0

核实记录：

1. 最小示例的 grep 命令逐字重放：

   ```text
   $ grep -n '^int main(void)$\|^void init_vm(void)$\|^static int is_first_time' minix3/minix/servers/vm/main.c
   79:static int is_first_time(void)
   93:int main(void)
   428:void init_vm(void)
   ```

   与正文第 38 至 41 行完全一致。C 的 428 行确实是 `void init_vm(void)`，没有 `static`；旧的 `static void init_vm` 模式匹配不到这一行，本处已改为 `^void init_vm(void)$`，可重放。

2. 失败对照（第 46 行）说门控必须先于任何初始化动作，锚点 `main.c:100-107`。C 的 100 行是 `if (is_first_time())`，101 行是 `init_vm()`，102 行是 `__vm_init_fresh=1`，106 行是 `sef_local_startup()`，107 行清 0；门控与 `init_vm()` 的先后关系成立。

## UNIT 1.2

VERDICT: PASS

FINDINGS: 0

核实记录：

1. 依赖树逐节点核对（`minix3/minix/servers/vm/main.c`）：`main()` 93 行；`is_first_time()` 79-88 行；门控 100-102 行；`sys_getkinfo` 442-444 行；`enable_filemap` 与 asserts 447-452 行；`get_mem_chunks` 455 行（`utility.c:44`）；`memset(vmproc)` 与 `vm_slot` 458-462 行；`acl_init` 465 行；`map_region_init` 468 行；`mem_init` 471 行（`alloc.c:306`）；`init_proc(VM_PROC_NR)` 474 行；`pt_init` 475 行；`__minix_init` 480 行；`mem_add_total_pages` 485-495 行；boot 进程循环 498-520 行；CALLMAP 522-575 行；实例标记与 SEF 特殊映射区 577-586 行；`sef_local_startup` 106 行。全部命中。

2. `main.c:577-586` 是一个连续块：577 行注释，578 行 `num_vm_instances = 1;`，579 行 `VMF_VM_INSTANCE`，581 行注释，582-583 行 `sef_llvm_add_special_mem_region`，584-586 行失败打印。正文第 88 行的“含 SEF 特殊映射区”与 §2.2.8 标题同一范围，彼此一致。

3. 树中“主循环 `while(TRUE)` main.c:110-192”的起点是 110 行（循环前的空行与注释），`while (TRUE)` 语句本身在 112 行，循环闭括号在 192 行。该范围是循环块的超集，覆盖完整的收消息、分发、回复与闭括号，没有漏行；不按错误计。

4. 两套编号对照表（第 104-110 行）逐行核对：A 段 442-452、B 段 455-471、C 段 474-475、D 段 480-520、E 段 522-586 与 C 一致；Rust 落点 `read_boot_params` + `validate`、分配器构造 + 编译期槽位 + 惰性区域表、`init_vm_self_pt()` + `init_vm_slot()`、`mark_initialized()` + `account_boot_memory()` + `init_boot_procs()`、编译时分发 + `mark_vm_instance()` 与 `vm_server.rs`、`boot.rs`、`main.rs` 的实现一致。SEF 启动与主循环不在 A–E 序列内的说明与 C 的 `main.c:106`、112 行一致。

## UNIT 1.4

VERDICT: PASS

FINDINGS: 0

核实记录：

1. 前提 1 引用的 `vm_server.rs:1692` 与当前代码一致：`grep -n 'pub fn run(&mut self)'` 输出 `1692:    pub fn run(&mut self) -> ! {`。原 T2B-01 已修复。

2. 前提 2 的 `pagetable.c:48-57`：`pagetable.c:55-58` 是 spare pages 注释，48-57 覆盖到注释主体 55-57；范围起点偏早、终点少 58 行的 `*/`，但指向明确，不按错误计。

3. 前提 3 与 4 的事实成立：`panic = "abort"` 在 `os/Cargo.toml:303`；`main.c:451-452` 是两个 assert，`main.c:504` 是 `assert(ip->start_addr)`。

## RESOLVED T2B-01（已修复）

- 位置：`01-vm-init-main.md:122`（§1.4 前提 1）
- 原问题：`VmServer::run` 的行号锚点曾写成 `vm_server.rs:1699`。
- 复核结论：冻结版已改为 `vm_server.rs:1692`，与当前代码（`c0c62076…`）一致；按可重放命令核对通过。

## UNIT 2.0

VERDICT: PASS

FINDINGS: 0

核实记录：

1. 最小示例命令逐字重放：

   ```text
   $ grep -n '^void init_vm(void)$\|^static struct vmproc \*init_proc\|^static void exec_bootproc' minix3/minix/servers/vm/main.c
   262:static struct vmproc *init_proc(endpoint_t ep_nr)
   331:static void exec_bootproc(struct vmproc *vmp, struct boot_image *ip)
   428:void init_vm(void)
   ```

   与正文第 164 至 167 行一致，顺序也符合 grep 按行号输出的实际行为。

2. 失败对照引 `main.c:458-462` 与 `main.c:276`：458 行 `memset(vmproc, 0, sizeof(vmproc))`，460-462 行写 `vm_slot`；276 行正是 `assert(!(vmp->vm_flags & VMF_INUSE));`。成立。

## UNIT 2.2

VERDICT: PASS

FINDINGS: 0

核实记录：

1. §2.2.1：代码块与 `main.c:442-452` 逐行一致；`kinfo_t` 在 `minix3/minix/include/minix/param.h`；两个 assert 的语义（`mmap_size > 0`、`mods_with_kernel > 0`）正确。

2. §2.2.2：`get_mem_chunks()` 在 `utility.c:44-79`，起始向上取整、结束向下取整（`utility.c:64-78`）；`memset(vmproc)` 458-462 行；`acl_init()` 在 `acl.c:21-30`（写 `NO_ACL` 与清静态数组）；`map_region_init()` 在 `region.c:36-38` 是空函数；`mem_init()` 在 `alloc.c:306-335`，正文写 306-334，少一行闭括号，但所有语句（319 行 `total_pages = 0`、321 行清位图、324-334 行累加）都在范围内；`free_pages_bitmap` 在 `alloc.c:35`，`free_pages()` 在 `alloc.c:465-476` 置位。结论成立。

3. §2.2.3 至 §2.2.4：474-475 行与 480 行锚点正确；`__minix_init()` 在 `minix3/minix/lib/libc/sys/init.c:20-32`，正文 20-33 覆盖函数与尾行。

4. §2.2.5 的三处修复核对：
   - `main.c:485-490` 的循环上界是 `mods_with_kernel-1`；`pre_init.c:194-199` 把内核登记为 `module_list[mi_mods_count]` 并令 `mods_with_kernel = mi_mods_count+1`，所以末条目被排除、内核占用走第二笔（492-495 行），正确。
   - `protect.c:449-454`：450 行 `add_memmap` 把 VM blob 还回 memmap，451 行 `mod->mod_end = mod->mod_start = 0`，454 行写 `vm_allocated_bytes`。因此 C 的模块循环在 VM 那一行算 `0-0+1 = 1` 字节，向上取整后正好 1 页。正文的 1 页口径成立。
   - `vm_allocated_bytes` 只被 `region.c:1369` 的 `get_usage_info_vm` 读取（函数 1366-1373 行），不参与 `total_pages`。正文正确。
   - Rust 交接页的模块列表确实排除了 VM 模块（`os/kernel/src/vm_handoff.rs:598-613` 跳过 `vm_module_idx`），且不含内核伪条目；`BootParams::extra_pages()` 对全部条目计费后再补 1 页，`test_boot_params_extra_pages_vm_module_row` 断言为 1。与 C 的口径一致。

5. §2.2.6 的代码块与 `main.c:498-520` 一致；新增段已改为“是 C 的源码顺序，不是硬依赖”，并给出真实前置（`pt_init_done=1` 后页目录/页表页由 `alloc_mem` 从位图供给，保留页池只服务自映射的递归分配；`pagetable.c:328,350,1311` 可核）。§2.2.7 第二条也已改为“CALLMAP 只需在主循环之前；C 把它排在 boot 进程循环之后……”。两处口径一致，原 T2B-02 已修复。

6. §2.2.7 的代码块与 `main.c:522-575` 一致，`NR_VM_CALLS` 的值 49 与 `com.h:769` 一致；注释已改为“以上 26 项即 C 的全部注册；`NR_VM_CALLS = 49，其余 23 个调用号未注册，主循环返回 ENOSYS`”；`CALLNUMBER` 锚点已改为 `main.c:57-59`。原 T2B-03、T2B-04 均已修复。

7. §2.2.8：代码块与 577-583 行一致；三条说明与 C 行为相符。附带说明：正文“实例标记必须在槽激活之后，因为它要求 `VMF_INUSE` 已置位”是语义约束，`vm_flags` 的按位或不检查 `VMF_INUSE`；但 `init_proc(VM_PROC_NR)` 在 474 行已把槽激活，顺序结论本身成立，不单独记 finding。

## RESOLVED T2B-02（已修复）

- 位置：`01-vm-init-main.md:345`（§2.2.6）与 `:391`（§2.2.7 第二条）
- 原问题：§2.2.6 曾写“必须排在 `__minix_init` 与 `mem_add_total_pages` 之后”，§2.2.7 曾写“CALLMAP 必须在 boot 进程循环之后”，两处都把源码顺序说成硬依赖。
- 复核结论：§2.2.6 已改为“是 C 的源码顺序，不是硬依赖”，并给出真实前置（`pt_init_done` 后的位图供给）；§2.2.7 已改为“CALLMAP 只需在主循环之前；C 把它排在 boot 进程循环之后……”。两处一致，原矛盾消除。

## RESOLVED T2B-03（已修复）

- 位置：`01-vm-init-main.md:387`（§2.2.7 代码块末尾）
- 原问题：`/* 其余 23 项略 */` 会被读成另有 23 条 `CALLMAP` 注册。
- 复核结论：冻结版已改为“以上 26 项即 C 的全部注册；`NR_VM_CALLS = 49，其余 23 个调用号未注册，主循环返回 ENOSYS`”，与 C 的 26 条注册及 §3.8 的 26/None 口径一致。

## RESOLVED T2B-04（已修复）

- 位置：`01-vm-init-main.md:390`（§2.2.7 第一条）与 `:314`（§2.2.5 第一条）
- 原问题：`CALLNUMBER` 锚点 `main.c:54-58` 与内核伪模块登记锚点 `pre_init.c:198-200` 没有覆盖关键行。
- 复核结论：冻结版已分别改为 `main.c:57-59` 与 `pre_init.c:196-199`；两处范围现在覆盖 `#define CALLNUMBER` 的结论行与 `module_list[k]` 的两条赋值。

## UNIT 2.3

VERDICT: PASS

FINDINGS: 0

核实记录：

1. 标题与 §2.3.1 的 `main.c:262-286` 正确：`init_proc()` 的闭括号在 286 行，`vm_exec_info` 结构从 288 行才开始；代码块与 262-286 行逐行一致，包括 272-273 的范围 panic、276 行的 `VMF_INUSE` 断言、277-280 行的清槽与字段赋值、285 行的 `panic("no init_proc")`。

2. §2.3.2 的五个子步骤逐项核对 `main.c:331-417`：`pt_new`/`pt_bind` 352-356 行；`sys_physcopy` 358-360 行；`exec_info` 填充与五个回调 362-381 行；`libexec_copy_physcopy` 294-303 行；`boot_alloc` 305-315 行（`map_page_region` 的参数与标志正确）；`libexec_alloc_vm_prealloc` 317-322 行（`MF_PREALLOC`）；`libexec_alloc_vm_ondemand` 324-329 行（flags 0）；`libexec_load_elf` 383-386 行；栈 388-398 行；`handle_memory_once` 400-402 行；`sys_datacopy` 404-406 行；`sys_exec` 408-412 行；`VMCTL_BOOTINHIBIT_CLEAR` 415-416 行。全部命中。

## UNIT 2.4

VERDICT: PASS

FINDINGS: 0

核实记录：

1. §2.4.1：代码块与 `main.c:219-239` 一致；两个 `sef_setcb_init_lu`/`restart` 指向同一回调、`__vm_init_fresh` 门控异步应答、`sef_startup()` 收尾都对得上。

2. §2.4.2：代码块与 `main.c:241-260` 一致；`sys_safecopyfrom` 的参数顺序（`RS_PROC_NR`、`rproctab_gid`、0、`rprocpub`、`sizeof`）与 246-247 行一致。

3. §2.4.3：`sef_cb_init_response_rs_asyn_once` 在 `minix3/minix/lib/libsys/sef_init.c:471-483`，代码块与源一致（`asynsend3(RS_PROC_NR, m_ptr, AMF_NOREPLY)`、随后恢复默认同步应答）；主循环 RS_INIT 分支 `main.c:149-152` 中 `result = SUSPEND` 的“不二次回复”语义正确。

4. §2.4.4 至 §2.4.5：`main.c:196-217` 的 `sef_cb_lu_state_changed`；`592-672` 的 `sef_cb_init_vm_multi_lu`（多组件过滤与 `do_rs_update`）；`677-726` 的 `sef_cb_init_lu_restart`（`swap_proc_slot`、`swap_proc_dyn_data`、`pt_bind`、`adjust_proc_refs`，末尾 725 行调 multi-LU）；`731-750` 的 `sef_cb_signal_handler`（`SIGKMEM` 转 `do_memory`、`missing_spares` 补页、`pt_clearmapcache`）。标题 `219-260、592-750` 与这些范围一致。

## UNIT 3.1

VERDICT: PASS

FINDINGS: 0

核实记录：

1. `BootParams` 的 11 个字段与 `boot.rs:61-130` 的顺序、类型、注释语义一致：`root_paddr`、`kerninfo_pa`、`total_pages`、`free_regions`、`boot_procs`、`modules`、`kernel_allocated`、`vm_allocated_bytes`、`is_first_time`、`kernel_layout`、`user_sp`。

2. `validate()`（`boot.rs:187-245`）逐项核对：根页表非零且页对齐、内核信息页 0 或页对齐、空闲区非空且逐项合法、`total_pages == max((base+size)/CLICK_SIZE)`、boot 进程号小于 `NR_BOOT_PROCS`、VM 槽最多一次。正文第 670 行的概括成立；“`mods_with_kernel > 0` 不强制”的理由与代码注释一致。

3. §3.1 的 ARCH 注记（第 668 行）已把 `vm_allocated_bytes` 定性为 usage 统计口径、不参与 `total_pages`，与 §2.2.5、§3.6 的新口径一致。

4. §3.1.1 的四条 reconcile 断言与 `boot.rs:433-482` 一致；调用点与函数本体现在分别写为 `boot.rs:387` 与 `boot.rs:433-482`，且补上了“构造后先 `params.validate()`”。原 T2B-05 已修复。

## RESOLVED T2B-05（已修复）

- 位置：`01-vm-init-main.md:674`（§3.1.1）
- 原问题：reconcile 的锚点曾写成 `boot.rs:374-470`，起点在 `BootParams` 构造内部、终点在 reconcile 的循环中间。
- 复核结论：冻结版已改为“调用点 `boot.rs:387`，函数本体 `boot.rs:433-482`”，并补入 `params.validate()`（386 行）这一相邻步骤；与当前 `boot.rs` 逐行一致。

## UNIT 3.3

VERDICT: PASS

FINDINGS: 0

核实记录：

1. 表内 14 行逐项核对：`global::init` + `set_kernel_layout`（`vm_server.rs:777`、809）；`BootParams::validate()`（构造期，351 行）；`create_default_allocator` + `VmPageAllocator::new`（`vm_server.rs:353-354`）；编译期槽位与 `get_empty()` 写 `vm_slot`；`AclState::Uninitialized` 编译期默认；`RegionMap::new()` 惰性；`init_vm_slot()` 走 `init_proc()`（`vm_server.rs:1346-1389`，包含 `activate`、`init_page_table`、`init_regions`、`set_boot`）；`init_vm_self_pt()` 构造期 adopt（407 行）；`mark_initialized()` 在 `init()` 末尾（726 行）；`account_boot_memory()` Phase 2b；`init_boot_procs()` + `exec_bootproc()` Phase 2c；编译时 CALLMAP；`mark_vm_instance()` Phase 2d；SEF 特殊映射区没有对应调用。全部与代码和正文其他节一致。

2. 表内 CALLMAP 行写“编译时 match”、§3.8 写“编译期解析 49 个调用号空间”。实现是编译期 `CALLMAP` 常量表（`dispatcher.rs:849-851` 的 `build_callmap`）加下标查表，再对查表结果做 `match`（806-816 行）；没有 49 臂的 `match`。两处用“编译时 match”作简称没有改变“C 运行期注册、Rust 编译期固定”的对比结论，且 §3.8 已补上 26/None/ENOSYS 的事实，不再单独记 finding；若要更精确，可写“编译期 `CALLMAP` 表 + 下标分发”。

3. 当前实现边界段（第 733 行）与 §3.5、§4.4.2 的新口径一致：ELF 装载、`minix_stack_*` 栈帧、`sys_exec`、blob 释放已实现，`name_ptr` 仍为 0。

## UNIT 3.4

VERDICT: PASS

FINDINGS: 0

核实记录：

1. 两遍 ACL 的实现与正文一致：`rs_handshake()` 在 `vm_server.rs:2141-2177`。第一遍只读校验：`vm_isokendpt`（`vmproc/table.rs:344-364`，只读检查 endpoint 与 `IN_USE`）与 `get_active`（`vmproc/table.rs:230-242`，只读检查槽状态），把 `(slot, acl_set(...))` 放进 `updates`；第二遍才 `set_acl(acl)`（2174 行）。`VmError` 类的失败只可能发生在第一遍，因此“失败无半初始化 ACL、无需回滚”成立。`drop(proc)` 只是让出借用，不是状态修改。

2. 主循环优先级 2 的行为与正文一致：`dispatch_on_msg` 在 1944 行，RS_INIT 分支在 1961 行起；成功臂 `asynsend(sef_init_reply(OK))` 后返回 `Suspend`（约 1987-1995 行）；失败臂先 `asynsend(sef_init_reply(e.to_errno()))`（2011 行），再计数并审计（2024-2036 行），返回 `NoReply`；两臂都不走 dispatcher 的二次回复。C 在 `main.c:151` 的 panic 对照正确。

3. 表内其余行：`ipc_call_rs_init()` 在 2471 行（`grep -n` 仍可命中）；`rs.rs` 的 `handle_rs_prepare` 在 191 行、`handle_rs_update` 在 357 行；`effective_kernel_signal` 在 1483 行、`handle_signal` 在 1457 行、`do_memory` 在 1504 行；`SIGNAL_KERNEL_MEMORY = 71` 在 `os/libs/minix-types/src/types/signal.rs:100`；`SEF_SIGNAL_REQUEST_TYPE` 在 `os/libs/minix-sef/src/lib.rs:69`；`SigSet` 128 位与位 70/74 的说明与 `os/kernel/src/syscall_signal.rs:70-104` 一致。

4. `RsInit` 的字段清单在 `os/libs/minix-types/src/ipc/rs.rs:97-116`；`init_type`、`flags`、`old_endpoint`、`restarts`、`buff_addr`、`buff_len`、`prepare_state` 在 `os/servers/vm/src` 中没有消费点，`rproctab_gid` 被 `ipc_call_rs_init` 消费。正文的“解码后未消费”成立。

5. “`rs_handshake()`（约 30 行）”是量级描述；函数当前约 38 行（含注释），代码行约 20 行，不构成事实错误。

## UNIT 3.5

VERDICT: PASS

FINDINGS: 0

核实记录：

1. 页表行：`init_proc()` 调 `proc.init_page_table()`（`vm_server.rs:1365`），生产用 `minix_arch::CurrentPaging`（`pagetable/mod.rs:32`），测试用 `SimPaging`（同文件 34 行）；`exec_bootproc` 里的 `sys_vmctl_set_addrspace` 在 968 行。正文的对应关系正确。

2. ELF 行：Direct Map 直读映像在 921-930 行；`entry_point` 在 933 行；`segment_iter` 在 937 行；逐段匿名 region 在 1051-1066 行；`file_pages` 循环里 `alloc_pfn`/`map_page` 与段字节写入在 1072-1137 行；in-page 头尾 BSS 清零在 1123-1133 行。需要说明的是只有文件页被立即实化，BSS 整页尾部依赖按需零页，正文的“逐页实化”是压缩写法；不影响“已实现”的总判断。

3. 栈建立行：`install_boot_stack` 调用点在 1140 行，定义在 1154 行；`stack_params` 在 1168 行、`stack_fill` 在 1173 行；4 MiB `DEFAULT_STACK_LIMIT` 在 1201 行；`handle_memory_once` 在 1262 行；Direct Map 写帧在 1293-1299 行；`sys_exec(endpoint, entry, vsp, 0, filled.ps_str)` 在 1310 行。正文的“真实 `vsp`/`ps_str`、唯一余项 `name_ptr = 0`”成立。

4. `sys_exec` 与 `VMCTL_BOOTINHIBIT_CLEAR` 行：`MessLsysKrnSysExec` 的五个字段在 `minix-types/src/ipc/message.rs:1808-1811`；`Syscall::Exec = 1` 在 `os/kernel/src/syscall.rs:126`，`dispatch_exec` 在 1064 行；`sys_vmctl_boot_inhibit_clear` 在 `kernel_gateway.rs` 的 `VMCTL_BOOTINHIBIT_CLEAR = 33` 路径上。成立。

5. 槽位行：`init_boot_procs()` 在 846 行，跳过负 `proc_nr`、VM 自身与 `endpoint == NONE`，`assert(start_addr != 0)`，循环内调 `exec_bootproc` 并释放 blob（871-877 行）。释放动作的 C 锚点已改为 `main.c:516-519`，代码注释同步改为 `516-519`。原 T2B-06 已修复。

## RESOLVED T2B-06（已修复）

- 位置：`01-vm-init-main.md:766`（§3.5 表“槽位建立”行）
- 原问题：释放 blob 的 C 锚点曾写成 `main.c:513-516`，范围止于注释，读不到 `free_mem`。
- 复核结论：冻结版已改为 `main.c:516-519`（注释、assert、roundup、`free_mem`），`os/servers/vm/src/vm_server.rs:871` 的代码注释也已同步改为 `516-519`；逐行核对通过。

## UNIT 3.6

VERDICT: PASS

FINDINGS: 0

核实记录：

1. `global::add_total_pages` 的实现与正文片段一致（`os/servers/vm/src/global.rs:80-82`，`*TOTAL_PAGES.get() += pages;`）。

2. `extra_pages()`（`boot.rs:263-289`）三轮核对：全部 `self.modules` 向上取整按页计费（271-274 行）；再固定 `pages += 1`（280 行）对应 C 被清零的 VM 模块行；最后 `round_up_page(static_bytes)` 加 `dynamic_bytes`（286-288 行）。`vm_allocated_bytes` 没有进入任何一轮。与 `main.c:485-495` 的两笔账逐项对应。

3. C 侧只有 `region.c:1369` 读取 `vm_allocated_bytes`；`utility.c:118` 的 `do_info` 的 `VMIW_STATS` 分支读取 `total_pages`，与正文“统计与查询”一致。四名字表（`BootParams.total_pages`、`boot_usable_pages`、`VmPageAllocator::total_pages()`、`global::total_pages()`）的口径分别对上 `boot.rs:363-367`、`vm_server.rs:424-428`、分配器自身与 `global.rs`。

4. `PageFrames::new(total_phys)` 在 `region/page_state.rs:239-243` 按 `total_phys / PAGE_SIZE` 建立逐帧状态数组；正文“物理帧元数据表，不是分配器位图”成立。

5. 测试读数：`test_boot_params_extra_pages_modules` 断言 5（1+2+1 三模块 + 1 VM 行）、`..._vm_module_row` 断言 1、`..._kernel` 断言 5；`test_vm_server_init_accounts_boot_memory` 断言 `TEST_TOTAL_PAGES + 3`（两个模块各 1 页 + VM 行 1 页）。三处与正文口径一致，实际运行均通过。

## UNIT 3.8

VERDICT: PASS

FINDINGS: 0

核实记录：

1. `minix3/minix/include/minix/com.h:769` 是 `#define NR_VM_CALLS 49`，正文的 `com.h:769` 已可重放；Rust 侧 `minix_types::NR_VM_CALLS = 49`（`os/libs/minix-types/src/ipc/vm.rs:163`）。

2. `CALLMAP` 表有 49 槽（`dispatcher.rs:838-849`）；`build_callmap()` 注册 26 个 handler（`grep -c '= Some(call_'` 输出 26）；`dispatch_by_number` 对查表结果 `Some` 调用、`None` 返回 `VmError::NotImplemented`（即 ENOSYS）。正文“49 个调用号空间；其中 C 注册的 26 个调用有 handler，其余位置为 `None` 并返回 ENOSYS”与代码一致。

3. 机制表述的精确化空间：实现不是 49 臂 `match`，而是编译期 `CALLMAP` 常量表加下标查表，`match` 只针对查表结果。正文没有声称 49 臂，且 26/None/ENOSYS 三项事实正确，因此不记 finding；若下一次润色，可改为“编译期 `CALLMAP` 表（`build_callmap`）+ 下标分发”。

## UNIT 4.0

VERDICT: PASS

FINDINGS: 0

核实记录：

1. 最小示例的 grep 命令按正文第 875 行的模式逐字重放，实际输出为：

   ```text
   os/servers/vm/src/main.rs:41:fn real_main() -> ! {
   os/servers/vm/src/main.rs:81:        match boot_route(params.is_first_time) {
   os/servers/vm/src/vm_server.rs:265:pub fn install_server(server: VmServer) -> Result<(), VmServer> {
   os/servers/vm/src/vm_server.rs:648:    pub fn init(&mut self) {
   os/servers/vm/src/vm_server.rs:1692:    pub fn run(&mut self) -> ! {
   ```

   正文第 878-880 行现在印的也是 265、648、1692，与重放输出一致；原 T2B-07 已修复。

2. 失败对照（第 885 行）说入口先把 server 装入 image slot、再在 `with_installed_server` 里调 `init()`，锚点 `main.rs:81-100`。`main.rs:81-89` 是 ColdInit 分支、`install_server`、`with_installed_server(|server| server.init())`；100 行起是 `e5d_self_pt_test` 的调用。范围覆盖了所述步骤，成立。

3. §4.0 章首承诺的四个问题（四态路由、image slot 与 `init()` 的先后、Phase 1/2a–2d 与 C 的对应、`run_installed()` 进入主循环）在 §4.2 与 §4.4 都有对应叙述，事实部分与本报告各单元一致。

## RESOLVED T2B-07（已修复）

- 位置：`01-vm-init-main.md:878-880`（§4.0 最小示例的输出块）
- 原问题：输出块曾印 266、649、1699，与本工作树的 265、648、1692 不符。
- 复核结论：冻结版输出块已是 265、648、1692，按正文模式逐字重放得到同样五行；可重放性恢复。

## UNIT 4.4

VERDICT: PASS

FINDINGS: 0

核实记录：

1. §4.4.1 的构造顺序与 `new_inner()` 一致：`params.validate()`（351 行）、`create_default_allocator`（353 行）、`VmPageAllocator::new`（354 行）、`init_vm_self_pt(params.root_paddr)`（406-407 行，非 test）、拷贝 `boot_procs`（411-415 行）、`boot_extra_pages = params.extra_pages()`（427 行）、`boot_usable_pages = Σ size/CLICK_SIZE`（430-434 行）。`register_page_alloc` 不在构造期，正文的说明正确。

2. §4.4.2 的代码块是骨架摘录：省略了 `e5d_diag` 与 `register_reclaim`（`vm_server.rs:707-709`），但 Phase 1/2a/2b/页帧表/2c/2d/`mark_initialized`/`initialized = true` 的顺序与源码一致（648-730 行）。正文的“两处完成时机不同”说明与 §3.3 一致。

3. `init_global_state()` 用 `global::init(boot_usable_pages)`（777 行）并 `set_kernel_layout`（809 行）；正文只列这两项，代码还在 817 行 `set_kerninfo_pa`，属于摘要省略，不影响所述事实。

4. `init_vm_slot()`（822-838 行）从 `boot_procs` 找 `VM_PROC_NR`，生产缺失即 panic、测试跳过；`init_proc()`（1346-1389 行）做 `get_empty`、`activate`、`init_page_table`、`init_regions`、`set_boot`；`account_boot_memory()`（1392-1400 行）在 `boot_extra_pages > 0` 时加总；`mark_vm_instance()`（1404-1409 行）对 VM 槽调 `mark_vm_instance` 并递增实例计数；`Drop`（2409-2428 行）调用 `unregister_page_alloc`。全部与正文一致。

5. §4.4.3：`run()`（1692 行）与 `run_once()` 经 transport 收发；`dispatch_on_msg`（1944 行）优先级 2 的 RS_INIT 分支与 §3.4 一致；`missing_spares`/`alloc_cycle` 补页链在 Rust 没有对应实现（`vm_server.rs:1462-1465`、1689-1692 的注释与 `handle_signal` 实现都说明了删除），`pt_clearmapcache` 无对应物；回收走 `alloc_pfn_reclaiming`（`alloc_page.rs:195`）。测试名 `test_run_once_rs_init_handshake_copies_and_maps_service` 与 `test_run_once_rs_init_fails_closed_on_safecopy_error` 均存在并通过。

## UNIT 5.1

VERDICT: PASS

FINDINGS: 0

核实记录：

1. §5.1 列出的 14 个测试与 `os/servers/vm/src/boot.rs` 的 14 个 `#[test]` 一一对应，包括新增的 `test_boot_params_extra_pages_vm_module_row`；旧的 `test_boot_params_extra_pages_vm_image` 已不存在，正文没有残留引用。

2. 覆盖描述与断言一致：三个 `extra_pages` 测试的期望值分别为 5、1、5（与 §3.6 的“模块 + 固定 1 页 + 内核”口径一致）；`adopt_free_regions_keeps_every_region_verbatim` 断言原样保留；四个 `reconcile` 负控与 `reconcile()` 的断言一一对应。

3. 实际运行：

   ```text
   $ cd os && cargo test -p minix-vm --lib boot_params_extra_pages
   3 passed; 0 failed; 546 filtered out
   ```

   三个测试全部通过。

## UNIT 5.2

VERDICT: PASS

FINDINGS: 0

核实记录：

1. §5.2 列出的 9 个测试都存在：`test_vm_server_init`、`test_vm_server_run_without_init`、`test_init_rejects_missing_kernel_layout`、`test_vm_server_init_with_boot_procs`、`test_vm_server_init_vm_instance_count`、`test_vm_server_init_accounts_boot_memory`、`test_install_boot_stack_writes_frame_and_exec_values`、`test_run_once_rs_init_handshake_copies_and_maps_service`、`test_run_once_rs_init_fails_closed_on_safecopy_error`。

2. 描述与测试体一致：`test_vm_server_init_with_boot_procs` 断言 VM 槽 active 且 `is_vm_instance()`、PFS 槽 active；`test_vm_server_init_accounts_boot_memory` 断言 `TEST_TOTAL_PAGES + 3`；`test_install_boot_stack_writes_frame_and_exec_values` 断言 mock 收到的 `sys_exec` 参数为 `(PFS, entry, vsp, ps_str)`，并从 Direct Map 读回帧字节与参考布局相等。

3. 测试边界说明正确：`exec_bootproc()` 与 `init_boot_procs()` 里的调用都被 `#[cfg(not(test))]` 挡住（`vm_server.rs:864-877`、909 行），栈帧子步骤由 `install_boot_stack` 的宿主测试单独覆盖。

4. 实际运行：

   ```text
   $ cd os && cargo test -p minix-vm --lib rs_init
   2 passed; 0 failed; 547 filtered out
   $ cd os && cargo test -p minix-vm --lib install_boot_stack
   1 passed; 0 failed; 548 filtered out
   ```

   与 §5.3 的 `549 passed; 0 failed` 基线一致（本工作树重跑同样得到 549/0）。

## UNIT 5.4

VERDICT: PASS

FINDINGS: 0

核实记录：

1. 表格五行路标与代码一致：`main.rs:59` 的 `e5d: before-read-boot-params`、64 行的 `e5d: boot-params-read`、78 行的 `e5d: vm-main-enter`、96 行的 `e5d: vm-init-done`；104 与 114 行分别是 `E5D-VM-SELF-PT PASS` 与 `FAIL`，正文“104-116”覆盖这两条。

2. 冻结版首句（第 1102 行）已改为“提供四段 `e5d:` 输出加一段自测结果（共五行）”。表格正好四行带 `e5d:` 前缀的路标加一行自测结果，计数与内容一致；起草时看到的“前三段”与五行不符的问题已由作者在冻结前修正，不再记 finding。

3. “本篇不声称这些路标已被独立 QEMU 门断言”的缺口声明与“宿主 549/0 不能证明真机按序”的判断一致，属于诚实边界说明。

---

HARD-REJECTIONS: 0
