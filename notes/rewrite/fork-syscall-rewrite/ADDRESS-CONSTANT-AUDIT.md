# 跨架构用户地址常量静态清扫（T13 执行件）

这份文档对 `os/` 全库做一次地址边界类常量的静态清扫，回答一个问题：**同一个「用户空间地址边界」语义，在三套硬件地址翻译规则（x86-64 四级页表、aarch64 双区域 VMSA、riscv64 Sv39）下，当前仓库里的每一处常量是否都落在合法范围内，以及是否存在「只按一种架构的心智模型写死、换一个架构就变成硬件拒绝的地址」的残留**。这个缺陷类别有一个已经定案的现实案例：VFS exec 的缺省用户栈顶 `0x7fff_ffff_f000` 在 riscv64 Sv39 下是非规范地址，CPU 在页表遍历开始之前就拒绝它，由此产生了 rc 进程反复存储页故障的整条调查（完整证据链见 `notes/rewrite/fork-syscall-rewrite/NK4C-BUG-RISCV64-TRANSIENT-PTE.md` §10.11，同类风险的第 9 章 T13 登记即本审计的执行依据；§10.12 给出了「这不是偶然笔误，而是一类系统性风险」的论证）。

清扫是纯静态的：读代码、数常量、对照判据表逐条分类。**生产代码零改动**——所有修复建议写进第 6 节，由主线程后续执行。扫描范围是 `os/` 全部 Rust 源（servers、libs、kernel、arch、boot-shim、tests、qemu-tests）；`minix3/`（C 源 ground truth，其常量属于 32 位时代，不适用 64 位判据）、`notes/`、`tmp/`（Phase E 待滚除的探针产物）不在范围内。

---

## 1. 三架构判据表（扫描的宪法）

三个架构的硬件在「地址是否可翻译」这个问题上是同构的：**虚拟地址的高位必须是对某一位的合法符号扩展（或落入某个编程指定的连续区域），否则翻译在页表遍历开始之前就被拒绝**——页表里有什么完全不影响判决。但「这一位是哪一位」「区域边界由谁决定」三架构各不相同，违规时的异常形态也不相同。判据表逐条给出锚点：规范卷册、QEMU 源码路径、本仓分页代码三类，可独立复核。

| 维度 | x86-64（四级页表） | aarch64（ARMv8-A VMSA） | riscv64（Sv39） |
| --- | --- | --- | --- |
| 规范性规则 | 位 63:47 必须全等于位 47 | 地址必须整体落在 TTBR0 低区或 TTBR1 高区，两区之间的空洞不可翻译 | 位 63:39 必须全等于位 38 |
| 用户半区上界 | 2^47（`0x0000_7fff_ffff_ffff`） | 2^(64−T0SZ)；本仓编程 T0SZ=16 ⇒ 2^48 | 2^38（`0x0000_003f_ffff_ffff`） |
| 内核半区下界 | 2^64−2^47（`0xffff_8000_0000_0000`） | 2^64−2^T1SZ；本仓 T1SZ=16 ⇒ `0xffff_0000_0000_0000` | `0xffff_ffc0_0000_0000`（高半区，位 63:39 全 1） |
| 边界由谁决定 | 页表级数（架构固定 4 级；5 级 LA57 若启用则上界变 2^56） | **可编程**：`TCR_EL1.T0SZ/T1SZ` | satp 的 MODE 字段（MODE=8 ⇒ 39 位） |
| 违规异常形态 | 真机 #GP(0)（通用保护故障，**不是**页故障）；QEMU TCG 数据通路不模拟（见下） | Translation fault（指令中止/数据中止，DFSrch 语义） | store/load/指令 page fault |
| QEMU 判决锚点 | `target/i386/monitor.c:39`（`addr_canonical`：按 `CR4.LA57` 分支取位 47/56 做符号扩展） | `target/arm/ptw.c` `get_phys_addr_lpae`：`inputsize = 64 − param.tsz`，区域外检查在 1683–1690，逐字注释「The gap between the two regions is a Translation fault」（1687） | `target/riscv/cpu_helper.c:896-898`（`get_physical_address` 第一道门 `masked_msbs`，不等 0 也不等全 1 即失败，遍历不开始） |
| 本仓分页代码锚点 | `os/arch/src/x86_64/pte.rs:10`（`PML4_ENTRIES = 512`）+ `os/arch/src/x86_64/paging.rs:41`（`PML4_SHIFT = 39`）；全仓 `rg "LA57"` 零命中 ⇒ 未启用 5 级 | `os/arch/src/arm64/paging.rs:543-560`（`enable()`：`(16 << 0)` 写 T0SZ、`(16 << 16)` 写 T1SZ，注释自陈 48 位 VA） | `os/arch/src/riscv64/tlb.rs`（`SV39_MODE: u64 = 8`，satp 编码注释明确 MODE=9/Sv48 不使用） |

三条架构各自的补充说明，以及一条跨架构的方法论注记：

**x86-64 的异常形态与 QEMU 的偏差**。真机上，对非规范地址的数据访问报 #GP(0)——这是通用保护故障类，不是页故障类；判据的规范出处是 Intel 64 and IA-32 Architectures Software Developer's Manual Vol. 3A 的 canonical addressing 章节（卷册级锚点；该手册多版本间小节编号有漂移，精确节号 [待验证]）。这一形态差异正是 §10.11 案例在 x86 上不会以同款症状出现的原因：同一个错误值在 x86 上若真被使用，报的是 #GP，与页故障驱动的调查路径完全对不上号。另有一个对调试者重要的 QEMU 行为：QEMU 的 TCG 数据通路不做规范性检查（`target/i386/tcg/translate.c` 内规范性相关检索零命中；`target/i386` 树内唯一实现了规范规则的 `monitor.c:addr_canonical` 只服务于监视命令），因此一个非规范用户地址在 QEMU 下不会以 #GP 复现，走表会按低位索引照常进行——在 QEMU 里调试 x86 目标时，「真机必炸的地址安静通过」是可能形态。

