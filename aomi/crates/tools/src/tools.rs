use crate::abi_encoder::{
    EncodeFunctionCall, EncodeFunctionCallParameters, execute_call as encode_function_call,
};
use crate::account::{
    GetAccountInfo, GetAccountInfoArgs, GetAccountTransactionHistory,
    GetAccountTransactionHistoryArgs, execute_get_account_info,
    execute_get_account_transaction_history,
};
use crate::brave_search::{BraveSearch, BraveSearchParameters, execute_call as brave_search_call};
use crate::cast::{
    CallViewFunction, CallViewFunctionParameters, GetAccountBalance, GetAccountBalanceParameters,
    GetBlockDetails, GetBlockDetailsParameters, GetContractCode, GetContractCodeParameters,
    GetContractCodeSize, GetContractCodeSizeParameters, GetTransactionDetails,
    GetTransactionDetailsParameters, SendTransaction, SendTransactionParameters,
    SimulateContractCall, SimulateContractCallParameters, execute_call_view_function,
    execute_get_account_balance, execute_get_block_details, execute_get_contract_code,
    execute_get_contract_code_size, execute_get_transaction_details, execute_send_transaction,
    execute_simulate_contract_call,
};
use crate::db_tools;
use crate::db_tools::{
    GetContractABI, GetContractArgs, GetContractSourceCode, execute_get_contract_abi,
    execute_get_contract_source_code,
};
use crate::docs::{SearchDocsInput, SharedDocuments, execute_call as docs_search};
use crate::etherscan::{
    FetchContractFromEtherscanParameters, GetContractFromEtherscan,
    execute_fetch_contract_from_etherscan,
};
// use crate::forge_script_builder::{
//     ForgeScriptBuilder, ForgeScriptBuilderParameters, ForgeScriptBuilderResult,
// };
use crate::wallet::{
    SendTransactionToWallet, SendTransactionToWalletParameters, execute_call as wallet_execute_call,
};
use rig::{
    completion::ToolDefinition,
    tool::{Tool, ToolError},
};
use serde_json::json;

impl Tool for EncodeFunctionCall {
    const NAME: &'static str = "encode_function_call";
    type Args = EncodeFunctionCallParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Encodes a function call into hex calldata for any contract function. Takes a function signature like 'transfer(address,uint256)' and an array of argument values."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Short label for what is being encoded, e.g. 'Encode balanceOf for Alice'"
                    },
                    "function_signature": {
                        "type": "string",
                        "description": "The function signature, e.g., 'transfer(address,uint256)' or 'balanceOf(address)'"
                    },
                    "arguments": {
                        "type": "array",
                        "description": "Array of argument values. For simple types pass strings, for array types pass arrays directly, e.g., for swapExactETHForTokens(uint256,address[],address,uint256) pass: [\"0\", [\"0xC02aaA39b223FE8D0A0e5C4F27eAD9083c756Cc2\", \"0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48\"], \"0x7099797051812dc3a010c7d01b50e0d17dc79c8\", \"1716302400\"]",
                        "items": {}
                    }
                },
                "required": ["topic", "function_signature", "arguments"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        encode_function_call(args).await
    }
}

impl Tool for GetAccountInfo {
    const NAME: &'static str = "get_account_info";

    type Error = ToolError;
    type Args = GetAccountInfoArgs;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Fetches account information (balance and nonce) from Etherscan API. The balance is returned in wei (smallest unit). Use this to check an account's current state.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Short note on what this account lookup is for"
                    },
                    "address": {
                        "type": "string",
                        "description": "The Ethereum address to query (e.g., \"0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045\"). Must be a 42-character hex string starting with 0x"
                    },
                    "chain_id": {
                        "type": "number",
                        "description": "The chain ID as an integer (e.g., 1 for Ethereum mainnet, 137 for Polygon, 42161 for Arbitrum)"
                    }
                },
                "required": ["topic", "address", "chain_id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        execute_get_account_info(args).await
    }
}

