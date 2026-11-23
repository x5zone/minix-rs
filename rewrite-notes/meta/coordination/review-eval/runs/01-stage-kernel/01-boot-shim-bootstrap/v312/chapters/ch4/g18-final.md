DOC: 01-boot-shim-bootstrap.md
CHAPTER: 4. 实现详解
HARD-REJECTIONS: 0
VERDICT: READY

输入核对：切片 sha256 6581f240…cfda（440 行）与指定值一致；只读切片、g18.md、章契约、scope S4。
三处 fact 修复已核对：§4.2 入口不共享函数、取件共用 FileLoader trait；§4.3 tty/mib 互换改为
place_modules 运行时 basename 核对停机；§4.10 预期改为内核侧 16 行、镜像侧 6 行；kmain 门链
表述仍与 §4.4.1 一致。均未触动 H1–H3 的结构判据。

H1（路标图缺 shim→enter_kernel_image_v1→接收镜像的边）：已消除，本轮未改动。
- §4.1 图 43–46 行保持完整链：交接行以 enter_kernel_image_v1 收尾（43）→ 跳独立 kernel.elf
  高半入口（44）→ arch_boot_resume_v1 解码六分区进 .bss→strict→resume_additive（45）→ kmain（46）。
- 非阻断：该标签在交接行与同级节点各出现一次，44–46 行挂在“UEFI 固件”根下；建议加“跳转后”标注。

H2（bash 最小示例用 `//` 且无预期输出）：已消除，本轮预期进一步量化。
- §4.10 围栏为 bash（417），注释统一 `#`（418、420–421），命令以 `$ grep` 起头（419）；`# 预期`
  块现为“内核侧 16 行 boot_stage!（Step 1/2、Step 4 与 kmain 各 Phase）；镜像侧 6 行
  "### minix-rs kernel-image"（每个 cfg 产品一行）”，与 424 行说明一致。命令可跑、结果可数。

H3（编号体系自撞：2a/2b 与 Step 2b、A+D、D1–D5 悬空）：已消除，本轮未改动。
- §4.1 六套编号表（17–26）仍完整：Step 0–4、动作 1–9、时间拍 1–10、防线 A–D、Step 2b/2c、D1–D5；
  旧“2a”全文已无；Step 2b/2c 定义为 Step 2 内部两小步（25、51）；A/B/C/D 在 24 与 175 双处声明为
  装载防线；D1–D5 在 26 与 175 双处标注“第 2 章的次序约束”。非阻断：2b/2c 前无 2a。

整章冷读：13 个单元（含 4.2.1、4.2.2、4.4.1）节序连贯，主线可跟读；TR-0 问题入口（4.0）、前置
概念、最小示例（4.10）、失败对照（4.9 及 §4.3/4.5/4.6 的反事实）、小结（4.11）、术语（4.12）齐全。
scope S4 五项均可走通：4.1/4.6 的 main→prepare_and_enter_kernel_image→enter_kernel_image_v1→
arch_boot_resume_v1；4.2.1 九动作表的读文件/ExitBootServices/提交计数/跳转；4.4 两条 KernelInfo
产出入径；4.5 六个落地区；4.4.1 逐腿门链。三处 fact 修复未改变 S4 五项的可达性。
残留 MINOR（不构成 HARD，未计入 HARD-REJECTIONS）：
(a) 439 行“三个动作 ID”应为“两个动作 ID、三个动作段”（动作 2 出现两段）；
(b) 350 行“三者分属‘内核落地区’‘wire’‘VM 交接’”按位次读会把 arena 对到内核落地区，建议显式
写“arena 属 wire、BootAlloc 属内核引导期、VmBootAllocator 属 VM 交接”；
(c) 175 行仍称两个直核腿有“NAME_POOL 与 MODULE_NAMES 的编译期逐字节对账”，与 231 行新事实
（编译期常量只核对池截断/NUL，拦不住顺序互换；实际由 place_modules 运行时 basename 核对停机）
不一致；建议把 175 行改为运行时核对。此为 fact 措辞残留，不阻断主线，建议冻结前修。
