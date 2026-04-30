//! Endpoint identifier type definitions.
//!
//! Provides process endpoint (Endpoint) type for process identification in IPC.
//!
//! # Slot Type Distinction
//!
//! - `UserSlot`: Server-local process table index (0 ~ NR_PROCS-1), used to access mproc/fproc/vmproc.
//! - `KernelSlot`: Kernel process table index (0 ~ NR_TASKS+NR_PROCS-1), used to access kernel proc table.
//!
//! Note: Kernel tasks (negative slot) are not in mproc/fproc/vmproc, can only be accessed via KernelSlot.

pub use super::com::MAX_NR_TASKS;

/// Endpoint generation shift bits.
///
/// Corresponds to Minix3's `_ENDPOINT_GENERATION_SHIFT`.
pub const ENDPOINT_GENERATION_SHIFT: i32 = 15;

/// Endpoint generation size.
///
/// Corresponds to Minix3's `_ENDPOINT_GENERATION_SIZE`.
pub const ENDPOINT_GENERATION_SIZE: i32 = 1 << ENDPOINT_GENERATION_SHIFT;

/// Endpoint slot upper limit.
///
/// Corresponds to Minix3's `_ENDPOINT_SLOT_TOP`.
pub const ENDPOINT_SLOT_TOP: i32 = ENDPOINT_GENERATION_SIZE - (MAX_NR_TASKS as i32);

/// Endpoint identifier.
///
/// Unique identifier for processes in Minix3, used for IPC communication.
/// Composed of two parts: process slot number and generation.
///
/// # Structure
/// - Lower 15 bits: Process slot number (0 ~ NR_PROCS-1 for user processes, negative for kernel tasks).
/// - Higher bits: Generation, incremented each time a slot is reused.
///
/// # Special Endpoints
/// - `NONE`: Invalid endpoint.
/// - `ANY`: Any process.
/// - `SELF`: Self process.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Endpoint(pub i32);

impl Endpoint {
    // Special endpoints
    pub const NONE: Endpoint = Endpoint(ENDPOINT_SLOT_TOP - 2); // Invalid endpoint
    pub const ANY: Endpoint = Endpoint(ENDPOINT_SLOT_TOP - 1);  // Any process
    pub const SELF: Endpoint = Endpoint(ENDPOINT_SLOT_TOP - 3); // Self process

    // Kernel tasks (-5 ~ -1)
    pub const ASYNCM: Endpoint = Endpoint(-5);   // Async message notification
    pub const IDLE: Endpoint = Endpoint(-4);     // Idle task
    pub const CLOCK: Endpoint = Endpoint(-3);    // Clock task
    pub const SYSTEM: Endpoint = Endpoint(-2);   // System task
    pub const KERNEL: Endpoint = Endpoint(-1);   // Kernel/hardware interrupt
    pub const HARDWARE: Endpoint = Self::KERNEL; // Hardware interrupt alias

    // User space processes (0 ~ 11)
    pub const PM: Endpoint = Endpoint(0);    // Process manager
    pub const VFS: Endpoint = Endpoint(1);   // Virtual file system
    pub const RS: Endpoint = Endpoint(2);    // Restart server
    pub const MEM: Endpoint = Endpoint(3);   // Memory driver
    pub const SCHED: Endpoint = Endpoint(4); // Scheduler
    pub const TTY: Endpoint = Endpoint(5);   // TTY driver
    pub const DS: Endpoint = Endpoint(6);    // Data store service
    pub const MIB: Endpoint = Endpoint(7);   // Management information base service
    pub const VM: Endpoint = Endpoint(8);    // Virtual memory manager
    pub const PFS: Endpoint = Endpoint(9);   // Pipe file system
    pub const MFS: Endpoint = Endpoint(10);  // Minix root file system
    pub const INIT: Endpoint = Endpoint(11); // Init process

    /// Gets the raw value (for debugging).
    #[inline(always)]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// Constructs endpoint from generation and slot (`_ENDPOINT(g, p)`).
    #[inline(always)]
    pub const fn from_generation_slot(generation: i32, slot: i32) -> Self {
        Self((generation << ENDPOINT_GENERATION_SHIFT) + slot)
    }

    /// Extracts slot number (`_ENDPOINT_P(e)`).
    #[inline(always)]
    pub const fn slot(self) -> i32 {
        ((self.0 + MAX_NR_TASKS as i32) & (ENDPOINT_GENERATION_SIZE - 1)) - MAX_NR_TASKS as i32
    }

