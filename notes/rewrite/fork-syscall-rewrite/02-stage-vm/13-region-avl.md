# 13-region-avl: 区域映射表（BTreeMap）

> **分类**: VM私有  
> **源码**: `minix3/minix/servers/vm/regionavl.c`, `regionavl_defs.h`, `cavl_if.h`, `cavl_impl.h`, `region.h`, `region.c`  
> **Rust 实现**: `os/servers/vm/src/region/region_map.rs`  
> **说明**: Minix3 使用 AVL 树高效管理虚拟区域，Rust 实现改用 `BTreeMap<VirBytes, VirRegion>` 提供相同的 O(log n) 语义

---

## 1. 概述

**AVL 树在区域管理中的作用**

VM 需要为每个进程维护一组虚拟区域（`vir_region`），这些区域按虚拟地址排列。核心操作包括：

| 操作 | 场景 | 频率 |
|------|------|------|
| 查找包含地址的区域 | 页错误处理 | 极高 |
| 插入新区域 | mmap、brk | 高 |
| 删除区域 | munmap | 高 |
| 查找重叠区域 | mmap 冲突检测 | 高 |
| 遍历所有区域 | fork、进程终止 | 中 |

这些操作都需要高效的地址查找，Minix3 使用 AVL 树提供了 O(log n) 的保证，Rust 实现改用 `BTreeMap` 提供相同的语义。

**为什么 Minix3 选择 AVL 树**

| 特性 | AVL 树 | 红黑树 | 链表 | 哈希表 |
|------|--------|--------|------|--------|
| 查找 | O(log n) | O(log n) | O(n) | O(1)* |
| 插入 | O(log n) | O(log n) | O(n) | O(1)* |
| 删除 | O(log n) | O(log n) | O(n) | O(1)* |
| 范围查找 | O(log n + k) | O(log n + k) | O(n) | 不支持 |
| 有序遍历 | O(n) | O(n) | O(n) | 不支持 |
| 平衡严格性 | 严格（高度差 ≤ 1） | 宽松（高度差 ≤ 2） | - | - |
| 旋转次数 | 较多 | 较少 | - | - |

*哈希表不支持范围查找和有序遍历，不适合区域管理。

Minix3 选择 AVL 树的原因：

1. **查找密集型**：页错误处理是最频繁的操作，AVL 树的严格平衡保证了更矮的树高
2. **范围查询**：需要查找包含某地址的区域，AVL 树的严格平衡使查找路径更短
3. **可预测性**：AVL 树的高度严格受限于 1.44 × log₂(n)，比红黑树更可预测
4. **简单实现**：Minix3 使用 Walt Karas 的公共域 AVL 库，通过宏实现泛型

**与 Minix3 的对应关系**

| Minix3 组件 | 作用 |
|------------|------|
| `regionavl_defs.h` | 宏定义配置（泛型参数） |
| `cavl_if.h` | 接口声明（类型、函数原型） |
| `cavl_impl.h` | 实现代码（函数体） |
| `regionavl.h` | 模块入口（组合以上头文件及 unavl.h） |
| `regionavl.c` | 编译单元（include cavl_impl.h 触发实例化） |
| `region_t.lower/higher/factor` | AVL 节点字段（嵌入 vir_region） |
| `region_avl` (vmproc.h) | 树根结构体（嵌入 vmproc） |
| `region_iter` | 迭代器结构体 |

**Minix3 的 AVL 实现特点**

Minix3 使用了一种独特的**宏泛型**方式实现 AVL 树：

```c
// regionavl_defs.h - 通过宏定义"泛型参数"（部分）
#define AVL_UNIQUE(id) region_ ## id    // 函数名前缀
#define AVL_HANDLE region_t *            // 节点句柄类型
#define AVL_KEY vir_bytes                // 键类型
#define AVL_MAX_DEPTH 30                 // 最大深度
#define AVL_NULL NULL                    // 空句柄
#define AVL_GET_LESS(h, a) (h)->lower    // 获取左子节点
#define AVL_GET_GREATER(h, a) (h)->higher // 获取右子节点
#define AVL_GET_BALANCE_FACTOR(h) (h)->factor // 获取平衡因子
```

这种方式的优点是零开销抽象（纯宏展开），缺点是类型不安全、难以调试。

---

## 2. C 源码分析

### 2.1 AVL 树结构

#### 2.1.1 region_avl 类型

**树根定义**

```c
// cavl_if.h - 宏展开后的树结构
typedef struct {
    region_t *root;    // 树根指针
} region_avl;
```

树根结构非常简单，只包含一个 `root` 指针。这个结构嵌入在进程的 `vmproc` 中：

```c
// vmproc.h
struct vmproc {
    // ...
    region_avl vm_regions_avl;     // 进程的区域 AVL 树
    vir_bytes vm_region_top;       // 最高 vaddr（最近插入的）
    // ...
};
```

**节点定义**

AVL 节点直接嵌入在 `vir_region` 结构体中：

```c
// region.h
typedef struct vir_region {
    vir_bytes   vaddr;             // 虚拟地址（AVL 键）
    vir_bytes   length;            // 区域长度
    struct phys_region **physblocks; // 物理块数组
    u16_t       flags;             // 标志
    struct vmproc *parent;         // 所属进程
    mem_type_t  *def_memtype;      // 内存类型
    int         remaps;            // 共享映射计数
    int         id;                // 唯一 ID
    union {
        phys_bytes phys;           // VR_DIRECT
        struct { endpoint_t ep; vir_bytes vaddr; int id; } shared;
        struct phys_block *pb_cache;
        struct { int inited; struct fdref *fdref; u64_t offset; u16_t clearend; } file;
    } param;

    // AVL 字段 - 嵌入在节点中
    struct vir_region *lower;      // 左子节点（地址更小）
    struct vir_region *higher;     // 右子节点（地址更大）
    int               factor;     // 平衡因子 (-1, 0, 1)
} region_t;
```

**关键设计决策**：

1. **AVL 字段嵌入节点**：`lower`、`higher`、`factor` 直接在 `vir_region` 中，无需额外分配
2. **键为 vaddr**：以虚拟地址作为排序键，`AVL_KEY` 定义为 `vir_bytes`
3. **最大深度 30**：`AVL_MAX_DEPTH = 30`，源码注释为"good for 2 million nodes"（AVL 树高度 30 对应约 200 万节点）

**32 位 vs 64 位差异**

| 方面 | Minix3 (32 位) | 说明 |
|------|---------------|------|
| `vir_bytes` | `u32` (4 字节) | 32 位地址空间 |
| `lower`/`higher` 指针 | 4 字节 | 32 位指针 |
| `factor` | `int` (4 字节) | 实际只用 -1/0/1 |
| `branch` 位图 | `unsigned long` (4 字节) | 32 位 long |
| `AVL_MAX_DEPTH` | 30 | good for ~2M nodes |

**平衡因子含义**

| factor 值 | 含义 |
|-----------|------|
| -1 | 左子树比右子树高 1 |
| 0 | 左右子树等高 |
| 1 | 右子树比左子树高 1 |

**内存布局**

