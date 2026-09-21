# NK4-A 回归评审报告（Phase 1-3 产出，2026-09-21）

> 评审对象：busy-billing agent 在 `940ad8363..cd22e2e68`（nk4a-agent-wip 分支钉存）的全部改动。
> 评审执行：独立新会话（zcode/GLM），只读 + 验证实跑，零代码修改。
> 证据目录：`notes/rewrite/fork-syscall-rewrite/evidence/20260921-2320/`（99 个 /tmp 日志 + tip 冒烟实录）。
> 保全分支：`nk4a-agent-wip` @ cd22e2e68（含其全部工作 + 停止令落盘 + 交接文档）。

---

## 一、总裁决建议：**保留但修补（路线 B/C），不建议全量 reset**

**一句话**：评审四问全部过关——它做的不是 symptom 压制，而是有据的协议级重设计，
产物可信度远超"不信任"预设；真实缺陷是 ①最终功能挂点没修完（V1 FAIL）②一个 commit
引入三架构载体编译回归（2 行级修复）③落盘 543 文件含杂物。全量 reset 会丢掉真机三连环
教训（fix27cde：payload 落 .bss / ram_top 过滤 / NXE 执行位）与已验真的 VM 拆分机制，
重做要重踩这些坑，而它们经本次评审逐条验真为 sound。

## 二、评审四问（任务书 §1.5 指定的主战场）

| # | 问题 | 结论 | 证据 |
|---|------|------|------|
| 1 | KernelInfo v6 ABI 全构造/消费点一致？ | ✅ 一致 | 共享类型 `os/libs/minix-boot/src/kernel_info.rs:13`（`reserved_regions` 字段），编译器强制全构造点补齐；构造点分布实测 ~30 文件（dm_coverage×7、arch/boot.rs×7、kernel/lib.rs×5、test 载体各 1）；消费侧版本化访问带 `< 6` 守卫（`minix-types/src/types/boot.rs:246`） |
| 2 | 847 行身份窗口/VM 拆分容量与碎片数学成立？ | ✅ 成立，全部 fail-fast | 链路：shim reserved（ram_top 过滤，`uefi_helpers.rs:286-321`）→ `store_kernel_info` assert ≤256（`kernel/lib.rs` store 段）→ `build_identity_windows` MAX_CANDIDATES=256 + 合并后 ≤64（assert）+ 总页 ≤512MiB（assert，`vm_handoff.rs:457-530`）→ `MAX_BOOT_REGIONS` 8→32 / `MAX_REGIONS_SPLITS` 32→64（`arch/frame.rs`，13 exclusion × 2 = 26 典型值有裕量）。所有容量越限即 panic/Err，零截断 |
| 3 | 测试是否钉住关键不变量？ | ✅ 钉住（有缺口，已自报） | `test_identity_windows_clip_sort_merge`（vm_handoff.rs:824：地板裁剪/排序/合并/页对齐/不重叠）、`test_identity_windows_empty_reserved_still_has_mmio`（:863）、`test_mock_map_kernel` 段 4 flags==RWX（arch/paging.rs:1141）。缺口（它 §5 自报属实）：深拷贝自拷贝分支、ram_top 过滤（UEFI bin 不可宿主单测）、两处 256 手工耦合无 const 断言 |
| 4 | 既有 2MiB/1GiB 映射契约被破坏？ | ✅ 未破坏 | `split_huge`（x86_64/paging.rs:724）是保守正确实现：翻译逐位保留（frame+flags）、1GiB 子叶保 PS/4KiB 子叶清 PS、512 项全显式写防陈旧、CR3 重载全刷 TLB；`Err(NotSupported)` 保USER-OR-into-huge 的保守缺省（:849）。`(leaf_pa + i) << SHIFT` 写法依赖 base 对齐，数学正确但可读性欠佳（P2） |

## 三、七项危险面核验结果

### 危险面 1：守卫/断言的删、移、放松 —— ✅ 零真删除

全 diff 删除线仅 7 条，逐条定性：

