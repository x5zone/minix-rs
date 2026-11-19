# P-FACT-B 独立事实审查报告（R34 确认轮，只审 Rust 侧断言）

## 基线核对

```
$ wc -l rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
3565 rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md

$ sha256sum rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
d909c236a373636678e66b13c0ad5ebde769ead738f09d81a846803647fcb98c  rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
```

与任务给定基线一致：3565 行，sha256 前缀 d909c236a3736366。

## 机械门读数（仓库根执行）

| 门 | 命令 | 读数 | 判定 |
|---|---|---|---|
| 引文对账 | `python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | `blocks=69 checked=29 failed=0 skipped=39 warned=1` | 通过（0 失败；1 警告未列出细节，需人工看） |
| 文档-代码映射 | `tools/doc-code-map.sh rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md --check` | 54 个文件，存在性 54/54 通过 | 通过 |
| 表结构/内部链接 | `tools/doc-structure-check.py …` | `✅ doc-structure-check: 1 个文件表结构与内部 .md 链接均可解析` | 通过 |
| 内部章节引用 | `tools/doc-ref-check.sh --check …` | `✅ doc-ref-check: … 内部章节引用均可解析` | 通过 |
| 文风 lint | `tools/doc-style-lint.sh rewrite-notes/…/01-boot-shim-bootstrap.md` | `命中汇总：\| error级=0`，exit 0 | 通过 |

工具能判定的部分（路径存在性、内部链接、引文块能否被脚本解析）不再手工重做；下文只写工具判不了的部分。

## 阴性声明（本轮扫了什么、发现数、N=0 不覆盖什么）

- **Rust 符号/路径存在性**：扫了文档全部 `os/…` 路径与 `<os/…:sym>` 锚点（doc-code-map 54/54 通过），另手工 grep 了 41 个被点名的 Rust 符号（`prepare_boot`、`entry_path_eq`、`UbootFileLoader`、`build_kernel_info`、`land_slice`、`store_kernel_info`、`BootWindows::push/check`、`CheckedWindows`、`identity_device_windows`、`device_window_regions`、`kernel_identity_mmio_regions`、`install_at`、`inherit_supervisor_half`（三架构）、`cross_space_copy`、`getinfo_kinfo`、`register_free`、`is_free_registered` 等）。发现数 0（全部可解析）。
- **常量与容量**：逐个核对了 26 个文档点名的常量值（`BOOT_PT_BUMP_PAGES=1024`、两个 bootface 的 `PT_BUMP_PAGES=256`、`RESERVED_REGION_STORE_LEN=256`、`MEMMAP_REGION_STORE_LEN=128`、`PLATFORM_SOURCE_STORE_LEN=4`、`PARAM_BUF_STORE_LEN=1024`、`WIRE_SOURCES_CAP=296`、`WIRE_STARTUP_CAP=20`、`BOOT_MODULE_NAME_LEN=16`、`VM_BOOT_HANDOFF_MAX_REGIONS=64`、`VM_BOOT_HANDOFF_MAX_DEDUCTED=47`、`VM_BOOT_HANDOFF_VERSION=9`、`BOOT_HANDOFF_V1_VERSION=9`、`ARENA_CAP=32768`、`ENVELOPE_CAP=256`、`WIRE_END=7872`、`WIRE_SOURCES_END=29184`、`WIRE_STARTUP_END=30624`、剩余 2144、`MAX_BOOT_WINDOWS=32`、`AP_STARTUP_PA=0x5000`、`BOOT_IDENTITY_MAP_END=0x1_0000_0000`、`KERN_SIZE=0x40_0000`、`riscv64.ld KERNEL_VIRT_BASE=0xFFFFFFC000000000`/`KERNEL_PHYS_BASE=0x80200000`）。发现数 0。
- **测试计数**：逐文件 `grep -c '#[test]'` 与文档 §5.2.1 的逐文件表**逐格一致**（boot-shim 7 个 lib 文件 22/11/12/45/17/15/13/1 = 136，另有 ebs_boundary_audit 6；minix-boot 32/8/8/74/2/10/21/29/2/5/5 = 196；minix-elf 23）。`cargo test -p minix-kernel --features mock --lib -- --list` 实测 `874 tests`，与文档"874 条（866 passed + 8 ignored）"总数吻合；`grep '#[ignore'` 实测 8 枚真实 ignore。发现数 0。
- **测试名**：核对了文档点名的 31 个测试函数名（loader.rs 11、opensbi_helpers.rs 4、uefi_helpers.rs 12 + 5 枚 assert_bootstrap/5 枚 assert_range、boot_params.rs 5、kernel_info.rs 32 的四个系列计数、boot_alloc/vm_handoff/dm_coverage/paging/lib.rs、pt_alloc_write_once/concurrent、`missing_boot_allocator_registration_fails_closed`、`picks_the_preferred_size_when_supported_and_aligned`）。发现数 0（唯一被划掉的 `test_build_kernel_info_bootstrap_zero_means_no_reclaim` 确实已不存在，文档已标删除）。
- **引文块逐字性**：扫了 17 个 Rust 代码块，逐 token 与源码比对。1 个根因（多处纯排版级差异，六例见 FINDING RD34-FACT-B-02）；其余 15 块与源码逐字一致或已声明省略；未发现任何 token/语义级改写。
- **`build_memmap` 15 个测试内核**：`grep -rl build_memmap os/qemu-tests/test-kernels/` 实测 15 个文件，与文档一致。
- **N=0 不覆盖**：本报告不覆盖 C 侧断言（P-FACT-A 的范围）；不覆盖 QEMU 门脚本的实际 PASS 状态（只核对了文档对脚本/标记的描述与脚本源码文本是否一致）；不覆盖 `check-layout.sh` 在真镜像上的实跑结果（L8 的 grep 形态另有观察，见 SUGGESTION）。

---

## FINDING RD34-FACT-B-01（P1）

- 位置：文档 §2.4.1.4 末段（第 740 行）；源码 `os/libs/minix-boot/src/handoff_v1.rs:288`（`BOOT_HANDOFF_V1_VERSION = 9`）与 `os/libs/minix-types/src/types/boot.rs:96-100`（`VM_BOOT_HANDOFF_VERSION = 9` 的版本史）
- 严重度：P1
- 问题：文档把 `kern_ident`/`kern_ident_count`/`VM_BOOT_HANDOFF_MAX_IDENT` 的删除记在**错误的版本常量**名下——写作"v9（wire 信封版本 `BOOT_HANDOFF_V1_VERSION = 9`，定义见 §3.4.3）已删除恒空的 `kern_ident`…"，但这三个符号属于 `VmBootHandoff` 协议，其版本常量是 `VM_BOOT_HANDOFF_VERSION`，与 arena 信封的 `BOOT_HANDOFF_V1_VERSION` 是两个独立计数器（当前都恰好是 9）。
- 影响：读者按文档给的锚点去 `handoff_v1.rs` 查 v9 版本史，只会看到"semantic work partition 被删、arena 从九页回到八页"，看不到 `kern_ident`；这会复现文档 §3.4.3 自己警告的"两层版本号不要混"错误。结论（不存在第三个 64 条上限）本身成立，但复核路径断在锚点上。
- 证据或推理：E1/E2。
  - `$ grep -rn "kern_ident\|MAX_IDENT" os --include=*.rs` 命中 9 处，全部是 `os/libs/minix-types/src/types/boot.rs`、`os/kernel/src/globals.rs`、`os/servers/vm/src/boot.rs`、`os/arch/src/arch/paging.rs`；`handoff_v1.rs` 零命中。
  - `os/libs/minix-types/src/types/boot.rs:96-100`：`/// - 9: kern_ident / kern_ident_count / VM_BOOT_HANDOFF_MAX_IDENT removed. …` 紧跟 `pub const VM_BOOT_HANDOFF_VERSION: u32 = 9;`。
  - `os/libs/minix-boot/src/handoff_v1.rs:282-288`：v9 的说明是 "the arena's semantic work partition (two `MemoryRegion` tables, 6144 B) was deleted (R18 D-2) …"，与 `kern_ident` 无关。
  - `$ grep -n "ident" os/libs/minix-boot/src/handoff_v1.rs` 无输出。
