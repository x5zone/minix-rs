# NK4-A 回归评审 + 视情况重做（新会话开局 Prompt）

> 本文件是**给新会话 agent 的开局指令**（用户将整段粘贴）。前情：
> 另一个 agent（busy-billing）接手 NK4-A 生产启动链首亮诊断，工作 8+ 小时，
> 以 WIP commit 序列落在 rewrite 上。用户对它的代码正确性深度存疑，委托本会话：
> **完整回归评审它的全部改动 → 给出逐 commit 定性 → 视评审结论决定保留或
> reset 后由本会话重做**。

## §0 开局必读（按序）

1. `CLAUDE.md` + `.claude/rules/review-core.md` + `review-process.md` + `fix-guard.md`
2. `rewrite-notes/coordination/HANDOFF-NK4A-boot-first-light.md`
   （上一棒交接：任务定义、已修三坑、路标体系、构建陷阱）
3. 本文件全文。
4. `bash tools/claim.sh list` + `git worktree list` + `pgrep -f qemu-system`
   ——确认现场无并发会话、无在跑 QEMU 后才动手。

## §1 背景与时间线（一段话）

总目标：三架构 QEMU 启动 minix-rs 并跑通 18-stage 命令（T4）。NS12（命令面）
已收口：12 包 65+3 bin 双 seam + sh/cat/ls 接线 + 冒烟脚本 `os/qemu-tests/
test-cmd-smoke.sh`（T4 验收契约：SKIP 门 → xtask 装配 → ESP 断言 →
`entering scheduler` → `rc: minimal boot script marker` 窗口）。
NK4-A = 生产启动链（boot-shim 装载 kernel + 12 模块 + imgrd）首次点亮。
前棒（zcode_glm_2，本会话前身）修了三层：BOOTX64 未入 ESP、D-64② memmap
快照顺序（必须在全部 LOADER_DATA 分配之后）、镜像跨距 2MiB 收口（.lbss/
.orphan 节归属 + ld 改后须 touch 源码强制重链）。内核当时活到
`kmain Phase A enter` 后挂死。busy-billing agent 接手后推进 8+ 小时
（fix22-27b+ 系列，自称任务名"fork 系统调用重写"——**系账本目录名误用**，
其 diff 实测零 fork 相关代码，全部是 nk4a boot 路径工作），用户不信任其产物。

## §1.5 它到底干了什么（接手方改动全像，基于 940ad8363..HEAD 实测 diff）

**接手时的挂点**：内核活到 `kernel: kmain Phase A enter` 后静默（前棒路标终点）。
Phase A/A.2 的语义 = 用 KernelInfo.memmap（conventional-only 快照）初始化
FREE_MEMMAP 并切割 boot 模块占页。

**它的诊断（第三层根因，前棒只修到第二层）**：前棒把 memmap 快照挪到了
root/bump 分配之后，但 **kernel 段与 12 个 boot module 的 allocate_pages 发生
在快照之后**——这些页在快照里仍是 conventional。内核 VM bootstrap 拿这张图
去切割模块占页/建 VM 区域时，走进活页或在 conventional 区里切出 13 个
exclusion 碎片，撑爆 VmBootRegions 容量。它的取证注记："shim printed memmaps
conv=12"（快照含 12 个 conventional 区）。

**它的修法（+2439 行的全部去向）**：
1. **快照三移**：memmap 快照 + `assert_bootstrap_outside_memmap` 双守卫挪到
   **全部 LOADER_DATA 分配完成之后、ExitBootServices 之前**（最终空闲态）。
   守卫是搬家重挂，非删除（已核验新位置）。
2. **KernelInfo 协议 v6（ABI 变更，评审重点）**：minix-boot 的 KernelInfo 增
   `reserved_regions` 身份窗口字段，"全部构造点同步"——这是 boot-shim↔kernel
   的交接契约变更，必须核验：构造侧（shim）与消费侧（kernel/VM）字段一致、
   三架构 test 载体同步、minix-boot 测试覆盖。
3. **身份窗口机制**（fix27b，847 行 `vm_handoff.rs` + shim 双清单快照 +
   `build_identity_windows` + `map_kernel` 第 4 段）：为 VM 的直接映射窗口
   建立 bootstrap 期覆盖。
4. **VM 侧拆分机制**（fix22-26）：huge-page 叶子拆 4KiB（保翻译/保旗标，
   `Err(NotSupported)` 缺省语义）+ `kernel_gateway.rs`（kernel→VM 页分配
   网关 47 行）+ `bitmap_alloc` 扩展 + `global.rs` +257 + `vm_server` 消费
   181 行——VM PMM 获得页粒度 unmap/remap 能力。
5. **bootmark 路标 crate**：新建诊断路标 crate（三架构 test 载体各 +1 行接线）。
6. **取证路标**：kmain A.1/A.1b/A.2a/A.2b 细分打印（fix27 系列，接续前棒的
   boot_stage! 体系）。

**它的状态**：29+ 轮迭代，最后可见检查 MARK=0（未达 rc marker）、第 29 轮曾有
E0133 编译错误（后续 commit 是否清掉待接手时实测）。用户给它两轮到停顿点，
之后由本会话评审。

