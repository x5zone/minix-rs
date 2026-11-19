# 02-higher-half-kernel · §3/§4 设计审查（P-DESIGN 第二遍）

- 审查对象：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` 的 §3（Rust 设计决策）与 §4（实现详解）。
- 基线：文档主张 ↔ 源码事实。C 源码只作为「C 有没有这个语义」的判据，不重复 §2 的 C 分析。
- 允许的 Rust 实现清单：`/home/xzhao/github/minix-rs/rewrite-notes/coordination/review-eval/runs/01-stage-kernel/02-higher-half-kernel/related-code.txt`。
- D1 按 D-024 停跑；本报告只做 D2 的七维设计审查。
- 未读取：作者解释、历史评审、`panel/round3/` 下其他 `.md`、defect-ledger/loop/metrics、expected/。
- 方法：先按文档 §3.1–§3.5、§4.1–§4.5 的主张逐条抽取，再用 `Read`/`grep` 到源码核对；对每个设计点做「状态表达、身份、所有权、错误、并发、抽象边界、可测试性」七维扫描，再查翻译残留、死抽象、非法状态、最小化机会。
- 限制：未运行构建、未跑 QEMU、未跑 Miri；结论是静态读码与 grep 的产物。`os/kernel/src/globals.rs`、`os/kernel-image/*.ld`、`os/libs/minix-boot/src/handoff_v1_write.rs`、`os/kernel/tests/link_ld_lockstep.rs` 不在 related-code 的 Rust 清单内，但分别承载 F01/F06/F08/F05 的直接源码事实（类型定义、链接布局、线协议断言）；只读取事实，不读取解释性历史文档。
- 同 agent 复核限制：本次审查与代码阅读是同一个模型完成，所有 `path:line` 已用 grep/Read 重放；`consistency` 自评见 §7，低于 1.0 的条目逐条写明「若我错了」。

**裁决口径**（本报告统一使用）：

- `拒绝`：拒绝当前设计，认为应当修改。
- `接受`：接受当前设计，只做文档/注释澄清或不需要动作。
- `需用户裁决`：存在两种以上合理取舍（API 破坏面、线协议版本、构建方式），需要用户拍板。

---

## 0. 结论摘要

| 项 | 数量 | 说明 |
|---|---|---|
| FINDING | 16 | 2 条 P1、9 条 P2、5 条 P3 |
| ARCH-PROPOSAL | 2 | 交接所有权模型、执行视图入契约 |
| 文档事实抽样 | 25 条 | 20 一致、2 部分、3 不符（不符集中在「谁读某字段/谁保证顺序/字段语义」三类声明） |
| 设计骨架判定 | 成立 | 双地址链接、按 `p_paddr` 摆放、trait 化的换栈跳转、页粒度策略分层，四点都有源码支撑 |

最重的两条：

1. `store_kernel_info` 是安全函数，返回 `&'static KernelInfo`，但允许第二次安全调用原地改写同一槽位；`kmain` 正好在持有一份 `&'static` 的同时把它传回去再写一次（F01）。
2. `minix-elf` 的段迭代器把「非 PT_LOAD」「畸形表项」「段数据越界」压成同一种「不产出」，引导、exec、VM bootstrap 三个消费者都得到「段比预期少」而不是错误（F02）。

---

## 1. DIFF-TABLE（文档主张 ↔ 源码事实）

状态列：`一致` = 主张与源码相符；`部分` = 方向对但表述不完整或过强；`不符` = 源码事实与主张相反或读者写错。

| # | 文档主张（位置） | 源码事实（path:line） | 状态 | 关联 |
|---|---|---|---|---|
| 1 | §3.1 第 354 行：x86_64 位置计数器起点是 `KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`，`AT(ADDR(...) - KERNEL_HIGH_BASE)` | `os/kernel-image/x86_64.ld:26-33` | 一致 | — |
| 2 | §3.1 第 354 行：aarch64/riscv64 起点 `KERNEL_VIRT_BASE`，`AT(ADDR(...) - KERNEL_VIRT_BASE + KERNEL_PHYS_BASE)` | `os/kernel-image/aarch64.ld:38-45`、`riscv64.ld:41-48` | 一致 | — |
| 3 | §3.1 第 356 行：`os/kernel/src/arch/*/link.ld` 三份都声明两个基址、没有任何 `AT()`、没有生产链接读者 | `os/kernel/src/arch/x86_64/link.ld:8,16-21`、`aarch64/link.ld:11-16`、`riscv64/link.ld:21-26`；`os/kernel/tests/link_ld_lockstep.rs:1-6` 自述无生产读者 | 一致 | — |
| 4 | §3.1 第 356 行：这些脚本「记录的是预期布局」，常量与生产脚本一致 | 常量一致，但 x86 脚本的 `. = KERN_VIRT_BASE`（`link.ld:21`）与生产首段 VMA `KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`（`x86_64.ld:31`）相差 2 MiB；若真被链接，`p_paddr` 会退化成高半地址 | 部分 | F08 |
| 5 | §3.1 第 388-394 行：`kern_virt_base` 在 x86 取首段 VMA，在 aarch64/riscv 取 `KERNEL_VIRT_BASE`，两种约定不能混用 | `compute_kernel_layout` 取 `min(p_vaddr)`（`os/boot-shim/src/loader.rs:196-198`）；两种约定确实并存 | 一致 | — |
| 6 | §3.1 第 394 行：`entry_point` 的读者是「装载方决定从哪里进入镜像」 | v1 跳转目标是 `report.kern_virt_base`（`os/kernel/src/lib.rs:510-511`）；`entry_point` 只被两条断言用来证明它等于 `kern_virt_base`（`os/boot-shim/src/loader.rs:398-401`、`os/boot-shim/src/uefi_helpers.rs:975-978`） | 不符 | F09 |
| 7 | §3.1 第 398 行：前三个量由 boot-shim helper 从 PT_LOAD 现算；自引导形态是与脚本同源的手写常量 | `compute_kernel_layout`（`loader.rs:175-215`）；`bootface.rs:108-121`、`bootface_a64.rs:76-83` 的手写常量 | 一致 | — |
| 8 | §3.2 第 404-406 行：五个 crate 依赖；四类调用点（boot-shim、boot.rs、trap_api、vm_server），vfs 只用常量 | `grep -rl 'minix-elf ='` 五行；`os/servers/vfs/src/exec_worker.rs:216-242` 自述只解析表头 | 一致 | — |
| 9 | §3.2 第 442 行：`Elf64Ehdr` 丢弃的字段；`p_align` 保留但无读者 | `os/libs/minix-elf/src/lib.rs:88-101`、`108-125`、`150` | 一致 | F13 |
| 10 | §3.2 第 454-496 行：迭代器在溢出、表项越界、非 PT_LOAD、段数据越界时 `continue` | `os/libs/minix-elf/src/lib.rs:352-399` | 一致 | F02 |
| 11 | §3.2 第 498 行：`InvalidSegment` 由四类原因产生（`filesz > memsz`、`paddr + memsz` 溢出、`kern_end - kern_base` 下溢、物理区间重叠） | `loader.rs:189-207,289-291`；`first_overlapping_pair` 在 `348-367` | 一致 | — |
| 12 | §3.2 第 500-511 行：`LoadSegment` 六字段在两个装载场景的去向 | `loader.rs:303-313`（内核：paddr/offset/filesz/memsz）；`os/arch/src/arch/boot.rs:483-500`（用户：vaddr/flags/offset/filesz/memsz） | 一致 | — |
| 13 | §3.2 第 513 行：步进用 `e_phentsize` 而不是结构体大小，解析偏移是 `e_phoff + index * e_phentsize` | `os/libs/minix-elf/src/lib.rs:356-370` | 一致 | — |
| 14 | §3.3 第 519-526 行：`kern_stack_top = kern_virt_base + kern_size`，`kern_size` 用物理侧极值 | `os/boot-shim/src/lib.rs:125`；`loader.rs:196-215` | 一致 | — |
| 15 | §3.3 第 533 行：三个架构的 `.ld` 都在 `.bss` 预留 64 KiB 引导栈，`kernel_boot_stack_top` 与 `__bss_end` 相邻赋值 | `os/kernel-image/x86_64.ld:87-91`、`aarch64.ld:86-90`、`riscv64.ld:94-98` | 一致 | — |
| 16 | §3.3 第 531 行：`syscall_entry` 是参考元数据，验证路径断言它等于 `kern_virt_base`，生产入口用 `syscall_entry_va()` | 字段注释写 `kern_virt_base + offset` 且称 x86 用它装 LSTAR（`os/libs/minix-boot/src/kernel_info.rs:109-117`）；严格校验要求相等（`458-461`）；后置校验只要求落在镜像内（`576-592`）；生产用 `syscall_entry_va()`（`os/kernel/src/lib.rs:2276`） | 不符 | F05 |
| 17 | §3.4 第 558-562 行：trait 方法四条 Safety 前置 + `-> !` | `os/kernel/src/boot/higher_half.rs:31-48` | 一致 | — |
| 18 | §3.4 第 572 行：`stack_top` 参数是可表示的冗余值，两个生产调用点都传 `kinfo.kern_stack_top`，并入 `kinfo` 属 D2-04 | `os/kernel/src/lib.rs:232,375` | 一致 | 已登记 D2-04 |
| 19 | §3.5 第 595-609 行：`plan_kernel_mapping` 先校验能力与布局，再按「首选整除 → 回退整除 → 否则错误」选粒度 | `os/kernel/src/boot_policy.rs:90-140` | 一致 | — |
| 20 | §3.5 第 618-621 行：三架构当前都不满足 1 GiB；2 MiB 档步进等于跨距除以 2 MiB | 基址 `0x200000/0x40200000/0x80200000` 均非 1 GiB 对齐（三份 `.ld`）；跨距 4 MiB | 一致 | F06 |
| 21 | §3.5 第 623-635 行：`ALIGN(0x200000)` 在 `.bss` 括号内，加的是末段 `memsz` | `os/kernel-image/riscv64.ld:105`、`x86_64.ld:98`、`aarch64.ld:98` | 一致 | F06 |
| 22 | §4.1 第 659-668、696-729 行：`build.rs` 按 target 选脚本；`KEEP(*(.text.boot))` 使入口等于首字节；`.bss` 四个普通符号 | `os/kernel-image/build.rs:21-29`；`x86_64.ld:33-37,87-91`；`os/kernel-image/src/main.rs:137-171` | 一致 | — |
| 23 | §4.2 第 870 行：生产路径由 `load_kernel_with_loader` 保证「先 compute 后 copy」的顺序 | UEFI 生产入口自己 `compute_kernel_layout`（`uefi_helpers.rs:970`）、自己断言 entry（`975`）、直接调 `load_segments_into_phys_memory`（`998`）；只有 OpenSBI 腿用 `load_kernel_with_loader_gated`（`opensbi_helpers.rs:370`） | 不符 | F03 |
| 24 | §4.4 第 1064 行：生产 `kmain` 第一条语句把形参遮蔽为 `.bss` 副本 | 遮蔽在 `os/kernel/src/lib.rs:1688`；随后 `1738` 又把这份 `&'static` 传回 `store_kernel_info`，后者在 `1561` 原地写回同一槽位 | 部分 | F01 |
| 25 | §4.4 第 1064 行附注：全仓唯一把 `syscall_entry` 当入口传入 `configure_syscall` 的是 `test-protection-riscv64` 载体 | `grep -rn configure_syscall os/qemu-tests`：该载体用 `info.syscall_entry`，其余用 `minix_arch::syscall_entry_va()` | 一致 | — |

