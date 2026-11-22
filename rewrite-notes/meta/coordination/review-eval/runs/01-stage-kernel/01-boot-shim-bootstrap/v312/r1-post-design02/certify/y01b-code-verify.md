# y01b 代码独立复验（D01 post-DESIGN-02 R1）

VERDICT: PASS

- 时间：2026-11-23 05:53 CST
- commit：`bb3b6e56a0f71d51e11955cfc66b5f5be9d7c5e5`（HEAD，与 `git rev-parse HEAD` 一致）
- 工作区状态：`git diff HEAD -- os` 为空，已跟踪源码与 HEAD 一致；不存在 `git diff bb3b6e56a^ bb3b6e56a -- os` 之外的源码改动。工作区另有 4 个已跟踪 review 元数据文件处于修改态（`loop.md`、`metrics.md`、`events.csv`、`state/...yaml`）和若干未跟踪文件（`01-design-learn.md`、`unit-evidence/` 等），均为核验期间并发产生，不是本次复验写入。本报告只写入 `tmp/.../certify/y01b-code-verify.md`。
- 证据记录：`tmp/review-loop/01-stage/evidence/y01b-qemu-*.log` 在核验开始（05:44 前后）时头部仍为修复前 `01dcb5252`、时间 05:37；核验过程中被并发重写为 `bb3b6e56a`、时间 05:48–05:50。下面的 QEMU 结论以本 verifier 自己在工作区=HEAD 上重跑的 `run_qemu.sh` 结果为准，不依赖这些日志头部。

## 逐项

### 1. commit 范围与三处禁改路径 — PASS（L1）

命令与输出：

```
$ git show bb3b6e56a --stat | head -22
 os/README.md                                       |   2 +-
 os/boot-shim/src/lib.rs                            |  11 +-
 os/boot-shim/src/opensbi_helpers.rs                |  48 +++----
 os/boot-shim/src/uefi_helpers.rs                   |  39 ++----
 os/kernel-image/Cargo.toml                         |   4 +-
 os/kernel-image/src/bootface.rs                    |   4 +-
 os/kernel-image/src/bootface_common.rs             |   4 +-
 os/kernel/src/lib.rs                               |  21 ++-
 os/libs/minix-boot/src/kernel_info.rs              |  51 +++----
 .../bootstrap/test-higher-half-aarch64/src/main.rs |  15 +--
 .../bootstrap/test-higher-half-riscv64/src/main.rs |   9 +-
 .../kernel/bootstrap/test-higher-half/src/main.rs  |  11 +-
 .../01-stage-kernel/01-boot-shim-bootstrap.md      | 146 +++++++++++----------
 .../v312/r1-post-design02/defect-ledger.md         |  88 +++++++++++++
 14 files changed, 248 insertions(+), 205 deletions(-)

$ git show bb3b6e56a --name-only --pretty=format: | grep -v '^os/'
rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r1-post-design02/defect-ledger.md

$ git diff bb3b6e56a^ bb3b6e56a --stat -- minix3 rewrite-notes/02-stage-vm rewrite-notes/03-stage-rs
（无输出，exit 0）
```

改动面只有 `os/` 源码/注释/载体、本篇文档与缺陷台账；`minix3/`、`rewrite-notes/02-stage-vm/`、`rewrite-notes/03-stage-rs/` 零改动。PASS。

### 2. 旧符号清零 — PASS（L1）

```
$ rg -n 'build_kernel_info|report_static|BootStorageCell|\bSTORAGE\b|KernelInfo<'\''static>|validate_impl_inner' os --glob '*.rs'
（无输出，exit 1）
```

六个旧符号在 `os/` 的 Rust 源中全部零命中。PASS。

### 3. DESIGN-02(b) 形态 — PASS（L1）

