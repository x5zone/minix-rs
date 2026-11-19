# D04 R1 P-FACT（C+Rust 合并）事实核对

- 受检文档：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`
- 基线：2688 行；sha256 前缀 `a8cb91b1`（工作树当前内容）。工作树相对 HEAD 有一处未提交改动：§5.3 删除 `boot_form.rs` 行并把"53 处"改成"52 处"；本报告按工作树内容核对（该改动未同步 §5.3/§6.2 的命令输出，见 RD4-FACT-04）。
- 核对日期：2026-11-12
- 材料隔离：未读取历史 scan / 其他面板输出 / expected/；冻结决策按 `round1-decisions-snippet.md` 只作 re-litigation 过滤。
- 结论统计：FINDING 8 条（P0×3、P1×2、P2×3）；SUGGESTION 7 条。P0 集中在 boot-shim 平台来源接口的文档漂移（函数名、`len` 字段/`BootStorage` 形态）与两处可机械复核的计数（`uefi_helpers` 测试数、`&[]` 条数）。

## 六门读数（任务 0）

下表 `<doc>` = `rewrite-notes/01-stage-kernel/04-platform-discovery.md`；所有命令在仓库根执行。

| 门 | 命令 | 读数 | 判定 |
|---|---|---|---|
| 锚点 | `tools/anchor-resolve.sh --check <doc>` | `anchors=72 resolved=71 zero-def=1 multi-def=0`；零定义：`os/boot-shim/src/uefi_helpers.rs:fn find_platform_sources`（文档 L2484） | FAIL（0 定义按门规则即 P0-fact） |
| 代码清单 | `tools/doc-code-map.sh <doc> --check` | 52 个文件，存在性 52/52 通过 | PASS |
| 结构 | `tools/doc-structure-check.py <doc>` | `1 个文件表结构与内部 .md 链接均可解析` | PASS |
| 引用 | `tools/doc-ref-check.sh <doc>` | `内部章节引用均可解析` | PASS |
| 文风 | `tools/doc-style-lint.sh <doc>` | `命中汇总：` + `error级=0` | PASS（只读 error 级；warning 未逐条纳入本面板） |
| 引文 | `tools/doc-citation-check.py <doc>` | `blocks=86 checked=0 failed=0 skipped=86` | 工具未实际校验任何块；本报告第 5 项改为手工逐字核对 |

---

## FINDING RD4-FACT-01（P0）

- 位置：`rewrite-notes/01-stage-kernel/04-platform-discovery.md` §1.4（L103）、§4.2 标题（L1550）、§4.2 代码块锚点（L1553）、§4.11 ② 表（L2332）、§5.2 末段（L2474）、§5.3 缺口表（L2484）、§5.3 判定准则（L2513）、§5.4 R04-T05（L2543）
- 严重度：P0
- 问题：文档通篇引用的函数名 `find_platform_sources` 在仓库中不存在；实际函数是 `find_platform_sources_into(out: &mut [PlatformDescSource]) -> usize`（`os/boot-shim/src/uefi_helpers.rs:208`，`os/boot-shim/src/lib.rs:250` 的 re-export 也是 `_into`）。
- 影响：§5.3 的"该函数没有单测"、§5.4 M1 与 R04-T05 都指向一个不存在的符号；读者按文档检索不到被测对象，TODO 无法落地；锚点门直接判 FAIL。
- 证据：`tools/anchor-resolve.sh --check` 输出 `os/boot-shim/src/uefi_helpers.rs:fn find_platform_sources → ZERO-DEF（符号不存在/已改名）→ 按 P0-fact 处理`；`grep -n "fn find_platform_sources" os/boot-shim/src/uefi_helpers.rs` 只命中 `find_platform_sources_into`（L208）；`rg -l 'find_platform_sources' os/` 的调用点全部写 `find_platform_sources_into`。证据强度 E1。
- 最小修复：把文档中所有 `find_platform_sources`（9 处、8 行；L2484 同一行出现两次）改成 `find_platform_sources_into`；§4.2 标题同步；重跑 `anchor-resolve.sh --check` 归零。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若仓库存在同名宏/别名 `find_platform_sources`（`rg 'find_platform_sources\b' os/ --type rust` 未发现），或文档刻意用一个概念名而非符号名——但代码块注释写的是 `os/boot-shim/src/uefi_helpers.rs:fn ...`，符号锚点语法要求真实函数名，概念名不成立。
- 判据：Blocker Gate 1.0 锚点解析（`review-process.md` L1250 起）"0 定义 → P0-fact"；Doc Code Sync 七项之"符号锚点"。

---

## FINDING RD4-FACT-02（P0）

- 位置：同一根因涉及 §3.1 KernelInfo 引文（L1056）、§3.6 `PlatformDescSource` 描述（L1359）、§4.2 两段 boot-shim 节选（L1553–L1585、L1598–L1608）、§4.11 ② 表（L2332）、§5.4 M7 表（L2527）
- 严重度：P0
- 问题：文档把 `PlatformDescSource` 描述成"一个 kind 标签加一个物理地址"，引文写作 `(u32 kind, u64 phys_addr)`，并声称 boot-shim 用 `Vec` + `Box::leak` 产出 `&'static [PlatformDescSource]`；现行源码已改成 `kind + len + phys_addr`（`#[repr(C)]`，`os/libs/minix-boot/src/platform.rs:125-130`），boot-shim 改为把来源写进调用方提供的 `BootStorage` 槽位（`uefi_helpers.rs:208` 的 `out: &mut [PlatformDescSource]` + `copy_from_slice`；`opensbi_helpers.rs:416-423` 的 `storage.push_platform_source(...)`），并用 `with_optional_len` 探测 DTB/RSDP 头部长度。`KernelInfo.platform_sources` 的源注释现在也写 `(u32 kind, u32 len, u64 phys_addr)`（`os/libs/minix-boot/src/kernel_info.rs:132`）。
- 影响：读者会得出"交接物没有长度字段、boot-shim 靠泄漏 Vec 拿 `'static`、内核按地址自行找 2 MiB 窗口"的旧模型；现行设计的核心新事实——每个来源携带 blob 长度（`dtb_total_size` / `rsdp_total_size`），窗口装配据此定界——在文档中完全缺失。§3.6 的"跨二进制纯数据"论证与 §4.2 的"Box::leak 可接受"段落都建立在不存在的代码上。
- 证据：`sed -n '125,130p' os/libs/minix-boot/src/platform.rs` 显示三字段结构；`sed -n '208,276p' os/boot-shim/src/uefi_helpers.rs` 显示 `RefCell<Vec>` 仅在函数内暂存后 `out[..sources.len()].copy_from_slice(&sources)`，无 `Box::leak`；同文件 L224/L241/L254 用 `with_optional_len`；`opensbi_helpers.rs:408` 注释明写 "no `Box::leak` and no `'static` slice (R18 D-1 stage 2)"；`kernel_info.rs:132` 注释为 `(u32 kind, u32 len, u64 phys_addr)`；`bootface_a64.rs:298-300` 同样用 `with_optional_len`。证据强度 E1。
- 最小修复：§3.1 引文改为现行 `(u32 kind, u32 len, u64 phys_addr)`（并保留 `len = 0` 的兼容语义）；§3.6 补第三个字段与"窗口按 blob 长度定界、0 回退 2 MiB"的语义；§4.2 两段节选取现行 `find_platform_sources_into` / `push_platform_source` 代码并按新流程改写说明；§4.11 ② 与 §5.4 M7 的 `PlatformDescSource::new` 改为 `with_optional_len`（或注明 `new` 是 len=0 的兼容构造）。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若 `BootStorage` 的 `platform_slots_mut` 只是把旧 `Box::leak` 包了一层，且 `len` 字段是文档交付后才加的而未进入基线——但基线工作树源码就是当前三字段结构，`git log -S'with_optional_len'` 显示该重构已在 `1c279cbd5`（R25）落地；文档的"逐字引用"标签要求跟随现行源码。
- 判据：Doc Code Sync 七项（静态可变/常量/符号/字段）；P0-fact 定义"文档写 `fn foo()` 但实际是 `fn baz()`"（`review.md` L644-648）；锚点纪律门（引文逐字）。

