---
name: "review-process-skill"
description: "Minix-RS Review 执行流程。定义强制步骤 Step 0-7、Blocker Gates、每个 Step 的中间产物格式、自检清单、以及工具命令速查。当 Agent 进入 Review 执行阶段时调用此 Skill。"
---

# Minix-RS Review 执行流程

> 每个 Step 必须产生**可见的中间产物**（表格、列表、grep 输出）。不允许"在脑子里过一遍"然后跳到最终输出。

---

## 〇、执行模式选择（构造 / 快速 / 深度）

> 根据任务规模和精度要求选择模式，不同模式裁剪不同 Step。

| 模式 | 适用场景 | 执行 Step | 预计耗时 |
|------|---------|----------|---------|
| **构造（Constructive）** | 初稿阶段，引导补全 | 0, 1, 1.5, 2, 5, 6 | 15~30 分钟 |
| **快速（Quick）** | 日常 PR、时间有限 | 0, 1, 2, 5 | 10~20 分钟 |
| **深度（Deep）** | 里程碑验收、关键模块 | 0-7（全量） | 40~120 分钟 |

**决策树**：初稿→构造；日常PR/时间紧→快速（P0>3 则升级深度）；里程碑/关键模块→深度。

---

## ⛔ Blocker Gates（阻断门，必须通过才能输出 Final Review）

| Gate | 检查项 | 通过标准 | 未通过后果 |
|------|--------|---------|-----------|
| **A** | Step 1.5 Coverage Enumeration | 已运行 coverage-extract.py + scan.md 附 SYMBOLS.md 路径 | 禁止输出 Final Review |
| **B** | Step 2 Diff Extraction | 已产出 Top 5 行为契约表（3 语义偏移 + 2 覆盖缺口，8 字段 × 5 函数） | 禁止输出 Final Review |
| **C** | Step 3.5 Precision Check | 已产出 5 元规则检查表 | 禁止输出 Final Review |
| **D** | P0 必检清单（见 patterns-skill §0） | 5 项已回答（✅/❌ + grep 证据） | 禁止输出 Final Review |
| **E** | Step 4.5 Test Verification | 文档 §5 每个测试函数已 grep 验证（若文档有 §5） | 禁止输出 Final Review |

**任一 Gate 未通过 → scan.md 标记 DRAFT，禁止写入 STATE.md。**

**Gate 证据规则（新增）**：
- scan.md 中每个 Gate 的通过声明必须附带**实际命令 + 输出片段**作为证据。
- Gate A：附 coverage-extract.py 命令行及 stdout 覆盖率摘要。
- Gate D：附 5 项 P0 必检的 grep/Read 证据（命令 + 结果）。
- Gate E：附每个测试函数名的 `rg "fn {name}"` 命令 + 结果表。
- 仅有 "✅ 通过" 而无证据的 Gate 视为未通过。

**Skill Invocation Log**（mandatory in scan.md）：
```markdown
## Skill Invocation Log
| # | Skill | 调用时机 | 关键产出 |
|---|-------|---------|---------|
| 1 | review-doc-skill | Step 3 | §6 概念准确性表 |
| 2 | review-code-skill | Step 5 | 代码质量审查 |
```
未含此章节 → scan.md 标记 DRAFT。

---

## Step 0: 范围声明 + 时间预算 + 状态恢复

- 按 §〇 确定执行模式（构造/快速/深度）
- 声明 Review 模式和范围
- 声明时间预算（可选；若填写，按 | <200行→10-20分 | 200-500→20-40 | 500-1000→40-80 | >1000→80-120 |）
- **读取状态（双路径）**：
  - **Trae IDE** → 读取 `notes/rewrite/{module}/.review/STATE.md`
  - **Claude Code Runtime** → 读取 `.review/{module}/STATE.md`（项目根）
  - 若两个 STATE.md 都存在且内容矛盾，**不要自动合并**，在 scan.md 中记录分歧并询问用户哪个为准。
  - **`{module}` 的确定**：取目标文档所在目录的**直接父目录名**。例如 `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/03-kmain-cstart.md` → `{module}=fork-syscall-rewrite`。这与覆盖率脚本 `--module kernel`（Minix3 模块名）和 `--output .review/kernel/...` 是**两个不同概念**，不得混用。

