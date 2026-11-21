# D02 R4 独立 P-FACT 报告

- 目标文档：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 行数：1289；sha256：`46a3497a40658daff0496baeb71ade5b9f188ddb21f8c5cf774f0dc3c5ddb4ab`
- 审读范围：全文；`tools/doc-code-map.sh` 生成的 30 文件清单（见 `rewrite-notes/coordination/review-eval/runs/01-stage-kernel/02-higher-half-kernel/related-code.txt`）
- 只读复核；事实依据仅限目标文档、`related-code.txt`、冻结决策片段与源码。一次全仓 `rg` 的命中列表曾带出 `panel/round*` 与旧 review 文件的文件名/片段（未主动打开、未采用其中任何内容）；本报告没有任何 FINDING 以这些文件为依据。

## 机械门读数（Step 0）

| 门 | 命令 | 退出码 | 读数 |
|---|---|---|---|
| anchor-resolve | `bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | 0 | `anchors=55 resolved=55 zero-def=0 multi-def=0` |
| doc-code-map | `bash tools/doc-code-map.sh rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | 0 | 30 个文件，存在性 30/30 通过 |
| doc-structure-check | `python3 tools/doc-structure-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | 0 | 表结构与内部 .md 链接均可解析 |
| doc-ref-check | `bash tools/doc-ref-check.sh rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | 0 | 内部章节引用均可解析 |
| doc-style-lint | `bash tools/doc-style-lint.sh rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | 0 | `error级=0` |
| doc-citation-check | `python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md --root .` | 0 | `blocks=32 checked=25 failed=0 skipped=4 warned=3` |

机械门全绿，但下文 FACT-01 表明 citation 门对 `459-497` 的引文块给出的是 WARN（该块标注里写了「注释有改写」，于是被当作非逐字引文做 structure-only 豁免），并未逐行对账；而这个块的问题是**代码行本身**已被现行实现淘汰，不只是注释改写。这属于门的能力边界，不能当作该引文块正确的证据。

---

## FINDING FACT-01（P0）

- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:457-499`（尤其 457、459-497、499）；源码 `/home/xzhao/github/minix-rs/os/libs/minix-elf/src/lib.rs:376-447`
- 严重度：P0
- 问题：§3.2 仍按 I-48/D-084 **之前**的迭代器语义描述：称 `next()` 对「表项计算/解析溢出、表项越界或太小、段文件数据越界」命中时一律 `continue`（静默不产出该段），并称「调用方看到的是『段比预期少』，而不是『段坏了』」。现行实现恰好相反：只有非 `PT_LOAD` 类型过滤是 `continue`；`checked_mul`/`checked_add` 溢出、`parse_phdr` 失败、`p_offset + p_filesz` 溢出或越过镜像尾、`p_filesz > p_memsz` 全部返回 `Some(Err(ElfError::…))`。所引代码块（459-497 行）是旧实现的逐字快照：`self.index += 1; continue;`、`Err(_) => continue`、`if seg_end > self.image.len() { continue; }` 在当前 `next()` 中都已不存在。同页 499 行「这是已知设计缺口（设计批 D2-05）：引导路径应 fail-closed」也建立在旧行为上；现行实现已经把畸形段变成 `Err`，`compute_kernel_layout` 用 `seg_result?` 短路。
- 影响：这是本篇 ELF 解析器一节的中心行为断言。读者会得出「解析器 fail-open、坏镜像只是少段」的结论，与 §4.2 自己的「`filesz > memsz` 已由迭代器拒绝」以及其他调用方（`rs/trap_api.rs`、`vm/vm_server.rs`、`arch/boot.rs` 均已把 `Err` 映射为拒绝）互相矛盾。D2-05 的描述也把仍存在的缺口（消费者若忽略 `Err` 项仍会走坏镜像）换成了已不存在的缺口。
- 证据或推理：E1：`os/libs/minix-elf/src/lib.rs:393-433` 的 `next()` 对三类畸形分别 `return Some(Err(ElfError::PhdrOutOfBounds))`、`return Some(Err(e))`、`return Some(Err(ElfError::SegmentOutOfBounds))`、`return Some(Err(ElfError::FileszExceedsMemsz))`；只有 413-415 行 `if phdr.p_type != PT_LOAD { continue; }`。E2：源码注释 326 行与 386-392 行明确写 “surfaces as `Err`, never as a silent skip (I-48 / D-084)”。E3：`python3 tools/doc-citation-check.py --verbose` 对该块（`459-497`）给出 `[WARN] … 声明为非逐字引文（structure-only），未做逐行对账`——因为标注里写了「注释有改写」；机械门因此完全没有检查这些代码行，不能否证本条。
- 最小修复：把 457-499 行改写成现行契约：`Item = Result<LoadSegment, ElfError>`；非 `PT_LOAD` 静默过滤；其余畸形一律 `Some(Err(..))`。用现行 `return Some(Err(...))` 片段替换旧 `continue` 引文块；把 D2-05 剩余缺口重述为「消费者必须短路首个 `Err`，否则仍会继续遍历坏镜像」。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若 457 行的「命中这些条件时 continue」指的是某个旧版本或某个消费侧包装器；但被引符号 `SegmentIter` 就在同文件，源码无第二个 `impl Iterator`，且 §4.2 的 `seg_result?` 与之互证。
- 对应判据：任务书第 4 条（行为合同）、第 5 条（引文块逐字一致）、第 2 条（符号锚点同源）；Doc Code Sync「符号锚点 / 内部行为描述」；线索 I-45/I-48。

