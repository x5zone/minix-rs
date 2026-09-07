//! 12/16 号初始化与 Live Update 的 handler shell（A-1 拆分）。决策与编排
//! 本体在 ready.rs/live_update.rs；此处是 run() 分派到达的处理壳。

use super::*;

impl RsServer {
    /// C: `do_upd_ready` — request.c:890-938 (main loop `RS_LU_PREPARE`
    /// arm, main.c:117): chain gate, `RS_PREPARE_DONE`, then either
    /// `end_update(result, RS_REPLY)` on failure, the next preparer walk
    /// (`start_update_prepare_next`), or `start_update`. Composed from the
    /// landed decision (`ready::do_upd_ready`) and orchestration
    /// (`UpdateState::{start_update_prepare_next,start_update,end_update}`);
    /// the prepare/update callback closures are the 19 asynsend seam.
    pub(crate) fn do_upd_ready_shell(&mut self, m: &minix_types::Message) -> Result<i32, Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        // C: request.c:897 — result = m_rs_update.result (typed decode).
        let result = minix_types::RsUpdate::decode_message(m).result;

        // C: request.c:903-910 — chain gate: a current entry must exist, its
        // slot must be the sender, and the update must not be initializing.
        // The current entry is captured up front — the walk below advances
        // `curr`, and the PREPARE_DONE mutation belongs to the entry that
        // reported (request.c:911 fires before the walk).
        let gate_curr = state.update.chain.curr().filter(|curr| {
            state
                .table
                .get(state.update.chain.get(*curr).slot)
                .pub_
                .endpoint
                == m.m_source
        });
        let gate_ok = gate_curr.is_some()
            && !state
                .update
                .flags
                .contains(live_update::RupdateFlags::INITIALIZING);

        // C: request.c:922-924 — walk to the next preparer before the
        // decision consumes the answer as `has_next`; the prepare requests
        // it issues are the 19 asynsend seam (noop, do_period convention).
        let mut noop_req = |_: &crate::service_slot::ServiceSlot, _: i32| {};
        let mut noop_vm = |_: Endpoint, _: Endpoint, _: crate::service_slot::SysFlags| {};
        let has_next = state
            .update
            .start_update_prepare_next(&mut state.table, &mut noop_req, &mut noop_vm)
            .is_some();

        let decision = crate::ready::do_upd_ready(result, gate_ok, has_next);
        if let Some(curr) = gate_curr {
            let curr_slot = state.update.chain.get(curr).slot;
            decision.mutations.apply(state.table.get_mut(curr_slot));
        }

