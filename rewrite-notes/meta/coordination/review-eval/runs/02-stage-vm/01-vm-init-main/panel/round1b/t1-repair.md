DOC 01-vm-init-main.md

- 审查范围：父章 1、2、4（单元 ID `1`、`2`、`4`），只读复核；未参考旧面板报告。
- 审查依据：当前正文、`minix3/minix/servers/vm/main.c`、`acl.c`、`region.c`、`alloc.c`、`utility.c`、`lib/libsys/sef*.c`、`os/servers/vm/src/{main.rs,boot.rs,vm_server.rs}`、`rewrite-notes/02-stage-vm/25-rs-services.md`、`plan.md` 与 todo 账本。
- VERDICT 判据：章首问题入口成立、章内承诺有落点、C 源证据一致为 PASS；出现破坏章级承诺的事实错误或跨文档责任落空为 FAIL。P2/P3 不自动等于 FAIL，但逐条列出。

## UNIT 1

VERDICT: PASS

FINDINGS: 0

复核要点：

- TR-0 成立。§1.0 有问题入口、最小示例与失败对照；示例命令的实际输出为 `79:static int is_first_time`、`93:int main(void)`、`428:static void init_vm`，与正文一致；失败对照引用的 `main.c:100-107` 与 C 源一致。
- 章内承诺有落点。§1.1 给出 A–E 五阶段，§1.2 给出启动主线图，§1.4 用四条执行前提回答“为什么按这个顺序”；§1.5 小结与术语与正文一致。
- 依赖链图与 §2.2.8、§3.3 表一致。C 的 577–586 实际为：577 注释、578 `num_vm_instances`、579 `VMF_VM_INSTANCE`、580 空行、581 注释、582–583 `sef_llvm_add_special_mem_region` 调用、584–586 失败打印分支；函数右花括号在 587。§1.2 的 `577-586（含 SEF 特殊映射区）` 与 §2.2.8 的 `577-586`、§3.3 的 `577-579` + `582-583` 三者可以逐行对上，无范围冲突。

## UNIT 2

VERDICT: FAIL

FINDINGS: 4

摘要：TR-0 的结构完整，新增的 `acl_init`/`map_region_init` 顺序理由与 C 源一致；但 §2.2.2 仍有一句错误的顺序理由（`get_mem_chunks` 必须先于 `memset`），§2.0 失败对照的锚点 `main.c:354` 指错，最小示例输出顺序不可重放，且承诺中的“为什么 boot 进程要等 `init_vm()` 尾部”只有材料没有显式因果句。

## FINDING T1-01

- 位置：§2.0 失败对照（文档第 172 行）
- 严重度：P2
- 问题：正文把 `init_proc()` 中 `assert(!(vmp->vm_flags & VMF_INUSE))` 的证据锚点写成 `main.c:354`。实际 `main.c:354` 是 `exec_bootproc()` 中 `pt_new` 失败分支之后的空行；该 assert 在 `init_proc()` 内，位于 `main.c:276`。
- 证据：
  - `grep -n 'assert(!(vmp->vm_flags & VMF_INUSE))' minix3/minix/servers/vm/main.c` 输出 `276:		assert(!(vmp->vm_flags & VMF_INUSE));	/* no double procs */`。
  - `awk 'NR>=350 && NR<=356' minix3/minix/servers/vm/main.c` 输出 352 `pt_new`、354 空行、355 `pt_bind`，与 `init_proc` 的 assert 无关。
- 最小修复：把该句末尾的 `main.c:354` 改为 `main.c:276`；`main.c:458-462` 的 `memset` 引用保持不变。
- 置信度：高

## FINDING T1-02

- 位置：§2.2.2（文档第 256 行）
- 严重度：P2
- 问题：正文称 `get_mem_chunks()`“必须先于 `memset` 与 `mem_init`，因为后面两张表都消费这组块”。`memset(vmproc, 0, ...)` 不读 `mem_chunks`，`acl_init()` 与 `map_region_init()` 也不读；`mem_chunks` 在 `init_vm()` 里只有两个消费者：`get_mem_chunks(mem_chunks)` 自己，以及 `mem_init(mem_chunks)`。因此“必须先于 `memset`”没有依赖依据，“两张表”也没有对应物。
- 证据：
  - `grep -n 'mem_chunks' minix3/minix/servers/vm/main.c` 除第 431 行的声明外，只有 `455:	get_mem_chunks(mem_chunks);` 与 `471:	mem_init(mem_chunks);` 两个使用点；`memset(vmproc)` 与 ACL/区域初始化都不引用它。
  - `mem_init()`（`alloc.c:306-335`）用 chunks 建 `free_pages_bitmap` 并累加 `total_pages`；`memset(vmproc)` 与 ACL/区域初始化都不消费 chunks。