**中间产物**：
```
- **执行模式**：[构造/快速/深度]
- **Review 模式**：[文档/代码/完整/局部]
- **目标文件**：xxx.md / xxx.rs
- **Step 2.5/Step 4.5 是否适用**：[适用/不适用（原因）]
- **规模**：约 N 行 | **预计**：X~Y 分钟
- **前置状态**：STATE.md 存在 → 已完成 phase [X, Y, Z]，待完成 [A, B, C] / STATE.md 不存在 → 从零开始
```

---

## Step 1: Ground Truth Lookup（源码定位）

- 识别文档中所有 Minix3 源文件
- 用 `rg` 验证文件是否存在于 `minix3/` 目录
- 记录每个引用的文件路径和行号范围

> 效率：文档>500行，先用 grep 提取 `.c`/`.h` 引用，抽样验证行号；Step 3 再精确验证。

**中间产物**：
```markdown
### Step 1 产物：源码文件清单

| 文件路径 | 文档引用位置 | 文件存在? | 引用行号范围 |
|---------|------------|----------|------------|
| minix3/minix/servers/vm/pb.c | Ch2§2.3 | ✅ | 33-168 |
```

---

## Step 1.5: Coverage Enumeration（覆盖率穷举，机器+AI）— **Gate A**

> **目的**：机器生成穷举清单，AI 只负责语义判断。解决覆盖率不足和跨轮次累积问题。
> **详见**：[review-coverage-skill](review-coverage-skill.md)

**执行步骤**：

1. **机器生成 SYMBOLS.md 骨架**：
   ```bash
   # 服务器模块（vm / pm / vfs / rs / ds / inet ...）
   python3 tools/coverage-extract/coverage-extract.py {module} {doc_dir} \
     --rust-dir os --c-dir minix3/minix/servers/{module} \
     --semantic-map tools/coverage-extract/{module}-semantic-map.json \
     --doc-file {target-doc-name}.md \
     --output .review/{module}/{target-doc-name}/SYMBOLS.md

   # 内核
   python3 tools/coverage-extract/coverage-extract.py kernel {doc_dir} \
     --rust-dir os --c-dir minix3/minix/kernel \
     --semantic-map tools/coverage-extract/kernel-semantic-map.json \
     --doc-file {target-doc-name}.md \
     --output .review/kernel/{target-doc-name}/SYMBOLS.md
   ```
   > **目录创建**：脚本已修复为使用 `--output` 时自动创建父目录；若使用旧版本脚本，请先 `mkdir -p $(dirname .review/kernel/{target-doc-name}/SYMBOLS.md)`。
   - `--rust-dir os`：扫描整个 `os/` 目录，避免遗漏跨 crate 实现（如 `kmain` 在 `os/kernel/src`，`ProtectionArch` 在 `os/arch/src`）。
   - `--c-dir`：服务器模块用 `minix3/minix/servers/{module}`，内核用 `minix3/minix/kernel`。
   - `--semantic-map`：对 C→Rust 改写项目必须提供语义映射表，否则 Rust 覆盖率会严重低估。
   - `--doc-file`：当 review 单篇文档时，必须限定到该文档，确保 coverage 数字与文档一一对应。
   - 若 `{module}-semantic-map.json` 不存在，先用空文件或从 `kernel-semantic-map.json` 裁剪。

2. **AI 补充语义判断**（5 项，每项标注 evidence [DIRECT/MEDIUM/INFERRED]）：
   - Rust 对应关系确认（名称匹配 ≠ 语义对应）
   - 架构演进标记（ARCH: 不需要 + 理由）
   - 语义归属判定（以功能语义为准）
   - 行为契约表（核心函数：输入/输出/副作用/错误码/时序）
   - 测试覆盖补充（L1对偶/L2契约/L3 doctest）

3. **更新 STATE.md Coverage Status 段**

