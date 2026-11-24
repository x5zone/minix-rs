# chunk-5（L1201–1500）跨章接缝证据

范围：01-boot-shim-bootstrap.md 第 1201–1500 行。只记证据，不判 verdict。

- [术语·字母复用] L1349 “本节的 A/B/C/D 是装载防线，不是第 2 章的次序约束 D1–D5” —— 同文件第 2 章 L540 “启动期的五条次序约束 D1–D5”；两套 A/B/C/D 字母表同篇并存。
- [术语·同一调用者两名] L1461 “两个 `bootface` 在调 `arch_boot` 之前显式调用” vs L1467 “`run_self_boot` 在 `arch_boot` 前调 strict”；L1034 称流程由 `bootface_common::run_self_boot` 承载。
- [计数·表行超出视图] L1422 “九个访问器覆盖 … 与三个镜像跨度字段”，L1431 表却列 `authority`；os/libs/minix-boot/src/kernel_info.rs:188-197 的 trait 确为九项且无 `authority`，L1417 它是 `store_boot_report` 的独立参数。
- [术语·裸名指代不明] L1390 “`validate` 只查 `boot_modules` 非空”，而 L1453 “`validate_before_tables` … 不查 `boot_modules` 非空”、L1455 `validate_requiring_sources` 才查；裸 `validate` 属哪档门未指明。
- [术语·实例名漂移] L1288 “两个 `BootAlloc` 实例不共享游标” vs L1544 “两个 bump 实例共享一套算法”、L1069 “它与 `BootAlloc` 是两个独立游标”；shim 侧实现名是 `bump_alloc_pages`/`BumpRegion`。
- [自相矛盾·定义] L1336 “`pa_limit` 是页表地址宽度，不是 RAM 顶” 紧接 “这个界是 `min(BOOT_IDENTITY_MAP_END, VM DM 窗口的物理上界)`”；前句为宽度常量，后句为两条业务上界取小。
- [命名·常量与字段混列] L1322 “四类容量上限（`WIRE_MEMMAP_CAP`、`WIRE_RESERVED_CAP`、`WIRE_SOURCES_CAP`、`side_cap`）”——前三为 `WIRE_*` 常量，末项为蛇形字段名。
- [术语·一对三] L1334 诊断串 “`boot-shim: memmaps conv=… reserved=… unmapped=…`” 把同段列出的 `Excluded`/`Side`/`DevicePath` 三类合并称 `unmapped`。
- [表与正文动作集不一致] L1265 “配对循环体内另有一处 DTB 头 preflight…`authorize_and_copy` 也在最后一次 `GetMemoryMap` 之后”——二者均未出现在 L1271–L1282 的“10 拍对应 9 个动作”表内。
- [措辞与表头张力] L1227 “两条固件路径只留四行对照”，表头却是 “UEFI 路径 | 非 UEFI 路径”，非 UEFI 列含直核面（L1231 “直核面：内核镜像自带的引导面”）。
- [抽象层混列] L1426 同格并列 “U-Boot 走 `argv[0]`，riscv64 直核走 `a1`，aarch64 直核读固定 `DTB_PA`”；`argv` 的来历只在 L1081 的 `(extra_argc, extra_argv)`，本节未复述。
- [悬空否证] L1310 “分类不是‘类型标签加 RAM 顶两遍扫描’”——被否定的旧说法在本篇内无出处与锚点。
- [表述歧义] L1436 “两者相差 0 到 2 MiB 减 16”——区间端点与减数归属可读成 0..(2 MiB−16) 或 (0..2 MiB)−16。
- [计数口径] L1486/L1497 §4.5 标题 “六个落地区”，L1497 正文却为 “六个落地区、一个名字池、一行 `KernelInfo.memmap` 切片视图与两个 `VmBootHandoff` 交接字段”；同一张“唯一权威”表承载三类对象而标题只计一类。
- [数字重载] L1372 “`ds` 在位置 0，进程号是 6” 与 L1405 “位置 6 … 绑到 `tty` 的进程号上”——相邻段落里“位置 6”与“进程号 6”指不同对象，未加区分词。
- [跨章转场] L1225 “Step 0–4 在本章立住…（概念入口见 §4.8）” 把 Step 编号与 §4.8 前向引用绑于一句；§4.8 实存于 L1575，指向可解。
