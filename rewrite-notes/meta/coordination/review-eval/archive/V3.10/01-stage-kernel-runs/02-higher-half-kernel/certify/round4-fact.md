# D02 R4 事实面第二次独立 certify 回读

- 结论：**PASS（18/18 CLOSED）**，一致性 100%，高于 CONVERGED 要求的 90%。
- 核验条数：18（不抽样，逐条回读）。
- 基线：HEAD `0f5a5bd9d9f63e3bbf8f3de1ffbc01f6ddc6df53`；文档 `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` sha256 `9f9ec2e7ccdfccb22b402d477c784a495df87c3aab2551134f920eb02f14c56e`；`git ls-files -m` 为 0，回读对象就是最终 HEAD。
- 角色：冷启动第二次独立回读者，未参与 R4 面板、修复与 pass1；只读，唯一写入是本文件。未跑 QEMU。测试计数用 `cargo test --list` 实测，构建目标目录重定向到 `/tmp/r4-cert-target`，未写入仓库 `target/`；27 条 minix-elf 测试与 31 条 loader 测试实际执行通过。
- 判定依据：最终 HEAD 的文档原文与源码；`/tmp/r4-cert-fact.md` 只用来框定 18 条 ID，pass1 回执只作对照，不作为判定证据。

## 一、18 个修改点逐条回读

### 1. FACT-01：CLOSED

- doc:458 现文（逐字）：「迭代器在 `next()` 里做类型过滤与范围检查：只有非 `PT_LOAD` 类型过滤走 `continue`（正常过滤），其余表项范围与段形失败都以 `Some(Err(..))` 浮出（I-48/D-084）。」后半句保留 D2-01 未检查加法残留。
- 与代码块同口径：doc:482-484 只对非 `PT_LOAD` 执行 `continue`；doc:464-473（`checked_mul`/`checked_add` 溢出 → `PhdrOutOfBounds`）、doc:477-480（`parse_phdr` 失败原样浮出）、doc:486-493（段文件字节越界 → `SegmentOutOfBounds`）、doc:494-496（`FileszExceedsMemsz`）全部 `Some(Err(..))`。
- 与 §4.2 同口径：doc:823 的 `let seg = seg_result?;`、doc:861 的 `collect::<Result<Vec<_>, _>>()?`，doc:851 写明「畸形程序头由迭代器以 `Err` 项浮出，`filesz > memsz` 的守卫也已收进迭代器」。
- 与 §5.1 同口径：doc:1120 的「迭代器行为」族列 `test_segment_iter_reports_segment_data_out_of_bounds`、`test_segment_iter_non_load_filter_stays_silent` 等 12 条，证明列写「表范围检查、非 `PT_LOAD` 过滤、三类畸形以 `Err` 返回（I-48/D-084）」。
- 源码：`os/libs/minix-elf/src/lib.rs:393-450` 的 `next()`；唯一的 `continue` 在 `:413-415`（`if phdr.p_type != PT_LOAD`）；四个 Err 出口在 `:395-404`、`:408-411`、`:419-425`、`:431-433`。
- 修复动作有据：`git diff d99dc16a7..0f5a5bd9d -- <doc>` 只有两行替换，本行是其中之一；旧行（`0f5a5bd9d^` 的 doc:458）仍是「命中这些条件时 `continue`」，新行已消除。
- 测试：`cargo test -p minix-elf --lib -- --list` → 27；实跑 27 passed / 0 条失败。

### 2. FACT-02：CLOSED

- doc:505 现为「共 **12 个变体**」，四组 5+2+2+3 与源码逐项可数。
- `os/libs/minix-elf/src/lib.rs:157-198` 的 `enum ElfError` 恰好 12 项（`:159` TooShort、`:161` BadMagic、`:163` Not64Bit、`:165` NotLittleEndian、`:167` NotExecutable、`:169` PhdrOutOfBounds、`:171` PhdrEntryTooSmall、`:173` NoLoadSegments、`:176` InvalidSegment、`:180` SegmentOutOfBounds、`:186` FileszExceedsMemsz、`:194` TargetBufferTooSmall）。
- loader 侧 `InvalidSegment` 三个生产点：`os/boot-shim/src/loader.rs:196`（`paddr + memsz` 的 `checked_add`）、`:208`（`kern_end_phys - kern_phys_base` 的 `checked_sub`）、`:353`（段间物理重叠）；源/目标重叠仍 `panic!`（`:361-364`，注释标 D01-R9）。`TargetBufferTooSmall` 由先验后写两循环产生（函数体 `:266-316`，变体在 `:290-293`）。
- `filesz > memsz` 已收进迭代器：`os/libs/minix-elf/src/lib.rs:431-433`；loader 内无同义的旧守卫（`grep 'filesz > ' os/boot-shim/src/loader.rs` 只命中注释与测试断言）。
- doc 内 `grep -c '九个变体'` → 0。

