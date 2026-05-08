# 13-region-avl: 区域 AVL 树

> **分类**: VM私有  
> **源码**: `minix3/minix/servers/vm/regionavl.c`, `regionavl_defs.h`, `cavl_if.h`, `cavl_impl.h`, `region.h`, `region.c`
> **说明**: 使用 AVL 树高效管理虚拟区域，支持快速查找、插入和删除

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

这些操作都需要高效的地址查找，AVL 树提供了 O(log n) 的保证。

**为什么选择 AVL 树**

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

## 3. Rust 设计决策

### 3.1 AVL 树实现选择

**选项对比**

| 选项 | 优点 | 缺点 |
|------|------|------|
| 使用现有库 | 成熟稳定、节省开发时间 | 可能不符合需求、外部依赖 |
| 自己实现 | 完全控制、可定制、无外部依赖 | 开发成本、需要充分测试 |
| 移植 Minix3 代码 | 与原系统一致 | C 风格代码、类型不安全 |

**Rust 生态中的 AVL 库**

| 库 | 特点 | 适用性 |
|----|------|--------|
| `avl` | 纯 Rust 实现，泛型支持 | 功能有限，无范围查找 |
| `ordered-trees` | 多种有序树 | 过于通用 |
| `slab` | 高性能 slab 分配器 | 不是 AVL 树 |

**选择：自己实现**

决定自己实现 AVL 树，原因如下：

1. **特殊需求**：需要"查找包含地址的区域"这一特殊操作，标准库和现有库不支持
2. **嵌入节点**：Minix3 的设计将 AVL 字段嵌入 `vir_region`，避免额外分配，这与标准库的 `BTreeMap` 不同
3. **无 `unsafe`**：可以完全用安全 Rust 实现
4. **学习价值**：理解 AVL 树的实现细节对 OS 开发有益
5. **零依赖**：减少外部依赖，提高可移植性

**与 `BTreeMap` 的对比**

| 特性 | 自实现 AVL | `BTreeMap` |
|------|-----------|------------|
| 节点分配 | 嵌入 VirRegion | 独立节点 |
| 查找复杂度 | O(log n) | O(log n) |
| 范围查找 | 支持（自定义） | 支持 |
| 包含地址查找 | 支持（自定义） | 需要额外逻辑 |
| 内存开销 | 低（无额外分配） | 较高（独立节点） |
| 迭代器 | 自定义 | 标准库支持 |

**实现策略**

1. **节点设计**：AVL 字段嵌入 VirRegion，lower/higher 用 `Option<Box<VirRegion>>`，factor 用 `i8`
2. **核心操作**：insert（递归 BST 插入，重复键替换）、remove（递归 BST 删除，两子节点时合并）、find（查找包含地址的区域）、find_overlap（查找重叠区域）
3. **迭代器**：实现 Iterator trait，中序遍历（按地址排序），使用 Vec 作为路径栈
4. **安全性**：大部分使用安全 Rust，`RegionIterMut` 使用 `*mut VirRegion` 原始指针配合 `PhantomData`

**当前实现状态**

已有基础实现 [avl.rs](../../../os/servers/vm/src/region/avl.rs)，包含：

- `RegionAvl` 结构体（`root: Option<Box<VirRegion>>`, `count: usize`）
- `insert` 递归 BST 插入（重复键替换，保留 AVL 链接）
- `remove` 递归 BST 删除（两子节点时合并子树）
- `find`/`find_mut` 地址包含查找
- `find_overlap`/`find_all_overlaps` 重叠查找
- `search` 通用搜索（支持 LESS/GREATER/EQUAL 等搜索类型）
- `find_slot` 空闲槽位查找
- `traverse` 中序遍历
- `iter`/`iter_mut` 迭代器
- `SearchType` 搜索类型定义

待完善：

- AVL 平衡因子维护（当前 `factor` 字段保留但未使用，插入/删除不维护平衡）
- 旋转操作实现（LL/RR/LR/RL 四种旋转）
- 迭代器性能优化（使用 `smallvec` 替代 `Vec`）

### 3.2 RegionAvlTree 结构

**设计目标**

1. **类型安全**：利用 Rust 类型系统避免 C 的指针错误
2. **封装**：隐藏 AVL 树内部细节，提供清晰 API
3. **所有权**：明确节点所有权，避免内存泄漏

**结构定义**

```rust
use minix_types::VirBytes;
use super::vir_region::VirRegion;

/// AVL 树结构
///
/// 管理进程的虚拟区域集合，按虚拟地址排序。
/// 对应 Minix3: `region_avl` 结构体
#[derive(Debug, Default)]
pub(crate) struct RegionAvl {
    /// 树根节点
    root: Option<Box<VirRegion>>,
    /// 节点数量
    count: usize,
}
```

**与 C 的对比**

| C 实现 | Rust 实现 |
|--------|----------|
| `region_t *root` | `Option<Box<VirRegion>>` |
| NULL 表示空树 | `None` 表示空树 |
| 手动内存管理 | 自动 Drop |
| 悬垂指针风险 | 编译期检查 |

**核心 API**

