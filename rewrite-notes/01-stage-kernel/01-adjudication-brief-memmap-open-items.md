# 引导期内存政策：今天的保证、未决问题与候选方案

> **重写**：2026-11-07 01:50 CST（`date` 实测）；**内容轮次：第一百二十五轮——O-11 严格两快照**（配置表 DTB 的 header 读取与副本分配移到预检快照之下；配对循环自己的取图是最终快照，全范围页类型复核与 body 复制只在它上面发生，此后到 `ExitBootServices` 零分配。本轮同时补齐上一轮 wire v7 的正文读数）。
> **本文件是送审正文，也是唯一的「现行规则」来源**。体积收缩时被移出正文的内容分两处存放，逐字保留、未改写：取证引文、用例穷举、逐轮读数与带行号的定位表在同目录 `03`，按批次从第五十四节递增到第七十节（正文写「见 `03`」即指这些节；六十六～六十七节存 §2/§7 收紧与分配路径读数，七〇节存第一百轮整段移交的设备资源与平台窗口原文）。另有三份配套：**设备资源发现**归同目录 `04-platform-discovery.md`；**硬件准入、用途资格与引导根映射计划**在同目录 `01-design-note-hardware-admission-model.md`（第九十六与一百轮逐字迁出，非本项验收条件）；本方自设又被推翻的判断与过程叙述在 `02` 第七节（标注了撤销理由，不构成合同）。两处与本文冲突时以本文为准。`04-design-backlog-round28.md` 是第二十八轮的冻结快照，不再更新。
> **送审前请对一下本行时间**：低于你正在评审的那一轮即说明材料不是最新。
> **读者**：看不到代码仓库的外部评审人。本文不要求你读代码，但要求你能判断两件事——问题是否真实成立、候选方案是否覆盖了失败形态。
> **怎么读**：§0 是现行模型的名字与它们之间的等式；§2 是代码**今天真实**保证的行为，每条都有测试名；§5 是**尚未裁定**的问题，其中出现的公式与方案一律是**候选**，不是合同。**不要把 §5 的任何一行当成实现依据。**

## 0. 一页速览：这条链今天是什么

本项交付的是从固件输入到 VM 可用页的一条链，四段都有代码与测试：

```
固件输入 ──► 来源记录 ──► 归一化投影 ──► 第一跳 ──► 内核政策 ──► 第二跳 ──► VM
UEFI 内存图    SourceRecord   memmap/reserved   BootHandoffV1   classify   VmBootHandoff   reconcile
DTB /memory
DTB /reserved-memory
MRB
```

| 交付物 | 名字 | 代码入口 |
|---|---|---|
| 来源事实 | `SourceRecord { id, kind, origin, range, input_object, local_source, snapshot_id, attribute, virtual_start, address_domain, raw_type }` | `os/libs/minix-boot/src/memmap_source.rs` |
| DTB 采集 | `DtbMemoryMap::collect_with_sources` | 同上 |
| UEFI 分类 | `adjudicate`（五步）+ `Plan::commit` | `os/boot-shim/src/ebs_map.rs` |
| 第一跳 | `BootHandoffV1`（wire v7，9 页 arena；296 来源槽 = 256 UEFI + 40 DTB，16 启动对象槽） | `os/libs/minix-boot/src/handoff_v1{,_read,_write}.rs` |
| 内核政策 | `KernelInfo.memmap_sources` + `validate` 第 6 步复核 | `os/libs/minix-boot/src/kernel_info.rs` |
| 第二跳 | `VmBootHandoff`（版本 8，47 条扣减上限） | `os/libs/minix-types/src/types/boot.rs`、`os/kernel/src/vm_handoff.rs` |
| VM 消费 | `adopt_free_regions` + `reconcile` | `os/servers/vm/src/boot.rs` |

四类启动机制、五条具体路径（x86 UEFI、aarch64 UEFI、aarch64 直启、riscv64 自引导、riscv64 U-Boot），各自的来源与交付形态：

| 入口 | RAM 权威 | 来源记录 | 第一跳形态 |
|---|---|---|---|
| x86_64 UEFI | UEFI 内存图 | 逐描述符（`SourceOrigin::UefiDescriptor`） | shim → arena → 内核，wire v7 |
| aarch64 UEFI | UEFI 内存图 | 逐描述符；配置表 DTB 只做交叉校验 | 同上 |
| aarch64 直启 | 启动 DTB `/memory` | `MemoryNode` / `ReservedMemoryChild` / `MbrEntry` | 无（同一映像 `.bss`） |
| riscv64 自引导 | 启动 DTB `/memory` | 同上 | 无（同一映像 `.bss`） |
| riscv64 U-Boot | 启动 DTB `/memory` | 同上 | shim → arena → 内核，wire v7 |

下游模型（用途准入、硬件准入、原子能力、映射计划、设备窗口）不在本文，见 `01-design-note-hardware-admission-model.md` 与 `04-platform-discovery.md`。

## 1. 设计模型

### 1.1 四层：来源事实 → 政策 → 投影 → 归因

| 层 | 回答的问题 | 今天的名字 |
|---|---|---|
| 来源事实 | 哪份输入、哪个节点/描述符声称了这段范围 | `SourceRecord { id, kind, origin, range }` |
| 冲突与政策 | 两份来源冲突时谁赢、类型与属性如何裁决 | `adjudicate`（UEFI 五步）、DTB 的保留叠加规则 |
| 纯集合投影 | 哪些整页可以进入 PMM 来源候选 | `MemoryRegion` 投影（可用表 / 保留表 / 侧表） |
| 归因 | 每个被扣除的页由哪条记录解释 | `deducted[]`（第二跳）+ `audit_source_coverage`（消费边界） |

三个原则贯穿实现：

1. **投影是视图，不是真值**：每条投影字节都必须能追到一条来源记录；`audit_source_coverage` 用贪心链覆盖检查这一点：它的输入是**投影结果**，四条腿在投影产生后、交棒前运行，内核在消费边界再运行一次。它证明的是「投影结果有来源覆盖」，不是「所有原始输入都已得到处理」。
2. **记录带身份，不是带范围**：`origin` 指出「输入对象内的哪一处声明」，`id` 在输入对象内稳定；同一范围的两条声明不会互相吞并。
3. **容量固定、失败关闭**：所有数组都是编译期定长；超容量返回具名错误或停机，从不截断。

### 1.2 为什么不能只信一份来源

- **UEFI 下 `/memory` 不是 RAM 权威**：RAM 由 `GetMemoryMap` 决定；配置表 DTB 的 `/memory` 只用于覆盖对照（不通过即停机）。磁盘镜像里的 `dumpdtb.dtb` 仍是纯诊断输入，不改变任何政策结论。
- **MRB 的强度高于普通保留**：规范要求客户端在启动程序给出明确用途前不得访问，不能与「保留但可读」混为一谈；这一语义在 `SourceKind` 里独立成 `MrbEntry`。
- **aarch64 UEFI 有三份设备树，只有一份是内存输入**：RAM 唯一来源是固件内存图；配置表 DTB 是保留政策的权威来源（三态：不存在／可解析且覆盖通过／不可用即停）；磁盘 DTB 只做诊断。

### 1.3 两跳交接的对象图

```
boot-shim（UEFI/U-Boot 腿）                 kernel                       VM
─────────────────────────                 ──────                       ──
源记录 + 投影 + 标量                       .bss 落地垫                   handoff 页（user RO）
  ├─ 写入 arena（LOADER_DATA/bump 池） ──► 解码：memmap/reserved/        ├─ read_boot_params(): validate
  ├─ 页表根页 + bump 三元组                  modules/platform/param/     ├─ adopt_free_regions(): 原样采纳
  └─ arena 的物理地址经 x0/rbx/a0 传递       sources 逐元素拷贝           └─ reconcile(): 4 条判定
                                            ├─ KernelInfo::validate 第 6 步复核
                                            └─ VmBootHandoff（alloc_page 帧）→ 用户只读映射
```

两条不变量把两段连起来：

- 第一跳：`decode_*` 不制造任何指向 arena 的 `'static` 引用；arena 的最后读取点是解码（`decoded_payloads_survive_arena_poison` 为证）。
- 第二跳：消费边界核对的是「**接到的扣减集合**与自由集合不相交」，并抽查根页与每个模块 blob 在记录内（`reconcile`）；它**不能**证明扣减集合本身覆盖了全部存活对象（内核映像、bump、各池已用后缀的枚举完整性由内核生产者的合同与测试保证，VM 无从独立枚举）。正文不得把「记录内部自洽」写成「所有存活对象都已扣减」。

### 1.4 与 Minix3 C 的关系

C 的 `kinfo.memmap[]` 是引导期自举分配器的记账本（`pg_utils.c` 的 `add_memmap`/`cut_memmap` + `alloc_lowest`）。Rust 侧没有这条消费链：引导分配走 `boot_alloc` 的 bump 池，VM 的引导分配走 `VmBootRegion`/`VmBootAllocator`，各自的排除集由 `vm_boot_exclusions` 与 `classify` 给出。因此本项**不维护可变的内存图副本**（`FREE_MEMMAP` 已删除），C 的两处效果由新结构承接：

- `cut_memmap(&kinfo, mod_start, len)` 的「引导期别把模块页发出去」效果 → `vm_boot_exclusions`（池选择排除）+ `classify` 的扣减记录；
- `add_memmap(&kinfo, mod_start, ...)` 的「VM 加载完把模块页还回去」效果 → **本项刻意不做**：模块 blob 全程保留（VM 的段帧在进入前取自这份空闲表，回收会踩坏 VM，gh53/58）。

### 1.5 边界声明：本项只裁决物理内存图

**I-29 只负责物理内存图与启动对象的物理占用**：哪些范围被权威输入描述为 RAM、哪些被保留或具有特殊类型与生命周期、每条主张来自哪里、内核与启动程序已占用哪些范围、哪些整页可进入 PMM 来源候选、这些事实如何经 boot-shim → kernel → VM 两跳交接而不因容量不足丢失。

**不属于本项**：设备资源发现（DTB 设备节点 `reg`、多级 `ranges`、ACPI/MADT/LAPIC/IO-APIC/PLIC/UART）归 `04-platform-discovery.md`；设备窗口映射政策与实际 PTE 覆盖审计归分页／映射专项；PAT/MTRR、MAIR、PMP/PMA 与处理器访问能力归硬件准入专项；驱动访问点与实际 VA 审计归驱动访问审计专项。**I-29 为了拿到内存图要校验整个 DTB 容器、解析 `/memory`、`/reserved-memory` 与 MRB，但不因此负责发现 DTB 里的全部硬件资源。**

一条不能因收缩而丢掉的边界：`no-map` 与 MRB 的**来源语义**属本项（必须在来源记录里准确保存），把它们**编译**成映射禁止集、访问禁止集并审计实际 PTE 属下游消费者。**本项保存事实与原始语义，下游决定如何兑现。**

**判据**：OS 决定完全不开大页，也不应要求 bootloader 换一种内存图交接方式。

## 2. 实现

### 2.1 来源记录（`os/libs/minix-boot/src/memmap_source.rs`）

```rust
pub struct SourceRecord {
    pub id: u16,                    // 输入对象内稳定（跨输入引用用下面两个字段）
    pub kind: SourceKind,           // Memory / FirmwarePrefix / MrbEntry /
                                    //   ReservedNoMap / ReservedOrdinary /
                                    //   ReservedReusable / StartupObject / UefiExcluded
    pub origin: SourceOrigin,       // 见下
    pub range: PhysicalRange,       // start: u64, end_exclusive: u128
    pub input_object: InputObjectId,// DtbBootInput / UefiMemoryMap / StartupObjects
    pub local_source: u16,          // 输入对象内序号（UEFI = 描述符索引）
    pub snapshot_id: u64,           // 观测实例序号；DTB 为 0（与 MapKey 无关）
    pub attribute: u64,             // UEFI 描述符原值；DTB 为 0
    pub virtual_start: u64,         // UEFI 描述符原值；DTB 为 0
    pub address_domain: AddressDomain, // Physical（v1 记录只有物理域）
    pub raw_type: u32,              // 固件 EFI 类型原值；非 UEFI 输入为 0
}
```

`origin` 的五种取值对应五类输入对象内的位置：

| 变体 | 含义 |
|---|---|
| `MemoryNode { node, reg }` | 第 `node` 个 `/memory` 节点的第 `reg` 个 `reg` 元组 |
| `FirmwarePrefix { memory_id, cut }` | 由第 `memory_id` 条 `/memory` 记录与 `firmware_reserve_end` 派生出的前缀 |
| `MrbEntry(u16)` | 内存保留块的第 `entry` 项 |
| `ReservedMemoryChild { node, reg }` | 第 `node` 个 `/reserved-memory` 子节点的第 `reg` 个元组 |
| `UefiDescriptor { index }` | 最终 `GetMemoryMap` 快照的第 `index` 个描述符 |

`PhysicalRange` 的末端用 128 位，表示**地址空间最后一页**（`[...f000, 2^64)`）；是否能投影成 `u64` 由 `projectable_end()` 回答，拒绝发生在各投影点，而不是把 `2^64` 悄悄截成 0。

**记录身份（第一百一十六轮；第一百二十轮修正确切语义）**：`SourceRecord` 增加 `input_object`（`DtbBootInput` / `UefiMemoryMap` / `StartupObjects`）、`local_source`（输入对象内序号）、`snapshot_id`（**观测实例序号**：每成功取一次 `GetMemoryMap` 自增一次；它与 `MapKey` 不是一回事——`MapKey` 只是固件给 `ExitBootServices` 的版本凭据，两次取图可能返回同一个 key，但仍是两份观测）、`attribute` 与 `virtual_start`（UEFI 描述符原值；DTB 为 0）、`address_domain`（`Physical` / `UefiMmioPortSpace`；后者用于 EFI 类型 12）与 `raw_type`（固件 EFI 类型原值；非 UEFI 输入为 0）。`kind` 是本项目的角色分类，`raw_type` 是固件的类型事实，二者不得混淆。跨输入引用只用 `(input_object, local_source)`，`id` 仅在单个输入对象内稳定。结构合法但被类型政策排除的描述符（`Side`/`Excluded`/`DevicePath`）也保留一条 `kind = UefiExcluded` 的记录，来源事实不因不进入投影而消失。

**不可读的 `reg` 形状一律拒绝**（第一百一十三轮）：`/memory` 与 `/reserved-memory` 的 `reg` 在声明 cell 宽度下截断（或 `#size-cells = 0`）时，`fdt` 迭代器不产出任何元组；把它当成「没有这段」会静默丢 RAM 或释放固件保留页。因此 `/memory` 返回 `UnsupportedMemoryReg`，`/reserved-memory` 返回 `UnsupportedReservedDeclaration`，四条负控：`dtb_truncated_memory_reg_is_rejected`、`dtb_zero_size_memory_reg_is_rejected`、`dtb_truncated_reserved_reg_is_rejected`、`dtb_zero_size_reservation_is_rejected`。

### 2.2 DTB 采集（三条 DTB 腿共用）

`DtbMemoryMap::new(&fdt, firmware_reserve_end).collect_with_sources()` 返回三个 `'static` 切片：可用表、保留表、来源记录。容量从结构推导，不猜：

- 可用表 / 保留表各 `DTB_MEMMAP_MAX = 8` 项；
- MRB `DTB_MRB_MAX = 16` 项；
- 记录槽 `DTB_SOURCE_MAX = 8 + 8 + 16 + 8 = 40`，恰好可达（正文此前写作 `SOURCE_RECORD_CAP`，代码里没有这个符号）；精确重复声明只占记录槽、不占投影槽，超过 40 仍返回 `TooManySourceRecords`，不截断。

**跨来源冲突按页拒绝**（第一百一十三轮）：非 `no-map` 的 `/reserved-memory` 与 MRB 重叠时，MRB 的访问禁令与普通保留的「可直达」不能同时兑现；页量化后返回 `MrbConflictingReservation`（`dtb_mrb_and_ordinary_reservation_overlap_is_rejected`、`dtb_mrb_and_reusable_reservation_overlap_is_rejected`、`dtb_mrb_and_reservation_on_the_same_page_conflict`）；`no-map` × MRB 双方都禁止访问，保持兼容（`dtb_mrb_and_no_map_reservation_are_compatible`）。FDT 容器的 MRB 上限与 collector 对齐为 16 条（第 17 条才拒绝，`accepts_exactly_the_collector_reservation_bound`）。`collect_with_panorama()` 额外返回去重后的 `/memory` 全景，供 UEFI 的静态保留类型核对使用（§2.3）。

投影前必跑覆盖审计（贪心链，节选）：

```rust
fn records_cover<F: Fn(&SourceRecord) -> bool>(start: u128, end: u128,
                                               sources: &[SourceRecord], keep: F) -> bool {
    let mut cursor = start;
    while cursor < end {
        let mut next = cursor;
        for r in sources {
            if keep(r) && r.range.start as u128 <= cursor && cursor < r.range.end_exclusive {
                next = next.max(r.range.end_exclusive);
            }
        }
        if next == cursor { return false; }
        cursor = next;
    }
    true
}
```

`audit_source_coverage(free, reserved, sources)` 分别用「`kind == Memory`」与「保留类 kind」两组谓词跑这条链；任一跨度无来源即返回具名错误，调用方停机。

三条 DTB 腿的差异只在「DTB 从哪来」：

| 腿 | DTB 来源 | 采集后 |
|---|---|---|
| riscv64 自引导（`kernel-image/src/bootface.rs`） | OpenSBI `a1` | 写进同映像 `.bss`，直接构造 `KernelInfo` |
| aarch64 直启（`kernel-image/src/bootface_a64.rs`） | QEMU 放在 RAM 基址的 DTB | 同上 |
| riscv64 U-Boot（`boot-shim/src/opensbi_helpers.rs`） | U-Boot `argv[0]` | 记录写进 arena 的来源分区，跨镜像交付 |

### 2.3 UEFI 分类器（`os/boot-shim/src/ebs_map.rs`）

**唯一解析器**：`MapStorage::parse_raw_map` 按 `DescriptorSize` 步长在原始缓冲上读固定偏移，不把描述符 cast 成结构体（固件可能给出更大的 stride）。描述符上限 `MAX_MAP_DESCRIPTORS = 256`，超出整体拒绝。

**五步裁决** `adjudicate(desc, pa_limit)`（节选第 4、5 步）：

```rust
// 第 4 步：独立语义的属性放弃两种普通角色
if a & attr::TYPE_INDEPENDENT != 0 { return Ok((Disposition::Side, len)); }
// 第 5 步：写回能力 + 类型表
if a & attr::WRITE_BACK == 0 { return Ok((Disposition::Side, len)); }
match desc.raw_type {
    ty::CONVENTIONAL => Ok((Disposition::Allocate, len)),
    ty::LOADER_CODE | ty::LOADER_DATA | ty::BOOT_SERVICES_CODE
    | ty::BOOT_SERVICES_DATA | ty::ACPI_RECLAIM => Ok((Disposition::ReservedMapped, len)),
    _ => Ok((Disposition::Side, len)),
}
```

| `Disposition` | 去向 | 来源记录 |
|---|---|---|
| `Allocate` | 可用表（进 PMM 候选） | `kind = Memory` |
| `ReservedMapped` | 保留表（直接映射可达，不再分配） | `kind = ReservedOrdinary` |
| `Side` | 侧表（两份普通表都不收） | `kind = UefiExcluded`（保留范围与 `raw_type`，不参与覆盖审计） |
| `Excluded` / `DevicePath` | 无条目（`RESERVED`/`UNUSABLE`/MMIO） | `kind = UefiExcluded`（同上；不产出投影字节） |

**快照纪律**（`ebs_pair::pair_and_classify`）：每个 EBS 尝试取一次图、按该 MapKey 分类、立即 `ExitBootServices`；被拒则重取重分类（`reset_map_partitions` 连来源分区一起清空），总尝试 2 次；容量预检在写之前完成（`TooManySourceRecords` 等）。

**逐描述符来源记录**：`Plan::commit` 为**每一个结构合法的描述符**写一条记录——进入普通表的用 `Memory`/`ReservedOrdinary`，其余（`Side`/`Excluded`/`DevicePath`）用 `UefiExcluded` 并保留 `raw_type`；范围是分类器发出的区间（无区间的用描述符原始 `PhysicalStart + Pages×4 KiB`）。容量按三句联动证明：`UefiDescriptorCount <= MAX_MAP_DESCRIPTORS`（解析器存储上限）、`DtbSourceRecordCount <= DTB_SOURCE_CAP`（采集器上限）、`MAX_MAP_DESCRIPTORS + DTB_SOURCE_CAP <= WIRE_SOURCES_CAP = 296`（`ebs_map` 的编译期断言）。运行期只检查描述符数不超过解析器上限；两类记录在同一个分区里的总和由第三条不等式保证。

```rust
let record = SourceRecord {
    id: index as u16,
    kind,
    origin: SourceOrigin::UefiDescriptor { index: index as u16 },
    range: PhysicalRange::new(region.base.0, region.base.0 as u128 + region.len as u128),
    input_object: InputObjectId::UefiMemoryMap,
    address_domain: AddressDomain::Physical,
    local_source: index as u16,
    snapshot_id,
    attribute: descs[index].attribute,
    virtual_start: descs[index].virtual_start,
    raw_type: descs[index].raw_type,
};
serializer.push_source(&record).expect("preflight proved the source partition");
```

