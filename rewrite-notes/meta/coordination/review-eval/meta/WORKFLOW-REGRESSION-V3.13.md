VERDICT: PASS

# Workflow 回归独立复核（第二轮，V3.13 章契约）

- 复核人：独立 workflow verifier（不参与设计/修复，只读）
- 日期：2026-11-24
- 基线：`HEAD 27768886d`，工作树含未提交变更（见文末 `git status`）
- 环境：Linux，Python 3.12.3
- 复核范围：第一轮报告列出的 8.1–8.5 修复项，重点重放 CI 本地等价命令、历史 D01、负/正/假阳性 fixture、上一轮的 fail-open 探针、brief 缺角色、outline good/bad，以及 §5.4 的 chapter-slice G18 报告。
- 结论：**8.1–8.5 的功能项全部通过；§5.4 的 `chapter-slice-g18.md` 在复核期间落盘，覆盖 1.0–1.4 五个单元并明确不要求全篇 48 单元。第一轮的全部阻塞项已消除，本轮判定 PASS。**

## 1. 本轮复核摘要

| 项 | 第一轮结论 | 本轮实测 | 判定 |
|---|---|---|---|
| CI 本地等价命令 | 两条活文档 exit 1，CI 红 | 五条命令全部 exit 0 | **PASS** |
| CI 三个门接入 | 只有 chapter-contract，无自测 | chapter-contract、outline、brief 三个 `--self-test` + brief 目录 + Ch1 切片 fixture | **PASS** |
| 历史 D01 | exit 1 | exit 1，命中 Ch1/Ch2/Ch3/Ch4 | **PASS** |
| 负/正/假阳性 fixture | 成立；`BOOT_HANDOFF` 未命中 | 成立；neg2 新增 `BOOT_HANDOFF` 命中 | **PASS** |
| 无章/未闭合围栏/前言/`## 1.1`/代码块内标题 | 全部 fail-open | 全部 FAIL，符合预期 | **PASS** |
| Ch2 围栏代码块 | 漏扫 | 命中 `KernelInfo`、`store_boot_report` | **PASS** |
| `BOOT_HANDOFF_V1`/`KERNEL_INFO_V10` | 漏网 | 前缀匹配命中 | **PASS** |
| 字段表 `字段名`/缩进表头 | 漏网 | 命中 | **PASS** |
| 大小写协议 token | `Wire`/`V1` 漏网 | 命中 | **PASS** |
| brief 缺角色 | 只放一份 editor 仍 PASS | 缺角色 exit 1 | **PASS** |
| brief 缺输出头 | 未测 | 缺 `DOC` exit 1 | **PASS** |
| outline good/bad | good PASS、bad FAIL | 一致；prose-only 也 FAIL | **PASS** |
| Ch1 切片 fixture | 不存在 | `prompt/review-panel-chapters/fixtures/ch1-slice.md` 存在，checker PASS | **PASS** |
| **G18 章级实跑报告** | 不存在 | `tmp/review-loop/01-stage/workflow/chapter-slice-g18.md` 已落盘；覆盖 1.0–1.4 五个单元，明示不要求全篇 48 单元；VERDICT NOT-READY 是 fixture 内容结论，不是 workflow 失效 | **PASS** |
| 规范/README 同步 | 16→17、六章、工具名 | 已改；`chapter-brief-check.py` 仍未在规范点名 | PASS（有小残留） |
| **总判定** | FAIL | 第一轮阻塞项全部消除；残留为非阻塞 | **PASS** |

## 2. CI 本地等价命令（五条，全 0）

CI 步骤在 `.github/workflows/rules-consistency.yml:156-164`：

```yaml
      - name: 章契约门（V3.13）
        # 迁移期只跑自测、brief schema 与受版管理的 Ch1 切片 fixture；
        # D01/D02 的 Ch1/Ch3 去漂移完成后，再把两篇活文档加到这里。
        run: |
          python3 tools/chapter-contract-check.py --self-test
          python3 tools/outline-contract-check.py --self-test
          python3 tools/chapter-brief-check.py --self-test
          python3 tools/chapter-brief-check.py prompt/review-panel-chapters
          python3 tools/chapter-contract-check.py prompt/review-panel-chapters/fixtures/ch1-slice.md
```

逐条重放：