```
┌─────────────────────────────────────────────────────────────┐
│ region_avl (嵌入在 vmproc 中)                                │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ root ──────┐                                            │ │
│ └────────────┼────────────────────────────────────────────┘ │
│              ↓                                             │
│   region_t (vaddr=0x400000)                                │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ vaddr: 0x400000    length: 0x1000                    │   │
│   │ ... 其他字段 ...                                     │   │
│   │ lower ──────┐    higher ──────┐   factor: 0         │   │
│   └──────────────┼────────────────┼─────────────────────┘   │
│                  ↓                ↓                         │
│   region_t (0x200000)   region_t (0x600000)                │
│   ┌─────────────────┐   ┌─────────────────┐                │
│   │ factor: -1      │   │ factor: 1       │                │
│   │ lower: None     │   │ lower ──┐       │                │
│   │ higher ──┐      │   │ higher: None     │                │
│   └──────────┼──────┘   └─────────┼───────┘                │
│              ↓                    ↓                         │
│        region_t (0x300000)  region_t (0x500000)            │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

#### 2.1.2 cavl_if.h 接口

**条件编译的 AVL 实现**

Minix3 的 AVL 树使用 Walt Karas 的公共域 C AVL 库，通过宏实现泛型。该库分为两个头文件：

| 文件 | 作用 |
|------|------|
| `cavl_if.h` | 接口声明（函数原型、类型定义） |
| `cavl_impl.h` | 实现代码（函数体） |

**宏配置机制**

`regionavl_defs.h` 定义了所有宏参数，`cavl_if.h` 和 `cavl_impl.h` 通过这些宏生成类型安全的代码：

```c
// regionavl_defs.h - "泛型参数"
#define AVL_UNIQUE(id) region_ ## id     // 所有函数名加 region_ 前缀
#define AVL_HANDLE region_t *             // 节点句柄 = region_t 指针
#define AVL_KEY vir_bytes                 // 键类型 = 虚拟地址
#define AVL_MAX_DEPTH 30                  // 最大深度（good for ~2M nodes）
#define AVL_NULL NULL                     // 空句柄
```

**宏展开后的接口**

`cavl_if.h` 展开后生成以下接口：

```c
// 树操作
void region_init(region_avl *tree);                          // 初始化空树
int region_is_empty(region_avl *tree);                       // 判空
region_t *region_insert(region_avl *tree, region_t *h);      // 插入节点
region_t *region_remove(region_avl *tree, vir_bytes k);      // 删除节点
region_t *region_search(region_avl *tree, vir_bytes k,       // 搜索节点
    avl_search_type st);
region_t *region_search_least(region_avl *tree);             // 最小节点
region_t *region_search_greatest(region_avl *tree);          // 最大节点
region_t *region_search_root(region_avl *tree);              // 根节点
region_t *region_subst(region_avl *tree, region_t *new_node); // 替换节点

// 迭代器操作
void region_start_iter(region_avl *tree, region_iter *iter,  // 开始迭代
    vir_bytes k, avl_search_type st);
void region_start_iter_least(region_avl *tree,               // 从最小开始
    region_iter *iter);
void region_start_iter_greatest(region_avl *tree,            // 从最大开始
    region_iter *iter);
region_t *region_get_iter(region_iter *iter);                // 获取当前
void region_incr_iter(region_iter *iter);                    // 递增迭代
void region_decr_iter(region_iter *iter);                    // 递减迭代
void region_init_iter(region_iter *iter);                    // 初始化迭代器
```

**搜索类型**

```c
typedef enum {
    AVL_EQUAL = 1,                            // 精确匹配
    AVL_LESS = 2,                             // 小于
    AVL_GREATER = 4,                          // 大于
    AVL_LESS_EQUAL = AVL_EQUAL | AVL_LESS,    // 小于等于
    AVL_GREATER_EQUAL = AVL_EQUAL | AVL_GREATER // 大于等于
} avl_search_type;
```

**迭代器结构**

```c
typedef struct {
    region_avl *tree_;          // 被迭代的树
    unsigned long branch;       // 路径位图（0=左，1=右）
    int depth;                  // 当前深度（~0 表示无效）
    region_t *path_h[AVL_MAX_DEPTH - 1]; // 路径上的节点
} region_iter;
```

**实现掩码**

可以选择性编译特定函数，减少代码体积：

```c
#define AVL_IMPL_INIT               1        // region_init
#define AVL_IMPL_IS_EMPTY           (1 << 1) // region_is_empty
#define AVL_IMPL_INSERT             (1 << 2) // region_insert
#define AVL_IMPL_SEARCH             (1 << 3) // region_search
#define AVL_IMPL_SEARCH_LEAST       (1 << 4) // region_search_least
#define AVL_IMPL_SEARCH_GREATEST    (1 << 5) // region_search_greatest
#define AVL_IMPL_REMOVE             (1 << 6) // region_remove
#define AVL_IMPL_BUILD              (1 << 7) // region_build
#define AVL_IMPL_START_ITER         (1 << 8) // region_start_iter
#define AVL_IMPL_START_ITER_LEAST   (1 << 9) // region_start_iter_least
#define AVL_IMPL_START_ITER_GREATEST (1 << 10) // region_start_iter_greatest
#define AVL_IMPL_GET_ITER           (1 << 11) // region_get_iter
#define AVL_IMPL_INCR_ITER          (1 << 12) // region_incr_iter
#define AVL_IMPL_DECR_ITER          (1 << 13) // region_decr_iter
#define AVL_IMPL_INIT_ITER          (1 << 14) // region_init_iter
#define AVL_IMPL_SUBST              (1 << 15) // region_subst
#define AVL_IMPL_ALL                (~0)      // 全部实现
```

### 2.2 核心操作

#### 2.2.1 region_init - 初始化 AVL 树

**源码位置**: [`cavl_impl.h`](../../../minix3/minix/servers/vm/cavl_impl.h)

```c
L__SC void L__(init)(L__(avl) *L__tree) {
    AVL_SET_ROOT(L__tree, AVL_NULL);
}
```

宏展开后：

```c
void region_init(region_avl *tree) {
    tree->root = NULL;
}
```

**分析**：

初始化非常简单，只需将根指针设为 NULL。这通常在进程创建时调用：

```c
// 进程初始化时
region_init(&vmp->vm_regions_avl);
```

#### 2.2.2 region_insert - 插入区域

**源码位置**: [`cavl_impl.h`](../../../minix3/minix/servers/vm/cavl_impl.h)

插入操作是 AVL 树最复杂的部分，需要：

1. 找到插入位置（BST 搜索）
2. 插入新节点
3. 更新平衡因子
4. 必要时旋转恢复平衡

**核心流程**

```c
region_t *region_insert(region_avl *tree, region_t *h)
{
    // 1. 初始化新节点
    h->lower = NULL;
    h->higher = NULL;
    h->factor = 0;

    // 2. 空树直接作为根
    if (tree->root == NULL) {
        tree->root = h;
        return h;
    }

    // 3. 搜索插入位置，记录最后一个不平衡节点
    region_t *unbal = NULL;         // 最后一个不平衡祖先
    region_t *parent_unbal = NULL;  // 不平衡节点的父节点
    int unbal_bf;
    unsigned depth = 0, unbal_depth = 0;
    unsigned long branch = 0;       // 路径位图

    region_t *hh = tree->root;
    region_t *parent = NULL;
    int cmp;

    do {
        if (hh->factor != 0) {     // 记录最后一个 factor != 0 的节点
            unbal = hh;
            parent_unbal = parent;
            unbal_depth = depth;
        }
        cmp = (h->vaddr > hh->vaddr ? 1 : (h->vaddr < hh->vaddr ? -1 : 0));
        if (cmp == 0) return hh;   // 重复键，返回已有节点
        parent = hh;
        if (cmp > 0) {
            hh = hh->higher;
            branch |= (1UL << depth);
        } else {
            hh = hh->lower;
        }
        depth++;
    } while (hh != NULL);

    // 4. 插入为叶子节点
    if (cmp < 0) parent->lower = h;
    else         parent->higher = h;

    // 5. 从 unbal 到新节点的路径上更新平衡因子
    // ... (省略平衡因子更新逻辑)

    // 6. 如果需要，旋转恢复平衡
    if (unbal != NULL) {
        unbal = balance(unbal);     // 执行旋转
        // 将旋转后的子树重新连接到父节点
        if (parent_unbal == NULL)
            tree->root = unbal;
        else if (/* 左子树 */)
            parent_unbal->lower = unbal;
        else
            parent_unbal->higher = unbal;
    }

    return h;
}
```

**平衡函数 - 旋转操作**

```c
static region_t *balance(region_t *bal_h)
{
    region_t *deep_h;

    if (bal_h->factor > 0) {
        // 右子树更深
        deep_h = bal_h->higher;

        if (deep_h->factor < 0) {
            // RL 旋转（先右旋后左旋）
            region_t *old_h = bal_h;
            bal_h = deep_h->lower;         // 新根

            old_h->higher = bal_h->lower;  // 旋转
            deep_h->lower = bal_h->higher;
            bal_h->lower = old_h;
            bal_h->higher = deep_h;

            // 更新平衡因子
            // ...
        } else {
            // RR 旋转（单左旋）
            bal_h->higher = deep_h->lower;
            deep_h->lower = bal_h;
            // 更新平衡因子
            // ...
            bal_h = deep_h;
        }
    } else {
        // 左子树更深（对称情况）
        // LL 旋转（单右旋）或 LR 旋转（先左旋后右旋）
        // ...
    }

    return bal_h;
}
```

**四种旋转情况**

```
┌─────────────────────────────────────────────────────────────┐
│                    AVL 旋转情况                              │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│ 1. LL 旋转（右旋）：                                         │
│    插入在左子树的左子树                                       │
│                                                             │
│        B(1)              A(0)                               │
│       / \               /   \                               │
│      A   T3    →      AL     B                              │
│     / \               / \   / \                             │
│    AL  AR            ...  ... AR  T3                        │
│                                                             │
│ 2. RR 旋转（左旋）：                                         │
│    插入在右子树的右子树                                       │
│                                                             │
│    A(-1)               B(0)                                 │
│   / \                /   \                                  │
│  T1   B      →      A     BR                                │
│      / \           / \   / \                                │
│     BL  BR       T1 BL ... ...                              │
│                                                             │
│ 3. LR 旋转（先左旋后右旋）：                                  │
│    插入在左子树的右子树                                       │
│                                                             │
│      C(1)             B(0)                                  │
│     / \              /   \                                  │
│    A   T4   →      A       C                                │
│   / \              / \     / \                              │
│  T1  B           T1  BL  BR  T4                             │
│     / \                                                  │
│    BL  BR                                                │
│                                                             │
│ 4. RL 旋转（先右旋后左旋）：                                  │
│    插入在右子树的左子树                                       │
│                                                             │
│    A(-1)             B(0)                                   │
│   / \               /   \                                   │
│  T1   C    →      A       C                                 │
│      / \         / \     / \                                │
│     B   T4     T1  BL  BR  T4                               │
│    / \                                                   │
│   BL  BR                                                 │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**插入流程总结**

