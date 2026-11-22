# READER-C R1
DOC 01-vm-init-main.md

审查范围：仅本篇文档；未读取源码、其他文档、diff、历史版本或面板报告。以下“文档”均指该文件，引文均为文档内原文。

---

## FINDING READER-C-01（P0）

- 位置：§4.0 失败对照 vs §4.4.1 构造步骤
- 严重度：P0（事实自相矛盾）
- 问题：同一个 `register_page_alloc`，§4.0 说“在构造期执行”，§4.4.1 说“不在构造期”，两处对危险窗口的定位相反。
- 影响：§4.0 用“构造后移动”解释为什么必须先装入 image slot；若注册其实发生在 `init()` 开头，构造后移动时根本没有已注册指针，所谓悬空路径不存在，image slot 的必要性论证被抽掉。读者也无法判断 `relocate()` 到底在修哪条路径。
- 证据或推理：§4.0：“`register_page_alloc` 在构造期执行（§4.4.1），因此真正的危险窗口是‘构造后移动’”。§4.4.1：“`global::register_page_alloc` 不在构造期：它在 `init()` 开头、server 已经进入 image slot 之后执行，因此注册的是最终地址。”§4.4.2 的代码同样把 `crate::global::register_page_alloc(&mut self.ctx.page_alloc);` 放在 `init()` 第一行，支持 §4.4.1。
- 最小修复：把 §4.0 改为“`register_page_alloc` 不在构造期，而在 `init()` 开头”；并把危险窗口改成“server 在 `install_server` 之后再次被移动，或 `init()` 之后全局指针仍指向旧地址”。若真实设计确为构造期注册，则反向修 §4.4.1/§4.4.2。
- 置信度：高。
- 是否需源码核实：否。
- 若我错了：若 §4.0 的“构造期”是笔误且 §4.4.1 是唯一口径，则事实以 §4.4.1 为准；但文档仍必须改正，因为两处不可能同时为真。

---

## FINDING READER-C-02（P1）

- 位置：§3.2 四态表、§4.4.1、§4.4.2、§5.2
- 严重度：P1（因果链缺步，热路径不可执行）
- 问题：`WarmRun` 被定义为“跳过 `init()`，直接从 slot 进入主循环”，但文档把三个进入主循环所需的前置动作都放在 `init()` 内：`register_page_alloc`、`mark_initialized`、`initialized = true`。`WarmRun` 全部跳过，因而没有可用的全局分配器指针、没有打开 IPC 就绪门、也没有已初始化标志。
- 影响：按文档自身的规则，`WarmRun` 要么在主循环无法收消息，要么被 `test_vm_server_run_without_init` 所描述的“未初始化进入主循环的 fail-fast”直接拦下；两种结果都与“直接从 slot 进入主循环”矛盾。读者无法判断热启动路径到底是可用路径还是 panic 路径。
- 证据或推理：§3.2 表：“`false` | 有 server | `WarmRun` | 跳过 `init()`，直接从 slot 进入主循环”。§4.4.2：“`// C: __minix_init() — 打开 IPC 就绪门，主循环才能阻塞收消息。 self.transport.borrow_mut().mark_initialized();`”，且 §4.4.1 说 `register_page_alloc` “在 `init()` 开头”执行。§5.2：“`test_vm_server_run_without_init` | 未初始化进入主循环的 fail-fast 行为”。
- 最小修复：在四态表中明确 `WarmRun` 的 server 必须已经完成 `register_page_alloc`/`mark_initialized`/`initialized`，并说明这些状态由谁、在何时建立；否则把 `WarmRun` 改成“重新执行 `init()` 的恢复路径”或直接 panic。
- 置信度：高（基于文档内三处互相约束的陈述）。
- 是否需源码核实：否。
- 若我错了：若 image slot 中的 server 在 `install_server` 时已由某个未文档化的步骤完成上述三项，或 `run()` 不检查 `initialized` 且 transport 无需 `mark_initialized` 即可接收，则 `WarmRun` 可成立；但文档没有给出该机制，且与 §5.2 的 fail-fast 测试描述冲突。

---

## FINDING READER-C-03（P1）