---

## FINDING RD4-FACT-03（P0）

- 位置：`rewrite-notes/01-stage-kernel/04-platform-discovery.md` §5.3 缺口表第一行（L2484）
- 严重度：P0
- 问题：文档称 `os/boot-shim/src/uefi_helpers.rs` 有 6 条 `#[test]`（1 条测 `KernelInfo` 字段、5 条测 bootstrap memmap 边界）；实际该文件有 12 条 `#[test]`：`test_build_kernel_info_fields` 1 条、`test_bootstrap_outside_memmap_*` 5 条、`test_reserve_kernel_span_rejects_unaligned_span` 1 条、`test_assert_range_clear_of_kernel_*` 4 条、`test_assert_modules_clear_of_kernel_*` 1 条。
- 影响：测试缺口论证的"现有测试覆盖面"基线错误；读者会低估 boot-shim 已有测试，且"6 条"与 `#[test]` 属性数不一致，属于可机械复核的计数事实。
- 证据：`grep -n '#\[test\]' os/boot-shim/src/uefi_helpers.rs` → 12 个属性（L1307/1352/1373/1386/1399/1411/1426/1440/1448/1454/1460/1470）；`grep -n 'fn test_'` 同名 12 个函数。证据强度 E1。
- 最小修复：把"6 条"改为"12 条"，并把括号内分解改为"1 条 `KernelInfo` 字段、5 条 bootstrap memmap、1 条 kernel span 对齐、4 条 `assert_range_clear_of_kernel`、1 条 `assert_modules_clear_of_kernel`"；再复核"均不覆盖 `find_platform_sources_into`"仍成立（实际无一条调用它，成立）。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若这些测试在 `#[cfg(feature = ...)]` 下默认不编译，文档按默认特性计数——12 条都在 `#[cfg(test)] mod tests` 内且无 feature 门（已用 `sed -n '1290,1310p'` 确认模块头），宿主 `cargo test -p boot-shim` 会编译它们。
- 判据：Doc Code Sync 七项之"测试计数"；任务 3（测试计数）。

