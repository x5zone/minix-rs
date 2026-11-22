# y02 R2 代码独立复验（D01 R2）

VERDICT: PASS

- 时间：2026-11-23 06:50 CST
- 复验对象：D01 R2 代码批次（DESIGN-04、DESIGN-05）
- commit：`11a33354c74fb1834e6893a6e4bf4627b9ca8b70`（`fix(01-01): R2 doc/code batch, design decisions open`）
- 工作区状态：`git status --porcelain -- os/` 为空，源码与 HEAD 一致。复验开始时 R2 改动尚未提交（`git diff HEAD` 显示 5 个 `os/` 文件 + 文档/元数据）；06:43 另一个进程把它们提交为上述 commit。提交内容与开始时捕获的 hunk 完全一致（`git diff 5b8325437 11a33354c --stat -- os/` → 5 files, 8 insertions, 17 deletions），测试读数随后在 HEAD 上全部重跑。本报告是本 verifier 唯一的仓库写入。
- 证据日志：本次会话的完整输出在 `/tmp/y02-r2/*.log`（会话外不持久；下面的关键读数逐条抄在正文）。

## 逐项

### 1. 范围：只动 R2 声明的代码面 — PASS（L1）

```
$ git diff 5b8325437 11a33354c --stat -- os/
 os/boot-shim/src/uboot_main.rs             | 6 +++---
 os/kernel/src/boot_alloc.rs                | 5 -----
 os/libs/minix-boot/src/boot_storage.rs     | 5 -----
 os/libs/minix-boot/src/handoff_v1_write.rs | 5 ++---
 os/libs/minix-boot/src/kernel_info.rs      | 4 +++-
 5 files changed, 8 insertions(+), 17 deletions(-)

$ git diff 5b8325437 11a33354c --stat -- minix3 rewrite-notes/02-stage-vm rewrite-notes/03-stage-rs
（无输出，exit 0）

$ git status --porcelain -- minix3 rewrite-notes/02-stage-vm rewrite-notes/03-stage-rs
（无输出）
```

改动面只有 5 个 `os/` 源文件，加上本篇文档与 R2 元数据（`git show --name-only 11a33354c` 的非 `os/` 部分全是 `rewrite-notes/`）。`minix3/` 与 02/03 stage 零改动。PASS。

### 2. `source_audit` 退出生产 API — PASS（L1）

- 定义处（`os/libs/minix-boot/src/kernel_info.rs:556-561`）：方法上方是 3 行文档 + `#[cfg(test)]`，`pub fn source_audit(&self) -> Option<SourceAuditError>` 紧随其后；方法体逐字未动（该文件 diff 为 +4/−1：3 行文档、1 行属性、1 行原两行文档之一改写）。
- 调用者：

```
$ rg -n 'source_audit' os/ --glob '*.rs'
os/libs/minix-boot/src/memmap_source.rs:2497/2533/2563/2578/2606   （5 个测试名，与被删方法无关）
os/libs/minix-boot/src/kernel_info.rs:561                            （cfg(test) 定义）
os/libs/minix-boot/src/kernel_info.rs:776                            （同文件单测，唯一方法调用）

$ rg -n '\.source_audit\(' os/ --glob '*.rs'
os/libs/minix-boot/src/kernel_info.rs:776:        assert!(info.source_audit().is_none());
```

- 生产审计路径未变：`validate` / `validate_requiring_sources` → `validate_view`，strict 分支仍调用 `audit_source_coverage_for` 与 `audit_source_coverage`（`kernel_info.rs:507,519`）；被 cfg 掉的方法只是这些结果的 `Option` 诊断包装。
- 口径说明：台账 DESIGN-04 的复验命令 `rg -n 'pub fn source_audit|impl Default for BootStorage|impl Default for BootAlloc' os（期望 0）` 实测 1 命中（`kernel_info.rs:561`）。命中是 `#[cfg(test)]` 下的定义行本身；处置写的是「改为 `#[cfg(test)]`」，与「期望 0」自相矛盾。按缺陷意图（生产 API 不再含该方法）判 PASS；台账命令需改（见 N3）。

### 3. 两个 `Default` impl 删除 — PASS（L1）

```
$ rg -n 'impl Default for (BootStorage|BootAlloc)' os/
（无输出，exit 1）

$ rg -n 'BootStorage::default|BootAlloc::default' os/
（无输出，exit 1）
```

- 全部构造点走 `new()`：`uboot_main.rs:475`、`uefi_helpers.rs:1086`、`bootface_common.rs:354`、`boot_alloc.rs:161`（`static BOOT_ALLOC: BootAlloc = BootAlloc::new()`）以及 `boot_storage.rs` 内 6 处测试；`BootStorage::new`/`BootAlloc::new` 都是 `pub const fn`，未随 impl 删除。
- 没有任何宿主结构体靠 `Default` 间接依赖它们：`rg -n 'BootStorage' os/ --glob '*.rs' | grep -iE 'default|derive'` 零命中；`cargo check`/`cargo test` 全绿（第 5 项）证明无隐式泛型绑定被打破。
- 公开面收窄（外部 crate 若用过 `BootStorage::default()` 会断）是 DESIGN-04 的意图；仓内零消费者，编译测试已覆盖。