## FINDING FACT-02（P0）

- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:501`；源码 `/home/xzhao/github/minix-rs/os/libs/minix-elf/src/lib.rs:155-198`、`/home/xzhao/github/minix-rs/os/boot-shim/src/loader.rs:191-208,345-354`
- 严重度：P0
- 问题：文档称 `ElfError`「共九个变体」，并把第九个 `InvalidSegment` 描述成 `filesz > memsz`、`paddr + memsz` 溢出、`kern_end_phys - kern_phys_base` 下溢、物理区间重叠都会返回它。现行枚举有 **12** 个变体：新增 `SegmentOutOfBounds`、`FileszExceedsMemsz`、`TargetBufferTooSmall { needed_end, buf_len }`。`filesz > memsz` 自 I-48/D-084 起在迭代器内返回 `FileszExceedsMemsz`，不再走 `InvalidSegment`；段文件字节越界返回 `SegmentOutOfBounds`；缓冲区装不下返回 `TargetBufferTooSmall`（I-47/D-089）。loader 侧生产代码里返回 `InvalidSegment` 的点当前是 3 个（`loader.rs:196` 的 `checked_add`、`:208` 的 `checked_sub`、`:353` 的段间重叠），不是「四个」。
- 影响：§3.2 的错误分类表被读者当作错误处理契约；按旧表匹配 `InvalidSegment` 会漏掉三个新变体。§5.4 M6 的「九个变体」也继承同一错误。
- 证据或推理：E1：`os/libs/minix-elf/src/lib.rs:157-198` 枚举逐项可数，共 12 项。E2：`loader.rs:700-721` 的测试 `test_compute_kernel_layout_rejects_filesz_greater_than_memsz` 断言 `Err(ElfError::FileszExceedsMemsz)`，测试消息原文写 “instead of the caller's generic InvalidSegment”。E3：`rg -n 'InvalidSegment' os/boot-shim/src/loader.rs` 的非测试返回点只有 196、208、353。
- 最小修复：把「九个变体」改为 12 个并按现行生产/消费方分组：文件头五道 → `TooShort/BadMagic/Not64Bit/NotLittleEndian/NotExecutable`；Phdr 表两道 → `PhdrOutOfBounds/PhdrEntryTooSmall`；迭代器段形两道 → `SegmentOutOfBounds/FileszExceedsMemsz`；loader 三道 → `NoLoadSegments/InvalidSegment/TargetBufferTooSmall`。删除 `InvalidSegment` 原因列表里的 `filesz > memsz`，把「四个判断点」改为三个并点名 `checked_add/checked_sub/overlap`。同步 §5.4 M6。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若 `ElfError` 有 `#[non_exhaustive]` 或另有别名枚举；源码 155-198 行是唯一 `enum ElfError`，无别名。
- 对应判据：任务书第 4 条（错误码/返回类型）、第 2 条（同源符号）；Doc Code Sync「常量/符号」；线索 I-45/I-47/I-48。

## FINDING FACT-03（P0）

- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:409`；源码 `/home/xzhao/github/minix-rs/os/boot-shim/src/loader.rs:218-240,509-518`
- 严重度：P0
- 问题：文档称 boot-shim「模块是按原字节整块拷贝的，**不经解析**——`loader.rs` 的 `fn load_boot_modules_into` 里**没有段迭代**」。现行 `load_boot_modules_into` 在拷贝前调用 `elf_file_end(data)`，后者构造 `minix_elf::segment_iter(data)` 并遍历所有 `PT_LOAD`，取最后一个 `offset + filesz` 作为运行期文件尾；ELF 模块会被修剪到该长度（I-36/D-082），只有非 ELF 或畸形镜像才保留全长（fail-safe）。「不经解析/没有段迭代」与「整块拷贝」两句话都已不成立。
- 影响：这恰好掩盖了本篇 §3.2 主题（同一个解析器）在模块装载层的真实使用；读者会以为模块路径不依赖 ELF 语义，从而漏掉 E 类 ABI 风险：任何 `elf_file_end` 的实现变动会同时改变 12 个模块的 `BootModule.len`，而 `BootModule.len` 是内核映射模块的边界。§5.1 也没有对应的 `elf_file_end` 测试说明。
- 证据或推理：E1：`loader.rs:517` `let used = elf_file_end(data).unwrap_or(data.len()); let data = &data[..used];`。E2：`loader.rs:227-240` 的 `elf_file_end` 首行即 `let iter = minix_elf::segment_iter(data).ok()?;`，随后 `for seg in iter { let seg = seg.ok()?; end = end.max(...) }`。E3：同文件 509-516 行注释明确写 “D-082（I-36）：修剪到 ELF 的运行期文件长度……这是模块装载里唯一需要 ELF 语义的层”。E4：测试 `test_load_boot_modules_into_trims_elf_to_file_end`（loader.rs:1085 起）断言 ELF 模块 `len == 0x1800`、非 ELF 模块 `len == 100`。
- 最小修复：把该括号句改为「模块 ELF 会用同一解析器修剪到最后一个 `PT_LOAD` 的文件尾（`elf_file_end`），非 ELF/畸形镜像保留全长；模块字节仍按原字节整块拷贝，不做逐段重定位」。在 §5.1 补 `elf_file_end_*` 与 trim 测试名。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若该句描述的是内核侧（`minix_kernel`）而不是 boot-shim；句首主语明确是 `loader.rs` 的 `load_boot_modules_into`，不成立。
- 对应判据：任务书第 6 条（覆盖率缺口——本篇声称的解析器用途清单漏了模块修剪这一真实用途）；线索 I-33/I-36；Doc Code Sync「内部 TODO/行为」。

## FINDING FACT-04（P1）

- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:365`、`:1018`；源码 `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/handoff_v1.rs:290-296`、`/home/xzhao/github/minix-rs/os/libs/minix-boot/src/handoff_v1_read.rs:76-93,124-145`、`/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:680-760`
- 严重度：P1
- 问题：文档两处把 v1 handoff 的载荷写成「七块载荷/七个分区」，并把 `startup` 列为其中一个分区。wire 版本已升到 10（D-062）：`user_sp`/`kern_stack_top`/`syscall_entry` 三个派生标量**与 startup-object 分区一起**从信封移除，startup arena 分区（20 × 72 = 1440 B）已退役；当前信封只有六个 payload 描述符（memmap/reserved/modules/platform/param/sources），`arch_boot_resume_v1` 也只解码六块。文档 §3.3 已经写对「v10（D-062）删除了 kern_stack_top」，但 §3.1/§4.4 交接段仍保留旧分区表。
- 影响：生产交接路径的载荷清单是读者对 v1 协议的第一手依据；多出的 `startup` 会让读者在 `handoff_v1_read.rs` / `arch_boot_resume_v1` 里找不到对应解码器，并误判 `validate_requiring_sources` 要求的来源记录范围。也说明 D-062 只被部分吸收进本篇。
- 证据或推理：E1：`handoff_v1.rs:290-296` 的版本注释原文：“10 = … and the startup-object partition left the envelope (D-062) … the startup arena partition (20 × 72 = 1440 B) is retired”；`BOOT_HANDOFF_V1_VERSION = 10`。E2：`handoff_v1_read.rs:82-88` 与 `:124-129` 的 scalars 只有六个 `*_byte_len`，无 startup。E3：`os/kernel/src/lib.rs:690-760` 依次 `decode_memmap/decode_reserved_regions/decode_modules/decode_platform_sources/decode_param_buf/decode_sources`，无 startup。E4：`main.rs:374` 的 “seven payload partitions” 是另一处同源陈旧注释，不能当作事实基线。
- 最小修复：两处都改为「六块载荷（memmap/reserved/modules/platform/param/sources）」，删除 `startup`；如需讲历史，另写一句「v10/D-062 之前的 startup 分区已退役」。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若仓库内另有一套旧 v9 协议仍被生产腿使用；`BOOT_HANDOFF_V1_VERSION=10` 且 `decode_envelope` 拒绝非 10，生产路径不存在 v9。
- 对应判据：任务书第 1 条（可解析来源锚点与路径）、第 4 条（结构体/协议字段）；线索 v10/D-062；Doc Code Sync「路径/常量/字段」。

## FINDING FACT-05（P1）

- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1111`、`:1120`、`:1169`、`:1170`
- 严重度：P1
- 问题：测试计数与实测不一致，且文档自己给出的复算命令会当场打脸：
  1. 1111 行：「`os/libs/minix-elf` 的 23 个测试（复算：`grep -c '    fn test_' … 输出 23`）」。实测 `grep -c '    fn test_' os/libs/minix-elf/src/lib.rs` 输出 **27**；`cd os && cargo test -p minix-elf --lib -- --list` 输出 `27 tests, 0 benchmarks`。
  2. 1120 行：「`loader.rs` 部分有 22 个测试（复算：`grep -c '#\[test\]' … 输出 22`；`cargo test -p boot-shim --lib` … 共 125 个测试，其中 22 个属于 `loader::tests`）」。实测 `grep -c '#\[test\]' os/boot-shim/src/loader.rs` 输出 **31**；`cargo test -p boot-shim --lib -- --list` 输出 `135 tests, 0 benchmarks`，其中 `loader::tests::` 前缀 **31** 条。
  3. 同段「模块那一侧的两条测试」：实际与模块装载有关的测试有 3 条（`test_load_boot_modules_loads_all_in_contract_order`、`test_load_boot_modules_into_trims_elf_to_file_end`、`test_load_boot_modules_panics_on_missing`），另有 3 条 `test_elf_file_end_*`。
  4. 1169/1170 行（§5.4 M6/M7）重复「minix-elf 23 测试」「loader 的 22 个测试中 13 个直接相关」。
  5. 计数漂移的原因是 I-45/I-47/I-48/D-082/D-089 新增的测试没有被并入清单：`test_segment_iter_truncated_mid_segment_is_err`、`test_segment_iter_filesz_exceeds_memsz_is_err`、`test_segment_iter_non_load_filter_stays_silent`、`test_segment_iter_rejects_filesz_greater_than_memsz`（minix-elf）；`test_compute_kernel_layout_rejects_truncated_mid_segment`、`test_load_segments_into_phys_memory_rejects_truncated_mid_segment`、`test_load_segments_into_buffer_propagates_truncated_mid_segment`、`test_load_segments_into_buffer_too_small_writes_nothing`、`test_load_segments_into_buffer_fitting_load_counts_all`、`test_elf_file_end_*` 3 条、`test_load_boot_modules_into_trims_elf_to_file_end`（loader）。
- 影响：测试名对账门（任务书第 3 条）在这些位置失败；读者按文档的复算命令无法得到文档声称的数字，属于可机械证伪的事实错误。§5.4 的覆盖状态（M6「已覆盖」、M7「13 个直接相关」）建立在旧计数上。
- 证据或推理：E1：上述三条命令的实际输出（记录于本报告开头与本次会话）：27 / 31 / 135。E2：`cargo test -p minix-elf --lib -- --list` 末尾 `27 tests`；`cargo test -p boot-shim --lib -- --list` 末尾 `135 tests`；`grep -c '^loader::tests::'` 为 31。E3：文档 1120 行自己声明的复算命令与实测输出相矛盾，这是自证式证据。
- 最小修复：把 1111 的 23 改为 27、把四族表补上四条 I-48 测试；把 1120 的 22/125 改为 31/135，并更正「两条模块测试」；1169/1170 同步。若不想维护精确整数，可改成「截至本轮 `cargo test -- --list` 的 N 条」并保留命令。
- 置信度：高
- 是否需源码核实：是（已核实，命令输出）
- 若我错了：若测试被 feature gate 排除了某些条目；两个复算命令都未带 feature，且 `--list` 是默认特性下的可执行集合，文档也未声明 feature。
- 对应判据：任务书第 3 条（测试名对账：数量与实测一致）；Doc Code Sync「测试计数」；R3 后 I-45..I-48/D-089 线索。

## FINDING FACT-06（P0）

- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1116`（表行）；源码 `/home/xzhao/github/minix-rs/os/libs/minix-elf/src/lib.rs:1005-1026`
- 严重度：P0
- 问题：§5.1「迭代器行为」族列出的 `test_segment_iter_skips_segment_data_out_of_bounds` 在当前源码中不存在；现行测试名是 `test_segment_iter_reports_segment_data_out_of_bounds`，且语义相反：它断言越界段产出 `Some(Err(ElfError::SegmentOutOfBounds))`，不是「越界段被跳过」。文档同表「证明什么」一列仍写「越界段被跳过」。
- 影响：测试名对账门失败；更严重的是它把 I-48/D-084 的核心反转（跳过 → 报错）写反了，与 FACT-01 同源，会让读者以为越界段是正常过滤。
- 证据或推理：E1：`rg -n 'skips_segment_data_out_of_bounds' os` 无命中；`rg -n 'reports_segment_data_out_of_bounds' os/libs/minix-elf/src/lib.rs` 命中 1005 行。E2：该测试 1017-1020 行断言 `matches!(iter.next(), Some(Err(ElfError::SegmentOutOfBounds)))`，注释写 “must surface as an error, not be silently skipped”。E3：`cargo test -p minix-elf --lib -- --list` 的 27 条里只有 `reports_…`，没有 `skips_…`。
- 最小修复：把测试名改为 `test_segment_iter_reports_segment_data_out_of_bounds`，把「越界段被跳过」改为「越界段以 `SegmentOutOfBounds` 报错」；若保留四族表，补上四条 I-48 新增测试。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若 `skips_…` 是某个 cfg(test) 下的别名；全仓 `rg` 无此标识符。
- 对应判据：任务书第 3 条（文档声称的每个测试函数是否存在）；Doc Code Sync「测试名」；线索 I-45/I-48。

## FINDING FACT-07（P2）

- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1148`；源码 `/home/xzhao/github/minix-rs/os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs:51-62`
- 严重度：P2
- 问题：§5.2 以 x86_64 载体为例写「`kern_virt_base: VirBytes(0xFFFF_8000_0000_0000)`、`kern_size: 0x200_000`、`kern_size: 0x200_000`」——同一个字段名 `kern_size` 出现两次，且现行源码里第二个字段是 `kern_phys_base: PhysBytes(image_base)`；`kern_size` 也不是字面量，而是 `image_end - image_base`（由 UEFI 镜像窗口按 2MiB 取整算出）。文档把「合成的 KernelInfo」写成了固定的字面量清单和重复字段。
- 影响：读者按此核对载体源码会对不上；两者之和（`kern_stack_top()`）与 `kern_size` 的取值来源（镜像窗口而非常量）也被简化为常量，削弱了 §3.3「现算」的论证。
- 证据或推理：E1：源码 51-62 行的字段序列为 `memmap, kern_virt_base, kern_phys_base, kern_size, boot_modules, …`，其中 54 行 `kern_phys_base: PhysBytes(image_base)`、55 行 `kern_size: image_end - image_base`。E2：36-37 行 `image_base = image_lo & !0x001F_FFFF`、`image_end = (image_hi + 0x001F_FFFF) & !0x001F_FFFF`，说明 `kern_size` 是运行期窗口跨度。
- 最小修复：把 1148 行改为「`kern_virt_base: VirBytes(0xFFFF_8000_0000_0000)`、`kern_phys_base: PhysBytes(image_base)`、`kern_size: image_end - image_base`（镜像窗口按 2MiB 取整后的跨度）」，并删掉重复的 `kern_size`。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若文档描述的是旧版载体；现行文件 51-62 行是唯一构造点，且 `rg kern_size` 无其他字面量 0x200_000。
- 对应判据：任务书第 2 条（路径）、第 4 条（字段）、第 5 条（引文块）；Doc Code Sync「常量/字段」。

## FINDING FACT-08（P2）

- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:882`；源码 `/home/xzhao/github/minix-rs/os/kernel-image/src/bootface_a64.rs:113-119`
- 严重度：P2
- 问题：文档称「`bootface_a64.rs` 的 bump 游标 `BUMP_PTR` 初值为 `BUMP_BASE`」。`bootface_a64.rs` 里没有 `BUMP_PTR`；现行符号是 `static BUMP_NEXT: AtomicU64 = AtomicU64::new(BUMP_BASE);`。`BUMP_PTR` 只在若干 QEMU 测试载体（`hello-boot-riscv64`、`test-paging-enable-riscv64`）和 `opensbi_helpers.rs` 的注释里出现，不是本句所指文件里的符号。
- 影响：这是一条用于支撑「初值非零 → 落 `.data` → 不属于 BSS 清零反例」的论据。论据成立（初值确实非零），但符号名指错文件，读者按名检索会落到测试载体或注释上，无法在 `bootface_a64.rs` 复核。
- 证据或推理：E1：`rg -n '\bBUMP_PTR\b' os --glob '*.rs'` 的命中全部在 `os/qemu-tests/...` 与 `os/boot-shim/src/opensbi_helpers.rs:774,782`（注释）；`bootface_a64.rs` 零命中。E2：`bootface_a64.rs:119` 定义 `BUMP_NEXT`，`:130`/`:134` 读写它，注释写 “Monotonic cursor over [BUMP_BASE, BUMP_BASE + BUMP_SIZE)”。
- 最小修复：把 `BUMP_PTR` 改为 `BUMP_NEXT`（或改写成「`bootface_a64.rs` 的 bump 游标（`static BUMP_NEXT: AtomicU64`，初值 `BUMP_BASE`）」）。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若文档指的是测试载体的同名符号；句子主语明确是 `bootface_a64.rs`，且其初值是 `BUMP_BASE`，与测试载体的 `DRAM_BASE + 0x0200_0000` 不同。
- 对应判据：任务书第 2 条（符号是否可解析）、第 4 条（字段/常量）；Doc Code Sync「符号锚点」。

