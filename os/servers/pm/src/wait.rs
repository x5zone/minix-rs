//! Wait slice.
//!
//! Implements core logic of wait/waitpid system calls.

use minix_types::Endpoint;

/// Waits for child process.
///
/// # Arguments
/// * `parent` - Parent process endpoint
/// * `pid` - Target child PID, -1 for any child
/// * `options` - Wait options
///
/// # Returns
/// * `Ok((pid, status))` - Child PID and exit status
pub fn sys_wait(parent: Endpoint, pid: i32, options: u32) -> Result<(i32, i32), WaitError> {
    // TODO: Implement wait logic
    // 1. Find matching child process
    // 2. If zombie child exists, return immediately
    // 3. Otherwise block parent, wait for child to exit
    todo!("wait implementation")
}

#[derive(Debug)]
pub enum WaitError {
    NoChild,
    InvalidEndpoint,
    Interrupted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wait_basic() {
        // TODO: Basic wait test
    }
}