- 五条生产腿发送侧均构造/借用 `BootReport<'_>`：
  - 两条 UEFI 腿共用 `prepare_and_enter_kernel_image`：`uefi_helpers.rs:1134` 与 `:1239` 均为 `storage.report(...)`，`write_envelope_report(&report)`。
  - U-Boot shim 腿：`opensbi_helpers.rs:425-426` 的 `BootPrepareResult { report: storage.report(...) }`，`uboot_main.rs:496-525` 对 `result.report` 序列化。
  - 两个直核面共用 `bootface_common.rs:547-554`：`storage.report(...)` 后 `report.validate_requiring_sources(DTB)`，再传给 `arch_boot`。
- 接收侧 `land_and_store` 深拷六个切片进 `.bss`：`os/kernel/src/lib.rs:1602-1703` 依次落 `reserved_regions`（`land_slice`，1604）、`memmap`（1616）、`boot_modules`+名字池（1620-1656）、`platform_sources`（1663）、`param_buf`（1666-1675，先清零再复制）、`source_records`（1684）；随后用 `.bss` 指针回填 `KernelInfo` 六字段。
- 两个 wrapper 都在：`store_kernel_info`（1562-1564，`land_and_store(kernel_info, kernel_info.authority)`）与 `store_boot_report`（1568-1573，`land_and_store(report, authority)`）。
- `arch_boot_finish` 门序（`lib.rs:228-237`）：`report.validate_before_tables()` → `report.check_boot_postconditions()` → `store_boot_report(report, authority)` → `register_panic_diagnostic()` → `arch_boot_impl` → `H::jump_to_kmain`。
- `validate_view` 的 authority 只在 strict 分支读取：函数体内 `authority` 仅出现在签名（282）和 `if level.is_strict()` 块（479-480）；非 strict 路径传 `None`，从不读取。

PASS。

### 4. DESIGN-04/05 校验 API — PASS（L1）

定义只剩五个：

```
$ rg -n 'fn validate\b|fn validate_requiring_sources|fn validate_before_tables|fn validate_view' os/libs/minix-boot/src/kernel_info.rs
260:    pub fn validate(&self) {
276:    pub fn validate_requiring_sources(&self) {
280:    fn validate_view<V: KernelInfoView>(
618:    pub fn validate_before_tables(&self) {          // BootReport
622:    pub fn validate_requiring_sources(&self, authority: RamAuthority) {  // BootReport
```

`KernelInfo::validate_before_tables` 与 `BootReport::validate` 已删；`BootReport::validate_before_tables()`（无 authority）与两个 strict 入口保留。旧调用清零：

```
$ rg -n 'KernelInfo::validate_before_tables|BootReport::validate\b|report\.validate\(' os rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
（无输出，exit 1）

$ rg -n '\.validate_before_tables\(' os --glob '*.rs'
os/kernel/src/lib.rs:228:    report.validate_before_tables();
```

全仓唯一 pre-table 调用是无参形式。PASS。

### 5. DESIGN-06/07 — PASS（口径说明，L1）

`bootface_common.rs` 与 boot-shim 的目标均达成：

```
$ rg -n 'param_ptr' os/kernel-image/src/bootface_common.rs
（无输出，exit 1）

$ rg -n 'test_kernel_info\(' os/boot-shim/src
（无输出，exit 1）
```

说明：prompt 的“`param_ptr` 零命中”若按全 `os/` 字面执行，仍有 2 处命中：`os/kernel/src/lib.rs:726`（`let param_ptr = param_store.as_ptr();`）与 `:744`（`param_buf: from_raw_parts(param_ptr, param_len)`）。这两处是 v1 接收路径 `arch_boot_resume_v1` 解码后构造 `KernelInfo` 的合法局部变量，有真实读者，不是被删除的 `bootface_common` 死变量；台账 R1-07 的复验口径也只限定 `bootface_common.rs`。按缺陷意图判 PASS，字面全仓计数为 2 属口径差异，不是缺陷。

