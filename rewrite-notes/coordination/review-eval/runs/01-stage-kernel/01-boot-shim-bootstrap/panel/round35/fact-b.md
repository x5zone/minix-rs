# fact-b：第 2 章与第 4–6 章事实核对

扫描范围：目标文档 §2.1–§2.8、§4.1–§4.9、§5.1–§5.4、§6.1–§6.4；C 源码 `minix3/minix/kernel/`、`minix3/minix/servers/vm/`、`minix3/sys/arch/i386/stand/`；Rust 源码 `os/boot-shim/`、`os/kernel/`、`os/kernel-image/`、`os/arch/`、`os/libs/minix-boot/`、`os/libs/minix-types/`；QEMU 测试脚本与 `test-kernel-map`。`cargo test --list` 不在本次允许命令清单内，测试计数用 `grep -c '#\[test\]'` / `grep '#\[ignore'` 逐文件复算，并在 §5 项下说明。

## FINDING FACT-B-01（P2）
- 位置：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:2296`（§4.1 `BOOT_PT_BUMP_PAGES` 条目）。
- 严重度：P2
- 问题：文档称“两个直核面只建到跳 `kmain` 为止……256 页（1 MiB）按直核面只建恒等窗口、内核高映射与模块放置所需的几级中层表来估，不覆盖 Direct Map 与 arena 链”。这与直核面的实际路径矛盾：`bootface.rs` 的 256 页常量注释明确把“两个 DM 窗口”计入预算，且 `bootface` 调 `arch_boot` 后走到 `arch_boot_impl` 的 Step 4 `establish_boot_dm`，DM 页表页正是从这同一片注册给 `pt_alloc` 的 256 页 bump 区取。
- 影响：该句是 1024 与 256 两个预算差异的理由。按文档理解，直核面的 DM 覆盖不占 256 页池；实际它占。后续改小 `PT_BUMP_PAGES`、分析 `AllocationFailed` 或判断“直核面为什么不需要 1024 页”时会得到错误模型。
- 证据或推理：
  - E1：`sed -n '126,134p' os/kernel-image/src/bootface.rs` 输出：
    - `/// Page-table bump region size: 1 MiB (256 pages...`；
    - `/// tables for Sv39 root + identity low half + high half + two DM windows`；
    - `const PT_BUMP_PAGES: usize = 256;`。
  - E1：`sed -n '278,282p' os/kernel-image/src/bootface.rs` 输出 `minix_kernel::boot_alloc::init_boot_pt_alloc(pt_base, pt_end);`；同文件 `:442` 输出 `minix_kernel::arch_boot(&kernel_info, root_page)`。
  - E1：`sed -n '214,228p' os/kernel/src/lib.rs` 显示 `arch_boot_finish` 调 `arch_boot_impl::<A>`；`sed -n '975,983p'` 显示 `arch_boot_impl` 的 Step 4 调 `crate::dm_coverage::establish_boot_dm::<A>`。
  - E3：由上述调用链，DM 页表分配发生在直核面、且发生在 `arch_boot` 返回/跳 `kmain` 之前；`bootface` 已把 `boot_pt_alloc` 注册为 `pt_alloc` 的取页函数（`boot_validate_and_prepare` 的 `is_registered()` 分支），所以“不覆盖 Direct Map”不成立。
- 最小修复：把该句改成“256 页覆盖恒等窗口、内核高映射、模块放置与两个 DM 窗口；arena 链不在直核面预算内”，并把 `bootface.rs:126-134` 的注释作为锚点；若确实想只讨论 `PT_BUMP_PAGES` 常量本身，则删去“不覆盖 Direct Map”，不要把它写成路径事实。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：如果这里的“Direct Map”专指运行期 VM DM 窗口而不是 boot 期 DM 覆盖，请把两个术语拆开并补锚点；按当前用词，它与 `bootface.rs` 的“two DM windows”直接冲突。