**中间产物**：
```markdown
### Step 1.5 产物：覆盖率穷举（Gate A）

**SYMBOLS.md**: .review/{module}/SYMBOLS.md

| 指标 | 数值 |
|------|------|
| C 符号总数 | N |
| 文档覆盖 | M (M%) |
| Rust 覆盖 | K (K%) |
| 完全缺口 | G |
| 架构演进 | A |

**P0 缺口**:
| 符号 | C 源码 | 判定 | evidence |
|------|--------|------|----------|
| `func_name` | file.c:N | P0 缺口 | DIRECT: rg 无结果 |

**ARCH 标记**:
| 符号 | 理由 | evidence |
|------|------|----------|
| `map_service` | IPC 协议演进 | INFERRED |
```

> **反幻觉**：每个覆盖判定必须先执行 grep 验证，再下结论。

---

## Step 2: Diff Extraction（差异提取）— **Gate B**

- 列出 **5 个**最背离 Minix3 原始语义的地方：
  - **Top 3 语义偏移**：文档/代码描述与 C 行为不符（行为契约表）
  - **Top 2 覆盖缺口**：来自 Step 1.5 SYMBOLS.md 的"文档覆盖=✅ 但 Rust 覆盖=❌"符号
- 说明：Minix3 实际行为、文档/代码中的描述、差异性质
- 每项填写 8 字段行为契约表（见 [core-semantics-skill](review-core-semantics-skill.md)）

**中间产物**：
```markdown
### Step 2 产物：Top 5 差异（Gate B）

#### Top 3 语义偏移
| # | 差异点 | Minix3 行为 | 文档/代码描述 | 差异性质 |
|---|--------|------------|-------------|---------|
| 1 | xxx | pagetable.c:333 实际 | 文档 L245 描述 | 概念错误 |

#### Top 2 覆盖缺口（来自 SYMBOLS.md）
| # | 符号 | C 源码 | 文档覆盖 | Rust 覆盖 | 缺口性质 |
|---|------|--------|---------|----------|---------|
| 4 | func_x | file.c:N | ✅ | ❌ | 实现缺失 |

#### 行为契约表（8 字段 × 5 函数）
[见 core-semantics-skill §2.2 模板]
```

---

## Step 2.5: Link Validation（链路验证）

> 局部 Review（仅 Ch1&2）时跳过。

1. Ch3→Ch1&2：每个设计决策是否有依据？
2. Ch4→Ch3：每个实现是否对应设计？
3. 测试→Ch3+Ch4：测试是否覆盖设计和实现细节？
4. 代码→Ch4：代码是否与文档一致？

**中间产物**：
```markdown
### Step 2.5 产物：链路验证

**Ch3→Ch1&2**：
| Ch3 设计决策 | Ch3 位置 | Ch1&2 依据 | 链路状态 |

**Ch4→Ch3**：
| Ch4 实现 | Ch4 位置 | Ch3 设计依据 | 链路状态 |

**测试→Ch3+Ch4**：
| 测试要点 | 位置 | 覆盖的设计/实现 | 链路状态 |

**代码→Ch4**（如适用）：
| Ch4 描述 | Ch4 位置 | 代码位置 | 一致? |
```

---

## Step 3: Sanity Check（一致性检查）

- 验证文档第 2 章引用的所有行号
- 验证所有数值常量
- 验证所有函数签名
- 按文档检查清单格式输出各维度验证表格

---

## Step 3.5: Precision Check（细节精确检查）— **Gate C**

> 大方向检查之后的第二层。5 个元规则跨阶段复用。Agent 标记可疑点，不要求自动判定正确性。

**3.5.1 外部知识标记**
- 扫描注释中的寄存器/标志位/协议/规范引用
- 输出标记列表供人工验证
- 口令："这个硬件行为/协议要求在上下文中成立吗？"

**3.5.2 通用接口纯度扫描**
- 扫描共享结构体/trait/公共 API 的字段/方法
- 问："对所有消费者上下文都有意义吗？"
- 标记仅在特定上下文有意义的元素

**3.5.3 返回值完整性扫描**
- 扫描外部调用返回值是否被使用/传递/注释说明可丢弃
- 标记被忽略且无说明的返回值

**3.5.4 资源生命周期闭环扫描**
- 扫描资源获取点，检查释放点或"不释放"理由
- 口令："谁释放？什么时候？不释放的理由？"

**3.5.5 理由可质疑性扫描**
- 扫描"因为/由于/避免/为了"类注释
- 输出"理由需质疑"标记列表

