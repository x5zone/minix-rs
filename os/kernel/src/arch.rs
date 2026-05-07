//! Architecture-specific code (mock).
//!
//! Provides hardware abstraction traits for architecture-specific operations.
//! All hardware-dependent code should go through these traits.

/// Extended register state (XSAVE area on x86-64, VFP/NEON on ARM64, F/D on RISC-V).
///
/// Modern 64-bit architectures do not have a separate FPU. Instead, floating-point
/// and SIMD operations use extended registers that are part of the general context.
/// This structure abstracts the architecture-specific extended register save area.
///
/// # Architecture Mapping
/// | Architecture | Register Set | Save Instruction |
/// |-------------|--------------|------------------|
/// | x86-64      | XMM/YMM/ZMM  | XSAVE/XRSTOR     |
/// | ARM64       | VFP/NEON/SVE | FPSIMD context   |
/// | RISC-V 64   | F/D/Q        | fsd/fsq          |
#[repr(align(64))]
#[derive(Debug, Clone)]
pub struct ExtRegState {
    /// Architecture-specific raw save area.
    /// Size is architecture-dependent; 576 bytes covers x86-64 AVX-512.
    data: [u8; EXT_REG_STATE_SIZE],
    /// Whether this state has been initialized/saved.
    valid: bool,
}

/// Extended register state size.
/// TODO: This is currently hardcoded for x86-64 AVX-512. Each architecture
/// should define its own size via a const or cfg-based selection.
const EXT_REG_STATE_SIZE: usize = 576;

impl ExtRegState {
    pub fn new() -> Self {
        Self {
            data: [0; EXT_REG_STATE_SIZE],
            valid: false,
        }
    }

    pub fn as_bytes(&self) -> &[u8; EXT_REG_STATE_SIZE] {
        &self.data
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8; EXT_REG_STATE_SIZE] {
        &mut self.data
    }

    pub fn is_valid(&self) -> bool {
        self.valid
    }

    pub fn mark_valid(&mut self) {
        self.valid = true;
    }

    pub fn invalidate(&mut self) {
        self.valid = false;
    }
}

impl Default for ExtRegState {
    fn default() -> Self {
        Self::new()
    }
}

/// Extended register operations trait.
///
/// Each architecture implements this trait to provide extended register
/// state management (floating-point, SIMD, vector registers).
pub trait ExtRegOps {
    /// Saves extended register state for the current process.
    ///
    /// # Safety
    /// Must be called with valid ExtRegState buffer.
    unsafe fn save_state(state: &mut ExtRegState);

    /// Restores extended register state for the current process.
    ///
    /// # Safety
    /// Must be called with valid ExtRegState buffer.
    unsafe fn restore_state(state: &ExtRegState);

    /// Checks if extended registers have been used by the process.
    fn is_used() -> bool;

    /// Copies extended register state from source to destination.
    fn copy_state(dst: &mut ExtRegState, src: &ExtRegState) {
        dst.data.copy_from_slice(&src.data);
        dst.valid = src.valid;
    }
}

/// Mock extended register implementation for testing.
pub struct MockExtReg;

impl ExtRegOps for MockExtReg {
    unsafe fn save_state(_state: &mut ExtRegState) {
        // Mock: mark as valid
        _state.mark_valid();
    }

    unsafe fn restore_state(_state: &ExtRegState) {
        // Mock: no-op
    }

    fn is_used() -> bool {
        // Mock: always return true for testing
        true
    }
}

/// Architecture initialization.
pub fn init() {
    // TODO: Architecture initialization
}