---

## FINDING RD4-FACT-04（P1）

- 位置：`rewrite-notes/01-stage-kernel/04-platform-discovery.md` §5.3 命令块与分布表（L2488–L2509）、§6.2 #4（L2591–L2592）
- 严重度：P1
- 问题：`platform_sources: &[]` 的条数在三处互相矛盾且都与实测不符：§5.3 命令输出写 53，§5.3 分布表逐行相加为 52、正文写"上述 52 处"，§6.2 #4 命令输出又写 53；实测 `rg -o 'platform_sources:\s*&\[\]' os/ --type rust -g '*.rs' | wc -l` 为 54。差异来源之一：分布表把 `os/kernel/src/vm_handoff.rs` 记为 1，实际为该文件 3 处。
- 影响：口径段（"口径 = 条"）与计数互相拆台，读者无法判断哪个数字才是当前仓库状态；后续新增/删除夹具时无从对账。这也是文档自己给的可复制命令，输出必须可复现。
- 证据：命令实测输出 54；`rg -c 'platform_sources:\s*&\[\]' os/ --type rust -g '*.rs'` 逐文件：`dm_coverage.rs:10`、`arch/boot.rs:9`、`kernel/src/lib.rs:6`、`kernel/src/vm_handoff.rs:3`、21 个测试内核各 1、`kernel_info.rs:1`、`handoff_v1_write.rs:1`、`boot_integration.rs:1`、`kerninfo.rs:1`、`ebs_pair.rs:1`，合计 54。证据强度 E1。
- 最小修复：把 §5.3 与 §6.2 #4 的命令输出改为 54；分布表 `vm_handoff.rs` 改为 3、合计改为 54、正文"上述 52 处"改为"上述 54 处"；若有意把 boot_form 移出清单，需在表中保留"已移除"说明而不是让三个数字并存。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若某个工作树外的生成文件或 `target/` 被计入导致 54 与 53 的差异——`rg` 默认遵守 `.gitignore`，`os/target/` 已被 `**/target/` 排除；我另行用 `--glob '!target'` 复核仍为 54。若基线期望是 53，则文档正文的 52 仍是错的。
- 判据：Doc Code Sync 七项之"测试计数/常量"；任务 3；`review.md` P0-fact 的"常量与源码不符"（本项按 P1 计，因为它是分布口径而非设计合同）。

---

## FINDING RD4-FACT-05（P1）

