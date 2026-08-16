# 00-input-overview: INPUT 整体架构概览

> **状态**: pending（最小骨架，待改写）
> **定位**: 全文档导航（阶段 0 总览）
> **源码**: `minix3/minix/servers/input/`（1 个 .c + 1 个 .h，759 行）+ `minix3/minix/lib/libchardriver/`（600 行，input 使用面）+ `minix3/minix/lib/libinputdriver/`（206 行）
> **Rust 模块**: `os/servers/input/` 全部
> **draft 素材**: `draft/README.md`（占位）+ `../../../../tmp/input/`（逐行笔记）

## 核心点

- input 是什么：键盘/鼠标输入事件服务器，运行在 chardriver 框架之上（`chardriver_task` 主循环），统一汇聚输入驱动事件，向打开的设备/mux 提供输入流，或转发 TTY（plan §1.1）
- 启动主线图：RS 运行时加载（不在 boot_image，`system.conf:400-403`）→ `main()` → `input_startup`（SEF）→ `chardriver_task` → 主循环分发（plan §1.2）
- 事件旅程次主线：pckbd 中断 → `inputdriver_send_event` → INPUT_EVENT → `input_event` → `input_process`（缓冲/唤醒/转发 TTY）（plan §1.3）
- 驱动生命周期次主线：announce → DS `drv.inp.` 注册 → connect（CONF 分配 id）→ events → disconnect（plan §1.3）
- 文档导航：16 篇（00 + 01~14 + 99），7 阶段，新编号交叉引用规则（plan §3.3）
- 设计原则：位置可回答性 / 禁止前向引用 / 每篇一个语义单元 / ARCH 三处一致标注
- ARCH 焦点：chardriver 框架依赖（A-1）、事件 id==数组下标（A-3）、环形缓冲溢出（A-4）、挂起读状态机（A-5）、全单向协议（A-9）

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制细节（见 01~14、99）
