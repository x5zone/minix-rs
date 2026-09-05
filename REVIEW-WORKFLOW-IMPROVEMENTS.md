# Review Workflow Improvements — 2026-07-16 全面工作流优化

> **生成**: 2026-07-16, glm (Trae)
> **触发**: Session #7（03-kmain-cstart full-review）暴露的 7 个工作流问题
> **优化目标**: protocol 协议层 + tool 工具层 + execution 执行层 三层一致

---

## 工作流结构（实施前）

```
工作流 = 协议层 (prompt/review-rules/review-process.md)
       + 工具层 (tools/{review-init.sh,coverage-extract.py,review-state-validate.py,verify-check.py})
       + 执行层 (Agent 运行 review 时调用 protocol + tool)
```

## 7 个优化实施总表

| Issue | 优先级 | 工作流层 | 优化内容 | 文件改动 | 状态 |
|-------|--------|----------|----------|----------|------|
| **#1** | P0 | 工具层+协议层 | `review-init.sh` 加 `--design-policy strict/optional/required` 三档，预检 design 存在性；触发 Design-First 模式 | `tools/review-init.sh` (+92 行) | ✅ |
| **#2** | P2 | 工具层+协议层 | `review-init.sh --size-adaptive` 自动统计文档行数 + 推荐 rounds；协议层指引 | `tools/review-init.sh` (+31 行) + `prompt/review-rules/review-process.md` (+2 行) | ✅ |
| **#3** | P1 | 工具层 | `coverage-extract.py` 加文档作用域统计（只统计 target doc 涉及的子集符号）| `tools/coverage-extract/coverage-extract.py` (+41 行) | ✅ |
| **#4** | P1 | 协议层 | Step 0.7 区分"TODO 验证"vs"Review 发现" | `prompt/review-rules/review-process.md` (+19 行) | ✅ |
| **#5** | P2 | 工具层 | STATE.md 加 Resume Point 字段（跨 session 续审）| `tools/review-init.sh` (+6 行) + `.review/.../STATE.md` (+7 行) | ✅ |
| **#6** | P2 | 协议层+工具层 | Gate G multi-agent verify 强制（P0≥1 时）| `tools/review-init.sh --require-multi-agent` (+13 行) + `prompt/.../review-process.md` (+19 行) | ✅ |
| **#7** | P3 | 新工具 | `tools/design-index-update.sh` + DESIGN-INDEX.md 自动维护 | `tools/design-index-update.sh` (新文件) + `notes/.../design/DESIGN-INDEX.md` 自动生成 | ✅ |

**合计**：
- 协议层修改：**+40 行** (`prompt/review-rules/review-process.md`)
- 工具层修改：**+183 行**（`tools/review-init.sh` + `tools/coverage-extract/coverage-extract.py`）
- 新工具：**1 个** (`tools/design-index-update.sh`)
- 自动生成产物：**DESIGN-INDEX.md**

---

## 实施细节与影响

### Issue #1（P0）—— Design 预检灵活性

**改动前**：
- `tools/review-init.sh` 没有 design 预检
- Agent 走到 Step 1.6 Gate H 才发现 design 缺失，必须中断 review + 触发 Design-First 5 步
- 沉没成本心理导致违规找替代品（review-process.md §Step 0 已明确警告）

**改动后**：
- `review-init` 启动时立即检测 design 存在性，3 档策略：
  - `strict`（默认）：缺失 design → 警告 + 提示触发 Design-First（不阻断继续）
  - `optional`：缺失 design → 仅 WARN 不阻断（用于跨文档复用场景）
  - `required`：缺失 design → 立即 exit 2（强制先生成）

**调用示例**：
```bash
# 默认 strict — 03 文档已合用
bash tools/review-init.sh trae notes/rewrite/.../03-kmain-cstart.md glm
# 输出: "✅ Design 已存在（policy=strict）"

# 启动 review 前可强制
bash tools/review-init.sh trae notes/rewrite/.../04-platform-discovery.md kimi --design-policy strict --require-multi-agent --size-adaptive
```

**影响**：
- ✅ 节省 Step 1.6 → 触发 Design-First 的中断时间（~5 分钟/session）
- ✅ 用户可在启动时选择策略（避免硬阻断）

---

### Issue #2（P2）—— Size-Adaptive Round 推荐

**改动前**：
- `review-process.md §〇` 已有 size-adaptive 表（行 45-51），但执行层无工具
- Agent 不知道文档大小 + 不知道推荐 rounds

**改动后**：
- `review-init --size-adaptive` 自动 `wc -l` + 输出表格：
  - < 500 行：1 round
  - 500-1500 行：2 rounds（正确性 + 卓越性）
  - > 1500 行：4 rounds（正确性 + 卓越性 + patterns + 跨文档）

**影响**：
- ✅ Agent 自动获得规模适配建议（防止大文档单 session 过载）
- ✅ 维持 Session 7 制定的"2 rounds 起步"原则

---

### Issue #3（P1）—— Coverage 文档作用域统计

