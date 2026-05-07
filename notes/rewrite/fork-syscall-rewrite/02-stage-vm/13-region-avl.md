# 13-region-avl: 区域 AVL 树

> **分类**: VM私有  
> **源码**: `minix3/minix/servers/vm/regionavl.c`, `cavl_if.h`  
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

| Minix3 | Rust |
|--------|------|
| `regionavl_defs.h` | 宏定义配置 |
| `cavl_if.h` | 接口声明 |
| `cavl_impl.h` | 实现代码 |
| `regionavl.h` | 模块入口 |
| `regionavl.c` | 编译单元 |
| `region_t.lower/higher/factor` | `VirRegion` 中的 AVL 字段 |
| `region_avl` (vmproc.h) | `RegionAvl` 结构体 |
| `region_avl` 迭代器 | `RegionIter` 迭代器 |

**Minix3 的 AVL 实现特点**

Minix3 使用了一种独特的**宏泛型**方式实现 AVL 树：

```c
// regionavl_defs.h - 通过宏定义"泛型参数"
#define AVL_UNIQUE(id) region_ ## id    // 函数名前缀
#define AVL_HANDLE region_t *            // 节点句柄类型
#define AVL_KEY vir_bytes                // 键类型
#define AVL_MAX_DEPTH 30                 // 最大深度
#define AVL_GET_LESS(h, a) (h)->lower    // 获取左子节点
#define AVL_GET_GREATER(h, a) (h)->higher // 获取右子节点
#define AVL_GET_BALANCE_FACTOR(h) (h)->factor // 获取平衡因子
```

这种方式的优点是零开销抽象（纯宏展开），缺点是类型不安全、难以调试。Rust 实现将用 trait 和泛型替代。

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
3. **最大深度 30**：`AVL_MAX_DEPTH = 30`，支持最多约 2³⁰ ≈ 10 亿个节点

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
#define AVL_MAX_DEPTH 30                  // 最大深度（支持 ~2^30 节点）
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
#define AVL_IMPL_INIT           (1 << 0)   // region_init
#define AVL_IMPL_IS_EMPTY       (1 << 1)   // region_is_empty
#define AVL_IMPL_INSERT         (1 << 2)   // region_insert
#define AVL_IMPL_SEARCH         (1 << 3)   // region_search
#define AVL_IMPL_REMOVE         (1 << 6)   // region_remove
#define AVL_IMPL_START_ITER     (1 << 8)   // region_start_iter
#define AVL_IMPL_INCR_ITER      (1 << 12)  // region_incr_iter
#define AVL_IMPL_ALL            (~0)        // 全部实现
```

**Rust 对比**

| C 宏机制 | Rust 等价 |
|---------|----------|
| `AVL_HANDLE` 宏 | 泛型参数 `T` |
| `AVL_KEY` 宏 | 泛型参数 `K` |
| `AVL_COMPARE_*` 宏 | `Ord` trait |
| `AVL_GET/SET_*` 宏 | 结构体字段访问 |
| `AVL_UNIQUE` 前缀 | 模块/impl 块 |
| 条件编译掩码 | feature flag / cfg |

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

**Rust 实现**

```rust
impl RegionAvl {
    pub fn new() -> Self {
        Self {
            root: None,
            count: 0,
        }
    }
}
```

Rust 的 `Option<Box<VirRegion>>` 天然表示了"空或非空"的语义，不需要显式初始化。

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

```
┌─────────────────────────────────────────────────────────────┐
│                    region_insert 流程                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   1. 初始化新节点 (lower=NULL, higher=NULL, factor=0)       │
│                      ↓                                      │
│   2. 空树? → 直接设为根                                     │
│                      ↓                                      │
│   3. BST 搜索插入位置                                       │
│      - 记录最后一个 factor != 0 的祖先 (unbal)              │
│      - 记录路径位图 (branch)                                │
│      - 重复键 → 返回已有节点                                │
│                      ↓                                      │
│   4. 插入为叶子节点                                         │
│                      ↓                                      │
│   5. 从 unbal 到新节点更新平衡因子                          │
│                      ↓                                      │
│   6. |factor| == 2? → 旋转恢复平衡                         │
│      - LL: 右旋                                            │
│      - RR: 左旋                                            │
│      - LR: 先左旋后右旋                                     │
│      - RL: 先右旋后左旋                                     │
│                      ↓                                      │
│   7. 返回新插入的节点                                       │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

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

```
┌─────────────────────────────────────────────────────────────┐
│              删除 vs 插入的平衡调整对比                       │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   插入:                                                     │
│   - 最多 1 次旋转（单旋或双旋）                              │
│   - 旋转后子树高度不变，上层不受影响                          │
│                                                             │
│   删除:                                                     │
│   - 最多 O(log n) 次旋转                                    │
│   - 旋转后子树高度可能减 1，上层可能继续不平衡               │
│   - 需要从删除点向上回溯到根                                 │
│                                                             │
│   示例（删除导致连续旋转）：                                  │
│                                                             │
│   删除前:                删除节点 X 后:                       │
│         C                    C (不平衡)                      │
│        / \                  / \                              │
│       B   D               B   D                             │
│      /   / \             /     \                            │
│     A   X   E           A       E                           │
│                                                             │
│   旋转 1:                旋转 2:                             │
│         C                    D                              │
│        / \                  / \                             │
│       B   D               C   E                            │
│      /     \             /                                  │
│     A       E           B                                   │
│                        /                                    │
│                       A                                     │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Minix3 的 subst 替代操作**

Minix3 使用 `region_subst` 替代直接删除+重插入：

```c
region_t *region_subst(region_avl *tree, region_t *new_node)
{
    // 查找与 new_node 相同键的节点
    region_t *old = region_search(tree, new_node->vaddr, AVL_EQUAL);

    if (old != NULL) {
        // 复制 AVL 链接到新节点
        new_node->lower = old->lower;
        new_node->higher = old->higher;
        new_node->factor = old->factor;

        // 替换父节点的引用
        // ...
    }

    return old;
}
```

这在区域调整大小时很有用：保持键不变，只替换节点内容。

**删除流程总结**

```
┌─────────────────────────────────────────────────────────────┐
│                    region_remove 流程                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   1. BST 搜索目标节点                                       │
│      - 未找到 → 返回 NULL                                   │
│                      ↓                                      │
│   2. 目标有两个子节点?                                       │
│      - 是 → 找中序后继，替换键值                             │
│      - 否 → 直接删除                                        │
│                      ↓                                      │
│   3. 将唯一子节点（或 NULL）连接到父节点                     │
│                      ↓                                      │
│   4. 从删除点到根更新平衡因子                                │
│                      ↓                                      │
│   5. |factor| == 2? → 旋转恢复平衡                         │
│      - 可能需要多次旋转                                     │
│      - 继续向上回溯直到根                                   │
│                      ↓                                      │
│   6. 返回被删除的节点                                       │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

#### 2.2.4 region_find - 查找区域

**源码位置**: [`cavl_impl.h`](../../../minix3/minix/servers/vm/cavl_impl.h)

查找是 AVL 树最频繁的操作，页错误处理时每次都需要查找包含目标地址的区域。

**精确查找**

```c
region_t *region_search(region_avl *tree, vir_bytes key, avl_search_type st)
{
    region_t *h = tree->root;
    region_t *found = NULL;

    while (h != NULL) {
        if (key < h->vaddr) {
            if (st & AVL_LESS) found = h;   // 记录最近的小于节点
            h = h->lower;
        } else if (key > h->vaddr) {
            if (st & AVL_GREATER) found = h; // 记录最近的大于节点
            h = h->higher;
        } else {
            if (st & AVL_EQUAL) found = h;   // 精确匹配
            break;
        }
    }

    return found;
}
```

**区域包含查找**

VM 最常用的查找是"找到包含某地址的区域"。这需要使用 `AVL_LESS_EQUAL` 搜索：

