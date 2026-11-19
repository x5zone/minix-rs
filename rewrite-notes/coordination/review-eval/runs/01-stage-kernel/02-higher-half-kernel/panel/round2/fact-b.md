# D02 R2 · P-FACT-B（Rust 侧事实审查）

- 角色：R2 确认轮独立事实审查者，P-FACT-B，只审 Rust 侧断言（Rust 代码、测试、工具读数）。C 侧断言归 P-FACT-A，本文不裁决。
- 读取范围：目标文档、`tools/doc-code-map.sh <doc> --check` 的 28 个 `os/` 文件、只读命令（含 `cargo test -- --list`）、冻结决策清单 `round1-decisions-snippet.md`。
- 未读取：历史评审、本 run 顶部 V1 段、R1 轨迹、其他面板输出、`expected/`。

## 0. 基线核对

```
$ wc -l rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
1280 rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
$ sha256sum rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
0888ffb0bbb27b4f67560058c91e99e0749addcc72868e41130d44ceb3a557b2
```

与给定基线一致：1280 行，sha256 前缀 `0888ffb0bbb27b4f`。**PASS**。

## 1. 机械门读数

| # | 门 | 命令 | 读数 | 判定 |
|---|---|---|---|---|
| 1 | doc-citation-check | `python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | `blocks=32 checked=1 failed=0 skipped=31 warned=0` | PASS（failed=0） |
| 2 | doc-code-map --check | `bash tools/doc-code-map.sh rewrite-notes/01-stage-kernel/02-higher-half-kernel.md --check` | 28 个 `os/` 文件，`存在性 28/28 通过` | PASS |
| 3 | doc-structure-check | `python3 tools/doc-structure-check.py <doc>` | `✅ doc-structure-check: 1 个文件表结构与内部 .md 链接均可解析` | PASS |
| 4 | doc-ref-check | `bash tools/doc-ref-check.sh <doc>` | `✅ doc-ref-check: ... 内部章节引用均可解析` | PASS |
| 5 | anchor-resolve（Sync-5） | `bash tools/anchor-resolve.sh --check <doc>` | `anchors=56 resolved=56 zero-def=0 multi-def=0` | PASS |

补充：doc-citation-check 的 `checked=1 skipped=31` 是工具口径（只逐字校验带可抽取代码块的引用，其余走块级摘要），不构成失败；`failed=0`。

## 2. Doc Code Sync 七项（Rust 侧）

| 编号 | 检查项 | 命令与读数 | 判定 |
|---|---|---|---|
| Sync-1 | 文档 `static mut` vs 代码现状 | 文档第 872 行提到 `static mut IMAGE_HEAP`；`grep -n 'IMAGE_HEAP' os/kernel-image/src/main.rs` 命中 `521: static mut IMAGE_HEAP: [u8; IMAGE_HEAP_LEN] = [0; IMAGE_HEAP_LEN];` | PASS（文档未过时） |
| Sync-2 | 文档路径与仓库实际 | 28/28 doc-code-map 文件存在；另用脚本核对文档中 52 个字面 `os/` 路径与 30 个花括号展开路径，全部存在（无 MISSING 输出） | PASS |
| Sync-3 | 虚构常量（Rust 侧） | 文档引用的 `KERNEL_HIGH_BASE`、`KERNEL_PHYS_BASE`、`KERNEL_VIRT_BASE`、`KERN_VIRT_BASE`、`KERN_PHYS_BASE`、`KERN_SIZE=0x40_0000`、`HUGE_PAGE_SIZE=1GiB`、`FALLBACK_HUGE_PAGE_SIZE=2MiB` 均在源码实读命中 | PASS（C 侧常量不在本角色射程） |
| Sync-4 | 文档内部待办 | `grep -nE '^\s*>\s*\*\*TODO' <doc>` 无输出 | PASS |
| Sync-5 | 符号锚点可解析 | `anchors=56 resolved=56 zero-def=0 multi-def=0`。另有 3 处 `:fn ...` 简写（`parse_phdr`、`compute_kernel_layout`、`init_clock_and_interrupts`）不被工具解析，但三个符号都实存（`os/libs/minix-elf/src/lib.rs:246`、`os/boot-shim/src/loader.rs:175`、`os/kernel/src/lib.rs:2407`），属写法卫生，不是 0 定义 | PASS |
| Sync-6 | 路径缺 workspace 前缀 | `grep -nE '(^|[^/a-zA-Z_])kernel/src/' <doc>` 无输出 | PASS |
| Sync-7 | 测试数量声称 | 文档声明 minix-elf 23、loader.rs 22、boot-shim `--lib` 125：`cargo test -p minix-elf -- --list` → `23 tests, 0 benchmarks`（另有 1 doctest）；`cargo test -p boot-shim --lib -- --list` → `125 tests, 0 benchmarks`；`grep -c '#\[test\]' os/boot-shim/src/loader.rs` → 22；`grep -c '    fn test_' os/libs/minix-elf/src/lib.rs` → 23 | PASS（偏差 0%，阈值 50%） |

## 3. 测试名与数量对账

### 3.1 minix-elf（文档 §5.1「23 个测试」）

`cargo test -p minix-elf -- --list` 实数 23 个 `tests::test_*`，与文档四族清单逐名对上：

- 文件头五道校验（6 名）：`test_parse_ehdr_valid`、`test_parse_ehdr_bad_magic`、`test_parse_ehdr_too_short`、`test_parse_ehdr_not_64bit`、`test_parse_ehdr_not_le`、`test_parse_ehdr_not_executable`。
- 迭代器行为（8 名）：`test_segment_iter_valid`、`test_segment_iter_no_pt_load`、`test_segment_iter_two_segments`、`test_segment_iter_phdr_out_of_bounds`、`test_segment_iter_phdr_entry_too_small`、`test_segment_iter_skips_segment_data_out_of_bounds`、`test_segment_iter_empty_image`、`test_segment_iter_header_only`。
- 段字段（5 名）：`test_segment_with_zero_filesz`、`test_mixed_pt_load_and_other`、`test_segment_flags`、`test_entry_point`、`test_machine_type`。
- 装载仿真（4 名）：`test_kernel_load_simulation_single_segment`、`test_kernel_load_simulation_two_segments`、`test_kernel_load_simulation_with_bss_segment`、`test_segment_copy_and_bss_fill`。

合计 23，**PASS**。

### 3.2 boot-shim loader（文档 §5.1「loader.rs 部分 22 个」/“直接相关 13 个”）

`cargo test -p boot-shim --lib -- --list | grep loader::tests` 实数 22 条。文档点名的 13 条全部存在：

- `test_compute_kernel_layout_from_mock_elf`、`test_read_required_panics_on_missing_kernel`、`test_bss_region_is_zeroed_after_load`、`test_load_segments_into_phys_memory_rejects_overlapping_elf`、`test_compute_kernel_layout_rejects_filesz_greater_than_memsz`、`test_compute_kernel_layout_rejects_paddr_memsz_overflow`、`test_compute_kernel_layout_zero_size_segment_yields_zero_kern_size`。
- 六个 `first_overlapping_pair_*`：`_disjoint`、`_overlap`、`_abutting`、`_ignores_zero_length`、`_returns_first`、`_empty`。

另外文档 §3.2 引用的 `test_compute_kernel_layout_rejects_image_without_load_segments` 存在。**PASS**。

### 3.3 锁步测试与布局断言

```
$ cargo test -p minix-kernel --features mock --test link_ld_lockstep
running 1 test
test arch_link_ld_constants_match_production_scripts ... ok
test result: ok. 1 passed; 0 failed
```

```
$ SKIP_BUILD=1 bash os/kernel-image/check-layout.sh all
... CHECK-LAYOUT=PASS（all）
```

三架构实测读数与文档 §6.2/§5.3 一致：

- LOAD 行数：x86_64 = 5、aarch64 = 4、riscv64 = 4（`readelf -lW ... | grep -c '^  LOAD'`）。
- L4 平移量：x86_64 = `0xffff800000000000`、aarch64 = `0xffff7fffbfe00000`、riscv64 = `0xffffffbf7fe00000`，与文档 §6.2 逐字对上。
- L5 三架构均 `4 MiB 且 2MiB 对齐（余 0 字节）`；L9 `ELF 实测 0x400000 = bootface.rs KERN_SIZE 0x400000`。
- x86_64 段序与节归属：`00 .text`、`01 .lrodata.*`、`02 .rodata .eh_frame_hdr`、`03 .data`、`04 .bss`，与文档 §6.2 的五行描述一致。
- 入口：`readelf -hW` 三架构均 `EXEC`，Entry 分别为 `0xffff800000200000` / `0xffff800000000000` / `0xffffffc000000000`，等于各自首段 VMA，与文档“入口等于镜像首字节”的等式一致。

### 3.4 数值复算

```
$ printf '%d\n' $((0xF0400000 / 0x400000))     # 961
$ printf '0x%x\n' $((0xF0400000 - 0x00400000)) # 0xf0000000
$ printf '0x%x\n' $(( (0xFFFFFFC000000000 >> 30) & 0x1FF ))  # 0x100
# 换成 0xFFFFFC0000000000 / 0xFFFFC00000000000 均输出 0x0
```

与文档 §2.2、§4.1、§6.2 的读数一致。riscv64 注释算式也已复算：源码注释的加数 `0xffffffbf7ffe0000` 确实多了一个 `f`（按它算得 `0xffffff7f7ffe477a`），按真实 delta `0xffffffbf7fe00000` 算得 `0xffffff7f7fe0477a`，正是注释所写的故障地址——文档 §4.3 的“原文加数多写了一个 f、与自身结果不自洽”成立。

## 4. 符号 / 路径 / 常量 / 签名 / 行为合同核对

下表逐条给出文档断言、实读位置与判定；只有出现偏差的才展开为 FINDING/SUGGESTION。

| 文档断言（位置） | 实读核对 | 判定 |
|---|---|---|
| §3.1 `os/kernel-image/*.ld` 三份都有 `AT()`；x86_64 首段 VMA = `KERNEL_HIGH_BASE + KERNEL_PHYS_BASE` | `x86_64.ld`: `KERNEL_HIGH_BASE = 0xFFFF800000000000`、`KERNEL_PHYS_BASE = 0x200000`、`SECTIONS { . = KERNEL_HIGH_BASE + KERNEL_PHYS_BASE; ... }`，四段均 `AT(ADDR(sec) - KERNEL_HIGH_BASE)` | 一致 |
| §3.1 aarch64/riscv64 起点 `KERNEL_VIRT_BASE`，`AT(ADDR(sec) - KERNEL_VIRT_BASE + KERNEL_PHYS_BASE)` | `aarch64.ld`（`0xFFFF800000000000` / `0x40200000`）、`riscv64.ld`（`0xFFFFFFC000000000` / `0x80200000`）逐字符合 | 一致 |
| §3.1 三份 `os/kernel/src/arch/*/link.ld` 声明 `KERN_VIRT_BASE`/`KERN_PHYS_BASE` 且无 `AT()`；x86_64 头注释 `No AT() directives — boot-shim handles LMA placement` | 三份脚本实读无 `AT(`；x86_64 头注释逐字命中；常量与生产脚本相等（0xFFFF800000000000/0x200000、0xFFFF800000000000/0x40200000、0xFFFFFFC000000000/0x80200000） | 一致 |
| §3.1 本仓构建脚本检索不到把 arch `link.ld` 作链接输入（声称空命中） | `grep -rn 'src/arch/.*link\.ld' os/qemu-tests/*.sh os/xtask/src/*.rs` → 无输出（exit 1）；放宽到全仓 `*.rs/*.toml/*.sh/Makefile` 只命中 `os/arch/src/arch/direct_map.rs` 三处文档注释与 `link_ld_lockstep.rs` 的读取 | 一致（阴性声明成立） |
| §3.1 测试载体 link.ld：`. = 0x80200000`、无 `AT()`、`__stack_top = . + 0x10000` | `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half-riscv64/link.ld` 逐字符合；x86_64/aarch64 的 `test-higher-half` 目录确无 `link.ld` | 一致 |
| §3.1 `link_ld_lockstep.rs` 只锁常量、不证明脚本被链接 | 测试体只有 `arch_link_ld_constants_match_production_scripts`，读两份脚本常量并 `assert_eq` | 一致 |
| §3.1 七块载荷 memmap/reserved/modules/platform/param/sources/startup 落 `.bss`、记录活动根、建 DM、不重建根 | `arch_boot_resume_v1` 逐块 `decode_*` 到 store；`resume_additive` 做 `set_current_root_phys` → `store_kernel_info` → `establish_boot_dm` → `jump_to_kmain` | 一致 |
| §3.2 minix-elf 被五个 crate 依赖 | `grep -rl 'minix-elf =' os/*/Cargo.toml os/*/*/Cargo.toml` → arch、boot-shim、vfs、rs、vm 五行 | 一致 |
| §3.2 四类调用点：boot-shim 装内核、`load_vm_elf`、`trap_api` exec、`vm_server` bootstrap；模块整块拷贝不经段迭代 | `load_boot_modules_into` 无 `segment_iter`；`os/arch/src/arch/boot.rs:461/464`；`os/servers/rs/src/trap_api.rs:443/469`；`os/servers/vm/src/vm_server.rs:890/892` | 一致 |
| §3.2 vfs exec 只用 `ET_EXEC`/`PT_LOAD` 常量，表头解析是自带 `ElfHead` | `exec_worker.rs:216` 注释与 `struct ElfHead`（:219）；生产解析不用 `SegmentIter` | 一致 |
| §3.2 结构体字段/丢失字段/`p_align` 无读者/`p_flags` 内核装载不用 | `os/libs/minix-elf/src/lib.rs:93-154` 字段与注释；全仓无 `seg.align` 生产读点；`load_segments_into_phys_memory`/`compute_kernel_layout` 不读 flags | 一致 |
| §3.2 字段偏移表 16/18/24/32/54/56 与 0/4/8/16/24/32/40/48 | `parse_ehdr`/`parse_phdr` 逐行实读一致 | 一致 |
| §3.2 九个 `ElfError` 变体；`InvalidSegment` 四来源；`NoLoadSegments` 只由 loader 产生 | 枚举九变体实读；`grep -n InvalidSegment loader.rs` → 190/195/207/290；minix-elf 只定义不构造；`compute_kernel_layout` 的 `!found_load` | 一致 |
| §3.3 `KernelInfo.kern_stack_top`；`build_kernel_info` 赋值 `kern_virt_base.0 + kern_size`；`BootStorage::report` 第二处同式 | `os/boot-shim/src/lib.rs:125` 逐字；`os/libs/minix-boot/src/boot_storage.rs:378` 逐字；`kernel_info.rs:107` 字段存在 | 一致 |
| §3.3 `boot_validate_and_prepare` 的 `kern_stack_top must be 16-byte aligned` 断言 | `os/kernel/src/lib.rs:878-882` 逐字 | 一致 |
| §3.3 `.bss` 64 KiB 引导栈 `kernel_boot_stack_bottom/top`，三份 image ld 同形；与 `__bss_end` 相邻赋值；`lea rsp,[rip+...]` 等四条入口序列 | `x86_64.ld:715-723`、`aarch64.ld:86-88`、`riscv64.ld` 同形；`os/kernel-image/src/main.rs` 四处 `global_asm!`（x86 `lea rsp`、aarch64 `adrp`+`add`、riscv direct `la sp`、riscv U-Boot `la sp`） | 一致 |
| §3.3 `syscall_entry`：安装用 `minix_arch::syscall_entry_va()`，生产路径没有读者，唯一真实读者是 `test-protection-riscv64` | 安装点 `os/kernel/src/lib.rs:2276`；`test-protection-riscv64/src/main.rs:148` 是唯一把它当入口的读者。**但生产路径并非没有读者**：`validate_requiring_sources`（`kernel_info.rs:458-461`）、`check_boot_postconditions`（`kernel_info.rs:581-590`，调用点 `os/kernel/src/lib.rs:224/739/1727`）、`handoff_v1_write.rs:143` 的 `debug_assert` 都读取该字段 | **偏差 → FINDING RD2-FACT-B-01** |
| §3.3 strict 档断言 `syscall_entry == kern_virt_base` | `kernel_info.rs:458-461` 逐字 | 一致 |
| §3.4 `trait HigherHalf` 签名 `unsafe fn jump_to_kmain(kinfo: &KernelInfo, stack_top: VirBytes) -> !`；Safety 四条 | `os/kernel/src/boot/higher_half.rs:31-55` 逐字（四条 bullet 对得上） | 一致 |
| §3.4 三份实现各自模块、`arch_boot` 三份 `#[cfg]` 分支；两个生产调用点都传 `kinfo.kern_stack_top` | `arch/x86_64|aarch64|riscv64/higher_half.rs` 三个 `impl HigherHalf`；`os/kernel/src/lib.rs:232/375` 两处 `H::jump_to_kmain(kinfo, kinfo.kern_stack_top)` | 一致 |
| §3.4 x86_64 `call` 无运行时检查；riscv64 `auipc/bltu` 条件补 delta；aarch64 `ldr` 绝对值 | 三份 asm 实读逐字符合 | 一致 |
| §3.5 `plan_kernel_mapping` 选择阶梯（首选三量整除 → 回退 → `FallbackUnsupported`/`NoKernelPageSize`） | `os/kernel/src/boot_policy.rs:122-133` 与节选逐字一致；前置校验 :102-120 与“前置校验略”相符 | 一致 |
| §3.5 panic 消息 `arch_boot: page-size policy: ...` / `... no kernel page size satisfies the layout` / `fallback page size … is not supported` | `os/kernel/src/lib.rs:947`、`boot_policy.rs:49/51` 逐字 | 一致 |
| §3.5 三架构 `HUGE_PAGE_SIZE = 1GiB`、`FALLBACK_HUGE_PAGE_SIZE = 2MiB`、当前工件跨距 4MiB 两次步进 | `os/arch/src/{x86_64,arm64,riscv64}/paging.rs` 常量实读；check-layout L5 输出 4 MiB | 一致 |
| §3.5 `riscv64.ld` `.bss` 末尾 `ALIGN(0x200000)` 及其注释；`bootface.rs KERN_SIZE = 0x40_0000`；L9 对账 | `riscv64.ld` 注释与 `ALIGN` 逐字；`bootface.rs:121`；L9 PASS | 一致 |
| §4.1 `build.rs` 按 `--target` 选脚本、宿主 `None` 不追加参数 | `os/kernel-image/build.rs:19-31` 节选逐字；`Cargo.toml` 只有 `[[bin]] name = "kernel"`，无 `[lib]` | 一致 |
| §4.1 `KEEP(*(.text.boot))` 首位、`_start` 在 `.text.boot`、`KERNEL_ENTRY_ANCHOR` 与 `.rodata.kernel_anchor` | `x86_64.ld` 实读；`main.rs:111-114` `#[used] #[unsafe(link_section = ".rodata.kernel_anchor")] static KERNEL_ENTRY_ANCHOR`；L6a/L8 PASS | 一致 |
| §4.1 `.data` 收 `.ldata`、`.bss` 收 `.lbss`；末尾 `ALIGN(0x200000)` 在 `.bss` 括号内 | `x86_64.ld`/`aarch64.ld`/`riscv64.ld` 实读 | 一致 |
| §4.1 `/DISCARD/` 段、riscv64 多 `.riscv.attributes`、x86_64 无 `OUTPUT_ARCH`、aarch64/riscv64 有 `ASSERT` | 四份脚本实读 | 一致 |
| §4.1 VPN[2] 复算 0x100/0x0/0x0；riscv64 基址 0xFFFFFFC000000000；direct_map VPN[2]=257 | 命令输出 0x100/0x0/0x0；`os/arch/src/arch/direct_map.rs:346-368` | 一致 |
| §4.1 各架构 `map_huge` 命中“叶子降级为表指针”时返回 `AlreadyMapped` | x86_64 `e3` 1GB 叶检查（paging.rs:1212-1216）、arm64 `e1` 1GB block 检查（:1183-1189）、riscv64 `e2` leaf 检查（:1174-1182） | 一致（针对“降级”动作） |
| §4.2 `parse_ehdr` 五道校验次序、`ET_EXEC` 拒绝 static-pie；`os/.cargo/config.toml` `relocation-model=static` | `lib.rs:185-218` 逐字；config `[target.x86_64-unknown-none]` 实读，注释明说 freestanding user images 需 ET_EXEC | 一致 |
| §4.2 `compute_kernel_layout` 函数体（极值、filesz>memsz、checked_add/sub、NoLoadSegments） | `loader.rs:175-212` 与节选逐字（注释省略已声明） | 一致 |
| §4.2 `load_segments_into_phys_memory` 函数体；段间重叠→`InvalidSegment`；源/目标重叠→panic；逐段拷贝+清零 | `loader.rs:286-314` 逐字；`first_overlapping_pair` :348-373 半开区间；`first_source_destination_overlap` :321-346 panic | 一致 |
| §4.2 缓冲孪生：`file_end <= buf.len()`/`mem_end <= buf.len()` 上界、无重叠检测；物理版不拒绝 filesz>memsz/溢出，`load_kernel_with_loader` 保证先算布局 | `loader.rs:224-278`、`286-314` 的 doc comment 与函数体实读 | 一致 |
| §4.3 x86_64/aarch64/riscv64 asm 节选逐字；`in("t2") delta`/`in("t4") kvirt`；`a0/a6/a7` 约束 | 三份 `higher_half.rs` 实读，含 `"Do NOT use a0/a7/a6 inside this asm block!"` | 一致 |
| §4.3 delta 注释算术与 `checked_sub` 前提 | 复算见 §3.4；`higher_half.rs` 的 `checked_sub(...).expect(...)` 实读 | 一致 |
| §4.3 生产 `kmain` 为普通 Rust 函数、序言不保存 `%rbp` | `kmain` 仅 `#[cfg(feature="qemu_test")]` 裸函数；`objdump` release 工件 `_ZN12minix_kernel5kmain...` 序言 `push %r15/%r14/%r12/%rbx`，无 `%rbp` | 一致（当前工件实证） |
| §4.4 `bootface.rs` 的 `build_kernel_info` 调用实参与顺序；`KERN_*` 常量；`validate_requiring_sources` 后调 `arch_boot` | `bootface.rs:427-441` 逐字（`KernelInfo` 组装段）；`KERN_VIRT_BASE/KERN_PHYS_BASE/KERN_SIZE` :108-121；末尾 `minix_kernel::arch_boot` | 一致 |
| §4.4 `place_modules`：整块拷贝 + 只清尾余量 + 名字池 | `bootface_common.rs:177-257`：`copy_nonoverlapping(len)`、`write_bytes(dest+len, 0, pages*4096-len)`、`pool.names + i*NAME_SLOT` | 一致 |
| §4.4 12 个模块；`MODULE_NAMES`/`BOOT_MODULE_PROC_NRS` 对账入口 | `kernel_info.rs:677 NR_BOOT_MODULES = 12`；`os/kernel/src/proc.rs:111` 常量存在 | 一致 |
| §4.4 `store_kernel_info` 把 reserved/memmap/modules 落 `.bss`；`kmain` 第一条语句遮蔽形参；早期指针读数 0x1ddda2ee | `os/kernel/src/lib.rs:1453-1563` 实读；`kmain` :1687-1688；`0x1ddda2ee` 在 :248 注释 | 一致（引文位置性质见 SUGGESTION 2） |
| §4.4 两块临时取证打印 `kernel: arch_boot arg=`/`kernel: preA rdi=`，注释自标 temporary | `os/kernel/src/lib.rs:252/257`、`1689/1707` 逐字 | 一致 |
| §5.2 `kmain_verify` 三条断言代码；naked asm 捕获序列（x86 `mov rdx,rsp`+`lea r8,[rip]`、aarch64 `mov x1,sp`+`adr x2,.`、riscv64 `mv a1,sp`+`auipc a2,0`） | `os/kernel/src/lib.rs:2110-2180`、2075-2098 逐字 | 一致 |
| §5.2 `PC` 只打印不判定、INFO 只在失败分支；测试载体合成值（x86_64 0xFFFF800000000000/0x200000/0xFFFF800000200000） | `kmain_verify` :2157-2179；`test-higher-half/src/main.rs:49-56` 实读，三者相加等式成立 | 一致 |
| §5.2 `run_qemu.sh`：`-serial file:`、`strings | grep '### TEST_RESULT: PASS'`、0=PASS/SKIP、1=FAIL | `run_qemu.sh:105/191-215` 实读 | 一致 |
| §6.2 `grep -c ': AT('` 生产三份各 4、arch 三份各 0；放宽 `AT(` 时 x86_64 生产仍 4、另两份 5、arch/x86_64 变 2 | 实跑输出 `4/4/4/0/0/0` 与 `x86_64 4, aarch64 5, riscv64 5, arch/x86_64 2` | 一致 |
| §6.2 LOAD 行数、x86_64 五行段序、首段 PhysAddr = KERNEL_PHYS_BASE | 见 §3.3；三架构 `readelf -lW` 实读 | 一致 |
| §6.2 `nm <工件> | grep kmain` 与 `$rip` 对比 | 工件中符号为 `_ZN12minix_kernel5kmain17h...`，`grep kmain` 可命中 | 一致（未加 `-w` 时） |
| §3.3/§4.3 `syscall_entry` 生产读者 | 见上 | **偏差 → FINDING RD2-FACT-B-01** |

结论：除 FINDING RD2-FACT-B-01 一处阴性声明过宽外，Rust 侧符号、路径、常量、签名、行为合同的断言全部与当前源码/工件一致。

## 5. 引文逐字 / 省略核对

以下逐字核对（“节选”类按文档声明的省略/注释改写规则处理）：

| 文档引文 | 源码 | 判定 |
|---|---|---|
| §3.1 x86_64.ld 基址与四段头 | `os/kernel-image/x86_64.ld` | 逐字一致 |
| §4.1 x86_64.ld `.text`/`.rodata`/`.data`/`.bss`/`/DISCARD/` 节选 | 同上（段体注释省略已声明） | 逐字一致，省略位置与声明相符 |
| §3.5 riscv64.ld `.bss` 收口注释与 `ALIGN` | `os/kernel-image/riscv64.ld` | 逐字一致 |
| §3.1 arch x86_64 `link.ld` 头注释 `No AT() directives — boot-shim handles LMA placement` | `os/kernel/src/arch/x86_64/link.ld:8` | 逐字一致 |
| §4.1 `build.rs` 选脚本节选 | `os/kernel-image/build.rs` | 逐字一致 |
| §3.2 三个结构体节选 | `os/libs/minix-elf/src/lib.rs` | 字段名/类型一致；注释按声明改写为行内注释 |
| §3.2 `SegmentIter::next` 节选 | 同上 :352-406 | 循环体、`continue`、越界检查逐字一致；`...` 处为 `return Some(LoadSegment{...})` |
| §4.2 `parse_ehdr` 校验段 | 同上 :185-218 | 逐字一致 |
| §4.2 `compute_kernel_layout` 函数体 | `os/boot-shim/src/loader.rs:175-212` | 逐字一致（注释省略已声明） |
| §4.2 `load_segments_into_phys_memory` 函数体 | 同上 :286-314 | 逐字一致；源/目标重叠段按声明以 `...` 省略 |
| §3.3 `build_kernel_info` 赋值行 | `os/boot-shim/src/lib.rs:125` | 逐字一致 |
| §3.5 `plan_kernel_mapping` 选择阶梯 | `os/kernel/src/boot_policy.rs:122-133` | 逐字一致 |
| §4.3 三份 asm 块 | 三份 `higher_half.rs` | 逐字一致（riscv 的 `.kern_virt_base`/`.0` 合并为一行，语义不变） |
| §4.4 `bootface.rs` `build_kernel_info` 调用 | `os/kernel-image/src/bootface.rs:427-441` | 逐字一致；中间 `kernel_info.validate_requiring_sources();` 以 `...` 省略 |
| §5.2 `kmain_verify` 三条断言 | `os/kernel/src/lib.rs:2144-2155` | 代码逐字一致；注释按声明“有删改” |
| §4.4 `store_kernel_info` 引文 `The reserved_regions payload lives in the boot-shim's UEFI-pool heap…` | `os/kernel/src/lib.rs:1457-1464` | 文本逐字存在，但位于函数体 `//` 注释而非 `///` 文档注释（见 SUGGESTION 2） |
| §3.3 `kernel_info.rs` 字段注释 “reference-only metadata” | `os/libs/minix-boot/src/kernel_info.rs:109-117` | 注释含 “this field is for reference only”，但同段首句说 “only x86-64 uses it to configure LSTAR MSR”，与生产代码不符；文档只引了后半句（见 SUGGESTION 1） |
| §4.4 临时打印字符串 | `os/kernel/src/lib.rs:257/1707` | 逐字一致 |
| §5.2 `INFO: PC not at high address (expected in test kernel)` | `os/kernel/src/lib.rs:2171` | 逐字一致 |
| §6.2 `CHECK-LAYOUT=PASS（x86_64）` 与 L4 输出形态 | `check-layout.sh:47-55`、实跑输出 | 逐字/格式一致（PASS 后两个空格） |

## 6. 阴性声明核对

| 阴性声明 | 核对 | 判定 |
|---|---|---|
| §3.1 构建脚本无 arch `link.ld` 链接读者 | 见 §4 表；`grep` 空命中；`link_ld_lockstep.rs` 只读常量 | 成立 |
| §3.2 `p_align` 无读者 | 全仓无 `seg.align` 生产读点 | 成立 |
| §3.2 `p_flags` 内核镜像装载不用 | `compute_kernel_layout`/`load_segments_into_phys_memory` 不读 flags | 成立 |
| §3.2 符号表/重定位/section header 不在射程 | minix-elf 模块文档与源码实读 | 成立 |
| §3.2 `kern_end - kern_base` 的 `checked_sub` 正常输入走不到 | 每段 `paddr+memsz >= paddr >= min(paddr)`，逻辑成立 | 成立 |
| §3.3 `syscall_entry` 生产路径没有读者 | 见 FINDING RD2-FACT-B-01 | **不成立（过宽）** |
| §3.3 `kern_stack_top` 不是链接符号 | `build_kernel_info`/`BootStorage::report` 计算式实读；无同名生产链接符号被内核消费 | 成立 |
| §4.1 x86_64.ld 无 `OUTPUT_ARCH`、无 `ASSERT` | 实读 | 成立 |
| §4.1 不能把“漏收 ⇒ L5 必红”当断言（`.lrodata.*` 落在收口前仍 PASS） | 实读 x86_64 段映射 + L5 PASS | 成立 |
| §4.2 `load_segments_into_phys_memory` 自身不拒绝 filesz>memsz/溢出 | 函数体与 doc comment 实读 | 成立 |
| §4.3 x86_64 侧无运行时“调用点=链接地址”检查 | x86_64 asm 无检查 | 成立 |
| §4.3 aarch64 删 `isb` 不产生按旧映射取指失败（防御性保留） | `enable()` 在 TTBR 写后 `dsb sy; isb`、SCTLR 写后 `isb`；三条路径都经 `build_bootstrap_root_and_enable` → `enable()` | 成立（论证与源码一致） |
| §5.1 宿主测试不证明按 `p_paddr` 写物理地址 | 唯一执行者 `load_segments_into_phys_memory`，宿主用 buffer 孪生 | 成立 |
| §5.2 PC 不在成败判定 | `kmain_verify` 实读 | 成立 |
| §5.4 L7 检查 Rust `kernel_boot_stack_*` 而非 C `k_initial_stktop` | `check-layout.sh` L7 用 `nm` 查 `kernel_boot_stack_bottom/top` | 成立 |

## FINDING RD2-FACT-B-01（P2）

- **位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:531`（§3.3，`syscall_entry` 段首句）。
- **严重度**：P2（Rust 侧事实错误 / 阴性声明过宽；不是核心行为错误）。
- **问题**：文档写「生产路径没有读者」。实读生产启动路径对该字段有三处读取：
  1. `KernelInfo::validate_requiring_sources` 的 strict 档：`os/libs/minix-boot/src/kernel_info.rs:458-461` 的 `assert_eq!(self.syscall_entry.0, self.kern_virt_base.0, ...)`；
  2. `KernelInfo::check_boot_postconditions`：`os/libs/minix-boot/src/kernel_info.rs:581-590` 读 `self.syscall_entry` 做「落在内核镜像内」的范围断言；该函数在 `os/kernel/src/lib.rs:224`（`arch_boot_finish`，direct 生产路径）、`:739`（`arch_boot_resume_v1`，v1 生产路径）、`:1727`（`kmain`）三处无条件调用；
  3. v1 发送侧 `os/libs/minix-boot/src/handoff_v1_write.rs:143` 的 `debug_assert_eq!(report.syscall_entry.0, report.kern_virt_base.0)`（debug 构建）。
  真正成立的是更窄的命题：**没有生产代码把该值当作 SYSCALL 入口安装**——安装点用 `minix_arch::syscall_entry_va()`（`os/kernel/src/lib.rs:2276`），全仓唯一把它传给 `configure_syscall` 的读者是 `test-protection-riscv64`（`:148`）。文档紧接着自己也承认 `validate_requiring_sources` 仍读它，与前一句「没有读者」自相矛盾。
- **影响**：读者（以及 D2-03「线协议 v2 删除该槽位」的执行者）可能据此认为只需处理「安装消费者」，漏掉 `check_boot_postconditions` 的范围断言与 v1 写入侧的 `debug_assert`，删字段后会得到编译失败或启动期断言失败；也削弱文档阴性声明的可信度。
- **证据或推理**：
  ```
  $ grep -rn "check_boot_postconditions()" os/kernel/src
  os/kernel/src/lib.rs:224:    kernel_info.check_boot_postconditions();
  os/kernel/src/lib.rs:739:    kinfo.check_boot_postconditions();
  os/kernel/src/lib.rs:1727:    kernel_info.check_boot_postconditions();
  $ sed -n '581,590p' os/libs/minix-boot/src/kernel_info.rs
      let entry_off = self.syscall_entry.0.checked_sub(self.kern_virt_base.0)...
      assert!(entry_off < self.kern_size, "KernelInfo postcond: syscall_entry ...");
  $ grep -rn "\.syscall_entry" os/libs/minix-boot/src
  ... kernel_info.rs:459 (assert_eq), :582 (postcond), handoff_v1_write.rs:143/154/167 ...
  $ grep -rn "configure_syscall(" os --include='*.rs'
  os/kernel/src/lib.rs:2276: trap.configure_syscall(syscall_entry_va());
  os/qemu-tests/.../test-protection-riscv64/src/main.rs:148: trap.configure_syscall(info.syscall_entry);
  ```
- **最小修复**：把首句改为范围明确的表述，例如：「生产路径没有把该值当作 SYSCALL 入口的消费者——内核装入口用的是 `minix_arch::syscall_entry_va()`；但验证/序列化路径仍读取它：`validate_requiring_sources` 断言它等于 `kern_virt_base`、`check_boot_postconditions` 断言它落在镜像内、`handoff_v1_write` 的 `debug_assert` 断言两者相等。全仓唯一把它当入口传入 `configure_syscall` 的读者是 `test-protection-riscv64` 载体。」（下一句关于 strict 档的说明可并入，避免重复。）
- **置信度**：高（读点与调用点均为实读；只有「读者」的语义边界可争）。
- **是否需源码核实**：已核实（只读命令），无需再取证；若后续代码删除 `check_boot_postconditions` 的该断言，本条需重估。
- **若我错了**：若作者把「读者」定义为「消费该值作为 SYSCALL 入口地址的代码」，则「没有读者」在意图上成立；即便如此，正文应先给出这个限定词，否则与紧随其后的 validate 句冲突。
- **对应判据**：阴性声明（本文 §6）；Doc Code Sync-2（文档与代码一致性，`prompt/review-rules/review-doc-checklist.md` §4 / §2.4）。

## SUGGESTION

1. **§3.3（:531）`kernel_info.rs` 字段注释的引证方向反了**：该字段注释（`os/libs/minix-boot/src/kernel_info.rs:109-117`）首句写的是「only x86-64 uses it to configure LSTAR MSR」，与生产代码（`:2276` 用 `syscall_entry_va()`）不符，是同源注释漂移；只有后半句「this field is for reference only」支持文档的用法。建议改引使用点注释 `os/kernel/src/lib.rs:2268` 的「KernelInfo.syscall_entry is reference-only boot metadata」或 `os/arch/src/lib.rs:374`，避免读者按图索骥时先读到相反结论。
2. **§4.4（:1064）「`lib.rs` 的 `fn store_kernel_info` 文档注释」措辞不准**：被引的 `The reserved_regions payload lives in the boot-shim's UEFI-pool heap…` 位于函数体 `//` 注释（`os/kernel/src/lib.rs:1457-1464`），不是函数上方的 `///` 文档注释。建议改为「函数体注释」或直接指向正文行。
3. **§4.1（:743）段末总结漏项**：同段刚列出的五处差异里，`OUTPUT_ARCH`（aarch64/riscv64 有、x86_64 无）与链接期 `ASSERT`（aarch64/riscv64 各一条、x86_64 无）也是 aarch64 与 x86_64 之间的差异；末句「差异只在基址、`AT` 表达式和 x86_64 这两条专属条目」与同段枚举不自洽。建议把末句限定为「段序与收段通配相同」。
4. **§5.1（:1100）「§4.2 那条链上每个环节都有宿主测试」过宽**：同节 :1115 明确说「不证明按 `p_paddr` 写物理地址这一步」，唯一执行者 `load_segments_into_phys_memory` 没有宿主直测。建议改为「解析、布局计算与缓冲孪生环节都有宿主测试」。
5. **§4.4（:1064）「不属于 §6.1 的正式路标」指向不准**：§6.1 是「症状 → 怀疑对象 → 确认命令」表，没有路标清单；路标/判定标记的说明在 §6.2 与源码 `boot_stage!` 处。建议核对后改指 §6.2，或删去该从句。
6. **§4.3（:589/:902）「`naked_asm!` 只用于 `qemu_test` 载体」在全仓口径下不成立**：`minix-kernel` 内确实只有 `qemu_test` 的裸 `kmain`；但 `os/libs/minix-rt/src/crt0.rs` 与 `signals.rs` 也用 `naked_asm!`（用户态启动/信号桩）。建议加限定「内核侧的 `kmain` 只在 `qemu_test` 形态下用 `naked_asm!`」。
7. **§6.2（:1230/:1239）GDB 预期读数的括注只覆盖 x86/aarch64**：「`$rsp` 落在 `kern_virt_base` 之上（`0xffff8000…` 一段）」；riscv64 生产镜像的高半基址是 `0xffffffc0…`。建议补一句「riscv64 为 `0xffffffc0…` 一段」，与同段 L4 平移量按架构分列的口径一致。

## 覆盖与限制声明

- 本报告只裁决 Rust 侧断言；C 源断言（`minix3/` 的链接脚本/汇编/常量）未纳入判断，仅确认 doc-citation-check 的块级引用 `failed=0`。
- 逐条核对覆盖：Rust 侧符号/路径断言 56 个锚点全部解析；28 个 doc-code-map 文件全部存在；文档中 52 个字面 `os/` 路径与 30 个花括号展开路径全部存在；测试名 23 + 22 条逐名对上；三份 arch asm、trait、loader、minix-elf、boot_policy、bootface、kernel_info、build.rs、三份生产 ld 与三份 arch link.ld 的代码引用逐字或按声明省略核对；工具读数（check-layout、readelf、cargo --list、grep 复算）全部现场重跑。
- 统计：FINDING 1 条（P2）；SUGGESTION 7 条；P0/P1 = 0。没有发现符号不存在、常量值错误、测试名虚构、引文改写、路径断链一类的事实错误。
- 同 agent 限制：本报告由单个只读审查者完成，所有结论均附命令与输出；未使用 `expected/`、历史评审或其他面板输出作为依据。
