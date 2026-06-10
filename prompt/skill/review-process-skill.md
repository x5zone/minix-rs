---
name: "review-process-skill"
description: "Minix-RS Review 执行流程。定义强制步骤 Step 0-7、每个 Step 的中间产物格式、自检清单、以及工具命令速查。当 Agent 进入 Review 执行阶段时调用此 Skill。"
---

# Minix-RS Review 执行流程

> 每个 Step 必须产生**可见的中间产物**（表格、列表、grep 输出）。不允许"在脑子里过一遍"然后跳到最终输出。

---

## Step 0: 范围声明 + 时间预算 + 状态恢复

- 声明 Review 模式和范围
- 声明时间预算（| <200行→10-20分 | 200-500→20-40 | 500-1000→40-80 | >1000→80-120）
- **读取状态**：检查 `.review/{module}/STATE.md` 是否存在

**中间产物**：
```
- **Review 模式**：[文档/代码/完整/局部]
- **目标文件**：xxx.md / xxx.rs
- **Step 2.5/Step 4 是否适用**：[适用/不适用（原因）]
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

## Step 2: Diff Extraction（差异提取）

- 列出 **3 个**最背离 Minix3 原始语义的地方
- 说明：Minix3 实际行为、文档/代码中的描述、差异性质

**中间产物**：
```markdown
### Step 2 产物：Top 3 差异

| # | 差异点 | Minix3 行为 | 文档/代码描述 | 差异性质 |
|---|--------|------------|-------------|---------|
| 1 | xxx | pagetable.c:333 实际 | 文档 L245 描述 | 概念错误 |
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

## Step 3.5: Precision Check（细节精确检查）

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

**中间产物**：可疑点标记表格（元规则/位置/内容/人工确认建议）

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

## Step 5: Final Review Output（最终输出）

- 按输出模板整理发现
- 每个问题标优先级（P0/P1/P2）+ 明确修改方向
- 必须含：维度覆盖自检、最弱项自检、时间预算评估

### Step 5.5: 状态写入与收敛判断

> 完成当前 phase 的验证后，将结果持久化写入 `.review/{module}/` 目录。

**中间产物**：
1. 更新或创建 `.review/{module}/STATE.md`（按 [review-agent.md §STATE.md 格式](review-agent.md)）
2. 更新或创建对应维度的检查文件（如 `.review/{module}/CONCEPT-CHECK.md`）
3. 更新 `.review/{module}/FINDINGS.md` 追加新发现的问题（不覆盖已有已修复项）
4. 更新 STATE.md 的 Phase Completion Log 和 Convergence Checklist

**收敛判断**：
```markdown
### 收敛状态评估

- [ ] 全维度覆盖：10/10 维度 COMPLETE
- [ ] P0 收敛：最近 Pass 新增 P0 = 0
- [ ] P1 收敛：最近 Pass 新增 P1 ≤ 1
- [ ] 独立验证：VERIFY-CHECK.md = PASS（或尚未执行）

**当前状态**：CONVERGED / NOT_CONVERGED (N phases remaining)
**下一步**：[下一阶段名称] 或 [执行独立验证] 或 [审查已收敛，可结束]
```

### Step 5.6: Review Verification Protocol（独立验证）

> **目的**：解决"自己审自己"的盲区。在所有维度 COMPLETE 后，在新会话中独立验证审查质量。
> 用户指令：「验证 review」或「review of review」

**执行步骤**（独立会话）：
1. 读取 `.review/{module}/STATE.md` + `FINDINGS.md` + 原始文档/代码
2. **随机抽样**：从 FINDINGS.md 中随机选取 20% 的已报告问题
3. **反向验证**：对每个抽样问题——source evidence 是否充分？判定等级是否合理？
4. **遗漏检查**：抽样 20% 的源码符号，验证是否都在文档/检查覆盖
5. **收敛验证**：检查 STATE.md 的 Convergence Checklist 是否有已标记 COMPLETE 但实际未完成的维度

**中间产物**（写入 `.review/{module}/VERIFY-CHECK.md`）：
```markdown
### Review Verification Result

**抽样一致性**：X/Y = Z%
**遗漏检查**：N 符号抽样，M 遗漏
**收敛验证**：K/10 维度可信

| 抽样问题 | FINDINGS 判定 | 独立重新判定 | 一致? |
|---------|-------------|------------|-------|

**判定**：PASS / CONCERN / FAIL
```

**判定标准**：
- **PASS**：一致性 ≥ 90%，无遗漏 key symbols，收敛状态可信 → 审查完成
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
- [ ] Step 2 Top 3 差异已输出
- [ ] Step 2.5 链路验证表格已输出（如适用）
- [ ] Step 3 概念准确性表格已输出
- [ ] Step 3 C 代码引用验证表格已输出
- [ ] Step 3 数据结构覆盖表格已输出
- [ ] Step 3 C 源码覆盖完整性表格已输出（含覆盖率）
- [ ] Step 3 文档风格验证表格已输出（§2.11）
- [ ] Step 4 跨文档检查已输出
- [ ] Step 4.1 语义归属判定已输出（如适用）
- [ ] Step 4.2 设计决策质量表格已输出（如适用）
- [ ] Step 5 维度覆盖自检表格已输出
- [ ] Step 5 最弱项自检 4 问题已确认
- [ ] Step 5 时间预算评估已输出
- [ ] Step 5.5 STATE.md 和维度检查文件已写入
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
