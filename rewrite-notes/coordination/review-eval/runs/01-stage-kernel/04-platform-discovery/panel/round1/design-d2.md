# D04 R1 — P-DESIGN-D2（设计轴：第 3/4 章 × 现有 Rust 实现）

- 基线：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`，2688 行，sha256 前缀 `a8cb91b1`（复核：`sha256sum` 输出 `a8cb91b11e94…e5b5`）。
- 范围：只对第 3 章（§3.1–§3.9）与第 4 章（§4.1–§4.11）的设计陈述与 `doc-code-map` 52 个 Rust 文件做对照；关联 C 事实只引用文档内已给出的引文，不回读 `minix3/`。
- 允许材料：本文件（doc）、`tools/doc-code-map.sh` 输出的 52 文件、`trait-audit.md`、`AGENTS.md` Key Constraints、`round1-decisions-snippet.md`。未读取作者解释、其他面板输出、`expected/`。
- 只读：未改动任何被审代码或文档；本文件为唯一写盘产物（分批写入）。
- 复核口径：所有计数命令均在仓库根执行，输出直接抄录；代码行号为本次读取时的行号。

## DIFF-TABLE（七维差异表）

差异维定义：

- **D1 陈述不符**：第 3/4 章的陈述（含「逐字引用」）与当前代码不一致。
- **D2 非法状态**：类型或解析器允许「不能同时成立/无法区分来源」的状态。
- **D3 翻译残留**：C 机制、C 常量或已退役实现形态留在设计文本里。
- **D4 死抽象**：trait 方法、字段或全局量在本文范围内没有活消费者。
- **D5 最小化机会**：重复层、重复真值源、可合并的转发面。
- **D6 度量/映射漂移**：计数、清单或文件映射与实现不同步。
- **D7 契约/真值偏差**：偏离 Minix3 语义锚，或违反项目第一原则（抽象语义，不描述机制）。

| # | 文档锚点 | 文档陈述（摘要） | 代码事实（锚点） | 维 | 级别 | 标签 |
|---|---|---|---|---|---|---|
| R01 | §3.1（1055–1058）、§3.6（1359） | `PlatformDescSource` 是纯数据 `(u32 kind, u64 phys_addr)`；「一个 kind 标签加一个物理地址」 | 实为三字段 `(kind, len, phys_addr)`，`len` 进入 handoff 线格式并由 `push_platform_windows` 定启动窗口 | D1、D3、D6 | P1 | [MODERN] |
| R02 | §4.2（1553–1585） | `find_platform_sources` 用 `Box::leak` 取得 `'static` 切片；「泄漏可接受」 | 函数已改名 `find_platform_sources_into`，写入 `BootStorage` 的槽位，无 `Box::leak` | D1、D3 | P1 | [MODERN] |
| R03 | §3.3（1176–1179） | 「有多少根 IRQ 线」走 trait 方法 `nr_irqs()`，经 vtable，每次调用；`frequency()` 是通用能力 | `nr_irqs()` 与 `frequency()` 的生产调用点为 0；全部消费者 `as_any().downcast_ref()` 后读字段 | D1、D4 | P2 | [MODERN] |
| R04 | §2.6（783）、§3.3（1170） | `nr_irqs` 是「IRQ 源数量」能力 | x86 `ApicDesc.nr_irqs` 与 aarch64 `Gicv3Desc.nr_irqs` 都是常量 64（IDT/向量容量），只有 PLIC 是 DTB `riscv,ndev`；消费端统一 `min(NR_IRQ_VECTORS)` | D1、D7 | P2 | [MODERN] |
| R05 | §3.5（1289–1309）、§4.4（1705–1711）、§4.5（1890–1897） | 兜底只有显式两条分支；解析器报错即换源；C 对 0 地址 `assert` | MADT 无 IOAPIC 记录 → 静默 `ioapic_base = 0xFEC0_0000`；无 CPU 记录 → 伪造 `hw_id=0`（ACPI）/ 单核默认（DTB）；`PlicDesc.context` 恒 1 | D2、D3 | P2 | [MODERN] |
| R06 | §3.3（1137–1200）、§4.10（2317–2319） | 品牌名退到 arch 模块；`minix-boot` 的 handoff 层不出现硬件品牌名 | `CpuInfo.gicr_base` / `mtimecmp_addr` 与 `ArchMiscDesc.acpi_tables` / `pmu_cycle_counter` 都在 `minix-boot::platform`；前两者无生产读者，`pmu_cycle_counter` 恒 false；`CpuInfo` 允许 4 种组合 | D2、D4、D7 | P2 | [ARCH] |
| R07 | §4.8（2095–2103）、§3.4（1253） | 三个构造点、两处 downcast；`ArchInit` 直接收 `&ArchMiscDesc` 不 downcast；`static mut` 已被否决 | aarch64 的 `ArchInit` 实现经全局 `platform_desc()` 再 downcast `Gicv3Desc` 并种入 `static mut GICD_BASE`；GICD 基址存在两份表示（控制器实例 VA / SMP 静态 VA） | D1、D2、D4 | P2 | [MODERN] |
| R08 | §3.7（1385–1397） | `Frozen` 冻结后所有 CPU 共享不可变 `&T` | `unsafe impl<T: Send> Sync for Frozen<T>`；`T: !Sync` 时 `get()` 可把 `&T` 并发交给多 CPU | D7 | P2 | [MODERN] |
| R09 | §3.2（1112–1129）、§4.7（2064–2079） | 分派表只有 `PlatformDescEnum` 的 25 条；「新增来源只改一处」 | 另有 `PlatformContext` 的 8 个同构转发方法（第二张分派表）；`PLATFORM` 可直接是 `Frozen<PlatformDescEnum>` | D5、D4 | P3 | [DEL] |
| R10 | §4.8（2153） | 「每个 arch 一条注入路径」 | riscv64 除实例字段 `plic_va_base` 外，还有只写不读的 `INJECTED_PLIC_VA` 全局，构成第二条通道 | D4、D5 | P3 | [DEL] |
| R11 | §1.5（159）、§3.8（1451） | 早期控制台描述符有项但活路径不读；判据是「有协议报告机制」才进描述符 | ACPI x86 路径直接写死 COM1 `0x3F8`（不读 SPCR），aarch64 为 `None`；`ConsoleDesc` 无任何生产消费者 | D4、D1 | P3 | [DEL] |
| R12 | §4.10（2321） | `CpuTopology` 用 `[CpuInfo; MAX_CPUS]`，常量决定收集上限 | `nr_cpus: u32` 公开且无类型约束；`cpu_topology()` 按值拷贝约 2.5 KiB，7 处调用 | D2、D5 | P3 | [MODERN] |

七维分布：D1 出现在 R01/R02/R03/R04/R05/R07/R11；D2 在 R05/R06/R07/R12；D3 在 R01/R02/R05；D4 在 R03/R06/R07/R09/R10/R11；D5 在 R09/R10/R12；D6 在 R01；D7 在 R04/R06/R08。P1 两条，P2 六条，P3 四条。

---

## FINDING RD4-DESIGN-D2-01（P1，[MODERN]）

**一句话**：`PlatformDescSource` 的实际形状是 `(kind, len, phys_addr)` 三字段，`len` 还决定启动恒等映射窗口；§3.1 的「逐字引用」与 §3.6 的「一个 kind 标签加一个物理地址」都只描述了两字段。

**文档陈述**：