### 3. FACT-03：CLOSED

- doc:410 现为「用途按调用点分五类」，并写明「用 `elf_file_end` 把模块 ELF 修剪到最后一个 `PT_LOAD` 的文件尾后整块拷贝（D-082/I-36；不重定位、不逐段映射）」。
- 源码：`os/boot-shim/src/loader.rs:227` 定义 `fn elf_file_end`，`:237-248` 遍历 `minix_elf::segment_iter` 取文件尾，`:517` 在 `load_boot_modules_into` 内调用 `let used = elf_file_end(data).unwrap_or(data.len());`。
- 五个依赖 crate 与文档一致：`cd os && grep -rl 'minix-elf =' */Cargo.toml */*/Cargo.toml` 输出 `arch`、`boot-shim`、`servers/vfs`、`servers/vm`、`servers/rs`。
- 测试计数实测：`cargo test -p boot-shim --lib --features test-all -- --list` → 146 tests；默认特性 → 135 tests。与 doc:1124 的「默认特性下共 135 个测试」一致；`opensbi_helpers` 的 11 条只在 `opensbi` 特性下编译（`os/boot-shim/src/lib.rs:239-240`，`Cargo.toml` 的 `default = ["uefi"]`）。

### 4. FACT-04：CLOSED

- doc:366 现为「六块载荷（memmap/reserved/modules/platform/param/sources）」；doc:1022 现为「信封与 memmap/reserved/modules/platform/param/sources 六个分区」。
- `os/libs/minix-boot/src/handoff_v1.rs:327-332` 只有六个描述符：`memmap_byte_len`、`reserved_regions_byte_len`、`boot_modules_byte_len`、`platform_sources_byte_len`、`param_buf_byte_len`、`sources_byte_len`；没有 `startup_byte_len`。`:296` 的 `BOOT_HANDOFF_V1_VERSION = 10`，`:289-295` 记录 v10/D-062 删除 startup 分区。
- 接收侧六个解码：`os/kernel/src/lib.rs:692`（memmap）、`:697`（reserved）、`:706`（modules）、`:736`（platform_sources）、`:741`（param_buf）、`:746`（sources）。
- 发送侧删除记录仍在：`os/boot-shim/src/uefi_helpers.rs:1220`、`os/boot-shim/src/uboot_main.rs:516`、`os/libs/minix-boot/src/boot_storage.rs:300`。
- 负向 grep：`'startup 分区'`、`'七块载荷|七个分区'` 在 doc 均为 0。

### 5. FACT-05：CLOSED

- doc:1115「27 个测试」，doc:1124「loader.rs 部分有 31 个测试……默认特性下共 135 个测试」，doc:526「wire 另带 `kern_phys_base` 作为第三个布局标量」。
- 实测计数：`cargo test -p minix-elf --lib -- --list` → `27 tests`；`cargo test -p boot-shim --lib -- --list` → `135 tests`，其中 `loader::tests::*` 31 条；`--features test-all` → `146 tests`。`cargo test -p minix-elf --lib` 与 `cargo test -p boot-shim --lib loader::tests` 均全绿（27 / 31 passed）。
- doc:1152 的载体表达式已改为 `kern_phys_base: PhysBytes(image_base)` 与 `kern_size: image_end - image_base`；`grep -c 'image_end - image_base'` → 1。

### 6. FACT-06：CLOSED

- doc:1120 现列 `test_segment_iter_reports_segment_data_out_of_bounds`；`grep -c 'skips_segment_data_out_of_bounds'` → 0。
- 源码同名测试在 `os/libs/minix-elf/src/lib.rs:1005`，断言 `Some(Err(ElfError::SegmentOutOfBounds))`（`:1016-1019`），并在其后断言迭代终止（`:1020`）。
- doc 内全部 36 个唯一 `test_*` 标识符逐个对源码 `fn <name>` 检索，无 MISSING。`cargo test --list` 的 27 条与 §5.1 的 12+6+5+4 分族逐名对得上。

### 7. FACT-07：CLOSED