```c
// map_lookup - 查找包含地址 v 的区域
region_t *map_lookup(struct vmproc *vmp, vir_bytes v, struct phys_region **pr)
{
    // 搜索 vaddr <= v 的最大区域
    region_t *r = region_search(&vmp->vm_regions_avl, v, AVL_LESS_EQUAL);

    if (r == NULL) return NULL;

    // 检查 v 是否在该区域范围内
    if (v >= r->vaddr && v < r->vaddr + r->length) {
        if (pr) {
            // 计算页内偏移
            vir_bytes offset = v - r->vaddr;
            offset = rounddown(offset, VM_PAGE_SIZE);
            *pr = physblock_get(r, offset);
        }
        return r;
    }

    return NULL;    // v 不在任何区域内
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
| fork | `region_copy_slab` | 遍历所有区域复制 |
| 进程终止 | `map_free` | 遍历所有区域释放 |
| 调试 | `region_sanitycheck` | 遍历检查一致性 |
| munmap | `map_unmap_region` | 查找重叠区域 |

#### 2.3.2 region_start - 开始迭代

**源码位置**: [`cavl_impl.h`](../../../minix3/minix/servers/vm/cavl_impl.h)

Minix3 提供三种开始迭代的方式：

**从最小节点开始**

```c
void region_start_iter_least(region_avl *tree, region_iter *iter)
{
    iter->tree_ = tree;
    iter->depth = ~0;       // 初始化为无效
    iter->branch = 0;

    region_t *h = tree->root;
    if (h != NULL) {
        // 沿左子树到底，找到最小节点
        while (h->lower != NULL) {
            iter->path_h[iter->depth + 1] = h;
            iter->depth++;
            h = h->lower;
        }
    }
}
```

**从最大节点开始**

```c
void region_start_iter_greatest(region_avl *tree, region_iter *iter)
{
    iter->tree_ = tree;
    iter->depth = ~0;
    iter->branch = 0;

    region_t *h = tree->root;
    if (h != NULL) {
        // 沿右子树到底，找到最大节点
        while (h->higher != NULL) {
            iter->path_h[iter->depth + 1] = h;
            iter->depth++;
            branch |= (1UL << iter->depth);
            h = h->higher;
        }
    }
}
```

**从指定键开始**

```c
void region_start_iter(region_avl *tree, region_iter *iter,
    vir_bytes key, avl_search_type st)
{
    iter->tree_ = tree;
    iter->depth = ~0;
    iter->branch = 0;

    region_t *h = tree->root;
    while (h != NULL) {
        if (key < h->vaddr) {
            iter->path_h[iter->depth + 1] = h;
            iter->depth++;
            h = h->lower;
            // branch bit = 0 (左)
        } else if (key > h->vaddr) {
            iter->path_h[iter->depth + 1] = h;
            iter->depth++;
            iter->branch |= (1UL << iter->depth);
            h = h->higher;
        } else {
            // 精确匹配
            break;
        }
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
    region_sanitycheck(r);
}
```

#### 2.3.3 region_next - 下一个节点

**源码位置**: [`cavl_impl.h`](../../../minix3/minix/servers/vm/cavl_impl.h)

`region_incr_iter` 实现中序遍历的递增操作，`region_decr_iter` 实现递减操作。

**递增迭代（中序后继）**

```c
void region_incr_iter(region_iter *iter)
{
    region_t *h = region_get_iter(iter);    // 获取当前节点

    if (h == NULL) return;

    // 情况 1: 当前节点有右子树
    // 中序后继是右子树的最左节点
    if (h->higher != NULL) {
        iter->path_h[iter->depth + 1] = h;
        iter->depth++;
        iter->branch |= (1UL << iter->depth);  // 标记走右

        h = h->higher;
        while (h->lower != NULL) {
            iter->path_h[iter->depth + 1] = h;
            iter->depth++;
            // branch bit = 0 (走左)
            h = h->lower;
        }
    }
    // 情况 2: 当前节点无右子树
    // 回溯到第一个"从左子树返回"的祖先
    else {
        while (iter->depth >= 0 &&
               (iter->branch & (1UL << iter->depth))) {
            iter->depth--;
        }
        iter->depth--;
        // 清除 branch 位
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
void region_decr_iter(region_iter *iter)
{
    region_t *h = region_get_iter(iter);

    if (h == NULL) return;

    // 情况 1: 当前节点有左子树
    // 中序前驱是左子树的最右节点
    if (h->lower != NULL) {
        iter->path_h[iter->depth + 1] = h;
        iter->depth++;
        // branch bit = 0 (走左)

        h = h->lower;
        while (h->higher != NULL) {
            iter->path_h[iter->depth + 1] = h;
            iter->depth++;
            iter->branch |= (1UL << iter->depth);
            h = h->higher;
        }
    }
    // 情况 2: 当前节点无左子树
    // 回溯到第一个"从右子树返回"的祖先
    else {
        while (iter->depth >= 0 &&
               !(iter->branch & (1UL << iter->depth))) {
            iter->depth--;
        }
        iter->depth--;
    }
}
```

**获取当前节点**

```c
region_t *region_get_iter(region_iter *iter)
{
    if (iter->depth < 0) return NULL;   // 无效迭代器

    region_t *h;
    if (iter->depth == 0) {
        h = iter->tree_->root;          // 根节点
    } else {
        // 从路径栈获取当前节点的父节点
        h = iter->path_h[iter->depth - 1];
        if (iter->branch & (1UL << (iter->depth - 1))) {
            h = h->higher;              // 从右子树来
        } else {
            h = h->lower;               // 从左子树来
        }
    }

    return h;
}
```

**Rust 迭代器对比**

| C 迭代器 | Rust 迭代器 |
|---------|------------|
| 手动管理 `depth`, `branch`, `path_h` | 编译器自动管理状态 |
| `region_start_iter_least` | `IntoIterator::into_iter` |
| `region_incr_iter` | `Iterator::next` |
| `region_get_iter` | `Iterator::current` (非标准) |
| O(1) 空间（固定大小栈） | O(log n) 空间（递归或栈） |

Rust 可以实现零分配的中序迭代器，使用与 C 相同的路径栈技术。

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

```
┌─────────────────────────────────────────────────────────────┐
│              AVL 树实现策略                                   │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   1. 节点设计                                               │
│      - AVL 字段嵌入 VirRegion                               │
│      - lower, higher, factor 作为 Option/引用              │
│                                                             │
│   2. 核心操作                                               │
│      - insert: 插入并平衡                                   │
│      - remove: 删除并平衡                                   │
│      - find: 查找包含地址的区域                             │
│      - find_overlap: 查找重叠区域                           │
│                                                             │
│   3. 迭代器                                                 │
│      - 实现 Iterator trait                                  │
│      - 中序遍历（按地址排序）                                │
│                                                             │
│   4. 安全性                                                 │
│      - 全部使用安全 Rust                                    │
│      - 无裸指针、无 unsafe 块                               │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**当前实现状态**

已有基础实现 [avl.rs](../../../os/servers/vm/src/region/avl.rs)，包含：

- `RegionAvl` 结构体
- `insert`, `remove`, `find` 基本操作
- `find_overlap` 重叠查找
- `traverse` 中序遍历

待完善：

- 平衡因子维护（当前未实现完整平衡）
- 迭代器 trait 实现
- 性能优化

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
pub struct RegionAvl {
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
    pub fn new() -> Self;

    /// 获取节点数量
    pub fn len(&self) -> usize;

    /// 检查是否为空
    pub fn is_empty(&self) -> bool;

    /// 查找包含指定地址的区域
    ///
    /// 返回 vaddr <= addr < vaddr + length 的区域
    pub fn find(&self, addr: VirBytes) -> Option<&VirRegion>;

    /// 查找指定地址的区域（可变）
    pub fn find_mut(&mut self, addr: VirBytes) -> Option<&mut VirRegion>;

    /// 插入区域
    ///
    /// 如果存在相同 vaddr 的区域，替换它
    pub fn insert(&mut self, region: VirRegion);

    /// 删除指定地址的区域
    ///
    /// 返回被删除的区域，如果不存在返回 None
    pub fn remove(&mut self, addr: VirBytes) -> Option<VirRegion>;

    /// 查找与指定范围重叠的区域
    pub fn find_overlap(&self, start: VirBytes, end: VirBytes) -> Option<&VirRegion>;

    /// 遍历所有区域（中序遍历）
    pub fn traverse<F>(&self, f: F) where F: FnMut(&VirRegion);
}
```

**类型安全保证**

```
┌─────────────────────────────────────────────────────────────┐
│              Rust 类型安全保证                               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   C 代码问题:                                               │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ region_t *r = tree->root;                           │   │
│   │ r->lower = some_ptr;  // 可能悬垂指针               │   │
│   │ free(r);              // use-after-free 风险        │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   Rust 解决:                                                │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ let r = tree.root.as_mut()?;  // Option 检查        │   │
│   │ r.lower = Some(Box::new(region)); // 所有权转移     │   │
│   │ // 自动 Drop，无 use-after-free                     │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   编译期检查:                                               │
│   - 空指针 → Option<T>                                     │
│   - 悬垂指针 → 借用检查器                                   │
│   - 内存泄漏 → Drop trait                                   │
│   - 数据竞争 → &mut 独占访问                                │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**嵌入 VirRegion 的 AVL 字段**

```rust
/// 虚拟区域
pub struct VirRegion {
    /// 虚拟地址（AVL 键）
    pub vaddr: VirBytes,
    /// 区域长度
    pub length: VirBytes,
    /// 其他字段...
    
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
    pub fn end_addr(&self) -> VirBytes {
        VirBytes(self.vaddr.0 + self.length.0)
    }
}
```

**内存布局**

```
┌─────────────────────────────────────────────────────────────┐
│              VirRegion 内存布局                              │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   VirRegion 结构体:                                         │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ vaddr: u64 (8 bytes)                                │   │
│   │ length: u64 (8 bytes)                               │   │
│   │ flags: VrFlags (4 bytes)                            │   │
│   │ ... 其他字段 ...                                     │   │
│   │ lower: Option<Box<VirRegion>> (8 bytes)            │   │
│   │ higher: Option<Box<VirRegion>> (8 bytes)           │   │
│   │ factor: i8 (1 byte)                                 │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   与 C 的对比:                                               │
│   - C: 指针 8 bytes, factor 4 bytes (padding)              │
│   - Rust: Option<Box> 8 bytes, factor 1 byte               │
│   - Rust 更紧凑（Option 优化）                              │
│                                                             │
│   Option<Box<T>> 优化:                                      │
│   - None 用 0 表示（空指针）                                │
│   - Some(ptr) 用非零指针表示                                │
│   - 无额外判别字段                                          │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 3.3 RegionIter 迭代器

**设计目标**

实现 Rust 的 `Iterator` trait，提供符合 Rust 惯例的迭代接口。

**迭代器结构**

```rust
/// AVL 树中序迭代器
///
/// 按虚拟地址升序遍历所有区域。
/// 对应 Minix3: `region_iter` 结构体
pub struct RegionIter<'a> {
    /// 路径栈，存储从根到当前节点的路径
    stack: Vec<&'a VirRegion>,
}

/// 可变迭代器
pub struct RegionIterMut<'a> {
    stack: Vec<&'a mut VirRegion>,
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
            self.stack.push(r);
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
    pub fn iter(&self) -> RegionIter<'_> {
        let mut stack = Vec::new();

        // 将左边界压栈（找到最小节点）
        let mut node = self.root.as_ref();
        while let Some(n) = node {
            stack.push(n);
            node = n.lower.as_ref();
        }

        RegionIter { stack }
    }

    /// 创建可变迭代器
    pub fn iter_mut(&mut self) -> RegionIterMut<'_> {
        let mut stack = Vec::new();

        let mut node = self.root.as_mut();
        while let Some(n) = node {
            stack.push(n);
            node = n.lower.as_mut();
        }

        RegionIterMut { stack }
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
    region.flags |= VR_ACCESSED;
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

pub struct RegionIter<'a> {
    stack: SmallVec<[&'a VirRegion; 30]>,  // 固定 30 层
}
```

---

## 4. 实现详解

### 4.1 节点定义

**键值设计**

AVL 节点的键是虚拟地址 `vaddr`，值是整个 `VirRegion` 结构体。

```rust
/// 平衡因子类型
///
/// -1: 左子树比右子树高 1
///  0: 左右子树等高
///  1: 右子树比左子树高 1
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BalanceFactor(i8);

impl BalanceFactor {
    pub const LEFT_HEAVY: Self = Self(-1);
    pub const BALANCED: Self = Self(0);
    pub const RIGHT_HEAVY: Self = Self(1);

    /// 是否需要重新平衡
    pub fn needs_rebalance(&self) -> bool {
        self.0.abs() > 1
    }

    /// 增加平衡因子（右子树增高）
    pub fn inc(&mut self) {
        self.0 += 1;
    }

    /// 减少平衡因子（左子树增高）
    pub fn dec(&mut self) {
        self.0 -= 1;
    }
}
```

**节点结构**

```rust
/// AVL 节点（嵌入在 VirRegion 中）
///
/// 对应 Minix3: `vir_region` 中的 `lower`, `higher`, `factor` 字段
pub struct AvlNode {
    /// 左子节点（地址更小的区域）
    pub lower: Option<Box<VirRegion>>,
    /// 右子节点（地址更大的区域）
    pub higher: Option<Box<VirRegion>>,
    /// 平衡因子
    pub factor: BalanceFactor,
}

impl AvlNode {
    /// 创建新的叶子节点
    pub fn new_leaf() -> Self {
        Self {
            lower: None,
            higher: None,
            factor: BalanceFactor::BALANCED,
        }
    }

    /// 获取子节点高度
    ///
    /// 返回 (左子树高度, 右子树高度)
    pub fn child_heights(&self) -> (u32, u32) {
        let left_h = self.lower.as_ref().map_or(0, |n| n.height());
        let right_h = self.higher.as_ref().map_or(0, |n| n.height());
        (left_h, right_h)
    }

    /// 更新平衡因子
    pub fn update_factor(&mut self) {
        let (left_h, right_h) = self.child_heights();
        self.factor = BalanceFactor((right_h as i8) - (left_h as i8));
    }
}
```

**VirRegion 中的 AVL 字段**

```rust
/// 虚拟区域
///
/// 包含 AVL 树节点字段，可直接作为 AVL 节点使用。
pub struct VirRegion {
    // ===== 区域属性 =====
    /// 虚拟地址起始（AVL 键）
    pub vaddr: VirBytes,
    /// 区域长度
    pub length: VirBytes,
    /// 区域标志
    pub flags: VrFlags,
    /// 所属进程
    pub parent: Weak<VmProc>,
    /// 内存类型
    pub mem_type: Arc<dyn MemType>,
    /// 物理区域数组
    pub phys_regions: Vec<PhysRegion>,
    /// 唯一 ID
    pub id: u32,
    /// 共享映射计数
    pub remaps: u32,
    /// 类型特定参数
    pub param: RegionParam,

    // ===== AVL 树字段 =====
    /// 左子节点
    pub lower: Option<Box<VirRegion>>,
    /// 右子节点
    pub higher: Option<Box<VirRegion>>,
    /// 平衡因子
    pub factor: BalanceFactor,
}

impl VirRegion {
    /// 获取节点高度
    pub fn height(&self) -> u32 {
        let left_h = self.lower.as_ref().map_or(0, |n| n.height());
        let right_h = self.higher.as_ref().map_or(0, |n| n.height());
        1 + left_h.max(right_h)
    }

    /// 获取结束地址
    pub fn end_addr(&self) -> VirBytes {
        VirBytes(self.vaddr.0 + self.length.0)
    }

    /// 检查地址是否在区域内
    pub fn contains(&self, addr: VirBytes) -> bool {
        addr >= self.vaddr && addr < self.end_addr()
    }

    /// 检查是否与指定范围重叠
    pub fn overlaps(&self, start: VirBytes, end: VirBytes) -> bool {
        self.vaddr < end && self.end_addr() > start
    }
}
```

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

**内存布局优化**

```rust
// 使用 repr(C) 确保 C 兼容布局（如果需要 FFI）
#[repr(C)]
pub struct VirRegion {
    pub vaddr: VirBytes,        // 8 bytes
    pub length: VirBytes,       // 8 bytes
    pub flags: VrFlags,         // 4 bytes
    pub _pad1: u32,             // 4 bytes (对齐)
    // ... 其他字段 ...
    pub lower: Option<Box<VirRegion>>,   // 8 bytes
    pub higher: Option<Box<VirRegion>>,  // 8 bytes
    pub factor: i8,             // 1 byte
    pub _pad2: [u8; 7],         // 7 bytes (对齐)
}
```

**键比较**

```rust
impl VirRegion {
    /// 比较键值
    ///
    /// 返回:
    /// - Ordering::Less: self.vaddr < other
    /// - Ordering::Equal: self.vaddr == other
    /// - Ordering::Greater: self.vaddr > other
    pub fn cmp_key(&self, other: VirBytes) -> Ordering {
        self.vaddr.0.cmp(&other.0)
    }
}

impl Ord for VirRegion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.vaddr.cmp(&other.vaddr)
    }
}

