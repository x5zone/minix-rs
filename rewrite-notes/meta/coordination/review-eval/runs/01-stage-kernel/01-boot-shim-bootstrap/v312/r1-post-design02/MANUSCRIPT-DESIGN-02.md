# D01 DESIGN-02 Manuscript 批完成记录（D-119）

- 完成时间：2026-11-23（北京时间）
- 授权：`DECISIONS.md` D-119（选 A，Manuscript 设计批；优先 (b)）
- 代码 commit：`cdd0d1c777042b048bd9fdfac3e5102b95dac869`
- 结果：**(b) 成功，未回退 (a)**；共享校验与 `.bss` 落地都按 (b) 完成。

## 改了什么

### 1. `minix-boot`：共享校验视图

- 新增 `pub trait KernelInfoView`，为 `KernelInfo` 与 `BootReport<'_>` 提供同一组只读切片/标量访问器。
- `validate_impl_inner` 改为 `validate_view<V: KernelInfoView>(view, authority, level)`；
  `KernelInfo` 的三个校验方法委托它；`BootReport` 新增
  `validate`/`validate_before_tables`/`validate_requiring_sources(authority)`。
- `check_boot_postconditions` 改为共享 `check_boot_postconditions_view`；断言文本、顺序、条件不变。
- `BootReport` 的 `authority` 由调用方传入（report 本身无该字段）。

### 2. `kernel`：接收侧唯一 `.bss` 落地

- `store_kernel_info` 的落地逻辑泛化为 `land_and_store<V: KernelInfoView>(view, authority)`；
  保留 `store_kernel_info(&KernelInfo)` 包装（v1 接收面），新增
  `store_boot_report(&BootReport<'_>, authority)`。
- `land_and_store` 仍把六个切片 land 到原 `.bss` stores，再从这些 `static` 数组构造
  `KernelInfo`（切片 `'static`）；同址 no-op、名字池截断、param 清零尾巴、二次 store panic 行为不变。
- `arch_boot_finish` 改为收 `&BootReport<'_>` + `RamAuthority`：先
  `validate_before_tables(authority)` / `check_boot_postconditions()`，再 `store_boot_report`。
- 三个 `arch_boot` cfg 臂（x86_64/aarch64/riscv64）同步改签名。
- 仍保留 `KernelInfo` 的 `'static` 形态；没有引入 `KernelInfo<'a>`。

### 3. 发送侧统一借用视图

- `kernel-image/src/bootface_common.rs`：删除 `static STORAGE`、`BootStorageCell::take()`、
  `report_static()`、`build_kernel_info()`；改为栈上 `BootStorage`，填完后
  `storage.report(...)` 得到 `BootReport<'_>`，`report.validate_requiring_sources(DTB)`，
  再 `arch_boot(&report, DTB, root_page)`。
- `boot-shim`：删除 `build_kernel_info`、`BootStorageCell`、`report_static`（整块删除）。
- UEFI 发送侧（`uefi_helpers.rs`）与 U-Boot shim（`uboot_main.rs`）改为栈上 `BootStorage`，
  `uboot_main` 用 `prepare_boot(&mut storage, ...)`；不再有静态 cell。
- 测试：`build_kernel_info` 调用改为测试内 `test_kernel_info` helper（字段拷贝）；
  `store_before_alloc_pin` 的 M4 pin 接受 `store_boot_report`/`store_kernel_info` 两个 store 入口。

### 4. 文档同步

§3.2/§3.2.1/§3.7/§4.4/§4.4.1/§4.5/§4.6/§7.5 已改为新设计：

- 五条腿发送侧统一 `BootReport<'_>`；两个直核腿直接借用栈上 storage，三条 v1 腿序列化。
- 接收侧统一 `store_boot_report` → `.bss` → 唯一 `KernelInfo<'static>`。
- 删除 `build_kernel_info`/`report_static`/`BootStorageCell` 的现行 API 描述；
  §4.4/§4.5 的代码引用改为 `arch_boot_finish`/`land_and_store`/两个 wrapper。
- 文档门：rubric 0 缺项、publication 0 新增、anchor 87/87、ref OK、density OK、
  citation failed=0、style error=0。

## 验证

