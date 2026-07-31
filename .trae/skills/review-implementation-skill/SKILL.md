---
name: review-implementation-skill
description: 设计→实施 验证技能。验证 Rust 代码正确实现 design doc (e.g. 06-design.md / 06-design-final.md)；追踪 §12 自我审查问题清单；检查概念抽象是否与设计一致。
---

# Implementation Review Skill

> 验证 Rust 代码对 design doc 的实现是否忠实、完整、无新增问题。
> 与 `review-code-skill`（纯代码质量）正交——本 skill 关注 **design ↔ code 一致性**。

## 何时使用本 skill

- **任务类型**：实施 design doc（`06-design.md` 非 bagging / `06-design-final.md` bagging 等大型设计）后，必须运行本 skill
- **触发场景**：
  - `06-*.md` / `*-design-*.md` 类设计文档进入"实施"阶段
  - IDE AI bagging 后追加了 §X self-review issues 的设计
  - 多 trait 重构、跨 crate API 变更
- **不适用**：纯代码 bug 修复（用 `review-code-skill`）、纯文档校对（用 `review-doc-skill`）

## 核心方法：4 个必查维度

### 1. §X Self-Review Issue Traceability（设计自我审查追踪）

> 设计文档末尾的 §X（多为 §12 / §13 / §Rule Discovery）通常列出了
> IDE AI bagging 之后追加的 self-review issues。每条 issue 都必须在
> 实施中得到处理（或显式标注 deferred）。

**检查步骤**：
1. 读 design doc 末尾的 self-review issues section
2. 对每条 issue，grep 代码确认已处理
3. 若 deferred，必须在代码注释或 commit message 中说明理由
4. 输出 issue → 处置 的 trace 表

**失败模式**：
- 设计写了 "fix this" 但代码未改（**P0 因果链断裂**）
- 设计 deferred 但代码中悄悄实现（或反之）
- 多个 issue 互相引用但实现只 fix 一个

### 2. Concept Abstraction Alignment（概念抽象对齐）

> design doc 中的 §3 / §4 通常定义了新的 OS-level 抽象（如 `CpuContextArch`、
> `ProcessCapability`、`EntrySpec`）。验证代码中的类型命名、字段、trait
> 方法签名与设计完全一致。

**检查步骤**：
1. 抽取 design doc 中的所有 `pub struct` / `pub enum` / `pub trait`
2. 抽取 Rust 代码中对应位置的 `pub struct` / `pub enum` / `pub trait`
3. 字段名 / 字段顺序 / 字段类型 / 关联类型 必须完全一致
4. trait 方法签名（参数类型 + 返回类型）必须完全一致
5. 不允许 "我用了更合理的命名" — 偏离设计必须显式标注 §X.Y 引用

**失败模式**：
- trait 方法签名变了（如 `&mut self` → `&self`）
- 关联类型关联错位
- Newtype 字段类型用了 `u64` 而非 Newtype（如 `TrapMask`）
- 字段命名 snake_case vs camelCase 不一致

### 3. Backward-Compatible Refactor（向后兼容重构）

> 大型设计实施通常涉及"删除旧 API + 引入新 API"。验证旧 API 的所有
> 调用方都已迁移，且没有"残留代码"。

**检查步骤**：
1. 列出 design doc 中标记为删除的所有旧类型/函数
2. grep 全仓库 `rg "OldType|old_function"` 找残留引用
3. 旧 API 文件本身（如 `proc_arch.rs`）应已 `rm`
4. 重导出（如 `pub use arch::proc_arch`）应已删除
5. 旧的常量（如 `X86_64_IOPL_BITS`）若失去唯一调用方，应已删除

**失败模式**：
- 旧 trait 文件 `rm` 了但 `mod.rs` 里还有 `pub mod proc_arch;`（编译失败）
- 旧常量未删除（dead_code 警告 → 噪音）
- 旧 trait 的 re-export 未删除（双重定义编译失败）
- 调用方部分迁移：一半用新 API 一半用旧 API

### 4. Test Coverage Boundary（测试覆盖边界）

> 设计通常包含新的测试要求（如"§5 each test function grep-verified"）。
> 验证测试覆盖了所有 §4 公开 API。

**检查步骤**：
1. 抽取 design doc §4 中的所有 `pub` API（trait 方法、struct 字段、enum 变体）
2. grep `cfg(test)` 块确认每个 API 有至少 1 个单元测试
3. 测试断言必须覆盖**正常路径 + 至少 1 个 edge case**
4. 错误路径测试（如 `Result::Err`）必须有 error variant 验证

**失败模式**：
- 测试只验证 happy path（边界错误未测）
- 测试断言使用 `unimplemented!()` placeholder
- 多个 trait 方法只测了 1 个
- 测试运行了但断言被注释（`// assert_eq!`）

