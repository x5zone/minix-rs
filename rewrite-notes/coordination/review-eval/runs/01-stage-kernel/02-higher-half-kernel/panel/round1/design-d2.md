# 02 · 高半区 — 独立设计审查（P-DESIGN-D2，Round 1）

## 基线核对

- 目标文件：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 行数：1275（`wc -l` 实测）
- sha256：`2bb4a585597ffbd27cf4495292e2e85f026e1782b08fe5268426f374357e5485`，前缀 `2bb4a585597ffbd2` 与任务基线一致。
- 审查范围：第 3 章（lines 326–645）与第 4 章（lines 647–1088）。
- 只读材料：`tools/doc-code-map.sh … --check` 的 27 个文件、`tmp/review-eval/01-stage-kernel/02-higher-half-kernel/trait-audit.md`、`AGENTS.md` 的 Key Constraints、`round1-decisions-snippet.md`。
- 未读：本 run 顶部 V1 段、meta、历史评审、其他面板输出、expected/。
- 越界披露：为核实 `KernelInfo.syscall_entry` 是否还有读者，除 27 个文件外，用 `grep -rn "\.syscall_entry" os/` 扫了全仓并只读取了命中行的上下文（`os/arch/src/lib.rs:365-386`、`os/kernel/src/lib.rs:2266-2270`、`os/qemu-tests/test-kernels/.../test-protection-riscv64/src/main.rs:148`）。该条 FINDING 的证据已标明这一越界，供裁决者打折。

## trait-audit 读数与本篇相关的部分

| trait | 位置 | 实现 | 引用 | dyn | 判定 |
|---|---|---:|---:|---:|---|
| `HigherHalf` | `os/kernel/src/boot/higher_half.rs:30` | 4 | 31 | 0 | MULTI-IMPL |
| `FileLoader` | `os/boot-shim/src/loader.rs:106` | 3 | 15 | 1 | MULTI-IMPL |
| `StacktraceArch` | `os/arch/src/arch/stacktrace.rs:41` | 3 | 16 | 0 | MULTI-IMPL |
| `BootArch` | `os/arch/src/arch/boot_arch.rs:20` | 4 | 55 | 0 | MULTI-IMPL |

`HigherHalf` 的 4 个实现是 3 个生产实现加 1 个测试 mock（`os/kernel/src/lib.rs:6075-6090`），不是 4 个架构实现。文档 §3.4/§4.3 的“三份实现”表述与生产代码一致；trait-audit 的 4 是源码计数，不是语义分歧。

---

## DIFF-TABLE

| 维度 | 文档陈述 | 现有实现 | 差异 | 判断 |
|---|---|---|---|---|
| 状态表达 / 栈顶 | §3.3：栈顶由装载方算好、按值交接，字段 `kern_stack_top` 是装载方的产出，内核只消费 | `build_kernel_info` 由 `kern_virt_base + kern_size` 导出（`os/boot-shim/src/lib.rs:126`）；严格门反过来断言它必须等于该和（`os/libs/minix-boot/src/kernel_info.rs:462-470`）；v1 线协议再存一份（`handoff_v1_read.rs:182`） | 文档把派生态当输入；实现把它当缓存，另加反向断言兜底 | FINDING D2-02 |
| 状态表达 / 校验严格度 | §3.1、§4.4：启动路径有“生产门” | `ValidationLevel` 三档（`kernel_info.rs:191-202`），由调用者选：`validate` / `validate_before_tables` / `validate_requiring_sources`；direct face 先 strict 再在 `arch_boot_finish` 重跑 lenient（`os/kernel/src/lib.rs:222`） | 同一对象可被不同严格度校验；生产腿可自选宽松门 | FINDING D2-15 |
| 状态表达 / 跨距来源 | §3.3、§3.5：跨距从 ELF 现算；自引导面是手写常量，riscv64 由 L9 对账，aarch64 只有上界断言 | `bootface.rs:121` 与 `bootface_a64.rs:83` 各自硬编码 `KERN_SIZE=0x40_0000`；aarch64 无等值检查，且 `KERN_SIZE` 同时决定映射上界与 `kern_stack_top` | 文档 §3.5 只点名 riscv64 的常量；aarch64 的同一风险被一句括号带过 | ARCH-PROPOSAL A2 |
| 状态表达 / 死字段 | 文档第 3、4 章未提 `KernelInfo.syscall_entry` | 生产消费者为零：LSTAR 走 `minix_arch::syscall_entry_va()`（`os/kernel/src/lib.rs:2266-2270` 自述该字段是 reference-only）；唯一读者是 riscv64 保护测试载体；仍在线协议、3 个构造点、严格门等值断言里 | 死状态占据跨镜像协议位 | FINDING D2-03 |
| 身份 / 模块 | §4.4：12 个模块按位置交接口 | `BootModule.name` 存名字串，位置是隐式契约；只有严格门比对名字与位置（`kernel_info.rs:467-481`）；测试载体传空表 | 双身份通道，宽松腿不校验 | FINDING D2-12 |
| 身份 / 引导腿 | §3.1 登记表按四条腿分类 | 腿由编译期 `#[cfg]` 选出；`KernelInfo.authority` 只标识 RAM 输入来源，不标识腿 | 运行时对象里没有腿身份，腿在类型之外 | 接受（腿是编译期事实，无需运行时表达） |
| 所有权 / 载荷 | §4.4：`KernelInfo` 落进内核 `.bss` 副本 | 7 个 landing pad（`store_kernel_info`，`os/kernel/src/lib.rs:1453-1565`），其中 5 处走 `land_slice`、2 处手写（模块名池、param 清零）；字段类型是 `&'static [T]`，但源内存在 shim 堆上 | 生命周期类型与实际所有权不一致；同一复制动作 3 种写法 | ARCH-PROPOSAL A1 |
| 所有权 / 文件字节 | 文档未提 | `FileBytes::{Borrowed,Owned}`（`loader.rs:90-123`）把“谁拥有字节”写进类型 | 无差异；这是正面样例 | 接受 |
| 所有权 / 线协议 | §4.4：v1 arena 七分区 | 信封还带 3 个派生标量（`user_sp`/`kern_stack_top`/`syscall_entry`），接收侧靠严格门重新验证 | 派生值跨镜像传输 | FINDING D2-02、D2-03 |
| 错误 / 边界 | §3.2：错误分类用 9 变体枚举，并承认 `InvalidSegment` 不是单一原因 | `ElfError` 同时被 boot-shim 当装载/布局错误用（`loader.rs:175,218,303`）；`minix-elf` 注释引用一个全仓不存在的 `LoaderError`（`os/libs/minix-elf/src/lib.rs:175`） | 解析错误与装载器错误共用一个枚举，无动态上下文 | FINDING D2-06 |
| 错误 / 损坏输入 | §3.2 明确写了“调用方看到的是段比预期少，而不是段坏了” | `SegmentIter::next` 对越界段 `continue`（`minix-elf/src/lib.rs:383-389`）；`compute_kernel_layout` 只对收到的段取极值，不记录被跳过的段 | 截断的 ELF 会得到一个“看起来合法”的更小布局 | FINDING D2-05 |
| 错误 / 哨兵 | 文档未提 | `load_segments_into_buffer` 解析失败返回 0、计数包含被跳过的段（`loader.rs:224-282`）；`load_elf_into` 用 `entry_point(image).unwrap_or(0)`，随后又二次解析头部（`os/arch/src/arch/boot.rs:461-464`） | 0 同时表示“空镜像”“解析失败”“入口未知” | FINDING D2-09 |
| 并发 / 一次性 | §3.4 前置条件第四条：只从引导 CPU 调用一次；表里承认“类型系统不区分调用次数” | `unsafe fn jump_to_kmain` 无令牌；两个调用点都是启动单线程 | 文档已诚实记录，未发现被违反 | 接受（见 D2-07 的可选强化） |
| 并发 / 全局共享 | §4.4：跳转前落 `.bss` 副本，之后只读 | `KERNEL_INFO` 是 `UnsafeCell` + `Option`，`store_kernel_info` 写一次；boot-shim 侧另有 `UnsafeCell`+`Sync` 的一次性存储 | 无锁靠“引导单线程”约定 | 接受（约定明确，注释完整） |
| 抽象边界 / `HigherHalf` | §3.4：trait 收契约，三份实现 | 每目标只编译一个实现；`H` 作为泛型参数穿过 `arch_boot_finish`/`resume_additive`；`dyn` 使用为 0 | trait 是架构缝，不是测试注入；但方法签名带冗余参数 | FINDING D2-04（参数）、D2-08（mock） |
| 抽象边界 / ELF 解析 | §3.2：共享 crate，四种去向 | 解析共享；摆放策略三份且语义确实不同：内核引导 eager 映射（`boot.rs:399`）、VM bootstrap 自己建页表（`vm_server.rs:890-893`）、RS exec 按需 copy（`trap_api.rs:443-469`）；VFS 另有一份只读头解析器 `ElfHead` 且接受 `ET_DYN` | 共享停在解析层是对的；VFS 的第二解析器有独立能力（部分缓冲 + ET_DYN） | 接受（摆放不该共享；VFS 解析器差异有理由） |
| 抽象边界 / 链接脚本 | §3.1：`os/kernel/src/arch/*/link.ld` 无生产链接读者，只记录预期布局 | 文档给的重算命令实测无命中；唯一读者是 `link_ld_lockstep.rs` 的文本解析，断言常量相等；其余引用都是注释 | 三份脚本加一个测试只为“文档化”存在 | FINDING D2-11 |
| 可测试性 / mock | `os/kernel/src/boot/higher_half.rs:28-30` 宣称 mock 可验证签名与调用契约 | mock 只置一个 `AtomicBool`，没有任何测试读它；测试只做 `accept_higher_half::<MockHigherHalf>()`（`lib.rs:6092-6099`） | 可测试性论据没有支撑 | FINDING D2-08 |
| 可测试性 / 缓冲区孪生 | §4.2 承认两版有差异，缺口记 D02-F04 | 两版循环已分叉：物理版有两道重叠检查（段-段、源-目的），缓冲版有界检查但无重叠检查；缓冲版返回值把被跳过的段也计入 | 宿主测试不覆盖生产路径的两道 panic 分支 | FINDING D2-06（返回值）、D2-14（引文漏第四步） |
| 可测试性 / 生产状态 | §4.4 状态表用 `kmain_verify` 的三条断言做依据 | `kmain_verify` 只在 `feature="qemu_test"` 下存在（`lib.rs:2067-2121`）；生产 `kmain` 是普通函数，入口栈状态不可观测 | 证据来源只在测试形态可用 | 接受（文档已说明载体形态） |

