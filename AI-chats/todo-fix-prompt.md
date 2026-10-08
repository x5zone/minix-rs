# 代码 TODO 修复与台账维护（可复用 prompt）

> 用途：从待办台账选**一项** TODO 完整修复，做正确性回归 + 测试，再把 `coordination/` 台账维护到位并提交。
> 用法：填好参数，整段发给 agent（Claude Code / Codex / OpenCode / Qoder 等）。
> 依据：技能 `prompt/skill/cmds/todo-fix/SKILL.md`（三段式、一次一条）；台账协议 `rewrite-notes/coordination/TODO-LEDGER-INDEX.md`（六值状态词表 §3、避让与收尾校验 §6）；目录职责 `rewrite-notes/coordination/README.md`。

## 参数

```
tool    = opencode   # claude / codex / trae / qoder，决定 .review 状态目录
todo_id = <可选：指定条目编号>  # 留空则由 agent 从 TODO-LEDGER-OPEN.md 选一项可立即开工的
```

**目标**：修复一项 TODO，做到「正确性回归 + 测试保障 + 台账一致 + 一笔提交」闭环。

> **三条硬规矩先说清**（避免返工，均来自规则原文）：
> - **一次只修一条**：`todo-fix` 明确"一次只修一个，批量需用户显式说"；不顺手修旁边的 TODO（只记录不执行）。
> - **DEFERRED 不是修复**：把 TODO 改成 DEFERRED 充数 = P0-process-violation；降级必须给"依赖未解除"的具体论证并登记。
> - **修复顺序 P0 → P1 → P2**：卓越性重构只在正确性坐实后做，发现新 bug 退回正确性线。

---

## 第 -1 步 · 并发隔离前置（worktree，动任何文件前先做）

本仓库正多 agent 并发（一个跑 doc、一个跑 todo 时共用同一棵树）。直接在共享主树开工是 `C-35` 事故判例——一次 `checkout`/`reset` 就销毁其它会话未提交在制品。**用 `tools/claim.sh`，别手搓 `git worktree`**（依据：`coordination/CONCURRENT-WORK-PROTOCOL.md` §4、§11）：

- **领取**：`tools/claim.sh claim <条目ID> <owner>`——建排他锁分支 `claim/<ID>-<owner>` + 自动建 `.wt/<id>-<owner>/` 专属树；分支名全局唯一，被别人领走就直接失败，换条目、不等不抢。
- **只在自己树里干**：`cd .wt/<id>-<owner>/`；主树内禁 `checkout`/`reset --force`；换任务前 `tools/claim.sh verify` 自检位置与未提交数。
- **归属线自查**（协议 §2）：按所选条目落在的线确认领地（一线=内核/引导/真机、二线=`os/libs/`枚举 crate+`os/drivers/`、三线=`os/servers/`+`os/fs/`+`os/net/` 等）；只改所有权清单内路径，越界先在 `new_edge4.md` 登记“跨界改点”。
- **QEMU 必须串行**：与 doc agent 同跑真机门会互污染串口读数；跑前 `tools/claim.sh list` + 直接问，确认无人占用。
- **台账是最大写热点**：两个 prompt 都写 `TODO-LEDGER-OPEN.md`/`DONE`。各分支各改，合回 `rewrite` 时在此处集中解一次 merge 冲突（优于同树静默覆盖）；若多人同时改，尽量让其中一方 rebase。
- **收尾合回**：自己树内一笔提交→合入 `rewrite`→`tools/claim.sh release <条目ID> <owner>` 删树销锁。

---

## 第 0 步 · 预加载 + 避让双查（动手前必做）

- 读台账三件：`coordination/README.md`（目录职责）、`coordination/TODO-LEDGER-INDEX.md`（§1 文件处置、§2 编号归一、§3 六值状态词表、§4 机器判据、§6 避让与收尾校验协议）、`coordination/TODO-LEDGER-OPEN.md`（未完成总账，选目标）。
- 读规范源并按需真调用技能（Skill 工具，"读过/我记得"不算）：`todo-fix`（主）、`test-audit`（测试）；**正确性回归用 `full-review`**；**仅设计级 `SD-*` 条目**才叠加 `code-excellence`；**任何文档改动前置 `style-bible`**。
- **避让双查**（这是并发开发区，别踩别的线程）：`bash tools/check-staged-inflight.sh --list` 与 `git status --porcelain`；落在避让清单里的文件**只读不写**。受保护件：`new_edge2.md` 受保护、`new_edge4.md` 为活跃认领板。
- **禁引已删原件**：54 份被合并删除的文件（各 `NK4C-接续PROMPT-*`、`NK4C-REVIEW-REPORT-*`、`claim-prompt.md`、`new_edge1/3.md` 等）不得引用，需回看靠 git 历史。

## 第 1 步 · 选 TODO（检查点 A：先汇报再动手）

