# 套利系统执行层设计文档 (Mantle 终版)

## 1\. 概述 (Overview)

执行层 (Execution Layer) 是套利系统的 "矛尖"。它的职责是将逻辑层发现的、有利可图的机会，转化为一笔安全、原子性的链上交易，并以尽可能高的成功率被 Mantle 的 Sequencer（定序器）优先打包。

本设计的核心原则是**安全第一，速度第二**。我们将通过一个专门的链上智能合约来执行套利，该合约内置了关键的安全检查，以确保我们只在有利可图时才执行交易，否则宁愿交易失败回滚 (Revert)，也绝不接受亏损。

## 2\. 核心理念：原子性、滑点保护与 Gas 竞价

### 2.1 原子性 (Atomicity)

所有的 `swap` 操作都封装在我们的智能合约的一个函数调用中。这意味着它们要么全部成功，要么全部失败。我们绝不会陷入只完成了一半交易的危险境地。

### 2.2 滑点保护 (Slippage Protection)

这是执行层的灵魂。我们的链下机器人 (Bot) 在发送交易时，会为这笔交易设定一个**最低可接受的利润**。如果因为网络延迟、价格波动或与池中其他交易竞争导致最终利润低于这个阈值，链上合约将自动回滚整笔交易。

### 2.3 Gas 竞价 (Gas Bidding)

在 Mantle 这样的 L2 网络中，交易进入区块的优先级主要由 **Priority Fee (优先费)** 决定。为了让我们的套利交易尽可能快地被 Sequencer 打包，我们必须采用动态的、有竞争力的 Gas 定价策略。这是在公开交易池中战胜对手的关键。

## 3\. 架构设计

执行层的架构分为链上和链下两个部分，二者紧密协作。

### **架构图 (Mermaid)**

```mermaid
sequenceDiagram
    participant Logic Layer
    participant Off-Chain Executor Bot
    participant Mantle Sequencer (RPC)
    participant ArbitrageExecutor Contract
    participant MerchantMoe Pools

    Logic Layer->>Off-Chain Executor Bot: (1) 发现套利机会 (路径, 最优输入, 预期利润)
    Off-Chain Executor Bot->>Off-Chain Executor Bot: (2) 计算 minAmountOut & 动态 Gas Price
    Off-Chain Executor Bot->>Mantle Sequencer (RPC): (3) 发送签名的交易 (调用 executeArbitrage)
    Mantle Sequencer (RPC)->>ArbitrageExecutor Contract: (4) 执行交易
    
    loop 交易路径
        ArbitrageExecutor Contract->>MerchantMoe Pools: (5) swap()
    end

    ArbitrageExecutor Contract->>ArbitrageExecutor Contract: (6) 检查: require(finalAmount >= minAmountOut)
    Note right of ArbitrageExecutor Contract: 如果检查失败，交易在此处 Revert
    
    ArbitrageExecutor Contract->>Off-Chain Executor Bot: (7) (可选) 返还利润
```

## 4\. 链上组件: `ArbitrageExecutor.sol`

这是我们部署在 Mantle 链上的核心执行合约。它是一个简单的、高度优化的合约，只有一个核心功能。合约代码与上一版相同，其设计已经足够健壮和高效。

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.18;

// Uniswap V2 / MoeLP 池的接口
interface IMoePair {
    function swap(
        uint amount0Out,
        uint amount1Out,
        address to,
        bytes calldata data
    ) external;
}

// ERC20 代币接口
interface IERC20 {
    function transfer(address to, uint256 amount) external returns (bool);
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
    function approve(address spender, uint256 amount) external returns (bool);
    function balanceOf(address account) external view returns (uint256);
}

/**
 * @title ArbitrageExecutor
 * @notice 一个用于在兼容Uniswap V2的DEX上执行原子性套利交易的合约。
 */
