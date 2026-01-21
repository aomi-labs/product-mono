// Storage slot description returned by the layout analysis
class SlotInfo {
  name string @description("Variable name from source code (prefix parent contract if helpful)")
  type string @description("Solidity type from source code (e.g., 'address', 'mapping(uint256 => address)')")
  base_slot string? @description("Base storage slot index in hex (e.g., '0x0', '0x1'). For packed/shared slots this is the shared slot")
  offset int? @description("Byte offset within the slot for packed variables (0-31). Null for standalone 32-byte slots")
  getter_signature string? @description("Getter signature if the ABI exposes this variable (e.g., 'owner()', 'validators(uint256,address)')")
  notes string? @description("Additional context about purpose, packing, or structure memberships")
}


// Complete storage analysis output
class LayoutAnalysisResult {
  contract_name string @description("Primary contract under analysis, usually the last contract defined")
  solidity_version string? @description("Solidity version from pragma (e.g., '^0.8.0', '0.6.12')")
  inheritance string @description("Linearized inheritance chain (e.g., 'MyToken -> ERC20 -> Context')")
  slots SlotInfo[] @description("Detailed storage slot mapping for all state variables")
  detected_constants map<string, string>? @description("Important constants encountered (role hashes, admin slots, etc.)")
  proxy_pattern string? @description("If contract implements a proxy or delegate pattern, summarize it")
  summary string @description("High-level overview of storage layout with respect to the user intent")
  warnings string[]? @description("Uncertainties or assumptions called out during analysis")
}

function AnalyzeLayout(contract_info: ContractInfo, abi_result: ABIAnalysisResult, intent: string) -> LayoutAnalysisResult {
  client DefaultOpus4

  prompt #"
    You are analyzing Ethereum smart contract source code to map the storage layout that backs on-chain state.

    ---

    {% if not contract_info.source_code %}
      {{ throw('Source code is required but not provided in contract_info') }}
    {% endif %}

    # Contract Information:
    {% if contract_info.description %}
    Description: {{ contract_info.description }}
    {% endif %}
    {% if contract_info.address %}
    Address: {{ contract_info.address }}
    {% endif %}
    Source Code:
    {{ contract_info.source_code }}

    ---

    # ABI Analysis Results (previously identified view functions and events):
    {{ abi_result }}

    ---

    # User Intent:
    {{ intent }}

    ---

    ## Output requirements

    1. **contract_name & solidity_version**
       - Identify the primary contract to analyze (typically the last or most feature-complete contract).
       - Extract the pragma statement and report the Solidity version range.

    2. **Inheritance map**
       - Produce a string describing the linearized inheritance order (e.g., `Child -> ParentA -> ParentB`).
       - Apply Solidity’s linearization rules: expand each parent’s bases before moving right, deduplicate shared ancestors.

    3. **slots (SlotInfo[])**
       - Enumerate every state variable following the inheritance chain (parents first, then child declarations).
       - Populate each `SlotInfo` with:
         * `name`: variable name; prefix with parent contract if needed to disambiguate.
         * `type`: exact Solidity type from source.
         * `base_slot`: sequential slot index in hex (`0x0`, `0x1`, …) representing where the variable (or pointer) is stored.
             - For mappings or dynamic arrays, use the slot that stores the seed/pointer.
             - For structs or fixed-size arrays, use the head slot and describe layout in `notes`.
         * `offset`: byte offset (0–31) only when multiple packed variables share the slot; otherwise null.
         * `getter_signature`: if `abi_result.retrievals` exposes a getter, provide the signature (`balanceOf(address)`, `owner()`).
         * `notes`: concise explanation of purpose, packing, struct members, or special calculation details.
       - Call out intentionally skipped/reserved slots in `notes`.

    4. **detected_constants**
       - Include role hashes, special storage slot constants, fixed addresses, or other meaningful literals.
       - Map human-readable keys to the literal value (e.g., `"ADMIN_ROLE": "keccak256(\"ADMIN_ROLE\")"`).

    5. **proxy_pattern**
       - If the contract is a proxy or manipulates implementation/admin slots, summarize the pattern and its intent.

    6. **summary**
       - Provide 2–3 sentences highlighting variable counts, notable mappings/arrays, and intent-relevant state.

    7. **warnings**
       - List uncertainties or ambiguous deductions (e.g., "struct members truncated in provided source"). Omit when unnecessary.

    ## Guidance
    - Follow Solidity declaration order strictly; never invent slot numbers that conflict with inheritance rules.
    - Emit slot indices in hex. For packed slots, increment offsets in ascending order.
    - Prefer concrete facts from source; when unsure, document the uncertainty in `warnings` instead of guessing.

    ---

    {{ ctx.output_format }}
  "#
}