---

## FINDING RD1-DESIGN-D2-01（P2）

- 位置：`/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:1671-1739`（`kmain` 头部）；文档 §4.4 `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1059`
- 严重度：P2（引导契约的签名与实际消费源不一致；不改变运行结果，但把错误前提写进了文档与类型）
- 分类：[DEL]
- 问题：生产 `kmain(kernel_info: &KernelInfo)` 的第一条语句把形参遮蔽成 `.bss` 全局副本，此后形参再无读者。实现注释给出的根因是“形参被编译器溢出到内核栈、从被覆写的栈槽读回”，并写着 `This shadow can be deleted once the diagnostics below confirm it`；文档 1059 行记录的根因却是另一件事——caller-saved 寄存器被 boot-shim 覆写（`in("rdi")` 的旧版本）。两处讲的是两次不同的故障，文档只收录了较早的一次。更要紧的是：文档说“指针本身仍按 ABI 放在 RDI/X0/A0”，读者会以为 `kmain` 消费这个指针，而生产代码明确不信任它；测试形态的 `kmain`（`feature="qemu_test"`）反而直接使用形参（`kmain_verify` 读 `kernel_info.kern_virt_base`）。生产与测试消费不同信息源。
- 影响：`HigherHalf` 的 `kinfo` 参数、`kmain` 的形参、`arch_boot_finish` 的 `store_kernel_info` 返回值三者构成一条“看起来在传指针、实际只传一个必须已被写好的全局”的链。新架构实现者按文档写一个消费形参的 `kmain` 会立刻踩回已知故障；而按实现写又会与文档 §3.4 的前置条件表（“`kinfo` 在高端可访问”）对不上——真正的先决条件是“`KERNEL_INFO` 已被 `store_kernel_info` 写入”。
- 证据或推理：`kmain` 第一句 `let kernel_info: &KernelInfo = crate::kernel_info().expect(...)`（1687-1688）遮蔽形参；函数体内此后没有对形参的读取。实现注释原文：`the parameter reference was spilled by the compiler to the kernel stack (near kern_stack_top) and reloaded from a stack slot corrupted by aliasing/overwrites — every field read through the parameter was .rdata ASCII garbage`（1678-1686）。测试侧 `#[cfg(feature = "qemu_test")] pub extern "C" fn kmain(kernel_info: &KernelInfo)`（2067-2068）与 `kmain_verify` 使用形参（2121 行 `let kern_high = kernel_info.kern_virt_base.0;`）。
- 最小修复：二选一。(a) 保守：形参改名 `_untrusted_kinfo`，在 §4.4 补记第二次根因，并在 trait 的 Safety 里把“`kinfo` 可访问”改为“`KERNEL_INFO` 已写入”；(b) 彻底：`kmain` 改无参，`HigherHalf::jump_to_kmain` 同步去掉 `kinfo`（与 D2-04 合并），Safety 只保留“分页已启用 + 全局副本已写 + 栈顶来自该副本”。(b) 让签名与语义一致，但要同步文档 §3.4/§4.3/§4.4 三处代码引用。
- 置信度：高（直接读生产函数体与测试函数体）。
- 是否需源码核实：否（Rust 侧事实）；C 侧 `main()` 无对应形参，不需要对照。
- 若我错了：如果某条非 x86 腿的 `kmain` 在遮蔽前读过形参，或 `kmain` 之外还有消费者依赖这个形参，则本条降级为文档补充。grep `kmain(kernel_info` 只见 `HigherHalf` 的 asm 调用与本函数定义，暂未发现第三个读者。
- 裁决：接受。理由：签名是本章契约的核心，不能留“形参不可信但不说明”的状态。代价：方案 (b) 约 6 处代码 + 3 段文档；方案 (a) 约 2 处代码 + 1 段文档。

## FINDING RD1-DESIGN-D2-02（P2）

- 位置：`/home/xzhao/github/minix-rs/os/libs/minix-boot/src/kernel_info.rs:99-107`、`462-470`、`576-629`；`os/boot-shim/src/lib.rs:126`；`os/kernel/src/lib.rs:726-733`；`os/libs/minix-boot/src/handoff_v1_read.rs:182`；文档 §3.3 `…/02-higher-half-kernel.md:519-531`
- 严重度：P2（同一不变量在四处表达；线协议携带可导出的状态）
- 分类：[DEL]
- 问题：文档把 `kern_stack_top` 描述为装载方的产出、按值交接的输入（“装载方算好一个地址，放进 `KernelInfo`，跳转处只消费它”）。实现里它是 `kern_virt_base + kern_size` 的派生缓存：`build_kernel_info` 由和导出，严格门又反向断言必须等于这个和，v1 线协议再存一份，`arch_boot_finish` 再把字段当第二个实参传下去。三个生产构造点（共享 builder、v1 解码、两个测试载体）都必须独立算对这个和；唯一允许偏离的是宽松门的测试载体。
- 影响：(1) 新增一条引导腿就要多算一次导出值，算错只在严格门炸，宽松门静默通过；(2) 线协议多一个可伪造字段，安全论证被迫写成“严格门等值断言”（`kernel_info.rs:447-470` 的注释直接说 A forged wire can otherwise carry a legal-looking `kern_stack_top`）；(3) 文档 §3.3 的“栈空间账”（从 `kern_stack_top` 往下量）与实现里的派生关系被讲成两件事，读者会以为栈顶可以独立于跨距移动。
- 证据或推理：`build_kernel_info` 的 `kern_stack_top: VirBytes(kern_virt_base.0 + kern_size)`（boot-shim/lib.rs:126）；严格门的 `assert_eq!(self.kern_stack_top.0, expected_stack_top, "production kern_stack_top does not equal kern_virt_base + kern_size")`（kernel_info.rs:462-470）；`check_boot_postconditions` 只保留 `>= base` 与 `<= base+size` 两条不等式（597-628）；v1 解码从线协议读它（handoff_v1_read.rs:182、lib.rs:731）；两个 `jump_to_kmain` 调用点都传 `kinfo.kern_stack_top`（lib.rs:232、375）。
- 最小修复：把字段降级为访问器（`fn kern_stack_top(&self) -> VirBytes`），保留 `kern_virt_base`/`kern_size` 两个真值；线协议 v2 删掉该槽位并在接收侧派生。注意 `kernel_info.rs:631-648` 的“Getter methods（preferred API over direct field access）”段现在是空段——说明访问器方向本来就存在过，后来被删了。
- 置信度：高（构造点与读者已穷举 grep）。
- 是否需源码核实：C 侧 `kinfo_t` 没有这个字段（C 用链接符号 `k_initial_stktop`，§2.4 已对照），所以这不是“保持 C 字段”的约束。
- 若我错了：如果存在合法形态要求栈顶与镜像尾分离（例如将来把引导栈移到镜像外），字段就是必要的。实现注释自己许诺了这种可能（`kernel_info.rs:619-622`：“If a future layout legally moves the boot stack out of the image, relax this”），但同一文件的严格门又把这条可能性钉死。两条注释互相抵消，至少应删掉其中一条。
- 裁决：访问器部分接受；线协议 v2 需用户裁决（成本在文档 01 与协议版本）。代价：访问器低；协议中。