## FINDING FACT-09（P2）

- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:323`、`:325`（以及 §2.4 的 ARM 行 208 的铺垫）；C 源码 `/home/xzhao/github/minix-rs/minix3/minix/kernel/arch/earm/Makefile.inc:58-64`、`/home/xzhao/github/minix-rs/minix3/minix/kernel/arch/earm/mpx.S:265-272`、`/home/xzhao/github/minix-rs/minix3/minix/kernel/arch/earm/kernel.lds:20-22`
- 严重度：P2
- 问题：§2.5 两处把 ARM 侧的结论写成「按同一判据、改名名单与 `.data` 收段证据待补」。但这条证据链在仓库里已经完整存在，且三处都能静态核：① `earm/Makefile.inc` 的 `.for unpaged_obj in …` 名单不含 `mpx.o`（`mpx.S` 只在 `SRCS+=` 行），故不产出 `unpaged_mpx.o`；② `earm/mpx.S:265-272` 在 `.bss` 后切回 `.data`，`k_initial_stack`/`__k_unpaged_k_initial_stktop` 落在 `.data`；③ `earm/kernel.lds:21` 的 `.unpaged_data` 只收 `unpaged_*.o(.data .rodata*)`，`mpx.o` 的 `.data` 只能进 `. += _kern_offset` 之后的通用 `.data`。结论与 i386 同向（高半），并不「待补」。
- 影响：§2.5 和 §2.4 末尾因此留下一个悬空结论（「ARM 侧待补」），与本章「两篇结论一致、三件事必须对齐」的收束目标不符；也是任务书第 8 条要求的阴性声明没有写扫描边界：没说扫了哪三个文件、N=0 的边界在哪。
- 证据或推理：E1：`earm/Makefile.inc:61-64` 名单为 `head.o pre_init.o direct_tty_utils.o pg_utils.o klib.o utility.o ${MINLIB_OBJS_UNPAGED} ${MINC_OBJS_UNPAGED} ${SYS_OBJS_UNPAGED} ${BSP_OBJS_UNPAGED}`，无 `mpx.o`；`:73` `SRCS+= mpx.S …`。E2：`earm/mpx.S:265-272` 原文 `.data` / `.short 0x526F` / `.bss` / `.data` / `.balign 4` / `k_initial_stack:`。E3：`earm/kernel.lds:21` `.unpaged_data ALIGN(4096) : { unpaged_*.o(.data .rodata*) }`，与 i386 的 `.unpaged_data` 同形。
- 最小修复：把两处「待补」改为结论句，并附三行证据（Makefile 名单、mpx.S 段切换、earm/kernel.lds 的 `.unpaged_data` 收段规则）；若确实还有未核项，写出「已扫 X/Y/Z，未核 W」而不是笼统「待补」。
- 置信度：高
- 是否需源码核实：是（已核实三处）
- 若我错了：若 ARM 构建另有 `mpx.S` 的改名规则或 `.unpaged_data` 还收别的输入；`earm/Makefile.inc` 全文只有一处 `.for unpaged_obj`，`mpx.S` 不在其中。
- 对应判据：任务书第 6 条（覆盖率缺口）、第 8 条（阴性声明须写扫描范围/N=0 边界）；§2.5 的结论完整性。

## FINDING FACT-10（P3）

- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:279-294`、`:90-110`、`:806`、`:1073`
- 严重度：P3
- 问题：四处引文标注与实际省略/改写方式不符：
  1. 279 行标签写「minix3/minix/kernel/arch/i386/mpx.S（文件末尾的段指示与栈定义）」，但该文件在 `LABEL(k_boot_stktop)`（641 行）之后还有 `k_stacks_start`/`k_stacks_end` 等 10 行到 651 行；引文尾部省略没有 `...`，且「文件末尾」不准确。
  2. 90-110 行的 `kernel.lds` 引文在 `.text` 行结束，而文件后面还有 `.data`、`.bss`、`_end`；标签只说「保留位置计数器起点与各段头部」，未标尾部省略。
  3. 806 行标签写「注释有删节（循环体内 3 行源注释删除）」，但源码循环体内是 6 行注释，引文保留的 2 行是按文意重写的（源文为 “A malformed program header (truncated segment, unreadable phdr, filesz > memsz) means …”，引文为 “I-48 / D-084: a malformed program header is an Err item…”），应标「注释改写/压缩」，不是「删除 3 行」。
  4. 1073 行称 `store_kernel_info` 的「文档注释」给出 `The reserved_regions payload lives in the boot-shim's UEFI-pool heap…`；该句实际位于函数体内 `unsafe` 块的普通注释（`lib.rs:1589-1591`），不是 `///` 文档注释。
