# NK4-C glm 会话交接（2026-10-02，续-153→190）

本文件是 2026-10-02 会话的交接登记页。会话主线、生产码改动清单、探针台账与验证配方已写入 `NK4C-WORKLOG.md` 的「§1.120续-190-交接」节（commit 54ad18056），本页不重复，只登记一项遗留欠账。

## TODO：riscv 生产改动的追溯 CodeReview（交接后由专人补做，非本文档作者执行）

2026-10-02 会话共有 37 笔提交（区间 `8a9a44933..54ad18056`）。其中约 6 笔生产码改动入库时未经 CodeReview 子代理评审——仅续-153 一笔在提交前完成评审（评审结论 REQUEST_CHANGES，三件意见全部落实后入库，见该提交信息）。以下各笔按项目纪律（生产码改动须过 CodeReview 加全验证链）属于欠账，交接后应逐笔补做追溯评审与回归验证。各笔 commit hash 已按 `git log --oneline 8a9a44933..HEAD` 现查核对：

| 续号 | 文件 | 内容 | commit |
|---|---|---|---|
| 续-160 | `os/servers/vm/src/region/region_map.rs` | `find` 的 iter 兜底（rm-fallback 打点）与 `find_mut` 的 pop_first 全量重建（rm-repair 打点），防御性绕过 BTreeMap range 与 iter 不一致 | 934af1bb2 |
| 续-168 | `os/servers/vm/src/boot.rs` | `read_boot_params` 的 free list 扣减第一层：按 handoff root_paddr 扣 64KB | a10c6d52e |
| 续-175 | `os/servers/vm/src/boot.rs` | 扣减第二层：扩展为全部 boot 模块区 + root 页（flat_map 多区间拆分） | cb11d2eef |
| 续-180 | `os/servers/vm/src/boot.rs` | 扣减第三层：扩展为整池 [0x82000000, 0x84000000) | af7085f0a |
| 续-187 | `os/kernel/src/vm_handoff.rs` | `classify` 不再把 VM 模块 blob reclaim 进 free list（全量保留） | e6aa68f07 |
| 续-179/186 | `os/servers/rs/src/process_table.rs` + `os/servers/vm/src/vm_server.rs` | `by_endpoint` 索引补上界守卫（412 行）；`handle_kernel_memreq` 对 target=VM 走 vm_self_map 自填路径 | 33ed4eb45 |

补做评审时建议的回归口径：host 测试按包集跑（`cargo test -p minix-kernel -p minix-arch -p minix-boot -p minix-types`，勿用 --workspace），已知基线 minix-vm 531 / minix-rs 351（出处：33ed4eb45 提交信息）；真机判据与当前失败尾（`cause_sig: sig manager 8`）见 WORKLOG「续-190-交接」节第三部分。

另有两项与上述欠账同源的登记（正文有据，均未执行）：RegionMap 兜底/重建与 VM 自填路径属防御性代码，若追溯评审后判定腐坏根因已消除，可评估是否保留（续-160 打点语义为「不掩盖」）；探针滚除须过 C-61 pattern-gate（台账见 WORKLOG 续-190 交接第二节）。

## 追溯评审结论（已于 2026-10-02 续-191~193 会话补做，CodeReview 子代理执行）

**前置事实**：本会话真机定谳——VM SIGSEGV 真身＝`paging.rs` 陈旧探针 U 态发 SBI ecall（成修 f90416cc9），resume-Fault/分配器腐坏/text 腐坏**家族全是幻影**。故上述 6 笔防御码的**动机大多误诊**。逐笔判定：

