---
name: "review-code-skill"
description: "Minix-RS Rust 代码 Review 检查清单。包含 §1-§14 全部维度：Rewrite 质量、硬件抽象和 trait 设计评估、类型安全、执行模型与并发、内存模型、模块设计、命名与可追溯性、测试、注释文档、64位假设、复杂度与工程性、no_std 约束、设计-代码一致性、C-Rust 语义对齐。当 Agent 需要检查代码(.rs)质量时调用此 Skill。"
---

# Minix-RS 代码 Review 检查清单

## 1. Rewrite 质量检查

> 最核心维度。发现 Translate 味道必须指出。

- [ ] 裸整数表达语义？→ 应 newtype/enum 替代
- [ ] C 式空指针/哨兵值（`0`/`-1` 表示"无值"）？→ 应 `Option<T>`
- [ ] 未命名的魔术数字？→ 应常量
- [ ] C 式 flag 组合？→ 应 bitflags/enum/typestate
- [ ] C 式错误码传递？→ 应 Result + ?
- [ ] C 宏直译成 Rust 宏？→ 应 trait/泛型
- [ ] 保留 C 式数据结构未重构？→ 应 Rust 所有权
- [ ] "为了 Rust 而 Rust"的过度设计（偏离 Minix 语义）？
- [ ] 是否做到"非法状态不可表达"？
- [ ] **错误码是否与 Minix3 原始 errno 严格对应**？禁止自创
- [ ] **内存所有权清晰**：C 中分配/释放点在 Rust 中有对应 Owner？C 中"借用"的 Rust 中不能误用为"所有权"
- [ ] **代码是否实现 Ch3 设计决策**？设计说 typestate 代码却用裸整数→P0
- [ ] **代码是否与 Ch4 描述一致**？函数签名/类型/语义是否匹配？

---

## 2. 硬件抽象检查

> **强制：所有硬件都必须抽象为 trait。** 抽象机制，不描述硬件。

- [ ] 出现具体硬件语义（CR3/TSS/MSR/I/O端口）？❌ P0
- [ ] 硬件交互全部通过 trait？上层仅依赖 trait 接口？
- [ ] 数据结构含架构特定硬件字段（如 `pde` 数组）？❌ 应内部管理
- [ ] 使用 `#[cfg(target_arch)]` 选行为？❌ 应 trait 静态分派
- [ ] OS 语义类型（如 `PageFlags`）与硬件编码分离？
- [ ] trait 定义在使用方附近（分散定义），实现集中在 arch crate（集中实现）？

### 2.5 trait 设计质量评估

> trait 是抽象机制的工具，不是装饰品。

- [ ] **多态必要性**：该 trait 是否有 ≥2 个**行为不同**的实现？都相同→P1
- [ ] **trait bound**：是否被用作泛型约束？从未→P1
- [ ] **单方法 trait**：只有1个方法？能否合并到已有 trait 或改为 enum 分发？
- [ ] **机制 vs 策略分离**：机制（页表映射）→属于 trait；策略（绑定进程地址空间）→属于上层，不在 trait
- [ ] **跨架构差异**：trait 每个方法在不同架构上实现真的不同？相同→考虑改为自由函数

**判定**：≥2个行为不同实现+被用作 bound→✅合理；所有实现相同+从未 bound→❌不必要（P1）；方法在所有架构上相同→❌（P1）。

---

## 3. 类型系统与安全

- [ ] typestate 真正约束状态？存在绕过路径？typestate 和 flags 严格一致？
- [ ] 可能同时持有同一对象多个 view（aliasing）？
- [ ] `unsafe` 最小化？每个块有安全契约注释？
- [ ] "逻辑正确但 Rust 内存模型下是 UB"？
- [ ] `MaybeUninit`/`UnsafeCell`/裸指针使用正确且必要？
- [ ] 手动 `unsafe Send`/`unsafe Sync` 安全性经过论证？
- [ ] Drop 语义清晰？隐式 drop 风险？

