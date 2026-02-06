# ERC-4337 Alto POC

This crate implements a proof-of-concept for ERC-4337 Account Abstraction (v0.7) with Alto bundler integration.

## Overview

The POC validates:
- EntryPoint v0.7 (`0x0000000071727De22E5E9d8BAf0edAc6f37da032`)
- Factory-based smart account creation (counterfactual deployment)
- Alto bundler integration
- UserOperation submission and execution
- End-to-end flow: deploy account + execute batch call

## Architecture

```
aa-poc Binary
  ├─ Deploys SimpleAccount, Factory, Counter contracts
  ├─ Verifies Alto bundler connectivity
  ├─ Builds and signs UserOperation
  ├─ Submits to Alto
  └─ Verifies execution (Counter incremented)
```

## Prerequisites

1. **Mainnet RPC URL** (Alchemy or Infura)
2. **Alto bundler** running (via Docker Compose)

## Quick Start

### 1. Set up environment

Create `.env.aa` file in the project root:

```bash
# Required: Mainnet RPC URL for forking
FORK_URL=https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY

# Optional: Override default Anvil test accounts
# These defaults are fine for local testing
EXECUTOR_PRIVATE_KEY=0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
UTILITY_PRIVATE_KEY=0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d
```

### 2. Start Alto bundler

```bash
# From project root
cd /Users/kevin/foameo/product-mono
docker compose -f docker/docker-compose-aa.yml --env-file .env.aa up -d
```

### 3. Run the POC

```bash
# From aomi directory
cd aomi
FORK_URL=https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY \
RUST_LOG=info \
cargo run --bin aa-poc
```

## What the POC Does

### Phase 1: Deploy Contracts
- Deploys `SimpleAccount` implementation
- Deploys `SimpleAccountFactory` (CREATE2-based)
- Deploys `Counter` test contract

### Phase 2: Verify Bundler
- Calls `eth_supportedEntryPoints`
- Verifies EntryPoint v0.7 is supported

### Phase 3: Build and Submit UserOperation
- Computes counterfactual account address
- Builds UserOperation with:
  - `factory` + `factoryData` (for account deployment)
  - `callData` = `executeBatch([Counter.increment()])`
- Estimates gas via `eth_estimateUserOperationGas`
- Signs with owner's private key
- Submits via `eth_sendUserOperation`
- Waits for receipt

### Phase 4: Verify Execution
- Reads `Counter.value` from the deployed Counter contract
- Verifies it equals 1 (was incremented)

## Project Structure

```
aomi/crates/apps/aa/
├── Cargo.toml                    # Crate dependencies
├── foundry.toml                  # Solidity compiler config
├── lib/                          # Forge dependencies
│   ├── account-abstraction/      # ERC-4337 v0.7 contracts
│   └── openzeppelin-contracts/   # OpenZeppelin utilities
├── src/
│   ├── contracts/                # Solidity contracts
│   │   ├── SimpleAccount.sol     # ERC-4337 account
│   │   ├── SimpleAccountFactory.sol
│   │   └── Counter.sol
│   ├── user_operation/           # UserOp builder & signer
│   │   ├── types.rs              # v0.7 PackedUserOperation
│   │   ├── builder.rs            # Construct UserOps
│   │   └── signer.rs             # Sign with ECDSA
│   ├── bundler/                  # Alto RPC client
│   │   ├── client.rs             # eth_* methods
│   │   └── types.rs              # GasEstimate, Receipt
│   ├── runner.rs                 # POC orchestration
│   └── lib.rs                    # Public API
└── README.md                     # This file

aomi/bin/aa-poc/
└── src/
    └── main.rs                   # POC entry point
```

## Configuration

### Environment Variables

- `FORK_URL` (required): Ethereum mainnet RPC URL
- `BUNDLER_RPC_URL` (optional): Alto bundler URL (default: `http://localhost:4337`)
- `EXECUTOR_PRIVATE_KEY` (optional): Alto executor key (default: Anvil test account #0)
- `UTILITY_PRIVATE_KEY` (optional): Alto utility key (default: Anvil test account #1)

### Chain Configuration

- **Chain ID**: 1 (Ethereum mainnet fork)
- **EntryPoint**: `0x0000000071727De22E5E9d8BAf0edAc6f37da032` (v0.7)
- **Owner**: Anvil test account #0 (for POC)

## Troubleshooting

### Alto bundler not starting

Check that `FORK_URL` is set correctly in `.env.aa`:

```bash
docker compose -f docker/docker-compose-aa.yml --env-file .env.aa logs alto
```

### UserOperation validation failed

Common issues:
- **Insufficient balance**: Fund the counterfactual account address
- **Gas estimation failed**: Check that EntryPoint exists at the expected address
- **Signature invalid**: Verify owner address matches the signer

### Contract compilation errors

Ensure Forge dependencies are installed:

```bash
cd aomi/crates/apps/aa
forge install
```

## Testing

Run unit tests:

```bash
cd aomi
cargo test --package aomi-aa
```

## Next Steps

After validating the POC:
1. **Forge Integration**: Extract transactions from Forge scripts → UserOps
2. **Paymaster**: Add gas sponsorship
3. **Session Keys**: Off-chain authorization
4. **Policy Engine**: Pre-execution validation
5. **Multi-chain**: Deploy to Arbitrum, Optimism, Base

## References

- [ERC-4337 Specification](https://eips.ethereum.org/EIPS/eip-4337)
- [Alto Bundler](https://github.com/pimlicolabs/alto)
- [eth-infinitism/account-abstraction](https://github.com/eth-infinitism/account-abstraction)
- [POC Plan Document](../../../erc_4337_alto_v_0.md)
