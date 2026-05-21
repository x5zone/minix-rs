# Profile H: Stage 1 — Ch1&2 Accuracy Verification

**Objective**: Verify that Ch1 (Concepts) and Ch2 (Source Analysis) accurately describe Minix3 C source code.
**Rules**: ~50 items (原 40 + 新增 H9/H10/H11)
**Input**: Target document + Minix3 C source code
**Output**: P0 concept errors + P0 reference errors + P0 coverage gaps + P1 architecture annotation gaps + P1 Ch1&2 Rust violations + P1 header violations

## Preparation

```bash
# Identify C source files
rg "minix3/minix/" {document} --type md -n

# Verify existence
ls minix3/minix/servers/{module}/*.{c,h}

# Extract all C symbols
rg "^static |^void |^int |^struct |^#define" minix3/minix/servers/{module}/ --type c -n
rg "^struct \w+" minix3/minix/servers/{module}/ --type c -n
rg "^typedef struct" minix3/minix/servers/{module}/ --type c -n
```

## H1: Concept Accuracy (P0, Mandatory)

From review-doc-checklist §2.1.

**Process**:
1. Extract all bold terms, type names, key nouns from Ch1
2. Extract all struct/function/macro names from Ch2
3. grep each concept against Minix3 source

| Check Item | Method | P0 if |
|-----------|--------|-------|
| Concept exists in C source | `rg "CONCEPT" minix3/.../module/ --type c` | Zero grep hits |
| Concept semantics match C source | Read surrounding context | Semantic contradiction |
| Capitalization correct (e.g., ALLOCMEM vs AllocMem) | Compare with grep output | Wrong case |

**Verification Table**:
```markdown
| Concept | Doc Claim | Source Hit | Source Semantics | Match? |
|---------|----------|-----------|-----------------|--------|
```

## H2: C Code Reference Verification (P0, Mandatory)

From review-doc-checklist §2.2.

**Process**:
1. Extract ALL C code references (file + line) from document
2. Verify file exists: `ls minix3/minix/servers/{module}/{file}`
3. Read the referenced lines: compare snippet, function name, behavior description

| Check Item | Method | P0 if |
|-----------|--------|-------|
| File exists | `ls` | File not found |
| Line range correct | Read file at referenced lines | Line off by >5, function at different line |
| Code snippet matches | Compare blocked code vs actual | Different code, different behavior |
| Explanation matches source | Read function body end-to-end | Explanation contradicts source |
| No absolute paths | grep `file:///` | Absolute path found |

**Verification Table**:
```markdown
| Ref # | File:Line | Exists? | Line OK? | Snippet OK? | Explanation OK? | Priority |
```

## H3: Numeric Constant Verification (P0)

From review-doc-checklist §2.3.

**Process**:
1. Extract all numeric constants from document
2. grep each in C source headers

```bash
rg "#define CONSTANT_NAME" minix3/minix/servers/{module}/ --type c -n
```

| Check Item | Method | P0 if |
|-----------|--------|-------|
| Constant value matches source | grep `#define NAME` | Wrong value |
| Constant semantics match | Read definition + usage context | Wrong meaning |

**Verification Table**:
```markdown
| Constant | Doc Value | Source Value | Source Location | Match? |
```

## H4: Algorithm Description Verification (P0)

From review-doc-checklist §2.3.

**Process**:
1. Extract all algorithm descriptions from Ch2
2. Read the corresponding C function end-to-end
3. Compare behavior claim against actual code path

| Check Item | Method | P0 if |
|-----------|--------|-------|
| Algorithm flow matches C function | Read function body | Wrong order, missing branch |
| All branches described | Trace all if/switch/case | Branch unmentioned |
| Error handling described | Trace error return paths | Error path missing |

**Verification Table**:
```markdown
| Algorithm | Doc Description | Source Function | Read Lines | Match? | Missing |
```

## H5: Data Structure Coverage (P1, Mandatory)

From review-doc-checklist §2.5.

**Process**:
1. Extract all struct names from document Ch2
2. List all structs in C source: `rg "^struct \w+"` and `rg "^typedef struct"`
3. For each struct in scope, read definition, compare with document analysis

| Check Item | Method | P0/P1 if |
|-----------|--------|----------|
| Core struct analyzed | Check doc has § for struct | Completely missing → P0 |
| All fields analyzed | Compare struct definition vs doc | Key fields missing → P1 |
| Field semantics correct | Read field usage in C code | Wrong interpretation → P1 |

**Coverage Table**:
```markdown
| Struct | C Source | Fields | Doc Section | Fields Covered | Missing Fields | Priority |
```

## H6: Architecture Evolution Annotation (P1)

From review-doc-checklist §2.5.5 + §2.6.

If document describes 32-bit architecture (e.g., 2-level page tables):

| Aspect | Minix3 (32-bit) | minix-rs (64-bit) | Doc Annotates? | P1 if |
|--------|----------------|-------------------|---------------|-------|
| Page table levels | 2 (PD+PT) | 4 (PML4+PDPT+PD+PT) | ✅/❌ | ❌ |
| Page table entry size | 32-bit | 64-bit | ✅/❌ | ❌ |
| Address split | 10+10+12 | 9+9+9+9+12 | ✅/❌ | ❌ |
| Page table pointer array | pt_pt[1024] | Dynamic allocation | ✅/❌ | ❌ |
| Address width | 32-bit linear | 48-bit virtual | ✅/❌ | ❌ |

## H7: Diagram Quality (P2)

From review-doc-checklist §2.7.

| Check Item | Criteria |
|-----------|----------|
| Necessity | Diagram conveys info text alone cannot? |
| Alignment | ASCII borders, arrows, text strictly aligned? |
| Maintainability | Can diagram be edited by adding one row without cascading reformat? |
| Information density | Each element carries unique information? |
| Simpler alternative | Can a table/list replace it? |

