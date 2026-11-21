# 出版标准与零欠账（V3.7，2026-11-21 用户裁决）

> **本文件优先于**：`review-evals.md` §4.0f（OPEN-DEBT 出口）、§4.1a（② 转出形态）、
> §4.1b（P3 / suggestions）、§6A G3（非阻断 backlog）、G9（⑥ 转下一设计批）。
> 上述条款中任何「延期 / 转出 / 下一批 / 建议级不阻断」的出口，自本文件生效日起**一律关闭**；
> 冲突以本文件为准。
>
> **用户原话锚点**：「代码和文档，都需要达到出版标准才可以的。」「以后不允许 defer 了，
> defer 就是给返工开窗。（也许可能个别 defer 延迟，那也必须经过我人工同意。）」

---

## 一、文档出版标准（DP-1..DP-6）

> **标号的定位（2026-11-21 用户重申）**：标号（D-061 / I-29 / R36 / F-R34-01）**完全服务于
> 项目管理，对读者没有任何意义，是中间结果**。标号引用的内容若有教学价值，**只能**改写成
> 自包含的对比段——「如果这样做，哪里不好，所以这里直接这样做」；**绝不允许出现历史史书**
> （「最初…后来删除」「已废弃」）。DP-1 与 DP-5 是这条的机械判据，DP-5 的反事实豁免是唯一
> 合法教学形态。

正式篇目 = `rewrite-notes/<stage>/NN-<slug>.md`（排除 review / todo / design / forensics /
evidence / backlog / history / learn / brief / note 等辅助件）。

| 编号 | 规则 | 判定 |
|---|---|---|
| **DP-1** | **不出现过程编号**：`D-nnn`、`I-n`、`F-Rnn-nn`、`DESIGN-*`、`Rnn`、`Tn`、`Phase X` | 命中即 FAIL。编号对读者没有意义；需要指代设计时用设计名称本身 |
| **DP-2** | **不出现提交指纹**：散文里的 7–40 位十六进制串（代码块与 `0x…` 字面量豁免） | 命中即 FAIL |
| **DP-3** | **不引用中间产物**：`.design/`、`meta/`、`runs/`、`DECISIONS.md`、`OPEN-DEBT`、`prompt/`、`tmp/`、`coordination/` | 命中即 FAIL。允许：正式文档互链、`os/…` 代码路径、`minix3/…` C 源路径 |
| **DP-4** | **不出现「编号图例」** | 命中即 FAIL——图例本身是 DP-1 违规的产物，不是解法 |
| **DP-5** | **不出现迭代历史**：旧版/旧形态/旧实现/原实现/前版/原先/最初/曾经/后来/过去/当年/当初/删除前/已删除/已被删除/不再使用/改成/替代/替换为/升级为/废弃/放弃/回退到/修复前/已修复 | 命中即 FAIL |
| **DP-6** | **不出现待办与临时疑问**：`TODO` / `FIXME` / `XXX` / 待办 / 待定 / 待确认 / 待核实 / 待补充 / 疑问 / 存疑 | 命中即 FAIL。**agent 不得在正式文档里记录 TODO**：待做事项进 `meta/coordination/todo-ledger/<stage>.yaml`；有教学价值的替代方案写成设计决策对比段（「如果采用它会怎样，所以不采用」），不是 TODO |

**DP-5 的唯一豁免形态：反事实设计讨论。** 历史若真有教学价值，必须改写成：

```markdown
如果把四个位置实参直接传下去会怎样：两个同类型参数交换后仍能编译，故障表现为
「看似合理的坏结果」，所以这里用一个按字段命名的结构体。
```

判定：同一句含「如果 / 若 / 假设 / 可以设计 / 把 X 改成 Y 会怎样」**且**含
「会 / 不会 / 不能 / 反而」；不含时间副词（最初/曾经/后来/过去/当年/当初）与已完成标记
（已删除/修复前/已修复）。也可用显式标记 `<!-- publication-ok: counterfactual -->`。

