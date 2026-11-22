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

- 发现总数: 15
- 唯一根因数: 15
- 本轮新准入: 15
- certify 新准入: 0
- 重复合并: 4（IST、载体、39/41、lgdt 跨角色）
- 自引入: 0
- 处置: ① 14 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 1 / SUGG 0
- 未闭环: 0
- edition: V3.11
- 轮型: full
- 结果: ROUND-CLOSED
- 代裁决: 0 / ARCH 提案: 1（D2 的 refresh no-op，未实施）/ 用户否决: 0
- 减法: 提案 1 / 采纳 0
- full 轮: R1，覆盖 44/44 单元
- 单元: 31 pass + 13 fix；R2 全部复审
- 机械门: 文档门全绿；cargo check minix-arch 通过

## R2 full（V3.11）

- 发现总数: 16
- 唯一根因数: 16
- 本轮新准入: 16
- certify 新准入: 0
- 重复合并: 5（IST、重复段、LDT、TSP、非 x86）
- 自引入: 3（R1 的 IST 表行、§4.4 重复段、LDT 引文收窄；均在本轮修复）
- 处置: ① 16 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0
- 未闭环: 0
- edition: V3.11
- 轮型: full
- 结果: ROUND-CLOSED
- 代裁决: 0 / ARCH 提案: 1（D2 的 refresh 无条件化，未实施）/ 用户否决: 0
- 减法: 提案 1 / 采纳 0
- full 轮: R2，覆盖 44/44 单元
- 单元: 44/44 frozen；R3 逐单元确认
- 机械门: 文档门全绿；cargo check minix-arch 通过

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
- 单元: 44/44 frozen；R3 逐单元 confirm-review
- 代裁决: 0 / ARCH 提案: 1（未实施）/ 用户否决: 0
- 减法: 提案 1 / 采纳 0
