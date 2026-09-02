# 07-文本过滤与数据处理

> **状态**: pending（最小骨架，待改写）
> **定位**: 文本过滤与数据处理
> **源码**: `minix3/usr.bin/35 个（head/tail/sort/tr/uniq/wc/cut/paste/join/comm/cmp/diff/sdiff/patch/col/colrm/expand/fold/hexdump/jot/lam/rev/seq/shuffle/split/csplit/tee/unexpand/unifdef/units/unvis/vis/yes/cksum/uuidgen/column…）、minix/commands/{look,ifdef,crc}/、minix/usr.bin/diff/`
> **Rust 模块**: `新 crate（待建）`
> **draft 素材**: 无（新建）

## 核心点

- - 过滤命令契约表（39 命令）：头部尾部（head/tail）、排序（sort）、去重（uniq）、统计（wc）、切割（cut/split/csplit）、列处理（column/paste/join）
- - 比较与补丁：cmp/diff/sdiff/patch（diff 在 minix/usr.bin）
- - 数据转换：tr/expand/unexpand/rev/hexdump/units/uuidgen/cksum/crc
- - 文本修饰：col/colrm/fold/tee/vis/unvis/yes/shuffle/jot/seq
- - [ARCH] A-8 关联：locale/多字节面 defer 决策

## 边界

- - **前置依赖**: 06、08（正则概念序前置）
- - **不覆盖（移交）**: 正则实现细节（08）、编辑器（09）

