# y03 独立设计裁决复核：D-120 / D-121

- 角色：fresh context 独立 verifier；只读仓库与证据，唯一写入是本报告。
- 基线：复核期间 HEAD 由 `4f563b2ce` 前进到 `7d6835aa5`（D-120/D-121 代码提交），再前进到 `5ce3f9144`（文档 §4.3 节选标注与 `proc.rs` 注释）；最终工作区另含已暂存的文档读数修订。最终代码状态 `5ce3f9144`，文档 sha `d035c692c7435dddcd57b04952d717c800a103030830add860646b9cf1f8b41b`（`wc -l` = 2953 行；sha 与调度方给出的 `d035c692…` 一致）。
- 代码范围：`7d6835aa5` 之后到最终状态，D-120/D-121 相关代码仅 `os/kernel/src/proc.rs` 的两行注释变化；行为代码未变。
- 结论：**VERDICT: PASS**

---

## D-120 必查

### 1. `minix-boot` 唯一权威表与两个 const 派生视图

命令与输出：

```text
$ rg -n 'BOOT_MODULE_IDENTITIES' os/libs/minix-boot/src/kernel_info.rs
685:pub const BOOT_MODULE_IDENTITIES: [BootModuleIdentity; NR_BOOT_MODULES] = [
686:    BootModuleIdentity { name: "ds", proc_nr: ProcNr(6) },
...
697:    BootModuleIdentity { name: "init", proc_nr: ProcNr(11) },
700:/// Derived name view of [`BOOT_MODULE_IDENTITIES`]. ...
719:pub const BOOT_MODULE_PROC_NRS: [ProcNr; NR_BOOT_MODULES] = [
```

- 表项为 12 项，每项 `name` + `ProcNr`；数组长度由 `NR_BOOT_MODULES` 钉住（`kernel_info.rs:667`，值 12）。
- `MODULE_NAMES`（`:702`）与 `BOOT_MODULE_PROC_NRS`（`:719`）逐项从 `BOOT_MODULE_IDENTITIES[i]` 取值，是派生视图，不是独立字面表。
- 派生视图采用显式逐项索引数组，而不是 `const` 映射函数；这正是 D-120 允许的回退形式（“若派生视图 const 求值不可用，回退为显式派生数组”），不改变“唯一权威表”的性质。

依赖边证据：

```text
$ grep -n "minix-types" os/libs/minix-boot/Cargo.toml
9:minix-types = { workspace = true }

$ git diff --stat 4f563b2ce..HEAD -- '*Cargo.toml'
（无输出）
```

`minix-boot` 在本轮之前已依赖 `minix-types`，本轮没有新增依赖边。判定：PASS。

### 2. kernel `proc.rs` 无第二份 proc-nr 字面表

```text
$ rg -n 'pub const BOOT_MODULE_PROC_NRS' os/kernel/src/proc.rs
（0 matches）

$ rg -n 'pub use minix_boot::BOOT_MODULE_PROC_NRS' os/kernel/src/proc.rs
105:pub use minix_boot::BOOT_MODULE_PROC_NRS;
```

消费点仍在原路径解析并通过编译：

```text
$ rg -n 'BOOT_MODULE_PROC_NRS' os/kernel/src | head
proc.rs:105:pub use minix_boot::BOOT_MODULE_PROC_NRS;
proc.rs:1976: BOOT_MODULE_PROC_NRS[i],
vm_handoff.rs:44:use crate::proc::{BOOT_MODULE_PROC_NRS, KERNEL_TASKS};
vm_handoff.rs:633: proc_nr: BOOT_MODULE_PROC_NRS[i].0,
misc.rs:33: BOOT_MODULE_PROC_NRS, ...
lib.rs:2797: BOOT_MODULE_PROC_NRS, ...
```

`cargo test -p minix-kernel --features mock --lib` 866 通过 / 0 失败 / 8 忽略（命令与输出见“通用门”）。判定：PASS。

### 3. `uboot_main.rs` 第三份名字抄本已删

```text
$ rg -n 'uboot_layout::MODULE_NAMES' os
（0 matches）

$ rg -n 'const MODULE_NAMES|minix_boot::MODULE_NAMES' os/boot-shim/src/uboot_main.rs
269:const MODULE_NAMES: [&str; minix_boot::NR_BOOT_MODULES] = minix_boot::MODULE_NAMES;
```

删除的 `uboot_layout::MODULE_NAMES` 字面数组（原 `uboot_main.rs:221-225`）已不存在；保留的 `MODULE_NAMES` 是类型化别名，值直接来自 `minix_boot`。`boot-shim/src/loader.rs:71` 原本就是 `pub use minix_boot::MODULE_NAMES;`，因此安装循环与 xtask 读的是同一份视图。`cargo test -p boot-shim --lib` 136 通过 / 0 失败。判定：PASS。

