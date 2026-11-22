# D01 y02b 简报：DESIGN-02 独立复核 FAIL

## y02b — DESIGN-02 独立复核 FAIL：两项阻断待面板后修复

- 独立设计批 verifier（fresh context，不读完成记录先下结论）核对 commit `cdd0d1c77`、
  doc sha `996ff161…`，10 个必查项中 8 项 PASS，第 8 项 FAIL，另加一项阻断。
- 已 PASS：旧符号清零；五条生产腿发送侧都只构造 `BootReport<'_>`；接收侧六切片全量深拷
  进 `.bss` 后才构造 `KernelInfo<'static>`；`arch_boot_finish` 门序正确；`validate_view`
  与旧断言文本/顺序/条件一致；wire/ABI/启动次序未改；三条宿主测试数字 188/0、136/0、
  866/0/8 与文档一致；无 `minix3/`、无 02/03 stage、无新 defer；实现确为 (b)。
- FAIL 1（文档同步）：§4.4 `:2368` 仍写 `validate_impl_inner`；`:2284` 写“接收侧统一走
  `store_boot_report`”，实际三条 v1 腿走 `arch_boot_resume_v1` → `store_kernel_info`，
  只有两个直核腿走 `store_boot_report`；`:2324` 的 `source_records` 行同样写错。代码/
  Cargo 还留有旧签名与旧 API 注释（`boot-shim/src/lib.rs:7/91`、`opensbi_helpers.rs:36`、
  `kernel/src/lib.rs:1553`、`kernel_info.rs:152`、`kernel-image/Cargo.toml:52/61`）。
- FAIL 2（回归）：`test-higher-half`、`test-higher-half-aarch64`、
  `test-higher-half-riscv64` 仍以两参数调用 `arch_boot(&kernel_info, root_page)`，E0061；
  `qemu-tests.yml` 的 build 段会红。修复在 D-119 授权的测试固定装置范围内。
- 关键事实修正：五条生产腿是 x86_64 UEFI、aarch64 UEFI、riscv64 U-Boot shim、riscv64 直核、
  **aarch64 直核**；没有 x86_64 直核生产路径（`kernel-image/src/main.rs:364-368` 明写）。
- 残余风险：QEMU 日志未绑定 commit；新直核接收路径没有启动门真正跑到；16 KiB 栈上
  `BootStorage` 余量未实测；`validate_view` 等价性只做人工 diff。
- 当前动作：面板仍在跑；为保持输入冻结，**不先改代码/文档**。面板报告到齐后把两项阻断与
  面板发现一起修，再由独立 verifier 复核。verifier 报告已入库
  `v312/r1-post-design02/certify/design02-verify.md`。