一致性统计：25 条中 20 条一致、2 条部分、3 条不符；按「部分计半分」84%，严格计 80%。这个抽样偏向「读者/消费者声明」，不是均匀随机抽样，不能当作全文错误率；但三条不符都属于「文档替源码说了一个更强的保证」，会误导后续维护，必须修。

---

## 2. 七维对照

| 维度 | 主要 FINDING | 判断 |
|---|---|---|
| 状态表达 | F05、F06、F09、F16 | 弱。交接结构体里混着「观测事实」与「派生值/常量」，派生的那部分用多套校验器表达同一语义 |
| 身份 | F09、F10、F12 | 中。`entry_point` 的名字与读者不符；模块名把物理地址伪装成 `&'static str`；`BootArch` 与 `HigherHalf` 的类型对没有绑定 |
| 所有权 | F01、F10 | 弱。两处「伪造 `'static`」：一处是可变全局里的 `&'static`，一处是物理名字池上的 `&'static str` |
| 错误 | F02、F03、F04、F14 | 弱。核心装载器把「畸形」当「缺段」；有一个安全包装器被生产腿绕过；测试孪生的返回值不能表达失败 |
| 并发 | F01、F11 | 中。引导阶段单 CPU 是注释与调用顺序维持的，安全 API 在 SMP 之后仍可调用 |
| 抽象边界 | F03、F10、F12 | 中。解析/校验/拷贝三步的边界没有类型承载；PA/VA 的视图差异漏进 `&str` |
| 可测试性 | F04、F15、F07 | 中。生产与测试是两份略有差异的循环；mock 只证明 trait 可命名；统一平移只在宿主脚本里检查 |

---

## 3. FINDINGS

## FINDING RD3-DESIGN-D2-01（P1）

- **位置**：`os/kernel/src/lib.rs:1453-1563`（`store_kernel_info`）、`os/kernel/src/lib.rs:1688`（kmain 取 `&'static`）、`os/kernel/src/lib.rs:1738`（kmain 把同一引用传回 store）、`os/kernel/src/lib.rs:3673-3677`（`kernel_info()`）、`os/kernel/src/globals.rs:277`（槽位定义）。
- **严重度**：P1。
- **分类**：[MODERN]（C 有一个全局 `kinfo`，Rust 应当用一次性写入/冻结来表达，而不是安全函数返回 `&'static` 再允许原地改写）。
- **问题**：`store_kernel_info` 是安全函数，返回指向 `KERNEL_INFO` 内部的 `&'static KernelInfo`；它没有 one-shot 标志、没有冻结点。`kmain` 先在 1688 行用 `kernel_info()` 取得指向该槽位的 `&'static`，再在 1738 行把这份引用传回 `store_kernel_info`；后者在 1561 行执行 `*KERNEL_INFO.get() = Some(k)`，原地改写同一槽位。函数自己的文档注释（1449-1452）写明「A later call updates the slot in place, so callers must not hold the reference across a second store」，而 `kmain` 正好是那个调用者。
- **影响**：按 Rust 的别名模型，一个 `&'static KernelInfo` 在其生命周期内被原地改写，是典型的共享引用失效；即使当前编译器没有把某次读取排到写之后，这仍然是「安全函数可以制造悬垂共享引用」的契约漏洞。两条通用生产入口在 `arch_boot_finish:225` / `resume_additive:352` 已经存过一次，`kmain` 的第二次写入在语义上是冗余的；真正需要它的是直接调用 `arch_boot_impl` 的测试载体（`test-rt-birth`、`test-sysboot`），它们不经过 `kmain`。
- **证据或推理**：`lib.rs:1453` 的签名返回 `&'static`；`lib.rs:1561` 是唯一发布点；`lib.rs:1688` 的 `kernel_info()` 与 `lib.rs:3673` 的 `Option<&'static KernelInfo>` 证明引用来自同一槽位；`lib.rs:1738` 传入的正是这份引用。`land_slice` 的指针相等守卫（1432）只跳过元素拷贝，不能阻止 1561 行的整体写入。
- **最小修复**：三选一，推荐前两条同时做：
  1. 让 `store_kernel_info` 幂等：若 `kernel_info` 的地址等于 `KERNEL_INFO` 当前载荷地址，直接返回现有引用，不写 1561 行；
  2. 拆成 `install_kernel_info(&KernelInfo) -> ()`（带 one-shot 标志，模式照 `BootStorageCell::take` 的 `AtomicBool`）与只读的 `kernel_info()`；写入只发生在引导入口；
  3. 删除 `kmain` 的第二次调用（生产已存；测试载体自己存），把 1738 行改成直接使用 1688 行的引用。
- **置信度**：高（路径、指针来源、写入点逐行确认）；「是否构成 UB」依赖 Rust 别名模型的严格程度，保守地按契约缺陷计 P1。
- **是否需源码核实**：否（已核实）。建议补一个宿主测试：`store_kernel_info(&a)` → `kernel_info()` → `store_kernel_info(取回的引用)`，在 Miri/Stacked Borrows 下跑一次。
- **若我错了**：若 `kernel_info()` 返回的引用在 1738 行之前已经不被编译器视为活跃，且「第二次 store 只写同一个值」被证明不破坏 validity，则 UB 降级为 P3 的 API 契约问题；但「安全函数可被二次调用改写 `'static` 引用」这一事实仍成立，仍建议删除 `kmain` 的重复调用。
- **裁决**：拒绝（当前实现应改）。

## FINDING RD3-DESIGN-D2-02（P1）

