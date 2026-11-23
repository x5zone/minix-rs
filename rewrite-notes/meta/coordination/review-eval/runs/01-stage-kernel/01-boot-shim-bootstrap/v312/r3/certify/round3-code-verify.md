VERDICT: PASS

# R3 独立代码复验

- 复验者：fresh subagent（只读，未修改仓库源码）
- 复验对象：`rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r3/defect-ledger.md` 的 DESIGN-01..04
- 基线：工作区 `git status --short` 与复验开始时一致（仅有本轮既有改动，无本复验新增改动）
- 工具链：cargo 1.94.1 / rustc 1.94.1（与 `minix-ci:1.94` 同版本）
- 结论速览：DESIGN-01 PASS、DESIGN-02 PASS、DESIGN-03 PASS、DESIGN-04 PASS（RECHECK 后）；三套宿主测试、`cargo check`、clippy 与四道 QEMU 门按各自判据 PASS。
- 复验历史：首次复验 DESIGN-04 因 `bootface_common.rs:531` 残留旧注释判 FAIL；该行修正后只读复核通过，改判 PASS，详见文末 RECHECK。

## DESIGN-01：authority ↔ source_records 绑定断言 — PASS

代码位置：

- `os/kernel/src/lib.rs:230-239`：`arch_boot_finish` 在 `store_boot_report`（`:240`）之前断言；`:234` 的 `if !report.source_records.is_empty()` 是空记录 lenient 分支。
- `os/libs/minix-boot/src/kernel_info.rs:224-237`：新增 `authority_has_source_record`，逐条比较 `record.input_object == authority.input_object()`。
- `os/libs/minix-boot/src/kernel_info.rs:1134-1154`：单元测试 `authority_binding_requires_a_record_from_the_declared_input`，覆盖 DTB 命中、UEFI 不命中、空记录不命中。

五条生产腿是否会被误触发（逐腿核对）：

- x86_64 UEFI：`os/boot-shim/src/uefi_helpers.rs:1130` 的 `Serializer::new(..., InputObjectId::UefiMemoryMap)` 直接把分类器记录写入 wire，`:1278` 调 `enter_kernel_image_v1`；接收侧走 `arch_boot_resume_v1`，不经过新增断言，接收侧另有严格门 `os/kernel/src/lib.rs:757` 的 `kinfo.validate_requiring_sources()`。
- aarch64 UEFI：同一 `prepare_and_enter_kernel_image` 路径，机制同上。
- riscv64 U-Boot shim：`os/boot-shim/src/uboot_main.rs:485` 用 `InputObjectId::DtbBootInput` 建 Serializer，`:509` 写 `report.source_records`，`:525` 进 `enter_kernel_image_v1`；同样不经过新增断言。
- riscv64 bootface（生产直核面）：`os/kernel-image/src/bootface_common.rs:540-542` 从 DTB 收集器填入 records，`:553` 已用 `RamAuthority::DTB` 跑严格门，`:555` 调 `arch_boot`。records 全部由 `os/libs/minix-boot/src/memmap_source.rs:698-716` 的 `push_source` 产出，`input_object` 恒为 `DtbBootInput`，与 DTB authority 匹配。
- aarch64 bootface_a64（生产直核面）：`os/kernel-image/src/bootface_a64.rs:141` 进入同一个 `run_self_boot`，同 DTB 路径，同上匹配。

空记录测试内核：

- `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs:62`、`test-higher-half-aarch64/src/main.rs:58`、`test-higher-half-riscv64/src/main.rs:135` 均传 `source_records: &[]`；新增断言的 `:234` 守卫会跳过，仍走 lenient 分支。三者的 `Cargo.toml` 都启用 `minix-kernel` 的 `qemu_test`，使用 naked kmain，不进入生产 kmain 的严格门。

命令与输出：

- `cargo test -p minix-boot --lib` → `test result: ok. 190 passed; 0 failed; 0 ignored`。
- `cargo test -p minix-boot --lib authority_binding -- --nocapture` → `authority_binding_requires_a_record_from_the_declared_input ... ok; 1 passed; 189 filtered out`。

## DESIGN-02：生产 kmain 参数绑定与临时取证块 — PASS

代码位置：