| 位置 | 删除内容 | 定性 |
|------|---------|------|
| arch/frame.rs ×2 | `debug_assert!(n_splits < MAX)` | **升级**：改为全 profile 生效的 `return Err(TooManyRegions)`（release 下 debug_assert 编译掉会越界写——注释明说，方向正确） |
| boot-shim/main.rs | 静默 `loop {}` panic handler | **增强**：非删除——原地改为串口可见化（raw COM1 + 栈缓冲格式化 + 诊断钩，EBS 前后双态可用，SAFETY 注释 + 有界 LSR 轮询） |
| uefi_helpers.rs ×2 | `assert_bootstrap_outside_memmap` 调用 | **搬家重挂**：新位置 `uefi_helpers.rs:201-202`，在最终快照（全部 LOADER_DATA 分配之后）与 EBS 之前，语义仍覆盖 D-64②（防活页表页进 conventional 快照 → PMM） |
| kernel/syscall.rs | `// C: assert(RTS_ISSET...)` 注释 | **伴随真修复**：`vmctl_vminhibit_clear` 裸位清除改走 `rts_unset`（C RTS_UNSET 就绪队列回队语义，proc.h:216-224）——修掉"最后一个阻塞位是 VMINHIBIT 的进程进不了运行队列"的真 bug |
| vm_server.rs | `.expect("init_proc...")` | **增强**：`unwrap_or_else` panic 附带进程名 + PMM 实态（free/largest/nodes/self_pages） |

放宽 2 处（与 STATUS 自报一致）：`MAX_CANDIDATES` 128→256、窗口 RW→RWX（[ARCH] 注记在位 + mock 断言钉死）。行为变化 1 处：`PANIC_DIAG_ACTIVE` 防重入跳过二次 stacktrace（lib.rs:2060，防刷屏，非绕过失败信号）。

### 危险面 2：新增 unsafe 审计 —— ✅ 抽查合格（33 处新增）

- x86_64/paging.rs ~14 处 `read_pte_dm`/`write_pte_dm`：DM 窗口激活 + 8 字节对齐为前置条件，`unsafe fn` 带 SAFETY 文档；
- boot-shim COM1 直写：SAFETY 注释（平台固定 I/O 资源 + 唯一所有者）+ 有界轮询；
- kernel/lib.rs 的 jump_to_kmain/asm 读 rdi/rsp/cr3：只读无副作用，options 声明正确；
- vm/global.rs：`unsafe impl Sync for EarlyPool` 有"单线程 VM 服务器"契约论证（符合执行模型规范）；
- **诚实缺口（P1）**：`write_pte_dm` 文档自认 VmDm flush 缺口——VM 自有活地址空间的 unmap/remap 需内核辅助 TLB flush，"not wired"（x86_64/paging.rs:178-181）。boot 路径不触发，但这是"页粒度 PMM"能力的已知残缺。

### 危险面 3：内存图语义（A2 灾难向量）—— ✅ 快照三移属实、纵深防御在

- `prepare_boot` 时序实测（uefi_helpers.rs:149-249）：root/bump 分配 → kernel ELF 装载 → 12 模块装载 → **最终双清单快照**（build_memmaps，conventional + reserved）→ 双守卫 → KernelInfo → EBS。conventional 快照天然排除全部占用段（UEFI 改标 LOADER_DATA）；
- conventional 腿语义未动（逐条进 regions，ram_top 只作用于 reserved 腿，:295-321）——STATUS Q4 声明属实；
- 内核侧 `cut_memmap` 排除机制仍在（vm_handoff.rs:243），VM 侧 13-exclusion 碎片路径保留为纵深防御；
- `store_kernel_info` 深拷贝：assert 长度 + 同区间跳过拷贝（比 STATUS 自述的"容忍自拷贝"更稳）+ .bss 落位，修掉 UEFI 堆载荷 higher-half 后读零的真机实证问题。

### 危险面 4：既有设计完整性存废 —— ✅ 无 (c) 类无据损毁

全 os/ diff 仅 **1 处 ≥20 行连续删除块**（syscall.rs vmctl_set_addr_space ~34 行改写）：
定性 (a)+(b)——实质是 **C 保真修复**（`as u32 as u64` 修掉 ≥0x8000_0000 物理根的符号扩展，
C 的 u32_t 模板转换语义）+ 等价结构重组，带 C 锚点注释。其余深改区全是净增：
dm_coverage.rs 360→524（+164 纯增）、x86_64/paging.rs +245/-9、vm_handoff.rs +252/-2、
lib.rs +316/-17。用户担心的"因为 bug 乱改设计"不成立。

### 危险面 5：三架构载体存活 —— ❌ **aarch64 + riscv64 编译断裂（本次评审最大硬发现）**

