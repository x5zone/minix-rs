DOC: 01-boot-shim-bootstrap
CHAPTER: 5. 测试：四条不变量各自的证据
HARD: 0
VERDICT: READABLE

切片校验：sha256 fe2f61658033798db6c363aa712c26c09cdd877507cf5abbe1618cc0c1212207、103 行，与交办一致；只读该文件，未改其他文件。

只核上轮两处 HARD
- H1 aarch64 标记：已落实。§5.4 第 58 行列 `kernel loaded (entry staged)`、`12 boot modules loaded`，逐字等于 `os/qemu-tests/test-shim-bootmarks-aarch64.sh:41-42`；第三条变体 `memmap source`（脚本 50/167 行）与 `DTB preflight failed`（171 行）亦在，`DTB_CFGTABLE` 于表前第 51 行释义；脚本 183-189 行要求 MARK1–MARK4 同时命中，与“两者互斥、均须前两条固定标记同时出现”相符。
- H2 证据标签：已落实。§5.1 第 26-29 行宿主格均带 `[结构保证]`、真机格均带 `[观测]`；§5.3 第 39 行给出三档定义与“未标注按 `[观测]` 读”的缺省，标签不再是空承诺。

残留 P2（不阻断）：定义在 §5.3 而首用在其前的 §5.1，第 39 行“后文”一说未覆盖这一处；§5.3 自己的表（42-45 行）未标档，其缺省读法会把指向 §5.1 `[结构保证]` 的指针格读成 `[观测]`。

HARD：0。两处卡点已落实，本章可作案头参考（READABLE）。
