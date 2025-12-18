// ============================================================================
// Forge Executor: Two-Phase Script Generation
// ============================================================================

// ============================================================================
// Phase 1: Extract Relevant Contract Information
// ============================================================================

class ContractInfo {
    description string?     @description("Optional human description of the contract")
    address string          @description("Contract address, e.g., '0x295a70b2de5e3953354a6a8344e616ed314d7251'")
    abi string              @description("Full ABI JSON string")
    source_code string?     @description("Full Solidity source code (if available)")
}

class Function {
    description string?     @description("Description of what this function does")
    signature string        @description("Function signature in Solidity, e.g., 'wrap()' or 'addLiquidity(address,address,uint256,uint256,uint256,uint256,address,uint256)'")
    abi string              @description("JSON snippet for this function from the ABI")
    body string?            @description("Function body from source code (if available)")
    arguments string?       @description("Human-readable description of arguments, if helpful")
}

class Storage {
    description string?     @description("What is stored in this variable")
    declaration string?     @description("Declaration from source code, e.g., 'uint256 public totalSupply;'")
    index int?              @description("Order of declaration in the contract (e.g., this is the 5th variable)")
}

class Event {
    signature string        @description("Event signature in Solidity, e.g., 'Transfer(address indexed from, address indexed to, uint256 value)'")
    abi string              @description("JSON snippet for this event from the ABI")
}

class ExtractedContractInfo {
    description string?     @description("Optional description of the contract")
    address string          @description("Contract address")
    interface_name string   @description("Interface name to use, e.g., 'IWETH', 'IQuoter', 'IERC20'")
    functions Function[]    @description("Functions needed for this operation group")
    storages Storage[]      @description("Storage variables accessed")
    events Event[]          @description("Events emitted or read")
}

function ExtractContractInfo(
    group_operations: string[],
    contracts: ContractInfo[]
) -> ExtractedContractInfo[] {
    client CustomHaiku
    prompt #"
        You are analyzing smart contracts to extract relevant information for executing operations.

        Given these operations:
        {{ group_operations }}

        And these contracts with full ABIs and source code:
        {{ contracts }}

        For EACH contract, extract:
        1. Which functions are needed to perform the operations?
        2. What storage variables might be accessed (if source code available)?
        3. What events might be emitted?

        Create a concise interface name (e.g., IWETH, IQuoter, IERC20).
        Include function bodies if available to understand behavior.
        Only include items that are relevant to the operations.

        {{ ctx.output_format }}
    "#
}

// ============================================================================
// Phase 2: Generate Forge Script
// ============================================================================

class Import {
    interface_name string   @description("Interface name, e.g., 'IERC20'")
    source string           @description("Import source path, e.g., 'forge-std/interfaces/IERC20.sol'")
}

class Interface {
    name string             @description("Interface name, e.g., 'IWETH'")
    solidity_code string    @description("Complete Solidity interface definition, e.g., 'interface IWETH { function wrap() external payable; }'")
}

class CodeLine {
    line string             @description("Solidity code line, e.g., 'IWETH(0xC02...).wrap{value: ethAmount}();'")
    import_spec Import?     @description("If this line needs a forge-std import, specify it here")
    interface Interface?    @description("If this line needs a custom interface definition, specify it here")
}

class ScriptBlock {
    codelines CodeLine[]    @description("List of code lines in execution order")
}

function GenerateScript(
    group_operations: string[],
    extracted_infos: ExtractedContractInfo[]
) -> ScriptBlock {
    client "openai-responses/gpt-5-mini"
    prompt #"
        You are a Solidity code generator for Forge scripts. Generate clean, correct Solidity code from operation descriptions. Make sure you follow Solidity syntax and best practices.
        Keep in mind that the code lines you generate will be placed in a function between vm.startBroadcast() and vm.stopBroadcast().

        Operations to perform:
        {{ group_operations }}

        Using extracted contract information:
        {{ extracted_infos }}

        For each operation, generate Solidity code lines:
        1. Parse the natural language to understand:
           - What function to call
           - What parameters are needed (amounts, addresses, etc.)
           - Whether ETH value is required (for payable functions)

        2. Generate the Solidity code:
           - Use the interface name to cast addresses: `InterfaceName(address).functionName(params)`
           - For deployments: `ContractName varName = new ContractName(params);`
           - For payable calls: `InterfaceName(address).functionName{value: amount}(params)`
           - Use proper Solidity types and formatting

        3. Handle special cases:
           - ETH amounts: use `0.75 ether` or `75 * 10**16` for readability
           - Addresses: use checksummed format or keep as provided
           - Placeholders like {$X}: keep them as variable names (e.g., `uint256 X = ...;`)
           - Variable references: if description says "use result from X", create appropriate variable

        4. For each code line, decide on import vs inline interface:
           - If it's a standard forge-std interface (IERC20, IERC721, etc.): set `import_spec`
           - If it's a custom interface (IWETH, IQuoter, etc.): set `interface` with full definition
           - Only one of `import_spec` or `interface` should be set, not both

        Example output:
        ```json
        {
          "codelines": [
            {
              "line": "uint256 ethAmount = 75 * 10**16;",
              "import_spec": null,
              "interface": null
            },
            {
              "line": "IWETH(0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2).wrap{value: ethAmount}();",
              "import_spec": null,
              "interface": {
                "name": "IWETH",
                "solidity_code": "interface IWETH { function wrap() external payable; }"
              }
            },
            {
              "line": "IERC20 token = IERC20(0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48);",
              "import_spec": {
                "interface_name": "IERC20",
                "source": "forge-std/interfaces/IERC20.sol"
              },
              "interface": null
            }
          ]
        }
        ```

        {{ ctx.output_format }}
    "#
}