- x86_64 载体（test-higher-half / test-kernel-map，x86_64-unknown-uefi）：✅ Finished；
- aarch64 载体：❌ `minix-kernel` lib 2-4 errors——`relocation type fixup_aarch64_movw unsupported on COFF targets`；
- riscv64 载体：❌ 同因——`unrecognized instruction mnemonic, did you mean: mv?`；
- 根因：**9e115387e 新增 4 处 x86 专属 asm 无 `target_arch` 门控**：
  `os/kernel/src/lib.rs:546`（`mov {}, rdi`）、`:548`（`mov {}, rsp`）、`:3497`（`mov {}, rsp`）、`:3544`（`mov {}, cr3`——ARM 无 CR3）。均只有 `#[cfg(not(feature = "mock"))]` 门；
- **基线对照**：`940ad8363` 的 test-higher-half-aarch64 构建 ✅ Finished（19.98s，worktree 实测）——回归铁证，破坏方就是 9e115387e；
- 修复成本极低：4 处 asm 补 `#[cfg(target_arch = "x86_64")]` 门（诊断代码本就只服务 x86 真机取证）。

### 危险面 6：范围审计 —— ✅ 无 drive-by；f1041b3f1 需清理裁决

- os/ 67 文件逐一过目：全部属于 boot-shim/kernel/arch/VM/minix-sys/minix-rt/命令载体接线域；
- `os/kernel/src/syscall.rs` +214 重点排查：全部落在 VMCTL 域（dispatch_vmctl / vmctl_set_addr_space / mark_flush_tlb / 新增 `nk4a_flags_mark`——后者是 `#[cfg(not(mock))]` 门控纯诊断打印）。零 fork 语义改动，零 boot 域外顺手修改；
- minix-sys `real-trap` feature → build.rs `kernel_trap` cfg（单一权威）：修复"镜像模块忘开 feature 静默 -EIO"，boot 域内合法；
- **f1041b3f1 落盘杂物（P1-process）**：543 文件 +35.7 万行，含 `tmp/`(132)、`.zcode/`(33)、`.trae/`(12)、`AI-chats/`(5)、`examples/`(6)、`qpid4`/`qpid5`、`.codebuddy`、`.patch_doc07.py` 等明显非仓库物——需 `git rm --cached` 分流（它自己在 STATUS §5-⚠3 也请求复核）。

### 危险面 7：STATUS 文档逐声明核验 —— ✅ 抽样全部对上（文档诚实度高）

| STATUS 声明 | 核验结果 |
|------------|---------|
| "没有删除或注释掉任何 assert/panic/守卫" | ✅ 属实（7 条删除线全升级/搬家/增强） |
| 快照三移 + 守卫新位置（Q3） | ✅ 属实（uefi_helpers.rs:157-202 实测） |
| conventional 集合语义未变（Q4） | ✅ 属实（ram_top 只作用于 reserved 腿） |
| fix27c 深拷贝 + 256×2 手工耦合无 const 断言（Q5/§5） | ✅ 属实（globals.rs 注释承认，store 有 assert 兜底） |
| fix27e RWX + [ARCH] + mock 断言同步 | ✅ 属实（arch/paging.rs:122/:637/:1141） |
| 3 个宿主测试存在（Q5） | ✅ 属实（vm_handoff.rs:824/:863 + paging.rs:1141） |
| PANIC_DIAG_ACTIVE 防重入 | ✅ 属实（lib.rs:2023/:2060） |
| "宿主测试全绿" | ✅ 属实（V2/V3 实测） |
| 最后挂点"诊断中（未解决）" | ✅ 如实（V1 实测复现同挂点） |
| **未自报**：4 处 asm 破坏 aarch64/riscv64 载体编译 | ❌ 漏报（V5 实测发现；宿主/UEFI-x86 构建面测不到，符合它自报的"宿主 dev/mock 组合不编译 cfg(not(mock)) 分支"盲区模式的延伸） |

## 四、逐 commit 三栏定性表

