# D02 事实审查（P-FACT）— round1

- 文档：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`（2108 行）
- 关联代码：`.../02-higher-half-kernel/related-code.txt`（30 文件，`doc-code-map --check` 30/30 存在）
- 模式：只读事实核对（P-FACT）。未读历史 `panel/round1`、`defect-ledger`、`archive`、`expected`。
- 冻结决策仅按编号引用（D-110/111/112/114/115），未读其历史论证。
- 本轮只核对可取证的事实面：路径、符号、常量、函数签名、结构体字段、错误码/错误枚举、测试名与计数、引文逐字性、C/Rust 覆盖与阴性声明。设计取舍的合理性不在本轮判决范围。

## 0. 机械门（全部为 L1 工具输出）

| 门 | 命令 | 结果 |
|---|---|---|
| anchor-resolve | `bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | `anchors=65 resolved=65 zero-def=0 multi-def=0`，exit 0 |
| doc-code-map | `bash tools/doc-code-map.sh <doc> --check` | 30 个文件，存在性 30/30 通过，exit 0 |
| doc-structure | `python3 tools/doc-structure-check.py <doc>` | `✅ 1 个文件表结构与内部 .md 链接均可解析`，exit 0 |
| doc-ref | `bash tools/doc-ref-check.sh <doc>` | `✅ 内部章节引用均可解析`，exit 0 |
| doc-style | `bash tools/doc-style-lint.sh <doc>` | `error级=0`，exit 0 |
| doc-citation | `python3 tools/doc-citation-check.py <doc>` | `blocks=47 checked=27 failed=0 skipped=15 warned=5`，exit 0 |
| doc-code-density | `bash tools/doc-code-density-check.sh <doc>` | 4 个可判章全部 `OK`，阻断项通过，exit 0 |
| doc-publication（参考） | `python3 tools/doc-publication-check.py <doc>` | `无新增违规`，exit 0 |

结论：机械门八项全绿。以下 FINDING 均为工具之外的语义核对结果。

## 1. 已核对通过的事实（正列表）

证据等级 E1 = 直接读源文件/工具输出；行号为当前工作树。

### 1.1 C 侧

