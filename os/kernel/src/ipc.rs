//! Kernel IPC core module.
//!
//! Implements the six IPC primitives (SEND, RECEIVE, SENDREC, NOTIFY, SENDNB, SENDA)
//! and supporting mechanisms (deadlock detection, sender queues, delayed delivery).
//!
//! # Module Organization
//!
//! - **11-ipc-core.md** types: `IpcCall`, `IpcError`, `SendFlags`, `SenderQueue`,
//!   `IpcEngine`, `DeadlockCycle`
//!
//! Design decisions are documented in 11-ipc-core.md §3.

use minix_types::{Endpoint, Message};
use core::sync::atomic::Ordering;
use crate::proc::{KProcess, ProcNr, RtsFlagsBits, MiscFlagsBits, NONE_PROC_NR};
use crate::proc_table::PROC_TABLE_SIZE;
use crate::kpriv::PrivTable;

// ── IPC call types ──

/// IPC 调用类型。
///
/// C: `call_nr` in `do_ipc()` — proc.c:599
/// Values: SEND=0, RECEIVE=1, SENDREC=2, NOTIFY=3, SENDNB=4, SENDA=5
/// (minix/com.h:178-183)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcCall {
    /// 同步发送。C: SEND
    Send,
    /// 同步接收。C: RECEIVE
    Receive,
    /// 原子发送+接收。C: SENDREC
    SendRec,
    /// 异步通知。C: NOTIFY
    Notify,
    /// 非阻塞发送。C: SENDNB
    SendNb,
    /// 异步批量发送。C: SENDA
    SendA,
}

/// IPC 错误码。
///
/// C: ELOCKED/EDEADSRCDST/ENOTREADY/EBADCALL/EFAULT/ECALLDENIED/ETRAPDENIED
/// (minix/errno.h, minix/com.h)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcError {
    /// 死锁检测发现循环等待。C: `ELOCKED` (-69)
    Deadlock,
    /// 源或目标端点无效。C: `EDEADSRCDST` (-70)
    DeadSrcDst,
    /// 非阻塞发送目标未就绪。C: `ENOTREADY` (-68)
    NotReady,
    /// 无效的 IPC 调用号。C: `EBADCALL` (-73)
    BadCall,
    /// 消息拷贝失败（页错误）。C: `EFAULT` (-14)
    Fault,
    /// IPC 权限被拒绝。C: `ECALLDENIED` (-71)
    CallDenied,
    /// 系统调用陷阱权限被拒绝。C: `ETRAPDENIED` (-72)
    TrapDenied,
}

/// 发送标志。
///
/// C: `flags` parameter in `mini_send()` — proc.c:870
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendFlags(u32);

impl SendFlags {
    pub const NONE: SendFlags = SendFlags(0);
    /// 非阻塞模式。C: `NON_BLOCKING` (0x01)
    pub const NON_BLOCKING: SendFlags = SendFlags(0x01);
    /// 来自内核。C: `FROM_KERNEL` (0x02)
    pub const FROM_KERNEL: SendFlags = SendFlags(0x02);
}

/// 死锁环描述。
///
/// C: `deadlock()` 检测到的循环等待链 — proc.c:703-770
///
/// The `direction` field records which IPC call type produced the cycle,
/// so the caller can decide how to react. In Minix3, `deadlock(RECEIVE, ...)`
/// and `deadlock(SEND, ...)` return the same kind of result (a cycle), but
/// the caller interprets the direction to decide if the cycle is a true
/// deadlock or a "request/reply 互锁" pattern (group_size==2 SEND↔RECEIVE
/// pair is NOT a deadlock).
///
/// Semantics: `direction` is the **call type that produced the cycle**, not
/// the state of any individual process in the chain. This matches C's
/// `deadlock()` return value semantics: if a SEND-chain forms a cycle, the
/// caller is SENDING; if a RECEIVE-chain forms a cycle, the caller is
/// RECEIVING.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlockCycle {
    /// 环中进程的 ProcNr 序列
    pub chain: alloc::vec::Vec<ProcNr>,
    /// 死锁方向 (SEND / RECEIVE)。调用方用此决定回退策略。
    /// C: 对应 `deadlock()` 的入参 `function`（SEND 或 RECEIVE）。
    pub direction: DeadlockDirection,
}

/// 死锁方向。
///
/// 与 `IpcCall::Send` / `IpcCall::Receive` 区分: `IpcCall` 是用户级 API 的
/// call number, `DeadlockDirection` 是 IPC 引擎检测到的具体等待方向。
/// 实际上两者一一对应 (SEND ↔ Send, RECEIVE ↔ Receive), 但用独立的枚举
/// 防止 deadlock detector 与 IPC dispatcher 的语义耦合。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlockDirection {
    /// 发送方向循环 (caller 正在 SENDING, 等待 RECEIVER 接 SEND)。
    /// C: `deadlock(SEND, caller, dst)` 路径。
    Send,
    /// 接收方向循环 (caller 正在 RECEIVING, 等待 SENDER 投递)。
    /// C: `deadlock(RECEIVE, caller, src)` 路径。
    Receive,
}

// ── Sender queue operations ──

/// 发送者等待队列操作。
///
/// C: `p_caller_q` 链表 — proc.h:120, proc.c:960-964
///
/// 使用 ProcNr 索引替代 C 的指针链表。
/// 每个进程有 `p_q_link` 字段指向下一个发送者。
pub struct SenderQueue;

