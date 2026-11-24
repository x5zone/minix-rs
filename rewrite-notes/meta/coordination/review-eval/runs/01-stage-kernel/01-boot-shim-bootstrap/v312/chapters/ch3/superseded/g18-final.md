DOC: tmp/review-loop/01-stage/chapters/01-boot-shim-bootstrap/ch3-slice-candidate.md
CHAPTER: 3. Rust 设计决策
HARD: 0
VERDICT: READY

核对：交办 sha256 d709dcfb72a133b0c8e23a4a5a21f4fb3ef39547b24fa6f991b1126ab814421c 与 586 行在收尾时匹配；按交办只复核上轮两处 HARD 的修复落点。
复核 1：§3.4.2 第 342 行已改为 `就是 §2.5.1 表里的"还 VM 镜像"那一行` 与 `VM 条目在回收后被置零，见 §2.5.1`；ch2 §2.5.1 的“三处改账动作”表确有该行，表后段落亦写明回收后置零。指针可达，原悬空消除。
复核 2：§3.2.1 第 112 行已就地写出 `LMA（装载地址，load address）与 VMA（执行地址，virtual address）`，并把后续指向改为“完整推演见 §3.4”；§3.4.1 正是用 `AT()` 与“执行地址/装载地址”展开这两个地址，落点一致。定义不再落空。
结论：两处 HARD 均已修复，未引入新矛盾；计数词与上轮复核一致（五条生产腿/三 v1/两直核/三组/六链/四门链，10 字段、六分区、128 B 信封、64+47 等算术自洽）。上轮 3 条软卡点按交办不重开，均不阻断。VERDICT: READY。
