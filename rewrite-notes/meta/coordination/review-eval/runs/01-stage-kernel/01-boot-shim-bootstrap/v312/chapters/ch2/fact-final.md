DOC: 01-boot-shim-bootstrap.md
CHAPTER: 2. C 源码分析
VERDICT: PASS

输入核对：ch2-slice-candidate.md 434 行，sha256 d08ba868…48be4d，与指定值一致。

修复 1（§2.4 pg_alloc_page）确认：
- pg_alloc_page 的唯一消费者是 pg_map 的 PG_ALLOCATEME 分支（pg_utils.c:288-289）；pg_map 的唯一调用者是 protect.c:381 的 libexec_pg_alloc，即装载启动镜像那条链。与 §2.4.2 末句“唯一”一致；“4KB 细粒度映射路径”与“PG_ALLOCATEME 路径”指同一条链，不是第二个调用者。
- kernel_may_alloc 在 main.c:142 置 1、main.c:105 置 0 后 main.c:107 才 switch_to_user；pg_alloc_page 首条断言（pg_utils.c:143）确实先撞。“VM 接手后的分配”表述已消失，改为 VM 自有分配器，正确。

修复 2（board 表/术语）确认：
- grep“提升”零命中，术语已删。mb_set_param（pre_init.c:48-58）的删除分支只可能在 p=bigbuf 命中；命中后整段左移并 break；新键由 pre_init.c:61-63 的末尾定位写回末尾；get_value（main.c:481）返回第一条匹配。故“首条被删、内核值追加末尾、内核值胜出”正确。
- 四情形逐条复核与代码一致：无外部键→内核值；非首条→扫描自记录第二字节起、board= 不再匹配，两条并存、外部值胜出；首条→删后追加、内核值胜出；去掉删除循环→首条外部值胜出。

I2 无承载者：add_memmap 只写 mm_base_addr/mm_length/mm_type（pg_utils.c:107-109），且 type 恒为 MULTIBOOT_MEMORY_AVAILABLE；kinfo.memmap 结构无声明者/依据字段，结论成立。

alloc_lowest 参数错位：pg_utils.c:81 以 (lowest, len) 调 cut_memmap，而 cut_memmap 第三参是 end（pg_utils.c:32），表三行与代码相符；四前提复核成立——cstart 末尾 arch_init（main.c:474）早于 smp_init（main.c:311）；arch_init 切 [0,0x4FF) 后经 cut 取整为 [0,0x1000)，add_memmap 起点页对齐，故最低基址 ≥0x1000；跳板 2128 B 数据加代码约 2212 B，roundup 4096；唯一调用点 arch_smp.c:76。第三行常态成立。

overlaps 的 -1：pre_init.c:82-84 宏与切片逐字一致；exec.c:752-753 的 mmo_end=base+len 证实半开；A=[0x100000,0x200000)、B=[0x200000,0x300000) 时去掉 -1 会让 OVERLAP(B,A) 以 0x200000 命中并误 panic，带 -1 查 0x1FFFFF 放行，结论正确。

注 1：§2.5“两点各管一种包含关系”把反方向调用的判据写进了单次 OVERLAP 展开——“比较对象包住对方→对方起点落在它内”实际由 OVERLAP(cmp,thismod) 完成；行为结论（成对调用覆盖双向、相切放行）正确，仅措辞需收紧。
注 2：§2.3 表中 board=intel 是示意值；实际 get_board_name(BOARD_ID_INTEL) 返回 "X86-I586-GENERIC-GENERIC-GENERIC"（minix/board.h:161），不影响删除循环与首匹配的结论。

SUMMARY: errors=0 notes=2
