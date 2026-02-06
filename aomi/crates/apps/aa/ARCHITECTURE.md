# ERC-4337 Account Abstraction POC Architecture

## System Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           aa-poc Binary (Rust)                               │
│                                                                               │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                         AAPocRunner                                  │   │
│  │                                                                       │   │
│  │  • Orchestrates 4-phase POC flow                                     │   │
│  │  • Manages contract deployment                                       │   │
│  │  • Builds & signs UserOperations                                     │   │
│  │  • Executes via EntryPoint                                           │   │
│  └───────────────────┬──────────────────────────────────────────────────┘   │
│                      │                                                        │
└──────────────────────┼────────────────────────────────────────────────────────┘
                       │
                       │ Uses
                       │
       ┌───────────────┼───────────────┐
       │               │               │
       │               │               │
       ▼               ▼               ▼
┌─────────────┐ ┌─────────────┐ ┌─────────────────────┐
│  Compiler   │ │   Bundler   │ │   UserOperation     │
│             │ │   Client    │ │   Builder/Signer    │
├─────────────┤ ├─────────────┤ ├─────────────────────┤
│ • Compiles  │ │ • Alto RPC  │ │ • Builds UserOps    │
│   Solidity  │ │ • Gas est.  │ │ • Computes hashes   │
│ • Gets      │ │ • Send ops  │ │ • Signs with EIP-191│
│   bytecode  │ │ • Receipts  │ │ • Packs for v0.7    │
└─────────────┘ └─────────────┘ └─────────────────────┘
       │               │                     │
       │               │                     │
       └───────────────┼─────────────────────┘
                       │
                       │ Interacts with
                       │
                       ▼
        ┌──────────────────────────────────┐
        │      Ethereum Network (Anvil)     │
        │                                   │
        │  ┌────────────────────────────┐  │
        │  │ EntryPoint v0.7            │  │
        │  │ 0x0000...7da032            │  │
        │  │                            │  │
        │  │ • handleOps()              │  │
        │  │ • getUserOpHash()          │  │
        │  │ • Validates signatures     │  │
        │  │ • Executes UserOps         │  │
        │  └─────────┬──────────────────┘  │
        │            │ Calls                │
        │            ▼                      │
        │  ┌────────────────────────────┐  │
        │  │ SimpleAccountFactory       │  │
        │  │                            │  │
        │  │ • createAccount()          │  │
        │  │ • getAddress() (CREATE2)   │  │
        │  └─────────┬──────────────────┘  │
        │            │ Deploys              │
        │            ▼                      │
        │  ┌────────────────────────────┐  │
        │  │ SimpleAccount (Proxy)      │  │
        │  │                            │  │
        │  │ • validateUserOp()         │  │
        │  │ • execute()                │  │
        │  │ • executeBatch()           │  │
        │  └─────────┬──────────────────┘  │
        │            │ Calls                │
        │            ▼                      │
        │  ┌────────────────────────────┐  │
        │  │ Counter (Test Contract)    │  │
        │  │                            │  │
        │  │ • increment()              │  │
        │  │ • getValue()               │  │
        │  └────────────────────────────┘  │
        └───────────────────────────────────┘
```

## Execution Flow (4 Phases)

```
PHASE 1: Deploy Contracts
══════════════════════════

 AAPocRunner
      │
      ├─→ Compile SimpleAccount.sol
      │        │
      │        └─→ Bytecode + constructor args (EntryPoint address)
      │
      ├─→ Deploy to Anvil
      │        │
      │        └─→ SimpleAccount Implementation @ 0x...
      │
      ├─→ Compile SimpleAccountFactory.sol
      │        │
      │        └─→ Bytecode + constructor args (EntryPoint address)
      │
      ├─→ Deploy to Anvil
      │        │
      │        └─→ SimpleAccountFactory @ 0x...
      │
      └─→ Compile & Deploy Counter.sol
               │
               └─→ Counter @ 0x...


PHASE 2: Verify Alto Bundler
═════════════════════════════

 AAPocRunner
      │
      └─→ BundlerClient.supported_entry_points()
               │
               ├─→ HTTP POST to http://localhost:4337
               │   {
               │     "method": "eth_supportedEntryPoints",
               │     "params": []
               │   }
               │
               └─→ Response: ["0x0000000071727De22E5E9d8BAf0edAc6f37da032"]
                            ✓ EntryPoint v0.7 supported