- 最小修复：改为“它必须先于 `mem_init`，因为只有 `mem_init` 消费这组块；`memset(vmproc)` 与它没有数据依赖”。不要保留“两张表”的因果句。
- 置信度：高

## FINDING T1-03

- 位置：§2.0 最小示例（文档第 164-167 行）
- 严重度：P3
- 问题：命令的输出顺序与真实 `grep` 输出不一致。文档按“先 `init_vm`、再 `init_proc`、再 `exec_bootproc`”排列为 428、262、331；`grep` 按文件行号输出，实际顺序是 262、331、428。
- 证据：执行文档中的命令，实际输出为
  ```text
  262:static struct vmproc *init_proc(endpoint_t ep_nr)
  331:static void exec_bootproc(struct vmproc *vmp, struct boot_image *ip)
  428:static void init_vm(void)
  ```
- 最小修复：按实际输出顺序重排；若确要按讲解顺序展示，需在代码块外注明“按本章讲解顺序重排，非原样输出”。
- 置信度：高

## FINDING T1-04

- 位置：§2.0 承诺（文档第 157 行）与 §2.2.6（文档第 317-343 行）
- 严重度：P3
- 问题：§2.0 承诺回答四个“为什么”，其中“为什么 boot 进程要等 `init_vm()` 尾部才被 `exec`”在章内只有“做了什么”的描述：§2.2.6 说明循环先建槽、再 `exec_bootproc`、最后归还 blob；§2.3.2 说明 `exec_bootproc` 的五个子步骤。章内没有一句显式因果句说明这个循环为什么必须排在 `__minix_init` 与 `mem_add_total_pages` 之后、`CALLMAP` 之前。读者需要自己从 `pt_new`、`map_page_region`、`VMCTL_BOOTINHIBIT_CLEAR` 反推。§2.2.7 的“`CALLMAP` 必须在 boot 进程循环之后”只解释了 `CALLMAP` 的位置，不解释 boot 循环本身的位置。
- 证据：正文 §2.2.6 只有流程描述，§2.3.2 只列 `exec_bootproc` 的五个子步骤；这两处都没有解释“为什么循环必须在这个位置”的因果句。§2.2.7 有一句“`CALLMAP` 必须在 boot 进程循环之后，因为……”，但它只解释 `CALLMAP` 的位置，不解释 boot 循环本身为什么在 `__minix_init` 与 `mem_add_total_pages` 之后。C 的 `exec_bootproc()`（`main.c:331-417`）确实依赖 `pt_init` 的保留页池与 `mem_init` 的位图，且最后一步才 `VMCTL_BOOTINHIBIT_CLEAR`。
- 最小修复：在 §2.2.6 末尾补 2–3 句：boot 进程的地址空间装配调用 `pt_new`/`map_page_region`，依赖 `pt_init` 的保留页池与 `mem_init` 的位图；每个 boot 进程由内核 `BOOTINHIBIT` 抑制，直到 `exec_bootproc()` 最后一步清除；`mem_add_total_pages` 是记账而非分配门，C 把它放在循环前是让总页数账先包含模块与内核占用。补完后四个承诺都有显式落点。
- 置信度：中（缺口是“显式因果缺失”，不是事实错误）

补充：新增的 `acl_init` 与 `map_region_init` 理由本身与 C 源一致。`acl_init()`（`acl.c:21-30`）确实遍历 `vmproc` 写 `vm_acl = NO_ACL` 并清零 `acl_mask`/`acl_inuse`，`memset` 会把 `vm_acl` 写成 0，所以它排在 `memset` 之后；`map_region_init()`（`region.c:36-38`）确为空函数，正文“顺序上没有依赖”成立。承诺中的其余三个问题分别有显式落点：`acl` 顺序在 §2.2.2，`pt_init` 先于 `__minix_init` 在 §2.2.4，`RS_INIT` 异步在 §2.4.3。

## UNIT 4

VERDICT: FAIL

FINDINGS: 2

摘要：Phase 2d 的 C 行号 `577-579` 与 `582-583` 准确，且与 §3.3 表逐行一致；但“SEF 特殊映射区登记……归 25”指向的 25 篇并不承接该条，形成跨文档责任落空；§4.0 最小示例的输出也不是当前命令的可重放输出。

## FINDING T1-05