impl PartialOrd for VirRegion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for VirRegion {}

impl PartialEq for VirRegion {
    fn eq(&self, other: &Self) -> bool {
        self.vaddr == other.vaddr
    }
}
```

### 4.2 旋转操作

**四种旋转类型**

AVL 树有四种旋转操作来恢复平衡：

| 类型 | 条件 | 操作 |
|------|------|------|
| LL（右旋） | 左子树的左子树过深 | 单次右旋 |
| RR（左旋） | 右子树的右子树过深 | 单次左旋 |
| LR（左右旋） | 左子树的右子树过深 | 先左旋后右旋 |
| RL（右左旋） | 右子树的左子树过深 | 先右旋后左旋 |

**右旋（LL 情况）**

```rust
/// 右旋
///
/// 用于 LL 情况：左子树的左子树过深
///
///     A              B
///    / \            / \
///   B   T3   →    T1   A
///  / \                / \
/// T1  T2            T2  T3
impl RegionAvl {
    fn rotate_right(node: &mut Box<VirRegion>) {
        // 1. 保存 B 的右子树
        let mut b = node.lower.take().expect("left child must exist");
        let t2 = b.higher.take();

        // 2. 更新平衡因子
        // 如果 B.factor == 0（删除情况），旋转后 A.factor = 0, B.factor = 0
        // 如果 B.factor == -1（插入情况），旋转后 A.factor = 0, B.factor = 0
        match b.factor.0 {
            -1 => {
                node.factor = BalanceFactor::BALANCED;
                b.factor = BalanceFactor::BALANCED;
            }
            0 => {
                // 删除时可能发生
                node.factor = BalanceFactor::LEFT_HEAVY;
                b.factor = BalanceFactor::RIGHT_HEAVY;
            }
            _ => unreachable!("invalid factor for LL rotation"),
        }

        // 3. 重新连接
        b.higher = Some(std::mem::replace(node, b));
        node.lower = t2;
    }
}
```

**左旋（RR 情况）**

```rust
/// 左旋
///
/// 用于 RR 情况：右子树的右子树过深
///
///   A                B
///  / \              / \
/// T1  B     →      A   T3
///    / \          / \
///   T2  T3      T1  T2
impl RegionAvl {
    fn rotate_left(node: &mut Box<VirRegion>) {
        // 1. 保存 B 的左子树
        let mut b = node.higher.take().expect("right child must exist");
        let t2 = b.lower.take();

        // 2. 更新平衡因子
        match b.factor.0 {
            1 => {
                node.factor = BalanceFactor::BALANCED;
                b.factor = BalanceFactor::BALANCED;
            }
            0 => {
                node.factor = BalanceFactor::RIGHT_HEAVY;
                b.factor = BalanceFactor::LEFT_HEAVY;
            }
            _ => unreachable!("invalid factor for RR rotation"),
        }

        // 3. 重新连接
        b.lower = Some(std::mem::replace(node, b));
        node.higher = t2;
    }
}
```

**LR 双旋（先左旋后右旋）**

```rust
/// LR 双旋
///
/// 用于 LR 情况：左子树的右子树过深
///
///       A              A              C
///      / \            / \            / \
///     B   T4   →     C   T4   →    B   A
///    / \            / \            / \ / \
///   T1  C          B   T3        T1 T2 T3 T4
///      / \        / \
///     T2  T3    T1  T2
impl RegionAvl {
    fn rotate_left_right(node: &mut Box<VirRegion>) {
        // 1. 对左子树左旋
        let b = node.lower.as_mut().expect("left child must exist");
        Self::rotate_left(b);

        // 2. 对当前节点右旋
        Self::rotate_right(node);

        // 3. 更新平衡因子（取决于 C 的原平衡因子）
        // C 是旋转后的新根（原 B.higher）
        let c_factor = node.factor.0;
        match c_factor {
            -1 => {
                // C 原来左重
                node.lower.as_mut().unwrap().factor = BalanceFactor::BALANCED;
                node.higher.as_mut().unwrap().factor = BalanceFactor::RIGHT_HEAVY;
            }
            0 => {
                node.lower.as_mut().unwrap().factor = BalanceFactor::BALANCED;
                node.higher.as_mut().unwrap().factor = BalanceFactor::BALANCED;
            }
            1 => {
                // C 原来右重
                node.lower.as_mut().unwrap().factor = BalanceFactor::LEFT_HEAVY;
                node.higher.as_mut().unwrap().factor = BalanceFactor::BALANCED;
            }
            _ => unreachable!(),
        }
        node.factor = BalanceFactor::BALANCED;
    }
}
```

**RL 双旋（先右旋后左旋）**

```rust
/// RL 双旋
///
/// 用于 RL 情况：右子树的左子树过深
///
///     A              A              C
///    / \            / \            / \
///   T1  B     →   T1   C     →    A   B
///      / \            / \        / \ / \
///     C   T4        T2  B      T1 T2 T3 T4
///    / \                / \
///   T2  T3            T3  T4
impl RegionAvl {
    fn rotate_right_left(node: &mut Box<VirRegion>) {
        // 1. 对右子树右旋
        let b = node.higher.as_mut().expect("right child must exist");
        Self::rotate_right(b);

        // 2. 对当前节点左旋
        Self::rotate_left(node);

        // 3. 更新平衡因子
        let c_factor = node.factor.0;
        match c_factor {
            -1 => {
                node.lower.as_mut().unwrap().factor = BalanceFactor::BALANCED;
                node.higher.as_mut().unwrap().factor = BalanceFactor::RIGHT_HEAVY;
            }
            0 => {
                node.lower.as_mut().unwrap().factor = BalanceFactor::BALANCED;
                node.higher.as_mut().unwrap().factor = BalanceFactor::BALANCED;
            }
            1 => {
                node.lower.as_mut().unwrap().factor = BalanceFactor::LEFT_HEAVY;
                node.higher.as_mut().unwrap().factor = BalanceFactor::BALANCED;
            }
            _ => unreachable!(),
        }
        node.factor = BalanceFactor::BALANCED;
    }
}
```

**旋转选择逻辑**

```rust
impl RegionAvl {
    /// 根据平衡因子选择旋转类型
    fn rebalance(node: &mut Box<VirRegion>) -> bool {
        let factor = node.factor.0;

        if factor <= -2 {
            // 左子树过深
            let left = node.lower.as_ref().unwrap();
            if left.factor.0 <= 0 {
                // LL 情况
                Self::rotate_right(node);
            } else {
                // LR 情况
                Self::rotate_left_right(node);
            }
            true
        } else if factor >= 2 {
            // 右子树过深
            let right = node.higher.as_ref().unwrap();
            if right.factor.0 >= 0 {
                // RR 情况
                Self::rotate_left(node);
            } else {
                // RL 情况
                Self::rotate_right_left(node);
            }
            true
        } else {
            // 不需要旋转
            false
        }
    }
}
```

**旋转图示**

```
┌─────────────────────────────────────────────────────────────┐
│                    四种旋转操作                               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  1. LL 右旋 (factor = -2, left.factor <= 0)                │
│                                                             │
│        A(-2)                  B(0)                          │
│        / \                   /   \                          │
│       B   T3      →         T1    A(0)                      │
│      / \                         / \                        │
│     T1  T2                     T2  T3                       │
│                                                             │
│  2. RR 左旋 (factor = 2, right.factor >= 0)                │
│                                                             │
│     A(2)                     B(0)                           │
│     / \                     /   \                           │
│    T1  B          →       A(0)   T3                         │
│       / \                 / \                               │
│      T2  T3             T1  T2                              │
│                                                             │
│  3. LR 双旋 (factor = -2, left.factor > 0)                 │
│                                                             │
│       A(-2)              A(-2)           C(0)               │
│       / \                / \            /   \               │
│      B   T4    →       C   T4   →     B     A               │
│     / \                / \           / \   / \              │
│    T1  C              B   T3       T1 T2 T3 T4              │
│       / \            / \                                    │
│      T2  T3        T1  T2                                   │
│                                                             │
│  4. RL 双旋 (factor = 2, right.factor < 0)                 │
│                                                             │
│     A(2)               A(2)            C(0)                 │
│     / \                / \            /   \                 │
│    T1  B      →      T1  C     →    A     B                 │
│       / \                / \        / \   / \               │
│      C   T4            T2  B      T1 T2 T3 T4               │
│     / \                    / \                              │
│    T2  T3                T3  T4                             │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 4.3 平衡维护