```rust
impl RegionAvl {
    /// 创建新的空 AVL 树
    pub(crate) fn new() -> Self;

    /// 获取节点数量
    pub(crate) fn len(&self) -> usize;

    /// 检查是否为空
    pub(crate) fn is_empty(&self) -> bool;

    /// 查找包含指定地址的区域
    ///
    /// 返回 vaddr <= addr < vaddr + length 的区域
    pub(crate) fn find(&self, addr: VirBytes) -> Option<&VirRegion>;

    /// 查找指定地址的区域（可变）
    pub(crate) fn find_mut(&mut self, addr: VirBytes) -> Option<&mut VirRegion>;

    /// 插入区域
    ///
    /// 如果存在相同 vaddr 的区域，替换它
    pub(crate) fn insert(&mut self, region: VirRegion);

    /// 删除指定地址的区域
    ///
    /// 返回被删除的区域，如果不存在返回 None
    pub(crate) fn remove(&mut self, addr: VirBytes) -> Option<VirRegion>;

    /// 查找与指定范围重叠的区域
    pub(crate) fn find_overlap(&self, start: VirBytes, end: VirBytes) -> Option<&VirRegion>;

    /// 遍历所有区域（中序遍历）
    pub(crate) fn traverse<F>(&self, f: F) where F: FnMut(&VirRegion);
}
```

**类型安全保证**

C 代码的问题：裸指针可能悬垂，free 后 use-after-free 风险。Rust 的解决：`Option<Box<VirRegion>>` 天然表示"空或非空"，所有权转移保证无 use-after-free，借用检查器防止悬垂指针，Drop trait 自动释放，`&mut` 独占访问防止数据竞争。

**嵌入 VirRegion 的 AVL 字段**

```rust
/// 虚拟区域
pub(crate) struct VirRegion {
    /// 虚拟地址（AVL 键）
    pub vaddr: VirBytes,
    /// 区域长度
    pub length: VirBytes,
    /// 物理块指针数组
    pub physblocks: Vec<Option<Box<PhysRegion>>>,
    /// 区域标志
    pub flags: VrFlags,
    /// 父进程槽位
    pub parent_slot: Option<UserSlot>,
    /// 默认内存类型
    pub def_memtype: Option<&'static dyn MemType>,
    /// 共享映射计数
    pub remaps: i32,
    /// 唯一 ID
    pub id: i32,
    /// 类型特定参数
    pub param: VrParam,

    // AVL 树字段 - 嵌入在节点中
    /// 左子节点（地址更小）
    pub lower: Option<Box<VirRegion>>,
    /// 右子节点（地址更大）
    pub higher: Option<Box<VirRegion>>,
    /// 平衡因子 (-1, 0, 1)
    pub factor: i8,
}

impl VirRegion {
    /// 获取结束地址
    pub(crate) fn end_addr(&self) -> VirBytes {
        self.vaddr + self.length
    }
}
```

**内存布局**

VirRegion 结构体：vaddr u64 8 bytes, length u64 8 bytes, flags VrFlags 4 bytes, ... 其他字段 ..., lower Option\<Box\<VirRegion\>\> 8 bytes, higher Option\<Box\<VirRegion\>\> 8 bytes, factor i8 1 byte。

与 C 的对比：C 指针 8 bytes + factor 4 bytes（padding）；Rust Option\<Box\> 8 bytes + factor 1 byte。Rust 更紧凑（Option 优化：None 用 0 表示空指针，Some(ptr) 用非零指针表示，无额外判别字段）。

### 3.3 RegionIter 迭代器

**设计目标**

实现 Rust 的 `Iterator` trait，提供符合 Rust 惯例的迭代接口。

**迭代器结构**

```rust
/// AVL 树中序迭代器
///
/// 按虚拟地址升序遍历所有区域。
/// 对应 Minix3: `region_iter` 结构体
pub(crate) struct RegionIter<'a> {
    /// 路径栈，存储从根到当前节点的路径
    stack: Vec<&'a VirRegion>,
}

/// 可变迭代器
pub(crate) struct RegionIterMut<'a> {
    stack: Vec<*mut VirRegion>,
    _marker: PhantomData<&'a mut VirRegion>,
}
```

**与 C 迭代器的对比**

| C 迭代器 | Rust 迭代器 |
|---------|------------|
| 固定大小数组 `path_h[29]` | 动态 `Vec` |
| 手动管理 `depth` | 自动管理栈深度 |
| `region_incr_iter` | `Iterator::next` |
| `region_get_iter` | 当前值由 `next` 返回 |
| 无生命周期 | 生命周期 `'a` 确保安全 |

**Iterator trait 实现**

```rust
impl<'a> Iterator for RegionIter<'a> {
    type Item = &'a VirRegion;

    fn next(&mut self) -> Option<Self::Item> {
        // 弹出栈顶节点
        let node = self.stack.pop()?;

        // 将右子树的左边界压栈
        let mut right = node.higher.as_ref();
        while let Some(r) = right {
            self.stack.push(r.as_ref());
            right = r.lower.as_ref();
        }

        Some(node)
    }
}
```

**创建迭代器**

```rust
impl RegionAvl {
    /// 创建中序迭代器
    pub(crate) fn iter(&self) -> RegionIter<'_> {
        let mut stack = Vec::new();

        // 将左边界压栈（找到最小节点）
        let mut node = self.root.as_ref();
        while let Some(n) = node {
            stack.push(n.as_ref());
            node = n.lower.as_ref();
        }

        RegionIter { stack }
    }

    /// 创建可变迭代器
    pub(crate) fn iter_mut(&mut self) -> RegionIterMut<'_> {
        let mut stack = Vec::new();

        let mut node = self.root.as_mut();
        while let Some(n) = node {
            stack.push(n.as_mut() as *mut VirRegion);
            node = n.lower.as_mut();
        }

        RegionIterMut { stack, _marker: PhantomData }
    }
}
```

**迭代过程示例**