1. 初始化新节点（lower=NULL, higher=NULL, factor=0）
2. 空树 → 直接设为根
3. BST 搜索插入位置，记录最后一个 factor != 0 的祖先（unbal）和路径位图（branch），重复键 → 返回已有节点
4. 插入为叶子节点
5. 从 unbal 到新节点更新平衡因子
6. |factor| == 2 → 旋转恢复平衡（LL: 右旋, RR: 左旋, LR: 先左旋后右旋, RL: 先右旋后左旋）
7. 返回新插入的节点

#### 2.2.3 region_remove - 删除区域

**源码位置**: [`cavl_impl.h`](../../../minix3/minix/servers/vm/cavl_impl.h)

删除操作比插入更复杂，因为：

1. 删除内部节点需要找后继（或前驱）替代
2. 删除后可能需要多次旋转恢复平衡（插入最多一次旋转）

**核心流程**

```c
region_t *region_remove(region_avl *tree, vir_bytes key)
{
    // 1. 搜索要删除的节点
    region_t *target = NULL;
    region_t *hh = tree->root;
    unsigned depth = 0;
    unsigned long branch = 0;

    while (hh != NULL) {
        int cmp = (key > hh->vaddr ? 1 : (key < hh->vaddr ? -1 : 0));
        if (cmp == 0) {
            target = hh;    // 找到目标
            break;
        }
        if (cmp > 0) {
            hh = hh->higher;
            branch |= (1UL << depth);
        } else {
            hh = hh->lower;
        }
        depth++;
    }

    if (target == NULL) return NULL;    // 未找到

    // 2. 如果目标有两个子节点，找中序后继替代
    if (target->lower != NULL && target->higher != NULL) {
        region_t *successor = target->higher;
        while (successor->lower != NULL)
            successor = successor->lower;

        // 交换键值（或替换节点）
        // Minix3 使用 subst 操作替换节点
    }

    // 3. 删除节点（最多一个子节点）
    region_t *child = (target->lower != NULL) ? target->lower : target->higher;

    // 将子节点连接到父节点
    // ...

    // 4. 从删除点到根路径上更新平衡因子
    // 删除可能导致多次旋转（不像插入最多一次）

    // 5. 返回被删除的节点
    return target;
}
```

**删除的平衡调整**

与插入不同，删除可能需要 O(log n) 次旋转：

- **插入**：最多 1 次旋转（单旋或双旋），旋转后子树高度不变，上层不受影响
- **删除**：最多 O(log n) 次旋转，旋转后子树高度可能减 1，上层可能继续不平衡，需要从删除点向上回溯到根

**Minix3 的 subst 替代操作**

Minix3 使用 `region_subst` 替代直接删除+重插入：

```c
// cavl_impl.h - region_subst 宏展开后
region_t *region_subst(region_avl *tree, region_t *new_node)
{
    region_t *h = tree->root;
    region_t *parent = NULL;
    int cmp, last_cmp = 0;

    // 搜索与 new_node 相同键的节点（内联搜索，不调用 region_search）
    for ( ; ; ) {
        if (h == NULL)
            return NULL;    // 无同键节点
        cmp = (new_node->vaddr > h->vaddr ? 1 :
               (new_node->vaddr < h->vaddr ? -1 : 0));
        if (cmp == 0)
            break;          // 找到
        last_cmp = cmp;
        parent = h;
        h = cmp < 0 ? h->lower : h->higher;
    }

    // 复制 AVL 链接到新节点
    new_node->lower = h->lower;
    new_node->higher = h->higher;
    new_node->factor = h->factor;

    // 替换父节点的引用
    if (parent == NULL) {
        tree->root = new_node;     // 新节点成为根
    } else {
        if (last_cmp < 0)
            parent->lower = new_node;
        else
            parent->higher = new_node;
    }

    return h;    // 返回被替换的旧节点
}
```

这在区域调整大小时很有用：保持键不变，只替换节点内容。

**删除流程总结**

1. BST 搜索目标节点，未找到 → 返回 NULL
2. 目标有两个子节点 → 从更深子树找替代叶子节点
3. 将唯一子节点（或 NULL）连接到父节点
4. 如果替代节点不是目标本身，将替代节点"塞入"目标位置（复制 lower/higher/factor）
5. 从替代节点的父节点到根，更新平衡因子并旋转恢复平衡（可能需要多次旋转）
6. 返回被删除的节点

#### 2.2.4 region_search - 搜索区域

**源码位置**: [`cavl_impl.h`](../../../minix3/minix/servers/vm/cavl_impl.h)

查找是 AVL 树最频繁的操作，页错误处理时每次都需要查找包含目标地址的区域。

**精确查找**

```c
// cavl_impl.h - region_search 宏展开后
region_t *region_search(region_avl *tree, vir_bytes key, avl_search_type st)
{
    int cmp, target_cmp;
    region_t *match_h = NULL;
    region_t *h = tree->root;

    // 确定搜索方向
    if (st & AVL_LESS)
        target_cmp = 1;         // 允许键大于节点
    else if (st & AVL_GREATER)
        target_cmp = -1;        // 允许键小于节点
    else
        target_cmp = 0;         // 必须精确匹配

    while (h != NULL) {
        cmp = (key > h->vaddr ? 1 : (key < h->vaddr ? -1 : 0));
        if (cmp == 0) {
            if (st & AVL_EQUAL) {
                match_h = h;     // 精确匹配
                break;
            }
            cmp = -target_cmp;   // 精确匹配但不需要，继续搜索
        }
        else if (target_cmp != 0)
            if (!((cmp ^ target_cmp) & L__MASK_HIGH_BIT))
                // cmp 和 target_cmp 同号，记录候选
                match_h = h;

        h = cmp < 0 ? h->lower : h->higher;
    }

    return match_h;
}
```

