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
