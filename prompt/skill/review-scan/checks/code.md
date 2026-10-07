# code: 代码正确性检查（16 个 code check）

> 本文件合并代码检查清单（源：`prompt/review-rules/review-code-checklist.md`，§1~§15），解决 attention decay 和过度拆解问题。
> **强制规则**：每个检查必须先执行 grep/read，再下结论。每个判定标注 evidence [DIRECT/MEDIUM/INFERRED]。
> **⛔ 前置**：进入本文件任何检查前，必须已通过 [SKILL.md Phase 1 §Step 0 硬阻断预检](../SKILL.md)。`{NN}-design.v*.md` / `{NN}-outline.v*.md` 缺失 → **Gate H.1/H.6 FAIL → Step 0.3 嵌入生成**（模式 69 PSMD 触发）。

---

## 通用规则（适用于所有 code check）

1. **先读后判**：每个判定必须先执行 grep/read 验证，再下结论。禁止凭印象判断。
2. **Evidence 分级**：[DIRECT] grep 直接验证 / [MEDIUM] 名称匹配需语义确认 / [INFERRED] 上下文推断（标"待确认"）。
3. **零输出禁令**：0 问题也必须写"Checked N items, found 0 issues"。
4. **证据强制**：每个检查必须包含 grep count / file count / line range 作为证据。
5. **Attention Decay**：code checks 是高风险被跳过项——每完成 4 个 check 后暂停，重读规则 1 和 3。

---

## Check 01: Rewrite 质量

> 最核心维度。发现 Translate 味道必须指出。C-Rust 语义对齐由 Check 15 单独覆盖。

- [ ] 裸整数表达语义（应 newtype/enum）、C 式哨兵值（裸 `0`/`-1`，应用 `Option<T>`）、魔术数字、C 式 flag 组合、C 式错误码传递（应用 `Result + ?`）
- [ ] C 宏直接翻译成 Rust 宏（应用 trait/泛型替代）；C 式数据结构未用 Rust 所有权重构
- [ ] **错误码对齐**：`Result<T, Error>` 封装的错误码与 Minix3 errno 严格对应，禁止合并/创造新错误语义 → P0
- [ ] **内存所有权清晰**：C 分配/释放点在 Rust 有对应 `Owner`；C"借用"未被误用为"所有权"
- [ ] "非法状态不可表达"（invalid states unrepresentable）；无"为了 Rust 而 Rust"的过度设计

## Check 02: 硬件抽象

> **强制规则：所有硬件都必须被抽象为 trait。** 抽象机制，而非描述硬件；描述"做什么"，而非"怎么做"。

- [ ] 具体硬件语义（CR3、TSS、MSR、I/O 端口）泄漏到 OS 层 → ❌ 必须拒绝（P0）
- [ ] 硬件交互全部通过 trait 抽象；trait 描述"OS 需要什么机制"而非"硬件怎么做"
- [ ] 数据结构含架构特定硬件字段（如 `pde` 数组）→ ❌ 应由 trait impl 内部管理
- [ ] `#[cfg(target_arch)]` 选择硬件行为 → ❌ 应通过 trait 静态分派
- [ ] OS 语义类型（`PageFlags`）与硬件编码分离；mock 硬件实现所有必需 trait；trait 设计考虑 x86-64/arm64/RISC-V 64 共性

## Check 03: Trait 设计质量

- [ ] **多态必要性**：trait 有 ≥2 个**行为不同**的实现？0 impl → P0（死代码/虚构）；1 impl → P1（需 ≥2）
- [ ] **trait bound 使用**：被用作泛型约束（`where T: Trait` / `impl Trait`）？从未作为 bound → P1
- [ ] 单方法 trait：能否合并到已有 trait 或改为固有方法 + enum 分发？
- [ ] **机制 vs 策略分离**：机制（页表映射）→ trait；策略（绑定进程地址空间）→ 上层，不应在 trait 中
- [ ] 跨架构差异验证：方法在各架构实现相同 → 改为自由函数/固有方法/关联常量（P1）

## Check 04: 类型安全

- [ ] typestate 真正约束状态，无绕过路径；typestate 与 flags 严格一致
- [ ] 同一对象多 view（aliasing 问题）；`unsafe` 最小化且每块有安全契约
- [ ] "逻辑正确但在 Rust 内存模型下是 UB"的代码；`MaybeUninit`/`UnsafeCell`/裸指针必要且正确
- [ ] 手动 `unsafe Send`/`unsafe Sync` 有安全性论证；Drop 语义清晰

## Check 05: 执行模型与并发（含 Kernel SMP/BKL）

- [ ] **用户态服务器**：单线程假设显式声明；`Sync`/`Send` 无不当实现；`UnsafeCell` 前提明确保证；未来多线程扩展不失效
- [ ] **Kernel SMP/BKL**：进入内核每条路径持 BKL；无未持间隙；临界区内无睡眠/调度/等待 IPC（→ P0 spinlock 死锁）；无提前 return 导致 BKL 未释放
- [ ] per-CPU 数据通过 `get_cpu_var()`/`put_cpu_var()` 成对访问，无跨 CPU 读取，无非 BKL 保护的并发修改
- [ ] 内核共享状态：`Rc`/`RefCell` 跨 CPU → ❌（`!Send + !Sync`）；`UnsafeCell` 安全论据为"BKL 保护 / per-CPU 隔离 / Atomic"，非"单线程"；跨 CPU 共享用 `Arc + Mutex`/`Atomic`