**中间产物**（Gate C）：
```markdown
### Step 3.5 产物：Precision Check（Gate C）

| 元规则 | 位置 | 内容 | 人工确认建议 |
|--------|------|------|------------|
| 3.5.1 外部知识 | vm.rs:267 | CR4.PSE 注释 | 验证 x86-64 长模式 |
| 3.5.4 资源生命周期 | vm.rs:801 | Option 未 take() | 验证清理路径 |
```

---

## Step 4: Cross-Document Check（跨文档联动）

> 局部 Review 跳过。

- 优先检查同目录文档，其次"参见"章节外部文档
- 检查共享数据结构（`vmproc`/`vir_region`）、共享常量（`CLICK_SIZE`）、跨模块调用
- 重复处理检查：同目录已有完整处理的概念→精简为引用

### 4.1 语义归属判定（grep 辅助）

1. 提取 Ch1&2 中所有 C 符号（函数/结构体/宏名）
2. grep 在同目录 `.md` 中搜索
3. 以**功能语义**为准判定归属（如 `vm_mappages` 定义在 `mmap.c` 但语义属"页表操作"）

```markdown
| 符号 | 类型 | 语义归属 | 当前覆盖状态 | 处理建议 |
|------|------|---------|-------------|---------|
| vm_mappages | 函数 | 07-pagetable-ops.md | 未覆盖 | 在 07 中补充 |
| pt_t | 结构体 | 06-pagetable-struct.md | 已覆盖 | 无需处理 |
```

### 4.2 Design Quality Check（设计质量检查）

1. 列出 Ch3 所有设计决策
2. 检查：可追溯性、场景覆盖、no_std 可行性
3. 发现更好替代方案→Ch3 加 TODO

### 4.2.1 Rust 代码设计质量检查

1. 列出所有自定义 trait
2. 检查：多态必要性、trait bound 使用、机制vs策略分离
3. 不必要的 trait→P1

---

## Step 4.5: Test Verification（测试章节验证）— **Gate E**

**适用条件**：文档含 §5 测试章节（或类似测试要点章节）

**执行步骤**：
1. 提取文档 §5 列出的所有测试函数名
2. 对每个测试函数名执行 grep：
   ```bash
   rg "fn {test_name}" {rust_dir} --type rust -n
   ```
3. 判定：
   - 存在 → ✅
   - 不存在 → ❌ **P0（测试缺失）**

**中间产物**（Gate E）：
```markdown
### Step 4.5 产物：测试章节验证（Gate E）

| 文档 §5 测试名 | grep 命令 | grep 结果 | 判定 |
|---------------|----------|----------|------|
| test_vmctl_clear_pagefault | `rg "fn test_vmctl_clear_pagefault" os/` | 0 matches | ❌ P0 缺失 |
| test_vmctl_param_from_u32 | `rg "fn test_vmctl_param_from_u32" os/` | vm.rs:1333 | ✅ 存在 |

**统计**：文档承诺 N 个测试，实际存在 M 个，缺失 N-M 个 → P0
```

> **若文档无 §5**：输出 "文档无 §5 测试章节，Gate E 不适用"，不阻断。

---

## Step 5: Final Review Output（最终输出）

- 按输出模板整理发现
- 每个问题标优先级（P0/P1/P2）+ 明确修改方向
- 必须含：维度覆盖自检、最弱项自检、时间预算评估、**Skill Invocation Log**、**Blocker Gates 通过状态**

### Step 5.5: 状态写入与收敛判断

> 完成当前 phase 的验证后，将结果持久化写入工具对应的路径（见 Step 0 双路径规则）。

**中间产物（精简版）**：
1. 更新或创建工具对应的 `STATE.md`：
   - Trae → `notes/rewrite/{module}/.review/STATE.md`
   - Claude → `.review/{module}/STATE.md`
2. 更新或创建 `SYMBOLS.md`（Step 1.5 机器产物）：
   - 单文档 review → `.review/{module}/{target-doc-name}/SYMBOLS.md`
   - 模块级 review → `.review/{module}/SYMBOLS.md`