## FINDING FACT-B-02（P3）
- 位置：`01-boot-shim-bootstrap.md:2584`（§4.3 `num_pages = (data.len() + 4095) / 4096` 条目）。
- 严重度：P3
- 问题：文中说“写小了（≤4094）则只要余数非零就会少一页，最后一段字节落进未分配的页”。这个反事实前提过强，实际不是“只要余数非零”就少页。
- 影响：这是对 4095 这个魔数的解释。读者若按字面理解，会以为任何 ≤4094 的加数在所有非整页输入上都少一页；实际只有余数 `r < 4096 - A` 的输入才少页。可能引出错误的“修法”。
- 证据或推理：
  - E1：`printf` 复算：`x=4095, A=4094 -> 1 (ceil=1)`，`x=4097, A=4094 -> 1 (ceil=2)`。即 `+4094` 对 4095 字节仍正确，对 4097 字节才少一页。
  - E2：正确条件为 `A >= 4096 - r`；`A=4095` 是对所有非零余数都成立的最小常量；`A<=4094` 只在 `r < 4096-A` 时失败。
- 最小修复：把“只要余数非零就会少一页”改为“`A=4095` 是对任何非零余数都成立的最小加数；更小的 `A` 会在 `r < 4096-A` 的输入上少一页（例如 `A=4094` 仅在 `r=1` 时失败）”。
- 置信度：高
- 是否需源码核实：否（纯算术）
- 若我错了：如果句子只针对“会有一些非零余数失败”而非“所有非零余数”，请把“只要”删掉；当前字面是全称判断。

## FINDING FACT-B-03（P3）
- 位置：`01-boot-shim-bootstrap.md:2678`（§4.4 `KernelInfo` 字段表 `syscall_entry` 行）。
- 严重度：P3
- 问题：文档称 `syscall_entry` 是“引用性启动元数据；实际门地址由各架构 asm 标签提供，当前三个架构都按基址使用”。源码明确只有 x86-64 消费 `KernelInfo.syscall_entry`；aarch64/riscv64 的门地址由 `syscall_entry_va()` 从各自 asm 标签取，该字段在两者上仅为 reference-only metadata。
- 影响：读者可能以为 aarch64/riscv64 的 syscall gate 也等于 `kern_virt_base`，或以为改 `KernelInfo.syscall_entry` 会影响三个架构的入口；实际只影响 x86-64 的 LSTAR 配置。
- 证据或推理：
  - E1：`sed -n '109,113p' os/libs/minix-boot/src/kernel_info.rs`：`Kernel metadata recorded for all architectures; only x86-64 uses it ... aarch64/riscv64 ... this field is for reference only.`
  - E1：`sed -n '366,389p' os/arch/src/lib.rs` 显示 `syscall_entry_va()` 按架构分派：x86-64 取 `trap_stub::syscall_entry_va()`，aarch64 取 `arm64::trap_stub::syscall_entry_va()`，riscv64 取 `user_trap_vector_va()`；`:482` 的注释再次写明 aarch64/riscv64 的 `syscall_entry_va` 来自各自 asm。
- 最小修复：改为“三个架构的 builder 都把该字段填成 `kern_virt_base`；只有 x86-64 消费它（LSTAR），aarch64/riscv64 的门地址来自各架构 asm 标签，字段仅作参考”。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：如果“按基址使用”仅指 builder 赋值而非消费者，请改成“按基址填充”，避免“使用”一词被读成消费。

## FINDING FACT-B-04（P3）
- 位置：`01-boot-shim-bootstrap.md:1247`（§2.7 `earm/head.S` 引文中的注释省略标记）。
- 严重度：P3
- 问题：引文写“……（4 行标题注释省略，原文见 earm/head.S:22-25）……”，实际 `earm/head.S` 的标题注释是 3 行（22–24），第 25 行是 `.global MINIX`，不是注释。
- 影响：锚点与计数不一致；按 `22-25` 去读会把 `.global` 也算进“标题注释”，属于引文定位噪声。
- 证据或推理：
  - E1：`sed -n '21,26p' minix3/minix/kernel/arch/earm/head.S | cat -n` 输出：21 `.section .startup`，22/23/24 三行 `/* ... */`，25 `.global MINIX`，26 `MINIX:`。