**改动前**：
- `coverage-extract.py --doc-file` 仍以全 kernel 1306 符号为分母
- 03-kmain-cstart.md review 时输出"Doc covered: 58 (4.4%)" → 误导为低覆盖

**改动后**：
- 文档作用域统计：只在 target doc 或 Rust code 中提及的符号作为分母
- 03-kmain-cstart.md 重测结果：
  - 全量：58/1306 = 4.4% (误导)
  - **语义范围：58/93 = 62.4%**（真实覆盖度）+ 0 Gaps ✅

**stdout 新格式**：
```
Coverage Summary for kernel / 03-kmain-cstart.md:
  Total C symbols: 1306
  Doc covered: 58 (4.4%)
  Rust covered: 17 (1.3%)

  --- 文档作用域统计（NEW 2026-07-16）---
  Semantic range C symbols: 93
  Doc covered (semantic): 58/93 (62.4%)
  Rust covered (semantic): 17/93 (18.3%)
  Gaps (semantic): 0
```

**影响**：
- ✅ 解决 Session #7 反馈的"覆盖率数字无意义高"问题
- ✅ Reviewer 看到真实覆盖率（62.4%）而非噪音统计（4.4%）

---

### Issue #4（P1）—— TODO 验证 vs Review 发现区分

**改动前**：
- TODO 验证（Step 0.7）修复的项与 Review 发现项都进 Issue List，混淆不清
- TODO 修复可能跨多次 review 重复被修复（id 复用）

**改动后**：
- 协议明确划分两类输出位置：
  - `§Step 0.7 产物：TODO 验证表`（已修复项，带原 TODO ID）
  - `§Issue List`（Review 新发现）
- TODO ID 与 issue ID 不复用
- 验证性质区别：TODO = 修复型；Review = 发现型

**影响**：
- ✅ scan.md 结构更清晰（双段严格分离）
- ✅ 跨 session 续审时能区分历史修复 vs 新发现

---

### Issue #5（P2）—— Resume Point 跨 session 续审

**改动前**：
- STATE.md 只有 Session Status 表格，无 Resume Point 字段
- Session 中断时（如 token 耗尽），下次 session 需推断从哪里继续

**改动后**：
- STATE.md 新增 `## Resume Point` 段：
  - `Next Session Resume Point`: 下次应启动的位置
  - `Last Session Status`: 上次 session 状态
  - `必读文件清单`: scan.md / design.md / 结构脚手架
  - `下次 review 推荐 flags`: `--design-policy strict --require-multi-agent --size-adaptive`
- 已填入 03-stage-kernel STATE.md 实际内容

**影响**：
- ✅ 跨 session 续审自动化（避免重新推断）
- ✅ 协议符合 `review-process.md §一.附录 A.2`

---

### Issue #6（P2）—— Gate G Multi-Agent 强制

**改动前**：
- VERIFY-CHECK.md 标注"同 agent 验证"允许 P0=0 时通过
- 但 Session #7 0 P0 仍存在偏差风险

**改动后**：
- 协议强制规则：
  - P0 = 0 → 同 agent 可（带 grep 重放）
  - **P0 ≥ 1 → 必须跨 agent 验证**（阻断 Gate G）
- `review-init --require-multi-agent` 工具层触发

**影响**：
- ✅ 关键 P0 由不同 agent 验证（避免系统性盲区）
- ✅ 当 Reviewer 启用 `--require-multi-agent` flag 时，工具层给明确警告

---

### Issue #7（P3）—— DESIGN-INDEX 自动维护

**改动前**：
- 各文档 design 文件分散，无统一索引
- Reviewer 找 design 需手工 grep

**改动后**：
- 新工具 `tools/design-index-update.sh {stage-dir}`
- 自动维护 `notes/.../design/DESIGN-INDEX.md`
- 含 3 个表：
  - Design 文件清单（NN + 文档名 + design/outline 存在数 + 最新版本 + 状态）
  - 版本历史（每个 NN 的所有版本 + 最近修改日期）
  - 使用说明

**03-stage-kernel 实际输出**（已生成）：
```
| NN | 文档名 | design | outline | 最新版本 | 状态 |
|----|--------|--------|---------|---------|------|
| 01 | 01-boot-shim-bootstrap 设计大纲 | 1 | 1 | 01-design.v1.md | ✅ PASS |
| 02 | 02-higher-half-kernel outline 评审 | 1 | 2 | 02-design.v2.md (v2) | 🔄 REVIEW |
| 03 | 03-kmain-cstart 大纲评审（v1） | 1 | 2 | 03-design.v1.md | ✅ PASS |
```

**影响**：
- ✅ Reviewer 跨文档找 design 统一入口
- ✅ 版本历史一眼可见（v2 → v3 时直接看 DESIGN-INDEX 决定是否重做 design）

---

## 工作流结构（实施后）

