# 反证报告：`02-higher-half-kernel.md`

> 只读审查。读取范围限定为目标文档、`related-code.txt` 列出的 Rust 源码、`.design/02-design.v4.md` 与 `.design/02-outline.v5.md`；未读作者结论性解释、历史评审或其他面板角色输出。`os/arch/src/x86_64/paging.rs` 与 `os/arch/src/riscv64/paging.rs` 不在允许读取清单内，涉及 `map_huge` 实现体的最终裁决在文中标明为待定。

## 结论摘要

| 编号 | 主张主题 | 结果 |
|---|---|---|
| 01 | J1“所有 `PT_LOAD` 平移一致” | 阻断 P1（跨节冲突） |
| 02 | `kern_stack_top = kern_virt_base + kern_size` 是镜像尾 | 反证失败（生产公式成立；测试载体有 P2 边界） |
| 03 | Step 1 重叠 2 MiB 窗口的 `AlreadyMapped` 是常态返回 | 阻断 P1（与允许源码注释直接矛盾；行为上可降 P2） |
| 04 | Step 2 的 1 GiB 叶会覆盖 Step 1 的 2 MiB 叶/下级表指针 | 阻断 P1（失败路径描述错误） |
| 05 | 直核腿 Step 1 窗口清单完整 | 阻断 P1（失败路径省略） |
| 06 | L9 的 x86_64 分支读“对应 direct face”的 `bootface.rs` | 非阻断 P2（测试覆盖描述不精确） |

---

## CLAIM 01

**主张**：J1 是一条全表不变量：“所有 `PT_LOAD` 的 `vaddr − paddr` 相等”（`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:27`；设计合同 `.design/02-design.v4.md:21` 同样写作“所有 `PT_LOAD`”）。§5.2 的 L4 也写作“所有 `PT_LOAD` 的 `(vaddr - paddr)` 是同一个平移量”（同文件 `:1675`）。

**反证尝试**：文档 §2.1 在解码 C 的 `kernel.lds` 时给出了相反的事实：低侧 `.unpaged_text`、`.unpaged_data`、`.unpaged_bss` 不带 `AT()`，平移量为 0；带 `AT(ADDR(sec) - _kern_offset)` 的 `.usermapped_glo`、`.usermapped`、`.text`、`.data`、`.bss` 平移量为 `_kern_offset = 0xF0000000`。原文是：“低侧 `.unpaged_*` 三段不带 `AT()`、平移量为 0——两组各自一致，不是全表一致”（`:99`）。`.unpaged_text`/`.unpaged_data` 是可执行/可写分配段，`.unpaged_bss` 是 NOBITS 分配段，它们都会进 `PT_LOAD`；因此 C 镜像里至少有两个不同的平移量，“所有 `PT_LOAD`”不成立。Rust 生产镜像的四个 `LOAD` 行确实各自同平移量（下条复算），但这只覆盖 `AT()` 组，不能反证 C 侧的低侧组。

**结果**：阻断 P1（跨节复述冲突）。J1 要么把射程写成“每个映射组内部一致；C 侧低侧组平移量为 0，高侧 `AT()` 组为 `_kern_offset`”，要么明确写成“Rust 生产镜像里所有带 `AT()` 的 `PT_LOAD`”。当前 §1.2 与设计合同的“所有”是过宽的。

**证据**：
- 文档 `02-higher-half-kernel.md:27`：“J1 | 平移一致：所有 `PT_LOAD` 的 `vaddr − paddr` 相等”。
- 文档 `:99`：“低侧 `.unpaged_*` 三段不带 `AT()`、平移量为 0——两组各自一致，不是全表一致。”
- 设计合同 `.design/02-design.v4.md:21`：“J1 | 所有 `PT_LOAD` 的 `vaddr−paddr` 相等”。
- Rust 侧复算（只证明 `AT()` 组，不证明 C 侧）：