`test_kernel_info` 在 kernel 侧的同名测试 helper（`kerninfo.rs:213`）按 prompt 明确不计；boot-shim 内零命中。被改的 uefi/opensbi 测试直接断言 `report.memmap`/`report.kern_*`/`report.boot_modules`，不再经 `KernelInfo` 转手。

### 6. DESIGN-03/09、README、Cargo 注释 — PASS（L1）

```
$ rg -n "KernelInfo<'static>|build_kernel_info|report_static|BootStorageCell|validate_impl_inner" \
    os rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md --glob '*.rs' --glob '*.md'
（无输出，exit 1）
```

- README：`os/README.md:43` 现为 “Boot protocol types (KernelInfo, BootStorage/BootReport, wire v1)”。
- Cargo 注释：`os/kernel-image/Cargo.toml:52`、`:61` 已由 `build_kernel_info` 改为 “BootStorage 装配”。
- DESIGN-09：目标句 `新增的 \`GET_KMESSAGES\`/\`VmBootHandoff\`` 在文档 `:1256` 存在（`rg` 命中 1）。

PASS。

### 7. QEMU 载体 — PASS（L1）

三个载体源码均为 `BootReport` + 三参数 `arch_boot`：

```
test-higher-half:          use minix_boot::{BootReport, RamAuthority};  arch_boot(&report, RamAuthority::UEFI, root_page)
test-higher-half-aarch64:  use minix_boot::{BootReport, RamAuthority};  arch_boot(&report, RamAuthority::UEFI, root_page)
test-higher-half-riscv64:  use minix_boot::{BootReport, MemoryRegion, RamAuthority};  arch_boot(&report, RamAuthority::DTB, root_page)
```

release 构建（本次重跑，工作目录 `os/`）：

```
$ cargo build -p test-higher-half --release --target x86_64-unknown-uefi --features fw-x86-uefi
Finished `release` profile [optimized] target(s) in 0.05s        EXIT 0
$ cargo build -p test-higher-half-aarch64 --release --target aarch64-unknown-uefi --features fw-aarch64-uefi
Finished `release` profile [optimized] target(s) in 0.05s        EXIT 0
$ cargo build -p test-higher-half-riscv64 --release --target riscv64gc-unknown-none-elf --features fw-riscv64-none
Finished `release` profile [optimized] target(s) in 0.08s        EXIT 0
```

`run_qemu.sh` 本次重跑（工作区源码 = HEAD）：

- x86_64：串口出现 `kernel: arch_boot entered`、`step4 DM coverage ok`、`### TEST_RESULT: PASS test-higher-half ###`、`✅ TEST PASSED`，EXIT 0。
- aarch64：`step2c device window empty (skip)`、`step4 DM coverage ok`、`### TEST_RESULT: PASS`、`✅ TEST PASSED`，EXIT 0。
- riscv64：`kmain_verify: reached!`、`### TEST_RESULT: PASS`、`✅ TEST PASSED`，EXIT 0。

evidence 目录的三份 `y01b-qemu-*.log` 当前头部已是 commit `bb3b6e56a` 且各含 1 个 PASS 标记；核验开始时它们记录的是 `01dcb5252`，此点已在顶部说明，结论不依赖该头部。

### 8. 测试与构建 — PASS（L1）

在 `os/` 本机 Rust 1.94.1 重跑：

