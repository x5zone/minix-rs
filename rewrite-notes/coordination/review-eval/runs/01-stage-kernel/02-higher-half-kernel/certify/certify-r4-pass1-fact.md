# D02 R4 事实面 certify 独立回读回执

- 结论：NOT PASS（15 条 CLOSED，3 条 STILL-BROKEN）
- 核验条数：18（不抽样，逐条回读）
- 基线：HEAD `7ce60e31f`；文档 `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` sha256 `8d140dcd8b4d58dbb3a6048de7de8455d3c477c8f5f3e89a0f7b226636f2f7eb`；`git diff HEAD` 对文档与三个关键源码为空，回读对象就是最终 HEAD。
- 角色：冷启动独立回读，未参与 R4 面板与修复；只读，唯一写入是本文件。未运行 QEMU 与完整测试套件；测试计数用源码静态计数，布局断言用既有工件。

## 一、18 个修改点逐条回读

### 1. FACT-01
- 判定：STILL-BROKEN
- 原文残留（doc:458，逐字）：「迭代器在 `next()` 里做类型过滤与范围检查，命中这些条件时 `continue`（不产出该段）；但 `SegmentIter::new` 与 `parse_phdr` 的表范围算式是未检查加法，畸形输入会 panic 而不是 `Err`（代码修复登记设计批 D2-01）」
- 证据：这一行与修复前 HEAD `9ebf3f054` 的 doc:457 **逐字相同**（`git show 9ebf3f054:... | awk 'NR==457'` 与最终 HEAD `awk 'NR==458'` 对比）。批一（`ca5830fd5`）替换了其后的代码块、并在 doc:503 补了正确的行为段（「迭代器只把非 `PT_LOAD` 类型当作正常过滤（`continue`）；其余畸形一律以 `Some(Err(..))` 浮出」），但 458 这一句没改。源码 `os/libs/minix-elf/src/lib.rs:398-433` 只有 `if phdr.p_type != PT_LOAD { continue; }`（`:413-415`）；`checked_mul`/`checked_add` 溢出返回 `Some(Err(PhdrOutOfBounds))`（`:395-404`）、`parse_phdr` 失败返回 `Some(Err(e))`（`:408-411`）、`p_offset + p_filesz` 越界返回 `Some(Err(SegmentOutOfBounds))`（`:419-425`）、`filesz > memsz` 返回 `Some(Err(FileszExceedsMemsz))`（`:431-433`）。因此 458 的前半句仍与实现相反，且与同节 503 自相矛盾。
- 最小修复：把 doc:458 的前半句改为「迭代器在 `next()` 里做类型过滤与范围检查：只有非 `PT_LOAD` 类型过滤走 `continue`（正常过滤），其余范围与形状检查失败都以 `Some(Err(..))` 浮出（I-48/D-084）」；后半句（`SegmentIter::new`/`parse_phdr` 的未检查加法与 D2-01 登记）保留不动。

### 2. FACT-02
- 判定：CLOSED
- 证据：doc:505 现为「共 **12 个变体**」，分组与源码逐项可数：文件头五道 `TooShort/BadMagic/Not64Bit/NotLittleEndian/NotExecutable`、程序头表两道 `PhdrOutOfBounds/PhdrEntryTooSmall`、迭代器段形两道 `SegmentOutOfBounds/FileszExceedsMemsz`、loader 三道 `NoLoadSegments/InvalidSegment/TargetBufferTooSmall { needed_end, buf_len }`；`os/libs/minix-elf/src/lib.rs:157-198` 恰好 12 项。doc:505 已写明 `filesz > memsz` 自 I-48/D-084 起由迭代器返回 `FileszExceedsMemsz`，loader 侧 `InvalidSegment` 只剩三个生产点：`os/boot-shim/src/loader.rs:196`（`paddr + memsz` 的 `checked_add`）、`:208`（`kern_end_phys - kern_phys_base` 的 `checked_sub`）、`:353`（段间物理重叠）；源/目标重叠仍 `panic!`（`:361-364`，注释标 D01-R9）。`TargetBufferTooSmall` 由 `load_segments_into_buffer` 的先验后写两循环产生（`:273-320`；`TargetBufferTooSmall` 在 `:290-293`）。

