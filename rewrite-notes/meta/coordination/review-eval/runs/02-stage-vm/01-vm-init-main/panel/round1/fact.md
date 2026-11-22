# P-FACT r1：01-vm-init-main 定点事实核对

核对对象：`rewrite-notes/02-stage-vm/01-vm-init-main.md`（1144 行）
核对方式：只读文档指定章节与对应 C/Rust 源；第 9 项实际执行 `cd os && cargo test -p minix-vm --lib`。

## CHECK 1 PASS

C `main()` 的首次启动顺序与文档 §2.1 一致。

- 证据：`minix3/minix/servers/vm/main.c:79-88`（`is_first_time` 定义）、`:100`（`if (is_first_time())`）、`:101`（`init_vm()`）、`:102`（`__vm_init_fresh=1`）、`:106`（`sef_local_startup()`）、`:107`（`__vm_init_fresh=0`）。
- 文档 §2.1（`01-vm-init-main.md:184-191`）逐行标注 100/101/102/106/107，与源码完全一致；`is_first_time()` 的 `sys_getproc(RS_PROC_NR)` + `RTS_ISSET(..., RTS_BOOTINHIBIT)` 实现也与文档 195-208 行一致。

## CHECK 2 FAIL

`init_vm()`（`main.c:428-587`）的步骤顺序与文档 §2.2 完全一致，但 §2.2.7 的 CALLMAP 行号锚点少算 2 行。

- 步骤顺序证据（全部命中）：
  - `main.c:442` `sys_getkinfo`，`:451-452` 两个 `assert`（文档 §2.2.1）
  - `main.c:455` `get_mem_chunks`，`:458` `memset(vmproc)`，`:460-462` `vm_slot=i`，`:465` `acl_init()`，`:468` `map_region_init()`，`:471` `mem_init()`（§2.2.2）
  - `main.c:474` `init_proc(VM_PROC_NR)`，`:475` `pt_init()`（§2.2.3）
  - `main.c:480` `__minix_init()`（§2.2.4）
  - `main.c:485-495` 模块循环 + 内核占用两笔 `mem_add_total_pages`（§2.2.5）
  - `main.c:498-520` boot 进程循环（§2.2.6）
  - `main.c:522-575` CALLMAP 块（§2.2.7）
  - `main.c:577-586` `num_vm_instances` / `VMF_VM_INSTANCE` / `sef_llvm_add_special_mem_region`（§2.2.8）
- §2.2.5 的“末条目是内核伪模块”也核实为真：`minix3/minix/kernel/arch/i386/pre_init.c:194-199` 把内核放在 `module_list[mi_mods_count]`，`mods_with_kernel = mi_mods_count + 1`，`main.c:486` 的循环上界 `mods_with_kernel-1` 正是排除该条目。
- 不一致点：文档 §2.2.7 标题与 §3.3 表都把 CALLMAP 写成 `main.c:522-573`，实际最后一条注册在 575。

### FINDING FACT-01（P2）

- 位置：`rewrite-notes/02-stage-vm/01-vm-init-main.md:345`（§2.2.7 标题）、`:727`（§3.3 表 CALLMAP 行）
- 问题：CALLMAP 块的锚点 `main.c:522-573` 比实际短 2 行；按该范围读会漏掉最后一条注册 `VM_GETRUSAGE`。
- 证据：`main.c:572` 是 `CALLMAP(VM_CLEARCACHE, do_clearcache);`，`:573` 是空行，`:574` 是 `/* getrusage */`，`:575` 是 `CALLMAP(VM_GETRUSAGE, do_getrusage);`；文档自己的代码块也把 getrusage 列在最后。
- 最小修复：两处统一改为 `main.c:522-575`。
- 置信度：高（`grep -n` 与逐行 `sed -n '570,576p'` 直接取证）。

## CHECK 3 PASS

`exec_bootproc()`（`main.c:331-417`）五步与文档 §2.3.2 一致，五个步骤的行号全部命中。

- 证据：
  1. 页表：`main.c:352-356`（`pt_new` / `pt_bind`）
  2. ELF 头：`main.c:358-360`（`sys_physcopy(NONE, ip->start_addr, SELF, hdr, ...)`）
  3. libexec 装载：`main.c:362-381`（`exec_info` 填充 + 五个回调），`main.c:383-386`（`libexec_load_elf`）
  4. 栈：`main.c:388-398`（`minix_stack_params` / `minix_stack_fill`），`:400-402`（`handle_memory_once`），`:404-406`（`sys_datacopy`）
  5. 交付：`main.c:408-412`（`sys_exec`），`:415-416`（`sys_vmctl(VMCTL_BOOTINHIBIT_CLEAR)`）