**aarch64 的边界是编出来的，不是天生的**。ARMv8-A 的 VMSA 把虚拟地址切成 TTBR0 低区与 TTBR1 高区两个连续区域，每个区域的宽度由 `TCR_EL1` 的 T0SZ/T1SZ 字段编程决定（规范出处 ARM Architecture Reference Manual DDI 0487 的 VMSA 章节，卷册级锚点）。因此「用户半区上界」对本仓而言**必须**从自己的 TCR 编程代码推导，不得引用「ARM 通常 48 位」式的经验说法。推导：`os/arch/src/arm64/paging.rs:543` 写入 `(16 << 0)`（T0SZ=16）与 550 行 `(16 << 16)`（T1SZ=16），⇒ 低区 = [0, 2^48)，高区 = [2^64−2^48, 2^64)，两区之间 (2^48, 2^64−2^48) 是空洞。仓内另一处独立陈述可交叉验证：`os/libs/minix-platform/src/arch/aarch64.rs:39` 注释「still inside the TTBR1 range (≥ 0xFFFF_0000_0000_0000)」——与本推导一致。QEMU 侧同一语义的实现是 `get_phys_addr_lpae`（`target/arm/ptw.c`，参数采集在 1628 行 `aa64_va_parameters` 调用）：区域选择按 top bit 与 tsz 完成，地址若落在两区之间，1683–1690 的 top-bits 检查直接转 Translation fault。

**riscv64 的判据与定案案例**。Sv39（satp MODE=8）的 39 位地址要做符号扩展到位 63:39；QEMU `get_physical_address` 的第一道门（`cpu_helper.c:896-898`）就是规范性检查，失败即整体翻译失败、走表不开始——§10.11 的证据链第 2 条已逐字引用过该门，判据沿用。用户半区上界 2^38、页对齐栈顶合法值 `0x3f_ffff_f000`（权威常量的 riscv64 分支）即由此而来。

**方法论注记：单列「物理地址」与「内核高半区」两类豁免**。物理地址常量（内核/模块装载基址 0x85000000 族、QEMU virt 的 DRAM 基址 0x8000_0000 族）不经过虚拟地址翻译，不受任何规范性规则约束。内核高半区虚拟地址（KDM 窗 `0xffff_ffc0_4000_0000` 族、x86 内核半区 `0xffff_8000_0000_0000` 族、aarch64 MMIO 窗 `0xffff_c000_0000_0000`）在各架构对应位模式均为规范的全 1 形态。这两类不越界，但值得警惕的是**跨架构形状**：x86 形状的内核半区地址（位 63:47 全 1 而位 46:39 非全 1，如 `0xffff_8000_0000_0000`）在 Sv39 下是非规范的（其位 63:39 不全 1）——它们今天被 `#[cfg(target_arch)]` 或逐架构权威选择所隔离，任何共享代码路径新增对它们的直接引用都必须过判据表。

---

## 2. 扫描方法（可复跑）

工具与命令全部可独立重放，命中清单以此对账（第 5 节）。

**层一 + 层二：字面量求值与常量全量枚举**（工具 `tools/address-constant-scan.py`，字节级读取——本仓 `grep` 实为 ugrep，对无效 UTF-8 文件会静默吞掉全部输出，扫描工具必须自己读字节）：

```sh
python3 tools/address-constant-scan.py os
```

四个输出通道：`LITERAL`（求值后 ≥ 2^38 的十六进制/十进制字面量）、`SHIFT`（`1 << N` 且 N∈[30,63] 的移位表达式——地址边界族偏爱移位写法）、`FAMILY`（0x7fff…/0x8000…/0x3f_ffff… 三个已知家族的全部出现，含小于 2^38 的成员，供权威一致性对账）、`CONST`（`const NAME: u64|usize|isize|u32` 定义全量枚举，不依赖搜索词）。当前读数：LITERAL 507、SHIFT 83、FAMILY 256、CONST 2295（其中 92 条按「值 ≥ 2^38 或名字含地址语义」过滤后人工分类）。

**层三：按语义找，不按数值找**（rg 于 `os/`，代表命令）：

```sh
rg -n "USER_STACK_TOP|USER_ADDRESS_SPACE_LIMIT|STACK_TOP|MMAP_TOP|MMAP_BASE|REMAP_MMAP" os/
rg -n "USER_SP|stack_high|stack_low|loader_base|stack_start|DEFAULT_STACK_LIMIT" os/servers os/libs
rg -n "canonical|规范" os/ -t rust
rg -n "0x3f_ffff|0x3fffff" os/ -t rust
```

**专项：架构前提断言**。检索测试与 `assert!` 中的架构取值前提（`rg -n "assert.*0x7fff|assert.*2\^|1 << 3[89]|1 << 4[0-8]" os/` + 人工复核），对照 §续-338b 的原型缺陷——「宿主上编译的测试断言的是另一个架构的值」。逐条结论并入发现清单与豁免清单。

**判定矩阵口径**：每个发现按「常量 × 三架构」判定（合法 / 越界但当前不可达 / 越界且可达 / 存疑），不是单架构打勾；可达性论证逐条给出。

---

## 3. 权威常量现状

清扫前先记录「已收敛到正确形态」的锚点，它们既是修复建议的目标形态，也是反查其他发现的参照系。

