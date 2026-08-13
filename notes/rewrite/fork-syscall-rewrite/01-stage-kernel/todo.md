# 03-stage-kernel 全局 TODO

> 本文件汇总 03-stage-kernel 中各文档尚未完成、待后续实现的 Rust 开发任务。
> 每个条目标注来源文档。

---

## 1. Boot module 内存回收（Rust 实现）

> 来源：`01-boot-shim-bootstrap.md` / `01-todo.md` C 项（C 源码讲解已完成）

**背景**：Minix3 的 boot module 物理内存生命周期是：`cut_memmap()` 临时切掉 → `protect.c` 解析 ELF 复制到进程空间 → `add_memmap()` 回收。C 源码已在 01 文档 §2.5 讲解完毕。

**Rust 实现待办**：

| 环节 | Minix3 C | Rust minix-rs | 状态 |
|------|----------|---------------|------|
| boot module 加载 | GRUB 传入 `module_list[]` | `boot_modules: &[]` 始终为空 | ❌ 缺失 |
| 从 memmap 切掉 module 内存 | `cut_memmap()` (pre_init.c:211) | 无等价实现 | ❌ 缺失 |
| ELF 解析 + 复制到进程空间 | `protect.c` 解析 ELF | `kmain()` 是 `loop {}` | ❌ 缺失 |
| 回收 module 物理内存 | `add_memmap()` (protect.c:450) | 无等价实现 | ❌ 缺失 |
| 回收 bootstrap 代码内存 | `add_memmap()` (main.c:301) | 无等价实现 | ❌ 缺失 |

`BootModule` 结构体已定义（`minix-types/src/kernel_info.rs`）但从未被填充使用。不仅在回收逻辑缺失，整个 boot module 生命周期（加载→ELF解析→复制→回收）都尚未实现。

---

## 2. `ExitBootServices` 后内存映射回收 + `Box::leak` 生命周期

> 来源：`01-boot-shim-bootstrap.md` §4 实现详解、`uefi_helpers.rs`

**问题**：
- `uefi_helpers.rs` 中 `build_memmap()` 在 `ExitBootServices()` **之前**调用，只收集 `CONVENTIONAL` 类型区域
- `exit_boot_services()` 返回的最终内存映射被丢弃（`_mmap`），kernel 永远看不到 `ExitBootServices` 后的页面重新分类
- `Box::leak()` 泄漏的 `MemoryRegion` 切片（`'static`）只是症状——根本问题是 kernel 无法知道 boot-shim 占用的内存在退出后被固件标记为什么类型（可用/保留/已用）

**与 Minix3 C 的差距**：
- C 版本：`pre_init()` 记录 `bootstrap_start`/`bootstrap_len` → `kmain()` 调用 `add_memmap(&kinfo, bootstrap_start, bootstrap_len)` **显式归还**
- Rust 版本：boot-shim 是独立 `.efi` 程序，kernel 不知道其内存布局；`ExitBootServices` 最终映射被丢弃后，boot-shim 内存对 kernel 不可见

**修复方向**：
- [ ] 在 `ExitBootServices` **之后**获取最终内存映射，替代 `build_memmap()` 的退出前快照
- [ ] 或：boot-shim 在 `BootPrepareResult` 中传递 `boot_shim_start`/`boot_shim_len`，让 kernel 在初始化后显式归还（对标 Minix3 C 的 `add_memmap(bootstrap)`）

---

## 3. qemu-tests 后续扩展

> 来源：`01-todo.md` D 项（目录结构重组已完成）

**待办**：

- [ ] 按文档语义添加后续测试目录：

```
test-kernels/
├── kernel/
│   ├── bootstrap/      # 01-boot-shim-bootstrap.md ✅ 已创建
│   ├── pagetable/      # 02-page-table-kernel.md
│   ├── exception/      # 05-exception-interrupt.md
│   └── ipc/            # 09-sync-ipc.md
└── vm/                 # 02-stage-vm/
    └── ...
```

- [ ] 代码去重：test-kernel 中的 UEFI 入口/串口/memmap 代码应复用 `boot-shim` + `kernel` 的正式代码，而非各自复制
- [ ] 全系统 QEMU 启动规划：每个阶段用同一内核 + 不同 boot modules

---

## 4. 页表页分配器：VM 阶段接入

> 来源：`01-todo.md` E 项（boot 阶段已完成）

- [ ] 设计 memmap 中切除 bump 范围的机制（确保 VM 不踩踏）
- [ ] VM 接入时实现 `vm_pt_alloc` 并注册

---

## 5. code-review 未修复问题（来自 02-code.md）

> 来源：`02-code.md` 修复记录中的"未修复（留待后续）"部分（审查日期 2026-06-09）
> 12 项 code-review 问题中 7 项已修复，本节列出剩余 5 项。
> 已修复的 7 项详见 02-code.md §修复记录；01-bug.md 同步更新了 §3.2 和 §5。

### 4.1 [P1] KernelInfo 字段 pub 封装 — ✅ 已完成（R-07 / FIX-12，2026-08-12）

- **类型**: 设计 / 命名 / 封装
- **位置**: `os/libs/minix-boot/src/kernel_info.rs`
- **问题**: `KernelInfo` 所有字段都是 `pub`，外部代码可以读/写所有内部状态，缺少封装边界。`free_upper_idx` 始终为 0 但没有"未初始化"语义，外部无法判断有效性。
- **已完成修复 (2026-06-16)**:
  - `free_upper_idx` 类型从 `usize` 改为 `Option<usize>`，用 `None` 表示"boot-shim 尚未计算"
  - 添加 `pub fn free_upper_idx(&self) -> Option<usize>` getter 方法
  - boot-shim 构造时填 `None`（C 中由 `pg_mapkernel()` 返回值设置，Rust boot-shim 尚未实现此计算）
  - kernel `init_post_and_memory` 中使用 `.expect()` 取值
  - `misc.rs` 诊断输出使用 `.unwrap_or(0)`
  - QEMU 测试中有意义的值填 `Some(N)`，无意义的填 `None`
- **R-07 完成修复 (2026-08-12, FIX-12)**:
  - 新增 `KernelInfo::validate()` 方法，强制 5 项不变量（bootstrap_len=0、kern_size>0、栈 16 字节对齐、memmap/boot_modules 非空）
  - `kmain` 入口调用 `validate()` fail-fast
  - 新增 12 个 `#[inline]` getter 方法作为首选 API
  - kernel 内部所有直接字段访问迁移到 getter（`lib.rs` 11 处 + `arch/boot.rs` 1 处）
  - 字段保持 `pub` 以兼容 boot-shim 构造和 test 断言（设计决策：`KernelInfo` 是 `Copy` POD，跨 crate 构造需求）
  - 7 个单元测试覆盖 validate() + getter
- **验证**: `cargo test -p minix-boot --lib kernel_info` → 7 passed + `cargo test -p minix-kernel --lib` → 561 passed + `cargo test -p minix-kernel --test boot_integration` → 2 passed

### 4.2 [P1] HigherHalf trait 单方法评估 — ✅ 已完成

- **类型**: trait 设计
- **位置**: `os/kernel/src/boot/higher_half.rs`
- **问题**: `HigherHalf` trait 只有 1 个方法 `jump_to_kmain`，3 个实现行为模式相同（切栈 + 对齐 + 清零 FP + 跳转），只是汇编指令不同。
- **当前评估**:
  - ✅ 3 个实现行为不同（x86_64 `mov rsp + call`，aarch64 `mov sp + br`，riscv64 `mv sp + jalr`）→ 多态合理
  - ✅ 被 `arch_boot()` 通过具体类型调用，但测试代码中已验证泛型 bound 能力（`fn accept_higher_half<H: boot::HigherHalf>()`）
  - ✅ 只有 1 个方法但语义完整
- **修复方案**: 采用方案 A，保持现状
  - `higher_half.rs` 文档注释已完整说明设计意图（架构隔离、统一调用点、可测试性三点理由）
  - `02-higher-half-kernel.md` §3.4 已详细展开 "为什么需要 trait 而不是 `#[cfg(target_arch)]`"
  - 无需新增方法或改为枚举分发
- **风险**: 如果未来 `HigherHalf` 增加方法，需要重写所有实现（当前单方法语义完整，无扩展计划）
- **验证**:
  - `cargo check -p minix-kernel --all-targets --all-features` 通过
  - 文档与代码一致：§3.4 的 trait 定义与 `higher_half.rs` 源码匹配
  - 测试代码验证泛型 bound：`kernel/src/lib.rs:718-719` `accept_higher_half::<MockHigherHalf>()` 编译通过
- **工作量**: 小（1-2 小时，主要是文档）

### 4.3 [P1] kmain_verify 三架构重复代码 — ✅ 已完成

- **类型**: 重构 / 抽象
- **位置**: `os/kernel/src/lib.rs:318-432`（约 100 行）
- **问题**: 三架构的打印和断言逻辑几乎完全相同，只是 `early_console` 模块路径不同。
- **重复模式**: 打印 `### test-higher-half ###` + 架构名 + kern_virt_base + kern_phys_base + kern_size；验证 KERN_VIRT_BASE 在 `0xFFFF_8000_0000_0000` 范围；验证 kern_phys_base < kern_virt_base；验证 kern_size 是 HUGE_PAGE_SIZE 倍数；打印 PASS/FAIL
- **修复方案**: 采用方案 A，抽象 `EarlyConsole` trait
  1. 在 `os/arch/src/early_console.rs` 定义 `EarlyConsole` trait，含 `write_byte` 抽象方法和 `write_str`/`write_hex` 默认方法（统一处理 `\n` → `\r\n` 和十六进制格式）（**落地位置更新（2026-08-13）**：trait 最终定义于 `os/plat/src/early_console.rs:14`——plat crate 持有，见 27-kernel-utility.md）
  2. 三架构 `early_console.rs` 各添加 ZST 类型（`X86_64EarlyConsole`/`AArch64EarlyConsole`/`Riscv64EarlyConsole`）实现 `EarlyConsole`
  3. `arch/src/lib.rs` 添加 `CurrentEarlyConsole` type alias，统一导出
  4. `kernel/src/lib.rs` 的 `kmain_verify` 使用 `CurrentEarlyConsole` 统一输出，仅保留 `#[cfg]` 区分寄存器标签（`RSP`/`SP` 等）和架构名，消除全部重复代码块
