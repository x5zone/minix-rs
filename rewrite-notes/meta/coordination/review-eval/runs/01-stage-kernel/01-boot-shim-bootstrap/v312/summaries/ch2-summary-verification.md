DOC: 01-boot-shim-bootstrap.md
CHAPTER: 2. C 源码分析
VERDICT: FAITHFUL

核对基线：切片 sha256 7de1821f5705cfc116dd98ed9b3d2db9c5b63b0501d73c43aa83200ef1bf214c（434 行）；摘要 sha256 978943b8a05780293bff072f9c93b67bddb1f5359582416e3ff29c9a5d42347c（54 行；tmp 草稿与 v312 规范副本逐字相同）。
PASS I3：摘要 L51“I3 分两步：引导器一次填好 `mbi`（发送侧发布），内核整份拷贝后独占 `kinfo`，此后不再改”与切片 L397 一致。
PASS 诊断入口：摘要 L12“`HAS_MODS` 未置查模块表分配失败，已置而条数为 0 才查两条指令的先后”与切片 L68 的分支判据一致。
PASS ① 摘要 L54“下一章把这套 C 记录改写成设计契约（KernelInfo 按值交接、两张表加来源记录、门链先于消费）”与切片 L385“类型化与协议留给第 3 章”、L397“第 3 章要补的来源记录缺口”的边界一致。
PASS ② 摘要 L51“I4 的版本项也没有对应物（Multiboot info 无版本字段）”与切片 L397 一致。
PASS ③ board（L22/L24 = L154–174）、load_mods（L9/L12 = L53/L66–68）、HAS_CMDLINE（L21 = L133/L176）、rootdevname（L25 = L58–60/L178–183）、跳板（L35 = L232–255）、三处改账（L42 = L299–316）、§2.7 四步（L49 = L362–371）均一致。
