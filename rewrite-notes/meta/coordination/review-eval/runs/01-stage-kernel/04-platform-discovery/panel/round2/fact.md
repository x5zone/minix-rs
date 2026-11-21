# D04 R2 确认轮 — P-FACT（C+Rust）事实核对

- 基线：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`，2708 行，sha256 前缀 `773c10f1`（本轮实测一致）。
- 范围：文档内全部事实断言、路径/行号锚点、引文、测试计数、常量/签名、反事实与阴性声明；对照 `minix3/` 与 `os/` 现源码。
- 口径：只读；每条 FINDING 给出位置 / 严重度 / 问题 / 影响 / 证据 / 最小修复 / 置信度 / 是否需源码核实 / 若我错了 / 判据。
- 已复核通过的门读数（无异议）：`rg -n 'acpi_init\(\)' minix3/minix/kernel/` = 3 条（arch_system.c:264 / arch_smp.c:315 / apic.c:1141）；`rg -n 'local_apic' .../arch/i386/` 仅 acpi.h:99；`wc -l acpi.c` = 410；`grep -c 'define ACPI_MADT_TYPE'` = 11；`Self::(DeviceTree|Acpi|QemuVirt)` = 25；构造点 global.rs:2 / kind.rs:4；`#[test]` 合计 26（逐文件 9/4/4/4/3/1/1）；`cargo test -p minix-platform` 实跑 18 passed；`platform_sources: &[]` 合计 54（10+9+6+3+21+1+1+1+1+1）；`run_all.sh` 的 96/102/108/131/211/246 行全部命中。

---

## FINDING RD4B-FACT-01（P2）

- **位置**：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/04-platform-discovery.md:1886`（§4.5 第一段）；同段 1890-1896 的模块注释引文重复同一说法。
- **严重度**：P2（机制断言与实现不符；结论"DTB 物理地址可解引用"仍成立，但理由写错）
- **问题**：文档说 `DeviceTreeDesc::parse` 能直接解引用 DTB 物理地址的"前提是此时内核已启用分页、低 4GB 铺了 VA == PA 的恒等映射（02 篇建立的映射，C 侧对应 `pg_identity()`）"。现内核并不铺满低 4GB：`arch_boot_impl` Step 1 只建立"最小启动恒等窗口"（shim 镜像/栈、内核 LMA、bootstrap pool、root/bump、模块、platform sources、设备窗口），源码注释明确写了全铺低 4GB 是已废弃做法。
- **影响**：读者会把"低 4GB 恒等映射"当成现行约束，据此推断任意低地址物理指针都可解引用；实际只有被列入窗口的地址可解引用。DTB 之所以可用，是因为 `push_platform_windows` 为每个 `PlatformDescSource` 单独建窗口（有 `len` 时按整块 blob，legacy `len=0` 时按 2 MiB 片段），不是"低 4GB 全铺"。同一段的"DTB 由 boot-shim 留在低地址的静态固件内存里"对 aarch64 UEFI 腿也不成立（该腿经 `dtb_copy::authorize_and_copy` 复制到 shim 自有缓冲区）。
- **证据**：
  - `/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:1221-1229`：`// Step 1: Minimal boot identity windows — VA = PA only for the pieces ... // Mapping the whole low 4 GiB kept every hole PRESENT and hid wild accesses; the minimal windows turn those into page faults.`
  - `/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:445-465`（`push_common_boot_windows` 调用处）：注释列出的窗口类别为 `shim image/stack, kernel LMA, root/bump/arena, modules, platform tables and device windows`。
  - `/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:1123-1142`：`push_platform_windows` 按 `source.has_len()` 建 `[start, start+len)`，否则 `push_2m(address)`。
  - `/home/xzhao/github/minix-rs/os/boot-shim/src/dtb_copy.rs:1-23`：UEFI 腿的 DTB 是"owned copy"（header 来自 preflight 捕获、body 来自源页），不是留在原固件内存。
