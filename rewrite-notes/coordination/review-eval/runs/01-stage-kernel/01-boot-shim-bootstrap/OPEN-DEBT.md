# D01 开放欠账（open-debt）

> 唯一欠账源：由 `defect-ledger.md` 的 R14/R15/R16 ② 行抽取，已剔除已完成行；行 ID 带轮次前缀。R18 实施后本表只剩两条：一条相邻项与一条第二阶段迁移；本表非空即不得标记本篇 CONVERGED（G8+G9 仍要求连续两轮零新增）。

| ID | 严重度 | 处置/档期 |
|---|---|---|
| R14/READER-A-16 | P2 | 相邻：Direct Map（`kernel_direct_map_size`/`kern_dm_pages`）的读者主要在尚未生成的 02 篇；本篇已保留边界声明，02 篇立项时补 |
| F-R18-01（D-1 第二阶段） | P2 | U-Boot shim 腿的 `BootStorage` 迁移：`prepare_boot` 目前仍用一次性 `.bss` 静态量 + `Box::leak`（模块表、平台源、参数落地区）；所有权真实但形态未统一。截止：code-excellence 下一批；判据：该腿不再 `Box::leak` 模块/平台/参数，`BootPrepareResult` 携带 storage 视图，QEMU `test-riscv64-uboot-shim.sh` 6/6 不退化 |
