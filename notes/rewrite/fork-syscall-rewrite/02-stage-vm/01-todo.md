# 01-todo: Review 修复记录

## 审查文件
`01-vmproc-struct.md`

## 审查结果

### P1 修复

#### 1. 行号错误：exit.c:91 → exit.c:73

**问题**: 文档中 `do_exit()` 检查 `VMF_EXITING` 的代码注释标注为 `exit.c:91`，但实际代码位于 `exit.c:73`。行 91 实际是 `clear_proc(vmp)` 调用。

**修复**: 将 `// exit.c:91` 改为 `// exit.c:73`。同时在 §3.2.1 的"进程退出"使用场景中补充了 `do_exit()` 检查 `VMF_EXITING` 的行号引用 `exit.c:73`，使文档更完整。

**验证**: `grep -n "VMF_EXITING" minix3/minix/servers/vm/exit.c` 确认行 73 为 `if(!(vmp->vm_flags & VMF_EXITING))`。

#### 2. 失效交叉引用：RECONSTRUCTION-PRINCIPLES.md

**问题**: §2.2.1 引用 `[重构指导原则](../../../RECONSTRUCTION-PRINCIPLES.md)`，但该文件不存在于仓库中。

**修复**: 移除失效链接，将引用改为内联描述"根据语义冻结原则（外部语义不变，内部表达可以改变）"，保留核心语义。

### Rust 代码审查

Rust 代码与文档设计一致，无需修改：

- `VmProc` 结构体字段与文档 §5.1 完全对应
- `VmFlags` bitflags 值（0x001/0x002/0x010）与 Minix3 `vmproc.h` 一致
- `clear()` 方法行为与文档 §5.3 对比表一致
- Typestate view（`EmptySlot`/`ActiveProc`/`ExitingProc`）与文档 §6.1.2 一致
- `vm_isokendpt()` 三重检查与文档 §6.2.2 一致
- `AclState` 三态 enum 与文档 §4.1.2 一致

### 未修复项（P2，记录备查）

1. 文档中 `fork.c:41-44` 的 `vm_isokendpt` 检查代码片段省略了 `printf` 和 `SANITYCHECK`，这是合理的简化，无需修改
2. 文档 §5.3 `clear()` 注释说 "Corresponds to Minix3's `free_proc()` + `clear_proc()`"，而 Rust 代码注释说 "Corresponds to Minix3's `acl_clear()` + `free_proc()` + `clear_proc()`"，后者更精确（因为 `clear_proc()` 内部调用了 `acl_clear()`），但语义等价，无需修改