- 位置：§1.0 失败对照、§2.1、§3.2 四态表、§4.5 术语
- 严重度：P1（同一机制三种状态模型并存）
- 问题：关于“热重启后 VM 还剩下什么状态”，文档给出三种不能同时成立的模型：§1.0 说热重启的 VM 仍有“已经建立好的进程表和页表”；§2.1 说重启后“直接走 SEF 的 LU/restart 状态恢复路径”；§3.2 说 `is_first_time=false` 且 slot 空就是“re-exec 后没有可恢复状态”，而 §4.5 把 slot 定义为进程内静态槽。
- 影响：`is_first_time` 门控的理由、`WarmNoState` 的 panic 语义、以及 `WarmRun` 是否可达，全都依赖“重启是不是 re-exec、静态状态是否保留”这个未定义前提。读者无法从文档判断热重启到底是恢复、重建还是拒绝。
- 证据或推理：§1.0：“如果跳过 `is_first_time()` 的门控，热重启的 VM 会再次执行 `init_vm()`，把已经建立好的进程表和页表当成空的重新初始化。”§3.2：“`false` | 空 | `WarmNoState` | panic：re-exec 后没有可恢复状态”。§4.5：“**image slot**：存放 `VmServer` 的地址稳定静态槽；server 移动后全局指针仍然有效。”若热重启是 re-exec，进程 BSS/静态槽必然为空，§1.0 所说的“已经建立好的进程表和页表”就不可能是 VM 自己的内存状态；若热重启不是 re-exec，§3.2 的“re-exec 后没有可恢复状态”又不适用。
- 最小修复：先定义重启模型：是 re-exec（静态状态全丢，只能靠 SEF 外部状态恢复）还是同进程重入（静态槽保留）。然后统一 §1.0/§2.1/§3.2/§4.5 的措辞，并说明 `WarmRun`/`WarmNoState` 各自对应的真实场景。
- 置信度：高。
- 是否需源码核实：否。
- 若我错了：若 Minix3 的 VM 热重启既保留部分地址空间又执行新的入口，且 `image slot` 不在普通 BSS 而在某种持久映射中，则三处可能同时成立；但文档没有给出这种机制，且 §4.5 的“静态槽”表述排除了它。

---

## FINDING READER-C-04（P2）

- 位置：§3.1 设计理由、§3.2 四态表、§4.2 入口代码
- 严重度：P2（显式化承诺与隐藏全局不一致）
- 问题：§3.1 把 `BootParams` 的核心价值定为“构造参数显式暴露启动依赖，签名即文档”；但 `BootRoute` 的四态判定除 `is_first_time` 外还依赖 `server_installed()` 读静态槽，而 §4.2 的调用是 `boot_route(params.is_first_time)`，签名只暴露一个输入，slot 状态是隐藏全局。
- 影响：读者按签名会以为路由只由 `is_first_time` 决定；测试也必须先操纵静态槽才能覆盖四态。§3.0 失败对照批评的“单元测试必须先伪造全局状态”在 `boot_route` 上重新出现。
- 证据或推理：§3.1：“构造参数显式暴露启动依赖，`VmServer::new_with_boot_params` 的签名即文档。”§3.2：“四态的真值表如下（`is_first_time` 来自 `BootParams`，slot 状态由 `server_installed()` 读静态槽）”。§4.2：“`match boot_route(params.is_first_time) {`”。
- 最小修复：把 slot 状态作为显式参数传入（如 `boot_route(is_first_time, server_installed())`），或在 §3.2/§4.2 明确说明 `boot_route` 读取静态槽并给出测试隔离办法。
- 置信度：高。
- 是否需源码核实：否。
- 若我错了：若 `boot_route` 的完整签名其实包含 slot 参数而 §4.2 只是节选，则问题降为示例代码不完整；文档仍应让签名与四态表一致。

---

## FINDING READER-C-05（P1）

- 位置：§2.4.1、§2.1 代码、§3.4
- 严重度：P1（单形态结论写成普遍结论）
- 问题：§2.4.1 说“由于 `__vm_init_fresh`，VM 跳过等待 RS_INIT 阶段——RS_INIT 会在主循环中到达”，但 §2.1 的代码显示 `__vm_init_fresh=1` 只在 `is_first_time()` 为真时设置，`sef_local_startup()` 在条件外调用。热重启时 `__vm_init_fresh` 在 `sef_startup()` 期间是 0，SEF 不会走“跳过等待”的 fresh 特殊路径，RS_INIT 不必然在主循环到达。
- 影响：§3.4 把 `do_sef_init_request` 的等价实现统一成“主循环优先级 2 直接调 `rs_handshake()`”。若重启路径的 RS_INIT 在 `sef_startup()` 内就被消费，Rust 的主循环处理器对重启并不等价。读者会误以为所有 RS_INIT 都经过同一条路径。
- 证据或推理：§2.4.1：“由于 `__vm_init_fresh`，VM 跳过‘等待 RS_INIT’阶段（sef.c 注释：‘VM handles fresh initialization by RS later’）——RS_INIT 会在主循环中到达并交给 `do_sef_init_request`。”§2.1 代码：“`if (is_first_time()) { init_vm(); __vm_init_fresh=1; } ... sef_local_startup(); __vm_init_fresh=0;`”。
- 最小修复：把 §2.4.1 的句子限定为“首次启动（`__vm_init_fresh=1`）”；补一段热重启路径：`__vm_init_fresh=0` 时 SEF 如何处理 RS_INIT、`rs_handshake` 是否参与。
- 置信度：高（文档内代码块足以证明条件性；SEF 的具体行为由 §2.4.1 的“跳过等待”反推）。
- 是否需源码核实：否。
- 若我错了：若 `sef_startup()` 在 `__vm_init_fresh=0` 时也总是跳过等待并把 RS_INIT 留给主循环，则 §2.4.1 普遍成立；但那样 C 只在 fresh 时设置异步回调的条件就失去解释，文档需说明该回调条件的真实作用。