## H8: C Source Coverage Completeness (P0)

From review-doc-checklist §2.8.

**Process**:
1. Determine semantic scope from document title + Ch1
2. List all C source files in the module
3. Extract all C symbols: `functions`, `structs`, `macros`, `enums`
4. For each symbol within semantic scope, check if document covers it
5. Coverage rate < 80% → P0

**Coverage Table**:
```markdown
| Symbol | Type | C File | In Scope? | Documented? | Doc Location | Priority |
```

**Statistics**:
```markdown
| Metric | Count | Rate |
|--------|-------|------|
| Total symbols in scope | {N} | — |
| Covered | {M} | {M/N*100}% |
| Uncovered | {K} | {K/N*100}% |
```

## H9: Ch1&2 Rust Content Check (P1, Mandatory)

> **硬规则来源**: review-doc-checklist.md L54 — "Ch1&2 不得包含 Rust 内容"
> **历史教训**: 10-phys-block.md、15-pagefault.md 均因 Ch2 包含"方案四视角"/"Direct Map 视角"等 Rust 设计内容被多次遗漏，直到第 2-3 轮 review 才发现。

**Process**:
1. 扫描 Ch1 和 Ch2 中的所有代码块
2. 检查代码块是否使用 Rust 语法
3. 检查 Ch1&2 中是否出现 Rust 设计术语

| Check Item | Method | P1 if |
|-----------|--------|-------|
| Ch1&2 代码块使用 Rust 语法 | 搜索 `fn `, `impl`, `pub`, `struct`, `trait`, `let `, `::`, `->`, `&mut`, `Option<`, `Result<` | 任何 Rust 代码块出现在 Ch1&2 |
| Ch1&2 出现 Rust 设计术语 | 搜索 "方案四视角", "Direct Map 视角", "Direct Map 标注", "HeapArena", "BumpBuf", "PhysBytes", "VirBytes" | 任何 Rust 设计术语出现在 Ch1&2 |
| Ch1&2 出现 Rust 版本对比 | 搜索 "Rust 版本", "Rust 中", "Rust 实现" | 任何 Rust 版本描述出现在 Ch1&2 |

**修复方式**: 将 Rust 内容移至 Ch3（添加 §3.0 或在 §3.1 中增加子节），Ch1&2 仅保留 Minix3 概念和 C 源码分析。

**Verification Table**:
```markdown
| Location | Content Type | Rust Content? | Moved To | Priority |
|----------|-------------|--------------|----------|----------|
```

## H10: Document Header Norm Check (P1, Mandatory)

> **历史教训**: 11-memtype.md 被标记为 "VM库" 但实际应为 "VM私有"，多轮 review 后才发现。

**Process**:
1. 读取文档头部（前 10 行）
2. 验证分类标签
3. 验证 §7 参见中的文件名

| Check Item | Method | P1 if |
|-----------|--------|-------|
| 分类标签正确 | 读取 `> **分类**:` 行 | VM 内部概念标为 "VM库" |
| 分类标签与同类文档一致 | 对比同目录其他文档的标签 | 与同类文档不一致 |
| §7 参见文件名存在 | `ls` 每个引用的文件 | 文件不存在（断链） |
| §7 参见文件名正确 | 对比实际文件名 | 文件名拼写错误 |

**分类标签判定规则**:
- **VM私有**: 仅 VM 内部使用的数据结构/函数（如 PhysBlock, MemType, VirRegion, PageFault 处理）
- **VM库**: 可被其他服务使用的接口（如 IPC 消息格式、系统调用接口）
- **全局基建**: 跨模块共享的基础设施（如 slab 分配器、物理内存管理）

**Verification Table**:
```markdown
| Check Item | Doc Value | Expected Value | Match? | Priority |
|-----------|----------|---------------|--------|----------|
| 分类标签 | | | | |
| §7 引用文件1 | | exists? | | |
| §7 引用文件2 | | exists? | | |
```

## H11: Grep Evidence Requirement (强制)

> **目的**: 防止 AI "凭印象扫描"而非"逐条验证"。

**规则**: 对 H1-H10 中的每个验证声明，必须满足以下条件之一：
1. 提供了 grep/read 命令的输出作为证据
2. 提供了源码行号作为证据
3. 明确标注"未验证"并说明原因

**禁止**:
- 仅凭记忆或印象给出"✅ 通过"
- 不执行 grep 就声称"源码中不存在"
- 选择性忽略 grep 结果为空的情况

**Verification Table**:
```markdown
| Claim | Evidence Type | Evidence Detail | Verified? |
|-------|-------------|----------------|-----------|
```

## H12: Output Format

```markdown
# Profile H: Ch1&2 Accuracy — {document}

## Summary
- Concepts verified: {N}, errors: {M}
- C references verified: {N}, errors: {M}
- Constants verified: {N}, errors: {M}
- Algorithms verified: {N}, errors: {M}
- Structs analyzed: {N}, gaps: {M}
- Architecture annotation gaps: {N}
- Ch1&2 Rust violations: {N}
- Header violations: {N}
- Coverage rate: {X}%

## P0 Issues
| # | Type | Location | Description | Evidence | Fix |
|---|------|---------|-------------|---------|-----|

## P1 Issues
| # | Type | Location | Description | Evidence | Fix |
|---|------|---------|-------------|---------|-----|

## Verdict
- Ch1&2 accuracy: 🟢 Accurate / 🟡 Minor issues / 🔴 Major issues
- Ready for Stage I (Profile I): ✅ Yes / ⚠️ Fix P0 first / ❌ Major rework needed
```
