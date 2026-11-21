# 出版标准与零欠账（V3.7，2026-11-21 用户裁决）

> **本文件优先于**：`review-evals.md` §4.0f（OPEN-DEBT 出口）、§4.1a（② 转出形态）、
> §4.1b（P3 / suggestions）、§6A G3（非阻断 backlog）、G9（⑥ 转下一设计批）。
> 上述条款中任何「延期 / 转出 / 下一批 / 建议级不阻断」的出口，自本文件生效日起**一律关闭**；
> 冲突以本文件为准。
>
> **用户原话锚点**：「代码和文档，都需要达到出版标准才可以的。」「以后不允许 defer 了，
> defer 就是给返工开窗。（也许可能个别 defer 延迟，那也必须经过我人工同意。）」

---

## 一、文档出版标准（DP-1..DP-5）

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
| **DP-3** | **不引用中间产物**：`.design/`、`runs/`、`DECISIONS.md`、`OPEN-DEBT`、`prompt/`、`tmp/`、`coordination/` | 命中即 FAIL。允许：正式文档互链、`os/…` 代码路径、`minix3/…` C 源路径 |
| **DP-4** | **不出现「编号图例」** | 命中即 FAIL——图例本身是 DP-1 违规的产物，不是解法 |
| **DP-5** | **不出现迭代历史**：旧版/旧形态/旧实现/原实现/前版/原先/最初/曾经/后来/过去/当年/当初/删除前/已删除/已被删除/不再使用/改成/替代/替换为/升级为/废弃/放弃/回退到/修复前/已修复 | 命中即 FAIL |

**DP-5 的唯一豁免形态：反事实设计讨论。** 历史若真有教学价值，必须改写成：

```markdown
如果把四个位置实参直接传下去会怎样：两个同类型参数交换后仍能编译，故障表现为
「看似合理的坏结果」，所以这里用一个按字段命名的结构体。
```

判定：同一句含「如果 / 若 / 假设 / 可以设计 / 把 X 改成 Y 会怎样」**且**含
「会 / 不会 / 不能 / 反而」；不含时间副词（最初/曾经/后来/过去/当年/当初）与已完成标记
（已删除/修复前/已修复）。也可用显式标记 `<!-- publication-ok: counterfactual -->`。

**检测器**：`python3 tools/doc-publication-check.py <doc>`（零容忍；不带基线）。
规则实现见 `tools/doc-publication-check.py` 的 DP-1..DP-5 与 `--self-test`。

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

---

## 四点五、TODO 单一账本（V3.7）

1. **规范源** = `rewrite-notes/coordination/todo-ledger/<stage>.yaml`（每 stage 一个，入库）。
   每条字段：`id/stage/doc/title/status/severity/kind/origin/evidence/defer`。
2. **旧 todo markdown 冻结为 archive**：`01-todo.md`、各 stage `todo.md`、`OPEN-DEBT.md`、
   `suggestions.md`、`follow-ups.md` 不再作为「当前欠账」的判据；只迁**open** 项，closed 历史
   留在旧文件。当前并发清理结束、D01 CONVERGED 时执行冻结（加归档头）。
   **外部线程正在执行的清单（D-107 的 D01 13 项 / D02 14 项）按历史 todo 处理，不逐条收集**；
   只需为每篇留一个 `blocked: true` 哨兵（`D01/D02-EXTERNAL-D107`），线程完成并经用户确认后
   关闭哨兵。这样既不重复收集，又不会在外部工作未完成时误判收敛。
3. **新发现的 TODO 一律写入账本**，不得再往旧 markdown 追加。
4. **门**：`python3 tools/todo-ledger.py check --doc <doc> --fail-on-open`——当前篇
   `status: open` 必须为 0；`closed` 必须有证据；`defer-approved` 必须有四要素。
5. **反假收敛**：`01-todo.md` 里的「已批准待实施」不再算闭环——它必须在账本里是
   `open`（继续做）或 `defer-approved`（有你的亲笔四项）。这是「CONVERGED 但 todo 还有」
   的制度性修复。

## 五、收敛重定义

`CONVERGED` 自本文件生效日起表示**出版就绪**，必须同时满足：

- G1–G9（`review-evals.md` §6A，其中 G3/G9 按本文件收窄）；
- **G10 文档出版**：当前篇 `doc-publication-check.py` 零命中；
- **G11 代码出版**：当前篇关联代码 `code-publication-check.py` 零命中；
- **G12 TODO 账本**：`todo-ledger.py check --doc <当前篇> --fail-on-open` 通过
  （open = 0 且 blocked = 0——待裁项存在时禁止收敛）；
- **零欠账**：`OPEN-DEBT` 无 `[OPEN]`，且无任何未获 `defer-approved` 的 ②/⑥/SUGG。

`ROUND-CLOSED` / `CONVERGENCE-CANDIDATE` 不再等于出版就绪；它们只是过程状态。