### 3. FACT-03
- 判定：CLOSED
- 证据：doc:410 现为「用途按调用点分五类」，并补「用 `elf_file_end` 把模块 ELF 修剪到最后一个 `PT_LOAD` 的文件尾后整块拷贝（D-082/I-36；不重定位、不逐段映射）」。源码 `os/boot-shim/src/loader.rs:227` 定义 `fn elf_file_end`，`:517` 在 `load_boot_modules_into` 内调用；`:237-248` 遍历 `minix_elf::segment_iter` 的文件尾。五类与五个依赖 crate 对得上：`cd os && grep -rl 'minix-elf =' */Cargo.toml */*/Cargo.toml` 输出 `arch`、`servers/vfs`、`servers/vm`、`boot-shim`、`servers/rs` 五行。`os/boot-shim/src` 的 `#[test]` 静态总数 146（15+45+17+1+31+11+14+12），与 `--features test-all` 的 146 一致；默认特性关闭 `opensbi_helpers` 的 11 条，故默认 135（见第 5 条）。

### 4. FACT-04
- 判定：CLOSED
- 证据：doc:366 现为「把 arena 里的六块载荷（memmap/reserved/modules/platform/param/sources）落进本镜像 `.bss`」；doc:1022 现为「信封与 memmap/reserved/modules/platform/param/sources 六个分区」。`grep -n 'startup 分区' doc` 为 0 命中，`grep -n '七块载荷\|七个分区' doc` 也为 0。源码 `os/libs/minix-boot/src/handoff_v1.rs:307-333` 的 `BootHandoffV1` 只有六个 payload 长度描述符（`memmap_byte_len`、`reserved_regions_byte_len`、`boot_modules_byte_len`、`platform_sources_byte_len`、`param_buf_byte_len`、`sources_byte_len`），没有 `startup_byte_len`；接收侧 `os/kernel/src/lib.rs:686-746` 只调用六个 `decode_*`，无 startup 解码。v10（D-062）的删除记录在 `handoff_v1.rs:285-296`。

### 5. FACT-05
- 判定：CLOSED
- 证据：doc:1115「27 个测试」，复算 `grep -c '    fn test_' os/libs/minix-elf/src/lib.rs` 输出 27；doc:1124「loader.rs 部分有 31 个测试」，复算 `grep -c '#\[test\]' os/boot-shim/src/loader.rs` 输出 31，并写明「默认特性下共 135 个测试」。静态核对：`boot-shim/src` 的 `#[test]` 共 146，默认特性下 `opensbi_helpers` 被 `#[cfg(feature = "opensbi")]` 排除（`lib.rs:239-240`），该文件 11 条测试，146 − 11 = 135；`loader::tests` 31 条恒在。doc:526 已注明 wire 另带 `kern_phys_base` 作为第三个布局标量（`BootHandoffV1` 的 `kern_virt_base`/`kern_phys_base`/`kern_size` 三标量）。

### 6. FACT-06
- 判定：CLOSED
- 证据：doc:1120 的「迭代器行为」族现列 `test_segment_iter_reports_segment_data_out_of_bounds`；`grep -n 'skips_segment_data_out_of_bounds' doc` 为 0。源码同名测试在 `os/libs/minix-elf/src/lib.rs:1005`，断言 `Some(Err(ElfError::SegmentOutOfBounds))`（`:1017-1020`）。doc 中全部 38 个 `test_*` 标识符逐个对源码 `fn <name>` 检索，无 MISSING。

### 7. FACT-07
- 判定：CLOSED
- 证据：doc:1152 现为「`kern_virt_base: VirBytes(0xFFFF_8000_0000_0000)`、`kern_phys_base: PhysBytes(image_base)`、`kern_size: image_end - image_base`（镜像窗口按 2 MiB 取整后的跨度）」；`grep -n 'kern_size: 0x200_000' doc` 为 0。源码 `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs:54-55` 正是 `kern_phys_base: PhysBytes(image_base)` 与 `kern_size: image_end - image_base`，`:36-37` 给出 `image_base`/`image_end` 的 2 MiB 取整定义。

### 8. FACT-08
- 判定：CLOSED
- 证据：doc:886 现为「`bootface_a64.rs` 的 bump 游标 `static BUMP_NEXT: AtomicU64` 初值为 `BUMP_BASE`」；`grep -n 'BUMP_PTR' doc` 为 0（`os/kernel-image/src` 与 `os/kernel/src` 也无 `BUMP_PTR`）。源码 `os/kernel-image/src/bootface_a64.rs:119` 正是 `static BUMP_NEXT: AtomicU64 = AtomicU64::new(BUMP_BASE);`，`BUMP_BASE` 在 `:96` 为非零常量，静态初值非零故落 `.data`，与被替换的 `.bss` 反例不冲突。

