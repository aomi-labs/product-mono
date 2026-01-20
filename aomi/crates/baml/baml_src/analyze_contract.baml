// Call handler config
class CallHandlerConfig {
  type string @description("Must be 'call'")
  method string @description("Function signature (e.g., 'owner()', 'balanceOf(address)')")
  args string[]? @description("Arguments if needed")
  ignoreRelative bool? @description("Don't follow this as a relative address")
  expectRevert bool? @description("Expect this call to revert")
  address string? @description("Call a different address")
}

// Storage handler config
class StorageHandlerConfig {
  type string @description("Must be 'storage'")
  slot string @description("Storage slot in hex format (e.g., '0x0', '0xa')")
  offset int? @description("Byte offset within slot for packed variables (0-31)")
  returnType string? @description("Solidity type to decode as (e.g., 'address', 'uint256')")
  ignoreRelative bool?
}

// AccessControl handler config
class AccessControlHandlerConfig {
  type string @description("Must be 'accessControl'")
  roleNames map<string, string>? @description("Map COMPLETE 66-char role hash (0x + 64 hex) to name. Example: {'0xdf8b4c520ffe197c5343c6f5aec59570151ef9a492f2c624fd45ddde6135ec42': 'ADMIN'}")
  pickRoleMembers string? @description("Return only members of specific role (optional)")
  ignoreRelative bool?
}

// DynamicArray handler config
class DynamicArrayHandlerConfig {
  type string @description("Must be 'dynamicArray'")
  slot string @description("Base storage slot for the array")
  returnType string? @description("Type of array elements")
  ignoreRelative bool?
}

// Union of all handler types
type HandlerConfig = CallHandlerConfig | StorageHandlerConfig | EventHandlerConfig | AccessControlHandlerConfig | DynamicArrayHandlerConfig

// A field with its handler configuration
class FieldHandler {
  fieldName string @description("Name for this field in discovered.json (e.g., 'owner', 'accessControl', 'whitelistedProposers')")
  config HandlerConfig @description("The handler configuration")
  description string? @description("Human-readable description of what this handler retrieves")
}

// Complete analysis result
class ContractAnalysis {
  handlers FieldHandler[] @description("All handlers to execute for this contract")
  patternDetected string? @description("Pattern detected (e.g., 'OpenZeppelin AccessControl', 'ERC20 Proxy', 'Custom Whitelist')")
  summary string @description("Brief summary of what these handlers will retrieve")
}

