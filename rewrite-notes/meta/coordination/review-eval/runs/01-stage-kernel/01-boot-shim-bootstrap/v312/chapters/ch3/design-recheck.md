DOC: 01-boot-shim-bootstrap.md
CHAPTER: 3. Rust 设计决策
VERDICT: PASS

校验：`sha256sum` = `69d5bad3b3d008fc9780eff202c7e5ceb8b4846c4cccf4f31ae01c8250993c58`，与要求一致；`wc -l` = 587。合同：`/home/xzhao/github/minix-rs/rewrite-notes/meta/design/01-stage-kernel/01-design.v4.md`。
复核范围：上一份 FAIL 报告的 5 个阻断项（FAIL-1、FAIL-2、F3、F4、F5）与 2 个非阻断项；只读当前切片、合同，以及报告为这 5 项引用的 `os/` 与 `minix3/` 文件。

## 结论

5 个阻断项均已消除，2 个非阻断项均已修正。FIX-4 所在段落残留一处编辑计数不一致（引导句写“四条结论”，下面实际六个条目），属非阻断，不改变本次通过判定；最小修法见 FIX-4 的“残留”行。

## FIX-1（FAIL-1，原 §3.6 线 511）：U-Boot shim 的 arena 承担物

- 切片：509。
- 修后文本：“U-Boot shim 腿自建表、不经过此通道——它用自建表取件，把模块描述表（`BootModule`：name/start/len）与 memmap 等载荷序列化进 arena；`BootFileTable` 与模块字节留在物理内存；arena 交给接收镜像的 `arch_boot_resume_v1`；调 `arch_boot` 的是两个直核面。”
- 判定：PASS。原缺陷是把 `BootFileTable` 说成被序列化进 arena；修后文本把承担者分开：shim 在 `.bss` 自建表取件，arena 只承载 `BootModule` 描述符与 memmap 等载荷，表与模块字节留在物理内存。
- 证据：`os/boot-shim/src/uboot_main.rs:365-373` 的 `build_shim_table` 在 shim 自己的 `.bss` 建表；`os/libs/minix-boot/src/handoff_v1.rs:52-56` 的 `WireBootModule` 是 name/start/len 三字段；`handoff_v1.rs:347-392` 的分区表只有 memmap、reserved、modules、platform、param、sources 六个分区，没有 `BootFileTable` 分区。
- 残留说明（非阻断）：同段 511 行“把内核 ELF 与 12 个引导模块准备成 `BootHandoffV1` arena”是交接载荷的简写；紧接的 509 行已限定“模块字节留在物理内存”，不构成原缺陷复发。

## FIX-2（FAIL-2，原 §3.7 线 552）：`authorize_and_copy` 与 DTB 记录的调用者

- 切片：550。
- 修后文本：“UEFI 腿的 source 分区由两个阶段直写 wire：分类器先写 UEFI 记录，EBS 配对循环在 `authorize_and_copy` 返回 `OwnedDtb` 后调用 `resv_claims::capture` 生成 DTB `/reserved-memory`/MRB 记录，再经 `push_dtb_sources` 追加并二次 `finish_sources()`；……”
- 判定：PASS。主语已从 `authorize_and_copy` 改为 EBS 配对循环；授权/拷贝与记录生成/追加的边界与代码一致。
- 证据：`os/boot-shim/src/dtb_copy.rs:176` 的 `authorize_and_copy(descs) -> Result<Option<OwnedDtb>, DtbPageError>` 只做授权与拷贝；`os/boot-shim/src/ebs_pair.rs:195-201` 在 `if let Some(owned) = authorize_and_copy(...)` 分支内调用 `resv_claims::capture(&mut claims, owned.bytes(), owned.source)` 与 `push_dtb_sources(serializer, claims.records())`；`ebs_pair.rs:243-250` 的 `push_dtb_sources` 逐条 `push_source` 后调用 `finish_sources()`。

## FIX-3（F3，§3.2.1）：`entry_point` 的去向