**区域包含查找**

VM 最常用的查找是"找到包含某地址的区域"。这需要使用 `AVL_LESS_EQUAL` 搜索：

```c
// region.c:616 - map_lookup
struct vir_region *map_lookup(struct vmproc *vmp,
    vir_bytes offset, struct phys_region **physr)
{
    struct vir_region *r;

    SANITYCHECK(SCL_FUNCTIONS);

#if SANITYCHECKS
    if(!region_search_root(&vmp->vm_regions_avl))
        panic("process has no regions: %d", vmp->vm_endpoint);
#endif

    if((r = region_search(&vmp->vm_regions_avl, offset, AVL_LESS_EQUAL))) {
        vir_bytes ph;
        if(offset >= r->vaddr && offset < r->vaddr + r->length) {
            ph = offset - r->vaddr;
            if(physr) {
                *physr = physblock_get(r, ph);
                if(*physr) assert((*physr)->offset == ph);
            }
            return r;
        }
    }

    SANITYCHECK(SCL_FUNCTIONS);

    return NULL;
}
```

**查找逻辑图**

```
┌─────────────────────────────────────────────────────────────┐
│              区域查找逻辑                                    │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   查找地址 v = 0x401000                                     │
│                                                             │
│   AVL 树:                                                   │
│              0x600000 (length=0x2000)                        │
│              /              \                                │
│     0x200000 (len=0x1000)   0x800000 (len=0x1000)          │
│              \                                              │
│              0x400000 (len=0x3000) ← 候选                   │
│                                                             │
│   步骤 1: region_search(v=0x401000, AVL_LESS_EQUAL)        │
│   - 0x401000 < 0x600000 → 走左子树                         │
│   - 0x401000 > 0x200000 → 走右子树                         │
│   - 0x401000 > 0x400000 → 匹配 (LESS_EQUAL)               │
│   → 返回 0x400000 区域                                      │
│                                                             │
│   步骤 2: 检查范围                                          │
│   - 0x400000 <= 0x401000 < 0x400000+0x3000 = 0x403000     │
│   - ✅ 在范围内，返回该区域                                  │
│                                                             │
│   ─────────────────────────────────────────────────────     │
│                                                             │
│   查找地址 v = 0x500000                                     │
│                                                             │
│   步骤 1: region_search(v=0x500000, AVL_LESS_EQUAL)        │
│   - 0x500000 < 0x600000 → 走左子树                         │
│   - 0x500000 > 0x200000 → 走右子树                         │
│   - 0x500000 > 0x400000 → 匹配 (LESS_EQUAL)               │
│   → 返回 0x400000 区域                                      │
│                                                             │
│   步骤 2: 检查范围                                          │
│   - 0x400000 <= 0x500000 但 0x500000 >= 0x403000          │
│   - ❌ 不在范围内，返回 NULL                                │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**搜索类型的使用场景**

| 搜索类型 | 场景 | 说明 |
|---------|------|------|
| `AVL_EQUAL` | 精确查找 | 查找指定 vaddr 的区域 |
| `AVL_LESS_EQUAL` | 页错误处理 | 查找包含某地址的区域 |
| `AVL_GREATER` | 冲突检测 | 查找下一个区域 |
| `AVL_LESS` | 前驱查找 | 查找前一个区域 |

**性能分析**

| 操作 | 时间复杂度 | 说明 |
|------|-----------|------|
| 精确查找 | O(log n) | BST 搜索 |
| 范围查找 | O(log n) | BST 搜索 + 范围检查 |
| 最小/最大 | O(log n) | 沿左/右子树到底 |
| 前驱/后继 | O(log n) | 使用迭代器 |

### 2.3 迭代器

#### 2.3.1 region_iter - 迭代器结构

**源码位置**: [`cavl_if.h`](../../../minix3/minix/servers/vm/cavl_if.h)

```c
typedef struct {
    region_avl *tree_;                          // 被迭代的树
    unsigned long branch;                       // 路径位图（0=左，1=右）
    int depth;                                  // 当前深度（~0 表示无效）
    region_t *path_h[AVL_MAX_DEPTH - 1];       // 路径上的节点栈
} region_iter;
```

**字段分析**

| 字段 | 类型 | 说明 |
|------|------|------|
| `tree_` | `region_avl*` | 指向被迭代的树 |
| `branch` | `unsigned long` | 路径位图，第 i 位表示第 i 层走左(0)还是右(1) |
| `depth` | `int` | 当前深度，`~0`(即 -1) 表示迭代器无效 |
| `path_h` | `region_t*[]` | 从根到当前节点的路径栈，最多 29 层 |

**路径位图的工作原理**

```
┌─────────────────────────────────────────────────────────────┐
│              路径位图 (branch) 示例                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   AVL 树:                                                   │
│              D (depth=0)                                    │
│             / \                                             │
│            B   F (depth=1)                                  │
│           / \ / \                                           │
│          A  C E  G (depth=2)                                │
│                                                             │
│   迭代到节点 E 时:                                          │
│   - depth = 2                                               │
│   - path_h[0] = D (根)                                     │
│   - path_h[1] = F (右子树)                                 │
│   - branch = 0b10 (bit0=0: D→B方向, bit1=1: F→E方向)       │
│     实际: bit0=1 表示根走右到 F                             │
│     branch = (1 << 0) | (0 << 1) = 0b01                    │
│                                                             │
│   注意: branch 的第 i 位 = 1 表示第 i 层走向右子树          │
│                                                             │
│   迭代到节点 A 时:                                          │
│   - depth = 2                                               │
│   - path_h[0] = D                                          │
│   - path_h[1] = B                                          │
│   - branch = 0b00 (全走左)                                  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**迭代器状态**

| depth 值 | 状态 | 说明 |
|----------|------|------|
| `~0` (-1) | 无效 | 未初始化或已耗尽 |
| 0 | 根节点 | `path_h` 为空 |
| > 0 | 内部 | `path_h[0..depth-1]` 为路径 |

**使用场景**

迭代器在以下场景使用：

| 场景 | 函数 | 说明 |
|------|------|------|
| fork | `map_copy_region` | 遍历所有区域复制 |
| 进程终止 | `map_free_proc` | 遍历所有区域释放 |
| 调试 | `map_sanitycheck` | 遍历检查一致性 |
| munmap | `map_unmap_region` | 查找重叠区域 |

#### 2.3.2 region_start - 开始迭代

**源码位置**: [`cavl_impl.h`](../../../minix3/minix/servers/vm/cavl_impl.h)

Minix3 提供三种开始迭代的方式：

**从最小节点开始**

```c
// cavl_impl.h - region_start_iter_least 宏展开后
void region_start_iter_least(region_avl *tree, region_iter *iter)
{
    region_t *h = tree->root;

    iter->tree_ = tree;
    iter->depth = ~0;       // 初始化为无效

    L__BIT_ARR_ALL(iter->branch, 0)  // branch 全部置 0

    while (h != NULL) {
        if (iter->depth != ~0)
            iter->path_h[iter->depth] = h;  // 跳过第一次（depth=-1）
        iter->depth++;
        h = h->lower;
    }
}
```

**关键细节**：第一次循环时 `depth == ~0`（-1），跳过 `path_h` 写入，因为此时还没有有效的路径条目。之后每次循环先将当前节点存入 `path_h[depth]`，再递增 `depth`。

**从最大节点开始**

```c
// cavl_impl.h - region_start_iter_greatest 宏展开后
void region_start_iter_greatest(region_avl *tree, region_iter *iter)
{
    region_t *h = tree->root;

    iter->tree_ = tree;
    iter->depth = ~0;

    L__BIT_ARR_ALL(iter->branch, 1)  // branch 全部置 1

    while (h != NULL) {
        if (iter->depth != ~0)
            iter->path_h[iter->depth] = h;
        iter->depth++;
        h = h->higher;
    }
}
```

**从指定键开始**

