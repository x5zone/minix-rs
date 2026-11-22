# R2 反证报告（P-REFUTE）

范围：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`，重点为 R1 修改段（InvalidSegment 计数、Step 2 守卫、L3/L6 子断言、三形态、栈回收假设、§4.0 建根、§5.4 边界）与测试计数。只读仓库；未读作者结论、其他角色报告与历史 panel。

取证方式：逐条给出实际命令与输出摘录，命令均在 `/home/xzhao/github/minix-rs` 下执行。

---

## CLAIM 01

文档 §3.2 断言：`ElfError` 共 **12 个变体**，按产生层分四组（文件头五道、程序头表两道、迭代器段形两道、loader 三道）。

证据：

```bash
$ grep -n 'pub enum ElfError' -A 40 os/libs/minix-elf/src/lib.rs
153:pub enum ElfError {
... TooShort, BadMagic, Not64Bit, NotLittleEndian, NotExecutable,
... PhdrOutOfBounds, PhdrEntryTooSmall, NoLoadSegments, InvalidSegment,
... SegmentOutOfBounds, FileszExceedsMemsz, TargetBufferTooSmall { .. }
```

逐项清点恰为 12 个变体；五/两/两/三的分组与源码顺序一致。

## VERDICT

CONFIRM。变体数与分组描述均与源码相符。

---

## CLAIM 02

文档 §3.2 断言：`InvalidSegment` 的返回点“不止三个”，除 `compute_kernel_layout` 的 `paddr + memsz` 溢出、`kern_end_phys - kern_phys_base` 下溢、段间物理重叠三处之外，`segment_spans` 还拒绝 `paddr + filesz`、`mem_end < file_end` 与两处 `usize::try_from`，`load_segments_into_buffer` 路径另有边界检查，因此“不能把三个读成穷举”。

证据：

```bash
$ grep -rn 'InvalidSegment' os/boot-shim/src/loader.rs
198: 段间重叠 -> InvalidSegment
271: paddr.checked_add(memsz).ok_or(InvalidSegment)
284: kern_end_phys.checked_sub(kern_phys_base).ok_or(InvalidSegment)
363/365/367: load_segments_into_buffer 的 dst_off/file_end/mem_end try_from
414/418: segment_spans 的 paddr+filesz、paddr+memsz
420: mem_end < file_end -> InvalidSegment
422/423/426: src_start/src_len try_from 与 src_end checked_add
```

结论“不止三处、不能当穷举”成立。附带一处不精确：`segment_spans` 在“两处 `usize::try_from`”之外还有 `src_end` 的 `checked_add`，且缓冲区路径另有 3 个 `usize::try_from` 与 `checked_sub`；文档自己的清单同样不是穷举，但它的主张只要求否定“三个即穷举”，因此不影响主张成立。

## VERDICT

CONFIRM（主张成立；清单本身不完整，属表述级瑕疵，非事实反转）。

---

## CLAIM 03

文档 §5.2 断言：L3 的子断言为 “L3a = 首段 VMA/LMA 等于该架构预期基址；L3b = 按脚本基址常量算出的预期首段 VMA 与实测首段 VMA 相等；L3c = 首段 LMA 等于预期装载基址”；L6 的子断言为 “L6a = 入口等于首段 VMA；L6b = `_start` 符号等于入口且落在首段内”。

证据：

```bash
$ sed -n '200,270p' os/kernel-image/check-layout.sh
check "L3a" "PT_LOAD 段数 >= 1（实得 $nload）" ...
check "L3b" "首段 VMA = 0x$want_vma ..." ...
check "L3c" "首段 LMA = 0x$phys_base ..." ...
check "L6a" "入口 = 0x$entry_want ..." ...
check "L6b" "符号 _start = 入口（实得 ${got_start:-无}）" ...
check "L6c" "入口 ... 落在首段 [...] 内" ...
```

实际 L3a 是“PT_LOAD 段数 >= 1”，不是“首段 VMA/LMA 等于预期基址”；L3b/L3c 才是 VMA/LMA 比较。实际 L6b 只比较 `_start` 符号与入口，不检查“落在首段内”；“落在首段内”是独立的 L6c，文档完全未列。该段正是 R1 修改段点名的“L3/L6 子断言”，属可复核的事实错误。

## VERDICT

REFUTE。L3a 的定义错误；L6b 被并入了不存在的“落在首段内”语义，且漏掉 L6c。

---

## CLAIM 04

文档 §4.3 断言：Step 2 有守卫 `if kern_virt != kern_phys`；两基址相等时跳过 Step 2，因为 Step 1 的窗口已覆盖内核 LMA 段。若删掉守卫照做 Step 2，后果不是静默覆盖，而是 `map_huge` 命中已有叶返回 `AlreadyMapped`，被 Step 2 的 `.expect("kernel map: map_huge failed — kernel image mismatch")` 变成建表点当场 panic。

证据：

```bash
$ sed -n '1388,1414p' os/kernel/src/lib.rs
// Step 2: Kernel high-address mapping.
if kern_virt != kern_phys {
    ...
    paging.map_huge(...).expect("kernel map: map_huge failed — kernel image mismatch");
}
$ sed -n '1262,1278p' os/kernel/src/lib.rs
windows.push(kern_phys_base, kern_phys_base + kern_size);   // kernel LMA window
$ sed -n '1325,1332p' os/arch/src/x86_64/paging.rs
let e2 = read_pte_dm(pd + i2 * 8, ...);
if e2 & PRESENT != 0 { return Err(PageTableError::AlreadyMapped); }
```

Step 1 的公共窗口确实包含 `[kern_phys_base, +kern_size)`；两基址相等时 Step 2 的首个 `map_huge` 必命中 Step 1 已装的 2 MiB 叶（若选 1 GiB 档则命中非叶的 present 条目），两条路径都返回 `AlreadyMapped`，随后 `.expect` panic。

## VERDICT

CONFIRM。守卫存在，删守卫的失败模式为 fail-closed panic，不是属性漂移。

---

## CLAIM 05

文档 §4.3 断言：Step 1 的重叠窗口在第二次安装同一 2 MiB 片时返回 `AlreadyMapped`，调用方按“已是同一片”吸收；源码 `os/kernel/src/lib.rs` 的 Step 1 注释仍写“同尺寸重写返回 `Ok`；`AlreadyMapped` 只在 1 GiB 叶时出现”，与三份 `map_huge` 实现不符。

证据：

```bash
$ sed -n '1311,1332p' os/arch/src/x86_64/paging.rs
if e3 PRESENT && e3 PS { return Err(AlreadyMapped); }   // 1 GiB leaf blocks finer
...
if e2 PRESENT != 0 { return Err(AlreadyMapped); }       // existing 2 MiB leaf
$ grep -n 'AlreadyMapped' os/arch/src/arm64/paging.rs | head
1221/1235/1248: ... return Err(PageTableError::AlreadyMapped);
$ grep -n 'AlreadyMapped' os/arch/src/riscv64/paging.rs | head
1228/1244/1257: ... return Err(PageTableError::AlreadyMapped);
$ sed -n '1325,1335p' os/kernel/src/lib.rs
// ... a second `map_huge` overwrites the same leaf with identical flags.
// `map_huge` returns `AlreadyMapped` only when a 1 GiB leaf already occupies the slot
```

三架构实现均在“目标叶已 present”时返回 `AlreadyMapped`；源码注释确实仍按旧行为描述，与文档指认一致。文档“以实现为准”的裁决符合真值优先序。

## VERDICT

CONFIRM。实现语义与文档一致；文档对源码注释过时的指认属实。

---

## CLAIM 06

文档 §3.1/§6.2 断言：三份生产链接脚本各 4 条 `: AT(`；若把模式放宽成 `AT(`，x86_64 仍 4 条、aarch64 与 riscv64 各 5 条（多出的一条来自头注释）。

证据：

```bash
$ grep -c ': AT(' os/kernel-image/x86_64.ld os/kernel-image/aarch64.ld os/kernel-image/riscv64.ld
os/kernel-image/x86_64.ld:4
os/kernel-image/riscv64.ld:4
os/kernel-image/aarch64.ld:4
$ grep -c 'AT(' os/kernel-image/x86_64.ld os/kernel-image/aarch64.ld os/kernel-image/riscv64.ld
aarch64:5  x86_64:4  riscv64:5
$ grep -n 'AT(' os/kernel-image/aarch64.ld
14: * ... AT() 表达式按此写 ...      <- 头注释
45/57/65/73: 四条段头 AT(...)
```

计数与差额来源完全对上。

## VERDICT

CONFIRM。4/4/4 与 4/5/5 的读数及“多出的一条来自头注释”均属实。

---

## CLAIM 07

文档 §3.1 断言：载体单地址脚本的实例 `test-higher-half-riscv64/link.ld` 只有一套地址（`. = 0x80200000`），段没有 `AT()`，收尾一行 `__stack_top = . + 0x10000`。

证据：

```bash
$ cat os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half-riscv64/link.ld
SECTIONS {
    . = 0x80200000;
    .text : { ... }
    .rodata : { ... }
    .data : { ... }
    .bss : { ... }
    __stack_top = . + 0x10000;
}
```

脚本中无 `AT(`。

## VERDICT

CONFIRM。三种链接形态的第二种实例与文档描述逐项相符。

---

## CLAIM 08

文档 §5.1 断言：`os/libs/minix-elf` 有 27 个测试，复算命令为 `grep -c '    fn test_' os/libs/minix-elf/src/lib.rs`，输出 27。

证据：

```bash
$ grep -c '    fn test_' os/libs/minix-elf/src/lib.rs
27
```

逐条列出亦为 27 个 `fn test_`（文件头五道 6 个、迭代器 12 个、段字段 5 个、装载仿真 4 个）。

## VERDICT

CONFIRM。计数与复算命令一致。

---

## CLAIM 09

文档 §5.1 断言：`os/boot-shim/src/loader.rs` 有 32 个测试，复算命令为 `grep -c '#\[test\]' os/boot-shim/src/loader.rs`，输出 32，且这 32 个属于 `loader::tests`。

证据：

```bash
$ grep -c '#\[test\]' os/boot-shim/src/loader.rs
32
$ grep -c '    fn test_' os/boot-shim/src/loader.rs
31
$ awk '/#\[test\]/{getline; print NR": "$0}' os/boot-shim/src/loader.rs
...
1059:     fn source_destination_overlap_stops_before_any_segment_write() {
```

32 个 `#[test]` 中有 1 个函数名不带 `test_` 前缀（`source_destination_overlap_stops_before_any_segment_write`），所以 `fn test_` 计数为 31。文档给出的复算命令与读数无误；若读者改用 `fn test_` 计数会得到 31，这是命名不一致而非文档虚报。

## VERDICT

CONFIRM（针对文档使用的 `#[test]` 口径；附带说明另一口径为 31）。

---

## CLAIM 10

文档 §5.1 断言：`cd os && cargo test -p boot-shim --lib` 在默认特性下共 136 个测试，其中 32 个属于 `loader::tests`。

证据（本次只做静态清点，未运行 cargo）：

```bash
$ grep -rc '#\[test\]' os/boot-shim/src/*.rs
dtb_copy.rs:15  lib.rs:1  loader.rs:32  resv_claims.rs:14
opensbi_helpers.rs:11  uefi_helpers.rs:12  ebs_pair.rs:17  ebs_map.rs:45
```

源文件 `#[test]` 静态合计 147，不是 136；136 只可能来自默认特性下裁掉部分模块后的实际编译计数。本次为避免写仓库未执行 `cargo test`，因此无法确认 136 这个读数。

## VERDICT

UNCLEAR。静态合计 147 与 136 不符；差值可由特性门控解释，但缺实际 `cargo test -p boot-shim --lib` 输出，不能判 CONFIRM，也不能仅凭静态数判 REFUTE。

---

## CLAIM 11

文档 §2.5 断言：C 侧 `kernel/main.c` 的 `kmain` 在 `system_init()` 之后调用 `add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len)`，源码注释写明 bootstrap 阶段结束、其物理内存可交回空闲链；因此若栈落在 `.unpaged_bss`，栈页会进空闲链。

证据：

```bash
$ sed -n '270,305p' minix3/minix/kernel/main.c
  memory_init();
  system_init();
  ...
  /* The bootstrap phase is over, so we can add the physical
   * memory used for it to the free list.
   */
  add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len);
```

调用次序与注释均与文档一致。

## VERDICT

CONFIRM。栈回收假设的 C 侧调用点、次序与注释原文均属实。

---

## CLAIM 12

文档 §4.1 断言：`build.rs` 按 `--target` 选择链接脚本：`x86_64-unknown-none → x86_64.ld`、`aarch64-unknown-none → aarch64.ld`、`riscv64gc-unknown-none-elf → riscv64.ld`，宿主目标不追加链接参数。

证据：

```bash
$ grep -n 'x86_64.unknown.none\|aarch64.unknown.none\|riscv64gc' os/kernel-image/build.rs
20:        "x86_64-unknown-none" => Some("x86_64.ld"),
21:        "aarch64-unknown-none" => Some("aarch64.ld"),
22:        "riscv64gc-unknown-none-elf" => Some("riscv64.ld"),
23:        _ => None,
```

映射与 `None` 分支均属实。

## VERDICT

CONFIRM。

---

## CLAIM 13

文档 §5.4 断言：“arch `link.ld` 形态没有生产读者，常量锁步对象不存在。”

证据：

```bash
$ find os/kernel/src/arch -name 'link.ld'
（无输出）
$ grep -rn 'link_ld_lockstep' os --include='*.rs' --include='*.toml' --include='*.sh'
（无源码命中）
```

源码树内既无 `os/kernel/src/arch/*/link.ld`，也无锁步测试/对象。仅 `os/target/debug/incremental/link_ld_lockstep-*` 构建残留在 target 下，属历史编译产物，不是树内源码。

## VERDICT

CONFIRM。两项“不存在”均成立（target 残留不算树内对象）。

---

## CLAIM 14

文档 §5.3 断言：`kmain_verify` 的 `sp_ok` 要求 SP 落在高半，`sp_aligned` 在 x86_64 检查 `sp & 0xF == 8`、aarch64/riscv64 检查 `sp & 0xF == 0`，`fp_zero` 要求 FP 为零；PC 只对 riscv64 并入 PASS，其余架构只打印 `INFO`，riscv64 低 PC 报 `FAIL: PC not at high address (riscv64 delta path broken)`。

证据：

```bash
$ sed -n '2286,2345p' os/kernel/src/lib.rs
let sp_ok = sp >= kern_high;
#[cfg(target_arch = "x86_64")] let sp_aligned = sp & 0xF == 8;
#[cfg(any(aarch64, riscv64))] let sp_aligned = sp & 0xF == 0;
let fp_zero = fp == 0;
#[cfg(target_arch = "riscv64")] let pc_ok = pc_at_high;
#[cfg(not(target_arch = "riscv64"))] let pc_ok = true;
...
"  FAIL: PC not at high address (riscv64 delta path broken)\n"
"  INFO: PC not at high address (expected in test kernel)\n"
```

判定条件 `if sp_ok && sp_aligned && fp_zero && pc_ok` 与文档描述一致。

## VERDICT

CONFIRM。三条寄存器断言与按架构分开的 PC 判据均属实。

---

## CLAIM 15

文档 §4.4 断言：riscv64 resume leg 若误加 `delta`，故障读数可复算为 `0xffffffc00000477a + 0xffffffbf7fe00000 = 0xffffff7f7fe0477a`；§6.2 又断言测试载体断点为 `0x80200144 + delta = 0xffffffc000200144`（`delta = 0xffffffbf80000000`）。

证据：

```bash
$ printf '0x%x\n' $((0xffffffc00000477a + 0xffffffbf7fe00000)) $((0x80200144 + 0xffffffbf80000000))
0xffffff7f7fe0477a
0xffffffc000200144
```

两式与文档给出的数值逐位相符；文档同时指出源码注释里的加数 `0xffffffbf7ffe0000` 有一位十六进制错位，正确的 `delta` 是 `0xffffffbf7fe00000`，复算支持这一更正。

## VERDICT

CONFIRM。算术、故障地址与断点地址均可复现。

---

## 汇总

- CONFIRM：12 条（CLAIM 01、02、04、05、06、07、08、09、11、12、13、14、15，其中 02 带“清单不完整”备注）
- REFUTE：1 条（CLAIM 03，L3/L6 子断言的定义与脚本不符）
- UNCLEAR：1 条（CLAIM 10，boot-shim `--lib` 136 的总数缺实际运行证据）
- 最重发现：CLAIM 03 属文档-代码同步错误，位于 R1 修改段点名的“L3/L6 子断言”，应按 P0-fact 处理；CLAIM 10 应降级为待复核读数，不宜作为已证事实引用。