- §3.1（1055–1058）引 `os/libs/minix-boot/src/kernel_info.rs` 的文档注释为「Each `PlatformDescSource` is pure data `(u32 kind, u64 phys_addr)`」，并冠以「逐字引用」。
- §3.6（1359）：「切片元素是 `PlatformDescSource`：一个 kind 标签加一个物理地址」；表（1361–1365）把采用方案写成「不透明 `u32` 标签 + 物理地址」。
- §1.4（98–106）的「引导阶段只取地址（不解析）」例外清单只列了 DTB `/memory` 与 aarch64 `dtb_copy`，没有列头部探测。

**代码事实**：

- `os/libs/minix-boot/src/platform.rs:123-182`：`#[repr(C)] pub struct PlatformDescSource { kind, len: u32, phys_addr: PhysBytes }`，并带 `new`（`len=0`）、`with_len`、`with_optional_len`、`len()`、`has_len()`。字段注释明写「`len` is the blob extent in bytes …；`0` means the producer did not record a length and the consumer must fall back to the address's 2 MiB piece」。
- `os/libs/minix-boot/src/kernel_info.rs:132-138`：现行注释已是「pure data `(u32 kind, u32 len, u64 phys_addr)` … `len = 0` means the producer did not record the blob extent; the window builder then falls back to one 2 MiB piece」。§3.1 引的不是这句话。
- `os/kernel/src/lib.rs:1122-1143` `push_platform_windows`：`if source.has_len() { … source.len() … } else { windows.push_2m(source.phys_addr().0) }`——`len` 是启动窗口大小的真值输入。
- `os/libs/minix-boot/src/handoff_v1_write.rs:257-262`：线格式每个来源 16 字节 = `kind u32 + len u32 + phys_addr u64`；`handoff_v1_read.rs:317` 用 `with_len` 还原。
- 生产者探测头部：`os/boot-shim/src/uefi_helpers.rs:221-224 / 238-241 / 251-254` 与 `os/boot-shim/src/opensbi_helpers.rs:419-421`、`os/kernel-image/src/bootface_a64.rs:300-301` 都调用 `minix_boot::rsdp_total_size` / `dtb_total_size`；riscv 直核的 `os/kernel-image/src/bootface.rs:326-328` 同形（该文件不在 52 文件映射内，映射只收了 `bootface_a64.rs`）。

**为什么是问题**：交接物的形状是 §3.6 的核心设计（跨二进制纯数据）。两字段与三字段的差别不是排版：`len` 是 boot-shim 侧读固件头部的结果，直接推翻「只搬地址、不解析」的表述；`len=0` 时窗口退化为 2 MiB 片；这条窗口构造与 §4.5「DTB 可被直接解引用」同属启动地址窗口族，却没有任何章节交代。§3.1 的「逐字引用」实际已与代码脱钩，属于文档-代码同步失败，而不是观点差异。

**建议（最小改动）**：

1. 按现行 `kernel_info.rs:132-138` 重写 §3.1 引文，恢复「逐字」性质；
2. 在 §3.6 把交接物写成三元组并解释 `len=0` 的退化语义；
3. 在 §1.4 例外表补一行「boot-shim/bootface 读 DTB/RSDP 头部取 blob 长度」，并说明它只读长度、不做结构解析；
4. 在 §4.2 或 §4.5 加上 `push_platform_windows` 这个消费者，说明启动窗口按 `len` 定界。

**证据命令**：

```bash
$ rg -n "with_optional_len|has_len\(\)|source\.len\(\)" os/boot-shim os/kernel-image os/kernel/src/lib.rs
os/boot-shim/src/uefi_helpers.rs:224: … PlatformDescSource::with_optional_len(RSDP, pa, len));
os/kernel/src/lib.rs:1128:        if source.has_len() {
os/kernel/src/lib.rs:1133:                    .checked_add(source.len() as u64)
```

---

## FINDING RD4-DESIGN-D2-02（P1，[MODERN]）

**一句话**：§4.2 整段 `find_platform_sources` 摘录（含 `Box::leak` 与「泄漏可接受」的理由）描述的是已退役实现；现行函数是 `find_platform_sources_into`，写入 `BootStorage` 的定长槽位，泄漏路径已被显式移除。

**文档陈述**：

- §4.2（1553–1585）以「`os/boot-shim/src/uefi_helpers.rs:fn find_platform_sources —— 节选`」引出代码，末尾 `Box::leak(sources.into_inner().into_boxed_slice())`，并写「`Box::leak` 让切片获得 `'static`：这个函数在启动期只跑一次，泄漏是可接受的（注释里写明与 `build_memmap` 同一模式）」。
- §5.3（2484）仍以 `fn find_platform_sources` 作为「没有单测」的对象名。

**代码事实**：

- `os/boot-shim/src/uefi_helpers.rs:208`：`pub fn find_platform_sources_into(out: &mut [PlatformDescSource]) -> usize`；函数体用 `RefCell<Vec<…>>` 收集后逐条拷入 `out`，末尾带容量断言。
- 调用点 `os/boot-shim/src/uefi_helpers.rs:1035-1036`：`let platform_len = find_platform_sources_into(storage.platform_slots_mut()); storage.set_platform_len(platform_len);`——没有 `'static` 切片返回。
- `rg "Box::leak" os/boot-shim/src/uefi_helpers.rs` 无匹配；`os/boot-shim/src/opensbi_helpers.rs:406-409` 的注释明写「The loader writes straight into the storage's module slots; no `Box::leak` and no `'static` slice (R18 D-1 stage 2)」。
- 五个生产腿（UEFI x86、UEFI aarch64、U-Boot/OpenSBI riscv、riscv 直核 bootface、aarch64 直核 bootface）都经 `BootStorage` 槽位或 `storage.platform_slots_mut()[0]` 写入，再用 `with_optional_len` 带上探测到的长度。

**为什么是问题**：这是「逐字引用」级别的实现陈述错误，且理由（泄漏可接受）与现行设计相反：现设计正是为了消除堆泄漏面才改成定长槽位。读者按 §4.2 去仓库里找 `find_platform_sources` 会找不到，按「泄漏一次没关系」去扩展代码会走回被移除的路径。§5.3 的函数名也随之失效（测试缺口的对象应改成 `find_platform_sources_into`）。

**建议（最小改动）**：用 `find_platform_sources_into` 的实际签名与一个生产调用点（`uefi_helpers.rs:1035`；另有 10 余个测试内核直接调用）重写 §4.2 摘录；删掉 `Box::leak` 段与「泄漏可接受」句；把「为什么不再用 `'static` 切片」作为一句设计理由保留（R18 D-1 stage 2）；同步 §5.3 的函数名。

**证据命令**：

```bash
$ rg -n "fn find_platform_sources" os/
os/boot-shim/src/uefi_helpers.rs:208:pub fn find_platform_sources_into(out: &mut [PlatformDescSource]) -> usize {

$ rg -n "Box::leak" os/boot-shim/src/uefi_helpers.rs
(no match)
```

---

## FINDING RD4-DESIGN-D2-03（P2，[MODERN]）

**一句话**：§3.3 把 `nr_irqs()` 与 `frequency()` 描述为「上层经 vtable 调用的通用能力」，但这两个 trait 方法在生产代码里没有任何调用点；所有消费者都先 `as_any().downcast_ref()` 再读具体字段。

**文档陈述**：

