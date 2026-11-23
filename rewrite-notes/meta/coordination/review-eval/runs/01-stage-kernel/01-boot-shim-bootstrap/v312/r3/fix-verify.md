DOC 01-boot-shim-bootstrap.md

# R3 修复批独立复验（doc + code）

VERDICT: PASS
RECHECK: PASS

- 文档 sha256: 67440cd885aad0e808cd5d76905323872b08b1c35849566b976c123d84dff6e8（3085 行）
- 文档复验: `tmp/review-loop/01-stage/r3-doc-verify.md`（23 项逐条、NEW-01..03；首轮 FAIL 的三项已修，RECHECK PASS）
- 代码复验: `tmp/review-loop/01-stage/r3-code-verify.md`（DESIGN-01..04；DESIGN-04 首轮 FAIL 的注释已修，RECHECK PASS）

## 覆盖

- 面板 22 个 FINDING、P-REFUTE 1 个 P2 观察与 2 条非阻断观察全部 ① 关闭。
- 独立复验新增 NEW-01（§4.4.1 时间序表 v1 次序）、NEW-02（§3.3 审计函数名）、NEW-03（§1.1.1 RamAuthority 形状）均 ① 关闭。
- 代码四项：authority 绑定断言 + 宿主负控、`kmain` 参数与全局 `ptr::eq`、Serializer 死计数器删除、五处过期注释。

## 关键重放

- `rg -n '五类错误各有落点' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` → 1 行。
- `rg -n '两个直核腿的六类载荷|两个直核腿各拥有一份栈上的' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` → 2 行（I3 行与 §3.7 所有权段）。
- `rg -n 'audit_source_coverage_for' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` → 1 行。
- `rg -n '两个关联常量' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` → 1 行。
- `sed -n '/按时间顺序把两条腿并排看/,/下表按门/p' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` → v1 列第 4 行 `resume_boot_pt_alloc`、第 5 行 boot_validate、第 6 行 `store_kernel_info → jump_to_kmain`。
- `rg -n 'authority_has_source_record' os/kernel/src/lib.rs os/libs/minix-boot/src/kernel_info.rs` → 定义、调用与负控各 ≥1。
- `rg -n 'ptr::eq\\(kernel_info' os/kernel/src/lib.rs` → 1 行。
- `rg -n 'modules_n|platform_n|param_n' os/libs/minix-boot/src/handoff_v1_write.rs` → 0 行。
- `rg -n 'seven payload|left to a separate cleanup|Called by .*kmain|Evidence capture|can be deleted|KernelInfo plus the real arch_boot gate' os --glob '*.rs'` → 0 行。
- 宿主：minix-boot 190/0、boot-shim 136/0、minix-kernel mock 866/0/8、xtask 顺序锁 1/1。
- QEMU：x86_64 UEFI、aarch64 UEFI、riscv64 U-Boot shim、riscv64 `bootface` 四门 PASS。

## 未纳入

- aarch64 UEFI 门后在用户链出现的 panic 属门后观察，位于本轮四项修复路径之外，记入 §5.5 的 A2 遗留读法。