- 位置：全文 86 个 fenced block（其中 70 个带路径头的代码块：33 `c` + 37 `rust`；1 个 `rust` 反例块无路径）的引文标注；六门读数中的 `tools/doc-citation-check.py`
- 严重度：P1
- 问题：`doc-citation-check.py` 的读数 `blocks=86 checked=0 failed=0 skipped=86`——86 个块全部被跳过，机械引文对账实际没有校验任何一行。原因是文档的标注方言与工具的解析式不兼容：工具只接受 `// 代码引用，os/...:fn x。` 或 `// 节选（...），os/...:fn x。`（`ANNOTATION_RE` 要求中文逗号分隔的 `代码引用|节选|改写|示意|反例` 前缀），而本文 86 个块没有一个使用该格式（常见形态是 `// os/...:fn x —— 逐字引用` 或 `// os/... —— 节选（...）`）。
- 影响：引文与源码的机械对账门形同虚设；像 RD4-FACT-02 那种"标注逐字引用但源码已改"的漂移不会被任何工具拦住，只能靠人工。六门里"引文"这门的 PASS 不能成立（读数是"0 个被检查"，不是"0 个失败"）。
- 证据：`tools/doc-citation-check.py rewrite-notes/01-stage-kernel/04-platform-discovery.md` → `blocks=86 checked=0 failed=0 skipped=86`；脚本 `ANNOTATION_RE = ^\s*//\s*(代码引用|节选|改写|示意|反例)(?:\s*（([^）]*)）)?\s*[，,]\s*(.+?)\s*。?\s*$`；用该正则逐块匹配 86 个块的首个非空行，命中 0 个。证据强度 E1。
- 最小修复：把每个块的首行统一改成工具接受的方言（`// 代码引用，os/...:fn x。` / `// 节选（...），os/...:fn x。`），再跑 `doc-citation-check.py` 直到 `checked` 等于块数或显式列出豁免；若判定该工具不适用于本文，需在文档头部或 Gate 记录里写明 `MANUAL_FALLBACK`，不能把 `checked=0` 当 PASS。
- 置信度：高
- 是否需源码核实：否（工具读数）
- 若我错了：若 `doc-citation-check.py` 另有一条未读到的兼容分支（例如按 `—— 逐字引用` 兜底）——实际 `--verbose` 也只报 skipped，且 `checked=0`；我按工具源码的 `ANNOTATION_RE` 与逐块匹配复核，未发现兜底。
- 判据：P-FACT 任务 0（六门机械门）与任务 5（引文块）；`review-process.md` Gate 证据规则；AGENTS.md "没有附命令与输出的 ✅ 通过一律作废"。

---

## FINDING RD4-FACT-06（P2）

- 位置：`rewrite-notes/01-stage-kernel/04-platform-discovery.md` §4.8（L2161）
- 严重度：P2
- 问题：文档写"常量对齐由 Step 2b 的一处编译期断言钉住（Step 1 用运行期 `assert_eq!`）"；实际 Step 2b 是 `const _: () = assert!(...)`（编译期），Step 1 的对齐/边界检查是运行期 `assert!`（`os/kernel/src/lib.rs:1250` 的 `assert!(end <= IDENTITY_MAP_END, ...)`），`os/kernel/src/lib.rs` 的 Step 1 区间没有对应的 `assert_eq!`。
- 影响：读者按文档去找 `assert_eq!` 会找不到，误以为 Step 1 还做了等值比较；两处断言的强度与性质（编译期 vs 运行期、`assert!` vs `assert_eq!`）被混淆。
- 证据：`sed -n '1330,1348p' os/kernel/src/lib.rs` 显示 `const _: () = assert!(MMIO_WINDOW_LEN % MMIO_BLOCK == 0 && ...)`；`sed -n '1248,1252p' os/kernel/src/lib.rs` 显示 `assert!(end <= IDENTITY_MAP_END, ...)`；`rg -n 'assert_eq!' os/kernel/src/lib.rs` 在 Step 1 区间（约 1221–1280）无命中。证据强度 E1。
- 最小修复：把"Step 1 用运行期 `assert_eq!`"改为"Step 1 用运行期 `assert!`"；若想强调对齐，补一句 Step 2b 的编译期断言是唯一对 `MMIO_*` 取模的检查。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若文档指的是另一个不在 `os/kernel/src/lib.rs` 的 Step 1（例如某个测试或 arch 层函数）——文中上下文明确是 `kernel/src/lib.rs` 的 Step 1/Step 2b 对举，且该文件内没有对应 `assert_eq!`。
- 判据：P-FACT 任务 4（行为合同/常量）；Doc Code Sync 七项之"符号/常量"。

---