## FINDING RD1-DESIGN-D2-03（P2）

- 位置：`/home/xzhao/github/minix-rs/os/libs/minix-boot/src/kernel_info.rs:109-117`（字段与自述）、`458-461`（严格门断言）；`os/boot-shim/src/lib.rs:126`；`handoff_v1_read.rs:183`；文档第 3、4 章未提及
- 严重度：P2（死状态占据跨镜像协议位，并制造一条假的相等不变量）
- 分类：[DEL]
- 问题：`KernelInfo.syscall_entry` 在生产路径没有读者。内核安装 SYSCALL 入口用的是 `minix_arch::syscall_entry_va()`（`os/arch/src/lib.rs:365-386` 的注释明确写 “not from the boot-provided `KernelInfo.syscall_entry`, which is reference-only metadata”；`os/kernel/src/lib.rs:2266-2270` 复述同一句）。字段仍由 `build_kernel_info` 写入、由 v1 线协议携带、由严格门断言等于 `kern_virt_base`。全仓唯一真实读者是 riscv64 保护测试载体（`test-protection-riscv64/src/main.rs:148`），而该载体的值 `base+0x100000` 会被生产严格门拒绝——正好证明“生产断言”和“真实读者”用的不是同一套语义。
- 影响：读者按字段注释（“only x86-64 uses it to configure LSTAR MSR”，`kernel_info.rs:109-117`）去 LSTAR 初始化处找消费者，会找不到；改动字段值不会影响任何生产行为，但会触发严格门 panic。这属于“类型上可表示、语义上无意义”的状态：等于把一个常量写进三个构造器、一条线协议、一条断言，再让所有人维护它。
- 证据或推理：`grep -rn "\.syscall_entry" os/ --include=*.rs`（越界扫描，见基线披露）只有：构造点、`opensbi_helpers.rs:864` 与 kernel 测试里的断言、`test-protection-riscv64:148` 的 `trap.configure_syscall(info.syscall_entry)`。生产消费者为零。`syscall_entry_va()` 在 `os/arch/src/x86_64/trap_stub.rs:601-604` 用 `&x86_syscall_entry` 的符号地址，另一条独立来源。
- 最小修复：线协议 v2 删除该槽位与字段；若协议本版冻结，则保留字段但把严格门断言改成 `debug_assert`，并在字段文档里写明“readers: 仅 riscv64 保护测试载体；生产以 `syscall_entry_va()` 为准”。
- 置信度：中高（读者穷举已做；但 `os/arch/src/lib.rs` 在 27 文件之外，见基线披露）。
- 是否需源码核实：需 C 源码核实一次：C 的 `kinfo_t` 是否有等价字段。实现注释说 C 从 asm label 取（protect.c:189-205），若属实则本字段是 Rust 新增而非 C 残留。我未读 C 源。
- 若我错了：如果某条未 grep 到的路径（例如 VM/PM 通过 GET_KINFO 转出该字段）读它，则本条降级为“文档补一句 reference-only”。目前 `misc.rs:721-726` 的 GET_KINFO 只取 `user_sp`/`kern_virt_base`。
- 裁决：需用户裁决（与 D2-02 同一次线协议版本变更）。代价：中。

## FINDING RD1-DESIGN-D2-04（P2）

- 位置：`/home/xzhao/github/minix-rs/os/kernel/src/boot/higher_half.rs:48`；调用点 `os/kernel/src/lib.rs:232`、`375`；文档 §3.4 `…/02-higher-half-kernel.md:556-568`
- 严重度：P2（可表示的非法组合，且无任何检查）
- 分类：[DEL]
- 问题：`jump_to_kmain(kinfo, stack_top)` 的两个实参其实是同一个信息源：两个调用点都传 `kinfo.kern_stack_top`。文档 §3.4 的前置条件表把 `stack_top` 列为独立值，并给它单独一条“16 字节对齐由值的一侧保证”的守卫；实现里这个值只能是 `kinfo` 的字段，独立违反方式只有“调用者手写一个不同的值”，而那正是没有任何运行时检查的组合。
- 影响：第二个参数制造了一个不变量（`stack_top == kinfo.kern_stack_top`），它既不写在 Safety 里，也不被断言，且一旦违反，栈会落在一个与映射/栈空间账不同的位置（§3.3 的“从 `kern_stack_top` 往下量”全部失效），症状是引导期随机覆写，而不是可定位的 panic。对一个 `unsafe` 引导契约来说，这是本可避免的自由度。
- 证据或推理：`grep -n "jump_to_kmain(" os/kernel/src/lib.rs` 只有 232、375 两处真实调用，实参均为 `kinfo.kern_stack_top`；trait 定义 48 行；mock 实现 6083 行也照抄两参。`stack_top` 在三个实现里都只用于 `mv/mov sp`（x86_64 58-69、aarch64 60-71、riscv64 99-121）。
- 最小修复：改为 `unsafe fn jump_to_kmain(kinfo: &KernelInfo) -> !`，三个实现内部读 `kinfo.kern_stack_top`；若与 D2-01(b) 合并，则改为零参并读全局副本。Safety 收窄为“分页已启用、全局副本已写、其栈顶 16 字节对齐”。
- 置信度：高。
- 是否需源码核实：否。
- 若我错了：如果将来需要在同一份 `KernelInfo` 上跳到不同栈（例如 AP 引导），参数才有独立意义；当前没有 AP 走这条路径（AP 由 `smp_init` 另行交付）。
- 裁决：接受。代价：3 个实现各改 2 行、2 个调用点、3 段文档引用；低。

## FINDING RD1-DESIGN-D2-05（P2）