- doc:1152 现为「`kern_phys_base: PhysBytes(image_base)`、`kern_size: image_end - image_base`（镜像窗口按 2 MiB 取整后的跨度）」；`grep -c 'kern_size: 0x200_000'` → 0，`grep -c 'kern_size: 0x200'` → 0。
- 源码 `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs:36-37` 定义 `image_base = image_lo & !0x001F_FFFF`、`image_end = (image_hi + 0x001F_FFFF) & !0x001F_FFFF`；`:54-55` 正是 `kern_phys_base: PhysBytes(image_base)` 与 `kern_size: image_end - image_base`。

### 8. FACT-08：CLOSED

- doc:886 现为「`bootface_a64.rs` 的 bump 游标 `static BUMP_NEXT: AtomicU64` 初值为 `BUMP_BASE`（非零）」；`grep -c 'BUMP_PTR'` → 0（doc 与 `os/kernel-image/src`、`os/kernel/src` 均无）。
- 源码 `os/kernel-image/src/bootface_a64.rs:119` 是 `static BUMP_NEXT: AtomicU64 = AtomicU64::new(BUMP_BASE);`；`:96` 的 `BUMP_BASE = DRAM_BASE + 0x0400_0000` 为非零常量，静态初值非零故落 `.data`，与 `.bss` 反例不冲突。

### 9. FACT-09：CLOSED

- doc:324 现为「ARM 侧同一判据也已静态核实：`earm/Makefile.inc` 的改名名单不含 `mpx.o`、`earm/mpx.S:265-272` 在 `.bss` 后切回 `.data`、`earm/kernel.lds` 的 `.unpaged_data` 只收 `unpaged_*.o`，故 `k_initial_stack` 落在高半 `.data`」。
- 三处逐条核：`minix3/minix/kernel/arch/earm/Makefile.inc:61-63` 的 `.for unpaged_obj` 名单为 `head.o pre_init.o direct_tty_utils.o pg_utils.o klib.o utility.o arch_reset.o` 加 `MINLIB/MINC/SYS/BSP_OBJS_UNPAGED`，不含 `mpx.o`（`mpx.o` 只在 `:80`、`:97` 的依赖行）；`earm/mpx.S:265-272` 依次为 `.data` / `.short 0x526F` / `.bss` / `.data` / `.balign 4` / `k_initial_stack:` / `.space K_STACK_SIZE` / `LABEL(__k_unpaged_k_initial_stktop)`；`earm/kernel.lds:18` 的 `.unpaged_data` 只收 `unpaged_*.o(.data .rodata*)`，高半 `.data` 在 `:33` 收 `*(.data .rodata* )`。

### 10. FACT-10：CLOSED

- ① doc:279 现写「省略号处为同文件此前的代码与 `k_boot_stktop` 之后的 10 行」：`minix3/minix/kernel/arch/i386/mpx.S` 共 651 行，`k_boot_stktop` 在 `:641`，其后 642-651 恰 10 行。
- ② doc:91 现写「省略号处为段体、别名与 `.text` 之后的 `.data/.bss/_end`」：`minix3/minix/kernel/arch/i386/kernel.lds` 的 `.text` 在 `:29`，其后 `.data`（`:30`）、`.bss`（`:32-35`）、`_end`（`:36`）；别名行与段体的省略方式已在标注里披露。
- ③ doc:810 现写「保留函数体；循环体内注释压缩/改写」：源码 `os/boot-shim/src/loader.rs:184-190` 的六行循环注释在 doc 的引文中压缩为两行，措辞相符。
- ④ doc:1077 现写 `store_kernel_info` 的「函数体注释」，引文 `The reserved_regions payload lives in the boot-shim's UEFI-pool heap…` 位于 `os/kernel/src/lib.rs:1588-1595` 的函数体内；文档注释在 `:1564-1574`，函数定义在 `:1575`，归属正确。
- 机械门：`python3 tools/doc-citation-check.py <doc>` → `blocks=32 checked=24 failed=0 skipped=4 warned=4`；`bash tools/anchor-resolve.sh --check <doc>` → `anchors=54 resolved=54 zero-def=0 multi-def=0`；`python3 tools/anchor-suspect-scan.py <doc>` → 可疑锚点 0 处；`bash tools/doc-style-lint.sh <doc>` → `error级=0`。

### 11. FACT-11：CLOSED

