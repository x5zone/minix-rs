# 五轮串行迭代：用 doc-gen-prompt 生成 01~05 并逐轮改进 REVIEW 工作流

> **已废弃（2026-10-28 注）**：本驱动协议为 docgen 第一棒（以用户否决收场）。演化链：本文件 → `doc-gen-iterate-prompt-v2.md` → 第三棒协议（原 `prompt/REVIEW-TRACE-docgen-01-05-v3.md` §协议，该文件已删档、git 历史可查）→ 统一规格 `prompt/DOC-ITER-WORKFLOW.md`。本文引用的 `prompt/` 取证文件已同批删档，增量承接见 `prompt/WORKFLOW-ADVICE-ARCHIVE.md`。

> 用途：**外层迭代驱动器**。每轮跑一遍 `AI-chats/doc-gen-prompt.md` 产出一篇正式文档，然后与 `draft/` 里人工审阅过的旧版做**教学性对比**，据此改进 REVIEW 工作流与本 prompt，提交，再回归复核自己的提交——串行五轮。
> 用法：整段发给 agent。先读 `AI-chats/doc-gen-prompt.md`（内层流程），本篇只管轮次编排、对比方法与改进纪律。
> 最终目标：五轮后当前工作流能稳定产出高质量技术文档——**产出好文档是结果，把工作流磨硬是主线**。

## 参数

```
stage  = 01-stage-kernel
nums   = 01 02 03 04 05          # 轮次 k 对应 nums[k]，序小到大，串行五轮
tool   = qoder                    # .review/{tool}/{stage}/ 的实际目录名
tree   = rewrite-notes
owner  = <本会话标识>              # claim 用，如 doc01iter
trace  = prompt/REVIEW-TRACE-docgen-01-05-5rounds.md
```

## ⚠️ 并发前提（另一个线程正在跑 todo-fix，这一节是硬约束）

- **必须走 worktree**：`tools/claim.sh claim DOCGEN-ITER-<num> <owner>` → `cd .wt/<id>-<owner>/`。**共享主树内禁止 `checkout` / `reset --force`**（`C-35` 判例：一次切分支销毁其它会话未提交在制品）。
- **开工前避让双查**：`bash tools/check-staged-inflight.sh --list` + `git status --porcelain`。高概率重叠路径**只读不写**：`os/kernel/`、`os/arch/`、`rewrite-notes/coordination/TODO-LEDGER-OPEN.md`、`AI-chats/todo-fix-prompt.md`。
- ⛔ **本迭代不得修改 `AI-chats/todo-fix-prompt.md`**（对方的战场）。要改它就在 `new_edge4.md` 登记跨界改点并先问用户。
- 会改到的共享热点：`AI-chats/doc-gen-prompt.md`、`prompt/review-rules/*`。改前先 `git log --oneline -5 -- <file>` 看有无新落地；改完**立刻提交**，不留长期脏态（在制品即提交）。
- **禁止 `git add -A`**：只 `git add` 自己明确改动的路径，提交前必跑 `check-staged-inflight.sh`（它会拦在制文件）。
- **QEMU 必须串行**：跑真机门前 `tools/claim.sh list` + 直接问用户"对方线程是否在跑 qemu"，确认空闲才跑。
- **worktree 内 untracked 不跨树**：`.review/`、`tmp/` 是本地未跟踪产物，`release` 删树即消失。所以**跨轮要留存的内容必须落到跟踪路径**（`trace` 文件），原始日志在 release 前拷回主树 `tmp/doc-gen-iterate/`。

---

## 第 0 轮（一次性）· 清理 01 的既有产物

现况已核实：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（机器生成版，**现算 501 行**——注意它已从别家 bagging 报告里的 295 行增长过，所以所有读数必须 `wc -l` 现算，不得抄旧表）与 `draft/01-boot-shim-bootstrap.md`（2019 行，**人工审阅版，是本轮对比基线**）**同名**，故只能改名迁移，不能直接搬。

1. 先确认跟踪状态再动手：`git ls-files rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md rewrite-notes/01-stage-kernel/01-todo.md`。
   - 已跟踪 → `git mv rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md rewrite-notes/01-stage-kernel/draft/01-boot-shim-bootstrap.gen-20261008.md`（保留历史，文件名带 `gen-` 前缀避免与人工版重名）；`01-todo.md` 用 `git rm`。
   - 未跟踪 → 普通 `mv` / `rm`，同样只用上面这两条精确路径。
