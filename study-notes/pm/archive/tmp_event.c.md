# servers/pm/event.c 逐行讲解

> **文件路径**: `minix3/minix/servers/pm/event.c`
> **核心功能**: 进程事件发布/订阅机制
> **代码行数**: 353 行

---

## 文件概述

### 是什么（功能说明）

这个文件实现了一个通用的进程事件发布/订阅机制，用于通知系统服务进程状态变化。

**支持的事件类型**:

| 事件 | 值 | 含义 |
|------|-----|------|
| `PROC_EVENT_EXIT` | 0x01 | 进程退出 |
| `PROC_EVENT_SIGNAL` | 0x02 | 进程捕获信号 |

**典型订阅者**: System V IPC 服务器

### 为什么（设计原因）

**同步串行设计**:
- 避免消息队列溢出
- 限制异步消息数量
- 简化实现复杂度

**消息数量对比**:

| 方式 | 消息数量 |
|------|---------|
| 串行同步 | NR_PROCS |
| 并行同步 | NR_PROCS × NR_SUBS |
| 异步通知 | 无上限 |

### 什么情景使用（应用场景）

| 场景 | 事件 | 订阅者行为 |
|------|------|-----------|
| 进程退出 | `PROC_EVENT_EXIT` | 清理 IPC 资源 |
| 进程收到信号 | `PROC_EVENT_SIGNAL` | 中断阻塞的系统调用 |

---

## 头文件注释详解

```c
/*
 * This file implements a generic process event publish/subscribe facility.
 * The facility for use by non-core system services that implement part of the
 * userland system call interface.  Currently, it supports two events: a
 * process catching a signal, and a process being terminated.  A subscribing
 * service would typically use such events to interrupt a blocking system call
 * and/or clean up process-bound resources.  As of writing, the only service
 * that uses this facility is the System V IPC server.
```

**注释翻译**: "这个文件实现了一个通用的进程事件发布/订阅机制。该机制用于实现用户空间系统调用接口的非核心系统服务。目前支持两种事件：进程捕获信号和进程终止。订阅服务通常使用这些事件来中断阻塞的系统调用和/或清理进程相关资源。截至撰写时，唯一使用此机制的服务是 System V IPC 服务器。"

**设计思路讲解**:
- **发布/订阅模式**: 解耦事件生产者（PM）和消费者（系统服务）
- **非核心服务**: 核心服务（VFS、VM）有其他机制，此机制用于扩展服务

```c
 * Each of these events will be published to subscribing services right after
 * VFS has acknowledged that it has processed the same event.  For each
 * subscriber, in turn, the process will be blocked (with the EVENT_CALL flag
 * set) until the subscriber acknowledges the event or PM learns that the
 * subscriber has died.  Thus, each subscriber adds a serialized messaging
 * roundtrip for each subscribed event.
```

**注释翻译**: "每个事件在 VFS 确认已处理该事件后立即发布给订阅服务。对于每个订阅者，依次地，进程将被阻塞（设置 EVENT_CALL 标志），直到订阅者确认事件或 PM 得知订阅者已死亡。因此，每个订阅者为每个订阅事件添加一个串行化的消息往返。"

**关键概念**:
- **EVENT_CALL 标志**: 表示进程正在等待事件订阅者回复
- **串行化**: 订阅者依次处理，不是并行

```c
 * The one and only reason for this synchronous, serialized approach is that it
 * avoids PM queuing up too many asynchronous messages.  In theory, each
 * running process may have an event pending, and thus, the serial synchronous
 * approach requires NR_PROCS asynsend slots.  For a parallel synchronous
 * approach, this would increase to (NR_PROCS*NR_SUBS).  Worse yet, for an
 * asynchronous event notification approach, the number of messages that PM can
 * end up queuing is potentially unbounded, so that is certainly not an option.
 * At this moment, we expect only one subscriber (the IPC server) which makes
 * the serial vs parallel point less relevant.
```

**注释翻译**: "使用同步串行方法的唯一原因是避免 PM 排队太多异步消息。理论上，每个运行进程可能有一个待处理事件，因此串行同步方法需要 NR_PROCS 个 asynsend 槽位。对于并行同步方法，这将增加到 (NR_PROCS*NR_SUBS)。更糟糕的是，对于异步事件通知方法，PM 可能排队的消息数量可能无限制，所以这绝对不是一个选项。目前，我们预计只有一个订阅者（IPC 服务器），这使得串行与并行的区别不太相关。"

