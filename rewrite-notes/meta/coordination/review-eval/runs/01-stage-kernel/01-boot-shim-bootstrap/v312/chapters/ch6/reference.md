DOC: 01-boot-shim-bootstrap.md
CHAPTER: 6. 诊断
HARD-REJECTIONS: 0
VERDICT: READABLE
MANUAL_FALLBACK: 独立 reference subagent 在 227 行切片上连续停滞，由主线程只读抽查；这不是独立冷读。

五处抽查均可在本章定位：EBS 分界（§6.1 第 18 行）、aarch64 UEFI 两条固定标记（§6.1 第 56 行）、panic 256 B 栈缓冲（§6.2）、模块次序失败（§6.3 第 130 行）、参数缓冲（§6.3 第 130 行与 §6.4）。跨章指针 §5.4/§5.5 有对应目标；无悬空指针。
