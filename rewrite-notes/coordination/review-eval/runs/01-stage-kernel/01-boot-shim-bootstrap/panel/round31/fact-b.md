# FACT-B 审查报告（R31）：第 3、4 章与 §5–§6 的 Rust 代码/测试/日志断言

- 审查对象：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
  （3554 行；任务基线 sha256 前缀 `1601d667b0791def`，实测
  `1601d667b0791defc0d751aeab89436451f5ee9d1ace736e0ff0d9746bfb050d`，与基线一致）
- 审查范围：§3.1–§3.10、§4.1–§4.9、§5.1–§5.4、§6.1–§6.4 中所有引用 Rust 代码、测试、路标字符串、
  常量与覆盖状态的断言。C 侧引文只作旁证（`minix3/`），不作主判据。
- 对照源码：`/home/xzhao/github/minix-rs/os/` 的**工作树**（含 `qemu-tests/` 脚本与测试内核）。
  工作树有未提交改动（见下节“基线口径”），本轮以工作树为准。
- 审查方式：只读。逐块把文档引文与源文件函数体对账；测试名逐条 grep；测试计数按逐文件
  `#[test]` 求和并与其 passed/ignored 分解交叉核对；路标字符串逐条 grep 到打印点并核对“适用腿”。
- 审查日期：2026-11-10

## 复核命令与读数

```bash
cd /home/xzhao/github/minix-rs
sha256sum rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md          # 1601d667…（3554 行）
for f in <§5.2.1 列出的 21 个文件>; do grep -c '#\[test\]' $f; done         # 与文档逐文件表逐项一致
grep -rc '#\[test\]' os/kernel/src/*.rs | awk -F: '{s+=$2} END{print s}'    # 874
grep -rn '^[[:space:]]*#\[ignore' os/kernel/src/ | wc -l                    # 8
grep -rn 'BBPattern' …                                                      # 见各 FINDING 反证路径
```

逐文件计数复核结果（全部与 §5.2.1 的表一致）：loader 22、opensbi_helpers 11、uefi_helpers 12、
ebs_map 45、ebs_pair 17、dtb_copy 15、resv_claims 13、ebs_boundary_audit 6、boot-shim lib.rs 1（合计 136）；
kernel_info 32、platform 8、boot_params 8、memmap_source 74、handoff_v1 2、handoff_v1_write 10、
handoff_v1_read 21、fdt_layout 29、minix-boot lib.rs 2、bump 5、boot_storage 5（合计 196）；
minix-elf lib.rs 23。kernel 侧 `#[test]` 总数 874、`#[ignore]` 属性 8（对应 866 passed + 8 ignored）。
`pt_alloc_write_once.rs` 2、`pt_alloc_concurrent.rs` 1、`boot_alloc.rs` 10、`dm_coverage.rs` 10、
`x86_64/paging.rs` 17、`vm_handoff.rs` 21（含 9 条 `free_set_*` + 2 条容量/边界）——与文档一致。

本轮未实跑 `cargo test`（工作树 `os/target` 内存在非本用户持有、权限受限的增量锁文件；
计数已用静态求和与 ignore 属性计数交叉核对，结论不依赖执行）。§5.2.3 的五个门链脚本、
`mdir` 前置检查、退出码分支、CI 只接线 `test-riscv64-uboot.sh` 均已按脚本正文与
`.github/workflows/qemu-tests.yml` 核对。

## 基线口径（工作树 vs HEAD）

`git status` 显示 14 个 `os/` 文件有未提交改动。文档 mtime 18:10，晚于其中大多数改动
（`boot_storage.rs` 18:05、`loader.rs` 18:09、`bootface*.rs` 08:31、`kernel/src/lib.rs` 08:33、
`test-riscv64-uboot.sh` 09:09）。逐项核对结果：`CheckedWindows` 令牌、`map_huge` 的
`Ok|AlreadyMapped` 放行、`store_kernel_info` 返回引用、`BootStorage` 的 `map_dirty`/`*_committed`、
`records_in_wire` 删除、两个 `bootface` 的 `BootStorageCell` 迁移、riscv64 接收臂的
`arena_pa == 0` 停机、`BootAlloc::init` 的自洽断言、`boot_windows_reject_empty_ranges`
——文档均已同步。未同步的两处构成本报告 FACT-B-03 与 SUGGESTION S5（见下）。

