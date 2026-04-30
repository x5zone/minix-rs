//! Message dispatcher for VFS service.
//!
//! Routes incoming IPC messages to appropriate handlers.

use minix_types::{VfsRequest, VfsResponse, VfsError, Endpoint, UserSlot};
use crate::fproc::{FProcTable, FProc, PID_FREE};

/// Dispatches VFS requests to appropriate handlers.
///
/// This is the central routing point for all VFS IPC messages.
pub struct MessageDispatcher;

impl MessageDispatcher {
    /// Dispatches a VFS request to the appropriate handler.
    ///
    /// # Arguments
    /// * `table` - Reference to the VFS process table
    /// * `request` - The incoming VFS request
    ///
    /// # Returns
    /// The response to send back to the caller.
    pub fn dispatch(table: &mut FProcTable, request: VfsRequest) -> VfsResponse {
        match request {
            VfsRequest::Fork { parent_endpoint, child_endpoint } => {
                Self::handle_fork_request(table, parent_endpoint, child_endpoint)
            }
        }
    }

    /// Handles fork request from PM.
    fn handle_fork_request(
        table: &mut FProcTable,
        parent_endpoint: Endpoint,
        child_endpoint: Endpoint,
    ) -> VfsResponse {
        match handle_fork(table, parent_endpoint, child_endpoint) {
            Ok(()) => VfsResponse::ForkOk,
            Err(e) => VfsResponse::Error(e),
        }
    }
}

/// Handles fork - copies parent's file descriptor table to child.
///
/// This is called by PM during fork to duplicate the parent's
/// file descriptors and directory pointers.
///
/// # Arguments
/// * `table` - VFS process table
/// * `parent_endpoint` - Parent process endpoint
/// * `child_endpoint` - Child process endpoint
///
/// # Returns
/// * `Ok(())` - Success
/// * `Err(e)` - Error
pub fn handle_fork(
    table: &mut FProcTable,
    parent_endpoint: Endpoint,
    child_endpoint: Endpoint,
) -> Result<(), VfsError> {
    // 1. Find parent process
    let parent_slot = parent_endpoint.to_user_slot()
        .ok_or(VfsError::InvalidEndpoint)?;

    let parent = table.get(parent_slot)
        .ok_or(VfsError::InvalidEndpoint)?;

    if parent.pid == PID_FREE {
        return Err(VfsError::InvalidEndpoint);
    }

    // 2. Get child slot
    let child_slot = child_endpoint.to_user_slot()
        .ok_or(VfsError::InvalidEndpoint)?;

    // 3. Copy fproc fields
    copy_fproc(table, parent_slot, child_slot, child_endpoint);

    Ok(())
}