## Check 06: 内存模型与状态表达

- [ ] 未混淆"未初始化内存"与"逻辑无效状态"；不在未初始化内存上读字段（UB）
- [ ] `MaybeUninit` 仅用于"空槽位"（通常误用）；Drop 用于资源释放而非状态管理

## Check 07: 模块设计与 pub 卫生

- [ ] `pub` 克制，遵守最小权限原则（口诀："这个 pub 是外部需要，还是内部懒得组织？"）
- [ ] 模块高内聚、职责清晰；模块间依赖清晰；本应 private 却暴露的 API

## Check 08: 命名与可追溯性

- [ ] Rust 规范命名（snake_case/CamelCase）；与 Minix3 C 名称一致（便于 grep 对照）
- [ ] 参数命名与 C 一致（`clicks` 而非 `count`、`base` 而非 `addr`）；grep C 函数名可找到对应 Rust 代码

## Check 09: 测试

- [ ] 覆盖正常路径、边界条件、状态转换（含非法路径）；每个 `unsafe` 有安全契约测试
- [ ] 测试验证"语义"而非"实现细节"；无无意义测试（测试标准库行为）；无跨模块冗余/错位测试

## Check 10: 注释与文档

- [ ] 每个 `pub` 函数/方法/类型有 `///`；模块有 `//!`；复杂算法有"为什么"行内注释
- [ ] **注释必须英文**；`unsafe` 有 safety 注释；关键设计决策解释"为什么不选常见方案"
- [ ] 判定：Reviewer 需读代码才能理解 `pub` 接口用途 → 缺文档注释

## Check 11: 64 位假设

- [ ] 无 32 位残留（`u32` 作地址/大小）；`as` 有损截断（`u64 as u32`）有安全性注释 → 无注释 → P1
- [ ] 利用了 64 位优势（更大地址空间）

## Check 12: 复杂度与工程性

- [ ] 设计不复杂于问题本身；无"理论优雅但工程不必要"；类型安全未引入过高复杂度
- [ ] trait 无过度抽象（"为了 trait 而 trait" → P1，见 Check 03）

## Check 13: no_std 约束

> 详见 [review.md §运行时环境约束](../../../../prompt/review-rules/review.md)。

- [ ] `use std::`（非 `#[cfg(test)]`）→ P0；`#![no_std]` 正确设置；无需要 `std` 的第三方 crate
- [ ] `alloc` 使用提供全局分配器实现；mock/test 用 `#[cfg(test)]` 隔离；无隐式依赖 `std`（`println!`、非 test 中 `Vec::new`）
- [ ] 允许例外：`#[cfg(test)]` 模块 / mock（feature gate 隔离）/ build.rs

## Check 14: 设计-代码一致性

- [ ] Ch3 每个设计决策有代码实现（typestate→裸整数 → P0；enum→常量 → P1）；Ch4 实现描述与代码匹配（签名/类型/语义）
- [ ] 代码→Ch3/Ch4 追溯：代码实现未描述功能 → P1（超出设计）；文档未提及类型/模式 → P1

## Check 15: C-Rust 语义对齐

- [ ] **对齐分级**：架构层（允许不对齐，doc §2.5 标注演进理由）/ 叶函数（**必须对齐**，不对齐须注释说明原因 → P1）/ 修正性（C bug，Rust 修正，注释说明 → P1）
- [ ] **叶函数语义**：返回值/错误码/副作用与 C 一致；不对齐有原因注释（C bug 引用位置 / Rust 类型约束 / 架构演进）
- [ ] **C 有实现但 Rust 缺失**：确认缺失合理（默认实现行为与 C 一致）；C 有特化行为但 Rust 依赖默认 → P1
- [ ] **注释中的 C 源码引用验证**：函数名/行为/位置引用错误 → P1/P2；C bug 修正注释含 `// MINIX3 BUG:` 标注（模式 78）
- [ ] 架构演进导致函数消失：文档说明 + 代码注释"Minix3 有 xxx，因 [演进] 不再需要"

## Check 16: Precision Check（细节精确性）

> 5 个元规则跨阶段复用，适用于 Boot/VM/PM/VFS 等所有模块。标记可疑点，输出待人工确认清单。

1. **外部知识可验证**：注释引用的硬件规范/协议/API 行为可独立验证且当前上下文成立 → 不符 → P1
2. **通用接口纯度**：共享结构体/trait/公共 API 字段对所有消费者有语义意义；上下文特定元素显式标注（`Option<T>` / 架构扩展 / 注释）→ P1
3. **返回值完整性**：外部调用返回值被处理；忽略时有显式"可安全丢弃"理由；关键返回值（内存映射/状态码）有替代来源 → P1
4. **资源生命周期闭环**：每个资源获取有对应释放路径；"不释放"有显式理由（boot 一次性/静态全局/移交内核）；释放责任方明确 → P1
5. **理由可质疑性**："为什么"解释在上下文中成立；涉及性能/安全/大小的理由有量化依据 → 明显不符 → P1（轻微）/ P0（严重误导设计决策）

---

## 判定汇总

- 每个 Check 输出：| Check | 验证命令 | 结果 | 判定 | Evidence |
- Gate D §0 P0 必检 5 项（test 存在 / trait ≥2 impl / 函数位置 / 算法非 stub（含 `spin_loop!`）/ §4 签名一致）为**强制**项，PARTIAL/⚠️ = ❌ FAIL
- 任何 ❌ → scan.md DRAFT，禁止写入 STATE.md