- 最小修复：把第 740 行括注改为"v9（`VmBootHandoff` 的 `VM_BOOT_HANDOFF_VERSION = 9`，与 §3.4.3 的 arena 信封版本 `BOOT_HANDOFF_V1_VERSION` 是两个计数器）"；或在 §3.4.3 的两层版本号说明里补一句"另有 `VM_BOOT_HANDOFF_VERSION`，`kern_ident` 的删除归它"。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若 `kern_ident` 曾经也在 `BootHandoffV1` 信封里出现、并与 arena 信封版本 9 同批删除，则本条的"锚点错"就不成立；但 `handoff_v1.rs` 全文零 `ident` 命中，且 v9 版本史写明删的是语义工作分区，所以这一反例不成立。

---

## FINDING RD34-FACT-B-02（P2）

- 位置：文档 §4.2（第 2301-2393 行，`prepare_and_enter_kernel_image` 引文）、§4.4（第 2689-2705 行，`boot_validate_and_prepare` 引文）、§3.8（第 2098-2137 行，`pt_alloc` 引文）、§4.3（第 2586-2618 行，`UbootFileLoader` 引文）、§6.2（第 3371-3407 行，panic handler 引文）；源码 `os/boot-shim/src/uefi_helpers.rs:477-487`、`os/kernel/src/lib.rs:862-876`、`os/arch/src/arch/pt_alloc.rs:127-131`、`os/boot-shim/src/opensbi_helpers.rs:204-215`、`os/boot-shim/src/main.rs:110-111`
- 严重度：P2
- 问题：多处标注为"代码引用/节选"的 Rust 引文与源码不是逐字一致，差异**全部是排版级**（换行位置、多行 `assert!`/`let` 压成单行、一处多了一对包裹括号），代码 token 序列与语义未变；另有一处注释从 6 行英文缩成 1 行中文（该块块头写了"注释有删节"，可算已声明）。冻结裁决 D-045 的口径是"节选引文只许省略不许改写"，按字节复核这些块会对不上。
- 影响：读者/后续轮次拿文档引文与源码做逐字对账时会看到持久差异，破坏"引文可直接复核"这一契约；对事实结论无影响（已逐 token 核对，语义一致）。
- 证据或推理：E2（逐块 diff）。
  1. `prepare_and_enter_kernel_image`（文档 2015-2023 vs 源码 `uefi_helpers.rs:477-487`）：源码 6 行英文注释被缩成 `// x86 的 AP 实模式跳板窗口；aarch64/riscv64 没有这个窗口。`（块头"注释有删节"可覆盖）；同块 `let (bump_base, bump_end) = boot_alloc::boot_alloc_region()` + `.expect(...)` 是源码单行 `let (bump_base, bump_end) =\n boot_alloc::boot_alloc_region().expect(...)` 的重排。
  2. `boot_validate_and_prepare`（文档 2694-2698 vs 源码 862-876）：源码的多行 `assert!(\n kp.is_multiple_of(0x1000),\n "…"\n );` 被压成单行。
  3. `pt_alloc`（文档 2129 vs 源码 127-131）：多行 `assert!(\n is_registered(),\n "pt_alloc::alloc_pt_page called before register"\n );` 被压成单行。
  4. `build_bootstrap_root_and_enable` Step 1（文档 2869-2870 vs 源码 `os/kernel/src/lib.rs:1245-1248`）：源码 `let end = raw_end\n .checked_add(…)\n .expect(…)\n & !(DEV_PIECE - 1);` 被改写成带外层括号的单表达式两行。
  5. `UbootFileLoader`（文档 2602-2604 vs 源码 204-205）：`let slice = unsafe { … };` 的换行位置不同。
  6. panic handler（文档 3397-3398 vs 源码 110-111）：`let message = …` 的换行位置不同。