```text
$ python3 tools/chapter-contract-check.py --self-test
chapter-contract-check: PASS（/tmp/tmp2dnw7177/good.md）
self-test PASS: Ch1 标识符/字段表/代码块、Ch2 散文与围栏、Ch3 长代码、无章/未闭合围栏、大写常量均被拦截
exit=0

$ python3 tools/outline-contract-check.py --self-test
[FAIL] /tmp/tmpcikymg7k/bad.md: 缺 不写什么、下游导出接口、上一章回顾、知识点/节序
self-test PASS: 缺“不写什么/导出接口/上一章回顾”被拦截
exit=0

$ python3 tools/chapter-brief-check.py --self-test
self-test PASS: 输出头与 Reader-B 三节检查
exit=0

$ python3 tools/chapter-brief-check.py prompt/review-panel-chapters
chapter-brief-check: PASS（prompt/review-panel-chapters）
exit=0

$ python3 tools/chapter-contract-check.py prompt/review-panel-chapters/fixtures/ch1-slice.md
chapter-contract-check: PASS（prompt/review-panel-chapters/fixtures/ch1-slice.md）
exit=0
```

`rg -n 'chapter-contract|outline-contract|chapter-brief' .github/workflows/rules-consistency.yml` 给出 160–164 五行，三个门都在。YAML 可解析，步骤没有 `continue-on-error` 或 `shell:` 覆盖。上一轮“CI 只接入一个门、活文档必红”的两个问题都已消失。

其余 5.1 命令也重放为 0：

```text
bash tools/generate-derived-skills.sh --check           # 0
bash tools/check-review-rules.sh                        # 0
bash tools/lint-review-rules.sh                         # 0
python3 tools/tutorial-rubric-check.py --self-test      # 0
python3 tools/review-units.py --self-test               # 0
python3 tools/panel-index.py --self-test                # 0
python3 tools/v312-closure-check.py --self-test         # 0
python3 tools/verify-check.py --help                    # 0
```

判定：**PASS**。

## 3. 历史 D01

```text
$ git show 78c6373ca:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md > /tmp/v-d01.md
$ python3 tools/chapter-contract-check.py /tmp/v-d01.md
[WARN] Ch4 出现设计/类型词（WARN）：KernelInfo、BootReport、BootStorage、RamAuthority、设计裁决、取舍
[FAIL] Ch1 含 1 个围栏代码块（禁止）
[FAIL] Ch1 含禁用项目标识符：KernelInfo、BootReport、BootStorage、RamAuthority、SourceRecord、BOOT_HANDOFF、wire、v1、MemoryRegion、BootModule、PlatformDescSource
[FAIL] Ch1 含字段表（字段/类型表头）
[FAIL] Ch2 含 Rust 项目名/路径：KernelInfo
[FAIL] Ch3 有超 10 行代码块：第1块 13行、第2块 14行、第3块 29行、第4块 28行、第5块 29行、第6块 20行、第7块 20行、第8块 12行、第10块 14行、第11块 11行、第12块 14行、第14块 22行、第15块 77行、第16块 12行
chapter-contract-check: FAIL（/tmp/v-d01.md）
exit=1
```

与证据文件的差异：本轮实际输出比第一轮多命中 `BOOT_HANDOFF`（前缀匹配生效），其余命中一致。历史 D01 与当前活文档逐字节相同（sha256 `9d6e18a5...`），所以它仍可作为“当前漂移会被抓住”的证据；CI 在迁移期不再直接跑它，等 D01/D02 去漂移完成后再接入，注释已写明。

判定：**PASS**。

## 4. 负样本、正样本、假阳性 fixture

```text
$ python3 tools/chapter-contract-check.py .../neg1-ch1-kernelinfo.md
[FAIL] Ch1 含禁用项目标识符：KernelInfo、MemoryRegion
[FAIL] Ch1 含字段表（字段/类型表头）
exit=1

$ python3 tools/chapter-contract-check.py .../neg2-ch1-wire.md
[FAIL] Ch1 含禁用项目标识符：BOOT_HANDOFF、wire、v9、v10
exit=1

$ python3 tools/chapter-contract-check.py .../neg3-ch1-report.md
[FAIL] Ch1 含禁用项目标识符：BootReport、RamAuthority、SourceRecord
exit=1

$ python3 tools/chapter-contract-check.py .../pos1-ch1-abstract.md
chapter-contract-check: PASS（.../pos1-ch1-abstract.md）
exit=0

$ python3 tools/chapter-contract-check.py .../fp1-ch1-abstract-terms.md
chapter-contract-check: PASS（.../fp1-ch1-abstract-terms.md）   exit=0

$ python3 tools/chapter-contract-check.py .../fp2-ch2-rust-word.md
chapter-contract-check: PASS（.../fp2-ch2-rust-word.md）        exit=0

$ python3 tools/chapter-contract-check.py .../fp3-ch3-short-code.md
chapter-contract-check: PASS（.../fp3-ch3-short-code.md）       exit=0
```

