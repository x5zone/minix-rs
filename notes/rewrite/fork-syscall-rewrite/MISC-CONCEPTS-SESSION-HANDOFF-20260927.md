# MISC-CONCEPTS-SESSION-HANDOFF — 知识点扫描线（电脑迁移交接）

> 体例：账本式交接件——行内日期为本体例惯例（文风检查的日期规则按 misc_concepts.md 5.18 分流处理）；本文只覆盖 misc_concepts 知识点扫描线，NK4-C 线交接见 `NK4C-WORKLOG.md` 顶部与续-74 commit（f7739a18f），pattern-gate 线见 786478d13 与 `PATTERN-GATE-SESSION-HANDOFF-20260927.md`
> 交接时刻：2026-09-27 晚；知识库本体已入库——c63d4ae6d（80 条）+ 82c12c4b8（并行 riscv 收口线追加 5.19，现共 81 条）
> 本文 + misc_concepts.md 即本线的全部持久状态（AI 会话历史不随仓库迁移）

## 1. 任务定义（常设任务）

用户指令："从 FIXLOG、WORKLOG、REVIEW 账本等文件中扫描并发现模式，萃取值得记录的知识点（易错点、重点、方法论），写入 `notes/rewrite/fork-syscall-rewrite/misc_concepts.md`（01-stage-kernel 同层级）。"——代码在迭代、日志在追加，故为**增量式常设任务**：每次执行时只扫基线之后的变化，续作节点由用户以"知识点扫描"类指令触发。

## 2. 交付物现状

- 文件：`notes/rewrite/fork-syscall-rewrite/misc_concepts.md`——**81 条 / 约 660 行**，六章结构（1 构建与工具链 / 2 真机调试与取证 / 3 内核与架构易错点 / 4 测试与防回归 / 5 流程与协作纪律 / 6 知识库指南针）。
- 每条三段式：机制/根因 → 正确做法 → `> 来源：` 锚点行；头部 `创建`/`重写` 元数据行记录各轮增量。
- 质量门：`bash tools/doc-style-lint.sh <该文件>` 退出码 0（每轮写入后必跑）；禁词 grep 电池（SL-3/5/7 族）全过。
- 入库 commit：c63d4ae6d（80 条入库）、82c12c4b8（并行线追加 5.19 多模式 grep 负结论纪律）。

## 3. 本会话三轮工作记录

| 轮次 | 日期 | 动作 | 产出 |
|---|---|---|---|
| 首轮成文 | 2026-09-27（凌晨批次，头部行写 2026-09-23，时间线以 git 为准） | 侦察 2 agent 定源与格式 → 3 萃取 agent（46 候选）→ 锚点全量 grep 验证 → 成文 | 37 条 / 308 行 |
| 增量一 | 2026-09-27 | 侦察 2 agent（基线后变化盘点）→ 3 萃取 agent（40 候选，两次并发超限重试）→ 锚点验证 → 扩展 5 条旧条 + 追加 | +31 条 / 554 行 |
| 增量二 | 2026-09-27 | 用户核对覆盖面后补扫：N 系漏读 7 文件 + `.zcode` 记忆/计划层 → 2 萃取 agent（22 候选）→ 锚点验证 → 扩展 5 条 + 追加 | +12 条 / 650 行 |
| 并行线 | 2026-09-27 晚 | riscv 收口线会话追加 5.19（非本会话） | 81 条（现状） |

## 4. 扫描源地图（续作不要重新侦察）

**已深扫并吸收（核心层，按密度排序）**：
1. `NK4A-TODO.md`（§5 十三条真机陷阱、§7 命令、§8 取证循环、§9 禁止事项）
2. `NK4C-WORKLOG.md`（S0→§1.119，截至 5237 行——**之后有 +2911 行待扫，见 §7**）
3. `NK4B-WORKLOG.md`（P1/P3 M3.1–M3.4/M4.4 各节教训与上交裁决）
4. `.review/zcode/edge1/FIXLOG.md`（迭代 11–33 全部，2640 行）
5. `NK4C-REVIEW-REPORT-20260923.md`/`-R2`/`-20260927-R3.md`（三轮评审账本）
6. `NK4A-QWEN-WORKLOG.md`（页故障停滞取证弧线）
7. `NK4C-OPENING-PROMPT.md`/`NK4C-RESUME-PROMPT.md`（任务书式约束与探针纪律）
8. `NK4A-HANDOFF-STATUS.md`、`NK4-REGRESSION-REVIEW-20260922.md`+PART2、`NK4B-TODO.md`、`NK4A-REVIEW-REPORT.md`
9. `PATTERN-SCAN-REPORT-20260923.md` + `tools/pattern-gate.sh`（模式沉淀层）
10. `edge_todo.md`（含 NK4-OPEN 收拢节）、`edge4.md`/`new_edge4.md`（认领板判例）
11. `.review/zcode/edge2/3/4/FIXLOG`、`edge3/FIXLOG_archive.md`、`.review/trae/FIXES-LOG`、`E-KERNINFO/fix-status.md`
12. `.zcode/cli/memories/` 跨会话记忆（两处项目哈希——仓库 `.zcode/` 仅 2 条，其余在 `~/.zcode/` 同名树下；事故型条目密度极高，已吸收 12 条）
13. `WORKFLOW-META-REVIEW.md`、`review-rules-meta/fix-status.md`（元层）、`new_todo_{deepseek,HY4,muse}.md`（多模型独立诊断）、`AI-chats/cross-review.md`（设计论证）
14. `.zcode/plans/` 34 份会话计划书（低密度，仅 1 条硬发现；续扫只看各份"现状对账/关键发现/风险"段）