---

## 4. 执行模型与并发

> 用户态服务器假设单线程事件循环；内核模块必须考虑 SMP + BKL。

### 4.1 用户态服务器检查项（VM/PM/VFS/RS/DS/INET 等）

- [ ] 模块是否声明单线程假设？
- [ ] `Sync`/`Send` 不当实现？`UnsafeCell` 前提被保证？
- [ ] 单线程下也有 aliasing UB？
- [ ] 未来扩展多线程该设计会失效？
- [ ] `lazy_static`/`OnceCell` 初始化顺序正确？
- [ ] IPC 消息传递的指针通过安全 Wrapper 保证？

### 4.2 内核 SMP/BKL 检查项（Kernel 模块专用）

> Minix3 kernel 有 `CONFIG_SMP` + `spinlock_t big_kernel_lock`。虽然 BKL 限制临界区内最多一个 CPU，但 spinlock 的特性引入额外约束。

**BKL 持有验证**：
- [ ] 进入内核的路径是否持有 BKL？调用链上是否有 `BKL_LOCK()`？
- [ ] BKL 临界区内是否有可能睡眠/调度/等待 IPC 的操作？→ ❌ 禁止（spinlock 内不能 sleep）
- [ ] BKL 释放后重新获取时，共享状态是否可能已被其他 CPU 修改？→ 需检查"释放→重新获取"窗口
- [ ] 是否有遗漏的 BKL 释放点（如函数提前 return 未调用 `BKL_UNLOCK()`）？

**CPU-local 数据完整性与隔离**：
- [ ] per-CPU 数据是否通过 `get_cpu_var()`/`put_cpu_var()` 成对访问？
- [ ] 是否存在"读取 CPU A 的 local 变量，但当前运行在 CPU B"的路径？
- [ ] per-CPU 数据是否被非 BKL 保护的并发操作修改？

**共享状态保护**：
- [ ] 内核全局变量（`EXTERN`/`static`）是否有明确的并发保护策略？BKL 保护 / per-CPU / `Atomic*`？
- [ ] 是否存在"宣称 BKL 保护但实际访问时未持有 BKL"的路径？
- [ ] `static mut` 是否通过 BKL 或 `Atomic*` 正确保护？→ Rust 中 `static mut` 本身是 unsafe，需额外论证

**内存排序与可见性**：
- [ ] BKL spinlock 的 acquire/release 语义是否提供了足够的内存排序保证？
- [ ] 是否有依赖比 BKL 更弱的内存排序保证的代码？（如裸 `Relaxed` ordering 依赖隐式屏障）

**Rust 类型系统与 SMP**：
- [ ] `RefCell` 在内核中不能用于跨 CPU 共享数据（`RefCell: !Sync`）
- [ ] `Rc` 不能跨 CPU/线程使用（`Rc: !Send + !Sync`）→ 如需跨 CPU 共享引用计数 → `Arc`
- [ ] `Cell`/`RefCell` 在 per-CPU 数据中合理，但需注释"per-CPU，无并发访问"
- [ ] `UnsafeCell` 安全论据是否从"单线程"改为"BKL 保护"或"per-CPU 隔离"或"Atomic 操作"？

---

## 5. 内存模型与状态表达

- [ ] 混淆"未初始化内存"和"逻辑无效状态"？
- [ ] 在未初始化内存上读字段（UB）？
- [ ] `MaybeUninit` 仅为"空槽位"（误用）？
- [ ] Drop 用于资源释放而非状态管理？

---

## 6. 公开接口与模块设计

- [ ] `pub` 克制？最小权限原则？
- [ ] 模块高内聚？职责清晰？模块间依赖合理？
- [ ] **判断口诀**：「这个 pub 是因为外部需要，还是懒得组织？」「导出后外部能做什么？不该做的就不该导出。」