**DP-6 与用户批注**：用户本人可以在正式文档里留临时笔记/疑惑，但**只能**放在
HTML 注释块内（`<!-- user-note: … -->`）——渲染后读者看不见，检测器也豁免它。**agent 不得
写这类注释**；可见的 TODO/疑问一律视为 agent 产物，命中即 FAIL。若笔记需要长期保留，放
`tmp/` 或对应的 `-notes.md`（辅助件），不要放正式篇目。

**检测器**：`python3 tools/doc-publication-check.py <doc>`（零容忍；不带基线）。
规则实现见 `tools/doc-publication-check.py` 的 DP-1..DP-6 与 `--self-test`。

---

## 二、代码出版标准（CP-1..CP-6）

生产代码 = 非 `#[cfg(test)]` 区段。

| 编号 | 规则 | 判定 |
|---|---|---|
| **CP-1** | 无占位实现：`todo!` / `unimplemented!`；`unreachable!` 必须有现场理由注释（同行或上一行 `// SAFETY:` / `// justified:` / `// unreachable because`） | 命中即 FAIL |
| **CP-2** | 无静默抑制：`#[allow(dead_code)]` / `#[allow(unused…)]` | 命中即 FAIL |
| **CP-3** | 注释里无过程编号：`D-nnn` / `I-n` / `F-Rnn-nn` / `Rnn` / 提交指纹 | 命中即 FAIL |
| **CP-4** | 注释里无迭代叙事：formerly / previously / originally / was removed / no longer / 旧版 / 最初 / 曾经 / 后来 / 删除前 / 已删除 / 不再 / 改成 / 替代 | 命中即 FAIL |
| **CP-5** | 注释里无未完成标记：TODO / FIXME / XXX / HACK | 命中即 FAIL |
| **CP-6** | 无被禁测试：`#[ignore]` 且无同行理由 | 命中即 FAIL |

**为什么代码也要这条**：读者读代码时，`// v10 (D-062): the value is derived, not received.
It was a wire field` 这样的注释只告诉他仓库的历史，不告诉他现在的契约。契约应写成
「该值由 `kern_virt_base + kern_size` 派生」，而不是「以前是 wire 字段」。

**检测器**：`python3 tools/code-publication-check.py <files…>`（零容忍；不带基线）。
规则实现见 `tools/code-publication-check.py` 的 CP-1..CP-6 与 `--self-test`。

---

## 三、零欠账（禁 defer）

1. **任何发现必须在当轮修完。** 以下出口自本文件生效日起全部关闭：
   - `②` 转出（含「用户批准的延期」作为常规出口）；
   - `⑥` 设计项转「下一 design batch」；
   - `suggestions.md` 的「待评估 / 随 batch 评估 / 登记不实施」；
   - `OPEN-DEBT.md` 作为「下一批再做」的登记簿；
   - §6A G3 的「非阻断 backlog 不参与收敛判定」；
   - §6A G9 的「⑥ 登记到下一个设计批」。
2. **唯一合法例外**：`DECISIONS.md` 有用户亲笔的 `defer-approved` 判决行，且同时写明
   ① 日期；② 范围（具体条目 ID，不接受「本批」）；③ 到期条件（什么事实出现就必须做）；
   ④ 不做的后果。缺任一项 = 未闭环。
3. `FP` 仍然合法（误报必须附源码/测试/文档原文反证）。
4. `suggestions.md` 只允许收**用户明确拒绝**的项（对应 `WONTFIX` 判决行）；不存在
   「建议级」这个中间态。
5. **设计项（⑥）必须在轮内实施**；实施不了的，就是本轮不收敛，而不是登记后关闭。
6. **SUGG 必须在轮内清零。** 读者/设计的 P3 建议不阻断过程，但**阻断 CONVERGED**：
   轮内能修的直接修；不能修的必须升级为正式 finding 并给出处置。不允许「本轮报收敛、
   SUG 留在 suggestions 无人问津」。**V3.7 起 SUGG 写入 TODO 账本**
   （`severity: P3`、`kind: design`），`suggestions.md` 只作历史归档。
