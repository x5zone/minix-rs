# D01 DESIGN-02 Manuscript 设计批计划（授权：D-119）

- 时间：2026-11-23 04:32（北京时间）
- 目标：五条生产腿的发送侧统一为借用视图；直核发送侧不再构造 `KernelInfo`，不再把借用抬成 `'static`。
- 选定路线：**(b)** 直核入口收 `&BootReport<'_>` + `RamAuthority`；接收侧先把切片深拷进内核
  `.bss`，再生成唯一的 `KernelInfo<'static>`。
- 回退：只有 (b) 被证据证明无法共享校验或无法完成 `.bss` 落地，才回退 (a) `KernelInfo<'a>`；
  回退写一页理由（`DESIGN-02-ROLLBACK.md`），不静默切换。
- 约束：不改 wire 格式、syscall ABI、启动次序、外部可观察行为；可改进程内 Rust 类型/签名与
  测试固定装置；不改 `minix3/`；不碰 02/03 stage；不做无关重构；不新增 defer。
- 停止口：新架构级阻塞或授权内无法解决的测试失败 → 停，写案件卷。
- 时间口：北京时间 09:00 进入峰值计费；到点暂停，写交接简报，等作者手动重启。

## 当前代码事实

1. `os/kernel-image/src/bootface_common.rs::run_self_boot`：
   - `static STORAGE: boot_shim::BootStorageCell`，`STORAGE.take()` 得到 `&mut BootStorage`；
   - 填六类载荷后 `STORAGE.report_static(...)` 得到 `BootReport<'static>`；
   - `build_kernel_info(&report, RamAuthority::DTB)` 构造 `KernelInfo`；
   - `kernel_info.validate_requiring_sources()`，再 `minix_kernel::arch_boot(&kernel_info, root_page)`。
2. `os/boot-shim/src/lib.rs`：
   - `build_kernel_info(&BootReport<'static>, RamAuthority) -> KernelInfo`；
   - `BootStorageCell`（`UnsafeCell<BootStorage>` + `taken` 原子位）、`take()`、`report_static()`。
3. `os/kernel/src/lib.rs`：
   - 三个 `arch_boot` cfg 臂（x86_64/aarch64/riscv64）收 `&KernelInfo`；
   - `arch_boot_finish(&KernelInfo, root_page)` 做 `validate_before_tables` →
     `check_boot_postconditions` → `store_kernel_info` → `arch_boot_impl` → `jump_to_kmain`；
   - `store_kernel_info(&KernelInfo) -> &'static KernelInfo` 把六个切片 land 到 `.bss`，
     构造 `k: KernelInfo`（切片指向 `.bss`）写 `KERNEL_INFO`；
   - v1 接收面 `resume_additive` 解码 wire 后也走 `store_kernel_info`。
4. `os/libs/minix-boot/src/kernel_info.rs`：
   - `KernelInfo` 的切片字段是 `&'static [T]`；
   - `validate`/`validate_before_tables`/`validate_requiring_sources`/`validate_impl_inner`
     直接读 `self` 字段与 `self.authority`。
5. `os/libs/minix-boot/src/boot_storage.rs`：
   - `BootReport<'a>` 是六个切片 + 三个标量的借用视图；`BootStorage::report()` 返回它。
   - v1 发送侧（UEFI/U-Boot）继续用 `BootStorage` 填 wire；本轮不删 `BootStorage`。

## 实施步骤

### 1. 共享校验（不改语义）

在 `kernel_info.rs` 增加一个 crate 内视图 trait，例如：

```rust
pub(crate) trait KernelInfoView {
    fn memmap(&self) -> &[MemoryRegion];
    fn reserved_regions(&self) -> &[MemoryRegion];
    fn boot_modules(&self) -> &[BootModule];
    fn platform_sources(&self) -> &[PlatformDescSource];
    fn param_buf(&self) -> &[u8];
    fn source_records(&self) -> &[SourceRecord];
    fn kern_virt_base(&self) -> VirBytes;
    fn kern_phys_base(&self) -> PhysBytes;
    fn kern_size(&self) -> u64;
}
```

- `KernelInfo` 实现它；`BootReport<'_>` 也在 `kernel_info.rs` 实现（同 crate，无环）。
- 把 `validate_impl_inner` 改成 `validate_view<V: KernelInfoView>(view: &V, authority: RamAuthority, level)`；
  `KernelInfo` 的三个方法委托 `validate_view(self, self.authority, level)`。
- 给 `BootReport<'_>` 增加：
  `validate(authority)`、`validate_before_tables(authority)`、
  `validate_requiring_sources(authority)`、`check_boot_postconditions(authority)`，
  委托同一个 `validate_view`。
- 保持断言文本、顺序、条件完全不变。

### 2. 接收侧落地（唯一 `KernelInfo<'static>`）

把 `os/kernel/src/lib.rs::store_kernel_info` 的落地逻辑泛化成：

```rust
fn land_and_store<V: KernelInfoView>(view: &V, authority: RamAuthority) -> &'static KernelInfo
fn store_kernel_info(ki: &KernelInfo) -> &'static KernelInfo          // wrapper, v1 保留
fn store_boot_report(r: &BootReport<'_>, authority: RamAuthority) -> &'static KernelInfo
```

- `land_and_store` 仍把六个切片 land 到现有 `.bss` stores（`RESERVED_REGION_STORE`、
  `MEMMAP_REGION_STORE`、`BOOT_MODULE_STORE` + 名字池、`PLATFORM_SOURCE_STORE`、
  `PARAM_BUF_STORE`、`SOURCE_RECORD_STORE`），再从这些 store 的 `static` 数组构造
  `KernelInfo`（切片 `'static`）。
