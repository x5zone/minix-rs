# D03 R3 certify 独立回读回执

- 结论：PASS
- 核验条数：8
- CLOSED：8；STILL-BROKEN：0

（本回执为第 2 轮复核：第 4、5、8 条按上一回执的证据改正后重核通过，第 1、2、3、6、7 条结论保留，文本未动——L714 / L1279 / L832 / L942 / L535 逐处 grep 在位。）

## 1. §2.8 对照表的 `tss_init` 行

- 判定：CLOSED
- 证据：该行实际位于 §2.7 的 i386↔earm 对照表（L714；§2.8 归纳表 L729-733 无 tss_init 参数行）。L714 原文：`| `tss_init` 的调用者 | `prot_init`（第二参 `&k_boot_stktop`，第一参是 CPU 序号 0；`arch_init` 与 AP 路径的第二次调用见 §2.4） |`，与 §2.3 引文 L311 的 `tss_init(0, &k_boot_stktop);` 及 §2.4 prose（L344）一致。源码复算：`grep -n "tss_init(" minix3/minix/kernel/arch/i386/protect.c` → `338:  tss_init(0, &k_boot_stktop);`。

## 2. §5.4 M1 行

- 判定：CLOSED
- 证据：L1279（M1 行证据列）：「宿主：16 个保护测试 + 13 个 init 门表测试；post-install 的 41 门状态与 `set_gate_ist` 无直接断言（`user_accessible` 的谓词与 DPL 已有直接断言，见 §5.1；设计批 D2-06）」。§5.1 L1237 有对应句：「`user_accessible` 的谓词与 DPL 已有直接断言（`test_user_accessible_gate_set` 测谓词本身、`set_handler_sets_dpl_correctly` 测 set_handler 写入的 DPL），但 post-install 的 41 门状态与 `set_gate_ist` 仍无断言（设计批 D2-06）」——user_accessible 不再被并入「无断言」。源码锚点核过：`os/arch/src/x86_64/trap_stub.rs:618-621`（`entry.set_handler(..., user_accessible(vector))`）、`:627-628`（`2 => entry.set_gate_ist(2, 1)`、`8 => entry.set_gate_ist(8, 2)`）；测试 `os/arch/src/x86_64/trap_entry.rs:368`、`os/arch/src/x86_64/trap_stub.rs:840`。

## 3. §3.3 的「E1 这条设计论据的原文如下」指针

- 判定：CLOSED
- 证据：L832 现文：「四步完整代码见 §4.2；E1 的原文已在上一段引出，这里给的是三个候选的取舍。」上一段代码块（L823-830，标注 `os/kernel/src/lib.rs:fn init_protection（E1 注释原文）`）逐字引用 E1 五行注释，与源码 `os/kernel/src/lib.rs:2232-2236` 一致（`sed -n '2215,2320p' os/kernel/src/lib.rs` 读出）。指针不再落空，后续候选取舍表（L834-840）衔接成立。

## 4. §5.1 的测试计数口径

- 判定：CLOSED（改正后重核）
- 证据：L1235 括注现文：「（若把行内与注释形态也计入，src 侧 grep 会读到 374 处/54 文件；`os/arch/tests` 侧宽口径另有 95 处/21 文件；本文只采用行首属性口径）」——95 不再写成 src 的文件数。复算：行首（允许缩进）口径 `grep -rcE '^[[:space:]]*#\[test\]' os/arch/src --include='*.rs'` = 373 行/54 文件，与「373 个/54 文件」一致；src 宽口径 `grep -rc '#\[test\]' os/arch/src --include='*.rs'` = 374 行/54 文件，「374 处/54 文件」逐字复现（多出的 1 处即注释命中 `os/arch/src/riscv64/trap_stub.rs:906`，9 文件子集 56 − 1 = 55 亦一致）；tests 宽口径 `grep -h -c '#\[test\]' os/arch/tests/*.rs | awk '{s+=$1} END{print s}'` = 95、`ls os/arch/tests/*.rs | wc -l` = 21，「95 处/21 文件」复现；tests 行首口径 87 处/21 文件，与尾句「另有 87 个/21 文件」一致。

## 5. §4.2 节选块的省略标注

