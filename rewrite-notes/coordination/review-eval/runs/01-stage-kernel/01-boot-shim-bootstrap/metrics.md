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