| 笔 | 判定 | 结论 |
|---|---|---|
| 续-160 region_map | REQUEST_CHANGES | 掩盖型兜底（健康 BTreeMap range/iter 不可能不一致），gh61 sf-*/rm-* 零命中＝惰性死码；find 合法 miss 多付 O(n) 全表扫描。建议降为 `debug_assert!` 或移除，rm-* 打点随 bootmark 滚除 |
| 续-168/175 boot.rs | REQUEST_CHANGES | 168 root_paddr 派生被 180 整体替换＝死贡献；175 模块循环被 180 整池吞（`de<=cur` 全跳过）＝死冗余 |
| 续-180 boot.rs | REQUEST_CHANGES（**P1·延后单独批次**） | 硬编码 riscv `0x82000000..0x84000000` 无 `#[cfg]` 门入**三架构共享** `read_boot_params`；x86/aarch64 当前 free_regions 不交集该窗＝no-op（未致 marker 回归），但属跨架构过扣隐患 + `flat_map` 单遍减法隐式依赖 deduct 升序（模块 base<池底会被静默漏扣，P2）。正解＝信任内核 classify 单点扣减、删 VM 侧整池二次扣减。**移除须过 x86+aarch64 non-regression 门，(A) 结案后单独处理** |
| 续-187 classify | REQUEST_CHANGES（延后） | 主动背离 C protect.c:450-451（C 语义恰是 reclaim VM 模块）；gh59 自证「改了仍死」＝路径排除。建议回退 C-对位 reclaim（省 ≈625KB），延后+双架构护航 |
| 续-179/186 RS 上界守卫 | **PASS（保留）** | `by_endpoint` `0..NR_PROCS` 守卫与既有访问器边界一致，消解 NONE/ANY/SELF 哨兵越界写（gh54 len64 index64 实证），C manager.c:2108 无检查会 UB——**唯一独立成立的真实修复** |
| 续-179/186 VM 自填路径 | PASS（保留·观察） | PageFlags WRITABLE|USER|PRESENT 无 EXEC 合 W^X、get_active None 早出口修复自洽；但 gh61 示 sf-* 零命中（memreq target=0x800c 非 VM），此支可能「正确但从未真正救火」，随 (A) 结案复核可达性 |

**总口径**：仅 ⑥Part-A 真修保留；①③④⑤为误诊衍生防御码/跨架构回归/死冗余，**移除/回退须集中在 (A) 结案后的独立批次、每笔过 x86+aarch64 non-regression 门**（当前 marker 均在，非活动回归，不动）。nk4a: 打点到期随探针滚除。

## 续-191~197 会话交接（2026-10-02 后续轮·新 agent 从此接手）

### 已完成（均验证+CodeReview+commit，详情见 WORKLOG §续-191~197）
- **P0 真修**：riscv VM 自管理 SIGSEGV（`cause_sig: sig manager 8`）真身＝`paging.rs` 陈旧 l1raw/q-badroot 探针在 VM(U 态)发 SBI putchar `ecall a7=1`、被内核 IPC 门误当 Send(msg ptr=a1=0)→EFAULT→SIGSEGV。删两探针即成修 `f90416cc9`。`cause_sig` 已绝迹（gh64 验）。
- **重大纠偏**：前 37 会话（续-139~190）追的 `kernel_call_resume`/`VmCheckResult::Fault`/delivermsg/自填/wrong-root/VM 分配器/text 改写**整族＝幻影**（gh61 多点探针正证据判死：mrr 全 Ok、sf-*/rvflt/kc-efault/WARNING 全零）。勿重走。
- **回溯审计（NK4C-RETRO-AUDIT）**：AF-8(P0，riscv 懒 FPU 非 owner FS Initial→Off，轮转腿可达)、AF-9、AF-10（钉测）`ef3c40c6b`；AF-12（NIT）`719716dab`；AF-5（P1，修 pm_sched mock → `cargo build --workspace --all-targets` 从 E0046 复活、pm_sched 4 passed，**关目标③集成测试静默腐坏盲区**）`6d485d4ca`。
- **AF-11**（探针滚除）：本轮新增 P1/P2/P3/ipcefault/kill 探针入台账，C-61 pattern-gate 前不滚（(A) 未结案）。