- **`minix_types::USER_STACK_TOP`**（`os/libs/minix-types/src/types/boot.rs:146`）：`cfg!` 分架构（riscv64 = `0x3f_ffff_f000`，其余 = `0x7fff_ffff_f000`），配两条编译期断言（`boot.rs:154-164`：页对齐 + 分架构规范性上界）。消费者三处生产路径全部引用常量而非裸值：boot-shim 两 builder（`os/boot-shim/src/opensbi_helpers.rs:482`、`os/boot-shim/src/uefi_helpers.rs:454`）与 VFS exec 缺省栈顶（`os/servers/vfs/src/main_loop.rs:1141`）。全库 62 行出现 `0x7fff_ffff_f000` 字样，逐行分类为：权威常量本体及其文档 2 行（`boot.rs:131/149`）、生产面注释 7 行（其中 5 行是 x86 心智模型表述，归 A12：`exec_worker.rs:33/459/984`、`message.rs:2326/2344`；2 行是对本缺陷的正确表述：`main_loop.rs:1138`、`trap_dispatch.rs:2113`）、生产编译面夹具行 1 行（即 A5 的 `vm/boot.rs:145`）、`#[cfg(test)]` 内 52 行（归 A9）。三个生产消费者行内都不含字面量——它们引用常量，这正是健康形态。
- **`minix_arch::DirectMapArch`**（`os/arch/src/arch/direct_map.rs:80-160`）：三架构各自的 `KERNEL_DIRECT_MAP_BASE` / `VM_DIRECT_MAP_BASE` / `VM_HEAP_BASE` / 窗口尺寸 + 每架构一条「heap 紧跟 DM 窗」编译期断言。该文件自带一则现成教材：riscv KDM 窗旧值 `0xFFFF_FC00_0000_0000` 正是一个非规范地址（其 39 位载荷解码后撞进内核镜像自己的 L2 槽），现值 `0xFFFF_FFC0_4000_0000` 才是规范高半区——「换架构不改常量」这个缺陷类别在该文件里已经发生过一次并被修掉，注释完整保留了推理。VM 服务器侧全部经 `CurrentDirectMap` 转发（`os/servers/vm/src/direct_map.rs:18-30`），无本地硬编码。
- **`USER_ADDRESS_SPACE_LIMIT`**（`os/kernel/src/ipc.rs:370-374`）：内核侧用户缓冲校验用的分架构用户半区上界，`#[cfg(target_arch)]` 三分支，消费点 `ipc.rs:419`、`ipc.rs:466`。这是「用户半区上界」语义的现行权威——但它是私有常量（无 `pub`），且 aarch64 分支有问题（发现 A4）。

三个权威并存、语义相邻但不互通：栈顶（minix-types）、DM 窗（minix-arch）、用户半区上界（kernel 私有）。第 6 节的收敛建议以此为基础。

---

## 4. 发现清单

严重度口径：**高危** = riscv64 上可达、会产生 §10.11 同型的「硬件拒绝、软件自检看不见」故障；**中危** = 当前不可达的休眠陷阱或与硬件事实不一致的常量；**低危** = 探针、注释、测试卫生。每条给出三架构判定矩阵与可达性论证。

### A1【高危】`MMAP_TOP = 2^41`：riscv64 越界，注释自陈的「48 位心智模型」

- 位置锚点：`os/servers/vm/src/mmap.rs:211-212`（`MMAP_BASE = 0x0000_0001_0000_0000`、`MMAP_TOP = 0x0000_0200_0000_0000`）；语义注释 `mmap.rs:203-209` 自陈「the address space is 48-bit canonical user space」。
- 语义：无提示地址的 mmap 分配区间上界；`find_slot(MMAP_BASE, MMAP_TOP, len)` 的扫描终点（`mmap.rs:271-273`）。消费者共三处：mmap 自身、remap 的同构拷贝（A2）、`os/servers/vm/src/map_phys.rs:88-89`（VM_MAP_PHYS 路径，引用 mmap 的常量，与 A1 同步修复即可）。
- 三架构矩阵：

  | 架构 | 判定 | 依据 |
  | --- | --- | --- |
  | x86-64 | 合法 | 2^41 < 2^47，用户半区内 |
  | aarch64 | 合法 | 2^41 < 2^48（T0SZ=16 推导） |
  | riscv64 | **越界** | 2^41 > 2^38；(2^38, 2^41) 内的地址位 63:39 不全等位 38，遍历前即拒绝 |

- 可达性论证：无提示路径走 first-fit，从 2^32 起步；要使落点越过 2^38，进程需先在 [2^32, 2^38) 内累计约 252 GiB 的映射（2^38 − 2^32 = 63 × 4 GiB；§10.12 的「约 251 GiB」即同一算术的近似）。当前系统没有任何路径会产生这种分配史，**当前不可达**。但「不可达」依赖的是分配史这一动态事实，不是结构性保证——这正是它必须改成常量级约束而不是依赖论证的原因。
- 修复建议（第 6 节 R1/R2 一并）：MMAP_TOP 改为从分架构权威导出（如 `min(USER_VA_LIMIT, USER_STACK_TOP − 栈区全额 − 安全间隙)` 或直接取栈下沿），并配每架构编译期断言「MMAP_TOP ≤ 用户半区上界」；`mmap.rs:203-209` 的「48-bit canonical」注释随值一并改写——那句注释就是 A1 的直接成因。

### A2【高危】`REMAP_MMAP_TOP`：2^41 的第二份字面量拷贝

- 位置锚点：`os/servers/vm/src/ipc/dispatcher.rs:1090-1091`（`REMAP_MMAP_BASE = 0x0000_0001_0000_0000`、`REMAP_MMAP_TOP = 0x0000_0200_0000_0000`），注释自陈「we use the same range as `handle_mmap`」。
- 语义：VM_REMAP / VM_REMAP_RO 的目标区间（`dispatcher.rs` 的 remap 实现第 6 步：无显式目标时 `find_slot(REMAP_MMAP_BASE, REMAP_MMAP_TOP, len)`）。
- 三架构矩阵：与 A1 逐格相同（x86 ✓ / aarch64 ✓ / riscv ✗）。
- 可达性论证：与 A1 同构（first-fit 从 2^32 起步，今天到不了 2^38）；remap 还有显式目标分支，见 A3。
- 修复建议：删掉本地字面量，直接引用 mmap 路径的同一常量——「same range」这个约定靠注释维持正是漂移的机制，两个值现在逐字节相同不代表下一次改动后还相同。

### A3【高危】mmap 与 remap 的提示/定址路径完全没有上界校验