```text
$ readelf -lW os/target/x86_64-unknown-none/release/kernel | sed -n '/LOAD/p'
  LOAD 0x001000 0xffff800000200000 0x0000000000200000 ...
  LOAD 0x048000 0xffff800000247000 0x0000000000247000 ...
  LOAD 0x154000 0xffff800000353000 0x0000000000353000 ...
  LOAD 0x1fc000 0xffff8000003fb000 0x00000000003fb000 ...
# 四行 VMA−LMA 均为 0xffff800000000000
```

aarch64 与 riscv64 的四行分别同为 `0xffff7fffbfe00000` 与 `0xffffffbf7fe00000`。这组输出与 §6.2 的读数一致。

**若我错了**：如果 J1 的射程本来就是“Rust 生产镜像里带 `AT()` 的 `PT_LOAD`”，那么应把 §1.2 与设计合同的“所有 `PT_LOAD`”改成带限定语的形式；否则 C 的 `.unpaged_*` 就是反例。把“J1 是历史 C 也满足的全表不变量”当成前提，会把 §2.1 的“两组各自一致”读成补充说明，实际它是反例。

---

## CLAIM 02

**主张**：`kern_stack_top = kern_virt_base + kern_size` 是镜像尾的虚拟地址，16 字节对齐；`kern_size` 由 `compute_kernel_layout` 的 `kern_end_phys - kern_phys_base` 得出，其中 `kern_end_phys` 是所有段 `paddr + memsz` 的最大值；当前三架构生产镜像跨距 4 MiB；2 MiB 收口同时保证 `kern_stack_top` 对齐（文档 §3.3 `:713-748`、§3.5 `:843-887`）。

**反证尝试**：
- `KernelInfo::kern_stack_top()` 确实是 `kern_virt_base.0.checked_add(self.kern_size)`，溢出 panic（`os/libs/minix-boot/src/kernel_info.rs:197-204`）。
- `compute_kernel_layout` 确实用 `kern_end_phys.max(seg_end)` 与 `kern_phys_base.min(seg.paddr)` 求 `kern_size = kern_end_phys - kern_phys_base`（`os/boot-shim/src/loader.rs:249-296`）。
- `boot_validate_and_prepare` 有一条 `kern_stack_top.0.is_multiple_of(16)` 断言，失败消息是 `arch_boot: kern_stack_top must be 16-byte aligned`（`os/kernel/src/lib.rs:911-933`）。
- 用三份已构建产物复算，`kern_virt_base + kern_size` 与末段 `VirtAddr + MemSiz` 逐架构相等：

| 架构 | 首段 VMA | `kern_size` | `kern_virt_base + kern_size` | 末段 VMA + MemSiz |
|---|---|---|---|---|
| x86_64 | `0xffff800000200000` | `0x400000` | `0xffff800000600000` | `0xffff8000003fb000 + 0x205000 = 0xffff800000600000` |
| aarch64 | `0xffff800000000000` | `0x400000` | `0xffff800000400000` | `0xffff800000179000 + 0x287000 = 0xffff800000400000` |
| riscv64 | `0xffffffc000000000` | `0x400000` | `0xffffffc000400000` | `0xffffffc00015e000 + 0x2a2000 = 0xffffffc000400000` |

没有找到生产镜像上的反例。唯一能找到的边界在测试载体：`os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs:33-62` 把 `kern_size` 写成 `image_end - image_base`，而 `image_end` 是把 UEFI `LoadedImage.info()` 给出的 `image_hi` 向上取整到 2 MiB。这个值不是 ELF 的“末段 `paddr + memsz` 端点”，所以测试载体没有验证 §3.3 的前提一（“镜像的最后一个可加载段覆盖到镜像尾”）。它只验证跳转后的 SP/PC/FP，不构成对生产公式的反例。

**结果**：反证失败（生产公式成立）。附 P2：测试载体的 `kern_size` 是取整窗口，不是 ELF 跨度；文档 §5.3 说“也就是 §3.3 的算式在测试形态下的取值”时，应补一句“输入是取整窗口，不是生产侧的 ELF 跨度”，否则读者会把载体的 PASS 当成前提一也被验证。