3. **所有维度结果写入 scan.md 单文件**（NOT 10 个维度检查文件）。
   - 若用户显式指定输出位置，**双写**：用户指定路径 + 工具默认路径（Trae: `notes/rewrite/{module}/.review/scans/`; Claude: `.review/{module}/`）。

**STATE.md 格式**：

```markdown
# Review State: {module-name}

- **Phase**: [concept-check | ref-check | struct-check | coverage | design | link | code | cross-doc | claims | verify | complete]
- **Last completed phase**: concept-check
- **Open P0 issues**: 3 (#1, #2, #3 from scan.md)
- **Open P1 issues**: 7
- **Open P2 issues**: 2
- **Convergence status**: NOT_CONVERGED (5 phases remaining)
- **Next action**: Run ref-check phase with fresh context
- **Blocker Gates**: A✅ B✅ C✅ D✅ E✅ (all passed)

## Phase Completion Log
| Phase | Date | Passes | P0 found | P1 found | P2 found |
|-------|------|--------|----------|----------|----------|

## 新增/关闭问题同步规则（强制）
- 每次 review 结束后，必须将 scan.md 中的 **新发现 P0/P1/P2** 同步到 STATE.md 的 Open P0/P1/P2 列表。
- 不能仅在 Phase Completion Log 中记录；Open 列表必须实时更新。
- 已修复的问题从 Open 列表移除，并移动到 "Closed Issues" 段落，注明修复 scan/日期。

## Convergence Checklist
- [ ] §2.1 概念准确性 — COMPLETE / 0 new P0
- [ ] §2.2 C引用验证 — COMPLETE / 0 new P0
- [ ] §2.3 数据结构覆盖 — COMPLETE / 0 new P0
- [ ] §2.8 源码覆盖完整性 — COMPLETE / 0 new P0
- [ ] §2.9 设计决策质量 — COMPLETE / 0 new P0
- [ ] §2.10 章节链路 — COMPLETE / 0 new P0
- [ ] Code §1-14 — COMPLETE / 0 new P0
- [ ] 跨文档联动 — COMPLETE / 0 new P0
- [ ] Claims-Evidence — COMPLETE / 0 new P0
- [ ] 独立验证 — COMPLETE / PASS
```

**增量 Review 策略**：
1. 启动时读取 `.review/{module}/STATE.md`
2. COMPLETE 的维度→跳过（读 scan.md 对应章节总结即可，不重做）
3. UNCHECKED 的维度→执行完整验证
4. 代码/文档有修改→检查是否影响已 COMPLETE 维度（有影响→标记 NEEDS_RECHECK）
5. 更新 STATE.md 和 scan.md

**收敛判断**：
```markdown
### 收敛状态评估

- [ ] 全维度覆盖：10/10 维度 COMPLETE
- [ ] P0 收敛：最近 Pass 新增 P0 = 0
- [ ] P1 收敛：最近 Pass 新增 P1 ≤ 1
- [ ] 独立验证：VERIFY-CHECK = PASS（**必须完成，不能跳过**）
- [ ] Blocker Gates：A-E 全部通过，且每个 Gate 都有证据附件

**当前状态**：CONVERGED / NOT_CONVERGED (N phases remaining)
**下一步**：[下一阶段名称] 或 [执行独立验证] 或 [审查已收敛，可结束]
```

> **重要**：VERIFY-CHECK.md 是收敛终止的必要条件。未完成 VERIFY-CHECK 时，状态必须为 NOT_CONVERGED，即使 P0=0。

### Step 5.6: Review Verification Protocol（独立验证）

> **目的**：解决"自己审自己"的盲区。在所有维度 COMPLETE 后，在新会话中独立验证审查质量。
> 用户指令：「验证 review」或「review of review」

**执行步骤**（独立会话）：
1. 读取 `.review/{module}/STATE.md` + `scan.md` + 原始文档/代码
2. **随机抽样**：从 scan.md 的 Issue List 中随机选取 20% 的已报告问题
3. **反向验证**：对每个抽样问题——source evidence 是否充分？判定等级是否合理？
4. **遗漏检查**：抽样 20% 的源码符号，验证是否都在文档/检查覆盖
5. **收敛验证**：检查 STATE.md 的 Convergence Checklist 是否有已标记 COMPLETE 但实际未完成的维度
6. **Blocker Gates 复验**：检查 scan.md 是否真的通过了 A-E 全部 Gate