- 位置锚点：`os/servers/vm/src/mmap.rs` 的 `mmap_region`——MAP_FIXED 分支 245-257 只查非零与页对齐，提示分支 265-270 只查非零、页对齐与重叠，两分支均无任何「落在用户半区内」的检查，随后 271-273 直接以 `MMAP_BASE..MMAP_TOP` 建 region；remap 的显式目标分支同构（`dispatcher.rs` remap 实现第 6 步：目标非零时以 `(target, target + len)` 为槽区间）。
- 语义：这是比 A1 更宽的一扇门——A1 只影响无提示分配的落点上限，而这里**任意页对齐地址**（包括远超 2^41、包括其他架构上非规范的地址）都会被原样接受并建成合法 region。
- 三架构矩阵：值本身不限，判定取决于传入值——riscv64 上任何 ≥ 2^38 的提示/定址（例如沿用 x86 惯例的 `0x7fff_0000_0000` 族）都会建出硬件不可翻译的区域，用户进程首次访问即 §10.11 同型故障，而 VM 的软件走表自检照常「看起来一切正常」。
- 可达性论证：**可达**。提示与 MAP_FIXED 是 POSIX mmap 的标准用法，由 guest 程序直接驱动；与 A1 不同，这里不需要 252 GiB 的分配史，一条带提示的 mmap 调用即可触发。内核侧对同类风险已经有现成的正确形态可对照——`os/kernel/src/ipc.rs:419` 在内核代读用户缓冲前用 `USER_ADDRESS_SPACE_LIMIT` 做范围校验；VM 的 mmap 入口没有对应物。
- 修复建议：在 `mmap_region` 与 remap 的提示/定址入口统一加「`addr ≥ MMAP 区间下界` 且 `addr + len ≤ 用户半区上界（分架构权威）`」校验，越界返回 `BadAddress`；与 R1 的权威常量收敛同一笔做。

### A4【中危】`USER_ADDRESS_SPACE_LIMIT` 的 aarch64 分支：值保守、注释错误

- 位置锚点：`os/kernel/src/ipc.rs:372`（aarch64 分支 `0x0000_8000_0000_0000`，注释写「TTBR1 region base」）。
- 语义：内核侧用户缓冲校验的用户半区上界。
- 三架构矩阵：x86 分支（2^47，注释 PML4[256] base）与硬件一致 ✓；riscv 分支（2^38，注释 Sv39 VPN[2]=256）与硬件一致 ✓；aarch64 分支——TCR 实编程 T0SZ=16（`os/arch/src/arm64/paging.rs:543`）⇒ 真实上界 2^48，TTBR1 区域基址是 `0xffff_0000_0000_0000` 而非 2^47。**常量值（2^47）比硬件上界（2^48）更窄**：保守方向，不会放行坏地址，只会额外拒绝 [2^47, 2^48) 里的合法用户地址。注释「TTBR1 region base」与硬件事实不符（仓内正确表述见 `os/libs/minix-platform/src/arch/aarch64.rs:39`）。
- 可达性论证：错误方向是「过度拒绝」，且当前系统没有用户地址落在 [2^47, 2^48)（权威栈顶 `0x7fff_ffff_f000` < 2^47），故无症状。但它是「判据与常量脱节」的活例：TCR 一旦重编（例如 T0SZ 改 25 走 39 位 VA），这个常量不会跟着错——因为它本来就是手写的。
- 修复建议：随 R1 一并迁入分架构权威常量（由 TCR/SATP/页表级数的编程值推导，而不是手写十六进制），aarch64 分支改为 2^48 或直接从 T0SZ 推导，注释同步改正。附带一条给权威常量自身的注记：`boot.rs:160-164` 对非 riscv 分支的断言写的是 `< 2^48`，这对 aarch64 恰好精确、对 x86 偏松（x86 真上界 2^47）——若未来把 `USER_STACK_TOP` 调到 [2^47, 2^48)，x86 真机 #GP 而断言照常通过；权威常量的断言宜按架构分别钉到 2^47 / 2^48 / 2^38。

### A5【中危】`BootParams::simple`：无测试门控的夹具构造器，内嵌 x86 栈顶

- 位置锚点：`os/servers/vm/src/boot.rs:127-146`（`pub fn simple`，文档注释自陈「Intended for unit tests... Production code must use real boot parameters」，`user_sp: VirBytes(0x7fff_ffff_f000)`）。
- 语义：VM 启动参数的「单 region、无模块」便捷构造，`user_sp` 字段是 §10.11 缺陷的同一语义位。
- 三架构矩阵：值在 x86 ✓ / aarch64 ✓ / riscv **非规范**。
- 可达性论证：**当前不可达**——全库调用方仅 `os/servers/vm/src/boot.rs` 测试模块内 8 处（`boot.rs:477-590`），生产代码零调用。但该函数没有被 `#[cfg(test)]` 门控（测试模块在 `boot.rs:464` 开始，函数在其之前），即它存在于生产编译面，唯一的防线是注释里的一句「must use real boot parameters」。VFS exec 的 `DEFAULT_USER_SP` 案例已经演示过一次「靠注释维持的约定」的下场。
- 修复建议：函数体改引 `minix_types::USER_STACK_TOP`（一行，语义还更准确：夹具想要的就是「本架构的合理栈顶」），或整体移入测试模块。前者改动更小且让夹具在 riscv 目标上自动正确。

### A6【中危】VM 内核布局的 mock 回退：生产可达、形状是 x86 的

- 位置锚点：`os/servers/vm/src/vm_server.rs:617-626`（`kernel_layout.unwrap_or_else`：handoff 未携带布局时回退到 `KernelLayout::new(0xFFFF_FFFF_8000_0000, …, 0xFFFF_8000_0000_0000, …)`，注释自陈 mock / legacy sentinel /「a real handoff is v5 and carries the arch's true KERNEL_DIRECT_MAP_BASE」）。
- 语义：VM 初始化内核映射常量的兜底路径。
- 三架构矩阵：回退值在 x86 上是规范内核半区形状 ✓；aarch64 上同形状也落在 TTBR1 高区 ✓；**riscv 上 `0xFFFF_8000_0000_0000` 非规范**（位 63:39 不全 1，判据表 §1）。
- 可达性论证：**当前不可达**——现行 boot 协议 handoff 为 v5、携带真实布局，回退分支不执行；且注释已注明 v5 承载逐架构真值。它保留的问题是「若未来出现 v3/v4 形态的 handoff 走到回退，riscv 上会拿 x86 形状的内核基址去建映射」且无断言拦截。
- 修复建议：回退分支加编译期或运行期断言（riscv 上直接 `unreachable!`/`compile_error!`，与该文件注释里讨论过的 `hardcoded_kernel_layout` 特性门方案对齐），或在 handoff 版本层拒绝不携带布局的版本——如果放任回退分支在 riscv 上执行，它会拿 x86 形状的基址建内核映射，这正是断言要在编译期或初始化期拦下的情形。