---

## FINDING FACT-B-01

**位置**

§4.5“内核侧第一步：store_kernel_info 的七个落地区加一个名字池”，文档第 2835 行：

> 三个生产调用点现在都使用返回值：`kmain` 用 `let kernel_info = store_kernel_info(kernel_info);`
> 继续读同一槽，`kernel_info()` 只留作诊断/测试的第二通道。

**源码证据**

`kernel_info()` 在生产路径上是活访问器，至少五处非测试调用：

- `os/kernel/src/misc.rs:721`（`fn getinfo_kinfo`，GET_KINFO 处理器）：
  `let (user_sp, freepde_start, vir_kern_start) = crate::kernel_info()...`；
  调用点在 `os/kernel/src/misc.rs:620` 的 `GetInfoRequest::KInfo => getinfo_kinfo(...)`。
- `os/kernel/src/misc.rs:1355`（`fn getinfo_image`，GET_IMAGE 处理器）：
  `let boot_modules = crate::kernel_info().map(|ki| ki.boot_modules)...`。
- `os/kernel/src/syscall.rs:2593-2598`（`fn phys_ram_view`）及其生产读者
  （同文件 2615、2759、2787、2813 行）。
- `os/kernel/src/lib.rs:1604`（`fn harden_kernel_text`）：
  `crate::kernel_info().expect("harden_kernel_text: KERNEL_INFO store missing")`。
- `os/kernel/src/lib.rs:1687`（`kmain` 入口的全局副本兜底）。

更直接的内部矛盾：文档同一节上方的引文（第 2813 行）自己就写着
`crate::kernel_info().expect("store_kernel_info: KERNEL_INFO just written")`——即
`store_kernel_info` 的返回值正是经由该访问器取得。

**为什么错**

“只留作诊断/测试的第二通道”把 GET_KINFO、GET_IMAGE、Direct Map 物理视图与内核文本加固
这些生产读者全部排除在外。机制结论（三个 store 调用点使用返回值、`land_slice` 的同址守卫）
仍然成立，但“访问器只服务诊断/测试”这一句与源码相反：读者会据此认为该访问器可在非测试
构建里移除或改写，而它正是 syscall 路径读取 `KernelInfo` 的唯一入口。

**严重度**

P1。事实性错误、局部成立范围被夸大，但不改变本节“深拷与同址跳过”的中心结论；
与 R30 FACT-B-02 同档（“声明完整却漏断言”型）。

**反证路径**

```bash
grep -rn "kernel_info()" os/kernel/src/misc.rs os/kernel/src/syscall.rs os/kernel/src/lib.rs
```

命中行见上；若认为这些调用属于“第二通道”的合法用法，需解释为什么 syscall 处理器算诊断/测试。

---

## FINDING FACT-B-02

**位置**

§3.4.1“交接的两个维度与 `KernelInfo` 结构”，`memmap` 行（文档第 1630 行）：

> 内核镜像在**riscv64 直核自引导腿**与 U-Boot shim 腿的表内；……模块同理：
> **DTB 来源腿**（riscv64 直核与 U-Boot shim）的 bump 页在表内，UEFI 腿是 `LOADER_DATA` 因而不在。

**源码证据**

- `os/kernel-image/src/bootface_a64.rs:203`：
  `let (memmap, reserved, sources) = minix_boot::DtbMemoryMap::new(&fdt, 0)`——aarch64 直核腿
  同样从 DTB `/memory` 取表，且 `fw_end = 0` 表示不做固件前缀裁剪。