**设计权衡**:
- **串行同步**: 消息数量可控，但延迟较高
- **并行同步**: 延迟低，但消息数量多
- **异步通知**: 延迟最低，但消息数量不可控

```c
 * It is not possible to subscribe to events from certain processes only.  If
 * a service were to subscribe to process events as part of a system call by
 * a process (e.g., semop(2) in the case of the IPC server), it may subscribe
 * "too late" and already have missed a signal event for the process calling
 * semop(2), for example.  Resolving such race conditions would require major
 * infrastructure changes.
```

**注释翻译**: "不可能只订阅某些进程的事件。如果服务在进程的系统调用期间订阅进程事件（例如 IPC 服务器的 semop(2)），它可能订阅'太晚'，已经错过了调用 semop(2) 的进程的信号事件。解决这种竞态条件需要重大的基础设施更改。"

**竞态条件示例**:
```
时间线:
T1: 进程 P 调用 semop()
T2: 进程 P 收到信号
T3: IPC 服务器订阅事件
T4: 信号事件已错过
```

```c
 * A server may however change its event subscription mask at runtime, so as to
 * limit the number of event messages it receives in a crude fashion.  For the
 * same race-condition reasons, new subscriptions must always be made when
 * processing a message that is *not* a system call potentially affected by
 * events.  In the case of the IPC server, it may subscribe to events from
 * semget(2) but not semop(2).  For signal events, the delay call system
 * guarantees the safety of this approach; for exit events, the message type
 * prioritization does (which is not great; see the TODO item in forkexit.c).
```

**注释翻译**: "然而，服务器可以在运行时更改其事件订阅掩码，以粗略限制其接收的事件消息数量。出于同样的竞态条件原因，新订阅必须在处理不受事件影响的系统调用消息时进行。对于 IPC 服务器，它可以从 semget(2) 订阅事件，但不能从 semop(2) 订阅。对于信号事件，延迟调用系统保证此方法的安全性；对于退出事件，消息类型优先级保证（这不是很好；参见 forkexit.c 中的 TODO 项）。"

**订阅时机**:
- **安全**: semget() - 不受事件影响
- **不安全**: semop() - 可能被信号中断

```c
 * After changing its mask, a subscribing service may still receive messages
 * for events it is no longer subscribed to.  It should acknowledge these
 * messages by sending a reply as usual.
 */
```

**注释翻译**: "更改掩码后，订阅服务可能仍会收到它不再订阅的事件的消息。它应该像往常一样发送回复来确认这些消息。"

---

## 头文件包含

```c
#include "pm.h"
#include "mproc.h"
#include <assert.h>
```

**讲解**: 包含必要的头文件：
- `pm.h`: PM 主头文件，包含全局定义
- `mproc.h`: PM 进程结构定义
- `assert.h`: 断言宏，用于运行时检查

---

## 订阅者数量限制

```c
/*
 * A realistic upper bound for the number of subscribing services.  The process
 * event notification system adds a round trip to a service for each subscriber
 * and uses asynchronous messaging to boot, so clearly it does not scale to
 * numbers larger than this.
 */
#define NR_SUBS		4
```

**注释翻译**: "订阅服务数量的现实上限。进程事件通知系统为每个订阅者添加一个往返行程，并使用异步消息传递，因此显然不能扩展到更大的数量。"

**讲解**: 
- **NR_SUBS = 4**: 最大订阅者数量
- **为什么限制**: 串行处理，订阅者越多延迟越高

---

## 订阅者数据结构

```c
static struct {
	endpoint_t endpt;		/* endpoint of subscriber */
	unsigned int mask;		/* interests bit mask (PROC_EVENT_) */
	unsigned int waiting;		/* # procs blocked on reply from it */
} subs[NR_SUBS];

static unsigned int nsubs = 0;
static unsigned int nested = 0;
```

**内存布局**:
```
subs 数组 (静态存储区):
┌─────────────────────────────────────────────────────────┐
│ subs[0]                                                  │
│  ├─ endpt:    订阅者端点 (4 字节)                        │
│  ├─ mask:     事件掩码 (4 字节)                          │
│  └─ waiting:  等待回复的进程数 (4 字节)                  │
├─────────────────────────────────────────────────────────┤
│ subs[1]                                                  │
│  └─ ...                                                  │
├─────────────────────────────────────────────────────────┤
│ subs[2]                                                  │
│  └─ ...                                                  │
├─────────────────────────────────────────────────────────┤
│ subs[3]                                                  │
│  └─ ...                                                  │
└─────────────────────────────────────────────────────────┘

nsubs (静态存储区): 当前订阅者数量 (4 字节)
nested (静态存储区): 嵌套深度 (4 字节)
```