每条记录同时携带身份：`input_object = UefiMemoryMap`（输入**类别**）、`local_source = 描述符索引`、`snapshot_id` 为本次取图的**观测实例序号**（与 `MapKey` 无关）、`attribute`/`virtual_start`/`raw_type` 为描述符原值。跨快照的同号描述符因此可区分。**拒绝诊断**：`adjudicate` 失败时 `classify` 返回 `MapError::RejectedDescriptor`，携带 `index`、`raw_type`、`physical_start`、`virtual_start`、`number_of_pages`、`attribute` 与 `failed`（`PolicyError`），停机报告不再只有错误码（`a_refused_descriptor_is_named_in_the_error`）。容量边界两向都有测试：257 条描述符拒绝（`rejects_a_map_larger_than_the_storage`），恰好 256 条且同时填满 128 可用 / 64 保留 / 64 侧表通过（`accepts_exactly_the_descriptor_and_partition_caps`）。

**静态保留 × EFI 类型覆盖**（`boot-shim/src/resv_claims.rs`，第一百一十三轮）：配置表 DTB 的 `/reserved-memory` 与 MRB 记录按 4 KiB 页向外量化成类型主张——普通/`reusable` → `EfiBootServicesData`，`no-map`/MRB → `EfiReservedMemoryType`；只保留与 `/memory` 全景相交的页（图外 framebuffer 声明不是 RAM 主张）。主张在 `find_platform_sources` 时采集（`resv_claims::capture`），EBS 配对循环在**分类之后、`ExitBootServices` 之前**逐页核对（`check_current`，失败形状 `PairError::ReservationCoverage`）；**配置表 DTB 的来源记录（≤40）也在 `capture` 时留存，并在配对循环里随 UEFI 记录一起写入来源分区**，供内核保留原始声明身份。：无覆盖或同页出现异类描述符即拒绝启动，即使另有兼容描述符覆盖同页也算冲突（保守加强）；配置表 DTB 自身页按 UEFI 2.11 允许 `EfiACPIReclaimMemory`。容量固定：主张 `MAX_CLAIMS = 64`、每条主张的覆盖区间 `MAX_INTERVALS = 256`，溢出拒绝不截断。三条必测形态（首页正确后续页错、多描述符连续完整覆盖、中间页异类）与边界共 9 条单测；默认 ACPI 腿无配置表 DTB（无主张、按空集通过），`DTB_CFGTABLE=1` 腿会真正采集并核对。

**复制到自有缓冲（`boot-shim/src/dtb_copy.rs`，严格两快照）**：三段落码。`note(dtb_pa)` 只记录配置表地址，不读一个字节。`preflight(descs)` 在**预检快照**下验证 `[dtb, dtb+40)` 覆盖的全部页为 `EfiACPIReclaimMemory` 且无同类异型覆盖，一次读 40 字节 header、校验 `totalsize`、分配自有缓冲；这次分配会改变活内存图，所以最终快照必须是再一次 `GetMemoryMap`。`authorize_and_copy(descs)` 在**最终快照**（配对循环自己的取图，与 `ExitBootServices` 的 MapKey 同源）下复核 `Outward([dtb, dtb+totalsize))` 的全部页，把预检捕获的 header 写入副本前 40 字节、只从源地址复制 body 一次，且不再分配；EBS 重试复用副本、不重读源。`ebs_pair::preflight_dtb` 只在记录了地址时多取一次图，没有配置表 DTB 的 x86_64 腿零额外调用。此后 `fdt_layout::validate`、解析、`collect` 与 `resv_claims` 只读副本。测试：14 条 `dtb_copy` 单测（页类型正反例、header 边界、最终快照类型变化在 body 复制前拒绝、最终快照分段变化仍覆盖、header 只读一次而 body 只在复制阶段读、副本复用、预检缺失即拒绝、最终阶段零分配）＋3 条 `ebs_pair` 时序测试（预检/最终/重试快照的取图次数、预检缓冲不足不读 header、接受记录的 `snapshot_id` 指向重试快照）。

### 2.4 第一跳：`BootHandoffV1`（wire v7）

arena 是 9 页（36,864 B），布局由编译期算术钉住：

| 分区 | 容量 | 字节 | 偏移 |
|---|---|---|---|
| 信封 | — | 184 B（≤ 256 B 预算） | 0 |
| 可用表（wire） | 128 × 16 B | 2,048 | 256 |
| 保留表（wire） | 256 × 16 B | 4,096 | 2,304 |
| 模块 | 12 × 32 B | 384 | 6,400 |
| 平台描述 | 4 × 16 B | 64 | 6,784 |
| 参数字节 | 1,024 | 1,024 | 6,848 |
| 可用表（语义工作区） | 128 × 16 B | 2,048 | 7,872 |
| 保留表（语义工作区） | 256 × 16 B | 4,096 | 9,920 |
| 来源记录（内存图） | 296 × 72 B | 21,312 | 14,016 |
| 启动对象记录 | 16 × 72 B | 1,152 | 35,328 |
| 余量 | — | 384 | 36,480 |

编译期门（`handoff_v1.rs` 的 `const _` 块）钉住：信封 size 184、`sources_byte_len` 偏移 168、`startup_byte_len` 偏移 176、`WireSourceRecord` 72 B/对齐 8、`WIRE_SOURCES_END == 35328`、`WIRE_STARTUP_END == 36480`、`SEMANTIC_END == 14016`、`ARENA_CAP % 4096 == 0`。

来源记录的线上布局（72 B，无损）：

```text
start u64 | end_lo u64 | end_hi u64 | attribute u64 | virtual_start u64
| snapshot u64 | cut u64 | id u16 | local u16 | a u16 | b u16
| kind u8 | origin_tag u8 | input_object u8 | address_domain u8 | raw_type u32
```

`end_hi ∈ {0,1}`（128 位末端的两个半字）；`a/b/cut` 是 origin 的变体载荷；编码器恒写 0 的字段若非零，解码按 `BadSourceRecord` 失败关闭；未知 `kind`/`origin_tag` 同样拒绝。**地址域与角色的配对也有解码闸**：非 `Physical` 域只允许 `kind = UefiExcluded`（类型 12 是来源事实、不是物理范围），其余组合按坏记录拒绝（`non_physical_domain_is_only_legal_on_excluded_records`）。

**版本**：`BOOT_HANDOFF_V1_VERSION = 7`（v3 增加身份、属性与虚拟地址；v4 增加启动对象分区；v5 把尾字从保留位改为固件 `raw_type` 并新增 `UefiExcluded` 角色；v6 把来源分区扩到 296 条并后移启动对象分区；v7 把信封偏移 12 的保留字改为 `ram_authority`），接收端按**精确相等**比较；v1–v6 的发送方会被 `UnsupportedVersion` 拒收，不会按新偏移读旧镜像。信封里的 `ram_authority`（0=启动 DTB，1=UEFI 内存图）由发送腿声明，接收端拒绝其它值。

**两类记录分槽**：内存图来源记录回答「这条投影从哪来」，启动对象记录回答「谁占了什么」。启动对象记录用独立分区（16 槽）与独立 `SourceKind::StartupObject` / `SourceOrigin::StartupObject` / `InputObjectId::StartupObjects`，不占内存图分区的 296 槽；`EBS` 重试清空分类快照时**不清**该分区（`reset_map_partitions_keeps_the_startup_partition`），因为它描述的是映像布局而非某次 MapKey 的分类结果。写入前按完整清单核算容量：UEFI shim 路径为「内核映像 + 全部模块 + 根页 + bump + arena」（最坏 1+12+3 = 16），三条 DTB 腿为「内核映像 + 全部模块」（1+12 = 13）；列表在第一次写入前断言 `<= WIRE_STARTUP_CAP`，序列化器再按 `WIRE_STARTUP_CAP` 拒绝一次——超限不截断。

### 2.5 内核侧：落地垫、承载字段、消费者边界

`arch_boot_resume_v1(arena_pa)` 是三条 shim 腿的统一入口：先解码信封，再把六个载荷逐元素拷进内核 `.bss` 的落地垫（`MEMMAP_REGION_STORE`、`RESERVED_REGION_STORE`、`BOOT_MODULE_STORE`/`BOOT_MODULE_NAME_STORE`、`PLATFORM_SOURCE_STORE`、`PARAM_BUF_STORE`、`SOURCE_RECORD_STORE`、`STARTUP_RECORD_STORE`），然后构造 `KernelInfo`。`handoff_v1_read` 的每个 `decode_*` 只写调用方给的缓冲区，**不制造指向 arena 的 `'static` 引用**——这是 arena 的最后使用点。

`KernelInfo.memmap_sources: &'static [SourceRecord]` 是政策的承载字段：

- 三条 DTB 腿在 `bootface*.rs` 里直接赋值（记录与投影在同一映像的 `.bss`）；
- U-Boot 腿在接收端解码到 `SOURCE_RECORD_STORE` 后赋值；
- UEFI 腿同样由 `decode_sources` 落地。

`KernelInfo.startup_objects: &'static [SourceRecord]` 是启动对象的承载字段，与 `memmap_sources` 并列但独立：三条 DTB 腿在同一映像的 `.bss` 里填内核映像与每个模块的记录，U-Boot 腿由接收端 `decode_startup_objects` 解码到 `STARTUP_RECORD_STORE`。它**不参与**来源覆盖审计——`audit_source_coverage` 只核对内存图分区，启动对象记录既不能补上内存图的缺口，也不会因不在投影里被误判。

`KernelInfo::validate()` 第 6 步是**消费者边界复核**；`validate_requiring_sources()` 是生产形态的严格门（第一百一十三轮）：

```rust
pub fn validate(&self) { self.validate_impl(false); }
pub fn validate_requiring_sources(&self) { self.validate_impl(true); }

fn validate_impl(&self, require_sources: bool) {
    // ... 前五项非地址不变量 ...
    if require_sources {
        assert!(!self.memmap_sources.is_empty(),
            "KernelInfo: production handoff carries no memory-map source records");
    }
    if !self.memmap_sources.is_empty() {
        match audit_source_coverage(self.memmap, self.reserved_regions, self.memmap_sources) {
            Ok(()) => {}
            Err(error) => panic!(
                "KernelInfo: source records do not cover the projected map: {error:?}"),
        }
    }
}
```

跨镜像 `arch_boot_resume_v1` 直接调用严格门；三条 DTB 腿在交出 `KernelInfo` 前断言来源非空。手工构造 `KernelInfo` 的 qemu 测试内核没有 DTB 来源，仍走宽松 `validate()`——两条测试 `test_validate_requiring_sources_rejects_an_empty_partition` / `_accepts_covering_records` 钉住两种形态。

**覆盖审计按输入权威过滤（显式）**：权威由发送腿在信封里声明（`ram_authority`），接收端调用 `validate_requiring_sources(authority)`：该权威必须至少有一条记录，缺失即失败关闭（**不回退**到另一种输入），并用 `audit_source_coverage_for(..., Some(authority))` 证明覆盖。aarch64 UEFI 的配置表 DTB 记录只做静态保留交叉核对，不能替 UEFI RAM 投影作证（混合输入测试 `uefi_projection_cannot_be_covered_by_a_dtb_memory_record`、`reserved_projection_is_also_authority_filtered` 与「声明的权威无记录即拒绝」`test_validate_requiring_sources_refuses_missing_authority`）。

**DTB 原始对象的时序假设**：`FREE_MEMMAP` 与 Phase A.2 已删除，自引导与 UEFI 腿都不再把 DTB 原始对象写进任何扣减记录；安全前提是「Phase A.5 解析完即无人再读原 blob」——任何后置消费者都必须先复制它，或把它登记为存活对象。声明同时写在 `kmain` 的 Phase A.2 注释与 `bootface_a64` 的采集点；已上机的四条腿不存在后置读者。

自引导两条腿没有 arena：采集结果（可用表、保留表、来源记录、参数）全部写在内核映像自己的 `.bss`，由 `classify` 的第一条扣减记录（内核映像整段）覆盖。映像跨度不得超过交给 `KernelInfo` 的 `KERN_SIZE = 0x40_0000`，这条前提由两个自引导链接脚本的链接期断言守住：

```
ASSERT(__bss_end - KERNEL_VIRT_BASE <= 0x400000,
       "kernel-image memsz exceeds KERN_SIZE (0x400000) — update KERN_SIZE in src/bootface*.rs")
```

### 2.6 第二跳：`VmBootHandoff`

`build_vm_handoff`/`classify` 在分类点把「内核自己占了什么」写成扣减记录，逐条可枚举：

| 来源 | 代码 |
|---|---|
| 内核映像 | `kernel_info.kern_phys_base()/kern_size()` |
| VM 自己的根页 | `arch_boot` 参数（交接里的 `root_paddr`） |
| 启动 bump 区整段 | `boot_alloc::boot_alloc_region()` |
| 引导期已用帧 | `VmBootAllocator` 各池区间的已用后缀（页表页在这里） |
| 保留模块 blob | `KernelInfo.boot_modules()` 的**全部** blob，含 VM 自己的 |

`push` 只做页取整与对齐断言，**不交集、不合并**：每个来源恰好一条记录，池区间按「被触碰的区间数」计。容量上界因此是 `3 + 32 + 12 = 47`（`MAX_BOOT_REGIONS = 32` 是分配器的聚合上限，`NR_BOOT_MODULES = 12`），由编译期断言与两枚边界测试钉住；超一条即启动期失败（`deduction record overflow`），不截断。

自由页集合 = 未裁剪 memmap − 扣减记录，再裁剪到 VM 的 Direct Map 窗口；交接页本身来自 `VmBootAllocator::alloc_page()`，落在某个池区间的已用后缀里，**不单独成条**（`test_bootstrap_frames_are_recorded_per_region_not_per_frame` 钉住「记录数不随帧数增长」且「每个交付帧都被覆盖」）。

`VmBootHandoff` 的合同：

| 项 | 值/语义 |
|---|---|
| 物理承载 | 一页普通帧，`VmBootAllocator::alloc_page()` 在分类点前取走，随 `deducted` 保留 |
| 固定 VA | `VM_BOOT_HANDOFF_VA = 0x1_0000_0000`；与 `KERNINFO_USER_VA` 一起用编译期断言钉在 2^38 以下（Sv39 用户上限，三架构最严） |
| 布局 | `#[repr(C)]`、little-endian、3,792 B（≤ 一页）；九个关键偏移有钉桩测试 |
| 版本 | `VM_BOOT_HANDOFF_VERSION = 8`，按精确相等比较 |
| 失败语义 | `magic` 不符、版本不等、`root_paddr` 为零/未对齐、任一 count 超容量 → `validate()` 断言失败，VM 不启动（七枚测试） |

### 2.7 VM 侧：原样采纳 + 消费边界

`read_boot_params()` 先 `validate()`，然后**原样**把交接页里的自由区映射成 VM 的内部类型（`adopt_free_regions`）——不在自己的入口再切任何范围。唯一的裁决点是 `reconcile`：

1. 扣减记录非空；
2. 被采纳的根页在记录内；
3. 每个保留模块 blob 在记录内（VM 自己的 blob 跳过，因为内核已把它写进记录）；
4. `free ∩ deducted = ∅`（Proof 2 在消费点）。

此前 VM 入口会按字面量 `0x8200_0000..0x8400_0000` 再切 32 MiB（riscv64 QEMU 的 bump 池地址）并把模块 blob 重扣一遍；这在 aarch64／UEFI 腿上指向普通 RAM，且会让第 4 条检查看不到真正的记账缺陷。两处都已删除（`test_adopt_free_regions_keeps_every_region_verbatim` 钉住「0x8200_0000 的区间原样保留」）。


### 2.8 与分页专项的交界（不作为本项验收）

两跳交接只回答「哪些页可以交给 VM」；把这些页映射进页表、以及映射后的叶子审计属分页专项。交界处的三个事实在这里点名，避免读者把交接当成映射保证：

- **引导根的建立不消费本项的政策**：`os/kernel/src/lib.rs:fn build_bootstrap_root_and_enable` 的恒等循环只按 `kern_huge`/`map_huge` 建叶子，不查 memmap、不查类型、不查禁止集合；三架构的 `paging.rs:fn map_huge` 写法不同（x86-64 会改写既存叶子并降级 1 GiB 叶子，arm64/riscv64 遇已存在槽即失败）。
- **直接映射的候选过滤**在 `os/kernel/src/dm_coverage.rs:fn establish_boot_dm`（四个调用点全部在根激活之后），它与本项的关系只是「候选来自交接后的可用表」；`dm_coverage` 里「bump inside memmap → dropped」的测试测的是这个过滤，不是分配器排除。
- **页表页本身**：引导期页表来自 `init_boot_pt_alloc` 登记的 bump 区间与 `VmBootAllocator` 的池区间，这两类都由第二跳的扣减记录覆盖（§2.6）；`vm_handoff` 的 disjoint 断言与 `ebs_map` 的分类结果各自只对自己那一段负责。

这三条不改变 §1.5 的边界：本项交付来源事实与交接记录，映射与叶子审计在专项。

## 3. 三条不变量（本项责任内的部分）

任何改动不得违反，§5 的候选方案也受它们约束。

1. **来源有身份，原始范围先保留**；量化在**每种政策投影内各执行一次**（同一来源可分别生成 PMM 排除与映射禁止投影）。**「不得合并」只约束来源记录层，不约束纯集合计算**：

| 层次 | 规则 |
|---|---|
| 来源事实层 | 每条声明保留自己的 `SourceRef`、原始范围与语义，来源记录不合并 |
| 冲突解析层 | 对重叠主张做兼容性裁决，保留全部参与裁决的来源身份 |
| 纯集合投影层 | 允许对已裁决的兼容区间做 `Union`，再按该投影规定的顺序量化 |
| 归因层 | 每个投影结果仍可反查贡献它的全部 `SourceRef` |

各投影的量化顺序：`RamSourcePageDomain` 先 `Union` 再 `FullPages`；`TypeAllocatablePages` 先裁决类型/属性兼容、再合并、再 `FullPages`；PMM 显式保留逐来源 `Outward` 后合并页集合；`no-map` 逐声明 `Outward`（映射禁止）；MRB 访问审计保留精确 byte range、页表落实时另行 `Outward`；设备映射按实际叶子逐页检查授权。
2. **PMM 基底层 = 可用 ⊍ 排除**，且只对 `RamSourcePageDomain` 成立（`PmmBaseAvailable(ctx) ⊍ PmmBaseExcluded(ctx)`，见 O-12i）。用途层**不是第二个划分**：`PmmRoleRejected(role, ctx) ⊆ PmmBaseAvailable(ctx)`，同一页可以「普通数据可用、页表用途拒绝」；把用途拒绝并进 `PMMExcluded` 会让同一页同时出现在可用与排除两侧，「一页只排除一次、排除理由全部可追溯」的记账立刻失效。`RamSourceDomain` = 该启动入口的权威来源识别出的物理主存区间，**尚未应用保留声明与对象生命周期**：DTB 入口取 `/memory`；UEFI 入口按明确策略从类型中识别（不是所有描述符）；MMIO、端口、地址空洞、未描述区域都不属于它。同一页有多条保留来源时，排除只计一次、来源身份不丢。
**必须分开的两个概念**：`RamSourceDomain` 回答「权威输入是否声称这段是物理 RAM」；`OrdinaryRamMappingEligibility`（原 `RamBacking`）只回答「当前阶段是否允许把它作为普通主存**映射**」；能否分配由 `PmmEligibility(phase)` 表达，某个对象能否经某条路径访问由 `AccessRequirement/AccessAuthorization` 表达，物理内存性质由 `MemoryKind` 表达。四者不能互相推导。**术语说明**：`backing` 容易被读成「物理支撑」，而本项说的是 OS 政策资格——`RUNTIME_SERVICES_*`、`ACPI_NVS`、`PERSISTENT` 都有物理支撑却不具普通资格，`UNACCEPTED` 有物理来源但未接受。物理支撑性质、OS 政策资格、硬件准入是三个层次，不能压成一个布尔值。`EfiReservedMemoryType` 是典型反例：aarch64 UEFI 的权威 DTB `/reserved-memory` 与 MRB 提供**保留来源身份**，UEFI 内存图提供物理存在与固件处置；两者可交叉验证保留声明，但**都不能单独推出 `RamSourceDomain` 成员资格**——只有独立平台证据能证明属于系统 RAM 时才纳入。x86_64 没有 DTB 来源，它的 `EfiReservedMemoryType` 只能按固件类型政策处置，不能套用 aarch64 的交叉核对。
3. **下游不变量（不属本项，方向不变）**：每棵树各有映射禁止集、访问禁止另立、禁止与分配正交、设备访问 ≠ 设备映射——这些判断仍成立，但集合构造与判定式由 BootRoot／分页／驱动审计专项维护。本项的责任是把产生它们的**来源事实**完整交接（第 1、2 条）。

## 4. 已裁定、但代码未实现

| 合同 | 内容 | 为什么现在还不能当现状 |
|---|---|---|
| `effective_pa_limit` 的含义（代码里是 `support.phys_addr_limit()`） | **物理地址可表示上界**（格式位宽 ∩ 平台位宽），不承诺上界以下可访问 | 已裁定；PMP 与 PMA 是独立准入维度（专项） |
| 恒等 VA 合法性 | `VA = PA` 必须落在该树分页配置的合法虚拟地址范围 | 属 BootRoot 专项，本项不验收 |

「三架构普通主存准入」「保留映射区权限」「设备访问边界」三行随射程移交，逐字见 `03` 七十一节。

## 5. 未决问题与候选方案

**实现状态索引**（逐项对应的代码与测试见 §2；状态以本表为准，正文保留裁定的完整表述）：

