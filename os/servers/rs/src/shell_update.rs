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

        // C: request.c:917-930 — the *mutating* walk runs only after the
        // gate passed AND result == OK. The decision's `has_next` input is
        // the walk's own answer, so it is taken as a pure peek
        // ([`UpdateState::peek_next`] shares the target rule with the walk)
        // instead of running the walk eagerly — an eager walk would advance
        // `curr` and arm RS_UPDATING even on the EINVAL / PrepareFailed
        // paths C leaves untouched (R35).
        let has_next = state.update.peek_next().is_some();

        let decision = crate::ready::do_upd_ready(result, gate_ok, has_next);
        if let Some(curr) = gate_curr {
            let curr_slot = state.update.chain.get(curr).slot;
            decision.mutations.apply(state.table.get_mut(curr_slot));
        }

        match decision.outcome {
            // Gate failed — request.c:910 (`return EINVAL`); no walk, no
            // chain state change.
            crate::ready::UpdReadyOutcome::Unexpected => Err(Errno::EINVAL),
            crate::ready::UpdReadyOutcome::PrepareFailed { result } => {
                // request.c:917-922 — end the update; the old version keeps
                // running and is replied to (RS_REPLY). C never walks here.
                let ticks = self.kernel.get_ticks()?;
                let mut noop_req = |_: &crate::service_slot::ServiceSlot, _: i32| {};
                let mut noop_script = |_: &mut crate::service_slot::ServiceSlot| Ok(());
                state.update.end_update(
                    &mut state.table,
                    self.kernel.as_mut(),
                    result,
                    crate::live_update::RS_REPLY,
                    ticks,
                    &mut live_update::EndEffects {
                        request_prepare: &mut noop_req,
                        run_script: &mut noop_script,
                    },
                );
                Ok(minix_types::EDONTREPLY)
            }
            // request.c:930-932 — the walk dispatches the next preparer and
            // the reply is deferred. The peek above guarantees the walk
            // dispatches (its `None` bail precedes every mutation, so
            // `has_next == true` implies a `Some` return).
            crate::ready::UpdReadyOutcome::NextPrepare => {
                let mut noop_req = |_: &crate::service_slot::ServiceSlot, _: i32| {};
                let mut noop_vm = |_: Endpoint, _: Endpoint, _: crate::service_slot::SysFlags| {};
                let walked = state.update.start_update_prepare_next(
                    &mut state.table,
                    &mut live_update::PrepareEffects {
                        request_prepare: &mut noop_req,
                        vm_prepare: &mut noop_vm,
                    },
                );
                debug_assert!(
                    walked.is_some(),
                    "peek_next said Some; the walk must dispatch the same target"
                );
                let _ = walked;
                Ok(minix_types::EDONTREPLY)
            }
            crate::ready::UpdReadyOutcome::StartUpdate => {
                // request.c:934-935 — perform the update and request each new
                // instance to initialize; the VM-update/init faces are the 19
                // seam (noop here, do_period convention). C's walk already
                // ran and returned NULL (zero mutations — update.c:470-472),
                // so going straight to start_update is the same observable
                // sequence.
                let mut noop_req = |_: &crate::service_slot::ServiceSlot, _: i32| {};
                let mut noop_update =
                    |_: crate::service_slot::SlotId,
                     _: crate::service_slot::SlotId,
                     _: crate::service_slot::SysFlags| Ok(());
                let mut noop_end = |_: i32| {};
                let mut noop_complete = |_: usize| Ok(());
                let mut noop_receive_vm_init = |_: Clock| 0;
                state.update.start_update(
                    &mut state.table,
                    &mut live_update::StartUpdateEffects {
                        request_prepare: &mut noop_req,
                        update_service: &mut noop_update,
                        end_update: &mut noop_end,
                        complete_srv: &mut noop_complete,
                        receive_vm_init: &mut noop_receive_vm_init,
                    },
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
            self.kernel.get_ticks()?,
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
                    let ticks = self.kernel.get_ticks()?;
                    state.update.end_update(
                        &mut state.table,
                        self.kernel.as_mut(),
                        0, // OK
                        1, // RS_REPLY
                        ticks,
                        &mut live_update::EndEffects {
                            request_prepare: &mut |_s, _ps| {},
                            run_script: &mut |_s| Ok(()),
                        },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_slot::RFlags;
    use crate::testutil::{
        booted_vfs_kernel, booted_vfs_labeled, booted_with, rs_init_envelope, two_entry_chain,
    };
    use minix_types::Endpoint;

    #[test]
    fn test_do_init_ready_failure_crashes_and_records_init_err() {
        // 12 wiring: request.c:488-497 — a failed init crashes the service
        // and records `r_init_err`; the reply is suppressed (EDONTREPLY).
        let mut server = booted_with(alloc::boxed::Box::new(crate::testutil::MockKernelApi::new(
            60,
        )));
        {
            let state = server.state.as_mut().unwrap();
            state
                .table
                .set_endpoint_index(Endpoint::VFS, Some(crate::service_slot::SlotId::new(0)));
            let s = state.table.get_mut(crate::service_slot::SlotId::new(0));
            s.flags = RFlags::IN_USE | RFlags::ACTIVE | RFlags::INITIALIZING;
            s.pub_.in_use = true;
            s.pub_.endpoint = Endpoint::VFS;
            s.pid = Some(700);
        }
        let r = server.do_init_ready(&rs_init_envelope(7)).unwrap();
        assert_eq!(r, minix_types::EDONTREPLY);
        let s = server
            .state
            .as_ref()
            .unwrap()
            .table
            .get(crate::service_slot::SlotId::new(0));
        assert_eq!(s.init_err, 7, "r_init_err records the failure");
    }

    #[test]
    fn test_do_init_ready_unexpected_is_einval() {
        // 12 wiring: request.c:477-483 — an init-ready from a slot that was
        // never asked to initialize → EINVAL.
        let mut server = booted_with(alloc::boxed::Box::new(crate::testutil::MockKernelApi::new(
            60,
        )));
        let r = server.do_init_ready(&rs_init_envelope(0)).unwrap_err();
        assert_eq!(r, Errno::EINVAL);
    }

    #[test]
    fn test_do_upd_ready_shell_gates_and_updates() {
        // 16 wiring: RS_LU_PREPARE (request.c:890-938) — the chain gate
        // (sender == curr entry, not initializing, request.c:903-910), the
        // PREPARE_DONE mutation (request.c:911, R24), and the outcome
        // dispatch: single-entry chain → start_update → EDONTREPLY
        // (request.c:934-935); a wrong sender → EINVAL; a prepare failure →
        // end_update(RS_REPLY) (request.c:917-922).
        let mut server = booted_vfs_labeled(b"vfs", b"vfs");
        let m = minix_types::Message {
            m_source: Endpoint::VFS,
            m_type: minix_types::RS_LU_PREPARE,
            m_u: Default::default(),
        };

        // Gate fail: no scheduled chain at all → EINVAL (request.c:910).
        assert_eq!(server.do_upd_ready_shell(&m), Err(Errno::EINVAL));

        // Schedule a one-entry chain and walk it (the UPD_START flow does
        // this) — curr points at the VFS slot and the update is UPDATING.
        {
            let state = server.state.as_mut().unwrap();
            let id = crate::service_slot::SlotId::new(0);
            state
                .update
                .chain
                .add(crate::live_update::UpdateEntry::new(id, Endpoint::VFS));
            let mut noop_req = |_: &crate::service_slot::ServiceSlot, _: i32| {};
            let mut noop_vm = |_: Endpoint, _: Endpoint, _: crate::service_slot::SysFlags| {};
            let mut noop_abort = |_: i32| {};
            let mut noop_end = |_: i32| {};
            state
                .update
                .start_update_prepare(
                    &mut state.table,
                    true,
                    true,
                    &mut noop_abort,
                    &mut noop_end,
                    &mut crate::live_update::PrepareEffects {
                        request_prepare: &mut noop_req,
                        vm_prepare: &mut noop_vm,
                    },
                )
                .expect("prepare schedules the single entry");
            // The scheduling phase creates the new instance (C
            // `rp->r_new_rp` — update.c:631 requires it at start time).
            let replica =
                crate::service_create::clone_slot(&mut state.table, id).expect("clone the replica");
            state.table.get_mut(id).new_rp = Some(replica);
        }

        // A different sender than the curr entry → gate fail → EINVAL.
        let mut wrong = m;
        wrong.m_source = Endpoint::PM;
        assert_eq!(server.do_upd_ready_shell(&wrong), Err(Errno::EINVAL));

        // The curr service reports readiness → start_update runs, the slot
        // carries PREPARE_DONE, and the reply is deferred (request.c:934-935).
        assert_eq!(server.do_upd_ready_shell(&m), Ok(minix_types::EDONTREPLY));
        let s = server
            .state
            .as_ref()
            .unwrap()
            .table
            .get(crate::service_slot::SlotId::new(0));
        assert!(
            s.flags.contains(RFlags::PREPARE_DONE),
            "PREPARE_DONE fires before the result check (R24)"
        );

        // A prepare failure ends the update (request.c:917-922) — on a fresh
        // scheduled instance, reporting a nonzero result runs
        // end_update(RS_REPLY): the old version is replied to and keeps
        // running, so the reply is deferred (EDONTREPLY) and the update
        // leaves the UPDATING state.
        let mut server2 = booted_vfs_labeled(b"vfs", b"vfs");
        {
            let state = server2.state.as_mut().unwrap();
            let id = crate::service_slot::SlotId::new(0);
            state
                .update
                .chain
                .add(crate::live_update::UpdateEntry::new(id, Endpoint::VFS));
            let mut noop_req = |_: &crate::service_slot::ServiceSlot, _: i32| {};
            let mut noop_vm = |_: Endpoint, _: Endpoint, _: crate::service_slot::SysFlags| {};
            let mut noop_abort = |_: i32| {};
            let mut noop_end = |_: i32| {};
            state
                .update
                .start_update_prepare(
                    &mut state.table,
                    true,
                    true,
                    &mut noop_abort,
                    &mut noop_end,
                    &mut crate::live_update::PrepareEffects {
                        request_prepare: &mut noop_req,
                        vm_prepare: &mut noop_vm,
                    },
                )
                .expect("reschedule");
            let replica =
                crate::service_create::clone_slot(&mut state.table, id).expect("clone the replica");
            state.table.get_mut(id).new_rp = Some(replica);
        }
        let mut fail = m;
        fail.m_u.m_rs_update.result = 5; // prepare failure
        assert_eq!(
            server2.do_upd_ready_shell(&fail),
            Ok(minix_types::EDONTREPLY),
            "end_update defers the reply to the old instance"
        );
        assert!(
            !server2
                .state
                .as_ref()
                .unwrap()
                .update
                .flags
                .contains(crate::live_update::RupdateFlags::UPDATING),
            "the failed update is no longer updating"
        );
    }

    #[test]
    fn test_do_upd_ready_gate_failure_leaves_chain_untouched() {
        // R35 negative: a gate rejection must not advance the chain. C
        // returns EINVAL before touching rupdate state (request.c:903-910);
        // an eager walk here would move `curr` to the second entry and arm
        // RS_UPDATING with no cleanup, letting a later forged report pass
        // the gate against the wrong service.
        let mut server = booted_vfs_labeled(b"vfs", b"vfs");
        two_entry_chain(&mut server);
        let before = {
            let state = server.state.as_ref().unwrap();
            (state.update.chain.curr(), state.update.flags)
        };

        // Wrong sender: curr expects VFS, PM reports → EINVAL.
        let mut wrong = minix_types::Message {
            m_source: Endpoint::PM,
            m_type: minix_types::RS_LU_PREPARE,
            m_u: Default::default(),
        };
        assert_eq!(server.do_upd_ready_shell(&wrong), Err(Errno::EINVAL));
        wrong.m_source = Endpoint::VFS;
        // Right sender but the update is initializing → also gated.
        server
            .state
            .as_mut()
            .unwrap()
            .update
            .flags
            .insert(crate::live_update::RupdateFlags::INITIALIZING);
        assert_eq!(server.do_upd_ready_shell(&wrong), Err(Errno::EINVAL));

        let state = server.state.as_ref().unwrap();
        assert_eq!(
            state.update.chain.curr(),
            before.0,
            "gate failures leave curr pointing at the reporting entry"
        );
        assert!(
            state
                .update
                .flags
                .contains(crate::live_update::RupdateFlags::INITIALIZING),
            "the gate flag survives: no end_update ran to clear it"
        );
        let after_flags = state.update.flags & !crate::live_update::RupdateFlags::INITIALIZING;
        assert_eq!(
            after_flags, before.1,
            "no phase flag changed beyond the test's own INITIALIZING write"
        );
    }

    #[test]
    fn test_do_upd_ready_walks_two_entry_chain_to_start_update() {
        // R35: the NextPrepare arm runs the real walk after the decision —
        // `curr` advances to the second entry (which receives the prepare
        // request via the 19 seam), and the second report drives start_update.
        let mut server = booted_vfs_labeled(b"vfs", b"vfs");
        two_entry_chain(&mut server);
        let mut ready_vfs = minix_types::Message {
            m_source: Endpoint::VFS,
            m_type: minix_types::RS_LU_PREPARE,
            m_u: Default::default(),
        };

        // First report → walk dispatches entry 1, reply deferred.
        assert_eq!(
            server.do_upd_ready_shell(&ready_vfs),
            Ok(minix_types::EDONTREPLY)
        );
        {
            let state = server.state.as_ref().unwrap();
            assert_eq!(state.update.chain.curr(), Some(1), "the walk advanced");
            let vfs_slot = state.update.chain.get(0).slot;
            assert!(
                state
                    .table
                    .get(vfs_slot)
                    .flags
                    .contains(RFlags::PREPARE_DONE),
                "PREPARE_DONE belongs to the reporter (request.c:911)"
            );
        }

        // Second report → exhausted chain (peek None) → start_update.
        ready_vfs.m_source = Endpoint::PM;
        assert_eq!(
            server.do_upd_ready_shell(&ready_vfs),
            Ok(minix_types::EDONTREPLY)
        );
        let state = server.state.as_ref().unwrap();
        let pm_slot = state.update.chain.get(1).slot;
        assert!(
            state
                .table
                .get(pm_slot)
                .flags
                .contains(RFlags::PREPARE_DONE),
            "the second reporter carries PREPARE_DONE too"
        );
    }

    #[test]
    fn test_clock_seam_failure_propagates_instead_of_poisoning() {
        // R37 policy lock: get_ticks is a kernel seam that CAN fail (C's
        // getticks cannot — it reads the kerninfo page, getuptime.c:9-23).
        // Handlers propagate the failure (`?`) instead of degrading
        // timestamps to 0: poisoned prepare_tm/alive_tm would fake timeouts
        // and crash healthy services. On the PrepareFailed rollback arm the
        // propagation leaves the update armed — convergence is the
        // do_period watchdog's job (R36 arm), which still sees intact
        // prepare_tm/maxtime.
        let mut mock = crate::testutil::MockKernelApi::new(60);
        mock.fail_calls = alloc::vec![crate::testutil::Call::GetTicks];
        let mut server = booted_vfs_kernel(alloc::boxed::Box::new(mock), b"vfs");
        two_entry_chain(&mut server);
        let mut fail = minix_types::Message {
            m_source: Endpoint::VFS,
            m_type: minix_types::RS_LU_PREPARE,
            m_u: Default::default(),
        };
        fail.m_u.m_rs_update.result = 5; // prepare failure → rollback arm

        // The clock read fails → the handler surfaces the seam error
        // instead of rolling back with tick-0 poisoned timestamps.
        assert!(
            server.do_upd_ready_shell(&fail).is_err(),
            "clock seam failure must propagate, not degrade to tick 0"
        );
        let state = server.state.as_ref().unwrap();
        assert_eq!(
            state.update.chain.len(),
            2,
            "rollback deferred: the update stays armed for the do_period watchdog"
        );
    }
}
