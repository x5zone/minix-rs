# Review 执行流程

> 本文档定义 AI 执行 Review 的强制步骤和工具命令。
> 输出格式见 [review.md §AI Review 输出模板](review.md#ai-review-输出模板)，口诀见 [review.md §快速判断口诀](review.md#快速判断口诀)。

---

## 〇、执行模式选择（构造 / 快速 / 深度）

> **目的**：根据任务规模和精度要求，选择合适的执行模式。不同模式裁剪不同的 Step，平衡覆盖度与效率。

| 模式 | 适用场景 | 执行 Step | 预计耗时 | 对应 Profile |
|------|---------|----------|---------|-------------|
| **构造模式（Constructive）** | 文档/代码初稿阶段，需要引导作者补全 | Step 0, 1, 1.5, 2, 5, 6 | 15~30 分钟 | A / B / G |
| **快速模式（Quick）** | 日常 PR 审阅、时间有限的扫描 | Step 0, 1, 2, 5 | 10~20 分钟 | D |
| **深度模式（Deep）** | 里程碑验收、关键模块完整 Review | Step 0-7（全量） | 40~120 分钟 | C / H→I→J→K / O / P |

### 构造模式（Constructive）

> **定位**：初稿阶段的"脚手架 Review"。不追求发现所有问题，而是帮作者建立完整骨架。
> **特点**：以覆盖率穷举和差异提取为核心，输出"缺什么"而非"哪里错"。

**执行 Step**：
1. **Step 0**：范围声明 + 时间预算
2. **Step 1**：源码定位（验证引用文件存在）
3. **Step 1.5**：覆盖率穷举（生成 SYMBOLS.md，标记缺口）
4. **Step 2**：差异提取（Top 3 语义偏差）
5. **Step 5**：输出（聚焦缺口清单 + 补全建议）
6. **Step 6**：修改项（P0 缺口必须生成补全项）

**跳过**：Step 2.5（链路验证，初稿可能链路未建）、Step 3（逐行精确验证）、Step 3.5（细节精确）、Step 4（跨文档，初稿可能未引用）

**输出重点**：
- 覆盖率缺口清单（哪些 C 函数/结构体/宏未覆盖）
- Top 3 语义偏差（方向性错误）
- 补全建议（下一步应该写什么）

### 快速模式（Quick）

> **定位**：日常 PR 审阅的"烟雾测试"。快速发现明显 P0，不做穷举。
> **特点**：用口诀扫描 + 差异提取，仅输出 P0。

**执行 Step**：
1. **Step 0**：范围声明 + 时间预算
2. **Step 1**：源码定位（仅验证关键引用）
3. **Step 2**：差异提取（Top 3）
4. **Step 5**：输出（仅 P0 问题）

**跳过**：Step 1.5（覆盖率穷举）、Step 2.5（链路验证）、Step 3（逐行验证）、Step 3.5（细节精确）、Step 4（跨文档）、Step 6（修改项，P0 在输出中直接说明）

**输出重点**：
- Top 3 语义偏差
- P0 问题清单（仅 P0，P1/P2 不输出）
- "建议升级到深度模式"的提示（如发现 P0 数量 > 3）

### 深度模式（Deep）

> **定位**：里程碑验收的"全量 Review"。执行所有 Step，穷举所有维度。
> **特点**：每个 Step 产生完整中间产物，覆盖正确性 + 卓越性 + 覆盖率。

**执行 Step**：Step 0-7 全量执行（见下方"一、AI 执行 Review 的强制步骤"）

**输出重点**：
- 全部中间产物（10 个维度的表格）
- P0/P1/P2 完整问题清单
- 修改项（P0 必须有代码修改项）
- 自检清单确认

**模式选择决策树**：
```
任务来了
  ├─ 是初稿？ → 构造模式
  ├─ 是日常 PR / 时间紧？ → 快速模式
  │    └─ 发现 P0 > 3？ → 建议升级深度模式
  └─ 是里程碑 / 关键模块？ → 深度模式
       ├─ 文档 < 300 行 → Profile C（单次深度）
       ├─ 文档 > 300 行 → Profile H→I→J→K（分阶段深度）
       ├─ 正确性已过，追求卓越 → Profile O（深度+卓越性）
       └─ 覆盖率验收 → Profile P（深度+覆盖率穷举）
```

---

## 一、AI 执行 Review 的强制步骤

> 为防止 AI 在 Review 过程中"浮躁"或遗漏关键验证，必须按以下步骤执行。
> **关键原则**：每个 Step 必须产生**可见的中间产物**（表格、列表、grep 输出）。不允许"在脑子里过一遍"然后跳到最终输出。

### Step 0: 范围声明 + 时间预算 + 状态恢复

- 按 [§〇 执行模式选择](#〇执行模式选择构造--快速--深度) 确定执行模式（构造/快速/深度）
- 按 [review.md §Review 启动：范围声明](review.md) 声明 Review 模式和范围
- 声明时间预算（可选；按 [review.md §时间预算参考](review.md#时间预算参考)）
- **读取状态（双路径）**：
  - **Trae IDE** → 读取 `notes/rewrite/{module}/.review/STATE.md`
  - **Claude Code Runtime** → 读取 `.review/{module}/STATE.md`（项目根）
  - 若两个 STATE.md 都存在且内容矛盾，**不要自动合并**，在 scan.md 中记录分歧并询问用户哪个为准。
- **中间产物**：执行模式 + 范围声明 + 时间预算声明（或省略说明） + 状态恢复摘要

### Step 1: Ground Truth Lookup（源码定位）

- 识别文档中提到的所有 Minix3 源文件
- 使用 `rg` 命令验证这些文件是否存在于 `minix3/` 目录
- 记录每个引用的**文件路径**和**行号范围**

> **效率提示**：如果文档较长（>500 行），优先用 grep 提取所有 `.c` / `.h` 文件引用（`rg "\.c" doc.md`），然后抽样验证引用行号的准确性，而非逐行全量验证。Step 3 再补做行号精确验证。

**中间产物**（必须输出）：
```markdown
### Step 1 产物：源码文件清单

| 文件路径 | 文档引用位置 | 文件存在? | 引用行号范围 |
|---------|------------|----------|------------|
| minix3/minix/servers/vm/pb.c | Ch2§2.3 | ✅ | 33-168 |
| minix3/minix/servers/vm/region.h | Ch2§2.2 | ✅ | 23-35 |
| ... | ... | ... | ... |
```

### Step 1.5: Coverage Enumeration（覆盖率穷举，机器+AI）

> **目的**：机器生成穷举清单，AI 只负责语义判断。解决 AI "凭印象扫描"导致覆盖率不足的问题。
> **基础**：`tools/coverage-extract/coverage-extract.py` 确定性脚本生成 SYMBOLS.md 骨架。
> **详见**：[review-coverage-skill.md](../skill/review-coverage-skill.md)

**执行步骤**：

1. **机器生成 SYMBOLS.md 骨架**：
   ```bash
   # 模块级 — 服务器模块（vm / pm / vfs / rs / ds / inet ...）
   python3 tools/coverage-extract/coverage-extract.py {module} {doc_dir} \
     --rust-dir os --c-dir minix3/minix/servers/{module} \
     --output .review/{module}/SYMBOLS.md

   # 模块级 — 内核
   python3 tools/coverage-extract/coverage-extract.py kernel {doc_dir} \
     --rust-dir os --c-dir minix3/minix/kernel \
     --output .review/kernel/SYMBOLS.md

   # 单文档级（推荐 doc-specific review）— 服务器模块
   python3 tools/coverage-extract/coverage-extract.py {module} {doc_dir} \
     --rust-dir os --c-dir minix3/minix/servers/{module} \
     --doc-file {target-doc}.md \
     --semantic-map tools/coverage-extract/{module}-semantic-map.json \
     --output .review/{module}/{target-doc}/SYMBOLS.md

   # 单文档级 — 内核
   python3 tools/coverage-extract/coverage-extract.py kernel {doc_dir} \
     --rust-dir os --c-dir minix3/minix/kernel \
     --doc-file {target-doc}.md \
     --semantic-map tools/coverage-extract/kernel-semantic-map.json \
     --output .review/kernel/{target-doc}/SYMBOLS.md
   ```
   脚本自动提取 C 函数/结构体/宏/枚举 + Rust pub 项 + 文档覆盖检查 + 名称匹配。
   - `--rust-dir os`：扫描整个 `os/` 目录，避免 `kmain`/`ProtectionArch` 等跨 crate 符号遗漏。
   - `--c-dir`：服务器模块用 `minix3/minix/servers/{module}`，内核用 `minix3/minix/kernel`。
   - `--semantic-map`：C→Rust 改写必须提供语义映射表，否则 Rust 覆盖率会显示为 0%。
   - `--doc-file`：限定到单篇文档，避免两篇 doc 的 coverage 数字完全相同。
   - 若 Rust 覆盖率为 0%，必须先检查 `--rust-dir`/`--semantic-map` 是否正确，或确实缺失实现。

2. **AI 补充语义判断**（5 项，每项标注 evidence [DIRECT/MEDIUM/INFERRED]）：
   - Rust 对应关系确认（名称匹配 ≠ 语义对应）
   - 架构演进标记（ARCH: 不需要 + 理由）
   - 语义归属判定（以功能语义为准）
   - 行为契约表（核心函数：输入/输出/副作用/错误码/时序）
   - 测试覆盖补充（L1对偶/L2契约/L3 doctest）

3. **更新 STATE.md Coverage Status 段**

**中间产物**（必须输出）：
```markdown
### Step 1.5 产物：覆盖率穷举

**SYMBOLS.md**: .review/{module}/SYMBOLS.md

| 指标 | 数值 |
|------|------|
| C 符号总数 | N |
| 文档覆盖 | M (M%) |
| Rust 覆盖 | K (K%) |
| 完全缺口 | G |
| 架构演进 | A |

**P0 缺口**（在语义范围内但无文档无Rust）:
| 符号 | C 源码 | 判定 | evidence |
|------|--------|------|----------|
| `func_name` | file.c:N | P0 缺口 | DIRECT: rg 无结果 |

**ARCH 标记**:
| 符号 | 理由 | evidence |
|------|------|----------|
| `map_service` | IPC 协议演进 | INFERRED |
```

> **反幻觉**：每个覆盖判定必须先执行 grep 验证，再下结论。禁止凭印象判断"已覆盖"。

### Step 2: Diff Extraction（差异提取）

- 列出 **3 个**你认为文档/代码中最背离 Minix3 原始语义的地方
- 对每个差异，说明：
  - Minix3 源码的实际行为
  - 文档/代码中的描述/实现
  - 差异的性质（概念错误、语义偏移、过度简化等）

**中间产物**（必须输出）：
```markdown
### Step 2 产物：Top 3 差异

| # | 差异点 | Minix3 行为 | 文档/代码描述 | 差异性质 |
|---|--------|------------|-------------|---------|
| 1 | xxx | pagetable.c:333 实际行为 | 文档 L245 描述 | 概念错误 |
| 2 | ... | ... | ... | ... |
| 3 | ... | ... | ... | ... |
```

### Step 2.5: Link Validation（链路验证）

> **目的**：验证文档各章节之间的推导链路是否完整。链路断裂 = 设计或实现有问题。
> 详细的链路验证规则见 [review-doc-checklist.md §2.10](review-doc-checklist.md#210-章节链路验证)。

**链路验证**：
1. Ch3→Ch1&2：每个设计决策是否有依据？
2. Ch4→Ch3：每个实现是否对应设计？
3. 测试→Ch3+Ch4：测试是否覆盖设计决策和实现细节？
4. 代码→Ch4：代码是否与文档描述一致？

**中间产物**：按 [review-doc-checklist.md §2.10](review-doc-checklist.md#210-章节链路验证) 要求的 4 个表格输出。

> **注意**：局部 Review（仅 Ch1&2）时跳过此步骤。完整 Review 时在执行 Step 4 后，结合跨文档信息做最终链路验证。

### Step 3: Sanity Check（一致性检查）

- 验证文档第 2 章引用的**所有行号**是否真实存在于本地文件
- 验证文档中的**所有数值常量**是否与 Minix3 源码一致
- 验证文档中的**所有函数签名**是否与 Minix3 源码一致

**中间产物**：按 [review-doc-checklist.md §2.1](review-doc-checklist.md#21-概念准确性检查强制不可跳过) 和 [§2.2](review-doc-checklist.md#22-c-代码引用验证强制不可跳过) 要求的表格输出。

### Step 3.5: Precision Check（细节精确检查）

> **目的**：在 Sanity Check（大方向正确）之后，对代码和注释的微观精确性进行扫描。本步骤的 5 个元规则跨阶段复用，适用于 Boot/VM/PM/VFS 等所有模块。
> **策略**：Agent 负责"标记可疑点"，不要求 100% 自动判定正确性，输出待人工确认的标记列表。

**3.5.1 外部知识标记**
- 扫描代码注释中引用的外部知识（寄存器名称、标志位、协议条款、规范版本）
- 输出标记列表："以下注释引用了外部知识，建议人工验证准确性"
- **检查口令**："注释中提到的这个硬件行为/协议要求，来源是什么？在当前上下文中仍然成立吗？"

**3.5.2 通用接口纯度扫描**
- 扫描共享结构体、trait、公共 API 的字段/方法
- 对每个元素提问："此字段/方法对所有消费者上下文都有语义意义吗？"
- 标记仅在特定上下文（架构、模块、进程类型）中有意义的元素
- **检查口令**："如果消费者是 aarch64/riscv64/其他模块，这个字段还有意义吗？"

**3.5.3 返回值完整性扫描**
- 扫描所有外部调用（固件、系统调用、库函数、硬件抽象 trait 方法）
- 检查返回值是否被：① 使用 ② 传递 ③ 显式注释说明可安全丢弃
- 标记被忽略但未说明理由的返回值
- **检查口令**："这个外部调用的返回值被忽略了吗？忽略是安全的吗？"

**3.5.4 资源生命周期闭环扫描**
- 扫描资源获取点（分配、映射、打开、租借）
- 检查是否有对应的释放点，或显式声明"不释放"的理由
- **检查口令**："这个资源谁负责释放？什么时候释放？如果'不释放'，理由是什么？"

**3.5.5 理由可质疑性扫描**
- 扫描所有包含"为什么"解释的注释（如"因为/由于/避免/为了/需要"）
- 输出"以下理由需人工质疑"标记列表
- **检查口令**："这个理由在当前上下文中成立吗？有没有更诚实的说法？"

**中间产物**：
```markdown
### Step 3.5 产物：细节精确性可疑点标记

| 元规则 | 位置 | 可疑内容 | 人工确认建议 |
|--------|------|---------|------------|
| 外部知识 | L942 | "CR4.PSE 支持 2MB 大页" | 验证 x86-64 长模式下 PSE 与 2MB 页的关系 |
| 通用接口 | L876 | `syscall_entry: VirBytes` | 确认是否对所有架构有意义 |
| 返回值 | Lxxx | `exit_boot_services()` 返回 `_mmap` | 确认丢弃最终内存映射是否安全 |
| 资源闭环 | Lxxx | `Box::leak(memmap)` | 确认泄漏后的回收路径 |
| 理由质疑 | L872 | "u64 避免 32 位截断" | 微内核是否真的需要担心 4GB 截断 |
```

### Step 4: Cross-Document Check（跨文档联动）

> **范围限定**：优先检查本文档所在目录下的其他文档，其次检查"参见"章节引用的外部文档。

- 检查本文档涉及的概念/类型/函数是否在同目录其他文档中已有定义
- 如果同目录其他文档已有完整设计和实现，检查本文档是否：
  - 正确引用了那些文档？
  - 与那些文档的描述一致？
  - 没有重复定义或矛盾？
- 检查本文档"参见"章节引用的外部文档：
  - 引用的文档是否存在？
  - 引用的内容是否与外部文档一致？
- 特别关注：
  - 共享的数据结构（如 `vmproc`、`vir_region`）
  - 共享的常量/配置（如 `CLICK_SIZE`、`VM_RQ_BASE`）
  - 跨模块调用（如 PM → VM 的 IPC 接口）
- **重复处理检查**：如果同目录其他文档已经完整处理了某个概念（分析+设计+实现），本文档是否又重复处理了一遍？如果是，应精简为引用而非重述

#### 4.1 语义归属判定（grep 辅助）

> **目的**：当同一个 C 函数/结构体/宏可能属于多个文档的语义范围时，通过 grep 搜索 + 语义分析确定其归属文档，避免遗漏或重复。

**执行步骤**：

1. **提取关键符号**：从当前文档的 Ch1&2 中提取所有涉及的 C 符号（函数名、结构体名、宏名）
2. **grep 跨文档搜索**：在同目录下所有 `.md` 文件中搜索这些符号
   ```bash
   rg "SYMBOL_NAME" notes/rewrite/{module}/ --type md -n
   ```
3. **语义归属判定**：对每个符号，根据其**语义本质**判断应归属哪个文档：
   - 如果符号的语义与当前文档主题直接相关 → 当前文档应完整覆盖（分析 + 设计 + 实现）
   - 如果符号的语义属于同目录其他文档的主题 → 应在那个文档中完整覆盖，当前文档仅保留引用
   - 如果符号在任何文档中都未被覆盖 → 判定归属后，在对应文档中补充

**判定原则**：
- 以**功能语义**为准，而非以"哪个文件定义了它"为准
- 例如：`vm_mappages` 定义在 `mmap.c`，但语义上属于"页表操作"，应归入 `07-pagetable-ops.md`
- 例如：`pt_t` 定义在 `proto.h`，但语义上属于"页表结构"，应归入 `06-pagetable-struct.md`

**输出格式**：
```
| 符号 | 类型 | 语义归属 | 当前覆盖状态 | 处理建议 |
|------|------|---------|-------------|---------|
| vm_mappages | 函数 | 07-pagetable-ops.md | 未覆盖 | 在 07 中补充分析 |
| pt_t | 结构体 | 06-pagetable-struct.md | 已覆盖 | 无需处理 |
| ARCH_VM_PTE_PRESENT | 宏 | 06-pagetable-struct.md | 07 中重复 | 07 精简为引用 |
```

#### 4.2 Design Quality Check（设计质量检查）

> **目的**：在跨文档联动确认后，对 Ch3 设计决策做质量评估。这步依赖 Step 2.5 的链路验证结果。
> 详细规则见 [review-doc-checklist.md §2.9](review-doc-checklist.md#29-设计决策质量检查ch3-专项)。

1. 列出 Ch3 的所有设计决策
2. 对每个设计决策，检查：
   - 是否有 Ch1&2 的依据？（可追溯性）
   - 是否考虑了替代方案？（合理性）
   - 是否覆盖了 Ch2 的所有场景？（完整性）
   - 是否在 `no_std` 下可行？（可实现性）
3. 如果发现更好的替代方案，要求在 Ch3 添加 TODO 段落

> **注意**：局部 Review（仅 Ch1&2）时跳过此步骤。

#### 4.2.1 Rust 代码设计质量检查

> **目的**：在文档设计质量检查之外，对 Rust 代码中的 trait/类型设计做独立评估。
> 详细规则见 [review-code-checklist.md §2.5](review-code-checklist.md#25-trait-设计质量评估)。

1. 列出代码中所有自定义 trait
2. 对每个 trait，检查：
   - 是否有 ≥2 个行为不同的实现？（多态必要性）
   - 是否被用作 trait bound？（多态使用）
   - 方法在所有架构上的实现是否真的不同？（跨架构差异）
   - 是否混淆了机制和策略？（职责分离）
3. 如果发现不必要的 trait，标记为 P1，建议简化

> **注意**：此检查在完整 Review（模式 C）时执行，局部 Review 时跳过。

### Step 5: Final Review Output（最终输出）

- 按 [review.md §AI Review 输出模板](review.md#ai-review-输出模板) 整理所有发现
- 每个问题必须标注优先级（P0/P1/P2）
- 每个问题必须提供明确的修改方向
- **必须包含**：维度覆盖自检表格、最弱项自检、时间预算评估

### Step 5.5: 状态写入与收敛判断

> **目的**：将当前 phase 的验证结果持久化写入工具对应的路径，并判断是否收敛。

**执行步骤**：
1. 创建或更新工具对应的 `STATE.md`：
   - Trae IDE → `notes/rewrite/{module}/.review/STATE.md`
   - Claude Code Runtime → `.review/{module}/STATE.md`（项目根）
2. 创建或更新 `SYMBOLS.md`（Step 1.5 产物）到对应路径
3. **所有维度结果写入 scan.md 单文件**（NOT 10 个维度检查文件）。若用户显式指定输出位置，双写到用户指定路径 + 工具默认路径。
4. 将 scan.md 中**新发现 P0/P1/P2** 同步到 STATE.md 的 Open P0/P1/P2 列表；已修复问题移入 Closed Issues 段落。
5. 更新 STATE.md 的 Phase Completion Log 和 Convergence Checklist
6. 输出收敛状态评估

**收敛终止条件**（全部满足才算审查完成）：
1. scan.md 中所有维度章节标记 COMPLETE
2. 最近一次完整 Pass 中，P0 新增数量 = 0
3. 最近一次完整 Pass 中，P1 新增数量 ≤ 1
4. **独立验证（VERIFY-CHECK.md）结果为 PASS**（**必须完成，不能跳过**）
5. scan.md / STATE.md 中所有 P0 已被修复并验证通过
6. SYMBOLS.md 覆盖率穷举完成
7. **Blocker Gates A-E 全部通过且有证据附件**

### Step 5.6: Review Verification Protocol（独立验证，强制）

> **目的**：解决"自己审自己"的盲区。在所有维度 COMPLETE 后，必须执行独立验证才能标记 CONVERGED。

**触发条件**：所有维度标记 COMPLETE + P0/P1 收敛后，**必须**执行本步骤并生成 VERIFY-CHECK.md。未执行 VERIFY-CHECK 时，状态必须为 NOT_CONVERGED。

**执行方式**（独立会话中执行）：
1. Agent 读取 STATE.md + scan.md + 原始文档/代码
2. **随机抽样**：从 scan.md Issue List 中随机选取 20% 的已报告问题
3. **反向验证**：对每个抽样问题，独立重新验证——source evidence 是否充分？判定等级是否合理？
4. **遗漏检查**：抽样 20% 的源码符号（函数/结构体/宏），验证是否都在文档/检查中覆盖了
5. **收敛验证**：检查 STATE.md 的 Convergence Checklist 是否有"标记 COMPLETE 但实际未完成"的维度
6. **Blocker Gates 复验**：检查 scan.md 中 Gate A-E 是否都附带真实证据
7. **输出判定**：
   - **PASS**：抽样验证一致性 ≥ 90%，无遗漏 key symbols，收敛状态可信，Gates 真实通过
   - **CONCERN**：抽样验证一致性 70-90% → 特定维度需重新审查
   - **FAIL**：抽样验证一致性 < 70% 或发现关键遗漏 → 整体重新审查

**输出**：写入工具对应的 VERIFY-CHECK.md 路径（Trae: `notes/rewrite/{module}/.review/VERIFY-CHECK.md`; Claude: `.review/{module}/VERIFY-CHECK.md`）。

### Step 6: Action Item Generation（修改项生成）

> **目的**：将 review 发现转化为可执行的修改项，确保代码会被实际修改。这是解决"review 完了代码不改"问题的关键步骤。

对每个 P0/P1 问题，必须生成：
1. **问题描述**：什么问题
2. **修改方向**：应该怎么改
3. **影响范围**：涉及哪些文件（文档 + 代码）
4. **验证方法**：修改后如何验证

**格式**：
```
### TODO #N: [简述]
- **优先级**: P0/P1
- **类型**: 设计缺陷 / 代码-设计不一致 / no_std 违规 / 语义偏移 / ...
- **文件**: `path/to/file.rs`
- **问题**: [详细描述]
- **修改方案**: [具体方案]
- **验证**: [如何验证修改正确]
```

**关键**：P0 问题必须生成代码修改项，不能只停留在"建议修改"。
P1 问题如果涉及设计改进，必须在文档 Ch3 添加 TODO 段落描述替代方案。

### Step 7: 自检清单确认（强制）

> **目的**：在输出最终结果前，AI 必须逐条确认以下清单。这是防止"输出看起来完整但实际漏了关键维度"的最后一道防线。
> **模式适配**：构造/快速模式跳过的 Step 标注"跳过（模式）"，深度模式必须全部确认。

```markdown
### Step 7 产物：自检清单

- [ ] Step 0 执行模式已声明 + 范围声明和时间预算已输出（或已说明省略）
- [ ] Step 0 已读取正确的 STATE.md（Trae/Claude 双路径）
- [ ] Step 1 源码文件清单已输出
- [ ] Step 1.5 覆盖率穷举已输出（SYMBOLS.md + 缺口/ARCH 判定）〔构造/深度必做，快速跳过〕
- [ ] **Gate A**: coverage-extract.py 已运行且 scan.md 附 SYMBOLS.md 路径
- [ ] Step 2 Top 3 差异 + Top 2 覆盖缺口已输出（8 字段行为契约表）
- [ ] **Gate B**: Top 5 行为契约表已产出
- [ ] Step 2.5 链路验证表格已输出（如适用）〔深度必做，构造/快速跳过〕
- [ ] Step 3 概念准确性表格已输出〔深度必做，构造/快速跳过〕
- [ ] Step 3 C 代码引用验证表格已输出〔深度必做，构造/快速跳过〕
- [ ] Step 3 数据结构覆盖表格已输出〔深度必做，构造/快速跳过〕
- [ ] Step 3 C 源码覆盖完整性表格已输出（含覆盖率）〔深度必做，构造/快速跳过〕
- [ ] Step 3 文档风格验证表格已输出（§2.11）〔深度必做，构造/快速跳过〕
- [ ] **Gate C**: Step 3.5 Precision Check 5 元规则检查表已产出
- [ ] Step 4 跨文档检查已输出〔深度必做，构造/快速跳过〕
- [ ] Step 4.1 设计决策质量表格已输出（如适用）〔深度必做，构造/快速跳过〕
- [ ] **Gate D**: P0 必检清单 5 项已回答 ✅/❌ + grep 证据（PARTIAL=FAIL）
- [ ] **Gate E**: §5 测试函数名已 grep 验证（若文档有 §5）
- [ ] Step 5 维度覆盖自检表格已输出
- [ ] Step 5 最弱项自检 4 个问题已确认
- [ ] Step 5 时间预算评估已输出（或已说明省略）
- [ ] Skill Invocation Log 已输出（真实 tool 调用记录）
- [ ] Step 5.5 scan.md 单文件已写入 + STATE.md 已更新 + Open P0/P1/P2 已同步
- [ ] Step 5.6 VERIFY-CHECK.md 已生成（收敛终止必要条件）
- [ ] Step 6 修改项已生成（P0 必须有代码修改项）〔构造/深度必做，快速跳过〕
- [ ] 所有 grep 命令的输出已作为证据附在对应表格后
```

> **如果以上任何一项未完成（且非模式跳过），AI 必须回到对应 Step 重新执行，不得跳过。**

---

## 二、Review 输出格式

> 输出格式已统一至 [review.md §AI Review 输出模板](review.md#ai-review-输出模板)。
> 所有 Review 任务必须按该模板输出，此处不再重复定义。

**关键约束**：
- 每个问题必须标注优先级（P0/P1/P2）
- 每个问题必须标注位置（`文档:章节` 或 `代码:行号`）
- 每个问题必须提供依据（源码行号或规则引用）
- 每个问题必须给出明确修复建议
- P0 问题必须生成可执行的修改项（按 Step 6 格式）

---

## 三、Review 工具命令

```bash
# ===== 根据文档所属模块，替换 {module} 为 vm/pm/vfs/kernel 等 =====

# 1. 验证常量定义
rg "^#define CONSTANT" minix3/minix/servers/{module}/ -n
rg "^#define CONSTANT" minix3/minix/kernel/ -n

# 2. 验证函数定义
rg "^return_type function_name\(" minix3/minix/servers/{module}/ -n
rg "^return_type function_name\(" minix3/minix/kernel/ -n

# 3. 验证结构体定义
rg "^struct struct_name " minix3/minix/servers/{module}/ -n
rg "^typedef struct" minix3/minix/servers/{module}/ -A 5

# 4. 验证宏使用
rg "MACRO_NAME" minix3/minix/servers/{module}/ --type c -n

# 5. 验证枚举值（头文件通常在 include/ 目录）
rg "ENUM_VALUE" minix3/minix/include/ --type h -n

# 6. 全局搜索（不确定模块时使用）
rg "SYMBOL_NAME" minix3/minix/ --type c --type h -n

# 7. 对比文档与代码
# 打开文档引用的代码位置，逐行对比
```

**模块路径速查**：
| 模块 | 源码路径 |
|------|----------|
| VM | `minix3/minix/servers/vm/` |
| PM | `minix3/minix/servers/pm/` |
| VFS | `minix3/minix/servers/vfs/` |
| Kernel | `minix3/minix/kernel/` |
| Drivers | `minix3/minix/drivers/` |
| 公共头文件 | `minix3/minix/include/` |

---

## 四、Review 快速判断口诀

> 口诀已统一至 [review.md §快速判断口诀](review.md#快速判断口诀)。
> 所有快速扫描任务使用 review.md 中的口诀，此处不再重复定义。

**使用时机**：
- 快速 Review（Profile D）时，用口诀快速扫描
- 详细 Review 时，先过一遍口诀找明显问题，再进入 Step 1-6