```
┌─────────────────────────────────────────────────────────────┐
│              中序迭代过程                                     │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   AVL 树:                                                   │
│              D                                              │
│             / \                                             │
│            B   F                                            │
│           / \ / \                                           │
│          A  C E  G                                          │
│                                                             │
│   初始状态 (iter):                                          │
│   stack = [A, B, D]  (左边界)                               │
│                                                             │
│   第 1 次 next():                                           │
│   - pop A, 返回 A                                          │
│   - A 无右子树, stack = [B, D]                              │
│                                                             │
│   第 2 次 next():                                           │
│   - pop B, 返回 B                                          │
│   - B 有右子树 C, 压栈 C 的左边界                           │
│   - stack = [C, D]                                         │
│                                                             │
│   第 3 次 next():                                           │
│   - pop C, 返回 C                                          │
│   - C 无右子树, stack = [D]                                 │
│                                                             │
│   第 4 次 next():                                           │
│   - pop D, 返回 D                                          │
│   - D 有右子树 F, 压栈 F 的左边界                           │
│   - stack = [E, F]                                         │
│                                                             │
│   ... 继续 ...                                              │
│                                                             │
│   第 7 次 next():                                           │
│   - pop G, 返回 G                                          │
│   - G 无右子树, stack = []                                  │
│                                                             │
│   第 8 次 next():                                           │
│   - stack 为空, 返回 None (迭代结束)                        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**使用示例**

```rust
// 遍历所有区域
for region in tree.iter() {
    println!("region: {:x}-{:x}", region.vaddr, region.end_addr());
}

// 查找特定条件的区域
let found = tree.iter()
    .find(|r| r.vaddr <= addr && addr < r.end_addr());

// 计算总内存
let total: u64 = tree.iter()
    .map(|r| r.length)
    .sum();

// 可变遍历
for region in tree.iter_mut() {
    region.flags |= VrFlags::WRITABLE;
}
```

**IntoIterator 实现**

```rust
impl<'a> IntoIterator for &'a RegionAvl {
    type Item = &'a VirRegion;
    type IntoIter = RegionIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
```

**性能考虑**

| 方面 | C 迭代器 | Rust 迭代器 |
|------|---------|------------|
| 空间 | 固定 29 * 8 = 232 bytes | 动态 Vec，平均 O(log n) |
| 时间 | O(1) per step | O(1) amortized per step |
| 分配 | 无 | 初始分配，后续无 |

可以使用 `smallvec` 优化为固定大小栈，避免堆分配：

```rust
use smallvec::SmallVec;

pub(crate) struct RegionIter<'a> {
    stack: SmallVec<[&'a VirRegion; 30]>,  // 固定 30 层
}
```

---

## 4. 实现详解

### 4.1 节点定义

**键值设计**

AVL 节点的键是虚拟地址 `vaddr`，值是整个 `VirRegion` 结构体。

**VirRegion 中的 AVL 字段**

AVL 字段直接嵌入 `VirRegion`，无需额外的节点包装类型：

```rust
/// 虚拟区域
///
/// 包含 AVL 树节点字段，可直接作为 AVL 节点使用。
pub(crate) struct VirRegion {
    pub vaddr: VirBytes,
    pub length: VirBytes,
    pub physblocks: Vec<Option<Box<PhysRegion>>>,
    pub flags: VrFlags,
    pub parent_slot: Option<UserSlot>,
    pub def_memtype: Option<&'static dyn MemType>,
    pub remaps: i32,
    pub id: i32,
    pub param: VrParam,

    // AVL 树字段
    pub lower: Option<Box<VirRegion>>,
    pub higher: Option<Box<VirRegion>>,
    pub factor: i8,
}

impl VirRegion {
    /// 获取结束地址
    pub(crate) fn end_addr(&self) -> VirBytes {
        self.vaddr + self.length
    }

    /// 检查地址是否在区域内
    pub(crate) fn contains(&self, addr: VirBytes) -> bool {
        addr >= self.vaddr && addr < self.end_addr()
    }