```
$ cargo test -p minix-boot --lib
test result: ok. 188 passed; 0 failed; 0 ignored
$ cargo test -p boot-shim --lib
test result: ok. 136 passed; 0 failed; 0 ignored
$ cargo test -p minix-kernel --features mock
test result: ok. 866 passed; 0 failed; 8 ignored        (lib)
test result: ok. 3 passed; 0 failed; 3 ignored          (boot_integration)
test result: ok. 4 passed; 0 failed; 0 ignored          (init_protection_order_pin)
test result: ok. 2 passed; 0 failed; 0 ignored          (set_addr_space_no_boot_alloc_pin)
test result: ok. 1 passed; 0 failed; 0 ignored          (store_before_alloc_pin)
test result: ok. 0 passed; 0 failed; 1 ignored          (doctest)
all doctests ran; FAILED 计数 0
$ cargo check -p minix-kernel
Finished `dev` profile ... (exit 0)
$ cargo build -q --release -p kernel-image --target riscv64gc-unknown-none-elf --features fw-riscv64-none
EXIT 0
$ cargo build -q --release -p kernel-image --target aarch64-unknown-none --features fw-aarch64-none
EXIT 0
```

读数与 prompt 期望一致（188/136/866+8，全部二进制 0 failed）。PASS。

### 9. 行为等价（`cdd0d1c77^` 旧 `validate_impl_inner`） — PASS（L1/L2）

以 `git show cdd0d1c77^:os/libs/minix-boot/src/kernel_info.rs` 提取旧函数体，与当前 `validate_view` 做规范化 diff（仅做 `self.`→`view.`、字段→访问器的语法归一）：

- 字符串字面量按序对比：旧 35 条，新 36 条；唯一新增是 strict 分支的
  `"KernelInfo: strict validation requires a declared RAM authority"`。
  所有既有断言的文本、顺序完全一致。
- 条件与顺序一致：`kern_size > 0` → `memmap` 非空 → platform_sources 三项检查 → 两表升序/两两不相交 → 跨表不相交 → 内核-模块/模块-模块半开区间 → 非零长度与页对齐 → `requires_boot_modules` 时的模块非空 → `param_buf <= 1024` → strict 来源审计 → `requires_kernel_span_coverage` 的内核跨度覆盖。
- 唯一语义差异：旧 strict 分支 `let authority = self.authority;`；新分支 `authority.expect(...)`。全部 strict 调用点都传 `Some`（`KernelInfo::validate_requiring_sources` 传 `Some(self.authority)`；`BootReport::validate_requiring_sources` 传 `Some(authority)`），strict+None 不可达，因此在可达路径上等价，新增的只是不可达分支的 fail-closed 消息。
- 落地函数：`land_slice` 与 `cdd0d1c77^` 逐字相同（脚本比较 `land_slice identical: True`）；一次性守卫文本与同址跳过逻辑未变。旧 `store_kernel_info` 的函数体除 `let mut k = *kernel_info;` 改为显式构造 `KernelInfo`（六个切片置空后回填、标量取自 view、authority 显式传入）外，六切片拷贝、容量断言、名字池截断、param 清零、来源落盘全部一致；`store_kernel_info` 与 `store_boot_report` 都收敛到同一个 `land_and_store`。

PASS（未发现放宽或删除的断言）。

### 10. 文档-代码同步（§4.4/§4.5/§3.7） — PASS（L1，附 P3 观察）

- §4.4 `arch_boot_finish` 签名与 `KernelInfoView` trait 与源码一致；`report.validate_before_tables()` → `check_boot_postconditions()` → `store_boot_report` 的顺序与 `lib.rs:228-231` 一致。九字段视图（六切片 + 三标量）与 `BootReport` 定义（`boot_storage.rs:292-314`）一致；`KernelInfo` 十字段（含 authority）与 `kernel_info.rs:20-149` 一致。
- §4.4.1 直核面次序 strict → pre-table → postconditions → store_boot_report → boot_validate → kmain validate → kmain postconditions 与 `bootface_common.rs:552`、`lib.rs:228-237`、`lib.rs:1864-1870` 一致；v1 接收面 strict → postconditions → resume_boot_pt_alloc → resume_additive → store_kernel_info → kmain validate → postconditions 与 `lib.rs:747-750`、`lib.rs:308-325` 一致。
- §4.5 `land_and_store`/`store_kernel_info`/`store_boot_report` 的签名逐字一致；六个落地区的容量常量（256/128/16/4/1024/296）与 `globals.rs:297-400` 一致；同址跳过与一次性守卫的叙述与源码一致。
- §3.7 的逐腿门序与上述源码一致。

