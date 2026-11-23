# Ch2 摘要 — C 侧从引导器到 kinfo 的交接路径

- 文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- 章：2. C 源码分析
- 章切片（纯正文，extractor 产出）：`ch2-final-207a12ef.md`，sha256 `207a12effbb4712b60a91da79da9305149dde96fb68cf2d31769d7d0fa8eb8b3`（953 行）；
  复核快照 `ch2-final-08f0715e.md` 是首轮 fact 的基线；207a12ef 是 G18/编辑/Reader-B 修复后的重排（2.5.1 模块归属移到 2.5 之后），事实点未改。
- 用途：Ch3/Ch4 作者与 review 的上一章输入；不要求读 Ch2 全文。

## 关键 C 函数行为

| 函数/路径 | 一句话行为 | 锚点 |
|---|---|---|
| `boot.cfg` | `load_mods` 必须排在 `multiboot` 前；反序时停在 `assert(mbi->mi_mods_count > 0)` | §2.1；`boot.cfg.default:4`、`boot2.c:146,151,153`、`pre_init.c:167-169` |
| `head.S: multiboot_init` | 设临时栈、清帧指针、按调用约定把 magic + `mbi` 交给 `pre_init` | §2.2；`i386/head.S:37-39,68-77` |
| `get_parameters` | 整份 `memcpy` `mbi`；写链接期常量；解析命令行/模块表/内存图；返回 `&kinfo` | §2.3；`i386/pre_init.c:94-215` |
| `mb_set_param` | 逐词扫描命令行，写 `kinfo.param_buf`；空键/越界/忽略返回值是 C 侧边界行为 | §2.3；`pre_init.c:35-75,144-150` |
| `add_memmap` | 入口 `assert(mmap_size < MAXMEMMAP)`；写条目时把 `mmap_size` 抬到 `m+1`；起点向上、长度向下取整到页 | §2.4；`pg_utils.c:97-120` |
| `cut_memmap` | 切除命中区间；左右余量各调一次 `add_memmap` 补回；起终点向外取整 | §2.4；`pg_utils.c:32-63` |
| `overlaps` | 半开区间相交检查；相切不算重叠；i386 侧调用在 `exec.c:753` | §2.5；`pre_init.c:77-92` |
| 模块三层检查 | `boot.cfg` 次序、`HAS_MODS` 标志、`mi_mods_count > 0`；失败现场不同 | §2.1/§2.6 |
| `alloc_lowest` | 从最低条目切一段；四前提成立时落“第三行常态”（`len ≤ lowest`，不切） | §2.4.2.3；`pg_utils.c:65-84`、`arch_smp.c:76` |

## 启动次序 D1–D5（约束名，不是执行序）

- D1 内存清单先于建表；D2 内核位置先于清单裁剪；D3 页表先于开启分页（02 篇 J3）；
  D4 结构先于 `kmain`；D5 恒等映射先于内核映射（02 篇 J3）。
- 五条是约束名；证据在 §2.8 与 02 篇 J3。
- 锚点：§2.8「D1–D5」列表。

## 模块与内存账

- 模块计数与归属：`NR_BOOT_MODULES = INIT_PROC_NR + 1 = 12`；`kmain` 硬检查
  `NR_BOOT_MODULES == kinfo.mbi.mi_mods_count`；`image[NR_BOOT_PROCS]` 共 17 项
  （5 内核任务 + 12 用户态），不是 12 项模块表。
- 内核把自己算成第 M+1 个模块；`overlaps` 检查与 `cut_memmap` 切除逐条交错。
- 内核账不归还除 VM 外的模块字节；VM 接手后用 `free_mem` 回收。C 侧真实行为，
  Rust 侧不必逐字节复刻。
- 满槽：`mmap_size == 40` 时，切除后的第一次 `add_memmap` 在入口
  `assert(mmap_size < MAXMEMMAP)`（`pg_utils.c:97`）停住；`panic`（`pg_utils.c:120`）
  只在 `-DNDEBUG`/`MKSMALL=yes` 下可达。
- 锚点：§2.4.2.5；`com.h:74`、`main.c:160-162`、`table.c:44-65`。

## ARM32 对照

- `earm/head.S` 与 `i386/head.S` 同名入口/函数，差异在参数来源与 MMU 寄存器。
- ARM32 用 `setup_mbi` 自己造 `multiboot_info`；`overlaps` 在 ARM32 是闭区间。
- 锚点：§2.7；`earm/head.S:21-55`、`earm/pre_init.c:167-216`。

## Rust 必须保住的外部行为

- 引导器契约的三样对接：识别头、寄存器约定、`mbi` 结构体。
- `boot.cfg` 的 `load_mods` 先于 `multiboot`；反序的失败现场。
- 内存图的来源（细表/粗档）、页对齐与夹取、相切不算重叠。
- 内核与模块占用区必须从可用清单里切掉；D1–D5 的次序。
- 模块数 12 的硬约束与 `kmain` 的失败现场。

## Rust 可以替换的机制

- `get_parameters` 的整份拷贝方式、`mb_set_param` 的扫描实现、`add_memmap`/
  `cut_memmap` 的数组槽位管理。
- 模块账的“不归还”细节；Rust 侧只需保住“谁占用、谁可用”的外部结果。
- ARM32 的 `setup_mbi` 构造方式；Rust 侧可用自己的平台描述。

## 原文锚点

- 章首问题与前置：§2.0 第 3–15 行。
- C 行为总表：§2.1–§2.7。
- 归纳与 D1–D5：§2.8。
- 失败对照/最小示例/小结/术语：§2.8 末尾四小节。