上一轮的 `BOOT_HANDOFF_V1` 漏网已修：neg2 现在输出 `BOOT_HANDOFF`，不再只靠 `wire/v9/v10` 撑红。三个假阳性仍未误杀。

判定：**PASS**。

## 5. fail-open 探针（上一轮全部 PASS，本轮全部按预期 FAIL）

| 探针文件 | 内容 | 上一轮 | 本轮 | 判定 |
|---|---|---|---|---|
| `/tmp/v-nochapters.md` | 无任何 `## N.` 章，含 `KernelInfo`/代码块 | PASS（错） | `[FAIL] 找不到任何 ## N. 章`，exit 1 | PASS |
| `/tmp/v-unclosed.md` | Ch1 围栏未闭合，后写 `KernelInfo`/`wire v10` | PASS（错） | `[FAIL] 围栏代码块未闭合`，exit 1 | PASS |
| `/tmp/v-bo-auth.md` | Ch1 写 `BOOT_HANDOFF_V1`、`KERNEL_INFO_V10` | PASS（错） | 命中 `BOOT_HANDOFF`、`KERNEL_INFO`，exit 1 | PASS |
| `/tmp/v-ch2-codeblock.md` | Ch2 围栏里写 `KernelInfo`、`store_boot_report` | PASS（错） | 命中两项，exit 1 | PASS |
| `/tmp/v-fieldtable.md` | Ch1 `\| 字段名 \| 类型 \|` 表 | PASS（错） | `Ch1 含字段表`，exit 1 | PASS |
| `/tmp/v-fieldtable2.md` | Ch1 缩进 `\| 字段 \| 类型 \|` 表 | PASS（错） | `Ch1 含字段表`，exit 1 | PASS |
| `/tmp/v-overwrite.md` | Ch1 后接 `## 1.1`，正文含禁用词 | PASS（错） | 命中 Ch1，exit 1 | PASS |
| `/tmp/v-preamble.md` | `## 1.` 之前写 `KernelInfo`/代码块 | PASS（错） | 前言并入 Ch1，命中，exit 1 | PASS |
| `/tmp/v-split-in-fence.md` | Ch3 的围栏内含 `## 4. 实现` + 20 行 | PASS（错） | `Ch3 有超 10 行代码块：第1块 22行`，exit 1 | PASS |
| `/tmp/v-versions.md` | Ch1 写 `V1/V9/V10` | PASS（错） | 命中 `v1、v9、v10`，exit 1 | PASS |
| `/tmp/v-wire-upper.md` | Ch1 写 `Wire` | PASS（错） | 命中 `wire`，exit 1 | PASS |
| `/tmp/v-ch4warn.md` | Ch4 写 `KernelInfo`、`设计裁决` | WARN + exit 0 | WARN + exit 0 | PASS |

判定：**PASS**。上一轮报告 §6 的九条 fail-open 全部被封。

## 6. brief 检查器

实跑：

```text
$ python3 tools/chapter-brief-check.py prompt/review-panel-chapters
chapter-brief-check: PASS（prompt/review-panel-chapters）
exit=0
```

缺角色探针（只放 `editor.md`）：

```text
[FAIL] 缺角色 brief：beginner-me.md
[FAIL] 缺角色 brief：g18.md
[FAIL] 缺角色 brief：navigator.md
[FAIL] 缺角色 brief：instructional.md
[FAIL] 缺角色 brief：reader-b.md
[FAIL] 缺效率读者 brief（reference.md 或 efficiency.md）
exit=1
```

缺 `g18.md`、缺 `DOC`、缺 `reference.md` 的探针也分别 exit 1：

