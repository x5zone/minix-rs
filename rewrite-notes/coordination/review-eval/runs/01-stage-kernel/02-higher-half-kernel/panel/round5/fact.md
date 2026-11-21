# fact 面板（独立事实审查，只读）

目标：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`（1846 行）

## 机械门读数（本次实跑）

- `python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md --root .`
  → `blocks=42 checked=28 failed=0 skipped=9 warned=5`。5 个 WARN 全部是文档自声明改写的块
  （613-635、651-692、1131-1177、1294-1317、1691-1703）；手工比对其中 1131-1177 与源码
  `os/boot-shim/src/loader.rs:249-300` 函数体逐行一致，仅注释被改写。
- `bash tools/anchor-resolve.sh --check <doc>` → `anchors=66 resolved=66 zero-def=0 multi-def=0`。
- `python3 tools/doc-structure-check.py <doc>` → `✅ 1 个文件表结构与内部 .md 链接均可解析`。
- `bash tools/doc-ref-check.sh <doc>` → `✅ 内部章节引用均可解析`。
- `bash tools/doc-style-lint.sh <doc>` → `error级=0`。
- `bash tools/doc-code-density-check.sh --check <doc>` → `✅ 承载门阻断项通过（4 个可判章均有码/命令）`（读数：2 章码 10、3 章码 10、4 章码 15、6 章命令 1）。
- `bash tools/doc-code-map.sh <doc>` → 30 个关联文件，存在性 30/30。

机械门全绿。以下发现来自机械门判不了的部分：语义核对、阴性声明、反事实、占位符、逐字引文。

## FINDING FACT-01（P1）

- 位置：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` §4.5 状态表（1534、1535 行）、§5.4 反转核对（1715 行）、§5.4 表 M8/M13（1726、1731 行）
- 严重度：P1
- 问题：具体阶段名与条目号被同一个占位词「阶段」替换，句子读不通且无法追溯原指。
- 影响：§4.5 读者不知道 `kmain` 早期阶段叫什么、`init_protection` 属于哪一段；§5.4 那句话说"3 条覆盖缺口（R02-阶段/阶段/阶段）… 阶段 由 L10、阶段 由 riscv64 的 PC 高半判据、阶段 由 ABI 栈形断言"，三条互不相同的缺口塌成同一个词，"三条均已闭合"因此不可核；1726/1731 的 `R02-阶段` 不成词。
- 证据或推理：E2 手工 grep：全文「阶段」22 次 / 14 行，其中 12 次落在 5 行上是占位符——1534 行 2 次（"（阶段，完整划分见…）"、"是 阶段 的第一条安装动作"）、1535 行 1 次（"`kmain` 的 阶段 里"）、1715 行 7 次（"（R02-阶段/阶段/阶段）" 加三处"阶段 由…"加"R02-阶段 的 arch link.ld"）、1726 行 1 次、1731 行 1 次；另 10 次是正常用词（boot 阶段、引导阶段、additive 阶段、规划阶段等）。E3 原值可部分还原：`os/kernel-image/check-layout.sh:47` 写 `L10（R02-T01）`；`os/kernel/src/arch/x86_64/higher_half.rs:30`、`.../aarch64/higher_half.rs:30`、`.../riscv64/higher_half.rs:33` 都写 `R02-T04`——说明本应是 `R02-T01/T03/T04` 这一类条目号。`R02-T03` 在 `os/` 树内没有任何对照文字（`rg 'R02-T0[0-9]'` 只命中 T01 与 T04），这一处必须从评审台账重新取，不能凭猜补。
- 最小修复：12 处「阶段」逐一换成真实阶段名/条目号；T03 那一处若查不到编号，就把判据本身写全（"riscv64 的 `kmain_verify` 把 `pc_at_high` 并入 PASS 条件"），不要留空标签。
- 置信度：高（占位符事实）/ 中（T03 的原始编号）
- 是否需源码核实：是（仅 T03 编号）
- 若我错了：若「阶段」是项目约定的脱敏标记，应在文首或文末给出标记释义；当前文档没有任何说明，按不可读处理。

## FINDING FACT-02（P2）

