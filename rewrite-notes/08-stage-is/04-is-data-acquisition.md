# 04-is-data-acquisition：数据获取五通道

> **源码**：`minix3/minix/include/minix/com.h:KERNEL_CALL`（KERNEL_CALL）、`:236`
> （SYS_GETINFO）、`:252`（SYS_DIAGCTL）、`:315-345`（GET_*）、`:412-415`
> （DIAGCTL_CODE_*）、`:476`（RS_GETSYSINFO）、`:507`（DS_GETSYSINFO）、
> `:729-734`（VM_INFO/VMIW_*）+ `minix3/minix/include/minix/sysinfo.h:SI_PROC_TAB`（SI_*）+ `minix3/minix/include/minix/callnr.h:PM_GETSYSINFO`
> （PM_GETSYSINFO）/`:120`（VFS_GETSYSINFO）+ `type.h:214-232`
> （minix_kerninfo）+ `syslib.h:164-187`（diagctl/getinfo 速记宏）+
> `libsys/{getsysinfo.c,vm_info.c,sys_diagctl.c,sys_getinfo.c}` +
> `libc/sys/init.c:1-32` + `kernel/usermapped_data.c:4-15` +
> `servers/pm/misc.c:105-145` + `servers/vfs/misc.c:52-113` +
> IS 使用点（见 §2 各节末"IS 用法"行）
> **Rust**：`os/libs/minix-types/src/ipc/sysinfo.rs`（新）+
> `os/servers/is/src/acquire.rs`（新）
> **draft 素材**：无（机制面无旧素材，全篇新建）
> **位置**：05~10 六个转储域篇的共同前置（plan §2.1）；回答"数据从哪来"

---

## 1. 概念：为什么五条通道而非一条

### 1.1 问题：数据主权是分散的

> **目标读者**：读完 01~03 的读者。前置知识：01 的错误面分界（transport
> panic vs 业务 warn）、02 的 `_taskcall` 同步语义。本章不讲布局解释
> （05~10）与各服务实现细节。

IS 自己不存任何状态数据——它是个空手来的聚合器。数据散在五个主权者手里，
每家有自己的交接规矩，统一不成一条通道：

| 主权者 | 数据 | 通道 | 交接规矩 |
|---|---|---|---|
| kernel（SYSTEM 任务） | 进程表/特权表/image 等 6 表 | `sys_getinfo` | 内核调用，存到调用方 |
| kernel（诊断面） | 进程栈回溯 | `sys_diagctl` | 内核调用，码复用参数 |
| kernel（映射页） | 内核消息环形缓冲 | kerninfo 直读 | 启动时映射，之后直读 |
| PM/VFS/RS/DS 各服务 | 私有进程表/存储 | `getsysinfo` | 跨服务 IPC + 精确尺寸 + root 门 |
| VM 服务 | 地址空间快照 | `vm_info` | 三子命令 + 游标分页 |

### 1.2 类比：档案馆调卷

把 IS 想象成写调查报告的记者。① 本馆复印（`sys_getinfo`）：档案馆
（kernel）有复印机，记者填单子，复印件交到记者手里；② 内部传阅
（`sys_diagctl`）：请馆长在某份卷宗上批注，批完还放在馆里；③ 阅览室直读
（kerninfo）：开馆时给记者发了阅览证，进门直接看，不用填单；④ 他馆发函
（`getsysinfo`）：外地档案馆，函上必须写清要几页（尺寸精确），非馆长级
介绍信不受理；⑤ 卷宗太厚分批送（`vm_info`）：一次搬不完，馆员给个书签
（游标），下次从书签处继续。

### 1.3 边界声明

**前置依赖**：01（panic/warn 分界）；kernel `25-misc-unported`（GETINFO
机制本体）、`28-usermapped-data`（usermapped 段语义）、`32-stack-tracing`
（DIAGCTL Rust 侧 ENOSYS 现状，C 侧已实现）。

**本篇职责**：五通道的消息格式 + 拷贝语义 + 错误面 + IS 使用枚举。

**不覆盖（移交）**：各表字段语义 → 05~10；VM_INFO 服务端实现 →
`02-stage-vm/26-vm-queries.md`；各服务 `do_getsysinfo` 实现细节 → 各服务
stage；kernel `do_getinfo`/`do_diagctl` 实现 → kernel stage（本篇只客户端面）。