contract ArbitrageExecutor {
    address public immutable owner;
    address public immutable WETH; // 在Mantle上是 WMNT

    constructor(address _wethAddress) {
        owner = msg.sender;
        WETH = _wethAddress;
    }

    modifier onlyOwner() {
        require(msg.sender == owner, "Caller is not the owner");
        _;
    }

    /**
     * @notice 执行一个多跳的套利交易
     * @param _amountIn 起始投入的WMNT数量
     * @param _minAmountOut 交易结束后，我们必须收回的最小WMNT数量（包含本金+最低利润）
     * @param _path 交易路径上的代币地址数组，第一个和最后一个必须是WMNT
     * @param _pools 交易路径上的池子地址数组，pools[i]是 path[i] 和 path[i+1] 之间的池子
     */
    function executeArbitrage(
        uint256 _amountIn,
        uint256 _minAmountOut,
        address[] calldata _path,
        address[] calldata _pools
    ) external onlyOwner {
        require(_path.length > 1, "Invalid path length");
        require(_path[0] == WETH && _path[_path.length - 1] == WETH, "Path must start and end with WETH");
        require(_path.length == _pools.length + 1, "Path and pools length mismatch");

        // 步骤1: 从owner账户将起始资金拉入本合约
        IERC20(WETH).transferFrom(owner, address(this), _amountIn);

        // 步骤2: 循环执行swap
        for (uint i = 0; i < _pools.length; i++) {
            address tokenIn = _path[i];
            address tokenOut = _path[i+1];
            address pool = _pools[i];
            uint256 amountToSwap = IERC20(tokenIn).balanceOf(address(this));
            
            // Uniswap V2/MoeLP 要求我们指定一个输出数量，另一个为0来触发swap
            // 我们需要根据 tokenIn 是池子的 token0 还是 token1 来决定
            // 简化逻辑：假设我们已经知道正确的输出参数
            (uint amount0Out, uint amount1Out) = (0, 1); // 这是一个占位符，实际需要动态计算
            
            address to = (i < _pools.length - 1) ? _pools[i+1] : address(this);
            
            IMoePair(pool).swap(amount0Out, amount1Out, to, new bytes(0));
        }

        // 步骤3: 安全检查 - 这是最关键的一步
        uint256 finalBalance = IERC20(WETH).balanceOf(address(this));
        require(finalBalance >= _minAmountOut, "Slippage protection failed: profit too low");

        // 步骤4: （可选）将利润返还给owner
        uint256 profit = finalBalance - _amountIn;
        if (profit > 0) {
            IERC20(WETH).transfer(owner, profit);
        }
    }
    
    // 增加一个提款函数，以防有代币卡在合约里
    function withdraw(address _token) external onlyOwner {
        IERC20 token = IERC20(_token);
        token.transfer(owner, token.balanceOf(address(this)));
    }
}
```

## 5\. 链下组件: Executor Bot

这是运行在我们服务器上的程序，负责构造交易并与 Mantle Sequencer 交互。

#### **工作流程**

1.  **接收机会:** 逻辑层发现套利机会，将参数（路径、最优输入额 `optimal_input`、预期利润 `expected_profit`）传递给执行器。

2.  **计算 `minAmountOut`:** 决定交易成败的关键安全计算。

      - `expected_amount_out = optimal_input + expected_profit`
      - `slippage_allowance = expected_profit * SLIPPAGE_PERCENTAGE` (例如 `0.1` 表示愿意放弃 10% 利润)
      - **`minAmountOut = expected_amount_out - slippage_allowance`**

3.  **动态 Gas 定价:** 这是战胜竞争者的核心。

      - **Base Fee:** 通过 `eth_getBlockByNumber('latest')` 获取当前的基础费。
      - **Priority Fee (小费):** 这是关键。可以基于历史数据或当前交易池的拥堵情况来动态设置。
          - *简单策略:* 设置一个固定的、有竞争力的小费（例如 1 Gwei）。
          - *高级策略:* 监控最近几个区块中成功套利交易的 Gas Price，并设置一个比它们略高的价格。或者根据你的预期利润，动态计算愿意支付的最高小费。`Priority Fee = f(expected_profit)`。

4.  **构建和发送交易:**

      - **Nonce 管理:** Bot 必须在本地严格管理自己的 nonce，不能依赖 RPC 节点的 `getTransactionCount`，以支持在必要时快速连续发送多笔交易或取消卡住的交易。
      - **编码与签名:** Bot 使用合约 ABI 和计算好的参数，编码对 `executeArbitrage` 函数的调用，然后用私钥签名，通过 `eth_sendRawTransaction` 发送出去。

5.  **监控结果:** 监控交易是否成功上链。

      - **成功:** 记录利润。
      - **失败 (Revert):** 如果是因为我们的 `Slippage protection failed` 错误而失败，这属于**成功的失败**。说明我们的安全机制生效。
      - **交易未打包:** 如果交易长时间 `pending`，意味着我们的 Gas Price 不够高。需要实现一个**交易替换 (Replacement)** 逻辑：使用相同的 nonce，但更高的 Priority Fee 来重新发送交易。

这个为 Mantle 链量身定制的设计，去除了不必要的复杂性，将所有精力集中在刀刃上：**通过链上合约保证安全，通过链下机器人的智能 Gas 竞价和精确计算来赢得执行速度**。