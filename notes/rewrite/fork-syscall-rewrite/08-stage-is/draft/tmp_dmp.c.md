# IS Server - dmp.c 逐行讲解

## 文件概述

**文件路径**: `minix/servers/is/dmp.c`

**核心功能**: 实现功能键映射管理和功能键按下事件处理。定义了功能键到转储函数的映射表。

**设计思路**: 使用表驱动设计，将功能键与转储函数关联。当功能键按下时，查找映射表并调用对应的转储函数。这种设计易于扩展，添加新的转储功能只需在表中添加一行。

---

## 头文件包含

```c
/* This file contains information dump procedures. During the initialization
 * of the Information Service 'known' function keys are registered at the TTY
 * server in order to receive a notification if one is pressed. Here, the
 * corresponding dump procedure is called.
 *
 * The entry points into this file are
 *   map_unmap_fkeys:	register or unregister function key maps with TTY
 *   do_fkey_pressed:	handle a function key pressed notification
 */

#include "inc.h"
#include <minix/vm.h>
```

**逐行讲解**:
- 注释说明文件职责和入口点
- `#include "inc.h"`：主头文件
- `#include <minix/vm.h>`：VM相关定义

---

## 功能键映射表

```c
struct hook_entry {
	int key;
	void (*function)(void);
	char *name;
} hooks[] = {
	{ F1, 	proctab_dmp, "Kernel process table" },
	{ F3,	image_dmp, "System image" },
	{ F4,	privileges_dmp, "Process privileges" },
	{ F5,	monparams_dmp, "Boot monitor parameters" },
	{ F6,	irqtab_dmp, "IRQ hooks and policies" },
	{ F7,	kmessages_dmp, "Kernel messages" },
	{ F8,	vm_dmp, "VM status and process maps" },
	{ F10,	kenv_dmp, "Kernel parameters" },
	{ SF1,	mproc_dmp, "Process manager process table" },
	{ SF2,	sigaction_dmp, "Signals" },
	{ SF3,	fproc_dmp, "Filesystem process table" },
	{ SF4,	dtab_dmp, "Device/Driver mapping" },
	{ SF5,	mapping_dmp, "Print key mappings" },
	{ SF6,	rproc_dmp, "Reincarnation server process table" },
	{ SF8,  data_store_dmp, "Data store contents" },
	{ SF9,  procstack_dmp, "Processes with stack traces" },
};
```

**逐行讲解**:
- `struct hook_entry`：映射条目结构
  - `key`：功能键码（F1-F12或SF1-SF12）
  - `function`：转储函数指针
  - `name`：转储描述字符串
- `hooks[]`：映射表数组
  - 静态存储，程序启动时初始化
  - 每个条目关联一个功能键和一个转储函数

**功能键定义**:
```
F1-F12:   普通功能键
SF1-SF12: Shift + 功能键

映射:
F1  → 内核进程表
F3  → 系统映像
F4  → 进程权限
F5  → 启动监视器参数
F6  → IRQ钩子和策略
F7  → 内核消息
F8  → VM状态
F10 → 内核参数
SF1 → PM进程表
SF2 → 信号处理
SF3 → VFS进程表
SF4 → 设备/驱动映射
SF5 → 功能键映射列表
SF6 → RS进程表
SF8 → DS数据存储
SF9 → 进程栈跟踪
```

---

## 映射表大小

```c
/* Define hooks for the debugging dumps. This table maps function keys
 * onto a specific dump and provides a description for it.
 */
#define NHOOKS (sizeof(hooks)/sizeof(hooks[0]))
```

**逐行讲解**:
- `NHOOKS`：映射表条目数量
  - `sizeof(hooks)`：整个数组的大小
  - `sizeof(hooks[0])`：单个元素的大小
  - 相除得到元素个数

**编译时计算**:
```
sizeof(hooks) = 16 * sizeof(struct hook_entry)
sizeof(hooks[0]) = sizeof(int) + sizeof(void*) + sizeof(char*)
                 = 4 + 4 + 4 = 12 (32位系统)
NHOOKS = 16
```

---

## map_unmap_fkeys函数

