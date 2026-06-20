---
name: "review-coverage-skill"
description: "Minix-RS 覆盖率穷举检查。使用 tools/coverage-extract/ 脚本生成 SYMBOLS.md 骨架，AI 补充语义判断（Rust对应关系、架构演进标记、语义归属、行为契约、测试覆盖）。解决覆盖率不足和跨轮次累积问题。当 Agent 需要检查 C 源码覆盖完整性、Rust 实现完整性、或生成/更新覆盖率清单时调用此 Skill。"
---

# Minix-RS 覆盖率穷举检查

> **目的**：机器生成穷举清单，AI 只负责语义判断。解决 AI "凭印象扫描"导致覆盖率不足的问题。
> **基础**：`tools/coverage-extract/coverage-extract.py` 确定性脚本生成 SYMBOLS.md 骨架。

---

## 0. Gate A 强制运行规则（反造假）

> **背景**：6 AI 共识（improve-v2 §1.2）——曾有 review 明确写"未运行 coverage-extract.py"，紧接着标 "✅ 通过"，直接 Gate 造假。本节堵住该漏洞。

**强制规则**：
1. **coverage-extract.py 必须运行** — 不允许"语义范围手动验证"代替。
2. **命令行 + stdout 摘要必须写入 scan.md 的 `gate-evidence-A` 块**（见下模板）。
3. **SYMBOLS.md 必须落到磁盘**（`.review/trae/{rw-module}/...` 标准路径；Trae IDE 硬编码 `.review/trae/`，不要使用 `{tool}` 变量），scan.md 附 `ls` 证明存在。
4. **禁止"使用其他 AI 的 SYMBOLS.md 替代"** — 必须为本 AI 本次 review 重新生成。

**降级路径**（仅当脚本物理不可用）：
- 在 scan.md 明确记录：`⚠️ coverage-extract.py 不可用（原因：xxx），Gate A 降级为 PARTIAL`
- **PARTIAL 状态 ≠ PASS**，不允许进 Final Review
- 必须提供修复 plan

**gate-evidence-A 块模板**（scan.md 必备，机器可校验关键字）：
````
```gate-evidence-A
command: python3 tools/coverage-extract/coverage-extract.py kernel {doc_dir} \
  --rust-dir os --c-dir minix3/minix/kernel \
  --semantic-map tools/coverage-extract/kernel-semantic-map.json \
  --doc-file {target-doc}.md \
  --output .review/trae/{rw-module}/scans/{doc-stem}-{agent}-SYMBOLS.md
exit: 0
stdout_contains: ["Found ", "C symbols", "Coverage Summary"]
artifact: .review/trae/{rw-module}/scans/{doc-stem}-{agent}-SYMBOLS.md
artifact_exists: true
```
````
校验规则（可由脚本执行）：Gate A 证据块必须含 `coverage-extract.py` + `Coverage Summary` + artifact 路径，且 `ls artifact` 成功。无证据块或关键字缺失 → Gate A 判 FAIL，不允许"自报 ✅"。

**证据强度**：Gate A 必须 **L1**（工具自动输出）。L3（语义推断）视为 FAIL，除非标 `MANUAL_FALLBACK` 并说明原因（此时降级 PARTIAL）。

---

## 1. 执行流程

### Step 1: 生成 SYMBOLS.md 骨架（机器）

