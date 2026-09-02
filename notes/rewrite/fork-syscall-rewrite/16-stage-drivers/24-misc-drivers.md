# 24-misc-drivers — 杂项驱动

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

printer/eeprom(cat24c256)/sensors(bmp085/sht21/tsl2550)/iommu(amddev)/bus(i2c/ti1225)/video(tda19988)/vmm_guest(vbox)/examples(hello)/power(tps65217/tps65950/acpi) 差异矩阵 + ACPI/AML 策略（A-9）。C: 上述目录。Rust: os/drivers/*。

## 边界

- **前置依赖**: 00
- **本篇不覆盖**: 各类别核心语义（05~23）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
