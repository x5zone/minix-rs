# 章契约与章级 review（V3.13）

> 本文件是章级边界与章级六视角的唯一规范源。`tools/chapter-contract-check.py` 只实现这里的
> 机械可判部分；教学合理性、节序与文风仍由章级六视角与 meta-review 判定。

## 一、章契约

每篇正式文档按 `## N.` 分六章正文；可选 `## 7. 参见` 是索引，不参与章契约。每章只允许写下列内容，禁止跨界：

| 章 | 只允许 | 禁止 |
|---|---|---|
| Ch1 概述 | OS 理论、问题模型、通用不变量、最多一两句“本项目支持三架构”的背景 | 项目类型名、协议名、字段表、wire/ABI、Rust 标识符、代码块 |
| Ch2 C 源码 | Minix3 C 行为、源码锚点、C 侧判据 | Rust 设计、项目类型、代码走读 |
| Ch3 Rust 设计 | 设计契约、类型/协议/取舍、与 C 的差异 | 逐行代码走读、重讲 Ch1/Ch2 |
| Ch4 实现 | 代码路径、失败落点、实现细节 | 重新论证设计方向、重讲 Ch3 |
| Ch5 证据 | 测试/真机门/“不证明什么” | 新设计、新机制 |
| Ch6 诊断 | 现象、路标、命令 | 新机制、设计论证 |

### 1.1 Ch1 的抽象级不变量

Ch1 的 `I1–I4` 只保留概念级表述：例如“跨边界传递的值不能依赖发送侧地址”“接收方必须有
可追溯的来源”“载荷有唯一所有者”“校验先于消费”。不得出现 `BootReport`、`.bss`、
`store_kernel_info`、`KernelInfo` 字段表等实现判据；最多一句“具体载荷类型与协议见 Ch3”。

### 1.2 机械判定

`tools/chapter-contract-check.py` 按 `## N.` 分章（围栏外识别，要求至少一章、章号不重复、围栏闭合；首章前言并入 Ch1 扫描）：

- Ch1：扫禁用标识符（至少 `KernelInfo`、`BootReport`、`BootStorage`、`wire`、`RamAuthority`、
  `SourceRecord`、`KernelInfoView`、`BOOT_HANDOFF`、`KERNEL_INFO`、`v1`、`v9`、`v10`）与
  围栏代码块；任一命中即 FAIL。
- Ch2：扫项目 Rust 类型/函数名（C 对照行白名单只允许出现 `Rust` 这个词本身）；命中即 FAIL。
- Ch3：单个围栏代码块 >10 行 → FAIL。
- Ch4：项目类型名/设计论证词先只报 WARN，避免误伤。

## 二、Outline 合同

每篇一份 `rewrite-notes/meta/design/{stage}/{NN}-outline.v{N}.md`，必须含：

1. 核心命题；
2. 每章知识点与节序；
3. **不写什么**（边界）；
4. 下游导出接口（下一章可以引用什么）；
5. 一页“上一章回顾”。

Outline review 只批准边界与节序，不评审正文。`tools/outline-contract-check.py` 要求 `## 核心命题`、`## 知识点/节序`、`## 不写什么`、`## 下游导出接口`、`## 上一章回顾` 五个标题；缺项 FAIL。outline 位于 `rewrite-notes/meta/design/`（gitignore 的中间产物），因此 CI 只跑该工具的 `--self-test`，活 outline 由本地/章级 review 检查。

## 三、章切片与冻结

- 章级 review 前按 `## N.` 从主文档切出 `tmp/review-loop/01-stage/chapters/<doc>/chN.md`；
  reviewer 只拿 `本章切片 + 本章契约 + 上一章一页回顾 + scope 本章条目`，不拿整篇。
- 章级六视角：beginner-me、G18、编辑、导航者、效率读者（brief 文件名 `reference.md`）、教学设计师；另有 Reader-B 结构报告。G18 章级变体只覆盖
  本章单元，固定输出头 `DOC`、`CHAPTER`、`HARD-REJECTIONS`、`VERDICT`，不冒充全篇 G18。
- **冻结条件**：本章六视角 6/6；Ch1 另加理论事实核对，Ch2 另加 fact 门。
  任何一票不过 → 修根因，不冻结、不进入下一章。
- 冻结后写 `CHAPTER-FREEZE.md`（章 sha、单元哈希、六报告路径、允许下游引用/禁止修改范围）；
  下游发现矛盾只定向重开相关单元并重跑对应视角，不整章重读。
- Ch1/Ch2 冻结时生成“一页接口摘要”（每概念一行定义 + 原文锚点）；Ch3/Ch4 reviewer 只拿
  摘要 + outline + 设计合同 + 冻结代码 + 本章。

## 四、整篇六视角

冻结章视为只读；整篇报告只聚焦跨章重复定义、术语漂移、章间过渡、指针落空、
摘要↔原文不一致，不逐行重审冻结章。整篇 6/6 才算读者验收收敛。