- 文档 §2.3.2（`01-vm-init-main.md:440-454`）的步骤划分、回调名与行号与源码一致。

## CHECK 4 FAIL

异步回复的语义与文档 §2.4.3 一致，但函数行号锚点少算 2 行。

- 语义证据：`minix3/minix/lib/libsys/sef_init.c:471` 函数定义，`:477` `asynsend3(RS_PROC_NR, m_ptr, AMF_NOREPLY)`，`:480` `sef_setcb_init_response(SEF_CB_INIT_RESPONSE_DEFAULT)`；一次性异步后改回默认阻塞回复的行为与文档 517-533 行描述一致。
- 不一致点：文档标题与代码块标签写 `sef_init.c:471-481`，但函数体到 483 才结束，且文档引用的 `return r;` / `}` 就在 482-483。

### FINDING FACT-02（P2）

- 位置：`rewrite-notes/02-stage-vm/01-vm-init-main.md:513`（§2.4.3 标题）、`:518`（代码块行尾注释）
- 问题：`sef_cb_init_response_rs_asyn_once` 的范围写成 `471-481`，实际函数为 `471-483`；文档代码块引用的 `return r;` 与闭括号超出所标范围。
- 证据：`sef_init.c:480` 是 `sef_setcb_init_response(...)`，`:481` 空行，`:482` `return r;`，`:483` `}`。
- 最小修复：两处改为 `sef_init.c:471-483`。
- 置信度：高（逐行 `sed -n '478,484p'` 取证）。

## CHECK 5 PASS

Rust `BootParams` 字段与文档 §3.1 的清单一致，字段名、类型、顺序、注释语义全部对应。

- 证据：`os/servers/vm/src/boot.rs:61-130` 声明 11 个字段：`root_paddr: PhysBytes`、`kerninfo_pa: PhysBytes`、`total_pages: usize`、`free_regions: &'a [BootMemRegion]`、`boot_procs: &'a [BootImage]`、`modules: &'a [BootModule]`、`kernel_allocated: KernelAllocated`、`vm_allocated_bytes: u64`、`is_first_time: bool`、`kernel_layout: Option<minix_types::KernelLayout>`、`user_sp: VirBytes`。
- 文档 §3.1（`01-vm-init-main.md:647-660`）逐字段列出同一组 11 项，顺序一致；`kernel_layout` 的限定路径差异不算语义差异。
- `validate()`（`boot.rs:187-253`）确实复刻 `main.c:451-452` 的两个 assert 并补充交接页约束，与文档 668 行一致。

## CHECK 6 PASS

`real_main` 与 `BootRoute` 的路由结构与文档 §4.2 一致。

- 证据：`os/servers/vm/src/main.rs:41` `real_main`，`:61` `read_boot_params()`，`:81` `match boot_route(params.is_first_time)`；ColdInit 分支 `:82-118` 为 `new_with_boot_params` → `install_server`（失败 panic）→ `with_installed_server(|server| server.init())`；`:120-130` WarmRun / WarmNoState / DoubleInit 四个臂与文档 §3.2 真值表一致；`:135` `run_installed()`。
- 证据：`os/servers/vm/src/vm_server.rs:227-240` 四个枚举变体，`:245-253` `boot_route` 的 `(is_first_time, installed)` 四组合映射与文档 §3.2 表逐行一致；`:298-300` `run_installed` 从 image slot 取 server 后进 `run()`。
- 备注（非断言冲突）：§4.2 的代码块是节选，省略了 `main.rs:57-117` 的 `e5d:` 诊断写入与 `e5d_self_pt_test()` 调用；路由结构与文档断言的行为不受影响。

## CHECK 7 PASS

`VmServer::init()` 的阶段顺序与文档 §4.4.2 一致。

- 证据：`os/servers/vm/src/vm_server.rs:649` 函数起点；`:659` `register_page_alloc`；`:679` `relocate()`（非 test）；`:684` `init_global_state()`（Phase 1）；`:688` `init_vm_slot()`（Phase 2a）；`:692` `account_boot_memory()`（Phase 2b）；`:702-704` `PageFrames::new`；`:717` `init_boot_procs()`（Phase 2c）；`:721` `mark_vm_instance()`（Phase 2d）；`:728` `mark_initialized()`；`:730` `initialized = true`。
- 文档 §4.4.2（`01-vm-init-main.md:951-981`）的阶段序列与上述一致，`PageFrames` 在 boot 进程循环之前的说明也与源码注释（`vm_server.rs:695-704`）一致；`pt_init` 对调为 `init_vm_self_pt` → `init_vm_slot` 的差异已在 986 行显式说明。
- 备注（非断言冲突）：文档代码块省略了 `vm_server.rs:709-711` 的 `register_reclaim` 与各处 `e5d_diag` 调用，不影响阶段顺序断言。