> **本章小结**：五主权五通道（§1.1 表）。下一章 §2 把每条通道的消息格式、
> 拷贝语义、错误面逐项对应到 C 行号。

---

## 2. C 源码分析

### 2.1 sys_getinfo：本馆复印（kernel 表）

调用号与请求表：

```c
#define KERNEL_CALL	0x600	/* base for kernel calls to SYSTEM */  /* com.h:205 */
#  define SYS_GETINFO    (KERNEL_CALL + 26) 	/* sys_getinfo() */  /* com.h:236 */
```

```c
/* Field names for SYS_GETINFO. */                        /* com.h:315-345 */
#   define GET_KINFO	   0	/* get kernel information structure */
#   define GET_IMAGE	   1	/* get system image table */
#   define GET_PROCTAB	   2	/* get kernel process table */
#   define GET_RANDOMNESS  3	/* get randomness buffer */
#   define GET_MONPARAMS   4	/* get monitor parameters */
#   define GET_KENV	   5	/* get kernel environment string */
#   define GET_IRQHOOKS	   6	/* get the IRQ table */
				/* 7: unassigned（无定义，非遗漏） */
#   define GET_PRIVTAB	   8	/* get kernel privileges table */
#   define GET_KADDRESSES  9	/* get various kernel addresses */
#   define GET_SCHEDINFO  10	/* get scheduling queues */
#   define GET_PROC 	  11	/* get process slot if given process */
#   define GET_MACHINE 	  12	/* get machine information */
#   define GET_LOCKTIMING 13	/* get lock()/unlock() latency timing */
#   define GET_BIOSBUFFER 14	/* get a buffer for BIOS calls */
#   define GET_LOADINFO   15	/* get load average information */
#   define GET_IRQACTIDS  16	/* get the IRQ masks */
#   define GET_PRIV	  17	/* get privilege structure */
#   define GET_HZ	  18	/* get HZ value */
#   define GET_WHOAMI	  19	/* get own name, endpoint, and privileges */
#   define GET_RANDOMNESS_BIN 20 /* get one randomness bin */
#   define GET_IDLETSC	  21	/* get cumulative idle time stamp counter */
				/* 22: unassigned（无定义，非遗漏） */
#   define GET_CPUINFO    23	/* get information about cpus */
#   define GET_REGS	  24	/* get general process registers */
#   define GET_CPUTICKS	  25	/* get per-state ticks for a cpu */
```

客户端（`minix3/minix/lib/libsys/sys_getinfo.c`，K&R 形参）：

```c
    message m;
    m.m_lsys_krn_sys_getinfo.request = request;
    m.m_lsys_krn_sys_getinfo.endpt = SELF;	/* always store values at caller */
    m.m_lsys_krn_sys_getinfo.val_ptr = (vir_bytes)ptr;
    ...
    return(_kernel_call(SYS_GETINFO, &m));
```

`endpt = SELF` 是本通道的签名语义：**内核永远存到调用方自己**——没有
"替别人取"的选项（对比 §2.4 getsysinfo 的跨地址空间拷贝）。速记宏
（`syslib.h:175-187`，如 `sys_getproctab(dst)` 即 `sys_getinfo(GET_PROCTAB,
dst, 0,0,0)`）是 IS 实际使用的拼写。

**IS 用法**（8 请求）：`GET_MONPARAMS`（minix3/minix/servers/is/dmp_kernel.c:monparams_dmp（L101，工具生成））、`GET_IRQHOOKS`
（:129）、`GET_IRQACTIDS`（:133）、`GET_IMAGE`（:174）、`GET_KINFO`（:197）、
`GET_MACHINE`（:201）、`GET_PRIVTAB`（:261）、`GET_PROCTAB`（:265/328/368、
minix3/minix/servers/is/dmp_vm.c:vm_dmp（L83，工具生成））。注意缺席者：`GET_KENV`——`kenv_dmp` 读的是 kinfo+machine
（:197/:201），从未调 `sys_getkenv`（V1 定型后无 `get_kenv` 方法可调，
§3 D2 V1 注记；初版 `GetRequest` 无 Kenv 变体同理）。

### 2.2 sys_diagctl_stacktrace：请馆长批注（栈回溯）