```c
// cavl_impl.h - region_start_iter 宏展开后
void region_start_iter(region_avl *tree, region_iter *iter,
    vir_bytes key, avl_search_type st)
{
    region_t *h = tree->root;
    unsigned d = 0;
    int cmp, target_cmp;

    iter->tree_ = tree;
    iter->depth = ~0;

    if (h == NULL)
        return;    // 空树

    // 确定搜索方向
    if (st & AVL_LESS)
        target_cmp = 1;     // 允许键大于节点
    else if (st & AVL_GREATER)
        target_cmp = -1;    // 允许键小于节点
    else
        target_cmp = 0;     // 必须精确匹配

    for ( ; ; ) {
        cmp = (key > h->vaddr ? 1 : (key < h->vaddr ? -1 : 0));
        if (cmp == 0) {
            if (st & AVL_EQUAL) {
                iter->depth = d;    // 找到精确匹配
                break;
            }
            cmp = -target_cmp;      // 精确匹配但不需要，继续搜索
        }
        else if (target_cmp != 0)
            if (!((cmp ^ target_cmp) & L__MASK_HIGH_BIT))
                iter->depth = d;    // cmp 和 target_cmp 同号，记录候选

        h = cmp < 0 ? h->lower : h->higher;
        if (h == NULL)
            break;
        if (cmp > 0)
            iter->branch |= (1UL << d);  // 标记走右
        else
            iter->branch &= ~(1UL << d); // 标记走左
        iter->path_h[d++] = h;
    }
}
```

**迭代器初始化流程**

```
┌─────────────────────────────────────────────────────────────┐
│          region_start_iter_least 流程                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   AVL 树:                                                   │
│              D                                              │
│             / \                                             │
│            B   F                                            │
│           / \ / \                                           │
│          A  C E  G                                          │
│                                                             │
│   start_iter_least:                                         │
│   1. h = D (根), depth = -1                                 │
│   2. D->lower != NULL → path_h[0] = D, depth=0, h = B      │
│   3. B->lower != NULL → path_h[1] = B, depth=1, h = A      │
│   4. A->lower == NULL → 停止                                │
│                                                             │
│   结果:                                                     │
│   - 当前节点: A (最小)                                      │
│   - path_h[0] = D, path_h[1] = B                           │
│   - depth = 1                                               │
│   - branch = 0 (全走左)                                     │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**VM 中的使用**

```c
// 遍历进程所有区域
region_iter iter;
region_t *r;

for (r = region_start_iter_least(&vmp->vm_regions_avl, &iter);
     r != NULL;
     r = region_incr_iter(&iter)) {
    // 处理每个区域
    map_sanitycheck(__FILE__, __LINE__);
}
```

#### 2.3.3 region_next - 下一个节点

**源码位置**: [`cavl_impl.h`](../../../minix3/minix/servers/vm/cavl_impl.h)

`region_incr_iter` 实现中序遍历的递增操作，`region_decr_iter` 实现递减操作。

**递增迭代（中序后继）**

```c
// cavl_impl.h - region_incr_iter 宏展开后
void region_incr_iter(region_iter *iter)
{
    if (iter->depth != ~0) {
        // 获取当前节点的右子节点
        region_t *h = (iter->depth == 0 ?
            iter->tree_->root : iter->path_h[iter->depth - 1])->higher;

        if (h == NULL) {
            // 无右子树：回溯到第一个"从左子树返回"的祖先
            do {
                if (iter->depth == 0) {
                    iter->depth = ~0;   // 已到最末
                    break;
                }
                iter->depth--;
            } while (iter->branch & (1UL << iter->depth));
            // branch bit = 1 表示从右子树来，继续回溯
            // branch bit = 0 表示从左子树来，停止
        } else {
            // 有右子树：进入右子树，沿左边界到底
            iter->branch |= (1UL << iter->depth);
            iter->path_h[iter->depth++] = h;
            for ( ; ; ) {
                h = h->lower;
                if (h == NULL) break;
                iter->branch &= ~(1UL << iter->depth);
                iter->path_h[iter->depth++] = h;
            }
        }
    }
}
```

**中序遍历原理**

```
┌─────────────────────────────────────────────────────────────┐
│              中序遍历 (递增迭代)                              │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   AVL 树:                  中序: A, B, C, D, E, F, G       │
│              D                                              │
│             / \                                             │
│            B   F                                            │
│           / \ / \                                           │
│          A  C E  G                                          │
│                                                             │
│   当前 = A, 下一个 = B:                                     │
│   - A 无右子树                                              │
│   - 回溯: depth=2→1, branch bit=0 (从左返回)               │
│   - 停在 B                                                  │
│                                                             │
│   当前 = B, 下一个 = C:                                     │
│   - B 有右子树 C                                           │
│   - 进入右子树，走到最左 = C                                │
│                                                             │
│   当前 = C, 下一个 = D:                                     │
│   - C 无右子树                                              │
│   - 回溯: depth=2→1→0, branch bit=1,1 → 继续回溯          │
│   - branch bit=0 at depth=0 → 停在 D                       │
│                                                             │
│   当前 = D, 下一个 = E:                                     │
│   - D 有右子树 F                                           │
│   - 进入右子树，走到最左 = E                                │
│                                                             │
│   当前 = G, 下一个 = NULL:                                  │
│   - G 无右子树                                              │
│   - 回溯: 所有 branch bit = 1 → depth < 0 → 结束          │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**递减迭代（中序前驱）**

```c
// cavl_impl.h - region_decr_iter 宏展开后
void region_decr_iter(region_iter *iter)
{
    if (iter->depth != ~0) {
        // 获取当前节点的左子节点
        region_t *h = (iter->depth == 0 ?
            iter->tree_->root : iter->path_h[iter->depth - 1])->lower;

        if (h == NULL) {
            // 无左子树：回溯到第一个"从右子树返回"的祖先
            do {
                if (iter->depth == 0) {
                    iter->depth = ~0;
                    break;
                }
                iter->depth--;
            } while (!(iter->branch & (1UL << iter->depth)));
            // branch bit = 0 表示从左子树来，继续回溯
            // branch bit = 1 表示从右子树来，停止
        } else {
            // 有左子树：进入左子树，沿右边界到底
            iter->branch &= ~(1UL << iter->depth);
            iter->path_h[iter->depth++] = h;
            for ( ; ; ) {
                h = h->higher;
                if (h == NULL) break;
                iter->branch |= (1UL << iter->depth);
                iter->path_h[iter->depth++] = h;
            }
        }
    }
}
```

**获取当前节点**

```c
// cavl_impl.h - region_get_iter 宏展开后
region_t *region_get_iter(region_iter *iter)
{
    if (iter->depth == ~0)
        return NULL;           // 无效迭代器

    return(iter->depth == 0 ?
        iter->tree_->root :    // depth=0 时当前节点就是根
        iter->path_h[iter->depth - 1]);  // path_h 直接存储当前节点
}
```

**注意**：`path_h` 数组存储的是路径上的节点本身（不是父节点）。`depth=0` 时当前节点是根节点，直接从 `tree_->root` 获取；`depth>0` 时，`path_h[depth-1]` 就是当前节点。

---

## 3. Rust 设计决策：使用 BTreeMap 替代自定义 AVL 树

### 3.1 为什么选择 BTreeMap

**Minix3 的 AVL 实现问题**

Minix3 的 AVL 树将 `lower`/`higher`/`factor` 三个字段嵌入 `vir_region` 结构体，使用宏泛型（`cavl_if.h`/`cavl_impl.h`）生成类型特化的代码。这种方式在 C 中可行，但在 Rust 中存在以下问题：

1. **侵入式节点**：`lower`/`higher` 字段使 `VirRegion` 无法独立于树结构存在，违反关注点分离
2. **所有权复杂**：`Option<Box<VirRegion>>` 的树形所有权使得节点摘出/重挂接操作繁琐
3. **平衡维护复杂**：AVL 树的旋转操作在安全 Rust 中实现冗长，且当前实现尚未完成平衡维护
4. **标准库可用**：`alloc::collections::BTreeMap` 在 `no_std` + `alloc` 环境下可用，提供 O(log n) 的有序映射