### 4. startup 注释残留与 `(d73)` — PASS（附口径说明）

- 三处原始误导陈述清零：

```
$ rg -n 'named by a startup|memmap/reserved/sources/param/startup|startup-object partition is deliberately' \
    os/libs/minix-boot/src/handoff_v1_write.rs os/boot-shim/src/uboot_main.rs
（无输出，exit 1）
```

- 两文件现存 `startup` 命中恰好 2 行，均为否定式、不把 startup 分区/记录写成现行结构：
  - `os/boot-shim/src/uboot_main.rs:517`（既有）`// No startup-object partition is written: ...`
  - `os/libs/minix-boot/src/handoff_v1_write.rs:391`（本轮改写）`// v10 has no startup-object partition; ...`
- 口径说明：台账 DESIGN-05 的复验命令 `rg -n 'startup-object|named by a startup' ...（期望 0）` 实测 2 命中，全部是上述否定句。正则过宽，不是残留；台账命令需改（见 N3）。
- `(d73)` 串保留有真实依赖：`uboot_main.rs:443`、`:477` 两串，`os/qemu-tests/test-riscv64-uboot-shim.sh:206,208` 用 `grep -qa` 精确匹配它们；删除会让 riscv64 U-Boot 真机门红。台账把它记 FP 有证据。
- 新注释的语义核对：`reset_map_partitions` 的唯一调用点是 `os/boot-shim/src/ebs_pair.rs:172` 的配对重试环；该环重跑时只重写两表与 sources（`classify_into_handoff` + `push_dtb_sources`），不重写 param/modules/envelope，所以「the cleared ranges cover every writable partition a retry can rewrite」成立。

### 5. 测试与检查读数 — PASS（L1）

全部按 AGENTS.md 权威配方在 docker `minix-ci:1.94`、`-m 2g`、`-j 1`、挂载仓库根且工作目录 `os/` 下重跑（`os/.cargo/config.toml` 的 `RUST_TEST_THREADS=1`、`RUST_MIN_STACK=8MiB` 生效）：

| 命令 | 读数 |
|---|---|
| `cargo test -p minix-boot --lib` | `test result: ok. 188 passed; 0 failed; 0 ignored` |
| `cargo test -p boot-shim --lib` | `test result: ok. 136 passed; 0 failed; 0 ignored` |
| `cargo test -p minix-kernel --features mock --lib` | `test result: ok. 866 passed; 0 failed; 8 ignored` |
| `cargo check -p minix-kernel` | `Finished dev profile`，0 error，`minix-kernel (lib) generated 87 warnings` |

补充说明：

- 裸形态 `cargo check -p minix-kernel` 能过：当前 `form_contract` 是 feature→form 的类型级绑定（`os/kernel/src/lib.rs:111-120`），不是 AGENTS 旧描述里的 compile_error 绊线；host 默认形态仍由 SD-30 的其他约束管理，本项不受影响。
- `--lib` 不含 `boot-shim` 的 bin（`uboot_main.rs`）与 `kernel-image`；本轮对 `uboot_main.rs` 的改动只动注释，不进入代码生成（见残余风险）。

### 6. 行为等价 — PASS（L1/L2）

- `source_audit`：方法体逐字未改，只加 `#[cfg(test)]` 与文档；生产审计仍由 `validate_view` 承担，运行时路径零变化。唯一测试调用保留并通过。
- `Default` 删除：仓内零调用者、零泛型约束，编译与 188/136/866 三组测试证明无破。仓外消费者不在本项目契约内。
- 注释三处：`uboot_main.rs` 两处（prepare_boot 语义表清单、arena 归属）与 `handoff_v1_write.rs` 一处（reset 清除范围）均不参与代码生成；改写后的事实陈述已在第 4 项逐条核对，未发现新的假陈述。
- `(d73)` 串与 QEMU 脚本的匹配关系未变。

## 新发现

N1（P3，代码/CI 棘轮）：删 `Default` 换来 2 条新的 clippy 警告 `new_without_default`。

- 位置：`os/libs/minix-boot/src/boot_storage.rs:111`（`BootStorage::new`）、`os/kernel/src/boot_alloc.rs:43`（`BootAlloc::new`）。
- 证据与读数：

```
$ cargo clippy -p minix-boot --lib
warning: you should consider adding a `Default` implementation for `BootStorage`
  --> libs/minix-boot/src/boot_storage.rs:111:5
（minix-boot lib 共 9 warnings）

$ cargo clippy -p minix-kernel --lib
warning: you should consider adding a `Default` implementation for `BootAlloc`
  --> kernel/src/boot_alloc.rs:43:5
（minix-kernel lib 共 131 warnings）

$ cargo clippy --workspace   # count_warnings 口径
current=366  baseline=341
```

