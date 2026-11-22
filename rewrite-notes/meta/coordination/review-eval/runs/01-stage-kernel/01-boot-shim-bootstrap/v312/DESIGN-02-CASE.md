# D01 R1 阻塞项案件卷：DESIGN-02（设计批级）

- 日期：2026-11-23
- 状态：`BLOCKED`；不宣告 `ROUND-CLOSED` / `CONVERGED`
- 来源：D01 V3.12 R1 面板 `DESIGN-02`（P2，`[DEL]`），并入台账 `R1-24`
- 影响单元：3.2 / 3.7 / 4.4

## 事实

五个生产腿有两种“发送侧记录”形态：

- 三条 v1 腿把 `&BootReport<'_>` 交给 `enter_kernel_image_v1`，借用生命周期由调用栈绑定；
- 两个直核腿却在 `os/kernel-image/src/bootface_common.rs` 里用
  `static STORAGE: BootStorageCell` + `STORAGE.take()` + `STORAGE.report_static()` 把同一份借用
  抬成 `'static`，再经 `os/boot-shim/src/lib.rs` 的 `build_kernel_info()` 逐字段搬进
  `KernelInfo`（其切片字段是 `&'static [T]`）。
- `STORAGE` 在 kernel-image `.data` 常驻约 10.5 KiB，进入 `kmain` 前又整份复制进 `.bss`；
  同一批字节存在两份。
- `report_static` 是全仓唯一把借用抬成 `'static` 的安全桥；它依赖“boot 单线程 + fill 已完成”
  的口头契约，`report_static` 自身不看 `taken`。

## 为什么它不能在 R1 轮内实施

`prompt/review-rules/review-publication.md` §四.6 写明：

> 设计批级改动（跨 crate 的 `wire` 版本、所有权/借用模型、ABI 布局等）不在轮内实施：
> 中止本轮，回到 Manuscript 阶段落地、重设基线，再重新开轮。

DESIGN-02 要改的正是直核发送侧的所有权/借用模型，并跨 `boot-shim`、`kernel-image`、`kernel`
三个 crate；按该条属于设计批级。

同时 `review-publication.md` §三（零欠账）关闭了 `⑥` 转下一设计批的出口，唯一合法例外是
`DECISIONS.md` 里作者亲笔的 `defer-approved` 判决行（日期 + 具体条目 + 到期条件 + 不做后果）。

## 现在的事实状态

- R1 台账 40 个根因：39 个已修并进入独立复验；唯一 open 是 R1-24。
- `review-units.py audit`：open=3（3.2 / 3.7 / 4.4，均来自 R1-24）。
- 本轮不冻结全部单元、不写 `FREEZE.md`、不宣告收敛。
- 文档其余 P1/P2/P3 修复已落地，文档门全绿；D01 的可读性不应被这个代码设计债阻塞。

## 需要作者裁决的选项

### A. 立即做设计批（Manuscript）

- 中止本轮，回到 Manuscript 落地 DESIGN-02；可选入口签名：
  (a) `KernelInfo<'a>` 泛化生命周期，或 (b) 直核入口收 `&BootReport<'_>` + authority。
- 删除 `BootStorageCell` / `report_static` / 直接调用 `build_kernel_info` 的路径与 `STORAGE`；
  `'static` 只在接收侧 `.bss` 深拷后生成。
- 代价：重设基线、重跑受影响测试与 QEMU、重启 R1 面板；本轮 39 个文档修复需要在新基线上复核。
- 收益：消除全仓唯一的伪 `'static` 桥与 10.5 KiB 常驻存储；文档 §3.7/§4.4 可删掉一半解释。

### B. 作者亲笔 `defer-approved`，把设计批排到 D01 出版后

- 在 `DECISIONS.md` 写一行：日期、`R1-24`、到期条件（例如“D01 FREEZE 后第一个边界必须实施”）、
  不做后果（保留伪 `'static` 桥与常驻 `STORAGE`，文档继续按现状描述）。
- 本轮可把 R1-24 记为已批准的延期，继续走完全部单元冻结与确认轮；D01 可以先交付阅读。
- 代价：代码债保留；到期未做则该项重新 open。

### C. 判 FP / 撤回

- 不推荐。面板给出了源码位置与语义论证，主 agent 复核后认为不是误报；把可修的设计债判 FP
  会破坏台账可信度。

## 建议

- 若优先“明天能读 D01”：选 B，作者亲笔一行 `defer-approved`，把设计批排到 D01 FREEZE 后；
  文档已经按现状准确描述，读起来不会被代码债卡住。
- 若优先“D01 出版时代码也干净”：选 A，但需要接受重设基线与重跑面板的成本。
- 无论选哪个，R1-24 在裁决前保持 open；不得由 me-agent 代批新的 defer。