**BTreeMap 的优势**

| 方面 | 自实现 AVL | BTreeMap |
|------|-----------|----------|
| 节点嵌入 | `lower`/`higher`/`factor` 内嵌 VirRegion | VirRegion 无树字段，关注点分离 |
| 查找复杂度 | O(log n) | O(log n) |
| 插入/删除 | 需手动维护平衡 | 标准库自动维护 |
| 代码量 | ~400 行（含未完成的平衡逻辑） | ~200 行（RegionMap 封装） |
| 迭代器 | 需自己实现 | 标准库提供 |
| 内存安全 | 需仔细处理 Box 所有权 | 标准库保证 |
| 测试负担 | 高（需测试旋转、平衡等） | 低（只需测试封装层） |
| 缓存局部性 | 差（节点分散） | 较好（B 树节点内多元素） |

**BTreeMap 能满足所有需求**

| Minix3 AVL 操作 | BTreeMap 对应 | 说明 |
|----------------|--------------|------|
| `region_insert(tree, r)` | `map.insert(r.vaddr, r)` | 以 vaddr 为键插入 |
| `region_remove(tree, key)` | `map.remove(&key)` | 按键删除 |
| `region_search(tree, key, AVL_LESS_EQUAL)` | `map.range(..=key).next_back()` + 范围检查 | 查找 ≤ key 的最大区域 |
| `region_search(tree, key, AVL_GREATER)` | `map.range(Bound::Excluded(key), ..).next()` | 查找 > key 的最小区域 |
| `region_start_iter_least` | `map.iter().next()` | 最小元素 |
| `region_start_iter_greatest` | `map.iter().next_back()` | 最大元素 |
| `region_incr_iter` | `Iterator::next` | 中序递增 |
| `region_find_slot` | 遍历间隙 | 自定义逻辑 |

**关键：地址包含查找的实现**

Minix3 最常用的操作是"查找包含指定地址的区域"（`map_lookup`），即找到 `vaddr <= addr < vaddr + length` 的区域。BTreeMap 的实现：

```rust
pub(crate) fn find(&self, addr: VirBytes) -> Option<&VirRegion> {
    self.regions
        .range(..=addr)          // 所有键 ≤ addr 的条目
        .next_back()             // 取最大的那个
        .filter(|(_, r)| r.contains_addr(addr))  // 验证 addr 在区域内
        .map(|(_, r)| r)
}
```

这等价于 Minix3 的 `region_search(tree, addr, AVL_LESS_EQUAL)` + 范围检查，时间复杂度 O(log n)。

### 3.2 RegionMap 结构

**设计目标**

1. **封装 BTreeMap**：隐藏 BTreeMap 操作细节，提供 VM 语义的 API
2. **类型安全**：利用 Rust 类型系统避免 C 的指针错误
3. **所有权清晰**：VirRegion 不再内嵌树节点字段，所有权由 BTreeMap 管理

**结构定义**

```rust
use alloc::collections::BTreeMap;
use minix_types::VirBytes;
use super::vir_region::VirRegion;

/// 区域映射表
///
/// 管理进程的虚拟区域集合，按虚拟地址排序。
/// 对应 Minix3: `region_avl` 结构体，但使用 BTreeMap 替代自定义 AVL 树。
#[derive(Debug, Default)]
pub(crate) struct RegionMap {
    regions: BTreeMap<VirBytes, VirRegion>,
}
```

**与 C 的对比**

| C 实现 | Rust 实现 |
|--------|----------|
| `region_avl { root: region_t* }` | `RegionMap { regions: BTreeMap<VirBytes, VirRegion> }` |
| NULL 表示空树 | 空的 BTreeMap |
| 手动内存管理 | 自动 Drop |
| 悬垂指针风险 | 编译期检查 |
| `lower`/`higher`/`factor` 内嵌节点 | 无树字段，关注点分离 |

**核心 API**

```rust
impl RegionMap {
    pub(crate) fn new() -> Self;
    pub(crate) fn len(&self) -> usize;
    pub(crate) fn is_empty(&self) -> bool;

    /// 查找包含指定地址的区域（对应 Minix3: map_lookup）
    pub(crate) fn find(&self, addr: VirBytes) -> Option<&VirRegion>;
    pub(crate) fn find_mut(&mut self, addr: VirBytes) -> Option<&mut VirRegion>;

    /// 通用搜索（对应 Minix3: region_search）
    pub(crate) fn search(&self, key: VirBytes, st: SearchType) -> Option<&VirRegion>;
    pub(crate) fn find_less(&self, key: VirBytes) -> Option<&VirRegion>;
    pub(crate) fn find_greater(&self, key: VirBytes) -> Option<&VirRegion>;
    pub(crate) fn find_less_equal(&self, key: VirBytes) -> Option<&VirRegion>;
    pub(crate) fn find_greater_equal(&self, key: VirBytes) -> Option<&VirRegion>;

    /// 重叠查找（对应 Minix3: find_overlap）
    pub(crate) fn find_overlap(&self, start: VirBytes, end: VirBytes) -> Option<&VirRegion>;
    pub(crate) fn find_all_overlaps<'a>(&'a self, start: VirBytes, end: VirBytes) -> impl Iterator<Item = &'a VirRegion>;

    /// 空闲槽位查找（对应 Minix3: region_find_slot）
    pub(crate) fn find_slot(&self, minv: VirBytes, maxv: VirBytes, length: VirBytes) -> Option<VirBytes>;

    /// 插入/删除/遍历
    pub(crate) fn insert(&mut self, region: VirRegion);
    pub(crate) fn remove(&mut self, addr: VirBytes) -> Option<VirRegion>;
    pub(crate) fn traverse<F>(&self, f: F) where F: FnMut(&VirRegion);
    pub(crate) fn iter(&self) -> impl Iterator<Item = &VirRegion>;
    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut VirRegion>;
    pub(crate) fn clear(&mut self);
}
```

### 3.3 SearchType 搜索类型

保留 Minix3 的搜索类型语义，便于对照：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SearchType(u8);

impl SearchType {
    pub(crate) const EQUAL: Self = Self(1);
    pub(crate) const LESS: Self = Self(2);
    pub(crate) const GREATER: Self = Self(4);
    pub(crate) const LESS_EQUAL: Self = Self(3);    // EQUAL | LESS
    pub(crate) const GREATER_EQUAL: Self = Self(5);  // EQUAL | GREATER

    pub(crate) fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }
}
```

与 Minix3 的对应：

| Minix3 常量 | Rust 常量 | 值 |
|-------------|----------|-----|
| `AVL_EQUAL` | `SearchType::EQUAL` | 1 |
| `AVL_LESS` | `SearchType::LESS` | 2 |
| `AVL_GREATER` | `SearchType::GREATER` | 4 |
| `AVL_LESS_EQUAL` | `SearchType::LESS_EQUAL` | 3 |
| `AVL_GREATER_EQUAL` | `SearchType::GREATER_EQUAL` | 5 |

### 3.4 VirRegion 的变化

使用 BTreeMap 后，`VirRegion` 不再需要 AVL 树字段：

```rust
pub(crate) struct VirRegion {
    pub vaddr: VirBytes,
    pub length: VirBytes,
    pub physblocks: Vec<Option<PageSlot>>,
    pub flags: VrFlags,
    pub parent_slot: Option<UserSlot>,
    pub def_memtype: Option<&'static dyn MemType>,
    pub remaps: i32,
    pub id: i32,
    pub param: VrParam,
    // 注意：不再有 lower/higher/factor 字段
}
```

**移除的字段**：

| 字段 | C 类型 | 原用途 | 移除原因 |
|------|--------|--------|---------|
| `lower` | `struct vir_region*` | AVL 左子节点 | BTreeMap 外部管理节点关系 |
| `higher` | `struct vir_region*` | AVL 右子节点 | BTreeMap 外部管理节点关系 |
| `factor` | `int` | AVL 平衡因子 | BTreeMap 内部维护平衡 |

**好处**：
- `VirRegion` 更纯粹，只包含区域本身的语义字段
- 无需在 `VirRegion::new()` 中初始化树节点字段
- 区域可以在不同数据结构间移动，不受树节点约束
- `Debug` 输出更清晰，不再包含树结构信息

### 3.5 进程结构的变化

```rust
// 旧设计
pub(crate) struct VmProc {
    pub(crate) vm_regions_avl: MaybeUninit<RegionAvl>,
    pub(crate) vm_regions_avl_initialized: bool,
    // ...
}

