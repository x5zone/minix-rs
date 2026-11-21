# D01 R37 certify — fact / refute 回读

> 独立冷启动实例只读回读；本文件由主 agent 从回执转录，保留逐 ID 判定。

| ID | 判定 | 位置 | 说明 |
|---|---|---|---|
| FACT-01 | CLOSED | §5.3/§5.4 | 五条真机门链表存在；三层判别改指 §5.3 |
| FACT-02 | CLOSED | §3.7 | postconditions 只剩 `kern_virt_base > u32::MAX`；重叠归 `validate_impl_inner` |
| FACT-03 | CLOSED（复读） | §6.1/§4.4 | 删重复行；“preA”归 `kmain` 内临时打印；§4.4 不再归因 `arch_boot` |
| FACT-04 | CLOSED（复读） | §3.7/§4.4 | 直核 store 先于 boot_validate；v1 先 boot_validate 后 store，链接 `resume_boot_pt_alloc` |
| FACT-05 | CLOSED | §4.2 | `(&layout)`、`.validate()`、`layout.layout()` 与源码一致 |
| FACT-06 | CLOSED | §3.4.2 | 三处 load 为 `Acquire` |
| FACT-07 | CLOSED | §3.4.2 | 改指 §3.6；旧 `3.8`/`§3.4 与 §3.4` 已清 |
| FACT-08 | CLOSED | §3.2.1 | `FREE_MEMMAP` 阴性声明撤除 |
| FACT-09 | CLOSED | §4.3 | 补 mfs 专用 16 MiB |
| FACT-10 | CLOSED | §6.4 | 改查派生值/访问器 |
| FACT-11 | CLOSED（复读） | §4.1 | 改各架构页表根（Sv39 / TTBR0+TTBR1） |
| FACT-12 | CLOSED | §5.5 | 阶段占位与 R02-阶段 已清 |
| X1 | CLOSED | §5.4 | 表存在，引用可解析 |
| X2 | CLOSED | §6.1 | ghost `arch_boot arg/reslen` 已删 |
| X3 | CLOSED | §3.2.1/§3.2.2 | 22 字段账一致 |
| X4 | CLOSED | §3.2.1 | `FREE_MEMMAP` 零命中 |
| X5 | CLOSED（复读） | §4.4 | 补“非完整清单”与 `validate_impl_inner` 的额外检查 |
| X6 | CLOSED（复读） | §6.3 | 补 WB→Side→strict fail-closed 失败模式 |

## 回执结论

- 初读 12 CLOSED / 6 STILL-BROKEN；6 条修复后复读 6/6 CLOSED。
- 未覆盖：未运行 cargo/QEMU/机械门；未逐条复验全文其它语义断言。


## 6 个 CLAIM 的裁决（refute 原文 + 主 agent spot check）

| ID | 判定 | 依据 |
|---|---|---|
| CLAIM-01 | FP（反证失败） | `alloc_lowest` 与 `pg_utils.c`/`arch_smp.c` 一致；无缺陷 |
| CLAIM-02 | ①（空键副作用） | §2.3 其八补 C 空键删除会截断后续记录；Rust 拒绝该输入 |
| CLAIM-03 | FP（反证失败） | ARM32 闭区间判定在当前尺寸下无害；无缺陷 |
| CLAIM-04 | ①（postconditions/I4） | §3.7/§4.4 已按实现修正 |
| CLAIM-05 | ①（所有权措辞） | §3.7 限定 UEFI source 记录与 fill_* 例外 |
| CLAIM-06 | FP（反证失败） | manifest/调用面与构建图一致；无缺陷 |

说明：这 6 条是 refute 的对抗主张，不另派实例逐条回读；裁决依据是 `panel/round37/refute.md` 的源码反证与主 agent 的 spot check。FINDING 的独立回读见上表与 `round37-reader.md`。

结论: PASS（18/18 CLOSED，0 STILL-BROKEN）。