test analyze_layout_validator_manager {
  functions [AnalyzeLayout]
  args {
    contract_info {
      description "Manages validator membership per chain"
      address "0x1111111111111111111111111111111111111111"
      source_code #"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ValidatorManager {
    address public owner;
    mapping(uint256 => mapping(address => bool)) public validators;
    uint256 public minimumStake;
    bool public emergencyExitEnabled;

    event ValidatorAdded(uint256 indexed chainId, address indexed validator);
    event ValidatorRemoved(uint256 indexed chainId, address indexed validator);
    event EmergencyExitToggled(bool enabled);
}
      "#
    }
    abi_result {
      retrievals [
        {
          name "owner"
          return_type "address"
          function_signature "owner()"
          requires_parameters false
          parameter_types null
          notes "Returns the current owner"
        },
        {
          name "validators"
          return_type "bool"
          function_signature "validators(uint256,address)"
          requires_parameters true
          parameter_types ["uint256", "address"]
          notes "Returns whether validator is active on a chain"
        },
        {
          name "minimumStake"
          return_type "uint256"
          function_signature "minimumStake()"
          requires_parameters false
          parameter_types null
          notes null
        },
        {
          name "emergencyExitEnabled"
          return_type "bool"
          function_signature "emergencyExitEnabled()"
          requires_parameters false
          parameter_types null
          notes null
        }
      ]
      events [
        {
          event_signature "ValidatorAdded(uint256 indexed chainId, address indexed validator)"
          description "Emitted when a validator is activated for a given chain"
          query_fields ["(chainId, uint256)", "(validator, address)"]
        },
        {
          event_signature "ValidatorRemoved(uint256 indexed chainId, address indexed validator)"
          description "Emitted when a validator is deactivated for a given chain"
          query_fields ["(chainId, uint256)", "(validator, address)"]
        },
        {
          event_signature "EmergencyExitToggled(bool enabled)"
          description "Records toggling of the emergency exit mode"
          query_fields ["(enabled, bool)"]
        }
      ]
      summary "4 callable functions and 3 events"
    }
    intent "Document storage used to track validators, stakes, and emergency exits"
  }
}

test analyze_layout_access_control {
  functions [AnalyzeLayout]
  args {
    contract_info {
      description "Access controlled message passer using role constants"
      address "0x2222222222222222222222222222222222222222"
      source_code #"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "@openzeppelin/contracts/access/AccessControl.sol";

contract L2MessagePasser is AccessControl {
    bytes32 public constant DEFAULT_ADMIN_ROLE = 0x00;
    bytes32 public constant L2_TX_SENDER_ROLE = keccak256("L2_TX_SENDER_ROLE");
    bytes32 public constant ADMIN_ROLE = keccak256("ADMIN_ROLE");

    mapping(address => bool) public whitelistedSenders;
    uint256 public messageCount;
    bool public paused;

    constructor() {
        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
        _grantRole(ADMIN_ROLE, msg.sender);
    }
}
      "#
    }
    abi_result {
      retrievals [
        {
          name "DEFAULT_ADMIN_ROLE"
          return_type "bytes32"
          function_signature "DEFAULT_ADMIN_ROLE()"
          requires_parameters false
          parameter_types null
          notes null
        },
        {
          name "L2_TX_SENDER_ROLE"
          return_type "bytes32"
          function_signature "L2_TX_SENDER_ROLE()"
          requires_parameters false
          parameter_types null
          notes null
        },
        {
          name "ADMIN_ROLE"
          return_type "bytes32"
          function_signature "ADMIN_ROLE()"
          requires_parameters false
          parameter_types null
          notes null
        },
        {
          name "whitelistedSenders"
          return_type "bool"
          function_signature "whitelistedSenders(address)"
          requires_parameters true
          parameter_types ["address"]
          notes null
        },
        {
          name "messageCount"
          return_type "uint256"
          function_signature "messageCount()"
          requires_parameters false
          parameter_types null
          notes null
        },
        {
          name "paused"
          return_type "bool"
          function_signature "paused()"
          requires_parameters false
          parameter_types null
          notes null
        }
      ]
      events [
        {
          event_signature "RoleGranted(bytes32 indexed role, address indexed account, address indexed sender)"
          description "AccessControl grant event linking role hashes to accounts"
          query_fields ["(role, bytes32)", "(account, address)", "(sender, address)"]
        },
        {
          event_signature "RoleRevoked(bytes32 indexed role, address indexed account, address indexed sender)"
          description "AccessControl revoke event describing role removals"
          query_fields ["(role, bytes32)", "(account, address)", "(sender, address)"]
        }
      ]
      summary "6 callable functions and 2 access control events"
    }
    intent "Surface storage slots relevant to managing role-protected messaging"
  }
}

test analyze_layout_proxy_contract {
  functions [AnalyzeLayout]
  args {
    contract_info {
      description "TransparentUpgradeableProxy wrapper exposing implementation and admin getters"
      address "0x3333333333333333333333333333333333333333"
      source_code #"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "@openzeppelin/contracts/proxy/transparent/TransparentUpgradeableProxy.sol";

contract CustomProxy is TransparentUpgradeableProxy {
    bytes32 internal constant ADMIN_SLOT = 0xb53127684a568b3173ae13b9f8a6016e243e63b6e8ee1178d6a717850b5d6103;
    bytes32 internal constant IMPLEMENTATION_SLOT = 0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc;

    constructor(address implementation, address admin, bytes memory data)
        TransparentUpgradeableProxy(implementation, admin, data)
    {}
}
      "#
    }
    abi_result {
      retrievals [
        {
          name "getImplementation"
          return_type "address"
          function_signature "getImplementation()"
          requires_parameters false
          parameter_types null
          notes null
        },
        {
          name "getAdmin"
          return_type "address"
          function_signature "getAdmin()"
          requires_parameters false
          parameter_types null
          notes null
        }
      ]
      events [
        {
          event_signature "AdminChanged(address previousAdmin, address newAdmin)"
          description "Proxy event signaling an admin change"
          query_fields ["(previousAdmin, address)", "(newAdmin, address)"]
        },
        {
          event_signature "Upgraded(address indexed implementation)"
          description "Proxy event signaling an implementation upgrade"
          query_fields ["(implementation, address)"]
        },
        {
          event_signature "ProxyInitialized(address implementation, address admin)"
          description "Proxy constructor-style event capturing initial admin and implementation"
          query_fields ["(implementation, address)", "(admin, address)"]
        }
      ]
      summary "2 callable functions and 3 proxy events"
    }
    intent "Identify proxy-specific storage slots and constants for upgrade tracking"
  }
}
