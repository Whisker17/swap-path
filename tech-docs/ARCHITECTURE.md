# High-Level 架构设计

系统将采用模块化设计，分为三个核心层：数据层、逻辑层和执行层

## 数据层 (Data Layer)：Market Data Poller

- 职责: 负责从 Mantle 链上高效、并发地获取最新的市场数据。

- 组件:

    - RPC Connector: 连接到 Mantle 链的 RPC 节点。

    - Batch Requester: 使用 multicall 模式，在单次 RPC 请求中批量查询所有目标 MoeLP 池的 getReserves() 方法，以最小化网络延迟。

    - Data Parser: 解析返回的 reserves 数据并更新内部状态。

## 逻辑层 (Logic Layer): Arbitrage Engine

- 职责: 系统的核心，负责维护资产图、识别套利路径并计算潜在利润。

- 组件:

    - Graph Engine: 使用 petgraph 库构建和维护代币-池的无向图。

    - Pathfinder: 在系统初始化时，基于 data/selected.csv 文件预先计算并缓存所有从 WMNT 出发的 3-hop 和 4-hop 循环路径。

    - Profit Calculator: 在每次收到新的 reserves 数据时，并行地对所有预计算的路径进行利润评估。

## 执行层 (Execution Layer): Transaction Executor (未来范围)

职责: （本次设计暂不实现，但需预留接口）负责将计算出的有利可图的套利机会转化为链上交易并发送。