- 归因：本轮唯一的代码增删就是两处 `impl Default` 删除，两条新警告正落在两个被删 impl 的 `new()` 上；其余 diff 是注释、`#[cfg(test)]` 与文档，不能产生 `new_without_default`。按构造推算父提交为 364，本轮的边际贡献是 +2。
- 影响：PD-31 的「新增必须为零」纪律下这 2 条属于新增；工作区绝对读数本就超基线 23 条（不是本轮造成，但本轮使其 +2）。若 `clippy-ratchet` 作为本轮门，这 2 条需要先处理。
- 最小修复（三选一）：给两个 `new` 各加一行带理由的 `#[allow(clippy::new_without_default)]`（CP-2 只禁 `allow(dead_code)`/`allow(unused*)`，不禁 clippy allow）；或恢复 `Default` impl（与 DESIGN-04 冲突）；收窄 `new` 公开面不可行（调用点众多）。

N2（P3，警告卫生）：`SourceAuditError` 在生产 cfg 下变成未用导入。

- 位置：`os/libs/minix-boot/src/kernel_info.rs:8` 的 `use crate::memmap_source::{InputObjectId, RamAuthority, SourceAuditError, SourceRecord};`。
- 证据：`cargo test -p boot-shim --lib` 构建依赖时输出 `warning: unused imports: InputObjectId and SourceAuditError`（`kernel_info.rs:8:28`）；`InputObjectId` 的未用是存量（只在 `mod tests` 里以全路径使用），本轮新增的是 `SourceAuditError` 这一项。
- 影响：警告条数不变（rustc 对同一条 use 合并为 1 条），但下游每个非 test 构建都会多看到一个未用项。
- 最小修复：把 `SourceAuditError` 移进 `#[cfg(test)]` 导入，或在 `source_audit` 签名里用 `crate::memmap_source::SourceAuditError` 全路径（该方法已是 test-only）。

N3（P3，台账复验口径）：DESIGN-04 与 DESIGN-05 的复验命令与所采纳的修复自相矛盾，按字面执行都返回非零。

- DESIGN-04 现命令：`rg -n 'pub fn source_audit|impl Default for BootStorage|impl Default for BootAlloc' os（期望 0）` → 实测 1 命中（cfg(test) 定义行）。
- DESIGN-05 现命令：`rg -n 'startup-object|named by a startup' os/libs/minix-boot/src/handoff_v1_write.rs os/boot-shim/src/uboot_main.rs（期望 0）` → 实测 2 命中（两条否定句）。
- 建议改为可判定的口径：
  - DESIGN-04：`rg -n 'impl Default for (BootStorage|BootAlloc)' os` → 0；且 `sed -n '559,561p' os/libs/minix-boot/src/kernel_info.rs` 显示 `#[cfg(test)]` 紧邻 `pub fn source_audit`。
  - DESIGN-05：上述三条原始短语（`named by a startup`、`memmap/reserved/sources/param/startup`、`startup-object partition is deliberately`）→ 0；两文件的 `startup` 命中只允许否定式。
- 影响：不改代码，只改台账「复验」列；否则按台账字面复验会把已达成意图的两项误判为未完成。

N4（P3，格式残迹）：两处 impl 删除点各留下一个连续空行。

- 位置：`os/libs/minix-boot/src/boot_storage.rs:279-280`、`os/kernel/src/boot_alloc.rs:157-158`（`}` 后两个空行）。
- 证据：`rustfmt --check` 在 277 与 155 行报的正是这两个多余空行。
- 影响：仓库没有 `cargo fmt` CI 门，且工作树本就不是 rustfmt-clean（同一命令在改前代码报大量存量差异），因此不阻断；最小修复是各删一个空行。

N5（观察，非缺陷）：`(d73)` 仍是生产串口输出里的过程编号，属于 CP-3 的字面例外。保留理由有硬证据（`test-riscv64-uboot-shim.sh:206,208`），台账按 FP 记录。若将来出版门把字符串也纳入 CP-3，这条会成为显式例外，需在裁决里保留。

## 残余风险

- Clippy 棘轮在工作区当前 commit 上是红的（366 vs 341）；其中 23 条与本轮无关，本轮边际 +2（N1）。若本轮把「新增为零」作为硬门，N1 必须先修。
- `boot-shim` 的 bin 目标（`uboot_main.rs`）与 `kernel-image` 未重编：CI 容器只装了 host target；`uboot_main.rs` 的改动是纯注释，不可能改变代码生成，`kernel-image` 使用的 `BootStorage::new` 未动。三架构真机门与本轮行为无关，未跑。
- 核验期间仓库被并发提交（11a33354c）。本报告以提交后的 HEAD 为准，四项读数全部在提交后重跑；若后续有人再改这 5 个文件，应以本报告的 commit 头为界重验。
- 构建命中增量缓存，不是 clean rebuild；docker 配方与工具链版本与 CI 一致（rustc 1.94.1 / clippy 0.1.94）。
- `rustfmt --check` 的其余差异是存量，不计入本轮；N4 只列本轮新引入的两个空行。
- 未对照 `minix3/` C 源码复核行为基线：本轮没有语义改动，C 真值对账归事实面板。
- QEMU 未跑：三处注释与两处 API 面清理不改变运行时行为；`(d73)` 串未动，真机门依赖不变。