```bash
# 基本用法（服务器模块：vm / pm / vfs / rs / ds / inet ...）
#   注：脚本第一参数 {minix3-module} 是 Minix3 模块名；--output 路径里的 {rw-module} 是 rewrite 模块名
python3 tools/coverage-extract/coverage-extract.py {minix3-module} <doc_dir> \
  --rust-dir os \
  --c-dir minix3/minix/servers/{minix3-module} \
  --output .review/trae/{rw-module}/scans/SYMBOLS.md

# 基本用法（内核）
python3 tools/coverage-extract/coverage-extract.py kernel <doc_dir> \
  --rust-dir os \
  --c-dir minix3/minix/kernel \
  --output .review/trae/{rw-module}/scans/SYMBOLS.md

# 单文档用法 — 服务器模块（必须指定 --doc-file）
python3 tools/coverage-extract/coverage-extract.py {minix3-module} <doc_dir> \
  --rust-dir os \
  --c-dir minix3/minix/servers/{minix3-module} \
  --doc-file {target-doc}.md \
  --semantic-map tools/coverage-extract/{minix3-module}-semantic-map.json \
  --output .review/trae/{rw-module}/scans/{doc-stem}-{agent}-SYMBOLS.md

# 单文档用法 — 内核
python3 tools/coverage-extract/coverage-extract.py kernel <doc_dir> \
  --rust-dir os \
  --c-dir minix3/minix/kernel \
  --doc-file {target-doc}.md \
  --semantic-map tools/coverage-extract/kernel-semantic-map.json \
  --output .review/trae/{rw-module}/scans/{doc-stem}-{agent}-SYMBOLS.md

# 示例
python3 tools/coverage-extract/coverage-extract.py vm \
  notes/rewrite/fork-syscall-rewrite/02-stage-vm \
  --rust-dir os --c-dir minix3/minix/servers/vm

python3 tools/coverage-extract/coverage-extract.py kernel \
  notes/rewrite/fork-syscall-rewrite/03-stage-kernel \
  --rust-dir os --c-dir minix3/minix/kernel \
  --doc-file 03-kmain-cstart.md \
  --semantic-map tools/coverage-extract/kernel-semantic-map.json \
  --output .review/trae/fork-syscall-rewrite/scans/03-kmain-cstart-glm-SYMBOLS.md
```

> **注意**：`--c-dir` 对服务器模块是 `minix3/minix/servers/{minix3-module}`，对内核是 `minix3/minix/kernel`。Trae IDE 时 agent=glm/kimi/...（硬编码 `.review/trae/`，不再使用 `{tool}` 变量）。

**输出**：`.review/trae/{rw-module}/scans/SYMBOLS.md`（模块级）或 `.review/trae/{rw-module}/scans/{doc-stem}-{agent}-SYMBOLS.md`（文档级）

**关键参数说明**：
- `--rust-dir os`：扫描整个 `os/` 目录，避免 `kmain`/`ProtectionArch` 等跨 crate 符号被遗漏。
- `--doc-file`：限定只统计某篇文档的覆盖率，防止两篇 doc 的 coverage 数字完全相同。
- `--semantic-map`：C→Rust 改写必须提供语义映射表；否则 Rust 覆盖率会显示为 0%（名称不匹配）。
- `--output`：明确产物路径，避免覆盖其他文档的 SYMBOLS.md。

**Rust 覆盖率 0% 的处理规则**：
- 如果 Rust 覆盖率显示 0%，**必须先检查原因**，不能直接接受或忽略：
  1. 是否未提供 `--rust-dir` 或路径错误？→ 修复命令后重跑。
  2. 是否未提供 `--semantic-map` 导致名称匹配失败？→ 补充语义映射表。
  3. Rust 实现是否确实缺失？→ 作为 P0 缺口记录。
- 在 scan.md 中必须说明 Rust 覆盖率 0% 的原因及处理结论，不能留白。

**覆盖率数字唯一性规则**：
- 同一模块下不同文档的 coverage 数字**不得完全相同**。
- 如果两篇 doc 的 "C 符号总数 / 文档覆盖 / Rust 覆盖" 完全一致，说明未正确使用 `--doc-file` 过滤，必须重跑。

脚本自动完成：
- 提取 C 源码所有函数/结构体/宏/枚举
- 提取 Rust 源码所有 pub 项
- 检查每个 C 符号在文档中是否被提及（名称匹配）
- 检查每个 C 符号在 Rust 中是否有名称匹配的实现
- 生成覆盖率统计 + 逐符号表格

### Step 2: AI 语义判断（人工补充）

机器只能做名称匹配，以下 5 项必须 AI 补充：

#### 2.1 Rust 对应关系确认

名称匹配 ≠ 语义对应。AI 需对每个 `✅(名称匹配)` 的符号确认：

```markdown
| C 符号 | Rust 实现 | 语义对应? | 说明 |
|--------|----------|----------|------|
| `alloc_mem` | `alloc_mem()` in alloc.rs | ✅ 语义一致 | 分配物理内存，签名和行为对齐 |
| `free_mem` | `free_mem()` in alloc.rs | ⚠️ 签名不同 | C 返回 void，Rust 返回 Result |
| `map_service` | (无) | ARCH: 不需要 | IPC 初始化在 Rust 中由 trait 处理 |
```

**判定标准**：
- ✅ 语义一致：外部可观察行为相同（输入/输出/副作用/错误码）
- ⚠️ 签名不同：语义一致但 Rust 类型系统改变了签名（允许的演进）
- ❌ 语义偏移：行为有差异（P0）
- ARCH: 架构演进不需要（需标注理由）