| 项 | 状态 | 代码/测试锚点 |
|---|---|---|
| O-01 保留表与生命周期 | **已按 B 落码**（静态处置表；`reusable` 独立 kind）；回收框架（A 方案）未实现 | `ebs_map::adjudicate` 的类型表；`SourceKind::ReservedReusable` |
| O-02a 恒等映射授权 | 移交映射专项；本项只交来源事实 | — |
| O-02b 保留声明不产生映射禁令 | **来源侧已落码**（`ReservedNoMap` 等独立 kind）；禁止集合在专项 | `memmap_source::SourceKind` |
| O-03 禁止项来源分级 | **来源侧已落码**；集合构造在专项 | `SourceKind::class()`（输入声明／派生） |
| O-04 对象负向义务 | 下游合同 | — |
| O-05 冻结时点 | 已移交 | `03` 六十五节 |
| O-06 类型资格三问 | **已落码为 `adjudicate` 五步 + `Disposition`**；字段拆分见本节表格 | `ebs_map::adjudicate`、`write_to` |
| O-07 实际访问需求 | 已移交 | — |
| O-08 端口 I/O 地址域 | 裁定 A；专项未实现 | — |
| O-09 诊断成员存储 | 裁定 A；`dumpdtb` 为诊断输入 | `uefi_helpers::load_dumpdtb_free` |
| O-10 第二枚负控 | 专项 | — |
| O-11 配置表 DTB 首读授权 | **三态、静态保留 × EFI 类型覆盖、copy-first 严格两快照均已落码**；预检快照（header 读一次、副本分配）与最终快照（全范围页类型复核、body 只复制一次、此后零分配）分离 | `uefi_helpers::{find_platform_sources,cross_validate_config_dtb}`、`ebs_pair::{preflight_dtb,pair_and_classify}`、`dtb_copy`、`resv_claims::{capture,check_current}` |
| O-12 集合构造与来源完整性 | **a/g 已落码**（含结构合法描述符的全量记录与 `raw_type`）；UEFI 腿的配置表 DTB 记录尚未随第一跳交付；c/h/i/j 的集合构造在专项 | `Plan::commit`、`WIRE_SOURCES_CAP` |
| O-13 固定布局 ABI | **基线、来源记录与启动对象记录已落码**（wire v7、第二跳版本 8）；`Attribute`/`VirtualStart`/`raw_type` 已记录 | `handoff_v1.rs`、`vm_handoff.rs`、`types/boot.rs` |


每条给出：状态、问题、失败形态、候选（**未裁定**，除标注「已裁定」者）、判据、第一个消费者。评审人请判断问题是否成立、候选是否覆盖失败形态；**不要把候选当实现依据**。

### O-01 保留表与生命周期

**问题**：今天的保留表是静态的，没有生命周期维度。UEFI 的几种类型在规范里**不是永久保留**：`EfiBootServicesCode/Data` 在 `ExitBootServices` 之后可供一般使用；`EfiACPIReclaimMemory` 在 ACPI 启用后可回收；只有 `EfiRuntimeServicesCode/Data` 必须留给运行时服务。第一版一律「永不再分配」是**项目策略**。
**失败形态**：一台机器的大部分内存在 `EfiBootServicesData` 里。第一版永不回收，这段内存永远不可分配；文档若写成「保留映射」，读者会以为这是规范要求。
**候选**：

- **A（生命周期处置）**：每条来源记录带处置 `Permanent` / `Until(event)` / `ReclaimableNow`；两张表按处置与阶段派生。**最终处置是三层叠加**：UEFI 类型生命周期 + DTB 保留声明叠加（同一 `EfiBootServicesData` 可能对应一条静态保留声明）+ 对象存活区间；**不得只按类型决定释放事件**。第一版可全取 `Permanent`，但必须标注是项目选择。
- **B（静态表 + 显式损失声明）**：保留两张表，明确写「第一版不回收 BootServices 与 ACPI_RECLAIM，损失不计入可用表」。

**逐类型保留期限（不能简化成「只有 Runtime 必须保留」）**：

| 类型 | 规范规定的保留期限 |
|---|---|
| `RUNTIME_SERVICES_CODE/DATA` | 工作状态及 ACPI S1–S3 |
| `ACPI_NVS` | 工作状态及 ACPI S1–S3 |
| `PAL_CODE` | 工作状态及 ACPI S1–S4 |
| `ACPI_RECLAIM` | ACPI 启用之前 |
| `BOOT_SERVICES_CODE/DATA` | `ExitBootServices` 后可一般使用 |
| `LOADER_CODE/DATA` | `ExitBootServices` 后可用，但当前 loader 可能仍在使用 |
| `PERSISTENT` | 按 Conventional 方式使用，但数据非易失 |

**没有 ACPI 启用事件的 profile：`EfiACPIReclaimMemory` 视为 `Permanent`**：它的规范生命周期是「ACPI 启用前保留、启用后可回收」，而标准 EFI DTB 又必须放在这种内存里。若某个受支持 profile 从不进入 `AcpiEnabled`，就不能把 `Until(AcpiEnabled)` 当成已满足——否则一旦复制完 DTB 就会把原页交给 PMM。规则：只有存在明确的 ACPI 启用事件，才允许该类型转为可回收；否则在该 profile 中永久保留。
**`EfiReservedMemoryType` 的禁止语义分三层，且不依赖 DTB**：

| 层 | 主张 |
|---|---|
| UEFI 类型事实 | 内存类型表的可用性列对该类型写的就是字面 **「Not usable」**——项目不得把它改写成一个更像政策标识符的驼峰名，也不得写成「保留但仍可用」或「受保护内存」；配套事实是 `GetMemoryMap()` 侧区分两种情况：没有物理硬件支撑的地址空间不作为物理内存范围返回，有物理支撑但不应由 OS 访问的范围必须以该类型返回 |
| OS 政策语义 | `DenyOsUse`：不得作为 OS 可用内存，也不得让普通内核对象访问它；普通 PMM 分配与普通系统映射都归它派生 |
| 硬件兑现 | UEFI 类型本身**不建立页表、不配置 PMP、不保证 CPU 访问会硬件故障**；映射隔离与访问阻断由内核兑现。它不是 DTSpec `no-map`（后者另有标准映射限制、非受控推测访问限制与驱动受控使用例外） |

**三层各自的说法不得串层**：第一层只有「Not usable」这一个事实，它既不承诺硬件会拦访问，也不等于本项目的 `DenyOsUse`；第二层才是项目政策的具名主张；第三层是「类型本身不兑现保护，映射隔离与访问阻断由内核建立」。两个方向的越层都要防：把第一层写成「硬件禁止访问」是拔高，把第三层「类型本身不兑现保护」反过来读成「所以第一层其实没那么强」是压低——后者尤其危险，因为 `DenyOsUse` 的强度来自第二层政策，不依赖硬件是否拦得住。UEFI 原文在本环境抓取返回 403，「Not usable」措辞与 `GetMemoryMap()` 的两段区分都由**评审人按 UEFI 2.11 核对**，本方独立取证仍是缺口（登记在 §6 的事实分级段），不升级成本方已验证的引用。
**与 `no-map` 的驱动例外组合时，第一版选「绝对禁止」**：`EfiReservedMemoryType` 区域即使同时带 DTB `no-map`，也不开放 `DriverControlledUse`——**理由是 UEFI 的「OS 不应访问」政策与本项目第一版不开放受控例外的合同共同成立**；它不是「UEFI 已给出与 MRB 同强度的硬件禁令」。组合测试（`EfiReservedMemoryType` + `no-map` + 受控使用）列为待补；另一个可选边界是「一般用途禁止、专属驱动例外可用」，但第一版不选它，避免让单一类型名隐式决定组合语义。
**`UNUSABLE` 产生独立的类型禁止项**：第一版至少阻止普通映射与对象访问，不能只靠 `TypeAllocatablePages` 把它挡在 PMM 之外——否则一旦另有对象或设备映射授权，类型本身「不得使用」就缺少负向裁决。它与 `EfiReservedMemoryType` 的 `DenyOsUse` 强度相近但**来源身份必须分开**：前者是「物理内存中检测到错误的区域」，后者是「物理支撑存在但 UEFI 声明 OS 不应访问」。`TypePolicy` 必须能独立生成 `DenyOsUse` 及其派生主张，**不要求存在对应的 DTB `SourceRecord`**（x86_64 没有 DTB，不能因此丢掉 UEFI 自己的禁止语义）。两者都可能导致普通直接映射被拒，但来源语义与例外规则不同，不得合并成一个无差别 `DenyMapping`。同理，`UNUSABLE` 是域内排除（来源身份存在、类型资格排除），不与域外的 `RESERVED` 合并。
**`reusable` 不能落成 `Permanent`**：规范允许 OS 使用，但所属驱动必须能收回——它带所有者关系。候选 A 将来需要表达 `ReclaimableUnder(owner)`；第一版不实现回收，但不能把这个语义从模型里抹掉。**owner 不能从节点层级推导**：DTSpec 用设备节点的 `memory-region` phandle 引用 `/reserved-memory` 子节点，所有者关系必须来自这条引用（或其它权威平台信息），并记录引用身份；`ReservedReusable` 目前只有声明身份，没有可兑现的回收授权。
**第九十六轮裁决：第一版采用 B（静态处置表 + 明确的损失声明），不引入事件驱动的回收框架**。理由不是生命周期不重要，而是**今天没有任何回收消费者**：BootServices 区域第一版不回收、`ACPI_RECLAIM` 在没有 `AcpiEnabled` 事件时不回收、Runtime Services 第一版不提供、`reusable` 第一版不回收。此时搭 `Permanent / Until(event) / ReusableUnder(owner)` 的事件框架，等于为零个消费者维护状态机。A 方案保留为目标合同。**但有一条不许随 B 简化掉**：`ReservedReusable` 必须继续是独立类型——即使第一版不回收，也不能把它并进普通永久保留，否则将来根本不知道哪些区域原本带驱动回收语义。

**判据**：每种 UEFI 类型都有完整阶段处置（`Permanent` / `Until(event)` / `ReclaimableNow` / `Unsupported`，`Permanent` 必须注明是对本项目支持的运行阶段而言）；文档不得把项目策略写成规范语义。
**第一个消费者**：最终直接映射的构造。

### O-02a 恒等映射的授权完整性

**问题**：引导根无差别覆盖低 4 GiB（现状事实与代码位置见 §2.3）。每一页是否都有「允许映射」的来源授权，今天没有证明；更严重的是**已经建立的叶子与目标禁止政策直接冲突**——`no-map`、MRB、`EfiReservedMemoryType`、`UNUSABLE` 只要落在低 4 GiB 内就都已有叶子。这不是「审计还没写」，而是「审计要拒绝的东西已经在页表里」。
**失败形态**：一段没有来源授权的地址被恒等段建立叶子，没有任何集合记录它的存在；驱动或内核任意代码经 `VA = PA` 读到 `no-map` 资源；`UNUSABLE` 页被普通访问；恒等叶子带 `EXECUTABLE`，于是低地址里任何内容都可被取指。
**裁决：采用甲案（授权驱动建表）**，乙案（把宽映射具名为 `BootTransitionMapping`）**否决**。否决不靠保守倾向，而靠两条无法用命名解决的事实：今天没有任何撤销恒等叶子的路径，所以「临时」不成立；覆盖事实不因改名而改变，而 `no-map` 的豁免只能来自它自己允许的受控例外（O-03），不能来自映射的自称用途。第三个事实由本轮补上：普通叶子还带 `EXECUTABLE`，等于把「启动阶段需要少数代码可执行」放大成整个低 4 GiB 可读写可执行。**「必需叶子走授权、普通 RAM 继续走旧宽映射」这种半拉子形态同样被否决**——那等于让两个互相矛盾的建表政策并存，而实际页表不会因为旁边写了「未闭合」就变窄。
**甲案的建表输入是四类已裁决授权对象，不是「只遍历可用表」**——那仍然不够，因为启动确实需要访问不属于 `CONVENTIONAL` 的 loader 代码与其它存活对象，它们可以有**对象级**映射授权，但不能因此把整个固件保留描述符或整个低 4 GiB 映射进来：

| 授权来源 | 建表时如何处理 |
|---|---|
| 合法普通 RAM | 先过来源、阶段、缓存属性与架构准入，再按完整叶子覆盖建映射 |
| 内核映像 | 代码、只读数据、可写数据**分别**登记权限与生命周期，不是一张表 |
| 启动对象 | 按实际存活对象及其访问需求登记：loader 代码、栈、自举页表、启动模块、DTB 原始对象与副本等 |
| 设备窗口 | 独立消费 `DeviceWindowPolicy`，逐页验证授权与设备属性（O-12c） |
**实施顺序是四段依赖链，责任归属以 §7 为唯一口径**（第九十八轮修正：本表此前仍把第 2、3 段写成 I-29，与同一节后文的「五项归专项」及 §7「只交付第一段」三处互相矛盾）。**段名固定为 I29-1／BootRoot-2／BootRoot-3／Paging-4**，避免「第一阶段」这种一词两义的写法：

| 段 | 交付内容 | 责任 |
|---|---|---|
| **I29-1** 来源事实进入交接 | 每条声明的身份（`SourceRef { input_object, local_source }`）、原始范围、原始语义、快照身份；跨来源兼容性裁决；类型／保留／生命周期的原始政策含义 | **I-29** |
| **BootRoot-2** 引导根负向投影 | 把上述事实**编译**成 `BootstrapRootDenyProjection(tree, phase)` | BootEntry／映射政策专项 |
| **BootRoot-3** 必需对象正向授权 | 把对象事实**编译**成 `RequiredObjectAuthorizations`（`ObjectId + PA + VA + R/W/X + Tree + Phase + AccessPath`）与 `BootstrapRootAuthorizationPlan`、`EarlyExecutionClosure` | BootEntry／映射政策专项 |
| **Paging-4** 建表消费与验收 | 两个建表点消费计划、替换旧宽映射、遍历实际 PTE 验收 | 分页专项 |

**I-29 交付对象事实，不交付对象映射授权计划**——这条细分是上表的关键：某个启动对象的身份、占用范围、最后使用阶段、来源是 `EfiLoaderData`／内核映像／启动模块，属本项；把这些事实转换成带 `VA`、`R/W/X`、树与访问路径的授权项，属 BootRoot-3。

I29-1 的最小要求与 BootRoot-2 的输入清单各在其处定义（`SourceRef` 定型与记录进政策见 O-12g；`BootstrapRootDenyProjection` 消费的五类来源见 O-12a），此处不重复展开；两者共同的硬约束是：某种输入没有接入时**必须拒绝该启动形态**，不得假定没有这种政策，且负向投影**不是把四种来源压成一个布尔**。

**BootRoot-2／BootRoot-3 的公式与审计谓词已移交专项册**（第一百轮，逐字入 `01-design-note-hardware-admission-model.md`）：`BootstrapRootAuthorizationPlan`、`ObservedMappingAudit`、`EarlyExecutionClosure`、`RequiredObjectAuthorizations` 与「先审后用」都在那里展开。本项只保留一句边界：**I29-1 交付对象事实（身份、范围、最后使用阶段、来源类型），不交付对象映射授权计划**；把所有普通 RAM 的映射授权为空（`OrdinaryRamMappingAuthorizations = ∅`）是第一版策略，它不因下游需要而被放宽。

**九页交接 arena 是本阶段最关键的对象，登记字段如下**（它的物理上属于普通 RAM，但授权对象是这个有身份、有范围、有使用者与生命周期的具体对象，不是它所在的整段 RAM——通用 RAM 授权集合为空时，对象级授权的并集仍可非空）：

| 字段 | 今天已核实的事实（代码位置） | 第一阶段还缺什么 |
|---|---|---|
| 身份 | 交接 v1 的 arena，容量常量 `minix_boot::ARENA_CAP = 9 * 4096`（`os/libs/minix-boot/src/handoff_v1.rs`） | 由第一跳 `startup_objects` 记录标识（UEFI 腿的第 16 条，`SourceOrigin::StartupObject`）；最后使用点 = `arch_boot_resume_v1` 解码完成 |
| 物理范围 | 由 `alloc_arena()` 现分配恰好 `ARENA_CAP` 字节的 `LOADER_DATA`，且被约束在恒等界以下（`os/boot-shim/src/uefi_helpers.rs`），以 `x0`/`rbx` 递进内核 | 范围只在寄存器里传递，未进入任何来源记录 |
| 来源 | UEFI `EfiLoaderData` 类型分配（`ebs_map.rs` 的类型表把 `LOADER_DATA` 判为 `ReservedMapped`） | 没有对应的 `SourceRecord`，x86/aarch64 UEFI 腿记录本就未产出（O-12g） |
| 存活期 | 内核在根激活后立即 `patch_runtime_fields_in` 写入，之后 `arch_boot_resume_v1` 再读取 | 没有「最后一次使用」的登记，也没有释放点 |
| 访问需求 | 写者=内核 patch 阶段；读后续消费者=`arch_boot_resume_v1` | 未按使用点登记，无法证明范围与路径 |
| 映射要求 | 今天靠恒等段的宽叶子顺带覆盖，权限 RW+X | 需要 `VA = PA`、要 `W`、无执行需求时应 `NX`；具体以多大叶子兑现属分页侧专项 |
| 负向检查 | 无任何禁止项参与（§2.3） | 必须过 `BootstrapRootDenyProjection` |

**因此第一阶段的启动形态**是：`KernelImageAuthorizations ∪ EarlyExecutionClosure ∪ LiveStartupObjectAuthorizations ∪ ExplicitDeviceWindowAuthorizations`，再减去禁止投影与不兼容覆盖；其余普通 RAM 不建通用直接映射，`establish_boot_dm` 也只能追加上述已授权范围。若某个启动对象在该边界之前就要用到一块任意普通 RAM，就必须把它逐对象登记进来或调整执行顺序，不得让旧宽映射代替授权。
**建表失败必须分档处理**，不得再用「忽略错误」把「没有建立完整授权覆盖」延迟成之后的不可预测缺页：① 必需叶子（执行闭包、内核映像、启动对象、页表）映射失败 → **拒绝启动**；② 可选映射未建立 → 只有政策明确允许不建立时才继续；③ 其余一律拒绝。代码里已有正确先例：同一个函数中，高半内核映射用的是 `.expect("kernel map: map_huge failed — kernel image mismatch")`，而恒等循环用的是 `let _ =`（`map_huge` 返回 `Result<(), PageTableError>`；错误种类的处置属分页侧专项）。**甲案把循环输入换成已裁决的授权集合之后，「地址没有物理内存支撑」这一类可容忍失败的理由随之消失**，因此忽略错误的写法失去依据。
**判据**：每个实际叶子都能反查到一个授权来源（四类之一），且叶子覆盖的**每一页**都通过来源、权限、内存属性、共享性与禁止项检查——大页覆盖「有授权 + 空洞 + `no-map`」时不能只看起始页，必须拆分叶子或拒绝。授权记录至少含：PA 范围、VA 范围、用途、R/W/X、内存属性、共享性、树与阶段。
**x86 的叶子属性判据**：`EffectiveMemoryType = Resolve(PAT_Entry, MTRR_Type(physical_range))`，不是简单二元运算；大页覆盖 MTRR 类型不同的区间时不能按起始地址判定，必须拆分或拒绝。**所有逻辑处理器的 `IA32_PAT` 必须一致**（或逐核读回验证），不能只查 `CR0.CD/NW`；同一物理页的多个 VA 别名不得产生不兼容的有效内存类型。
**消费者有两个，缺一即边界不成立**：`build_bootstrap_root_and_enable` 的恒等循环，以及同一棵根上追加直接映射的 `establish_boot_dm`（§2.3）。第一阶段对实际 PTE 审计时，两处产出的叶子都要过同一条判据：有授权来源、不命中禁止投影、逐页权限与属性兼容。

### O-02b 普通保留声明是否产生映射禁令

**本项只保留来源语义**：普通 `/reserved-memory` 声明产生**分配排除**，不自动产生映射或访问禁令；`reusable` 带驱动回收责任、`no-map` 才带两项独立子语义（禁标准系统映射、禁非受控推测访问）。把三者编译成禁止集、以及「保留映射区权限」的判据属 BootRoot／分页专项。

### O-03 禁止项的来源分级

**本项交付分级事实，不交付禁止集合**：`no-map`、MRB、`EfiReservedMemoryType` 的强度与例外机制各自独立，来源记录必须分别保存身份与原始范围；把它们编成 `DenyStandardSystemMapping`／`DenyUncontrolledSpeculativeAccess`／`DenyOsUse` 并决定冲突优先级，属映射政策专项。第一版唯一固定的政策是：`DriverControlledUse` 的能力保留在模型里，**没有任何可通过的政策实例**。

### O-04 对象负向义务的投影

**对象级 `MustNotMap`／`MustNotAccess` 是下游合同**：本项只保证每个负向来源在交接清单里可追溯（对象、来源、原始范围、阶段），投影与审计属映射专项。

### O-05 冻结时点（已移交）

T0／T1／T2 的检查点划分、冻结字段与「先审后用」全部属 BootEntry／分页专项；本项只要求交接清单记录每个启动对象的**最后使用点**，供该专项决定何时可以冻结。逐字内容见 `03` 六十五节与专项册。

### O-06 `OrdinaryRamMappingEligibility` 的阶段化定义

**问题**：UEFI 分支不能拿来源域当 backing——它含类型排除、诊断成员与设备类描述符。
**失败形态**：一条 `RESERVED` 描述符被当作「完整 RAM 支撑」，某段保留声明因此通过 backing 检查并被映射成普通内存。
**候选 A（已裁定，待实现）**：每个来源记录回答三个问题——是否具备普通主存属性、当前阶段是否允许普通映射、当前阶段是否允许 PMM 分配。逐类型至少：

**一个字段只回答一个问题**（第一百轮把混在一个枚举里的维度拆开）：

| 字段 | 取值 | 回答什么 |
|---|---|---|
| `PhysicalBackingKind` | `VolatileRam`／`PersistentMemory`／`NonRamPhysicalResource`／`Unknown` | 物理存储性质 |
| `UefiMemoryType` | 描述符原始类型（`Conventional`／`BootServicesCode`／`ACPIReclaim`／`NVS`／`Unaccepted`／`Unusable`／`MMIO`…） | 固件给的类型标签 |
| `MemoryAcceptance` | `Accepted`／`Unaccepted`／`NotApplicable` | 是否已接受 |
| `MemoryUsability` | `Usable`／`Unusable`／`NotApplicable` | 健康／可用性 |
| `PolicySupport` | `Supported`／`Unsupported` | 本项目是否支持该输入 |
| 处置 | `AvailableNow`／`ExcludedPermanently`／`RejectBoot` | **第一版**给出什么结论 |