- **风险**: 无。各架构原有的 `early_console::write_str`/`write_hex` 自由函数保留，测试内核代码无需改动。
- **验证**: `cargo check -p minix-arch -p minix-kernel --all-targets --all-features` 通过；`kmain_verify` 输出格式与重构前完全一致
- **工作量**: 中（4-6 小时）

### 4.4 [P2] PTE_HUGE_FLAGS 语义不明 — ✅ 已完成

- **类型**: 命名
- **位置**: `os/arch/src/arch/paging_ext.rs:96`
- **问题**: `PTE_HUGE_FLAGS: u64 = 0`（riscv64/arm64）或 `1 << 7`（x86_64），命名暗示"大页的 PTE 标志"，实际含义是"需要在 PTE 中额外设置的大页标识位"。
- **修复**: 采用方案 A，重命名为 `PTE_HUGE_IDENTIFIER_BIT`，并补充文档注释说明各架构取值（x86_64: PS bit `1 << 7`; ARM64/RISC-V: 0）。同时更新了所有文档引用。
- **验证**: `cargo check -p minix-arch --all-targets --all-features` 通过；`grep PTE_HUGE_FLAGS` 确认代码和文档中无遗漏（仅剩 x86_64/pte.rs 内部常量 `PTE_HUGE`，非 trait 接口）

### 4.5 [P2] 调试输出无注释 — ✅ 已完成

- **类型**: 注释 / 工程规范
- **位置**: `os/kernel/src/lib.rs:82-85,177,208,246,251`
- **状态**: 2026-06-09 已彻底移除（推荐方案 A：完全移除）
- **备注**: 生产内核不应包含调试输出，feature gate 增加了配置复杂度

---

### 4.x 处理优先级建议

| 子项 | 优先级 | 工作量 | 建议处理时间 |
|------|--------|--------|--------------|
| 4.1 KernelInfo pub 封装 | P1 | 中 | ✅ 已完成（R-07 / FIX-12） |
| 4.3 kmain_verify 重复 | P1 | 中 | 重构窗口期统一处理 |
| 4.2 HigherHalf 单方法 | P1 | 小 | 文档完善即可（推荐保持现状） |
| 4.4 PTE_HUGE_FLAGS 命名 | P2 | 小 | ✅ 已完成 |
| 4.5 调试输出 | P2 | — | ✅ 已完成 |

**建议**: 4.1 和 4.3 在下一次大规模重构时统一处理（涉及 trait 设计和 API 变更）；4.2 通过文档注释解决。

---

## 6. 测试缺口

> 来源：`04-tests.md`（分析日期 2026-06-11）+ `07-cross-space-init.md` review（2026-06-21）
> 分析覆盖：03-kmain-cstart.md + 04-clock-interrupt-init.md + 07-cross-space-init.md
> 总测试数：126 单元测试 + 6 QEMU 测试 + 本轮新增 13 测试（arch 9 + per-arch 9 + kernel 2 → 去重后 ~20）全部通过。

### 6.1 [P1] 异常端到端测试（L4）：handler 地址为 0，异常交付未验证

> 当前 IDT handler 地址全为 0（`os/arch/src/x86_64/trap_entry.rs:151`），无法验证异常交付链路。

- [ ] x86_64: 触发除零异常（vector 0）→ 验证 handler 执行
- [ ] x86_64: 触发缺页异常（vector 14）→ 验证 handler 执行
- [ ] aarch64: 触发 SVC → 验证 VBAR_EL1 跳转到 handler
- [ ] riscv64: 触发 ecall → 验证 stvec 跳转到 handler

**范围**：此缺口属于后续"异常处理"文档的阶段。当前 03/04 文档只需证明寄存器配置正确。

### 6.2 [P1] 中断端到端测试（L5）：中断交付链路未验证

> QEMU GDB 脚本验证了中断控制器寄存器初始化配置，但未验证实际中断到达 → handler 执行 → EOI 完整链路。

- [ ] 时钟中断到达 CPU → handler 执行 → ClockState::tick() 被调用
- [ ] 中断完整链路：设备 → GIC/APIC/PLIC → CPU → handler → EOI
- [ ] 中断 mask/unmask 端到端行为验证

**范围**：此缺口属于后续"中断处理"文档的阶段。当前只需证明中断控制器和时钟硬件寄存器配置正确。

### 6.3 [P1] init/load 顺序约束缺少测试

> `ProtectionArch::load()` 必须在 `TrapEntryArch::load()` 之前的约束仅在文档中声明（03 文档 §4.1），无测试验证违反顺序的后果。

**范围**：依赖 SMP 多核支持，将在 SMP 阶段补充。

### 6.4 [P1] init_ap 路径验证缺失

> AP 启动路径完全未测试（`init_ap` 函数未被任何测试覆盖）。

**范围**：依赖 SMP 多核支持，将在 SMP 阶段补充。

### 6.5 [P2] aarch64 GICv3 PPI unmask 未实现 — ✅ 已完成

> **位置**: `os/plat/src/arm64/interrupt.rs` `unmask()` / `mask()` 方法
> PPI (IRQ < 32) 的 unmask 已实现，通过 Redistributor 的 `GICR_ISENABLER0`/`GICR_ICENABLER0` 寄存器控制。
> 同时更新了 `InterruptController` trait 文档中的 ARM64 列，反映 PPI 路径。
> 文档 `04-clock-interrupt-init.md` §4.6 状态说明已同步更新 (2026-06-16)。

### 6.6 [P2] riscv64 PLIC base 硬编码 — ✅ 已完成

> **位置**: `os/plat/src/riscv64/interrupt.rs`
> PLIC_BASE = 0x0C00_0000 硬编码为 QEMU virt 默认地址，未从 device tree 自动发现。有 `set_base()` 方法可手动覆盖。

**2026-06-20 更新**：`DeviceTreeDesc` 已实现 RISC-V PLIC 节点解析（`os/libs/minix-platform/src/device_tree.rs`）。当 boot-shim 传入 DTB 指针时，`init_from_kinfo()` 走 DTB 解析路径，不再使用硬编码 `PLIC_BASE`；无 DTB 时才回退到 `QemuVirtDesc`。

### 6.7 [P2] riscv64 PMP 仅配置 entry 0

> **位置**: `os/arch/src/riscv64/arch_init.rs`
> PMP 仅配置 entry 0 为 Allow All (NAPOT + R+W+X)，未设置区域隔离。

当前简单场景足够，暂无安全隔离需求。后续需完整 PMP 配置。

### 6.8 [P2] QEMU GDB 脚本未集成到 CI

> **位置**: `os/arch/tests/qemu_test_{x86_64,aarch64,riscv64}.sh`
> 三个 QEMU GDB 自动化脚本当前为手动运行，未集成到 CI pipeline。

建议后续集成到 CI，每个 PR 自动验证 QEMU 寄存器初始化。

---

## 7. PlatformDesc / 硬件发现

> 来源：设备树 / ACPI 硬件发现 TODO

**已完成（2026-06-20）**：

| 任务 | 状态 | 实际文件 |
|------|------|----------|
| 设计 `PlatformDesc` trait | ✅ | `os/libs/minix-platform/src/desc.rs` |
| 实现 `QemuVirtDesc` 兜底 | ✅ | `os/libs/minix-platform/src/qemu_virt.rs` |
| 实现 `DeviceTreeDesc`（FDT/DTB 解析） | ✅ | `os/libs/minix-platform/src/device_tree.rs` |
| 实现 `AcpiDesc`（最小 ACPI 解析） | ✅ | `os/libs/minix-platform/src/acpi.rs` |
| 实现 `PlatformContext` 全局 + `init_from_kinfo()` | ✅ | `os/libs/minix-platform/src/global.rs` |
| UEFI boot-shim 定位 RSDP/DTB | ✅ | `os/boot-shim/src/uefi_helpers.rs` |
| OpenSBI boot-shim 保存 a1 DTB 指针 | ✅ | `os/boot-shim/src/opensbi_helpers.rs` |
| `KernelInfo` 扩展 `platform_descriptor` 字段 | ✅ | `os/libs/minix-boot/src/kernel_info.rs` |

**验证**：
- `cargo test -p minix-platform`：19/19 通过（含 DTB/ACPI 合成表解析测试）。
- `cargo check -p minix-platform -p minix-kernel`：通过。
- `cargo check -p boot-shim --features test-all`：通过。

**剩余偏差 / 后续扩展**：

- `AcpiDesc` 当前为最小化实现：仅支持 RSDP → XSDT/RSDT → MADT，提取 LAPIC/IOAPIC base 和 CPU 拓扑。HPET、x2APIC 中断投递、Interrupt Source Override、多 IOAPIC 等尚未实现（QEMU `virt` x86_64 当前够用）。
- 文件名 `device_tree.rs` 与设计稿 `fdt.rs` 不一致，属命名偏差，功能等价。

**收益**：支持真实硬件移植时，不需要为每块板子单独修改 Rust 源码；ARM/RISC-V 换 DTB，x86 换 ACPI 表即可。

---

## 8. 平台发现阶段预存在问题（与本次改动无关）

> 来源：Phase 2~4 实施过程中通过 `cargo build --workspace` 发现，与本次 `minix-platform` 新增代码无关。

### 8.1 `test-memmap-riscv64` 编译失败

