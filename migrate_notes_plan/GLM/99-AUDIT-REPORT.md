# notes 目录迁移 · GLM 独立审计最终报告

> **审计方**: GLM（ZCode 会话），未参与迁移、未读取 DeepSeek 审计方（`migrate_notes_plan/DS/`）的任何产物。
> **审计时刻**: 2026-10-07 19:03 ～ 20 时段（+08:00）。
> **协议**: `migrate_notes_plan/REVIEW-PROMPT.md` 第 0～8 项全项执行 + 自加检查 A1～A6。
> **逐项证据**: `01-ITEM-RESULTS.md`（每条判定附命令与实测值），原始输出在 `evidence/`。
> **纪律**: 判定纪律按协议原文——第 1/3/4/6 项任一不成立即 P0。

---

## 结论：**可交付**（P0 = 0，P1 = 0，P2 = 5 项观察 + 3 项未能核对）

本次笔记目录迁移在内容层、跟踪层、引用层、门禁层、编译层五个面上全部通过独立复算；
执行日志与留存文档声称的量化数字经抽查**无一失实**；三处自造故障与两处越界拦截均有如实记录。

---

## P0（内容丢失 / 改错 / 历史断裂 / 门失效）

**无。** 四条一票否决项的独立复算结果：

| 否决项 | 独立复算方法 | 结果 |
|---|---|---|
| 内容一致性 | manifest↔path-map↔磁盘三方行数与集合级对账；2428 条逐条 sha256；333 个内容差异逐行归因到记载动作（321 个由 path-map 派生的合同变换完全解释，12 个文件全部是执行日志记载过的 README 重写与链接重定目标）；冻结区内容变更 = 0 | 通过 |
| 跟踪数守恒 | pre tag 1102 = HEAD 三树 1105 − 3 新建件；映射的 git-mv 目标在跟踪面缺失 0（Phase 2 的 108 个静默退跟踪修复无残留）；e90e4173d 权威口径 1102 × R100、0 A、0 D；2 个 D 形态经 blob 级谱系查证为改名检测极限伪象，非丢失 | 通过 |
| 引用残留 | python3 字节级扫描必改域：白名单外残留 0；冻结区保留计数 evidence=4 / legacy-fork-bak=8 / .design=56 与执行日志逐个相等 | 通过 |
| 门有效性 | 三类正向埋点（坏文风 / 坏链 / 坏锚点）全部被抓到，探针逐一复原且工作树零残留；规则三端一致 + 派生无漂移 + 覆盖率工具新布局可用 / 旧布局可读报错 | 通过 |

## P1（引用漏改 / 文档与现实不一致）

**无。** 必改域零残留、断链零新增（M3 复跑 + 审计方独立新跑 2133 个文件、373 条断链与 after.txt
集合级逐行相等）、锚点基线行数与路径实存性全部吻合、`{stage}`/`{module}` 概念分离在规则源
`prompt/review-rules/review-process.md:403` 落地且残余 65 处 `{module}` 抽样均为 Minix3 模块概念。

## P2（观察与建议，均不阻断交付）

1. **留档笔误**：`migrate_notes_plan/pre-migrate-20261007/00-SNAPSHOT.md` §5.2（第 169 行）记录
   `tools/atf-c-compat/probes/p7.c` 的 sha256 前 12 位为 `b8da7f7b726`，实测为 `b8da7f7b7276…`
   （第 11 位掉一个 7）。tar 卷、磁盘、sha256sum 工具三方读数一致，内容无恙，仅表格单元格笔误。
2. **REVIEW-PROMPT 自身的三处期望值缺陷**（会让后续按此 prompt 重审的人多走弯路，不影响迁移本身）：
   - 第 1b 项期望「一致 2428 / 缺失 0 / 不一致 0」与迁移自身设计矛盾——Phase 3 引用重写
     本就要改 333 个树内文件的内容。正确表述应为「不一致项须逐条归因到 Phase 3/4/7 的记载动作」
     （本审计已代为完成该归因）。
   - 第 2 项 `tar -tzf | grep -c '^notes/'` 把 583 个目录条目一起计入（实测 2536），
     应排除以 `/` 结尾的行（文件条目恰为 2428）。
   - 第 3 项期望「1102」与「只有 R」按 Phase 2 时点书写，未计入 Phase 7 新建的 3 个跟踪文件
     （HEAD 实为 1105）与 git 非精确改名检测上限（累积 diff 呈 1100 R + 5 A + 2 D，
     2 个 D 已用 blob 谱系证伪丢失）。
3. **MIGRATION.md §六「已知悬空引用」未逐条覆盖两处散文路径实例**（链接门管不到散文）：
   - `rewrite-notes/04-stage-pm/draft/mproc-design.md:2654` 的 `rewrite-notes/mp-flags-analysis.md`
     ——迁移前该引用即悬空（真实文件在 `archive/legacy-fork-bak/`），机械改写如实保留了悬空，
     属已登记类别，但未像 `fork-syscall-plan.md` 那样逐条点名；
   - `rewrite-notes/18-stage-commands/doc_rerank_deepseek.md:2184` 的 `rewrite-notes/edge*.md`
     通配——迁移前 glob 能命中 `fork-syscall-rewrite/edge_todo.md`，迁移后该文件在
     `coordination/` 下，通配落空。这是机械前缀替换对「散文里的通配符」的语义极限，
     属迁移前准确、迁移后失准的一例（非链接、不影响任何门）。
