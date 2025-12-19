# ForgeApp Test Intents

Natural language intents corresponding to test fixtures in `crates/tools/src/forge_executor/tests/fixtures/`

To test, activate ForgeApp with: **"forge-magic"**

---

## 1. swap-eth-to-usdc.json

### Simple Intent (Vague)
```
I want to swap 0.5 ETH for USDC on Uniswap
```

### Detailed Intent
```
I want to swap 0.5 ETH to USDC using Uniswap V3 SwapRouter on Ethereum mainnet
```

### Expected Output
- **1 group**, no dependencies
- Operation: Swap using `exactInputSingle` on SwapRouter
- Contracts: SwapRouter (0xE592...), WETH (0xC02a...), USDC (0xA0b8...)

---

## 2. wrap-and-quote-usdc.json

### Simple Intent
```
Wrap 0.75 ETH to WETH and tell me how much USDC I can get for it on Uniswap
```

### Detailed Intent
```
I want to wrap 0.75 ETH to WETH and then use the Uniswap V3 Quoter to see how much USDC I would get if I swapped it
```

### Expected Output
- **1 group**, no dependencies
- Operations:
  1. Wrap ETH using WETH.wrap()
  2. Deploy Quoter contract
  3. Quote USDC output using Quoter.quoteExactInputSingle
- Contracts: WETH, USDC, UniswapV3Factory

---

## 3. wrap-and-stake-weth-usdc.json

### Simple Intent (Vague)
```
I want to convert 1 ETH to USDC and deposit it into Aave to earn yield
```

### Detailed Intent
```
Wrap 1 ETH to WETH, swap it for USDC on Uniswap, then deposit the USDC into Aave V3 to receive aUSDC
```

### Expected Output
- **3 groups** with dependencies: [[], [0], [1]]
- Group 0: Wrap ETH to WETH
- Group 1: Approve WETH → Swap WETH for USDC on Uniswap (depends on group 0)
- Group 2: Get USDC balance → Approve USDC → Supply to Aave (depends on group 1)
- Contracts: WETH, USDC, SwapRouter02, AaveV3Pool, aUSDC

---

## 4. borrow-and-swap-aave.json

### Simple Intent (Very Vague)
```
I want to use my ETH as collateral on Aave to borrow USDC, then swap the USDC back to ETH
```

### Detailed Intent
```
Supply 2 WETH as collateral to Aave V3, borrow 1000 USDC against it, then swap the borrowed USDC back to ETH via Uniswap V3 and unwrap it
```

### Expected Output
- **4 groups** with dependencies: [[], [0], [1], [2]]
- Group 0: Wrap 2 ETH to WETH
- Group 1: Approve WETH → Supply WETH to Aave (depends on group 0)
- Group 2: Borrow USDC from Aave (depends on group 1)
- Group 3: Get USDC balance → Approve USDC → Swap to WETH → Unwrap to ETH (depends on group 2)
- Contracts: WETH, AaveV3Pool, aWETH, USDC, VariableDebtUSDC, SwapRouter

---

## 5. add-liquidity-uniswap-v3.json

### Simple Intent (Vague)
```
I want to provide liquidity to the USDC/WETH pool on Uniswap V3 using 0.5 ETH
```

### Detailed Intent
```
Starting with 0.5 ETH, wrap it to WETH, swap half to get USDC, then add both WETH and USDC as liquidity to Uniswap V3 to get an LP NFT position
```

### Expected Output
- **3 groups** with dependencies: [[], [0], [1]]
- Group 0: Wrap 0.5 ETH to WETH
- Group 1: Swap 0.25 ETH for USDC using SwapRouter (depends on group 0)
- Group 2: Get balances → Approve both tokens → Mint LP position NFT (depends on group 1)
- Contracts: WETH, USDC, SwapRouter, NonfungiblePositionManager

---

## Testing Strategy

### Phase 1: Simple Intents
Start with the simple/vague versions to test if the agent can:
- Identify the correct contracts and addresses
- Infer missing details (like fee tiers, deadlines)
- Break down into appropriate operation groups
- Identify dependencies correctly