- **位置**：`os/libs/minix-elf/src/lib.rs:352-399`（`Iterator for SegmentIter`，越界跳过在 379-386）；`os/boot-shim/src/loader.rs:175-215`；`os/servers/rs/src/trap_api.rs:443-450`；`os/servers/vm/src/vm_server.rs:890-893`；`os/arch/src/arch/boot.rs:461-464`。
- **严重度**：P1。
- **分类**：[MODERN]（C 侧把畸形镜像的拒绝交给外部装载器；Rust 自建解析器换了现代错误枚举，却把「畸形」丢进了过滤路径）。
- **问题**：`SegmentIter::next` 把三种情况都变成「不产出」：非 `PT_LOAD`（正常过滤）、表项算术/解析失败（畸形）、`p_offset + p_filesz` 超出镜像（截断或损坏）。调用方看到的只是「段比预期少」，无法区分「这个 ELF 本来没有这个段」和「这个段坏了」。
- **影响**：引导路径上，`compute_kernel_layout` 会算出偏小的 `kern_size`，`load_segments_into_phys_memory` 会少拷一段，内核仍然启动；`rs` 的 exec 路径会少拷一段代码/数据后继续 exec（`copied_any` 只挡住「一段都没有」）；VM 的 bootstrap 装载会拿到偏小的段表。文档 §3.2 第 496 行把这件事登记为 D2-05，但只限定在「引导路径」；同一个迭代器同时服务 exec 与 VM，安全影响比文档写的更大。
- **证据或推理**：`lib.rs:379-386` 的 `seg_end > self.image.len()` 分支直接 `continue`；`loader.rs:184-199` 的循环只统计收到的段；`trap_api.rs:443` 用 `for seg in segment_iter(exec)` 且不检查段数；`vm_server.rs:892` 把迭代器 `collect()` 成 `Vec`。文档 §3.2 第 496 行自述「调用方看到的是段比预期少，而不是段坏了」。
- **最小修复**：把迭代器的 `Item` 改成 `Result<LoadSegment, ElfError>`（或新增 `try_segment_iter`，旧 `segment_iter` 标注 deprecated），非 `PT_LOAD` 继续跳过，畸形返回 `Err`；`NoLoadSegments` 在消费端统计 `Ok` 数。三个消费者按 fail-closed 处理：boot 转成 `Err`/panic，exec 转 `ENOEXEC`，VM 拒绝 boot image。
- **置信度**：高（代码路径与三个消费者已实读）。
- **是否需源码核实**：否；建议补一个「中间一个 PT_LOAD 的 `p_offset + p_filesz` 越界」的构造 ELF，断言 boot、exec、VM 三个消费者都拒绝而不是少段启动。
- **若我错了**：若产品策略明确允许「损坏 ELF 尽力启动」（例如救援镜像），boot 侧可以保留 skip；但 exec/VM 侧仍应 fail-closed。此时应把策略做成显式参数（`OnMalformed::Skip/Reject`），而不是让迭代器替所有消费者决定。
- **裁决**：需用户裁决（是破坏 `minix-elf` 的公共 API 换成 `Result`，还是新增 `try_segment_iter` 并逐步迁移；我建议后者作为过渡，最终统一到 `Result`）。

## FINDING RD3-DESIGN-D2-03（P2）

- **位置**：`os/boot-shim/src/loader.rs:286-316`（`pub unsafe fn load_segments_into_phys_memory`，Safety 契约在 263-285）、`os/boot-shim/src/loader.rs:374-407`（`load_kernel_with_loader_gated`）、`os/boot-shim/src/uefi_helpers.rs:970-999`（UEFI 生产腿绕过 wrapper）、`os/boot-shim/src/opensbi_helpers.rs:370`（唯一使用 wrapper 的生产腿）。
- **严重度**：P2。
- **分类**：[MODERN]（C 侧装载器没有「先校验后拷贝」的 Rust 式前置条件；Rust 应当把这个前置条件放进类型，而不是放进文档）。
- **问题**：`load_segments_into_phys_memory` 的 Safety 契约要求「调用者必须先跑 `compute_kernel_layout`（或等价校验）」，因为它自己不拒绝 `filesz > memsz` 或地址溢出；这个前置条件没有类型载体。生产 UEFI 腿在 `uefi_helpers.rs:970` 自己 compute、975 自己断言 entry、998 直接调这个 unsafe 函数，绕过了专门为「先校验后拷贝」写的 `load_kernel_with_loader_gated`。文档 §4.2 第 870 行说「生产路径由 `load_kernel_with_loader` 保证这个顺序」，与源码不符（该 wrapper 只被 OpenSBI 腿使用）。
- **影响**：顺序完全靠 UEFI 调用点的书写纪律维持；这条路径本来就在 compute 与 copy 之间做 `reserve_kernel_span`/`alloc_root_page`/`alloc_bump_region`，未来任何重排都可能先 copy 后 validate。一旦发生，`filesz > memsz` 会让「拷贝长度」大于「声明区间」，把字节写到 `paddr + memsz` 之外；地址溢出则依赖 release 下的回绕。这个函数是 `pub unsafe`，但「必须配一个看起来该用、实际没用的 wrapper」是最坏的 API 形状。
- **证据或推理**：`loader.rs:275-278` 的文档自述「this function itself does not reject `filesz > memsz` or address arithmetic overflow」；`loader.rs:396-401` 的 entry 断言与 `uefi_helpers.rs:975-978` 是同一条件的第二份拷贝；`grep load_kernel_with_loader` 只有 `opensbi_helpers.rs:370` 一个生产调用。
- **最小修复**：让 `compute_kernel_layout` 返回不透明的 `ValidatedKernelImage<'a>`（含 `KernelLoadResult` 与已解析段表，或至少含一个只能由它构造的 token），`load_segments_into_phys_memory` 只接受这个类型；UEFI 腿用 `ValidatedKernelImage::layout()` 做 reserve/alloc，不再二次解析。若不想动大 API，至少把 UEFI 腿改成 `load_kernel_with_loader_gated`，并在 gate 里做 `reserve_kernel_span`/`record_kernel_span`（需要把 wrapper 的 gate 时机调到「compute 之后、copy 之前、alloc 之后」），同时修正 §4.2 的表述。
- **置信度**：高（UEFI 直调与 wrapper 使用面已 grep）。
- **是否需源码核实**：否。若要确认重排风险，可查看 `uefi_helpers.rs:970-998` 之间所有分配点及其错误路径。
- **若我错了**：若 `prepare_and_enter_kernel_image` 不是生产入口（只被测试调用），则「生产路径」的表述成立；但 `os/boot-shim/src/main.rs` 会走它，所以不成立。若 wrapper 的 gate 语义本来就不打算覆盖 UEFI，那至少 §4.2 必须改，finding 降为 P3 文档项。
- **裁决**：拒绝（当前 API 边界应改；实现方式二选一）。

## FINDING RD3-DESIGN-D2-04（P2）

- **位置**：`os/boot-shim/src/loader.rs:221-260`（`load_segments_into_buffer`，`count += 1` 在 258）、`os/boot-shim/src/loader.rs:286-316`（生产版）、`os/boot-shim/src/loader.rs:786`（测试调用）。
- **严重度**：P2。
- **分类**：[MODERN]（同一份「按段拷贝」语义应当只有一份实现；测试替身不应当有独立的行为）。
- **问题**：`load_segments_into_buffer` 的文档说「Returns the number of segments loaded」，但 246-252 行在 `file_end > buf.len()` 时跳过 copy，258 行仍然 `count += 1`；`seg.paddr < buf_base_paddr` 的段在 235 行 `continue`，不计入。于是返回值既不是「看到的段数」也不是「装载成功的段数」。同时，缓冲版没有物理版的段间重叠检测，物理版没有缓冲版的目标长度上界；两版不是同一个循环的两个后端。
- **影响**：这是文档 §4.2/§5.1 指定的「可测的孪生」，但测试通过不能证明生产路径的校验行为；`count` 的失真让「一个段因越界被跳过」在测试里也看不出来。可测试性维度上，这是「测试代码与生产代码各说各话」的典型。
- **证据或推理**：`loader.rs:246-256` 的 `if file_end <= buf.len()` 与 254 行的 `if mem_end <= buf.len() && ...`；258 行无条件计数；`loader.rs:235` 的 `continue` 在计数之前；文档 §4.2 第 870 行自述两版差异是「可测试性缺口」。
- **最小修复**：抽出一个 `copy_segment(seg, sink) -> Result<(), SegmentError>` 内核，`sink` 可以是「物理地址写」或「缓冲区写」；两版共用段间重叠检测与长度校验。缓冲版返回 `Result<usize, SegmentError>`，只在 copy 与 zero-fill 都完成后计数，越界返回带 `seg_index` 的错误。若短期不改结构，至少把返回值改名为 `count_pt_load_segments_seen` 并删除「loaded」措辞，同时补一个「部分段越界」的测试。
- **置信度**：高（计数路径逐行确认）。
- **是否需源码核实**：否；建议补测试：`buf` 只能容纳第一段，第二段 `paddr + memsz` 越界，断言返回 `Err` 而不是 `2`。
- **若我错了**：若这个函数只被测试用来数「遍历到的 PT_LOAD 数」，那它不应该做拷贝，也不应该叫 `load_...`；当前名字、文档、行为三者不一致，仍需修。
- **裁决**：拒绝（当前 API 应改）。

## FINDING RD3-DESIGN-D2-05（P2）

