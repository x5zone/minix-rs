# D01 开放欠账（open-debt）

> 本表只收**跨 crate / 需要独立项目**的 ② 项（负责人 + 截止轮次），按 `prompt/review-rules/review-evals.md` §4.1 与 G9 的口径管理。
> `adjacent` 项不进本表：它们在 `follow-ups.md` 登记目标轮次，按 §4.2 的“adjacent 阻塞 = 0”规则**不阻塞本篇收敛**。
> 本表为空是“没有跨 crate 欠账”的充分信号，但不是收敛的必要条件；收敛只看 G8（连续两轮无新 in-scope 发现）与 G9（⑥ 项清零）。

| ID | 严重度 | 处置/档期 |
|---|---|---|
| F-R17-02 | P2 | ✅ 已修：UEFI 测试辅助三件套 + `UefiPoolAllocator` + 四个 static 收进 `qemu-test-kernels` feature；16 个测试内核 manifest 同步；生产 `fw-*-uefi` 不再链接 |
| F-R17-04 | P2 | `KernelInfo` 增 `ram_authority` 字段（跨 minix-boot/内核/文档 ABI）；截止 R23 前裁决（已逾期两轮） |
| F-R17-01 | P2 | `BootShim` 单实现 trait 的架构去留（保留为未来固件腿接口 / 改普通函数）；截止 R23 |
| F-R17-03 | P2 | `dtb_copy::Phase` 栈化（带真实生命周期）；截止 R23 |
| F-R20-01 | P2 | ✅ 已修：`BootPrepareResult.bump: PhysRange`、`arena_pa/root_page: PhysBytes`；`enter_kernel_image_v1` 收 `PhysBytes` |
| F-R20-02 | P2 | ✅ 已修：`PhysRange`（构造拒绝反向）统一 `FreeRangeSet::cut`，调用点不再传 `(start,size)` |
| F-R20-03 | P2 | **部分**：helper 已收进 `qemu-test-kernels`（生产不链接），但 `static mut`/假 `'static` 仍在测试路径；完整迁移受 `KernelInfo: &'static` 约束，见 R23 阻塞项 |
| F-R20-04 | P2 | ✅ 已修：补 reserved / from_memmap / dtb 覆盖的溢出/反向负控；minix-boot 192、kernel 862+8 |
| — | — | F-R18-01 已实施；R14/READER-A-16 为 adjacent，见 `follow-ups.md` |
