// EVAL FIXTURE — 不属于内核代码，只用于 review-loop 的对抗样本。
// 场景：一个进程表查找函数。请审查设计，不要修改代码。

pub struct Entry {
    pub key: u32,
    pub value: u32,
}

pub fn lookup(table: &[Entry], key: u32) -> i32 {
    for (index, entry) in table.iter().enumerate() {
        if entry.key == key {
            return index as i32;
        }
    }
    -1
}
