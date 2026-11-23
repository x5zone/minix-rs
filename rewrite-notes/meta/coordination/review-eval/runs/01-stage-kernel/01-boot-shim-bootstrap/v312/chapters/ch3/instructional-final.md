DOC: 01-boot-shim-bootstrap / ch3-slice-candidate.md（sha256 9f21c03f…，586 行，已核对）
CHAPTER: 3. Rust 设计决策
HARD-REJECTIONS: 0
VERDICT: READABLE

## 复核范围
- 只核题面指定的两处：§3.7 v1 门序补全、小结第 5 条补五腿构成与门链差异。

## 1. §3.7 v1 门序
- L551 现为 `validate_requiring_sources → check_boot_postconditions → resume_boot_pt_alloc → boot_validate_and_prepare → store_kernel_info → establish_boot_dm → jump_to_kmain → kmain strict validate + check_boot_postconditions`；`…` 已消除，收尾三步与题面一致。

## 2. 小结第 5 条
- L578 含五腿构成“两条 UEFI、U-Boot shim、两个直核面”，并给门链差异：直核面多一道建表前门，v1 面跳过，两者都在 `kmain` 做 strict 与 postconditions 复核；与 §3.7 L550–551 一致。

## 判定
- 两处修复均已落地且相互一致；先前 HARD 的根因（v1 门序缺口、小结第 5 条缺五腿/门链）已消除。HARD-REJECTIONS: 0，READABLE。