反例之所以重要：`EfiACPIReclaimMemory` 的物理性质仍是易失 RAM，`ACPI reclaim` 是类型与生命周期语义，不能取代物理性质；`Unsupported` 是项目政策，不是内存性质。

| 类型 | 属 `RamSourceDomain` | `PhysicalBackingKind` | 类型允许普通映射 | 类型允许普通分配 | 规范生命周期 → **第一版处置** |
|---|---|---|---|---|---|
| `CONVENTIONAL` | 是 | `VolatileRam` | 是（缓存属性另行判定） | 是（阶段／属性／排除另行判定） | 无事件 → **`AvailableNow`** |
| `LOADER_CODE` | 是 | `VolatileRam` | 是（存活期需 X） | 否 | `ExitBootServices` → **`ExcludedPermanently`** |
| `LOADER_DATA` | 是 | `VolatileRam` | 是（默认 NX） | 否 | 同上 |
| `BOOT_SERVICES_CODE/DATA` | 是 | `VolatileRam` | 是（固件活跃期） | 否 | 规范：`ExitBootServices` 后可通用 → **第一版 `ExcludedPermanently`** |
| `ACPI_RECLAIM` | 是 | `VolatileRam`（reclaim 是类型语义，不是物理性质） | 是 | 否 | 规范：ACPI 启用后可回收 → **第一版 `ExcludedPermanently`** |
| `ACPI_NVS` | 是 | `VolatileRam` | 否 | 否 | 无 → `ExcludedPermanently` |
| `PERSISTENT` | 是 | `PersistentMemory` | 否（第一版普通 volatile 策略） | 否 | 无 → `ExcludedPermanently` |
| `RUNTIME_SERVICES_CODE/DATA` | 是 | `VolatileRam` | 否 | 否 | 第一版永久排除 → `ExcludedPermanently` |
| `RESERVED` | 否（采用 B） | `VolatileRam`（有平台证据时） | 否 | 否 | 无 → `ExcludedPermanently` |
| `UNUSABLE` | 是 | `VolatileRam` | 否 | 否 | 无（`Usability = Unusable`）→ `ExcludedPermanently` |
| `UNACCEPTED` | 是 | `VolatileRam` | 否 | 否 | **`RejectBoot`**（不是页排除） |
| `PAL_CODE` | 不支持 | `Unknown` | — | — | **`RejectBoot`** |
| 类型 11 / 12 | 否 | `NonRamPhysicalResource` | 否 | 否 | 无 → 设备路径（见 O-12c 移交说明） |

**「规范生命周期」与「第一版处置」必须分两段读**：`BOOT_SERVICES_*` 在规范里 `ExitBootServices` 之后可作一般用途、`ACPI_RECLAIM` 在 ACPI 启用后可回收，这是**规范事实**；而 O-01 已裁定第一版采用静态处置表、没有任何回收消费者，所以**第一版处置一律 `Permanent`**。把两者写成一句「退出前否／退出后是」会被误读成第一版真的会在该事件后回收。

**列名为什么不叫「普通主存性质」**（第九十八轮按评审人的拆分重写）：

| 列 | 它回答的问题 | 反例 |
|---|---|---|
| 属 `RamSourceDomain` | 权威来源是否声称这段是物理主存 | `UNUSABLE` 属域内但类型禁止使用 |
| `MemoryKind` | 易失 RAM／Persistent／NVS／Unaccepted／Unusable 等**物理性质** | `PERSISTENT` 有 backing，但不是第一版普通易失 PMM |
| 类型允许普通映射 | 类型政策本身是否允许把它作为普通内存映射 | `RUNTIME_SERVICES_*` 第一版永久排除 |
| 类型允许普通分配 | 类型政策本身是否允许进普通 PMM | `ACPI_NVS` 第一版不许普通分配 |
| 生命周期 | 哪个事件之后可能改变处置 | `UNACCEPTED` 需要先被接受 |


**`CONVENTIONAL` 是有条件的**：它可以构成 `RamSourceDomain` 与普通主存资格，但 `OrdinaryRamMappingEligibility`（类型层面）**不隐含缓存属性检查**；最终普通映射 = 类型资格 ∧ 缓存属性资格（第一版要求 WB），最终 PMM 分配 = 阶段资格 ∧ 缓存资格 ∧ 全部排除项。无 WB 的描述符不进入普通可分配集合，但**保留其来源身份**。
**术语边界**：这里读的是 `GetMemoryMap()` 的 `EFI_MEMORY_DESCRIPTOR.Attribute`（能力位掩码），与 `EFI Memory Attributes Table` 反映的保护配置不是一回事；第一版不解析后者，也不宣称已获知固件实际施加的全部权限。
**`EfiReservedMemoryType` 在域外但来源身份仍有效**：它不进入 `RamSourceDomain`，因此不参与 `PmmBaseAvailable`/`PmmBaseExcluded` 的域内分割；但与其重叠的 DTB `/reserved-memory`、MRB 原始记录仍然保留，并可独立产生映射禁止与访问禁止。没有独立平台证据时，不把它重新解释为普通主存。
**UEFI Runtime Services 的支持边界（明确声明）**：第一版**不调用 `SetVirtualAddressMap()`、不提供 UEFI Runtime Services**；`RUNTIME` 属性与 `RUNTIME_SERVICES_*` 类型只保留诊断与来源信息，不产生任何普通或运行时映射义务。将来支持时，必须**单独增加**运行时对象与 MMIO 映射义务，不得混入普通直接映射授权。
**属性位编码存在两套官方取值，政策后果如下**（逐位矩阵、版本读数与两组编码对照见 `03` 六十二节）：

- **`0x1_F000` 整个冲突位域无条件拒绝（已实现）**：`WP`/`RP`/`XP`/`NV`/`MORE_RELIABLE` 在 UEFI 2.10 原始版与 2.10 Errata A／2.11 PDF 里位值不同，任一读法都可能把必须排除出 PMM 的内存伪装成元数据，因此在已知位闸之前单独拒绝（`PolicyError::AmbiguousAttribute`，负控 `every_contested_attribute_bit_fails_closed`、`the_ambiguous_bit_fails_closed_under_either_name`）。
- **高位 `RO`/`SP`/`CPU_CRYPTO` 改为 profile 门控（已实现）**：这两组位值在两个正式发布版本族里就不同，内核无法从 `uefi-raw` 常量判断启动固件用哪一版，故 profile 未钉住前直接拒绝（`PolicyError::ProfileAttribute`，测试 `profile_only_attributes_fail_closed_without_a_pinned_profile`）。
- **代价与恢复路径**：`NV` 位因此也落进拒绝域——模型认识 NV 语义，但当前分类器不接受带 NV 位的输入，该分支在真实输入上**不可达**；恢复需两步（先钉带版本证据的固件编码 profile，再把无条件拒绝改成按 profile 解释），只加 profile 记录解不开。`SPECIAL_PURPOSE` 同理不可达。
- **`UefiMemoryAttributeProfile` 的钉法**：必须绑定**实际启动固件镜像身份**（x86 与 aarch64 分别记录，两组固件不必同源），并**同时钉住低位组与高位组的完整映射**；只授权三个高位会得到「部分可识别、其余永久拒绝」的半套 profile。升级 `uefi-raw` 时必须**重核全部已识别属性位**。EDK2/Linux 的具体 commit/tag 尚未钉死（`main` 会漂移），`HOT_PLUGGABLE` 在 `uefi-raw 0.9` 无常量、走未知位闸。

**候选 B（不可接受，列出以说明为何否）**：只看平台物理内存描述、不看类型分类——会把不可用内存当成 backing。
**判据**：每个 backing 页能指回一个「普通主存 = 是」的来源记录，且阶段允许。
**PAT/MTRR、MAIR、PMA、PMP 与逐 CPU 的物理访问准入不在本项**：它们决定「这台机器允许这次访问吗」，属硬件准入，已随 §1.6 的重规划入 `01-design-note-hardware-admission-model.md`。本项只回答「这段范围的来源事实、类型与生命周期是什么」。**名字也要拆开**：`OrdinaryRamMappingEligibility` 把来源分类与映射决定重新绑在一起，本文保留该名字只用于**类型层面**的映射资格，最终映射资格还需叠加缓存属性与硬件准入（专项侧）。

**边界**：`OrdinaryRamMappingEligibility` 只描述来源与 OS 政策资格，**不证明某架构实际能访问该区间**——x86 还要 PAT/MTRR 有效类型、AArch64 还要 MAIR 与逐 PE 配置、RISC-V 还要 PMA 能力与逐 hart 的 PMP。硬件准入不得被 `ordinary_main_memory = true` 代替。
**`LOADER_CODE` 不是一律 R/NX**：只要它仍是当前执行中的 loader 代码，就需要可执行映射；可执行权限来自仍存活的 loader 对象义务，不因「被分类为保留内存」自动变成 R/NX。最后一次执行点之后才撤销执行映射或按生命周期处置——这也是对象存活区间必须参与映射权限计算的原因。
**第一个消费者**：R4b-c3 检查器。

### O-07 实际访问需求与完整性来源（已移交）

对象访问点清单、实例展开与完整性证明属驱动访问审计专项；本项只保证启动对象在交接清单里有身份与存活期。

### O-08 端口 I/O 的地址域（已裁定：采用 A）

原始端口操作只允许出现在逐个列举的包装函数、调用点带稳定 `AccessUseId`、CI 静态审计加两枚负控——属驱动访问审计专项。本项只要求 UEFI 类型 12 的地址域记成 `UefiMmioPortSpaceAddress`，不得与 x86 端口范围共用类型。

### O-09 诊断成员与样本的存储登记（已裁定：采用 A）

完整诊断成员表与操作者可见的有界样本表各自登记存储范围、容量、写入者、读取者、最后使用阶段，防止切根后继续引用已失效的引导期缓冲。**判据**：容量清单里每一项都能在对象目录里找到位置与生命周期。**消费者**：容量清单。

### O-10 R13-A 的第二枚负控（已裁定：采用 A）

负控细节属映射／访问专项；本项保留「每个禁止项都必须有可追溯来源」这一条。

### O-11 配置表 DTB 的首次访问授权
**问题**：四条启动入口都会收到一个「启动程序提供的 DTB 地址」，随后读 header、解析 MRB。若 DTB 本体落在 MRB 内，读取发生在「知道它是 MRB」之前。

| 输入路径 | 初始授权来源 |
|---|---|
| aarch64 UEFI | UEFI 配置表中的 DTB 指针 |
| aarch64 直启 | 架构启动 ABI 传入的 DTB 指针 |
| riscv64 自引导 | OpenSBI / 启动 ABI 传入的 DTB 指针 |
| riscv64 U-Boot | U-Boot 启动 ABI 传入的 DTB 指针 |

授权必须是启动输入合同，而不是只给 UEFI 配置表路径特批。
**失败形态**：固件把 DTB 放在 MRB 内；内核在知道它是 MRB 之前已经读了它。
**候选**：

- **A（两步授权，第九十五轮裁定为「具名一次性启动读取合同」）**：① 对应启动 ABI 提供的 DTB 指针授权对**固定 header 区域**的一次性只读访问（授权主体与传递方式按每条入口分别定义）；② 校验 magic、版本、`totalsize`、回绕与范围合法性后，才授权对 `[dtb, dtb + totalsize)` 的完整读取，并复制到自有缓冲。授权范围与失效点写进启动输入合同。
- **B**：不承认该授权，则必须定义不读取原始地址也能取得 DTB 的其他机制（例如固件提供副本），否则该布局拒绝启动。

**配置表 DTB 存在却没被解析时怎么办（第九十六轮裁定，不留给实现者猜）**：

| 配置表 DTB 状态 | 第一版行为 |
|---|---|
| 不存在 | 允许仅按 UEFI 内存图启动；**不回退**到磁盘镜像 DTB（那不是内存政策输入） |
| 存在且已完整解析并与内存图交叉验证 | 按两份权威来源共同处理 |
| 存在但尚未接入，或解析失败 | **拒绝启动**，不得静默丢弃其中的 `no-map`／MRB 声明 |

**三态已落码（第一百零七轮）**：`uefi_helpers::cross_validate_config_dtb` 在两个 UEFI 入口各调一次——无 DTB 走第一行；有且可解析则要求 `/memory` 覆盖每条 conventional 跨度（否则 halt）；容器坏、拒图、解析失败都 halt。**复制到自有缓冲再解析已落码，严格两快照（第一百二十五轮）**：`find_platform_sources` 只记录配置表地址；预检快照由 `ebs_pair::preflight_dtb` 取得，`dtb_copy::preflight` 在它的授权下验证 `[dtb, dtb+40)` 的页类型、读一次 40 字节 header、按 `totalsize` 分配副本；配对循环自己的 `GetMemoryMap` 是最终快照，`authorize_and_copy` 在它上面复核全范围页类型、写入 header、只复制 body 一次，之后到 `ExitBootServices` 之间不再分配。AAVMF 的强制 DT 模式把配置表 DTB 放在非 `EfiACPIReclaimMemory` 页，因此该腿断言「存在但不可用」的拒绝，且拒绝发生在预检阶段、header 读取之前（串口 `DTB preflight failed: DtbPages(NotAcpiReclaim { page: … })`）。

**另需裁定一致性策略**：配置表 DTB 的保留声明与 UEFI 内存图之间——(A) 解析权威 DTB，保留来源身份并与内存图交叉验证；(B) 只信内存图，接受来源身份丢失并对无法区分的区域取更保守的统一政策。第一版倾向前者，但 B 也可以是明确的简化，不是规范要求。
**四条 DTB 输入路径统一两阶段**（x86_64 UEFI 没有 DTB 输入，不属于这四条）：

- **两个独立前置条件，缺一不可**（不能合成一个「授权」）：
  - **`BootInputProvenance`**：源授权——哪个启动 ABI 把这个对象交给内核、授权读哪些字节、哪个阶段读、何时失效。启动 ABI 递指针建立的是这一条。
  - **`BootInputReachability`**：可达性——给定 PA/VA、当前地址转换状态与页表，`[dtb, dtb+40)` 这 40 字节**整段**确实可读（起点对齐不保证不跨 4 KiB 页）。
  - 两者不能互推：有指针不等于可解引用；有可读映射不等于这个地址就是启动程序授权的 DTB。
- **`ValidatedObjectReadAuthorization`**：校验 magic/版本/`totalsize`/回绕/范围后，授权 `[dtb, dtb+totalsize)` 整段只读并复制；之后政策代码只消费副本。
- **对象身份在首读前建立**：`BootInputObjectId` 由启动 ABI 交付时即分配，不等到 `totalsize` 才知道——首读前已经存在一个具 40 字节读取授权的对象。同一身份随阶段扩充范围：

```
BootInputObject { id, provenance, initial_read_range /*40B*/, validated_full_range, owned_copy_range, lifecycle }
```

header 读取、完整对象授权、自有副本、生命周期记录都继承同一 ID，避免出现「header 是对象 A、完整 DTB 是对象 B、副本是对象 C」只能靠地址猜测关联。
**全范围可达是第二个独立检查**：拿到 `totalsize` 后必须**重新证明** `[dtb, dtb + totalsize)` 在当前转换下整段可读（覆盖所有跨页、地址转换与末端），不能拿 header 的 40 字节证明代替；复制只能在这次证明之后开始。
**复制目标不得与源重叠**：原始 DTB 不一定在 MRB 里，也未必已被分配器排除。分配自有副本前，先把原始 DTB 的完整范围登记为**存活输入对象**；源与目标范围不相交才允许复制（DTSpec 要求客户端在 DTB 使用完之前不得覆盖它，即使它不在保留区内）。于是 O-11 有四个互相独立的条件：**来源授权、地址可达、内容一致（不得并发修改）、自有副本**。
**`OwnedCopyValidation`（第三段，不能省）**：`fdt_layout::validate` 并非只读 40 字节 header——它还要扫 MRB、结构块、属性长度与字符串名称。因此完整容器校验**必须在自有副本上执行**：原始 DTB 的 **header 只在 preflight 阶段读一次**，**body `[dtb+40, dtb+totalsize)` 只在复制阶段读一次**；两部分都进入自有副本后，校验与解析只访问副本，不再访问原始地址。三段的顺序是：provenance + reachability（`[dtb, dtb+40)` 整段）→ header preflight → 完整读取授权 + 对齐副本 → 在副本上 `validate` → 解析 → `collect`。
**副本必须是一份一致快照（复制算法写死）**：`totalsize` 本身来自 header，若「先读 40 字节、再整段复制」就会把 header 读两次。算法：

1. 一次读 40 字节到局部 header snapshot；
2. 只从 snapshot 校验 magic/版本/`totalsize` 上限与加法；
3. 分配对齐的自有缓冲区；
4. 把 snapshot 写入副本前 40 字节，**只从原地址复制 `[dtb+40, dtb+totalsize)`**；
5. 在副本上执行完整 `fdt_layout::validate`。
**ABI 地址、源物理范围与读取 VA 是三个概念**：`BootInputAddress`（ABI 递交的地址及其地址域——四条路径实际交付的都是**物理地址**：AArch64 `x0`、UEFI 配置表 DTB、RISC-V `a1`/`satp=0`）、`SourcePhysicalRange`（DTB 对象实际占用的物理范围）、`ReadVirtualRange`（当前阶段实际用于读取的 VA 范围）。不构造概括性的 `SourceVirtualRange`——那会把物理地址误包装成虚拟地址。`LiveInputReservation = Outward(SourcePhysicalRanges)`——**PMM 排除的是物理页，不是虚拟区间**；目标副本与源的不重叠也必须**按物理地址**判断：源 VA 与目标 VA 数值不同不代表不落在同一 PA（直接映射、恒等映射等别名都可能重合）。
**统一物理范围类型 `PhysicalRange { start: u64, end_exclusive: u128 }`**（`start < 2^64`、`start < end_exclusive ≤ 2^64`）：DTB 的 `address + size` 与 UEFI 描述符都能表达 `[0xffff_ffff_ffff_f000, 2^64)`，统一来源记录不得因 Rust 字段类型丢掉这个合法边界。原始输入字段保留各自编码（DTB 的 address+size、UEFI 的 start+pages），规范化层才转成 `PhysicalRange`；`UnsupportedPhysicalAddressEnd` 只在项目明确选择拒绝该边界时使用，**不能成为 `u64` 加法的副作用**。这条与 O-12h 结构校验、O-12i `FullPages` 是同一条边界合同；DTB 腿已按此实现（分名规则见 §2.1 与 O-12i），UEFI 侧的 `SourceRecord` 尚未接入。
**顶端那一页：集合层能表示，第一版仍拒绝，而拒绝是政策不是算术**。`FullPages` 的页编号范围是 `0 ≤ n < 2^52`，`n = 2^52 − 1` 的独占末端恰好是 `2^64`，所以纯集合层完全能表示最后一个 4 KiB 页；今天 `declared_page_range` 在投影末端到达 `2^64` 时返回 `UnsupportedPhysicalAddressEnd` 是**实现处置**。第一版的支持边界写明为：允许集合层表示该页，能否进入 PMM 由平台物理地址准入决定，而投影侧在拿到逐平台物理地址上界证据之前一律拒绝——理由是「该端点在本项目支持的任何一个 profile 上都没有真实 RAM 或设备窗口落在其上，接受它等于接受一条无法验证的路径」。错误码不得再被读成「`u64` 无法表达合法范围」。
**副本与源的相交检查绑定实际分配结果**：按 PA 判断不重叠之后，仍须在分配器返回副本物理地址后，用**实际分配范围**再作一次相交检查。「分配器通常不会从 ACPI reclaim 区分配」不是证据——分配资格与源对象的生命周期登记是两份独立证据。
**`BootInputReachability` 是端到端可读性，不是「PTE 存在」**：在指定阶段与访问路径下，完整对象的只读访问所需的地址翻译、页表访问、物理访问保护与平台准入条件必须全部成立——RISC-V 上 PMP 可能拒绝 S 模式访问、PMA 还可能限制访问方式；AArch64/x86 各有对应门控。具体由各架构实现验证，通用层只表达「可读」语义，不暴露 PMP/页表遍历等机制名。
**输入合同**：启动程序交接后、复制完成前，DTB 剩余字节不得被并发修改（固件或其它代理都不写）。这条不能证明时必须定义一致性检测或拒绝策略——否则副本可能是不同时刻拼出的混合状态。今天代码没有复制，自然也不做这项检测（见 §2.1 的「就地消费」）。
**UEFI ↔ DTB 交叉验证是覆盖合同，不是重叠检查**：静态 `/reserved-memory` 与 MRB 在 UEFI 启动时须出现在 `GetMemoryMap()` 中，且类型须匹配（`no-map`/MRB → `EfiReservedMemoryType`，普通静态 → `EfiBootServicesData`；MRB 在规范里是 `should`，本项目可提升为强制并写明是项目策略）。验证式：

```
Outward(R_reserved) ⊆ ⋃ { 类型兼容的 EFI 描述符覆盖的页 }
```

**权威表一：逐页 EFI 类型主张裁决**（本表是该主题的唯一裁决来源，O-06 与 O-11 其余各处只引用它、不再另立结论）。输入是 `PageSourceClaims(page) = { (SourceRef, RequiredEfiType, 规范强度) }`，裁决函数 `ResolvedEfiType(page) = ResolveCompatibleClaims(...)`；**「同页」一律指 `Outward(原始范围)` 相交，不是原始字节相交**。

