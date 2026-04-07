//! mproc 重构方案性能和内存基准测试
//!
//! 测试三种方案的内存占用和性能差异：
//! - 方案二：Enum 状态机
//! - 方案三：分层抽象
//! - 方案五：Bitflags + View
//!
//! 运行方式：
//! ```bash
//! cargo test --release -- --nocapture
//! ```

use std::mem::size_of;
use std::time::Instant;

// ============================================================================
// 方案二：Enum 状态机
// ============================================================================

/// 进程生命周期（方案二）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleV2 {
    Unused,
    Running,
    Exiting { exit_code: i8, sig_status: i8 },
    TraceZombie { exit_code: i8, sig_status: i8 },
    Zombie { exit_code: i8, sig_status: i8 },
    ToldParent { exit_code: i8, sig_status: i8 },
}

impl LifecycleV2 {
    pub fn is_zombie(&self) -> bool {
        matches!(self, Self::Zombie { .. } | Self::TraceZombie { .. })
    }
    
    pub fn is_exiting(&self) -> bool {
        matches!(self, Self::Exiting { .. })
    }
}

impl Default for LifecycleV2 {
    fn default() -> Self {
        Self::Unused
    }
}

/// 阻塞状态（方案二）
#[derive(Debug, Clone, Copy, Default)]
pub struct BlockStateV2 {
    pub stopped: bool,
    pub ipc_blocked: Option<IpcBlockReasonV2>,
    pub unpaused: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum IpcBlockReasonV2 {
    VfsCall,
    EventCall,
    DelayedSignal,
}

/// 进程结构（方案二）
#[derive(Debug, Clone)]
pub struct ProcessV2 {
    pub pid: i32,
    pub endpoint: i32,
    pub procgrp: i32,
    pub name: [u8; 16],
    pub lifecycle: LifecycleV2,
    pub block: BlockStateV2,
    pub parent: usize,
    pub tracer: Option<usize>,
    pub child_utime: i64,
    pub child_stime: i64,
    pub nice: i32,
    pub flags: u32,
}

// ============================================================================
// 方案三：分层抽象
// ============================================================================

/// 身份信息（方案三）
#[derive(Debug, Clone, Default)]
pub struct ProcessIdentityV3 {
    pub pid: i32,
    pub endpoint: i32,
    pub parent: usize,
    pub procgrp: i32,
    pub name: [u8; 16],
}

/// 状态机（方案三）
#[derive(Debug, Clone)]
pub struct ProcessStateV3 {
    pub lifecycle: LifecycleV3,
    pub block: BlockStateV3,
    pub tracer: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleV3 {
    Unused,
    Running,
    Exiting { exit_code: i8, sig_status: i8 },
    TraceZombie { exit_code: i8, sig_status: i8 },
    Zombie { exit_code: i8, sig_status: i8 },
    ToldParent { exit_code: i8, sig_status: i8 },
}

impl LifecycleV3 {
    pub fn is_zombie(&self) -> bool {
        matches!(self, Self::Zombie { .. } | Self::TraceZombie { .. })
    }
    
