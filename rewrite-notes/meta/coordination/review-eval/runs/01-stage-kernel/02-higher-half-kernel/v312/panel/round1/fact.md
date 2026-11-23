DOC 02-higher-half-kernel.md

# D02 事实审查（V3.12 R1）

- 目标文档：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- sha256：`95e32868e74b730663b9f436805546ecce8fa14e9ca1a314da4faea8cb25736a`（与 prompt 给定值一致）
- 行数：2,120
- 审查范围：目标文档、`related-code.txt` 声明的 30 个 C/Rust 文件、只读命令、冻结决策 D-024/D-047/D-062/D-116/D-117/D-119
- 结论：9 条 FINDING（2 条 P0、4 条 P2、3 条 P3）。其中 FACT-01（D-119/DESIGN-02(b) 未同步）与 FACT-02（`[ARCH]` 标记按 D-047 的硬阻断）需在收敛判定中按 P0 处理；按 D-047，FACT-02 使本轮结果进入 BLOCKED 轨道。

## 机械门读数（先跑，供引用）

| 门 | 命令 | 读数 |
|---|---|---|
| 锚点解析 | `./tools/anchor-resolve.sh rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | `anchors=65 resolved=65 zero-def=0 multi-def=0` |
| 文件映射 | `./tools/doc-code-map.sh …` | 30 个文件，存在性 30/30 通过 |
| 结构 | `python3 tools/doc-structure-check.py …` | ✅ 表结构与内部 `.md` 链接均可解析 |
| 内部引用 | `./tools/doc-ref-check.sh …` | ✅ 内部章节引用均可解析 |
| 文风（仅取读数） | `./tools/doc-style-lint.sh …` | `error级=0` |
| 引用对账 | `python3 tools/doc-citation-check.py …` | `blocks=47 checked=27 failed=0 skipped=15 warned=5` |
| 出版检查 | `python3 tools/doc-publication-check.py …` | 无新增违规 |
| 承载门 | `bash tools/doc-code-density-check.sh …` | 4 个可判章均有码/命令，通过 |
| 测试计数 | `grep -c '    fn test_' os/libs/minix-elf/src/lib.rs`；`grep -c '#\[test\]' os/boot-shim/src/loader.rs`；`cargo test -p boot-shim --lib -- --list` | 27；32；`136 tests`（其中 `loader::tests::` 32 个） |
| 宿主测试 | `cd os && cargo test -p minix-elf` | `27 passed; 0 failed` |
| 布局门 | `bash os/kernel-image/check-layout.sh all` | 三架构全部 PASS；L4 平移量 `0xffff800000000000` / `0xffff7fffbfe00000` / `0xffffffbf7fe00000` |

`doc-citation-check` 的 5 个 WARN 块为 754–776、792–833、1307–1353、1470–1493、1913–1925。WARN 成因是块首注记含「改写」字样，工具转入 structure-only；这类块没有被逐行对账，本报告对它们做了人工核对（见 FACT-06）。

## FINDING FACT-01（P0）

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` 第 4、86、691、716、884、1207、1646、1655、1929 行；涉及 `os/kernel-image/src/main.rs:115`、`os/libs/minix-boot/src/boot_storage.rs:262`、`os/kernel/src/lib.rs:1578`。
- 严重度：P0
- 问题：D-119/DESIGN-02(b) 已把五条腿的**发送侧**统一为 `BootReport<'_>`，由接收侧 `store_boot_report` 落 `.bss` 后构造唯一 `KernelInfo`；文档仍把发送侧写成合成 `KernelInfo`，并把 `KERNEL_ENTRY_ANCHOR` 的类型写成旧的 `fn(&KernelInfo, PhysBytes) -> !`。
- 影响：读者按 §4.5 的路径 A/B 或 §3.1/§3.3 的载体说明去构造 `KernelInfo` 发送，拿不到现行 API（`BootReport` 是借用视图，`KernelInfo` 是接收侧的 `'static` 形态）；`KERNEL_ENTRY_ANCHOR` 的签名不可编译。该错误同时与文档自己在第 1687 行写的「发送侧只构造 `BootReport<'_>` 借用视图」相矛盾，读者无法判断哪一处是权威。
- 证据或推理（E1）：
  - `os/kernel-image/src/main.rs:115`：`static KERNEL_ENTRY_ANCHOR: fn(&BootReport<'_>, RamAuthority, PhysBytes) -> ! = minix_kernel::arch_boot;`。
  - `os/libs/minix-boot/src/boot_storage.rs:262-290`：`report()` 返回 `BootReport<'_>`；同文件注释写明「`KernelInfo` … is the `'static` receiver-side form …, not a sender type」。
  - `os/kernel/src/lib.rs:1578-1583`：`store_boot_report(report: &BootReport<'_>, …) -> &'static KernelInfo`。
  - 发送侧现况：`os/boot-shim/src/uefi_helpers.rs:1239`、`os/boot-shim/src/opensbi_helpers.rs:426`、`os/kernel-image/src/bootface_common.rs:554` 均构造 `BootStorage::report`；三个测试载体分别在 `…/test-higher-half/src/main.rs:53`、`…-aarch64/src/main.rs:49`、`…-riscv64/src/main.rs:125` 构造 `BootReport`。
  - `os/libs/minix-boot/src/handoff_v1_write.rs:132-151`：生产发送用 `write_envelope_report(&BootReport)`；`write_envelope(&KernelInfo)` 被注释标明为 test/carrier 形态、不是生产 sender API。
  - 文档位置原文：第 691 行「以合成的 `KernelInfo` 走进同一段引导代码」；第 716 行「`KernelInfo` 的基址必须由载体自己填对」；第 884 行「合成 `KernelInfo` 的路径无法替换它」；第 1207 行「`static KERNEL_ENTRY_ANCHOR: fn(&KernelInfo, PhysBytes) -> ! …`」；第 1646 行「组 KernelInfo（v1 交接载荷）」；第 1655 行「组 KernelInfo → `minix_kernel::arch_boot`」；第 1929 行「测试载体的 `KernelInfo` 是合成的」。