- 最小修复：改成“3 行标题注释（22–24）”或“原文见 earm/head.S:21-25”。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：如果“4 行”把 `.section .startup` 也算作标题块，请把范围写成 21–24 并说明计数口径；当前“22-25”确实包含非注释行。

## FINDING FACT-B-05（P3）
- 位置：`01-boot-shim-bootstrap.md:3285`（§5.2.3 深拷探针读数段）。
- 严重度：P3
- 问题：文档用“探针页是 `LOADER_DATA`，最终 EBS 快照本来就会把它列进保留类型”解释 `in_snapshot_free=0`。但在 `test-kernel-map` 中，`in_snapshot_free` 读的是 `kernel_info.memmap`，而该切片由 `build_memmap()` 在探针分配之前取得；该测试随后调用的是 `uefi_helpers::exit_boot_services()`，没有生产 UEFI 的 EBS 配对/分类循环，因此不存在“最终 EBS 快照把它列进保留类型”这一步。`build_memmap()` 返回的又是 conventional-only 投影（`Disposition::Allocate`），`in_snapshot_free=0` 只能说明该投影未包含该区间，不能按文中机制归因。
- 影响：这是对探针判别力的解释。机制归因错位会让读者高估/低估探针排除的候选，尤其第二层“候选②未被排除”的论证依赖这个字段的含义。
- 证据或推理：
  - E1：`sed -n '64,66p;95,100p;111,112p;142,145p' os/qemu-tests/test-kernels/kernel/bootstrap/test-kernel-map/src/main.rs`：`let memmap = uefi_helpers::build_memmap();` 在探针 `allocate_pages` 之前；`exit_boot_services()` 无配对循环；`in_free` 遍历 `kernel_info.memmap`。
  - E1：`sed -n '524,526p' os/boot-shim/src/uefi_helpers.rs` 显示 `build_memmap()` 只是 `build_memmaps().0`；`build_memmaps` 的 `conv` 表来自 `Disposition::Allocate`，不是最终 EBS 快照的全部描述符。
- 最小修复：把该句改成“`in_snapshot_free` 读的是 `build_memmap()` 在探针分配前拍的 conventional-only 投影；0 只说明该投影未包含该区间，不能反推原始 UEFI 快照或 EBS 配对的归类”。若想讨论生产 EBS 快照，请另引 `pair_and_classify` 的输出，不要用 `test-kernel-map` 的 `build_memmap` 字段代替。
- 置信度：中高
- 是否需源码核实：是（已核实测试代码）
- 若我错了：如果该段的“最终 EBS 快照”指的不是 `test-kernel-map` 的 `kernel_info.memmap` 而是另一次生产运行，请补那次运行的路径/命令；按当前引用的测试内核，二者不是同一个对象。

## FINDING FACT-B-06（P3）
- 位置：`01-boot-shim-bootstrap.md:3497`（§6.4 GDB 命令注释）。
- 严重度：P3
- 问题：注释写“`resume_additive` 只在 U-Boot 接收镜像里”。实际 x86_64 与 aarch64 的 UEFI v1 接收腿也调用 `resume_additive`：aarch64 经 `arch_boot_resume_high_half`，x86_64 经 `resume_after_v1_decode`；`resume_additive` 本身没有 target/feature 门（仅 `not(feature = "mock")`）。
- 影响：GDB 排查 aarch64/x86 UEFI 接收腿时，按这句话会以为 `resume_additive` 不在符号表里；实际可以下断点，且它正是 additive 阶段的泛型体。
- 证据或推理：
  - E1：`grep -n 'resume_additive::<' os/kernel/src/lib.rs` 命中 `:388`（aarch64 `arch_boot_resume_high_half`）、`:758`（x86_64 `resume_after_v1_decode`）、`:777`（riscv64 `resume_after_v1_decode`）。
  - E1：`sed -n '325,326p' os/kernel/src/lib.rs` 的函数定义只有 `#[cfg(not(feature = "mock"))]`，没有 target 门。
