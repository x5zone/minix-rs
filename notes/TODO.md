# 待办事项：文件拆分与整理

本文件记录需要拆分到多个目录的文件，以及后续需要进行的改动。

---

## ✅ 已完成的拆分

### 1. `redesign/improve_minix_refactored.md` 拆分完成

**已完成操作**：
1. ✅ 创建备份文件 `improve_minix_refactored.md.backup`
2. ✅ 创建 `redesign/architecture-changes.md`：包含章节一、二（架构变更）
3. ✅ 创建 `rewrite/modern-hardware-and-rust.md`：包含章节三至八（现代硬件适配、性能理论、工程实践、Rust 重构指导）
4. ✅ 创建 `study/learning-path.md`：包含章节九、十（学习路线建议、结语）
5. ✅ 更新 `rewrite/rewrite.md` 和 `redesign/redesign.md` 的文档索引

**拆分标准**：
- **redesign**：架构设计变更（如将 PM 移入内核地址空间、LPE Core 策略）
- **rewrite**：重构相关（面向现代硬件、性能优化、工程实践、Rust 指导原则）
- **study**：学习路线和方法论

---

## 待处理的文件

### 2. `redesign/improve_minix.md` 待处理

**问题**：与 `improve_minix_refactored.md` 内容相似，需要确认是否删除或合并。

**建议**：确认 `improve_minix_refactored.md` 已正确拆分后，可删除此文件。

---

### 3. `redesign/semantic-modules.md` 待评估

**问题**：该文件包含了语义模块设计，需要评估是否需要拆分。

**当前状态**：已正确放置在 `redesign/` 目录，暂不拆分。

---

## 需要删除的备份文件

以下备份文件可以在确认内容已正确迁移后删除：

- [ ] `redesign/improve_minix.md.backup` - 原始文件备份
- [ ] `redesign/improve_minix_refactored.md.backup` - 重构版备份
- [ ] `study/ipc/ipc-detail.md.bak` - IPC 详细文档备份
- [ ] `study/ipc/ipc.md.bak` - IPC 模块文档备份
- [ ] `study/progress.md.backup_20260329_233603` - 进度文件备份

---

### 3. `rewrite/misc.md`（异步消息表分析）

**问题**：该文件分析了 C 语言中的"阴险锁"设计，属于 rewrite 阶段的 Rust 重写思路。

**当前状态**：已正确放置在 `rewrite/` 目录，无需拆分。

---

### 4. `rewrite/arch_mapping.md`（架构机制映射）

**问题**：该文件讨论了如何将硬件描述抽象为机制，属于 rewrite 阶段的设计思路。

**当前状态**：已正确放置在 `rewrite/` 目录，无需拆分。

---

### 5. `rewrite/invariant.md`（内核不变量分析）

**问题**：该文件分析了 MINIX3 内核的不变量，指导 Rust 重写。

**当前状态**：已正确放置在 `rewrite/` 目录，无需拆分。

---

### 6. `rewrite/ipc-sendrec.md`（SENDREC 原子性问题）

**问题**：该文件分析了 SENDREC 的原子性问题，属于 rewrite 阶段的设计思路。

**当前状态**：已正确放置在 `rewrite/` 目录，无需拆分。

---

### 7. `redesign/ipc-improve.md`（IPC 设计改进思考）

**问题**：该文件记录了关于 IPC 设计改进的思考，属于 redesign 阶段。

**当前状态**：已正确放置在 `redesign/` 目录，无需拆分。

---

## 需要删除的备份文件

以下备份文件可以在确认内容已正确迁移后删除：

- [ ] `study/ipc/ipc-detail.md.bak` - 原始 IPC 详细文档备份
- [ ] `study/ipc/ipc.md.bak` - 原始 IPC 模块文档备份
- [ ] `study/progress.md.backup_20260329_233603` - 进度文件备份

---

## 需要更新的链接

以下文件中的链接需要更新（因为文件位置已改变）：

### `study/progress.md`

- [ ] 更新所有 `../tmp/` 链接到正确的相对路径
- [ ] 更新模块链接（如 `ipc/ipc-detail.md` → `study/ipc/ipc-detail.md`）

### `study/ipc/ipc-detail.md`

- [ ] 更新交叉引用链接（如 `../process/process-detail.md` → `../process/process-detail.md`）

### `study/*/xxx.md` 文件

- [ ] 检查并更新所有相对路径链接

---

## 后续工作