**字段讲解**:
- **endpt**: 订阅者的进程端点，用于发送消息
- **mask**: 事件掩码，表示感兴趣的事件
  - `PROC_EVENT_EXIT (0x01)`: 退出事件
  - `PROC_EVENT_SIGNAL (0x02)`: 信号事件
- **waiting**: 正在等待此订阅者回复的进程数量

**全局变量**:
- **nsubs**: 当前活跃的订阅者数量
- **nested**: 嵌套深度，用于检测递归调用

---

## resume_event 函数

```c
/*
 * For the current event of the given process, as determined by its flags, send
 * a process event message to the next subscriber, or resume handling the
 * event itself if there are no more subscribers to notify.
 */
static void
resume_event(struct mproc * rmp)
{
	message m;
	unsigned int i, event;
	int r;

	assert(rmp->mp_flags & IN_USE);
	assert(rmp->mp_flags & EVENT_CALL);
	assert(rmp->mp_eventsub != NO_EVENTSUB);
```

**注释翻译**: "对于给定进程的当前事件（由其标志确定），向下一个订阅者发送进程事件消息，或者如果没有更多订阅者需要通知，则恢复处理事件本身。"

**讲解**: 恢复事件处理，向下一个订阅者发送事件消息。

**参数**:
- `rmp`: 指向进程的 mproc 结构

**断言检查**:
1. 进程正在使用（`IN_USE`）
2. 进程正在等待事件回复（`EVENT_CALL`）
3. 事件订阅者索引有效（`!= NO_EVENTSUB`）

```c
	/* Which event should we be concerned about? */
	if (rmp->mp_flags & EXITING)
		event = PROC_EVENT_EXIT;
	else if (rmp->mp_flags & UNPAUSED)
		event = PROC_EVENT_SIGNAL;
	else
		panic("unknown event for flags %x", rmp->mp_flags);
```

**注释翻译**: "我们应该关注哪个事件？"

**讲解**: 根据进程标志确定事件类型：
- `EXITING`: 进程正在退出 → `PROC_EVENT_EXIT`
- `UNPAUSED`: 进程收到信号 → `PROC_EVENT_SIGNAL`
- 其他: 程序错误，panic

```c
	/*
	 * If there are additional services interested in this event, send a
	 * message to the next one.
	 */
	for (i = rmp->mp_eventsub; i < nsubs; i++, rmp->mp_eventsub++) {
		if (subs[i].mask & event) {
			memset(&m, 0, sizeof(m));
			m.m_type = PROC_EVENT;
			m.m_pm_lsys_proc_event.endpt = rmp->mp_endpoint;
			m.m_pm_lsys_proc_event.event = event;

			r = asynsend3(subs[i].endpt, &m, AMF_NOREPLY);
			if (r != OK)
				panic("asynsend failed: %d", r);

			assert(subs[i].waiting < NR_PROCS);
			subs[i].waiting++;

			return;
		}
	}
```

**注释翻译**: "如果有其他服务对此事件感兴趣，向下一个发送消息。"

**讲解**: 遍历订阅者，找到对当前事件感兴趣的订阅者。

**消息构造**:
- `m_type = PROC_EVENT`: 消息类型
- `endpt`: 进程端点
- `event`: 事件类型

**异步发送**: `asynsend3(..., AMF_NOREPLY)` 
- 异步发送，不等待回复
- `AMF_NOREPLY`: 不期望立即回复

**更新等待计数**: `subs[i].waiting++`

```c
	/* No more subscribers to be notified, resume the actual event. */
	rmp->mp_flags &= ~EVENT_CALL;
	rmp->mp_eventsub = NO_EVENTSUB;

	if (event == PROC_EVENT_EXIT)
		exit_restart(rmp);
	else if (event == PROC_EVENT_SIGNAL)
		restart_sigs(rmp);
}
```

**注释翻译**: "没有更多订阅者需要通知，恢复实际事件处理。"