    pub fn is_exiting(&self) -> bool {
        matches!(self, Self::Exiting { .. })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BlockStateV3 {
    pub stopped: bool,
    pub ipc_blocked: Option<IpcBlockReasonV3>,
    pub unpaused: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum IpcBlockReasonV3 {
    VfsCall,
    EventCall,
    DelayedSignal,
}

/// 资源（方案三）
#[derive(Debug, Clone)]
pub struct ProcessResourcesV3 {
    pub child_utime: i64,
    pub child_stime: i64,
    pub nice: i32,
    pub flags: u32,
}

/// 进程结构（方案三）
#[derive(Debug, Clone)]
pub struct ProcessV3 {
    pub identity: ProcessIdentityV3,
    pub state: ProcessStateV3,
    pub resources: ProcessResourcesV3,
}

// ============================================================================
// 方案五：Bitflags + View
// ============================================================================

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ProcFlagsV5: u32 {
        const IN_USE = 0x00001;
        const WAITING = 0x00002;
        const ZOMBIE = 0x00004;
        const PROC_STOPPED = 0x00008;
        const ALARM_ON = 0x00010;
        const EXITING = 0x00020;
        const TOLD_PARENT = 0x00040;
        const TRACE_STOPPED = 0x00080;
        const SIGSUSPENDED = 0x00100;
        const VFS_CALL = 0x00400;
        const NEW_PARENT = 0x00800;
        const UNPAUSED = 0x01000;
        const PRIV_PROC = 0x02000;
        const PARTIAL_EXEC = 0x04000;
        const TRACE_EXIT = 0x08000;
        const TRACE_ZOMBIE = 0x10000;
        const DELAY_CALL = 0x20000;
        const TAINTED = 0x40000;
        const EVENT_CALL = 0x80000;
    }
}

/// 生命周期视图（方案五）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleViewV5 {
    Unused,
    Running,
    Exiting,
    TraceZombie,
    Zombie,
    ToldParent,
}

/// 进程结构（方案五）
#[derive(Debug, Clone)]
pub struct ProcessV5 {
    pub pid: i32,
    pub endpoint: i32,
    pub procgrp: i32,
    pub name: [u8; 16],
    pub raw_flags: ProcFlagsV5,
    pub parent: usize,
    pub tracer: usize,
    pub child_utime: i64,
    pub child_stime: i64,
    pub nice: i32,
    pub exit_code: i8,
    pub sig_status: i8,
}

impl ProcessV5 {
    pub fn lifecycle(&self) -> LifecycleViewV5 {
        if self.raw_flags.contains(ProcFlagsV5::ZOMBIE) {
            LifecycleViewV5::Zombie
        } else if self.raw_flags.contains(ProcFlagsV5::TRACE_ZOMBIE) {
            LifecycleViewV5::TraceZombie
        } else if self.raw_flags.contains(ProcFlagsV5::TOLD_PARENT) {
            LifecycleViewV5::ToldParent
        } else if self.raw_flags.contains(ProcFlagsV5::EXITING) {
            LifecycleViewV5::Exiting
        } else if self.raw_flags.contains(ProcFlagsV5::IN_USE) {
            LifecycleViewV5::Running
        } else {
            LifecycleViewV5::Unused
        }
    }
    
    pub fn is_zombie(&self) -> bool {
        self.raw_flags.contains(ProcFlagsV5::ZOMBIE) 
            || self.raw_flags.contains(ProcFlagsV5::TRACE_ZOMBIE)
    }
    
    pub fn is_exiting(&self) -> bool {
        self.raw_flags.contains(ProcFlagsV5::EXITING)
    }
    
    pub fn is_stopped(&self) -> bool {
        self.raw_flags.contains(ProcFlagsV5::PROC_STOPPED)
    }
    
    pub fn set_exiting(&mut self, exit_code: i8, sig_status: i8) {
        self.raw_flags.insert(ProcFlagsV5::EXITING);
        self.exit_code = exit_code;
        self.sig_status = sig_status;
    }
    