## FINDING RD4-FACT-07（P2）

- 位置：`rewrite-notes/01-stage-kernel/04-platform-discovery.md` §1.4（L119）与 §4.1（L1520）
- 严重度：P2
- 问题：同一篇文档对"C 侧 ACPI 结果的消费者"给了两个不一致的枚举。§1.4 写"三处内核内消费者：`apic_single_cpu_init` / `discover_cpus` / `acpi_init_poweroff`（FACP 表）"；§4.1 写"C 侧消费 ACPI 结果的入口只有少数几处——APIC 初始化……与 CPU 拓扑发现……，外加用户态 ACPI 驱动经 `machine.acpi_rsdp` 的一处"，把内核内的 `acpi_init_poweroff` 漏掉了。
- 影响：读者会得到"内核内只有两个 ACPI 消费者"的错误印象；而 §1.4 自己已经把 `acpi_init_poweroff` 列为第三处，且 C 的 `acpi_init` 末尾确实调用它并用 `acpi_get_table_base("FACP")` 读 FACP/DSDT。两处对照会让人怀疑消费者清单的完整性。
- 证据：文档 L119 与 L1520 原文；C `minix3/minix/kernel/arch/i386/acpi.c:310-341` 的 `acpi_init` 末尾调用 `acpi_init_poweroff()`，后者在 L230 起用 `acpi_phys2vir(acpi_get_table_base("FACP"))` 读 FACP、用 `fadt_header->dsdt` 读 DSDT。证据强度 E1。
- 最小修复：§4.1 的句子补上 `acpi_init_poweroff`（或改写为"除 ACPI 模块内部的 poweroff 解析外，内核内只有两处"），与 §1.4 的三处清单对齐。
- 置信度：高
- 是否需源码核实：否（已核实）
- 若我错了：若 §4.1 的"消费 ACPI 结果"被限定为"ACPI 模块之外的消费者"，而 `acpi_init_poweroff` 是 `acpi_init` 内部的最后一步——那应在 §4.1 明说这个限定，现文没有。
- 判据：P-FACT 任务 2（文档-代码同步）与任务 6（覆盖率缺口）；同一文档内部事实一致性。

---

## FINDING RD4-FACT-08（P2）

- 位置：`rewrite-notes/01-stage-kernel/04-platform-discovery.md` §4.9 `init_smp_state` 代码块（L2186–L2206）、§3.3 `os/plat/src/riscv64/interrupt.rs:fn new` 代码块（L1183–L1196）、§4.8 aarch64 `new()` 代码块（L2114 起）
- 严重度：P2
- 问题：三个"节选"代码块没有做到"只省略、不改写"（冻结决策 D-045），且两处省略没有用 `...` 标注：
  1. §4.9 `init_smp_state`：源码的 5 行注释（"OpenSBI may hand control to any hart ... clock state all land on the CPU that is actually running."）被压缩成 2 行改写；`state.seed_bsp_masks` 前的 2 行注释（"Publish BSP_LOGICAL ... idempotent."）被删掉但块内没有 `...`；块尾还加了源码没有的 `// ... boot set re-home + BSP ptproc slot (see source)`。
  2. §3.3 `os/plat/src/riscv64/interrupt.rs:fn new`：源码在 `.expect(...)` 与 `Self {` 之间有 4 行注释（"plic_va_base defaults to the identity view; ..."），文档块直接删掉，没有 `...`。
  3. §4.8 aarch64 `new()`：源码的英文注释被改写成中文（块首自述"注释有删改"），属于改写而非省略。
- 影响：代码块被读者当作"源码节选"阅读，但注释内容与源码不同；`init_smp_state` 的块尾总结行更是文档作者的话，不是源码。D-045 的"节选引文只许省略不许改写"被违反，且省略未标注会让读者误以为源码没有那些注释。
- 证据：`sed -n '1961,1984p' os/kernel/src/lib.rs` 与文档 L2186–L2206 对照；`sed -n '107,121p' os/plat/src/riscv64/interrupt.rs` 与文档 L1183–L1196 对照；`sed -n '225,265p' os/plat/src/arm64/interrupt.rs` 与文档 L2114 起对照。证据强度 E1。
- 最小修复：把三块的首行标注改成 `// 改写（...），path:fn x。` 或 `// 示意（...），path:fn x。`（工具对这两类豁免逐行检查），或者把注释恢复为源码逐字并在省略处加 `...`；`init_smp_state` 块尾的总结行删掉或移到正文。
- 置信度：中高
- 是否需源码核实：否（已核实）
- 若我错了：若面板把"节选"理解为允许作者用中文转述注释（文档 §4.5 的 `parse_cpu_topology` 块确实这样自述），那么问题降为标注不一致——但 D-045 明写"只许省略不许改写"，且 §4.5 那块至少显式写了"注释是中文转述，不是源码逐字"，§4.9 与 §3.3 没有这句。
- 判据：冻结决策 D-045；P-FACT 任务 5（引文块与省略标注）；`review-doc-checklist` 的引文纪律。