- 选定一条：`todo_id` 指定则用它，否则从 `TODO-LEDGER-OPEN.md` 挑一条**可立即开工**（A 类，不依赖他人裁决）的条目。
- 讲明白它是什么、为什么修（给现状 grep 证据 + 锚点），并判定它属于哪类：代码 bug / 结构债 `SD-*` / 跨阶段 `E-*` / 齐平 `P-*` / 卫生项——据此决定后续要不要 `code-excellence`。
- **检查点 A**：把「目标条目 + 现状证据 + 修复方案（对比 Linux/Redox/OS 理论，优中选优并给理由）」汇报用户，确认后再实施。

## 第 2 步 · 实施（`todo-fix` 三段式的第三段）

- 代码 + 文档 + 测试**一并**改，遵守 `fix-guard`（动手前读±5 行、grep 确认、一次一条、修后留痕）与 translate 防线、测试名对账门。
- KERNEL 约束照旧：SMP+BKL，共享数据 `Arc`+`Mutex`/`Atomic`，禁 `Rc`/`RefCell` 跨 CPU；抽象语义不暴露 CR3/PTE。三档术语 Rewrite/Refactor/[ARCH] 一致标注。
- 若牵动文档正文：文风走 `style-bible`（读者体验优先，纯坐标外移锚点底账，过程痕迹禁入正文）。

## 第 3 步 · 正确性回归 REVIEW + 测试（免引新错）

先正确性、后（如适用）卓越性，顺序不许合并（`full-review` 不做卓越性、`code-excellence` 不修 bug）：

- **正确性回归 `full-review`**（scope 限定本次 diff 涉及的文档/代码）：对照 `minix3/` C 源核行为一致，P0 六类清零；Blocker Gates 附命令+输出证据（L1）。
- **测试专项 `test-audit`**（5 维度）：完备性、测试自身正确性（防"测试代码就是错的"）、冗余、无效、虚构（文档测试描述与代码双向对账）；思考并补上后续保障测试。
- **单元/编译**：`cargo test -p {crate}`、`cargo clippy`（工作区根 `os/`）；重跑受影响 Gate。
- **真机门（涉及内核/启动/多核/三架构行为时必跑）**：按 `os/qemu-tests/README.md` 用 `run_qemu.sh` 做三架构回归；权威编译门配方见 `AGENTS.md`（docker `minix-ci:1.94` + `-m 2g -j 1`）。

## 第 4 步 · 维护 coordination 台账

- **只动两本台账**：完成即在 `TODO-LEDGER-OPEN.md` 翻状态，或搬到 `TODO-LEDGER-DONE.md` §1（代码坐实的 `closed`）/ §2（已裁决执行）。
- **状态用六值词表**：`open` / `stopgap landed` / `partially fixed` / `adjudicated` / `closed` / `dropped-from-ledgers`（"删除即解决"用 `dropped-from-ledgers` 防错觉）。
- **每条状态变更附三件套**（缺一视同未核实）：可重跑的核实命令与读数、核实日期、当时快照提交号。
- **权威分工不许越界**：状态以本台账为准；描述与修法以原账本（`edge_todo.md` / 各阶段 `todo.md` / `STRUCTURAL-DEBT-REGISTER-*`）为准；裁决以 `PD-*` 为准——**不改原账本的论证文字，不改裁决文件**。
- **相关文档由你判断同步更新**：概念/机制类知识进同级 `../concepts/`（如 `capability.md`、`typestate.md`）；历史杂项在 `../misc/`；案卷 `CASE-*`、WORKLOG 等按归属更新，不做无谓改动。

## 第 5 步 · 收尾五道校验（`TODO-LEDGER-INDEX.md` §6）

依次跑并处理输出：

1. `bash tools/todo-reconcile-scan.sh`（先重算机器可判据再写状态；基线失配是复核后改台账，不是改脚本基线）
2. `python3 tools/notes-link-check.py rewrite-notes/`（与上轮基线做集合差，新增断链必须为零）
3. `bash tools/todo-staleness-check.sh rewrite-notes/coordination/TODO-LEDGER-OPEN.md`
4. `bash tools/doc-style-lint.sh --diff`（零 error）
5. `bash tools/check-staged-inflight.sh`

## 第 6 步 · 提交

- 五道校验通过、P0 清零、测试与（如需）qemu 门绿后，**一小步一提交**将本次改动（代码 + 文档 + 测试 + 台账变更）提交。
- 提交信息按项目既有风格；**不跳过任何 hook**。任一门 FAIL、三件套不齐、或未过用户检查点，则**先回报、不提交**。

---

## 完成定义（DoD）

目标 TODO 已按 `todo-fix` 三段式实现（讲明白→讲怎么修→实施），非 DEFERRED 充数；正确性回归 `full-review` P0 清零；`test-audit` 五维度过、`cargo test`+`clippy` 绿、涉及真机行为时 qemu 三架构回归绿；台账已用六值词表翻状态/搬 DONE 且每条附三件套；受影响的 `concepts/`+`misc/`+案卷按判断同步；收尾五道校验全过；一笔提交落地（无跳 hook）。
