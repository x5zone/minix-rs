# D01 开放欠账（open-debt）

> 本表只收**跨 crate / 需要独立项目**的 ② 项（负责人 + 截止轮次），按 `prompt/review-rules/review-evals.md` §4.1 与 G9 的口径管理。
> `adjacent` 项不进本表：它们在 `follow-ups.md` 登记目标轮次，按 §4.2 的“adjacent 阻塞 = 0”规则**不阻塞本篇收敛**。
> 本表为空是“没有跨 crate 欠账”的充分信号，但不是收敛的必要条件；收敛只看 G8（连续两轮无新 in-scope 发现）与 G9（⑥ 项清零）。

| ID | 严重度 | 处置/档期 |
|---|---|---|
| F-R17-02 | P2 | UEFI 测试辅助三件套的 feature 门（跨 16 个测试内核）；截止 R23（已逾期两轮） |
| F-R17-04 | P2 | `KernelInfo` 增 `ram_authority` 字段（跨 minix-boot/内核/文档 ABI）；截止 R23 前裁决（已逾期两轮） |
| F-R17-01 | P2 | `BootShim` 单实现 trait 的架构去留（保留为未来固件腿接口 / 改普通函数）；截止 R23 |
| F-R17-03 | P2 | `dtb_copy::Phase` 栈化（带真实生命周期）；截止 R23 |
| F-R20-01 | P2 | `BootPrepareResult`/`enter_kernel_image_v1` 的 bump/arena 改物理区间类型（跨 crate）；截止 R23 |
| F-R20-02 | P2 | `MemoryRegion`/`cut(start,size)`/`MemRange` 三套端点约定收敛为 `Range{start,end}`；截止 R23 |
| F-R20-03 | P2 | 测试内核 `build_memmaps`/`exit_boot_services` 假 `'static` 迁移到 `BootStorage`；截止 R23 |
| F-R20-04 | P2 | 13 个 `checked_add` 点仅 3 处有溢出负控；截止 R23 |
| — | — | F-R18-01 已实施；R14/READER-A-16 为 adjacent，见 `follow-ups.md` |