**实现模式**：`mod.rs` 不导出内部类型；`internal.rs` 含 `pub(crate)` 字段；`view.rs` typestate views 对外公开。

---

## 7. 命名与可追溯性

- [ ] Rust 规范（snake_case/CamelCase）？
- [ ] 与 Minix3 C 代码一致（优先同名，便于对照）？
- [ ] 能双向搜索（grep C 函数名找 Rust 代码）？
- [ ] 参数名与 Minix3 一致（如 `clicks` 非 `count`）？

---

## 8. 测试

- [ ] 覆盖：正常路径、边界条件、状态转换（含非法路径）？
- [ ] 每个 `unsafe` 函数/块有测试验证安全契约？
- [ ] 测试验证"语义"而非"实现细节"？无无意义测试（如测标准库行为）？
- [ ] 不属于本模块的测试应迁移；跨模块测试无冗余。

---

## 9. 注释与文档

### 9.1 注释覆盖率（强制）
- [ ] 每个 `pub` 函数/方法/类型有 `///` 文档注释
- [ ] 每个 `pub(crate)` 非自解释函数有注释
- [ ] 每个 `mod.rs` 有 `//!` 模块级注释
- [ ] 复杂算法有行内注释解释"为什么"；文件职责不显然有文件头注释

### 9.2 注释质量
- [ ] **注释使用英文**（原则）
- [ ] 文档注释描述契约和前置条件；`unsafe` 有 safety 注释
- [ ] 无冗余注释；无过多空行；关键决策解释"为什么不用另一种方案"

---

## 10. 64 位假设

- [ ] 代码基于 64 位编写？无 32 位残留（`u32` 用于地址/大小）？
- [ ] `as` 有损截断（`u64 as u32`）有无注释说明安全性？
- [ ] 利用 64 位优势（更大地址空间）？

---

## 11. 复杂度与工程性

> 防止过度设计的刹车系统。

- [ ] 设计明显复杂于问题本身？"理论优雅但工程不必要"？
- [ ] 为类型安全引入过高复杂度？能否用更简单模型？
- [ ] **trait 过度抽象**？参考 §2.5 判定标准

---

## 12. no_std 约束检查

> **除 mock/test 外，所有代码必须在 `no_std` 下运行。**

- [ ] 非 `#[cfg(test)]` 中 `use std::`？→ P0
- [ ] `Cargo.toml` 正确设置 `#![no_std]`？
- [ ] 第三方 crate 需 `std`？`alloc` 有全局分配器？
- [ ] mock/test 用 `#[cfg(test)]` 隔离？隐式依赖 `std`（如 `println!` 在非 test 中）？

**允许例外**：`#[cfg(test)]`、mock（需 feature gate）、build.rs

---

## §1.5 Design 对齐检查

> **核心**：如果 design 不抓本质（如漏掉核心概念），仅 review 代码实现无意义。这种情况应触发 Refactor（Gate H 阻断）。

| 检查项 | 期望 | 状态 |
|--------|------|------|
| design ↔ code 一致性 | ≥80%（Step 1.6.2）| ⚠️/✅ |
| design 是否定义核心 trait | 是 | ✅/❌ |
| code 是否实现 design 中所有决策 | 是 | ✅/❌ |
| design 是否抓到 Minix3 本质 | 是 | ✅/❌ |
| 错误码策略 | 严格对齐 Minix3 | ✅/❌ |
| `as` 截断阈值规则 | design 明文 | ✅/❌ |

> **关键判定**：
> - 一致性 ≥ 80% + design 本质正确 → ✅ 继续 review
> - 一致性 < 80% 但 design 正确 → **code Refactor**（修 code）
> - design 漏概念（design-missing）→ **design Refactor**（先补 design）
> - design 抓错本质（design-wrong）→ **design Refactor 必须**（先 redesign）