impl Tool for GetAccountTransactionHistory {
    const NAME: &'static str = "get_account_transaction_history";
    type Error = ToolError;
    type Args = GetAccountTransactionHistoryArgs;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Fetches transaction history for an address with smart database caching. Automatically syncs with Etherscan if the nonce is newer than the cached data. Returns transactions ordered by block number (newest first).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Short note on what this transaction review is for"
                    },
                    "address": {
                        "type": "string",
                        "description": "The Ethereum address to query transactions for"
                    },
                    "chain_id": {
                        "type": "number",
                        "description": "The chain ID as an integer (e.g., 1 for Ethereum mainnet)"
                    },
                    "current_nonce": {
                        "type": "number",
                        "description": "The current nonce of the account (use get_account_info to fetch this)"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of transactions to return (default: 100)"
                    },
                    "offset": {
                        "type": "number",
                        "description": "Number of transactions to skip for pagination (default: 0)"
                    }
                },
                "required": ["topic", "address", "chain_id", "current_nonce"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        execute_get_account_transaction_history(args).await
    }
}

impl Tool for GetContractABI {
    const NAME: &'static str = "get_contract_abi";

    type Error = rig::tool::ToolError;
    type Args = GetContractArgs;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "IMPORTANT: If you know the contract address, ALWAYS provide both address and chain_id for direct lookup. Only use symbol/name/protocol searches when you don't have the contract address. Symbol data may not be populated for all contracts. Retrieves smart contract source code from the database. Search priority (use in this order): 1. address + chain_id (exact match, fastest), 2. symbol, 3. contract_type + protocol + version (combined filters), 4. name (fuzzy search, slowest). Prefer exact address when available.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Short note on what this contract info is for"
                    },
                    "chain_id": {
                        "type": "number",
                        "description": "Optional chain ID filter (e.g., 1 for Ethereum, 137 for Polygon, 42161 for Arbitrum). Required if providing an address"
                    },
                    "address": {
                        "type": "string",
                        "description": "The contract address on chain (e.g., \"0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48\"). REQUIRED if known - always use this instead of symbol/name searches when available. If provided with chain_id, will fetch this specific contract"
                    },
                    "symbol": {
                        "type": "string",
                        "description": "Don't use this if there is a known address. Token symbol (e.g., \"USDC\", \"DAI\"). Note: symbol data is not always available"
                    },
                    "name": {
                        "type": "string",
                        "description": "Contract name for fuzzy search (e.g., \"Uniswap\", \"Aave Pool\"). Case-insensitive partial matching"
                    },
                    "protocol": {
                        "type": "string",
                        "description": "Protocol name for fuzzy search (e.g., \"Uniswap\", \"Aave\", \"Compound\"). Case-insensitive partial matching"
                    },
                    "contract_type": {
                        "type": "string",
                        "description": "Contract type for exact match (e.g., \"ERC20\", \"UniswapV2Router\", \"LendingPool\")"
                    },
                    "version": {
                        "type": "string",
                        "description": "Protocol version for exact match (e.g., \"v2\", \"v3\")"
                    }
                },
                "required": ["topic"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        db_tools::run_sync(execute_get_contract_abi(args))
    }
}

impl Tool for GetContractSourceCode {
    const NAME: &'static str = "get_contract_source_code";

    type Error = rig::tool::ToolError;
    type Args = GetContractArgs;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "IMPORTANT: If you know the contract address, ALWAYS provide both address and chain_id for direct lookup. Only use symbol/name/protocol searches when you don't have the contract address. Symbol data may not be populated for all contracts. Retrieves smart contract source code from the database. Search priority (use in this order): 1. address + chain_id (exact match, fastest), 2. symbol, 3. contract_type + protocol + version (combined filters), 4. name (fuzzy search, slowest). Prefer exact address when available.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Short note on what this contract info is for"
                    },
                    "chain_id": {
                        "type": "number",
                        "description": "Optional chain ID filter (e.g., 1 for Ethereum, 137 for Polygon, 42161 for Arbitrum). Required if providing an address"
                    },
                    "address": {
                        "type": "string",
                        "description": "The contract address on chain (e.g., \"0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48\"). REQUIRED if known - always use this instead of symbol/name searches when available. If provided with chain_id, will fetch this specific contract"
                    },
                    "symbol": {
                        "type": "string",
                        "description": "Don't use this if there is a known address. Token symbol (e.g., \"USDC\", \"DAI\"). Note: symbol data is not always available"
                    },
                    "name": {
                        "type": "string",
                        "description": "Contract name for fuzzy search (e.g., \"Uniswap\", \"Aave Pool\"). Case-insensitive partial matching"
                    },
                    "protocol": {
                        "type": "string",
                        "description": "Protocol name for fuzzy search (e.g., \"Uniswap\", \"Aave\", \"Compound\"). Case-insensitive partial matching"
                    },
                    "contract_type": {
                        "type": "string",
                        "description": "Contract type for exact match (e.g., \"ERC20\", \"UniswapV2Router\", \"LendingPool\")"
                    },
                    "version": {
                        "type": "string",
                        "description": "Protocol version for exact match (e.g., \"v2\", \"v3\")"
                    }
                },
                "required": ["topic"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        db_tools::run_sync(execute_get_contract_source_code(args))
    }
}