- **最小修复**：把"低 4GB 铺了 VA == PA 的恒等映射"改为"启动期 Step 1 的恒等窗口覆盖了 DTB blob 的完整地址范围（有长度记录时按整块、legacy 无长度时按 2 MiB 片段）"，并删除"02 篇建立的映射 / C 侧对应 `pg_identity()`"这一沿用旧设计的类比；`device_tree.rs` 模块注释的同一句是过时原文，引文处加"（模块注释沿用旧口径）"或同步修源。
- **置信度**：高（源码注释与窗口构造函数两处互证）
- **是否需源码核实**：是（已核 `os/kernel/src/lib.rs`、`os/boot-shim/src/dtb_copy.rs`）
- **若我错了**：若 02 篇描述的"低 4GB 恒等映射"在当前 `arch_boot_impl` 之外还有另一条仍生效的映射路径（例如 Direct Map Step 4 之前对全部 4 GiB 的临时根），则原断言可成立；但 `kmain Phase A.5` 发生在 `arch_boot_impl` Step 0–4 之后，届时活动根是 Step 1 的最小窗口根，故该反驳不成立。
- **判据**：真值优先序——Rust 实现 > 技术文档；同一事实两处源码冲突时以现行 `os/kernel/src/lib.rs` 为准。

---

## FINDING RD4B-FACT-02（P2）

- **位置**：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/04-platform-discovery.md:1274`（§3.4 末段）。
- **严重度**：P2（设计论证的可变性前提被源码反例推翻；同篇 §4.8 自相矛盾）
- **问题**：文档断言"实例字段在构造后不再被 `&mut self` 方法重写；……读者拿到 `&self` 就知道描述符字段不会再变"。riscv64 的中断控制器实例有公开 setter：`set_plic_va_base(&mut self, va_base)` 在构造后写入 `plic_va_base`，且生产路径真的调用它。同篇 §4.8 的表（文档 2170 行）自己也写着"PLIC 实例存 `plic_base` 与 `plic_va_base`；内核 check-and-reject 后注入高半 VA"。
- **影响**：§3.4 用"实例字段不可变"作为实例化优于 C 全局变量的论据；这一论据对 riscv64 PLIC 实例不成立（存在一次受控的建后写入）。设计本身仍可辩护（写一次后只读、由内核校验后注入），但当前措辞是绝对断言，读者会据此排除"实例构造后仍可被改写"的故障形态。
- **证据**：
  - `/home/xzhao/github/minix-rs/os/plat/src/riscv64/interrupt.rs:282-288`：`/// PD-22 step 2: install the kernel device-window VA for this PLIC. ... pub fn set_plic_va_base(&mut self, va_base: usize) { self.plic_va_base = va_base; }`
  - `/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:2493`：`intr.set_plic_va_base(plic_va);`
  - 文档自身 §4.8 表格 2170 行与 §4.8 正文 2172 行（x86 check-and-reject 描述）都承认"注入"这一步。
- **最小修复**：改为"描述符来源的基址/频率字段在构造后不再被重写；riscv64 的 `plic_va_base` 是唯一的建后注入字段（`set_plic_va_base`，由内核 check-and-reject 后写入设备窗口 VA）"。
- **置信度**：高（setter 与调用点均在现行源码）
- **是否需源码核实**：是（已核 `os/plat/src/riscv64/interrupt.rs:286`、`os/kernel/src/lib.rs:2493`）
- **若我错了**：若 `set_plic_va_base` 不被视为"实例字段重写"（例如把 `plic_va_base` 排除在"描述符字段"之外），则原句可勉强成立；但原句主语是"实例字段"，`plic_va_base` 是实例字段，故该解释不成立。
- **判据**：文档内两处（§3.4 与 §4.8）对同一事实的表述必须一致；以代码为准。

---

## FINDING RD4B-FACT-03（P2）