**讲解**: 所有订阅者已通知完毕，恢复事件处理：
1. 清除 `EVENT_CALL` 标志
2. 重置订阅者索引
3. 调用相应的恢复函数：
   - 退出事件 → `exit_restart()`
   - 信号事件 → `restart_sigs()`

---

## remove_sub 函数

```c
/*
 * Remove a subscriber from the set, forcefully if we have to.  Ensure that
 * any processes currently subject to process event notification are updated
 * accordingly, in a way that no services are skipped for process events.
 */
static void
remove_sub(unsigned int slot)
{
	struct mproc *rmp;
	unsigned int i;

	/* The loop below needs the remaining items to be kept in order. */
	for (i = slot; i < nsubs - 1; i++)
		subs[i] = subs[i + 1];
	nsubs--;
```

**注释翻译**: "从集合中移除订阅者，必要时强制移除。确保当前正在进行进程事件通知的进程得到相应更新，不会跳过任何服务。"

**讲解**: 从订阅者数组中移除指定槽位的订阅者。

**数组移除操作**:
```
移除前:
subs[0] = A
subs[1] = B  ← 要移除
subs[2] = C
nsubs = 3

移除后:
subs[0] = A
subs[1] = C  ← 从 subs[2] 移动
nsubs = 2
```

```c
	/* Adjust affected processes' event subscriber indexes to match. */
	for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++) {
		if ((rmp->mp_flags & (IN_USE | EVENT_CALL)) !=
		    (IN_USE | EVENT_CALL))
			continue;
		assert(rmp->mp_eventsub != NO_EVENTSUB);
```

**注释翻译**: "调整受影响进程的事件订阅者索引以匹配。"

**讲解**: 遍历所有进程，更新正在等待事件回复的进程的订阅者索引。

**检查条件**: 进程正在使用且正在等待事件回复

```c
		/*
		 * While resuming a process could trigger new events, event
		 * calls always take place after the corresponding VFS calls,
		 * making this nesting-safe.  Check anyway, because if nesting
		 * does occur, we are in serious (un-debuggable) trouble.
		 */
		if ((unsigned int)rmp->mp_eventsub == slot) {
			nested++;
			resume_event(rmp);
			nested--;
		} else if ((unsigned int)rmp->mp_eventsub > slot)
			rmp->mp_eventsub--;
	}
}
```

**注释翻译**: "虽然恢复进程可能触发新事件，但事件调用总是在相应的 VFS 调用之后发生，这使得这嵌套安全。无论如何都要检查，因为如果确实发生嵌套，我们就遇到了严重的（无法调试的）麻烦。"

**讲解**: 
- 如果进程正在等待被移除的订阅者，立即恢复事件处理
- 如果进程正在等待后面的订阅者，减少索引

**nested 变量**: 用于检测嵌套调用，帮助调试

---

## do_proceventmask 函数

```c
/*
 * Subscribe to process events.  The given event mask denotes the events in
 * which the caller is interested.  Multiple calls will each replace the mask,
 * and a mask of zero will unsubscribe the service from events altogether.
 * Return OK on success, EPERM if the caller may not register for events, or
 * ENOMEM if all subscriber slots are in use already.
 */
int
do_proceventmask(void)
{
	unsigned int i, mask;

	/* This call is for system services only. */
	if (!(mp->mp_flags & PRIV_PROC))
		return EPERM;

	mask = m_in.m_lsys_pm_proceventmask.mask;
```

**注释翻译**: "订阅进程事件。给定的事件掩码表示调用者感兴趣的事件。多次调用将替换掩码，掩码为零将完全取消订阅。成功返回 OK，如果调用者不能注册事件返回 EPERM，如果所有订阅者槽位已使用返回 ENOMEM。"

**讲解**: 处理 `proceventmask` 系统调用，订阅或取消订阅进程事件。

**权限检查**: 只有系统服务（`PRIV_PROC`）可以订阅事件。

```c
	/*
	 * First check if we need to update or remove an existing entry.
	 * We cannot actually remove services for which we are still waiting
	 * for a reply, so set their mask to zero for later removal instead.
	 */
	for (i = 0; i < nsubs; i++) {
		if (subs[i].endpt == who_e) {
			if (mask == 0 && subs[i].waiting == 0)
				remove_sub(i);
			else
				subs[i].mask = mask;
			return OK;
		}
	}
```

**注释翻译**: "首先检查是否需要更新或移除现有条目。我们不能实际移除仍在等待回复的服务，所以将其掩码设置为零以便稍后移除。"

