# metrics

## R0 账户初始化

- 发现总数: 0
- 唯一根因数: 0
- 本轮新准入: 0
- 未闭环: 0
- 轮型: account-init
- 轮结果: R0
- full_rounds: 0

## R1 full（V3.11）

- 发现总数: 12
- 唯一根因数: 12
- 本轮新准入: 12
- certify 新准入: 0
- 重复合并: 3（L3b/L3c、堆栈症状等跨角色同根）
- 自引入: 0
- 处置: ① 11 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 1 / SUGG 0
- 未闭环: 0
- edition: V3.11
- 轮型: full
- 结果: ROUND-CLOSED
- 代裁决: 0 / ARCH 提案: 1（D2 的根 typestate/EnterTarget，未实施）/ 用户否决: 0
- 减法: 提案 1 / 采纳 0
- full 轮: R1，覆盖 41/41 单元
- 单元: 32 pass + 9 fix；R2 全部复审
- 机械门: publication/tutorial/structure/ref/style/citation/anchor/density 全绿

## R2 full（V3.11）

- 发现总数: 9
- 唯一根因数: 9
- 本轮新准入: 9
- certify 新准入: 0
- 重复合并: 3（L3/L6、§5.4 边界、§2.5 后果跨角色）
- 自引入: 2（R1 的 L3/L6 子断言写错、§5.4 边界只列三项；均在本轮修复）
- 处置: ① 9 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0
- 未闭环: 0
- edition: V3.11
- 轮型: full
- 结果: ROUND-CLOSED
- 代裁决: 0 / ARCH 提案: 1（D2 的 SelectedSize，未实施）/ 用户否决: 0
- 减法: 提案 1 / 采纳 0
- full 轮: R2，覆盖 41/41 单元
- 单元: 41/41 frozen；R3 逐单元确认
- 机械门: 文档门全绿；cargo check minix-kernel mock 通过

## R3 confirmation（V3.11）

- 发现总数: 0
- 唯一根因数: 0
- 本轮新准入: 0
- certify 新准入: 0
- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0
- 未闭环: 0
- edition: V3.11
- 轮型: confirmation
- 结果: CONVERGED
- full 轮: R1/R2 均覆盖全部单元
- 单元: 41/41 frozen；R3 逐单元 confirm-review
- 代裁决: 0 / ARCH 提案: 1（未实施）/ 用户否决: 0
- 减法: 提案 1 / 采纳 0
