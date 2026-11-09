# D01 指标

## R17

- 发现总数: 87
- 处置: ① 26 / ② 3 / ③ 10 / ④ 48 / ⑤ 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 8 / 接受 8 / 实施 8 / 登记 4
- 成本: agent 7 / token 未采集 / 耗时 约 2.5 小时（面板 + 修复）

## R18

- 发现总数: 98（文档缺陷 86 + D1 方案否决 12；按角色：FACT 12 主条 + 9 次级；READER-A 20；READER-B 20；READER-C 18；REFUTE 6 主张；DESIGN-D2 13 条）
- 处置: ① 21（FACT 16、REFUTE 4、DESIGN-D2-10 1）/ ② 0 / ③ 21（FACT 5、READER-B-15、READER-C-10、REFUTE 2、DESIGN-D2 12）/ ④ 56（READER-A 20、READER-B 19、READER-C 17）/ ⑤ 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 1 / 接受 1 / 实施 1 / 登记 0（`KernelInfo::validate` 增加内核×模块×模块半开区间复核）
- 成本: agent 7（fact / reader A/B/C / refute / design D1 / design D2）/ token 未采集 / 耗时 约 4 小时（面板 + 修复 + 三门）
- 回读: RB17 独立回读判 NOT PASS（P0=0、P1=0），其 P2/P3 补修已并入本轮；RB18 待本轮提交后启动

## R19

- 发现总数: 80（FACT 6；READER-A 20；READER-B 20；READER-C 15；REFUTE 7；DESIGN-D2 12）
- 处置: ① 13 / ② 0 / ③ 14 / ④ 53 / ⑤ 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 1 / 接受 1 / 实施 1 / 登记 0（`KernelInfo::validate` 跨度 `checked_add`）
- 成本: agent 7 / token 未采集 / 耗时 约 45 分钟（面板 + 修复 + 三门，不含排队）
- 回读: RB19 判 NOT PASS（FACT-06 行号）；补修后 meta R19 判 IMPROVED / NOT PASS（Trace integrity 与计数差 1），二次补修后四组探针均 FAIL、R18/R19 计数闭合

## R20

- 发现总数: 64（FACT 3；READER-A 20；READER-B 20；READER-C 8；REFUTE 6；DESIGN-D2 7）
- 处置: ① 12 / ② 3 / ③ 1 / ④ 48 / ⑤ 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 1 / 接受 1 / 实施 1 / 登记 0（六处跨度加法 checked_add）
- 成本: agent 7 / token 未采集 / 耗时 约 40 分钟（面板 + 修复 + 三门）
- 回读: RB20 判 NOT PASS（LSR 释义/八条未同步等 2 P2 + 5 P3），补修后 minix-boot 189、三门回绿；meta R20 待跑

## R21

- 发现总数: 63（READER-A 20；READER-B 20；READER-C 11；REFUTE 7；DESIGN-D2 5；P-FACT MISSING）
- 处置: ① 14 / ② 3 / ③ 3 / ④ 43 / ⑤ 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 6 / token 未采集 / 耗时 约 50 分钟
- 回读: RB21 判 NOT PASS（36/42 ④ 未修）；meta R21 判 NOT PASS（严格一致 14/37=37.8%，Precision/Teaching/Trace FAIL），R22 逐 ID 补修

## R22

- 发现总数: 67（FACT-A 3；FACT-B 2；READER-A 20；READER-B 20；READER-C 10；REFUTE 7 条主张；DESIGN-D2 5）
- 处置: ① 9 / ② 3 / ③ 5 / ④ 50 / ⑤ 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 7 / token 未采集 / 耗时 约 55 分钟
- 回读: RB22 判 NOT PASS（EAB/审计表/生产腿/§7.5 等）；六轮复核后 meta R22 **IMPROVED / PASS**（50/50 ④ 全修、探针全 FAIL、67 分档闭合）；保留 60.9% 历史抽样读数

## R23

- 发现总数: 110（FACT-A 7；FACT-B 17；READER-A 24；READER-B 20；READER-C 20；REFUTE 6 条主张；DESIGN-D2 16）
- 处置: ① 61 / ② 3 / ③ 6 / ④ 40 / ⑤ 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 8 / 接受 5 / 实施 5 / 登记 3（bootface 合并、kern_ident 删除、pt_alloc release 语义转 R24；wire 偏移与 landing helper 本轮已实施）
- 成本: agent 7 / token 未采集 / 耗时 约 4 小时（面板 + 修复 + 三门）
- 回读与回归: 面板 104 条 + 6 主张全部有处置；doc 五门 PASS；宿主 196/135+5/862+8/287；QEMU x86/aarch64/riscv 三门 PASS（日志 `logs/round23-*.log`）。RB23 首轮 NOT PASS（B1 §4.2 引文旧 decode 路径、B2 假归因；Q1–Q4）→ 补修后 PASS；meta R23 首轮 IMPROVED / 运行质量 NOT PASS（严格一致率 73.3%）→ 5 项收口后第二次审计 IMPROVED / 条件 PASS（最终态 90.0%）

## R24

- 发现总数: 118（FACT-A 15；FACT-B 21；READER-A 20；READER-B 20；READER-C 20；REFUTE 6 条主张；DESIGN-D2 16）
- 处置: ① 114 / ② 0 / ③ 4 / ④ 0 / ⑤ 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 4 / 接受 3 / 实施 3 / 登记 0（`ident_regions` 死接口删除、公共窗口 helper、`PlacementPool`；bootface 整段合并不做）
- 成本: agent 7 / token 未采集 / 耗时 约 2 小时（面板 + 修复 + 宿主/QEMU）
- 回读: 宿主 boot-shim 136+6、kernel 865+8、types 339、boot 196、vm 548、arch 288；QEMU x86/aarch64/riscv U-Boot shim、riscv bootface、riscv kernel-image、aarch64 SMP AP 六门 PASS；doc 五门 PASS；RB24 首轮 71/80=88.8% NOT PASS；B1/B2+N1–N11 共 13 项补修后，二次定向回读 23/23=100% PASS（13 定向 + 10 重放；不是 118 条整体抽样）；提交后 RB-post 随机 29 条 26/29=89.7% NOT PASS → 补跨度测试后二次 13/14=92.9% PASS；修复提交链 `91e652a73`→`6a40c61fc`→`91da40986`

## R25

- 发现总数: 124（FACT-A 14；FACT-B 17；READER-A 20；READER-B 31；READER-C 20；REFUTE 6 条主张；DESIGN-D2 16）
- 处置: ① 98 / ② 0 / ③ 26 / ④ 0 / ⑤ 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 3 / 接受 2 / 实施 2（`PlatformDescSource` 可选长度、`map_kernel` 收 `KernelLayout`）；`records_in_wire`/直核静态落地区维持 D-034 与 D-013 边界
- 成本: agent 7 / token 未采集 / 耗时 约 2 小时（面板 + P0/P1 修复 + 宿主/QEMU）
- 回读: 待 RB25；宿主 136+6/865+8/339/196/548/288；QEMU 六门日志 `logs/round25-*.log`；doc 五门 PASS