### 当前唯一阻塞＝(A) pagefault-in-VM（riscv64 marker 差这一环）
- 现象（gh64/gh66 逐位相同、确定性）：VM 自身 walk 时 `read_pte_dm` 访问 `sepc 0x3ac1a stval 0x409d28bb20` → `trap_dispatch.rs:1991` `pagefault in VM` panic（VM 自身缺页致命，C exception.c 对位）。
- **已定性质（硬证据）**：全 512MB 快照（gh66 pmemsave）反查——无任何 8 字节槽 `paddr==` 假表基址 `0x309d28b000`；VM 自身根表树 BFS 22 表全净。⇒ 假地址**只在寄存器/瞬态**，非持久内存 PTE。`0x309d28b000 = 0x9d28b000 | 3<<36`（DM 基址 bit36/37 泄入 PPN）。静态源码回推：save/restore 两腿均从 ctx 全量装 gp_regs→**非恢复腿丢槽**。怀疑面收窄到二选一：① VM 自身 walk 算术（算子表地址时 DM 位泄入）或② 子进程 0x800c 树某中间 PTE 读自一个已被 VM 自己 reclaim/重用的帧（PT↔data 活别名，非噪声）。两者均需活体 trace。
- **下一步（唯一有效路径，记忆续-124：纯文本探针已到边界）**：`qemu-system-riscv64 -s -S -gdb tcp::3333` + `gdb-multiarch`（本机可用），在 (A) 崩溃现场读 VM 活体寄存器 + 其父表槽实际内容，或对 walk 跟随的中间帧设硬件写点回溯写者（区分“算术造出” vs “读了个正在被复用的旧 PTE”）。坐实前不成修。

### 三大目标进度
- ①三架构 marker：x86✅、aarch64✅、**riscv64 ❌（只差 (A)）**。② 18-stage 命令面：仅 x86 核心✅（riscv/aarch64 待①）。③ 586 C 测试上机：未启动（AF-5 已清编译盲区为前置）。三条全未成，**不标 complete**。

# ★(A) 现状（2026-10-02 续-212 更新）——「DM 覆盖洞」定论**已被实验证伪并 revert**，(A) 仍 open
> ⚠️ 下方旧「根因定论=VM DM 窗覆盖洞」已被续-212 的正常-boot 真机实验推翻，勿再据此修复（曾据此实现连续 DM 窗→gh71 验证失败→已 revert）。

**证伪经过**：按覆盖洞假说实现 `establish_boot_dm` VM 窗改连续 `[0, kernel_dm_pa_end)≈[0,0xA0000000)`（覆盖全 RAM），重建 riscv 真机 gh71：`cause_sig=0`（paging.rs 探针 P0 修复仍有效）但 `pagefault in VM` **依旧**，且正常-boot fault `stval 0x10bd28bb2c` → PA `0xbd28bb2c` **> RAM 顶 0xA0000000**——再全的连续 DM 窗也映不到 RAM 外的伪 PA。⇒ 覆盖洞假说不成立；`-S` 下看到的 in-window 0x82ce4000 是 gdb 停机时序造成的**幻影变体**。
**当前最佳判断（未坐实）**：VM 走子进程页表时 `read_pte_dm` 跟随一个 **>RAM 的伪 table_pa**（gh71=0xbd28c000），该值在全 RAM 快照无任何匹配持久 PTE ⇒ 要么 (i) VM walk 的表基址**算术**把非法值/高位当 PA（`walk_read`/`query`），要么 (ii) 某父 PTE 指向的子表页**被 VM 自身 reclaim/复用清零**后仍被引用（PT 帧生命周期，与 `ptalloc-reuse` 同域）。二者都需活体 step-back 区分。
**唯一有效下一步（专注交互式 gdb 会话，非单发可竟功）**：**正常 boot（不加 `-S`，避免时序幻影）**下 `gdb-multiarch hbreak *0x3abec` 命中后 `stepi` 回溯：找出把 `0xbd28c000`（或当轮伪 PA）装入寄存器那条 `ld` 的**源地址**，读该源真值 → 源合法却算出 >RAM ⇒ 算术 bug；源本身是垃圾/0 ⇒ 父表页被回收。harness 模板见 WORKLOG §续-205（qemu -S -gdb tcp::3333 + `hbreak *0x<fault-va>`，VM text VA→guest-phys 用 minix-vm 字节匹配快照定位）。**未坐实不成修。**
**不变的事实**（已确证，勿重走）：① paging.rs U 态 SBI ecall = 真 P0，已修 `f90416cc9`，`cause_sig` 绝迹；② resume-Fault 全族 = 幻影；③ VM 自身根表树 22 表全净、伪 PA 不在任何持久 PTE（排除「内存 PTE 被写坏」）。证据 `tmp/ram70.bin`。