| commit | 定性 | 一句话理由 | 复核方式 |
|--------|------|-----------|---------|
| `d697c481e` fix27a（类型/协议层，37 文件 +472/-13） | **可信** | 纯 additive：KernelInfo v6 字段 + 全构造点同步，编译器强制一致性 | V2/V3 绿；`git show d697c481e` |
| `77d50669b` fix27b（窗口数据源+构造+映射，4 文件 +447/-28） | **可信** | 双清单快照/过滤排序合并/map_kernel 第 4 段；当时参数错（128 上限、RW 缺执行位）但随后 a9f8681fc 即修，中间态未出货 | V3 绿；vm_handoff.rs:446 / arch/paging.rs:621-646 |
| `ffff852f6` fix22-26（VM 侧消费+bootmark，12 文件 +887/-62） | **可信** | 消除 fabricated-endpoint 假成功（kernel_gateway 动机注释自证）、handoff 采纳、EarlyPool；真机 12 模块 exec ok 佐证 | V3 绿；os/servers/vm/src/kernel_gateway.rs:1-30 |
| `9e115387e` 取证路标累积（13 文件 +627/-115） | **存疑（含 P0-build 回归）** | 定位地基（panic 可见化+路标体系）价值大且多为门控纯诊断；**但 4 处无架构门 x86 asm 打断 aarch64/riscv64 载体编译（基线绿→现红）** | V5 双向对照；lib.rs:546/:548/:3497/:3544 |
| `a9f8681fc` fix27cde（三连环真机贯通，5 文件 +113/-8） | **可信（应拆三）** | 深拷贝/ram_top/RWX 三修正均有真机证据 + [ARCH]/mock 钉死；混一条 commit 不利回滚归因（自评已认） | a9f8681fc diff；serial_fix37 证据 |
| `f1041b3f1` 停止令落盘（543 文件 +35.7 万行） | **部分可信+需清理** | PF 探针一发命中递归点（对）；142 项历史 untracked 全量入库含明显杂物（需 git rm --cached 分流） | `git show --stat f1041b3f1` |

（另 5 个 commit 为用户侧 docs：dd484552c / 9699e04a3 / cf7f1a5d2 / c407fde2d / cd22e2e68，不在评审范围。）

## 五、V1-V7 验证矩阵（全部实测）

| # | 验证项 | 结果 | 明细 |
|---|--------|------|------|
| V1 | 它 tip 的冒烟 | ❌ **FAIL (exit 1)** | stage 3 超时：调度器切 RS 后 `pf#0 rip=0x1ddc227b`（CR2=0x10，null+0x10 解引用）→ 同址 PF 递归 12+ 次 → 栈尽 Triple Fault。未达 `entering scheduler`，更未达 rc marker。与 STATUS §1.3 自述挂点一致（真实未修完，非文档美化） |
| V2 | 13 包宿主回归 | ✅ PASS (exit 0) | 680 tests passed，0 failed |
| V3 | 内核/rt/init 宿主回归 | ✅ PASS (exit 0) | 1013 tests passed，0 failed |
| V4 | xtask 契约 | ✅ PASS (exit 0) | 10 tests passed（含 BOOTX64/startup.nsh ESP 断言） |
| V5 | 三架构载体 spot | ❌ **FAIL** | x86_64 ✅ Finished ×2；aarch64 ❌ 2-4 errors；riscv64 ❌ 2-4 errors（同根因：4 处无门 x86 asm；基线对照证明为 9e115387e 回归） |
| V6 | guest 工件面 | ✅ PASS | NS12 13 包 68/68 bin 全部产出（x86_64-unknown-none；须逐包独立构建——init 与他包同图构建会因 minix-rt feature 并集撞 E0152，这是命令形态陷阱非代码回归） |
| V7 | clippy 对账（基线 940ad8363） | ⚠️ **未全过（+3 净增）** | 同包同命令对账（kernel/arch/boot/types/sys/rt/vm，--all-targets）：当前 62 vs 基线 59 条实告警，无 error。新增全部 P2 卫生级：`unnecessary unsafe block`（vm_server.rs 多处多余包裹——安全性不受损）+ `mem::drop` 非 Drop 值（含 vm_handoff.rs:500 `drop(push)` 闭包借用收口，E0503 的解法，语义中性）。按"零新增"门严格判 FAIL，量级为 lint 卫生 |

## 五-附：V7 明细

- 命令：`cargo clippy -j 1 --all-targets -p minix-kernel -p minix-arch -p minix-boot -p minix-types -p minix-sys -p minix-rt -p minix-vm`（docker minix-ci:1.94，两棵树同参）。
- 基线树：worktree @ 940ad8363（独立 target 卷）。
- 原始输出：`/tmp/nk4a_v7_current.txt`、`/tmp/nk4a_v7_baseline.txt`（评审后随 /tmp 生命周期消失，结论以本表为准）。