impl Tool for GetContractFromEtherscan {
    const NAME: &'static str = "fetch_contract_from_etherscan";

    type Error = ToolError;
    type Args = FetchContractFromEtherscanParameters;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Important: use get_contract_abi with address before trying to use this tool. This will fetch a verified contract directly from the Etherscan v2 API using chain_id and address, store it in the local contract database, and return the ABI plus source code."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Short description of why this contract is being fetched"
                    },
                    "chain_id": {
                        "type": "number",
                        "description": "Numeric EVM chain ID (e.g., 1 for Ethereum mainnet, 137 for Polygon, 8453 for Base)"
                    },
                    "address": {
                        "type": "string",
                        "description": "Target contract address (42-character hex with 0x prefix)"
                    }
                },
                "required": ["topic", "chain_id", "address"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let result = db_tools::run_sync(execute_fetch_contract_from_etherscan(args))?;
        serde_json::to_value(result)
            .map_err(|e| ToolError::ToolCallError(format!("Serialization error: {}", e).into()))
    }
}

impl Tool for SharedDocuments {
    const NAME: &'static str = "search_uniswap_docs";
    type Args = SearchDocsInput;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Search Uniswap V2 and V3 documentation for concepts, contracts, and technical details"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Short note on what this docs search is for"
                    },
                    "query": {
                        "type": "string",
                        "description": "Search query for Uniswap documentation"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Number of results to return (default: 3, max: 10)",
                        "default": 3,
                        "minimum": 1,
                        "maximum": 10
                    }
                },
                "required": ["topic", "query"]
            }),
        }
    }

    async fn call(&self, input: Self::Args) -> Result<Self::Output, Self::Error> {
        docs_search(self, input).await
    }
}

impl Tool for BraveSearch {
    const NAME: &'static str = "brave_search";
    type Args = BraveSearchParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Search the web using the Brave Search API. Returns formatted results with titles, URLs, and descriptions (rate-limited to ~1 req/s).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {"type": "string", "description": "Short label for this search, e.g. 'Check ETH staking news'"},
                    "query": {"type": "string", "description": "Search query string"},
                    "count": {"type": "integer", "description": "Optional number of results to return (default Brave behaviour, max 20)"},
                    "offset": {"type": "integer", "description": "Optional offset for pagination"},
                    "lang": {"type": "string", "description": "Optional language preference (e.g., 'en-US')"},
                    "country": {"type": "string", "description": "Optional country preference (e.g., 'US')"},
                    "safesearch": {"type": "string", "description": "Optional safesearch level: 'off', 'moderate', or 'strict'"},
                    "freshness": {"type": "string", "description": "Optional freshness filter: 'day', 'week', 'month', 'year'"}
                },
                "required": ["topic", "query"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        brave_search_call(args).await
    }
}