### 4. `xtask/src/manifest.rs` 顺序锁改为 typed 读取

```text
$ rg -n 'minix_boot::MODULE_NAMES|include_str|loader_rs' os/xtask/src/manifest.rs
132:        let names: Vec<&str> = minix_boot::MODULE_NAMES.to_vec();

$ cargo test -p xtask manifest::tests
test manifest::tests::boot_module_count_matches_single_source_constant ... ok
test manifest::tests::boot_module_order_matches_boot_shim_module_names ... ok
test manifest::tests::every_manifest_entry_points_at_an_existing_package_dir ... ok
test manifest::tests::module_names_are_unique ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 13 filtered out
```

测试不再 `include_str!` 抽取源码文本，也不解析 `= &[`。判定：PASS。

### 5. 精确口径：独立抄本 2→1，不宣称常量个数字面 -1

- `DECISIONS.md:195`（D-120）明确写：“**精确口径**：独立抄本数 2→1，漂移面关掉；源码常量个数不是字面 -1（1 张权威表 + 2 个派生常量）”。
- 缺陷台账 DESIGN-01 行只写“`BOOT_MODULE_IDENTITIES` 为唯一权威表；`MODULE_NAMES`/`BOOT_MODULE_PROC_NRS` 为 const 派生视图；`uboot_main` 第三份抄本删除”，没有“常量 -1”表述。
- 全轮 diff 中检索“字面 -1 / 常量 -1 / 常量个数”，只命中 D-120 自己那句“不是字面 -1”的限定，无相反宣称。

事实核对：改动前有 2 张按位置相关的独立字面表（`minix-boot::MODULE_NAMES` 与 `kernel::proc::BOOT_MODULE_PROC_NRS`），另加 `uboot_main` 的第三份名字抄本；改动后 1 张权威表 + 2 个派生视图，独立抄本数为 1。口径准确。判定：PASS。

---

## D-121 必查

### 1. `kmain` 调 strict 门，且保留 postconditions

```text
$ rg -n 'kernel_info.validate_requiring_sources\(\)|kernel_info.validate\(\)|kernel_info.check_boot_postconditions\(\)' os/kernel/src/lib.rs
1867:    kernel_info.validate_requiring_sources();
1869:    kernel_info.check_boot_postconditions();
```

`sed -n 1858,1872p os/kernel/src/lib.rs` 显示该调用点位于 `kmain` 的 Phase A 入口（`boot_stage!("kernel: kmain Phase A enter\n")` 之后），`kernel_info.validate()` 在 `kmain` 路径上已无调用。`validate_requiring_sources` 的实现（`kernel_info.rs:278`）以 `Some(self.authority)` 与 `ValidationLevel::Strict` 调用共享校验体，检查名字位置、records 非空、权威记录存在、权威覆盖与内核跨度覆盖（`kernel_info.rs:485-521`）。判定：PASS。

### 2. 宿主 DTB fixture 覆盖 `fw_end=0` 与 `0x8020_0000`

测试位于 `os/libs/minix-boot/src/memmap_source.rs:3434-3457`：

- `for fw_end in [0u64, 0x8020_0000]`；
- 断言 `!owned.sources().is_empty()`；
- 断言 `audit_source_coverage(owned.free(), owned.reserved(), owned.sources()) == Ok(())`。

实跑：

```text
$ cargo test -p minix-boot --lib memmap_source::tests::direct_core_dtb_fixture_projects_nonempty_sources_for_both_fw_end_forms -- --exact
test memmap_source::tests::direct_core_dtb_fixture_projects_nonempty_sources_for_both_fw_end_forms ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 188 filtered out
```

两条直核腿的实际参数与 fixture 对应：aarch64 `bootface_a64.rs:148` 传 `fw_end: 0`；riscv64 `bootface.rs:143` 传 `fw_end: KERN_PHYS_BASE`（`0x8020_0000`）。两个 bootface 在调用 `arch_boot` 前都走 `bootface_common.rs:552` 的 `report.validate_requiring_sources(RamAuthority::DTB)`。判定：PASS。

### 3. 生产腿不因 strict 失败

x86-64 UEFI shim 门实跑：

```text
$ cd os && ./qemu-tests/test-shim-bootmarks-x86.sh
### TEST_RESULT: PASS test-shim-bootmarks-x86 ###
（退出码 0；脚本 PASS 条件包含 MARK1..MARK5，MARK5 = "entering scheduler"）
```

串口可见 `### minix-rs kernel-image: resuming high-half boot (handoff v1 arena, x86_64)`，即接收侧已过 `kmain` 之前的入口；脚本判定 PASS 时五个路标均已匹配。

riscv64 U-Boot shim 全链门实跑：