---

## FINDING READER-C-06（P1）

- 位置：§4.4.2 “两处与 C 的完成时机不同，但阶段顺序不变”、§1.2 对应表
- 严重度：P1（顺序声明与实际排列相反）
- 问题：C 的顺序是 `init_proc(VM_PROC_NR)`（474）→ `pt_init()`（475）；Rust 的对应排列是 `init_vm_self_pt()` 在构造期、`init_vm_slot()` 在 Phase 2a，即 VM 自身页表先于 VM 槽。文档却称“阶段顺序不变”，只把差异描述为“完成时机不同”。
- 影响：若 `pt_init()` 依赖 `init_proc(VM_PROC_NR)` 建立的 `vm_boot`/endpoint/槽状态，这次交换就是语义顺序变化，不是时机变化；读者会据此低估构造期建页表的风险。
- 证据或推理：§4.4.2：“两处与 C 的完成时机不同，但阶段顺序不变：”。§1.2 表：C 单元格“`init_proc(VM_PROC_NR)` / `pt_init`（474-475）”，Rust 单元格“`init_vm_self_pt()`（构造期）+ `init_vm_slot()`（Phase 2a）”。两列的先后正好对调。
- 最小修复：把“阶段顺序不变”改为“A–E 大阶段顺序不变，但 C 的 `init_proc`→`pt_init` 在 Rust 中变成 `init_vm_self_pt`→`init_vm_slot`”，并说明后者不依赖 VM 槽的依据。
- 置信度：高（文本排列直接可见）；对语义影响的判断为中。
- 是否需源码核实：否。
- 若我错了：若“阶段顺序”只指 A–E 五个大阶段的先后，而非阶段内步骤，则措辞可辩护；但 §1.2 的单元格按执行先后列出两项，读者仍会把它当顺序证据，文档应消除歧义。

---

## FINDING READER-C-07（P1）

- 位置：§1.5 术语、§3.9、§1.2 表 D 行、§4.4.2
- 严重度：P1（同一边界两种时序说法）
- 问题：§1.5 说 Rust 用 `mark_initialized()` 表达 `__minix_init` 的“同一时刻”；§3.9 说 `mark_initialized` 在 `init()` 末尾；§4.4.2 更明确它在 Phase 2d 之后。C 的 `__minix_init` 在总页数校准和 boot 进程循环之前，Rust 的对应门在它们之后，二者不是同一时刻。§1.2 的 D 行还以“IPC 可用”命名该阶段，却把 `mark_initialized()`（末尾）与 2b/2c 并列。
- 影响：§1.1 的阶段 D 定义是“IPC 向量可用，此后进行总页数校准、boot 进程地址空间建立”。在 Rust 中 2b/2c 发生在就绪门打开之前，阶段 D 的前提不成立；读者无法判断 boot 期的 `sys_exec` 是否需要在 IPC 门打开后执行。
- 证据或推理：§1.5：“**`__minix_init`**：C libc 在 VM 确定内核映射后建立内核 IPC 向量的分界点；Rust 用 `mark_initialized()` 表达同一时刻。”§3.9：“`VmServer::init()` 保持 C 的阶段划分，但两处完成时机不同：`pt_init` 在构造期，`mark_initialized` 在末尾；原因见 §4.4.2。”§1.2 表：“D IPC 可用 | `__minix_init` / `mem_add_total_pages` / boot 进程（480-520） | `mark_initialized()`（末尾）+ `account_boot_memory()`（2b）+ `init_boot_procs()`（2c）”。
- 最小修复：把术语改成“同一职责的分界标记，不是同一时刻”；在 §1.1/§1.2 明确 Rust 的 D 不是“IPC 可用”前置阶段，而是职责映射；或者把 `mark_initialized()` 移到 2b/2c 之前以保持 C 的时序。
- 置信度：高。
- 是否需源码核实：否。
- 若我错了：若“同一时刻”指“同一个语义里程碑”而非 wall-clock 顺序，则术语可辩护；但 §1.1 的“此后进行”把它用作因果前置，时序含义是承重的。