**证据**：
- 文档 §3.3 `:729-732`：“式中 `kern_size` 来自 `loader.rs` 的 `fn compute_kernel_layout` 的 `kern_end_phys - kern_phys_base`……于是栈顶 = 镜像虚拟基址 + 镜像跨距 = 镜像尾的虚拟地址。这个值成立要两个前提：前提一：镜像的最后一个可加载段覆盖到镜像尾……”
- `kernel_info.rs:197-204`、`loader.rs:249-296`、`lib.rs:911-933`。
- 复算命令：`readelf -lW os/target/<target>/release/kernel`（三架构），输出见上表。

**若我错了**：如果某个架构的当前工件跨距不是 4 MiB（例如只有 2 MiB），那么 §3.5 的“当前工件跨距 4 MiB，即两次”这句话在该架构上会错；本机三份产物都是 4 MiB，所以这条没有触发。如果测试载体的 `LoadedImage.info().size` 恰好已是 2 MiB 整数倍，那么测试载体的 `kern_size` 也会等于实际镜像尾；本机没有该载体的产物，未能验证这个巧合。

---

## CLAIM 03

**主张**：Step 1 的重叠 2 MiB 窗口第二次安装会返回 `AlreadyMapped`，调用方把这个返回值当成功吸收；这条臂在根初始化路径下“可达，是重叠窗口的常态返回，不是防御性例外”（文档 §4.3 `:1282-1283`）。§3.4 进一步说 `map_huge` 的 2 MiB 分支“先拒绝已存在的 1 GiB 叶……再拒绝已存在的 2 MiB 叶”，并把它抬成扩展 trait 的契约：“不得把已有大页降级、也不得重复安装同一个叶”（`:837`）。

**反证尝试**：`os/kernel/src/lib.rs:1356-1401` 的 Step 1 代码注释与上述说法直接相反：

> “every write is a fixed 2 MiB leaf, the flags depend only on the piece address (not on which window supplied it), and a second `map_huge` overwrites the same leaf with identical flags. `map_huge` returns `AlreadyMapped` only when a 1 GiB leaf already occupies the slot, which cannot happen here because Step 1 never emits 1 GiB leaves.”

以及循环体内的注释：

> “Overlapping 2 MiB windows rewrite the same leaf and return Ok; `AlreadyMapped` only appears when the slot already holds a 1 GiB leaf. Step 1's fresh root never creates one, so the arm is defensive.”

按这两段注释：Step 1 的重叠窗口第二次写入是“覆盖并返回 `Ok`”，`AlreadyMapped` 臂不可达，是防御性分支；这正好否定文档的“拒绝重写并返回 `AlreadyMapped`”“可达、常态返回”。同时它也否定 §3.4 的“2 MiB 分支再拒绝已存在的 2 MiB 叶”这个契约：同尺寸、同标志的重复安装不会报错。允许读取范围内没有 `paging.rs`，无法用实现体做最终裁决；但文档与同仓 `lib.rs` 注释不能同时为真。

**结果**：阻断 P1（文档把一条可达性与契约级事实写反，且与允许源码的注释直接冲突）。如果只看可观测行为，同标志覆盖与拒绝重写的最终映射相同，可以降为 P2；但文档用这条事实解释 Step 1 的 `match` 臂和 `AlreadyMapped` 的安全阀，不能算“已证明”。

**证据**：
- 文档 `:1282-1283`：“Step 1 的重叠窗口会合法地走到 2 MiB 这一支：同一 2 MiB 片被两个窗口各写一次时，第二次安装拒绝重写并返回 `AlreadyMapped`，调用方把这个返回值当成功吸收；因此这条臂在根初始化路径下**可达**，是重叠窗口的常态返回，不是防御性例外。”
- 文档 `:837`：“`AlreadyMapped` 守卫处理的是‘目标块已被占用’的两类情形……2 MiB 分支先拒绝已存在的 1 GiB 叶……再拒绝已存在的 2 MiB 叶。……扩展 trait 承担的不只是尺寸查询，还有‘不得把已有大页降级、也不得重复安装同一个叶’这条非平凡契约。”
- `os/kernel/src/lib.rs:1356-1401` 的两段注释（原文见上）。
- 待裁决的实现体：`os/arch/src/x86_64/paging.rs`、`os/arch/src/riscv64/paging.rs`（不在本次允许读取清单内）。