### A7【低危】`region_map.rs` 的移除探针：riscv 门控配 x86 阈值，在其目标架构上恒死

- 位置锚点：`os/servers/vm/src/region/region_map.rs:287-297`（`#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]` 门内 `if addr.0 > 0x7fff_0000_0000`）。
- 语义：§续-328 期间的「region 被移除时打 va 范围」取证探针（用后即滚类）。
- 三架构矩阵：阈值 0x7fff_0000_0000 ≈ 2^43 远超 Sv39 用户上界 2^38 ⇒ riscv64 用户地址永不满足 ⇒ **探针在其唯一的目标架构上一次都不会触发**。这是 A3/A1 的镜像教训：探针作者当时脑中的「用户高地址」是 x86 形状的。
- 修复建议：随 Phase E 探针滚除一并删除；若需在同位置重建取证，阈值应取自分架构权威（如 `USER_ADDRESS_SPACE_LIMIT` 的 VM 侧对应物）而非字面量。

### A8【低危】`cow_exec_pf.rs` 的双探针：同族阈值、无架构门

- 位置锚点：`os/servers/vm/src/cow_exec_pf.rs:155`、`os/servers/vm/src/cow_exec_pf.rs:192`（`#[cfg(not(test))]` 门内 `if vaddr.0 > 0x7fff_0000_0000`）。
- 语义：走链对账与写回读验证探针（用后即滚类）的高地址门。
- 三架构矩阵：riscv64 上同 A7 恒死；aarch64 上对 [2^43, 2^48) 的用户地址会触发（纯日志，无行为影响）；x86 上语义符合作者预期。
- 修复建议：与 A7 同批（Phase E 滚除或改权威阈值）。

### A9【中危】测试夹具的 `user_sp = 0x7fff_ffff_f000` 硬编码族

- 位置锚点与全族分解（本审计逐行核对过门控归属）。精确 token `0x7fff_ffff_f000` 共 62 行：权威本体/文档 2（§3）、生产注释 7（A12）、A5 行 1、`#[cfg(test)]` 内 52 行——后者是本条主体：`os/servers/vm/src/ipc/dispatcher.rs`（26 处 VmContext 夹具，`dispatcher.rs:1330-2385`）、`os/servers/vfs/src/exec_worker.rs`（7 处，`exec_worker.rs:1508-1676`）、`os/kernel/src/dm_coverage.rs`（8 处，`dm_coverage.rs:438-810`）、`os/kernel/src/lib.rs`（3 处）、`os/servers/vm/src/boot.rs:537/558`、`os/kernel/src/kerninfo.rs:207`、`os/kernel/src/vm_handoff.rs:817`、`os/servers/vm/src/vm_server.rs:3880`、`os/libs/minix-sys/src/stack.rs:272`、`os/libs/minix-boot/src/kernel_info.rs:366`。书写变体 `0x0000_7fff_ffff_f000` 另有 32 行：`#[cfg(test)]` 内 8（`message.rs:4065-4078`、`ipc/mib.rs:333/338`、`ipc.rs:4733`、`dm_coverage.rs:836`）、`os/kernel/tests/` 独立测试目标 1（`boot_integration.rs:28`）、qemu-tests 各架构测试内核 23（`test-paging-enable*/src/main.rs:44` 族，E5 类——它们跑在自己声明的架构上，x86 形状值对 x86/aarch64 测试内核合法）、生产注释 0。
- 语义：主体是 KernelInfo / VmContext / wire 编解码夹具里的「用户栈顶/用户地址」占位值——§10.12 所说「语义多头」的存量形态：这些值今天只在宿主测试里流动，但每个都是一枚「若流入 riscv 可达路径即复活 §10.11」的种子。其中 wire 编解码类夹具的值只承担「64 位车道装得下」的编码语义，与翻译无关，风险最低；KernelInfo/VmContext 类夹具的值则与生产 handoff 同字段同语义，风险最高。
- 三架构矩阵：值本身 x86 ✓ / aarch64 ✓ / riscv ✗；现状全部宿主编译（x86 语义）故无症状。
- 修复建议：机械替换为 `minix_types::USER_STACK_TOP` 引用（`os/kernel/src/dm_coverage.rs` 等文件已经在引 `minix_types`，替换零新增依赖）；wire 类夹具可保留字面量但在旁注一句「编码语义、非翻译语义」以免下次清扫再误报。此项不改任何生产行为，纯测试代码。

### A10【低危】`exec_worker.rs` 测试的栈下沿硬编码：§续-338b 原型的现存样本

- 位置锚点：`os/servers/vfs/src/exec_worker.rs:1508-1509`（`let stack_low = 0x7fff_ffff_f000 - exec::DEFAULT_STACK_LIMIT;`，位于 `#[test]` 内）。
- 语义：NS5-A 64 位车道 pin 测试用「权威栈顶减栈限额」构造栈 region 下沿——但栈顶那一半用的是字面量而不是 `minix_types::USER_STACK_TOP`。生产路径的同一算术两端都走常量（栈顶 = `main_loop.rs:1141` 引用的权威常量，限额 = `exec_worker.rs:462` 引用的 `exec::DEFAULT_STACK_LIMIT`），测试却把栈顶那一半抄成了字面量版本。
- 三架构矩阵：同 A9。
- 修复建议：改引权威常量后，该测试在 riscv 目标上自动表达正确前提（这正是 §续-338b P0 的教训：宿主编译的测试断言的应是「本架构的值」，而权威常量是唯一能让「本架构」随目标切换的写法）。

### A11【低危】`pte_walk.rs` 的 x86 前提断言：已自陈架构、登记在案