---

## FINDING READER-C-08（P1）

- 位置：§1.4 前提 4 vs §2.2.1、§2.2.6、§3.1
- 严重度：P1（fail-fast 边界多列了一项不存在的检查）
- 问题：§1.4 把“boot 进程表为空”列为 `init_vm()` 直接 panic 的条件，并给出 `main.c:442-452`、`main.c:504` 作为证据；但 §2.2.1 引用的两个 assert 是 `mmap_size > 0` 与 `mods_with_kernel > 0`，§2.2.6 的循环只在 `proc_nr < 0` 时 `continue`，`main.c:504` 是 `assert(ip->start_addr)`。三处都没有“boot 进程表为空”检查。
- 影响：一个 boot 进程表全为负项（或全被跳过）的启动会静默产生零个 boot 进程，VM 仍进入主循环；这与文档宣称的 fail-fast 边界相反。读者会误以为该输入已在启动期被拒绝。
- 证据或推理：§1.4：“`sys_getkinfo` 失败、boot 进程表为空、boot 进程装载失败都直接 panic（`main.c:442-452`、`main.c:504`）”。§2.2.1：“两个 assert 是启动协议的完整性检查：内存映射非空、模块表非空（`mods_with_kernel` 含内核自身）”，代码为 `assert(kernel_boot_info.mmap_size > 0); assert(kernel_boot_info.mods_with_kernel > 0);`。§3.1 的 `validate()` 清单也没有“boot 进程表非空”。
- 最小修复：删掉“boot 进程表为空”这一项，或把它补成真实检查（C 的 assert 与 Rust `validate()` 都加）；如果确实存在未引用的检查，补上准确行号与代码。
- 置信度：高。
- 是否需源码核实：否。
- 若我错了：若 `NR_BOOT_PROCS` 表在更早的内核交接中被保证至少含 VM 自身，且该保证在别处 assert，则“不会为空”可成立；但文档引用的范围内没有这个检查，且 §3.1 的 `validate` 清单也未提。

---

## FINDING READER-C-09（P1）

- 位置：§3.3 “当前实现边界”、§3.5 表、§2.3.2 第 4–5 步
- 严重度：P1（实现边界自相矛盾）
- 问题：§3.3 说 `exec_bootproc()` 的 `sys_exec` “已经实现”，同时说 `minix_stack_*` 初始栈帧 ABI 未落地、`stack`/`ps_str` 以 0 上报；但 C 的 `sys_exec(endpoint, sp, progname, pc, psp)` 的 `sp`/`psp` 正是上一步 `minix_stack_params`/`minix_stack_fill` 的产物。跳过栈建立却称交付执行已实现，文档没有解释 0 栈如何可执行。
- 影响：读者会高估 `exec_bootproc` 的完成度。若内核 `dispatch_exec` 不接受 0 栈，boot 进程无法真正启动；若内核会把 0 解释为默认栈，文档应写出该契约，否则“已实现”无法判断。
- 证据或推理：§3.3：“`exec_bootproc()` 的 ELF 装载、`sys_exec` 与 blob 释放已经实现；`minix_stack_*` 的初始栈帧 ABI 尚未落地，因此 `stack`/`ps_str` 暂以 0 上报。”§2.3.2：“**栈建立**：`minix_stack_params` + `minix_stack_fill` 生成初始栈（main.c:388-398）……**交付执行**：`sys_exec(endpoint, sp, progname, pc, psp)`（main.c:408-412）”。
- 最小修复：把状态拆成“`sys_exec` 调用路径已实现”与“可执行的 boot 交付未实现（初始栈 ABI 缺失）”；补一句 0 值在 `KernelGateway::sys_exec`/内核 `dispatch_exec` 中的解释，或明确当前真机启动会失败。
- 置信度：高（文本层面的边界冲突）；对内核行为的推断为中。
- 是否需源码核实：否。
- 若我错了：若内核 `dispatch_exec` 对 `sp=0` 有明确的默认栈回退（例如使用 `BootParams.user_sp`），则 `sys_exec` 路径可执行；但文档没有写出这条契约，读者无法从文档得到该结论。