- 同址 no-op、名字池截断、param 清零尾巴、第二次 store panic 的行为保持不变。
- `store_kernel_info` 变 wrapper，v1 的 `resume_additive` 不受影响。
- `arch_boot_finish` 改为：
  ```rust
  fn arch_boot_finish<A, H>(report: &BootReport<'_>, authority: RamAuthority, root_page: PhysBytes) -> !
  {
      report.validate_before_tables(authority);
      report.check_boot_postconditions(authority);
      let kinfo = store_boot_report(report, authority);
      register_panic_diagnostic();
      arch_boot_impl::<A>(kinfo, root_page);
      unsafe { H::jump_to_kmain(kinfo) }
  }
  ```
- 三个 `arch_boot` cfg 臂改为收 `&BootReport<'_>, RamAuthority`，转调 `arch_boot_finish`。

### 3. 直核发送侧

- `bootface_common.rs` 删除 `static STORAGE`、`BootStorageCell::take()`、`report_static()`、
  `build_kernel_info()`。
- 改成栈上 `let mut storage = BootStorage::new();`，照旧 `fill_*`，然后
  `let report = storage.report(kern_virt_base, kern_phys_base, kern_size);`
  `report.validate_requiring_sources(RamAuthority::DTB);`
  `minix_kernel::arch_boot(&report, RamAuthority::DTB, root_page)`。
- 若 10.5 KiB 栈上 `BootStorage` 造成栈溢出（启动栈 64 KiB），改用直接由
  `DtbMapOwned`/模块表/`[u8;1024]`/source 记录构造 `BootReport<'_>`；
  这是同一 (b) 的形态，不算回退 (a)。
- `os/boot-shim/src/lib.rs` 删除 `build_kernel_info`、`BootStorageCell` 及其 impl/Default；
  检查 re-export 与测试调用点：`opensbi_helpers.rs:765/824`、`uefi_helpers.rs:1438` 的
  测试改为直接构造 `KernelInfo`（测试静态切片）或构造 `BootReport` 后走接收侧路径。
- 检查 `bootface_common.rs` 的 `use`，删除不再使用的 `BootStorageCell` 导入。

### 4. 文档同步

- §3.2：`KernelInfo` 字段表/产出入径去掉“直核发送侧直接构造 `KernelInfo`”；说明所有五条腿
  发送侧都是 `BootReport<'_>`，接收侧唯一 `KernelInfo<'static>`。
- §3.7：所有权段落删“`'static` 只由 `report_static` 的 one-shot 静态存储支撑”，改为
  “发送侧借用视图；接收侧 `.bss` 深拷后生成唯一 `'static` 视图；`BootStorageCell`/`report_static`
  不再存在”。
- §4.4：两条产出入径改为“五条腿统一：发送侧 `BootReport<'_>` → 接收侧落 `.bss` →
  `KernelInfo<'static>`”；§4.4 字段表/门链不变。
- 其他提及 `STATIC STORAGE`/`report_static`/`build_kernel_info` 的段落同步清理（`rg` 全扫）。

### 5. 验证

- `cargo test -p minix-boot --lib`（基线 188/0）
- `cargo test -p minix-kernel --features mock`（基线 866/0/8）
- `cargo test -p boot-shim --lib`（基线 136/0）
- `cargo check -p minix-kernel-image` / 相关 target check
- 按 `AGENTS.md` 跑启动链 QEMU 门（至少 x86_64 UEFI、aarch64 UEFI、riscv64 U-Boot shim/
  bootface；具体脚本见 `os/qemu-tests/README.md`）
- 机械门：`cargo fmt --check`、`cargo clippy`（受影响 crate）
- 文档门：publication/rubric/anchor/ref/density/citation/style
- 独立 verifier：代码 + 文档各一名，复核四条验收 + Doc Code Sync
- 重设基线：新 commit + 文档 sha/lines + 测试计数；写 `v312/MANUSCRIPT-DESIGN-02.md`
  完成记录，然后重开 R1 面板（新 run 或新 edition，按 §四.6）

## 验收清单（D-119）

- [ ] `rg -n 'report_static|BootStorageCell|STORAGE' os/` 只允许命中文档/历史说明，不允许命中代码；
- [ ] `rg -n 'build_kernel_info' os/` 0 命中；
- [ ] 直核路径不再构造 `KernelInfo`（`bootface_common.rs` 只构造 `BootReport<'_>`）；
- [ ] `minix-boot`、`minix-kernel --features mock`、`boot-shim` 测试全绿；
- [ ] QEMU 启动链门按 `AGENTS.md` 通过；
- [ ] 文档 §3.2/§3.7/§4.4 与代码同步，`rg` 无旧 API 描述；
- [ ] 独立 verifier PASS；
- [ ] 基线重设 + R1 面板重开记录。

## 停止/回退

- (b) 若在实现中无法共享校验（trait 方案失败）或无法完成 `.bss` 落地（借用/生命周期/
  栈容量问题），写 `DESIGN-02-ROLLBACK.md`（一页：证据、为什么 (b) 不行、(a) 的迁移面、
  为何 (a) 不改变外部行为），再改 (a)。
- 出现新架构级阻塞或授权内无法解决的测试失败：停，写案件卷，不硬做。
- 09:00 峰值前未完成：写 `MANUSCRIPT-HANDOFF.md`，保留工作树/commit/测试读数，暂停。