## Skill 输出模板

```markdown
## Implementation Review Result

### §X Self-Review Issues
| Issue ID | Design § | Code disposition | Evidence |
|----------|---------|------------------|----------|
| 12.1 | aarch64 FPU field loss | ✅ Fixed (AArch64CpuContext.fpu_enable_el0) | `os/arch/src/arm64/boot.rs:44` |
| 12.2 | load_vm_elf silent failure | ✅ Fixed (Result<VmLoadResult, VmLoadError>) | `os/arch/src/arch/boot.rs:203` |
| 12.7 | Rule Discovery section | ⏸ Deferred to Phase 9 | see STATE.md |
| 12.8 | _state underscore parameter | ✅ Fixed (let _ = ctx;) | `os/arch/src/arch/boot.rs:171` |

### Concept Abstraction Alignment
| Design struct | Code struct | Field match | Trait method match |
|---------------|-------------|-------------|-------------------|
| CpuContextArch | CpuContextArch | ✅ 5 fields | ✅ 5 methods |
| EntrySpec | EntrySpec | ✅ 3 fields | n/a |

### Backward-Compatible Refactor
| Old API | Status | Residual refs |
|---------|--------|--------------|
| ArchProcReset | ✅ Deleted | 0 (rg verified) |
| proc_arch.rs (4 files) | ✅ Deleted | 0 |
| set_boot_initial_reg_state | ✅ Deleted | 0 |
| ExtRegState | ✅ Deleted | 0 |

### Test Coverage Boundary
| API | Test present? | Test cases |
|-----|---------------|-----------|
| CpuContextArch::build_cpu_context | ✅ | 4 (kernel-task, vm, root-service, default) |
| CpuContextArch::apply_to_trap_frame | ✅ | 1 |
| CpuContextArch::enable_user_io | ✅ | 1 |
| load_vm_elf | ✅ | 1 (invalid ELF) |

### Severity: P0 / P1 / P2
- P0: <list>
- P1: <list>
- P2: <list>
```

## Gate D-Impl（实施 Gate）

| 检查项 | 通过条件 | 失败后果 |
|--------|---------|---------|
| §X issues 全部处置 | 100% (含 deferred 显式标注) | scan DRAFT |
| 概念抽象 0 偏离 | 字段名/签名/类型 100% 匹配 | scan DRAFT |
| 旧 API 0 残留 | rg 验证 0 hit | scan DRAFT |
| 测试覆盖 ≥ 80% | grep 验证每个 API 有测试 | scan DRAFT |
| `cargo test` 全绿 | 0 failed | scan DRAFT |

## 与其他 Skill 的协作

- **`review-process-skill`**：本 skill 是 Step 3.5c（概念抽象验证）的扩展
- **`review-code-skill`**：本 skill 完成后，再用 review-code-skill 检查纯代码质量
- **`review-patterns-skill`**：模式 48（因果链编造）+ 51（实现驱动概念章）是本 skill 的常见失败模式
- **`review-core-semantics-skill`** §1.4-1.5：Refactor 判定 + 优先级链（Minix3 > design > code > doc）
- **`review-process-skill`** Step 1.6：设计对齐检查 — 本 skill 的前置 Gate H 之一

## Design-First 视角

> 配合 [review-profiles.md Profile R](../review-rules/review-profiles.md) 使用。

**Design-First Review 模式下，本 skill 的执行差异**：
1. **审查重点变化**：从"代码是否符合 design"扩展到"design 本身是否完整、正确、可实现"
2. **新检查项**：
   - Design 文档 §3 trait/方法签名是否可实现？（编译性预检）
   - Design 不变量是否在代码中有对应测试？
   - Design 错误码策略是否在代码中体现？
3. **失败模式新增**：
   - **P0-design-missing**：design 缺关键决策（如未定义 trait 方法签名）
   - **P0-design-wrong**：design 决策技术错误（如不安全抽象）
   - **P0-test-missing**：design 要求测试但代码无

详见 [review-patterns-skill.md §X.5 Pattern 63 Design-Missing](../skill/review-patterns-skill.md)。

## 完成标准

实施 review 完成后，scan.md 的"实施审查"章节必须填齐 4 个维度的表
+ Severity 列表。任何 "✅ passed" 无 grep 证据 = Gate D-Impl FAIL。

## 元说明

- 本 skill 由 06-design-final.md 实施过程沉淀（2026-06-22）
- 适用于所有"多 AI bagging 后设计 + 大型代码重构"场景
- 与 review-code-skill 的边界：本 skill 关注 design ↔ code 一致性；
  review-code-skill 关注代码本身的 idiomatic 质量、测试、注释