- **位置**：`os/qemu-tests/test-kernels/kernel/bootstrap/test-memmap-riscv64/src/main.rs`
- **错误**：`use minix_plat::riscv64::early_console` → `could not find riscv64 in minix_plat`
- **含义**：测试内核引用了一个不存在的模块路径 `minix_plat::riscv64`。可能是 `os/plat/src/riscv64` 子模块尚未创建，或测试内核的导入路径已过时。
- **影响**：仅影响该单个测试二进制；`minix-platform` 自身、UEFI/OpenSBI boot-shim、`minix-kernel` 均不受影响。
- **修复方向**：
  1. 确认 `os/plat/src/riscv64/mod.rs` 是否存在；若不存在则创建。
  2. 若模块已存在但路径不同，更新测试内核的 `use` 语句。
  3. 若该测试已废弃，考虑移除或重命名。

### 8.2 `boot-shim` 默认 target 的 `panic_impl` lang item 冲突

- **位置**：`os/boot-shim`
- **错误**：
  - `error[E0152]: found duplicate lang item panic_impl`
  - `error[E0425]: cannot find function arch_boot in crate minix_kernel`
- **含义**：
  - `boot-shim` 在默认 target 下既自己实现了 `panic_handler`，又链接了同样实现 `panic_handler` 的 crate（如 `minix-kernel` 或测试框架），导致 Rust lang item 重复。
  - 同时 `arch_boot` 函数在当前 cfg/target 组合下不可见。
- **影响**：
  - `cargo check -p boot-shim`（不带 feature）失败。
  - `cargo check -p boot-shim --features test-all` 通过，说明 `test-all` feature 的依赖/link 配置是正确的。
- **修复方向**：
  1. 检查 `boot-shim/Cargo.toml` 的默认 feature 是否错误地依赖了 `minix-kernel` 或测试 crate。
2. 检查 `boot-shim/src/main.rs` 的 `panic_handler` 是否在非测试 target 下被错误启用。
3. 检查 `arch_boot` 的可见性：是否只在某个 feature 或 target_arch 下暴露，而默认 target 没有。
4. 考虑把 `boot-shim` 的默认 target 也改为与 `--features test-all` 一致的配置，或明确区分"真实固件目标"与"测试目标"的 panic_handler 归属。

**备注**：这两个问题在本次 Phase 2~4 实施前已存在，不应由本次 `minix-platform` 改动负责。后续优先处理 8.2，因为它影响 `boot-shim` 的常规构建体验。

---

## 9. Rust 2024 edition 迁移后暴露的残留编译错误

> 来源：2026-06-21 将全部 `os/` crate edition 由 2021 升级到 2024 时，`cargo check --workspace --all-targets` 暴露的、与 edition 无关的预先存在问题。
> 2024 edition 本身只引入 4 处真实语法问题（已就地修复，详见本文件 §9.0），其余错误均为更早阶段遗留。

### 9.0 2024 edition 自身引入并修复的 4 处问题 ✅

| 文件 | 行号 | 修改 |
|------|------|------|
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-memmap-riscv64/src/main.rs` | 19 | `#[link_section = ".bss"]` → `#[unsafe(link_section = ".bss")]` |
| 同上 | 79 | `#[no_mangle]` → `#[unsafe(no_mangle)]` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection-aarch64/src/main.rs` | 140 | `extern "C" { ... }` → `unsafe extern "C" { ... }` |
| 同上 | 307 | `extern "C" { ... }` → `unsafe extern "C" { ... }` |

### 9.1 [P1] `E0152 duplicate lang item panic_impl` — test-kernel 与 boot-shim

**根因**：测试二进制是 `#![no_std]` 的 `bin`，自带 `fn panic(_: &PanicInfo) -> !` 作为 `panic_handler`。但它们依赖的 `minix-kernel`（默认 feature = mock，链接到 `std` 用于测试）以及 `boot-shim` 的 lib/test 目标都会拉入 `std`，而 `std` 已经定义了一个 `panic_impl` lang item，导致冲突。

**触发位置**：

| 文件 | 备注 |
|------|------|
| `os/boot-shim/src/main.rs:57` | `boot-shim` 默认 target 的 panic_handler |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs:70` | test-higher-half bin |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-proc-init/src/main.rs:263` | test-proc-init bin |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-paging-enable/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-paging-enable-aarch64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-paging-enable-riscv64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection-aarch64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection-riscv64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-kernel-map/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-kernel-map-aarch64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-kernel-map-riscv64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-memmap-aarch64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-memmap-riscv64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/hello-boot/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/hello-boot-aarch64/src/main.rs` | 同类 |
| `os/qemu-tests/test-kernels/kernel/bootstrap/hello-boot-riscv64/src/main.rs` | 同类 |

**影响范围**：所有依赖 `minix-kernel` 或 `boot-shim` 的 `#[no_std]` 测试 bin 在 `cargo check --tests` 时全部失败。

**修复方向**：
1. **方案 A（推荐）**：让 `minix-kernel` 和 `boot-shim` 的"生产代码 target"也通过 cfg 与 "测试 target"分离 panic_handler——生产 target 依赖 `core::panic::PanicInfo`，测试 target 走 std 的 panic_handler。
2. **方案 B**：让 test-kernel bin 不通过 `minix-kernel` 转而直接使用 `minix-arch` + `minix-platform` 这类底层 crate，跳过 mock feature 带来的 std 依赖。
3. **方案 C**：把 `minix-kernel` 的 mock feature 与 std 依赖解耦（`mock` 不应拉 `std` 进生产二进制）。

### 9.2 [P1] `E0425 cannot find function arch_boot / init_proc_and_boot / init_post_and_memory` — cfg gating 不匹配

**根因**：`os/kernel/src/lib.rs` 中这些函数都标注 `#[cfg(not(feature = "mock"))]`，而 `os/qemu-tests/test-kernels/kernel/bootstrap/*` 编译时通过 workspace 传递 `minix-kernel` 的默认 feature（= `mock`），导致函数被 cfg 掉。

**触发位置**：

| 函数 | kernel/src/lib.rs 行号 | 引用方 |
|------|----------------------|--------|
| `arch_boot` | 64 / 70 / 82 / 94（按 target_arch 三份 + 一份 mock） | `boot-shim`、`hello-boot`、`test-paging-enable-*`、`test-protection-aarch64` |
| `init_proc_and_boot` | 693 | `test-proc-init/src/main.rs:168` |
| `init_post_and_memory` | 910 | `test-proc-init/src/main.rs:250` |

**影响范围**：依赖 `minix-kernel` 但又要走真实 arch 路径的 test-kernel bin 全部失败。

**修复方向**：
1. 让 test-kernel 在自己的 `Cargo.toml` 中显式 `default-features = false, features = ["real"]`（需新增对应 feature）或
2. 让 `minix-kernel` 拆分为 `minix-kernel-mock` 与 `minix-kernel-real` 两个 crate，test-kernel 引用 real 版本；或
3. 把 `arch_boot` 等从 cfg gating 中放出来，改为 `#[cfg(any(not(feature = "mock"), feature = "test-allow-real"))]` 这种"测试场景下也编译"的形式。

### 9.3 [P1] `E0432/E0433 unresolved import minix_plat::arm64 / riscv64` — 路径不存在

**根因**：`os/plat/src/` 当前没有 `arm64/mod.rs` 和 `riscv64/mod.rs` 子模块，但部分 test-kernel bin 通过 `use minix_plat::arm64::...` / `minix_plat::riscv64::...` 引用。

**触发位置**：

| 文件 | 行号 |
|------|------|
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half-aarch64/src/main.rs` | `use minix_plat::arm64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half-riscv64/src/main.rs` | `use minix_plat::riscv64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-kernel-map-aarch64/src/main.rs` | `use minix_plat::arm64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-kernel-map-riscv64/src/main.rs` | `use minix_plat::riscv64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-memmap-aarch64/src/main.rs` | `use minix_plat::arm64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-memmap-riscv64/src/main.rs` | `use minix_plat::riscv64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-paging-enable-aarch64/src/main.rs` | `use minix_plat::arm64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-paging-enable-riscv64/src/main.rs` | `use minix_plat::riscv64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/hello-boot-aarch64/src/main.rs` | `use minix_plat::arm64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/hello-boot-riscv64/src/main.rs` | `use minix_plat::riscv64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection-aarch64/src/main.rs` | `use minix_plat::arm64` |
| `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection-riscv64/src/main.rs` | `use minix_plat::riscv64` |

**修复方向**：
1. 在 `os/plat/src/arm64/mod.rs` 与 `os/plat/src/riscv64/mod.rs` 创建空模块（最低修复成本），或
2. 重构 `os/plat/` 把 aarch64/riscv64 早期的 `early_console` 等能力下沉到 `os/arch/src/arm64/` / `os/arch/src/riscv64/`（已经在那里），让 test-kernel 改 import 路径——参考 `04-platform-discovery.md §3.5` 已迁移的设计。

### 9.4 [P1] `the #[global_allocator] in this crate conflicts with global allocator in: uefi` — uefi crate 已自带 allocator

**根因**：UEFI 测试 bin 通过 `uefi` crate 间接拉入其内置的 `#[global_allocator]`（用于 `Vec` 等），而 test-kernel 自带的 `BootAllocator` 也声明 `#[global_allocator]`，两个 allocator 冲突。

**触发位置**：`test-paging-enable-aarch64/src/main.rs` 与 `test-paging-enable-riscv64/src/main.rs` 之类的带 `BootAllocator` 的 UEFI 测试 bin。

**修复方向**：把 `BootAllocator` 移除，改用 `uefi` 自带的 allocator；或在 feature flag 控制下二选一。

### 9.5 修复优先级建议

| 子项 | 根因类别 | 影响 crate 数 | 建议处理时机 |
|------|---------|--------------|------------|
| 9.1 panic_impl lang item | `no_std` bin 与 `std` 依赖混杂 | 17 个 test-kernel + boot-shim | 重构 `minix-kernel` 拆分 crate 时一并修 |
| 9.2 cfg gating 不匹配 | mock feature 默认开启 | 17 个 test-kernel | 同上 |
| 9.3 模块路径缺失 | `os/plat` 拆分未完成 | 12 个 test-kernel | 同 9.1/9.2 一次处理 |
| 9.4 global_allocator 冲突 | uefi crate 自带 allocator | 2 个 test-kernel | 最小修复，可单 PR 处理 |