- 影响：引文纪律要求「逐字引用 + 省略处标注」；这些标注让读者以为未标 `...` 的部分要么在文中、要么只被删除，实际是尾部省略或改写。第 3 条尤其重要：被改写的那两行恰好是本篇讨论 I-48 语义的句子，与 FACT-01 的旧行为描述相互纠缠。
- 证据或推理：E1：`wc -l mpx.S` = 651；`grep -n 'LABEL(k_boot_stktop)\|k_stacks_start\|k_stacks_end'` = 641/644/649。E2：`kernel.lds` 第 29 行是 `.text`，30-36 行还有 `.data`/`.bss`/`_end`。E3：`loader.rs:185-190` 循环注释 6 行，文档 816-818 只有 2 行且措辞不同。E4：`lib.rs:1589` 注释前缀是 `//` 而非 `///`，其所属函数文档注释在 1564-1573 行。
- 最小修复：给两处尾部省略补 `...` 并把「文件末尾」改为「文件尾部附近（`k_boot_stktop` 及其前文）」；把 806 行标签改为「注释改写/压缩」或还原源码 6 行注释；把 1073 行的「文档注释」改为「函数体注释」。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若仓库的引文规则允许「节选」自动豁免尾部省略；但 `doc-citation-check.py` 的规则要求省略用 `...`/`…` 标注，且本任务第 5 条明确要求「节选是否标注省略」。
- 对应判据：任务书第 5 条（引文块逐字一致、节选标注省略）；`prompt` 引文纪律。

## FINDING FACT-11（P3）

- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1004`；源码 `/home/xzhao/github/minix-rs/os/kernel/src/arch/riscv64/higher_half.rs:41-57`
- 严重度：P3
- 问题：文档用行号锚点「`os/kernel/src/arch/riscv64/higher_half.rs:41-52`」指向 direct-leg 说明，但 41 行是空注释行，52 行已经是 resume-leg 说明的首行（`// - **Resume leg** …`）；direct leg 的实际范围是 42-51。行号范围整体偏移一格。
- 影响：低；读者按范围读会多读到一句 resume leg，且项目规则优先使用符号锚点而非行号。但这是一条可机械证伪的事实引用，仍是 P3 事实错误。
- 证据或推理：`nl -ba os/kernel/src/arch/riscv64/higher_half.rs | sed -n '41,57p'`：41 = `//`，42 = `// - **Direct leg** …`，51 = `// real hardware: PC now reads 0xffffffc0…`，52 = `// - **Resume leg** …`。
- 最小修复：把范围改为 `:42-51`，或改为符号锚点 `:fn jump_to_kmain` / `impl HigherHalf`。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若行号计算从 0 开始；编辑器与 `nl` 均为 1-based，文档其他行号也是 1-based。
- 对应判据：任务书第 1 条（可解析来源锚点）、第 2 条（路径/符号锚点纪律）。

## FINDING FACT-12（P1）

- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:578`、`:582`、`:1099`；源码 `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/kernel_info.rs:186-204`、`/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:232,370`
- 严重度：P1
- 问题：三处仍把 `kern_stack_top` 当**字段**：§3.4 「断言只覆盖 `kinfo.kern_stack_top`；形参由两个调用点的写法保证（**都传该字段**）」、 「两个生产调用点都从 `kinfo.kern_stack_top` 取值」，以及 §4.5 收尾「栈顶值**进结构体字段**」。v10（D-062）已经把该字段从 `KernelInfo` 与 wire 删除，现行是访问器方法 `KernelInfo::kern_stack_top(&self) -> VirBytes`；两个生产调用点写的是 `kinfo.kern_stack_top()`（带括号）。断言覆盖的是 `boot_validate_and_prepare` 里由 `kern_virt_base + kern_size` 现算的局部值，不是某个字段。§3.3（522/526 行）与 §4.5 表格（1095 行）已正确写成访问器，这三处残留旧字段口径。
- 影响：这正是 R3 后 D-062 的「旧字段」残留：读者会以为交接结构里还有一个 `kern_stack_top` 字段，并把「断言覆盖该字段」理解成对交接载荷的校验；实际断言覆盖的是本地派生值，交接载荷里没有这个字段。降级地说，Rust 里 `kinfo.kern_stack_top`（无括号）也不是可取值表达式，会让按文档写代码的读者编译失败。
- 证据或推理：E1：`kernel_info.rs:189-204` 的文档注释写 “v10 (D-062) removed `kern_stack_top` from both the wire and this struct”，下面是 `pub fn kern_stack_top(&self)`。E2：`lib.rs:232` 与 `:370` 均为 `H::jump_to_kmain(kinfo, kinfo.kern_stack_top())`。E3：`lib.rs:938-959` 的断言主体是 `boot_validate_and_prepare` 内的局部 `let kern_stack_top = VirBytes(kv.checked_add(ks)...)`，不是 `self.kern_stack_top` 字段。
- 最小修复：把 578 行改为「断言只覆盖 `boot_validate_and_prepare` 现算的 `kern_stack_top`（= `kinfo.kern_stack_top()` 的同一个值）；形参由两个调用点的写法保证（都传 `kinfo.kern_stack_top()`），不是断言保证」；把 582 行的 `kinfo.kern_stack_top` 改为 `kinfo.kern_stack_top()`，并把「并回 `kinfo`」改成「改为从 `kinfo` 现算、删除形参」；把 1099 行的「栈顶值进结构体字段」改为「栈顶值进派生访问器」。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若文档的 `kinfo.kern_stack_top` 是方法引用的简写而非字段声明；但同句的「该字段」与「并回 kinfo」明确按字段语义使用，且 §3.3 已改为访问器口径，前后不一致本身成立。
- 对应判据：任务书第 2 条（符号是否可解析）、第 4 条（结构体字段/签名）；线索 v10/D-062「删除的 KernelInfo 字段」；Doc Code Sync「符号锚点/行为」。

---

## 反事实前提核对（任务书第 7 条）

按「前提对象是否真实存在」核对全文的“如果反过来”句，未发现虚构前提；逐条状态：

| 位置 | 反事实前提 | 前提是否真实存在 | 证据 |
|---|---|---|---|
| §2.1 `:129` | 五处 `AT()` 可统一改成 `AT(_kern_phys_base)` | 是：`kernel.lds` 确有 5 处 `AT(` | `grep -c 'AT(' kernel.lds` = 5（第 25/27/29/30/32 行） |
| §2.1 `:131` | 只删 `AT()`、保留高 VMA，装载器会向 VMA 写 | 是：删 `AT()` 后 LMA=VMA 是链接器规则；外部装载器读 `p_paddr` | 设计推演，未执行链接 |
| §2.2 `:210` | 把 `pg_utils.o` 从改名名单去掉 | 是：名单在 `Makefile.inc:51-54`；`pre_init.c` 不在任何 SRCS | `grep` 只见注释提及 `pre_init.c`；`pre_init.c:230-234` 确有五连调用 |
| §2.3 `:268` | 只切栈不跳，PC 停在 `hang` | 是：`head.S:90-91` 有 `hang: jmp hang` | 源码 |
| §2.3 `:270` | 只跳不切，`call` 压返回地址到旧栈 | 是：`load_stack` 在 `head.S:94-96`，设栈取 `load_stack_start` | 源码 |
| §2.3 `:272` | 把切栈挪进 `kmain` 内部 | 是：`call` 前序言会动栈；`head.S` 的设栈在 `call` 之前 | 源码 |
| §2.4 `:313` | 栈定义落在 `.unpaged_bss` 会被回收 | 是：`.unpaged_bss` 规则存在；`main.c:294-301` 的 bootstrap 回收存在 | 源码 |
| §3.1 `:403` | 给 `x86_64.ld` 去掉 `AT()` | 是：四条 `AT` 存在 | `grep -c ': AT(' x86_64.ld` = 4 |
| §3.2 `:516` | 把步进从 `e_phentsize` 改成结构体大小 | 是：`next()` 用 `e_phentsize` 计算 `entry_off` | `lib.rs:394-405` |
| §3.3 `:557` | 去掉 2MiB 收口 | 是：三份脚本都有 `. = ALIGN(0x200000)` | 三份 `.ld` |
| §3.4 `:595` | 把跳转函数写成返回 `()` | 是：trait 返回 `!` | `higher_half.rs:31-49` |
| §3.5 `:657` | 把收口改成 4KiB | 是：`ALIGN(0x200000)` 可替换；后果链未执行（只读） | `riscv64.ld:104` |
| §4.1 `:767` | 把 `.lbss` 从 `.bss` 通配去掉 / 挪走 `KEEP(*(.text.boot))` | 是：两条语句都存在 | `x86_64.ld:81,28` |
| §4.1 `:765` | 把基址写成 `0xFFFFFC0000000000` | 是：该常量可写；VPN[2] 复算已核 | `printf` 输出 0x0 |
| §4.2 `:882` | 把 BSS 清零从装载方与入口汇编两处都去掉 | 是：两处都存在（loader `write_bytes`；`main.rs` 的 `__bss_start/__bss_end` 循环） | `loader.rs:371-375`；`main.rs:154-155` |
| §4.3 `:1004` | riscv64 去掉 `add t1, t1, t2` | 是：该行在 asm 中 | `higher_half.rs:84` |
| §4.3 `:1006` | aarch64 去掉 `isb` | 是：该行在 asm 中；`enable()` 确有 `dsb sy; isb` 两处 | `higher_half.rs:38`；`arm64/paging.rs:720-735` |
| §4.3 `:1008` | x86_64 去掉 `xor rbp, rbp` | 是：该行在 asm 中 | `higher_half.rs:36` |
| §4.4 `:1075` | 自引导镜像照搬 v1 重拷自己的段 | 是：bootface 当前只拷模块，不重拷段 | `bootface.rs` 模块文档 |
| §4.5 `:1101` | 把 C 两段式布局照搬进 Rust | 是：Rust 生产镜像无 unpaged 段 | `os/kernel-image/*.ld` 只有高半段 |

反事实中唯一内容错误是 FACT-08（`BUMP_PTR` 符号名），其论据本身（初值非零 → `.data`）成立。

## 阴性声明与「证据不足」清单（任务书第 8 条）

| 检查 | 扫描范围 | 发现数 | N=0 时的边界 |
|---|---|---|---|
| `BUMP_PTR` 是否存在于 `bootface_a64.rs` | 该文件全文 + 全仓 `*.rs` `rg '\bBUMP_PTR\b'` | 0（该文件）；全仓命中仅测试载体与 `opensbi_helpers.rs` 注释 | 只证明该文件无此符号；不排除文档另指测试载体 |
| `test_segment_iter_skips_segment_data_out_of_bounds` | 全仓 `rg` | 0 | 只证明无此测试名；同名逻辑可能以别的名字存在（已找到 `reports_…`） |
| v1 wire 的 `startup` 分区 | `handoff_v1.rs`、`handoff_v1_read.rs`、`handoff_v1_write.rs`、`arch_boot_resume_v1` | 0 个分区描述符/解码器 | 只证明当前 v10 wire；不能证明历史 v9 没有（已由版本注释确认 v10 退役） |
| Rust 测试是否引用 C `kernel.lds` | `rg -l 'kernel\.lds' os --glob '*.rs'` | 0 | 支持 §5.4 M1「C lds 无机器断言」，但未扫非 Rust 测试脚本 |
| `LoadSegment.align` 的语义消费者 | `loader.rs`、`vm_server.rs`、`trap_api.rs`、`arch/boot.rs` | 0（仅 fixture 初始化 `align: 0x1000`） | 支持「`p_align` 当前没有读者」 |
| `os/kernel/src/arch/*/link.ld` 的链接读者 | 文档给出的 `os/qemu-tests/*.sh os/xtask/src/*.rs` + 全仓 `*.rs`/`*.toml`/`*.sh`/Makefile | 0 个构建输入；6 处源码注释（不含 `os/target/` 生成副本） | 若把生成目录算入「全仓」会多 1 处；结论（无生产链接读者）不变 |
| R02-T01/T03/T04 是否已在 TODO 台账登记 | 未扫（任务书禁止读取历史报告/todo 之外的台账；允许清单中无 TODO 文件） | 未验证 | 不据此下结论；本报告不把 R02 编号当事实 |
| 真机读数（`0x1ddda2ee`、`0x8020…`、E2 全零） | 未复跑 QEMU/真机 | 未验证 | 文档中这些是历史现场；本报告只核对现存源码与注释，不把历史读数判为对或错 |
| GNU ld 的 LMA 重叠报错行为 | 未执行外部工具实验 | 未验证 | 不据此下结论；FACT 未以该行为为依据 |

## SUGGESTION

1. **§3.3 `:522` 措辞**：`kern_stack_top` 本身就是 v10 删除的三个派生标量之一，句子的「字段与三个派生标量一起删除」读起来像四个；建议改为「该字段与另外两个派生标量（`user_sp`/`syscall_entry`）一起删除」。
2. **§7.2 的 `minix3/minix/servers/vm/pagetable.c`**：全文只在 §7.2 清单出现一次，§2–§4 无任何讨论或锚点；建议要么在正文补一句它和「页表多级结构归 07 篇」的关系，要么从清单移到背景阅读。
3. **§3.1 `:359` 的阴性声明**：建议给检索面加一句「不含 `os/target/` 生成副本」；字面执行「全仓」会多命中 `os/target/fmtchk/kernel_src_lib.rs` 的生成拷贝。
4. **§5.1 `:1124` 的「每个环节都有宿主测试」**：同段后文明确说 `load_segments_into_phys_memory` 的物理写入在宿主上不可执行；建议改为「解析/布局/缓冲区语义每个环节」。
5. **§3.2 `:457` 的「畸形输入会 panic 而不是 Err」**：`SegmentIter::new` 的未检查算术在 debug 下 panic、release 下可能回绕后由 `next()` 返回 `Err`；若保留这句，建议注明构建形态，或改成「`new()` 的表范围算式仍未检查；`next()` 已把可检出的畸形项返回为 `Err`」。

---

## 完成声明

- 已按任务书 0–8 项完成：六道机械门实跑（读数见上表）；锚点/路径/常量/签名/行为合同逐条对源码；测试名与数量用 `cargo test -- --list` 与 `grep` 双向对账；引文块做人工逐行比对；反事实前提逐条列表；阴性声明补扫描边界。
- 未执行：完整单元测试套件（只做 `--list` 编译与计数）、QEMU 真机跑、TODO 台账核对（不在允许读取范围）、GNU ld 外部工具实验。
- 已复核的关键源码路径：`os/libs/minix-elf/src/lib.rs`、`os/boot-shim/src/loader.rs`、`os/libs/minix-boot/src/{kernel_info,handoff_v1,handoff_v1_read}.rs`、`os/kernel/src/{lib,boot_policy}.rs`、`os/kernel/src/boot/higher_half.rs` 与三架构 `higher_half.rs`、`os/kernel-image/{build.rs,x86_64.ld,aarch64.ld,riscv64.ld,check-layout.sh}`、`os/kernel-image/src/{bootface,bootface_a64,bootface_common,main}.rs`、`os/kernel/tests/link_ld_lockstep.rs`、`os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs`，以及 `minix3/minix/kernel/arch/i386/{kernel.lds,head.S,mpx.S,Makefile.inc,pre_init.c,pg_utils.c,protect.c}`、`minix3/minix/kernel/{main.c,const.h,proto.h}`、`minix3/minix/kernel/arch/earm/{kernel.lds,head.S,mpx.S,Makefile.inc}`、`minix3/sys/arch/i386/include/asm.h`、`minix3/sys/arch/x86/include/loadfile_machdep.h`。
- **FINDING 总数：12**（P0=4，P1=3，P2=3，P3=2）。
- **SUGGESTION 总数：5**。