```c
#  define SYS_DIAGCTL    (KERNEL_CALL + 44)	/* sys_diagctl() */  /* com.h:252 */
/* Codes and field names for SYS_DIAGCTL. */                             /* :412-415 */
#define DIAGCTL_CODE_DIAG	1	/* Print diagnostics. */
#define DIAGCTL_CODE_STACKTRACE	2	/* Print process stack. */
#define DIAGCTL_CODE_REGISTER	3	/* Register for diagnostic signals */
#define DIAGCTL_CODE_UNREGISTER	4	/* Unregister for diagnostic signals */
```

```c
int sys_diagctl(int code, char *arg1, int arg2)   /* sys_diagctl.c */
{
  message m;
  m.m_lsys_krn_sys_diagctl.code = code;
  switch(code) {
  ...
  case DIAGCTL_CODE_STACKTRACE:
	m.m_lsys_krn_sys_diagctl.endpt = (endpoint_t)arg2;
	break;
  ...
  default:
	panic("Unknown SYS_DIAGCTL request %d\n", code);
  }
  return(_kernel_call(SYS_DIAGCTL, &m));
}
#define sys_diagctl_stacktrace(ep) \
	sys_diagctl(DIAGCTL_CODE_STACKTRACE, NULL, ep)   /* syslib.h:166-168 */
```

目标端点复用 `arg2`（`int`→`endpoint_t` 强转）——单消息多用途的典型 C
手法。语义是"请内核**打印**该进程的栈"（经内核诊断输出，不回传任何缓冲——
输出面属 kernel，详见 `32-stack-tracing.md`；对比 §2.3 的 kmessages 回传，
两者不可互换，A-3 设计已言明）。

**IS 用法**：`procstack_dmp` 对每个目标 `sys_diagctl_stacktrace(rp->p_endpoint)`
（minix3/minix/servers/is/dmp_kernel.c:procstack_dmp（L378，工具生成））。**两侧现状**：Minix3 C 侧完整实现（`do_diagctl` 的
STACKTRACE 分支调 `proc_stacktrace`，`kernel/system/do_diagctl.c:43-47`）；
minix-rs Rust 侧暂 ENOSYS（`syscall.rs:670`，`32-stack-tracing.md` forward
reference，plan §5.2 已登记）——IS 调了，Rust 内核暂不接，结果是
warn-and-continue（§2.6），不是启动失败。

### 2.3 kerninfo 直读：阅览证（kmessages）+ A-3

```c
struct minix_kerninfo *_minix_kerninfo = NULL;   /* libc/sys/init.c:6 */

void __minix_init(void)   /* __attribute__((constructor))，:13-31 */
{
	if((ipc_minix_kerninfo(&_minix_kerninfo) != 0) ||
		(_minix_kerninfo->kerninfo_magic != KERNINFO_MAGIC))
	{
		_minix_kerninfo = NULL;
	}
	...
}
```

每个服务进程启动时，libc 构造子调 `ipc_minix_kerninfo()`（机器相关汇编，
`MINIX_KERNINFO` 门）取内核映射好的 `.usermapped` 页地址，并以
`KERNINFO_MAGIC`（`0xfc3b84bf`，type.h:229）校验——magic 不对就置 NULL
（之后 `get_minix_kerninfo()` 的 `assert` 会炸，`minix3/minix/lib/libc/sys/kernel_utils.c:get_minix_kerninfo`）。
页内 `kmessages`（`minix3/minix/kernel/usermapped_data.c:kmessages`）是内核诊断消息环形缓冲，
`kmessages_dmp` 直读并展开（minix3/minix/servers/is/dmp_kernel.c:kmessages_dmp，`km_next`/`km_size` 环形
游标，`_KMESS_BUF_SIZE` 静态打印缓冲）。

**`[ARCH: A-3]`**（三处之一，本节）：minix-rs 64-bit 不移植 `.usermapped`
段（`28-usermapped-data.md` 既定结论）→ 上述整条"映射页直读"链无处可挂。
演进设计：新增 `GET_KMESSAGES` 等价 `sys_getinfo` 子请求（kernel 侧拷贝
`kmessages` 环形缓冲），IS 经 `KerninfoTransport` 通道取数（§3.3）。
否决复用 DIAGCTL（§2.2：只打印不回传）。
另两处：design D3 + `KerninfoTransport` 注释。

### 2.4 getsysinfo：他馆发函（跨服务表）

客户端（`minix3/minix/lib/libsys/getsysinfo.c` 全文逻辑）：

