# 99-devm-global-concepts: 全局概念收口

> **状态**: pending（最小骨架，待改写）
> **定位**: 全局概念（阶段 99）
> **源码**: `minix3/minix/include/minix/com.h:846-866`、`servers/devman/devman.h`、`servers/devman/proto.h`
> **Rust 模块**: `minix-types`
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- 常量总表：DEVMAN_BASE 0x1200、10 个消息常量、5 个字段宏（m4_l1/l2/l3）、DEVMAN_STRING_LEN 128、BUF_SIZE 4097、ADD_STRING/REMOVE_STRING、状态常量（UNBOUND=0/BOUND=1/ZOMBIE=2）
- endpoint/label：devman 由 RS 运行时加载（不在 boot_image）、DS label "devman"、RS_PROC_NR 白名单
- 错误码汇总：OK/EPERM/ENODEV/ENOMEM/EINVAL（+ unbind 容错的 ENODEV=19）
- 全局状态：next_device_id、root_dev、event_inode_data/event_inode、dev_list（客户端）
- 跨服务引用：RS（bind/unbind）、VFS（VTreeFS 挂载/read）、DS（label 查询）、usbd/usb_storage/usb_hub（libdevman）、devmand（事件/属性）
- 单线程事件循环执行模型声明（与 Kernel SMP+BKL 的区别）

## 边界

- **前置依赖**: 全部
- **不覆盖（移交）**: 各机制细节（01~13）