| 该页上的主张组合 | 要求的 EFI 类型 | 结果 |
|---|---|---|
| 普通静态保留（`ReservedOrdinary`） | `EfiBootServicesData`（规范 `must`） | 兼容 |
| `ReservedReusable` | 同上——它是非 `no-map` 的静态保留 | 兼容；生命周期与 owner 关系另行裁决，不改变类型要求 |
| `no-map` | `EfiReservedMemoryType`（规范 `must`） | 兼容 |
| MRB | `EfiReservedMemoryType`（规范 `should`，本项目升为强制） | 兼容（按项目强化） |
| `no-map` + MRB | `EfiReservedMemoryType` | 兼容 |
| 普通 / `reusable` + MRB | BootServicesData(`must`) vs Reserved(项目强化) | **冲突 → 拒绝启动**：一条描述符无法同时满足两者 |
| 普通 / `reusable` + `no-map`（原始字节不相交但同页） | BootServicesData vs Reserved | **冲突 → 拒绝启动**：两条声明各自向外量化到同一 4 KiB 页，原始范围不重叠不豁免页级冲突 |
| 普通静态 + `no-map`（原始字节重叠） | — | DTB 侧已是冲突（同范围异语义），见 §2.1 |
| 仅 EFI DTB 对象 | `EfiACPIReclaimMemory`（UEFI 2.11 §4.6.1.3 对标准 `EFI_DTB_TABLE_GUID` 的专属要求） | 兼容 |
| EFI DTB + MRB（**原始字节相交**） | `EfiACPIReclaimMemory`：DTB 的 `must` 压过 MRB 的 `should` | 允许首读，但必须走具名 `BootInputReadUse`；MRB 的来源身份、访问禁止与 PMM 排除**不因该页取 ACPIReclaim 而消失** |
| EFI DTB + MRB（**只有向外量化后同页**） | 同上 | 首读**不属于 MRB 原始字节访问**；若第一版仍要求授权，必须标成项目额外施加的**页级**启动读取限制，不得说成 MRB 规则 |
| EFI DTB + 普通 / `reusable` 静态保留 | `must` vs `must` | **冲突 → 拒绝启动**：静态保留的类型对应是规范要求，不能被另一条 `must` 静默覆盖 |
| EFI DTB + `no-map` | `must` vs `must` | **冲突 → 拒绝启动**；第一版**不区分原始字节是否真的不相交** |
| EFI DTB + MRB + `no-map` | — | **冲突 → 拒绝启动**：MRB 例外只解决 DTB 与 MRB 的类型冲突，不解除 `no-map` 的类型主张，两条例外不能相加抵消 |
| EFI DTB + 动态 `size` 保留 | 动态保留按规范不入内存图 | 拒绝：`UnsupportedDynamicReservation`（与「DTB 必须在图内」不能同时成立） |
| 其它 UEFI 类型 × 任意静态保留 | 与预期类型不符 | 拒绝启动，除非有明确列出的项目扩展规则 |

**为什么不是「Reserved 更强所以覆盖」**：DTSpec 对普通静态保留指定的是类型本身，不只是「受保护」；用 Reserved 覆盖等于悄悄牺牲一条来源要求。若将来要接受普通保留 + MRB，唯一自洽的做法是**放弃 MRB 的 `should`→must 强化**（回到规范级别，MRB 的访问禁止由独立政策兑现），而不是让 `ResolveCompatibleClaims` 返回一个隐藏牺牲项的结果。跨来源身份照旧各留一条；覆盖公式分别定义 `R_static` 与 `R_MRB`，经页级裁决后再比对。表中的「同页冲突即拒绝」是**保守规则**，它会拒绝掉原始字节其实不相交的合法配置，换来的是不必证明「固件给出的页内字节布局恰好不重叠」——第一版接受这个代价。
**粒度只有两条，不得互相扩张（这是对上表的解释，不是第三份裁决）**：EFI 类型裁决按 **4 KiB 页**向外量化——部分覆盖即构成该页的类型主张，不能因为原始字节只占半页就把同页另一半当成「无主张」；MRB 的访问禁止与受限授权按声明的**原始字节** `[start, start+len)`，**不取整**——不能因交集页取了 `EfiACPIReclaimMemory` 就把字节授权扩成整页可读，也不能因某页类型合规就允许越出原始字节的访问。`no-map` 与普通保留的**分配排除**按页向外，但访问禁止的**强度**仍来自各自来源语义，不因取整而改变。DTB 首读范围本身是原始字节（header 40 B、body `totalsize`），只有**类型验证按页**。页级类型裁决成立**不**意味着该页全部字节可访问；反之亦然。
**交叉验证是覆盖合同，不是重叠检查**（验证式 `Outward(R_reserved) ⊆ ⋃ { 类型兼容的 EFI 描述符覆盖的页 }`）：只证明「有重叠」或「首类正确」不够——一个正确的首页描述符 + 其后的 `EfiConventionalMemory` 会让保留区大部分页未被保护；反向也不成立，一个兼容描述符只覆盖部分不能把整条来源标记为已验证。**必测三种形态（已实现：`first_page_correct_and_later_page_wrong_is_rejected`、`consecutive_compatible_descriptors_cover_the_claim`、`incompatible_middle_page_is_reported_with_its_type`）**：① 仅首页类型正确、后续页类型错误 → 拒绝；② 多个连续同类描述符共同完整覆盖 → 通过；③ 中间一页类型不兼容 → 拒绝。MRB 与普通声明重叠时，交叉验证只核对覆盖与兼容性，政策层保留两个独立来源身份，不得合并成一个更大区间后丢掉语义差异。规范强度照旧分开记录：静态 `/reserved-memory` 入图与类型对应是明确要求；MRB 入图是明确要求，但用 `EfiReservedMemoryType` 的措辞是 `should`（项目可提升为严格合同，不得写成 must）。`reusable` 与 `no-map` 互斥、MRB 之间不得重叠（规范 *shall*）两种强度不因本表改变。
**权威表二：DTB 一次性首读授权**（启动读取是一个具名用途 `BootInputReadUse { ObjectId, PA/VA, 读取范围, 只读, 阶段, 访问路径, 失效点 }`，不是全局正向授权；每个负向来源分别判断是否允许它，`BootInputProvenance` 不能覆盖所有禁止项）。标准 DTB 表有专属规则，不能套用一般配置表允许的四种类型：`RuntimeServicesData`/`BootServicesData`/`ACPINVS` 对标准 DTB 表不成立，`LoaderData` 更不在其中。

| 原始 DTB 所在区域 | 一次性启动读取的裁决 | 授权来源与失效点 |
|---|---|---|
| 普通 RAM | 允许 | 启动 ABI 授权 + 完整范围端到端可达 |
| 普通 `/reserved-memory` | 允许 | 由启动输入对象授权决定；**分配排除仍然保留** |
| MRB | 允许，但走具名受限授权 | MRB 规则自身允许的、启动程序明确用途对应的一次性访问授权（带对象身份、阶段与只读路径）；授权不覆盖所在 MRB 区域的其它访问，也不授权复制完成后继续访问原地址 |
| `no-map` | **首读允许、解析后发现冲突即拒绝继续启动** | DTSpec 的例外是「由使用该区域的设备驱动控制」的访问，**首读本身允许**（走 O-03 具名 `BootInputReadUse`）；此处拒绝的是「把它说成规范定义的驱动受控例外」。两个决定必须分开：**首读是否允许**属启动输入授权，**解析后发现源对象落在 `no-map` 里是否允许启动继续**属解析后的支持政策；除非实现并验证专用受控路径 |
| `EfiReservedMemoryType` | **第一版拒绝** | 配置表里的指针只建立来源身份，不能推翻内存图对该范围的禁止处置；除非有专门且明确的启动 ABI 例外与规范依据 |
| `EfiACPIReclaimMemory` | 允许（唯一合规存放类型） | 整段 DTB 对象所在描述符必须是该类型才允许首读；ACPI 启用前的保留处置不变 |
| `EfiUnusableMemory` / `EfiUnacceptedMemoryType` / MMIO / 端口空间 / `PAL_CODE` | 拒绝，或走各自独立准入规则 | 不得借用普通 DTB 读取通道 |

**类型验证分两个时点**（读完 header 之前并不知道 `totalsize`）：① **首读前**用 ABI 给出的物理起始地址验证 `[dtb, dtb+40)` 覆盖的全部页都落在 `EfiACPIReclaimMemory`，并完成 `BootInputProvenance` 与端到端 `BootInputReachability`；② **header preflight 后**由 snapshot 得到 `totalsize`，验证 `Outward([dtb, dtb+totalsize))` 的**全部页**都由该类型描述符完整覆盖（可多个连续同类型共同覆盖）；覆盖不完整、类型不一致或中途落入其它类型 → 立即拒绝，**不得继续复制 body**。**必测已落码**（`dtb_copy` 14 条单测）：`all_pages_acpi_reclaim_passes`、`a_reserved_page_is_not_acpi_reclaim`、`an_uncovered_page_is_rejected`、`a_conflicting_type_beside_acpi_reclaim_is_rejected`、`the_range_is_page_rounded_outward`、`a_too_small_header_is_refused`，以及两快照时序的 `preflight_authorizes_the_header_then_the_final_map_copies_once`、`a_final_map_type_change_stops_before_the_body_copy`、`the_final_map_may_segment_the_same_range_differently`、`the_header_is_read_once_and_the_body_once`、`the_final_map_phase_allocates_nothing`、`the_copy_is_reused_when_the_final_map_is_read_again`、`copy_without_preflight_is_fail_closed` 与 `an_absent_dtb_needs_no_copy`；配对层由 `ebs_pair` 的 `dtb_preflight_reads_one_map_before_the_paired_final_map`、`preflight_buffer_too_small_stops_before_the_header_read`、`a_retried_ebs_takes_a_new_snapshot_and_the_copy_survives` 钉住。
**这两张表的射程**：它们**只**决定「该页的 EFI 类型主张是什么」与「一次性只读能否发生」，**不**修改 `RamSourceDomain`、`PmmBaseAvailable` 或 `OrdinaryRamMappingEligibility`。
**实现状态：已实现（UEFI 腿）**。三段的代码位置见 §2.2 的「复制到自有缓冲」段：`note` → `preflight` → `authorize_and_copy`；三条 DTB 直启腿在各自映像内本来就持有副本，不存在「先读固件对象、再复制」的窗口。`ebs_pair::preflight_dtb` 只在记录了配置表地址时取预检快照，无 DTB 的 x86_64 腿没有额外调用。
**副本生命周期**：原始 DTB 只在复制期间可读；自有副本登记进对象目录；切根前不得回收或覆盖；副本的最终释放必须有明确消费者与阶段。边界条件：header 固定读取长度写明；`totalsize` 有项目上限；`dtb + totalsize` 必须 checked；完整范围必须证明可安全读取（不能只验证起始地址）。
**MRB 与 EFI DTB 的页集合差分**（上表第一行的例外只作用于交集页，不是整条 MRB）：

```
DtbPages = Outward(EfiDtbObjectPhysicalRange)
MrbPages = Outward(MrbPhysicalRange)

MrbPages \ DtbPages  → EfiReservedMemoryType
MrbPages ∩ DtbPages  → EfiACPIReclaimMemory
```


交集页上的 MRB 来源身份、访问禁令与 PMM 排除仍然存在，只是该页的 EFI 类型要求服从 DTB 专属 `must`。因此一条多页 MRB 若只与 DTB 相交其中一页，描述符必须能按页分段：其余页仍按项目强化规则取 `EfiReservedMemoryType`。**规则已落码**：`resv_claims::check` 按**每条主张页**判定，DTB 自身范围以外的页不接受 `EfiACPIReclaimMemory`（`dtb_pages_accept_acpi_reclaim_for_the_reservation_type` 覆盖「DTB 只占该页才接受、否则拒绝」）。**仍缺专门用例**：多页 MRB 且 DTB 只落在中间页的分段场景——当前单页版证明规则，多页只是同一循环的重复。
**判据**：该腿能指出保留声明的实际读取点；若 DTB 在 MRB 内，能指出授权依据（候选 A）或拒绝依据（候选 B）。
**第一个消费者**：aarch64 UEFI 腿的保留政策。

### O-12 集合构造与来源完整性（总括，拆成可独立签字的子合同）

**问题**：禁止映射集合、禁止访问集合、设备窗口集合、PMM 累积器今天的构造式、消费者与容量都没有定义；它们来源不同，不应捆绑成一个裁决。

- **O-12a** 引导根禁止映射集合：每条禁止项可追溯到一个来源（`no-map` / MRB / 类型 / 对象）。
- **O-12b** 最终根禁止映射集合：同上，且包含对象负向义务的投影。
- **O-12c** 设备窗口授权：见下节；判据是**每页都能指回一条有效的设备映射政策授权**。
- **O-12d** PMM 排除累积器：与可用表互补（对来源域）；每来源贡献一次。
- **O-12e** 容量与失败形状：超限拒绝启动；两遍扫描；每份输出独立容量。
- **O-12f** `RamSourceDomain` 与 `OrdinaryRamMappingEligibility` 分开定义（四条入口各一组规则）。
- **O-12g** 采集器重构：保留原始来源记录，再派生投影。
- **O-12h** UEFI 描述符结构校验与政策分类分开（见下）。
- **O-12i** 每种政策投影的页粒度规则（见下）；其**后半节**（普通访问准入、页表用途、原子能力、对象访问三层展开、整页全称判定）已在第九十六轮迁到 `01-design-note-hardware-admission-model.md`。
- **O-12j** 三层：**a 容器与 MRB（已实现）**——magic、`totalsize`（≤64 MiB）、blob/`off_mem_rsvmap` 8 字节对齐、`struct`/`strings` 块 checked 边界与互不重叠、结构块 4 字节对齐并以 `FDT_END` 结尾、MRB 终止项落在 `min(off_dt_struct, totalsize)`、**MRB 条目上限 16**、FDT 版本 `17/16`（项目子集）、块顺序 MRB → structure → strings（项目输入限制）。**b 结构块预扫（已实现，消费能力已验）**——无分配 token 预扫：token 合法性、单根、嵌套平衡、节点名/属性长度与名称偏移边界、**属性必须先于子节点**、深度 ≤63、`FDT_END` 唯一且最后。**c 目标节点语义**——`/memory` 与 `/reserved-memory` 的拒绝形状已落码（截断/零长 `reg`、MRB 冲突、动态 `size` 声明的拒绝）；**设备 `reg`／`ranges` 的解析与地址转换已移交 `04-platform-discovery.md`**，不再作为本项未闭合项。

**O-12j-b 的两条边界必须与解析库实现对齐，不能只按格式规范写**：

- **属性序**：`fdt 0.1.5` 的 `properties()` 在首个非属性 token 处停止，`children()` 与 `all_nodes()` 在「子节点之后的属性」处**直接结束迭代**——这种布局不是「容忍即可」，而是会把子树静默截断成部分树，因此预扫拒绝（`PropertyAfterChild`）。「QEMU/OpenSBI 会产出这种布局」的说法**不成立**：QEMU 8.2.2 `dumpdtb` 全树 0 处违规，当时的观测来自有缺陷的临时扫描器。引用早期提交上的双 PASS 读数时必须带提交与脚本，不能当作当前基线证据（当前 aarch64 shim 腿被既有阻断挡住，详见 `03`）。

**O-12j 状态须按代码状态与合同状态两列读**：

| 子项 | 代码状态 | 合同状态 |
|---|---|---|
| a 容器与 MRB | 已实现 | 已实现；后续输入变化仍需复审 |
| b 结构块预扫 | 已实现 | 部分闭合：跳转表与「解析器可消费」已证；**预编译依赖闭包中是否含跳转表未证**（`-C jump-tables=no` 只作用于本项目编译的 crate） |
| c `/memory` 语义解析 | 已实现（status/hotplug 准入、重复去重、部分重叠拒绝） | 部分闭合 |
| c `/reserved-memory` 静态 `reg` | 已实现（三语义、`ranges` 空、cells 一致、重复/重叠/冲突三分） | 部分闭合 |
| c 动态 `size` / `alignment` / `alloc-ranges` | **已实现**：`size`-only → `UnsupportedDynamicReservation`；无 `reg` 且无 `size` → `UnsupportedReservedDeclaration`；`reg`+`size` → 按 `reg` 处理、`size` 忽略 | 形状分流与规范依据**均已闭合**：三条规定都是 DTSpec 3.5 逐字规定（原文见 `03`），因此 `UnsupportedDynamicReservation` 拒绝的是**规范允许、本项目不支持**的输入，不是非法 DTB；`alignment`/`alloc-ranges` 只在有 `reg` 时被忽略、未单独设错误，因为规范把它们定义为动态分配约束 |
| c 设备 `reg` / 多级 `ranges` 转换 | 移交 `04-platform-discovery.md` | 不属本项 |

**早期执行闭包（EarlyExecutionClosure）合同**：一段代码在某阶段执行之前，必须证明它的 `Instructions ∪ ReadOnlyDataDependencies ∪ RequiredWritableState` 在该阶段的地址转换与权限下全部可达；只证明入口指令可执行不够。实证：预扫逻辑位于已映射页内，但 `switch` 被编译成 `.rodata` 跳转表 + `jr` 绝对地址，触发 RISC-V `instruction access fault`（`cause=1`，`epc` 落在 `validate_structure` 内）。
**跳转表禁用的落地方式**：本工具链（rustc 1.94.1）的选项拼写是 **`-C jump-tables=no`**（`-C no-jump-tables` 在该版本不存在）；`RUSTFLAGS` 会**覆盖** target config，因此镜像构建器（xtask）在注入的 `RUSTFLAGS` 中同时带上它，config 另为 riscv64 与 aarch64 直启目标保留一份。反汇编确认 `validate_structure` 只剩直接 `jalr`；去掉该选项时原故障复现。**边界**：预编译的 `core` 不受该选项影响，其代码路径仍需单独核对。

#### O-12c 设备资源不在本项（已移交）

设备资源的两份记录必须分开：**`MemorySourceRecord`**（本来就在物理地址域里的 RAM、保留区、MRB、EFI 描述符）与 **`DeviceResourceRecord`**（保留原始 bus 地址域、`reg` 元组、父节点与每级 `ranges` 转换，只有转换成功并确认地址域为 CPU 物理地址后才派生 `PhysicalRange`）。后者连同 `reg`／`ranges` 解析、ACPI 与平台 profile 的对应关系，已在第一百轮**整体移交** `04-platform-discovery.md`；`DeviceWindowPolicy`、授权范围、映射覆盖与实际 PTE 审计归分页／映射专项。逐字原文存 `03` 七十节。

本项只保留两条对内存图有影响的结论：① `reg` 只是资源位置描述，不推出「必须映射」「属性一定是 DEVICE」「可以进入恒等映射」；② 设备资源记录**不得**直接塞进以 `PhysicalRange` 为地址类型的内存来源记录——那是两类地址域，转换失败（缺失 `ranges`、某一级无法转换、只覆盖部分 `reg`、溢出、最终不是 CPU 物理地址）时不得静默产出看似合法的记录。

#### O-12k UEFI 内存图快照与 MapKey 一致性（**大部分实现**：单快照配对与重试、PreflightMap → header → 副本分配 → FinalMap 序列已落码；按最小描述符尺寸推导缓冲上界与最终范围的可达性独立复核未实现）

`GetMemoryMap()` 返回的 `MapKey` 随内存图变化而变化，`ExitBootServices()` 必须使用**当前**内存图的 key；key 失效时须重新取图、重试。这与 O-11 的自有副本、对象登记表、诊断缓冲区的分配直接冲突：每次 Boot Services 分配都可能改变内存图。
**合同**：

1. 所有可能改变 UEFI 内存图的 Boot Services 分配（DTB 自有副本、引导对象缓冲、登记表、诊断区）**先完成**；
2. 最后取一次 `GetMemoryMap()`，保存 `MapKey`、`DescriptorSize`、`DescriptorVersion` 与**完整描述符快照**；
3. 用这份快照生成来源记录与政策投影；从取图到成功 `ExitBootServices()` 之间**不再执行会改变内存图的操作**；
4. `ExitBootServices()` 因 key 失效失败 → **重新取图、重建来源记录与政策投影、重新审计**后再重试；不得沿用旧快照的 `PmmBaseAvailable` 与用途准入证据；
5. 重取后描述符数量与分段可能变化：容量不足、描述符增多、重试耗尽都要有明确失败形状，不能假设「只差一个 MapKey」。

**两次快照、两种用途（与 O-11 的时间顺序）**：

| 顺序 | 操作 | 使用的内存图 |
|---|---|---|
| 1 | 预分配内存图缓冲区与首次读取所需的最小工作区 | 尚无快照 |
| 2 | `GetMemoryMap()` → 检查 `[dtb, dtb+40)` 全部页属于 `EfiACPIReclaimMemory` | **`PreflightMap`**，只用于首次读取授权 |
| 3 | 读取一次 40 字节 header snapshot，校验 magic/版本/`totalsize`/边界 | 不再读原 header |
| 4 | 分配 DTB 自有副本与其余工作区；此后不再进行 Boot Services 分配 | `PreflightMap` 不再作为政策依据 |
| 5 | 再次 `GetMemoryMap()` → 检查完整 DTB 页范围属于 `EfiACPIReclaimMemory` | **`FinalMap`**，取得最终 `MapKey` |
| 6 | header snapshot 写入副本、只复制 body；在副本上完成校验、解析、来源记录、政策投影与审计 | 所有结果绑定 `FinalMap` |
| 7 | 不再调用会改变内存图的服务，立即 `ExitBootServices(FinalMap.MapKey)` | 同一最终快照 |

