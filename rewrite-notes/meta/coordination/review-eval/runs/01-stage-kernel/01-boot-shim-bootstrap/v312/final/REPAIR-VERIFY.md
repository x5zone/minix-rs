DOC: 01-boot-shim-bootstrap.md
SHA256: 5023b4543f55a1f37ed2105b4b70d617ebdcd50ce463484e6f1f9c3477abd5d2
SCOPE: REPAIR-VERIFY R1–R6（R1 复核轮）
VERDICT: PASS

BASELINE: sha256sum 与 wc -l（2020 行）均与 R1 修复后声明一致。

R1: CLOSED — L43「承载内存图与其他启动参数」、L45 两处「承载内存图与其他必要启动参数」、L62「交接结构检查承载内存图的记录本身」、L94「描述这三段区间与必要启动参数的记录」、L121「承载内存图与其他必要启动参数的交接结构」、L137 术语同；Ch1 全文已无「交接结构承载镜像位置」或「承载前两类/两者/它们」的定义（L45 仅存「前两类数据是内容」，属内容分类，不涉承载）。L45/L106 与 Ch2 L281「C 侧不通过交接结构传它」一致：镜像位置由链接布局重建。
R2: CLOSED — Ch1 术语 L131 无 `kinfo`（全文首个 `kinfo` 在 L162/Ch2）；Ch2 L166 改为「本章会用到 `struct kinfo`」；L581 术语表有「内存账」；`首次出现` 零命中。
R3: CLOSED — Ch5 L1627、Ch6 L1730 均写「第 1 章的 I1–I4 与第 3 章的五条生产腿」。
R4: CLOSED — 章表 L13 的 Ch4 行已无「换栈」（只剩装载、建根、映射、跳转）；`换栈` 仅命中 L4/L84/L284/L548/L2016，Ch4 正文（L1176–1616）零命中；L548 判据指向 02 篇 §2.5，02 §2.5（L518–556）标题即落段判定并附三行证据表。
R5: CLOSED — `下一篇` 零命中；L4、L76 首用均为 `[02 篇（镜像篇）](02-higher-half-kernel.md)`；L604 给出「02 篇」的文件名定义，可解析。
R6: CLOSED — L744/L1082 → 02 §2.2（两份 `pg_utils.o`，02 L184/L201）与 §2.4（`pagedir`，02 L334/L366/L398）；L548/L885 → 02 §2.5；L911/L2010 → `pg_utils.c:alloc_pagetable`（C L123，6 页静态池与 `panic("no more pagetables")` 实测命中）；L938/L946 → `memory.c:createpde`（C L69）/`virtual_copy_f`（C L592），`freepdes[]` 自增见 C L711–712；L1074 只称 02 §3.6/§4.5 确认「跳转前完成」（02 L997/L1595/L1602；02 §4.3 L1340 自述 DM 不展开）；L2009 `freepde_start` → 02 §2.4（02 L321 有 `kinfo.freepde_start = pg_mapkernel()`）。
R6-NOTE: L744 括注中的「换根」动作本身未被 02 §2.2/§2.4 展开（两节只支撑「两份 `pg_utils.o`」与 `pagedir` 内容）；作旁证可读，不计为阻断。

NEW: 无。前轮 NEW-1（五处「承载前两类」）与 NEW-2（L94 示例前提）已随 L43/L45/L62/L94/L121/L137 的收口消除，未观察到其他新矛盾。