```c
  switch (who) {
  case PM_PROC_NR: call_nr = PM_GETSYSINFO; break;    /* callnr.h:60 = 47 */
  case VFS_PROC_NR: call_nr = VFS_GETSYSINFO; break;  /* callnr.h:120 = 0x130 */
  case RS_PROC_NR: call_nr = RS_GETSYSINFO; break;    /* com.h:476 = RS_RQ_BASE+9 */
  case DS_PROC_NR: call_nr = DS_GETSYSINFO; break;    /* com.h:507 = DS_BASE+7 */
  default:
	return ENOSYS;
  }
  memset(&m, 0, sizeof(m));
  m.m_lsys_getsysinfo.what = what;     /* SI_* */
  m.m_lsys_getsysinfo.where = (vir_bytes)where;  /* 调用方地址 */
  m.m_lsys_getsysinfo.size = size;     /* 调用方声明长度 */
  return _taskcall(who, call_nr, &m);
```

`who` 只认四服务，其余 `ENOSYS`（客户端守门）。消息带三字段：取什么
（`what`）、放哪（调用方虚地址 `where`）、多长（`size`）。

服务侧（PM `misc.c:105-145` / VFS `misc.c:61-113`，RS/DS 同构）：

1. **root 门**：PM 查 `mp_effuid != 0`（+ stacktrace 留痕），VFS 查
   `super_user` → `EPERM`（"This call leaks important information"，
   两处注释原文）。
2. **`what` 分发表**：`SI_PROC_TAB` → 整表地址 + `sizeof × NR` 长度
   （VFS 另有 `SI_DMAP_TAB`、`SI_PROCLIGHT_TAB` 现场组装）。
3. **尺寸精确匹配**：`if (len != size) return(EINVAL)`（PM `:140` /
   VFS `:108`）——差一字节就拒绝，不截断不补零。调用方必须传
   `sizeof` 精确值（Rust D4 义务②）。
4. **`sys_datacopy(SELF → who_e)`**：服务把自家内存拷到调用方地址
   （与 §2.1 `endpt=SELF` 对比：本通道是**跨地址空间拷贝**，方向仍是
   "到调用方"，但执行者是服务进程而非内核）。

**IS 用法**：PM×2（`SI_PROC_TAB`，minix3/minix/servers/is/dmp_pm.c:mproc_dmp（L47，工具生成）/82）、VFS×2（`SI_PROC_TAB`
:31 + `SI_DMAP_TAB` :71）、RS×2（`SI_PROCPUB_TAB` + `SI_PROC_TAB`，
minix3/minix/servers/is/dmp_rs.c:rproc_dmp（L33，工具生成））、DS×1（`SI_DATA_STORE`，minix3/minix/servers/is/dmp_ds.c:data_store_dmp（L15，工具生成））。IS uid 0，
root 门恒过（D4 义务①的证据）。

### 2.5 vm_info×3：厚卷分批送（地址空间）

```c
#define VM_INFO			(VM_RQ_BASE+40)   /* com.h:729 */
/* VM_INFO 'what' values. */                          /* com.h:731-734 */
#define VMIW_STATS			1
#define VMIW_USAGE			2
#define VMIW_REGION			3
```

客户端（`minix3/minix/lib/libsys/vm_info.c`）：

- `vm_info_stats(vsi)`：`what=VMIW_STATS, ptr=vsi`（全系统内存统计，一发一收）。
- `vm_info_usage(who, vui)`：加 `ep=who`（某进程用量）。
- `vm_info_region(who, vri, count, next)`：加 `count/next`，返回后**写回**
  `*next = m.next`，返回 `m.count`（实际条数）——游标协议：
  `next==0` 开新的一轮（`prev_base` 初值 0，`first = prev_base == 0` —
  minix3/minix/servers/is/dmp_vm.c:vm_dmp（L62，工具生成）/92），返回的 count/next 驱动下一轮（10 的
  `prev_base`/`prev_i` 即此游标，minix3/minix/servers/is/dmp_vm.c:vm_dmp（L94，工具生成））。

**IS 用法**：`vm_info_stats`（minix3/minix/servers/is/dmp_vm.c:vm_dmp（L66，工具生成））+ `sys_getproctab`（:83 取端点表）
+ `vm_info_usage`（:110）+ `vm_info_region` 批循环（:94/:131）。服务端实现
归 `02-stage-vm/26-vm-queries.md`（本篇只客户端面）。

### 2.6 错误面：三层错误与 warn-and-continue