---

## FINDING READER-C-10（P2）

- 位置：§3.4 表、§3.2 四态表
- 严重度：P2（错误处理理由只在冷路径成立）
- 问题：§3.4 用“此时 `init_vm()` 已完成、没有半初始化状态需要回滚”来证明 RS_INIT 失败可以不 panic。但四态表允许 `WarmRun` 跳过 `init()`；在这条路径上 `init_vm()` 并未完成，理由不成立。另外 `rs_handshake()` 是“逐条 `acl_set`”，中途失败会留下部分 ACL 已写的状态，也不是“没有半初始化状态”。
- 影响：读者会把“RS_INIT 失败不 panic”当成全局安全策略；在热路径或部分 ACL 写入失败时，VM 可能带着不完整 ACL 继续运行。
- 证据或推理：§3.4：“C 在 main.c:151 panic；Rust 选择让 VM 存活并把错误交给 RS，因为此时 `init_vm()` 已完成、没有半初始化状态需要回滚”。§3.2：“`WarmRun` | 跳过 `init()`，直接从 slot 进入主循环”。§3.4 表：“`ipc_call_rs_init()` 取 rproctab → 逐条 `acl_set`（等价 `map_service`）”。
- 最小修复：把理由限定为 `ColdInit` 路径；补 `WarmRun` 下 RS_INIT 失败的策略，以及 `acl_set` 部分失败的幂等/回滚说明。
- 置信度：高（文本内条件冲突）；对实际 ACL 状态的影响为中。
- 是否需源码核实：否。
- 若我错了：若 `WarmRun` 不可达或 RS_INIT 只在冷路径发生，则该豁免只影响冷路径，理由成立；但文档把四态表写成通用模型，仍未说明。

---

## FINDING READER-C-11（P2）

- 位置：§1.1 阶段 B/C vs §1.2 对应表 B 行
- 严重度：P2（同一阶段模型两处归属不同）
- 问题：§1.1 把“建立物理内存分配器”放在阶段 C，阶段 B 只列“解析物理内存布局、清空进程表、初始化 ACL 与区域管理”；§1.2 的 B 行却把 `mem_init`（建立分配器）与 `get_mem_chunks`/`memset`/`acl_init`/`map_region_init` 一起归入“B 不需要堆”。
- 影响：`mem_init` 到底属于“不需要堆的纯结构阶段”还是“需要物理页的分配器阶段”，决定读者如何判断 `init_proc`/`pt_init` 的前置条件。两处不一致使 A–E 阶段模型不能作为可靠骨架。
- 证据或推理：§1.1：“**阶段 B（不需要堆）**：解析物理内存布局、清空进程表、初始化 ACL 与区域管理。”与“**阶段 C（需要物理页，用保留页池）**：建立物理内存分配器、VM 自身进程槽与页表。”§1.2 表：“B 不需要堆 | `get_mem_chunks` / `memset` / `acl_init` / `map_region_init` / `mem_init`（455-471）”。
- 最小修复：把 `mem_init` 统一放进 B（并说明它只建位图、不消耗页表页），或统一放进 C；同时修正另一处的列表。
- 置信度：高。
- 是否需源码核实：否。
- 若我错了：若 §1.1 的“阶段 C”只是强调“从这里开始首次需要物理页”，并不表示 `mem_init` 属于 C，则分组可辩护；但 §1.2 的显式列表仍应与此一致，避免两种读法。

---

## FINDING READER-C-12（P2）

- 位置：§3.1 字段注释与 `validate()` 描述
- 严重度：P2（同名口径的两种定义）
- 问题：`BootParams.total_pages` 的字段注释是“位图容量：最高物理页号 + 1”，`validate()` 的约束却是“`total_pages` 等于最高空闲页号 + 1”。最高物理页与最高空闲页不是同一个量：内核/模块占用的高页可能不在任何空闲区内。
- 影响：读者无法确定 `validate()` 到底保证哪一个不变式，也无法判断位图是否覆盖非空闲的高物理页。`PageFrames::new(total_phys)` 又依赖“后端总容量”，三处口径可能不一致。
- 证据或推理：§3.1 字段：“`pub total_pages: usize, // 位图容量：最高物理页号 + 1`”。§3.1 验证段：“`total_pages` 等于最高空闲页号 + 1”。§3.6 表又写“`BootParams.total_pages` | 位图容量 = 最高物理页号 + 1”。
- 最小修复：统一成一个公式；若实际约束是“最高空闲页号 + 1”，就把字段注释改为“最高可用页号 + 1”，并说明位图容量与物理页上界的关系。
- 置信度：高（文本定义不同）；实际实现取哪一个为中。
- 是否需源码核实：否。
- 若我错了：若内核交接页的空闲区已经覆盖到最高物理页（不存在高于最高空闲页的非空闲页），两个说法在数据上重合；但字段注释与 `validate` 仍应使用同一措辞，否则读者无法判断约束边界。