| # | 文档断言 | 源文件证据 | 判定 |
|---|---|---|---|
| C1 | `kernel.lds` 三常量 `_kern_phys_base=0x00400000`、`_kern_vir_base=0xF0400000`、`_kern_offset=(vir-phys)` | `minix3/minix/kernel/arch/i386/kernel.lds:4-6` 逐字一致 | 通过 |
| C2 | 五个 `AT()` 输出段：`.usermapped_glo`、`.usermapped`、`.text`、`.data`、`.bss` | `kernel.lds:25,27,29,30,32` | 通过 |
| C3 | 低侧三段 `.unpaged_text/.unpaged_data/.unpaged_bss` 无 `AT()`，`__k_unpaged__kern_unpaged_start/end`，`. += _kern_offset` 在其后 | `kernel.lds:14-22` | 通过 |
| C4 | 三常量的第二份前缀名 | `kernel.lds:8-10` 逐字一致 | 通过 |
| C5 | vm.h 位/常量全表（PRESENT 0x001、WRITE 0x002、READ 0x000、USER 0x004、PWT 0x008、PCD 0x010、ACC 0x020、DIRTY 1<<6、PS/BIGPAGE 0x080、GLOBAL 1<<8、ADDR_MASK 0xFFFFF000、ADDR_MASK_4MB 0xFFC00000、OFFSET_MASK_4MB 0x003FFFFF、PT_ENT_MASK 0x3FF、DIR_ENTRIES/PT_ENTRIES 1024、PAGE_SIZE 4096、BIG_PAGE_SIZE=4096*1024） | `minix3/minix/include/arch/i386/include/vm.h:7-38` | 通过 |
| C6 | 地址宏三条：`PDE(v)=v>>22`、`PTE(v)=(v>>12)&0x3FF`、`PFA(e)=e&0xFFFFF000` | `vm.h:61-63`（后两条经 `I386_VM_PT_ENT_SHIFT/MASK`、`I386_VM_ADDR_MASK` 展开等值） | 通过 |
| C7 | CR0/CR4 位值：PE bit0、WP bit16、PG bit31、PSE bit4、PGE bit7 | `vm.h:42,47,48,55,58` | 通过 |
| C8 | `vm.h` 里 `I386_CR0_WP` 注释误写为 `/* Enable paging */`，与 PG 行逐字相同 | `vm.h:47-48` | 通过（文档指出的注释错误属实） |
| C9 | `pg_identity` 全文（含 USER、BIGPAGE、WRITE，条件加 PWT|PCD，`mem_high_phys & MASK_4MB <= phys & MASK_4MB`） | `pg_utils.c:162-184` 与节选逐字一致 | 通过 |
| C10 | `pg_mapkernel` 全文（两行断言、`pde=kern_vir_start/I386_BIG_PAGE_SIZE`、循环体、`return pde /* free pde */`） | `pg_utils.c:186-202` | 通过 |
| C11 | `vm_enable_paging` 次序：清 `PG`、清 `PGE|PSE`、置 `PSE`、置 `PG`、置 `WP`、按 `pgeok` 置 `PGE`；CR4 只清 PGE|PSE | `pg_utils.c:204-245` | 通过 |
| C12 | `pg_clear` = `memset(pagedir,0,sizeof)`；`pg_load` 写 CR3 | `pg_utils.c:247-257` | 通过 |
| C13 | 文件级静态量 `kern_vir_start/kern_phys_start/kern_kernlen` 由 `&_kern_*` 取得 | `pg_utils.c:10-16` | 通过 |
| C14 | `pg_map` 置 USER 并用 `I386_VM_ADDR_MASK` 挂页表项 | `pg_utils.c:294-305` | 通过 |
| C15 | `pre_init` 函数体八条语句（assert + get_parameters + 五步 + return），注释 `but first a 1:1 mapping so this code stays where it should be` | `pre_init.c:217-238` 逐字一致 | 通过 |
| C16 | `head.S` 换栈四行，注释 `returnd` 拼写原样保留，`call _C_LABEL(kmain)`、`hang: jmp hang` | `minix3/minix/kernel/arch/i386/head.S:79-89` | 通过 |
| C17 | `mpx.S`：`.short 0x526F` 在 `.data` 首项；`.bss` 中 `k_initial_stack`、`.space K_STACK_SIZE`、`LABEL(__k_unpaged_k_initial_stktop)`、`k_boot_stack`/`LABEL(k_boot_stktop)` | `minix3/minix/kernel/arch/i386/mpx.S:626-641` | 通过（节选头已声明注释有删节） |
| C18 | `K_STACK_SIZE` = `I386_PAGE_SIZE` | `.../arch/i386/include/arch_proto.h:7` | 通过 |
| C19 | `tss_init(0, &k_boot_stktop)` 在 protect.c | `minix3/minix/kernel/arch/i386/protect.c:323,338` | 通过 |
| C20 | 改名名单显式 9 个对象；`MINLIB=3`、`MINC=13`、`SYS=2`，合计 27 | `.../i386/Makefile.inc`（`.for` 块 + 文档给出的三条复算命令实跑得 9/3/13/2） | 通过 |
| C21 | `mpx.S` 不在改名名单，只在 `SRCS+= mpx.S arch_clock.c …` | `Makefile.inc:75`；`.for` 名单无 mpx | 通过 |
| C22 | `proto.h`：`#ifndef UNPAGED` / `#define kmain __k_unpaged_kmain` | `minix3/minix/kernel/proto.h:49-52` | 通过 |
| C23 | `asm.h`：`_C_LABEL(x)` 在 `__ELF__` 下恒等；`_LABEL(x)` 展开 `.globl x; x:`；`LABEL(y)` 展开 `_LABEL(_C_LABEL(y))`，且 `#define` 后用制表符 | `minix3/sys/arch/i386/include/asm.h:64-65,98-99,177`；`grep -nP '#define\s+LABEL'` 命中 177 行，空格检索确实漏命中 | 通过 |
| C24 | `util_stacktrace` 从 `get_bp()` 起 `while(bp)`，`pc=bp[1]`、`hbp=bp[0]`，防环条件 `hbp <= bp` | `minix3/minix/lib/libsys/stacktrace.c:17-34` | 通过 |
| C25 | `0xF0400000 / 0x400000 = 961`、`0xF0400000 − 0x00400000 = 0xf0000000`、`961 × 4MB = 0xF0400000` | 文档 §6.2 两条 `printf` 的算式复算一致（0xF0400000 = 4030726144；4030726144/4194304 = 961） | 通过 |
| C26 | §2.4.1 表里 `pg_map` 是 4KB 细粒度映射函数、`I386_VM_READ` 无使用点 | `pg_utils.c:267-309`（USER/WRITE/PRESENT/ADDR_MASK 全用到；READ 不出现） | 通过 |

