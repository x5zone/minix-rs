# Review 执行流程

> 本文档定义 AI 执行 Review 的强制步骤和工具命令。
> 输出格式见 [review.md §AI Review 输出模板](review.md#ai-review-输出模板)，口诀见 [review.md §快速判断口诀](review.md#快速判断口诀)。

---

## 一、AI 执行 Review 的强制步骤

> 为防止 AI 在 Review 过程中"浮躁"或遗漏关键验证，必须按以下步骤执行。

### Step 1: Ground Truth Lookup（源码定位）

- 识别文档中提到的所有 Minix3 源文件
- 使用 `rg` 命令验证这些文件是否存在于 `minix3/` 目录
- 记录每个引用的**文件路径**和**行号范围**

> **效率提示**：如果文档较长（>500 行），优先用 grep 提取所有 `.c` / `.h` 文件引用（`rg "\.c" doc.md`），然后抽样验证引用行号的准确性，而非逐行全量验证。Step 3 再补做行号精确验证。

### Step 2: Diff Extraction（差异提取）

- 列出 **3 个**你认为文档/代码中最背离 Minix3 原始语义的地方
- 对每个差异，说明：
  - Minix3 源码的实际行为
  - 文档/代码中的描述/实现
  - 差异的性质（概念错误、语义偏移、过度简化等）

### Step 2.5: Link Validation（链路验证）

> **目的**：验证文档各章节之间的推导链路是否完整。链路断裂 = 设计或实现有问题。
> 详细的链路验证规则见 [review-doc-checklist.md §2.10](review-doc-checklist.md#210-章节链路验证)。

**链路验证**：
1. Ch3→Ch1&2：每个设计决策是否有依据？
2. Ch4→Ch3：每个实现是否对应设计？
3. 测试→Ch3+Ch4：测试是否覆盖设计决策和实现细节？
4. 代码→Ch4：代码是否与文档描述一致？

**输出**：列出所有链路断裂点，标注优先级

> **注意**：局部 Review（仅 Ch1&2）时跳过此步骤。完整 Review 时在执行 Step 4 后，结合跨文档信息做最终链路验证。

### Step 3: Sanity Check（一致性检查）

- 验证文档第 2 章引用的**所有行号**是否真实存在于本地文件
- 验证文档中的**所有数值常量**是否与 Minix3 源码一致
- 验证文档中的**所有函数签名**是否与 Minix3 源码一致

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

- 按"Review 输出格式"整理所有发现
- 每个问题必须标注优先级（P0/P1/P2）
- 每个问题必须提供明确的修改方向

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