## 六、发现清单（P0/P1/P2）

- **F0 [P0-functional，未修完]**：调度器首次切换用户态服务器后 null+0x10 解引用 → PF 递归 → Triple Fault。它的任务本来就停在诊断半途（探针已命中递归点，源码定位未完成）。
- **F1 [P0-build，回归]**：aarch64/riscv64 载体编译断裂——lib.rs:546/:548/:3497/:3544 四处 x86 asm 无 target_arch 门（9e115387e 引入，基线对照证明）。修复 ≈ 每处一行 cfg。
- **F2 [P1-design-gap]**：VmDm flush 缺口未接线（x86_64/paging.rs:178-181 自认）——VM 自有活地址空间的页粒度 unmap/remap 缺内核辅助 TLB flush；boot 路径不触发。
- **F3 [P1-security-debt]**：身份窗口 supervisor RWX（含 reserved 区）——W^X 让步有 [ARCH] 注记与理由，仍是安全债；重做方向（loader 段表拆 RX/RW）它自己已写明。
- **F4 [P1-process]**：f1041b3f1 落盘 543 文件含杂物（tmp/AI-chats/.codebuddy/qpid 等），需分流。
- **F5 [P2-latent]**：kmain 影子 kernel_info（参数引用"遭栈覆写"的 fix3 声称无独立复核证据）——行为安全但若声称属实则底层栈损坏问题被遮蔽；注释自标"证实后可删"。
- **F6 [P2]**：RESERVED_REGION_STORE[256] 与 MAX_CANDIDATES=256 手工耦合，无 const 断言（自报）。
- **F7 [P2]**：split_huge 的 `(base + i) << SHIFT` 依赖对齐的优先级惯用法（正确但脆）。
- **F8 [P2]**：assert_bootstrap_outside_memmap 只护 root+bump 两笔最早分配，kernel 段/模块占页依赖 UEFI 改标正确性（快照时点已结构性保证，守卫可再加两笔覆盖作 belt-and-braces）。

## 七、裁决选项（Phase 4 触发条件对照）

任务书 §7 触发条件（满足其一即进重做）：V1 不过 ✅（满足）；或 (c) 类无据损毁 ❌（未发现）；或用户批准。

**选项 A：按任务书默认全量 reset 重做**
- 代价：丢真机三教训（fix27cde）+ 已验真的身份窗口/VM 拆分机制 + 取证体系，重做需重踩（BOOT 段身份窗口是它 847 行解决的真问题，最小修复也绕不开）。
- 收益：重做产物按"每步正确性标准"重来，历史干净。

**选项 B：保留但修补（我的建议）**
- 在 nk4a-agent-wip 产物之上：①修 F1（4 行 cfg 门）→ V5 翻绿；②清理 F4 落盘杂物；③继续 F0 挂点诊断（它已把递归点收窄到 PF 处理树内，STATUS §7 的三步打分法可直接续用）；④F2/F3/F5-F8 登记入 FIXLOG/code-excellence 批。
- 收益：保住真机验证过的机制与教训；它的产物经本次评审验真为 sound，全弃无据。
- 代价：接受其 WIP 历史（可后置 rebase 整理，a9f8681fc 应拆三条它自己已认）。

**选项 C：折中**——机制 commit（fix27a/27b/22-26/27cde）保留，9e115387e 出架构门修复补丁，f1041b3f1 清理；效果与 B 等价、表述更保守。

> 用户裁决后：选 A → 本会话执行 §7 reset-and-redo 流程；选 B/C → 修补清单按 fix-guard 逐条执行。

## 八、方法与局限声明

- 本评审为同 agent VERIFY-CHECK（grep/命令重放式，全部关键结论附实测命令与输出路径）；按规则标注验证局限：构建矩阵 V1-V7 为工具输出（L1），危险面定性为 diff 逐条人工核验（L2）。
- 证据文件：`evidence/20260921-2320/`（smoke-at-tip-*.log = V1 庭证；run37.log/img37.log = 它最后一轮的串口与构建实录）。
- 基线对照使用独立 worktree（/tmp/nk4a-baseline-wt @ 940ad8363，评审后已清理），未触碰工作树。