#### 2.2 架构演进标记

某些 C 函数在 Rust 中不需要，AI 需标记：

```markdown
| C 符号 | 标记 | 理由 |
|--------|------|------|
| `map_service` | ARCH | IPC 协议演进，Rust 用 trait 抽象 |
| `pt_t` (32位) | ARCH | 64 位页表层级变化，用 4 级页表 trait |
| `bitchunk_t` | ARCH | 用 bitflags 替代 |
```

**可接受的架构演进理由**（见 review.md §Allowed Evolution）：
- 32→64 位位宽变化
- 2→4 级页表
- `int+macros`→`enum`
- `errno`→`Result`
- `free()`→RAII
- `bitchunk_t`→`bitflags`
- IPC 协议演进（需详细说明）

#### 2.3 语义归属判定

某些符号可能属于其他文档的语义范围：

```markdown
| C 符号 | 定义位置 | 语义归属 | 处理建议 |
|--------|---------|---------|---------|
| `vm_mappages` | mmap.c | 07-pagetable-ops.md | 当前文档仅引用，不重复分析 |
| `pt_t` | proto.h | 06-pagetable-struct.md | 在 06 中完整覆盖 |
```

**判定原则**：以**功能语义**为准，而非"哪个文件定义了它"。

#### 2.4 行为契约表（核心函数）

对每个核心函数（IPC handler、syscall、关键状态机），补充行为契约：

```markdown
## do_brk (break.c:44)

| 列 | 值 |
|----|-----|
| 输入 | proc_nr, new_brk |
| 输出 | 旧 brk 值 |
| 副作用 | 修改 proc->p_brk, 调整 region 映射 |
| 错误码 | ENOMEM (无法扩展), EINVAL (非法地址) |
| 时序 | 必须在 PM 持有 proc 锁后调用 |
| Rust 对应 | brk.rs:50 do_brk() |
| 测试对应 | test_brk.rs |
```

详见 [review-core-semantics.md](../review-rules/review-core-semantics.md)。

#### 2.5 测试覆盖补充

```markdown
| C 符号 | Rust 实现 | 测试文件 | 测试函数 | 覆盖度 |
|--------|----------|---------|---------|--------|
| `alloc_mem` | alloc.rs | test_alloc.rs | test_alloc_free_basic | L1+L2 |
| `do_brk` | brk.rs | (无) | - | ❌ 缺测试 |
```

**测试三重标准**（见 review-code-excellence.md）：
- L1 C-Rust 对偶测试：验证 Rust 实现与 C 行为一致
- L2 Trait 契约测试：验证 trait 实现满足契约
- L3 doctest：文档中的代码示例可运行

### Step 3: 更新 STATE.md

将覆盖率结果写入工具对应的 `STATE.md`（双路径，互不共享）：
- **Trae** → `.review/trae/{module}/STATE.md`
- **Claude** → `.review/claude/{module}/STATE.md`

```markdown
## Coverage Status

- **SYMBOLS.md**: .review/trae/{rw-module}/scans/SYMBOLS.md (generated YYYY-MM-DD)
- **C 符号总数**: 351
- **文档覆盖**: 320/351 (91.2%)
- **Rust 覆盖**: 10/351 (2.8%, 名称匹配) / AI确认: X/351
- **完全缺口**: 31
- **架构演进标记**: X 个符号标记为 ARCH
- **行为契约表**: X/Y 核心函数已补充
- **测试覆盖**: X/351 有测试对应

### 缺口清单（P0 候选）
- [ ] `func_name` (file.c:N) — 在语义范围内但无文档无Rust
- [ ] ...
```

---

## 2. 覆盖率判定标准

### 状态图例

| 状态 | 含义 | 优先级 |
|------|------|--------|
| ✅ 覆盖 | 文档+Rust 均有，语义一致 | - |
| ⚠️ 有文档无Rust | 文档已分析但未实现 | P2（可能是后续阶段任务） |
| ⚠️ 有Rust无文档 | Rust 实现了但文档未分析 | P1（应补充文档） |
| ❌ 缺口 | C 有但文档和 Rust 均无 | P0（需判断是否在语义范围内） |
| ARCH | 架构演进，不需要 | - （需标注理由） |
| ❌ 语义偏移 | Rust 实现行为与 C 不同 | P0 |

### 缺口判定流程

对每个 `❌ 缺口` 符号：