function AnalyzeContractForHandlers(
  contract_info: ContractInfo,
  intent: string
) -> ContractAnalysis {
  client DefaultOpus4

  prompt #"
    You are analyzing an Ethereum smart contract to generate handler configurations.

    # Contract Information

    {% if contract_info.description %}
    Description: {{ contract_info.description }}
    {% endif %}

    {% if contract_info.address %}
    Address: {{ contract_info.address }}
    {% endif %}

    {% if contract_info.abi %}
    ABI:
    {{ contract_info.abi }}
    {% endif %}

    {% if contract_info.source_code %}
    Source Code:
    {{ contract_info.source_code }}
    {% endif %}

    ---

    # User Intent

    {{ intent }}

    ---

    # Your Task

    Analyze the contract and generate handler configurations that will retrieve the data the user is interested in.
    You should return MULTIPLE handlers of DIFFERENT types based on what patterns you detect.

    ---

    # Handler Selection Logic

    ## 1. AccessControl Detection

    **When to use:** Contract uses OpenZeppelin AccessControl pattern

    **Detection criteria:**
    - ABI contains events: `RoleGranted(bytes32,address,address)` and `RoleRevoked(bytes32,address,address)`
    - Source code has role constant definitions

    **Steps:**
    1. Scan source code for role definitions:
       ```solidity
       bytes32 public constant ADMIN_ROLE = 0xdf8b4c52...
       bytes32 public constant ORACLE_ROLE = 0x352d05fe...
       ```
    2. Extract COMPLETE 66-character role hashes (0x + 64 hex digits) - validate each hash is exactly 66 chars and contains only hex characters
    3. Return AccessControlHandlerConfig with roleNames (only include valid 66-char hashes)

    **Example output:**
    - fieldName: "accessControl"
    - handler.type: "accessControl"
    - handler.roleNames: Map role hash 0xdf8b... to "ADMIN_ROLE", 0x352d... to "ORACLE_ROLE"
    - description: "OpenZeppelin AccessControl roles and members"

    ## 2. Event Pattern Detection

    **When to use:** Events track sets/lists with add/remove semantics

    **Detection criteria:**
    - Events with boolean flags indicating add/remove
    - Paired events (one for add, one for remove)
    - Events that modify a set of addresses/values

    **Example pattern:**
    ```solidity
    event ProposerPermissionUpdated(address proposer, bool allowed);
    ```

    **Steps:**
    1. Identify the event that modifies the set
    2. Determine which parameter to extract (the item being added/removed)
    3. Determine the condition for add vs remove
    4. Return EventHandlerConfig

    **Example output:**
    - fieldName: "whitelistedProposers"
    - handler.type: "event"
    - handler.select: "proposer"
    - handler.add.event: "ProposerPermissionUpdated"
    - handler.add.where: ["=", "allowed", true]
    - handler.remove.event: "ProposerPermissionUpdated"
    - handler.remove.where: ["=", "allowed", false]
    - description: "Set of addresses with proposer permissions"

    **Where clause format:**
    - `["=", "fieldName", value]` - Equality check
    - `["!=", "fieldName", value]` - Inequality check
    - Use `#` prefix for event parameters

    ## 3. Call Handler Detection

    **When to use:** View/pure functions retrieve contract state

    **Detection criteria:**
    - Function has `stateMutability: "view"` or `"pure"` in ABI
    - Function matches user intent
    - Function doesn't require any parameters
    - If Proxy pattern detected, SKIP call handlers and use Storage instead

    **Steps:**
    1. Filter ABI for view/pure functions with ZERO parameters (empty inputs array)
    2. Only create handlers for functions that directly match user intent
    3. Verify function exists in provided ABI before creating handler
    4. For proxy contracts, prefer Storage handlers over Call handlers for implementation/admin

    **Example output:**
    - fieldName: "owner"
    - handler.type: "call"
    - handler.method: "owner()"
    - handler.ignoreRelative: false
    - description: "Contract owner address"

    ## 4. Storage Handler Detection

    **When to use:** Direct storage slot access needed (especially for proxies)

    **Detection criteria:**
    - Proxy pattern detected (EIP-1967, EIP-1822)
    - Implementation/admin stored in specific slots
    - No public getter available
    - Use source code to determine exact slots

    **Common proxy slots:**
    - Implementation: `0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc`
    - Admin: `0xb53127684a568b3173ae13b9f8a6016e243e63b6e8ee1178d6a717850b5d6103`

    **Example output:**
    - fieldName: "implementation"
    - handler.type: "storage"
    - handler.slot: "0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc"
    - handler.returnType: "address"
    - description: "EIP-1967 implementation address"

    ## 5. DynamicArray Handler Detection

    **When to use:** Dynamic arrays in storage need enumeration

    **Detection criteria:**
    - Source code shows dynamic array state variable
    - Array needs to be fully enumerated
    - No convenient getter function

    **Example output:**
    - fieldName: "voters"
    - handler.type: "dynamicArray"
    - handler.slot: "0x2"
    - handler.returnType: "address"
    - description: "Array of all voter addresses"

    ---

    # Important Rules

    1. **Prioritize source code analysis** for event patterns and role detection
       - Events signatures are in ABI, but semantics (add vs remove) require source code
       - Role names require source code constant definitions

    2. **Return multiple handlers** when appropriate
       - A contract can have BOTH AccessControl AND Event handlers
       - Don't limit yourself to one handler type

    3. **Match intent carefully**
       - If user asks for "roles", prioritize AccessControl handler
       - If user asks for "whitelisted addresses", look for event-based tracking
       - If user asks for "implementation", look for proxy patterns

    4. **Use appropriate ignoreRelative flags**
       - Set `ignoreRelative: true` for non-contract addresses (like admin EOAs)
       - Set `ignoreRelative: false` for contract addresses that should be explored

    5. **Field naming conventions**
       - Use camelCase for field names
       - Be descriptive but concise
       - Examples: `owner`, `implementation`, `accessControl`, `whitelistedProposers`

    6. **Filter out Call handlers**
       - Filter out Call handlers for Proxy contracts, DO NOT include them in the output
       - If the call requires parameter inputs, DO NOT include them in the output
       - If the call doesn't exist on the contract, DO NOT include in the output

    ---

    # Output Format

    Return a ContractAnalysis object with:
    - **handlers**: Array of FieldHandler objects (fieldName + handler config + description)
    - **patternDetected**: What pattern you recognized (optional but helpful)
    - **summary**: Brief explanation of what these handlers will retrieve

    {{ ctx.output_format }}
  "#
}