- **位置**：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/04-platform-discovery.md:1071-1075`（§3.1 `KernelInfo` 代码块，标注"节选（本字段的文档注释）"）。
- **严重度**：P2（节选引文改写，违反 D-045"节选引文只许省略不许改写"；且丢掉了 `len = 0` 的窗口构造规则）
- **问题**：文档把 `kernel_info.rs` 的 Cross-binary safety 注释重排并改写为：
  `Each PlatformDescSource is pure data (u32 kind, u32 len, u64 phys_addr) — len = 0 means unknown/legacy — safe to pass ...`
  源码实际为两段：
  `Each PlatformDescSource is pure data (u32 kind, u32 len, u64 phys_addr)` / `— safe to pass ... No function pointers cross the boundary. len = 0` / `means the producer did not record the blob extent; the window builder then falls back to one 2 MiB piece.`
  即：`len = 0` 的语义被提前并改写为"unknown/legacy"，且"window builder 退回 2 MiB 片段"这整句被删除而未以省略号标示。
- **影响**：①引文不再是逐字节选，读者无法核对原文；②被删掉的正是 §4.2/§5.3 依赖的 `len=0` 回退规则（窗口按地址所在 2 MiB 片段覆盖），削弱了文档自身的可追溯性。
- **证据**：
  - 文档 1071-1075 行（上述改写文本）。
  - `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/kernel_info.rs:130-136`（原文，逐字如上）。
  - `/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:1123-1142`（`push_platform_windows` 的 `has_len`/`push_2m` 分支，证明被删句是现行业务规则）。
- **最小修复**：把注释块恢复为源码逐字（可用 `...` 表示省略），或在改写处显式标注"非逐字，见源码"并补回 `len = 0` 的 2 MiB 回退规则。
- **置信度**：高（逐字比对）
- **是否需源码核实**：是（已核 `kernel_info.rs:130-136`）
- **若我错了**：若仓库另有版本的 `kernel_info.rs` 注释恰为文档文本，则引文成立；当前 HEAD 工作区不是，故不成立。
- **判据**：D-045（节选引文只许省略不许改写）+ 文档 1067 行自注"节选（本字段的文档注释；省略号处为其余字段）"。

---

## FINDING RD4B-FACT-04（P3）

- **位置**：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/04-platform-discovery.md:2125-2133`（§4.8 aarch64 `new()` 代码块及其头注）。
- **严重度**：P3（仅注释文本；代码与断言逐字一致，语义未丢）
- **问题**：代码块头注写"节选（注释有删改：行首的过程编号未保留）"，但现行源码 `os/plat/src/arm64/interrupt.rs:230-235` 的注释是英文且**没有**行首过程编号；文档块内是中文翻译（"先判零再翻译——descriptor 缺 GICD/GICR/GICR stride……"），不是"删掉编号后的原文"。
- **影响**：读者按头注去源码核对时会找不到对应文本；不影响对代码行为的理解（三处 assert 与 `Self` 构造均逐字一致）。
- **证据**：
  - 文档 2125-2133 行（中文注释块）。
  - `/home/xzhao/github/minix-rs/os/plat/src/arm64/interrupt.rs:230-235`（英文原文：`// Check for zero before translating: ...`）。
- **最小修复**：把头注改为"注释为中文转述，非逐字"，或直接换回源码英文注释。
- **置信度**：高（逐行比对）
- **是否需源码核实**：是（已核 `os/plat/src/arm64/interrupt.rs:226-266`）
- **若我错了**：若仓库另有带编号注释的 `interrupt.rs` 版本（例如生成源或历史分支），则头注成立；当前工作区不是。
- **判据**：代码块头注是对源码形态的事实断言；注释内容属"改写"，与 D-045 的"只许省略"冲突。

---

## FINDING RD4B-FACT-05（P3）

