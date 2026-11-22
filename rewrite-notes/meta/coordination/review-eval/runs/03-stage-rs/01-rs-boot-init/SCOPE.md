# 03-stage 01 doc 内容范围裁决（V3.12）

## 输入

- `rewrite-notes/03-stage-rs/plan.md` §2 阶段表与 §5 N01 契约；
- 五份 rerank：`doc_rerank_HY4.md`、`doc_rerank_deepseek.md`、`doc_rerank_glm.md`、
  `doc_rerank_muse.md`、`doc_rerank_qwen.md` 的 01 相关契约；
- 现有 `rewrite-notes/03-stage-rs/01-rs-boot-init.md`（1047 行，作为素材）；
- C 真源 `minix3/minix/servers/rs/main.c`、`table.c`；Rust `os/servers/rs/src/`。

## 共识

plan §2 与 qwen 的收缩方案一致：01 讲 **RS 从 `main()` 到进入主循环之前的 boot 初始化机制**。

必须覆盖：

- `main()` 的三段骨架与 `sef_local_startup()` 的回调注册（只列 RS 注册了哪 7 个回调，不讲 SEF 框架本体）；
- `sef_cb_init_fresh()` 四步 boot：Step 1 本地属性、Step 2 放行、Step 3 收齐 ready、Step 4 pid/alarm；
- `boot_image_*` 三张表与 `boot_image_info_lookup()`；
- boot 尾部的 RS 自升级只讲“存在与结果”，机制归 18；
- 每步的机制归属用一句“见 NN”标注，不展开、不堆导航段。

不覆盖：

- SEF 框架状态机与消息拦截（20）；
- 外部调用签名/消息槽（19/99）；
- 进程表字段（02）、priv（03）、send mask（05）、ready 协议（12）、
  主循环 dispatch（06）、心跳（07）、LU（16/18）。

## 分歧与取舍

| 分歧 | 方案 | 取舍 |
|---|---|---|
| HY4 把 01 改成“外部接口面” | 先集中讲所有外部调用 | **不采纳**：plan 与 qwen 都把外部面归 19；01 应是 boot 机制 |
| glm 把 01 改成“常量与标志词典” | 常量集中可查 | **不采纳**：常量归 99；01 讲启动顺序 |
| deepseek 把 01 扩成“SEF 运行时 + 进程骨架” | SEF 是入口 | **部分采纳**：保留 7 回调注册表，SEF 框架本体归 20 |
| qwen 删导航图/前向引用段、行数压到 ≤750 | 01 只讲机制 | **采纳方向**：删除 §1.2 主线图与 §1.3/1.4 的框架/分发细节；行数以讲清为准，不硬凑 750 |
| plan 把 01 定位为导航骨架 | 早期写作假设 | **修正**：导航归 00；01 不再承担“映射 02~18”的职责 |

## 最终范围

`01-rs-boot-init.md` = 5 个内容章 + 过渡：

1. 问题与 boot 顺序：RS 的自举矛盾、四步依赖、为什么先本地建表再放行。
2. C 源码分析：`main`/`sef_local_startup` 回调注册、`sef_cb_init_fresh` 四步、
   `boot_image_info_lookup`、`table.c` 三张表。
3. Rust 设计决策：BootTables、lookup 的 Result 化、SefCallbacks、BootInit 状态机、
   外部边界、自升级调用点。
4. 实现详解：main.rs/lib.rs/table.rs/boot.rs/sef.rs/dispatch.rs 的落点。
5. 测试要点：table/boot/sef 测试与基线；不把测试编号写进正文。
6. 过渡：到 02/03/12/18。

## 生成约束

- V3.12 TR-0..TR-6：每章问题入口、示例、失败对照、回顾、术语；
- V3.7 出版标准：不写过程编号、commit、DEFERRED/TODO、迭代叙事、行尾坐标注释；
- SEF 框架只留“注册表 + 一句职责”，不复制 20 篇内容。
