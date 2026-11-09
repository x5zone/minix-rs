# R24 待裁决事项——用户裁决（2026-11-09）

> 五项均按主 agent 建议裁决。**主对话正在并发实现中**——side chat 不改代码，只落账裁决。

## 裁决总表

| 编号 | 裁决 | 状态 | 实施要点 |
|---|---|---|---|
| DESIGN-01 | **删除** `ident_regions()` 三层空通道（方法 + `map_kernel` 参数 + 断言） | **待实施** | 类型已表达"没有这个输入"；删后 `map_kernel` 不再收 `ident_regions` 参数 |
| DESIGN-02 | **补长度**：平台描述符窗口按 blob 实际大小扩展覆盖范围，不再只取地址所在的 2 MiB 片 | **待实施** | `PlatformDescSource` 需加 `len` 字段或从 DTB totalsize 推导 |
| DESIGN-03 | **改 fail-fast**：`push` 对反序/零长度 panic（不再静默丢弃） | **待实施** | 当前调用点无一处依赖静默丢弃 |
| DESIGN-04 | **按窗口表推导上限**：Step 0 断言从"物理上限 ≥ 4 GiB"改为"各窗口物理区间可寻址" | **待实施** | BOOT-SCOPE 2a 已改为最小窗口，断言引用的前提已退役 |
| DESIGN-05 | **类型标记**：`BootReport` 的 source/startup 字段加显式标记，表明 UEFI 腿此两字段为空（数据在 wire 分区） | **待实施** | 文档化现状（§3.4 已有），类型层面防 UEFI 腿消费者误判 |

## 详细裁决理由（逐条，含代码约束）

### DESIGN-01：`ident_regions()` 三层空通道

三层全空（方法恒返 `&[]` → 调用方原样传 → `map_kernel` 只断言为空），类型系统本来就能表达"没有这个输入"。删除方法、参数与断言；如果未来有非空恒等映射需求，届时再引入。

### DESIGN-02：平台描述符窗口

`push_2m` 只取地址所在的一个 2 MiB 片，跨边界的 DTB 只有前半片在恒等窗口里。当前没有可复现缺页（消费者 `init_from_kinfo` 运行在 DM 之后），但窗口语义与文档的"固件描述符表"不一致。裁决：补长度。

### DESIGN-03：`push` 静默丢弃

反序/零长度是调用方 bug，静默丢弃把错误推到 enable 后取指。裁决：改成 fail-fast（panic 或返回 Err）。

### DESIGN-04：Step 0 的 4 GiB 上限断言

断言引用 BOOT-SCOPE 2a 已退役的"4 GiB 全铺"前提，BOOT-SCOPE 已改为最小窗口。裁决：按窗口表推导上限（窗口内可寻址即可，不要求物理上限 ≥ 4 GiB）。

### DESIGN-05：UEFI 腿的 `BootReport` 部分视图

UEFI 腿的 source/startup 记录由分类器直接写 wire，`BootReport` 对应字段为空切片——类型与 U-Boot/直核腿完全相同，但数据不同。当前 `enter_kernel_image_v1` 不读这两个字段，所以是潜伏状态。裁决：类型标记，防未来消费者误判。