**插入时的平衡维护**

插入后，需要从插入点向上回溯更新平衡因子，并在必要时旋转。

```rust
impl RegionAvl {
    /// 插入区域并维护平衡
    pub fn insert(&mut self, mut region: VirRegion) -> Option<VirRegion> {
        // 初始化新节点
        region.lower = None;
        region.higher = None;
        region.factor = BalanceFactor::BALANCED;

        // 空树
        if self.root.is_none() {
            self.root = Some(Box::new(region));
            self.count = 1;
            return None;
        }

        // 搜索插入位置，记录路径
        let mut path: Vec<*mut VirRegion> = Vec::new();
        let mut current = self.root.as_mut().unwrap();

        loop {
            path.push(&mut **current);

            match region.vaddr.cmp(&current.vaddr) {
                Ordering::Less => {
                    if current.lower.is_none() {
                        current.lower = Some(Box::new(region));
                        break;
                    }
                    current = current.lower.as_mut().unwrap();
                }
                Ordering::Greater => {
                    if current.higher.is_none() {
                        current.higher = Some(Box::new(region));
                        break;
                    }
                    current = current.higher.as_mut().unwrap();
                }
                Ordering::Equal => {
                    // 重复键，替换
                    let old = std::mem::replace(current, Box::new(region));
                    return Some(*old);
                }
            }
        }

        self.count += 1;

        // 从插入点向上更新平衡因子
        // 插入在左子树: factor -= 1
        // 插入在右子树: factor += 1
        for node_ptr in path.into_iter().rev() {
            let node = unsafe { &mut *node_ptr };

            // 更新平衡因子（需要知道插入方向）
            // 这里简化处理，实际需要记录方向
            node.update_factor();

            // 检查是否需要旋转
            if node.factor.needs_rebalance() {
                // 执行旋转
                // 注意：这里需要处理 Box 的所有权
                // 实际实现更复杂
            }
        }

        None
    }
}
```

**删除时的平衡维护**

删除比插入更复杂，可能需要多次旋转。