**评审含义**：它做的不是 symptom 压制，而是把 VM bootstrap 从"conventional
快照 + 切洞"重设计为"reserved_regions + 身份窗口 + 页粒度 PMM"——这是一个
**协议级重设计**（KernelInfo v6 是 shim↔kernel ABI），方向可能正确，但：
ABI 变更是否所有构造/消费点一致、847 行新机制的容量与碎片数学是否成立、
测试是否钉住关键不变量、既有 2MiB/1GiB 映射契约是否被无意破坏——这四问
就是评审的主战场。

## §2 任务总定义

基线 commit = `940ad8363`（接手前最后 commit）。评审范围 =
`940ad8363..rewrite`（其全部 WIP commit；接手时以 `git log --oneline
940ad8363..HEAD` 实测为准，可能比交接时多 1-2 个）。

四阶段，顺序执行，**阶段之间向用户汇报**：

- **Phase 0 冻结与保全**（见 §3）
- **Phase 1 完整回归评审**（见 §4）——不改任何代码，只读 + 跑验证
- **Phase 2 验证矩阵**（见 §5）——客观判定它的聚合产物到底处于什么状态
- **Phase 3 评审报告**（见 §6）——三栏定性，交用户裁决
- **Phase 4 重做执行**（见 §7）——仅在用户批准或评审结论为"必须重做"时执行

## §3 Phase 0：冻结与证据保全（动手前必做，顺序严格）

1. **保全分支**：`git branch nk4a-agent-wip`（钉在它的最终 tip 上——此后
   无论 reset 还是改写，它的全部工作永可追溯）。
2. **证据归档**：把 `/tmp` 下它的最新串口/冒烟日志（`ls -lat /tmp/*.log |
   head` 按时间挑）复制到 `tmp/evidence/`
   （新建目录），文件名带采集时间。这些是易失证据。
3. **它 tip 上的冒烟实测**：跑一次
   `SMOKE_SKIP_BOOT=0 bash os/qemu-tests/test-cmd-smoke.sh`，完整记录
   exit code + 串口到达序列。这是"它的聚合产物到底处于什么状态"的客观
   庭证——无论它自己的文档怎么说。若它有未提交工作区改动：先原样提交到
   保全分支（`git stash` 或直接 commit 到 nk4a-agent-wip），不留脏状态。
4. 若存在 `rewrite-notes/coordination/NK4A-HANDOFF-STATUS.md`
   （前一道停止令要求它写的）：读一遍当**地图**用——但按用户裁决，它的
   自述**仅供参考、不可信**：上下文只有 200k 的模型在多轮迭代后会对
   "当初为什么这么写"失忆甚至混淆，文档与代码矛盾处一律以代码为准，
   矛盾本身就是一条评审发现。

## §4 Phase 1：完整回归评审

对 `940ad8363..nk4a-agent-wip` 同时做**净 diff 评审**（整体效果）与
**逐 commit 评审**（中间态可能引入后又掩盖的错误；净 diff 才是出货面）。

### 4.1 危险面清单（逐条核验，次序即优先级）

1. **守卫/断言的删、移、放松**：`git diff | grep -E "^-.*(assert|expect|
   panic|debug_assert)"` 逐条定性——真修复（附新位置与理由）/ 症状压制
   （为了让 boot 走远而拆警报）/ 中性移动。已知它**移动了**
   `assert_bootstrap_outside_memmap`（至全部 LOADER_DATA 分配后的最终
   快照处）并删了 shim 的无声 panic handler——核验：①守卫新位置的语义
   是否仍覆盖 D-64②（LOADER_DATA 分配不得出现在 conventional 快照）；
   ②shim panic handler 删除后由谁接管、接管方在 EBS 前后能否输出。
2. **新增 unsafe 逐处审计**（接手时约 35+ 处，实测为准）：每处必须有
   SAFETY 注释且理由成立；boot 路径的 unsafe 特别注意端口 I/O、裸指针
   跨地址空间、Box::leak 生命周期。
3. **内存图语义（A2 灾难向量）**：内核传给 VM PMM 的 conventional 图
   ——①快照时点（必须在**全部** LOADER_DATA 分配后）；②boot 模块占页
   是否被排除（它的第二层修复声称解决"13 个 exclusion 切洞撑爆
   VmBootRegions"——核验该机制是否真在、是否正确、容量数学是否成立）。
4. **既有设计的完整性与存废**（用户特别关切）：对每个被删除 ≥20 行的
   既有函数/机制，判定三种之一——(a) 搬家（新位置可追溯）；
   (b) 有据重设计（附 [ARCH] 注记或成文理由，且文档同步）；
   (c) **无据损毁**（= 用户最担心的"因为 bug 乱改设计"）。重点核对
   `os/kernel/src/dm_coverage.rs`、`os/arch/src/arch/{paging,frame,boot}.rs`、
   `os/arch/src/x86_64/paging.rs`——这些是它的深改区，且都是启动正确性
   的承重墙。文档交叉验证：被改机制对应的 stage 设计文档
   （07-paging_init_design 等）是否同步；代码删了文档没删 = 红旗。