- 位置锚点：`os/kernel/src/pte_walk.rs:387-399`（对 `0x0000_0040_0000_1000` = 2^38 + 4 KiB 逐级断言 PML4/PDPT/PD/PT 索引，注释明写「On x86-64」）。
- 语义：软件走表的索引数学测试，输入值取在 2^38 之上正是为了测「跨过 Sv39 上界的地址在 x86 上的索引」——宿主（x86_64）上运行则断言自洽。它与 §续-338b 原型的差别在于**测试自己声明了架构前提**，且共享走表函数在各架构语义由各自分页层定义，宿主单架构运行不会误判。
- 修复建议：保留，但按「架构前提断言」登记惯例在测试名或模块注释里维持显式架构标注即可（现状已满足）；若未来把内核宿主测试矩阵扩到非 x86 宿主，此测试需换架构化参数或门控。

### A12【低危】注释层的 x86 心智模型残留

- 位置锚点与内容：
  - `os/servers/vfs/src/exec_worker.rs:33`（「minix-rs 用户态是 LP64（栈顶 `0x7fff_ffff_f000` 一族）」——把单架构取值表述成了架构无关事实）；
  - `os/servers/vfs/src/exec_worker.rs:459`、`os/servers/vfs/src/exec_worker.rs:984`（「栈顶 0x7fff_ffff_f000 一族」同款）；
  - `os/libs/minix-types/src/ipc/message.rs:2326/2344`（「the exec stack top lives at `0x7fff_ffff_f000`」——把单架构取值写进 wire 契约的文档注释，作架构无关事实陈述）；
  - `os/servers/vm/src/vm_server.rs:1017-1019`（「生产 user_sp=0x7ffffffff000」——在 riscv64 上生产的栈顶已是 `0x3f_ffff_f000`，此句作为全架构陈述已过时；其所在的下溢防护断言本身仍正确，因为两个架构值都远大于 4 MiB 限额）；
  - `os/servers/vm/src/mmap.rs:203-209`（「48-bit canonical user space」——A1 的直接成因注释，随 A1 一并改写）；
  - `os/servers/vm/src/vm_server.rs:1017`（`DEFAULT_STACK_LIMIT` 本地复制——权威在 `os/servers/vfs/src/exec.rs:36`，顺带收敛）。
- 语义：这些注释本身不改行为，但它们是「地址常量可以不分架构」这一错误直觉的载体——A1/A7/A8 的作者都读过同类注释。LP64（long 与指针 64 位的 ABI 分类）描述的是整数宽度，不承诺任何地址边界；「48 位 canonical」只对 x86-64 与本仓 TCR 编程下的 aarch64 成立。
- 修复建议：随 A1/A3 的修复顺带改写（同一批 diff，避免两次触碰同一区域），措辞模板：「用户半区上界随架构（x86-64 四级 2^47、aarch64 T0SZ=16 ⇒ 2^48、riscv64 Sv39 2^38），权威常量见 minix_types」。

### A13【低危】KDM 窗常量的 7 处本地复制（全部在取证探针内）

- 位置锚点：`os/kernel/src/syscall.rs:2588/2604/2621/3243/3303`、`os/kernel/src/trap_dispatch.rs:2000/2128`（各 `const KDM: u64 = 0xFFFF_FFC0_4000_0000;`）；同族还有探针内硬编码的 PA 区间检查 `[0x8000_0000, 0xA000_0000)`（QEMU virt DRAM）与 boot 文件表 PA `0x8500_0000`（`syscall.rs:3248` 附近）。
- 语义：riscv KDM 窗的真值权威在 `minix_arch::CurrentDirectMap::KERNEL_DIRECT_MAP_BASE`（`os/arch/src/arch/direct_map.rs:149`，附编译期断言）；探针每处自带一份本地拷贝。PA 类不受 VA 判据约束（豁免清单 E1），列出仅为完整性。
- 三架构矩阵：`0xFFFF_FFC0_4000_0000` 在 Sv39 为规范高半区 ✓；这些探针全部 riscv 门控或仅服务 riscv 取证，无跨架构消费。
- 修复建议：探针属用后即滚类，滚除即消解；若存活期延长，改引 `CurrentDirectMap::KERNEL_DIRECT_MAP_BASE`（`os/kernel` 已依赖 `minix_arch`，无新增依赖）。

---

## 5. 豁免清单

以下类别经判据表核验后**不构成发现**。豁免是逐类的，每类给理由与代表锚点；「tmp/ 探针」按任务边界整类豁免（Phase E 滚除已登记）。

**E1 · 物理地址常量**。不经过虚拟地址翻译，与三套规范性规则无关。代表：QEMU virt DRAM 基址 `0x8000_0000`（`os/boot-shim/src/opensbi_helpers.rs:61` `DRAM_BASE`、`os/kernel-image/src/bootface.rs:111`、`os/kernel/src/lib.rs:4569`）、内核物理基址 `0x8020_0000`（`os/kernel/src/lib.rs:4585` 注释）、boot 文件表 `0x8500_0000`（`os/kernel/src/syscall.rs` 探针）、模块区 `DRAM_BASE + 0x0200_0000`（`os/boot-shim/src/opensbi_helpers.rs:72`）、UEFI 内存洞 `0xfd0000_0000`（`os/boot-shim/src/uefi_helpers.rs:289` 注释）、探针 PA 窗检查 `[0x8000_0000, 0xA000_0000)`（`os/kernel/src/trap_dispatch.rs`，2020、2028、2040 行三处）。

**E2 · 内核高半区 VA（各架构规范形态）**。位模式为全 1 符号扩展，三架构均合法（判据表 §1）。代表：x86 内核半区 `0xFFFF_8000_0000_0000` 与 KDM `0xFFFF_8080_0000_0000`（`os/arch/src/arch/direct_map.rs:82/117` 权威 + 大量 x86 门控测试）、x86 镜像基座 `0xFFFF_FFFF_8000_0000`（`os/libs/minix-types/src/types/boot.rs:588` 断言、`os/boot-shim/src/opensbi_helpers.rs:608`）、riscv 镜像/KDM `0xFFFF_FFC0_0000_0000` / `0xFFFF_FFC0_4000_0000`（`os/kernel-image/src/bootface.rs:118`、`os/arch/src/arch/direct_map.rs:149`）、aarch64 MMIO 窗 `0xFFFF_C000_0000_0000`（`os/libs/minix-platform/src/arch/aarch64.rs:40`，注释自陈在 TTBR1 范围内）。逐架构归属由 `#[cfg]` 与 `CurrentDirectMap` 选择保证；§1 末尾的跨架构形状警示（x86 形状地址在 Sv39 非规范）是本类的边界条件。