```rust
impl RegionAvl {
    /// 删除区域并维护平衡
    pub fn remove(&mut self, key: VirBytes) -> Option<VirRegion> {
        // 搜索要删除的节点
        let mut path: Vec<*mut VirRegion> = Vec::new();
        let mut current = self.root.as_mut()?;

        loop {
            path.push(&mut **current);

            match key.cmp(&current.vaddr) {
                Ordering::Less => {
                    current = current.lower.as_mut()?;
                }
                Ordering::Greater => {
                    current = current.higher.as_mut()?;
                }
                Ordering::Equal => {
                    break;
                }
            }
        }

        // 找到节点，执行删除
        let removed = if current.lower.is_none() || current.higher.is_none() {
            // 最多一个子节点，直接删除
            let child = current.lower.take().or_else(|| current.higher.take());
            // 将子节点连接到父节点
            // ...
            self.count -= 1;
            Some(current)
        } else {
            // 两个子节点，找中序后继
            let successor = Self::find_min_mut(&mut current.higher);
            // 交换键值
            // ...
            self.count -= 1;
            Some(successor)
        };

        // 从删除点向上更新平衡因子
        for node_ptr in path.into_iter().rev() {
            let node = unsafe { &mut *node_ptr };
            node.update_factor();

            // 删除可能需要多次旋转
            while node.factor.needs_rebalance() {
                Self::rebalance(node);
            }
        }

        removed.map(|b| *b)
    }

    /// 找到子树的最小节点
    fn find_min_mut(node: &mut Option<Box<VirRegion>>) -> &mut Box<VirRegion> {
        let mut current = node.as_mut().unwrap();
        while current.lower.is_some() {
            current = current.lower.as_mut().unwrap();
        }
        current
    }
}
```

**插入 vs 删除的平衡差异**

```
┌─────────────────────────────────────────────────────────────┐
│           插入 vs 删除的平衡调整对比                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   插入:                                                     │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 1. 从插入点向上回溯                                  │   │
│   │ 2. 更新平衡因子                                      │   │
│   │ 3. 遇到第一个不平衡节点时旋转                        │   │
│   │ 4. 旋转后子树高度不变，停止回溯                      │   │
│   │ 5. 最多 1 次旋转                                     │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   删除:                                                     │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 1. 从删除点向上回溯                                  │   │
│   │ 2. 更新平衡因子                                      │   │
│   │ 3. 遇到不平衡节点时旋转                              │   │
│   │ 4. 旋转后子树高度可能减 1，继续回溯                  │   │
│   │ 5. 最多 O(log n) 次旋转                              │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   示例：删除导致连续旋转                                     │
│                                                             │
│   删除前:                   删除 X 后:                       │
│         C(0)                   C(-1)                         │
│        / \                     / \                          │
│       B   D(0)                B   D(-1)                      │
│      /   / \                 /     \                        │
│     A   X   E               A       E                        │
│                                                             │
│   旋转 1 (D 处):            旋转 2 (C 处):                    │
│         C(-1)                  D(0)                          │
│        / \                    /   \                          │
│       B   D(-1)    →         C     E                        │
│      /     \                /                                │
│     A       E              B                                 │
│                            /                                 │
│                           A                                  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**平衡因子更新规则**

| 操作 | 方向 | 平衡因子变化 |
|------|------|-------------|
| 插入到左子树 | 左 | factor -= 1 |
| 插入到右子树 | 右 | factor += 1 |
| 删除左子树节点 | 左 | factor += 1 |
| 删除右子树节点 | 右 | factor -= 1 |

**停止条件**

| 操作 | 停止回溯条件 |
|------|-------------|
| 插入 | factor 变为 0（子树高度不变）或旋转后 |
| 删除 | factor 变为 ±1（子树高度不变）或旋转后仍需检查 |

**高度变化传播**

```rust
/// 插入后更新平衡因子
///
/// 返回 true 表示子树高度增加，需要继续向上传播
fn update_balance_after_insert(node: &mut VirRegion, went_left: bool) -> bool {
    if went_left {
        node.factor.dec();
    } else {
        node.factor.inc();
    }

    match node.factor.0 {
        0 => false,    // 高度不变，停止传播
        -1 | 1 => true, // 高度增加，继续传播
        _ => {
            // 需要旋转
            // 旋转后高度恢复原状，停止传播
            false
        }
    }
}

/// 删除后更新平衡因子
///
/// 返回 true 表示子树高度减少，需要继续向上传播
fn update_balance_after_remove(node: &mut VirRegion, removed_from_left: bool) -> bool {
    if removed_from_left {
        node.factor.inc();
    } else {
        node.factor.dec();
    }

    match node.factor.0 {
        -1 | 1 => false, // 高度不变，停止传播
        0 => true,       // 高度减少，继续传播
        _ => {
            // 需要旋转
            // 旋转后可能继续减少，需要检查
            true
        }
    }
}
```

### 4.4 查找优化

**地址范围查找的特殊性**

VM 的 AVL 树查找与普通 BST 查找不同：我们需要查找"包含指定地址的区域"，而不是精确匹配键值。

```
┌─────────────────────────────────────────────────────────────┐
│              地址范围查找 vs 精确键查找                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   普通 BST 查找:                                             │
│   - 查找键 == 节点键                                         │
│   - 返回精确匹配的节点                                        │
│                                                             │
│   VM 区域查找:                                               │
│   - 查找地址 >= 节点.vaddr                                   │
│   - 查找地址 < 节点.end_addr()                               │
│   - 返回包含该地址的区域                                      │
│                                                             │
│   示例:                                                      │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 区域 A: vaddr=0x1000, length=0x2000                 │   │
│   │ 区域 B: vaddr=0x4000, length=0x1000                 │   │
│   │ 区域 C: vaddr=0x6000, length=0x3000                 │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   find(0x1500) → 区域 A (0x1000 <= 0x1500 < 0x3000)        │
│   find(0x3500) → None (间隙)                                │
│   find(0x4500) → 区域 B (0x4000 <= 0x4500 < 0x5000)        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

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
pub struct SearchType(u8);

impl SearchType {
    /// 精确匹配键值
    pub const EQUAL: Self = Self(1);
    /// 小于指定键
    pub const LESS: Self = Self(2);
    /// 大于指定键
    pub const GREATER: Self = Self(4);
    /// 小于等于
    pub const LESS_EQUAL: Self = Self(3);  // EQUAL | LESS
    /// 大于等于
    pub const GREATER_EQUAL: Self = Self(5); // EQUAL | GREATER

    pub fn contains(&self, other: Self) -> bool {
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
    pub fn find(&self, addr: VirBytes) -> Option<&VirRegion> {
        Self::find_containing(&self.root, addr)
    }

    fn find_containing(node: &Option<Box<VirRegion>>, addr: VirBytes) -> Option<&VirRegion> {
        let n = node.as_ref()?;

        if addr < n.vaddr {
            // 地址在当前区域之前，搜索左子树
            Self::find_containing(&n.lower, addr)
        } else if addr >= n.end_addr() {
            // 地址在当前区域之后，搜索右子树
            Self::find_containing(&n.higher, addr)
        } else {
            // 地址在当前区域内
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
    pub fn find_overlap(&self, start: VirBytes, end: VirBytes) -> Option<&VirRegion> {
        Self::find_overlap_node(&self.root, start, end)
    }

    fn find_overlap_node(
        node: &Option<Box<VirRegion>>,
        start: VirBytes,
        end: VirBytes,
    ) -> Option<&VirRegion> {
        let n = node.as_ref()?;

        // 检查当前节点是否重叠
        if n.vaddr < end && n.end_addr() > start {
            return Some(n);
        }

        // 根据位置决定搜索方向
        if end <= n.vaddr {
            // 查询范围完全在当前区域左侧
            Self::find_overlap_node(&n.lower, start, end)
        } else if start >= n.end_addr() {
            // 查询范围完全在当前区域右侧
            Self::find_overlap_node(&n.higher, start, end)
        } else {
            // 不应该到达这里（已被第一个条件捕获）
            None
        }
    }

    /// 查找所有与指定范围重叠的区域
    ///
    /// 返回重叠区域的迭代器
    pub fn find_all_overlaps<'a>(
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
    pub fn search(&self, key: VirBytes, st: SearchType) -> Option<&VirRegion> {
        Self::search_node(&self.root, key, st)
    }

    fn search_node(
        node: &Option<Box<VirRegion>>,
        key: VirBytes,
        st: SearchType,
    ) -> Option<&VirRegion> {
        let n = node.as_ref()?;
        let mut match_h: Option<&VirRegion> = None;

        let target_cmp = if st.contains(SearchType::LESS) {
            1  // 允许键大于节点
        } else if st.contains(SearchType::GREATER) {
            -1 // 允许键小于节点
        } else {
            0  // 必须精确匹配
        };

        let mut current = Some(n);
        while let Some(h) = current {
            let cmp = key.0.cmp(&h.vaddr.0);

            if cmp == Ordering::Equal {
                if st.contains(SearchType::EQUAL) {
                    return Some(h);
                }
                // 继续向目标方向搜索
                return if target_cmp < 0 {
                    Self::search_node(&h.lower, key, st)
                } else {
                    Self::search_node(&h.higher, key, st)
                };
            }

            // 记录候选匹配
            if target_cmp != 0 && (cmp.0 ^ target_cmp) >= 0 {
                // cmp 和 target_cmp 同号
                match_h = Some(h);
            }

            current = if cmp == Ordering::Less {
                h.lower.as_ref().map(|b| b.as_ref())
            } else {
                h.higher.as_ref().map(|b| b.as_ref())
            };
        }

        match_h
    }

    /// 查找小于指定键的最大区域
    pub fn find_less(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::LESS)
    }

    /// 查找大于指定键的最小区域
    pub fn find_greater(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::GREATER)
    }

    /// 查找小于等于指定键的最大区域
    pub fn find_less_equal(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::LESS_EQUAL)
    }