```text
$ rm /tmp/v-briefs4/g18.md; python3 tools/chapter-brief-check.py /tmp/v-briefs4
[FAIL] 缺角色 brief：g18.md
exit=1

$ sed -i '/DOC:/d' /tmp/v-briefs5/editor.md; python3 tools/chapter-brief-check.py /tmp/v-briefs5
[FAIL] editor.md 缺输出头 DOC
exit=1

$ rm /tmp/v-briefs6/reference.md; python3 tools/chapter-brief-check.py /tmp/v-briefs6
[FAIL] 缺效率读者 brief（reference.md 或 efficiency.md）
exit=1
```

`reader-b.md` 缺节在上一轮已验，仍 exit 1。规范 `review-chapter-contract.md:52` 已写明“效率读者（brief 文件名 `reference.md`）”，命名漂移已消。

一个非阻塞的残留：`chapter-brief-check.py --self-test` 本身仍然只覆盖“合法目录”和“Reader-B 缺节”，没有覆盖缺角色、缺 `DOC/CHAPTER/VERDICT`、g18 缺 `HARD-REJECTIONS`。检查器功能是对的（上面的外部探针证明），但证据文件 5.1 写“`--self-test`……缺头/缺节会红”并不准确：缺头/缺角色是检查器会红，不是自测覆盖。建议补三类负样本，或把证据措辞改成“检查器会红”。

判定：**PASS（功能）/ 残留（自测覆盖）**。

## 7. outline 合同

```text
$ python3 tools/outline-contract-check.py .../outline-fixtures/good.md
outline-contract-check: PASS（.../good.md）   exit=0

$ python3 tools/outline-contract-check.py .../outline-fixtures/bad.md
[FAIL] .../bad.md: 缺 不写什么、下游导出接口、上一章回顾
exit=1

$ python3 tools/outline-contract-check.py rewrite-notes/meta/design/01-stage-kernel/01-outline.v6.md
outline-contract-check: PASS（.../01-outline.v6.md）   exit=0
```

检查器已从“正文子串存在”收紧为“`##` 标题存在”：

```text
$ printf '# O\n\n核心命题\n知识点与节序\n不写什么\n下游导出接口\n上一章回顾\n' > /tmp/v-outline-prose.md
$ python3 tools/outline-contract-check.py /tmp/v-outline-prose.md
[FAIL] /tmp/v-outline-prose.md: 缺 核心命题、不写什么、下游导出接口、上一章回顾、知识点/节序
exit=1
```

判定：**PASS**。

## 8. §5.4 章切片 G18 报告（已补齐）

任务书 §5.4 要求“用一个 chapter-slice fixture 跑一次，确认不要求全篇单元”。复核期间 `chapter-slice-g18.md` 落盘（mtime 00:35），内容如下：

```text
$ head -4 tmp/review-loop/01-stage/workflow/chapter-slice-g18.md
DOC: ch1-slice.md（Ch1 章切片 fixture）
CHAPTER: 1. 概述
HARD-REJECTIONS: 0
VERDICT: NOT-READY
```

报告正文明确写了两件事：

- “覆盖范围：本章单元 1.0、1.1、1.2、1.3、1.4，共 5 个。**本报告不要求覆盖全篇 48 单元，也不冒充全篇 G18**；整篇冷读仍需另起独立事件。”
- “本章 1.0–1.4 共 5 个单元已全部覆盖；本报告不要求覆盖全篇 48 单元。”

逐单元表也覆盖 1.0–1.4，每节给“可跟读/有卡点 + 最小修复”。这正好满足 §5.4 要确认的两点：章级 brief 只要求本章单元；不冒充全篇。

需要说明的是报告结论为 `VERDICT: NOT-READY`，原因是 fixture 本身缺 TR-3 最小示例、TR-4 失败对照、TR-5 小结、TR-6 术语定义（报告第 2 节，四项阻断）。这是 fixture 内容的教学构件缺口，不是 workflow 回归失败：本项的目的是验证“章级 G18 只覆盖本章、不要求全篇”，报告已证明这一点；fixture 的 TR 缺项应作为 D01 Ch1 改写时的输入，而不是要求这个 test fixture 先 READY。`chapter-contract-check.py` 对该 fixture 仍 PASS（CI 五条命令之一），两套判据各管一件事。

判定：**PASS**。

## 9. 非阻塞残留（不改变本轮 PASS 的归因）

