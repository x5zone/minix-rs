# REVIEW-TRACE — doc-gen 五轮迭代（01-stage-kernel 01~05）

> 外层驱动 `AI-chats/doc-gen-iterate-prompt.md`，内层 `AI-chats/doc-gen-prompt.md`。每轮一节，**只写增量、含失败与返工**。追加前自检 `grep -c '^## Round'` 防复述。
> 工作树：`.wt/docgen-iter-01-qoder`（分支 `claim/DOCGEN-ITER-01-qoder`）；owner=qoder。
> 并发：另有 `.wt/p-all-05-qoder`（P-ALL-05）活跃；避让 `os/kernel`、`os/arch`、`coordination/TODO-3ARCH-PARITY*`、`todo-fix-prompt.md`。

## Round 0 · 清理既有 01（一次性）

- 现算读数：机器版 01 = 501 行；draft/01 人工版 = 2019 行（对比基线）。
- 动作：`git mv 01-boot-shim-bootstrap.md → draft/01-boot-shim-bootstrap.gen-20261008.md`；`git rm 01-todo.md`。commit `e28a3615e`。
- `.design/01-*` 与 `.review/` 系 gitignored → 不随工作树检出 → Round 1 走 **Step 0.3 就地现推大纲/design**（旧 outline 不继承，符合规程）。
- 中产决策（trace 要求写选择）：**删除**旧快照路径（工作树本就没有），非"保留当证据"。

<!-- 以下各轮 Round 1..5 由执行时逐轮追加 -->