PASS。三处 P3 精度观察见下节，均不影响事实结论。

## 新发现

P3-1（文档精度，§4.4 代码引文块的省略说明）
- 位置：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:2293-2304`。
- 证据：引文头写“略去函数体中的诊断打印与注释”，但块内也省略了 `register_panic_diagnostic();`（源码 `os/kernel/src/lib.rs:231`）与 `where` 子句。同一函数在 §3.6 `:1690` 给出了包含该调用的完整形式，故不构成事实错误。
- 最小修复：把省略说明改为“略去诊断打印、`register_panic_diagnostic()` 与注释”，或保留该调用行。

P3-2（文档精度，§4.5 代码引文块的省略说明）
- 位置：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:2405-2430`。
- 证据：块头只列“六个切片的 `land_slice` 调用与模块名/param 两块的细节省略”，但块内也省略了 `land_and_store` 顶部的一次性守卫（源码 `os/kernel/src/lib.rs:1583-1586`）。该守卫在表后正文 `:2462` 有完整叙述，故不构成事实错误。
- 最小修复：块头补“一次性守卫”或把守卫的 `assert!` 三行保留。

P3-3（文档完整性，§3.7 v1 链尾）
- 位置：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:1850`。
- 证据：§3.7 的 v1 接收链止于“`kmain` 的 `validate`”，未写 kmain 随后还会调用 `check_boot_postconditions`；§4.4.1 `:2380` 的权威链已写“`kmain` 的 `validate` → `check_boot_postconditions` 复核”，源码 `lib.rs:1864-1870` 也确为两步。属压缩叙述漏一项，不是错误陈述。
- 最小修复：在 §3.7 v1 链尾补“→ `check_boot_postconditions` 复核”。

代码类新发现：0（P0/P1/P2 均为 0）。

## 残余风险

- 本机工具链为 rustc/cargo 1.94.1（与 `minix-ci:1.94` 同版本），但未按 AGENTS 的权威配方在 docker `-m 2g -j 1` 下重跑；且构建命中了增量缓存，不是 clean rebuild。若 CI 环境差异敏感，需补 docker 门。
- evidence 三份 QEMU 日志在核验期间被并发重写（05:49–05:50），本报告不以其 commit 头为唯一凭据；结论建立在本次独立 `run_qemu.sh` 重跑上。若后续有人再次改写这些日志，应以源码与重跑输出为准。
- 工作区的 4 个已跟踪 review 元数据修改与新增未跟踪文件不是本 verifier 写入；若以“工作区状态”作为后续门禁输入，需要在收口时重新取 `git status`。
- `param_ptr` 的全仓字面口径仍有 2 处合法命中（v1 接收路径）；本报告按台账 R1-07 的 `bootface_common.rs` 口径判 PASS，已在第 5 项说明。
- 未对照 `minix3/` C 源码复核行为基线；本任务只验证 DESIGN-02/04/05/06/07/09 的代码落点与文档同步，C 行为基线由事实面板负责。

## 未覆盖

- docker 权威编译门（`minix-ci:1.94` 挂载仓库根、工作目录 `os/`、`-m 2g -j 1`）未重跑。
- 从空 `target/` 的 clean build 未做（本机重跑命中缓存）。
- 36 条面板 FINDING 的逐条复算未重做；本轮只按 prompt 的 1–10 项验证。
- 文档机械门（DP-1..DP-6、教程 rubric、出版检查）与 `minix3/` 原 C 行为对账不在本 verifier 范围。
- aarch64/riscv64 的 `kernel-image` 只做了 release 编译，未各自启动完整 QEMU 链（prompt 只要求 build）；三支 `test-higher-half*` 已实跑 PASS。