- §3.3（1174–1179）分工表：「『有多少根 IRQ 线』→ trait 方法 `nr_irqs()`，经 vtable → 每次调用」；「『`lapic_base` 是多少』→ arch 层 `as_any().downcast_ref::<ApicDesc>()` → 构造实例时一次」。
- §3.3（1141–1161）三个子 trait 的签名把 `nr_irqs()` / `frequency()` 与 `as_any()` 并列，作为「接口只谈能力」的证据。

**代码事实**：

- `rg -n "nr_irqs\(\)" os/ --type rust` 只有一处命中，是 `os/libs/minix-boot/src/platform.rs:33` 的模块注释；没有任何调用表达式。
- `rg -n "\.frequency\(\)" os/arch os/kernel os/plat` 无匹配。
- 所有生产消费者直接 downcast 读字段：`os/plat/src/x86_64/interrupt.rs:461`（`ApicDesc`，随后读 `apic.nr_irqs`）、`os/plat/src/arm64/interrupt.rs:228`（`Gicv3Desc.gicr_base/gicr_stride/nr_irqs`）、`os/plat/src/riscv64/interrupt.rs:109`（`PlicDesc`）、`os/arch/src/x86_64/clock.rs:172`（`PitDesc.pit_base_freq`）、`os/arch/src/riscv64/clock.rs:114`（`ClintDesc` 四个字段）、`os/arch/src/arm64/clock.rs:81`（仅做类型校验）。
- 三个中断控制器实现（`ApicDesc` / `Gicv3Desc` / `PlicDesc`）的 `nr_irqs()` 都只在各自的 `impl` 里定义；`PlatformDescEnum` 只转发 `interrupt_controller()` 等子描述符获取方法，同样不调用 `nr_irqs()`。

**为什么是问题**：§3.3 的整个「品牌枚举 → 能力 trait」论证建立在一个事实上不成立的分工上。实际形态是「trait 只提供 `as_any()`，能力方法无人使用」；这意味着 `nr_irqs()` / `frequency()` 是两个只为文档存在的死方法，`ConsoleDesc` 更是只剩 `as_any()`。若将来有人按 §3.3 的分工去写新消费者（调用 `nr_irqs()`），行为会与现有实例字段路径分叉；若按代码现状写，则 §3.3 的分工表需要改写。

**建议（最小改动）**：

- 方案 A（推荐，恢复设计）：让三个中断控制器 `new()` 用 `desc.nr_irqs()` 取线数（只 downcast 取基址），让时钟 `new()` 用 `desc.frequency()` 取频率（只 downcast 取地址），使 §3.3 的表成为事实；
- 方案 B（承认现状）：删掉 `nr_irqs()` / `frequency()`，把两个 trait 改成显式的「`Any` 载体 + arch 字段」接口，并改写 §3.3 的分工表与「接口只谈能力」的结论。
- 两者选一，不要保留「文档说有、代码不用」的第三种状态。

**证据命令**：

```bash
$ rg -n "nr_irqs\(\)" os/ --type rust
os/libs/minix-boot/src/platform.rs:33://! lets the upper layer call generic methods (`nr_irqs()`) while the arch

$ rg -n "\.frequency\(\)" os/arch os/kernel os/plat
(no match)
```

---

## FINDING RD4-DESIGN-D2-04（P2，[MODERN]）

**一句话**：`nr_irqs` 在三套描述符里不是同一种量——x86 与 aarch64 存的是内核 IDT/向量表容量常量 64，riscv64 存的是 DTB 报出的外部中断源数量；§3.3 却把它的语义写成统一的「IRQ 源数量」。

**文档陈述**：

- §3.3（1141–1143）trait 注释：「`fn nr_irqs(&self) -> u32` — Number of IRQ sources supported.」
- §3.3（1170–1172）字段表把三架构的 `nr_irqs` 并列成同一列。
- §2.6（783）却写：「`nr_irqs` 是 IDT/IRQ 向量容量（常量 64），IOAPIC 引脚数由 `IOAPIC_NUM_PINS = 24` 常量给出」。

**代码事实**：

- `os/libs/minix-platform/src/arch/x86_64.rs:28-30`：`nr_irqs` 注释「Number of IRQ vectors supported (IDT capacity, not IOAPIC pins)」；ACPI 解析器 `os/libs/minix-platform/src/acpi.rs:701` 直接 `nr_irqs: 64, // IDT/IRQ vector capacity, not IOAPIC pins`。
- `os/libs/minix-platform/src/acpi.rs:275-280`（aarch64）：`Gicv3Desc { nr_irqs: 64 }`；`os/libs/minix-platform/src/device_tree.rs:276` 同为常量 64。
- `os/libs/minix-platform/src/device_tree.rs:228-233`（riscv64）：`nr_irqs` 取 `riscv,ndev`，缺失才回落 64——这是真正的「中断源数量」。
- 三个消费端（`os/plat/src/x86_64/interrupt.rs:464`、`os/plat/src/arm64/interrupt.rs:264`、`os/plat/src/riscv64/interrupt.rs:118`）统一做 `.min(NR_IRQ_VECTORS)`；x86 的 `mask_all` 再取 `.min(16)`（`os/plat/src/x86_64/interrupt.rs:487-493`）。

**为什么是问题**：同一个 trait 方法在三种实现上代表两个不同的硬件事实。今天没有 bug，是因为消费端都做了钳位；但任何「用 `nr_irqs` 判断 IOAPIC 有多少引脚 / GIC 有多少 SPI」的新消费者都会在 x86/aarch64 上拿到 64 这个内核表长，而不是硬件规模。这与 §3.8 的「描述符只放有协议报告机制的硬件参数」也冲突：64 是 `NR_IRQ_VECTORS` 的编译期值，不是固件报告。

**建议（最小改动）**：把两个概念分开命名与归属——描述符字段改名为 `irq_sources`（PLIC 用 `riscv,ndev`；x86/aarch64 若固件不报源数，就明确标为「内核向量容量」而不是硬件源数），内核表容量继续用 `minix_plat::NR_IRQ_VECTORS`，消费端不再把描述符值当作向量表长。若暂不拆，至少在 §3.3 的表里按架构注明「x86/aarch64 = 向量容量，riscv64 = 源数量」，并修正 trait 注释。

**证据命令**：

```bash
$ rg -n "nr_irqs: 64|nr_irqs: \(" os/libs/minix-platform/src/acpi.rs os/libs/minix-platform/src/arch/*.rs
os/libs/minix-platform/src/acpi.rs:701:        nr_irqs: 64, // IDT/IRQ vector capacity, not IOAPIC pins; real HW varies.
os/libs/minix-platform/src/acpi.rs:279:                nr_irqs: 64,
```

---

## FINDING RD4-DESIGN-D2-05（P2，[MODERN]）

**一句话**：解析器内部有四处「把缺失的固件报告替换成 QEMU/PC 常量」的静默兜底：MADT 无 IOAPIC 记录时 `ioapic_base = 0xFEC0_0000`、MADT 无 CPU 记录时伪造 `hw_id = 0`、DTB 无 CPU 节点时返回单核默认；另有 `PlicDesc.context` 恒为 1 的写法。它们与 §3.5 的「兜底只有显式两条分支」直接矛盾。

**文档陈述**：

- §3.5（1289–1309）：兜底是显式的两条分支（`platform_sources` 为空 / 全部解析失败），「兜底不是『给一组安全默认值』，而是『给一组特定机器的常量』」。
- §2.6（779）引 C 的 `acpi_get_ioapics`：「`assert(acpi_ioa->address)` 把『地址为 0 的 IOAPIC 记录』当作不该出现的输入，直接断言终止。」
- §4.4（1705–1711）列出的失败集合只有签名/长度/校验和，没有「缺 IOAPIC 记录」或「缺 CPU 记录」。
- §4.5（1890–1897）字段表只写 `riscv,ndev` 缺失 → 64、无 CPU 节点 → 单核默认，没有说明这是「伪造硬件事实」。

