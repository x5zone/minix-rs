//! Exec 切片
//!
//! 实现 exec 系统调用的核心逻辑

use minix_ipc::Endpoint;

/// 执行新程序
///
/// # Arguments
/// * `proc` - 进程 endpoint
/// * `path` - 程序路径
/// * `argv` - 参数
/// * `envp` - 环境变量
pub fn sys_exec(
    proc: Endpoint,
    path: &str,
    argv: &[&str],
    envp: &[&str],
) -> Result<(), ExecError> {
    // TODO: 实现 exec 逻辑
    // 1. 验证路径
    // 2. 读取 ELF 文件（通过 VFS）
    // 3. 释放旧地址空间
    // 4. 加载新地址空间（通过 VM）
    // 5. 设置入口点
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
        // TODO: 基础 exec 测试
    }
}