7. **待裁项必须阻断。** 当前篇存在「即使有代裁授权、agent 也无法决断」的事项时，必须写进
   TODO 账本并标 `blocked: true` + `blocked_reason`；**有 blocked 项时任何轮不得 CONVERGED**，
   必须停下来问用户。不允许把待裁项写成「已批准待实施」或「随下一批」。

---

## 四、顺序迭代

用户原话（2026-11-21）：「当迭代 D01 时，那意味着所有文档和代码，它都能修改，直至它完全满足。
当迭代 D02 时，意味着，除过 D01 的修改，需要我人工干预并通过，后序的任何文档和代码，它都能修改。」

据此定义**可改区**与**冻结区**：

1. **当前篇及其之后的全部文档与代码 = 可改区。** 迭代 D01 时，D01..D0n 的文档与全仓代码
   都可修改（不限于 `related-code.txt`）；迭代 D02 时，D02..D0n 与全仓代码可改。
2. **已获用户批准/已 FREEZE 的前序篇 = 冻结区。** 迭代 D02 时 D01 是冻结区；改动它需要
   用户人工批准（`DECISIONS.md` 判决行），改后必须重跑该篇出版门 + 五门，并在 FREEZE
   追加回归节。
3. **当前篇 CONVERGED 之前不得开下一篇。**
4. 当前篇的判定顺序：先清 DP/CP（出版门零命中）→ 再跑六角色面板 → 再修发现 → 再跑出版门
   → 全绿才算 CONVERGED。
5. **为什么允许改「后面的」文档与代码**：D01 迭代中发现的契约变化会波及后续篇；禁止改会
   制造新的「已写死但已过期」的后续文档——那正是返工的来源。顺序迭代的约束只针对
   **已批准的前序篇**，不针对未来。
6. **Review 期间代码冻结（V3.8，用户 2026-11-21 裁决）**：当前篇进入 Review（面板 survey）
   后，其关联代码（`related-code.txt`）冻结：**面板 survey 与 certify 阶段不得写 `os/`**。
   本轮台账内已判定的修复（L3/L3.5）是唯一常规例外；每批修复后必须重设基线（提交号 +
   文档行数/sha256 + 测试计数）并重跑受影响的机械门。**设计批级改动**（跨 crate 的 `wire`
   版本、所有权/借用模型、ABI 布局等）不在轮内实施：中止本轮，回到 Manuscript 阶段落地、
   重设基线，再重新开轮。`os/` 的无关改动一律判违规。
7. **为什么冻结**：D01 的 109 次 post-freeze `os/` 改动是三次假收敛的直接原因——文档在追一个
   正在施工、施工图还在改的工地。冻结把「写作」与「验收」分开，是章节级收敛的前提。

---

## 四点五、TODO 单一账本（V3.7）

1. **规范源** = `rewrite-notes/meta/coordination/todo-ledger/<stage>.yaml`（每 stage 一个，入库）。
   每条字段：`id/stage/doc/title/status/severity/kind/origin/evidence/defer`；v2 追加
   `refs{code,docs,tests,decisions}`、`relations{blocked_by,duplicates,supersedes}`、
   `lifecycle{created,updated,closed_by}`。**不存行号**：代码用 `path:symbol`，文档用
   `review-units` 的单元 ID（`ch 3.4.2`），行号由 `todo-ledger.py check` 运行时解析。
   `origin_file/origin_line` 是来源快照，不是活引用。open/blocked 项至少有一个**强 ref**（code `path:symbol`、docs `path#unit`、test 或
   decision）；仅路径的 ref 只算弱引用。`--strict-refs` 会对缺口 exit 1，但在迁移覆盖率未达 80% 前**不把它作为
   CONVERGED 的阻断门**（只出覆盖率报告）。Phase 2 门槛按 ref class 分档：`coordination`/`scope: stage` 单列（允许 `decision` 或
   `blocked_reason`）；其余 `code`/`doc` 强 ref ≥80%；`design`/`arch` 允许 `docs#unit` 或
   `decision`。coordination 另跑 `check --scope coordination --fail-on-open` 项目级门。