- 最小修复：将上述发送侧/载体侧的 `KernelInfo` 改为 `BootReport<'_>`（保留接收侧描述中的 `KernelInfo`）；把 `KERNEL_ENTRY_ANCHOR` 的内联签名改成 `fn(&BootReport<'_>, RamAuthority, PhysBytes) -> !`；改完复跑 `grep -n '合成的 KernelInfo\|组 KernelInfo\|fn(&KernelInfo, PhysBytes)'` 应为 0 命中。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若文档把 `KernelInfo` 当作与具体 Rust 类型无关的「交接语义」泛称，且 01 篇统一采用该泛称，那么这是术语选择而非事实错误；但第 1207 行是带具体签名的内联代码，且第 1687 行已明确区分发送/接收类型，泛称解释无法覆盖该行。

## FINDING FACT-02（P0）

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` 第 417、1451 行（`[ARCH: boot 最小恒等窗口；跨镜像腿不再铺低 4 GiB]`）。
- 严重度：P0
- 问题：文档含 `[ARCH: …]` 架构演进标记；按冻结决策 D-047，`[ARCH]` 是硬阻断项，须入台账 ⑤ 且本轮结果 BLOCKED。当前允许读取范围内，代码侧（`os/arch/src/lib.rs`、`os/kernel/src/lib.rs`）没有该标记或同义 `[ARCH: …]` 注释，无法证明「文档 + 设计 + 代码」三处一致。
- 影响：该标记声称的演化（boot 最小恒等窗口、跨镜像腿不再铺低 4 GiB）在三档术语下没有完成三处一致标注；读者会把一个未闭合的架构演进当成已定稿事实，本轮也不能按 CONVERGED 处理。
- 证据或推理（E2）：
  - `grep -n '\[ARCH' rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` → 仅第 417、1451 两处，均为同一个 `[ARCH: boot 最小恒等窗口；跨镜像腿不再铺低 4 GiB]`。
  - `grep -rn '\[ARCH' os/arch/src/lib.rs os/kernel/src/lib.rs` → 无该标记；相关代码只有普通注释（`os/kernel/src/lib.rs:1049` 的 `// Steps 1-3: build the bootstrap root (minimal identity windows + kernel`），没有 `[ARCH: …]` 标签。
  - `grep -n '\[ARCH' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` → 只有第 1112 行的通用说明与其它条目的 `[ARCH: …]`，无这条 boot 最小恒等窗口标记。
  - 该标记在允许读取范围外还出现在 `rewrite-notes/01-stage-kernel/01-design-learn.md`、`rewrite-notes/01-stage-kernel/draft/v3-01-boot-shim-bootstrap.md`、`rewrite-notes/01-stage-kernel/DECISIONS-PENDING-BOOT-SCOPE.md`，但这些不是设计源或代码，不能单独充当三处一致的「设计 + 代码」两端。
  - 冻结决策 D-047 原文：「[ARCH] 硬阻断，入台账 ⑤ + 轮结果 BLOCKED。」
