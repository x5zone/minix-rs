# D01 章级六视角重跑记录（R1–R6 修复后）

DATE: 2026-11-24
DOC: `01-boot-shim-bootstrap.md`
DOC_SHA256: `b5ceb02cc8834e640711340d8e88808213e7d104e982084a5411a43fc55c5cff`（2017 行）
SCOPE: 只记录 R1–R6 修复后的章级重跑；不代替整篇六视角。

## 结论

**章级 6/6 未达成，`CHAPTER-FREEZE.md` 不更新，最终整篇六视角不重跑。**
重跑在 Ch2/Ch5/Ch6 找到新的 HARD 与多条 P1，Ch1 的最终报告因后续修复失效，Ch3 仍有 NEEDS-WORK。
这不是 R1–R6 的回归，而是严格冷读暴露的既有章内问题；需要单独一轮章内修复。

## 各章状态

### Ch1（当前切片 `f06ac18f`，132 行）

- 无有效章级报告。六份报告在切片 `557f680f` 上完成，之后又修了发送侧/转交者/声明者角色映射与术语表；这些报告已归档到 `chapters/ch1/superseded/rereview-557f680f/`，不得作为当前正文的冻结证据。
- 已修：L18 角色映射（引导器=发送侧/转交者，固件=声明者，内核=接收方）；术语表补“转交者”。Ch1 需重新起六视角。

### Ch2（切片 `5adbbe1f`，435 行）

六份报告全部 NEEDS-WORK；HARD 合计 4（beginner 1、g18 3）。

- g18 HARD：L35 “两个寄存器各放一个魔术值”与 L39 表/L86 asm 冲突（`%eax` 是 info magic，`%ebx` 是 info 结构地址）；L118/L122 “三层”与 L120/L397 的四步载荷对不上；L342 “各有一份实现”与 L356 “不需要第二份实现”冲突。
- beginner HARD：同一“三层按值复制”无法枚举为三层。
- 其他：`内存账` 术语表“过滤后”与粗档正文冲突；L32 与 L52/L128/L289 的镜像位置口径未统一；§2.5 表与 §2.1/失败对照在模块条数判据上冲突；D1 主链与 §2.8 对 `get_parameters`/`cut_memmap` 次序画法不同；`freepde_start` 被列入“交付物”却又说不是交接数据；`mmo_end`/`mod_end` 不一致；术语首用先于定义。

### Ch3（切片 `848bfdf1`，586 行）

已有报告：beginner READY、editor READY、navigator NEEDS-WORK、reference NEEDS-WORK；g18 停滞被停止，instructional 只有旧切片报告（未计入）。

- navigator：`[`memory.c:createpde`](…)` 的链接文字被 `anchor-resolve.sh` 当裸锚点，`memory.c` 多定义，全篇 anchor 读数 exit 1；章末无“第 4 章”指路。
- reference：`freepde_start` 的 02 篇 §2.4 指针未落到 Ch3 正文；L351 把 Rust 侧三前提的完整陈述错误归给 C 源。
- 其余为空格/节号精度等 P2。

### Ch5（切片 `4a4054b3`，103 行）

已有报告：beginner HARD 5 NEEDS-WORK、editor NEEDS-WORK、instructional HARD 2 NEEDS-WORK；g18/reference/navigator 报告缺失或为旧切片，不计入。

- 证据标签：`[结构保证]` 被用在 round-trip、`should_panic` 负控等运行期测试上，与 §5.3 的“不依赖运行环境”定义不符。
- 门链计数：§5.1 的“五条生产腿/五条门链/四条有门链的腿”与 §5.4 表（含一条测试链、排除 aarch64 直核）未对账；“五条链能走到各自终点”抹平覆盖深度。
- 判据不可执行：bootface 的 “table holds 0x…（归档示例是 0 entries）”未说明 0 entries 为何算 PASS；`DTB_CFGTABLE=1` 的设置者与 AAVMF 分支未交代。
- 其他：A2 同句重复定义；“本表”无先行词；§5.2 真机门清单漏 aarch64 UEFI 脚本；宿主读数 866/0/8 与项目指令的 831/0/8 差异未说明。

### Ch6（切片 `90b309c3`，227 行）

已有报告：beginner HARD 1 NEEDS-WORK、instructional HARD 3 NEEDS-WORK；其余四份被停止，未计入。

- beginner：L136 “发送侧走完、接收端未起”指向 §6.4 第三层，但第三层只给 riscv64 QEMU 命令，x86_64 UEFI 读者无法照做。
- instructional：L70 的“v4→step1+2 之间”与 L136/Ch5 的“step1+2 之后”冲突；L20–31 与 L74–111 的完整函数/impl 超出 2–5 行代码规范；L137 整行删除线 `bootstrap_len` 与 L195 “已随字段删除”把迭代历史带进诊断章。
- 其他：§6.5 与 §6.2 近逐字重复；§6.4 未给每条失败模式的复现命令；术语表缺生产腿/v1/直核面。

## 下一步

1. 按章修 Ch1/Ch2/Ch5/Ch6 的 HARD 与 P1，Ch3 修 anchor 文本与两处指针；每章修完重跑六视角。
2. 六视角 6/6 后再更新 `chapters/chN-manifest.md`、`CHAPTER-FREEZE.md`、`summary-faithful` 事件与切片快照。
3. 之后才重跑最终整篇六视角。
4. R7（入口计数）与 R8（02 反向引用）仍待处理。

## 证据

- 报告目录：`v312/chapters/chN/`；Ch1 旧报告：`v312/chapters/ch1/superseded/rereview-557f680f/`。
- 修复清单：`v312/final/REPAIR-R1-R6.md`；R1–R6 独立复核：`v312/final/REPAIR-VERIFY.md`（PASS，针对 `5023b454`）。
- 机械门（当前 `b5ceb02c`）：style error 0、contract PASS、rubric 0 缺项、citation failed 0、ref 全解析、六份摘要锚点 PASS。