// 新设计
pub(crate) struct VmProc {
    pub(crate) vm_regions: MaybeUninit<RegionMap>,
    pub(crate) vm_regions_initialized: bool,
    // ...
}
```

API 调用方式不变，只是类型名从 `RegionAvl` 变为 `RegionMap`：

```rust
// 旧
active.regions_mut().insert(new_region);  // RegionAvl::insert
active.regions_mut().remove(vaddr);       // RegionAvl::remove

// 新
active.regions_mut().insert(new_region);  // RegionMap::insert
active.regions_mut().remove(vaddr);       // RegionMap::remove
```

### 3.6 操作语义对照

| Minix3 C 操作 | Rust RegionMap 操作 | 说明 |
|--------------|--------------------|----|
| `region_init(&tree)` | `RegionMap::new()` | 创建空映射表 |
| `region_insert(&tree, r)` | `map.insert(r)` | 以 vaddr 为键插入 |
| `region_remove(&tree, key)` | `map.remove(key)` | 按键删除，返回被删除的区域 |
| `region_subst(&tree, new)` | `map.insert(new)` | BTreeMap 的 insert 对同键自动替换 |
| `region_search(&tree, k, AVL_LESS_EQUAL)` | `map.find_less_equal(k)` | 查找 ≤ k 的最大区域 |
| `region_search(&tree, k, AVL_GREATER)` | `map.find_greater(k)` | 查找 > k 的最小区域 |
| `region_search_least(&tree)` | `map.iter().next()` | 最小区域 |
| `region_search_greatest(&tree)` | `map.iter().next_back()` | 最大区域 |
| `region_start_iter_least` + `region_incr_iter` | `map.iter()` | 中序遍历迭代器 |
| `region_find_slot_range` | `map.find_slot(min, max, len)` | 查找空闲地址槽位 |
| `map_lookup(vmp, offset, &physr)` | `map.find(offset)` + `get_slot()` | 地址包含查找 |

---

## 4. 实现详解

### 4.1 核心操作实现

**查找包含地址的区域**

这是最常用的操作，对应 Minix3 的 `map_lookup`：

```rust
impl RegionMap {
    pub(crate) fn find(&self, addr: VirBytes) -> Option<&VirRegion> {
        self.regions
            .range(..=addr)
            .next_back()
            .filter(|(_, r)| r.contains_addr(addr))
            .map(|(_, r)| r)
    }

    pub(crate) fn find_mut(&mut self, addr: VirBytes) -> Option<&mut VirRegion> {
        self.regions
            .range_mut(..=addr)
            .next_back()
            .filter(|(_, r)| r.contains_addr(addr))
            .map(|(_, r)| r)
    }
}
```

实现原理：
1. `range(..=addr)` 返回所有键 ≤ addr 的条目（BTreeMap 的范围查询，O(log n)）
2. `.next_back()` 取其中键最大的条目（即 ≤ addr 的最大 vaddr）
3. `.filter()` 验证 addr 是否在该区域的 `[vaddr, vaddr + length)` 范围内
4. 如果 addr 落在两个区域之间的间隙，filter 会过滤掉

**插入与删除**

```rust
impl RegionMap {
    pub(crate) fn insert(&mut self, region: VirRegion) {
        self.regions.insert(region.vaddr, region);
    }

    pub(crate) fn remove(&mut self, addr: VirBytes) -> Option<VirRegion> {
        self.regions.remove(&addr)
    }
}
```

BTreeMap 的 `insert` 对同键自动替换，等价于 Minix3 的 `region_subst`。`remove` 返回被删除的值，等价于 Minix3 的 `region_remove`。

**通用搜索**

```rust
impl RegionMap {
    pub(crate) fn search(&self, key: VirBytes, st: SearchType) -> Option<&VirRegion> {
        use core::ops::Bound;

        if st.contains(SearchType::EQUAL) {
            if let Some(r) = self.regions.get(&key) {
                return Some(r);
            }
        }

        if st.contains(SearchType::LESS) {
            if let Some((_, r)) = self.regions.range(..key).next_back() {
                return Some(r);
            }
        }

        if st.contains(SearchType::GREATER) {
            if let Some((_, r)) = self.regions.range(Bound::Excluded(key), ..).next() {
                return Some(r);
            }
        }

        None
    }

    pub(crate) fn find_less(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::LESS)
    }

    pub(crate) fn find_greater(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::GREATER)
    }

    pub(crate) fn find_less_equal(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::LESS_EQUAL)
    }

    pub(crate) fn find_greater_equal(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::GREATER_EQUAL)
    }
}
```

与 Minix3 `region_search` 的对应：
- `AVL_EQUAL` → `get(&key)`：精确匹配
- `AVL_LESS` → `range(..key).next_back()`：严格小于 key 的最大条目
- `AVL_GREATER` → `range(Bound::Excluded(key), ..).next()`：严格大于 key 的最小条目
- `AVL_LESS_EQUAL` → 先尝试 EQUAL，再尝试 LESS
- `AVL_GREATER_EQUAL` → 先尝试 EQUAL，再尝试 GREATER

**重叠查找**

```rust
impl RegionMap {
    pub(crate) fn find_overlap(&self, start: VirBytes, end: VirBytes) -> Option<&VirRegion> {
        for (_, region) in self.iter() {
            if region.overlaps(start, end) {
                return Some(region);
            }
        }
        None
    }