- 最小修复：在代码（`minix_arch` 的恒等窗口常量/`kernel` 的 Step 1 注释）与正式设计源补上同一条 `[ARCH: boot 最小恒等窗口；跨镜像腿不再铺低 4 GiB]`（或经作者裁决把文档标记降为普通叙述并留裁决记录），同时按 D-047 入台账 ⑤、本轮结果标 BLOCKED，直到三处一致。
- 置信度：高（标记存在与代码侧缺失，均经 grep 复核）；中（设计源是否在 `rewrite-notes/meta/design/` 内已有该标记，因超出本面板允许读取范围而未核）
- 是否需源码核实：是（代码侧已核；设计侧未核）
- 若我错了：若正式设计快照 `rewrite-notes/meta/design/01-stage-kernel/02-design.*` 已含同一条标记，且代码侧在本次未扫描的文件（如 `os/boot-shim/src/**`）里也有一条等价 `[ARCH: …]`，则三处一致成立；请用 `rg -n '\[ARCH.*最小恒等|\[ARCH.*identity' os rewrite-notes/meta/design` 复核。

## FINDING FACT-03（P2）

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` 第 1691 行（§4.5「两条生产路径在 `kmain` 处合流」段）。
- 严重度：P2
- 问题：该段把两条路径的交接细节混写：①「再由 `arch_boot_finish` 把同一副本地址交给 `jump_to_kmain`」只对直核路径成立，v1 接收路径的跳转在 `resume_additive`；②「`reserved_regions`/`memmap`/`boot_modules` 原本指向 boot-shim 堆上的字节」只对 v1 成立，直核自引导的 `BootStorage` 在调用方栈上（文档第 1388 行自己写了这一点）。
- 影响：读者按 §4.5 找 v1 接收的跳转点会找到错误的函数；把直核载荷来源记成 boot-shim 堆，会在排查「读回全零/页错误」时查错内存所有者。
- 证据或推理（E1）：
  - `os/kernel/src/lib.rs:673-766`：`arch_boot_resume_v1` 解码 arena 后调用 `resume_after_v1_decode`；`os/kernel/src/lib.rs:766-800` 的三个 `resume_after_v1_decode` 分别转 `arch_boot_resume_high_half` 或 `resume_additive`；`os/kernel/src/lib.rs:363-380` 的 `resume_additive` 末尾是 `unsafe { H::jump_to_kmain(kinfo) }`。`arch_boot_finish`（`os/kernel/src/lib.rs:215-248`）是直核 `arch_boot` 的 tail，不在这条链上。
  - `os/kernel-image/src/bootface_common.rs:345-350`（`run_self_boot`）：`let mut storage = minix_boot::BootStorage::new();` 在栈上；第 1388 行文档原文「`bootface_common::run_self_boot` 把 `BootStorage` 放在调用方栈上」。
  - 文档第 1691 行原文：「现行形状是接收侧先落 `.bss`、再由 `arch_boot_finish` 把同一副本地址交给 `jump_to_kmain`」；同段「原本指向 boot-shim 堆上的字节」。
- 最小修复：把该段拆成两句——v1 接收：`resume_additive` 的 `jump_to_kmain`，载荷来自 boot-shim 堆；直核：`arch_boot_finish` 的 `jump_to_kmain`，载荷来自调用方栈上的 `BootStorage`。或把函数名改成「接收侧公共体」并补注「v1 是 `resume_additive`，直核是 `arch_boot_finish`」。
- 置信度：高（函数链）；中（作者是否只用 `arch_boot_finish` 概括直核路径）
- 是否需源码核实：是（已核实）
- 若我错了：若该段的「接收侧」专指直核自引导的接收侧、前一句的 v1 只是背景，则 `arch_boot_finish` 不算错；但「发送侧曾把 shim 栈上的 `&KernelInfo` 经 `noreturn` asm 直传」明确指向 v1，段落主语并未切换。

## FINDING FACT-04（P2）

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` 第 2018 行（§6.1「诊断注记」）。
- 严重度：P2
- 问题：文档声称 x86_64 生产路径上有一块临时打印 `kernel: preA rdi=…`（源码注释自标 `temporary`）；当前 `os/kernel/src/lib.rs` 已无该打印，只有 `kernel: arch_boot entered` 这一条路标。`git log -S'preA rdi'` 显示它在 `ea18db20e`（`fix(01-01): R3 面板修复与代码设计项落地`）中被删除。
- 影响：读者按 §6.1 过滤「两块打印」时会找不到 `preA`；把已删除的取证打印当成现行噪声，也会误导对旧日志读数的判断（文档同段引用的 `0x1ddda2ee` 只属于旧版本）。
- 证据或推理（E1）：
  - `grep -rn 'preA\|rdi=' os/kernel os/kernel-image os/arch --include=*.rs` → 无命中；`grep -rn 'arch_boot entered' os/kernel/src/lib.rs` → 第 265 行命中。
  - `git log -S'preA rdi' --oneline -- os/kernel/src/lib.rs` → `ea18db20e`；`git show ea18db20e -- os/kernel/src/lib.rs` 显示以 `-` 行删除 `Console::write_str("kernel: preA rdi=");` 及其后续 `kinfo`/`rsp`/`blen` 打印。
  - 残留的 `os/target/fmtchk/kernel_src_lib.rs:553` 是构建/格式化暂存副本，不是源码。