## CHECK 8 PASS

`extra_pages()` 与文档 §3.6 一致。

- 证据：`os/servers/vm/src/boot.rs:263-288`：先对 `self.modules` 全部条目 `round_up_page(m.len) / CLICK_SIZE` 计费（271-274），再补 `vm_allocated_bytes`（280），最后加 `static` 向上取整 + `dynamic` 的内核占用（284-285）。
- 文档 §3.6（`01-vm-init-main.md:790-802`）给出同样三段与同一理由（handoff 模块表已排除 VM 模块、无内核伪条目）；与 §2.2.5 对 C 两笔校准的解释一致。

## CHECK 9 FAIL

测试名与 549/0 基线本身成立，但 §5.1 的测试覆盖表有一处描述与测试实现矛盾、并漏列一个直接相关测试。

- 测试名：§5.1 的 13 个 boot.rs 测试全部存在于 `boot.rs:492-678`；§5.2 的 8 个 vm_server.rs 测试全部存在于 `vm_server.rs:2715-4426`。无虚构测试名。
- 基线：实测 `cd os && cargo test -p minix-vm --lib` → `test result: ok. 549 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s`，与 §5.3（`01-vm-init-main.md:1084-1089`）的 549/0/0 完全一致。
- 不一致点见下两条 FINDING。

### FINDING FACT-03（P1）

- 位置：`rewrite-notes/02-stage-vm/01-vm-init-main.md:1056`（§5.1 表 `test_boot_params_extra_pages_modules` 行）
- 问题：覆盖描述写成“C 模块循环 + 末条目排除”，但该测试断言的是三个模块全部计费（1+2+1=4 页），并没有排除末条目；这与 §3.6 的“Rust 对列表全部条目计费”（`01-vm-init-main.md:793-794`）直接矛盾，属于测试描述与代码双向对账失败。
- 证据：`os/servers/vm/src/boot.rs:536-559`，三个模块期望 `extra_pages() == 4`；`boot.rs:537-539` 注释明确写 “The handoff list has no kernel pseudo-entry and omits VM's module, so all three are real modules”；实现 `boot.rs:271-274` 对 `self.modules` 全量累加，无 `saturating_sub(1)`。
- 最小修复：该格改为“全部模块计费（handoff 已排除 VM 模块与内核伪条目）；回归保护：不得再扣末条目”。
- 置信度：高（测试体与实现体均已逐行核对）。

### FINDING FACT-04（P2）

- 位置：`rewrite-notes/02-stage-vm/01-vm-init-main.md:1056-1057`（§5.1 表缺少一行）
- 问题：§5.1 标题声明覆盖 §3.6，但漏列 `test_boot_params_extra_pages_vm_image`；该测试正是 §3.6 “VM 映像经 `vm_allocated_bytes` 单独补记”这一分支的唯一宿主测试。
- 证据：`os/servers/vm/src/boot.rs:561-579` 存在该测试，断言 `vm_allocated_bytes = PAGE+1` 时 `extra_pages() == 2`；`boot.rs:276-280` 与文档 §3.6（`01-vm-init-main.md:798-799`）都声称该补记路径。
- 最小修复：在 `extra_pages_modules` 与 `extra_pages_kernel` 之间加一行 `| test_boot_params_extra_pages_vm_image | VM 映像经 vm_allocated_bytes 补记（C 模块表已省略 VM 模块） |`。
- 置信度：高。

## SUMMARY

- PASS：6 项（CHECK 1、3、5、6、7、8）
- FAIL：3 项（CHECK 2、4、9）
- FINDING：4 条
  - P1：1 条（FACT-03，测试覆盖描述与实现矛盾）
  - P2：3 条（FACT-01 CALLMAP 锚点 522-573→522-575；FACT-02 sef_init 锚点 471-481→471-483；FACT-04 §5.1 漏列 vm_image 测试）
- 基线复核：`cargo test -p minix-vm --lib` 实测 549 passed / 0 failed / 0 ignored，与 §5.3 一致。
