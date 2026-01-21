// AccessControl handler configuration matching Rust AccessControl
class AccessControlConfig {
  event_signature string @description("Event signature for role changes, usually 'RoleGranted(...)' or 'RoleRevoked(...)'")
  role_names map<string, string>? @description("Mapping of human-readable role names to their hashes (e.g., {'ADMIN_ROLE': 'keccak256(\"ADMIN_ROLE\")'})")
  pick_role_members string? @description("Specific role to track when user intent narrows the scope (e.g., 'L2_TX_SENDER_ROLE')")
}


// Event action configuration for discovered.json
class EventAction {
  field_name string @description("Identifier for the tracked value (e.g., 'validatorsByChain', 'proxyAdmin')")
  action_description string @description("Human-readable explanation of what the handler collects")
  handler EventHandlerConfig | AccessControlConfig @description("Handler parameters describing how to stream the data from logs")
}

// Complete event analysis output
class EventAnalyzeResult {
  event_actions EventAction[] @description("Generated retrieval operations describing how to read dynamic state from events")
  detected_constants map<string, string>? @description("Important constants (role hashes, chain IDs, addresses) used in filters")
  proxy_pattern string? @description("If this contract manages an upgradeable proxy, summarize the pattern")
  summary string @description("Overview of tracked event actions and how they satisfy the intent")
  warnings string[]? @description("Open questions or gaps (missing events, ambiguous semantics, etc.)")
}

function AnalyzeEvent(contract_info: ContractInfo, abi_result: ABIAnalysisResult, intent: string?) -> EventAnalyzeResult {
  client CustomOpus4

  prompt #"
    You are analyzing Ethereum smart contract source code to design event-driven handlers that track dynamic state over time.

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

    # ABI Analysis Results (pre-parsed callable functions and events):
    {{ abi_result }}

    ---

    {% if intent %}
    # User Intent:
    {{ intent }}
    {% endif %}

    ---

    ## Responsibilities

    1. **Determine what should be tracked**
       - Use the user intent (when provided) plus contract semantics to decide which values require historical tracking.
       - Prioritize items that change over time: validator sets, configuration parameters, role memberships, proxy admin/implementation, toggles, etc.

    2. **Create EventAction entries**
       - `field_name`: concise machine-friendly identifier (camelCase or snake_case) describing the tracked data.
       - `action_description`: 1–2 sentences that explain what is being retrieved and how it changes.
       - `handler`: choose between
         * **EventHandlerConfig** for general events.
           - `event_signature`: include when a single event both identifies and updates the value (omit when using explicit add/remove operations).
           - `select_field`: parameter name from the ABI event definition representing the tracked value.
           - `return_type`: parameter type from the ABI.
           - `add` / `remove`: use when separate events add or remove items. Provide the full event signature and optional `where` filters.
         * **AccessControlConfig** when RoleGranted/RoleRevoked patterns exist. Populate `role_names` with discovered constants and use `pick_role_members` to focus on the relevant role.

    3. **Leverage ABI metadata**
       - Reuse exact event signatures and parameter names from `abi_result.events`.
       - When filters are required (e.g., specific `chainId`), express them as `['operator', '#field', 'value']`. Literal numbers should be strings.

    4. **detected_constants**
       - Surface constants referenced in filters or useful to downstream consumers (role hashes, chain IDs, sentinel addresses).

    5. **proxy_pattern**
       - If the contract is a proxy or emits upgrade-related events, summarize how admin/implementation changes are emitted.

    6. **summary & warnings**
       - Summarize the generated handlers and their relationship to the intent.
       - Record missing or ambiguous pieces in `warnings` (e.g., "No removal event found for validator set").

    ## Guidance
    - Do not invent events that are not present in the ABI or source.
    - Prefer explicit add/remove semantics when available over boolean flags.
    - If events emit only a boolean toggle (e.g., `OperatorStatusChanged(address,bool)`), treat `true` as add and `false` as remove.
    - When uncertain about semantics, note the assumption in `warnings`.

    ---

    {{ ctx.output_format }}
  "#
}

test analyze_event_validator_manager {
  functions [AnalyzeEvent]
  args {
    contract_info {
      description "Validator membership manager with add/remove events"
      address "0x1111111111111111111111111111111111111111"
      source_code #"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ValidatorManager {
    mapping(uint256 => mapping(address => bool)) public validators;
    event ValidatorAdded(uint256 indexed chainId, address indexed validator);
    event ValidatorRemoved(uint256 indexed chainId, address indexed validator);
}
      "#
    }
    abi_result {
      retrievals [
        {
          name "validators"
          return_type "bool"
          function_signature "validators(uint256,address)"
          requires_parameters true
          parameter_types ["uint256", "address"]
          notes null
        }
      ]
      events [
        {
          event_signature "ValidatorAdded(uint256 indexed chainId, address indexed validator)"
          description "Emitted when a validator becomes active for a specific chain"
          query_fields ["(chainId, uint256)", "(validator, address)"]
        },
        {
          event_signature "ValidatorRemoved(uint256 indexed chainId, address indexed validator)"
          description "Emitted when a validator is removed from a chain"
          query_fields ["(chainId, uint256)", "(validator, address)"]
        }
      ]
      summary "1 callable function and 2 membership events"
    }
    intent "Track validators for chainId 325 using ValidatorAdded/ValidatorRemoved events"
  }
}

test analyze_event_access_control {
  functions [AnalyzeEvent]
  args {
    contract_info {
      description "Access Control example with role constants"
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

    function addWhitelistedSender(address sender) external onlyRole(ADMIN_ROLE) {
        whitelistedSenders[sender] = true;
        _grantRole(L2_TX_SENDER_ROLE, sender);
    }
}
      "#
    }
    abi_result {
      retrievals [
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
        }
      ]
      events [
        {
          event_signature "RoleGranted(bytes32 indexed role, address indexed account, address indexed sender)"
          description "Standard AccessControl event fired when a role is granted to an account"
          query_fields ["(role, bytes32)", "(account, address)", "(sender, address)"]
        },
        {
          event_signature "RoleRevoked(bytes32 indexed role, address indexed account, address indexed sender)"
          description "AccessControl event fired when a role is revoked from an account"
          query_fields ["(role, bytes32)", "(account, address)", "(sender, address)"]
        }
      ]
      summary "3 callable functions and 2 access control events"
    }
    intent "List addresses that ever received the L2_TX_SENDER_ROLE"
  }
}

test analyze_event_proxy_tracking {
  functions [AnalyzeEvent]
  args {
    contract_info {
      description "Transparent proxy emitting admin and implementation changes"
      address "0x3333333333333333333333333333333333333333"
      source_code #"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "@openzeppelin/contracts/proxy/transparent/TransparentUpgradeableProxy.sol";

contract CustomProxy is TransparentUpgradeableProxy {
    event AdminChanged(address previousAdmin, address newAdmin);
    event Upgraded(address indexed implementation);
    event ProxyInitialized(address implementation, address admin);
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
          description "Transparent proxy event noting a change in the proxy admin"
          query_fields ["(previousAdmin, address)", "(newAdmin, address)"]
        },
        {
          event_signature "Upgraded(address indexed implementation)"
          description "Transparent proxy event emitted when the implementation logic changes"
          query_fields ["(implementation, address)"]
        },
        {
          event_signature "ProxyInitialized(address implementation, address admin)"
          description "Initialization event capturing the first implementation and admin"
          query_fields ["(implementation, address)", "(admin, address)"]
        }
      ]
      summary "2 getter functions and 3 proxy events"
    }
    intent null
  }
}