- 最小修复：删掉该段对 `preA` 的描述，或在句首加「已删除（`ea18db20e`）」并把「两块打印」改成「一条 `arch_boot entered` 路标」；同时把 `0x1ddda2ee` 明确标成「旧版本读数」。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若 `preA` 打印在某个未编译进默认构建的 feature 分支里，我的 grep 会漏；但当前 `os/kernel/src/lib.rs` 全文无该字符串（`grep -n 'preA' os/kernel/src/lib.rs` 为空），且 git 记录显示删除，基本可排除。

## FINDING FACT-05（P2）

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` 第 924 行（§3.4「四条理由」前的段落）。
- 严重度：P2
- 问题：文档说 Step 1 的恒等窗口「按文件级常量 `PageSize::MIB_2` 分片，不经 `BootPageSupport`/`supports_huge_page`」。实际 `build_bootstrap_root_and_enable` 的 Step 1 调用 `paging.map_huge(..., PageSize::MIB_2, ...)`，而三份 `map_huge` 实现的第一道检查就是 `if !Self::supports_huge_page(size) { return Err(NotSupported) }`；Step 1 的 `match` 会把 `NotSupported` 落到 `Err(error) => panic!(...)`。它确实不经过 `BootPageSupport` 快照与策略层，但**经过** `supports_huge_page`。
- 影响：读者会以为这条硬编码 2 MiB 完全没有能力判定、只靠「三架构都支持」的口头前提；实际上它有运行期 fail-closed 检查。这削弱了该段「策略层未验证」的论证精度，也会让后续把 2 MiB 改坏的人误以为不会在建表点报错。
- 证据或推理（E1）：
  - `os/kernel/src/lib.rs:1384-1392`：Step 1 的 `match paging.map_huge(VirBytes(addr), PhysBytes(addr), PageSize::MIB_2, piece_flags)`；`Err(error)` 分支 panic。
  - `os/arch/src/x86_64/paging.rs:1246-1249`：`if !Self::supports_huge_page(size) { return Err(PageTableError::NotSupported); }`；`os/arch/src/arm64/paging.rs:1177-1180`、`os/arch/src/riscv64/paging.rs:1199-1202` 同形。
  - `os/arch/src/arch/paging_ext.rs:75-105` 的 `HugePages::supports_huge_page` 是无默认实现的能力查询。
- 最小修复：把该句改成「不经 `BootPageSupport` 快照；`map_huge` 内部仍按 `supports_huge_page` 做一次运行期能力检查并 fail-closed」，或明确区分「策略层未选」与「实现层已查」。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若作者把「`supports_huge_page`」特指 `BootPageSupport` 里的策略判定而非 `HugePages` 能力查询，语义可辩护；但两者同名且文档未限定，按字面读是错的。

## FINDING FACT-06（P2）

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` 第 792–833 行（§3.2 `SegmentIter::next` 引文块）。
- 严重度：P2
- 问题：块首注记为「节选（保留 next() 的类型过滤与四类 Err 返回；注释有改写；省略号处为返回值构造）」，但块内代码并非现行源码的逐字节选：源码用 `let seg_end = match (...) { Some(end) => end, None => return Some(Err(ElfError::SegmentOutOfBounds)) };`，文档改写成 `let Some(seg_end) = (...) else { return Some(Err(...)); };`；块内注释也与源码不同（文档「表项越界/太小是畸形…」，源码「parse_phdr already validated bounds in new(), but be defensive」）。注记只声明「注释有改写」，未声明代码改写。
- 影响：按 `doc-citation-check` 的 V3.2 边界（R30 FACT-B-01 判例），`节选` 只允许省略、不允许改写；该块会让读者把 let-else 当成现行源码，也会让后续维护者以文档为准反向修改源码。
- 证据或推理（E1）：
  - 源码 `os/libs/minix-elf/src/lib.rs:400-411`：`let seg_end = match (phdr.p_offset as usize).checked_add(phdr.p_filesz as usize) { Some(end) => end, None => return Some(Err(ElfError::SegmentOutOfBounds)), };`
  - 文档第 819–823 行：`let Some(seg_end) = (phdr.p_offset as usize).checked_add(phdr.p_filesz as usize) else { return Some(Err(ElfError::SegmentOutOfBounds)); };`
  - `python3 tools/doc-citation-check.py …` 对该块给 WARN，原因是注记含「改写」触发 structure-only；工具的 WARN 不等于已对账。
