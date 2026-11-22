# R2 反证报告（P-REFUTE，D01）

角色：只读反证者。未读作者结论、其他角色报告与历史 panel。
复查对象：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（2026-11-22 当前树，2973 行）。
重点：R1 修改段（pt_alloc、authority、source 分区例外、五条腿门覆盖、aarch64 DTB preflight、§5.5 范围）与测试计数。

结论概览：CONFIRM 9 条，REFUTE 2 条，UNCLEAR 4 条。两条 REFUTE 均可复跑：§5.4 的 aarch64 DTB preflight 描述与脚本实况相反；§5.1 证据矩阵被一行游离正文截断，表格结构已坏。

| 编号 | 主张 | 判定 |
|---|---|---|
| 01 | `pt_alloc::register` 是两种构建档都 fail-closed 的写一次槽 | CONFIRM |
| 02 | pt_alloc 换手表的三次注册与调用点描述 | CONFIRM（“Step 0”标签略不精确） |
| 03 | `register_free` 只服务 VM 侧，未注册时销毁路径只清零根、中间页泄漏 | CONFIRM |
| 04 | `authority` 已成为 `KernelInfo` 字段，`validate_requiring_sources()` 无参 | CONFIRM |
| 05 | `InputObjectId` 只剩 DTB/UEFI 两变体，`StartupObjects` 随 v10 删除 | CONFIRM |
| 06 | UEFI 腿 source 分区只写 wire，`BootStorage` 的 source 切片为空 | CONFIRM |
| 07 | v1 接收序：strict → postconditions → resume → boot_validate，且不过 pre-table | CONFIRM |
| 08 | §5.1 I3 “五条腿都有门覆盖” | UNCLEAR（与 §5.4 的“门链五条”成员不同） |
| 09 | §5.4 aarch64 DTB preflight 的描述（“无 DTB 配置”“已声明无 DTB”） | REFUTE |
| 10 | §5.4 其余四条门链的判据与脚本一致 | CONFIRM |
| 11 | §5.0/§5.2 宿主测试计数 188/136/866/0/8/27 | CONFIRM |
| 12 | §5.1 证据矩阵表格完整性 | REFUTE |
| 13 | §5.5 “引导交接范围内没有未闭环的测试缺口” | UNCLEAR |
| 14 | 扣除不在 `pt_alloc`，而在 `build_vm_handoff` 调用的 `classify` | CONFIRM |

---

## CLAIM 01

主张（§3.4.2 / §4.6）：`pt_alloc::register` 用 `compare_exchange(0, …)` 抢唯一写权，第二次注册 panic，且该 panic 在 release 也生效，不存在“release 静默保留第一个注册者”的分支。

命令：

```bash
sed -n '45,75p' os/arch/src/arch/pt_alloc.rs
```

证据（逐字）：

```rust
PT_ALLOC
    .compare_exchange(0, alloc_fn as usize, Ordering::AcqRel, Ordering::Acquire)
    .unwrap_or_else(|_| panic!("pt_alloc::register called twice"));
```

同段的源码注释写明 “Write-once is enforced in both profiles. A second registration is a boot bug: silently keeping the first allocator hides a wiring error, so fail closed in release too.” 没有 `cfg(debug_assertions)` 分支。`alloc_pt_page` 另有 `assert!(raw != 0, "pt_alloc::alloc_pt_page called before register")`。

**VERDICT: CONFIRM**

---

## CLAIM 02

主张（§3.4.2 表）：“换手由一次初始化与三次注册组成”：第 1 行只初始化 bump 池不注册；第 2 行 shim 与接收镜像各注册一次（同一 helper `boot_validate_and_prepare`）；第 3 行 VM 注册。且“shim 侧不直接调 `register`”。

命令：

```bash
grep -n 'init_boot_pt_alloc\|pt_alloc::register\|is_registered' \
  os/boot-shim/src/uefi_helpers.rs os/boot-shim/src/uboot_main.rs \
  os/kernel/src/lib.rs os/servers/vm/src/vm_server.rs
sed -n '918,930p' os/kernel/src/lib.rs
```

证据：