### 1.2 Rust 侧

| # | 文档断言 | 源文件证据 | 判定 |
|---|---|---|---|
| R1 | x86_64.ld：`KERNEL_HIGH_BASE=0xFFFF800000000000`、`KERNEL_PHYS_BASE=0x200000`、`.text/.rodata/.data/.bss` 四段各带 `AT(ADDR(sec)-KERNEL_HIGH_BASE)` | `os/kernel-image/x86_64.ld:26-27` 及四段头（节选与 §3.1/§4.1 一致） | 通过 |
| R2 | aarch64/riscv64 用 `KERNEL_VIRT_BASE` + `AT(... - VIRT + PHYS)`；aarch64 VIRT=`0xFFFF800000000000`/PHYS=`0x40200000`，riscv64 VIRT=`0xFFFFFFC000000000`/PHYS=`0x80200000` | `aarch64.ld:38-39`、`riscv64.ld:41-42`；两文件 `AT` 表达式与文档描述同形 | 通过 |
| R3 | `ENTRY(_start)` 三份脚本都有；`__image_end` 两条 `ASSERT`（≤0x400000、%0x200000==0），x86_64 减 `HIGH+PHYS`、另两份减 `VIRT` | `x86_64.ld:24,116,118`；`aarch64.ld:36,117,119`；`riscv64.ld:39,125,127` | 通过 |
| R4 | `.bss` 末尾 `. = ALIGN(0x200000)`，x86_64 与另两份同形 | `x86_64.ld:98`、`aarch64.ld:98`、`riscv64.ld:105` | 通过 |
| R5 | 自引导常量 `KERN_SIZE = 0x40_0000`，在 `bootface.rs` 与 `bootface_a64.rs` | `os/kernel-image/src/bootface.rs:113`、`os/kernel-image/src/bootface_a64.rs:80` | 通过 |
| R6 | `Paging` trait 恰好 24 个成员（`const PAGE_SIZE` + 23 方法，方法名单与文档逐一相符，含 `unsafe fn enable/destroy/flush_tlb/flush_tlb_addr`） | `os/arch/src/arch/paging.rs`（`sed` 抽取计数 24，23 个方法名全员存在） | 通过 |
| R7 | 三架构 `HUGE_PAGE_SIZES=[GIB_1,MIB_2]`、`HUGE_PAGE_SIZE=GIB_1`、`FALLBACK=MIB_2` | `x86_64/paging.rs:1236-1238`、`riscv64/paging.rs:1182-1184`、`arm64/paging.rs:1166-1168`（aarch64 侧目录名是 `arm64`，文档未写 `os/arch/src/aarch64`，无路径错） | 通过 |
| R8 | x86-64 `supports_huge_page` 查 `CPUID.80000001H:EDX.GBPAGES` | `x86_64/paging.rs:1223,1343,1753` | 通过 |
| R9 | `minix-elf` 被 5 个 crate 依赖 | `grep -rl 'minix-elf =' os/*/Cargo.toml os/*/*/Cargo.toml` 输出 arch/boot-shim/rs/vfs/vm 五行 | 通过 |
| R10 | `ElfError` 共 12 个变体 | `os/libs/minix-elf/src/lib.rs` 枚举体计数 12 | 通过 |
| R11 | `minix-elf` 27 个测试，四族名单（6+12+5+4）全部存在 | `grep -c '    fn test_'` = 27；§5.1 列出的 27 个名字逐个 `grep -c` 均 ≥1 | 通过 |
| R12 | `loader.rs` 32 个测试；代表测试名（含 6 个 `first_overlapping_pair_*`、3 个 `test_elf_file_end_*`、3 个 `load_segments_into_buffer_*`）存在 | `grep -c '#\[test\]'` = 32；13 个代表名逐个存在；`first_overlapping_pair` 六个测试名与文档括注语义一一对上（disjoint/overlap/abutting/ignores_zero_length/returns_first/empty） | 通过 |
| R13 | `compute_kernel_layout` 节选：`seg_result?`、`checked_add(paddr,memsz)`→`InvalidSegment`、`checked_sub`→`InvalidSegment`、`found_load`→`NoLoadSegments`、`KernelImage{layout}` | `os/boot-shim/src/loader.rs:255-295` 与节选逐行一致（注释压缩已声明） | 通过 |
| R14 | `load_segments_into_phys_memory` 接受 `&ValidatedKernelImage`，逐段 `segment_spans` + `copy_from_nonoverlapping` + BSS 清零 | `loader.rs:462-...`（函数体与节选同形） | 通过 |
| R15 | `KernelImage::validate` 先查段间重叠（`first_overlapping_pair`）再查源/目标重叠（命中 panic） | `loader.rs:196-217`；`first_overlapping_pair` 半开区间判据在 `loader.rs:514` 起 | 通过 |
| R16 | aarch64 `jump_to_kmain` asm 八条（mov sp / mov x1,#-16 / mov x2 / and / mov sp / mov x29,#0 / isb / ldr+br），`isb` 行内注释原文 | `os/kernel/src/arch/aarch64/higher_half.rs:53-67`；`isb` 行注释与源逐字相同 | 通过 |
| R17 | 三份实现都从 `kinfo.kern_stack_top()` 现算栈顶，`options(noreturn)` | aarch64 版 `higher_half.rs:29`（另两份由 anchor 解析与 §4.4 节选交叉确认） | 通过 |
| R18 | 测试载体 `kern_virt_base=0xFFFF_8000_0000_0000`、`kern_phys_base=image_base`、`kern_size=image_end-image_base`（x86_64/aarch64 载体） | `test-higher-half/src/main.rs:55-57`、`test-higher-half-aarch64/src/main.rs:51-53` | 通过 |
| R19 | riscv64 测试载体链接在 `0x80200000`，但合成 `KernelInfo` 的 `kern_phys_base=DRAM_BASE`（不是链接基址），`kern_size=0x400_000` | `test-higher-half-riscv64/link.ld:6`（`. = 0x80200000`）、`.../riscv64/src/main.rs:123,128`；与 §6.2 的 `kern_phys_base=0x80000000`、break=`0xffffffc000200144`、SP=`0xffffffc000400000` 三处数值自洽 | 通过（文档内部与源码三方一致） |
| R20 | §7.2 列出的 23 个 C 源/头路径全部存在 | 逐条 `test -f` 全部命中（含 `const.h`、`share/mk/sys.mk`、`pagetable.c`） | 通过 |

