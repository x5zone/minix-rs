//! Signal slice.
//!
//! Implements core logic of signal system calls.

use minix_ipc::Endpoint;

/// Sends a signal.
///
/// # Arguments
/// * `target` - Target process endpoint
/// * `sig` - Signal number
pub fn sys_kill(target: Endpoint, sig: i32) -> Result<(), SignalError> {
    // TODO: Implement kill logic
    // 1. Validate permissions
    // 2. Add signal to target process's pending set
    // 3. If target process is sleeping, wake it up
    todo!("kill implementation")
}

/// Sets signal handler.
///
/// # Arguments
/// * `proc` - Process endpoint
/// * `sig` - Signal number
/// * `handler` - Handler function
pub fn sys_sigaction(
    proc: Endpoint,
    sig: i32,
    handler: SignalHandler,
) -> Result<(), SignalError> {
    // TODO: Implement sigaction logic
    todo!("sigaction implementation")
}

pub enum SignalHandler {
    Default,
    Ignore,
    Custom(usize),
}

#[derive(Debug)]
pub enum SignalError {
    InvalidSignal,
    InvalidEndpoint,
    NoPerm,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_basic() {
        // TODO: Basic signal test
    }
}
