正文引用 `tools/.anchor-resolve-selftest/lib.rs:fn clock_init` 应解析。
带工具派生后缀 `tools/.anchor-resolve-selftest/lib.rs:struct Clock（L6，工具生成）` 应解析。
impl 限定方法 `tools/.anchor-resolve-selftest/lib.rs:impl Clock::reset` 应解析。
impl 块锚点 `tools/.anchor-resolve-selftest/lib.rs:impl Clock` 应解析。
C 函数 `tools/.anchor-resolve-selftest/clock.c:clock_init` 应解析。
fenced 块内的伪锚点不会被抓取：
```
tools/.anchor-resolve-selftest/lib.rs:fn vanishing_in_fence
```
坏符号 `tools/.anchor-resolve-selftest/lib.rs:fn vanishing_fn` 期望 ZERO-DEF。
裸同名方法 `tools/.anchor-resolve-selftest/lib.rs:fn reset` 期望 MULTI-DEF（要求补 impl 限定）。