5. **三架构载体存活**：它给 8 个 `test-kernels/bootstrap/*` 各加了 1 行——
   核验是否破坏 aarch64/riscv64 载体编译（宿主 ulimit 逐个 spot check）。
6. **范围审计**：67+ 文件逐一确认属于 boot/VM/命令域；任何 drive-by 改动
   （与 nk4a 无关的顺手修改）单独列出。
7. **它的 STATUS 文档（若存在）逐声明核验**：文档说的每条"已修复/已验证"
   在代码与串口证据里找对应；对不上的就是一条发现。

### 4.2 已知陷阱（不要重交学费）

- 改 `x86_64.ld` 后**必须** `touch kernel-image/src/main.rs` 强制重链
  （cargo 不追踪 ld 依赖）；链接脚本须收编 rustc 的 `.lbss/.ldata` 拆分节
  （孤儿节会撑破镜像跨距对齐）。
- EBS（exit_boot_services）之后 `uefi::println!` 会死锁（crate 日志路径
  依赖已消失的 boot services）——EBS 后只能裸写 COM1（boot-shim
  `raw_serial`，已在库）。
- 验证 none 构建**必须走消费方的真实 feature 面**；手拼
  `--features ...,std` + none target 必炸（E0463）。
- docker 镜像无 x86_64-unknown-none/uefi target——这两类构建走宿主
  `ulimit -v 3145728`；常规 crate 走 docker minix-ci:1.94。
- `ulimit` 下逐包构建；整仓 build 禁止（内存）。

## §5 Phase 2：验证矩阵（全部实测，不采信任何人的声称）

| # | 验证项 | 命令 | 判据 |
|---|---|---|---|
| V1 | 它 tip 的冒烟 | `SMOKE_SKIP_BOOT=0 bash os/qemu-tests/test-cmd-smoke.sh` | exit 0 且串口含 `rc: minimal boot script marker` |
| V2 | 13 包宿主回归 | docker `cargo test` 逐包（fileops/textfilter/stdio-games/regex/text-games/doctools/proctools/editor/termctl/compress/diskfmt/mountinfo/shell） | 全绿 |
| V3 | 内核/rt/init 宿主回归 | `cargo test -p minix-kernel -p minix-rt -p minix-init`（mock 面） | 全绿 |
| V4 | xtask 契约 | `cargo test -p xtask` | 全绿（含 BOOTX64/startup.nsh 断言） |
| V5 | 三架构载体 spot | 宿主 ulimit build test-higher-half/test-kernel-map 三 arch | Finished |
| V6 | guest 工件面 | ulimit build x86_64-unknown-none 全 commands 包 | 68 bin |
| V7 | clippy 对账 | 逐包 `--all-targets`，基线 = 940ad8363 | 零新增 |

V1 是唯一翻绿判据；V1 不过则它的"聚合产物"无论内部多精巧都判
**未完成**，进入 §7 重做流程。

## §6 Phase 3：评审报告（交用户）

三栏定性 + 逐 commit 表：

```
| commit | 定性（可信/存疑/需重做） | 一句话理由 | 复核方式 |
```

附：守卫变动清单、unsafe 清单、既有设计存废清单（§4.1-4 三分类）、
V1-V7 结果表、以及你的总裁决建议（保留它的产物 / 保留但修补 /
reset 重做）。用户裁决后才进 Phase 4。

## §7 Phase 4：重做执行（reset-and-redo）

触发条件（满足其一）：V1 不过；或评审发现 (c) 类无据损毁且影响启动
正确性；或用户直接批准。

流程：
1. `git branch nk4a-agent-wip` 已保全（§3 第 1 步，确认存在）。
2. `git reset --hard 940ad8363`（rewrite 回基线；此时你拥有：完整诊断
   路标体系 + ld 收口 + BOOTX64 修复，**没有**它的 VM 侧改动——boot 会
   重新挂回它所修的位置，这是预期）。
3. 重做 = 重走它踩过的坑，但每步按正确性标准做：
   - 跑冒烟 → 读串口挂点 → 定位 → ≥2 方案对比 → 修 → 复跑。
   - 已知重做队列（按它 diff 反推的顺序）：①memmap 快照移到全部
     LOADER_DATA 分配后（含 kernel+12 模块）；②BOOT 段身份窗口/VM
     bootstrap 容量问题（它为此建了 vm_handoff 847 行——先用最小正确
     修复让 boot 走通，机制化留给后续 code-excellence 批）；③kmain
     Phase A 及后续。
   - 每修一层提交一次（小 commit），诊断路标保留（最终裁决去留）。
4. 翻绿后补：run_all 接线、13 包回归、账本 ✅、FIXLOG。

## §8 交付物清单

1. `rewrite-notes/coordination/NK4A-REVIEW-REPORT.md`（§6 报告）。
2. 保全分支 `nk4a-agent-wip`（永删不得）。
3. 逐 commit 定性表 + V1-V7 结果（贴给用户）。
4. （若执行 Phase 4）重做 commit 序列 + 翻绿证据 + 账本/FIXLOG 更新。
