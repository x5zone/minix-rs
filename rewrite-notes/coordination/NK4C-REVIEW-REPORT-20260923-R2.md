# NK4-C 评审报告 R2（增量评审 + 交接方自审）

- **日期**：2026-09-23（接 R1：`NK4C-REVIEW-REPORT-20260923.md`，覆盖 `56d6dec4c..e90efbcd8`）
- **范围**：①增量——另一 agent 收尾提交；②自审——交接方（上一棒）2026-09-23 当日全部提交
- **方法**：code-excellence（分层设计审视 + 非法态封堵清单 + 死代码清单 + 文档同步 + 实测验证）
- **结论先行**：①**无未审增量**——另一 agent 的会话止于 `e90efbcd8`，其"收尾"（1.7 定性 + tail-dump 探针）已在 R1 覆盖，本轮以四重证据核实无遗漏；②自审发现 **1 项已修复缺陷（kdst 门，另一 agent 代修）+ 1 项死代码 + 1 项文档同步缺口 + 1 项交接骨架错误（已被对方 S2c 证伪并修正）**，EFER.NXE 修复本体经设计对照与代码复审成立。

---

## 一、增量评审：无未审提交（四重证据）

| 证据 | 结果 |
|------|------|
| `git log e90efbcd8..HEAD` | 仅 1 笔 = R1 评审报告本身（`d2f822889`），无对方新提交 |
| `git status` WORKLOG | 与 HEAD 零差异（mtime 23:02:49，早于 R1 的 23:16） |
| `git reflog -15` | 对方会话最后一笔 = `e90efbcd8`（1.7 tail-dump），其后仅本方 R1 |
| `git fetch` + 全远程 | `origin/rewrite` 落后本地 1676 笔；`trae/solo-agent-*` 为无关文档线 |

**判定**：另一 agent 的「收尾」= `a400eb44e`（1.7 定性）+ `e90efbcd8`（tail-dump 探针）——**R1 已完整覆盖**。其收尾日志即 WORKLOG 顶部「新停点（阶段 1.7 处置，已定性=硬 livelock）」节。本轮评审人另做了一次独立真机复跑（`serial_rev_s1`：449 轮、尾态逐字一致、零 panic/零 rc marker），与 R1 结论互证。

## 二、交接方自审（2026-09-23 当日 13 笔代码/文档提交）

### 2.1 `1d25f433e` EFER.NXE 修复 —— 设计复审

**设计对比**（"如果今天重写会怎么设计"）：

| 方案 | 内容 | 判定 |
|------|------|------|
| A. 仅 AP asm | 只修 AP 冷启动 | ❌ BSP 仍依赖固件继承（UEFI 恰好开了 NXE），不变式不归内核所有 |
| B. 仅 BSP enable() | AP 的 asm 是唯一 MSR 编程点，够不着 | ❌ AP 必炸如故 |
| **C. 双侧（已选）** | AP asm `or eax,0x800` + BSP `enable()` RMW 显式置位 | ✅ 与 Linux（top-level 与 secondary startup 均置 NXE）、Redox 同构：「每 CPU 在启用分页前置 NXE」是 OS 理论标准姿态 |

**代码复审**：AP asm 的 `rdmsr/wrmsr` 在 32-bit protected mode 合法（非 64-bit 专属指令）；RMW 保固件 SCE/LME 位；BSP 侧内联 asm 的 `in("ecx")/out("rax")/out("rdx")` 用法正确；两次验证（c29b per-CPU dump → c30a 风暴清零）证据闭环。**首版 `0x1000`（bit12=SVME/保留位）笔误在工作树内自纠，未入库。**

**非法态封堵评估**（excellence 清单）：非法态 =「NXE 关闭 + 存在 NX 标记页表树」。当前封堵 = 构造点（AP asm + BSP enable），**无运行期不变式检测**——未来新增 CPU 启动路径若绕过这两点，非法态静默重现（本轮 bug 即此形态）。**建议（登记不实施）**：在 per-CPU 初始化收口处（如 `init_protection`）加 `rdmsr(EFER) & NXE` 断言，一行成本把非法态从"静默风暴"变"显性 fail-fast"。

**文档同步缺口（excellence 清单）**：`NK4A-HANDOFF-STATUS.md:229`（fix27e 注记）仍写「OVMF 已开 EFER.NXE」——这正是被 AP bug 证伪的安全性论据（它只覆盖 BSP）。修复后不变式已改为内核自持，**设计注记未随改**。建议在 fix27e 相关设计文档补一句「NXE 由每 CPU 启动路径自持（`1d25f433e`），不依赖固件继承」。

### 2.2 探针组（`967a903e7` + `85a0d7cd8`）—— 卫生审计

| 审计项 | 结果 |
|--------|------|
| mock 门一致性 | ❌→✅ **kdst 三调用点 `not(test)` vs 定义 `not(feature="mock")` 不一致，宿主 mock 构建 E0425 必炸**——交接后由另一 agent `200a016a8` 修复（归责已在其 commit 注明）。其余探针全部门正确：`dm-cov`/`i33-save`/`cr3`/`msgw` 走 `not(feature="mock")`，`pte-wb`/`sas-send`/`vmpt2bf`/`PT_SEEN` 走 `bootmark::mark`（内置 cfg(test) 安全）或 `not(test)`+bootmark |
| 去重/过滤合规 | `kdst` 按 (tag,pa,len) 去重 ✓；`msgw`/`i33-save`/`cr3` 为无条件计数 cap——**存在采样饥饿风险**（msgw 64 条被启动期消息投递耗尽的风险真实存在），判别窗口场景需按铁律 2 加过滤（后续轮已按此实践） |
| 写生产上下文的探针 | 金丝雀已摘除（`967a903e7`），`trap_dispatch.rs` 仅余注释引用——**无残留上下文写探针** ✓ |
| `task1-close 裁决删除` 标记 | dm_coverage.rs/vm.rs/alloc_page.rs/cow_exec_pf.rs 四文件全有 ✓ |