- 最小修复：改为“`resume_additive` 是三条 v1 接收腿共用的泛型体；riscv 直核会话不经过它，改 break `minix_kernel::arch_boot`”。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：如果“U-Boot 接收镜像”被用来泛指所有 v1 接收镜像，请改成“v1 接收镜像”，避免与“只有 U-Boot 腿”混淆。

## FINDING FACT-B-07（P3）
- 位置：`01-boot-shim-bootstrap.md:1090`（§2.6.2 对 `pg_map` 静态状态的引用）。
- 严重度：P3
- 问题：文中引用 `pg_utils.c:270-302` 的 `static int mapped_pde = -1; static u32_t *pt;`；实际源码是 `static u32_t *pt = NULL;`（`pg_utils.c:271`）。这不是语义错误，但属于行内源码引用与现行源码不完全一致。
- 影响：读者按引用去源码找“没有初始化的 `pt`”会看到 `= NULL`；严格引文对账会报不一致。对机制理解影响很小。
- 证据或推理：
  - E1：`sed -n '270,271p' minix3/minix/kernel/arch/i386/pg_utils.c` 输出 `static int mapped_pde = -1;` / `static u32_t *pt = NULL;`。
- 最小修复：在行内引用补上 `= NULL`，或把该处标成“语义摘述”而不是逐字引用。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：如果该行引用的是某个历史版本，请标出版本；按现行源码它不是逐字一致。

## FINDING FACT-B-08（P3）
- 位置：`01-boot-shim-bootstrap.md:2307-2318`（§4.2 `prepare_and_enter_kernel_image` 引文块）。
- 严重度：P3
- 问题：引文块开头声称“保留顺序与关键约束”，并列出未展开段，但没有列出被省略的 `assert_eq!(layout.entry_point, layout.kern_virt_base.0, ...)`（`uefi_helpers.rs:977-980`）。该断言正是“入口等于首段 VMA”的关键约束，且不是注释或排版改写。
- 影响：引文读者会以为 UEFI 入口只做了 `compute_kernel_layout` 后直接 `reserve_kernel_span`；实际两者之间还有入口点校验。虽然 §4.4 另有“装载器另断言 e_entry == min(vaddr)”的说明，引文块本身仍不完整。
- 证据或推理：
  - E1：`sed -n '970,983p' os/boot-shim/src/uefi_helpers.rs` 显示 `compute_kernel_layout` 后有 `assert_eq!(layout.entry_point, layout.kern_virt_base.0, "boot-shim: ELF entry does not equal the first PT_LOAD VMA")`，然后才是 `// A: claim the span...` 和 `reserve_kernel_span(&layout);`。
- 最小修复：在引文块中保留该 `assert_eq!`，或在“未展开段”清单中明确写“入口点/VMA 校验省略”。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：如果作者把该断言归入“注释与排版改写”，请把它显式写进省略清单；它不是注释。