- `os/libs/minix-boot/src/memmap_source.rs:895-898` 的文档注释明确写：
  “the **aarch64 direct leg passes 0** (no firmware prefix to carve; the aarch64 UEFI leg also
  passes 0 and clips at classification time)”。
- `os/kernel-image/src/bootface_a64.rs:398-410`：`build_kernel_info(memmap, reserved, …,
  PhysBytes(KERN_PHYS_BASE), KERN_SIZE, …)` 直接使用上表；`bootface_a64` 在构造前只追加
  startup 记录（同文件 337-363 行），**没有**把 `[0x4020_0000, +KERN_SIZE)` 或 bump 池
  `[0x4400_0000, 0x4600_0000)` 从自由表里扣除。因此 aarch64 直核腿的内核镜像与模块 bump 页
  都在 `KernelInfo.memmap` 内，`vm_handoff::classify` 对它是**实际扣除**（`os/kernel/src/vm_handoff.rs`
  的扣除表 `kernel image` 行与 `for_each_live_bootstrap` 调用，见同文件 16-24、427-438 行）。
- 对照：riscv64 直核腿 `os/kernel-image/src/bootface.rs:213` 用 `DtbMemoryMap::new(&fdt, KERN_PHYS_BASE)`
  裁掉 `[0x8000_0000, 0x8020_0000)`，镜像在 `>= 0x8020_0000` 仍在表内。

**为什么错**

该行把“镜像在自由表内”的腿穷举成 riscv64 直核与 U-Boot shim 两条，把“DTB 来源腿”定义为
同样两条，漏掉第三个 DTB 来源腿 aarch64 直核。读者据此会以为 aarch64 直核的镜像/模块像
UEFI 腿一样不在自由表里、分类时内核行退化为空操作；实际相反。文档内部也自相矛盾：
§3.8 的扣除表注释（文档第 2169 行）写“自引导腿与 U-Boot shim 腿仍是实际扣除”，
“自引导腿”按本篇术语包含 aarch64；§3.4.1 自己的第三条 bullet（第 1637 行）也写
“两个直核面……aarch64 传 `fw_end = 0`”。

**严重度**

P2。枚举不完整且与同篇另一节冲突；不改变“UEFI 腿镜像被预订为 LOADER_DATA”的核心结论，
但会让 aarch64 腿的归属被读错（属 R30 P2“声明范围比实际宽/窄”同档）。

**反证路径**

```bash
sed -n '195,210p' os/kernel-image/src/bootface_a64.rs
sed -n '890,900p' os/libs/minix-boot/src/memmap_source.rs
sed -n '390,412p' os/kernel-image/src/bootface_a64.rs
```

若要推翻本条，需要给出 aarch64 直核腿在 `build_kernel_info` 之前把 `KERN_PHYS_BASE/KERN_SIZE`
或 bump 池从 `memmap` 中切掉的代码；工作树中不存在这样的代码。

---

## FINDING FACT-B-03

**位置**

§5.2.3“真机层：探针与各链”，文档第 3273 行：

> 退出码语义（表中五个脚本一致）：0 = PASS，1 = FAIL，2 = SKIP（前置工具缺失）。……
> 没有 U-Boot 的开发机上，riscv64 U-Boot 直载测试链返回 2 而不是 1。

以及 §6.4 第一层，文档第 3450 行：

> 若关键行缺失：aarch64 脚本会打印“出现在/未出现”两行；U-Boot 与 bootface 脚本只回打串口尾部 20 行……

**源码证据**

`os/qemu-tests/test-riscv64-uboot.sh`（工作树 09:09 改动，早于文档 18:10）新增两类宿主兼容 SKIP：

- 同文件 146-150 行：串口出现 `Hit any key to stop autoboot` 且没有 `## Executing script` 时置
  `HOST_GAP=1`；
- 同文件 156-158 行：串口出现 `Unhandled exception: Load access fault` 时置 `HOST_GAP=1`；
- 同文件 159-163 行：`HOST_GAP=1` 时打印
  `SKIP: host U-Boot could not load the script/ELF (kernel never executed)` 与 U-Boot 版本行，
  `exit 2`——此时**没有**回打串口尾部 20 行（tail 在 165 行，SKIP 分支先返回）。
