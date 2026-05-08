# 02-todo: Review 修复记录

## 审查文件
`02-vmproc-table.md`

## 审查结果

### P2 修复

#### 1. `swap_proc_slot()` 安全性描述不够精确

**问题**: §5.3 设计要点表中"安全性"描述为"方法本身是 safe 的，内部使用 `ptr::swap`"，但实际 Rust 代码中使用了 `unsafe { core::ptr::swap(...) }` 块。虽然方法签名确实是 safe 的，但描述应更精确地反映内部使用了 unsafe 块。

**修复**: 改为"方法签名是 safe 的，内部使用 `unsafe { ptr::swap }`"。

**验证**: 查看 `vmproc_handle.rs:420-434`，`swap_proc_slot()` 方法签名为 `pub(crate) fn swap_proc_slot(&mut self, other: &mut ActiveProc<'_>)`（safe），内部使用 `unsafe { core::ptr::swap(self.inner as *mut VmProc, other.inner as *mut VmProc); }`。

#### 2. `VM_PROC_COUNT`/`VM_EXEC_TMP_SLOT` 可见性描述不准确

**问题**: §5.4 可见性表中描述"未从 mod.rs 重导出，仅 table.rs 内部使用"，但这两个常量在 table.rs 中定义为 `pub(crate)`，意味着 vm crate 的其他模块可以通过 `crate::vmproc::table::VM_PROC_COUNT` 访问（虽然 mod.rs 没有重导出，但 `pub(crate)` 本身允许同 crate 访问）。

**修复**: 改为"定义在 table.rs 中为 `pub(crate)`，但未从 mod.rs 重导出；vm crate 其他模块可通过 `table::VM_PROC_COUNT` 访问"。

**验证**: `table.rs:27-30` 中 `VM_PROC_COUNT` 和 `VM_EXEC_TMP_SLOT` 均为 `pub(crate)`。

### 源码行号验证

| 引用 | 文档标注 | 实际位置 | 一致? |
|------|----------|----------|-------|
| `glo.h:17-20` 进程表定义 | L17-20 | L17=`VMP_EXECTMP`, L18=`VMP_NR`, L20=`vmproc[]` | ✅ |
| `utility.c:84-94` vm_isokendpt | L84-94 | L84=函数签名, L93=return OK | ✅ |
| `utility.c:188` swap_proc_slot | L188 | L188=函数签名 | ✅ |

### Rust 代码审查

Rust 代码与文档设计一致，无需修改：

- `VmProcTable` 结构体与文档 §5.1 一致
- `AssumeSyncCell` 包装与文档 §4.2.4 一致
- Typestate view API 与文档 §5.3 一致
- `vm_isokendpt()` 三重检查与文档 §3.2.1 一致
- `VmProcIter` 实现与文档 §5.3 一致
- 可见性设计与文档 §5.4 基本一致（已修正描述）
- `swap_proc_slot()` 实现与文档 §5.3 一致

### 未修复项（P2，记录备查）

1. 文档 §3.2.1 中 `vm_isokendpt` 的 C 代码有冗余检查 `if(*procn >= 0 && ...)`——因为步骤 1 已排除 `*procn < 0`，步骤 2/3 的 `*procn >= 0` 条件恒为真。这是 Minix3 原始代码的风格，文档如实记录，无需修改
2. 文档附录 A 中 `pt_new()` 的代码是简化版本，省略了部分实现细节（如 `ARCH_VM_DIR_ENTRIES` 清空循环），这是合理的简化