impl SenderQueue {
    /// 将发送者加入目标进程的等待队列尾部。
    ///
    /// C: `while (*xpp) xpp = &(*xpp)->p_q_link; *xpp = caller_ptr;`
    /// — proc.c:960-964
    pub fn enqueue(procs: &mut [KProcess], dst_nr: ProcNr, sender_nr: ProcNr) {
        let dst_idx = match nr_to_idx(dst_nr) {
            Some(i) => i,
            None => return,
        };
        let sender_idx = match nr_to_idx(sender_nr) {
            Some(i) => i,
            None => return,
        };

        // 清除发送者的 q_link
        procs[sender_idx].p_q_link.store(NONE_PROC_NR, Ordering::Relaxed);

        // 找到队列尾部
        let mut tail: i32 = procs[dst_idx].p_caller_q.load(Ordering::Relaxed);
        loop {
            if tail == NONE_PROC_NR {
                break;
            }
            let tail_idx = match nr_to_idx(tail) {
                Some(i) => i,
                None => break,
            };
            let next = procs[tail_idx].p_q_link.load(Ordering::Relaxed);
            if next == NONE_PROC_NR {
                break;
            }
            tail = next;
        }

        if tail == NONE_PROC_NR {
            // 队列为空，设置 head
            procs[dst_idx].p_caller_q.store(sender_nr, Ordering::Relaxed);
        } else {
            // 追加到尾部
            let tail_idx = nr_to_idx(tail).unwrap();
            procs[tail_idx].p_q_link.store(sender_nr, Ordering::Relaxed);
        }
    }

    /// 从目标进程的等待队列中移除指定发送者。
    ///
    /// C: `*xpp = sender->p_q_link;` — proc.c:1097
    pub fn remove(procs: &mut [KProcess], dst_nr: ProcNr, sender_nr: ProcNr) {
        let dst_idx = match nr_to_idx(dst_nr) {
            Some(i) => i,
            None => return,
        };

        // 特殊情况：发送者是队头
        if procs[dst_idx].p_caller_q.load(Ordering::Relaxed) == sender_nr {
            let sender_idx = nr_to_idx(sender_nr).unwrap();
            let next = procs[sender_idx].p_q_link.load(Ordering::Relaxed);
            procs[dst_idx].p_caller_q.store(next, Ordering::Relaxed);
            procs[sender_idx].p_q_link.store(NONE_PROC_NR, Ordering::Relaxed);
            return;
        }

        // 遍历队列查找
        let mut current = procs[dst_idx].p_caller_q.load(Ordering::Relaxed);
        while current != NONE_PROC_NR {
            let cur_idx = nr_to_idx(current).unwrap();
            if procs[cur_idx].p_q_link.load(Ordering::Relaxed) == sender_nr {
                let sender_idx = nr_to_idx(sender_nr).unwrap();
                let next = procs[sender_idx].p_q_link.load(Ordering::Relaxed);
                procs[cur_idx].p_q_link.store(next, Ordering::Relaxed);
                procs[sender_idx].p_q_link.store(NONE_PROC_NR, Ordering::Relaxed);
                return;
            }
            current = procs[cur_idx].p_q_link.load(Ordering::Relaxed);
        }
    }

    /// 遍历队列，找到匹配源端点的发送者。
    ///
    /// C: `while (*xpp) { if (CANRECEIVE(...)) break; }` — proc.c:1077-1105
    pub fn find_matching(
        procs: &[KProcess],
        dst_nr: ProcNr,
        src_endpoint: Endpoint,
    ) -> Option<ProcNr> {
        let dst_idx = nr_to_idx(dst_nr)?;
        let mut current = procs[dst_idx].p_caller_q.load(Ordering::Relaxed);
        while current != NONE_PROC_NR {
            let cur_idx = nr_to_idx(current)?;
            // ANY 匹配所有，或端点匹配
            if src_endpoint == Endpoint::ANY
                || procs[cur_idx].p_endpoint == src_endpoint
            {
                return Some(current);
            }
            current = procs[cur_idx].p_q_link.load(Ordering::Relaxed);
        }
        None
    }
}

// ── IPC Engine ──

/// IPC 核心引擎。
///
/// 封装所有 IPC 操作，替代 C 的全局函数调用。
/// 所有方法需要 `&mut [KProcess]`（BKL 保护）。
///
/// Design decision: 结构体封装替代 C 的全局函数（11-ipc-core.md §3）。
pub struct IpcEngine;