---

## FINDING READER-C-13（P2）

- 位置：§3.6 四口径表、§4.4.1 构造步骤、§4.4.2 `init_global_state`
- 严重度：P2（结论没有推导过程）
- 问题：§3.6 说 `boot_usable_pages` 由 `new_with_boot_params` 写入，但 §4.4.1 的四步构造清单没有计算 `boot_usable_pages` 的步骤；§4.4.2 只下结论“总页数用 boot 可用页，而不是位图容量”，没有给出 `boot_usable_pages` 与 `BootParams.total_pages`/`VmPageAllocator::total_pages()` 的换算关系。
- 影响：读者无法把 C 的 `total_pages = Σ chunks` 对应到 Rust 的两个总页数字段，也无法验证 `global::init` 的输入是否正确。四口径表因此只能背，不能推。
- 证据或推理：§3.6 表：“`boot_usable_pages` | Σ 空闲区页数（C 的 `total_pages`） | `new_with_boot_params` | `global::init`”。§4.4.1：“1. `params.validate()`……2. `create_default_allocator(total_pages, free_regions)` + `VmPageAllocator::new`……3. `init_vm_self_pt()`……4. 拷贝 `boot_procs` 到定长数组 + 预计算 `boot_extra_pages = params.extra_pages()`。”§4.4.2：“总页数用 boot 可用页，而不是位图容量。”
- 最小修复：在 §4.4.1 增加 `boot_usable_pages = Σ free_regions` 的计算步；在 §3.6 表补一列“由谁、用什么公式得到”，并说明为何 `global::init` 不能直接用位图容量。
- 置信度：高（文本缺步）；具体代码是否有该步为中。
- 是否需源码核实：否。
- 若我错了：若 `boot_usable_pages` 在 `BootParams::simple()` 或 `validate()` 内已经算好，则 §4.4.1 只是省略了展示；但 §3.6 明确说由 `new_with_boot_params` 写入，构造步骤列表应包含它。

---

## FINDING READER-C-14（P2）

- 位置：§3.5 末段 vs §4.4.2 `init_boot_procs`
- 严重度：P2（未声明的第三个过滤条件）
- 问题：§3.5 说 `init_boot_procs` “保留 C 的两个保护：跳过负 `proc_nr`（内核任务）、`assert(start_addr != 0)`”；§4.4.2 却写“对每个非 VM、非负、endpoint 非 NONE 的 boot 条目建槽”，多出 `endpoint != NONE` 这个过滤条件。§2.2.6 的 C 循环只跳过负 `proc_nr` 和 VM，没有 endpoint 判断。
- 影响：若存在 `endpoint == NONE` 的非负 boot 条目，Rust 会跳过它，而 C 会为其建槽并 `exec_bootproc`；这是一处未解释的语义差异。读者会以为 Rust 只保留了两个保护，实际有三个条件。
- 证据或推理：§3.5：“`init_boot_procs` 保留 C 的两个保护：跳过负 `proc_nr`（内核任务）、`assert(start_addr != 0)`。”§4.4.2：“`init_boot_procs()`——对每个非 VM、非负、endpoint 非 NONE 的 boot 条目建槽（等价 `init_proc`），并调用 `exec_bootproc()`；保留 C 的 `assert(start_addr != 0)`。”§2.2.6 代码：“`if(ip->proc_nr < 0) continue;`……`if(ip->proc_nr == VM_PROC_NR) continue;`”。
- 最小修复：在 §3.5 明确第三个条件及其理由（例如 `endpoint == NONE` 表示非可执行条目），并说明 C 是否有等价保证；否则删除该过滤以对齐 C。
- 置信度：高（文本差异直接可见）；该条件是否必要为中。
- 是否需源码核实：否。
- 若我错了：若 C 的 boot 进程表构造保证所有非负、非 VM 条目的 endpoint 都非 NONE，则第三个过滤是冗余保护，不影响行为；但文档仍未说明这个保证，读者无法判断。

---

## FINDING READER-C-15（P2）

