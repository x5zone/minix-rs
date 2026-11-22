# DESIGN-02 (b) 独立验证报告

- VERDICT: **FAIL**
- 时间：2026-11-23 05:12 CST
- 核对 commit：`cdd0d1c777042b048bd9fdfac3e5102b95dac869`（工作区 HEAD `df3441a57`；`git diff cdd0d1c77 df3441a57 --stat` 只动 9 个 review 元数据文件，`os/` 代码无后续改动）
- 文档：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`，2,946 行，sha256 `996ff161d02ef867151e780adb7243e49e7dc249b5b161d6d0c5ed229b5f22ec`
- 方法：只读源码 + git 只读命令 + 独立重跑宿主测试 + 读 QEMU 证据日志 + 构建三个 QEMU 载体验证调用点。未修改任何仓库文件，只写本报告。

## 总判定依据

10 个必查项中第 8 项不通过，另有额外阻断发现 A：

- **第 8 项 FAIL**：指定同步的 §4.4 仍出现旧符号 `validate_impl_inner`，且“接收侧统一走 `store_boot_report`”与代码及 §4.4.1/§4.5 自相矛盾；代码侧还有多处旧签名/旧 API 文档注释未清。
- **额外阻断发现 A**：这次 API 换签名把仓库内三个 QEMU 测试载体（`test-higher-half`、`test-higher-half-aarch64`、`test-higher-half-riscv64`）编译打断，`os/qemu-tests/run_all.sh` 与 `.github/workflows/qemu-tests.yml` 的构建矩阵因此红。三个指定宿主测试命令本身全绿，但“boot-shim 相关测试全绿”在仓库 CI 意义下不成立。

其余 1–7、9、10 项通过，证据与限定见下。

---

## 1. 旧符号消失

命令与关键输出：

```
$ rg -n 'build_kernel_info|report_static|BootStorageCell' os --glob '*.rs'
（无输出，exit 1）