## 2. FINDINGS

### F1 — §3.2「loader 侧返回 `InvalidSegment` 的点是三个」与源码不符（计数不完整）

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:831`（§3.2 末段）
- 严重度：**P2**（事实计数不准；该句不承载后续论证，但它是“错误分层现状”的量化断言）
- 问题：原文说“loader 侧生产代码里返回 `InvalidSegment` 的点是三个：`paddr + memsz` 的 `checked_add`、`kern_end_phys - kern_phys_base` 的 `checked_sub`、段间物理重叠”。实际上 `os/boot-shim/src/loader.rs` 的生产函数 `segment_spans` 另有 6 个返回点，`load_segments_into_buffer` 路径还有 3 个 `usize::try_from`/`checked_add` 返回点，总数远超三个。
- 影响：把“三个”读成穷举会低估错误面的复杂度；§4.2（文档 1377 行）自己又写了“两版共用的 `segment_spans` 拒绝”，两处口径不一致——§4.2 说 `segment_spans` 会拒绝溢出，§3.2 的穷举却把它排除在外。读者按 §3.2 判断“`InvalidSegment` 只可能来自哪三处”会漏掉段内 `filesz`/`offset` 溢出与 `mem_end < file_end`。
- 证据：E1
  - `os/boot-shim/src/loader.rs:271`（`paddr+memsz` `checked_add` → `InvalidSegment`）
  - `os/boot-shim/src/loader.rs:284`（`kern_end_phys - kern_phys_base` `checked_sub` → `InvalidSegment`）
  - `os/boot-shim/src/loader.rs:198`（段间重叠 → `InvalidSegment`）
  - `os/boot-shim/src/loader.rs:414,418,420,422,424,427`（`segment_spans`：`paddr+filesz`、`paddr+memsz`、`mem_end<file_end`、两个 `usize::try_from`、`src_start+src_len`，全部返回 `InvalidSegment`）
  - `os/boot-shim/src/loader.rs:363,365,367`（缓冲区装载路径的同类返回）
  - 对照 `grep -n 'InvalidSegment' os/boot-shim/src/loader.rs` 共 10+ 命中
- 最小修复：把该句改为“`compute_kernel_layout` 与 `validate` 里各两处/一处，另有 `segment_spans` 的 6 处与被调方转换”，或明确写“除 `segment_spans` 外有三处”，并同步 §4.2 的口径。
- 置信度：高（源文件逐点取证）。
- 若我错了：若作者把“loader 侧”窄定义为仅 `compute_kernel_layout` + `KernelImage::validate`，而把 `segment_spans` 归入“共用工具”不计，则该句只需补一个限定语而不是改数字；§4.2 的“两版共用”措辞支持这种读法，但 §3.2 原句未写任何限定，仍属表述不准确。

### F2 — §4.4 aarch64 节选头写“注释未摘录”，但代码块内含源文件行内注释

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1536`（节选头）与 `:1544`（`isb` 行）
- 严重度：**SUGGESTION**（引文自述与内容不一致，不构成事实错误：该注释确在源码中）
- 问题：节选头写“仅保留 asm 块；注释未摘录”，但代码块保留 `"isb", // defensive barrier; enable() already synchronized TTBR/SCTLR`。这与 §4.1 对 `main.rs` 引文的处理（明确写“注释有删节”）不一致。
- 影响：读者无法判断代码块里的行内注释是源码原文还是文档作者补写；引文可核查性下降。
- 证据：E1 —— `os/kernel/src/arch/aarch64/higher_half.rs:60` 的行内注释与文档 1544 行逐字相同。
- 最小修复：节选头改为“仅保留 asm 块（含 `isb` 行内注释）；前置断言与文档注释未摘录”，或删掉该行内注释。
- 置信度：高。
- 若我错了：若“注释未摘录”只指模块级/方法级文档注释（`//!`/`///`），行内 `//` 不在其列，则只需换一种更精确的措辞，仍属 SUGGESTION 级。