impl IpcEngine {
    /// 死锁检测。
    ///
    /// C: `deadlock()` — proc.c:703-770
    /// 跟踪 `P_BLOCKEDON(xp)` 链，发现环则返回 `Some(DeadlockCycle)`。
    /// 对 SEND/RECEIVE 走相同逻辑——差别在每步 walk 的字段：
    /// - SEND: walk `p_sendto_e`（目标正在等谁发来）
    /// - RECEIVE: walk `p_getfrom_e`（目标正在等谁送过来）
    ///
    /// C 在 `group_size == 2` 的特例下区分 SEND ↔ RECEIVE 直接互换
    /// （这是合法的 "request/reply 互锁"），由调用方通过
    /// `IpcCall` 的状态位负责（详见 11-ipc-core.md §3 D5）。
    pub fn detect_deadlock(
        procs: &[KProcess],
        function: IpcCall,
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
    ) -> Option<DeadlockCycle> {
        // Choose the chain field and required RTS flag based on the
        // direction. Both branches share the cycle-detection core
        // (`P_BLOCKEDON` 链), only the per-step field/flag differ.
        //
        // RECEIVE deadlock parity (2026-06-13): RECEIVE 之前直接返回 `None`, 与 C 的
        // `deadlock(RECEIVE, ...)` 不对齐. 现在补齐 parity.
        let (chain_field_getter, required_flag): (
            fn(&KProcess) -> Endpoint,
            fn(&KProcess) -> bool,
        ) = match function {
            IpcCall::Send | IpcCall::SendRec | IpcCall::SendNb => (
                |p| p.p_sendto_e,
                |p| p.p_rts_flags.is_set(RtsFlagsBits::SENDING),
            ),
            IpcCall::Receive => (
                |p| p.p_getfrom_e,
                |p| p.p_rts_flags.is_set(RtsFlagsBits::RECEIVING),
            ),
            _ => return None,
        };

        let mut visited: alloc::vec::Vec<ProcNr> = alloc::vec::Vec::new();
        let mut current_ep = dst_endpoint;

        visited.push(caller_nr);

        loop {
            // 找到 current_ep 对应的进程
            let target = procs.iter().find(|p| p.p_endpoint == current_ep)?;
            let target_nr = target.p_nr;

            if visited.contains(&target_nr) {
                // 发现环
                let start = visited.iter().position(|&nr| nr == target_nr).unwrap();
                return Some(DeadlockCycle {
                    chain: visited[start..].to_vec(),
                    direction: match function {
                        IpcCall::Send | IpcCall::SendRec | IpcCall::SendNb
                            => DeadlockDirection::Send,
                        IpcCall::Receive => DeadlockDirection::Receive,
                        // Other variants are filtered at function entry; this
                        // arm is unreachable. The default keeps the match
                        // exhaustive.
                        _ => DeadlockDirection::Send,
                    },
                });
            }

            // 目标不在预期的 RTS 状态，无环
            // (SEND 链要求每步都在 SENDING; RECEIVE 链要求 RECEIVING)
            if !required_flag(target) {
                return None;
            }

            visited.push(target_nr);
            current_ep = chain_field_getter(target);
        }
    }

    /// 异步通知。
    ///
    /// C: `mini_notify()` — proc.c:1122-1196
    ///
    /// # C-Rust 签名分歧（§14 语义对齐）
    ///
    /// C 签名: `int mini_notify(const struct proc *caller_ptr, endpoint_t dst_e)`
    ///   - 返回 `EDEADSRCDST` 当目标端点无效 (proc.c:1132)
    ///   - 其余情况返回 OK（永不阻塞）
    ///
    /// Rust 签名: `fn notify(...) -> Result<(), IpcError>`
    ///   - `Err(IpcError::DeadSrcDst)` 对应 C 的 `EDEADSRCDST`
    ///   - `Ok(())` 对应 C 的 OK
    ///
    /// 设计决策：Rust 使用 `Result` 而非裸 `int` 返回值，因为：
    /// 1. 类型安全：强制调用方处理错误路径，不会忽略返回值
    /// 2. 与 Rust 错误处理惯用法一致（`?` 操作符）
    /// 3. 语义等价：错误码映射是 1:1 的（EDEADSRCDST ↔ DeadSrcDst）
    ///
    /// 当目标进程正在 RECEIVE 且匹配源时，直接投递通知消息；
    /// 否则设置 `priv(dst)->s_notify_pending` 位图，等目标下次
    /// RECEIVE 时检查（`has_pending_notify()` — proc.c:967-989）。
    ///
    /// # BKL (Big Kernel Lock)
    ///
    /// **Precondition**: the caller must hold the BKL. Same as `send()`.
    pub fn notify(
        procs: &mut [KProcess],
        priv_table: &mut PrivTable,
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
    ) -> Result<(), IpcError> {
        let caller_idx = nr_to_idx(caller_nr).ok_or(IpcError::DeadSrcDst)?;
        let dst = procs.iter().find(|p| p.p_endpoint == dst_endpoint)
            .ok_or(IpcError::DeadSrcDst)?;
        let dst_nr = dst.p_nr;
        let dst_priv_id = dst.priv_id;

        // 检查目标是否在 RECEIVE 且匹配
        let will_receive = !dst.p_rts_flags.is_set(RtsFlagsBits::SENDING)
            && dst.p_rts_flags.is_set(RtsFlagsBits::RECEIVING)
            && (dst.p_getfrom_e == Endpoint::ANY
                || dst.p_getfrom_e == procs[caller_idx].p_endpoint);

        if will_receive {
            // 直接投递通知消息
            // C: BuildNotifyMessage(&dst_ptr->p_delivermsg, src_proc_nr, caller_ptr);
            let dst_idx = nr_to_idx(dst_nr).unwrap();
            procs[dst_idx].p_delivermsg.m_source = procs[caller_idx].p_endpoint;
            procs[dst_idx].p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
            procs[dst_idx].p_rts_flags.clear(RtsFlagsBits::RECEIVING);
        } else {
            // C: priv(dst_ptr)->s_notify_pending |= (1 << priv_id(caller_ptr));
            // — proc.c:1178
            // Set the notify-pending bit in the destination's privilege structure.
            // `src_id` is the caller's privilege table index, matching C's
            // `priv_id(caller_ptr)` (not proc_nr — see proc.c:1172-1178).
            if let Some(priv_id) = dst_priv_id {
                if let Some(dst_priv) = priv_table.get_mut(priv_id) {
                    // Compute the caller's priv_id for the bitmap index.
                    // C uses `priv_id(caller_ptr)` as the bit position.
                    // In our model, the caller's priv_id is stored in KProcess.
                    let caller_priv_id = procs[caller_idx].priv_id;
                    if let Some(caller_pid) = caller_priv_id {
                        if caller_pid < 64 {
                            dst_priv.s_notify_pending |= 1u64 << caller_pid;
                        }
                    }
                }
            }
            // If dst has no priv_id (user process without privilege slot),
            // notifications are silently dropped. This matches Minix3's
            // behavior where only system processes have privilege entries
            // and user-process notifications go through PM.
        }

        Ok(())
    }

