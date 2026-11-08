# D01 开放欠账（open-debt）

> 唯一欠账源：由 `defect-ledger.md` 的 R14/R15/R16 ② 行抽取，已剔除已完成行；行 ID 带轮次前缀。R17 按本表施工；本表为空才允许标记本篇 CONVERGED（G8+G9 仍要求连续两轮零新增）。

> **代码债**：DESIGN 项（43 条）已从评审欠账移入 code-excellence 批次（不阻塞文档收敛，但阻塞 G9）。文档债 28 条。

| ID | 严重度 | 处置/档期 |
|---|---|---|
| R14/READER-A-16 | P2 | 登记 F-R14-04（02 篇范围），本篇只保留边界声明 |
| R14/READER-A-20 | P3 | 登记 F-R14-05（分类章表格重排） |
| R14/READER-B-06 | P2 | 裁决：待裁决项 1（删 prepare_boot/BootForm）；截止 R16 前；裁决前说明必须各处可见 |
| R14/DESIGN-02 | P1 | 登记 F-R14-13（`BootReport<'a>` 重构；跨 6 crate，须单独立项） |
| R14/DESIGN-04 | P2 | 登记 F-R14-14 |
| R14/DESIGN-05 | P2 | 登记 F-R14-15 |
| R14/DESIGN-08 | P2 | 登记 F-R14-18 |
| R14/DESIGN-11 | P3 | 登记 F-R14-21 |
| R14/DESIGN-15 | P3 | 登记 F-R14-25（建议改名不重构） |
| R15/DESIGN-02 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-04 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-05 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-08 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-10 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-11 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/DESIGN-13 | P2 | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 |
| R15/READER-A-15 | P2 | ② 结构项（§1.1 拆段仍不够三论证分离），修于 R18 |
| R15/READER-A-16 | P2 | ② 结构项（§2.2 ABI 支线），修于 R18 |
| R15/READER-A-22 | P2 | ② 结构项（§3.2 条件编译叙述），修于 R18 |
| R15/READER-B-09 | P2 | 结构项（下游交代），修于 R18 |
| R15/READER-B-12 | P3 | 结构项（§6.3 信号重复），修于 R18 |
| R16/DESIGN-02 | P2 | BootReport 重构，修于 R18（跨 6 crate） |
| R16/DESIGN-03 | P2 | prepare_boot 删除，阻塞于裁决项 1 |
| R16/DESIGN-04 | P2 | arena 双写简化，修于 R18 |
| R16/DESIGN-05 | P2 | 双 bump 分配器统一，修于 R18 |
| R16/DESIGN-06 | P2 | DM 候选顺序提常量，修于 R18 |
| R16/DESIGN-08 | P2 | FileLoader borrow 接口，修于 R18 |
| R16/DESIGN-11 | P2 | pub 字段/getter 二选一，修于 R18 |
| R16/DESIGN-12 | P2 | 三校验 enum 化，修于 R18 |
| R16/DESIGN-15 | P3 | Plan 两遍遍历改名，修于 R18 |
| R16/DESIGN-16 | P3 | 同上 R18 |
| R16/DESIGN-17 | P3 | 同上 R18 |
| R16/READER-A-03 | P2 | 跨文档链接基址混用（B-46/C-18 门），修于 R18 |
| R16/READER-A-11 | P2 | 12 模块名缩写与 RS 展开，修于 R18 |
| R16/READER-A-20 | P2 | §3.8 两次注册与写槽的混淆，修于 R18 |
| R16/READER-B-15 | P2 | §6.3 与 §2.2 信号重复，修于 R18 |
| R16/READER-B-17 | P2 | §4.2.1 九步表内部动作粒度，修于 R18 |
| R16/READER-B-20 | P2 | §3.1 三否一采计数口径，修于 R18 |
| R16/READER-C-07 | P2 | §6.1 v0 双打印与 aarch64 路标说明的口径差异，修于 R18 |
| R16/READER-C-08 | P2 | §3.8 三次注册算不平，修于 R18 |
| R16/READER-C-09 | P2 | §2.6.3/§4.2 pg_map USER 位口径不一，修于 R18 |
| R16/READER-C-11 | P2 | §6.4 `movaps` 风险无测试/诊断面，修于 R18 |
| R16/READER-C-12 | P2 | aarch64 分道 QEMU 不设 x0 前提无证据，修于 R18 |
| R16/READER-C-13 | P2 | §2.4.2 结算段下游越界未收口，修于 R18 |