**代码事实**：

- `os/libs/minix-platform/src/acpi.rs:475-476`：`let mut ioapic_base = 0xFEC0_0000usize; // Default IOAPIC base (QEMU/PC).` 只有遇到 IOAPIC 记录才覆写；MADT 没有该记录时，`ApicDesc.ioapic_base` 仍是非零的 QEMU 值，`init_clock_and_interrupts` 的「描述符 IOAPIC 基址 == 设备窗口页」断言（`os/kernel/src/lib.rs:2456-2462`）会通过，内核继续编程一个固件从未报告过的 IOAPIC。
- `os/libs/minix-platform/src/acpi.rs:671-674`：`if nr_cpus == 0 { cpus[0] = CpuInfo { hw_id: 0, … }; nr_cpus = 1; }`——伪造一颗 hart 0。
- `os/libs/minix-platform/src/device_tree.rs:503-506`：无 CPU 节点时 `return CpuTopology::default();`，同样是一颗伪造的 CPU 0。
- `os/libs/minix-platform/src/device_tree.rs:235-237`：`let context = 1u32;`——QEMU virt 的 S-mode context 常量，§5.1（2392）自己承认「这是『断言现状』，不是从 DTB 发现出来的值」。

**为什么是问题**：这三处把「固件没报告」与「固件报告了这个值」压成同一个状态。对 IOAPIC，这正好是 §2.5 批评 C 的那类错误的反面：C 在 LAPIC 上把常量当答案；Rust 在 IOAPIC 上把 QEMU 常量当答案。对 CPU，伪造的 `hw_id=0` 会与真实的 hart 0 混淆，`SmpState` 的 `nr_cpus=1` 和 `bsp_id=0` 看起来像合法发现结果。设计上应让「缺失」可表达（`Option`/`Err`），而不是在解析器内部替补。

**建议（最小改动）**：

- `MadtResult.ioapic_base` 改为 `Option<usize>`，`ApicDesc` 增加「未报告」表达或让 `AcpiDesc::parse` 在 x86 上返回 `Err(AcpiParseError::IoapicNotFound)`（与 §2.6 的 C assert 对齐）；
- `parse_madt` / `parse_cpu_topology` 在 CPU 记录数为 0 时返回专用错误，而不是伪造 CPU 0；
- `PlicDesc.context` 至少在 §4.5 字段表里显式写成「QEMU virt 常量，非发现值」，或从 DTB 的 `interrupts-extended`/`riscv,ndev` 侧推导。

**证据命令**：

```bash
$ rg -n "0xFEC0_0000|Default IOAPIC|if nr_cpus == 0|CpuTopology::default|let context = 1" os/libs/minix-platform/src/acpi.rs os/libs/minix-platform/src/device_tree.rs
os/libs/minix-platform/src/acpi.rs:476:    let mut ioapic_base = 0xFEC0_0000usize; // Default IOAPIC base (QEMU/PC).
os/libs/minix-platform/src/acpi.rs:671:    if nr_cpus == 0 {
os/libs/minix-platform/src/device_tree.rs:503:        if nr_cpus == 0 {
os/libs/minix-platform/src/device_tree.rs:237:        let context = 1u32;
```

---

## FINDING RD4-DESIGN-D2-06（P2，[ARCH]）

**一句话**：`CpuInfo` 与 `ArchMiscDesc` 是 handoff 层里残留的「按架构取并集」结构：`CpuInfo` 带 `gicr_base` / `mtimecmp_addr` 两个品牌字段（无生产读者；两个 `Option` 的 4 种组合中，每个架构只允许 1 种，类型却不区分架构），`ArchMiscDesc` 带 x86 的 `acpi_tables` 与 ARM 的 `pmu_cycle_counter`（后者所有生产者都填 false）。这与 §3.3「品牌名退到 arch 模块」以及 `minix-boot/src/platform.rs` 自己「Brand names … never appear here」的模块注释都冲突。

**文档陈述**：

- §3.3（1137–1200）：上一版把品牌名写进公共 API，改造后「品牌名 struct 退到 `minix-platform/src/arch/<arch>.rs`」；§3.3 明确被 trait 化的是描述符内部的三个品牌枚举；「来源」层仍是枚举。
- §4.10（2317–2319）把「`CpuInfo` 的 per-CPU 私有地址」列为 Rust 侧的「新增」项，没有讨论它是否把品牌名带回 handoff 层。
- `os/libs/minix-boot/src/platform.rs:1-6` 模块文档：「Brand names like `Apic`, `Gicv3`, `Plic`, `DeviceTree`, `Acpi` never appear here」。

**代码事实**：

- `os/libs/minix-boot/src/platform.rs:294-308`：`pub struct CpuInfo { pub hw_id: u64, pub gicr_base: Option<usize>, pub mtimecmp_addr: Option<usize> }`——`gicr` 与 `mtimecmp` 都是具体设备名。
- `os/libs/minix-boot/src/platform.rs:325-332`：`pub struct ArchMiscDesc { pub acpi_tables: Option<usize>, pub pmu_cycle_counter: bool }`——一个 x86 字段、一个 ARM 字段同处一个「架构杂项」结构。
- 无生产读者（本文件范围）：`rg -n "gicr_base|mtimecmp_addr" os/` 除去 `minix-platform` 自身后只剩 `os/plat/src/arm64/interrupt.rs:86-127` 的注释与它自己的 `gicr_base_for` 公式；`rg -n "cpu_topology\(\)"` 的 7 个调用点只读 `nr_cpus` / `hw_id` / `bsp_id`。riscv64 时钟的 per-hart 比较器地址由 `ClintDesc` 的 base/stride 现算（`os/arch/src/riscv64/clock.rs:112-120`、`mtimecmp_base + hart*stride`），aarch64 的 redistributor 地址由 `Gicv3Desc` 的 base/stride 现算（`os/plat/src/arm64/interrupt.rs:126-127`）。
- `pmu_cycle_counter` 的四个生产者全部写 `false`：`os/libs/minix-platform/src/arch/{x86_64,aarch64,riscv64}.rs` 的 QemuVirtDesc 与 `os/libs/minix-platform/src/acpi.rs:245`；`AArch64ArchInit` 的 `if self.pmu_cycle_counter` 分支（实现位于 `os/arch/src/arm64/arch_init.rs`）因此永远不执行。
- 非法状态：两个 `Option` 相互独立，可同时为 `Some`（无任何架构同时需要 GICR 与 mtimecmp），也可在 riscv64 上出现 `gicr_base: Some`；类型不排除这些组合。
- 体积代价：`CpuInfo` 为 40 字节（`u64` + 两个 16 字节 `Option<usize>`），`CpuTopology` 为 8 + 64×40 = 2568 字节；`cpu_topology()` 按值返回且 `CpuTopology: Copy`，7 个调用点每次都整块拷贝。

**为什么是问题**：§3.3 的改造目标是把「品牌差异」从公共 API 赶到 arch 模块，但 `CpuInfo` / `ArchMiscDesc` 是从 handoff 层（`minix-boot`）直接暴露的字段级并集，恰好是 §3.3 要消除的形态——只是从 enum 变体换成了字段。两个字段当前都没有消费者，说明它们不是被需求拉进来的，而是被「SMP 预备」这一未来需求提前放进去的；以当前状态它们只贡献非法状态和 2.5 KiB 的拷贝开销。架构层面的处置见 `## ARCH-PROPOSAL`。