- **位置**：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/04-platform-discovery.md:2206-2211`（§4.9 `init_smp_state` 代码块）。
- **严重度**：P3（引文内部有改写与顺序调整，但不是关键行为断言）
- **问题**：块首标注"节选（第一个 unsafe 块）"，但块内 `use minix_platform::{platform_desc, PlatformDesc};` 与源码顺序相反（源码为 `{PlatformDesc, platform_desc}`），且提示注释被压缩改写：源码是"OpenSBI may hand control to any hart (QEMU -smp 4 has booted hart 1 and hart 2), and the entry election cell records the real one. Resolve it here, before per-CPU state is seeded, so the BSP's READY flag, ptproc and clock state all land on the CPU that is actually running."，文档压成"election/CPUID resolves the CPU actually executing this code"。
- **影响**：引文可核对性下降；`use` 顺序不影响语义，注释压缩不改变结论，但对"节选只许省略"的纪律构成偏离。
- **证据**：
  - 文档 2206-2210 行。
  - `/home/xzhao/github/minix-rs/os/kernel/src/lib.rs:1962`（`use minix_platform::{PlatformDesc, platform_desc};`）与 `1963-1968`（英文注释原文）。
- **最小修复**：恢复源码 `use` 顺序；注释按原文逐字，或显式标注"注释为转述"。
- **置信度**：高（逐行比对）
- **是否需源码核实**：是（已核 `os/kernel/src/lib.rs:1961-1975`）
- **若我错了**：若 `rustfmt` 会在某配置下重排 `use` 组（`rustfmt` 默认不重排花括号内顺序），则顺序差可解释；注释改写仍无解释。
- **判据**：D-045（节选引文只许省略不许改写）。

---

## FINDING RD4B-FACT-06（P3）

- **位置**：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/04-platform-discovery.md:2548`（§5.4 M7 证据列）。
- **严重度**：P3（构造器名与签名不符；证据链仍成立）
- **问题**：M7 写"DTB 单源装在 `os/kernel-image/src/bootface_a64.rs:fn bootface_a64`（`PlatformDescSource::new(DTB, DTB_PA)` → `parse_gic`）"。实际构造是 `PlatformDescSource::with_optional_len(DTB, PhysBytes(DTB_PA), dtb_len)`（`dtb_len` 由 `dtb_total_size` 读出）；`new` 的第二参也要求 `PhysBytes`，不是裸 `DTB_PA`。
- **影响**：读者按 `new(DTB, DTB_PA)` 检索会找不到调用点；`with_optional_len` 与 `new` 的差别正是 §4.2 讨论的 `len` 交接格式，写错会削弱该段的论证。
- **证据**：
  - `/home/xzhao/github/minix-rs/os/kernel-image/src/bootface_a64.rs:298-300`：
    `let dtb_len = unsafe { minix_boot::dtb_total_size(PhysBytes(DTB_PA)) };`
    `storage.platform_slots_mut()[0] = PlatformDescSource::with_optional_len(DTB, PhysBytes(DTB_PA), dtb_len);`
  - `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/platform.rs:125-166`（`new` 与 `with_optional_len` 的签名）。
- **最小修复**：把括号内改为 `PlatformDescSource::with_optional_len(DTB, PhysBytes(DTB_PA), dtb_len)`。
- **置信度**：高
- **是否需源码核实**：是（已核 `bootface_a64.rs:298`）
- **若我错了**：若 bootface 另有调用 `PlatformDescSource::new(DTB, DTB_PA)` 的分支（如 aarch64 以外），该表述可指那一处；当前文件只有 `with_optional_len` 一处平台源构造。
- **判据**：文档 §4.2 自己给出的交接线格式是 `(kind, len, phys_addr)`，证据列应与该格式一致。

---

## FINDING RD4B-FACT-07（P3）