- 切片：134（位于 §3.2.1“三个布局数字”段之后）。
- 修后文本：“`entry_point` 不进 `KernelInfo`/wire：boot-shim 从 ELF `e_entry` 取它并断言等于 `kern_virt_base`，跳转只用发送侧布局值；接收侧从 `kern_virt_base` 进入，不需要单独字段。”
- 判定：PASS。合同 `01-design.v4.md:11` 把 `entry_point` 列入内核位置记录；修后文本交代了发送侧来源、断言、跳转用法与接收侧重建方式，合同四项都有去向。
- 证据：`os/boot-shim/src/loader.rs:251、293` 从 ELF 头取 `e_entry` 放进 `KernelLoadResult.entry_point`；`loader.rs:553-566` 的 `load_kernel_with_loader_gated` 断言 `entry_point == kern_virt_base`，U-Boot shim 经 `os/boot-shim/src/opensbi_helpers.rs:359` 走这里；`os/boot-shim/src/uefi_helpers.rs:1043-1046` 的 UEFI 生产入口有同一断言；`loader.rs:559-562` 的注释明确“跳转目标是 `kern_virt_base`，不是 `entry_point`”，与切片“跳转只用发送侧布局值”一致。
- 说明（非阻断）：两个直核面不从 ELF 解析布局，切片 136 行的“两个直核面是例外”已覆盖，不与本句冲突。

## FIX-4（F4，§3.4.1）：删除“逐条解码”框架

- 切片：289-296。
- 修后文本：引导句改为“这段区域的设计性质可以压成四条结论（行级细节见第四章与 02 篇）：”，其后六个条目陈述链接脚本的符号、unpaged 段的执行地址等于装载地址、`_kern_offset` 与 `AT()`、以及该区域只服务分页前并在 `kmain` 后段归还。
- 判定：PASS（原缺陷消除）。切片不再声称“逐条解码”一段不存在的链接脚本摘录；“前三个赋值”的不可核对指代已删除，行级细节明确指向第四章与 02 篇，符合上一份报告建议的“压缩成设计结论”修法。
- 证据：`minix3/minix/kernel/arch/i386/kernel.lds:4-6、14-22、25-34` 与六个条目逐项对得上（三个基址赋值、`. = _kern_phys_base`、`unpaged_*.o` 三个段、`__k_unpaged__kern_unpaged_start/end`、`. += _kern_offset`、`AT(ADDR(...) - _kern_offset)`、`.bss` 收尾的 `__k_unpaged__kern_size` 与 `_kern_size`）；`minix3/minix/kernel/arch/i386/Makefile.inc:64-67` 的 `objcopy --prefix-symbols=__k_unpaged_` 与 294 行一致；Ch2 §2.3 的 `get_parameters` 节选（`tmp/review-loop/01-stage/chapters/01-boot-shim-bootstrap/ch2-slice-candidate.md:104-123`）支持 294 行的“引文可核对”。
- 残留（非阻断，本次修法带入的编辑问题）：289 行说“四条结论”，291-296 行实际六个条目。最小修法：把“四条”改为“六条”；或把 291-292 行并成一条布局事实、296 行明确标为结论，使条目数与引导句一致。

## FIX-5（F5，原 §3.6 线 503-505）：删掉没有引文的“引文”指代

- 切片：503。
- 修后文本：“additive resume 的 Step 4 就是 02 篇 §4.3 的 `establish the direct map`；本篇 §4.8 只保留启动期概念入口，完整机制归独立内存管理主题；这里只保留时序位置。`new_from_page` 的真实签名是 `new_from_page(root_page, channel: PteChannel)`：……”
- 判定：PASS。切片不再说“引文里的……”“引文省略了……”，改为直接陈述 Step 4 与真实签名。
- 证据：`os/kernel/src/lib.rs:288-300` 的 `arch_boot_resume_v1` 文档注释含 “land the KernelInfo into this image's `.bss`, establish the direct map, then enter `kmain`”（原句在 295 行）；`os/arch/src/arch/paging.rs:270` 的签名是 `fn new_from_page(root_page: PhysBytes, channel: PteChannel) -> Self`。

## 非阻断项

- 非阻断-1（Ch1 术语锚点，切片 112）：PASS。原“§1.0 术语”已改为“§1.6 术语”；`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:115` 是 `### 1.6 本章回顾`，`:129` 是 `#### 术语`，`:20` 的 §1.0 是“本章导读”，没有术语表。
- 非阻断-2（五个裸 u64 措辞，切片 416）：PASS。原“五个相邻的裸 `u64`”已改为“`PlacementPool` 的五个同型字段（名字池地址、bump 起点、bump 终点、内核起点、内核终点）”；`os/kernel-image/src/bootface_common.rs:176-189` 的 `PlacementPool` 正是这五个 `u64` 字段，“互换后仍能编译”成立。`bootface_common.rs:169` 的注释“The three physical addresses…”仍在讲旧签名，属代码注释残留，不影响本项判定。

## 证据口径

本复核只读当前切片、合同与上一份报告为这 5 项引用的代码文件；每条 PASS 均给出切片行号与代码或文档锚点。除 FIX-4 的“四条/六条”计数外，未发现阻断项复发。

SUMMARY: blockers_fixed=5/5 nonblocking_fixed=2/2