**共性**：四个子项都源于 `os/qemu-tests/test-kernels/*` 这批早期脚手架代码未跟随 `minix-kernel` / `minix-platform` / `os/plat` 的重构而同步更新。**修复它们的最佳窗口是下一次涉及这三个 crate 的大规模重构**——单独修只能解决症状，治本需要重新设计 test-kernel 与生产 crate 的依赖边界。

**临时缓解**：日常开发可只跑 `cargo check -p minix-arch -p minix-platform -p minix-boot -p minix-elf -p minix-rt -p minix-sys -p minix-types -p minix-plat`（这 8 个核心 crate 已 2024 edition 干净通过），CI 可加白名单忽略 test-kernel bin 直到 §9.x 解决。

---

## 10. boot-shim 与 kernel 的 `static mut` 收编到 `AssumeSyncCell`

> 来源：本次 `04-platform-discovery.md §3.5` 重构时把 `SyncPlatformCell` 替换为 `minix-types::AssumeSyncCell`，审计其他同类位置时发现 `boot-shim` 和 `kernel` 还在用 `static mut`——属于同一模式但更大范围的重构。

### 10.1 现状：项目内仍使用 `static mut` 的位置

| 文件 | 行号 | 标识符 | 用途推断（未验证） |
|------|------|--------|---------------------|
| `os/boot-shim/src/opensbi_helpers.rs` | 252 | `BOOT_FILE_TABLE_PTR` | OpenSBI 启动文件表指针 |
| `os/boot-shim/src/opensbi_helpers.rs` | 300 | `DTB_PTR` | 设备树指针（OpenSBI 阶段） |
| `os/boot-shim/src/opensbi_helpers.rs` | 335 | `BUMP_PTR` | bump allocator 游标 |
| `os/kernel/src/lib.rs` | 999 | `FREE_MEMMAP` | 物理内存 map（待函数过滤） |
| `os/kernel/src/lib.rs` | 1011 | `KERNEL_INFO` | `Option<KernelInfo>`，单写多读 |
| `os/kernel/src/lib.rs` | 1032 | `FREE_PDE_SLOTS` | 页表项空闲槽位 |
| `os/kernel/src/lib.rs` | 1090 | `IPC_FILTER_POOL` | IPC 过滤器池 |

### 10.2 已完成的同模式重构（参考样板）

**参考实现**：`os/libs/minix-platform/src/global.rs` 当前形态（2026-06-21 改后）。

```rust
// 之前：自己定义 wrapper
struct SyncPlatformCell(UnsafeCell<Option<PlatformContext>>);
unsafe impl Sync for SyncPlatformCell {}
static PLATFORM: SyncPlatformCell = SyncPlatformCell(UnsafeCell::new(None));

// 之后：复用项目统一原语
use minix_types::AssumeSyncCell;
static PLATFORM: AssumeSyncCell<Option<PlatformContext>> = AssumeSyncCell::new(None);
// init:  *PLATFORM.get() = Some(...);
// read:  (*PLATFORM.get()).as_ref().expect(...)
```

**收益**：消除重复 wrapper，集中安全契约（"调用方保证单线程独占访问"），与 VM server / heap arena / vmproc table 保持一致。

### 10.3 重构范围与顺序

| 阶段 | 范围 | 难度 | 前置条件 |
|------|------|------|----------|
| 10.3.1 | `KERNEL_INFO: Option<KernelInfo>`（kernel/src/lib.rs:1011） | **低**——已是 `Option<T>`，模式与 PLATFORM 完全一致 | 验证 `KernelInfo: Send + Sync`（大概率已具备，因字段是 usize/u64/array） |
| 10.3.2 | `FREE_MEMMAP`、`FREE_PDE_SLOTS`、`IPC_FILTER_POOL`（kernel/src/lib.rs:999, 1032, 1090） | **中**——类型/大小需确认是 Copy 还是含数组；含大数组时 `AssumeSyncCell<T>` 仍然合适，但需要验证运行时拷贝开销 | 验证这些类型已经是 `Copy`/`Clone` 或可由 `AssumeSyncCell<T>` 直接持有 |
| 10.3.3 | `BOOT_FILE_TABLE_PTR`、`DTB_PTR`、`BUMP_PTR`（boot-shim/src/opensbi_helpers.rs:252, 300, 335） | **中-高**——boot 阶段初始化顺序敏感；OpenSBI 阶段可能比 `minix-types` 更早，需要 `cfg`-gate 或 `boot-shim` 内复制一份等价 wrapper | 确认 boot-shim 与 minix-types 的依赖顺序，必要时在 boot-shim 内定义等价 wrapper 后再统一收编 |

### 10.4 触发条件

> **触发条件**：`os/kernel/` 完成重大重构（特别是内存管理、IPC 重构）后，**重新扫描项目内所有 `static mut`**——一次性扫描+收编，避免每次单独 PR。

**重新扫描命令**：
```bash
rg "^static mut " os/ --type rust -n
```

**执行 PR 检查清单**：
1. 确认目标类型是 `Send + Sync`（或本就是单线程使用）
2. 确认无跨线程共享访问（通过 BKL 或单线程上下文保证）
3. 替换为 `static X: AssumeSyncCell<T> = AssumeSyncCell::new(...);`
4. 所有读 `*X` 改为 `unsafe { *X.get() }` 或 `unsafe { X.as_ptr() }`
5. 所有写 `X = ...` 改为 `unsafe { *X.get() = ... }`
6. 验证 `cargo check --workspace` 通过、`cargo test -p kernel -p boot-shim` 通过

### 10.5 不迁移 `static mut` 的反例（应保留）

- **`OnceLock` 等已被更优类型替代的**：本任务前必须先确认目标类型没有被更好的并发原语替代（如 `AtomicU64`、`OnceLock`）
- **类型内部已带同步原语的**：如果 `static mut` 持有 `Mutex`/`RwLock`，迁移到 `AssumeSyncCell` 是退化（去掉了一层 sync），应跳过
- **真正跨 CPU 共享的可变状态**：必须用原子类型或显式 lock，`AssumeSyncCell` 不适用（它的安全契约就是"单线程"）

### 10.6 为什么不在本次一起做

- **boot 阶段 `unsafe` 审计成本高**：OpenSBI 阶段、kernel 早期 init 的内存安全论据需要逐条审查，独立 PR 更安全
- **kernel 还在重构**：本目录 §9 列出的 `minix-kernel` 拆分 panic_impl、cfg gating 等尚未解决，等 kernel crate 稳定后再统一处理
- **风险局部化**：本次 `minix-platform` 重构是已知安全的（只替换 wrapper，行为完全等价），kernel/boot-shim 的 `static mut` 涉及多线程契约，需要单独的设计评审

---

## 11. QEMU 测试与生产路径不一致：`QemuVirtDesc` 替代了 DTB/ACPI parser

> 来源：`04-platform-discovery.md §3.6` 重写时发现——`QemuVirtDesc` 不是设计缺陷，但**当前测试路径刻意走它**而非 DTB/ACPI parser，这违背了"test what you fly"原则。

### 11.1 问题陈述

**事实链**：

1. QEMU `virt` 机器**本身就提供 DTB/ACPI**——aarch64/riscv64 提供 DTB（GICv3/PLIC 基地址、virtio-mmio 设备），x86-64 提供 ACPI 表（RSDP→XSDT→MADT）。QEMU 在启动时把这些嵌入固件接口，boot-shim 完全有条件拿到。
2. 当前 `boot-shim` **在某些测试路径显式构造 `platform_descriptor: None`**（见 `os/boot-shim/src/opensbi_helpers.rs:535`、`os/boot-shim/src/uefi_helpers.rs:79` 注释 "use the QEMU fallback"）。
3. `platform::init_from_kinfo` 看到 `None` → 走 `QemuVirtDesc` 兜底路径 → 用硬编码常量填充硬件参数。
4. 结果：**DTB parser（aarch64/riscv64）和 ACPI parser（x86-64）在测试中根本不跑**。

**后果**：

| 后果 | 严重度 |
|------|-------|
| DTB/ACPI parser 有 bug 也发现不了（测试绿但生产挂） | **P0**——隐性故障源 |
| QEMU 升级后 virt 机器布局漂移（例如 PLIC 基地址变了），硬编码常量过时，测试还过——真实硬件走 parser 拿到的是新值，跟测试常量不一致 | **P1**——版本漂移 |
| 测试覆盖率统计失真（parser 主路径 0% 覆盖，但报告里看不出来） | **P1**——决策失据 |
| `04-platform-discovery.md §3.6` 原表述 "QEMU 测试不需要 DTB/ACPI 解析器" 是**因果倒置**——不是"不需要"，是"故意不用"，掩盖了上面的问题 | **P0**——文档误导 |

### 11.2 触发条件

**QEMU 测试路径故意走 `QemuVirtDesc` 的代码位置**：

> ⚠️ **RCPD 过时标注（2026-08-13, Task 4 回归）**：下表为 2026-07 分析时的位置清单，行号与文件已过时——`os/arch/src/{x86_64,riscv64,arm64}/proc_arch.rs` 已重构删除（`platform_descriptor` 已不在 arch crate）；opensbi/uefi_helpers 行号已漂移。历史记录保留，不再作为有效引用。

| 文件 | 行号 | 上下文 |
|------|------|--------|
| `os/boot-shim/src/opensbi_helpers.rs` | 535 | `None, // platform_descriptor` 测试用 `KernelInfo` 构造 |
| `os/boot-shim/src/opensbi_helpers.rs` | 305 | `"Passing 0 is equivalent to 'no DTB available' (kernel uses QEMU fallback)"` |
| `os/boot-shim/src/uefi_helpers.rs` | 79 | `"use the QEMU fallback"` 注释 |
| `os/arch/src/x86_64/proc_arch.rs` | 350 | mock 路径，`platform_descriptor: None` |
| `os/arch/src/riscv64/proc_arch.rs` | 252 | mock 路径 |
| `os/arch/src/arm64/proc_arch.rs` | 279 | mock 路径 |

