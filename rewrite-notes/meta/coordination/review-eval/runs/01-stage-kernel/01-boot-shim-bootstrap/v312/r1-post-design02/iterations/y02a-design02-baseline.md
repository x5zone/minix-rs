# D01 y02a 简报：DESIGN-02（b）与基线重设

## y02a — DESIGN-02（b）Manuscript 设计批完成，D01 基线重设，新 R1 面板已派发

- 作者裁决：选 A，授权 Manuscript 设计批；优先 (b)，回退仅限“证据证明无法共享校验或无法
  完成 `.bss` 落地”。授权写入 `DECISIONS.md` D-119。
- 结果：**(b) 成功，未回退 (a)**。五条生产腿的发送侧统一为 `BootReport<'_>` 借用视图；
  接收侧 `store_boot_report` 先把六个切片深拷进内核 `.bss`，再构造唯一的
  `KernelInfo<'static>`。删除 `BootStorageCell`、`report_static`、`STORAGE` 与直核
  `build_kernel_info` 路径；`KernelInfoView` 承载共享校验。
- 旧符号：`rg 'build_kernel_info|report_static|BootStorageCell' os --glob '*.rs'` → 0；
  `rg '\bSTORAGE\b' os --glob '*.rs'` → 0。
- 测试：`minix-boot --lib` 188/0；`boot-shim --lib` 136/0；`minix-kernel --features mock`
  866/0/8（全部测试二进制 876/0/12）；`cargo check -p minix-kernel`（非 mock）通过。
- QEMU 启动链：x86_64 UEFI、aarch64 UEFI、riscv64 U-Boot shim、riscv64 bootface 四门 PASS
  （日志 `tmp/review-loop/01-stage/evidence/design02-qemu-*.log`）。
- 文档同步：§3.2/§3.2.1/§3.7/§4.4/§4.4.1/§4.5/§4.6/§7.5；doc 2,946 行，sha256
  `996ff161…`；机械门 rubric 0、publication 0、anchor 87/87、ref OK、density OK、
  citation failed=0、style error=0。
- 旧 R1 按 `review-publication.md` §四.6 标 `INVALIDATED`，不继承旧 48 FINDING/40 根因。
- 新基线：commit `cdd0d1c77`（设计批）、`df3441a57`（基线重设与 R1 重开）；state 48 单元、
  0 冻结、`round=1`、`full_rounds=1`；新 run 目录 `v312/r1-post-design02/`。
- 新 R1 面板：6 角色 fresh（P-FACT、READER-A/B/C、DESIGN-D2、P-REFUTE）+ 独立设计批
  verifier 并行派发；角色事件已 plan，报告目标
  `tmp/review-eval/.../panel/v312-r1-post-design02/`，verifier 目标
  `tmp/review-loop/01-stage/certify/design02-verify.md`。
- 下一步：面板入库 → 归因 → 修复 → 复验 → 冷读。北京时间 09:00 进入峰值计费，届时若未收口，
  写交接并暂停，不继续消耗峰值额度。
