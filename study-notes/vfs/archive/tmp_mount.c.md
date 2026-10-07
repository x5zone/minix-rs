# servers/vfs/mount.c 逐行讲解

> **文件路径**: `minix3/minix/servers/vfs/mount.c`
> **核心功能**: mount/umount 系统调用实现

---

## 文件概述

这个文件实现了 mount 和 umount 系统调用。

**核心概念**: 文件系统挂载，设备号管理，挂载点。

---

## 逐行讲解

### 全局变量

```c
static int have_root = 0;

static bitchunk_t nonedev[BITMAP_CHUNKS(NR_NONEDEVS)] = { 0 };

#define alloc_nonedev(dev) SET_BIT(nonedev, minor(dev) - 1)
#define free_nonedev(dev) UNSET_BIT(nonedev, minor(dev) - 1)
```

**讲解**:
- **have_root**: 是否已有根文件系统
- **nonedev**: "none" 伪设备位图
- 用于绑定挂载

---

### 设备号查找

```c
static dev_t name_to_dev(int allow_mountpt, char path[PATH_MAX]);
static dev_t find_free_nonedev(void);
```

**讲解**:
- **name_to_dev**: 路径名转设备号
- **find_free_nonedev**: 查找空闲伪设备

---

## 要点总结

1. **mount**: 挂载文件系统
2. **设备号**: 标识文件系统
3. **绑定挂载**: 使用伪设备

---

## 互动自测

1. **问题**: 什么是绑定挂载？
   **答案**: 将一个目录挂载到另一个目录，共享文件系统。