```text
$ cd os && ./qemu-tests/test-riscv64-uboot-shim.sh
RESULT: PASS (D-73 U-Boot → boot-shim → kernel-uboot full chain: 6/6 markers, entering scheduler)
（退出码 0）
```

aarch64 直核：本轮时间盒内未重跑该腿。静态核对显示该腿使用 `fw_end=0` 的 DTB 收集器，fixture 已证成同一形态的 `sources()` 非空与覆盖审计通过；文档 §5.1/§5.4 已登记 aarch64 直核只到 SMP 交付，其后的用户链 Data Abort 是已登记的 A2 遗留，不是 `kmain` 门失败。按调度方口径记为“`kmain` 门前已过、A2 非本批”，不据此判 FAIL；该腿未重跑列入“未覆盖”。判定：x86/riscv64 实跑 PASS，aarch64 按静态与已登记 A2 记部分覆盖。

### 4. 文档同步

- §3.7（`:1849`）：门链写“直核面 ... → `kmain` 的 strict `validate` → `check_boot_postconditions` 复核；v1 接收面 ... → `kmain` 的 strict `validate` → `check_boot_postconditions` 复核”，与新代码一致。
- §4.4（`:2289` 起）：字段表 `authority` 行写两个直核面在 `arch_boot` 调用点传 `RamAuthority::DTB`，v1 接收侧由 wire 解码写入，与 `land_and_store`（`kernel_info.authority`）一致。
- §4.4.1（`:2395`）：门表行名为 `KernelInfo::validate_requiring_sources`（`kmain` 入口），并写“按存储后的 `KernelInfo.authority` 复核记录集与内核跨度覆盖”，与实现一致；同表“共享校验体 ... `kmain` 入口再跑一次”也一致。
- §4.3（`:2171-2183`）：代码块标注为“结构节选”，列出 `BOOT_MODULE_IDENTITIES` 为唯一权威表、`MODULE_NAMES`/`BOOT_MODULE_PROC_NRS` 为派生视图、boot-shim 只重导出；正文写明“每个名字绑哪个进程号由 `BOOT_MODULE_IDENTITIES` 表项的 `proc_nr` 字段给出”，与代码一致。
- §3.2.2.3 本身只讲控制台与 `reserved_regions`/`kern_stack_top`/`platform_sources`/`param_buf` 四个字段，不承载 `kmain` 门或模块表描述；相邻的 §3.2.1（`:1186`、`:1190`）正确区分 strict `validate_requiring_sources` 与非 strict `validate`，并写明 `KernelInfo::validate_requiring_sources()` 读结构体字段。

判定：PASS；另见“新发现”中的 P3 命名残留。

---

## 通用门

| 检查 | 命令 | 结果 |
|---|---|---|
| minix-boot 宿主测 | `cargo test -p minix-boot --lib` | 189 passed / 0 failed |
| boot-shim 宿主测 | `cargo test -p boot-shim --lib` | 136 passed / 0 failed |
| kernel mock 宿主测 | `cargo test -p minix-kernel --features mock --lib` | 866 passed / 0 failed / 8 ignored |
| xtask manifest | `cargo test -p xtask manifest::tests` | 4 passed / 0 failed |
| kernel 编译 | `cargo check -p minix-kernel` | 退出码 0；`minix-kernel (lib) generated 87 warnings` |
| clippy | `cargo clippy -p minix-boot -p minix-kernel` | 退出码 0；`minix-boot (lib) generated 8 warnings`、`minix-kernel (lib) generated 130 warnings` |
| 基线告警对比 | 父提交 `7d6835aa5^` 的 `git archive` 副本 + 独立 `CARGO_TARGET_DIR` | 基线同为 minix-boot 8 / minix-kernel 130（check 亦同为 87）；无新增 warning |
| 仓库范围 | `git diff --name-only 4f563b2ce..HEAD` | 仅 `os/` 六个 D-120/D-121 文件与 `rewrite-notes/` 台账/文档；`minix3/`、02/03 stage 均无改动 |
| 新 defer | `git diff 4f563b2ce..HEAD -- rewrite-notes/meta \| rg -i defer` | 仅命中既有 D-118/D-119 文本（diff 上下文），无新增 defer 行 |

告警明细：minix-boot 的两条 warning 是 `kernel_info.rs:8` 的 `unused import: InputObjectId` 与 `memmap_source.rs:39` 的 `unused import: AtomicBool`；两行在父提交中已存在且未被本轮改动，父提交副本的 clippy 基线读数相同，故不构成新增 warning。

## 新发现