impl Tool for SendTransactionToWallet {
    const NAME: &'static str = "send_transaction_to_wallet";
    type Args = SendTransactionToWalletParameters;
    type Output = serde_json::Value;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Send a crafted transaction to the user's wallet for approval and signing. This triggers a wallet popup in the frontend."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Short note on what this transaction does"
                    },
                    "to": {
                        "type": "string",
                        "description": "The recipient address (contract or EOA) - must be a valid Ethereum address"
                    },
                    "value": {
                        "type": "string",
                        "description": "Amount of ETH to send in wei (as string). Use '0' for contract calls with no ETH transfer"
                    },
                    "data": {
                        "type": "string",
                        "description": "The encoded function call data (from encode_function_call tool). Use '0x' for simple ETH transfers"
                    },
                    "gas_limit": {
                        "type": "string",
                        "description": "Optional gas limit for the transaction. If not provided, the wallet will estimate"
                    },
                    "description": {
                        "type": "string",
                        "description": "Human-readable description of what this transaction does, for user approval"
                    }
                },
                "required": ["topic", "to", "value", "data", "description"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        wallet_execute_call(args).await
    }
}

impl Tool for GetAccountBalance {
    const NAME: &'static str = "get_account_balance";
    type Args = GetAccountBalanceParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description:
                "Get the balance of an account (address or ENS) in wei on the specified network."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {"type": "string", "description": "Short label for what this balance check is for"},
                    "address": {"type": "string", "description": "Account address or ENS name to query"},
                    "block": {"type": "string", "description": "Optional block number/hash tag (e.g., 'latest', '12345', or block hash)"},
                    "network": {"type": "string", "description": "Optional network name (defaults to 'testnet')"}
                },
                "required": ["topic", "address"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        execute_get_account_balance(args).await
    }
}

impl Tool for CallViewFunction {
    const NAME: &'static str = "call_view_function";
    type Args = CallViewFunctionParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Call a view function against a contract with optional calldata."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {"type": "string", "description": "Short label for this read-only call"},
                    "from": {"type": "string", "description": "Sender address or ENS name"},
                    "to": {"type": "string", "description": "Target contract or account address/ENS"},
                    "value": {"type": "string", "description": "Amount of ETH to send in wei (as decimal string)"},
                    "input": {"type": "string", "description": "Optional calldata (0x-prefixed hex)"},
                    "network": {"type": "string", "description": "Optional network name (defaults to 'testnet')"}
                },
                "required": ["topic", "from", "to", "value"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        execute_call_view_function(args).await
    }
}

impl Tool for SimulateContractCall {
    const NAME: &'static str = "simulate_contract_call";
    type Args = SimulateContractCallParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description:
                "Simulate a non-view function call against a contract with optional calldata."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {"type": "string", "description": "Short label for this simulation"},
                    "from": {"type": "string", "description": "Sender address or ENS name"},
                    "to": {"type": "string", "description": "Target contract or account address/ENS"},
                    "value": {"type": "string", "description": "Amount of ETH to send in wei (as decimal string)"},
                    "input": {"type": "string", "description": "Optional calldata (0x-prefixed hex)"},
                    "network": {"type": "string", "description": "Optional network name (defaults to 'testnet')"}
                },
                "required": ["topic", "from", "to", "value"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        execute_simulate_contract_call(args).await
    }
}

impl Tool for SendTransaction {
    const NAME: &'static str = "send_transaction";
    type Args = SendTransactionParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Broadcast a transaction using the connected RPC (intended for testnets)."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {"type": "string", "description": "Short label for this on-chain send"},
                    "from": {"type": "string", "description": "Sender address or ENS name (must have signing capability on the RPC)"},
                    "to": {"type": "string", "description": "Recipient address or ENS name"},
                    "value": {"type": "string", "description": "Amount of ETH to send in wei (as decimal string)"},
                    "input": {"type": "string", "description": "Optional calldata (0x-prefixed hex)"},
                    "network": {"type": "string", "description": "Optional network name (defaults to 'testnet')"}
                },
                "required": ["topic", "from", "to", "value"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        execute_send_transaction(args).await
    }
}

impl Tool for GetContractCode {
    const NAME: &'static str = "get_contract_code";
    type Args = GetContractCodeParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Fetch the runtime bytecode for a deployed contract.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {"type": "string", "description": "Short label for this bytecode lookup"},
                    "address": {"type": "string", "description": "Contract address (or ENS name resolving to contract)"},
                    "network": {"type": "string", "description": "Optional network name (defaults to 'testnet')"}
                },
                "required": ["topic", "address"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        execute_get_contract_code(args).await
    }
}