    /// Extracts generation (`_ENDPOINT_G(e)`).
    #[inline(always)]
    pub const fn generation(self) -> i32 {
        (self.0 + MAX_NR_TASKS as i32) >> ENDPOINT_GENERATION_SHIFT
    }

    /// Checks if this is NONE.
    #[inline(always)]
    pub const fn is_none(self) -> bool {
        self.0 == Self::NONE.0
    }

    /// Checks if this is ANY.
    #[inline(always)]
    pub const fn is_any(self) -> bool {
        self.0 == Self::ANY.0
    }

    /// Checks if this is SELF.
    #[inline(always)]
    pub const fn is_self(self) -> bool {
        self.0 == Self::SELF.0
    }

    /// Checks if valid (not NONE/ANY/SELF).
    #[inline(always)]
    pub const fn is_valid(self) -> bool {
        self.0 != Self::NONE.0 && self.0 != Self::ANY.0 && self.0 != Self::SELF.0
    }

    /// Checks if this is a kernel task (slot is negative).
    #[inline(always)]
    pub const fn is_kernel_task(self) -> bool {
        self.slot() < 0
    }

    /// Checks if this is a user process (slot >= 0).
    #[inline(always)]
    pub const fn is_user_proc(self) -> bool {
        self.slot() >= 0
    }

    /// Converts to UserSlot (user process).
    #[inline(always)]
    pub const fn to_user_slot(self) -> Option<UserSlot> {
        if self.is_user_proc() {
            Some(UserSlot(self.slot() as usize))
        } else {
            None
        }
    }

    /// Converts to KernelSlot.
    #[inline(always)]
    pub const fn to_kernel_slot(self) -> KernelSlot {
        // Kernel task: slot is negative, position is MAX_NR_TASKS + slot (e.g. -1 -> MAX_NR_TASKS-1)
        // User process: slot is positive, position is MAX_NR_TASKS + slot
        KernelSlot((MAX_NR_TASKS as i32 + self.slot()) as usize)
    }

    /// Maximum endpoint generation value.
    ///
    /// Corresponds to Minix3's `_ENDPOINT_MAX_GENERATION = INT_MAX / _ENDPOINT_GENERATION_SIZE - 1`.
    pub const ENDPOINT_MAX_GENERATION: i32 = i32::MAX / ENDPOINT_GENERATION_SIZE - 1;

    /// Generates a new endpoint for child process during fork.
    ///
    /// Corresponds to the generation increment and endpoint generation logic in Minix3 do_fork.c:
    /// ```c
    /// gen = _ENDPOINT_G(rpc->p_endpoint);
    /// if(++gen >= _ENDPOINT_MAX_GENERATION) gen = 1;
    /// rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr);
    /// ```
    ///
    /// Extracts generation from the child slot's current endpoint, increments it,
    /// and combines with child process number to form a new endpoint.
    /// Generation wraps to 1 when exceeding max (0 is not used to avoid conflicts with hardcoded endpoints).
    ///
    /// # Parameters
    /// - `current_endpoint`: Current endpoint of the child slot (for extracting old generation).
    /// - `child_slot`: Child process number.
    ///
    /// # Returns
    /// New endpoint with generation incremented from old generation.
    #[inline]
    pub const fn fork_new_endpoint(current_endpoint: Endpoint, child_slot: i32) -> Endpoint {
        let mut generation = current_endpoint.generation();
        generation += 1;
        if generation >= Self::ENDPOINT_MAX_GENERATION {
            generation = 1;
        }
        Endpoint::from_generation_slot(generation, child_slot)
    }
}

impl Default for Endpoint {
    fn default() -> Self {
        Self::NONE
    }
}

/// User process slot index (0 ~ NR_PROCS-1).
/// Used to access mproc/fproc/vmproc, **does not include kernel tasks**.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UserSlot(pub usize);

impl UserSlot {
    #[inline(always)]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    #[inline(always)]
    pub const fn get(self) -> usize {
        self.0
    }