三条硬约束：**`FinalMap` 的完整类型检查必须在读 body 之前**（不能用 `PreflightMap` 的 header 验证代替整段授权）；**步骤 5 与步骤 6 之间必须再确认一次完整范围的 `BootInputReachability`**——`FinalMap` 只证明物理范围的类型与覆盖，不证明当前 VA 翻译、PMP/PMA 与访问路径仍可读，两者不能互推；**最终取图之后的解析/采集/投影/审计必须是无分配路径**（隐藏的动态分配同样会让 `MapKey` 失效）。`PreflightMap` 不进入 PMM 投影，也不与 `FinalMap` 的记录混用。
**缓冲区不足分三种情况，不能共用一个规则**：

| 发生位置 | 第一版处理 |
|---|---|
| `PreflightMap` 太小 | 尚未读 header：可以扩容后重取，或明确拒绝启动 |
| 首次 `FinalMap` 太小 | 尚未调用 `ExitBootServices()`：可扩容后重跑最终取图与审计；若坚持「步骤 4 后不再分配」，则明确拒绝 |
| 首次 `ExitBootServices()` 失败后的重取太小 | **保持现有决定**：复用缓冲区、不扩容、明确失败——此时固件可能已部分关闭 Boot Services，约束不同 |

**扩容即失效**：只要为扩大缓冲区调用了可能改变内存图的 Boot Services 分配，当前 `MapSnapshotId`、`MapKey`、来源记录与全部政策投影**整体失效**——必须重新 `GetMemoryMap()`、生成新快照身份、从新快照重建依赖结果。`MapSnapshotId` 用预分配或单调递增的本地序号生成，**不能通过新的动态分配创建**（观察实例的身份不应反过来改变被观察的内存图）。

**必测（已实现 ①②③④ 的全部可测形态）**：① 预检时 header 页是 ACPI reclaim、最终快照类型改变 → 读 body 前拒绝（`a_final_map_type_change_stops_before_the_body_copy`）；② 预检跨两个连续同类型描述符、最终快照分段变化但仍完整覆盖 → 允许（`the_final_map_may_segment_the_same_range_differently`）；③ 最终快照验证后零分配（`the_final_map_phase_allocates_nothing` 用测试期分配计数；`ebs_pair` 的 `dtb_preflight_reads_one_map_before_the_paired_final_map` 在整段配对调用上再断言一次）；④ 三种 `EFI_BUFFER_TOO_SMALL`：预检不足（`preflight_buffer_too_small_stops_before_the_header_read`，且 header 未被读）、首次最终图不足（`buffer_too_small_before_ebs_asks_caller_to_grow`）、重试不足（`too_small_on_retry_is_fail_closed`）。

**重试阶段的执行环境必须预先约束**（规范允许首次 `ExitBootServices()` 失败后固件已部分关闭 Boot Services）：

- 首次调用**之前**，重试所需的一切已分配：内存图缓冲区、分类记录、政策投影、审计工作区、诊断状态；
- 首次失败后，重试路径**只使用预留缓冲区与纯内存计算**：不调用文件系统、控制台、设备协议、事件服务或任何依赖完整 Boot Services 的功能；
- 复用同一内存图缓冲区，并预留描述符增长余量（规范明确：分配新缓冲区本身可能增大内存图）；重取返回 `EFI_BUFFER_TOO_SMALL` 时第一版**直接以明确错误终止**，不在部分关闭状态下扩容；
- 新快照必须**重建**全部依赖描述符的结果（来源记录、资格集合、PMM 投影、审计意见）；`MapKey`、记录与投影属于同一快照版本，不得混用；
- 只对 `EFI_INVALID_PARAMETER` 走 stale-key 重试；其它错误走独立失败路径；重试次数有上限；
- **重试期间不打印日志**：错误路径不得调用 Console 或协议服务取诊断——那正是首次失败后被限制的环境。诊断输出只在交接成功后由自有设施进行。

**静态准备前移，缩短「取图 → 交接」窗口**：对象登记表、静态设备政策、固定容量工作区与静态审计规则不依赖本次 EFI 快照，应在最终取图**之前**构建；最终取图后只做必须依赖本次快照的验证与投影，然后立即 `ExitBootServices()`。这不改变正确性要求——首次失败后仍须用新快照完整重建，旧投影不得复用。

重试内核是纯函数式阶段，逐级重建：

```
GetMemoryMap(reused_buffer) → ValidateDescriptorSnapshot
  → RebuildSourceRecords → RevalidateBootInputObjectsAgainstFinalMap → RebuildPolicyProjections → Audit
  → ExitBootServices(new_map_key)
```

**必测负控（已实现）**：① stale key 后新快照的分段变化 → 按新快照重建（`invalid_parameter_retries_once_and_uses_the_new_map`；DTB 副本与记录快照的对应见 `a_retried_ebs_takes_a_new_snapshot_and_the_copy_survives`，接受记录的 `snapshot_id` 为 2）；② 重试缓冲不足 → `BufferReuseInsufficient`（`too_small_on_retry_is_fail_closed`）；③ 非 `EFI_INVALID_PARAMETER` → `ExitStatus`（`other_ebs_status_fails_immediately`）；④ 重试耗尽 → `RetryExhausted`（`two_invalid_parameters_exhaust_the_retry_budget`）；⑤ 失败后的路径不再调用 Boot Services 或分配器——由「取图之后只有分类、副本复制、静态主张与 EBS」的代码结构与 ③ 的零分配断言共同保证。

**为什么单列**：它约束的是 UEFI 输入快照的**时间一致性**，不是描述符几何合法性（O-12h）；两者失败形态不同。

#### O-12h 的两级结构校验

| 层级 | 检查 |
|---|---|
| 内存图元数据 | `DescriptorVersion` 受支持；`DescriptorSize` 不小于已知前缀；`MemoryMapSize` 能被 `DescriptorSize` 整除**且不超过调用方缓冲区长度**；按 `DescriptorSize` 步进；字段读取不越出该条前缀 |
| 每个描述符 | `PhysicalStart` 对齐且不超上界；`VirtualStart` 按规范校验（**`0` 可接受**，它满足对齐；第一版验证后忽略）；`NumberOfPages` 非零，页数约束**分别对物理与虚拟起点**计算；两个末端都用 **u128**；**不论类型是否输出区间都执行** |
| 跨描述符 | **物理区间不得重叠**：两个描述符覆盖同一页却给出不同政策时，分类器无法给出唯一结果，必须拒绝启动 |
| 顶端边界 | **结构合法性与平台准入分开（已裁定，未实现）**：`[0xffff_ffff_ffff_f000, 2^64)` 是合法的最后一页，UEFI 只约束「页起始地址不超过 `0xffff_ffff_ffff_f000`」，不要求独占末端能用 `u64` 表示。结构校验用 `u128` 接受 `end == 2^64`；是否可用由平台地址准入层判定（`effective_pa_limit`、PMA/PMP 等），拒绝时用独立错误码。DTB 腿的三层落定见 §2.1 与 O-12i，UEFI 描述符路径本就接受 `end == 2^64`；测试 `physical_range_expresses_the_top_page` 与四个边界用例 |
| 版本策略 | 只接受 `DescriptorVersion = 1`；其它版本明确拒绝（不是「受支持」的模糊说法） |
| 顶端页测试（待补） | `PhysicalStart = 0xffff_ffff_ffff_f000, Pages = 1` 通过；同起点 `Pages = 2` 拒绝；`VirtualStart` 同两点；`VirtualStart = 0` 允许但页数仍须满足虚拟上界；**以上对不进入任何分类输出的类型同样执行** |

#### O-12g 的附加要求

原始记录保留：声明所在节点、**每一级 `ranges` 转换的输入/输出与引用节点**、最终 CPU 物理地址、属性与来源身份——只保留最终地址会让审计无法解释地址是怎么来的。
**四条补充要求**：① **排除原因多重归属**——同一页可同时被普通保留、MRB、存活对象覆盖，PMM 只扣一次，但审计能列出全部 `SourceId`；② 固件前缀、内核镜像、boot shim、自举页表、栈、启动模块、DTB 原始对象与副本、登记表**不能合并成一个 Reserved 标记**，它们各自有阶段化读写/执行需求；③ **重叠规则**：**同一权威来源内部的同语义重复**集合去重、保留多个 `SourceId`；**跨来源证据**（UEFI 类型、平台分类、固件保留与 RAM 来源）对同一物理区域提出**不兼容主张**时进入冲突裁决，不能靠并集掩盖——不兼容即拒绝启动。`/memory` 本身只有 RAM 语义（`status` 非 `okay` 与 `hotpluggable` 已被准入拒绝），不存在「同源一条说 RAM、一条说不可用」的形态；④ **原始记录必须保留不输出区间的描述符**（含 `RESERVED`/`UNUSABLE`）——否则 O-06 的「`UNUSABLE` 属于来源域」没有可实现的输入；诊断成员表是面向内核的另一层，不能替代原始记录。

**状态按六层记（O-12g，第九十六轮按代码读数重述——审计自洽不等于交棒）**：

| 层 | 现状判断 |
|---|---|
| DTB 采集器能生成带身份的记录 | **已实现**（`fn collect_with_sources`） |
| 三条 DTB 路径能用记录做覆盖审计 | **已实现**：riscv bootface、aarch64 直启、riscv U-Boot 都在投影产生后、交棒前调用 `audit_source_coverage`（输入是投影结果，证明「投影有来源覆盖」） |
| 四条 DTB 路径均生成并保留记录 | **已实现（第一百二十二轮）**：三条 DTB 腿把记录写入 `KernelInfo.memmap_sources`；aarch64 UEFI 的配置表 DTB 记录（≤40）在 `capture` 时留存、随 UEFI 记录一起写入来源分区，容量 296 覆盖两类输入之和 |
| UEFI 描述符全部生成带身份的记录 | **已实现（第一百二十轮）**：每个结构合法描述符一条记录，`Side`/`Excluded`/`DevicePath` 记 `UefiExcluded`；`raw_type`/`Attribute`/`VirtualStart`/快照序号齐备 |
| 来源记录进入跨第一跳的 ABI | **已实现（四条腿）**：`KernelInfo.memmap_sources` ＋ v1 arena 的来源分区（wire v7，296 条）；UEFI 腿逐描述符记录自 A2 起接入，启动对象另有 16 槽独立分区 |
| 内核政策消费者实际消费来源记录 | **消费边界审计已实现**（`KernelInfo::validate` 用记录复核覆盖）；**记录驱动的 `PolicyProjection` 未实现**——属范围裁决项（见 §7） |

**容量上限必须按「解析器接受数」而不是「缓冲物理容纳量」算（第九十八轮更正）**：候选缓冲的容量由 `fn probe_map_buffer_size` 给出——`max(初始需求, 16 KiB) + max(DescriptorSize, 64) × 8 + PAGE_SIZE`；在最小 `DescriptorSize = 40`、初始需求 ≤ 16 KiB 时是 `16384 + 512 + 4096 = 20992` 字节，**能装 524 条 40 字节描述符**。但解析器只接受 256 条（`MapStorage.descs: [DescriptorRef; 256]`），所以真正的接受上限是：

```
ActualDescriptorCount    = MemoryMapSize / DescriptorSize
MaxAcceptedDescriptorCount = min(MAX_MAP_DESCRIPTORS, floor(MapBufferCapacity / MinimumSupportedDescriptorSize))
                           = 256（当前配置）
```

实现合同是四步，不要求任何「取图前知道最终条数」：

1. 最终取图**前**按 `MaxAcceptedDescriptorCount = 256` 预分配固定容量；
2. 最终取图**后**按 `ActualDescriptorCount = MemoryMapSize / DescriptorSize` 计算；
3. 超过 256 就以 `TooManyDescriptors` **整体拒绝**，不得截断；
4. 不为 524 条（缓冲理论容纳量）分配槽位——那是缓冲容量，不是接受上限。

**总容量公式**（不能各模块各自声明后依赖偶然的数组大小）：

```
MemorySourceRecordCapacity  = UefiDescriptorSourceCapacity      ← ≥ 256（= MaxAcceptedDescriptorCount）
                            + DtbSourceRecordCapacity           ← 40（8 + 8 + 16 + 8；已含最多 8 条固件前缀）
                            + AdditionalDerivedSourceCapacity   ← 只在 40 槽之外另计的派生记录；固件前缀**不重复计**

StartupObjectRecordCapacity = KernelImage + BootModule + BootPageTable + BootRoot + BootArena
                            + OtherStartupObject                 ← 各项按真实 wire 记录逐条核算
```

**两类记录不是同一种东西**，不能用一个 `TotalSourceCapacity` 混算：来源记录是「事实从哪来」，启动对象记录是「谁占了什么」。它们可以共用一块物理缓冲区，但槽位定义必须各自成立。**第一百一十七轮已落码**：启动对象记录有独立分区（16 槽 × 72 B）与独立种类标签，不再与内存图分区共用槽位；派生记录仍在内存图分区的 256 槽内。

**启动对象的职责按跳分层（不是所有对象都进第一跳）**：

| 对象 | 责任位置 |
|---|---|
| 内核映像、启动模块（UEFI 腿另有根页、bump、arena） | 第一跳 `startup_objects`（16 槽；UEFI 腿恰好 16 条，三条 DTB 腿 13 条） |
| 内核创建的 root、bump、页表池已用后缀 | 第二跳 `deducted` |
| 第一跳 arena | 临时交接对象，最后使用点是解码完成（`decoded_payloads_survive_arena_poison`） |
| DTB 原始对象 | 按最后读取点处理：Phase A.5 解析后无人再读；任何后置消费者必须先复制或登记为存活对象 |

`SourceRecord` 目前**没有** `last_use` 字段；第一版的生命周期表达是「上表按跳分层 + 第二跳用扣减记录兜底」，不是逐对象生命周期。

**容量闭合：五个问题按代码读数回答（不再只留「固定容量工作区」一句话）**

| 问 | 代码事实 |
|---|---|
| Map 缓冲实际能容纳多少描述符 | **不是固定常数**：`fn probe_map_buffer_size` 取 `size.max(16 KiB) + margin`，`margin = desc_size.max(64) × 8 + PAGE_SIZE`，再 `allocate_pool(LOADER_DATA, bytes)` |
| 最小接受的 `DescriptorSize` | `MEMORY_DESCRIPTOR_SIZE = 40`；更小 → `DescriptorTooSmall`；不是 8 的倍数 → `DescriptorSizeNotAligned` |
| 256 是拒绝上限还是缓冲推导 | **是解析器的存储上限**：`MapStorage.descs: [DescriptorRef; 256]`，`entries > 256` 直接 `TooManyDescriptors`；与缓冲大小无关 |
| 来源记录数组是否 ≥ 256 槽 | **已扩到 296**：`WIRE_SOURCES_CAP = 296`（256 描述符 + 40 DTB 记录），9 页 arena 放下后余 384 B；`ebs_map` 有编译期断言 `WIRE_SOURCES_CAP >= MAX_MAP_DESCRIPTORS + DTB_SOURCE_CAP`，覆盖审计按输入权威过滤 |
| 派生记录与启动对象记录容量 | 启动对象：独立 16 槽（`WIRE_STARTUP_CAP`，wire v7）；派生记录：占用内存图分区的 296 槽中的描述符份额 |

`MaxDescriptorCount = floor(MapBufferCapacity / MinimumSupportedDescriptorSize)` 这条可计算关系仍是**未实现**要求；但「stale-key 重试后描述符数超上限必须整体拒绝、不得截断」这一条**已实现**：超限走 `TooManyDescriptors`，shim 侧 `panic!("boot-shim: memory map rejected: …")`，是整体拒绝而非截断。**两枚边界测试已落码**：恰好 256 条通过（`accepts_exactly_the_descriptor_and_partition_caps`）、257 条以具名错误拒绝（`rejects_a_map_larger_than_the_storage`）；DTB 侧对等测试 `dtb_source_capacity_is_reachable_at_exactly_forty`（上限 `DTB_SOURCE_MAX = 8 + 8 + 16 + 8 = 40`）。启动对象分区的边界由 `startup_partition_accepts_exactly_its_capacity`（16 条通过、17 条拒绝）钉住。

**arena 载荷预算（wire v7 的真实数字）**：`WireMemoryRegion` 16 B、`WireBootModule` 32 B，所以可用表 128×16 = 2,048 B、保留表 256×16 = 4,096 B、模块表 12×32 = 384 B、平台描述 4×16 = 64 B、参数字节 1,024 B；编译期算术给出 `WIRE_PARAM_OFFSET = 6848`、`WIRE_END = 7872`、`SEMANTIC_END = 14016`。内存图来源分区开在 `SEMANTIC_END`（`WIRE_SOURCES_OFFSET`），`WireSourceRecord` 72 B（无损承载 `u128` 的独占末端、身份与属性），容量 **296 条 = 21,312 B**（= `MAX_MAP_DESCRIPTORS` 256 + 配置表 DTB 记录 40；aarch64 UEFI 同时具备两类输入，不能二选一），结束于 **35,328 B**；启动对象分区紧随其后（`WIRE_STARTUP_OFFSET = 35,328`），**16 条 = 1,152 B**，结束于 **36,480 B**。`ARENA_CAP` 为 **9 页 = 36,864 B** → **余量 384 B**；编译期断言 `WIRE_SOURCES_END == 35328`、`WIRE_STARTUP_END == 36480` 与两者 `<= ARENA_CAP` 同时钉住。信封为 184 B（`sources_byte_len` @168、`startup_byte_len` @176），仍在 256 B 预算内；`BOOT_HANDOFF_V1_VERSION = 7` 且按精确相等比较，v1–v6 的发送方会被拒收，不会按新偏移读旧镜像。

**`#[repr(C)]` 不等于完整 ABI 合同——六项核对的当前读数**

| 核对项 | 第一跳 `BootHandoffV1` | 第二跳 `VmBootHandoff` |
|---|---|---|
| 结构大小／对齐 | 编译期断言（`size_of == 184`、`align_of == 8`、`<= ENVELOPE_CAP`） | 有（`size_of == 3792`、`align_of == 8`、`<= 4096`） |
| 逐字段偏移 | **有**（`offset_of!` 逐个钉：magic 0、version 4、size 8、arena_len 16、`kern_virt_base` 24…`param_buf_byte_len` 160、`sources_byte_len` 168、`startup_byte_len` 176） | **有**（`test_vm_boot_handoff_layout_is_pinned` 钉九个偏移：magic 0、version 4、root_paddr 8、kerninfo_pa 16、free_region_count 100、deducted_count 104、module_count 108、deducted 1136、modules 1888） |
| 地址字段固定宽度 | 有（`WirePhysAddr`/`WireVirtAddr` 是 `#[repr(transparent)] u64`） | 有（`u64`/`u32`） |
| 是否含 `usize`／Rust 枚举／`bool`／引用 | 无（纯标量与定长数组；VM 侧解码后的 `BootParams` 才引入 `bool`/`Option`/切片） | 线上结构无，解码后有 |
| 载荷偏移相对 arena 起点 | 有算术断言 | 未定义（用固定 VA `VM_BOOT_HANDOFF_VA` 直读） |
| 版本／未知字段拒绝行为 | 头部有 magic/version/size | 消费侧 `assert_eq!` 相等才通过，**不等即 panic**，没有优雅拒绝路径 |

第二跳的容量推导按**最终 wire 记录数**算，不是按语义来源数；两个前提在本轮取到代码证据（第九十九轮更正为 **47**）：`MAX_BOOT_REGIONS = 32` 是**分配器的聚合上限**（`VmBootRegions { regions: [VmBootRegion; 32] }`，超限即 `TooManyRegions`，不是每池 32）；`NR_BOOT_MODULES = 12`，且 VM 自己的 blob **也在**扣减记录里（内核侧有 `boot_modules().len() == NR_BOOT_MODULES` 断言）。`deducted` 数组里存的是**来源自身的范围，不是与候选页域求交后的片段**（第一百轮按 `fn push` 的读数钉死）：`push` 只把长度向上取整到页并断言起点对齐，**既不与候选域相交、也不合并相邻记录**；内核映像、自身根页、bump 整段、每个模块 blob 各是一条整段记录；池区间的已用后缀由分配器按「从区间尾部向前消费」的规则产生，**每个被触碰的池区间一条**。因此每条来源恰好一条记录，与候选域怎样切分无关——这正是 47 能作为上界的原因；若将来改成按交集片段记账，就必须按片段数重新推导（不能再用来源数相加）。所以最坏 `3 + 32 + 12 = 47` 条，**大于当时的 `VM_BOOT_HANDOFF_MAX_DEDUCTED = 32`**（现为 47）；散文里那句「逐项相加 22」只在「每个池只碰一个区间」时成立。容量目标先按保守上界写成 `ConservativeDeductionBound = 3 + 32 + 12 = 47`；若要降到 46，必须证明第 12 条模块记录不可能成为扣减项。当前 `record.push` 在超容量时直接命中 `deduction record overflow — LiveBootstrap is not enumerable within capacity` 的断言——**这是启动期 panic 路径**，不是可接受的失败形状。§2.4 说容量「固定、不许放宽」，指的是**不得改成无界动态扩容**；把固定值按上界提高不属于「放宽」。O-13b 的验收：把容量提到该上界（或把记录按语义合并后重新推导），外加恰好达上限与超一条的两枚测试。（该处旧注释与实现的矛盾已于第一百零六轮随 `FREE_MEMMAP` 清理改正。）**第一百零一轮落码**：容量 47 + 版本 8 + 编译期断言 + 两枚边界测试已进 `minix-types`／`minix-kernel`，宿主读数 861 通过 / 0 失败 / 8 忽略，三架构 `--no-default-features` 交叉构建通过；QEMU 腿在本机未跑（CI 镜像内无 qemu、宿主无 Rust 工具链），按未验证处理。 `MAX_MAP_DESCRIPTORS = 256`、`SIDE_CAP = 64`、`WIRE_MEMMAP_CAP = 128`、`WIRE_RESERVED_CAP = 256`。O-12g 要求**每个通过结构校验的描述符**都有原始记录，所以来源记录数组容量必须 ≥ 最终 Map 缓冲可容纳的最大描述符数（按 `descriptor_count = MemoryMapSize / DescriptorSize` 计算），且必须在最终取图**之前**就发现容量不足；`ExitBootServices` 重试导致描述符数量增加时，只能安全失败，不得截断记录（截断即违反「每个描述符都有身份」）。现有失败形状已具名：`TooManyDescriptors`、`TooManySideEntries`、`Capacity(TooManyMemmap/TooManyReservedRegions)`。