- 位置：全文 15 处 `设计裁决`、9 处 `评审发现`，例如 550、599、649、694、696、715、734、1179、1208、1519、1715、1723 行
- 严重度：P2
- 问题：具体裁决号/发现号被类别词顶替，事实断言失去可审计锚点。
- 影响：同类断言在别处都带编号（D2-01、D-084、D-089、D-062、D-094、D-107），这 24 处却只写"设计裁决/评审发现"，读者无法判断是哪一条、是否仍然有效；项目锚点纪律要求事实断言带可解析锚点，这 24 处不满足，且与同篇保留编号的写法自相矛盾。
- 证据或推理：E2 计数 `设计裁决`=15、`评审发现`=9（`grep -o … | wc -l`）。E3 可还原的几处：715 行"v10（设计裁决）"对应 `os/kernel/src/lib.rs:856` 的注释 `Since wire v10 (D-062 / I-31)`；649/694/696/1208 行的段形错误守卫对应 `os/libs/minix-elf/src/lib.rs` 的 `I-48 / D-084`（迭代器内守卫）与 `I-47 / D-089`（缓冲区两循环）；550 行"已按 `设计裁决` 删除 arch link.ld"对应 `rewrite-notes/coordination/review-eval/DECISIONS.md` D-107 ①（"删除三份无生产读者的 arch `link.ld` 与 `link_ld_lockstep.rs`"）。
- 最小修复：每处换成实际编号（或删除括号）；550 行写 D-107 ①，715 行写 v10 / D-062，649/694/696/1208 行写 I-48/D-084 与 I-47/D-089。
- 置信度：高
- 是否需源码核实：否（编号取自源码注释与 DECISIONS 原文）
- 若我错了：若这些是有意的脱敏（例如为公开版抹掉内部编号），同样需要在文首声明；但同篇大量保留 D2-01/D-084/D-089/D-107，脱敏口径不一致。

## FINDING FACT-03（P1）

- 位置：§4.3 第 1290 行（Step 2c 段末尾）
- 严重度：P1
- 问题：文档称 x86_64 的 `kernel_identity_mmio_regions()` "本身返回 IO-APIC（`0xFEC0_0000`）与 local APIC（`0xFEE0_0000`）两条固定页"，实际该函数在 x86_64 上返回空表。
- 影响：这是对现有源码符号返回值的直接错误陈述。读者若据此在别处寻找/恢复这两条"固定页"，会与 Step 2c 的唯一安装点重复；同节 1280、1284 行已正确写着"x86 上 `identity_device_windows()` 返回空、LAPIC/IO-APIC 只由 Step 2c 安装"，1290 行与它们自相矛盾。
- 证据或推理：E1 `sed -n '255,320p' os/arch/src/lib.rs`：`#[cfg(target_arch = "x86_64")] pub const fn kernel_identity_mmio_regions() -> &'static [(u64, u64)] { &[] }`，函数文档明写 `x86-64: empty. … a low-VA table here had no production reader.`；两页的真实归属在 `os/arch/src/arch/device_window.rs:141-146`（`0x0FEC0_0000` slot0、`0x0FEE0_0000` slot1，`bootstrap: true`）。E2 `rg 'kernel_identity_mmio_regions' os` 只有三份 cfg 定义、`os/kernel/src/lib.rs:1100`（riscv64 分支）与 `os/kernel/src/vm_handoff.rs:1236`（测试）两个读取点。
- 最小修复：改为"x86_64 的同名函数返回空表（函数文档写明原来的低 VA 表没有生产读者）；LAPIC/IO-APIC 只在 `device_window_regions()` 里，由 Step 2c 安装"。
- 置信度：高
- 是否需源码核实：是（已核）
- 若我错了：除非存在另一份同名实现返回两页；全树只有 `os/arch/src/lib.rs` 的三份 cfg 版本（x86_64/aarch64 空、riscv64 两窗口）。

## FINDING FACT-04（P1）

