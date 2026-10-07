现在这版，我的判断比 v4.9 更明确：

> **架构方案已经可以 FROZEN。**
>
> 但我会在真正把状态改成 `FROZEN` 前，再修 **两个很小但真实存在的“冻结措辞/定义问题”**。它们不是新方案，也不是 blocker 级架构问题。

### 先说结论

整体已经形成一个非常完整的闭环：

```text
pt_init
  ↓ semantic decomposition
Layer 1: address-space identity
  ↓
A1 adoption
  ↓
唯一 canonical self-root
  ↓
PhysAccessRequired / DM coverage
  ↓
bootstrap resource provenance
  ↓
A2 post-bootstrap classification
  ↓
VM PMM qualification
  ↓
P1/P2 + Proof 2
  ↓
Gate 8 反向攻击
  ↓
paging_init / bind_to_process 删除
```

而且 v4.10 把我上一轮要求的四个事实检查全部做掉了。特别是：

**① bootstrap DM split 不再有隐含循环。**
你已经把“建立 DM 的 PT 页”与“使用 DM 访问 PT 页”彻底分开：

```text
分配 PT page
    ↓
identity VA 写入
    ↓
挂入 root
    ↓
DM 建立完成
    ↓
之后运行期才使用 DM
```

这个闭环是成立的。

**② 两源 candidate 问题已经真正解决。**
尤其你发现：

> root 和 boot-shim bump 都是 `LOADER_DATA`，不是 conventional memmap。

这个修正很重要，否则之前的 `memmap → DM` 推导确实永远漏 root。

**③ A2 的时序已经真正闭合。**

你现在明确区分：

```text
select_multi = bootstrap pool selection
free_regions = final VM resource classification
```

这比之前“移动 select_multi”准确很多。

**④ 幽灵已经被限制在 D8-④/⑤。**

这说明 `paging_init` 已经没有任何设计职责了。

所以，**不要再开第 14 轮方案评审。**

---

# 但我发现两个值得在 Freeze 前修掉的小问题

## 1. “两源使用同一 resource filtering”这一句，严格说仍然有歧义

你现在写的是：

```text
DM mapping candidates
    = resource-classified memmap ranges
      ∪ explicit bootstrap PhysAccess ranges
    → resource filtering（Table A/B 逐类裁决；两源都过同一裁决）
```

这句话容易造成一个数学上的悖论。

因为你前面又定义了：

```text
VM PMM eligible
    = conventional
      ∩ DM-representable
      − LiveBootstrap
```

而 root 明明是：

```text
root ∉ conventional memmap
```

所以如果“两源都过同一 resource filtering”被理解成：

```text
source2 → conventional ∩ representable
```

root 又会被过滤掉。

你实际上想表达的应该是：

> **两源接受相同的 DM admissibility / window / page-granularity 约束，但来源类别的 qualification predicate 不相同。**

也就是：

```text
source 1:
    conventional VM-managed RAM
        → conventional qualification
        → DM representability

source 2:
    explicit bootstrap PhysAccess
        → explicit bootstrap qualification
        → DM representability

                    ↓

              common DM mapping policy
                    ↓
              window / fragmentation
                    ↓
              page-size selection
```

这个问题不是新 blocker，但我建议 **在 Freeze 前改一句**，因为你整个 §6.1 最重要的证明之一就是“root 不在 conventional memmap，但仍进入 coverage”。

可以直接把：

> 两源都过同一 resource filtering

改成：

> **两源均接受相同的 DM representability、窗口边界、资源安全性与 page-size containment 规则，但各源保持自身的资源类别资格判据；bootstrap PhysAccess 不适用 conventional-RAM qualification。**

这样就完全没有歧义了。

---

## 2. AArch64 的 “latent” 已经接近设计边界，最好把它明确成 implementation precondition

这是 v4.10 唯一一个我认为需要认真处理的点。

你现在已经查实：

```text
AArch64:
VM DM = [0, 1 GiB)
QEMU virt RAM starts at 1 GiB
```

于是：

```text
VM PMM eligible = ∅
```

而且更进一步：

```text
self root / bootstrap PT
    必须 DM-covered
```

你因此提出：

> AArch64 `VM_DIRECT_MAP_SIZE` 常量调整为 [latent] 实施项。