**若我错了**：如果 `paging.rs` 的 2 MiB 分支确实对任何 `PRESENT` 叶返回 `AlreadyMapped`，那么 `lib.rs` 的两段注释是过时的，文档正确；但同一仓里两处相反的说法必须改一处，否则后续读者无法判断 Step 1 的 `Ok(()) | Err(AlreadyMapped)` 合并臂到底是不是常态路径。

---

## CLAIM 04

**主张**：若 `kern_virt == kern_phys` 时照做 Step 2，且 `kern_huge` 选中 1 GiB，1 GiB 叶会覆盖 Step 1 的 2 MiB 叶或它所在的下级页表指针；镜像仍是恒等映射，但该 1 GiB 槽内的 `DEVICE` 分片与窄窗口被改掉——MMIO 缓存属性与窗口边界改变（文档 §4.3 `:1327-1331`）。

**反证尝试**：文档 §3.4 自己说 1 GiB 分支“先拒绝任何已存在的 PDPT 条目”，引用的代码是：

```rust
if size_bytes == 1 << 30 {
    if e3 & X64PteFlags::PRESENT.bits() != 0 {
        return Err(PageTableError::AlreadyMapped);
    }
```

（文档 `:809-814`。）§3.6 又把 `AlreadyMapped` 写成安全阀：“1 GiB 叶与已有 2 MiB 子项冲突时把整槽顶掉。冲突要在建表时停下，不能等到取指。”（`:921`。）Step 1 用 2 MiB 叶建表时，该 1 GiB 槽的 PDPT 条目必然 `PRESENT` 并指向 PD 表；因此 Step 2 的 1 GiB `map_huge` 会先返回 `AlreadyMapped`。而 `build_bootstrap_root_and_enable` 的 Step 2 用的是：

```rust
paging.map_huge(...).expect("kernel map: map_huge failed — kernel image mismatch")
```

（`os/kernel/src/lib.rs:1417-1428`。）所以实际结果是启动期 panic，不是静默覆盖。文档描述的“覆盖后 MMIO 属性与窗口边界改变”在这条代码路径上不可达；它把失败方式写成了“静默改写”，而同一文档的 §3.4/§3.6 说这条路径会被安全阀拦住。

**结果**：阻断 P1（失败路径描述错误；同一文档的 §3.4/§3.6 与 §4.3 互相矛盾）。

**证据**：
- 文档 `:1327-1331`：“同一个 VPN[2] 槽位第二次写入一条 1 GiB 叶，会把 Step 1 的 2 MiB 叶（或它所在的下级页表指针）覆盖掉……被覆盖的是该 1 GiB 槽内的 `DEVICE` 分片与窄窗口——MMIO 缓存属性与窗口边界改变。”
- 文档 `:809-814`（1 GiB 分支的 `if e3 & PRESENT != 0 { return Err(AlreadyMapped) }`）。
- 文档 `:921`（`AlreadyMapped` 安全阀：“冲突要在建表时停下”）。
- `os/kernel/src/lib.rs:1417-1428`（Step 2 的 `.expect("kernel map: map_huge failed — kernel image mismatch")`）。

**若我错了**：如果 1 GiB 分支只检查“已存在的 1 GiB 叶”（`e3 & PRESENT && e3 & PS`），不检查表指针，那么覆盖确实可能；但那样 §3.4 的“先拒绝任何已存在的 PDPT 条目”就是错的，§3.6 的安全阀描述也不完整。文档仍然不能同时成立。另一种可能的辩解是“这段只是假设没有守卫时会发生什么”，但原文没有这个限定，反而说“会把……覆盖掉”，读者会把它当成现行行为。

---

## CLAIM 05

**主张**：J3 的 Step 1 直核腿恒等窗口清单是“内核 LMA、root/bump、模块、平台源与设备窗口”（文档 §4.3 `:1270-1275`）；J3 要求进入高半之前“取指地址、栈地址和即将读取的数据都必须已经有映射”（`:905-914`）。

**反证尝试**：`arch_boot_impl` 在直核路径上除了 `push_common_boot_windows`（内核 LMA、root、可选 bump、模块、平台源、设备窗口）之外，还有一个单独的调用者栈窗口：