**证据命令**：

```bash
$ rg -n "CpuInfo|ArchMiscDesc" os/libs/minix-boot/src/platform.rs
294:pub struct CpuInfo {
304:    pub gicr_base: Option<usize>,
307:    pub mtimecmp_addr: Option<usize>,
327:pub struct ArchMiscDesc {
329:    pub acpi_tables: Option<usize>,
331:    pub pmu_cycle_counter: bool,

$ rg -n "pmu_cycle_counter:" os/libs/minix-platform/src
os/libs/minix-platform/src/arch/riscv64.rs:140:                pmu_cycle_counter: false,
os/libs/minix-platform/src/arch/x86_64.rs:109:                pmu_cycle_counter: false,
os/libs/minix-platform/src/arch/aarch64.rs:144:                pmu_cycle_counter: false,
os/libs/minix-platform/src/acpi.rs:245:            pmu_cycle_counter: false,
```

---

## FINDING RD4-DESIGN-D2-07（P2，[MODERN]）

**一句话**：§4.8 把消费侧描述成「三个构造点、两处 downcast，`ArchInit` 直接收 `&ArchMiscDesc` 不 downcast」；aarch64 的实现却在 `ArchInit::init()` 里重新经全局 `platform_desc()` 取中断控制器、再 downcast `Gicv3Desc`，并把 GICD 基址种进 `static mut GICD_BASE`——这是三个构造点之外多出的一个 downcast 点，也是 §3.4 明确否决过的 `static mut` 形态，且同一 GICD 基址存在两份表示（控制器实例的 VA / SMP 静态的 VA）。

**文档陈述**：

- §4.8（2095–2103）：「三个构造点里有两处做 downcast（`ArchInit` 直接收 `&ArchMiscDesc`，见下表）」；表里 `ArchInit` 一行的「搬运的字段」是「各架构的 arch-misc 参数（`&ArchMiscDesc` 直接传，不 downcast）」。
- §4.1（1546）：「『必须早于谁』的确切名单就是这三个构造点：`CurrentInterruptController::new` 与 `CurrentArchInit::new`（`init_clock_and_interrupts` 内）、`CurrentClockArch::new`（后续 `boot_init_timer` 内）。」
- §3.4（1253–1265）：`static mut` 被列为被否决的形态之一（「每次访问包 `unsafe`，且『启动期写一次』的约定没有任何检查」）。

**代码事实**：

- `os/arch/src/arm64/arch_init.rs:89-97`：`let controller = minix_platform::platform_desc().interrupt_controller(); if let Some(gicv3) = controller.as_any().downcast_ref::<Gicv3Desc>() { unsafe { crate::arm64::smp::set_gicd_base(gicv3.gicd_base) }; }`。这个文件不在 `doc-code-map` 的 52 文件里（`tools/doc-code-map.sh` 只产出 `os/arch/src/arch/arch_init.rs` 与 `os/arch/src/x86_64/arch_init.rs`），属于 §4.8 的覆盖缺口。
- `os/arch/src/arm64/smp.rs:196`：`static mut GICD_BASE: usize = 0;`；`set_gicd_base`（213–225）把物理基址先 `mmio_translate` 成高半 VA 再存入；`gicd_base()`（235–245）每次 `unsafe` 读并断言非零。
- `os/plat/src/arm64/interrupt.rs:228-270` 的 `new()` 已经把 `Gicv3Desc.gicd_base` 经 `mmio_translate` 存进控制器实例字段 `gicd_base`；同一物理基址因此有「控制器实例 VA」与「SMP 静态 VA」两份。
- `os/kernel/src/lib.rs:2407-2520`：`init_clock_and_interrupts` 构造 `CurrentInterruptController::new(pd.interrupt_controller())` 后把实例移入 `IRQ_MANAGER`；`CurrentArchInit::new(&pd.arch_misc())` 的 `init()` 再走上面的全局路径。

**为什么是问题**：这是设计陈述与实现的直接冲突，同时引入重复真值源。`ArchInit` 的实例字段（`ArchMiscDesc`）并不包含 GICD 基址，所以实现只能回到全局描述符；这既让「三个构造点」的枚举不完整，也让 `ArchInit` 从「收 arch-misc 的 phase trait」变成「中断控制器描述符的第二个消费者」。`static mut` 的存在还让 §3.4 对全局变量形态的批评在本文范围内失效——SGI 热路径读的是一个未受 `Frozen` 协议保护的 `static mut`。

**建议（最小改动）**：

1. `GICD_BASE` 改为 `AtomicUsize`（与 `os/plat/src/x86_64/interrupt.rs:226` 的 `LAPIC_VA` 同形）或 `Frozen<usize>`，消除 `unsafe fn gicd_base()`；
2. 把「aarch64 `ArchInit::init` 读中断描述符并 seed SGI 静态」写进 §4.8 的消费表和 §4.1 的时序名单（它是 `init_clock_and_interrupts` 内部的第四个读取点）；
3. 若要消掉两份 VA，把 seed 动作移到控制器实例拥有 GICD VA 之后（例如由 `init_clock_and_interrupts` 在 `intr.init()` 后调用一个受控注入函数），而不是让 `ArchInit` 重新 downcast。

**证据命令**：

```bash
$ rg -n "static mut GICD_BASE|fn set_gicd_base|fn gicd_base" os/arch/src/arm64/smp.rs
196:static mut GICD_BASE: usize = 0;
213:pub(crate) unsafe fn set_gicd_base(base: usize) {
235:unsafe fn gicd_base() -> usize {

$ tools/doc-code-map.sh rewrite-notes/01-stage-kernel/04-platform-discovery.md --format=plain | rg arch_init
os/arch/src/arch/arch_init.rs
os/arch/src/x86_64/arch_init.rs
```

---

## FINDING RD4-DESIGN-D2-08（P2，[MODERN]）

**一句话**：§3.7 把 `Frozen` 表述为「冻结后所有 CPU 共享不可变 `&T`」的安全协议，但类型只要求 `T: Send` 就 `unsafe impl Sync`；当 `T: !Sync` 时，`get()` 可以把 `&T` 并发交给多个 CPU，`Frozen` 自己的安全契约并不成立。

**文档陈述**：

- §3.7（1385–1397）逐字引用 `Frozen` 的 Safety contract：「freeze happens-before any `get` … all later readers share the same immutable `&T`」。
- §3.7（1433–1435）：`Frozen` 替换 `AssumeSyncCell` 的理由是「AP 上线后 `platform_desc()` 会被多个 CPU 并发调用」，即读侧是多 CPU 并发共享。

**代码事实**：

- `os/libs/minix-types/src/types/cell.rs:208-213`：`pub struct Frozen<T> { value: UnsafeCell<MaybeUninit<T>>, frozen: AtomicBool }`；紧接的 `unsafe impl<T: Send> Sync for Frozen<T> {}`（约 213 行）只要求 `Send`。
- `get()`（247–258）在 `frozen` 为真后返回 `&T`；两个 CPU 同时 `get()` 得到两个 `&T`，若 `T` 不是 `Sync`，通过 `&T` 访问内部可变状态就是数据竞争（例如 `Frozen<Cell<u32>>`：`Cell` 是 `Send` 但不是 `Sync`，`Frozen<Cell<u32>>` 却会被判为 `Sync`）。
- 当前唯一实例 `Frozen<PlatformContext>` 的 `T: Sync`，所以今天没有可利用路径；这是公共原语的契约缺陷，不是现存触发点。