2. 中间结果二选一，并把选择写进 `trace`：
   - **保留** `.design/01-*` 与 `.review/qoder/01-stage-kernel/01-boot-shim-bootstrap*` 当上一轮证据（**默认建议**：Step 0 预检要查 `ls {tree}/{stage}/.design/{num}-*.v*.md`，留着可直接过门）。
   - **删除**：只删本文 doc-stem 对应的那一个目录/文件，禁止对 `.design/` 或 `.review/` 整棵树用通配。删掉后 Step 0 预检不过 → 走 Step 0.3 就地重生成（不中断 review，不许标 N/A，不许复用他篇快照）。
3. ⚠️ **旧 `.design/01-outline.v*.md` 不得当作新大纲继承**——按 doc-gen 第 1.5 步，新大纲必须从 C 源码 + Rust 实现 + `doc_rerank_*` 现推，旧 outline 只作对照，新版本号递增。
4. 02~05 尚无正文、`.design/` 快照已在位，无需清理。

---

## 每轮（k = 1..5）的六个阶段

### A 生成 · 完整跑 doc-gen-prompt 的内层流程

读 `AI-chats/doc-gen-prompt.md`，对 `num = nums[k]` 走完它的第 -1 步到第 7 步（并发隔离 → 预加载 → 选题 → 人工资产对账 → 现推大纲 → 生成 → R1 正确性 → R1 修复反向验证 → R2 卓越性 → 测试 → 提交）。检查点 A / A2 / B1 / B2 照常向用户汇报。

### B 轨迹 · 记录全程 REVIEW 工作流轨迹

原始记录落 `tmp/doc-gen-iterate/round-k/`（命令、完整输出、Skill 实际调用、每个 Gate 的判定与退出码、每阶段耗时与返工次数）。提炼版增量写进 `trace`（见"记账纪律"）。**必须含失败与返工**：只记成功轨迹等于没记——改进点全在返工处。

### C 对比 · 与人工审阅版做教学性量化对账（不许凭口味）

基线 = `draft/{num}-*.md`。两侧都跑同一组读数命令，逐条比：

1. 机器读数（两侧）：`tools/doc-code-density-check.sh draft/{num}-*.md` 与 `tools/doc-code-density-check.sh {num}-{topic}.md`（承载章数、节选码数、诊断命令数、豁免数）；`tools/doc-snippet-extract.sh --count` 对比围栏块数。
2. 八类资产对账（第 1.3 步那张表的**回收验证**）：判为"继承"的每一项，在新正文里**是否真的出现**，出现于哪节。缺失即"口头继承、实际蒸发"。
3. 十条写作底线逐条勾对，每条必答"本处已有（在第几节）/ 仍缺为何可不补"。⛔ 不许用"审计通过、无改动"收尾。
4. 教学性专项（新读者视角通读）：**本轮对比的主问题不是"旧版多少行/新版多少行、谁围栏多"（那些是 presence 读数，新版靠堆徒可过）——而是 GPT 那句："旧版让读者做了哪些认知动作，而新版把哪些动作替读者完成了？"** 对每个被整段分析的 C 函数，逐一对账旧版那四步（看真实代码→逐条解码→发现不明显事实→自己归纳）在新版是否被压成了一句结论；凡被替做 = 认知跃迁压缩 = 本轮必修的**教学性差距**。再查：Ch1 有无机制图与非函数名主语、四类触发器是否各配辨析、有无取舍表/反例推演/认知减负。
5. **差距二分类（改进的唯一入口）**：每条差距必须归到一类——
   - **规则已要求但未执行**（执行退化）→ 靠加门/加证据要求解决；
   - **规则未覆盖**（规则缺口）→ 需新增条目，属**待裁决**，写进 `trace` 交用户拍板，不擅自动规范源。

### D 改进 · 改 REVIEW 工作流与 doc-gen-prompt

1. **改动落点分层**：会话模板改 `AI-chats/doc-gen-prompt.md`（无需同步门）；规范源改 `prompt/review-rules/*`（承载门、Gate 证据、文风条款等）；技能改 `prompt/skill/*`。
2. ⛔ **改完规范源必须跑四道门全绿**（AGENTS.md 一致性约定，CI `rules-consistency.yml` 常驻）：
   `tools/generate-derived-skills.sh` → `tools/check-review-rules.sh` → `tools/lint-review-rules.sh` → `tools/diff-trae-skills.sh --only-diff`