    pub fn set_zombie(&mut self) {
        self.raw_flags.remove(ProcFlagsV5::EXITING);
        self.raw_flags.insert(ProcFlagsV5::ZOMBIE);
    }
}

// ============================================================================
// 默认实现
// ============================================================================

impl Default for ProcessV2 {
    fn default() -> Self {
        Self {
            pid: 0,
            endpoint: 0,
            procgrp: 0,
            name: [0; 16],
            lifecycle: LifecycleV2::default(),
            block: BlockStateV2::default(),
            parent: 0,
            tracer: None,
            child_utime: 0,
            child_stime: 0,
            nice: 0,
            flags: 0,
        }
    }
}

impl Default for ProcessV3 {
    fn default() -> Self {
        Self {
            identity: ProcessIdentityV3 {
                pid: 0,
                endpoint: 0,
                parent: 0,
                procgrp: 0,
                name: [0; 16],
            },
            state: ProcessStateV3 {
                lifecycle: LifecycleV3::Unused,
                block: BlockStateV3::default(),
                tracer: None,
            },
            resources: ProcessResourcesV3 {
                child_utime: 0,
                child_stime: 0,
                nice: 0,
                flags: 0,
            },
        }
    }
}

impl Default for ProcessV5 {
    fn default() -> Self {
        Self {
            pid: 0,
            endpoint: 0,
            procgrp: 0,
            name: [0; 16],
            raw_flags: ProcFlagsV5::empty(),
            parent: 0,
            tracer: usize::MAX,
            child_utime: 0,
            child_stime: 0,
            nice: 0,
            exit_code: 0,
            sig_status: 0,
        }
    }
}

// ============================================================================
// 基准测试
// ============================================================================

const NUM_PROCESSES: usize = 256;
const NUM_ITERATIONS: usize = 1_000_000;

fn main() {
    println!("=== mproc 重构方案基准测试 ===\n");
    
    // 1. 内存大小测试
    println!("--- 内存大小测试 ---");
    println!("方案二 (Enum):");
    println!("  LifecycleV2:   {} bytes", size_of::<LifecycleV2>());
    println!("  BlockStateV2:  {} bytes", size_of::<BlockStateV2>());
    println!("  ProcessV2:     {} bytes", size_of::<ProcessV2>());
    println!("  {} 个进程:      {} KB", NUM_PROCESSES, 
        (size_of::<ProcessV2>() * NUM_PROCESSES) as f64 / 1024.0);
    
    println!("\n方案三 (分层):");
    println!("  ProcessIdentityV3: {} bytes", size_of::<ProcessIdentityV3>());
    println!("  ProcessStateV3:    {} bytes", size_of::<ProcessStateV3>());
    println!("  ProcessResourcesV3:{} bytes", size_of::<ProcessResourcesV3>());
    println!("  ProcessV3:         {} bytes", size_of::<ProcessV3>());
    println!("  {} 个进程:         {} KB", NUM_PROCESSES,
        (size_of::<ProcessV3>() * NUM_PROCESSES) as f64 / 1024.0);
    
    println!("\n方案五 (Bitflags+View):");
    println!("  ProcFlagsV5:   {} bytes", size_of::<ProcFlagsV5>());
    println!("  ProcessV5:     {} bytes", size_of::<ProcessV5>());
    println!("  {} 个进程:      {} KB", NUM_PROCESSES,
        (size_of::<ProcessV5>() * NUM_PROCESSES) as f64 / 1024.0);
    
    // 内存对比
    println!("\n--- 内存对比 ---");
    let v2_size = size_of::<ProcessV2>();
    let v3_size = size_of::<ProcessV3>();
    let v5_size = size_of::<ProcessV5>();
    
    println!("方案二 vs 方案五: {:.1}% 差异", 
        ((v2_size as f64 / v5_size as f64) - 1.0) * 100.0);
    println!("方案三 vs 方案五: {:.1}% 差异",
        ((v3_size as f64 / v5_size as f64) - 1.0) * 100.0);
    println!("方案二 vs 方案三: {:.1}% 差异",
        ((v2_size as f64 / v3_size as f64) - 1.0) * 100.0);
    
    // 2. 性能测试
    println!("\n--- 性能测试 ---");
    
    // 初始化进程表
    let mut v2_procs: Vec<ProcessV2> = (0..NUM_PROCESSES)
        .map(|i| ProcessV2 { pid: i as i32 + 1, ..Default::default() })
        .collect();
    let mut v3_procs: Vec<ProcessV3> = (0..NUM_PROCESSES)
        .map(|i| ProcessV3 { 
            identity: ProcessIdentityV3 { pid: i as i32 + 1, ..Default::default() },
            ..Default::default() 
        })
        .collect();
    let mut v5_procs: Vec<ProcessV5> = (0..NUM_PROCESSES)
        .map(|i| ProcessV5 { pid: i as i32 + 1, ..Default::default() })
        .collect();
    
    // 测试 1: 状态检查 (is_zombie)
    println!("\n测试 1: is_zombie() - {} 次迭代", NUM_ITERATIONS);
    
    let start = Instant::now();
    let mut count = 0u64;
    for _ in 0..NUM_ITERATIONS {
        for proc in &v2_procs {
            if proc.lifecycle.is_zombie() { count += 1; }
        }
    }
    let v2_time = start.elapsed();
    println!("  方案二: {:?} (count={})", v2_time, count);
    
    let start = Instant::now();
    let mut count = 0u64;
    for _ in 0..NUM_ITERATIONS {
        for proc in &v3_procs {
            if proc.state.lifecycle.is_zombie() { count += 1; }
        }
    }
    let v3_time = start.elapsed();
    println!("  方案三: {:?} (count={})", v3_time, count);
    
    let start = Instant::now();
    let mut count = 0u64;
    for _ in 0..NUM_ITERATIONS {
        for proc in &v5_procs {
            if proc.is_zombie() { count += 1; }
        }
    }
    let v5_time = start.elapsed();
    println!("  方案五: {:?} (count={})", v5_time, count);
    
    // 测试 2: 状态设置 (set_exiting)
    println!("\n测试 2: 设置状态 - {} 次迭代", NUM_ITERATIONS);
    
    let start = Instant::now();
    for _ in 0..NUM_ITERATIONS {
        for proc in &mut v2_procs {
            proc.lifecycle = LifecycleV2::Exiting { exit_code: 0, sig_status: 0 };
        }
    }
    let v2_time = start.elapsed();
    println!("  方案二: {:?}", v2_time);
    
    let start = Instant::now();
    for _ in 0..NUM_ITERATIONS {
        for proc in &mut v3_procs {
            proc.state.lifecycle = LifecycleV3::Exiting { exit_code: 0, sig_status: 0 };
        }
    }
    let v3_time = start.elapsed();
    println!("  方案三: {:?}", v3_time);
    
    let start = Instant::now();
    for _ in 0..NUM_ITERATIONS {
        for proc in &mut v5_procs {
            proc.set_exiting(0, 0);
        }
    }
    let v5_time = start.elapsed();
    println!("  方案五: {:?}", v5_time);
    
    // 测试 3: 复杂状态检查
    println!("\n测试 3: 复杂状态检查 - {} 次迭代", NUM_ITERATIONS);
    
    // 设置一些复杂状态
    for proc in &mut v2_procs {
        proc.lifecycle = LifecycleV2::Running;
        proc.block.stopped = true;
        proc.block.ipc_blocked = Some(IpcBlockReasonV2::VfsCall);
    }
    for proc in &mut v3_procs {
        proc.state.lifecycle = LifecycleV3::Running;
        proc.state.block.stopped = true;
        proc.state.block.ipc_blocked = Some(IpcBlockReasonV3::VfsCall);
    }
    for proc in &mut v5_procs {
        proc.raw_flags = ProcFlagsV5::IN_USE | ProcFlagsV5::PROC_STOPPED | ProcFlagsV5::VFS_CALL;
    }
    
    let start = Instant::now();
    let mut count = 0u64;
    for _ in 0..NUM_ITERATIONS {
        for proc in &v2_procs {
            if proc.lifecycle.is_exiting() && proc.block.stopped { count += 1; }
        }
    }
    let v2_time = start.elapsed();
    println!("  方案二: {:?} (count={})", v2_time, count);
    
    let start = Instant::now();
    let mut count = 0u64;
    for _ in 0..NUM_ITERATIONS {
        for proc in &v3_procs {
            if proc.state.lifecycle.is_exiting() && proc.state.block.stopped { count += 1; }
        }
    }
    let v3_time = start.elapsed();
    println!("  方案三: {:?} (count={})", v3_time, count);
    
    let start = Instant::now();
    let mut count = 0u64;
    for _ in 0..NUM_ITERATIONS {
        for proc in &v5_procs {
            if proc.is_exiting() && proc.is_stopped() { count += 1; }
        }
    }
    let v5_time = start.elapsed();
    println!("  方案五: {:?} (count={})", v5_time, count);
    
    // 3. 总结
    println!("\n=== 总结 ===");
    println!("| 方案 | 进程大小 | 256进程内存 | 状态检查 | 状态设置 |");
    println!("|------|---------|------------|---------|---------|");
    println!("| 方案二 (Enum) | {} bytes | {:.1} KB | - | - |", 
        v2_size, (v2_size * NUM_PROCESSES) as f64 / 1024.0);
    println!("| 方案三 (分层) | {} bytes | {:.1} KB | - | - |",
        v3_size, (v3_size * NUM_PROCESSES) as f64 / 1024.0);
    println!("| 方案五 (Bitflags) | {} bytes | {:.1} KB | - | - |",
        v5_size, (v5_size * NUM_PROCESSES) as f64 / 1024.0);
}