- **位置**：`os/libs/minix-boot/src/kernel_info.rs:107-117`（字段与注释）、`:458-470`（严格校验）、`:576-592`（后置校验）、`os/libs/minix-boot/src/handoff_v1_write.rs:142-147`（写入侧断言）、`os/kernel/src/lib.rs:2276`（生产消费者）、`os/qemu-tests/.../test-protection-riscv64/src/main.rs:148`（唯一字段消费者）。
- **严重度**：P2。
- **分类**：[MODERN]（C 的 `kinfo` 有这些字段；Rust 应当把它们表达成「由布局派生」或「由架构提供」，而不是让同一个字段带三套规格）。
- **问题**：`syscall_entry` 有三套互不相同的规格：
  1. 字段注释说 `= kern_virt_base + offset`，并且「only x86-64 uses it to configure LSTAR MSR」；
  2. `validate_requiring_sources` 在 `458-461` 要求它严格等于 `kern_virt_base`（offset 恒 0）；
  3. `check_boot_postconditions` 在 `576-592` 只要求它落在 `[kern_virt_base, kern_virt_base + kern_size)` 内。

  写入侧 `handoff_v1_write.rs:143` 又 debug_assert 相等。生产消费者根本不读它：`lib.rs:2276` 用 `minix_arch::syscall_entry_va()`；唯一把它当入口传入 `configure_syscall` 的消费者是 `test-protection-riscv64` 载体（文档 §3.3 自述，grep 确认）。`user_sp` 是同类问题：字段 + 线协议 + 严格校验 `== USER_STACK_TOP`，生产消费者读到的值始终等于这个常量，携带它只是多一份可被伪造的副本。
- **影响**：同一个字段在不同层有不同含义，测试载体用 `base + 0x100000` 能过 lenient 门（`test-higher-half/src/main.rs:47`）但过不了 strict 门，于是「测试认为合法的状态」和「生产认为合法的状态」不同。wire 携带可推导常量，伪造值只在 strict 门被挡；字段注释还声称 x86 用它装 LSTAR，与 `lib.rs:2276` 的注释（「KernelInfo.syscall_entry is reference-only boot metadata」）直接冲突。
- **证据或推理**：`kernel_info.rs:109-117` 的注释与 `458-461`、`576-592` 三处；`handoff_v1_write.rs:143`；`lib.rs:2273-2276`；`test-higher-half/src/main.rs:47` 的 offset 值；`grep config_syscall` 的唯一字段消费者。
- **最小修复**：按 D2-03 从 wire 与 `KernelInfo` 删除 `syscall_entry`；`user_sp` 改为消费端从 `minix_types::USER_STACK_TOP` 派生（KERNINFO 页需要时直接填常量）；若因 C 兼容必须保留字段，则统一为一条规格（`== kern_virt_base`），删掉「+offset」与「x86 用它装 LSTAR」的说法，并把 `check_boot_postconditions` 的区间检查改成等式检查。
- **置信度**：高（三处规格与消费者已核实）。
- **是否需源码核实**：否。若线协议 v2 的删除计划已经覆盖 `syscall_entry`，只需把 `user_sp` 与三套规格一并纳入。
- **若我错了**：若存在一个尚未合入的 x86 消费者确实需要 `kern_virt_base + offset` 来装 LSTAR，那么 offset 语义是有意的；但当前源码没有这个消费者，且 strict 门会把 offset 拒掉，所以三者至少有一个必须改。若维护者决定保留字段作 C 对照，也应删掉与源码矛盾的注释。
- **裁决**：需用户裁决（wire v2 的删除范围；我建议删除 `syscall_entry`，`user_sp` 至少改成派生）。

## FINDING RD3-DESIGN-D2-06（P2）

- **位置**：`os/kernel-image/src/bootface.rs:113-121`（`KERN_SIZE = 0x40_0000`）、`os/kernel-image/src/bootface_a64.rs:81-83`（同一常量）、`os/kernel-image/aarch64.ld:109-117`（只有上界 `ASSERT`）、`os/kernel-image/riscv64.ld:117-125` 与 `os/kernel-image/x86_64.ld:98`（2 MiB 收口）。
- **严重度**：P2。
- **分类**：[MODERN]（C 的 `_kern_size` 是链接符号，由链接器派生；Rust 自引导面把它手写成常量，应当由构建产物派生或至少用等值断言锁住）。
- **问题**：`KERN_SIZE` 是链接产物的事实（`__bss_end` 经 `ALIGN(0x200000)` 收口后的跨距），却以四处手写常量/注释存在：两份 `.ld` 的注释与断言、两个 bootface 的常量。aarch64 只有 `ASSERT(__bss_end - KERNEL_VIRT_BASE <= 0x400000)` 的上界，没有等值；riscv 的等值对账在宿主 `check-layout.sh` 的 L9（文档 §3.5 自述），设备端不检查。由于 `ALIGN(0x200000)` 使跨距只能是 2 MiB 的整数倍，镜像合法缩到 2 MiB 时上界断言仍然通过，而常量还是 4 MiB。
- **影响**：高估的 `KERN_SIZE` 会让自引导面把 `[base+2MiB, base+4MiB)` 当成内核镜像：`startup_record` 多预留 2 MiB 可用内存，`kern_stack_top` 落在镜像尾之外 2 MiB，映射循环多映射 2 MiB；如果那块物理内存属于 bump 池、模块区或固件保留区，第一次压栈就可能写进别人的页。低估则截断镜像。两种都是静默错误，直到运行期才以页错误或数据损坏出现。
- **证据或推理**：`aarch64.ld:116-117` 的断言只比较 `<= 0x400000`；`bootface.rs:121` 与 `bootface_a64.rs:83` 都写 4 MiB；`bootface.rs:113-120` 的注释自称「single source of truth」，但 `.ld` 才是真正的源；`riscv64.ld:124-125` 同样只有上界。
- **最小修复**：最小改动是给两份 `.ld` 加「精确档位」断言：既然收口后跨距是 2 MiB 的整数倍，而当前常量声明 4 MiB，就断言 `__bss_end - KERNEL_VIRT_BASE > 0x200000`（强制 4 MiB 档）；镜像缩到 2 MiB 时链接期失败，迫使同步改常量。更彻底的做法是让 `build.rs` 从链接脚本或 `readelf` 生成 `KERN_SIZE`（`include!(concat!(env!("OUT_DIR"), ...))`），或让 `.ld` 用 absolute symbol 导出跨距、Rust 通过 `extern` 读取（文档 §3.5 说 symbol section 试过会破坏 2 MiB 收口，absolute symbol 不占段，值得重试）。
- **置信度**：高（断言与常量已实读）；「镜像缩到 2 MiB」是合法状态推断，未构造工件验证。
- **是否需源码核实**：是——建议用 `readelf -lW` 核对当前 aarch64/riscv 工件的 `p_memsz` 跨度是否恰为 `0x400000`，并人为缩短一个段验证上界断言仍绿。
- **若我错了**：若 `.ld` 的 `ASSERT` 实际比较的是收口后的位置计数器而不是收口前赋值的 `__bss_end`（我读到的是 `__bss_end = .` 在 `ALIGN` 之前，断言用 `__bss_end`），或镜像存在其它下限断言使其不可能小于 2 MiB，则高估路径不成立；即便如此，「多处手写常量 + 只有一处等值对账」仍是单源问题。
- **裁决**：需用户裁决（生成式 vs 断言式；我建议先加上界→精确档断言，成本最低）。

## FINDING RD3-DESIGN-D2-07（P2）

- **位置**：`os/boot-shim/src/loader.rs:175-215`（`compute_kernel_layout` 取极值，无平移一致性检查）、`os/boot-shim/src/loader.rs:303-313`（按 `paddr` 拷贝）、文档 §3.1 第 354 行（不变量）与 §3.5 第 635 行（映射以 `kern_size` 为上界）。
- **严重度**：P2。
- **分类**：[MODERN]（C 的装载器与分页代码默认链接脚本给出统一平移；Rust 自建解析器应当在唯一入口校验它，而不是只靠宿主 L4）。
- **问题**：`compute_kernel_layout` 分别取 `min(p_vaddr)`、`min(p_paddr)`、`max(p_paddr + p_memsz)`，但不校验所有段的 `vaddr - paddr` 相等。内核映射按「统一平移」把 `[kern_virt_base, +kern_size)` 指到 `[kern_phys_base, +kern_size)`；L4 只在宿主 `check-layout.sh` 检查生产工件。
- **影响**：一个混合平移的 ELF（链接脚本写错、工具链改布局、手工构造）能通过 `compute_kernel_layout` 与重叠检测，按 `paddr` 正常拷贝，但高半映射把某些段映到错误的虚拟页；症状是跳进 `kmain` 后某个段的内容错位或取指错误，根因在 loader 没有拒绝这个输入。
- **证据或推理**：`loader.rs:196-198` 三个独立极值；`loader.rs:205-207` 的 `kern_size` 只用物理侧；`loader.rs:303-313` 只按 `paddr` 拷贝；文档 §3.1 第 354 行明确要求「所有段的虚拟地址 − 装载地址相等」，但只说 L4 检查，未说运行时拒绝。
- **最小修复**：在循环里记录第一段的 `vaddr.checked_sub(paddr)`（或 `paddr.checked_sub(vaddr)`，按架构约定取符号一致的方向），后续段不等就返回新错误 `InconsistentSegmentTranslation`（或 `InvalidSegment` 加段号）；`compute_kernel_layout` 是唯一的布局入口，代价是一次减法。
- **置信度**：高（逻辑推理与代码路径）；没有真实坏工件，未做运行验证。
- **是否需源码核实**：否；建议加一个「两段 `vaddr - paddr` 不同」的构造 ELF 测试，断言 `Err`。
- **若我错了**：若 `vaddr` 与 `paddr` 的差在链接脚本里本来就允许逐段不同（例如某些段故意放在不同物理区），那「统一平移」的映射前提本身不成立，问题更大；当前文档明确要求平移一致，所以这个反例不成立。
- **裁决**：拒绝（应补校验）。