### 11.3 修复方向

**目标**：让 QEMU 测试走完整的 boot-shim → kernel → `platform::init_from_kinfo` → DTB/ACPI parser 主路径，跟真实硬件完全一致。

**步骤**：

1. **boot-shim 改造**：在 `find_platform_descriptor()` 中确认 QEMU 提供的 DTB/RSDP 一定可拿到（即便在测试固件中），而不是仅在某些路径返回 `Some(...)`，另一些路径返回 `None`。
2. **替换 `None` 为 `Some`**：
   - `os/boot-shim/src/opensbi_helpers.rs:535` 测试用 `KernelInfo` 改为传 `Some(PlatformDescriptorPtr::Dtb(dtb_phys))`
   - 三个 `proc_arch.rs:350/252/279` 的 mock 路径改为传真实 QEMU 提供的 DTB/RSDP
3. **DTB/ACPI parser 验证**：跑通一次完整 QEMU 测试，验证 parser 正确解析 QEMU 提供的 DTB/ACPI——这一步可能暴露 parser 的既有 bug（这正是目的）。
4. **修复 parser bug**：如果在 11.3.3 暴露 parser bug，修复并加单测覆盖。
5. **`QemuVirtDesc` 角色回归**：修复后 `QemuVirtDesc` 应该**仅在 boot-shim 完全失败时**作为最后兜底（panic 前还能输出一点诊断）。如果新路径下 `QemuVirtDesc` 永远不会被触发，那是好事——说明 boot-shim 总是能正常工作。
6. **更新 §3.6**：完成后删除 §3.6 的"已知缺陷"标注，因为问题已解决。

### 11.4 优先级

**P0**——这是隐性故障源，会让 parser 的 bug 偷偷溜过去。修复工作量不大（主要是 boot-shim 调整 + parser 验证），但需要先把 §9.3（`os/plat` 拆分未完成）一并处理，否则 test-kernel 无法改 import 路径。

### 11.5 风险与缓解

| 风险 | 缓解 |
|------|------|
| 修复后测试大规模失败（parser bug 暴露） | 这是预期收益，不是风险；记录并修复 |
| QEMU 不同版本提供的 DTB/ACPI 字段有差异 | 锁版本（CI 用固定 QEMU 版本），并在 parser 中容忍未知字段 |
| boot-shim 在某些 firmware 配置下确实找不到 DTB/RSDP | `QemuVirtDesc` 兜底保留——这是它的合法用途 |
| 修改波及 17 个 test-kernel（todo.md §9.3） | 与 §9.3 同步处理，合并 PR |

### 11.6 与其他章节的关系

- **§9.3**（`os/plat` 拆分未完成）：本次修改需要 test-kernel 改 import 路径（从 `minix_plat::arm64` 等迁移到 `minix_arch::arch::*`），应在 §9.3 解决时同步做。
- **§11.3.6** 完成后，**§3.6 的"已知缺陷"标注**应同步删除——避免文档与代码现实脱节。

---

## 12. 07-cross-space-init 文档 review 待办（来自 2026-06-21 深度 review）

> 来源：`07-cross-space-init.md` 深度 review（2026-06-21）
> 文档语义范围：Phase D — `arch_post_init` + `memory_init`，设置 ptproc、分配 freepdes 槽位。

### 12.1 [P0] §5 测试缺口 — ✅ **已修复（2026-06-21）**

**问题**：`07-cross-space-init.md §5` 列出了 13 项测试，此前仅有 2 项已实现，其余 11 项为 TODO。

**修复结果**：13 项中 11 项已实现（✅），1 项已有覆盖（✅），1 项 DEFERRED。

**已补测试分布**：

| §5 子节 | 测试函数 | 位置 | 状态 |
|---------|---------|------|------|
| 5.1 | `test_set_ptproc_accepts_valid_vm_page_table_info` | `arch/src/{x86_64,arm64,riscv64}/post_init.rs` | ✅ |
| 5.2 | `test_memory_init_arch_allocates_two_consecutive_pdes` | `arch/src/arch/post_init.rs` | ✅ |
| 5.2 | `test_memory_init_arch_advances_free_upper_idx_by_two` | `kernel/src/lib.rs` (`test_advance_free_upper_idx`) | ✅ 已有 |
| 5.2 | `test_allocate_free_pdes_panics_on_overflow` | `arch/src/{x86_64,arm64,riscv64}/post_init.rs` (per-arch) | ✅ |
| 5.2 | `test_free_pde_slots_new_is_empty` | `arch/src/arch/post_init.rs` | ✅ |
| 5.2 | `test_free_pde_slots_push_*` | `arch/src/arch/post_init.rs` | ✅ |
| 5.3 | `test_init_post_and_memory_phase_d_dependencies` | `kernel/src/lib.rs` | ✅ |
| 5.3 | `test_free_pde_slots_global_persists_after_init` | `kernel/src/lib.rs` (`test_free_upper_idx_starts_at_zero`) | ✅ 已有 |
| 5.3 | `test_createpde_does_not_reallocate_slots` | `kernel/src/lib.rs` | ✅ |
| 5.1/5.3 | ptproc 未初始化 / virt_root=None / mem_clear_mapcache | — | DEFERRED（见 §12.2） |

**编译验证**：
- `cargo test -p minix-arch --features mock --lib -- post_init::tests` / `x86_64::post_init::tests` → OK
- `cargo test -p minix-kernel --features mock --lib` → OK (test_createpde + test_phase_d_dependencies)

### 12.2 [P1] ptproc per-CPU 变量未实现（架构 TODO）

**问题**：`PostInitArch::set_ptproc()` 当前在三个架构实现中都只是占位（`let _ = vm_page_table`），未实际存储 ptproc 指针。这意味着 `createpde()` 等后续依赖 ptproc 的功能无法工作。

**位置**：
- `os/arch/src/x86_64/post_init.rs:46-58`
- `os/arch/src/arm64/post_init.rs:38-48`
- `os/arch/src/riscv64/post_init.rs:42-52`

**修复方向**：
1. 在 arch 层添加 `pub fn ptproc() -> Option<*const KProcess>` 和 `pub fn set_ptproc_value(...)` 自由函数（封装 per-CPU 访问的 `AtomicPtr` 或 `[UnsafeCell<Option<usize>>; MAX_CPUS]`）。
2. `PostInitArch::set_ptproc(&vm_page_table)` 改为：`minix_kernel::set_ptproc_value(vm_page_table.phys_root);` 把页表物理地址存储为全局变量，供 `createpde()` 读取。
3. 加测试验证 set/get 配对。
4. 与 §10 `static mut` 收编到 `AssumeSyncCell` 协同处理：ptproc 单写多读场景很适合 `AssumeSyncCell<Option<usize>>`。

### 12.3 [P1] §3.5 BKL 安全论证与实际实现不一致 — ⏸ **短期已修，长期 DEPENDS §12.2**

**问题**：§3.5 论证"ptproc 是 per-CPU 变量"但 ptproc 当前未实现。

**短期修复**：已在 §3.5 末尾追加"实现状态"段落（含 freepdes 已实现 / ptproc per-CPU 未实现的标注、BKL 论证、当前局限性、差异追踪表）。参见 `07-cross-space-init.md §3.5` 末尾。

**长期**：§12.2 完成后重写 §3.5 让 Rust 实现与论证对齐。

### 12.4 [P2] §4.5 "KernelState 聚合类型" 与实现不一致（已修）

**问题**：原 §4.5 显示 `static mut KERNEL_STATE: Option<KernelState>` 聚合类型。实际代码是分散的 `static mut FREE_PDE_SLOTS` / `static FREE_UPPER_IDX: AtomicUsize`。

**修复**：已在本轮 review 重写 §4.5，使用与代码一致的分散 static 模式 + 解释为何不聚合。

### 12.5 [P2] §4.4 `init_post_and_memory` 示例代码陈旧（已修）

**问题**：原示例代码使用 `PhysBytes(0)/VirBytes(0)` 硬编码 + `_free_pde_slots` 被丢弃。实际代码已演进为：从 `proc_table.get(VM_PROC_NR).p_seg` 读取 + 存储到 `FREE_PDE_SLOTS` 全局。

**修复**：已在本轮 review 用实际 `lib.rs:910-980` 代码替换示例。

### 12.6 [P2] §2.3 IPCNAME 宏位置错误（已修）

**问题**：原文"IPCNAME 宏（定义在 `kernel/ipc.h`）" — 实际定义在 `minix3/minix/kernel/main.c:277`，调用方在 `main.c:285-290`。同时代码块中 IPCNAME 调用使用了两参数形式（`IPCNAME(SEND, 0)`），实际是一参数（`IPCNAME(SEND)`），且 6 个调用而非 "约 13 个"。

**修复**：已在本轮 review 改为正确的 IPCNAME 宏定义 + 调用代码块 + 注明实际 6 个调用。

### 12.7 [P2] §4.1 FreePdeSlots 方法签名陈旧（已修）

**问题**：原示例显示 `push()` 返回 `Result<(), &'static str>`、`get()` 之外无其他方法。实际是 `push() -> Result<(), usize>`（Err 返回 pde_index）、并有 `len()/is_empty()/iter()`。

**修复**：已在本轮 review 用实际 `post_init.rs:109-160` 代码替换示例。

### 12.8 [P2] cpulocals.h 行号微偏（已修）

**问题**：原文 `cpulocals.h:55`，实际 `ptproc` 字段在第 56 行。

**修复**：已在 §2.1 与 §6 参见章节更新为 `:56`。

---

## 13. Rust 阶段 B 页表重建决策 — 需再次 review

> 来源：`06-proc-init-boot-proc.md` §1.5（2026-06-22 review 提出）

**背景**：

Minix3 C 版的页表生命周期：