3. **反膨胀元规则**（防五轮把规则加爆）：
   - 新检查项一律以 `review-process.md`「检查项注册表」**一行**加入，不新开 Step 章节；
   - 每轮除"加"之外必须做一次"减"的判定：上一轮新增的条目/门若本轮零命中，写**保留 / 软化 / 删除**三选一与理由（带命令与输出）；
   - 按文档长度分档，短文只强制客观可判的阻断项；
   - **反膨胀 ≠ 砍教学**：删减只删重复内容、过程痕迹、越界内容；贴码/图/辨析/对照/诊断不属膨胀。两类信号冲突时以保留教学性为准。
4. 每条改进都要能被第三个人复核：写清"本轮哪个证据暴露了它 → 改哪一处 → 复跑什么能看到效果变化"。**"本轮无可改进"也要带证据**（差距清单为空 + 读数对比截图）。

### E 提交 · git commit

- 五道文档门全绿才提交：`tools/anchor-resolve.sh --check`、`tools/doc-code-map.sh --check`、`tools/doc-style-lint.sh --diff`（零 error）、`tools/doc-ref-check.sh`（需 `AWK=gawk`）、`tools/doc-code-density-check.sh --check`。
- 正文指纹绑定：`sha256sum {num}-{topic}.md` 记进 `STATE.md`，与提交前复算一致（`review-process.md` 0.13）。
- `git add` **逐路径点名**，提交前 `check-staged-inflight.sh` 必须 0；**一个逻辑单元一笔提交**（文档一笔、工作流改进一笔更清晰）；不跳 hook。
- 提交信息沿用仓库既有风格（`docs(...)` / `feat(review-workflow)` / `fix(review-workflow)`）。

### F 回归复核 · 审自己的 commit（不许自证通过）

对刚落的 commit 做一次独立复核（`git log -1` 取号，作为显式 Git 目标）：

1. 优先派 **CodeReview 子代理**（`review_scope=explicit_target`，覆盖完整性/正确性/影响面一次做完）；拿不到就 `full-review` 自评 + **显式标注"同 agent 复核 + 已重放命令"**，consistency < 90% 就写 NOT PASS。
2. 必查清单：
   - 引用的路径/工具/章节号**真实存在**（逐条 grep 或 `anchor-resolve --check`，不许凭记忆写 `§某条`）；
   - 新条目不与既有条目矛盾、不重复、不越界削弱任何真值校验；
   - 形近字重扫零命中（本项目稳定失败模式；⚠️ **回改错字会引入新错**，且**编辑工具替换项 original 与 new 相同会返回成功却零改动**——所以完成判据是重扫零命中，不是工具返回 success）；
   - 改过规范源则四道门复跑全绿；
   - DoD 达成、正文指纹一致。
3. 复核发现问题 → 当场修（一次一条）→ 重新提交 → 重新复核，直到干净才进下一轮。复核结论写进 `trace` 增量。

---

## 记账纪律（只写增量，防日志膨胀）

- `trace` 每轮一节，编号连续；**追加前自检标题行数必须为 1**（`grep -c '^## Round k'` > 1 即本轮是重复追加）。本项目有过一轮修复日志被复述 32 次、膨胀到 6.8 万行、九成是重复的事故。
- 禁止把已知内容再抄一遍以求完整；引用而非复述。
- 中间产物带作者后缀且互不引用（多模型独立扫同一目标，交集=可信、差集=需复核、矛盾=待裁决）。

## 检查点与停止条件

- **默认串行自主推进**：每轮 F 阶段干净后自动进下一轮，并向用户汇报本轮「教学性差距条目 + 工作流改进 + commit 号」。
- **必须停下回报**：任一门 FAIL 且无法当场修；`check-staged-inflight` 拦下；与 todo-fix 线程在同一路径撞车；qemu 串口被占；出现属"规则缺口"的改进需求（要用户裁决）。
- **停止迭代**：某轮新增可执行改进为 0 且差距清单连续两轮为空 → 视为工作流已收敛，提前交付结论，不再凑满五轮。

## 第 5 轮收尾必交的跨轮汇总

1. 01~05 五篇的最终读数 vs 各自 draft 基线（承载章/节选码/诊断命令/围栏块数一张表）。
2. 五轮累计的工作流改动清单：每条标**命中次数**；从未命中的条目列为删除候选。
3. 仍属"规则缺口、待裁决"的遗留项，以及被证伪的假改进（加了但没用的门）——诚实列出，不为收敛粉饰。