| 层 | 值域 | 例子 | IS 反应 | 证据 |
|---|---|---|---|---|
| 传输失败 | 负值 | `_taskcall` 发不出（`s < 0`） | 告警 + continue（位图按零） | 03 §2.3② |
| 服务拒绝 | 正 errno | EPERM（非 root）、EINVAL（尺寸）、ENOSYS（未知 who） | 告警 + return（跳过本屏） | minix3/minix/servers/is/dmp_pm.c:mproc_dmp（L47，工具生成） `!= OK` 即返 |
| 数据缺席 | — | DIAGCTL 未接线（ENOSYS）、kmessages 未映射 | 告警 + return | minix3/minix/servers/is/dmp_kernel.c:procstack_dmp（L378，工具生成） 后流程照走 |

统一形状（minix3/minix/servers/is/dmp_kernel.c:monparams_dmp（L101，工具生成） 式）：`if ((r = sys_getX(...)) != OK) { printf告警;
return; }`——**永不 panic**。与 01 的分界：transport（收/发）失败 = 事件
循环已死 → panic；acquire（取数）失败 = 本屏作废 → warn。`panic` 归
transport，`warn` 归 acquire（`acquire.rs` 头注释原文）。

---

## 3. Rust 设计决策

### 3.1 D1：常量入 `minix-types`（单权威）

`ipc/sysinfo.rs`：GET_* 全表（含 7/22 缺号注释）、SI_* 7 值、DIAGCTL_CODE_*
4 值、SYS_GETINFO/SYS_DIAGCTL、PM/VFS_GETSYSINFO（RS/DS/VM 同类已在各模块，
不重复）。否决散落 05~10（pattern A）。05~10 从此 import（§2.4g 跨篇权威，
02 §4.2 体例延续）。

### 3.2 D2：五通道 trait 缝（细缝，各取所需）

`acquire.rs` 五 trait（§4.2 签名）：`SysGetinfoTransport` 传请求码（载荷类型
归 kernel crate，04 不碰布局——布局是 05 的）；`DiagctlTransport` 固定
STACKTRACE（码不参数化，调点无选择）；`KerninfoTransport`（A-3 缝）；
`GetSysinfoTransport`（who/what/len）；`VmInfoTransport`（region 三元组）。
生产实现 forward ref（minix-sys stub 期无内核可调；缝放 is crate，接线时
下沉——07-stage-ds A-8 客户端库归属先例）。否决一 trait 打包（05~10 各取
子集要细缝）与裸 `i32` 传请求（`GetRequest`/`SiWhat` 枚举限域：IS 未用的
GET_KENV、MIB 面的 SI_CALL_STATS 等编译期拒绝）。

> **V1 审查轮更新（2026-09-15，数据出口定型）**：初版 trait 只回状态码——
> `sys_getinfo(req) -> i32` 拿不到任何数据，而 C 语义是"数据拷到调用方"
> （§2.1 `endpt = SELF`、§2.4 `sys_datacopy`）。审查坐实这是 run_dump 填体
> 的前置缺口（todo V1-P1-2），定型为**类型化出参**：每个 C `sys_get*` 速记
> 宏 1:1 变成一个带出参切片的方法（`sys_getproctab(dst)` ↔
> `get_proctab(out: &mut [KProcSnap])`，slice 长度即调用方声明的容量），
> `GetRequest` 枚举被方法本身取代——只用来选方法的枚举没有存在价值，
> "IS 未用的请求编译期拒绝"从枚举限域升级为结构性不可表达（GET_KENV/
> GET_MACHINE 无方法可调）。`SiWhat`/`getsysinfo_call`/`IS_GETSYSINFO_CALLS`
> 降格为生产 transport 的 wire 编码助手（合同与测试保留）。编排器侧新增
> `Acquires` 超特质（五 trait blanket 打包，RS `KernelApi` 五域先例）：
> `IsServer` 是唯一五通道都说话的组件，dump 体仍是只吃"已取快照 + 游标 +
> 诊断 sink"的自由函数，细缝决策不变。A-6 诊断输出通道同期定型：
> `SefTransport::diag_out() -> &mut dyn core::fmt::Write`（printf 的
> no_std 对应物，见 01 §3 D4 V1 注记）。

### 3.3 D3：A-3 kerninfo 抽象（§2.3 三处之二）

内容见 §2.3 演进设计。`KerninfoTransport::kmessages(meta, ring)` 即该通道
的缝（游标与环体分两口出参）。否决直读移植（与 28 既定结论冲突）
与 DIAGCTL 复用（语义不同）。