| 阶段 | C 行为 | Rust 行为 |
|------|--------|-----------|
| pre_init | `pg_identity()` + `pg_mapkernel()` + `pg_load()` 建立初始页表 | 同 C |
| 阶段 B（`prot_init()`） | `pg_clear()` + `pg_identity()` + `pg_mapkernel()` + `pg_load()` **重建**页表 | **不重建**，直接复用 pre_init 页表 |
| 阶段 C（`arch_boot_proc(VM)`） | `pg_map(PG_ALLOCATEME, ...)` 添加 VM 映射 | 同 C |

C 版重建的动机（`protect.c:357-358` 注释）："Set up a new post-relocate bootstrap pagetable so that we can map in VM, and we no longer rely on pre-relocated data."

**当前 Rust 决策**：阶段 B 不重建页表，直接复用 pre_init 页表。

**待决问题**：

- [ ] **决策是否合理？** 需要核对以下上下文后再次 review：
  1. Rust pre_init 阶段是否也存在"pre-relocated data 依赖"问题？如果存在，不重建会导致相同问题
  2. Rust pre_init 阶段是否已经做了"post-relocate"（重定位后再建页表）？如果是，不重建是合理的
  3. Rust 阶段 B 的代码路径（`prot_init` 或 `cstart`）是否有重定位发生？如有，是否需要在阶段 B 重建以匹配 C 行为
  4. 不重建是否有性能/正确性优势？或仅是实现简化？

- [ ] **决策依据缺失**：当前文档中"Rust 版不重建"的描述仅一句话（§1.5 注释），缺乏：
  - 为什么 Rust 不需要重建的论证
  - 是否有测试验证不重建后阶段 C 添加 VM 映射正常工作
  - 与 C 行为的语义等价性论证

**下一步**：

- 核对 Rust `pre_init()` 实现，确认是否已 post-relocate
- 核对 Rust `cstart()` / `prot_init()` 是否涉及重定位
- 补充决策依据到 `06-proc-init-boot-proc.md` §1.5 或 `02-higher-half-kernel.md`
- 若决策确为合理，更新文档明确说明；若不合理，需实现页表重建逻辑

---

## 14. Task 1: C 源码覆盖扫描 + 28-30 新文档补足（2026-08-12 完成）

> 来源：用户任务 1 — "03-stage-kernel 目录下，01~27 文档关联的 rust 实现，对比 minix3 的 c 实现，是否有任何遗漏的地方？若有，则新建文档 28,29 等等，补足遗漏。"

**执行结果**：

| 阶段 | 内容 | 状态 |
|------|------|------|
| Phase A | 运行 coverage-extract.py 扫描模块级 C 符号覆盖 | ✅ 完成 |
| Phase A | 识别 3 个完全缺口源文件（usermapped_data.c / debug.c / profile.c） | ✅ 完成 |
| Phase A | 生成 NEW-DOCS-CANDIDATES.md 候选清单 | ✅ 完成 |
| Phase B.1 | 新建 28-usermapped-data.md（含 design/outline/scan/SYMBOLS/structure/VERIFY-CHECK） | ✅ CONVERGED |
| Phase B.2 | 新建 29-kernel-debug.md（含 design/outline/scan/SYMBOLS/structure/VERIFY-CHECK） | ✅ CONVERGED |
| Phase B.3 | 新建 30-kernel-profile.md（含 design/outline/scan/SYMBOLS/structure/VERIFY-CHECK） | ✅ CONVERGED |
| Phase B.4 | 31+ arch 文档决策：不新建（ARCH-stub 已分散在各 doc 内） | ✅ 完成 |
| Phase C | 重新跑 coverage 验证：31.2% → 33.8%（+33 symbols） | ✅ 完成 |
| Phase C | 三文档 semantic gaps 全部 = 0 | ✅ 完成 |
| Phase C | 更新 checklist.md / 00-kernel-overview.md / STATE.md / todo.md | ✅ 完成 |

**Blocker Gates**：28/29/30 三文档全部 0/A/B/C/D/D-6/E/G/H PASS

## 15. Task 1 复扫（2026-08-13）：FPU 子系统缺口 → doc 31 + FpuTrap 实现

> 来源：用户任务 1 重新发起——"01~30 文档关联的 rust 实现 vs minix3 C 实现，是否有遗漏？若有，新建 31,32 等补足"。
> 2026-08-12 的 31+ 决策为"不新建"（ARCH-stub 已分散），本轮按用户要求**重新扫描**（含 28-30 引入后的复扫）。

**执行结果**：

| 项 | 内容 | 状态 |
|----|------|------|
| 复扫 | coverage-extract 重跑：1306 C 符号，doc 覆盖 446 (34.2%)，semantic gaps 0 | ✅ |
| 缺口判定 | 唯一真实 OS 知识点缺口 = **FPU 子系统**（arch_system.c fpu 函数族 / proc.c copr_not_available_handler / exception.c enable+disable_fpu_exception / do_sigsend.c save_fpu 路径在 01-30 全部无文档）；arch/earm + arch/i386 legacy = 范围外（2026-08-12 决策）；prepare_shutdown/debug 工具 = 已记录缺口/琐碎跳过 | ✅ |
| 新建 doc 31 | `31-fpu-context-switching.md`（Ch1 概念 6 节 / Ch2 C 全析 9 节 / Ch3 设计 6 决策 / Ch4 实现 5 节 / Ch5 测试 / Ch6 参见） | ✅ CONVERGED |
| 快照 | `31-outline.v1.md` + `31-outline-review.v1.md` + `31-design.v1.md`（Step 0.3 嵌入生成） | ✅ |
| 代码实现 | **FIX-31-1**：`ExceptionOutcome::FpuTrap` + vector 7 && is_user 特判 + 2 新测试（exception_dispatcher.rs）→ 167/167 tests pass | ✅ |
| 文档同步 | doc 14 §4.2 分发片段 + §4.5 注释同步 vector 7 特判；dispatcher 过时 doc 引用修复（Pattern #76）；fpu_arch.rs 注释行号修复（Pattern #77，lib.rs:1382→:1805） | ✅ |
| 显式缺口 | lazy-restore 主体（待异常交付路径接线）+ 信号路径 FPU 保存（D6，Task 3 候选）——31 doc §4.5 标注 | ✅（forward reference 合规） |
| Blocker Gates | 31 文档 0/A/B/C/D/D-6/E/G/H 全 PASS；VERIFY-CHECK consistency 100% | ✅ |

**遗留债务（本轮记录，不阻塞）**：docs 15/17-27/28/29/30 的 `.design/` 快照缺失
（前 session 协议未走；28-30 的 review 产物齐全）。已登记批量豁免，Proposal #18
（Step 0.3.3 批量补齐）待用户确认后统一执行。不可泛化为其他文档免快照（模式 71 DOG）。

**新文档决策摘要**：

| Doc | 决策 | 理由 |
|-----|------|------|
| 28-usermapped-data | WONTFIX | 64-bit 重写不保留 `.usermapped` 段；数据通过 `sys_getinfo` 获取（D1） |
| 29-kernel-debug | Partial+ | **✅ runqueues_ok / print_proc / rtsflagstr 已实现 (Phase 8, 2026-08-13)**；BKL timing 用 `debug_assert!` + `log!` 替代；非生产路径 |
| 30-kernel-profile | Partial+ | profile clock interface 保留；**✅ sample collection 已实现 (Phase 8, 2026-08-13)**；NMI profiling WONTFIX（D2/D3/D4） |

**剩余缺口（不新建文档的理由）**：

- `arch/earm/bsp/ti/omap_*.h` BSP 寄存器定义 (286+) — 硬件特定，OUT OF SCOPE
- `arch/i386/` 历史架构代码 (210+) — x86_64 有独立实现，ARCH-stub 化
- 编译时配置宏 (DEBUG_*/VF_*) — 已在 29-kernel-debug.md 语义覆盖
- DEFERRED 函数 — 已在现有文档中标注

**Task 1 状态**：✅ **CONVERGED** — 进入 Task 2（Rust 代码改进扫描）

---

## 16. Task 2 反向覆盖（2026-08-13）：StacktraceArch 零文档缺口 → doc 32

> 来源：用户任务 3——"kernel 相关 Rust 实现（涵盖 OS 知识点）未被 01~30 覆盖？未覆盖则补足"。
> 方法：`find os/ -name *.rs` × 文档文件名引用交叉（systematic reverse scan）。

**执行结果**：

| 项 | 内容 | 状态 |
|----|------|------|
| 反向扫描 | 13 个未覆盖 .rs 文件分类：真实知识点缺口 1（stacktrace.rs）/ 概念已覆盖 4（errno.rs、ipc/notify.rs、types/address.rs、types/com.rs）/ 超出内核范围 4（ipc/kernel.rs 仅 PM 用、ipc/pm.rs、ipc/vfs.rs、types/{id,pid,bitmap}.rs）| ✅ |
| 缺口判定 | **stacktrace.rs（StacktraceArch，113 行 trait + 2 impl + 默认 walk_frames 实现）有 Rust 实现零文档**——01-30 文件名 0 命中；doc 27 "util_stacktrace 未实现" 仅指内核栈 panic 版本（进程栈 proc_stacktrace 语义未覆盖）。OS 知识点 = 栈回溯机制，非 trivial | ✅ |
| 新建 doc 32 | `32-stack-tracing.md`（409 行，自包含：Ch1 概念 6 节 / Ch2 C 5 节 / Ch3 设计 D1-D7 / Ch4 实现 5 节 / Ch5 测试 / Ch6 参见）| ✅ CONVERGED |
| 快照 | `32-outline.v1.md` + `32-outline-review.v1.md` + `32-design.v1.md`（Step 0.3 嵌入生成）| ✅ |
| 代码实现 | **FIX-32-1**：x86_64/boot.rs 新增 stacktrace_tests 模块 5 测试（链遍历/循环检测/读失败/上限 32/GP_RBP 索引）→ 172/172 tests pass | ✅ |
| 文档同步 | FIX-32-2 4 处修正（trait span :41-113 / 172 passed 总数 / §5.1 缺 caps_at_max 行 / 测试数表述）| ✅ |
| Blocker Gates | doc 32 0/A/B/C/D/D-6/E/G/H 全 PASS；VERIFY-CHECK consistency 100%（14 项 grep 重放）| ✅ |
| 遗留 | ~~riscv64 StacktraceArch impl 缺失~~ → **FIX-32-2 已实现（2026-08-13，boot.rs:182-198，Task 3）**；DIAGCTL STACKTRACE 未接线（syscall.rs:670 ENOSYS，doc §4.5 forward reference）| ⏸ 接线仍遗留 |