```c
/*===========================================================================*
 *				map_unmap_keys				     *
 *===========================================================================*/
void
map_unmap_fkeys(int map)
{
  int fkeys, sfkeys;
  int h, s;

  fkeys = sfkeys = 0;

  for (h = 0; h < NHOOKS; h++) {
      if (hooks[h].key >= F1 && hooks[h].key <= F12)
          bit_set(fkeys, hooks[h].key - F1 + 1);
      else if (hooks[h].key >= SF1 && hooks[h].key <= SF12)
          bit_set(sfkeys, hooks[h].key - SF1 + 1);
  }

  if (map) s = fkey_map(&fkeys, &sfkeys);
  else s = fkey_unmap(&fkeys, &sfkeys);

  if (s != OK)
	printf("IS: warning, fkey_ctl failed: %d\n", s);
}
```

**逐行讲解**:
- `void map_unmap_fkeys(int map)`：注册或取消功能键映射
  - 参数`map`：TRUE表示注册，FALSE表示取消
- `int fkeys, sfkeys;`：位图变量
  - `fkeys`：普通功能键位图（F1-F12）
  - `sfkeys`：Shift+功能键位图（SF1-SF12）
- 遍历映射表，设置对应的位
  - `bit_set(fkeys, hooks[h].key - F1 + 1)`：设置位
    - F1对应位1，F2对应位2，依此类推
- `fkey_map(&fkeys, &sfkeys)`：向TTY注册功能键
- `fkey_unmap(&fkeys, &sfkeys)`：取消注册

**位图示意**:
```
fkeys位图 (32位整数):
┌───┬───┬───┬───┬───┬───┬───┬───┬───┬───┬───┬───┐
│ 0 │ 1 │ 1 │ 1 │ 1 │ 1 │ 1 │ 0 │ 1 │ 0 │ 0 │ 0 │ ...
└───┴───┴───┴───┴───┴───┴───┴───┴───┴───┴───┴───┘
  F0  F1  F2  F3  F4  F5  F6  F7  F8  F9 F10 F11

位1(F1)、位3(F3)、位4(F4)...被设置
表示IS关心这些键
```

---

## do_fkey_pressed函数

```c
/*===========================================================================*
 *				handle_fkey				     *
 *===========================================================================*/
#define pressed(start, end, bitfield, key) \
	(((start) <= (key)) && ((end) >= (key)) && \
	 bit_isset((bitfield), ((key) - (start) + 1)))
int do_fkey_pressed(m)
message *m;					/* notification message */
{
  int s, h;
  int fkeys, sfkeys;

  /* The notification message does not convey any information, other
   * than that some function keys have been pressed. Ask TTY for details.
   */
  s = fkey_events(&fkeys, &sfkeys);
  if (s < 0) {
      printf("IS: warning, fkey_events failed: %d\n", s);
  }
```

**逐行讲解**:
- `#define pressed(start, end, bitfield, key)`：宏，检查键是否被按下
  - 参数：起始键、结束键、位图、目标键
  - 返回：非零表示按下
- `int do_fkey_pressed(message *m)`：处理功能键按下事件
  - 参数：通知消息（内容不重要）
- `fkey_events(&fkeys, &sfkeys)`：从TTY获取按下的键
  - 通知消息本身不包含具体信息
  - 需要主动查询TTY

**通知与查询**:
```
TTY检测到功能键按下
    ↓
发送通知给IS（消息内容为空）
    ↓
IS收到通知
    ↓
调用fkey_events查询具体哪些键被按下
    ↓
TTY返回位图
```

---

```c
  /* Now check which keys were pressed: F1-F12, SF1-SF12. */
  for(h=0; h < NHOOKS; h++) {
	if (pressed(F1, F12, fkeys, hooks[h].key)) {
		hooks[h].function();
	} else if (pressed(SF1, SF12, sfkeys, hooks[h].key)) {
		hooks[h].function();
	}
  }

  /* Don't send a reply message. */
  return(EDONTREPLY);
}
```

**逐行讲解**:
- 遍历映射表，检查每个键是否被按下
- 如果按下，调用对应的转储函数
- `hooks[h].function()`：通过函数指针调用转储函数
- 返回`EDONTREPLY`：不发送回复

**函数指针调用**:
```
hooks[h].function 等价于:
- proctab_dmp (如果key=F1)
- mproc_dmp   (如果key=SF1)
- ...
```