- **位置**：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/04-platform-discovery.md:2505`（§5.3 第一条缺口）。
- **严重度**：P3（计数本身正确，括注只解释了其中 6 条，读者会算不平）
- **问题**：文中说"该文件有 12 条 `#[test]`（1 条测 `KernelInfo` 字段、5 条测 bootstrap memmap 边界），均不覆盖 `find_platform_sources_into`"。括注只覆盖 1+5=6 条，剩余 6 条（`test_reserve_kernel_span_rejects_unaligned_span`、4 条 `test_assert_range_clear_of_kernel_*`、`test_assert_modules_clear_of_kernel_rejects_byte_overlap`）没有归类。
- **影响**：读者会把 12 与 6 并列，怀疑计数口径；"均不覆盖 `find_platform_sources_into`"这一关键结论本身经复核为真（该文件仅 208 行函数定义与 1035 行调用，无测试引用）。
- **证据**：
  - `/home/xzhao/github/minix-rs/os/boot-shim/src/uefi_helpers.rs:1296-1479` 的 12 个 `fn test_*`（已逐条列出）。
  - `grep -c '#\[test\]' os/boot-shim/src/uefi_helpers.rs` = 12。
  - `rg -n 'find_platform_sources_into' os/boot-shim/src/uefi_helpers.rs` 仅 208/1035 两行，无测试。
- **最小修复**：括注补一句"其余 6 条测 kernel-span 预留与 assert 边界"。
- **置信度**：高
- **是否需源码核实**：是（已核 `uefi_helpers.rs:1296-1479`）
- **若我错了**：若"12 条"的口径不是文件全部 `#[test]`（例如只数 `mod tests` 内某子集），则括注可成立；现行文件只有这一个 `mod tests`，故不成立。
- **判据**：同一句话里的总数与分项必须相合。

---

## FINDING RD4B-FACT-08（P3）

- **位置**：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/04-platform-discovery.md:2030`（§4.6 `CpuInfo::zero` 段）。
- **严重度**：P3（语义准确，标注方式与同篇其他转述不一致）
- **问题**：文中以中文引号给出"`#[derive(Default)]` 生成的是非 const 的 `default()`，所以 `const fn` 不能调它"，并说"注释写明"。源码注释是英文："`#[derive(Default)]` generates a non-const `default()`, so `const fn`s cannot call `CpuInfo::default()`."。这是翻译，不是原文；同篇 §4.5 对同类情况明确标注"注释是中文转述，不是源码逐字"。
- **影响**：读者按引号内文字去源码检索会找不到；语义无偏差。
- **证据**：
  - 文档 2030 行。
  - `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/platform.rs:306-316`（英文原文）。
- **最小修复**：改成英文原文引用，或加"（中文转述）"。
- **置信度**：高
- **是否需源码核实**：是（已核 `platform.rs:306-316`）
- **若我错了**：若仓库存在中文注释版本（例如生成的文档注释），则该引文成立；当前源不是。
- **判据**：引号内的"注释写明"应与源码字符一致，或显式标注转述。

---

## FINDING RD4B-FACT-09（P3）