    /// 检查是否与指定范围重叠
    pub(crate) fn overlaps(&self, start: VirBytes, end: VirBytes) -> bool {
        self.vaddr < end && self.end_addr() > start
    }
}
```

**与文档 Ch3 中设计决策的对应**：`factor` 使用 `i8` 而非自定义 `BalanceFactor` 类型，因为当前实现尚未加入平衡维护逻辑，直接使用原始类型更简洁。待平衡逻辑实现后可考虑引入类型安全的封装。

**节点关系图**

```
┌─────────────────────────────────────────────────────────────┐
│              AVL 节点关系                                     │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   VirRegion 节点:                                           │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ vaddr: 0x400000                                      │   │
│   │ length: 0x3000                                       │   │
│   │ ... 其他字段 ...                                     │   │
│   │ lower ───────────────────────┐                      │   │
│   │ higher ──────────────────┐   │                      │   │
│   │ factor: 0                 │   │                      │   │
│   └────────────────────────────┼───┼────────────────────┘   │
│                               │   │                         │
│              ┌────────────────┘   └────────────────┐        │
│              ↓                                     ↓        │
│   ┌─────────────────────┐             ┌─────────────────────┐│
│   │ vaddr: 0x200000     │             │ vaddr: 0x600000     ││
│   │ factor: -1          │             │ factor: 1           ││
│   │ lower: None         │             │ lower ────────┐     ││
│   │ higher ────────┐    │             │ higher: None  │     ││
│   └─────────────────┼────┘             └───────────────┼────┘│
│                     ↓                                 ↓     │
│           ┌─────────────────┐               ┌─────────────────┐
│           │ vaddr: 0x300000 │               │ vaddr: 0x500000 │
│           │ factor: 0       │               │ factor: 0       │
│           └─────────────────┘               └─────────────────┘
│                                                             │
│   键排序: 0x200000 < 0x300000 < 0x400000 < 0x500000 < 0x600000
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**键比较**

```rust
impl VirRegion {
    /// 比较键值
    pub(crate) fn cmp_key(&self, other: VirBytes) -> Ordering {
        self.vaddr.cmp(&other)
    }
}
```

### 4.2 插入与删除

**当前实现状态**

当前 Rust 实现采用简单的递归 BST 插入/删除，尚未实现 AVL 平衡维护。`factor` 字段保留但未使用，为后续添加平衡逻辑预留。

**插入操作**

```rust
impl RegionAvl {
    pub(crate) fn insert(&mut self, region: VirRegion) {
        let was_inserted = Self::insert_node(&mut self.root, region);
        if was_inserted {
            self.count += 1;
        }
    }

    fn insert_node(node: &mut Option<Box<VirRegion>>, mut region: VirRegion) -> bool {
        match node {
            None => {
                *node = Some(Box::new(region));
                true
            }
            Some(n) => {
                if region.vaddr < n.vaddr {
                    Self::insert_node(&mut n.lower, region)
                } else if region.vaddr > n.vaddr {
                    Self::insert_node(&mut n.higher, region)
                } else {
                    // 重复键：替换节点内容，保留 AVL 链接
                    region.lower = n.lower.take();
                    region.higher = n.higher.take();
                    region.factor = n.factor;
                    **n = region;
                    false
                }
            }
        }
    }
}
```

**插入逻辑说明**：

1. 空节点位置 → 创建新节点，返回 `true` 表示新增
2. 键小于当前节点 → 递归插入左子树
3. 键大于当前节点 → 递归插入右子树
4. 键相等 → 替换节点内容，保留 `lower`/`higher`/`factor`（对应 Minix3 的 `region_subst` 语义），返回 `false` 表示替换而非新增

**删除操作**

```rust
impl RegionAvl {
    pub(crate) fn remove(&mut self, addr: VirBytes) -> Option<VirRegion> {
        Self::remove_node(&mut self.root, addr).map(|region| {
            self.count -= 1;
            *region
        })
    }

    fn remove_node(node: &mut Option<Box<VirRegion>>, addr: VirBytes) -> Option<Box<VirRegion>> {
        let n = node.as_mut()?;

        if addr < n.vaddr {
            return Self::remove_node(&mut n.lower, addr);
        } else if addr > n.vaddr {
            return Self::remove_node(&mut n.higher, addr);
        }

        let mut removed = node.take().unwrap();

        match (removed.lower.take(), removed.higher.take()) {
            (None, None) => Some(removed),
            (Some(left), None) => {
                *node = Some(left);
                Some(removed)
            }
            (None, Some(right)) => {
                *node = Some(right);
                Some(removed)
            }
            (Some(left), Some(right)) => {
                // 两个子节点：将左子树挂到右子树的最左端
                *node = Some(right);
                let mut current = node.as_mut().unwrap();
                while current.lower.is_some() {
                    current = current.lower.as_mut().unwrap();
                }
                current.lower = Some(left);
                Some(removed)
            }
        }
    }
}
```

**删除逻辑说明**：

1. 无子节点 → 直接移除
2. 仅一个子节点 → 用子节点替换
3. 两个子节点 → 用右子树替换，将左子树挂到右子树的最左端（不同于 Minix3 的替代节点策略，但结果等价）

**与 Minix3 删除的差异**：Minix3 的 `region_remove` 从更深的子树找替代叶子节点，然后"塞入"被删节点位置，并从替代节点的父节点向上回溯平衡。当前 Rust 实现简化为直接合并子树，不维护平衡因子。

**待实现的平衡维护**

当前实现未维护 AVL 平衡因子，最坏情况下可能退化为链表。后续需添加：

1. 插入后从插入点向上回溯更新 `factor`，必要时旋转
2. 删除后从删除点向上回溯更新 `factor`，必要时旋转
3. 四种旋转操作：LL（右旋）、RR（左旋）、LR（先左旋后右旋）、RL（先右旋后左旋）

平衡因子更新规则（待实现时参考）：

| 操作 | 方向 | 平衡因子变化 |
|------|------|-------------|
| 插入到左子树 | 左 | factor -= 1 |
| 插入到右子树 | 右 | factor += 1 |
| 删除左子树节点 | 左 | factor += 1 |
| 删除右子树节点 | 右 | factor -= 1 |

| 操作 | 停止回溯条件 |
|------|-------------|
| 插入 | factor 变为 0（子树高度不变）或旋转后 |
| 删除 | factor 变为 ±1（子树高度不变）或旋转后仍需检查 |

### 4.3 查找优化

**地址范围查找的特殊性**

VM 的 AVL 树查找与普通 BST 查找不同：我们需要查找"包含指定地址的区域"，而不是精确匹配键值。

普通 BST 查找：查找键 == 节点键，返回精确匹配的节点。

VM 区域查找：查找地址 >= 节点.vaddr 且查找地址 < 节点.end_addr()，返回包含该地址的区域。

示例：区域 A (vaddr=0x1000, length=0x2000), 区域 B (vaddr=0x4000, length=0x1000), 区域 C (vaddr=0x6000, length=0x3000)。
- find(0x1500) → 区域 A (0x1000 <= 0x1500 < 0x3000)
- find(0x3500) → None（间隙）
- find(0x4500) → 区域 B (0x4000 <= 0x4500 < 0x5000)

**Minix3 的搜索类型**

Minix3 的 `cavl_if.h` 定义了多种搜索类型：

```c
typedef enum {
    AVL_EQUAL = 1,           // 精确匹配
    AVL_LESS = 2,            // 小于
    AVL_GREATER = 4,         // 大于
    AVL_LESS_EQUAL = AVL_EQUAL | AVL_LESS,     // 小于等于
    AVL_GREATER_EQUAL = AVL_EQUAL | AVL_GREATER // 大于等于
} avl_search_type;
```

**Rust 搜索类型定义**

```rust
/// AVL 搜索类型
///
/// 对应 Minix3: `avl_search_type` 枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SearchType(u8);

impl SearchType {
    pub(crate) const EQUAL: Self = Self(1);
    pub(crate) const LESS: Self = Self(2);
    pub(crate) const GREATER: Self = Self(4);
    pub(crate) const LESS_EQUAL: Self = Self(3);
    pub(crate) const GREATER_EQUAL: Self = Self(5);

    pub(crate) fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }
}
```

**地址包含查找**

```rust
impl RegionAvl {
    /// 查找包含指定地址的区域
    ///
    /// 这是最常用的查找操作，用于：
    /// - 缺页处理：查找发生缺页的区域
    /// - 内存访问：验证地址是否在有效区域内
    ///
    /// 时间复杂度: O(log n)
    pub(crate) fn find(&self, addr: VirBytes) -> Option<&VirRegion> {
        Self::find_containing(&self.root, addr)
    }

    fn find_containing(node: &Option<Box<VirRegion>>, addr: VirBytes) -> Option<&VirRegion> {
        let n = node.as_ref()?;

        if addr < n.vaddr {
            Self::find_containing(&n.lower, addr)
        } else if addr >= n.end_addr() {
            Self::find_containing(&n.higher, addr)
        } else {
            Some(n)
        }
    }
}
```

**查找过程图示**

```
┌─────────────────────────────────────────────────────────────┐
│              查找包含地址 0x2500 的区域                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   AVL 树状态:                                               │
│                    D (0x4000-0x5000)                        │
│                   / \                                       │
│          B (0x1000-0x3000)  F (0x6000-0x9000)              │
│         / \                / \                             │
│   A (0x0-0x1000)  C (0x3500-0x4000)  G (0x5500-0x6000)    │
│                                                             │
│   查找过程:                                                  │
│   1. 从根 D 开始: 0x2500 < 0x4000 → 左子树                  │
│   2. 到达 B: 0x2500 >= 0x1000 且 0x2500 < 0x3000 → 找到!   │
│                                                             │
│   结果: 返回区域 B (0x1000-0x3000)                          │
│                                                             │
│   查找地址 0x3200 的过程:                                    │
│   1. 从根 D 开始: 0x3200 < 0x4000 → 左子树                  │
│   2. 到达 B: 0x3200 >= 0x3000 → 右子树                     │
│   3. 到达 C: 0x3200 < 0x3500 → 左子树                      │
│   4. 左子树为空 → 返回 None                                 │
│                                                             │
│   结果: None (地址在 B 和 C 之间的间隙)                      │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**重叠查找**

```rust
impl RegionAvl {
    /// 查找与指定范围重叠的任意区域
    ///
    /// 用于 mmap 检查新区域是否与现有区域冲突。
    /// 重叠条件: region.vaddr < end && region.end_addr() > start
    ///
    /// 时间复杂度: O(log n) 平均，最坏 O(n) 如果范围跨越多个区域
    pub(crate) fn find_overlap(&self, start: VirBytes, end: VirBytes) -> Option<&VirRegion> {
        Self::find_overlap_node(self.root.as_ref(), start, end)
    }

