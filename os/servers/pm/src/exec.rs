//! Exec slice.
//!
//! Implements core logic of exec system call.

use minix_types::Endpoint;

/// Executes a new program.
///
/// # Arguments
/// * `proc` - Process endpoint
/// * `path` - Program path
/// * `argv` - Arguments
/// * `envp` - Environment variables
pub fn sys_exec(
    proc: Endpoint,
    path: &str,
    argv: &[&str],
    envp: &[&str],
) -> Result<(), ExecError> {
    // TODO: Implement exec logic
    // 1. Validate path
    // 2. Read ELF file (via VFS)
    // 3. Release old address space
    // 4. Load new address space (via VM)
    // 5. Set entry point
    todo!("exec implementation")
}

#[derive(Debug)]
pub enum ExecError {
    NoEnt,
    NoExec,
    NoMem,
    TooBig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exec_basic() {
        // TODO: Basic exec test
    }
}
