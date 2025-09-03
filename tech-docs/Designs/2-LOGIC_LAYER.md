# 套利系统逻辑层设计文档

```md
套利系统中的图有两个核心元素：

1.  **图的拓扑结构 (Topology):** 谁和谁相连（即哪些池子存在）。这个结构在系统运行期间是**静态的**。
2.  **边的权重 (Edge Weights):** 交易对的价格/`Reserves`。这个数据是**动态的**，每个区块都在变。

最高效的架构是：**一次性找出所有可能的套利路径（拓扑问题），然后在每次数据更新时，只对这些固定的路径进行快速的数学计算（权重问题）。**
```

## 1\. 概述 (Overview)

逻辑层是套利系统的大脑。它接收由数据层提供的实时市场快照，通过高效的算法分析其中蕴含的套利机会，并量化出潜在的利润。

本设计的核心原则是**性能至上**。为了实现极致的速度，我们将采取一种**预计算与实时计算分离**的架构，以确保在每个新区块到达后的几毫秒内完成所有潜在机会的评估。

## 2\. 算法选型：预计算 vs. 实时图遍历

在设计之初，我们评估了两种核心算法思路：

### 2.1 方案 A: 实时图遍历 (以 SPFA 为例)

这个方案的思路是在每次收到新的 `Reserves` 数据时，根据最新的价格动态构建一个加权有向图，然后运行 SPFA 或 Bellman-Ford 算法来寻找负权环。

  - **优点:** 理论上非常灵活，能发现任意长度的套利路径。
  - **缺点 (致命):**
    1.  **性能极差:** SPFA 的时间复杂度在最坏情况下为 O(VE)，对于实时系统来说开销过大。在 2 秒的区块时间内反复运行它，会消耗大量宝贵的计算时间。
    2.  **冗余计算:** 我们的目标仅限于 3-hop 和 4-hop 路径。运行一个能寻找所有路径的通用算法，99% 的计算都是无效的。
    3.  **不利于并行:** 图遍历算法本身的并行化改造非常复杂。

### 2.2 方案 B: 路径预计算 + 并行化利润计算 (推荐)

这个方案将问题一分为二：

1.  **路径发现 (Pathfinding - 系统初始化时执行一次):**

      - 将市场抽象为一个**无权图**，只关心代币之间的连接关系。
      - 从起点 `WMNT` 开始，使用**深度优先搜索 (DFS)** 算法，并**限制搜索深度为 4**，找出所有长度为 3 和 4 的、回到 `WMNT` 的简单环路。
      - 将这些路径的**静态拓扑结构**（例如 `[WMNT, mETH, PUFF, WMNT]`）缓存到内存列表中。

2.  **利润计算 (Profit Calculation - 每次数据更新时执行):**

      - 当数据层传来新的 `Reserves` 快照时，系统**不会进行任何图遍历**。
      - 而是直接**并行遍历**预计算好的路径列表。
      - 对每一条路径，进行纯粹的、高速的数学计算，以评估其潜在利润。

### 对比总结

| 特性 | 方案 A (SPFA 实时遍历) | 方案 B (预计算 + 并行计算) | 结论 |
| :--- | :--- | :--- | :--- |
| **实时性能** | 慢 (毫秒级甚至更高) | **极快** (微秒级) | **方案 B 完胜** |
| **计算复杂度** | O(VE) | O(k)，k 为预计算路径数 | **方案 B 完胜** |
| **并行能力** | 差 | **极佳 (Embarrassingly Parallel)** | **方案 B 完胜** |
| **资源利用** | 低效，大量冗余计算 | 高效，计算都用在刀刃上 | **方案 B 完胜** |
| **实现复杂度** | 较高 (实时图构建+算法) | 中等 (一次性 DFS + 简单数学) | 方案 B 更清晰 |

**结论：** 方案 B 完美契合我们的性能要求，是构建高性能套利逻辑层的正确选择。

## 3\. 架构与工作流

逻辑层 (`ArbitrageEngine`) 在内部由两个主要组件和一个数据流构成。

### **架构图 (Mermaid)**