- 最小修复：这六处按源码原样重排（只改换行/括号位置，不动语义），使"代码引用"块可字节级对账。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若项目在本轮之前已把"引文纯排版重排不算改写"作为通行约定（例如 doc-citation-check 的 `checked=29` 已把重排归一化），则本条应降为 SUGGESTION；我未找到该约定的书面出处，D-045 的文本反而是反向的，所以按 P2 报。

---

## FINDING RD34-FACT-B-03（P2）

- 位置：文档 §4.6 末段（第 2969 行）；源码 `os/kernel-image/src/main.rs:154-172`（x86_64 `_start`）、`:196-215`（aarch64 `_start`）、`:252-290` 与 `:296-320`（riscv64 两个 `_start`）、`os/kernel-image/riscv64.ld:93-96` / `aarch64.ld:85-88` / `x86_64.ld:86-89`
- 严重度：P2
- 问题：句子"三个镜像的入口 asm 显式把栈对齐到 16 字节"把对齐来源记错了：只有 x86_64 的 `_start` 有显式对齐指令（`lea rsp, [rip + kernel_boot_stack_top]; and rsp, -16`）；aarch64 的 `_start` 是 `mov sp, x9`、riscv64 的两个 `_start` 是 `la sp, kernel_boot_stack_top`，都没有对齐指令——这两架构的 16 字节对齐来自链接脚本（`. = ALIGN(16); kernel_boot_stack_bottom = .; . += 0x10000;`，因此 `kernel_boot_stack_top` 天然 16 对齐），不是入口 asm 做的。
- 影响：把"对齐由链接脚本保证"写成"入口 asm 显式对齐"，会让读者以为新增一臂时必须写对齐指令、或误判某架构入口已自带对齐。结论（Rust 侧不继承 C 的入口栈错位）不变，但论据的机制归属错了。
- 证据或推理：E1。
  - `os/kernel-image/src/main.rs` x86_64 臂：`lea rsp, [rip + kernel_boot_stack_top]` 后紧跟 `and rsp, -16`。
  - aarch64 臂：`adrp x9, kernel_boot_stack_top; add x9, x9, :lo12:kernel_boot_stack_top; mov sp, x9`，无 `and`。
  - riscv64 两个臂：`la sp, kernel_boot_stack_top`，无 `andi`。
  - 三份 `*.ld` 的 `.bss` 均在设置 `kernel_boot_stack_bottom` 前 `. = ALIGN(16)`，且 `+= 0x10000`。
  - 注：`jump_to_kmain` 在三架构都显式对齐（x86 `and rsp,-16` + `sub rsp,8`；aarch64 经 x2 `and`；riscv `andi` 等价物），所以该句后半段正确，错误只在前半段的"三个镜像的入口 asm"。