- **位置**：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/04-platform-discovery.md:2708`（§7 术语表末注）。
- **严重度**：P3（术语结论正确，范围断言过窄）
- **问题**：文中说"`FDT` 只作为格式名出现在注释与 `fdt_layout` 的内部常量名里"。实际 `os/libs/minix-boot/src/platform.rs:185` 有 `pub const FDT_MAGIC: u32 = 0xd00d_feed;`（public，不在 `fdt_layout`），`fdt_layout.rs:103` 的 `FDT_MAX_LEN` 也是 `pub const`。核心结论"没有 `FDT` 这个 kind 常量"仍成立（`kind.rs` 只 re-export `DTB`/`RSDP`）。
- **影响**：读者若按"只在 fdt_layout 内部"去检索，会漏掉 `FDT_MAGIC` 这个公共常量；不影响术语表的主结论。
- **证据**：
  - `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/platform.rs:185`。
  - `/home/xzhao/github/minix-rs/os/libs/minix-boot/src/fdt_layout.rs:100-103`。
  - `/home/xzhao/github/minix-rs/os/libs/minix-platform/src/kind.rs:28`（`pub use minix_boot::platform::{DTB, RSDP};`）。
- **最小修复**：改为"`FDT` 只作为格式名出现在注释与若干常量名里（`FDT_MAGIC`、`FDT_VERSION`、`FDT_MAX_LEN` 等），kind 常量仍只有 `DTB`"。
- **置信度**：高
- **是否需源码核实**：是（已核上述三处）
- **若我错了**：若"内部常量名"是泛指"非 kind 常量的常量名"，则该句可宽泛成立；但 `FDT_MAGIC` 是 `pub`，不是"内部"。
- **判据**：绝对范围词（"只"）需与全仓检索一致。

---

## FINDING RD4B-FACT-10（P3）

- **位置**：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/04-platform-discovery.md:1541`（§4.1 末段）与 `:113-116`（§1.4 概念图第 ⑤ 段）。
- **严重度**：P3（文档内部计数不一致；不影响"Rust 消费者更多"的结论）
- **问题**：§1.4 把 C 侧内核内消费者列为三处：`apic_single_cpu_init` / `discover_cpus` / `acpi_init_poweroff（FACP 表）`；§4.1 复述时只列了 APIC 初始化与 CPU 拓扑发现两处，把 `acpi_init_poweroff` 漏掉，并说"C 侧消费 ACPI 结果的入口只有少数几处——…与…，外加用户态…"。
- **影响**：读者会把 §4.1 的两处当完整清单；§1.4 与 §4.1 的"三处/两处"不一致会让"消费者变多了"这一论证的基数模糊。若 §4.1 的意图是"四组硬件参数的消费者"，应在措辞上限定，而不是省略 §1.4 已列的第三处。
- **证据**：
  - 文档 113-116 行（§1.4 的三处清单）。
  - 文档 1541 行（§4.1 的两处清单）。
  - `/home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/acpi.c:230-249,310-340`（`acpi_init_poweroff` 确实消费 `acpi_get_table_base("FACP")`，并在 `acpi_init` 末尾调用）。
- **最小修复**：§4.1 补上 `acpi_init_poweroff`，或把句子改为"消费四组硬件参数的入口"以限定范围。
- **置信度**：中高（文本对账清楚；"入口"一词的意图需作者裁决）
- **是否需源码核实**：是（已核 `acpi.c:230-249`、`310-340`）
- **若我错了**：若作者定义"入口"只指"内核外向发现模块取值的调用者"，而 `acpi_init_poweroff` 是 `acpi_init` 内部步骤，则 §4.1 可以成立；但 §1.4 已把它与另两处并列为"三处内核内消费者"，两处表述至少有一处需改。
- **判据**：同篇两处对同一集合的计数必须一致。

---

## 复核通过（无 FINDING）的关键事实清单

以下条目已逐条对照现行源码，未发现偏差；记录命令/位置以便复核（L1 工具输出或 L2 直接阅读）：

