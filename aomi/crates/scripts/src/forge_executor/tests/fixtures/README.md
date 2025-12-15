# Fixture File Guidelines

## Purpose
These JSON fixtures define operation groups for testing the ForgeExecutor + BAML integration. Each fixture specifies:
1. Natural language operations with **specific function calls and parameters**
2. Contract dependencies with chain_id, address, and name
3. Dependency relationships between groups

## Key Principles

### 1. Operation Format: ACTION + function + interface + [parameters]
Each operation string must follow this structure:
- **ACTION**: What you're doing (Wrap, Swap, Approve, Deploy, Get, Deposit, etc.)
- **Function signature**: The exact function being called with types
- **Interface name**: Which interface the function comes from
- **Parameters**: All parameters in format `[param1: value1, param2: value2, ...]`

✅ **Good:** "Wrap 0.75 ETH to WETH using function wrap() of IWETH interface [value: 0.75 ether]"

❌ **Bad:** "Call IWETH.wrap{value: 0.75 ether}() to wrap ETH"

❌ **Bad:** "wrap 1 ETH into WETH"

### 2. Parameter Format: [key: value, key: value, ...]
Use square brackets with colon-separated key-value pairs:

✅ **Good:** "Swap 0.5 ETH for USDC using function exactInputSingle(ExactInputSingleParams memory params) of ISwapRouter interface [value: 0.5 ether, params: tokenIn=0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2, tokenOut=0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48, fee=3000, recipient=msg.sender, amountIn=0.5 ether, amountOutMinimum=0, sqrtPriceLimitX96=0]"

❌ **Bad:** "swap WETH for USDC on Uniswap"

### 3. Specify Exact Contract Addresses
Operations should reference the actual contract addresses that will be used. This helps BAML understand the context.

### 4. Include All Required Contract Dependencies
The `contracts` array must list:
- All contracts that will be called in the operations
- Contracts whose interfaces/ABIs are needed (even if not directly called)
- Use the correct contract name that matches the interface (e.g., "WETH" not "IWETH" for the token)

### 5. Multi-Step Operations
When an operation requires multiple steps (approve then transfer, balance check then deposit), list each step as a separate operation string within the same group.

## Example Fixtures

### Simple Single-Group (swap-eth-to-usdc.json)
- 1 group with no dependencies
- Direct swap using Uniswap V3 Router
- Includes WETH and USDC token addresses + SwapRouter address

### Medium Complexity (wrap-and-quote-usdc.json)
- 1 group with 3 operations
- Wrap ETH, deploy Quoter, call quoteExactInputSingle
- Includes WETH, USDC, and UniswapV3Factory addresses

### Complex Multi-Group (wrap-and-stake-weth-usdc.json)
- 3 groups with linear dependencies (0 → 1 → 2)
- Group 0: Wrap ETH to WETH
- Group 1: Swap WETH to USDC (depends on 0)
- Group 2: Stake USDC in Aave (depends on 1)
- Each group has specific approve + action patterns

## BAML Integration Flow

The SourceFetcher will:
1. Extract all contracts from the fixture
2. Fetch source code and ABI from database/Etherscan
3. Pass to BAML Phase 1: Extract contract info (functions, storage, events)
4. Pass to BAML Phase 2: Generate Solidity script

**Why specificity matters:**
- BAML Phase 1 needs to know which functions/events to extract
- BAML Phase 2 uses extracted info to generate correct function calls
- Ambiguous operations lead to hallucinated or incorrect code generation

## Testing Against Real Code

When creating/updating fixtures, compare against actual Forge scripts:
- Look at real examples in `/Users/ceciliazhang/Code/uniswap/contracts/src/pkgs/`
- Ensure fixture operations would generate similar code
- Include all necessary approvals, balance checks, and parameter structs

## Fixture Format

```json
{
  "name": "fixture-name",
  "description": "High-level description of the workflow",
  "groups": [
    {
      "description": "Specific group description with function names",
      "operations": [
        "ACTION using function functionName(type param1, type param2) of InterfaceName interface [param1: value1, param2: value2]",
        "ACTION using function anotherFunc(StructName memory params) of AnotherInterface interface [params: field1=val1, field2=val2, field3=val3]"
      ],
      "dependencies": [0, 1],  // Indices of groups this depends on
      "contracts": [
        {
          "chain_id": "1",
          "address": "0xContractAddress",
          "name": "ContractName"
        }
      ]
    }
  ]
}
```

## Common DeFi Patterns

### ERC20 Approve
```
"Approve X tokens for RouterContract using function approve(address spender, uint256 amount) of IERC20 interface [spender: 0x..., amount: X]"
```

### Wrap ETH
```
"Wrap X ETH to WETH using function deposit() of IWETH interface [value: X ether]"
```

### Uniswap V3 Swap
```
"Swap X ETH for TokenB using function exactInputSingle(ExactInputSingleParams memory params) of ISwapRouter interface [value: X ether, params: tokenIn=0x..., tokenOut=0x..., fee=3000, recipient=msg.sender, deadline=block.timestamp, amountIn=X ether, amountOutMinimum=0, sqrtPriceLimitX96=0]"
```

### Aave Supply
```
"Approve X USDC for Aave Pool using function approve(address spender, uint256 amount) of IERC20 interface [spender: 0x..., amount: X]"

"Deposit X USDC into Aave using function supply(address asset, uint256 amount, address onBehalfOf, uint16 referralCode) of IPool interface [asset: 0x..., amount: X, onBehalfOf: msg.sender, referralCode: 0]"
```

### Get Balance
```
"Get token balance using function balanceOf(address account) of IERC20 interface [account: msg.sender] and store the result in variable tokenAmount"
```

### Deploy Contract
```
"Deploy a new ContractName contract using constructor ContractName(address param1, uint256 param2) [param1: 0x..., param2: value]"
```

---

Last Updated: 2025-12-03