- 位置：§2.1、§2.3.2 第 5 步
- 严重度：P2（门控的自清除机制缺失）
- 问题：§2.1 说 RS 在启动时持有 `RTS_BOOTINHIBIT`，VM 是第一个运行的服务器，“此时 RS 尚未被解除启动抑制”；但没有说明是谁解除、在何时解除。文档后文 §2.3.2 的 `exec_bootproc` 第 5 步对每个 boot 进程发 `VMCTL_BOOTINHIBIT_CLEAR`，这才是让后续启动的 `is_first_time()` 变为 false 的机制，两处没有接上。
- 影响：读者会把 `is_first_time` 看成外部给定条件，而不是 VM 自己在 boot 循环中清除 RS 抑制后产生的结果；也无法判断“VM 在清除 RS 抑制前崩溃”时下一次启动会走哪条路径。§2.1 的“若 VM 因崩溃/Live Update 重启，RS 已解除抑制”因此过于绝对。
- 证据或推理：§2.1：“RS 在系统启动时持有 `RTS_BOOTINHIBIT`；VM 是第一个运行的服务器，此时 RS 尚未被解除启动抑制 → `is_first_time()` 为真”。§2.3.2：“`sys_exec(endpoint, sp, progname, pc, psp)`（main.c:408-412）+ `sys_vmctl(VMCTL_BOOTINHIBIT_CLEAR)`（main.c:415-416）解除启动抑制。”§2.2.6 又说明 boot 循环覆盖 “PM/VFS/RS/DS/...”。
- 最小修复：在 §2.1 补一句“RS 的 `RTS_BOOTINHIBIT` 由 VM 在 `init_vm()` 的 boot 循环里通过 `VMCTL_BOOTINHIBIT_CLEAR` 清除，因此首次 `init_vm()` 成功后后续启动才看到 false”；并把“若崩溃”限定为“在 RS 抑制已清除之后崩溃”。
- 置信度：中高（文档内两处可拼出该链条，但未明写）。
- 是否需源码核实：否。
- 若我错了：若 `VMCTL_BOOTINHIBIT_CLEAR` 清的是 boot 进程自身而非 RS 的 RTS 标志，或 RS 的抑制由内核/RS 自己清除，则这条因果链不成立；但那样 §2.1 的“尚未被解除”就缺少主语，文档仍应补明。

---

## FINDING READER-C-16（P2）

- 位置：§3.2 末段 vs §2.4.3 `sef_cb_init_response_rs_asyn_once`
- 严重度：P2（把一次性的异步行为写成始终异步）
- 问题：§3.2 说 `__vm_init_fresh` 在 Rust 中不需要独立字段，因为 `rs_handshake()` 的出生应答“直接走单臂 `send`，没有‘下一次改回同步’的状态要切换”；但 §2.4.3 的 C 代码明确写着首次异步后要“Use a blocking reply call next time”。文档没有论证为什么 Rust 可以永久异步。
- 影响：若后续 RS_INIT（例如重启/LU 场景）在 C 中依赖阻塞回复的协议语义，Rust 的永久单臂发送会造成行为差异；读者会以为这个差异不存在。
- 证据或推理：§3.2：“`__vm_init_fresh`（异步 RS_INIT 回复门控）在 Rust 中不需要独立字段：`rs_handshake()` 的出生应答直接走单臂 `send`，没有‘下一次改回同步’的状态要切换。”§2.4.3 代码：“`/* Use a blocking reply call next time. */ sef_setcb_init_response(SEF_CB_INIT_RESPONSE_DEFAULT);`”。
- 最小修复：说明 `rs_handshake` 是否只处理每次进程生命周期的第一次 RS_INIT；若是，则“永久异步”等同于“一次异步”；若不是，补上后续 RS_INIT 的回复语义与不等价风险。
- 置信度：中（文本差异明确；是否造成实际差异取决于调用次数）。
- 是否需源码核实：否。
- 若我错了：若 Rust 的 `rs_handshake` 在每个 VM 实例中只被调用一次，后续 RS_INIT 走 LU/restart 的其他路径，则“没有下一次”成立；但 §3.4 把它写成主循环优先级 2 的统一 RS_INIT 处理，文档需收窄该表述。

---

## FINDING READER-C-17（P2）