```mermaid
graph TD
    subgraph Data Layer
        A[Market Data Poller]
    end
    
    subgraph Logic Layer
        B(Channel Receiver)
        C{Pathfinder (Init)}
        D[Parallel Profit Calculator]
        E[Arbitrage Opportunities]
    end
    
    subgraph Application Startup
        F(Load Config) --> C
    end

    A -- new reserves snapshot --> B
    C -- pre-computed paths --> D
    B -- triggers --> D
    D -- found --> E
```

### **工作流程**

1.  **初始化阶段 (Application Startup):**

      - 系统启动，加载 `selected.csv` 等配置文件。
      - `Pathfinder` 组件基于池子的连接关系构建图。
      - 运行一次性的深度受限 DFS，找出所有 3-hop 和 4-hop 的 `WMNT` 环路，并将结果（一个路径列表）交给 `Parallel Profit Calculator`。

2.  **实时循环阶段 (Real-time Loop):**

      - `Channel Receiver` 异步等待数据层通过 Channel 发送过来的最新 `Reserves` 快照。
      - 一旦收到新数据，立即触发 `Parallel Profit Calculator`。
      - `Parallel Profit Calculator` 启动一个**并行任务**（例如，使用 Rust 的 Rayon 库）。
      - 它将预计算好的路径列表分配到多个 CPU 核心上，同时对所有路径进行利润计算。
      - 如果发现任何一条路径的净利润（扣除 Gas 费后）大于预设阈值，就将其格式化为一个 `ArbitrageOpportunity` 对象，并输出或存入结果队列。

## 4\. 实现细节

### 4.1 路径发现 (Pathfinder)

  - **算法:** 深度优先搜索 (DFS) 的一个变体。
  - **伪代码:**
    ```
    function find_cycles(start_node, max_depth):
        all_cycles = []
        stack = [(start_node, [start_node])] // (current_node, path_taken)
        
        while stack is not empty:
            (current_node, path) = stack.pop()
            
            if length(path) > max_depth + 1:
                continue

            for neighbor in neighbors_of(current_node):
                if neighbor == start_node and length(path) in [3, 4]:
                    // Found a valid cycle
                    cycle = path + [neighbor]
                    all_cycles.add(cycle)
                    continue
                
                if neighbor not in path:
                    // Continue search
                    new_path = path + [neighbor]
                    stack.push((neighbor, new_path))
        
        return all_cycles
    ```

### 4.2 利润计算 (Profit Calculator)

这是系统的“热路径 (Hot Path)”，必须极致优化。

1.  **输入:** 一条固定的路径（如 `[T1, T2, T3, T1]`）和一个包含所有池子最新 `Reserves` 的 `HashMap`。
2.  **核心公式:** 链式调用 `getAmountOut`。
    `amount_out = getAmountOut(getAmountOut(getAmountOut(amount_in, Pool1), Pool2), Pool3)`
3.  **最优输入额求解:**
      - 对于一条给定的路径，利润并不是随着输入金额线性增长的。存在一个**最优输入金额 (Optimal Input Amount)** 可以使利润最大化。
      - 这个最优值可以通过**数值优化算法**快速求解。由于函数是单峰的，可以使用**三分搜索 (Ternary Search)** 或其他梯度上升方法，在一个合理的范围内（例如 0.1 - 10 WMNT）快速找到近似最优解。
4.  **并行化实现 (以 Rust + Rayon 为例):**
    ```rust
    // all_precomputed_paths: Vec<SwapPath>
    // latest_reserves: &HashMap<Address, Reserves>

    let opportunities: Vec<ArbitrageOpportunity> = all_precomputed_paths
        .par_iter() // <-- Rayon's parallel iterator
        .filter_map(|path| {
            // 1. Calculate optimal input and max profit for this path
            //    using latest_reserves.
            let result = calculate_max_profit_for_path(path, latest_reserves);

            // 2. Subtract gas cost and check against threshold.
            if result.net_profit_usd > PROFIT_THRESHOLD {
                Some(result.to_opportunity())
            } else {
                None
            }
        })
        .collect();
    ```

通过这种架构，我们将重量级的图算法与轻量级的实时计算彻底分离，确保逻辑层能够以最高的效率处理数据流，为在瞬息万变的市场中捕捉套利机会提供了坚实的性能保障。