        match decision.outcome {
            // Gate failed — request.c:910 (`return EINVAL`).
            crate::ready::UpdReadyOutcome::Unexpected => Err(Errno::EINVAL),
            crate::ready::UpdReadyOutcome::PrepareFailed { result } => {
                // request.c:917-922 — end the update; the old version keeps
                // running and is replied to (RS_REPLY).
                let ticks = self.kernel.get_ticks().unwrap_or(0);
                let mut noop_req = |_: &crate::service_slot::ServiceSlot, _: i32| {};
                let mut noop_script = |_: &mut crate::service_slot::ServiceSlot| Ok(());
                state.update.end_update(
                    &mut state.table,
                    self.kernel.as_mut(),
                    result,
                    crate::live_update::RS_REPLY,
                    ticks,
                    &mut noop_req,
                    &mut noop_script,
                );
                Ok(minix_types::EDONTREPLY)
            }
            // request.c:930-932 — the next preparer was asked; reply deferred.
            crate::ready::UpdReadyOutcome::NextPrepare => Ok(minix_types::EDONTREPLY),
            crate::ready::UpdReadyOutcome::StartUpdate => {
                // request.c:934-935 — perform the update and request each new
                // instance to initialize; the VM-update/init faces are the 19
                // seam (noop here, do_period convention).
                let mut noop_req = |_: &crate::service_slot::ServiceSlot, _: i32| {};
                let mut noop_update =
                    |_: crate::service_slot::SlotId,
                     _: crate::service_slot::SlotId,
                     _: crate::service_slot::SysFlags| Ok(());
                let mut noop_end = |_: i32| {};
                let mut noop_complete = |_: usize| Ok(());
                let mut noop_receive_vm_init = |_: Clock| 0;
                let mut noop_read_exec = |_: &mut crate::service_slot::ServiceSlot| Ok(());
                state.update.start_update(
                    &mut state.table,
                    self.kernel.as_mut(),
                    &mut noop_req,
                    &mut noop_update,
                    &mut noop_end,
                    &mut noop_complete,
                    &mut noop_receive_vm_init,
                    &mut noop_read_exec,
                )?;
                Ok(minix_types::EDONTREPLY)
            }
        }
    }

    /// C: `do_init_ready` — request.c:462-529 (the `RS_INIT` handler, 12).
    /// The decoded `result` selects the branch; every path ends `EDONTREPLY`
    /// — the service itself is unblocked by the handler's internal reply
    /// (request.c:520-522) or killed (request.c:492).
    pub(crate) fn do_init_ready(&mut self, m: &minix_types::Message) -> Result<i32, Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        // C: request.c:474-475 — `rp = rproc_ptr[who_p]` (registered source).
        let Some(id) = state.table.endpoint_slot(m.m_source) else {
            return Err(Errno::EINVAL);
        };
        // C: request.c:473 — `result = m_ptr->m_rs_init.result`. The typed
        // accessor is total; classify guarantees the RS_INIT arm, so `None`
        // is a program error shaped as EINVAL.
        let Some(result) = m.rs_init_result() else {
            return Err(Errno::EINVAL);
        };
        let updating = state
            .table
            .get(id)
            .flags
            .contains(crate::service_slot::RFlags::UPDATING);
        let decision = crate::ready::do_init_ready(
            state.table.get(id).flags,
            result,
            updating,
            state.update.num_init_ready_pending,
            self.kernel.get_ticks().unwrap_or(0),
        );
        decision.mutations.apply(state.table.get_mut(id));
        match decision.outcome {
            // C: request.c:477-483 — not initializing → EINVAL.
            crate::ready::ReadyOutcome::Unexpected => Err(Errno::EINVAL),
            crate::ready::ReadyOutcome::InitFailed { .. } => {
                // C: request.c:488-497 — crash the service (the REINCARNATE/
                // init_err mutations already applied); RS's own crash ends
                // the loop (C `exit(1)`, manager.c:395-397).
                let outcome =
                    crate::recovery::crash_service(state.table.get(id), self.kernel.as_mut())?;
                if outcome == crate::recovery::CrashOutcome::SelfTerminate {
                    return Err(Errno::EGENERIC);
                }
                Ok(minix_types::EDONTREPLY)
            }
            crate::ready::ReadyOutcome::UpdateInitDone { pending_remaining } => {
                state.update.num_init_ready_pending = pending_remaining;
                if pending_remaining == 0 {
                    // C: request.c:511-514 — end_update(OK, RS_REPLY).
                    let ticks = self.kernel.get_ticks().unwrap_or(0);
                    state.update.end_update(
                        &mut state.table,
                        self.kernel.as_mut(),
                        0, // OK
                        1, // RS_REPLY
                        ticks,
                        &mut |_s, _ps| {},
                        &mut |_s| Ok(()),
                    );
                }
                Ok(minix_types::EDONTREPLY)
            }
            crate::ready::ReadyOutcome::FreshInitDone => {
                // C: request.c:517-524 — unblock the service with the echo of
                // its own RS_INIT message (m_type = OK), then finalize.
                let _ = self.kernel.reply(m.m_source, 0, m);
                let has_prev = state.table.get(id).prev_rp.is_some();
                crate::ready::end_srv_init(state.table.get_mut(id), has_prev);
                Ok(minix_types::EDONTREPLY)
            }
        }
    }
}