- 位置：§5.0、§5.3
- 严重度：P2（基线结论无文档内证据，且自违“不引历史”规则）
- 问题：§5.0 要求“测试计数必须在当前 HEAD 上重跑，不能引用历史数字”，§5.3 却写“此前记录的 `alloc_page` 与 `test_map_lazy` 失败已经不再出现”，并直接断言 `548 passed / 0 failed`。文档没有给出命令输出、提交号、日期、feature/目标形态或运行环境。
- 影响：读者无法复现或判断 548/0 是否对应当前版本；把宿主 `--lib` 的单一形态结果写成“当前基线没有失败项”，容易被误读为启动链在真实形态下也已验证（§5.4 才补充限制）。
- 证据或推理：§5.0：“测试计数必须在当前 HEAD 上重跑，不能引用历史数字。”§5.3：“在当前 HEAD 上执行 `cargo test -p minix-vm --lib`，读数是 **548 passed / 0 failed**。此前记录的 `alloc_page` 与 `test_map_lazy` 失败已经不再出现”。
- 最小修复：附上实际命令与输出摘要（含提交号/日期/形态）；删掉历史失败叙述，或把它移到独立的变更记录而不作为“当前基线”的论据；在 §5.3 标题或首句限定“宿主 `--lib` 形态”。
- 置信度：高（文本自相矛盾）；548 数值是否准确为中。
- 是否需源码核实：否。
- 若我错了：若 548/0 的命令输出在文档之外有归档，则证据存在但未随文档自包含；对“正式文档自包含”的要求而言仍是缺口。

---

## FINDING READER-C-18（P2）

- 位置：§5.2 测试表 vs §4.4.2 新增顺序约束
- 严重度：P2（测试映射未覆盖文档新增的关键顺序）
- 问题：§4.4.2 新增了两条启动顺序约束：页帧表必须在 `init_boot_procs()` 之前建立，`mark_initialized()` 必须在最后；但 §5.2 的测试表没有点名任何测试覆盖这两条顺序，`test_vm_server_init` 只写“`init()` 的基本路径与 `initialized` 标志”。
- 影响：读者会以为 §5.2 的测试表证明了 §4.4.2 的全部阶段顺序；实际上最容易出错的两条顺序（页帧表前置、IPC 门后置）没有可见的测试证据。
- 证据或推理：§5.2 表：“`test_vm_server_init` | `init()` 的基本路径与 `initialized` 标志”。§4.4.2：“`// 页帧表在 boot 进程循环之前建立：exec_bootproc 需要从它取段页。`”与“`// C: __minix_init() — 打开 IPC 就绪门，主循环才能阻塞收消息。`”。
- 最小修复：在 §5.2 增加两行，分别点名页帧表顺序与 `mark_initialized` 的测试（或明确写“未覆盖，属真机门/后续测试”）。
- 置信度：高（表内无名）；是否被 `test_vm_server_init` 间接覆盖为中。
- 是否需源码核实：否。
- 若我错了：若 `test_vm_server_init` 的断言实际包含页帧表与 transport 就绪门检查，则覆盖存在但未在表中点名；测试表仍应显式映射，否则读者无法核对。

---

## SUMMARY

计数：P0 = 1；P1 = 7；P2 = 10；P3 = 0；合计 18 条。

最严重 3 条：

1. **READER-C-01（P0）**：`register_page_alloc` 在构造期还是 `init()` 开头，§4.0 与 §4.4.1 直接对立，导致 image slot 的安全论证前提不成立。
2. **READER-C-02（P1）**：`WarmRun` 跳过 `init()`，却同时跳过 `register_page_alloc`、`mark_initialized`、`initialized` 三个进入主循环的前置动作，热路径按文档自身规则不可执行，并与“未初始化 run 即 fail-fast”的测试描述冲突。
3. **READER-C-08（P1）**：§1.4 宣称“boot 进程表为空”会 panic，但 §2.2.1/§2.2.6/§3.1 引用的 assert 与 validate 都没有这个检查；空表会静默产生零个 boot 进程，是真实的 fail-fast 缺口。

最影响因果理解的 3 处：

1. **READER-C-03**：热重启状态模型三说并存（§1.0 的“已建立表”、§2.1 的 SEF 恢复、§3.2 的“re-exec 无可恢复状态”），使 `is_first_time`、`WarmRun`、`WarmNoState` 的因果基础不清。
2. **READER-C-05**：RS_INIT 只在 `__vm_init_fresh=1` 的 fresh 路径被 SEF 留给主循环，文档却把“RS_INIT 会在主循环中到达”写成普遍结论，进而把 `rs_handshake` 写成所有 RS_INIT 的统一处理。
3. **READER-C-07**：`__minix_init` 的“同一时刻”与 `mark_initialized` 的“末尾”冲突，且 §1.2 的 D 阶段以“IPC 可用”为前提却把 2b/2c 排在其前，阶段 D 的因果前置在 Rust 中不成立。
