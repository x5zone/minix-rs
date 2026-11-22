# 02-stage 01 R1 缺陷台账（V3.12）

- edition: V3.12
- 轮次: R1 full
- 归并口径：按根因合并五份面板报告；括号内为原始发现 ID。
- 全部行 `处置类 = ①`（本轮修复），复验见每行命令/门读数。

| 发现 ID | 单元 | 处置类 | 处置 | 复验 | 根因 ID | origin |
|---|---|---|---|---|
|  READER-A-02、READER-B-15、READER-C-01、DESIGN-D2-06  |  1  |  ①  |  统一 `register_page_alloc` 到 `init()` 开头；重写 §4.0 失败对照为“构造期注册、随后移动”  |  `rg -n 'register_page_alloc' os/servers/vm/src/vm_server.rs`；`cargo test -p minix-vm --lib` 549/0  |  R1-01  |  R1  |
|  READER-A-01、READER-B-13、READER-C-07  |  1  |  ①  |  统一 `mark_initialized` 末尾时序；区分“同一职责”与“同一时刻”；补 `pt_init` 构造期理由  |  `rg -n 'mark_initialized  or  阶段划分' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-02  |  R1  |
| READER-A-05、READER-C-02、READER-C-03、READER-C-04、DESIGN-D2-02、READER-C-08 | 1 | ① | 补 `BootRoute` 四态真值表、当前可达列；写明内核恒写 `is_first_time=1`、仅 `ColdInit` 可达；补 `WarmRun` 前置条件 | `rg -n '当前可达  or  恒写 1  or  WarmRun' rewrite-notes/02-stage-vm/01-vm-init-main.md` | R1-03 | R1 |
| DESIGN-D2-01、READER-C-09、READER-A-06、READER-B-14 | 1 | ① | 把初始栈帧从“未实现”改为“已实现”；唯一余项是 `name_ptr=0`；同步 §3.3/§3.5/§3.9 | `rg -n 'install_boot_stack  or  name_ptr' rewrite-notes/02-stage-vm/01-vm-init-main.md` | R1-04 | R1 |
|  DESIGN-D2-03  |  1  |  ①  |  `extra_pages()` 计费全部 handoff 模块，并为 C 清零的 VM 模块条目补 1 页（P-REFUTE-01 修正，不再按 `vm_allocated_bytes` 计整幅映像）；更新 `test_boot_params_extra_pages_modules` 与 `test_boot_params_extra_pages_vm_module_row`  |  `cargo test -p minix-vm --lib` 549/0  |  R1-05  |  R1  |
|  DESIGN-D2-04  |  1  |  ①  |  新增 §3.1.1 A2 adopt/reconcile 契约；§5.1 补 6 个 reconcile 测试行  |  `rg -n 'reconcile' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-06  |  R1  |
|  DESIGN-D2-05  |  1  |  ①  |  改正 `get_mem_chunks` 职责：内核 classify 产出 free_regions，VM 只 adopt  |  `rg -n 'get_mem_chunks' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-07  |  R1  |
| DESIGN-D2-07、DESIGN-D2-08、DESIGN-D2-19、READER-A-20、READER-C-05、READER-C-16、READER-A-04、READER-C-10 | 1 | ① | SEF 只保留 fresh 握手；`asynsend` 取代 `send`；明确 `init_type` 未消费、LU 未实现 | `rg -n 'asynsend  or  init_type  or  fresh 握手' rewrite-notes/02-stage-vm/01-vm-init-main.md` | R1-08 | R1 |
|  DESIGN-D2-09  |  1  |  ①  |  `missing_spares`/`alloc_cycle` 改为“无对应实现”，指向 `alloc_pfn_reclaiming`  |  `rg -n 'missing_spares' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-09  |  R1  |
|  DESIGN-D2-10、READER-A-19  |  1  |  ①  |  把 SigSet 理由改为 128 位 + 裸通知不置 pending；拆分信号路径为三步  |  `rg -n '128 位  or  p_pending' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-10  |  R1  |
|  DESIGN-D2-11  |  1  |  ①  |  生产路径缺 VM boot 条目时 panic；测试构建保留跳过  |  `rg -n 'VM boot image missing' os/servers/vm/src/vm_server.rs`；`cargo test -p minix-vm --lib` 549/0  |  R1-11  |  R1  |
| DESIGN-D2-12、DESIGN-D2-17、DESIGN-D2-18 | 1 | ① | 改正 HeapArena 来源：连续 VA + `VmPageAllocator` 帧 + `vm_self_mappages`；Direct Map 只用于元数据/页表页 | `rg -n 'HeapArena  or  vm_self_mappages' rewrite-notes/02-stage-vm/01-vm-init-main.md` | R1-12 | R1 |
|  DESIGN-D2-13  |  1  |  ①  |  补 A1/A-3 v2/A-5/A-14/镜像槽四态路由的 ARCH 表与正文标注  |  `rg -n '本阶段 ARCH 决定  or  \\[ARCH:' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-13  |  R1  |
|  DESIGN-D2-14、DESIGN-D2-16、READER-A-12  |  1  |  ①  |  `pt_init` 改为结构变化（adopt 引导根）；`pt_bind` 归 `exec_bootproc`；补 Phase 1/pt_init 时机说明  |  `rg -n '结构变化  or  pt_bind  or  Phase 1' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-14  |  R1  |
|  READER-A-03、READER-C-12、READER-C-13、DESIGN-D2-15  |  1  |  ①  |  统一 `total_pages` 四口径表；区分位图容量/可用页/全局账；补 `boot_usable_pages` 计算步骤  |  `rg -n '位图容量  or  boot_usable_pages' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-15  |  R1  |
| READER-B-01、READER-C-06、READER-C-11、READER-A-08、READER-B-10、READER-B-11 | 1 | ① | 补 A–E ↔ Phase ↔ §2.2 映射表；`mem_init` 归阶段 B；说明 `init_proc→pt_init` 对调 | `rg -n '两套编号的对应  or  阶段 B' rewrite-notes/02-stage-vm/01-vm-init-main.md` | R1-16 | R1 |
|  READER-B-03、READER-A-07  |  1  |  ①  |  C 章移除 Rust 状态；transport 定义移到 §3.9 术语  |  `rg -n 'transport  or  __minix_init' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-17  |  R1  |
|  READER-B-05  |  1  |  ①  |  删除 §4.5 端点常量修订史；§4.9 去掉对应结论  |  `rg -n '端点常量修正' rewrite-notes/02-stage-vm/01-vm-init-main.md`（应无输出）  |  R1-18  |  R1  |
|  READER-B-06  |  1  |  ①  |  §4.4.4 收窄为 Drop 生命周期；测试窗口移出正文  |  `rg -n 'with_test_window' rewrite-notes/02-stage-vm/01-vm-init-main.md`（应无输出）  |  R1-19  |  R1  |
|  READER-B-07  |  1  |  ①  |  新增 §5.4 真机门清单与 `e5d:` 路标；写明独立 VM 启动门缺口  |  `rg -n '真机门清单  or  e5d:' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-20  |  R1  |
|  READER-B-17、READER-C-17  |  1  |  ①  |  基线读数收敛到 §5.3；删除重复与历史失败名；保留命令与 549/0 输出  |  `rg -n '549 passed  or  549/0' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-21  |  R1  |
| READER-B-18、READER-A-15 | 1 | ① | 章节连续重编号（1.0–1.5、4.1–4.5、5.0–5.5） | `grep -n '^### [0-9]' rewrite-notes/02-stage-vm/01-vm-init-main.md` | R1-22 | R1 |
| READER-B-08、READER-B-09、READER-B-12、READER-A-17、READER-A-22、READER-B-02 | 1 | ① | 图标题/阅读范围限定到第 2 章；路线图补 §2.6/§4.4.4；过渡补 07/08/09/25；ACL 与物理内存导航拆分 | `rg -n '启动主线  or  委托出去  or  访问控制' rewrite-notes/02-stage-vm/01-vm-init-main.md` | R1-23 | R1 |
|  READER-B-04、READER-A-20  |  1  |  ①  |  §1.3 压缩为 SEF 概念段；回调表归 §2.4；补 SEF 三层说明  |  `rg -n 'SEF 生命周期是什么' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-24  |  R1  |
|  READER-B-16  |  1  |  ①  |  删除重复鸡生蛋叙述；§2.0 示例改为 `init_vm/init_proc/exec_bootproc` 定位  |  `rg -n 'init_proc  or  exec_bootproc' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-25  |  R1  |
|  READER-A-09、READER-C-15  |  1  |  ①  |  补“VM 自己在 boot 循环清 RS 抑制”因果链  |  `rg -n 'VMCTL_BOOTINHIBIT_CLEAR' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-26  |  R1  |
| READER-A-10、READER-A-11、READER-A-21 | 1 | ① | 补 ACL/RTS/kernel_boot_info/kerninfo/rproctab 术语 | `rg -n 'ACL  or  rproctab' rewrite-notes/02-stage-vm/01-vm-init-main.md` | R1-27 | R1 |
|  READER-A-13、READER-A-14  |  1  |  ①  |  CALLMAP 摘录标注“其余 23 项略”；统一主循环行号；统一 `init_vm` 到 428-587  |  `rg -n '其余 23 项  or  110-192  or  428-587' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-28  |  R1  |
|  READER-A-16  |  1  |  ①  |  清除“新增/不保留/原为/旧读数/当前 HEAD”等过程措辞  |  `rg -n '当前 HEAD  or  原为  or  不保留' rewrite-notes/02-stage-vm/01-vm-init-main.md`（应无输出）  |  R1-29  |  R1  |
|  READER-B-19、READER-B-20、READER-A-22  |  1  |  ①  |  A1/A-3 v2 图例；统一 `init_vm` 行号；ACL 导航拆分  |  `rg -n '设计编号  or  428-587' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-30  |  R1  |
|  READER-C-14、READER-A-18  |  1  |  ①  |  说明 `endpoint == NONE` 是 Rust 交接页空槽表示，不影响 C 的 boot 集合  |  `rg -n 'endpoint == NONE' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-31  |  R1  |
|  READER-C-18  |  1  |  ①  |  明确 `exec_bootproc` 在 test 构建跳过，页帧顺序由生产/真机门覆盖  |  `rg -n '测试边界' rewrite-notes/02-stage-vm/01-vm-init-main.md`  |  R1-32  |  R1  |
| FACT-01、FACT-02 | 1 | ① | 修正 CALLMAP 到 main.c:522-575、SEF 异步回复到 sef_init.c:471-483 | `rg -n '522-575  or  471-483' rewrite-notes/02-stage-vm/01-vm-init-main.md` | R1-33 | R1 |
| FACT-03、FACT-04 | 1 | ① | §5.1 改为“handoff 模块全部计费 + VM 清零条目 1 页”；测试行改为 `test_boot_params_extra_pages_vm_module_row` | `rg -n 'extra_pages_vm_module_row  or  VM 清零' rewrite-notes/02-stage-vm/01-vm-init-main.md`；cargo test 549/0 | R1-34 | R1 |


## P-REFUTE 附加

| claim ID | 结果 | 处置 |
|---|---|---|
| REFUTE-01 | 成立（P0-fact） | ① `extra_pages()` 改为只补 VM 清零模块条目的 1 页；`vm_allocated_bytes` 不再计入 `total_pages`；§2.2.5/§3.6/§5.1 同步。复验：`cargo test -p minix-vm --lib` 549/0；`rg -n 'pages \\+= 1' os/servers/vm/src/boot.rs` |
| REFUTE-02 | 成立（P0-fact） | ① §3.5“当前实现边界”把 `minix_stack_*` 改为已实现（`install_boot_stack` + 真实 `vsp`/`ps_str`）；§5.2 补 `test_install_boot_stack_writes_frame_and_exec_values`。复验：`rg -n 'minix_stack_\\* 初始栈帧 \\| 已实现' rewrite-notes/02-stage-vm/01-vm-init-main.md` |
| REFUTE-03 | 成立（P1） | ① `rs_handshake()` 改为两遍：先校验全部 rproctab 行，再一次性写 ACL；A-14 正文改为“先校验后写入、失败无半初始化 ACL”。复验：`rg -n 'let mut updates' os/servers/vm/src/vm_server.rs`；`cargo test -p minix-vm --lib` 549/0 |

## 修复轮补充（round1b/1c）

| 发现 ID | 结果 | 处置 |
|---|---|---|
| T1-01..T1-06 | RESOLVED | 首轮 T1 复评的 6 条（锚点、示例输出、get_mem_chunks 因果、boot 循环因果、SEF 区域落点、run 行号）；报告 `panel/round1b/t1-repair.md` |
| T1B-01..T1B-07 | RESOLVED | 第二轮 T1 复评的 7 条（start_addr/ELF 锚点、§2.2.6 因果、函数范围、init_vm_slot 步骤、e5d 表计数、CALLMAP 口径）；报告 `panel/round1c/t1-repair2.md` |
| T2B-01..T2B-07 | RESOLVED | T2 复评的 7 条（run/install/init 行号、CALLMAP 26/23、CALLNUMBER、pre_init、reconcile 锚点、blob 释放锚点）；报告 `panel/round1c/t2-repair2.md` |
| P-REFUTE-01..03 | RESOLVED | 见文末 `## P-REFUTE 附加`；报告 `panel/round1/refute.md` |
