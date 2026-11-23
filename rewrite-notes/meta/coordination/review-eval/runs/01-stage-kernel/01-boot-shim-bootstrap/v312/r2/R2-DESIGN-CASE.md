# D01 R2 设计裁决案件卷（DESIGN-01 / DESIGN-02）

- 日期：2026-11-23
- 来源：`v312/r2/panel/round2/design-d2.md`
- 状态：**已由作者裁决（2026-11-23 12:00）：DESIGN-01=A，DESIGN-02=A**；实现见 `DECISIONS.md` D-120/D-121 与 commit `7d6835aa5`。

## DESIGN-01（P2，[MODERN]）：合并两张模块身份表

- 现象：C 的 `table.c` 用一张 `{proc_nr, name}` 表描述 12 个启动镜像；Rust 拆成
  `minix-boot::MODULE_NAMES`（12 个名字）与 `kernel::BOOT_MODULE_PROC_NRS`（12 个进程号）两张
  按位置相关的表，分属两个 crate，没有编译期链接。strict 门只按位置核 `MODULE_NAMES`，因此
  两张表错位时不会在该门被抓住。
- 选项 A：合并成一张 `{name, proc_nr}` 表（D2 推荐）。净减一张表，关掉静默错位面；需要改
  `minix-boot` 的常量形状、kernel 的一处派生与相关引用，行为不变。
- 选项 B：保留两张表，增加一条编译期/宿主测试把 12 组 `(name, proc_nr)` 逐项钉住（D2 认为
  这会形成第三份抄本，不如合并）。
- 不做的后果：未来新增/重排启动模块时，名字表与进程号表可能单边漂移；症状是服务按错误
  进程号绑定，启动仍能到 `kmain`，到第一次系统调用才暴露。

## DESIGN-02（P2）：`kmain` 终门改用 strict validate

- 现象：`KernelInfo.authority` 的 strict 门只在 v1 接收侧与两个直核面的 `BootReport` 上跑；
  直核面把 `authority` 作为独立实参传两次（report 一次、`arch_boot` 一次），存储后的
  `KernelInfo.authority` 到 `kmain` 只跑非 strict `validate()`，不再按权威过滤记录。今天两处
  都写 `DTB`，无实际故障；结构上保留了“两个可接反实参”的面。
- 选项 A：`kmain` 的 `validate()` 改为 `validate_requiring_sources()`（D2 推荐，1 行）。
  `kmain` 不编进 mock/QEMU 测试内核；五条生产腿都携带记录，理论上不改变成功启动，但会把
  “记录与声明不一致”的失败提前到 `kmain`（更严格）。
- 选项 B：保持非 strict `validate()`，在文档里明确“`kmain` 只做非地址不变量复核；权威审计
  在腿级 strict 门完成”。这不关闭直核面“存储后不再审计”的窗口。
- 不做的后果：未来第三条直核腿或重构把两处常量写岔，`KernelInfo` 可带着错配权威进入
  `kmain`，直到后续严格审计才暴露。

## 裁决结果

作者选 **DESIGN-01=A、DESIGN-02=A**（2026-11-23 12:00）。实现与验证见 `DECISIONS.md`
D-120/D-121；R2 台账两项已从 ⑤ 转 ①。R2 仍因冷读 `NOT-READY`/R3 未开而 `BLOCKED`。