PHASE 3: Build & Execute UserOperation
═══════════════════════════════════════

 Step 1: Build UserOperation
 ────────────────────────────
 UserOperationBuilder
      │
      ├─→ Call factory.getAddress(owner, salt=0)
      │        │
      │        └─→ Counterfactual address: 0xABC...123
      │
      ├─→ Build initCode
      │        │
      │        └─→ [factory_address(20 bytes)][createAccount_calldata]
      │
      ├─→ Build callData (executeBatch)
      │        │
      │        └─→ abi.encode(
      │              targets: [Counter],
      │              values: [0],
      │              data: [increment()]
      │            )
      │
      └─→ Create unsigned UserOperation
               {
                 sender: 0xABC...123,
                 nonce: 0,
                 factory: 0x...,
                 factoryData: 0x...,
                 callData: 0x...,
                 callGasLimit: 100000,      // Initial estimate
                 verificationGasLimit: 300000,
                 preVerificationGas: 50000,
                 maxFeePerGas: 50 gwei,
                 maxPriorityFeePerGas: 1 gwei,
                 signature: ""  // Empty initially
               }


 Step 2: Sign for Gas Estimation
 ────────────────────────────────
 UserOperationSigner
      │
      ├─→ Pack UserOperation (v0.7 format)
      │        │
      │        └─→ PackedUserOperation {
      │              sender,
      │              nonce,
      │              initCode: factory + factoryData,
      │              callData,
      │              accountGasLimits: [verificationGasLimit][callGasLimit],
      │              preVerificationGas,
      │              gasFees: [maxPriorityFee][maxFee],
      │              paymasterAndData: "",
      │              signature: ""
      │            }
      │
      ├─→ Compute hash (ERC-4337 v0.7 spec)
      │        │
      │        ├─→ Hash dynamic fields:
      │        │     hashInitCode = keccak256(initCode)
      │        │     hashCallData = keccak256(callData)
      │        │     hashPaymasterAndData = keccak256("")
      │        │
      │        ├─→ ABI-encode tuple:
      │        │     encoded = abi.encode(
      │        │       sender,
      │        │       nonce,
      │        │       hashInitCode,      ← Hashed!
      │        │       hashCallData,      ← Hashed!
      │        │       accountGasLimits,
      │        │       preVerificationGas,
      │        │       gasFees,
      │        │       hashPaymasterAndData
      │        │     )
      │        │
      │        ├─→ Hash encoded tuple:
      │        │     packedHash = keccak256(encoded)
      │        │
      │        └─→ Final hash:
      │              userOpHash = keccak256(
      │                abi.encode(packedHash, entryPoint, chainId)
      │              )
      │
      └─→ Sign with owner's private key (EIP-191)
               │
               └─→ signature = sign(
                     keccak256("\x19Ethereum Signed Message:\n32" + userOpHash)
                   )


 Step 3: Gas Estimation via Alto
 ────────────────────────────────
 BundlerClient
      │
      └─→ eth_estimateUserOperationGas(userOp, entryPoint)
               │
               ├─→ Alto validates signature ✓
               │   (Proves our signing logic is correct!)
               │
               └─→ Response:
                     {
                       callGasLimit: 30971,
                       verificationGasLimit: 230691,
                       preVerificationGas: 50142
                     }


 Step 4: Re-sign with Final Gas Values
 ──────────────────────────────────────
 UserOperationSigner
      │
      ├─→ Update gas values from estimation
      │
      ├─→ Clear old signature
      │
      └─→ Sign again with final values
               │
               └─→ Final signed UserOperation ready!


 Step 5: Attempt Alto Bundler (Falls back to direct)
 ────────────────────────────────────────────────────
 BundlerClient
      │
      └─→ eth_sendUserOperation(userOp, entryPoint)
               │
               └─→ ✗ FAILS: Alto requires debug_traceCall
                   (Anvil doesn't support JavaScript tracers)

                   Fallback to direct execution...


 Step 6: Direct EntryPoint Execution
 ────────────────────────────────────
 AAPocRunner
      │
      ├─→ Fund account's EntryPoint deposit
      │        │
      │        └─→ Call EntryPoint.depositTo(account)
      │              Send 10 ETH
      │
      └─→ Call EntryPoint.handleOps([userOp], beneficiary)
               │
               ├─→ EntryPoint validates signature ✓
               │        │
               │        └─→ Computes hash (matches our hash!)
               │              Recovers signer from signature
               │              Verifies signer == owner
               │
               ├─→ EntryPoint calls factory.createAccount()
               │        │
               │        └─→ Deploys SimpleAccount proxy @ 0xABC...123
               │
               ├─→ EntryPoint calls account.validateUserOp()
               │        │
               │        └─→ Account re-validates signature ✓
               │
               └─→ EntryPoint calls account.executeBatch()
                        │
                        └─→ Account calls Counter.increment()
                                 │
                                 └─→ Counter.value: 0 → 1 ✓


PHASE 4: Verify Execution
═════════════════════════

 AAPocRunner
      │
      └─→ Call Counter.getValue()
               │
               └─→ Returns: 1 ✓

                   SUCCESS! 🎉
```

## Key Data Structures

### UserOperation (Unpacked)
```rust
struct UserOperation {
    sender: Address,                    // Smart account address
    nonce: U256,                        // Replay protection
    factory: Option<Address>,           // Factory for counterfactual deployment
    factory_data: Option<Bytes>,        // Factory call data
    call_data: Bytes,                   // What to execute
    call_gas_limit: U256,               // Gas for execution
    verification_gas_limit: U256,       // Gas for validation
    pre_verification_gas: U256,         // Gas for bundler overhead
    max_fee_per_gas: U256,             // EIP-1559 max fee
    max_priority_fee_per_gas: U256,    // EIP-1559 priority fee
    paymaster: Option<Address>,         // Gas sponsor (not used in POC)
    paymaster_verification_gas_limit: Option<U256>,
    paymaster_post_op_gas_limit: Option<U256>,
    paymaster_data: Option<Bytes>,
    signature: Bytes,                   // Owner's signature
}
```

### PackedUserOperation (v0.7 Wire Format)
```rust
struct PackedUserOperation {
    sender: Address,
    nonce: U256,
    initCode: Bytes,                    // factory + factoryData
    callData: Bytes,
    accountGasLimits: FixedBytes<32>,   // [verificationGasLimit(16)][callGasLimit(16)]
    preVerificationGas: U256,
    gasFees: FixedBytes<32>,            // [maxPriorityFee(16)][maxFee(16)]
    paymasterAndData: Bytes,            // paymaster + verification + postOp + data
    signature: Bytes,
}
```

## Hash Computation (Critical!)

```
Input: UserOperation (unpacked)

Step 1: Pack to v0.7 format
────────────────────────────
  initCode = factory(20 bytes) + factoryData
  accountGasLimits = verificationGasLimit(16) + callGasLimit(16)
  gasFees = maxPriorityFee(16) + maxFee(16)
  paymasterAndData = paymaster(20) + verificationGas(16) + postOpGas(16) + data

Step 2: Hash dynamic fields (CRITICAL STEP!)
─────────────────────────────────────────────
  hashInitCode = keccak256(initCode)
  hashCallData = keccak256(callData)
  hashPaymasterAndData = keccak256(paymasterAndData)

Step 3: ABI-encode flat tuple with hashes
──────────────────────────────────────────
  encoded = abi.encode(
    sender,
    nonce,
    hashInitCode,          ← Hash, not raw bytes!
    hashCallData,          ← Hash, not raw bytes!
    accountGasLimits,
    preVerificationGas,
    gasFees,
    hashPaymasterAndData   ← Hash, not raw bytes!
  )

Step 4: Hash the encoded tuple
───────────────────────────────
  packedHash = keccak256(encoded)

Step 5: Add EntryPoint and chainId
───────────────────────────────────
  finalEncoded = abi.encode(packedHash, entryPoint, chainId)

Step 6: Final hash
──────────────────
  userOpHash = keccak256(finalEncoded)

Step 7: Sign with EIP-191
─────────────────────────
  messageHash = keccak256("\x19Ethereum Signed Message:\n32" + userOpHash)
  signature = sign(messageHash, privateKey)
```

## Success Metrics

✅ **Contracts deployed**: SimpleAccount, Factory, Counter
✅ **Counterfactual address computed**: Before deployment
✅ **Gas estimation succeeds**: Alto validates signature
✅ **Hash computation matches**: EntryPoint agrees with our hash
✅ **Signature validation passes**: Both in gas estimation and execution
✅ **Account deployed**: Via factory during handleOps
✅ **Transaction executed**: Counter incremented from 0 to 1
✅ **Gas used**: 263,741 gas

## Known Limitations

⚠️ **Alto bundler incompatibility**: Requires Geth's `debug_traceCall` API
   - Anvil doesn't support JavaScript tracers
   - Workaround: Direct EntryPoint.handleOps call (used in POC)
   - Production: Use Geth instead of Anvil, or use bundler without debug requirements

## References

- **EntryPoint v0.7**: `0x0000000071727De22E5E9d8BAf0edAc6f37da032` (deployed on mainnet)
- **Source**: https://github.com/eth-infinitism/account-abstraction/tree/releases/v0.7.0
- **EIP-4337**: https://eips.ethereum.org/EIPS/eip-4337
- **Key discovery**: `UserOperationLib.encode()` hashes dynamic fields before encoding