## FINDING RD3-DESIGN-D2-08（P2）

- **位置**：`os/kernel/src/arch/x86_64/link.ld:4-21`（`KERN_VIRT_BASE = 0xFFFF800000000000`、`. = KERN_VIRT_BASE`、无 `AT()`）、`os/kernel-image/x86_64.ld:26-33`（生产首段 VMA 是 `KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`）、`os/kernel/tests/link_ld_lockstep.rs:45-75`（只锁 `KERN_VIRT_BASE`/`KERN_PHYS_BASE` 两个常量）。
- **严重度**：P2。
- **分类**：[DEL]（C 只有 `kernel.lds` 一套链接脚本；这三份无读者的脚本是 Rust 早期形态的残留，应当删除或改造成真正可链接的脚本）。
- **问题**：文档 §3.1 第 356 行把 `os/kernel/src/arch/*/link.ld` 描述为「记录预期布局」；但以 x86 为例，脚本把位置计数器放在 `KERN_VIRT_BASE`，生产脚本放在 `KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`，两者相差 2 MiB；脚本又没有 `AT()`，一旦被当作链接输入，所有 `p_paddr` 会退化成高半地址，`load_segments_into_phys_memory` 会向 `0xFFFF8000...` 写字节。`link_ld_lockstep.rs` 只比较两个基址常量，无法发现这个 2 MiB 差异，给读者一种「两份脚本一致」的假保证。
- **影响**：三份脚本 + 一个锁步测试构成一个无生产读者的维护面；未来有人「看起来有现成的 link.ld」把它接进构建，会得到一个与生产约定不同的 VMA/LMA 镜像。测试通过反而会强化错误判断。
- **证据或推理**：`os/kernel/src/arch/x86_64/link.ld:16-21` 的 `KERN_VIRT_BASE = 0xFFFF800000000000` 与 `. = KERN_VIRT_BASE`；`os/kernel-image/x86_64.ld:31` 的 `. = KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`；`link_ld_lockstep.rs:45-75` 只比较 `KERN_VIRT_BASE` 与 `KERNEL_HIGH_BASE`、`KERN_PHYS_BASE` 与 `KERNEL_PHYS_BASE`。
- **最小修复**：删除三份脚本与锁步测试；若确实想把「预期布局」留作文档，把它们改成不可链接的注释文件（例如 `.md` 或 `.ld.in`）或在测试里真正跑一次 `ld -T` 并断言生成的 `p_paddr`，后者能捕获 2 MiB 差异。文档 §3.1 第 356 行相应改写，去掉「预期布局」的歧义。
- **置信度**：高（脚本与测试实读）。
- **是否需源码核实**：否；若要确认「没有生产读者」，可复跑文档给出的 `grep -rn 'src/arch/.*link.ld'`。
- **若我错了**：若这三份脚本是某个未入库的构建路径（例如外部工具）的输入，删除会破坏它；但仓库内检索不到读者，且测试自述「no production link reader」。若维护者决定保留，至少应把锁步测试升级为「链接一次并检查 LMA」。
- **裁决**：需用户裁决（删除 vs 改造为真正的链接测试）。

## FINDING RD3-DESIGN-D2-09（P2）

- **位置**：`os/boot-shim/src/loader.rs:73-84`（`KernelLoadResult.entry_point: u64`）、`:398-401`（entry 断言）、`os/kernel/src/lib.rs:510-511`（v1 实际跳转目标）、`os/boot-shim/src/uefi_helpers.rs:975-978`（第二份 entry 断言）；文档 §3.1 第 394 行。
- **严重度**：P2。
- **分类**：[MODERN]（C 的 GRUB 用 `e_entry` 进入镜像；Rust 改成「进入首个 PT_LOAD 的 VMA」，这个更安全的规则本身可以保留，但字段与文档必须表达真实规则）。
- **问题**：文档 §3.1 的字段表说 `entry_point` 的读者是「装载方决定从哪里进入镜像」；源码里 v1 装载的跳转目标是 `report.kern_virt_base`（`lib.rs:510-511`），`entry_point` 只被两条断言用来证明它等于 `kern_virt_base`。也就是说，真正的入口规则是「`min(p_vaddr)`」，`e_entry` 是被校验的冗余值，不是跳转依据。此外 `entry_point` 是 `u64`，而同表的 `kern_virt_base` 是 `VirBytes`，地址身份标注不一致。
- **影响**：读者会以为改 `ENTRY(_start)` 就能改进入点；实际上 loader 会先断言 `e_entry == min(p_vaddr)`，失败即 panic。字段 `entry_point` 在 `KernelLoadResult` 里也没有第二个读者，属于可删除的冗余状态；保留却写错读者，会误导后续维护。
- **证据或推理**：`loader.rs:398-401` 与 `uefi_helpers.rs:975-978` 的同一断言；`lib.rs:510` 的 `black_box(report.kern_virt_base.0)`；`lib.rs:546-577` 的 `jump_kernel_image_v1(entry, arena)`；文档第 394 行。
- **最小修复**：二选一：
  1. 保留「跳 `min(p_vaddr)`」的规则：删除 `KernelLoadResult.entry_point` 字段，把断言移进 `compute_kernel_layout`（`e_entry == kern_virt_base` 不成立直接 `Err`），文档改成「入口规则是首个 PT_LOAD 的 VMA；`e_entry` 只作一致性断言」；
  2. 改成真正的 `e_entry` 入口：跳转用 `layout.entry_point`，去掉断言；但这会让「`ENTRY` 写到非首段」的镜像跳到别处，需要额外的安全论证。
- **置信度**：高（读者与跳转目标已实读）。
- **是否需源码核实**：否。
- **若我错了**：若 `report.kern_virt_base` 在发送侧被赋成了 `entry_point` 而不是 `layout.kern_virt_base`，那跳转目标就是 entry；但 `uefi_helpers.rs:1064`/`opensbi_helpers.rs:449` 的 `storage.report(kern.kern_virt_base, ...)` 明确传的是 `kern_virt_base`，所以不成立。
- **裁决**：接受（保留「跳首个 PT_LOAD VMA」的设计，它比 `e_entry` 更安全）；但拒绝文档表述，且建议删除 `entry_point` 字段。

## FINDING RD3-DESIGN-D2-10（P2）

- **位置**：`os/kernel-image/src/bootface_common.rs:254-273`（`place_modules` 用 `from_raw_parts`/`from_utf8_unchecked` 从**物理地址**制造 `&'static str`）、`os/libs/minix-boot/src/kernel_info.rs:691-695`（`BootModule.name: &'static str`）、`os/kernel/src/lib.rs:1484-1523`（`store_kernel_info` 复制名字到 `.bss` 名字池）。
- **严重度**：P2。
- **分类**：[MODERN]（C 的 `module_list[]` 存的是原始地址；Rust 应当用 `PhysBytes` + 长度表达「物理视图里的名字」，进入内核 `.bss` 后再构造 `&str`）。
- **问题**：自引导面把名字字节拷进一页物理名字池，然后执行：

  ```rust
  let landed: &'static str = unsafe {
      core::str::from_utf8_unchecked(core::slice::from_raw_parts(slot as *const u8, n))
  };
  ```

  这个 `&'static str` 的指向是物理地址：在 satp=0 视图里它是可读的字节，在开启高半映射后同一个数字是另一个虚拟地址。`'static` 是伪造的，实际生命周期只到「视图切换前」；唯一的保护是 `store_kernel_info` 在跳转前把名字复制进内核 `.bss`，而这个顺序没有任何类型或代码检查（`arch_boot_finish` 的 store 在 `arch_boot_impl` 之前，是调用顺序与源码顺序测试维持的约定，不是类型契约）。