4. **relink 留档只保了第二轮**：`tmp/pre-migrate-snapshot-20261007/relink-report.tsv` 现存
   20 条 REWRITE（第二轮）+ 503 条悬空（第二轮口径），首轮 118 条重定基记录被第二轮覆盖，
   「两轮合计 138 处」无法从留档精确复核。其**效果**已被独立覆盖（断链集合对账 0 新增 +
   审计方行级归因），仅记录证据完整性弱点。
5. **02-VERIFY.md 门 M2 取证块未贴实际输出**（`gates.txt` 同样只有期望文字；
   run-gates.sh 的 grep 在 0 命中时输出为空，空即证据但不可读）。本审计已用字节级扫描独立坐实，
   建议后续取证文档对「0 命中」类结论显式写出计数（如 `0 个文件`）。

## 未能核对的项与原因

1. **双 tag 是否已推远端**：本会话 `git ls-remote --tags origin` 无网络输出（连 9 个上游 tag
   也未返回），属网络凭证限制。本地双 tag 存在性与指向已核实；「不推远端」的裁决在本地侧成立。
2. **裁决 4「分阶段交互确认」的交互过程本身**：过程性事实无法取证，凭 12 条裁决表、
   J1/J2/J3 问答记录与裁决 12 的补问记录自证——材料内部一致，无矛盾。
3. **`.claude/settings.local.json` 的字节级冻结验证**：该文件未被 git 跟踪、未入 tar 卷，
   无迁移前基线哈希可比。已验证的部分：迁移提交链未触碰它（`git log pre..HEAD -- <path>` 为空），
   旧路径命中 38（留档）→ 40（现值）的增长符合其冻结语义「新命令会重新走授权」。

## 对执行方遗留观察的裁决回应（02-VERIFY「一项未结观察」）

执行方留给审阅方的宿主偶发失败，本审计在**权威配方**（docker `minix-ci:1.94`、`-m 2g -j 1`）下取证：

- `cargo check --workspace --tests -j 1`：RC=0，零 error。
- 全量 test 在 docker 下**同样出现**执行方宿主记录的现象，且捕获到具体失败对象：
  `minix-driver-rt --lib` SIGSEGV；`minix-ds --lib` 的
  `server::tests::test_sys_kernel_hosted_reports_eio` 断言失败（期望 `Err(5)`/EIO，实际 `Err(38)`/ENOSYS，
  `servers/ds/src/server.rs:1002`）；`minix-is --lib` 两个 hosted 类测试失败；`minix_init` 测试二进制 SIGSEGV。
- 失败对象在 crate 间漂移、单跑稳定（HEAD 与 pre-tag 各 3 遍全过）。
- **pre-tag 源码同配方对照**（`git archive` 导出至 /tmp，不触碰仓库）：全量 `--no-fail-fast`
  test 呈现**逐一相同的失败签名**——`minix-ds` 同一测试 `test_sys_kernel_hosted_reports_eio`
  同一断言（期望 `Err(5)`/EIO，实际 `Err(38)`/ENOSYS，`servers/ds/src/server.rs:1002`）、
  `minix-driver-rt` 与 `minix_init` 同样 SIGSEGV（二进制哈希不同而失败相同，证明崩溃跟随源码
  构造而非某个构建产物）、`minix-is` 同样两个 hosted 测试失败。证据：
  `evidence/item7d-pretag-fulltest.txt`。

**裁决**：维持并**实证升级**执行方「与迁移无关的既有不稳定」结论——该不稳定在迁移前的源码上、
同一权威配方下逐一复现，先于迁移存在，与本迁移无因果。失败集中在 hosted 内核类测试
（E5 宿主半可链接面），失败名与断言值已捕获，供后续独立排查该不稳定本身。

## 审计留档清单（本目录）

| 文件 | 内容 |
|---|---|
| `00-AUDIT-PLAN.md` | 审计计划、边界纪律、方法、自加检查项定义 |
| `01-ITEM-RESULTS.md` | 协议 8 项 + A1～A6 逐项结果（每条判定带命令与实测值） |
| `99-AUDIT-REPORT.md` | 本文件：终局结论 |
| `evidence/item0…item10-*.txt` | 17 份原始命令输出 |
| `audit_item1.py` / `audit_item1c.py` / `audit_a3_tar.py` / `audit_item4_residue.py` / `audit_item8_decisions.py` | 审计自用脚本（可复跑） |

审计对仓库的写入仅限本目录；第 6 项埋点均已复原（埋点前后 `git status --porcelain` 逐行一致）。