生产实现（`KernelKmessTransport`）按 10008 字节快照一次取回再拆包：8 字节
游标头（`km_next`/`km_size`，LE）进 `meta`，10000 字节环体按调用方缓冲
容量填前缀——缓冲短于环体不改变语义，`kmessages_dmp` 的打印组装本来就
按 `km_size` 游标重排（`minix3/minix/servers/is/dmp_kernel.c:kmessages_dmp`）。
宿主构建（未开 real-trap）下如实上浮 `-EIO`，不假装成功。

### 3.4 D4：IS 侧三义务（机读化）

① root 恒过（IS uid 0，system.conf:271，01 §2.11）；② len 精确
（调用点传 `size_of`，服务侧精确匹配是 EINVAL 的另一半）；③ 越权值拒绝
（`SiWhat` 四变体：ProcTab/DmapTab/ProcPubTab/DataStore——注意 ProcTab 的
owner 不是表的属性而是调用点的属性：PM 与 RS 各拉一次 `SI_PROC_TAB`
（minix3/minix/servers/is/dmp_pm.c:mproc_dmp（L47，工具生成）/minix3/minix/servers/is/dmp_rs.c:rproc_dmp（L34，工具生成）），故不用 `owner()` 方法而用
`IS_GETSYSINFO_CALLS` 六对表显式枚举，单 owner 映射会误路由 RS 那一路）。

### 3.5 D5：region 游标签名（三元组）

`region(who, count, next) -> (status, next_out, count_out)`（minix3/minix/lib/libsys/vm_info.c:vm_info_region
写回语义）；推进规则归 10。本篇 fake 镜像写回单测。

### 3.6 D6：错误面（i32 直通，不转 Result）

traits 返回 Minix 码原样：05~10 按 C 惯用 `!= OK` 判定；转 `Result` 会割裂
与 C 行号的对照（review 时 grep `!= OK` 双向可搜）。panic/warn 分界见 §2.6。

---

## 4. 实现详解

### 4.1 模块树（增量）

```text
os/libs/minix-types/src/ipc/sysinfo.rs — GET/SI/DIAGCTL/SYS/PM/VFS 常量 + 单测（D1）
os/servers/is/src/acquire.rs           — SiWhat/IS_GETSYSINFO_CALLS/getsysinfo_call 助手 + 5 类型化 trait + Acquires 超特质 + 生产实现（SysAcquires/KernelKmessTransport/SysGetsysinfo）+ Unimplemented + fake（D2-D6，V1 轮定型；生产面随 S23 分批真装）
os/servers/is/src/lib.rs               — pub mod acquire + 重导出 + IsServer<T, F, A: Acquires>
```

### 4.2 关键签名（与 §3 一致，Gate D-5 依据；V1 轮按数据出口定型重写）

```rust
// acquire.rs（wire 助手）
pub enum SiWhat { ProcTab, DmapTab, ProcPubTab, DataStore }          // 生产 transport 的 what 编码用
pub const IS_GETSYSINFO_CALLS: &[(Endpoint, SiWhat)];   // 6 对（RS 双拉含内）
pub const fn getsysinfo_call(who: Endpoint) -> i32;   // PM/VFS/RS/DS → callnr，else ENOSYS

// acquire.rs（五通道缝：类型化出参，C 速记宏 1:1）
pub trait SysGetinfoTransport {
    fn get_kinfo(&mut self, out: &mut KinfoSnap) -> i32;                 // sys_getkinfo（dmp_kernel.c:197）
    fn get_image(&mut self, out: &mut [BootImageSnap]) -> i32;           // sys_getimage（:174）
    fn get_proctab(&mut self, out: &mut [KProcSnap]) -> i32;             // sys_getproctab（:265/328/368、dmp_vm.c:83）
    fn get_monparams(&mut self, out: &mut [u8]) -> i32;                  // sys_getmonparams（:101）
    fn get_irqhooks(&mut self, out: &mut [IrqHookSnap]) -> i32;          // sys_getirqhooks（:129）
    fn get_irqactids(&mut self, out: &mut [i32]) -> i32;                 // sys_getirqactids（:133）
    fn get_privtab(&mut self, out: &mut [KPrivSnap]) -> i32;             // sys_getprivtab（:261）
}
pub trait DiagctlTransport { fn stacktrace(&mut self, proc: Endpoint) -> i32; }  // 无出参：trace 打进内核日志
pub trait KerninfoTransport { fn kmessages(&mut self, meta: &mut KmessagesSnap, ring: &mut [u8]) -> i32; }  // [ARCH: A-3]
pub trait GetSysinfoTransport {
    fn pm_proc_tab(&mut self, out: &mut [MProcSnap]) -> i32;             // dmp_pm.c:47/82
    fn vfs_proc_tab(&mut self, out: &mut [FProcSnap]) -> i32;            // dmp_fs.c:31
    fn vfs_dmap_tab(&mut self, out: &mut [DmapSnap]) -> i32;             // dmp_fs.c:71
    fn rs_tables(&mut self, pub_out: &mut [RprocpubSnap], priv_out: &mut [RprocSnap]) -> i32;  // dmp_rs.c:33-34 双拉一体
    fn ds_data_store(&mut self, out: &mut [DsEntrySnap]) -> i32;         // dmp_ds.c:15
}
pub trait VmInfoTransport {
    fn vm_stats(&mut self, out: &mut VmStatsSnap) -> i32;                // dmp_vm.c:66
    fn vm_usage(&mut self, who: Endpoint, out: &mut VmUsageSnap) -> i32; // dmp_vm.c:110
    fn vm_region(&mut self, who: Endpoint, out: &mut [VmRegionSnap], next: u64) -> (i32, u64, i32);  // :94/131
}
pub trait Acquires: SysGetinfoTransport + DiagctlTransport + KerninfoTransport + GetSysinfoTransport + VmInfoTransport {}  // blanket impl；IsServer<T, F, A: Acquires>
```