**确认无增量**：README.md（纯索引）、NK4A-QWEN-OPENING-PROMPT.md（与 NK4A-TODO §5 全重合）、NK4B-OPENING-PROMPT（要点已并入交接条目）、edge2/3/3_archive/4 修复日志、trae/codex/claude 域、new_edge1-3、new_todo×5、AI-chats 其余。

**只做指针不重复萃取**：`prompt/review-rules/review-patterns.md`（85 模式分类学权威，misc_concepts 第 6 章指南针指向它）。

## 5. 续作协议（增量扫描七步）

1. **定基线**：`stat -c '%y' notes/rewrite/fork-syscall-rewrite/misc_concepts.md`（或看头部最后一行 `重写` 的日期）。
2. **增量侦察**：`git log --since=<基线> --oneline` 按目录分布；`find notes/rewrite .review .zcode -name '*.md' -newer <基线>` 找变化文件；区分大增量/小增量/零变化，**零变化不重扫**。并行会话可能已推进仓库（本轮就遇到），以 git 现场为准。
3. **并行萃取**：发 3 个 Explore agent（read-only）分片；大文件先 `grep -n '^## \|^### '` 建节清单，再用"教训|根因|更正|翻案|误诊|证伪|自检|定性|陷阱|假说|撤回|判别|防回归"定位高密度节精读，流水账跳过。统一 schema：`### 标题（一句话断言式）/ 分类 / 机制根因 / 正确做法 / 来源锚点 / 密度评级`。agent 可能报"user concurrency limit exceeded"——逐个重发即可。
4. **锚点验证**：所有代码锚点（path:符号）与文档节名逐个 grep 实证后才写入；查不到的改描述性引用或标注。
5. **去重合并**：新证据充实旧条 → 就地扩展（正文+来源行）；全新主题 → 追加新条（**编号顺延现章内编号，不重排不插号**）；确认与既有条目重复 → 只报不写。
6. **写入**：头部追加一行 `> **重写**: <日期>（增量N：…新增 X 条、扩展 Y 条，共 Z 条）`；正文新条保持三段式。
7. **验证与落账**：lint 退出码 0 + 禁词 grep 电池全过 → `git add <明确路径>` + commit（一逻辑单元一 commit，禁 `-A`）。

## 6. 硬约束（违者返工）

- **文风**：先读 `.agents/skills/style-bible/SKILL.md`。正文无日期（仅 `创建`/`重写` 行豁免）；无评审工具术语（scan.md/STATE.md/VERIFY-CHECK/模式 N/Gate X/反查维度）；无修复史叙事词（旧版/已修复/修复前/原先等——用"含缺陷的实现/改动前"替代）；禁黑话压缩简写。
- **体例**：受跟踪文档**不引用 gitignored 修复日志的条目序号**（会撞号且不可移植）——来源行用"文件路径 + 可 grep 的条目标题关键词"或"迭代 N"；`.design/`、`tmp_design_and_todo/` 绝不引用；跨会话记忆锚点写"跨会话记忆 `<文件名>.md`（见 6.3）"。
- **内容门槛**：只收有真机/测试实证、跨任务可复用、违反有实际代价的条目；排除流水账、状态记录、纯流程条文（无事故判例不收）。
- **环境**：文档工作无需 cargo；lint 是唯一机器门。git 纪律：明确路径 add、禁 push、禁 reset/--force（完整规则见 `new_edge4.md` §1）。

## 7. 待扫增量清单（截至本交接，按优先级）

1. **`NK4C-WORKLOG.md`：5237 → 8148 行（+2911，最大头）**——续-68…续-74：VM 用户栈腐蚀竞态十二探针取证弧线、idle() 补 AP stop_local_timer 修复（续-72）、schedctl 钳制与 x86_64 -smp4 首稳达 rc marker（续-73）、迁移交接件（续-74）；重点抓各轮"翻案/定性/教训"段。
2. **`riscv-reviewlog.md`（新面孔）**——riscv 移交收口线引入；5.19 已收其一条（fence.i 假负结论），其余待扫。
3. **`NK4C-BUG-AARCH64-VEC-CAP.md`（151 行）/ `NK4C-BUG-AARCH64-VEC-CAP-GLM.md`（306 行）**（untracked 新文件）。
4. **`PATTERN-GATE-SESSION-HANDOFF-20260927.md`（untracked，116 行）** + 入库版 786478d13（pattern-gate 线 C-61/62/63 状态与教训）。
5. 续-68…74 的**提交信息本体**（git log，翻案/取证叙述密集）。
6. 常规复查：edge1 FIXLOG（交接时 2640 行未变）、edge_todo/new_edge4 状态行、`.zcode` 记忆新增条目。