```rust
// Direct-core leg: the caller keeps executing on its pre-entry stack —
// the UEFI firmware stack in the bootstrap carriers, the firmware-given
// stack in the direct-boot face. ... the caller's stack IS the boot
// path's own stack, so it belongs in the window list under the same
// category ("shim image/stack", cf. the list documented above). Without
// it the first stack access after enable() — the `ret` that follows the
// CR3/TTBR/satp load — faults into an unmapped page with no IDT/VBAR/
// stvec installed: the I-43 layer-2 silent stop (hello-boot fresh binary:
// rsp=0x7E6_6470 while the image window was [0x5E0_0000,0x600_0000)).
```

（`os/kernel/src/lib.rs:1013-1055`。）这段代码读当前 SP，向下留 4 MiB、按 2 MiB 对齐后压入窗口。文档的直核腿清单没有这一项；§3.6 的概括行虽然写了“栈”，但 §4.3 的逐腿清单是读者实现窗口表的依据，缺这一项会让 J3 的“栈已映射”在直核腿上失效。这个缺口正好对应代码注释里记录的静默停机现场，不是文字细节。

**结果**：阻断 P1（失败路径省略；与 J3 的要求直接冲突）。

**证据**：
- 文档 `:905-914`（J3 的要求与 Step 1 表格：“取指地址、栈地址和即将读取的数据都必须已经有映射”；Step 1 行写“低地址恒等窗口（VA = PA），含当前执行代码、栈与设备窗口分片”）。
- 文档 `:1270-1275`（逐腿窗口来源）：两条 UEFI 跨镜像腿写“shim 栈”，U-Boot shim 腿写“栈在 `.stack` 段”，两个直核腿只写“内核 LMA、root/bump、模块、平台源与设备窗口”。
- `os/kernel/src/lib.rs:1013-1055`（直核腿的调用者栈窗口与失败注释）。

**若我错了**：如果直核腿的调用者栈总是落在内核 LMA 窗口内（例如栈就在镜像的 `.bss` 里），那么这一项可由内核窗口覆盖，文档省略无害；但代码注释明确说它是 UEFI 固件栈或固件给出的栈，不在镜像窗口内，所以才单独压入。某个直核腿的固件栈恰好落在内核 LMA 内只能是巧合，不能当成文档可以默认的前提。

---

## CLAIM 06

**主张**：§5.2 的 L9 说“ELF 实测跨距 = 对应 direct face 的 `KERN_SIZE`（x86_64/riscv64 读 `bootface.rs`，aarch64 读 `bootface_a64.rs`）”（文档 `:1680`）；§5.4 的 M10 说“L9 三架构，按架构选 `bootface.rs`/`bootface_a64.rs`”（`:1728`）。

**反证尝试**：`os/kernel-image/src/main.rs:329-333` 只在 `target_arch = "riscv64"` 时编译 `mod bootface;`：

```rust
#[cfg(target_arch = "riscv64")]
mod bootface;
```

`os/kernel-image/src/bootface.rs:96-113` 的常量是 riscv64 的 `DRAM_BASE = 0x8000_0000`、`KERN_PHYS_BASE = 0x8020_0000`、`KERN_VIRT_BASE = 0xFFFF_FFC0_0000_0000`。文档 §4.5.1 自己也写“x86_64 没有直核面”（`:1617`）。因此“x86_64 读 `bootface.rs`”不能字面成立为“对应 direct face 的 `KERN_SIZE`”。唯一可能的解释是 `check-layout.sh` 对 x86_64 也 grep 同一个 `KERN_SIZE = 0x40_0000` 字面量；那是共享的 4 MiB 期望值，不是 x86_64 的 direct face 常量。

**结果**：非阻断 P2（测试覆盖描述不精确；L9 可能仍然工作，但“对应 direct face”这句话对 x86_64 不成立）。

