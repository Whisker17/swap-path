# 多路径多池原子套利设计

## Prerequisite

1. Merchant Moe 中存在两种池子设计，一种是类似于 Uniswao v2 的 MoeLP 池，即维持最简单的 x*y=z 作为 reserves 的依据，另一种则是类似于 Trade Joe/Uniswap v3 的 tick 设计

## 单 DEX 的多池套利

1. 选择 Merchant Moe 中的 MoeLP 结构池，爬取所有目前存在流动性的 MoeLP 池，构建一个资产池图，将每个币种看做一个顶点，每个交易对就是一条边，问题就转化成了如何在一个有向图里面寻找环状路径的问题
2. 通过图论的方式，来计算，需要注意的是路径每增加一步就要消耗更多 gas，因此需要控制长度(maxHops)，在这里选择的是 3hops 或者 4 hops
3. 提前查询好流动性充足的池子，放在 /data/selected/poolLists.csv 文件中，通过里面的池子来组建 3 hops 和 4 hops 的套利路径
3. 设计思路为：
    3.1 内置一个 graph 的模块，使用 petgraph 构建无向图，节点是代币，边是池子
    3.2 引入一个 monitor 模块，每 2s 对池状态数据（使用 getReserves() 调用）的并发访问，如果存在 reserve 的变化，需要并行计算所有与该路径相关的套利路径是否存在套利空间
    3.3 需要注意的是 3 hops 和 4 hops 所消耗的 gas 是不一样的，大约是 700m/720m gas，gasprice 我们假定为 0.02 gwei，gas的单位为 MNT，这样方便你进行成本估算(需要注意的是 Mantle 和别的 L2 不同，别的 L2 使用 ETH 作为 gas，而 Mantle 使用 MNT 作为 native gas token)
    3.4 需要将 WMNT 作为套利路径的起点和终点