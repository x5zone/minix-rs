# D01 开放欠账（open-debt）

> 唯一欠账源：由 `defect-ledger.md` 的 R14/R15/R16 ② 行抽取，已剔除已完成行；行 ID 带轮次前缀。R17 按本表施工；本表为空才允许标记本篇 CONVERGED（G8+G9 仍要求连续两轮零新增）。

> **代码债**：DESIGN 项（43 条）已从评审欠账移入 code-excellence 批次（不阻塞文档收敛，但阻塞 G9）。文档债 3 条。

| ID | 严重度 | 处置/档期 |
|---|---|---|
| R14/READER-A-16 | P2 | 相邻：02 篇尚未生成；本篇已保留边界声明 |
| R14/DESIGN-02 | P1 | 登记 F-R14-13（`BootReport<'a>` 重构；跨 6 crate，须单独立项） |
| R14/DESIGN-04 | P2 | 登记 F-R14-14 |
| R14/DESIGN-05 | P2 | 登记 F-R14-15 |
| R14/DESIGN-06 | P2 | 登记 F-R14-16 |
| R14/DESIGN-07 | P2 | 登记 F-R14-17 |
| R14/DESIGN-08 | P2 | 登记 F-R14-18 |
| R14/DESIGN-09 | P2 | 登记 F-R14-19 |
| R14/DESIGN-10 | P2 | 登记 F-R14-20 |
| R14/DESIGN-12 | P3 | 登记 F-R14-22 |
| R14/DESIGN-13 | P3 | 登记 F-R14-23 |
| R14/DESIGN-14 | P3 | 登记 F-R14-24（先做读者审计再删） |
| R15/DESIGN-02 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-04 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-05 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-06 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-07 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-08 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-09 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-10 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-12 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-13 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-14 | P3 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-16 | P3 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-17 | P3 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-18 | P3 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R16/DESIGN-02 | P2 | BootReport 重构，修于 R18（跨 6 crate） |
| R16/DESIGN-03 | P2 | prepare_boot 删除，阻塞于裁决项 1 |
| R16/DESIGN-04 | P2 | arena 双写简化，修于 R18 |
| R16/DESIGN-05 | P2 | 双 bump 分配器统一，修于 R18 |
| R16/DESIGN-06 | P2 | DM 候选顺序提常量，修于 R18 |
| R16/DESIGN-07 | P2 | identity_windows 恒空删除，修于 R18 |
| R16/DESIGN-08 | P2 | FileLoader borrow 接口，修于 R18 |
| R16/DESIGN-09 | P2 | 后置断言独立输入，修于 R18 |
| R16/DESIGN-12 | P2 | 三校验 enum 化，修于 R18 |
| R16/DESIGN-14 | P3 | PagingWithId/HUGE_PAGE_SHIFT 审计后删，修于 R18 |
| R16/DESIGN-16 | P3 | 同上 R18 |
| R16/DESIGN-17 | P3 | 同上 R18 |