- `os/boot-shim/src/uefi_helpers.rs:1949`（当前行号约）只有 `minix_kernel::boot_alloc::init_boot_pt_alloc(bump_base, bump_end);`，全文件无 `pt_alloc::register`。
- `os/kernel/src/lib.rs:924-926`：

  ```rust
  if !pt_alloc::is_registered() {
      pt_alloc::register(boot_alloc::boot_pt_alloc);
  }
  ```

- 该 helper 同时被 `prepare_kernel_image_v1`（shim 进程内建根前）与 `resume_additive`（接收镜像）调用；两个二进制各有自己的 `PT_ALLOC` 静态，因此是两次独立注册。
- `os/servers/vm/src/vm_server.rs:381/389` 是第三处注册（`register` + `register_free`）。

唯一措辞问题：§3.4.2 把 shim 侧那次称为“Step 0”。在文档自己的编号里 §4.1 的 Step 0–4 是 `arch_boot_impl` 的阶段，而 shim 的注册点是 `prepare_kernel_image_v1 → boot_validate_and_prepare`，不是 Step 0。这是标签借用，不改变事实，但跨节阅读会误指。

**VERDICT: CONFIRM**（附措辞保留：shim 侧那次不应叫“Step 0”）

---

## CLAIM 03

主张（§3.4.2 末）：`register_free` 只对应 VM 侧池；两者成对，未注册归还钩子时销毁路径只清零根、中间页表页泄漏。

命令：

```bash
grep -rn 'register_free' os --include=*.rs | grep -v '/target/'
```

证据：生产调用点只有 `os/servers/vm/src/vm_server.rs:389`；`os/arch/src/x86_64/paging.rs:1923` 在 `#[cfg(test)]` 内（测试用），`os/arch/tests/pt_alloc_write_once.rs` 是集成测试。`pt_alloc::free_pt_page` 的文档注释逐字写着 “destroy paths gate the full-reclaim walk on `is_free_registered()` and fall back to zero-root-only when absent”。

**VERDICT: CONFIRM**

---

## CLAIM 04

主张（§3.2、§4.4）：`authority` 是 `KernelInfo` 的第 10 个字段；`validate_requiring_sources()` 不再收参数，直接读 `KernelInfo.authority`。

命令：

```bash
grep -n 'pub struct KernelInfo' -A 16 os/libs/minix-boot/src/kernel_info.rs
sed -n '240,250p' os/libs/minix-boot/src/kernel_info.rs
grep -n 'authority: scalars.ram_authority' os/kernel/src/lib.rs
```

证据：结构体 10 个字段，末项 `pub authority: RamAuthority`；函数签名为 `pub fn validate_requiring_sources(&self)`；接收侧构造时写 `authority: scalars.ram_authority`。`handoff_v1_read.rs:161-163` 只接受 0/1，其余返回 `BadRamAuthority`。

**VERDICT: CONFIRM**

---

## CLAIM 05

主张（§3.2、§3.5、§4.2）：“`InputObjectId` 现在只有 DTB/UEFI 两个变体，`StartupObjects` 与 `startup_record()` 已随 v10 删除”。

命令：

```bash
sed -n '585,612p' os/libs/minix-boot/src/memmap_source.rs
grep -rn 'StartupObjects\|startup_record' os/libs/minix-boot/src os/kernel/src | head
```

证据：`enum InputObjectId { DtbBootInput, UefiMemoryMap }`，紧随的注释是 “Numeric tag 2 is retired with the v10 startup-object set; it stays reserved”。全仓 `StartupObjects`/`startup_record` 零命中（grep 无输出）。`handoff_v1.rs:112` 与 `:121` 分别保留 “Kind 6 (startup object) is retired” 与 “Origin 5 (startup object) is retired”。

保留：文档还写了“wire 标签 2（输入对象）…只作保留位”，这一句与 `InputObjectId` 的 tag 2 保留注释对得上，但本轮没有单独追踪 `Serializer` 写入该 tag 的代码路径，只核了变体集合与读者。

**VERDICT: CONFIRM**

---

## CLAIM 06

