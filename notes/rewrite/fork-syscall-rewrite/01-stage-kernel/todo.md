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
  1. 在 `os/arch/src/early_console.rs` 定义 `EarlyConsole` trait，含 `write_byte` 抽象方法和 `write_str`/`write_hex` 默认方法（统一处理 `\n` → `\r\n` 和十六进制格式）
  2. 三架构 `early_console.rs` 各添加 ZST 类型（`X86_64EarlyConsole`/`AArch64EarlyConsole`/`Riscv64EarlyConsole`）实现 `EarlyConsole`
  3. `arch/src/lib.rs` 添加 `CurrentEarlyConsole` type alias，统一导出
  4. `kernel/src/lib.rs` 的 `kmain_verify` 使用 `CurrentEarlyConsole` 统一输出，仅保留 `#[cfg]` 区分寄存器标签（`RSP`/`SP` 等）和架构名，消除全部重复代码块
- **风险**: 无。各架构原有的 `early_console::write_str`/`write_hex` 自由函数保留，测试内核代码无需改动。
- **验证**: `cargo check -p minix-arch -p minix-kernel --all-targets --all-features` 通过；`kmain_verify` 输出格式与重构前完全一致
- **工作量**: 中（4-6 小时）

### 4.4 [P2] PTE_HUGE_FLAGS 语义不明 — ✅ 已完成

- **类型**: 命名
- **位置**: `os/arch/src/paging_ext.rs:96`
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