**已解决的旧冲突（按来源分治）**：`/reserved-memory` 的精确重复按**三种语义**（普通/`reusable`/`no-map`）分别去重、来源各留一条，跨语义同范围一律冲突；MRB 的精确重复**必须拒绝**（DTSpec 5.3.2）；`/memory` 的部分重叠按项目策略拒绝。六条测试见 §2.1。

**合法记录与拒绝诊断记录分开**（**已落码，第一百一十六轮 B1**）：结构校验失败的描述符不进入来源记录层；拒绝发生时 `MapError::RejectedDescriptor` 携带 `index`、原始 `raw_type`、`physical_start`、`virtual_start`、`number_of_pages`、`attribute` 与失败项（`PolicyError`），停机报告可定位到具体描述符与失败原因（负控 `a_refused_descriptor_is_named_in_the_error`）。形态与「持久诊断记录」不同：拒绝是停机路径，没有后续读者；若将来出现非致命拒绝，再按记录形态补。

**统一来源身份**（**已落码，以 `SourceRecord` 字段形式，没有单独的 `SourceRef` 类型**）：唯一定位一条记录的三元组是 **`(input_object, snapshot_id, local_source)`**。

| 字段 | 语义 |
|---|---|
| `input_object` | 输入**类别**：`DtbBootInput` / `UefiMemoryMap` / `StartupObjects` |
| `snapshot_id` | 该类别的**一次具体观测**：UEFI 每次成功取图生成新序号（与 `MapKey` 无关），DTB 固定为 0 |
| `local_source` | 该观测内的源序号（UEFI = 描述符索引；DTB = 采集顺序） |
| `id` | 记录在输入对象内的稳定序号，供诊断与排序；**不是**跨输入的来源身份 |

`origin` 的变体载荷（如 `FirmwarePrefix { memory_id, cut }`）是「输入对象内的位置细节」，不是跨输入的引用；跨输入只认上面的三元组。`MapKey` 只属于 `ExitBootServices` 的配对凭据。`PreflightMap` 不产生最终政策记录；`FinalMap` 的全部描述符同属一个 `snapshot_id`，stale-key 重试产生新序号且旧记录与旧投影整体废弃。节点路径文本与设备 `ranges` 转换链**不属于**内存图来源记录（后者已随设备资源发现移交 `04`）。

**UEFI 原始描述符记录的最小字段**（**已落码，第一百一十六轮 A2 / 一百一十七轮 B1**）：`DescriptorIndex`、原始 `Type`、原始 `Attribute`、`PhysicalStart`、`VirtualStart`、`NumberOfPages`、**`address_domain`**、结构校验结果。地址域必须是显式类型，不能靠字段名推断：

```
DescriptorAddressDomain = PhysicalAddress | UefiMmioPortSpaceAddress | UnsupportedOrOpaque
    ← **地址域只表达事实，是否支持由另一字段表达**（第九十六轮改）：`EfiMemoryMappedIO`（类型 11）
      描述的是物理 MMIO 区域，`PhysicalStart` 的地址域就是 PhysicalAddress；把它记成
      UnsupportedOrOpaque 会把两个不同问题混起来，将来新增一种暂不支持的物理资源类型，就会把
      已知物理地址误标成不透明地址。政策侧改记 `PolicyDisposition = UnsupportedUefiRuntimeMmio`：
      不产生 `DeviceWindowPolicy`、不进 OS 普通设备资源授权，但来源事实保留
      （UEFI 在 `ExitBootServices` 之后不再用该类型，系统 MMIO 信息应从 ACPI 取；
      这条按「评审人已核的 UEFI 2.10/2.11 表述」使用，本方环境抓不到 uefi.org）。
      **类型 12（`EfiMemoryMappedIOPortSpace`）用第三个成员 `UefiMmioPortSpaceAddress`**：它是架构相关的 I/O 空间，
      规范对 Itanium 另有一句「只定义一个这样的区域」的特别说明，**不是 x86 的 `IN`/`OUT` 端口地址**，因此不得默认与
      O-08 的 `PortWindowPolicy` 共用类型、也不得转成可被 x86 端口消费的范围。第一版若不支持类型 12，保留原始字段
      与来源身份即可。这些规范表述按「评审人已核（UEFI 2.10/2.11）」使用，本方环境抓不到 uefi.org，未复现。
    ← EfiMemoryMappedIO 可以产生物理地址域记录；
      EfiMemoryMappedIOPortSpace（类型 12）**不得**因为描述符字段叫 PhysicalStart 就规范化成
      PhysicalRange——§3 不变量 5 已规定端口 I/O 不属于物理地址集合，不进 PMM、页表或物理范围集合。
      只有 PhysicalAddress 才允许进入 PhysicalRange；类型 11（运行时服务 MMIO 请求）按项目政策
      不产生窗口，记 PolicyDisposition = UnsupportedUefiRuntimeMmio 并保留原始字段
```

每个通过结构校验的描述符先成为带身份的记录，再进入类型裁决——裁决可以拒绝启动，但不能在来源层消灭 `RESERVED`/`UNUSABLE`/MMIO/类型 11/12；导致拒绝的 `PAL_CODE`/`UNACCEPTED` 至少要在错误上下文保留索引、类型与范围。

**仍缺的字段**：节点路径文本（今天只记 `origin` 类别 + 序号；序号的意义绑定输入对象与遍历顺序，跨对象不可解释）。设备 `ranges` 转换链已移交 `04`；`SourceRef` 不做独立类型，身份以 `SourceRecord` 三元组字段表达；UEFI 侧描述符记录已全量输出（第一百二十轮）。顺序不变：先把记录接到启动路径并由政策层持有，再谈 O-02/O-03 的禁止集合编译；否则 DenyStandardSystemMapping、DenyUncontrolledSpeculativeAccess、MRB 访问禁令与「同页多重排除理由」都没有输入来源。

**记录层与原始输入对象分开**：原始记录保真**不等于**永久保留整份 DTB 或 EFI 内存图缓冲区。`SourceRecord` 是规范化记录（保留到政策投影与审计不再需要）；原始 DTB 副本只在解析与转换期间需要，最后一个消费者结束后可以释放。设备 `ranges` 转换链（每级输入/输出地址、引用节点、转换属性）属设备资源发现（`04`），内存图来源记录只保存物理内存图自己的原始声明与范围。

#### O-12i 每种投影的页粒度规则

| 投影 | 边界处理 |
|---|---|
| `RamSourceDomain` | 保留原始 byte range；只有构造页域时才说明边界处理 |
| `PmmSourceCandidatePages` | 只交付完整落入合法 RAM、且通过保留／存活对象／类型与生命周期排除的整页（向内收缩） |
| `HardwareAdmissionExcluded` | 架构与平台物理访问能力不满足而排除的页（专项侧，不在本项） |
| `PmmBaseAvailable` | 上面两项组合后的最终基底可用页 |
| `PmmBaseExcluded` | 定义为 `RamSourcePageDomain \ PmmBaseAvailable`（最终补集）；每页为何在排除侧由贡献记录解释——保留声明、存活对象、类型／生命周期政策与硬件准入各自成条，不是只有保留声明 |
| `PmmRoleRejected` | 不做量化：它标注的是基底内某页缺哪条用途证明，不产生页集合边界 |
| `no-map` 映射禁止 | 向外覆盖所有相交页，防止大页/小页叶子跨入禁止区域 |
| MRB 访问禁止 | 保留精确 byte range；若通过页表落实，另做页粒度投影 |
| 设备窗口映射 | 叶子覆盖的**全部页**都要有兼容授权，不能只看起始页 |

**用途集合是分配时的凭证，不是事后可以随意扩大的标签**：分配交付的是 `(page, U_verified, p)`。① `U_verified` 记录交付时已证明的用途集合，页表 / 可执行 / 原子用途必须显式在里面；② 承担新用途（数据页改作页表、页表页改作可执行、任意页首次做 AMO）要**先重新准入再使用**——对 `U_verified ∪ {新用途}` 重跑 `PmmRoleEligible`，且用当下的 `H`（参与 hart 集合）与 `p`（阶段）重取证据，因为 hart 入场与阶段切换都可能使旧证明失效；③ **撤回用途不释放容量**：`U` 变小不产生新的可用页，一页能否回到 `PmmBaseAvailable` 只由生命周期决定（存活对象是否解除、保留是否撤销、`UNACCEPTED` 是否接受），不由用途标签变化决定——否则「改个标签」就凭空多出内存。

**所有 `PMM*` 集合都是 4 KiB 页集合**：`TypeAllocatablePages(p)` 表示「整页都满足阶段 `p` 的类型资格」的页；`PlatformAllocationDenials(p)` 也必须是页集合（或先向外量化）。来源原始范围仍是 byte range，只有进入指定投影时才量化；**阶段资格变化必须由显式事件触发**，不得随时间推进自动释放。

**`FullPages` 以页编号定义**：

```
PageBytes(n) = [ u128(n) * 4096, (u128(n) + 1) * 4096 )   ← 右端独占；FullPages(R) = { n | PageBytes(n) ⊆ R }
```

参数是页编号而不是物理地址或 byte range；`0 ≤ n < 2^52`（4 KiB 页、64 位物理地址），边界先转 `u128` 再运算，最高页的独占末端可以是 `2^64`。**两端都是半开区间**，与全文集合模型一致。只有整页 byte range 都被 RAM 来源覆盖才进入页域，不是「起点向上、终点向下后拼接」。
**页域与排除贡献的正式定义**（基底用补集，用途用子集——用途层不构成第二次划分）。第九十九轮把三件事分开：**贡献记录**（谁、为什么、原始范围、页投影、阶段）、**页集合**（并集）、**致命拒绝**（不是集合贡献）：

```
ExclusionContribution { evidence_ref, reason, raw_range, page_projection, phase }
    evidence_ref ∈ SourceRef | ObjectRef | PolicyRuleRef     ← 不强迫对象与政策贡献伪装成原始来源记录
    reason ∈ StaticReservation | LiveObject | TypePolicy | Lifecycle

PmmSourceExclusionContributions(ctx) =
    静态保留类：FirmwarePrefix | ReservedOrdinary | ReservedReusable | ReservedNoMap | MrbEntry
      ← **MRB 必须单列**：它的分配排除与访问禁止是两份合同、两条贡献，不因同出一源就合并；
        与 EFI DTB 相交时，DTB 的类型特例只改类型主张，MRB 的排除仍然存在
    ∪ LiveObject（以 ObjectRef 为身份）∪ TypePolicy(ExcludeRange) ∪ LifecycleRule（以 PolicyRuleRef 为身份）

PmmSourceExcludedPages(ctx) = Union(c.page_projection for c in 上述贡献)
    ← 页只扣一次由并集保证；「为什么扣、哪些来源共同扣」由贡献记录保证——裸 Union 只留成员关系，
      不留归因，所以两个产物必须分开写

TypePolicyDecision = AcceptWithDisposition | ExcludeRange | RejectBoot
    ← **拒绝启动不是页集合贡献**：只有 ExcludeRange 进 PmmSourceExcludedPages；
      UNACCEPTED 与 PAL_CODE 属 RejectBoot，整个启动输入不被接受，
      绝不能写成「排除若干页」后仍然产出一份看起来合法的候选集合

PmmSourceCandidatePages(ctx) = RamSourcePageDomain \ PmmSourceExcludedPages(ctx)
    ← **本项的输出边界就到这一层**：它回答「来源与生命周期上这页能不能进 PMM 候选」，
      不回答「这台机器允许这次访问吗」。
```

**最终基底的一层（含域裁剪）**：

```
PmmBaseAvailable(ctx) ⊍ PmmBaseExcluded(ctx) = RamSourcePageDomain
PmmBaseExcluded = RamSourcePageDomain ∩ (PmmSourceExcludedPages ∪ HardwareAdmissionExcluded)
    ← **域裁剪不可省**：硬件准入排除与存活对象范围都可能落在 RAM 来源域之外；贡献记录保留完整物理范围与
      归因，但参与基底分割的页集合必须先裁剪到 `RamSourcePageDomain`，否则「互补且只对页域成立」不是公式
      保证的性质。等价写法：`PmmBaseExcluded = RamSourcePageDomain \ PmmBaseAvailable`
```

**页粒度表同时要改**（原表把 `PmmBaseAvailable` 写成「包含用途无关准入域的整页」，那是旧模型）：`PmmSourceCandidatePages` 负责来源、保留声明、对象存活与类型／生命周期政策；`HardwareAdmissionExcluded` 负责架构与平台的物理访问能力；`PmmBaseAvailable` 是两者组合后的最终基底可用页。

**来源排除与硬件准入是两次相减，不是一次**：`PmmSourceExcluded` 由本项定义（四类贡献、逐条可归因），`HardwareAdmissionExcluded` 由专项定义；两次相减的顺序不影响结果，但**谁贡献了哪一条排除理由**必须分别可查。；用途资格属于消费者在分配时附加的约束，不应与基底排除合并；硬件准入、用途资格与映射路径审计见 `01-design-note-hardware-admission-model.md`。随本轮一并移出的还有 `MemoryAdmissionContext` 的架构证据槽位、`PolicyProjectionKey` 的完整签名、参与 hart 集合变化导致投影失效的推导、`MappingIntent`／`MappingAudit` 与 `DenyStandardSystemMapping`／`DenyUncontrolledSpeculativeAccess` 的叶子级判定。**本项只保留与交棒直接相关的一条**：快照身份、阶段与证据代次必须进投影签名，否则同一份来源记录会产出两个都叫「可用表」的结论。

- 任何「可用」主张必须同时带用途集合与准入上下文：裸写 `PMMAvailable(·)` 不是良定义项。
- 用途集合**可组合、不互斥**，不是页的分类；分配时用途未知只能保守取 `U = AllRoles`——这两条约束的是消费者接口，本项保证交接信息足以支撑它们。

#### 用途准入与硬件准入模型已迁出本项（第九十六轮）

从这里起的位置，属于另一份文档：那一段「普通访问准入、页表用途、原子能力、对象访问三层展开、整页全称判定」的模型。它回答的是**硬件能否以某次操作触碰某段字节，以及 OS 该不该把这一页交给某个用途**——按 §1.6 的划界属 BootEntry 与分页侧专项，不是内存图交棒的验收条件。第九十六轮把它**逐字**迁到同目录的 `01-design-note-hardware-admission-model.md`；迁移动机与两份未闭合项写在那一份的篇头，公式本身一条没改。

本项只保留三条与交棒直接相关的事实，它们留在 O-12i 前半节：

| 保留项 | 它对交棒合同的意义 |
|---|---|
| 页域与两层划分的定义（`RamSourcePageDomain`、`PmmBaseAvailable(ctx) ⊍ PmmBaseExcluded(ctx)`） | 交接信息必须能被消费者无歧义地量化到页；量化顺序（先兼容性裁决、再并集、再取完整页）本身是来源归一化的一部分 |
| 「用途拒绝不等于基底排除」这条不变量（§3 第 2 条） | 两类结论一旦压成同一个集合，「这一页为什么被排除、被谁排除」就不可追溯 |
| 用途集合是**分配时的凭证**（交付 `(page, U_verified, p)`，不得事后扩大） | 说明交接清单必须能支撑这种凭证；凭证本身的实现属内核侧，不由本项兑现 |
### O-13 固定布局 ABI（第三层）

**先更正一句过头的话**：第三层不是「完全没有现状」。今天已有一份 `#[repr(C)] struct BootHandoffV1`（`os/libs/minix-boot/src/handoff_v1.rs`）：头部含 magic/version/size/arena_len，标量含内核映像与引导区各段地址长度，切换后回填 `root_page/bump_base/bump_next/bump_end`，载荷按**字节长度**给出（`memmap_byte_len` 等），容量是 128/256/12/4/1024 + 来源记录 296 条 + 启动对象 16 条，`ARENA_CAP = 9 * 4096`，并有编译期布局门；地址经 `x0`（AArch64）、`rbx`（x86_64）、`a0`（RISC-V）递交。第二跳的 `#[repr(C)] struct VmBootHandoff` 同样是固定数组＋magic/version，扣减记录容量 `VM_BOOT_HANDOFF_MAX_DEDUCTED = 47`（第一百零一轮按 `3 + 32 + 12` 上界修正，版本随之升到 8），并有尺寸／对齐与九个字段偏移的钉桩测试（第一百零三轮）——**它是真二进制 ABI，不是同一镜像内的普通 Rust 结构**。

未定的是**扩展**，不是基线。因此拆成两项：**O-13a（boot-shim → 内核）**回答现有 v1 布局如何承载来源记录、启动对象身份与生命周期（平台资源描述随射程移交 `04`，不进这条 ABI）（来源记录与启动对象身份**已落码**：`KernelInfo.memmap_sources` + `KernelInfo.startup_objects`，wire v7 的两段独立分区；UEFI 逐描述符记录与配置表 DTB 记录都已接入）；**O-13b（内核 → VM）**回答扣减记录、自由页集合、容量与所有权的传递与失败形状。`test_boot_image_table_layout` 钉的是 boot_image 表的算术（5 个内核任务 + 12 个模块 = `NR_BOOT_PROCS`），**不覆盖内存图语义**，别把它当交棒语义的证据。等 O-12 的子项定型后，按真实消费者设计，不在集合未定时冻结。
**判据**：每个跨界记录有唯一消费者、容量核算与失败形状。**消费者**：内核消费方。

## 6. 证据与复核

**三个提交身份分开写**（「本文对应提交 X」容易被读成「全文快照就是那一版」）：

| 身份 | 记录什么 |
|---|---|
| `CodeEvidenceCommit` | 本节宿主读数、交叉构建与 QEMU 腿实际对应的代码提交 |
| `DocumentRevision` | 本轮正文与档案的版本 |
| `ReviewedRepositoryCommit` | 评审人应拿去核对的完整仓库提交（文档 + 代码）。分支头是移动引用，不能当评审身份；每轮的完整 SHA 与两份 sha256 存档在 `03` 的评审身份表与台账里 |

本轮（第一百二十三轮）的取值：代码证据提交见台账对应笔；文档版本为第一百二十三轮；完整仓库快照（评审身份）以台账登记的合并提交为准。三种身份分别记录，不合并成一个轮次。

### 6.1 宿主读数

```
$ cargo test -p minix-types                 → 341 passed / 0 failed（第一百一十八轮复跑）
$ cargo test -p minix-boot                  → 173 passed / 0 failed
$ cargo test -p boot-shim                   → 103 passed / 0 failed（默认 uefi；14 dtb_copy + 3 配对时序）
$ cargo test -p boot-shim --features test-all → 115 passed / 0 failed（uefi+opensbi）
$ cargo test -p minix-kernel --features mock → 863 passed / 0 failed / 8 ignored
$ cargo test -p minix-vm                    → 548 passed / 0 failed
```

### 6.2 交叉构建（宿主 rustc 1.94.1，与本机 CI 镜像同版本）

```
kernel-image --target aarch64-unknown-none        --features fw-aarch64-none --release  ✅
kernel-image --target riscv64gc-unknown-none-elf  --features fw-riscv64-none --release  ✅
kernel-image --target x86_64-unknown-none / x86_64-unknown-uefi … 由 e5-ds/sysboot 构建链覆盖 ✅
boot-shim    --target x86_64-unknown-uefi / aarch64-unknown-uefi / riscv64gc-unknown-none-elf ✅
```

### 6.3 QEMU 腿（本机 QEMU 8.2.2；四条启动链全覆盖）

| 腿 | 脚本 | 结果 |
|---|---|---|
| x86_64 UEFI 链路 | `test-shim-bootmarks-x86.sh` | **PASS**（`platform DTB absent` → UEFI 唯一来源；arena=0x1dfd5000 conv=13 res=112；进入内核与 scheduler） |
| x86_64 全系统 | `test-e5-ds.sh` | **PASS**（12 个 server 出生，`E5f-LABELS 12/12`，DS 传输链四腿通电）→ 第二跳与 VM `reconcile` 在真机成立 |
| x86_64 双镜像 | `test-sysboot.sh` | **PASS**（多进程引导 + 首次用户态 IPC） |
| aarch64 UEFI 链路（ACPI 模式） | `test-shim-bootmarks-aarch64.sh` | **PASS**（`platform DTB absent — UEFI map is the only source`；`conv=13 dtb_regions=1` 诊断交叉对照） |
| aarch64 UEFI 链路（DT 模式） | `DTB_CFGTABLE=1 test-shim-bootmarks-aarch64.sh`（`-machine virt,gic-version=3,acpi=off`） | **PASS（负例）**（EDK2 把机器 DTB 发布到配置表，但该 blob 所在页不是 `EfiACPIReclaimMemory`；O-11 预检快照拒绝启动（读 header 之前），串口为 `DTB preflight failed: DtbPages(NotAcpiReclaim { page: … })`。三态「存在但不可用」由此获得固件路径；「存在且可解析」的正例只剩宿主单测） |
| riscv64 自引导 | `test-kernel-image-riscv64-bootface.sh` | **PASS**（B1/B2/B3 三点位；`KernelInfo::validate` 第 6 步在真机跑过） |
| riscv64 U-Boot 全链 | `test-riscv64-uboot-shim.sh` | **PASS**（6/6 标记：shim 进入 → MNXBOOT1 表 → 内核+12 模块 → arena → 接收端 → scheduler；wire v7、`decode_sources` 与 `decode_startup_objects` 在真机跑过） |
| riscv64 全系统 | `test-riscv64-boot-full.sh` | **PASS**（12 server 出生，`minix-rs rc: minimal boot script marker`）→ 第二跳与 VM `reconcile` 在 riscv 成立 |
| 全量套件 | `run_all.sh` | **38 passed / 1 failed / 0 skipped / 0 build failures**（第一百一十四与一百一十八两轮在复审/落码后的代码上复跑，读数不变）；唯一红是 `test-rt-birth-aarch64`，在**基线 `537f5c16b` 上逐字同签名**（`elr=0x140003628 esr=0x92000047 far=0x7ffffffff000`），属既有红（证据见 `03` 七十七节） |

