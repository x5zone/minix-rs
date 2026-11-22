# D01 R1 metrics（V3.12）

## R1（V3.12，full）

- 发现总数: 48
- 唯一根因数: 41
- 本轮新准入: 40
- certify 新准入: 1
- 重复合并: 8
- 自引入: 1
- 处置: ① 40 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 1 / FP 0 / SUGG 0
- 未闭环: 1
- 轮型: full
- 轮结果: BLOCKED
- 代裁决: 0 / ARCH 提案: 0 / 用户否决: 0
- ARCH 提案: 0
- 用户否决: 0
- edition: V3.12
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 减法: 提案 0 / 采纳 0
- 成本: agent 20 / token 未统计 / 耗时 约 3 小时

说明：
- `R1-24`（DESIGN-02）按 `review-publication.md` §四.6 属设计批级，本轮不实施；
  这是唯一 open 根因，`⑥` 只此一行。
- 36 条 SUGGESTION 在台账 `## 建议` 节；已随修复落地约 15 条，其余待 R2/作者裁决。
  本轮 `BLOCKED`，不宣告 `CONVERGED`，因此 `SUGG` 未清账不影响状态判定。
- 面板角色事件 6 条 completed-pass；正文修复后 5 条 stale；最终单元事件未立。
- 独立复验：文档 34/35→R1-14 修正后 PASS；代码 R1-37..R1-40 + R1-41 PASS；
  beginner-me 冷读 `HARD-REJECTIONS: 0` / `VERDICT: READY`。
