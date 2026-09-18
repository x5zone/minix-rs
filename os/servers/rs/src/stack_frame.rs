//! 服务进程初始栈帧构建——C `minix_stack_params`/`minix_stack_fill`
//! (`minix3/minix/lib/libc/sys/stack_utils.c`)的 Rust 对应物。
//!
//! 新实例的初始栈布局(NetBSD 纪律,stack_utils.c:22-52):帧头部从低到
//! 高是 `argc`、argv 指针数组、NULL、env 指针数组、NULL;字符串区从
//! 固定偏移(`STACK_MIN_SZ - ps_strings + 指针槽数`)起,后随字对齐垫,
//! 最后是 `ps_strings` 结构。所有指针以 `vsp + 帧内偏移` 表达——vsp 是
//! 新地址空间里的栈指针(`stack_high - frame_size`)。
//!
//! 纯函数模块:无 I/O、无内核交互,`srv_execve`(S17)以此组装栈帧,
//! 组装出的字节经 `sys_datacopy` 拷到子进程。

/// aux 向量槽数。C: `PMEF_AUXVECTORS` — com.h:356。
pub const PMEF_AUXVECTORS: usize = 20;
/// 可执行名槽长(含 NUL)。C: `PMEF_EXECNAMELEN1 = PATH_MAX` —
/// com.h:357、syslimits.h:64。
pub const PMEF_EXECNAMELEN1: usize = 1024;
/// `AuxInfo` 字节宽(LP64:类型+联合各 8 字节,exec_elf.h:1105)。
const AUX_INFO_SIZE: usize = 16;
/// 字宽(LP64 指针/长)。
const WORD: usize = 8;
/// `struct ps_strings` 字节宽(exec.h:111-116:ptr+int+pad+ptr+int+pad)。
pub const PS_STRINGS_SIZE: usize = 32;

/// 帧最小尺寸(stack_utils.c:60-70 的 `STACK_MIN_SZ`):argc 槽、argv/
/// env 两个 NULL 结尾、aux 向量族、可执行名槽、ps_strings。
pub const STACK_MIN_SZ: usize =
    4 + WORD * 2 + AUX_INFO_SIZE * PMEF_AUXVECTORS + PMEF_EXECNAMELEN1 + PS_STRINGS_SIZE;

/// `minix_stack_params` 的结果(stack_utils.c:76-117)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackPlan {
    /// 对齐后的帧大小。
    pub frame_size: usize,
    pub argc: usize,
    pub envc: usize,
}

/// 计算帧大小与参数计数。溢出(C 的 `overflow` 标志 → `E2BIG`)以
/// `None` 表达。
///
/// C 的计数把每个指针槽(8 字节)计进 `stack_size` 再整体对齐;参数
/// 为空切片时结果是 `STACK_MIN_SZ` 对齐值——与 C 逐语句一致。
pub fn stack_params(argv: &[&[u8]], envp: &[&[u8]]) -> Option<StackPlan> {
    let mut size = STACK_MIN_SZ;
    for a in argv {
        size = size.checked_add(WORD + a.len() + 1)?;
    }
    for e in envp {
        size = size.checked_add(WORD + e.len() + 1)?;
    }
    let frame_size = (size + WORD - 1) & !(WORD - 1);
    Some(StackPlan {
        frame_size,
        argc: argv.len(),
        envc: envp.len(),
    })
}