### Phase 2: Edge Cases
Try variations like:
```
"I want to do the Aave collateral thing but with only 1 ETH instead of 2"
"Can you add liquidity but I want to use 1 ETH total, not 0.5?"
"Do the swap but I want USDT instead of USDC"
```

### Phase 3: Error Handling
Try intentionally problematic requests:
```
"Swap 1000000 ETH for USDC" (unrealistic amount)
"Use Aave on Base chain" (protocol may not exist there)
"Provide liquidity to a pool that doesn't exist"
```

---

## Success Criteria

For each intent, the agent should:

1. ✅ Call `set_execution_plan` with correctly structured groups
2. ✅ Use proper operation format: `"Action using function sig() of Interface [params]"`
3. ✅ Identify correct contract addresses on Ethereum mainnet (chain_id: "1")
4. ✅ Set up proper dependency chains (e.g., wrap before swap, approve before transfer)
5. ✅ Call `next_groups` iteratively until `remaining_groups = 0`
6. ✅ Present generated Solidity code and transaction descriptions clearly
7. ✅ Handle errors gracefully if a group fails

---

## Example Conversation Flow

**User:** "forge-magic"
_[Backend switches to ForgeApp]_

**User:** "I want to swap 0.5 ETH for USDC on Uniswap"

**Expected Agent Response:**
1. Acknowledges the request
2. Calls `set_execution_plan` with 1 group
3. Calls `next_groups` to execute
4. Shows the generated Solidity code
5. Describes the transaction (e.g., "Swap 0.5 ETH for USDC via Uniswap V3 SwapRouter")
6. Notes that the transaction is ready for user to broadcast

---

## Quick Copy-Paste Test Messages

```
forge-magic
```

Then try these one at a time:

```
I want to swap 0.5 ETH for USDC on Uniswap
```

```
Wrap 0.75 ETH to WETH and tell me how much USDC I can get for it on Uniswap
```

```
I want to convert 1 ETH to USDC and deposit it into Aave to earn yield - Partial pass, last group failed ( compile error tried to use a variable from previous script )
```

```
I want to use my ETH as collateral on Aave to borrow USDC, then swap the USDC back to ETH
```

```
I want to provide liquidity to the USDC/WETH pool on Uniswap V3 using 0.5 ETH
```

# Contracts required for fixtures
  - chain 1 — 0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2 — WETH — [add-liquidity-uniswap-v3.json, borrow-and-swap-aave.json, swap-eth-to-
    usdc.json, wrap-and-quote-usdc.json, wrap-and-stake-weth-usdc.json]
  - chain 1 — 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48 — USDC — [add-liquidity-uniswap-v3.json, borrow-and-swap-aave.json, swap-eth-to-
    usdc.json, wrap-and-quote-usdc.json, wrap-and-stake-weth-usdc.json]
  - chain 1 — 0xE592427A0AEce92De3Edee1F18E0157C05861564 — SwapRouter — [add-liquidity-uniswap-v3.json, borrow-and-swap-aave.json, swap-eth-
    to-usdc.json]
  - chain 1 — 0xC36442b4a4522E871399CD717aBDD847Ab11FE88 — NonfungiblePositionManager — [add-liquidity-uniswap-v3.json]
  - chain 1 — 0x1F98431c8aD98523631AE4a59f267346ea31F984 — UniswapV3Factory — [wrap-and-quote-usdc.json]
  - chain 1 — 0x61fFE014bA17989E743c5F6cB21bF9697530B21e — QuoterV2 — [wrap-and-quote-usdc.json]
  - chain 1 — 0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2 — AaveV3Pool — [borrow-and-swap-aave.json, wrap-and-stake-weth-usdc.json]
  - chain 1 — 0x4d5F47FA6A74757f35C14fD3a6Ef8E3C9BC514E8 — aWETH — [borrow-and-swap-aave.json]
  - chain 1 — 0x72E95b8931767C79bA4EeE721354d6E99a61D004 — VariableDebtUSDC — [borrow-and-swap-aave.json]
  - chain 1 — 0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45 — SwapRouter02 — [wrap-and-stake-weth-usdc.json]
  - chain 1 — 0xBcca60bB61934080951369a648Fb03DF4F96263C — aUSDC — [wrap-and-stake-weth-usdc.json]