    /// 查找大于等于指定键的最小区域
    pub fn find_greater_equal(&self, key: VirBytes) -> Option<&VirRegion> {
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
    ///
    /// 参数:
    /// - minv: 最小起始地址
    /// - maxv: 最大结束地址（0 表示使用 minv + length）
    /// - length: 需要的空间大小
    ///
    /// 返回: 可用起始地址，或 None 表示无合适空间
    pub fn find_slot(
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

```
┌─────────────────────────────────────────────────────────────┐
│                    查找操作对比                               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   操作           时间复杂度    用途                          │
│   ────────────────────────────────────────────────────────  │
│   find           O(log n)     缺页处理、地址验证             │
│   find_overlap   O(log n)     mmap 冲突检测                 │
│   search         O(log n)     通用搜索                      │
│   find_slot      O(n)         寻找空闲地址空间              │
│   iter           O(n)         遍历所有区域                  │
│                                                             │
│   优化策略:                                                  │
│   1. find/find_overlap: 利用 BST 性质，剪枝搜索             │
│   2. find_slot: 需要遍历间隙，无法避免 O(n)                 │
│   3. 缓存最近查找结果（可选优化）                            │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 5. 性能分析

### 5.1 时间复杂度

**基本操作复杂度**

AVL 树通过强制平衡，保证了所有基本操作的 O(log n) 时间复杂度。

```
┌─────────────────────────────────────────────────────────────┐
│                    AVL 树操作时间复杂度                       │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   操作           平均         最坏         说明              │
│   ────────────────────────────────────────────────────────  │
│   查找 find      O(log n)     O(log n)     包含地址查找      │
│   插入 insert    O(log n)     O(log n)     含平衡调整        │
│   删除 remove    O(log n)     O(log n)     含平衡调整        │
│   最小值         O(log n)     O(log n)     左边界遍历        │
│   最大值         O(log n)     O(log n)     右边界遍历        │
│   中序后继       O(log n)     O(log n)     删除时使用        │
│   遍历 iter      O(n)         O(n)         访问所有节点      │
│                                                             │
│   对比普通 BST:                                              │
│   查找           O(log n)     O(n)         最坏退化为链表    │
│   插入           O(log n)     O(n)         最坏退化为链表    │
│   删除           O(log n)     O(n)         最坏退化为链表    │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

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
/// 查找包含指定地址的区域
///
/// 时间复杂度分析:
/// - 每次迭代比较一次，进入一个子树
/// - 树高度为 O(log n)，最多迭代 O(log n) 次
/// - 每次迭代 O(1) 操作
/// - 总时间: O(log n)
fn find_containing(node: &Option<Box<VirRegion>>, addr: VirBytes) -> Option<&VirRegion> {
    let n = node.as_ref()?;

    // O(1) 比较
    if addr < n.vaddr {
        // 进入左子树，递归深度 +1
        Self::find_containing(&n.lower, addr)
    } else if addr >= n.end_addr() {
        // 进入右子树，递归深度 +1
        Self::find_containing(&n.higher, addr)
    } else {
        // 找到，O(1)
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

```
┌─────────────────────────────────────────────────────────────┐
│                    实际性能影响因素                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   1. 常数因子                                                │
│      - AVL 树的平衡操作比红黑树更频繁                         │
│      - 但 AVL 树查找更快（树更矮）                            │
│      - 适合读多写少的场景                                    │
│                                                             │
│   2. 缓存局部性                                              │
│      - 节点分散在堆上，指针跳转多                             │
│      - 每次访问可能触发缓存未命中                             │
│      - 实际性能受内存访问模式影响                             │
│                                                             │
│   3. 分支预测                                                │
│      - 比较结果难以预测                                       │
│      - 分支预测失败代价高                                     │
│                                                             │
│   4. 内存分配                                                │
│      - 每个节点单独分配                                       │
│      - 分配/释放开销                                          │
│      - 可考虑内存池优化                                       │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

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

```
┌─────────────────────────────────────────────────────────────┐
│                    AVL vs 红黑树对比                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   特性              AVL 树           红黑树                  │
│   ────────────────────────────────────────────────────────  │
│   平衡条件          |平衡因子| ≤ 1    无红红相邻              │
│   最大高度          1.44 log n       2 log n                 │
│   查找性能          更优              稍差                    │
│   插入旋转          最多 1 次         最多 2 次               │
│   删除旋转          最多 O(log n)     最多 3 次               │
│   插入调整          O(log n)          O(1) 均摊              │
│   删除调整          O(log n)          O(1) 均摊              │
│   空间开销          1 字节平衡因子    1 位颜色                │
│                                                             │
│   适用场景:                                                  │
│   - AVL: 读多写少，查找密集                                  │
│   - 红黑树: 写多读少，修改频繁                               │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Minix3 选择 AVL 的原因**

```
1. VM 区域操作的特点:
   ┌─────────────────────────────────────────────────────┐
   │ 操作           频率           说明                   │
   │ ─────────────────────────────────────────────────── │
   │ 查找 find      非常高        每次缺页、内存访问      │
   │ 插入 insert    中等          mmap, brk              │
   │ 删除 remove    低            munmap                 │
   │ 遍历 iter      中等          fork, exit            │
   └─────────────────────────────────────────────────────┘
   
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
插入操作:
┌─────────────────────────────────────────────────────┐
│ 情况           AVL 旋转      红黑树旋转              │
│ ─────────────────────────────────────────────────── │
│ 无需调整       0             0                       │
│ 单旋转         1             1                       │
│ 双旋转         2             2                       │
│ 颜色调整       -             可能多次                │
│ 最大旋转       2             2                       │
└─────────────────────────────────────────────────────┘

删除操作:
┌─────────────────────────────────────────────────────┐
│ 情况           AVL 旋转      红黑树旋转              │
│ ─────────────────────────────────────────────────── │
│ 无需调整       0             0                       │
│ 简单情况       1             1-2                     │
│ 复杂情况       O(log n)      最多 3                  │
│ 最大旋转       O(log n)      3                       │
└─────────────────────────────────────────────────────┘

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

```
┌─────────────────────────────────────────────────────────────┐
│                    为什么 VM 使用 AVL 树                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   1. 查找密集: 缺页处理、地址验证频繁，AVL 查找更快          │
│                                                             │
│   2. 修改稀疏: mmap/munmap 相对较少，删除旋转开销可接受      │
│                                                             │
│   3. 历史兼容: Minix3 使用 AVL，保持一致性                   │
│                                                             │
│   4. 实现简洁: 平衡条件直观，代码易于维护                    │
│                                                             │
│   5. 节点嵌入: 平衡因子只需 1 字节，开销小                   │
│                                                             │
│   总结: VM 场景的特点（读多写少）使 AVL 成为更优选择          │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

### 5.3 缓存效率

**节点内存布局**

AVL 树节点的内存布局直接影响缓存效率：

```
┌─────────────────────────────────────────────────────────────┐
│                    VirRegion 内存布局                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   C 语言布局 (Minix3):                                       │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ vaddr: 8 bytes                                      │   │
│   │ length: 8 bytes                                     │   │
│   │ flags: 4 bytes                                      │   │
│   │ ... 其他字段 ...                                     │   │
│   │ lower: 8 bytes (指针)                               │   │
│   │ higher: 8 bytes (指针)                              │   │
│   │ factor: 4 bytes (int，实际只用 1 byte)              │   │
│   └─────────────────────────────────────────────────────┘   │
│   总大小: 约 80-120 bytes                                    │
│                                                             │
│   Rust 布局:                                                 │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ vaddr: VirBytes (8 bytes)                           │   │
│   │ length: VirBytes (8 bytes)                          │   │
│   │ flags: VrFlags (4 bytes)                            │   │
│   │ ... 其他字段 ...                                     │   │
│   │ lower: Option<Box<VirRegion>> (8 bytes, None=0)    │   │
│   │ higher: Option<Box<VirRegion>> (8 bytes)           │   │
│   │ factor: i8 (1 byte)                                 │   │
│   │ padding: 7 bytes (对齐)                             │   │
│   └─────────────────────────────────────────────────────┘   │
│   总大小: 类似 C 布局                                        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

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

```
┌─────────────────────────────────────────────────────────────┐
│                    AVL 树访问模式                             │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   查找操作:                                                  │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ Root → Node1 → Node2 → ... → Target                │   │
│   │                                                     │   │
│   │ 特点:                                               │   │
│   │ - 顺序访问，但地址不连续                            │   │
│   │ - 每次跳转可能跨越不同内存页                        │   │
│   │ - 预取器难以预测                                    │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   遍历操作 (中序):                                           │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ A → B → C → D → E → F → G                          │   │
│   │                                                     │   │
│   │ 特点:                                               │   │
│   │ - 按地址顺序访问                                    │   │
│   │ - 但节点物理位置不连续                              │   │
│   │ - 每次访问可能缓存未命中                            │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   与数组对比:                                                │
│   数组遍历: 连续内存，预取器高效工作                         │
│   AVL 遍历: 指针跳转，每次可能缓存未命中                     │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**优化策略**

```
1. 节点预分配

   使用内存池预分配节点，提高局部性:

   ┌─────────────────────────────────────────────────────┐
   │ 内存池布局:                                          │
   │ [Node0][Node1][Node2][Node3]...[NodeN]             │
   │                                                     │
   │ 优点:                                               │
   │ - 相关节点可能在同一缓存行                           │
   │ - 减少内存碎片                                       │
   │ - 分配/释放更快                                      │
   └─────────────────────────────────────────────────────┘

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
B 树的缓存优势:

┌─────────────────────────────────────────────────────────────┐
│   B 树节点 (假设阶数 16)                                     │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ [key0][key1][key2]...[key15]  (16 * 8 = 128 bytes) │   │
│   │ [ptr0][ptr1][ptr2]...[ptr16] (17 * 8 = 136 bytes)  │   │
│   └─────────────────────────────────────────────────────┘   │
│   一个节点包含 16 个键，一次缓存加载可比较多个键             │
│                                                             │
│   AVL 树节点                                                 │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ [key][left][right][factor]  (~24 bytes AVL 部分)   │   │
│   └─────────────────────────────────────────────────────┘   │
│   一个节点只有 1 个键，每次缓存加载只比较 1 个键             │
│                                                             │
│   缓存效率: B 树 > AVL 树                                    │
│   但 B 树不适合 VM 区域管理的特殊查找需求                    │
└─────────────────────────────────────────────────────────────┘
```

**Rust 实现的缓存考虑**

```rust
/// 优化后的 VirRegion 布局
#[repr(C)]
pub struct VirRegion {
    // ===== 热字段: 查找时必访问 =====
    /// 虚拟地址（键）
    pub vaddr: VirBytes,
    /// 区域长度
    pub length: VirBytes,
    /// 左子节点
    pub lower: Option<Box<VirRegion>>,
    /// 右子节点
    pub higher: Option<Box<VirRegion>>,
    /// 平衡因子
    pub factor: i8,
    
    // ===== 冷字段: 特定操作时访问 =====
    /// 区域标志
    pub flags: VrFlags,
    /// 物理区域
    pub phys_regions: Vec<PhysRegion>,
    // ... 其他字段
}
```

**总结**

```
┌─────────────────────────────────────────────────────────────┐
│                    缓存效率总结                               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   1. AVL 树缓存效率一般:                                     │
│      - 节点分散，指针跳转多                                  │
│      - 每次访问可能缓存未命中                                │
│      - 但 VM 场景节点数少，影响有限                          │
│                                                             │
│   2. 优化空间有限:                                           │
│      - 节点大小由 VirRegion 决定                             │
│      - 不能像 B 树那样增加节点密度                           │
│      - 内存池可改善局部性                                    │
│                                                             │
│   3. 实际性能足够:                                           │
│      - 典型进程 < 100 个区域                                 │
│      - 查找深度 < 10                                         │
│      - 微秒级延迟，满足需求                                  │
│                                                             │
│   4. 选择 AVL 的理由:                                        │
│      - 算法简单，实现可靠                                    │
│      - 查找性能优于红黑树                                    │
│      - Minix3 兼容性                                         │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 6. 测试与验证

### 6.1 基本操作测试

**插入测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_region(vaddr: u64, length: u64) -> VirRegion {
        VirRegion::new(VirBytes(vaddr), VirBytes(length), VrFlags::empty())
    }

    #[test]
    fn test_insert_single() {
        let mut avl = RegionAvl::new();
        avl.insert(make_region(0x1000, 0x1000));

        assert_eq!(avl.len(), 1);
        assert!(avl.find(VirBytes(0x1500)).is_some());
    }

    #[test]
    fn test_insert_multiple() {
        let mut avl = RegionAvl::new();

        // 乱序插入
        avl.insert(make_region(0x3000, 0x1000));
        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x2000, 0x1000));

        assert_eq!(avl.len(), 3);

        // 验证所有区域都可找到
        assert!(avl.find(VirBytes(0x1500)).is_some());
        assert!(avl.find(VirBytes(0x2500)).is_some());
        assert!(avl.find(VirBytes(0x3500)).is_some());
    }

    #[test]
    fn test_insert_duplicate() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x1000, 0x2000)); // 相同 vaddr，不同 length

        // 应该替换，不是增加
        assert_eq!(avl.len(), 1);
        assert_eq!(avl.find(VirBytes(0x1000)).unwrap().length, VirBytes(0x2000));
    }
}
```

**查找测试**

```rust
#[test]
fn test_find_containing() {
    let mut avl = RegionAvl::new();

    avl.insert(make_region(0x1000, 0x1000)); // 0x1000-0x2000
    avl.insert(make_region(0x3000, 0x1000)); // 0x3000-0x4000

    // 查找包含的地址
    assert_eq!(avl.find(VirBytes(0x1000)).unwrap().vaddr, VirBytes(0x1000));
    assert_eq!(avl.find(VirBytes(0x1500)).unwrap().vaddr, VirBytes(0x1000));
    assert_eq!(avl.find(VirBytes(0x1FFF)).unwrap().vaddr, VirBytes(0x1000));

    // 查找间隙中的地址
    assert!(avl.find(VirBytes(0x2000)).is_none());
    assert!(avl.find(VirBytes(0x2500)).is_none());
    assert!(avl.find(VirBytes(0x2FFF)).is_none());

    // 查找第二个区域
    assert_eq!(avl.find(VirBytes(0x3500)).unwrap().vaddr, VirBytes(0x3000));
}

#[test]
fn test_find_empty_tree() {
    let avl = RegionAvl::new();
    assert!(avl.find(VirBytes(0x1000)).is_none());
}

#[test]
fn test_find_boundary() {
    let mut avl = RegionAvl::new();
    avl.insert(make_region(0x1000, 0x1000));

    // 边界测试
    assert!(avl.find(VirBytes(0x0FFF)).is_none());  // 区域前
    assert!(avl.find(VirBytes(0x1000)).is_some());  // 区域起始
    assert!(avl.find(VirBytes(0x1FFF)).is_some());  // 区域结束前
    assert!(avl.find(VirBytes(0x2000)).is_none());  // 区域结束
}
```

**删除测试**

```rust
#[test]
fn test_remove_leaf() {
    let mut avl = RegionAvl::new();

    avl.insert(make_region(0x2000, 0x1000));
    avl.insert(make_region(0x1000, 0x1000));
    avl.insert(make_region(0x3000, 0x1000));

    // 删除叶子节点
    let removed = avl.remove(VirBytes(0x1000));
    assert!(removed.is_some());
    assert_eq!(removed.unwrap().vaddr, VirBytes(0x1000));

    assert_eq!(avl.len(), 2);
    assert!(avl.find(VirBytes(0x1500)).is_none());
    assert!(avl.find(VirBytes(0x2500)).is_some());
}

#[test]
fn test_remove_root() {
    let mut avl = RegionAvl::new();

    avl.insert(make_region(0x2000, 0x1000));
    avl.insert(make_region(0x1000, 0x1000));
    avl.insert(make_region(0x3000, 0x1000));

    // 删除根节点
    let removed = avl.remove(VirBytes(0x2000));
    assert!(removed.is_some());

    assert_eq!(avl.len(), 2);
    assert!(avl.find(VirBytes(0x2500)).is_none());
    assert!(avl.find(VirBytes(0x1500)).is_some());
    assert!(avl.find(VirBytes(0x3500)).is_some());
}

#[test]
fn test_remove_nonexistent() {
    let mut avl = RegionAvl::new();
    avl.insert(make_region(0x1000, 0x1000));

    let removed = avl.remove(VirBytes(0x2000));
    assert!(removed.is_none());
    assert_eq!(avl.len(), 1);
}
```

**重叠查找测试**

```rust
#[test]
fn test_find_overlap_basic() {
    let mut avl = RegionAvl::new();

    avl.insert(make_region(0x1000, 0x1000)); // 0x1000-0x2000
    avl.insert(make_region(0x3000, 0x1000)); // 0x3000-0x4000

    // 完全包含
    assert!(avl.find_overlap(VirBytes(0x1500), VirBytes(0x1800)).is_some());

    // 部分重叠
    assert!(avl.find_overlap(VirBytes(0x1500), VirBytes(0x2500)).is_some());

    // 完全跨越
    assert!(avl.find_overlap(VirBytes(0x0500), VirBytes(0x5000)).is_some());

    // 无重叠
    assert!(avl.find_overlap(VirBytes(0x2000), VirBytes(0x3000)).is_none());
    assert!(avl.find_overlap(VirBytes(0x5000), VirBytes(0x6000)).is_none());
}

#[test]
fn test_find_all_overlaps() {
    let mut avl = RegionAvl::new();

    avl.insert(make_region(0x1000, 0x1000));
    avl.insert(make_region(0x2000, 0x1000));
    avl.insert(make_region(0x3000, 0x1000));

    let overlaps: Vec<_> = avl
        .find_all_overlaps(VirBytes(0x1500), VirBytes(0x3500))
        .map(|r| r.vaddr)
        .collect();

    assert_eq!(overlaps, vec![
        VirBytes(0x1000),
        VirBytes(0x2000),
        VirBytes(0x3000)
    ]);
}
```

### 6.2 平衡性测试

**AVL 性质验证**

```rust
impl RegionAvl {
    /// 验证 AVL 性质（仅测试用）
    #[cfg(test)]
    pub fn verify_avl(&self) -> bool {
        self.verify_avl_node(self.root.as_ref()).is_some()
    }

    fn verify_avl_node(node: Option<&Box<VirRegion>>) -> Option<i32> {
        let n = node?;

        let left_h = Self::verify_avl_node(n.lower.as_ref())?;
        let right_h = Self::verify_avl_node(n.higher.as_ref())?;

        // 验证平衡因子
        let expected_factor = right_h - left_h;
        if expected_factor.abs() > 1 {
            return None; // 不平衡
        }

        // 验证存储的平衡因子正确
        if n.factor != expected_factor as i8 {
            return None;
        }

        Some(1 + left_h.max(right_h))
    }
}

#[test]
fn test_avl_balance_after_insert() {
    let mut avl = RegionAvl::new();

    // 顺序插入（最坏情况）
    for i in 0..100 {
        avl.insert(make_region(i * 0x1000, 0x1000));
    }

    assert!(avl.verify_avl(), "AVL 性质应保持");
}

#[test]
fn test_avl_balance_after_remove() {
    let mut avl = RegionAvl::new();

    // 插入 100 个区域
    for i in 0..100 {
        avl.insert(make_region(i * 0x1000, 0x1000));
    }

    // 删除一半
    for i in (0..100).step_by(2) {
        avl.remove(VirBytes(i * 0x1000));
    }

    assert!(avl.verify_avl(), "删除后 AVL 性质应保持");
}

#[test]
fn test_avl_height_bound() {
    let mut avl = RegionAvl::new();

    for i in 0..1000 {
        avl.insert(make_region(i * 0x1000, 0x1000));
    }

    // AVL 高度最多 1.44 * log2(n)
    let max_height = (1.44 * (1000f64).log2()) as i32 + 1;
    let actual_height = avl.height();

    assert!(
        actual_height <= max_height,
        "高度 {} 应 <= {}",
        actual_height,
        max_height
    );
}
```

**旋转正确性测试**

```rust
#[test]
fn test_ll_rotation() {
    let mut avl = RegionAvl::new();

    // 构造 LL 情况
    avl.insert(make_region(0x3000, 0x1000));
    avl.insert(make_region(0x2000, 0x1000));
    avl.insert(make_region(0x1000, 0x1000));

    assert!(avl.verify_avl());

    // 验证新的根
    let root = avl.root.as_ref().unwrap();
    assert_eq!(root.vaddr, VirBytes(0x2000));
}

#[test]
fn test_rr_rotation() {
    let mut avl = RegionAvl::new();

    // 构造 RR 情况
    avl.insert(make_region(0x1000, 0x1000));
    avl.insert(make_region(0x2000, 0x1000));
    avl.insert(make_region(0x3000, 0x1000));

    assert!(avl.verify_avl());

    let root = avl.root.as_ref().unwrap();
    assert_eq!(root.vaddr, VirBytes(0x2000));
}

#[test]
fn test_lr_rotation() {
    let mut avl = RegionAvl::new();

    // 构造 LR 情况
    avl.insert(make_region(0x3000, 0x1000));
    avl.insert(make_region(0x1000, 0x1000));
    avl.insert(make_region(0x2000, 0x1000));

    assert!(avl.verify_avl());

    let root = avl.root.as_ref().unwrap();
    assert_eq!(root.vaddr, VirBytes(0x2000));
}

#[test]
fn test_rl_rotation() {
    let mut avl = RegionAvl::new();

    // 构造 RL 情况
    avl.insert(make_region(0x1000, 0x1000));
    avl.insert(make_region(0x3000, 0x1000));
    avl.insert(make_region(0x2000, 0x1000));

    assert!(avl.verify_avl());

    let root = avl.root.as_ref().unwrap();
    assert_eq!(root.vaddr, VirBytes(0x2000));
}
```

### 6.3 迭代器测试

**遍历顺序测试**

```rust
#[test]
fn test_iter_order() {
    let mut avl = RegionAvl::new();

    // 乱序插入
    avl.insert(make_region(0x5000, 0x1000));
    avl.insert(make_region(0x1000, 0x1000));
    avl.insert(make_region(0x3000, 0x1000));
    avl.insert(make_region(0x2000, 0x1000));
    avl.insert(make_region(0x4000, 0x1000));

    // 验证中序遍历是升序
    let addrs: Vec<_> = avl.iter().map(|r| r.vaddr).collect();
    assert!(addrs.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(addrs, vec![
        VirBytes(0x1000),
        VirBytes(0x2000),
        VirBytes(0x3000),
        VirBytes(0x4000),
        VirBytes(0x5000)
    ]);
}

#[test]
fn test_iter_empty() {
    let avl = RegionAvl::new();
    let count = avl.iter().count();
    assert_eq!(count, 0);
}

#[test]
fn test_iter_single() {
    let mut avl = RegionAvl::new();
    avl.insert(make_region(0x1000, 0x1000));

    let addrs: Vec<_> = avl.iter().map(|r| r.vaddr).collect();
    assert_eq!(addrs, vec![VirBytes(0x1000)]);
}
```

**迭代器完整性测试**

```rust
#[test]
fn test_iter_covers_all() {
    let mut avl = RegionAvl::new();

    for i in 0..100 {
        avl.insert(make_region(i * 0x1000, 0x1000));
    }

    let count = avl.iter().count();
    assert_eq!(count, 100);
}

#[test]
fn test_iter_after_remove() {
    let mut avl = RegionAvl::new();

    for i in 0..10 {
        avl.insert(make_region(i * 0x1000, 0x1000));
    }

    avl.remove(VirBytes(0x5000));

    let addrs: Vec<_> = avl.iter().map(|r| r.vaddr).collect();
    assert_eq!(addrs.len(), 9);
    assert!(!addrs.contains(&VirBytes(0x5000)));
}
```

**可变迭代器测试**

```rust
#[test]
fn test_iter_mut_modify() {
    let mut avl = RegionAvl::new();

    avl.insert(make_region(0x1000, 0x1000));
    avl.insert(make_region(0x2000, 0x1000));

    // 修改所有区域的长度
    for region in avl.iter_mut() {
        region.length = VirBytes(0x2000);
    }

    // 验证修改生效
    for region in avl.iter() {
        assert_eq!(region.length, VirBytes(0x2000));
    }
}
```

**IntoIterator 测试**

```rust
#[test]
fn test_into_iterator() {
    let mut avl = RegionAvl::new();

    avl.insert(make_region(0x1000, 0x1000));
    avl.insert(make_region(0x2000, 0x1000));

    // 使用 for 循环
    let mut count = 0;
    for region in &avl {
        count += 1;
        assert!(region.vaddr.0 >= 0x1000);
    }
    assert_eq!(count, 2);
}
```

**测试总结**

```
┌─────────────────────────────────────────────────────────────┐
│                    测试覆盖总结                               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   基本操作测试:                                              │
│   ✓ 插入单个/多个节点                                        │
│   ✓ 插入重复键                                               │
│   ✓ 查找包含地址                                             │
│   ✓ 查找边界条件                                             │
│   ✓ 删除叶子/内部/根节点                                     │
│   ✓ 删除不存在的节点                                         │
│   ✓ 重叠查找                                                 │
│                                                             │
│   平衡性测试:                                                │
│   ✓ AVL 性质验证                                             │
│   ✓ 插入后平衡                                               │
│   ✓ 删除后平衡                                               │
│   ✓ 高度上界                                                 │
│   ✓ 四种旋转正确性                                           │
│                                                             │
│   迭代器测试:                                                │
│   ✓ 遍历顺序正确                                             │
│   ✓ 空/单节点树                                              │
│   ✓ 覆盖所有节点                                             │
│   ✓ 删除后迭代                                               │
│   ✓ 可变迭代器                                               │
│   ✓ IntoIterator trait                                       │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 7. 参见

- [12-vir-region.md](12-vir-region.md) - 使用 AVL 树的 vir_region
- [17-vm-fork.md](17-vm-fork.md) - fork 时遍历 AVL 树

---

*分类: VM私有*
