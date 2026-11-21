# 01-design.v4 — 引导交接载荷（2026-11-21 重写轴）

> 状态：DESIGN；轴：01 = 交接载荷；02 = 内核镜像。
> 前序：`01-design.v3.md`（事实来源，非结构契约）。

## Ch1. 核心抽象

内核接手时只需要三份数据，外加一条接缝：

1. **可用内存清单**：`memmap` 与 `reserved_regions` 两张表，加来源记录。
2. **内核位置记录**：`kern_virt_base`/`kern_phys_base`/`kern_size`/`entry_point`；镜像如何摆放归 02。
3. **交接结构**：`KernelInfo`（进程内）与 `BootReport`/wire（跨镜像）。
4. **接缝**：`pre_init` 的 `get_parameters` 属 01，`pg_*` 五步属 02；`head.S` 第一跳属 01，第二跳属 02。

## Ch2. 四条不变量

| ID | 不变量 | 判据 | 失败落点 |
|---|---|---|---|
| I1 | 值可跨镜像，指针/句柄不可 | wire 只写固定宽度标量与字节分区；接收侧重建切片 | 把 `&'static` 指针写进 wire |
| I2 | 来源可追溯 | 每个投影字节有 `SourceRecord`；`audit_source_coverage` | 覆盖审计报 `FreeSpanWithoutMemorySource` |
| I3 | 所有权明确 | `BootStorage` 单所有者；`fill_*` 一次提交 | 第二次 fill panic；slice 活过 backing |
| I4 | 校验先于消费 | `validate_before_tables` → `store_kernel_info` → `validate_requiring_sources` | 非法布局进入建表 |

## Ch3. 决策清单

| # | 决策 | 理由 | 当前落点 |
|---|---|---|---|
| P-1 | `KernelInfo` 按值交接，不做跨镜像 ABI | 两个镜像各有 `.bss`；wire 固定宽度 | `kernel_info.rs:struct KernelInfo`、`handoff_v1.rs:struct BootHandoffV1` |
| P-2 | `RamAuthority` 区分输入对象与 RAM 权威 | `InputObjectId` 是来源身份，权威是子集 | `memmap_source.rs:struct RamAuthority` |
| P-3 | 两张内存表分离 | free 与 reserved 的消费语义不同 | `kernel_info.rs:struct MemoryRegion` |
| P-4 | `SourceRecord` 带 provenance | 投影必须可解释 | `memmap_source.rs:struct SourceRecord` |
| P-5 | `BootStorage` 单所有者 + `fill_*` | 半提交不可表示 | `boot_storage.rs:struct BootStorage` |
| P-6 | 分配器换手用 `init`/`resume` | 跨镜像续游标，不重发页 | `boot_alloc.rs:fn init_boot_pt_alloc` |
| P-7 | 直核面共享 `run_self_boot` | 流程单源，两壳只留常量 | `bootface_common.rs:fn run_self_boot` |
| P-8 | 五条生产腿按发送语义收敛 | 腿的差异在取件来源，不在载荷形状 | `loader.rs:trait FileLoader` |

## Ch4. 接缝声明

- 01 管：取到字节、校验、记账、构造载荷、发送。
- 02 管：把字节解释成镜像、摆放、映射、进入。
- 01 不写页表建立/映射细节；02 不写 wire 字段与来源覆盖。
- `pre_init`：`get_parameters` 属 01，`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`/`vm_enable_paging` 属 02。
- `head.S`：`load_stack_start → pre_init` 属 01；`k_initial_stktop → kmain` 属 02。