- 最小修复：把块内 `let Some(...) else` 换回源码的 `match` 形态，并把注释恢复成源码原文；若作者确实想用 let-else 做教学改写，把注记改成「示意/改写」并删掉「保留 next() 的类型过滤与四类 Err 返回」这种逐字暗示。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若「注释有改写」被作者理解为「整块（含少量代码形状）允许重排」，则该块可算声明式改写；但工具与规则把「注释」和「代码」分开，且该改写没有被注记覆盖。

## FINDING FACT-07（P3）

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` 第 848 行（§3.2「四种去向」）与第 1237 行（§4.1「三个符号」）。
- 严重度：P3
- 问题：两处数量词与紧随其后的清单不一致。第 848 行说「同一份解析结果、四种去向」，但第 841–847 行的表有五行（`paddr`、`vaddr`、`offset` 与 `filesz`、`memsz`、`flags`）。第 1237 行说「`.bss` 里的三个符号各有用处」，但同句列出 `__bss_start`/`__bss_end`/`kernel_boot_stack_bottom`/`kernel_boot_stack_top` 四个；第 1239 行自己也写「上面这四个符号」。
- 影响：读者按「四种去向」检查字段时会漏掉 `flags`；按「三个符号」核对 `.bss` 时会以为有一个符号是多余或不属于 `.bss`。这类计数错误会直接影响「文档-代码同步」的可核对性。
- 证据或推理（E1）：
  - 第 841–847 行表格共 5 个字段行；第 848 行「四种去向」。
  - 第 1237 行同句列 4 个符号名；第 1239 行「上面这四个符号」。
  - `readelf -SW os/target/x86_64-unknown-none/release/kernel` 中 `__bss_start`/`__bss_end`/`kernel_boot_stack_bottom`/`kernel_boot_stack_top` 均可解析（`nm` 亦同）。
- 最小修复：第 848 行改为「五种字段去向」或「四条字段行加 `flags` 行」；第 1237 行改为「四个符号」。
- 置信度：高
- 是否需源码核实：否（文本内部对账）
- 若我错了：若「四种去向」指「内核装载 / 用户 ELF 装载 / 布局 / 权限」四种消费者而非表格行，则 848 行可辩护；但同句紧接着说「差异全部来自'这一侧是谁在摆放'」，与五行的字段去向更贴近，且 1237 行无同类辩护空间。

## FINDING FACT-08（P3）

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` 第 2029 行（§6.2 第一组命令的注释）。
- 严重度：P3
- 问题：注释说当前工件的 4 个 `LOAD` 行「依次是 .text（RX）、.rodata（R，含 .lrodata 与 .eh_frame_hdr）、.data（RW）、.bss（RW、NOBITS）」。实测：x86_64 与 aarch64 工件的 `.rodata` 段含 `.eh_frame_hdr`，但三份工件都没有 `.lrodata` 节；riscv64 工件连 `.eh_frame_hdr` 都没有。三份工件的 `LOAD` 行数确为 4。
- 影响：读者按注释去 `.rodata` 段里找 `.lrodata` 会找不到，可能误判链接脚本通配或工件损坏；在 riscv64 上找 `.eh_frame_hdr` 同样空手。这是诊断章「按名对段」的可复现性缺陷。
- 证据或推理（E1）：
  - `readelf -lW os/target/x86_64-unknown-none/release/kernel` → 4 个 LOAD；`readelf -SW … | grep -E 'lrodata|eh_frame'` → 仅 `.eh_frame_hdr`，无 `.lrodata`。
  - `aarch64-unknown-none` 同（有 `.eh_frame_hdr`，无 `.lrodata`）；`riscv64gc-unknown-none-elf` 两者都没有。
  - `readelf -lW … | sed -n '/Section to Segment/,$p'` 的映射与「.text/.rodata/.data/.bss」四段名一致，仅括注的节名不成立。
