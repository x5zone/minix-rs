# D01 R2 — 设计差异审查（P-DESIGN D2）

范围：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` §3/§4；只读源码
`os/libs/minix-boot/src/{kernel_info,handoff_v1,memmap_source,boot_storage}.rs`、
`os/boot-shim/src/lib.rs`、`os/kernel/src/{lib,boot_alloc}.rs`、`os/arch/src/arch/pt_alloc.rs`、
`os/boot-shim/src/{uefi_helpers,ebs_pair,uboot_main,opensbi_helpers,resv_claims}.rs`、`AGENTS.md`。
未读 round1/作者说明；R1 是否引入下列偏差，按现文本判定，不做历史归因。

## FINDINGS

### F1 [DEL] UEFI 腿的 source 分区不止分类器一个生产者

- 文档：§3.7「UEFI 腿的 source 分区由分类器直写 wire，storage 的 source 切片为空」；§3.2.3 同一说法（"source 分区例外，只写 wire"）。
- 源码：`os/boot-shim/src/ebs_pair.rs:194-201` 在 `classify_into_handoff` 提交 UEFI 记录之后，`authorize_and_copy` 拿到配置表 DTB，再 `resv_claims::capture` 生成 DTB `/reserved-memory`/MRB 记录，`push_dtb_sources`（`ebs_pair.rs:243-250`）追加进同一个 source 分区并二次 `finish_sources()`。同期还有一道独立门 `resv_claims::check_current`（`ebs_pair.rs:202`）在 EBS 前校验 DTB 保留声明与 UEFI 描述符类型覆盖。
- 事实偏差：UEFI 腿的 source 分区有两个写入阶段、两种 `input_object`（`UefiMemoryMap` + `DtbBootInput`，见 `os/boot-shim/src/resv_claims.rs:392`），且该第二阶段的记录在 strict 审计里被 authority 过滤忽略（`os/libs/minix-boot/src/memmap_source.rs:352-364`）。"由分类器直写"作为唯一来源的表述与源码不符；`storage` 的 source 切片为空这一点仍然成立。

### F2 [DEL] §3.2.1 把 UEFI 腿的权威声明说成构造点传参

- 文档：§3.2.1「它由构造点写入——wire 解码把 … `ram_authority` 复制进来：两个直核面在 `build_kernel_info` 调用点传 `RamAuthority::DTB`，两条 UEFI 腿传 `RamAuthority::UEFI`，riscv64 U-Boot shim 发送侧在 `Serializer::new` 里声明 DTB 权威」。
- 源码：生产 UEFI 腿没有 `RamAuthority::UEFI` 的构造点。发送侧在 `os/boot-shim/src/uefi_helpers.rs:1131` 声明的是 `InputObjectId::UefiMemoryMap`；接收侧由 `decode_envelope` 把 wire 标签 1 映射为 `RamAuthority::UEFI`（`os/libs/minix-boot/src/handoff_v1_read.rs:160-164`），最终写入 `KernelInfo.authority`（`os/kernel/src/lib.rs:738-741`）。仓内 `RamAuthority::UEFI` 的直接构造只出现在测试（`os/libs/minix-boot/src/kernel_info.rs:912`、`os/boot-shim/src/uefi_helpers.rs:1438`）。
- 事实偏差：UEFI 腿的权威是"发送侧声明 `InputObjectId`、接收侧解码成 `RamAuthority`"，不是"传 `RamAuthority::UEFI`"。§4.4 表已按正确口径描述，§3.2.1 这一段与 §4.4 自相矛盾。

### F3 [DEL] §4.4 字段表把 `source_records` 说成 builder 参数

- 文档：§4.4 表行「`source_records` | 调用方经 builder 参数传入」。
- 源码：`os/boot-shim/src/lib.rs:111-127` 的 `build_kernel_info(report, authority)` 只有两个参数；`source_records: report.source_records` 由 `BootReport` 提供，不是独立实参。只有 `authority` 才是 builder 参数（同表下一行正确）。
- 事实偏差：直核 builder 的入径描述错误，且与紧随其后的 `authority` 行口径不一致。

## 已核对一致（不报）

- `pt_alloc` 写一次槽：`PT_ALLOC`/`PT_FREE`、`register` 的 `compare_exchange`+release fail-closed、`alloc_pt_page` 断言消息与 `os/arch/src/arch/pt_alloc.rs:39-133` 逐条一致；`init_boot_pt_alloc` 只设 bump 区间不注册（`os/kernel/src/boot_alloc.rs:183-186`）；shim/接收镜像经 `boot_validate_and_prepare` 的 `is_registered()` 守卫间接注册（`os/kernel/src/lib.rs:924-926`），VM 注册分配/归还钩子（`os/servers/vm/src/vm_server.rs:381-390`）。§3.4.2 的"一次初始化 + 三次注册"成立。
- `KernelInfo` 10 字段、`authority: RamAuthority`、`validate_requiring_sources()` 无参且从字段读权威、strict 审计按 authority 过滤记录（`kernel_info.rs:20-147,243,449-483`；`memmap_source.rs:352-364`）。
- `RamAuthority` 只包 `InputObjectId`、后者的 DTB/UEFI 两变体与"标签 2 保留"注释（`memmap_source.rs:589-635`）；`StartupObjects`/`startup_record`/`WIRE_STARTUP_END` 在当前树零命中，与 §3.2.1/§3.2.3 的 v10 删除叙述一致。
- arena 常量：`ARENA_CAP=32768`、`WIRE_END=7872`、`WIRE_SOURCES_END=29184`、余 3584、`BOOT_HANDOFF_V1_VERSION=10`（`handoff_v1.rs:278,378-392,432-464`），与 §3.2.3 分区算术一致；`check_header` 校验 magic/version/size/arena_len（`handoff_v1_read.rs:100-116`）。
- U-Boot shim 发送侧在 `Serializer::new(..., InputObjectId::DtbBootInput)` 声明 DTB 权威（`os/boot-shim/src/uboot_main.rs:491`）；直核面 `build_kernel_info(..., RamAuthority::DTB)`（`os/kernel-image/src/bootface_common.rs:558`）。§3.2.1/§4.4 这两处成立。
- `BootStorage` 五个 `fill_*` 入口（`boot_storage.rs:141,163,178,196,211`）与 §3.7「五个 `fill_*`、六类载荷」一致；U-Boot 腿走 `fill_source_records`（`opensbi_helpers.rs:316`），UEFI 腿的 storage source 切片为空。
- 落地区容量：`MEMMAP_REGION_STORE_LEN=128`、`RESERVED_REGION_STORE_LEN=256`、`VM_BOOT_HANDOFF_MAX_REGIONS=64`、`VM_BOOT_HANDOFF_MAX_DEDUCTED=47`（`os/kernel/src/globals.rs:297,327`；`os/libs/minix-types/src/types/boot.rs:69,84`），与 §3.2.1 三个数字一致；C `MAXMEMMAP 40` 见 `minix3/minix/include/minix/param.h:13`。
- `check_boot_postconditions` 仅剩 `kern_virt_base > u32::MAX`（`kernel_info.rs:553-569`）；`decode_envelope` 的 bump 三元组/root page 校验（`handoff_v1_read.rs:160-174`）与 §3.6/§4.4.1 描述一致。
- A+D：`reserve_kernel_span` 用 `AllocateType::Address(base)` + `LOADER_DATA`（`uefi_helpers.rs:69-92`），arena 用 `MaxAddress(bootstrap_alloc_limit())`（`uefi_helpers.rs:700-709`），与 §3.2.1/§4.2.1 动作 2/4 一致。

## VERDICT

**CONDITIONAL PASS（P1 级文档事实偏差，3 条，无代码缺陷）**

- 未发现 R1 修复引入的 P0 设计事实错误：`pt_alloc` 换手、arena 常量、authority 的类型与解码、strict 审计口径均与源码一致。
- 残留偏差集中在 source 分区与 authority 的"谁写/从哪来"叙述：F1 是 §3.7/§3.2.3 对 UEFI source 分区生产者的不完整描述（漏 DTB claims 第二阶段），F2/F3 是 §3.2.1/§4.4 的归属与参数口径错误。三条均需按源码改写，不改代码。
- 修法（一次一条，按 fix-guard）：F1 在 §3.7 与 §4.2 的配对循环说明中补 `resv_claims::capture` → `push_dtb_sources` 的二次写入与 `check_current` 门，并说明该批记录在 strict 审计中被 authority 过滤；F2 改为"UEFI 发送侧在 `Serializer::new` 声明 `InputObjectId::UefiMemoryMap`，接收侧 `decode_envelope` 映射为 `RamAuthority::UEFI`"；F3 改为"`source_records` 经 `BootReport` 提供，builder 参数只有 `authority`"。