- **影响**：任何在视图切换后解引用 `BootModule.name` 的新消费者都会读到未映射地址或别的内容；任何调整 `store_kernel_info` 与 `arch_boot_impl` 顺序的重构都会让自引导面在拷贝名字时页错误。名字又恰好是内核严格门用来做位置身份校验的字段（`kernel_info.rs:471-481`），坏掉的后果不是显示问题，而是拒绝启动或错配进程。
- **证据或推理**：`bootface_common.rs:264-268` 的 `from_raw_parts(slot as *const u8, n)`；`kernel_info.rs:692` 的 `&'static str`；`lib.rs:1493` 的 `module.name.as_bytes()`（复制发生在 store 时）；`bootface.rs:41-51` 的模块文档自述「every address value Rust obtains is a physical address」。
- **最小修复**：把 `BootModule.name` 改成 `BootName { pa: PhysBytes, len: u8 }`（或 `name_pa: PhysBytes` + `name_len`），生产者只填物理地址与长度；消费者在 `store_kernel_info` 里把字节复制进 `.bss` 名字池后，从池上的 `static` 构造 `&'static str`。如果暂时不改公共类型，至少在 `place_modules` 里不要制造 `&'static str`，改为 `&'static [u8]` 或 `PhysBytes`，并在 `BootModule` 上写清「名字只在物理视图有效」。
- **置信度**：高（伪 `'static` 与复制顺序已逐行确认）。
- **是否需源码核实**：否；建议在 QEMU 上把 `store_kernel_info` 移到 `arch_boot_impl` 之后做一次负向实验，确认会页错误（验证顺序确实是承重的）。
- **若我错了**：若 `slot` 恰好也是一个在高半视图下有效的虚拟地址（例如名字池被同时映射到两个视图），则解引用不会坏；但 `bootface.rs:41-51` 明确说自引导面运行在「link-time VMA − runtime PA = delta」的物理视图，名字池只有物理地址语义，所以不成立。
- **裁决**：需用户裁决（改公共类型 vs 先加视图标注；我建议纳入 ARCH-A01 的 arena 统一处理）。

## FINDING RD3-DESIGN-D2-11（P2）

- **位置**：`os/kernel/src/lib.rs:1453`（`pub fn store_kernel_info`）、`os/kernel/src/lib.rs:3673-3677`（`kernel_info()`）、`os/kernel/src/globals.rs:277`（`KERNEL_INFO`）、`os/kernel/src/globals.rs:48-56`（`SyncUnsafeCell` 的 `BklProtected` 契约）、`os/kernel-image/src/bootface_a64.rs:111-130`（`static mut BUMP_PTR` + 单 CPU 注释）。
- **严重度**：P2。
- **分类**：[MODERN]（C 的引导全局是单线程约定；Rust 应当用「引导阶段令牌」或一次性冻结把约定变成类型/状态，而不是注释）。
- **问题**：引导阶段的「单 CPU、只写一次」是注释与调用顺序维持的。`store_kernel_info` 是 `pub` 安全函数，任何内核模块在 SMP 启动之后仍可调用它并安全地改写 `KERNEL_INFO`；`kernel_info()` 返回 `&'static` 且只要求「调用者持有 BKL」；`bootface_a64` 的 `BUMP_PTR` 是 `static mut`，单线程假设只写在注释里。代码库已经有更好的模式（`BootStorageCell::take` 用 `AtomicBool` 做 one-shot），但 `KERNEL_INFO` 没有采用。
- **影响**：Kernel = SMP + BKL 的模型下，一个晚到的 `store_kernel_info` 会在其他 CPU 读 `KERNEL_INFO` 时产生数据竞争（`SyncUnsafeCell` 的 `Sync` 契约假定「写一次后只读」）。当前没有晚到调用，但安全 API 没有阻止它；这属于「非法状态可表示」。
- **证据或推理**：`lib.rs:1453` 的 `pub` 可见性；`globals.rs:48-56` 的 `BklProtected` 注释「write-once-read-only after boot」；`lib.rs:3674-3675` 的「Caller is responsible for BKL synchronization」；`bootface_a64.rs:117-119` 的 SAFETY 注释依赖「single-threaded boot face」。
- **最小修复**：引入一个零大小的 `BootCpu` 令牌（只在 `arch_boot`/`arch_boot_resume_*` 入口构造），`store_kernel_info(&BootCpu, &KernelInfo)` 与 `install_kernel_info` 需要它；`kernel_info()` 保持只读。若不想改签名，把 `store_kernel_info` 改成 `pub(crate)` 并加 one-shot `AtomicBool`（第二次调用 panic），同时把 `KERNEL_INFO` 的发布改成 `Once`/`OnceCell` 语义。
- **置信度**：高（可见性与契约注释已核实）；「当前没有晚到调用」是 grep 结论。
- **是否需源码核实**：否；若要确认风险面，可 grep 所有 `store_kernel_info` 调用点（生产 2 处、测试载体 2 处、kmain 1 处）。
- **若我错了**：若 `minix_kernel` 的 `pub` 只在 crate 内可见（Rust 的 `pub` 在 crate 根就是对外），或有编译期 lint 禁止外部调用，则暴露面更小；但 kmain 的第二次调用仍在，F01 的别名问题独立成立。
- **裁决**：需用户裁决（令牌类型会改多个签名，收益是把引导阶段变成类型状态；也可先用 one-shot + 可见性收紧）。

## FINDING RD3-DESIGN-D2-12（P3）

- **位置**：`os/kernel/src/lib.rs:213-233`（`arch_boot_finish<A, H>`）、`:325-376`（`resume_additive<A, H>`）、`:285-300`（x86/aarch64 实例化）、`:388`、`:757`、`:777`、`:792`（各调用点）。
- **严重度**：P3。
- **分类**：[MODERN]（C 的 `head.S` 与分页代码天然按架构绑定；Rust 可以把 `A` 与 `H` 的配对做成 associated type，消除错配的可能）。
- **问题**：`arch_boot_finish::<A, H>` 与 `resume_additive::<A, H>` 把「启动架构」与「换栈跳转实现」作为两个自由类型参数，二者没有类型级约束；`H` 只需实现 `HigherHalf`，与 `A` 无关。虽然当前每个 target 只编译自己那一个 `HigherHalf` 类型，错配在正常构建里不可达，但函数签名允许 `arch_boot_finish::<X86_64BootArch, 某个别的 H>`，编译器不会拒绝。
- **影响**：这是抽象边界上的一个「可表示的非法组合」；未来新增架构、测试替身或条件编译调整时，错配会以「编译通过、运行期用错指令序列」的形式出现。它不是当前 bug，属于类型系统没有把设计意图写全。
- **证据或推理**：`lib.rs:214-218` 的两个 where 子句 `A: BootArch`、`H: crate::boot::HigherHalf`；`lib.rs:232` 只调用 `H::jump_to_kmain`；没有任何 `A::...H` 的关联约束。
- **最小修复**：给 `BootArch` 加 `type Transition: HigherHalf;`（或新建 `BootEntry` trait，把 `BootArch` 与 `HigherHalf` 的配对、`arch_boot_impl` 的调用一起收进去），`arch_boot_finish<A: BootArch>` 只保留一个类型参数，跳转用 `A::Transition::jump_to_kmain`。测试替身用 `MockBootArch::Transition = MockHigherHalf`。
- **置信度**：高（签名与调用点已读）；「当前不可达」是 cfg 分析结论。
- **是否需源码核实**：否。
- **若我错了**：若项目刻意保留两个参数以便在测试里注入不同的 `H`，那么这是一种有意的可测试性设计；但 `arch_boot_finish` 本身是 `#[cfg(not(feature = "mock"))]`，mock 构建里根本不存在，所以这个理由不成立。
- **裁决**：拒绝（当前签名应收紧）。

## FINDING RD3-DESIGN-D2-13（P3）

- **位置**：`os/libs/minix-elf/src/lib.rs:88-101`（`Elf64Ehdr` 的 `e_type`/`e_machine`）、`:108-125`（`Elf64Phdr` 的 `p_align`）、`:138-153`（`LoadSegment.align`）、`:427-430`（`machine_type`）。
- **严重度**：P3。
- **分类**：[DEL]（这些字段/函数是为「完整 ELF 头」镜像而留的，C 侧没有对应物，仓库内也没有读者）。
- **问题**：`machine_type()` 没有任何生产或测试消费者（只有它自己的单元测试）；`Elf64Ehdr.e_type` 在构造前已被 `parse_ehdr` 检查，字段本身没有读者；`e_machine` 只被 `machine_type` 读；`Elf64Phdr.p_align` 被解析并复制进 `LoadSegment.align`，而 `align` 在全仓没有读者（测试构造除外）。
- **影响**：公共 API 面比实际需要大，读者会以为存在对齐处理；`Elf64Ehdr` 通过 `SegmentIter::ehdr()` 暴露给 boot-shim，新增字段容易被误用。最小化机会明确。
- **证据或推理**：`grep -rn machine_type os --include=*.rs` 只有定义与自测；`grep -rn '\.align'` 的匹配都在测试构造；文档 §3.2 第 442 行自述「`p_align` 保留了但当前没有读者」。
- **最小修复**：删除 `machine_type()`；`Elf64Ehdr.e_type`/`e_machine` 若只用于校验，改成 `parse_ehdr` 内部局部变量，不放进结构体；`p_align`/`align` 在没有消费者前删除，或在 `LoadSegment` 上标注 `#[doc(hidden)]` 并加「无读者」注释。若保留 `p_align` 是为了未来的页对齐校验，应在同一次提交里加上消费者。
- **置信度**：高（grep 已重放）。
- **是否需源码核实**：否。
- **若我错了**：若 `machine_type` 被仓库外（例如工具或下游 crate）使用，删除会破坏它；但它是 `pub` 且 crate 未发布，仓库内无读者，风险低。若未来要加「拒绝错误架构的 ELF」校验，`e_machine` 会重新有读者，可以到时再加。
- **裁决**：拒绝（应删/收窄）。