**为什么是问题**：`Frozen` 的设计目标就是替代「调用方自己保证」的 `AssumeSyncCell`，把协议写进类型；但 `Sync` 的 bound 漏了 `T: Sync`，等于把「共享 `&T`」的证明责任又退回到调用方。文档把它当项目级 boot-frozen 原语介绍，后续任何 `Frozen<非 Sync T>` 的使用都会重新引入 §3.7 想消除的那类隐患。

**建议（最小改动）**：`unsafe impl<T: Send + Sync> Sync for Frozen<T> {}`（或把 `Sync` 的 bound 单独写成 `T: Sync`，`Send` 由自动派生处理）；补一条编译期负向断言（`static_assertions` 风格的 `assert_not_impl` 或 `trybuild`，若宿主测试形态允许）。文档 §3.7 的 Safety contract 同步写明 `T: Send + Sync`。

**证据命令**：

```bash
$ rg -n "unsafe impl<T: Send> Sync for Frozen|pub fn get\(&self\)" os/libs/minix-types/src/types/cell.rs
213:unsafe impl<T: Send> Sync for Frozen<T> {}
247:    pub fn get(&self) -> &T {
```

---

## FINDING RD4-DESIGN-D2-09（P3，[DEL]，SUGGESTION）

**一句话**：`PlatformContext` 是一个只有 `desc` 一个字段的壳，`impl PlatformDesc for PlatformContext` 的 8 个方法全是同构转发；`PLATFORM` 完全可以直接是 `Frozen<PlatformDescEnum>`，删掉这层后第二张分派表消失。

**文档陈述**：

- §3.2（1112–1128）三层职责表把「分派（每个方法的转发）」写成 `impl PlatformDesc for PlatformDescEnum` 的 8 个转发（22 条 `Self::…`）+ `Debug`（3 条）= 25 条，并强调「维护方只有一处分支不成立——`PlatformDescEnum` 自己就是那张 25 条的分派表」。
- §4.7（2064–2079）：「读取侧是 `PlatformContext` 的八次转发」，代码摘录只展示前两个方法和 `source()`。

**代码事实**：

- `os/libs/minix-platform/src/global.rs:51`：`static PLATFORM: Frozen<PlatformContext> = Frozen::new();`
- `global.rs:57-60`：`pub struct PlatformContext { pub desc: PlatformDescEnum }`——唯一字段。
- `global.rs:62-87`：`impl PlatformDesc for PlatformContext` 的 8 个方法全部是 `self.desc.<method>()`。
- `global.rs:132-203`：`impl PlatformDesc for PlatformDescEnum` 再写一遍同样的 8 个方法（22 条变体臂）；`global.rs:120-130` 另有 `impl Debug` 的 3 条 `Self::…`，两部分合计 25 条。
- 计数：`rg -n "fn (interrupt_controller|timer|early_console|cpu_topology|arch_misc|source|sswi_setip_base|psci_conduit)" os/libs/minix-platform/src/global.rs | wc -l` 输出 `16`（8 + 8）。
- `PlatformDescEnum` 自身已实现 `PlatformDesc` 与 `Debug`，且只含普通数据；`platform_desc()`（295–300）返回 `&'static dyn PlatformDesc`，由 `&PlatformDescEnum` 直接强制转换即可。

**为什么是问题（成本而非错误）**：这层壳不携带额外状态，也不改变可见性；它把「新增一个 trait 方法」从改一处变成改两处，把「分派表」从一张变成两张，并让每次读取多一次间接调用。§3.2 的「25 条」统计因此只覆盖了下半张表，读者会低估维护面。

**建议（SUGGESTION）**：把 `static PLATFORM` 改为 `Frozen<PlatformDescEnum>`，删除 `PlatformContext` 及 `impl PlatformDesc for PlatformContext`；`platform_desc()` 的返回类型不变。若保留 `PlatformContext` 是为了将来放第二个字段，应在 §3.2/§4.7 写明该字段是什么；当前没有。

**证据命令**：

```bash
$ rg -n "fn (interrupt_controller|timer|early_console|cpu_topology|arch_misc|source|sswi_setip_base|psci_conduit)" os/libs/minix-platform/src/global.rs | wc -l
16
```

---

## FINDING RD4-DESIGN-D2-10（P3，[DEL]，SUGGESTION）

**一句话**：§4.8 说「每个 arch 一条注入路径」，riscv64 实际有两条——控制器实例的 `plic_va_base`，以及 `irq_manager::INJECTED_PLIC_VA` 这个只写不读的诊断全局。

**文档陈述**：

- §4.8（2153）地址表：riscv64「PLIC 实例存 `plic_base` 与 `plic_va_base`；内核 check-and-reject 后注入高半 VA → PLIC 走高半 VA」。
- §4.8（2157）：「三个零基址/零 stride 断言」与「地址三语义」的讨论只覆盖描述符/实例/MMIO 操作数，没有第二个全局副本。

**代码事实**：

- `os/kernel/src/lib.rs:2494`：`crate::irq_manager::set_injected_plic_va(plic_va);`
- `os/kernel/src/irq_manager.rs:312-319`（该文件不在 52 文件映射内，写点 `lib.rs:2494` 在映射内）：`pub static INJECTED_PLIC_VA: AtomicUsize` 与 `set_injected_plic_va`；`rg -n "INJECTED_PLIC_VA|set_injected_plic_va" os/ --type rust` 只有这两处定义/写入，没有任何读取点——注释写「diagnostics read it on the fault path」，但故障路径没有 reader。
- 实例侧：`os/plat/src/riscv64/interrupt.rs:64/92/286-288` 的 `plic_va_base` + `set_plic_va_base` 才是 PLIC MMIO 实际使用的地址。

**为什么是问题**：写后不读的全局是纯债务；它让「注入路径」在文档里少一条、在代码里多一条，而且注释声称的消费者不存在，下一次排障会误以为可以从 `INJECTED_PLIC_VA` 取到值。P3 建议级，因为不影响当前行为。

**建议（SUGGESTION）**：删除 `INJECTED_PLIC_VA` 与 `set_injected_plic_va`（连同 `lib.rs:2494` 调用）；若确实要保留故障诊断，给出读取它的 reader（例如 panic 诊断打印），否则不要留写点。§4.8 保持「一条注入路径」的表述即可。

**证据命令**：

```bash
$ rg -n "INJECTED_PLIC_VA|set_injected_plic_va" os/ --type rust
os/kernel/src/lib.rs:2494:        crate::irq_manager::set_injected_plic_va(plic_va);
os/kernel/src/irq_manager.rs:312:pub static INJECTED_PLIC_VA: core::sync::atomic::AtomicUsize =
os/kernel/src/irq_manager.rs:317:pub fn set_injected_plic_va(va: usize) {
os/kernel/src/irq_manager.rs:318:    INJECTED_PLIC_VA.store(va, core::sync::atomic::Ordering::Relaxed);
```

---

## FINDING RD4-DESIGN-D2-11（P3，[DEL]，SUGGESTION）

**一句话**：`ConsoleDesc` / `early_console()` 在本文范围内没有任何生产消费者（文档自己也承认），但 ACPI 路径仍然把 ISA 常量 COM1 `0x3F8` 放进描述符，违反了 §3.8 自己给出的「有协议报告机制才进描述符」判据。

