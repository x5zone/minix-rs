# R24 待裁决事项——用户裁决（2026-11-09）

> 五项均按主 agent 建议裁决。**主对话已按五项裁决实施**；状态与证据见下表与 `DECISIONS.md` D-030..D-034。

## 裁决总表

| 编号 | 裁决 | 状态 | 实施要点与证据 |
|---|---|---|---|
| DESIGN-01 | **删除** `ident_regions()` 三层空通道（方法 + `map_kernel` 参数 + 断言） | **已实施** | 类型已表达“没有这个输入”；删后 `map_kernel` 不再收 `ident_regions` 参数。证据：`grep -rn ident_regions os` 零命中；`map_kernel` 7 参；kernel 865+8。 |
| DESIGN-02 | **补长度**：平台描述符窗口按 blob 实际大小扩展覆盖范围，不再只取地址所在的 2 MiB 片 | **已实施** | `PlatformDescSource` 增加 `len`（wire 16 B 原 padding）；DTB 读 `totalsize`、RSDP 读 `length`；`push_platform_windows` 按跨度建窗。证据：`PlatformDescSource::{with_len,len}`、`dtb_total_size`/`rsdp_total_size`、wire `off+4`；`platform_window_uses_the_recorded_blob_length`；minix-boot 196 / kernel 865+8。 |
| DESIGN-03 | **改 fail-fast**：`push` 对反序/零长度 panic（不再静默丢弃） | **已实施** | `BootWindows::push` 断言 `start < end`；`push_2m` 用 `checked_add`。证据：反序负控与单片测试；kernel 865+8。 |
| DESIGN-04 | **按窗口表推导上限**：Step 0 断言从“物理上限 ≥ 4 GiB”改为“各窗口物理区间可寻址” | **已实施** | 移除 Step 0 的全局上界断言；窗口组装后 `assert_boot_windows_addressable` 逐窗比较 `end <= phys_addr_limit`。证据：两枚上限测试；kernel 865+8。 |
| DESIGN-05 | **显式标记**：`BootReport` 的 source/startup 字段加 `records_in_wire` 咨询标记，表明 UEFI 腿此两字段为空（数据在 wire 分区）；当前无消费者、无运行期强制 | **已实施** | `BootReport.records_in_wire`（咨询标记，当前无消费者）；UEFI 发送侧置 true；文档 §3.4.5。证据：minix-boot 196。 |

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