## FINDING FACT-B-09（P3）
- 位置：`01-boot-shim-bootstrap.md:725`（§2.4.1.3 前提二）。
- 严重度：P3
- 问题：文中写“本仓核对过的 E820 样本最低条目正是 `0x1000`”，但未给样本文件、QEMU 命令或日志路径；在允许读取的 C/Rust 源码范围内没有可复核的对应物。该句自己接着说“论证只需要‘非零且页对齐’，不依赖这个具体值”，因此这是可删除的具体读数，而不是论证所需的前提。
- 影响：无法按文档锚点复核的读数会削弱前提二的可审计性；读者可能把 `0x1000` 当成结构保证，而它只是某次样本的观测值。
- 证据或推理：
  - E2：`rg -n '本仓核对过的 E820 样本|E820 样本最低条目' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 只命中该段；C 源码的 `arch_init` 只给出 `BIOS_MEM_BEGIN/END` 与 `BASE_MEM_TOP/UPPER_MEM_END`（`minix3/minix/include/arch/i386/include/memory.h:29/33/118/119`），没有 E820 样本文件。
  - E3：结论“最低基址 ≥ `0x1000`”可由 `arch_init` 的两条 `cut_memmap`（`arch_system.c:277-278`）与 `add_memmap` 的 `roundup` 推出，不需要样本读数。
- 最小修复：若要保留样本，给出 QEMU 日志/脚本路径或复跑命令；否则删掉“本仓核对过的 E820 样本最低条目正是 `0x1000`”，只保留“`arch_init` 切掉 `[0,0x1000)` 后最低基址至少 `0x1000`”的推导。
- 置信度：中
- 是否需源码核实：是（源码侧已核实可推出；样本本身不可核实）
- 若我错了：如果该样本在 `tmp/` 或某次运行日志里且允许引用，请补路径；当前文档没有给锚点。

## SUGGESTION FACT-B-S01
- 位置：`01-boot-shim-bootstrap.md:2296`（§4.1 同一段）。
- 建议：句中“四条架构各自的中层表”与本篇其余处“三条架构/五条腿”的计数口径不一致；仓内 Rust 架构是 x86_64、aarch64、riscv64 三个。若原意是“四条腿/五条生产腿”，改成“各条腿/各架构”；若原意是架构数，改成“三条架构”。这不是事实错误的核心，但会让预算段落的计数读起来自相矛盾。

## 覆盖率与阴性声明

- C 函数覆盖：§2 点名的 C 函数（`multiboot_init`、`pre_init`、`get_parameters`、`mb_set_param`、`overlaps`、`add_memmap`、`cut_memmap`、`alloc_lowest`、`pg_alloc_page`、`pg_clear`、`pg_identity`、`pg_mapkernel`、`pg_load`、`vm_enable_paging`、`alloc_pagetable`、`pg_map`、`memory_init`、`createpde`、`arch_post_init`、`pt_init`、`copy_trampoline`、`arch_boot_proc`、`setup_mbi`）均有对应说明；未发现“点名但正文完全无解释”的 C 函数。`vir2phys`、`pg_rounddown`、`pg_info`、`module_init` 只在调用链/行内解释，未单列小节，但都有实质说明。
- 测试计数：按文档同样的 `grep -c '#\[test\]'` 复算，boot-shim `--lib` 136（另 `tests/ebs_boundary_audit.rs` 6 条）、minix-boot 196、minix-elf 23、`os/kernel/src` 874（实际 `#[ignore = ...]` 属性 8 处）与 §5.2.1 的数字一致；§5 点名的测试函数用 `rg -n 'fn <name>'` 逐条核对存在。`cargo test --list` 未运行（不在允许命令清单），因此这里的“一致”是源码计数与文档计数的对账，不是 cargo 实跑转录。
- 未覆盖：§1 与 §3（另一角色负责）；历史 review 报告、defect-ledger、loop、metrics、suggestions、`expected/`、`panel/round35/` 其他 .md（按任务禁止读取，未作为证据）；QEMU 真机门链与探针 PA 读数未重跑，只核对了其代码/脚本与文档字符串的一致性；`tmp/` 下的运行日志未作为证据。
- 阴性声明：在本次允许的 C/Rust 源码范围内，未发现文档引用的常量值、结构体字段名、函数签名、错误码或测试名有“硬错误”；上述 FINDING 均为局部机制说明、引文完整性或计数/锚点问题。若 N=0 的类别，本报告不覆盖运行时行为、未列出的外部日志、以及任务禁止读取的历史材料。

- 本次扫描：9 条 FINDING，1 条 SUGGESTION。