## ★(A) 续-216 更新（2026-10-02·**「算术 bug」与「分配器双发/refcount」两条分支均已关闭**，详见 WORKLOG §续-216）
> 上方 line 57-58 列的两个候选（(i) walk 表基址**算术**把非法值当 PA、(ii) **父 PTE 指向的子表页被 reclaim/复用后仍引用**）在续-216 均被证据关闭，**勿再据此修复**：
> - **(i) 算术 bug — 作废**：反汇编 `Riscv64Paging::query`@0x3ab58 证 walk 算术 `ld a1,0(a1)`=`pte_to_paddr(l2e)+DM_BASE+i1*8` 完全正确；旧「坏掩码 0x00fffffffffff000 = 寄存器破坏」是误读（那正是 `PTE_PPN_MASK<<2` 正常掩码）。
> - **(ii) 分配器双发/refcount — 真机可信检测器证伪**：gh72/gh73（当前构建、确定性复现，逐字节同）里 `ptalloc-DUP=0`/`alloc-reuse-PT=0`/`dblfree=0`/`ptalloc-OOR=0`/`BADPFN=0` ⇒ vm_pt_alloc 恒返 RAM 内合法 PA、从不双发/重 free；仅歧义 `reuse-DATA` 命中（因 VM 堆/heap_arena 旁路 `DATA_SEEN` 而不可信）。⇒ **加「拒发 live 帧」= 空修。** 静态审计（对照 C pagetable.c/alloc.c/cache.c）亦证 reclaim 谓词、VM 进程独占中间表 free、PT+data 共享池均 C-faithful。
> - **新的唯一有效下一步（a2d）**：(A) 真身＝某子进程根表的一个父级表槽在被 walk 读那一刻持有 PPN>RAM 的**瞬态值**（全 512MB 新快照 `tmp/nk4a/ram73.bin` 0 个持久槽含它 ⇒ 快照抓不到、需读取瞬间抓）。riscv 无 `gdb record` ⇒ 先加 riscv-gated U-safe 探针（`bootmark::mark`/sys_diagctl，绝不用 `CurrentEarlyConsole`＝记忆 a298c18a）在 query 读 l2e 且 `!leaf && paddr>=0xA0000000` 时打 `(walked_root, level, idx, raw)` 定帧，再设写 watchpoint 抓写者。未坐实不成修。

## ★(A) 续-226~238 前沿覆写（2026-10-02·下一会话先读本块；上方 a2d 探针路径已被实测推翻）
> 【重要纠正】上方 line 65「先加 U-safe 探针定帧」与旧「交互式 gdb 需真人真终端」两前提**均已失效**：
> - **a2d 读侧 U-safe 探针（gh74）实测 badpte=0 且探针扰动时序反使 (A) 本轮不复现**（§续-218/219）⇒ 文本探针在 (A) 上是 Heisenbug 扰动源，不能用来定帧。
> - **「需真人/环境不可驱动 gdb」已被 §续-226 实测推翻**（脚本化 qemu `-S -gdb tcp::PORT` + gdb-multiarch `hbreak`+`continue` 可驱动读到活体寄存器；真错因先前是不带 `-S` 崩完才 attach + 未清残留 qemu 占 gdb 单客户端口）。⇒ 降级为「可脚本自动化，但 (A) 对 halt 敏感」。
> - **§续-232 铁证**：(A) 对 gdb 暂停/恢复**本身**极敏感（gh81 2s-attach 仍把 0x3abec→0x3ac1a、stval 变体），任何基于 halt 的活体抓法都在扰动它。§续-236 QEMU record 本配置启动即退。