    /// 同步发送。`mini_send()` 的 Rust 等价。
    ///
    /// C: `mini_send()` — proc.c:870-952
    ///
    /// 行为:
    /// 1. 检查 `dst` 是否在 RECEIVE 且源匹配 — 若匹配则直接投递并解除阻塞。
    /// 2. 若 `NON_BLOCKING` 标志被设置且目标未就绪 — 返回 `ENOTREADY`。
    /// 3. 调用 `detect_deadlock()` 检查 SEND 环 — 若发现返回 `ELOCKED`。
    /// 4. 否则：把消息缓存到 `caller.p_sendmsg`，设置 `RTS_SENDING` + `p_sendto_e`，
    ///    解除 caller 的运行状态，dequeue from runqueue。
    ///
    /// # BKL
    /// 调用方必须持有 Big Kernel Lock（BKL）。`procs` 的 `&mut` 借用是 BKL 的类型代理。
    /// 这与 C 的"kernel 全局可访问但 BKL 保护"一致。
    ///
    /// # C-Rust 签名分歧（§14 语义对齐）
    ///
    /// C 签名: `int mini_send(struct proc *caller_ptr, endpoint_t dst_e, message *m_ptr, int flags)`
    ///   - 返回 OK 或 E* 错误码
    ///   - 调用方需把 OK 转成 `SEND` IPC 状态位（caller 继续运行）
    ///
    /// Rust 签名: `fn send(procs, caller_nr, dst_e, msg, flags) -> Result<(), IpcError>`
    ///   - `Ok(())` 含义分两种：消息已投递 OR caller 已加入等待队列。两种情况下 caller
    ///     都会在 IPC 状态位上得到合适的状态（`Ok` 对应"已投递"路径）。
    ///   - 阻塞路径（caller 已 enqueue）通过 `Err(IpcError::NotReady)` 表达；scheduler
    ///     看到此错误码会把 caller 标记为阻塞。**不**与 C 的"OK + 后续状态"路径冲突。
    ///
    /// # 阻塞 vs 立即投递
    /// 本实现是骨架（skeleton）：仅完成类型与状态位切换，**不**实现调度器集成
    /// （dequeue_runnable + 进程切换由 `proc.rs` 的 scheduler 负责，参见 RtsFlags 两层 API）。
    ///
    /// # BKL (Big Kernel Lock)
    ///
    /// **Precondition**: the caller must hold the BKL. In C, `mini_send()`
    /// is called from `do_ipc()` which runs under BKL (acquired in the
    /// assembly trap entry). The BKL is **not** released during IPC —
    /// blocking is implemented via `RTS_SENDING` flag + scheduler skip,
    /// not via BKL release. This matches C's pattern where BKL is held
    /// across the entire IPC operation.
    pub fn send(
        procs: &mut [KProcess],
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
        msg: &Message,
        flags: SendFlags,
    ) -> Result<(), IpcError> {
        // C: dst_p = _ENDPOINT_P(dst_e); dst_ptr = proc_addr(dst_p);
        let dst_idx = procs
            .iter()
            .position(|p| p.p_endpoint == dst_endpoint)
            .ok_or(IpcError::DeadSrcDst)?;
        let dst = &procs[dst_idx];

        // C: if (RTS_ISSET(dst_ptr, RTS_NO_ENDPOINT)) return EDEADSRCDST;
        if dst.p_rts_flags.is_set(RtsFlagsBits::NO_ENDPOINT) {
            return Err(IpcError::DeadSrcDst);
        }

        let caller_idx = nr_to_idx(caller_nr).ok_or(IpcError::DeadSrcDst)?;

        // C: WILLRECEIVE(caller, dst, m, NULL) — check if dst is in
        // RECEIVE state and matches caller's endpoint.
        //
        // WILLRECEIVE simplifies to: dst is RECEIVING and (getfrom is ANY
        // or matches caller's endpoint). RECEIVE parity note: virtual-address check
        // (m_ptr) is currently a no-op skeleton — m_ptr is unused in the
        // matching predicate, matching the IPC-filter-less path used in
        // this rewrite. See ipc_filter.rs for hookable extension.
        let will_receive = !dst.p_rts_flags.is_set(RtsFlagsBits::SENDING)
            && dst.p_rts_flags.is_set(RtsFlagsBits::RECEIVING)
            && (dst.p_getfrom_e == Endpoint::ANY
                || dst.p_getfrom_e == procs[caller_idx].p_endpoint);

        if will_receive {
            // C: copy_msg_from_user + p_delivermsg.m_source = caller->p_endpoint
            //    + p_misc_flags |= MF_DELIVERMSG
            //    + RTS_UNSET(dst, RTS_RECEIVING)
            //
            // Skeleton: copy is a no-op (FROM_KERNEL path); when FROM_KERNEL
            // is unset, the user-mode copy is delegated to a future
            // user-copy helper. The MSG_FROM_KERNEL flag is currently
            // unused in this rewrite (no userspace IPC filter).
            procs[dst_idx].p_delivermsg = msg.clone();
            procs[dst_idx].p_delivermsg.m_source = procs[caller_idx].p_endpoint;
            procs[dst_idx].p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
            procs[dst_idx].p_rts_flags.clear(RtsFlagsBits::RECEIVING);
            Ok(())
        } else {
            // C: if (flags & NON_BLOCKING) return ENOTREADY;
            if flags.0 & SendFlags::NON_BLOCKING.0 != 0 {
                return Err(IpcError::NotReady);
            }

            // C: if (deadlock(SEND, caller, dst_e)) return ELOCKED;
            if IpcEngine::detect_deadlock(
                procs, IpcCall::Send, caller_nr, dst_endpoint,
            ).is_some() {
                return Err(IpcError::Deadlock);
            }

            // C: copy message to caller->p_sendmsg; set RTS_SENDING;
            //    p_sendto_e = dst_e; dequeue from run queue.
            //
            // Skeleton: message cached, RTS_SENDING set, p_sendto_e set.
            // Runqueue dequeue is the caller's responsibility (the IPC
            // dispatcher knows when to call schedule()); we surface a
            // `NotReady` error to signal "caller is now blocked, the
            // scheduler should re-evaluate".
            procs[caller_idx].p_sendmsg = msg.clone();
            procs[caller_idx].p_rts_flags.set(RtsFlagsBits::SENDING);
            procs[caller_idx].p_sendto_e = dst_endpoint;
            Err(IpcError::NotReady)
        }
    }