- 最小修复：改为"三个内核镜像的引导栈顶由链接脚本 `. = ALIGN(16)` 保证 16 字节对齐；x86_64 的入口 asm 另显式 `and rsp, -16`；`jump_to_kmain` 三架构均再对齐一次，x86_64 另按 psABI 做 `sub rsp, 8` 调整"。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若"入口 asm"在本篇的用语里把链接脚本的符号解析（`lea`/`adrp`/`la` 取符号）也算作"asm 对齐"，则本条只剩措辞问题；但"显式把栈对齐到 16 字节"在 aarch64/riscv 两臂上没有任何对应指令，按字面读仍是错的。

---

## SUGGESTION

以下不是缺陷，只是可读性/取证便利性建议，不计入 FINDING。

1. §3.4.5（第 1770 行）"约 9 KiB 闲置容量"是估算值（`128×16 + 256×16 + 40×sizeof(SourceRecord)` ≈ 8.5–8.8 KiB，取决于 `SourceRecord` 的实际大小）；建议改成"约 8.5–9 KiB"或直接用 `size_of` 复算命令给出精确值，避免读者按 9 KiB 复核时对不上。
2. §5.2.1（第 3213 行）"可跑形态需 `--features "x86_64 runtime-window"`"——`x86_64` 是 `minix-arch` 里声明的空 feature（`os/arch/Cargo.toml:29`），命令可跑；但建议注明这是 `minix-arch` 的 feature 而非 target，避免读者把它当 `--target`。
3. §5.2.2 的 L8 行（第 3261 行）描述为"`minix_kernel::arch_boot` 符号在镜像里"。脚本实际 grep 的是 legacy mangled 片段 `minix_kernel9arch_boot`（`check-layout.sh:244`，其中 `9` 是 `arch_boot` 的长度前缀）。文档描述本身没错，但若后续有人"顺手修正"这个 grep，L8 会失效——建议在该行加一句"脚本按 legacy mangled 名匹配，不是拼写错误"。
4. §3.2（第 1525 行）"7 处 feature 属性"与源码 `lib.rs` 顶部注释一致；但 `grep -c '#\[cfg(feature' os/boot-shim/src/lib.rs` 会返回 8（多的一处是 `//!` 文档注释行）。建议在复算命令旁注明"排除文档注释行"，否则按命令复核的人会看到 8。

## 覆盖缺口说明（本轮未覆盖、需别轮或别角色）

- C 侧断言（§2 章的函数体/链接脚本/调用序、§2.7 ARM32 的 `overlaps` 闭区间）不在本角色范围，未核。
- 真机门链的实际 PASS/FAIL 状态未核（只核对了文档对脚本判定标记、退出码语义、CI 接线状态的描述与脚本源码一致）。
- `check-layout.sh` 在真实产物上的 L0–L9 实跑结果未核（无三架构镜像产物）；只核对了断言清单文本与脚本实现一致。
- 文档 §3.4.4/§5.2.3 的"真机读数"（`res=127 zeroed=127`、`pa=0x5e9b000`、`verdict=replayed`、`0x4a00_0000_0001_ff00`）属运行时观察，仓库内只有探针代码与注释，无归档日志，无法独立复算；本轮只确认探针代码与文档描述的机制一致（分配类型、三个时点、判定字符串）。

## 本轮统计

- FINDING 3 条：P1 × 1（版本常量锚点错）、P2 × 2（Rust 引文未声明改写；入口栈对齐机制归属错）。
- SUGGESTION 4 条。
- 未展开根因：无（根因数 3 < 40）。