- **P3（文档命名残留，非阻断）**：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:2162`（§4.3）在防线 D 的描述中仍写“`KernelInfo::validate` 的两段断言”。两段重叠断言实际在共享校验体 `validate_view` 中，`kmain` 现在经 `validate_requiring_sources` 执行它；本句不直接描述 `kmain` 门，但按新代码的命名宜改为 `KernelInfo::validate_requiring_sources`（或“共享校验体 `validate_view`”）。同一命名残留还出现在 `:2529`、`:2944` 的 `overlaps` 对照行；`:2825` 的“`KernelInfo::validate` 的 strict 门”则把非 strict 方法名与 strict 门混写，建议一并改为 strict 入口名。以上均不影响 D-120 模块表描述与 §3.7/§4.4/§4.4.1 的 `kmain` 门描述。

## 残余风险

- `BOOT_MODULE_IDENTITIES`、`MODULE_NAMES`、`BOOT_MODULE_PROC_NRS` 是三个 `const` 数组；新增第 13 项需要同时改权威表与两个显式索引视图。这正是 D-120 选择“派生视图”口径时接受的代价：值只有一处权威，但数组形状的维护点仍是三处。若改成 `const fn` 映射可收为两处，不在本轮授权内。
- `xtask` 顺序锁现在直接比较 `minix_boot::MODULE_NAMES` 与装机清单。当前 `loader.rs` 重导出同一常量，等价成立；若未来有人在 loader 里重新引入本地顺序表，xtask 锁本身不会再从 loader 源码发现漂移，只能依赖内核 `kmain` 的 strict 按位置核名在运行期拦截。这不是本轮引入的缺陷，但锁的覆盖面比旧文本抽取版窄了一层。
- aarch64 直核腿未在本轮重跑；其 `fw_end=0` 的 DTB 收集器形态由宿主 fixture 覆盖，整腿是否仍停在已登记 A2 用户链只依据文档登记与静态路径，没有本轮新读数。

## 未覆盖

- aarch64 直核完整启动门（`test-smp-aps-aarch64.sh` 或等价）本轮未重跑；仅做静态核对与 `fw_end=0` fixture。
- aarch64 UEFI 全链门、riscv64/aarch64 直核 `test-kernel-image-*` 门本轮未跑。
- 物理机证据（QEMU/OVMF 之外的固件行为）不在本轮范围。
- 未做 `minix3/` C 源逐行复核；权威表数值只按既有 C 引用（`table.c:44-64`、`com.h`）与现有测试间接核对。

---

## 复检（cbc105c7）

- 复检基线：HEAD `b079fa0e3`（`docs(boot): test count 189 and design case decided`），文档 sha `cbc105c790057db8ca93a63aa2ba7b94b214425d8b151e8c6e7a4867eb53ac9f`；`wc -l` = 2953（sha 与调度方一致，行计数口径差异同前）。
- 代码未变：`git diff --stat 5ce3f9144 -- os` 无输出；D-120/D-121 的代码结论沿用前文，不需要重跑行为门。

四处命名残留的复检命令：

```text
$ rg -n "KernelInfo::validate" rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
2162: ... D 在接收侧重查同一对关系并追加模块 vs 模块（`KernelInfo::validate_requiring_sources` 的两段断言 ...）
2529: ... D `KernelInfo::validate_requiring_sources`（模块 × 模块 + 内核 × 模块）
2825: ... 随后 `KernelInfo::validate_requiring_sources` 的 strict 门
2944: ... D `KernelInfo::validate_requiring_sources`（模块 × 内核跨度 + 模块 × 模块，接收侧复核 ...）
```

- 原报告指出的 §4.3（`:2162`）、§4.7（`:2529`）、§6.3（`:2825`）、§7.5（`:2944`）四处均已改为 `validate_requiring_sources`；§6.3 的“strict 门”与新名字一致，不再有“非 strict 方法名 + strict 门”的矛盾。
- 其余 `KernelInfo::validate` 命中（`:1186`、`:1335`、`:2377`、`:2589`、`:2615`）都处于“非 strict 变体保留给测试/诊断、`kmain` 不调用”的语境；其中 `:2377` 明确写“`KernelInfo::validate`（非 strict，保留给测试/诊断，`kmain` 不再调用）”，与代码一致，不需要改名。`:2615` 的 `KernelInfo::validate_*` 是通配写法，覆盖两个入口。
- D-120 结论复核：权威表、两个派生视图、`proc.rs` re-export、`uboot_main` 删除第三份抄本、xtask typed 读取均未变（代码 diff 为空）；原报告的 2→1 精确口径结论继续成立。
- D-121 结论复核：`kmain` 的 `kernel_info.validate_requiring_sources()` + `check_boot_postconditions()` 调用未变（`os/kernel/src/lib.rs:1867`、`:1869`）；`fw_end=0`/`0x8020_0000` 宿主 fixture 未变；x86/riscv64 QEMU 门的 PASS 读数仍对应当前代码（代码未变，本轮未重跑）。

**RECHECK: PASS**