这在**架构模型层面没有问题**，但“address-layout constant”本身是设计输入，不完全像普通实现细节。

所以这里建议不要再写成一个模糊的 `[latent] implementation item`，而是明确成：

> **当前 AArch64 常量不满足本专项规定的 DM-window admissibility precondition；AArch64 实施必须在启用 VM PMM 前将窗口调整到满足该 precondition 的布局。该调整不改变本专项任何不变量、资格规则或映射模型。**

这样非常干净：

```text
本专项冻结：
    invariant / qualification / identity / coverage / lifecycle

AArch64 当前问题：
    arch-specific constant 尚未满足 invariant 的可实现前提

所以：
    不是重新设计
    也不是本专项 blocker
    是实现前必须满足的 arch precondition
```

这比“latent”更适合作为 FROZEN 文档。

否则 reviewer 很可能抓着：

> “你自己刚证明 AArch64 当前窗口让整个 VM impossible，为什么还说三架构 closed？”

然后又开一轮没必要的讨论。

---

# 还有一个我会顺手改，但不是 blocker

你现在 D1-C 写：

> `t_reconc = adoption 完成时点`

但你的实际运行时链是：

```text
bootstrap allocations
        ↓
LiveBootstrap closed
        ↓
post-bootstrap classification
        ↓
free_regions finalized
        ↓
adoption
        ↓
PMM enabled
```

所以从严格术语来说：

```text
resource classification / exclusion
```

实际上发生在 adoption **之前**。

你又一直强调：

> identity transition ≠ resource transition

因此这里最好把 `t_reconc` 的名字再稍微去耦。

例如：

```text
t_classify = free_regions 定稿
t_adopt    = A1 adoption 完成
t_enable   = VM PMM enabled
```

然后：

```text
resource record active lifetime:
    [first bootstrap allocation, t_classify]

handoff closure:
    t_classify → t_adopt → t_enable
```

或者你坚持 `t_reconc` 这个名字，也可以定义成：

> “bootstrap resource handoff sequence 的 closure point”

而不要让读者以为：

```text
t_reconc = A2 实际发生的瞬间
```

**这是措辞问题，不是模型问题。**

---

# Gate 现在怎么判断？

按你自己的 §9，我会这样打：

| Gate                                          | 当前判断                         |
| --------------------------------------------- | ---------------------------- |
| Gate 1 D1-D7                                  | ✅                            |
| Gate 2 Proof 1                                | ✅ design closed              |
| Gate 3 Proof 2                                | ✅                            |
| Gate 4 D2 / coverage / permissions / capacity | ✅，但 AArch64 precondition 应明确 |
| Gate 5 D6 semantic checklist                  | ✅                            |
| Gate 6 G1-G6 ownership                        | ✅                            |
| Gate 7 external review closure                | ✅                            |
| Gate 8 six-case attack                        | ✅                            |

所以：

> **没有新的 Freeze blocker。**

---

# 还有一个很重要的区别

现在不要把：

> `E9 runtime first validation`
> `x86 split branch implementation`
> `explicit two-source registration`
> `AllocateMaxAddress`
> `AArch64 window adjustment`
> `VM-side PTE access backend`

继续当成“Freeze 前设计问题”。

它们现在已经全部变成：

> **Frozen design → implementation obligations / validation obligations**

这正是你应该达到的状态。

尤其 `paging_init` 这一点，现在已经非常干净了：

```text
C pt_init
    ↓
semantic decomposition
    ├── identity → A1
    ├── resource accounting → A2
    ├── mapping → boot path / DM
    ├── page-size capability → HugePages
    ├── runtime binding → switch_address_space
    └── rebuild artifact → [ARCH] eliminated

Rust:
    不存在一个“pt_init replacement function”
```

这其实比简单回答“删掉 paging_init”更重要——**你已经把 C 的生命周期语义拆开并重新归位了。**

### 我的建议

把上面 **两处必须精修**：

1. “两源都过同一 resource filtering”改成“相同 DM policy，不同 source qualification”；
2. AArch64 当前窗口改成明确的 **arch-specific implementation precondition**；

`t_reconc` 那处作为可选的术语清理。

改完之后，我会直接把它视为：

> **FROZEN。停止继续 review 设计，进入 D8 实施。**

而且这次是真的可以停，不需要再造第 14/15 轮。
