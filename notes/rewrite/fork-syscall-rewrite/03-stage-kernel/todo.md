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

### 4.1 [P1] KernelInfo 字段 pub 封装 — ✅ 部分完成

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
- **剩余**: 其他字段（如 `kern_size`, `user_sp` 等）仍为 `pub`，可按需逐步添加 getter
- **验证**: `cargo check` + `cargo test` (299 passed) + `boot_integration.rs` (2 passed)

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
| 4.1 KernelInfo pub 封装 | P1 | 中 | 重构窗口期统一处理 |
| 4.3 kmain_verify 重复 | P1 | 中 | 重构窗口期统一处理 |
| 4.2 HigherHalf 单方法 | P1 | 小 | 文档完善即可（推荐保持现状） |
| 4.4 PTE_HUGE_FLAGS 命名 | P2 | 小 | ✅ 已完成 |
| 4.5 调试输出 | P2 | — | ✅ 已完成 |

**建议**: 4.1 和 4.3 在下一次大规模重构时统一处理（涉及 trait 设计和 API 变更）；4.2 通过文档注释解决。

---

## 6. 测试缺口（来自 04-tests.md 完备性分析）

> 来源：`04-tests.md`（分析日期 2026-06-11）
> 分析覆盖：03-kmain-cstart.md + 04-clock-interrupt-init.md
> 当前 126 单元测试 + 6 QEMU 测试全部通过。

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
