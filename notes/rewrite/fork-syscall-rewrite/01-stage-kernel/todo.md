# 01-stage-kernel 全局 TODO — 已全部清空（2026-08-14）

> 本文件为历史归档。原列出的全部 todo 均已处理（✅ 已解决 / 🔀 已转移 / 📋 完成记录），
> **无剩余未处理项**。跨阶段未完成项已实体转移到目标文档（见 §3 转移清单）。

## 1. 处理摘要

| 原章节 | 原内容 | 处置 | 去向 / 证据 |
|--------|--------|------|------------|
| §1 | Boot module 内存回收（Rust 实现） | ✅ 已解决 | boot-shim 加载路径已实现，todo 过时 |
| §2 | EBS 后映射回收 + `Box::leak` 生命周期 | ✅ 已解决 | 01-boot-shim-bootstrap §3.5.1 有意的 0/0 设计决策 |
| §3 | qemu-tests 后续扩展 | ✅ 已解决 | 目录已演进为 22 个按功能命名 test-kernels，代码复用 + run_all.sh 就位 |
| §4 | 页表页分配器 VM 阶段接入 | 🔀 转移 | `02-stage-vm/00-vm-overview.md` §8.4 |
| §5 | code-review 未修复问题（4.1-4.5） | ✅ 已解决 | 全部完成（KernelInfo pub 封装 / HigherHalf trait / kmain_verify / PTE_HUGE_FLAGS / 调试输出） |
| §6.1/6.2 | 异常 / 中断端到端测试（QEMU L4/L5） | 🔀 转移 | 单元层已实现（trap_entry 完整 IDT + 5 测试 / exception_dispatcher 分发测试）；QEMU E2E → `notes/TODO.md` QEMU backlog |
| §6.3/6.4 | init/load 顺序测试 + init_ap 路径 | 🔀 转移 | `16-smp.md` §5.3（x86_64 init_ap 仍为 panic! 占位，protection.rs:370） |
| §6.5/6.6 | aarch64 GICv3 PPI unmask / riscv64 PLIC base | ✅ 已解决 | 已完成 |
| §6.7 | riscv64 PMP 仅配置 entry 0 | 🔀 转移 | 当前行为符合文档描述（allow-all），增强 → `notes/TODO.md` QEMU backlog |
| §6.8 | QEMU GDB 脚本未集成 CI | 🔀 转移 | `16-smp.md` §5.3 |
| §7 | PlatformDesc / 硬件发现 | ✅ 已解决 | 全部完成 + 验证通过（19/19 platform 测试）；AcpiDesc 最小化扩展 → `00-vm-overview.md` §8.4 |
| §8.1/8.2 | 平台发现阶段预存在编译失败 | ✅ 已解决（2026-08-14） | **19/19 test-kernels 三 target 编译通过**，见 §2 |
| §9.0-9.4 | Rust 2024 迁移后残留编译错误 | ✅ 已解决（2026-08-14） | 修复方式见 §2（9.0 原已修，9.1-9.4 本次修复） |
| §10 | `static mut` 收编 AssumeSyncCell | ✅ 已解决 | kernel 4 个 lib.rs 目标已迁移 `.get()`；6 处生产位置 SAFETY 论证齐全（FIX-T3-10） |
| §11 | QEMU 测试与生产路径不一致（QemuVirtDesc P0） | ✅ 已解决 | PlatformDescSource 架构（UEFI RSDP/DTB + OpenSBI `install_dtb_ptr`/`dtb_ptr`）+ doc 04 重写（2026-07-16） |
| §12.1-12.8 | 07-cross-space-init review 待办 | ✅ 已解决 | 12.1-12.8 全部完成；12.2 ptproc / 12.3 BKL 长期 → `16-smp.md` §5.3 |
| §13 | 阶段 B 页表重建决策 | ✅ 已解决 | 02-higher-half-kernel.md:117-119 论证（Rust 内核第一指令即在最终虚拟地址，无 pre-relocated data） |
| §14 | Task 1 C 源码覆盖扫描 + doc 28-30 | 📋 完成记录 | 28/29/30 三文档 CONVERGED |
| §15 | Task 1 复扫：FPU 缺口 → doc 31 | 📋 完成记录 | 31-fpu-context-switching CONVERGED；G3 信号路径 FPU 保存已在 31 §4.5 forward reference |
| §16 | Task 2 反向覆盖 → doc 32 | 📋 完成记录 | 32-stack-tracing CONVERGED；DIAGCTL STACKTRACE **已接线**（syscall.rs:2167 `2 =>` 分支） |
| §17 | Task 3 卓越性 2nd-pass | 📋 完成记录 | kernel 链 clippy 0 warnings；Pattern #76 90 处 doc 引用修复 |
| §10(二) | Task 2-4 完成状态 | 📋 完成记录 | 12-ipc-core CONVERGED（5 P0 + 6 P1 修复）+ krandom 文档同步 + Phase 4 收尾 |
| §11(二) | Clippy Round 2 遗留 | ✅ 已解决 | 全部机械项随 Phase 4 完成；排除项 C-D-1~5 → `notes/TODO.md` kernel 设计级 backlog |
| §18 | Task 4 收尾回归 | 📋 完成记录 | 17 项 doc 引用修复 + 894 passed 回归 |