    /// Checks if the endpoint's slot part matches this user slot.
    ///
    /// Returns `false` for kernel tasks (negative slot numbers) as they
    /// don't have a corresponding UserSlot.
    ///
    /// # Examples
    /// ```
    /// use minix_types::{Endpoint, UserSlot};
    ///
    /// let slot = UserSlot::new(5);
    /// let endpoint = Endpoint::from_generation_slot(1, 5);
    ///
    /// assert!(slot.matches(endpoint));
    /// assert!(!UserSlot::new(3).matches(endpoint));
    ///
    /// // Kernel tasks don't match any UserSlot
    /// assert!(!slot.matches(Endpoint::KERNEL));
    /// ```
    pub fn matches(self, endpoint: Endpoint) -> bool {
        if !endpoint.is_valid() {
            return false;
        }
        let ep_slot = endpoint.slot();
        // Kernel tasks have negative slot numbers, they don't match UserSlot
        if ep_slot < 0 {
            return false;
        }
        ep_slot as usize == self.get()
    }
}

/// Kernel process table slot index (0 ~ NR_TASKS+NR_PROCS-1).
/// Used to access kernel proc_tab: 0~NR_TASKS-1 are kernel tasks, NR_TASKS~ are user processes.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KernelSlot(pub usize);

impl KernelSlot {
    #[inline(always)]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    #[inline(always)]
    pub const fn get(self) -> usize {
        self.0
    }

    /// Converts from UserSlot.
    #[inline(always)]
    pub const fn from_user_slot(user_slot: UserSlot) -> Self {
        Self(user_slot.0 + MAX_NR_TASKS as usize)
    }