- `os/kernel/src/lib.rs:1830`：参数名恢复为 `kernel_info`（不再是 `_kernel_info`）。
- `os/kernel/src/lib.rs:1839-1845`：先取全局 `stored`，再 `core::ptr::eq(kernel_info, stored)` 断言，随后以 `let kernel_info = stored` 读取；`store_kernel_info` 的一次性守卫与注释一致。
- `git diff -- os/kernel/src/lib.rs` 显示 “Evidence capture (temporary, v3)” 整块与 “This shadow can be deleted…” 过程注释已删除；`rg -n '_kernel_info' os --glob '*.rs'` 只剩 `store_kernel_info`/`kernel_info()` 等无关标识，无该参数名。
- `os/kernel/src/boot/higher_half.rs:37-46`：`jump_to_kmain` 合同仍是 “kinfo 作为第一实参传给 kmain”，与新的 “参数与全局同址、全局是唯一权威” 不矛盾；三架构实现的注释也只描述传参寄存器，不再声称 kmain 只读参数。

命令与输出：

- `cargo check -p minix-kernel` → 退出码 0，`Finished dev profile`；0 error，87 warnings（存量 warning，非本次新增）。

运行时旁证：x86_64 串口日志 `os/target/image/x86_64/serial_nk1x.log:26-31` 依次出现 `kmain Phase A enter` 与 `A.1 validate ok`，说明 `ptr::eq` 断言在真实启动中未触发。

## DESIGN-03：Serializer 死计数器 — PASS

代码位置：

- `os/libs/minix-boot/src/handoff_v1_write.rs:99-104`：`Serializer` 只剩 `wire_memmap_n`、`reserved_n`、`sources_n`，三个死计数器与初始化赋值均已删除。
- byte_len 行为未变：`write_boot_modules` 在 `:218-222` 用 `modules.len() * 32` 写 `MODULES_BYTE_LEN`；`write_platform_sources` 在 `:240-243` 用 `sources.len() * 16`；`write_param_buf` 在 `:258` 用 `bytes.len()`。均不依赖被删计数器。

命令与输出：

- `rg -n 'modules_n|platform_n|param_n' os/libs/minix-boot/src/handoff_v1_write.rs` → 0 行（exit 1）。
- 全仓同名扫描的 3 个命中均为无关标识（`test_validate_cpu_param_none_always_ok`、`platform_number`），不属于 Serializer。

## DESIGN-04：五处过期注释 — PASS（RECHECK 后）

五处均已改：

- `os/boot-shim/src/main.rs:63`：`seven payload partitions` 已改为 `six payload partitions`。
- `os/kernel-image/src/bootface_common.rs:334-335`：共享流程的 `KernelInfo → arch_boot` 已改为 `BootReport<'_> → arch_boot`，并说明接收侧落 `.bss` 后构造 `KernelInfo`。
- `os/kernel-image/src/bootface_common.rs:531-532`：第二处发送侧注释已由 `// 6. KernelInfo plus the real arch_boot gate.` 改为 `// 6. BootReport plus the real arch_boot gate (the receiver lands it and // builds KernelInfo).`；`git diff` 确认该 hunk 仅注释、无代码改动。
- `os/kernel-image/src/bootface_a64.rs:39-44`：旧的 `12 modules→KernelInfo` 与 `left to a separate cleanup increment` 已改写为 `BootReport<'_>` 与共享 `bootface_common` 的现状说明。
- `os/libs/minix-boot/src/kernel_info.rs:262-266`：`Called by kmain at entry` 已改为 lenient/test gate 说明，并指向 `validate_requiring_sources`。
- `os/kernel/src/lib.rs`：kmain 中带 `bootstrap_len==0` 的临时取证注释已随 DESIGN-02 删除；`:1986-1989` 的现行注释明确 “`KernelInfo` has no bootstrap-span fields”，与结构一致。

命令与输出（RECHECK 复核）：

- `rg -n 'BootReport plus the real arch_boot gate|KernelInfo plus the real arch_boot gate' os/kernel-image/src/bootface_common.rs` → 仅 `:531` 的 `BootReport plus the real arch_boot gate`，旧短语 0 行。
- `rg -n 'KernelInfo plus the real arch_boot gate|seven payload|left to a separate cleanup|Called by .*kmain|Evidence capture|can be deleted' os --glob '*.rs'` → 0 行（exit 1），PASS。
- `git diff -- os/kernel-image/src/bootface_common.rs` → 两个 hunk 均为注释行改动（`:331-335`、`:528-532`），无行为变化。