### F3 — §2.5 mpx.S 节选省略 `.balign K_STACK_SIZE`，与正文“k_stacks_start/k_stacks_end 每 CPU 两页”的衔接不完整

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:560-576`（节选）与 `:582`（正文）
- 严重度：**SUGGESTION**（节选头已声明“注释有删节”，但未声明代码行省略；正文提到的 `k_stacks_start` 在节选里不存在）
- 问题：源码在 `LABEL(k_boot_stktop)` 与 `LABEL(k_stacks_start)` 之间有 `.balign K_STACK_SIZE`。文档节选把这段落入“…”之后，正文却用括号说 `k_stacks_start`/`k_stacks_end` 是“每 CPU 两页的栈数组”，读者在节选里找不到这两个标签，也无法看出栈数组按 `K_STACK_SIZE` 对齐。
- 影响：引用完整性；不影响 `k_initial_stktop` 落段结论（该结论由 `Makefile.inc` + `kernel.lds` 三条证据支撑，已核对）。
- 证据：E1 —— `minix3/minix/kernel/arch/i386/mpx.S:641-649`（`LABEL(k_boot_stktop)` 后有 `/* top of kernel stack */`，随后 `.balign K_STACK_SIZE`、`LABEL(k_stacks_start)`、`.space 2 * (K_STACK_SIZE * CONFIG_MAX_CPUS)`、`LABEL(k_stacks_end)`）。
- 最小修复：节选头补一句“省略号处含 `.balign K_STACK_SIZE` 与 `k_stacks_*` 定义”，或把 `.balign` 一行保留进节选。
- 置信度：高（省略事实清楚）；对结论无影响。
- 若我错了：若“k_boot_stktop 之后的 10 行”已被头注覆盖（现头注写的是“k_boot_stktop 之后的 10 行”，但省略号画在 `k_boot_stktop` 之前），则属头注位置笔误，仍是 SUGGESTION。

## 3. 未能核实 / 不在本轮范围（明确登记，不当作 FINDING）

以下断言需要运行态或历史材料，本轮未取到 L1 证据；按 Gate 证据规则**不得**记作通过，也不据此报缺陷：

- §5.1「`cargo test -p boot-shim --lib` 默认特性下共 136 个测试」：静态 `#[test]` 计数为 147（dtb_copy 15 + ebs_map 45 + ebs_pair 17 + lib 1 + resv_claims 14 + opensbi_helpers 11 + uefi_helpers 12 + loader 32），差额可由 host 上被 cfg 排除的 UEFI/OpenSBI 模块解释，需实跑才能判定 136 是否正确。
- §5.1「loader 32 个测试中 28 条与本节直接相关」：未逐条分类。
- §5.2 L0–L10 的逐条行为、L9 的 face 选择（已确认脚本 282-287 行确实按架构选 `bootface.rs`/`bootface_a64.rs`），以及 L4 三种平移量读数（`0xffff800000000000`/`0xffff7fffbfe00000`/`0xffffffbf7fe00000`）：未实跑 `check-layout.sh`。
- §6.2「三架构各 4 行 LOAD、依次 .text/.rodata/.data/.bss」：依赖构建产物，当前未构建。
- §5.4 M11/M13 的“已覆盖”判定与 §5.3 `kmain_verify` 的 `INFO`/`FAIL` 分支文本：只核到 `kmain_verify` 存在（anchor 通过），未逐字核对断言文本。
- §2.4 Step 1 的窗口清单（180、1024/256 页预算、`BOOT_WINDOW_FLOOR`）与 `identity_device_windows()` 三份 cfg 定义：只确认函数存在（`os/kernel/src/lib.rs:1074,1081,1086`），未逐条核数值。
- 历史/决策类叙述（D2-01、D2-06、D01-R9、D2-09、NK4-A、wire v10 删除三字段、01 篇各节交叉引用）：按任务约束不读历史论证，未裁决；只确认涉及的符号/文件在树内成立。
- C 侧 `earm` 三文件的具体差异（`.startup`、`MINLIB_OBJS_UNPAGED` 只留 `get_bp.o`、`.data/.bss` 切换）：文件存在且路径正确，内容未逐行对账。

## 4. 结论

- 机械门 8/8 通过；抽查的常量、位值、函数体、宏展开、测试名/计数、链接脚本常量与断言、trait 成员数、错误变体数均与源码一致。
- 未发现 P0 级事实错误；未发现路径、符号、函数签名、错误码或结构体字段层面的错误。
- 发现 1 条 P2（`InvalidSegment` 返回点计数不完整，且与 §4.2 口径不一致）与 2 条 SUGGESTION（节选自述与内容/省略范围）。
- verdict：**PASS WITH FINDINGS**（无 P0；P2 一条需修，其余为可选润色）。

---

审查者：D02 P-FACT（独立事实面板，round1）
审查方式：只读源文件与工具输出；未修改仓库；未读历史面板/账本/归档/expected。