1. **判断是否在语义范围内**：
   - 该符号的功能是否与当前文档主题相关？
   - 如果相关 → P0：必须补充文档和实现
   - 如果不相关 → 标注"属于 XXX 文档的语义范围"

2. **判断是否为架构演进**：
   - 该符号是否因 32→64 位/类型系统/RAII 等原因在 Rust 中不需要？
   - 如果是 → 标注 ARCH + 理由
   - 如果否 → P0：必须实现

3. **判断是否为辅助函数**：
   - 该符号是否为内部辅助函数（如 sanity check、debug print）？
   - 如果是 → P2：可选实现
   - 如果否 → 按上述流程判断

---

## 3. 跨轮次累积

SYMBOLS.md 是**累积**的：

- **首次生成**：脚本生成完整骨架
- **后续轮次**：AI 在骨架上补充语义判断，不重新生成
- **文档/代码更新后**：重新运行脚本，diff 新旧 SYMBOLS.md，只处理新增符号

```bash
# 更新流程
python3 tools/coverage-extract/coverage-extract.py vm notes/rewrite/fork-syscall-rewrite/02-stage-vm
# 脚本会覆盖 SYMBOLS.md，AI 需要从 STATE.md 恢复之前的语义判断
# 建议：AI 补充的语义判断单独存入 SYMBOLS-ANNOTATED.md
```

---

## 4. 与其他 Skill 的关系

| Skill | 关系 |
|-------|------|
| [review-doc-skill](review-doc-skill.md) §2.8 | 本 Skill 是 §2.8 C源码覆盖完整性的**增强版**，提供机器穷举 |
| [review-process-skill](review-process-skill.md) Step 1.5 | 本 Skill 在 Step 1.5 执行（Ground Truth Lookup 之后） |
| [review-core-semantics](../review-rules/review-core-semantics.md) | 行为契约表的定义在此 Skill 中使用 |
| [review-code-excellence](../review-rules/review-code-excellence.md) | 测试三重标准在此 Skill 中使用 |

---

## 5. 反幻觉机制

> 覆盖率检查本身容易产生幻觉（AI 声称"已覆盖"但实际未验证）。

### 强制"先读后判"

对每个符号的覆盖判定，AI 必须先执行 grep，再下结论：

```bash
# 验证文档覆盖
rg "symbol_name" notes/rewrite/{module}/ --type md -n
# 如果有结果 → 文档覆盖
# 如果无结果 → 文档未覆盖（不要猜测"可能在其他文档"）

# 验证 Rust 覆盖
rg "symbol_name" os/servers/{module}/src/ --type rust -n
# 如果有结果 → Rust 实现
# 如果无结果 → Rust 未实现
```

### Evidence 分级

每个覆盖判定必须标注 evidence 级别：

| 级别 | 含义 | 可信度 |
|------|------|--------|
| DIRECT | grep 直接验证（有明确输出） | 高 |
| MEDIUM | 名称匹配但语义需进一步确认 | 中 |
| INFERRED | 基于上下文推断（未直接验证） | 低，需标注"待确认" |

---

## 6. 输出格式

覆盖率检查的中间产物：

```markdown
### Coverage Check 产物

**SYMBOLS.md**: .review/trae/{rw-module}/scans/{doc-stem}-{agent}-SYMBOLS.md（文档级）或 .review/trae/{rw-module}/scans/SYMBOLS.md（模块级）
**生成命令**（服务器模块示例）：
```bash
python3 tools/coverage-extract/coverage-extract.py {module} {doc_dir} \
  --rust-dir os --c-dir minix3/minix/servers/{module} \
  --semantic-map tools/coverage-extract/{module}-semantic-map.json
```
（内核将 `--c-dir` 替换为 `minix3/minix/kernel`）

**统计**:
| 指标 | 数值 |
|------|------|
| C 符号总数 | N |
| 文档覆盖 | M/N (P%) |
| Rust 覆盖 | K/N (Q%) |
| 完全缺口 | G |
| 架构演进 | A |

**P0 缺口**（在语义范围内但无文档无Rust）:
| 符号 | C 源码 | 判定 | evidence |
|------|--------|------|----------|
| `func_name` | file.c:N | P0 缺口 | DIRECT: rg 无结果 |

**P1 有Rust无文档**:
| 符号 | Rust 位置 | evidence |
|------|----------|----------|
| `func_name` | file.rs:N | DIRECT: rg 有结果但文档无 |
```