- 位置：`/home/xzhao/github/minix-rs/os/libs/minix-elf/src/lib.rs:330-390`（`Iterator for SegmentIter`）；`os/boot-shim/src/loader.rs:175-222`（`compute_kernel_layout`）；文档 §3.2 `…/02-higher-half-kernel.md:496`
- 严重度：P2（静默降级为“布局更小”，与本仓已发生过的故障同型）
- 分类：[MODERN]
- 问题：迭代器对损坏输入的动作是 `continue`——不产出该段，不报错。`SegmentIter::new` 已经把程序头表的范围校验完（305-330），所以 `parse_phdr` 的越界分支在正常路径不可达；真正会触发的静默跳过是最后一道：`p_offset + p_filesz > image.len()`（383-389）。`compute_kernel_layout` 只对“收到的段”取 min/max，不统计被跳过的段，于是被截断的 ELF 会得到一个“看起来合法”的更小 `kern_size`。
- 影响：内核镜像从 ESP / FAT 读短（坏块、镜像打包截断、memsz 写错）时，高半映射只覆盖镜像的一部分，`kern_stack_top` 落在镜像中间。这不是假想故障：`os/kernel-image/src/bootface.rs:113-120` 的注释记录过同型现场——“the high half then mapped only half the image and `kern_stack_top` landed mid-.bss (silent corruption)”。引导路径应 fail-closed，而不是把“段比预期少”交给调用方去猜。文档 §3.2 诚实地记录了这个行为，但没有把它判为设计缺陷。
- 证据或推理：`next()` 的 `Err(_) => continue`（370-375）与 `if seg_end > self.image.len() { continue; }`（383-389）；`compute_kernel_layout` 的 `while let Some(seg) = iter.next()` 与 `found_load` 只区分“零段”和“有段”（189-218）；`ElfError` 没有“表项被跳过”这一变体。对照组：`load_kernel_with_loader_gated` 对 `e_entry != kern_virt_base` 选择 `assert_eq!` 当场失败（loader.rs:404-408），说明本路径的既定风格是 fail-fast，段跳过是唯一例外。
- 最小修复：给 `SegmentIter` 增加一个 `error: Option<ElfError>` 槽（或在 `next` 里记录第一个跳过原因），`compute_kernel_layout` 在循环后检查并返回该错误；非 `PT_LOAD` 过滤保持“正常跳过”。若不想改迭代器公有 API，可加 `segment_iter_strict()` 仅供布局计算使用。
- 置信度：高（代码路径直接可读）。
- 是否需源码核实：否（这是 Rust 侧解析器；C 侧由 GRUB 读，无对应行为）。
- 若我错了：如果生产者保证“镜像永远完整”（例如 loader 在读文件后先做长度校验），跳过分支就是纯防御、可以保留。但 `load_kernel_with_loader` 没有对 `elf_data.len()` 与段声明的总量做交叉校验，所以当前没有这层保证。
- 裁决：接受。代价：低（错误槽方案）；中（`Item = Result` 方案，5 个调用点）。

## FINDING RD1-DESIGN-D2-06（P2）

- 位置：`/home/xzhao/github/minix-rs/os/libs/minix-elf/src/lib.rs:165-186`、`os/boot-shim/src/loader.rs:175-222`、`224-282`、`286-341`；文档 §3.2 `…/02-higher-half-kernel.md:498`、§4.2 `:865`
- 严重度：P2（错误类型越层复用 + 归一化，boot 失败不可诊断）
- 分类：[MODERN]
- 问题：`minix_elf::ElfError` 同时承担“解析失败”和“装载/布局失败”两种职责。boot-shim 用 `InvalidSegment` 表示至少四种不同事实：`filesz > memsz`（loader.rs:205-207）、`paddr + memsz` 溢出（207-210）、`kern_end_phys - kern_phys_base` 下溢（218-221）、段物理区间重叠（303-305）。`ElfError` 是 `Copy` 的纯枚举、无负载，所以错误现场（哪一段、哪个地址、哪个量）在返回时就丢了。`minix-elf` 的注释还引用了一个全仓不存在的类型：`Reason is a static label — see LoaderError for the dynamic message`（minix-elf/src/lib.rs:175），而 `grep -rn "LoaderError" os/` 只命中这一行注释本身。
- 影响：引导失败时串口只能打印一个变体名，无法区分“镜像截断”“链接脚本重叠”“物理地址回绕”。宿主测试也只能断言变体相等，无法断言具体原因。缓冲区孪生 `load_segments_into_buffer` 把这条错误通道彻底抹平：`Err(_) => return 0`（225-228），且 `count += 1` 对被跳过的段也计数（267），于是“返回 0”同时表示空镜像与解析失败。文档 §4.2 已把两版差异记为 D02-F04，但没把“错误类型丢失”记进去。
- 证据或推理：四处 `ElfError::InvalidSegment` 的产生点（loader.rs:205、209、219、303）；`ElfError` 定义（minix-elf 165-186）无负载字段；`load_segments_into_buffer` 的返回值语义（224-282）；`load_segments_into_phys_memory` 在重叠时返回同一个变体，而源-目的重叠又改走 `panic!`（308-315）——同一类“重叠”被分到两个错误通道（返回值 vs panic），调用方无法统一处理。
- 最小修复：在 boot-shim 定义 `pub enum LoaderError { Parse(minix_elf::ElfError), Segment { index: usize, reason: SegmentReason }, SpanOverflow, Overlap { a: usize, b: usize }, SourceOverlap { src: usize, dst: usize } }`；`minix-elf` 只保留解析错误；`load_segments_into_buffer` 返回 `Result<usize, LoaderError>`，`count` 只统计真正落地的段。源-目的重叠与段-段重叠统一走返回值或统一 panic（当前的选择应写明理由）。
- 置信度：高（类型与产生点直接可读）。
- 是否需源码核实：否。
- 若我错了：如果上层只关心“成功/失败”而不需要原因，纯枚举是刻意的极简。但同一路径已经有 `panic!` 携带动态消息（源-目的重叠），说明作者自己认为需要可诊断性；两条通道并存才是问题。
- 裁决：接受。代价：中（loader 的 5 个公有函数签名、测试与两个 firmware helper 的调用点）。

## FINDING RD1-DESIGN-D2-07（P3）

- 位置：`/home/xzhao/github/minix-rs/os/kernel/src/boot/higher_half.rs:40-48`；文档 §3.4 `:562-568`
- 严重度：P3
- 分类：[MODERN]
- 问题：Safety 第四条“只从引导 CPU 调用一次”没有类型证据；两次调用会二次换栈。文档自己在表里承认“类型系统不区分调用次数”，所以这不是文档与实现不符，而是可选的强化点：本仓已有 `CheckedWindows` 令牌先例（D-040），用零尺寸令牌把一次性证明前移是既有风格。
- 影响：当前只有两个调用点、都是启动单线程，实际风险接近零；引入令牌的收益主要是防止未来新增第三条跳转路径时重复调用。
- 证据或推理：`grep -n "jump_to_kmain("` 只有 `arch_boot_finish` 与 `resume_additive` 两处；两者都在 `-> !` 路径上，且各自只执行一次。
- 最小修复：若采纳 D2-01(b)/D2-04（零参跳转），Safety 收窄为“全局副本已写、分页已启用”，一次性风险由“只有一个入口函数”表达，无需令牌；否则可加 `struct BootCpu(());` 由 `arch_boot_finish` 生成并消费。
- 置信度：高。
- 是否需源码核实：否。
- 若我错了：如果将来有 AP 引导复用这条路径（当前 AP 走 `smp_init`），一次性假设会被打破，届时令牌才真正有价值。
- 裁决：拒绝（本轮）。理由：收益不足，且与 D2-01/D2-04 的签名收缩方向重复；若签名收缩落地，令牌的必要性进一步下降。代价：低（但不做）。

## FINDING RD1-DESIGN-D2-08（P3）

- 位置：`/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:6065-6099`；`os/kernel/src/boot/higher_half.rs:21-30`
- 严重度：P3（虚假的可测试性论据 + 死测试脚手架）
- 分类：[DEL]
- 问题：trait 的模块文档把 Testability 列为选择 trait 的三条理由之一（“Mock implementations can verify the signature and the call contract”），但唯一的 mock 测试只做编译期实例化 `accept_higher_half::<MockHigherHalf>()`；`HIGHER_HALF_CALLED` 只被写、从不被读；mock 的注释还停在一个已不存在的签名上（`The real trait has fn jump_to_kmain(kinfo: &KernelInfo) -> !`，而真实签名是两参）。这个测试不会发现任何行为回归，签名漂移也由真实实现的三份 `#[cfg]` 编译覆盖。
- 影响：评审者看到“有 mock 测试”会误以为跳转契约在宿主上可验证；实际只有真机 `test-higher-half` 能验证。文档 §5.2 已正确地把真机测试列为唯一证据，因此这里的论据是多余的乐观。
- 证据或推理：`HIGHER_HALF_CALLED` 的两处出现（6070 定义、6084 写入）之外无读者；测试体（6092-6099）只有 `accept_higher_half`；mock 循环体 `loop { spin_loop() }` 永远不会被执行（没有测试调用 `jump_to_kmain`）。
- 最小修复：删掉 mock、`AtomicBool` 与循环体，把“签名由三份实现保证”写进 trait 文档；保留一条 `const _: () = { fn _assert<H: HigherHalf>() {} };` 之类的编译期断言也可以，但不要声称它能验证调用契约。
- 置信度：高。
- 是否需源码核实：否。
- 若我错了：如果某个 `mock` 构建的宿主测试打算调用 `jump_to_kmain` 并检查标志（现在没有），mock 就有用途；那需要补上读取断言，而不是删掉。
- 裁决：接受。代价：极低。

