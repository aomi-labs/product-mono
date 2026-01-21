// Types for forge script transaction generation

class Parameter {
  name string @description("Parameter name (e.g., 'to', 'amount')")
  param_type string @description("Solidity type (e.g., 'address', 'uint256')")
  value string @description("The value - can be literal, reference like 'TOKEN_ADDRESS', 'msg.sender', 'block.timestamp', etc.")
}

class Operation {
  contract_address string @description("Contract address (empty string for deployments)")
  contract_name string? @description("Contract name for deployments (e.g., 'SimpleToken')")
  abi string @description("JSON ABI of the contract")
  function_name string @description("Function name to call (or 'constructor' for deployments)")
  parameters Parameter[] @description("Function parameters")
  eth_value string? @description("ETH value in wei for payable functions (e.g., '10000000000000000000' for 10 ETH)")
}

enum InterfaceSource {
  ForgeStd
  Inline
}

class FunctionSignature {
  name string @description("Function name")
  signature string @description("Full function signature with types")
}

class InterfaceDefinition {
  name string @description("Interface name (e.g., 'IERC20', 'IUniswapV2Router02')")
  functions FunctionSignature[] @description("Function signatures in the interface")
  source InterfaceSource? @description("Whether from forge-std or needs inline definition")
  solidity_code string? @description("Full Solidity interface code (required for Inline source)")
}

class TransactionCall {
  solidity_code string @description("The Solidity code for this transaction call")
  description string @description("Comment describing what this call does")
}

class GeneratedScript {
  transaction_calls TransactionCall[] @description("List of transaction calls in order")
  interfaces_needed InterfaceDefinition[] @description("Interfaces that need to be defined/imported")
}

// Main function: Generate Solidity transaction calls from structured operations
function GenerateTransactionCalls(
  operations: Operation[],
  available_interfaces: InterfaceDefinition[],
  deployed_addresses: map<string, string>
) -> GeneratedScript {
  client DefaultOpus4

  prompt #"
    You are a Solidity code generator. Generate transaction calls for a Forge script from structured operations.

    Operations to perform: {{ operations }}
    Available Interfaces: {{ available_interfaces }}
    Deployed Contract References: {{ deployed_addresses }}

    For each operation:
    1. Analyze the ABI to understand the function signature and parameters
    2. Generate clean, correct Solidity code for the call
    3. If it's a deployment (empty contract_address), use: ContractName varName = new ContractName(args);
    4. If it's a contract call, cast to the appropriate interface and call the function
    5. Handle parameter values:
       - If value matches a key in deployed_addresses, use that reference
       - If value is "msg.sender", use msg.sender directly
       - If value is "block.timestamp", use block.timestamp (add +300 for deadlines)
       - Otherwise, treat as a literal value
    6. For payable functions with eth_value, add {value: X} before parameters

    Return structured output with:
    - transaction_calls: Array of calls with solidity_code and description
    - interfaces_needed: Simply return the available_interfaces provided (they are already prepared)

    Example transaction call:
    ```solidity
    IERC20 token = IERC20(0x1234...);
    token.approve(address(router), 1000000 ether);
    ```

    {{ ctx.output_format }}
  "#
}

test GenerateTransactionCallsTest {
  functions [GenerateTransactionCalls]
  args {
    operations [
      {
        contract_address ""
        contract_name "SimpleToken"
        abi "[{\"type\":\"constructor\",\"inputs\":[{\"name\":\"name\",\"type\":\"string\"},{\"name\":\"symbol\",\"type\":\"string\"},{\"name\":\"initialSupply\",\"type\":\"uint256\"}]}]"
        function_name "constructor"
        parameters [
          {
            name "name"
            param_type "string"
            value "AomiCoin"
          },
          {
            name "symbol"
            param_type "string"
            value "AOM"
          },
          {
            name "initialSupply"
            param_type "uint256"
            value "1000000000000000000000000"
          }
        ]
      },
      {
        contract_address "TOKEN_ADDRESS"
        abi "[{\"name\":\"approve\",\"type\":\"function\",\"inputs\":[{\"name\":\"spender\",\"type\":\"address\"},{\"name\":\"amount\",\"type\":\"uint256\"}],\"outputs\":[{\"type\":\"bool\"}]}]"
        function_name "approve"
        parameters [
          {
            name "spender"
            param_type "address"
            value "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"
          },
          {
            name "amount"
            param_type "uint256"
            value "1000000000000000000000000"
          }
        ]
      }
    ]
    available_interfaces [
      {
        name "IERC20"
        functions [
          {
            name "approve"
            signature "approve(address,uint256)"
          }
        ]
        source ForgeStd
      }
    ]
    deployed_addresses {
      TOKEN_ADDRESS: "token"
    }
  }
}