**证据**：
- 文档 `:1680`（L9 行）、`:1728`（M10 行）、`:1617`（“x86_64 没有直核面”）。
- `os/kernel-image/src/main.rs:329-333`（`mod bootface` 只在 riscv64 编译）。
- `os/kernel-image/src/bootface.rs:96-113`（riscv64 的 `KERN_PHYS_BASE`/`KERN_VIRT_BASE`/`KERN_SIZE`）。
- `os/kernel-image/src/bootface_a64.rs:69-80`（aarch64 常量，`KERN_SIZE = 0x40_0000`）。

**若我错了**：`check-layout.sh`（不在本次允许读取清单内）可能有一个显式的 x86_64 分支读取 `bootface.rs` 的 `KERN_SIZE`，那么 L9 的检查本身成立，只是措辞把“共享常量”说成了“对应 direct face”；这仍是 P2，不是功能缺陷。若脚本对 x86_64 根本不查 L9，则应升级为 P1（测试覆盖声明缺失）。

---

## 附注（非编号主张，均为 P2）

1. **标题重复**：§3.6 标题出现两次（`02-higher-half-kernel.md:901` 与 `:903`），§3.8 标题出现两次（`:966` 与 `:968`）。这是结构问题，不影响论证，但会让目录与锚点重复。
2. **悬空下游引用**：§2.4 说 `pg_mapkernel` 的返回值“它的下游见 §4.3”（`:361`），但 §4.3 没有讨论 `freepde_start`/`free pde`；C 的下游（VM 的 `pt_init`）不在本篇射程内。建议改成“C 侧下游属 VM 页表初始化，本篇不展开”。
3. **载荷计数术语不统一**：`arch_boot_resume_v1` 的文档注释说“land all seven payloads”（`os/kernel/src/lib.rs:621-623`），正文 §3.1 说“六块载荷（memmap/reserved/modules/platform/param/sources）”（`:556`），§4.5 又说“信封与……六个分区”（`:1470`）。把信封算不算一块载荷说清楚即可。
4. **`/DISCARD/` 与 `.eh_frame_hdr`**：§4.1 的 `/DISCARD/` 通配写了 `.eh_frame .eh_frame.*`（`:1073`），§6.2 又说 `.rodata` 含 `.eh_frame_hdr`（`:1762`）；本机 artifact 的 `readelf -lW` 确实显示 `.rodata .eh_frame_hdr`。这不一定是矛盾（`.eh_frame_hdr` 可能是链接器合成节，不受该输入通配影响），但建议在 `/DISCARD/` 段加一句，避免读者把两者当成互相否定。

---

## 未构成反证的尝试

- **x86_64 `jump_to_kmain` 的对齐算式**：文档 §3.7 `:964` 说 `and rsp, -16`、`push 0`、`sub rsp, 8`、`call` 之后入口 RSP = `stack_top − 24`（8 mod 16），去掉 `sub rsp, 8` 或去掉 `push 0` 都会破坏 System V 入口对齐。逐条复算成立：`push 0` 后 `top−8`，`sub` 后 `top−16`，`call` 压返回地址后 `top−24`；去掉 `sub` 或去掉 `push` 都会让入口变成 0 mod 16。`os/kernel/src/arch/x86_64/higher_half.rs:25-75` 的断言与 asm 与文档一致，`kmain_verify` 的 `sp_aligned` 也一致（`os/kernel/src/lib.rs:2316-2319`）。没有找到反例。
- **Rust 生产镜像的 4 MiB 跨距与同平移量**：三份 artifact 的 `readelf -lW` 输出与文档 §3.5/§6.2 的读数一致；`kern_virt_base + kern_size` 与末段 `VMA + MemSiz` 相等。没有找到反例（`kern_size` 由 LMA 端点算、`kern_virt_base` 取 VMA 最小值，在平移一致时数值相等）。
- **`store_kernel_info` 的 `.bss` 副本**：文档 §4.5 说生产 `kmain` 第一条语句遮蔽为 `.bss` 副本，形参无读者。`os/kernel/src/lib.rs:1811-1826` 的 `kmain(_kernel_info)` 确实在函数体开头用 `crate::kernel_info()` 遮蔽；`store_kernel_info` 把六类切片都落进 `.bss` 静态池（`:1579-1697`）。没有找到反例。