**中间产物**（写入 `.review/{module}/VERIFY-CHECK.md`）：
```markdown
### Review Verification Result

**抽样一致性**：X/Y = Z%
**遗漏检查**：N 符号抽样，M 遗漏
**收敛验证**：K/10 维度可信
**Blocker Gates 复验**：A-E 全部真实通过? ✅/❌

| 抽样问题 | scan.md 判定 | 独立重新判定 | 一致? |
|---------|-------------|------------|-------|

**判定**：PASS / CONCERN / FAIL
```

**判定标准**：
- **PASS**：一致性 ≥ 90%，无遗漏 key symbols，收敛状态可信，Gates 真实通过 → 审查完成
- **CONCERN**：一致性 70-90% → 特定维度需重新审查（标注在 STATE.md）
- **FAIL**：一致性 < 70% 或发现关键遗漏 → 标记 STATE.md 中相关维度为 NEEDS_RECHECK

---

## Step 6: Action Item Generation（修改项生成）

> 局部 Review 跳过。将发现转化为可执行修改项。

对每个 P0/P1 问题生成：

```
### TODO #N: [简述]
- **优先级**: P0/P1
- **类型**: 设计缺陷 / 代码-设计不一致 / no_std 违规 / 语义偏移 / ...
- **文件**: `path/to/file.rs`
- **问题**: [详细描述]
- **修改方案**: [具体方案]
- **验证**: [如何验证修改正确]
```

P0 必须有代码修改项。P1 涉及设计改进→Ch3 加 TODO 段落。

---

## Step 7: 自检清单确认（强制）

> 以下任何一项未完成，回到对应 Step 重新执行。

```markdown
### Step 7 产物：自检清单

- [ ] Step 0 范围声明和时间预算已输出
- [ ] Step 0 STATE.md 状态已检查
- [ ] Step 1 源码文件清单已输出
- [ ] **Gate A**: Step 1.5 覆盖率穷举已输出（SYMBOLS.md + 缺口/ARCH 判定）
- [ ] **Gate B**: Step 2 Top 5 差异已输出（3 语义偏移 + 2 覆盖缺口 + 8 字段契约表）
- [ ] Step 2.5 链路验证表格已输出（如适用）
- [ ] Step 3 概念准确性表格已输出
- [ ] Step 3 C 代码引用验证表格已输出
- [ ] Step 3 数据结构覆盖表格已输出
- [ ] Step 3 C 源码覆盖完整性表格已输出（含覆盖率）
- [ ] Step 3 文档风格验证表格已输出（§2.11）
- [ ] **Gate C**: Step 3.5 Precision Check 5 元规则检查表已输出
- [ ] Step 4 跨文档检查已输出
- [ ] Step 4.1 语义归属判定已输出（如适用）
- [ ] Step 4.2 设计决策质量表格已输出（如适用）
- [ ] **Gate D**: P0 必检清单 5 项已回答（见 patterns-skill §0）
- [ ] **Gate E**: Step 4.5 测试章节验证已输出（若文档有 §5）
- [ ] Step 5 维度覆盖自检表格已输出
- [ ] Step 5 最弱项自检 8 问题已确认（含 Blocker Gates）
- [ ] Step 5 时间预算评估已输出
- [ ] **Skill Invocation Log** 已输出
- [ ] Step 5.5 STATE.md 和 scan.md 已写入（NOT 10 个维度文件）
- [ ] Step 5.5 收敛状态评估已输出
- [ ] Step 6 修改项已生成（P0 必须有代码修改项）
- [ ] 所有 grep 命令输出作为证据附在对应表格后
```

---

## 工具命令速查