    fn find_overlap_node(
        node: Option<&Box<VirRegion>>,
        start: VirBytes,
        end: VirBytes,
    ) -> Option<&VirRegion> {
        let n = node?;

        if n.vaddr < end && n.end_addr() > start {
            return Some(n);
        }

        if end <= n.vaddr {
            Self::find_overlap_node(n.lower.as_ref(), start, end)
        } else if start >= n.end_addr() {
            Self::find_overlap_node(n.higher.as_ref(), start, end)
        } else {
            None
        }
    }

    /// 查找所有与指定范围重叠的区域
    ///
    /// 返回重叠区域的迭代器
    pub(crate) fn find_all_overlaps<'a>(
        &'a self,
        start: VirBytes,
        end: VirBytes,
    ) -> impl Iterator<Item = &'a VirRegion> {
        self.iter().filter(move |r| r.overlaps(start, end))
    }
}
```

**搜索类型查找**

```rust
impl RegionAvl {
    /// 通用搜索函数
    ///
    /// 支持多种搜索类型，对应 Minix3 的 `region_search`
    pub(crate) fn search(&self, key: VirBytes, st: SearchType) -> Option<&VirRegion> {
        Self::search_node(self.root.as_ref(), key, st)
    }

    fn search_node(
        mut node: Option<&Box<VirRegion>>,
        key: VirBytes,
        st: SearchType,
    ) -> Option<&VirRegion> {
        let target_cmp = if st.contains(SearchType::LESS) {
            1i32
        } else if st.contains(SearchType::GREATER) {
            -1i32
        } else {
            0i32
        };

        let mut match_h: Option<&VirRegion> = None;

        while let Some(h) = node {
            let cmp = key.0.cmp(&h.vaddr.0);

            if cmp == Ordering::Equal {
                if st.contains(SearchType::EQUAL) {
                    return Some(h);
                }
                return if target_cmp < 0 {
                    Self::search_node(h.lower.as_ref(), key, st)
                } else {
                    Self::search_node(h.higher.as_ref(), key, st)
                };
            }

            let cmp_val = if cmp == Ordering::Less { -1i32 } else { 1i32 };
            if target_cmp != 0 && (cmp_val ^ target_cmp) >= 0 {
                match_h = Some(h);
            }

            node = if cmp == Ordering::Less {
                h.lower.as_ref()
            } else {
                h.higher.as_ref()
            };
        }

        match_h
    }