    pub(crate) fn find_all_overlaps<'a>(
        &'a self,
        start: VirBytes,
        end: VirBytes,
    ) -> impl Iterator<Item = &'a VirRegion> {
        self.iter().filter(move |r| r.overlaps(start, end))
    }
}
```

优化思路：可以利用 BTreeMap 的有序性，从 `range(..end)` 开始遍历，跳过 vaddr ≥ end 的区域，因为它们不可能与 `[start, end)` 重叠。

**空闲槽位查找**

```rust
impl RegionMap {
    pub(crate) fn find_slot(
        &self,
        minv: VirBytes,
        maxv: VirBytes,
        length: VirBytes,
    ) -> Option<VirBytes> {
        if length.0 == 0 {
            return None;
        }

        let maxv = if maxv.0 == 0 {
            VirBytes(minv.0.checked_add(length.0)?)
        } else {
            maxv
        };

        if minv.0 >= maxv.0 || minv.0.checked_add(length.0)? > maxv.0 {
            return None;
        }

        let mut prev_end = minv;

        for region in self.iter() {
            if region.vaddr >= prev_end {
                let gap_start = prev_end.max(minv);
                let gap_end = region.vaddr.min(maxv);

                if gap_end.0 > gap_start.0
                    && gap_end.0.saturating_sub(gap_start.0) >= length.0
                {
                    return Some(VirBytes(gap_end.0 - length.0));
                }
            }

            prev_end = region.end_addr().max(prev_end);
        }

        let gap_start = prev_end.max(minv);
        if maxv.0 > gap_start.0
            && maxv.0.saturating_sub(gap_start.0) >= length.0
        {
            return Some(VirBytes(maxv.0 - length.0));
        }

        None
    }
}
```

与 Minix3 `region_find_slot_range` 逻辑一致：遍历区域间的间隙，找到第一个 ≥ length 的空闲段。

### 4.2 迭代器

BTreeMap 自带迭代器，无需手动实现中序遍历：

```rust
impl RegionMap {
    pub(crate) fn iter(&self) -> impl Iterator<Item = &VirRegion> {
        self.regions.iter().map(|(_, r)| r)
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut VirRegion> {
        self.regions.iter_mut().map(|(_, r)| r)
    }

    pub(crate) fn traverse<F>(&self, mut f: F)
    where
        F: FnMut(&VirRegion),
    {
        for (_, region) in &self.regions {
            f(region);
        }
    }
}
```

与自实现 AVL 迭代器的对比：

| 方面 | 自实现 AVL 迭代器 | BTreeMap 迭代器 |
|------|-----------------|----------------|
| 实现 | 手动维护路径栈 | 标准库提供 |
| 生命周期 | 需 `PhantomData` | 标准库处理 |
| 可变迭代 | 需 `*mut` 原始指针 | 安全 Rust |
| 性能 | O(1) amortized/step | O(1) amortized/step |
| 代码量 | ~80 行 | ~10 行 |

### 4.3 查找优化总结

| 操作 | 时间复杂度 | BTreeMap 实现方式 | 用途 |
|------|-----------|------------------|------|
| find | O(log n) | `range(..=addr).next_back()` + filter | 缺页处理、地址验证 |
| find_overlap | O(n) | 遍历 + filter | mmap 冲突检测 |
| search | O(log n) | `get` + `range` | 通用搜索 |
| find_slot | O(n) | 遍历间隙 | 寻找空闲地址空间 |
| iter | O(n) | `regions.iter()` | 遍历所有区域 |

---

## 5. 性能分析

### 5.1 时间复杂度

**BTreeMap 操作时间复杂度**

| 操作 | 平均 | 最坏 | 说明 |
|------|------|------|------|
| 查找 find | O(log n) | O(log n) | `range(..=addr).next_back()` + filter |
| 插入 insert | O(log n) | O(log n) | 含节点分裂 |
| 删除 remove | O(log n) | O(log n) | 含节点合并 |
| 范围查询 range | O(log n) | O(log n) | 定位范围边界 |
| 遍历 iter | O(n) | O(n) | 访问所有元素 |

与 Minix3 AVL 树的对比：两者都是 O(log n) 查找/插入/删除，但常数因子不同。

**BTreeMap vs AVL 树的常数因子**

| 方面 | AVL 树 | BTreeMap |
|------|--------|----------|
| 查找比较次数 | log₂(n) ≈ 10 (n=1000) | log_B(n) ≈ 3-4 (B=32, n=1000) |
| 缓存效率 | 差（每次跳转一个节点） | 好（节点内多元素连续存储） |
| 插入代价 | 1-2 次旋转 | 节点分裂（较少发生） |
| 删除代价 | O(log n) 次旋转 | 节点合并（较少发生） |

BTreeMap 的 B 树节点通常包含多个键（Rust 标准库默认 B ≈ 32），一次缓存行加载可以比较多个键，缓存效率显著优于 AVL 树的逐节点跳转。

**实际性能考量**

```
典型进程的虚拟区域数量:
- 简单进程: 10-50 个区域
- 复杂进程: 100-500 个区域
- 极端情况: 1000+ 个区域

对于 n = 500:
- BTreeMap 查找: ~3 次节点访问
- 实际非常快，微秒级
```

### 5.2 为什么 BTreeMap 适合 VM 场景

**Minix3 选择 AVL 的历史原因**

Minix3 使用 AVL 树的原因：查找密集（缺页处理频繁），AVL 查找更快；修改稀疏（mmap/munmap 较少）；cavl 库提供了成熟实现；节点嵌入节省内存分配。

**Rust 选择 BTreeMap 的理由**

1. **标准库可用**：`alloc::collections::BTreeMap` 在 `no_std` + `alloc` 环境下可用，无需自实现
2. **缓存友好**：B 树节点内多个键连续存储，减少指针跳转和缓存未命中
3. **代码简洁**：~200 行封装代码 vs ~400 行自实现 AVL（含未完成的平衡逻辑）
4. **语义等价**：BTreeMap 提供与 AVL 树相同的 O(log n) 有序映射语义
5. **关注点分离**：VirRegion 不再内嵌树节点字段，结构更清晰
6. **维护成本低**：标准库负责平衡维护，无需自行实现旋转操作

**与 Linux 的对比**

Linux 内核使用红黑树管理 VMA（虚拟内存区域），原因是修改频繁（mmap/munmap/brk），红黑树删除最多 3 次旋转。Minix3 VM 场景类似，但区域数量通常更少（< 100），BTreeMap 的性能完全足够。

### 5.3 缓存效率

**BTreeMap 的缓存优势**

BTreeMap 使用 B 树结构，每个节点存储多个键值对：

```
B 树节点（简化示意，B ≈ 32）:
┌──────────────────────────────────────────────────────────┐
│ key₁ │ key₂ │ key₃ │ ... │ keyₖ │ ptr₁ │ ptr₂ │ ... │
└──────────────────────────────────────────────────────────┘
一次缓存行加载可以比较多个键

AVL 树节点:
┌──────────────────────────┐
│ key │ left │ right │ bal │
└──────────────────────────┘
每次缓存行加载只比较 1 个键
```

**缓存行分析**

```
典型 CPU 缓存行大小: 64 bytes

BTreeMap 节点（B=32）:
- 内部节点: ~256 bytes（多个键 + 子指针）
- 叶子节点: 类似大小
- 一次加载可比较 ~8-16 个键

AVL 树节点:
- VirRegion: ~100 bytes（含 lower/higher/factor）
- 每次加载只比较 1 个键
- 查找需要 ~10 次节点跳转（n=1000）

BTreeMap 查找:
- ~3-4 次节点访问（n=1000）
- 每次节点访问可能 1-2 次缓存行加载
- 总缓存行加载: ~6-8 次
```

**总结**

1. BTreeMap 缓存效率优于 AVL 树：节点内多元素连续存储，减少指针跳转
2. 实际性能足够：典型进程 < 100 个区域，查找微秒级
3. 代码维护成本低：标准库保证正确性，无需自行实现平衡逻辑
4. 选择 BTreeMap 的核心理由：语义等价 + 代码简洁 + 缓存友好

---

## 6. 测试要点

### 6.1 测试维度

| 维度 | 测试场景 | 优先级 |
|------|---------|--------|
| 基本操作 | 插入单个/多个区域、重复键替换 | 高 |
| 查找 | 包含地址查找、边界条件、空映射表查找 | 高 |
| 删除 | 按键删除、不存在键删除 | 高 |
| 范围查询 | LESS/GREATER/EQUAL 等搜索类型 | 高 |
| 重叠查找 | 完全包含、部分重叠、无重叠 | 中 |
| 迭代器 | 遍历顺序、空/单元素映射表 | 中 |
| 可变迭代 | 修改区域属性后一致性 | 中 |
| find_slot | 间隙查找、边界条件 | 中 |
| 32/64 位 | 大地址空间（>4GB）下的查找和插入 | 中 |

### 6.2 关键测试场景

1. **顺序插入 1000 个区域**：验证 BTreeMap 保持有序，查找正确
2. **地址间隙查找**：验证 find 对区域间间隙返回 None
3. **find_overlap 边界**：查询范围恰好与区域边界相切
4. **find_slot 边界**：minv=maxv、length=0、无空闲空间
5. **search 类型组合**：验证 LESS/GREATER/EQUAL/LESS_EQUAL/GREATER_EQUAL 语义
6. **大地址空间**：使用 64 位地址（>4GB）验证查找正确性
7. **重复键插入**：验证同键替换行为（对应 Minix3 的 region_subst）

---

## 7. 参见

- [11-region-mapping.md](11-region-mapping.md) - 区域映射层（vir_region + phys_region）
- [12-vir-region.md](12-vir-region.md) - vir_region 结构定义
- [16-pagefault.md](16-pagefault.md) - 缺页处理中的区域查找（map_lookup）
- [17-vm-fork.md](17-vm-fork.md) - fork 时遍历区域映射表
- [19-vm-map.md](19-vm-map.md) - mmap/munmap 中的区域插入与删除
- [01-vmproc-struct.md](01-vmproc-struct.md) - vmproc 中的 vm_regions 字段

---

*分类: VM私有*