- 位置：§4.3 第 1327–1331 行（"若恒等映射只覆盖可用内存、或虚拟基址等于物理基址时照做第二步"的"其二"）
- 严重度：P1
- 问题：反事实把"`kern_virt == kern_phys` 时照做 Step 2"的失败方式写成"1 GiB 叶覆盖 Step 1 的 2 MiB 叶、只改缓存属性"，以及"只有回到 2 MiB 才是同尺寸覆盖"；实际两种尺寸都被 `AlreadyMapped` 拒绝，Step 2 的 `.expect` 当场上 panic。
- 影响：这一段是解释"为什么需要 `if kern_virt != kern_phys` 守卫"的唯一推演。按文档，删守卫的后果是"镜像不丢、只改属性"；按代码，删守卫的后果是启动期 panic（`kernel map: map_huge failed — kernel image mismatch`）。读者会按错误方向排查（查 PWT/PCD、查窗口边界），而不是查 already-mapped。对当前 riscv64 载体（VMA=LMA、`kern_huge`=2 MiB）这个反事实就落在真实配置上。
- 证据或推理：E1 三份实现的守卫：`os/arch/src/x86_64/paging.rs:1290-1310`（1 GiB 支：`e3` PRESENT → `AlreadyMapped`；2 MiB 支：`e2` PRESENT → `AlreadyMapped`，不比较 PA 是否相同）；`os/arch/src/riscv64/paging.rs:1218-1250`（`present2` / `l1e & V` → 同）；`os/arch/src/arm64/paging.rs:1218-1240`（`e1 VALID` → 同）。E1 Step 2 的调用形态：`os/kernel/src/lib.rs:1415-1429` `paging.map_huge(...).expect("kernel map: map_huge failed — kernel image mismatch")`。E1 Step 1 必然先覆盖同一跨度：`push_common_boot_windows` 推内核 LMA 窗口（`lib.rs:1000-1005`，跨镜像与直核两条入口分别在 `lib.rs:461-486`、`998-1012` 调用），Step 1 以 2 MiB piece 铺满（`lib.rs:1360-1408`）。
- 最小修复：重写成"删守卫后，1 GiB 与 2 MiB 两档都在 Step 2 第一条 `map_huge` 返回 `AlreadyMapped`，`.expect` 当场 panic（fail-closed，现场可定位）"。若仍想保留"属性被覆盖"的推演，必须显式改成更强的反事实（"连 `map_huge` 的 `AlreadyMapped` 守卫一起去掉"），并说明那是第二处改动。
- 置信度：高
- 是否需源码核实：是（已核）
- 若我错了：需要证明某个架构的 `map_huge` 对"同 VA、同 PA 的 2 MiB 重写"返回 `Ok`；三份实现都在写叶之前检查既有叶，未发现该分支。

## FINDING FACT-05（P2）

- 位置：§6.1 诊断注记（第 1751 行）
- 严重度：P2
- 问题：被点名的第一块临时打印字符串 `kernel: arch_boot arg=…` 在树内不存在；且"两块…源码注释自标 `temporary`"只对第二块成立。
- 影响：诊断注记的作用是让读者复现串口日志时按字符串过滤噪声。按本文找 `arch_boot arg=` 会一无所获；真实的第一个路标是 `kernel: arch_boot entered`（其注释是 "Kernel-side first marker"，没有 temporary 字样），只有 `kernel: preA rdi=` 那块注释写 `(temporary, v3)`。
- 证据或推理：E1 `rg -n 'arch_boot arg|preA rdi' os` 只命中 `os/kernel/src/lib.rs:1845`（`kernel: preA rdi=`）；`sed -n '250,262p' os/kernel/src/lib.rs` 给出 `Console::write_str("kernel: arch_boot entered\n")` 与其注释 "Kernel-side first marker"；`sed -n '1820,1830p'` 给出 "Evidence capture (temporary, v3)"。
- 最小修复：写成 `kernel: arch_boot entered` 与 `kernel: preA rdi=`，并把 temporary 归属限定到后者。
- 置信度：高
- 是否需源码核实：是（已核）
- 若我错了：除非 `arch_boot arg=` 在别的 crate；`rg` 覆盖全 `os/`，无命中。

## FINDING FACT-06（P2）