### 9. FACT-09
- 判定：CLOSED
- 证据：doc:324 现为「ARM 侧同一判据也已静态核实：`earm/Makefile.inc` 的改名名单不含 `mpx.o`、`earm/mpx.S:265-272` 在 `.bss` 后切回 `.data`、`earm/kernel.lds` 的 `.unpaged_data` 只收 `unpaged_*.o`，故 `k_initial_stack` 落在高半 `.data`」。三处逐条核：`minix3/minix/kernel/arch/earm/Makefile.inc:61-63` 的 `.for unpaged_obj` 名单为 `head.o pre_init.o direct_tty_utils.o pg_utils.o klib.o utility.o arch_reset.o` 加 `MINLIB/MINC/SYS/BSP_OBJS_UNPAGED`，不含 `mpx.o`（`mpx.o` 只在 `:80`、`:97` 的依赖行）；`earm/mpx.S:265-272` 为 `.data` / `.short 0x526F` / `.bss` / `.data` / `.balign 4` / `k_initial_stack:` / `.space K_STACK_SIZE` / `LABEL(__k_unpaged_k_initial_stktop)`；`earm/kernel.lds:18` 为 `.unpaged_data ALIGN(4096) : { unpaged_*.o(.data .rodata*) }`，而高半 `.data` 在 `:33` 收 `*(.data .rodata* )`。结论成立。

### 10. FACT-10
- 判定：CLOSED
- 证据：四处引文标注逐条核。① `doc:279` 现写「省略号处为同文件此前的代码与 `k_boot_stktop` 之后的 10 行」，`minix3/minix/kernel/arch/i386/mpx.S` 共 651 行，`k_boot_stktop` 在 `:641`，其后 642-651 恰 10 行。② `doc:91` 现写「省略号处为段体、别名与 `.text` 之后的 `.data/.bss/_end`」，`i386/kernel.lds:29-36` 正是 `.text` 后接 `.data`、`.bss`、`_end`。③ `doc:810` 现写「保留函数体；循环体内注释压缩/改写」，源码 `loader.rs:184-190` 的六行循环注释在引文中压缩为两行，措辞相符。④ `doc:1077` 现写 `store_kernel_info` 的「函数体注释」，源码 `os/kernel/src/lib.rs:1575` 是函数定义，被引句在 `:1588-1595` 的函数体内，不是文档注释（文档注释在 `:1564-1574`）。机械门：`python3 tools/doc-citation-check.py <doc>` 输出 `blocks=32 checked=24 failed=0 skipped=4 warned=4`；`python3 tools/anchor-suspect-scan.py <doc>` 输出可疑锚点 0 处；`bash tools/doc-style-lint.sh <doc>` 输出 `error级=0`。

### 11. FACT-11
- 判定：CLOSED
- 证据：doc:1008 的行号锚点现为 `os/kernel/src/arch/riscv64/higher_half.rs:42-51`。源码 `:42` 是 `// - **Direct leg** (`arch_boot_impl`)...` 的首行，`:51` 是 `real hardware: PC now reads 0xffffffc0...` 的末行；`:52` 已是 `// - **Resume leg**` 的首行，`:41` 是空注释行。锚点不再跨到 resume-leg 说明。`bash tools/anchor-resolve.sh --check <doc>` 输出 `anchors=54 resolved=54 zero-def=0 multi-def=0`。

### 12. FACT-12
- 判定：CLOSED
- 证据：`grep -n 'kinfo.kern_stack_top' doc` 只命中 doc:582 与 doc:586，均为 `kinfo.kern_stack_top()` 访问器写法；`grep -n 'kinfo.kern_stack_top`' doc`（带反引号，字段式）为 0。源码 `os/libs/minix-boot/src/kernel_info.rs:200` 是 `pub fn kern_stack_top(&self) -> VirBytes`，`KernelInfo` 结构体内无同名字段；两个生产调用点 `os/kernel/src/lib.rs:232` 与 `:370` 都写 `kinfo.kern_stack_top()`。

### 13. DESIGN-D2-01
- 判定：STILL-BROKEN
- 原文残留：与第 1 条同一条 doc:458（面板 DESIGN-D2-01 原话就是引「命中这些条件时 `continue`（不产出该段）」）。九个变体与 D2-05 已改，但「命中检查条件即 `continue`」这一句没改。
- 证据：见第 1 条。设计面板的最小修复要求「重写 §3.2 的『迭代器行为』段与九个变体清单」，当前 doc:503 已重写，doc:458 的题句未重写，仍与 doc:483、doc:503 相反。
- 最小修复：同第 1 条。