- doc:1008 的行号锚点现为 `os/kernel/src/arch/riscv64/higher_half.rs:42-51`。
- 源码 `:42` 是 `// - **Direct leg** (`arch_boot_impl`): ...` 的首行，`:51` 是 `real hardware: PC now reads 0xffffffc0...` 的末行；`:52` 已是 `// - **Resume leg**` 的首行，`:41` 是空注释行。锚点不再跨到 resume-leg 说明。
- `anchor-resolve.sh --check` 54/54 解析，无零定义/多定义。

### 12. FACT-12：CLOSED

- `grep -n 'kinfo.kern_stack_top' doc` 只命中 doc:582 与 doc:586，均为 `kinfo.kern_stack_top()` 访问器写法；`grep -c 'kinfo.kern_stack_top`'`（带反引号，字段式）→ 0。
- 源码 `os/libs/minix-boot/src/kernel_info.rs:200` 是 `pub fn kern_stack_top(&self) -> VirBytes`；`KernelInfo` 结构体内无同名字段。v10 删除字段的记录在 `handoff_v1.rs:289-295` 与 `os/libs/minix-boot/src/boot_storage.rs:300`。
- 两个生产调用点 `os/kernel/src/lib.rs:232` 与 `:370` 都写 `kinfo.kern_stack_top()`；`grep 'kern_stack_top:' os/libs/minix-boot/src/kernel_info.rs os/kernel/src` 无输出。

### 13. DESIGN-D2-01：CLOSED

- 与第 1 条同源：doc:458 已改为「只有非 `PT_LOAD` 过滤走 `continue`，其余 `Some(Err)`」，不再是「命中这些条件时 `continue`」。
- 12 变体见 doc:505 与源码 `lib.rs:157-198`；`grep -c '九个变体'` → 0；`grep -c 'D2-05'` → 0；`grep -c '失败即停机'` → 0。D2-01 现在只用于指认 `SegmentIter::new`/`parse_phdr` 的未检查加法残留（`lib.rs:357-358`、`:271`），与 RD2-05 的登记口径一致。

### 14. DESIGN-D2-02：CLOSED

- 与第 3 条同源：doc:410 的「五类」与 `elf_file_end` 修剪说明都在；`loader.rs:227/517` 的调用链存在；146/135 的测试计数实测吻合。

### 15. DESIGN-D2-03：CLOSED

- 与第 4 条同源：doc:366「六块载荷」、doc:1022「六个分区」；`startup 分区` 与 `七块载荷/七个分区` 均 0 命中；wire 六个描述符与接收侧六个解码函数存在。

### 16. DESIGN-D2-10：CLOSED

- 与第 5/7 条同源：27/31/135/146 的计数全部实测吻合；§5.2 载体表达式 `PhysBytes(image_base)` / `image_end - image_base` 与源码逐字同形；§3.3 的 wire 三标量已注明（doc:526）。
- `grep -c '125'`、`'23 个测试'`、`'22 个测试'`、`'九个变体'`、`'七块'` 在 doc 均为 0。

### 17. CLAIM-05：CLOSED

- CLAIM-05 的三个验收面全部通过：`grep -c '12 个变体'` → 3；`grep -c '六块载荷'` → 1（doc:366）；计数 27/31/135 实测吻合，`test_segment_iter_reports_segment_data_out_of_bounds` 存在。
- pass1 判它 STILL-BROKEN 的唯一残留是 doc:458 的 旧的开放语义 散文；该行已被 `0f5a5bd9d` 替换，CLAIM-05 的阻断面消失。

### 18. CLAIM-06：CLOSED

- CLAIM-06 的实体是「v1 arena 七块载荷含 startup」：doc:366 与 doc:1022 均为六块/六个分区；`BootHandoffV1` 六个 `*_byte_len`（`handoff_v1.rs:327-332`）与六个 `decode_*`（`kernel/src/lib.rs:692/697/706/736/741/746`）一致；v10 删除记录在 `handoff_v1.rs:289-295`、`uefi_helpers.rs:1220`、`uboot_main.rs:516`、`boot_storage.rs:300`。

## 二、pass1 五条同源残项的重点复核

pass1 的 5 条 STILL-BROKEN 指 FACT-01、DESIGN-D2-01、CLAIM-05（事实面）与 READER-A-01、READER-B-03（读者面，不在本回执 18 条内），根因都是 doc:458 一句。最终提交 `0f5a5bd9d` 的文档 diff 只有两行：