1. **C 侧引文**：§2.1 `cstart`/`arch_init`/`init_clock`/`intr_init`、§2.2 `get_acpi_rsdp`/`acpi_rsdp_test`、§2.3 `acpi_init`/`acpi_read_sdt_at`/静态定义、§2.4 `acpi_madt_get_typed_item`/`acpi_get_lapic_next`/`discover_cpus`、§2.5 `acpi_phys2vir`/`lapic_enable_in_msr`、§2.6 `acpi_get_ioapics`/`detect_ioapics`、§2.8 `struct machine`/`usermapped_data.c`、§2.9 `cpu_identify`/`bsp_init`/`board.h`/`intr_init`/`kmain` 全部与 `minix3/` 原文逐字一致（仅按"节选"省略）。证据：`sed`/`rg` 逐段比对 `minix3/minix/kernel/arch/i386/{acpi.c,apic.c,arch_smp.c,arch_system.c,i8259.c,memory.c,main.c}`、`minix3/minix/include/minix/{type.h,board.h}`。
2. **C 侧行号锚点**：`acpi.c:410` 行、`acpi.h:99`、`main.c:304-307`/`443-468`、`memory.c:819/852/954`、`pci.c:917`、`arch_system.c:264`/`arch_smp.c:315`/`apic.c:1141`、`type.h:122-131` 全部命中。
3. **C 侧常量**：`TIMER_FREQ=1193182`、`INT_CTL/INT_CTLMASK/INT2_CTL/INT2_CTLMASK=0x20/0x21/0xA0/0xA1`、`NR_IRQ_VECTORS` 16/64 条件定义、`LOCAL_APIC_DEF_ADDR=0xfee00000`、`MAX_RSDT=35`、`ACPI_MADT_TYPE` 11 个定义、`sizeof(acpi_sdt_header)=36`、`sizeof(acpi_rsdt)=176`、`madt_header_size=44` 全部与源码一致。
4. **Rust 侧引文/签名**：§3.1 `KernelInfo`（除 FINDING-03 的注释改写）、§3.2 `PlatformDesc`/`PlatformDescEnum`、§3.3 三个子 trait、§3.5 `qemu_fallback_or_panic`、§3.6 `PlatformDescKind`/kind 常量注释、§3.7 `Frozen`/`init`/`platform_desc`、§4.3 `parse_by_kind`（含 `kind.rs:57/69`）、§4.5 `from_fdt`/模块注释/Safety 段、§4.6 `qemu_virt.rs` re-export/riscv64 `QemuVirtDesc`/`Default`、§4.7 `init_from_kinfo`/`PlatformContext`、§4.8 riscv64 `new`/`TIMER_IRQ`、§4.9 `MachineStruct`/`MachineInfo`/`init_protection`、§5.1/§5.2 测试代码块均与源码一致（省略处均属"节选"）。
5. **Rust 常量/规模**：`MAX_CPUS=64`、`NR_IRQ_VECTORS=64`、`IOAPIC_NUM_PINS=24`、`MAX_ROOT_ENTRIES=35`、`MAX_TABLE_LEN=64*1024`、QEMU 三份 `QemuVirtDesc` 值（`0xFEE0_0000`/`0xFEC0_0000`/`1193182`/`0x3F8`、`0x0800_0000`/`0x080A_0000`/`0x2_0000`/`0x0900_0000`、`0x0C00_0000`/`ctx 1`/`0x200_BFF8`/`0x200_4000`/stride 8/10 MHz）、`GICR_STRIDE=0x2_0000`、`MMIO_PA_BASE=0x0800_0000`/`MMIO_WINDOW_LEN=0x0200_0000`/`MMIO_VA_BASE=0xFFFF_C000_0000_0000` 全部一致。
6. **阴性声明**：`rg -n 'local_apic' minix3/minix/kernel/arch/i386/` 仅 acpi.h:99；per-arch 三个文件方法体内 `#[cfg(target_arch)]` 为 0（只有模块级 `#[cfg(test)]`）；`early_console()` 无生产消费者（仅 global.rs 转发与测试）；`Frozen` 无二次 freeze 宿主测试；`platform_sources: &[]` 的 54 条分布与 §5.3 表一致；`find_platform_sources_into` 无单测且 10 个测试内核调用（`rg -l` 计数 10）；`PlatformDescEnum::(DeviceTree|Acpi|QemuVirt)` 的构造点仅 kind.rs 两处、global.rs 两处、test-timer-irq-aarch64 一处。
7. **测试计数**：`#[test]` 合计 26（acpi 9 / device_tree 4 / arch x86_64 4 / arch riscv64 4 / arch aarch64 3 / kind 1 / global 1）；`cargo test -p minix-platform` 实跑 **18 passed / 0 failed**（与 §5.1 "x86_64 宿主实际运行 18 条"一致）；`test_check_gic_madt_decision_table` 实为 8 条 `assert_eq!`；`uefi_helpers.rs` 12 条 `#[test]`。
8. **反事实推演**：§2.3 的 32 位整数提升算式 `(-1-36)/4 = 1073741814`（i386 32 位 ABI 下成立）、`sdt_count` 越界进入点 `i=35`、`acpi_phys_copy` 在 `!vm_running` 返回 0；§2.4 去掉 `flags & 1` 后首次调用耗尽记录返回 NULL；§2.7 `no_apic`/`no_smp` 三种取值分支与 `cstart` 的读取次序；§2.10/§3.7 的"发现跑两次会撞 `Frozen::freeze`"——均与源码逻辑一致。
9. **路径存在性**：文档 §头部与 §7 列出的全部 `os/` 路径（`minix-boot`/`minix-platform`/`minix-types`/`kernel`/`plat`/`arch`/`boot-shim`/`qemu-tests`/`kernel-image`）与 `rewrite-notes/` 兄弟篇（01/02/03/05）均存在；`16-smp.md` 不存在但文档已注明"尚未重生成"。
10. **门读数复算**（命令与输出见本文件头部）：`acpi_init()` 3 条、`local_apic` 1 条、`wc -l acpi.c` 410、`grep -c ACPI_MADT_TYPE` 11、`Self::…` 25、`rg -c PlatformDescEnum` global 2 / kind 4、`#[test]` 26、`platform_sources &[]` 54、`run_all.sh` 7 行（96/102/108/131/211/246 加第 96 行内的多个名字）。

