# D01 指标

- 发现总数: 85
- 处置: ① 37 / ② 48 / ③ 0 / ④ 0 / ⑤ 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 15 / 接受 5 / 实施 5 / 登记 10
- 成本: agent 7 / token 未采集 / 耗时 面板约 32min + 修复约 2h

## 说明

- 发现总数 = 7 个面板角色报出的 85 条 `## FINDING`。P-REFUTE 的 3 条反证成立项（CLAIM 04/05/06）内容已并入 FACT-15/03/05/07/09，不单独计为发现。
- ① 37 条：事实 17 条、设计 5 条（DESIGN-06/10/12/14/15）、读者 A 10 条、读者 C 5 条。
- ② 48 条：设计 10 条（DESIGN-01/02/03/04/05/07/08/09/11/13）、读者 A 6 条、读者 B 20 条、读者 C 12 条。全部进 `follow-ups.md`，不作为本轮阻塞。
- 误报为 0，是因为本轮没有把任何面板发现判成 ③；这不等于精度完美，reader B/C 的转出项中可能含少量措辞偏好，留给下一轮闭环复核。
- D01 状态：`NOT CONVERGED`（design-debt 10 条、teaching-debt 若干、test-debt 1 条：`minix-arch` 测试模块编译失败）。

## meta-review

- 报告：`rewrite-notes/coordination/review-eval/meta/01-boot-shim-bootstrap-review.md`
- 总体：`IMPROVED`（V1 基线已建立）
- 运行质量：`NOT PASS`
- 抽样：24/85（28.2%）；处置一致性 21/24 = 87.5%（低于 90%）
- 三条否决：N1 测试计数按文档文件表口径漏文件（实际 125/175）、N2 DESIGN-10 会让 QEMU 测试内核 panic、N3 `kernel_info.rs` 的 `reserved_regions` 注释未同步；均已在评审后补修。
- 新增盲区：B-14 测试计数口径、B-15 公共入口调用方枚举、B-16 材料违规分级。
- 局限：同 agent 验证；未跑 QEMU 真机；token 未采集。