## FINDING RD1-DESIGN-D2-09（P3）

- 位置：`/home/xzhao/github/minix-rs/os/kernel/src/boot_policy.rs:107-120`；`os/arch/src/arch/boot.rs:461-464`、`734`
- 严重度：P3（死计算与哨兵式错误吞没）
- 分类：[DEL]
- 问题：两处小残留。(1) `plan_kernel_mapping` 计算 `virtual_end` 后写 `let _ = virtual_end;`（120），说明曾经用它做上界检查，现在只剩 `validate_virtual_range`，溢出检查是否等价取决于该函数内部是否 checked——现状是读者无法从这段代码判断。(2) `load_elf_into` 用 `entry_point(image).unwrap_or(0)`（464），随后在 734 行把 0 当作 pc 写进 `VmLoadResult`；同一函数在 461 行已经用 `segment_iter` 解析过一次 ELF 头，而 `SegmentIter::ehdr()` 正好提供 `e_entry`，所以这是“二次解析 + 哨兵吞错”的组合。
- 影响：`unwrap_or(0)` 把“入口未知”变成“入口是 0”，一个语义上不可能的值（用户进程入口不能是 0）；下游若有人忘了检查，会得到 `pc=0` 的进程。二次解析则是同一份字节的两套校验，未来改一处漏一处。
- 证据或推理：`let _ = virtual_end;`（boot_policy.rs:120）；`let entry = minix_elf::entry_point(image).unwrap_or(0);`（boot.rs:464）；`pc: VirBytes(entry)`（734）；`SegmentIter::ehdr()` 的定义与用法（minix-elf 312-316、loader.rs:177）。
- 最小修复：`plan_kernel_mapping` 删掉 `virtual_end` 或改为 `if layout.virtual_base.checked_add(layout.size).is_none() { return Err(InvalidKernelLayout) }`（保持显式）；`load_elf_into` 改为在迭代前取 `let entry = iter.ehdr().e_entry;`，删掉 `unwrap_or(0)`。
- 置信度：高。
- 是否需源码核实：否。
- 若我错了：如果 `entry_point` 的失败条件与 `segment_iter` 的失败条件不同（看代码：两者都调用 `parse_ehdr`，条件相同），二次解析就是纯冗余；若将来 `entry_point` 增加额外校验，哨兵仍应改成显式错误。
- 裁决：接受。代价：极低。

## FINDING RD1-DESIGN-D2-10（P3）

- 位置：`/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:236-300`、`785-795`
- 严重度：P3（三份复制，其中一份已经分叉）
- 分类：[DEL]
- 问题：三个 `arch_boot` 壳的函数体都只有一句 `arch_boot_finish::<XBootArch, XHigherHalf>(kernel_info, root_page)`；差异只有 `HigherHalf` 类型与注释。x86_64 那份额外带 40 行取证打印（D2-13），说明复制已经开始漂移。项目已经在 `resume_additive` 里用 `CurrentBootArch` 别名消掉了同类重复（lib.rs:388、758、777），但 `arch_boot` 没有对应的 `CurrentHigherHalf` 别名。
- 影响：新增一条启动腿要记得改三处；注释里的腿特定信息（aarch64 的 `.bss` 取值、riscv64 的 a0 约定）可以留在 arch 模块，不必留在入口壳里。
- 证据或推理：三个壳的签名与唯一语句（285-290、295-299、790-794）；`CurrentBootArch` 的既有用法。
- 最小修复：各 arch 模块导出 `pub type CurrentHigherHalf = XHigherHalf;`（`higher_half` 模块已在 `mod.rs` 里公开），把 `arch_boot` 收成一个 `#[cfg(any(target_arch = ...))]` 的函数；取证打印随 D2-13 删除。
- 置信度：高。
- 是否需源码核实：否。
- 若我错了：如果三条腿将来需要不同的入口语义（例如 riscv64 直载需要不同的 `root_page` 预处理），壳才有独立价值；当前三条腿的实参完全同形。
- 裁决：接受（与 D2-13 一起做）。代价：低。

## FINDING RD1-DESIGN-D2-11（P3）

- 位置：`/home/xzhao/github/minix-rs/os/kernel/src/arch/x86_64/link.ld`、`aarch64/link.ld`、`riscv64/link.ld`（三份均不在 27 文件内，仅按文档与测试引用判断）；`/home/xzhao/github/minix-rs/os/kernel/tests/link_ld_lockstep.rs:1-85`
- 严重度：P3（死亡配置 + 为它专设的测试）
- 分类：[DEL]
- 问题：文档 §3.1（356 行）自己断言这三份脚本没有生产链接读者，并给出重算命令；我按该命令复跑，`os/qemu-tests/*.sh` 与 `os/xtask/src/*.rs` 无命中。全仓唯一的程序化读者是 `link_ld_lockstep.rs`：它把三份脚本当文本读进来，用一个手写的 `parse_hex_value` 解析常量并与 `os/kernel-image/*.ld` 比较。也就是说，三份不被链接的文件由一条测试维持“常量与生产脚本相等”，其余引用（`os/kernel/src/lib.rs:6220`、`os/arch/src/arch/direct_map.rs:244,296,350`）全是注释。文档 §4.1 已经把同一布局讲了一遍，并给了 `os/kernel-image/*.ld` 的逐段解读。
- 影响：读者会以为这些脚本是可切换的链接输入；实际上改变它们只影响一条测试。维护成本有三份文件 + 一个自定义文本解析器；风险是有人误以为改它们能改变布局。
- 证据或推理：文档给的重算命令实测 `rc=1`（无命中）；`link_ld_lockstep.rs` 的 `fs::read_to_string` 与 `parse_hex_value`（18-45、47-85）是唯一读者；`link_ld_lockstep.rs:3-5` 的注释自述“has no production link reader … but its constants are still documentation of the intended layout”。
- 最小修复：删除三份脚本与 `link_ld_lockstep.rs`，把“预期布局”留在文档 §3.1/§4.1（已有）；若它们是对外部工具（例如某个未入库的链接流程）的约定，则在仓库根加一份声明文件说明读者是谁，并把测试改成“从生产脚本派生”的形式。
- 置信度：中（文件不在允许直读范围，依据是文档复算命令、测试内容与注释）。
- 是否需源码核实：需确认仓库外是否有工具消费这三份路径；仓内已无可执行读者。
- 若我错了：如果 xtask 的镜像打包在某个未覆盖的分支里读取它们（当前 `grep` 未命中），删除会破坏构建；裁决前应把 `grep -rn 'link\\.ld' os/xtask os/qemu-tests tools` 扩到全仓再确认。
- 裁决：需用户裁决（可能是外部约定）。代价：低。

## FINDING RD1-DESIGN-D2-12（P3）

- 位置：`/home/xzhao/github/minix-rs/os/libs/minix-boot/src/kernel_info.rs:119-121`、`679-695`、`467-481`；`os/boot-shim/src/loader.rs:519-560`
- 严重度：P3（一条身份有两个通道，只有一条被生产门校验）
- 分类：[MODERN]
- 问题：模块身份同时由“位置”和“名字字符串”表达。位置是隐式契约（`MODULE_NAMES` 数组顺序、`NR_BOOT_MODULES` 常数、内核的 `BOOT_MODULE_PROC_NRS`），名字存在 `BootModule.name` 里；严格门把两者交叉比对（`i < MODULE_NAMES.len() && module.name == MODULE_NAMES[i]`），宽松门不比。测试载体传空模块表，于是“名字”这条通道在测试形态完全不存在。
- 影响：一个宽松腿可以把位置 6 的镜像配上位置 7 的名字而不被发现（严格门明确说这是“forged wire”防护）。从类型上看，`BootModule` 允许任意 `&'static str` 与任意位置组合，语义上只允许 12 个固定组合。
- 证据或推理：`MODULE_NAMES` 的注释自称“shared positional contract”（kernel_info.rs:679-687）；`BootModule.name` 是 `&'static str`（691-695）；严格门的名字-位置比对（467-481）；`build_kernel_info` 的调用者传 `modules` 切片，名字来自 `loader.rs` 的 `MODULE_NAMES`（519-560）。
- 最小修复：把身份收敛到一个通道。最小改动是把 `name` 改成访问器 `fn name(&self) -> &'static str`，由位置查 `MODULE_NAMES`（字段删除，线协议随之简化）；更大的改动是 `ModuleSlot(u8)` newtype + `[BootModule; NR_BOOT_MODULES]` 定长数组，让位置在类型里。
- 置信度：中高（跨 crate 的身份消费者不止 `loader.rs`，`BOOT_MODULE_PROC_NRS` 在 `os/kernel/src/proc.rs`，不在 27 文件内，未逐行核实）。
- 是否需源码核实：需核实内核是否在别处按 `name` 做查找（而非按位置）；若是，访问器方案要保留名字查询语义。
- 若我错了：如果 `name` 还承担用户可见的进程名（`p_name`），删字段会改变可观测输出；那就保留两个通道，但把严格门改成无条件（所有形态）比对，至少让两个通道不可能不一致。
- 裁决：拒绝（改造为 newtype 的成本高于收益）；接受“文档明确 name 是诊断字段 + 严格门是唯一一致性保证”的补注。代价：低。

