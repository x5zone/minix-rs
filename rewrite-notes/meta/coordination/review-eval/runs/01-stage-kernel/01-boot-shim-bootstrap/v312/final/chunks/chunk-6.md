# chunk-6 接缝证据（01-boot-shim-bootstrap.md L1501–1800）
| # | 类别 | 行号 | 证据（短引文） |
|---|---|---|---|
| 1 | 重复定义 | 1537 / 1540 / 1587 | 同一断言三写：1537「`boot_alloc_region()` 为 `None` 时直接停机」；1540「后果是 fail-closed 停机」；1587「`boot_validate_and_prepare` 直接停机」 |
| 2 | 重复定义 | 1524 / 1614 | arena 两次定义：1524「三条 v1 腿交接用的八页物理区」；1614 同句再加「解码后即失去用途」 |
| 3 | 重复定义 | 1684 | A2 一句三写：「已登记的 A2 遗留（A2 = aarch64 直核 SMP 交付后进入用户链时的 Data Abort）」紧接「A2 指 aarch64 直核在 SMP 交付之后进入用户链时的 Data Abort」 |
| 4 | 术语漂移 | 1512 / 1611 / 1497 | 1512「名字池是唯一允许截断的落地区」把名字池算作落地区；1611「落地区：内核 `.bss` 里的六个定长存储区」；1497「六个落地区、一个名字池」把二者并列 |
| 5 | 数字不自洽 | 1501–1510 / 1497、1606、1611、1629 | 表列 7 个 `*_STORE`（含 `BOOT_MODULE_NAME_STORE`），正文四处一律「六个落地区」 |
| 6 | 术语漂移（腿名） | 1542 / 1546 / 1668 | 1542「自引导与 U-Boot 腿才实际扣」；1546「（U-Boot shim 与合成测试）」；1668 五腿名单为「x86_64 UEFI、aarch64 UEFI、riscv64 U-Boot shim、riscv64 `bootface`、aarch64 `bootface_a64`」，无「自引导/合成测试」 |
| 7 | 术语漂移 | 1514 / 1537 / 1579 | 直核面阶段在四处异名且未声明关系：1514「`arch_boot_finish` 的 `store_boot_report`」、1537「两个直核面经共用的 `run_self_boot`」、1579「`arch_boot_impl`（内含 `build_bootstrap_root_and_enable` 与 `establish_boot_dm`）」、1587「`init_boot_pt_alloc` 只初始化 bump 区间」 |
| 8 | 链版本不一致 | 1579 / 1606 | 同一直核链两写：1579「`store_boot_report` → `arch_boot_impl`（…）→ `jump_to_kmain`」无 `boot_validate_and_prepare`；1606「直核面 strict → 建表前门 → postconditions → `store_boot_report` → `boot_validate_and_prepare`」 |
| 9 | 悬空指针 | 1581 | 「完整布局与覆盖策略属于独立的内存管理主题」无篇名/节号锚点 |
| 10 | 悬空指针 | 1686 | 「C 侧段归属由 `check-layout.sh` 的 L10」未给路径；同段其余证据均给 `os/…` 路径 |
| 11 | 章间过渡 | 1552 | 「本节与第 2 章的 C 行为叙述冲突时，以源码为准」——把本节与 Ch2 的冲突裁决外包给源码 |
| 12 | 章间过渡 | 1637 / 1626–1629 | 1637 依赖「换算见 §3.6 节末表」区分三套腿数；§5.0 前置概念只列第 1/2/3/4 章，未列 §3.6 |
| 13 | 证据档自相矛盾 | 1641、1643 / 1654 | 1641「`[结构保证]` `handoff_v1` 的 encode/decode round-trip」、1643「`[结构保证]` … one-shot 测试」都是运行期测试；1654 定义 `[结构保证]` 为「不依赖运行环境的结论」 |
| 14 | 证据档自相矛盾 | 1658 / 1662 | 深拷不变量在 1658 归「见 §5.1 的 I1、I3 行」即 `[结构保证]`；1662 同一「入口尽早深拷」称「按 `[预防性不变量]` 读，不是已证机制」 |
| 15 | 数字漂移（跨文档） | 1648、1703 / AGENTS.md | 本章「`-p minix-kernel --features mock --lib` 866/0/8」；项目 AGENTS.md 记同一形态「831 通过 / 0 失败 / 8 忽略」 |
| 16 | 复算命令与声明不符 | 1782 / 1761、1764、1766 | 1782 称「按文件覆盖表内全部来源」，grep 只列 `os/boot-shim/src/main.rs`、`uefi_helpers.rs`；表内 `ebs_pair.rs:219-221`、`uboot_main.rs`、`opensbi_helpers::prepare_boot` 无对应扫描 |