### 4.2b 生产取数面（S23 分批落地）

五种通道的生产实现在 `SysAcquires` 一处装配（`main.rs` 换装），按批：
`SysGetinfoTransport` 七 what 直调 `sys_getinfo_into`、`DiagctlTransport`
走 `SYS_DIAGCTL` code 2、`ClockTransport` 读 `SYS_TIMES` 的 `real_ticks`
（A-3 无 usermapped 页）、`KerninfoTransport` 按 `GET_KMESSAGES` 的 10008
字节快照取回再拆包（§3.3）。

`GetSysinfoTransport` 的生产件 `SysGetsysinfo` 是五条腿的客户端，每条腿
一个 `*_via(transport, …)` 形态（生产传 `DirectTrapTransport`，测试传脚本
双替身）：请求按 `m_lsys_getsysinfo` 的三 lane 打包（`what` @0、调用方缓冲
虚地址 @8、字节长度 @16），调用号取自 `getsysinfo_call`（C
`getsysinfo.c:14-24` 的 `who` 开关），`size` **必须**等于出参切片的
`size_of_val`——服务端的尺寸门是精确匹配（§2.4）。两条腿的边界情形：

- **RS 双拉**（`rs_tables`）：先 `SI_PROCPUB_TAB` 后 `SI_PROC_TAB`，前者
  失败即返回（C `dmp_rs.c:33-34` 的 `||` 短路）。
- **VM_INFO 三查询**（`VmInfoTransport`，片 3b-3）：走值通道（`[ARCH:
  26-D1]`）——请求域 `what`/`ep`/`count`/`next` 按 `m_lsys_vm_info` 打包，
  `ptr` 恒 0（结果不回写调用方地址空间），结果从回复消息的 M1 槽解码
  （槽位对照见 §4.2b 与 `10-is-dump-vm.md` §3.1）。REGION 批非空而条目
  数组未随回复送达时（D7 编码缺口）回 `-ENOTSUP`，不伪造空表。

- **VFS 两腿**：本地回 `-ENOSYS` 而不发消息——VFS 侧的 `do_getsysinfo`
  纯函数半已备，但运行时应答面尚未接线（`dispatch_syscall` 把
  `Getsysinfo` 归入 `SyscallResult::Nosys` 且主循环不回信），此刻发过去
  会等一个永不来的回复并把 IS 挂死。VFS 应答面随 W1 传输批次落地后，这两
  条腿换成正常发送即可。

常量权威位置（§2.4g）：GET/SI/DIAGCTL/SYS/PM/VFS 唯一定义于
`minix-types::ipc::sysinfo`（RS/DS/VM 同类沿用各模块，不迁入）。

### 4.3 关键不变量