**第一百一十三轮复跑**（代码变更后受影响腿必重跑）：`test-kernel-image-riscv64-bootface`、`test-shim-bootmarks-x86`、`test-shim-bootmarks-aarch64` 三条再次 **PASS**；`boot-shim` 五个目标与 `fw-riscv-uboot` 单特性构建均 ✅。

**第一百一十四轮全面回归**（16 笔复审修复之后）：宿主 339/163/82/863/548 全绿；五个交叉构建目标全通过；`run_all` 38/1/0/0；另跑五条非注册腿——`test-shim-bootmarks-x86`、`test-shim-bootmarks-aarch64`（ACPI）、`DTB_CFGTABLE=1`（DT，含 `resv_claims` 核对）、`test-kernel-image-riscv64-bootface`、`test-riscv64-uboot-shim`（6/6）、`test-riscv64-boot-full`——全部 PASS。

**第一百一十七轮（A3 后受影响腿复跑）**：`test-shim-bootmarks-x86`、`test-shim-bootmarks-aarch64`（ACPI）、`DTB_CFGTABLE=1`（DT 负例）、`test-kernel-image-riscv64-bootface`、`test-riscv64-uboot-shim`（6/6）全部 **PASS**——wire v4 跨第一跳、启动对象分区解码与消费者边界在四条链上跑过。

**第一百二十四轮（wire v7 代码上）**：宿主 `341/173/92(+5)/863/548`；五个交叉构建全通过；六条非注册腿全 PASS（x86 bootmarks、aarch64 ACPI、`DTB_CFGTABLE=1` 负例、riscv bootface、riscv U-Boot shim 6/6、riscv 全系统）。

**第一百二十五轮（wire v7，O-11 严格两快照）**：宿主 `341/173/103(+5)/115(test-all)/863/548`；五个交叉构建全通过（kernel-image aarch64/riscv；boot-shim x86_64-uefi/aarch64-uefi/riscv64-uboot）；六条非注册腿全 PASS——x86 bootmarks（`platform DTB absent`）、aarch64 ACPI、`DTB_CFGTABLE=1`（负例在**预检阶段**拒绝：`DTB preflight failed: DtbPages(NotAcpiReclaim { page: 1206870016 })`）、riscv bootface、riscv U-Boot shim（6/6）、riscv 全系统。`run_all` 本轮未复跑（并发会话占用 QEMU）；该套件的测试内核自带 `main`、不经生产 shim，生产 UEFI 链路由上面三条腿覆盖。

**第一百二十二轮（wire v6 代码上）**：宿主 `341/171/92(+5)/863/548`；五个交叉构建全通过；`run_all` **38/1/0/0**（唯一红仍为基线同签名的 `test-rt-birth-aarch64`）；六条非注册腿全 PASS——x86 bootmarks、aarch64 ACPI、`DTB_CFGTABLE=1`（`DtbPages(NotAcpiReclaim …)` 负例）、riscv bootface、riscv U-Boot shim（6/6）、riscv 全系统。

**第一百二十轮（wire v5 代码上）**：宿主 `341/169/92(+5)/863/548`；五个交叉构建全通过；`run_all` **38/1/0/0**（唯一红仍为基线同签名的 `test-rt-birth-aarch64`）；六条非注册腿全 PASS——x86 bootmarks、aarch64 ACPI、`DTB_CFGTABLE=1`（`DtbPages(NotAcpiReclaim …)` 负例）、riscv bootface、riscv U-Boot shim（6/6）、riscv 全系统。

**第一百一十八轮全面回归（wire v4 代码上）**：宿主 `341/169/91(+5)/863/548` 全绿；五个交叉构建目标全通过；`run_all` **38/1/0/0**（唯一红仍为 `test-rt-birth-aarch64`，基线同签名）；六条非注册腿全 PASS——x86 bootmarks、aarch64 bootmarks（ACPI，`platform DTB absent`）、`DTB_CFGTABLE=1`（负例，串口 `DtbPages(NotAcpiReclaim { page: 1206870016 })`）、riscv bootface、riscv U-Boot shim（6/6）、riscv 全系统。

**路径覆盖说明**：五条具体路径里，x86 UEFI、aarch64 UEFI、riscv64 自引导、riscv64 U-Boot 有上述 QEMU 腿；**aarch64 直启目前只有交叉构建记录，没有独立 QEMU 启动腿**。

**证据分层：编译通过 ≠ 启动验证 ≠ 新分支被覆盖。** 上述 QEMU 腿证明了四条启动链、两跳交接、内核消费者边界审计与 VM `reconcile` 的真实运行；它们**没有**走到下面这些分支：

- 配置表 DTB「存在但不可用」的负例已有固件路径（`DTB_CFGTABLE=1` 腿：EDK2 把机器 DTB 发布到配置表，但所在页不是 `EfiACPIReclaimMemory`，页类型预检拒绝启动）；「存在且可解析」的正例只剩宿主单测。AAVMF 在默认 ACPI 模式下不发布 DTB 表（8 条配置表项里没有 `b1b621d5`，日志逐字 `platform DTB absent`），这是 EDK2 的 `PlatformHasAcpiDtDxe` 按 ACPI 可用性二选一的行为，不是固件缺件；
- UEFI 真实固件的描述符分布达到 256 条上限的边界（容量由宿主测试与预检覆盖）；
- `Excluded`/`DevicePath` 描述符在真实固件上的分布（它们不产生记录，宿主测试覆盖计数）。

### 6.4 仍需补的证据与未落码项

- OVMF/AAVMF 的实际镜像身份与哈希，以及代表性描述符的 `Attribute` 原始值——要证明运行固件实际用了哪套编码。`Attribute` 已进入来源记录（wire v3 起，`WireSourceRecord` 偏移 24）；仍未做的是运行固件原始值的取证。
- 配置表 DTB 的可控正例：本机 AAVMF 在 `acpi=off` 下会发布 DTB，但所在页不是 `EfiACPIReclaimMemory`，只能作负例；正例需要能控制 DTB 页类型的固件配置或可注入配置表的固件变体。
- O-11 的复制链**已完整落码（第一百二十五轮，严格两快照）**：`note` 只记地址；`preflight` 在预检快照下验证 header 页、读一次 40 字节、分配副本；`authorize_and_copy` 在最终快照下复核全范围、只复制 body 一次且不再分配。三条 DTB 直启腿本就复制进自有映像/arena。

**事实分级的两侧口径**：RISC-V 与设备树的规范原文由本方从固定 commit 的源文本直取；UEFI 侧三项——`EfiReservedMemoryType` 的「Not usable」、`GetMemoryMap()` 对无物理支撑地址与有物理支撑但 OS 不应访问的两段区分、§4.6.1.3 的 DTB 表位置要求——由**评审人按 UEFI 2.11 独立核对**；本方环境抓取 uefi.org 返回 403，按第三方核对引用，不升级成本方已验证。

**复核命令**：`git rev-parse HEAD`、`git status --short`、§6.1 的六条 `cargo test`，QEMU 腿用本目录 `run_all.sh` 或单腿脚本（脚本头部列明前置与退出码）。读数发布后如有新提交必须重跑。

**测试通过不等于输入完整性**：上面 341/169/92/863/548 条说明的是已覆盖的行为，不能证明所有 DTB 输入都被解析、所有原始访问点都被登记、每条启动路径都用了正确的权威来源——这几项各自需要独立的完整性论证（O-07、O-11、O-12）。

## 7. 签字状态

| 层 | 结论 | 理由 |
|---|---|---|
| 第一层：语义与方向 | **可签** | §1.6 三合同划界；来源事实／冲突裁决／纯集合投影三层分离；保留语义（普通／`reusable`／`no-map`、MRB、UEFI 类型）各自独立 |
| 第二层：来源完整性与交接判据 | **待评审方签字**（本方复核：五项验收全部闭合） | 来源身份三元组、地址域、296 条 ABI 与权威审计、启动对象写前证明、O-11 严格两快照顺序均已落码；O-11 的时序证据见 §6.1 与 `dtb_copy`/`ebs_pair` 测试。签字与否由评审方按落码复核。 |
| 第三层：固定布局 ABI | **不签（此前的「缺扩展」理由已不成立）** | `BootHandoffV1` wire v7 已承载来源记录（296）与启动对象（16）两段独立分区，`VmBootHandoff` 版本 8 的尺寸/对齐/九个偏移与失败语义均已钉桩；层结论待评审方复核，不再是「缺扩展」。 |

**第一百一十三轮（实现复审修复，不改变层结论）**：双轴复审提出的两项 fail-open 与四项合同/文档不一致已闭合——① 截断或零长的 `reg` 不再被跳过（四条负控）；② 非 `no-map` 保留与 MRB 的页级冲突拒绝（三正一负）；③ 跨镜像 resume 走 `validate_requiring_sources`，三条 DTB 腿交出前断言来源非空；④ FDT 容器 MRB 上限回到 16；⑤ O-11 静态保留 × EFI 类型覆盖落码（`resv_claims`，三条必测形态）；⑥ 注释/引用同步与 I-29 触及文件的 rustfmt。读数见 §6.1/§6.3；仍开放的是 O-11「复制到自有缓冲再解析」与真机固件证据。

**第一百一十五轮（A1 落码，不改变层结论）**：O-11 的 copy-first 与页类型预检落地；AAVMF 强制 DT 腿由正例改为「存在但不可用」负例，正例只剩宿主单测。

**第一百一十六轮（A2 落码，不改变层结论）**：记录身份与 `Attribute`/`VirtualStart` 进入 wire v3；`MapSnapshotId` 为本次 `GetMemoryMap` 的 MapKey；跨快照同号描述符可区分。

**第一百一十七轮（A3 落码，不改变层结论）**：启动对象记录进入独立分区（16 槽 × 72 B，wire v4，版本 4）；`KernelInfo.startup_objects` 与 `STARTUP_RECORD_STORE` 在四条腿落地；启动对象记录不参与内存图来源覆盖审计。第二层验收项 2 与 5 已按代码更新，三层结论不变。

**第二层的验收内容已按 §1.6 收紧**，只保留真正属于「资源发现与交接」的五项（平台资源描述随射程移交 `04`，不再作为签字项）：

1. **输入真实性**——**provenance 与 reachability 分开**：I-29 负责各启动 ABI 的**对象身份、地址域、授权范围、阶段与失效点**，以及可复核的来源依据（provenance）；「该地址在当前阶段是否端到端可达、是否满足非受控推测访问限制」（reachability）由 BootEntry 与各架构 adapter 负责。**配置表 DTB 指针存在，不能推出它可读**——即便 UEFI 规定该指针按物理地址解释且 DTB 必须位于 `EfiACPIReclaimMemory`，可达性仍要单独证明。aarch64 UEFI 配置表 DTB 的三态规则已落码（不存在→仅按内存图；已解析→`/memory` 必须覆盖每条 conventional 跨度，否则 halt；存在但不可用→halt），实现为 `uefi_helpers::cross_validate_config_dtb`，两个 UEFI 入口各调一次。真机覆盖按 A1 之后的口径：默认 ACPI 腿验证「不存在」；`acpi=off` 腿下 EDK2 把 DTB 发布在非 `EfiACPIReclaimMemory` 页，验证「存在但不可用」的拒绝（`DtbPages(NotAcpiReclaim …)`）；「存在且可解析」的正例今天只有宿主测试（固件无法被外部改造成该形态），见 §6.3、§6.4。
2. **来源保真**：每条启动路径的 `SourceRecord` 生产、保存与**传递**。三条 DTB 政策路径（riscv bootface、aarch64 直启、riscv U-Boot）都生成记录、在投影产生后交棒前审计、并写入 `KernelInfo.memmap_sources`；U-Boot 腿的记录经 v1 来源分区跨镜像，接收端落进内核 `.bss` 后由 `KernelInfo::validate` 第 6 步在消费者边界重跑覆盖审计。**UEFI 的逐描述符记录已接入**：EBS 配对循环为每个产出入普通表的描述符写一条 `SourceOrigin::UefiDescriptor { index }` 记录（范围=分类器实际发出的区间），接收端与 DTB 腿走同一个 `decode_sources` 与消费者边界审计；容量 256 = `MAX_MAP_DESCRIPTORS`。`Excluded`/`DevicePath` 描述符不产出投影字节，因此没有记录（计数单独打印），这是唯一登记在案的省略。
3. **冲突与生命周期**：DTB／UEFI 静态保留的交叉验证、MRB、`no-map`、类型政策各自语义独立；首读与解析后拒绝两个决定已分开（O-04／O-11 的冲突已消）；O-01 第一版采静态处置表 + 损失声明，`ReservedReusable` 保持独立类型。
4. **分配所有权**：候选集与交付集不得混称。§2.8 的时间线已证明两件事——**候选集合包含 bump 池**、**首次分配早于任何扣减**；UEFI 腿的根页与 bump 靠 `assert_bootstrap_outside_memmap` 挡在候选之外，**自引导两腿的根页与页表 bump 都取自 `/memory` 内的固定池，按现有披露事实应视为已落入候选集合**（除非补出 `select_multi` 内部的过滤证据）。验收三枚：`BootBumpRange ∩ FirstVmBootAllocatorCandidates = ∅`、`BootRootPage ∩ FirstVmBootAllocatorCandidates = ∅`、`FirstVmAllocatedPages ∩ (BootBumpRange ∪ BootRootPage) = ∅`；第三枚是结果断言，前两枚替代不了。**消费边界的证明边界**：`reconcile` 只证明「接到的扣减集合与自由集合不相交」并抽查根页/模块在记录内，不证明集合完整；完整性由内核 `classify` 的枚举合同与测试负责（第一百二十轮明确写开，避免把内部自洽读成覆盖完整）。**第一百零四轮已补**：`test_first_vm_allocation_never_lands_in_bump_or_root` 走生产的 `select_multi` + `VmBootAllocator`，取尽 703 页并断言全部避开 bump 与根页。另加 `FREE_MEMMAP` 去留裁决（它零读者、也不是候选输入）。**第一百零一轮已落码**：排除清单从「内核 + 模块」扩成 `vm_boot_exclusions`（内核、模块、根页、bump，容量 `NR_BOOT_MODULES + 3`），真实与 mock 两个分支共用；单测 `test_vm_boot_exclusions_cover_kernel_root_and_bump` 覆盖四类来源与缺省情形。**已补**：第三枚结果断言就是 `test_first_vm_allocation_never_lands_in_bump_or_root`（真实 `select_multi` + `VmBootAllocator`，703 页全部避开）；QEMU 四条启动链与全系统腿已跑（§6.3），第二跳与 VM `reconcile` 在 x86 与 riscv 全系统腿上成立。
5. **容量与 ABI**：接受上限是 `min(256, floor(缓冲/40)) = 256`，**不是**缓冲能装的 524；来源记录按两类拆分：`MemorySourceRecordCapacity`（UEFI ≥256、DTB 40 含 8 条固件前缀、40 槽之外的派生）与 `StartupObjectRecordCapacity`（内核映像／模块／页表池／根页／arena／其它）；启动对象记录自 A3 起有独立分区（16 槽 × 72 B），派生记录仍在内存图分区的 256 槽内。第一跳 arena 为 9 页：来源分区 **296 条** × 72 B = 21,312 B（256 UEFI 描述符 + 40 配置表 DTB 记录）、启动对象分区 16 条 × 72 B = 1,152 B（wire v7，余 384 B）；两段分区都在写之前拒绝超限（`TooManySourceRecords` / `WIRE_STARTUP_CAP`），且有编译期断言 `296 >= 256 + 40`。第二跳的 `deducted` 按来源记账（`push` 只做页取整，不交集、不合并），上界 `3 + 32 + 12 = 47`。**第一百零一轮已落码**：`VM_BOOT_HANDOFF_MAX_DEDUCTED` 由 32 提到 47、`VM_BOOT_HANDOFF_VERSION` 7→8，并加编译期断言 `>= 3 + MAX_BOOT_REGIONS + NR_BOOT_MODULES` 与两枚边界测试（`test_deduction_capacity_accepts_full_bound`、`test_deduction_capacity_overflow_is_fail_stop`）；**第一百零三轮已落码**：`VmBootHandoff` 的尺寸、对齐与九个关键字段偏移已用 `test_vm_boot_handoff_layout_is_pinned` 钉死；第一百零八轮已补齐：交接页合同（物理帧来源、生命周期、little-endian、3,792 B ≤ 一页）写入 §2.6；`magic`／精确 `version`／根页／三类容量的 fail-stop 由七枚测试钉住；两个固定用户 VA 用编译期断言压在 2^38 以下。「超限整体拒绝不截断」**已实现**。第一跳两段来源分区与 `KernelInfo` 两个承载字段已按上段落码（wire v7，版本号精确比较，超容量与坏记录都失败关闭）。

**已登记的实施清理（状态更新）**：① 三条 DTB 腿已全部改用 `collect_with_sources()`，`collect()` 只剩 UEFI 诊断路径；② `FREE_MEMMAP` 已删除（第一百零六轮）；③ `VmBootAllocator` 的构造参数仍是原始候选区集合，排除依赖每个调用点调 `vm_boot_exclusions`（未改）；④ `Attribute` 不进入来源记录、也没有诊断输出，但分类器五步裁决会读取它（`AMBIGUOUS`/`PROFILE_ONLY`/`ISA_*` fail-closed，`WRITE_BACK` 是普通映射准入）——未做的是「记录与诊断」，不是「读取」；⑤ 已落码（47 + 两枚边界测试，第一百零一轮）；⑥ 启动对象已独立 16 槽（A3）；派生记录仍与 UEFI/DTB 记录共用 256 槽，其余类别未单列；⑦ 登记（第一百一十三轮复审）：`collect_inner` 单函数约 350 行，MRB 与 `/reserved-memory` 两段 span 裁剪样板重复——非阻断，按「非阻断发现登记、不追改」留待专门重构。

**驱动访问点审计（每个使用点的 VA／宽度／读写方向／参与 CPU）不在本项**：§1.6 的边界声明已把它与 `DeviceResource` 分开，O-07 的实际访问完整性不再算作本项第七项阻塞。**列为外部依赖、不影响本项签字状态**：叶子粒度与拆分／降级路径、PTE 的 A/D 更新方案、页表访问的 PMP/PMA 证明、LR/SC／AMO／预约集等原子能力、根激活前后的审计时序与 `FrozenBootMappingSet` 的冻结字段。这些材料在 `01-design-note-hardware-admission-model.md` 与 `03` 六十五节里继续存在，其中两条已知未闭合（共享原子合同仍按 LR/SC 分支；跨页源级访问的整体覆盖规则未进入逐操作判定式）已写在那一册篇头。

**现状与合同相反的一条（不是缺口，裁决已下）**：低 4 GiB 恒等映射不消费 `no-map`、MRB 与类型禁止政策，普通叶子带 `EXECUTABLE`，且今天的启动流程没有对该区间执行拆分或撤销的调用路径（§2.3）。O-02a 裁定甲案（授权驱动建表）、否决乙案与半拉子形态；其四段依赖链中，**本项只交付第一段（来源进入政策）**，第二、三段属映射政策消费者，第四段属分页专项——这条归属划分不改变「输入边界与来源完整性是它的前置证明」。

## 7.1 冻结规则（第一百轮写入，作为后续评审的准入）

**一、内容冻结**：本项交付固定为三件事——物理内存图、启动对象物理占用、两跳交接。设备发现、设备映射、硬件准入与驱动访问审计不再进入正文。任何新建议先回答「它是否直接影响这三项之一」；不直接影响的登记到对应专项或 backlog。确需新增同级合同时，**优先替换或修订现有条款，净篇幅不增**。

**二、意见分诊**：后续只接收三类问题——① 正文内部矛盾（同一事实、公式、责任或容量给出不兼容结论）；② 与代码／ABI 事实冲突（合同声称与实际实现不一致，或代码证据不支持主张）；③ **本项自己的**交付缺口（缺该事实就无法判断内存图、启动对象占用或两跳交接）。第三类不得替换成「下游实现这个功能需要更细的设计」。

**三、停止条件**：本次裁剪与必要合同修订完成后，**设计评审循环冻结**。只有三种情况可重新打开：实现与已冻结合同不一致；实际代码揭示合同遗漏了不可避免的输入或失败形态；项目需求发生变化。命名更漂亮、模型更完整、别的 OS 可能需要某能力，都不构成重开理由。**冻结的是设计范围与合同，不禁止修复实现缺陷。**

冻结之后的三件代码工作已全部落码并经真机回归：来源记录接进四条启动路径并经第一跳 ABI（wire v7）交给内核政策消费者；首次分配前对 bump／根页／其它存活对象的排除；扣减容量按真实记录语义定为 47 并按上界修正。本项此后只做合同-实现对账与真机回归，不再重开设计。

「禁止集合编译 + 引导根审计」即便有代码，也只能作为第一轮实现复审的起点。在那之前，对 §5 的评审只需回答两个问题：问题是否成立、候选是否覆盖失败形态。