    /// 查找小于指定键的最大区域
    pub(crate) fn find_less(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::LESS)
    }

    /// 查找大于指定键的最小区域
    pub(crate) fn find_greater(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::GREATER)
    }

    /// 查找小于等于指定键的最大区域
    pub(crate) fn find_less_equal(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::LESS_EQUAL)
    }

    /// 查找大于等于指定键的最小区域
    pub(crate) fn find_greater_equal(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::GREATER_EQUAL)
    }
}
```

**查找空闲槽位**

Minix3 的 `region_find_slot_range` 函数用于查找可用的虚拟地址空间：

```rust
impl RegionAvl {
    /// 在指定范围内查找足够大的空闲槽位
    ///
    /// 对应 Minix3: `region_find_slot_range`
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

        // 遍历区域，寻找间隙
        let mut prev_end = minv;

        for region in self.iter() {
            // 检查当前区域之前是否有足够空间
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

        // 检查最后一个区域之后的空间
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

**查找优化总结**

| 操作 | 时间复杂度 | 用途 |
|------|-----------|------|
| find | O(log n) | 缺页处理、地址验证 |
| find_overlap | O(log n) | mmap 冲突检测 |
| search | O(log n) | 通用搜索 |
| find_slot | O(n) | 寻找空闲地址空间 |
| iter | O(n) | 遍历所有区域 |

优化策略：find/find_overlap 利用 BST 性质剪枝搜索；find_slot 需要遍历间隙，无法避免 O(n)；可选缓存最近查找结果。

---

## 5. 性能分析

### 5.1 时间复杂度

**基本操作复杂度**

AVL 树通过强制平衡，保证了所有基本操作的 O(log n) 时间复杂度。

**AVL 树操作时间复杂度**

| 操作 | 平均 | 最坏 | 说明 |
|------|------|------|------|
| 查找 find | O(log n) | O(log n) | 包含地址查找 |
| 插入 insert | O(log n) | O(log n) | 含平衡调整 |
| 删除 remove | O(log n) | O(log n) | 含平衡调整 |
| 最小值 | O(log n) | O(log n) | 左边界遍历 |
| 最大值 | O(log n) | O(log n) | 右边界遍历 |
| 中序后继 | O(log n) | O(log n) | 删除时使用 |
| 遍历 iter | O(n) | O(n) | 访问所有节点 |

对比普通 BST：查找/插入/删除最坏均为 O(n)（退化为链表）。

**高度证明**

AVL 树高度 h 与节点数 n 的关系：

```
设 N(h) 为高度为 h 的 AVL 树的最少节点数。

递推关系:
N(0) = 1      (空树)
N(1) = 1      (单节点)
N(h) = 1 + N(h-1) + N(h-2)  (根 + 左右子树)

这与斐波那契数列相似:
N(h) = F(h+2) - 1

其中 F(n) 是斐波那契数列。

因此:
n >= N(h) ≈ φ^(h+2) / √5 - 1

其中 φ = (1 + √5) / 2 ≈ 1.618 (黄金比例)

解得:
h <= c * log₂(n)

其中 c = 1 / log₂(φ) ≈ 1.44

所以 AVL 树高度最多为 1.44 * log₂(n)
```

**查找复杂度分析**

```rust
fn find_containing(node: &Option<Box<VirRegion>>, addr: VirBytes) -> Option<&VirRegion> {
    let n = node.as_ref()?;

    if addr < n.vaddr {
        Self::find_containing(&n.lower, addr)
    } else if addr >= n.end_addr() {
        Self::find_containing(&n.higher, addr)
    } else {
        Some(n)
    }
}
```

**插入复杂度分析**

```
插入过程:
1. 搜索插入位置: O(log n) - 从根到叶
2. 插入新节点: O(1)
3. 回溯更新平衡因子: O(log n) - 从叶到根
4. 旋转调整: O(1) - 最多一次旋转

总时间: O(log n)

关键点: 插入后最多需要一次旋转即可恢复平衡
```

**删除复杂度分析**

```
删除过程:
1. 搜索删除节点: O(log n)
2. 找到替代节点(如需要): O(log n)
3. 执行删除: O(1)
4. 回溯更新平衡因子: O(log n)
5. 旋转调整: O(log n) - 可能多次旋转

总时间: O(log n)

关键点: 删除可能需要 O(log n) 次旋转，但总时间仍为 O(log n)
```

**实际性能考量**

**实际性能影响因素**

1. **常数因子**：AVL 树平衡操作比红黑树更频繁，但查找更快（树更矮），适合读多写少场景
2. **缓存局部性**：节点分散在堆上，指针跳转多，每次访问可能缓存未命中，实际性能受内存访问模式影响
3. **分支预测**：比较结果难以预测，分支预测失败代价高
4. **内存分配**：每个节点单独分配，分配/释放有开销，可考虑内存池优化

**VM 场景的实际复杂度**

```
典型进程的虚拟区域数量:
- 简单进程: 10-50 个区域
- 复杂进程: 100-500 个区域
- 极端情况: 1000+ 个区域

对于 n = 500:
- 树高度 ≈ log₂(500) ≈ 9 (理论最大 1.44 * 9 ≈ 13)
- 查找需要约 9 次比较
- 实际非常快，微秒级

对于 n = 10000:
- 树高度 ≈ log₂(10000) ≈ 14
- 查找需要约 14 次比较
- 仍然非常高效
```

---

### 5.2 与红黑树对比

**两种平衡树的对比**

AVL 树和红黑树都是自平衡二叉搜索树，但平衡策略不同：

**AVL vs 红黑树对比**

| 特性 | AVL 树 | 红黑树 |
|------|--------|--------|
| 平衡条件 | \|平衡因子\| ≤ 1 | 无红红相邻 |
| 最大高度 | 1.44 log n | 2 log n |
| 查找性能 | 更优 | 稍差 |
| 插入旋转 | 最多 1 次 | 最多 2 次 |
| 删除旋转 | 最多 O(log n) | 最多 3 次 |
| 插入调整 | O(log n) | O(1) 均摊 |
| 删除调整 | O(log n) | O(1) 均摊 |
| 空间开销 | 1 字节平衡因子 | 1 位颜色 |

适用场景：AVL 适合读多写少、查找密集；红黑树适合写多读少、修改频繁。

**Minix3 选择 AVL 的原因**

```
1. VM 区域操作的特点:

   | 操作 | 频率 | 说明 |
   |------|------|------|
   | 查找 find | 非常高 | 每次缺页、内存访问 |
   | 插入 insert | 中等 | mmap, brk |
   | 删除 remove | 低 | munmap |
   | 遍历 iter | 中等 | fork, exit |

   查找操作远多于修改操作，AVL 的低高度优势明显。

2. 历史原因:
   - Minix3 开发时，AVL 实现已成熟
   - cavl 库（公共域代码）提供了可靠的实现
   - 红黑树当时在内核中使用较少

3. 实现简洁性:
   - AVL 的平衡条件直观（高度差 ≤ 1）
   - 旋转操作易于理解和验证
   - 代码可维护性好
```

**高度对比示例**

```
对于 n = 1000 个节点:

AVL 树:
- 最大高度: 1.44 * log₂(1000) ≈ 1.44 * 10 ≈ 14
- 平均高度: 约 log₂(1000) ≈ 10
- 查找比较次数: 10-14 次

红黑树:
- 最大高度: 2 * log₂(1000) ≈ 2 * 10 = 20
- 平均高度: 约 1.5 * log₂(1000) ≈ 15
- 查找比较次数: 15-20 次

AVL 查找快约 30-50%
```

**旋转次数对比**

```
插入操作旋转对比：

| 情况 | AVL 旋转 | 红黑树旋转 |
|------|---------|-----------|
| 无需调整 | 0 | 0 |
| 单旋转 | 1 | 1 |
| 双旋转 | 2 | 2 |
| 颜色调整 | - | 可能多次 |
| 最大旋转 | 2 | 2 |

删除操作旋转对比：

| 情况 | AVL 旋转 | 红黑树旋转 |
|------|---------|-----------|
| 无需调整 | 0 | 0 |
| 简单情况 | 1 | 1-2 |
| 复杂情况 | O(log n) | 最多 3 |
| 最大旋转 | O(log n) | 3 |

红黑树删除最多 3 次旋转，AVL 可能需要 O(log n) 次。
但 VM 场景删除较少，影响有限。
```

**Linux 的选择**

Linux 内核使用红黑树的原因：

```
1. 调度器: 进程频繁插入/删除，红黑树修改更快
2. CFS: 公平调度器需要频繁更新虚拟运行时间
3. 内存管理: vma 区域修改频繁

但 Minix3 VM 场景不同:
- 区域数量较少（通常 < 100）
- 查找远多于修改
- AVL 更适合
```

**Rust 标准库的选择**

Rust 的 `BTreeMap` 使用 B 树而非 AVL 或红黑树：

```
B 树优势:
1. 缓存友好: 节点包含多个键，减少指针跳转
2. 减少分配: 每个节点存储多个元素
3. 遍历高效: 顺序访问时局部性好

但 B 树不适合 VM 区域管理:
1. 需要特殊的"包含地址查找"
2. 节点大小固定，不适合嵌入 VirRegion
3. Minix3 兼容性要求 AVL
```

**结论**

VM 使用 AVL 树的原因：1) 查找密集（缺页处理频繁），AVL 查找更快；2) 修改稀疏（mmap/munmap 较少），删除旋转开销可接受；3) 历史兼容（Minix3 使用 AVL）；4) 实现简洁（平衡条件直观）；5) 节点嵌入（平衡因子只需 1 字节）。VM 场景读多写少的特点使 AVL 成为更优选择。