    /// 同步接收。`mini_receive()` 的 Rust 等价。
    ///
    /// C: `mini_receive()` — proc.c:1032-1118
    ///
    /// 行为:
    /// 1. 若调用方已 `MF_REPLY_PEND`（SENDREC 的 REPLY 阶段），则直接从
    ///    `caller->p_delivermsg` 取消息，**不**等待。这是 SENDREC 的关键优化。
    /// 2. 否则：扫描发送者队列 `p_caller_q`，找到匹配 `src` 的发送者并投递。
    /// 3. 否则（无可用发送者）：缓存 `src` 到 `p_getfrom_e`，设置 `RTS_RECEIVING`，
    ///    返回 `NotReady` 表示 caller 即将阻塞。
    ///
    /// # C-Rust 签名分歧（§14 语义对齐）
    ///
    /// C: `int mini_receive(struct proc *caller_ptr, endpoint_t src_e, message *m_ptr)`
    ///   - 返回 OK 表示"消息已就绪，需 sys_datacopy 到 user"
    ///   - 阻塞路径不显式返回；通过 RTS_RECEIVING 标志+调度器隐式表达
    ///
    /// Rust: `fn receive(procs, caller_nr, src_e) -> Result<Message, IpcError>`
    ///   - `Ok(msg)` 表示消息已可直接拷贝到 user（caller 不阻塞）
    ///   - `Err(IpcError::NotReady)` 表示 caller 进入 RECEIVING，调度器将阻塞它
    ///
    /// # BKL (Big Kernel Lock)
    ///
    /// **Precondition**: the caller must hold the BKL. Same as `send()` —
    /// C's `mini_receive()` runs under BKL without release.
    pub fn receive(
        procs: &mut [KProcess],
        caller_nr: ProcNr,
        src_endpoint: Endpoint,
    ) -> Result<Message, IpcError> {
        let caller_idx = nr_to_idx(caller_nr).ok_or(IpcError::DeadSrcDst)?;
        let caller = &procs[caller_idx];

        // C: if (caller->p_misc_flags & MF_REPLY_PEND) — SENDREC REPLY shortcut.
        if caller.p_misc_flags.is_set(MiscFlagsBits::REPLY_PEND) {
            // C: caller->p_misc_flags &= ~MF_REPLY_PEND; caller is ready to run.
            procs[caller_idx].p_misc_flags.clear(MiscFlagsBits::REPLY_PEND);
            // C: caller->p_delivermsg is the reply message (caller's view
            // of p_delivermsg is read by the dispatcher after return).
            return Ok(procs[caller_idx].p_delivermsg.clone());
        }

        // C: scan p_caller_q for a matching sender.
        //
        // Skeleton: the production implementation does the queue walk
        // + dequeue + p_misc_flags |= MF_DELIVERMSG + RTS_UNSET(sender, RTS_SENDING)
        // + IPC_STATUS_ADD_CALL for the sender. For now we return
        // NotReady (caller enters RECEIVE) — this matches the
        // empty-queue case which is the most common in skeleton tests.
        let _ = src_endpoint;
        procs[caller_idx].p_getfrom_e = src_endpoint;
        procs[caller_idx].p_rts_flags.set(RtsFlagsBits::RECEIVING);
        Err(IpcError::NotReady)
    }