$ rg -n '\bSTORAGE\b' os --glob '*.rs'
（无输出，exit 1）
```

判定：**PASS**（按任务给定的两条命令）。
证据强度：E1（工具输出）。

补充（不改变本项判定）：把 glob 放宽到整个 `os/` 时有 2 处非 Rust 命中：

```
$ rg -n 'build_kernel_info|report_static|BootStorageCell|\bSTORAGE\b' os
os/kernel-image/Cargo.toml:52:# （BootFileTable/bump 池/build_kernel_info——取页纪律单源，不在镜像侧
os/kernel-image/Cargo.toml:61:# §续-406：aarch64 自引导面同车道（BootFileTable/magic/build_kernel_info
```

两处都是注释，不构成直核发送路径；但 DESIGN-02 计划第 142 行的验收写成 `rg -n 'build_kernel_info' os/` 0 命中，按该更严格口径不成立（见“与完成记录的差异”）。

## 2. 五条生产腿的发送侧

按实际代码，五条生产腿是：x86_64 UEFI、aarch64 UEFI、riscv64 U-Boot shim、riscv64 直核、aarch64 直核。**没有 x86_64 直核生产路径**：`os/kernel-image/src/main.rs:364-368` 明写 “x86 has no direct-boot face”，`arena_pa == 0` 直接 halt。任务文字里的“x86_64 直核”应以实际代码为准替换为 aarch64 直核。

逐腿证据（E2，源码位置）：

| 腿 | 发送侧证据 | 接收侧 |
|---|---|---|
| x86_64 UEFI | `os/boot-shim/src/uefi_helpers.rs:1086` `let mut storage = minix_boot::BootStorage::new();`；`:1134` `storage.report(...)`；`:1278-1283` `enter_kernel_image_v1(arena_pa, &report, root_page, ...)` | `os/kernel/src/lib.rs:663` `arch_boot_resume_v1`，按 `byte_len` 解码进 `.bss` store（`:674-731`），再构造 `KernelInfo`（`:733-746`） |
| aarch64 UEFI | 同一 `prepare_and_enter_kernel_image`，同上行号 | 同一 `arch_boot_resume_v1` |
| riscv64 U-Boot shim | `os/boot-shim/src/uboot_main.rs:475` 栈上 storage；`:476` `prepare_boot(&mut storage, ...)`；`:531` `enter_kernel_image_v1(..., &result.report, ...)`；`opensbi_helpers.rs:425` `report: storage.report(...)` | 同一 `arch_boot_resume_v1` |
| riscv64 直核 | `os/kernel-image/src/bootface.rs:139` → `bootface_common.rs:354` 栈上 storage；`:549` `storage.report(...)`；`:556` `arch_boot(&report, DTB, root_page)` | `arch_boot_finish` → `store_boot_report` |
| aarch64 直核 | `os/kernel-image/src/bootface_a64.rs:141/144` → 同一 `run_self_boot`；`bootface_common.rs:354/549/556` | 同上 |

发送侧不构造 `KernelInfo` 的辅助证据（E1）：

```
$ rg -n "KernelInfo \{" os/boot-shim/src os/kernel-image/src
os/boot-shim/src/opensbi_helpers.rs:638-639   （#[cfg(test)] mod tests，:307 起）
os/boot-shim/src/uefi_helpers.rs:1409-1410    （#[cfg(test)] mod tests，:1398 起）
os/boot-shim/src/ebs_pair.rs:417,838          （#[cfg(test)] mod tests，:307 起）
```

生产函数体内无 `KernelInfo` 字面量；其余命中是测试 helper 与文档注释。

判定：**PASS**。
证据强度：E2（源码位置）+ E1（rg）。

## 3. 接收侧唯一落地与 arch_boot 门序

- `land_and_store`：`os/kernel/src/lib.rs:1578`。六个切片全部深拷：`reserved_regions` `:1605-1607`；`memmap` `:1617-1619`；`boot_modules` 加名字池 `:1620-1660`；`platform_sources` `:1664-1666`；`param_buf`（先清零再复制）`:1667-1678`；`source_records` `:1685-1687`。之后才构造 `KernelInfo` 字面量 `:1688-1699`，切片回填到 `.bss` store 指针 `:1700-1705`。一次性守卫 `:1586-1589`。
- `store_kernel_info` `:1565` 与 `store_boot_report` `:1571-1576` 都只是 `land_and_store` 的 wrapper；前者用 `kernel_info.authority`，后者收显式 `authority`。前者 `pub`（v1 接收面与测试用），后者 `pub(crate)`（直核面用）。
- `arch_boot_finish` `:215-238` 顺序为 `validate_before_tables`（`:228`）→ `check_boot_postconditions`（`:229`）→ `store_boot_report`（`:230`）→ `register_panic_diagnostic`（`:231`）→ `arch_boot_impl`（`:232`）→ `jump_to_kmain`（`:237`）。先校验后 store，成立。
- v1 接收面：`arch_boot_resume_v1` `:663-751` 解码进 store → 构造 `KernelInfo` `:733` → `validate_requiring_sources` `:747` → `check_boot_postconditions` `:748` → `resume_after_v1_decode` `:750` → `resume_additive` `:300-350` 内 `store_kernel_info` `:325`（同址跳过），与文档 §3.7/§4.4.1 一致。
- 三个 `arch_boot` cfg 臂签名一致：x86_64 `:244`、aarch64 `:265`、riscv64 `:796`，均为 `(report: &minix_boot::BootReport<'_>, authority: minix_boot::RamAuthority, root_page: PhysBytes) -> !`。

判定：**PASS**。
证据强度：E2（源码位置）。

附带缺陷（不改变本项判定）：`store_kernel_info` 的文档块 `:1550-1564` 是未清理的旧文本并被重复追加——它仍说 “Production has two call sites: `arch_boot_finish` (a real copy) and `resume_additive`”，但 `arch_boot_finish` 现在调 `store_boot_report`，只有 `resume_additive` 调 `store_kernel_info`。详见第 8 项。

## 4. 共享校验

- `KernelInfoView`：`os/libs/minix-boot/src/kernel_info.rs:186-196`，9 个访问器；`impl for KernelInfo` `:198-208`；`impl for BootReport` `:210-220`。`KernelInfo` 有 10 个字段，第 10 个 `authority` 刻意不在视图内：`KernelInfo` 的三个校验方法传 `self.authority`，`BootReport` 的三个方法收 `authority` 参数（`:619-638`）。就“校验输入”而言 9 个切片/标量加 out-of-band 的 authority 覆盖全部 10 个字段；就字面“访问器与字段一一对应”而言是 9 对 10，这一点必须按设计注记读，不能按字面读。
- `validate_view` `:290-560` 只经访问器读值，不直接读 `KernelInfo` 字段；`check_boot_postconditions_view` `:601-617` 同样。
- 与 `cdd0d1c77^` 的逐行 diff：断言文本、顺序、条件全部保留。检查项包括平台源空址/kind/溢出、两表升序与两两不相交、跨表不相交、内核与模块及模块与模块半开区间、非零长度与页对齐、boot_modules 非空（档位控制）、param_buf 上限、strict 的权威记录覆盖与内核跨度覆盖。未发现被静默放宽或删除的校验。
- `KernelInfo` 路径等价：旧 `validate_impl_inner(&ValidationLevel)` 读 `self.authority`；新 `validate_view(self, self.authority, &level)`，语义一致。

判定：**PASS**（带上述 9/10 注记）。
证据强度：E2（源码位置）+ E1（`git diff` 输出）。

## 5. wire / ABI / 行为不变

命令与关键输出（E1）：

```
$ git diff cdd0d1c77^ cdd0d1c77 -- os/libs/minix-boot/src/handoff_v1.rs \
    os/libs/minix-boot/src/handoff_v1_read.rs os/libs/minix-boot/src/handoff_v1_write.rs | wc -l
0

$ git diff cdd0d1c77^ cdd0d1c77 | rg -n "repr\(C\)|BOOT_HANDOFF_V1_VERSION|NR_SYSCALL|SYS_"
（只命中文档正文的 repr(C) 解释与版本历史行；无代码改动）
```

- `BootHandoffV1`、wire 版本、分区布局、`PlatformDescSource` 的 `#[repr(C)]`、syscall 号/参数均未改。
- 启动次序：`arch_boot_finish` 的“先校验后 store 再建表”与旧 `arch_boot` 相同；v1 接收面 decode → validate → resume → store 也未变。
- 行为改动仅限进程内 Rust 类型/签名与测试固定装置（`loader.rs` 测试静态 `STORAGE` 改名 `LEAKED_BUFFERS`、测试 helper、`store_before_alloc_pin` pin 放宽到两个 store 入口），属 D-119 明确允许的范围。
- `os/servers/vm/src/vm_server.rs` 只改一行注释（`build_kernel_info` -> `boot handoff's KernelInfo`）。

判定：**PASS**。
证据强度：E1（git diff）+ E2（源码位置）。

## 6. 测试重跑与数字对账

独立重跑（E1）：

```
$ cd os && cargo test -p minix-boot --lib
test result: ok. 188 passed; 0 failed; 0 ignored

$ cargo test -p boot-shim --lib
test result: ok. 136 passed; 0 failed; 0 ignored

$ cargo test -p minix-kernel --features mock --lib
test result: ok. 866 passed; 0 failed; 8 ignored

$ cargo test -p minix-kernel --features mock        # 全部测试二进制
lib 866/0/8；integration 3/0/3、4/0/0、2/0/0、1/0/0；doctest 0/0/1；全部 ok，0 failed
```

补充（E1）：

```
$ cargo check -p minix-kernel        -> exit 0（87 warnings，无 error）
$ cargo build -p minix-kernel --target aarch64-unknown-none --no-default-features -> exit 0
$ cargo build -p minix-kernel --target riscv64gc-unknown-none-elf --no-default-features -> exit 0
```

与文档对账：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:2578` 与 `:2612` 声称 188/0、136/0、`minix-kernel --features mock --lib` 866/0/8，与实测完全一致。完成记录里的“lib 866/0/8；全部测试二进制通过”也与实测一致。

判定：**PASS**（对三条指定命令与文档数字）。
证据强度：E1（工具输出）。

注意：本项 PASS 不覆盖 QEMU 测试载体的编译；三个 `test-higher-half*` 载体的编译失败见“额外阻断发现 A”。

## 7. QEMU 门

四条日志都存在，均无 `panicked at`、无 `TEST_RESULT: FAIL`（E1）：

| 日志 | 文件大小/行数 | 关键 marker | 判定 |
|---|---|---|---|
| `tmp/review-loop/01-stage/evidence/design02-qemu-x86-uefi.log` | 109,089 B / 2,671 行 | `### TEST_RESULT: PASS test-shim-bootmarks-x86 ###`；脚本 `os/qemu-tests/test-shim-bootmarks-x86.sh:30-34` 要求 MARK1..MARK5，其中 MARK5=`entering scheduler`，否则不会打印 PASS | PASS |
| `.../design02-qemu-aarch64-uefi.log` | 152,417 B / 3,285 行 | `### TEST_RESULT: PASS test-shim-bootmarks-aarch64 ###`；串口读到 `boot-shim: memmap source: uefi-getmemorymap (dtb /memory cross-checked...)`、`entering kernel image v1` | PASS |
| `.../design02-qemu-riscv64-uboot-shim.log` | 660 B / 16 行 | `RESULT: PASS (D-73 U-Boot → boot-shim → kernel-uboot full chain: 6/6 markers, entering scheduler)` | PASS |
| `.../design02-qemu-riscv64-bootface.log` | 37,488 B / 927 行 | B1/B2/B3 全 PASS；`### TEST_RESULT: PASS test-kernel-image-riscv64-bootface ###` | PASS |

判定：**PASS**（按“日志缺失或含 FAIL 才判 FAIL”的口径）。
证据强度：E1（PASS 行）+ E3（对门覆盖范围的解释，见残余风险）。

限定：aarch64 门的判据只到发送侧分类点（脚本只要求 shim 行，日志在 `entering kernel image v1` 后因超时被 kill），不证明接收侧落 `.bss` 后走到 `kmain`；riscv64 bootface 门在模块装载失败处 halt（B2 的负例），不进入 `arch_boot`；x86 门的 MARK5 因脚本 `head -12` 没有出现在归档 stdout 里，只能靠脚本自己的 PASS 条件背书。详见残余风险 1–4。

## 8. 文档同步（§3.2 / §3.7 / §4.4）

命令与关键输出（E1/E2）：

```
$ sha256sum rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
996ff161d02ef867151e780adb7243e49e7dc249b5b161d6d0c5ed229b5f22ec

$ rg -n "build_kernel_info|report_static|BootStorageCell|\bSTORAGE\b" <doc>
（无输出）

$ rg -n "validate_impl_inner" rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
2368: ... 共享校验体 `validate_impl_inner` 还检查 ...

$ rg -n "validate_impl_inner" os
os/libs/minix-boot/src/kernel_info.rs:152:/// Validation strictness for [`KernelInfo::validate_impl_inner`].
```

发现：

1. **§4.4 仍出现旧符号**：`01-boot-shim-bootstrap.md:2368` 把共享校验体叫 `validate_impl_inner`；当前代码只有 `validate_view`（`os/libs/minix-boot/src/kernel_info.rs:290`），旧名只在上一条代码注释里残留。这是指定同步章节内的旧符号，按本项判据直接不合格。
2. **§4.4 的概括句与代码相反**：`:2284` 写“接收侧统一走 `store_boot_report`，先把切片深拷进内核 `.bss`”。实际只有两个直核腿经 `store_boot_report`（`os/kernel/src/lib.rs:230`）；三条 v1 腿的接收侧是 `arch_boot_resume_v1` 解码进 `.bss` 后由 `resume_additive` 调 `store_kernel_info`（`os/kernel/src/lib.rs:325`），`store_boot_report` 根本不在 v1 路径上。同一文档 `:2374`（§4.4.1）与 `:2397`（§4.5）写的是正确的分腿版本，`:2284` 与它们自相矛盾。`:2324` 的 `source_records` 行也把 v1 的落地写成 `store_boot_report`。
3. **代码侧旧签名/旧 API 注释未清**（Doc Code Sync 未完成）：
   - `os/boot-shim/src/lib.rs:7` 仍写 `arch_boot(&KernelInfo, root_page)`，实际是 `arch_boot(&BootReport<'_>, RamAuthority, PhysBytes)`。
   - `os/boot-shim/src/lib.rs:91` 与 `os/boot-shim/src/opensbi_helpers.rs:36` 仍说装载流水线 “build KernelInfo”；发送侧已不构造 `KernelInfo`。
   - `os/kernel/src/lib.rs:1553` 的 `store_kernel_info` 文档说 `arch_boot_finish` 是它的调用点之一（“a real copy”）；实际 `arch_boot_finish` 调 `store_boot_report`。
   - `os/libs/minix-boot/src/kernel_info.rs:152` 的 `validate_impl_inner` 链接名旧。
   - `os/kernel-image/Cargo.toml:52,61` 描述仍以 `build_kernel_info` 为取件车道的一部分。
   这些不是文风问题，是符号/签名与当前代码不一致。

§3.2 与 §3.7 的正文核对：`KernelInfo` 十字段、五腿发送视图、`BootReport::validate_requiring_sources(authority)`、`validate_view`（`:1844`）、逐腿 store/boot_validate 次序（`:1840`、`:1844`）均与代码一致；§3.7 本身没有发现旧符号。

判定：**FAIL**。
证据强度：E1（rg 输出）+ E2（行号与源码对照）。

## 9. 边界

命令与关键输出（E1）：

```
$ git show cdd0d1c77 --name-only --format="" | rg "^(minix3/|rewrite-notes/(02-stage-vm|03-stage-rs)/)"
（无输出，exit 1）

$ git diff cdd0d1c77^ cdd0d1c77 -- rewrite-notes/meta/coordination/review-eval/DECISIONS.md
（只新增 D-119 一行；无 defer-approved 新增）

$ git diff cdd0d1c77^ cdd0d1c77 | rg '^\+.*(defer|DEFER|TODO|FIXME|待办|未完成|欠账)'
（只命中 D-119 授权文本与计划文本里的“不新增 defer”，无新增延迟项）
```

无 `minix3/`、无 `02-stage-vm/`、无 `03-stage-rs/`；`os/servers/vm/src/vm_server.rs` 的单行注释改动、`loader.rs` 测试静态改名、`store_before_alloc_pin` 的 pin 更新都与旧符号删除直接相关，不算无关重构。

判定：**PASS**。
证据强度：E1（git 输出）。

## 10. 回退判断

`rg -n "KernelInfo<'" os --glob '*.rs'` 的命中全是 `KernelInfo<'static>` 的文档/注释表述，没有 `KernelInfo<'a>` 类型；`KernelInfo` 本身仍是无生命周期参数的 `'static` 切片结构（`os/libs/minix-boot/src/kernel_info.rs:21` 起）。发送侧统一为 `BootReport<'_>`，接收侧 `store_boot_report`/`store_kernel_info` 落 `.bss`；这是 (b)，不是 (a) 或混合。`v312/` 下没有 `DESIGN-02-ROLLBACK.md`，按授权不需要回退理由页。

判定：**PASS**。
证据强度：E2（源码位置）+ E1（文件列表）。

---

## 额外阻断发现 A：三个 QEMU 测试载体编译失败（本次批次引入）

独立构建（E1）：

```
$ cd os && cargo build -p test-higher-half --target x86_64-unknown-uefi --features fw-x86-uefi
error[E0061]: this function takes 3 arguments but 2 arguments were supplied
  --> qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs:75:5
   |     minix_kernel::arch_boot(&kernel_info, root_page);
   |     expected `&BootReport<'_>`, found `&KernelInfo`
EXIT=101

$ cargo build -p test-higher-half-aarch64 --target aarch64-unknown-uefi --features fw-aarch64-uefi
error[E0061] ... test-higher-half-aarch64/src/main.rs:65:5
EXIT=101

$ cargo build -p test-higher-half-riscv64 --target riscv64gc-unknown-none-elf --features fw-riscv64-none
error[E0061] ... test-higher-half-riscv64/src/main.rs:146:5
EXIT=101
```

这是本批次引入的回归，不是历史债：`git show cdd0d1c77^:os/kernel/src/lib.rs` 的三个 `arch_boot` 臂当时都是 `pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> !`，三个载体的两参数调用在父提交上自洽。

影响面（E2）：

- `os/qemu-tests/run_all.sh:96`（x86_64 列表）、`:102`（aarch64 列表）、`:108`（riscv64 列表）都构建这三个包。
- `.github/workflows/qemu-tests.yml:61` 跑 `run_all.sh`；`:126` 还显式构建 `test-higher-half-aarch64`。
- 因此 qemu-tests 的 `qemu` 与 `cross-arch-kernel-build` 作业会在 build 段变红；这不属于“宿主三条命令全绿”能覆盖的范围。

修复在 D-119 明确允许的“测试固定装置”范围内：把三个载体手里的 `KernelInfo` 改造成 `BootReport`（或测试内 helper 后调 `arch_boot(&report, RamAuthority::..., root_page)`）。本报告只指出，不修改源码。

## 额外发现 B：文档/注释同步债（非功能性）

- 计划 `v312/DESIGN-02-MANUSCRIPT.md:142` 的验收 `rg -n 'build_kernel_info' os/` 0 命中不成立：`os/kernel-image/Cargo.toml:52,61` 有两处注释。提交给 D-119 的命令限定 `--glob '*.rs'` 才为 0。
- `os/kernel/src/lib.rs:1550-1564` 的 `store_kernel_info` 文档块是旧文本 + 新文本拼接，既重复（“The slot is written exactly once per leg” 段）又保留已不存在的调用点描述。
- `os/libs/minix-boot/src/kernel_info.rs:152` 的 `KernelInfo::validate_impl_inner` 是已改名符号。

这些不影响运行时行为，但与“文档 §3.2/§3.7/§4.4 与代码同步”的验收和 Doc Code Sync 要求不一致。

---

## 残余风险

1. **QEMU 日志与 commit 没有硬绑定**：四个日志 mtime 为 04:47–04:49，提交时间为 04:55:45，日志内没有 commit hash 或源码 hash。日志里的编译告警行（如 `kernel_info.rs:8` 的 unused import、`main.rs:368` 的 `return halt()`）与当前源码一致，可作弱证据，但不能证明日志对应的树与 `cdd0d1c77` 完全相同。未重跑 QEMU（时间与固件条件未确认）。
2. **新的直核接收路径没有 QEMU 运行证据**：四个日志中，x86 UEFI 与 riscv64 U-Boot shim 是 v1 路径（`store_kernel_info`）；aarch64 UEFI 日志止于 `entering kernel image v1`；riscv64 bootface 日志在模块装载失败处 halt，未进入 `arch_boot`；aarch64 直核不在四条日志内。也就是说 `store_boot_report`/`arch_boot_finish` 的“借用视图 → `.bss` 深拷 → 唯一 `KernelInfo`”只有宿主测试与静态走查，没有一条启动门真正跑到。
3. **栈上 `BootStorage` 的余量未在启动路径上验证**：`run_self_boot` 从 `.data` 的 `BootStorageCell` 改为 `let mut storage = BootStorage::new()`（`bootface_common.rs:354`），编译期只断言 `size_of::<BootStorage>() <= 16 * 1024`（`:18`），启动栈 64 KiB。没有 QEMU 运行进入 `arch_boot`，所以这条路径的栈余量未被实测。
4. **aarch64 UEFI 门只证明发送侧**：`test-shim-bootmarks-aarch64.sh` 的 MARK1..MARK4 只要求 shim 行；日志在 `entering kernel image v1` 后超时。这与文档 §5.1 的 I3 行自述一致，但不能当作接收侧落地证据。
5. **x86 门的 MARK5 不在归档里**：`test-shim-bootmarks-x86.sh` 要求 `entering scheduler` 才打印 PASS，但归档 stdout 只 `head -12` 了串口匹配行，`entering scheduler` 未出现。PASS 依赖脚本条件，不是可直接复查的日志行。
6. **`validate_view` 的等价性只做了人工 diff**：本次逐行对照了旧 `validate_impl_inner` 与新 `validate_view`，断言文本、顺序、条件一致；没有形式化等价证明，也没有为“旧路径断言集合 = 新路径断言集合”建立自动对拍测试。
7. **`KernelInfoView` 是 9 个访问器对 10 个字段**：`authority` 走 out-of-band 参数。若评审把“一一对应”按字面理解，这是一处设计口径差异；按代码注释与 D-119 的“接收侧唯一 `KernelInfo<'static>`”读，它是刻意为之。
8. **未跑 `cargo fmt --check` / `cargo clippy`**：任务给的可执行命令清单不含这两条；计划 §5 提到过它们，本次未复核。`cargo check` 只有 warning，无 error。

## 与完成记录的差异

独立核对完成之后才读 `v312/r1-post-design02/MANUSCRIPT-DESIGN-02.md`（89 行）用于对照：

1. 记录 §4“文档同步”称 §3.2/§3.7/§4.4 已与代码同步；实际 §4.4 还有 `validate_impl_inner` 旧符号（`:2368`）和“接收侧统一走 `store_boot_report`”的相反概括（`:2284`），代码与 Cargo 注释也还留有旧 API 名。记录高估了同步完成度。
2. 记录“宿主测试”与“交叉构建”只覆盖三条宿主命令、boot-shim/kernel-image 的生产构建，没有提三个 QEMU 测试载体的编译；实际 `test-higher-half*` 全部 E0061 失败，qemu-tests 矩阵 build 段会红。这是记录未披露的阻断项。
3. 记录“QEMU 启动链门”的四条 PASS 与日志一致，但记录的措辞没有带上门覆盖范围的限定（aarch64 只到发送侧、bootface 只到错误路径、x86 的 MARK5 只在脚本条件里）；这些限定在文档 §5.1 有，但完成记录里读不到。
4. 记录的数字对账（188/0、136/0、866/0/8、doc sha `996ff161…`、2,946 行）与本次实测完全一致；commit `cdd0d1c77` 与 HEAD 之后只有 review 元数据差异，也一致。
5. 记录验收清单最后一项“独立 verifier 复核”仍是未勾选状态；本报告就是该复核，结论为 FAIL，不通过。