impl Tool for GetContractCodeSize {
    const NAME: &'static str = "get_contract_code_size";
    type Args = GetContractCodeSizeParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Return the runtime bytecode size (bytes) for a contract.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {"type": "string", "description": "Short label for this size check"},
                    "address": {"type": "string", "description": "Contract address or ENS name"},
                    "network": {"type": "string", "description": "Optional network name (defaults to 'testnet')"}
                },
                "required": ["topic", "address"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        execute_get_contract_code_size(args).await
    }
}

impl Tool for GetTransactionDetails {
    const NAME: &'static str = "get_transaction_details";
    type Args = GetTransactionDetailsParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Retrieve transaction (and optional receipt) data by hash.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {"type": "string", "description": "Short label for this transaction lookup"},
                    "tx_hash": {"type": "string", "description": "Transaction hash (0x-prefixed)"},
                    "field": {"type": "string", "description": "Optional specific field to extract from the transaction/receipt JSON"},
                    "network": {"type": "string", "description": "Optional network key defined in CHAIN_NETWORK_URLS_JSON (defaults to 'testnet')"}
                },
                "required": ["topic", "tx_hash"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        execute_get_transaction_details(args).await
    }
}

impl Tool for GetBlockDetails {
    const NAME: &'static str = "get_block_details";
    type Args = GetBlockDetailsParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description:
                "Inspect a block by number/hash or fetch the latest block if not specified."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {"type": "string", "description": "Short label for this block inspection"},
                    "block": {"type": "string", "description": "Optional block identifier ('latest', number, or hash). Defaults to latest."},
                    "field": {"type": "string", "description": "Optional field to pull from the block JSON (e.g., 'timestamp', 'miner')"},
                    "network": {"type": "string", "description": "Optional network name (defaults to 'testnet')"}
                },
                "required": ["topic"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        execute_get_block_details(args).await
    }
}

// MultiStep tool removed (redundant with ForgeScriptBuilder)

// impl Tool for ForgeScriptBuilder {
//     const NAME: &'static str = "build_forge_script";
//     type Args = ForgeScriptBuilderParameters;
//     type Output = ForgeScriptBuilderResult;
//     type Error = ToolError;

