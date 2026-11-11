# D01 R31 结果（V3.1 追加轮；NOT CONVERGED）

## 事实

- 6 个角色（FACT-A/B、READER A/B/C、DESIGN-D2）对 R30 收口后的文档做完整 survey。
- 发现 **53 条**：
  - FACT-A 8：P0 1 / P1 1 / P2 3 / P3 3
  - FACT-B 3：P1 1 / P2 2
  - READER-A 12：P2 3 / P3 9
  - READER-B 16：P2 5 / P3 11
  - READER-C 5：P2 3 / P3 2
  - DESIGN-D2 9：P1 1 / P2 5 / P3 3
- 头号 P0：§2.6.4 把 C 的自举页表说成“清活动根再回填”。实际 `pg_utils.c` 被编译两份：
  `unpaged_pg_utils.o`（低段，`pre_init` 用）与 paged `pg_utils.o`（高半，`prot_init`
  用），各自有 `static pagedir[]`；`prot_init` 清的是 paged 副本 B，活动根 A 不变。
  整段“靠旧 4MB TLB 项撑过清表窗口”的论证不存在。

## 本轮已修

- §2.6.4 改写为“两份 `pagedir` 换根”；§2.4.2/§3.5 的 unpaged 归还改为
  “`kinfo` 已拷 `.usermapped`、活跃 `pagedir`/静态池在 paged 副本”。
- §4.5 `kernel_info()` 改为活访问器（GET_KINFO/GET_IMAGE/phys_ram_view/harden/kmain）。
- §3.4.1 补 aarch64 直核腿；§3.3 补直核面 `KERN_*` 常量来源与 `check-layout` 对账。
- 两条 loader 增加 `e_entry == kern_virt_base` 交叉校验；`AlreadyMapped` 语义改为
  “仅 1 GiB 叶冲突，Step 1 新根不可达”。
- §5.2.3/§6.4 同步 U-Boot 脚本的宿主 SKIP 语义；§1.1 补 `fail-closed`/`conventional`
  等术语；§1.2 D4 箭头与主链对应改写；§3.4.1/§4.3/§5.1 的 A–D 防线标签统一。

## 未关闭

- Reader A/B/C 的其余 P2/P3（术语重复、指针偏移、计数枚举、教学图细节）没有全部修完；
  没有建 R31 的逐条 `defect-ledger` 行，也没有 100% 复验。
- 因此 R31 **不判 CONVERGED**；R30 的 `CONVERGED` 被本轮推翻。

## 结论

V3.1 在同一篇文档上跑 R28/R29/R30 三轮后，R30 收口；用户要求追加的 R31 仍找到
53 条（含 1 条 P0、3 条 P1）。这说明当前面板的 P2/P3 长尾不会因为再跑一轮而消失；
继续同一 edition 只会重复“新建发现—线级修复”。若还要继续，应开 V3.2 并把读者
P3 措辞/指针降为 suggestions，只对 P0/P1/P2 做阻断；否则应接受“P0/P1 已修、已知项
有登记”的冻结口径，不再追求零 P3。