---

## key_name函数

```c
/*===========================================================================*
 *				key_name				     *
 *===========================================================================*/
static char *key_name(int key)
{
	static char name[15];

	if(key >= F1 && key <= F12)
		snprintf(name, sizeof(name), " F%d", key - F1 + 1);
	else if(key >= SF1 && key <= SF12)
		snprintf(name, sizeof(name), "Shift+F%d", key - SF1 + 1);
	else
		strlcpy(name, "?", sizeof(name));
	return name;
}
```

**逐行讲解**:
- `static char *key_name(int key)`：获取键名
  - 参数：键码
  - 返回：键名字符串
- `static char name[15]`：静态缓冲区
  - 存储位置：静态数据段
  - 注意：不是线程安全的，但IS是单线程
- `snprintf`：格式化字符串
  - 防止缓冲区溢出

---

## mapping_dmp函数

```c
/*===========================================================================*
 *				mapping_dmp				     *
 *===========================================================================*/
void mapping_dmp(void)
{
  int h;

  printf("Function key mappings for debug dumps in IS server.\n");
  printf("        Key   Description\n");
  printf("-------------------------------------");
  printf("------------------------------------\n");

  for(h=0; h < NHOOKS; h++)
      printf(" %10s.  %s\n", key_name(hooks[h].key), hooks[h].name);
  printf("\n");
}
```

**逐行讲解**:
- `void mapping_dmp(void)`：显示功能键映射列表
  - 按Shift+F5触发
- 遍历映射表，打印每个键的名称和描述

**输出示例**:
```
Function key mappings for debug dumps in IS server.
        Key   Description
--------------------------------------------------------------------------------
          F1.  Kernel process table
          F3.  System image
          F4.  Process privileges
          F5.  Boot monitor parameters
          F6.  IRQ hooks and policies
          F7.  Kernel messages
          F8.  VM status and process maps
         F10.  Kernel parameters
     Shift+F1.  Process manager process table
     Shift+F2.  Signals
     Shift+F3.  Filesystem process table
     Shift+F4.  Device/Driver mapping
     Shift+F5.  Print key mappings
     Shift+F6.  Reincarnation server process table
     Shift+F8.  Data store contents
     Shift+F9.  Processes with stack traces
```

---

## 要点总结

1. **表驱动设计**: 使用映射表关联功能键和转储函数，易于扩展和维护。

2. **位图管理**: 使用位图表示关心的功能键集合，高效且紧凑。

3. **通知-查询模式**: TTY发送通知，IS主动查询详情，减少消息大小。

---

## 灾难预演

**如果删除`pressed`宏中的边界检查会怎样？**

如果键码超出范围，`bit_isset`可能访问位图的非法位，导致：
1. 读取到错误的值
2. 可能访问越界内存
3. 转储函数可能被错误调用或不调用

**如果`hooks`数组为空会怎样？**

`NHOOKS`为0，循环不执行。所有功能键都不会触发转储。用户按下功能键没有任何响应。

---

## 互动自测

1. **内存模型**: `hooks`数组存储在哪个内存段？每个元素的大小是多少字节？

2. **设计选择**: 为什么使用函数指针数组而不是switch-case语句？这种设计有什么优势？

3. **位图操作**: 如果同时按下F1和F3，`fkeys`变量的值是多少？假设F1=1，F3=3。

---

## Rust实现对比

