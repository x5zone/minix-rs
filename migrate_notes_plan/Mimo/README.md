# MiMo 侧独立审计产物索引

本目录是独立审计方（MiMo，opencode 会话）对 notes 目录迁移的只读审计产物。
与迁移执行方的交付物 `migrate_notes_plan/` 上层文件、以及 DeepSeek（`DS/`）、GLM（`GLM/`）、
Muse（`Muse/`）三方的审计产物分开存放，以保证四份审计互相独立：
本次审计没有打开那三方的报告与证据文件，只列出过目录文件名。

## 怎么读

1. 先读 [`REVIEW-REPORT.md`](REVIEW-REPORT.md)：结论、逐项判定、问题清单、未能核对的项都在那一份里。
2. 复核某条结论时，按报告里的「证据：`evidence/itemN-*.txt`」直接打开对应文件，
   里面是当时跑的命令与原始输出。
3. 想自己重跑，用本目录下的 `audit_*.py` 脚本（全部只读，只往本目录写结果）。

## 文件清单

| 文件 | 用途 |
|---|---|
| `00-AUDIT-PLAN.md` | 审计计划：范围、独立性声明、环境事实、方法与对协议的偏离点 |
| `REVIEW-REPORT.md` | 最终审计报告（按 `REVIEW-PROMPT.md` 要求的交回格式） |
| `audit_1_content.py` | 按 `path-map.tsv` 逐条比对迁移前后 sha256，并核对映射双射与现盘文件集合 |
| `audit_1d_classify.py` | 对 333 个内容有变化的文件做行级分类（前缀替换 / 相对链接重定基 / 其余） |
| `audit_1e_residual.py` | 用路径映射表 + 前缀规则 + 相对链接重定基还原旧文本，找还原不出来的残差 |
| `audit_1f_real_edits.py` | 以真实的旧文本→新文本差异为基准，判断每处改动能否被迁移合同解释 |
| `audit_4_refs.py` | 必改域旧路径残留的字节级扫描（不依赖本机 grep 实现） |
| `evidence/` | 每一步的原始命令输出与控制台记录，文件名以报告里的引用为准 |

## 审计时点

- 审计时点：与迁移收口同日，仓库分支 `rewrite`，起始 HEAD `77be8773a`。
- 审计结束时仓库状态与开始时一致：工作树只有维护者在制的一处改动与既有未跟踪件，
  本目录是本次审计唯一新增的写入点。

## 证据文件索引