/// `minix_stack_fill`(stack_utils.c:119-172):按 NetBSD 纪律生成帧
/// 字节。返回 `(vsp, ps_str)`——新地址空间的栈指针与 `ps_strings`
/// 结构地址(`srv_execve` 传给 exec 握手的 `ps_str`)。
///
/// `frame` 必须不短于 [`stack_params`] 给出的 `frame_size`;C 以
/// `overflow` 标志防线,这里以调用方契约表达(调用方先跑
/// [`stack_params`])。
pub fn stack_fill(frame: &mut [u8], stack_high: u64, argv: &[&[u8]], envp: &[&[u8]]) -> (u64, u64) {
    let stack_size = frame.len();
    let vsp = stack_high - stack_size as u64;

    // C:141-143 —— 头部第一个字是 argc。
    frame[0..WORD].copy_from_slice(&(argv.len() as u64).to_ne_bytes());

    // C:146-148 —— 字符串区固定起点:MIN - ps_strings + 指针槽数。
    let mut fp = STACK_MIN_SZ - PS_STRINGS_SIZE + (envp.len() + argv.len()) * WORD;
    // 字槽游标:紧跟头部 argc 字之后。
    let mut wp = WORD;

    // C:150-157 —— argv 指针与字符串本体交替写。
    for a in argv {
        let ptr = vsp + fp as u64;
        frame[wp..wp + WORD].copy_from_slice(&ptr.to_ne_bytes());
        wp += WORD;
        frame[fp..fp + a.len()].copy_from_slice(a);
        frame[fp + a.len()] = 0; // C strlen+1 的 NUL
        fp += a.len() + 1;
    }
    // C:158 —— argv NULL 结尾。
    frame[wp..wp + WORD].copy_from_slice(&0u64.to_ne_bytes());
    wp += WORD;

    // C:160-167 —— 环境同型。
    for e in envp {
        let ptr = vsp + fp as u64;
        frame[wp..wp + WORD].copy_from_slice(&ptr.to_ne_bytes());
        wp += WORD;
        frame[fp..fp + e.len()].copy_from_slice(e);
        frame[fp + e.len()] = 0;
        fp += e.len() + 1;
    }
    frame[wp..wp + WORD].copy_from_slice(&0u64.to_ne_bytes());
    let _ = wp; // env NULL 之后字槽游标完成使命

    // C:169-170 —— 字对齐垫。
    while !fp.is_multiple_of(WORD) {
        frame[fp] = 0;
        fp += 1;
    }

    // C:172-178 —— ps_strings:argv 表基、个数、env 表基、个数。
    let ps_argvstr = vsp + WORD as u64; // vsp + sizeof(argc 槽)
    let ps_envstr = ps_argvstr + (argv.len() as u64 + 1) * (WORD as u64);
    frame[fp..fp + WORD].copy_from_slice(&ps_argvstr.to_ne_bytes());
    frame[fp + WORD..fp + WORD + 4].copy_from_slice(&(argv.len() as u32).to_le_bytes());
    frame[fp + 2 * WORD..fp + 3 * WORD].copy_from_slice(&ps_envstr.to_ne_bytes());
    frame[fp + 3 * WORD..fp + 3 * WORD + 4].copy_from_slice(&(envp.len() as u32).to_le_bytes());

    let ps_str = vsp + fp as u64;
    (vsp, ps_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 空参数:帧大小即 STACK_MIN_SZ 对齐值,头部 argc=0。
    #[test]
    fn empty_params_give_min_frame() {
        let plan = stack_params(&[], &[]).unwrap();
        assert_eq!(plan.argc, 0);
        assert_eq!(plan.envc, 0);
        assert_eq!(plan.frame_size, (STACK_MIN_SZ + WORD - 1) & !(WORD - 1));
        let mut frame = alloc::vec![0u8; plan.frame_size];
        let (vsp, ps_str) = stack_fill(&mut frame, 0xF000_0000, &[], &[]);
        assert_eq!(vsp, 0xF000_0000 - plan.frame_size as u64);
        // argc 字在帧头。
        assert_eq!(u64::from_le_bytes(frame[0..8].try_into().unwrap()), 0);
        let _ = ps_str;
    }

    /// 两参数一环境:头部指针序列与字符串区内容、ps_strings 四域全验。
    #[test]
    fn fill_matches_netbsd_layout() {
        let argv: &[&[u8]] = &[b"/bin/rs", b"-v"];
        let envp: &[&[u8]] = &[b"KEY=1"];
        let plan = stack_params(argv, envp).unwrap();
        let mut frame = alloc::vec![0u8; plan.frame_size];
        let (vsp, ps_str) = stack_fill(&mut frame, 0xF000_0000, argv, envp);

        let word = |o: usize| u64::from_le_bytes(frame[o..o + 8].try_into().unwrap());
        assert_eq!(word(0), 2, "argc"); // argc
                                        // C stack_utils.c:146-148:字符串区固定起点 = MIN - ps_strings +
                                        // (envc+argc)×8 = 1396-32+24 = 1388(argv0 是第一个字符串)。
        let str_area = (STACK_MIN_SZ - PS_STRINGS_SIZE + (plan.envc + plan.argc) * WORD) as u64;
        assert_eq!(word(8), vsp + str_area, "argv0 指向字符串区固定起点");
        assert_eq!(
            word(16),
            vsp + str_area as u64 + 8,
            "argv1 紧随 argv0 的字符串"
        );
        assert_eq!(word(24), 0, "argv NULL 结尾");
        // argv 字符串区:/bin/rs\0(8) + -v\0(3) = 11 字节,env0 紧随。
        assert_eq!(
            word(32),
            vsp + str_area as u64 + 11,
            "env0 紧随 argv 字符串"
        );
        assert_eq!(word(40), 0, "env NULL 结尾");

        // ps_strings 四域(位于字对齐垫之后的 fp)。
        let ps_off = (ps_str - vsp) as usize;
        let ps_argvstr = word(ps_off);
        assert_eq!(
            ps_argvstr,
            vsp + WORD as u64,
            "ps_argvstr = vsp + sizeof(argc)"
        );
        assert_eq!(
            u32::from_le_bytes(frame[ps_off + 8..ps_off + 12].try_into().unwrap()),
            2,
            "ps_nargvstr"
        );
        assert_eq!(
            word(ps_off + 16),
            ps_argvstr + 3 * (WORD as u64),
            "ps_envstr 越过 argv 表"
        );
        assert_eq!(
            u32::from_le_bytes(frame[ps_off + 24..ps_off + 28].try_into().unwrap()),
            1,
            "ps_nenvstr"
        );
        // 字符串本体:NUL 结尾。
        let a0 = (word(8) - vsp) as usize;
        assert_eq!(&frame[a0..a0 + 8], b"/bin/rs\0");
    }

    /// 大参数(1 MiB 字符串)仍可用:帧大小如实反映,无截断。
    #[test]
    fn large_params_scale() {
        let big = alloc::vec![b'x'; 1024 * 1024];
        let argv: &[&[u8]] = &[&big];
        let plan = stack_params(argv, &[]).unwrap();
        assert!(plan.frame_size > 1024 * 1024);
        assert_eq!(plan.argc, 1);
    }
}