主张（§3.2、§3.7、§4.2、§4.4）：UEFI 腿的 source 分区由分类器直写 wire，`BootStorage` 的 source 切片为空；`fill_source_records` 只有 DTB 腿调用。

命令：

```bash
grep -rn 'fill_source_records' os --include=*.rs | grep -v '/target/'
sed -n '95,130p' os/libs/minix-boot/src/boot_storage.rs
grep -n 'Serializer::new' os/boot-shim/src/uefi_helpers.rs
```

证据：

- 生产调用点只有 `os/boot-shim/src/opensbi_helpers.rs:316` 与 `os/kernel-image/src/bootface_common.rs:545`；`uefi_helpers.rs` 无调用。
- `BootStorage::new` 里 `source_records_len: 0`、`sources_committed: false`；`report()` 返回 `source_records: self.source_records()`，未提交时是空切片。
- `uefi_helpers.rs` 的 `Serializer::new(arena, InputObjectId::UefiMemoryMap)` 先写权威；`ebs_pair::pair_and_classify` 的分类计划把 UEFI source 记录直接写进 wire（`ebs_pair.rs:239` 注释：“`ClassificationPlan::commit` already wrote `sources_byte_len` for the UEFI records”）。

**VERDICT: CONFIRM**

---

## CLAIM 07

主张（§4.4.1）：v1 接收面执行序是 strict → `check_boot_postconditions` → `resume_boot_pt_alloc` → `boot_validate_and_prepare`（在 `resume_additive` 内），且 `resume_additive` 体内没有 `validate_before_tables`。

命令：

```bash
sed -n '745,753p' os/kernel/src/lib.rs
grep -n 'validate_before_tables' os/kernel/src/lib.rs
sed -n '299,312p' os/kernel/src/lib.rs
```

证据：接收入口逐行是

```rust
kinfo.validate_requiring_sources();
kinfo.check_boot_postconditions();
boot_alloc::resume_boot_pt_alloc(scalars.bump_base, scalars.bump_next, scalars.bump_end);
resume_after_v1_decode(&kinfo, scalars.root_page)
```

`validate_before_tables` 在 `lib.rs` 只出现于 `arch_boot_finish`（直核家族），`resume_additive` 开头直接调 `boot_validate_and_prepare`。

**VERDICT: CONFIRM**

---

## CLAIM 08

主张（§5.1 I3 行）：“五条腿都有门覆盖”。

反证要点：§5.4 的“真机门链五条”成员是 x86 UEFI、aarch64 UEFI、riscv64 U-Boot 直载测试链、riscv64 U-Boot shim、riscv64 直核 bootface；aarch64 直核面**不在**表内，改由 `test-smp-aps-aarch64.sh` 驱动，判据是 `nk4c: ap-arrived` 与 `nk4c: ap-irq-ready`，不是启动路标。§3.6 的换算表自己也写了“两个‘5’成员不同：…真机门链五条含直载测试链、不含 aarch64 直核面”。

命令：

```bash
grep -n 'AP_MARKER\|AP_IRQ_MARKER\|TEST_RESULT' os/qemu-tests/test-smp-aps-aarch64.sh | head
grep -n '不含 aarch64 直核面' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
```

因此“五条腿都有门覆盖”只有在把 SMP 交付脚本也算作“门”时才成立，而同一句后半又承认“aarch64 直核只到 SMP 交付（部分覆盖）”。这是术语碰撞：§5.4 的“门链五条”与 §3.6 的“生产腿五条”不是同一集合。读者若按 §5.4 读，会得出 aarch64 直核没有任何门的结论；若按 §3.6 读，会以为 §5.4 的表漏了一行。文档没有把这两个“五”在同一句里分清。

**VERDICT: UNCLEAR**

---

## CLAIM 09

主张（§5.4 aarch64 UEFI 行）：“前两条固定标记 + 第三条变体（`memmap source`，或无 DTB 配置下的 `DTB preflight failed` fail-closed 路径）；后者只在前两条固定标记同时出现且该固件配置已声明无 DTB 时判 PASS”。

脚本实况（`os/qemu-tests/test-shim-bootmarks-aarch64.sh`）：