- 位置：第 901/903 行（§3.6）、第 966/968 行（§3.8）
- 严重度：P2
- 问题：两个章节标题各出现两次、逐字相同，中间夹一个空行。
- 影响：一篇文档里出现两个同编号同名标题（3.6 与 3.8 各一对），阅读时像丢了一节；对 §3.6/§3.8 的引用在渲染锚点上会重复。`doc-structure-check` 只查表格与外部 md 链接、`doc-ref-check` 只查能否解析，两道工具都不查重名，所以机械门仍绿。
- 证据或推理：E2 `grep -n '^#\+ ' <doc>` 输出中 `901:### 3.6 自举根与映射建立的设计（J3）` 与 `903:### 3.6 …` 逐字相同，`966:### 3.8 接收侧两条路径（J4）` 与 `968:### 3.8 …` 同。
- 最小修复：各删一行（保留带内容的那一份，删掉前面重复的那一行）。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若这是刻意的"分隔线式重复标题"约定，全仓其它篇应同形；抽查同目录邻篇未见此形。

## FINDING FACT-07（P3）

- 位置：§5.3 第 1707 行首句
- 严重度：P3
- 问题：加粗断言"PC 不在成败判定里"对 riscv64 不成立，且同一段后文与 §5.4 M8 都写着 riscv64 把 `pc_at_high` 并入 PASS。
- 影响：读者只看首句会以为 PC 永不参与判定，与 `test-higher-half` 在 riscv64 的 `FAIL: PC not at high address (riscv64 delta path broken)` 判据相反。
- 证据或推理：E1 `os/kernel/src/lib.rs:2330-2336`：`#[cfg(target_arch = "riscv64")] let pc_ok = pc_at_high;` 其余架构 `let pc_ok = true;`；FAIL/INFO 两条消息在 2344-2350 行。文档同段 1707 行后半句与 §5.4 M8 均正确。
- 最小修复：首句改成"PC 不进入 x86_64/aarch64 的成败判定（riscv64 相反，见下）"。
- 置信度：高
- 是否需源码核实：是（已核）
- 若我错了：若"成败判定"被限定指"全架构统一的判定"，那也应写明限定语；当前字面读法与本段其余文字冲突。

## FINDING FACT-08（P3）

- 位置：§2.2 第三张表 `head.o` 行（第 147 行）
- 严重度：P3
- 问题：该行把 `head.o` 的落点写成 `.unpaged_text`/`.unpaged_data`/`.unpaged_bss` 三段，而 `head.S` 只有 `.text`（:32）与 `.data`（:93）两个段，没有 `.bss` 供料。
- 影响：读者会以为 `.unpaged_bss` 有 `head.o` 的内容；同一张表对 `pre_init.o` 只写一段（`.unpaged_text`），说明这张表本意是按对象实际段逐项列，`head.o` 行因此不一致。
- 证据或推理：E2 `grep -n '^\.text\|^\.data\|^\.bss' minix3/minix/kernel/arch/i386/head.S` → 仅 `32:.text` 与 `93:.data`；`load_stack` 在 `.data` 的 `.space 4096`。`.unpaged_bss` 的收段规则（`minix3/minix/kernel/arch/i386/kernel.lds:19`）只收 `unpaged_*.o(.bss COMMON)`，`head.o` 无 bss 可收。
- 最小修复：该行改为 `.unpaged_text`/`.unpaged_data`。
- 置信度：中（"head.o 无 .bss"依赖汇编器不产生 .bss；要绝对确定需看工件）
- 是否需源码核实：是（可选，看链接产物）
- 若我错了：若 `head.o` 因 `COMMON` 或汇编器产物带出非空 `.bss`，该行成立；`objdump -h unpaged_head.o` 一次可判死。

## FINDING FACT-09（P3）

- 位置：§4.3 第 1278 行
- 严重度：P3
- 问题：括号里写"见下文'窗口建表失败怎么处理'一条"，全文没有这个标题的条目。
- 影响：内部交叉引用悬空；读者按引号里的名字找不到落点。实际承接它的条目是 1282 行"Step 1 的 `map_huge(...)` 用 `match` 区分两类结果"（其下 1283 行另有加粗小标题"重叠窗口的真实语义"）。
- 证据或推理：E2 `grep -n '窗口建表失败' <doc>` 只命中第 1278 行自身；同节条目枚举（`grep -n '^- \|^  \*\*'`）无此名。
- 最小修复：改成实际条目名（"见下条 `map_huge` 的两类结果"）或删掉括号。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若该条目被改名后仍打算保留旧引用，应恢复条目名。