// ============================================================================
// Tests
// ============================================================================

test ExtractContractInfoTest {
    functions [ExtractContractInfo]
    args {
        group_operations [
            "wrap 0.75 ETH to WETH by calling wrap() function",
            "quote swap of 0.75 WETH to USDC using Uniswap V3 Quoter with 0.3% fee tier"
        ]
        contracts [
            {
                description: "Wrapped ETH contract"
                address: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
                abi: "[{\"name\":\"wrap\",\"type\":\"function\",\"stateMutability\":\"payable\",\"inputs\":[],\"outputs\":[]}]"
                source_code: "contract WETH { function wrap() external payable { balances[msg.sender] += msg.value; } }"
            },
            {
                description: "Uniswap V3 Quoter for quoting swap amounts"
                address: "0xb27308f9F90D607463bb33eA1BeBb41C27CE5AB6"
                abi: "[{\"type\":\"function\",\"name\":\"quoteExactInputSingle\",\"inputs\":[{\"name\":\"params\",\"type\":\"tuple\",\"internalType\":\"struct IQuoter.QuoteExactInputSingleParams\",\"components\":[{\"name\":\"tokenIn\",\"type\":\"address\",\"internalType\":\"address\"},{\"name\":\"tokenOut\",\"type\":\"address\",\"internalType\":\"address\"},{\"name\":\"amountIn\",\"type\":\"uint256\",\"internalType\":\"uint256\"},{\"name\":\"fee\",\"type\":\"uint24\",\"internalType\":\"uint24\"},{\"name\":\"sqrtPriceLimitX96\",\"type\":\"uint160\",\"internalType\":\"uint160\"}]}],\"outputs\":[{\"name\":\"amountReceived\",\"type\":\"uint256\",\"internalType\":\"uint256\"},{\"name\":\"sqrtPriceX96After\",\"type\":\"uint160\",\"internalType\":\"uint160\"},{\"name\":\"initializedTicksCrossed\",\"type\":\"uint32\",\"internalType\":\"uint32\"},{\"name\":\"gasEstimate\",\"type\":\"uint256\",\"internalType\":\"uint256\"}],\"stateMutability\":\"view\"}]"
                source_code: "contract Quoter is IQuoter { address public immutable factory; constructor(address _factory) { factory = _factory; } function quoteExactInputSingle(QuoteExactInputSingleParams memory params) public view override returns (uint256 amountReceived, uint160 sqrtPriceX96After, uint32 initializedTicksCrossed, uint256 gasEstimate) { /* implementation */ } }"
            }
        ]
    }
}

test GenerateScriptTest {
    functions [GenerateScript]
    args {
        group_operations [
            "wrap 0.75 ETH to WETH by calling wrap() function",
            "quote swap of 0.75 WETH to USDC using Uniswap V3 Quoter with 0.3% fee (3000)"
        ]
        extracted_infos [
            {
                address: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
                interface_name: "IWETH"
                functions: [
                    {
                        signature: "wrap()"
                        abi: "{\"name\":\"wrap\",\"type\":\"function\",\"stateMutability\":\"payable\",\"inputs\":[],\"outputs\":[]}"
                    }
                ]
                storages: []
                events: []
            },
            {
                address: "0xb27308f9F90D607463bb33eA1BeBb41C27CE5AB6"
                interface_name: "IQuoter"
                functions: [
                    {
                        signature: "quoteExactInputSingle(QuoteExactInputSingleParams)"
                        abi: "{\"type\":\"function\",\"name\":\"quoteExactInputSingle\",\"inputs\":[{\"name\":\"params\",\"type\":\"tuple\",\"internalType\":\"struct IQuoter.QuoteExactInputSingleParams\",\"components\":[{\"name\":\"tokenIn\",\"type\":\"address\"},{\"name\":\"tokenOut\",\"type\":\"address\"},{\"name\":\"amountIn\",\"type\":\"uint256\"},{\"name\":\"fee\",\"type\":\"uint24\"},{\"name\":\"sqrtPriceLimitX96\",\"type\":\"uint160\"}]}],\"outputs\":[{\"name\":\"amountReceived\",\"type\":\"uint256\"},{\"name\":\"sqrtPriceX96After\",\"type\":\"uint160\"},{\"name\":\"initializedTicksCrossed\",\"type\":\"uint32\"},{\"name\":\"gasEstimate\",\"type\":\"uint256\"}],\"stateMutability\":\"view\"}"
                    }
                ]
                storages: []
                events: []
            }
        ]
    }
}