## FINDING RD1-DESIGN-D2-13（P2）

- 位置：`/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:246-276`（`arch_boot` 的取证块）、`1681-1715`（`kmain` 的 preA 取证块）
- 严重度：P2（生产引导路径里的临时取证输出，文档未记录）
- 分类：[DEL]
- 问题：x86_64 的 `arch_boot` 在进入时打印 `kernel: arch_boot arg=…`、`reslen=… r0=…`，并把注释写成 `Boot-visibility evidence capture (temporary)`；`kmain` 在遮蔽形参后又读取 RDI/RSP 打印 `kernel: preA rdi=… kinfo=… rsp=…`，注释写 `Evidence capture (temporary, v3)`，并说 `This shadow can be deleted once the diagnostics below confirm it`。这两块在 `#[cfg(not(feature = "mock"))]` 的生产代码里，每次启动都会输出。文档 §4.4 的“从装载到 kmain 的完整路径”与状态表都没有这些输出，按文档复现串口日志会对不上。
- 影响：引导路径多两次寄存器读取与多行串口写；更实质的是它们把“临时诊断”固化成可观测行为，任何依赖串口日志的门脚本可能已经把它们当作锚点（需要核对）。同时 x86_64 的壳因此与另外两条腿分叉（D2-10）。
- 证据或推理：两处注释原文与打印语句（252-275、1689-1715）；`kmain` 的 `let kernel_info = crate::kernel_info()...` 在取证块之前（1686-1688），取证块紧接其后；`boot_stage!` 宏是另一套正式路标（399-410），与这两块独立。
- 最小修复：删除两块取证打印，保留 `boot_stage!` 路标；若某个门脚本依赖 `preA rdi=` 或 `arch_boot arg=` 字符串，先把该依赖改成正式路标再删（需要读 `os/qemu-tests/*.sh` 与 `tools/` 下的门脚本确认，不在本次允许范围）。
- 置信度：高（代码位置与注释明确）。
- 是否需源码核实：否；需脚本核实（门脚本是否 grep 这些字符串）。
- 若我错了：如果这两块是某个正在进行的真机取证的唯一手段，删除会中断取证；那就把它们移到 `feature = "boot_forensics"` 下，默认不编译，而不是留在默认路径。
- 裁决：接受（删除或加 feature 门）。代价：低（含一次门脚本核对）。

## FINDING RD1-DESIGN-D2-14（P2）

- 位置：文档 `…/02-higher-half-kernel.md:928-933`（riscv64 delta 节选）、`:1036-1053`（`build_kernel_info` 节选）、`:837-863`（`load_segments_into_phys_memory` 节选）；对应实现 `os/kernel/src/arch/riscv64/higher_half.rs:92-97`、`os/kernel/src/bootface.rs:427-439`、`os/boot-shim/src/loader.rs:286-341`
- 严重度：P2（三处“代码引用”与实现不同步；按文档复现会编译失败或漏掉一条 panic 分支）
- 分类：[DEL]
- 问题：三处事实偏差。
  1. 文档 928-933 写的是 `kinfo.kern_virt_base().0`、`kinfo.kern_phys_base().0`（方法调用）。`KernelInfo` 现在没有这两个方法：`kernel_info.rs:631-648` 的“Getter methods”段是空段，只剩注释；真实实现是字段访问（riscv64/higher_half.rs:92-97）。
  2. 文档 1038-1049 的 `build_kernel_info` 调用是 10 个实参，其中 `PhysBytes(0), 0` 出现在第 7、8 位；当前签名是 11 个参数，第 7 位是 `platform_sources`，末尾是 `memmap_sources, startup_objects, authority`（`os/boot-shim/src/lib.rs:101-131`）。真实调用点 `bootface.rs:427-439` 也是 11 参。引文所示代码无法编译。
  3. 文档 837-861 标“代码引用（保留函数体）”，但函数体除了文档列出的三步（收集、段重叠、拷贝）还有第四步：`first_source_destination_overlap` 检查与 `panic!`（loader.rs:301-315）。文档 863 行只说“再做重叠检测”，会让读者以为只有段-段一道检查；而两道检查的错误通道不同（返回值 vs panic），这正好是 D2-06 的核心事实。
- 影响：读者按 §4.3 抄 `delta` 代码会编译失败；按 §4.4 抄 builder 调用会参数错位；按 §4.2 理解装载器会漏掉一条在真机上可能触发的 panic。按本仓 review 规则，过期的代码引用属于文档事实错误（P0-fact 候选），我不越权判 P0，只标注“需 doc-fact 门复核”。
- 证据或推理：上述实现行号；`kernel_info.rs:637-648` 的空 getter 段；`bootface.rs:427-439` 的 11 个实参；`loader.rs:301-315` 的源-目的重叠块。`os/target/fmtchk/kernel_src_lib.rs:301` 还留着 `kernel_info.kern_virt_base()` 的旧副本，说明 getter 曾经存在、后来被删，文档没有跟。
- 最小修复：从当前源码重新生成三处节选；§4.2 的步骤说明补上第四步，并写明“源-目的重叠是 panic，段-段重叠是返回值”这条不对称。
- 置信度：高。
- 是否需源码核实：否（Rust 侧直接可读）。
- 若我错了：如果文档发布时对应的是另一个分支/版本，则属于版本错位而非事实错误；但基线 sha256 与本地文件一致，且 27 文件清单就是当前工作树，暂不支持这个解释。
- 裁决：接受。代价：低。

## FINDING RD1-DESIGN-D2-15（P3）

- 位置：`/home/xzhao/github/minix-rs/os/libs/minix-boot/src/kernel_info.rs:187-266`；`os/kernel/src/lib.rs:217-233`；`os/kernel-image/src/bootface.rs:440`；测试载体 `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs:41-56`
- 严重度：P3（校验严格度是调用者的选择，类型不携带“已校验”状态）
- 分类：[MODERN]
- 问题：`ValidationLevel`（`BeforeTables`/`Standard`/`Strict`）是 `pub(crate)` 枚举，三个公开入口各对应一档，由调用者选。direct face 先自己跑 `validate_requiring_sources()`（bootface.rs:440），再在 `arch_boot_finish` 里跑 `validate_before_tables()`（lib.rs:222）——同一对象被严格门和宽松门各校验一次，后者不可能发现前者漏掉的东西。测试载体则只用宽松门，并且真的持有生产契约禁止的值：x86_64 载体把 `syscall_entry` 写成 `kern_virt_base + 0x100000`（main.rs:50），而严格门要求它等于 `kern_virt_base`（kernel_info.rs:458-460）。
- 影响：类型上无法区分“原始交接数据”和“已通过生产门的数据”，新增一条生产腿时只要复制 `arch_boot_finish` 就自动得到宽松门；误用不会在编译期暴露。另一方面，direct face 的重复校验是纯浪费（虽然只在启动期一次）。
- 证据或推理：`ValidationLevel` 的定义与三个入口（191-202、233、248、264）；`arch_boot_finish` 的 `validate_before_tables()` 调用（lib.rs:222）；bootface 的 strict 调用（440）；x86 载体的非规范 `syscall_entry`（main.rs:50）。
- 最小修复：最小改动是改名为 `validate_test_carrier()` 并把文档写清“仅测试载体可用”，同时让 direct face 不再重复校验（`arch_boot_finish` 接收一个已校验令牌或直接跳过）；更大的改动是类型状态（`KernelInfo<Raw>` → `ValidatedKernelInfo`），让“通过生产门”成为类型。
- 置信度：高。
- 是否需源码核实：否。
- 若我错了：如果宽松门是给宿主单元测试而不是给真机载体用的，测试载体应改用 strict（它们的合成值需要先改成规范值）；这仍是“两档并存导致误用”的证据，而不是反证。
- 裁决：需用户裁决（类型状态成本中等；最小改名可先做）。代价：低（改名）到中（令牌/类型状态）。