## FINDING RD3-DESIGN-D2-14（P3）

- **位置**：`os/arch/src/arch/boot.rs:461-464`（`load_elf_into` 先 `segment_iter(image)`，再 `entry_point(image).unwrap_or(0)`）。
- **严重度**：P3。
- **分类**：[MODERN]（同一份 ELF 头被解析两次；第二次失败在逻辑上不可能，却用 `unwrap_or(0)` 把错误变成合法的空入口）。
- **问题**：`segment_iter(image)` 已经调用 `parse_ehdr` 并通过，紧接着 `entry_point(image)` 再解析一次同一份字节；失败只可能在「字节在两次调用之间被改写」（当前没有）或解析器不一致（不应有）。`unwrap_or(0)` 把这种不可能的错误静默替换成入口 0，随后进程会从空地址取指。
- **影响**：错误路径不可达，但一旦可达就是一个静默的「跳到 0」；同时多一次解析。可读性上，读者会以为 entry 解析可能失败、失败可以接受。
- **证据或推理**：`boot.rs:461-464`；`minix-elf` 的 `entry_point` 在 `lib.rs:419-422` 重新调用 `parse_ehdr`。
- **最小修复**：用已经持有的迭代器头：`let iter = segment_iter(image)...; let entry = iter.ehdr().e_entry;`，删除第二次解析与 `unwrap_or(0)`。`trap_api.rs:469` 同样可以复用前面迭代器的 `ehdr()`，但它先消费了迭代器，改动稍大，可另开一条。
- **置信度**：高（代码路径已读）。
- **是否需源码核实**：否。
- **若我错了**：若 `segment_iter` 故意不暴露 `e_entry`（它暴露了 `ehdr()`），或者 `entry_point` 有额外的架构过滤（没有），则复用不成立；当前 API 支持复用。
- **裁决**：拒绝（应改成复用已解析头）。

## FINDING RD3-DESIGN-D2-15（P3）

- **位置**：`os/kernel/src/boot/higher_half.rs:21-30`（trait 文档声称 mock 可验证调用契约）、`os/kernel/src/lib.rs:6065-6100`（`MockHigherHalf` 与唯一测试）、`os/kernel/src/lib.rs:213-233`（`arch_boot_finish` 是 `#[cfg(not(feature = "mock"))]`）。
- **严重度**：P3。
- **分类**：[MODERN]（C 没有宿主测试；Rust 既然引入了 trait 与 mock，就应当让 mock 真正承载调用契约，而不是只做名字检查）。
- **问题**：trait 文档的第 3 条理由是「Testability: Mock implementations can verify the signature and the call contract」；实际唯一的测试 `test_higher_half_trait_is_implementable` 只做 `accept_higher_half::<MockHigherHalf>()` 的 trait bound 检查。`MockHigherHalf` 里的 `HIGHER_HALF_CALLED` 原子标志从未被断言；`arch_boot_finish` 与 `resume_additive` 在 mock 构建里不编译，所以 mock 根本没有被调用的宿主调用点。
- **影响**：可测试性论证是空的；读者会以为「换栈跳转的调用契约有宿主测试」，实际只有 QEMU 载体。未来若有人依赖这个论证保留 trait，会保留一个没有测试支撑的抽象。
- **证据或推理**：`lib.rs:6072-6100`；`lib.rs:213` 的 `#[cfg(not(feature = "mock"))]`；`higher_half.rs:28-30`。
- **最小修复**：二选一：
  1. 让 `arch_boot_finish`/`resume_additive` 在 mock 构建里也编译（把 `arch_boot_impl` 换成 mock 版本，`H` 用 `MockHigherHalf`），断言 `jump_to_kmain` 被调用一次且参数等于 `kinfo.kern_stack_top`；
  2. 删除 `HIGHER_HALF_CALLED` 与 mock 的「call contract」说法，把 trait 文档的理由改成「统一调用点 + 架构隔离」，并注明「换栈语义只有 QEMU 测试覆盖」。
- **置信度**：高（测试与 cfg 已核实）。
- **是否需源码核实**：否。
- **若我错了**：若 `arch_boot_finish` 的 mock 版本在别处被编译并测试（grep 没有找到第二个定义），则第 1 条已成立；当前只有 `arch_boot_test`（`lib.rs:798`）绕过 store 直接调 `arch_boot_impl`，不经过 `H`。
- **裁决**：拒绝（应让 mock 真正被调用，或删除空壳）。

## FINDING RD3-DESIGN-D2-16（P3）

- **位置**：`os/libs/minix-boot/src/kernel_info.rs:13-22`（`#[repr(C)]` 与「pins the field order and each field's offset for debuggability and for a future FFI boundary」）。
- **严重度**：P3。
- **分类**：[DEL]（`kinfo_t` 的 C 布局是历史形态；Rust 的跨镜像交接已经由 v1 序列化器承担，`KernelInfo` 自己不是 ABI）。
- **问题**：`KernelInfo` 被标注 `#[repr(C)]`，理由写的是「调试与未来 FFI 边界」；同一段注释又说「It is NOT a cross-image layout contract」，wire 用 `BootHandoffV1` 的独立 `#[repr(C)]` 结构加 offset 断言。`KernelInfo` 的字段含 `&'static [T]`，其内部表示是工具链定义的，本身就不能作为 FFI 布局；`repr(C)` 在这里只约束了一个不该被约束的 Rust 类型。
- **影响**：读者会误以为 `KernelInfo` 的字段顺序是稳定契约；未来若有人按 `offset_of!` 读它，会得到跨编译器不保证的结果。移除 `repr(C)` 没有行为风险（当前没有 FFI 读者）。
- **证据或推理**：`kernel_info.rs:13-22`；`handoff_v1.rs` 的 wire 结构另有 `repr(C)`（文档 §3.1 与 v1 序列化器）；`KernelInfo` 的 slice 字段无法稳定布局。
- **最小修复**：删除 `#[repr(C)]` 与「future FFI boundary」的理由，保留调试用的 `Debug`（可派生）；如果确实需要稳定布局，另建一个 POD 的 wire 结构（已有 `BootHandoffV1`）。
- **置信度**：高（注释自相矛盾）；「没有 FFI 读者」是 grep 结论。
- **是否需源码核实**：否。
- **若我错了**：若仓库外有一个 C/汇编读者按 `KernelInfo` 的偏移读字段（当前没有），删除会破坏它；但 `KernelInfo` 含 Rust 胖指针，跨语言读本身不成立。
- **裁决**：需用户裁决（保留作调试便利的收益很小；我建议删除，但这是低风险偏好项）。

---

## 4. ARCH-PROPOSAL

> 本节的两条会改变「boot 是什么」：A01 改变交接的所有权模型，A02 改变交接契约里对执行视图的表达。它们不是 FINDING，供架构层拍板。

## ARCH-PROPOSAL RD3-A01

**标题**：把交接从「借用快照 + 逐载荷落坑」改成「所有权移交的单 arena」。

**动机**：F01、F10、F11 的根因是同一个：生产者把自己地址空间里的切片借给接收者，接收者再用一组 `.bss` landing pad 把每类载荷深拷贝一遍，并在拷贝前把「物理地址上的 `&str`」当成 `'static`。v1 路径其实已经有一个 arena（`BootHandoffV1`），但 direct/self-boot 路径仍走「固件池 + landing pad」；`KernelInfo` 的 `&'static [T]` 字段是这两种模型的折中，因此需要 `store_kernel_info` 这个手动深拷贝仪式。

**变更**：

1. 所有生产腿统一为「生产者在一块预先保留的 arena 里写头部 + 分区载荷；接收者 adopt 这块 arena 为只读」；arena 的物理范围进入 `startup_objects`，由内核在运行期保留。
2. `KernelInfo` 变成 `KernelInfo<'arena>`（或 offset 描述符），不再持有伪造 `'static` 的引用；模块名以 `PhysBytes + len` 存在 header 里，接收者在 arena 映射后构造 `&'arena str`。
3. 删除 `store_kernel_info` 的逐载荷 landing pad 与 `kmain` 的第二次 store；只保留「adopt arena」一步。

**影响面**：v1 线协议版本、direct/self-boot 的 arena 分配（bump 池已有空间）、`BootStorage`/`KernelInfo`/`BootModule` 的类型、`kmain` 的 Phase A。**代价**：direct 腿需要把 DTB 派生的 memmap/param/名字复制进 arena（当前有些字节直接指向 DTB 或 bump 池），多几页内存与一次拷贝；arena 必须在高半/DM 建立前可读，映射顺序要重排。**收益**：F01 的别名问题、F10 的伪 `'static`、F11 的 one-shot 约定、F05 的部分派生字段都可以在同一个模型里消掉。

**与现有文档的关系**：文档 §3.3/§4.4 描述了 landing 机制与「两条生产路径在 kmain 合流」；A01 会把「合流点」提前到 arena adopt，`kmain` 不再需要知道三种交接形态。**需用户裁决**：是继续维护 landing pad（成本低、现状稳定），还是做一次 wire v2 + direct arena 的统一（成本高、消除三类问题）。