- 同文件 97-98 行另把 `boot.scr` 复制为 `boot.scr.uimg`（U-Boot 2025.10 bootflow 兼容）。

**为什么错**

`2 = SKIP（前置工具缺失）` 的定义现在不完整：这条链在 **U-Boot 存在、QEMU 存在** 但宿主
U-Boot 停在 autoboot 或在装载脚本/ELF 时 trap 的情况下也返回 2。§5.2.3 后文“把 SKIP 当作
FAIL 处理会怎样”只举“没有 U-Boot 与 QEMU 的开发机”，也是同一处的过窄描述。§6.4 的
“U-Boot 与 bootface 脚本只回打串口尾部 20 行”在该 SKIP 分支下也不再成立（SKIP 分支先退出）。
影响是诊断性的：按文档排查“关键行缺失”时，会先去找缺失的前置工具，而实际信号是
`SKIP: host U-Boot could not load the script/ELF` 与 U-Boot 版本行；这条链在这种宿主上
从“FAIL”变成了“SKIP”，覆盖语义也随之改变（CI 直接执行该脚本，exit 2 仍会让作业失败，
但手工运行时的判读不同）。

**严重度**

P2。测试语义/覆盖状态的描述与实际脚本不符；不影响脚本自身的判据正确性。

**反证路径**

```bash
sed -n '136,168p' os/qemu-tests/test-riscv64-uboot.sh
git diff -- os/qemu-tests/test-riscv64-uboot.sh
```

若认为“前置工具缺失”可以泛指宿主环境不兼容，需要同时解释 159-163 行的 SKIP 文案与
该分支跳过 tail 回打的行为。

---

## SUGGESTION

### S1（P3）§3.4.5 的 `*_slots_mut` 通配把 `map_dirty` 机制说宽了

文档第 1757 行：“`map_tables_mut`/`*_slots_mut` 会置 `map_dirty`”。工作树
`os/libs/minix-boot/src/boot_storage.rs` 中只有 `memmap_slots_mut`（:143）、`map_tables_mut`（:153）、
`reserved_slots_mut`（:173）置 `map_dirty`；`module_slots_mut`（:194）、`platform_slots_mut`、
`param_buf_mut`、`memmap_source_slots_mut`、`startup_slots_mut` 都不置，它们靠各自的
`*_committed` 标志约束。按现文写法，读者会以为模块装载后到 `set_modules_len` 之间调用
`report()` 也会 panic（实际不会）。建议改成“三个内存图表槽访问器（`memmap_slots_mut`/
`reserved_slots_mut`/`map_tables_mut`）置 `map_dirty`”。

### S2（P3）§3.4.5 的 `set_startup_len` 不是生产 setter

文档第 1757 行把 `set_startup_len` 与 `set_modules_len`/`set_platform_len`/`set_param_len`/
`set_memmap_sources_len` 并列为“各自只允许提交一次”的提交口。源码里
`os/libs/minix-boot/src/boot_storage.rs:293-294` 是 `#[cfg(test)] pub(crate) fn set_startup_len`；
生产路径（两个 bootface、`prepare_boot`、UEFI 省略段）用的是 `push_startup`（同文件 :305，
也只允许一次，超容量返回 `BootStorageFull`）或直写 wire。建议注明 `set_startup_len` 仅测试可见，
生产提交口是 `push_startup`。

### S3（P3）§4.4 的“`arch_boot` 的调试打印”只对 x86_64 成立

文档第 2675 行：“`boot_validate_and_prepare` 是这份结构在内核侧的入口校验（`arch_boot` 的调试
打印已先读过部分字段）”。三个 `arch_boot` 里只有 x86_64 那份有诊断打印
（`os/kernel/src/lib.rs:239-291` 的 `kernel: arch_boot arg=` / `reslen=` / `entered`）；
aarch64（:292-300）与 riscv64（:786-797）的 `arch_boot` 直接转 `arch_boot_finish`，没有任何打印
（§6.1 的里程碑表也已按“仅 x86_64 测试内核”标注）。建议把这句括注限定为 x86_64，免得与 §6.1 冲突。

