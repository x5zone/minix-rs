DOC: rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
CHAPTER: 第 6 章 诊断（切片 tmp/review-loop/01-stage/chapters/01-boot-shim-bootstrap/ch6.md，sha 5ac709e4）
VERDICT: DEFICIENT

核对基线：摘要 52 行，sha256 `29399850…`；切片 192 行，sha `5ac709e4` 与给定值一致。

缺项（下游边界/定位信号）
1. 摘要 L34「自举页被报成可用：`assert_bootstrap_outside_memmap`…」未带适用腿。切片 L107 明确该断言只在两条 UEFI 腿的 `prepare_and_enter_kernel_image` 内运行，不在 `prepare_boot`、也不在两个直核面，其余三腿另有机制，并说「排查时不要按它去找」；L182 小结再次强调「只在两条 UEFI 腿的发送侧运行」。照摘要会在错误腿上找该消息。最小修法：补「仅两条 UEFI 发送腿；其余三腿没有此消息」。
2. 摘要 L43「`mmu` 给页表遍历」丢掉切片 L156 的限定：`page` 是 user-mode 专用项，system 模式不产生页表遍历。启动诊断恰在 system 模式，照摘要会误期待页表遍历。最小修法：改为「`mmu` 给 MMU/页表活动记录；`page` 仅 user-mode，system 模式无页表遍历」。
3. 摘要 L12–L13 的关键发送侧路标漏了切片 L41 的 `memory map left N range(s) out of both ordinary tables`（「EBS 成功后 `Side` 侧桶去向的唯一说明行」），L37 的 WB/Side 条目也没有这个信号。最小修法：补进发送侧关键行或 Side 条目。

越界（说了切片没有的话）
4. 摘要 L46「`DTB_CFGTABLE=1` 强制走 DTB 配置表分支；`SKIP` 只表示宿主 U-Boot 不兼容，不是链失败」。切片 L118 只说 DTB_CFGTABLE=1 形态是「DTB preflight 拒绝行」，L127 只说「宿主兼容缺口时打印 SKIP 原因」；「强制走分支」「只表示」「不是链失败」均为摘要添加，且会盖掉 L118 的拒绝行预期。最小修法：按切片措辞改写或删去推断。
5. 摘要 L16「不为此增加打印」与 L52「不增打印的约定」不在切片 L65；L65 只说「见 §5.5 的已知诊断边界」。最小修法：改为「见 §5.5」，不把章外裁决写成章内事实。

通过项：EBS 两通道与 `raw_serial`/`raw_serial_line` 差异、panic 四约束与两侧注册点、§6.3 表各行（C 两条、取件/分配/v1 解码/4 GiB/大页/切页/WB/KernelInfo 全零）、三层诊断命令与 aarch64 直核不在五门内的边界，均能在切片找到对应。
