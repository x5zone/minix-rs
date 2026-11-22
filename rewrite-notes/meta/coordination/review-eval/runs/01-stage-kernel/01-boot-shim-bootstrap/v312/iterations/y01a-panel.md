# y01a — D01 R1 面板结果（D01 V3.12 R1）

- 时间：2026-11-23
- 面板：6 个独立角色（P-FACT、READER-A/B/C、DESIGN-D2、P-REFUTE）全部完成。
- 入库：`v312/panel/round1/`（6 份报告 + INDEX.md）；6 个角色事件全部 `completed-pass`。
- 原始 tmp 报告在 `tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/v312-r1/`；
  入库副本顶部由调度层加了一行 `DOC 01-boot-shim-bootstrap.md` 绑定，原始报告未改。
- 发现：48 条 FINDING（P0 0 / P1 5 / P2 24 / P3 19）+ 36 条 SUGGESTION。
- 反证：6 条核心主张全部反证失败（主张成立），另有 4 条论证边界 P2。

## P1（5 条）

1. `DESIGN-01`：§3.2.2.2 说 `KinfoStruct.user_sp` 没有 Rust userland 读者；实际 RS 的
   `TrapKernelApi::user_sp()` 经 `GET_KINFO` 读 offset 8，决定 exec 栈帧落点。删除依据建立在假事实上。
2. `READER-B-01`：§2.4.2.1 槽位表“或数组末尾”与 §2.4 引文的 `panic("no available memmap slot")` 冲突。
3. `READER-B-02`：§6.4 说“riscv64 两条链看 TEST_RESULT”，与 §5.4 的三条链三套判据冲突。
4. `READER-C-01`：I4 写成“所有校验在消费之前”，与 §4.4.1 把 `KernelInfo::validate` 放在 `kmain` 入口冲突。
5. `READER-C-02`：§5.1/§5.3 给 I4 的证据没有一条证明“校验先于建表/分配”。

## P2 里最重的

- `DESIGN-02`：三个 v1 腿用借用视图，两个直核腿却用 `BootStorageCell::report_static` 把借用抬成
  `'static`，再 `build_kernel_info` 搬进 `KernelInfo`；kernel-image `.data` 常驻约 10.5 KiB
  `BootStorage`。建议走 D-024 允许的回退：入口收借用视图，接收侧落 `.bss` 后再生成唯一的
  `KernelInfo<'static>`；删 `BootStorageCell`/`report_static`/`build_kernel_info`/`STORAGE`。
- `FACT-01`：§4.2.2 引用了不存在的 `MapError::Policy` 变体。
- 多个读者项指向指针/表格/术语：§7.5 速查表指错节、§3.2.2.3 标题与内容不符、§3.2.2.2 的 used 计数、
  §4.0 把 `register` 挂在 `init_boot_pt_alloc` 名下、§5.5 的 `A2` 未定义、v10 删除史在六节重复。
- `DESIGN-03/04/05/06`：死面删除、`PlatformDescSource.len` 哨兵改 `Option<NonZeroU32>`、
  三处 v9 代码注释、四处文档精度。

## 下一步

- 归并分析起草 defect-ledger（按根因合并 48 条 FINDING，建议单独归档）。
- 主 agent 复核覆盖、根因、处置类与复验命令；然后按批次修复。
- 面板 checkpoint commit：`bfa72225d`。