- 判定：CLOSED（措辞偏差改正后重核）
- 证据：L988 标注现文：「第一处除 `program_entry_base` 的 BSP 调用外还省了它**之前**的 BSP 拓扑计算实代码（`cpu_topology()` 一行加四个派生量赋值，按物理行 5 行、扣去函数调用行 4 行）；第二处省的是 `register_current_arch` 的说明注释，注册调用行本身保留在正文，段间说明注释按节选略去」。源码核（`os/kernel/src/lib.rs`）：拓扑计算在调用**之前**——`let topo = ...cpu_topology();`(:2249) 加派生量赋值 `let bsp_hw`(:2250)、`let bsp_logical`(:2251-2253) 共 4 物理行，合计 5 物理行，`program_entry_base` 调用在 :2254——「按物理行 5 行、扣去函数调用行 4 行」两个读数均复现；第二处为 `register_current_arch()` 上方 3 行说明注释(:2277-2279)，doc 节选 L999 保留 `trap_dispatch::register_current_arch();` 调用行，逐字属实。

## 6. §3.7 门类型的两个写点（§4.3/§4.4 的设计侧对应节）

- 判定：CLOSED
- 证据：L942（§3.7）：「Rust 侧的门类型有**两个写点**：`X86_64TrapEntry::init` 先把 8 个异常/软中断门写成 trap gate（`is_trap=true`，见 §4.4），随后 `install_trap_stubs` 经 `set_handler` 整条重写为中断门——生产路径上活的是后者，只装 init 元数据、不调 `install_trap_stubs` 的测试载体才会看到 trap gate」。源码核：`os/arch/src/x86_64/trap_entry.rs` 的 `init`（:198 起）内 `is_trap=true` 恰 8 条——向量 0、1、3、4、32、33、34、35；`set_handler`（:333-341）收尾 `self.set_gate(vector.get(), handler.get(), dpl, 0, false)` 写中断门；`install_idt_handlers`（`os/arch/src/x86_64/trap_stub.rs:618-621`）经 `set_handler` 全量重写。测试载体在同篇 §5.2 有点名：L1245「x86_64 版不调 `install_trap_stubs`，门里的入口保持占位 0」、L1259「`test-protection` 的 x86_64 载体不调 `install_trap_stubs`」——合读即 test-protection 这类只装 init 元数据的载体看到 trap gate。

## 7. §2.5 的 lgdt→lidt 窗口

- 判定：CLOSED
- 证据：L535 第 1 步现文：「这个中间态在**取指轴**上安全，因为缓存描述符在重载前一直有效；但**异常递送轴**在这个窗口内不设防：IDTR 仍指向引导方的 IDT，而 GDTR 已换——若此窗口内来一次异常，CPU 按旧 IDT 递送，门选择子却要经新 GDTR 查表，正是 §3.2 反事实的错配方向。所以这一步的「安全」只对取指成立，窗口终点也不是 `prot_init` 入口而是其内部的 `idt_reload`（第 3 步）。」源码核：`minix3/minix/kernel/arch/i386/protect.c:304-306` = `x86_lgdt(&gdt_desc);` → `idt_init();` → `idt_reload();`，`:268-270` `void idt_reload(void){ x86_lidt(&idt_desc); }`——窗口终点确为 prot_init 调用链内的 `idt_reload`，两个限定均已写入。

## 8. §2.1 反事实（删除步骤 2 后先炸点）的排序限定

- 判定：CLOSED（限定补入后重核）
- 证据：L188 反事实段「……会在 `cstart`→`prot_init` 链上先炸；不是“切不出后果”。」句后现文：「**「先炸」的排序没有取证**：`main.c:133` 的 `env_get(BOARDVARNAME)` 早于 `main.c:147` 的 `cstart()`，也是 `kinfo` 的消费者，它在 `.usermapped` 全零时的行为（返回空串、还是读到坏指针）本篇没有核对，所以“第一个失败点是 `pg_identity` 的断言”目前只是推断，不是实测顺序。」——限定位置、内容与要求一致。行号复算：`grep -n "env_get(BOARDVARNAME)\|cstart();" minix3/minix/kernel/main.c` → `133:   machine.board_id = get_board_id_by_name(env_get(BOARDVARNAME));`、`147:  cstart();`，与文中引用逐字相符；`env_get` 作为 `kinfo` 消费者与 L186「步骤 3 依赖步骤 2（`env_get` 读 `kinfo.param_buf`）」自洽；`assert(cbi->mem_high_phys)` 在 `pg_utils.c:170` 亦核对无误。

结论：回读验证通过，8 条逐条落证。