### (A) 位级定性（§续-233）+ 静态对账后的正确 watch 靶（§续-238、本块核心）
- **位级**：伪 child_pa = 合法在-RAM 帧 | 多余高位（0xbd28c000=0x9d28c000|bit29；0x409d28bb20=0x9d28bb20|1<<38）。基址 `0x9d28xxxx` 落在 §续-217 那簇 0x800c PT 帧 + 相邻 /etc/rc 页缓存数据帧。⇒ (A)=**帧级 PT/data 双别名**（一帧既当页表又被旧数据引用方写踩 PTE），非随机瞬态。
- **已静态排除（勿重走）**：双发检测器=0（§续-216）/ fork+map_page refcount 均对称正确、无漏计（§续-234）/ exit-teardown 主路径表帧独占且对称释放无行级错（§续-235）/ verify_refcounts 测不到 PT 帧（refcount 恒 0）。
- **§续-238 零扰动钉死的 watch 靶（纠 §续-221/228）**：反汇 `minix-vm`@0x3abec=`ld a1,0(a1)` + 解 gh72 clean-repro GPR——
  1. **a1(x11) 不是 `query` 的 `&self`**，是算出的被读槽调试底图 VA（故 x11==stval 是应然）；§续-221 step3 判读作废。
  2. off=`0xb2c` **非 8 对齐**⇒按 §续-153b 自己的判据是**叶/数据访问**，非干净槽读（父基址被写入带了 vaddr 低位）。
  3. 被 walk vaddr 属 `0x3ffffdfbXX` 族（x2/x5，off 逐位吻合）⇒ **i2=255（0xFF）非 511**；§续-228「root+511*8」=0x82000FF8 是错靶。
  4. 被 walk 子根=`0x9dc37000`（§续-229 sas-fork）非 VM 根 `0x82000000`（pfvm 探针印的是 VM 自己 satp）。⇒ 父槽物理=`0x9dc37000 + 255*8`=**`0x9DC377F8`**。**§续-239 源码级定级定根（收敛到唯一靶）**：反汇 0x3abec=`read_pte_dm(pte_to_paddr(l2e)+i1*8)`（L1-槽读），其表基址来自上一步 **l2e=`root+i2*8`（L2 层）**⇒被写坏的是 **L2 中间项非叶**；`pte_to_paddr` 恒 4K 对齐、实测 off=0xb2c 非 8 倍⇒喂进的 l2e 是个**数据字**（坐实 §续-233 双别名）；又 §续-195 已证 VM 自身根 22 表全净⇒VM 根备选 **0x820007F8 已排除**。**唯一硬件写 watchpoint 靶 = `0x9DC377F8`**，级别/根均已定死。
- **抓写者正确配方**：正常 boot（**不加 `-S`**，保 §续-228 的无-gdb 确定时序）下 `hbreak` 到 `trap_dispatch.rs:1991` panic 前（全 boot 仅命中一次→不慢），一次读 fault frame a2 + 子进程 0x800c phys_root → 据上判据算父槽物理→重启对该**物理**地址设 qemu 硬件写 watchpoint（条件「写入值解码 PPN≥RAM 顶 0xA0000000，或 off 非 8 对齐才停」），零 guest 文本探针扰动。harness：`tmp/nk4a/` fresh 端口 1240 + `ghNN-*` 独立前缀 + 点火前 `pkill -f '[q]emu-system-riscv'` 只杀本 harness qemu。
- **未坐实不成修**；每改码守 x86 marker=2/panic=0 + aarch64 marker 不回归 + check-layout all + host 4 包集（基线 1400/0）。(A) 是① riscv marker 与③ riscv 上机的共同关键路径。三目标均未全成，goal active。