- 位置：§4.4.2 Phase 2d 注释（文档第 974-975 行）；同一说法见 §3.3 表与 §6（第 729、1135 行）
- 严重度：P2
- 问题：注释写“SEF 特殊映射区登记（`main.c:582-583`）当前未实现，归 25”。`25-rs-services.md` 不承接这一条：它的“说明”明确写“不覆盖：SEF 生命周期与 `map_service` 启动注册的完整叙事（01）”；其 §4.8“A-8 缺口契约（SEF / Live Update）”表列出 6 个缺口（`map_proc_dyn_data`、UPDATE 切换、MAKE_VM、SET_PRIV 位图、RS_INIT 握手取表、MAP_PREALLOC 立即分配），没有 `sef_llvm_add_special_mem_region`；25 篇全文也没有 `%MMAP_ALL`/特殊映射区的任何表述。todo 账本同样没有该条。结果是“当前未实现”被指向一个不存在的落点。
- 证据：
  - `sed -n '1,20p' rewrite-notes/02-stage-vm/25-rs-services.md` 第 4 行含“不覆盖：SEF 生命周期与 `map_service` 启动注册的完整叙事（01）”。
  - `sed -n '582,600p' rewrite-notes/02-stage-vm/25-rs-services.md` 的 A-8 缺口表无该条。
  - `grep -n 'MMAP_ALL\|special_mem_region\|特殊映射区' rewrite-notes/02-stage-vm/25-rs-services.md` 无输出。
  - `grep -n 'MMAP_ALL\|special_mem_region\|特殊映射区' rewrite-notes/meta/coordination/todo-ledger/02-stage-vm.yaml` 无输出。
  - `plan.md:182` 的 25 行只列 `do_rs_*`、`rs_memctl_*`、`map_service`、`adjust_proc_refs`、`RprocTab`、A-8 缺口契约，未列 SEF 特殊映射区。
- 最小修复：二选一。(a) 在 25 篇 §4.8 A-8 缺口表补一行“SEF 特殊映射区登记（`main.c:582-583`）｜`sef_llvm_add_special_mem_region(VM_OWN_HEAPBASE, VM_OWN_MMAPTOP-VM_OWN_HEAPBASE, "%MMAP_ALL")`｜无对应调用｜Live Update 状态传输/VM 自更新”，并同步 25 篇的“说明”口径；(b) 若该条不归 25，则把 01 的“归 25”改为指向实际承接处，并在 todo 账本登记，避免“未实现 + 无账本”的悬空。不要只保留当前单向指针。
- 置信度：高

## FINDING T1-06

- 位置：§4.0 最小示例（文档第 872-877 行）
- 严重度：P2
- 问题：示例命令包含 `install_server` 与 `boot_route` 两个模式，并同时搜索 `main.rs` 与 `vm_server.rs`；文档只列 4 行，且 `vm_server.rs:1693` 不是 `pub fn run` 的行。当前 `pub fn run(&mut self) -> !` 在 `vm_server.rs:1699`；1693 是 `run` 的文档注释。实际输出还包含 `main.rs:47`、`main.rs:85`、`vm_server.rs:245`、`vm_server.rs:266`、`vm_server.rs:298` 以及测试模块的多处匹配，均被省略且未标“略”。
- 证据：执行文档中的命令，实际输出为
  ```text
  os/servers/vm/src/main.rs:41:fn real_main() -> ! {
  os/servers/vm/src/main.rs:47:        use minix_vm::{BootRoute, VmServer, boot_route, install_server, read_boot_params, run_installed, with_installed_server};
  os/servers/vm/src/main.rs:81:        match boot_route(params.is_first_time) {
  os/servers/vm/src/main.rs:85:                if install_server(server).is_err() {
  os/servers/vm/src/vm_server.rs:245:pub fn boot_route(is_first_time: bool) -> BootRoute {
  os/servers/vm/src/vm_server.rs:266:pub fn install_server(server: VmServer) -> Result<(), VmServer> {
  os/servers/vm/src/vm_server.rs:285:/// caller routes through [`boot_route`] first, so an empty slot here is a
  os/servers/vm/src/vm_server.rs:298:pub fn run_installed() -> ! {
  os/servers/vm/src/vm_server.rs:649:    pub fn init(&mut self) {
  os/servers/vm/src/vm_server.rs:1699:    pub fn run(&mut self) -> ! {
  ```
  测试模块还有 `vm_server.rs:2888/2889/2893/2897/2900/2909` 的匹配。
- 最小修复：按实际输出完整粘贴并标“测试模块匹配略”，或把命令收窄到本章要展示的锚点（例如只匹配 `fn real_main`、`match boot_route`、`pub fn install_server`、`pub fn init`、`pub fn run(&mut self)` 的定义行）。无论选哪种，把 `1693` 改为 `1699`。
- 置信度：高

补充：Phase 2d 的 C 行号本身正确。`main.c:577-579` 是 `num_vm_instances`、`VMF_VM_INSTANCE` 标记与注释；`main.c:582-583` 是 `sef_llvm_add_special_mem_region` 调用；`main.c:584-586` 是失败打印分支。§3.3 表的 `577-579` 与 `582-583` 两行与之一致，问题只在“归 25”的落点。

HARD-REJECTIONS: 0