```bash
# ===== {module} = vm/pm/vfs/kernel 等 =====

# 验证常量
rg "^#define CONSTANT" minix3/minix/servers/{module}/ -n
rg "^#define CONSTANT" minix3/minix/kernel/ -n

# 验证函数
rg "^return_type function_name\(" minix3/minix/servers/{module}/ -n

# 验证结构体
rg "^struct struct_name " minix3/minix/servers/{module}/ -n
rg "^typedef struct" minix3/minix/servers/{module}/ -A 5

# 验证宏
rg "MACRO_NAME" minix3/minix/servers/{module}/ --type c -n

# 枚举值（头文件）
rg "ENUM_VALUE" minix3/minix/include/ --type h -n

# 全局搜索
rg "SYMBOL_NAME" minix3/minix/ --type c --type h -n

# 列出 C 源文件
ls minix3/minix/servers/{module}/*.c
ls minix3/minix/servers/{module}/*.h

# 提取函数定义
rg "^[a-z_].*\w+\(.*\)\s*$" minix3/minix/servers/{module}/FILE.c -n

# 跨文档搜索
rg "SYMBOL_NAME" notes/rewrite/{module}/ --type md -n

# 同目录常量重复
rg "CONSTANT\s*=" "notes/rewrite/{module}/" --type md -n

# ===== P0 必检清单命令（Gate D）=====

# 1. 文档 §5 测试是否存在
rg "fn {test_name}" {rust_dir} --type rust -n

# 2. trait 是否有 impl
rg "impl.*{TraitName}" {rust_dir} --type rust -n

# 3. 函数是否在声明的文件中
rg "fn {name}" {file}

# 4. 核心算法是否是 stub（含 panic! 检查）
rg "spin_loop\|todo!\|unimplemented!\|unreachable!\|panic!" {rust_dir} --type rust -n

# 5. 文档 §4 签名是否与实际一致（逐函数对比）
```

### 模块路径速查

| 模块 | 源码路径 |
|------|----------|
| VM | `minix3/minix/servers/vm/` |
| PM | `minix3/minix/servers/pm/` |
| VFS | `minix3/minix/servers/vfs/` |
| Kernel | `minix3/minix/kernel/` |
| Drivers | `minix3/minix/drivers/` |
| 公共头文件 | `minix3/minix/include/` |

---

## 五、修复阶段工作流（Fix Phase）

> Review 结束后进入修复阶段时，AI 必须按本流程执行，确保修复不违反 review 规则。

### 1. 修复前准备
1. 重读 STATE.md 中的 Open Issues 列表与对应的 scan.md Issue List。
2. 按问题类型显式加载 Skill：
   - 代码修复（Rust） → `review-code-skill` + `review-patterns-skill`
   - 文档修复（Markdown） → `review-doc-skill` + `review-patterns-skill`
   - 涉及核心语义（IPC/生命周期/错误/权限/地址空间） → `review-core-semantics-skill`
   - 涉及覆盖率/状态追踪 → `review-process-skill` + `review-coverage-skill`
3. 对每个修复项确认：修改范围、验证方法、是否引入新的 P0/P1。

### 2. 修复执行原则
- **先 P0 后 P1/P2**：P0 全部修复并验证前，不标记收敛。
- **文档与代码同步修**：改代码若影响 Ch4 描述，必须同步改文档；改文档若已要求代码实现，必须同步改代码。
- **禁止引入新的违反**：修复过程中仍需满足 no_std、硬件抽象 trait、SMP/BKL、Claims-Evidence 等约束。
- **保留证据**：每个修复项在 scan.md / STATE.md 中记录：修复日期、修改文件、验证命令输出。

### 3. 修复后验证
1. **单元测试**：`cargo test -p <crate>` 必须全部通过。
2. **编译检查**：`cargo check` 无新增 error；新增 warning 需说明理由。
3. **重新跑相关 Gate**：
   - 修了代码语义 → 重新跑 Gate B（Top 5 差异表）抽样验证。
   - 修了代码/测试 → 重新跑 Gate D（P0 必检）和 Gate E（§5 测试存在性）。
   - 修了文档 claim → 重新跑 Gate A/C 相关部分。
4. **更新 STATE.md**：将已修复问题从 Open 列表移入 Closed Issues，注明修复 scan/日期，更新 Convergence Checklist。

### 4. 修复结束标准
- 本次计划修复的所有 P0 已修复并验证。
- 未修复的 P0 必须标记为 `WONTFIX` 并给出不可辩驳的理由（如架构演进明确替代）。
- 最新一次完整 Pass：新增 P0 = 0，新增 P1 ≤ 1。
- 完成 VERIFY-CHECK.md 后才可标记 **CONVERGED**。