**文档陈述**：

- §1.5（159）：「Rust 侧的描述符里有一个『早期控制台』方法、三种来源的实现都写了它，但本篇检索范围内没有活消费者——早期控制台实际走各架构自己的模块。」
- §3.8（1451）：「早期控制台基址 | 有（ACPI 的 SPCR 表；设备树里的串口节点）| 描述符里有这一项（`ConsoleDesc`），但活路径不读它」。
- §3.8（1453）：「判据是『有没有协议报告机制』，不是『值会不会变』。」

**代码事实**：

- `rg -n "early_console\(\)" os/ --type rust | grep -v "minix-platform\|minix-boot"` 无匹配；唯一涉及的是 `os/libs/minix-platform/src/global.rs:319` 的单测绑定。
- ACPI x86：`os/libs/minix-platform/src/acpi.rs:264-266` 直接 `let console = Some(IsaSerialDesc { port_base: 0x3F8 });`——没有读 SPCR（代码里也没有 SPCR 解析器）。
- ACPI aarch64：`acpi.rs:316-322` 返回 `None`，注释「early console is a hardcoded PL011 in minix-plat」。
- DTB 路径确实按协议读节点：`device_tree.rs:418-440` 匹配 `ns16550a`/`sifive,uart0`/`arm,pl011`。
- 内核活路径：`os/kernel/src/lib.rs:1735` 的 `Console::init()` 使用 `minix_plat::EarlyConsole` 的架构实现，不读描述符。

**为什么是问题**：三种来源里只有一种真的按协议填值；x86 的 `IsaSerialDesc { port_base: 0x3F8 }` 是 §3.8 判据下的「架构常量」，却被放进描述符，恰好是 §3.8 警告的「把 ISA 字面量也搬进描述符」。因为当前无消费者，这不是 bug，而是设计一致性债务：要么删掉整个 `ConsoleDesc` 面直到有消费者，要么实现 SPCR 并让活路径读描述符。

**建议（SUGGESTION）**：短期内把 `early_console()` 从 `PlatformDesc` 删除（连带 `ConsoleDesc`、`IsaSerialDesc`、`MmioSerialDesc`、`Riscv64ConsoleDesc` 与 `parse_console` 的返回值），等 05 篇或控制台篇出现真实消费者时再加回；若保留，必须让 ACPI x86 路径读 SPCR（否则 §3.8 的判据表要改成两套：DTB 走协议、ACPI 走常量，并说明为什么）。

**证据命令**：

```bash
$ rg -n "early_console\(\)" os/ --type rust | grep -v "minix-platform\|minix-boot"
(no external consumer)

$ sed -n "264,266p" os/libs/minix-platform/src/acpi.rs
            // Early console: COM1 (0x3F8) — standard PC AT serial port.
            let console = Some(IsaSerialDesc { port_base: 0x3F8 });
```

---

## FINDING RD4-DESIGN-D2-12（P3，[MODERN]，SUGGESTION）

**一句话**：`CpuTopology` 的 `nr_cpus: u32` 与公开的 `cpus: [CpuInfo; MAX_CPUS]` 之间没有类型约束（`nr_cpus > 64` 可表示），且 `cpu_topology()` 按值返回 2.5 KiB 结构，7 个调用点各自整块拷贝。

**文档陈述**：

- §4.10（2321）：「`CpuTopology` 用 `[CpuInfo; MAX_CPUS]` 数组，`MAX_CPUS = 64` … 这个常量同时决定 `parse_cpu_topology` 与 `parse_madt` 的 CPU 收集上限。」
- §3.2（1086）trait 签名：`fn cpu_topology(&self) -> CpuTopology;`（按值返回）。

**代码事实**：

- `os/libs/minix-boot/src/platform.rs:268-282`：`pub struct CpuTopology { pub nr_cpus: u32, pub bsp_id: u32, pub cpus: [CpuInfo; MAX_CPUS] }`，`#[derive(Debug, Clone, Copy)]`；字段全公开。
- 生产者目前都做了钳位（`acpi.rs:239` 的 `.min(MAX_CPUS)`、`device_tree.rs:458` 的 `>= MAX_CPUS` break），但类型本身不阻止 `nr_cpus = 100` 的实例；消费者按 `0..topo.nr_cpus` 索引 `cpus`（`os/kernel/src/lib.rs:1965-1970`、`os/kernel/src/smp.rs:1126-1131` 等 7 处）。
- 体积：`CpuInfo` = 8 + 2×16 = 40 字节，`CpuTopology` = 8 + 64×40 = 2568 字节；`cpu_topology()` 每次调用按值返回并拷贝这份数组。

**为什么是问题**：这是「非法状态可表示」的典型（`nr_cpus` 与数组长度的关系靠生产者自律），也是最小化机会（把不变式收进构造器/访问器，或让 `nr_cpus` 只读、`cpus` 私有）。按值返回 2.5 KiB 在启动路径不是性能灾难，但与「描述符是只读全局」的设计意图相悖——本可以返回 `&CpuTopology`。

**建议（SUGGESTION）**：`CpuTopology::new(nr_cpus, bsp_id, cpus) -> Result/Option` 强制 `nr_cpus <= MAX_CPUS`，字段改私有并提供 `nr_cpus()` / `cpu(i)`；`PlatformDesc::cpu_topology()` 改为返回 `&CpuTopology`（三个实现都返回实例字段的引用，`PlatformDescEnum` 的 match 返回 `&`）。若暂不改签名，至少在 §4.10 写明该不变式与「按值返回是有意为之/待改」。

**证据命令**：

```bash
$ rg -n "pub nr_cpus|pub cpus" os/libs/minix-boot/src/platform.rs
272:    pub nr_cpus: u32,
281:    pub cpus: [CpuInfo; MAX_CPUS],

$ rg -n "cpu_topology\(\)" os/kernel/src os/plat/src os/arch/src --type rust | wc -l
7
```

---

## ARCH-PROPOSAL

（对应 FINDING RD4-DESIGN-D2-06；建议标注 `[ARCH: handoff-cpu-info]`，三处一致：本文档 §3.3/§4.5/§4.10 + `os/libs/minix-boot/src/platform.rs` 的 `CpuInfo`/`ArchMiscDesc` 文档 + `os/arch/src/arch/arch_init.rs` 的 `ArchInit` 文档。）

### 依据

1. §3.3 的改造目标是「品牌名退到 `minix-platform/src/arch/<arch>.rs`，handoff 层只谈能力」；`CpuInfo` 的 `gicr_base`/`mtimecmp_addr` 与 `ArchMiscDesc` 的 `acpi_tables`/`pmu_cycle_counter` 是同一类品牌/架构字段留在了 `minix-boot` 的公共 API 上。
2. 这些字段当前没有生产读者：`gicr_base` 由 `os/plat/src/arm64/interrupt.rs:126-127` 用 `Gicv3Desc` 的 base/stride 现算，`mtimecmp_addr` 由 `os/arch/src/riscv64/clock.rs` 用 `ClintDesc` 的 base/stride 现算；`pmu_cycle_counter` 的四个生产者全部写 false；`acpi_tables` 的 x86 `ArchInit` 消费者字段被 `#[allow(dead_code)]` 标注，唯一真实消费者是 `os/kernel/src/misc.rs:1131-1138` 的 `GET_MACHINE` 发布。
3. 类型允许非法组合（两个 `Option` 可同时为 `Some`，也可在错误架构上为 `Some`），且 `CpuTopology` 每次按值拷贝 2568 字节。

