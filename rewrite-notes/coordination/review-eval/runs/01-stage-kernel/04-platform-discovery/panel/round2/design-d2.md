# 04-platform-discovery R2 确认轮 — P-DESIGN-D2（设计轴）

- 受检文档：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`
- 基线：2708 行，sha256 前缀 `773c10f1`（全文 `773c10f1bf69de6f3604a690bc97850282b7077705ac760d8b493087d56675e3`）
- 对照面：`doc-code-map` 抽出的 52 个 Rust 文件、`trait-audit.md`、`AGENTS.md` Key Constraints、R1 冻结决策清单
- 本轮性质：只读确认；未修改任何源码或文档；未实施 ⑥
- 口径说明：`trait-audit` 只统计单行 trait 定义；本篇的 `PlatformDesc`、`InterruptControllerDesc`、`TimerDesc`、`ConsoleDesc`、`InterruptRouter`、`ClockArch`、`ArchInit` 都是多行定义，不在该表统计范围内。不能把"未出现在 trait-audit"读成"无实现"。

## DIFF-TABLE

七维定义（本表只用于本轮对账，不引入新规则）：

- **D1 类型与签名**：trait 方法集、返回类型、`Any`/downcast、实现完整性。
- **D2 计数与枚举**：可复算的条数、变体数、常量值。
- **D3 时序与生命周期**：启动相位、构造点、冻结与只读共享、实例寿命。
- **D4 存储与所有权**：全局/实例/瞬态、字段是否被读取、跨二进制承载。
- **D5 错误与非法状态**：兜底、拒绝路径、可表示的坏状态、C 真值对照。
- **D6 跨架构与 cfg**：per-arch 文件切分、cfg 分布、品牌名可见性。
- **D7 引用与翻译残留**：逐字/节选保真、陈旧注释、字段来源表。

| # | 维度 | 文档陈述（节） | 代码锚点 / 复算证据 | 判定 | FINDING |
|---|---|---|---|---|---|
| 1 | D1 | §3.2 `PlatformDesc` 八个方法、六个必需两个默认；§3.3 三个子 trait 带 `Any` | `os/libs/minix-boot/src/platform.rs:350-385`（八个方法）、`:415-451`（三个子 trait） | 一致 | — |
| 2 | D1 | §3.2 "三个来源一个接口"；§4.5 `psci_conduit` 是描述符字段 | `device_tree.rs:137`（inherent 方法）与 `:519-542`（trait impl 缺 `psci_conduit`）；`global.rs:196-202` 的转发靠 inherent 方法优先解析 | 缺口 | 01 |
| 3 | D2 | §3.2 25 条 `Self::` 分派；§4.3 `rg -c` 得 2/4；§4.4 六个记录常量、C 声明 11 处理 2；§4.10 `MAX_CPUS = 64` | `rg -o 'Self::(DeviceTree\|Acpi\|QemuVirt)' global.rs \| wc -l` → 25；`rg -c` → global.rs 2 / kind.rs 4；`acpi.rs:72-97` 六常量；C `acpi.h:103-113` 11 条；`platform.rs:266` | 一致 | — |
| 4 | D3 | §4.1 `init_from_kinfo` 在 Phase A.5，早于 `init_protection`；§3.7 一次冻结；§4.7 八次转发 | `os/kernel/src/lib.rs:1752-1785`；`global.rs:51,214-219`；`cell.rs:203-257`；`misc.rs:1106-1142` | 一致 | — |
| 5 | D4 | §3.4 `ClockArch`/`ArchInit` 实例化；§4.6 `const fn new()` 与 `Default` 分工；§4.6 cfg 分布 | `os/arch/src/arch/clock.rs:75-127`；`arch/riscv64.rs:146-161`；`arch/mod.rs:18-23`；`qemu_virt.rs:21-28`；per-arch 方法体 0 处 `cfg(target_arch)` | 一致（残留另列） | 10,11 |
| 6 | D5 | §3.5 开发构建兜底、发布构建 panic；§4.7 失败分支不打印 | `global.rs:275-288,241-268`（与文档一致，函数自身的 "logs a warning" 注释与实现相反） | 一致（文档已点出） | — |
| 7 | D5 | §4.4 IOAPIC 基址来自 MADT 记录；LAPIC 基址读 MADT 表头 | `acpi.rs:476`（无记录时保留 `0xFEC0_0000`）、`:466-472`（表头值 0 时原样保留）；C `apic.c:919-960,1125-1155` 在无 IOAPIC 时关 APIC 回退 PIC | 缺口 | 06 |
| 8 | D5 | §4.5 riscv `PlicDesc` 字段来源表 | `device_tree.rs:235-243`（`context = 1u32` 硬编码）；表内无 `context` 行 | 缺口 | 07 |
| 9 | D4 | §4.5/§4.10 per-CPU 私有地址；§4.8 "计数器地址与频率" | `CpuInfo.gicr_base`/`mtimecmp_addr` 只有写入点（`device_tree.rs:479-499`、`acpi.rs:684-690`、`arch/*.rs`）；`Riscv64ClockArch` 的 `mtime_addr`/`mtimecmp_base`/`mtimecmp_stride` 只有拷贝（`riscv64/clock.rs:83-119`），读取只有 `self.freq`（`:138,198`） | 死抽象 | 08 |
| 10 | D6 | §3.3 "arch 模块内部的枚举是模块私有实现细节"；§1.5/§3.8 早期控制台无活消费者 | `arch/riscv64.rs:64-95`（`pub enum` + `pub fn kind`）；`early_console()` 在 `minix-platform` 之外无调用点（`rg` 为空） | 死抽象 + 可见性漂移 | 09 |
| 11 | D4 | §3.7 `AssumeSyncCell` "只是不再用于 PLATFORM" | `rg -n "AssumeSyncCell" os` 只命中 `cell.rs` 自身与 `global.rs:28` 的历史注释 | 死抽象 | 11 |
| 12 | D4 | §4.7 `source()` 的 "诊断" 用途 | `rg` 全仓无生产代码读取 `source()`；仅 `arch/*.rs` 的 `test_qemu_virt_source` 与 `global.rs` 测试 | 死抽象 | 12 |
| 13 | D5 | §3.5 "每个来源的解析器都返回 Err" 才会兜底；§4.1 `validate()` 是入口门 | `kernel_info.rs:233-270` 起的 `validate_impl_inner` 不检查 `platform_sources`；`kind.rs:49-74` 直接把 `PhysBytes(0)` 解引用；`platform.rs:194-238` 的长度探测反而有 `pa.0 == 0` 守卫 | 非法状态 | 05 |
| 14 | D3 | §1.3/§1.4/§4.2 引导阶段 "只取地址"，例外只有 `/memory` 与 aarch64 DTB 预校验 | `uefi_helpers.rs:221,238,251`、`opensbi_helpers.rs:419`、`bootface_a64.rs:298` 都在引导阶段读 RSDP/DTB 头部取 `len` | 缺口 | 03 |
| 15 | D1 | §4.5 解析前提是 "低 4GB 恒等映射" | `os/kernel/src/lib.rs` Step 1 注释明确 "Mapping the whole low 4 GiB ... minimal windows turn those into page faults"；`push_common_boot_windows` 经 `push_platform_windows` 按 `len`/2 MiB 片注册窗口；`device_tree.rs:33-39` 的 Safety 注释已陈旧 | 漂移 | 04 |
| 16 | D4 | §3.1 `KernelInfo` 是跨二进制 handoff 结构；§3.6 `PlatformDescSource` 纯数据直接跨二进制 | `kernel_info.rs:13-19` 明确 "NOT a cross-image layout contract"（sender 写 wire，receiver 在自己的 `.bss` 重建）；`PlatformDescKind`（`platform.rs:54-55`）无 `repr` | 漂移 | 02 |
| 17 | D6 | §4.8 x86 描述符地址不是 MMIO 操作数；§3.4 实例字段构造后不变 | `os/plat/src/x86_64/interrupt.rs:226-253,269-278`：`LAPIC_VA`/`IOAPIC_VA` 是 `AtomicUsize`，`lapic_read`/`lapic_write` 用 `let _ = base;` 丢弃描述符地址；`os/kernel/src/lib.rs:2437-2473` 在 check-and-reject 后写入两个原子 | 架构缺口 | 16 |
| 18 | D2 | §4.8 "ClockArch 搬运频率（x86/aarch64）"；§3.4 ArchInit 实例字段 | `os/arch/src/arm64/clock.rs:74-83`（不搬任何字段，只做类型校验）；`x86_64/arch_init.rs:33-43` 只存 `acpi_tables`（不是 `acpi_tables + pmu_cycle_counter` 的并集） | 漂移 | 15 |
| 19 | D7 | §3.1 KernelInfo 注释节选；§4.9 `getinfo_machine` 节选；§4.9 `init_smp_state` 节选；§4.8 aarch64 注释 "有删改" | `kernel_info.rs:132-136` 与文档引文的 `len = 0` 子句位置/措辞不同；`misc.rs:1106-1142` 现行体走 `machine_record`；`lib.rs:1961-1974` 的导入顺序与注释文本均被改写 | 翻译残留 | 14 |

## FINDING

## FINDING RD4B-DESIGN-D2-01（P2）

**文档陈述**：§3.2 把 `psci_conduit()` 列为八个接口方法之一，并说明"回答不了的来源用默认值表示没有这个能力"；§4.5 说该字段从 DTB `/psci/method` 解析出 `hvc`/`smc`；§3.2 还给出消费者路径 `os/kernel/src/smp.rs` → `os/arch/src/arm64/smp.rs:fn set_psci_conduit_smc`。

**代码事实**：
- `os/libs/minix-platform/src/device_tree.rs:137-139` 有一个**固有方法** `pub fn psci_conduit(&self) -> Option<PsciConduit>`。
- 同文件 `:519-542` 的 `impl PlatformDesc for DeviceTreeDesc` 只有七个方法，**没有** `psci_conduit`。trait 默认实现返回 `None`。
- `os/libs/minix-platform/src/global.rs:196-202` 的枚举转发 `Self::DeviceTree(d) => d.psci_conduit()` 之所以能取到字段，是因为 Rust 的固有方法优先于 trait 方法。用最小样例复核（`rustc` + 运行）：

```rust
trait T { fn f(&self) -> u32 { 0 } }
struct S;
impl S { fn f(&self) -> u32 { 1 } }
impl T for S {}
fn main() { let s = S; println!("{}", s.f()); let d: &dyn T = &s; println!("{}", d.f()); }
// 输出：1 / 0
```

**差异**：现行唯一消费路径（`platform_desc()` → `PlatformContext` → `PlatformDescEnum` → 固有方法）能工作，但"接口统一"不成立：任何把 `DeviceTreeDesc` 当 `&dyn PlatformDesc` 使用的代码都会得到 `None`，而且不会有编译错误或警告。这不是理论风险：`PlatformDesc` 的公开契约、§3.2 的"八方法"表和 `DeviceTreeDesc` 的 trait 实现三者已经不一致。

**影响**：PSCI 通道选错会让 CPU_ON 静默无效或反弹成未分类异常（`platform.rs:377-384` 的注释自己记录了这两种实测状态）。如果未来有人为测试或重构直接持有 `&dyn PlatformDesc`，`smc` 机器会被当成 `hvc`，故障点离发现点很远。

**建议**：在 `impl PlatformDesc for DeviceTreeDesc` 中补上 `fn psci_conduit(&self) -> Option<PsciConduit> { self.psci_conduit }`，并删除或降级固有方法（保留一个 `pub(crate)` 名字会继续制造同名解析歧义）。如果希望"默认 None 表示没有能力"，就必须让每个能回答的来源显式覆盖；这正是 §3.2 "六个必需 + 两个可选" 的语义。

## FINDING RD4B-DESIGN-D2-02（P3）

**文档陈述**：§3.1 说 "`KernelInfo` 是 01 篇定义的跨二进制 handoff 结构"，并逐字引用字段注释 "safe to pass from boot-shim to kernel even when they are separate ELF binaries"；§3.6 用同一理由解释为什么不能带函数指针，并说 `PlatformDescSource` 是"纯数据（kind 标签 + blob 长度 + 物理地址）"。

**代码事实**：
- `os/libs/minix-boot/src/kernel_info.rs:13-19` 的模块注释明确写："`#[repr(C)]` ... is NOT a cross-image layout contract: after handoff v9 the sender writes `BootReport` into the wire and the receiver reconstructs `KernelInfo` in its own `.bss`; neither image reads this struct from the other's memory."
- 同一段注释给出的分层是：跨镜像承载物是 handoff 的 wire 类型（`BootReport`），`KernelInfo` 与 `PlatformDescSource` 是接收侧重建出来的语义类型，不是直接跨镜像的字节块。
- `PlatformDescKind`（`platform.rs:54-55`）是 `pub struct PlatformDescKind(u32)`，**没有** `#[repr(transparent)]` 或 `#[repr(C)]`；同仓的 `PhysBytes`（`os/libs/minix-types/src/types/address.rs:158-160`）有 `#[repr(transparent)]`。

**差异**：文档把"语义类型直接跨二进制"当成现行机制，实际机制是"固定宽度 wire 类型 + 接收侧重构"（`kernel_info.rs` 模块注释自己的分层）。`PlatformDescKind` 的布局没有被语言钉住，所以即使按文档字面理解，它也只在"同一编译器、同一 crate graph 的 rustc 布局计算"下是 4 字节。

**影响**：这是设计叙事层面的错误锚点：方案 A 的否决理由、§3.6 的"纯数据"论证都指向了错误的承载物。未来的修改者若照着文档给 `KernelInfo` 加 ABI 字段，会绕过真正的 wire 约束；若照着文档认为 `PlatformDescKind` 可直接跨镜像，会漏掉 `repr`。

**建议**：把 §3.1/§3.6 的跨二进制论证改写到 handoff v1 的 wire 类型上，说明 `KernelInfo`/`PlatformDescSource` 只在各自镜像内存在；若仍要保留"`PlatformDescSource` 纯数据"这句，至少给 `PlatformDescKind` 补 `#[repr(transparent)]` 并把理由写成"即使当前不跨镜像，也保持零成本 ABI 形状"。

## FINDING RD4B-DESIGN-D2-03（P3）

**文档陈述**：§1.3 的原则是"引导阶段只取地址（不解析）——窗口内动作最小化"；§1.4 的例外注记只列两处：memmap 装配所需的 `/memory` 子集，以及 aarch64 UEFI 腿的 DTB 预校验；§4.2 展开时仍写 "引导阶段的两处例外也在这一侧"。

**代码事实**：x86-64 与 aarch64 的 UEFI 腿、riscv64 的 OpenSBI 腿、aarch64 直核腿都在引导阶段读取固件 blob 的头部来计算长度：

```text
os/boot-shim/src/uefi_helpers.rs:221   let len = unsafe { minix_boot::rsdp_total_size(pa) };   // x86-64
os/boot-shim/src/uefi_helpers.rs:238   let len = unsafe { minix_boot::dtb_total_size(pa) };    // aarch64 DTB
os/boot-shim/src/uefi_helpers.rs:251   let len = unsafe { minix_boot::rsdp_total_size(pa) };   // aarch64 ACPI
os/boot-shim/src/opensbi_helpers.rs:419 let len = unsafe { minix_boot::dtb_total_size(...) };  // riscv64
os/kernel-image/src/bootface_a64.rs:298 let dtb_len = unsafe { minix_boot::dtb_total_size(...) }; // aarch64 直核
```

`rsdp_total_size` 读签名（偏移 0）、revision（15）、length（20）；`dtb_total_size` 读 magic（0）与 totalsize（4）（`platform.rs:194-238`）。这不是"只取地址"，也不在文档列出的两处例外里。§4.2 的代码块里其实展示了 `rsdp_total_size`，但正文把它们归入"取地址"，例外清单没有第三项。

**影响**：这是 §1.3 的核心边界声明。按现行实现，"定位"包含一次有界头读；把它写成"只取地址"会让读者以为 boot-shim 可以完全不碰 blob 字节。更实际的风险在安全论证：头读发生在固件身份映射内，若某条腿的窗口没覆盖该 blob，`dtb_total_size`/`rsdp_total_size` 会在发现之前先 fault——文档的两处例外清单不足以覆盖这个前置条件。

**建议**：在 §1.3/§1.4/§4.2 把"blob 头部长度探测"列为第三类例外，并明确它的边界（只读 magic/signature/revision/length，不构造结构、不校验表体）；或者把原则改写成"只做有界头探测与地址搬运，不做表解析"，让例外清单与代码一致。

## FINDING RD4B-DESIGN-D2-04（P2）

**文档陈述**：§4.5 说 `parse` 能直接解引用物理地址，"靠的是 boot 早期的恒等映射……低 4GB 铺了 VA == PA 的恒等映射（02 篇建立）"，并原样引用 `device_tree.rs` 模块注释的 Safety 段；篇首前置知识也写"高半地址空间与低 4GB 恒等映射已建立"。

**代码事实**：
- `os/kernel/src/lib.rs` 的 Step 1 注释（`build_bootstrap_root_and_enable` 内）明确写："C maps 4 GiB with `pg_identity` because the 32-bit kernel runs inside that window; the Rust cross-image leg only needs its shim image/stack, the kernel LMA span, the bootstrap pool ... the firmware tables it reads, and the device windows it drives. Mapping the whole low 4 GiB kept every hole PRESENT and hid wild accesses; the minimal windows turn those into page faults."
- 平台 blob 的窗口由 `push_common_boot_windows` → `push_platform_windows` 注册（`os/kernel/src/lib.rs` 内）：`source.has_len()` 时按 `source.len()` 建窗，`len = 0` 时按地址对齐取 2 MiB 片。
- `os/libs/minix-platform/src/device_tree.rs:33-39` 的 Safety 段仍写 "identity mapping over the low 4 GB (C `pg_identity()` semantics)"，是这次迁移后未同步的注释；文档逐字引用了这条陈旧前提。

**差异**：真实前置条件不是"低 4GB 整段恒等映射"，而是"该 blob 落在启动窗口列表里（`push_platform_windows` 已注册），或调用方临时建了映射"。低 4GB 的整段映射已经按最小窗口策略删除，注释和文档都停留在旧机制上。

**影响**：这是解析器的安全契约。当前 `arch_boot` 恰好把 `platform_sources` 注册进窗口，所以 Phase A.5 能读；如果某个新启动腿忘了传 `platform_sources` 给窗口构造，故障是"取指/读数据 page fault"，不是 `DtParseError`/`AcpiParseError`。文档现在的写法会让实现者以为"只要地址小于 4GB 就安全"，掩盖了窗口注册这一必要步骤。

**建议**：把 §4.5 与 `device_tree.rs`/`acpi.rs` 的 Safety 段改为"the blob must be covered by a boot identity window (registered from `KernelInfo.platform_sources`) or a temporary mapping"；同时把 §1 的前置知识从"低 4GB 恒等映射"改成"按启动窗口注册的最小恒等映射（平台 blob 已包含）"。

## FINDING RD4B-DESIGN-D2-05（P2）

**文档陈述**：§3.5 的兜底表把"boot-shim 没找到任何来源"和"切片里的来源全部解析失败"都归到 `qemu_fallback_or_panic`；§4.1 说 `kmain` 先跑 `kernel_info.validate()`，再跑 `init_from_kinfo`；§3.6 说 `PlatformDescSource` 是纯数据 `(kind, len, phys_addr)`。

**代码事实**：
- `PlatformDescSource` 的地址字段是 `PhysBytes`；`os/libs/minix-types/src/types/address.rs:154-157` 的 TECH DEBT 注释明确承认 `PhysBytes(0)` 被用作"无映射/无效"哨兵，并建议改成 `Option<PhysBytes>`。
- `KernelInfo::validate_impl_inner`（`os/libs/minix-boot/src/kernel_info.rs:233` 起）检查 `kern_size`、栈对齐、`memmap`/`reserved_regions` 排序与相交、`boot_modules`、`param_buf`，**没有**任何 `platform_sources` 检查。
- `parse_by_kind`（`os/libs/minix-platform/src/kind.rs:49-74`）对每个来源直接 `source.phys_addr().0 as usize` 传给 `DeviceTreeDesc::parse` / `AcpiDesc::parse`；`device_tree.rs:121-127` 调 `fdt::Fdt::from_ptr`，`acpi.rs:181-193` 直接读 8 字节签名。两个解析器都不检查地址是否为 0。
- 反证：`dtb_total_size`/`rsdp_total_size`（`platform.rs:194-238`）都有 `if pa.0 == 0 { return None; }`，说明"0 是无效地址"已经是本模块自己的约定；只有真正的解析入口没有继承。

**差异**：文档把"全失败 → 兜底"写成 `Err` 路径，但零地址/未映射地址进入解析器会先 fault（或读到无意义字节后返回签名错误），不保证走到 `Err`。同时，`PlatformDescSource::new(..., PhysBytes(0))` 在测试内核与 `.bss` 槽位占位中被普遍用作哨兵，说明"缺省"和"有效地址 0"在类型上不可区分。

**影响**：启动入口的失败模式从"可诊断的 parse error / 兜底"退化成"page fault 或随机签名错误"；这与 §3.5 的"兜底生效本身就是按 QEMU 参数访问别的机器"完全不同的故障类。若固件配置表项地址为 0，现在会在 `init_from_kinfo` 里先解引用，而不是被 `validate()` 拒绝。

**建议**：在 `KernelInfo::validate_impl_inner` 增加 `platform_sources` 逐项检查（`phys_addr != 0`、`len` 不超过容量、kind 非 0），或在 `PlatformDescSource` 构造处把地址改成 `NonZeroU64`/`Option<PhysBytes>`；同时让 `parse_by_kind` 在解引用前返回一个 `PlatformParseError::InvalidSource`，把这类失败纳入 §3.5 的 `Err` 路径。

## FINDING RD4B-DESIGN-D2-06（P2）

**文档陈述**：§4.4 说 IOAPIC 基址来自"第一条 IOAPIC 记录"，`lapic_base` 那一行是"C 没做的那一步：从 MADT 表头读出 Local APIC Address"；§3.5 把"兜底给一组特定机器的常量"当作只属于 `QemuVirtDesc` 的代价。

**代码事实**：
- `os/libs/minix-platform/src/acpi.rs:476`：`let mut ioapic_base = 0xFEC0_0000usize; // Default IOAPIC base (QEMU/PC).` 遍历结束后如果没有 `MADT_TYPE_IOAPIC` 记录，这个常量会原样进入 `MadtResult` → `ApicDesc.ioapic_base`。x86 的 `init_clock_and_interrupts` 用它和设备窗口 `regions[0].phys_base` 比对（`os/kernel/src/lib.rs:2437-2473`），而设备窗口本身就是 `0xFEC0_0000`，所以校验通过，驱动继续按有 IOAPIC 的形态编程。
- `acpi.rs:466-472` 读 MADT 表头的 `local_apic_addr` 并原样赋给 `MadtResult.lapic_base`；`parse_madt` 没有"0 表示默认"的分支。ACPI 6.4 §5.2.12 的表项语义是：该字段为 0 时 LAPIC 地址等于 `0xFEE00000`。C 侧不读这个字段，`apic_single_cpu_init` 无条件使用 `LOCAL_APIC_DEF_ADDR`（`minix3/minix/kernel/arch/i386/apic.c:1130`），所以 C 在"字段为 0"的合规固件上是对的；Rust 会把 0 带进 `ApicDesc`，随后在 `init_lapic`（`os/plat/src/x86_64/interrupt.rs:310-320`）与 `init_clock_and_interrupts` 的 `0xFEE0_0000` 断言上 panic。
- C 真值对照：`acpi_get_ioapics` 在一条记录都没有时返回 0（`apic.c:919-948`），`detect_ioapics` 返回 0（`:950-964`），`apic_single_cpu_init` 因此 `lapic_disable(); lapic_addr = 0x0; return 0;`（`:1125-1155`），最终 `arch_init` 走 legacy PIC（`arch_system.c:264-272`）。Rust 的 ACPI 路径没有这个"没有 IOAPIC 就拒绝 APIC"的状态。

**差异**：§4.4 的描述只在"表里有 IOAPIC 记录且 LAPIC 字段非 0"这一种固件形态下成立；另外两种边界（无 IOAPIC 记录、LAPIC 字段为 0）在 Rust 里分别变成"用 QEMU/PC 常量继续"和"断言失败"。前者正是 §3.5 批评的"特定机器常量"，只是藏在 ACPI 解析器里而不是 `QemuVirtDesc`。

**影响**：在非 QEMU 的 x86 固件上，如果 MADT 不含 IOAPIC（例如只有 8259 的旧机器）或按规范把 LAPIC 地址写 0，Rust 的行为分别是"静默编程一个不存在的 IOAPIC"和"启动期 panic"，都不是 C 的"回退 legacy PIC"。QEMU/OVMF 两者都非 0 且有记录，所以现有真机门全绿，风险被测试形态掩盖。

**建议**：(a) `lapic_base == 0` 时按 ACPI 规则取 `0xFEE0_0000`，并把这条写进 §4.4 的字段说明；(b) `ioapic_base` 改成 `Option<usize>` 或在无记录时返回新的 `MadtNoIoapic` 错误，让 `init_from_kinfo` 落到下一来源/兜底，而不是用 QEMU 常量；如果决定保留默认，必须在 §3.5/§4.4 把它登记成"ACPI 路径的特定机器默认"，并加进 §6.1 的诊断表。

## FINDING RD4B-DESIGN-D2-07（P2）

**文档陈述**：§3.3 的 `PlicDesc { plic_base, nr_irqs, context }` 把 `context` 列为描述符字段；§3.8 的判据是"有协议报告机制 → 进描述符"；§4.5 的 `ic`（riscv64）来源表只写基址（`reg` 首格）与线数（`riscv,ndev`），没有 `context` 一行。

**代码事实**：
- `os/libs/minix-platform/src/device_tree.rs:235-243`：`// S-mode context for hart 0 is typically context 1 in QEMU virt (M-mode = 0, S-mode = 1). let context = 1u32;`——常量，没有任何 DTB 属性读取。
- `os/libs/minix-platform/src/arch/riscv64.rs:122-125` 的 `QemuVirtDesc::new` 同样写 `context: 1`。
- DTB 里存在可报告的对应物：CPU 节点的 `interrupts-extended` 把该 hart 的 PLIC context 作为 `<&plic context>` 对给出；QEMU virt 只是恰好让 hart 0 的 S-mode context 等于 1。

**差异**：`context` 是 PLIC 描述符三个字段里唯一没有来源行、也没有被登记为假设的字段。它不是 QEMU 布局常量（`mtimecmp_stride = 8` 那种设备规范常量），而是一个平台拓扑量；按 §3.8 自己的判据，它应该被发现，或者至少像 R04-T11 的 GICC UID 假设那样登记成一条显式假设。

**影响**：在 hart 0 的 S-mode context 不是 1 的 RISC-V 平台上，`threshold_offset`/`claim_offset` 会指向错误的 context（`os/plat/src/riscv64/interrupt.rs:98-105`），表现为"PLIC 不投递/claim 永远 0"，而描述符、DTB 与诊断表都没有线索。`context` 还会进入 `Riscv64InterruptController`，但该值来自描述符，排查者会误以为它是从固件读到的。

**建议**：在 `parse_plic` 里从 CPU 节点的 `interrupts-extended` 解析 hart 0 的 PLIC context（找不到再回退 1，并让来源可见）；若本轮不做，至少在 §4.5 表加一行 `context`，写明"当前硬编码 1（QEMU virt 假设）"，并在 §5.4/§6.1 登记成与 R04-T11 同级的假设项。

## FINDING RD4B-DESIGN-D2-08（P3）

**文档陈述**：§4.10 的"新增"行把 `CpuInfo` 的 per-CPU 私有地址列为设计新增；§4.5 末尾说"每个 CPU 的 `hw_id` 直读 `reg` 首格……per-CPU 私有地址按架构计算：aarch64 是 `gicr_base + hw_id * gicr_stride`；riscv64 是 `mtimecmp_base + hw_id * mtimecmp_stride`"；§4.8 表把 riscv64 的 `ClockArch` 搬运内容写成"计数器地址与频率"；§4.9 说 `cpu_topology()` 的产物在本篇展开三处。

**代码事实**：
- `CpuInfo` 的 `gicr_base`/`mtimecmp_addr`（`os/libs/minix-boot/src/platform.rs:294-308`）只在构造点被写入：`device_tree.rs:479-499`、`acpi.rs:684-690`、`arch/{aarch64,riscv64}.rs` 的 `Default`。
- 在 `os/kernel`、`os/arch`、`os/plat` 中检索 `mtimecmp_addr|gicr_base`，命中全部来自 `minix-platform` 自身、`minix-boot` 定义、测试/注释；`AArch64InterruptController::gicr_base_for`（`os/plat/src/arm64/interrupt.rs:126-127`）用实例自己的字段计算，不读 `CpuInfo`。
- `Riscv64ClockArch` 的 `mtime_addr`、`mtimecmp_base`、`mtimecmp_stride`（`os/arch/src/riscv64/clock.rs:83-88,115-119`）只有构造时拷贝；全文件对它们的读取为零，实际使用的是 `self.freq`（`:138,198`）。`read_ticks` 走 `time` CSR，`init_timer`/`stop_local_timer`/`local_timer_eoi` 走 SBI `TIME` ecall；`mtime_addr` 在仓库里没有任何读取者。

**差异**：文档把"per-CPU 私有地址"当成已交付的拓扑扩展，把 CLINT 地址当成时钟实例的搬运字段；实现里这些值是只写不读的死数据。§4.9 列出的三处消费者（`init_smp_state`、`init_protection`、`getinfo_machine`）都没有读这两个 `Option` 字段。

**影响**：`CpuTopology` 的 ABI 形状（64 × `CpuInfo`，每个带两个 `Option<usize>`）被两个未消费的字段撑大；未来 SMP 篇若真的需要 per-CPU GICR/mtimecmp，可能再写一套而不复用这两个字段。`mtime_addr` 尤其容易误导：它暗示 MMIO 读 mtime，而实现已经因为 OpenSBI 的 PMP 保护改走 `time` CSR。

**建议**：二选一。(a) 在 `16-smp.md` 落地消费者前，把 `CpuInfo.gicr_base`/`mtimecmp_addr` 与 `Riscv64ClockArch` 的三个 CLINT 地址字段删除（`ClintDesc` 可保留给未来的 M-mode 路径，但要在 §3.9/§4.6 说明它是"为 M-mode 保留、当前无读者"）；(b) 若保留，在 §4.5/§4.10 明确写"这些字段当前只有写入点，消费者在 16-smp.md"，并加 `#[allow(dead_code)]` 或测试断言，避免读者把"字段存在"当成"已被消费"。

## FINDING RD4B-DESIGN-D2-09（P3）

**文档陈述**：§1.5 承认"描述符里有一个早期控制台方法、三种来源的实现都写了它，但本篇检索范围内没有活消费者"；§3.8 的表把早期控制台基址写成"描述符里有这一项（`ConsoleDesc`），但活路径不读它"；§3.3 说"`arch/riscv64.rs` 内部的 `Riscv64ConsoleKind` 也是枚举……arch 模块内部的枚举是模块私有实现细节"。

**代码事实**：
- `PlatformDesc::early_console`（`os/libs/minix-boot/src/platform.rs:355-356`）的返回类型 `ConsoleDesc` 有三个实现（x86 `IsaSerialDesc`、aarch64 `MmioSerialDesc`、riscv `Riscv64ConsoleDesc`），另有两个解析器分支（`device_tree.rs:417-440`、`acpi.rs:264-265`）。
- 在 `minix-platform` 之外检索 `early_console()` 为空；唯一的使用是 `minix-platform/src/global.rs` 的测试与转发。早期控制台实际由 `os/plat/src/early_console.rs` 与各架构模块负责。
- `Riscv64ConsoleKind` 是 `pub enum`（`arch/riscv64.rs:64-70`），`Riscv64ConsoleDesc::kind()` 是 `pub fn`（`:91-94`），且 `arch` 是 `pub mod`（`lib.rs:31`）；"模块私有"在 Rust 可见性意义上不成立。`mmio()` 构造器（`:86-90`）只被 `device_tree.rs:423` 使用，`kind()` 没有消费者。

**影响**：一个 trait + 三个品牌结构 + 两个解析器分支 + 一个 `Option` 字段全部为"协议上存在但活路径不读"服务；其中 `Mmio` 变体还会让读者以为运行时控制台选择已实现。§3.3 的"模块私有"说法则会让人误以为外部 crate 无法命名这些品牌名，实际上 `minix_platform::arch::riscv64::Riscv64ConsoleKind` 是公开可达的。

**建议**：短期最小化——删除 `PlatformDesc::early_console`、`ConsoleDesc`、三个 console struct 与解析器里的 `parse_console` 分支（ACPI 的 `console` 字段与 `IsaSerialDesc` 一并删）；如果确实要保留"未来 SPCR/stdout-path 发现"的接口，就把它降级为 `#[cfg(test)]` 测试接缝并在 §3.8 写明"当前仅供测试/未来协议占位"。无论保留与否，`Riscv64ConsoleKind` 应收成 `pub(crate)` 并删除无消费者的 `kind()`。

## FINDING RD4B-DESIGN-D2-10（P3）

**文档陈述**：§3.7 展示 `static PLATFORM: Frozen<PlatformContext>`；§4.7 说"读取侧是 `PlatformContext` 的八次转发"；§3.2 的三层职责表是"接口 / 存储 / 分派"。

**代码事实**：
- `PlatformContext`（`global.rs:57-60`）只有一个 `pub desc: PlatformDescEnum` 字段。
- `impl PlatformDesc for PlatformContext`（`:62-87`）八个方法全部是 `self.desc.<same>()` 的一行转发；`Debug` 实现也只是包一层。
- `PlatformDescEnum` 本身已经实现 `PlatformDesc`（`:132-203`），所以 `Frozen<PlatformDescEnum>` 可以直接作为 `&'static dyn PlatformDesc` 的来源。
- 全仓检索 `PlatformContext`：只有定义、`PLATFORM` 静态、`init()` 包装与 re-export；没有外部构造者或额外字段。

**差异**：设计文档把 `PlatformContext` 当作"全局上下文"层，但它目前没有承载除 `desc` 之外的任何状态。§3.2 的三层是"接口/存储/分派"，`PlatformContext` 是第四层纯包装。

**影响**：多一层静态调用与一个公开类型；更实际的是它让 `device_tree/psci` 的 trait 缺口更难被发现——读者会以为 `PlatformContext` 的转发是接口的一部分，而 `PlatformDescEnum` 的转发又依赖固有方法（见 FINDING 01）。删掉它能把"接口实现"集中到 `PlatformDescEnum` 与三个具体类型上。

**建议**：改成 `static PLATFORM: Frozen<PlatformDescEnum> = Frozen::new();`，`platform_desc()` 返回 `PLATFORM.get()` 并让编译器做 `&PlatformDescEnum → &dyn PlatformDesc` 的 unsize 强转；删除 `PlatformContext` 的八次转发、`Debug` 实现与 `init()` 里的包装。若未来确实需要"全局上下文加字段"，再引入并说明它承载什么。

## FINDING RD4B-DESIGN-D2-11（P3）

**文档陈述**：§3.7 说 "`AssumeSyncCell` 是这次替换的前身，它仍在仓库里（`os/libs/minix-types/src/types/cell.rs`，与 `Frozen` 同一个文件），只是不再用于 `PLATFORM`"。

**代码事实**：`rg -n "AssumeSyncCell" os` 的全部命中都在 `os/libs/minix-types/src/types/cell.rs` 的定义/文档/测试，以及 `os/libs/minix-platform/src/global.rs:28` 的一段历史说明。没有任何生产模块把它用作全局或字段类型。

**差异**："只是不再用于 `PLATFORM`"在字面上为真，但会让读者以为它仍服务于其他全局；实际它已经是仅测试可见的死类型。它的 `unsafe impl<T> Sync` 契约（"调用方保证单线程独占访问"）在 SMP 内核里正是 §3.7 要淘汰的形状。

**影响**：保留一个公开的 `unsafe impl Sync` 类型，等于给未来的内核代码留了一条绕过 `Frozen` 协议的捷径；它与 `Frozen` 放在同一文件、名字只差一个词，也增加选型时的误用概率。

**建议**：删除 `AssumeSyncCell` 及其测试（`cell.rs:12-171`），并把 `global.rs:28` 的历史说明改成"曾用 `UnsafeCell` + 手工 `Sync`，现由 `Frozen` 取代"；若出于兼容必须保留，加 `#[deprecated(note = "use Frozen for boot-frozen globals")]` 并在文档里写明"仅测试/历史保留"。

## FINDING RD4B-DESIGN-D2-12（P3）

**文档陈述**：§4.7 的 `source()` 用途表列出"诊断时区分'走了兜底'与'解析成功' | 合理"；§6.1 的第一条诊断也依赖 `source()` 判断兜底。

**代码事实**：
- `PlatformSource` 只在 `minix-boot/platform.rs:252-261` 定义、`minix-platform` re-export；生产代码中没有 `match`/`==` 读取它。
- `source()` 的调用点只有 `os/libs/minix-platform/src/arch/{x86_64,aarch64,riscv64}.rs` 的 `test_qemu_virt_source` 与 `global.rs` 的测试；平台 crate 之外没有生产读取者。
- `qemu_fallback_or_panic` 与 `init_from_kinfo` 的失败分支都不打印（`global.rs:241-288`），所以"走了兜底"在生产日志里没有任何输出；§6.1 说的 `source()` 只能由调试器/未来调用方读取。

**差异**：§4.7 把"诊断"写成已实现的用途，但仓库里没有生产诊断消费者。`source()` 目前是测试接缝，不是诊断通道。

**影响**：读者按 §6.1 排障时会在启动日志里找不到 `PlatformSource::QemuVirt`，因为没有任何代码打印它；文档与诊断入口的承诺不匹配。

**建议**：二选一。(a) 在 `init_clock_and_interrupts` 或 `bsp_finish_booting` 已经具备控制台之后打印一次 `source()`（不必在 A.5 打印，可避开 §6.1 解释的控制台未就绪），这样 §6.1 的第一条诊断成立；(b) 在 §4.7/§6.1 明确写"`source()` 当前只供测试与调试器读取，生产路径不打印"，并把表格里的"诊断 | 合理"改成"测试接缝/未来诊断"。若选 (b)，`PlatformSource` 可以进一步降级为 `#[cfg(test)]` 或从公开 API 移除。

## FINDING RD4B-DESIGN-D2-13（P3）

**文档陈述**：§4.10 说 `CpuTopology` 用 `[CpuInfo; MAX_CPUS]` 数组，`MAX_CPUS = 64`，"这个常量同时决定 `parse_cpu_topology` 与 `parse_madt` 的 CPU 收集上限"；§4.9 的消费者按 `topo.cpus[i]` 取 per-CPU 信息。

**代码事实**：
- `CpuTopology { pub nr_cpus: u32, pub bsp_id: u32, pub cpus: [CpuInfo; MAX_CPUS] }`（`os/libs/minix-boot/src/platform.rs:269-282`）三个字段都是 `pub`，没有构造函数或 `validate()`。
- 解析器确实在收集时钳制（`device_tree.rs:458-460`、`acpi.rs:535,563,596`），但类型本身允许 `nr_cpus` 大于 64。
- 消费者直接按 `nr_cpus` 索引：`os/kernel/src/lib.rs:1972-1974`、`:2236-2238`（`init_protection`）、`os/kernel/src/smp.rs:1260-1282` 都是 `(0..topo.nr_cpus as usize)` 后 `topo.cpus[i]`；`init_protection` 的 `unwrap_or(0)` 还允许 `nr_cpus == 0` 时进入后续逻辑。QEMU 测试内核也直接构造 `CpuTopology`。

**差异**：§4.10 描述的是一个"解析器会钳制"的事实，而不是一个类型不变量；文档没有说明 `CpuTopology` 的合法值域，也没有要求消费者检查。`nr_cpus > MAX_CPUS` 或 `nr_cpus == 0` 都是可表示的，前者会让消费侧 panic（索引越界），后者在 `init_smp_state` 里被 `.max(1)` 兜住、在 `init_protection` 里没有。

**影响**：这是一个可表示的非法状态，只靠"三个解析器都钳制了"的约定维持。任何新增来源（例如未来的 ACPI SRAT/PPT 或测试夹具）只要漏一次钳制，故障点就在消费侧而不是发现侧。

**建议**：把 `CpuTopology` 的字段收成私有，提供 `CpuTopology::new(&[CpuInfo], bsp_id) -> Self`（内部钳制/断言）与只读访问器；或把 `nr_cpus` 改成 `u8` 并在 `validate()` 里断言 `1 <= nr_cpus <= MAX_CPUS as u8`。如果保留公开字段，至少在 §4.10 写明"构造方必须保证 `1 <= nr_cpus <= MAX_CPUS`；消费者不重复检查"。

## FINDING RD4B-DESIGN-D2-14（P3）

**文档陈述**：§3.1 的 `KernelInfo` 代码块标注"节选（本字段的文档注释；省略号处为其余字段）"；§4.9 的 `getinfo_machine` 代码块标注"节选（函数上方的文档注释省略；`MachineStruct` 的构造已抽到 `machine_record`，下面块内的字面量只示意调用顺序，不是现行函数体）"；§4.9 的 `init_smp_state` 标注"节选（第一个 `unsafe` 块）"；§4.8 的 aarch64 代码块标注"注释有删改：行首的过程编号未保留"。

**代码事实**：
1. §3.1 引文把 `len = 0` 的说明写成 "`len = 0` means unknown/legacy" 并放进 `(u32 kind, u32 len, u64 phys_addr)` 同一行；现行 `kernel_info.rs:132-136` 是：
   `/// Each PlatformDescSource is pure data (u32 kind, u32 len, u64 phys_addr)` / `/// — safe to pass ... No function pointers cross the boundary. len = 0` / `/// means the producer did not record the blob extent; the window builder` / `/// then falls back to one 2 MiB piece.`
   即引文改写了措辞、丢掉了"window builder 回退 2 MiB"这一机制说明。
2. §4.9 的 `getinfo_machine` 块展示的是内联 `MachineStruct { processors_count: ..., bsp_id: ..., apic_enabled, acpi_rsdp, ..Default::default() }` 与内联 `let acpi_rsdp = ...`；现行函数（`os/kernel/src/misc.rs:1120-1142`）在取 `smp`/`acpi_rsdp` 后调用 `machine_record(processors_count, bsp_id, acpi_rsdp)`（`machine_record` 定义在 `:1106-1118`）。文档自己在括号里承认"不是现行函数体"，但代码块标题仍写 `// os/kernel/src/misc.rs:fn getinfo_machine —— 节选`。
3. §4.9 的 `init_smp_state` 块把代码注释改成 "The DTB/ACPI `bsp_id` is a hint, not the truth: election/CPUID resolves the CPU actually executing this code."；现行 `os/kernel/src/lib.rs:1961-1974` 的注释是 "OpenSBI may hand control to any hart (QEMU -smp 4 has booted hart 1 and hart 2), and the entry election cell records the real one..."。同块还把 `use minix_platform::{PlatformDesc, platform_desc};` 写成 `{platform_desc, PlatformDesc}`。
4. §4.8 的 aarch64 注释块是中文改写（"先判零再翻译……"），代码里对应的是英文注释；标签"注释有删改"直接承认了改写。

**差异**：D-045 的规则是"节选引文只许省略不许改写"。上述四处中，第 1、2 处改写了实质内容（`len` 语义、函数体），第 3 处改写了注释并调整了导入顺序，第 4 处改写了注释。它们不是"省略"，而是"重写后仍挂原函数锚点"。

**影响**：读者会把 `getinfo_machine` 的内联构造当成现行实现（尤其在排查 `board_id`/`padding` 时），也会把 `len = 0 means unknown/legacy` 当成现行语义——后者正是 `kernel_info.rs:132-136` 自己写明、而引文丢掉的那半句（"the window builder then falls back to one 2 MiB piece"）。引文保真是本项目锚点纪律的硬要求；这类漂移会让后续 review 重复核对同一处。

**建议**：把 §3.1 的引文替换为现行 `kernel_info.rs:132-136` 原文（只省略头两行也可）；把 §4.9 的 `getinfo_machine` 块换成现行三行节选（`let smp = ...; let acpi_rsdp = ...; let machine = machine_record(...)`）或整块删掉并改为文字说明；把 `init_smp_state` 的注释恢复原文或整段改成"示意"且不挂 `fn` 锚点；§4.8 的中文注释块改为"示意（非逐字）"，不要写"节选"。

## FINDING RD4B-DESIGN-D2-15（P3）

**文档陈述**：§4.8 的消费点表写 "`boot_init_timer` | `ClockArch` | x86-64 `PitDesc` / aarch64 `ArmGenericTimerDesc` / riscv64 `ClintDesc` | 频率（x86/aarch64）/ 计数器地址与频率（riscv64）"；§3.4 的实例化表写 `ArchInit` 的"实例字段"是"架构杂项（`acpi_tables: Option<usize>` + `pmu_cycle_counter: bool`）"。

**代码事实**：
- `os/arch/src/arm64/clock.rs:74-83` 的 `AArch64ClockArch::new` 只做 `downcast_ref::<ArmGenericTimerDesc>().expect(...)`，然后返回 `Self`；`Self` 是空结构。频率在 `init_timer` 里用 `mrs cntfrq_el0` 运行时读取（`:93-96`）。所以 aarch64 的 `ClockArch` 不搬运任何字段。
- `os/arch/src/x86_64/arch_init.rs:33-43` 的 `X86_64ArchInit` 只有 `acpi_tables: Option<usize>`（并标 `#[allow(dead_code)]`）；没有任何 `ArchInit` 实例同时持有 §3.4 表里写的两个字段。

**差异**：§4.8 的"频率（x86/aarch64）"对 aarch64 是错的，且与 §4.5 的表"aarch64 timer 只确认存在（`ArmGenericTimerDesc` 无字段）；频率运行时从 `CNTFRQ_EL0` 读"直接矛盾。§3.4 的 `ArchInit` 行把描述符 `ArchMiscDesc` 的字段并集写成了实例字段，读者会以为两个字段在同一个实例里。

**影响**：这两处都是"设计决策表"的输入。aarch64 的时钟初始化其实不依赖描述符值（只依赖类型正确），把"频率搬运"写进去会掩盖"频率必须在运行时读"这一事实；`ArchInit` 的字段并集会让读者误判 x86 的 `acpi_tables` 与 aarch64 的 `pmu_cycle_counter` 有共同的消费路径。

**建议**：§4.8 的"搬运的字段"按架构拆开：x86-64 "PIT 基频"，aarch64 "无（仅类型校验；频率运行时读 `CNTFRQ_EL0`）"，riscv64 "CLINT 地址与频率（当前仅频率被读取，见 FINDING 08）"；§3.4 的 `ArchInit` 行改成"各架构只存自己需要的字段：x86-64 `acpi_tables`，aarch64 `pmu_cycle_counter`"。

## FINDING RD4B-DESIGN-D2-16（P2）

**文档陈述**：§3.4 说 Rust 把 C 的全局硬件地址改成实例字段，并强调"读者拿到 `&self` 就知道描述符字段不会再变"；§4.8 的表说 x86 的描述符地址"不是 MMIO 操作数本身"，`lapic_read`/`lapic_write`/`ioapic_*_indirect` 都经 `LAPIC_VA`/`IOAPIC_VA`；§3.9 的"地址三语义"把描述符 PA、实例 VA、MMIO 操作数分开。

**代码事实**：
- `os/plat/src/x86_64/interrupt.rs:226-241` 定义 `static LAPIC_VA: AtomicUsize`（初值 `0xFEE0_0000`）与 `static IOAPIC_VA: AtomicUsize`（初值 `0xFEC0_0000`），以及 `x86_64_interrupt_set_lapic_va`/`set_ioapic_va` 两个写入口。
- `:245-253` 的 `lapic_read`/`lapic_write` 第一行是 `let _ = base; // window base governs; the caller's PA base is legacy`，实际地址来自 `LAPIC_VA.load(...)`；`:269-278` 的 `ioapic_read_indirect`/`ioapic_write_indirect` 同样 `let _ = base;` 后使用 `IOAPIC_VA`。
- `os/kernel/src/lib.rs:2437-2473` 在 `init_clock_and_interrupts` 里先断言描述符 LAPIC/IOAPIC 基址等于固定设备窗口页（`0xFEE0_0000`/`regions[0].phys_base`），再调用两个 setter 把 VA 写进原子。
- 因此 x86 的 `X86_64InterruptController` 实例字段 `lapic_base`/`ioapic_base` 只被 `lapic_base()`/`ioapic_base()` 访问器与断言读取；真正决定 MMIO 落点的状态是进程全局原子，是在实例构造之后写入的。

**差异**：文档已经披露了"描述符地址不是 MMIO 操作数"这一事实，但把它当作地址三语义的正常分工；从设计轴看，这使 §3.4 的"实例化取代全局地址"只完成了一半：C 的 `lapic_addr` 全局被换成了 `LAPIC_VA` 全局（外加一个描述符 check 值）。这同时是 R04-T09（重定位 LAPIC/IOAPIC 的动态设备窗口）与 R04-T10（多 IOAPIC）的结构性阻塞：`device_window_regions()` 是固定表，`ApicDesc` 只有一个 `ioapic_base`，LAPIC/IOAPIC 的每次 MMIO 都经两个单值原子。

**影响**：这不是当前 QEMU 启动的 bug（固定窗口与 QEMU 布局一致，断言能通过），而是设计演进的天花板：任何"描述符报出的地址会变"的机器都会在 check-and-reject 处停机；任何第二颗 IOAPIC 都没有地方表达。文档把这条限制写成"当前未实现（R04-T09）"，但没有给出替代形状；如果继续在 §3.4 宣称实例字段是唯一事实源，后续实现会在"原子槽"与"实例字段"之间反复分叉。

**建议**：见下节 `## ARCH-PROPOSAL`。本轮只登记，不实施。

**最小化机会汇总**（与 FINDING 08–12 对应；均为 P3 SUGGESTION，不阻断本轮）：

| 可删/可并 | 证据 | 预期收益 | 风险 |
|---|---|---|---|
| `ConsoleDesc` 全族 + `early_console()` + 三个 console struct + `parse_console` 分支 | FINDING 09；`minix-platform` 之外零调用 | 删掉一整条"协议上存在、活路径不读"的接口；消除 `Riscv64ConsoleKind::Mmio` 的误导 | 若未来要做 SPCR/stdout-path 发现，需要重新引入；可先降级为 `#[cfg(test)]` 接缝 |
| `PlatformContext` 单字段包装 | FINDING 10；`PlatformDescEnum` 已实现同一 trait | 少一个公开类型、八次转发与一层静态调用 | 若未来全局需要额外字段，需要重新引入；当前没有此类字段 |
| `AssumeSyncCell` + 其测试 | FINDING 11；仅 `cell.rs` 自身与 `global.rs` 历史注释命中 | 删掉一个公开 `unsafe impl Sync` 的旧逃生口 | 若有仓外使用者会破坏 API；本仓内没有 |
| `CpuInfo.gicr_base`/`mtimecmp_addr` + `Riscv64ClockArch` 的三个 CLINT 地址字段 | FINDING 08；只有写入点，无读取者 | `CpuTopology` 每 CPU 少两个 `Option<usize>`（64 CPU 约 2 KiB），删掉 `mtime_addr` 的误导 | SMP 篇若真需要，需重新加回；可保留 `ClintDesc` 并把"无读者"写清 |
| `PlatformSource`/`source()`（或补一条生产日志） | FINDING 12；无生产消费者 | 若删：少一个公开枚举与 trait 方法；若补日志：§6.1 的诊断承诺成立 | 删会削弱调试信息；补日志需要一个已就绪的控制台点 |

## ARCH-PROPOSAL

### AP-D2-01 把设备窗口做成显式能力：`DeviceWindow` 拥有"描述符 PA → MMIO VA"的映射，取代 x86 的 `LAPIC_VA`/`IOAPIC_VA` 原子槽

**对应 FINDING**：16（[ARCH] P2）。**状态**：仅提议；本轮不实施，不改代码、不改文档。**与既有登记的关系**：为 R04-T09（重定位 LAPIC/IOAPIC 的动态设备窗口）与 R04-T10（多 IOAPIC 的 GSI-range 模型）提供统一形状；不新增功能承诺。

**问题形状**：

当前的地址三语义（§3.9）在 x86 上落成三个互不引用的位置：

```text
描述符（ApicDesc.lapic_base/ioapic_base）   → 只用于 init_lapic / init_clock_and_interrupts 的断言
实例（X86_64InterruptController 同名字段）  → 只用于访问器与断言，不参与 MMIO 地址计算
全局原子（LAPIC_VA / IOAPIC_VA）            → 真正决定每次 lapic_read/write 的落点，在实例构造之后写入
```

`os/plat/src/x86_64/interrupt.rs:245-278` 的 `let _ = base;` 就是这条分裂的显式痕迹。它的直接后果是：`ApicDesc` 报出的地址不能用于 MMIO，只能用于拒绝；R04-T09/R04-T10 需要的"地址可变"与"多控制器"都没有承载物。

**目标形状**（不要求一次落全，可分两步）：

1. 在架构层（`os/arch/src/arch/` 或 `minix-platform` 的 arch 子模块）引入一个**值类型** `DeviceWindow`（名字可改），它拥有：
   - 一组物理区间（base, len）与其 VA 别名；
   - `fn translate(&self, pa: PhysBytes) -> Option<VirBytes>`（或返回带拒绝原因的结果）；
   - 构造来源是平台描述符 + 当前架构的窗口常量（aarch64 的 `MMIO_PA_BASE`/`MMIO_VA_BASE` 已经是这个形状的雏形，见 `minix-platform/src/arch/aarch64.rs:30-54`）。
2. 把 `InterruptRouter::new`（以及 `ClockArch::new` 中需要 MMIO 的架构）改为接收 `&DeviceWindow`，或让 `init_clock_and_interrupts` 在构造实例前先建立窗口，再把窗口引用交给实例；实例字段保存"本控制器的 PA + 解析后的 VA"或只保存窗口句柄。x86 的 `LAPIC_VA`/`IOAPIC_VA` 原子与两个 setter 删除，`lapic_read/write(pa, off)` 改为 `window.mmio(pa, off)`。
3. 重定位与多 IOAPIC 变成数据问题而不是断言分支：描述符报 `0xFEE0_0000` 以外的 LAPIC 时，窗口构造决定"映射到哪里"（成功则继续，失败则拒绝并给出窗口诊断）；`ApicDesc` 改为 `IoApicSet`（至少一个元素；首个元素保持现有 ABI），`init_ioapic` 遍历集合而不是单个 `ioapic_base`。

**迁移顺序建议**：

- 第 1 步只做"窗口值类型 + x86 两个原子删除"，行为不变（固定窗口仍然只映射那两页），用现有三架构 QEMU 门和 `test-protection`/`test-timer-irq` 证明无回归。
- 第 2 步再做"窗口按描述符构造 + 重定位拒绝路径的负向测试"（R04-T09 的载体），最后做 `IoApicSet`（R04-T10）。
- aarch64 的 `mmio_translate`（const 断言 + 固定窗口）可以保留为窗口实现的一个后端；riscv64 的 `plic_va_base` + `set_plic_va_base` 同理，最终由同一个窗口值类型统一，而不是三套 setter。

**为什么这是 Architectural Evolution 而不是 Refactor**：

- Minix3 C 的 `lapic_addr`/`io_apic[]` 是"地址就是 MMIO 操作数"的模型；把地址、映射与实例拆开并引入可查询的窗口能力，改变了平台发现与消费之间的契约，不是保持外部行为的重构。
- 按 AGENTS 的三档术语，必须在文档、设计快照与代码三处同时标注 `[ARCH: device-window]`（或把现有 `[ARCH: mmio-device]` 扩展为覆盖 x86 的这个形状），并给出 C 对照说明（C 无此概念）。D-047 规定 ARCH 硬阻断：在作者裁决并登记决策之前，不应把本提议并入正文。

**风险与回退**：

- 窗口必须在第一次 LAPIC MMIO 之前可用；x86 的 `enable_current_cpu_lapic`（`os/plat/src/x86_64/interrupt.rs:411-433`）目前直接用 MSR 里的 LAPIC 基址计算 VA，迁移时要决定 AP 上窗口是全局共享还是每 CPU 一份；如果处理不当，会在 AP 首次 LAPIC 写时 fault。
- 回退路径简单：保留两个原子与 setter 但标 `#[deprecated]`，让窗口类型先只做读取；一旦窗口值类型在三个架构上稳定，再删原子。
- 验收证据至少需要：三架构现有 QEMU 门全绿；x86 的"描述符 LAPIC 地址与窗口不一致"负向测试（当前是 panic，改造后应是窗口拒绝的可诊断错误）；以及一份含两个 IOAPIC 记录的合成 MADT 测试（R04-T10）。

## VERDICT

**结论：NOT CONVERGED（确认轮不通过）。**

计数：P0 = 0，P1 = 0，P2 = 6（FINDING 01、04、05、06、07、16），P3 = 10（FINDING 02、03、08–15），ARCH 提议 = 1（AP-D2-01，对应 FINDING 16）。

理由（按阻断强度排序）：

1. **接口契约与实现不一致（01）**：`DeviceTreeDesc` 的 `PlatformDesc` 实现缺 `psci_conduit`，现行唯一路径靠固有方法优先解析；任何 `&dyn PlatformDesc` 使用都会静默得到 `None`。这是 §3.2 "三个来源一个接口" 的直接反例，不能以 P3 放过。
2. **C 真值偏离（06）**：ACPI 路径在"无 IOAPIC 记录"和"MADT LAPIC 地址为 0"两种固件形态下分别用 QEMU/PC 常量继续和断言失败，而 C 的行为是回退 legacy PIC。现行 QEMU 测试形态不覆盖这两个边界，所以绿灯不能证明设计成立。
3. **安全前置条件写错（04、05）**：§4.5 的"低 4GB 恒等映射"已不再成立（最小启动窗口），§3.5 的兜底叙事不覆盖"来源地址为 0/未映射"的 fault 路径，而 `KernelInfo::validate` 不检查 `platform_sources`。这两条都影响"发现失败时会发生什么"的设计陈述。
4. **平台假设未登记（07）**：`PlicDesc.context` 硬编码 1，既无来源行也无假设登记，违反 §3.8 自己的判据。
5. **架构天花板（16）**：R04-T09/R04-T10 的结构性根因仍在（全局原子槽 + 单 IOAPIC 字段）；本轮只登记 AP-D2-01，需作者裁决是否按 ARCH 硬阻断处理。

P3 项（02、03、08–15）是引文漂移、边界叙事不完整、死抽象、冗余包装与字段表不准，按本轮口径均为 SUGGESTION；它们不单独阻断，但 02/03 是核心边界声明（跨二进制承载、引导窗口例外），08/09/10/11/12 合起来说明"描述符家族"有一整层未被消费的接口，建议在 R3 一次性收口，而不是逐条打补丁。

**最小放行条件**（供作者裁决）：

- 01：补 trait 方法（或在 §3.2 明确"`psci_conduit` 不是接口方法"并改掉八方法表述）。
- 04、05：改写 §4.5/§3.5 的前置条件与兜底范围；05 还需要在 `KernelInfo::validate` 或 `parse_by_kind` 增加零地址/非法来源拒绝。
- 06：补 ACPI 的 0 ⇒ `0xFEE00000` 规则；`ioapic_base` 的缺记录行为必须二选一（拒绝或登记为显式默认）。
- 07：解析 `interrupts-extended` 的 context，或在 §4.5/§5.4 登记为显式假设。
- 02、03、14、15：文档修正，不涉及代码。
- 16/AP-D2-01：作者裁决；若接受，按 `[ARCH: ...]` 三处一致登记后再排期；若拒绝，在 §4.8 写明"固定窗口是长期选择，R04-T09/T10 关闭"。

**本轮合规声明**：只读；未修改任何源码或文档；未实施 ⑥；未读取作者解释、V1/R1 面板、其他面板或 expected/。证据强度：P2 项均附代码锚点与可复算命令输出（下附），未使用纯推断结论。

复算命令与输出（D2 维度）：

```bash
$ rg -o 'Self::(DeviceTree|Acpi|QemuVirt)' os/libs/minix-platform/src/global.rs | wc -l
25
$ rg -c 'PlatformDescEnum::(DeviceTree|Acpi|QemuVirt)' os/libs/minix-platform/src/kind.rs os/libs/minix-platform/src/global.rs
os/libs/minix-platform/src/global.rs:2
os/libs/minix-platform/src/kind.rs:4
$ grep -rc '#\[test\]' os/libs/minix-platform --include=*.rs | awk -F: '{s+=$2} END {print s}'
26
$ rg -o 'platform_sources:\s*&\[\]' os/ --type rust -g '*.rs' | wc -l
54
$ grep -c 'define ACPI_MADT_TYPE' minix3/minix/kernel/arch/i386/acpi.h
11
$ wc -l minix3/minix/kernel/arch/i386/acpi.c
410 minix3/minix/kernel/arch/i386/acpi.c
```

以上六项与文档 §3.2/§4.3/§4.4/§4.10/§6.2 的数值一致；本轮没有发现计数漂移，差异集中在语义、边界与死抽象，而不是数字。


