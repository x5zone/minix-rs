# D01 指标

## R17

- 发现总数: 87
- 处置: ① 26 / ② 3 / ③ 10 / ④ 48 / ⑤ 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 8 / 接受 8 / 实施 8 / 登记 4
- 成本: agent 7 / token 未采集 / 耗时 约 2.5 小时（面板 + 修复）

## R18

- 发现总数: 98（FACT 12 主条 + 9 次级；READER-A 20；READER-B 20；READER-C 18；REFUTE 6 主张；DESIGN-D2 13 条）
- 处置: ① 21（FACT 16、REFUTE 4、DESIGN-D2-10 1）/ ② 0 / ③ 21（FACT 5、READER-B-15、READER-C-10、REFUTE 2、DESIGN-D2 12）/ ④ 56（READER-A 20、READER-B 19、READER-C 17）/ ⑤ 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 1 / 接受 1 / 实施 1 / 登记 0（`KernelInfo::validate` 增加内核×模块×模块半开区间复核）
- 成本: agent 7（fact / reader A/B/C / refute / design D1 / design D2）/ token 未采集 / 耗时 约 4 小时（面板 + 修复 + 三门）
- 回读: RB17 独立回读判 NOT PASS（P0=0、P1=0），其 P2/P3 补修已并入本轮；RB18 待本轮提交后启动
