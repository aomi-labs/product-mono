  forge_script_builder.rs Overview

  This file implements a Forge Script Builder that transforms high-level blockchain operations into
  executable Forge scripts with broadcastable transactions.

  ---
  Data Flow

  ┌─────────────────────────────────────────────────────────────────────────────┐
  │                              LLM AGENT                                      │
  │  (Understands user intent: "Deploy a token and add liquidity")              │
  └─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
  ┌─────────────────────────────────────────────────────────────────────────────┐
  │  ForgeScriptBuilderParameters                                               │
  │  ├── operations: Vec<Operation>         ← Structured contract calls         │
  │  ├── available_interfaces: Vec<InterfaceDefinition>  ← ABIs/interfaces      │
  │  └── funding_requirements: Option<Vec<FundingRequirement>>  ← ETH/ERC20     │
  └─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
  ┌─────────────────────────────────────────────────────────────────────────────┐
  │  ForgeScriptBuilder::execute()                                              │
  │  Step 1: Sanitize operations (checksum addresses)                           │
  │  Step 2: Call BAML API ──────────────────────────────────────────┐          │
  └──────────────────────────────────────────────────────────────────│──────────┘
                                                                     │
                                                                     ▼
                                ┌────────────────────────────────────────────────┐
                                │  BAML Server (forge_transactions.baml)         │
                                │  GenerateTransactionCalls()                    │
                                │  - Takes operations, interfaces, deployed_addrs│
                                │  - LLM generates Solidity code for each op     │
                                │  - Returns GeneratedScript                     │
                                └────────────────────────────────────────────────┘
                                                                     │
                                                                     ▼
  ┌─────────────────────────────────────────────────────────────────────────────┐
  │  GeneratedScript (from BAML)                                                │
  │  ├── transaction_calls: Vec<TransactionCall>   ← Solidity code snippets     │
  │  └── interfaces_needed: Vec<InterfaceDefinition>                            │
  └─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
  ┌─────────────────────────────────────────────────────────────────────────────┐
  │  ScriptAssembler::assemble()                                                │
  │  Builds complete Forge script:                                              │
  │  ┌────────────────────────────────────────────────────────┐                 │
  │  │ pragma solidity ^0.8.20;                               │                 │
  │  │ import {Script} from "forge-std/Script.sol";           │                 │
  │  │ import {StdCheats} from "forge-std/StdCheats.sol";     │                 │
  │  │ import {IERC20} from "forge-std/interfaces/IERC20.sol";│                 │
  │  │                                                        │                 │
  │  │ contract forge_script is Script, StdCheats {           │                 │
  │  │     function run() public {                            │                 │
  │  │         deal(msg.sender, 10 ether);  ← funding         │                 │
  │  │         vm.startBroadcast();                           │                 │
  │  │         // Deploy SimpleToken                          │                 │
  │  │         SimpleToken token = new SimpleToken(...);      │                 │
  │  │         // Approve router                              │                 │
  │  │         token.approve(address(router), ...);           │                 │
  │  │         vm.stopBroadcast();                            │                 │
  │  │     }                                                  │                 │
  │  │ }                                                      │                 │
  │  └────────────────────────────────────────────────────────┘                 │
  └─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
  ┌─────────────────────────────────────────────────────────────────────────────┐
  │  ForgeScriptBuilder::execute_script()                                       │
  │  Step 1: Create ContractSession with fork config                            │
  │  Step 2: Compile script via session.compile_source()                        │
  │  Step 3: Deploy script contract to local EVM                                │
  │  Step 4: Call run() function                                                │
  │  Step 5: Extract broadcastable transactions                                 │
  └─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
  ┌─────────────────────────────────────────────────────────────────────────────┐
  │  ForgeScriptBuilderResult                                                   │
  │  ├── success: bool                                                          │
  │  ├── script: String              ← Complete Solidity script                 │
  │  ├── transactions: Vec<TransactionData>  ← Ready to broadcast               │
  │  │   ├── from, to, value, data, rpc_url                                     │
  │  └── error: Option<String>                                                  │
  └─────────────────────────────────────────────────────────────────────────────┘

  ---
  Key Types

  Input Types (from agent)

  | Type                | Purpose                                      |
  |---------------------|----------------------------------------------|
  | Operation           | Single contract interaction (deploy or call) |
  | Parameter           | Function parameter with name, type, value    |
  | InterfaceDefinition | ABI/interface info (ForgeStd or Inline)      |
  | FundingRequirement  | ETH or ERC20 tokens needed for execution     |

  BAML Types (LLM-generated)

  | Type            | Purpose                                              |
  |-----------------|------------------------------------------------------|
  | GeneratedScript | LLM output containing transaction calls + interfaces |
  | TransactionCall | Single Solidity code snippet + description           |

  Output Types

  | Type                     | Purpose                                      |
  |--------------------------|----------------------------------------------|
  | ForgeScriptBuilderResult | Final result with script + transactions      |
  | TransactionData          | Serializable transaction ready for broadcast |

  ---
  How forge_transactions.baml Fits In

  The BAML file defines the LLM-powered code generation step:

  1. Input: Structured Operation[] (contract calls with ABIs)
  2. LLM Task: Generate correct Solidity syntax for each operation
  3. Output: GeneratedScript with TransactionCall[]

  The LLM handles:
  - Parsing ABIs to understand function signatures
  - Generating proper Solidity cast + call syntax
  - Handling special values (msg.sender, block.timestamp, deployed refs)
  - Generating deployment code (new ContractName(...))

  Example transformation:
  Input Operation:
    contract_address: "0x1234..."
    function_name: "approve"
    parameters: [{name: "spender", value: "0x7a25..."}, {name: "amount", value: "1000..."}]

  LLM Output (TransactionCall):
    solidity_code: "IERC20(0x1234...).approve(0x7a25..., 1000...);"
    description: "Approve spender"

  ---
  Summary

  | Component               | Role                                              |
  |-------------------------|---------------------------------------------------|
  | ForgeScriptBuilder      | Orchestrates the pipeline, executes scripts       |
  | ScriptAssembler         | Wraps generated code in executable Forge script   |
  | forge_transactions.baml | LLM generates Solidity from structured operations |
  | ContractSession         | Compiles + simulates script in forked EVM         |