**讲解**: 检查调用者是否已订阅：
- 如果已订阅且新掩码为零且没有等待的进程，移除订阅
- 否则更新掩码

**为什么不能立即移除**: 如果有进程正在等待回复，移除会导致进程永久阻塞。

```c
	/* Add a new entry, unless the given mask is empty. */
	if (mask == 0)
		return OK;

	/* This case should never trigger. */
	if (nsubs == __arraycount(subs)) {
		printf("PM: too many process event subscribers!\n");
		return ENOMEM;
	}

	subs[nsubs].endpt = who_e;
	subs[nsubs].mask = mask;
	nsubs++;

	return OK;
}
```

**注释翻译**: "添加新条目，除非给定掩码为空。这种情况不应该触发。"

**讲解**: 添加新订阅者：
1. 如果掩码为零，直接返回
2. 检查是否已满
3. 添加新条目

---

## do_proc_event_reply 函数

```c
/*
 * A subscribing service has replied to a process event message from us, or at
 * least that is what should have happened.  First make sure of this, and then
 * resume event handling for the affected process.
 */
int
do_proc_event_reply(void)
{
	struct mproc *rmp;
	endpoint_t endpt;
	unsigned int i, event;
	int slot;

	assert(nested == 0);
```

**注释翻译**: "订阅服务已回复我们的进程事件消息，或者至少应该如此。首先确认这一点，然后恢复受影响进程的事件处理。"

**讲解**: 处理订阅服务的回复。

**断言**: `nested == 0`，确保不在嵌套调用中。

```c
	/*
	 * Is this an accidental call from a misguided user process?
	 * Politely tell it to go away.
	 */
	if (!(mp->mp_flags & PRIV_PROC))
		return ENOSYS;
```

**注释翻译**: "这是来自误导的用户进程的意外调用吗？礼貌地告诉它离开。"

**讲解**: 权限检查，只有系统服务可以回复事件消息。

```c
	/*
	 * Ensure that we got the reply that we want.  Since this code is
	 * relatively new, produce lots of warnings for cases that should never
	 * or rarely occur.  Later we can just ignore all mismatching replies.
	 */
	endpt = m_in.m_pm_lsys_proc_event.endpt;
	if (pm_isokendpt(endpt, &slot) != OK) {
		printf("PM: proc event reply from %d for invalid endpt %d\n",
		    who_e, endpt);
		return SUSPEND;
	}
	rmp = &mproc[slot];
	if (!(rmp->mp_flags & EVENT_CALL)) {
		printf("PM: proc event reply from %d for endpt %d, no event\n",
		    who_e, endpt);
		return SUSPEND;
	}
	if (rmp->mp_eventsub == NO_EVENTSUB ||
	    (unsigned int)rmp->mp_eventsub >= nsubs) {
		printf("PM: proc event reply from %d for endpt %d index %d\n",
		    who_e, endpt, rmp->mp_eventsub);
		return SUSPEND;
	}
	i = rmp->mp_eventsub;
	if (subs[i].endpt != who_e) {
		printf("PM: proc event reply for %d from %d instead of %d\n",
		    endpt, who_e, subs[i].endpt);
		return SUSPEND;
	}
```

**注释翻译**: "确保我们收到了想要的回复。由于此代码相对较新，为不应该或很少发生的情况产生大量警告。稍后我们可以忽略所有不匹配的回复。"

**讲解**: 验证回复的有效性：
1. 进程端点有效
2. 进程正在等待事件回复
3. 订阅者索引有效
4. 回复来自正确的订阅者

**返回 SUSPEND**: 不回复调用者，因为这是回复消息的回复。

```c
	if (rmp->mp_flags & EXITING)
		event = PROC_EVENT_EXIT;
	else if (rmp->mp_flags & UNPAUSED)
		event = PROC_EVENT_SIGNAL;
	else {
		printf("PM: proc event reply from %d for %d, bad flags %x\n",
		    who_e, endpt, rmp->mp_flags);
		return SUSPEND;
	}
	if (m_in.m_pm_lsys_proc_event.event != event) {
		printf("PM: proc event reply from %d for %d for event %d "
		    "instead of %d\n", who_e, endpt,
		    m_in.m_pm_lsys_proc_event.event, event);
		return SUSPEND;
	}
```

**讲解**: 验证事件类型匹配。

