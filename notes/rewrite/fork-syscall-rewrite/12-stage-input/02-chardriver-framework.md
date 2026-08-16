# 02-chardriver-framework: chardriver 框架契约

> **状态**: pending（最小骨架，待改写）
> **定位**: `chardriver_task` 主循环与 CDEV 分发（阶段 1 运行框架）
> **源码**: `minix3/minix/lib/libchardriver/chardriver.c`（600 行，按 input 使用面裁剪）+ `minix3/minix/include/minix/chardriver.h`
> **Rust 模块**: 等价框架（A-1，决策见 plan §7.3）
> **draft 素材**: 无（框架无历史笔记）

## 核心点

- `chardriver_task`（:549-573）：`sef_receive_status(ANY)` 主循环
- `chardriver_process`（:455-536）：notify（HARDWARE/CLOCK → cdr_intr/cdr_alarm、其他 → cdr_other）、BDEV_OPEN → `do_block_open`（ENXIO）、CDEV RQ → minor 提取 + open_devs 门卫、消息分派或 cdr_other
- CDEV 消息协议 m10：CDEV_OPEN/CLOSE/READ/WRITE/IOCTL/CANCEL/SELECT 字段布局 + CDEV_REPLY/SEL1_REPLY/SEL2_REPLY 回复（`chardriver_reply` EDONTREPLY/ERESTART 过滤）
- `chardriver_announce`（:99-127）：`sys_statectl(SYS_STATE_CLEAR_IPC_REFS)` + `drv.chr.<label>` 发布 + 清 open_devs
- `chardriver_reply_task`（:129-151）/`chardriver_reply_select`（:153-174）：异步唤醒通道
- `chardriver_get_minor`（:575-598）、open_devs 门卫（is_open_dev/set_open_dev/clear_open_devs :61-97）
- ARCH A-1：os/ 无 chardriver 等价物 → 框架决策（共享 crate vs input 内部最小实现）

## 边界

- **前置依赖**: 01
- **不覆盖（移交）**: input 业务结构（03）、各 handler 实现（06~10）、chardriver 非 input 使用面（驱动阶段）