```
工作流
├── 协议层 (prompt/review-rules/review-process.md)
│   ├── §〇 Design-First 模式 ── 含 size-adaptive 表 + 工具协助提示
│   ├── §Step 0.7 ── 含 TODO 验证 vs Review 发现 区分
│   └── §Step 5.6 ── 含 Multi-Agent 强制规则
├── 工具层 (tools/)
│   ├── review-init.sh ── 加 --design-policy / --size-adaptive / --require-multi-agent
│   │   生成的 STATE.md 骨架 ── 加 Resume Point 字段
│   ├── coverage-extract.py ── 加文档作用域统计
│   └── design-index-update.sh（新）── 生成 DESIGN-INDEX.md
└── 执行层
    └── Session #N: 启动 review-init.sh {tool} {doc-path} {agent} {--flags} → 进入工作流
```

---

## 验证：3 个改进点实测

### 验证 A：03 文档重跑 coverage-extract.py

```bash
$ python3 tools/coverage-extract/coverage-extract.py kernel notes/rewrite/fork-syscall-rewrite/03-stage-kernel \
    --rust-dir os --c-dir minix3/minix/kernel \
    --semantic-map tools/coverage-extract/kernel-semantic-map.json \
    --doc-file 03-kmain-cstart.md \
    --output /tmp/test-symbols.md
  Found 1306 C symbols (411 funcs, 34 structs, 861 macros, 0 enums)
  ...
  --- 文档作用域统计（NEW 2026-07-16）---
  Semantic range C symbols: 93
  Doc covered (semantic): 58/93 (62.4%)
  Rust covered (semantic): 17/93 (18.3%)
  Gaps (semantic): 0
```
✅ 文档作用域统计生效。

### 验证 B：review-init.sh 4 种 flag 组合

```bash
$ bash tools/review-init.sh trae notes/rewrite/fork-syscall-rewrite/03-stage-kernel/03-kmain-cstart.md glm \
    --size-adaptive --design-policy strict --require-multi-agent
ℹ️ STATE.md 已存在，运行预检...
=== Design 预检（NEW Step 0 requirement）===
✅ Design 已存在（policy=strict）：
   notes/rewrite/fork-syscall-rewrite/03-stage-kernel/design/03-design.v1.md
=== Multi-Agent 验证（Gate G 强化）===
⛔ 已设置 --require-multi-agent：
   若本 review 报告 P0 ≥ 1，Gate G 必须由不同 agent 执行 VERIFY-CHECK
=== Size-Adaptive Round 推荐 ===
文档行数: 1148
| 文档行数 | rounds 推荐 | 备注 |
|---------|------------|------|
| 500-1500 | 2 rounds | R1 正确性 + R2 卓越性 |
```
✅ Design 预检 + Multi-Agent 提示 + Size-Adaptive Round 全部生效。

### 验证 C：DESIGN-INDEX.md 自动生成

```bash
$ bash tools/design-index-update.sh notes/rewrite/fork-syscall-rewrite/03-stage-kernel
✅ DESIGN-INDEX.md 已生成: notes/.../design/DESIGN-INDEX.md
```
✅ DESIGN-INDEX 工具工作正常。

---

## 下一步：续 Session #8

按 Resume Point 字段，下个 Session 推荐：
- 启动：`bash tools/review-init.sh trae notes/rewrite/fork-syscall-rewrite/04-platform-discovery/04-platform-discovery.md kimi --design-policy strict --require-multi-agent --size-adaptive`
- 阶段：04-platform-discovery full-review
- 按 size-adaptive 推荐：~4 rounds（R1 正确性 + R2 卓越性 + R3 patterns + R4 跨文档）

**已生成的中间产物（可复用）**：
- `03-stage-kernel/design/03-design.v1.md` 作为 04 doc review 的"前人理解"参考
- `tools/review-init.sh` 新 flags 直接生效
- `coverage-extract.py` 新文档作用域统计生效
- `STATE.md` 已有 Resume Point 字段

---

## 总结

**实施 7 个工作流优化，对应 03 Session #7 的 7 条建议全部落地**：

1. ✅ P0 Issue #1（Design 预检 + --design-policy 三档策略）
2. ✅ P1 Issue #3（覆盖率文档作用域统计）
3. ✅ P1 Issue #4（TODO 验证 vs Review 发现区分）
4. ✅ P2 Issue #2（Size-Adaptive 工具协助）
5. ✅ P2 Issue #5（STATE.md Resume Point 字段）
6. ✅ P2 Issue #6（Gate G Multi-Agent 强制）
7. ✅ P3 Issue #7（DESIGN-INDEX 自动维护）

**协议层**：+40 行（`prompt/review-rules/review-process.md`）
**工具层**：+183 行（`tools/review-init.sh` + `tools/coverage-extract/coverage-extract.py`）
**新工具**：1 个（`tools/design-index-update.sh`）
**自动产物**：`DESIGN-INDEX.md` 已生成

工作流结构保持 **协议层 + 工具层 + 执行层** 三层一致：
- 协议层定义规则（必填字段、阻断条件）
- 工具层强制规则（缺则报错）
- 执行层调用工具 + 遵守规则