```rust
#![no_std]

use core::mem;
use alloc::string::String;
use alloc::format;
use minix_rs::ipc::Message;
use minix_rs::sys::{OK, EDONTREPLY};

extern crate alloc;

type DumpFn = fn();

struct HookEntry {
    key: i32,
    function: DumpFn,
    name: &'static str,
}

static HOOKS: &[HookEntry] = &[
    HookEntry { key: F1,  function: proctab_dmp, name: "Kernel process table" },
    HookEntry { key: F3,  function: image_dmp, name: "System image" },
    HookEntry { key: F4,  function: privileges_dmp, name: "Process privileges" },
    HookEntry { key: F5,  function: monparams_dmp, name: "Boot monitor parameters" },
    HookEntry { key: F6,  function: irqtab_dmp, name: "IRQ hooks and policies" },
    HookEntry { key: F7,  function: kmessages_dmp, name: "Kernel messages" },
    HookEntry { key: F8,  function: vm_dmp, name: "VM status and process maps" },
    HookEntry { key: F10, function: kenv_dmp, name: "Kernel parameters" },
    HookEntry { key: SF1, function: mproc_dmp, name: "Process manager process table" },
    HookEntry { key: SF2, function: sigaction_dmp, name: "Signals" },
    HookEntry { key: SF3, function: fproc_dmp, name: "Filesystem process table" },
    HookEntry { key: SF4, function: dtab_dmp, name: "Device/Driver mapping" },
    HookEntry { key: SF5, function: mapping_dmp, name: "Print key mappings" },
    HookEntry { key: SF6, function: rproc_dmp, name: "Reincarnation server process table" },
    HookEntry { key: SF8, function: data_store_dmp, name: "Data store contents" },
    HookEntry { key: SF9, function: procstack_dmp, name: "Processes with stack traces" },
];

pub fn map_unmap_fkeys(map: bool) {
    let mut fkeys: u32 = 0;
    let mut sfkeys: u32 = 0;

    for hook in HOOKS {
        if hook.key >= F1 && hook.key <= F12 {
            fkeys |= 1 << (hook.key - F1);
        } else if hook.key >= SF1 && hook.key <= SF12 {
            sfkeys |= 1 << (hook.key - SF1);
        }
    }

    let result = if map {
        fkey_map(&fkeys, &sfkeys)
    } else {
        fkey_unmap(&fkeys, &sfkeys)
    };

    if result != OK {
        log::warn!("IS: fkey_ctl failed: {}", result);
    }
}

pub fn do_fkey_pressed(_m: &Message) -> i32 {
    let (fkeys, sfkeys) = match fkey_events() {
        Ok(keys) => keys,
        Err(e) => {
            log::warn!("IS: fkey_events failed: {}", e);
            return EDONTREPLY;
        }
    };

    for hook in HOOKS {
        if pressed(F1, F12, fkeys, hook.key) || pressed(SF1, SF12, sfkeys, hook.key) {
            (hook.function)();
        }
    }

    EDONTREPLY
}

fn pressed(start: i32, end: i32, bitfield: u32, key: i32) -> bool {
    key >= start && key <= end && (bitfield & (1 << (key - start))) != 0
}

fn key_name(key: i32) -> String {
    if key >= F1 && key <= F12 {
        format!(" F{}", key - F1 + 1)
    } else if key >= SF1 && key <= SF12 {
        format!("Shift+F{}", key - SF1 + 1)
    } else {
        String::from("?")
    }
}

pub fn mapping_dmp() {
    println!("Function key mappings for debug dumps in IS server.");
    println!("        Key   Description");
    println!("-------------------------------------");
    
    for hook in HOOKS {
        println!(" {:>10}.  {}", key_name(hook.key), hook.name);
    }
}

extern "C" {
    fn proctab_dmp();
    fn image_dmp();
    fn privileges_dmp();
    fn monparams_dmp();
    fn irqtab_dmp();
    fn kmessages_dmp();
    fn vm_dmp();
    fn kenv_dmp();
    fn mproc_dmp();
    fn sigaction_dmp();
    fn fproc_dmp();
    fn dtab_dmp();
    fn rproc_dmp();
    fn data_store_dmp();
    fn procstack_dmp();
}

const F1: i32 = 1;
const F12: i32 = 12;
const SF1: i32 = 13;
const SF12: i32 = 24;
```

### Rust改进点

1. **静态切片**: 使用`&[HookEntry]`静态切片，编译时确定大小，无需`sizeof`计算。

2. **类型安全**: `DumpFn`是函数指针类型别名，确保只有匹配签名的函数可以赋值。

3. **位运算**: 使用Rust的位运算符，更清晰直观。

4. **String类型**: `key_name`返回`String`，避免静态缓冲区的线程安全问题。

### unsafe说明

- 转储函数声明为`extern "C"`，调用时需要`unsafe`块
- 这些函数可能访问内核内存，需要特殊处理
- 未来改进：将转储函数也用Rust重写，消除unsafe