    /// 原子 SEND + RECEIVE。`mini_sendrec()` 的 Rust 等价。
    ///
    /// C: `mini_sendrec()` — proc.c:1135-1175 (注: minix-rs 当前实现里
    /// mini_sendrec 在 proc.c 是 SENDREC 路径，由 do_ipc 调入)
    ///
    /// 行为:
    /// 1. 先 `send(caller, dst, send_msg, NONE)`。
    /// 2. 若 send 成功投递（即 `Ok(())`），继续 `receive(caller, ANY)`。
    /// 3. 若 send 阻塞（`Err(NotReady)`），把 `MF_REPLY_PEND` 设上，让
    ///    receive 在被唤醒后走 reply shortcut 路径。
    ///
    /// # C-Rust 签名分歧（§14 语义对齐）
    ///
    /// C: `int mini_sendrec(struct proc *, endpoint, message *, message *)`
    ///   - 用指针 out 传 reply
    ///   - 返回 OK / E*
    ///
    /// Rust: `fn sendrec(procs, caller, dst, msg) -> Result<Message, IpcError>`
    ///   - `Ok(msg)` 是 reply 消息（dispatcher 把它复制到 user）
    ///   - `Err(NotReady)` 表示 caller 阻塞在 SEND 阶段，等回复来后由
    ///     `receive()` 的 `MF_REPLY_PEND` 分支处理
    ///
    /// # BKL (Big Kernel Lock)
    ///
    /// **Precondition**: the caller must hold the BKL. Same as `send()` —
    /// C's `mini_sendrec()` runs under BKL without release.
    pub fn sendrec(
        procs: &mut [KProcess],
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
        msg: &Message,
    ) -> Result<Message, IpcError> {
        let caller_idx = nr_to_idx(caller_nr).ok_or(IpcError::DeadSrcDst)?;

        // Try send first.
        let send_result = IpcEngine::send(
            procs, caller_nr, dst_endpoint, msg, SendFlags::NONE,
        );

        match send_result {
            Ok(()) => {
                // C: SEND phase delivered synchronously (dst was in
                // RECEIVE). Now do RECEIVE-ANY for the reply.
                IpcEngine::receive(procs, caller_nr, Endpoint::ANY)
            }
            Err(IpcError::NotReady) => {
                // C: caller is now blocked in SENDING. The dispatcher
                // will surface this as "caller is suspended". When the
                // reply arrives, the target's MF_DELIVERMSG and
                // RTS_UNSET(RECEIVING) path completes, and the reply
                // delivery code (notify's delivery path is similar) sets
                // MF_REPLY_PEND on the caller so the next receive()
                // returns immediately.
                //
                // Skeleton: set MF_REPLY_PEND now so the eventual reply
                // delivery will see the flag and caller's receive()
                // will take the shortcut.
                procs[caller_idx].p_misc_flags.set(MiscFlagsBits::REPLY_PEND);
                Err(IpcError::NotReady)
            }
            Err(e) => Err(e),
        }
    }

    /// 异步批量发送。`mini_senda()` 的 Rust 等价。
    ///
    /// C: `mini_senda()` — proc.c:1198-1260
    /// 一次性向多个目标（最多 1 个）发送同一消息的多个不同 slot（caller
    /// 把消息写入自己的 `p_mess` 区，指定不同 dst_slot）。
    ///
    /// # SENDA DEFERRED — IpcEngine::senda implementation path
    ///
    /// SENDA 不是一个独立的 IPC 原语,而是 `mini_send` 的批量调用
    /// 形式:对每个 (dst, msg) 对调用一次 `mini_send` 的核心路径。
    /// 由于目标端点和消息体在一次调用中可能不同,实现路径如下:
    ///
    /// 1. **遍历请求数组**:参数是一个固定大小数组(`[Option<(Endpoint, &Message)>; N]`),
    ///    顺序处理每个 (dst, msg) 对。
    /// 2. **每个 (dst, msg) 调用 `send()`**:对每个目标复用 `IpcEngine::send`
    ///    的核心路径(包括 will_receive 匹配 + deadlock 检测 + RTS_SENDING
    ///    状态切换)。
    /// 3. **聚合结果**:把所有 `Ok(())` / `Err(NotReady)` / `Err(Deadlock)` /
    ///    `Err(DeadSrcDst)` 收集到 `SenadaResult` 数组中返回。调用方按
    ///    index 区分每个目标的投递状态。
    /// 4. **BKL 共享**:整个批量调用持有一个 BKL(通过 `&mut [KProcess]`
    ///    的可变借用保证),不需要 per-target 重新加锁。
    /// 5. **错误恢复**:第一个返回 `Err(NotReady)` 的目标会阻塞 caller,
    ///    后续目标不会被处理(与 C 一致 — `mini_senda` 在第一个非 OK
    ///    返回时停止遍历)。
    ///
    /// # 当前 stub 行为
    ///
    /// 接收单个 (dst, msg) 对并委托给 `send()`,保持与 C 相同的单目标
    /// 退化行为。多目标批量调用由调用方在循环中多次调用此函数。
    /// 全批量实现需要 kernel IPC core follow-up。
    ///
    /// # 返回值
    ///
    /// `Ok(())` 表示至少一个目标投递成功,`Err(NotReady)` 表示 caller
    /// 阻塞在第一个未投递的目标上,`Err(DeadSrcDst)` 表示第一个目标
    /// 端点无效。
    pub fn senda(
        procs: &mut [KProcess],
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
        msg: &Message,
    ) -> Result<(), IpcError> {
        // SENDA DEFERRED: current stub delegates to `send()` (single-target
        // 退化). Full multi-target loop lands in kernel IPC core follow-up. See the
        // 5-step implementation path above.
        IpcEngine::send(procs, caller_nr, dst_endpoint, msg, SendFlags::NONE)
    }
}

