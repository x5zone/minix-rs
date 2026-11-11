# D01 开放欠账（open-debt）

> 本表只收**跨 crate / 需要独立项目**的 ② 项（负责人 + 截止轮次），按 `prompt/review-rules/review-evals.md` §4.0f（V3.3 单一未闭环口径）与 G9 管理。
> `adjacent` 项不进本表：它们在 `follow-ups.md` 登记目标轮次。
> **本表的未销账行计入未闭环（V3.3 §4.0f）**：有未销账行 = 本轮不得关闭、不得派下一轮面板。
> 只有两个出口：实施（附代码证据或 `dup-of` 指向覆盖根因）或 `DECISIONS.md` 判决账行；没有第三个状态。

| ID | 严重度 | 处置/档期 |
|---|---|---|
| F-R17-02 | P2 | ✅ 已修：UEFI 测试辅助三件套 + `UefiPoolAllocator` + 四个 static 收进 `qemu-test-kernels` feature；16 个测试内核 manifest 同步；生产 `fw-*-uefi` 不再链接 |
| F-R17-04 | P2 | ✅ 已修（R23 前置）：`KernelInfo` 增第 14 个字段 `authority: InputObjectId`，`validate_requiring_sources()` 改无参读字段；wire 解码复制 `ram_authority`，直核腿传 `DtbBootInput`、UEFI 腿传 `UefiMemoryMap`；文档 13→14 与绑定说明同步；`minix-boot` 192 通过 |
| F-R17-01 | P2 | ✅ 已修（R23 前置）：删除单实现 `BootShim` trait，U-Boot 腿改普通函数 `opensbi_helpers::prepare_boot`；§3.2 按事实重写；`impl BootShim` 与 `dyn BootShim` 零命中；三 QEMU 门 PASS |
| F-R17-03 | P2 | ✅ 已修：`static mut PHASE` → `PhaseCell(UnsafeCell<Phase>)`，`Sync` 的安全论证集中一处；无 `static mut` |
| F-R20-01 | P2 | ✅ 已修：`BootPrepareResult.bump: PhysRange`、`arena_pa/root_page: PhysBytes`；`enter_kernel_image_v1` 收 `PhysBytes` |
| F-R20-02 | P2 | ✅ 已修：`PhysRange`（构造拒绝反向）统一 `FreeRangeSet::cut`，调用点不再传 `(start,size)` |
| F-R20-03 | P2 | ✅ 已修：四个 `static mut` 表 → write-once `TestMapTables`（AtomicBool 单写 + Release 发布，初始化后只读）；`qemu-test-kernels` 门控，生产不链接；`'static` 由真实 static 生命周期支撑 |
| F-R20-04 | P2 | ✅ 已修：补 reserved / from_memmap / dtb 覆盖的溢出/反向负控；minix-boot 192、kernel 862+8 |
| F-R23-03 | P2 | ✅ 已修（R23 后）：删除恒空 `kern_ident`/`kern_ident_count` 与 `VM_BOOT_HANDOFF_MAX_IDENT`；`VM_BOOT_HANDOFF_VERSION` 8→9；VM server 与测试同步 |
| R26-design-batch | P1 | **已销账（R32 代理裁决）**：D2-01/02/03/04/05 在 R27 code batch 落码或被后续轮取代；D2-07..12 的核心项（BootStorage 统一、BootStorageCell、store_kernel_info 返回值）已在 R29/R30 实施；残余项（参数互换 newtype、enum 哨兵）降为 V3.2 改进方向。DECISIONS.md D-043 关闭 |
| — | — | F-R18-01 已实施；R14/READER-A-16 为 adjacent，见 `follow-ups.md` |