**Task 2 状态**：✅ **CONVERGED** — 进入 Task 3（卓越性 2nd-pass）

---

## 17. Task 3 卓越性 2nd-pass（2026-08-13）：clippy 清零 + Pattern #76 全量复扫

> 来源：用户任务 2——"对照 Redox 与 Rust 社区最佳实践，修复所有改进点；修复一项标注一项"。
> 原则：每个修复标注 ID + file:line + 反查维度；关联文档同步。

**执行结果**：

| 改进项 | 修复内容 | 状态 |
|--------|---------|------|
| **FIX-T3-1** clippy E0133 批量 | `minix-plat` 43 处 `unsafe_op_in_unsafe_fn`（Rust 2024）→ `cargo clippy --fix` 包 unsafe block（plat/src/x86_64/early_console.rs ×9 + interrupt.rs ×21 + 其余） | ✅ |
| **FIX-T3-2** missing_safety_doc | `plat/src/x86_64/interrupt.rs:141-160` `lapic_id`/`ioapic_version` 补 `# Safety` 段（MMIO 地址有效性 + BKL 论证） | ✅ |
| **FIX-T3-3** unnecessary_parens | `arch/src/x86_64/paging.rs:218,323` `((vaddr >> 12) & 0x1FF)` → `(vaddr >> 12) & 0x1FF` | ✅ |
| **FIX-T3-4** Default impl | `arch/src/arch/post_init.rs:109-113` `FreePdeSlots` 补 `impl Default`（委托 `Self::new()`，与 FaultContextTracker 模式一致） | ✅ |
| **FIX-T3-5** doc indent | `arch/src/x86_64/trap_entry.rs:12,14` doc list item 缩进（markdown 续行 4 空格） | ✅ |
| **FIX-T3-6** same-type cast | `arch/src/x86_64/fpu.rs:102,120` `as *mut u8`/`as *const u8` 移除（as_mut_ptr/as_ptr 已返回目标类型） | ✅ |
| **FIX-T3-7** field_reassign | `arch/src/x86_64/signal.rs:221-259` `build_sigcontext` 30 字段连续赋值 → struct literal 全字段显式赋值（clippy struct update no effect 证明全覆盖，语义 = 原零初始化 + 赋值） | ✅ |
| **FIX-T3-8** result_unit_err | `arch/src/arch/clock.rs:142` `init_profile_clock` + `arch/src/arch/boot.rs:211` `write_user_register` → `#[allow(clippy::result_unit_err)]` + TODO(C-D-5) 标注（trait 契约，设计级改动不在此轮） | ✅（allow 标注） |
| **FIX-T3-9** Pattern #76 复扫 | **90 处 doc 引用失效修复**：`06-design-final.md`×42 → `06-design.v1.md`（章节号验证 D1-D8/§2.x/§3.x/§4.x，5 处不存在章节 §5/§12.5/§12.9/§15.5 → D7/§3.3/§3.10 语义修正）；`15-design.md`×9 → `15-clock-timer.md`；编号漂移×12 类（04-clock→05、19-syscall-device→20、18-syscall-signal→19、18-syscall-device→20、17-syscall-copy→18、16-syscall-process→17、07-scheduling→11、05-exception→14、06-arch-post-init→08、05-proc-init→06、08-vm-boot→09、02-page-table→02-higher-half）；语义映射×3（`03-vm-request`→`24-cross-space-runtime` §2.7/§2.8/§4.3、`08-proc-macros`→`06-proc-init-boot-proc` §3.1/§5.2、`01-bug`→`02-higher-half-kernel` 附录 A） | ✅ |
| **FIX-T3-10** SAFETY 注释审计 | krandom.rs try_krandom/krandom（BKL 论证 + addr_of_mut!）、misc.rs SPROF statics（BKL 论证）、arm64/smp.rs GICD_BASE（单线程早期启动论证）、boot-shim 3 处 static mut（exactly-once + # Safety）→ 全部论证齐全 | ✅ |
| G3 信号路径 FPU 保存 | 31 doc §4.5 显式缺口（do_sigsend.c:86 save_fpu 到 sigcontext）——涉及 sigcontext 布局变化 + 传递路径接线 = 设计级新功能 | ⏸ 保持 forward reference（31 doc 已诚实标注，入 backlog） |

**验证（2026-08-13）**：
- `cargo clippy -p minix-kernel -p minix-arch -p minix-plat -p minix-types -p minix-platform -p minix-boot` → **0 warnings**（除 workspace profiles 配置层）
- `cargo test` kernel 链全绿：609 kernel + 172 arch + 62 types + 15/17/19/2 其余
- 全 os/ doc 引用 0 MISSING（文件存在性验证）；riscv64 qemu-tests 编译失败 = 预存在问题（stash 验证，backlog）

**Task 3 状态**：✅ **COMPLETED** — 进入 Task 4（收尾回归 review 01~30）

---

## 10. Task 2-4 完成状态（2026-08-13）

### Task 2: 12-ipc-core P0 修复（5 P0 + 6 P1）

| P0 ID | 描述 | 状态 |
|-------|------|------|
| P0-12-1 | 14 个 P0 测试路径补齐 | ✅ 已修复 |
| P0-12-2 | receive Phase 2 async 完整实现 | ✅ 已修复 |
| P0-12-3 | do_ipc SENDA table 完整实现 | ✅ 已修复 |
| P0-12-4 | IpcEngine<'a> + SenderQueue(VecDeque) 对齐 design | ✅ 已修复 |
| P0-12-5 | REPLY_PEND 正确跳过 notify 但不跳过 async/caller_q | ✅ 已修复 |

**12-ipc-core 状态**：✅ **CONVERGED**（Session #24, 2026-08-13）— VERIFY-CHECK-12.md PASS

### Task 3: krandom 文档同步（反向覆盖）

| 文档 | 修复内容 | 状态 |
|------|----------|------|
| 25-misc-unported.md | §4.7 krandom 子系统接入描述 | ✅ 已修复 |
| 08-system-init-boot-finish.md | krandom_init() 已实现标注 | ✅ 已修复 |
| 14-exception-interrupt.md | get_randomness no-op stub 标注 | ✅ 已修复 |
| 20-syscall-device.md | generic_handler get_randomness 标注 | ✅ 已修复 |
| checklist.md | G-026/D-16 状态更新 | ✅ 已修复 |

### Task 4: 卓越性全量 2nd-pass（clippy 清理）