### 提案（采用 C）

**C1. `CpuInfo` 收缩为 `{ hw_id: u64 }`。** per-CPU 私有地址由各 arch 消费者从自己的描述符推导（现有代码已经是这么做的，见依据 2）；`CpuTopology` 从 2568 字节降到 8 + 64×8 = 520 字节，`Copy` 代价同步下降。

**C2. 删除 `ArchMiscDesc.pmu_cycle_counter`。** 没有生产者能把它置 true；`AArch64ArchInit` 的 `if self.pmu_cycle_counter` 分支随之删除。等 PMU 真正接入时再加回，并按 C1 的方式放进 arch 模块的构造参数。

**C3. 保留 `ArchMiscDesc.acpi_tables`，但在文档里重新定性。** 它不是「平台能力」，而是 C `machine.acpi_rsdp` 的发布桥（`GET_MACHINE` 的用户态 ACPI 驱动读它）；§3.4/§4.8/§4.10 应写明这是 handoff 层唯一获准出现的固件格式字段，理由是与 C 的跨特权级契约同形，而不是描述符能力。若不愿保留，替代方案是 `PlatformDesc::acpi_rsdp(&self) -> Option<PhysBytes>`（默认 `None`，仅 `AcpiDesc` 覆盖），`getinfo_machine` 改调它；两种都可，C3 选前者是因为它不动 `ArchInit::new` 的签名。

### 备选与否决

| 方案 | 做法 | 否决理由 |
|---|---|---|
| A（现状） | 保留 `CpuInfo` 双 Option + `pmu_cycle_counter` | 非法状态与死字段留在 handoff 层；§3.3 的「品牌名不进公共 API」无法自洽 |
| B（全泛型） | `CpuInfo<Arch>` / 关联类型 | `no_std` + 静态全局 + 三份 per-arch 文件已经用 `#[cfg]` 解决差异；泛型化会把 52 文件里的类型链全部改写，收益不抵成本 |
| **C（采用）** | C1 + C2 + C3 | 不改 `KernelInfo` 线格式、不改 `PlatformDesc` 的 8 方法形状；只删字段与一段死分支 |

### 代价与迁移

- 改 4 个生产者（`os/libs/minix-platform/src/arch/{x86_64,aarch64,riscv64}.rs` 的 `QemuVirtDesc`、`os/libs/minix-platform/src/{acpi,device_tree}.rs` 的 `CpuInfo` 构造）与 2 个消费者（`os/arch/src/arm64/arch_init.rs`、`os/kernel/src/misc.rs`），以及相应单测的字段断言。
- `CpuTopology` 不进入 handoff 线格式（`handoff_v1_write.rs`/`handoff_v1_read.rs` 无 `CpuInfo`），所以不涉及跨二进制 ABI；`KernelInfo` 不变。
- 代价：`CpuInfo` 少两个字段后，未来若真需要「每 CPU 私有地址」的跨架构统一入口，需要在 arch 层新增（现有 `gicr_base_for` / `mtimecmp_base + hart*stride` 已经是那个入口的雏形）。这是把「现在没有消费者」的字段推迟到有消费者时再引入，而不是永久放弃。

### 不并入本提案的相邻项

- `PlatformDescSource.len`（F01）是 handoff 线格式字段，已有真实消费者，不动；
- `CpuTopology.nr_cpus` 不变式与按值返回（F12）可在 C1 之后作为独立小改，不需 ARCH 标注。

---

## VERDICT

**设计轴结论：NOT PASS（D04 R1 / P-DESIGN-D2）**。

- 计数：P0 = 0；P1 = 2（F01、F02）；P2 = 6（F03、F04、F05、F06、F07、F08）；P3 = 4（F09、F10、F11、F12）。F06 为 `[ARCH]`，提案已写在 `## ARCH-PROPOSAL`，按 D-047 需协调者裁决后才能进入修复。
- 阻断项（P1）都是「设计章对实现的陈述已失效」，不是代码行为错误：
  - F01：`PlatformDescSource` 的 `len` 字段、它的线格式与启动窗口消费者完全缺席 §3.1/§3.6，且 §3.1 的「逐字引用」与 `kernel_info.rs:132-138` 现行注释不一致；
  - F02：§4.2 的 `find_platform_sources`/`Box::leak` 摘录与「泄漏可接受」的理由描述的是已退役实现，现行是 `find_platform_sources_into` + `BootStorage` 槽位。
- 两条 P1 同根（handoff 层的真实形状与传输机制在写作后发生过一次改造：三字段 + 定长槽位），建议同一轮一起修；修完这两条，§3.1/§3.6/§4.2 才能作为实现契约使用。
- P2 中 F05（解析器静默常量）最接近语义风险：在「MADT 无 IOAPIC 记录」的机器上，内核会把 `0xFEC0_0000` 当作固件报告值并通过消费端断言，行为与 C 的「找不到 IOAPIC 就退回 8259」不同；若协调者按行为偏差定级，可上调为 P1。
- P2 中 F03/F04（trait 能力方法无调用点、`nr_irqs` 语义分叉）、F07（构造点之外多出的 downcast + `static mut`）、F08（`Frozen` 的 `Sync` bound）都是「类型/契约层」问题，不影响当前三架构真机门，但会被后续消费者继承。
- P3 四条为最小化/死抽象建议：`PlatformContext` 壳、`INJECTED_PLIC_VA`、`ConsoleDesc` 无消费者、`CpuTopology` 不变式与按值拷贝。

**同 agent 验证声明**：本报告所有「代码事实」均由本会话内实际命令输出支撑（命令与输出已逐条抄录）；`rg` 计数与 `doc-code-map` 输出各复跑一次。计数复核：`Self::(DeviceTree|Acpi|QemuVirt)` = 25（与 §3.2 一致）；`#[test]` = 26（与 §5.1 一致）；52 文件映射 = 52 行（`tools/doc-code-map.sh … --format=plain`，`--check` 通过）。**未做门禁级跨工具只读复核**，consistency 按本面板口径给 0.86（两条 P1 为硬冲突，其余为字段/语义级差异），不宣称 CONVERGED。

**范围外观测（不计入 FINDING）**：

1. §5.3 / §6.2 的 `platform_sources: &[]` 计数 53 已漂移到 54：`os/kernel/src/vm_handoff.rs` 现有 3 处（文档表写 1 处），`rg -c` 可复现。该数字在第 5 章，超出本面板 3/4 章范围，但同一文档内已不一致。
2. `doc-code-map` 的 52 文件不含 `os/arch/src/arm64/arch_init.rs`、`os/kernel-image/src/bootface.rs` 与 `os/kernel/src/irq_manager.rs`，而 §4.8 的 `ArchInit` aarch64 实现、riscv 直核的来源装着、以及 PLIC VA 诊断全局分别落在这些文件里（F07/F10/F01 的证据来源）。这是文件映射缺口（D6）：它们都是对应设计陈述的直接承载者，建议下一轮补进映射或在正文中明确引用；本报告在引用处已逐个标注。
3. `trait-audit.md` 对 `PlatformDesc` 记 7 实现（含测试实现）、`InterruptControllerDesc`/`TimerDesc`/`ConsoleDesc` 各 3–4 实现；与 §3.3 的「三来源各一实现」不冲突，但说明「实现数」统计里混入了测试桩，若 §3.3 要引用计数需注明口径。