---

## SUGGESTION

以下为不构成事实错误、但建议顺手收口的项（P3 级；不入 FINDING 台账）：

- **S-01（§6.2 #5 输出顺序）**：命令 `rg -n 'acpi_init\(\)' minix3/minix/kernel/` 的实测输出顺序是 `arch_smp.c` → `apic.c` → `arch_system.c`，文档写的是 `arch_system.c` → `arch_smp.c` → `apic.c`。`rg` 的并行遍历顺序不保证稳定；建议命令加 `--sort path` 并把预期输出按排序后的结果重写，否则下一位复核者会以为命令或文档有一处错。
- **S-02（§6.2 #4 的 `head -3` 输出）**：命令以 `| head -3` 结束，但文档在"其余见 §5.3 分布表"之后又列了 `handoff_v1_write.rs:1` 与 `kerninfo.rs:1` 两行——这两行不可能由该命令打印。建议删掉这两行，或去掉 `head -3` 并给完整输出（同时按 RD4-FACT-04 修正总数）。
- **S-03（§4.4 `AcpiParseError` 列表）**：正文列了 6 个变体，实际枚举有 13 个（含 `BadRsdpSignature`、`NoXsdtPointer`、`MadtNotFound`、`MadtTooShort`、`GicdNotFound`、`GicrNotFound`、`GicVersionUnsupported`）。若句子本意是"例如"，加"例如"二字即可；若本意是穷举，补齐 13 个。
- **S-04（§1.4 与 §5.3 的循环引用）**：§5.3 表说 21 个测试内核"全量即 §6.2 #4 输出里的测试内核条目"，而 §6.2 #4 的输出又写"其余见 §5.3 分布表"——两边互指，读者拿不到 21 个名字。建议在其中一处直接列出 21 个目录名（或给 `rg -l` 的一行命令）。
- **S-05（§4.1 "Phase B 的第一个动作"）**：x86-64 在 `init_protection` 之前还有一段 `GS.BASE` 三点采样（`os/kernel/src/lib.rs` Phase B 段，`#[cfg(target_arch = "x86_64")]`）；`init_protection` 是"第一个实质初始化动作"而非"第一个动作"。建议加"实质"二字。
- **S-06（§2.9 板号缺失的例子）**：文中说"例如全名在 `board` 环境里丢了"会让 `board_id` 为 0；若 `board` 键完全缺失，`env_get` 返回 `NULL`，`get_board_id_by_name(NULL)` 会把 `NULL` 交给 `strncmp`（不是返回 0）。若想表达"键在但值不认识/为空"，建议写清楚；否则这个例子会引导读者以为缺失键也会安静落到 0。
- **S-07（§4.10 `MachineStruct` 宽度）**：表里写 `acpi_rsdp` 对应 `phys_bytes (u64)`；C 的 i386 `phys_bytes` 是 32 位，Rust 的 wire 结构是 64 位。§4.10 的"对账"若只谈语义没问题，但建议加一句"这是 Rust 侧 wire ABI，不是与 C i386 `struct machine` 的字节级同构"，避免被读成布局一致。

---

## 核对覆盖与阴性声明