## 回归测试与启动门 — PASS（按各自判据）

宿主测试：

| 命令 | 结果 |
|---|---|
| `cargo test -p minix-boot --lib` | 190 passed / 0 failed / 0 ignored |
| `cargo test -p boot-shim --lib` | 136 passed / 0 failed / 0 ignored |
| `cargo test -p minix-kernel --features mock --lib` | 866 passed / 0 failed / 8 ignored |

四道 QEMU 门（日志均晚于源码修改时间 19:14-19:17，门运行于 19:18-19:19）：

| 日志 | 结果行 | 判定 |
|---|---|---|
| `tmp/review-loop/01-stage/logs/r3-x86-shim.log` | `:2617 ### TEST_RESULT: PASS test-shim-bootmarks-x86 ###` | PASS；底层串口 `os/target/image/x86_64/serial_nk1x.log:49` 有 `entering scheduler`，满足脚本 MARK5 |
| `tmp/review-loop/01-stage/logs/r3-aarch64-shim.log` | `:3243 ### TEST_RESULT: PASS test-shim-bootmarks-aarch64 ###` | PASS（该脚本只判 shim 侧 MARK1-4）；底层串口 `os/target/image/aarch64/serial_m33a.log:54` 已到 `entering scheduler` |
| `tmp/review-loop/01-stage/logs/r3-riscv-uboot-shim.log` | `:15 RESULT: PASS (D-73 ... 6/6 markers, entering scheduler)` | PASS |
| `tmp/review-loop/01-stage/logs/r3-riscv-bootface.log` | `:922/924/926 PASS`、`:927 ### TEST_RESULT: PASS test-kernel-image-riscv64-bootface ###` | PASS；该腿走 `arch_boot_finish`，是 DESIGN-01 断言在真实直核腿上的正向证据 |

四份日志均无 `panicked at`/`FAIL`/`assertion` 行。附一条门后观察（不影响本门判据）：aarch64 底层串口 `serial_m33a.log:67` 在 `entering scheduler` 之后有 `PANIC ... aarch64 user sync exception ... trap_dispatch.rs:3520`；四道门中的 aarch64 门按脚本定义只要求 shim 侧路标，且 kmain 的 A.1..D 全部通过，该 panic 落在用户链 VM 初始化，不在本次四项修复的改动路径上；建议由 owner 确认是否为已登记的 A2 用户链问题，本条不作为 DESIGN-01..04 的失败依据。

## 编译与 clippy — PASS

- `cargo check -p minix-kernel` → exit 0；0 error；87 warnings（存量）。
- `cargo clippy -p minix-boot --lib` → exit 0；8 warnings，0 error；新增的 `authority_has_source_record` 与测试未产生新 warning。
- `cargo clippy -p minix-kernel --features mock --lib` → exit 0；89 warnings，0 error（含 `warn[E0133]` 级别的存量 unsafe 诊断，不是 error）。

## RECHECK: PASS

- 复核对象：`os/kernel-image/src/bootface_common.rs:531-532`。现文为 `// 6. BootReport plus the real arch_boot gate (the receiver lands it and` / `// builds KernelInfo).`，与同文件 `:547-555` 的发送侧实际形状一致。
- 变更范围：`git diff -- os/kernel-image/src/bootface_common.rs` 显示仅两个注释 hunk（`:331-335` 与 `:528-532`），无代码行为变化。
- 零命中命令：`rg -n 'KernelInfo plus the real arch_boot gate|seven payload|left to a separate cleanup|Called by .*kmain|Evidence capture|can be deleted' os --glob '*.rs'` → 0 行（exit 1）。
- 复验结论：DESIGN-04 改判 PASS，整体 VERDICT 改判 PASS。DESIGN-01..03、宿主测试（minix-boot 190/0、boot-shim 136/0、minix-kernel mock 866/0/8）、`cargo check`、clippy 与四道 QEMU 门结论不变；本次为纯注释改动，无需重跑这些门。