---

## ARCH-PROPOSAL

### A1 · 载荷所有权类型化：`KernelInfo` 的 `&'static [T]` 改为“已落地”类型

**扩张内容**

把 `KernelInfo` 里 7 个指向固件池的 `&'static [T]` 载荷（`memmap`、`reserved_regions`、`boot_modules`、`platform_sources`、`param_buf`、`memmap_sources`、`startup_objects`）拆成两层类型：

- 引导侧：`PayloadWitness<'a>`（生命周期绑定到 boot-shim 的池/静态存储），只能被交接点消费一次；
- 内核侧：`LandedKernelInfo`（字段是固定 `.bss` 数组的切片，或指向单一 `.bss` arena 的偏移+长度），`store_kernel_info` 成为唯一构造器，内核其余代码只见 `&'static LandedKernelInfo`。

现有 `KernelInfo` 保留为 wire/生成侧的描述类型，不再被内核运行期持有。

**三处一致计划**

1. 文档：§3.3 的“按值交接”段改写为“所有权交接”；§4.4 的 `store_kernel_info` 段从“复制 7 个切片”改为“一次落地 + 类型边界”；§4.5 的对照表补一行“C 的 `kinfo_t` 由内核静态持有 → Rust 的 landed 类型”。
2. 设计/接口：`minix-boot` 新增 `PayloadWitness`/`LandedKernelInfo`；`build_kernel_info` 返回 `PayloadWitness`；v1 解码返回 `PayloadWitness`；`KernelInfo::validate*` 的派生等值断言随字段迁移；5 个构造点（UEFI、OpenSBI、riscv64 bootface、aarch64 bootface、v1 接收）统一。
3. 代码：`os/kernel/src/lib.rs:1418-1565` 的 7 个 landing pad 合并为一个 `BootArena`/`LandingPad<T, N>`；`globals.rs` 的 7 个 `UnsafeCell` 存储收敛；wire 的 scalars 只保留不可从其他字段导出的量（`kern_virt_base`、`kern_phys_base`、`root_page`、bump 三元组等），派生标量按 D2-02/D2-03 删除。

**不做会坏在哪**

- 新增一个载荷字段必须同时改 7 个 pad；漏一个的后果是内核持有一个悬垂的 `&'static`，而类型系统不会报警。`store_kernel_info` 的注释已经记录这一疾病复发过三次（`reserved_regions`、`memmap`、`boot_modules` 各自一次真机零读）。
- `param_buf`（清零尾部）与 `boot_modules`（名字池 + UTF-8 前缀）两处已经手写分叉，`land_slice` 只覆盖 5 处；后续修复必须记得“有的 pad 不走 land_slice”。
- `#[repr(C)]` + `&'static` 的组合会让读者把它当成跨镜像 ABI（注释自己说“不是 ABI”），这类误读在交接协议演进时最危险。

**代价**

高：跨 `boot-shim`、`minix-boot`、`kernel` 三个 crate，5 条引导腿，wire 版本变更，文档 01/02 两篇。建议不与本轮 P2/P3 修复混做；若暂缓，先做 D2-02/D2-03 的线协议清理与 `land_slice` 统一（低成本部分）。

### A2 · 镜像跨距改由链接符号差导出，删除手写 `KERN_SIZE`

**扩张内容**

在三份 `os/kernel-image/*.ld` 的 `.bss` 闭合（`. = ALIGN(0x200000)`）之后各加一行 `__image_end = .;`；`bootface.rs`/`bootface_a64.rs` 用两个链接符号之差计算跨距：

```
kern_size = (__image_end 的运行期地址) - (_start 的运行期地址)
```

两个符号在同一次取址、同一执行视图下解析，差值是视图不变量（链接期常量），所以物理视图与高半视图给出同一个数。删除 `const KERN_SIZE`，L9 从“实测跨距对常量”改为“实测跨距对派生值”。

**三处一致计划**

1. 文档：§3.3 的“例外是自引导形态——三个量是与脚本同源的手写常量”改写；§3.5 的“RISC-V 自引导面把跨距写成了常量”改写；§5.3 的 L9 说明同步。关键论证要补一句：§3.3 拒绝链接符号的理由（符号的绝对值随执行视图变化）只适用于**绝对值**（栈顶），不适用于**两个符号之差**——差值在两个视图下相同。
2. 脚本：三份 `os/kernel-image/*.ld` 加 `__image_end`；`os/kernel/src/arch/*/link.ld` 若按 D2-11 保留，也加同形符号。
3. 代码/门：`bootface.rs`、`bootface_a64.rs` 各改一处；`check-layout.sh` 的 L9 与 `ostest` 的布局断言同步；`startup_objects` 里内核镜像记录改用派生跨距。

**不做会坏在哪**

- aarch64 direct-core 现在只有链接期上界断言（`__bss_end - KERNEL_VIRT_BASE <= 0x400000`），没有等值对账；`KERN_SIZE` 同时决定映射上界与 `kern_stack_top`（§3.3 的栈空间账）。镜像实际跨距小于 4 MiB 时，栈顶越过真实镜像尾，落在“被内核映射但不属于镜像”的物理页上——当前 QEMU 布局下这块恰好空闲，换成别的装载布局就不再保证。
- riscv64 有 L9 兜住常量与实测的相等；aarch64 没有对等物。两条同构腿的保证强度不同，而文档只用一句括号带过。
- 一旦有人放宽或替换链接脚本的上界断言（例如镜像增大到 5 MiB），aarch64 会静默地把栈顶放到镜像中段——正是 `bootface.rs:113-120` 记录过的同型故障。

**代价**

中：三份脚本各一行、两个 face 各一处、L9 与文档两节。主要风险有两条。其一，符号取址在物理视图下是 PC 相对（`la`/`adrp`），实现必须用“两符号之差”，不能写成“符号减常量”，否则重现 §3.3 已经论证过的错误。其二，差值必须覆盖整个镜像：`_start` 只在“`KEEP(*(.text.boot))` 位于首段首字节”这个现有前提下等于镜像起点；更稳的写法是脚本里同时定义 `__image_start` 与 `__image_end`，用两者之差，不依赖 `_start` 的位置。

---

## 非法状态清单（类型上可表示、语义上不可能）

| # | 可表示的状态 | 语义上为什么不可能 | 现在靠什么拦 | 关联 |
|---|---|---|---|---|
| 1 | `jump_to_kmain(kinfo, s)` 中 `s != kinfo.kern_stack_top` | 栈空间账、映射上界、保护结构的 TSS/SP_EL1 都按字段值算；两者不一致时没有任何一处对得上 | 无（两个调用点恰好相同） | D2-04 |
| 2 | `kern_stack_top != kern_virt_base + kern_size` | 严格门自己把它定义为和；文档 §3.3 的栈账也以镜像尾为前提 | 只有 strict 门；宽松门与测试载体放行 | D2-02、D2-15 |
| 3 | `syscall_entry` 为任意值 | 生产 LSTAR 不读它；该字段没有语义消费者 | 严格门断言等于 `kern_virt_base`；宽松门放行（x86 载体写 `base+0x100000`） | D2-03、D2-15 |
| 4 | 只通过 `validate_before_tables` 的 `KernelInfo` 被生产消费者使用 | `BeforeTables` 跳过 boot_modules 与 provenance 审计 | 类型不区分；仅靠调用点选择 | D2-15 |
| 5 | 一个损坏段与一个“本来就不存在”的段不可区分 | ELF 规范里 PT_LOAD 段是镜像的一部分，缺失只能是损坏 | 无；迭代器 `continue` | D2-05 |
| 6 | 位置 i 的 `BootModule` 配位置 j 的名字 | 名字与位置是同一身份的两种写法 | 只有严格门交叉比对 | D2-12 |
| 7 | `load_segments_into_buffer` 返回的计数大于真正拷贝的段数 | 计数被调用方当作“落地段数” | 无（`count += 1` 在界检查之外） | D2-06 |
| 8 | 用户进程入口 `pc = 0` | 0 不是合法用户入口 | 无（`unwrap_or(0)` 直接写入 `VmLoadResult`） | D2-09 |