2. **TODO 载体（2026-11-21）**：01/02 的 `01-todo.md`、stage `todo.md`、`smp_todo.md`、
   `endpoint_todo.md`、`draft/*`、`archive/*` 的 open 项已迁入 YAML 并删除旧文件；其余
   stage 的 `todo.md` 尚未迁移，迁移前仍是输入，新发现一律写 YAML 账本。closed 历史靠
   git 历史 + 每条 `origin_commit` 追溯。
   **轨迹制品（`OPEN-DEBT.md` / `suggestions.md` / `follow-ups.md`）原位保留**，只迁其中的
   open 项，不删除——它们是 `eval-trace-check.py` 的证据链。
   **外部线程执行过的清单（D-107）已按用户确认收口**；两条哨兵已转 `closed`。
3. **新发现的 TODO 一律写入账本**，不得再往旧 markdown 追加。
4. **门**：`python3 tools/todo-ledger.py check --doc <doc> --fail-on-open`——当前篇
   `status: open` 必须为 0；`closed` 必须有证据；`defer-approved` 必须有四要素。
   `--doc` 只计 `scope: doc` 行；`scope: stage` 行用 `check --scope stage --fail-on-open`
   单独检查，不因同 stage 其它篇的 stage 级债务阻断当前篇。
   引用漂移检查：`check --strict-refs` 会解析 `refs` 的文件/符号/文档单元/测试/判决行，
   并检查 `relations` 的悬空 ID 与环路；查询用
   `todo-ledger.py query --file/--symbol/--doc-unit/--status`。
   **跨阶段账本**：`check --scope coordination --fail-on-open` 作为项目级门，不计入单篇
   G12，但项目收尾时必须为 0——否则 coordination 的 open 会再次成为隐藏欠账。
5. **反假收敛**：旧 todo 文件（已删除）里的「已批准待实施」不再算闭环——它必须在账本里是
   `open`（继续做）或 `defer-approved`（有你的亲笔四项）。这是「CONVERGED 但 todo 还有」
   的制度性修复。

## 五、收敛重定义

`CONVERGED` 自本文件生效日起表示**出版就绪**，必须同时满足：

- G1–G9（`review-evals.md` §6A，其中 G3/G9 按本文件收窄）；
- **G13 教程 rubric**：`tools/tutorial-rubric-check.py <当前篇>` 零缺项（TR-1..TR-6）；
- **G14 单元账本**：`tools/review-units.py check --doc <当前篇>` 与 `audit --doc <当前篇>` 退出 0，
  全部单元 frozen、`reopened=0`、`stale=0`、T3 通过（evidence 含可重放 token）；旧 FREEZE 篇目（`runs/*/FREEZE.md`）不追溯；
- **G10 文档出版**：当前篇 `doc-publication-check.py` 零命中；
- **G11 代码出版**：当前篇关联代码 `code-publication-check.py` 零命中；
- **G12 TODO 账本**：`todo-ledger.py check --doc <当前篇> --fail-on-open` 通过
  （open = 0 且 blocked = 0——待裁项存在时禁止收敛）；
- **零欠账**：`OPEN-DEBT` 无 `[OPEN]`，且无任何未获 `defer-approved` 的 ②/⑥/SUGG。
- **卓越性缺口**按 G2 的 P1/P2 入 Issue List 并当轮处置；不另设卓越性门槛。

G10–G14 证据写入 scan.md 的 `gate-evidence-G10`..`gate-evidence-G13` 围栏块（工具名 + 0 命中/exit 0 + 计数），由 `tools/verify-check.py --self-check` 强制；缺块即 Gate 0 FAIL。

`ROUND-CLOSED` / `CONVERGENCE-CANDIDATE` 不再等于出版就绪；它们只是过程状态。
