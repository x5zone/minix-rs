# D01 章内修复轮记录

DATE: 2026-11-24
DOC: `01-boot-shim-bootstrap.md`
DOC_SHA256: `db6586a2babe3563d24d043792bc0d1272e95621fe54ba5088fce1c5fbf2d909`
LINES: 1987

## 结论

章内 HARD 与主要 P1 已修；但六视角重跑在修复过程中持续暴露新问题，**6/6 未达成，`CHAPTER-FREEZE.md` 不更新，最终整篇六视角不重跑**。
当前面板报告多数对应修复前切片，只能作定位证据，不能作为冻结凭证。

## 已修

### Ch1

- 入口与小结改为“三类信息：两类要带走，镜像位置由链接布局重建”，消除“带走三类数据”与 R1 的冲突。
- 角色映射：引导器=发送侧/内存图转交者，固件=声明者，内核=接收方；发布时发送侧即所有者。
- I2 表补“含由链接布局重建的镜像位置”；I3 表失败现象与正文对齐；I2/I3 来源与 I4 示例对齐。
- §1.4“架构启动篇”死指针改为“02 篇与 04 篇”；“第 2/3 章”加“本篇”；章末写“第 2 章”。
- 术语补链接布局、固件/引导器、声明者（来源）/所有者；删除 Ch1 的「内存账」越界条。

### Ch2

- “三层按值复制”改为“结构体整份拷贝 + 三类指针载荷（命令行、模块表、内存图）落盘”。
- 寄存器魔术值句改为 `%eax`=info magic、`%ebx`=info 结构地址。
- 镜像位置统一为 `_kern_phys_base`/`_kern_size`（链接布局）；`bootstrap_*` 只是 unpaged 段；识别头五个 0 不再说“让引导器决定装载位置”。
- `内存账` 术语统一，粗档定义限定为“可能含保留区”；`kinfo` 三写法对齐；62 行模块槽改为“第一个空槽”。
- D1 次序、I2 范围、`freepde_start` 交付面、`mmo_end`→`mod_end`、`pg_alloc_page` 消费点、调用点补 `pre_init.c` 等修正。
- §2.4.1.1 提升为 §2.4.2，原 §2.4.2 顺延 §2.4.3；相关指针全部改指 §2.4.3。

### Ch3

- `memory.c:createpde` 链接文字改为 `createpde`，anchor 全解析。
- `freepde_start` 赋值补 02 篇 §2.4；C 侧链条与 Rust 三前提分开。
- 02 篇指针统一到 §2.2/§2.4/§2.5/§3.6/§4.5；章末补“第 4 章”。
- `reserved_regions` 三处定义统一为“固件声明为保留，运行期可经 Direct Map 访问，但不进入空闲分配”。
- `§2.4.2` 误指全部改为 `§2.4.3`（16 条截断、跳板、pg_alloc_page）。
- 信封偏移表“本篇不复制”改为“见 §3.2.3.1”。

### Ch5

- 宿主测试的 `[结构保证]` 改为 `[观测]`，结构项保留 `[结构保证]`；分档规则改为按 §5.1 行内标签读。
- 门链计数改为“§5.4 表列五条链（含一条测试链）”；补 aarch64 UEFI 与直载测试链；`0 entries` 标为空表负控。
- 删 A2 重复定义与“冻结的裁决”过程语；补生产腿/v1/直核面/A2 术语；补 831/866 读数说明；小结压到 5 条。

### Ch6

- 入口补 6.5–6.8；前置概念补 Ch2 并合并 Ch3 重复项。
- 完整 `raw_serial`/panic handler 代码块压到 2–4 行示意；删除删除线 `bootstrap_len` 历史行与“已裁决”过程语。
- §6.5 改为正确/错误路径对照；补 check-layout 路径、页数公式、脚本名、CR2/epc 术语。
- 小结压到 5 条，补失败模式与 panic 约束。

## 面板状态（当前切片）

| 章 | 已有结论 | 仍需 |
|---|---|---|
| Ch1 | beginner READY、g18 PASS、editor PASS-WITH-FIXES、reference READABLE | navigator/instructional 复核已启动，报告未齐 |
| Ch2 | beginner READY、navigator PASS；g18 HARD 2、reference HARD 3、instructional P1 已修 | 需在最终切片重跑 g18/reference/instructional/editor |
| Ch3 | beginner READY、editor HARD 3、g18 PASS、navigator READABLE、reference READABLE、instructional READABLE | editor HARD 已修，需重跑 editor |
| Ch5 | 旧面板 HARD 已修 | 最终切片六视角未跑 |
| Ch6 | 旧面板 HARD 已修 | 最终切片六视角未跑 |

## 机械门

style error 0、contract PASS、rubric 0 缺项、citation failed 0、ref 全解析、anchor 93/93、六份摘要锚点 PASS。

## 下一步

1. 在最终切片 `db6586a2` 上重跑 Ch1 的 navigator/instructional、Ch2 的 g18/editor/reference/instructional、Ch3 的 editor。
2. 跑 Ch5/Ch6 六视角。
3. 全部 6/6 后再更新 manifest、`CHAPTER-FREEZE.md`、summary-faithful 事件与切片快照。
4. 最后重跑整篇六视角；R7/R8 仍待处理。