1. **确认文件移动正确**：检查所有文件是否在正确的目录中
2. **更新所有链接**：修改文件中的相对路径链接
3. **拆分混合内容文件**：按照上述建议拆分文件
4. **删除备份文件**：确认内容正确后删除备份
5. **创建索引文件**：为每个目录创建 README.md 或索引文件

---

## 文件移动记录

### 已移动到 `study/`

- `ipc/` → `study/ipc/`
- `process/` → `study/process/`
- `syscall/` → `study/syscall/`
- `interrupt/` → `study/interrupt/`
- `boot/` → `study/boot/`
- `arch/` → `study/arch/`
- `vm/` → `study/vm/`
- `pm/` → `study/pm/`
- `vfs/` → `study/vfs/`
- `services/` → `study/services/`
- `progress.md` → `study/progress.md`
- `roadmap.md` → `study/roadmap.md`
- `minix_structure.md` → `study/minix_structure.md`
- `daily.md` → `study/daily.md`

### 已移动到 `rewrite/`

- `invariant.md` → `rewrite/invariant.md`
- `arch_mapping.md` → `rewrite/arch_mapping.md`
- `misc.md` → `rewrite/misc.md`
- `study/ipc/ipc-sendrec.md` → `rewrite/ipc-sendrec.md`

### 已移动到 `redesign/`

- `improve_minix.md` → `redesign/improve_minix.md`
- `improve_minix_refactored.md` → `redesign/improve_minix_refactored.md`
- `study/ipc/ipc-improve.md` → `redesign/ipc-improve.md`
- `redesign/semantic-modules.md` → `redesign/semantic-modules.md`（已存在）

---

## kernel 设计级清理 backlog（从 01-stage-kernel/todo.md 转移，2026-08-14）

> 以下项目原属 `01-stage-kernel/todo.md` §11.4（Clippy Round 2 排除项 — 需设计决策），
> 触发时机为 arch 重构完成后，随 todo.md 清空转移至此。

| 编号 | 任务 | 描述 | 依赖 |
|------|------|------|------|
| C-D-1 | `verify_grant` 参数 struct 化 | `os/kernel/src/grant.rs:348` 10 个参数 → 4 个小组，签名变化 | Round 3 完成 |
| C-D-2 | `configure_boot_priv` 参数 struct 化 | `os/kernel/src/kpriv.rs:793` 8 个参数 → 2 个小组 | Round 3 完成 |
| C-D-3 | `proc::from_str` → `FromStr` trait | `os/kernel/src/proc.rs:1034` 实现标准 trait，类型变化 | 无 |
| C-D-4 | `syscall_copy` if_same_then_else 调查 | `os/kernel/src/syscall_copy.rs:829-832` 两分支均 return EINVAL（C-ref: do_umap_remote.c:57-66），确认 C 行为后合并或显式 `#[allow]` | 无 |
| C-D-5 | `Result<(), ()>` → 自定义错误类型 | `vm::enqueue_and_notify`（vm.rs:642）/ `clock::init_profile_clock`（clock.rs:289）/ `arch::clock::init_profile_clock` / `arch::boot::write_user_register`（后两者已 `#[allow(clippy::result_unit_err)]` + TODO 标注） | 无 |

## QEMU 集成测试 backlog（从 01-stage-kernel/todo.md 转移，2026-08-14）

> 以下项目原属 `01-stage-kernel/todo.md` §6 测试缺口，属 QEMU 真实硬件路径验证/riscv64 增强，
> 随 todo.md 清空转移至此。单元层均已实现（trap_entry 完整 IDT + 5 测试 / exception_dispatcher 测试），
> 剩余为 QEMU 层端到端验证。

| 待办 | 原出处 | 当前状态 |
|------|--------|---------|
| 异常端到端测试（L4）：handler 地址为 0，异常交付链路未在 QEMU 验证（单元层已有：trap_entry 重写 + 5 测试） | todo §6.1 [P1] | 单元层 ✅，QEMU E2E 未实现 |
| 中断端到端测试（L5）：中断交付链路未在 QEMU 验证（单元层已有：exception_dispatcher 分发测试） | todo §6.2 [P1] | 单元层 ✅，QEMU E2E 未实现 |
| riscv64 PMP 仅配置 entry 0（allow-all），多 entry 增强未实现——当前行为符合文档描述，属已知限制 | todo §6.7 [P2] | 符合描述，增强未实现 |
