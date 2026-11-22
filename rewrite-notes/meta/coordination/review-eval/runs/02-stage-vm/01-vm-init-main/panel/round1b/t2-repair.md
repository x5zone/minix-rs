DOC 01-vm-init-main.md

复核范围：`1.2` 初始化依赖链图、`2.2` 的 `init_vm()` C 源码分析、`4.4` 的 `vm_server.rs` 实现说明。事实来源为 `minix3/minix/servers/vm/main.c`、`acl.c`、`region.c`、`vmproc.h` 与 `os/servers/vm/src/vm_server.rs`、`vmproc/vmproc.rs`、`vmproc/vmproc_handle.rs`、`global.rs`。本报告的结论均来自上述源码与目标文档的直接核对，未引用任何历史面板报告。

## UNIT 1.2

VERDICT: PASS

FINDINGS: 0

核实记录：

1. C 侧范围。`main.c:577-586` 是一个连续块：577 行是 `/* Mark VM instances. */`，578 行是 `num_vm_instances = 1;`，579 行是 `vmproc[VM_PROC_NR].vm_flags |= VMF_VM_INSTANCE;`，581 行是 SEF 注释，582-583 行是 `sef_llvm_add_special_mem_region(...)` 调用，584-586 行是该调用的失败打印分支。文档第 88 行标为 `main.c:577-586（含 SEF 特殊映射区）`，范围与 C 块完全对齐。

2. 与 §2.2.8 标题一致。文档第 391 行的标题是 `#### 2.2.8 VM 实例标记（main.c:577-586）`，与第 88 行使用同一范围；该小节正文的代码块与三条说明覆盖 `num_vm_instances`、`VMF_VM_INSTANCE` 标记与 `sef_llvm_add_special_mem_region` 三件事。

3. 与 §3.3 表一致。第 728 行把实例标记记为 `main.c:577-579`，第 729 行把 SEF 登记记为 `main.c:582-583`。这是对同一连续块的按操作拆分：580-581 行是空行与注释，584-586 行是 SEF 调用的失败分支，没有遗漏独立操作；粗粒度的依赖图节点与细粒度的映射表不冲突。第 110 行的 E 阶段范围 `522-586` 同样覆盖该块。

4. 未发现新的前后矛盾或锚点错误。`init_vm()` 的函数范围在 §2.2 标题中写为 `main.c:428-587`，与 C 的闭括号 587 行一致；第 88 行的末端 586 行没有越过函数边界。

## UNIT 2.2

VERDICT: PASS

FINDINGS: 0

核实记录：

1. `acl_init()` 的锚点。`acl.c:21` 是函数头，`acl.c:25-26` 遍历 `ELEMENTS(vmproc)` 写 `vmproc[i].vm_acl = NO_ACL`，`acl.c:28-29` 分别清空静态数组 `acl_mask` 与 `acl_inuse`，`acl.c:30` 是闭括号；文档第 258 行标注的 `acl.c:21-30` 精确。`NO_ACL` 在 `acl.c:10` 定义为 `-1`，`vm_acl` 在 `vmproc.h:23` 是 `int`，因此“清零写成 0”与“-1 哨兵”的对比成立。

2. 顺序依赖。C 的 `memset(vmproc, 0, sizeof(vmproc))` 在 `main.c:458`，`acl_init()` 在 `main.c:465`。清零会把每个槽的 `vm_acl` 写成 0，即 `USER_ACL`；只有 `acl_init()` 这一遍会把全部槽恢复为 `NO_ACL`。若把它提前到 `main.c:458` 之前，随后的清零会覆盖这个哨兵。文档第 258 行的理由与 C 行为一致。文中的“只有这一遍”指启动期整表清零之后的恢复；单槽销毁路径上的 `acl_clear()` 不属于启动期整表恢复，不构成反例。

3. `map_region_init()`。`region.c:36` 是函数头，37 行是 `{`，38 行是 `}`，函数体为空。因此“当前是空函数，顺序上没有依赖”成立；它排在 `memset` 之后不改变任何可观察状态。文档第 259 行标注的 `region.c:36-38` 精确。

4. 交叉引用一致。文档第 251-253 行的代码块按 `acl_init(); map_region_init(); mem_init(mem_chunks);` 排列，与 C 的 465、468、471 行一致；§3.3 第 720-721 行把 `acl_init()` 记为 `main.c:465`、把 `map_region_init()` 记为 `main.c:468`，与 §1.2 第 79-80 行一致。未发现新的前后矛盾或锚点错误。

## UNIT 4.4

VERDICT: PASS

FINDINGS: 0

核实记录：

1. C 调用点。`main.c:582-583` 是两行调用 `s = sef_llvm_add_special_mem_region((void*)VM_OWN_HEAPBASE, VM_OWN_MMAPTOP-VM_OWN_HEAPBASE, "%MMAP_ALL");`，584-586 行是 `if(s < 0)` 的失败打印。文档第 975 行标注的 `main.c:582-583` 精确。

2. Rust 侧没有对应实现。对 `os/` 全树的 Rust 源检索 `sef_llvm_add_special_mem_region`、`MMAP_ALL`、`special_mem`、`special_mmap`，并检索 `os/libs/minix-sef` 中的 `special` 与 `llvm`，均无命中。`VmServer::init()`（`vm_server.rs:649`）的 Phase 2d 只调用 `self.mark_vm_instance()`（`vm_server.rs:720-721`），源码注释也只记 `main.c:577-579 VM instance mark`。因此“当前未实现”准确。

3. 与 §3.3 表一致。第 729 行写“SEF 特殊映射区登记（main.c:582-583）｜当前没有对应调用；`%MMAP_ALL` 区登记随 Live Update 归 25｜`25-rs-services.md`”，与 §4.4.2 的 Phase 2d 注释同口径；§6 第 1135 行也把“SEF 特殊映射区登记”列在 `25-rs-services.md` 的委托范围内。三处一致。

4. `mark_vm_instance()` 的本地事实。`vm_server.rs:1411-1416` 从 `VmProcTable::get_global()` 取 VM 槽；`vmproc_handle.rs:277-280` 的 `ActiveProc::mark_vm_instance()` 设置 `VmFlags::VM_INSTANCE` 并调用 `global::inc_vm_instance()`；`vmproc.rs:190-192` 的 `VmProc::clear()` 在 `VM_INSTANCE` 置位时调用 `dec_vm_instance()`。§4.4.2 第 1000 行的“配对 `VmProc::clear()` 的递减”与代码一致。

5. 未发现新的前后矛盾或锚点错误。Phase 2d 的 577-579 与 SEF 的 582-583 覆盖 C 块中的两个操作；`mark_initialized()` 在 Phase 2d 之后、C 的 `__minix_init()` 在 480 行而实例标记在 577-586 行，先后顺序不同，但文档 §4.4.2 第 989-990 行已明确说明这是两处完成时机差异，不是本次修正引入的新矛盾。

HARD-REJECTIONS: 0