## FINDING FACT-10（P3）

- 位置：§4.5 生产路径 B 第 1480 行
- 严重度：P3
- 问题：把"策略"记在 `arch_boot_finish` 名下（"arch_boot_finish（校验/策略 → arch_boot_impl Step1 …）"），实际 `plan_boot_pages` 在 `arch_boot_impl` 内调用；`arch_boot_finish` 只做校验与落地。
- 影响：与同篇 §3.5 第 862 行"`arch_boot_impl` 调用 `plan_boot_pages`"冲突，两处对"策略在哪一步"给出不同答案；按路径图找代码的人会先怀疑 finish。
- 证据或推理：E1 `os/kernel/src/lib.rs:214-232`：finish 体为 `validate_before_tables` → `check_boot_postconditions` → `store_kernel_info` → `register_panic_diagnostic` → `arch_boot_impl` → `jump_to_kmain`；`plan_boot_pages` 调用点在 `lib.rs:992`（impl 内，Step 0 之后），resume 面在 `lib.rs:453`。
- 最小修复：路径行改为"arch_boot_finish（校验 → arch_boot_impl（策略/Step1…））"。
- 置信度：高
- 是否需源码核实：是（已核）
- 若我错了：若 finish 另有一处策略调用；`rg 'plan_boot_pages' os/kernel/src/lib.rs` 只有 453 与 992 两处，均不在 finish（214-232）。

## 已核对且未发现问题的部分（供合并时排除重复劳动）

