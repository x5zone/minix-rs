# D01 R37 certify — reader 回读

> 独立冷启动实例只读回读；本文件由主 agent 从回执转录。对象为当前工作树 `01-boot-shim-bootstrap.md`。

| ID | 判定 | 位置 | 说明 |
|---|---|---|---|
| READER-A-01 | CLOSED | §1.2/§3.7/§4.4 | I4 改为集合式陈述，逐腿门序指向 §4.4 |
| READER-A-02 | CLOSED | §3.7/§4.4 | 门内容与 §4.4 对齐；postconditions 只剩 4 GiB 门 |
| READER-A-03 | CLOSED | §1.1/§2.4/§3.2.1 | 框 1 定义与“未扣除”表述一致 |
| READER-A-04 | CLOSED | §1.1/§4.4 | `entry_point` 不是字段 |
| READER-A-05 | CLOSED | §2.8/§1.4 | D1–D5 定义在 §1.4 |
| READER-A-06 | CLOSED | §3.2.1/§3.2.2 | 22 字段账一致 |
| READER-A-07 | CLOSED | §5.3/§5.4 | §5.4 表存在 |
| READER-B-01 | CLOSED | §1.2/§3.7/§4.4 | I4 门序 |
| READER-B-02 | CLOSED | §5.3/§5.4/§5.5 | 门链表与三项关闭记录 |
| READER-B-03 | CLOSED（复读） | §3.4.2 | 开头改指 §3.6 |
| READER-B-04 | CLOSED | §1.1/§2.3/§2.4/§2.8 | 框 1/框 2 定义一致 |
| READER-B-05 | CLOSED | §3.2.1/§3.2.2 | 22 字段 |
| READER-B-06 | CLOSED | §3.7/§1.2 | 所有权在 §3.7 开头；I3 行有指针 |
| READER-B-07 | CLOSED | §4.7/§7.5 | 优先序统一 |
| READER-B-08 | CLOSED | §1.1/§1.4/§2.2 | “第一次/第二次跳转” |
| READER-B-09 | CLOSED | §1.4/§2.4.2 | 越界行存在 |
| READER-B-10 | CLOSED（复读） | §1.2 | 白话术语段；`BootStorage` 归发送侧 |
| READER-C-01 | CLOSED | §1.2/§3.7/§4.4 | I4 时序 |
| READER-C-02 | CLOSED | §5.3/§5.4/§5.5 | 表与关闭记录 |
| READER-C-03 | CLOSED | §1.1/§3.2.1/§4.4 | `entry_point`/`BootReport` 角色 |
| READER-C-04 | CLOSED | §1.1/§2.4/§2.5/§2.8 | 框 1 链 |
| READER-C-05 | CLOSED | §1.2/§1.3/§3.3 | 跨镜像与直核的来源记录 |
| READER-C-06 | CLOSED | §2.6 | FileLoader 与直核面分工 |
| READER-C-07 | CLOSED（复读） | §2.4.1/§2.2/§2.8 | 三处空锚点已回填；最后一项指 02 篇 §2.5 |
| READER-C-08 | CLOSED | §3.2.1/§3.4.1/§5.5/§6.1 | 占位串已清 |
| READER-C-09 | CLOSED | §5.1/§5.2/§5.4/§6.4 | `test-sysboot` 不是启动门 |
| READER-C-10 | CLOSED | §3.2.1/§3.2.2 | 22 字段 |
| READER-C-11 | CLOSED | §2.6 | `overlaps` 半开判据与 `-1` 差异 |

## 回执结论

- 初读 25 CLOSED / 3 STILL-BROKEN（B-03、B-10、C-07）；逐条修复后复读 3/3 CLOSED。
- 最终：28/28 CLOSED，0 STILL-BROKEN。
- 未覆盖：未做全量源码事实复核、未跑门禁、未审 SUGGESTION 项。


结论: PASS（28/28 CLOSED，0 STILL-BROKEN）。