### 14. DESIGN-D2-02
- 判定：CLOSED
- 证据：同第 3 条。doc:410 的「五类」与 `elf_file_end` 修剪说明都在；`loader.rs:227/517` 的调用链存在。

### 15. DESIGN-D2-03
- 判定：CLOSED
- 证据：同第 4 条。doc:366「六块载荷」、doc:1022「六个分区」；doc 内 `startup 分区` 与 `七块载荷` 均为 0 命中；wire 六个描述符与六个解码函数存在。

### 16. DESIGN-D2-10
- 判定：CLOSED
- 证据：同第 5 条与第 7 条。计数 27/31/135 与两处复算命令一致；§5.2 载体表达式 `PhysBytes(image_base)` / `image_end - image_base` 与源码一致；§3.3 的 wire 三标量已注明。doc 内 `125`、`23 个测试`、`22 个测试`、`九个变体`、`七块` 均为 0 命中。

### 17. CLAIM-05
- 判定：STILL-BROKEN
- 原文残留：同第 1 条 doc:458。CLAIM-05 的三组内容里，12 变体（doc:505）、27/31/135 计数（doc:1115/1124）、测试名（doc:1120）都已闭合；唯一未闭合的是「文档把畸形检查描述成 `continue` 跳过」这一分句。
- 证据：见第 1 条；面板的期望复核点（`grep '12 个变体'`、`grep 'reports_segment_data_out_of_bounds'`、27 条测试）全部通过，但机械复核点覆盖不到 doc:458 这句散文，所以不能据此判 CLOSED。
- 最小修复：同第 1 条。

### 18. CLAIM-06
- 判定：CLOSED
- 证据：CLAIM-06 的实体是「v1 arena 七块载荷含 startup」。doc:366 与 doc:1022 都已改为六块；`BootHandoffV1` 六个 `*_byte_len`（`handoff_v1.rs:307-333`）与六个 `decode_*`（`lib.rs:686-746`）一致；两个发送侧与 `boot_storage.rs` 的 v10 删除记录仍在（`uefi_helpers.rs:1220-1222`、`uboot_main.rs:516-517`、`boot_storage.rs:305-310`）。`grep -n '六块载荷' doc` 命中 doc:366。

## 二、任务书重点的附加核验（不属于上述 18 条）

### A. §3.2 迭代器与 I-48/D-084 的整体一致性
- 非 `PT_LOAD` 过滤：`os/libs/minix-elf/src/lib.rs:413-415` 唯一的 `continue`；doc:483 的代码注释「非可加载段是正常过滤」与 doc:503 的说明一致。
- 畸形返回 `Err`：`:395-404`（`PhdrOutOfBounds`）、`:408-411`（`parse_phdr` 的 `Err` 原样浮出）、`:419-425`（`SegmentOutOfBounds`）、`:431-433`（`FileszExceedsMemsz`）。
- 12 变体与分组：`:157-198`，与 doc:505 的 5+2+2+3 分组逐项对上。
- loader 三个 `InvalidSegment` 点：`loader.rs:196`、`:208`、`:353`；源/目标重叠走 `panic!`（`:361-364`）。`compute_kernel_layout` 用 `seg_result?`（`:191`），`load_segments_into_phys_memory` 用 `collect::<Result<Vec<_>, _>>()?`（`:351`），与 doc:503 一致。
- 结论：除 doc:458 的题句外，§3.2 的主干与源码一致；这也是第 1/13/17 条判 STILL-BROKEN 的唯一原因。

### B. v1 是否六块载荷
- 权威定义 `handoff_v1.rs:326-333` 只有六个 `*_byte_len`；接收侧 `kernel/src/lib.rs:686-746` 只解码六类；doc:366 与 doc:1022 均写六块/六个分区。CLOSED。

### C. `kern_stack_top` 是否全为访问器
- `kernel_info.rs:200` 访问器；结构体字段列表里没有 `kern_stack_top`；doc 的两处生产调用点描述（doc:582、586）与源码 `kernel/src/lib.rs:232/370` 一致。doc 内无 `kern_stack_top:` 字段字面量。CLOSED。

### D. 测试计数与 §5.2 载体表达式
- minix-elf 27（`grep -c '    fn test_'` = 27）；loader 31（`grep -c '#\[test\]'` = 31）；boot-shim 默认 135（146 静态总数 − `opensbi_helpers` 11 条）；`--features test-all` 静态 146。
- §5.2 载体：`test-higher-half/src/main.rs:54-55` 的 `PhysBytes(image_base)` 与 `image_end - image_base` 与 doc:1152 逐字同形。CLOSED。