| 文件 | 内容 |
|---|---|
| `item0-boundary.txt` | 开工时的仓库边界：HEAD、分支、双标签、在制改动、未跟踪件、grep 与 python 版本 |
| `item1a-content-reconcile.txt` | 集合层：映射表与清单一一对应、2428 个目标全部存在、333 个文件内容有变化 |
| `item1d-classify.txt`、`item1d-other-lines.txt` | 第一版行级分类与明细，后续被 `item1f` 的口径取代，保留作为过程记录 |
| `item1e-residual-summary.txt`、`item1e-residual.txt` | 用合同机械变换还原旧文本后的残差（41 个文件），用于圈定需要人工判读的范围 |
| `item1f-real-edits-summary.txt`、`item1f-real-edits.txt` | 最终口径：1358 处替换可由合同解释，149 处真实编辑的逐行明细 |
| `item1g-frozen-zones.txt` | 333 个变更文件是否触及取证日志、设计快照、备份稿三个冻结区（全部 0） |
| `item2-snapshot.txt` | `snapshot.sha256` 七项校验与 tar 卷 sha256 |
| `item2b-tar-breakdown.txt` | tar 条目按顶层前缀、文件与目录拆分 |
| `item2c-tar-vs-manifest.txt` | tar 覆盖范围与两份清单的差集 |
| `item2d-tar-gap-detail.txt` | 不在 tar 卷内的 784 件明细与分类 |
| `item2d-relink-tag-testprogress.txt` | relink 清单行数、标签元数据、当时测试进度 |
| `item2e-declared-numbers.txt` | 快照第八节声明数字逐条复核，以及 relink 报告按类型计数 |
| `item3-history-conservation.txt` | 标签指向、`--follow` 回溯、纯移动提交形态、跟踪数守恒 |
| `item3b-rename-pairs.txt` | 提高 rename limit 之后仍然是 2 删除 5 新增的复核 |
| `item3c-edge3-readme-pairing.txt` | 两条删除与配对文件的 blob 对读 |
| `item3d-similarity.txt`、`item3d-similarity-lines.txt` | 两条被删文件的字节相似度与行相似度测量 |
| `item3e-relink-count.txt` | relink 报告按类型计数与执行日志里 138 这个数字的对照 |
| `item4-summary.txt` | 必改域字节级残留扫描，结果 0 |
| `item4-residue.txt` | 残留明细，因结果为 0 而为空 |
| `item4-protocol-grep.txt` | 协议原样 grep 命令的输出，加冻结区与 `.review/` 抽查 |
| `item4b-bare-mention-commands.txt` | 必改域里 `fork-syscall-rewrite` 裸提及中的命令形态样例 |
| `item4c-stale-commands.txt` | 排除归档、设计快照与对照表之后的命令形态清单，共 31 处 |
| `item4d-frozen-file-counts.txt` | 冻结区含旧路径的文件数：4 / 8 / 56 |
| `item5-links-anchors.txt` | 断链对账（新断链 0）、锚点基线行数、基线路径列检查 |
| `item5b-baseline-formats.txt` | 两份锚点基线的表头与格式 |
| `item5c-baseline-diff.txt` | 两份基线相对迁移前标签的差异，全部是路径前缀行 |
| `item5d-broken-now-vs-baseline.txt` | 现状断链报告与迁移后清单的头部与计数 |
| `item5e-broken-set-diff.txt` | 断链集合差：新增 0，消失的 27 条全在三棵树之外 |
| `item5f-scope-reconcile.txt` | 扫描范围对账：三棵树 2133 + 清单外 95 = 2228 |
| `item6a-lint-probe.txt` | 文风门三类探针：内置自测、违规文件、历史区间 |
| `item6b-linkcheck-probe.txt` | 断链检查器三类探针：内置自测、现状扫描、副本埋点 |
| `item6c-rules-and-tools.txt` | 规则三端一致性与工具在新布局下的退出码 |
| `item7a-os-static.txt` | `os/` 非注释新增行与改动总账 |
| `item7b-cargo-check.txt` | `cargo check --workspace --tests` 结果 |
| `item7c-test-workspace-summary.txt` | 全量测试汇总（两种运行方式） |
| `item7d-test-error-contexts.txt` | 11 个失败目标的原始报错上下文 |
| `item7e-failing-crates-single.txt` | 11 个失败 crate 逐个单跑的结果 |
| `item7f-rs-comment-only.txt` | `.rs` 增删行逐条判定，全部落在注释形态 |
| `item7g-nonrs-diff.txt` | 两个非 `.rs` 文件的实际差异 |
| `item7h-j1-control.txt` | 串行控制实验与失败集合比对 |
| `item7i-env-and-test-coupling.txt` | 环境事实、残留进程、测试与被搬文档的耦合检查 |
| `item7j-feature-clue.txt` | 特征集线索与双包对照实验 |
| `item7k-container.txt` | 权威容器配方下的全量测试结果（迁移后） |
| `item7l-container-pre.txt` | 同一容器配方下的全量测试结果（迁移前标签内容）与集合比对 |
| `item8a-ruling5-inflight.txt` | 在制两件的裁决核对 |
| `item8b-rulings.txt` | 其余裁决的核对 |
| `item8c-module-stage.txt` | 状态目录键与覆盖率参数两个概念的核对 |
| `item8d-offtree-untouched.txt` | 树外 2530 件按字节数与 mtime 逐条对读 |