**E3 · 宿主测试夹具与 wire 编解码值（全部 `#[cfg(test)]` 内）**。字节布局/编解码语义，值不进翻译。代表：`os/libs/minix-sys/src/stack.rs:272/316/359`（三种栈顶占位值证明布局与 SP 相对性）、`os/libs/minix-types/src/ipc/*` 的 round-trip 地址、`os/servers/vm/src/vm_server.rs:3415`（mmap 上界 pin 测试——A1 修复时此值须随权威常量更新，单列于此防止遗漏）、`os/arch/src/arch/boot.rs:853`（`0x7fff_ffff_ffff_0000`，非法 ELF 拒绝路径的占位 KernelInfo）、哨兵值族（`0xDEAD_BEEF_CAFE_BABE`、`0x5A5A…`、boot magic `0x3154_4f4f_4258_4e4d`、AP 启动 magic ASCII 值）。x86 门控取证探针的栈地址（`os/kernel/src/trap_dispatch.rs:314-329` `nk4a_pte_watch`，`#[cfg(target_arch = "x86_64")]` 门内）按「用后即滚」政策随 Phase E 处理。

**E4 · 非地址语义的数值巧合**。值落在扫描区间但语义完全无关。代表：文件位置上限 `MAX_FILE_POS = 0x7FFF_FFFF`（`os/servers/vfs/src/fcntl.rs:92`，C `minix/const.h:124` 同值）、块页数哨兵 `BLK_PAGES_UNBOUNDED`（`os/servers/vfs/src/misc.rs:344`）、定时器哨兵 `TMR_NEVER = 0x80000000`（`os/kernel/src/clock.rs:2262`）、进程旗标族 `0x8000/0x80000`（`RTS_NO_QUANTUM`、`EVENT_CALL` 等）、信号量序号掩码 `0x7fff`（`os/servers/ipc-server/src/sem/table.rs:324`）、ioctl 方向位 `IOC_IN = 0x8000_0000`、`MAP_THIRDPARTY = 0x800000`、DES 分组掩码 `(1 << 48) - 1`（`os/libs/minix-crypt/src/des.rs:31`）、GDT 描述符（`os/arch/src/x86_64/ap_early_entry.rs:73-76`）、随机数乘子（`0x9E37_79B9_7F4A_7C15` 族）、UTIME/NSEC 哨兵 `(1 << 30) - 1`、大页尺寸 `1 << 30 / 1 << 21`、PTE 旗标位（NX `1 << 63` 等）与 PTE 地址掩码（`ADDR_MASK` 族——它们是从 PTE 里**提取**物理地址的字段掩码，不是虚拟地址）、`BumpDma` 的 `aligned | 0xffff_8000_0000_0000`（`os/libs/minix-types/src/types/dma.rs:104`，`#[cfg(test)]` 内的假 DMA 助手，值纯字节语义）。

**E5 · qemu-tests 测试内核的常量**。`os/qemu-tests/test-kernels/` 下的独立小内核使用 DRAM/PA 常量（E1 同理）与各自的哨兵/旗标；其用户态探针内核（如 `test-user-trap`）的地址常量随各自测试语义自洽。判据表对其一视同仁地核验过（LITERAL 通道约 90 处命中全部落在 E1/E3/E4 类），无一越界。

---

## 6. 覆盖率声明（通道 × 处置对账）

| 通道 | 命中 | 处置分布 |
| --- | --- | --- |
| LITERAL（≥ 2^38 字面量） | 507 | 生产发现命中 18 条：A1（`mmap.rs:212` 的 MMAP_TOP，1 条——MMAP_BASE 为 2^32 低于本通道阈值，经 CONST 通道按名字捕获）+ A2（`dispatcher.rs:1091`，1 条）+ A4（`ipc.rs:370/372/374`，3 条）+ A5（`vm/boot.rs:145`，1 条）+ A6（`vm_server.rs:620/624`，2 条）+ A7/A8（`region_map.rs:290` + `cow_exec_pf.rs:155/192`，3 条）+ A13（KDM 常量 7 条）；其余 489 条为测试夹具/内核高半区/PA/魔法数（A9 族 + E1–E5）。A3 无字面量命中（它是校验缺失，经层三语义扫描发现） |
| SHIFT（1 << N, N∈[30,63]） | 83 | 地址相关的仅权威断言 2 条（`boot.rs:157/162`，合法）与 x86 PML4 区域算术 1 条（`os/arch/src/x86_64/paging.rs:1393`，合法）+ 探针 2 条（`trap_dispatch.rs:1973-1977`，E3）；其余 78 条为大页尺寸/旗标位/UTIME 哨兵（E4） |
| FAMILY（0x7fff/0x8000/0x3f_ffff 族，值 < 2^38 的成员） | 256 | PA `0x8000_0000` 族（DRAM/物理基址，E1）、小型 x86 测试栈值 `0x7fff_0000` 族（E3）、旗标/掩码 `0x8000/0x800000/0x8000_0000` 族（E4）、`0x3f_ffff` 文档行（权威常量的 riscv 值，合法）合计 256 行；权威常量本体的 62 行在 LITERAL 通道，已入 §3 与 A9 口径 |
| CONST（const 全量枚举） | 2295（过滤后 92） | 92 条逐条人工分类：地址语义入发现清单与权威清单（§3）；其余为旗标/尺寸/掩码/容量语义（E4），无静默 |
| 专项（架构前提断言） | 4 组 | A10、A11、权威断言注记（A4 附带）、`stack.rs` LP64 预算 pin（E3，布局语义） |

