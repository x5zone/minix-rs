指向定义行：`tools/.anchor-migrate-selftest/lib.rs:fn clock_init` 应转为符号锚点（无后缀）。
指向函数内部：`tools/.anchor-migrate-selftest/lib.rs:fn clock_init（L9，工具生成）` 应带（L9，工具生成）后缀。
C 定义行：`tools/.anchor-migrate-selftest/clock.c:clock_stop` 应转为 C 符号锚点。
C 函数内部：`tools/.anchor-migrate-selftest/clock.c:clock_init（L7，工具生成）` 应带（L7，工具生成）后缀。
fenced 块内的行号是代码内容，不迁移：
```
let x = tools/.anchor-migrate-selftest/lib.rs:9;
```
文件缺失：`tools/.anchor-migrate-selftest/vanished.rs:12` 期望进无法解析清单。
无定义可寻：`tools/.anchor-migrate-selftest/lib.rs:99` 期望进无法解析清单（向上无定义）。
边界保真：（tools/.anchor-migrate-selftest/vanished.h:28 extern）与 `tools/.anchor-migrate-selftest/vanished.c:9` 的括号与反引号必须原样保留。