### 2.3 死代码清单（excellence）

| 项 | 状态 | 建议 |
|----|------|------|
| `sync_slot_pte` 的 `pub(crate)` 放宽（`545763d02`） | **仍无 crate 外使用者**（全部 5 处调用在定义模块内）；原计划"第 20 轮 eager PTE sync"因根因改道未发生 | 回退为模块私有；或留待真实使用者出现时再放宽（倾向前者，excellence：不留投机可见性） |
| 交接期迭代 12-18 探针（`6d6c753f1`…`616783747`：毒化/池游标/refault/PA 别名/memset ret 等） | 全部为被动观察、已被根因修复与 S2c 证伪超越 | 属阶段 5「task1-close 探针大裁决」既定清理范围，无需提前单独删 |
| `kernel_call_resume`（syscall.rs，无调用方） | 另一 agent 已登记 task1-close 裁决 | 同上 |

### 2.4 交接骨架错误（自审认账）

1. **「PTE 物理消失」骨架被证伪**：我的迭代 25/26 把 48 次 `lvl1=0` dump 解读为"PTE 被抹"，并写进交接骨架第 1 条。另一 agent 的 S2c 哨兵实证：崩溃窗口内监视 VA 的 PTE 全程完好——**那些 lvl1=0 是正常 lazy 缺页（故障时 PTE 尚未填，本是需求缺页的定义）**。我的错误 = 把"故障时观察到 PTE=0"过度推理为"PTE 曾被写后被清"。WORKLOG「已排除」账已由对方修正；本报告再次显式认账，防止后续引用旧骨架。
2. **kdst 门不一致**：我的缺陷，对方代修（见 §2.2）。
3. **文档四笔**（`0329c62d2`/`56d6dec4c`/`65ab6fc8a`/`d2f822889`）：本轮已将 prompt 状态节更新至修复后现状（原 prompt 的 §4/§6/§8 仍描述"Task C 未修"，属交接后未跟进而非错误）；edge_todo 指针正确；R1 报告经对方工作验证结论无一需要撤回。

### 2.5 实测验证（自审的回归面）

HEAD（= 另一 agent 终态 + 我的 R1）：
- docker：`minix-arch 242` / `minix-kernel 810` / `minix-vm 526` / `minix-rt 57` 全绿（810 = 809 基线 + 对方 `reply_wire` 判别测试）；
- 镜像构建成功；真机一轮（`serial_rev_s1`）与对方终态声称逐字一致（449 轮 livelock、零 panic、零 rc marker）。
- 我的 EFER.NXE 修复在对方终态下仍有效（无 err=8 回归——449 轮全程无保留位违例）。

## 三、发现汇总（本轮新增）

| 级别 | 项 | 处置建议 |
|------|-----|----------|
| P2-doc | fix27e 注记「OVMF 已开 EFER.NXE」未随 `1d25f433e` 更新（安全性论据已被证伪） | 设计文档补 NXE 内核自持声明 |
| P2-dead | `sync_slot_pte` `pub(crate)` 无使用者（`545763d02`） | 回退私有或等真实使用者 |
| P2-design | NXE 非法态无运行期检测（构造点封堵已够，但无 fail-fast） | 登记：per-CPU init 加一行不变式断言（随 task1-close 或阶段 1.6 顺带） |
| P2-hygiene | `msgw`/`i33-save`/`cr3` 探针无条件计数 cap 有采样饥饿风险（本轮未实际造成损失） | 已在后续轮实践过滤；task1-close 统一删除 |
| 认账 | 交接骨架「PTE 物理消失」被 S2c 证伪；kdst 门缺陷 | 已由对方修正并传播至 WORKLOG；本报告存档 |

## 四、当前进度口径（重申 R1 §五，无变化）

| 计划阶段 | 状态 |
|----------|------|
| 1.1 / 1.2 | ✅（含 Task C 根因修复，三次独立真机验证） |
| **1.3 rc marker** | ❌ **当前阻塞 = 449-livelock**（PAGEFAULT→VM 投递/唤醒腿，tail-dump 已实锤矛盾形态，三入口已列，首推 P1-ipc `clear_ipc_refs`） |
| 1.4 | ◐ x86_64 完成，aarch64/riscv 待迁移（P1-arch） |
| 1.5 / 1.6 / 1.7 | ❌ 未开工（F11/F12/F13 属打通 1.3 的使能修复） |
| 阶段 2-5 | ❌ 未开工 |

## 五、新会话 prompt 更新说明

`NK4C-OPENING-PROMPT.md` 已随本报告同步更新：§1 阅读清单加入 R1 报告与门纪律代码锚点；§4 重写为修复后现状（Task C 闭环 + 449-livelock 骨架 + 全部登记项）；§6 路线图标注 1.1/1.2 ✅、1.3 为当前位置、1.4 余项精确化；§8 决策树改为 449-livelock 起步的三入口分支。其余（环境命令、铁律、节奏、停止条件）不变。