---

### 5.3 缓存效率

**节点内存布局**

AVL 树节点的内存布局直接影响缓存效率：

C 语言布局（Minix3）：vaddr 8 bytes, length 8 bytes, flags 4 bytes, ... 其他字段 ..., lower 8 bytes（指针）, higher 8 bytes（指针）, factor 4 bytes（int，实际只用 1 byte）。总大小约 80-120 bytes。

Rust 布局：vaddr VirBytes 8 bytes, length VirBytes 8 bytes, flags VrFlags 4 bytes, ... 其他字段 ..., lower Option\<Box\<VirRegion\>\> 8 bytes（None=0）, higher Option\<Box\<VirRegion\>\> 8 bytes, factor i8 1 byte + padding 7 bytes。总大小类似 C 布局。

**缓存行分析**

```
典型 CPU 缓存行大小: 64 bytes

VirRegion 大小: ~100 bytes
- 每个 VirRegion 跨越约 2 个缓存行
- 查找时需要加载 2 个缓存行才能访问所有 AVL 字段

缓存未命中代价:
- L1 缓存命中: ~4 cycles
- L2 缓存命中: ~12 cycles
- L3 缓存命中: ~40 cycles
- 主存访问: ~200 cycles

查找 n 个节点的缓存未命中:
- 最坏情况: O(log n) 次指针跳转
- 每次跳转可能触发缓存未命中
- 总代价: O(log n) * 200 cycles
```

**内存访问模式**

查找操作：Root → Node1 → Node2 → ... → Target，顺序访问但地址不连续，每次跳转可能跨越不同内存页，预取器难以预测。

遍历操作（中序）：A → B → C → D → E → F → G，按地址顺序访问但节点物理位置不连续，每次访问可能缓存未命中。

