| R3-design-batch | P2 | [CLOSED] 已实施：I-45..I-48 落码（SegmentIter `Err`、Safety 前提、buffer 两循环、store 别名收口）；arch link.ld 项迁入 R4-design-batch。证据见 `os/libs/minix-elf/src/lib.rs`、`os/boot-shim/src/loader.rs` 与 `certify/round3-certify.md`。 |
| R4-design-batch | P2 | **用户批准 2026-11-21（代理裁决 D-105/D-106；授权来源为用户 2026-11-21 review-loop D02 会话代裁指令，见 loop.md R4 §0）延期至下一 design batch**：DESIGN-D2-04（`build_kernel_info` 改收 `BootReport` 命名视图，删 12 处 `from_raw_parts`；改动 5 调用点/1 生命周期收紧）与 DESIGN-D2-06（三份无生产读者的 arch `link.ld` + `link_ld_lockstep.rs`：删除或补齐；`dup-of` R3-design-batch 的同一项与 V1 `D02-F03`）。确认轮不实施 ⑥（§3.6），登记后销账 |

## D-107 清零批

- `R4-design-batch` 的 D-105/D-106 延期已被 D-107 覆盖为**本轮实施**：D2-04（`build_kernel_info` 收 `BootReport`、删 12 处 `from_raw_parts`）与 D2-06（删除三份 arch `link.ld` + `link_ld_lockstep.rs`）均为 OPEN，直到清零批落码并附证据。
- `R3-design-batch` 已按实施关闭（见上表第一行）。