- 默认配置（无 `DTB_CFGTABLE`）：`MARK3 = "boot-shim: memmap source: uefi-getmemorymap (no dumpdtb.dtb on ESP)"`，PASS 要求 MARK1/MARK2/MARK3 同时出现。这是“没有 DTB 可用”的配置，期望的是 memmap source 行，不是 `DTB preflight failed`。
- `DTB_CFGTABLE=1`：`-machine virt,gic-version=3,acpi=off`，EDK2 把机器 DTB 发布到 UEFI 配置表；脚本注释与 `MARK_DTB_REFUSED` 明说这条腿验证“**存在但不可用**”的拒绝分支（AAVMF 的 DTB 页不是 `EfiACPIReclaimMemory`），期望行是 `boot-shim: DTB preflight failed: DtbPages(NotAcpiReclaim…`。

命令：

```bash
grep -n 'DTB_CFGTABLE\|MARK_DTB_REFUSED\|MARK3=\|MARK4=' \
  os/qemu-tests/test-shim-bootmarks-aarch64.sh
sed -n '158,196p' os/qemu-tests/test-shim-bootmarks-aarch64.sh
```

由此文档把两个配置对调了：`DTB preflight failed` 出现在“有 DTB 但不可用”的强制 DT 配置，不在“无 DTB 配置”；默认无 DTB 配置的判据是 memmap source 行。后半句“该固件配置已声明无 DTB 时判 PASS”在脚本里也没有对应检查——脚本只 grep 字符串，没有任何“声明无 DTB”的输入或断言。

**VERDICT: REFUTE**

---

## CLAIM 10

主张（§5.4 其余四行）：x86_64 UEFI 的五个路标、riscv64 U-Boot 直载测试链的 `### TEST_RESULT: PASS hello-boot-riscv64 ###`、riscv64 U-Boot shim 的 6/6 路标、riscv64 直核 bootface 的三条行序断言，都与脚本一致。

命令与证据：

```bash
grep -n 'MARK[1-5]=' os/qemu-tests/test-shim-bootmarks-x86.sh
grep -n 'TEST_RESULT: PASS hello-boot-riscv64' os/qemu-tests/test-riscv64-uboot.sh
grep -n 'n[1-6]=0\|6/6 markers' os/qemu-tests/test-riscv64-uboot-shim.sh
grep -n 'B1=0; B2=0; B3=0\|table holds 0x0000000000000000 entries\|ln_tab -gt' \
  os/qemu-tests/test-kernel-image-riscv64-bootface.sh
```

- x86：MARK1–MARK5 与文档五条逐字相同，PASS 要求五条同时出现。
- U-Boot 直载：PASS 行逐字相同，宿主 U-Boot 不兼容时 exit 2 SKIP 也与文档一致。
- U-Boot shim：六条 `n1..n6` 的判据与文档六条逐字对应，PASS 行打印 `6/6 markers`。
- bootface：B1 表址行、B2 `table holds 0x0… entries` + `module placement failed`、B3 行序 `memmap < table < MISSING`，与文档“表址行、table holds 0x0 entries、module placement failed 的行序”一致。

**VERDICT: CONFIRM**

---

## CLAIM 11

主张（§5.0 最小示例与 §5.2）：`cargo test -p minix-boot --lib` 188/0、`-p boot-shim --lib` 136/0、`-p minix-kernel --features mock --lib` 866/0/8、`-p minix-elf --lib` 27/0。

命令（`os/` 下执行）：

```bash
cargo test -q -p minix-boot --lib -- --list | grep -c ': test$'
cargo test -q -p boot-shim --lib -- --list | grep -c ': test$'
cargo test -q -p minix-kernel --features mock --lib -- --list | grep -c ': test$'
cargo test -q -p minix-elf --lib -- --list | grep -c ': test$'
grep -rn '#\[ignore = ' os/kernel/src | wc -l
```