### E. `.lrodata` 归并与 x86 工件 LOAD 行数
- 三份生产脚本都有：`os/kernel-image/x86_64.ld:63`、`os/kernel-image/aarch64.ld:62`、`os/kernel-image/riscv64.ld:68` 均为 `*(.rodata .rodata.* .lrodata .lrodata.*)`（riscv64 另带 `.srodata`/`.sdata2`）。
- x86 工件：`readelf -lW os/target/x86_64-unknown-none/release/kernel | awk '$1=="LOAD"' | wc -l` 输出 4；`SKIP_BUILD=1 bash os/kernel-image/check-layout.sh x86_64` 全 PASS，其中 `[L3a] PASS PT_LOAD 段数 >= 1（实得 4）`。注意 `os/target/image/x86_64/staging/EFI/minix/kernel.elf` 是 06:24 的旧工件（早于 10:42 的 `.ld` 修改），它仍是 5 个 LOAD；check-layout 读的是 `os/target/x86_64-unknown-none/release/kernel`（10:42，与 `.ld` 同步），以该工件为准。CLOSED。

### F. D2-04/D2-06 的延期登记
- `rewrite-notes/coordination/review-eval/DECISIONS.md:188` 是 D-105（DESIGN-D2-04：`build_kernel_info` 改收 `BootReport`，删 12 处 `from_raw_parts`，改 5 调用点/1 生命周期），`:189` 是 D-106（DESIGN-D2-06：三份无生产读者的 arch `link.ld` 与 `link_ld_lockstep.rs`，删除或补齐）；两条都写「用户批准 2026-11-21」「批准延期到 `R4-design-batch`」「未实施（已批准延期）」。
- `rewrite-notes/coordination/review-eval/runs/01-stage-kernel/02-higher-half-kernel/OPEN-DEBT.md:2` 有 `R4-design-batch` 行，明确列出 DESIGN-D2-04 与 DESIGN-D2-06，并写「用户批准 2026-11-21（代理裁决 D-105/D-106）」。CLOSED。

## 三、机械门复跑

- `python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` → `blocks=32 checked=24 failed=0 skipped=4 warned=4`。
- `bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` → `anchors=54 resolved=54 zero-def=0 multi-def=0`。
- `python3 tools/anchor-suspect-scan.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` → 可疑锚点 0 处。
- `bash tools/doc-style-lint.sh rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` → `error级=0`。
- `SKIP_BUILD=1 bash os/kernel-image/check-layout.sh x86_64` → 全 PASS（复用既有工件，未重新构建）。

## 四、汇总

| 项 | 数量 | 编号 |
|---|---|---|
| CLOSED | 15 | FACT-02、FACT-03、FACT-04、FACT-05、FACT-06、FACT-07、FACT-08、FACT-09、FACT-10、FACT-11、FACT-12、DESIGN-D2-02、DESIGN-D2-03、DESIGN-D2-10、CLAIM-06 |
| STILL-BROKEN | 3 | FACT-01、DESIGN-D2-01、CLAIM-05 |

三条 STILL-BROKEN 是同一个残留：doc:458 那句题句与修复前 `9ebf3f054` 的 doc:457 逐字相同，批一只替换了它下面的代码块并在 doc:503 补了更正段，没有改题句。因此文档在 §3.2 内部仍存在「检查命中即 `continue`」与「只有非 `PT_LOAD` 过滤 `continue`」的正面冲突。修掉这一句后，这三条可以一并复审。

一致性：15/18 = 83.3%，低于 CONVERGED 要求的 90%，故 NOT PASS。

## 五、局限

- 未跑 QEMU 与 `cargo test`。27/31/135/146 是源码 `#[test]` 属性的静态计数与 feature 门推导，不是 cargo 注册表输出；`cargo test --list` 的实测可以把这层不确定性降为零，但本角色只读，不写 `target/`。
- x86 的 4 个 LOAD 用的是既有 `os/target/x86_64-unknown-none/release/kernel`（mtime 与 `.ld` 修改同步）与 check-layout 的 L3a 输出，不是本轮重新构建的工件；`staging/EFI/minix/kernel.elf` 是旧工件，5 个 LOAD，不能拿它反证。
- citation 门的 4 个 `warned` 是「非逐字引文」的结构豁免，覆盖不到 doc:458 这类散文句；这正是机械门全绿而事实面仍有残留的原因。
- 本回执是同一模型冷启动回读，没有第二个独立工具复核；每条都附了可重放的命令或源码行号，便于跨工具复验。