    /// Checks if the endpoint's slot part matches this kernel slot.
    ///
    /// KernelSlot is the index into the kernel's process table (proc_tab).
    /// It includes both kernel tasks (0 ~ MAX_NR_TASKS-1) and user processes (MAX_NR_TASKS ~).
    ///
    /// # Mapping
    /// - Kernel tasks: negative endpoint slot -> KernelSlot = MAX_NR_TASKS + slot
    ///   - e.g., KERNEL = -1 -> KernelSlot = MAX_NR_TASKS - 1
    /// - User processes: non-negative endpoint slot -> KernelSlot = MAX_NR_TASKS + slot
    ///   - e.g., PM = 0 -> KernelSlot = MAX_NR_TASKS + 0 = MAX_NR_TASKS
    ///
    /// # Examples
    /// ```
    /// use minix_types::{Endpoint, KernelSlot, UserSlot};
    ///
    /// // Kernel task: KERNEL has endpoint slot -1
    /// // In kernel's proc_tab, it's at index MAX_NR_TASKS - 1
    /// let kernel_ep = Endpoint::KERNEL;
    /// assert!(KernelSlot::new(MAX_NR_TASKS - 1).matches(kernel_ep));
    ///
    /// // User process: PM has endpoint slot 0
    /// // In kernel's proc_tab, it's at index MAX_NR_TASKS (after all kernel tasks)
    /// let user_ep = Endpoint::PM;
    /// let pm_kernel_slot = KernelSlot::from_user_slot(UserSlot::new(0));
    /// assert!(pm_kernel_slot.matches(user_ep));
    /// ```
    pub fn matches(self, endpoint: Endpoint) -> bool {
        if !endpoint.is_valid() {
            return false;
        }
        // Use the same logic as to_kernel_slot()
        let kernel_idx = (MAX_NR_TASKS as i32 + endpoint.slot()) as usize;
        kernel_idx == self.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_slot_extraction() {
        // 测试槽位提取：endpoint = (generation << 15) + slot
        // 当 generation = 0 时，endpoint = slot
        let ep = Endpoint::from_generation_slot(0, 5);
        assert_eq!(ep.slot(), 5);
        assert_eq!(ep.generation(), 0);

        // 测试带 generation 的情况
        let ep = Endpoint::from_generation_slot(1, 5);
        assert_eq!(ep.slot(), 5);
        assert_eq!(ep.generation(), 1);
    }

    #[test]
    fn test_endpoint_special_values() {
        assert!(Endpoint::NONE.is_none());
        assert!(Endpoint::ANY.is_any());
        assert!(Endpoint::SELF.is_self());

        assert!(!Endpoint::NONE.is_valid());
        assert!(!Endpoint::ANY.is_valid());
        assert!(!Endpoint::SELF.is_valid());

        assert!(Endpoint::PM.is_valid());
    }

    #[test]
    fn test_endpoint_kernel_task() {
        // 内核任务的槽位号为负
        let kernel_ep = Endpoint(-5); // 槽位号 -5
        assert!(kernel_ep.is_kernel_task());
        assert!(!kernel_ep.is_user_proc());
    }

    #[test]
    fn test_endpoint_equality() {
        let e1 = Endpoint::from_generation_slot(1, 100);
        let e2 = Endpoint::from_generation_slot(1, 100);
        let e3 = Endpoint::from_generation_slot(2, 100);

        assert_eq!(e1, e2);
        assert_ne!(e1, e3);
    }

    #[test]
    fn test_negative_slot() {
        // 内核任务使用负数槽位
        let endpoint = Endpoint::from_generation_slot(1, -1);
        assert_eq!(endpoint.slot(), -1);
        assert_eq!(endpoint.generation(), 1);
    }

    #[test]
    fn test_user_slot() {
        let idx = UserSlot::new(5);
        assert_eq!(idx.get(), 5);
    }

    #[test]
    fn test_kernel_slot() {
        let idx = KernelSlot::new(10);
        assert_eq!(idx.get(), 10);
    }

    #[test]
    fn test_kernel_slot_from_user() {
        let user = UserSlot::new(5);
        let kernel = KernelSlot::from_user_slot(user);
        assert_eq!(kernel.get(), 5 + MAX_NR_TASKS as usize);
    }

    #[test]
    fn test_endpoint_to_user_slot() {
        // 用户进程的 endpoint 可以转换为 UserSlot
        let ep = Endpoint::from_generation_slot(1, 5);
        assert_eq!(ep.to_user_slot(), Some(UserSlot::new(5)));

        // 内核任务的 endpoint 不能转换为 UserSlot
        let kernel_ep = Endpoint::from_generation_slot(0, -1);
        assert_eq!(kernel_ep.to_user_slot(), None);
    }

    #[test]
    fn test_endpoint_to_kernel_slot() {
        // 用户进程的内核槽位
        let ep = Endpoint::from_generation_slot(1, 5);
        assert_eq!(ep.to_kernel_slot().get(), 5 + MAX_NR_TASKS as usize);

        // 内核任务的内核槽位
        let kernel_ep = Endpoint::from_generation_slot(0, -1);
        assert_eq!(kernel_ep.to_kernel_slot().get(), (MAX_NR_TASKS - 1) as usize);
    }

    #[test]
    fn test_fork_new_endpoint_increment() {
        let current = Endpoint::from_generation_slot(5, 10);
        let new = Endpoint::fork_new_endpoint(current, 10);
        assert_eq!(new.generation(), 6);
        assert_eq!(new.slot(), 10);
    }

    #[test]
    fn test_fork_new_endpoint_wraparound() {
        let max_gen = Endpoint::ENDPOINT_MAX_GENERATION;
        let current = Endpoint::from_generation_slot(max_gen, 10);
        let new = Endpoint::fork_new_endpoint(current, 10);
        assert_eq!(new.generation(), 1);
        assert_eq!(new.slot(), 10);
    }

    #[test]
    fn test_fork_new_endpoint_different_slot() {
        let current = Endpoint::from_generation_slot(3, 5);
        let new = Endpoint::fork_new_endpoint(current, 20);
        assert_eq!(new.generation(), 4);
        assert_eq!(new.slot(), 20);
    }

    #[test]
    fn test_fork_new_endpoint_max_generation_value() {
        assert_eq!(Endpoint::ENDPOINT_MAX_GENERATION, 65534);
    }

    #[test]
    fn test_user_slot_matches_matching() {
        let slot = UserSlot::new(5);
        let endpoint = Endpoint::from_generation_slot(1, 5);

        assert!(slot.matches(endpoint));
    }

    #[test]
    fn test_user_slot_matches_mismatch() {
        let slot = UserSlot::new(5);
        let endpoint = Endpoint::from_generation_slot(1, 3);

        assert!(!slot.matches(endpoint));
    }

    #[test]
    fn test_user_slot_matches_invalid_endpoint() {
        assert!(!UserSlot::new(0).matches(Endpoint::NONE));
        assert!(!UserSlot::new(0).matches(Endpoint::ANY));
        assert!(!UserSlot::new(0).matches(Endpoint::SELF));
    }

    #[test]
    fn test_user_slot_matches_with_generation() {
        let slot = UserSlot::new(10);
        let endpoint = Endpoint::from_generation_slot(5, 10);

        assert!(slot.matches(endpoint));
    }
}