```c
	/*
	 * Do NOT check the event against the subscriber's event mask, since a
	 * service may have unsubscribed from an event while it has yet to
	 * process some leftover notifications for that event.  We could decide
	 * not to wait for the replies to those leftover notifications upon
	 * unsubscription, but that could result in problems upon quick
	 * resubscription, and such cases may in fact happen in practice.
	 */

	assert(subs[i].waiting > 0);
	subs[i].waiting--;
```

**注释翻译**: "不要根据订阅者的事件掩码检查事件，因为服务可能已取消订阅某个事件，但尚未处理该事件的一些剩余通知。我们可以决定在取消订阅时不等待这些剩余通知的回复，但这可能导致快速重新订阅时出现问题，这种情况实际上可能发生。"

**讲解**: 不检查掩码，因为订阅者可能已取消订阅但仍有待处理的通知。

**更新等待计数**: `subs[i].waiting--`

```c
	/*
	 * If we are now no longer waiting for any replies from an already
	 * unsubscribed (but alive) service, remove it from the set now; this
	 * will also resume events for the current process.  In the normal case
	 * however, let the current process move on to the next subscriber if
	 * there are more, and the actual event otherwise.
	 */
	if (subs[i].mask == 0 && subs[i].waiting == 0) {
		remove_sub(i);
	} else {
		rmp->mp_eventsub++;

		resume_event(rmp);
	}

	/* In any case, do not reply to this reply message. */
	return SUSPEND;
}
```

**注释翻译**: "如果我们不再等待已取消订阅（但存活）服务的任何回复，现在将其从集合中移除；这也将恢复当前进程的事件。但在正常情况下，让当前进程继续下一个订阅者（如果有更多），或者处理实际事件。"

**讲解**: 
- 如果订阅者已取消订阅且没有等待的进程，移除它
- 否则，继续下一个订阅者

---

## publish_event 函数

```c
/*
 * Publish a process event to interested subscribers.  The event is determined
 * from the process flags.  In addition, if the event is a process exit, also
 * check if it is a subscribing service that died.
 */
void
publish_event(struct mproc * rmp)
{
	unsigned int i;

	assert(nested == 0);
	assert((rmp->mp_flags & (IN_USE | EVENT_CALL)) == IN_USE);
	assert(rmp->mp_eventsub == NO_EVENTSUB);
```

**注释翻译**: "向感兴趣的订阅者发布进程事件。事件由进程标志确定。此外，如果事件是进程退出，还要检查是否是订阅服务死亡。"

**讲解**: 发布进程事件。

**断言检查**:
1. 不在嵌套调用中
2. 进程正在使用且不在等待事件回复
3. 订阅者索引未设置

```c
	/*
	 * If a system service exited, we have to check if it was subscribed to
	 * process events.  If so, we have to remove it from the set and resume
	 * any processes blocked on an event call to that service.
	 */
	if ((rmp->mp_flags & (PRIV_PROC | EXITING)) == (PRIV_PROC | EXITING)) {
		for (i = 0; i < nsubs; i++) {
			if (subs[i].endpt == rmp->mp_endpoint) {
				/*
				 * If the wait count is nonzero, we may or may
				 * not get additional replies from this service
				 * later.  Those will be ignored.
				 */
				remove_sub(i);

				break;
			}
		}
	}
```

**注释翻译**: "如果系统服务退出，我们必须检查它是否订阅了进程事件。如果是，我们必须将其从集合中移除，并恢复任何阻塞在该服务事件调用上的进程。"

**讲解**: 如果退出的进程是订阅服务，移除它。

**条件**: `PRIV_PROC | EXITING` - 系统服务正在退出

```c
	/*
	 * Either send an event message to the first subscriber, or if there
	 * are no subscribers, resume processing the event right away.
	 */
	rmp->mp_flags |= EVENT_CALL;
	rmp->mp_eventsub = 0;

	resume_event(rmp);
}
```

**注释翻译**: "要么向第一个订阅者发送事件消息，如果没有订阅者，则立即恢复处理事件。"

**讲解**: 
1. 设置 `EVENT_CALL` 标志
2. 初始化订阅者索引为 0
3. 调用 `resume_event()` 开始通知订阅者

---

## 要点总结

### 核心知识点

1. **发布/订阅模式**: 解耦事件生产者和消费者

2. **同步串行通知**: 依次通知订阅者，避免消息队列溢出

