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

- 发现总数: 46（逐发现计数；唯一根因 38，含 15 条反证主张）
- 唯一根因数: 38
- 本轮新准入: 38
- certify 新准入: 0
- 重复合并: 18（编号重复/测试计数/证据口径跨角色合并）
- 自引入: 0
- 处置: ① 40 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 3 / SUGG 3
- 未闭环: 0
- edition: V3.11
- 轮型: full
- 减法: 提案 1 / 采纳 0
- ARCH 提案: 1
- 用户否决: 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: token 约 4.2M（7 个面板 agent 的会话量级；无精确计费）
- 轮结果: ROUND-CLOSED（进入 CONVERGENCE-CANDIDATE）
- 代裁决: 0 / ARCH 提案: 0 / 用户否决: 0
- full 轮: R1，覆盖 49/49 单元
- 单元: R1 review 35 个 pass、22 个 fix；R2 必须全部复审
- 机械门: publication 0 新增；tutorial 6/6；citation failed=0；anchor 90/90；doc-code-map 48/48；density 通过

## R2 full（V3.11）

- 发现总数: 14
- 唯一根因数: 14
- 本轮新准入: 14
- certify 新准入: 0
- 重复合并: 6（FACT-01/RA-F1/RB-F2；RA-F2/RB-F3 等）
- 自引入: 2（R1 的 §5.1 断表、§5.4 DTB 变体写反，均在本轮修复）
- 处置: ① 14 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0
- 未闭环: 0
- edition: V3.11
- 轮型: full
- 结果: ROUND-CLOSED
- 代裁决: 0 / ARCH 提案: 1（D2 的 ARCH 候选，未实施）/ 用户否决: 0
- 减法: 提案 1 / 采纳 0
- full 轮: R2，覆盖 48/48 单元（2.4.2 层级修正后并入 2.4）
- 单元: 48/48 frozen；R3 逐单元确认
- 机械门: structure/ref/publication/tutorial/citation/anchor/density 全绿；cargo minix-boot 188/0、boot-shim 136/0、minix-arch 266/0、minix-kernel mock 866/0/8

## R3 confirmation（V3.11）

- 发现总数: 0
- 唯一根因数: 0
- 本轮新准入: 0
- certify 新准入: 0
- 处置: ① 0 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0
- 未闭环: 0
- edition: V3.11
- 轮型: confirmation
- 结果: NOT CONVERGED（用户 2026-11-22 人工审阅否决；机械确认轮 0 新发现，不构成出版验收）
- full 轮: R1/R2 均覆盖全部单元
- 单元: 48/48 frozen；R3 逐单元 confirm-review
- 代裁决: 0 / ARCH 提案: 0 / 用户否决: 0
- 减法: 提案 1 / 采纳 0
- 最终状态: NOT CONVERGED（D-116；用户人工审阅否决）