## 死抽象判定（trait-audit 事实 + 自核）

- `HigherHalf`：trait-audit 记 4 实现 / 31 引用 / 0 dyn。4 = 3 生产 + 1 mock；每个目标只编译一个生产实现。它不是“单实现依赖注入”，因为注入点是架构选择，且 AGENTS.md 要求硬件差异封在 trait 后面。判定：**活抽象**，但方法参数冗余（D2-04）、mock 测试空转（D2-08）。
- `FileLoader`：3 个真实实现（UEFI / U-Boot / 测试 mock），1 处 dyn 使用；共享装载逻辑确实通过它做到“一份代码两条固件路径”。判定：**活抽象**。
- `BootArch` / `StacktraceArch` / `DirectMapArch`：都是多实现架构缝，且被本篇章的映射、帧指针链、VPN 槽位论证直接依赖。判定：**活抽象**，不在删除建议内。
- 真正今天不承担语义职责的东西不在 trait 表里：三份 `os/kernel/src/arch/*/link.ld` 与 `link_ld_lockstep.rs`（D2-11）、`HigherHalf` 的 mock 与 `HIGHER_HALF_CALLED`（D2-08）、x86_64 `arch_boot` 的取证打印与 `kmain` 的 preA 打印（D2-13）、`let _ = virtual_end`（D2-09）。判定：**死重量**，建议删除或加 feature 门。
- 未发现“无状态 Manager”型结构；也未发现为了凑编译的 `clone`。

---

## 翻译残留分类核对

| 类别 | 本篇结论 |
|---|---|
| 单实现依赖注入 trait | 未发现有害实例。`HigherHalf` 每个目标只编译一个实现，但它是 AGENTS.md 要求的架构缝（硬件差异封在实现里），不是测试注入；`FileLoader` 有 UEFI/U-Boot/mock 三个真实实现。唯一问题是它的 mock 测试空转（D2-08），不是 trait 本身多余。 |
| 无状态 Manager | 未发现。`KernelMapPolicy`/`BootPagePolicy` 是携带决策值的纯对象；`UbootFileLoader` 是借用表适配器，都有语义职责。 |
| 布尔/字符串状态机 | 两处：模块身份用名字字符串与位置双通道（D2-12）；校验严格度由调用者从三档里选（D2-15）。`ValidationLevel` 替换两个 bool 本身是正面改动（源码注释 R17 DESIGN-12）。 |
| 哨兵返回值 | 两处：`load_segments_into_buffer` 用 0 表示解析失败、返回值含被跳过段（D2-06）；`load_elf_into` 用 `unwrap_or(0)` 表示入口未知（D2-09）。`compute_kernel_layout` 的 `u64::MAX` 初值有 `found_load` 兜底，不算哨兵泄漏。 |
| clone 到能编译 | 未发现。`loader.rs` 的注释记录过一次已删除的 clone-to-compile 形（D01 Round1 D2-08），当前 `BootModule.name` 直接用 `&'static str`。`store_kernel_info` 的 `k = *kernel_info` 是真正的按值快照，不是凑编译。 |
| 对象图共享 | 一处：`KernelInfo` 的切片先指向 shim 堆，再在 `store_kernel_info` 里重绑到 7 个 `.bss` pad（A1）。bootface 侧 `BOOT_STORAGE` 的一次性 `&'static` 视图（`bootface.rs:400-425`）是同一模式的另一半，注释明确说“写完再取视图”，属于约定而非类型。 |

## 最小化机会汇总（对应任务第 5 项）

| 机会 | 删除/合并什么 | 不改外部行为？ | 代价 |
|---|---|---|---|
| D2-01 + D2-04 | `kmain` 形参、`jump_to_kmain` 的 `kinfo`/`stack_top` 两个参数 | 是（全局副本与字段是唯一真实来源） | 低 |
| D2-02 | `kern_stack_top` 字段降为访问器；wire v2 删槽位 | 是 | 低/中 |
| D2-03 | `syscall_entry` 字段、wire 槽位、严格门断言 | 是（生产无读者） | 中 |
| D2-06 | 统一错误类型；`load_segments_into_buffer` 返回值语义 | 是（错误消息更具体） | 中 |
| D2-08 | mock、`AtomicBool`、空转循环 | 是 | 极低 |
| D2-10 | 两个 `arch_boot` 壳（保留一个） | 是 | 低 |
| D2-11 | 三份死 `link.ld` + `link_ld_lockstep.rs` | 是（仓内无读者时） | 低 |
| D2-14 | 三处过期引文重生成 | 是（只改文档） | 低 |

未建议删除的层次：`HigherHalf` trait 本身、`FileBytes`、`KernelMapPolicy`/`BootPagePolicy`、`ValidationLevel` 枚举（只建议收窄可用面）。理由：它们各自承担一个类型级职责，删除会把不变量退回注释。

## VERDICT

- 本轮共 15 条 FINDING：P2 八条（D2-01、D2-02、D2-03、D2-04、D2-05、D2-06、D2-13、D2-14），P3 七条（D2-07、D2-08、D2-09、D2-10、D2-11、D2-12、D2-15）。
- 两条 ARCH-PROPOSAL：A1（载荷所有权类型化）、A2（跨距改由链接符号差导出）。按 D-047，二者只在此节登记，不写成 FINDING，也不构成硬阻断；需要判决账行才能进入实施。
- 最高风险项是 **A2**：aarch64 direct-core 的跨距常量没有等值对账，而同一条常量同时决定映射上界与栈顶。它不是本轮新增的缺陷，是文档 §3.5 已用括号记录、但没有给出对等保证的风险。
- 设计结论（七维）：状态表达上，本篇把“装载方产出”讲成输入的地方（栈顶）与实现的“派生缓存 + 反向断言”不一致（D2-02）；身份上，模块的双通道与腿的编译期身份都还能接受，前者建议补文档而非改类型（D2-12）；所有权上，`&'static` 与实际生命周期不一致是唯一的结构性问题（A1）；错误上，ELF 解析错误与装载错误共用一个无负载枚举，且损坏段被静默降级（D2-05、D2-06）；并发上，一次性跳转靠 `unsafe` 契约，文档已诚实记录，令牌收益不足（D2-07 拒绝）；抽象边界上，`HigherHalf` 与 `FileLoader` 都有真实职责，不建议去 trait，但 `jump_to_kmain` 的参数应收缩（D2-04）；可测试性上，mock 空转（D2-08）与缓冲区孪生分叉（D2-06）是两处“看起来测了、实际没测”的缺口，真机 `test-higher-half` 仍是唯一证据（文档 §5.2 已正确声明）。
- 与冻结决策的关系：本报告未改动 D-024 的职责划分，未触碰 D-032/033/040/042/045/046；所有 `[ARCH]` 内容仅出现在 ARCH-PROPOSAL，符合 D-047 的“代裁决边界 = Minix3 语义锚”——A1/A2 都不改变 Minix3 可观测行为，属内部契约重构，但仍需用户裁决后才可实施。
- 置信度自评：D2-01/02/04/05/06/09/13/14 为直接源码事实，高；D2-03 依赖一次越界 grep，中高；D2-11 依据文档复算与测试内容，中；A2 的技术前提（两符号之差是视图不变量）由 riscv64 实现注释中 `la` 的视图行为推出，高，但落地需在三条腿上各验证一次。
- 下一步建议：先做零行为风险的 D2-08、D2-09、D2-13、D2-14（删死测试/死计算/取证/过期引文），再做 D2-04 + D2-01(b) 的签名收缩并同步文档，然后把 D2-05 改成 fail-closed；D2-02/D2-03 与 A1/A2 一起进入判决账，避免两次 wire 版本变更。

