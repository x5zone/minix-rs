# D01 重写基线 scan（R37，full）

> 文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（2673 行，sha256 `665230ebb46af3d1…`）
> 状态：ROUND-CLOSED；不是 CONVERGED（full 轮无收敛声明权；G11/G13/外部 todo 哨兵另见 loop §7）。

## Scope

- 目标：01 篇全文 + `related-code.txt` 的 48 个关联 Rust 文件。
- 模式：doc review（本轮不改代码）。
- 不在范围：`minix3/`、`os/` 的代码修改；相邻模块；下一批结构重构。

## Skill Invocation Log

| 技能 | 调用方式 | 结果 |
|---|---|---|
| `review-loop` | `Skill(review-loop)`，参数 `01-stage-kernel/01-boot-shim-bootstrap R37; ... full` | 已加载并按 L0–L7 执行 |
| 面板 brief | 由 `prompt/review-rules/review-panel.md` 第四节逐字派发 | 5 个独立角色 |

## Step 0 硬阻断预检

```text
$ ls rewrite-notes/01-stage-kernel/.design/01-outline.v5.md rewrite-notes/01-stage-kernel/.design/01-design.v4.md
两文件存在

$ bash tools/design-coverage-check.sh 01-stage-kernel
✅ ALL DOCS COMPLETE: design + outline 快照齐全
```

结果：PASS。快照未被当作事实合同；与文档冲突时以文档/源码为准。

## Blocker Gates

| Gate | 证据（命令与读数） | 状态 |
|---|---|---|
| 0 预检 | 上节 `design-coverage-check` PASS | ✅ |
| A 锚点 | `bash tools/anchor-resolve.sh --check <doc>` → `anchors=89 resolved=89 zero-def=0 multi-def=0` | ✅ |
| B 引文 | `python3 tools/doc-citation-check.py <doc>` → `blocks=48 checked=21 failed=0 skipped=23 warned=4` | ✅ |
| C 结构/链接 | `doc-structure-check.py` → 表与内部 .md 链接均可解析；`AWK=gawk doc-ref-check.sh --check` → 内部章节引用均可解析 | ✅ |
| D 关联代码 | `doc-code-map.sh <doc> --check` → 48 个文件，存在性 48/48 | ✅ |
| D-6 文风 | `doc-style-lint.sh <doc>` → `error级=0` | ✅ |
| E 承载门 | `doc-code-density-check.sh --check <doc>` → 4 个可判章均有码/命令 | ✅ |
| G 出版 | `doc-publication-check.py <doc>` → 无新增违规 | ✅ |
| H TODO | `todo-ledger.py check --doc rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md --fail-on-open` → 路径过滤读数 OK；按 doc 基名运行仍见两条外部哨兵（loop §7 记录） | ⚠️ |

## Issue List

- 40 个 FINDING + 6 个 CLAIM；逐条处置见 `defect-ledger.md` 的 `## R37` 段，摘要见 `loop.md` §2–§4。
- 处置：① 40 / FP 3 / SUGG 3；未闭环 0。
- 全部为文档修复；`os/`、`minix3/` 零改动。

## Structure 摘要

- 见 `structure.md`：12 个骨架单元的读者问题、承载内容与接缝。
- 主轴：01 = 交接载荷（内存清单、内核位置、交接结构 + `pre_init` 接缝）。
- 证据：`SYMBOLS.md` 列锚点覆盖；`VERIFY-CHECK.md` 列独立回读与一致性。