- 逐字引文：`kernel.lds` 三个常量/五处 `AT()`/`.unpaged_bss → . += _kern_offset → .bss` 次序；`head.S` 四行交接与 `hang: jmp hang`、`load_stack` 4096；`mpx.S` 栈定义与 `LABEL(__k_unpaged_k_initial_stktop)`；`pg_utils.c` 三个静态量与 `pg_mapkernel`；`pre_init.c` 八条语句；`vm.h` 位/掩码/CR0-CR4 值（含文档指出的 `I386_CR0_WP` 注释误抄 "Enable paging"——属实）。
- 数量复算：`Makefile.inc` 9+3+13+2=27 个改名对象（三条复算命令读数 13/3/2 与文档一致）；`MINC_OBJS_UNPAGED` 13 个；`usermapped_data_arch.c` 3 个 + `usermapped_data.c` 8 个；`printf` 段的 961/0xf0000000。
- Rust 侧：minix-elf 27 个测试与文档四族名单逐名对上；`loader.rs` 32 个（文档列名的 22 条全部存在，含六个 `first_overlapping_pair_*`）；`boot-shim --lib` 默认特性 136 = 147 − 11（`opensbi_helpers` 受 `#[cfg(feature = "opensbi")]` 门控，default = `["uefi"]`）；`ElfError` 12 变体与四组划分；`Paging` 24 成员（`const PAGE_SIZE` + 23 方法，名单与顺序一致）；`Elf64Ehdr/Phdr/LoadSegment` 字段与 phdr 偏移表；`SegmentIter::new`/`parse_phdr` 的未检查加法（D2-01）；`compute_kernel_layout`/`load_segments_into_phys_memory`/`load_segments_into_buffer`/`segment_spans`/`first_overlapping_pair`/`first_source_destination_overlap` 摘录与源码一致；`test_bss_region_is_zeroed_after_load` 的 0xAB/0xCC 判据与哨兵字节；`test_compute_kernel_layout_from_mock_elf` 的注释原因；`MockPaging::map_huge` 4 KiB 展开与任一页已映射即 `AlreadyMapped`；`boot_policy` 阶梯与两条错误消息（`no kernel page size satisfies the layout`、`fallback page size … is not supported`）与 `arch_boot: page-size policy:` 包装；`HigherHalf` trait 三条 Safety 前提；三份 asm（x86_64 `mov/and/xor/push/sub/call`、aarch64 `mov sp/x1-x2 and/x29/isb/ldr/br`、riscv64 `auipc/bltu/delta/jalr`）；三份 `jump_to_kmain` 的 16 字节断言与 x86_64 的 `(stack_top−24) & 0xF == 8`；`kmain_verify` 四条判据与 FAIL/INFO 字符串；riscv64 注释里的 `0xffffffbf7ffe0000` 错位与文档的更正（复算 `0xffffffc00000477a + 0xffffffbf7fe00000 = 0xffffff7f7fe0477a`，与注释记录的故障地址一致）；三份 `os/kernel-image/*.ld`（x86_64 4 个 `AT`、aarch64/riscv64 各 4 个 `: AT(`、放宽到 `AT(` 时 4/5/5；每条两条 `ASSERT`、`__image_end`、`kernel_boot_stack_*`、`.bss` 内 `ALIGN(0x200000)`、x86_64 无 `OUTPUT_ARCH`、riscv64 多 `.riscv.attributes`）；`check-layout.sh` L0–L10 判据与 `  [Lx] PASS  ` 两空格格式；`aarch64.ld` 的 `0x40200000`（使 L4 平移量 `0xffff7fffbfe00000` 成立）；`KERN_SIZE = 0x40_0000`；`BOOT_PT_BUMP_PAGES = 1024` / `PT_BUMP_PAGES = 256`；`MAX_BOOT_WINDOWS = 180` 与 `BOOT_WINDOW_FLOOR` 编译期断言；`AP_STARTUP_PA = 0x5000` 与 `windows.push(0, DEV_PIECE)` 两处；`device_window_regions()` 三份表（aarch64 空、riscv64 PLIC/UART/SSWI 且 SSWI `bootstrap: false`、x86 LAPIC/IO-APIC `bootstrap: true`）；`device_window_flags()` 返回 `kernel_read_write() | DEVICE` 且 `pte_to_flags` 无法还原 `DEVICE`；`identity_device_windows()` 三份；`platform_desc` 的 16 B wire 与信封版本 10；`KernelInfo` 十个字段（无 `kern_stack_top`/`user_sp`/`syscall_entry`）；`kern_stack_top()` 访问器与 16 字节断言；`set_current_root_phys` 在 `build_bootstrap_root_and_enable` 内；`new_from_page` 的 `write_bytes(ptr, 0, 512)`；`enable()` 的 NXE→CR3→CR0.PG/WP→CR4.PGE 次序；`enter_kernel_image_v1` 的三条跳转（`br {entry}`/`jr {entry}`/x86 `__minix_x86_enter_image` stub `mov rbx,rsi; mov rax,rdi; cli; cld; jmp rax`）；`_start` 四条 global_asm 与 x86 次序（`cli`→`cld`→清 `.bss` 只碰 rdi/rcx→`lea rsp`→`mov rdi,rbx`→`call`）；`rust_image_main` 三份 cfg 臂；`place_modules` 的只清尾部与名字池；`run_self_boot` 的 `DtbMemoryMap::new(&fdt, fw_end)`（riscv `KERN_PHYS_BASE`、aarch64 0）；`run_qemu.sh` 的 `-serial file:`、`-d int,cpu_reset -D /tmp/qemu_int.log`、PASS/SKIP 判定与退出码；`test-higher-half` 合成 KernelInfo 的三个值与 PC 判定；`minix-elf` 五个依赖 crate；`exec_worker.rs` 的 `ElfHead`；`vm_server.rs` 的 `entry_point`/`segment_iter`；`trap_api.rs` 的 exec 路径；`build.rs` 的 `-T` 选择；`os/.cargo/config.toml` 的 `relocation-model=static` 与 `x86_64-unknown-uefi` 的 `-sse,-sse2,-mmx`。
- 阴性声明核对：`pre_init.c` 不在任何 `SRCS`（`#define UNPAGED 1` 的存在也解释了 `proto.h` 的 `#ifndef UNPAGED` 宏为何在本文件不生效）；`mpx.o` 只在 `SRCS+=`、不在 `.for` 名单；`os/kernel/src` 无 `minix_elf`/`PT_LOAD`（"内核侧一行解析代码也没有"成立）；`os/kernel/src/arch/*/link.ld` 与 `link_ld_lockstep.rs` 在树内已不存在（与 D-107 ① 决定一致，尽管 D-106 的状态列仍写"未实施（已批准延期）"——这是台账列与树的状态差，不是文档错误）。