与数组对比：数组遍历连续内存、预取器高效工作；AVL 遍历指针跳转、每次可能缓存未命中。

**优化策略**

```
1. 节点预分配

   使用内存池预分配节点，提高局部性：相关节点可能在同一缓存行，减少内存碎片，分配/释放更快。

2. 冷热分离

   频繁访问的字段放在前面:

   struct VirRegion {
       // 热字段（查找时访问）
       vaddr: VirBytes,      // 键
       length: VirBytes,     // 计算结束地址
       lower: Option<...>,   // 左子树
       higher: Option<...>,  // 右子树
       factor: i8,           // 平衡因子
       
       // 冷字段（仅在特定操作时访问）
       flags: VrFlags,
       phys_regions: Vec<...>,
       // ...
   }

3. 批量操作

   批量处理减少单独查找开销:

   // 不好的做法: 多次单独查找
   for addr in addresses {
       if let Some(r) = tree.find(addr) { ... }
   }

   // 更好的做法: 利用遍历
   let sorted_addrs = addresses.sorted();
   let mut iter = tree.iter();
   let mut current = iter.next();
   for addr in sorted_addrs {
       while let Some(r) = current {
           if r.contains(addr) { ...; break; }
           current = iter.next();
       }
   }
```

**实际测量**

```
测试场景: 查找 1000 次随机地址

节点数: 100
- AVL 查找: ~1000 * log₂(100) ≈ 7000 次比较
- 缓存未命中: ~7000 次（最坏）
- 估计时间: ~7000 * 200 cycles ≈ 1.4M cycles ≈ 0.5ms

节点数: 1000
- AVL 查找: ~1000 * log₂(1000) ≈ 10000 次比较
- 缓存未命中: ~10000 次（最坏）
- 估计时间: ~10000 * 200 cycles ≈ 2M cycles ≈ 0.7ms

实际性能通常更好，因为:
1. 部分节点已在缓存中
2. TLB 缓存减少页表查找
3. 分支预测器帮助减少流水线停顿
```

**与 B 树对比**

```
B 树的缓存优势：B 树节点（假设阶数 16）包含 16 个键（128 bytes）和 17 个指针（136 bytes），一次缓存加载可比较多个键。AVL 树节点只有 1 个键（~24 bytes AVL 部分），每次缓存加载只比较 1 个键。缓存效率 B 树 > AVL 树，但 B 树不适合 VM 区域管理的特殊查找需求。
```

**Rust 实现的缓存考虑**

```rust
/// 优化后的 VirRegion 布局（冷热分离建议）
#[repr(C)]
pub(crate) struct VirRegion {
    // ===== 热字段: 查找时必访问 =====
    pub vaddr: VirBytes,
    pub length: VirBytes,
    pub lower: Option<Box<VirRegion>>,
    pub higher: Option<Box<VirRegion>>,
    pub factor: i8,
    
    // ===== 冷字段: 特定操作时访问 =====
    pub physblocks: Vec<Option<Box<PhysRegion>>>,
    pub flags: VrFlags,
    pub parent_slot: Option<UserSlot>,
    pub def_memtype: Option<&'static dyn MemType>,
    pub remaps: i32,
    pub id: i32,
    pub param: VrParam,
}
```

**总结**

1. AVL 树缓存效率一般：节点分散、指针跳转多，每次访问可能缓存未命中，但 VM 场景节点数少、影响有限
2. 优化空间有限：节点大小由 VirRegion 决定，不能像 B 树那样增加节点密度，内存池可改善局部性
3. 实际性能足够：典型进程 < 100 个区域，查找深度 < 10，微秒级延迟
4. 选择 AVL 的理由：算法简单实现可靠，查找性能优于红黑树，Minix3 兼容性

---

## 6. 测试要点

### 6.1 测试维度

| 维度 | 测试场景 | 优先级 |
|------|---------|--------|
| 基本操作 | 插入单个/多个节点、重复键替换 | 高 |
| 查找 | 包含地址查找、边界条件、空树查找 | 高 |
| 删除 | 叶子/内部/根节点删除、不存在键删除 | 高 |
| 平衡性 | 顺序插入后 AVL 性质、删除后平衡、高度上界 | 高 |
| 旋转 | LL/RR/LR/RL 四种旋转正确性 | 高 |
| 重叠查找 | 完全包含、部分重叠、无重叠 | 中 |
| 迭代器 | 中序遍历顺序、空/单节点树、删除后迭代 | 中 |
| 可变迭代 | 修改区域属性后一致性 | 中 |
| 32/64 位 | 大地址空间（>4GB）下的查找和插入 | 中 |

### 6.2 关键测试场景

1. **顺序插入 1000 个区域**：验证 AVL 性质保持，高度不超过 1.44 × log₂(n)
2. **删除后连续旋转**：构造需要 O(log n) 次旋转的删除场景
3. **地址间隙查找**：验证 find 对区域间间隙返回 None
4. **find_overlap 边界**：查询范围恰好与区域边界相切
5. **迭代器与修改交互**：删除节点后迭代器行为正确
6. **大地址空间**：使用 64 位地址（>4GB）验证查找正确性

---

## 7. 参见

- [12-vir-region.md](12-vir-region.md) - 使用 AVL 树的 vir_region
- [16-pagefault.md](16-pagefault.md) - 缺页处理中的区域查找（map_lookup）
- [17-vm-fork.md](17-vm-fork.md) - fork 时遍历 AVL 树
- [19-vm-map.md](19-vm-map.md) - mmap/munmap 中的区域插入与删除
- [01-vmproc-struct.md](01-vmproc-struct.md) - vmproc 中的 vm_regions_avl 字段

---

*分类: VM私有*
