DOC: 01-boot-shim-bootstrap.md
CHAPTER: 3. Rust 设计决策
HARD-REJECTIONS: 0
VERDICT: READY

七条主线（协议换代、按值交接、两张表、bootstrap 区间、取件抽象、五条生产腿、门链）逐节都有问题入口与收口，§3.0/§3.1.1/§3.2/§3.6 的计数与成员、§3.7 的两族门链次序能互相对上，冷读可顺着主线走通；§3.2.1 的 `memmap` 单元格与 §3.6 的 U-Boot shim 段最密，但都属于可回读的细节层，不构成断点。