//     async fn definition(&self, _prompt: String) -> ToolDefinition {
//         ToolDefinition {
//             name: Self::NAME.to_string(),
//             description: "Build and simulate a Forge script from structured blockchain operations. Generates Solidity code, compiles it, simulates execution, and returns broadcastable transactions.\n\nPreparation workflow:\n1. Fetch contract ABIs using get_contract_abi or get_contract_from_etherscan\n2. Generate interface definitions from ABIs:\n   - For standard interfaces (IERC20, IERC721, etc.), mark source as 'forge-std' (no solidity_code needed)\n   - For custom contracts, mark source as 'inline' and generate the Solidity interface code from the ABI\n   - Include relevant functions the operations will use\n3. Structure operations with contract addresses, ABIs, function names, and parameters\n4. Call this tool with operations and available_interfaces\n\nThe tool will compile and simulate the script, returning broadcastable transactions for user approval.".to_string(),
//             parameters: json!({
//                 "type": "object",
//                 "properties": {
//                     "operations": {
//                         "type": "array",
//                         "description": "List of operations to include in the script. Each operation should specify the contract, function, and parameters.",
//                         "items": {
//                             "type": "object",
//                             "properties": {
//                                 "contract_address": {
//                                     "type": "string",
//                                     "description": "Contract address (empty string for deployments)"
//                                 },
//                                 "contract_name": {
//                                     "type": "string",
//                                     "description": "Contract name for deployments (e.g., 'SimpleToken')"
//                                 },
//                                 "abi": {
//                                     "type": "string",
//                                     "description": "JSON ABI of the contract"
//                                 },
//                                 "function_name": {
//                                     "type": "string",
//                                     "description": "Function name to call (or 'constructor' for deployments)"
//                                 },
//                                 "parameters": {
//                                     "type": "array",
//                                     "description": "Function parameters",
//                                     "items": {
//                                         "type": "object",
//                                         "properties": {
//                                             "name": {
//                                                 "type": "string",
//                                                 "description": "Parameter name"
//                                             },
//                                             "param_type": {
//                                                 "type": "string",
//                                                 "description": "Solidity type (e.g., 'address', 'uint256')"
//                                             },
//                                             "value": {
//                                                 "type": "string",
//                                                 "description": "Parameter value (literal, reference, msg.sender, block.timestamp, etc.)"
//                                             }
//                                         },
//                                         "required": ["name", "param_type", "value"]
//                                     }
//                                 },
//                                 "eth_value": {
//                                     "type": "string",
//                                     "description": "ETH value in wei for payable functions (optional)"
//                                 }
//                             },
//                             "required": ["contract_address", "abi", "function_name", "parameters"]
//                         }
//                     },
//                     "available_interfaces": {
//                         "type": "array",
//                         "description": "Interface definitions generated from contract ABIs. For each contract involved in operations, provide an InterfaceDefinition.",
//                         "items": {
//                             "type": "object",
//                             "properties": {
//                                 "name": {
//                                     "type": "string",
//                                     "description": "Interface name (e.g., 'IERC20', 'IUniswapV2Router02')"
//                                 },
//                                 "source": {
//                                     "type": "string",
//                                     "enum": ["ForgeStd", "Inline"],
//                                     "description": "Source type: 'ForgeStd' for standard interfaces (IERC20, IERC721), 'Inline' for custom contracts"
//                                 },
//                                 "solidity_code": {
//                                     "type": "string",
//                                     "description": "Solidity interface code (required for 'inline' source, omit for 'forge-std')"
//                                 },
//                                 "functions": {
//                                     "type": "array",
//                                     "description": "List of function signatures (for reference)",
//                                     "items": {
//                                         "type": "object",
//                                         "properties": {
//                                             "name": {
//                                                 "type": "string",
//                                                 "description": "Function name"
//                                             },
//                                             "signature": {
//                                                 "type": "string",
//                                                 "description": "Full function signature (e.g., 'transfer(address,uint256)')"
//                                             }
//                                         },
//                                         "required": ["name", "signature"]
//                                     }
//                                 }
//                             },
//                             "required": ["name", "source", "functions"]
//                         }
//                     },
//                     "funding_requirements": {
//                         "type": "array",
//                         "description": "Optional funding instructions applied before the script runs. Use this to preload ETH or ERC20 balances via forge's deal cheat.",
//                         "items": {
//                             "type": "object",
//                             "properties": {
//                                 "asset_type": {
//                                     "type": "string",
//                                     "enum": ["eth", "erc20"],
//                                     "description": "Asset to fund. 'eth' sets the caller's native balance, 'erc20' mints ERC20 tokens."
//                                 },
//                                 "amount": {
//                                     "type": "string",
//                                     "description": "Human-readable amount (e.g., '10', '0.5', '1000.25'). For ERC20 amounts this will be converted using the provided decimals."
//                                 },
//                                 "token_address": {
//                                     "type": "string",
//                                     "description": "ERC20 token address (required when asset_type is 'erc20')"
//                                 },
//                                 "decimals": {
//                                     "type": "integer",
//                                     "description": "ERC20 decimals used to convert the amount into base units (required when asset_type is 'erc20')"
//                                 }
//                             },
//                             "required": ["asset_type", "amount"]
//                         }
//                     }
//                 },
//                 "required": ["operations", "available_interfaces"]
//             }),
//         }
//     }

//     async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
//         // Spawn a dedicated OS thread and wait for it to complete
//         let handle = std::thread::spawn(move || {
//             // Create a new tokio runtime for this thread
//             let rt = tokio::runtime::Runtime::new().map_err(|e| {
//                 ToolError::ToolCallError(format!("Failed to create runtime: {}", e).into())
//             })?;

//             // Block on the async execution
//             rt.block_on(ForgeScriptBuilder::execute(args)).map_err(|e| {
//                 ToolError::ToolCallError(format!("Forge script build failed: {}", e).into())
//             })
//         });

//         // Wait for the thread to finish
//         handle
//             .join()
//             .map_err(|_| ToolError::ToolCallError("Thread panicked".into()))?
//     }
// }