- 最小修复：把括注改成「.rodata（R；x86_64/aarch64 当前含 .eh_frame_hdr；.lrodata 通配只为将来出现时兜底，当前三份工件均无该节）」，或按架构分别注明。
- 置信度：高
- 是否需源码核实：是（已核实三份工件）
- 若我错了：若作者说的是「链接脚本的 .rodata 通配能收 .lrodata」，那是对的；但原句是「当前工件 … 含 .lrodata」，与工件实测不符。

## FINDING FACT-09（P3）

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` 第 1687 行（§4.5「两个自引导面收成薄壳」句）。
- 严重度：P3
- 问题：文档说 `bootface.rs` 与 `bootface_a64.rs` 「只保留本架构常量、入口横幅与各自的 console 实现」，并说「bump 取页 … 全部走 `bootface_common`」。实际 `bootface_a64.rs` 还持有 `static BUMP_NEXT: AtomicU64`、`bump_alloc_pages`、`BUMP_BASE`/`BUMP_SIZE` 与 `BOOT_HART_ELECTED` 等入口/分配器状态；`run_self_boot` 只是通过回调调用它，分配器本身不在 `bootface_common`。
- 影响：读者按「薄壳」清单检查共享面时会漏掉 aarch64 独有的 bump 分配器和入口选举状态；把「取页」全部归到 common，也会在排查 `BUMP_NEXT` 初始值/`.data` 落点时找错文件（第 1388 行正是用 `BUMP_NEXT` 说明 `.data` 静态）。
- 证据或推理（E1）：
  - `os/kernel-image/src/bootface_a64.rs:105`：`static BUMP_NEXT: AtomicU64 = AtomicU64::new(BUMP_BASE);`；同文件 117–125 行 `fn bump_alloc_pages`；89–96 行 `BUMP_BASE`/`BUMP_SIZE`。
  - `os/kernel-image/src/bootface_common.rs:341-349`：`run_self_boot(..., mut bump_alloc_pages: B, ...)` 以参数接收分配器。
  - `os/kernel-image/src/bootface.rs:78-80` 则从 `boot_shim::opensbi_helpers` 导入 `bump_alloc_pages`/`bump_end_bound`/`bump_pool_base`，与 aarch64 的自有分配器不同。
- 最小修复：把「只保留本架构常量、入口横幅与各自的 console 实现」改成「本架构常量、入口横幅、console 实现与各自的 bump 分配器回调」；把「bump 取页 … 全部走 `bootface_common`」改成「bump 页的取用流程在 `bootface_common`，分配器回调由各面提供」。
- 置信度：高
- 是否需源码核实：是（已核实）
- 若我错了：若「本架构常量」被作者用来涵盖 `BUMP_BASE`/`BUMP_SIZE`/`BUMP_NEXT`，且「bump 取页」指调用流程而非分配器实现，则这条只是措辞宽泛；但「只保留」清单明确排除了分配器函数，按字面仍不成立。

## SUGGESTION

- `rewrite-notes/meta/design/01-stage-kernel/02-design.*` 是否已有 `[ARCH: boot 最小恒等窗口；跨镜像腿不再铺低 4 GiB]`，超出本面板允许读取范围；建议下一轮由允许读取设计源的 panel 复核 FACT-02 的设计侧一端，避免把「设计已有、仅代码缺」误判成三处全缺。
- 第 1869 行「与本节直接相关的有 28 条」的分类口径未在文档中定义（32 减去哪 4 条）；本次无法从源码机械判定 28 是否正确。若作者希望这条可复核，建议补一句排除项（本次观察到 3 个 `MockLoader`/路径测试与 1 个模块缺失 panic 测试可能是被排除项），或删除具体数字改用「多数」。
- 第 1632 行「即『PC 仍留在低地址』」与前半句「只有跳转目标被抬到高半」在字面上相反；建议改为「跳转前的取指 PC 仍在低地址；跳转目标才是高半 VMA」，避免读者把「去掉 `add` 后的故障形态」与「正常形态」混读。

## 阴性声明

以下检查均给出「扫了什么、发现数、N=0 不覆盖什么」：

1. **锚点解析**：扫全篇 65 个 `path:symbol` 锚点，65/65 解析到唯一定义，零定义 0、多定义 0。N=0 不覆盖「锚点指向的定义是否就是文档想说的那个符号」——该层由人工逐条核对（重点核了 `pg_mapkernel`、`pg_identity`、`HugePages`、`compute_kernel_layout`、`load_segments_into_*`、`build_bootstrap_root_and_enable`、`arch_boot_finish`、`resume_additive`、`store_kernel_info`、`kmain_verify`、`init_protection`）。
2. **路径存在性**：从文档抽取 82 个唯一 `os/…`/`minix3/…` 路径，逐个 `-e` 检查，缺失 0。N=0 不覆盖 `{a,b}` 展开后的每个组合、通配路径和 `tmp/` 运行时路径。
3. **文档-代码同步七项**：静态可变（`IMAGE_HEAP`、`BUMP_NEXT`、`KERNEL_ENTRY_ANCHOR`、`BOOT_MODULES` 等）与路径、常量、内部 TODO/临时打印、符号锚点、工作区前缀、测试计数逐项核对；发现并记录 FACT-01（符号/签名）、FACT-04（临时打印）、FACT-07（计数）、FACT-08（工件节清单）。N=0 不覆盖未在文档出现的静态项，也不覆盖 `os/target/` 下的构建残留。
4. **测试名与数量**：`minix-elf` 27 个测试名逐个 `grep`，27/27 存在且 `cargo test -p minix-elf` 27 passed；`boot-shim` `loader.rs` 32 个测试名逐个核对，32/32 存在，`cargo test -p boot-shim --lib --list` 共 136 tests、`loader::tests::` 32 个；`check-layout.sh all` 三架构 PASS。N=0 不覆盖「28 条直接相关」这一分类口径（见 SUGGESTION）。
5. **常量与签名**：核对 `_kern_*`、`I386_*`、`KERNEL_*BASE`、`KERN_SIZE`、`BOOT_PT_BUMP_PAGES`、`PT_BUMP_PAGES`、`MAX_BOOT_WINDOWS`、`AP_STARTUP_PA`、`BOOT_HANDOFF_V1_VERSION=10`、`KERNEL_ENTRY_ANCHOR` 签名、`kern_stack_top` 访问器、`HigherHalf::jump_to_kmain` 签名、三份 `jump_to_kmain` 实现、`kmain_verify` 三/四条判定；发现 FACT-01。N=0 不覆盖未在本篇出现的常量。
6. **引文块**：`doc-citation-check` 的 `failed=0` 只覆盖 27 个真正逐行对账的块；15 个无注记块被 SKIP、5 个含「改写」的块只做 structure-only。人工核对了这 5 个 WARN 块：754–776（结构体字段，注释改写已声明）、1307–1353（`compute_kernel_layout`，注释压缩已声明）、1470–1493（`enable`，单行重排已声明）、1913–1925（`kmain_verify`，注释改写已声明）通过；792–833（`next`）代码本身被改写，记 FACT-06。C 侧引文（kernel.lds、Makefile.inc、head.S、mpx.S、pre_init.c、pg_utils.c、vm.h）逐块与源码比对，N=0 失败。
7. **覆盖率缺口**：逐项核对文档声称覆盖的 C 函数（`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`/`vm_enable_paging`/`pre_init`/`tss_init`/`add_memmap`/`util_stacktrace`/`proc_stacktrace_execute`）与 Rust 实现（`parse_ehdr`/`parse_phdr`/`next`/`compute_kernel_layout`/`load_segments_into_*`/`build_bootstrap_root_and_enable`/`plan_kernel_mapping`/`enable`/三份 `jump_to_kmain`/`arch_boot_*`/`resume_*`/`run_self_boot`/`place_modules`/`store_kernel_info`/`store_boot_report`/`kmain_verify`/`init_protection`），未发现「文档声称覆盖而完全没有对应说明」的函数。N=0 不覆盖本篇范围之外（如 `pg_map` 函数体、VM `pt_init` 全流程）的说明义务。
8. **反事实前提**：核对 §2.1/§2.2/§2.3/§2.4/§2.5、§3.1–§3.5、§4.1–§4.6、§5.1–§5.3 的「如果反过来想/会怎样」共 20 余处，前提均真实存在（AT 五处、`pg_utils.o` 在名单、`pg_identity` 全铺、`mpx.o` 不在名单、三份脚本的 `ALIGN(0x200000)`/`ASSERT`、`kern_virt == kern_phys` 测试载体、riscv64 direct leg 等）；未发现以不存在的代码/常量当前提。N=0 不覆盖反事实推演本身的因果正确性（例如 GNU ld 的历史版本行为、QEMU 对未映射物理写的具体表现），这些只有外部工具/真机证据。
9. **阴性声明**：文档「C 侧次序无自动断言」「没有映射先于进入的独立断言」「宿主测试不证明按 `p_paddr` 写物理地址」等阴性陈述与源码/测试一致（`check-layout.sh` 只做静态布局；`test-higher-half` 只到 `kmain`；`load_segments_into_phys_memory` 无宿主执行路径）。N=0 不证明仓外 CI、历史日志或真机链上没有别处的断言。