## 2. 2026-08-14 修复记录（§8/§9 编译问题）

todo.md §8.1/§8.2/§9.1-9.4 记录的 qemu-tests 编译失败（19 个 test-kernels 三 target），
根因是早期脚手架未跟随 kernel/plat 重构同步。本次修复：

| 问题 | 根因 | 修复 |
|------|------|------|
| §9.1 `E0152 duplicate panic_impl` | 裸机 bin 在 host `cargo test` 下编译 test harness，链接 std（std 已定义 panic_impl） | **19 个 test-kernel Cargo.toml 添加 `[[bin]] test = false`**——裸机固件不编译 host test harness |
| §9.2 `E0425 arch_boot/init_proc_and_boot` cfg 不匹配 | 早期默认 mock feature 拉 std；现 test-kernels 已 `default-features = false` | 裸机 target 编译验证通过（cfg 匹配已正确） |
| §9.3 `E0432 minix_plat::riscv64` 路径不存在 | host 下 `target_arch` 不匹配，riscv64 模块被 cfg 掉 | `test = false` 后 host 不再编译 test harness；裸机 target 下模块存在 ✅ |
| §9.4 `#[global_allocator]` 与 uefi 冲突 | 12 个 test-kernels 声明 `uefi features = ["global_allocator"]`，feature unification 传播到 boot-shim lib test | 移除该 feature（test-kernels 自带 HybridAllocator，改用 `features = ["alloc"]`） |

**验证（2026-08-14）**：
- 19/19 test-kernels 三 target 编译 0 errors（x86_64-unknown-uefi ×7 / aarch64-unknown-uefi ×6 / riscv64gc-unknown-none-elf ×6）
- 全 workspace `cargo test`：kernel 609 + arch 172 + plat 19 + types 62 + platform 15 + boot-shim 26+13 + 其余全绿
  （**唯一例外**：minix-vm 15 failed = **pre-existing**，stash 验证与本次改动无关 → `00-vm-overview.md` §8.4）
- 全 workspace `cargo clippy`：无 error

## 3. 转移清单（跨阶段未完成项）

| 目标文档 | 追加位置 | 转移项 |
|---------|---------|--------|
| `02-stage-vm/00-vm-overview.md` | §8.4 | 页表页分配器 VM 阶段接入（原 §4）+ minix-vm 116 clippy warnings（原 §11.3）+ **minix-vm 15 测试失败（本次发现，pre-existing）** + AcpiDesc 最小化扩展（原 §7） |
| `01-stage-kernel/16-smp.md` | §5.3 | init/load 顺序测试（原 §6.3）+ init_ap 路径验证（原 §6.4）+ QEMU GDB CI（原 §6.8）+ ptproc per-CPU 语义跟踪（原 §12.2，单核占位 OK） |
| `notes/TODO.md` | kernel 设计级 backlog 段 | C-D-1~5 设计级清理（原 §11.4：verify_grant / configure_boot_priv / FromStr / if_same_then_else / Result<(),()>） |
| `notes/TODO.md` | QEMU 集成测试 backlog 段 | 异常 E2E（原 §6.1）+ 中断 E2E（原 §6.2）+ riscv64 PMP 多 entry 增强（原 §6.7） |

## 4. 最终验证（2026-08-14）

- `cargo test`（os/ workspace）：全部通过（除 minix-vm pre-existing 15 failed，见上）
- `cargo clippy`（全 workspace）：无 error
- test-kernels 三 target 编译：19/19 × 0 errors
- `git status`：本次改动全部为 tracked 文件修改（`git add -u` 模式提交）