/// Convert a logical process number to an array index.
/// Same as proc_table::nr_to_idx.
#[inline]
fn nr_to_idx(nr: ProcNr) -> Option<usize> {
    use crate::proc_table::NR_TASKS;
    let offset = nr as isize + NR_TASKS as isize;
    if offset < 0 || offset as usize >= PROC_TABLE_SIZE {
        return None;
    }
    Some(offset as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_call_variants() {
        let _ = IpcCall::Send;
        let _ = IpcCall::Receive;
        let _ = IpcCall::SendRec;
        let _ = IpcCall::Notify;
        let _ = IpcCall::SendNb;
        let _ = IpcCall::SendA;
    }

    #[test]
    fn test_ipc_error_variants() {
        let _ = IpcError::Deadlock;
        let _ = IpcError::DeadSrcDst;
        let _ = IpcError::NotReady;
        let _ = IpcError::BadCall;
        let _ = IpcError::Fault;
        let _ = IpcError::CallDenied;
        let _ = IpcError::TrapDenied;
    }

    #[test]
    fn test_send_flags() {
        assert_eq!(SendFlags::NONE, SendFlags(0));
        assert_eq!(SendFlags::NON_BLOCKING, SendFlags(0x01));
        assert_eq!(SendFlags::FROM_KERNEL, SendFlags(0x02));
    }

    #[test]
    fn test_deadlock_no_cycle() {
        // 空进程表不应有死锁
        let procs: [KProcess; 0] = [];
        let result = IpcEngine::detect_deadlock(
            &procs, IpcCall::Send, 0, Endpoint(1),
        );
        assert!(result.is_none());
    }

    // ── RECEIVE deadlock parity (2026-06-13) — RECEIVE deadlock detection parity ──
    //
    // Before the fix, `detect_deadlock(..., IpcCall::Receive, ...)`
    // returned `None` unconditionally, even when a real receive-cycle
    // existed. These tests pin the parity with C's `deadlock(RECEIVE, ...)`.
    //
    // C parity:
    //   SEND-direction:    walk `p_sendto_e` chain (test_deadlock_send_cycle)
    //   RECEIVE-direction: walk `p_getfrom_e` chain (test_deadlock_receive_cycle)
    //   SENDING state required for SEND; RECEIVING state required for RECEIVE
    //   cycle of length 2 (caller ↔ dst) is NOT a deadlock in C for
    //     SEND↔RECEIVE combinations — but `detect_deadlock` reports any
    //     cycle it finds, callers interpret group_size=2 special case.

    use crate::proc::RtsFlags;

    /// Build a 2-proc scenario: caller A, dst B. B is in SENDING or
    /// RECEIVING state and waiting on `B's chain target`.
    fn build_two_proc_scenario(
        a_ep: Endpoint,
        b_ep: Endpoint,
        b_state: RtsFlagsBits,
        b_chain_target: Endpoint,
    ) -> [KProcess; 2] {
        let mut a = KProcess::new(1, a_ep);
        let mut b = KProcess::new(2, b_ep);
        a.p_rts_flags = RtsFlags::new(); // a not in SENDING/RECEIVING
        b.p_rts_flags = RtsFlags::with(b_state);
        match b_state {
            RtsFlagsBits::SENDING => b.p_sendto_e = b_chain_target,
            RtsFlagsBits::RECEIVING => b.p_getfrom_e = b_chain_target,
            _ => panic!("test setup: b_state must be SENDING or RECEIVING"),
        }
        [a, b]
    }

    #[test]
    fn test_deadlock_send_cycle() {
        // SEND scenario: A sends to B, B is SENDING to A — classic 2-cycle.
        // C: `deadlock(SEND, A, B_ep)` returns group_size=2 → caller checks
        // for SEND↔RECEIVE compatibility and accepts or rejects.
        // Rust: returns Some(Cycle) — caller interprets.
        let procs = build_two_proc_scenario(
            Endpoint(1), Endpoint(2), RtsFlagsBits::SENDING, Endpoint(1),
        );
        let result = IpcEngine::detect_deadlock(
            &procs, IpcCall::Send, 1, Endpoint(2),
        );
        assert!(result.is_some(), "SEND cycle must be detected");
        let cycle = result.unwrap();
        // The cycle includes the caller (A=1) and the target B=2.
        assert!(cycle.chain.contains(&1));
        assert!(cycle.chain.contains(&2));
    }

    #[test]
    fn test_deadlock_receive_cycle() {
        // RECEIVE parity fix: RECEIVE scenario: A receives from B, B is RECEIVING
        // from A — classic 2-cycle in the receive direction. Before the
        // fix this returned `None` (regression vs C). After the fix it
        // returns `Some(Cycle)`.
        let procs = build_two_proc_scenario(
            Endpoint(1), Endpoint(2), RtsFlagsBits::RECEIVING, Endpoint(1),
        );
        let result = IpcEngine::detect_deadlock(
            &procs, IpcCall::Receive, 1, Endpoint(2),
        );
        assert!(
            result.is_some(),
            "RECEIVE cycle must be detected (RECEIVE parity 2026-06-13 fix)"
        );
        let cycle = result.unwrap();
        assert!(cycle.chain.contains(&1), "cycle must include caller A=1");
        assert!(cycle.chain.contains(&2), "cycle must include B=2");
    }

    #[test]
    fn test_deadlock_receive_no_cycle() {
        // RECEIVE scenario without cycle: A receives from B, B is not
        // in RECEIVING state (e.g. RUNNING). No cycle.
        // Built inline because the helper requires SENDING or RECEIVING.
        let mut a = KProcess::new(1, Endpoint(1));
        let mut b = KProcess::new(2, Endpoint(2));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::new(); // B is RUNNING, not RECEIVING
        b.p_getfrom_e = Endpoint::NONE;
        let procs = [a, b];

        let result = IpcEngine::detect_deadlock(
            &procs, IpcCall::Receive, 1, Endpoint(2),
        );
        assert!(
            result.is_none(),
            "RECEIVE without RECEIVING state on target must be no-cycle"
        );
    }

    #[test]
    fn test_deadlock_send_state_mismatch() {
        // A sends to B, but B is in RECEIVING state (not SENDING). No
        // SEND cycle (B is not waiting to send). The flag check rejects
        // B from the SEND chain.
        let procs = build_two_proc_scenario(
            Endpoint(1), Endpoint(2), RtsFlagsBits::RECEIVING, Endpoint(1),
        );
        let result = IpcEngine::detect_deadlock(
            &procs, IpcCall::Send, 1, Endpoint(2),
        );
        assert!(
            result.is_none(),
            "SEND chain must not include procs in RECEIVING state"
        );
    }

    #[test]
    fn test_deadlock_three_proc_send_cycle() {
        // 3-proc SEND cycle: A → B → C → A.
        // Verify that the chain-walking core handles multi-step cycles.
        let mut a = KProcess::new(1, Endpoint(1));
        let mut b = KProcess::new(2, Endpoint(2));
        let mut c = KProcess::new(3, Endpoint(3));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::with(RtsFlagsBits::SENDING);
        b.p_sendto_e = Endpoint(3);
        c.p_rts_flags = RtsFlags::with(RtsFlagsBits::SENDING);
        c.p_sendto_e = Endpoint(1);
        let procs = [a, b, c];

        let result = IpcEngine::detect_deadlock(
            &procs, IpcCall::Send, 1, Endpoint(2),
        );
        assert!(result.is_some(), "3-proc SEND cycle must be detected");
        let cycle = result.unwrap();
        // Cycle must include all three procs.
        assert!(cycle.chain.contains(&1));
        assert!(cycle.chain.contains(&2));
        assert!(cycle.chain.contains(&3));
    }

    #[test]
    fn test_deadlock_three_proc_receive_cycle() {
        // RECEIVE parity fix: 3-proc RECEIVE cycle: A receives from B, B receives
        // from C, C receives from A. The 3-step cycle must be detected.
        let mut a = KProcess::new(1, Endpoint(1));
        let mut b = KProcess::new(2, Endpoint(2));
        let mut c = KProcess::new(3, Endpoint(3));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
        b.p_getfrom_e = Endpoint(3);
        c.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
        c.p_getfrom_e = Endpoint(1);
        let procs = [a, b, c];

        let result = IpcEngine::detect_deadlock(
            &procs, IpcCall::Receive, 1, Endpoint(2),
        );
        assert!(
            result.is_some(),
            "3-proc RECEIVE cycle must be detected (RECEIVE parity 2026-06-13 fix)"
        );
        let cycle = result.unwrap();
        assert!(cycle.chain.contains(&1));
        assert!(cycle.chain.contains(&2));
        assert!(cycle.chain.contains(&3));
    }

    // ── notify bitmap (2026-06-14) — notify sets s_notify_pending bitmap ──

    #[test]
    fn test_notify_direct_delivery() {
        // When dst IS in RECEIVE and matches, notify directly delivers
        // the notification message without touching s_notify_pending.
        // Use kernel-task proc_nrs so nr_to_idx maps to valid indices.
        let mut pt = crate::proc_table::ProcessTable::new();
        // Set up A (IDLE, nr=-4) and B (CLOCK, nr=-3)
        {
            let a = pt.get_mut(-4).unwrap();
            a.p_rts_flags = RtsFlags::new();
        }
        {
            let b = pt.get_mut(-3).unwrap();
            b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
            b.p_getfrom_e = Endpoint::ANY;
        }
        let b_endpoint = pt.get(-3).unwrap().p_endpoint;
        let procs = pt.procs_slice_mut();

        let mut priv_table = PrivTable::new();

        let result = IpcEngine::notify(
            procs, &mut priv_table, -4, b_endpoint,
        );
        assert!(result.is_ok(), "notify should succeed");
        // B should have DELIVERMSG set and RECEIVING cleared
        let b_idx = nr_to_idx(-3).unwrap();
        assert!(procs[b_idx].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
        assert!(!procs[b_idx].p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
    }

    #[test]
    fn test_notify_pending_bitmap_with_priv() {
        // When dst is NOT in RECEIVE, notify must set the
        // s_notify_pending bit in dst's KPriv, using the caller's
        // priv_id as the bit position.
        // C: priv(dst_ptr)->s_notify_pending |= (1 << priv_id(caller_ptr));
        let mut pt = crate::proc_table::ProcessTable::new();
        {
            let a = pt.get_mut(-4).unwrap();
            a.p_rts_flags = RtsFlags::new();
            a.priv_id = Some(0); // caller's priv_id = 0
        }
        {
            let b = pt.get_mut(-3).unwrap();
            b.p_rts_flags = RtsFlags::new(); // B not RECEIVING
            b.priv_id = Some(1); // dst's priv_id = 1
        }
        let b_endpoint = pt.get(-3).unwrap().p_endpoint;
        let procs = pt.procs_slice_mut();

        let mut priv_table = PrivTable::new();
        priv_table.assign_static(-4).unwrap(); // IDLE: priv_id = 0
        priv_table.assign_static(-3).unwrap(); // CLOCK: priv_id = 1

        let result = IpcEngine::notify(
            procs, &mut priv_table, -4, b_endpoint,
        );
        assert!(result.is_ok(), "notify should succeed");

        // Check that bit 0 (caller's priv_id) is set in dst's s_notify_pending
        let dst_priv = priv_table.get(1).unwrap();
        assert_ne!(
            dst_priv.s_notify_pending & (1u64 << 0), 0,
            "s_notify_pending bit 0 should be set (caller's priv_id=0)"
        );
    }
}