1. doc:458：旧句「迭代器在 `next()` 里做类型过滤与范围检查，命中这些条件时 `continue`（不产出该段）」→ 新句「只有非 `PT_LOAD` 类型过滤走 `continue`（正常过滤），其余表项范围与段形失败都以 `Some(Err(..))` 浮出（I-48/D-084）」。
2. doc:580：为 reader 面补充「`resume_after_v1_decode`（三架构同名壳，公共体 `resume_additive`）」；源码 `os/kernel/src/lib.rs:773/781/800` 三个同名 `#[cfg]` 壳、`:326` 的 `resume_additive` 公共体核对成立，与本 18 条无冲突。

line 458 与三处同口径的交叉核对结论：

| 位置 | 口径 | 与 line 458 一致 |
|---|---|---|
| doc:461-500 代码节选 | 仅 `p_type != PT_LOAD` 走 `continue`；其余四个出口 `Some(Err)` | 是 |
| doc:503 行为段 | 「只把非 `PT_LOAD` 类型当作正常过滤；其余畸形一律以 `Some(Err(..))` 浮出」 | 是 |
| doc:812-882 §4.2 | `seg_result?` 与 `collect::<Result<Vec<_>, _>>()?` 在第一个 `Err` 短路 | 是 |
| doc:1120 §5.1 | 迭代器测试族含 non-load 过滤与 `Err` 断言 | 是（见下「观察」的计数措辞） |
| 实现 `lib.rs:393-450` | 唯一 `continue` 在非 `PT_LOAD`；四个 `Some(Err)` 出口 | 是 |

其余 15 点：最终提交只改了上述两行，未触碰任何其他证据行；本回执仍按最终 HEAD 逐条重读了原文与源码（不是仅比对 diff），15 点全部保持 CLOSED。

## 三、机械门复跑（最终 HEAD）

- `python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` → `blocks=32 checked=24 failed=0 skipped=4 warned=4`。
- `bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` → `anchors=54 resolved=54 zero-def=0 multi-def=0`。
- `python3 tools/anchor-suspect-scan.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` → 可疑锚点 0 处（C=0，Rust=0）。
- `bash tools/doc-style-lint.sh rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` → `error级=0`。
- 测试清单与实跑（`CARGO_TARGET_DIR=/tmp/r4-cert-target`）：minix-elf 27 条（27 passed）；boot-shim 默认 135 条（`loader::tests` 31 条，31 passed）；`--features test-all` 146 条。未跑 QEMU。

## 四、汇总

| 项 | 数量 | 编号 |
|---|---|---|
| CLOSED | 18 | FACT-01..FACT-12、DESIGN-D2-01、DESIGN-D2-02、DESIGN-D2-03、DESIGN-D2-10、CLAIM-05、CLAIM-06 |
| STILL-BROKEN | 0 | — |

pass1 的三条事实面残项（FACT-01、DESIGN-D2-01、CLAIM-05）同源，均已随 doc:458 的替换闭合；两条 reader 面同源残项（READER-A-01、READER-B-03）的根因句也已消失，最终提交另补的 doc:580 说明经源码核对成立（不在本回执 18 条判定内）。

## 五、观察（不计入 18 条判定）

- doc:1120 的证明列写「三类畸形以 `Err` 返回」。迭代器测试族实际覆盖四个 `ElfError` 变体：构造期的 `PhdrOutOfBounds`、`PhdrEntryTooSmall`，`next()` 的 `SegmentOutOfBounds`、`FileszExceedsMemsz`；若把「表范围检查」单列，剩余畸形为两类而不是三类。这是总结列的计数措辞不精确，不影响 18 条的任一验收面（12 变体、测试名、27 条计数均已对）。若要做 P3 收口，最小改法是把「三类畸形」改为「四类畸形」，或改成不报数的「多种畸形」。reader pass1 也把它列为未展开的旁及项。
- 本回执是同一模型冷启动的第二回读，没有第二个独立工具复核；每条都附了可重放的命令或源码行号，便于跨工具复验。citation 门的 4 个 `warned` 是非逐字引文的结构豁免，覆盖不到散文级漂移；doc:458 这类句子靠人工逐字回读兜底。
- 计数类的 27/31/135/146 已从静态 `#[test]` 计数升级为 `cargo test --list` 实测，并实跑了 minix-elf 全量与 loader 全量；仍未验证的是 `p_paddr` 物理写入、三个架构的真机跳转与 QEMU 行为，那些不在事实面 18 条的射程内。

- 遗留 P0: 0 / P1: 0