## 8. 有意未吸收的盲区（在跨会话记忆中在案，用户点名才扫）

- 文档重排线：九份 stage 重建蓝图（VFS/DS/MIB/Runtime/FS/Net/Commands/Drivers R 相）、notes 迁移 glm 方案、VFS 对话臂模板（S12）——属文档重建域，非实操内核知识点。
- 裁决型：OQ-N3 console 裁决（方案 B 内核中转+串口先行，已落地）、NS12 bin seam 状态（65 bin 批交付）。
- 技术缺口：riscv 线 sscratch/sfence/split_huge 三簇（NK4C riscv 线 future，随 riscv-reviewlog.md 增量一并看）。

## 9. 新机注意事项

- misc_concepts.md 有 8 个条目的来源行引用"跨会话记忆"（1.5–1.8、5.15、5.18 及 6.3 指南针）——记忆文件在 `~/.zcode/cli/memories/projects/project-9c6ac911fb57ac45/memory/` 与 `project-9cac911fb57ac45/memory/`（注意第二个哈希少一个 c）。**新机若不迁移用户主目录，这些锚点文件不存在**，但相关事实已完整吸收进正文，仅深溯源时需要；可把两目录拷到新机同路径恢复。
- 仓库 `.zcode/cli/memories/` 只随仓库走 2 条（fmt 漂移、测试内存铁律）。
- 本线不依赖 docker/target/rust 工具链，新机只要 git + bash + 可跑 `tools/doc-style-lint.sh`（纯 shell/awk）即可开工。

## 10. 续作 prompt（保存为文本文件，新机新会话原样粘贴）

见下方代码块内容（与本文件 §10 同步维护）。

---

### §10 续作 prompt 全文

```text
【任务】minix-rs 仓库"知识点扫描"线——新机续作

你在 minix-rs 仓库（Minix3 内核 Rust 重写项目，路径按实际挂载）。此前会话建立了实操知识点汇总库
notes/rewrite/fork-syscall-rewrite/misc_concepts.md（81 条，doc-style-lint 零命中），
并留下完整交接档案 notes/rewrite/fork-syscall-rewrite/MISC-CONCEPTS-SESSION-HANDOFF-20260927.md。

第 0 步：读交接档案全文（任务定义、源地图、续作协议、硬约束、待扫增量清单都在里面），不要重新侦察。
第 1 步：按档案 §7 待扫增量清单开始增量扫描（基线 = misc_concepts.md 头部最后一行"重写"的日期）。

标准流程（七步，详见档案 §5）：
1. 定基线（mtime 或头部重写行日期）
2. 增量侦察：git log --since + find -newer，区分大增量/小增量/零变化，零变化不重扫；注意并行会话可能已推进仓库
3. 并行发 3 个只读萃取 agent 分片扫变化文件（大文件先 grep 节清单再用教训类关键词定位精读）；
   统一 schema：标题断言式/分类/机制根因/正确做法/来源锚点/密度评级；
   agent 报并发超限就逐个重发
4. 所有代码/文档锚点 grep 实证后才写入
5. 去重合并：新证据扩展旧条（就地），全新主题追加新条（编号顺延不重排），重复的只报不写
6. 写入：头部追加一行「> **重写**: <日期>（增量N：…新增 X 条、扩展 Y 条，共 Z 条）」
7. bash tools/doc-style-lint.sh <文件> 必须退出码 0 → git add 明确路径 + commit（禁 -A、禁 push）

硬约束（详见档案 §6）：正文无日期（仅创建/重写行豁免）、无评审工具术语、无修复史叙事词、
不引用 gitignored 修复日志的条目序号（来源行用"文件+可 grep 的条目标题关键词"）、
不引用 .design/ 与 tmp_design_and_todo/、先读 .agents/skills/style-bible/SKILL.md。

高密度源排序：NK4 系 WORKLOG/任务书 ≈ .review/zcode/edgeN/FIXLOG ≈ NK4C-REVIEW-REPORT 系列
> PATTERN-SCAN-REPORT/pattern-gate > edge_todo/edge4 认领板 > .zcode 跨会话记忆（~/.zcode 与仓库
.zcode 两处）> .zcode/plans（低密度，只看"现状对账/关键发现/风险"段）。

用户触发语（示例）："知识点扫描"（增量扫 misc_concepts.md）；"扫描 XX 文件"（定向补扫）。
```
