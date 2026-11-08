# D01 开放欠账（open-debt）

> 唯一欠账源：由 `defect-ledger.md` 的 R14/R15/R16 ② 行抽取，已剔除已完成行；行 ID 带轮次前缀。R17 按本表施工；本表为空才允许标记本篇 CONVERGED（G8+G9 仍要求连续两轮零新增）。

| ID | 严重度 | 处置/档期 |
|---|---|---|
| R14/READER-A-16 | P2 | 相邻：02 篇尚未生成；本篇已保留边界声明 |
| R14/DESIGN-02 | P1 | 登记 F-R14-13（`BootReport<'a>` 重构；跨 6 crate，须单独立项） |
| R14/DESIGN-04 | P2 | 登记 F-R14-14 |
| R14/DESIGN-05 | P2 | R18：分区语义需设计裁决——试合并游标后 QEMU U-Boot shim 门在 3/6 标记处 panic（bump_end 是 1024 页页表专用子区间，模块/arena 在其外），已回滚 |
| R14/DESIGN-08 | P2 | 登记 F-R14-18 |
| R15/DESIGN-02 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-04 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-05 | P2 | 同 R14/DESIGN-05 |
| R15/DESIGN-08 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R16/DESIGN-02 | P2 | BootReport 重构，修于 R18（跨 6 crate） |
| R16/DESIGN-04 | P2 | arena 双写简化，修于 R18 |
| R16/DESIGN-05 | P2 | 同 R14/DESIGN-05 |
| R16/DESIGN-08 | P2 | FileLoader borrow 接口，修于 R18 |