1. **brief 自测覆盖**：`chapter-brief-check.py --self-test` 没有缺角色/缺头负样本，证据 5.1 的“缺头/缺节会红”应改为“检查器会红”，或补三类负样本。
2. **规范未点名 brief 工具**：`review-chapter-contract.md` 已点名 `tools/outline-contract-check.py`，但仍未点名 `tools/chapter-brief-check.py`。作为唯一规范源，建议在 §三补一句“brief schema 由 `tools/chapter-brief-check.py prompt/review-panel-chapters` 检查”。
3. **日志未刷新**：`mechanical.log`（00:15）与 `fixtures.log`（00:16）仍是第一轮的旧输出，没有本轮五条 CI 命令，也没有 neg2 新增的 `BOOT_HANDOFF` 命中；证据文件（00:30）却引用它们作为“机械日志”。本报告的命令输出可以替代，但证据文件应重定向到新日志或注明“日志为第一轮快照，第二轮由 verifier 重放”。
4. **无 Ch1 的前言边界**：`## 2.` 开头、前言含 `KernelInfo`/`wire` 的文档仍会 PASS（我另跑 `/tmp/v-no-ch1.md`，exit 0），因为 `check` 只在 `1 in chs` 时把前言并入 Ch1。规范 §1.2 写“要求至少一章”，没有要求必须有 Ch1；若要把“六章正文”作为硬合同，应在检查器里对缺失 Ch1 报错，或无条件扫描前言。当前不算违约，但属于同一类 fail-open 的边界。
5. **禁词清单有限**：Ch1 写 `BootPrepareResult`/`BootAlloc` 仍 PASS（它们在 `CH2_RUST` 有、`CH1_FORBIDDEN` 没有）。规范写的是“至少”，所以不是违约；机械门的 PASS 只能证明“清单内没命中”，不能证明“Ch1 已无项目类型”，这一点仍要靠章级六视角。

## 10. 复现命令

```bash
cd /home/xzhao/github/minix-rs

# CI 本地等价（五条，均应为 0）
python3 tools/chapter-contract-check.py --self-test
python3 tools/outline-contract-check.py --self-test
python3 tools/chapter-brief-check.py --self-test
python3 tools/chapter-brief-check.py prompt/review-panel-chapters
python3 tools/chapter-contract-check.py prompt/review-panel-chapters/fixtures/ch1-slice.md

# 历史 D01（应 exit 1）
git show 78c6373ca:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md > /tmp/v-d01.md
python3 tools/chapter-contract-check.py /tmp/v-d01.md

# fixture
for f in tmp/review-loop/01-stage/workflow/chapter-contract-fixtures/*.md; do
  python3 tools/chapter-contract-check.py "$f"
done
python3 tools/outline-contract-check.py tmp/review-loop/01-stage/workflow/outline-fixtures/good.md
python3 tools/outline-contract-check.py tmp/review-loop/01-stage/workflow/outline-fixtures/bad.md

# 缺角色 brief
rm -rf /tmp/v-briefs3 && mkdir -p /tmp/v-briefs3
cp prompt/review-panel-chapters/editor.md /tmp/v-briefs3/
python3 tools/chapter-brief-check.py /tmp/v-briefs3   # 应 exit 1
```

## 11. 仓库状态

```text
$ git status --short --untracked-files=all
 M .github/workflows/rules-consistency.yml
 M AGENTS.md
 M prompt/README.md
?? prompt/review-panel-chapters/beginner-me.md
?? prompt/review-panel-chapters/editor.md
?? prompt/review-panel-chapters/fixtures/ch1-slice.md
?? prompt/review-panel-chapters/g18.md
?? prompt/review-panel-chapters/instructional.md
?? prompt/review-panel-chapters/navigator.md
?? prompt/review-panel-chapters/reader-b.md
?? prompt/review-panel-chapters/reference.md
?? prompt/review-rules/review-chapter-contract.md
?? rewrite-notes/01-stage-kernel/01-design-learn.md
?? rewrite-notes/meta/coordination/review-eval/meta/01-boot-shim-bootstrap-r26.md
?? tools/chapter-brief-check.py
?? tools/chapter-contract-check.py
?? tools/outline-contract-check.py
```

本次复核没有修改任何仓库文件；所有探针写在 `/tmp`。第一轮的全部阻塞项已消除；§9 的五条残留是非阻塞的加固建议，不改变 PASS 判定。