| 检查 | 扫了什么 | 发现数 | N=0 时不覆盖什么 |
|---|---|---|---|
| C 引文逐字 | 文档中 33 个 `c` 块的头注释路径与源码逐行对照（去空行/注释行后）；重点核了 `acpi.c`、`apic.c`、`arch_smp.c`、`arch_system.c`、`i8259.c`、`memory.c`、`main.c`、`clock.c`、`type.h`、`board.h`、`omap_*.c`、`pre_init.c`、`usermapped_data.c`、`do_getinfo.c`、ACPI 驱动 | 0 处非注释代码行不符 | 注释文本的逐字一致性由人工抽看关键块（`struct machine`、`board.h`、`acpi_init` 等），未做全量字符级 diff；C 运行时行为未执行 |
| Rust 引文逐字 | 37 个可定位源码的 `rust` 块按头注释路径与源码做非注释行子序列核对（另 1 个"教学简化，本仓不存在"的反例块按声明豁免）；另人工对照了 `KernelInfo`、`PlatformDescSource`、`Frozen`、`PlatformDescEnum`、`init_from_kinfo`、`check_gic_madt`、`TIMER_IRQ`、`MachineStruct`、`MachineInfo` 等逐字块 | 2 个块（§4.2 uefi_helpers/opensbi）出现源码中不存在的代码行 → RD4-FACT-02；其余 0 | 注释改写/省略（RD4-FACT-08）不在该启发式的射程内；声明为"示意/中文转述"的块按声明豁免，不证明其与源码同形 |
| 锚点/路径 | `anchor-resolve.sh --check` 的 72 个锚点；文档全部 74 个 `minix3/` 与 `os/` 路径的 `os.path.exists` | 1 个零定义锚点 → RD4-FACT-01；0 个缺失路径 | 跨文档章节引用的语义正确性（只验证可解析）；C 符号的"同名不同函数"歧义未逐个人工复核 |
| 常量/签名 | 文档出现的 410、35、176、44、64 KiB、64、24、0xFEE0_0000、0xFEC0_0000、0x0C00_0000、0x0800_0000、0x080A_0000、0x0900_0000、0x0803_0000、0x200_BFF8、0x200_4000、1_193_182、10_000_000、16250/15000/660/32768、0x50/0x70、8、0x2_0000 等；5 个结构体字段表 | 0 处不符 | 未对每个常量做二进制/真机验证；只验证源码文本 |
| 测试名对账 | `minix-platform` 26 条 `#[test]`（逐文件 9/4/4/4/3/1/1）；`uefi_helpers` 12 条；`test-smp-topo*` 三变体的 `test_` 函数与断言；`run_all.sh` 的六行引用 | 0 个不存在的测试名；1 处文件级计数 6→12 → RD4-FACT-03 | 测试的运行结果未执行（只做 `cargo test --list` 的编译与枚举）；测试是否真能证明其声称的契约未逐条执行 |
| 宿主运行数 | `cargo test -p minix-platform -- --list` | 18 条，与文档一致 | 其它 crate 的宿主测试数未核（不在本篇声称范围） |
| `&[]` 计数 | 文档给出的 `rg` 命令 + 逐文件 `rg -c` | 实测 54，文档命令 53、表/正文 52 → RD4-FACT-04 | 每个夹具的"是否依赖平台发现"判断未逐个读测试体（只核了计数与名单） |
| 反事实前提 | §1.1、§1.3、§1.4、§2.1–§2.9、§3.2、§3.4、§3.5、§3.7、§3.8、§4.1–§4.7、§5.1–§5.3、§6.1 的"会坏在哪"段 | 0 处前提不存在（§2.9 的"board 环境里丢了"例子见 S-06） | 反事实的工程后果大多无法在不改代码的前提下实测；只核对前提是否有源码依据 |
| 阴性声明 | `local_apic` 全部出现（1 行声明）；`arch/` 只有 `earm`/`i386`；`acpi_init()` 三个调用点；`ACPI_MADT_TYPE` 11 个声明；`board_id` 无 Rust 板卡标识；`SPCR` 无 Rust 消费者；`early_console()` 无内核活消费者；`init_from_kinfo` 生产调用 1 处 | 与文档声称一致 | 阴性声明只覆盖我实际跑过的 `rg` 模式；未覆盖的相似拼写（如 `LocalApic`、缩写别名）不在 N=0 的保证内 |
| 文档-代码同步七项 | 静态可变、路径、常量、内部 TODO、符号锚点、工作区前缀、测试计数 | 路径/常量/锚点/测试计数各有发现（01/02/03/04）；`static mut` 只出现在方案否决表与反事实段，`TODO/FIXME` 只出现在 C 引文与 TODO 表头，无未跟踪的内部 TODO | 工作区前缀（`os/`）由 doc-code-map 52/52 覆盖；未检查文档中所有裸文件名是否都带前缀（只检查了带反引号的路径 token） |
