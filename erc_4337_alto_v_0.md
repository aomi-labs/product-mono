# ERC‑4337 Alto (v0.7) Factory‑Based Account POC

This document outlines a **minimal but complete end‑to‑end POC** to validate:

- ERC‑4337 **EntryPoint v0.7**
- **Alto bundler** running locally
- **Factory‑based smart account creation** (counterfactual)
- **UserOperation submission and execution**

The goal is to prove the full AA pipeline works **without Forge integration first**, so each moving part can be validated independently before committing architectural changes.

---

## 🎯 POC Goal

> Build and submit a single **v0.7 UserOperation** that:
>
> 1. Deploys a smart account via a factory (`factory` + `factoryData`)
> 2. Executes one or more calls via `executeBatch`
> 3. Is accepted by **Alto** and executed through **EntryPoint.handleOps**

---

## 🧱 Scope

### Included
- Anvil mainnet fork
- EntryPoint v0.7 (already deployed on mainnet)
- Factory‑based smart account creation
- Alto bundler (local)
- `eth_sendUserOperation` / `eth_estimateUserOperationGas`

### Explicitly Excluded (for now)
- Forge transaction extraction
- Paymasters
- Session keys / policies
- Multi‑chain support

---

## 🔢 Versions & Constants

### EntryPoint v0.7 (mainnet singleton)
```
0x0000000071727De22E5E9d8BAf0edAc6f37da032
```

This address will exist automatically when forking mainnet at a recent block.

### Chain
- Anvil fork of Ethereum mainnet
- Local chain ID (e.g. `31337`)

---

## 🏗️ High‑Level Architecture

**Processes**
1. Anvil (forked mainnet)
2. Deployed contracts on fork:
   - SmartAccount implementation
   - SmartAccountFactory
   - Test target contract (e.g. `Counter`)
3. Alto bundler (connected to Anvil)
4. Local client (Rust / script) that builds and submits UserOps

---

## Phase 0 — Start Anvil Fork

```bash
anvil \
  --fork-url $MAINNET_RPC \
  --fork-block-number <BLOCK_NUMBER> \
  --chain-id 31337
```

Notes:
- Use a **fixed fork block** for reproducibility
- Ensure Alto and your client use the same chainId

---

## Phase 1 — EntryPoint

No deployment needed.

- EntryPoint v0.7 already exists on mainnet
- Alto will be configured to use:

```
0x0000000071727De22E5E9d8BAf0edAc6f37da032
```

---

## Phase 2 — Deploy Smart Account + Factory

Deploy the following **into the forked state**:

### Smart Account (v0.7 compatible)
Minimum requirements:
- `validateUserOp`
- `execute`
- `executeBatch`
- Owner‑based signature validation

(SimpleAccount‑style implementation is ideal.)

### Factory
- Deterministic deployment (CREATE2 preferred)
- Function like:

```solidity
function createAccount(address owner, uint256 salt) returns (address)
```

The factory should return the account address even if already deployed.

### Test Target Contract
Example:
```solidity
contract Counter {
    uint256 public value;
    function increment() external {
        value++;
    }
}
```

This is used to prove execution works.

---

## Phase 3 — Start Alto Bundler

Run Alto locally and point it at Anvil.

High‑level requirements:
- RPC URL: `http://localhost:8545`
- EntryPoint v0.7 address
- Executor key funded by Anvil

Sanity checks:
- `eth_supportedEntryPoints`
- `eth_chainId`

At this point, Alto should be ready to accept UserOps.

---

## Phase 4 — Build a v0.7 UserOperation

### 4.1 Compute Counterfactual Sender

Using the factory’s CREATE2 logic:
- Compute the **future smart account address**
- This address is used as `sender` even before deployment

This step is factory‑specific.

---

### 4.2 Build `factoryData`

ABI‑encode the factory call, e.g.:

```text
createAccount(owner, salt)
```

Populate UserOp fields:
- `factory` = factory address
- `factoryData` = encoded calldata

---

### 4.3 Build `callData`

ABI‑encode:

```text
executeBatch([
  { to: Counter, value: 0, data: increment() }
])
```

---

### 4.4 Estimate Gas

Call bundler RPC:

```
eth_estimateUserOperationGas
```

Fill in:
- `callGasLimit`
- `verificationGasLimit`
- `preVerificationGas`

---

### 4.5 Sign UserOperation

- Compute UserOp hash (v0.7 rules)
- Sign with owner EOA key
- Populate `signature`

---

### 4.6 Send UserOperation

Call:
```
eth_sendUserOperation(userOp, entryPoint)
```

Poll:
```
eth_getUserOperationReceipt
```

---

## 🧪 Incremental Testing Plan

### Test 1 — Alto ↔ Anvil
- `eth_supportedEntryPoints`
- `eth_chainId`

### Test 2 — Factory‑Only Deployment
- UserOp with `factory` + `factoryData`
- Minimal or no execution

### Test 3 — Execution‑Only
- Manually deploy account
- UserOp without factory fields
- Call `executeBatch`

### Test 4 — Full E2E
- Single UserOp
- Deploy account + execute call(s)

---

## ✅ Definition of Done

POC is successful when:
- Alto accepts `eth_sendUserOperation`
- Anvil contains a tx to `EntryPoint.handleOps`
- `UserOperationEvent` is emitted
- Smart account is deployed
- Target contract state changes as expected

---

## 🔜 Next Steps (Post‑POC)

Once this works:
- Wire Forge transaction extraction → batch calls
- Add paymaster
- Add policy checks
- Add session keys

---

## 🧠 Design Principle

> Keep onchain logic minimal, offchain logic rich.

This POC validates the hardest part: **UserOp correctness + bundler + EntryPoint + factory creation**. Everything else composes cleanly on top.

