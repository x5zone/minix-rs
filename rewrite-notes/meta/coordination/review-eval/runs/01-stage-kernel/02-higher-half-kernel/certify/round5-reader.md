# D02 R5 certify — reader 回读

> 独立冷启动实例只读回读；本文件由主 agent 从回执转录。对象为当前工作树 `02-higher-half-kernel.md`。

| ID | 判定 | 位置 | 说明 |
|---|---|---|---|
| READER-A-01 | CLOSED | §1.1/§1.2/§2.1 | J1 组内一致；L4 只对 Rust 生产镜像 |
| READER-A-02 | CLOSED | §3.7 | ①限定非 16 对齐 stack_top；对齐时去三条仍合法 |
| READER-B-01 | CLOSED | §1.2/§2.1 | J1 分组；J2 限定 Rust 生产镜像 |
| READER-B-02 | CLOSED（复读） | §5 | J3 行改为 §4.3 Step 次序 + §5.3 到达 kmain + 无独立断言 |
| READER-B-03 | CLOSED | §1.3/§3.1/§5.2 | L0–L10 权威清单只在 §5.2 |
| READER-B-04 | CLOSED | §1.3/§4.5 | 接缝与正文分工一致 |
| READER-B-05 | CLOSED | §3.6/§3.8 | 重复标题已删 |
| READER-B-06 | CLOSED | §4.5 | 删重复形态表，只留路径图 |
| READER-B-07 | CLOSED | §3.1 | 三形态枚举；两种 AT 写法降为语法差异 |
| READER-B-08 | CLOSED（复读） | §5.2 | L5 阶梯指针改 §3.5 |
| READER-B-09 | CLOSED | §3.6 | 删“C 侧推演过”错指 |
| READER-B-10 | CLOSED | §2.1/§4.1 | 孤儿段 LMA 继承例外 |
| READER-B-11 | CLOSED | §2.4 | USER 位理由改指本篇 §2.4 |
| READER-B-12 | CLOSED | §4.4 | 引文头注明两条对齐断言省略 |
| READER-B-13 | CLOSED | §5.4 | M3/M9 覆盖措辞已诚实降级 |
| READER-B-14 | CLOSED | §6.1 | LMA 重叠限定旧工具链 |
| READER-B-15 | CLOSED（复读） | §2.3 | “进入行的前提”表述与 §1.1 一致 |
| READER-B-16 | CLOSED | §2.3/§2.5 | 物理页回收后静默损坏，不再说页错误 |
| READER-C-01 | CLOSED | §1.1/§1.2 | J1 组内一致 |
| READER-C-02 | CLOSED | §3.6/§4.5 | EBS 后建根，与 §4.5 一致 |
| READER-C-03 | CLOSED | §3.6/§4.3/§4.1 | 守卫拒绝与反事实分离；Step 1 已按实现改写 |
| READER-C-04 | CLOSED | §1.2 | J2 限定 Rust 生产镜像 |
| READER-C-05 | CLOSED | §4.5.1 | “部分 QEMU 直核测试内核”限定 |
| READER-C-06 | CLOSED | §1.2 | J2 范围 |
| READER-C-07 | CLOSED | §1.1/J4 | SP 为主判据，PC 按架构 |
| READER-C-08 | CLOSED | §4.5 | 无映射页错误 vs 历史零读 |

## 回执结论

- 初读 23 CLOSED / 3 STILL-BROKEN（B-02、B-08、B-15）；修复后复读 3/3 CLOSED。
- 最终：26/26 CLOSED，0 STILL-BROKEN。
- 未覆盖：未读 `os/`/`minix3/` 源码，未运行 cargo/QEMU/`check-layout.sh`，只判文档内部闭合。


结论: PASS（26/26 CLOSED，0 STILL-BROKEN）。
