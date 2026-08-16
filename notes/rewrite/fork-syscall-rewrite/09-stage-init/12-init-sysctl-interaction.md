# 12-init-sysctl-interaction: securelevel 与 CHROOT（sysctl 交互）

> **状态**: pending（最小骨架，待改写）
> **定位**: init ↔ 内核 sysctl 的两类交互：KERN_SECURELVL 与 init.root 动态节点
> **源码**: `minix3/sbin/init/init.c`：`has_securelevel`（544-566）、`getsecuritylevel`（568-592）、`setsecuritylevel`（594-621）、`createsysctlnode`（1811-1857）、`shouldchroot`（1859-1902）
> **Rust 模块**: 无
> **draft 素材**: 无

## 核心点

- securelevel：`has_securelevel`（KERN_SECURELVL 探测）、`getsecuritylevel`、`setsecuritylevel`（单用户降 0 / 多用户升 1）
- **ARCH A-4**：minix-rs kernel 无 securelevel 语义 → defer + 契约标注
- CHROOT：`createsysctlnode`（CTL_CREATE 动态建 `init.root` 字符串节点，CTLFLAG_PRIVATE）、`shouldchroot`（读取并验证字符串，`rootdir` 非 "/" 才 chroot）
- **ARCH A-5**：minix-rs 无 sysctl 服务 → defer（或编译期常量简化）
- 调用点：01（has_securelevel 探测）、04（单用户降级）、05（shouldchroot）、09（多用户升级）

## 边界

- **前置依赖**: 01（探测调用点）/05（chroot 调用点）
- **不覆盖（移交）**: 内核 sysctl 实现（`../01-stage-kernel/` 对应文档）