test analyze_usdc_proxy {
  functions [AnalyzeContractForHandlers]
  args {
    contract_info {
      description "USDC Proxy Contract"
      address "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
      abi #"[{"constant":false,"inputs":[{"name":"newImplementation","type":"address"}],"name":"upgradeTo","outputs":[],"payable":false,"stateMutability":"nonpayable","type":"function"},{"constant":false,"inputs":[{"name":"newImplementation","type":"address"},{"name":"data","type":"bytes"}],"name":"upgradeToAndCall","outputs":[],"payable":true,"stateMutability":"payable","type":"function"},{"constant":true,"inputs":[],"name":"implementation","outputs":[{"name":"","type":"address"}],"payable":false,"stateMutability":"view","type":"function"},{"constant":false,"inputs":[{"name":"newAdmin","type":"address"}],"name":"changeAdmin","outputs":[],"payable":false,"stateMutability":"nonpayable","type":"function"},{"constant":true,"inputs":[],"name":"admin","outputs":[{"name":"","type":"address"}],"payable":false,"stateMutability":"view","type":"function"},{"inputs":[{"name":"_implementation","type":"address"}],"payable":false,"stateMutability":"nonpayable","type":"constructor"},{"payable":true,"stateMutability":"payable","type":"fallback"},{"anonymous":false,"inputs":[{"indexed":false,"name":"previousAdmin","type":"address"},{"indexed":false,"name":"newAdmin","type":"address"}],"name":"AdminChanged","type":"event"},{"anonymous":false,"inputs":[{"indexed":false,"name":"implementation","type":"address"}],"name":"Upgraded","type":"event"}]"#
      source_code null
    }
    intent "Find the implementation address and admin address"
  }
}

test analyze_access_control_contract {
  functions [AnalyzeContractForHandlers]
  args {
    contract_info {
      description "Contract with OpenZeppelin AccessControl"
      address "0x1234567890123456789012345678901234567890"
      abi #"[{"anonymous":false,"inputs":[{"indexed":true,"name":"role","type":"bytes32"},{"indexed":true,"name":"account","type":"address"},{"indexed":true,"name":"sender","type":"address"}],"name":"RoleGranted","type":"event"},{"anonymous":false,"inputs":[{"indexed":true,"name":"role","type":"bytes32"},{"indexed":true,"name":"account","type":"address"},{"indexed":true,"name":"sender","type":"address"}],"name":"RoleRevoked","type":"event"},{"anonymous":false,"inputs":[{"indexed":true,"name":"role","type":"bytes32"},{"indexed":true,"name":"previousAdminRole","type":"bytes32"},{"indexed":true,"name":"newAdminRole","type":"bytes32"}],"name":"RoleAdminChanged","type":"event"},{"constant":true,"inputs":[{"name":"role","type":"bytes32"},{"name":"account","type":"address"}],"name":"hasRole","outputs":[{"name":"","type":"bool"}],"payable":false,"stateMutability":"view","type":"function"}]"#
      source_code #"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract MyContract {
    bytes32 public constant ADMIN_ROLE = 0xdf8b4c520ffe197c5343c6f5aec59570151ef9a492f2c624fd45ddde6135ec42;
    bytes32 public constant ORACLE_ROLE = 0x352d05fe3946dbe49277552ba941e744d5a96d9c60bc1ba0ea5f1d3ae000f7c8;
    bytes32 public constant UPGRADER_ROLE = 0xa615a8afb6fffcb8c6809ac0997b5c9c12b8cc97651150f14c8f6203168cff4c;

    function grantRole(bytes32 role, address account) public {
        // implementation
    }

    function revokeRole(bytes32 role, address account) public {
        // implementation
    }
}
      "#
    }
    intent "Find all roles and their members"
  }
}

test analyze_event_based_whitelist {
  functions [AnalyzeContractForHandlers]
  args {
    contract_info {
      description "Contract with event-based proposer whitelist"
      address "0xabcdef1234567890abcdef1234567890abcdef12"
      abi #"[{"anonymous":false,"inputs":[{"indexed":true,"name":"proposer","type":"address"},{"indexed":false,"name":"allowed","type":"bool"}],"name":"ProposerPermissionUpdated","type":"event"},{"constant":true,"inputs":[],"name":"owner","outputs":[{"name":"","type":"address"}],"payable":false,"stateMutability":"view","type":"function"}]"#
      source_code #"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ProposerRegistry {
    address public owner;
    mapping(address => bool) private proposers;

    event ProposerPermissionUpdated(address indexed proposer, bool allowed);

    function setProposerPermission(address proposer, bool allowed) public {
        proposers[proposer] = allowed;
        emit ProposerPermissionUpdated(proposer, allowed);
    }
}
      "#
    }
    intent "Track which addresses have proposer permissions"
  }
}