3. **事件类型**: 退出事件和信号事件

### 关键标志

| 标志 | 含义 |
|------|------|
| `EVENT_CALL` | 进程正在等待事件订阅者回复 |
| `PROC_EVENT_EXIT` | 进程退出事件 |
| `PROC_EVENT_SIGNAL` | 进程信号事件 |

---

## 灾难预演

### 场景 1: 订阅者崩溃

**如果订阅者崩溃**:
```c
// 订阅者无法回复
// 进程永久阻塞在 EVENT_CALL 状态
```

**后果**: 
- 进程挂起
- 资源无法释放

**解决方案**: `publish_event()` 检测订阅服务退出并移除

### 场景 2: 订阅者过多

**如果超过 NR_SUBS**:
```c
if (nsubs == __arraycount(subs)) {
    printf("PM: too many process event subscribers!\n");
    return ENOMEM;
}
```

**后果**: 
- 新订阅者无法订阅
- 返回 ENOMEM

### 场景 3: 嵌套调用

**如果发生嵌套**:
```c
assert(nested == 0);
```

**后果**: 
- 断言失败
- 系统崩溃

---

## 互动自测

### 问题 1: 为什么使用串行而不是并行通知？

**答案**: 
- 串行方式消息数量为 NR_PROCS
- 并行方式消息数量为 NR_PROCS × NR_SUBS
- 异步方式消息数量无上限
- 串行方式最安全

### 问题 2: 订阅者可以只订阅特定进程的事件吗？

**答案**: 
- 不可以
- 订阅是全局的，不是针对特定进程
- 原因：竞态条件，可能错过事件

### 问题 3: 订阅服务退出时会发生什么？

**答案**: 
- `publish_event()` 检测到订阅服务退出
- 调用 `remove_sub()` 移除订阅者
- 恢复所有等待该订阅者的进程

---

## Rust 实现对比

### 事件类型枚举

```rust
#![no_std]

use core::result::Result;

bitflags::bitflags! {
    pub struct ProcEvent: u32 {
        const EXIT = 0x01;
        const SIGNAL = 0x02;
    }
}
```

### 订阅者结构

```rust
#[derive(Debug, Clone, Default)]
pub struct Subscriber {
    pub endpt: i32,
    pub mask: ProcEvent,
    pub waiting: u32,
}

pub const NR_SUBS: usize = 4;
pub const NO_EVENTSUB: i8 = -1;

pub struct EventManager {
    subs: [Subscriber; NR_SUBS],
    nsubs: usize,
    nested: u32,
}

impl EventManager {
    pub const fn new() -> Self {
        Self {
            subs: [Subscriber::default(); NR_SUBS],
            nsubs: 0,
            nested: 0,
        }
    }
    
    pub fn subscribe(&mut self, endpt: i32, mask: ProcEvent) -> Result<(), EventError> {
        for i in 0..self.nsubs {
            if self.subs[i].endpt == endpt {
                if mask.is_empty() && self.subs[i].waiting == 0 {
                    self.remove_sub(i)?;
                } else {
                    self.subs[i].mask = mask;
                }
                return Ok(());
            }
        }
        
        if mask.is_empty() {
            return Ok(());
        }
        
        if self.nsubs >= NR_SUBS {
            return Err(EventError::NoSpace);
        }
        
        self.subs[self.nsubs] = Subscriber {
            endpt,
            mask,
            waiting: 0,
        };
        self.nsubs += 1;
        
        Ok(())
    }
    
    fn remove_sub(&mut self, slot: usize) -> Result<(), EventError> {
        for i in slot..self.nsubs - 1 {
            self.subs[i] = self.subs[i + 1].clone();
        }
        self.nsubs -= 1;
        Ok(())
    }
}

#[derive(Debug)]
pub enum EventError {
    NoSpace,
    InvalidEndpoint,
    NotWaiting,
    InvalidSubscriber,
}
```

### Rust 实现的优势

1. **类型安全**: 使用 `bitflags!` 宏定义事件掩码

2. **封装**: `EventManager` 封装所有状态

3. **错误处理**: 使用 `Result<T, E>` 显式处理错误

4. **常量**: 使用 `const fn` 初始化

### Rust 实现的权衡

1. **数组初始化**: 需要默认值或 `MaybeUninit`

2. **克隆开销**: 移除订阅者时需要克隆

3. **与 C 交互**: 需要使用 `unsafe` 块