### S4（P3）§3.7 “把表与模块序列化进 arena”措辞

文档第 2061 行：“U-Boot shim 腿自建表、不经过此通道——它把表与模块序列化进 arena……”。
跨镜像 wire 携带的是 `BootModule` 记录（name/start/len）与 `PlatformDescSource` 指针
（`os/libs/minix-boot/src/handoff_v1.rs` 的 `WireBootModule`/`PlatformDescSource` 分区），
`BootFileTable` 本身不随信封交付（它留在 shim 的 `.bss`，只用于取件）。建议把“表”改成
“模块表（`BootModule` 记录）”，避免读者以为 `BootFileTable` 跨了边界。

### S5（P3）§4.3 未登记 loader 新增的入口一致性断言

工作树 `os/boot-shim/src/loader.rs:393-401` 在 `load_kernel_with_loader_gated` 里新增
`assert_eq!(layout.entry_point, layout.kern_virt_base.0, "boot-shim: ELF entry does not equal
the first PT_LOAD VMA")`（工作树 18:09 改动，文档 18:10 未提）。它把 §5.2.2 L6 的宿主断言
（入口 = 首段 VMA）复制到装载期，属 §4.3 装载防线的第五条；目前没有对应用例（`grep` 仅命中断言
本身）。建议在 §4.3 或 §5.2.2 L6 行补一句“loader 另有装载期同断言”，并视需要补一条负控。

---

## 已核对且未发现偏差的断言（抽样清单，供反驳用）

- §3.1：`exit_boot_services` 引文、`#[cfg(feature = "qemu-test-kernels")]` 门、
  “生产入口用 raw `ExitBootServices`”与三个入口寄存器的分配。
- §3.2：`prepare_boot` 签名与 `BootPrepareResult<'a>` 字段（与 `boot_prepare.rs` 逐字段一致）、
  `FileLoader`/`UefiFileLoader` 引文、7 处 `#[cfg(feature)]`（6 uefi + 1 opensbi）与 8 处
  `qemu-test-kernels` 门、`bootface_common` 的 `NAME_SLOT=16`/12 槽 `NAME_POOL`/五项检查/
  `PlacementPool`、两腿 bump 池 `0x8200_0000..0x8400_0000` 与 `0x4400_0000..0x4600_0000`。
- §3.3：四份 manifest 的依赖表、`boot-shim → minix-kernel` 调用面仅
  `enter_kernel_image_v1`/`init_boot_pt_alloc`、`BootShim` trait 在当前树零命中、
  `5ed051a53` 上仅一处 `impl BootShim`（`git grep` 复核）。
- §3.4：`KernelInfo` 14 字段顺序、`BootHandoffV1` 22 字段与 `size_of == 160`、
  `BOOT_HANDOFF_V1_VERSION == 9`、`ARENA_CAP=32768`/`WIRE_END=7872`/`WIRE_SOURCES_END=29184`/
  `WIRE_STARTUP_END=30624`、`BootStorage` 五类容量与 40 条来源槽、`KinfoStruct` 48 B 七字段、
  `MAXMEMMAP=40`、`VM_BOOT_HANDOFF_MAX_REGIONS=64`/`MAX_DEDUCTED=47`、
  `MEM VmBootHandoff` 字段名 `kernel_allocated_static`/`kernel_allocated_dynamic`、
  `FREE_MEMMAP` 仅 bootface_a64 注释提及、`kern_ident`/`MAX_IDENT` 已无定义。
- §3.5–§3.6：C `kernel.lds` 引文与 `objcopy --prefix-symbols=__k_unpaged_`、
  184→160 的 wire 收缩、`HugePages` trait 引文、`Paging` 22 个成员（1 const + 21 方法，逐名一致）、
  x86 `HUGE_PAGE_SIZES`/`map_huge` 引文、`plan_kernel_mapping` 阶梯引文、CPUID `0x80000001` EDX bit26。