/// Copies parent's fproc fields to child.
fn copy_fproc(
    table: &mut FProcTable,
    parent_slot: UserSlot,
    child_slot: UserSlot,
    child_endpoint: Endpoint,
) {
    // Extract values from parent first to avoid borrow conflict
    let pid;
    let root_dir;
    let work_dir;
    let filps;
    let cloexec_set;
    let real_uid;
    let eff_uid;
    let real_gid;
    let eff_gid;
    let ngroups;
    let supplemental_groups;
    let umask;
    let name;

    {
        let parent = table.get(parent_slot).unwrap();
        pid = parent.pid;
        root_dir = parent.root_dir;
        work_dir = parent.work_dir;
        filps = parent.filps;
        cloexec_set = parent.cloexec_set;
        real_uid = parent.real_uid;
        eff_uid = parent.eff_uid;
        real_gid = parent.real_gid;
        eff_gid = parent.eff_gid;
        ngroups = parent.ngroups;
        supplemental_groups = parent.supplemental_groups;
        umask = parent.umask;
        name = parent.name;
    }

    // Now modify child
    let child = table.get_mut(child_slot).unwrap();

    // Set endpoint
    child.endpoint = child_endpoint;

    // Note: PID is set by PM, not VFS. We keep the parent's PID temporarily.
    // The actual child PID will be set by PM later.
    child.pid = pid;

    // Copy directory pointers (shared, reference counts increased elsewhere)
    child.root_dir = root_dir;
    child.work_dir = work_dir;

    // Copy file descriptor table (shared, reference counts increased elsewhere)
    child.filps = filps;

    // Copy FD_CLOEXEC bitmap
    child.cloexec_set = cloexec_set;

    // Copy credentials
    child.real_uid = real_uid;
    child.eff_uid = eff_uid;
    child.real_gid = real_gid;
    child.eff_gid = eff_gid;
    child.ngroups = ngroups;
    child.supplemental_groups = supplemental_groups;

    // Copy umask
    child.umask = umask;

    // Copy process name with suffix
    let mut child_name = name;
    // Append "_c" to name if there's room
    let current_len = child_name.iter().position(|&b| b == 0).unwrap_or(name.len());
    if current_len + 2 < name.len() {
        child_name[current_len] = b'_';
        child_name[current_len + 1] = b'c';
    }
    child.name = child_name;

    // Clear blocking state
    child.blocked_on = crate::fproc::BlockedOn::None;
    child.flags = crate::fproc::FpFlags::NOFLAGS;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fproc::FpFlags;

    fn create_test_table_with_parent() -> FProcTable {
        let mut table = FProcTable::new();

        // Initialize a parent process
        let parent_slot = UserSlot::new(0);
        let parent = table.get_mut(parent_slot).unwrap();
        parent.pid = 100;
        parent.endpoint = Endpoint::from_generation_slot(1, 0);
        parent.real_uid = 1000;
        parent.eff_uid = 1000;
        parent.real_gid = 1000;
        parent.eff_gid = 1000;
        parent.umask = 0o022;
        parent.name = *b"parent\0\0\0\0\0\0\0\0\0\0";

        table
    }

    #[test]
    fn test_handle_fork_success() {
        let mut table = create_test_table_with_parent();

        let result = handle_fork(
            &mut table,
            Endpoint::from_generation_slot(1, 0),
            Endpoint::from_generation_slot(1, 1),
        );

        assert!(result.is_ok());

        // Verify child was created
        let child = table.get(UserSlot::new(1)).unwrap();
        assert_eq!(child.endpoint, Endpoint::from_generation_slot(1, 1));
        assert_eq!(child.real_uid, 1000);
        assert_eq!(child.umask, 0o022);
    }

    #[test]
    fn test_handle_fork_parent_not_found() {
        let mut table = FProcTable::new();

        let result = handle_fork(
            &mut table,
            Endpoint::from_generation_slot(1, 0),
            Endpoint::from_generation_slot(1, 1),
        );

        assert!(matches!(result, Err(VfsError::InvalidEndpoint)));
    }

    #[test]
    fn test_dispatch_fork_success() {
        let mut table = create_test_table_with_parent();

        let request = VfsRequest::Fork {
            parent_endpoint: Endpoint::from_generation_slot(1, 0),
            child_endpoint: Endpoint::from_generation_slot(1, 1),
        };

        let response = MessageDispatcher::dispatch(&mut table, request);

        assert!(matches!(response, VfsResponse::ForkOk));
    }

    #[test]
    fn test_copy_fproc_preserves_credentials() {
        let mut table = create_test_table_with_parent();

        // Set some credentials on parent
        let parent = table.get_mut(UserSlot::new(0)).unwrap();
        parent.real_uid = 1001;
        parent.eff_uid = 0;  // root
        parent.ngroups = 2;
        parent.supplemental_groups[0] = 100;
        parent.supplemental_groups[1] = 200;

        copy_fproc(
            &mut table,
            UserSlot::new(0),
            UserSlot::new(1),
            Endpoint::from_generation_slot(1, 1),
        );

        let child = table.get(UserSlot::new(1)).unwrap();
        assert_eq!(child.real_uid, 1001);
        assert_eq!(child.eff_uid, 0);
        assert_eq!(child.ngroups, 2);
        assert_eq!(child.supplemental_groups[0], 100);
        assert_eq!(child.supplemental_groups[1], 200);
    }
}