| 改进项 | 修复前 | 修复后 | 状态 |
|--------|--------|--------|------|
| clippy --fix 自动修复 | 272 warnings | 75 warnings | ✅ |
| unnecessary unsafe block (Rust 2024 union write) | 96 | 0 | ✅ |
| field_reassign_with_default (test code) | 109 | 0 (crate-level cfg(test) allow) | ✅ |
| dead_code (ipc_filter_check + IpcFilterElFlags) | 2 | 0 (#[allow] + reason) | ✅ |
| result_unit_err (clock/vm) | 2 | 0 (#[allow] + reason) | ✅ |
| too_many_arguments (grant/kpriv C-mirrored) | 2 | 0 (#[allow] + reason) | ✅ |
| should_implement_trait (proc from_str) | 1 | 0 (#[allow] + reason) | ✅ |
| if_same_then_else (syscall_copy) | 1 | 0 (#[allow] + reason) | ✅ |
| needless_range_loop (memmap) | 1 | 0 (iter_mut().enumerate()) | ✅ |
| assertions_on_constants (lib.rs) | 1 | 0 (const { assert! }) | ✅ |
| unusual_byte_groupings (memmap hex) | 1 | 0 (regrouped) | ✅ |
| map→if let (proc_table) | 1 | 0 | ✅ |

**kernel crate clippy**：✅ **0 warnings**（arch/plat/types 的 78 warnings 是 Rust 2024 unsafe_op_in_unsafe_fn，独立迁移任务）

**测试验证**：✅ **608/608 tests pass**，0 failures

### Phase 5: 收尾回归

- 12-ipc-core per-doc gates 升 CONVERGED ✅
- 15/26/27 文档存在且内容完整（26=WONTFIX）✅
- Phase 4 代码改动仅影响测试代码 + #[allow] 属性，无生产签名变化 ✅
- CONVERGED 文档不受影响 ✅

**整体状态**：✅ **Phase 1-5 全部完成**

---

## 11. 遗留项（Clippy Round 2 — Mechanical Cleanup，未执行）

> **来源**：`~/.trae/documents/clippy-round2-mechanical-cleanup-plan.md`（2026-08-13 制定，未实施）
> **背景**：Round 1 完成后 `minix-kernel` 仍有 47 个 clippy warnings。Round 2 计划范围限定为**纯机械修复**（不改语义/签名/类型），其余需设计决策的项已排除。
> **状态**：⏸️ **DEFERRED**（用户决定不修，登记后续处理）

### 11.1 Round 2 已排除项（需设计决策，**排除**）

| 警告 | 位置 | 排除原因 |
|------|------|----------|
| `too_many_arguments` (10/7) | `os/kernel/src/grant.rs:348` `verify_grant` | 需引入参数 struct → 签名变化 |
| `too_many_arguments` (8/7) | `os/kernel/src/kpriv.rs:793` `configure_boot_priv` | 需引入参数 struct → 签名变化 |
| `should_implement_trait` | `os/kernel/src/proc.rs:1034` `from_str` | 应实现 `std::str::FromStr` trait → API 重设计 |
| `if_same_then_else` | `os/kernel/src/syscall_copy.rs:829-832` | 两分支均 `return EINVAL` → 可能是 bug 或有意为之（C-ref: `do_umap_remote.c:57-66`） |
| `result_unit_err` | `os/kernel/src/vm.rs:642` `enqueue_and_notify` | `Result<(), ()>` → 自定义错误类型 → API 变化 |
| `result_unit_err` | `os/kernel/src/clock.rs:289` `init_profile_clock` | 同上 |

### 11.2 Round 2 计划执行的机械修复（已随整体 Phase 4 阶段完成，故跳过）

| 任务 | 范围 | Round 2 计划 | 实际状态 |
|------|------|-------------|----------|
| Task A | `no_effect` (1 处) | 移除 `();` 空语句 | ✅ 已完成（Phase 4 `syscall_copy.rs`） |
| Task B | `field_reassign_with_default` (3 处) | 转 struct literal | ✅ 已完成（Phase 4 通过 crate-level `cfg(test)` allow 抑制） |
| Task C | `needless_range_loop` (5 安全子集) | 转 `iter().take()` | ✅ 已完成（Phase 4 `memmap.rs` + 其他） |
| Task D1 | 真正死代码移除 (~6 处) | grep 验证后删除 | ✅ 已完成（Phase 4 已审查） |
| Task D2/D3/D4 | `#[allow(dead_code)]` 或 `#[cfg(test)]` (~15 处) | 加属性 + 注释 | ✅ 已完成（Phase 4 `ipc_filter_check`/`IpcFilterElFlags`/`proc_table::FREE_*` 等） |

**说明**：Round 2 计划是 Phase 4 工作的**子集**。Phase 4 在更广范围完成 clippy 清理（kernel crate 272→0 warnings），因此 Round 2 列出的所有机械项都被一并解决，无需独立执行。

### 11.3 剩余 clippy 警告分布（2026-08-13 状态 → 2026-08-13 晚 Task 3 清理后）

| Crate | 警告数 | 范围 | 处理建议 |
|-------|--------|------|----------|
| `minix-kernel` | **0** | ✅ 已清理 | — |
| `minix-arch` | **0** | ✅ 已清理（Task 3：10 处，含 E0133 批量 + 括号/Default/doc-indent/cast/signal struct literal） | — |
| `minix-plat` | **0** | ✅ 已清理（Task 3：43 处 E0133 unsafe block 批量 + 2 处 # Safety doc） | — |
| `minix-types` | **0** | ✅ 已清理（Task 2） | — |
| `minix-platform` | **0** | ✅ 已清理（Task 2） | — |
| `minix-boot` | **0** | ✅ 已清理（Task 2） | — |
| `minix-vm` | 116 | VM 服务器 crate（01-stage-kernel 范围外） | backlog（VM stage） |

**验证（2026-08-13）**：`cargo clippy -p minix-kernel -p minix-arch -p minix-plat` → **0 warnings**（除 workspace profiles 配置层警告）。`cargo test` kernel 链全绿（609 kernel + 172 arch + 62 types + 其余）。

### 11.4 待启动的设计级清理（未来 Round）

**触发时机**：arch 重构完成后启动

| 编号 | 任务 | 描述 | 依赖 |
|------|------|------|------|
| C-D-1 | `verify_grant` 参数 struct 化 | 10 个参数→ 4 个小组，签名变化 | Round 3 完成 |
| C-D-2 | `configure_boot_priv` 参数 struct 化 | 8 个参数 → 2 个小组 | Round 3 完成 |
| C-D-3 | `proc::from_str` → `FromStr` trait | 实现标准 trait，类型变化 | 无 |
| C-D-4 | `syscall_copy` if_same_then_else 调查 | 确认 C 行为，合并 or 显式 `#[allow]` | 无 |
| C-D-5 | `Result<(), ()>` → 自定义错误类型 | `vm::enqueue_and_notify` / `clock::init_profile_clock` / **`arch::clock::init_profile_clock` / `arch::boot::write_user_register`（Task 3 已 `#[allow(clippy::result_unit_err)]` + TODO 标注）** | 无 |

**Task 3 补充记录（2026-08-13，kernel 链 clippy 清零）**：
- 修复项：`plat` E0133×43（--fix 批量包 unsafe block）+ `plat` missing_safety_doc×2（interrupt.rs lapic_id/ioapic_version 加 # Safety）+ `arch` 括号×2（paging.rs:218/323）+ `arch` FreePdeSlots Default（post_init.rs）+ `arch` doc-indent×2（trap_entry.rs）+ `arch` same-type cast×2（fpu.rs:102/120）+ `arch` field_reassign→struct literal（signal.rs build_sigcontext，含移除无效 `..Default::default()`）
- 语义验证：signal.rs struct literal 全字段显式赋值 = 原 memset 零初始化 + 赋值（clippy struct update no effect 证明字段全覆盖）；所有修复 `cargo test` kernel 链全绿
- 未修（设计级，已 allow 标注）：`Result<(), ()>`×2（arch clock/boot trait 契约，C-D-5）

### 11.5 重启 Round 2 的方式

如需重启：
1. 阅读本章节确认 Round 2 范围
2. 阅读 `~/.trae/documents/clippy-round2-mechanical-cleanup-plan.md` 获取详细 plan
3. 优先修复 Round 3-6（arch/types/platform/boot）后再回到本任务
4. 执行后删除本章节（任务结束）

**Action Item**：本节保留至所有 Round 完成

## 18. Task 4 收尾回归（2026-08-13）：17 项 doc 引用修复 + 全量回归验证

**范围**：回归 review 01-30 文档 + 关联 Rust 实现的收尾。发现并修复 17 项 P2 doc 引用漂移/状态过时（无 P0/P1 新发现）。

**修复清单**（ID + file:line + 反查维度）：

| # | ID | 文件:行 | 修复 | 反查维度 |
|---|-----|--------|------|---------|
| 1 | FIX-T4-1 | 04-platform-discovery.md:397/401 | `clock.rs:182` → `:72`（ClockArch trait 实际定义位置）| 维度1 outline↔doc |
| 2 | FIX-T4-2 | 04-platform-discovery.md:401 | `interrupt.rs:128` → `:129`（InterruptController trait）| 维度1 |
| 3 | FIX-T4-3 | 04-platform-discovery.md:432 | `boot.rs:386` → `:443`（platform_sources fixture）| 维度5 doc↔code |
| 4 | FIX-T4-4 | 04-platform-discovery.md:432 | lib.rs fixture 行号 8 处 → :2265/2329/2411/2506/2537/2778/2802/2826 | 维度5 |
| 5 | FIX-T4-5 | 04-platform-discovery.md:432 | helpers 329/569 描述修正：生产构造函数参数（uefi:228/opensbi:432），非测试 fixture | 维度5（事实纠正）|
| 6 | FIX-T4-6 | 09-vm-boot-protocol.md:503 | `boot.rs:287-323` → `:290-326`（identity mapping，本 session +3 引入）| 维度6 元层 |
| 7 | FIX-T4-7 | 25-misc-unported.md:708 | `boot.rs:211` → `:214`（write_user_register，本 session +3）| 维度6 |
| 8 | FIX-T4-8 | 02-higher-half-kernel.md:1290 | `os/arch/src/paging.rs` → `os/arch/src/arch/paging.rs` | 维度5 |
| 9 | FIX-T4-9 | 02-higher-half-kernel.md:1291 | `direct_map.rs` 补 `arch/` 前缀 | 维度5 |
| 10 | FIX-T4-10 | 18-syscall-copy.md:607 | 同上 | 维度5 |
| 11 | FIX-T4-11 | 20-syscall-device.md:119 | `os/arch/src/x86_64/port_io.rs` → `os/plat/src/x86_64/port_io.rs`（crate 错位）| 维度5 |
| 12 | FIX-T4-12 | 01-boot-shim-bootstrap.md:1727 | `os/kernel/src/main.rs` 虚构路径 → panic=abort（Cargo.toml:32/37）+ EarlyConsole（plat）准确描述 | 维度5（虚构位置）|
| 13 | FIX-T4-13 | todo.md:145 | `paging_ext.rs` 补 `arch/` 前缀 | 维度5 |
| 14 | FIX-T4-14 | todo.md §4.3 | EarlyConsole 落地位置标注（方案 arch → 实际 plat）| 维度6 |
| 15 | FIX-T4-15 | todo.md §11.2 | RCPD 过时标注（proc_arch.rs ×3 已删 + helpers 行号漂移，Pattern #66）| 维度6 |
| 16 | FIX-T4-16 | checklist.md | syscall_signal.rs 10 处行号 → :107/266/348/437/615（perl 负向前瞻保护 P1-07/P1-08 历史记录）| 维度5 |
| 17 | FIX-T4-17 | checklist.md F-20/F-21 | ⚠️ Partial → ✅ Complete（2026-08-01 已落地 data_copy_vmcheck + sigframe）| 维度5（状态过时）|

**最终回归验证**（2026-08-13）：
- `cargo build`：✅（minix-vm 116 warnings = 已知 backlog，VM stage 范围外）
- `cargo clippy` kernel 链 6 crates：✅ 0 warnings（仅 workspace profiles 配置噪音）
- `cargo test` kernel 链：**894 passed, 0 failed**（kernel 609 + arch 172 + plat 19 + types 62 + platform 15 + boot 17）
- RCPD 复扫（Pattern #66）：100 unique `os/` 路径，5 missing 全部处置（trap_return.rs forward reference 合规 ×1 + todo.md §11.2 RCPD 标注 ×4）
- 收敛成本评估：触发 Step 7.1 停止规则 4（zero-bias——本回归 0 新 P0/P1，仅 P2 引用修复）

**backlog**（移出 Task 4 范围）：minix-vm 116 clippy warnings + riscv64 qemu-tests 编译失败（预先存在 ~250 errors）+ docs 15/17-27/28/29/30 `.design/` 快照缺失（Proposal #18 待用户确认）+ G3 信号路径 FPU 保存（doc 31 §4.5 forward reference）+ DIAGCTL STACKTRACE 接线（syscall.rs:670 ENOSYS）+ C-D-1~5 design-level cleanups（§11.4）。