## ARCH-PROPOSAL RD3-A02

**标题**：把「执行视图」（物理视图/链接视图）写进交接契约，而不是在 RISC-V 的跳转实现里用 PC 探测。

**动机**：`Riscv64HigherHalf` 在 asm 里用 `auipc t3,0` + `bltu t3, t4` 判断当前 PC 是否低于 `kern_virt_base`，低于则加 `delta`；这等于在实现里维护一个「direct leg vs resume leg」的隐藏模式。它的正确性依赖一个未写进契约的布局不变量（恒等窗口的 PC 严格低于 `kern_virt_base`，且未来不会出现「PC 在高半但仍需 delta」的腿）。x86_64 又依赖「调用点执行地址 = 链接地址」，aarch64 则两者都不需要。三种架构对同一个 trait 的隐含前提不同，而 trait 的 Safety 段没有表达它。

**变更**：

1. 交接头部增加一个 arch-neutral 字段：`EntryView { LinkView, Relocated }`（或 `entry_delta: u64`，由生产者静态计算；`0` 表示 PC 等于链接地址）。这里的判据是「PC 是否等于链接地址」，不是「PC 在高半还是低半」：x86 测试载体链接在低地址、在高半映射建立后仍在低地址执行，PC 等于链接地址，属于 `LinkView`；aarch64 direct-core 链接在高半、在低地址执行，属于 `Relocated`（但它的 `ldr` 取绝对值，所以实现可以忽略该字段）。
2. 生产者按自己的启动腿填写：riscv direct = `Relocated` + `delta`，riscv v1 resume = `LinkView`，aarch64 direct = `Relocated`（实现忽略），aarch64 v1 resume = `LinkView`，x86 生产与测试载体 = `LinkView`。
3. `HigherHalf::jump_to_kmain` 的签名带入 `EntryView`（或从 `kinfo` 读取），x86 在 `Relocated` 下 panic（`call` 是链接期相对位移，从重定位后的 PC 出发会落到 `kmain` 的低地址副本，不是链接 VMA），aarch64 忽略，riscv 在 `Relocated` 下加 `delta`、在 `LinkView` 下直接跳；删除 PC 探测。

**影响面**：wire v2 增加字段、所有生产者的构造点、三份 `HigherHalf` 实现、§3.4/§4.3 的说明。**代价**：一个字段与生产者义务；文档 §3.4 的「不要把这个细节推给调用者」的论证需要重写（因为 `EntryView` 是启动腿的语义，不是 RISC-V 指令细节）。**收益**：删除运行时启发式与未写明的布局不变量；让「跳转后 PC 落在哪一侧」成为可测试的契约；未来新增启动腿不必改 arch asm。

**与现有文档的关系**：文档 §3.4 末尾明确选择「留在实现里」，理由是「把它写成调用者前置条件会把 riscv64 的实现细节推给调用者」。A02 的反驳是：`EntryView` 不是 riscv 指令细节，而是「生产者从哪个视图进入」这一 OS 语义，三个架构都能消费；把它写进交接反而更符合第一原则「抽象语义，不描述机制」。**需用户裁决**：若团队认为 PC 探测已被真机验证且成本更低，可以保持现状，只在 §3.4 补一句「该不变量只在当前布局下成立」；若希望消除隐藏模式，则走 A02。

---

## 5. 已核查并接受的设计点（不计 FINDING）

1. **RISC-V 两腿合一 + `checked_sub`**：接受。trait 的语义契约是「最终落在 `kmain` 的链接 VMA」，PC 探测是局部实现；`kern_virt_base >= kern_phys_base` 用 `checked_sub` 显式化，release 下不会静默回绕。A02 只是把它提升为契约，不是否定当前实现。
2. **`HigherHalf` trait 本身**：接受。三份 asm 差异大，trait 提供单一 Safety 落点与统一调用点；F12/F15 是签名与测试的局部问题，不需要拆掉 trait。
3. **`stack_top` 冗余参数**：接受现状，同意文档登记的 D2-04。两个生产调用点都传 `kinfo.kern_stack_top`，删除参数是纯收敛；不另立 FINDING。
4. **入口规则「跳首个 PT_LOAD 的 VMA，并断言 `e_entry` 相等」**：接受。它比「跟随 `ENTRY`」更安全，能挡住链接脚本把入口放到非首段；F09 只要求文档与字段跟上这个规则。
5. **`load_segments_into_phys_memory` 对源/目标重叠 panic**：接受。引导期没有恢复面，fail-fast 合理；与段间重叠返回 `Err` 的不对称已在 §4.2 写明，属于有意的分级。
6. **`BootHandoffV1` 的 `#[repr(C)]` 与 offset 断言**：接受。wire 是真正的字节级契约，需要 `repr(C)` + `offset_of!`；F16 只针对 `KernelInfo` 自身的 `repr(C)`。
7. **`kern_stack_top` 作为字段**：接受。测试载体需要自定义栈顶；生产严格门把它锁成 `kern_virt_base + kern_size`，这个「字段 + 强校验」的组合是合理的，F05 只针对 `syscall_entry`/`user_sp` 的派生常量。
8. **`place_modules` 把名字拷进物理名字池**：接受机制。satp=0 视图下不能解引用静态胖指针，字节池是正确手法；F10 只反对用 `&'static str` 表达它。
9. **`boot_policy` 的粒度阶梯**：接受。能力事实与策略分层清楚，错误消息不点名数值是有意的「不制造伪精确」；没有发现非法状态。
10. **`arch_boot_finish` 与 `resume_additive` 分开**：接受。两条路径的相位不同（直接建根 vs 在活动根上做加法），强行合并会把「不得重建根」这条安全约束藏进一个布尔参数；当前的注释与 `new_from_page` 说明足够清楚。

---

## 6. 与文档已登记 D2-* 的关系

| 文档登记 | 我的结论 | 关系 |
|---|---|---|
| D2-01（`SegmentIter::new`/`parse_phdr` 未检查加法） | 同意存在；修法应并入 F02 的 `Result` 迭代器，而不是只做 checked 算术 | F02 覆盖更大范围 |
| D2-03（wire v2 删除 `syscall_entry`） | 同意；建议把 `user_sp` 与三套规格一并处理 | F05 扩展 |
| D2-04（`stack_top` 参数并回 `kinfo`） | 同意；不另立 FINDING | §5 第 3 条 |
| D2-05（引导路径 fail-closed） | 同意方向；范围应扩到 exec/VM | F02 |
| D2-06（解析错误与装载错误分层） | 同意；F02/F03 给出类型形状（`Result` 迭代器 + `ValidatedKernelImage`） | F02/F03 |
| D2-13（x86_64 临时取证打印） | 同意存在；不属于 §3/§4 设计骨架，未进一步展开 | 无 |
| D2-14（bootface 容量编译期断言） | 已在 `bootface_common.rs:16-21` 落实，未发现问题 | 无 |

---

## 7. VERDICT

**事实层**：§3/§4 的绝大多数主张与源码相符（抽样 25 条，20 条一致）。三条「不符」都集中在「文档替源码说了一个更强的保证」：

- §3.1 说 `entry_point` 是装载方的进入依据，实际跳的是 `kern_virt_base`（F09）；
- §4.2 说生产路径由 `load_kernel_with_loader` 保证先校验后拷贝，实际 UEFI 腿绕过它（F03）；
- §3.3 对 `syscall_entry` 的字段语义、严格校验、后置校验三套规格没有统一（F05）。

**设计层**：骨架成立，不需要推翻「双地址链接 + 按 `p_paddr` 摆放 + trait 化跳转 + 页粒度策略」这四件事。需要先修的两条是所有权与错误模型：

1. F01：`store_kernel_info` 的 `&'static` + 二次写入，是安全 API 的别名契约漏洞；最小修复（幂等/删 kmain 第二次调用）成本很低。
2. F02：段迭代器的 fail-open 影响 boot、exec、VM 三个消费者；修法会动 `minix-elf` 的公共 API，需要用户裁决（`Result` 直接切换或新增 `try_segment_iter` 过渡）。

**不建议判 CONVERGED 的理由**：在 F01/F02/F03 修完之前，§3/§4 的「所有权」「错误」两个维度不满足设计审查的收敛条件；F05/F06/F08 还涉及线协议与链接布局的单源问题，至少需要用户裁决后才可关闭。

**一致性自评**：本报告 16 条 FINDING 的 `path:line` 均已用 `Read`/`grep` 重放；其中 F01、F02、F03、F05、F08、F09、F10、F12 的结论是逐行读码得出，置信度高；F06 含一个未构造工件的推理步骤，已标注「需源码核实」；F07 的缺口可由构造 ELF 测试确认；F16 是低风险偏好项。`consistency` 自评约 94%（这是审查结论的路径复核一致率，不是 §1 的文档事实一致率）——不确定的部分集中在「UB 判定依赖别名模型」与「镜像缩到 2 MiB 是否合法」两处，已在对应条目的「若我错了」写明。

- 本次扫描：16 条 FINDING，2 条 ARCH-PROPOSAL。