1. **每个取数方法带类型化出参**：出参 slice 是调用方内存（C `endpt = SELF`
   /`sys_datacopy` 的 Rust 形状），slice 长度即调用方声明的容量；IS 永不
   发的请求（GET_KENV/GET_MACHINE）无方法可调——结构性拒绝（V1 定型，
   取代初版 `GetRequest` 枚举限域）。
2. 调用对表显式（`IS_GETSYSINFO_CALLS` 六对；RS 双拉陷阱已命名，RS 双表
   收敛为全有或全无的一个方法）。
3. `getsysinfo_call` 未知 who → ENOSYS（minix3/minix/lib/libsys/getsysinfo.c:getsysinfo（L14，工具生成））。
4. `vm_region` 三元组写回，`out.len()` 即 capacity，返回的 count ≤ capacity
   （minix3/minix/lib/libsys/vm_info.c:vm_info_region）。
5. Unimplemented 全 panic（fail-closed）；acquire 永不 panic（§2.6）。

---

## 5. 测试要点

| # | 场景 | 期望 | C 依据 |
|---|---|---|---|
| T1 | GET 速记宏 ↔ 类型化方法 1:1（7 方法，KENV/MACHINE 无方法） | 结构性（签名即对账） | syslib.h:175-187 |
| T2 | SI 7 值 + DIAGCTL 4 值 + SYS 两码 + PM/VFS callnr | 逐值 | sysinfo.h/callnr.h |
| T3 | SiWhat 码 + 调用对表（含 RS 双拉陷阱） | — | minix3/minix/servers/is/dmp_rs.c:rproc_dmp（L33，工具生成） |
| T4 | who→callnr 四映射 + TTY/VM → ENOSYS | — | minix3/minix/lib/libsys/getsysinfo.c:getsysinfo（L14，工具生成） |
| T5 | 五通道 OK/ERR 双路（ERR 回流不 panic；ERR 时出参不被改写） | — | §2.6 |
| T6 | OK 取数填出参（proctab/monparams 实填断言） | — | §2.1/§2.4 拷贝语义 |
| T7 | region 三元组写回 + count 夹到 capacity | — | minix3/minix/lib/libsys/vm_info.c:vm_info_region |
| T8 | Unimplemented 全 panic（A-3 含） | `#[should_panic]` | fail-closed |

### 5.3 测试统计（截至 2026-09-04；V1 轮定型后基线见 todo.md §0）

- `cargo test -p minix-is`：**47 passed, 0 failed**（`acquire` 新增 6）。
- `cargo test -p minix-types`：**122 passed, 0 failed**（`sysinfo` 新增 3）。
- `cargo clippy/check` 两 crate：零新增警告。
- 完整清单：`rg "#\[test\]" os/servers/is/src/acquire.rs os/libs/minix-types/src/ipc/sysinfo.rs`。

---

## 6. 过渡：05~10 消费映射

| 篇 | 取用通道 | 请求 |
|---|---|---|
| 05-kernel | sys_getinfo ×8 + diagctl + kerninfo | §2.1 八项 + STACKTRACE + kmessages |
| 06-pm | getsysinfo ×1 表 | SI_PROC_TAB（PM） |
| 07-vfs | getsysinfo ×2 表 | SI_PROC_TAB + SI_DMAP_TAB（VFS） |
| 08-rs | getsysinfo ×2 表 | SI_PROCPUB_TAB + SI_PROC_TAB（RS） |
| 09-ds | getsysinfo ×1 表 | SI_DATA_STORE（DS） |
| 10-vm | sys_getinfo ×1 + vm_info ×3 | PROCTAB + stats/usage/region |

数据面前置完成——05 起每篇只讲"布局怎么解释"，不再讲"数怎么来的"。

---

## 7. 参见

- `01-is-init-main.md` §2.9/§2.5：panic/warn 分界（本篇错误面的上游）
- `05~10-is-dump-*.md`（待写）：通道消费者（§6 映射表）
- `../01-stage-kernel/25-misc-unported.md`：GETINFO 机制本体
- `../01-stage-kernel/28-usermapped-data.md`：usermapped 移除（A-3 上游）
- `../01-stage-kernel/32-stack-tracing.md`：DIAGCTL ENOSYS 现状
- `../02-stage-vm/26-vm-queries.md`：VM_INFO 服务端
- `../07-stage-ds/03-ds-data-structures.md` + `11-ds-getsysinfo.md`：DS 布局 + 服务侧实现
- plan §4（A-3/A-4）/§5.2（头文件表）/§5.3（04 函数清单）