---

## 汇总

| ID | 位置 | 严重度 | 一句话 |
|---|---|---|---|
| RD4B-FACT-01 | §4.5 1886 行 | P2 | "低 4GB 恒等映射"与现行 Step 1 最小窗口实现不符（DTB 可用性结论仍成立） |
| RD4B-FACT-02 | §3.4 1274 行 | P2 | "实例字段构造后不被 &mut self 重写"被 riscv64 `set_plic_va_base` 反例推翻，且与 §4.8 自相矛盾 |
| RD4B-FACT-03 | §3.1 1071-1075 行 | P2 | `KernelInfo` 文档注释引文被改写，丢失 `len=0 → 2 MiB 片段` 规则 |
| RD4B-FACT-04 | §4.8 2125-2133 行 | P3 | aarch64 `new()` 注释为中文翻译，头注"过程编号未保留"与源码不符 |
| RD4B-FACT-05 | §4.9 2206-2211 行 | P3 | `init_smp_state` 节选的 `use` 顺序与注释文本被改写 |
| RD4B-FACT-06 | §5.4 2548 行 | P3 | bootface_a64 构造点写成 `new(DTB, DTB_PA)`，实为 `with_optional_len(..., dtb_len)` |
| RD4B-FACT-07 | §5.3 2505 行 | P3 | 12 条测试的括注只解释 6 条 |
| RD4B-FACT-08 | §4.6 2030 行 | P3 | `CpuInfo::zero` 注释以中文译文作引文，未标转述 |
| RD4B-FACT-09 | §7 2708 行 | P3 | "`FDT` 只在 fdt_layout 内部常量名里"漏掉 `pub const FDT_MAGIC` |
| RD4B-FACT-10 | §4.1 1541 行 vs §1.4 113 行 | P3 | C 侧消费者清单两处计数不一致（三处 vs 两处） |

**总评**：文档主体事实（C 引文、行号、常量、测试计数、门读数、反事实推演）与现行源码高度一致，未发现 P0/P1；10 条 FINDING 中 3 条 P2 需修（§4.5 机制、§3.4 可变性、§3.1 引文），其余 7 条为 P3 引文/范围/计数精度问题。