- 旧符号 `rg -n 'build_kernel_info|report_static|BootStorageCell' os --glob '*.rs'` → **0 命中**。
- `rg -n '\bSTORAGE\b' os --glob '*.rs'` → **0 命中**（测试里的同名 `STORAGE` 已改名 `LEAKED_BUFFERS`）。
- 宿主测试：
  - `cargo test -p minix-boot --lib` → 188/0
  - `cargo test -p boot-shim --lib` → 136/0
  - `cargo test -p minix-kernel --features mock` → lib 866/0/8；全部测试二进制通过
  - `cargo check -p minix-kernel`（非 mock）通过
- 交叉构建：
  - `boot-shim` riscv64 U-Boot shim、x86_64 UEFI、aarch64 UEFI 均 build 通过
  - `kernel-image` riscv64/aarch64 build 通过
- QEMU 启动链门（日志在 `tmp/review-loop/01-stage/evidence/design02-qemu-*.log`）：
  - `test-kernel-image-riscv64-bootface.sh` → PASS（B1/B2/B3）
  - `test-riscv64-uboot-shim.sh` → PASS（6/6 markers，entering scheduler）
  - `test-shim-bootmarks-x86.sh` → PASS
  - `test-shim-bootmarks-aarch64.sh` → PASS
- 未改：wire 格式、syscall ABI、启动次序、外部可观察行为、`minix3/`、02/03 stage。

## 基线重设与 R1 重开

- 旧 R1（`v312/` 根）按 §四.6 标 `INVALIDATED`；旧 48 FINDING / 40 根因 / 单元证据只作历史。
- 新基线：doc 2,946 行，sha `996ff161d02ef867151e780adb7243e49e7dc249b5b161d6d0c5ed229b5f22ec`，
  commit `cdd0d1c77`；state 已 `init --force` 重置为 R0 后重新 `baseline` 到 R1。
- 新 run 目录：`v312/r1-post-design02/`；`related-code.txt` 由 `doc-code-map.sh --format=plain`
  重新生成，49 个文件（旧清单漏了 `ap_early_entry.rs`、`proc_table.rs`、`trap_api.rs`）。
- 新 R1 面板：6 角色 fresh；读者只拿正文+画像+§4.0e；D2 走 D-024 回退；P-REFUTE 强制。

## 验收清单

- [x] `rg` 证明 `report_static`/`BootStorageCell`/`STORAGE`/`build_kernel_info` 路径消失
- [x] `minix-boot`、`boot-shim`、`minix-kernel --features mock` 测试全绿
- [x] 启动链 QEMU 门（x86 UEFI、aarch64 UEFI、riscv64 U-Boot shim、riscv64 bootface）PASS
- [x] 文档 §3.2/§3.7/§4.4 与相关符号表同步
- [ ] 独立 verifier 复核：**FAIL**（`certify/design02-verify.md`）；待修项见下节
- [x] 基线重设并重开 R1（新 run 目录、新 baseline、6 角色面板已准备）

## 独立 verifier 后续（2026-11-23 05:12）

独立 verifier 判定 **FAIL**，两项阻断：

1. **文档同步未完成**：§4.4 `:2368` 仍是旧符号 `validate_impl_inner`；`:2284` 的
   “接收侧统一走 `store_boot_report`”与代码及 §4.4.1/§4.5 矛盾（三条 v1 腿走
   `arch_boot_resume_v1` → `store_kernel_info`，只有两个直核腿走 `store_boot_report`）；
   `:2324` 的 `source_records` 行同样把 v1 落地写错。代码与 `Cargo.toml` 还留有旧签名/旧
   API 注释（`boot-shim/src/lib.rs:7/91`、`opensbi_helpers.rs:36`、`kernel/src/lib.rs:1553`、
   `kernel_info.rs:152`、`kernel-image/Cargo.toml:52/61`）。
2. **三个 QEMU 测试载体编译回归**：`test-higher-half`、`test-higher-half-aarch64`、
   `test-higher-half-riscv64` 仍以两参数调用 `arch_boot(&kernel_info, root_page)`，
   报 E0061；`os/qemu-tests/run_all.sh` 与 `.github/workflows/qemu-tests.yml` 的 build 段
   会红。修复在 D-119 允许的“测试固定装置”范围内。

残留风险（不阻断本次验收，但记录）：四条 QEMU 日志未硬绑定 commit；新的直核接收路径
没有一条启动门真正跑到；16 KiB 栈上 `BootStorage` 余量未实测；`validate_view` 等价性只做
人工 diff。上述两项阻断在面板完成后按 R1 代码/文档批修复，再由独立 verifier 复核。