对账恒等式：507 + 83 + 256 = 846 行原始命中，每行归入「发现清单 / 权威清单 / 豁免清单」三类之一，无第四类；发现清单 13 项（A1–A13）覆盖全部非豁免命中。判据表 3 架构 × 6 维度全部带锚点，无「ARM 通常 48 位」式经验表述；标注 [待验证] 的仅 Intel SDM 精确节号一处（卷册级锚点已给）。

---

## 7. 给主线程的修复排序（只建议，不动手）

1. **R1（高危，含 A1/A2/A3/A4）**：在 `minix_types` 落地分架构用户半区权威常量（建议名 `USER_VA_LIMIT`：x86 = 2^47、aarch64 = 2^48、riscv = 2^38），从各架构分页编程值推导（或与推导处互设编译期断言），`USER_STACK_TOP`、`USER_ADDRESS_SPACE_LIMIT`（从 `ipc.rs` 迁出转 pub）、`MMAP_TOP` 全部改为从它导出；`MMAP_TOP` 的导出式需同时满足「≤ USER_VA_LIMIT」与「与栈区无交叠」两条编译期断言，`REMAP_MMAP_*` 删本地字面量改引 mmap 权威。具体导出式（固定区间还是栈下沿减间隙）由主线程定夺——判据只锁上界，不锁选值策略。
2. **R2（高危，随 R1 同批）**：`mmap_region` 与 remap 的提示/MAP_FIXED 入口加「`addr + len ≤ USER_VA_LIMIT`」校验，越界返 `BadAddress`。这是 13 项里唯一「一条 guest 调用即可触发」的口，建议与 R1 同笔落地、同笔上 riscv 门验证（构造 ≥ 2^38 的 mmap 提示，期望 EPERM/BadAddress 而非静默建 region）。
3. **R3（中危）**：A5（`BootParams::simple` 改引权威栈顶）、A6（mock 布局回退加 riscv 拒绝断言）、A9/A10（夹具与测试改引权威常量，机械替换）。
4. **R4（低危，可搭探针滚除车）**：A7/A8（死探针与同族阈值随 Phase E 滚除）、A13（存活探针改引 `CurrentDirectMap`）、A12（注释层改写随 R1 同 diff）。
5. **R5（登记项）**：A11 维持现状并保持架构显式标注惯例；权威常量断言按架构分钉（A4 附带注记）。

修复后复跑 `python3 tools/address-constant-scan.py os`，期望：LITERAL 通道中 `MMAP_TOP`/`REMAP_MMAP_TOP` 的 2^41 条目消失、A5/A9 族的 `0x7fff_ffff_f000` 条目收敛到权威常量本体与 wire 语义注释；本档第 6 节的通道计数随之更新。

## §8 修复轮落地注记（NK4C 续-344，R1+R2 同笔）

按 §7 建议落地 R1+R2（一笔）；修复后复扫与本节由修复轮补记：

- **R1 落地**：`minix_types::USER_VA_LIMIT` 上线（x86_64=2^47 / aarch64=2^48 / riscv64=2^38），per-arch 编译期断言分钉（R5 附带项一并：x86 收紧到 2^47，替换原「非 riscv 统一 <2^48」松口径）。`USER_STACK_TOP` 增 `< USER_VA_LIMIT` 上界断言。aarch64 侧在 `os/arch/src/arm64/paging.rs` 提取 `TCR_T0SZ=16` 常量、TCR 编程值与 `USER_VA_LIMIT` 互为编译期交叉断言（任一侧漂移即编译失败）。
- **A4 修复**：`os/kernel/src/ipc.rs` 私有三分支 `USER_ADDRESS_SPACE_LIMIT` 删除，改 `use minix_types::USER_VA_LIMIT as USER_ADDRESS_SPACE_LIMIT`——aarch64 从 2^47（保守窄）归位 TCR 推导的 2^48，错误注释（「TTBR1 region base」）一并更正。kernel 侧用户拷贝检查的接受面随之加宽到 TCR 事实边界（该区间为 canonical 用户半区，无映射 VA 的行为不变=走表失败 EFAULT）。
- **A1/A2 修复**：`MMAP_TOP` 改派生式（`minix_types::USER_VA_LIMIT < 2^41` 时取 USER_VA_LIMIT，否则维持 2^41 宽松窗）——riscv 窗口顶收到 2^38，越界窗结构性消除；与栈区搜索窗重叠由 find_slot 的 region overlap 检查兜底（窗口是上界安全的超集，语义不变）。`REMAP_MMAP_*` 第二份字面量删除，`use` 引 `crate::mmap::{MMAP_BASE, MMAP_TOP}`。
- **A3 修复（R2）**：`mmap.rs` 新增 `check_user_range(limit, addr, len)`（checked_add + 双侧上界，形态同 kernel `user_copy_range_mapped` 第 1 步），MAP_FIXED 与 hint 两分支接入；`dispatcher.rs` remap 显式 target 分支同形校验（越界 `VmError::InvalidAddress`）。语义取舍：越界 hint **拒绝而非静默回退窗口**（Linux 非 FIXED hint 会回退；此处按审计 fail-closed 取舍，越界提示视为调用方 bug）。
- **复扫**（`python3 tools/address-constant-scan.py os`）：2^41 独立字面量清零——唯一残留 `mmap.rs:217` 为派生比较点（封顶语义本体，非权威副本）；`REMAP_MMAP_*` 字面量清零；`USER_ADDRESS_SPACE_LIMIT` 三分支清零（ipc.rs 归 `use`）。§6 的四通道计数系交付时点快照，本笔后小幅漂移（净减 5 处），不回改 §6 原始记录。
- **验证**：host 四包 + vm 536/0（新增 `test_check_user_range_both_arch_families`：x86/riscv 双族值注入覆盖，含 2^41-hint 在 riscv 族被拒/在 x86 族合法的对照断言）+ vfs 538/0 + boot-shim test-all 27/0 + check-layout all PASS + x86 cmd-smoke PASS + aarch64 bootmarks PASS（rc=0）+ riscv boot-full PASS（marker reached）。A5/A6/A9/A10（R3）与 A7/A8/A13（R4）未动，按 §7 排序留给后续笔。