- §3.7–§3.9：`arch_boot_finish`、`prepare_kernel_image_v1`/`enter_kernel_image_v1`/
  `jump_kernel_image_v1` 引文与三段跳转 asm（含 x86 stub `mov rbx, rsi; mov rax, rdi; cli; cld; jmp rax`）、
  `resume_additive` 文档引文、`pt_alloc` 引文、`BootAlloc::alloc` 引文、`vm_handoff` 扣除表与
  `classify` 引文、三架构 `inherit_supervisor_half` 的 256..512 复制。
- §4.1–§4.3：`main.rs` 入口引文、`prepare_and_enter_kernel_image` 引文与动作顺序、
  `bootstrap_alloc_limit`/`alloc_root_page` 引文、`ebs_map` 五步 + 两闸 + 令牌表
  （`LAST_KNOWN=15`、`KNOWN` 16 常量/31 位、`AMBIGUOUS`/`PROFILE_ONLY`/`TYPE_INDEPENDENT`）、
  `KERNEL_PATH`/`DUMPDTB_PATH`/`MODULES_DIR`、`MODULE_NAMES` 12 项顺序、
  `load_boot_modules_into` 引文、`entry_path_eq`/`UbootFileLoader` 引文、
  `staged_elf_used_len` 的 PT_LOAD 修剪与 loader 二次修剪幂等。
- §4.4–§4.5：`build_kernel_info` 引文、`boot_validate_and_prepare` 引文与四道门顺序、
  `land_slice`/`store_kernel_info` 引文、七落地区 + 名字池容量表（256/128/12/4/1024/296/20/16）、
  七个 `should_panic` 的输入尺寸（257/129/13/5/1025/297/21）。
- §4.6–§4.9：Step 1/Step 2 引文（含 `assert end <= IDENTITY_MAP_END` 与 `AlreadyMapped` 放行）、
  `X86_64Paging::enable` 引文、Step 2b/2c 与 `device_window_regions()` 三架构表、
  `BootAlloc::init`/`resume` 引文（含工作树新断言）、`rust_image_main` 三臂引文、
  `__bss` 清零与栈切换的 trampoline、§4.9 对照行的落点与 `install_at`。
- §5：全部列名测试（`test_store_kernel_info_*`、`test_validate_*`、`test_validate_requiring_sources_*`、
  `test_postconditions_reject_*`、`free_set_*`、`boot_windows_*`、`boot_identity_windows_*`、
  `decoded_map_survives_arena_poison_and_reissues_the_arena_frames`、
  `missing_boot_allocator_registration_fails_closed`、`test_device_flags_set_pwt_and_pcd`、
  `picks_the_preferred_size_when_supported_and_aligned`、pt_alloc 三例、loader/opensbi/uefi 列名用例）
  均存在且计数与文档一致；`build_memmap()` 的 15 个测试内核调用点、`build_memmaps` 无生产调用、
  `check-layout.sh` 的 L0–L9 与退出码、五个门链脚本的路标/`mdir` 前置/CI 接线均核对通过。
- §6：`raw_serial` 引文、`raw_serial_line` 的 LSR bit5 有界轮询与 `\r` 补全、
  panic handler 引文与“钩子未注册”分支、里程碑表的适用腿与字符串（逐条 grep 到打印点；
  `kernel: step0/step1+2/step2c/step4` 的腿归属与 §6.1 描述一致）、`-d int/mmu` 与 GDB 断点说明。

## 结论

- 发现数：3
- 按严重度：P1 = 1（FACT-B-01）、P2 = 2（FACT-B-02、FACT-B-03）
- SUGGESTION：5 条（均 P3）
- 未发现 P0-fact 级错误（无“引文与源码语义相反”或“签名/字段/常量错到会误导实现”的条目）。