输出：`188`、`136`、`874`、`27`；`#[ignore = …]` 实际属性 8 条（`debug.rs` 2、`stacktrace.rs` 3、`clock.rs` 1、`syscall.rs` 1，另 `syscall.rs` 内联用例 1，合计 8）。874 总数减 8 忽略 = 866 运行，与文档 `866/0/8` 一致。`rewrite-notes/meta/coordination/todo-ledger/01-stage-kernel.yaml` 的 closed 条目也记录了同一组读数（minix-boot 188/0、boot-shim 136/0、minix-kernel mock 866/0/8）。

说明：`--list` 只证数量，不证“0 失败”。本轮没有在 5 分钟窗口内重跑全套测试，因此这条的“全绿”部分按 L2 证据（数量 + 台账记录）处理，不是 L1 运行输出。

**VERDICT: CONFIRM**（数量为 L1；全绿为 L2）

---

## CLAIM 12

主张（§5.1）：四条不变量的证据矩阵是一张四列表。

实况：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 第 2636 行是游离正文，位于 I1 行与 I2 行之间：

```text
门覆盖不等于成功路径覆盖；逐行读“不证明什么”列，尤其 aarch64 直核只到 SMP 交付。
```

它前面是 I1 表格行，后面空一行再起 I2 行。Markdown 解析会把 I1 及其表头当成一张闭合表，I2/I3/I4 三行成为没有表头的第二张表（或无表正文）。渲染后“不变量 / 宿主证据 / 真机证据 / 不证明什么”四列语义只覆盖 I1 一行，I2–I4 失去列名。这是 R1 修改段内的格式回归，不是内容争议。

命令：

```bash
sed -n '2633,2641p' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
```

**VERDICT: REFUTE**

---

## CLAIM 13

主张（§5.5）：“在 §5.1–§5.4 列出的引导交接范围内没有未闭环的测试缺口”。

反证要点：这句话的成立依赖把“未闭环”重新定义为“不在本表范围”。同一篇 §5.1 的 I3 行明写：riscv64 `bootface` 与 aarch64 UEFI 的 `DTB_CFGTABLE` 形态只验证 fail-closed 错误路径，不证明走到 `kmain`；aarch64 直核只到 SMP 交付。§5.4 的 aarch64 UEFI 行把正常形态算作 PASS，但没有覆盖“存在且可解析的 DTB”正例（脚本注释：该正例只有宿主单测）。这些不是“已关闭”，而是“按范围声明不计入”。§5.5 后半句确实重申了“I3 的真机覆盖按上表限定为‘五条腿都有门’，不把 fail-closed 形态算成走到 `kmain`”，所以它没有隐瞒，但“没有未闭环的测试缺口”这个断言本身强于证据：它把覆盖度不足改写成了范围外。

台账侧：`rewrite-notes/meta/coordination/todo-ledger/01-stage-kernel.yaml` 中与本文相关的条目均为 `closed`/`historical`，没有 open 的 R02-T 编号；另一条 closed 记录写着“本轮文档-only，未重跑五条真机门链”。因此“账本上没有 open 项”成立，“测试缺口不存在”不成立。

**VERDICT: UNCLEAR**

---

## CLAIM 14

主张（§3.4.2）：“扣除那一侧不在 `pt_alloc` 里，而在构造 VM 交接记录时（`vm_handoff.rs` 的 `build_vm_handoff` 调用的 `classify`）”。

命令：

```bash
grep -n 'pub fn classify\|pub fn build_vm_handoff\|let classification = classify' \
  os/kernel/src/vm_handoff.rs
```

证据：`classify` 定义在 `vm_handoff.rs:452`，`build_vm_handoff` 在 `:575`，函数体内 `:581` 为 `let classification = classify(kernel_info, root_paddr, vm_alloc);`。`pt_alloc.rs` 只有分配/归还函数指针槽，没有 `FreeRangeSet` 或扣除逻辑。

**VERDICT: CONFIRM**

---

## 附：本轮未覆盖与证据强度声明

- 未运行五条 QEMU 真机门（时间窗口限制），门链判据只做脚本静态核对（L1 读源码，非运行输出）。
- 测试计数为 `cargo test --list` 的 L1 输出；"0 失败"取自台账记录，未在本轮重放 `cargo test` 运行。
- 未读 `panel/round2/INDEX.md`、作者结论文档、其他角色报告与历史 panel，符合只读反证约束。
