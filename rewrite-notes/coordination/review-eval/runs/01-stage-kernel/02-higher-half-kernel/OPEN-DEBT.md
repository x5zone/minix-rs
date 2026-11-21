| R3-design-batch | P2 | [CLOSED] 已实施：I-45..I-48 落码（SegmentIter `Err`、Safety 前提、buffer 两循环、store 别名收口）；arch link.ld 项迁入 R4-design-batch。证据见 `os/libs/minix-elf/src/lib.rs`、`os/boot-shim/src/loader.rs` 与 `certify/round3-certify.md`。 |
| R4-design-batch | P2 | [CLOSED] 已按 `D-107` 实施：D2-06（三份 arch `link.ld` + `link_ld_lockstep.rs` 删除、文档/映射同步）提交 `503ae169e`；D2-04（`build_kernel_info` 改收 `&BootReport`、删 12 处 `from_raw_parts`、新增 `report_static` 唯一寿命桥）提交 `309cd6e2c`；D2-05（`jump_to_kmain` 删 `stack_top` 形参）提交 `83e8cbb2b`。D-105/D-106 的延期文字已被 D-107 取代，不再是未清出口。 |

## D-107 清零批终态

- 本表两行均为 `[CLOSED]`；D-107 覆盖了 D-105/D-106 的“延期至下一 design batch”，对应代码已在本轮落码并提交（见 R4-design-batch 行）。
- 后续设计改进（`source_records` 命名、`RamAuthority` 值类型、`free_pt_page` fail-closed）已在 R35/R36 批实施，不构成本表欠账。