> **配套机制**：[review.md §Design First 原则](../review-rules/review.md) + [review-process.md §Step 1.6](../review-rules/review-process.md) + [review-process.md §Gate H design 门控](../review-rules/review-process.md)。

---

## 13. 设计-代码一致性检查

> 代码必须实现文档中的设计，不能各写各的。

- [ ] **Ch3 设计→代码**：每个设计决策代码中都有对应？typestate→裸整数=P0；enum→常量=P1
- [ ] **Ch4 描述→代码**：函数签名一致？类型定义一致？行为语义一致？
- [ ] **代码→Ch3/Ch4 追溯**：每个关键实现文档中有对应描述？实现未描述功能→P1；使用未提及类型→P1

---

## 14. C-Rust 语义对齐检查

> Rust 实现必须与 Minix3 C 语义对齐。不对齐必须注释说明原因。

### 14.1 对齐分级

| 级别 | 说明 | 要求 |
|------|------|------|
| 架构层 | 涉及整体架构变化 | 允许不对齐（文档 §2.5 标注） |
| 叶函数 | 不依赖内部函数、直接操作数据 | **必须对齐**，不对齐注释说明（P1） |
| 修正性 | C 源码有 bug，Rust 修正 | 允许不对齐，注释说明 bug+修正（P1） |

### 14.2 叶函数语义对齐（强制）

- [ ] 每个叶函数行为与 C 一致？返回值/错误码/副作用等价？
- [ ] 不对齐时注释说明：C 源码 bug（引用位置+描述）/Rust 类型约束/架构演进影响

### 14.3 C 有实现但 Rust 缺失（强制）

当 C 中某类型实现了某个回调/函数但 Rust trait impl 中无覆盖时：
- [ ] 缺失合理（默认实现即可）？默认行为与 C 回调行为一致？
- [ ] 不一致→P1（应覆盖）；缺失不合理→添加覆盖

### 14.4 Rust 注释中的 C 源码引用验证（强制）

- [ ] 注释引用的 C 函数名正确？描述的 C 行为与源码一致？源码位置准确？
- 函数名错误→P1；行为不符→P1；位置不准→P2

### 14.5 架构演进导致的函数消失
- [ ] 文档说明为什么不需要？代码注释"Minix3 有 xxx，因 [原因] 不再需要"？

### 14.6 修正性不对齐（允许但必须注释）

```rust
// Minix3 bug: anon_contig_reference() returns ENOMEM but region.c ignores
// the return value (line 841). Rust fix: ev_copy returns NotSupported to
// correctly reject fork for contiguous memory regions.
```

### 14.7 执行策略
1. 优先检查叶函数；2. 抽样验证高风险（返回错误码/涉及内存/涉及并发）；3. 发现一个检查同类。

---

## 15. 细节精确性检查

> 第二层检查，跨阶段复用。Agent 标记可疑点，人工确认。

### 15.1 外部知识可验证
- [ ] 注释引用外部知识（硬件规范/协议/API）？
- [ ] 引用的外部知识在上下文中仍然成立？（legacy 特性在新模式？）
- 不符→P1

### 15.2 通用接口纯度
- [ ] 共享结构体/trait/API 的每个字段对所有消费者有意义？
- [ ] 仅特定上下文有意义的字段是否标注？（Option/注释/扩展结构）
- 未标注→P1

### 15.3 返回值完整性
- [ ] 外部调用返回值被处理/传递/注释说明可丢弃？
- [ ] 关键返回值（内存映射/状态码）被无说明忽略？
- 无说明忽略→P1

### 15.4 资源生命周期闭环
- [ ] 资源获取有对应释放路径或"不释放"理由？
- [ ] 释放责任方明确？
- 无路径且无说明→P1

### 15.5 理由可质疑性
- [ ] "因为/由于/避免"类注释的理由在上下文中成立？
- [ ] 理由是否唯一/最真实？
- 明显不符→P1；严重误导设计→P0
